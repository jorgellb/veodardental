//! Trabajo en varios puestos (ordenadores) con una carpeta compartida.
//!
//! Todos los puestos usan el mismo fichero de datos en una carpeta de red.
//! Para que no se pisen:
//! * cada puesto tiene un número (1-15) y los registros nuevos reciben
//!   identificadores que nunca coinciden entre puestos;
//! * al guardar se bloquea el fichero un instante;
//! * si otro puesto guardó entretanto, se combinan los cambios registro a
//!   registro (no se sobrescriben);
//! * los puestos sin cambios pendientes recargan solos lo que guardan los demás.

use crate::model::Db;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Ajustes locales de este ordenador (no van en el fichero compartido).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Station {
    /// Servidor de la clínica (IP:puerto); si está, los datos van al servidor.
    pub server: String,
    /// Clave de la clínica para conectar con el servidor.
    pub key: String,
    /// Carpeta compartida con los datos; vacía = datos en este ordenador.
    pub data_dir: String,
    /// Número de puesto (1-15), distinto en cada ordenador.
    pub number: u32,
    pub name: String,
    /// Clave pública del servidor (hexadecimal), apuntada al conectar la
    /// primera vez: después el puesto no acepta otro servidor.
    pub server_pin: String,
}

/// Puesto visto por última vez (se guarda en los datos compartidos).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StationSeen {
    pub id: u32,
    pub name: String,
    pub last_seen: NaiveDateTime,
}

fn settings_path() -> PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("puesto.json")
}

pub fn load_station() -> Station {
    std::fs::read_to_string(settings_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_station(s: &Station) -> Result<(), String> {
    let p = settings_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

static STATION: OnceLock<Station> = OnceLock::new();

/// Ajustes con los que arrancó este puesto (los cambios valen tras reiniciar).
pub fn station() -> &'static Station {
    STATION.get_or_init(load_station)
}

/// Puesto conectado a un servidor de la clínica.
pub fn remote_mode() -> bool {
    !station().server.trim().is_empty()
}

/// Puesto que usa una carpeta compartida (sin servidor).
pub fn shared() -> bool {
    !remote_mode() && !station().data_dir.trim().is_empty()
}

/// Nombre con el que este ordenador se presenta al servidor.
pub fn machine_name() -> String {
    let s = station();
    if !s.name.trim().is_empty() {
        return s.name.trim().to_string();
    }
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "Puesto".into())
}

pub fn station_label() -> String {
    let s = station();
    if s.name.trim().is_empty() { format!("Puesto {}", s.number) } else { format!("{} (puesto {})", s.name, s.number) }
}

// ───────────────────────────── bloqueo ─────────────────────────────

pub struct Lock(PathBuf);

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Bloquea el fichero de datos mientras se guarda. Un bloqueo de más de 20 s
/// se considera abandonado (p. ej. un puesto que se apagó al guardar).
pub fn acquire(dir: &Path, who: &str) -> Result<Lock, String> {
    let path = dir.join("clinica.lock");
    for _ in 0..60 {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                use std::io::Write;
                let _ = writeln!(f, "{who}");
                return Ok(Lock(path));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let stale = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() > 20);
                if stale {
                    let _ = std::fs::remove_file(&path);
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
            Err(e) => return Err(format!("No se puede escribir en la carpeta de datos: {e}")),
        }
    }
    Err("Otro puesto está guardando; se reintentará en un momento.".into())
}

// ───────────────────────────── combinación ─────────────────────────────

fn id_of(v: &Value) -> Option<String> {
    v.as_object().and_then(|o| o.get("id")).map(|id| id.to_string())
}

fn keyed(arr: &[Value]) -> bool {
    arr.iter().all(|v| v.as_object().is_some_and(|o| o.contains_key("id")))
}

/// Registros por su identificador (si se repite, vale el primero).
fn index(arr: &[Value]) -> HashMap<String, &Value> {
    let mut m = HashMap::with_capacity(arr.len());
    for v in arr {
        if let Some(id) = id_of(v) {
            m.entry(id).or_insert(v);
        }
    }
    m
}

