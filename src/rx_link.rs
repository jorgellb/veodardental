//! «Mis recetas»: enlace personal para que el paciente vea por internet sus
//! recetas electrónicas, en qué estado están y la hoja con los códigos para
//! enseñarla en la farmacia desde el móvil (control SC03 de la certificación,
//! acceso del paciente al sistema de receta electrónica).
//!
//! Como el DNI electrónico no está generalizado, el control admite un
//! mecanismo alternativo con medidas compensatorias: el enlace secreto, que
//! solo recibe el paciente, más su DNI/NIE y su fecha de nacimiento. La página
//! no muestra nada antes de comprobarlos, lo publicado solo guarda una huella
//! de esos datos (nunca el dato) y tras 5 intentos fallidos el enlace se
//! bloquea media hora.

use crate::model::*;
use crate::srep::RPrescripcion;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RxLink {
    pub token: String,
    pub patient_id: u32,
    pub created: Option<NaiveDateTime>,
    /// A quién se envió por correo.
    pub sent_to: String,
    /// Último acceso correcto del paciente, y cuántos lleva.
    pub viewed: Option<NaiveDateTime>,
    pub accesses: u32,
}

pub fn link_url(base: &str, token: &str) -> String {
    format!("{}/recetas/{token}", base.trim().trim_end_matches('/'))
}

/// DNI/NIE sin espacios, puntos ni guiones, en mayúsculas.
pub fn norm_id(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase()
}

/// Fecha escrita por el paciente: «31/12/1980», «31-12-1980» o «1980-12-31».
pub fn parse_birth(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    ["%d/%m/%Y", "%d-%m-%Y", "%Y-%m-%d", "%d.%m.%Y"].iter().find_map(|f| NaiveDate::parse_from_str(s, f).ok())
}

/// Huella de los datos que el paciente tiene que escribir (con el código del
/// enlace, para que no sirva en otro enlace).
pub fn verifier(token: &str, id: &str, birth: NaiveDate) -> String {
    Sha256::digest(format!("{token}:{}:{}", norm_id(id), birth.format("%Y-%m-%d")).as_bytes()).iter().map(|x| format!("{x:02x}")).collect()
}

/// ¿Puede entrar este paciente? (hace falta su DNI/NIE y su fecha de nacimiento en la ficha)
pub fn can_verify(p: &Patient) -> bool {
    norm_id(&p.nif).len() >= 5 && p.birth.is_some()
}

impl Db {
    pub fn rx_link(&self, patient_id: u32) -> Option<&RxLink> {
        self.rx_links.iter().find(|l| l.patient_id == patient_id)
    }

    /// Código del enlace «Mis recetas» del paciente (se crea la primera vez).
    pub fn rx_token(&mut self, patient_id: u32) -> String {
        if let Some(l) = self.rx_link(patient_id) {
            return l.token.clone();
        }
        let token = crate::portal::new_token();
        self.rx_links.push(RxLink { token: token.clone(), patient_id, created: Some(now()), ..Default::default() });
        token
    }

    /// El paciente ha entrado a ver sus recetas.
    pub fn rx_link_used(&mut self, patient_id: u32, when: NaiveDateTime) -> bool {
        let Some(l) = self.rx_links.iter_mut().find(|l| l.patient_id == patient_id) else { return false };
        l.viewed = Some(when);
        l.accesses += 1;
        let name = self.patient_name(patient_id);
        self.audit("Acceso del paciente a sus recetas por internet", &name);
        true
    }
}

// ───────────────────────────── lo que ve la página ─────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct PubRxIssue {
    pub date: NaiveDate,
    pub doctor: String,
    pub test: bool,
    pub confidential: bool,
    pub signer: String,
    pub issued: Vec<RPrescripcion>,
    /// Último estado conocido de cada receta en el repositorio.
    pub status: BTreeMap<String, u8>,
    pub checked_at: Option<NaiveDateTime>,
}

#[derive(Clone, Debug, Default)]
pub struct PubRx {
    pub patient_id: u32,
    pub first_name: String,
    /// Huella del DNI/NIE y la fecha de nacimiento (vacía si faltan en la ficha).
    pub check: String,
    /// Recetas emitidas, de la más reciente a la más antigua.
    pub issues: Vec<PubRxIssue>,
    /// idRepositorio, idEntidad e idSistema para la hoja.
    pub ids: (String, String, String),
}

