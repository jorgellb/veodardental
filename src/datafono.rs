//! Datáfono (T-F6-03): cobros y devoluciones con tarjeta en la clínica.
//!
//! - **Cada operación** ([`CardTx`]) tiene su referencia única, su estado y su
//!   historia. El cobro (o la devolución) se apunta una sola vez, cuando el
//!   datáfono dice que está aprobada.
//! - **Recuperación:** si se corta la conexión al enviarla, no se sabe si se
//!   cobró. Queda como «Desconocida» y se pregunta al datáfono por su
//!   referencia, nunca se reenvía a ciegas. Si el datáfono no la recibió, se
//!   da por no hecha.
//! - **Cancelación:** mientras está en el datáfono. Si ya se cobró, manda lo
//!   que diga el datáfono (y se puede devolver).
//! - **Devolución:** de una operación aprobada, por lo que quede sin devolver.
//! - **Tarjeta:** solo los cuatro últimos dígitos.
//! - **Adaptadores** ([`Terminal`]):
//!   - [`Manual`], para un datáfono que no se conecta con el programa: se cobra
//!     en el datáfono y se apunta el resultado con el número de autorización;
//!   - [`Http`], con el contrato del apartado «Contrato» (`docs/datafono.md`),
//!     probado con un datáfono simulado. La especificación de un proveedor
//!     concreto está pendiente (B-19, T-F6-03X).

use crate::cents::Cents;
use crate::model::*;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardState {
    /// Creada y aún no enviada.
    Pendiente,
    EnDatafono,
    Aprobada,
    Denegada,
    Cancelada,
    /// Se cortó al enviarla o al preguntar: hay que preguntar otra vez.
    Desconocida,
}

impl CardState {
    pub fn label(self) -> &'static str {
        match self {
            CardState::Pendiente => "Sin enviar",
            CardState::EnDatafono => "En el datáfono",
            CardState::Aprobada => "Aprobada",
            CardState::Denegada => "Denegada",
            CardState::Cancelada => "Cancelada",
            CardState::Desconocida => "Sin respuesta: hay que comprobarla",
        }
    }

    pub fn is_final(self) -> bool {
        matches!(self, CardState::Aprobada | CardState::Denegada | CardState::Cancelada)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardKind {
    Venta,
    Devolucion,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CardTx {
    pub id: u32,
    pub patient_id: u32,
    pub kind: CardKind,
    /// Importe en céntimos (siempre positivo; la devolución resta).
    pub amount: i64,
    pub reference: String,
    pub state: CardState,
    /// Adaptador con que se hizo («manual», «http»).
    pub provider: String,
    #[serde(default)]
    pub provider_id: String,
    #[serde(default)]
    pub auth_code: String,
    /// Solo los cuatro últimos dígitos.
    #[serde(default)]
    pub card: String,
    #[serde(default)]
    pub message: String,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
    pub by: String,
    pub center_id: u32,
    /// El cobro (o la devolución) que se apuntó al aprobarse.
    #[serde(default)]
    pub payment_id: Option<u32>,
    /// En una devolución, la operación que se devuelve.
    #[serde(default)]
    pub original: Option<u32>,
    #[serde(default)]
    pub history: Vec<(NaiveDateTime, CardState, String)>,
}

/// Lo que dice el datáfono de una operación.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reply {
    pub state: Option<CardState>,
    pub provider_id: String,
    pub auth_code: String,
    pub card: String,
    pub message: String,
}

/// Un datáfono: enviar la operación, preguntar por ella y cancelarla.
pub trait Terminal {
    fn name(&self) -> &'static str;
    fn send(&self, tx: &CardTx, original: Option<&CardTx>) -> Result<Reply, String>;
    fn query(&self, tx: &CardTx) -> Result<Reply, String>;
    fn cancel(&self, tx: &CardTx) -> Result<Reply, String>;
}

/// Solo los cuatro últimos dígitos (aunque el datáfono mande más).
pub fn mask_card(s: &str) -> String {
    let digits: Vec<char> = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return String::new();
    }
    format!("···· {}", digits[digits.len() - 4..].iter().collect::<String>())
}

/// Lo que queda por devolver de una venta aprobada.
fn refundable(db: &Db, original: &CardTx) -> i64 {
    let refunded: i64 = db.card_txs.iter().filter(|t| t.original == Some(original.id) && t.kind == CardKind::Devolucion && !matches!(t.state, CardState::Denegada | CardState::Cancelada)).map(|t| t.amount).sum();
    original.amount - refunded
}

