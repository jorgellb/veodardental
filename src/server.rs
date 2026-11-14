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
/// Un puesto con otra versión de los datos no escribe en el servidor: uno
/// anterior borraría lo que no conoce, y uno más nuevo escribiría lo que el
/// servidor no entiende. Cada puesto la manda en esta cabecera.
const DATA_HEADER: &str = "X-Datos";
pub const UPDATE_STATION: &str = "Este puesto tiene una versión de VeodarDental anterior a la del servidor: actualícelo. Lo hecho aquí y no enviado se guarda en este ordenador y se envía al actualizar.";
pub const UPDATE_SERVER: &str = "El servidor de la clínica tiene una versión de VeodarDental anterior a la de este puesto: actualice primero el servidor. Lo hecho aquí y no enviado se guarda en este ordenador.";

/// Bandeja de este ordenador con lo que un usuario no ha podido enviar al
/// servidor (sin conexión, o porque hay que actualizar uno de los dos): así
/// no se pierde al cerrar el programa ni al actualizarlo.
/// - **Una por usuario y servidor:** solo se envía cuando entra ese mismo
///   usuario en ese mismo servidor, con sus permisos, nunca con los de otro.
/// - **Solo para el usuario del sistema:** lleva datos de pacientes.
pub fn outbox_path(user: u32, server_pin: &str) -> PathBuf {
    let pin: String = server_pin.chars().filter(|c| c.is_ascii_hexdigit()).take(16).collect();
    crate::paths::config().join(crate::brand::DIR).join("pendientes").join(format!("usuario{user}_{pin}.json"))
}

/// Lo que no se reenvía desde una bandeja (hay que hacerlo de nuevo, con la
/// sesión abierta): cambios de administración (usuarios y permisos, puestos,
/// conexiones y configuración), por si alguien la hubiera tocado.
pub const NOT_REPLAYED: [&str; 4] = ["users", "stations", "connectors", "config"];

/// Separa lo que se reenvía de la bandeja de lo que no (cuántos).
pub fn replayable(changes: Vec<Change>) -> (Vec<Change>, usize) {
    let n = changes.len();
    let ok: Vec<Change> = changes.into_iter().filter(|c| !NOT_REPLAYED.contains(&c.field.as_str())).collect();
    let skipped = n - ok.len();
    (ok, skipped)
}

