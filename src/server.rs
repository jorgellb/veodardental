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
//! * POST /cadena             → {"accion": "emitir"|"rectificar"|"subsanar"|"anular", …}
//!   facturas y registros de VERI*FACTU, numerados y encadenados por el servidor
//!   POST /parar              → detiene el servidor (solo desde el propio servidor)
//!
//! Cada puesto tiene una sesión, que sigue valiendo con la pantalla bloqueada
//! (los WhatsApp, la cita online y el correo siguen guardando) y se renueva al
//! volver a entrar; caduca tras 24 horas sin usarse.

use crate::app::ChainOp;
use crate::model::{Db, sha256_hex};
use crate::shared::StationSeen;
use crate::sync::{self, Change, Snapshot, canonical};
use crate::users::{LoginStep, Perm, ct_eq};
use crate::vista::View;
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
    crate::paths::config().join(crate::brand::DIR).join("servidor.json")
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
    std::fs::read_to_string(config_path()).ok().and_then(|t| serde_json::from_str::<ServerConfig>(&t).ok()).filter(|c| !c.key.is_empty()).unwrap_or(ServerConfig { port: DEFAULT_PORT, key: String::new(), tls_key: String::new() })
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
    crate::paths::write_private(path, text.as_bytes()).map_err(|e| e.to_string())
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
        if pin.is_empty() || ct_eq(public, &pin) { Ok(rustls::client::danger::ServerCertVerified::assertion()) } else { Err(mismatch()) }
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
    let cfg = rustls::ClientConfig::builder_with_provider(p).with_protocol_versions(&[&rustls::version::TLS13]).map_err(|e| e.to_string())?.dangerous().with_custom_certificate_verifier(Arc::new(verifier)).with_no_client_auth();
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

/// Escribe la respuesta; con `gzip` (si el puesto lo admite), las grandes van comprimidas.
fn write_response(w: &mut impl Write, resp: &Response, gzip_ok: bool) -> std::io::Result<()> {
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
    let packed = (gzip_ok && resp.body.len() > 64 * 1024).then(|| gzip(resp.body.as_bytes())).filter(|z| !z.is_empty());
    let (body, encoding) = match &packed {
        Some(z) => (z.as_slice(), "Content-Encoding: gzip\r\n"),
        None => (resp.body.as_bytes(), ""),
    };
    write!(w, "HTTP/1.1 {} {reason}\r\nContent-Type: application/json; charset=utf-8\r\n{encoding}Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", resp.code, body.len())?;
    w.write_all(body)?;
    w.flush()
}

/// Comprime con gzip (nivel rápido: el JSON de los datos ocupa unas diez veces menos).
fn gzip(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::with_capacity(data.len() / 6), flate2::Compression::fast());
    if e.write_all(data).is_err() {
        return vec![];
    }
    e.finish().unwrap_or_default()
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
        let _ = write_response(&mut &tcp, &error(426, OLD_STATION), false);
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
    let gzip_ok = matches!(&read, Ok(Some(req)) if req.header("Accept-Encoding").is_some_and(|v| v.contains("gzip")));
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
    let _ = write_response(&mut s, &resp, gzip_ok);
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

/// Número de historia más alto que se admite (con ellos se nombran carpetas).
const MAX_HISTORY_NO: u64 = 999_999_999;

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
    /// Diario de cambios (abierto para añadir) y cuánto lleva desde la última reescritura.
    journal: Option<std::fs::File>,
    journal_lines: usize,
    journal_bytes: usize,
    last_backup_day: Option<chrono::NaiveDate>,
    /// Cómo fue la última copia diaria (se enseña en los puestos a Dirección).
    backup: crate::tools::BackupStatus,
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

