//! Telefonía: al entrar una llamada, la centralita o el programa de
//! teléfono (softphone) avisa a VeodarDental con el número, y el puesto que
//! la recibe muestra al momento quién llama (paciente, contacto, proveedor…)
//! con su próxima cita, avisos y saldo.
//!
//! Dos formas de avisar, según lo que permita la centralita:
//! * ejecutar `veodardental --llamada <número>` (se lo pasa a la ventana abierta);
//! * abrir la dirección local `http://127.0.0.1:8897/llamada?numero=<número>`.

use crate::model::*;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// Puerto local (solo este ordenador) por el que llegan los avisos de llamada.
pub const CALL_PORT: u16 = 8897;
const KEEP: usize = 1000;
/// La misma llamada avisada otra vez antes de estos segundos es repetida.
pub const REPEAT_SECS: i64 = 15;
/// Avisos de llamada por minuto como máximo (más es una avalancha).
pub const MAX_PER_MINUTE: usize = 30;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CallLog {
    pub id: u32,
    pub when: NaiveDateTime,
    pub number: String,
    /// Pacientes con ese número (una familia puede compartir teléfono).
    pub patients: Vec<u32>,
    /// Quién es, si no es paciente (contacto, colaborador, proveedor, mutua).
    pub other: String,
    /// Puesto/usuario que recibió el aviso.
    pub taken_by: String,
}

/// Parte nacional del número, solo cifras (quita +34 / 0034).
pub fn normalize(n: &str) -> String {
    let mut d: String = n.chars().filter(|c| c.is_ascii_digit()).collect();
    if let Some(rest) = d.strip_prefix("00") {
        d = rest.to_string();
    }
    if d.len() > 9 && d.starts_with("34") {
        d = d[2..].to_string();
    }
    d
}

/// ¿Son el mismo número aunque estén escritos distinto?
pub fn same_number(a: &str, b: &str) -> bool {
    let (x, y) = (normalize(a), normalize(b));
    if x.len() < 6 || y.len() < 6 {
        return false;
    }
    if x.len() >= 9 && y.len() >= 9 { x[x.len() - 9..] == y[y.len() - 9..] } else { x == y }
}

/// Número legible (600 123 456 / 91 555 10 30); otros formatos se dejan tal cual.
pub fn pretty(n: &str) -> String {
    let d = normalize(n);
    if d.len() == 9 {
        let three = || format!("{} {} {}", &d[0..3], &d[3..6], &d[6..9]);
        if d.starts_with('6') || d.starts_with('7') || d.starts_with("90") || d.starts_with("80") {
            // móviles y números de tarificación especial (900, 902, 800…)
            three()
        } else if d.starts_with("91") || d.starts_with("93") {
            // Madrid y Barcelona
            format!("{} {} {} {}", &d[0..2], &d[2..5], &d[5..7], &d[7..9])
        } else if d.starts_with('9') || d.starts_with('8') {
            // resto de fijos: prefijo de provincia de 3 cifras
            format!("{} {} {} {}", &d[0..3], &d[3..5], &d[5..7], &d[7..9])
        } else {
            three()
        }
    } else if d.is_empty() {
        "número oculto".into()
    } else {
        n.trim().to_string()
    }
}

impl Db {
    /// Pacientes cuyo móvil o teléfono coincide con el número.
    pub fn patients_by_phone(&self, number: &str) -> Vec<u32> {
        let mut v: Vec<&Patient> = self.patients.iter().filter(|p| same_number(&p.mobile, number) || same_number(&p.phone, number)).collect();
        // primero los activos y los vistos más recientemente
        v.sort_by_key(|p| (!p.active, std::cmp::Reverse(self.last_visit(p.id))));
        v.into_iter().map(|p| p.id).take(6).collect()
    }

