//! Derechos de protección de datos del paciente (T-F4-03, RGPD artículos 15 a
//! 18 y 20, y Ley 41/2002 para la historia clínica). Ver `docs/normativa.md`.
//! - **Solicitudes:** cada derecho que se ejerce queda registrado, con quién
//!   lo pide, en calidad de qué (el propio paciente, su representante legal o
//!   un autorizado) y con qué lo acredita, y con el plazo de respuesta de un
//!   mes.
//! - **Acceso y portabilidad:** exportación íntegra en un ZIP con sus datos, sus
//!   ficheros y un manifiesto con la huella de cada uno.
//! - **Rectificación:** se corrige en la ficha; la solicitud queda registrada.
//! - **Limitación del tratamiento:** se conservan los datos, pero no se le
//!   envían mensajes.
//! - **Bloqueo:** sale de las listas, y solo Dirección lo quita.
//! - **Supresión:** desactivada hasta la revisión del responsable de
//!   protección de datos (B-10). Antes se comprueba la conservación mínima de
//!   la historia (5 años desde el último proceso) y que las facturas se
//!   conservan por ley.

use crate::model::{Db, today};
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

/// La supresión no se activa hasta la revisión externa de plazos y excepciones.
pub const SUPPRESSION_ENABLED: bool = false;
/// Años mínimos de conservación de la historia (Ley 41/2002, art. 17; una
/// comunidad autónoma puede exigir más).
pub const MIN_YEARS: i32 = 5;
/// Desde los 14 años el paciente ejerce sus derechos (LOPDGDD, art. 7); antes, su representante.
pub const OWN_AGE: i32 = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RightKind {
    Acceso,
    Rectificacion,
    Supresion,
    Limitacion,
    Oposicion,
    Portabilidad,
}

impl RightKind {
    pub const ALL: [RightKind; 6] = [RightKind::Acceso, RightKind::Rectificacion, RightKind::Supresion, RightKind::Limitacion, RightKind::Oposicion, RightKind::Portabilidad];
    pub fn label(self) -> &'static str {
        match self {
            RightKind::Acceso => "Acceso",
            RightKind::Rectificacion => "Rectificación",
            RightKind::Supresion => "Supresión",
            RightKind::Limitacion => "Limitación del tratamiento",
            RightKind::Oposicion => "Oposición",
            RightKind::Portabilidad => "Portabilidad",
        }
    }
}

/// En calidad de qué lo pide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capacity {
    Paciente,
    RepresentanteLegal,
    Autorizado,
}

impl Capacity {
    pub fn label(self) -> &'static str {
        match self {
            Capacity::Paciente => "El propio paciente",
            Capacity::RepresentanteLegal => "Representante legal (padre, madre, tutor)",
            Capacity::Autorizado => "Persona autorizada por escrito",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RightsRequest {
    pub id: u32,
    pub patient_id: u32,
    pub kind: RightKind,
    pub received: NaiveDate,
    pub requester: String,
    pub capacity: Capacity,
    /// Documento que lo acredita (DNI, libro de familia, autorización…), sin copiarlo.
    pub proof: String,
    /// Plazo para responder (un mes, RGPD art. 12.3).
    pub due: NaiveDate,
    #[serde(default)]
    pub resolved: Option<(NaiveDate, String)>,
    pub by: String,
}

/// Marca de limitación o de bloqueo: desde cuándo, por qué y quién.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    pub since: NaiveDateTime,
    pub reason: String,
    pub by: String,
}

fn add_month(d: NaiveDate) -> NaiveDate {
    d.checked_add_months(chrono::Months::new(1)).unwrap_or(d)
}

impl Db {
    /// Registra una solicitud de derechos, si quien la pide puede hacerlo.
    pub fn register_request(&mut self, pid: u32, kind: RightKind, requester: &str, capacity: Capacity, proof: &str, received: NaiveDate) -> Result<u32, String> {
        let p = self.patient(pid).ok_or("El paciente no existe.")?;
        if requester.trim().is_empty() {
            return Err("Escriba quién lo pide.".into());
        }
        let minor = p.age(received).is_some_and(|a| a < OWN_AGE);
        match capacity {
            Capacity::Paciente if minor => return Err(format!("El paciente tiene menos de {OWN_AGE} años: sus derechos los ejerce su representante legal.")),
            Capacity::RepresentanteLegal | Capacity::Autorizado if proof.trim().is_empty() => return Err("Falta el documento que acredita la representación o la autorización.".into()),
            _ => {}
        }
        let id = self.new_id();
        let by = self.config.current_user.clone();
        self.rights_requests.push(RightsRequest { id, patient_id: pid, kind, received, requester: requester.trim().into(), capacity, proof: proof.trim().into(), due: add_month(received), resolved: None, by });
        self.audit("Solicitud de derechos (RGPD)", &format!("{} de la historia nº {}", kind.label(), self.patient(pid).map(|p| p.history_no).unwrap_or(0)));
        Ok(id)
    }

