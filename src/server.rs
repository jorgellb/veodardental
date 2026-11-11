//! Servidor de datos de la clínica y conexión de los puestos (clientes).
//!
//! Un ordenador hace de servidor: guarda el único fichero de datos, hace las
//! copias de seguridad y atiende a los puestos de recepción y gabinetes.
//! Cada puesto trabaja con una copia en memoria y envía sus cambios; el
//! servidor los combina con los de los demás (registro a registro) y devuelve
//! el resultado. Los puestos sin cambios recogen los de los demás cada pocos
//! segundos.
//!
//! Protocolo HTTP sencillo (solo red local o VPN), con clave de la clínica:
//!   GET  /estado            → {"rev": n}
//!   GET  /datos             → datos completos
//!   POST /guardar?base=REV  → (cuerpo: datos del puesto) datos combinados
//!   POST /registro          → (cuerpo: {"nombre": ..}) {"numero": n}
//!   POST /parar             → detiene el servidor

use crate::model::Db;
use crate::shared::{StationSeen, merge};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 8899;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    pub key: String,
}

fn config_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join(crate::brand::DIR).join("servidor.json")
}

/// Clave nueva de 12 caracteres (no depende de nada externo).
pub fn new_key() -> String {
    use sha2::{Digest, Sha256};
    let seed = format!("{:?}{}{:?}", std::time::SystemTime::now(), std::process::id(), std::thread::current().id());
    let h = Sha256::digest(seed.as_bytes());
    let alphabet = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    h.iter().take(12).map(|b| alphabet[(*b as usize) % alphabet.len()] as char).collect()
}

/// Configuración guardada del servidor de este ordenador (sin clave si nunca
/// se ha activado). No escribe nada.
pub fn server_config() -> ServerConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|t| serde_json::from_str::<ServerConfig>(&t).ok())
        .filter(|c| !c.key.is_empty())
        .unwrap_or(ServerConfig { port: DEFAULT_PORT, key: String::new() })
}

/// Configuración del servidor; la crea (con clave nueva) la primera vez.
pub fn ensure_server_config() -> ServerConfig {
    let c = server_config();
    if !c.key.is_empty() {
        return c;
    }
    let c = ServerConfig { port: DEFAULT_PORT, key: new_key() };
    if let Some(d) = config_path().parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(config_path(), serde_json::to_string_pretty(&c).unwrap_or_default());
    c
}

/// Dirección de este ordenador en la red local (para decírsela a los puestos).
pub fn local_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.168.1.1:80").ok()?;
    s.local_addr().ok().map(|a| a.ip().to_string())
}

// ───────────────────────────── servidor ─────────────────────────────

struct State {
    data: Value,
    history: VecDeque<(u64, Value)>,
    path: PathBuf,
    last_backup_day: Option<chrono::NaiveDate>,
}

fn rev_of(v: &Value) -> u64 {
    v.get("rev").and_then(|r| r.as_u64()).unwrap_or(0)
}

impl State {
    fn persist(&mut self) -> Result<(), String> {
        let tmp = self.path.with_extension("json.tmp");
        if let Some(d) = self.path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&tmp, serde_json::to_string_pretty(&self.data).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
        // copia de seguridad diaria en el servidor
        let today = crate::model::today();
        if self.last_backup_day != Some(today) && crate::tools::needs_daily(today) {
            if let Ok(db) = serde_json::from_value::<Db>(self.data.clone()) {
                let _ = crate::tools::create_backup(&db, "automatica (servidor)");
            }
            self.last_backup_day = Some(today);
        }
        Ok(())
    }

