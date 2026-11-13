//! Centros (multicentro) y documentos imprimibles (presupuestos y facturas).

use crate::app::App;
use crate::model::*;
use crate::theme::*;
use egui::{Color32, Ui};

pub fn centers_panel(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Centros");
            right_aligned(ui, |ui| {
                if ui.button("＋ Nuevo centro").clicked() {
                    let id = app.db.new_id();
                    app.db.centers.push(Center { id, name: format!("Centro {id}"), address: String::new(), phone: String::new(), color: [56, 132, 214], active: true });
                    let bid = app.db.new_id();
                    app.db.boxes.push(Boxroom { id: bid, name: format!("Gabinete centro {id}"), center_id: id });
                    app.db.audit("Nuevo centro", &format!("Centro {id}"));
                    app.changed();
                }
            });
        });
        muted(ui, "Cada centro tiene su agenda, sus gabinetes y su caja. Los pacientes, el historial y las facturas son comunes. Con más de un centro aparece un selector en la barra superior.");
        ui.add_space(6.0);
        let before = serde_json::to_string(&app.db.centers).unwrap_or_default();
        let n_active = app.db.centers.iter().filter(|c| c.active).count();
        for c in app.db.centers.iter_mut() {
            ui.horizontal_wrapped(|ui| {
                let mut col = Color32::from_rgb(c.color[0], c.color[1], c.color[2]);
                if ui.color_edit_button_srgba(&mut col).changed() {
                    c.color = [col.r(), col.g(), col.b()];
                }
                ui.add_sized([220.0, 32.0], egui::TextEdit::singleline(&mut c.name).hint_text("Nombre"));
                ui.add_sized([260.0, 32.0], egui::TextEdit::singleline(&mut c.address).hint_text("Dirección"));
                ui.add_sized([130.0, 32.0], egui::TextEdit::singleline(&mut c.phone).hint_text("Teléfono"));
                if c.id != 1 || n_active > 1 {
                    ui.checkbox(&mut c.active, "Activo");
                }
            });
        }
        if serde_json::to_string(&app.db.centers).unwrap_or_default() != before {
            app.dirty = true;
        }
        if app.db.centers.len() > 1 {
            ui.add_space(8.0);
            section(ui, "Gabinetes por centro");
            let centers: Vec<(u32, String)> = app.db.centers.iter().map(|c| (c.id, c.name.clone())).collect();
            let before = serde_json::to_string(&app.db.boxes).unwrap_or_default();
            for b in app.db.boxes.iter_mut() {
                ui.horizontal(|ui| {
                    lab(ui, 200.0, &b.name);
                    let cur = centers.iter().find(|c| c.0 == cid(b.center_id)).map(|c| c.1.clone()).unwrap_or_default();
                    crate::theme::combo(("box_c", b.id)).width(220.0).selected_text(cur).show_ui(ui, |ui| {
                        for (id, n) in &centers {
                            ui.selectable_value(&mut b.center_id, *id, n);
                        }
                    });
                });
            }
            ui.add_space(8.0);
            section(ui, "Colaboradores por centro");
            let cbefore = serde_json::to_string(&app.db.collaborators).unwrap_or_default();
            for d in app.db.collaborators.iter_mut().filter(|d| d.active) {
                ui.horizontal_wrapped(|ui| {
                    lab(ui, 200.0, &d.alias);
                    let mut all = d.centers.is_empty();
                    if ui.checkbox(&mut all, "Todos").changed() {
                        d.centers = if all { vec![] } else { vec![1] };
                    }
                    if !all {
                        for (id, n) in &centers {
                            let mut on = d.centers.contains(id);
                            if ui.checkbox(&mut on, n).changed() {
                                if on {
                                    d.centers.push(*id);
                                } else {
                                    d.centers.retain(|x| x != id);
                                }
                            }
                        }
                    }
                });
            }
            if serde_json::to_string(&app.db.boxes).unwrap_or_default() != before || serde_json::to_string(&app.db.collaborators).unwrap_or_default() != cbefore {
                app.dirty = true;
            }
        }
    });
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