impl Db {
    /// Operación nueva (sin enviar). Una devolución, de una venta aprobada del
    /// mismo paciente y por lo que quede sin devolver.
    pub fn card_new(&mut self, pid: u32, kind: CardKind, amount: Cents, original: Option<u32>, provider: &str, now: NaiveDateTime) -> Result<u32, String> {
        if self.patient(pid).is_none() {
            return Err("El paciente ya no existe.".into());
        }
        if amount.0 <= 0 {
            return Err("Indique el importe.".into());
        }
        if amount.0 > 10_000_000 {
            return Err("Importe demasiado alto para el datáfono.".into());
        }
        if kind == CardKind::Devolucion {
            let o = original.and_then(|o| self.card_txs.iter().find(|t| t.id == o)).ok_or("Falta la operación que se devuelve.")?;
            if o.kind != CardKind::Venta || o.state != CardState::Aprobada || o.patient_id != pid {
                return Err("Solo se devuelve una venta aprobada de este paciente.".into());
            }
            let left = refundable(self, o);
            if amount.0 > left {
                return Err(format!("De esa operación quedan {} por devolver.", Cents(left)));
            }
        }
        let id = self.new_id();
        let by = self.config.current_user.clone();
        let center = cid(self.config.current_center);
        self.card_txs.push(CardTx {
            id,
            patient_id: pid,
            kind,
            amount: amount.0,
            reference: format!("VD{id}"),
            state: CardState::Pendiente,
            provider: provider.into(),
            provider_id: String::new(),
            auth_code: String::new(),
            card: String::new(),
            message: String::new(),
            created: now,
            updated: now,
            by,
            center_id: center,
            payment_id: None,
            original: if kind == CardKind::Devolucion { original } else { None },
            history: vec![(now, CardState::Pendiente, String::new())],
        });
        Ok(id)
    }

    /// Antes de enviarla: si se corta a mitad, queda «en el datáfono» y se
    /// pregunta por ella (no se reenvía).
    pub fn card_sending(&mut self, id: u32, now: NaiveDateTime) -> Result<CardTx, String> {
        let t = self.card_txs.iter_mut().find(|t| t.id == id).ok_or("La operación ya no existe.")?;
        if t.state != CardState::Pendiente {
            return Err("Esta operación ya se envió: pregunte por ella.".into());
        }
        t.state = CardState::EnDatafono;
        t.updated = now;
        t.history.push((now, CardState::EnDatafono, "enviada".into()));
        Ok(t.clone())
    }

    /// Lo que respondió el datáfono (o el error al hablar con él). Devuelve el
    /// estado en que queda.
    pub fn card_apply(&mut self, id: u32, reply: Result<Reply, String>, now: NaiveDateTime) -> Result<CardState, String> {
        let t = self.card_txs.iter().find(|t| t.id == id).cloned().ok_or("La operación ya no existe.")?;
        let (new, note, r) = match reply {
            // sin respuesta: no se sabe; si ya era definitiva, se queda
            Err(e) if t.state.is_final() => (t.state, format!("sin respuesta al preguntar: {e}"), Reply::default()),
            Err(e) => (CardState::Desconocida, e, Reply::default()),
            Ok(r) => {
                let s = r.state.unwrap_or(t.state);
                let s = match (t.state, s) {
                    // lo definitivo no cambia, salvo que el datáfono diga que sí cobró
                    // una que se dio por cancelada (llegó a pasar la tarjeta)
                    (CardState::Cancelada, CardState::Aprobada) => CardState::Aprobada,
                    (a, b) if a.is_final() && a != b => a,
                    (_, b) => b,
                };
                (s, r.message.clone(), r)
            }
        };
        let approved_now = new == CardState::Aprobada && t.payment_id.is_none();
        let x = self.card_txs.iter_mut().find(|x| x.id == id).ok_or("La operación ya no existe.")?;
        x.state = new;
        x.updated = now;
        if !r.provider_id.is_empty() {
            x.provider_id = r.provider_id.chars().take(64).collect();
        }
        if !r.auth_code.is_empty() {
            x.auth_code = r.auth_code.chars().filter(|c| c.is_ascii_alphanumeric()).take(12).collect();
        }
        if !r.card.is_empty() {
            x.card = mask_card(&r.card);
        }
        x.message = note.chars().take(200).collect();
        x.history.push((now, new, x.message.clone()));
        if approved_now {
            // el cobro, una sola vez, en la caja del centro donde se hizo
            let (center, sign, concept) = match t.kind {
                CardKind::Venta => (t.center_id, 1.0, format!("Datáfono {}", x.auth_code)),
                CardKind::Devolucion => (t.center_id, -1.0, format!("Devolución con datáfono {}", x.auth_code)),
            };
            let was = self.config.current_center;
            self.config.current_center = center;
            let pid = self.add_payment(t.patient_id, sign * Cents(t.amount).eur(), "Tarjeta", &concept);
            self.config.current_center = was;
            if let Some(x) = self.card_txs.iter_mut().find(|x| x.id == id) {
                x.payment_id = Some(pid);
            }
            let who = self.patient_name(t.patient_id);
            self.audit(if t.kind == CardKind::Venta { "Cobro con datáfono" } else { "Devolución con datáfono" }, &format!("{who} · {} · {}", Cents(t.amount), t.reference));
        }
        Ok(new)
    }

