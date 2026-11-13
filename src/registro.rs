//! Registro local de errores (T-F3-04).
//! - **Dónde:** se guarda en este ordenador (`paths::data()/registro`), solo
//!   para el usuario, con rotación: tres ficheros de 1 MB como mucho.
//! - **Minimización:** antes de escribir se quitan los datos que puedan ser
//!   de personas: correos, teléfonos y números largos, NIF/NIE, IBAN, rutas
//!   (los nombres de carpetas y ficheros suelen llevar nombres de pacientes),
//!   claves y lo que va entre comillas o «».
//! - **Envío a soporte:** nada sale de aquí solo. El informe para soporte se
//!   prepara a mano y se ve entero antes de copiarlo, guardarlo o enviarlo.
//!   Enviarlo pide una casilla de consentimiento y una dirección de soporte,
//!   que por defecto está vacía.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Tamaño máximo de cada fichero del registro.
pub const MAX_BYTES: u64 = 1 << 20;
/// Ficheros que se guardan además del actual (`errores.1.log`…).
pub const KEEP: usize = 2;
/// Largo máximo de una línea.
const MAX_LINE: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Aviso,
    Fallo,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Aviso => "AVISO",
            Level::Fallo => "FALLO",
        }
    }
}

pub fn dir() -> PathBuf {
    crate::paths::data().join("registro")
}

/// Quita del texto lo que pueda ser un dato de una persona.
pub fn scrub(text: &str) -> String {
    // lo que va entre comillas o «» suele ser lo que escribió alguien
    let mut out = String::new();
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match (quote, c) {
            (None, '«') => {
                quote = Some('»');
                out.push_str("«…");
            }
            (None, '"') => {
                quote = Some('"');
                out.push_str("\"…");
            }
            (Some(q), c) if c == q => {
                quote = None;
                out.push(c);
            }
            (Some(_), _) => {}
            (None, c) if c.is_control() => out.push(' '),
            (None, c) => out.push(c),
        }
    }
    let words: Vec<String> = out.split(' ').map(scrub_word).collect();
    let mut s = words.join(" ");
    if s.chars().count() > MAX_LINE {
        s = s.chars().take(MAX_LINE).collect::<String>() + "…";
    }
    s
}

fn scrub_word(w: &str) -> String {
    let core = w.trim_matches(|c: char| matches!(c, ',' | ';' | '.' | ':' | '(' | ')' | '[' | ']' | '\'' | '¿' | '?' | '¡' | '!'));
    if core.is_empty() {
        return w.to_string();
    }
    let digits = core.chars().filter(|c| c.is_ascii_digit()).count();
    let letters = core.chars().filter(|c| c.is_alphabetic()).count();
    let upper = core.to_uppercase();
    let nif = (core.len() == 9 && digits >= 7 && letters >= 1) || (core.len() >= 9 && upper.starts_with(['X', 'Y', 'Z']) && digits >= 7);
    let iban = core.len() >= 15 && upper.starts_with("ES") && digits >= 13;
    let path = core.contains(['/', '\\']) && core.len() > 3;
    let key = core.len() >= 20 && core.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '=')) && digits > 0 && letters > 0;
    let mark = if core.contains('@') {
        Some("<correo>")
    } else if iban {
        Some("<cuenta>")
    } else if nif {
        Some("<documento>")
    } else if path {
        Some("<ruta>")
    } else if key {
        Some("<clave>")
    } else if digits >= 6 {
        Some("<número>")
    } else {
        None
    };
    match mark {
        Some(m) => w.replacen(core, m, 1),
        None => w.to_string(),
    }
}

/// Apunta una línea (ya minimizada) en el registro de `dir`, rotando si
/// hace falta. Un fallo al apuntar no molesta a quien trabaja: se ignora.
pub fn log_in(dir: &Path, level: Level, context: &str, message: &str) {
    let _ = std::fs::create_dir_all(dir);
    let file = dir.join("errores.log");
    if std::fs::metadata(&file).is_ok_and(|m| m.len() >= MAX_BYTES) {
        rotate(dir);
    }
    let line = format!("{} {} [{}] {}\n", crate::model::now().format("%Y-%m-%dT%H:%M:%S"), level.tag(), scrub(context), scrub(message));
    if let Ok(mut f) = crate::paths::append_private(&file) {
        let _ = f.write_all(line.as_bytes());
    }
}

fn rotate(dir: &Path) {
    let name = |i: usize| if i == 0 { dir.join("errores.log") } else { dir.join(format!("errores.{i}.log")) };
    let _ = std::fs::remove_file(name(KEEP));
    for i in (0..KEEP).rev() {
        let _ = std::fs::rename(name(i), name(i + 1));
    }
}

/// Apunta en el registro de este ordenador.
pub fn log(level: Level, context: &str, message: &str) {
    if cfg!(test) {
        return;
    }
    log_in(&dir(), level, context, message);
}

/// Los fallos del programa (pánicos) también se apuntan, con dónde fue en
/// el código y el mensaje minimizado.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
        log(Level::Fallo, &place, &msg);
        previous(info);
    }));
}

/// Las últimas `n` líneas del registro (de los ficheros, del más antiguo al actual).
pub fn tail_in(dir: &Path, n: usize) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for i in (0..=KEEP).rev() {
        let f = if i == 0 { dir.join("errores.log") } else { dir.join(format!("errores.{i}.log")) };
        if let Ok(t) = std::fs::read_to_string(f) {
            lines.extend(t.lines().map(String::from));
        }
    }
    let skip = lines.len().saturating_sub(n);
    lines.split_off(skip)
}

