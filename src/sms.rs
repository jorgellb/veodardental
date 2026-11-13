//! Envío de SMS por una pasarela HTTP (T-F3-07). Es genérico: se envía un
//! JSON por POST a la dirección del proveedor que contrate la clínica, sin
//! suponer el formato de ninguno en concreto (B-14 y B-19).
//! - **Cuerpo:** `{"to": "+34600…", "text": "…", "from": "…", "reference": "…"}`.
//! - **Cabeceras:** `Authorization: Bearer <clave>` e `Idempotency-Key` con la
//!   referencia del mensaje. Un reintento lleva la misma clave, y el
//!   proveedor que la respete no lo manda dos veces.
//!
//! El resultado se clasifica como el de un correo (`mail::SendError`):
//! - **No enviado** (se puede reintentar): sin conexión o rechazado (4xx o 503).
//! - **Incierto**: se cortó tras enviarlo, o la pasarela falló con 500, 502 o
//!   504. No se reintenta sin que alguien diga si llegó.
//!
//! La dirección y la clave se guardan en este ordenador (`sms.json`, solo
//! para el usuario), no en los datos de la clínica.

use crate::mail::SendError;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SmsSettings {
    /// Dirección de la pasarela (https). Vacía: los SMS se copian y se envían a mano.
    pub url: String,
    pub token: String,
    /// Remitente (nombre corto o número), si la pasarela lo admite.
    pub sender: String,
}

impl SmsSettings {
    pub fn enabled(&self) -> bool {
        !self.url.trim().is_empty()
    }
}

fn settings_path() -> PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("sms.json")
}

