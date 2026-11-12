//! Servidor de datos de la clínica y conexión de los puestos (clientes).
//!
//! Un ordenador hace de servidor: guarda el único fichero de datos, hace las
//! copias de seguridad y atiende a los puestos de recepción y gabinetes.
//! Cada puesto trabaja con una copia en memoria y envía solo lo que cambia;
//! el servidor lo combina con lo de los demás (registro a registro, ver
//! [`crate::sync`]) y devuelve solo lo nuevo. Los puestos sin cambios recogen
//! cada pocos segundos lo que han cambiado los demás.
//!
//! Seguridad:
//! - Todo va cifrado con TLS 1.3. El servidor tiene su propia identidad (una
//!   clave Ed25519 creada al azar que no sale de él). La primera vez, antes de
//!   mandarle nada, el puesto le pide que firme un reto con la clave de la
//!   clínica (ligado a su clave pública); si cuadra, apunta su huella y
//!   después no acepta otro servidor. Nadie en la red puede leer los datos ni
//!   hacerse pasar por el servidor (tampoco, una vez dado de alta el puesto,
//!   quien sepa la clave de la clínica). Si la huella cambia (servidor
//!   reinstalado), el puesto la enseña para compararla con la del servidor.
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
//! * GET  /datos              → datos completos (al entrar)
//! * GET  /cambios?desde=REV  → lo que ha cambiado desde esa revisión
//! * POST /cambios?base=REV   → {"cambios": [...]} solo lo cambiado en el puesto
//!   (cada cosa con cómo estaba y cómo está); devuelve lo nuevo desde `base`
//! * POST /contrasena         → {"usuario", "actual"?, "nueva", "cambiar_al_entrar"}
//! * POST /movil              → {"usuario", "accion": "activar"|"quitar", "clave", "codigo"}
//!   POST /parar              → detiene el servidor (solo desde el propio servidor)
//!
//! Cada puesto tiene una sesión, que sigue valiendo con la pantalla bloqueada
//! (los WhatsApp, la cita online y el correo siguen guardando) y se renueva al
//! volver a entrar; caduca tras 24 horas sin usarse.

use crate::model::{Db, sha256_hex};
use crate::shared::StationSeen;
use crate::sync::{self, Change, Snapshot, canonical};
use crate::users::{LoginStep, Perm, ct_eq};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
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
const WRONG_KEY: &str = "La clave de la clínica no es correcta (o ese ordenador no es el servidor de la clínica).";
const NO_CONNECTION: &str = "No hay conexión con el servidor de la clínica.";
/// El servidor no es el que el puesto conocía (otra clave pública).
pub const SERVER_CHANGED: &str = "El servidor de la clínica no es el de siempre: su huella ha cambiado. Si se ha reinstalado o cambiado el servidor, compruebe que la huella nueva es la que muestra el servidor; si no, avise al responsable: alguien podría estar haciéndose pasar por él.";
/// Una sesión sin usar durante este tiempo caduca.
const SESSION_IDLE_HOURS: i64 = 24;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    pub key: String,
    /// Identidad del servidor para el cifrado (clave Ed25519 en PKCS#8, en
    /// hexadecimal). No sale de este ordenador.
    #[serde(default)]
    pub tls_key: String,
}

impl ServerConfig {
    fn identity(&self) -> Option<Vec<u8>> {
        unhex(&self.tls_key).filter(|k| !identity_pin(k).is_empty())
    }

    /// Clave pública del servidor (la que guardan los puestos).
    pub fn pin(&self) -> String {
        self.identity().map(|k| identity_pin(&k)).unwrap_or_default()
    }
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
        .unwrap_or(ServerConfig { port: DEFAULT_PORT, key: String::new(), tls_key: String::new() })
}

/// Configuración del servidor; la crea (con clave nueva) la primera vez.
/// Configuración del servidor; la crea (con clave e identidad nuevas) la
/// primera vez, y añade la identidad a la de una versión anterior.
pub fn ensure_server_config() -> Result<ServerConfig, String> {
    let mut c = server_config();
    if !c.key.is_empty() && c.identity().is_some() {
        return Ok(c);
    }
    if c.key.is_empty() {
        c = ServerConfig { port: DEFAULT_PORT, key: new_key(), tls_key: String::new() };
    }
    c.tls_key = hex(&new_identity()?);
    if let Some(d) = config_path().parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    write_private(&config_path(), &serde_json::to_string_pretty(&c).map_err(|e| e.to_string())?)?;
    Ok(c)
}

/// Escribe un fichero que solo puede leer el usuario de este ordenador
/// (lleva la clave de la clínica y la del servidor).
fn write_private(path: &std::path::Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Dirección de este ordenador en la red local (para decírsela a los puestos).
pub fn local_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.168.1.1:80").ok()?;
    s.local_addr().ok().map(|a| a.ip().to_string())
}

// ───────────────────────── cifrado (TLS) ─────────────────────────

/// Identidad nueva del servidor: clave Ed25519 al azar (PKCS#8). Se guarda solo
/// en el servidor; los puestos guardan su clave pública la primera vez que
/// conectan y después no aceptan otra.
pub fn new_identity() -> Result<Vec<u8>, String> {
    ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).map(|d| d.as_ref().to_vec()).map_err(|_| "No se pudo crear la clave del servidor.".to_string())
}