/// Número de puesto del servidor para los identificadores que crea: los
/// múltiplos de 16, que no usa ningún puesto (son del 1 al 15).
const SERVER_STATION: u32 = 16;

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
            journal: None,
            journal_lines: 0,
            journal_bytes: 0,
            last_backup_day: None,
            backup: crate::tools::BackupStatus::from_dir(&crate::tools::backup_dir()),
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

    /// Reescribe el fichero de datos entero y vacía el diario.
    fn rewrite(&mut self) -> Result<(), String> {
        if let Some(d) = self.path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut f = crate::paths::create_private(&tmp).map_err(|e| e.to_string())?;
            f.write_all(self.snap.to_json().as_bytes()).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
        // el fichero ya lo tiene todo: el diario empieza de nuevo
        let journal = crate::paths::create_private(&sync::journal_path(&self.path)).map_err(|e| e.to_string())?;
        self.journal = Some(journal);
        self.journal_lines = 0;
        self.journal_bytes = 0;
        Ok(())
    }

    /// Apunta la revisión en el diario (y en el disco antes de responder);
    /// cada cierto tiempo reescribe el fichero entero.
    fn persist(&mut self, rev: u64, touched: &[(String, Option<String>)]) -> Result<(), String> {
        if self.journal.is_none() {
            self.rewrite()?;
        }
        let mut line = sync::journal_line(rev, &self.snap, touched);
        line.push('\n');
        if let Some(f) = self.journal.as_mut() {
            f.write_all(line.as_bytes()).and_then(|_| f.sync_data()).map_err(|e| format!("No se pudo guardar en el diario de cambios: {e}"))?;
        }
        self.journal_lines += 1;
        self.journal_bytes += line.len();
        if self.journal_lines >= 1000 || self.journal_bytes > 64 << 20 {
            self.rewrite()?;
        }
        // copia de seguridad diaria en el servidor
        let today = crate::model::today();
        if self.last_backup_day != Some(today) && crate::tools::needs_daily(today) {
            let done = self.snap.to_db().and_then(|db| crate::tools::create_backup(&db, "automatica (servidor)"));
            if let Err(e) = &done {
                eprintln!("Copia de seguridad: {e}");
            }
            self.backup.record(&done, crate::model::now());
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
            if crate::paths::write_private(&tmp, t.as_bytes()).is_ok() {
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
        let old = self.rev();
        let rev = old + 1;
        self.snap.set_rev(rev);
        if let Err(e) = self.persist(rev, &touched) {
            self.snap.set_rev(old);
            return Err(e);
        }
        self.log.push_back((rev, touched));
        while self.log.len() > LOG_REVS {
            self.log.pop_front();
        }
        Ok(())
    }

    /// Cambia algunos campos con las funciones del programa (entradas,
    /// contraseñas, puestos…), sin convertir los datos enteros.
    fn modify<T>(&mut self, fields: &[&str], f: impl FnOnce(&mut Db) -> T) -> Result<T, String> {
        let mut db = sync::partial_db(&self.snap, fields)?;
        let out = f(&mut db);
        let mut touched = vec![];
        let mut previous = vec![];
        for ch in sync::changes_in(&self.snap, &db, fields) {
            previous.push(sync::before(&self.snap, &ch.field, ch.key.as_deref()));
            self.snap.set(&ch.field, ch.key.as_deref(), ch.after);
            touched.push((ch.field, ch.key));
        }
        if let Err(e) = self.commit(touched) {
            sync::undo(&mut self.snap, previous);
            return Err(e);
        }
        Ok(out)
    }

    fn user(&self, id: u32) -> Option<crate::users::User> {
        serde_json::from_str(self.snap.get("users", Some(&id.to_string()))?).ok()
    }

    /// ¿Puede recetar este colaborador? (ver `Db::prescriber_problem`)
    fn can_prescribe(&self, doctor: u32) -> bool {
        self.snap.get("collaborators", Some(&doctor.to_string())).and_then(|j| serde_json::from_str::<crate::model::Collaborator>(j).ok()).is_some_and(|c| crate::finance::collaborator_problem(&c).is_none())
    }

    /// Nombre del usuario de la sesión si es un profesional que puede recetar.
    fn prescriber_name(&self, id: u32) -> Option<String> {
        let u = self.user(id).filter(|u| u.active)?;
        let d = u.doctor_id?;
        self.can_prescribe(d).then_some(u.name)
    }

    fn is_admin(&self, id: u32) -> bool {
        self.user(id).is_some_and(|u| u.active && u.profile.can(Perm::Configuracion))
    }

    /// Lo que ve el usuario de la sesión (según su perfil).
    fn view_of(&self, id: u32) -> View {
        self.user(id).map(|u| View::of(u.profile)).unwrap_or(View::of(crate::users::Profile::Recepcion))
    }

    /// Un registro tal como se le manda a un puesto: sin contraseñas y sin lo
    /// que su perfil no ve.
    fn outgoing(view: View, list: &str, json: &str) -> Option<String> {
        if list == "users" {
            return Some(strip_user_json(json));
        }
        view.show(list, json)
    }

    /// Aplica los cambios de un puesto, combinándolos con los de los demás.
    /// Devuelve todo lo que mandó el puesto (también lo que no se ha aceptado,
    /// para que el puesto se quede con lo que de verdad hay en el servidor).
    fn save(&mut self, mut changes: Vec<Change>, station: u32, user: u32) -> Result<Vec<(String, Option<String>)>, String> {
        let admin = self.is_admin(user);
        // cada registro con su propia clave: así la protección de los usuarios
        // mira al usuario de verdad
        sync::check_keys(&changes)?;
        let sent: Vec<(String, Option<String>)> = changes.iter().map(|c| (c.field.clone(), c.key.clone())).collect();
        // los puestos (y la revisión) los lleva el servidor; los usuarios solo los
        // cambia Dirección, y nunca sus contraseñas ni los fallos de acceso
        changes.retain(|c| c.field != "stations" && c.field != "rev" && (c.field != "users" || admin));
        for c in changes.iter_mut().filter(|c| c.field == "users") {
            let current = c.key.as_deref().and_then(|k| self.snap.get("users", Some(k))).cloned();
            c.before = c.before.as_deref().map(|b| protect_user_json(b, current.as_ref())).transpose()?;
            c.after = c.after.as_deref().map(|a| protect_user_json(a, current.as_ref())).transpose()?;
        }
        // lo que el perfil no ve: en las listas que no recibe no puede escribir
        // nada (ni añadir: una receta o una imagen las crea quien las ve), y en
        // los demás registros lo que no ve queda como está
        let view = self.view_of(user);
        if !view.is_full() {
            changes.retain(|c| !view.hides(&c.field));
            for c in changes.iter_mut().filter(|c| c.key.is_some()) {
                let current = self.snap.get(&c.field, c.key.as_deref()).cloned();
                c.before = c.before.as_deref().map(|b| view.restore(&c.field, b, current.as_ref())).transpose()?;
                c.after = c.after.as_deref().map(|a| view.restore(&c.field, a, current.as_ref())).transpose()?;
            }
        }
        // lo firmado (documentos, cuestionarios) y las recetas emitidas no se
        // cambian ni se borran ni pasan a otro paciente: solo se anota la
        // revocación o el estado en las farmacias (fusionar pacientes es una
        // operación del servidor, `ChainOp::MergePatients`)
        for c in changes.iter_mut().filter(|c| matches!(c.field.as_str(), "documents" | "health" | "prescriptions")) {
            let Some(cur) = c.key.as_deref().and_then(|k| self.snap.get(&c.field, Some(k))).cloned() else { continue };
            if let Some(kept) = crate::firmado::protect_json(&c.field, &cur, c.after.as_deref()) {
                c.after = Some(kept);
            }
        }
        // recetas: la revisión de sus avisos solo la da quien puede recetar,
        // con su nombre; y el prescriptor tiene que poder recetar
        let reviewer = self.prescriber_name(user);
        for c in changes.iter_mut().filter(|c| c.field == "prescriptions") {
            let Some(after) = c.after.as_deref().and_then(|a| serde_json::from_str::<crate::finance::Prescription>(a).ok()) else { continue };
            let cur = c.key.as_deref().and_then(|k| self.snap.get("prescriptions", Some(k))).and_then(|j| serde_json::from_str::<crate::finance::Prescription>(j).ok());
            let mut fixed = after.clone();
            let (was_w, was_by, was_at) = cur.as_ref().map(|r| (r.reviewed_warnings.clone(), r.reviewed_by.clone(), r.reviewed_at)).unwrap_or_default();
            if (after.reviewed_warnings != was_w || after.reviewed_by != was_by || after.reviewed_at != was_at) && !after.reviewed_warnings.is_empty() {
                match &reviewer {
                    Some(name) => fixed.reviewed_by = name.clone(),
                    None => (fixed.reviewed_warnings, fixed.reviewed_by, fixed.reviewed_at) = (was_w, was_by, was_at),
                }
            }
            if let Some(cur) = &cur
                && after.doctor_id != cur.doctor_id
                && !self.can_prescribe(after.doctor_id)
            {
                fixed.doctor_id = cur.doctor_id;
            }
            if fixed != after {
                c.after = serde_json::to_string(&fixed).ok();
            }
        }
        // análisis con IA: lo que contestó la IA no cambia; la revisión solo la
        // da un profesional colegiado, con su nombre; lo revisado, usado o de
        // pago no se borra (ver `ia::protect_report`)
        for c in changes.iter_mut().filter(|c| c.field == "ia_reports") {
            let after = c.after.as_deref().and_then(|a| serde_json::from_str::<crate::ia::IaReport>(a).ok());
            let cur = c.key.as_deref().and_then(|k| self.snap.get("ia_reports", Some(k))).and_then(|j| serde_json::from_str::<crate::ia::IaReport>(j).ok());
            let fixed = match (&cur, after) {
                (Some(cur), after) => crate::ia::protect_report(cur, after.as_ref(), reviewer.as_deref()),
                (None, Some(new)) => Some(crate::ia::protect_new_report(new, reviewer.as_deref())),
                (None, None) => continue,
            };
            c.after = fixed.and_then(|r| serde_json::to_string(&r).ok());
        }
        // números de historia con sentido (con ellos se nombran carpetas)
        for c in changes.iter().filter(|c| c.field == "patients") {
            let n = c.after.as_deref().and_then(|a| serde_json::from_str::<Value>(a).ok()).and_then(|v| v["history_no"].as_u64());
            if n.is_some_and(|n| n > MAX_HISTORY_NO) {
                return Err("Número de historia no válido.".into());
            }
        }
        self.drop_auto_duplicates(&mut changes);
        // el mismo tratamiento hecho a la vez en dos puestos: el material se gasta una vez
        self.drop_double_consumption(&mut changes);
        let existed_in = |field: &str| -> std::collections::HashSet<String> { changes.iter().filter(|c| c.field == field).filter_map(|c| c.key.clone()).filter(|k| self.snap.get(field, Some(k)).is_some()).collect() };
        let (existed, paid_before, moved_before) = (existed_in("patients"), existed_in("payments"), existed_in("stock_moves"));
        // existencias de los productos antes de guardar (`None`: producto nuevo de este puesto)
        let stock_before: HashMap<String, Option<f64>> = changes
            .iter()
            .filter(|c| c.field == "products")
            .filter_map(|c| c.key.clone())
            .map(|k| {
                let s = self.snap.get("products", Some(&k)).map(|j| serde_json::from_str::<Value>(j).ok().and_then(|v| v["stock"].as_f64()).unwrap_or(0.0));
                (k, s)
            })
            .collect();
        let applied = sync::apply(&mut self.snap, &changes)?;
        let (mut touched, mut previous) = (applied.touched, applied.previous);
        // si algo falla a partir de aquí, se deshace todo
        let rest = (|| -> Result<(), String> {
            // dos puestos que dan de alta pacientes a la vez sacan el mismo número de
            // historia (con él se nombra su carpeta de imágenes): el nuevo, el siguiente libre
            let new_patients: Vec<String> = touched.iter().filter(|(f, k)| f == "patients" && k.as_ref().is_some_and(|k| !existed.contains(k))).filter_map(|(_, k)| k.clone()).collect();
            if !new_patients.is_empty() {
                for (key, old) in self.fix_history_numbers(&new_patients)? {
                    previous.push(("patients".into(), Some(key.clone()), Some(old), None));
                    if !touched.iter().any(|(f, k)| f == "patients" && k.as_deref() == Some(&key)) {
                        touched.push(("patients".into(), Some(key)));
                    }
                }
            }
            // cobros nuevos en una caja que otro puesto acaba de cerrar (el arqueo
            // ya está hecho sin ellos): a la caja abierta del centro, o a ninguna
            let new_payments: Vec<String> = touched.iter().filter(|(f, k)| f == "payments" && k.as_ref().is_some_and(|k| !paid_before.contains(k))).filter_map(|(_, k)| k.clone()).collect();
            if !new_payments.is_empty() {
                for (key, old) in self.move_late_payments(&new_payments)? {
                    previous.push(("payments".into(), Some(key), Some(old), None));
                }
            }
            // las existencias solo cambian con los movimientos del almacén nuevos:
            // así cuentan los de todos los puestos, y reenviar uno no lo cuenta dos veces
            let new_moves: Vec<String> = touched.iter().filter(|(f, k)| f == "stock_moves" && k.as_ref().is_some_and(|k| !moved_before.contains(k))).filter_map(|(_, k)| k.clone()).collect();
            for (key, old) in self.fix_stock(&stock_before, &new_moves)? {
                previous.push(("products".into(), Some(key.clone()), Some(old), None));
                if !touched.iter().any(|(f, k)| f == "products" && k.as_deref() == Some(&key)) {
                    touched.push(("products".into(), Some(key)));
                }
            }
            if station > 0 {
                let key = station.to_string();
                if let Some(old) = self.snap.get("stations", Some(&key)).cloned()
                    && let Ok(mut s) = serde_json::from_str::<StationSeen>(&old)
                {
                    s.last_seen = crate::model::now();
                    self.snap.set("stations", Some(&key), serde_json::to_string(&s).ok());
                    previous.push(("stations".into(), Some(key.clone()), Some(old), None));
                    touched.push(("stations".into(), Some(key)));
                }
            }
            self.commit(touched)
        })();
        if let Err(e) = rest {
            sync::undo(&mut self.snap, previous);
            return Err(e);
        }
        Ok(sent)
    }

    /// Recordatorios y tareas automáticas nuevos que ya existen (otro puesto
    /// los generó a la vez): se descartan, y el puesto, al recibir la
    /// respuesta, se queda sin ellos.
    fn drop_auto_duplicates(&self, changes: &mut Vec<Change>) {
        let is_new = |c: &Change| c.before.is_none() && c.after.is_some() && self.snap.get(&c.field, c.key.as_deref()).is_none();
        let msg_key = |json: &str| serde_json::from_str::<crate::comms::Message>(json).ok().as_ref().and_then(crate::comms::auto_message_key);
        let task_key = |json: &str| serde_json::from_str::<crate::tasks::Task>(json).ok().map(|t| t.auto).filter(|a| !a.is_empty());
        let key_of = |c: &Change| match c.field.as_str() {
            "messages" => c.after.as_deref().and_then(msg_key),
            "tasks" => c.after.as_deref().and_then(task_key),
            _ => None,
        };
        if !changes.iter().any(|c| is_new(c) && key_of(c).is_some()) {
            return;
        }
        let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
        for (field, f) in [("messages", &msg_key as &dyn Fn(&str) -> Option<String>), ("tasks", &task_key)] {
            for (_, json) in self.snap.records.get(field).into_iter().flat_map(|r| r.iter()) {
                if let Some(k) = f(json) {
                    seen.insert((field.to_string(), k));
                }
            }
        }
        changes.retain(|c| !is_new(c) || key_of(c).is_none_or(|k| seen.insert((c.field.clone(), k))));
    }

    /// Pacientes nuevos (`keys`) cuyo número de historia ya tiene otro: les da
    /// el siguiente libre. Devuelve los que ha cambiado, con cómo estaban.
    fn fix_history_numbers(&mut self, keys: &[String]) -> Result<Vec<(String, String)>, String> {
        let no = |json: &str| serde_json::from_str::<Value>(json).ok().and_then(|v| v["history_no"].as_u64());
        let Some(list) = self.snap.records.get("patients") else { return Ok(vec![]) };
        let mut used: HashMap<u64, usize> = HashMap::new();
        for (_, p) in list.iter() {
            if let Some(n) = no(p) {
                *used.entry(n).or_default() += 1;
            }
        }
        let mut next = used.keys().max().copied().unwrap_or(1000);
        let mut fixed = vec![];
        for key in keys {
            let Some(json) = self.snap.get("patients", Some(key)).cloned() else { continue };
            let Some(n) = no(&json) else { continue };
            if used.get(&n).copied().unwrap_or(0) <= 1 {
                continue;
            }
            next = next.checked_add(1).filter(|n| *n <= MAX_HISTORY_NO).ok_or("No quedan números de historia.")?;
            let mut v: Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            v["history_no"] = json!(next);
            self.snap.set("patients", Some(key), Some(canonical("patients", &v.to_string())?));
            *used.entry(n).or_default() -= 1;
            used.insert(next, 1);
            fixed.push((key.clone(), json));
        }
        Ok(fixed)
    }

    /// Cobros nuevos (`keys`) apuntados a una caja ya cerrada: pasan a la
    /// abierta de su centro (o a ninguna). Devuelve los que ha cambiado, con
    /// cómo estaban. Ver [`Db::move_late_payments`].
    fn move_late_payments(&mut self, keys: &[String]) -> Result<Vec<(String, String)>, String> {
        let Some(cash) = self.snap.records.get("cash") else { return Ok(vec![]) };
        let (mut closed, mut open) = (std::collections::HashSet::new(), vec![]);
        for (_, c) in cash.iter() {
            let Ok(v) = serde_json::from_str::<Value>(c) else { continue };
            let (Some(id), center) = (v["id"].as_u64(), crate::model::cid(v["center_id"].as_u64().unwrap_or(0) as u32)) else { continue };
            match v["state"].as_str() {
                Some("Cerrada") => {
                    closed.insert(id);
                }
                Some("Abierta") => open.push((center, id)),
                _ => {}
            }
        }
        let mut fixed = vec![];
        for key in keys {
            let Some(json) = self.snap.get("payments", Some(key)).cloned() else { continue };
            let mut v: Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            if !v["cash_session"].as_u64().is_some_and(|s| closed.contains(&s)) {
                continue;
            }
            let center = crate::model::cid(v["center_id"].as_u64().unwrap_or(0) as u32);
            v["cash_session"] = open.iter().find(|(c, _)| *c == center).map(|x| json!(x.1)).unwrap_or(Value::Null);
            self.snap.set("payments", Some(key), Some(canonical("payments", &v.to_string())?));
            fixed.push((key.clone(), json));
        }
        Ok(fixed)
    }

    /// Deja las existencias de los productos que ya había (`before`, cómo
    /// estaban antes de guardar) en lo que tenían más lo de los movimientos
    /// nuevos (`moves`). Los productos nuevos se quedan como llegan. Devuelve
    /// los que ha cambiado, con cómo estaban.
    /// Consumos nuevos de material de un tratamiento que el servidor ya tiene
    /// gastado (otro puesto lo marcó como hecho a la vez): se descartan. Cuentan
    /// las devoluciones que vienen en la misma tanda (deshacer y volver a hacer).
    fn drop_double_consumption(&self, changes: &mut Vec<Change>) {
        let mv = |json: &str| -> Option<(u64, u64, f64)> {
            let v: Value = serde_json::from_str(json).ok()?;
            Some((v["entry_id"].as_u64()?, v["product_id"].as_u64()?, v["qty"].as_f64()?))
        };
        let new_moves: Vec<(usize, (u64, u64, f64))> =
            changes.iter().enumerate().filter(|(_, c)| c.field == "stock_moves" && c.key.as_deref().is_some_and(|k| self.snap.get("stock_moves", Some(k)).is_none())).filter_map(|(i, c)| Some((i, mv(c.after.as_deref()?)?))).collect();
        if !new_moves.iter().any(|(_, m)| m.2 < 0.0) {
            return;
        }
        let wanted: std::collections::HashSet<u64> = new_moves.iter().map(|(_, m)| m.0).collect();
        let mut net: HashMap<(u64, u64), f64> = HashMap::new();
        if let Some(recs) = self.snap.records.get("stock_moves") {
            for (_, json) in recs.iter().filter(|(_, j)| j.contains("entry_id")) {
                if let Some((e, p, q)) = mv(json).filter(|m| wanted.contains(&m.0)) {
                    *net.entry((e, p)).or_default() += q;
                }
            }
        }
        for (_, (e, p, q)) in new_moves.iter().filter(|(_, m)| m.2 > 0.0) {
            *net.entry((*e, *p)).or_default() += q;
        }
        let drop: std::collections::HashSet<usize> = new_moves.iter().filter(|(_, (e, p, q))| *q < 0.0 && net.get(&(*e, *p)).is_some_and(|n| *n < -1e-9)).map(|(i, _)| *i).collect();
        if !drop.is_empty() {
            let mut i = 0;
            changes.retain(|_| {
                i += 1;
                !drop.contains(&(i - 1))
            });
        }
    }

    fn fix_stock(&mut self, before: &HashMap<String, Option<f64>>, moves: &[String]) -> Result<Vec<(String, String)>, String> {
        let stock_of = |json: &str| serde_json::from_str::<Value>(json).ok().and_then(|v| v["stock"].as_f64()).unwrap_or(0.0);
        let mut target: HashMap<String, f64> = before.iter().filter_map(|(k, s)| s.map(|s| (k.clone(), s))).collect();
        for key in moves {
            let Some(m) = self.snap.get("stock_moves", Some(key)).and_then(|j| serde_json::from_str::<Value>(j).ok()) else { continue };
            let (Some(pid), Some(qty)) = (m["product_id"].as_u64(), m["qty"].as_f64()) else { continue };
            let pk = pid.to_string();
            let start = match before.get(&pk) {
                Some(Some(s)) => *s,
                // el producto lo ha dado de alta este puesto con sus movimientos ya contados
                Some(None) => continue,
                None => match self.snap.get("products", Some(&pk)) {
                    Some(j) => stock_of(j),
                    None => continue,
                },
            };
            *target.entry(pk).or_insert(start) += qty;
        }
        let mut fixed = vec![];
        for (key, want) in target {
            let Some(json) = self.snap.get("products", Some(&key)).cloned() else { continue };
            let want = crate::model::round2(want);
            if (stock_of(&json) - want).abs() < 1e-9 {
                continue;
            }
            let mut v: Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            v["stock"] = json!(want);
            self.snap.set("products", Some(&key), Some(canonical("products", &v.to_string())?));
            fixed.push((key, json));
        }
        Ok(fixed)
    }

    /// Lo que ha cambiado desde la revisión `since`, más lo de `also` (o, si
    /// `since` es de hace mucho, que el puesto pida los datos enteros).
    fn since(&self, since: u64, also: &[(String, Option<String>)], view: View) -> Response {
        let rev = self.rev();
        if since < rev && self.log.front().is_none_or(|(first, _)| *first > since + 1) {
            return reply(200, json!({ "rev": rev, "completo": true, "copia": self.backup }));
        }
        let mut seen = std::collections::HashSet::new();
        let mut out = vec![];
        let logged = self.log.iter().filter(|(r, _)| *r > since).flat_map(|(_, t)| t.iter());
        for (field, key) in logged.chain(also.iter()) {
            if !seen.insert((field.clone(), key.clone())) {
                continue;
            }
            if view.hides(field) {
                continue;
            }
            let after = match (self.snap.get(field, key.as_deref()), key) {
                (Some(rec), Some(_)) => Self::outgoing(view, field, rec),
                (other, _) => other.cloned(),
            };
            out.push(Change { field: field.clone(), key: key.clone(), before: None, after });
        }
        reply(200, json!({ "rev": rev, "cambios": out, "copia": self.backup }))
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
        // su zona horaria: lo que hace el servidor (cobros de cuotas, remesas,
        // facturas) lleva su fecha local, y los informes agrupan por esa fecha
        let offset = chrono::Local::now().offset().local_minus_utc();
        reply(200, json!({ "clinica": name, "usuarios": users, "rev": self.rev(), "desfase": offset }))
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

    /// Una operación de la cadena de facturas (VERI*FACTU: emitir, rectificar,
    /// subsanar o anular) o de las que cobran o liquidan algo (remesas,
    /// liquidaciones a mutuas, conciliación del banco): ver [`ChainOp`]. Las
    /// hace el servidor, una tras otra, para que los números sean correlativos
    /// sin repetirse, cada registro enlace con el anterior y nada se cobre o se
    /// liquide dos veces aunque varios puestos lo hagan a la vez.
    fn chain_op(&mut self, req: &Request, me: u32) -> Response {
        let Ok(v) = serde_json::from_slice::<Value>(&req.body) else { return error(400, "Petición no válida.") };
        let op = match ChainOp::from_json(&v) {
            Ok(op) => op,
            Err(e) => return error(400, &e),
        };
        let (perm, fields) = op.needs();
        let Some(u) = self.user(me).filter(|u| perm.is_none_or(|p| u.profile.can(p))) else {
            return error(
                403,
                match perm {
                    Some(Perm::Facturacion) => "Este usuario no puede facturar.",
                    Some(Perm::Economia) => "Este usuario no puede llevar los cobros.",
                    Some(Perm::Gastos) => "Este usuario no puede llevar el banco.",
                    Some(Perm::Caja) => "Este usuario no puede llevar la caja (anular cobros o devolver dinero).",
                    Some(Perm::Configuracion) => "Solo Dirección puede hacerlo (por ejemplo, fusionar pacientes).",
                    _ => "Este usuario no tiene permiso para hacerlo.",
                },
            );
        };
        let center = v["centro"].as_u64().map(|c| c as u32);
        let r = self.modify(fields, |db| {
            db.station = SERVER_STATION;
            // lo que se crea lleva el usuario de la sesión y el centro del puesto,
            // que no se quedan en la configuración del servidor
            let (user, center_was) = (std::mem::replace(&mut db.config.current_user, u.name.clone()), db.config.current_center);
            if let Some(c) = center {
                db.config.current_center = c;
            }
            let out = op.run(db, &u.name, u.profile == crate::users::Profile::Direccion);
            db.config.current_user = user;
            db.config.current_center = center_was;
            out
        });
        match r {
            Err(e) => error(500, &e),
            Ok(Err(msg)) => error(403, &msg),
            Ok(Ok(id)) => reply(200, json!({ "id": id, "rev": self.rev() })),
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
                    ("GET", "/datos") => {
                        let view = self.view_of(me);
                        Response { code: 200, body: self.snap.to_json_with(&|list, rec| Self::outgoing(view, list, rec)) }
                    }
                    ("GET", "/cambios") => self.since(param("desde"), &[], self.view_of(me)),
                    ("POST", "/cambios") => match serde_json::from_slice::<PushBody>(&req.body).map_err(|e| e.to_string()).and_then(|b| self.save(b.changes, station, me)) {
                        Ok(sent) => self.since(param("base"), &sent, self.view_of(me)),
                        Err(e) => error(400, &e),
                    },
                    ("POST", "/cadena") => self.chain_op(req, me),
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
    // el fichero de datos queda completo y el diario, vacío
    if let Err(e) = st.rewrite() {
        eprintln!("No se pueden guardar los datos: {e}");
        return;
    }
    for job in jobs {
        let (resp, stop_now) = st.handle(&job.req, &key);
        if stop_now {
            // al parar, el fichero de datos queda completo
            let resp = match st.rewrite() {
                Ok(()) => resp,
                Err(e) => error(500, &format!("No se pudieron guardar los datos al parar: {e}")),
            };
            let _ = job.reply.send(resp);
            break;
        }
        let _ = job.reply.send(resp);
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
    // solo el servidor guarda en estos datos mientras funciona
    let _lock = crate::local::lock(&Db::path()).map_err(|e| if e == crate::local::IN_USE { "Los datos de este ordenador están abiertos en VeodarDental: ciérrelo antes de arrancar el servidor de datos.".to_string() } else { e })?;
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
    /// Cómo van las copias de seguridad del servidor (lo último que ha dicho).
    pub backup: Arc<Mutex<Option<crate::tools::BackupStatus>>>,
}

/// Lo que el servidor da antes de entrar: la clínica y sus usuarios.
#[derive(Clone, Debug, Deserialize)]
pub struct AccessInfo {
    #[serde(rename = "clinica", default)]
    pub clinic: String,
    #[serde(rename = "usuarios", default)]
    pub users: Vec<crate::users::User>,
    /// Diferencia con UTC del servidor, en segundos (no la dan las versiones anteriores).
    #[serde(rename = "desfase", default)]
    pub utc_offset: Option<i32>,
}

/// Aviso si el servidor tiene otra zona horaria que el puesto: lo que hace
/// el servidor llevaría otra fecha, y un cobro de las 0:30 podría contar en
/// el día anterior en los informes.
pub fn clock_warning(server: Option<i32>, here: i32) -> Option<String> {
    let s = server?;
    if s == here {
        return None;
    }
    let utc = |o: i32| format!("UTC{}{:02}:{:02}", if o < 0 { '-' } else { '+' }, o.abs() / 3600, o.abs() % 3600 / 60);
    Some(format!(
        "El servidor de la clínica está en otra zona horaria ({}) que este puesto ({}): los cobros y facturas que haga el servidor llevarán su fecha y los informes podrían contar un cobro en otro día. Ponga en el servidor la zona horaria de la clínica.",
        utc(s),
        utc(here)
    ))
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
    /// Cómo van las copias de seguridad del servidor.
    #[serde(rename = "copia", default)]
    pub backup: Option<crate::tools::BackupStatus>,
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

/// Lo más que se lee de una respuesta del servidor (ya descomprimida): los
/// datos enteros de una clínica grande caben de sobra; más, sería un servidor
/// que no es el de la clínica (o una respuesta comprimida que no acaba nunca).
const MAX_ANSWER: u64 = 1 << 30;
/// La respuesta de la prueba de identidad son unos pocos bytes.
const MAX_PROOF: u64 = 64 * 1024;

fn read_limited(r: ureq::Response, max: u64) -> Result<String, String> {
    let mut s = String::new();
    r.into_reader().take(max + 1).read_to_string(&mut s).map_err(|e| e.to_string())?;
    if s.len() as u64 > max {
        return Err("La respuesta del servidor es demasiado grande.".into());
    }
    Ok(s)
}

fn read_all(r: ureq::Response) -> Result<String, String> {
    read_limited(r, MAX_ANSWER)
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
        Remote { base: format!("https://{addr}"), key, station, session: String::new(), pin, seen, verifying: Arc::new(Mutex::new(())), agent: b.build(), backup: Arc::new(Mutex::new(None)) }
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
        let probe = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).timeout_read(Duration::from_secs(READ_TIMEOUT)).max_idle_connections(0).tls_config(tls).build();
        // sin la clave de la clínica ni la sesión
        // sin comprimir y con poco límite: aún no se sabe si es el servidor bueno
        let answer = probe.get(&format!("{}/prueba?reto={}", self.base, hex(&challenge))).set("Accept-Encoding", "identity").call().map_err(err_text).and_then(|r| read_limited(r, MAX_PROOF));
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
        let d: Delta = serde_json::from_str(&self.get(&format!("/cambios?desde={since}"))?).map_err(|e| format!("Respuesta no válida del servidor: {e}"))?;
        self.note_backup(&d);
        Ok(d)
    }

    /// Apunta lo que dice el servidor de sus copias de seguridad.
    fn note_backup(&self, d: &Delta) {
        if let Some(b) = &d.backup
            && let Ok(mut x) = self.backup.lock()
        {
            *x = Some(b.clone());
        }
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
        self.note_backup(&delta);
        // lo enviado ya está en el servidor (y lo que haya cambiado al combinar, llega en la respuesta)
        for ch in &changes {
            synced.set(&ch.field, ch.key.as_deref(), ch.after.clone());
        }
        sync::mark_synced(db);
        self.receive(db, synced, delta)?;
        Ok(changes.len())
    }

    /// Una operación de la cadena de facturas (o una remesa, una liquidación…)
    /// en el servidor (ver [`ChainOp`]):
    /// antes se envía lo pendiente (los tratamientos recién hechos, los datos
    /// del cliente…) y después se recoge el resultado. Devuelve el id creado.
    pub fn chain_op(&self, db: &mut Db, synced: &mut Snapshot, op: &Value) -> Result<u32, String> {
        self.push(db, synced)?;
        let answer = self.post("/cadena", &op.to_string());
        // también si no se ha podido (p. ej., otro puesto acaba de hacerlo): así
        // el puesto ve enseguida lo que hay
        match self.pull(db.rev) {
            Ok(d) => self.receive(db, synced, d)?,
            Err(e) if answer.is_ok() => return Err(e),
            Err(_) => {}
        }
        let v = parse(&answer?)?;
        v["id"].as_u64().map(|n| n as u32).ok_or_else(|| "Respuesta no válida del servidor.".to_string())
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
        let disk: Db = crate::sync::on_disk(&path);
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
        let disk = || -> Db { crate::sync::on_disk(&path) };
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
        // y no puede colar un usuario con la clave de otro para saltarse la protección
        let mut doc = disk().user(4).unwrap().clone();
        doc.pin_hash = crate::users::hash_pin("sal", "111111");
        doc.salt = "sal".into();
        let fake = Change { field: "users".into(), key: Some("77".into()), before: None, after: Some(serde_json::to_string(&doc).unwrap()) };
        let body = json!({ "cambios": [fake] }).to_string();
        assert!(dir.post(&format!("/cambios?base={}", d.rev), &body).unwrap_err().contains("otra clave"));
        assert!(crate::users::pin_matches(disk().user(4).unwrap(), "582941") || crate::users::pin_matches(disk().user(4).unwrap(), "0000"));

        // las contraseñas se cambian con su propia orden
        assert!(rec.set_pin(4, None, "582941", true).unwrap_err().contains("Dirección"));
        dir.set_pin(4, None, "582941", true).unwrap();
        let disk: Db = crate::sync::on_disk(&path);
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
    fn reception_does_not_receive_clinical_data() {
        let (addr, path, h) = start("CLAVE7");
        let disk = || -> Db { crate::sync::on_disk(&path) };
        // un doctor apunta antecedentes, notas y una nota clínica de un tratamiento
        let doc = signed_in(&addr, "CLAVE7", "Gabinete", 3, "0000");
        let (mut dd, mut sdd) = doc.load().unwrap();
        dd.station = doc.station;
        let pid = dd.patients[0].id;
        dd.patients[0].background = "Hipertensión, anticoagulado".into();
        dd.patients[0].notes = "Ansiedad en el sillón".into();
        dd.entries[0].notes = "Sangrado al sondar".into();
        doc.push(&mut dd, &mut sdd).unwrap();
        let rec = signed_in(&addr, "CLAVE7", "Recepción", 2, "0000");
        let full = disk();
        assert!(!full.health.is_empty() && !full.perio.is_empty() && !full.images.is_empty() && !full.prescriptions.is_empty() && !full.ortho.is_empty());
        assert_eq!(full.patient(pid).unwrap().background, "Hipertensión, anticoagulado");
        let (mut d, mut sd) = rec.load().unwrap();
        d.station = rec.station;
        assert!(d.perio.is_empty() && d.images.is_empty() && d.ia_reports.is_empty() && d.prescriptions.is_empty() && d.ortho.is_empty(), "nada clínico");
        // de los cuestionarios, solo si los hay, su fecha y si están firmados (para pedirlos)
        assert_eq!(d.health.len(), full.health.len());
        assert!(d.health.iter().all(|f| f.yes.is_empty() && f.details.is_empty() && f.notes.is_empty() && f.reason.is_empty()));
        assert!(d.health.iter().zip(&full.health).all(|(a, b)| a.date == b.date && a.signed == b.signed));
        assert!(d.patients.iter().all(|p| p.background.is_empty() && p.notes.is_empty()), "ni antecedentes ni notas");
        assert!(d.entries.iter().all(|e| e.notes.is_empty()));
        assert_eq!(d.entries.len(), full.entries.len(), "los tratamientos sí (para cobrar)");
        assert_eq!(d.documents.len(), full.documents.len(), "los documentos sí (para firmar)");
        assert!(d.patients.iter().zip(&full.patients).all(|(a, b)| a.alerts == b.alerts && a.allergies == b.allergies), "las alertas sí");
        // cambia el teléfono de un paciente: sus antecedentes y notas siguen en el servidor
        let p = d.patients.iter_mut().find(|p| p.id == pid).unwrap();
        p.mobile = "600111222".into();
        rec.push(&mut d, &mut sd).unwrap();
        let after = disk();
        let (before_p, after_p) = (full.patient(pid).unwrap(), after.patient(pid).unwrap());
        assert_eq!(after_p.mobile, "600111222");
        assert_eq!((after_p.background.as_str(), after_p.notes.as_str()), (before_p.background.as_str(), before_p.notes.as_str()));
        // añade un cuestionario (el que manda un paciente por internet): se guarda
        // entero, pero le vuelve sin las respuestas
        let mut form = full.health[0].clone();
        form.id = d.new_id();
        form.notes = "Rellenado por internet".into();
        form.yes.insert("diabetes".into(), true);
        d.health.push(form.clone());
        rec.push(&mut d, &mut sd).unwrap();
        let saved = disk().health.into_iter().find(|f| f.id == form.id).unwrap();
        assert_eq!((saved.notes.as_str(), saved.yes.get("diabetes")), ("Rellenado por internet", Some(&true)));
        let (fresh, _) = rec.load().unwrap();
        let back = fresh.health.iter().find(|f| f.id == form.id).unwrap();
        assert!(back.notes.is_empty() && back.yes.is_empty());
        // y si lo marca como firmado, las respuestas siguen en el servidor
        let mut d = fresh;
        let mut sd = Snapshot::from_db(&d);
        crate::sync::mark_synced(&mut d);
        d.station = rec.station;
        d.health.iter_mut().find(|f| f.id == form.id).unwrap().signed = true;
        rec.push(&mut d, &mut sd).unwrap();
        let saved = disk().health.into_iter().find(|f| f.id == form.id).unwrap();
        assert!(saved.signed && saved.notes == "Rellenado por internet" && saved.yes.get("diabetes") == Some(&true));
        // no puede añadir nada a lo que no ve (una receta falsa, una imagen con otra ruta…)
        let mut rx = full.prescriptions[0].clone();
        rx.id = 9_999_001;
        let mut img = full.images[0].clone();
        img.id = 9_999_002;
        img.path = "/etc/passwd".into();
        let new_rx = Change { field: "prescriptions".into(), key: Some(rx.id.to_string()), before: None, after: Some(serde_json::to_string(&rx).unwrap()) };
        let new_img = Change { field: "images".into(), key: Some(img.id.to_string()), before: None, after: Some(serde_json::to_string(&img).unwrap()) };
        let body = json!({ "cambios": [new_rx, new_img] }).to_string();
        rec.post(&format!("/cambios?base={}", d.rev), &body).unwrap();
        assert!(disk().prescriptions.iter().all(|r| r.id != rx.id), "Recepción no puede crear recetas");
        assert!(disk().images.iter().all(|i| i.id != img.id));
        // no puede borrar ni cambiar lo que no ve
        let perio = serde_json::to_string(&full.perio[0]).unwrap();
        let key = full.perio[0].id.to_string();
        let body = json!({ "cambios": [Change { field: "perio".into(), key: Some(key.clone()), before: Some(perio), after: None }] }).to_string();
        rec.post(&format!("/cambios?base={}", d.rev), &body).unwrap();
        assert!(disk().perio.iter().any(|c| c.id.to_string() == key));
        assert_eq!(disk().entries[0].notes, "Sangrado al sondar");
        // un doctor lo recibe todo
        let (all, _) = doc.load().unwrap();
        assert_eq!((all.perio.len(), all.health.len()), (full.perio.len(), full.health.len() + 1));
        assert_eq!(all.patient(pid).unwrap().background, "Hipertensión, anticoagulado");
        assert_eq!(all.entries[0].notes, "Sangrado al sondar");
        doc.stop().unwrap();
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
        let disk: Db = crate::sync::on_disk(&path);
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

    /// T-F3-01. Ensayo de instalación en Linux con las raíces aisladas de las
    /// pruebas (`paths`): el servidor crea su clave y su identidad una vez,
    /// en un fichero solo para su usuario; un puesto se da de alta y apunta la
    /// huella que enseña el servidor; los datos, el diario, las sesiones y
    /// las copias quedan solo para el usuario; cada usuario hace solo lo de
    /// su permiso; y el cifrado del disco se comprueba de verdad.
    #[cfg(unix)]
    #[test]
    fn installation_rehearsal_on_linux() {
        use crate::app::ChainOp;
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert!(config_path().starts_with(std::env::temp_dir()) || std::env::var_os(crate::paths::ROOT_VAR).is_some(), "raíz aislada: {}", config_path().display());
        // 1. instalar el servidor: clave e identidad, una sola vez
        let cfg = ensure_server_config().unwrap();
        assert_eq!(mode(&config_path()), 0o600, "la clave de la clínica, solo para el usuario");
        let again = ensure_server_config().unwrap();
        assert_eq!((again.key.clone(), again.pin()), (cfg.key.clone(), cfg.pin()), "repetirlo no cambia la identidad");
        let identity = cfg.identity().unwrap();
        let dir = std::env::temp_dir().join(format!("ds_ensayo_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clinica.json");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let (p2, k2, id2) = (path.clone(), cfg.key.clone(), identity.clone());
        let h = std::thread::spawn(move || serve(listener, p2, k2, id2, Db::sample(NaiveDate::from_ymd_opt(2026, 11, 6).unwrap())));
        // 2. alta de un puesto: apunta la huella, la misma que enseña el servidor
        let mut station = Remote::new(&addr, &cfg.key, 0);
        connect(&mut station, "Recepción", 1).unwrap();
        assert_eq!(fingerprint(&station.pinned()), fingerprint(&cfg.pin()));
        assert_eq!(Remote::new(&addr, "CLAVE_MALA", 0).access().unwrap_err(), WRONG_KEY);
        // 3. trabajar: un cambio y una operación con su permiso
        let rec = signed_in(&addr, &cfg.key, "Recepción", 2, "0000");
        let (mut a, mut sa) = rec.load().unwrap();
        let pid = a.patients[0].id;
        a.patients.iter_mut().find(|p| p.id == pid).unwrap().notes.push_str(" (ensayo)");
        rec.push(&mut a, &mut sa).unwrap();
        assert!(rec.chain_op(&mut a, &mut sa, &ChainOp::Reconcile.json()).unwrap_err().contains("banco"), "Recepción no lleva el banco");
        let dirc = signed_in(&addr, &cfg.key, "Dirección", 1, "0000");
        let (mut b, mut sb) = dirc.load().unwrap();
        assert!(dirc.chain_op(&mut b, &mut sb, &ChainOp::Reconcile.json()).is_ok(), "Dirección sí");
        // 4. lo que queda en el disco, solo para el usuario
        for f in [path.clone(), crate::sync::journal_path(&path), path.with_file_name("sesiones.json")] {
            if f.exists() {
                assert_eq!(mode(&f), 0o600, "{}", f.display());
            }
        }
        assert_eq!(mode(&path), 0o600);
        let backup = crate::tools::create_backup_in(&crate::sync::on_disk(&path), &dir.join("copias"), "ensayo").unwrap();
        assert_eq!(mode(&backup), 0o600, "la copia también");
        // un fichero de datos de una versión anterior (legible por todos) se corrige al guardarlo
        let old = dir.join("antiguo.json");
        std::fs::write(&old, "{}").unwrap();
        std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o644)).unwrap();
        Db::write_full(&Db::sample(crate::model::today()), &old).unwrap();
        assert_eq!(mode(&old), 0o600);
        // 5. cifrado del disco: se comprueba de verdad (el resultado depende del ordenador)
        match crate::disco::check_now() {
            crate::disco::Disk::Checking => panic!("la comprobación tiene que terminar"),
            other => eprintln!("cifrado del disco en este ordenador: {other:?}"),
        }
        rec.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
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
        // el puesto ni siquiera llegó a darse de alta en el impostor
        assert!(crate::sync::on_disk(&ipath).stations.is_empty(), "no ha llegado a darse de alta en el impostor");
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
        let v: Value = serde_json::from_str(&st.since(12, &[], View::FULL).body).unwrap();
        assert_eq!((v["rev"].as_u64(), v["completo"].as_bool()), (Some(40), Some(true)));
        let v: Value = serde_json::from_str(&st.since(40, &[], View::FULL).body).unwrap();
        assert_eq!(v["cambios"], json!([]), "al día: nada");
        // lo que mandó el puesto vuelve siempre, aunque no haya cambiado nada en el servidor
        let v: Value = serde_json::from_str(&st.since(40, &[("users".into(), Some("4".into()))], View::FULL).body).unwrap();
        assert_eq!(v["cambios"][0]["clave"], "4");
        assert!(v["cambios"][0]["ahora"].as_str().unwrap().contains(HIDDEN), "sin la contraseña");
    }

    /// Lo que cuesta entrar en un puesto con una clínica grande:
    /// `cargo test big_clinic_login -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn big_clinic_login_costs() {
        let db = crate::shared::big_clinic();
        let st = State::new(&db, std::env::temp_dir().join("ds_big_login").join("clinica.json"), "K", vec![1; 32]);
        drop(db);
        for (who, view) in [("doctor", View::FULL), ("recepción", View::of(crate::users::Profile::Recepcion))] {
            let t = Instant::now();
            let body = st.snap.to_json_with(&|list, rec| State::outgoing(view, list, rec));
            println!("{who} · servidor prepara los datos: {:?} · {} MB", t.elapsed(), body.len() / 1_000_000);
            let t = Instant::now();
            let z = gzip(body.as_bytes());
            println!("{who} · comprimir: {:?} · {} MB", t.elapsed(), z.len() / 1_000_000);
            let t = Instant::now();
            let mut d: Db = serde_json::from_str(&body).unwrap();
            let synced = Snapshot::from_db(&d);
            sync::mark_synced(&mut d);
            println!("{who} · puesto los lee y prepara: {:?}", t.elapsed());
            drop(synced);
        }
    }

    #[test]
    fn answers_are_read_with_a_limit() {
        // una «bomba» comprimida: pocos KB que se descomprimen en mucho más que el límite
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut tcp, _)) = listener.accept() {
                let _ = read_head(&mut BufReader::new(&tcp), IpAddr::V4(Ipv4Addr::LOCALHOST));
                let z = gzip(&vec![b' '; (MAX_PROOF * 4) as usize]);
                let _ = write!(tcp, "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", z.len());
                let _ = tcp.write_all(&z);
            }
        });
        let r = ureq::get(&format!("http://127.0.0.1:{port}/")).call().unwrap();
        assert_eq!(read_limited(r, MAX_PROOF).unwrap_err(), "La respuesta del servidor es demasiado grande.");
    }

    #[test]
    fn big_answers_travel_compressed() {
        let r = Response { code: 200, body: format!("{{\"x\": \"{}\"}}", "datos ".repeat(50_000)) };
        let mut out = vec![];
        write_response(&mut out, &r, true).unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("Content-Encoding: gzip"));
        assert!(out.len() < r.body.len() / 10, "{} de {}", out.len(), r.body.len());
        let mut plain = vec![];
        write_response(&mut plain, &r, false).unwrap();
        assert!(!String::from_utf8_lossy(&plain).contains("gzip"), "si el puesto no lo admite, sin comprimir");
        let mut small = vec![];
        write_response(&mut small, &error(400, "no"), true).unwrap();
        assert!(!String::from_utf8_lossy(&small).contains("gzip"), "lo pequeño, sin comprimir");
    }

    #[test]
    fn invoices_from_two_stations_keep_their_numbers_and_the_chain() {
        let (addr, path, h) = start("CLAVE9");
        let ra = signed_in(&addr, "CLAVE9", "Recepción 1", 2, "0000");
        let rb = signed_in(&addr, "CLAVE9", "Recepción 2", 1, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        a.station = ra.station;
        b.station = rb.station;
        let day = crate::model::today();
        let pending: Vec<(u32, u32)> = a.entries.iter().filter(|e| e.state == crate::model::TxState::Realizado && e.invoice_id.is_none()).map(|e| (e.patient_id, e.id)).collect();
        let (p1, e1) = pending[0];
        let (p2, e2) = *pending.iter().find(|(p, _)| *p != p1).unwrap();
        // los dos facturan sin haber visto la factura del otro
        let ia = ra.chain_op(&mut a, &mut sa, &json!({ "accion": "emitir", "paciente": p1, "tratamientos": [e1], "fecha": day })).unwrap();
        let ib = rb.chain_op(&mut b, &mut sb, &json!({ "accion": "emitir", "paciente": p2, "tratamientos": [e2], "fecha": day })).unwrap();
        let disk = crate::sync::on_disk(&path);
        let (fa, fb) = (disk.invoices.iter().find(|i| i.id == ia).unwrap(), disk.invoices.iter().find(|i| i.id == ib).unwrap());
        assert_eq!(fb.number, fa.number + 1, "números seguidos, sin repetir");
        assert_eq!(fb.prev_hash, fa.hash, "la segunda enlaza con la primera");
        assert!(disk.verify_chain().is_ok(), "{:?}", disk.verify_chain());
        // cada puesto tiene ya su factura (y la del otro, al recoger)
        assert!(a.invoices.iter().any(|i| i.id == ia) && b.invoices.iter().any(|i| i.id == ib));
        assert_eq!(a.entries.iter().find(|e| e.id == e1).unwrap().invoice_id, Some(ia));
        assert_eq!(fa.user, "Recepción", "con el usuario de la sesión");
        // rectificar y anular, también en el servidor y en la cadena
        let ra2 = ra.chain_op(&mut a, &mut sa, &json!({ "accion": "rectificar", "factura": ia, "motivo": "Error en el importe" })).unwrap();
        assert!(crate::sync::on_disk(&path).invoices.iter().any(|i| i.id == ra2 && i.rectifies == Some(ia)));
        assert!(crate::sync::on_disk(&path).verify_chain().is_ok());
        // sin permiso de facturación, no
        let doc = signed_in(&addr, "CLAVE9", "Gabinete", 3, "0000");
        let (mut d, mut sd) = doc.load().unwrap();
        let (p3, e3) = *pending.iter().find(|(p, _)| *p != p1 && *p != p2).unwrap();
        assert!(doc.chain_op(&mut d, &mut sd, &json!({ "accion": "emitir", "paciente": p3, "tratamientos": [e3], "fecha": day })).unwrap_err().contains("facturar"));
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn two_new_patients_never_share_a_history_number() {
        let (addr, path, h) = start("CLAVE10");
        let ra = signed_in(&addr, "CLAVE10", "Recepción 1", 2, "0000");
        let rb = signed_in(&addr, "CLAVE10", "Recepción 2", 1, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        a.station = ra.station;
        b.station = rb.station;
        // los dos dan de alta un paciente a la vez: el mismo número siguiente
        let new_patient = |d: &mut Db, name: &str| {
            let mut p = d.patients[0].clone();
            p.id = d.new_id();
            p.history_no = d.next_history_no();
            p.first_name = name.into();
            d.patients.push(p.clone());
            p
        };
        let pa = new_patient(&mut a, "Desde A");
        let pb = new_patient(&mut b, "Desde B");
        assert_eq!(pa.history_no, pb.history_no);
        ra.push(&mut a, &mut sa).unwrap();
        rb.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        let (na, nb) = (disk.patient(pa.id).unwrap().history_no, disk.patient(pb.id).unwrap().history_no);
        assert_ne!(na, nb, "números de historia distintos");
        assert_eq!(na, pa.history_no, "el primero conserva el suyo");
        assert_eq!(b.patient(pb.id).unwrap().history_no, nb, "el segundo puesto recibe el número corregido");
        let mut all: Vec<u32> = disk.patients.iter().map(|p| p.history_no).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), disk.patients.len());
        // con un número de historia fuera de rango no se aplica nada (ni lo demás que venga)
        let rev = rb.pull(0).unwrap().rev;
        let mut p = disk.patients[0].clone();
        p.id = 9_000_016;
        p.history_no = u32::MAX;
        let mut q = disk.patients[1].clone();
        q.mobile = "699999999".into();
        let body = json!({ "cambios": [
            Change { field: "patients".into(), key: Some(p.id.to_string()), before: None, after: Some(serde_json::to_string(&p).unwrap()) },
            Change { field: "patients".into(), key: Some(q.id.to_string()), before: Some(serde_json::to_string(&disk.patients[1]).unwrap()), after: Some(serde_json::to_string(&q).unwrap()) },
        ] })
        .to_string();
        assert!(rb.post(&format!("/cambios?base={rev}"), &body).unwrap_err().contains("historia"));
        let after = crate::sync::on_disk(&path);
        assert!(after.patient(p.id).is_none() && after.patients[1].mobile == disk.patients[1].mobile);
        assert_eq!(rb.pull(0).unwrap().rev, rev, "ni una revisión más");
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn material_spent_by_two_stations_at_once_is_all_taken_from_stock() {
        let (addr, path, h) = start("CLAVE17");
        let ra = signed_in(&addr, "CLAVE17", "Recepción 1", 2, "0000");
        let rb = signed_in(&addr, "CLAVE17", "Recepción 2", 1, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        a.station = ra.station;
        b.station = rb.station;
        let before = a.product(4).unwrap().stock;
        a.stock_move(4, -0.2, crate::stock::MoveKind::Consumo, "gabinete 1", None, "");
        b.stock_move(4, -0.25, crate::stock::MoveKind::Consumo, "gabinete 2", None, "");
        ra.push(&mut a, &mut sa).unwrap();
        rb.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        let want = before - 0.45;
        assert!((disk.product(4).unwrap().stock - want).abs() < 1e-9, "en el servidor: {}", disk.product(4).unwrap().stock);
        assert!((b.product(4).unwrap().stock - want).abs() < 1e-9, "y el puesto lo sabe");
        // un puesto que reenvía lo mismo (no le llegó la respuesta) no lo descuenta otra vez
        let (mut c, mut sc) = ra.load().unwrap();
        c.station = ra.station;
        let snap_before = sc.clone();
        c.stock_move(4, -1.0, crate::stock::MoveKind::Consumo, "gabinete 3", None, "");
        let resend = (c.clone(), snap_before.clone());
        ra.push(&mut c, &mut sc).unwrap();
        b.stock_move(4, 2.0, crate::stock::MoveKind::Entrada, "pedido", None, "");
        rb.push(&mut b, &mut sb).unwrap();
        let (mut c2, mut sc2) = resend;
        ra.push(&mut c2, &mut sc2).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert!((disk.product(4).unwrap().stock - (want + 1.0)).abs() < 1e-9, "una vez cada movimiento: {}", disk.product(4).unwrap().stock);
        // con cada respuesta, el servidor dice cómo van sus copias de seguridad
        assert!(ra.backup.lock().unwrap().is_some());
        let _ = rb.pull(b.rev).unwrap();
        assert!(rb.backup.lock().unwrap().is_some());
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Lo firmado no lo cambia ni lo borra un puesto: el servidor conserva el
    /// documento y el cuestionario tal como se firmaron; solo acepta que se
    /// anote la revocación.
    #[test]
    fn signed_documents_and_forms_cannot_be_changed_by_a_station() {
        let (addr, path, h) = start("CLAVE19");
        let rd = signed_in(&addr, "CLAVE19", "Gabinete", 3, "0000");
        let (mut a, mut sa) = rd.load().unwrap();
        a.station = rd.station;
        let tid = a.doc_templates.iter().find(|t| t.name.contains("extracción")).map(|t| t.id).unwrap();
        let (d1, d2) = (a.new_document(101, tid, &[38]).unwrap(), a.new_document(101, tid, &[48]).unwrap());
        let strokes = vec![(0..24).map(|i| [i as f32 / 24.0, 0.5]).collect::<Vec<_>>()];
        a.sign_document(d1, "Carlos Martínez Sanz", strokes.clone()).unwrap();
        a.sign_document(d2, "Carlos Martínez Sanz", strokes).unwrap();
        let hid = a.new_health(101, crate::model::today(), "El propio paciente");
        a.sign_health(hid).unwrap();
        rd.push(&mut a, &mut sa).unwrap();
        let signed = crate::sync::on_disk(&path);
        let orig = signed.documents.iter().find(|d| d.id == d1).unwrap().clone();
        let orig_form = signed.health.iter().find(|f| f.id == hid).unwrap().clone();
        // el puesto cambia el texto de uno, borra el otro y cambia una respuesta
        a.documents.iter_mut().find(|d| d.id == d1).unwrap().body = "Otro texto".into();
        a.documents.retain(|d| d.id != d2);
        a.health.iter_mut().find(|f| f.id == hid).unwrap().yes.insert("diabetes".into(), true);
        rd.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.documents.iter().find(|d| d.id == d1).map(|d| d.body.clone()), Some(orig.body.clone()), "el texto firmado no cambia");
        assert!(disk.documents.iter().any(|d| d.id == d2), "un documento firmado no se borra");
        assert_eq!(disk.health.iter().find(|f| f.id == hid), Some(&orig_form), "el cuestionario firmado no cambia");
        assert_eq!(disk.doc_integrity(d1), crate::firmado::Integrity::Intact);
        // y el puesto recibe lo que hay en el servidor
        let (back, _) = rd.load().unwrap();
        assert_eq!(back.documents.iter().find(|d| d.id == d1).unwrap().body, orig.body);
        // revocarlo sí se puede (y queda anotado en el servidor)
        let (mut b, mut sb) = rd.load().unwrap();
        b.station = rd.station;
        b.revoke_document(d1, "Carlos Martínez Sanz", "Prefiere no operarse").unwrap();
        rd.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        let r = disk.documents.iter().find(|d| d.id == d1).unwrap();
        assert!(r.revoked_at.is_some() && r.revoke_reason == "Prefiere no operarse" && r.body == orig.body);
        rd.stop().unwrap();
        h.join().unwrap();
    }

    /// Un puesto no puede pasar lo firmado a otro paciente borrando el suyo en
    /// la misma tanda: fusionar pacientes lo hace el servidor, y solo Dirección.
    #[test]
    fn signed_records_only_change_patient_through_a_merge_on_the_server() {
        use crate::app::ChainOp;
        let (addr, path, h) = start("CLAVE20");
        let rd = signed_in(&addr, "CLAVE20", "Gabinete", 3, "0000");
        let (mut a, mut sa) = rd.load().unwrap();
        a.station = rd.station;
        let tid = a.doc_templates.iter().find(|t| t.name.contains("extracción")).map(|t| t.id).unwrap();
        let doc = a.new_document(102, tid, &[38]).unwrap();
        a.sign_document(doc, "Laura Fernández Ortiz", vec![(0..24).map(|i| [i as f32 / 24.0, 0.5]).collect()]).unwrap();
        rd.push(&mut a, &mut sa).unwrap();
        // borra al paciente 102 y pasa su consentimiento firmado al 103
        a.patients.retain(|p| p.id != 102);
        a.documents.iter_mut().find(|d| d.id == doc).unwrap().patient_id = 103;
        rd.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.documents.iter().find(|d| d.id == doc).unwrap().patient_id, 102, "no se pasa a otro paciente");
        // fusionar de verdad: en el servidor, solo Dirección
        let merge = ChainOp::MergePatients { keep: 104, remove: 105 };
        let (mut b, mut sb) = rd.load().unwrap();
        let doc2 = b.new_document(105, tid, &[48]).unwrap();
        b.sign_document(doc2, "Sofía Romero Navas", vec![(0..24).map(|i| [i as f32 / 24.0, 0.5]).collect()]).unwrap();
        b.station = rd.station;
        rd.push(&mut b, &mut sb).unwrap();
        assert!(rd.chain_op(&mut b, &mut sb, &merge.json()).unwrap_err().contains("Dirección"));
        let dir = signed_in(&addr, "CLAVE20", "Despacho", 1, "0000");
        let (mut c, mut sc) = dir.load().unwrap();
        c.station = dir.station;
        dir.chain_op(&mut c, &mut sc, &merge.json()).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert!(disk.patient(105).is_none());
        assert_eq!(disk.documents.iter().find(|d| d.id == doc2).unwrap().patient_id, 104, "la fusión sí lo pasa");
        assert_eq!(disk.doc_integrity(doc2), crate::firmado::Integrity::Intact, "y la firma sigue valiendo");
        dir.stop().unwrap();
        h.join().unwrap();
    }

    /// Dar por revisados los avisos de una receta solo lo acepta el servidor de
    /// quien puede recetar, y con su nombre (no el que mande el puesto).
    #[test]
    fn only_a_prescriber_marks_prescription_warnings_reviewed() {
        let (addr, path, h) = start("CLAVE21");
        let hyg = signed_in(&addr, "CLAVE21", "Higiene", 5, "0000");
        let (mut a, mut sa) = hyg.load().unwrap();
        a.station = hyg.station;
        a.patient_mut(101).unwrap().allergies = "Penicilina".into();
        let id = a.new_prescription(101, 1);
        let amox = a.drugs.iter().find(|d| d.name.starts_with("Amoxicilina 500")).cloned().unwrap();
        a.prescriptions.iter_mut().find(|r| r.id == id).unwrap().items.push(crate::finance::rx_item_from_drug(&amox));
        a.review_rx_warnings(id, "Dra. Ruiz");
        hyg.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        let r = disk.prescriptions.iter().find(|r| r.id == id).unwrap();
        assert!(r.reviewed_warnings.is_empty() && r.reviewed_by.is_empty() && r.reviewed_at.is_none(), "la higienista no los revisa");
        let doc = signed_in(&addr, "CLAVE21", "Gabinete", 3, "0000");
        let (mut b, mut sb) = doc.load().unwrap();
        b.station = doc.station;
        b.review_rx_warnings(id, "Otro nombre");
        doc.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        let r = disk.prescriptions.iter().find(|r| r.id == id).unwrap();
        assert_eq!(r.reviewed_by, "Dra. Ruiz", "con el nombre de quien entró");
        assert!(!r.reviewed_warnings.is_empty());
        doc.stop().unwrap();
        h.join().unwrap();
    }

    /// Un análisis con IA lo da por bueno solo un profesional colegiado, con
    /// el nombre de quien entró, y después no se cambia la revisión.
    #[test]
    fn only_a_licensed_professional_reviews_an_ai_analysis() {
        let (addr, path, h) = start("CLAVE36");
        let hyg = signed_in(&addr, "CLAVE36", "Higiene", 5, "0000");
        let (mut a, mut sa) = hyg.load().unwrap();
        a.station = hyg.station;
        let id = a.ia_reports[0].id;
        assert!(a.ia_reports[0].review.is_none());
        // la higienista se lo salta en su puesto: el servidor no lo acepta
        a.ia_reports[0].review = Some(crate::ia::IaReview { at: crate::model::now(), by: "Dra. Ruiz".into(), valid: true });
        hyg.push(&mut a, &mut sa).unwrap();
        assert!(crate::sync::on_disk(&path).ia_reports.iter().find(|r| r.id == id).unwrap().review.is_none(), "la higienista no lo revisa");
        let doc = signed_in(&addr, "CLAVE36", "Gabinete", 3, "0000");
        let (mut b, mut sb) = doc.load().unwrap();
        b.station = doc.station;
        b.ia_reports.iter_mut().find(|r| r.id == id).unwrap().review = Some(crate::ia::IaReview { at: crate::model::now(), by: "Otro nombre".into(), valid: true });
        doc.push(&mut b, &mut sb).unwrap();
        let v = crate::sync::on_disk(&path).ia_reports.iter().find(|r| r.id == id).unwrap().review.clone().unwrap();
        assert_eq!((v.by.as_str(), v.valid), ("Dra. Ruiz", true), "con el nombre de quien entró");
        // una revisión hecha no se cambia, ni lo que contestó la IA, ni se borra
        let stored = crate::sync::on_disk(&path).ia_reports.iter().find(|r| r.id == id).cloned().unwrap();
        {
            let r = b.ia_reports.iter_mut().find(|r| r.id == id).unwrap();
            r.review = Some(crate::ia::IaReview { at: crate::model::now(), by: "Dra. Ruiz".into(), valid: false });
            r.analysis.summary = "Sin hallazgos.".into();
            r.free = !r.free;
            r.model = "otro".into();
        }
        doc.push(&mut b, &mut sb).unwrap();
        assert_eq!(crate::sync::on_disk(&path).ia_reports.iter().find(|r| r.id == id).unwrap(), &stored, "nada cambia");
        b.ia_reports.retain(|r| r.id != id);
        doc.push(&mut b, &mut sb).unwrap();
        assert!(crate::sync::on_disk(&path).ia_reports.iter().any(|r| r.id == id), "lo revisado no se borra");
        // un borrador de pago tampoco (cuenta en el límite del mes); uno gratuito sin usar, sí
        let mut paid = stored.clone();
        (paid.id, paid.free, paid.review, paid.planned) = (b.new_id(), false, None, vec![]);
        let mut free = paid.clone();
        (free.id, free.free) = (b.new_id(), true);
        b.ia_reports.push(paid.clone());
        b.ia_reports.push(free.clone());
        doc.push(&mut b, &mut sb).unwrap();
        b.ia_reports.retain(|r| r.id != paid.id && r.id != free.id);
        doc.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert!(disk.ia_reports.iter().any(|r| r.id == paid.id) && !disk.ia_reports.iter().any(|r| r.id == free.id));
        doc.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Dos gabinetes marcan a la vez como hecho el mismo tratamiento previsto:
    /// el material se descuenta una vez, no dos.
    #[test]
    fn the_same_treatment_done_on_two_stations_spends_its_material_once() {
        let (addr, path, h) = start("CLAVE22");
        let ra = signed_in(&addr, "CLAVE22", "Gabinete 1", 3, "0000");
        let rb = signed_in(&addr, "CLAVE22", "Gabinete 2", 1, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        a.station = ra.station;
        let today = crate::model::today();
        let e = a.add_entry(110, "0302", vec![16], vec![], crate::model::TxState::Previsto, 1, today);
        ra.push(&mut a, &mut sa).unwrap();
        let stock = crate::sync::on_disk(&path).product(4).unwrap().stock;
        let (mut b, mut sb) = rb.load().unwrap();
        b.station = rb.station;
        a.realize_entry(e, today, 1);
        b.realize_entry(e, today, 1);
        ra.push(&mut a, &mut sa).unwrap();
        rb.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert!((disk.product(4).unwrap().stock - (stock - 0.2)).abs() < 1e-6, "una vez: {} → {}", stock, disk.product(4).unwrap().stock);
        let net: f64 = disk.stock_moves.iter().filter(|m| m.entry_id == Some(e) && m.product_id == 4).map(|m| m.qty).sum();
        assert!((net + 0.2).abs() < 1e-6, "un solo consumo del composite: {net}");
        // deshacerlo y volver a hacerlo sí gasta otra vez (lo devuelto vuelve)
        let (mut c, mut sc) = ra.load().unwrap();
        c.station = ra.station;
        c.unrealize_entry(e).unwrap();
        c.realize_entry(e, today, 1);
        ra.push(&mut c, &mut sc).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert!((disk.product(4).unwrap().stock - (stock - 0.2)).abs() < 1e-6);
        ra.stop().unwrap();
        h.join().unwrap();
    }

    /// Dos puestos importan el mismo extracto a la vez: el servidor lo añade
    /// una vez. Y no concilian dos veces lo mismo, ni deshacen dos veces, ni
    /// crean dos gastos con el mismo cargo.
    #[test]
    fn a_statement_imported_and_reconciled_by_two_stations_at_once_counts_once() {
        use crate::app::ChainOp;
        use crate::bank::{Match, RawMove};
        let (addr, path, h) = start("CLAVE31");
        let ra = signed_in(&addr, "CLAVE31", "Administración", 1, "0000");
        let rb = signed_in(&addr, "CLAVE31", "Dirección", 1, "0000");
        let rc = signed_in(&addr, "CLAVE31", "Recepción", 2, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        let (mut c, mut sc) = rc.load().unwrap();
        a.station = ra.station;
        b.station = rb.station;
        c.station = rc.station;
        let before = crate::sync::on_disk(&path).bank_moves.len();
        let day = crate::model::today();
        let mv = |c: &str, amount: f64| RawMove { date: day, amount, concept: c.into(), reference: String::new() };
        // dos transferencias iguales el mismo día son dos
        let import = ChainOp::ImportBank { account: 1, moves: vec![mv("TRANSFERENCIA ANA", 57.0), mv("TRANSFERENCIA ANA", 57.0), mv("COMISION", -12.34)] }.json();
        assert_eq!(ra.chain_op(&mut a, &mut sa, &import).unwrap(), 3);
        assert_eq!(rb.chain_op(&mut b, &mut sb, &import).unwrap(), 0, "el otro puesto no lo tenía aún, y no lo repite");
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.bank_moves.len(), before + 3);
        assert_eq!(b.bank_moves.len(), before + 3, "y lo ve");
        // el mismo cobro con dos movimientos distintos desde dos puestos: el segundo, no
        let pid = a.add_payment(102, 57.0, "Transferencia", "Pago");
        ra.push(&mut a, &mut sa).unwrap();
        let ids: Vec<u32> = disk.bank_moves.iter().filter(|m| m.concept == "TRANSFERENCIA ANA").map(|m| m.id).collect();
        let pick = |i: usize| ChainOp::MatchBank { bank_move: ids[i], with: Match::Payments(vec![pid]) }.json();
        ra.chain_op(&mut a, &mut sa, &pick(0)).unwrap();
        assert!(rb.chain_op(&mut b, &mut sb, &pick(1)).unwrap_err().contains("con otro movimiento"));
        let undo = ChainOp::UnmatchBank { bank_move: ids[0] }.json();
        ra.chain_op(&mut a, &mut sa, &undo).unwrap();
        assert!(rb.chain_op(&mut b, &mut sb, &undo).unwrap_err().contains("no está conciliado"), "no se deshace dos veces");
        // un gasto con un cargo, una vez
        let fee = disk.bank_moves.iter().find(|m| m.concept == "COMISION").unwrap().id;
        let expense = ChainOp::BankExpense { bank_move: fee }.json();
        ra.chain_op(&mut a, &mut sa, &expense).unwrap();
        assert!(rb.chain_op(&mut b, &mut sb, &expense).unwrap_err().contains("ya está conciliado"));
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.expenses.iter().filter(|e| e.concept == "COMISION").count(), 1);
        // lo que llega mal se rechaza, y Recepción no lleva el banco
        assert!(rc.chain_op(&mut c, &mut sc, &import).unwrap_err().contains("banco"));
        assert!(rb.chain_op(&mut b, &mut sb, &json!({ "accion": "importar_extracto", "cuenta": 1, "movimientos": [{ "fecha": "2026-11-05", "centimos": 1.5 }] })).is_err());
        assert!(rb.chain_op(&mut b, &mut sb, &json!({ "accion": "importar_extracto", "cuenta": 99, "movimientos": [{ "fecha": "2026-11-05", "centimos": 150 }] })).unwrap_err().contains("cuenta"));
        assert!(rb.chain_op(&mut b, &mut sb, &json!({ "accion": "conciliar_movimiento", "movimiento": ids[1], "con": { "Otro": "" } })).is_err());
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// El puesto sabe la zona horaria del servidor y avisa si no es la suya
    /// (lo que hace el servidor llevaría otra fecha en los informes).
    #[test]
    fn a_station_is_warned_when_the_server_has_another_time_zone() {
        let (addr, path, h) = start("CLAVE33");
        let mut r = Remote::new(&addr, "CLAVE33", 0);
        let info = connect(&mut r, "Recepción", 1).unwrap();
        let here = chrono::Local::now().offset().local_minus_utc();
        assert_eq!(info.utc_offset, Some(here), "el servidor da su desfase");
        assert_eq!(clock_warning(info.utc_offset, here), None, "misma zona: sin aviso");
        let w = clock_warning(Some(0), 3600).unwrap();
        assert!(w.contains("UTC+00:00") && w.contains("UTC+01:00"), "{w}");
        assert!(clock_warning(Some(-12_600), 0).unwrap().contains("UTC-03:30"));
        assert_eq!(clock_warning(None, 3600), None, "un servidor anterior no lo dice");
        let rb = signed_in(&addr, "CLAVE33", "Dirección", 1, "0000");
        rb.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Dos puestos que pulsan «Enviar por correo» a la vez: el servidor da
    /// cada correo pendiente a uno solo.
    #[test]
    fn two_stations_sending_emails_at_once_send_each_one_once() {
        use crate::app::ChainOp;
        use crate::comms::{Channel, Message, MsgKind, MsgState};
        let (addr, path, h) = start("CLAVE34");
        let ra = signed_in(&addr, "CLAVE34", "Recepción 1", 2, "0000");
        let rb = signed_in(&addr, "CLAVE34", "Recepción 2", 2, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        a.station = ra.station;
        b.station = rb.station;
        let mut ids = vec![];
        for pid in [101, 102, 103] {
            let id = a.new_id();
            a.messages.push(Message {
                id,
                patient_id: pid,
                kind: MsgKind::Libre,
                channel: Channel::Email,
                to: format!("p{pid}@example.com"),
                subject: "Aviso".into(),
                text: "Hola".into(),
                created: crate::model::now(),
                sent_at: None,
                state: MsgState::Pendiente,
                related: None,
                user: String::new(),
                error: String::new(),
                attempts: 0,
                uncertain: false,
                sending: String::new(),
                sending_since: None,
                sent_by: String::new(),
            });
            ids.push(id);
        }
        ra.push(&mut a, &mut sa).unwrap();
        let claim = |t: &str| ChainOp::ClaimEmails { ids: ids.clone(), token: t.into() }.json();
        assert_eq!(ra.chain_op(&mut a, &mut sa, &claim("A")).unwrap(), 3);
        assert_eq!(rb.chain_op(&mut b, &mut sb, &claim("B")).unwrap(), 0, "ya los envía el otro puesto");
        assert_eq!(a.claimed_emails("A").len(), 3);
        assert!(b.claimed_emails("B").is_empty() && b.pending_emails().is_empty(), "y el segundo lo ve");
        // lo que llega mal se rechaza
        assert!(rb.chain_op(&mut b, &mut sb, &json!({ "accion": "reservar_correos", "mensajes": [1], "marca": "" })).is_err());
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Dos gabinetes registran a la vez un control del mismo caso de
    /// ortodoncia: quedan los dos (antes, el segundo borraba el primero).
    #[test]
    fn two_ortho_controls_recorded_at_once_are_both_kept() {
        let (addr, path, h) = start("CLAVE35");
        let ra = signed_in(&addr, "CLAVE35", "Gabinete 1", 1, "0000");
        let rb = signed_in(&addr, "CLAVE35", "Gabinete 2", 1, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        a.station = ra.station;
        b.station = rb.station;
        let case = a.ortho.iter().find(|c| c.state == crate::extra::OrthoState::Activo).unwrap().id;
        let n = a.ortho.iter().find(|c| c.id == case).unwrap().visits.len();
        let today = crate::model::today();
        let visit = |work: &str| crate::extra::OrthoVisit { date: today, work: work.into(), wire: String::new(), hygiene_ok: true, next_weeks: 4, notes: String::new() };
        a.add_ortho_visit(case, visit("Cambio de arco")).unwrap();
        b.add_ortho_visit(case, visit("Elásticos")).unwrap();
        ra.push(&mut a, &mut sa).unwrap();
        rb.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        let works: Vec<String> = disk.ortho.iter().find(|c| c.id == case).unwrap().visits.iter().map(|v| v.work.clone()).collect();
        assert_eq!(works.len(), n + 2, "{works:?}");
        assert!(works.contains(&"Cambio de arco".to_string()) && works.contains(&"Elásticos".to_string()));
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Dos puestos marcan a la vez como recibido el mismo trabajo de
    /// laboratorio: su coste va a Gastos una vez, no dos.
    #[test]
    fn a_lab_work_received_on_two_stations_is_one_expense() {
        let (addr, path, h) = start("CLAVE37");
        let ra = signed_in(&addr, "CLAVE37", "Recepción 1", 1, "0000");
        let rb = signed_in(&addr, "CLAVE37", "Recepción 2", 1, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        a.station = ra.station;
        let lab = a.suppliers.iter().find(|s| s.is_lab).map(|s| s.id).unwrap();
        let id = a.new_lab_work(101, None, lab, 7);
        a.lab_works.iter_mut().find(|w| w.id == id).unwrap().cost = 180.0;
        a.set_lab_state(id, crate::stock::LabState::Enviado);
        ra.push(&mut a, &mut sa).unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        b.station = rb.station;
        a.set_lab_state(id, crate::stock::LabState::Recibido);
        b.set_lab_state(id, crate::stock::LabState::Recibido);
        ra.push(&mut a, &mut sa).unwrap();
        rb.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        let tag = format!("#L{id}");
        assert_eq!(disk.expenses.iter().filter(|e| e.concept.ends_with(&tag)).count(), 1, "una vez");
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// La respuesta del servidor se pierde y el puesto vuelve a mandar lo
    /// mismo: no se duplica nada (ni cobros ni recibos).
    #[test]
    fn a_push_resent_after_a_lost_answer_changes_nothing() {
        let (addr, path, h) = start("CLAVE38");
        let ra = signed_in(&addr, "CLAVE38", "Recepción", 2, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        a.station = ra.station;
        let id = a.add_payment(101, 25.0, "Tarjeta", "Pago");
        // lo que el puesto tenía antes de mandarlo (la respuesta no le llega)
        let (mut again, mut stale) = (a.clone(), sa.clone());
        ra.push(&mut a, &mut sa).unwrap();
        let after_first = crate::sync::on_disk(&path);
        ra.push(&mut again, &mut stale).unwrap();
        let after_second = crate::sync::on_disk(&path);
        assert_eq!(after_second.payments.iter().filter(|p| p.id == id).count(), 1);
        assert_eq!(after_second.payments.len(), after_first.payments.len(), "nada nuevo");
        assert_eq!(serde_json::to_value(&*after_second.payments).unwrap(), serde_json::to_value(&*after_first.payments).unwrap());
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// El servidor se reinicia (o se va la luz): arranca con todo lo
    /// guardado y los puestos siguen donde estaban.
    #[test]
    fn the_server_restarts_with_everything_and_stations_carry_on() {
        let (addr, path, h) = start("CLAVE39");
        let ra = signed_in(&addr, "CLAVE39", "Recepción", 2, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        a.station = ra.station;
        let id = a.add_payment(101, 25.0, "Tarjeta", "Antes del reinicio");
        ra.push(&mut a, &mut sa).unwrap();
        ra.stop().unwrap();
        h.join().unwrap();
        // arranca otra vez con lo que hay en disco (fichero y diario)
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr2 = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let initial = crate::sync::on_disk(&path);
        let rev = initial.rev;
        let p2 = path.clone();
        let h2 = std::thread::spawn(move || serve(listener, p2, "CLAVE39".into(), new_identity().unwrap(), initial));
        let rb = signed_in(&addr2, "CLAVE39", "Recepción", 2, "0000");
        let (mut b, mut sb) = rb.load().unwrap();
        b.station = rb.station;
        assert!(b.payments.iter().any(|p| p.id == id), "lo de antes sigue");
        assert!(b.rev >= rev, "la revisión no vuelve atrás");
        let id2 = b.add_payment(102, 10.0, "Efectivo", "Después");
        rb.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert!(disk.payments.iter().any(|p| p.id == id) && disk.payments.iter().any(|p| p.id == id2));
        rb.stop().unwrap();
        h2.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Si no se puede escribir en el disco (lleno, sin permiso), el cambio no
    /// queda a medias: el servidor sigue con lo de antes, el puesto recibe el
    /// error y, cuando se puede escribir, se guarda entero.
    #[test]
    fn a_failed_write_leaves_nothing_half_saved() {
        let dir = std::env::temp_dir().join(format!("ds_server_write_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clinica.json");
        let mut st = State::new(&Db::sample(NaiveDate::from_ymd_opt(2026, 11, 6).unwrap()), path.clone(), "CLAVE40", vec![]);
        st.rewrite().unwrap();
        let before = st.snap.clone();
        let rev = st.rev();
        let mut db = st.snap.to_db().unwrap();
        let id = db.add_payment(101, 25.0, "Tarjeta", "Pago");
        let changes = crate::sync::diff(&before, &Snapshot::from_db(&db));
        // el diario no se puede escribir
        st.journal = Some(std::fs::File::open(crate::sync::journal_path(&path)).unwrap());
        assert!(st.save(changes.clone(), 1, 1).is_err());
        assert_eq!(st.rev(), rev, "la revisión no avanza");
        assert!(st.snap == before, "el servidor sigue con lo de antes");
        assert!(!crate::sync::on_disk(&path).payments.iter().any(|p| p.id == id), "en disco, nada a medias");
        // ya se puede escribir: el mismo cambio se guarda entero
        st.journal = None;
        st.save(changes, 1, 1).unwrap();
        assert!(crate::sync::on_disk(&path).payments.iter().any(|p| p.id == id));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El lector de peticiones con bytes al azar y con peticiones reales
    /// estropeadas: nunca se cae ni lee de más.
    #[test]
    fn the_request_reader_survives_random_bytes() {
        let mut rng = crate::test_rng::Rng(0x5eed_0001);
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let real: [&[u8]; 3] =
            [b"POST /cambios?base=3 HTTP/1.1\r\nX-Clave: K\r\nX-Sesion: abc\r\nContent-Length: 2\r\n\r\n{}", b"GET /datos HTTP/1.1\r\nX-Clave: K\r\n\r\n", b"POST /cadena HTTP/1.1\r\nContent-Length: 30\r\n\r\n{\"accion\":\"conciliar\"}"];
        for i in 0..20_000 {
            let n = rng.below(400);
            let input = if i % 3 == 0 { rng.bytes(n) } else { rng.mutate(real[i % 3]) };
            let mut r = std::io::BufReader::new(&input[..]);
            if let Ok(Some(mut req)) = read_head(&mut r, peer)
                && read_body(&mut r, &mut req, 4096).is_ok()
            {
                assert!(req.body.len() <= 4096);
            }
        }
    }

    /// El servidor con peticiones al azar y con peticiones reales estropeadas
    /// (con sesión y sin ella): nunca se cae, contesta con un código, y sus
    /// datos se siguen pudiendo leer.
    #[test]
    fn the_server_survives_random_requests_and_its_data_stays_readable() {
        let dir = std::env::temp_dir().join(format!("ds_server_fuzz_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut st = State::new(&Db::sample(NaiveDate::from_ymd_opt(2026, 11, 6).unwrap()), dir.join("clinica.json"), "CLAVE42", vec![1; 32]);
        st.rewrite().unwrap();
        let token = st.new_session(1, 1);
        let mut db = st.snap.to_db().unwrap();
        db.add_payment(101, 25.0, "Tarjeta", "Pago");
        let changes = crate::sync::diff(&st.snap, &Snapshot::from_db(&db));
        let bodies: Vec<Vec<u8>> = vec![
            json!({ "cambios": changes }).to_string().into_bytes(),
            ChainOp::Reconcile.json().to_string().into_bytes(),
            ChainOp::ImportBank { account: 1, moves: vec![crate::bank::RawMove { date: NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(), amount: 5.0, concept: "X".into(), reference: String::new() }] }.json().to_string().into_bytes(),
            json!({ "usuario": 1, "contrasena": "0000" }).to_string().into_bytes(),
            b"{}".to_vec(),
        ];
        let paths = ["/acceso", "/registro", "/entrar", "/codigo", "/estado", "/datos", "/cambios", "/cadena", "/movil", "/pin", "/nada", "/../../etc/passwd"];
        let mut rng = crate::test_rng::Rng(0x5eed_0002);
        let peer = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 7));
        for i in 0..4_000 {
            let (n, k) = (rng.below(300), rng.below(bodies.len()));
            let body = if i % 4 == 0 { rng.bytes(n) } else { rng.mutate(&bodies[k]) };
            let mut headers = vec![("X-Clave".to_string(), if rng.below(10) == 0 { "MAL".to_string() } else { "CLAVE42".to_string() })];
            if rng.below(4) > 0 {
                headers.push(("X-Sesion".to_string(), token.clone()));
            }
            let query = ["", "base=3", "base=-1", "base=99999999999999999999", "reto=zz"][rng.below(5)].to_string();
            let req = Request { method: ["GET", "POST", "PUT"][rng.below(3)].into(), path: paths[rng.below(paths.len())].into(), query, headers, body, peer };
            let (resp, _) = st.handle(&req, "CLAVE42");
            assert!((200..600).contains(&resp.code), "{}", resp.code);
            // (tantas peticiones seguidas acaban en «demasiados intentos»: se olvidan)
            st.key_fails.clear();
            st.logins.clear();
        }
        assert!(st.snap.to_db().is_ok(), "los datos se siguen leyendo");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Un apagón a mitad de escribir el diario deja la última línea cortada:
    /// se recupera todo hasta el último cambio entero. Y una línea dañada en
    /// medio no deja aplicar lo de después (quedaría un hueco sin avisar).
    #[test]
    fn a_cut_or_damaged_journal_never_applies_changes_out_of_order() {
        let dir = std::env::temp_dir().join(format!("ds_server_journal_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clinica.json");
        let mut st = State::new(&Db::sample(NaiveDate::from_ymd_opt(2026, 11, 6).unwrap()), path.clone(), "CLAVE43", vec![]);
        st.rewrite().unwrap();
        let rev0 = st.rev();
        let mut ids = vec![];
        for amount in [10.0, 20.0] {
            let before = st.snap.clone();
            let mut db = st.snap.to_db().unwrap();
            ids.push(db.add_payment(101, amount, "Tarjeta", "Pago"));
            st.save(crate::sync::diff(&before, &Snapshot::from_db(&db)), 1, 1).unwrap();
        }
        let journal = crate::sync::journal_path(&path);
        let text = std::fs::read_to_string(&journal).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        // la segunda línea, cortada a la mitad
        std::fs::write(&journal, format!("{}\n{}", lines[0], &lines[1][..lines[1].len() / 2])).unwrap();
        let db = crate::sync::on_disk(&path);
        assert_eq!(db.rev, rev0 + 1);
        assert!(db.payments.iter().any(|p| p.id == ids[0]) && !db.payments.iter().any(|p| p.id == ids[1]));
        // la primera dañada y la segunda entera: no se aplica la segunda sin la primera
        std::fs::write(&journal, format!("{}\n{}\n", lines[0].replace('"', "#"), lines[1])).unwrap();
        let db = crate::sync::on_disk(&path);
        assert_eq!(db.rev, rev0, "se queda en el último cambio entero");
        assert!(!db.payments.iter().any(|p| ids.contains(&p.id)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_installment_paid_by_two_stations_at_once_is_charged_once() {
        use crate::app::ChainOp;
        let (addr, path, h) = start("CLAVE18");
        let ra = signed_in(&addr, "CLAVE18", "Recepción 1", 2, "0000");
        let rb = signed_in(&addr, "CLAVE18", "Dirección", 1, "0000");
        let rd = signed_in(&addr, "CLAVE18", "Gabinete", 3, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        a.station = ra.station;
        let bid = a.budgets.iter().find(|x| x.patient_id == 101 && x.plan.is_some()).unwrap().id;
        a.accept_budget(bid, crate::model::today());
        ra.push(&mut a, &mut sa).unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        let (mut dr, mut sd) = rd.load().unwrap();
        b.station = rb.station;
        dr.station = rd.station;
        // los dos cobran la primera cuota a la vez: el segundo recibe un aviso
        let pay = ChainOp::PayInstallment { budget: bid, index: 0, method: "Efectivo".into() };
        let id = ra.chain_op(&mut a, &mut sa, &pay.json()).unwrap();
        assert!(rb.chain_op(&mut b, &mut sb, &pay.json()).unwrap_err().contains("pagada"));
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.payments.iter().filter(|p| p.installment == Some((bid, 0))).count(), 1, "una sola vez");
        assert!(b.budgets.iter().find(|x| x.id == bid).unwrap().plan.as_ref().unwrap().paid[0], "y el segundo puesto lo ve");
        // anular y devolver: con permiso de caja (un doctor, no)
        let cancel = ChainOp::CancelPayment { payment: id, reason: "error".into() };
        assert!(rd.chain_op(&mut dr, &mut sd, &cancel.json()).unwrap_err().contains("caja"));
        ra.chain_op(&mut a, &mut sa, &cancel.json()).unwrap();
        assert!(ra.chain_op(&mut a, &mut sa, &cancel.json()).unwrap_err().contains("anulado"), "no se anula dos veces");
        let disk = crate::sync::on_disk(&path);
        assert!(!disk.budgets.iter().find(|x| x.id == bid).unwrap().plan.as_ref().unwrap().paid[0], "la cuota vuelve a estar por pagar");
        let credit = -disk.patient_balance(101);
        if credit > 1.0 {
            let refund = ChainOp::Refund { patient: 101, amount: 1.0, method: "Efectivo".into(), reason: "prueba".into() };
            assert!(rd.chain_op(&mut dr, &mut sd, &refund.json()).unwrap_err().contains("caja"));
            ra.chain_op(&mut a, &mut sa, &refund.json()).unwrap();
        }
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_payment_made_while_another_station_closes_the_cash_leaves_it_balanced() {
        let (addr, path, h) = start("CLAVE14");
        let ra = signed_in(&addr, "CLAVE14", "Recepción 1", 2, "0000");
        let rb = signed_in(&addr, "CLAVE14", "Recepción 2", 1, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        a.station = ra.station;
        b.station = rb.station;
        // A cierra la caja con su arqueo (sin abrir otra)
        let s1 = a.open_cash().unwrap().id;
        let expected = a.cash_expected(a.open_cash().unwrap());
        a.close_cash_session(s1, expected);
        ra.push(&mut a, &mut sa).unwrap();
        // B, que aún no lo sabe, cobra en efectivo en la caja cerrada
        let late = b.add_payment(100, 20.0, "Efectivo", "tarde");
        rb.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.payments.iter().find(|p| p.id == late).unwrap().cash_session, None, "sin caja abierta, en ninguna");
        assert_eq!(b.payments.iter().find(|p| p.id == late).unwrap().cash_session, None, "y el puesto lo sabe");
        assert!((disk.cash_expected(disk.cash.iter().find(|c| c.id == s1).unwrap()) - expected).abs() < 0.005, "la caja cerrada sigue cuadrada");
        // A abre otra; B la ve. Después A la cierra y abre la siguiente, y B cobra sin saberlo
        let s2 = a.open_cash_session(100.0);
        ra.push(&mut a, &mut sa).unwrap();
        let d = rb.pull(b.rev).unwrap();
        rb.receive(&mut b, &mut sb, d).unwrap();
        assert_eq!(b.open_cash().map(|c| c.id), Some(s2));
        a.close_cash_session(s2, 100.0);
        let s3 = a.open_cash_session(100.0);
        ra.push(&mut a, &mut sa).unwrap();
        let late = b.add_payment(100, 30.0, "Efectivo", "tarde otra vez");
        rb.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.payments.iter().find(|p| p.id == late).unwrap().cash_session, Some(s3), "a la caja abierta");
        assert!((disk.cash_expected(disk.cash.iter().find(|c| c.id == s2).unwrap()) - 100.0).abs() < 0.005);
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn remittances_and_claims_made_by_two_stations_at_once_charge_once() {
        use crate::app::ChainOp;
        let (addr, path, h) = start("CLAVE15");
        let ra = signed_in(&addr, "CLAVE15", "Recepción 1", 2, "0000");
        let rb = signed_in(&addr, "CLAVE15", "Recepción 2", 1, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        a.station = ra.station;
        // un presupuesto financiado aceptado, con cuotas para domiciliar
        let bid = a.budgets.iter().find(|x| x.patient_id == 101 && x.plan.is_some()).unwrap().id;
        a.accept_budget(bid, crate::model::today());
        ra.push(&mut a, &mut sa).unwrap();
        let server_user = crate::sync::on_disk(&path).config.current_user;
        let (mut b, mut sb) = rb.load().unwrap();
        b.station = rb.station;
        let until = crate::model::today() + chrono::Duration::days(40);
        let col = crate::model::today() + chrono::Duration::days(5);
        let n = b.due_installments(until).iter().filter(|x| x.problem.is_none()).count();
        assert!(n > 0);
        // los dos generan la remesa a la vez: el segundo ya no tiene cuotas que remesar
        let remit = ChainOp::Remit { until, collection: col }.json();
        let rid = ra.chain_op(&mut a, &mut sa, &remit).unwrap();
        assert!(rb.chain_op(&mut b, &mut sb, &remit).unwrap_err().contains("cuotas"));
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.remittances.len(), 1);
        assert_eq!(disk.remittances[0].items.len(), n);
        assert!(b.remittances.iter().any(|r| r.id == rid), "el segundo puesto ve la remesa del primero");
        // los dos registran el cobro de la remesa: una sola vez
        let pays = disk.payments.len();
        ra.chain_op(&mut a, &mut sa, &ChainOp::Collected { remittance: rid }.json()).unwrap();
        assert!(rb.chain_op(&mut b, &mut sb, &ChainOp::Collected { remittance: rid }.json()).unwrap_err().contains("cobrada"));
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.payments.len(), pays + n, "cada cuota se cobra una vez");
        assert!(disk.payments.iter().filter(|p| p.method == "Domiciliación").all(|p| p.cash_session.is_none() || disk.cash.iter().any(|c| Some(c.id) == p.cash_session)));
        // devolución de un recibo
        ra.chain_op(&mut a, &mut sa, &ChainOp::Returned { remittance: rid, item: 0, reason: "AM04 Saldo insuficiente".into() }.json()).unwrap();
        assert!(rb.chain_op(&mut b, &mut sb, &ChainOp::Returned { remittance: rid, item: 0, reason: String::new() }.json()).is_err());
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.payments.iter().filter(|p| p.concept.starts_with("Devolución")).count(), 1);
        assert_eq!(disk.remittances[0].items[0].return_reason, "AM04 Saldo insuficiente");
        // liquidaciones a la mutua a la vez: los actos, una vez
        let from = crate::model::today() - chrono::Duration::days(3650);
        let claim = ChainOp::Claim { tariff: 2, from, to: crate::model::today(), date: crate::model::today() }.json();
        let cid = ra.chain_op(&mut a, &mut sa, &claim).unwrap();
        assert!(rb.chain_op(&mut b, &mut sb, &claim).is_err());
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.claims.len(), 1);
        assert!(disk.audit_log.iter().any(|x| x.action == "Liquidación a mutua"));
        assert!(b.claims.iter().any(|c| c.id == cid));
        // el servidor no deja su usuario ni su centro en la configuración
        assert_eq!(disk.config.current_user, server_user);
        // cada operación, con su permiso: Recepción no lleva el banco (Dirección sí)
        assert!(ra.chain_op(&mut a, &mut sa, &ChainOp::Reconcile.json()).unwrap_err().contains("banco"));
        assert!(rb.chain_op(&mut b, &mut sb, &ChainOp::Reconcile.json()).is_ok());
        assert!(ra.chain_op(&mut a, &mut sa, &json!({ "accion": "otra" })).is_err());
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn an_online_request_confirmed_by_two_stations_at_once_makes_one_appointment() {
        use crate::app::ChainOp;
        let (addr, path, h) = start("CLAVE16");
        let ra = signed_in(&addr, "CLAVE16", "Recepción 1", 2, "0000");
        let rb = signed_in(&addr, "CLAVE16", "Gabinete 1", 3, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        a.station = ra.station;
        b.station = rb.station;
        let req = a.booking_requests.iter().find(|r| r.state == crate::booking::ReqState::Pendiente && r.replaces.is_none() && a.is_free(r.doctor_id, r.date, r.start, r.end(), None)).unwrap().id;
        let appts = crate::sync::on_disk(&path).appointments.len();
        let appt = ra.chain_op(&mut a, &mut sa, &ChainOp::Confirm { request: req }.json()).unwrap();
        assert!(rb.chain_op(&mut b, &mut sb, &ChainOp::Confirm { request: req }.json()).unwrap_err().contains("atendió"));
        assert!(rb.chain_op(&mut b, &mut sb, &ChainOp::Reject { request: req, why: "no".into() }.json()).is_err(), "ni se rechaza después");
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.appointments.len(), appts + 1);
        assert!(b.appointments.iter().any(|x| x.id == appt), "el otro puesto la ve");
        assert_eq!(disk.booking_requests.iter().find(|r| r.id == req).unwrap().state, crate::booking::ReqState::Confirmada);
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn reminders_made_by_two_stations_at_once_are_sent_once() {
        let (addr, path, h) = start("CLAVE11");
        let ra = signed_in(&addr, "CLAVE11", "Recepción 1", 2, "0000");
        let rb = signed_in(&addr, "CLAVE11", "Recepción 2", 1, "0000");
        let (mut a, mut sa) = ra.load().unwrap();
        let (mut b, mut sb) = rb.load().unwrap();
        a.station = ra.station;
        b.station = rb.station;
        let day = crate::model::today();
        // los dos arrancan a la vez y generan los recordatorios y las tareas del día
        let (na, _) = a.generate_messages(day);
        let (nb, _) = b.generate_messages(day);
        let (ta, tb) = (a.generate_tasks(day), b.generate_tasks(day));
        assert!(na > 0 && na == nb && ta == tb, "{na} {nb} {ta} {tb}");
        ra.push(&mut a, &mut sa).unwrap();
        rb.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        let key = |m: &crate::comms::Message| (format!("{:?}", m.kind), m.related, m.patient_id);
        let mut keys: Vec<_> = disk.messages.iter().filter(|m| m.related.is_some()).map(key).collect();
        let n = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), n, "ningún recordatorio repetido");
        let mut autos: Vec<&str> = disk.tasks.iter().filter(|t| !t.auto.is_empty()).map(|t| t.auto.as_str()).collect();
        let n = autos.len();
        autos.sort();
        autos.dedup();
        assert_eq!(autos.len(), n, "ninguna tarea repetida");
        // y el segundo puesto se queda sin los suyos repetidos
        assert_eq!(b.messages.len(), disk.messages.len());
        assert_eq!(b.tasks.len(), disk.tasks.len());
        ra.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
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
