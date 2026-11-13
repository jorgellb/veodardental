//! Orden de trabajo digital para el laboratorio protésico: documento listo
//! para imprimir (con el esquema de las piezas), y paquete ZIP con la orden y
//! los escaneados, fotos y radiografías del paciente que se adjuntan.

use crate::model::{Db, ImageKind, PatientImage, fmt_date, today};
use crate::stock::LabWork;
use std::io::Write;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Iniciales del paciente (para enviar la orden sin el nombre completo).
pub fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).filter(|c| c.is_alphabetic()).map(|c| format!("{}.", c.to_uppercase())).collect::<Vec<_>>().join(" ")
}

/// Esquema FDI de la boca con las piezas del trabajo resaltadas (SVG).
pub fn tooth_chart_svg(teeth: &[u8]) -> String {
    let rows: [[u8; 16]; 2] = [[18, 17, 16, 15, 14, 13, 12, 11, 21, 22, 23, 24, 25, 26, 27, 28], [48, 47, 46, 45, 44, 43, 42, 41, 31, 32, 33, 34, 35, 36, 37, 38]];
    let (w, h, gap, mid) = (30.0, 30.0, 4.0, 14.0);
    let total_w = 16.0 * (w + gap) + mid;
    let mut s = format!("<svg xmlns='http://www.w3.org/2000/svg' width='{total_w}' height='{}' viewBox='0 0 {total_w} {}' font-family='Inter, Arial, sans-serif'>", 2.0 * h + 26.0, 2.0 * h + 26.0);
    for (r, row) in rows.iter().enumerate() {
        let y = if r == 0 { 0.0 } else { h + 26.0 };
        for (i, t) in row.iter().enumerate() {
            let x = i as f32 * (w + gap) + if i >= 8 { mid } else { 0.0 };
            let on = teeth.contains(t);
            let (fill, stroke, text) = if on { ("#0d9488", "#0f766e", "#ffffff") } else { ("#f8fafc", "#cbd5e1", "#64748b") };
            s += &format!(
                "<rect x='{x}' y='{y}' width='{w}' height='{h}' rx='7' fill='{fill}' stroke='{stroke}'/><text x='{}' y='{}' text-anchor='middle' font-size='12' font-weight='{}' fill='{text}'>{t}</text>",
                x + w / 2.0,
                y + h / 2.0 + 4.0,
                if on { 700 } else { 500 }
            );
        }
    }
    let midx = 8.0 * (w + gap) + mid / 2.0 - gap / 2.0;
    s += &format!("<line x1='{midx}' y1='0' x2='{midx}' y2='{}' stroke='#cbd5e1' stroke-dasharray='3 3'/>", 2.0 * h + 26.0);
    s += &format!("<text x='0' y='{}' font-size='10' fill='#94a3b8'>DERECHA</text><text x='{total_w}' y='{}' font-size='10' fill='#94a3b8' text-anchor='end'>IZQUIERDA</text>", h + 17.0, h + 17.0);
    s += "</svg>";
    s
}

/// Nombre de fichero seguro para el paquete.
fn safe(s: &str) -> String {
    let t: String = crate::model::fold(s).chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
    t.trim_matches('_').chars().take(60).collect()
}

/// Imágenes de la galería que lleva la orden (en el orden en que se eligieron).
pub fn attached<'a>(db: &'a Db, w: &LabWork) -> Vec<&'a PatientImage> {
    w.attachments.iter().filter_map(|id| db.images.iter().find(|i| i.id == *id && i.patient_id == w.patient_id)).collect()
}

/// Nombre con el que va cada adjunto dentro del paquete.
fn attachment_name(k: usize, img: &PatientImage) -> String {
    let ext = if img.demo { "png".to_string() } else { std::path::Path::new(&img.path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "dat".into()) };
    let teeth: Vec<String> = img.teeth.iter().map(|t| t.to_string()).collect();
    format!("{:02}_{}{}_{}.{ext}", k + 1, safe(img.kind.short()), if teeth.is_empty() { String::new() } else { format!("_{}", teeth.join("-")) }, img.date.format("%Y%m%d"))
}