    /// Quién llama si no es paciente: contacto, colaborador, proveedor/laboratorio o mutua.
    pub fn other_caller(&self, number: &str) -> Option<String> {
        if let Some(c) = self.contacts.iter().find(|c| same_number(&c.phone, number) || same_number(&c.mobile, number)) {
            return Some(if c.company.is_empty() { c.name.clone() } else { format!("{} ({})", c.name, c.company) });
        }
        if let Some(c) = self.collaborators.iter().find(|c| same_number(&c.phone, number)) {
            return Some(format!("{} · colaborador", c.name));
        }
        if let Some(s) = self.suppliers.iter().find(|s| same_number(&s.phone, number)) {
            return Some(format!("{} · {}", s.name, if s.is_lab { "laboratorio" } else { "proveedor" }));
        }
        if let Some(t) = self.tariffs.iter().find(|t| same_number(&t.phone, number)) {
            return Some(format!("{} · compañía", t.name));
        }
        None
    }

    /// Apunta la llamada en el registro y devuelve su id.
    /// Si la centralita avisa otra vez de la misma llamada (en menos de
    /// [`REPEAT_SECS`] segundos), es la misma: no se apunta dos veces.
    pub fn log_call(&mut self, number: &str, taken_by: &str) -> u32 {
        let t = now();
        if let Some(c) = self.calls.iter().rev().find(|c| same_number(&c.number, number) && (t - c.when).num_seconds().abs() <= REPEAT_SECS) {
            return c.id;
        }
        let id = self.new_id();
        let patients = self.patients_by_phone(number);
        let other = if patients.is_empty() { self.other_caller(number).unwrap_or_default() } else { String::new() };
        self.calls.push(CallLog { id, when: now(), number: number.trim().to_string(), patients, other, taken_by: taken_by.into() });
        if self.calls.len() > KEEP {
            let extra = self.calls.len() - KEEP;
            self.calls.drain(0..extra);
        }
        id
    }

    /// Llamadas de un paciente (las más recientes primero).
    pub fn calls_of(&self, pid: u32) -> Vec<&CallLog> {
        let mut v: Vec<&CallLog> = self.calls.iter().filter(|c| c.patients.contains(&pid)).collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.id));
        v
    }
}

// ───────────────────────────── aviso local ─────────────────────────────

fn percent_decode(s: &str) -> String {
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// ¿Parece un número de teléfono? Cifras (de 3 a 17, con el prefijo «00»
/// internacional), separadores habituales y un «+» delante; nada más.
pub fn plausible_number(n: &str) -> bool {
    let t = n.trim();
    let digits = t.chars().filter(|c| c.is_ascii_digit()).count();
    (3..=17).contains(&digits) && t.chars().enumerate().all(|(i, c)| c.is_ascii_digit() || matches!(c, ' ' | '-' | '.' | '(' | ')') || (c == '+' && i == 0))
}

/// Número que viene en la dirección (`numero`, `number`, `n` o `caller`), si
/// parece un número de teléfono.
pub fn number_from_url(url: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        matches!(k, "numero" | "number" | "n" | "caller" | "telefono").then(|| percent_decode(v).trim().to_string()).filter(|v| plausible_number(v))
    })
}

