//! Ficha completa del paciente: versión imprimible (para PDF), exportación
//! de sus datos en formato estructurado (derecho de acceso y portabilidad
//! del RGPD) y exportación por lotes de todas las fichas.

use crate::model::*;
use chrono::NaiveDate;
use std::path::{Path, PathBuf};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

const UPPER: [u8; 16] = [18, 17, 16, 15, 14, 13, 12, 11, 21, 22, 23, 24, 25, 26, 27, 28];
const LOWER: [u8; 16] = [48, 47, 46, 45, 44, 43, 42, 41, 31, 32, 33, 34, 35, 36, 37, 38];

/// Estado de una pieza para el esquema: 0 sana, 1 tratada, 2 pendiente, 3 ausente.
fn tooth_state(db: &Db, pid: u32, t: u8) -> u8 {
    let es: Vec<&MedEntry> = db.entries.iter().filter(|e| e.patient_id == pid && e.teeth.contains(&t)).collect();
    if es.iter().any(|e| e.odg == OdgKind::Ausente) {
        3
    } else if es.iter().any(|e| e.state == TxState::Previsto) {
        2
    } else if es.iter().any(|e| e.state == TxState::Realizado) {
        1
    } else {
        0
    }
}

/// Esquema dental sencillo en SVG (se imprime sin depender de imágenes).
pub fn odontogram_svg(db: &Db, pid: u32) -> String {
    let mut s = String::from("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 680 150' width='100%' style='max-width:680px'>");
    for (row, teeth) in [(0, UPPER), (1, LOWER)] {
        for (i, t) in teeth.iter().enumerate() {
            let x = 8 + i as i32 * 41 + if i >= 8 { 12 } else { 0 };
            let y = 14 + row * 70;
            let (fill, stroke) = match tooth_state(db, pid, *t) {
                1 => ("#cfe3f8", "#2f6fb5"),
                2 => ("#fde2e2", "#c0392b"),
                3 => ("#eef1f5", "#9aa5b1"),
                _ => ("#ffffff", "#9aa5b1"),
            };
            s.push_str(&format!("<rect x='{x}' y='{y}' width='34' height='40' rx='8' fill='{fill}' stroke='{stroke}' stroke-width='2'/>"));
            if tooth_state(db, pid, *t) == 3 {
                s.push_str(&format!("<path d='M{} {} L{} {} M{} {} L{} {}' stroke='#9aa5b1' stroke-width='2'/>", x + 6, y + 6, x + 28, y + 34, x + 28, y + 6, x + 6, y + 34));
            }
            let ty = if row == 0 { y - 3 } else { y + 54 };
            s.push_str(&format!("<text x='{}' y='{ty}' font-size='11' text-anchor='middle' fill='#475569'>{t}</text>", x + 17));
        }
    }
    s.push_str("</svg>");
    s
}

