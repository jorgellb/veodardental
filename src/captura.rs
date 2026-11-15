//! Capturas para la web (`--screenshot` con `--zonas-privadas`): el programa
//! apunta dónde dibuja algo con forma de teléfono, DNI, NIE, CIF o IBAN, y lo
//! guarda junto a la captura (`captura.png.zonas.json`, en píxeles) para
//! difuminarlo después. Los datos de ejemplo son inventados, pero tienen la
//! forma de los reales y podrían coincidir con los de alguien.

use egui::epaint::text::cursor::CCursor;
use egui::{Context, FullOutput, Rect, Shape};
use std::path::Path;
use std::sync::Mutex;

/// Las zonas del último fotograma (`None` si no se ha pedido).
static ZONES: Mutex<Option<Vec<[f32; 4]>>> = Mutex::new(None);

/// Tramos (en caracteres, `inicio..fin`) de un texto con forma de teléfono
/// español (9 cifras en grupos de 2 o más, empezando por 6, 7, 8 o 9; o con el
/// 34 delante), DNI, NIE, CIF o IBAN español.
pub fn sensitive_spans(text: &str) -> Vec<(usize, usize)> {
    let c: Vec<char> = text.chars().collect();
    let alnum = |i: usize| c.get(i).is_some_and(|x| x.is_ascii_alphanumeric());
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if !alnum(i) || (i > 0 && alnum(i - 1)) {
            i += 1;
            continue;
        }
        // cifras en grupos separados por un espacio
        let (mut j, mut digits, mut groups, mut shortest, mut run) = (i, 0, 0, usize::MAX, 0);
        while j < c.len() && c[j].is_ascii_digit() {
            digits += 1;
            run += 1;
            j += 1;
            let more = j + 1 < c.len() && c[j] == ' ' && c[j + 1].is_ascii_digit();
            if j >= c.len() || !c[j].is_ascii_digit() {
                groups += 1;
                shortest = shortest.min(run);
                run = 0;
                if more {
                    j += 1;
                }
            }
        }
        // el final de la última cifra (sin el espacio que se haya saltado)
        let end = if j > i && c[j - 1] == ' ' { j - 1 } else { j };
        if end > i && !alnum(end) && shortest >= 2 && groups >= 1 {
            let first = c[i];
            let phone = (digits == 9 && matches!(first, '6'..='9')) || (digits == 11 && first == '3' && c.get(i + 1) == Some(&'4'));
            if phone {
                out.push((i, end));
                i = end;
                continue;
            }
        }
        // una palabra de letras y cifras
        let mut k = i;
        while alnum(k) {
            k += 1;
        }
        let w: Vec<char> = c[i..k].to_vec();
        let digits_in = |r: std::ops::Range<usize>| r.into_iter().all(|n| w[n].is_ascii_digit());
        let upper = |n: usize| w[n].is_ascii_uppercase();
        let id = match w.len() {
            9 => (digits_in(0..8) && upper(8)) || (matches!(w[0], 'X' | 'Y' | 'Z') && digits_in(1..8) && upper(8)) || ("ABCDEFGHJNPQRSUVW".contains(w[0]) && digits_in(1..8) && (w[8].is_ascii_digit() || upper(8))),
            _ => false,
        };
        if id {
            out.push((i, k));
            i = k;
            continue;
        }
        // IBAN: ES y dos cifras, y después grupos de cifras hasta 24 caracteres
        if w.len() >= 4 && w[0] == 'E' && w[1] == 'S' && digits_in(2..w.len()) {
            let (mut e, mut n) = (k, w.len());
            while e + 1 < c.len() && c[e] == ' ' && c[e + 1].is_ascii_digit() {
                e += 1;
                while e < c.len() && c[e].is_ascii_digit() {
                    e += 1;
                    n += 1;
                }
            }
            if n >= 20 && !alnum(e) {
                out.push((i, e));
                i = e;
                continue;
            }
        }
        i = k;
    }
    out
}