/// Listas a las que solo se añade (registros de accesos, de auditoría, de
/// programas abiertos y de WhatsApp ya leídos), con su tamaño máximo. Al
/// combinar se juntan las entradas nuevas de los dos puestos, ninguno puede
/// borrar las del otro y se quitan las más antiguas si pasan del máximo.
const APPEND_ONLY: [(&str, usize); 4] = [("access_log", 2000), ("audit_log", 5000), ("launches", 300), ("wa_seen", 3000)];

/// Dato de los productos del almacén que se combina sumando lo que cambió
/// cada puesto (es el único `stock` de los datos).
const STOCK: &str = "stock";

/// Tamaño máximo de un registro que solo crece (`None` si el campo no es de esos).
pub fn append_only_cap(field: &str) -> Option<usize> {
    APPEND_ONLY.iter().find(|(name, _)| *name == field).map(|(_, cap)| *cap)
}

pub fn merge_append(base: Option<&Value>, ours: &[Value], theirs: &[Value], cap: usize) -> Value {
    let mut seen: HashSet<String> = base.and_then(|b| b.as_array()).into_iter().flatten().chain(theirs).map(|v| v.to_string()).collect();
    let mut out = theirs.to_vec();
    for v in ours {
        if seen.insert(v.to_string()) {
            out.push(v.clone());
        }
    }
    // los registros con hora quedan en orden (la hora se guarda como texto ISO)
    if out.iter().all(|v| v.get("when").is_some_and(|w| w.is_string())) {
        out.sort_by(|a, b| a["when"].as_str().cmp(&b["when"].as_str()));
    }
    if out.len() > cap {
        out.drain(..out.len() - cap);
    }
    Value::Array(out)
}

/// Combinación a tres bandas: `base` es lo que este puesto leyó, `ours` lo que
/// tiene ahora y `theirs` lo que hay en disco (guardado por otro puesto).
/// Las listas de registros con `id` se combinan registro a registro; si los
/// dos puestos cambiaron el mismo dato, gana este puesto y se cuenta.
pub fn merge(base: Option<&Value>, ours: &Value, theirs: &Value, conflicts: &mut usize) -> Value {
    if ours == theirs {
        return ours.clone();
    }
    if base == Some(ours) {
        return theirs.clone();
    }
    if base == Some(theirs) {
        return ours.clone();
    }
    match (ours, theirs) {
        (Value::Object(o), Value::Object(t)) => {
            let b = base.and_then(|b| b.as_object());
            let mut out = Map::new();
            for (k, tv) in t {
                match o.get(k) {
                    Some(ov) => {
                        let bk = b.and_then(|b| b.get(k));
                        let append = APPEND_ONLY.iter().find(|(name, _)| name == k).map(|(_, cap)| *cap);
                        let m = match (append, ov, tv) {
                            (Some(cap), Value::Array(oa), Value::Array(ta)) => merge_append(bk, oa, ta, cap),
                            // las existencias de un producto son un contador: si los dos
                            // puestos gastan (o reciben) a la vez, cuentan los dos
                            (_, Value::Number(on), Value::Number(tn)) if k == STOCK && ov != tv => match (bk.and_then(|b| b.as_f64()), on.as_f64(), tn.as_f64()) {
                                (Some(b), Some(o), Some(t)) => serde_json::Number::from_f64(crate::model::round2(t + o - b)).map(Value::Number).unwrap_or_else(|| tv.clone()),
                                _ => merge(bk, ov, tv, conflicts),
                            },
                            _ => merge(bk, ov, tv, conflicts),
                        };
                        out.insert(k.clone(), m);
                    }
                    None => {
                        out.insert(k.clone(), tv.clone());
                    }
                }
            }
            for (k, ov) in o {
                if !t.contains_key(k) {
                    out.insert(k.clone(), ov.clone());
                }
            }
            Value::Object(out)
        }
        (Value::Array(o), Value::Array(t)) if keyed(o) && keyed(t) => {
            // índices por identificador: combinar es lineal aunque haya miles de registros
            let b = base.and_then(|b| b.as_array()).map(|a| index(a)).unwrap_or_default();
            let (oi, ti) = (index(o), index(t));
            let mut out = Vec::with_capacity(t.len().max(o.len()));
            for tv in t {
                let Some(id) = id_of(tv) else { continue };
                let bv = b.get(&id).copied();
                match oi.get(&id) {
                    Some(ov) => out.push(merge(bv, ov, tv, conflicts)),
                    None => match bv {
                        // este puesto lo borró y el otro no lo tocó: queda borrado
                        Some(bv) if bv == tv => {}
                        // este puesto lo borró pero el otro lo cambió: se conserva
                        Some(_) => {
                            *conflicts += 1;
                            out.push(tv.clone());
                        }
                        // nuevo del otro puesto
                        None => out.push(tv.clone()),
                    },
                }
            }
            for ov in o {
                let Some(id) = id_of(ov) else { continue };
                if ti.contains_key(&id) {
                    continue;
                }
                match b.get(&id) {
                    Some(bv) if *bv == ov => {}
                    Some(_) => {
                        *conflicts += 1;
                        out.push(ov.clone());
                    }
                    None => out.push(ov.clone()),
                }
            }
            Value::Array(out)
        }
        _ => {
            *conflicts += 1;
            ours.clone()
        }
    }
}

