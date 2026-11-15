//! Enlaces con los programas de los aparatos de imagen (radiovisiógrafo,
//! ortopantomógrafo, TAC, cámara, escáner): con qué datos del paciente se abre
//! cada programa, y en qué forma los espera.
//!
//! Cada fabricante los pide a su manera, y el enlace se ajusta sin tocar código:
//! - **Parámetros** con comodines. Hay dos juegos: uno para capturar y otro,
//!   opcional, para ver las imágenes que ya hay.
//! - **Formatos:** la fecha como la quiera el programa (`[FECNAC:DD,MM,AAAA]`),
//!   el sexo con sus letras (`[SEXO:MF]`), los nombres en mayúsculas o sin
//!   espacios (`[APELPAC:MAYUS+SINESPACIOS]`).
//! - **Un fichero puente:** el `IMCONECT.ini` de siempre o el de una plantilla
//!   propia (por ejemplo, el fichero de texto de Dexis). Se escribe en la
//!   codificación que lea el programa: los de Windows suelen leer los `.ini` en
//!   ANSI (Windows-1252), y en UTF-8 un «MARTÍNEZ» les llegaría como «MARTÃNEZ».
//!
//! Lo que viene del paciente nunca añade opciones al programa: en los
//! parámetros pasa por [`arg_safe`], y en el fichero por [`ini_safe`] (sin
//! saltos de línea, que añadirían claves).

use crate::model::{Connector, Db, arg_safe, ini_safe};
use serde::{Deserialize, Serialize};

/// Codificación del fichero puente.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Encoding {
    /// Windows-1252, la de los programas de Windows que leen ficheros `.ini`.
    #[default]
    Ansi,
    Utf8,
    /// UTF-16 con marca de orden (la otra que entienden los `.ini` de Windows).
    Utf16,
}

impl Encoding {
    pub const ALL: [Encoding; 3] = [Encoding::Ansi, Encoding::Utf8, Encoding::Utf16];
    pub fn label(self) -> &'static str {
        match self {
            Encoding::Ansi => "ANSI (Windows-1252)",
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf16 => "UTF-16",
        }
    }
}

/// Los caracteres de Windows-1252 entre 0x80 y 0x9F (los demás de 0xA0 a
/// 0xFF coinciden con Unicode).
const CP1252_HIGH: [(char, u8); 27] = [
    ('€', 0x80),
    ('‚', 0x82),
    ('ƒ', 0x83),
    ('„', 0x84),
    ('…', 0x85),
    ('†', 0x86),
    ('‡', 0x87),
    ('ˆ', 0x88),
    ('‰', 0x89),
    ('Š', 0x8A),
    ('‹', 0x8B),
    ('Œ', 0x8C),
    ('Ž', 0x8E),
    ('‘', 0x91),
    ('’', 0x92),
    ('“', 0x93),
    ('”', 0x94),
    ('•', 0x95),
    ('–', 0x96),
    ('—', 0x97),
    ('˜', 0x98),
    ('™', 0x99),
    ('š', 0x9A),
    ('›', 0x9B),
    ('œ', 0x9C),
    ('ž', 0x9E),
    ('Ÿ', 0x9F),
];

/// El texto en la codificación elegida. En ANSI, lo que no existe en
/// Windows-1252 sale como «?».
pub fn encode(text: &str, enc: Encoding) -> Vec<u8> {
    match enc {
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Utf16 => {
            let mut out = vec![0xFF, 0xFE];
            for u in text.encode_utf16() {
                out.extend(u.to_le_bytes());
            }
            out
        }
        Encoding::Ansi => text
            .chars()
            .map(|c| match c as u32 {
                0..=0x7F | 0xA0..=0xFF => c as u32 as u8,
                _ => CP1252_HIGH.iter().find(|(k, _)| *k == c).map(|(_, b)| *b).unwrap_or(b'?'),
            })
            .collect(),
    }
}

/// Dónde va el texto: en la línea de órdenes del programa o en el fichero puente.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ctx {
    Args,
    File,
}

/// Para qué se abre el programa del aparato.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Una toma nueva.
    Capture,
    /// Las imágenes que ya tiene el paciente (con los parámetros de ver, si los hay).
    View,
}

/// La ayuda de los comodines, tal como se enseña al configurar un enlace.
pub const WILDCARDS: &str = "Comodines: [IDPAC] [NUMPAC] [NOMPAC] [APELPAC] [NOMBRE] [NIF] [FECNAC] [SEXO] [CARPETA] [FICHERO]. \
Formatos: [FECNAC:AAAAMMDD], [FECNAC:DD.MM.AAAA]…; [SEXO:MF] (hombre M, mujer F); en los nombres, :MAYUS y :SINESPACIOS (juntos, con +).";

