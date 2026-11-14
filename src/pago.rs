//! Pago por internet (T-F5-02): presupuestos, cuotas y señales de reserva con
//! el TPV virtual de Redsys, con tarjeta o con Bizum, en la página de pago de
//! Redsys (redirección). Ver `docs/normativa.md`.
//!
//! **Fuente:** documentación de desarrolladores de Redsys
//! (pagosonline.redsys.es), consultada el 14/11/2026: «Realizar un pago»,
//! «Firmar una operación» (HMAC_SHA512_V2), «Devolver o anular un pago» y
//! «Parámetros de entrada y salida». La firma se comprueba con sus ejemplos.
//!
//! - **Sin datos de tarjeta:** el paciente paga en la página de Redsys. Aquí no
//!   pasa ni se guarda ningún dato de la tarjeta.
//! - **El importe lo pone el programa:** sale de la petición de pago (el
//!   presupuesto, la cuota o la señal), nunca del navegador.
//! - **Solo cuenta la notificación firmada de Redsys:** tiene que ser del
//!   comercio y terminal configurados, en euros y por el importe pedido. Que el
//!   paciente vuelva a la página de «pago hecho» no da nada por pagado.
//! - **Una sola vez:**
//!   - el mismo pedido notificado dos veces se apunta una vez;
//!   - si se paga dos veces (dos pestañas), el segundo cobro se apunta, porque
//!     el dinero entró, y queda marcado para devolverlo.
//! - **Incierto:** un intento sin notificación pasada [`UNCERTAIN_MINUTES`] se
//!   marca para comprobarlo en el portal del TPV. Nunca se da por pagado solo.
//! - **Devoluciones:** petición REST firmada (tipo 3; «0900» si se hace). Un
//!   corte después de enviarla la deja incierta, y no se repite sola.
//! - **Señal de reserva:** política configurable que ve el paciente antes de
//!   pagar ([`DepositPolicy::text`]). Al anular la cita, se devuelve o se
//!   retiene según esa política ([`Db::deposit_on_cancel`]).
//! - **Clave del comercio:** solo en el ordenador que publica la página, en un
//!   fichero privado (`tpv.json`), nunca en los datos de la clínica.

use crate::model::*;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const SIGNATURE_VERSION: &str = "HMAC_SHA512_V2";
/// Sin notificación pasado este tiempo, el intento se comprueba a mano.
pub const UNCERTAIN_MINUTES: i64 = 30;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Env {
    #[default]
    Pruebas,
    Real,
}

impl Env {
    pub fn label(self) -> &'static str {
        match self {
            Env::Pruebas => "Pruebas (sin cargos)",
            Env::Real => "Real",
        }
    }
    fn host(self) -> &'static str {
        match self {
            Env::Pruebas => "https://sis-t.redsys.es:25443",
            Env::Real => "https://sis.redsys.es",
        }
    }
}

// ───────────────────────────── el TPV de este ordenador ─────────────────────────────

/// Datos del TPV virtual que da el banco. Solo en el ordenador que publica la
/// página, en un fichero privado.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TpvSettings {
    #[serde(default)]
    pub env: Env,
    /// Número de comercio (FUC), de 9 cifras.
    #[serde(default)]
    pub merchant: String,
    #[serde(default)]
    pub terminal: String,
    /// Clave de firma del terminal.
    #[serde(default)]
    pub key: String,
    /// Dirección alternativa de Redsys (solo para pruebas internas).
    #[serde(default)]
    pub endpoint: String,
}

impl TpvSettings {
    pub fn problem(&self) -> Option<&'static str> {
        if self.merchant.len() != 9 || !self.merchant.chars().all(|c| c.is_ascii_digit()) {
            return Some("El número de comercio (FUC) tiene 9 cifras.");
        }
        if self.terminal.is_empty() || self.terminal.len() > 3 || !self.terminal.chars().all(|c| c.is_ascii_digit()) {
            return Some("El terminal es un número de hasta 3 cifras (normalmente 1).");
        }
        if self.key.trim().is_empty() {
            return Some("Falta la clave de firma del terminal.");
        }
        None
    }

    fn base(&self) -> String {
        if self.endpoint.trim().is_empty() { self.env.host().to_string() } else { self.endpoint.trim().trim_end_matches('/').to_string() }
    }

    /// Página de pago de Redsys (redirección).
    pub fn pay_url(&self) -> String {
        format!("{}/sis/realizarPago", self.base())
    }

    /// Peticiones REST (devoluciones).
    pub fn rest_url(&self) -> String {
        format!("{}/sis/rest/trataPeticionREST", self.base())
    }

    fn same_terminal(&self, t: &str) -> bool {
        t.trim().parse::<u32>().ok().is_some_and(|n| Some(n) == self.terminal.parse::<u32>().ok())
    }
}

fn settings_path() -> std::path::PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("tpv.json")
}

pub fn load_settings() -> TpvSettings {
    std::fs::read_to_string(settings_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_settings(s: &TpvSettings) -> Result<(), String> {
    let p = settings_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    crate::paths::write_private(&p, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())
}

// ───────────────────────────── firma HMAC_SHA512_V2 ─────────────────────────────

/// Base64URL sin relleno (como los parámetros y la firma de Redsys).
pub fn b64url(data: &[u8]) -> String {
    crate::ia::b64(data).replace('+', "-").replace('/', "_").trim_end_matches('=').to_string()
}

/// Base64 de los dos alfabetos, con o sin relleno.
pub fn unb64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.trim().trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Clave de la operación: AES-CBC (vector de ceros, relleno PKCS#7) del número
/// de pedido con los 16 primeros caracteres de la clave del terminal
/// (rellenada con «0» si es más corta), en Base64.
fn order_key(key: &str, order: &str) -> String {
    use aes::cipher::{BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
    let mut k = [b'0'; 16];
    for (i, b) in key.trim().bytes().take(16).enumerate() {
        k[i] = b;
    }
    let ct = cbc::Encryptor::<aes::Aes128>::new(&k.into(), &[0u8; 16].into()).encrypt_padded_vec::<Pkcs7>(order.as_bytes());
    crate::ia::b64(&ct)
}

/// Firma de unos parámetros (ya en Base64URL) de un pedido.
pub fn sign(key: &str, order: &str, params: &str) -> String {
    let k = ring::hmac::Key::new(ring::hmac::HMAC_SHA512, order_key(key, order).as_bytes());
    b64url(ring::hmac::sign(&k, params.as_bytes()).as_ref())
}

/// Parámetros JSON en Base64URL, como los manda Redsys.
pub fn encode_params(v: &serde_json::Value) -> String {
    b64url(v.to_string().as_bytes())
}

/// Comprueba la firma de unos parámetros recibidos de Redsys y los devuelve.
pub fn verify(key: &str, params: &str, signature: &str) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let raw = unb64(params).ok_or("Los parámetros no están en Base64.")?;
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(&raw).map_err(|_| "Los parámetros no son un JSON.")?;
    let order = get(&map, "Ds_Order").or_else(|| get(&map, "DS_MERCHANT_ORDER")).ok_or("Falta el número de pedido.")?;
    let want = sign(key, &order, params);
    let norm = |s: &str| s.trim().trim_end_matches('=').replace('+', "-").replace('/', "_");
    if !crate::users::ct_eq(norm(&want).as_bytes(), norm(signature).as_bytes()) {
        return Err("La firma no es válida.".into());
    }
    Ok(map)
}

/// Un campo (en mayúsculas o no) como texto.
fn get(m: &serde_json::Map<String, serde_json::Value>, k: &str) -> Option<String> {
    let v = m.iter().find(|(x, _)| x.eq_ignore_ascii_case(k))?.1;
    Some(match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    })
}

// ───────────────────────────── la página de pago ─────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    #[default]
    Tarjeta,
    Bizum,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::Tarjeta => "Tarjeta",
            Method::Bizum => "Bizum",
        }
    }
}