/// Escucha en este ordenador (127.0.0.1) los avisos de llamada. Devuelve el
/// puerto real y el canal por el que llegan los números; `wake` se llama al
/// recibir uno (para que la ventana se redibuje al momento).
pub fn listen(port: u16, wake: impl Fn() + Send + 'static) -> Result<(u16, Receiver<String>), String> {
    let server = tiny_http::Server::http(("127.0.0.1", port)).map_err(|e| format!("No se pudo abrir el puerto {port}: {e}"))?;
    let real = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(port);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut recent: std::collections::VecDeque<std::time::Instant> = std::collections::VecDeque::new();
        for req in server.incoming_requests() {
            let url = req.url().to_string();
            let h = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..]).unwrap();
            // una página web abierta en el navegador (otro sitio) no puede avisar
            // de llamadas: el navegador lo dice en «Sec-Fetch-Site»; la centralita,
            // el programa de teléfono o la dirección escrita a mano, no
            let site = req.headers().iter().find(|x| x.field.equiv("Sec-Fetch-Site")).map(|x| x.value.as_str().to_ascii_lowercase());
            if site.as_deref().is_some_and(|s| s != "none" && s != "same-origin") {
                let _ = req.respond(tiny_http::Response::from_string("no").with_status_code(403).with_header(h));
                continue;
            }
            // una avalancha de avisos se corta (no tapa las llamadas de verdad)
            let now_i = std::time::Instant::now();
            while recent.front().is_some_and(|t| now_i.duration_since(*t).as_secs() >= 60) {
                recent.pop_front();
            }
            if recent.len() >= MAX_PER_MINUTE {
                let _ = req.respond(tiny_http::Response::from_string("demasiados avisos").with_status_code(429).with_header(h));
                continue;
            }
            recent.push_back(now_i);
            match (url.starts_with("/llamada"), number_from_url(&url)) {
                (true, Some(n)) => {
                    let ok = tx.send(n).is_ok();
                    wake();
                    let _ = req.respond(tiny_http::Response::from_string(if ok { "ok" } else { "cerrado" }).with_header(h));
                }
                (true, None) => {
                    let _ = req.respond(tiny_http::Response::from_string("falta el número: /llamada?numero=600123456").with_status_code(400).with_header(h));
                }
                _ => {
                    let _ = req.respond(tiny_http::Response::from_string("VeodarDental · aviso de llamadas").with_header(h));
                }
            }
        }
    });
    Ok((real, rx))
}

/// Pasa una llamada a la ventana de VeodarDental ya abierta en este ordenador.
pub fn forward(number: &str, port: u16) -> Result<(), String> {
    let n: String = number.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect::<String>().replace('+', "%2B");
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_millis(800)).timeout(Duration::from_secs(3)).build();
    agent.get(&format!("http://127.0.0.1:{port}/llamada?numero={n}")).call().map(|_| ()).map_err(|e| e.to_string())
}