/// Lo que hay en la bandeja.
pub fn load_outbox(path: &std::path::Path) -> Vec<Change> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Añade a la bandeja lo que el puesto tiene sin enviar: de cada registro, el
/// «antes» más antiguo y el «ahora» más reciente.
pub fn save_outbox(path: &std::path::Path, changes: &[Change]) -> Result<(), String> {
    let mut all = load_outbox(path);
    for ch in changes {
        match all.iter_mut().find(|x| x.field == ch.field && x.key == ch.key) {
            Some(x) => x.after = ch.after.clone(),
            None => all.push(ch.clone()),
        }
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    crate::paths::write_private(&tmp, serde_json::to_string(&all).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Lo que el puesto tiene cambiado y sin enviar.
pub fn unsent(db: &Db, synced: &Snapshot) -> Vec<Change> {
    station_changes(db, synced)
}

/// ¿Se acepta este cambio de un cobro de quien no es Dirección? (`caja`: tiene
/// permiso de caja). Un cobro nuevo, sí, salvo que saque dinero (en negativo o
/// anulando otro) sin permiso de caja; uno que ya está, no se borra ni cambia
/// lo que es el cobro (importe, paciente, forma de pago, fecha, lo que anula o
/// la cuota que paga); sí, por ejemplo, la caja a la que va.
fn payment_change_ok(current: Option<&String>, after: Option<&String>, caja: bool) -> bool {
    const FIXED: [&str; 6] = ["amount", "patient_id", "method", "date", "cancels", "installment"];
    let parse = |j: &String| serde_json::from_str::<Value>(j).ok();
    match (current.and_then(parse), after.and_then(parse)) {
        // borrarlo, no
        (Some(_), None) => false,
        (None, Some(a)) => caja || (a["amount"].as_f64().is_some_and(|x| x >= 0.0) && a["cancels"].is_null()),
        (Some(c), Some(a)) => FIXED.iter().all(|f| c[*f] == a[*f]),
        // ilegible: no
        (None, None) => after.is_none() && current.is_none(),
    }
}

/// ¿Se acepta este cambio de una financiación de quien no es Dirección? Solo
/// pedirla (nueva, sin nada de dinero); lo demás lo hace el servidor con sus
/// operaciones (`ChainOp::FinStep`, `FinPaid`, `FinCancel`, `FinMatchBank`).
fn financing_change_ok(current: Option<&String>, after: Option<&String>) -> bool {
    match (current, after.and_then(|j| serde_json::from_str::<Value>(j).ok())) {
        (None, Some(a)) => a["state"] == "Solicitada" && ["signed", "paid", "payment_id", "fee_expense", "refund_payment_id", "bank_move"].iter().all(|f| a[*f].is_null()) && a["history"].as_array().is_some_and(|h| h.len() <= 1),
        _ => false,
    }
}

/// Lo único de la configuración que cambia quien no es Dirección: las
/// iniciales en la pizarra y lo propio de su puesto (su usuario y su centro).
/// Todo lo demás (quién es la clínica, IVA, numeración, cobros, remesas,
/// pedidos, encuestas, IA, correo de soporte…) y lo que se añada después,
/// solo Dirección.
const USER_CONFIG: [&str; 3] = ["board_initials", "current_user", "current_center"];

/// ¿Se acepta este cambio de un proveedor de quien no es Dirección? Lo de
/// siempre, sí; activar o quitar el envío automático de pedidos, o cambiar el
/// correo de un proveedor que lo tiene, no (los pedidos irían a otro sitio).
fn supplier_change_ok(current: Option<&String>, after: Option<&String>, others: &[Value]) -> bool {
    let parse = |j: Option<&String>| j.and_then(|j| serde_json::from_str::<Value>(j).ok());
    let (c, a) = (parse(current), parse(after));
    // uno nuevo no repite el NIF ni el correo de otro (no se parte un pedido en dos fichas)
    if c.is_none()
        && let Some(a) = &a
    {
        let norm = |v: &Value, k: &str| v[k].as_str().unwrap_or("").chars().filter(|c| c.is_alphanumeric() || *c == '@' || *c == '.').collect::<String>().to_lowercase();
        let dup = |k: &str| !norm(a, k).is_empty() && others.iter().any(|o| o["id"] != a["id"] && norm(o, k) == norm(a, k));
        if dup("nif") || dup("email") {
            return false;
        }
    }
    let auto = |v: &Option<Value>| v.as_ref().is_some_and(|v| v["auto_send"].as_bool().unwrap_or(false));
    if !auto(&c) && !auto(&a) {
        return true;
    }
    match (c, a) {
        (Some(c), Some(a)) => c["auto_send"] == a["auto_send"] && c["email"] == a["email"],
        _ => false,
    }
}

/// ¿Se acepta este cambio de un pedido de quien no es Dirección? Solo crear,
/// cambiar o borrar un borrador sin nada recibido, aprobado ni enviado; lo
/// demás va por `ChainOp::*Order*` (ver `compras`).
fn order_change_ok(current: Option<&String>, after: Option<&String>) -> bool {
    let draft = |j: &String| {
        serde_json::from_str::<Value>(j).ok().is_some_and(|v| {
            v["state"] == "Borrador" && v["approved"].is_null() && v["sent"].is_null() && v["receipts"].as_array().is_none_or(|r| r.is_empty()) && v["lines"].as_array().is_some_and(|ls| ls.iter().all(|l| l["received"].as_f64().unwrap_or(0.0) == 0.0))
        })
    };
    current.is_none_or(draft) && after.is_none_or(draft) && (current.is_some() || after.is_some())
}

/// ¿Puede escribir en el servidor el puesto de esta petición?
fn data_mismatch(req: &Request) -> Option<&'static str> {
    match req.header(DATA_HEADER).and_then(|v| v.trim().parse::<u32>().ok()) {
        Some(v) if v == crate::model::DATA_VERSION => None,
        Some(v) if v > crate::model::DATA_VERSION => Some(UPDATE_SERVER),
        _ => Some(UPDATE_STATION),
    }
}
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
    /// Registro de accesos a las historias (lo escribe el servidor) y su freno.
    history: PathBuf,
    history_throttle: crate::accesos::Throttle,
    /// Revisión del último cambio de lo que ve algún usuario (sus centros o
    /// su perfil): los puestos que vienen de antes recogen los datos enteros.
    visibility_rev: u64,
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
        let history = crate::accesos::log_file(&path);
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
            history,
            history_throttle: Default::default(),
            visibility_rev: 0,
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
            f.write_all(&crate::datos_cifrados::to_disk(self.snap.to_json().as_bytes())?).map_err(|e| e.to_string())?;
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
        let mut line = crate::datos_cifrados::line_to_disk(&sync::journal_line(rev, &self.snap, touched))?;
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
        let v = self.user(id).map(|u| View::of_user(&u)).unwrap_or(View::of(crate::users::Profile::Recepcion));
        // el registro de jornada de los demás, solo para Dirección
        if self.is_admin(id) { v } else { v.only_own(id) }
    }

    /// Un registro tal como se le manda a un puesto: sin contraseñas y sin lo
    /// que su perfil no ve.
    fn outgoing(view: &View, list: &str, json: &str) -> Option<String> {
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
        // los puestos (y la revisión) los lleva el servidor; los usuarios y las
        // claves de grupo solo los cambia Dirección, y nunca las contraseñas ni
        // los fallos de acceso
        changes.retain(|c| c.field != "stations" && c.field != "rev" && ((c.field != "users" && c.field != "group_keys") || admin));
        // ¿cambia lo que ve algún usuario (sus centros o su perfil)?
        let field = |j: Option<&String>, f: &str| j.and_then(|j| serde_json::from_str::<Value>(j).ok()).map(|v| v[f].clone());
        let visibility = changes.iter().filter(|c| c.field == "users").any(|c| {
            let current = c.key.as_deref().and_then(|k| self.snap.get("users", Some(k)));
            ["centers", "profile"].iter().any(|f| field(current, f) != field(c.after.as_ref(), f))
        });
        for c in changes.iter_mut().filter(|c| c.field == "users") {
            let current = c.key.as_deref().and_then(|k| self.snap.get("users", Some(k))).cloned();
            c.before = c.before.as_deref().map(|b| protect_user_json(b, current.as_ref())).transpose()?;
            c.after = c.after.as_deref().map(|a| protect_user_json(a, current.as_ref())).transpose()?;
        }
        // lo que el perfil no ve: en las listas que no recibe no puede escribir
        // nada (ni añadir: una receta o una imagen las crea quien las ve), y en
        // los demás registros lo que no ve queda como está
        let view = self.view_of(user);
        // de unos centros: en los demás no escribe (ni crea, ni cambia, ni mueve
        // algo de un centro a otro, ni lo borra)
        if view.centers().is_some() {
            let snap = &self.snap;
            changes.retain(|c| {
                let current = c.key.as_deref().and_then(|k| snap.get(&c.field, Some(k)));
                !current.is_some_and(|j| view.other_center(&c.field, j)) && !c.after.as_deref().is_some_and(|j| view.other_center(&c.field, j))
            });
        }
        if !view.is_full() {
            changes.retain(|c| !view.hides(&c.field));
            for c in changes.iter_mut().filter(|c| c.key.is_some()) {
                let current = self.snap.get(&c.field, c.key.as_deref()).cloned();
                c.before = c.before.as_deref().map(|b| view.restore(&c.field, b, current.as_ref())).transpose()?;
                c.after = c.after.as_deref().map(|a| view.restore(&c.field, a, current.as_ref())).transpose()?;
            }
        }
        // campañas: programarlas, cancelarlas, cambiar qué dicen o a quién llegan
        // y borrarlas es cosa de Dirección; los demás puestos solo hacen
        // borradores (`campanas::draft_only`). Los mensajes de una campaña
        // programada los pasa a la cola el propio servidor (`ChainOp::QueueCampaigns`)
        if !admin {
            let snap = &self.snap;
            changes.retain(|c| {
                if c.field != "campaigns" {
                    return true;
                }
                let parse = |j: Option<&String>| j.and_then(|j| serde_json::from_str::<crate::campanas::Campaign>(j).ok());
                let current = parse(c.key.as_deref().and_then(|k| snap.get("campaigns", Some(k))));
                let after = parse(c.after.as_ref());
                (c.after.is_none() || after.is_some()) && crate::campanas::draft_only(current.as_ref(), after.as_ref())
            });
        }
        // dinero: un cobro en negativo o una anulación es sacar dinero, con permiso
        // de caja (como `ChainOp::Refund`); un cobro no se borra ni cambia de
        // importe, paciente, forma de pago o fecha (salvo Dirección); el banco,
        // con su permiso (sus operaciones van por `ChainOp`); la caja y las
        // devoluciones con datáfono, con el de caja; una financiación ya pedida
        // solo cambia por sus operaciones (`ChainOp::Fin*`)
        if !admin {
            let profile = self.user(user).map(|u| u.profile);
            let can = |p: Perm| profile.is_some_and(|x| x.can(p));
            let (caja, gastos) = (can(Perm::Caja), can(Perm::Gastos));
            let snap = &self.snap;
            let suppliers: Vec<Value> = snap.records.get("suppliers").into_iter().flat_map(|r| r.iter()).filter_map(|(_, j)| serde_json::from_str(j).ok()).collect();
            changes.retain(|c| match c.field.as_str() {
                "payments" => payment_change_ok(c.key.as_deref().and_then(|k| snap.get("payments", Some(k))), c.after.as_ref(), caja),
                "bank_moves" | "bank_accounts" => gastos,
                "cash" => caja,
                "card_txs" => caja || ![c.key.as_deref().and_then(|k| snap.get("card_txs", Some(k))), c.after.as_ref()].iter().any(|j| j.is_some_and(|j| j.contains("\"Devolucion\""))),
                "financings" => financing_change_ok(c.key.as_deref().and_then(|k| snap.get("financings", Some(k))), c.after.as_ref()),
                // autorizaciones y liquidaciones: solo por sus operaciones en el
                // servidor (`ChainOp::Auth*` y `Claim*`), que apuntan quién lo hizo
                "auth_requests" | "claims" => false,
                // un pedido: desde el puesto, solo el borrador; lo demás, por sus operaciones
                "orders" => order_change_ok(c.key.as_deref().and_then(|k| snap.get("orders", Some(k))), c.after.as_ref()),
                // el envío automático de pedidos a un proveedor, y su correo mientras lo tenga, solo Dirección
                "suppliers" => supplier_change_ok(c.key.as_deref().and_then(|k| snap.get("suppliers", Some(k))), c.after.as_ref(), &suppliers),
                _ => true,
            });
        }
        // la configuración la cambia Dirección: de otro perfil solo se aceptan los
        // pocos campos de `USER_CONFIG` (lo demás, también lo que se añada, se queda como está)
        if !admin {
            let current = self.snap.get("config", None).and_then(|j| serde_json::from_str::<Value>(j).ok());
            changes.retain(|c| c.field != "config" || (c.after.is_some() && current.is_some()));
            for c in changes.iter_mut().filter(|c| c.field == "config" && c.key.is_none()) {
                if let (Some(cur), Some(Ok(after))) = (current.as_ref(), c.after.as_deref().map(serde_json::from_str::<Value>)) {
                    let mut v = cur.clone();
                    for f in USER_CONFIG {
                        match after.get(f) {
                            Some(x) => v[f] = x.clone(),
                            None => {
                                if let Some(o) = v.as_object_mut() {
                                    o.remove(f);
                                }
                            }
                        }
                    }
                    c.after = Some(sync::canonical("config", &v.to_string())?);
                }
            }
        }
        // un paciente con los datos bloqueados tampoco lo borra otro perfil
        if !admin {
            changes.retain(|c| c.field != "patients" || c.after.is_some() || !c.key.as_deref().and_then(|k| self.snap.get("patients", Some(k))).and_then(|j| serde_json::from_str::<Value>(j).ok()).is_some_and(|v| !v["blocked"].is_null()));
        }
        // registro de jornada (ver `jornada`): solo se añade, nunca se cambia ni se
        // borra; cada uno ficha lo suyo y a la hora del servidor; corrige Dirección
        let now = crate::model::now();
        let me_name = self.user(user).map(|u| u.name).unwrap_or_default();
        let me_station = self.session_station(user).unwrap_or(station);
        // lo que ya hay de este usuario (para comprobar el orden: entrada, pausa…)
        let mut mine: Vec<crate::jornada::Punch> = self.snap.records.get("punches").into_iter().flat_map(|r| r.iter()).filter_map(|(_, j)| serde_json::from_str::<crate::jornada::Punch>(j).ok()).filter(|p| p.user_id == user).collect();
        let snap = &self.snap;
        changes.retain_mut(|c| {
            if c.field != "punches" {
                return true;
            }
            let exists = c.key.as_deref().is_some_and(|k| snap.get("punches", Some(k)).is_some());
            let Some(mut p) = c.after.as_deref().and_then(|a| serde_json::from_str::<crate::jornada::Punch>(a).ok()) else { return false };
            if exists || c.before.is_some() {
                return false;
            }
            let correction = p.corrects.is_some() || !p.reason.is_empty() || p.void;
            if correction {
                if !admin || p.reason.trim().is_empty() {
                    return false;
                }
                // quien corrige es el usuario de la sesión, no lo que diga el puesto
                p.by = me_name.clone();
            } else {
                if p.user_id != user {
                    return false;
                }
                p.at = now;
                p.by.clear();
                p.station = me_station;
                let today = crate::jornada::effective(&mine, user, now.date());
                if !crate::jornada::next_allowed(&today).contains(&p.kind) {
                    return false;
                }
                mine.push(p.clone());
            }
            c.after = serde_json::to_string(&p).ok();
            c.after.is_some()
        });
        // el bloqueo de los datos de un paciente solo lo pone o lo quita Dirección
        // (RGPD; ver `rgpd::Db::set_block`): de otro perfil, se queda como está
        if !admin {
            for c in changes.iter_mut().filter(|c| c.field == "patients" && c.after.is_some()) {
                let current = c.key.as_deref().and_then(|k| self.snap.get("patients", Some(k))).and_then(|j| serde_json::from_str::<Value>(j).ok());
                let keep = current.as_ref().map(|v| v["blocked"].clone()).unwrap_or(Value::Null);
                if let Some(a) = c.after.as_deref()
                    && let Ok(mut v) = serde_json::from_str::<Value>(a)
                    && v["blocked"] != keep
                {
                    if keep.is_null() {
                        if let Some(o) = v.as_object_mut() {
                            o.remove("blocked");
                        }
                    } else {
                        v["blocked"] = keep;
                    }
                    c.after = Some(sync::canonical("patients", &v.to_string())?);
                }
            }
        }
        // lo firmado (documentos, cuestionarios), las recetas emitidas y las
        // notas de evolución confirmadas no se cambian ni se borran ni pasan a
        // otro paciente: solo se anota la revocación o el estado en las
        // farmacias (fusionar pacientes es una operación del servidor,
        // `ChainOp::MergePatients`)
        for c in changes.iter_mut().filter(|c| crate::firmado::PROTECTED_LISTS.contains(&c.field.as_str())) {
            let Some(cur) = c.key.as_deref().and_then(|k| self.snap.get(&c.field, Some(k))).cloned() else { continue };
            if let Some(kept) = crate::firmado::protect_json(&c.field, &cur, c.after.as_deref()) {
                c.after = Some(kept);
            }
        }
        // notas de evolución: quien confirma es el usuario de la sesión, a la
        // hora del servidor (un puesto no puede poner otro nombre)
        let confirmer = self.user(user).filter(|u| u.active && u.profile.can(Perm::Clinico)).map(|u| u.name);
        for c in changes.iter_mut().filter(|c| c.field == "evo_notes") {
            let Some(mut after) = c.after.as_deref().and_then(|a| serde_json::from_str::<crate::evolucion::EvoNote>(a).ok()) else { continue };
            let was = c.key.as_deref().and_then(|k| self.snap.get("evo_notes", Some(k))).and_then(|j| serde_json::from_str::<crate::evolucion::EvoNote>(j).ok());
            if after.is_confirmed() && !was.is_some_and(|w| w.is_confirmed()) {
                match &confirmer {
                    Some(name) => after.stamp(name, crate::model::now()),
                    None => (after.confirmed, after.hash) = (None, String::new()),
                }
                c.after = serde_json::to_string(&after).ok();
            }
        }
        // alertas clínicas revisadas: solo se añaden, por un usuario con
        // permiso clínico, con su nombre y la hora del servidor, de una alerta
        // que existe y de un paciente que existe (las ya guardadas no cambian:
        // `firmado::PROTECTED_LISTS`)
        let mut drop: Vec<Option<String>> = vec![];
        for c in changes.iter_mut().filter(|c| c.field == "alert_acks") {
            if c.key.as_deref().and_then(|k| self.snap.get("alert_acks", Some(k))).is_some() {
                continue;
            }
            let Some(mut ack) = c.after.as_deref().and_then(|a| serde_json::from_str::<crate::alertas::AlertAck>(a).ok()) else {
                drop.push(c.key.clone());
                continue;
            };
            let known = crate::alertas::RULES.iter().any(|r| r.key == ack.rule) && self.snap.get("patients", Some(&ack.patient_id.to_string())).is_some();
            match (&confirmer, known) {
                (Some(name), true) => {
                    (ack.by, ack.at) = (name.clone(), crate::model::now());
                    c.after = serde_json::to_string(&ack).ok();
                }
                _ => drop.push(c.key.clone()),
            }
        }
        // esterilización (ver `esterilizacion`): el servidor comprueba cada
        // paso con lo que tiene él, no con lo que diga el puesto
        {
            use crate::esterilizacion::{Cycle, CycleState, Pack};
            let who = self.user(user).filter(|u| u.active).map(|u| u.name);
            let clinical = self.user(user).is_some_and(|u| u.active && u.profile.can(Perm::Clinico));
            let today = crate::model::today();
            let snap_cycle = |id: u32| self.snap.get("sterile_cycles", Some(&id.to_string())).and_then(|j| serde_json::from_str::<Cycle>(j).ok());
            // ciclos: nuevo en curso, sin indicadores y con quién; validar con
            // permiso clínico e indicadores correctos; fallido con su motivo;
            // indicadores puestos y fallido, para siempre; nada se borra
            for c in changes.iter_mut().filter(|c| c.field == "sterile_cycles") {
                let cur = c.key.as_deref().and_then(|k| k.parse().ok()).and_then(snap_cycle);
                let after = c.after.as_deref().and_then(|a| serde_json::from_str::<Cycle>(a).ok());
                let fixed = match (cur, after) {
                    (Some(cur), None) => Some(cur),
                    (Some(cur), Some(_)) if cur.state == CycleState::Fallido => Some(cur),
                    (Some(cur), Some(mut a)) => {
                        (a.autoclave, a.number, a.program, a.started, a.by) = (cur.autoclave.clone(), cur.number, cur.program.clone(), cur.started, cur.by.clone());
                        for (now_, was) in [(&mut a.chemical, cur.chemical), (&mut a.bowie_dick, cur.bowie_dick), (&mut a.biological, cur.biological)] {
                            if was.is_some() {
                                *now_ = was;
                            }
                        }
                        let ok = match (cur.state, a.state) {
                            (x, y) if x == y => true,
                            (CycleState::EnCurso, CycleState::Validado) => clinical && a.chemical == Some(true) && a.bowie_dick != Some(false),
                            (_, CycleState::Fallido) => !a.incident.trim().is_empty(),
                            _ => false,
                        };
                        if !ok {
                            (a.state, a.incident) = (cur.state, cur.incident.clone());
                        }
                        Some(a)
                    }
                    (None, Some(mut a)) => {
                        (a.state, a.chemical, a.biological, a.bowie_dick) = (CycleState::EnCurso, None, None, None);
                        a.incident.clear();
                        a.by = who.clone().unwrap_or_default();
                        Some(a)
                    }
                    (None, None) => None,
                };
                if let Some(f) = fixed {
                    c.after = serde_json::to_string(&f).ok();
                }
            }
            // los ciclos que esta tanda crea, para sus lotes nuevos
            let new_cycles: Vec<u32> = changes.iter().filter(|c| c.field == "sterile_cycles" && c.key.as_deref().and_then(|k| self.snap.get("sterile_cycles", Some(k))).is_none()).filter_map(|c| c.key.as_deref()?.parse().ok()).collect();
            let mut bad_packs: Vec<Option<String>> = vec![];
            let mut new_codes: Vec<String> = vec![];
            for c in changes.iter_mut().filter(|c| c.field == "sterile_packs") {
                let cur = c.key.as_deref().and_then(|k| self.snap.get("sterile_packs", Some(k))).and_then(|j| serde_json::from_str::<Pack>(j).ok());
                let after = c.after.as_deref().and_then(|a| serde_json::from_str::<Pack>(a).ok());
                let fixed = match (cur, after) {
                    // un lote no se borra
                    (Some(cur), None) => Some(cur),
                    (Some(cur), Some(mut a)) => {
                        (a.code, a.cycle_id, a.contents, a.expires) = (cur.code.clone(), cur.cycle_id, cur.contents.clone(), cur.expires);
                        if cur.used.is_some() {
                            a.used = cur.used.clone();
                        } else if let Some(u) = a.used.as_mut() {
                            // el uso, con lo que sabe el servidor: ciclo validado, sin caducar, sin retirar, paciente que existe
                            let usable = clinical && cur.withdrawn.is_none() && cur.expires >= today && snap_cycle(cur.cycle_id).is_some_and(|x| x.state == CycleState::Validado) && self.snap.get("patients", Some(&u.patient_id.to_string())).is_some();
                            match (&who, usable) {
                                (Some(n), true) => (u.by, u.at) = (n.clone(), crate::model::now()),
                                _ => a.used = None,
                            }
                        }
                        if cur.withdrawn.is_some() {
                            a.withdrawn = cur.withdrawn.clone();
                        }
                        Some(a)
                    }
                    // lote nuevo: de un ciclo en curso, sin usar ni retirar, con etiqueta única
                    (None, Some(mut a)) => {
                        let cycle_ok = new_cycles.contains(&a.cycle_id) || snap_cycle(a.cycle_id).is_some_and(|x| x.state == CycleState::EnCurso);
                        // la misma regla en las dos comprobaciones: sin espacios y sin distinguir mayúsculas
                        let norm = |s: &str| s.trim().to_ascii_lowercase();
                        let code = norm(&a.code);
                        let unique = !code.is_empty() && !new_codes.contains(&code) && !self.snap.records.get("sterile_packs").is_some_and(|r| r.iter().any(|(_, j)| serde_json::from_str::<Pack>(j).is_ok_and(|p| norm(&p.code) == code)));
                        if cycle_ok && unique {
                            new_codes.push(code);
                            (a.used, a.withdrawn) = (None, None);
                            Some(a)
                        } else {
                            bad_packs.push(c.key.clone());
                            None
                        }
                    }
                    (None, None) => None,
                };
                if let Some(f) = fixed {
                    c.after = serde_json::to_string(&f).ok();
                }
            }
            changes.retain(|c| !(c.field == "sterile_packs" && bad_packs.contains(&c.key)));
        }
        changes.retain(|c| !(c.field == "alert_acks" && drop.contains(&c.key)));
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
        // de quién es cada historia que se cambia (para el registro de accesos),
        // con lo que había y lo que queda en el servidor, no con lo que diga el puesto
        let server_side: Vec<Change> =
            touched.iter().map(|(f, k)| Change { field: f.clone(), key: k.clone(), before: previous.iter().find(|p| &p.0 == f && &p.1 == k).and_then(|p| p.2.clone()), after: self.snap.get(f, k.as_deref()).cloned() }).collect();
        let history = crate::accesos::patients_touched(&server_side);
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
        if visibility {
            self.visibility_rev = self.rev();
        }
        let station = self.session_station(user).unwrap_or(station);
        self.note_history(user, station, crate::accesos::Action::Modificacion, history);
        Ok(sent)
    }

    /// Apunta en el registro de accesos lo que hizo `user` en cada historia.
    fn note_history(&mut self, user: u32, station: u32, action: crate::accesos::Action, parts: std::collections::BTreeMap<u32, std::collections::BTreeSet<String>>) {
        let name = self.user(user).map(|u| u.name).unwrap_or_else(|| format!("usuario {user}"));
        for (patient, ctx) in parts {
            let context = ctx.into_iter().collect::<Vec<_>>().join(", ");
            let e = crate::accesos::Entry { at: crate::model::now(), user_id: user, user: name.clone(), station, patient, action, context, prev: String::new() };
            if let Err(err) = crate::accesos::append(&self.history, e) {
                eprintln!("{err}");
            }
        }
    }

    /// Un puesto avisa de que su usuario consulta una historia: el servidor lo
    /// apunta con el usuario de la sesión (no con el que diga el puesto).
    /// Puesto de la sesión abierta de un usuario (el que quedó al entrar, no el
    /// que diga cada petición).
    fn session_station(&self, user: u32) -> Option<u32> {
        self.sessions.values().filter(|s| s.user == user).max_by_key(|s| s.last_used).map(|s| s.station)
    }

    fn history_access(&mut self, req: &Request, me: u32) -> Response {
        let station = req.header("X-Sesion").map(token_hash).and_then(|h| self.sessions.get(&h)).map(|s| s.station).unwrap_or(0);
        let v: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
        let Some(patient) = v["paciente"].as_u64().map(|n| n as u32) else { return error(400, "Falta «paciente».") };
        if self.snap.get("patients", Some(&patient.to_string())).is_none() {
            return error(404, "Ese paciente no existe.");
        }
        let context: String = v["contexto"].as_str().unwrap_or("Ficha").chars().filter(|c| !c.is_control()).take(60).collect();
        if self.history_throttle.fresh(me, patient, &context) {
            self.note_history(me, station, crate::accesos::Action::Consulta, [(patient, [context].into_iter().collect())].into_iter().collect());
        }
        reply(200, json!({ "ok": true }))
    }

    /// El registro de accesos de una historia (o de todas): solo Dirección.
    fn history_read(&self, req: &Request, me: u32) -> Response {
        if !self.is_admin(me) {
            return error(403, "Solo Dirección puede ver el registro de accesos a las historias.");
        }
        let patient = req.param("paciente").and_then(|p| p.parse::<u32>().ok());
        let list: Vec<crate::accesos::Entry> = crate::accesos::read(&self.history, patient).into_iter().take(2000).collect();
        let integrity = match crate::accesos::verify(&self.history) {
            Ok(n) => format!("Íntegro ({n} entradas)"),
            Err(e) => e,
        };
        reply(200, json!({ "accesos": list, "integridad": integrity }))
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
        // ha cambiado lo que ve algún usuario: los datos enteros, con lo que ve ahora
        if since < rev && (since < self.visibility_rev || self.log.front().is_none_or(|(first, _)| *first > since + 1)) {
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
                (Some(rec), Some(_)) => Self::outgoing(&view, field, rec),
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
        reply(200, json!({ "clinica": name, "usuarios": users, "rev": self.rev(), "desfase": offset, "datos": crate::model::DATA_VERSION, "version": env!("CARGO_PKG_VERSION") }))
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
                    Some(Perm::Gastos) => "Este usuario no puede llevar los gastos ni el banco (por ejemplo, aprobar pedidos o conciliar).",
                    Some(Perm::Caja) => "Este usuario no puede llevar la caja (anular cobros o devolver dinero).",
                    Some(Perm::Configuracion) => "Solo Dirección puede hacerlo (por ejemplo, fusionar pacientes).",
                    _ => "Este usuario no tiene permiso para hacerlo.",
                },
            );
        };
        let center = v["centro"].as_u64().map(|c| c as u32);
        // de unos centros: solo en los suyos, y no con lo de otro centro
        let view = self.view_of(me);
        if view.centers().is_some() {
            if !center.is_some_and(|c| view.center_ok(c)) {
                return error(403, "Este usuario no trabaja en ese centro.");
            }
            if let Some((list, id)) = op.center_record()
                && self.snap.get(list, Some(&id.to_string())).is_some_and(|j| view.other_center(list, j))
            {
                return error(403, "Es de un centro en el que este usuario no trabaja.");
            }
        }
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

    /// Resumen de esta clínica para el grupo (solo cifras; ver `grupo`), con
    /// una clave de grupo en uso.
    fn group_summary(&self, req: &Request) -> Response {
        let token = req.header("X-Grupo").unwrap_or_default();
        let keys: Vec<crate::grupo::GroupKey> = self.snap.records.get("group_keys").into_iter().flat_map(|r| r.iter()).filter_map(|(_, j)| serde_json::from_str(j).ok()).collect();
        if crate::grupo::valid_key(&keys, token).is_none() {
            return error(401, "La clave de grupo no es válida o se ha retirado.");
        }
        let date = |k: &str| req.param(k).and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
        let (Some(from), Some(to)) = (date("desde"), date("hasta")) else { return error(400, "Faltan las fechas (desde y hasta, AAAA-MM-DD).") };
        if let Err(e) = crate::grupo::check_period(from, to) {
            return error(400, &e);
        }
        match self.snap.to_db() {
            Ok(db) => reply(200, json!(db.group_summary(from, to))),
            Err(e) => error(500, &e),
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
        // el resumen para el grupo de clínicas va con su propia clave (ver `grupo`),
        // no con la de la clínica, y no abre nada más
        if req.path == "/grupo/resumen" {
            if req.method != "GET" {
                return (error(405, "Solo se puede pedir el resumen."), false);
            }
            let r = self.group_summary(req);
            if r.code == 401 {
                hits(&mut self.key_fails, req.peer, window, true);
            }
            return (r, false);
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
                        Response { code: 200, body: self.snap.to_json_with(&|list, rec| Self::outgoing(&view, list, rec)) }
                    }
                    ("GET", "/cambios") => self.since(param("desde"), &[], self.view_of(me)),
                    ("POST", "/cambios" | "/cadena") if data_mismatch(req).is_some() => error(426, data_mismatch(req).unwrap_or(UPDATE_STATION)),
                    ("POST", "/cambios") => match serde_json::from_slice::<PushBody>(&req.body).map_err(|e| e.to_string()).and_then(|b| self.save(b.changes, station, me)) {
                        Ok(sent) => self.since(param("base"), &sent, self.view_of(me)),
                        Err(e) => error(400, &e),
                    },
                    ("POST", "/cadena") => self.chain_op(req, me),
                    ("POST", "/historia") => self.history_access(req, me),
                    ("GET", "/historia") => self.history_read(req, me),
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
/// Cómo arranca el servidor sin ventana (`--servidor`).
#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    /// Otro puerto que el de `servidor.json` (`--puerto`).
    pub port: Option<u16>,
    /// Sin datos de la clínica, empezar con los de ejemplo (solo para ensayos:
    /// `--datos-de-ejemplo`). Si no, sin datos no arranca.
    pub sample_ok: bool,
}

/// Sin datos de la clínica en este ordenador, el servidor no arranca: en un
/// servidor nuevo (en la nube) hay que llevar antes los datos.
pub const NO_DATA: &str = "No hay datos de la clínica en este ordenador. Lleve antes los datos de la clínica (ver docs/despliegue_nube.md) o, para un ensayo, arranque con --datos-de-ejemplo.";

pub fn run(opts: &RunOptions) -> Result<(), String> {
    use std::io::IsTerminal;
    let mut cfg = ensure_server_config()?;
    if let Some(p) = opts.port {
        cfg.port = p;
    }
    let identity = cfg.identity().ok_or("La identidad del servidor está dañada: borre servidor.json y vuelva a conectar los puestos.")?;
    if !Db::path().exists() && !opts.sample_ok {
        return Err(NO_DATA.into());
    }
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
    // la clave solo en una terminal: como servicio, la salida va al registro del sistema
    if std::io::stdout().is_terminal() {
        println!("  Clave de la clínica: {}", cfg.key);
    } else {
        println!("  Clave de la clínica: no se escribe aquí (véala en este ordenador con «veodardental --clave-servidor»)");
    }
    println!("  Huella del servidor: {} (la misma que ven los puestos)", fingerprint(&cfg.pin()));
    // las fechas de cobros, facturas e informes son las de la hora local del servidor
    let offset = chrono::Local::now().offset().local_minus_utc();
    println!("  Hora del servidor: UTC{:+}:{:02} (TZ={}); tiene que ser la de la clínica", offset / 3600, (offset.abs() % 3600) / 60, std::env::var("TZ").unwrap_or_else(|_| "la del sistema".into()));
    match crate::disco::check_now() {
        crate::disco::Disk::Plain(why) => println!("  ⚠ {why} Los datos de salud de los pacientes están en este disco."),
        crate::disco::Disk::Unknown(why) => println!("  ⚠ Cifrado del disco: {why}"),
        _ => {}
    }
    serve(listener, path, cfg.key, identity, db);
    Ok(())
}

/// Variable con la clave de la clínica para `--estado-servidor` (la clave
/// nunca va en la orden: se vería en la lista de procesos).
pub const KEY_VAR: &str = "VEODARDENTAL_CLAVE";

/// `--estado-servidor DIRECCIÓN [HUELLA]` (la clave, en [`KEY_VAR`] o por la
/// entrada): comprueba desde cualquier ordenador que el servidor responde,
/// con la prueba de identidad (o la huella, si se da) y la clave de la
/// clínica. Para vigilarlo en la nube.
pub fn status_cli(addr: &str, key: &str, pin: &str) -> Result<String, String> {
    let r = Remote::with_pin(addr, key, 0, pin);
    let info = r.access()?;
    let seen = r.pinned();
    let offset = info.utc_offset.map(|o| format!("UTC{:+}:{:02}", o / 3600, (o.abs() % 3600) / 60)).unwrap_or_else(|| "—".into());
    Ok(format!(
        "Servidor de {} · versión {} · datos {} · hora {} · huella {}",
        info.clinic,
        info.version.as_deref().unwrap_or("—"),
        info.data_version.map(|v| v.to_string()).unwrap_or_else(|| "—".into()),
        offset,
        fingerprint(if seen.is_empty() { pin } else { &seen })
    ))
}

/// `--parar-servidor [DIRECCIÓN]`: para el servidor de este ordenador (solo
/// desde el propio ordenador; con la clave de su `servidor.json`).
pub fn stop_cli(addr: Option<&str>) -> Result<(), String> {
    let cfg = server_config();
    if cfg.key.is_empty() {
        return Err("En este ordenador no hay servidor configurado.".into());
    }
    let addr = addr.map(|a| a.to_string()).unwrap_or_else(|| format!("127.0.0.1:{}", cfg.port));
    Remote::with_pin(&addr, &cfg.key, 0, &cfg.pin()).stop()
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
    /// Versión de los datos del servidor (no la dan las versiones anteriores).
    #[serde(rename = "datos", default)]
    pub data_version: Option<u32>,
    /// Versión del programa en el servidor.
    #[serde(default)]
    pub version: Option<String>,
}

impl AccessInfo {
    /// Si este puesto y el servidor no pueden trabajar juntos, qué hay que actualizar.
    pub fn mismatch(&self) -> Option<&'static str> {
        match self.data_version {
            Some(v) if v == crate::model::DATA_VERSION => None,
            Some(v) if v > crate::model::DATA_VERSION => Some(UPDATE_STATION),
            _ => Some(UPDATE_SERVER),
        }
    }
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
        let mut r = self.agent.request(method, &format!("{}{}", self.base, path)).set("X-Clave", &self.key).set("X-Puesto", &self.station.to_string()).set(DATA_HEADER, &crate::model::DATA_VERSION.to_string());
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

    /// Avisa al servidor de que el usuario de la sesión consulta una historia.
    pub fn note_access(&self, patient: u32, context: &str) -> Result<(), String> {
        self.post("/historia", &json!({ "paciente": patient, "contexto": context }).to_string()).map(|_| ())
    }

    /// Registro de accesos a una historia (solo Dirección) y su integridad.
    pub fn history_log(&self, patient: u32) -> Result<(Vec<crate::accesos::Entry>, String), String> {
        let v = parse(&self.get(&format!("/historia?paciente={patient}"))?)?;
        let list = serde_json::from_value(v["accesos"].clone()).map_err(|e| format!("Respuesta no válida del servidor: {e}"))?;
        Ok((list, v["integridad"].as_str().unwrap_or("").to_string()))
    }

    /// Envía cambios guardados en la bandeja de este ordenador (de una sesión
    /// anterior que no pudo enviarlos) y recoge lo que devuelve el servidor.
    /// El servidor los combina con lo que tenga como cualquier otro cambio.
    pub fn push_changes(&self, db: &mut Db, synced: &mut Snapshot, changes: &[Change]) -> Result<usize, String> {
        if changes.is_empty() {
            return Ok(0);
        }
        let body = json!({ "cambios": changes }).to_string();
        let delta: Delta = serde_json::from_str(&self.post(&format!("/cambios?base={}", db.rev), &body)?).map_err(|e| format!("Respuesta no válida del servidor: {e}"))?;
        self.note_backup(&delta);
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

/// Pide a una clínica del grupo su resumen (ver `grupo`): solo con su huella
/// ya conocida (si no coincide, no conecta) y con la clave de grupo, sin la
/// clave de la clínica.
pub fn group_summary(link: &crate::grupo::Link, from: chrono::NaiveDate, to: chrono::NaiveDate) -> Result<crate::grupo::Summary, String> {
    if unhex(&link.pin).is_none_or(|p| p.len() != 32) {
        return Err("Falta la huella del servidor de esa clínica.".into());
    }
    crate::grupo::check_period(from, to)?;
    let r = Remote::with_pin(&link.address, "", 0, &link.pin);
    let url = format!("{}/grupo/resumen?desde={}&hasta={}", r.base, from.format("%Y-%m-%d"), to.format("%Y-%m-%d"));
    let text = read_all(r.agent.get(&url).set("X-Grupo", link.token.trim()).call().map_err(err_text)?)?;
    serde_json::from_str(&text).map_err(|e| format!("Respuesta no válida de la clínica: {e}"))
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
                let info = r.access()?;
                return match info.mismatch() {
                    Some(e) => Err(format!("{e} (servidor: {}; este puesto: {})", info.version.as_deref().unwrap_or("versión anterior"), env!("CARGO_PKG_VERSION"))),
                    None => Ok(info),
                };
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

    /// T-F3-02. Un puesto con otra versión de los datos no escribe en el
    /// servidor, y el mensaje dice qué hay que actualizar; al conectar ya se
    /// sabe. Leer sí puede (para ver lo que hay mientras se actualiza).
    #[test]
    fn stations_and_server_with_other_data_versions_do_not_write_to_each_other() {
        use crate::model::DATA_VERSION;
        let dir = std::env::temp_dir().join(format!("ds_versiones_{}", std::process::id()));
        let mut st = State::new(&Db::sample(crate::model::today()), dir.join("clinica.json"), "CLAVEV", vec![7; 32]);
        let token = st.new_session(1, 3);
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let req = |method: &str, path: &str, data: Option<String>| {
            let mut headers = vec![("X-Clave".to_string(), "CLAVEV".to_string()), ("X-Sesion".to_string(), token.clone())];
            if let Some(d) = data {
                headers.push((DATA_HEADER.to_string(), d));
            }
            Request { method: method.into(), path: path.into(), query: String::new(), headers, body: b"{\"cambios\": []}".to_vec(), peer }
        };
        for (data, want) in [(None, Some(UPDATE_STATION)), (Some("0".to_string()), Some(UPDATE_STATION)), (Some((DATA_VERSION + 1).to_string()), Some(UPDATE_SERVER)), (Some(DATA_VERSION.to_string()), None)] {
            for path in ["/cambios", "/cadena"] {
                let (resp, _) = st.handle(&req("POST", path, data.clone()), "CLAVEV");
                match want {
                    Some(msg) => assert_eq!((resp.code, resp.body.contains(msg)), (426, true), "{path} {data:?}: {}", resp.body),
                    None => assert_ne!(resp.code, 426, "{path}: {}", resp.body),
                }
            }
            let (resp, _) = st.handle(&req("GET", "/datos", data.clone()), "CLAVEV");
            assert_eq!(resp.code, 200, "leer, sí");
        }
        // al conectar: lo que dice el servidor
        let (resp, _) = st.handle(&req("GET", "/acceso", None), "CLAVEV");
        let info: AccessInfo = serde_json::from_str(&resp.body).unwrap();
        assert_eq!((info.data_version, info.version.as_deref(), info.mismatch()), (Some(DATA_VERSION), Some(env!("CARGO_PKG_VERSION")), None));
        let with = |v: serde_json::Value| serde_json::from_value::<AccessInfo>(v).unwrap().mismatch();
        assert_eq!(with(json!({ "datos": DATA_VERSION + 1 })), Some(UPDATE_STATION));
        assert_eq!(with(json!({ "datos": 0 })), Some(UPDATE_SERVER));
        assert_eq!(with(json!({})), Some(UPDATE_SERVER), "un servidor que no la dice es anterior");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// T-F3-02. Lo que un puesto no pudo enviar queda en la bandeja de su
    /// ordenador (solo para el usuario), con el primer «antes» y el último
    /// «ahora» de cada registro; en la sesión siguiente se envía, y enviarlo
    /// otra vez no cambia nada.
    #[test]
    fn unsent_changes_survive_in_the_outbox_and_reach_the_server_later() {
        let (addr, path, h) = start("CLAVE_BANDEJA");
        let a = signed_in(&addr, "CLAVE_BANDEJA", "Dirección", 1, "0000");
        let (mut db, synced) = a.load().unwrap();
        let pid = db.patients[0].id;
        let outbox = path.with_file_name("pendientes.json");
        db.patients.iter_mut().find(|p| p.id == pid).unwrap().notes = "primera nota".into();
        save_outbox(&outbox, &unsent(&db, &synced)).unwrap();
        db.patients.iter_mut().find(|p| p.id == pid).unwrap().notes = "nota definitiva".into();
        db.appointments[0].notes = "cita cambiada".into();
        save_outbox(&outbox, &unsent(&db, &synced)).unwrap();
        let kept = load_outbox(&outbox);
        assert_eq!(kept.len(), 2, "un cambio por registro: {kept:?}");
        assert!(kept.iter().any(|c| c.field == "patients" && c.after.as_deref().is_some_and(|x| x.contains("nota definitiva")) && c.before.as_deref().is_some_and(|x| !x.contains("nota"))));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&outbox).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // cada usuario (y servidor), su bandeja; lo de administración no se reenvía
        assert_ne!(outbox_path(1, "aa"), outbox_path(2, "aa"));
        assert_ne!(outbox_path(1, "aa"), outbox_path(1, "bb"));
        let admin = Change { field: "users".into(), key: Some("1".into()), before: None, after: Some("{}".into()) };
        let (ok, skipped) = replayable(kept.iter().cloned().chain([admin]).collect());
        assert_eq!((ok.len(), skipped), (2, 1));
        // otra sesión (el programa se cerró y se volvió a abrir): se envía
        let b = signed_in(&addr, "CLAVE_BANDEJA", "Dirección", 1, "0000");
        let (mut db2, mut s2) = b.load().unwrap();
        assert_eq!(b.push_changes(&mut db2, &mut s2, &kept).unwrap(), 2);
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.patient(pid).unwrap().notes, "nota definitiva");
        assert_eq!(disk.appointments[0].notes, "cita cambiada");
        assert_eq!(db2.patient(pid).unwrap().notes, "nota definitiva", "y el puesto lo tiene");
        // enviarla otra vez (si no llegó a borrarse) no cambia nada
        let before = serde_json::to_string(&crate::sync::on_disk(&path).patients).unwrap();
        b.push_changes(&mut db2, &mut s2, &kept).unwrap();
        assert_eq!(serde_json::to_string(&crate::sync::on_disk(&path).patients).unwrap(), before);
        a.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// T-F4-02. En red, el servidor apunta quién consulta y quién modifica
    /// cada historia, con el usuario de la sesión (no el que diga el puesto) y
    /// sin copiar la historia. Solo Dirección lo lee, y no va con los datos.
    #[test]
    fn the_server_records_who_reads_and_changes_each_history() {
        let (addr, path, h) = start("CLAVE_HIST");
        let rec = signed_in(&addr, "CLAVE_HIST", "Recepción", 2, "0000");
        let dir = signed_in(&addr, "CLAVE_HIST", "Dirección", 1, "0000");
        // consulta (el puesto intenta decir que es otro usuario: no cuenta)
        rec.post("/historia", &json!({ "paciente": 101, "contexto": "Citas", "usuario": "Dirección" }).to_string()).unwrap();
        rec.note_access(101, "Citas").unwrap();
        assert!(rec.note_access(999_999, "Ficha").is_err(), "paciente que no existe");
        // modificación de un tratamiento del 101 por Dirección
        let (mut d, mut sd) = dir.load().unwrap();
        let secret = "Nota clínica que no debe salir al registro";
        let e = d.entries.iter_mut().find(|e| e.patient_id == 101).unwrap();
        e.notes = secret.into();
        dir.push(&mut d, &mut sd).unwrap();
        // el puesto que dice ser otro (cabecera X-Puesto) queda con el de su sesión
        let mut liar = rec.clone();
        liar.station = 99;
        liar.note_access(102, "Recetas").unwrap();
        // un cambio con un «antes» falso que nombra a otro paciente: cuenta lo del servidor
        let target = d.entries.iter().find(|e| e.patient_id == 101).unwrap().clone();
        let mut fake_before = serde_json::to_value(&target).unwrap();
        fake_before["patient_id"] = json!(103);
        let mut after = target.clone();
        after.notes = "otra nota".into();
        let body = json!({ "cambios": [{ "campo": "entries", "clave": target.id.to_string(), "antes": fake_before.to_string(), "ahora": serde_json::to_string(&after).unwrap() }] });
        dir.post(&format!("/cambios?base={}", d.rev), &body.to_string()).unwrap();
        let (l103, _) = dir.history_log(103).unwrap();
        assert!(l103.is_empty(), "no se atribuye a otro paciente: {l103:?}");
        let (l102, _) = dir.history_log(102).unwrap();
        assert!(l102.iter().all(|e| e.station != 99 && e.station == rec.station), "{l102:?}");
        // solo Dirección lo lee
        assert!(rec.history_log(101).unwrap_err().contains("Solo Dirección"));
        let (list, integrity) = dir.history_log(101).unwrap();
        assert!(integrity.starts_with("Íntegro"), "{integrity}");
        let reads: Vec<&crate::accesos::Entry> = list.iter().filter(|e| e.action == crate::accesos::Action::Consulta).collect();
        assert_eq!(reads.len(), 1, "la consulta repetida no se apunta dos veces: {list:?}");
        assert_eq!((reads[0].user.as_str(), reads[0].user_id, reads[0].context.as_str()), ("Recepción", 2, "Citas"));
        assert!(list.iter().any(|e| e.action == crate::accesos::Action::Modificacion && e.user_id == 1 && e.context == "tratamientos"), "{list:?}");
        assert_eq!(list.iter().filter(|e| e.action == crate::accesos::Action::Modificacion).count(), 2, "las dos modificaciones del 101: {list:?}");
        // sin copiar la historia, y fuera de los datos que reciben los puestos
        let file = std::fs::read_to_string(crate::accesos::log_file(&path)).unwrap();
        assert!(!file.contains(secret));
        assert!(!rec.get("/datos").unwrap().contains("accesos_historias") && !rec.get("/datos").unwrap().contains("\"Consulta\""));
        assert_eq!(crate::accesos::verify(&crate::accesos::log_file(&path)).unwrap(), list.len() + l102.len(), "las del 101 y la del 102");
        rec.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// T-F4-03. El bloqueo de los datos de un paciente solo lo pone o lo quita
    /// Dirección, también en red (el servidor lo hace cumplir).
    #[test]
    fn only_management_blocks_or_unblocks_a_patient_on_the_server() {
        let (addr, path, h) = start("CLAVE_BLOQ");
        let dir = signed_in(&addr, "CLAVE_BLOQ", "Dirección", 1, "0000");
        let rec = signed_in(&addr, "CLAVE_BLOQ", "Recepción", 2, "0000");
        let (mut d, mut sd) = dir.load().unwrap();
        d.set_block(101, Some("Se conserva por obligación legal"), true).unwrap();
        dir.push(&mut d, &mut sd).unwrap();
        assert!(crate::sync::on_disk(&path).patient(101).unwrap().blocked.is_some());
        // Recepción intenta quitarlo (y bloquear a otro): no cambia nada de eso
        let (mut r, mut sr) = rec.load().unwrap();
        r.patients.iter_mut().find(|p| p.id == 101).unwrap().blocked = None;
        r.patients.iter_mut().find(|p| p.id == 101).unwrap().mobile = "600999888".into();
        r.patients.iter_mut().find(|p| p.id == 102).unwrap().blocked = Some(crate::rgpd::Mark { since: crate::model::now(), reason: "x".into(), by: "Recepción".into() });
        rec.push(&mut r, &mut sr).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert!(disk.patient(101).unwrap().blocked.is_some(), "sigue bloqueado");
        assert_eq!(disk.patient(101).unwrap().mobile, "600999888", "lo demás sí cambia");
        assert!(disk.patient(102).unwrap().blocked.is_none());
        // Dirección sí lo quita
        let (mut d, mut sd) = dir.load().unwrap();
        d.set_block(101, None, true).unwrap();
        dir.push(&mut d, &mut sd).unwrap();
        assert!(crate::sync::on_disk(&path).patient(101).unwrap().blocked.is_none());
        dir.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// T-F4-06. En red, el registro de jornada solo crece: cada uno ficha lo
    /// suyo y a la hora del servidor, nadie cambia ni borra una marca, y las
    /// correcciones (con motivo) son de Dirección. Un paciente bloqueado
    /// tampoco lo borra otro perfil.
    #[test]
    fn the_time_record_only_grows_on_the_server() {
        use crate::jornada::{Kind, Punch};
        let (addr, path, h) = start("CLAVE_JORN");
        let rec = signed_in(&addr, "CLAVE_JORN", "Recepción", 2, "0000");
        let dir = signed_in(&addr, "CLAVE_JORN", "Dirección", 1, "0000");
        let (mut r, mut sr) = rec.load().unwrap();
        let early = crate::model::now() - chrono::Duration::hours(5);
        // ficha con la hora cambiada en su ordenador: vale la del servidor
        r.punch(2, Kind::Entrada, early).unwrap();
        // y por otra persona, y una corrección sin ser Dirección: no
        let (a, b) = (r.new_id(), r.new_id());
        r.punches.push(Punch { id: a, user_id: 3, at: early, kind: Kind::Entrada, station: 0, corrects: None, reason: String::new(), by: String::new(), void: false });
        r.punches.push(Punch { id: b, user_id: 2, at: early, kind: Kind::Salida, station: 0, corrects: None, reason: "me fui antes".into(), by: "Recepción".into(), void: false });
        rec.push(&mut r, &mut sr).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.punches.len(), 1, "{:?}", disk.punches);
        let mine = disk.punches[0].clone();
        assert!((crate::model::now() - mine.at).num_minutes().abs() < 5, "hora del servidor, no la del puesto");
        assert_eq!(mine.station, rec.station, "el puesto, el de la sesión");
        // una segunda entrada seguida: no (el orden lo comprueba el servidor)
        let (mut r, mut sr) = rec.load().unwrap();
        let c = r.new_id();
        r.punches.push(Punch { id: c, user_id: 2, at: early, kind: Kind::Entrada, station: 0, corrects: None, reason: String::new(), by: String::new(), void: false });
        rec.push(&mut r, &mut sr).unwrap();
        assert_eq!(crate::sync::on_disk(&path).punches.len(), 1);
        // cambiarla o borrarla: no
        let (mut r, mut sr) = rec.load().unwrap();
        r.punches.iter_mut().for_each(|p| p.at = early);
        rec.push(&mut r, &mut sr).unwrap();
        r.punches.clear();
        rec.push(&mut r, &mut sr).unwrap();
        assert_eq!(crate::sync::on_disk(&path).punches.to_vec(), vec![mine.clone()]);
        // Dirección corrige, con motivo
        let (mut d, mut sd) = dir.load().unwrap();
        d.correct_punch(Some(mine.id), 2, Some(Kind::Entrada), early, "Llegó a las 9; el ordenador estaba apagado", true).unwrap();
        dir.push(&mut d, &mut sd).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.punches.len(), 2);
        assert!(disk.punches.iter().any(|p| p.corrects == Some(mine.id) && p.at == early && p.by == "Dirección"), "{:?}", disk.punches);
        // cada uno recibe solo su registro de jornada; Dirección, el de todos
        let (r2, _) = rec.load().unwrap();
        d.punch(1, Kind::Entrada, crate::model::now()).unwrap();
        dir.push(&mut d, &mut sd).unwrap();
        let (r3, _) = rec.load().unwrap();
        assert!(r3.punches.iter().all(|p| p.user_id == 2) && r3.punches.len() == r2.punches.len(), "{:?}", r3.punches);
        let (d2, _) = dir.load().unwrap();
        assert!(d2.punches.iter().any(|p| p.user_id == 1) && d2.punches.iter().any(|p| p.user_id == 2));
        // un paciente bloqueado no lo borra Recepción
        d.set_block(101, Some("Se conserva por obligación legal"), true).unwrap();
        dir.push(&mut d, &mut sd).unwrap();
        let (mut r, mut sr) = rec.load().unwrap();
        r.patients.retain(|p| p.id != 101);
        rec.push(&mut r, &mut sr).unwrap();
        assert!(crate::sync::on_disk(&path).patient(101).is_some());
        dir.stop().unwrap();
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
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
            let body = st.snap.to_json_with(&|list, rec| State::outgoing(&view, list, rec));
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

    /// Notas de evolución en red: una confirmada no la cambia ni la borra un
    /// puesto (un borrador sí), y Recepción no las recibe.
    #[test]
    fn a_confirmed_evolution_note_cannot_be_changed_by_a_station() {
        let (addr, path, h) = start("CLAVE47");
        let rd = signed_in(&addr, "CLAVE47", "Gabinete", 3, "0000");
        let (mut a, mut sa) = rd.load().unwrap();
        a.station = rd.station;
        let now = crate::model::now();
        let p = |o| crate::evolucion::Provenance { origin: o, detail: String::new(), at: now, source_hash: String::new() };
        let done = a.evo_draft(101, 1, None, "Pieza 46: endodoncia sin incidencias.", vec![p(crate::evolucion::Origin::Escrito)], now);
        a.evo_confirm(done, "Dra. Ruiz", &[], false, now).unwrap();
        let draft = a.evo_draft(101, 1, None, "Borrador", vec![p(crate::evolucion::Origin::Escrito)], now);
        rd.push(&mut a, &mut sa).unwrap();
        let orig = crate::sync::on_disk(&path).evo_notes.iter().find(|n| n.id == done).unwrap().clone();
        // quien la confirma es el usuario de la sesión, no el nombre que mande el puesto
        let me = a.users.iter().find(|u| u.id == 3).unwrap().name.clone();
        assert_eq!(orig.confirmed.as_ref().map(|c| c.1.clone()), Some(me.clone()), "no «Dra. Ruiz»: lo pone el servidor");
        assert_eq!(orig.hash, {
            let mut x = orig.clone();
            x.stamp(&me, now);
            x.hash
        });
        // el puesto cambia la confirmada, la borra… y cambia el borrador
        a.evo_notes.iter_mut().find(|n| n.id == done).unwrap().text = "Pieza 47".into();
        a.evo_notes.iter_mut().find(|n| n.id == draft).unwrap().text = "Borrador cambiado".into();
        rd.push(&mut a, &mut sa).unwrap();
        a.evo_notes.retain(|n| n.id != done);
        rd.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.evo_notes.iter().find(|n| n.id == done), Some(&orig), "la confirmada ni cambia ni se borra");
        assert_eq!(disk.evo_notes.iter().find(|n| n.id == draft).unwrap().text, "Borrador cambiado");
        // Recepción no las recibe
        let rr = signed_in(&addr, "CLAVE47", "Recepción", 2, "0000");
        let (r, _) = rr.load().unwrap();
        assert!(r.evo_notes.is_empty(), "las notas clínicas no van a Recepción");
        // al fusionar dos fichas, las notas, los pagos por internet y las
        // solicitudes de derechos pasan a la que se conserva (y se guardan)
        let (mut b, mut sb) = rd.load().unwrap();
        b.station = rd.station;
        let n2 = b.evo_draft(105, 1, None, "Nota de la ficha duplicada", vec![p(crate::evolucion::Origin::Escrito)], now);
        b.evo_confirm(n2, "x", &[], false, now).unwrap();
        let rq = b.new_id();
        b.pay_requests.push(crate::pago::PayRequest {
            id: rq,
            token: crate::portal::new_token(),
            patient_id: 105,
            what: crate::pago::PayFor::Budget(0),
            concept: "Prueba".into(),
            amount: 100,
            created: now,
            created_by: String::new(),
            state: crate::pago::PayState::Pendiente,
            attempts: vec![],
            charges: vec![],
            refund_due: false,
            note: String::new(),
        });
        rd.push(&mut b, &mut sb).unwrap();
        let dir = signed_in(&addr, "CLAVE47", "Despacho", 1, "0000");
        let (mut c, mut sc) = dir.load().unwrap();
        c.station = dir.station;
        dir.chain_op(&mut c, &mut sc, &crate::app::ChainOp::MergePatients { keep: 104, remove: 105 }.json()).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.evo_notes.iter().find(|n| n.id == n2).unwrap().patient_id, 104, "la nota pasa a la ficha que se conserva");
        assert_eq!(disk.pay_requests.iter().find(|r| r.id == rq).unwrap().patient_id, 104, "y la petición de pago también");
        rd.stop().unwrap();
        h.join().unwrap();
    }

    /// Alertas revisadas y esterilización en red: el servidor sella quién y
    /// cuándo; un visto o un uso anotado no se cambia ni se borra desde un
    /// puesto; un lote retirado no vuelve y un ciclo fallido no vuelve atrás.
    /// La clave de grupo (T-F6-01) solo abre el resumen, que no lleva datos de
    /// pacientes; la clave de la clínica no abre el resumen; y probar claves
    /// se frena como con la de la clínica.
    #[test]
    fn the_group_key_opens_only_the_summary() {
        let dir = std::env::temp_dir().join(format!("ds_grupo_{}", std::process::id()));
        let db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 6).unwrap());
        let mut st = State::new(&db, dir.join("clinica.json"), "CLAVEG", vec![7; 32]);
        let token = st.modify(&["group_keys", "audit_log", "config", "next_id"], |d| d.group_key_new("Grupo Norte")).unwrap().unwrap().1;
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let req = |method: &str, path: &str, query: &str, headers: &[(&str, &str)]| Request {
            method: method.into(),
            path: path.into(),
            query: query.into(),
            headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body: vec![],
            peer,
        };
        let year = "desde=2026-01-01&hasta=2026-12-31";
        let (r, _) = st.handle(&req("GET", "/grupo/resumen", year, &[("X-Grupo", &token)]), "CLAVEG");
        assert_eq!(r.code, 200, "{}", r.body);
        let s: crate::grupo::Summary = serde_json::from_str(&r.body).unwrap();
        assert_eq!(s, db.group_summary(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()));
        // en la respuesta tal cual, nada de los pacientes
        for p in &db.patients {
            for v in [p.full_name(), p.nif.clone(), p.mobile.clone(), p.email.clone()] {
                assert!(v.trim().len() < 4 || !r.body.contains(v.trim()), "«{v}» en el resumen");
            }
        }
        crate::grupo::only_known_fields(&r.body).unwrap();
        // con la clave de grupo no se entra en nada más
        for path in ["/datos", "/cambios", "/acceso", "/estado", "/historia"] {
            let (r, _) = st.handle(&req("GET", path, "", &[("X-Grupo", &token)]), "CLAVEG");
            assert_eq!(r.code, 401, "{path}");
        }
        let (r, _) = st.handle(&req("POST", "/grupo/resumen", year, &[("X-Grupo", &token)]), "CLAVEG");
        assert_eq!(r.code, 405);
        // la clave de la clínica no sirve para el resumen
        let (r, _) = st.handle(&req("GET", "/grupo/resumen", year, &[("X-Clave", "CLAVEG")]), "CLAVEG");
        assert_eq!(r.code, 401);
        // fechas que faltan o un periodo demasiado largo
        for q in ["", "desde=2026-01-01", "desde=2026-12-31&hasta=2026-01-01", "desde=2020-01-01&hasta=2026-12-31", "desde=ayer&hasta=hoy"] {
            let (r, _) = st.handle(&req("GET", "/grupo/resumen", q, &[("X-Grupo", &token)]), "CLAVEG");
            assert_eq!(r.code, 400, "{q}");
        }
        // probar claves se frena (y luego tampoco entra la buena desde ahí)
        let bad = "0".repeat(64);
        let mut last = 0;
        for _ in 0..25 {
            last = st.handle(&req("GET", "/grupo/resumen", year, &[("X-Grupo", &bad)]), "CLAVEG").0.code;
        }
        assert_eq!(last, 429);
        assert_eq!(st.handle(&req("GET", "/grupo/resumen", year, &[("X-Grupo", &token)]), "CLAVEG").0.code, 429);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Dos sociedades, cada una con su servidor (T-F6-01, D-25): el grupo junta
    /// sus resúmenes con la clave de cada una. Una clave no vale en la otra
    /// clínica, ni con otra huella, ni retirada; y recepción no crea claves.
    #[test]
    fn a_group_of_two_clinics_adds_up_their_summaries() {
        use crate::grupo::{Link, consolidate};
        let (addr_a, path_a, ha) = start("CLAVE51");
        let (addr_b, path_b, hb) = start("CLAVE52");
        let dir_a = signed_in(&addr_a, "CLAVE51", "Despacho", 1, "0000");
        let dir_b = signed_in(&addr_b, "CLAVE52", "Despacho", 1, "0000");
        let (pin_a, pin_b) = (dir_a.pinned(), dir_b.pinned());
        assert!(!pin_a.is_empty() && pin_a != pin_b, "cada servidor, su identidad");
        let key = |r: &Remote, extra: f64| {
            let (mut d, mut s) = r.load().unwrap();
            d.station = r.station;
            let (id, token) = d.group_key_new("Grupo").unwrap();
            if extra > 0.0 {
                d.add_payment(101, extra, "Tarjeta", "Implante");
            }
            r.push(&mut d, &mut s).unwrap();
            (id, token)
        };
        let (ka_id, ka) = key(&dir_a, 0.0);
        let (_, kb) = key(&dir_b, 250.0);
        let (from, to) = (NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
        let link = |addr: &str, pin: &str, token: &str| Link { name: addr.into(), address: addr.into(), pin: pin.into(), token: token.into() };
        let sa = group_summary(&link(&addr_a, &pin_a, &ka), from, to).unwrap();
        let sb = group_summary(&link(&addr_b, &pin_b, &kb), from, to).unwrap();
        assert_eq!(sa, crate::sync::on_disk(&path_a).group_summary(from, to));
        assert_eq!(sb, crate::sync::on_disk(&path_b).group_summary(from, to));
        assert_eq!(sb.totals().collected - sa.totals().collected, 25_000, "lo que cobró de más la segunda");
        let (t, invoiced, new_patients) = consolidate(&[sa.clone(), sb.clone()]);
        assert_eq!((t.collected, t.production, invoiced, new_patients), (sa.totals().collected + sb.totals().collected, sa.totals().production + sb.totals().production, sa.invoiced + sb.invoiced, sa.new_patients + sb.new_patients));
        // la clave de una clínica no vale en la otra; ni con la huella de otro servidor
        assert!(group_summary(&link(&addr_b, &pin_b, &ka), from, to).unwrap_err().contains("no es válida"));
        assert!(group_summary(&link(&addr_a, &pin_b, &ka), from, to).is_err(), "con otra huella no conecta");
        assert!(group_summary(&link(&addr_a, "", &ka), from, to).unwrap_err().contains("huella"));
        // recepción no crea claves de grupo
        let rec = signed_in(&addr_a, "CLAVE51", "Recepción", 2, "0000");
        let (mut r, mut s) = rec.load().unwrap();
        r.station = rec.station;
        let (_, sneaky) = r.group_key_new("Mía").unwrap();
        rec.push(&mut r, &mut s).unwrap();
        assert!(group_summary(&link(&addr_a, &pin_a, &sneaky), from, to).is_err(), "la de recepción no vale");
        // retirada, ya no vale
        let (mut d, mut s) = dir_a.load().unwrap();
        d.station = dir_a.station;
        d.group_key_revoke(ka_id).unwrap();
        dir_a.push(&mut d, &mut s).unwrap();
        assert!(group_summary(&link(&addr_a, &pin_a, &ka), from, to).is_err());
        dir_a.stop().unwrap();
        dir_b.stop().unwrap();
        ha.join().unwrap();
        hb.join().unwrap();
    }

    /// Pedidos (T-F6-05): dos puestos que proponen o reciben a la vez no
    /// duplican; aprobar es de quien lleva los gastos; desde un puesto solo
    /// se edita un borrador.
    #[test]
    fn orders_are_proposed_and_received_once_and_approved_by_expenses() {
        use crate::app::ChainOp;
        use crate::stock::OrderState;
        let (addr, path, h) = start("CLAVE55");
        let dir = signed_in(&addr, "CLAVE55", "Despacho", 1, "0000");
        let r1 = signed_in(&addr, "CLAVE55", "Recepción 1", 2, "0000");
        let r2 = signed_in(&addr, "CLAVE55", "Recepción 2", 2, "0000");
        // Dirección: aprobación desde 1 €, sin pedidos abiertos
        let (mut d, mut sd) = dir.load().unwrap();
        d.station = dir.station;
        d.config.order_approval_from = Some(1.0);
        for o in d.orders.iter_mut().filter(|o| o.state.open()) {
            o.state = OrderState::Cancelado;
        }
        dir.push(&mut d, &mut sd).unwrap();
        let (mut a, mut sa) = r1.load().unwrap();
        let (mut b, mut sb) = r2.load().unwrap();
        let first = r1.chain_op(&mut a, &mut sa, &ChainOp::ProposeOrders.json()).unwrap();
        assert!(first > 0);
        assert_eq!(r2.chain_op(&mut b, &mut sb, &ChainOp::ProposeOrders.json()).unwrap(), 0, "lo ya pedido no se vuelve a proponer");
        let open = |p: &std::path::Path| crate::sync::on_disk(p).orders.iter().filter(|o| o.state.open()).count();
        let n = open(&path);
        // confirmar: por aprobar; recepción no aprueba, Dirección sí
        r1.chain_op(&mut a, &mut sa, &ChainOp::SubmitOrder { order: first }.json()).unwrap();
        assert!(r1.chain_op(&mut a, &mut sa, &ChainOp::ApproveOrder { order: first }.json()).unwrap_err().contains("gastos"));
        let (mut d, mut sd) = dir.load().unwrap();
        dir.chain_op(&mut d, &mut sd, &ChainOp::ApproveOrder { order: first }.json()).unwrap();
        r1.chain_op(&mut a, &mut sa, &ChainOp::OrderSent { order: first, channel: "teléfono".into() }.json()).unwrap();
        // a mano desde el puesto: ni aprobarlo ni recibirlo
        let (mut a, mut sa) = r1.load().unwrap();
        a.station = r1.station;
        let forged = a.orders.iter().find(|o| o.state == OrderState::Borrador).map(|o| o.id);
        if let Some(f) = forged {
            a.orders.iter_mut().find(|o| o.id == f).unwrap().state = OrderState::Aprobado;
        }
        a.orders.iter_mut().find(|o| o.id == first).unwrap().lines[0].received = 1.0;
        r1.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        if let Some(f) = forged {
            assert_eq!(disk.orders.iter().find(|o| o.id == f).unwrap().state, OrderState::Borrador, "ni se aprueba a mano");
        }
        assert_eq!(disk.orders.iter().find(|o| o.id == first).unwrap().lines[0].received, 0.0, "ni se recibe a mano");
        // la configuración que decide (aprobación, envío automático) y el correo de un
        // proveedor con envío automático, solo Dirección
        let (mut a, mut sa) = r1.load().unwrap();
        a.station = r1.station;
        a.config.order_approval_from = None;
        a.config.orders_auto_send = true;
        let initials = !a.config.board_initials;
        a.config.board_initials = initials;
        let sid = a.suppliers[0].id;
        a.suppliers.iter_mut().find(|s| s.id == sid).unwrap().auto_send = true;
        r1.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!((disk.config.order_approval_from, disk.config.orders_auto_send), (Some(1.0), false), "ni se quita la aprobación ni se activa el envío");
        assert_eq!(disk.config.board_initials, initials, "lo demás de la configuración, sí");
        assert!(!disk.suppliers.iter().find(|s| s.id == sid).unwrap().auto_send, "ni se activa el envío a un proveedor");
        let (mut d, mut sd) = dir.load().unwrap();
        d.station = dir.station;
        d.suppliers.iter_mut().find(|s| s.id == sid).unwrap().auto_send = true;
        dir.push(&mut d, &mut sd).unwrap();
        let (mut a, mut sa) = r1.load().unwrap();
        a.station = r1.station;
        a.suppliers.iter_mut().find(|s| s.id == sid).unwrap().email = "otro@sitio.example".into();
        a.config.support_email = "fuera@sitio.example".into();
        a.config.next_receipt = 1;
        let twin = a.new_id();
        let mut s2 = a.suppliers.iter().find(|s| s.id == sid).unwrap().clone();
        (s2.id, s2.auto_send) = (twin, false);
        a.suppliers.push(s2);
        r1.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_ne!(disk.suppliers.iter().find(|s| s.id == sid).unwrap().email, "otro@sitio.example", "ni se cambia su correo mientras lo tiene");
        assert_ne!(disk.config.support_email, "fuera@sitio.example", "ni a dónde van los informes de errores");
        assert!(!disk.suppliers.iter().any(|s| s.id == twin), "ni otra ficha del mismo proveedor (mismo NIF o correo)");
        // dos puestos reciben lo mismo a la vez: entra una vez
        let o = disk.orders.iter().find(|o| o.id == first).unwrap().clone();
        let traced = o.lines.iter().any(|l| disk.product(l.product_id).is_some_and(|p| p.traced));
        let all: Vec<(usize, f64, String, String)> = o.lines.iter().enumerate().map(|(i, l)| (i, l.qty, if traced { "L1".into() } else { String::new() }, String::new())).collect();
        let stock_before: f64 = o.lines.iter().map(|l| disk.product(l.product_id).unwrap().stock).sum();
        let (mut a, mut sa) = r1.load().unwrap();
        let (mut b, mut sb) = r2.load().unwrap();
        r1.chain_op(&mut a, &mut sa, &ChainOp::ReceiveOrder { order: first, lines: all.clone() }.json()).unwrap();
        assert!(r2.chain_op(&mut b, &mut sb, &ChainOp::ReceiveOrder { order: first, lines: all }.json()).is_err(), "la segunda, no");
        let disk = crate::sync::on_disk(&path);
        let stock_after: f64 = o.lines.iter().map(|l| disk.product(l.product_id).unwrap().stock).sum();
        assert!((stock_after - stock_before - o.lines.iter().map(|l| l.qty).sum::<f64>()).abs() < 1e-6, "entra una sola vez");
        assert_eq!(disk.orders.iter().find(|x| x.id == first).unwrap().state, OrderState::Recibido);
        assert_eq!(open(&path), n - 1);
        dir.stop().unwrap();
        h.join().unwrap();
    }

    /// Autorizaciones (T-F6-04): las crea el servidor, así que dos puestos que
    /// las piden a la vez para el mismo acto no hacen dos; un puesto no la
    /// crea por su cuenta ni la borra, y sí apunta el envío y la respuesta.
    #[test]
    fn two_stations_cannot_ask_twice_for_the_same_authorization() {
        use crate::app::ChainOp;
        let (addr, path, h) = start("CLAVE54");
        let a1 = signed_in(&addr, "CLAVE54", "Recepción 1", 2, "0000");
        let a2 = signed_in(&addr, "CLAVE54", "Recepción 2", 2, "0000");
        let (mut d1, mut s1) = a1.load().unwrap();
        d1.station = a1.station;
        let t = d1.tariffs.iter().find(|t| t.is_insurer).unwrap().clone();
        let pid = d1.patients.iter().find(|p| p.tariff_id == t.id).map(|p| p.id).unwrap_or(d1.patients[0].id);
        d1.patients.iter_mut().find(|p| p.id == pid).unwrap().tariff_id = t.id;
        let code = t.prices.iter().find(|p| p.insurer_fee > 0.0).unwrap().code.clone();
        let e = d1.add_entry(pid, &code, vec![46], vec![], crate::model::TxState::Previsto, 1, crate::model::today());
        a1.push(&mut d1, &mut s1).unwrap();
        let (mut d2, mut s2) = a2.load().unwrap();
        d2.station = a2.station;
        let op = ChainOp::AuthRequest { patient: pid, entries: vec![e] }.json();
        let id = a1.chain_op(&mut d1, &mut s1, &op).unwrap();
        assert!(a2.chain_op(&mut d2, &mut s2, &op).unwrap_err().contains("ya está en la solicitud"), "una sola");
        // por su cuenta, no: ni crearla, ni apuntar el envío o una respuesta, ni borrarla
        let (mut d2, mut s2) = a2.load().unwrap();
        d2.station = a2.station;
        let mut forged = d2.auth_requests.iter().find(|r| r.id == id).unwrap().clone();
        forged.id = d2.new_id();
        d2.auth_requests.push(forged.clone());
        d2.auth_sent(id, "Portal", "W-1", crate::model::now()).unwrap();
        a2.push(&mut d2, &mut s2).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert!(!disk.auth_requests.iter().any(|r| r.id == forged.id), "no se crea por otro camino");
        assert_eq!(disk.auth_requests.iter().find(|r| r.id == id).unwrap().state, crate::autorizaciones::AuthState::Borrador, "ni se apunta el envío a mano");
        let (mut d2, mut s2) = a2.load().unwrap();
        d2.station = a2.station;
        d2.auth_requests.retain(|r| r.id != id);
        a2.push(&mut d2, &mut s2).unwrap();
        assert!(crate::sync::on_disk(&path).auth_requests.iter().any(|r| r.id == id), "ni se borra");
        // por el servidor, sí, con quién lo hizo
        let (mut d2, mut s2) = a2.load().unwrap();
        a2.chain_op(&mut d2, &mut s2, &ChainOp::AuthSent { req: id, channel: "Portal".into(), external_ref: "W-1".into() }.json()).unwrap();
        a2.chain_op(&mut d2, &mut s2, &ChainOp::AuthAnswer { req: id, approved: true, number: "N-7".into(), denied: vec![], valid_until: None, note: String::new(), raw: "ok".into() }.json()).unwrap();
        let disk = crate::sync::on_disk(&path);
        let r = disk.auth_requests.iter().find(|r| r.id == id).unwrap();
        assert_eq!((r.state, r.answers[0].by.as_str()), (crate::autorizaciones::AuthState::Aprobada, "Recepción"), "quien estaba en la sesión");
        assert_eq!(disk.auth_no(e), "N-7");
        // liquidaciones: rechazar a mano un acto (para cobrarlo otra vez) o tocar lo cobrado, no
        let (mut d1, mut s1) = a1.load().unwrap();
        d1.station = a1.station;
        let today = crate::model::today();
        let claim = a1.chain_op(&mut d1, &mut s1, &ChainOp::Claim { tariff: t.id, from: today - chrono::Duration::days(800), to: today, date: today }.json()).unwrap();
        let (mut d1, mut s1) = a1.load().unwrap();
        d1.station = a1.station;
        let c = d1.claims.iter_mut().find(|c| c.id == claim).unwrap();
        c.lines[0].rejected = Some("a mano".into());
        c.paid = c.total;
        a1.push(&mut d1, &mut s1).unwrap();
        let c = crate::sync::on_disk(&path).claims.iter().find(|c| c.id == claim).unwrap().clone();
        assert!(c.lines.iter().all(|l| l.rejected.is_none()) && c.paid == 0.0, "ni rechazos ni cobros a mano");
        let (mut d1, mut s1) = a1.load().unwrap();
        a1.chain_op(&mut d1, &mut s1, &ChainOp::ClaimSent { claim, channel: "Portal".into(), external_ref: String::new() }.json()).unwrap();
        a1.chain_op(&mut d1, &mut s1, &ChainOp::ClaimPaid { claim, amount: 100 }.json()).unwrap();
        assert!(a1.chain_op(&mut d1, &mut s1, &ChainOp::ClaimPaid { claim, amount: 99_999_999 }.json()).is_err(), "no más de lo pendiente");
        assert_eq!(crate::sync::on_disk(&path).claims.iter().find(|c| c.id == claim).unwrap().paid, 1.0, "por el servidor, sí");
        a1.stop().unwrap();
        h.join().unwrap();
    }

    /// Sacar dinero, con permiso de caja; el banco, con el suyo; y un cobro no
    /// se borra ni cambia de importe (revisión de seguridad de T-F6-03). Lo
    /// mismo en los pasos de la financiación, que hace el servidor.
    #[test]
    fn money_out_and_the_bank_need_their_permission() {
        use crate::app::ChainOp;
        let (addr, path, h) = start("CLAVE53");
        let doc = signed_in(&addr, "CLAVE53", "Gabinete", 3, "0000");
        let rec = signed_in(&addr, "CLAVE53", "Recepción", 2, "0000");
        let dir = signed_in(&addr, "CLAVE53", "Despacho", 1, "0000");
        let on_disk = |id: u32| crate::sync::on_disk(&path).payments.iter().find(|p| p.id == id).cloned();
        // la doctora (cobros sí, caja no): un cobro sí; sacar dinero, no
        let (mut a, mut sa) = doc.load().unwrap();
        a.station = doc.station;
        let existing = a.payments.iter().find(|p| p.amount > 0.0).unwrap().clone();
        let plus = a.add_payment(101, 20.0, "Efectivo", "Revisión");
        let minus = a.add_payment(101, -20.0, "Efectivo", "Devolución");
        let voiding = a.add_payment(101, 0.0, "Efectivo", "Anulación");
        a.payments.iter_mut().find(|p| p.id == voiding).unwrap().cancels = Some(existing.id);
        a.payments.iter_mut().find(|p| p.id == existing.id).unwrap().amount = 0.01;
        doc.push(&mut a, &mut sa).unwrap();
        assert!(on_disk(plus).is_some(), "un cobro, sí");
        assert!(on_disk(minus).is_none() && on_disk(voiding).is_none(), "sacar dinero o anular, no");
        assert_eq!(on_disk(existing.id).unwrap().amount, existing.amount, "ni cambiar el importe de un cobro");
        let (mut a, mut sa) = doc.load().unwrap();
        a.station = doc.station;
        a.payments.iter_mut().find(|p| p.id == plus).unwrap().concept = "Revisión anual".into();
        a.payments.retain(|p| p.id != existing.id);
        doc.push(&mut a, &mut sa).unwrap();
        assert!(on_disk(existing.id).is_some(), "ni borrarlo");
        assert_eq!(on_disk(plus).unwrap().concept, "Revisión anual", "el concepto sí se corrige");
        // recepción (caja sí, banco no): una devolución sí; el banco, no
        let (mut r, mut sr) = rec.load().unwrap();
        r.station = rec.station;
        let back = r.add_payment(101, -10.0, "Tarjeta", "Devolución con datáfono");
        let mv = r.bank_moves.iter().find(|m| m.matched.is_none()).map(|m| m.id);
        if let Some(m) = mv {
            r.bank_moves.iter_mut().find(|x| x.id == m).unwrap().matched = Some(crate::bank::Match::Other("a mano".into()));
        }
        rec.push(&mut r, &mut sr).unwrap();
        assert!(on_disk(back).is_some(), "con permiso de caja, sí");
        if let Some(m) = mv {
            assert!(crate::sync::on_disk(&path).bank_moves.iter().find(|x| x.id == m).unwrap().matched.is_none(), "el banco, con su permiso");
        }
        // financiación: los pasos con dinero, en el servidor y cada uno con su permiso
        let (mut r, mut sr) = rec.load().unwrap();
        r.station = rec.station;
        let at = crate::model::now();
        let f = r.fin_new(105, None, "Financiera Ejemplo", crate::cents::Cents(60_000), 6, at).unwrap();
        rec.push(&mut r, &mut sr).unwrap();
        assert!(crate::sync::on_disk(&path).financings.iter().any(|x| x.id == f), "pedirla, sí");
        // los pasos, en el servidor y con su fecha; a mano desde el puesto, no
        use crate::financiacion::FinState;
        for (to, rf) in [(FinState::Aprobada, "FE-9"), (FinState::Firmada, "")] {
            rec.chain_op(&mut r, &mut sr, &ChainOp::FinStep { fin: f, to, provider_ref: rf.into() }.json()).unwrap();
        }
        assert_eq!(crate::sync::on_disk(&path).financings.iter().find(|x| x.id == f).unwrap().signed, Some(crate::model::today()), "firmada con la fecha del servidor");
        let (mut r, mut sr) = rec.load().unwrap();
        r.station = rec.station;
        r.financings.iter_mut().find(|x| x.id == f).unwrap().signed = Some(at.date() + chrono::Duration::days(60));
        let forged = r.fin_new(106, None, "X", crate::cents::Cents(100), 3, at).unwrap();
        r.financings.iter_mut().find(|x| x.id == forged).unwrap().state = FinState::Pagada;
        rec.push(&mut r, &mut sr).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.financings.iter().find(|x| x.id == f).unwrap().signed, Some(crate::model::today()), "ni se cambia la fecha de la firma (el plazo para desistir)");
        assert!(!disk.financings.iter().any(|x| x.id == forged), "ni se crea ya pagada");
        // la caja y las devoluciones con datáfono, con permiso de caja
        let (mut a, mut sa) = doc.load().unwrap();
        a.station = doc.station;
        let cash = a.cash.first().map(|c| c.id);
        if let Some(c) = cash {
            a.cash.iter_mut().find(|x| x.id == c).unwrap().moves.push(crate::model::CashMove { when: at, amount: -50.0, concept: "Salida".into() });
        }
        let sale = a.card_new(101, crate::datafono::CardKind::Venta, crate::cents::Cents(1000), None, "manual", at).unwrap();
        doc.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        if let Some(c) = cash {
            assert!(!disk.cash.iter().find(|x| x.id == c).unwrap().moves.iter().any(|m| m.concept == "Salida"), "la caja, con permiso de caja");
        }
        assert!(disk.card_txs.iter().any(|t| t.id == sale), "un cobro con datáfono, sí");
        let (mut a, mut sa) = doc.load().unwrap();
        a.station = doc.station;
        a.card_txs.iter_mut().find(|t| t.id == sale).unwrap().state = crate::datafono::CardState::Aprobada;
        let refund = a.card_new(101, crate::datafono::CardKind::Devolucion, crate::cents::Cents(500), Some(sale), "manual", at).unwrap();
        doc.push(&mut a, &mut sa).unwrap();
        assert!(!crate::sync::on_disk(&path).card_txs.iter().any(|t| t.id == refund), "una devolución con datáfono, con caja");
        let (mut a, mut sa) = doc.load().unwrap();
        let cancel = ChainOp::FinCancel { fin: f, reason: "x".into() }.json();
        assert!(doc.chain_op(&mut a, &mut sa, &cancel).unwrap_err().contains("caja"), "cancelar (devolver), con caja");
        let (mut r, mut sr) = rec.load().unwrap();
        r.station = rec.station;
        let paid = ChainOp::FinPaid { fin: f, received: 57_000 }.json();
        rec.chain_op(&mut r, &mut sr, &paid).unwrap();
        assert!(rec.chain_op(&mut r, &mut sr, &paid).is_err(), "una vez");
        let disk = crate::sync::on_disk(&path);
        let fin = disk.financings.iter().find(|x| x.id == f).unwrap().clone();
        assert_eq!(fin.state, crate::financiacion::FinState::Pagada);
        assert!(disk.payments.iter().any(|p| Some(p.id) == fin.payment_id && p.amount == 600.0));
        assert!(disk.expenses.iter().any(|e| Some(e.id) == fin.fee_expense && e.base == 30.0), "con la comisión");
        // conciliar: con permiso del banco (Dirección)
        let (mut d, mut sd) = dir.load().unwrap();
        d.station = dir.station;
        d.add_bank_moves(1, &[crate::bank::RawMove { date: at.date(), amount: 570.0, concept: "FINANCIERA EJEMPLO".into(), reference: String::new() }]);
        dir.push(&mut d, &mut sd).unwrap();
        let (d, _) = dir.load().unwrap();
        let mv = d.fin_bank_candidates(f)[0];
        let bank = ChainOp::FinMatchBank { fin: f, bank_move: mv }.json();
        assert!(rec.chain_op(&mut r, &mut sr, &bank).unwrap_err().contains("banco"), "recepción no concilia");
        let (mut d, mut sd) = dir.load().unwrap();
        dir.chain_op(&mut d, &mut sd, &bank).unwrap();
        assert_eq!(crate::sync::on_disk(&path).financings.iter().find(|x| x.id == f).unwrap().bank_move, Some(mv));
        dir.stop().unwrap();
        h.join().unwrap();
    }

    /// Un usuario de un centro (T-F6-01, D-25): no recibe las cajas de los
    /// demás centros, y en ellos no da citas, no cobra ni lleva la caja (ni
    /// moviendo algo de un centro a otro). Si Dirección le cambia los
    /// centros, su puesto recoge los datos enteros con lo que ve ahora.
    #[test]
    fn a_user_of_one_center_cannot_act_in_another() {
        use crate::app::ChainOp;
        let (addr, path, h) = start("CLAVE50");
        let dir = signed_in(&addr, "CLAVE50", "Despacho", 1, "0000");
        // Dirección: dos centros, una caja abierta en cada uno, un cobro en el
        // primero y recepción solo en el segundo
        let (mut d, mut sd) = dir.load().unwrap();
        d.station = dir.station;
        if !d.centers.iter().any(|c| c.id == 2) {
            d.centers.push(crate::model::Center { id: 2, name: "Centro Norte".into(), address: String::new(), phone: String::new(), color: [1, 2, 3], active: true });
        }
        d.cash.clear();
        d.config.current_center = 1;
        let cash1 = d.open_cash_session(50.0);
        let pay1 = d.add_payment(101, 30.0, "Efectivo", "Revisión");
        d.config.current_center = 2;
        let cash2 = d.open_cash_session(80.0);
        d.users.iter_mut().find(|u| u.id == 2).unwrap().centers = vec![2];
        dir.push(&mut d, &mut sd).unwrap();
        let rec = signed_in(&addr, "CLAVE50", "Recepción", 2, "0000");
        let (mut a, mut sa) = rec.load().unwrap();
        a.station = rec.station;
        assert!(a.cash.iter().any(|c| c.id == cash2) && !a.cash.iter().any(|c| c.id == cash1), "solo la caja de su centro");
        assert!(a.payments.iter().any(|p| p.id == pay1), "los cobros, sí: el saldo del paciente es de toda la clínica");
        // citas: nueva en el centro 1 (no), en el 2 (sí) y cambiar una del 1 (no)
        let base = a.appointments.iter().find(|x| crate::model::cid(x.center_id) == 1).unwrap().clone();
        let (n1, n2) = (a.new_id(), a.new_id());
        let mut x1 = base.clone();
        (x1.id, x1.center_id) = (n1, 1);
        let mut x2 = base.clone();
        (x2.id, x2.center_id) = (n2, 2);
        a.appointments.push(x1);
        a.appointments.push(x2);
        a.appointments.iter_mut().find(|x| x.id == base.id).unwrap().reason = "Cambiada desde otro centro".into();
        rec.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert!(!disk.appointments.iter().any(|x| x.id == n1), "ni da citas en otro centro");
        assert!(disk.appointments.iter().any(|x| x.id == n2), "en el suyo, sí");
        assert_eq!(disk.appointments.iter().find(|x| x.id == base.id).unwrap().reason, base.reason, "ni cambia las de otro centro");
        // mover la suya al otro centro, cerrar la caja del otro centro o borrar su cobro: no
        let (mut a, mut sa) = rec.load().unwrap();
        a.station = rec.station;
        a.appointments.iter_mut().find(|x| x.id == n2).unwrap().center_id = 1;
        a.payments.retain(|p| p.id != pay1);
        rec.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(crate::model::cid(disk.appointments.iter().find(|x| x.id == n2).unwrap().center_id), 2, "ni la mueve a otro centro");
        assert!(disk.payments.iter().any(|p| p.id == pay1), "ni borra un cobro de otro centro");
        // operaciones del servidor: en otro centro, o con un cobro de otro centro, no
        let (mut a, mut sa) = rec.load().unwrap();
        a.station = rec.station;
        let mut op = ChainOp::Refund { patient: 101, amount: 1.0, method: "Efectivo".into(), reason: "x".into() }.json();
        op["centro"] = 1.into();
        assert!(rec.chain_op(&mut a, &mut sa, &op).unwrap_err().contains("no trabaja en ese centro"));
        op["centro"] = serde_json::Value::Null;
        assert!(rec.chain_op(&mut a, &mut sa, &op).unwrap_err().contains("no trabaja en ese centro"), "sin decir el centro, tampoco");
        let mut cancel = ChainOp::CancelPayment { payment: pay1, reason: "x".into() }.json();
        cancel["centro"] = 2.into();
        assert!(rec.chain_op(&mut a, &mut sa, &cancel).unwrap_err().contains("centro en el que este usuario no trabaja"));
        assert!(crate::sync::on_disk(&path).payments.iter().all(|p| p.cancels != Some(pay1)));
        // Dirección le da los dos centros: su puesto recoge los datos enteros
        let (mut d, mut sd) = dir.load().unwrap();
        d.station = dir.station;
        d.users.iter_mut().find(|u| u.id == 2).unwrap().centers = vec![1, 2];
        dir.push(&mut d, &mut sd).unwrap();
        assert!(rec.pull(a.rev).unwrap().full, "lo que ve ha cambiado: los datos enteros");
        let (a, _) = rec.load().unwrap();
        assert!(a.cash.iter().any(|c| c.id == cash1), "y ya recibe la caja del otro centro");
        dir.stop().unwrap();
        h.join().unwrap();
    }

    /// Campañas: recepción hace borradores, pero programarlas, cambiarlas,
    /// borrarlas o marcar destinatarios no. Los mensajes los pasa a la cola el
    /// servidor (`ChainOp::QueueCampaigns`, que cualquiera puede pedir), con
    /// su hora: cada destinatario marcado apunta a un mensaje suyo de verdad.
    #[test]
    fn only_direction_schedules_campaigns_and_the_server_queues_them() {
        use crate::app::ChainOp;
        use crate::campanas::{CampState, Campaign, Recipient};
        let (addr, path, h) = start("CLAVE49");
        let dir = signed_in(&addr, "CLAVE49", "Despacho", 1, "0000");
        let rec = signed_in(&addr, "CLAVE49", "Recepción", 2, "0000");
        let at = chrono::NaiveDate::from_ymd_opt(2026, 11, 16).unwrap().and_hms_opt(12, 0, 0).unwrap();
        // recepción: un borrador sí; programarlo, no
        let (mut a, mut sa) = rec.load().unwrap();
        a.station = rec.station;
        for p in a.patients.iter_mut() {
            (p.marketing, p.accepts_gdpr, p.accepts_whatsapp, p.mobile) = (Some(at.date()), true, true, "600000000".into());
        }
        let id = a.new_id();
        a.campaigns.push(Campaign::new(id, "Higiene", crate::comms::Channel::WhatsApp, "Hola [NOMBRE]"));
        rec.push(&mut a, &mut sa).unwrap();
        let (mut a, mut sa) = rec.load().unwrap();
        a.station = rec.station;
        assert!(a.campaigns.iter().any(|c| c.id == id), "el borrador");
        a.campaign_schedule(id, at).unwrap();
        rec.push(&mut a, &mut sa).unwrap();
        assert_eq!(crate::sync::on_disk(&path).campaigns.iter().find(|c| c.id == id).unwrap().state, CampState::Borrador, "recepción no la programa");
        // Dirección la programa
        let (mut d, mut sd) = dir.load().unwrap();
        d.station = dir.station;
        let n = d.campaign_schedule(id, at).unwrap();
        dir.push(&mut d, &mut sd).unwrap();
        let programmed = crate::sync::on_disk(&path).campaigns.iter().find(|c| c.id == id).unwrap().clone();
        assert_eq!((programmed.state, programmed.recipients.len()), (CampState::Programada, n));
        // recepción no marca destinatarios (ni como pasados a la cola ni
        // como que ya no podían recibirla), no añade a nadie, no la cambia ni la borra
        let (mut a, mut sa) = rec.load().unwrap();
        a.station = rec.station;
        a.campaign_tick(at);
        a.messages.retain(|m| m.related != Some(id));
        let x = a.campaigns.iter_mut().find(|c| c.id == id).unwrap();
        x.recipients.iter_mut().for_each(|r| r.message_id = None);
        x.recipients.push(Recipient { patient_id: 101, sent: None, message_id: None, known_appts: vec![] });
        x.text = "Otra cosa".into();
        rec.push(&mut a, &mut sa).unwrap();
        let (mut a, mut sa) = rec.load().unwrap();
        a.station = rec.station;
        a.campaigns.retain(|c| c.id != id);
        rec.push(&mut a, &mut sa).unwrap();
        assert_eq!(crate::sync::on_disk(&path).campaigns.iter().find(|c| c.id == id), Some(&programmed), "ni la marca, ni la cambia, ni la borra");
        // el servidor pasa a la cola lo que toca ahora (según su hora, puede
        // que nada): lo que marca, con su mensaje de verdad
        let (mut a, mut sa) = rec.load().unwrap();
        a.station = rec.station;
        let k = rec.chain_op(&mut a, &mut sa, &ChainOp::QueueCampaigns.json()).unwrap() as usize;
        let disk = crate::sync::on_disk(&path);
        let c = disk.campaigns.iter().find(|c| c.id == id).unwrap();
        let marked: Vec<&Recipient> = c.recipients.iter().filter(|r| r.message_id.is_some()).collect();
        assert_eq!(marked.len(), k);
        for r in marked {
            let m = disk.messages.iter().find(|m| Some(m.id) == r.message_id).expect("el mensaje existe");
            assert_eq!((m.patient_id, m.related, m.kind, m.state), (r.patient_id, Some(id), crate::comms::MsgKind::Campana, crate::comms::MsgState::Pendiente));
        }
        assert_eq!(disk.messages.iter().filter(|m| m.related == Some(id)).count(), k, "y ninguno más");
        dir.stop().unwrap();
        h.join().unwrap();
    }

    #[test]
    fn alert_reviews_and_sterilization_are_stamped_and_kept_by_the_server() {
        let (addr, path, h) = start("CLAVE48");
        let rd = signed_in(&addr, "CLAVE48", "Gabinete", 3, "0000");
        let (mut a, mut sa) = rd.load().unwrap();
        a.station = rd.station;
        let me = a.users.iter().find(|u| u.id == 3).unwrap().name.clone();
        let then = crate::model::now() - chrono::Duration::days(3);
        // un visto con otro nombre y otra hora, y otro de una alerta que no existe
        a.ack_alert(101, "anticoagulantes", "Dr. Falso", then).unwrap();
        let fake = a.new_id();
        a.alert_acks.push(crate::alertas::AlertAck { id: fake, patient_id: 101, rule: "inventada".into(), by: "x".into(), at: then });
        // un ciclo que llega ya «validado» y con un lote «usado»: el servidor
        // lo deja en curso y sin uso (se valida y se usa después, con él)
        let c = a.new_cycle("Autoclave 1", "134 ºC", &["A".into(), "B".into()], 30, then).unwrap();
        let codes: Vec<String> = a.sterile_packs.iter().filter(|p| p.cycle_id == c).map(|p| p.code.clone()).collect();
        a.sterile_cycles.iter_mut().find(|x| x.id == c).unwrap().state = crate::esterilizacion::CycleState::Validado;
        a.sterile_packs.iter_mut().find(|p| p.code == codes[0]).unwrap().used = Some(crate::esterilizacion::PackUse { patient_id: 101, at: then, entry_id: None, appt_id: None, by: "x".into() });
        rd.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.cycle(c).unwrap().state, crate::esterilizacion::CycleState::EnCurso, "un ciclo nuevo empieza en curso");
        assert!(disk.sterile_packs.iter().all(|p| p.used.is_none()), "ni se usa un lote de un ciclo sin validar");
        let (mut a, mut sa) = rd.load().unwrap();
        a.station = rd.station;
        a.validate_cycle(c, true, None, then).unwrap();
        rd.push(&mut a, &mut sa).unwrap();
        // ya validado: un lote nuevo en ese ciclo no se acepta
        let (mut a, mut sa) = rd.load().unwrap();
        a.station = rd.station;
        let extra = a.new_id();
        a.sterile_packs.push(crate::esterilizacion::Pack { id: extra, code: "COLADO-1".into(), cycle_id: c, contents: "x".into(), expires: then.date() + chrono::Duration::days(30), used: None, withdrawn: None });
        // un lote usado (con otro nombre y hora) y otro retirado
        a.use_pack(&codes[0], 101, None, None, then).unwrap();
        a.sterile_packs.iter_mut().find(|p| p.code == codes[0]).unwrap().used.as_mut().unwrap().by = "Dr. Falso".into();
        a.sterile_packs.iter_mut().find(|p| p.code == codes[1]).unwrap().withdrawn = Some("Envoltorio dañado".into());
        rd.push(&mut a, &mut sa).unwrap();
        assert!(!crate::sync::on_disk(&path).sterile_packs.iter().any(|p| p.id == extra), "un lote nuevo solo en un ciclo en curso");
        // dos lotes nuevos de la misma tanda con la misma etiqueta (mayúsculas, espacios): solo uno
        let (mut a, mut sa) = rd.load().unwrap();
        a.station = rd.station;
        let c2 = a.new_cycle("Autoclave 3", "134 ºC", &["Z".into()], 30, then).unwrap();
        let (p1, p2) = (a.new_id(), a.new_id());
        for (id, code) in [(p1, "dup-1"), (p2, " DUP-1 ")] {
            a.sterile_packs.push(crate::esterilizacion::Pack { id, code: code.into(), cycle_id: c2, contents: "x".into(), expires: then.date() + chrono::Duration::days(30), used: None, withdrawn: None });
        }
        rd.push(&mut a, &mut sa).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.sterile_packs.iter().filter(|p| p.id == p1 || p.id == p2).count(), 1, "la misma etiqueta no entra dos veces");
        let disk = crate::sync::on_disk(&path);
        let ack = disk.alert_acks.iter().find(|x| x.rule == "anticoagulantes").unwrap().clone();
        assert_eq!(ack.by, me, "lo pone el servidor");
        assert!(ack.at > then + chrono::Duration::days(2), "a la hora del servidor");
        assert!(!disk.alert_acks.iter().any(|x| x.id == fake), "una alerta que no existe no se guarda");
        let used = disk.sterile_packs.iter().find(|p| p.code == codes[0]).unwrap().used.clone().unwrap();
        assert_eq!((used.patient_id, used.by.as_str()), (101, me.as_str()));
        // el puesto intenta cambiarlo y borrarlo todo
        let (mut b, mut sb) = rd.load().unwrap();
        b.station = rd.station;
        b.alert_acks.clear();
        b.sterile_packs.iter_mut().find(|p| p.code == codes[0]).unwrap().used.as_mut().unwrap().patient_id = 102;
        b.sterile_packs.iter_mut().find(|p| p.code == codes[1]).unwrap().withdrawn = None;
        rd.push(&mut b, &mut sb).unwrap();
        b.cycle_incident(c, "Biológico positivo", crate::model::now()).unwrap();
        rd.push(&mut b, &mut sb).unwrap();
        b.sterile_cycles.iter_mut().find(|x| x.id == c).unwrap().state = crate::esterilizacion::CycleState::Validado;
        b.sterile_packs.retain(|p| p.code != codes[0]);
        rd.push(&mut b, &mut sb).unwrap();
        let disk = crate::sync::on_disk(&path);
        assert_eq!(disk.alert_acks.iter().find(|x| x.id == ack.id), Some(&ack), "el visto ni cambia ni se borra");
        let p0 = disk.sterile_packs.iter().find(|p| p.code == codes[0]).expect("un lote usado no se borra");
        assert_eq!(p0.used.as_ref().unwrap().patient_id, 101, "ni cambia de paciente");
        assert!(disk.sterile_packs.iter().find(|p| p.code == codes[1]).unwrap().withdrawn.is_some(), "un lote retirado no vuelve");
        assert_eq!(disk.cycle(c).unwrap().state, crate::esterilizacion::CycleState::Fallido, "un ciclo fallido no vuelve atrás");
        assert_eq!(disk.affected_by_cycle(c).len(), 1);
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
            let mut headers = vec![("X-Clave".to_string(), if rng.below(10) == 0 { "MAL".to_string() } else { "CLAVE42".to_string() }), (DATA_HEADER.to_string(), crate::model::DATA_VERSION.to_string())];
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
    /// medio no deja aplicar lo de después, y se avisa (no queda un hueco sin avisar).
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
        // la primera dañada y la segunda entera: no se aplica la segunda sin la primera, y se avisa
        std::fs::write(&journal, format!("{}\n{}\n", lines[0].replace('"', "#"), lines[1])).unwrap();
        let mut db: Db = serde_json::from_str(&crate::datos_cifrados::read_text(&path).unwrap()).unwrap();
        assert!(crate::sync::replay_journal(&mut db, &path).unwrap_err().contains("dañada"));
        assert_eq!(db.rev, rev0, "se queda en el último cambio entero");
        assert!(!db.payments.iter().any(|p| ids.contains(&p.id)));
        assert!(Db::load_from(&path, false).1.unwrap().contains("dañada"), "y al abrir se avisa (y no se guarda encima)");
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