pub fn load_settings() -> SmsSettings {
    std::fs::read_to_string(settings_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_settings(s: &SmsSettings) -> Result<(), String> {
    let p = settings_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    crate::paths::write_private(&p, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())
}

/// Número en formato internacional: un móvil español de 9 cifras lleva +34.
pub fn e164(mobile: &str) -> Option<String> {
    let t = mobile.trim();
    let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    let n = if t.starts_with('+') {
        format!("+{digits}")
    } else if let Some(rest) = digits.strip_prefix("00") {
        format!("+{rest}")
    } else if digits.len() == 9 && digits.starts_with(['6', '7']) {
        format!("+34{digits}")
    } else {
        return None;
    };
    (9..=16).contains(&(n.len() - 1)).then_some(n)
}

fn local(url: &str) -> bool {
    url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost")
}

/// Envía un SMS. `reference` identifica el mensaje y va como clave de
/// idempotencia: es la misma en cada reintento.
pub fn send(s: &SmsSettings, to: &str, text: &str, reference: &str) -> Result<(), SendError> {
    let to = e164(to).ok_or_else(|| SendError::NotSent(format!("Número de móvil no válido: {to}")))?;
    let url = s.url.trim();
    if !url.starts_with("https://") && !local(url) {
        return Err(SendError::NotSent("La pasarela de SMS tiene que ir cifrada (https://).".into()));
    }
    let agent = ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(5)).timeout(std::time::Duration::from_secs(20)).build();
    let body = serde_json::json!({ "to": to, "text": text, "from": s.sender.trim(), "reference": reference }).to_string();
    let r = agent.post(url).set("Content-Type", "application/json").set("Authorization", &format!("Bearer {}", s.token.trim())).set("Idempotency-Key", reference).send_string(&body);
    match r {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(code, resp)) => {
            let mut t = String::new();
            let _ = resp.into_reader().take(2000).read_to_string(&mut t);
            let t: String = t.chars().filter(|c| !c.is_control()).take(200).collect();
            match code {
                500 | 502 | 504 => Err(SendError::Uncertain(format!("La pasarela de SMS falló al enviarlo ({code}): puede que haya salido. {t}"))),
                _ => Err(SendError::NotSent(format!("La pasarela de SMS no lo envió ({code}): {t}"))),
            }
        }
        Err(ureq::Error::Transport(t)) => {
            use ureq::ErrorKind::*;
            Err(match t.kind() {
                Dns | ConnectionFailed | InvalidUrl | UnknownScheme | InsecureRequestHttpsOnly | InvalidProxyUrl | ProxyConnect | ProxyUnauthorized => SendError::NotSent(format!("Sin conexión con la pasarela de SMS: {t}")),
                _ => SendError::Uncertain(format!("Se cortó la conexión con la pasarela de SMS al enviarlo: puede que haya salido ({t})")),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    /// DOBLE DE PRUEBA de una pasarela de SMS (explícito, en 127.0.0.1): a
    /// cada petición contesta con el código de `codes` (0: corta la conexión
    /// sin responder tras leerla). Lleva la cuenta de los SMS «enviados» y no
    /// repite los que traen una clave de idempotencia ya vista.
    fn fake_gateway(codes: Vec<u16>) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/sms", listener.local_addr().unwrap().port());
        let sent: Arc<Mutex<Vec<String>>> = Arc::default();
        let log = sent.clone();
        let h = std::thread::spawn(move || {
            let mut seen = HashSet::new();
            for code in codes {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = vec![];
                let mut chunk = [0u8; 8192];
                let (head, body) = loop {
                    let n = s.read(&mut chunk).unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(i) = text.find("\r\n\r\n") {
                        let len: usize = text.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
                        if buf.len() >= i + 4 + len {
                            break (text[..i].to_string(), text[i + 4..].to_string());
                        }
                    }
                };
                let key = head.lines().find_map(|l| l.strip_prefix("Idempotency-Key: ").or_else(|| l.strip_prefix("idempotency-key: "))).unwrap_or("").to_string();
                assert!(head.to_lowercase().contains("authorization: bearer clave-sms"), "{head}");
                // la pasarela lo procesa (salvo si lo rechaza), aunque luego no conteste
                if (code == 0 || code == 200 || code >= 500 && code != 503) && seen.insert(key.clone()) {
                    log.lock().unwrap().push(body.clone());
                }
                if code == 0 {
                    drop(s);
                    continue;
                }
                let reply = "{\"ok\": true}";
                let _ = write!(s, "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
            }
        });
        (url, sent, h)
    }

    fn cfg(url: &str) -> SmsSettings {
        SmsSettings { url: url.into(), token: "clave-sms".into(), sender: "CLINICA".into() }
    }

    #[test]
    fn numbers_go_out_in_international_format() {
        assert_eq!(e164("600 11 22 33").as_deref(), Some("+34600112233"));
        assert_eq!(e164("+34 711-22-33-44").as_deref(), Some("+34711223344"));
        assert_eq!(e164("0033612345678").as_deref(), Some("+33612345678"));
        assert_eq!(e164("912345678"), None, "un fijo no recibe SMS");
        assert_eq!(e164("12"), None);
    }

    /// Envío, rechazo, caída antes de enviar, corte después de enviar
    /// (incierto) y fallos de la pasarela; el reintento lleva la misma clave
    /// y la pasarela no lo repite.
    #[test]
    fn each_outcome_is_classified_and_a_retry_never_sends_twice() {
        let (url, sent, h) = fake_gateway(vec![200, 400, 0, 200, 502, 503]);
        let s = cfg(&url);
        assert!(send(&s, "600112233", "Cita mañana a las 10:00", "veodardental-1").is_ok());
        assert!(matches!(send(&s, "600112233", "x", "veodardental-2"), Err(SendError::NotSent(e)) if e.contains("(400)")));
        // se corta después de recibirlo: incierto (puede que haya salido)
        assert!(matches!(send(&s, "600112233", "Cita", "veodardental-3"), Err(SendError::Uncertain(_))));
        // si alguien decide reintentarlo, va con la misma clave: no sale dos veces
        assert!(send(&s, "600112233", "Cita", "veodardental-3").is_ok());
        assert!(matches!(send(&s, "600112233", "y", "veodardental-4"), Err(SendError::Uncertain(e)) if e.contains("502")));
        assert!(matches!(send(&s, "600112233", "z", "veodardental-5"), Err(SendError::NotSent(e)) if e.contains("503")));
        h.join().unwrap();
        let sent = sent.lock().unwrap().clone();
        assert_eq!(sent.len(), 3, "1, 3 (una vez) y 4: {sent:?}");
        assert!(sent[0].contains("\"to\":\"+34600112233\"") && sent[0].contains("\"reference\":\"veodardental-1\""));
        // sin conexión: no enviado
        assert!(matches!(send(&cfg("http://127.0.0.1:9/sms"), "600112233", "x", "r"), Err(SendError::NotSent(e)) if e.contains("Sin conexión")));
        // sin cifrar a otro ordenador, no
        assert!(matches!(send(&cfg("http://pasarela.ejemplo/sms"), "600112233", "x", "r"), Err(SendError::NotSent(e)) if e.contains("https")));
        assert!(matches!(send(&cfg(&url), "912345678", "x", "r"), Err(SendError::NotSent(e)) if e.contains("no válido")));
    }

    /// El resultado se apunta en el mensaje como el de un correo: enviado,
    /// fallido (se puede reintentar) o incierto (espera a que alguien lo decida).
    #[test]
    fn the_outcome_is_recorded_on_the_message() {
        let mut db = crate::model::Db::sample(crate::model::today());
        db.generate_messages(crate::model::today());
        let ids: Vec<u32> = db.messages.iter().filter(|m| m.state == crate::comms::MsgState::Pendiente).map(|m| m.id).take(3).collect();
        assert_eq!(ids.len(), 3, "hay mensajes de ejemplo pendientes");
        db.sms_outcome(ids[0], &Ok(()));
        db.sms_outcome(ids[1], &Err(SendError::NotSent("rechazado".into())));
        db.sms_outcome(ids[2], &Err(SendError::Uncertain("cortado".into())));
        let m = |id: u32| db.messages.iter().find(|m| m.id == id).unwrap().clone();
        assert_eq!(m(ids[0]).state, crate::comms::MsgState::Enviado);
        assert!(m(ids[1]).state == crate::comms::MsgState::Pendiente && !m(ids[1]).uncertain && m(ids[1]).attempts == 1);
        assert!(m(ids[2]).uncertain && m(ids[2]).state == crate::comms::MsgState::Pendiente);
    }
}
