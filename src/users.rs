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
    /// Datos clínicos (salud, periodontograma, imágenes). Los perfiles ya lo
    /// tienen en cuenta, pero aún no se usa para ocultar pantallas: falta que la
    /// clínica diga qué partes clínicas debe ver recepción.
    #[allow(dead_code)]
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

fn make_salt(seed: u32) -> String {
    let t = now().and_utc().timestamp_nanos_opt().unwrap_or(0) as u64 ^ (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("{t:016x}")
}

pub fn hash_pin(salt: &str, pin: &str) -> String {
    let mut h = sha256_hex(&format!("{salt}:{pin}"));
    for _ in 0..2000 {
        h = sha256_hex(&format!("{salt}{h}"));
    }
    h
}

pub fn new_user(id: u32, name: &str, profile: Profile, doctor_id: Option<u32>, pin: &str) -> User {
    let salt = make_salt(id);
    User { id, name: name.into(), profile, doctor_id, pin_hash: hash_pin(&salt, pin), salt, must_change: true, active: true, totp_secret: String::new(), totp_last: 0 }
}

pub fn default_users() -> Vec<User> {
    vec![
        new_user(1, "Dirección", Profile::Direccion, None, "0000"),
        new_user(2, "Recepción", Profile::Recepcion, None, "0000"),
        new_user(3, "Dra. Ruiz", Profile::Doctor, Some(1), "0000"),
        new_user(4, "Dr. Navarro", Profile::Doctor, Some(2), "0000"),
        new_user(5, "Hig. Campos", Profile::Higienista, Some(4), "0000"),
    ]
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

    /// Fallos seguidos (contraseña o código) del usuario, del más reciente al más antiguo.
    fn fail_streak(&self, name: &str) -> Vec<chrono::NaiveDateTime> {
        self.access_log.iter().rev().take_while(|l| l.user == name && !l.ok).map(|l| l.when).collect()
    }

    /// Dirección no se bloquea nunca (se quedaría la clínica sin poder entrar),
    /// pero tras 5 fallos seguidos tiene que esperar 5 minutos.
    fn admin_must_wait(&self, u: &User) -> bool {
        u.profile == Profile::Direccion && self.fail_streak(&u.name).get(4).is_some_and(|t| now() - *t < chrono::Duration::minutes(5))
    }

    /// Un fallo más: tras 5 seguidos el usuario queda bloqueado (Dirección solo espera).
    fn fail(&mut self, u: &User, event: &str, msg: &str) -> Result<LoginStep, String> {
        self.log_access(&u.name, event, false);
        if self.fail_streak(&u.name).len() >= 5 {
            if u.profile == Profile::Direccion {
                return Err("Demasiados intentos fallidos: espere 5 minutos y vuelva a probar.".into());
            }
            if let Some(x) = self.users.iter_mut().find(|x| x.id == u.id) {
                x.active = false;
            }
            self.log_access(&u.name, "Usuario bloqueado por 5 intentos fallidos", false);
            return Err("Demasiados intentos: el usuario ha quedado bloqueado.".into());
        }
        Err(msg.into())
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
        if self.admin_must_wait(&u) {
            return Err("Demasiados intentos fallidos: espere 5 minutos y vuelva a probar.".into());
        }
        if hash_pin(&u.salt, pin) != u.pin_hash {
            return self.fail(&u, "Contraseña incorrecta", "Contraseña incorrecta.");
        }
        if !u.totp_secret.is_empty() {
            // no se anota nada: los fallos del código siguen contando seguidos
            return Ok(LoginStep::NeedCode);
        }
        self.config.current_user = u.name.clone();
        self.log_access(&u.name, "Entrada", true);
        Ok(LoginStep::Done)
    }

    /// Segundo paso: el código de 6 cifras de la app del móvil (`unix`, la hora).
    pub fn login_code(&mut self, id: u32, code: &str, unix: u64) -> Result<(), String> {
        let u = self.user(id).cloned().ok_or("Usuario no encontrado")?;
        if !u.active {
            self.log_access(&u.name, "Acceso denegado (usuario bloqueado)", false);
            return Err("Usuario bloqueado. Pida a Dirección que lo reactive.".into());
        }
        if self.admin_must_wait(&u) {
            return Err("Demasiados intentos fallidos: espere 5 minutos y vuelva a probar.".into());
        }
        match crate::totp::verify(&u.totp_secret, code, unix, u.totp_last) {
            Some(step) => {
                if let Some(x) = self.users.iter_mut().find(|x| x.id == id) {
                    x.totp_last = step;
                }
                self.config.current_user = u.name.clone();
                self.log_access(&u.name, "Entrada con el código del móvil", true);
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

    pub fn logout(&mut self, id: u32, reason: &str) {
        if let Some(u) = self.user(id).cloned() {
            self.log_access(&u.name, reason, true);
        }
    }

    pub fn change_pin(&mut self, id: u32, old: &str, new: &str) -> Result<(), String> {
        let u = self.user(id).cloned().ok_or("Usuario no encontrado")?;
        if hash_pin(&u.salt, old) != u.pin_hash {
            return Err("La contraseña actual no es correcta.".into());
        }
        self.set_pin(id, new)
    }

    pub fn set_pin(&mut self, id: u32, new: &str) -> Result<(), String> {
        if new.chars().count() < 4 {
            return Err("La contraseña debe tener al menos 4 caracteres.".into());
        }
        if new == "0000" || new == "1234" {
            return Err("Elija una contraseña menos evidente.".into());
        }
        let salt = make_salt(id);
        let name = {
            let u = self.users.iter_mut().find(|u| u.id == id).ok_or("Usuario no encontrado")?;
            u.pin_hash = hash_pin(&salt, new);
            u.salt = salt;
            u.must_change = false;
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
        d.set_pin(3, "7391").unwrap();
        assert!(d.login(3, "7391").is_ok());
        assert!(!d.user(3).unwrap().must_change);
    }
}