/// Clave pública (en hexadecimal) de una identidad del servidor.
pub fn identity_pin(pkcs8: &[u8]) -> String {
    use ring::signature::KeyPair;
    ring::signature::Ed25519KeyPair::from_pkcs8_maybe_unchecked(pkcs8).map(|k| hex(k.public_key().as_ref())).unwrap_or_default()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// Huella para comparar a simple vista (la del servidor y la que guardó el
/// puesto): 16 cifras hexadecimales de la SHA-256 de la clave pública.
pub fn fingerprint(pin: &str) -> String {
    if pin.trim().is_empty() {
        return String::new();
    }
    let h = sha256_hex(pin.trim());
    h.chars().take(16).collect::<Vec<_>>().chunks(4).map(|c| c.iter().collect::<String>().to_uppercase()).collect::<Vec<_>>().join(" ")
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn server_tls(identity: &[u8]) -> Result<Arc<rustls::ServerConfig>, String> {
    let der = rustls::pki_types::PrivatePkcs8KeyDer::from(identity.to_vec());
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

/// Señal (en el error de TLS) de que el servidor no es el que el puesto conocía.
const PIN_MISMATCH: &str = "VEODAR: servidor distinto del conocido";

/// El puesto solo acepta el servidor cuya clave pública guardó la primera vez
/// (si aún no la conoce, la acepta y la apunta).
struct PinnedServer {
    /// Huella apuntada (vacía mientras no se conoce: solo para la prueba).
    pin: Arc<Mutex<Vec<u8>>>,
    seen: Arc<Mutex<Vec<u8>>>,
    algs: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl std::fmt::Debug for PinnedServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PinnedServer")
    }
}

impl rustls::client::danger::ServerCertVerifier for PinnedServer {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let mismatch = || rustls::Error::General(PIN_MISMATCH.into());
        let (_, cert) = x509_parser::parse_x509_certificate(end_entity.as_ref()).map_err(|_| mismatch())?;
        let public = cert.public_key().subject_public_key.data.as_ref();
        if let Ok(mut s) = self.seen.lock() {
            *s = public.to_vec();
        }
        let pin = self.pin.lock().map(|p| p.clone()).unwrap_or_default();
        if pin.is_empty() || ct_eq(public, &pin) {
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

fn client_tls(pin: Arc<Mutex<Vec<u8>>>, seen: Arc<Mutex<Vec<u8>>>) -> Result<Arc<rustls::ClientConfig>, String> {
    let p = provider();
    let verifier = PinnedServer { pin, seen, algs: p.signature_verification_algorithms };
    let cfg = rustls::ClientConfig::builder_with_provider(p)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

// ─────────────── prueba de que el servidor sabe la clave ───────────────
//
// La primera vez que un puesto conecta (al darse de alta, o tras actualizar)
// aún no conoce la huella del servidor. Antes de mandarle nada, le pide que
// firme un reto al azar junto con su propia clave pública, con una clave que
// sale de la clave de la clínica. Solo el servidor de la clínica puede firmarlo,
// y la firma no vale para otra clave pública (un intermediario no puede
// reenviar la del servidor de verdad). Si cuadra, el puesto apunta la huella.

const PROOF_SALT: &[u8] = b"VeodarDental prueba del servidor v1";
/// Vueltas de PBKDF2: con la firma de un reto no se puede adivinar la clave probando.
const PROOF_ROUNDS: u32 = if cfg!(test) { 1_000 } else { 100_000 };

/// Clave de las firmas, sacada de la clave de la clínica (una vez por clave).
fn proof_key(clinic_key: &str) -> ring::hmac::Key {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<String, [u8; 32]>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let cached = cache.lock().ok().and_then(|c| c.get(clinic_key).copied());
    let derived = cached.unwrap_or_else(|| {
        let mut out = [0u8; 32];
        let rounds = std::num::NonZeroU32::new(PROOF_ROUNDS).unwrap_or(std::num::NonZeroU32::MIN);
        ring::pbkdf2::derive(ring::pbkdf2::PBKDF2_HMAC_SHA256, rounds, PROOF_SALT, clinic_key.as_bytes(), &mut out);
        if let Ok(mut c) = cache.lock() {
            c.insert(clinic_key.to_string(), out);
        }
        out
    });
    ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &derived)
}

fn proof_message(challenge: &[u8], public: &[u8]) -> Vec<u8> {
    let mut m = b"VeodarDental servidor v1|".to_vec();
    m.extend_from_slice(challenge);
    m.push(b'|');
    m.extend_from_slice(public);
    m
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

/// Lee la línea de la petición y las cabeceras (el cuerpo, aparte y solo si
/// trae la clave de la clínica); `None` si la conexión se cierra sin mandar nada.
fn read_head(r: &mut impl BufRead, peer: IpAddr) -> Result<Option<Request>, Response> {
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
    Ok(Some(req))
}

/// Longitud del cuerpo que anuncia la petición.
fn body_len(req: &Request) -> Result<usize, Response> {
    match req.header("Content-Length") {
        Some(v) => v.parse().map_err(|_| error(400, "Longitud no válida.")),
        None => Ok(0),
    }
}

/// Lee el cuerpo poco a poco (sin reservar de golpe lo que diga la petición).
fn read_body(r: &mut impl BufRead, req: &mut Request, max_body: usize) -> Result<(), Response> {
    let len = body_len(req)?;
    if len > max_body {
        return Err(error(413, "Los datos enviados son demasiado grandes."));
    }
    let mut body = Vec::with_capacity(len.min(1 << 20));
    r.by_ref().take(len as u64).read_to_end(&mut body).map_err(|_| error(400, "Petición incompleta."))?;
    if body.len() != len {
        return Err(error(400, "Petición incompleta."));
    }
    req.body = body;
    Ok(())
}

/// ¿Trae la clave de la clínica? Sin ella no se lee el cuerpo de la petición.
fn has_key(req: &Request, key: &str) -> bool {
    req.header("X-Clave").is_some_and(|k| ct_eq(k.trim().as_bytes(), key.as_bytes()))
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
fn handle_conn(tcp: TcpStream, tls: Arc<rustls::ServerConfig>, key: &str, jobs: mpsc::Sender<Job>) {
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
        // un cuerpo pequeño se lee para que la respuesta le llegue bien; uno grande, no
        let mut r = BufReader::new(&tcp);
        if let Ok(Some(mut req)) = read_head(&mut r, peer) {
            let _ = read_body(&mut r, &mut req, 64 * 1024);
        }
        let _ = write_response(&mut &tcp, &error(426, OLD_STATION));
        return;
    }
    let Ok(conn) = rustls::ServerConnection::new(tls) else { return };
    let mut s = rustls::StreamOwned::new(conn, tcp);
    let read = {
        let mut r = BufReader::new(&mut s);
        match read_head(&mut r, peer) {
            // sin la clave no se lee el cuerpo: el servidor contesta (y apunta el intento)
            Ok(Some(mut req)) if has_key(&req, key) => read_body(&mut r, &mut req, MAX_BODY).map(|_| Some(req)),
            other => other,
        }
    };
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

/// Revisiones de las que se recuerda qué se tocó (para dar a los puestos solo
/// lo nuevo; un puesto más atrasado recibe los datos enteros).
const LOG_REVS: usize = 5000;

/// Campos que cambian las entradas y las contraseñas.
const AUTH_FIELDS: [&str; 3] = ["users", "access_log", "config"];

struct State {
    /// Los datos, registro a registro (texto JSON compacto de cada uno).
    snap: Snapshot,
    /// Qué se tocó en cada revisión.
    log: VecDeque<(u64, sync::Touched)>,
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
    proofs: HashMap<IpAddr, VecDeque<Instant>>,
    /// Clave pública del servidor y clave de las firmas de la prueba.
    public: Vec<u8>,
    proof: ring::hmac::Key,
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

/// Un usuario para un puesto: sin la huella de la contraseña ni la clave del móvil.
pub fn strip_user_json(json: &str) -> String {
    let Ok(mut u) = serde_json::from_str::<Value>(json) else { return json.to_string() };
    for f in ["pin_hash", "totp_secret"] {
        if u.get(f).and_then(|x| x.as_str()).is_some_and(|s| !s.is_empty()) {
            u[f] = json!(HIDDEN);
        }
    }
    u["salt"] = json!("");
    u["totp_last"] = json!(0);
    canonical("users", &u.to_string()).unwrap_or_else(|_| u.to_string())
}

/// Un usuario que manda un puesto (Dirección): con lo que solo guarda el
/// servidor (contraseña, clave del móvil, fallos) sacado del actual.
fn protect_user_json(json: &str, current: Option<&String>) -> Result<String, String> {
    let mut u: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    match current.and_then(|c| serde_json::from_str::<Value>(c).ok()) {
        Some(c) => {
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
    canonical("users", &u.to_string())
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

#[derive(Deserialize)]
struct PushBody {
    #[serde(rename = "cambios", default)]
    changes: Vec<Change>,
}

impl State {
    fn new(initial: &Db, path: PathBuf, key: &str, public: Vec<u8>) -> State {
        let sessions_path = path.with_file_name("sesiones.json");
        let limit = crate::model::now() - chrono::Duration::hours(SESSION_IDLE_HOURS);
        let sessions: HashMap<String, Session> = std::fs::read_to_string(sessions_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let sessions = sessions.into_iter().filter(|(_, s)| s.last_used > limit).collect();
        State {
            snap: Snapshot::from_db(initial),
            log: VecDeque::new(),
            path,
            last_backup_day: None,
            sessions,
            sessions_saved: Instant::now(),
            pending: HashMap::new(),
            key_fails: HashMap::new(),
            logins: HashMap::new(),
            proofs: HashMap::new(),
            public,
            proof: proof_key(key),
        }
    }

    fn rev(&self) -> u64 {
        self.snap.rev()
    }

    fn persist(&mut self) -> Result<(), String> {
        let tmp = self.path.with_extension("json.tmp");
        if let Some(d) = self.path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&tmp, self.snap.to_json()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
        // copia de seguridad diaria en el servidor
        let today = crate::model::today();
        if self.last_backup_day != Some(today) && crate::tools::needs_daily(today) {
            if let Ok(db) = self.snap.to_db() {
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

    /// Nueva revisión con lo tocado, y a disco.
    fn commit(&mut self, touched: Vec<(String, Option<String>)>) -> Result<(), String> {
        if touched.is_empty() {
            return Ok(());
        }
        let rev = self.rev() + 1;
        self.snap.set_rev(rev);
        self.log.push_back((rev, touched));
        while self.log.len() > LOG_REVS {
            self.log.pop_front();
        }
        self.persist()
    }

    /// Cambia algunos campos con las funciones del programa (entradas,
    /// contraseñas, puestos…), sin convertir los datos enteros.
    fn modify<T>(&mut self, fields: &[&str], f: impl FnOnce(&mut Db) -> T) -> Result<T, String> {
        let mut db = sync::partial_db(&self.snap, fields)?;
        let out = f(&mut db);
        let mut touched = vec![];
        for ch in sync::changes_in(&self.snap, &db, fields) {
            self.snap.set(&ch.field, ch.key.as_deref(), ch.after);
            touched.push((ch.field, ch.key));
        }
        self.commit(touched)?;
        Ok(out)
    }

    fn user(&self, id: u32) -> Option<crate::users::User> {
        serde_json::from_str(self.snap.get("users", Some(&id.to_string()))?).ok()
    }

    fn is_admin(&self, id: u32) -> bool {
        self.user(id).is_some_and(|u| u.active && u.profile.can(Perm::Configuracion))
    }

    /// Aplica los cambios de un puesto, combinándolos con los de los demás.
    /// Devuelve todo lo que mandó el puesto (también lo que no se ha aceptado,
    /// para que el puesto se quede con lo que de verdad hay en el servidor).
    fn save(&mut self, mut changes: Vec<Change>, station: u32, user: u32) -> Result<Vec<(String, Option<String>)>, String> {
        let admin = self.is_admin(user);
        let sent: Vec<(String, Option<String>)> = changes.iter().map(|c| (c.field.clone(), c.key.clone())).collect();
        // los puestos (y la revisión) los lleva el servidor; los usuarios solo los
        // cambia Dirección, y nunca sus contraseñas ni los fallos de acceso
        changes.retain(|c| c.field != "stations" && c.field != "rev" && (c.field != "users" || admin));
        for c in changes.iter_mut().filter(|c| c.field == "users") {
            let current = c.key.as_deref().and_then(|k| self.snap.get("users", Some(k))).cloned();
            c.before = c.before.as_deref().map(|b| protect_user_json(b, current.as_ref())).transpose()?;
            c.after = c.after.as_deref().map(|a| protect_user_json(a, current.as_ref())).transpose()?;
        }
        let mut touched = sync::apply(&mut self.snap, &changes)?.touched;
        if station > 0 {
            let key = station.to_string();
            if let Some(mut s) = self.snap.get("stations", Some(&key)).and_then(|j| serde_json::from_str::<StationSeen>(j).ok()) {
                s.last_seen = crate::model::now();
                self.snap.set("stations", Some(&key), serde_json::to_string(&s).ok());
                touched.push(("stations".into(), Some(key)));
            }
        }
        self.commit(touched)?;
        Ok(sent)
    }

    /// Lo que ha cambiado desde la revisión `since`, más lo de `also` (o, si
    /// `since` es de hace mucho, que el puesto pida los datos enteros).
    fn since(&self, since: u64, also: &[(String, Option<String>)]) -> Response {
        let rev = self.rev();
        if since < rev && self.log.front().is_none_or(|(first, _)| *first > since + 1) {
            return reply(200, json!({ "rev": rev, "completo": true }));
        }
        let mut seen = std::collections::HashSet::new();
        let mut out = vec![];
        let logged = self.log.iter().filter(|(r, _)| *r > since).flat_map(|(_, t)| t.iter());
        for (field, key) in logged.chain(also.iter()) {
            if !seen.insert((field.clone(), key.clone())) {
                continue;
            }
            let mut after = self.snap.get(field, key.as_deref()).cloned();
            if field == "users" {
                after = after.map(|u| strip_user_json(&u));
            }
            out.push(Change { field: field.clone(), key: key.clone(), before: None, after });
        }
        reply(200, json!({ "rev": rev, "cambios": out }))
    }

    fn register(&mut self, name: &str) -> Result<u32, String> {
        self.modify(&["stations"], |db| match db.stations.iter().find(|s| s.name == name) {
            Some(s) => Ok(s.id),
            None => {
                let n = (1..=15).find(|n| !db.stations.iter().any(|s| s.id == *n)).ok_or("No caben más puestos (máximo 15).")?;
                db.stations.push(StationSeen { id: n, name: name.into(), last_seen: crate::model::now() });
                Ok(n)
            }
        })?
    }

    /// Lo que necesita la pantalla de entrada (antes de entrar).
    fn access(&self) -> Response {
        let users: Vec<Value> = self.snap.records.get("users").into_iter().flat_map(|r| r.iter()).filter_map(|(_, u)| serde_json::from_str(&strip_user_json(u)).ok()).collect();
        let name = self.snap.get("config", None).and_then(|c| serde_json::from_str::<Value>(c).ok()).map(|c| c["name"].clone()).unwrap_or(Value::Null);
        reply(200, json!({ "clinica": name, "usuarios": users, "rev": self.rev() }))
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
        match self.modify(&AUTH_FIELDS, |db| db.login(b.usuario, &b.contrasena)) {
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
        match self.modify(&AUTH_FIELDS, |db| {
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
            (true, Some(old)) => self.modify(&AUTH_FIELDS, |db| db.change_pin(me, old, &b.nueva)),
            // la propia sin la actual: la inicial, o Dirección
            (true, None) if must || admin => self.modify(&AUTH_FIELDS, |db| db.set_pin(me, &b.nueva)),
            (true, None) => return error(403, "Escriba su contraseña actual."),
            // la de otro usuario: solo Dirección
            (false, _) if admin => self.modify(&AUTH_FIELDS, |db| {
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
            Ok(Ok(())) => reply(200, json!({ "ok": true, "rev": self.rev() })),
        }
    }

    fn totp(&mut self, req: &Request, me: u32) -> Response {
        let Ok(b) = serde_json::from_slice::<TotpBody>(&req.body) else { return error(400, "Petición no válida.") };
        // como en la pantalla (Configuración › Usuarios): solo Dirección. Así nadie
        // con la sesión de otro puede quitarle o cambiarle el código del móvil.
        if !self.is_admin(me) {
            return error(403, "Solo Dirección puede activar o quitar el código del móvil.");
        }
        let by = self.user(me).map(|u| u.name).unwrap_or_default();
        let unix = crate::totp::unix_now();
        let r = match b.accion.as_str() {
            "activar" => self.modify(&AUTH_FIELDS, |db| db.enable_totp(b.usuario, &b.clave, &b.codigo, unix)),
            "quitar" => self.modify(&AUTH_FIELDS, |db| {
                db.disable_totp(b.usuario, &by);
                Ok(())
            }),
            _ => return error(400, "Acción no válida."),
        };
        match r {
            Err(e) => error(500, &e),
            Ok(Err(msg)) => error(403, &msg),
            Ok(Ok(())) => reply(200, json!({ "ok": true, "rev": self.rev() })),
        }
    }

    /// Atiende una petición; devuelve la respuesta y si hay que parar.
    fn handle(&mut self, req: &Request, key: &str) -> (Response, bool) {
        let window = Duration::from_secs(600);
        // la prueba de identidad va sin la clave de la clínica (el puesto aún no sabe si es el servidor bueno)
        if req.method == "GET" && req.path == "/prueba" {
            if hits(&mut self.proofs, req.peer, window, true) > 30 {
                return (error(429, "Demasiadas comprobaciones desde este ordenador: espere unos minutos."), false);
            }
            let Some(challenge) = req.param("reto").and_then(unhex).filter(|c| c.len() == 16) else { return (error(400, "Reto no válido."), false) };
            let tag = ring::hmac::sign(&self.proof, &proof_message(&challenge, &self.public));
            return (reply(200, json!({ "prueba": hex(tag.as_ref()) })), false);
        }
        if hits(&mut self.key_fails, req.peer, window, false) >= 20 {
            return (error(429, "Demasiados intentos con una clave incorrecta desde este ordenador: espere unos minutos."), false);
        }
        if !has_key(req, key) {
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
                let param = |k: &str| req.param(k).and_then(|b| b.parse::<u64>().ok()).unwrap_or(0);
                match (m, p) {
                    ("GET", "/estado") => reply(200, json!({ "rev": self.rev() })),
                    ("GET", "/datos") => Response { code: 200, body: self.snap.to_json_with(&strip_user_json) },
                    ("GET", "/cambios") => self.since(param("desde"), &[]),
                    ("POST", "/cambios") => match serde_json::from_slice::<PushBody>(&req.body).map_err(|e| e.to_string()).and_then(|b| self.save(b.changes, station, me)) {
                        Ok(sent) => self.since(param("base"), &sent),
                        Err(e) => error(400, &e),
                    },
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
pub fn serve(listener: TcpListener, path: PathBuf, key: String, identity: Vec<u8>, initial: Db) {
    let tls = match server_tls(&identity) {
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
        let key_for_conns = Arc::new(key.clone());
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
                let (tls, tx, active, key) = (tls.clone(), jobs_tx.clone(), active.clone(), key_for_conns.clone());
                std::thread::spawn(move || {
                    handle_conn(tcp, tls, &key, tx);
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });
    }
    let public = unhex(&identity_pin(&identity)).unwrap_or_default();
    let mut st = State::new(&initial, path, &key, public);
    drop(initial);
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
    let cfg = ensure_server_config()?;
    let identity = cfg.identity().ok_or("La identidad del servidor está dañada: borre servidor.json y vuelva a conectar los puestos.")?;
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
    println!("  Huella del servidor: {} (la misma que ven los puestos)", fingerprint(&cfg.pin()));
    match crate::disco::check_now() {
        crate::disco::Disk::Plain(why) => println!("  ⚠ {why} Los datos de salud de los pacientes están en este disco."),
        crate::disco::Disk::Unknown(why) => println!("  ⚠ Cifrado del disco: {why}"),
        _ => {}
    }
    serve(listener, path, cfg.key, identity, db);
    Ok(())
}

/// ¿Hay un servidor funcionando en este ordenador?
pub fn local_server_running() -> bool {
    let cfg = server_config();
    if cfg.key.is_empty() {
        return false;
    }
    local_remote(&cfg).access().is_ok()
}

/// Conexión con el servidor de este mismo ordenador (con su huella).
pub fn local_remote(cfg: &ServerConfig) -> Remote {
    Remote::with_pin(&format!("127.0.0.1:{}", cfg.port), &cfg.key, 0, &cfg.pin())
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
    /// Huella del servidor que acepta (vacía hasta que el servidor demuestra
    /// que sabe la clave de la clínica) y la que ha visto al conectar.
    pin: Arc<Mutex<Vec<u8>>>,
    seen: Arc<Mutex<Vec<u8>>>,
    /// Una sola prueba a la vez.
    verifying: Arc<Mutex<()>>,
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

/// Lo que devuelve el servidor: su revisión y lo que ha cambiado.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Delta {
    pub rev: u64,
    #[serde(rename = "cambios", default)]
    pub changes: Vec<Change>,
    /// El puesto está demasiado atrasado: tiene que pedir los datos enteros.
    #[serde(rename = "completo", default)]
    pub full: bool,
}

/// Lo que ha cambiado en el puesto respecto al servidor. El usuario y el
/// centro de este puesto no cuentan como cambios (cada puesto tiene los suyos).
fn station_changes(db: &Db, synced: &Snapshot) -> Vec<Change> {
    let mut changes = sync::diff_db(synced, db);
    let server = synced.plain.get("config").and_then(|c| serde_json::from_str::<Value>(c).ok());
    changes.retain_mut(|ch| {
        if ch.field != "config" || ch.key.is_some() {
            return true;
        }
        let (Some(theirs), Some(Ok(mut ours))) = (&server, ch.after.as_deref().map(serde_json::from_str::<Value>)) else { return true };
        ours["current_user"] = theirs["current_user"].clone();
        ours["current_center"] = theirs["current_center"].clone();
        match canonical("config", &ours.to_string()) {
            Ok(c) if ch.before.as_deref() == Some(c.as_str()) => false,
            Ok(c) => {
                ch.after = Some(c);
                true
            }
            Err(_) => true,
        }
    });
    changes
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
            if all.contains(PIN_MISMATCH) {
                SERVER_CHANGED.into()
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
    /// Conexión con un servidor que el puesto aún no conoce (al darse de alta):
    /// antes de mandarle nada, el servidor demuestra que sabe la clave de la
    /// clínica, y entonces se apunta su huella (ver [`Remote::pinned`]).
    pub fn new(addr: &str, key: &str, station: u32) -> Self {
        Self::with_pin(addr, key, station, "")
    }

    /// Conexión que solo acepta el servidor con esa clave pública (la que
    /// guardó el puesto al darse de alta); vacía, como [`Remote::new`].
    pub fn with_pin(addr: &str, key: &str, station: u32, pin: &str) -> Self {
        let addr = addr.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
        let addr = if addr.contains(':') { addr.to_string() } else { format!("{addr}:{DEFAULT_PORT}") };
        let key = key.trim().to_string();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let pin = Arc::new(Mutex::new(unhex(pin).unwrap_or_default()));
        // por lectura y escritura (no en total): enviar los datos de una clínica grande puede tardar
        let mut b = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).timeout_read(Duration::from_secs(READ_TIMEOUT)).timeout_write(Duration::from_secs(60));
        // si el cifrado no se pudiera preparar, el agente normal no aceptaría
        // el certificado del servidor: no conecta, pero nunca va sin cifrar
        if let Ok(tls) = client_tls(pin.clone(), seen.clone()) {
            b = b.tls_config(tls);
        }
        Remote { base: format!("https://{addr}"), key, station, session: String::new(), pin, seen, verifying: Arc::new(Mutex::new(())), agent: b.build() }
    }

    /// Clave pública (hexadecimal) del servidor que ha visto esta conexión, aunque
    /// no sea la conocida (para enseñar la huella nueva); vacía si no ha conectado.
    pub fn server_pin(&self) -> String {
        self.seen.lock().map(|s| hex(&s)).unwrap_or_default()
    }

    /// Huella comprobada del servidor (la que el puesto debe guardar); vacía
    /// si el servidor aún no ha demostrado saber la clave de la clínica.
    pub fn pinned(&self) -> String {
        self.pin.lock().map(|p| hex(&p)).unwrap_or_default()
    }

    /// Antes de mandarle nada a un servidor cuya huella aún no se conoce, este
    /// tiene que firmar un reto al azar con la clave de la clínica.
    ///
    /// La prueba va por una conexión aparte, de un solo uso (sin guardarla para
    /// reutilizarla), y la firma se comprueba con la clave pública de esa misma
    /// conexión. La conexión de trabajo no se abre hasta tener la huella
    /// comprobada, así que nunca habla con un servidor sin comprobar.
    fn verify_server(&self) -> Result<(), String> {
        let _one_at_a_time = self.verifying.lock().unwrap_or_else(|e| e.into_inner());
        if self.pin.lock().map(|p| !p.is_empty()).unwrap_or(false) {
            return Ok(());
        }
        use ring::rand::SecureRandom;
        let mut challenge = [0u8; 16];
        ring::rand::SystemRandom::new().fill(&mut challenge).map_err(|_| "El sistema no ha dado números aleatorios.".to_string())?;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let tls = client_tls(Arc::new(Mutex::new(Vec::new())), seen.clone())?;
        let probe = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(3))
            .timeout_read(Duration::from_secs(READ_TIMEOUT))
            .max_idle_connections(0)
            .tls_config(tls)
            .build();
        // sin la clave de la clínica ni la sesión
        let answer = probe.get(&format!("{}/prueba?reto={}", self.base, hex(&challenge))).call().map_err(err_text).and_then(read_all);
        drop(probe);
        let public = seen.lock().map(|s| s.clone()).unwrap_or_default();
        // para enseñar la huella que se ha visto, aunque no se acepte
        if let Ok(mut s) = self.seen.lock() {
            s.clone_from(&public);
        }
        let tag = parse(&answer?)?["prueba"].as_str().and_then(unhex).ok_or(WRONG_KEY)?;
        if public.is_empty() || ring::hmac::verify(&proof_key(&self.key), &proof_message(&challenge, &public), &tag).is_err() {
            return Err(WRONG_KEY.into());
        }
        if let Ok(mut p) = self.pin.lock() {
            *p = public;
        }
        Ok(())
    }

    /// Dirección del servidor (sin «https://»).
    pub fn address(&self) -> &str {
        self.base.trim_start_matches("https://")
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    fn request(&self, method: &str, path: &str) -> ureq::Request {
        let mut r = self.agent.request(method, &format!("{}{}", self.base, path)).set("X-Clave", &self.key).set("X-Puesto", &self.station.to_string());
        if !self.session.is_empty() {
            r = r.set("X-Sesion", &self.session);
        }
        r
    }

    fn get(&self, path: &str) -> Result<String, String> {
        self.verify_server()?;
        read_all(self.request("GET", path).call().map_err(err_text)?)
    }

    fn post(&self, path: &str, body: &str) -> Result<String, String> {
        self.verify_server()?;
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

    #[cfg(test)]
    pub fn rev(&self) -> Result<u64, String> {
        Ok(parse(&self.get("/estado")?)?["rev"].as_u64().unwrap_or(0))
    }

    /// Los datos enteros (al entrar): los datos y cómo están en el servidor.
    pub fn load(&self) -> Result<(Db, Snapshot), String> {
        let text = self.get("/datos")?;
        let mut db: Db = serde_json::from_str(&text).map_err(|e| format!("Datos del servidor no válidos: {e}"))?;
        drop(text);
        let synced = Snapshot::from_db(&db);
        sync::mark_synced(&mut db);
        // lo que traen las versiones nuevas (llega al servidor con el próximo guardado)
        db.upgrade();
        Ok((db, synced))
    }

    pub fn register(&self, name: &str) -> Result<u32, String> {
        let v = parse(&self.post("/registro", &json!({ "nombre": name }).to_string())?)?;
        v["numero"].as_u64().map(|n| n as u32).ok_or_else(|| "Respuesta no válida del servidor".into())
    }

    /// Lo que ha cambiado en el servidor desde la revisión `since`.
    pub fn pull(&self, since: u64) -> Result<Delta, String> {
        serde_json::from_str(&self.get(&format!("/cambios?desde={since}"))?).map_err(|e| format!("Respuesta no válida del servidor: {e}"))
    }

    /// Envía al servidor lo que ha cambiado en el puesto (si no ha cambiado
    /// nada, no envía nada) y pone en sus datos lo que devuelve el servidor
    /// (lo suyo combinado y lo de los demás). Devuelve cuántos cambios envió.
    pub fn push(&self, db: &mut Db, synced: &mut Snapshot) -> Result<usize, String> {
        let changes = station_changes(db, synced);
        if changes.is_empty() {
            sync::mark_synced(db);
            return Ok(0);
        }
        let body = json!({ "cambios": changes }).to_string();
        let delta: Delta = serde_json::from_str(&self.post(&format!("/cambios?base={}", db.rev), &body)?).map_err(|e| format!("Respuesta no válida del servidor: {e}"))?;
        // lo enviado ya está en el servidor (y lo que haya cambiado al combinar, llega en la respuesta)
        for ch in &changes {
            synced.set(&ch.field, ch.key.as_deref(), ch.after.clone());
        }
        sync::mark_synced(db);
        self.receive(db, synced, delta)?;
        Ok(changes.len())
    }

    /// Pone en los datos del puesto lo que ha cambiado en el servidor.
    pub fn receive(&self, db: &mut Db, synced: &mut Snapshot, delta: Delta) -> Result<(), String> {
        if delta.full {
            // el puesto estaba demasiado atrasado: los datos enteros
            let (fresh, snap) = self.load()?;
            *db = adopt(db, fresh);
            *synced = snap;
            return Ok(());
        }
        // el usuario y el centro son de cada puesto
        let (user, center) = (db.config.current_user.clone(), db.config.current_center);
        sync::apply_to_db(db, synced, &delta.changes)?;
        db.config.current_user = user;
        db.config.current_center = center;
        db.rev = delta.rev;
        Ok(())
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
        let initial = Db::sample(today);
        let (p2, k2) = (path.clone(), key.to_string());
        let identity = new_identity().unwrap();
        let h = std::thread::spawn(move || serve(listener, p2, k2, identity, initial));
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

        // clave incorrecta
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

        let (mut a, mut sa) = ra.load().unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        assert!(a.users.iter().all(|u| u.pin_hash == HIDDEN && u.salt.is_empty()));
        a.station = ra.station;
        b.station = rb.station;
        // lo que trae la versión nueva al cargar llega con el primer envío; después, sin cambios no se envía nada
        ra.push(&mut a, &mut sa).unwrap();
        rb.push(&mut b, &mut sb).unwrap();
        assert_eq!(ra.push(&mut a, &mut sa).unwrap(), 0, "sin cambios no se envía nada");
        let pays = a.payments.len();
        a.add_payment(100, 25.0, "Tarjeta", "desde recepción");
        b.appointments[0].notes = "nota del gabinete".into();
        let ida = a.new_id();
        let idb = b.new_id();
        assert_ne!(ida, idb);

        // solo van los registros cambiados (el cobro, la caja, next_id…), no los datos enteros
        let sent = ra.push(&mut a, &mut sa).unwrap();
        assert!((1..=6).contains(&sent), "{sent} cambios");
        rb.push(&mut b, &mut sb).unwrap();
        assert_eq!(b.payments.len(), pays + 1, "el gabinete recibe el cobro de recepción al guardar");
        assert_eq!(b.appointments[0].notes, "nota del gabinete");
        assert_eq!(rb.rev().unwrap(), b.rev);
        // recepción recoge lo del gabinete (solo lo nuevo)
        let d = ra.pull(a.rev).unwrap();
        assert!(!d.full && d.changes.iter().any(|c| c.field == "appointments"));
        ra.receive(&mut a, &mut sa, d).unwrap();
        assert_eq!(a.appointments[0].notes, "nota del gabinete");
        assert_eq!(a.rev, b.rev);
        assert!(ra.pull(a.rev).unwrap().changes.is_empty(), "al día: nada nuevo");
        // lo recibido no vuelve como cambio del puesto
        assert_eq!(ra.push(&mut a, &mut sa).unwrap(), 0);
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
        let (mut d, mut sd) = rec.load().unwrap();
        d.station = rec.station;
        d.users.iter_mut().find(|u| u.id == 2).unwrap().profile = crate::users::Profile::Direccion;
        d.users.iter_mut().find(|u| u.id == 5).unwrap().name = "Cambiado".into();
        d.users.push(crate::users::new_user(99, "Intruso", crate::users::Profile::Direccion, None, "0000"));
        d.access_log.clear();
        rec.push(&mut d, &mut sd).unwrap();
        let disk = || -> Db { serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap() };
        assert_eq!(disk().user(2).unwrap().profile, crate::users::Profile::Recepcion);
        assert_eq!(disk().user(5).unwrap().name, "Hig. Campos");
        assert!(disk().user(99).is_none());
        assert!(!disk().access_log.is_empty(), "no puede vaciar el registro de accesos");

        // Dirección sí cambia usuarios, pero no sus contraseñas por /guardar
        let dir = signed_in(&addr, "CLAVE2", "Despacho", 1, "0000");
        let (mut d, mut sd) = dir.load().unwrap();
        d.station = dir.station;
        d.users.iter_mut().find(|u| u.id == 5).unwrap().name = "Higienista Campos".into();
        d.users.iter_mut().find(|u| u.id == 4).unwrap().pin_hash = "pbkdf2$1$falso".into();
        d.users.push(crate::users::new_user(20, "Nueva", crate::users::Profile::Recepcion, None, "0000"));
        dir.push(&mut d, &mut sd).unwrap();
        assert_eq!(d.user(5).unwrap().name, "Higienista Campos");
        assert!(d.user(20).unwrap().must_change);
        assert_eq!(d.user(4).unwrap().pin_hash, HIDDEN, "al puesto le vuelve sin la contraseña");
        assert!(crate::users::pin_matches(disk().user(4).unwrap(), "0000"), "la contraseña del doctor no cambia");
        assert!(crate::users::pin_matches(disk().user(20).unwrap(), "0000"));
        // Dirección puede borrar un usuario
        d.users.retain(|u| u.id != 20);
        dir.push(&mut d, &mut sd).unwrap();
        assert!(disk().user(20).is_none());

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
        // solo Dirección lo activa o lo quita (ni para sí mismo, con la sesión de otro)
        let doc = signed_in(&addr, "CLAVE3", "Gabinete 2", 4, "0000");
        assert!(doc.totp_on(4, &secret, &code(now)).unwrap_err().contains("Dirección"));
        dir.totp_on(3, &secret, &code(now)).unwrap();
        assert!(doc.totp_off(3).unwrap_err().contains("Dirección"));
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
    fn stations_only_accept_the_server_they_know() {
        let (addr, path, h) = start("CLAVE5");
        // al darse de alta, el puesto apunta la huella del servidor
        let mut first = Remote::new(&addr, "CLAVE5", 0);
        connect(&mut first, "Recepción", 1).unwrap();
        let pin = first.pinned();
        assert_eq!(pin.len(), 64);
        assert_eq!(pin, first.server_pin());
        // con esa huella conecta; con otra (otro servidor, o alguien haciéndose pasar por él), no
        assert!(Remote::with_pin(&addr, "CLAVE5", 0, &pin).access().is_ok());
        let other = identity_pin(&new_identity().unwrap());
        let fake = Remote::with_pin(&addr, "CLAVE5", 0, &other);
        assert_eq!(fake.access().unwrap_err(), SERVER_CHANGED);
        assert_eq!(fake.server_pin(), pin, "enseña la huella nueva para compararla");
        assert_eq!(fingerprint(&pin).len(), 19);
        // cada servidor tiene su propia identidad, aunque tenga la misma clave de la clínica
        assert_ne!(identity_pin(&new_identity().unwrap()), identity_pin(&new_identity().unwrap()));

        // la primera vez, un ordenador que no sabe la clave de la clínica no
        // supera la prueba: el puesto no apunta su huella ni le manda la clave
        let (impostor, ipath, ih) = start("OTRA_CLAVE");
        let mut victim = Remote::new(&impostor, "CLAVE5", 0);
        assert_eq!(connect(&mut victim, "Recepción", 1).unwrap_err(), WRONG_KEY);
        assert!(victim.pinned().is_empty());
        assert!(!victim.server_pin().is_empty(), "lo ha visto, pero no lo acepta");
        // el impostor no ha guardado nada: el puesto ni siquiera llegó a darse de alta
        assert!(!ipath.exists(), "no ha llegado a darse de alta en el impostor");
        // tampoco sirve reenviar la firma del servidor de verdad: va ligada a su huella
        let mut st = State::new(&Db::sample(crate::model::today()), std::env::temp_dir().join("ds_proof_test").join("clinica.json"), "CLAVE5", vec![7; 32]);
        let reto = "00112233445566778899aabbccddeeff";
        let q = Request { method: "GET".into(), path: "/prueba".into(), query: format!("reto={reto}"), headers: vec![], body: vec![], peer: IpAddr::V4(Ipv4Addr::LOCALHOST) };
        let (resp, _) = st.handle(&q, "CLAVE5");
        let tag = unhex(serde_json::from_str::<Value>(&resp.body).unwrap()["prueba"].as_str().unwrap()).unwrap();
        let key = proof_key("CLAVE5");
        assert!(ring::hmac::verify(&key, &proof_message(&unhex(reto).unwrap(), &[7; 32]), &tag).is_ok());
        assert!(ring::hmac::verify(&key, &proof_message(&unhex(reto).unwrap(), &[8; 32]), &tag).is_err());
        Remote::with_pin(&impostor, "OTRA_CLAVE", 0, "").stop().unwrap();
        ih.join().unwrap();
        let _ = std::fs::remove_dir_all(ipath.parent().unwrap());

        first.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn nothing_secret_goes_to_an_unproven_server() {
        // servidor falso: contesta a todo con una prueba mala y apunta lo que recibe
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let tls = server_tls(&new_identity().unwrap()).unwrap();
        let got: Arc<Mutex<Vec<(String, bool)>>> = Arc::default();
        let log = got.clone();
        std::thread::spawn(move || {
            for tcp in listener.incoming().flatten() {
                let Ok(conn) = rustls::ServerConnection::new(tls.clone()) else { continue };
                let mut s = rustls::StreamOwned::new(conn, tcp);
                let head = read_head(&mut BufReader::new(&mut s), IpAddr::V4(Ipv4Addr::LOCALHOST));
                if let Ok(Some(req)) = head {
                    log.lock().unwrap().push((req.path.clone(), req.header("X-Clave").is_some() || req.header("X-Sesion").is_some()));
                }
                let body = format!("{{\"prueba\": \"{}\"}}", "0".repeat(64));
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}", body.len());
                let _ = s.flush();
                // la conexión se queda abierta, por si el puesto la reutilizara para lo demás
                let log = log.clone();
                std::thread::spawn(move || {
                    if let Ok(Some(req)) = read_head(&mut BufReader::new(&mut s), IpAddr::V4(Ipv4Addr::LOCALHOST)) {
                        log.lock().unwrap().push((req.path.clone(), req.header("X-Clave").is_some() || req.header("X-Sesion").is_some()));
                    }
                });
            }
        });
        let mut r = Remote::new(&addr, "CLAVE6", 0);
        assert_eq!(connect(&mut r, "Recepción", 1).unwrap_err(), WRONG_KEY);
        assert!(r.access().is_err() && r.login(1, "0000").is_err());
        assert!(r.pinned().is_empty());
        let got = got.lock().unwrap().clone();
        assert!(!got.is_empty() && got.iter().all(|(path, secret)| path == "/prueba" && !secret), "{got:?}");
    }

    #[test]
    fn a_station_too_far_behind_gets_everything() {
        // servidor recién arrancado: no sabe qué cambió antes, así que da los datos enteros
        let mut db = Db::sample(crate::model::today());
        db.rev = 40;
        let st = State::new(&db, std::env::temp_dir().join("ds_since_test").join("clinica.json"), "K", vec![1; 32]);
        let v: Value = serde_json::from_str(&st.since(12, &[]).body).unwrap();
        assert_eq!((v["rev"].as_u64(), v["completo"].as_bool()), (Some(40), Some(true)));
        let v: Value = serde_json::from_str(&st.since(40, &[]).body).unwrap();
        assert_eq!(v["cambios"], json!([]), "al día: nada");
        // lo que mandó el puesto vuelve siempre, aunque no haya cambiado nada en el servidor
        let v: Value = serde_json::from_str(&st.since(40, &[("users".into(), Some("4".into()))]).body).unwrap();
        assert_eq!(v["cambios"][0]["clave"], "4");
        assert!(v["cambios"][0]["ahora"].as_str().unwrap().contains(HIDDEN), "sin la contraseña");
    }

    #[test]
    fn requests_are_limited() {
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let raw = |s: &str| std::io::Cursor::new(s.as_bytes().to_vec());
        let read = |s: &str| -> Result<Request, Response> {
            let mut r = raw(s);
            let mut req = read_head(&mut r, peer)?.unwrap();
            read_body(&mut r, &mut req, MAX_BODY)?;
            Ok(req)
        };
        let big = format!("POST /guardar HTTP/1.1\r\nContent-Length: {}\r\n\r\n", MAX_BODY + 1);
        assert_eq!(read(&big).err().unwrap().code, 413);
        let long = format!("GET /{} HTTP/1.1\r\n\r\n", "a".repeat(9000));
        assert_eq!(read(&long).err().unwrap().code, 431);
        let chunked = "POST /guardar HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n";
        assert_eq!(read(chunked).err().unwrap().code, 501);
        let short = "POST /guardar HTTP/1.1\r\nContent-Length: 10\r\n\r\n{}";
        assert_eq!(read(short).err().unwrap().code, 400);
        let ok = "POST /guardar?base=7 HTTP/1.1\r\nx-clave: K\r\nContent-Length: 2\r\n\r\n{}";
        let r = read(ok).unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str(), r.param("base"), r.header("X-Clave"), r.body.as_slice()), ("POST", "/guardar", Some("7"), Some("K"), &b"{}"[..]));
        // sin la clave de la clínica no se lee el cuerpo (se mire lo que se mire lo que anuncie)
        assert!(has_key(&r, "K"));
        assert!(!has_key(&r, "OTRA") && !has_key(&Request { headers: vec![], ..read(ok).unwrap() }, "K"));

        // claves aleatorias y, con una clave mal 20 veces, el ordenador espera
        assert_ne!(new_key(), new_key());
        assert_eq!(new_key().len(), 12);
        let mut st = State::new(&Db::sample(crate::model::today()), std::env::temp_dir().join("ds_limit_test").join("clinica.json"), "BIEN", vec![1; 32]);
        let bad = Request { method: "GET".into(), path: "/acceso".into(), query: String::new(), headers: vec![("X-Clave".into(), "MAL".into())], body: vec![], peer };
        for _ in 0..20 {
            assert_eq!(st.handle(&bad, "BIEN").0.code, 401);
        }
        let good = Request { headers: vec![("X-Clave".into(), "BIEN".into())], ..bad };
        assert_eq!(st.handle(&good, "BIEN").0.code, 429);
    }
}
