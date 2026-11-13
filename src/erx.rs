//! Receta médica privada electrónica (Sistema de Receta Electrónica Privada).
//!
//! Una clínica privada no se conecta al sistema de receta de su comunidad
//! autónoma (es solo para la sanidad pública): prescribe a través de un
//! sistema de prescripción homologado por los Consejos Generales, que valida
//! al colegiado, firma la receta y la hace dispensable en cualquier farmacia
//! (Nodofarma), devolviendo un código de verificación (CVE).
//!
//! Este módulo prepara esa conexión: comprueba los datos obligatorios de la
//! receta, genera el texto para pasarlo a la plataforma, guarda el CVE
//! devuelto y lleva la lista de requisitos de cada dentista.

use crate::finance::Prescription;
use crate::model::*;
use crate::srep::{RPaciente, RPrescripcion, RPrescriptor, RProducto, RReceta};
use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ErxDoctor {
    pub doctor_id: u32,
    /// Tiene certificado cualificado de firma (o identificación por vídeo en la plataforma).
    pub certificate: bool,
    /// Está dado de alta en la plataforma de prescripción.
    pub registered: bool,
    /// Correo del prescriptor (obligatorio en la receta privada; si falta, el de la clínica).
    #[serde(default)]
    pub email: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ErxConfig {
    /// Plataforma de prescripción homologada que usa la clínica.
    pub platform: String,
    /// Dirección web donde se emiten las recetas.
    pub url: String,
    pub doctors: Vec<ErxDoctor>,
    // ── conexión directa: este programa como sistema de prescripción (SREP) ──
    /// Dirección del repositorio de recetas (servicio `--repositorio-recetas`).
    #[serde(default)]
    pub repo_addr: String,
    #[serde(default)]
    pub repo_key: String,
    /// Identificadores que asigna el CGCOM al certificar el sistema y el repositorio.
    #[serde(default)]
    pub id_sistema: String,
    #[serde(default)]
    pub id_entidad: String,
    #[serde(default)]
    pub id_repositorio: String,
    /// Nombre público con el que Nodofarma llega al repositorio (detrás del proxy TLS).
    #[serde(default)]
    pub public_host: String,
    /// Emitir las recetas oficiales por la conexión directa.
    #[serde(default)]
    pub direct: bool,
    /// Identificador de acceso de cada paciente en el repositorio (el mismo en todas sus recetas).
    #[serde(default)]
    pub access_ids: BTreeMap<u32, String>,
    /// Día en que se envió a los Consejos la solicitud de certificación.
    #[serde(default)]
    pub cert_requested: Option<NaiveDate>,
    /// Para firmar recetas hay que haber entrado con el código del móvil (control SC01).
    #[serde(default)]
    pub require_2fa: bool,
}

/// ¿Puede firmar recetas esta sesión? Con `require_2fa`, solo si se entró con el
/// código de la app del móvil además de la contraseña.
pub fn second_factor_ok(cfg: &ErxConfig, session_2fa: bool) -> Result<(), String> {
    if cfg.require_2fa && !session_2fa {
        return Err("Para firmar recetas hay que entrar con la contraseña y el código del móvil. Actívelo en Configuración › Usuarios y permisos y vuelva a entrar.".into());
    }
    Ok(())
}

impl ErxConfig {
    /// Certificado el sistema: tiene los identificadores que asigna el CGCOM.
    pub fn certified(&self) -> bool {
        self.id_sistema.trim().len() == 64 && self.id_repositorio.trim().len() == 32 && !self.id_entidad.trim().is_empty()
    }
    pub fn id_repositorio_or_test(&self) -> String {
        if self.id_repositorio.trim().len() == 32 { self.id_repositorio.trim().to_string() } else { "0".repeat(32) }
    }
    pub fn id_entidad_or_test(&self) -> String {
        if self.id_entidad.trim().is_empty() { "0000".into() } else { self.id_entidad.trim().to_string() }
    }
}

/// Recetas de un tratamiento: cada dispensación cubre como mucho un mes y
/// el plan como mucho un año (RD 1718/2010, art. 8.3); cada receta vale diez
/// días desde que se puede recoger. Devuelve (desde, hasta, envases).
pub fn plan(start: NaiveDate, days: u32, envases: u32) -> Vec<(NaiveDate, NaiveDate, u32)> {
    let days = days.clamp(1, 365);
    let envases = envases.max(1);
    let n = days.div_ceil(30).min(envases).max(1);
    let (base, extra) = (envases / n, envases % n);
    (0..n)
        .map(|i| {
            let ini = start + Duration::days(30 * i as i64);
            (ini, ini + Duration::days(10), base + u32::from(i < extra))
        })
        .collect()
}

fn split_name(full: &str) -> (String, String) {
    let words: Vec<&str> = full.split_whitespace().filter(|w| !matches!(w.to_lowercase().as_str(), "dr." | "dra." | "dr" | "dra")).collect();
    match words.split_first() {
        Some((n, rest)) => (n.to_string(), rest.join(" ")),
        None => (String::new(), String::new()),
    }
}

/// Datos que faltan para emitir por conexión directa (paciente, prescriptor y cada medicamento).
pub fn direct_missing(db: &Db, r: &Prescription) -> Vec<String> {
    let mut v = vec![];
    if let Some(problem) = db.prescriber_problem(r.doctor_id) {
        v.push(problem);
    }
    if !db.rx_unreviewed(r.id).is_empty() {
        v.push("Revisar los avisos de seguridad".to_string());
    }
    match db.patient(r.patient_id) {
        Some(p) => {
            if p.first_name.trim().is_empty() || p.last_name.trim().is_empty() {
                v.push("Nombre y apellidos del paciente".to_string());
            }
            if p.nif.trim().is_empty() {
                v.push("DNI/NIE del paciente".to_string());
            }
            if p.birth.is_none() {
                v.push("Fecha de nacimiento del paciente".to_string());
            }
        }
        None => v.push("Paciente".to_string()),
    }
    match db.doctor(r.doctor_id) {
        Some(d) => {
            if d.license.trim().is_empty() || d.license.trim() == "-" {
                v.push("Nº de colegiado del prescriptor".to_string());
            }
            if split_name(&d.name).1.is_empty() {
                v.push("Nombre completo (con apellidos) del prescriptor".to_string());
            }
            if d.phone.trim().is_empty() && db.config.phone.trim().is_empty() {
                v.push("Teléfono del prescriptor".to_string());
            }
            if db.erx.doctor(r.doctor_id).email.trim().is_empty() && db.config.email.trim().is_empty() {
                v.push("Correo del prescriptor".to_string());
            }
        }
        None => v.push("Prescriptor".to_string()),
    }
    if r.items.is_empty() {
        v.push("Al menos un medicamento".to_string());
    }
    for it in &r.items {
        let name = if it.drug.trim().is_empty() { "un medicamento".to_string() } else { it.drug.clone() };
        let cn_ok = !it.cn.trim().is_empty() && it.cn.trim().len() <= 7 && it.cn.trim().len() >= 6 && it.cn.trim().chars().all(|c| c.is_ascii_digit());
        if !it.cn.trim().is_empty() && !cn_ok {
            v.push(format!("Código nacional de {name}: 6 o 7 cifras"));
        }
        if it.product_type == 4 {
            if it.composition.trim().is_empty() {
                v.push(format!("Composición de la fórmula {name}"));
            }
        } else if it.cn.trim().is_empty() && it.active_code.trim().is_empty() {
            v.push(format!("Código nacional o código del principio activo de {name}"));
        }
        if it.form.trim().is_empty() {
            v.push(format!("Forma farmacéutica de {name}"));
        }
        if it.product_type != 4 && it.dosage.trim().is_empty() {
            v.push(format!("Dosificación de {name}"));
        }
        if it.take <= 0.0 || it.take_unit.trim().is_empty() || it.per <= 0.0 || it.per_unit.trim().is_empty() {
            v.push(format!("Posología completa de {name} (cantidad por toma y veces al día)"));
        }
        if it.days == 0 || it.units == 0 {
            v.push(format!("Duración y envases de {name}"));
        }
        if it.narcotic && (it.days > 90 || plan(today(), it.days, it.units).iter().any(|x| x.2 > 4)) {
            v.push(format!("{name} es estupefaciente: como mucho 3 meses de tratamiento y 4 envases por receta"));
        }
    }
    v
}

/// Las prescripciones (una por medicamento) que se envían al repositorio, sin firmar.
pub fn build_srep(db: &Db, r: &Prescription, id_acceso: &str, pin: Option<&str>, today: NaiveDate) -> Vec<RPrescripcion> {
    let p = db.patient(r.patient_id).cloned().unwrap_or_default();
    let d = db.doctor(r.doctor_id).cloned();
    let (nombre, apellidos) = split_name(&d.as_ref().map(|d| d.name.clone()).unwrap_or_default());
    let email = Some(db.erx.doctor(r.doctor_id).email).filter(|e| !e.trim().is_empty()).unwrap_or_else(|| db.config.email.clone());
    let telefono = d.as_ref().map(|d| d.phone.clone()).filter(|t| !t.trim().is_empty()).unwrap_or_else(|| db.config.phone.clone());
    let destinatario = p.age(today).map(|a| {
        if a < 2 {
            0
        } else if a < 14 {
            1
        } else {
            2
        }
    });
    r.items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let id = crate::srep::new_id32();
            RPrescripcion {
                id_prescripcion: id.clone(),
                id_acceso: id_acceso.to_string(),
                receta_local: r.id,
                linea: i,
                doctor_id: r.doctor_id,
                paciente: RPaciente { nombre: p.first_name.trim().into(), apellidos: p.last_name.trim().into(), fecha_nacimiento: p.birth.map(fmt_date).unwrap_or_default(), tipo_id: 1, dni_nie: p.nif.trim().to_uppercase(), ..Default::default() },
                fecha_prescripcion: today,
                id_entidad: db.erx.id_entidad_or_test(),
                id_centro: cid(db.config.current_center).to_string(),
                prescriptor: RPrescriptor {
                    id: d.as_ref().map(|d| d.license.trim().to_string()).unwrap_or_default(),
                    nombre: nombre.clone(),
                    apellidos: apellidos.clone(),
                    especialidad: d.as_ref().map(|d| d.specialty.clone()).unwrap_or_default(),
                    email: email.trim().into(),
                    telefono: telefono.trim().into(),
                },
                producto: RProducto {
                    cod_producto: it.cn.trim().into(),
                    tipo: it.product_type,
                    principio_activo: it.active_code.trim().into(),
                    composicion: it.composition.trim().into(),
                    denominacion: it.drug.trim().into(),
                    estupefaciente: it.narcotic,
                    psicotropo: it.psychotropic,
                    dosificacion: it.dosage.trim().into(),
                    forma: it.form.trim().into(),
                    via: it.route.trim().into(),
                    formato: it.pack.trim().into(),
                    observaciones: it.notes.trim().into(),
                    destinatario,
                },
                toma: it.take,
                ud_toma: it.take_unit.trim().into(),
                frecuencia: it.per,
                ud_frecuencia: it.per_unit.trim().into(),
                duracion: it.days,
                ud_duracion: "días".into(),
                observaciones: it.notes.trim().into(),
                pin_hash: pin.map(|x| crate::srep::pin_hash(&id, x)).unwrap_or_default(),
                recetas: plan(today, it.days, it.units).into_iter().map(|(ini, fin, n)| RReceta { id_receta: crate::srep::new_id32(), fecha_ini: ini, fecha_fin: fin, num_envases: n, estado: 1, ..Default::default() }).collect(),
                pruebas: !db.erx.certified(),
                ..Default::default()
            }
        })
        .collect()
}