pub enum Synced {
    Saved,
    /// Se combinaron cambios de otro puesto (nº de datos en conflicto).
    Merged(usize),
}

fn rev_of(v: &Value) -> u64 {
    v.get("rev").and_then(|r| r.as_u64()).unwrap_or(0)
}

/// Guarda los datos combinándolos con lo que otro puesto haya guardado desde
/// la última lectura. `base` se actualiza con lo que queda en disco.
pub fn save_synced(db: &mut Db, base: &mut Value, path: &Path) -> Result<Synced, String> {
    save_synced_with(db, base, path, |_| ()).map(|(s, _)| s)
}

/// Como [`save_synced`], pero con el fichero bloqueado hace `f` sobre los
/// datos ya al día (con lo de los demás puestos) antes de guardarlos: para lo
/// que tiene que hacer un puesto solo, como numerar una factura.
pub fn save_synced_with<R>(db: &mut Db, base: &mut Value, path: &Path, f: impl FnOnce(&mut Db) -> R) -> Result<(Synced, R), String> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let who = station_label();
    let _lock = acquire(dir, &who)?;
    if db.station > 0 {
        let now = crate::model::now();
        db.stations.retain(|s| s.id != db.station);
        db.stations.push(StationSeen { id: db.station, name: who.clone(), last_seen: now });
    }
    let disk: Option<Value> = match std::fs::read_to_string(path) {
        Ok(t) => Some(serde_json::from_str(&t).map_err(|e| format!("El fichero de datos compartido está dañado: {e}"))?),
        Err(_) => None,
    };
    let outcome = match disk {
        Some(d) if rev_of(&d) != rev_of(base) => {
            let ours = serde_json::to_value(&*db).map_err(|e| e.to_string())?;
            let mut conflicts = 0;
            let mut m = merge(Some(base), &ours, &d, &mut conflicts);
            let next = ours.get("next_id").and_then(|v| v.as_u64()).unwrap_or(0).max(d.get("next_id").and_then(|v| v.as_u64()).unwrap_or(0));
            m["next_id"] = Value::from(next);
            m["rev"] = Value::from(rev_of(&d) + 1);
            let (station, user, center) = (db.station, db.config.current_user.clone(), db.config.current_center);
            *db = serde_json::from_value(m).map_err(|e| format!("No se pudieron combinar los datos: {e}"))?;
            db.station = station;
            db.config.current_user = user;
            db.config.current_center = center;
            // pacientes dados de alta a la vez en dos puestos: el de este puesto,
            // con el siguiente número de historia libre
            let on_disk: std::collections::HashSet<u32> = d["patients"].as_array().into_iter().flatten().filter_map(|p| p["id"].as_u64()).map(|i| i as u32).collect();
            db.fix_history_numbers(|id| on_disk.contains(&id));
            // recordatorios y tareas que este puesto y otro generaron a la vez: los del otro
            let msgs: std::collections::HashSet<u32> = ["messages", "tasks"].iter().flat_map(|f| d[*f].as_array().into_iter().flatten()).filter_map(|m| m["id"].as_u64()).map(|i| i as u32).collect();
            db.drop_auto_duplicates(|id| msgs.contains(&id));
            // cobros de este puesto en una caja que otro acaba de cerrar: a la abierta
            let pays: std::collections::HashSet<u32> = d["payments"].as_array().into_iter().flatten().filter_map(|p| p["id"].as_u64()).map(|i| i as u32).collect();
            db.move_late_payments(|id| !pays.contains(&id));
            Synced::Merged(conflicts)
        }
        _ => {
            db.rev += 1;
            Synced::Saved
        }
    };
    let out = f(db);
    let ours = serde_json::to_value(&*db).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(&ours).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    *base = ours;
    Ok((outcome, out))
}

