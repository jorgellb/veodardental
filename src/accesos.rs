//! Registro de accesos a las historias clínicas (T-F4-02): quién consulta y
//! quién modifica la historia de cada paciente, cuándo, desde qué puesto y
//! en qué parte (ficha, odontograma, recetas…). Nunca copia la historia: solo
//! apunta qué parte se vio o se cambió.
//!
//! - **Fichero aparte** (`accesos_historias.jsonl`, solo para el usuario): no
//!   va con los datos de la clínica, así que los puestos no lo reciben.
//! - **Encadenado:** cada línea lleva la huella de la anterior. Borrar o
//!   cambiar una línea se detecta ([`verify`]).
//! - **En red lo escribe el servidor:** las consultas le llegan por la sesión
//!   del puesto, y el usuario lo pone el servidor, no el puesto. Las
//!   modificaciones las apunta al recibir los cambios.
//! - **Solo Dirección** puede leerlo.

use crate::sync::Change;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Consulta,
    Modificacion,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Consulta => "Consulta",
            Action::Modificacion => "Modificación",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub at: NaiveDateTime,
    pub user_id: u32,
    pub user: String,
    pub station: u32,
    pub patient: u32,
    pub action: Action,
    /// Qué parte de la historia (no su contenido).
    pub context: String,
    /// Huella de la línea anterior (vacía en la primera).
    #[serde(default)]
    pub prev: String,
}

/// Lo que ve Dirección de una historia: las entradas y si el registro está
/// íntegro (o por qué no se pudo leer).
pub type Loaded = Result<(Vec<Entry>, String), String>;

/// El registro va al lado de los datos de la clínica.
pub fn log_file(data: &Path) -> PathBuf {
    data.with_file_name("accesos_historias.jsonl")
}

fn sha(s: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, s.as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// Última línea completa del fichero (leyendo solo el final, como bytes).
/// `Ok(None)`: no hay fichero o está vacío. Si hay algo pero no se puede leer
/// la última línea, es un error: la cadena no se rompe en silencio.
fn last_line(path: &Path) -> Result<Option<String>, String> {
    let Ok(mut f) = std::fs::File::open(path) else { return Ok(None) };
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    if len == 0 {
        return Ok(None);
    }
    let start = len.saturating_sub(256 * 1024);
    f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let line = buf.split(|b| *b == b'\n').rev().find(|l| !l.iter().all(|b| b.is_ascii_whitespace())).ok_or("El registro de accesos no se puede leer.")?;
    let text = std::str::from_utf8(line).map_err(|_| "La última línea del registro de accesos está dañada.".to_string())?;
    serde_json::from_str::<Entry>(text).map_err(|_| "La última línea del registro de accesos está dañada.".to_string())?;
    Ok(Some(text.to_string()))
}

/// Añade una entrada al final, encadenada con la anterior.
pub fn append(path: &Path, mut e: Entry) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    e.prev = last_line(path)?.map(|l| sha(&l)).unwrap_or_default();
    let mut line = serde_json::to_string(&e).map_err(|e| e.to_string())?;
    line.push('\n');
    let mut f = crate::paths::append_private(path).map_err(|e| format!("No se puede abrir el registro de accesos: {e}"))?;
    f.write_all(line.as_bytes()).and_then(|_| f.sync_data()).map_err(|e| format!("No se pudo apuntar el acceso: {e}"))
}

/// Comprueba que no falta ni se ha cambiado ninguna línea. Devuelve cuántas
/// hay; si no, dice todas las líneas rotas (no solo la primera).
pub fn verify(path: &Path) -> Result<usize, String> {
    let Ok(text) = std::fs::read_to_string(path) else { return Ok(0) };
    let mut prev = String::new();
    let mut n = 0;
    let mut broken = vec![];
    for (i, l) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        match serde_json::from_str::<Entry>(l) {
            Ok(e) if e.prev == prev => {}
            _ => broken.push(i + 1),
        }
        prev = sha(l);
        n += 1;
    }
    if broken.is_empty() {
        Ok(n)
    } else {
        let list: Vec<String> = broken.iter().map(|b| b.to_string()).collect();
        Err(format!("El registro de accesos se ha tocado: no siguen a la anterior las líneas {}.", list.join(", ")))
    }
}

/// Las entradas (de un paciente, o todas), de la más reciente a la más antigua.
pub fn read(path: &Path, patient: Option<u32>) -> Vec<Entry> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut v: Vec<Entry> = text.lines().filter_map(|l| serde_json::from_str::<Entry>(l).ok()).filter(|e| patient.is_none_or(|p| p == e.patient)).collect();
    v.reverse();
    v
}