/// Ficha completa imprimible.
pub fn record_html(db: &Db, pid: u32, today: NaiveDate) -> String {
    let Some(p) = db.patient(pid) else { return String::new() };
    let mut b = String::new();
    let row = |k: &str, v: &str| if v.trim().is_empty() { String::new() } else { format!("<tr><th>{}</th><td>{}</td></tr>", esc(k), esc(v)) };
    // filiación
    b.push_str("<h2>Datos personales</h2><table class='kv'>");
    b.push_str(&row("Nº historial", &p.history_no.to_string()));
    b.push_str(&row("DNI/NIE", &p.nif));
    b.push_str(&row("Fecha de nacimiento", &p.birth.map(|d| format!("{} ({} años)", fmt_date(d), p.age(today).unwrap_or(0))).unwrap_or_default()));
    b.push_str(&row("Teléfonos", &[p.mobile.as_str(), p.phone.as_str()].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" · ")));
    b.push_str(&row("Email", &p.email));
    b.push_str(&row("Dirección", format!("{} {} {}", p.address, p.zip, p.city).trim()));
    b.push_str(&row("Tarifa / póliza", format!("{} {}", db.tariff_name(p.tariff_id), p.policy_no).trim()));
    b.push_str(&row("Doctor habitual", &db.doctor(p.doctor_id).map(|d| d.name.clone()).unwrap_or_default()));
    b.push_str(&row("Alta", &p.registered.map(fmt_date).unwrap_or_default()));
    b.push_str(&row("Protección de datos", if p.accepts_gdpr { "Firmada" } else { "Pendiente de firma" }));
    b.push_str("</table>");
    // salud
    b.push_str("<h2>Salud</h2>");
    if !p.alerts.is_empty() || !p.allergies.is_empty() {
        b.push_str(&format!("<p class='alert'>{}{}</p>", if p.alerts.is_empty() { String::new() } else { format!("⚠ {}", esc(&p.alerts)) }, if p.allergies.is_empty() { String::new() } else { format!("<br>Alergias: {}", esc(&p.allergies)) }));
    }
    match db.latest_health(pid) {
        Some(h) => b.push_str(&format!("<p>Cuestionario de salud del {}{}.{}</p>", fmt_date(h.date), if h.signed { ", firmado" } else { ", sin firmar" }, if h.notes.trim().is_empty() { String::new() } else { format!(" Observaciones: {}", esc(&h.notes)) })),
        None => b.push_str("<p class='muted'>Sin cuestionario de salud.</p>"),
    }
    if !p.background.trim().is_empty() {
        b.push_str(&format!("<p>Antecedentes: {}</p>", esc(&p.background)));
    }
    // odontograma e historial
    b.push_str("<h2>Odontograma</h2>");
    b.push_str(&odontogram_svg(db, pid));
    b.push_str("<p class='muted'><span class='k' style='background:#cfe3f8;border-color:#2f6fb5'></span> tratada &nbsp; <span class='k' style='background:#fde2e2;border-color:#c0392b'></span> pendiente &nbsp; <span class='k' style='background:#eef1f5'></span> ausente</p>");
    let mut es: Vec<&MedEntry> = db.entries.iter().filter(|e| e.patient_id == pid).collect();
    es.sort_by_key(|e| (e.date, e.id));
    b.push_str("<h2>Historial de tratamientos</h2>");
    if es.is_empty() {
        b.push_str("<p class='muted'>Sin tratamientos.</p>");
    } else {
        b.push_str("<table><tr><th>Fecha</th><th>Tratamiento</th><th>Piezas</th><th>Doctor</th><th>Estado</th><th class='n'>Importe</th></tr>");
        for e in es {
            let teeth: Vec<String> = e.teeth.iter().map(|t| t.to_string()).collect();
            b.push_str(&format!("<tr><td>{}</td><td>{}{}</td><td>{}</td><td>{}</td><td>{}</td><td class='n'>{}</td></tr>", fmt_date(e.date), esc(&e.description), if e.notes.is_empty() { String::new() } else { format!("<br><small>{}</small>", esc(&e.notes)) }, teeth.join("-"), esc(&db.doctor_alias(e.doctor_id)), e.state.label(), esc(&money(e.amount()))));
        }
        b.push_str("</table>");
    }
    // presupuestos
    let bs: Vec<&Budget> = db.budgets.iter().filter(|x| x.patient_id == pid).collect();
    if !bs.is_empty() {
        b.push_str("<h2>Presupuestos</h2><table><tr><th>Fecha</th><th>Presupuesto</th><th>Estado</th><th class='n'>Total</th></tr>");
        for x in bs {
            b.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td><td class='n'>{}</td></tr>", fmt_date(x.date), esc(&x.title), x.state.label(), esc(&money(x.total()))));
        }
        b.push_str("</table>");
    }
    // cuenta
    let mut ps: Vec<&Payment> = db.payments.iter().filter(|x| x.patient_id == pid).collect();
    ps.sort_by_key(|x| x.date);
    b.push_str("<h2>Cuenta</h2>");
    b.push_str(&format!("<p>Realizado: <b>{}</b> · Pagado: <b>{}</b> · {}</p>", esc(&money(db.patient_debit(pid))), esc(&money(db.patient_paid(pid))), {
        let bal = db.patient_balance(pid);
        if bal > 0.005 { format!("Pendiente: <b>{}</b>", esc(&money(bal))) } else if bal < -0.005 { format!("Saldo a favor: <b>{}</b>", esc(&money(-bal))) } else { "Al corriente".to_string() }
    }));
    if !ps.is_empty() {
        b.push_str("<table><tr><th>Fecha</th><th>Recibo</th><th>Concepto</th><th>Forma de pago</th><th class='n'>Importe</th></tr>");
        for x in ps {
            b.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class='n'>{}</td></tr>", x.date.format("%d/%m/%Y"), esc(&x.receipt_no), esc(&x.concept), esc(&x.method), esc(&money(x.amount))));
        }
        b.push_str("</table>");
    }
    // citas
    let mut ap: Vec<&Appointment> = db.appointments.iter().filter(|a| a.patient_id == Some(pid)).collect();
    ap.sort_by_key(|a| (a.date, a.start));
    if !ap.is_empty() {
        b.push_str("<h2>Citas</h2><table><tr><th>Fecha</th><th>Hora</th><th>Motivo</th><th>Doctor</th><th>Situación</th></tr>");
        for a in ap {
            b.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>", fmt_date(a.date), a.start.format("%H:%M"), esc(&a.reason), esc(&db.doctor_alias(a.doctor_id)), a.state.label()));
        }
        b.push_str("</table>");
    }
    // documentos, recetas e imágenes
    let docs: Vec<String> = db.documents.iter().filter(|d| d.patient_id == pid).map(|d| format!("<li>{} · {}{}</li>", fmt_date(d.date), esc(&d.title), if d.signed { " (firmado)" } else { "" })).collect();
    if !docs.is_empty() {
        b.push_str(&format!("<h2>Documentos y consentimientos</h2><ul>{}</ul>", docs.join("")));
    }
    let rx: Vec<String> = db
        .prescriptions
        .iter()
        .filter(|r| r.patient_id == pid)
        .map(|r| format!("<li>{} · {}{}</li>", fmt_date(r.date), esc(&r.items.iter().map(|i| i.drug.clone()).collect::<Vec<_>>().join(", ")), if r.erx_code.is_empty() { String::new() } else { format!(" · CVE {}", esc(&r.erx_code)) }))
        .collect();
    if !rx.is_empty() {
        b.push_str(&format!("<h2>Recetas</h2><ul>{}</ul>", rx.join("")));
    }
    let imgs = db.images.iter().filter(|i| i.patient_id == pid).count();
    if imgs > 0 {
        b.push_str(&format!("<p class='muted'>{imgs} imágenes (radiografías, fotos) guardadas en la clínica.</p>"));
    }
    if !p.notes.trim().is_empty() {
        b.push_str(&format!("<h2>Observaciones</h2><p>{}</p>", esc(&p.notes)));
    }
    format!(
        "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Ficha {hist} · {name}</title><style>body{{font-family:'Open Sans',Arial,sans-serif;max-width:900px;margin:30px auto;color:#1e293b;font-size:13px}}.head{{display:flex;justify-content:space-between;border-bottom:3px solid #0e9688;padding-bottom:10px}}h1{{font-size:22px;margin:16px 0 2px}}h2{{font-size:15px;color:#0e7c71;margin:22px 0 6px;border-bottom:1px solid #cbd5e1;padding-bottom:3px}}table{{width:100%;border-collapse:collapse}}th{{text-align:left;background:#eef3f8;color:#475569;font-size:11px;text-transform:uppercase;padding:5px 6px}}td{{padding:5px 6px;border-bottom:1px solid #e2e8f0;vertical-align:top}}.kv th{{width:190px;background:none;text-transform:none;font-size:12.5px;border-bottom:1px solid #e2e8f0}}.n{{text-align:right}}.muted{{color:#64748b}}.alert{{color:#b91c1c;font-weight:600}}.k{{display:inline-block;width:12px;height:12px;border:1px solid #9aa5b1;border-radius:3px;vertical-align:middle}}@media print{{body{{margin:10mm}}h2{{page-break-after:avoid}}tr{{page-break-inside:avoid}}}}</style></head><body><div class='head'><div><b>{clinic}</b><br><span class='muted'>{addr} · NIF {nif}</span></div><div class='muted' style='text-align:right'>Ficha clínica<br>emitida el {today}</div></div><h1>{name}</h1>{b}<p class='muted' style='margin-top:30px'>Documento confidencial con datos de salud. Uso exclusivo del paciente y de la clínica.</p></body></html>",
        hist = p.history_no,
        name = esc(&p.full_name()),
        clinic = esc(&db.config.name),
        addr = esc(&db.config.address),
        nif = esc(&db.config.nif),
        today = fmt_date(today),
    )
}

