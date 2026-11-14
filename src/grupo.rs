//! Grupos de clínicas (T-F6-01, D-25).
//!
//! - **Cada sociedad, su instalación.** Cada sociedad (cada NIF) tiene sus
//!   propios datos y su servidor: sus pacientes, su numeración de facturas,
//!   su cadena VERI*FACTU, sus cajas y sus copias. Entre sociedades no pasa
//!   nada personal: cada una es responsable de los datos de sus pacientes.
//! - **Dentro de una sociedad, varios centros.** Comparten pacientes, historia
//!   y saldo. Un usuario puede trabajar solo en unos centros (`User::centers`,
//!   ver `vista`).
//! - **La vista del grupo** junta el resumen de cada clínica ([`Summary`]):
//!   solo cifras por centro, sin ningún dato de pacientes. Cada clínica lo da
//!   con una clave de grupo ([`GroupKey`]) que crea y retira su Dirección.
//!   Esa clave solo sirve para pedir el resumen, no abre nada más. Va por la
//!   misma conexión cifrada que los puestos y solo con la huella del servidor
//!   ya conocida.

use crate::cents::Cents;
use crate::informes::Filter;
use crate::model::*;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

/// Cifras de un centro en el periodo (importes en céntimos).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Figures {
    /// Tratamientos realizados y su producción (paciente y mutua).
    pub acts: usize,
    pub production: i64,
    /// Cobrado neto (cobros menos anulaciones y devoluciones).
    pub collected: i64,
    /// Citas (sin anuladas ni reprogramadas), a las que acudió y a las que no.
    pub appointments: usize,
    pub attended: usize,
    pub no_shows: usize,
}

impl Figures {
    fn add(&mut self, o: &Figures) {
        self.acts += o.acts;
        self.production += o.production;
        self.collected += o.collected;
        self.appointments += o.appointments;
        self.attended += o.attended;
        self.no_shows += o.no_shows;
    }
}

/// Resumen de una clínica (una sociedad) para el grupo: sin datos de pacientes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub clinic: String,
    /// NIF de la sociedad (no es un dato de pacientes).
    pub nif: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Por centro: (nombre, cifras).
    pub centers: Vec<(String, Figures)>,
    /// Facturado neto (las facturas son de la sociedad, no de un centro).
    pub invoiced: i64,
    /// Pacientes dados de alta en el periodo.
    pub new_patients: usize,
}

impl Summary {
    pub fn totals(&self) -> Figures {
        let mut t = Figures::default();
        for (_, f) in &self.centers {
            t.add(f);
        }
        t
    }
}

/// Totales del grupo: (cifras de todos los centros, facturado, altas).
pub fn consolidate(list: &[Summary]) -> (Figures, i64, usize) {
    let mut t = Figures::default();
    let (mut invoiced, mut new_patients) = (0, 0);
    for s in list {
        t.add(&s.totals());
        invoiced += s.invoiced;
        new_patients += s.new_patients;
    }
    (t, invoiced, new_patients)
}

/// Lo más largo que se puede pedir de una vez.
pub const MAX_DAYS: i64 = 400;

impl Db {
    /// El resumen de esta clínica para el grupo, con los mismos cálculos que
    /// los informes (`informes`).
    pub fn group_summary(&self, from: NaiveDate, to: NaiveDate) -> Summary {
        // sin centros dados de alta, el principal
        let ids: Vec<(u32, String)> = if self.centers.is_empty() { vec![(1, self.config.name.clone())] } else { self.centers.iter().map(|c| (c.id, c.name.clone())).collect() };
        let figures = |center: u32| {
            let f = Filter { from, to, center: Some(center), doctor: None };
            let p = self.report_production(&f);
            let c = self.report_collected(&f);
            let appts: Vec<&Appointment> = self.appointments.iter().filter(|a| f.day(a.date) && cid(a.center_id) == cid(center) && !matches!(a.state, ApptState::Anulada | ApptState::Reprogramada)).collect();
            Figures {
                acts: p.acts,
                production: p.total().0,
                collected: c.net.0,
                appointments: appts.len(),
                attended: appts.iter().filter(|a| a.state == ApptState::Finalizada).count(),
                no_shows: appts.iter().filter(|a| a.state == ApptState::NoAcude).count(),
            }
        };
        Summary {
            clinic: self.config.name.clone(),
            nif: self.config.nif.clone(),
            from,
            to,
            centers: ids.into_iter().map(|(id, name)| (name, figures(id))).collect(),
            invoiced: self.report_invoiced(&Filter::period(from, to)).net.0,
            new_patients: self.patients.iter().filter(|p| p.registered.is_some_and(|d| d >= from && d <= to)).count(),
        }
    }
}

// ───────────────────────── claves de grupo ─────────────────────────

