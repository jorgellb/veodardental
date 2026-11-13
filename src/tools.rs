//! Copias de seguridad, listín telefónico y consultas (segmentación) de pacientes.

use crate::model::*;
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ───────────────────────────── Copias de seguridad ─────────────────────────────

pub fn backup_dir() -> PathBuf {
    Db::path().parent().map(|p| p.join("copias")).unwrap_or_else(|| PathBuf::from("copias"))
}

#[derive(Clone, Debug)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub when: NaiveDateTime,
    pub bytes: u64,
    /// Cifrada con la contraseña de las copias.
    pub encrypted: bool,
}

fn backup_name(when: NaiveDateTime, reason: &str, ext: &str) -> String {
    let r: String = fold(reason).chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("clinica_{}_{}.{ext}", when.format("%Y%m%d_%H%M%S"), r.trim_matches('_'))
}

/// Escribe el fichero entero o nada (primero a un temporal y luego se renombra).
fn write_whole(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    crate::paths::write_private(&tmp, bytes).map_err(|e| format!("No se puede escribir la copia: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("No se puede escribir la copia: {e}"))
}

/// Guarda una copia completa de los datos en `dir`, cifrada si se da la clave.
fn backup_in(db: &Db, dir: &Path, reason: &str, key: Option<&crate::cifrado::BackupKey>) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("No se puede crear la carpeta de copias: {e}"))?;
    let txt = serde_json::to_string(db).map_err(|e| e.to_string())?;
    let (path, bytes) = match key {
        Some(k) => (dir.join(backup_name(now(), reason, crate::cifrado::EXT)), crate::cifrado::encrypt(txt.as_bytes(), k)?),
        None => (dir.join(backup_name(now(), reason, "json")), txt.into_bytes()),
    };
    write_whole(&path, &bytes)?;
    prune(dir, now());
    Ok(path)
}

/// Guarda una copia completa de los datos en `dir` (para las pruebas).
#[cfg(test)]
pub fn create_backup_in(db: &Db, dir: &Path, reason: &str) -> Result<PathBuf, String> {
    backup_in(db, dir, reason, None)
}

/// Copia en la carpeta de copias de este ordenador y, si está configurada, la
/// segunda copia fuera de él (disco USB o carpeta en la nube), que va cifrada
/// cuando hay contraseña de copias.
pub fn create_backup(db: &Db, reason: &str) -> Result<PathBuf, String> {
    create_backup_to(db, &backup_dir(), db.config.backup_extra.trim(), crate::cifrado::current_key().as_ref(), reason)
}

pub fn create_backup_to(db: &Db, local: &Path, extra: &str, key: Option<&crate::cifrado::BackupKey>, reason: &str) -> Result<PathBuf, String> {
    let p = backup_in(db, local, reason, None)?;
    if !extra.is_empty() {
        backup_in(db, Path::new(extra), reason, key).map_err(|e| format!("Copia local hecha, pero falló la segunda copia: {e}"))?;
    }
    Ok(p)
}

pub fn list_backups(dir: &Path) -> Vec<BackupInfo> {
    let mut v: Vec<BackupInfo> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    let name = p.file_name()?.to_string_lossy().to_string();
                    let encrypted = name.ends_with(&format!(".{}", crate::cifrado::EXT));
                    if !name.starts_with("clinica_") || !(name.ends_with(".json") || encrypted) {
                        return None;
                    }
                    let when = NaiveDateTime::parse_from_str(name.get(8..23)?, "%Y%m%d_%H%M%S").ok()?;
                    Some(BackupInfo { bytes: e.metadata().map(|m| m.len()).unwrap_or(0), path: p, when, encrypted })
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|b| std::cmp::Reverse(b.when));
    v
}