/// Lee los datos del disco si otro puesto los cambió (revisión distinta).
pub fn reload_if_changed(db: &Db, path: &Path) -> Option<(Db, Value)> {
    let t = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&t).ok()?;
    if rev_of(&v) == db.rev {
        return None;
    }
    let mut new: Db = serde_json::from_value(v.clone()).ok()?;
    new.station = db.station;
    new.config.current_user = db.config.current_user.clone();
    new.config.current_center = db.config.current_center;
    Some((new, v))
}

/// Pone en marcha la carpeta compartida: si no hay datos en ella, copia los
/// de este puesto. Devuelve si se copiaron.
pub fn prepare_folder(db: &Db, dir: &Path) -> Result<bool, String> {
    if !dir.is_dir() {
        return Err("La carpeta no existe o no es accesible desde este ordenador.".into());
    }
    let probe = dir.join(".veodardental_prueba");
    std::fs::write(&probe, b"ok").map_err(|e| format!("No se puede escribir en la carpeta: {e}"))?;
    let _ = std::fs::remove_file(&probe);
    let file = dir.join("clinica.json");
    if file.exists() {
        serde_json::from_str::<Db>(&std::fs::read_to_string(&file).map_err(|e| e.to_string())?).map_err(|e| format!("Ya hay un fichero de datos en la carpeta pero no se puede leer: {e}"))?;
        return Ok(false);
    }
    std::fs::write(&file, serde_json::to_string(db).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use serde_json::json;

    #[test]
    fn merge_keeps_both_sides_changes() {
        let base = json!({"next_id": 10, "patients": [{"id": 1, "name": "Ana", "phone": "1"}, {"id": 2, "name": "Luis", "phone": "2"}]});
        // este puesto cambia el teléfono de Ana y añade a Eva
        let ours = json!({"next_id": 11, "patients": [{"id": 1, "name": "Ana", "phone": "111"}, {"id": 2, "name": "Luis", "phone": "2"}, {"id": 11, "name": "Eva", "phone": "5"}]});
        // el otro cambia el nombre de Ana, borra a Luis y añade a Pablo
        let theirs = json!({"next_id": 12, "patients": [{"id": 1, "name": "Ana María", "phone": "1"}, {"id": 12, "name": "Pablo", "phone": "6"}]});
        let mut c = 0;
        let m = merge(Some(&base), &ours, &theirs, &mut c);
        let ps = m["patients"].as_array().unwrap();
        assert_eq!(c, 1, "solo next_id choca"); // next_id se corrige aparte al guardar
        assert_eq!(ps.len(), 3);
        assert_eq!(ps[0], json!({"id": 1, "name": "Ana María", "phone": "111"}));
        assert!(ps.iter().any(|p| p["name"] == "Pablo") && ps.iter().any(|p| p["name"] == "Eva"));
        assert!(!ps.iter().any(|p| p["name"] == "Luis"));
    }

    #[test]
    fn logs_from_both_stations_are_kept() {
        let e = |t: &str, ev: &str| json!({"when": format!("2026-11-11T{t}"), "user": "Ana", "event": ev, "ok": true});
        let base = json!({"access_log": [e("09:00:00", "Entrada")], "wa_seen": ["m1"]});
        // cada puesto anota lo suyo a la vez
        let ours = json!({"access_log": [e("09:00:00", "Entrada"), e("09:05:00", "Bloqueo A")], "wa_seen": ["m1", "m2"]});
        let theirs = json!({"access_log": [e("09:00:00", "Entrada"), e("09:03:00", "Entrada B"), e("09:07:00", "Salida B")], "wa_seen": ["m1", "m3"]});
        let mut c = 0;
        let m = merge(Some(&base), &ours, &theirs, &mut c);
        let evs: Vec<&str> = m["access_log"].as_array().unwrap().iter().map(|x| x["event"].as_str().unwrap()).collect();
        assert_eq!(evs, ["Entrada", "Entrada B", "Bloqueo A", "Salida B"], "todas, por orden de hora");
        assert_eq!(m["wa_seen"], json!(["m1", "m3", "m2"]));
        assert_eq!(c, 0, "no es un choque");
        // un puesto no puede borrar entradas del registro
        let ours = json!({"access_log": [], "wa_seen": ["m1"]});
        let m = merge(Some(&base), &ours, &theirs, &mut c);
        assert_eq!(m["access_log"].as_array().unwrap().len(), 3);
        // y el registro no pasa de su tamaño máximo (se quitan las más antiguas)
        let many: Vec<Value> = (0..2100).map(|i| json!({"when": format!("2026-11-11T10:{:02}:{:02}", i / 60 % 60, i % 60), "user": "x", "event": format!("e{i}"), "ok": false})).collect();
        let m = merge(Some(&json!({"access_log": []})), &json!({"access_log": many}), &json!({"access_log": [e("09:00:00", "Entrada")]}), &mut c);
        let log = m["access_log"].as_array().unwrap();
        assert_eq!(log.len(), 2000);
        assert_eq!(log.last().unwrap()["event"], "e2099");
    }

    #[test]
    fn merging_big_clinics_is_fast() {
        // una clínica grande: 20 000 registros por lista, cambios en los dos lados
        let rec = |i: u32, v: &str| json!({"id": i, "patient_id": i % 500, "text": format!("entrada {i} {v}"), "price": 35.0});
        let list = |f: &dyn Fn(u32) -> Value| Value::Array((1..=20_000).map(f).collect());
        let base = json!({"entries": list(&|i| rec(i, "")), "payments": list(&|i| rec(i, ""))});
        let ours = json!({"entries": list(&|i| rec(i, if i == 7 { "nuestro" } else { "" })), "payments": list(&|i| rec(i, ""))});
        let theirs = json!({"entries": list(&|i| rec(i, if i == 19_000 { "suyo" } else { "" })), "payments": list(&|i| rec(i, if i == 3 { "suyo" } else { "" }))});
        let t = std::time::Instant::now();
        let mut c = 0;
        let m = merge(Some(&base), &ours, &theirs, &mut c);
        let took = t.elapsed();
        assert_eq!(m["entries"][6]["text"], "entrada 7 nuestro");
        assert_eq!(m["entries"][18_999]["text"], "entrada 19000 suyo");
        assert_eq!(m["payments"][2]["text"], "entrada 3 suyo");
        assert_eq!(c, 0);
        assert!(took < std::time::Duration::from_secs(3), "combinar ha tardado {took:?}");
    }

    #[test]
    fn two_stations_invoicing_in_a_shared_folder_keep_the_chain() {
        let dir = std::env::temp_dir().join(format!("ds_shared_inv_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clinica.json");
        let start = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 6).unwrap());
        prepare_folder(&start, &dir).unwrap();
        let load = || -> (Db, Value) {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            (serde_json::from_value(v.clone()).unwrap(), v)
        };
        // los dos leen los datos antes de que el otro facture
        let (mut a, mut base_a) = load();
        let (mut b, mut base_b) = load();
        a.station = 1;
        b.station = 2;
        let day = crate::model::today();
        let pending: Vec<(u32, u32)> = a.entries.iter().filter(|e| e.state == crate::model::TxState::Realizado && e.invoice_id.is_none()).map(|e| (e.patient_id, e.id)).collect();
        let (p1, e1) = pending[0];
        let (p2, e2) = *pending.iter().find(|(p, _)| *p != p1).unwrap();
        // y a la vez dan de alta un paciente con el mismo número de historia
        let mut pa = a.patients[0].clone();
        pa.id = a.new_id();
        pa.history_no = a.next_history_no();
        a.patients.push(pa.clone());
        let mut pb = b.patients[0].clone();
        pb.id = b.new_id();
        pb.history_no = b.next_history_no();
        b.patients.push(pb.clone());
        let (_, ia) = save_synced_with(&mut a, &mut base_a, &path, |db| db.issue_invoice(p1, &[e1], day)).unwrap();
        let (_, ib) = save_synced_with(&mut b, &mut base_b, &path, |db| db.issue_invoice(p2, &[e2], day)).unwrap();
        let (fin, _) = load();
        let (fa, fb) = (fin.invoice(ia.unwrap()).unwrap(), fin.invoice(ib.unwrap()).unwrap());
        assert_eq!(fb.number, fa.number + 1);
        assert!(fin.verify_chain().is_ok(), "{:?}", fin.verify_chain());
        assert_ne!(fin.patient(pa.id).unwrap().history_no, fin.patient(pb.id).unwrap().history_no);
        assert_eq!(fin.patient(pa.id).unwrap().history_no, pa.history_no, "el primero conserva el suyo");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_payment_made_while_another_station_closes_the_cash_goes_to_the_open_one() {
        let dir = std::env::temp_dir().join(format!("ds_shared_cash_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clinica.json");
        prepare_folder(&Db::sample(NaiveDate::from_ymd_opt(2026, 11, 6).unwrap()), &dir).unwrap();
        let load = || -> (Db, Value) {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            (serde_json::from_value(v.clone()).unwrap(), v)
        };
        let (mut a, mut base_a) = load();
        let (mut b, mut base_b) = load();
        a.station = 1;
        b.station = 2;
        let s1 = a.open_cash().unwrap().id;
        // A cierra con su arqueo y abre la caja siguiente
        let expected = a.cash_expected(a.open_cash().unwrap());
        a.close_cash_session(s1, expected);
        let s2 = a.open_cash_session(100.0);
        save_synced(&mut a, &mut base_a, &path).unwrap();
        // B, que aún no lo sabe, cobra en efectivo: va a la caja que ya está cerrada
        let pay = b.add_payment(100, 20.0, "Efectivo", "tarde");
        assert_eq!(b.payments.iter().find(|p| p.id == pay).unwrap().cash_session, Some(s1));
        save_synced(&mut b, &mut base_b, &path).unwrap();
        let (fin, _) = load();
        assert_eq!(fin.payments.iter().find(|p| p.id == pay).unwrap().cash_session, Some(s2), "a la caja abierta");
        let closed = fin.cash.iter().find(|c| c.id == s1).unwrap();
        assert!((fin.cash_expected(closed) - expected).abs() < 0.005, "la caja cerrada sigue cuadrada");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stock_spent_by_two_stations_adds_up() {
        // un producto con 5: este puesto gasta 0,2 y el otro 0,25 a la vez
        let base = json!({"products": [{"id": 9, "stock": 5.0, "name": "Composite"}]});
        let ours = json!({"products": [{"id": 9, "stock": 4.8, "name": "Composite"}]});
        let theirs = json!({"products": [{"id": 9, "stock": 4.75, "name": "Composite A2"}]});
        let mut c = 0;
        let m = merge(Some(&base), &ours, &theirs, &mut c);
        assert!((m["products"][0]["stock"].as_f64().unwrap() - 4.55).abs() < 1e-9, "{m}");
        assert_eq!(m["products"][0]["name"], "Composite A2");
        assert_eq!(c, 0, "no es un conflicto");
    }

    #[test]
    fn stock_moves_made_by_two_stations_at_once_are_all_counted() {
        let dir = std::env::temp_dir().join(format!("ds_shared_stock_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clinica.json");
        prepare_folder(&Db::sample(NaiveDate::from_ymd_opt(2026, 11, 6).unwrap()), &dir).unwrap();
        let load = || -> (Db, Value) {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            (serde_json::from_value(v.clone()).unwrap(), v)
        };
        let (mut a, mut base_a) = load();
        let (mut b, mut base_b) = load();
        a.station = 1;
        b.station = 2;
        let before = a.product(4).unwrap().stock;
        a.stock_move(4, -0.2, crate::stock::MoveKind::Consumo, "gabinete 1", None, "");
        b.stock_move(4, -0.25, crate::stock::MoveKind::Consumo, "gabinete 2", None, "");
        save_synced(&mut a, &mut base_a, &path).unwrap();
        save_synced(&mut b, &mut base_b, &path).unwrap();
        let (fin, _) = load();
        assert!((fin.product(4).unwrap().stock - (before - 0.45)).abs() < 1e-9, "se descuentan los dos: {}", fin.product(4).unwrap().stock);
        assert_eq!(fin.stock_moves.iter().filter(|m| m.product_id == 4 && m.reason.starts_with("gabinete")).count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleted_here_but_edited_there_is_kept() {
        let base = json!({"x": [{"id": 1, "v": 1}]});
        let ours = json!({"x": []});
        let theirs = json!({"x": [{"id": 1, "v": 2}]});
        let mut c = 0;
        assert_eq!(merge(Some(&base), &ours, &theirs, &mut c), json!({"x": [{"id": 1, "v": 2}]}));
        assert_eq!(c, 1);
    }

    #[test]
    fn two_stations_saving_to_same_file_lose_nothing() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        let dir = std::env::temp_dir().join(format!("ds_shared_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clinica.json");
        let start = Db::sample(today);
        assert!(prepare_folder(&start, &dir).unwrap());
        let load = || -> (Db, Value) {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            (serde_json::from_value(v.clone()).unwrap(), v)
        };
        let (mut a, mut base_a) = load();
        let (mut b, mut base_b) = load();
        a.station = 1;
        b.station = 2;
        // los dos crean un paciente a la vez: identificadores distintos
        let ida = a.new_id();
        let idb = b.new_id();
        assert_ne!(ida, idb);
        assert_eq!(ida % 16, 1);
        assert_eq!(idb % 16, 2);
        let mut pa = a.patients[0].clone();
        pa.id = ida;
        pa.first_name = "Desde A".into();
        a.patients.push(pa);
        let mut pb = b.patients[0].clone();
        pb.id = idb;
        pb.first_name = "Desde B".into();
        b.patients.push(pb);
        // y uno cambia una cita mientras el otro cobra
        b.appointments[0].notes = "nota de B".into();
        let pays = a.payments.len();
        a.add_payment(100, 10.0, "Efectivo", "prueba");
        assert!(matches!(save_synced(&mut a, &mut base_a, &path).unwrap(), Synced::Saved));
        assert!(matches!(save_synced(&mut b, &mut base_b, &path).unwrap(), Synced::Merged(_)));
        let (fin, _) = load();
        assert!(fin.patients.iter().any(|p| p.first_name == "Desde A"));
        assert!(fin.patients.iter().any(|p| p.first_name == "Desde B"));
        assert_eq!(fin.payments.len(), pays + 1);
        assert_eq!(fin.appointments[0].notes, "nota de B");
        assert!(fin.next_id >= ida.max(idb));
        assert_eq!(fin.stations.len(), 2);
        // A no tenía cambios: recarga lo de B
        let (re, _) = reload_if_changed(&a, &path).unwrap();
        assert!(re.patients.iter().any(|p| p.first_name == "Desde B"));
        assert!(!dir.join("clinica.lock").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Clínica grande de prueba (unos 10 años de trabajo) para medir tiempos.
#[cfg(test)]
pub fn big_clinic() -> Db {
    let sample = serde_json::to_value(Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 4).unwrap())).unwrap();
    let targets = [
        ("patients", 8_000),
        ("entries", 100_000),
        ("appointments", 120_000),
        ("payments", 80_000),
        ("invoices", 40_000),
        ("budgets", 20_000),
        ("documents", 20_000),
        ("perio", 3_000),
        ("messages", 100_000),
        ("calls", 50_000),
        ("recalls", 8_000),
        ("health", 8_000),
        ("images", 30_000),
    ];
    let mut v = sample.clone();
    for (k, n) in targets {
        let Some(src) = sample[k].as_array().filter(|a| !a.is_empty()) else { continue };
        let out: Vec<Value> = (0..n)
            .map(|i| {
                let mut r = src[i % src.len()].clone();
                if r.get("id").is_some() {
                    r["id"] = Value::from(1_000_000 + i as u64);
                }
                r
            })
            .collect();
        v[k] = Value::Array(out);
    }
    serde_json::from_value(v).unwrap()
}

#[cfg(test)]
mod big_clinic_timings {
    use super::*;
    use crate::sync::{Snapshot, apply, diff};
    use std::time::Instant;

    fn rss_mb() -> u64 {
        std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok()).map(|p| p * 4096 / 1_000_000).unwrap_or(0)
    }

    /// Tiempos y memoria con una clínica grande (unos 220 MB de datos):
    /// `cargo test big_clinic -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn big_clinic_save_costs() {
        let t = Instant::now();
        let mut db = big_clinic();
        println!("crear: {:?} · memoria {} MB", t.elapsed(), rss_mb());
        let t = Instant::now();
        let text = serde_json::to_string(&db).unwrap();
        println!("datos enteros a texto (lo que se enviaba en cada guardado): {:?} · {} MB", t.elapsed(), text.len() / 1_000_000);
        drop(text);
        let t = Instant::now();
        let mut server = Snapshot::from_db(&db);
        let synced = server.clone();
        println!("partir en registros: {:?} · memoria {} MB (datos + servidor + lo sincronizado)", t.elapsed(), rss_mb());
        // un cambio normal: una cita y un cobro (tras el último envío)
        crate::sync::mark_synced(&mut db);
        db.appointments[500].notes = "llamar antes".into();
        db.add_payment(1_000_100, 40.0, "Tarjeta", "");
        let t = Instant::now();
        let changes = crate::sync::diff_db(&synced, &db);
        println!("puesto: comparar solo lo tocado: {:?} · {} cambios", t.elapsed(), changes.len());
        let t = Instant::now();
        let clone = db.clone();
        println!("copiar los datos (para hacerlo en segundo plano): {:?}", t.elapsed());
        drop(clone);
        let t = Instant::now();
        let ours = Snapshot::from_db(&db);
        let changes2 = diff(&synced, &ours);
        assert_eq!(changes.len(), changes2.len(), "lo mismo que comparando todo");
        drop(ours);
        let sent: usize = changes.iter().map(|c| c.after.as_ref().map_or(0, |a| a.len()) + c.before.as_ref().map_or(0, |b| b.len())).sum();
        println!("puesto: ver qué ha cambiado: {:?} · {} cambios · se envían {} KB", t.elapsed(), changes.len(), sent / 1000);
        let t = Instant::now();
        let o = apply(&mut server, &changes).unwrap();
        println!("servidor: aplicar: {:?} · {} tocados", t.elapsed(), o.touched.len());
        let t = Instant::now();
        let out = server.to_json();
        println!("servidor: escribir el fichero: {:?} · {} MB", t.elapsed(), out.len() / 1_000_000);
        println!("memoria al final: {} MB", rss_mb());
        assert!(changes.len() <= 6);
    }

    /// Un solo ordenador: guardar tras un cambio pequeño.
    /// `cargo test big_clinic_local -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn big_clinic_local_save() {
        let dir = std::env::temp_dir().join(format!("ds_big_local_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clinica.json");
        let mut db = big_clinic();
        std::fs::write(&path, serde_json::to_string(&db).unwrap()).unwrap();
        let t = Instant::now();
        let mut base = serde_json::to_value(&db).unwrap();
        println!("al abrir (copia para combinar): {:?} · memoria {} MB", t.elapsed(), rss_mb());
        db.appointments[500].notes = "llamar antes".into();
        let t = Instant::now();
        save_synced(&mut db, &mut base, &path).unwrap();
        println!("antes · guardar un cambio: {:?} · memoria {} MB", t.elapsed(), rss_mb());
        drop(base);
        // ahora: datos bloqueados y diario
        let t = Instant::now();
        let mut st = crate::local::LocalStore::new(path.clone(), crate::local::lock(&path).unwrap(), &mut db).unwrap();
        println!("ahora · al abrir: {:?} · memoria {} MB", t.elapsed(), rss_mb());
        db.appointments[501].notes = "otra nota".into();
        let t = Instant::now();
        let n = st.save(&mut db).unwrap();
        println!("ahora · guardar un cambio: {:?} · {n} cambios · diario {} bytes · memoria {} MB", t.elapsed(), std::fs::metadata(crate::sync::journal_path(&path)).unwrap().len(), rss_mb());
        db.patients[10].mobile = "600000000".into();
        let t = Instant::now();
        st.save(&mut db).unwrap();
        println!("ahora · guardar un cambio en un paciente: {:?}", t.elapsed());
        let t = Instant::now();
        st.close(&db).unwrap();
        println!("ahora · al cerrar (fichero completo): {:?}", t.elapsed());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
