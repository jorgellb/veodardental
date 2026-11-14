//! Periodontograma por voz (T-F5-05): el dictado (transcrito o escrito con el
//! dictado del móvil) se interpreta en cambios del periodontograma, que se ven
//! uno a uno y solo se aplican al confirmarlos. **Nada dudoso se aplica**: se
//! señala para corregirlo y volver a interpretar.
//!
//! Lo que se entiende (en palabras o en cifras):
//! - **pieza** (FDI): «dieciocho», «pieza 18», «uno ocho»;
//! - **lado:** «vestibular», «palatino» o «lingual»;
//! - **sondaje:** tres números tras el lado: distal, centro y mesial;
//! - **recesión:** «recesión» y tres números;
//! - **sangrado / placa:** «sangrado» o «placa» y los puntos («distal»,
//!   «centro», «mesial» o «todos»; sin puntos, los tres del lado);
//! - **movilidad / furca:** «movilidad 1», «furca 2» (de 0 a 3);
//! - **ausente:** la pieza no está.
//!
//! Es dudoso, y se señala sin aplicarse:
//! - un sondaje de menos o de más de tres números;
//! - una profundidad mayor de 12 mm;
//! - una palabra que no se entiende en medio de unas cifras;
//! - una pieza que no existe;
//! - datos antes de decir la pieza o el lado;
//! - el mismo punto dictado dos veces con valores distintos.
//!
//! Las muletillas («eh», «vale», «bueno»…) se ignoran.

use crate::model::{PerioChart, PerioTooth, fold};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Vestibular,
    Interno,
}

