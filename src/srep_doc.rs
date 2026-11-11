//! Hoja de Información al Paciente (HIP) de la receta electrónica privada:
//! datos legibles, un código DataMatrix por receta y el código de barras del
//! sistema de prescripción («Diseño técnico e-receta privada – Servicios
//! Repositorios v2.04.1», apartado 2.2).
//!
//! Contenido del DataMatrix (ECC 200, campos con identificador como en GS1;
//! los de longitud variable terminan en «!»):
//! 08 IdRepositorio(32) · 09 IdAcceso(32) · 10 IdReceta(32) · 11 Código
//! nacional(7) · 12 Principio activo(..40!) · 13 Composición(..40!) ·
//! 14 Denominación(..60!) · 15 Fecha inicio DDMMAA · 16 Fecha fin DDMMAA ·
//! 17 Envases(..3!) · 18 Estupefaciente(1) · 19 Psicótropo(1).

use crate::srep::{RPrescripcion, RProducto};
use chrono::NaiveDate;

/// Texto apto para el DataMatrix: mayúsculas sin tildes, sin el separador «!».
fn dm_text(s: &str, max: usize) -> String {
    let folded: String = s
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'Á' | 'À' => 'A',
            'é' | 'è' | 'É' | 'È' => 'E',
            'í' | 'Í' => 'I',
            'ó' | 'ò' | 'Ó' | 'Ò' => 'O',
            'ú' | 'ü' | 'Ú' | 'Ü' => 'U',
            'ñ' | 'Ñ' => 'N',
            'ç' | 'Ç' => 'C',
            c => c.to_ascii_uppercase(),
        })
        .filter(|c| c.is_ascii() && !c.is_ascii_control() && *c != '!')
        .collect();
    folded.trim().chars().take(max).collect()
}

fn ddmmaa(d: NaiveDate) -> String {
    d.format("%d%m%y").to_string()
}

/// Contenido del DataMatrix de una receta.
#[allow(clippy::too_many_arguments)]
pub fn dm_content(id_rep: &str, id_acc: &str, id_receta: &str, p: &RProducto, ini: NaiveDate, fin: NaiveDate, envases: u32) -> String {
    let mut s = format!("08{id_rep}09{id_acc}10{id_receta}");
    // el código nacional va con su dígito de control (7 cifras)
    let cn = crate::nomenclator::cn7(&p.cod_producto);
    if !cn.is_empty() {
        s.push_str(&format!("11{cn:0>7}"));
    }
    if !p.principio_activo.trim().is_empty() {
        s.push_str(&format!("12{}!", dm_text(&p.principio_activo, 40)));
    }
    if !p.composicion.trim().is_empty() {
        s.push_str(&format!("13{}!", dm_text(&p.composicion, 40)));
    }
    s.push_str(&format!("14{}!", dm_text(&p.denominacion, 60)));
    s.push_str(&format!("15{}16{}17{}!18{}19{}", ddmmaa(ini), ddmmaa(fin), envases.min(999), p.estupefaciente as u8, p.psicotropo as u8));
    s
}

/// DataMatrix ECC 200 como SVG (módulos de `module` px y zona en blanco de 2 módulos).
pub fn datamatrix_svg(content: &str, module: f32) -> Result<String, String> {
    let code = datamatrix::DataMatrix::encode(content.as_bytes(), datamatrix::SymbolList::default().enforce_square()).map_err(|e| format!("No se pudo generar el DataMatrix: {e:?}"))?;
    let bm = code.bitmap();
    let (w, h) = (bm.width(), bm.height());
    let q = 2.0;
    let total_w = (w as f32 + 2.0 * q) * module;
    let total_h = (h as f32 + 2.0 * q) * module;
    let mut s = format!("<svg xmlns='http://www.w3.org/2000/svg' width='{total_w}' height='{total_h}' viewBox='0 0 {total_w} {total_h}' shape-rendering='crispEdges'><rect width='100%' height='100%' fill='#fff'/>");
    for (x, y) in bm.pixels() {
        s.push_str(&format!("<rect x='{}' y='{}' width='{module}' height='{module}'/>", (x as f32 + q) * module, (y as f32 + q) * module));
    }
    s.push_str("</svg>");
    Ok(s)
}

/// Módulos (cuadros) del DataMatrix: para dibujarlo en pantalla.
#[cfg(test)]
pub fn datamatrix_bits(content: &str) -> Option<(usize, Vec<bool>)> {
    let code = datamatrix::DataMatrix::encode(content.as_bytes(), datamatrix::SymbolList::default().enforce_square()).ok()?;
    let bm = code.bitmap();
    Some((bm.width(), bm.bits().to_vec()))
}

