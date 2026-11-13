//! Datos en este ordenador (sin servidor ni carpeta compartida).
//!
//! Al guardar no se reescribe el fichero de datos entero: se apunta en el
//! diario (`clinica.cambios`, ver [`crate::sync`]) cómo han quedado los
//! registros que han cambiado, en el disco antes de seguir, y el fichero
//! entero se reescribe cada cierto tiempo y al cerrar. Al abrir los datos, lo
//! del diario se pone en ellos ([`crate::model::Db::load_or_sample`]).
//!
//! Solo una ventana de VeodarDental (o el servidor de datos) puede guardar en
//! los mismos datos a la vez: los bloquea al abrirlos, y el sistema quita el
//! bloqueo si el programa se cierra o se cae. Las demás los abren sin guardar.

use crate::model::Db;
use crate::sync::{self, Snapshot};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Reescribir el fichero entero cada tantas líneas o bytes del diario.
const REWRITE_LINES: usize = 1000;
const REWRITE_BYTES: usize = 64 << 20;

/// Mensaje cuando otra ventana (o el servidor) tiene los datos abiertos.
pub const IN_USE: &str = "Los datos ya están abiertos en otra ventana de VeodarDental (o en el servidor de datos de este ordenador): esta ventana no guarda. Ciérrela y use la otra.";

/// Bloquea los datos de `data` para este programa mientras viva el fichero devuelto.
pub fn lock(data: &Path) -> Result<File, String> {
    if let Some(d) = data.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let f = OpenOptions::new().create(true).truncate(false).write(true).open(data.with_extension("bloqueo")).map_err(|e| e.to_string())?;
    match f.try_lock() {
        Ok(()) => Ok(f),
        Err(std::fs::TryLockError::WouldBlock) => Err(IN_USE.into()),
        Err(std::fs::TryLockError::Error(e)) => Err(format!("No se pueden bloquear los datos: {e}")),
    }
}

/// Los datos de este ordenador abiertos para guardar.
pub struct LocalStore {
    path: PathBuf,
    _lock: File,
    journal: Option<File>,
    lines: usize,
    bytes: usize,
    /// Historias tocadas en el último guardado (para el registro de accesos).
    pub touched: std::collections::BTreeMap<u32, std::collections::BTreeSet<String>>,
    /// Revisión de lo que hay en el disco (fichero y diario).
    rev: u64,
    /// Cómo están los datos en el disco, registro a registro.
    synced: Snapshot,
}

impl LocalStore {
    /// Con los datos recién abiertos (y bloqueados). Si aún no hay fichero
    /// (la primera vez), lo crea: el diario solo vale junto a él.
    pub fn new(path: PathBuf, lock: File, db: &mut Db) -> Result<LocalStore, String> {
        if !path.exists() {
            Db::write_full(db, &path)?;
        }
        let synced = Snapshot::from_db(db);
        sync::mark_synced(db);
        Ok(LocalStore { path, _lock: lock, journal: None, lines: 0, bytes: 0, rev: db.rev, synced, touched: Default::default() })
    }

    /// Guarda lo que ha cambiado en `db` (si nada, no escribe nada). Devuelve
    /// cuántos registros o campos han cambiado.
    pub fn save(&mut self, db: &mut Db) -> Result<usize, String> {
        let changes = sync::diff_db(&self.synced, db);
        if changes.is_empty() {
            sync::mark_synced(db);
            return Ok(0);
        }
        self.touched = crate::accesos::patients_touched(&changes);
        // la revisión nunca baja (aunque se haya restaurado una copia antigua):
        // si no, al abrir se descartaría lo apuntado
        let rev = self.rev.max(db.rev) + 1;
        let mut line = crate::datos_cifrados::line_to_disk(&sync::journal_line_of(rev, &changes))?;
        line.push('\n');
        if self.journal.is_none() {
            self.journal = Some(crate::paths::append_private(&sync::journal_path(&self.path)).map_err(|e| format!("No se puede abrir el diario de cambios: {e}"))?);
        }
        if let Some(f) = self.journal.as_mut() {
            f.write_all(line.as_bytes()).and_then(|_| f.sync_data()).map_err(|e| format!("No se pudo guardar: {e}"))?;
        }
        db.rev = rev;
        self.rev = rev;
        for ch in &changes {
            self.synced.set(&ch.field, ch.key.as_deref(), ch.after.clone());
        }
        sync::mark_synced(db);
        self.lines += 1;
        self.bytes += line.len();
        if self.lines >= REWRITE_LINES || self.bytes > REWRITE_BYTES {
            self.rewrite(db)?;
        }
        Ok(changes.len())
    }