/// Copias que se guardan. Las automáticas, escalonadas: todas las de los
/// últimos 14 días, la última de cada semana de los últimos 3 meses y la
/// última de cada mes del último año (así, un error que se descubre tarde aún
/// tiene arreglo). Las demás (manuales, «antes de importar»…), las 10 últimas,
/// aparte, para que no echen a las automáticas.
const KEEP_DAYS: i64 = 14;
const KEEP_WEEKS_DAYS: i64 = 91;
const KEEP_MONTHS_DAYS: i64 = 366;
const KEEP_OTHERS: usize = 10;

/// Copia hecha sola cada día (en un puesto o en el servidor).
fn is_automatic(b: &BackupInfo) -> bool {
    b.path.file_name().map(|n| n.to_string_lossy().to_string()).and_then(|n| n.get(24..).map(|r| r.starts_with("automatica"))).unwrap_or(false)
}

/// Las copias de `list` (de la más nueva a la más antigua) que se quedan.
fn to_keep(list: &[BackupInfo], now: NaiveDateTime) -> std::collections::HashSet<PathBuf> {
    let mut keep = std::collections::HashSet::new();
    let (mut weeks, mut months) = (std::collections::HashSet::new(), std::collections::HashSet::new());
    for b in list.iter().filter(|b| is_automatic(b)) {
        let age = (now - b.when).num_days();
        let week = (b.when.date().iso_week().year(), b.when.date().iso_week().week());
        let month = (b.when.year(), b.when.month());
        let fresh_week = weeks.insert(week);
        let fresh_month = months.insert(month);
        if age <= KEEP_DAYS || (age <= KEEP_WEEKS_DAYS && fresh_week) || (age <= KEEP_MONTHS_DAYS && fresh_month) {
            keep.insert(b.path.clone());
        }
    }
    keep.extend(list.iter().filter(|b| !is_automatic(b)).take(KEEP_OTHERS).map(|b| b.path.clone()));
    keep
}

fn prune(dir: &Path, now: NaiveDateTime) {
    let list = list_backups(dir);
    let keep = to_keep(&list, now);
    for b in list.into_iter().filter(|b| !keep.contains(&b.path)) {
        let _ = std::fs::remove_file(b.path);
    }
}

/// Cómo van las copias de seguridad automáticas (en red, las del servidor).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BackupStatus {
    /// La última copia que hay.
    #[serde(rename = "ultima", default)]
    pub last_ok: Option<NaiveDateTime>,
    /// Si la última copia automática falló, por qué.
    #[serde(default)]
    pub error: Option<String>,
}

impl BackupStatus {
    /// La última copia que hay en `dir`.
    pub fn from_dir(dir: &Path) -> Self {
        BackupStatus { last_ok: list_backups(dir).first().map(|b| b.when), error: None }
    }

    /// Apunta cómo ha ido una copia.
    pub fn record(&mut self, r: &Result<PathBuf, String>, when: NaiveDateTime) {
        match r {
            Ok(_) => {
                self.last_ok = Some(when);
                self.error = None;
            }
            Err(e) => self.error = Some(e.clone()),
        }
    }

    /// Aviso para Dirección (`None` si las copias van bien).
    pub fn warning(&self, now: NaiveDateTime) -> Option<String> {
        let last = self.last_ok.map(|t| format!(" La última copia es del {}.", t.format("%d/%m/%Y %H:%M"))).unwrap_or_else(|| " No hay ninguna copia.".into());
        if let Some(e) = &self.error {
            return Some(format!("La copia de seguridad ha fallado: {e}.{last}"));
        }
        match self.last_ok {
            None => Some("No hay ninguna copia de seguridad de los datos.".into()),
            // se hace cada día que se trabaja: tres días sin copia (más que un fin de semana) es raro
            Some(t) if (now - t).num_days() > 3 => Some(format!("Hace {} días que no se hace copia de seguridad.{last}", (now - t).num_days())),
            _ => None,
        }
    }
}

/// Mensaje cuando una copia cifrada necesita la contraseña.
pub const NEEDS_PASSWORD: &str = "Esta copia está cifrada: escriba la contraseña de las copias para abrirla.";