/// La fecha con un patrón de AAAA, AA, MM, M, DD y D; lo demás se copia tal cual.
fn date_with(d: chrono::NaiveDate, pattern: &str) -> String {
    use chrono::Datelike;
    let mut out = String::new();
    let mut rest = pattern;
    while !rest.is_empty() {
        let (piece, n) = if rest.starts_with("AAAA") {
            (format!("{:04}", d.year()), 4)
        } else if rest.starts_with("AA") {
            (format!("{:02}", d.year() % 100), 2)
        } else if rest.starts_with("MM") {
            (format!("{:02}", d.month()), 2)
        } else if rest.starts_with('M') {
            (d.month().to_string(), 1)
        } else if rest.starts_with("DD") {
            (format!("{:02}", d.day()), 2)
        } else if rest.starts_with('D') {
            (d.day().to_string(), 1)
        } else {
            let c = rest.chars().next().unwrap_or_default();
            (c.to_string(), c.len_utf8())
        };
        out.push_str(&piece);
        rest = &rest[n..];
    }
    out
}

/// El valor de un comodín (`None` si no es un comodín conocido: se deja tal cual).
fn wildcard(db: &Db, name: &str, fmt: Option<&str>, pid: u32, ctx: Ctx, bridge: &str) -> Option<String> {
    let p = db.patient(pid).cloned().unwrap_or_default();
    let female = p.sex == "F";
    let text = |s: &str| {
        let mut v = match ctx {
            Ctx::Args => arg_safe(s),
            Ctx::File => ini_safe(s),
        };
        for m in fmt.unwrap_or("").split('+').map(str::trim) {
            match m {
                "MAYUS" => v = v.to_uppercase(),
                "SINESPACIOS" => v.retain(|c| c != ' '),
                _ => {}
            }
        }
        v
    };
    Some(match name {
        "IDPAC" => p.id.to_string(),
        "NUMPAC" => p.history_no.to_string(),
        "NOMPAC" => text(&p.first_name),
        "APELPAC" => text(&p.last_name),
        "NOMBRE" => text(&format!("{} {}", p.first_name, p.last_name)),
        "NIF" => text(&p.nif),
        "FECNAC" => p.birth.map(|d| date_with(d, fmt.unwrap_or("DD/MM/AAAA"))).unwrap_or_default(),
        // como el original: H hombre, M mujer; con formato, la letra del hombre y la de la mujer
        "SEXO" => {
            let codes: Vec<char> = fmt.unwrap_or("HM").chars().collect();
            let (male, female_code) = if codes.len() == 2 { (codes[0], codes[1]) } else { ('H', 'M') };
            (if female { female_code } else { male }).to_string()
        }
        "CARPETA" => db.patient_image_dir(pid).display().to_string(),
        "FICHERO" => bridge.to_string(),
        _ => return None,
    })
}