/// Todos los datos del paciente en formato estructurado (JSON), para
/// entregárselos cuando ejerce su derecho de acceso o portabilidad.
pub fn patient_data_json(db: &Db, pid: u32, today: NaiveDate) -> String {
    let docs: Vec<serde_json::Value> = db
        .documents
        .iter()
        .filter(|d| d.patient_id == pid)
        .map(|d| serde_json::json!({ "fecha": d.date, "titulo": d.title, "texto": d.body, "firmado": d.signed, "firmante": d.signer, "firmado_el": d.signed_at }))
        .collect();
    let v = serde_json::json!({
        "exportado_el": today,
        "clinica": { "nombre": db.config.name, "nif": db.config.nif, "direccion": db.config.address },
        "paciente": db.patient(pid),
        "cuestionarios_de_salud": db.health.iter().filter(|h| h.patient_id == pid).collect::<Vec<_>>(),
        "tratamientos": db.entries.iter().filter(|e| e.patient_id == pid).collect::<Vec<_>>(),
        "presupuestos": db.budgets.iter().filter(|b| b.patient_id == pid).collect::<Vec<_>>(),
        "pagos": db.payments.iter().filter(|x| x.patient_id == pid).collect::<Vec<_>>(),
        "facturas": db.invoices.iter().filter(|i| i.patient_id == pid).collect::<Vec<_>>(),
        "citas": db.appointments.iter().filter(|a| a.patient_id == Some(pid)).collect::<Vec<_>>(),
        "periodontogramas": db.perio.iter().filter(|x| x.patient_id == pid).collect::<Vec<_>>(),
        "documentos": docs,
        "recetas": db.prescriptions.iter().filter(|r| r.patient_id == pid).collect::<Vec<_>>(),
        "imagenes": db.images.iter().filter(|i| i.patient_id == pid).map(|i| serde_json::json!({ "fecha": i.date, "tipo": i.kind.label(), "piezas": i.teeth, "archivo": i.path })).collect::<Vec<_>>(),
        "mensajes": db.messages.iter().filter(|m| m.patient_id == pid).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

pub fn safe_name(s: &str) -> String {
    let f: String = fold(s).chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    f.split('_').filter(|x| !x.is_empty()).collect::<Vec<_>>().join("_")
}

pub fn out_dir() -> PathBuf {
    dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from(".")).join(crate::brand::DIR).join("fichas")
}

/// Exporta la ficha imprimible de cada paciente (activos o todos) a una carpeta.
pub fn export_all(db: &Db, dir: &Path, today: NaiveDate, only_active: bool) -> Result<(PathBuf, usize), String> {
    let folder = dir.join(format!("fichas_{}", today.format("%Y%m%d")));
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let mut n = 0;
    for p in db.patients.iter().filter(|p| !only_active || p.active) {
        let path = folder.join(format!("{:05}_{}.html", p.history_no, safe_name(&p.full_name())));
        std::fs::write(&path, record_html(db, p.id, today)).map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok((folder, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
    }

    #[test]
    fn record_contains_every_section() {
        let db = Db::sample(d());
        let h = record_html(&db, 101, d());
        for s in ["Datos personales", "Salud", "Odontograma", "Historial de tratamientos", "Cuenta", "Citas", "<svg"] {
            assert!(h.contains(s), "falta {s}");
        }
        assert!(h.contains("Carlos"));
        assert!(record_html(&db, 99999, d()).is_empty());
    }

    #[test]
    fn data_export_is_valid_json_with_only_this_patient() {
        let db = Db::sample(d());
        let j = patient_data_json(&db, 101, d());
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["paciente"]["id"], 101);
        assert!(v["tratamientos"].as_array().unwrap().iter().all(|e| e["patient_id"] == 101));
        assert!(!v["tratamientos"].as_array().unwrap().is_empty());
    }

    #[test]
    fn batch_export_writes_one_file_per_patient() {
        let db = Db::sample(d());
        let dir = std::env::temp_dir().join(format!("ds_fichas_test_{}", std::process::id()));
        let (folder, n) = export_all(&db, &dir, d(), false).unwrap();
        assert_eq!(n, db.patients.len());
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), n);
        assert_eq!(safe_name("José Ñúñez, Ana"), "jose_nunez_ana");
    }
}