/// Anchos (barra, espacio, …) de los símbolos de Code 128; 106 es la parada.
const C128: [&str; 107] = [
    "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212", "221213", "221312", "231212", "112232", "122132", "122231", "113222", "123122", "123221", "223211", "221132", "221231", "213212", "223112", "312131", "311222", "321122", "321221", "312212", "322112", "322211", "212123", "212321",
    "232121", "111323", "131123", "131321", "112313", "132113", "132311", "211313", "231113", "231311", "112133", "112331", "132131", "113123", "113321", "133121", "313121", "211331", "231131", "213113", "213311", "213131", "311123", "311321", "331121", "312113", "312311", "332111", "314111", "221411", "431111", "111224",
    "111422", "121124", "121421", "141122", "141221", "112214", "112412", "122114", "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111", "111242", "121142", "121241", "114212", "124112", "124211", "411212", "421112", "421211", "212141", "214121", "412121", "111143", "111341", "131141", "114113",
    "114311", "411113", "411311", "113141", "114131", "311141", "411131", "211412", "211214", "211232", "2331112",
];

/// Símbolos de Code 128 (juego B) con su dígito de control.
pub fn code128_values(text: &str) -> Vec<usize> {
    let mut v = vec![104];
    v.extend(text.chars().filter(|c| (' '..='~').contains(c)).map(|c| c as usize - 32));
    let check = v.iter().enumerate().map(|(i, x)| if i == 0 { *x } else { x * i }).sum::<usize>() % 103;
    v.push(check);
    v.push(106);
    v
}

pub fn code128_svg(text: &str, module: f32, height: f32) -> String {
    let mut bars = vec![];
    let mut x = 10.0 * module;
    for sym in code128_values(text) {
        for (i, ch) in C128[sym].chars().enumerate() {
            let w = ch.to_digit(10).unwrap_or(1) as f32 * module;
            if i % 2 == 0 {
                bars.push(format!("<rect x='{x}' y='0' width='{w}' height='{height}'/>"));
            }
            x += w;
        }
    }
    let total = x + 10.0 * module;
    format!("<svg xmlns='http://www.w3.org/2000/svg' width='{total}' height='{height}' viewBox='0 0 {total} {height}' shape-rendering='crispEdges'><rect width='100%' height='100%' fill='#fff'/>{}</svg>", bars.concat())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\'', "&#39;")
}

/// Datos fijos de la hoja (clínica, sistema y paciente).
pub struct HipHeader<'a> {
    pub clinic: &'a str,
    pub clinic_address: &'a str,
    pub clinic_phone: &'a str,
    pub id_repositorio: &'a str,
    pub id_entidad: &'a str,
    pub id_sistema: &'a str,
    pub test_mode: bool,
    pub confidential: bool,
    pub signer: &'a str,
}

/// «IDXXXX/idSistema»: lo lee Nodofarma para validar el sistema de prescripción.
pub fn system_barcode_text(id_entidad: &str, id_sistema: &str) -> String {
    format!("ID{}/{}", id_entidad.trim(), id_sistema.trim())
}

/// ID_HIP: 6 caracteres propios de la hoja + IDXXXX.
pub fn hip_id(seed: &str, id_entidad: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(seed.as_bytes());
    let alphabet = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let six: String = h.iter().take(6).map(|b| alphabet[(*b as usize) % alphabet.len()] as char).collect();
    format!("{six}ID{}", id_entidad.trim())
}

pub(crate) fn posologia(p: &RPrescripcion) -> String {
    let n = |x: f64| if x.fract() == 0.0 { format!("{}", x as i64) } else { format!("{x}").replace('.', ",") };
    format!("{} {} {} veces al {} durante {} {}", n(p.toma), p.ud_toma, n(p.frecuencia), p.ud_frecuencia, p.duracion, p.ud_duracion)
}

