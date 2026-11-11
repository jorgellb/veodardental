//! Servidor de datos de la clínica y conexión de los puestos (clientes).
//!
//! Un ordenador hace de servidor: guarda el único fichero de datos, hace las
//! copias de seguridad y atiende a los puestos de recepción y gabinetes.
//! Cada puesto trabaja con una copia en memoria y envía sus cambios; el
//! servidor los combina con los de los demás (registro a registro) y devuelve
//! el resultado. Los puestos sin cambios recogen los de los demás cada pocos
//! segundos.
//!
//! Seguridad:
//! - Todo va cifrado con TLS 1.3. La clave del certificado del servidor sale
//!   de la clave de la clínica, así que cada puesto sabe de antemano qué
//!   servidor es el bueno: nadie en la red puede leer los datos ni hacerse
//!   pasar por el servidor, y no hay que instalar ni confirmar certificados.
//! - Para ver o guardar datos hace falta, además de la clave de la clínica,
//!   haber entrado con un usuario (contraseña y, si lo tiene, el código del
//!   móvil). El servidor comprueba el acceso y da una sesión al puesto.
//! - Las contraseñas y las claves del código del móvil nunca salen del
//!   servidor, y solo Dirección puede cambiar los usuarios.
//!
//! Protocolo (HTTP sobre TLS, con la cabecera `X-Clave`; las marcadas con *
//! necesitan además la sesión en `X-Sesion`):
//!   GET  /acceso             → nombre de la clínica y usuarios (sin contraseñas)
//!   POST /registro           → {"nombre"} → {"numero": n} (número del puesto)
//!   POST /entrar             → {"usuario", "contrasena"} → sesión o código del móvil
//!   POST /codigo             → {"usuario", "codigo", "previa"} → sesión
//! * GET  /estado             → {"rev": n}
//! * GET  /datos              → datos completos
//! * POST /guardar?base=REV   → (cuerpo: datos del puesto) datos combinados
//! * POST /contrasena         → {"usuario", "actual"?, "nueva", "cambiar_al_entrar"}
//! * POST /movil              → {"usuario", "accion": "activar"|"quitar", "clave", "codigo"}
//!   POST /parar              → detiene el servidor (solo desde el propio servidor)
//!
//! Cada puesto tiene una sesión, que sigue valiendo con la pantalla bloqueada
//! (los WhatsApp, la cita online y el correo siguen guardando) y se renueva al
//! volver a entrar; caduca tras 24 horas sin usarse.

use crate::model::{Db, sha256_hex};
use crate::shared::{StationSeen, keep_logs, merge};
use crate::users::{LoginStep, Perm, ct_eq};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

pub const DEFAULT_PORT: u16 = 8899;
/// Tamaño máximo de una petición (los datos completos de una clínica grande caben de sobra).
const MAX_BODY: usize = 512 * 1024 * 1024;
/// Conexiones atendidas a la vez (cada puesto usa una cada vez que habla con el servidor).
const MAX_CONNECTIONS: usize = 64;
/// Lo que ven los puestos en lugar de la huella de la contraseña o de la clave del móvil.
pub const HIDDEN: &str = "oculto";
/// Datos de los usuarios que solo cambia el servidor (al entrar o con sus órdenes propias).
const SERVER_FIELDS: [&str; 6] = ["pin_hash", "salt", "totp_secret", "totp_last", "failed_logins", "last_failed"];
/// Mensaje cuando la sesión del puesto ya no vale (el puesto vuelve a pedir el usuario).
pub const SESSION_EXPIRED: &str = "La sesión con el servidor ha terminado: vuelva a entrar con su usuario.";
const OLD_STATION: &str = "Este puesto tiene una versión anterior de VeodarDental: actualícelo para conectar con el servidor de la clínica.";
const OLD_SERVER: &str = "El servidor de la clínica tiene una versión anterior de VeodarDental: actualice el programa en el ordenador servidor.";
const WRONG_KEY: &str = "La clave de la clínica no es correcta.";
const NO_CONNECTION: &str = "No hay conexión con el servidor de la clínica.";
/// Una sesión sin usar durante este tiempo caduca.
const SESSION_IDLE_HOURS: i64 = 24;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    pub key: String,
}

fn config_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join(crate::brand::DIR).join("servidor.json")
}

/// Clave nueva de 12 caracteres (60 bits) del generador aleatorio del sistema.
/// También la usan las claves del repositorio de recetas y de WhatsApp.
pub fn new_key() -> String {
    use ring::rand::SecureRandom;
    let alphabet = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut b = [0u8; 12];
    if ring::rand::SystemRandom::new().fill(&mut b).is_err() {
        // sin generador del sistema (no debería pasar): el de los enlaces, que mezcla la hora
        b.copy_from_slice(&crate::portal::new_token().as_bytes()[..12]);
    }
    // 32 símbolos: cada byte da uno sin sesgo (256 es múltiplo de 32)
    b.iter().map(|x| alphabet[(*x as usize) % alphabet.len()] as char).collect()
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

fn rev_of(v: &Value) -> u64 {
    v.get("rev").and_then(|r| r.as_u64()).unwrap_or(0)
}

// ───────────────────────── cifrado (TLS) ─────────────────────────

const TLS_SALT: &[u8] = b"VeodarDental servidor de datos TLS v1";
/// Vueltas de PBKDF2 para sacar la clave del certificado de la clave de la
/// clínica: quien vea el certificado no puede adivinar la clave probando.
const TLS_ROUNDS: u32 = if cfg!(test) { 1_000 } else { 100_000 };

/// Semilla Ed25519 de la clave de la clínica (se calcula una vez por clave).
fn tls_seed(key: &str) -> [u8; 32] {
    static CACHE: OnceLock<Mutex<HashMap<String, [u8; 32]>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(s) = cache.lock().ok().and_then(|c| c.get(key).copied()) {
        return s;
    }
    let mut seed = [0u8; 32];
    let rounds = std::num::NonZeroU32::new(TLS_ROUNDS).unwrap_or(std::num::NonZeroU32::MIN);
    ring::pbkdf2::derive(ring::pbkdf2::PBKDF2_HMAC_SHA256, rounds, TLS_SALT, key.as_bytes(), &mut seed);
    if let Ok(mut c) = cache.lock() {
        c.insert(key.to_string(), seed);
    }
    seed
}

/// Clave privada Ed25519 en PKCS#8 v1 (RFC 8410): cabecera fija y la semilla.
fn ed25519_pkcs8(seed: &[u8; 32]) -> Vec<u8> {
    let mut v = vec![0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20];
    v.extend_from_slice(seed);
    v
}

/// Clave pública que debe tener el servidor de una clave de la clínica.
pub fn tls_public_key(key: &str) -> Vec<u8> {
    use ring::signature::KeyPair;
    ring::signature::Ed25519KeyPair::from_seed_unchecked(&tls_seed(key)).map(|k| k.public_key().as_ref().to_vec()).unwrap_or_default()
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn server_tls(key: &str) -> Result<Arc<rustls::ServerConfig>, String> {
    let der = rustls::pki_types::PrivatePkcs8KeyDer::from(ed25519_pkcs8(&tls_seed(key)));
    let kp = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&der, &rcgen::PKCS_ED25519).map_err(|e| e.to_string())?;
    let mut params = rcgen::CertificateParams::new(vec!["veodardental".to_string()]).map_err(|e| e.to_string())?;
    params.distinguished_name.push(rcgen::DnType::CommonName, "Servidor de datos de VeodarDental");
    let cert = params.self_signed(&kp).map_err(|e| e.to_string())?;
    let cfg = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(vec![cert.der().clone()], rustls::pki_types::PrivateKeyDer::Pkcs8(der))
        .map_err(|e| e.to_string())?;
    Ok(Arc::new(cfg))
}

/// Señal (en el error de TLS) de que el servidor no tiene la clave de la clínica.
const KEY_MISMATCH: &str = "VEODAR: certificado de otra clave";

/// El puesto solo acepta el servidor cuya clave pública sale de la clave de la clínica.
struct ClinicKeyVerifier {
    expected: Vec<u8>,
    algs: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl std::fmt::Debug for ClinicKeyVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClinicKeyVerifier")
    }
}