/// Formulario que el navegador del paciente envía a Redsys.
#[derive(Clone, Debug, PartialEq)]
pub struct PayForm {
    pub url: String,
    pub params: String,
    pub signature: String,
}

impl PayForm {
    /// Página que lo envía sola (y con un botón, por si el navegador no ejecuta código).
    pub fn html(&self) -> String {
        let e = crate::booking::esc_html;
        format!(
            "<!doctype html><html lang=\"es\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Pago</title><body onload=\"document.forms[0].submit()\"><form method=\"post\" action=\"{}\"><input type=\"hidden\" name=\"Ds_SignatureVersion\" value=\"{SIGNATURE_VERSION}\"><input type=\"hidden\" name=\"Ds_MerchantParameters\" value=\"{}\"><input type=\"hidden\" name=\"Ds_Signature\" value=\"{}\"><p>Le llevamos a la página de pago segura del banco…</p><button>Continuar al pago</button></form></body></html>",
            e(&self.url),
            e(&self.params),
            e(&self.signature)
        )
    }
}

/// Número de pedido nuevo: 12 caracteres, los 4 primeros cifras (como
/// recomienda Redsys) y el resto al azar. Redsys no admite un pedido repetido.
pub fn new_order(request_id: u32) -> String {
    use ring::rand::SecureRandom;
    const A: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut r = [0u8; 8];
    let _ = ring::rand::SystemRandom::new().fill(&mut r);
    let tail: String = r.iter().map(|b| A[*b as usize % A.len()] as char).collect();
    format!("{:04}{tail}", request_id % 10_000)
}

/// Formulario de pago de `amount` céntimos (el importe de la petición).
pub fn pay_form(s: &TpvSettings, order: &str, amount: i64, method: Method, notify_url: &str, back_url: &str, concept: &str) -> Result<PayForm, String> {
    if let Some(p) = s.problem() {
        return Err(p.into());
    }
    if amount <= 0 {
        return Err("No hay nada que pagar.".into());
    }
    let mut p = serde_json::json!({
        "DS_MERCHANT_ORDER": order,
        "DS_MERCHANT_MERCHANTCODE": s.merchant,
        "DS_MERCHANT_TERMINAL": s.terminal,
        "DS_MERCHANT_CURRENCY": "978",
        "DS_MERCHANT_TRANSACTIONTYPE": "0",
        "DS_MERCHANT_AMOUNT": amount.to_string(),
        "DS_MERCHANT_MERCHANTURL": notify_url,
        "DS_MERCHANT_URLOK": format!("{back_url}?resultado=ok"),
        "DS_MERCHANT_URLKO": format!("{back_url}?resultado=ko"),
        "DS_MERCHANT_PRODUCTDESCRIPTION": concept.chars().take(125).collect::<String>(),
    });
    if method == Method::Bizum {
        p["DS_MERCHANT_PAYMETHODS"] = "z".into();
    }
    let params = encode_params(&p);
    Ok(PayForm { url: s.pay_url(), signature: sign(&s.key, order, &params), params })
}

/// Lo que dice una notificación de Redsys, ya comprobada.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub order: String,
    /// Céntimos.
    pub amount: i64,
    /// `Ds_Response` como número.
    pub response: u32,
    pub auth: String,
    /// Bizum («68» en `Ds_ProcessedPayMethod`).
    pub bizum: bool,
}

impl Notice {
    /// 0000 a 0099: pago autorizado.
    pub fn paid(&self) -> bool {
        self.response <= 99
    }
}

/// Lee un formulario `application/x-www-form-urlencoded`.
pub fn parse_form(body: &str) -> HashMap<String, String> {
    let dec = |s: &str| {
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'+' => out.push(b' '),
                b'%' if i + 2 < b.len() => match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                },
                c => out.push(c),
            }
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    };
    body.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (dec(k), dec(v))).collect()
}

/// Comprueba una notificación (los tres campos del formulario o del JSON).
pub fn read_notice(s: &TpvSettings, version: &str, params: &str, signature: &str) -> Result<Notice, String> {
    if !version.trim().eq_ignore_ascii_case(SIGNATURE_VERSION) {
        return Err(format!("Versión de firma no admitida: {version}."));
    }
    let m = verify(&s.key, params, signature)?;
    let field = |k: &str| get(&m, k).unwrap_or_default();
    if field("Ds_MerchantCode") != s.merchant || !s.same_terminal(&field("Ds_Terminal")) {
        return Err("La notificación es de otro comercio o terminal.".into());
    }
    if field("Ds_Currency") != "978" {
        return Err("La notificación no es en euros.".into());
    }
    Ok(Notice {
        order: field("Ds_Order"),
        amount: field("Ds_Amount").trim().parse().map_err(|_| "Importe no válido en la notificación.")?,
        response: field("Ds_Response").trim().parse().map_err(|_| "Respuesta no válida en la notificación.")?,
        auth: field("Ds_AuthorisationCode").trim().to_string(),
        bizum: field("Ds_ProcessedPayMethod").trim() == "68",
    })
}

// ───────────────────────────── devoluciones ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum RefundOutcome {
    /// Hecha («0900»).
    Done { auth: String },
    /// Redsys la rechaza (código).
    Denied(String),
    /// No ha salido: se puede repetir.
    NotSent(String),
    /// Puede haberse hecho: se comprueba en el portal del TPV antes de repetir.
    Uncertain(String),
}

/// Pide a Redsys la devolución de `amount` céntimos del pedido `order`.
pub fn send_refund(s: &TpvSettings, order: &str, amount: i64) -> RefundOutcome {
    if let Some(p) = s.problem() {
        return RefundOutcome::NotSent(p.into());
    }
    let params = encode_params(&serde_json::json!({
        "DS_MERCHANT_ORDER": order,
        "DS_MERCHANT_MERCHANTCODE": s.merchant,
        "DS_MERCHANT_TERMINAL": s.terminal,
        "DS_MERCHANT_TRANSACTIONTYPE": "3",
        "DS_MERCHANT_CURRENCY": "978",
        "DS_MERCHANT_AMOUNT": amount.to_string(),
    }));
    let body = serde_json::json!({ "Ds_SignatureVersion": SIGNATURE_VERSION, "Ds_MerchantParameters": params, "Ds_Signature": sign(&s.key, order, &params) });
    let agent = ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(10)).timeout(std::time::Duration::from_secs(40)).build();
    let text = match agent.post(&s.rest_url()).set("Content-Type", "application/json").send_string(&body.to_string()) {
        Ok(r) => r.into_string(),
        Err(ureq::Error::Status(code, r)) if (400..500).contains(&code) && code != 408 => return RefundOutcome::NotSent(format!("Redsys respondió {code}: {}", r.into_string().unwrap_or_default().chars().take(200).collect::<String>())),
        Err(ureq::Error::Status(code, _)) => return RefundOutcome::Uncertain(format!("Redsys respondió {code}: compruebe la operación en el portal del TPV antes de repetirla.")),
        Err(ureq::Error::Transport(t)) if matches!(t.kind(), ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed) => return RefundOutcome::NotSent(format!("Sin conexión con Redsys: {t}")),
        Err(e) => return RefundOutcome::Uncertain(format!("Se cortó la conexión con Redsys ({e}): compruebe la operación en el portal del TPV antes de repetirla.")),
    };
    let Ok(text) = text else { return RefundOutcome::Uncertain("No se pudo leer la respuesta de Redsys: compruebe la operación en el portal del TPV.".into()) };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return RefundOutcome::Uncertain("Respuesta de Redsys no válida: compruebe la operación en el portal del TPV.".into()) };
    if let Some(code) = v["errorCode"].as_str() {
        return RefundOutcome::Denied(code.to_string());
    }
    let s_ = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    match read_notice(s, &s_("Ds_SignatureVersion"), &s_("Ds_MerchantParameters"), &s_("Ds_Signature")) {
        Ok(n) if n.order == order && n.response == 900 && n.amount == amount => RefundOutcome::Done { auth: n.auth },
        Ok(n) if n.order != order || n.amount != amount => RefundOutcome::Uncertain("La respuesta de Redsys no es de esta devolución: compruébela en el portal del TPV.".into()),
        Ok(n) => RefundOutcome::Denied(format!("{:04}", n.response)),
        Err(e) => RefundOutcome::Uncertain(format!("Respuesta de Redsys sin firma válida ({e}): compruebe la operación en el portal del TPV.")),
    }
}