/// Lee y comprueba una copia antes de restaurarla. Las cifradas se abren con
/// la clave de este ordenador o, si no sirve, con la contraseña.
pub fn read_backup(path: &Path, password: Option<&str>) -> Result<Db, String> {
    read_backup_with(path, password, crate::cifrado::current_key().as_ref())
}

pub fn read_backup_with(path: &Path, password: Option<&str>, key: Option<&crate::cifrado::BackupKey>) -> Result<Db, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("No se puede leer la copia: {e}"))?;
    let plain = if crate::cifrado::is_encrypted(&bytes) {
        match (key.and_then(|k| crate::cifrado::decrypt_with_key(&bytes, k)), password) {
            (Some(p), _) => p,
            (None, Some(pw)) => crate::cifrado::decrypt(&bytes, pw)?,
            (None, None) => return Err(NEEDS_PASSWORD.into()),
        }
    } else {
        bytes
    };
    let txt = String::from_utf8(plain).map_err(|_| "La copia está dañada o no es de esta aplicación.".to_string())?;
    let mut db = serde_json::from_str::<Db>(&txt).map_err(|e| format!("La copia está dañada o no es de esta aplicación: {e}"))?;
    // copia hecha en un puesto conectado: sin contraseñas (si se restaura en
    // red, el servidor conserva las suyas)
    db.reset_hidden_credentials();
    Ok(db)
}

/// Pone las copias de `dir` con la clave nueva: cifra las que no lo están y
/// pasa a la clave nueva las de la clave anterior. Cada copia se escribe y se
/// comprueba antes de borrar la vieja; las que no se pueden abrir (otra
/// contraseña) se quedan como están. Devuelve cuántas se cambiaron.
pub fn rekey_folder(dir: &Path, old: Option<&crate::cifrado::BackupKey>, new: &crate::cifrado::BackupKey) -> Result<usize, String> {
    let mut n = 0;
    for b in list_backups(dir) {
        let Ok(bytes) = std::fs::read(&b.path) else { continue };
        let plain = if crate::cifrado::is_encrypted(&bytes) {
            if crate::cifrado::decrypt_with_key(&bytes, new).is_some() {
                continue; // ya está con la clave nueva
            }
            match old.and_then(|k| crate::cifrado::decrypt_with_key(&bytes, k)) {
                Some(p) => p,
                None => continue,
            }
        } else {
            // solo copias de verdad de este programa
            if serde_json::from_slice::<Db>(&bytes).is_err() {
                continue;
            }
            bytes
        };
        let target = b.path.with_extension(crate::cifrado::EXT);
        let sealed = crate::cifrado::encrypt(&plain, new)?;
        let tmp = target.with_extension("tmp");
        crate::paths::write_private(&tmp, &sealed).map_err(|e| format!("No se pudo cifrar {}: {e}", b.path.display()))?;
        // comprobar lo escrito antes de quitar la copia vieja
        let check = std::fs::read(&tmp).ok().and_then(|w| crate::cifrado::decrypt_with_key(&w, new));
        if check.as_deref() != Some(plain.as_slice()) {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("No se pudo comprobar la copia cifrada de {}.", b.path.display()));
        }
        std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
        if target != b.path {
            std::fs::remove_file(&b.path).map_err(|e| format!("La copia cifrada está hecha, pero no se pudo borrar {}: {e}", b.path.display()))?;
        }
        n += 1;
    }
    Ok(n)
}

pub fn summary(db: &Db) -> String {
    format!("{} pacientes · {} citas · {} tratamientos · {} cobros · {} facturas", db.patients.len(), db.appointments.len(), db.entries.len(), db.payments.len(), db.invoices.len())
}

/// ¿Hace falta la copia automática del día?
pub fn needs_daily(today: NaiveDate) -> bool {
    !list_backups(&backup_dir()).iter().any(|b| b.when.date() == today)
}