    pub fn resolve_request(&mut self, id: u32, answer: &str, day: NaiveDate) -> Result<(), String> {
        let r = self.rights_requests.iter_mut().find(|r| r.id == id).ok_or("La solicitud no existe.")?;
        if r.resolved.is_some() {
            return Err("Ya está contestada.".into());
        }
        r.resolved = Some((day, answer.trim().to_string()));
        Ok(())
    }

    /// Solicitudes sin contestar y fuera de plazo.
    pub fn overdue_requests(&self, day: NaiveDate) -> Vec<&RightsRequest> {
        self.rights_requests.iter().filter(|r| r.resolved.is_none() && r.due < day).collect()
    }

    /// Limita (o deja de limitar) el tratamiento de los datos del paciente.
    pub fn set_restriction(&mut self, pid: u32, reason: Option<&str>) -> Result<(), String> {
        let by = self.config.current_user.clone();
        let p = self.patients.iter_mut().find(|p| p.id == pid).ok_or("El paciente no existe.")?;
        p.restricted = reason.map(|r| Mark { since: crate::model::now(), reason: r.trim().into(), by: by.clone() });
        let h = p.history_no;
        self.audit(if reason.is_some() { "Limitación del tratamiento" } else { "Fin de la limitación del tratamiento" }, &format!("historia nº {h}"));
        Ok(())
    }

    /// Bloquea (o desbloquea, solo Dirección) los datos del paciente.
    pub fn set_block(&mut self, pid: u32, reason: Option<&str>, is_admin: bool) -> Result<(), String> {
        if reason.is_none() && !is_admin {
            return Err("Solo Dirección puede desbloquear los datos de un paciente.".into());
        }
        let by = self.config.current_user.clone();
        let p = self.patients.iter_mut().find(|p| p.id == pid).ok_or("El paciente no existe.")?;
        p.blocked = reason.map(|r| Mark { since: crate::model::now(), reason: r.trim().into(), by: by.clone() });
        let h = p.history_no;
        self.audit(if reason.is_some() { "Bloqueo de datos" } else { "Desbloqueo de datos" }, &format!("historia nº {h}"));
        Ok(())
    }

    /// Última actividad asistencial del paciente (tratamiento, cita o documento).
    pub fn last_care(&self, pid: u32) -> Option<NaiveDate> {
        let e = self.entries.iter().filter(|e| e.patient_id == pid).map(|e| e.date);
        let a = self.appointments.iter().filter(|a| a.patient_id == Some(pid)).map(|a| a.date);
        let d = self.documents.iter().filter(|d| d.patient_id == pid).map(|d| d.date);
        e.chain(a).chain(d).max()
    }

    /// Hasta cuándo hay que conservar la historia como mínimo.
    pub fn keep_until(&self, pid: u32) -> Option<NaiveDate> {
        let l = self.last_care(pid)?;
        NaiveDate::from_ymd_opt(l.year() + MIN_YEARS, l.month(), l.day().min(28))
    }