/// Listas que forman la historia de un paciente, y cómo se llaman.
pub const HISTORY_LISTS: [(&str, &str); 12] = [
    ("patients", "ficha"),
    ("entries", "tratamientos"),
    ("budgets", "presupuestos"),
    ("perio", "periodontograma"),
    ("images", "imágenes"),
    ("documents", "documentos y consentimientos"),
    ("health", "cuestionario de salud"),
    ("prescriptions", "recetas"),
    ("ortho", "ortodoncia"),
    ("ia_reports", "análisis con IA"),
    ("evo_notes", "notas de evolución"),
    ("alert_acks", "alertas clínicas revisadas"),
];

/// De unos cambios, a qué pacientes tocan y en qué partes de su historia.
pub fn patients_touched(changes: &[Change]) -> BTreeMap<u32, BTreeSet<String>> {
    let mut out: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for c in changes {
        let Some((_, label)) = HISTORY_LISTS.iter().find(|(f, _)| *f == c.field) else { continue };
        let pid = |j: &Option<String>| -> Option<u32> {
            let v: serde_json::Value = serde_json::from_str(j.as_deref()?).ok()?;
            let key = if c.field == "patients" { "id" } else { "patient_id" };
            v[key].as_u64().map(|n| n as u32)
        };
        let ids: BTreeSet<u32> = [pid(&c.after), pid(&c.before)].into_iter().flatten().chain(if c.field == "patients" { c.key.as_deref().and_then(|k| k.parse().ok()) } else { None }).collect();
        for p in ids {
            out.entry(p).or_default().insert(label.to_string());
        }
    }
    out
}

/// Para no apuntar la misma consulta en cada fotograma: una vez cada cinco
/// minutos por usuario, paciente y parte.
#[derive(Default)]
pub struct Throttle(HashMap<(u32, u32, String), std::time::Instant>);

impl Throttle {
    pub fn fresh(&mut self, user: u32, patient: u32, context: &str) -> bool {
        let now = std::time::Instant::now();
        let k = (user, patient, context.to_string());
        match self.0.get(&k) {
            Some(t) if now.duration_since(*t) < std::time::Duration::from_secs(300) => false,
            _ => {
                if self.0.len() > 5000 {
                    self.0.clear();
                }
                self.0.insert(k, now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(p: u32, a: Action, ctx: &str) -> Entry {
        Entry { at: crate::model::now(), user_id: 2, user: "Recepción".into(), station: 3, patient: p, action: a, context: ctx.into(), prev: String::new() }
    }

    #[test]
    fn the_log_is_chained_and_tampering_shows() {
        let dir = std::env::temp_dir().join(format!("ds_accesos_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let f = log_file(&dir.join("clinica.json"));
        for (p, a, c) in [(101, Action::Consulta, "Ficha"), (102, Action::Modificacion, "tratamientos"), (101, Action::Consulta, "Odontograma")] {
            append(&f, entry(p, a, c)).unwrap();
        }
        assert_eq!(verify(&f), Ok(3));
        assert_eq!(read(&f, Some(101)).iter().map(|e| e.context.as_str()).collect::<Vec<_>>(), vec!["Odontograma", "Ficha"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // borrar una línea o cambiarla se nota
        let text = std::fs::read_to_string(&f).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        std::fs::write(&f, format!("{}\n{}\n", lines[0], lines[2])).unwrap();
        assert!(verify(&f).unwrap_err().contains("líneas 2"));
        std::fs::write(&f, text.replace("Recepción", "Otra")).unwrap();
        assert!(verify(&f).unwrap_err().contains("líneas 2, 3"), "dice todas las rotas");
        // con la última línea ilegible, no se apunta nada (no se empieza otra cadena)
        std::fs::write(&f, format!("{text}{{dañada\n")).unwrap();
        assert!(append(&f, entry(103, Action::Consulta, "Ficha")).unwrap_err().contains("dañada"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn changes_tell_which_patient_and_part_without_content() {
        let ch = |field: &str, key: &str, after: &str| Change { field: field.into(), key: Some(key.into()), before: None, after: Some(after.into()) };
        let t = patients_touched(&[
            ch("entries", "5", r#"{"id":5,"patient_id":101,"description":"Endodoncia"}"#),
            ch("patients", "102", r#"{"id":102,"allergies":"Penicilina"}"#),
            ch("perio", "9", r#"{"id":9,"patient_id":101}"#),
            ch("appointments", "7", r#"{"id":7,"patient_id":101}"#),
            ch("expenses", "8", r#"{"id":8}"#),
        ]);
        assert_eq!(t.get(&101).unwrap().iter().cloned().collect::<Vec<_>>(), vec!["periodontograma", "tratamientos"]);
        assert_eq!(t.get(&102).unwrap().iter().cloned().collect::<Vec<_>>(), vec!["ficha"]);
        assert_eq!(t.len(), 2, "las citas y los gastos no son la historia");
        let mut th = Throttle::default();
        assert!(th.fresh(1, 101, "Ficha") && !th.fresh(1, 101, "Ficha") && th.fresh(1, 101, "Recetas") && th.fresh(2, 101, "Ficha"));
    }
}