const CSS: &str = "body{font-family:'Open Sans',Arial,sans-serif;max-width:820px;margin:36px auto;color:#1e293b}.head{display:flex;justify-content:space-between;border-bottom:3px solid #0e9688;padding-bottom:12px}.head b{font-size:20px}.muted{color:#64748b;font-size:13px}h1{font-size:22px;margin:22px 0 6px}table{width:100%;border-collapse:collapse;margin-top:14px}th{background:#eef3f8;color:#475569;font-size:12px;text-transform:uppercase;text-align:left;padding:8px}td{padding:8px;border-bottom:1px solid #e2e8f0;font-size:14px}td.n{text-align:right}tr.total td{font-weight:700;font-size:16px;border:none}.box{background:#f4f8fb;border-radius:8px;padding:12px;margin-top:16px;font-size:13px}.firma{margin-top:50px;display:flex;gap:80px}.firma div{border-top:1px solid #94a3b8;width:240px;padding-top:6px;font-size:12px;color:#64748b}@media print{body{margin:14mm}}";

pub fn open_html(ctx: &egui::Context, name: &str, html: String) -> Result<std::path::PathBuf, String> {
    let dir = crate::paths::documents().join(crate::brand::DIR).join("imprimir");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, html).map_err(|e| e.to_string())?;
    ctx.open_url(egui::OpenUrl::new_tab(format!("file://{}", path.display())));
    Ok(path)
}

fn header(app: &App, center: u32) -> String {
    let c = &app.db.config;
    let (name, addr, phone) = match app.db.centers.iter().find(|x| x.id == cid(center)).filter(|_| app.db.multi_center()) {
        Some(x) => (format!("{} · {}", c.name, x.name), if x.address.is_empty() { c.address.clone() } else { x.address.clone() }, if x.phone.is_empty() { c.phone.clone() } else { x.phone.clone() }),
        None => (c.name.clone(), c.address.clone(), c.phone.clone()),
    };
    format!("<div class='head'><div><b>{}</b><br><span class='muted'>{} · {} · NIF {}</span></div><div class='muted' style='text-align:right'>{}</div></div>", esc(&name), esc(&addr), esc(&phone), esc(&c.nif), esc(&c.email))
}

/// Presupuesto imprimible (se abre en el navegador para imprimir o guardar en PDF).
pub fn print_budget(app: &App, ctx: &egui::Context, bid: u32) -> Result<std::path::PathBuf, String> {
    let b = app.db.budgets.iter().find(|b| b.id == bid).cloned().ok_or("Presupuesto no encontrado")?;
    let p = app.db.patient(b.patient_id).cloned().unwrap_or_default();
    let rows: String = b
        .lines
        .iter()
        .map(|l| {
            let teeth: Vec<String> = l.teeth.iter().map(|t| t.to_string()).collect();
            format!("<tr><td>{}</td><td>{}</td><td class='n'>{}</td><td class='n'>{}</td><td class='n'>{}</td><td class='n'>{}</td></tr>", esc(&l.description), teeth.join("-"), l.units, esc(&money(l.price)), if l.discount_pct > 0.0 { format!("{:.0}%", l.discount_pct) } else { String::new() }, esc(&money(l.amount())))
        })
        .collect();
    let plan = b.plan.as_ref().map(|pl| format!("<div class='box'><b>Financiación:</b> entrada de {} y {}, cada {} mes/es, la primera el {}.</div>", esc(&money(pl.down_payment)), esc(&pl.installments_text(b.total())), pl.every_months, fmt_date(pl.first_date))).unwrap_or_default();
    let html = format!(
        "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Presupuesto {id}</title><style>{css}</style></head><body>{head}<h1>Presupuesto nº {id}</h1><div class='muted'>Fecha {date} · válido hasta {exp} · {doctor} · Tarifa {tar}</div><div class='box'><b>{pname}</b> · Nº historia {hist} · NIF {nif}<br>{addr}</div><h3>{title}</h3><table><tr><th>Tratamiento</th><th>Piezas</th><th class='n'>Uds.</th><th class='n'>Precio</th><th class='n'>Dto.</th><th class='n'>Importe</th></tr>{rows}<tr class='total'><td colspan='5' class='n'>TOTAL</td><td class='n'>{total}</td></tr></table>{plan}<div class='box'>{notes}Servicios sanitarios exentos de IVA. Precios válidos durante el periodo indicado.</div><div class='firma'><div>Firma del paciente (acepto el presupuesto)</div><div>{doctor_full}</div></div></body></html>",
        id = b.id,
        css = CSS,
        head = header(app, app.db.config.current_center),
        date = fmt_date(b.date),
        exp = fmt_date(b.expires()),
        doctor = esc(&app.db.doctor_alias(b.doctor_id)),
        tar = esc(&app.db.tariff_name(b.tariff_id)),
        pname = esc(&p.full_name()),
        hist = p.history_no,
        nif = esc(&p.nif),
        addr = esc(&format!("{}, {} {}", p.address, p.zip, p.city)),
        title = esc(&b.title),
        rows = rows,
        total = esc(&money(b.total())),
        plan = plan,
        notes = if b.notes.is_empty() { String::new() } else { format!("{}<br><br>", esc(&b.notes)) },
        doctor_full = esc(&app.db.doctor(b.doctor_id).map(|d| format!("{} · Nº col. {}", d.name, d.license)).unwrap_or_default()),
    );
    open_html(ctx, &format!("presupuesto_{}.html", b.id), html)
}