impl rustls::client::danger::ServerCertVerifier for ClinicKeyVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let mismatch = || rustls::Error::General(KEY_MISMATCH.into());
        let (_, cert) = x509_parser::parse_x509_certificate(end_entity.as_ref()).map_err(|_| mismatch())?;
        let public = cert.public_key().subject_public_key.data.as_ref();
        if !self.expected.is_empty() && ct_eq(public, &self.expected) {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(mismatch())
        }
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &rustls::pki_types::CertificateDer<'_>, dss: &rustls::DigitallySignedStruct) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &rustls::pki_types::CertificateDer<'_>, dss: &rustls::DigitallySignedStruct) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.algs.supported_schemes()
    }
}

fn client_tls(key: &str) -> Result<Arc<rustls::ClientConfig>, String> {
    let p = provider();
    let verifier = ClinicKeyVerifier { expected: tls_public_key(key), algs: p.signature_verification_algorithms };
    let cfg = rustls::ClientConfig::builder_with_provider(p)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

// ───────────────────────────── HTTP ─────────────────────────────

/// Petición HTTP/1.1 ya leída (sin «chunked»: los puestos siempre mandan su longitud).
pub struct Request {
    pub method: String,
    pub path: String,
    query: String,
    headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub peer: IpAddr,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    fn param(&self, key: &str) -> Option<&str> {
        self.query.split('&').find_map(|kv| kv.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v))
    }
}

#[derive(Debug)]
pub struct Response {
    pub code: u16,
    pub body: String,
}

fn reply(code: u16, v: Value) -> Response {
    Response { code, body: v.to_string() }
}

fn error(code: u16, msg: &str) -> Response {
    reply(code, json!({ "error": msg }))
}

/// Lee una línea de como mucho 8 KB.
fn read_line(r: &mut impl BufRead, buf: &mut Vec<u8>) -> Result<usize, Response> {
    buf.clear();
    let n = r.by_ref().take(8192).read_until(b'\n', buf).map_err(|_| error(400, "Petición incompleta."))?;
    if n > 0 && !buf.ends_with(b"\n") {
        return Err(error(431, "Cabecera demasiado larga."));
    }
    Ok(n)
}

/// Lee una petición; `None` si la conexión se cierra sin mandar nada.
fn read_request(r: &mut impl BufRead, peer: IpAddr, max_body: usize) -> Result<Option<Request>, Response> {
    let mut line = Vec::new();
    if read_line(r, &mut line)? == 0 {
        return Ok(None);
    }
    let first = String::from_utf8_lossy(&line).to_string();
    let mut parts = first.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else { return Err(error(400, "Petición no válida.")) };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut headers = vec![];
    loop {
        if read_line(r, &mut line)? == 0 {
            return Err(error(400, "Petición incompleta."));
        }
        let h = String::from_utf8_lossy(&line).trim_end().to_string();
        if h.is_empty() {
            break;
        }
        if headers.len() >= 64 {
            return Err(error(431, "Demasiadas cabeceras."));
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let req = Request { method: method.to_string(), path: path.to_string(), query: query.to_string(), headers, body: vec![], peer };
    if req.header("Transfer-Encoding").is_some() {
        return Err(error(501, "Envío por partes no admitido."));
    }
    let len: usize = match req.header("Content-Length") {
        Some(v) => v.parse().map_err(|_| error(400, "Longitud no válida."))?,
        None => 0,
    };
    if len > max_body {
        return Err(error(413, "Los datos enviados son demasiado grandes."));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).map_err(|_| error(400, "Petición incompleta."))?;
    Ok(Some(Request { body, ..req }))
}

fn write_response(w: &mut impl Write, resp: &Response) -> std::io::Result<()> {
    let reason = match resp.code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        426 => "Upgrade Required",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        501 => "Not Implemented",
        _ => "Error",
    };
    write!(w, "HTTP/1.1 {} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", resp.code, resp.body.len())?;
    w.write_all(resp.body.as_bytes())?;
    w.flush()
}

struct Job {
    req: Request,
    reply: mpsc::Sender<Response>,
}

/// Atiende una conexión: cifrado, lectura de la petición y respuesta del servidor.
fn handle_conn(tcp: TcpStream, tls: Arc<rustls::ServerConfig>, jobs: mpsc::Sender<Job>) {
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(60)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(120)));
    let Ok(peer) = tcp.peer_addr().map(|a| a.ip()) else { return };
    let mut first = [0u8; 1];
    if !matches!(tcp.peek(&mut first), Ok(1)) {
        return;
    }
    // Un saludo TLS empieza por 0x16. Si no, es un puesto con una versión
    // anterior (HTTP sin cifrar): se lee su petición y se le explica qué pasa.
    if first[0] != 0x16 {
        let _ = read_request(&mut BufReader::new(&tcp), peer, MAX_BODY);
        let _ = write_response(&mut &tcp, &error(426, OLD_STATION));
        return;
    }
    let Ok(conn) = rustls::ServerConnection::new(tls) else { return };
    let mut s = rustls::StreamOwned::new(conn, tcp);
    let read = read_request(&mut BufReader::new(&mut s), peer, MAX_BODY);
    let resp = match read {
        Ok(None) => return,
        Err(r) => r,
        Ok(Some(req)) => {
            let (tx, rx) = mpsc::channel();
            if jobs.send(Job { req, reply: tx }).is_err() {
                return;
            }
            match rx.recv() {
                Ok(r) => r,
                Err(_) => return,
            }
        }
    };
    let _ = write_response(&mut s, &resp);
    s.conn.send_close_notify();
    let _ = s.flush();
}

