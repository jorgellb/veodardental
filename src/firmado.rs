//! Lo que firma el paciente: documentos y consentimientos, y cuestionarios de
//! salud.
//!
//! Al firmar se guarda la huella de lo firmado (el texto o las respuestas, para
//! qué paciente) y un sello que la une con quién firmó, cuándo y el trazo de la
//! firma. Después se comprueba que nada ha cambiado. Ni un puesto en red ni la
//! carpeta compartida pueden cambiar ni borrar lo firmado: solo se puede anotar
//! la revocación de un consentimiento y, al fusionar dos fichas del mismo
//! paciente, pasarlo a la que se conserva.

use crate::comms::PatientDoc;
use crate::finance::Prescription;
use crate::health::HealthForm;
use crate::model::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Estado de lo firmado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Integrity {
    NotSigned,
    /// Firmado y sin cambios desde la firma.
    Intact,
    /// Algo ha cambiado después de firmarse.
    Modified,
    /// Firmado antes de que se guardara la huella: no se puede comprobar.
    Unsealed,
}

impl Integrity {
    pub fn label(self) -> &'static str {
        match self {
            Integrity::NotSigned => "Sin firmar",
            Integrity::Intact => "Firma comprobada: sin cambios desde que se firmó",
            Integrity::Modified => "⚠ El contenido ha cambiado después de firmarse",
            Integrity::Unsealed => "Firmado antes de guardarse la huella: no se puede comprobar",
        }
    }
}

fn sha_hex(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

// ───────────────────────────── documentos ─────────────────────────────

/// Paciente para el que se firmó (el actual si se firmó antes de guardarlo).
fn doc_signed_patient(d: &PatientDoc) -> u32 {
    d.signed_patient.unwrap_or(d.patient_id)
}

/// Huella del contenido firmado: el texto que lee el paciente, para quien se firmó.
pub fn doc_content_hash(d: &PatientDoc) -> String {
    let mut c = d.clone();
    c.patient_id = doc_signed_patient(d);
    crate::doc_link::doc_hash(&c)
}

/// Sello: el contenido firmado, quién, cuándo y el trazo de la firma.
pub fn doc_seal(d: &PatientDoc) -> String {
    let when = d.signed_at.map(|t| t.format("%Y-%m-%dT%H:%M:%S%.f").to_string()).unwrap_or_default();
    sha_hex(&format!("{}|{}|{}|{}", d.content_hash, d.signer, when, serde_json::to_string(&d.signature).unwrap_or_default()))
}

/// Guarda la huella y el sello al firmar.
pub fn seal_doc(d: &mut PatientDoc) {
    d.signed_patient = Some(d.patient_id);
    d.content_hash = doc_content_hash(d);
    d.seal = doc_seal(d);
}

pub fn doc_integrity(d: &PatientDoc) -> Integrity {
    if !d.signed {
        Integrity::NotSigned
    } else if d.content_hash.is_empty() {
        Integrity::Unsealed
    } else if doc_content_hash(d) != d.content_hash || doc_seal(d) != d.seal {
        Integrity::Modified
    } else {
        Integrity::Intact
    }
}

/// Lo que queda de un documento firmado cuando un puesto manda otra versión
/// o lo borra: el firmado tal cual, con la revocación si la anota. Pasarlo a
/// otro paciente solo lo hace la fusión de pacientes, que es una operación
/// del servidor (o con el fichero compartido bloqueado), no un cambio suelto.
pub fn keep_signed_doc(current: &PatientDoc, proposed: Option<&PatientDoc>) -> PatientDoc {
    let mut out = current.clone();
    if let Some(p) = proposed
        && out.revoked_at.is_none()
        && p.revoked_at.is_some()
    {
        out.revoked_at = p.revoked_at;
        out.revoked_by = p.revoked_by.clone();
        out.revoke_reason = p.revoke_reason.clone();
    }
    out
}

// ───────────────────────────── cuestionarios ─────────────────────────────

/// Respuestas firmadas de un cuestionario (los campos, uno a uno, para que la
/// huella no cambie si el formato gana campos nuevos). Las observaciones del
/// profesional no son del paciente: no entran.
#[derive(Serialize)]
struct HealthContent<'a> {
    id: u32,
    patient: u32,
    date: chrono::NaiveDate,
    filled_by: &'a str,
    reason: &'a str,
    yes: &'a BTreeMap<String, bool>,
    details: &'a BTreeMap<String, String>,
    cigarettes: u32,
    brushings: u32,
    last_visit: usize,
    aids: &'a [String],
}