/// Recetas que se publican para cada enlace: las emitidas en el último año.
pub fn publish_rx(db: &Db, today: NaiveDate) -> HashMap<String, PubRx> {
    let mut m = HashMap::new();
    let e = &db.erx;
    let ids = (e.id_repositorio_or_test(), e.id_entidad_or_test(), if e.id_sistema.trim().is_empty() { "PRUEBAS".to_string() } else { e.id_sistema.trim().to_string() });
    for l in &db.rx_links {
        let Some(p) = db.patient(l.patient_id) else { continue };
        let mut issues: Vec<PubRxIssue> = db
            .prescriptions
            .iter()
            .filter(|r| r.patient_id == p.id && (today - r.date).num_days() <= 366)
            .filter_map(|r| {
                let s = r.srep.as_ref()?;
                Some(PubRxIssue {
                    date: r.date,
                    doctor: db.doctor(r.doctor_id).map(|d| d.name.clone()).unwrap_or_default(),
                    test: s.test,
                    confidential: s.confidential,
                    signer: s.signer.clone(),
                    issued: s.issued.clone(),
                    status: s.status.clone(),
                    checked_at: s.checked_at,
                })
            })
            .collect();
        issues.sort_by_key(|i| std::cmp::Reverse(i.date));
        let check = match p.birth {
            Some(b) if can_verify(p) => verifier(&l.token, &p.nif, b),
            _ => String::new(),
        };
        m.insert(l.token.clone(), PubRx { patient_id: p.id, first_name: p.first_name.clone(), check, issues, ids: ids.clone() });
    }
    m
}

/// Lo que se le enseña al paciente una vez comprobados sus datos.
pub fn rx_json(p: &crate::booking::Published, d: &PubRx, today: NaiveDate) -> serde_json::Value {
    use serde_json::json;
    let recetas: Vec<serde_json::Value> = d
        .issues
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let meds: Vec<serde_json::Value> = s
                .issued
                .iter()
                .map(|x| {
                    let recs: Vec<serde_json::Value> = x
                        .recetas
                        .iter()
                        .map(|r| {
                            let st = if x.anulada || r.anulada { 11 } else { s.status.get(&r.id_receta).copied().unwrap_or_else(|| r.estado_en(today)) };
                            json!({"desde": fmt_date(r.fecha_ini), "hasta": fmt_date(r.fecha_fin), "envases": r.num_envases, "estado": st, "estado_texto": crate::srep::estado_texto(st)})
                        })
                        .collect();
                    json!({"nombre": x.producto.denominacion, "posologia": crate::srep_doc::posologia(x), "recetas": recs})
                })
                .collect();
            json!({"i": i, "fecha": fmt_date(s.date), "profesional": s.doctor, "confidencial": s.confidential, "pruebas": s.test, "medicamentos": meds})
        })
        .collect();
    let updated = d.issues.iter().filter_map(|s| s.checked_at).max();
    json!({
        "ok": true, "nombre": d.first_name, "clinica": p.clinic, "telefono": p.phone,
        "recetas": recetas,
        "actualizado": updated.map(|t| format!("{} a las {}", fmt_date(t.date()), t.format("%H:%M"))),
    })
}

/// Hoja de información de una de sus recetas, con los códigos para la farmacia.
pub fn rx_sheet(p: &crate::booking::Published, d: &PubRx, i: usize) -> Option<String> {
    let s = d.issues.get(i)?;
    let h =
        crate::srep_doc::HipHeader { clinic: &p.clinic, clinic_address: &p.address, clinic_phone: &p.phone, id_repositorio: &d.ids.0, id_entidad: &d.ids.1, id_sistema: &d.ids.2, test_mode: s.test, confidential: s.confidential, signer: &s.signer };
    Some(crate::srep_doc::hip_html(&h, &s.issued))
}

/// Lo que escribe el paciente para entrar.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct RxLogin {
    pub dni: String,
    pub nacimiento: String,
}

impl RxLogin {
    /// ¿Coinciden con los de la ficha?
    pub fn matches(&self, token: &str, d: &PubRx) -> bool {
        if d.check.is_empty() {
            return false;
        }
        parse_birth(&self.nacimiento).is_some_and(|b| verifier(token, &self.dni, b) == d.check)
    }
}