    /// Resultado de un datáfono que no se conecta (adaptador manual): con el
    /// número de autorización del justificante, o que no pasó.
    pub fn card_manual_result(&mut self, id: u32, approved: bool, auth_code: &str, now: NaiveDateTime) -> Result<CardState, String> {
        let t = self.card_txs.iter().find(|t| t.id == id).ok_or("La operación ya no existe.")?;
        if t.provider != Manual.name() {
            return Err("Esta operación es de un datáfono conectado: pregunte por ella.".into());
        }
        if t.state.is_final() {
            return Err("Esta operación ya está cerrada.".into());
        }
        if approved && auth_code.trim().is_empty() {
            return Err("Escriba el número de autorización del justificante del datáfono.".into());
        }
        let r = Reply {
            state: Some(if approved { CardState::Aprobada } else { CardState::Denegada }),
            auth_code: auth_code.trim().into(),
            message: if approved { "apuntada a mano".into() } else { "no pasó (apuntado a mano)".into() },
            ..Default::default()
        };
        self.card_apply(id, Ok(r), now)
    }

    /// Las que hay que comprobar: sin respuesta, o en el datáfono hace más
    /// de dos minutos (de un datáfono conectado).
    pub fn card_to_check(&self, now: NaiveDateTime) -> Vec<u32> {
        self.card_txs.iter().filter(|t| t.provider != Manual.name() && (t.state == CardState::Desconocida || (t.state == CardState::EnDatafono && (now - t.updated).num_seconds() >= 120))).map(|t| t.id).collect()
    }
}

// ───────────────────────────── adaptadores ─────────────────────────────

/// Datáfono que no se conecta: se cobra en él y se apunta el resultado.
pub struct Manual;

impl Terminal for Manual {
    fn name(&self) -> &'static str {
        "manual"
    }
    fn send(&self, _: &CardTx, _: Option<&CardTx>) -> Result<Reply, String> {
        Ok(Reply { state: Some(CardState::EnDatafono), message: "Cobre en el datáfono y apunte el resultado.".into(), ..Default::default() })
    }
    fn query(&self, _: &CardTx) -> Result<Reply, String> {
        Ok(Reply::default())
    }
    fn cancel(&self, _: &CardTx) -> Result<Reply, String> {
        Ok(Reply { state: Some(CardState::Cancelada), message: "cancelada".into(), ..Default::default() })
    }
}

/// Datáfono conectado con el contrato de `docs/datafono.md`.
pub struct Http {
    pub base: String,
    pub token: String,
    pub agent: ureq::Agent,
}