pub fn health_hash(f: &HealthForm) -> String {
    let c = HealthContent {
        id: f.id,
        patient: f.signed_patient.unwrap_or(f.patient_id),
        date: f.date,
        filled_by: &f.filled_by,
        reason: &f.reason,
        yes: &f.yes,
        details: &f.details,
        cigarettes: f.cigarettes,
        brushings: f.brushings,
        last_visit: f.last_visit,
        aids: &f.aids,
    };
    sha_hex(&serde_json::to_string(&c).unwrap_or_default())
}

pub fn seal_health(f: &mut HealthForm) {
    f.signed_patient = Some(f.patient_id);
    f.signed_hash = health_hash(f);
}

pub fn health_integrity(f: &HealthForm) -> Integrity {
    if !f.signed {
        Integrity::NotSigned
    } else if f.signed_hash.is_empty() {
        Integrity::Unsealed
    } else if health_hash(f) != f.signed_hash {
        Integrity::Modified
    } else {
        Integrity::Intact
    }
}

/// Las respuestas firmadas de un cuestionario no cambian; las observaciones
/// del profesional sí.
pub fn keep_signed_health(current: &HealthForm, proposed: Option<&HealthForm>) -> HealthForm {
    let mut out = current.clone();
    if let Some(p) = proposed {
        out.notes = p.notes.clone();
    }
    out
}

// ───────────────────────────── recetas emitidas ─────────────────────────────

/// Una receta emitida es el registro de lo prescrito: no cambia ni deja de
/// estar emitida. Sí se actualiza su estado en las farmacias y se puede
/// corregir el código de la plataforma por otro (nunca borrarlo: después se
/// podría cambiar la receta).
pub fn keep_issued_rx(current: &Prescription, proposed: Option<&Prescription>) -> Prescription {
    let mut out = current.clone();
    if let Some(p) = proposed {
        if !p.erx_code.trim().is_empty() {
            (out.erx_code, out.erx_platform, out.erx_date) = (p.erx_code.clone(), p.erx_platform.clone(), p.erx_date.or(out.erx_date));
        }
        if let (Some(o), Some(n)) = (out.srep.as_mut(), p.srep.as_ref()) {
            o.status = n.status.clone();
            o.checked_at = n.checked_at;
            o.events = n.events.clone();
        }
    }
    out
}

// ───────────────────────── protección en red y en carpeta ─────────────────────────

/// Las listas con registros que un puesto no puede cambiar ni borrar una vez
/// firmados, emitidos o confirmados (ver [`protect_json`]).
pub const PROTECTED_LISTS: [&str; 5] = ["documents", "health", "prescriptions", "evo_notes", "alert_acks"];

/// Servidor: versión que se acepta de un registro firmado de `documents` o
/// `health`, o de una receta emitida de `prescriptions` (`current`), cuando un
/// puesto manda `after` (`None`: lo borra). Devuelve `None` si no hay nada que
/// corregir (no estaba firmado, o el cambio está permitido tal cual).
pub fn protect_json(field: &str, current: &str, after: Option<&str>) -> Option<String> {
    fn fix<T: serde::de::DeserializeOwned + Serialize + PartialEq>(current: &str, after: Option<&str>, signed: impl Fn(&T) -> bool, keep: impl Fn(&T, Option<&T>) -> T) -> Option<String> {
        let cur: T = serde_json::from_str(current).ok()?;
        if !signed(&cur) {
            return None;
        }
        let proposed: Option<T> = after.and_then(|a| serde_json::from_str(a).ok());
        let kept = keep(&cur, proposed.as_ref());
        if proposed.as_ref() == Some(&kept) {
            return None;
        }
        serde_json::to_string(&kept).ok()
    }
    match field {
        "documents" => fix::<PatientDoc>(current, after, |d| d.signed, keep_signed_doc),
        "health" => fix::<HealthForm>(current, after, |f| f.signed, keep_signed_health),
        "prescriptions" => fix::<Prescription>(current, after, |r| r.issued(), keep_issued_rx),
        "evo_notes" => fix::<crate::evolucion::EvoNote>(current, after, |n| n.is_confirmed(), crate::evolucion::keep_confirmed),
        "alert_acks" => fix::<crate::alertas::AlertAck>(current, after, |_| true, |cur, _| cur.clone()),
        _ => None,
    }
}

/// Lo firmado que hay en el fichero compartido, para conservarlo al guardar.
#[derive(Default)]
pub struct SignedOnDisk {
    docs: Vec<PatientDoc>,
    health: Vec<HealthForm>,
    rx: Vec<Prescription>,
    notes: Vec<crate::evolucion::EvoNote>,
}