// ───────────────────────────── Listín ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Contact {
    pub id: u32,
    pub name: String,
    pub company: String,
    pub category: String,
    pub phone: String,
    pub mobile: String,
    pub email: String,
    pub notes: String,
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub kind: &'static str,
    pub name: String,
    pub detail: String,
    pub phone: String,
    pub email: String,
    pub patient_id: Option<u32>,
    pub contact_id: Option<u32>,
}

impl Db {
    /// Listín unificado: contactos, pacientes, colaboradores y proveedores.
    pub fn directory(&self, q: &str) -> Vec<DirEntry> {
        let q = fold(q.trim());
        let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
        let hit = |texts: &[&str], phones: &[&str]| -> bool {
            if q.is_empty() {
                return true;
            }
            let joined = fold(&texts.join(" "));
            q.split_whitespace().all(|w| joined.contains(w)) || (digits.len() >= 3 && phones.iter().any(|p| p.replace(' ', "").contains(&digits)))
        };
        let mut v = vec![];
        for c in &self.contacts {
            if hit(&[&c.name, &c.company, &c.category], &[&c.phone, &c.mobile]) {
                v.push(DirEntry {
                    kind: "Contacto",
                    name: c.name.clone(),
                    detail: format!("{} {}", c.category, if c.company.is_empty() { String::new() } else { format!("· {}", c.company) }),
                    phone: if c.phone.is_empty() { c.mobile.clone() } else { c.phone.clone() },
                    email: c.email.clone(),
                    patient_id: None,
                    contact_id: Some(c.id),
                });
            }
        }
        for c in &self.collaborators {
            if hit(&[&c.name, &c.alias, &c.specialty], &[&c.phone]) {
                v.push(DirEntry { kind: "Colaborador", name: c.name.clone(), detail: c.specialty.clone(), phone: c.phone.clone(), email: String::new(), patient_id: None, contact_id: None });
            }
        }
        for s in &self.suppliers {
            if hit(&[&s.name, &s.contact], &[&s.phone]) {
                v.push(DirEntry { kind: if s.is_lab { "Laboratorio" } else { "Proveedor" }, name: s.name.clone(), detail: s.contact.clone(), phone: s.phone.clone(), email: s.email.clone(), patient_id: None, contact_id: None });
            }
        }
        for t in self.tariffs.iter().filter(|t| t.is_insurer && !t.phone.is_empty()) {
            if hit(&[&t.name], &[&t.phone]) {
                v.push(DirEntry { kind: "Mutua", name: t.name.clone(), detail: t.notes.clone(), phone: t.phone.clone(), email: String::new(), patient_id: None, contact_id: None });
            }
        }
        if !q.is_empty() {
            for p in &self.patients {
                if hit(&[&p.first_name, &p.last_name, &p.history_no.to_string()], &[&p.mobile, &p.phone]) {
                    v.push(DirEntry { kind: "Paciente", name: p.full_name(), detail: format!("Nº {}", p.history_no), phone: p.mobile.clone(), email: p.email.clone(), patient_id: Some(p.id), contact_id: None });
                }
            }
        }
        v.truncate(300);
        v
    }
}

// ───────────────────────────── Consultas ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    pub age_min: Option<u32>,
    pub age_max: Option<u32>,
    pub sex: Option<String>,
    pub tariff: Option<u32>,
    pub doctor: Option<u32>,
    pub no_visit_months: Option<u32>,
    pub min_debt: Option<f64>,
    pub pending_budget: bool,
    pub treatment: Option<String>,
    pub treatment_state: Option<TxState>,
    pub city: String,
    pub channel_ok: Option<&'static str>,
    pub only_active: bool,
}