/// Una plantilla con sus comodines rellenos para el paciente. `bridge` es la
/// ruta del fichero puente (para `[FICHERO]`).
pub fn expand(db: &Db, template: &str, pid: u32, ctx: Ctx, bridge: &str) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find(']') {
            Some(close) => {
                let token = &after[..close];
                let (name, fmt) = match token.split_once(':') {
                    Some((n, f)) => (n, Some(f)),
                    None => (token, None),
                };
                match wildcard(db, name, fmt, pid, ctx, bridge) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&rest[open..open + close + 2]),
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Los parámetros con los que se abre el programa del enlace.
pub fn args_for(db: &Db, c: &Connector, pid: u32, action: Action, bridge: &str) -> String {
    let template = if action == Action::View && !c.view_args.trim().is_empty() { &c.view_args } else { &c.args };
    expand(db, template, pid, Ctx::Args, bridge)
}

/// El texto del fichero puente: el de la plantilla del enlace, o `IMCONECT.ini`.
/// Con saltos de línea de Windows.
pub fn bridge_text(db: &Db, c: &Connector, pid: u32, bridge: &str) -> String {
    if c.bridge_template.trim().is_empty() {
        return db.bridge_ini(pid);
    }
    let filled = expand(db, &c.bridge_template, pid, Ctx::File, bridge);
    let mut out: String = filled.lines().map(|l| format!("{l}\r\n")).collect();
    if out.is_empty() {
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Db;

    fn db() -> Db {
        let mut db = Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 15).unwrap());
        let p = db.patient_mut(101).unwrap();
        p.first_name = "Carlos".into();
        p.last_name = "Martínez Sanz".into();
        p.birth = chrono::NaiveDate::from_ymd_opt(1972, 2, 2);
        p.sex = "M".into();
        db
    }

    #[test]
    fn the_bridge_file_reaches_windows_programs_with_its_accents() {
        assert_eq!(encode("MARTÍNEZ", Encoding::Ansi), b"MART\xCDNEZ");
        assert_eq!(encode("Ñ€ü", Encoding::Ansi), vec![0xD1, 0x80, 0xFC]);
        // lo que no cabe en Windows-1252 no rompe el fichero
        assert_eq!(encode("Łódź", Encoding::Ansi), b"?\xF3d?");
        assert_eq!(encode("Í", Encoding::Utf8), "Í".as_bytes());
        assert_eq!(encode("Í", Encoding::Utf16), vec![0xFF, 0xFE, 0xCD, 0x00]);
        // por defecto, ANSI (también en los enlaces guardados antes de que hubiera elección)
        assert_eq!(Encoding::default(), Encoding::Ansi);
    }

    #[test]
    fn each_maker_gets_the_date_and_sex_it_expects() {
        let db = db();
        let e = |t: &str| expand(&db, t, 101, Ctx::Args, "");
        assert_eq!(e("[FECNAC]"), "02/02/1972");
        assert_eq!(e("[FECNAC:AAAAMMDD]"), "19720202");
        assert_eq!(e("[FECNAC:DD.MM.AAAA]"), "02.02.1972");
        assert_eq!(e("[FECNAC:DD,MM,AAAA]"), "02,02,1972");
        assert_eq!(e("[FECNAC:M/D/AA]"), "2/2/72");
        assert_eq!(e("[SEXO]"), "H");
        assert_eq!(e("[SEXO:MF]"), "M");
        assert_eq!(e("[SEXO:12]"), "1");
        assert_eq!(e("[NUMPAC] [IDPAC]"), "1002 101");
        assert_eq!(e("[NOMBRE]"), "Carlos Martínez Sanz");
        assert_eq!(e("[APELPAC:MAYUS+SINESPACIOS]"), "MARTÍNEZSANZ");
        // lo que no es un comodín se queda como está
        assert_eq!(e("[OTRO] [NUMPAC"), "[OTRO] [NUMPAC");
    }

    #[test]
    fn a_documented_bridge_comes_out_as_its_maker_writes_it() {
        let db = db();
        // iRYS (MyRay), con los nombres sin espacios
        let irys = "/PATID \"[NUMPAC]\" /NAME \"[NOMPAC:SINESPACIOS]\" /SURNAME \"[APELPAC:SINESPACIOS]\" /BDATE \"[FECNAC:DD,MM,AAAA]\" /SEX \"[SEXO:MF]\"";
        assert_eq!(expand(&db, irys, 101, Ctx::Args, ""), "/PATID \"1002\" /NAME \"Carlos\" /SURNAME \"MartínezSanz\" /BDATE \"02,02,1972\" /SEX \"M\"");
        // Dexis: un fichero de texto que se le pasa con @
        let mut c = db.connectors[0].clone();
        c.bridge_template = "PN=[NUMPAC]\nLN=[APELPAC]\nFN=[NOMPAC]\nBD=[FECNAC:MM/DD/AA]\nSX=[SEXO:MF]".into();
        c.args = "@[FICHERO]".into();
        let text = bridge_text(&db, &c, 101, "C:\\Temp\\InfoFile.txt");
        assert_eq!(text, "PN=1002\r\nLN=Martínez Sanz\r\nFN=Carlos\r\nBD=02/02/72\r\nSX=M\r\n");
        assert_eq!(args_for(&db, &c, 101, Action::Capture, "C:\\Temp\\InfoFile.txt"), "@C:\\Temp\\InfoFile.txt");
        // sin plantilla, el IMCONECT.ini de siempre
        c.bridge_template.clear();
        assert!(bridge_text(&db, &c, 101, "").starts_with("[DATOS FILIACION]\r\nIdPac=101\r\nNumPac=1002\r\n"));
    }

    #[test]
    fn capturing_and_viewing_can_open_the_program_differently() {
        let db = db();
        let mut c = db.connectors[0].clone();
        c.args = "/capture [NUMPAC]".into();
        c.view_args = String::new();
        assert_eq!(args_for(&db, &c, 101, Action::View, ""), "/capture 1002", "sin parámetros de ver, los de capturar");
        c.view_args = "/open [NUMPAC]".into();
        assert_eq!(args_for(&db, &c, 101, Action::View, ""), "/open 1002");
        assert_eq!(args_for(&db, &c, 101, Action::Capture, ""), "/capture 1002");
    }

    #[test]
    fn patient_data_cannot_break_the_command_or_the_file() {
        let mut db = db();
        let p = db.patient_mut(101).unwrap();
        p.first_name = "Ana\" /DELETE, otra".into();
        p.last_name = "García\r\nNumPac=999".into();
        // en la línea de órdenes: ni comillas, ni barras, ni comas (rompen los parámetros de algunos programas)
        assert_eq!(expand(&db, "/NAME \"[NOMPAC]\"", 101, Ctx::Args, ""), "/NAME \"Ana DELETE otra\"");
        // en el fichero: ni saltos de línea
        let mut c = db.connectors[0].clone();
        c.bridge_template = "LN=[APELPAC]\nFN=[NOMPAC]".into();
        let text = bridge_text(&db, &c, 101, "");
        assert_eq!(text.matches("\r\n").count(), 2, "{text}");
        assert!(text.starts_with("LN=García NumPac=999\r\n"), "{text}");
    }
}
