//! Liquidaciones a compañías aseguradoras (mutuas).

use crate::app::{App, PatTab};
use crate::model::*;
use crate::theme::*;
use egui::{RichText, Ui};

fn save_file(name: &str, content: &str) -> Result<std::path::PathBuf, String> {
    let dir = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| std::path::PathBuf::from(".")).join(crate::brand::DIR).join("mutuas");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn show(app: &mut App, ui: &mut Ui) {
    title(ui, "Liquidaciones a mutuas", "Actos de pacientes asegurados que la compañía debe pagar a la clínica");
    // resumen por compañía
    let summary = app.db.insurer_summary();
    if summary.is_empty() {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            empty_state(ui, "No hay tarifas marcadas como compañía aseguradora (pantalla Tarifas).");
        });
        return;
    }
    if !summary.iter().any(|s| s.0 == app.lists.claim_tariff) {
        app.lists.claim_tariff = summary[0].0;
    }
    let w = kpi_w(ui, summary.len().min(4));
    ui.horizontal(|ui| {
        for (id, name, n, pend, owed) in summary.iter().take(4) {
            let sel = *id == app.lists.claim_tariff;
            let r = ui.scope(|ui| kpi(ui, w, name, &money(*pend), &format!("{n} actos sin liquidar · debe {}", money(*owed)), if sel { ACCENT } else { INFO })).response;
            if r.interact(egui::Sense::click()).on_hover_text("Ver esta compañía").clicked() {
                app.lists.claim_tariff = *id;
            }
            ui.add_space(4.0);
        }
    });
    ui.add_space(8.0);
    let tid = app.lists.claim_tariff;
    let tname = app.db.tariff_name(tid);
    let lines = app.db.claimable(tid, app.lists.inv_from, app.lists.inv_to);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, &format!("Pendiente de liquidar a {tname}"));
            right_aligned(ui, |ui| {
                let total: f64 = lines.iter().map(|l| l.fee).sum();
                if ui.add_enabled(!lines.is_empty(), egui::Button::new(RichText::new(format!("Generar liquidación ({})", money(total))).color(egui::Color32::WHITE)).fill(ACCENT)).clicked() {
                    if let Some(id) = app.db.create_claim(tid, app.lists.inv_from, app.lists.inv_to, app.today) {
                        let num = app.db.claims.iter().find(|c| c.id == id).map(|c| c.number.clone()).unwrap_or_default();
                        app.db.audit("Liquidación a mutua", &format!("{num} · {tname}"));
                        app.changed();
                        app.notify(format!("Liquidación {num} creada."));
                    }
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("Actos realizados desde");
            crate::screens::date_picker(ui, "cl_from", &mut app.lists.inv_from);
            ui.label("hasta");
            crate::screens::date_picker(ui, "cl_to", &mut app.lists.inv_to);
        });
        let missing = lines.iter().filter(|l| l.auth_no.is_empty()).count();
        if missing > 0 {
            ui.label(RichText::new(format!("⚠ {missing} actos sin número de autorización: muchas compañías los rechazan sin él.")).color(WARN));
        }
        ui.add_space(4.0);
        if lines.is_empty() {
            empty_state(ui, "No hay actos pendientes de liquidar en este periodo.");
            return;
        }
        let tbl = Table::new("claimable", vec![("Fecha", 100.0), ("Paciente", 230.0), ("Póliza", 120.0), ("Acto", 260.0), ("Piezas", 80.0), ("Autorización", 160.0), ("Importe", 100.0)]);
        tbl.header(ui);
        let mut open_p = None;
        let s = tbl.scale(ui) * 0.98;
        for (i, l) in lines.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(12.0);
                lab(ui, 100.0 * s, fmt_date(l.date));
                if lab(ui, 230.0 * s, RichText::new(&l.patient).color(ACCENT)).interact(egui::Sense::click()).on_hover_text("Abrir ficha").clicked() {
                    open_p = Some(l.patient_id);
                }
                lab(ui, 120.0 * s, RichText::new(if l.policy_no.is_empty() { "sin póliza".to_string() } else { l.policy_no.clone() }).color(if l.policy_no.is_empty() { WARN } else { TEXT }));
                lab(ui, 260.0 * s, format!("{} {}", l.code, l.description));
                lab(ui, 80.0 * s, &l.teeth);
                let mut a = l.auth_no.clone();
                if ui.add_sized([150.0 * s, 30.0], egui::TextEdit::singleline(&mut a).hint_text("Nº autorización").id_salt(("auth", l.entry_id))).changed() {
                    app.db.set_auth_no(l.entry_id, &a);
                    app.changed();
                }
                ui.add_space(10.0 * s);
                lab(ui, 100.0 * s, RichText::new(money(l.fee)).family(sb()));
            });
            let _ = i;
        }
        if let Some(p) = open_p {
            app.open_patient(p, PatTab::Cuenta);
        }
    });
    ui.add_space(8.0);
    // liquidaciones emitidas
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Liquidaciones emitidas");
        let mut list: Vec<crate::insurer::Claim> = app.db.claims.to_vec();
        list.sort_by_key(|c| std::cmp::Reverse(c.id));
        if list.is_empty() {
            empty_state(ui, "Aún no se ha emitido ninguna liquidación.");
            return;
        }
        let tbl = Table::new("claims", vec![("Número", 130.0), ("Compañía", 200.0), ("Periodo", 200.0), ("Actos", 70.0), ("Total", 110.0), ("Cobrado", 110.0), ("Estado", 140.0), ("", 300.0)]);
        tbl.header(ui);
        let mut del = None;
        let s = tbl.scale(ui) * 0.98;
        for c in &list {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(12.0);
                lab(ui, 130.0 * s, RichText::new(&c.number).family(sb()));
                lab(ui, 200.0 * s, app.db.tariff_name(c.tariff_id));
                lab(ui, 200.0 * s, format!("{} – {}", fmt_date(c.from), fmt_date(c.to)));
                lab(ui, 70.0 * s, c.lines.len().to_string());
                lab(ui, 110.0 * s, money(c.total));
                lab(ui, 110.0 * s, money(c.paid));
                let col = match c.state() {
                    "Cobrada" => OK,
                    "Cobro parcial" => WARN,
                    _ => INFO,
                };
                ui.scope(|ui| {
                    ui.set_width(140.0 * s);
                    pill(ui, c.state(), col);
                });
                ui.spacing_mut().item_spacing.x = 6.0;
                if ui.button("🖨").on_hover_text("Imprimir").clicked() {
                    let html = app.db.claim_html(c.id);
                    match crate::screens::centros::open_html(ui.ctx(), &format!("{}.html", c.number), html) {
                        Ok(_) => app.notify("Liquidación abierta para imprimir."),
                        Err(e) => app.warn(format!("No se pudo abrir: {e}")),
                    }
                }
                if ui.button("⬇ CSV").on_hover_text("Fichero para el portal de la compañía").clicked() {
                    match save_file(&format!("{}.csv", c.number), &app.db.claim_csv(c.id)) {
                        Ok(p) => app.notify(format!("Guardado en {}", p.display())),
                        Err(e) => app.warn(format!("No se pudo guardar: {e}")),
                    }
                }
                if c.pending() > 0.0 {
                    if ui.button("✔ Cobrada").on_hover_text("La compañía ha pagado todo lo pendiente").clicked() {
                        app.db.claim_paid(c.id, c.pending(), app.today);
                        app.db.audit("Cobro de mutua", &format!("{} · {}", c.number, money(c.pending())));
                        app.changed();
                    }
                    if c.paid == 0.0 && ui.button("Anular").on_hover_text("Los actos vuelven a quedar pendientes").clicked() {
                        del = Some(c.id);
                    }
                }
            });
        }
        if let Some(id) = del {
            app.db.delete_claim(id);
            app.db.audit("Liquidación anulada", &id.to_string());
            app.changed();
        }
    });
}