impl Http {
    pub fn new(base: &str, token: &str) -> Http {
        let agent = ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(3)).timeout(std::time::Duration::from_secs(20)).build();
        Http { base: base.trim().trim_end_matches('/').to_string(), token: token.trim().to_string(), agent }
    }

    fn parse(text: &str) -> Result<Reply, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("Respuesta no válida del datáfono: {e}"))?;
        let state = match v["estado"].as_str().unwrap_or("") {
            "en_curso" => CardState::EnDatafono,
            "aprobada" => CardState::Aprobada,
            "denegada" => CardState::Denegada,
            "cancelada" => CardState::Cancelada,
            other => return Err(format!("Estado desconocido del datáfono: «{}»", other.chars().take(20).collect::<String>())),
        };
        let s = |k: &str| v[k].as_str().unwrap_or("").chars().take(200).collect::<String>();
        Ok(Reply { state: Some(state), provider_id: s("id"), auth_code: s("autorizacion"), card: s("tarjeta"), message: s("mensaje") })
    }

    fn call(&self, r: ureq::Request, body: Option<serde_json::Value>) -> Result<Reply, String> {
        let r = r.set("Authorization", &format!("Bearer {}", self.token));
        let resp = match body {
            Some(b) => r.set("Content-Type", "application/json").send_string(&b.to_string()),
            None => r.call(),
        };
        match resp {
            Ok(x) => Self::parse(&x.into_string().map_err(|e| e.to_string())?),
            // la operación no consta: el datáfono no la recibió (no se cobró)
            Err(ureq::Error::Status(404, _)) => Ok(Reply { state: Some(CardState::Cancelada), message: "el datáfono no la recibió".into(), ..Default::default() }),
            Err(ureq::Error::Status(c, x)) => Err(format!("El datáfono respondió {c}: {}", x.into_string().unwrap_or_default().chars().take(120).collect::<String>())),
            Err(e) => Err(format!("Sin respuesta del datáfono: {e}")),
        }
    }
}

impl Terminal for Http {
    fn name(&self) -> &'static str {
        "http"
    }
    fn send(&self, tx: &CardTx, original: Option<&CardTx>) -> Result<Reply, String> {
        let body = serde_json::json!({
            "referencia": tx.reference,
            "tipo": if tx.kind == CardKind::Venta { "venta" } else { "devolucion" },
            "importe": tx.amount,
            "moneda": "EUR",
            "original": original.map(|o| o.reference.clone()),
        });
        // la misma referencia es la misma operación: repetirla no cobra dos veces
        self.call(self.agent.post(&format!("{}/operaciones", self.base)).set("Idempotency-Key", &tx.reference), Some(body))
    }
    fn query(&self, tx: &CardTx) -> Result<Reply, String> {
        self.call(self.agent.get(&format!("{}/operaciones/{}", self.base, tx.reference)), None)
    }
    fn cancel(&self, tx: &CardTx) -> Result<Reply, String> {
        self.call(self.agent.post(&format!("{}/operaciones/{}/cancelar", self.base, tx.reference)), Some(serde_json::json!({})))
    }
}

/// Ajustes del datáfono de este puesto (fichero privado, como `tpv.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Vacío: manual.
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub token: String,
}

impl Settings {
    pub fn terminal(&self) -> Box<dyn Terminal + Send> {
        if self.url.trim().is_empty() { Box::new(Manual) } else { Box::new(Http::new(&self.url, &self.token)) }
    }

    pub fn problem(&self) -> Option<&'static str> {
        let u = self.url.trim();
        if u.is_empty() {
            return None;
        }
        if !(u.starts_with("https://") || u.starts_with("http://127.0.0.1") || u.starts_with("http://localhost")) {
            return Some("La dirección del datáfono tiene que ir por https (o ser de este mismo ordenador).");
        }
        if self.token.trim().is_empty() {
            return Some("Falta la clave del datáfono.");
        }
        None
    }
}

fn settings_path() -> std::path::PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("datafono.json")
}

