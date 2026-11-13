//! Cifrado de las copias de seguridad que salen del ordenador.
//!
//! La segunda copia (disco USB, carpeta en la nube) lleva los datos de salud
//! de todos los pacientes. Con una contraseña de copias se guarda cifrada con
//! AES-256-GCM: sin la contraseña no se puede leer, y si alguien cambia un
//! solo byte la copia ya no se abre. La clave sale de la contraseña con
//! PBKDF2-HMAC-SHA256 (600 000 vueltas) y una sal aleatoria.
//!
//! En este ordenador se guarda la clave ya derivada (nunca la contraseña), para
//! hacer la copia diaria sin preguntar; para restaurar en otro ordenador hace
//! falta la contraseña.
//!
//! Formato del fichero: `DSCOPIA1` · vueltas (u32, little endian) · sal (16) ·
//! nonce (12) · datos cifrados con su etiqueta. La cabecera entera va como
//! datos asociados, así que tampoco se puede tocar.

use chrono::NaiveDateTime;
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::path::PathBuf;

const MAGIC: &[u8; 8] = b"DSCOPIA1";
const SALT_LEN: usize = 16;
const HEADER_LEN: usize = 8 + 4 + SALT_LEN + NONCE_LEN;
/// Vueltas de PBKDF2 para las claves nuevas (recomendación de OWASP para HMAC-SHA256).
pub const ITERATIONS: u32 = 600_000;
/// Extensión de las copias cifradas.
pub const EXT: &str = "dscopia";
/// Longitud mínima de la contraseña.
pub const MIN_LEN: usize = 8;

/// Clave de las copias de este ordenador: la sal, las vueltas y la clave derivada.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BackupKey {
    pub salt: String,
    pub iterations: u32,
    pub key: String,
    /// Desde cuándo se cifran las copias.
    pub since: NaiveDateTime,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

fn random(buf: &mut [u8]) -> Result<(), String> {
    SystemRandom::new().fill(buf).map_err(|_| "El sistema no ha dado números aleatorios: no se cifra la copia.".to_string())
}

fn derive(password: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    let n = NonZeroU32::new(iterations.max(1)).unwrap_or(NonZeroU32::MIN);
    ring::pbkdf2::derive(ring::pbkdf2::PBKDF2_HMAC_SHA256, n, salt, password.as_bytes(), &mut out);
    out
}

/// ¿Sirve como contraseña de copias? Devuelve el motivo si no.
pub fn check_password(pw: &str, again: &str) -> Result<(), String> {
    if pw.chars().count() < MIN_LEN {
        return Err(format!("La contraseña debe tener al menos {MIN_LEN} caracteres."));
    }
    if pw != again {
        return Err("Las dos contraseñas no coinciden.".into());
    }
    Ok(())
}

impl BackupKey {
    /// Clave nueva para una contraseña, con una sal aleatoria.
    pub fn new(password: &str, now: NaiveDateTime) -> Result<BackupKey, String> {
        Self::with_iterations(password, ITERATIONS, now)
    }

    pub fn with_iterations(password: &str, iterations: u32, now: NaiveDateTime) -> Result<BackupKey, String> {
        let mut salt = [0u8; SALT_LEN];
        random(&mut salt)?;
        Ok(BackupKey { salt: hex(&salt), iterations, key: hex(&derive(password, &salt, iterations)), since: now })
    }

    fn raw(&self) -> Option<([u8; SALT_LEN], [u8; 32])> {
        Some((unhex(&self.salt)?.try_into().ok()?, unhex(&self.key)?.try_into().ok()?))
    }

    /// ¿Es esta la contraseña de la clave?
    pub fn matches(&self, password: &str) -> bool {
        self.raw().is_some_and(|(salt, key)| derive(password, &salt, self.iterations) == key)
    }
}

/// ¿Son datos cifrados por este programa?
pub fn is_encrypted(bytes: &[u8]) -> bool {
    bytes.len() >= HEADER_LEN && &bytes[..8] == MAGIC
}

/// ¿Es una copia cifrada (por su nombre o por su contenido)?
pub fn is_encrypted_file(path: &std::path::Path) -> bool {
    if path.extension().is_some_and(|e| e == EXT) {
        return true;
    }
    use std::io::Read;
    let mut head = [0u8; HEADER_LEN];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && is_encrypted(&head)
}

/// Cifra con la clave de las copias.
pub fn encrypt(plain: &[u8], key: &BackupKey) -> Result<Vec<u8>, String> {
    let (salt, k) = key.raw().ok_or("La clave de las copias está dañada: vuelva a poner la contraseña.")?;
    let mut nonce = [0u8; NONCE_LEN];
    random(&mut nonce)?;
    let mut out = Vec::with_capacity(HEADER_LEN + plain.len() + AES_256_GCM.tag_len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&key.iterations.to_le_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    let mut body = plain.to_vec();
    let sealing = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &k).map_err(|_| "clave no válida")?);
    sealing.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(&out[..HEADER_LEN]), &mut body).map_err(|_| "No se pudo cifrar la copia.")?;
    out.extend_from_slice(&body);
    Ok(out)
}

fn open(bytes: &[u8], key: &[u8; 32]) -> Option<Vec<u8>> {
    if !is_encrypted(bytes) {
        return None;
    }
    let nonce: [u8; NONCE_LEN] = bytes[8 + 4 + SALT_LEN..HEADER_LEN].try_into().ok()?;
    let mut body = bytes[HEADER_LEN..].to_vec();
    let opening = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).ok()?);
    let plain = opening.open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(&bytes[..HEADER_LEN]), &mut body).ok()?;
    Some(plain.to_vec())
}