/// Clave con la que el grupo pide el resumen de esta clínica. Se guarda solo
/// su huella; la clave se enseña una vez, al crearla.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupKey {
    pub id: u32,
    /// Para quién es (el grupo, la gestoría del grupo…).
    pub name: String,
    pub hash: String,
    pub created: NaiveDateTime,
    pub by: String,
    #[serde(default)]
    pub revoked: Option<NaiveDateTime>,
}

fn token_hash(token: &str) -> String {
    sha256_hex(&format!("veodardental-grupo:{}", token.trim()))
}

/// La clave que vale (no retirada), si alguna.
pub fn valid_key<'a>(keys: impl IntoIterator<Item = &'a GroupKey>, token: &str) -> Option<&'a GroupKey> {
    let token = token.trim();
    if token.len() < 32 {
        return None;
    }
    let h = token_hash(token);
    keys.into_iter().find(|k| k.revoked.is_none() && k.hash.len() == h.len() && k.hash.bytes().zip(h.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0)
}

impl Db {
    /// Clave nueva para el grupo: devuelve la clave (que no se guarda).
    pub fn group_key_new(&mut self, name: &str) -> Result<(u32, String), String> {
        use ring::rand::SecureRandom;
        let name = name.trim();
        if name.is_empty() {
            return Err("Diga para quién es la clave.".into());
        }
        let mut b = [0u8; 32];
        ring::rand::SystemRandom::new().fill(&mut b).map_err(|_| "No se pudo crear la clave.".to_string())?;
        let token: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let id = self.new_id();
        let by = self.config.current_user.clone();
        self.group_keys.push(GroupKey { id, name: name.into(), hash: token_hash(&token), created: now(), by, revoked: None });
        self.audit("Clave de grupo creada", name);
        Ok((id, token))
    }

    pub fn group_key_revoke(&mut self, id: u32) -> Result<(), String> {
        let k = self.group_keys.iter_mut().find(|k| k.id == id && k.revoked.is_none()).ok_or("Esa clave ya no está en uso.")?;
        k.revoked = Some(now());
        let name = k.name.clone();
        self.audit("Clave de grupo retirada", &name);
        Ok(())
    }
}

// ───────────────── enlaces (en el ordenador de quien ve el grupo) ─────────────────

/// Una clínica del grupo: dónde está su servidor, su huella y la clave de grupo.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub name: String,
    pub address: String,
    /// Clave pública del servidor (hexadecimal): sin ella no se conecta.
    pub pin: String,
    pub token: String,
}

const PREFIX: &str = "veodardental-grupo:";

/// Lo que la clínica da al grupo, en una línea para copiar y pegar.
pub fn connection_text(address: &str, pin: &str, token: &str) -> String {
    format!("{PREFIX}{}#{}#{}", address.trim(), pin.trim(), token.trim())
}

/// Lee lo que dio la clínica.
pub fn parse_connection(name: &str, text: &str) -> Result<Link, String> {
    let rest = text.trim().strip_prefix(PREFIX).ok_or("No son datos de conexión de un grupo (empiezan por «veodardental-grupo:»).")?;
    let parts: Vec<&str> = rest.split('#').collect();
    let [address, pin, token] = parts.as_slice() else { return Err("Faltan datos: dirección, huella y clave.".into()) };
    if address.trim().is_empty() {
        return Err("Falta la dirección del servidor.".into());
    }
    if pin.len() != 64 || !pin.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("La huella del servidor no es válida.".into());
    }
    if token.len() < 32 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("La clave de grupo no es válida.".into());
    }
    Ok(Link { name: if name.trim().is_empty() { address.trim().to_string() } else { name.trim().to_string() }, address: address.trim().into(), pin: pin.to_lowercase(), token: token.to_lowercase() })
}

fn links_path() -> std::path::PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("grupo.json")
}