// ───────────────────────────── las peticiones de pago ─────────────────────────────

/// Pago por internet de la clínica (sin secretos: la clave va aparte).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PayConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bizum: bool,
    #[serde(default)]
    pub deposit: DepositPolicy,
}

impl PayConfig {
    pub fn is_default(&self) -> bool {
        *self == PayConfig::default()
    }
}

/// Señal de reserva de una cita.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DepositPolicy {
    /// Céntimos (0: sin señal).
    #[serde(default)]
    pub amount: i64,
    /// Si el paciente anula con al menos estas horas de antelación, se le devuelve.
    #[serde(default)]
    pub refund_hours: u32,
}

impl DepositPolicy {
    /// Lo que ve el paciente antes de pagar, y lo que se le aplica.
    pub fn text(&self) -> String {
        let a = crate::cents::Cents(self.amount);
        if self.refund_hours == 0 {
            format!("La señal de {a} se descuenta del tratamiento. Si anula la cita, se le devuelve. Si la anula la clínica, siempre se le devuelve.")
        } else {
            format!(
                "La señal de {a} se descuenta del tratamiento. Si anula la cita con al menos {} horas de antelación, se le devuelve; si la anula con menos o no acude, la clínica la conserva. Si la anula la clínica, siempre se le devuelve.",
                self.refund_hours
            )
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayFor {
    /// Un presupuesto (o parte).
    Budget(u32),
    /// Una cuota de un plan: (presupuesto, nº de cuota desde 0).
    Installment(u32, u32),
    /// La señal de una cita.
    Deposit(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayState {
    #[default]
    Pendiente,
    Pagado,
    /// La anula la clínica antes de pagarse.
    Anulado,
    /// Señal que la clínica conserva según la política.
    Retenido,
    Devuelto,
}

impl PayState {
    pub fn label(self) -> &'static str {
        match self {
            PayState::Pendiente => "Pendiente",
            PayState::Pagado => "Pagado",
            PayState::Anulado => "Anulado",
            PayState::Retenido => "Señal retenida",
            PayState::Devuelto => "Devuelto",
        }
    }
}

/// Un intento de pago (un pedido en Redsys).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attempt {
    pub order: String,
    pub method: Method,
    pub at: NaiveDateTime,
    /// `Ds_Response` de la notificación (ninguna: aún no ha llegado).
    #[serde(default)]
    pub response: Option<u32>,
    /// Aviso (importe distinto, comprobado a mano…).
    #[serde(default)]
    pub note: String,
}

/// Un cobro hecho por el TPV.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Charge {
    pub order: String,
    pub payment_id: u32,
    pub amount: i64,
    pub method: Method,
    #[serde(default)]
    pub auth: String,
    /// Céntimos ya devueltos de este cobro.
    #[serde(default)]
    pub refunded: i64,
    /// Devolución enviada sin respuesta clara: no se repite sin comprobarla.
    #[serde(default)]
    pub refund_uncertain: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PayRequest {
    pub id: u32,
    /// Código del enlace (128 bits).
    pub token: String,
    pub patient_id: u32,
    pub what: PayFor,
    pub concept: String,
    /// Céntimos.
    pub amount: i64,
    pub created: NaiveDateTime,
    #[serde(default)]
    pub created_by: String,
    #[serde(default)]
    pub state: PayState,
    #[serde(default)]
    pub attempts: Vec<Attempt>,
    #[serde(default)]
    pub charges: Vec<Charge>,
    /// Hay que devolver (cobro doble, señal de una cita anulada a tiempo…).
    #[serde(default)]
    pub refund_due: bool,
    #[serde(default)]
    pub note: String,
}

impl PayRequest {
    pub fn paid(&self) -> i64 {
        self.charges.iter().map(|c| c.amount - c.refunded).sum()
    }

    /// Intentos sin notificación pasado [`UNCERTAIN_MINUTES`].
    pub fn uncertain(&self, now: NaiveDateTime) -> Vec<&Attempt> {
        self.attempts.iter().filter(|a| a.response.is_none() && a.note.is_empty() && (now - a.at).num_minutes() >= UNCERTAIN_MINUTES).collect()
    }
}

/// Qué ha pasado con una notificación.
#[derive(Clone, Debug, PartialEq)]
pub enum Applied {
    Paid {
        payment_id: u32,
    },
    /// Pagado otra vez (otro pedido): apuntado y para devolver.
    PaidTwice {
        payment_id: u32,
    },
    /// Ya se había apuntado este pedido.
    Duplicate,
    Denied(u32),
    /// Importe distinto del pedido: no se apunta solo.
    Review(String),
}

impl Db {
    pub fn pay_request(&self, id: u32) -> Option<&PayRequest> {
        self.pay_requests.iter().find(|r| r.id == id)
    }

    /// Lo que se pide en cada caso: el importe sale del programa.
    fn expected(&self, what: PayFor) -> Result<(u32, i64, String), String> {
        let cents = |e: f64| crate::cents::Cents::from_eur(e).0;
        match what {
            PayFor::Budget(bid) => {
                let b = self.budgets.iter().find(|b| b.id == bid).ok_or("El presupuesto ya no existe.")?;
                if matches!(b.state, BudgetState::Rechazado | BudgetState::Cerrado) {
                    return Err("El presupuesto está rechazado o cerrado.".into());
                }
                Ok((b.patient_id, cents(b.total()), format!("Presupuesto «{}»", b.title)))
            }
            PayFor::Installment(bid, i) => {
                let b = self.budgets.iter().find(|b| b.id == bid).ok_or("El presupuesto ya no existe.")?;
                let plan = b.plan.as_ref().ok_or("El presupuesto no tiene plan de cuotas.")?;
                if i >= plan.installments {
                    return Err("Esa cuota no existe.".into());
                }
                if plan.paid.get(i as usize).copied().unwrap_or(false) {
                    return Err(format!("La cuota {} ya está pagada.", i + 1));
                }
                if let Some(r) = self.installment_remittance(bid, i) {
                    return Err(format!("La cuota {} va en la remesa #{r} enviada al banco.", i + 1));
                }
                Ok((b.patient_id, cents(plan.installment(i, b.total())), format!("Cuota {} del presupuesto «{}»", i + 1, b.title)))
            }
            PayFor::Deposit(aid) => {
                let a = self.appointments.iter().find(|a| a.id == aid).ok_or("La cita ya no existe.")?;
                let pid = a.patient_id.ok_or("La cita no es de un paciente con ficha.")?;
                if a.state.is_cancelled() {
                    return Err("La cita está anulada.".into());
                }
                let amount = self.config.pay_online.deposit.amount;
                if amount <= 0 {
                    return Err("La clínica no pide señal (Configuración › Pago por internet).".into());
                }
                Ok((pid, amount, format!("Señal de la cita del {} a las {}", fmt_date(a.date), a.start.format("%H:%M"))))
            }
        }
    }

    /// Petición de pago por internet de `what` (el importe lo calcula el
    /// programa; `amount` solo puede pedir menos en un presupuesto, a cuenta).
    /// Si ya hay una abierta para lo mismo, la devuelve.
    pub fn create_pay_request(&mut self, what: PayFor, amount: Option<i64>, now: NaiveDateTime) -> Result<u32, String> {
        if !self.config.pay_online.enabled {
            return Err("El pago por internet no está activado (Configuración › Pago por internet).".into());
        }
        if let Some(r) = self.pay_requests.iter().find(|r| r.what == what && r.state == PayState::Pendiente) {
            return Ok(r.id);
        }
        if let Some(r) = self.pay_requests.iter().find(|r| r.what == what && r.state == PayState::Pagado && !matches!(what, PayFor::Budget(_))) {
            return Err(format!("Ya está pagado por internet (petición #{}).", r.id));
        }
        let (pid, max, concept) = self.expected(what)?;
        let amount = match (what, amount) {
            (PayFor::Budget(_), Some(a)) if a > 0 && a <= max => a,
            (PayFor::Budget(_), Some(_)) => return Err(format!("El importe tiene que ser mayor que 0 y no pasar de {}.", crate::cents::Cents(max))),
            _ => max,
        };
        let id = self.new_id();
        let token = crate::portal::new_token();
        self.pay_requests.push(PayRequest {
            id,
            token,
            patient_id: pid,
            what,
            concept: concept.clone(),
            amount,
            created: now,
            created_by: self.config.current_user.clone(),
            state: PayState::Pendiente,
            attempts: vec![],
            charges: vec![],
            refund_due: false,
            note: String::new(),
        });
        self.audit("Pedir pago por internet", &format!("{} · {concept} · {}", self.patient_name(pid), crate::cents::Cents(amount)));
        Ok(id)
    }

    /// La clínica anula una petición aún sin pagar.
    pub fn cancel_pay_request(&mut self, id: u32) -> Result<(), String> {
        let r = self.pay_requests.iter_mut().find(|r| r.id == id).ok_or("La petición ya no existe.")?;
        if r.state != PayState::Pendiente {
            return Err("Solo se anula una petición pendiente.".into());
        }
        if r.attempts.iter().any(|a| a.response.is_none() && a.note.is_empty()) {
            return Err("Hay un intento de pago sin confirmar: compruébelo antes en el portal del TPV.".into());
        }
        r.state = PayState::Anulado;
        Ok(())
    }

    /// El paciente va a pagar: se apunta el pedido nuevo.
    pub fn pay_started(&mut self, token: &str, order: &str, method: Method, now: NaiveDateTime) -> bool {
        let Some(r) = self.pay_requests.iter_mut().find(|r| r.token == token) else { return false };
        if r.attempts.iter().any(|a| a.order == order) {
            return false;
        }
        r.attempts.push(Attempt { order: order.into(), method, at: now, response: None, note: String::new() });
        true
    }

    /// Apunta una notificación de Redsys ya comprobada (firma, comercio, euros).
    pub fn apply_notice(&mut self, n: &Notice, now: NaiveDateTime) -> Result<Applied, String> {
        let idx = self.pay_requests.iter().position(|r| r.attempts.iter().any(|a| a.order == n.order)).ok_or_else(|| format!("Pedido desconocido: {}.", n.order))?;
        let r = self.pay_requests[idx].clone();
        let Some(a) = r.attempts.iter().find(|a| a.order == n.order).cloned() else { return Err(format!("Pedido desconocido: {}.", n.order)) };
        if a.response.is_some() || r.charges.iter().any(|c| c.order == n.order) {
            return Ok(Applied::Duplicate);
        }
        let set = |db: &mut Db, response: u32, note: &str| {
            if let Some(x) = db.pay_requests[idx].attempts.iter_mut().find(|x| x.order == n.order) {
                x.response = Some(response);
                x.note = note.to_string();
            }
        };
        if !n.paid() {
            set(self, n.response, "");
            return Ok(Applied::Denied(n.response));
        }
        if n.amount != r.amount {
            let note = format!("Importe distinto del pedido: {} en lugar de {}.", crate::cents::Cents(n.amount), crate::cents::Cents(r.amount));
            set(self, n.response, &note);
            self.pay_requests[idx].note = note.clone();
            return Ok(Applied::Review(note));
        }
        set(self, n.response, "");
        let method = if n.bizum || a.method == Method::Bizum { Method::Bizum } else { Method::Tarjeta };
        let twice = r.state == PayState::Pagado || !r.charges.is_empty();
        let payment_id = self.charge(idx, &n.order, n.amount, method, &n.auth, now);
        if twice {
            let p = &mut self.pay_requests[idx];
            p.refund_due = true;
            p.note = "Pagado dos veces: devuelva el segundo cobro.".into();
            Ok(Applied::PaidTwice { payment_id })
        } else {
            Ok(Applied::Paid { payment_id })
        }
    }

    /// Apunta un cobro del TPV (y la cuota, si era una cuota).
    fn charge(&mut self, idx: usize, order: &str, amount: i64, method: Method, auth: &str, _now: NaiveDateTime) -> u32 {
        let r = self.pay_requests[idx].clone();
        let pid = self.add_payment(r.patient_id, amount as f64 / 100.0, method.label(), &format!("{} · pago por internet, pedido {order}", r.concept));
        if let PayFor::Installment(bid, i) = r.what
            && r.charges.is_empty()
        {
            if let Some(p) = self.payments.iter_mut().find(|p| p.id == pid) {
                p.installment = Some((bid, i));
            }
            self.set_installment_paid(bid, i, true);
        }
        let p = &mut self.pay_requests[idx];
        p.charges.push(Charge { order: order.into(), payment_id: pid, amount, method, auth: auth.into(), refunded: 0, refund_uncertain: false });
        p.state = PayState::Pagado;
        let who = self.patient_name(r.patient_id);
        self.audit("Cobro por internet", &format!("{who} · {} · {} · pedido {order}", r.concept, crate::cents::Cents(amount)));
        pid
    }

    /// Un intento sin notificación, comprobado a mano en el portal del TPV:
    /// cobrado (se apunta) o no.
    pub fn settle_by_hand(&mut self, id: u32, order: &str, charged: bool, now: NaiveDateTime) -> Result<Option<u32>, String> {
        let idx = self.pay_requests.iter().position(|r| r.id == id).ok_or("La petición ya no existe.")?;
        let a = self.pay_requests[idx].attempts.iter().find(|a| a.order == order).cloned().ok_or("Ese pedido no es de esta petición.")?;
        if a.response.is_some() || !a.note.is_empty() {
            return Err("Ese intento ya está resuelto.".into());
        }
        let who = self.config.current_user.clone();
        if let Some(x) = self.pay_requests[idx].attempts.iter_mut().find(|x| x.order == order) {
            x.note = format!("Comprobado a mano por {who}: {}.", if charged { "cobrado" } else { "no cobrado" });
        }
        if !charged {
            return Ok(None);
        }
        let amount = self.pay_requests[idx].amount;
        let twice = !self.pay_requests[idx].charges.is_empty();
        let pid = self.charge(idx, order, amount, a.method, "", now);
        if twice {
            self.pay_requests[idx].refund_due = true;
        }
        Ok(Some(pid))
    }

    /// Al anular una cita con señal pagada: se devuelve (la anula la clínica,
    /// o el paciente con la antelación de la política) o se retiene. Devuelve
    /// lo decidido, para avisar.
    pub fn deposit_on_cancel(&mut self, appt_id: u32, by_patient: bool, now: NaiveDateTime) -> Option<String> {
        let a = self.appointments.iter().find(|a| a.id == appt_id)?.clone();
        let hours = self.config.pay_online.deposit.refund_hours;
        let r = self.pay_requests.iter_mut().find(|r| r.what == PayFor::Deposit(appt_id) && r.state == PayState::Pagado)?;
        let start = a.date.and_time(a.start);
        let ahead = (start - now).num_minutes();
        let refund = !by_patient || ahead >= hours as i64 * 60;
        let msg = if refund {
            r.refund_due = true;
            r.note = if by_patient { format!("Anulada por el paciente con {} h de antelación: se devuelve la señal.", ahead.max(0) / 60) } else { "Anulada por la clínica: se devuelve la señal.".into() };
            format!("Hay que devolver la señal de {} ({}).", crate::cents::Cents(r.paid()), r.concept)
        } else {
            r.state = PayState::Retenido;
            r.note = format!("Anulada por el paciente con {} h de antelación (menos de {hours} h): la clínica conserva la señal.", ahead.max(0) / 60);
            format!("La señal de {} se conserva según la política ({}).", crate::cents::Cents(r.paid()), r.concept)
        };
        Some(msg)
    }

    /// Apunta el resultado de una devolución pedida a Redsys.
    pub fn record_refund(&mut self, id: u32, order: &str, amount: i64, outcome: &RefundOutcome) -> Result<String, String> {
        let idx = self.pay_requests.iter().position(|r| r.id == id).ok_or("La petición ya no existe.")?;
        let c = self.pay_requests[idx].charges.iter().find(|c| c.order == order).cloned().ok_or("Ese cobro no es de esta petición.")?;
        match outcome {
            RefundOutcome::Done { auth } => {
                let pat = self.pay_requests[idx].patient_id;
                let back = self.add_payment(pat, -(amount as f64) / 100.0, c.method.label(), &format!("Devolución por internet del pedido {order} (autorización {auth})"));
                if amount == c.amount
                    && let Some(x) = self.payments.iter_mut().find(|x| x.id == back)
                {
                    x.cancels = Some(c.payment_id);
                }
                let r = &mut self.pay_requests[idx];
                if let Some(x) = r.charges.iter_mut().find(|x| x.order == order) {
                    x.refunded += amount;
                    x.refund_uncertain = false;
                }
                // todo devuelto; o, si se pagó dos veces, ya solo queda un pago
                let left = r.paid();
                if left <= 0 {
                    r.state = PayState::Devuelto;
                    r.refund_due = false;
                } else if r.refund_due && !matches!(r.what, PayFor::Deposit(_)) && left <= r.amount {
                    r.refund_due = false;
                }
                let (pid, concept) = (r.patient_id, r.concept.clone());
                if let PayFor::Installment(bid, i) = self.pay_requests[idx].what
                    && self.pay_requests[idx].paid() <= 0
                {
                    self.set_installment_paid(bid, i, false);
                }
                let who = self.patient_name(pid);
                self.audit("Devolución por internet", &format!("{who} · {concept} · {} · pedido {order}", crate::cents::Cents(amount)));
                Ok(format!("Devolución hecha: {}.", crate::cents::Cents(amount)))
            }
            RefundOutcome::Uncertain(e) => {
                if let Some(x) = self.pay_requests[idx].charges.iter_mut().find(|x| x.order == order) {
                    x.refund_uncertain = true;
                }
                Err(e.clone())
            }
            RefundOutcome::Denied(code) => Err(format!("Redsys no ha hecho la devolución (código {code}).")),
            RefundOutcome::NotSent(e) => Err(e.clone()),
        }
    }

    /// Antes de pedir una devolución: cuánto se puede devolver de ese cobro.
    pub fn refundable(&self, id: u32, order: &str) -> Result<i64, String> {
        let r = self.pay_request(id).ok_or("La petición ya no existe.")?;
        let c = r.charges.iter().find(|c| c.order == order).ok_or("Ese cobro no es de esta petición.")?;
        if c.refund_uncertain {
            return Err("Hay una devolución de este cobro sin respuesta clara: compruébela en el portal del TPV antes de repetirla.".into());
        }
        let left = c.amount - c.refunded;
        if left <= 0 {
            return Err("Ese cobro ya está devuelto.".into());
        }
        Ok(left)
    }

    /// Una devolución incierta comprobada a mano en el portal del TPV.
    pub fn settle_refund_by_hand(&mut self, id: u32, order: &str, amount: i64, done: bool) -> Result<String, String> {
        let idx = self.pay_requests.iter().position(|r| r.id == id).ok_or("La petición ya no existe.")?;
        if let Some(x) = self.pay_requests[idx].charges.iter_mut().find(|x| x.order == order) {
            x.refund_uncertain = false;
        }
        if done { self.record_refund(id, order, amount, &RefundOutcome::Done { auth: "comprobada a mano".into() }) } else { Ok("Devolución no hecha: se puede pedir otra vez.".into()) }
    }
}

// ───────────────────────────── la página del paciente ─────────────────────────────

/// Lo que se publica de cada petición (sin datos de salud).
#[derive(Clone, Debug, PartialEq)]
pub struct PubPay {
    pub id: u32,
    pub first_name: String,
    pub concept: String,
    pub amount: i64,
    pub state: PayState,
    /// Política de la señal (solo en las señales).
    pub policy: Option<String>,
    pub bizum: bool,
}

/// Peticiones que se pueden abrir: las pendientes y, 60 días, las demás.
pub fn publish_pays(db: &Db, now: NaiveDateTime) -> HashMap<String, PubPay> {
    if !db.config.pay_online.enabled {
        return HashMap::new();
    }
    db.pay_requests
        .iter()
        .filter(|r| r.state == PayState::Pendiente || (now - r.created).num_days() <= 60)
        .map(|r| {
            let policy = matches!(r.what, PayFor::Deposit(_)).then(|| db.config.pay_online.deposit.text());
            let first = db.patient(r.patient_id).map(|p| p.first_name.clone()).unwrap_or_default();
            (r.token.clone(), PubPay { id: r.id, first_name: first, concept: r.concept.clone(), amount: r.amount, state: r.state, policy, bizum: db.config.pay_online.bizum })
        })
        .collect()
}

/// La página de pago (sin código: funciona con cualquier navegador).
pub fn pay_page(clinic: &str, token: &str, p: &PubPay, result: Option<&str>, ready: bool) -> String {
    let e = crate::booking::esc_html;
    let amount = crate::cents::Cents(p.amount).to_string();
    let mut body = format!("<p class=\"hint\">Hola{}. Pago de <b>{}</b>.</p><p class=\"amount\">{}</p>", if p.first_name.is_empty() { String::new() } else { format!(", {}", e(&p.first_name)) }, e(&p.concept), e(&amount));
    if let Some(pol) = &p.policy {
        body.push_str(&format!("<p class=\"policy\"><b>Condiciones de la señal.</b> {}</p>", e(pol)));
    }
    let refresh = match (p.state, result) {
        (PayState::Pagado, _) => {
            body.push_str("<p class=\"ok\">Pago recibido. Muchas gracias.</p>");
            false
        }
        (PayState::Pendiente, Some("ok")) => {
            body.push_str("<p class=\"wait\">Estamos confirmando el pago con el banco. Esta página se actualiza sola; si no cambia en unos minutos, llame a la clínica antes de volver a pagar.</p>");
            true
        }
        (PayState::Pendiente, r) => {
            if r == Some("ko") {
                body.push_str("<p class=\"err\">El pago no se ha completado. Puede intentarlo de nuevo.</p>");
            }
            if ready {
                let form = |m: &str, label: &str| format!("<form method=\"post\" action=\"/api/pagar/{}/iniciar\"><input type=\"hidden\" name=\"metodo\" value=\"{m}\"><button class=\"main\">{label}</button></form>", e(token));
                body.push_str(&form("tarjeta", "Pagar con tarjeta"));
                if p.bizum {
                    body.push_str(&form("bizum", "Pagar con Bizum"));
                }
                body.push_str("<p class=\"hint\">Le llevamos a la página de pago segura del banco (Redsys). La clínica no ve ni guarda los datos de su tarjeta.</p>");
            } else {
                body.push_str("<p class=\"err\">El pago por internet no está disponible ahora. Llame a la clínica, por favor.</p>");
            }
            false
        }
        (s, _) => {
            body.push_str(&format!("<p class=\"hint\">Estado: {}.</p>", s.label()));
            false
        }
    };
    let meta = if refresh { "<meta http-equiv=\"refresh\" content=\"10\">" } else { "" };
    format!(
        "<!doctype html><html lang=\"es\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"robots\" content=\"noindex\"><meta name=\"referrer\" content=\"no-referrer\">{meta}<title>Pago · {c}</title><style>body{{margin:0;font-family:system-ui,Arial,sans-serif;color:#1e293b;background:#f4f7fb}}.wrap{{max-width:560px;margin:0 auto;padding:28px 18px}}.card{{background:#fff;border:1px solid #e2e8f0;border-radius:20px;padding:22px}}h1{{font-size:21px;margin:0 0 14px}}.amount{{font-size:32px;font-weight:700;margin:6px 0 14px}}.hint{{color:#64748b}}.policy{{background:#fff6e6;border-radius:12px;padding:12px}}.ok{{color:#16a34a;font-weight:600}}.err{{color:#dc2626}}.wait{{color:#b45309}}.main{{width:100%;margin-top:12px;background:#0d9488;border:0;color:#fff;border-radius:14px;padding:14px;font-size:16px;font-weight:600;cursor:pointer}}</style></head><body><div class=\"wrap\"><div class=\"card\"><h1>{c}</h1>{body}</div></div></body></html>",
        c = e(clinic)
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const TEST_KEY: &str = "sq7HjrUOBfKmC576ILgskD5srU870gJ7";

    /// Los ejemplos de la documentación de Redsys (clave de pruebas pública).
    #[test]
    fn the_signature_matches_the_redsys_examples() {
        // «Firmar una operación»: clave de la operación y firma
        assert_eq!(order_key(TEST_KEY, "1234567890"), "RWt3/IPTzYRMXsQtkiGRKg==");
        let params = "eyJEU19NRVJDSEFOVF9BTU9VTlQiOiI5OTkiLCJEU19NRVJDSEFOVF9PUkRFUiI6IjEyMzQ1Njc4OTAiLCJEU19NRVJDSEFOVF9NRVJDSEFOVENPREUiOiI5OTkwMDg4ODEiLCJEU19NRVJDSEFOVF9DVVJSRU5DWSI6Ijk3OCIsIkRTX01FUkNIQU5UX1RSQU5TQUNUSU9OVFlQRSI6IjAiLCJEU19NRVJDSEFOVF9URVJNSU5BTCI6IjEiLCJEU19NRVJDSEFOVF9NRVJDSEFOVFVSTCI6Imh0dHA6XC9cL3d3dy5wcnVlYmEuY29tXC91cmxOb3RpZmljYWNpb24ucGhwIiwiRFNfTUVSQ0hBTlRfVVJMT0siOiJodHRwOlwvXC93d3cucHJ1ZWJhLmNvbVwvdXJsT0sucGhwIiwiRFNfTUVSQ0hBTlRfVVJMS08iOiJodHRwOlwvXC93d3cucHJ1ZWJhLmNvbVwvdXJsS08ucGhwIn0";
        assert_eq!(sign(TEST_KEY, "1234567890", params), "Vjo02eSWq249IeZZp3R-ArFnGLhKY0OuzDDlx1BuVtZDC2yhczA7_11uZhsYzLZBCMFAz8u8uzGDX3AErHKmmw");
        // «Realizar un pago»: el formulario del ejemplo
        let pay = "eyJEU19NRVJDSEFOVF9PUkRFUiI6IjAwMDE0MzM4ejc0NCIsIkRTX01FUkNIQU5UX01FUkNIQU5UQ09ERSI6Ijk5OTAwODg4MSIsIkRTX01FUkNIQU5UX1RFUk1JTkFMIjoiMDAxIiwiRFNfTUVSQ0hBTlRfQ1VSUkVOQ1kiOiI5NzgiLCJEU19NRVJDSEFOVF9UUkFOU0FDVElPTlRZUEUiOiIwIiwiRFNfTUVSQ0hBTlRfQU1PVU5UIjoiMjQ5IiwiRFNfTUVSQ0hBTlRfTUVSQ0hBTlRVUkwiOiJodHRwOi8vd3d3LnBydWViYS5jb20vdXJsTm90aWZpY2FjaW9uLnBocCIsIkRTX01FUkNIQU5UX1VSTE9LIjoiaHR0cDovL3d3dy5wcnVlYmEuY29tL3VybE9LLnBocCIsIkRTX01FUkNIQU5UX1VSTEtPIjoiaHR0cDovL3d3dy5wcnVlYmEuY29tL3VybEtPLnBocCJ9";
        assert_eq!(sign(TEST_KEY, "00014338z744", pay), "DfXW23aJNwzzLH2IoV5c9YiRKOf4cUgTbd4XtDIxDM_P-XEr79S3cQy_GZUl6P2Sf-udXwRGI0_NoLJ3ChIJkg");
        // y se verifica (con o sin relleno, en los dos alfabetos)
        assert!(verify(TEST_KEY, pay, "DfXW23aJNwzzLH2IoV5c9YiRKOf4cUgTbd4XtDIxDM_P-XEr79S3cQy_GZUl6P2Sf-udXwRGI0_NoLJ3ChIJkg==").is_ok());
        assert!(verify(TEST_KEY, pay, "DfXW23aJNwzzLH2IoV5c9YiRKOf4cUgTbd4XtDIxDM/P+XEr79S3cQy/GZUl6P2Sf+udXwRGI0/NoLJ3ChIJkg").is_ok());
        assert!(verify(TEST_KEY, pay, "DfXW23aJNwzzLH2IoV5c9YiRKOf4cUgTbd4XtDIxDM_P-XEr79S3cQy_GZUl6P2Sf-udXwRGI0_NoLJ3ChIJkh").is_err());
        assert!(verify("otraClaveDeOtroComercio00000000", pay, "DfXW23aJNwzzLH2IoV5c9YiRKOf4cUgTbd4XtDIxDM_P-XEr79S3cQy_GZUl6P2Sf-udXwRGI0_NoLJ3ChIJkg").is_err());
        assert_eq!(unb64("aG9sYQ"), Some(b"hola".to_vec()));
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
    }

    pub(crate) fn settings() -> TpvSettings {
        TpvSettings { env: Env::Pruebas, merchant: "999008881".into(), terminal: "1".into(), key: TEST_KEY.into(), endpoint: String::new() }
    }

    /// Lo que mandaría Redsys: parámetros y firma con la clave.
    pub(crate) fn redsys_says(s: &TpvSettings, order: &str, amount: i64, response: &str, extra: &[(&str, &str)]) -> (String, String) {
        let mut v = serde_json::json!({"Ds_Date": "14%2F11%2F2026", "Ds_Hour": "10%3A00", "Ds_Amount": amount.to_string(), "Ds_Currency": "978", "Ds_Order": order, "Ds_MerchantCode": s.merchant, "Ds_Terminal": "001", "Ds_Response": response, "Ds_TransactionType": "0", "Ds_AuthorisationCode": "372663"});
        for (k, x) in extra {
            v[*k] = (*x).into();
        }
        let p = encode_params(&v);
        let sig = sign(&s.key, order, &p);
        (p, sig)
    }

    #[test]
    fn the_pay_form_and_notices_are_signed_and_checked() {
        let s = settings();
        let f = pay_form(&s, "0042abcdefgh", 2500, Method::Bizum, "https://citas.clinica.es/api/tpv", "https://citas.clinica.es/pagar/x", "Señal").unwrap();
        assert_eq!(f.url, "https://sis-t.redsys.es:25443/sis/realizarPago");
        let m = verify(&s.key, &f.params, &f.signature).unwrap();
        assert_eq!((get(&m, "DS_MERCHANT_AMOUNT").as_deref(), get(&m, "DS_MERCHANT_PAYMETHODS").as_deref()), (Some("2500"), Some("z")));
        assert!(f.html().contains("realizarPago") && f.html().contains(SIGNATURE_VERSION));
        // la notificación buena
        let (p, sig) = redsys_says(&s, "0042abcdefgh", 2500, "0000", &[("Ds_ProcessedPayMethod", "68")]);
        let n = read_notice(&s, SIGNATURE_VERSION, &p, &sig).unwrap();
        assert!(n.paid() && n.bizum && n.amount == 2500 && n.order == "0042abcdefgh");
        // firma de otra clave, otro comercio, otra moneda, versión antigua
        let other = TpvSettings { key: "otraClaveDeOtroComercio00000000".into(), ..s.clone() };
        let (p2, sig2) = redsys_says(&other, "0042abcdefgh", 2500, "0000", &[]);
        assert!(read_notice(&s, SIGNATURE_VERSION, &p2, &sig2).unwrap_err().contains("firma"));
        let (p3, sig3) = redsys_says(&s, "0042abcdefgh", 2500, "0000", &[("Ds_MerchantCode", "111111111")]);
        assert!(read_notice(&s, SIGNATURE_VERSION, &p3, &sig3).unwrap_err().contains("otro comercio"));
        let (p4, sig4) = redsys_says(&s, "0042abcdefgh", 2500, "0000", &[("Ds_Currency", "840")]);
        assert!(read_notice(&s, SIGNATURE_VERSION, &p4, &sig4).unwrap_err().contains("euros"));
        assert!(read_notice(&s, "HMAC_SHA256_V1", &p, &sig).is_err());
        // el formulario de la notificación
        let form = parse_form(&format!("Ds_SignatureVersion={SIGNATURE_VERSION}&Ds_MerchantParameters={}&Ds_Signature={}", p, sig.replace('-', "%2D")));
        assert_eq!(form["Ds_Signature"], sig);
        assert!(new_order(7).starts_with("0007") && new_order(7).len() == 12 && new_order(7) != new_order(7));
        assert!(TpvSettings::default().problem().is_some() && s.problem().is_none());
    }

    pub(crate) fn clinic() -> (Db, u32, NaiveDateTime) {
        let mut db = Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 14).unwrap());
        db.config.pay_online = PayConfig { enabled: true, bizum: true, deposit: DepositPolicy { amount: 3000, refund_hours: 24 } };
        db.pay_requests.clear();
        let b = db.budgets.iter().find(|b| b.state == BudgetState::Pendiente && b.total() > 0.0).unwrap().id;
        (db, b, chrono::NaiveDate::from_ymd_opt(2026, 11, 14).unwrap().and_hms_opt(10, 0, 0).unwrap())
    }

    #[test]
    fn a_notice_is_charged_once_and_a_second_payment_is_flagged() {
        let (mut db, b, now) = clinic();
        let total = crate::cents::Cents::from_eur(db.budgets.iter().find(|x| x.id == b).unwrap().total()).0;
        // el importe lo pone el programa; a cuenta, menos y no más
        assert!(db.create_pay_request(PayFor::Budget(b), Some(total + 1), now).is_err());
        let id = db.create_pay_request(PayFor::Budget(b), Some(5000), now).unwrap();
        assert_eq!(db.create_pay_request(PayFor::Budget(b), None, now).unwrap(), id, "sin duplicados mientras está pendiente");
        let token = db.pay_request(id).unwrap().token.clone();
        assert!(db.pay_started(&token, "0001aaaaaaaa", Method::Tarjeta, now));
        assert!(db.pay_started(&token, "0001bbbbbbbb", Method::Bizum, now));
        let pays = db.payments.len();
        let ok = |order: &str, amount: i64| Notice { order: order.into(), amount, response: 0, auth: "123456".into(), bizum: false };
        // importe distinto: no se apunta
        assert!(matches!(db.apply_notice(&ok("0001aaaaaaaa", 4999), now).unwrap(), Applied::Review(_)));
        assert_eq!(db.payments.len(), pays);
        assert_eq!(db.apply_notice(&ok("0001aaaaaaaa", 5000), now).unwrap(), Applied::Duplicate, "el pedido ya tiene respuesta");
        // el otro intento sí: un cobro
        let Applied::Paid { payment_id } = db.apply_notice(&ok("0001bbbbbbbb", 5000), now).unwrap() else { panic!() };
        let p = db.payments.iter().find(|p| p.id == payment_id).unwrap();
        assert_eq!((p.amount, p.method.as_str()), (50.0, "Bizum"), "el intento era con Bizum");
        assert_eq!(db.apply_notice(&ok("0001bbbbbbbb", 5000), now).unwrap(), Applied::Duplicate);
        assert_eq!(db.payments.len(), pays + 1, "una notificación repetida no cobra dos veces");
        // pagado otra vez con otro pedido: se apunta y queda para devolver
        db.pay_requests.iter_mut().find(|r| r.id == id).unwrap().attempts.push(Attempt { order: "0001cccccccc".into(), method: Method::Tarjeta, at: now, response: None, note: String::new() });
        assert!(matches!(db.apply_notice(&ok("0001cccccccc", 5000), now).unwrap(), Applied::PaidTwice { .. }));
        assert!(db.pay_request(id).unwrap().refund_due);
        // un pedido que no es nuestro
        assert!(db.apply_notice(&ok("9999zzzzzzzz", 5000), now).is_err());
        // denegado: sigue pendiente y se puede reintentar
        let id2 = {
            let bid = db.budgets.iter().find(|x| x.id != b && x.state == BudgetState::Pendiente && x.total() > 0.0).unwrap().id;
            db.create_pay_request(PayFor::Budget(bid), None, now).unwrap()
        };
        let t2 = db.pay_request(id2).unwrap().token.clone();
        db.pay_started(&t2, "0002aaaaaaaa", Method::Tarjeta, now);
        assert_eq!(db.apply_notice(&Notice { response: 190, ..ok("0002aaaaaaaa", db.pay_request(id2).unwrap().amount) }, now).unwrap(), Applied::Denied(190));
        assert_eq!(db.pay_request(id2).unwrap().state, PayState::Pendiente);
    }

    #[test]
    fn an_attempt_without_notice_becomes_uncertain_and_is_settled_by_hand() {
        let (mut db, b, now) = clinic();
        let id = db.create_pay_request(PayFor::Budget(b), Some(2000), now).unwrap();
        let token = db.pay_request(id).unwrap().token.clone();
        db.pay_started(&token, "0003aaaaaaaa", Method::Tarjeta, now);
        assert!(db.pay_request(id).unwrap().uncertain(now + chrono::Duration::minutes(10)).is_empty());
        assert_eq!(db.pay_request(id).unwrap().uncertain(now + chrono::Duration::minutes(31)).len(), 1);
        assert!(db.cancel_pay_request(id).unwrap_err().contains("sin confirmar"));
        let pays = db.payments.len();
        assert!(db.settle_by_hand(id, "0003aaaaaaaa", true, now).unwrap().is_some());
        assert_eq!(db.payments.len(), pays + 1);
        assert!(db.settle_by_hand(id, "0003aaaaaaaa", true, now).is_err(), "no se resuelve dos veces");
        assert!(db.pay_request(id).unwrap().uncertain(now + chrono::Duration::hours(2)).is_empty());
    }

    #[test]
    fn installments_and_deposits_follow_their_rules() {
        let (mut db, b, now) = clinic();
        // cuota: el importe es el de la cuota y queda pagada
        let bud = db.budgets.iter_mut().find(|x| x.id == b).unwrap();
        bud.plan = Some(FinancePlan { down_payment: 0.0, installments: 3, first_date: now.date(), every_months: 1, paid: vec![false; 3], exact: true });
        let total = bud.total();
        let want = crate::cents::Cents::from_eur(bud.plan.as_ref().unwrap().installment(1, total)).0;
        let id = db.create_pay_request(PayFor::Installment(b, 1), Some(1), now).unwrap();
        assert_eq!(db.pay_request(id).unwrap().amount, want, "en una cuota no se elige el importe");
        let token = db.pay_request(id).unwrap().token.clone();
        db.pay_started(&token, "0004aaaaaaaa", Method::Tarjeta, now);
        db.apply_notice(&Notice { order: "0004aaaaaaaa".into(), amount: want, response: 0, auth: "1".into(), bizum: false }, now).unwrap();
        assert!(db.budgets.iter().find(|x| x.id == b).unwrap().plan.as_ref().unwrap().paid[1]);
        assert!(db.create_pay_request(PayFor::Installment(b, 1), None, now).is_err(), "una cuota pagada no se pide otra vez");
        // señal: el paciente anula a tiempo → se devuelve; tarde → se conserva
        let mut appt = |hours: i64| {
            let mut a = db.appointments.iter().find(|a| a.patient_id.is_some()).unwrap().clone();
            let start = now + chrono::Duration::hours(hours);
            (a.id, a.date, a.start, a.state) = (db.new_id(), start.date(), start.time(), ApptState::Confirmada);
            let aid = a.id;
            db.appointments.push(a);
            let id = db.create_pay_request(PayFor::Deposit(aid), None, now).unwrap();
            let t = db.pay_request(id).unwrap().token.clone();
            let order = format!("{:04}dddddddd", id % 10_000);
            db.pay_started(&t, &order, Method::Tarjeta, now);
            db.apply_notice(&Notice { order, amount: 3000, response: 0, auth: "1".into(), bizum: false }, now).unwrap();
            (aid, id)
        };
        let (early, r1) = appt(48);
        let (late, r2) = appt(5);
        let (clinic_cancel, r3) = appt(2);
        assert!(db.deposit_on_cancel(early, true, now).unwrap().contains("devolver"));
        assert!(db.pay_request(r1).unwrap().refund_due);
        assert!(db.deposit_on_cancel(late, true, now).unwrap().contains("conserva"));
        assert_eq!(db.pay_request(r2).unwrap().state, PayState::Retenido);
        assert!(db.deposit_on_cancel(clinic_cancel, false, now).unwrap().contains("devolver"), "si anula la clínica, siempre se devuelve");
        assert!(db.pay_request(r3).unwrap().refund_due);
        assert!(DepositPolicy { amount: 3000, refund_hours: 24 }.text().contains("24 horas"));
    }

    /// Redsys simulado: comprueba la firma de la devolución y responde firmado.
    fn fake_redsys(s: TpvSettings, answer: &'static str) -> (String, std::thread::JoinHandle<Vec<serde_json::Value>>) {
        let srv = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", srv.server_addr().to_ip().unwrap());
        let h = std::thread::spawn(move || {
            let mut seen = vec![];
            if let Ok(Some(mut req)) = srv.recv_timeout(std::time::Duration::from_secs(10)) {
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let v: serde_json::Value = serde_json::from_str(&body).unwrap();
                let m = verify(&s.key, v["Ds_MerchantParameters"].as_str().unwrap(), v["Ds_Signature"].as_str().unwrap()).expect("la devolución va firmada");
                seen.push(serde_json::Value::Object(m.clone()));
                let order = get(&m, "DS_MERCHANT_ORDER").unwrap();
                let amount: i64 = get(&m, "DS_MERCHANT_AMOUNT").unwrap().parse().unwrap();
                let reply = match answer {
                    "error" => serde_json::json!({"errorCode": "SIS0057"}).to_string(),
                    "cut" => return seen,
                    code => {
                        let (p, sig) = redsys_says(&s, &order, amount, code, &[("Ds_TransactionType", "3")]);
                        serde_json::json!({"Ds_SignatureVersion": SIGNATURE_VERSION, "Ds_MerchantParameters": p, "Ds_Signature": sig}).to_string()
                    }
                };
                let _ = req.respond(tiny_http::Response::from_string(reply));
            }
            seen
        });
        (url, h)
    }

    #[test]
    fn refunds_are_signed_recorded_once_and_uncertain_ones_are_not_repeated() {
        let (mut db, b, now) = clinic();
        let id = db.create_pay_request(PayFor::Budget(b), Some(4000), now).unwrap();
        let token = db.pay_request(id).unwrap().token.clone();
        db.pay_started(&token, "0005aaaaaaaa", Method::Tarjeta, now);
        db.apply_notice(&Notice { order: "0005aaaaaaaa".into(), amount: 4000, response: 0, auth: "1".into(), bizum: false }, now).unwrap();
        // hecha
        let (url, h) = fake_redsys(settings(), "0900");
        let s = TpvSettings { endpoint: url, ..settings() };
        let out = send_refund(&s, "0005aaaaaaaa", 1500);
        let seen = h.join().unwrap();
        assert_eq!(get(seen[0].as_object().unwrap(), "DS_MERCHANT_TRANSACTIONTYPE").as_deref(), Some("3"));
        assert!(matches!(out, RefundOutcome::Done { .. }), "{out:?}");
        let pays = db.payments.len();
        db.record_refund(id, "0005aaaaaaaa", 1500, &out).unwrap();
        assert_eq!(db.payments.len(), pays + 1);
        assert_eq!(db.payments.last().unwrap().amount, -15.0);
        assert_eq!(db.refundable(id, "0005aaaaaaaa").unwrap(), 2500);
        // rechazada por Redsys
        let (url, h) = fake_redsys(settings(), "error");
        let out = send_refund(&TpvSettings { endpoint: url, ..settings() }, "0005aaaaaaaa", 2500);
        h.join().unwrap();
        assert_eq!(out, RefundOutcome::Denied("SIS0057".into()));
        // cortada tras enviarla: incierta, y no se repite sin comprobar
        let (url, h) = fake_redsys(settings(), "cut");
        let out = send_refund(&TpvSettings { endpoint: url, ..settings() }, "0005aaaaaaaa", 2500);
        h.join().unwrap();
        assert!(matches!(out, RefundOutcome::Uncertain(_)), "{out:?}");
        assert!(db.record_refund(id, "0005aaaaaaaa", 2500, &out).is_err());
        assert!(db.refundable(id, "0005aaaaaaaa").unwrap_err().contains("portal"));
        db.settle_refund_by_hand(id, "0005aaaaaaaa", 2500, true).unwrap();
        assert_eq!(db.pay_request(id).unwrap().state, PayState::Devuelto);
        assert!(db.refundable(id, "0005aaaaaaaa").is_err(), "ya está devuelto");
        // sin conexión: no ha salido
        let out = send_refund(&TpvSettings { endpoint: "http://127.0.0.1:9".into(), ..settings() }, "0005aaaaaaaa", 100);
        assert!(matches!(out, RefundOutcome::NotSent(_)), "{out:?}");
    }
}