/// Lo que se firma: la prescripción entera sin los campos de la firma.
pub fn signed_document(p: &RPrescripcion) -> String {
    let mut c = p.clone();
    c.firma.clear();
    c.firmante.clear();
    c.documento.clear();
    serde_json::to_string(&c).unwrap_or_default()
}

/// Firma cada prescripción (CAdES) con el certificado del prescriptor.
pub fn sign_all(list: &mut [RPrescripcion], signer: &crate::firma::Signer, now: chrono::DateTime<chrono::Utc>) -> Result<(), String> {
    for p in list.iter_mut() {
        p.documento = signed_document(p);
        let cms = signer.sign_cades(p.documento.as_bytes(), now)?;
        p.firma = crate::firma::b64(&cms);
        p.firmante = if signer.info.cn.is_empty() { signer.info.subject.clone() } else { signer.info.cn.clone() };
    }
    Ok(())
}

/// Ruta del certificado (.p12/.pfx) de cada prescriptor en ESTE ordenador.
fn cert_file() -> std::path::PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("firma.json")
}

pub fn cert_path(doctor_id: u32) -> String {
    std::fs::read_to_string(cert_file()).ok().and_then(|t| serde_json::from_str::<BTreeMap<u32, String>>(&t).ok()).and_then(|m| m.get(&doctor_id).cloned()).unwrap_or_default()
}