fn header(bytes: &[u8]) -> Option<(u32, [u8; SALT_LEN])> {
    if !is_encrypted(bytes) {
        return None;
    }
    Some((u32::from_le_bytes(bytes[8..12].try_into().ok()?), bytes[12..12 + SALT_LEN].try_into().ok()?))
}

/// Descifra con la clave de este ordenador, si la copia se hizo con ella.
pub fn decrypt_with_key(bytes: &[u8], key: &BackupKey) -> Option<Vec<u8>> {
    let (iterations, salt) = header(bytes)?;
    let (my_salt, k) = key.raw()?;
    if salt != my_salt || iterations != key.iterations {
        return None;
    }
    open(bytes, &k)
}

/// Descifra con la contraseña.
pub fn decrypt(bytes: &[u8], password: &str) -> Result<Vec<u8>, String> {
    let (iterations, salt) = header(bytes).ok_or("No es una copia cifrada de VeodarDental.")?;
    if iterations > 50_000_000 {
        return Err("La copia está dañada.".into());
    }
    open(bytes, &derive(password, &salt, iterations)).ok_or_else(|| "La contraseña no es correcta, o la copia está dañada.".to_string())
}

// ───────────────────────────── la clave de este ordenador ─────────────────────────────

fn key_path() -> PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("copias.json")
}

#[cfg(not(test))]
static KEY: std::sync::Mutex<Option<Option<BackupKey>>> = std::sync::Mutex::new(None);

#[cfg(test)]
thread_local! {
    // en las pruebas, cada prueba con la suya y sin tocar la configuración del ordenador
    static KEY: std::cell::RefCell<Option<Option<BackupKey>>> = const { std::cell::RefCell::new(Some(None)) };
}

/// Clave de las copias de este ordenador (se lee una vez del disco).
pub fn current_key() -> Option<BackupKey> {
    #[cfg(not(test))]
    {
        let mut g = KEY.lock().unwrap_or_else(|e| e.into_inner());
        g.get_or_insert_with(|| std::fs::read_to_string(key_path()).ok().and_then(|t| serde_json::from_str(&t).ok())).clone()
    }
    #[cfg(test)]
    {
        KEY.with(|k| k.borrow().clone().flatten())
    }
}

/// Guarda (o quita) la clave de las copias de este ordenador.
pub fn set_key(key: Option<BackupKey>) -> Result<(), String> {
    #[cfg(not(test))]
    {
        let p = key_path();
        match &key {
            Some(k) => {
                if let Some(d) = p.parent() {
                    std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
                }
                std::fs::write(&p, serde_json::to_string_pretty(k).map_err(|e| e.to_string())?).map_err(|e| format!("No se pudo guardar la clave de las copias: {e}"))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
                }
            }
            None => {
                if p.exists() {
                    std::fs::remove_file(&p).map_err(|e| e.to_string())?;
                }
            }
        }
        *KEY.lock().unwrap_or_else(|e| e.into_inner()) = Some(key);
    }
    #[cfg(test)]
    {
        let _ = key_path;
        KEY.with(|k| *k.borrow_mut() = Some(key));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 11).unwrap().and_hms_opt(9, 0, 0).unwrap()
    }

    #[test]
    fn encrypts_and_opens_with_the_key_or_the_password() {
        let key = BackupKey::with_iterations("Sonrisa-2026", 1000, now()).unwrap();
        let plain = "{\"pacientes\": [\"Ana García\"]}".as_bytes();
        let c = encrypt(plain, &key).unwrap();
        assert!(is_encrypted(&c));
        assert!(!c.windows(4).any(|w| w == b"Ana "), "el nombre no debe verse en la copia cifrada");
        assert_eq!(decrypt_with_key(&c, &key).unwrap(), plain);
        assert_eq!(decrypt(&c, "Sonrisa-2026").unwrap(), plain);
        assert!(decrypt(&c, "sonrisa-2026").is_err());
        // cada copia con su nonce: el mismo contenido no da el mismo fichero
        assert_ne!(encrypt(plain, &key).unwrap(), c);
        // otra clave (otra sal) no la abre sin la contraseña
        let other = BackupKey::with_iterations("Sonrisa-2026", 1000, now()).unwrap();
        assert!(decrypt_with_key(&c, &other).is_none());
        assert!(key.matches("Sonrisa-2026") && !key.matches("otra"));
    }

    #[test]
    fn any_change_is_detected() {
        let key = BackupKey::with_iterations("Sonrisa-2026", 1000, now()).unwrap();
        let c = encrypt(b"datos de la clinica", &key).unwrap();
        for i in [0, 9, 20, HEADER_LEN, c.len() - 1] {
            let mut bad = c.clone();
            bad[i] ^= 1;
            assert!(decrypt(&bad, "Sonrisa-2026").is_err() && decrypt_with_key(&bad, &key).is_none(), "byte {i} cambiado y no se nota");
        }
        assert!(decrypt(&c[..HEADER_LEN - 1], "Sonrisa-2026").is_err());
        assert!(decrypt(b"{\"no\": \"cifrada\"}", "x").is_err());
    }

    #[test]
    fn password_rules_and_key_kept_in_memory_during_tests() {
        assert!(check_password("corta", "corta").is_err());
        assert!(check_password("bastante-larga", "otra-distinta").is_err());
        assert!(check_password("bastante-larga", "bastante-larga").is_ok());
        assert_eq!(current_key(), None);
        let key = BackupKey::with_iterations("bastante-larga", 1000, now()).unwrap();
        set_key(Some(key.clone())).unwrap();
        assert_eq!(current_key(), Some(key));
        set_key(None).unwrap();
        assert_eq!(current_key(), None);
        // las claves nuevas usan las vueltas recomendadas
        assert_eq!(BackupKey::new("bastante-larga", now()).unwrap().iterations, ITERATIONS);
    }
}