impl Side {
    fn base(self) -> usize {
        match self {
            Side::Vestibular => 0,
            Side::Interno => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum What {
    Depth([u8; 3]),
    Recession([u8; 3]),
    /// Puntos del lado (0 distal, 1 centro, 2 mesial).
    Bleeding(Vec<usize>),
    Plaque(Vec<usize>),
    Mobility(u8),
    Furcation(u8),
    Missing,
}

/// Un dato entendido.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub tooth: u8,
    pub side: Option<Side>,
    pub what: What,
}

/// Algo dudoso: no se aplica.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    /// El trozo del dictado.
    pub near: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parsed {
    pub readings: Vec<Reading>,
    pub issues: Vec<Issue>,
}

const FILLERS: [&str; 12] = ["eh", "em", "mm", "vale", "bueno", "pues", "a", "ver", "y", "la", "el", "de"];

fn number(w: &str) -> Option<u32> {
    if let Ok(n) = w.parse::<u32>() {
        return Some(n);
    }
    let words = ["cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve", "diez", "once", "doce", "trece", "catorce", "quince", "dieciseis", "diecisiete", "dieciocho", "diecinueve", "veinte"];
    if let Some(i) = words.iter().position(|x| *x == w) {
        return Some(i as u32);
    }
    if w == "un" {
        return Some(1);
    }
    // veintiuno…, treinta y…, cuarenta y… (con «y» separado, ver `tooth_words`)
    let tens = [("veinti", 20), ("treinta", 30), ("cuarenta", 40)];
    for (p, base) in tens {
        if let Some(rest) = w.strip_prefix(p) {
            if rest.is_empty() {
                return Some(base);
            }
            return words.iter().take(10).position(|x| *x == rest).map(|i| base + i as u32);
        }
    }
    None
}

fn valid_tooth(n: u32) -> bool {
    matches!(n / 10, 1..=4) && matches!(n % 10, 1..=8)
}

fn point(w: &str) -> Option<Vec<usize>> {
    Some(match w {
        "distal" => vec![0],
        "centro" | "medio" | "central" => vec![1],
        "mesial" => vec![2],
        "todos" | "todo" => vec![0, 1, 2],
        _ => return None,
    })
}

/// Palabras del dictado, sin acentos ni signos; «treinta y seis» → «treintaseis».
fn words(text: &str) -> Vec<String> {
    let raw: Vec<String> = fold(text).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect();
    let mut out: Vec<String> = vec![];
    let mut i = 0;
    while i < raw.len() {
        let w = &raw[i];
        if (w == "treinta" || w == "cuarenta") && raw.get(i + 1).map(String::as_str) == Some("y") && raw.get(i + 2).and_then(|x| number(x)).is_some_and(|n| n < 10) {
            out.push(format!("{w}{}", raw[i + 2]));
            i += 3;
            continue;
        }
        out.push(w.clone());
        i += 1;
    }
    out
}

/// Interpreta un dictado.
pub fn parse(text: &str) -> Parsed {
    let w = words(text);
    let mut out = Parsed::default();
    let mut tooth: Option<u8> = None;
    let mut side: Option<Side> = None;
    let near = |i: usize| w[i.saturating_sub(2)..(i + 3).min(w.len())].join(" ");
    let mut i = 0;
    // números seguidos desde `i` (para el sondaje y la recesión)
    let take_numbers = |i: usize| -> (Vec<u32>, usize) {
        let mut v = vec![];
        let mut j = i;
        while j < w.len() {
            if let Some(n) = number(&w[j]) {
                // tras tres valores, un número de pieza empieza la siguiente
                if v.len() == 3 && n > 12 {
                    break;
                }
                v.push(n);
                j += 1;
            } else if FILLERS.contains(&w[j].as_str()) {
                j += 1;
            } else {
                break;
            }
        }
        (v, j)
    };
    while i < w.len() {
        let x = w[i].as_str();
        if FILLERS.contains(&x) {
            i += 1;
            continue;
        }
        if x == "pieza" || x == "diente" {
            i += 1;
            continue;
        }
        // pieza: un número de pieza fuera de un sondaje; dicha cifra a cifra
        // («uno ocho») solo tras «pieza», para que unas cifras sueltas no
        // pasen por una pieza
        if let Some(n) = number(x) {
            let after_word = i > 0 && matches!(w[i - 1].as_str(), "pieza" | "diente");
            let pair = if after_word && n < 10 { w.get(i + 1).and_then(|y| number(y)).filter(|m| *m < 10).map(|m| n * 10 + m) } else { None };
            let (t, used) = match pair {
                Some(t) => (t, 2),
                None => (n, 1),
            };
            if valid_tooth(t) {
                tooth = Some(t as u8);
                side = None;
            } else {
                // una pieza que no existe: lo que venga después no se atribuye
                // a la anterior; una cifra suelta (restos de un sondaje mal
                // entendido) solo se señala
                if t >= 11 {
                    (tooth, side) = (None, None);
                }
                out.issues.push(Issue { near: near(i), reason: format!("«{}» no es una pieza (FDI, de 11 a 48) ni va tras un lado o un dato.", w[i..i + used].join(" ")) });
            }
            i += used;
            continue;
        }
        let Some(t) = tooth else {
            out.issues.push(Issue { near: near(i), reason: "Falta decir la pieza antes de los datos.".into() });
            i += 1;
            continue;
        };
        match x {
            "vestibular" | "bucal" => {
                side = Some(Side::Vestibular);
                let (v, j) = take_numbers(i + 1);
                if !v.is_empty() {
                    depth(&mut out, t, Side::Vestibular, &v, &near(i));
                }
                i = j;
            }
            "palatino" | "palatina" | "lingual" => {
                side = Some(Side::Interno);
                let (v, j) = take_numbers(i + 1);
                if !v.is_empty() {
                    depth(&mut out, t, Side::Interno, &v, &near(i));
                }
                i = j;
            }
            "recesion" | "recesiones" => {
                let (v, j) = take_numbers(i + 1);
                match side {
                    Some(s) if v.len() == 3 && v.iter().all(|n| *n <= 12) => out.readings.push(Reading { tooth: t, side: Some(s), what: What::Recession([v[0] as u8, v[1] as u8, v[2] as u8]) }),
                    Some(_) => out.issues.push(Issue { near: near(i), reason: format!("La recesión necesita tres valores de 0 a 12 y hay {}.", v.len()) }),
                    None => out.issues.push(Issue { near: near(i), reason: "Falta decir el lado (vestibular o palatino/lingual).".into() }),
                }
                i = j;
            }
            "sangrado" | "sangra" | "placa" => {
                let mut pts: Vec<usize> = vec![];
                let mut j = i + 1;
                while let Some(p) = w.get(j).and_then(|y| point(y)) {
                    pts.extend(p);
                    j += 1;
                }
                if pts.is_empty() {
                    pts = vec![0, 1, 2];
                }
                pts.sort();
                pts.dedup();
                match side {
                    Some(s) => out.readings.push(Reading { tooth: t, side: Some(s), what: if x == "placa" { What::Plaque(pts) } else { What::Bleeding(pts) } }),
                    None => out.issues.push(Issue { near: near(i), reason: "Falta decir el lado (vestibular o palatino/lingual).".into() }),
                }
                i = j;
            }
            "movilidad" | "furca" | "furcacion" => {
                match w.get(i + 1).and_then(|y| number(y)) {
                    Some(n) if n <= 3 => out.readings.push(Reading { tooth: t, side: None, what: if x == "movilidad" { What::Mobility(n as u8) } else { What::Furcation(n as u8) } }),
                    _ => out.issues.push(Issue { near: near(i), reason: format!("«{x}» necesita un grado de 0 a 3.") }),
                }
                i += 2;
            }
            "ausente" | "falta" => {
                out.readings.push(Reading { tooth: t, side: None, what: What::Missing });
                i += 1;
            }
            other => {
                out.issues.push(Issue { near: near(i), reason: format!("No se entiende «{other}».") });
                i += 1;
            }
        }
    }
    conflicts(&mut out);
    out
}

fn depth(out: &mut Parsed, t: u8, s: Side, v: &[u32], near: &str) {
    if v.len() != 3 {
        out.issues.push(Issue { near: near.into(), reason: format!("El sondaje necesita tres valores (distal, centro y mesial) y hay {}.", v.len()) });
    } else if v.iter().any(|n| *n > 12) {
        out.issues.push(Issue { near: near.into(), reason: "Una profundidad de más de 12 mm: seguramente se ha entendido mal.".into() });
    } else {
        out.readings.push(Reading { tooth: t, side: Some(s), what: What::Depth([v[0] as u8, v[1] as u8, v[2] as u8]) });
    }
}

/// El mismo dato dictado dos veces con valores distintos: ninguno se aplica
/// (el sangrado y la placa solo suman puntos: no se contradicen).
fn conflicts(out: &mut Parsed) {
    let key = |r: &Reading| (r.tooth, r.side.map(|s| s.base()), std::mem::discriminant(&r.what));
    let adds = |r: &Reading| matches!(r.what, What::Bleeding(_) | What::Plaque(_));
    let mut bad: Vec<(u8, Option<usize>, std::mem::Discriminant<What>)> = vec![];
    for (a, ra) in out.readings.iter().enumerate() {
        for rb in &out.readings[a + 1..] {
            if !adds(ra) && key(ra) == key(rb) && ra != rb && !bad.contains(&key(ra)) {
                bad.push(key(ra));
            }
        }
    }
    for k in &bad {
        out.issues.push(Issue { near: format!("pieza {}", k.0), reason: "El mismo dato se ha dictado dos veces con valores distintos.".into() });
    }
    out.readings.retain(|r| !bad.contains(&key(r)));
}

/// Un cambio que se verá antes de aplicarlo.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub tooth: u8,
    pub label: String,
    pub before: String,
    pub after: String,
}

fn site_label(i: usize) -> &'static str {
    ["V distal", "V centro", "V mesial", "P/L distal", "P/L centro", "P/L mesial"][i]
}