/// Factura imprimible.
pub fn print_invoice(app: &App, ctx: &egui::Context, id: u32) -> Result<std::path::PathBuf, String> {
    let f = app.db.invoice(id).cloned().ok_or("Factura no encontrada")?;
    let rows: String = f
        .lines
        .iter()
        .map(|l| {
            let teeth: Vec<String> = l.teeth.iter().map(|t| t.to_string()).collect();
            format!("<tr><td>{}</td><td>{}</td><td class='n'>{}</td><td class='n'>{}</td><td class='n'>{:.0}%</td><td class='n'>{}</td></tr>", esc(&l.description), teeth.join("-"), l.units, esc(&money(l.price)), l.vat_pct, esc(&money(l.base())))
        })
        .collect();
    let rect = f.rectifies.and_then(|r| app.db.invoice(r)).map(|o| format!("<div class='box'><b>Factura rectificativa</b> de la factura {} · Motivo: {}</div>", esc(&o.code()), esc(&f.reason))).unwrap_or_default();
    let html = format!(
        "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Factura {code}</title><style>{css}</style></head><body>{head}<h1>{kind} {code}</h1>{vf}<div class='muted'>Fecha de expedición {date}</div><div class='box'><b>{client}</b><br>NIF {nif}<br>{addr}{patient}</div>{rect}<table><tr><th>Concepto</th><th>Piezas</th><th class='n'>Uds.</th><th class='n'>Precio</th><th class='n'>IVA</th><th class='n'>Base</th></tr>{rows}<tr><td colspan='5' class='n'>Base imponible</td><td class='n'>{base}</td></tr><tr><td colspan='5' class='n'>IVA</td><td class='n'>{vat}</td></tr><tr class='total'><td colspan='5' class='n'>TOTAL</td><td class='n'>{total}</td></tr></table><div class='box'>Operación exenta de IVA según el art. 20.Uno.3º de la Ley 37/1992 (asistencia sanitaria).<br><span class='muted'>Registro VERI*FACTU · huella {hash}</span></div></body></html>",
        code = esc(&f.code()),
        css = CSS,
        head = header(app, app.db.config.current_center),
        kind = if f.rectifies.is_some() { "Factura rectificativa" } else { "Factura" },
        date = fmt_date(f.date),
        client = esc(&f.client_name),
        nif = esc(if f.client_nif.is_empty() { "—" } else { &f.client_nif }),
        addr = esc(&f.client_address),
        patient = if f.patient_name.is_empty() { String::new() } else { format!("<br>Paciente: {}", esc(&f.patient_name)) },
        rect = rect,
        rows = rows,
        base = esc(&money(f.base())),
        vat = esc(&money(f.vat())),
        total = esc(&money(f.total())),
        hash = esc(&f.hash),
        // código QR y leyenda VERI*FACTU al principio de la factura
        vf = crate::screens::verifactu::print_block(&app.db, &f),
    );
    open_html(ctx, &format!("factura_{}.html", f.code()), html)
}