impl Db {
    pub fn run_query(&self, q: &Query, today: NaiveDate) -> Vec<u32> {
        let city = fold(&q.city);
        self.patients
            .iter()
            .filter(|p| !q.only_active || p.active)
            .filter(|p| {
                let age = p.age(today);
                q.age_min.map(|m| age.map(|a| a >= m as i32).unwrap_or(false)).unwrap_or(true) && q.age_max.map(|m| age.map(|a| a <= m as i32).unwrap_or(false)).unwrap_or(true)
            })
            .filter(|p| q.sex.as_ref().map(|s| &p.sex == s).unwrap_or(true))
            .filter(|p| q.tariff.map(|t| p.tariff_id == t).unwrap_or(true))
            .filter(|p| q.doctor.map(|d| p.doctor_id == d).unwrap_or(true))
            .filter(|p| city.is_empty() || fold(&p.city).contains(&city))
            .filter(|p| match q.no_visit_months {
                None => true,
                Some(m) => {
                    let limit = add_months(today, 0) - chrono::Duration::days(m as i64 * 30);
                    self.last_visit(p.id).map(|d| d < limit).unwrap_or(true)
                }
            })
            .filter(|p| q.min_debt.map(|d| self.patient_balance(p.id) >= d).unwrap_or(true))
            .filter(|p| !q.pending_budget || self.budgets.iter().any(|b| b.patient_id == p.id && b.state == BudgetState::Pendiente))
            .filter(|p| match &q.treatment {
                None => true,
                Some(code) => self.entries.iter().any(|e| e.patient_id == p.id && &e.code == code && q.treatment_state.map(|s| e.state == s).unwrap_or(true)),
            })
            .filter(|p| match q.channel_ok {
                None => true,
                Some("SMS") => p.accepts_gdpr && p.accepts_sms && !p.mobile.is_empty(),
                Some("WhatsApp") => p.accepts_gdpr && p.accepts_whatsapp && !p.mobile.is_empty(),
                Some(_) => p.accepts_gdpr && p.accepts_email && !p.email.is_empty(),
            })
            .map(|p| p.id)
            .collect()
    }

    pub fn query_csv(&self, ids: &[u32], today: NaiveDate) -> String {
        let mut s = String::from("Nº historial;Apellidos;Nombre;NIF;Edad;Móvil;Email;Población;Tarifa;Doctor;Última visita;Saldo\n");
        for id in ids {
            if let Some(p) = self.patient(*id) {
                s.push_str(&format!(
                    "{};{};{};{};{};{};{};{};{};{};{};{}\n",
                    p.history_no,
                    p.last_name,
                    p.first_name,
                    p.nif,
                    p.age(today).map(|a| a.to_string()).unwrap_or_default(),
                    p.mobile,
                    p.email,
                    p.city,
                    self.tariff_name(p.tariff_id),
                    self.doctor_alias(p.doctor_id),
                    self.last_visit(p.id).map(fmt_date).unwrap_or_default(),
                    format!("{:.2}", self.patient_balance(p.id)).replace('.', ",")
                ));
            }
        }
        s
    }
}

