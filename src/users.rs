//! Usuarios, perfiles y permisos, con registro de accesos (auditoría).

use crate::model::*;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum Profile {
    Direccion,
    Doctor,
    Higienista,
    Recepcion,
}

impl Profile {
    pub const ALL: [Profile; 4] = [Profile::Direccion, Profile::Doctor, Profile::Higienista, Profile::Recepcion];
    pub fn label(self) -> &'static str {
        match self {
            Profile::Direccion => "Dirección / administrador",
            Profile::Doctor => "Doctor",
            Profile::Higienista => "Higienista / auxiliar",
            Profile::Recepcion => "Recepción",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perm {
    Economia,
    Caja,
    Facturacion,
    Informes,
    Configuracion,
    Almacen,
    /// Datos clínicos (salud, periodontograma, imágenes, recetas, ortodoncia y
    /// notas clínicas): ver [`crate::vista`].
    Clinico,
    Gastos,
}

impl Profile {
    pub fn can(self, p: Perm) -> bool {
        use Perm::*;
        match self {
            Profile::Direccion => true,
            Profile::Doctor => matches!(p, Economia | Clinico | Almacen),
            Profile::Higienista => matches!(p, Clinico | Almacen),
            Profile::Recepcion => matches!(p, Economia | Caja | Facturacion | Almacen),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: u32,
    pub name: String,
    pub profile: Profile,
    pub doctor_id: Option<u32>,
    pub salt: String,
    pub pin_hash: String,
    pub must_change: bool,
    pub active: bool,
    /// Segundo factor: clave (base32) de la app del móvil; vacía si no lo usa.
    #[serde(default)]
    pub totp_secret: String,
    /// Último paso de tiempo aceptado (un código no sirve dos veces).
    #[serde(default)]
    pub totp_last: u64,
    /// Fallos seguidos (contraseña o código) desde su última entrada correcta.
    #[serde(default)]
    pub failed_logins: u32,
    #[serde(default)]
    pub last_failed: Option<NaiveDateTime>,
    /// Centros en los que trabaja (vacío: todos). En los demás no da citas,
    /// no cobra ni lleva la caja, y no recibe sus cajas (lo comprueba el
    /// servidor; ver `vista` y D-25).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub centers: Vec<u32>,
}

/// Cómo ha ido la contraseña.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginStep {
    /// Ha entrado.
    Done,
    /// Contraseña correcta: falta el código de la app del móvil.
    NeedCode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessLog {
    pub when: NaiveDateTime,
    pub user: String,
    pub event: String,
    pub ok: bool,
}

/// Fallos seguidos que bloquean al usuario (o hacen esperar a Dirección).
pub const MAX_FAILS: u32 = 5;
/// Longitud mínima de las contraseñas nuevas.
pub const MIN_PIN_LEN: usize = 6;
/// Vueltas de PBKDF2-HMAC-SHA256 (recomendación de OWASP). En las pruebas,
/// pocas, para que no tarden.
const PIN_ITERATIONS: u32 = if cfg!(test) { 1_000 } else { 600_000 };

/// Sal aleatoria de 128 bits en hexadecimal.
fn make_salt(seed: u32) -> String {
    use ring::rand::SecureRandom;
    let mut b = [0u8; 16];
    if ring::rand::SystemRandom::new().fill(&mut b).is_err() {
        // sin generador del sistema (no debería pasar): hora en nanosegundos mezclada
        let t = now().and_utc().timestamp_nanos_opt().unwrap_or(0) as u128 ^ (seed as u128).wrapping_mul(0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C834);
        b = t.to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Formato anterior (SHA-256 repetido): solo para comprobar contraseñas
/// guardadas por versiones antiguas, que pasan al formato nuevo al entrar.
fn legacy_hash(salt: &str, pin: &str) -> String {
    let mut h = sha256_hex(&format!("{salt}:{pin}"));
    for _ in 0..2000 {
        h = sha256_hex(&format!("{salt}{h}"));
    }
    h
}

/// Huella de la contraseña: `pbkdf2$vueltas$clave` (PBKDF2-HMAC-SHA256, 32 bytes).
pub fn hash_pin(salt: &str, pin: &str) -> String {
    format!("pbkdf2${PIN_ITERATIONS}${}", pbkdf2_hex(salt, pin, PIN_ITERATIONS))
}

fn pbkdf2_hex(salt: &str, pin: &str, iterations: u32) -> String {
    let mut out = [0u8; 32];
    let n = std::num::NonZeroU32::new(iterations.max(1)).unwrap_or(std::num::NonZeroU32::MIN);
    ring::pbkdf2::derive(ring::pbkdf2::PBKDF2_HMAC_SHA256, n, salt.as_bytes(), pin.as_bytes(), &mut out);
    out.iter().map(|x| format!("{x:02x}")).collect()
}

/// Comparación en tiempo constante (no revela cuántos caracteres coinciden).
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// ¿Es `pin` la contraseña de `u`? Admite el formato nuevo y el anterior.
pub fn pin_matches(u: &User, pin: &str) -> bool {
    match u.pin_hash.strip_prefix("pbkdf2$").and_then(|r| r.split_once('$')) {
        Some((iters, hash)) => iters.parse::<u32>().is_ok_and(|n| ct_eq(pbkdf2_hex(&u.salt, pin, n).as_bytes(), hash.as_bytes())),
        None => ct_eq(legacy_hash(&u.salt, pin).as_bytes(), u.pin_hash.as_bytes()),
    }
}

/// ¿La contraseña está guardada en un formato antiguo o con menos vueltas?
fn needs_rehash(u: &User) -> bool {
    u.pin_hash.strip_prefix("pbkdf2$").and_then(|r| r.split_once('$')).and_then(|(n, _)| n.parse::<u32>().ok()).is_none_or(|n| n < PIN_ITERATIONS)
}

/// ¿Sirve como contraseña nueva? Al menos 6 caracteres y que no sea evidente
/// (todo el mismo carácter, o cifras seguidas como 123456 o 654321).
pub fn check_new_pin(pin: &str) -> Result<(), String> {
    let c: Vec<char> = pin.chars().collect();
    if c.len() < MIN_PIN_LEN {
        return Err(format!("La contraseña debe tener al menos {MIN_PIN_LEN} caracteres."));
    }
    let same = c.iter().all(|x| *x == c[0]);
    let digits: Option<Vec<i32>> = c.iter().map(|x| x.to_digit(10).map(|d| d as i32)).collect();
    let run = digits.is_some_and(|d| d.windows(2).all(|w| w[1] - w[0] == 1) || d.windows(2).all(|w| w[0] - w[1] == 1));
    if same || run {
        return Err("Elija una contraseña menos evidente.".into());
    }
    Ok(())
}

pub fn new_user(id: u32, name: &str, profile: Profile, doctor_id: Option<u32>, pin: &str) -> User {
    let salt = make_salt(id);
    User { id, name: name.into(), profile, doctor_id, pin_hash: hash_pin(&salt, pin), salt, must_change: true, active: true, totp_secret: String::new(), totp_last: 0, failed_logins: 0, last_failed: None, centers: vec![] }
}

/// Usuarios de una clínica nueva, todos con la contraseña inicial 0000 (que
/// deben cambiar al entrar). Como esa contraseña la sabe todo el mundo, la
/// huella se calcula una sola vez para no hacer esperar al arrancar.
pub fn default_users() -> Vec<User> {
    let first = new_user(1, "Dirección", Profile::Direccion, None, "0000");
    let like = |id: u32, name: &str, profile: Profile, doctor_id: Option<u32>| User { id, name: name.into(), profile, doctor_id, ..first.clone() };
    let rest = [like(2, "Recepción", Profile::Recepcion, None), like(3, "Dra. Ruiz", Profile::Doctor, Some(1)), like(4, "Dr. Navarro", Profile::Doctor, Some(2)), like(5, "Hig. Campos", Profile::Higienista, Some(4))];
    std::iter::once(first.clone()).chain(rest).collect()
}

impl Db {
    pub fn user(&self, id: u32) -> Option<&User> {
        self.users.iter().find(|u| u.id == id)
    }

    fn log_access(&mut self, user: &str, event: &str, ok: bool) {
        self.access_log.push(AccessLog { when: now(), user: user.into(), event: event.into(), ok });
        if self.access_log.len() > 2000 {
            self.access_log.remove(0);
        }
    }

    /// Dirección no se bloquea nunca (se quedaría la clínica sin poder entrar),
    /// pero tras 5 fallos seguidos tiene que esperar: 5 minutos, y el doble
    /// con cada nueva tanda de 5 fallos (hasta 4 horas). Devuelve los minutos.
    fn admin_wait_minutes(u: &User) -> Option<i64> {
        if u.profile != Profile::Direccion || u.failed_logins < MAX_FAILS {
            return None;
        }
        let rounds = (u.failed_logins - MAX_FAILS) / MAX_FAILS;
        let minutes = (5i64 << rounds.min(6)).min(240);
        let last = u.last_failed?;
        (now() - last < chrono::Duration::minutes(minutes)).then_some(minutes)
    }

    fn wait_msg(minutes: i64) -> String {
        format!("Demasiados intentos fallidos: espere {minutes} minutos y vuelva a probar.")
    }

    /// Un fallo más del usuario: tras 5 seguidos queda bloqueado (Dirección solo espera).
    fn fail(&mut self, u: &User, event: &str, msg: &str) -> Result<LoginStep, String> {
        self.log_access(&u.name, event, false);
        let Some(x) = self.users.iter_mut().find(|x| x.id == u.id) else { return Err(msg.into()) };
        x.failed_logins = x.failed_logins.saturating_add(1);
        x.last_failed = Some(now());
        if x.failed_logins < MAX_FAILS {
            return Err(msg.into());
        }
        if x.profile == Profile::Direccion {
            let minutes = Self::admin_wait_minutes(x).unwrap_or(5);
            return Err(Self::wait_msg(minutes));
        }
        x.active = false;
        self.log_access(&u.name, "Usuario bloqueado por 5 intentos fallidos", false);
        Err("Demasiados intentos: el usuario ha quedado bloqueado.".into())
    }

    /// Entrada correcta: los fallos vuelven a cero.
    fn succeed(&mut self, id: u32, event: &str) {
        let Some(x) = self.users.iter_mut().find(|x| x.id == id) else { return };
        x.failed_logins = 0;
        x.last_failed = None;
        let name = x.name.clone();
        self.config.current_user = name.clone();
        self.log_access(&name, event, true);
    }

    /// Pasa la contraseña al formato actual (tras comprobar que es correcta).
    fn upgrade_hash(&mut self, id: u32, pin: &str) {
        if let Some(x) = self.users.iter_mut().find(|x| x.id == id)
            && needs_rehash(x)
        {
            x.salt = make_salt(id);
            x.pin_hash = hash_pin(&x.salt, pin);
        }
    }

    /// Comprueba el PIN. Si el usuario tiene el código del móvil activado,
    /// todavía no entra: falta [`Db::login_code`]. Tras 5 fallos seguidos (de
    /// contraseña o de código) el usuario queda bloqueado hasta que Dirección
    /// lo reactive.
    pub fn login(&mut self, id: u32, pin: &str) -> Result<LoginStep, String> {
        let u = self.user(id).cloned().ok_or("Usuario no encontrado")?;
        if !u.active {
            self.log_access(&u.name, "Acceso denegado (usuario bloqueado)", false);
            return Err("Usuario bloqueado. Pida a Dirección que lo reactive.".into());
        }
        if let Some(m) = Self::admin_wait_minutes(&u) {
            return Err(Self::wait_msg(m));
        }
        if !pin_matches(&u, pin) {
            return self.fail(&u, "Contraseña incorrecta", "Contraseña incorrecta.");
        }
        self.upgrade_hash(id, pin);
        if !u.totp_secret.is_empty() {
            // no se anota nada: los fallos del código siguen contando seguidos
            return Ok(LoginStep::NeedCode);
        }
        self.succeed(id, "Entrada");
        Ok(LoginStep::Done)
    }

    /// Segundo paso: el código de 6 cifras de la app del móvil (`unix`, la hora).
    pub fn login_code(&mut self, id: u32, code: &str, unix: u64) -> Result<(), String> {
        let u = self.user(id).cloned().ok_or("Usuario no encontrado")?;
        if !u.active {
            self.log_access(&u.name, "Acceso denegado (usuario bloqueado)", false);
            return Err("Usuario bloqueado. Pida a Dirección que lo reactive.".into());
        }
        if let Some(m) = Self::admin_wait_minutes(&u) {
            return Err(Self::wait_msg(m));
        }
        match crate::totp::verify(&u.totp_secret, code, unix, u.totp_last) {
            Some(step) => {
                if let Some(x) = self.users.iter_mut().find(|x| x.id == id) {
                    x.totp_last = step;
                }
                self.succeed(id, "Entrada con el código del móvil");
                Ok(())
            }
            None => self.fail(&u, "Código del móvil incorrecto", "El código no es correcto o ya se ha usado: escriba el que muestra ahora la app.").map(|_| ()),
        }
    }

    /// Activa el código del móvil de un usuario, comprobando antes un código
    /// de la app con la clave nueva (así se sabe que la ha guardado bien).
    pub fn enable_totp(&mut self, id: u32, secret: &str, code: &str, unix: u64) -> Result<(), String> {
        let step = crate::totp::verify(secret, code, unix, 0).ok_or("El código no coincide: compruebe que ha escaneado el código QR y escriba el que muestra ahora la app.")?;
        let name = {
            let u = self.users.iter_mut().find(|u| u.id == id).ok_or("Usuario no encontrado")?;
            u.totp_secret = secret.to_string();
            u.totp_last = step;
            u.name.clone()
        };
        self.log_access(&name, "Código del móvil activado", true);
        Ok(())
    }

    /// Quita el código del móvil (por ejemplo, si ha perdido el teléfono).
    pub fn disable_totp(&mut self, id: u32, by: &str) {
        let name = match self.users.iter_mut().find(|u| u.id == id) {
            Some(u) => {
                u.totp_secret.clear();
                u.totp_last = 0;
                u.name.clone()
            }
            None => return,
        };
        self.log_access(&name, &format!("Código del móvil quitado por {by}"), true);
    }

    /// Las copias hechas en un puesto conectado al servidor no llevan las
    /// contraseñas ni las claves del móvil (se quedan en el servidor). Al usar
    /// una así sin servidor, esos usuarios vuelven a la contraseña inicial
    /// 0000, que deben cambiar al entrar, y sin código del móvil. Devuelve cuántos.
    pub fn reset_hidden_credentials(&mut self) -> usize {
        let hidden = crate::server::HIDDEN;
        let initial = new_user(0, "", Profile::Recepcion, None, "0000");
        let mut names = vec![];
        for u in self.users.iter_mut().filter(|u| u.pin_hash == hidden || u.totp_secret == hidden) {
            if u.pin_hash == hidden {
                u.salt = initial.salt.clone();
                u.pin_hash = initial.pin_hash.clone();
                u.must_change = true;
            }
            if u.totp_secret == hidden {
                u.totp_secret.clear();
                u.totp_last = 0;
            }
            names.push(u.name.clone());
        }
        for n in &names {
            self.log_access(n, "Contraseña inicial puesta al usar una copia hecha en un puesto (sin contraseñas)", true);
        }
        names.len()
    }

    pub fn logout(&mut self, id: u32, reason: &str) {
        if let Some(u) = self.user(id).cloned() {
            self.log_access(&u.name, reason, true);
        }
    }

    pub fn change_pin(&mut self, id: u32, old: &str, new: &str) -> Result<(), String> {
        let u = self.user(id).cloned().ok_or("Usuario no encontrado")?;
        if !pin_matches(&u, old) {
            return Err("La contraseña actual no es correcta.".into());
        }
        self.set_pin(id, new)
    }

    /// Pone contraseña nueva; el usuario queda activo y sin fallos pendientes.
    pub fn set_pin(&mut self, id: u32, new: &str) -> Result<(), String> {
        check_new_pin(new)?;
        let salt = make_salt(id);
        let name = {
            let u = self.users.iter_mut().find(|u| u.id == id).ok_or("Usuario no encontrado")?;
            u.pin_hash = hash_pin(&salt, new);
            u.salt = salt;
            u.must_change = false;
            u.active = true;
            u.failed_logins = 0;
            u.last_failed = None;
            u.name.clone()
        };
        self.log_access(&name, "Cambio de contraseña", true);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn db() -> Db {
        Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap())
    }

    #[test]
    fn login_and_permissions() {
        let mut d = db();
        assert!(d.login(2, "9999").is_err());
        assert!(d.login(2, "0000").is_ok());
        assert_eq!(d.config.current_user, "Recepción");
        assert!(Profile::Recepcion.can(Perm::Caja));
        assert!(!Profile::Recepcion.can(Perm::Configuracion));
        assert!(!Profile::Higienista.can(Perm::Economia));
        assert!(d.access_log.iter().any(|l| !l.ok));
    }

    #[test]
    fn second_factor_with_the_phone_code() {
        let mut d = db();
        let secret = crate::totp::base32_encode(b"12345678901234567890");
        let t = 1_800_000_000u64;
        let code = |at: u64| format!("{:06}", crate::totp::code_at(&secret, at).unwrap());
        let bad = |at: u64| (0..).map(|n| format!("{n:06}")).find(|c| crate::totp::verify(&secret, c, at, 0).is_none()).unwrap();
        // para activarlo hace falta un código bueno de la app
        assert!(d.enable_totp(3, &secret, &bad(t), t).is_err());
        d.enable_totp(3, &secret, &code(t), t).unwrap();
        // con la contraseña buena todavía no entra
        assert_eq!(d.login(3, "0000"), Ok(LoginStep::NeedCode));
        // el código de la activación ya no sirve; el siguiente sí
        assert!(d.login_code(3, &code(t), t).is_err());
        assert!(d.login_code(3, &code(t + 30), t + 30).is_ok());
        assert_eq!(d.config.current_user, "Dra. Ruiz");
        assert!(d.access_log.iter().any(|l| l.event == "Entrada con el código del móvil"));
        // 5 códigos malos seguidos bloquean, aunque la contraseña sea buena
        for i in 0..5 {
            assert_eq!(d.login(3, "0000"), Ok(LoginStep::NeedCode));
            assert!(d.login_code(3, &bad(t + 90), t + 90 + i).is_err());
        }
        assert!(!d.user(3).unwrap().active);
        // quitarlo (por ejemplo, si pierde el móvil)
        d.disable_totp(3, "Dirección");
        assert!(d.user(3).unwrap().totp_secret.is_empty());
        assert!(d.access_log.iter().any(|l| l.event.contains("quitado por Dirección")));
        // Dirección no se queda bloqueada, pero tras 5 fallos tiene que esperar
        for _ in 0..5 {
            let _ = d.login(1, "9999");
        }
        assert!(d.user(1).unwrap().active);
        assert!(d.login(1, "0000").unwrap_err().contains("espere"));
    }

    #[test]
    fn lockout_after_failures_and_pin_change() {
        let mut d = db();
        for _ in 0..5 {
            let _ = d.login(5, "1111");
        }
        assert!(!d.user(5).unwrap().active);
        assert!(d.set_pin(3, "12").is_err());
        assert!(d.set_pin(3, "7391").is_err(), "las contraseñas nuevas deben tener al menos 6 caracteres");
        assert!(d.set_pin(3, "000000").is_err(), "una contraseña evidente no sirve");
        d.set_pin(3, "739154").unwrap();
        assert!(d.login(3, "739154").is_ok());
        assert!(!d.user(3).unwrap().must_change);
        assert!(d.user(3).unwrap().pin_hash.starts_with("pbkdf2$"));
    }

    #[test]
    fn alternating_users_does_not_reset_the_lockout() {
        let mut d = db();
        // intentos alternos contra dos usuarios: cada uno lleva su propia cuenta
        for _ in 0..5 {
            let _ = d.login(5, "1111");
            let _ = d.login(4, "1111");
        }
        assert!(!d.user(5).unwrap().active, "la higienista debe quedar bloqueada");
        assert!(!d.user(4).unwrap().active, "el doctor debe quedar bloqueado");
        // la entrada correcta de otro usuario tampoco borra los fallos
        let mut d = db();
        for _ in 0..4 {
            let _ = d.login(5, "1111");
            assert!(d.login(2, "0000").is_ok());
        }
        let _ = d.login(5, "1111");
        assert!(!d.user(5).unwrap().active);
        // cambiar el nombre del usuario no borra los fallos
        let mut d = db();
        for _ in 0..4 {
            let _ = d.login(5, "1111");
        }
        d.users.iter_mut().find(|u| u.id == 5).unwrap().name = "Otro nombre".into();
        let _ = d.login(5, "1111");
        assert!(!d.user(5).unwrap().active);
        // al asignarle contraseña nueva vuelve a estar activo y sin fallos
        d.set_pin(5, "582941").unwrap();
        assert!(d.user(5).unwrap().active);
        assert_eq!(d.user(5).unwrap().failed_logins, 0);
    }

    #[test]
    fn own_successful_login_resets_the_count() {
        let mut d = db();
        for _ in 0..4 {
            let _ = d.login(5, "1111");
        }
        assert!(d.login(5, "0000").is_ok());
        for _ in 0..4 {
            let _ = d.login(5, "1111");
        }
        assert!(d.user(5).unwrap().active, "tras entrar bien, la cuenta empieza de cero");
    }

    #[test]
    fn director_wait_grows_with_more_failures() {
        let mut d = db();
        for _ in 0..5 {
            let _ = d.login(1, "9999");
        }
        assert!(d.login(1, "0000").unwrap_err().contains("espere 5 minutos"));
        // pasada la espera, otra tanda de 5 fallos: la espera se dobla
        d.users.iter_mut().find(|u| u.id == 1).unwrap().failed_logins = 10;
        assert!(d.login(1, "0000").unwrap_err().contains("espere 10 minutos"));
        assert!(d.user(1).unwrap().active, "Dirección nunca se bloquea");
    }

    #[test]
    fn old_hashes_still_work_and_are_upgraded() {
        let mut d = db();
        // usuario con el formato anterior (SHA-256 repetido)
        let u = d.users.iter_mut().find(|u| u.id == 2).unwrap();
        u.salt = "00112233aabbccdd".into();
        u.pin_hash = legacy_hash(&u.salt, "4321");
        assert!(d.login(2, "1111").is_err());
        assert!(d.login(2, "4321").is_ok());
        let u = d.user(2).unwrap();
        assert!(u.pin_hash.starts_with("pbkdf2$"), "al entrar se pasa al formato nuevo");
        assert_ne!(u.salt, "00112233aabbccdd", "con sal nueva");
        assert!(d.login(2, "4321").is_ok(), "y la misma contraseña sigue sirviendo");
    }

    #[test]
    fn backups_made_on_a_station_can_be_used_without_server() {
        let mut d = db();
        // como llegan a un puesto: sin las contraseñas
        for u in d.users.iter_mut() {
            *u = serde_json::from_str(&crate::server::strip_user_json(&serde_json::to_string(u).unwrap())).unwrap();
        }
        d.users.iter_mut().find(|u| u.id == 3).unwrap().totp_secret = crate::server::HIDDEN.into();
        assert!(d.login(1, "0000").is_err(), "sin las contraseñas nadie entra");
        assert_eq!(d.reset_hidden_credentials(), 5);
        assert_eq!(d.login(1, "0000"), Ok(LoginStep::Done));
        assert!(d.user(1).unwrap().must_change);
        assert_eq!(d.login(3, "0000"), Ok(LoginStep::Done), "sin código del móvil");
        assert_eq!(d.reset_hidden_credentials(), 0, "las que tienen contraseña no se tocan");
    }

    #[test]
    fn salts_are_random() {
        let a = new_user(1, "A", Profile::Recepcion, None, "0000");
        let b = new_user(1, "A", Profile::Recepcion, None, "0000");
        assert_ne!(a.salt, b.salt);
        assert_ne!(a.pin_hash, b.pin_hash);
        assert_eq!(a.salt.len(), 32);
    }
}