pub fn load_settings() -> Settings {
    std::fs::read_to_string(settings_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_settings(s: &Settings) -> Result<(), String> {
    let p = settings_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    crate::paths::write_private(&p, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())
}

/// Un datáfono simulado con el contrato, para las pruebas (y para ensayar
/// sin datáfono): aprueba a la segunda consulta salvo importes acabados en
/// 13 céntimos (deniega), y puede «perder» respuestas.
#[cfg(test)]
pub mod fake {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    pub struct Op {
        pub state: String,
        pub queries: u32,
        pub amount: i64,
    }

    #[derive(Default)]
    pub struct State {
        pub ops: HashMap<String, Op>,
        /// Cuántas veces se pasó una tarjeta (cobros de verdad).
        pub charges: u32,
        /// La próxima operación se hace pero no se responde.
        pub drop_next_reply: bool,
        /// La próxima operación no llega (503 sin hacerla).
        pub refuse_next: bool,
    }

    /// Arranca el datáfono simulado. Devuelve su dirección y su estado.
    pub fn start(token: &'static str) -> (String, Arc<Mutex<State>>) {
        let srv = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", srv.server_addr().to_ip().unwrap());
        let st = Arc::new(Mutex::new(State::default()));
        let s2 = st.clone();
        std::thread::spawn(move || {
            for mut req in srv.incoming_requests() {
                let auth = req.headers().iter().any(|h| h.field.equiv("Authorization") && h.value.as_str() == format!("Bearer {token}"));
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let url = req.url().to_string();
                let mut s = s2.lock().unwrap();
                let json = |op: &Op, r: &str| format!(r#"{{"referencia":"{r}","estado":"{}","id":"T-{r}","autorizacion":"{}","tarjeta":"4548 8120 4940 0004","mensaje":"ok"}}"#, op.state, if op.state == "aprobada" { "A1B2C3" } else { "" });
                let (code, out) = if !auth {
                    (401, "{}".to_string())
                } else if url == "/operaciones" {
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                    let r = v["referencia"].as_str().unwrap_or("").to_string();
                    if s.refuse_next {
                        s.refuse_next = false;
                        (503, "{}".into())
                    } else {
                        if !s.ops.contains_key(&r) {
                            let amount = v["importe"].as_i64().unwrap_or(0);
                            s.ops.insert(r.clone(), Op { state: "en_curso".into(), queries: 0, amount });
                        }
                        let out = json(&s.ops[&r], &r);
                        if s.drop_next_reply {
                            // la hace (la tarjeta se pasa) pero la respuesta no llega
                            s.drop_next_reply = false;
                            let op = s.ops.get_mut(&r).unwrap();
                            op.state = "aprobada".into();
                            s.charges += 1;
                            drop(s);
                            drop(req);
                            continue;
                        }
                        (200, out)
                    }
                } else if let Some(r) = url.strip_prefix("/operaciones/").and_then(|r| r.strip_suffix("/cancelar")) {
                    match s.ops.get_mut(r) {
                        Some(op) => {
                            if op.state == "en_curso" {
                                op.state = "cancelada".into();
                            }
                            (200, json(op, r))
                        }
                        None => (404, "{}".into()),
                    }
                } else if let Some(r) = url.strip_prefix("/operaciones/") {
                    let mut charged = false;
                    let out = match s.ops.get_mut(r) {
                        Some(op) => {
                            op.queries += 1;
                            if op.state == "en_curso" && op.queries >= 2 {
                                op.state = if op.amount % 100 == 13 { "denegada".into() } else { "aprobada".into() };
                                charged = op.state == "aprobada";
                            }
                            Some(json(op, r))
                        }
                        None => None,
                    };
                    if charged {
                        s.charges += 1;
                    }
                    match out {
                        Some(o) => (200, o),
                        None => (404, "{}".into()),
                    }
                } else {
                    (404, "{}".into())
                };
                drop(s);
                let _ = req.respond(tiny_http::Response::from_string(out).with_status_code(code));
            }
        });
        (addr, st)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 16).unwrap().and_hms_opt(11, 0, 0).unwrap()
    }

    fn db() -> Db {
        let mut db = Db::sample(now().date());
        db.card_txs.clear();
        db
    }

    fn cards(db: &Db, pid: u32) -> Vec<&Payment> {
        db.payments.iter().filter(|p| p.patient_id == pid && p.method == "Tarjeta" && p.concept.contains("atáfono")).collect()
    }

    /// Envío, consultas hasta que se aprueba, y el cobro una sola vez aunque
    /// se pregunte otra vez o llegue la respuesta repetida.
    #[test]
    fn a_connected_terminal_charges_once() {
        let (url, fake) = fake::start("TOK");
        let term = Http::new(&url, "TOK");
        let mut db = db();
        let id = db.card_new(101, CardKind::Venta, Cents(4500), None, term.name(), now()).unwrap();
        let tx = db.card_sending(id, now()).unwrap();
        assert!(db.card_sending(id, now()).is_err(), "no se envía dos veces");
        assert_eq!(db.card_apply(id, term.send(&tx, None), now()).unwrap(), CardState::EnDatafono);
        assert_eq!(db.card_apply(id, term.query(&tx), now()).unwrap(), CardState::EnDatafono);
        assert_eq!(db.card_apply(id, term.query(&tx), now()).unwrap(), CardState::Aprobada);
        for _ in 0..2 {
            db.card_apply(id, term.query(&tx), now()).unwrap();
            db.card_apply(id, term.send(&tx, None), now()).unwrap();
        }
        let t = db.card_txs.iter().find(|t| t.id == id).unwrap();
        assert_eq!((t.auth_code.as_str(), t.card.as_str()), ("A1B2C3", "···· 0004"), "solo los cuatro últimos dígitos");
        assert!(!serde_json::to_string(&db.card_txs.to_vec()).unwrap().contains("4548"), "el número de la tarjeta no se guarda");
        assert_eq!(cards(&db, 101).len(), 1, "un solo cobro");
        assert_eq!(cards(&db, 101)[0].amount, 45.0);
        assert_eq!(fake.lock().unwrap().charges, 1, "y el datáfono cobró una vez");
        // denegada: sin cobro
        let id2 = db.card_new(101, CardKind::Venta, Cents(1013), None, term.name(), now()).unwrap();
        let tx2 = db.card_sending(id2, now()).unwrap();
        db.card_apply(id2, term.send(&tx2, None), now()).unwrap();
        db.card_apply(id2, term.query(&tx2), now()).unwrap();
        assert_eq!(db.card_apply(id2, term.query(&tx2), now()).unwrap(), CardState::Denegada);
        assert_eq!(cards(&db, 101).len(), 1);
        // con otra clave, el datáfono no atiende
        assert!(Http::new(&url, "OTRA").query(&tx).is_err());
    }

    /// Se corta la respuesta al enviarla: queda «sin respuesta», se pregunta
    /// por la referencia (no se reenvía) y se apunta el cobro que sí se hizo.
    /// Y si no llegó al datáfono, se da por no hecha.
    #[test]
    fn a_lost_reply_is_recovered_by_asking_not_by_resending() {
        let (url, fake) = fake::start("TOK");
        let term = Http::new(&url, "TOK");
        let mut db = db();
        let id = db.card_new(102, CardKind::Venta, Cents(9000), None, term.name(), now()).unwrap();
        let tx = db.card_sending(id, now()).unwrap();
        fake.lock().unwrap().drop_next_reply = true;
        assert_eq!(db.card_apply(id, term.send(&tx, None), now()).unwrap(), CardState::Desconocida);
        assert_eq!(db.card_to_check(now()), vec![id], "hay que comprobarla");
        assert_eq!(db.card_apply(id, term.query(&tx), now()).unwrap(), CardState::Aprobada);
        assert_eq!(cards(&db, 102).len(), 1);
        assert_eq!(fake.lock().unwrap().charges, 1, "una sola vez");
        assert!(db.card_to_check(now()).is_empty());
        // no llegó (503 sin hacerla): al preguntar, no consta → no hecha
        let id2 = db.card_new(102, CardKind::Venta, Cents(2000), None, term.name(), now()).unwrap();
        let tx2 = db.card_sending(id2, now()).unwrap();
        fake.lock().unwrap().refuse_next = true;
        assert_eq!(db.card_apply(id2, term.send(&tx2, None), now()).unwrap(), CardState::Desconocida);
        assert_eq!(db.card_apply(id2, term.query(&tx2), now()).unwrap(), CardState::Cancelada);
        assert_eq!(cards(&db, 102).len(), 1, "sin cobro");
        // en el datáfono hace más de dos minutos: también se comprueba
        let id3 = db.card_new(102, CardKind::Venta, Cents(500), None, term.name(), now()).unwrap();
        let tx3 = db.card_sending(id3, now()).unwrap();
        db.card_apply(id3, term.send(&tx3, None), now()).unwrap();
        assert!(!db.card_to_check(now()).contains(&id3));
        assert!(db.card_to_check(now() + chrono::Duration::minutes(3)).contains(&id3));
    }

    /// Cancelar mientras está en el datáfono; si ya se cobró, manda el
    /// datáfono. Y devolver lo cobrado, sin pasarse.
    #[test]
    fn cancelling_and_refunding_follow_the_terminal() {
        let (url, _fake) = fake::start("TOK");
        let term = Http::new(&url, "TOK");
        let mut db = db();
        let id = db.card_new(103, CardKind::Venta, Cents(6000), None, term.name(), now()).unwrap();
        let tx = db.card_sending(id, now()).unwrap();
        db.card_apply(id, term.send(&tx, None), now()).unwrap();
        assert_eq!(db.card_apply(id, term.cancel(&tx), now()).unwrap(), CardState::Cancelada);
        assert_eq!(db.card_apply(id, term.query(&tx), now()).unwrap(), CardState::Cancelada, "cancelada, ya no se cobra");
        assert!(cards(&db, 103).is_empty());
        // una cancelada que el datáfono dice que sí cobró: manda el datáfono
        let late = Reply { state: Some(CardState::Aprobada), auth_code: "Z9".into(), ..Default::default() };
        assert_eq!(db.card_apply(id, Ok(late), now()).unwrap(), CardState::Aprobada);
        assert_eq!(cards(&db, 103).len(), 1);
        // una aprobada no vuelve atrás
        let back = Reply { state: Some(CardState::Denegada), ..Default::default() };
        assert_eq!(db.card_apply(id, Ok(back), now()).unwrap(), CardState::Aprobada);
        // devolución: de una venta aprobada, sin pasarse
        assert!(db.card_new(103, CardKind::Devolucion, Cents(7000), Some(id), term.name(), now()).is_err(), "más de lo cobrado, no");
        assert!(db.card_new(101, CardKind::Devolucion, Cents(100), Some(id), term.name(), now()).is_err(), "de otro paciente, no");
        let r = db.card_new(103, CardKind::Devolucion, Cents(2500), Some(id), term.name(), now()).unwrap();
        let rtx = db.card_sending(r, now()).unwrap();
        let orig = db.card_txs.iter().find(|t| t.id == id).cloned();
        db.card_apply(r, term.send(&rtx, orig.as_ref()), now()).unwrap();
        db.card_apply(r, term.query(&rtx), now()).unwrap();
        assert_eq!(db.card_apply(r, term.query(&rtx), now()).unwrap(), CardState::Aprobada);
        let amounts: Vec<f64> = cards(&db, 103).iter().map(|p| p.amount).collect();
        assert_eq!(amounts, vec![60.0, -25.0], "el cobro y la devolución");
        assert!(db.card_new(103, CardKind::Devolucion, Cents(3600), Some(id), term.name(), now()).is_err(), "quedan 35 €");
        assert!(db.card_new(103, CardKind::Devolucion, Cents(3500), Some(id), term.name(), now()).is_ok());
    }

    /// Datáfono que no se conecta: se apunta el resultado con la autorización.
    #[test]
    fn a_standalone_terminal_is_recorded_by_hand() {
        let mut db = db();
        let id = db.card_new(104, CardKind::Venta, Cents(3000), None, Manual.name(), now()).unwrap();
        let tx = db.card_sending(id, now()).unwrap();
        assert_eq!(db.card_apply(id, Manual.send(&tx, None), now()).unwrap(), CardState::EnDatafono);
        assert!(db.card_to_check(now() + chrono::Duration::hours(1)).is_empty(), "una manual no se pregunta");
        assert!(db.card_manual_result(id, true, "  ", now()).is_err(), "con su número de autorización");
        assert_eq!(db.card_manual_result(id, true, "123456", now()).unwrap(), CardState::Aprobada);
        assert!(db.card_manual_result(id, true, "123456", now()).is_err(), "una vez");
        assert_eq!(cards(&db, 104).len(), 1);
        let id2 = db.card_new(104, CardKind::Venta, Cents(3000), None, Manual.name(), now()).unwrap();
        db.card_sending(id2, now()).unwrap();
        assert_eq!(db.card_manual_result(id2, false, "", now()).unwrap(), CardState::Denegada);
        assert_eq!(cards(&db, 104).len(), 1);
        assert!(db.card_new(104, CardKind::Venta, Cents(0), None, Manual.name(), now()).is_err());
        assert_eq!(mask_card("4548812049400004"), "···· 0004");
        assert_eq!(mask_card("12"), "");
        assert!(Settings { url: "http://datafono.clinica.es".into(), token: "x".into() }.problem().is_some(), "sin cifrar, solo en este ordenador");
        assert!(Settings { url: "https://datafono.clinica.es".into(), token: String::new() }.problem().is_some());
        assert!(Settings::default().problem().is_none());
    }
}