/// Documento de la orden (HTML listo para imprimir o guardar en PDF).
pub fn order_html(db: &Db, w: &LabWork) -> String {
    let c = &db.config;
    let p = db.patient(w.patient_id).cloned().unwrap_or_default();
    let lab = db.suppliers.iter().find(|s| s.id == w.lab_id).cloned();
    let doctor = w.entry_id.and_then(|e| db.entries.iter().find(|x| x.id == e)).map(|e| e.doctor_id).unwrap_or(p.doctor_id);
    let doc = db.doctor(doctor).cloned();
    let patient = if w.anonymize { format!("{} (historia nº {})", initials(&p.full_name()), p.history_no) } else { format!("{} (historia nº {})", p.full_name(), p.history_no) };
    let age = p.age(today()).map(|a| format!("{a} años")).unwrap_or_else(|| "—".into());
    let sex = match p.sex.as_str() {
        "F" => "Mujer",
        "M" => "Hombre",
        _ => "—",
    };
    let teeth: Vec<String> = w.teeth.iter().map(|t| t.to_string()).collect();
    let files: Vec<String> = attached(db, w).iter().enumerate().map(|(k, i)| format!("<li>{} · {} · {}</li>", esc(&attachment_name(k, i)), esc(i.kind.label()), fmt_date(i.date))).collect();
    let row = |k: &str, v: &str| format!("<tr><th>{k}</th><td>{}</td></tr>", if v.trim().is_empty() { "—".to_string() } else { esc(v) });
    format!(
        "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Orden de trabajo {id}</title><style>
body{{font-family:Inter,Segoe UI,Arial,sans-serif;color:#1e293b;margin:32px auto;max-width:820px;padding:0 24px}}
.top{{display:flex;justify-content:space-between;align-items:flex-start;border-bottom:3px solid #0d9488;padding-bottom:14px}}
h1{{font-size:24px;margin:0;color:#0b1324}} .muted{{color:#64748b;font-size:13px}}
.badge{{display:inline-block;background:#e6f6f4;color:#0f766e;border-radius:999px;padding:4px 12px;font-weight:600;font-size:13px}}
.grid{{display:grid;grid-template-columns:1fr 1fr;gap:14px;margin-top:18px}}
.box{{border:1px solid #e2e8f0;border-radius:12px;padding:14px 16px}} .box h2{{font-size:13px;letter-spacing:.06em;text-transform:uppercase;color:#64748b;margin:0 0 8px}}
table{{border-collapse:collapse;width:100%}} th{{text-align:left;font-weight:500;color:#64748b;padding:4px 10px 4px 0;width:42%;vertical-align:top}} td{{padding:4px 0;font-weight:600}}
.chart{{margin-top:18px}} .notes{{white-space:pre-wrap;min-height:60px}}
.sign{{display:flex;gap:40px;margin-top:34px}} .sign div{{flex:1;border-top:1px solid #94a3b8;padding-top:6px;font-size:12px;color:#64748b}}
@media print{{body{{margin:0}}}}
</style></head><body>
<div class='top'><div><h1>Orden de trabajo para laboratorio</h1><div class='muted'>{clinic} · {addr} · {phone}</div></div><div style='text-align:right'><span class='badge'>Nº {id}</span><div class='muted' style='margin-top:6px'>Enviada el {sent}</div></div></div>
<div class='grid'>
<div class='box'><h2>Laboratorio</h2><table>{lab_name}{lab_contact}{lab_phone}{lab_mail}</table></div>
<div class='box'><h2>Paciente y prescriptor</h2><table>{pat}{age}{sex}{alg}{doc}{lic}</table></div>
</div>
<div class='box' style='margin-top:14px'><h2>Trabajo</h2><table>{desc}{pieces}{shade}{mat}{due}</table>
<div class='chart'>{chart}</div></div>
<div class='box' style='margin-top:14px'><h2>Indicaciones</h2><div class='notes'>{notes}</div></div>
<div class='box' style='margin-top:14px'><h2>Archivos adjuntos</h2>{files}</div>
<div class='sign'><div>Firma del prescriptor</div><div>Recibido por el laboratorio (fecha y firma)</div></div>
</body></html>",
        id = w.id,
        clinic = esc(&c.name),
        addr = esc(&c.address),
        phone = esc(&c.phone),
        sent = fmt_date(w.sent),
        lab_name = row("Nombre", &lab.as_ref().map(|l| l.name.clone()).unwrap_or_default()),
        lab_contact = row("Contacto", &lab.as_ref().map(|l| l.contact.clone()).unwrap_or_default()),
        lab_phone = row("Teléfono", &lab.as_ref().map(|l| l.phone.clone()).unwrap_or_default()),
        lab_mail = row("Correo", &lab.as_ref().map(|l| l.email.clone()).unwrap_or_default()),
        pat = row("Paciente", &patient),
        age = row("Edad", &age),
        sex = row("Sexo", sex),
        alg = row("Alergias", &p.allergies),
        doc = row("Prescriptor", &doc.as_ref().map(|d| if d.name.is_empty() { d.alias.clone() } else { d.name.clone() }).unwrap_or_default()),
        lic = row("Nº de colegiado", &doc.as_ref().map(|d| d.license.clone()).unwrap_or_default()),
        desc = row("Trabajo", &w.description),
        pieces = row("Piezas", &teeth.join(", ")),
        shade = row("Color", &w.shade),
        mat = row("Material", &w.material),
        due = row("Fecha prevista de entrega", &fmt_date(w.due)),
        chart = tooth_chart_svg(&w.teeth),
        notes = if w.notes.trim().is_empty() { "—".into() } else { esc(&w.notes) },
        files = if files.is_empty() { "<div class='muted'>Sin archivos adjuntos.</div>".into() } else { format!("<ul>{}</ul>", files.join("")) },
    )
}

/// Bytes de una imagen de la galería (las de demostración se generan como PNG).
fn image_bytes(img: &PatientImage) -> Result<Vec<u8>, String> {
    if img.demo {
        let rgb = crate::screens::imagenes::rgb_for_ai(img)?;
        let mut out = std::io::Cursor::new(Vec::new());
        rgb.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        return Ok(out.into_inner());
    }
    if std::path::Path::new(&img.path).is_dir() {
        return Err(format!("{} es una carpeta (estudio DICOM): expórtela como fichero para adjuntarla.", img.path));
    }
    std::fs::read(&img.path).map_err(|e| format!("No se puede leer {}: {e}", img.path))
}

/// Paquete ZIP con la orden y los adjuntos. Devuelve los bytes y los avisos (adjuntos que no se pudieron incluir).
pub fn package(db: &Db, w: &LabWork) -> Result<(Vec<u8>, Vec<String>), String> {
    use zip::write::SimpleFileOptions;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opt = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    z.start_file("orden_de_trabajo.html", opt).map_err(|e| e.to_string())?;
    z.write_all(order_html(db, w).as_bytes()).map_err(|e| e.to_string())?;
    let mut warnings = vec![];
    for (k, img) in attached(db, w).iter().enumerate() {
        match image_bytes(img) {
            Ok(b) => {
                // los STL y DICOM se guardan sin comprimir más (ya ocupan lo suyo)
                let o = if matches!(img.kind, ImageKind::Escaneo3D | ImageKind::Tac) { SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored) } else { opt };
                z.start_file(attachment_name(k, img), o).map_err(|e| e.to_string())?;
                z.write_all(&b).map_err(|e| e.to_string())?;
            }
            Err(e) => warnings.push(e),
        }
    }
    let bytes = z.finish().map_err(|e| e.to_string())?.into_inner();
    Ok((bytes, warnings))
}

/// Nombre del paquete.
pub fn package_name(db: &Db, w: &LabWork) -> String {
    let p = db.patient(w.patient_id).cloned().unwrap_or_default();
    let who = if w.anonymize { format!("historia_{}", p.history_no) } else { safe(&p.full_name()) };
    format!("Orden_{}_{}.zip", w.id, who)
}

/// Texto del correo que acompaña a la orden.
pub fn mail_text(db: &Db, w: &LabWork) -> (String, String) {
    let p = db.patient(w.patient_id).cloned().unwrap_or_default();
    let who = if w.anonymize { format!("{} (historia nº {})", initials(&p.full_name()), p.history_no) } else { p.full_name() };
    let teeth: Vec<String> = w.teeth.iter().map(|t| t.to_string()).collect();
    let subject = format!("Orden de trabajo nº {} · {} · {}", w.id, w.description, who);
    let body = format!(
        "Buenos días:\n\nLes enviamos la orden de trabajo nº {} de {}.\n\nPaciente: {}\nTrabajo: {}\nPiezas: {}\nColor: {}\nMaterial: {}\nFecha prevista de entrega: {}\n\nEn el fichero adjunto van la orden completa (para imprimir) y {} archivo(s) del paciente.\n\nIndicaciones: {}\n\nUn saludo,\n{}\n{}",
        w.id,
        db.config.name,
        who,
        w.description,
        if teeth.is_empty() { "—".into() } else { teeth.join(", ") },
        if w.shade.is_empty() { "—" } else { &w.shade },
        if w.material.is_empty() { "—" } else { &w.material },
        fmt_date(w.due),
        attached(db, w).len(),
        if w.notes.trim().is_empty() { "—" } else { w.notes.trim() },
        db.config.name,
        db.config.phone
    );
    (subject, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db_with_work() -> (Db, u32) {
        let mut db = Db::sample(today());
        let pid = 101;
        let lab = db.suppliers.iter().find(|s| s.is_lab).map(|s| s.id).unwrap();
        let id = db.new_lab_work(pid, None, lab, 7);
        let imgs: Vec<u32> = db.images.iter().filter(|i| i.patient_id == pid && i.kind != ImageKind::Tac).map(|i| i.id).collect();
        let w = db.lab_works.iter_mut().find(|w| w.id == id).unwrap();
        w.teeth = vec![36, 46];
        w.material = "Zirconio".into();
        w.notes = "Hombro cervical <1 mm>".into();
        w.attachments = imgs;
        (db, id)
    }

    #[test]
    fn order_document_and_package() {
        let (mut db, id) = db_with_work();
        let w = db.lab_works.iter().find(|w| w.id == id).unwrap().clone();
        let html = order_html(&db, &w);
        assert!(html.contains("Orden de trabajo para laboratorio") && html.contains("Zirconio"));
        assert!(html.contains("Hombro cervical &lt;1 mm&gt;"), "el texto se escapa");
        assert!(html.contains("Carlos Martínez"), "con el nombre completo");
        let _ = std::fs::write(std::env::temp_dir().join("veodardental_orden_prueba.html"), &html);
        // las piezas del trabajo van resaltadas en el esquema
        assert_eq!(tooth_chart_svg(&w.teeth).matches("fill='#0d9488'").count(), 2);
        let (zip, warnings) = package(&db, &w).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let mut a = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
        let names: Vec<String> = (0..a.len()).map(|i| a.by_index(i).unwrap().name().unwrap().to_string()).collect();
        assert_eq!(names[0], "orden_de_trabajo.html");
        assert_eq!(names.len(), 1 + w.attachments.len());
        assert!(names[1..].iter().all(|n| n.ends_with(".png")));
        // sin el nombre completo
        db.lab_works.iter_mut().find(|w| w.id == id).unwrap().anonymize = true;
        let w = db.lab_works.iter().find(|w| w.id == id).unwrap().clone();
        let html = order_html(&db, &w);
        assert!(!html.contains("Carlos Martínez") && html.contains("C. M. S."));
        let (subject, body) = mail_text(&db, &w);
        assert!(!subject.contains("Carlos") && !body.contains("Martínez"));
        assert!(package_name(&db, &w).starts_with(&format!("Orden_{id}_historia_")));
    }
}