    /// Por qué no se puede suprimir (vacío: se podría). Hoy siempre lleva que
    /// está desactivada hasta la revisión externa.
    pub fn suppression_problems(&self, pid: u32, day: NaiveDate) -> Vec<String> {
        let mut v = vec![];
        if !SUPPRESSION_ENABLED {
            v.push("La supresión está desactivada hasta que el responsable de protección de datos revise plazos y excepciones (B-10). Mientras tanto, puede bloquear los datos.".into());
        }
        if !self.rights_requests.iter().any(|r| r.patient_id == pid && r.kind == RightKind::Supresion) {
            v.push("No hay una solicitud de supresión registrada.".into());
        }
        if let Some(k) = self.keep_until(pid).filter(|k| *k > day) {
            v.push(format!("La historia se conserva como mínimo hasta el {} ({MIN_YEARS} años desde el último proceso, Ley 41/2002; la comunidad autónoma puede exigir más).", k.format("%d/%m/%Y")));
        }
        if self.invoices.iter().any(|i| i.patient_id == pid) {
            v.push("Tiene facturas, que se conservan por obligación fiscal y mercantil: esos datos no se suprimen.".into());
        }
        v
    }
}

/// Acceso y portabilidad: ZIP con sus datos (JSON), sus ficheros (imágenes y
/// documentos que estén en este ordenador) y un manifiesto con la huella
/// (SHA-256) de cada uno, quién lo exportó y cuándo. Solo para el usuario.
pub fn export_zip(db: &Db, pid: u32, dest: &Path, who: &str) -> Result<usize, String> {
    let data = crate::record::patient_data_json(db, pid, today());
    let extra = serde_json::json!({
        "ortodoncia": db.ortho.iter().filter(|o| o.patient_id == pid).collect::<Vec<_>>(),
        "analisis_con_ia": db.ia_reports.iter().filter(|r| r.patient_id == pid).collect::<Vec<_>>(),
        "solicitudes_de_derechos": db.rights_requests.iter().filter(|r| r.patient_id == pid).collect::<Vec<_>>(),
    });
    let mut files: Vec<(String, Vec<u8>)> = vec![("datos.json".into(), data.into_bytes()), ("datos_adicionales.json".into(), serde_json::to_vec_pretty(&extra).map_err(|e| e.to_string())?)];
    let mut missing = vec![];
    for (i, img) in db.images.iter().filter(|i| i.patient_id == pid && !i.demo).enumerate() {
        let p = Path::new(&img.path);
        match std::fs::read(p) {
            Ok(b) if p.is_file() => files.push((format!("ficheros/{:03}_{}", i + 1, crate::record::safe_name(&p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())), b)),
            _ => missing.push(img.path.clone()),
        }
    }
    let sha = |b: &[u8]| ring::digest::digest(&ring::digest::SHA256, b).as_ref().iter().map(|x| format!("{x:02x}")).collect::<String>();
    let mut manifest = format!("Exportación de los datos del paciente nº {}\nExportado el {} por {who}\n\nFichero\tSHA-256\tbytes\n", db.patient(pid).map(|p| p.history_no).unwrap_or(0), crate::model::now().format("%d/%m/%Y %H:%M"));
    for (n, b) in &files {
        manifest += &format!("{n}\t{}\t{}\n", sha(b), b.len());
    }
    if !missing.is_empty() {
        manifest += &format!("\nFicheros que no están en este ordenador (solo su ruta, en datos.json): {}\n", missing.len());
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (n, b) in files.iter().chain(std::iter::once(&("MANIFIESTO.txt".to_string(), manifest.into_bytes()))) {
            z.start_file(n.as_str(), opt).map_err(|e| e.to_string())?;
            z.write_all(b).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    crate::paths::write_private(dest, buf.get_ref()).map_err(|e| e.to_string())?;
    Ok(files.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn d(y: i32, m: u32, dd: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, dd).unwrap()
    }

    /// DATOS FICTICIOS (los de ejemplo). Solicitudes con plazo de un mes y control
    /// de representación: un menor de 14 años, por su representante; quien no es
    /// el paciente, con su acreditación.
    #[test]
    fn requests_have_a_deadline_and_check_who_asks() {
        let mut db = Db::sample(d(2026, 11, 13));
        let adult = db.patients.iter().find(|p| p.age(d(2026, 11, 13)).is_some_and(|a| a >= 18)).unwrap().id;
        let id = db.register_request(adult, RightKind::Acceso, "El propio paciente", Capacity::Paciente, "", d(2026, 1, 31)).unwrap();
        assert_eq!(db.rights_requests.iter().find(|r| r.id == id).unwrap().due, d(2026, 2, 28), "un mes");
        assert!(db.register_request(adult, RightKind::Acceso, "Un vecino", Capacity::Autorizado, "", d(2026, 3, 1)).unwrap_err().contains("acredita"));
        assert!(db.register_request(adult, RightKind::Acceso, "Su hija", Capacity::Autorizado, "Autorización firmada del 01/03/2026", d(2026, 3, 1)).is_ok());
        let p = db.patients.iter_mut().find(|p| p.id == adult).unwrap();
        p.birth = Some(d(2015, 6, 1));
        assert!(db.register_request(adult, RightKind::Acceso, "Él mismo", Capacity::Paciente, "", d(2026, 3, 1)).unwrap_err().contains("menos de 14"));
        assert!(db.register_request(adult, RightKind::Acceso, "Su madre", Capacity::RepresentanteLegal, "Libro de familia", d(2026, 3, 1)).is_ok());
        assert_eq!(db.overdue_requests(d(2026, 3, 2)).len(), 1, "la de enero, sin contestar");
        db.resolve_request(id, "Entregada la exportación", d(2026, 3, 2)).unwrap();
        assert!(db.overdue_requests(d(2026, 3, 2)).is_empty());
        assert!(db.resolve_request(id, "otra vez", d(2026, 3, 3)).is_err());
        assert!(db.audit_log.iter().any(|a| a.action == "Solicitud de derechos (RGPD)"));
    }

    /// Limitación: sin mensajes. Bloqueo: solo Dirección lo quita.
    #[test]
    fn restriction_stops_messages_and_only_management_unblocks() {
        let mut db = Db::sample(crate::model::today());
        db.generate_messages(crate::model::today());
        let with_msgs: u32 = db.messages.iter().map(|m| m.patient_id).find(|p| *p != 0).unwrap();
        db.messages.clear();
        db.set_restriction(with_msgs, Some("Lo pide el paciente mientras se resuelve su reclamación")).unwrap();
        db.generate_messages(crate::model::today());
        assert!(!db.messages.iter().any(|m| m.patient_id == with_msgs), "limitado: no se le envía nada");
        db.set_restriction(with_msgs, None).unwrap();
        db.set_block(with_msgs, Some("Fin de la relación; se conserva por obligación legal"), false).unwrap();
        assert!(db.patient(with_msgs).unwrap().blocked.is_some());
        assert!(db.set_block(with_msgs, None, false).unwrap_err().contains("Dirección"));
        db.set_block(with_msgs, None, true).unwrap();
        assert!(db.patient(with_msgs).unwrap().blocked.is_none());
    }

    /// La supresión está desactivada; y aun activada, se respetan la
    /// conservación mínima de la historia y las facturas.
    #[test]
    fn suppression_is_disabled_and_explains_what_must_be_kept() {
        let mut db = Db::sample(crate::model::today());
        let pid = db.invoices.first().map(|i| i.patient_id).unwrap();
        let day = crate::model::today();
        let p = db.suppression_problems(pid, day);
        assert!(p[0].contains("desactivada hasta"), "{p:?}");
        assert!(p.iter().any(|x| x.contains("No hay una solicitud")));
        assert!(p.iter().any(|x| x.contains("se conserva como mínimo hasta")));
        assert!(p.iter().any(|x| x.contains("facturas")));
        let last = db.last_care(pid).unwrap();
        assert_eq!(db.keep_until(pid).unwrap().year(), last.year() + 5);
        db.register_request(pid, RightKind::Supresion, "El paciente", Capacity::Paciente, "", day).ok();
        assert!(!db.suppression_problems(pid, day).iter().any(|x| x.contains("No hay una solicitud")));
        const { assert!(!SUPPRESSION_ENABLED) };
    }

    /// Acceso y portabilidad: el ZIP lleva sus datos (y solo los suyos), sus
    /// ficheros y un manifiesto con la huella de cada uno.
    #[test]
    fn the_export_is_complete_traceable_and_private() {
        let mut db = Db::sample(crate::model::today());
        let dir = std::env::temp_dir().join(format!("ds_rgpd_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let img = dir.join("rx.png");
        std::fs::write(&img, b"imagen ficticia").unwrap();
        let pid = 101;
        let other = db.patients.iter().find(|p| p.id != pid).unwrap().full_name();
        let id = db.new_id();
        db.images.push(crate::model::PatientImage {
            id,
            patient_id: pid,
            kind: crate::model::ImageKind::Periapical,
            date: crate::model::today(),
            teeth: vec![36],
            path: img.to_string_lossy().into(),
            source: String::new(),
            notes: String::new(),
            demo: false,
        });
        let out = dir.join("export.zip");
        let n = export_zip(&db, pid, &out, "Dirección").unwrap();
        assert!(n >= 3);
        let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let mut read = |name: &str| {
            let mut s = Vec::new();
            z.by_name(name).unwrap().read_to_end(&mut s).unwrap();
            s
        };
        let data = String::from_utf8(read("datos.json")).unwrap();
        assert!(data.contains(&db.patient(pid).unwrap().last_name) && !data.contains(&other));
        let manifest = String::from_utf8(read("MANIFIESTO.txt")).unwrap();
        let file_name = manifest.lines().find(|l| l.starts_with("ficheros/")).unwrap().split('\t').next().unwrap().to_string();
        let bytes = read(&file_name);
        assert_eq!(bytes, b"imagen ficticia");
        let sha: String = ring::digest::digest(&ring::digest::SHA256, &bytes).as_ref().iter().map(|x| format!("{x:02x}")).collect();
        assert!(manifest.contains(&sha) && manifest.contains("por Dirección"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