// ───────────────────────────── servidor ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Session {
    user: u32,
    station: u32,
    last_used: chrono::NaiveDateTime,
}

struct State {
    data: Value,
    history: VecDeque<(u64, Value)>,
    path: PathBuf,
    last_backup_day: Option<chrono::NaiveDate>,
    /// Sesiones abiertas, por la huella (SHA-256) de su código.
    sessions: HashMap<String, Session>,
    sessions_saved: Instant,
    /// Contraseña correcta, falta el código del móvil: huella del código previo → (usuario, cuándo).
    pending: HashMap<String, (u32, Instant)>,
    /// Intentos con la clave de la clínica mal y entradas, por ordenador.
    key_fails: HashMap<IpAddr, VecDeque<Instant>>,
    logins: HashMap<IpAddr, VecDeque<Instant>>,
}

/// Cuántas veces en la última ventana (y, si `add`, apunta una más).
fn hits(map: &mut HashMap<IpAddr, VecDeque<Instant>>, ip: IpAddr, window: Duration, add: bool) -> usize {
    if map.len() > 10_000 {
        map.retain(|_, q| q.back().is_some_and(|t| t.elapsed() < window));
    }
    let q = map.entry(ip).or_default();
    while q.front().is_some_and(|t| t.elapsed() >= window) {
        q.pop_front();
    }
    if add {
        q.push_back(Instant::now());
    }
    q.len()
}

fn token_hash(t: &str) -> String {
    sha256_hex(t.trim())
}

/// Copia de los datos para un puesto: sin huellas de contraseñas ni claves del móvil.
pub fn public_view(v: &Value) -> Value {
    let mut out = v.clone();
    if let Some(users) = out.get_mut("users").and_then(|u| u.as_array_mut()) {
        users.iter_mut().for_each(strip_user);
    }
    out
}

fn strip_user(u: &mut Value) {
    for f in ["pin_hash", "totp_secret"] {
        if u.get(f).and_then(|x| x.as_str()).is_some_and(|s| !s.is_empty()) {
            u[f] = json!(HIDDEN);
        }
    }
    u["salt"] = json!("");
    u["totp_last"] = json!(0);
}

fn users_by_id(v: &Value) -> HashMap<u64, Value> {
    v.get("users").and_then(|u| u.as_array()).into_iter().flatten().filter_map(|u| Some((u.get("id")?.as_u64()?, u.clone()))).collect()
}

/// Pone en los usuarios de un puesto los datos que solo guarda el servidor.
fn hydrate_users(ours: &mut Value, current: &Value) {
    let cur = users_by_id(current);
    let Some(users) = ours.get_mut("users").and_then(|u| u.as_array_mut()) else { return };
    for u in users {
        let Some(c) = u.get("id").and_then(|i| i.as_u64()).and_then(|i| cur.get(&i)) else { continue };
        for f in SERVER_FIELDS {
            match c.get(f) {
                Some(x) => u[f] = x.clone(),
                None => {
                    if let Some(o) = u.as_object_mut() {
                        o.remove(f);
                    }
                }
            }
        }
    }
}

/// Tras combinar: los usuarios solo los cambia Dirección, y las contraseñas,
/// las claves del móvil y los fallos de acceso nunca vienen de un puesto.
fn protect_users(merged: &mut Value, current: &Value, admin: bool) {
    if !admin {
        merged["users"] = current.get("users").cloned().unwrap_or_else(|| json!([]));
        return;
    }
    let cur = users_by_id(current);
    hydrate_users(merged, current);
    let Some(users) = merged.get_mut("users").and_then(|u| u.as_array_mut()) else { return };
    for u in users {
        match u.get("id").and_then(|i| i.as_u64()).and_then(|i| cur.get(&i)) {
            Some(c) => {
                // reactivado por Dirección: sus fallos anteriores ya no cuentan
                if c["active"] == json!(false) && u["active"] == json!(true) {
                    u["failed_logins"] = json!(0);
                    u["last_failed"] = Value::Null;
                }
            }
            None => {
                // usuario nuevo: con su contraseña inicial, que tendrá que cambiar
                u["must_change"] = json!(true);
                u["totp_secret"] = json!("");
                u["totp_last"] = json!(0);
                u["failed_logins"] = json!(0);
                u["last_failed"] = Value::Null;
            }
        }
    }
}

#[derive(Deserialize)]
struct EnterBody {
    usuario: u32,
    #[serde(default)]
    contrasena: String,
}

#[derive(Deserialize)]
struct CodeBody {
    usuario: u32,
    codigo: String,
    previa: String,
}