/// Clínicas del grupo guardadas en este ordenador.
pub fn load_links() -> Vec<Link> {
    if cfg!(test) {
        return vec![];
    }
    std::fs::read_to_string(links_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_links(links: &[Link]) -> Result<(), String> {
    if cfg!(test) {
        return Ok(());
    }
    let p = links_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    // llevan las claves de grupo: solo para el usuario de este ordenador
    crate::paths::write_private(&p, serde_json::to_string_pretty(links).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())
}

/// Un periodo que se puede pedir.
pub fn check_period(from: NaiveDate, to: NaiveDate) -> Result<(), String> {
    if from > to {
        return Err("La fecha de inicio es posterior a la de fin.".into());
    }
    if (to - from).num_days() > MAX_DAYS {
        return Err(format!("Como mucho {MAX_DAYS} días de una vez."));
    }
    Ok(())
}

/// Importe para mostrar.
pub fn money(c: i64) -> String {
    Cents(c).to_string()
}

/// Los campos que puede llevar un resumen, y ninguno más (para las pruebas).
#[cfg(test)]
pub fn only_known_fields(json: &str) -> Result<(), String> {
    const KNOWN: [&str; 13] = ["clinic", "nif", "from", "to", "centers", "invoiced", "new_patients", "acts", "production", "collected", "appointments", "attended", "no_shows"];
    fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    out.push(k.clone());
                    walk(x, out);
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut keys = vec![];
    walk(&serde_json::from_str(json).map_err(|e| e.to_string())?, &mut keys);
    match keys.into_iter().find(|k| !KNOWN.contains(&k.as_str())) {
        Some(k) => Err(format!("campo «{k}» en el resumen")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    /// Dos centros: el resumen da por centro lo mismo que los informes, y no
    /// lleva ningún dato de pacientes.
    #[test]
    fn the_summary_matches_the_reports_and_carries_no_patient_data() {
        let mut db = Db::sample(d(11, 4));
        if !db.centers.iter().any(|c| c.id == 2) {
            db.centers.push(Center { id: 2, name: "Centro Norte".into(), address: String::new(), phone: String::new(), color: [1, 2, 3], active: true });
        }
        for (i, a) in db.appointments.iter_mut().enumerate() {
            a.center_id = if i % 3 == 0 { 2 } else { 1 };
        }
        for (i, p) in db.payments.iter_mut().enumerate() {
            p.center_id = if i % 2 == 0 { 2 } else { 1 };
        }
        for (i, e) in db.entries.iter_mut().enumerate() {
            e.center_id = if i % 4 == 0 { 2 } else { 1 };
        }
        let (from, to) = (d(1, 1), d(12, 31));
        let s = db.group_summary(from, to);
        assert_eq!(s.centers.len(), db.centers.len());
        for (cid, (name, f)) in db.centers.iter().map(|c| c.id).zip(&s.centers) {
            let fl = Filter { from, to, center: Some(cid), doctor: None };
            assert_eq!(f.production, db.report_production(&fl).total().0, "{name}");
            assert_eq!(f.collected, db.report_collected(&fl).net.0, "{name}");
            assert_eq!(f.acts, db.report_acts(&fl).len());
        }
        let all = Filter::period(from, to);
        assert_eq!(s.totals().production, db.report_production(&all).total().0, "los centros suman el total");
        assert_eq!(s.totals().collected, db.report_collected(&all).net.0);
        assert!(s.totals().appointments > 0 && s.totals().production > 0);
        // sin nada de los pacientes: ni nombres, ni documentos, ni teléfonos, ni correos, ni números de historia
        let json = serde_json::to_string(&s).unwrap();
        for p in &db.patients {
            for v in [p.full_name(), p.nif.clone(), p.mobile.clone(), p.email.clone()] {
                assert!(v.trim().len() < 4 || !json.contains(v.trim()), "«{v}» en el resumen");
            }
        }
        only_known_fields(&json).unwrap();
    }

    #[test]
    fn group_keys_are_stored_as_hashes_and_can_be_revoked() {
        let mut db = Db::sample(d(11, 4));
        let (id, token) = db.group_key_new("Grupo Dental Norte").unwrap();
        assert_eq!(token.len(), 64);
        let json = serde_json::to_string(&db.group_keys.to_vec()).unwrap();
        assert!(!json.contains(&token), "la clave no se guarda");
        assert_eq!(valid_key(db.group_keys.iter(), &token).map(|k| k.id), Some(id));
        assert!(valid_key(db.group_keys.iter(), &token[..63]).is_none());
        assert!(valid_key(db.group_keys.iter(), "").is_none());
        let (_, other) = db.group_key_new("Otro").unwrap();
        assert_ne!(token, other);
        db.group_key_revoke(id).unwrap();
        assert!(valid_key(db.group_keys.iter(), &token).is_none(), "retirada, ya no vale");
        assert!(valid_key(db.group_keys.iter(), &other).is_some());
        assert!(db.group_key_revoke(id).is_err());
        assert!(db.group_key_new("  ").is_err());
        assert!(db.audit_log.iter().any(|a| a.action == "Clave de grupo retirada"));
    }

    #[test]
    fn the_connection_text_round_trips_and_needs_the_server_fingerprint() {
        let pin = "ab".repeat(32);
        let token = "cd".repeat(32);
        let t = connection_text("192.168.1.20:7430", &pin, &token);
        let l = parse_connection("Clínica Norte", &t).unwrap();
        assert_eq!((l.address.as_str(), l.pin.as_str(), l.token.as_str(), l.name.as_str()), ("192.168.1.20:7430", pin.as_str(), token.as_str(), "Clínica Norte"));
        assert!(parse_connection("", &connection_text("192.168.1.20:7430", "", &token)).is_err(), "sin huella, no");
        assert!(parse_connection("", &connection_text("192.168.1.20:7430", &pin, "corta")).is_err());
        assert!(parse_connection("", "otra cosa").is_err());
        assert!(check_period(d(1, 1), d(12, 31)).is_ok());
        assert!(check_period(d(12, 31), d(1, 1)).is_err());
        assert!(check_period(d(1, 1), d(1, 1) + chrono::Duration::days(MAX_DAYS + 1)).is_err());
    }
}