impl SignedOnDisk {
    pub fn read(disk: &serde_json::Value) -> SignedOnDisk {
        let take = |field: &str| disk[field].as_array().into_iter().flatten().filter(|v| v["signed"] == true).cloned().collect::<Vec<_>>();
        SignedOnDisk {
            docs: take("documents").into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect(),
            health: take("health").into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect(),
            rx: disk["prescriptions"].as_array().into_iter().flatten().filter_map(|v| serde_json::from_value::<Prescription>(v.clone()).ok()).filter(|r| r.issued()).collect(),
            notes: disk["evo_notes"].as_array().into_iter().flatten().filter_map(|v| serde_json::from_value::<crate::evolucion::EvoNote>(v.clone()).ok()).filter(|n| n.is_confirmed()).collect(),
        }
    }
}

impl Db {
    pub fn doc_integrity(&self, id: u32) -> Integrity {
        self.documents.iter().find(|d| d.id == id).map(doc_integrity).unwrap_or(Integrity::NotSigned)
    }

    pub fn health_integrity(&self, id: u32) -> Integrity {
        self.health.iter().find(|h| h.id == id).map(health_integrity).unwrap_or(Integrity::NotSigned)
    }

    /// Carpeta compartida: lo firmado y lo emitido del fichero se queda como
    /// estaba (con la revocación o el estado en farmacias que anote este
    /// puesto). La fusión de pacientes va aparte, con el fichero bloqueado.
    pub fn keep_signed(&mut self, disk: &SignedOnDisk) {
        fn restore<T: Clone + PartialEq>(list: &mut Vec<T>, cur: &T, same: impl Fn(&T) -> bool, keep: impl Fn(&T, Option<&T>) -> T) {
            let ours = list.iter().position(&same);
            let kept = keep(cur, ours.map(|i| &list[i]));
            match ours {
                Some(i) if list[i] != kept => list[i] = kept,
                Some(_) => {}
                None => list.push(kept),
            }
        }
        for cur in &disk.docs {
            restore(&mut self.documents, cur, |d| d.id == cur.id, keep_signed_doc);
        }
        for cur in &disk.rx {
            restore(&mut self.prescriptions, cur, |r| r.id == cur.id, keep_issued_rx);
        }
        for cur in &disk.health {
            restore(&mut self.health, cur, |f| f.id == cur.id, keep_signed_health);
        }
        for cur in &disk.notes {
            restore(&mut self.evo_notes, cur, |n| n.id == cur.id, crate::evolucion::keep_confirmed);
        }
    }

    /// Firma el cuestionario de salud (revisado por el paciente) y guarda la
    /// huella de sus respuestas.
    pub fn sign_health(&mut self, id: u32) -> Result<(), String> {
        let f = self.health.iter_mut().find(|h| h.id == id).ok_or("Cuestionario no encontrado")?;
        if f.signed {
            return Err("El cuestionario ya está firmado: para cambiarlo, haga una revisión nueva.".into());
        }
        f.signed = true;
        seal_health(f);
        Ok(())
    }

    /// Revoca un consentimiento firmado: el documento y su firma se quedan, y
    /// al lado la revocación (quién, cuándo y por qué). Si es el de protección
    /// de datos, el paciente deja de tener autorizadas las comunicaciones.
    pub fn revoke_document(&mut self, id: u32, by: &str, reason: &str) -> Result<(), String> {
        let (by, reason) = (by.trim(), reason.trim());
        if by.is_empty() || reason.is_empty() {
            return Err("Indique quién revoca y el motivo.".into());
        }
        let d = self.documents.iter_mut().find(|d| d.id == id).ok_or("Documento no encontrado")?;
        if !d.signed {
            return Err("Solo se revoca un documento firmado.".into());
        }
        if d.revoked_at.is_some() {
            return Err("Ese documento ya está revocado.".into());
        }
        d.revoked_at = Some(now());
        d.revoked_by = by.to_string();
        d.revoke_reason = reason.to_string();
        let (pid, tid, title) = (d.patient_id, d.template_id, d.title.clone());
        // revocar es la última voluntad del paciente: aunque firmara antes otro
        // igual, deja de estar autorizado (firmar uno nuevo lo vuelve a autorizar)
        if self.doc_templates.iter().any(|x| x.id == tid && x.marks_gdpr)
            && let Some(p) = self.patient_mut(pid)
        {
            p.accepts_gdpr = false;
        }
        let who = self.patient_name(pid);
        self.audit("Revocar consentimiento", &format!("{title} de {who}: revocado por {by}. Motivo: {reason}"));
        Ok(())
    }
}