#[derive(Deserialize)]
struct PinBody {
    usuario: u32,
    #[serde(default)]
    actual: Option<String>,
    nueva: String,
    #[serde(default = "yes")]
    cambiar_al_entrar: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct TotpBody {
    usuario: u32,
    accion: String,
    #[serde(default)]
    clave: String,
    #[serde(default)]
    codigo: String,
}

impl State {
    fn new(data: Value, path: PathBuf) -> State {
        let sessions_path = path.with_file_name("sesiones.json");
        let limit = crate::model::now() - chrono::Duration::hours(SESSION_IDLE_HOURS);
        let sessions: HashMap<String, Session> = std::fs::read_to_string(sessions_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let sessions = sessions.into_iter().filter(|(_, s)| s.last_used > limit).collect();
        State {
            data,
            history: VecDeque::new(),
            path,
            last_backup_day: None,
            sessions,
            sessions_saved: Instant::now(),
            pending: HashMap::new(),
            key_fails: HashMap::new(),
            logins: HashMap::new(),
        }
    }

    fn persist(&mut self) -> Result<(), String> {
        let tmp = self.path.with_extension("json.tmp");
        if let Some(d) = self.path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&tmp, serde_json::to_string(&self.data).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
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

    /// Guarda las sesiones (huellas, nunca los códigos) para que sigan valiendo
    /// si el servidor se reinicia.
    fn save_sessions(&mut self) {
        let p = self.path.with_file_name("sesiones.json");
        if let Ok(t) = serde_json::to_string(&self.sessions) {
            let tmp = p.with_extension("json.tmp");
            if std::fs::write(&tmp, t).is_ok() {
                let _ = std::fs::rename(&tmp, &p);
            }
        }
        self.sessions_saved = Instant::now();
    }

    /// Deja `db` como datos actuales (nueva revisión) y los guarda.
    fn commit(&mut self, mut db: Db) -> Result<(), String> {
        let rev = rev_of(&self.data);
        db.rev = rev + 1;
        let v = serde_json::to_value(&db).map_err(|e| e.to_string())?;
        self.history.push_back((rev, std::mem::replace(&mut self.data, v)));
        while self.history.len() > 40 {
            self.history.pop_front();
        }
        self.persist()
    }

    /// Cambia los datos con las funciones del programa (entradas, contraseñas…).
    fn modify<T>(&mut self, f: impl FnOnce(&mut Db) -> T) -> Result<T, String> {
        let mut db: Db = serde_json::from_value(self.data.clone()).map_err(|e| format!("datos no válidos: {e}"))?;
        let out = f(&mut db);
        self.commit(db)?;
        Ok(out)
    }

    fn user(&self, id: u32) -> Option<crate::users::User> {
        users_by_id(&self.data).remove(&(id as u64)).and_then(|u| serde_json::from_value(u).ok())
    }

    fn is_admin(&self, id: u32) -> bool {
        self.user(id).is_some_and(|u| u.active && u.profile.can(Perm::Configuracion))
    }

    /// Combina los datos de un puesto con los actuales (quedan en `self.data`).
    fn save(&mut self, base_rev: u64, mut ours: Value, station: u32, user: u32) -> Result<(), String> {
        let admin = self.is_admin(user);
        let cur_rev = rev_of(&self.data);
        hydrate_users(&mut ours, &self.data);
        let merged = {
            let current = &self.data;
            let base = if base_rev == cur_rev { Some(current) } else { self.history.iter().find(|(r, _)| *r == base_rev).map(|(_, v)| v) };
            let mut merged = if base_rev == cur_rev {
                ours
            } else {
                let mut conflicts = 0;
                let mut m = merge(base, &ours, current, &mut conflicts);
                let next = ours.get("next_id").and_then(|v| v.as_u64()).unwrap_or(0).max(current.get("next_id").and_then(|v| v.as_u64()).unwrap_or(0));
                m["next_id"] = Value::from(next);
                m
            };
            // los registros (accesos, auditoría…) solo crecen: un puesto no borra entradas
            keep_logs(base, &mut merged, current);
            protect_users(&mut merged, current, admin);
            merged
        };
        // los datos deben seguir siendo válidos
        let mut db: Db = serde_json::from_value(merged).map_err(|e| format!("datos no válidos: {e}"))?;
        if let Some(s) = db.stations.iter_mut().find(|s| station > 0 && s.id == station) {
            s.last_seen = crate::model::now();
        }
        self.commit(db)
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
        self.commit(db)?;
        Ok(n)
    }

    /// Lo que necesita la pantalla de entrada (antes de entrar).
    fn access(&self) -> Response {
        let v = public_view(&self.data);
        reply(200, json!({ "clinica": v["config"]["name"], "usuarios": v["users"], "rev": rev_of(&self.data) }))
    }

    fn new_session(&mut self, user: u32, station: u32) -> String {
        // cada puesto tiene una sesión: al entrar otra vez se cierra la anterior
        if station > 0 {
            self.sessions.retain(|_, s| s.station != station);
        }
        let token = crate::portal::new_token();
        self.sessions.insert(token_hash(&token), Session { user, station, last_used: crate::model::now() });
        self.save_sessions();
        token
    }

    /// Usuario de la sesión de la petición, si sigue valiendo.
    fn session(&mut self, req: &Request) -> Option<u32> {
        let h = token_hash(req.header("X-Sesion")?);
        let limit = crate::model::now() - chrono::Duration::hours(SESSION_IDLE_HOURS);
        let s = self.sessions.get(&h)?.clone();
        if s.last_used < limit || !self.user(s.user).is_some_and(|u| u.active) {
            self.sessions.remove(&h);
            self.save_sessions();
            return None;
        }
        if let Some(x) = self.sessions.get_mut(&h) {
            x.last_used = crate::model::now();
        }
        if self.sessions_saved.elapsed() > Duration::from_secs(600) {
            self.save_sessions();
        }
        Some(s.user)
    }

    fn enter(&mut self, req: &Request, station: u32) -> Response {
        if hits(&mut self.logins, req.peer, Duration::from_secs(60), true) > 30 {
            return error(429, "Demasiados intentos de entrada desde este ordenador: espere un minuto.");
        }
        let Ok(b) = serde_json::from_slice::<EnterBody>(&req.body) else { return error(400, "Petición no válida.") };
        match self.modify(|db| db.login(b.usuario, &b.contrasena)) {
            Err(e) => error(500, &e),
            Ok(Err(msg)) => error(403, &msg),
            Ok(Ok(LoginStep::Done)) => {
                let t = self.new_session(b.usuario, station);
                reply(200, json!({ "paso": "dentro", "sesion": t }))
            }
            Ok(Ok(LoginStep::NeedCode)) => {
                let t = crate::portal::new_token();
                self.pending.retain(|_, (_, at)| at.elapsed() < Duration::from_secs(300));
                self.pending.insert(token_hash(&t), (b.usuario, Instant::now()));
                reply(200, json!({ "paso": "codigo", "previa": t }))
            }
        }
    }

    fn code(&mut self, req: &Request, station: u32) -> Response {
        if hits(&mut self.logins, req.peer, Duration::from_secs(60), true) > 30 {
            return error(429, "Demasiados intentos de entrada desde este ordenador: espere un minuto.");
        }
        let Ok(b) = serde_json::from_slice::<CodeBody>(&req.body) else { return error(400, "Petición no válida.") };
        // el código solo sirve justo después de la contraseña correcta
        let h = token_hash(&b.previa);
        if !self.pending.get(&h).is_some_and(|(u, at)| *u == b.usuario && at.elapsed() < Duration::from_secs(300)) {
            return error(403, "Vuelva a escribir la contraseña.");
        }
        let unix = crate::totp::unix_now();
        match self.modify(|db| {
            let r = db.login_code(b.usuario, &b.codigo, unix);
            (r, db.user(b.usuario).is_some_and(|u| !u.active))
        }) {
            Err(e) => error(500, &e),
            Ok((Ok(()), _)) => {
                self.pending.remove(&h);
                let t = self.new_session(b.usuario, station);
                reply(200, json!({ "paso": "dentro", "sesion": t }))
            }
            Ok((Err(msg), blocked)) => {
                if blocked {
                    self.pending.remove(&h);
                }
                reply(403, json!({ "error": msg, "bloqueado": blocked }))
            }
        }
    }

    fn set_pin(&mut self, req: &Request, me: u32) -> Response {
        let Ok(b) = serde_json::from_slice::<PinBody>(&req.body) else { return error(400, "Petición no válida.") };
        let admin = self.is_admin(me);
        let must = self.user(me).is_some_and(|u| u.must_change);
        let r = match (b.usuario == me, b.actual.as_deref()) {
            // la propia, comprobando la actual
            (true, Some(old)) => self.modify(|db| db.change_pin(me, old, &b.nueva)),
            // la propia sin la actual: la inicial, o Dirección
            (true, None) if must || admin => self.modify(|db| db.set_pin(me, &b.nueva)),
            (true, None) => return error(403, "Escriba su contraseña actual."),
            // la de otro usuario: solo Dirección
            (false, _) if admin => self.modify(|db| {
                db.set_pin(b.usuario, &b.nueva)?;
                if let Some(u) = db.users.iter_mut().find(|u| u.id == b.usuario) {
                    u.must_change = b.cambiar_al_entrar;
                }
                Ok(())
            }),
            (false, _) => return error(403, "Solo Dirección puede cambiar la contraseña de otro usuario."),
        };
        match r {
            Err(e) => error(500, &e),
            Ok(Err(msg)) => error(403, &msg),
            Ok(Ok(())) => reply(200, json!({ "ok": true, "rev": rev_of(&self.data) })),
        }
    }

    fn totp(&mut self, req: &Request, me: u32) -> Response {
        let Ok(b) = serde_json::from_slice::<TotpBody>(&req.body) else { return error(400, "Petición no válida.") };
        if b.usuario != me && !self.is_admin(me) {
            return error(403, "Solo Dirección puede cambiar el código del móvil de otro usuario.");
        }
        let by = self.user(me).map(|u| u.name).unwrap_or_default();
        let unix = crate::totp::unix_now();
        let r = match b.accion.as_str() {
            "activar" => self.modify(|db| db.enable_totp(b.usuario, &b.clave, &b.codigo, unix)),
            "quitar" => self.modify(|db| {
                db.disable_totp(b.usuario, &by);
                Ok(())
            }),
            _ => return error(400, "Acción no válida."),
        };
        match r {
            Err(e) => error(500, &e),
            Ok(Err(msg)) => error(403, &msg),
            Ok(Ok(())) => reply(200, json!({ "ok": true, "rev": rev_of(&self.data) })),
        }
    }

    /// Atiende una petición; devuelve la respuesta y si hay que parar.
    fn handle(&mut self, req: &Request, key: &str) -> (Response, bool) {
        let window = Duration::from_secs(600);
        if hits(&mut self.key_fails, req.peer, window, false) >= 20 {
            return (error(429, "Demasiados intentos con una clave incorrecta desde este ordenador: espere unos minutos."), false);
        }
        if !req.header("X-Clave").is_some_and(|k| ct_eq(k.trim().as_bytes(), key.as_bytes())) {
            hits(&mut self.key_fails, req.peer, window, true);
            return (error(401, WRONG_KEY), false);
        }
        let station: u32 = req.header("X-Puesto").and_then(|s| s.parse().ok()).unwrap_or(0);
        let (m, p) = (req.method.as_str(), req.path.as_str());
        let resp = match (m, p) {
            ("GET", "/acceso") => self.access(),
            ("POST", "/registro") => {
                let name = serde_json::from_slice::<Value>(&req.body).ok().and_then(|v| v["nombre"].as_str().map(|s| s.trim().to_string())).unwrap_or_default();
                match self.register(&name) {
                    Ok(n) => reply(200, json!({ "numero": n })),
                    Err(e) => error(400, &e),
                }
            }
            ("POST", "/entrar") => self.enter(req, station),
            ("POST", "/codigo") => self.code(req, station),
            ("POST", "/parar") if req.peer.is_loopback() => return (reply(200, json!({ "ok": true })), true),
            ("POST", "/parar") => error(403, "Solo se puede parar el servidor desde el propio ordenador servidor."),
            _ => {
                let Some(me) = self.session(req) else { return (error(401, SESSION_EXPIRED), false) };
                match (m, p) {
                    ("GET", "/estado") => reply(200, json!({ "rev": rev_of(&self.data) })),
                    ("GET", "/datos") => Response { code: 200, body: public_view(&self.data).to_string() },
                    ("POST", "/guardar") => {
                        let base: u64 = req.param("base").and_then(|b| b.parse().ok()).unwrap_or(0);
                        match serde_json::from_slice::<Value>(&req.body).map_err(|e| e.to_string()).and_then(|v| self.save(base, v, station, me)) {
                            Ok(()) => Response { code: 200, body: public_view(&self.data).to_string() },
                            Err(e) => error(400, &e),
                        }
                    }
                    ("POST", "/contrasena") => self.set_pin(req, me),
                    ("POST", "/movil") => self.totp(req, me),
                    _ => error(404, "no existe"),
                }
            }
        };
        (resp, false)
    }
}

/// Atiende peticiones hasta recibir /parar desde el propio ordenador.
pub fn serve(listener: TcpListener, path: PathBuf, key: String, initial: Value) {
    let tls = match server_tls(&key) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("No se pudo preparar el cifrado del servidor: {e}");
            return;
        }
    };
    let port = listener.local_addr().map(|a| a.port()).ok();
    let (jobs_tx, jobs) = mpsc::channel::<Job>();
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        let active = Arc::new(AtomicUsize::new(0));
        std::thread::spawn(move || {
            for tcp in listener.incoming() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(tcp) = tcp else { continue };
                // demasiadas conexiones a la vez: se cierra esta (los puestos reintentan)
                if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                    continue;
                }
                active.fetch_add(1, Ordering::SeqCst);
                let (tls, tx, active) = (tls.clone(), jobs_tx.clone(), active.clone());
                std::thread::spawn(move || {
                    handle_conn(tcp, tls, tx);
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });
    }
    let mut st = State::new(initial, path);
    for job in jobs {
        let (resp, stop_now) = st.handle(&job.req, &key);
        let _ = job.reply.send(resp);
        if stop_now {
            break;
        }
    }
    stop.store(true, Ordering::SeqCst);
    // despierta la espera de conexiones para que termine
    if let Some(port) = port {
        let _ = TcpStream::connect_timeout(&SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port), Duration::from_secs(1));
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
    let listener = TcpListener::bind(("0.0.0.0", cfg.port)).map_err(|e| format!("No se puede abrir el puerto {}: {e}", cfg.port))?;
    println!("VeodarDental · servidor de datos (conexiones cifradas)");
    println!("  Datos: {}", path.display());
    println!("  Dirección para los puestos: {}:{}", local_ip().unwrap_or_else(|| "IP-de-este-ordenador".into()), cfg.port);
    println!("  Clave de la clínica: {}", cfg.key);
    let initial = serde_json::to_value(&db).map_err(|e| e.to_string())?;
    serve(listener, path, cfg.key, initial);
    Ok(())
}

/// ¿Hay un servidor funcionando en este ordenador?
pub fn local_server_running() -> bool {
    let cfg = server_config();
    if cfg.key.is_empty() {
        return false;
    }
    Remote::new(&format!("127.0.0.1:{}", cfg.port), &cfg.key, 0).access().is_ok()
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
    /// Sesión dada por el servidor al entrar (vacía hasta entonces).
    pub session: String,
    agent: ureq::Agent,
}

/// Lo que el servidor da antes de entrar: la clínica y sus usuarios.
#[derive(Clone, Debug, Deserialize)]
pub struct AccessInfo {
    #[serde(rename = "clinica", default)]
    pub clinic: String,
    #[serde(rename = "usuarios", default)]
    pub users: Vec<crate::users::User>,
}

/// Respuesta del servidor a la contraseña.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// Ha entrado: sesión del puesto.
    Inside(String),
    /// Falta el código del móvil: código previo para pedirlo.
    NeedCode(String),
}