    /// Combina los datos de un puesto con los actuales y devuelve el resultado.
    fn save(&mut self, base_rev: u64, ours: Value, station: u32) -> Result<Value, String> {
        let cur_rev = rev_of(&self.data);
        let mut merged = if base_rev == cur_rev {
            ours
        } else {
            let base = self.history.iter().find(|(r, _)| *r == base_rev).map(|(_, v)| v.clone());
            let mut conflicts = 0;
            let mut m = merge(base.as_ref(), &ours, &self.data, &mut conflicts);
            let next = ours.get("next_id").and_then(|v| v.as_u64()).unwrap_or(0).max(self.data.get("next_id").and_then(|v| v.as_u64()).unwrap_or(0));
            m["next_id"] = Value::from(next);
            m
        };
        // los datos deben seguir siendo válidos
        let mut db: Db = serde_json::from_value(merged.clone()).map_err(|e| format!("datos no válidos: {e}"))?;
        if station > 0 {
            if let Some(s) = db.stations.iter_mut().find(|s| s.id == station) {
                s.last_seen = crate::model::now();
            }
        }
        db.rev = cur_rev + 1;
        merged = serde_json::to_value(&db).map_err(|e| e.to_string())?;
        self.history.push_back((cur_rev, self.data.clone()));
        while self.history.len() > 40 {
            self.history.pop_front();
        }
        self.data = merged.clone();
        self.persist()?;
        Ok(merged)
    }

    fn register(&mut self, name: &str) -> Result<u32, String> {
        let mut db: Db = serde_json::from_value(self.data.clone()).map_err(|e| e.to_string())?;
        let n = match db.stations.iter().find(|s| s.name == name) {
            Some(s) => s.id,
            None => {
                let n = (1..=15).find(|n| !db.stations.iter().any(|s| s.id == *n)).ok_or("No caben más puestos (máximo 15).")?;
                db.stations.push(StationSeen { id: n, name: name.into(), last_seen: crate::model::now() });
                n
            }
        };
        let rev = rev_of(&self.data);
        db.rev = rev + 1;
        self.history.push_back((rev, self.data.clone()));
        self.data = serde_json::to_value(&db).map_err(|e| e.to_string())?;
        self.persist()?;
        Ok(n)
    }
}

fn query(url: &str, key: &str) -> Option<String> {
    url.split_once('?')?.1.split('&').find_map(|kv| kv.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v.to_string()))
}

fn respond(req: tiny_http::Request, code: u16, body: String) {
    let h = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..]).unwrap();
    let _ = req.respond(tiny_http::Response::from_string(body).with_status_code(code).with_header(h));
}

/// Atiende peticiones hasta recibir /parar.
pub fn serve(server: tiny_http::Server, path: PathBuf, key: String, initial: Value) {
    let mut st = State { data: initial, history: VecDeque::new(), path, last_backup_day: None };
    for mut req in server.incoming_requests() {
        let ok_key = req.headers().iter().any(|h| h.field.equiv("X-Clave") && h.value.as_str() == key);
        if !ok_key {
            respond(req, 401, json!({"error": "Clave de la clínica incorrecta"}).to_string());
            continue;
        }
        let url = req.url().to_string();
        let route = url.split('?').next().unwrap_or("").to_string();
        let method = req.method().clone();
        let station: u32 = req.headers().iter().find(|h| h.field.equiv("X-Puesto")).and_then(|h| h.value.as_str().parse().ok()).unwrap_or(0);
        let mut body = String::new();
        let _ = req.as_reader().read_to_string(&mut body);
        match (method, route.as_str()) {
            (tiny_http::Method::Get, "/estado") => respond(req, 200, json!({"rev": rev_of(&st.data)}).to_string()),
            (tiny_http::Method::Get, "/datos") => respond(req, 200, st.data.to_string()),
            (tiny_http::Method::Post, "/guardar") => {
                let base: u64 = query(&url, "base").and_then(|b| b.parse().ok()).unwrap_or(0);
                match serde_json::from_str::<Value>(&body).map_err(|e| e.to_string()).and_then(|v| st.save(base, v, station)) {
                    Ok(v) => respond(req, 200, v.to_string()),
                    Err(e) => respond(req, 400, json!({"error": e}).to_string()),
                }
            }
            (tiny_http::Method::Post, "/registro") => {
                let name = serde_json::from_str::<Value>(&body).ok().and_then(|v| v["nombre"].as_str().map(|s| s.to_string())).unwrap_or_default();
                match st.register(name.trim()) {
                    Ok(n) => respond(req, 200, json!({"numero": n}).to_string()),
                    Err(e) => respond(req, 400, json!({"error": e}).to_string()),
                }
            }
            (tiny_http::Method::Post, "/parar") => {
                respond(req, 200, json!({"ok": true}).to_string());
                break;
            }
            _ => respond(req, 404, json!({"error": "no existe"}).to_string()),
        }
    }
}