pub fn set_cert_path(doctor_id: u32, path: &str) -> Result<(), String> {
    if cfg!(test) {
        return Ok(()); // las pruebas no tocan la configuración del ordenador
    }
    let f = cert_file();
    let mut m: BTreeMap<u32, String> = std::fs::read_to_string(&f).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    m.insert(doctor_id, path.trim().to_string());
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&f, serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

impl ErxConfig {
    pub fn doctor(&self, id: u32) -> ErxDoctor {
        self.doctors.iter().find(|d| d.doctor_id == id).cloned().unwrap_or(ErxDoctor { doctor_id: id, ..Default::default() })
    }
    pub fn set_doctor(&mut self, d: ErxDoctor) {
        self.doctors.retain(|x| x.doctor_id != d.doctor_id);
        self.doctors.push(d);
    }
}

/// Requisitos para que un dentista pueda emitir receta oficial: (requisito, cumplido, cómo resolverlo).
pub fn doctor_readiness(db: &Db, doctor_id: u32) -> Vec<(&'static str, bool, &'static str)> {
    let d = db.doctor(doctor_id);
    let lic = d.map(|d| d.license.trim().to_string()).unwrap_or_default();
    let cfg = db.erx.doctor(doctor_id);
    vec![
        ("Plataforma homologada elegida", !db.erx.platform.trim().is_empty() && !db.erx.url.trim().is_empty(), "Indique en Configuración › Receta electrónica la plataforma y su dirección web"),
        ("Nº de colegiado", !lic.is_empty() && lic != "-", "Rellénelo en Colaboradores; la plataforma lo valida con el Consejo de Dentistas"),
        ("Certificado de firma cualificado", cfg.certificate, "Certificado digital personal (FNMT, colegio…) o identificación por vídeo de la plataforma"),
        ("Alta en la plataforma", cfg.registered, "Darse de alta como prescriptor en la plataforma elegida"),
    ]
}

pub fn doctor_ready(db: &Db, doctor_id: u32) -> bool {
    doctor_readiness(db, doctor_id).iter().all(|r| r.1)
}

/// Datos obligatorios de la receta que faltan (paciente, prescriptor y medicamentos).
pub fn missing_data(db: &Db, r: &Prescription) -> Vec<String> {
    let mut v = vec![];
    if let Some(problem) = db.prescriber_problem(r.doctor_id) {
        v.push(problem);
    }
    if !db.rx_unreviewed(r.id).is_empty() {
        v.push("Revisar los avisos de seguridad".to_string());
    }
    match db.patient(r.patient_id) {
        Some(p) => {
            if p.nif.trim().is_empty() {
                v.push("DNI/NIE del paciente".to_string());
            }
            if p.birth.is_none() {
                v.push("Fecha de nacimiento del paciente".to_string());
            }
        }
        None => v.push("Paciente".to_string()),
    }
    match db.doctor(r.doctor_id) {
        Some(d) if !d.license.trim().is_empty() && d.license.trim() != "-" => {}
        _ => v.push("Nº de colegiado del prescriptor".to_string()),
    }
    if r.items.is_empty() {
        v.push("Al menos un medicamento".to_string());
    }
    for it in &r.items {
        if it.dose.trim().is_empty() || it.frequency.trim().is_empty() {
            v.push(format!("Dosis y pauta de {}", it.drug));
        }
        if it.units == 0 || it.days == 0 {
            v.push(format!("Envases y duración de {}", it.drug));
        }
    }
    v
}

/// Texto con todos los datos de la receta para pegarlo en la plataforma.
pub fn transfer_text(db: &Db, r: &Prescription) -> String {
    let p = db.patient(r.patient_id);
    let d = db.doctor(r.doctor_id);
    let mut s = String::new();
    s.push_str(&format!("PACIENTE: {}\n", p.map(|p| p.full_name()).unwrap_or_default()));
    s.push_str(&format!("DNI/NIE: {}\n", p.map(|p| p.nif.clone()).unwrap_or_default()));
    s.push_str(&format!("Fecha de nacimiento: {}\n", p.and_then(|p| p.birth).map(fmt_date).unwrap_or_default()));
    if let Some(p) = p
        && !p.allergies.trim().is_empty()
    {
        s.push_str(&format!("Alergias: {}\n", p.allergies));
    }
    s.push_str(&format!("PRESCRIPTOR: {} · Nº colegiado {}\n", d.map(|d| d.name.clone()).unwrap_or_default(), d.map(|d| d.license.clone()).unwrap_or_default()));
    if !r.diagnosis.trim().is_empty() {
        s.push_str(&format!("Diagnóstico: {}\n", r.diagnosis));
    }
    for (i, it) in r.items.iter().enumerate() {
        s.push_str(&format!("{}. {} — {} {} durante {} días — {} envase(s){}\n", i + 1, it.drug, it.dose, it.frequency, it.days, it.units, if it.notes.trim().is_empty() { String::new() } else { format!(" — {}", it.notes) }));
    }
    s
}

/// Un CVE válido tiene al menos 8 caracteres alfanuméricos (se admiten guiones).
pub fn cve_valid(code: &str) -> bool {
    let c: String = code.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    c.len() >= 8 && c.chars().all(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn signing_can_require_the_phone_code() {
        let mut c = ErxConfig::default();
        assert!(second_factor_ok(&c, false).is_ok(), "sin la opción, como siempre");
        c.require_2fa = true;
        assert!(second_factor_ok(&c, false).unwrap_err().contains("código del móvil"));
        assert!(second_factor_ok(&c, true).is_ok());
    }

    fn db() -> Db {
        Db::sample(NaiveDate::from_ymd_opt(2026, 11, 5).unwrap())
    }

    #[test]
    fn readiness_lists_what_is_missing() {
        let mut db = db();
        assert!(!doctor_ready(&db, 1));
        db.erx.platform = "Plataforma homologada".into();
        db.erx.url = "https://ejemplo.invalid".into();
        db.erx.set_doctor(ErxDoctor { doctor_id: 1, certificate: true, registered: true, ..Default::default() });
        assert!(doctor_ready(&db, 1), "{:?}", doctor_readiness(&db, 1));
        // la higienista no tiene nº de colegiado de prescriptor
        let hyg = db.collaborators.iter().find(|c| c.license == "-").map(|c| c.id).unwrap();
        db.erx.set_doctor(ErxDoctor { doctor_id: hyg, certificate: true, registered: true, ..Default::default() });
        assert!(!doctor_ready(&db, hyg));
    }

    #[test]
    fn prescription_checks_and_transfer() {
        let mut db = db();
        let id = db.new_prescription(100, 1);
        let r = db.prescriptions.iter().find(|r| r.id == id).unwrap().clone();
        assert!(missing_data(&db, &r).iter().any(|m| m.contains("medicamento")));
        let rm = db.prescriptions.iter_mut().find(|r| r.id == id).unwrap();
        rm.items.push(crate::finance::RxItem { drug: "Ibuprofeno 600 mg".into(), dose: "1 comprimido".into(), frequency: "cada 8 horas".into(), days: 3, units: 1, ..Default::default() });
        let r = db.prescriptions.iter().find(|r| r.id == id).unwrap().clone();
        assert!(missing_data(&db, &r).is_empty(), "{:?}", missing_data(&db, &r));
        let t = transfer_text(&db, &r);
        assert!(t.contains("Ibuprofeno") && t.contains("DNI/NIE") && t.contains("colegiado"));
        assert!(cve_valid("ABCD-1234-EF"));
        assert!(!cve_valid("12 3"));
    }

    #[test]
    fn treatment_plan_splits_by_month() {
        let d0 = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        assert_eq!(plan(d0, 7, 1), vec![(d0, d0 + Duration::days(10), 1)]);
        let p = plan(d0, 90, 3);
        assert_eq!(p.len(), 3);
        assert_eq!(p[2].0, d0 + Duration::days(60));
        assert!(p.iter().all(|x| x.2 == 1));
        assert_eq!(plan(d0, 90, 2).len(), 2, "no más recetas que envases");
        assert_eq!(plan(d0, 900, 20).len(), 13, "como mucho un año");
        assert_eq!(plan(d0, 60, 5).iter().map(|x| x.2).collect::<Vec<_>>(), vec![3, 2]);
    }

    #[test]
    fn direct_prescription_signed_and_dispensed() {
        let mut db = db();
        let rid = db.prescriptions[0].id;
        let r0 = db.prescriptions[0].clone();
        let miss = direct_missing(&db, &r0);
        assert!(miss.iter().any(|m| m.contains("Código nacional o código del principio activo")), "{miss:?}");
        for (i, it) in db.prescriptions[0].items.iter_mut().enumerate() {
            it.cn = format!("65432{i}");
        }
        let r = db.prescriptions.iter().find(|x| x.id == rid).unwrap().clone();
        assert!(direct_missing(&db, &r).is_empty(), "{:?}", direct_missing(&db, &r));
        let today = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let acc = crate::srep::new_id32();
        let mut list = build_srep(&db, &r, &acc, Some("2468"), today);
        assert_eq!(list.len(), r.items.len());
        assert!(list[0].pruebas, "sin identificadores del CGCOM es de pruebas");
        assert!(!list[0].prescriptor.apellidos.is_empty() && !list[0].prescriptor.email.is_empty());
        let signer = crate::firma::load_p12(&crate::firma::tests::test_p12("x"), "x").unwrap();
        sign_all(&mut list, &signer, chrono::Utc::now()).unwrap();
        for p in &list {
            let cms = crate::firma::unb64(&p.firma).unwrap();
            crate::firma::verify_cades(&cms, p.documento.as_bytes()).unwrap();
            assert_eq!(p.documento, signed_document(p));
        }
        // al repositorio y consulta de una farmacia con el PIN
        let mut store = crate::srep::RepoStore::default();
        assert_eq!(store.emitir(list.clone()).unwrap(), list.iter().map(|p| p.recetas.len()).sum::<usize>());
        let q = crate::srep::Query { id_farmacia: "F1".into(), id_acceso: acc.clone(), id_transaccion: "T1".into(), pin: "2468".into(), sw_nodo: "Nodofarma".into(), ..Default::default() };
        let (_, v) = store.consultar(&q, "", today);
        assert_eq!(v["codResultado"], "CONOK");
        assert_eq!(v["prescripciones"].as_array().unwrap().len(), list.len());
        assert_eq!(v["prescripciones"][0]["producto"]["codProducto"], "654320");
        // sin el PIN la farmacia no ve el tratamiento confidencial
        let q2 = crate::srep::Query { pin: String::new(), ..q };
        assert_eq!(store.consultar(&q2, "", today).1["codResultado"], "ERR017");
    }
}