/// Todos los mensajes de un error y de sus causas.
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut s = e.to_string();
    let mut cur = e.source();
    while let Some(c) = cur {
        s.push_str(" · ");
        s.push_str(&c.to_string());
        cur = c.source();
    }
    s
}

fn err_text(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, r) => {
            let t = r.into_string().unwrap_or_default();
            serde_json::from_str::<Value>(&t).ok().and_then(|v| v["error"].as_str().map(|s| s.to_string())).unwrap_or_else(|| if code == 401 { WRONG_KEY.into() } else { format!("error {code} del servidor") })
        }
        ureq::Error::Transport(t) => {
            let all = error_chain(&t);
            if all.contains(KEY_MISMATCH) {
                WRONG_KEY.into()
            } else if all.contains("InvalidContentType") || all.contains("corrupt message") {
                // respuesta sin cifrar: un servidor con una versión anterior
                OLD_SERVER.into()
            } else {
                NO_CONNECTION.into()
            }
        }
    }
}

/// Segundos de espera de cada lectura (en las pruebas, menos).
const READ_TIMEOUT: u64 = if cfg!(test) { 3 } else { 20 };

/// Sin conexión cifrada: ¿contesta en HTTP sin cifrar un servidor con una
/// versión anterior? (Se pregunta sin la clave de la clínica.)
fn old_server_answers(base: &str) -> bool {
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(2)).timeout(Duration::from_secs(3)).build();
    let url = format!("{}/estado", base.replacen("https://", "http://", 1));
    match agent.get(&url).call() {
        // el servidor nuevo contesta en claro solo para pedir que se actualice el puesto
        Err(ureq::Error::Status(426, _)) => false,
        Ok(_) | Err(ureq::Error::Status(..)) => true,
        Err(ureq::Error::Transport(_)) => false,
    }
}