/// Llamadas de ejemplo para el modo demostración.
pub fn sample_calls(db: &mut Db) {
    let mut list: Vec<(i64, String)> = vec![];
    for (mins, pid) in [(190, 103u32), (75, 100), (12, 101)] {
        if let Some(p) = db.patient(pid) {
            list.push((mins, p.mobile.clone()));
        }
    }
    if let Some(s) = db.suppliers.first() {
        list.push((40, s.phone.clone()));
    }
    list.push((25, "699 123 456".into()));
    list.sort_by_key(|x| std::cmp::Reverse(x.0));
    for (mins, n) in list {
        let id = db.log_call(&n, "Recepción");
        if let Some(c) = db.calls.iter_mut().find(|c| c.id == id) {
            c.when = now() - chrono::Duration::minutes(mins);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn db() -> Db {
        Db::sample(NaiveDate::from_ymd_opt(2026, 11, 6).unwrap())
    }

    #[test]
    fn numbers_match_in_any_format() {
        assert!(same_number("+34 610 200 100", "610200100"));
        assert!(same_number("0034610200100", "610 200 100"));
        assert!(same_number("610-200-100", "(+34) 610.200.100"));
        assert!(!same_number("610200101", "610200100"));
        assert!(!same_number("", "610200100"));
        assert!(!same_number("112", "112"), "números cortos no identifican a nadie");
        assert_eq!(pretty("+34610200100"), "610 200 100");
        assert_eq!(pretty("915551030"), "91 555 10 30");
        assert_eq!(pretty("900100100"), "900 100 100");
        assert_eq!(pretty("958123456"), "958 12 34 56");
        assert_eq!(pretty(""), "número oculto");
    }

    #[test]
    fn caller_is_identified_and_logged() {
        let mut db = db();
        let ana = db.patients.iter().find(|p| p.id == 100).unwrap().clone();
        assert_eq!(db.patients_by_phone(&format!("+34{}", ana.mobile.replace(' ', ""))), vec![100]);
        assert_eq!(db.patients_by_phone(&ana.phone), vec![100], "también por el fijo");
        // una familia que comparte móvil
        let mut hijo = db.patients.iter().find(|p| p.id == 101).unwrap().clone();
        hijo.id = 999;
        hijo.mobile = ana.mobile.clone();
        db.patients.push(hijo);
        assert_eq!(db.patients_by_phone(&ana.mobile).len(), 2);
        // proveedor o contacto
        let sup = db.suppliers[0].clone();
        assert!(db.patients_by_phone(&sup.phone).is_empty());
        assert!(db.other_caller(&sup.phone).unwrap().contains(&sup.name));
        let id = db.log_call(&sup.phone, "Recepción");
        let c = db.calls.iter().find(|c| c.id == id).unwrap();
        assert!(c.patients.is_empty() && c.other.contains(&sup.name));
        let before = db.calls_of(100).len();
        db.log_call(&ana.mobile, "Recepción");
        assert_eq!(db.calls_of(100).len(), before + 1);
        assert!(db.other_caller("699 999 999").is_none());
    }

    #[test]
    fn url_number_is_decoded() {
        assert_eq!(number_from_url("/llamada?numero=%2B34610200100").as_deref(), Some("+34610200100"));
        assert_eq!(number_from_url("/llamada?x=1&n=915551030").as_deref(), Some("915551030"));
        assert_eq!(number_from_url("/llamada?numero="), None);
        assert_eq!(number_from_url("/llamada"), None);
    }

    /// Solo números de teléfono (no texto ni cifras de más o de menos), y la
    /// misma llamada avisada dos veces seguidas se apunta una vez.
    #[test]
    fn only_real_numbers_and_no_repeated_calls() {
        assert_eq!(number_from_url("/llamada?numero=%3Cscript%3E"), None);
        assert_eq!(number_from_url("/llamada?numero=12"), None);
        assert_eq!(number_from_url(&format!("/llamada?numero={}", "6".repeat(25))), None);
        assert_eq!(number_from_url("/llamada?numero=600%20111%20222").as_deref(), Some("600 111 222"));
        let mut d = db();
        let n = d.calls.len();
        let a = d.log_call("600111222", "Recepción");
        let b = d.log_call("+34 600 111 222", "Recepción");
        assert_eq!((a, d.calls.len()), (b, n + 1), "el mismo aviso repetido");
        d.calls.iter_mut().find(|c| c.id == a).unwrap().when -= chrono::Duration::seconds(REPEAT_SECS + 1);
        assert_ne!(d.log_call("600111222", "Recepción"), a, "pasado un rato, es otra llamada");
    }

    /// Una página web abierta en el navegador no puede hacer saltar avisos
    /// de llamada (la petición lleva «Sec-Fetch-Site: cross-site»), y una
    /// avalancha se corta.
    #[test]
    fn web_pages_cannot_fake_calls_and_floods_are_cut() {
        let (port, rx) = listen(0, || {}).unwrap();
        let get = |extra: Option<(&str, &str)>| {
            let mut r = ureq::get(&format!("http://127.0.0.1:{port}/llamada?numero=600111222"));
            if let Some((k, v)) = extra {
                r = r.set(k, v);
            }
            match r.call() {
                Ok(r) => r.status(),
                Err(ureq::Error::Status(c, _)) => c,
                Err(_) => 0,
            }
        };
        assert_eq!(get(Some(("Sec-Fetch-Site", "cross-site"))), 403);
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "no llega");
        assert_eq!(get(Some(("Sec-Fetch-Site", "none"))), 200, "escrita a mano en el navegador, sí");
        let codes: Vec<u16> = (0..MAX_PER_MINUTE + 2).map(|_| get(None)).collect();
        assert!(codes.contains(&429), "{codes:?}");
    }

    #[test]
    fn local_listener_receives_calls() {
        let (port, rx) = listen(0, || {}).unwrap();
        forward("+34 610 200 100", port).unwrap();
        let got = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(same_number(&got, "610200100"), "{got}");
    }
}