/// Recoge las zonas que hay que difuminar en lo que egui va a dibujar.
pub struct Zones;

impl egui::Plugin for Zones {
    fn debug_name(&self) -> &'static str {
        "zonas privadas de la captura"
    }

    fn output_hook(&mut self, _ctx: &Context, output: &mut FullOutput) {
        let ppp = output.pixels_per_point;
        let mut found = Vec::new();
        for s in &output.shapes {
            collect(&s.shape, s.clip_rect, &mut found);
        }
        let px = found.into_iter().map(|r| [r.min.x * ppp, r.min.y * ppp, r.max.x * ppp, r.max.y * ppp]).collect();
        *ZONES.lock().unwrap_or_else(|e| e.into_inner()) = Some(px);
    }
}

fn collect(shape: &Shape, clip: Rect, out: &mut Vec<Rect>) {
    match shape {
        Shape::Vec(v) => {
            for s in v {
                collect(s, clip, out);
            }
        }
        Shape::Text(t) => {
            for (a, b) in sensitive_spans(t.galley.text()) {
                let (r0, r1) = (t.galley.pos_from_cursor(CCursor::new(a)), t.galley.pos_from_cursor(CCursor::new(b)));
                // si el tramo pasa de una línea a otra, se cubren las dos enteras
                let r = if (r0.min.y - r1.min.y).abs() < 0.5 { Rect::from_min_max(r0.min, r1.max) } else { Rect::from_min_max(egui::pos2(t.galley.rect.min.x, r0.min.y), egui::pos2(t.galley.rect.max.x, r1.max.y)) };
                let r = r.translate(t.pos.to_vec2()).expand(3.0).intersect(clip);
                if r.is_positive() {
                    out.push(r);
                }
            }
        }
        _ => {}
    }
}

/// Junto a la captura, sus zonas privadas (si se pidieron con `--zonas-privadas`).
pub fn save_next_to(png: &Path) {
    let zones = ZONES.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(z) = zones {
        let path = png.with_extension("png.zonas.json");
        if let Err(e) = std::fs::write(&path, serde_json::to_string(&z).unwrap_or_default()) {
            eprintln!("No se pudieron guardar las zonas en {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sensitive_spans;

    fn found(text: &str) -> Vec<String> {
        let c: Vec<char> = text.chars().collect();
        sensitive_spans(text).into_iter().map(|(a, b)| c[a..b].iter().collect()).collect()
    }

    /// Lo que tiene forma de teléfono, DNI, NIE, CIF o IBAN se encuentra, con
    /// su tramo exacto; fechas, horas, importes, números de factura, huellas y
    /// sondajes no.
    #[test]
    fn phones_and_ids_are_found_and_the_rest_is_left_alone() {
        assert_eq!(found("41 años  ·  10000000Z  ·  Tarifa: Privado  ·  📞 610 200 100"), ["10000000Z", "610 200 100"]);
        assert_eq!(found("Teléfono 91 555 10 30, centralita 900 000 001"), ["91 555 10 30", "900 000 001"]);
        assert_eq!(found("Móvil 622000111 o +34 622 000 111"), ["622000111", "34 622 000 111"]);
        assert_eq!(found("NIE X1234567L, CIF B00000000 y A1234567J"), ["X1234567L", "B00000000", "A1234567J"]);
        assert_eq!(found("Cuenta ES91 2100 0418 4502 0005 1332."), ["ES91 2100 0418 4502 0005 1332"]);
        for nothing in ["Citas de hoy 18", "15/11/2026 09:30", "1.875,00 €", "F2026-00003", "Nº historial 1001", "7292DB865767…", "3 2 3 3 2 3 4 3 2", "Total 2026 10 30", "lun 16/11 · 10:00", "A10000000Z"] {
            assert!(found(nothing).is_empty(), "{nothing}: {:?}", found(nothing));
        }
    }
}