/// Informe para soporte: versión, sistema, tamaño de los datos (cuántos, no
/// cuáles) y las últimas líneas del registro, ya minimizadas. Es lo que se
/// enseña entero antes de copiarlo, guardarlo o enviarlo.
pub fn support_report(db: &crate::model::Db, log_dir: &Path) -> String {
    let mut s = format!(
        "Informe para soporte de VeodarDental\nVersión: {} · datos {}\nSistema: {} {}\nModo: {}\nDatos: {} pacientes, {} citas, {} facturas\n\nÚltimas líneas del registro de errores (sin datos de personas):\n",
        env!("CARGO_PKG_VERSION"),
        db.data_version,
        std::env::consts::OS,
        std::env::consts::ARCH,
        if crate::shared::remote_mode() {
            "puesto de un servidor"
        } else if crate::shared::shared() {
            "carpeta compartida"
        } else {
            "un solo ordenador"
        },
        db.patients.len(),
        db.appointments.len(),
        db.invoices.len()
    );
    let tail = tail_in(log_dir, 200);
    if tail.is_empty() {
        s.push_str("(vacío)\n");
    }
    for l in tail {
        s.push_str(&l);
        s.push('\n');
    }
    s
}

/// Envía el informe por correo a soporte: solo con la casilla marcada por
/// quien lo ha visto entero y con una dirección de soporte escrita.
pub fn send_support(cfg: &crate::mail::MailConfig, to: &str, report: &str, consent: bool) -> Result<(), String> {
    if !consent {
        return Err("Marque antes que ha revisado el informe y acepta enviarlo.".into());
    }
    if !to.contains('@') {
        return Err("Escriba la dirección de correo de soporte.".into());
    }
    crate::mail::send(cfg, to.trim(), "Informe para soporte de VeodarDental", report, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ds_registro_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn personal_data_never_reaches_the_log() {
        let raw =
            "No se pudo leer /home/clinica/Pacientes/Juan Pérez/pano.jpg de «Juan Pérez» (DNI 12345678Z, ana.garcia@correo.es, 600 123 456, tel 600123456, ES9121000418450200051332, clave abcdEFGH1234ijklMNOP5678, X1234567L, C:\\Clinica\\Rx\\a.jpg)";
        let s = scrub(raw);
        for secret in ["clinica/Pacientes", "Juan Pérez", "12345678Z", "ana.garcia", "600123456", "ES9121000418450200051332", "abcdEFGH1234ijklMNOP5678", "X1234567L", "Clinica\\Rx"] {
            assert!(!s.contains(secret), "{secret} en «{s}»");
        }
        for mark in ["<ruta>", "«…»", "<documento>", "<correo>", "<número>", "<cuenta>", "<clave>"] {
            assert!(s.contains(mark), "{mark} en «{s}»");
        }
        assert!(s.starts_with("No se pudo leer"), "lo demás se entiende: {s}");
        assert_eq!(scrub("Sin conexión con el servidor (error 503)"), "Sin conexión con el servidor (error 503)");
        assert_eq!(scrub("dos\nlíneas"), "dos líneas", "una línea por apunte");
        assert!(scrub(&"x".repeat(2000)).chars().count() <= MAX_LINE + 1);
    }

    #[test]
    fn the_log_rotates_and_keeps_three_files_at_most() {
        let d = tmp("rota");
        let long = "a".repeat(400);
        for i in 0..12_000 {
            log_in(&d, Level::Error, "prueba", &format!("{i} {long}"));
        }
        let mut names: Vec<String> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        names.sort();
        assert_eq!(names, vec!["errores.1.log", "errores.2.log", "errores.log"]);
        for n in &names {
            assert!(std::fs::metadata(d.join(n)).unwrap().len() <= MAX_BYTES + 600, "{n}");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(d.join("errores.log")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let tail = tail_in(&d, 3);
        assert_eq!(tail.len(), 3);
        assert!(tail[2].contains("11999") || tail[2].contains("<número>") || tail[2].contains("ERROR [prueba]"));
        let _ = std::fs::remove_dir_all(d);
    }

    /// El informe no lleva datos de pacientes, solo cuántos, y nada se envía
    /// sin la casilla y la dirección de soporte (por defecto, ninguna).
    #[test]
    fn the_support_report_is_previewed_and_only_sent_with_consent() {
        let d = tmp("soporte");
        let db = crate::model::Db::sample(crate::model::today());
        log_in(&d, Level::Error, "guardar", &format!("No se pudo guardar la ficha de «{}» en /datos/{}", db.patients[0].full_name(), db.patients[0].nif));
        let r = support_report(&db, &d);
        assert!(r.contains(&format!("{} pacientes", db.patients.len())));
        assert!(r.contains("ERROR [guardar] No se pudo guardar la ficha de «…» en <ruta>"), "{r}");
        for p in &db.patients {
            assert!(!r.contains(&p.full_name()) && (p.nif.is_empty() || !r.contains(&p.nif)), "{}", p.full_name());
        }
        let cfg = crate::mail::MailConfig::default();
        assert!(send_support(&cfg, "soporte@ejemplo.es", &r, false).unwrap_err().contains("Marque"));
        assert!(send_support(&cfg, "", &r, true).unwrap_err().contains("dirección"));
        let _ = std::fs::remove_dir_all(d);
    }
}
