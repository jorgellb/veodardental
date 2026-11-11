//! Segundo factor de acceso: el código de 6 cifras de una app del móvil
//! (Google Authenticator, Microsoft Authenticator, FreeOTP, Authy…).
//!
//! Es el estándar TOTP (RFC 6238, sobre HOTP, RFC 4226): la app y el
//! programa comparten una clave secreta y, cada 30 segundos, los dos calculan
//! el mismo código con HMAC-SHA1 de la clave y la hora. Se acepta el código
//! del momento y el de 30 segundos antes o después (relojes algo
//! desajustados), y nunca dos veces el mismo.

use ring::rand::{SecureRandom, SystemRandom};

/// Segundos que dura cada código.
pub const STEP: u64 = 30;
const DIGITS: u32 = 6;
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Base32 (RFC 4648) sin relleno: así se escribe la clave en las apps.
pub fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for &b in data {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(ALPHABET[((buf >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.chars().filter(|c| !c.is_whitespace() && *c != '=' && *c != '-') {
        let v = ALPHABET.iter().position(|&a| a as char == c.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            out.push((buf >> (bits - 8)) as u8);
            bits -= 8;
        }
    }
    Some(out)
}

/// Clave nueva de 160 bits, en base32.
pub fn new_secret() -> Result<String, String> {
    let mut b = [0u8; 20];
    SystemRandom::new().fill(&mut b).map_err(|_| "El sistema no ha dado números aleatorios.".to_string())?;
    Ok(base32_encode(&b))
}

/// La clave en grupos de 4 letras, para escribirla a mano en la app.
pub fn grouped(secret: &str) -> String {
    secret.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).to_string()).collect::<Vec<_>>().join(" ")
}

/// HOTP (RFC 4226): código de 6 cifras para un contador.
pub fn hotp(key: &[u8], counter: u64) -> u32 {
    let k = ring::hmac::Key::new(ring::hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, key);
    let tag = ring::hmac::sign(&k, &counter.to_be_bytes());
    let h = tag.as_ref();
    let o = (h[h.len() - 1] & 0x0f) as usize;
    let bin = (u32::from(h[o] & 0x7f) << 24) | (u32::from(h[o + 1]) << 16) | (u32::from(h[o + 2]) << 8) | u32::from(h[o + 3]);
    bin % 10u32.pow(DIGITS)
}

/// Código TOTP de una clave (base32) en un instante (segundos desde 1970).
/// Solo lo usan las pruebas: el programa nunca enseña el código, solo lo comprueba.
#[cfg(test)]
pub fn code_at(secret: &str, unix: u64) -> Option<u32> {
    let key = base32_decode(secret).filter(|k| !k.is_empty())?;
    Some(hotp(&key, unix / STEP))
}

/// Comprueba el código escrito: el del momento o el de un paso antes o
/// después, y solo si es posterior al último aceptado (`last`). Devuelve el
/// paso aceptado, para guardarlo.
pub fn verify(secret: &str, code: &str, unix: u64, last: u64) -> Option<u64> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if code.len() != DIGITS as usize || !code.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let want: u32 = code.parse().ok()?;
    let key = base32_decode(secret).filter(|k| !k.is_empty())?;
    let now = unix / STEP;
    [now, now.saturating_sub(1), now + 1].into_iter().find(|&step| step > last && hotp(&key, step) == want)
}

fn pct(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// Dirección que se lee con la cámara en la app (código QR).
pub fn uri(secret: &str, account: &str, issuer: &str) -> String {
    format!("otpauth://totp/{}:{}?secret={secret}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP}", pct(issuer), pct(account), pct(issuer))
}

/// Segundos de ahora (UTC) para los códigos.
pub fn unix_now() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"12345678901234567890";

    #[test]
    fn rfc4226_hotp_vectors() {
        let want = [755224, 287082, 359152, 969429, 338314, 254676, 287922, 162583, 399871, 520489];
        for (i, w) in want.iter().enumerate() {
            assert_eq!(hotp(KEY, i as u64), *w, "contador {i}");
        }
    }

    #[test]
    fn rfc6238_totp_vectors_with_six_digits() {
        let secret = base32_encode(KEY);
        assert_eq!(secret, "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(base32_decode(&secret).unwrap(), KEY);
        assert_eq!(base32_decode("gezd gnbv-gy3t qojq gezd gnbv gy3t qojq").unwrap(), KEY, "como se escribe a mano");
        for (t, code) in [(59u64, 287082u32), (1111111109, 81804), (1111111111, 50471), (1234567890, 5924), (2000000000, 279037), (20000000000, 353130)] {
            assert_eq!(code_at(&secret, t), Some(code), "t = {t}");
        }
    }

    #[test]
    fn codes_are_accepted_once_and_with_some_drift() {
        let secret = base32_encode(KEY);
        let t = 1234567890u64;
        let now = format!("{:06}", code_at(&secret, t).unwrap());
        let before = format!("{:06}", code_at(&secret, t - STEP).unwrap());
        let later = format!("{:06}", code_at(&secret, t + 3 * STEP).unwrap());
        let step = verify(&secret, &now, t, 0).unwrap();
        assert_eq!(step, t / STEP);
        assert!(verify(&secret, &now, t, step).is_none(), "el mismo código no sirve dos veces");
        assert!(verify(&secret, &before, t, 0).is_some(), "30 segundos de margen");
        assert!(verify(&secret, &later, t, 0).is_none(), "uno de dentro de minuto y medio, no");
        assert!(verify(&secret, "12345", t, 0).is_none() && verify(&secret, "abcdef", t, 0).is_none());
        assert!(verify("", &now, t, 0).is_none(), "sin clave no hay código");
        let s = new_secret().unwrap();
        assert_eq!(s.len(), 32);
        assert_eq!(grouped("ABCDEFGHIJ"), "ABCD EFGH IJ");
        let u = uri(&s, "Dra. Ruiz (Clínica)", "VeodarDental");
        assert!(u.starts_with("otpauth://totp/VeodarDental:Dra.%20Ruiz%20%28Cl%C3%ADnica%29?secret="));
        assert!(u.contains("&period=30"));
    }
}