fn read_all(r: ureq::Response) -> Result<String, String> {
    let mut s = String::new();
    r.into_reader().read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(s)
}

fn parse(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|e| format!("Respuesta no válida del servidor: {e}"))
}

impl Remote {
    pub fn new(addr: &str, key: &str, station: u32) -> Self {
        let addr = addr.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
        let addr = if addr.contains(':') { addr.to_string() } else { format!("{addr}:{DEFAULT_PORT}") };
        let key = key.trim().to_string();
        // por lectura y escritura (no en total): enviar los datos de una clínica grande puede tardar
        let mut b = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).timeout_read(Duration::from_secs(READ_TIMEOUT)).timeout_write(Duration::from_secs(60));
        // si el cifrado no se pudiera preparar, el agente normal no aceptaría
        // el certificado del servidor: no conecta, pero nunca va sin cifrar
        if let Ok(tls) = client_tls(&key) {
            b = b.tls_config(tls);
        }
        Remote { base: format!("https://{addr}"), key, station, session: String::new(), agent: b.build() }
    }

    fn request(&self, method: &str, path: &str) -> ureq::Request {
        let mut r = self.agent.request(method, &format!("{}{}", self.base, path)).set("X-Clave", &self.key).set("X-Puesto", &self.station.to_string());
        if !self.session.is_empty() {
            r = r.set("X-Sesion", &self.session);
        }
        r
    }

    fn get(&self, path: &str) -> Result<String, String> {
        read_all(self.request("GET", path).call().map_err(err_text)?)
    }

    fn post(&self, path: &str, body: &str) -> Result<String, String> {
        read_all(self.request("POST", path).set("Content-Type", "application/json").send_string(body).map_err(err_text)?)
    }

    /// Nombre de la clínica y usuarios, para la pantalla de entrada.
    pub fn access(&self) -> Result<AccessInfo, String> {
        serde_json::from_str(&self.get("/acceso")?).map_err(|e| format!("Respuesta no válida del servidor: {e}"))
    }

    /// Comprueba la contraseña en el servidor.
    pub fn login(&self, user: u32, pin: &str) -> Result<Entry, String> {
        let v = parse(&self.post("/entrar", &json!({ "usuario": user, "contrasena": pin }).to_string())?)?;
        match (v["paso"].as_str(), v["sesion"].as_str(), v["previa"].as_str()) {
            (Some("dentro"), Some(s), _) => Ok(Entry::Inside(s.to_string())),
            (Some("codigo"), _, Some(p)) => Ok(Entry::NeedCode(p.to_string())),
            _ => Err("Respuesta no válida del servidor.".into()),
        }
    }

    /// Segundo paso: el código del móvil. Devuelve la sesión.
    pub fn login_code(&self, user: u32, code: &str, previous: &str) -> Result<String, String> {
        let v = parse(&self.post("/codigo", &json!({ "usuario": user, "codigo": code, "previa": previous }).to_string())?)?;
        v["sesion"].as_str().map(|s| s.to_string()).ok_or_else(|| "Respuesta no válida del servidor.".into())
    }

    /// Contraseña nueva: la propia (con la actual, salvo la inicial) o, para
    /// Dirección, la de otro usuario.
    pub fn set_pin(&self, user: u32, current: Option<&str>, new: &str, change_at_login: bool) -> Result<(), String> {
        self.post("/contrasena", &json!({ "usuario": user, "actual": current, "nueva": new, "cambiar_al_entrar": change_at_login }).to_string()).map(|_| ())
    }

    pub fn totp_on(&self, user: u32, secret: &str, code: &str) -> Result<(), String> {
        self.post("/movil", &json!({ "usuario": user, "accion": "activar", "clave": secret, "codigo": code }).to_string()).map(|_| ())
    }

    pub fn totp_off(&self, user: u32) -> Result<(), String> {
        self.post("/movil", &json!({ "usuario": user, "accion": "quitar" }).to_string()).map(|_| ())
    }

    pub fn rev(&self) -> Result<u64, String> {
        Ok(rev_of(&parse(&self.get("/estado")?)?))
    }

    pub fn load(&self) -> Result<(Db, Value), String> {
        let v = parse(&self.get("/datos")?)?;
        let mut db: Db = serde_json::from_value(v.clone()).map_err(|e| format!("Datos del servidor no válidos: {e}"))?;
        // lo que traen las versiones nuevas (llega al servidor con el próximo guardado)
        db.upgrade();
        Ok((db, v))
    }

    pub fn register(&self, name: &str) -> Result<u32, String> {
        let v = parse(&self.post("/registro", &json!({ "nombre": name }).to_string())?)?;
        v["numero"].as_u64().map(|n| n as u32).ok_or_else(|| "Respuesta no válida del servidor".into())
    }

    /// Envía los datos del puesto; devuelve los datos combinados del servidor.
    pub fn save(&self, db: &Db) -> Result<(Db, Value), String> {
        let body = serde_json::to_string(db).map_err(|e| e.to_string())?;
        let v = parse(&self.post(&format!("/guardar?base={}", db.rev), &body)?)?;
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

/// Conecta un puesto: se registra (recibe su número) y recoge la clínica y
/// sus usuarios para la pantalla de entrada. Los datos llegan al entrar.
/// Si el servidor es este mismo ordenador y aún está arrancando, espera un poco.
pub fn connect(r: &mut Remote, name: &str, attempts: u32) -> Result<AccessInfo, String> {
    let mut last = String::new();
    for i in 0..attempts.max(1) {
        if i > 0 {
            std::thread::sleep(Duration::from_millis(250));
        }
        match r.register(name) {
            Ok(n) => {
                r.station = n;
                return r.access();
            }
            Err(e) => last = e,
        }
    }
    if last == NO_CONNECTION && old_server_answers(&r.base) {
        return Err(OLD_SERVER.into());
    }
    Err(last)
}

pub fn is_local(addr: &str) -> bool {
    let a = addr.trim().trim_start_matches("https://").trim_start_matches("http://");
    a.starts_with("127.0.0.1") || a.starts_with("localhost")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    /// Servidor de prueba en un puerto libre, con los datos de ejemplo.
    fn start(key: &str) -> (String, PathBuf, std::thread::JoinHandle<()>) {
        let today = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        let dir = std::env::temp_dir().join(format!("ds_server_test_{}_{}", std::process::id(), key));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clinica.json");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let initial = serde_json::to_value(Db::sample(today)).unwrap();
        let (p2, k2) = (path.clone(), key.to_string());
        let h = std::thread::spawn(move || serve(listener, p2, k2, initial));
        (format!("127.0.0.1:{port}"), path, h)
    }

    fn signed_in(addr: &str, key: &str, name: &str, user: u32, pin: &str) -> Remote {
        let mut r = Remote::new(addr, key, 0);
        connect(&mut r, name, 1).unwrap();
        match r.login(user, pin).unwrap() {
            Entry::Inside(s) => r.session = s,
            Entry::NeedCode(_) => panic!("no debería pedir código"),
        }
        r
    }

    #[test]
    fn server_with_two_clients() {
        let (addr, path, h) = start("CLAVE");

        // clave incorrecta: el certificado no corresponde y no se manda nada
        assert_eq!(Remote::new(&addr, "MAL", 0).access().unwrap_err(), WRONG_KEY);

        let mut ra = Remote::new(&addr, "CLAVE", 0);
        let mut rb = Remote::new(&addr, "CLAVE", 0);
        let info = connect(&mut ra, "Recepción", 1).unwrap();
        connect(&mut rb, "Gabinete 1", 1).unwrap();
        assert_ne!(ra.station, rb.station);
        assert_eq!(ra.register("Recepción").unwrap(), ra.station, "un puesto conserva su número");
        assert!(info.users.len() >= 5);
        assert!(info.users.iter().all(|u| u.salt.is_empty() && u.pin_hash == HIDDEN), "las contraseñas no salen del servidor");

        // sin entrar no hay datos
        assert_eq!(ra.load().unwrap_err(), SESSION_EXPIRED);
        assert!(ra.login(2, "9999").unwrap_err().contains("incorrecta"));
        ra.session = match ra.login(2, "0000").unwrap() {
            Entry::Inside(s) => s,
            e => panic!("{e:?}"),
        };
        rb.session = match rb.login(3, "0000").unwrap() {
            Entry::Inside(s) => s,
            e => panic!("{e:?}"),
        };

        let (mut a, _) = ra.load().unwrap();
        let (mut b, _) = rb.load().unwrap();
        assert!(a.users.iter().all(|u| u.pin_hash == HIDDEN && u.totp_secret.is_empty() || u.totp_secret == HIDDEN));
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
        // el servidor lo ha guardado en disco, con las contraseñas de verdad
        let disk: Db = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(disk.payments.len(), pays + 1);
        assert_eq!(disk.stations.len(), 2);
        assert!(disk.users.iter().all(|u| u.pin_hash.starts_with("pbkdf2$") && !u.salt.is_empty()));
        // las dos entradas quedan en el registro de accesos
        assert!(disk.access_log.iter().filter(|l| l.event == "Entrada").count() >= 2);

        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn only_direction_changes_users_and_secrets_stay_on_the_server() {
        let (addr, path, h) = start("CLAVE2");
        // recepción intenta hacerse Dirección y desbloquear a otro
        let rec = signed_in(&addr, "CLAVE2", "Recepción", 2, "0000");
        let (mut d, _) = rec.load().unwrap();
        d.station = rec.station;
        d.users.iter_mut().find(|u| u.id == 2).unwrap().profile = crate::users::Profile::Direccion;
        d.users.iter_mut().find(|u| u.id == 5).unwrap().name = "Cambiado".into();
        d.users.push(crate::users::new_user(99, "Intruso", crate::users::Profile::Direccion, None, "0000"));
        d.access_log.clear();
        let (back, _) = rec.save(&d).unwrap();
        assert_eq!(back.user(2).unwrap().profile, crate::users::Profile::Recepcion);
        assert_eq!(back.user(5).unwrap().name, "Hig. Campos");
        assert!(back.user(99).is_none());
        assert!(!back.access_log.is_empty(), "no puede vaciar el registro de accesos");

        // Dirección sí cambia usuarios, pero no sus contraseñas por /guardar
        let dir = signed_in(&addr, "CLAVE2", "Despacho", 1, "0000");
        let (mut d, _) = dir.load().unwrap();
        d.station = dir.station;
        d.users.iter_mut().find(|u| u.id == 5).unwrap().name = "Higienista Campos".into();
        d.users.iter_mut().find(|u| u.id == 4).unwrap().pin_hash = "pbkdf2$1$falso".into();
        d.users.push(crate::users::new_user(20, "Nueva", crate::users::Profile::Recepcion, None, "0000"));
        let (back, _) = dir.save(&d).unwrap();
        assert_eq!(back.user(5).unwrap().name, "Higienista Campos");
        assert!(back.user(20).unwrap().must_change);
        let disk: Db = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(crate::users::pin_matches(disk.user(4).unwrap(), "0000"), "la contraseña del doctor no cambia");
        assert!(crate::users::pin_matches(disk.user(20).unwrap(), "0000"));

        // las contraseñas se cambian con su propia orden
        assert!(rec.set_pin(4, None, "582941", true).unwrap_err().contains("Dirección"));
        dir.set_pin(4, None, "582941", true).unwrap();
        let disk: Db = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(crate::users::pin_matches(disk.user(4).unwrap(), "582941"));
        assert!(disk.user(4).unwrap().must_change);
        // la propia, la inicial sin la actual; después ya hace falta la actual
        rec.set_pin(2, None, "739154", false).unwrap();
        assert!(rec.set_pin(2, Some("000000"), "731954", false).unwrap_err().contains("actual"));
        rec.set_pin(2, Some("739154"), "731954", false).unwrap();

        dir.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn phone_code_needs_the_password_first() {
        let (addr, path, h) = start("CLAVE3");
        let dir = signed_in(&addr, "CLAVE3", "Despacho", 1, "0000");
        let secret = crate::totp::base32_encode(b"12345678901234567890");
        let now = crate::totp::unix_now();
        let code = |t: u64| format!("{:06}", crate::totp::code_at(&secret, t).unwrap());
        dir.totp_on(3, &secret, &code(now)).unwrap();
        let disk: Db = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(disk.user(3).unwrap().totp_secret, secret);
        let (d, _) = dir.load().unwrap();
        assert_eq!(d.user(3).unwrap().totp_secret, HIDDEN, "el puesto sabe que lo tiene, pero no la clave");

        let mut r = Remote::new(&addr, "CLAVE3", 0);
        connect(&mut r, "Gabinete", 1).unwrap();
        // sin la contraseña, el código no sirve aunque sea bueno
        assert!(r.login_code(3, &code(now + 30), "0123456789abcdef0123456789abcdef").unwrap_err().contains("contraseña"));
        let previous = match r.login(3, "0000").unwrap() {
            Entry::NeedCode(p) => p,
            e => panic!("{e:?}"),
        };
        let session = r.login_code(3, &code(now + 30), &previous).unwrap();
        r.session = session;
        assert!(r.load().is_ok());
        dir.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn old_versions_get_a_clear_message() {
        // un puesto antiguo (HTTP sin cifrar) contra el servidor nuevo
        let (addr, path, h) = start("CLAVE4");
        let old = ureq::get(&format!("http://{addr}/estado")).set("X-Clave", "CLAVE4").call();
        let msg = match old {
            Err(ureq::Error::Status(426, r)) => r.into_string().unwrap(),
            other => panic!("{other:?}"),
        };
        assert!(msg.contains("versión anterior"), "{msg}");
        // el servidor solo se para desde el propio ordenador y con la clave
        assert_eq!(Remote::new(&addr, "OTRA", 0).stop().unwrap_err(), WRONG_KEY);
        Remote::new(&addr, "CLAVE4", 0).stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());

        // un puesto nuevo contra un servidor antiguo (HTTP sin cifrar), que
        // no contesta al saludo cifrado pero sí a una petición normal
        let plain = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = plain.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            while let Ok(Some(rq)) = plain.recv_timeout(Duration::from_secs(10)) {
                let _ = rq.respond(tiny_http::Response::from_string("{\"error\": \"Clave de la clínica incorrecta\"}").with_status_code(401));
            }
        });
        let t = Instant::now();
        let e = connect(&mut Remote::new(&format!("127.0.0.1:{port}"), "CLAVE", 0), "Puesto", 1).unwrap_err();
        assert_eq!(e, OLD_SERVER);
        assert!(t.elapsed() < Duration::from_secs(10), "sin esperas largas: {:?}", t.elapsed());
        // y si no hay nadie, «sin conexión» enseguida
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = closed.local_addr().unwrap().port();
        drop(closed);
        assert_eq!(connect(&mut Remote::new(&format!("127.0.0.1:{port}"), "CLAVE", 0), "Puesto", 1).unwrap_err(), NO_CONNECTION);
    }

    #[test]
    fn requests_are_limited() {
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let raw = |s: &str| std::io::Cursor::new(s.as_bytes().to_vec());
        let big = format!("POST /guardar HTTP/1.1\r\nContent-Length: {}\r\n\r\n", MAX_BODY + 1);
        assert_eq!(read_request(&mut raw(&big), peer, MAX_BODY).err().unwrap().code, 413);
        let long = format!("GET /{} HTTP/1.1\r\n\r\n", "a".repeat(9000));
        assert_eq!(read_request(&mut raw(&long), peer, MAX_BODY).err().unwrap().code, 431);
        let chunked = "POST /guardar HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n";
        assert_eq!(read_request(&mut raw(chunked), peer, MAX_BODY).err().unwrap().code, 501);
        let ok = "POST /guardar?base=7 HTTP/1.1\r\nx-clave: K\r\nContent-Length: 2\r\n\r\n{}";
        let r = read_request(&mut raw(ok), peer, MAX_BODY).unwrap().unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str(), r.param("base"), r.header("X-Clave"), r.body.as_slice()), ("POST", "/guardar", Some("7"), Some("K"), &b"{}"[..]));

        // claves aleatorias y, con una clave mal 20 veces, el ordenador espera
        assert_ne!(new_key(), new_key());
        assert_eq!(new_key().len(), 12);
        let mut st = State::new(json!({}), std::env::temp_dir().join("ds_limit_test").join("clinica.json"));
        let bad = Request { method: "GET".into(), path: "/acceso".into(), query: String::new(), headers: vec![("X-Clave".into(), "MAL".into())], body: vec![], peer };
        for _ in 0..20 {
            assert_eq!(st.handle(&bad, "BIEN").0.code, 401);
        }
        let good = Request { headers: vec![("X-Clave".into(), "BIEN".into())], ..bad };
        assert_eq!(st.handle(&good, "BIEN").0.code, 429);
    }
}