/// Arranca el servidor en este ordenador (modo `--servidor`).
pub fn run() -> Result<(), String> {
    let cfg = ensure_server_config();
    let (db, err) = Db::load_or_sample();
    if let Some(e) = err {
        return Err(e);
    }
    let path = Db::path();
    let server = tiny_http::Server::http(("0.0.0.0", cfg.port)).map_err(|e| format!("No se puede abrir el puerto {}: {e}", cfg.port))?;
    println!("VeodarDental · servidor de datos");
    println!("  Datos: {}", path.display());
    println!("  Dirección para los puestos: {}:{}", local_ip().unwrap_or_else(|| "IP-de-este-ordenador".into()), cfg.port);
    println!("  Clave de la clínica: {}", cfg.key);
    let initial = serde_json::to_value(&db).map_err(|e| e.to_string())?;
    serve(server, path, cfg.key, initial);
    Ok(())
}

/// ¿Hay un servidor funcionando en este ordenador?
pub fn local_server_running() -> bool {
    let cfg = server_config();
    if cfg.key.is_empty() {
        return false;
    }
    Remote::new(&format!("127.0.0.1:{}", cfg.port), &cfg.key, 0).rev().is_ok()
}

/// Lanza el servidor como proceso aparte (sigue funcionando aunque se cierre la ventana).
pub fn spawn_local_server() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe).arg("--servidor").spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ───────────────────────────── cliente ─────────────────────────────

#[derive(Clone)]
pub struct Remote {
    base: String,
    key: String,
    pub station: u32,
    agent: ureq::Agent,
}

fn err_text(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401, _) => "La clave de la clínica no es correcta.".into(),
        ureq::Error::Status(code, r) => {
            let t = r.into_string().unwrap_or_default();
            serde_json::from_str::<Value>(&t).ok().and_then(|v| v["error"].as_str().map(|s| s.to_string())).unwrap_or_else(|| format!("error {code} del servidor"))
        }
        ureq::Error::Transport(_) => "No hay conexión con el servidor de la clínica.".into(),
    }
}

fn read_all(r: ureq::Response) -> Result<String, String> {
    let mut s = String::new();
    r.into_reader().read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(s)
}

impl Remote {
    pub fn new(addr: &str, key: &str, station: u32) -> Self {
        let addr = addr.trim().trim_start_matches("http://").trim_end_matches('/');
        let addr = if addr.contains(':') { addr.to_string() } else { format!("{addr}:{DEFAULT_PORT}") };
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(2)).timeout(Duration::from_secs(20)).build();
        Remote { base: format!("http://{addr}"), key: key.trim().to_string(), station, agent }
    }

    fn get(&self, path: &str) -> Result<String, String> {
        let r = self.agent.get(&format!("{}{}", self.base, path)).set("X-Clave", &self.key).set("X-Puesto", &self.station.to_string()).call().map_err(err_text)?;
        read_all(r)
    }

    fn post(&self, path: &str, body: &str) -> Result<String, String> {
        let r = self.agent.post(&format!("{}{}", self.base, path)).set("X-Clave", &self.key).set("X-Puesto", &self.station.to_string()).set("Content-Type", "application/json").send_string(body).map_err(err_text)?;
        read_all(r)
    }

    pub fn rev(&self) -> Result<u64, String> {
        let v: Value = serde_json::from_str(&self.get("/estado")?).map_err(|e| e.to_string())?;
        Ok(rev_of(&v))
    }

    pub fn load(&self) -> Result<(Db, Value), String> {
        let v: Value = serde_json::from_str(&self.get("/datos")?).map_err(|e| e.to_string())?;
        let mut db: Db = serde_json::from_value(v.clone()).map_err(|e| format!("Datos del servidor no válidos: {e}"))?;
        // lo que traen las versiones nuevas (llega al servidor con el próximo guardado)
        db.upgrade();
        Ok((db, v))
    }

    pub fn register(&self, name: &str) -> Result<u32, String> {
        let v: Value = serde_json::from_str(&self.post("/registro", &json!({ "nombre": name }).to_string())?).map_err(|e| e.to_string())?;
        v["numero"].as_u64().map(|n| n as u32).ok_or_else(|| "Respuesta no válida del servidor".into())
    }

    /// Envía los datos del puesto; devuelve los datos combinados del servidor.
    pub fn save(&self, db: &Db) -> Result<(Db, Value), String> {
        let body = serde_json::to_string(db).map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_str(&self.post(&format!("/guardar?base={}", db.rev), &body)?).map_err(|e| e.to_string())?;
        let mut new: Db = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
        new.station = db.station;
        new.config.current_user = db.config.current_user.clone();
        new.config.current_center = db.config.current_center;
        Ok((new, v))
    }

    pub fn stop(&self) -> Result<(), String> {
        self.post("/parar", "{}").map(|_| ())
    }
}