/// Una receta electrónica emitida de ejemplo (dos medicamentos, uno ya
/// dispensado en parte) para las pruebas y la página de demostración.
#[cfg(test)]
pub fn sample_issue(db: &mut Db, patient_id: u32, today: NaiveDate) -> u32 {
    use crate::srep::{RProducto, RReceta};
    let rid = db.new_id();
    let rec = |id: &str, ini: NaiveDate, n: u32| RReceta { id_receta: id.into(), fecha_ini: ini, fecha_fin: ini + chrono::Duration::days(10), num_envases: n, estado: 1, ..Default::default() };
    let med = |name: &str, recs: Vec<RReceta>| RPrescripcion {
        id_prescripcion: crate::srep::new_id32(),
        producto: RProducto { denominacion: name.into(), ..Default::default() },
        toma: 1.0,
        ud_toma: "comprimido".into(),
        frecuencia: 3.0,
        ud_frecuencia: "día".into(),
        duracion: 7,
        ud_duracion: "días".into(),
        recetas: recs,
        ..Default::default()
    };
    let a = med("AMOXICILINA 750 MG 30 COMPRIMIDOS", vec![rec("ra1", today, 1)]);
    let b = med("IBUPROFENO 600 MG 40 COMPRIMIDOS", vec![rec("rb1", today, 2), rec("rb2", today + chrono::Duration::days(30), 1)]);
    let mut status = BTreeMap::new();
    status.insert("rb1".to_string(), 8u8);
    let issue = crate::finance::SrepIssue { id_acceso: crate::srep::new_id32(), issued: vec![a, b], issued_at: Some(now()), signer: "Dra. Ruiz".into(), test: true, confidential: false, status, checked_at: Some(now()), events: vec![] };
    db.prescriptions.push(crate::finance::Prescription {
        id: rid,
        patient_id,
        doctor_id: 1,
        date: today,
        diagnosis: "Infección tras extracción".into(),
        items: vec![],
        notes: String::new(),
        erx_code: String::new(),
        erx_platform: String::new(),
        erx_date: None,
        srep: Some(issue),
        reviewed_warnings: vec![],
        reviewed_by: String::new(),
        reviewed_at: None,
    });
    rid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_checked_without_publishing_it() {
        let b = NaiveDate::from_ymd_opt(1985, 2, 7).unwrap();
        assert_eq!(norm_id(" 12.345.678-z "), "12345678Z");
        for s in ["07/02/1985", "07-02-1985", "1985-02-07", "7/2/1985"] {
            assert_eq!(parse_birth(s), Some(b), "{s}");
        }
        assert!(parse_birth("1985").is_none());
        let v = verifier("t1", "12345678Z", b);
        assert_eq!(v, verifier("t1", "12.345.678-z", b));
        assert_ne!(v, verifier("t2", "12345678Z", b), "la huella depende del enlace");
        assert_ne!(v, verifier("t1", "12345678Z", NaiveDate::from_ymd_opt(1985, 2, 8).unwrap()));
        assert!(!v.contains("12345678"));
        let d = PubRx { check: v, ..Default::default() };
        assert!(RxLogin { dni: "12345678z".into(), nacimiento: "07/02/1985".into() }.matches("t1", &d));
        assert!(!RxLogin { dni: "12345678Z".into(), nacimiento: "08/02/1985".into() }.matches("t1", &d));
        assert!(!RxLogin { dni: "".into(), nacimiento: "".into() }.matches("t1", &PubRx::default()), "sin datos en la ficha no se entra");
    }

    #[test]
    fn only_issued_prescriptions_of_the_linked_patient_are_published() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let mut db = Db::sample(today);
        db.rx_links.clear();
        let pid = db.patients[0].id;
        db.patients[0].nif = "12345678Z".into();
        db.patients[0].birth = NaiveDate::from_ymd_opt(1985, 2, 7);
        let rid = db.new_id();
        let item = RPrescripcion {
            producto: crate::srep::RProducto { denominacion: "AMOXICILINA 750 MG".into(), ..Default::default() },
            recetas: vec![crate::srep::RReceta { id_receta: "r1".into(), fecha_ini: today, fecha_fin: today + chrono::Duration::days(10), num_envases: 1, estado: 1, ..Default::default() }],
            ..Default::default()
        };
        db.prescriptions.push(crate::finance::Prescription {
            id: rid,
            patient_id: pid,
            doctor_id: 1,
            date: today,
            diagnosis: String::new(),
            items: vec![],
            notes: String::new(),
            erx_code: String::new(),
            erx_platform: String::new(),
            erx_date: None,
            srep: Some(crate::finance::SrepIssue { issued: vec![item], test: true, ..Default::default() }),
            reviewed_warnings: vec![],
            reviewed_by: String::new(),
            reviewed_at: None,
        });
        assert!(publish_rx(&db, today).is_empty(), "sin enlace no se publica nada");
        let t = db.rx_token(pid);
        assert_eq!(db.rx_token(pid), t, "un enlace por paciente");
        let m = publish_rx(&db, today);
        let d = &m[&t];
        assert_eq!(d.issues.len(), 1);
        assert!(RxLogin { dni: "12345678Z".into(), nacimiento: "07/02/1985".into() }.matches(&t, d));
        let p = crate::booking::publish(&db, today);
        let v = rx_json(&p, d, today);
        assert_eq!(v["recetas"][0]["medicamentos"][0]["nombre"], "AMOXICILINA 750 MG");
        assert_eq!(v["recetas"][0]["medicamentos"][0]["recetas"][0]["estado_texto"], "Dispensable");
        assert!(rx_sheet(&p, d, 0).unwrap().contains("<svg"));
        assert!(rx_sheet(&p, d, 5).is_none());
        // sin DNI en la ficha no se puede entrar
        db.patients[0].nif.clear();
        assert!(publish_rx(&db, today)[&t].check.is_empty());
        assert!(db.rx_link_used(pid, now()));
        assert_eq!(db.rx_link(pid).unwrap().accesses, 1);
    }
}