/// Hoja de Información al Paciente lista para imprimir o enviar (HTML).
pub fn hip_html(h: &HipHeader, list: &[RPrescripcion]) -> String {
    let Some(first) = list.first() else { return String::new() };
    let pa = &first.paciente;
    let pr = &first.prescriptor;
    let id_acc = &first.id_acceso;
    let hipid = hip_id(&list.iter().map(|p| p.id_prescripcion.as_str()).collect::<String>(), h.id_entidad);
    let barcode = code128_svg(&system_barcode_text(h.id_entidad, h.id_sistema), 1.0, 46.0);
    let mut rows = String::new();
    for p in list {
        let prod = &p.producto;
        let what = if !prod.cod_producto.is_empty() {
            format!("CN {} · {}", esc(&prod.cod_producto), esc(&prod.denominacion))
        } else if !prod.composicion.is_empty() {
            format!("Fórmula magistral: {}", esc(&prod.composicion))
        } else {
            format!("Principio activo {} · {}", esc(&prod.principio_activo), esc(&prod.denominacion))
        };
        for (i, r) in p.recetas.iter().enumerate() {
            let dm = datamatrix_svg(&dm_content(h.id_repositorio, id_acc, &r.id_receta, prod, r.fecha_ini, r.fecha_fin, r.num_envases), 2.2).unwrap_or_default();
            rows.push_str(&format!(
                "<tr><td class='dm'>{dm}</td><td><b>{what}</b><br>{forma}{via}<br>Posología: {pos}<br>Envases: <b>{env}</b> · Se puede recoger del <b>{ini}</b> al <b>{fin}</b>{plan}<br><small>ID.Rec: {idr}</small>{flags}{obs}</td></tr>",
                forma = esc(&format!("{} {}", prod.forma, prod.dosificacion).trim().to_string()),
                via = if prod.via.is_empty() { String::new() } else { format!(" · vía {}", esc(&prod.via)) },
                pos = esc(&posologia(p)),
                env = r.num_envases,
                ini = r.fecha_ini.format("%d/%m/%Y"),
                fin = r.fecha_fin.format("%d/%m/%Y"),
                plan = if p.recetas.len() > 1 { format!(" · receta {} de {}", i + 1, p.recetas.len()) } else { String::new() },
                idr = esc(&r.id_receta),
                flags = if prod.estupefaciente || prod.psicotropo { "<br><b>Medicamento sujeto a control especial: en la farmacia le pedirán el DNI.</b>".to_string() } else { String::new() },
                obs = if p.observaciones.trim().is_empty() { String::new() } else { format!("<br><i>Indicaciones: {}</i>", esc(&p.observaciones)) },
            ));
        }
    }
    let test = if h.test_mode { "<div class='test'>PRUEBAS — esta hoja no es válida en la farmacia</div>" } else { "" };
    let conf = if h.confidential { "<p class='conf'><b>Tratamiento confidencial:</b> en la farmacia tendrá que decir su PIN para que aparezca. No lo comparta.</p>" } else { "" };
    format!(
        "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Hoja de información al paciente</title><style>
body{{font-family:'Open Sans',Arial,sans-serif;max-width:780px;margin:28px auto;color:#1d2433;font-size:14px}}
.head{{display:flex;justify-content:space-between;border-bottom:3px solid #0e9688;padding-bottom:8px}}
h1{{font-size:20px;margin:14px 0 4px}} .ids{{font-family:monospace;font-size:12.5px;background:#f2f6f8;padding:8px;border-radius:6px}}
table{{width:100%;border-collapse:collapse;margin-top:12px}} td{{border-top:1px solid #dde3ea;padding:10px 8px;vertical-align:top}}
td.dm{{width:120px}} .test{{background:#b42318;color:#fff;font-weight:bold;text-align:center;padding:6px;border-radius:6px;margin:10px 0}}
.conf{{background:#fff4e5;padding:8px;border-radius:6px}} .legal{{font-size:11px;color:#556070;margin-top:18px}}
.bar{{margin-top:12px}}</style></head><body>
<div class='head'><div><b>{clinic}</b><br><small>{addr} · {phone}</small></div><div style='text-align:right'><small>Receta médica privada electrónica</small><br><b>{fecha}</b></div></div>
{test}<h1>Hoja de información al paciente</h1>
<p>Paciente: <b>{pac}</b> · {tipo_id} {dni} · Nacimiento {nac}<br>
Prescriptor: <b>{doc}</b> · Nº colegiado {col}{esp} · {mail} · {tel}</p>
<div class='ids'>ID.Rep: {idrep}<br>ID.Acc: {idacc}<br>ID_HIP: {hipid}</div>{conf}
<table>{rows}</table>
<div class='bar'>{barcode}<br><small>{bartext}</small></div>
<p><small>Firmado electrónicamente por {signer}.</small></p>
<p class='legal'>En la farmacia muestre esta hoja (en papel o en el móvil) y su DNI: el farmacéutico lee uno de los códigos y ve sus recetas. Protección de datos: el responsable del tratamiento es {clinic}; sus datos se usan para la prestación sanitaria y la dispensación de este tratamiento (art. 9.2.h del RGPD) y solo se comunican a la farmacia que lo dispensa a través de la red de los Consejos Generales. Puede ejercer sus derechos de acceso, rectificación, supresión, limitación y oposición en la clínica.</p>
</body></html>",
        clinic = esc(h.clinic),
        addr = esc(h.clinic_address),
        phone = esc(h.clinic_phone),
        fecha = first.fecha_prescripcion.format("%d/%m/%Y"),
        pac = esc(&format!("{} {}", pa.nombre, pa.apellidos)),
        tipo_id = if pa.tipo_id == 2 { "DNI del representante" } else { "DNI/NIE" },
        dni = esc(if pa.tipo_id == 2 { &pa.dni_nie_representante } else { &pa.dni_nie }),
        nac = esc(&pa.fecha_nacimiento),
        doc = esc(&format!("{} {}", pr.nombre, pr.apellidos)),
        col = esc(&pr.id),
        esp = if pr.especialidad.is_empty() { String::new() } else { format!(" · {}", esc(&pr.especialidad)) },
        mail = esc(&pr.email),
        tel = esc(&pr.telefono),
        idrep = esc(h.id_repositorio),
        idacc = esc(id_acc),
        bartext = esc(&system_barcode_text(h.id_entidad, h.id_sistema)),
        signer = esc(h.signer),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datamatrix_content_matches_the_standard_example() {
        // Ejemplo del «Diseño técnico e-receta privada – Servicios Repositorios v2.04.1» (2.2.2)
        let id = "12345678912345678912345678912345";
        let p = RProducto { cod_producto: "8188727".into(), denominacion: "ABSORCOL 10MG 28 COMPRIMIDOS".into(), ..Default::default() };
        let c = dm_content(id, id, id, &p, NaiveDate::from_ymd_opt(2020, 10, 5).unwrap(), NaiveDate::from_ymd_opt(2020, 10, 21).unwrap(), 15);
        let ejemplo = "0812345678912345678912345678912345091234567891234567891234567891234510123456789123".to_string() + "4567891234567891234511818872714ABSORCOL 10MG 28 COMPRIMIDOS!" + "15051020162110201715!180190";
        assert_eq!(c, ejemplo);
    }

    #[test]
    fn datamatrix_texts_are_scanner_safe() {
        let p = RProducto { principio_activo: "Ibuprofeno".into(), denominacion: "Ibuprofeno 600 mg ¡comprimidos!".into(), estupefaciente: true, ..Default::default() };
        let id = "a".repeat(32);
        let c = dm_content(&id, &id, &id, &p, NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(), NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(), 2);
        assert!(c.contains("12IBUPROFENO!14IBUPROFENO 600 MG COMPRIMIDOS!"), "{c}");
        assert!(c.ends_with("1506112616161126172!181190"), "{c}");
        assert!(c.is_ascii());
    }

    #[test]
    fn datamatrix_round_trip() {
        let p = RProducto { cod_producto: "654321".into(), denominacion: "AMOXICILINA 500 MG 30 CAPSULAS".into(), ..Default::default() };
        let id = crate::srep::new_id32();
        let content = dm_content(&id, &id, &id, &p, NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(), NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(), 1);
        assert!(content.contains("116543214"), "el código nacional lleva su dígito de control");
        let (w, bits) = datamatrix_bits(&content).unwrap();
        let back = datamatrix::DataMatrix::decode(&bits, w).unwrap();
        assert_eq!(String::from_utf8(back).unwrap(), content);
        assert!(datamatrix_svg(&content, 2.0).unwrap().starts_with("<svg"));
    }

    #[test]
    fn code128_table_and_check_digit() {
        for (i, p) in C128.iter().enumerate() {
            let sum: u32 = p.chars().map(|c| c.to_digit(10).unwrap()).sum();
            assert_eq!(sum, if i == 106 { 13 } else { 11 }, "símbolo {i}");
        }
        let mut uniq = C128.to_vec();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), 107);
        // «Wikipedia»: control 88
        let v = code128_values("Wikipedia");
        assert_eq!(v[v.len() - 2], 88);
        assert_eq!(system_barcode_text("1234", "abc"), "ID1234/abc");
        assert!(hip_id("x", "1234").ends_with("ID1234") && hip_id("x", "1234").len() == 12);
    }
}