/// Deja los datos del servidor listos en un puesto (conservando lo propio del puesto).
pub fn adopt(local: &Db, mut remote: Db) -> Db {
    remote.station = local.station;
    remote.config.current_user = local.config.current_user.clone();
    remote.config.current_center = local.config.current_center;
    remote
}

/// Conecta un puesto: se registra (recibe su número) y descarga los datos.
/// Si el servidor es este mismo ordenador y aún está arrancando, espera un poco.
pub fn connect(r: &mut Remote, name: &str, attempts: u32) -> Result<(Db, Value), String> {
    let mut last = String::new();
    for i in 0..attempts.max(1) {
        if i > 0 {
            std::thread::sleep(Duration::from_millis(250));
        }
        match r.register(name) {
            Ok(n) => {
                r.station = n;
                let (mut db, v) = r.load()?;
                db.station = n;
                return Ok((db, v));
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

pub fn is_local(addr: &str) -> bool {
    let a = addr.trim();
    a.starts_with("127.0.0.1") || a.starts_with("localhost")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn server_with_two_clients() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        let dir = std::env::temp_dir().join(format!("ds_server_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clinica.json");
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let initial = serde_json::to_value(Db::sample(today)).unwrap();
        let p2 = path.clone();
        let h = std::thread::spawn(move || serve(server, p2, "CLAVE".into(), initial));
        let addr = format!("127.0.0.1:{port}");

        // clave incorrecta
        assert!(Remote::new(&addr, "MAL", 0).rev().unwrap_err().contains("clave"));

        let mut ra = Remote::new(&addr, "CLAVE", 0);
        let mut rb = Remote::new(&addr, "CLAVE", 0);
        ra.station = ra.register("Recepción").unwrap();
        rb.station = rb.register("Gabinete 1").unwrap();
        assert_ne!(ra.station, rb.station);
        assert_eq!(ra.register("Recepción").unwrap(), ra.station, "un puesto conserva su número");

        let (mut a, _) = ra.load().unwrap();
        let (mut b, _) = rb.load().unwrap();
        a.station = ra.station;
        b.station = rb.station;
        let pays = a.payments.len();
        a.add_payment(100, 25.0, "Tarjeta", "desde recepción");
        b.appointments[0].notes = "nota del gabinete".into();
        let ida = a.new_id();
        let idb = b.new_id();
        assert_ne!(ida, idb);

        let (a2, _) = ra.save(&a).unwrap();
        let (b2, _) = rb.save(&b).unwrap();
        assert_eq!(b2.payments.len(), pays + 1, "el gabinete recibe el cobro de recepción");
        assert_eq!(b2.appointments[0].notes, "nota del gabinete");
        assert!(rb.rev().unwrap() > a2.rev);
        // recepción recoge lo del gabinete
        let (a3, _) = ra.load().unwrap();
        assert_eq!(a3.appointments[0].notes, "nota del gabinete");
        // el servidor lo ha guardado en disco
        let disk: Db = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(disk.payments.len(), pays + 1);
        assert_eq!(disk.stations.len(), 2);

        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