pub fn sample_contacts(db: &mut Db) {
    let c =
        |id: u32, name: &str, company: &str, cat: &str, phone: &str, email: &str| Contact { id, name: name.into(), company: company.into(), category: cat.into(), phone: phone.into(), mobile: String::new(), email: email.into(), notes: String::new() };
    *db.contacts = vec![
        c(1, "Servicio técnico de equipos", "Técnico Ejemplo", "Mantenimiento", "900 300 300", "sat@tecnico-ejemplo.es"),
        c(2, "Gestoría", "Gestoría Ejemplo", "Administración", "91 500 50 50", "info@gestoria-ejemplo.es"),
        c(3, "Colegio de Dentistas", "", "Institucional", "91 600 60 60", ""),
        c(4, "Recogida de residuos sanitarios", "Residuos Ejemplo", "Proveedores", "900 700 700", ""),
    ];
    let _ = Datelike::year(&today());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()
    }

    #[test]
    fn backup_roundtrip_and_prune() {
        let dir = std::env::temp_dir().join(format!("ds_bk_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::sample(d());
        let p = create_backup_in(&db, &dir, "manual").unwrap();
        let back = read_backup(&p, None).unwrap();
        assert_eq!(back.patients.len(), db.patients.len());
        assert_eq!(summary(&back), summary(&db));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn automatic_copies_are_kept_staggered_for_a_year() {
        let dir = std::env::temp_dir().join(format!("ds_bk_keep_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let now = d().and_hms_opt(20, 0, 0).unwrap();
        // dos años de copias diarias y 15 manuales
        for i in 0..730 {
            let day = d() - chrono::Duration::days(i);
            std::fs::write(dir.join(format!("clinica_{}_090000_automatica.json", day.format("%Y%m%d"))), "{}").unwrap();
        }
        for i in 0..15 {
            std::fs::write(dir.join(format!("clinica_{}_1000{i:02}_antes_de_importar.json", d().format("%Y%m%d"))), "{}").unwrap();
        }
        prune(&dir, now);
        let left = list_backups(&dir);
        let auto: Vec<NaiveDate> = left.iter().filter(|b| is_automatic(b)).map(|b| b.when.date()).collect();
        assert_eq!(left.iter().filter(|b| !is_automatic(b)).count(), 10, "las 10 últimas manuales");
        for i in 0..=14 {
            assert!(auto.contains(&(d() - chrono::Duration::days(i))), "las de los últimos 14 días: falta la de hace {i}");
        }
        // de hace 1 a 3 meses, una por semana; de 3 a 12, una por mes; de hace más de un año, ninguna
        let between = |a: i64, b: i64| auto.iter().filter(|x| (d() - **x).num_days() > a && (d() - **x).num_days() <= b).count();
        assert!((10..=14).contains(&between(14, 91)), "{}", between(14, 91));
        assert!((8..=10).contains(&between(91, 366)), "{}", between(91, 366));
        assert_eq!(between(366, 10_000), 0);
        assert!(auto.len() < 45, "se guardan {} copias automáticas", auto.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_or_old_backup_is_warned() {
        let now = d().and_hms_opt(20, 0, 0).unwrap();
        let mut s = BackupStatus { last_ok: Some(now - chrono::Duration::hours(5)), error: None };
        assert_eq!(s.warning(now), None);
        s.record(&Err("no se encuentra el disco USB".into()), now);
        let w = s.warning(now).unwrap();
        assert!(w.contains("disco USB") && w.contains(&(now - chrono::Duration::hours(5)).format("%d/%m/%Y").to_string()), "{w}");
        s.record(&Ok(PathBuf::from("x")), now);
        assert_eq!(s.warning(now), None, "la siguiente sale bien: sin aviso");
        assert!(s.warning(now + chrono::Duration::days(5)).unwrap().contains("5 días"));
        assert!(BackupStatus::default().warning(now).is_some());
    }

    fn key(pw: &str) -> crate::cifrado::BackupKey {
        crate::cifrado::BackupKey::with_iterations(pw, 1000, now()).unwrap()
    }

    #[test]
    fn second_copy_is_encrypted_with_the_backup_password() {
        let base = std::env::temp_dir().join(format!("ds_bk_enc_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (local, extra) = (base.join("local"), base.join("usb"));
        let db = Db::sample(d());
        let k = key("Sonrisa-2026");
        create_backup_to(&db, &local, &extra.to_string_lossy(), Some(&k), "manual").unwrap();
        // la de este ordenador sigue legible; la de fuera va cifrada
        let l = list_backups(&local);
        let e = list_backups(&extra);
        assert_eq!((l.len(), e.len()), (1, 1));
        assert!(!l[0].encrypted && e[0].encrypted);
        assert!(e[0].path.extension().is_some_and(|x| x == "dscopia"));
        let bytes = std::fs::read(&e[0].path).unwrap();
        let name = &db.patients[0].last_name;
        assert!(!String::from_utf8_lossy(&bytes).contains(name.as_str()), "los datos se leen en la copia cifrada");
        // se abre con la clave de este ordenador, o con la contraseña en otro
        assert_eq!(read_backup_with(&e[0].path, None, Some(&k)).unwrap().patients.len(), db.patients.len());
        assert_eq!(read_backup_with(&e[0].path, None, None).unwrap_err(), NEEDS_PASSWORD);
        assert!(read_backup_with(&e[0].path, Some("otra"), None).is_err());
        assert_eq!(read_backup_with(&e[0].path, Some("Sonrisa-2026"), None).unwrap().patients.len(), db.patients.len());
        assert!(crate::cifrado::is_encrypted_file(&e[0].path) && !crate::cifrado::is_encrypted_file(&l[0].path));
        // sin contraseña de copias, la segunda copia sale como siempre
        create_backup_to(&db, &local, &extra.to_string_lossy(), None, "sin clave").unwrap();
        assert_eq!(list_backups(&extra).iter().filter(|b| !b.encrypted).count(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn old_copies_are_encrypted_and_moved_to_a_new_password() {
        let dir = std::env::temp_dir().join(format!("ds_bk_rekey_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::sample(d());
        create_backup_in(&db, &dir, "vieja sin cifrar").unwrap();
        std::fs::write(dir.join("clinica_20200101_000000_ajena.json"), "{\"no\": \"es una copia\"}").unwrap();
        let first = key("Primera-clave");
        // la primera vez: se cifran las que había sin cifrar (las que no son copias se dejan)
        assert_eq!(rekey_folder(&dir, None, &first).unwrap(), 1);
        let list = list_backups(&dir);
        assert_eq!(list.iter().filter(|b| b.encrypted).count(), 1);
        assert!(list.iter().any(|b| !b.encrypted && b.path.to_string_lossy().contains("ajena")));
        // con la clave puesta, otra copia más, ya cifrada
        create_backup_to(&db, &dir.join("local"), &dir.to_string_lossy(), Some(&first), "diaria").unwrap();
        // cambiar la contraseña: todas pasan a la nueva, también sin saber la vieja
        let second = key("Segunda-clave");
        assert_eq!(rekey_folder(&dir, Some(&first), &second).unwrap(), 2);
        for b in list_backups(&dir).into_iter().filter(|b| b.encrypted) {
            assert!(read_backup_with(&b.path, Some("Segunda-clave"), None).is_ok());
            assert!(read_backup_with(&b.path, Some("Primera-clave"), None).is_err());
        }
        // una copia de otra contraseña que no se conoce se queda como está
        let stranger = key("De-otro-ordenador");
        let foreign = dir.join("clinica_20200102_000000_otra.dscopia");
        std::fs::write(&foreign, crate::cifrado::encrypt(serde_json::to_string(&db).unwrap().as_bytes(), &stranger).unwrap()).unwrap();
        assert_eq!(rekey_folder(&dir, Some(&first), &second).unwrap(), 0);
        assert!(read_backup_with(&foreign, Some("De-otro-ordenador"), None).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn directory_and_queries() {
        let db = Db::sample(d());
        assert!(db.directory("gestoria").iter().any(|e| e.kind == "Contacto"));
        assert!(db.directory("Navarro").iter().any(|e| e.kind == "Colaborador"));
        assert!(db.directory("").iter().all(|e| e.kind != "Paciente"));
        let q = Query { tariff: Some(2), ..Default::default() };
        let ids = db.run_query(&q, d());
        assert!(!ids.is_empty() && ids.iter().all(|id| db.patient(*id).unwrap().tariff_id == 2));
        let q2 = Query { pending_budget: true, ..Default::default() };
        assert!(db.run_query(&q2, d()).iter().all(|id| db.budgets.iter().any(|b| b.patient_id == *id && b.state == BudgetState::Pendiente)));
        let q3 = Query { age_max: Some(17), ..Default::default() };
        assert!(db.run_query(&q3, d()).iter().all(|id| db.patient(*id).unwrap().age(d()).unwrap() <= 17));
        assert!(db.query_csv(&ids, d()).lines().count() == ids.len() + 1);
    }
}