    /// Reescribe el fichero de datos entero y vacía el diario.
    pub fn rewrite(&mut self, db: &Db) -> Result<(), String> {
        Db::write_full(db, &self.path)?;
        self.journal = Some(crate::paths::create_private(&sync::journal_path(&self.path)).map_err(|e| format!("No se puede vaciar el diario de cambios: {e}"))?);
        self.lines = 0;
        self.bytes = 0;
        Ok(())
    }

    /// Al cerrar: el fichero queda completo (si había algo en el diario).
    pub fn close(&mut self, db: &Db) -> Result<(), String> {
        if self.lines > 0 { self.rewrite(db) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ds_local_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.join("clinica.json")
    }

    /// T-F3-02. Datos de una versión más nueva del programa: se leen, pero ni
    /// se reescriben ni se quita su diario (se perderían sus campos nuevos).
    /// De una versión anterior: copia de seguridad antes de completarlos.
    #[test]
    fn newer_data_is_never_rewritten_and_older_data_is_backed_up_first() {
        let path = dir("versiones");
        let db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap());
        let mut v: serde_json::Value = serde_json::to_value(&db).unwrap();
        v["data_version"] = (crate::model::DATA_VERSION + 1).into();
        v["patients"][0]["campo_de_la_version_nueva"] = "no se pierde".into();
        let text = serde_json::to_string(&v).unwrap();
        std::fs::write(&path, &text).unwrap();
        let journal = crate::sync::journal_path(&path);
        std::fs::write(&journal, "").unwrap();
        let (read, err) = Db::load_from(&path, true);
        assert!(err.as_deref().is_some_and(|e| e.contains("versión más nueva")), "{err:?}");
        assert_eq!(read.patients.len(), db.patients.len(), "se pueden ver");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "el fichero no se toca");
        assert!(journal.exists(), "ni su diario");
        // de una versión anterior: copia antes de completarlos
        let mut old = serde_json::to_value(&db).unwrap();
        old["data_version"] = 0.into();
        std::fs::write(&path, serde_json::to_string(&old).unwrap()).unwrap();
        let before: Vec<_> = crate::tools::list_backups(&crate::tools::backup_dir()).into_iter().map(|b| b.path).collect();
        let (read, err) = Db::load_from(&path, true);
        assert_eq!((err, read.data_version), (None, crate::model::DATA_VERSION));
        let made: Vec<_> = crate::tools::list_backups(&crate::tools::backup_dir()).into_iter().filter(|b| !before.contains(&b.path) && b.path.to_string_lossy().contains("antes_de_actualizar")).collect();
        assert_eq!(made.len(), 1, "una copia previa");
        assert_eq!(crate::tools::read_backup(&made[0].path, None).unwrap().data_version, 0, "con los datos como estaban");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// T-F4-02, un solo ordenador: cada guardado dice qué historias tocó y en
    /// qué parte (para el registro de accesos), sin su contenido.
    #[test]
    fn each_save_tells_which_histories_it_touched() {
        let path = dir("historias");
        let mut db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap());
        Db::write_full(&db, &path).unwrap();
        let mut st = LocalStore::new(path.clone(), lock(&path).unwrap(), &mut db).unwrap();
        db.patients.iter_mut().find(|p| p.id == 101).unwrap().allergies = "Látex".into();
        let e = db.entries.iter_mut().find(|e| e.patient_id == 102).unwrap();
        e.notes = "revisar".into();
        db.expenses.clear();
        st.save(&mut db).unwrap();
        let t: Vec<(u32, Vec<String>)> = st.touched.iter().map(|(p, s)| (*p, s.iter().cloned().collect())).collect();
        assert_eq!(t, vec![(101, vec!["ficha".to_string()]), (102, vec!["tratamientos".to_string()])]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn saves_only_what_changed_and_comes_back_after_a_crash() {
        let path = dir("save");
        let mut db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap());
        Db::write_full(&db, &path).unwrap();
        let size = std::fs::metadata(&path).unwrap().len();
        let mut st = LocalStore::new(path.clone(), lock(&path).unwrap(), &mut db).unwrap();
        assert_eq!(st.save(&mut db).unwrap(), 0, "sin cambios no se escribe nada");
        db.appointments[0].notes = "llamar antes".into();
        let pays = db.payments.len();
        db.add_payment(100, 30.0, "Efectivo", "");
        assert!(st.save(&mut db).unwrap() >= 2);
        // el fichero no se ha reescrito: el cambio está en el diario, que es pequeño
        assert_eq!(std::fs::metadata(&path).unwrap().len(), size);
        assert!(std::fs::metadata(sync::journal_path(&path)).unwrap().len() < 4000);
        // el programa se cae sin cerrar: al abrir, todo está
        drop(st);
        let (back, err) = Db::load_from(&path, true);
        assert!(err.is_none(), "{err:?}");
        assert_eq!(back.appointments[0].notes, "llamar antes");
        assert_eq!(back.payments.len(), pays + 1);
        assert!(!sync::journal_path(&path).exists(), "al abrir, el fichero queda completo y el diario se quita");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn restoring_an_old_backup_is_not_lost() {
        let path = dir("restore");
        let mut db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap());
        db.rev = 100;
        Db::write_full(&db, &path).unwrap();
        let mut st = LocalStore::new(path.clone(), lock(&path).unwrap(), &mut db).unwrap();
        // se restaura una copia antigua (revisión más baja) sin cerrar después
        let mut old: Db = serde_json::from_str(&serde_json::to_string(&db).unwrap()).unwrap();
        old.rev = 40;
        old.patients[0].first_name = "De la copia".into();
        let mut db = old;
        st.save(&mut db).unwrap();
        assert!(db.rev > 100);
        drop(st);
        let (back, _) = Db::load_from(&path, true);
        assert_eq!(back.patients[0].first_name, "De la copia");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_first_time_the_file_is_created() {
        let path = dir("first");
        let mut db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap());
        let mut st = LocalStore::new(path.clone(), lock(&path).unwrap(), &mut db).unwrap();
        assert!(path.exists());
        db.patients[0].first_name = "Primera".into();
        st.save(&mut db).unwrap();
        drop(st);
        assert_eq!(Db::load_from(&path, true).0.patients[0].first_name, "Primera");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn only_one_window_saves() {
        let path = dir("lock");
        let first = lock(&path).unwrap();
        assert_eq!(lock(&path).unwrap_err(), IN_USE);
        drop(first);
        // (otra prueba de este proceso puede estar lanzando un programa: hasta que
        // arranca, el hijo tiene una copia de los ficheros abiertos, también este;
        // fuera de las pruebas no pasa, pero aquí se espera un poco)
        let reopened = (0..100).find_map(|_| {
            lock(&path).ok().or_else(|| {
                std::thread::sleep(std::time::Duration::from_millis(10));
                None
            })
        });
        assert!(reopened.is_some(), "al cerrarse la primera, se puede abrir");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn closing_leaves_the_file_complete() {
        let path = dir("close");
        let mut db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap());
        Db::write_full(&db, &path).unwrap();
        let mut st = LocalStore::new(path.clone(), lock(&path).unwrap(), &mut db).unwrap();
        db.patients[0].mobile = "600000000".into();
        st.save(&mut db).unwrap();
        st.close(&db).unwrap();
        assert_eq!(std::fs::metadata(sync::journal_path(&path)).unwrap().len(), 0);
        let on_file: Db = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_file.patients[0].mobile, "600000000");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