/// Aplica las lecturas a una copia de la ficha y dice qué cambia (lo que no
/// cambia nada no sale). La ficha real solo cambia al confirmar.
pub fn apply(chart: &PerioChart, readings: &[Reading]) -> (PerioChart, Vec<Change>) {
    let mut c = chart.clone();
    let mut changes = vec![];
    for r in readings {
        if !c.teeth.iter().any(|t| t.tooth == r.tooth) {
            c.teeth.push(PerioTooth { tooth: r.tooth, ..Default::default() });
        }
        let Some(t) = c.teeth.iter_mut().find(|t| t.tooth == r.tooth) else { continue };
        let base = r.side.map(|s| s.base()).unwrap_or(0);
        let mut set = |label: String, before: String, after: String| {
            if before != after {
                changes.push(Change { tooth: r.tooth, label, before, after });
            }
        };
        match &r.what {
            What::Depth(v) | What::Recession(v) => {
                let rec = matches!(r.what, What::Recession(_));
                for (k, val) in v.iter().enumerate() {
                    let slot = if rec { &mut t.recession[base + k] } else { &mut t.depth[base + k] };
                    set(format!("{} {}", if rec { "Recesión" } else { "Sondaje" }, site_label(base + k)), slot.to_string(), val.to_string());
                    *slot = *val;
                }
            }
            What::Bleeding(pts) | What::Plaque(pts) => {
                let bleed = matches!(r.what, What::Bleeding(_));
                for k in pts {
                    let slot = if bleed { &mut t.bleeding[base + k] } else { &mut t.plaque[base + k] };
                    set(format!("{} {}", if bleed { "Sangrado" } else { "Placa" }, site_label(base + k)), if *slot { "sí" } else { "no" }.into(), "sí".into());
                    *slot = true;
                }
            }
            What::Mobility(n) => {
                set("Movilidad".into(), t.mobility.to_string(), n.to_string());
                t.mobility = *n;
            }
            What::Furcation(n) => {
                set("Furca".into(), t.furcation.to_string(), n.to_string());
                t.furcation = *n;
            }
            What::Missing => {
                set("Ausente".into(), if t.missing { "sí" } else { "no" }.into(), "sí".into());
                t.missing = true;
            }
        }
    }
    (c, changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(t: u8, s: Side, v: [u8; 3]) -> Reading {
        Reading { tooth: t, side: Some(s), what: What::Depth(v) }
    }

    #[test]
    fn a_clean_dictation_is_understood() {
        let p = parse(
            "Pieza dieciocho, vestibular tres dos tres; palatino 2 2 4, sangrado mesial. Recesión uno cero cero. Movilidad uno. Pieza uno siete, vestibular cuatro cinco cuatro, placa. Treinta y seis lingual 3 3 3, furca 2. Cuarenta y ocho ausente.",
        );
        assert!(p.issues.is_empty(), "{:?}", p.issues);
        assert_eq!(
            p.readings,
            vec![
                d(18, Side::Vestibular, [3, 2, 3]),
                d(18, Side::Interno, [2, 2, 4]),
                Reading { tooth: 18, side: Some(Side::Interno), what: What::Bleeding(vec![2]) },
                Reading { tooth: 18, side: Some(Side::Interno), what: What::Recession([1, 0, 0]) },
                Reading { tooth: 18, side: None, what: What::Mobility(1) },
                d(17, Side::Vestibular, [4, 5, 4]),
                Reading { tooth: 17, side: Some(Side::Vestibular), what: What::Plaque(vec![0, 1, 2]) },
                d(36, Side::Interno, [3, 3, 3]),
                Reading { tooth: 36, side: None, what: What::Furcation(2) },
                Reading { tooth: 48, side: None, what: What::Missing },
            ]
        );
    }

    /// Ruido y errores de transcripción: lo dudoso se señala y no se aplica;
    /// lo demás, sí.
    #[test]
    fn noise_and_transcription_errors_never_slip_in() {
        // muletillas: se ignoran
        let p = parse("eh, pieza dieciséis, vale, vestibular eh tres dos tres");
        assert!(p.issues.is_empty(), "{:?}", p.issues);
        assert_eq!(p.readings, vec![d(16, Side::Vestibular, [3, 2, 3])]);
        // «tres» mal transcrito como «tren»: el sondaje se queda corto y no se aplica
        let p = parse("dieciséis vestibular tres tren tres palatino dos dos dos");
        assert_eq!(p.readings, vec![d(16, Side::Interno, [2, 2, 2])]);
        assert!(p.issues.iter().any(|i| i.reason.contains("tres valores")));
        assert!(p.issues.iter().any(|i| i.reason.contains("tren")));
        // un número de más, una profundidad imposible, una pieza que no existe
        for bad in ["dieciséis vestibular 3 2 3 4", "dieciséis vestibular 3 22 3", "pieza diecinueve vestibular 3 2 3"] {
            let p = parse(bad);
            assert!(p.readings.iter().all(|r| !matches!(r.what, What::Depth(_))), "{bad}: {:?}", p.readings);
            assert!(!p.issues.is_empty(), "{bad}");
        }
        // tras una pieza que no existe, unas cifras sueltas no pasan por otra pieza
        let p = parse("diecinueve vestibular 3 2 3 palatino 2 2 2");
        assert!(p.readings.is_empty(), "{:?}", p.readings);
        // tres valores y la pieza siguiente sin pausa
        assert_eq!(parse("dieciocho palatino 2 2 4 diecisiete vestibular 3 3 3").readings, vec![d(18, Side::Interno, [2, 2, 4]), d(17, Side::Vestibular, [3, 3, 3])]);
        // el sangrado dictado en dos veces suma
        assert_eq!(parse("dieciséis vestibular sangrado distal, sangrado mesial").readings.len(), 2);
        // datos sin pieza o sin lado
        assert!(parse("vestibular 3 2 3").readings.is_empty());
        assert!(parse("dieciséis sangrado").issues.iter().any(|i| i.reason.contains("lado")));
        // el mismo punto dos veces con valores distintos: ninguno
        let p = parse("dieciséis vestibular 3 2 3 vestibular 4 2 3");
        assert!(p.readings.is_empty() && p.issues.iter().any(|i| i.reason.contains("dos veces")));
        // y repetido igual, una vez
        assert_eq!(parse("dieciséis vestibular 3 2 3 vestibular 3 2 3").readings.len(), 2);
        // al azar: nunca se aplica un sondaje que no sean tres valores de 0 a 12
        let vocab = ["dieciseis", "vestibular", "palatino", "tres", "dos", "veinte", "tren", "sangrado", "mesial", "eh", "movilidad", "cinco", "recesion", "pieza", "48", "99", "uno"];
        let mut rng = crate::test_rng::Rng(11);
        for _ in 0..500 {
            let n = rng.below(14) + 1;
            let text: Vec<&str> = (0..n).map(|_| vocab[rng.below(vocab.len())]).collect();
            let p = parse(&text.join(" "));
            for r in &p.readings {
                assert!(crate::screens::pacientes::valid_tooth(r.tooth), "{text:?}");
                if let What::Depth(v) | What::Recession(v) = &r.what {
                    assert!(v.iter().all(|x| *x <= 12), "{text:?}");
                }
                if let What::Mobility(x) | What::Furcation(x) = r.what {
                    assert!(x <= 3);
                }
            }
        }
    }

    #[test]
    fn changes_are_shown_before_and_only_the_copy_changes() {
        let chart = PerioChart { id: 1, patient_id: 101, date: chrono::NaiveDate::from_ymd_opt(2026, 11, 14).unwrap(), doctor_id: 1, teeth: vec![PerioTooth { tooth: 16, depth: [3, 2, 3, 0, 0, 0], ..Default::default() }], notes: String::new() };
        let p = parse("dieciséis vestibular 3 2 4, sangrado distal; veintiséis palatino 2 2 2");
        let (copy, changes) = apply(&chart, &p.readings);
        assert_eq!(chart.teeth[0].depth[2], 3, "la ficha real no cambia hasta confirmar");
        assert_eq!(copy.teeth.iter().find(|t| t.tooth == 16).unwrap().depth, [3, 2, 4, 0, 0, 0]);
        // solo sale lo que cambia: el 3 2 de antes no
        assert_eq!(changes[0], Change { tooth: 16, label: "Sondaje V mesial".into(), before: "3".into(), after: "4".into() });
        assert!(changes.iter().any(|c| c.label == "Sangrado V distal"));
        assert_eq!(changes.iter().filter(|c| c.tooth == 26).count(), 3);
    }
}
