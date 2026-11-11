//! Facturación: libro de facturas, emisión desde tratamientos realizados,
//! rectificativas y registro encadenado (huella SHA-256, estilo VeriFactu).

use crate::app::{App, Dialog, PatTab};
use crate::model::*;
use crate::theme::*;
use egui::{Color32, CornerRadius, RichText, Ui};

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if ui.selectable_label(app.lists.fact_tab == 0, "Facturas a pacientes").clicked() {
            app.lists.fact_tab = 0;
        }
        if ui.selectable_label(app.lists.fact_tab == 1, "Liquidaciones a mutuas").clicked() {
            app.lists.fact_tab = 1;
        }
        let pend = if app.db.verifactu.enabled { crate::verifactu::pending(&app.db, &app.db.verifactu).len() } else { 0 };
        let label = if pend > 0 { format!("VERI*FACTU · {pend} por enviar") } else { "VERI*FACTU (AEAT)".to_string() };
        if ui.selectable_label(app.lists.fact_tab == 2, label).clicked() {
            app.lists.fact_tab = 2;
        }
    });
    if app.lists.fact_tab == 1 {
        crate::screens::mutuas::show(app, ui);
        return;
    }
    if app.lists.fact_tab == 2 {
        egui::ScrollArea::vertical().id_salt("vf_scroll").auto_shrink([false, false]).show(ui, |ui| crate::screens::verifactu::panel(app, ui));
        return;
    }
    ui.horizontal(|ui| {
        title(ui, "Facturas", "Libro de facturas emitidas · serie correlativa por año · registro encadenado");
        right_aligned(ui, |ui| {
            if primary(ui, "🔒 Verificar integridad").clicked() {
                match app.db.verify_chain() {
                    Ok(n) => app.notify(format!("Cadena de facturación correcta: {n} registros sin alteraciones.")),
                    Err(e) => app.warn(e),
                }
            }
            if ui.button("⬇ Exportar CSV").clicked() {
                match export_csv(app) {
                    Ok(p) => app.notify(format!("Exportado en {p}")),
                    Err(e) => app.warn(e),
                }
            }
        });
    });
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Desde");
            crate::screens::date_picker(ui, "inv_from", &mut app.lists.inv_from);
            ui.label("hasta");
            crate::screens::date_picker(ui, "inv_to", &mut app.lists.inv_to);
            ui.separator();
            ui.add_sized([240.0, 34.0], egui::TextEdit::singleline(&mut app.lists.inv_text).hint_text("Buscar cliente, NIF o número"));
        });
    });
    ui.add_space(8.0);
    let q = fold(&app.lists.inv_text);
    let mut list: Vec<Invoice> = app
        .db
        .invoices
        .iter()
        .filter(|i| i.date >= app.lists.inv_from && i.date <= app.lists.inv_to)
        .filter(|i| q.is_empty() || fold(&i.client_name).contains(&q) || fold(&i.client_nif).contains(&q) || fold(&i.code()).contains(&q))
        .cloned()
        .collect();
    list.sort_by_key(|i| std::cmp::Reverse((i.date, i.id)));
    let total: f64 = list.iter().map(|i| i.total()).sum();
    let rects = list.iter().filter(|i| i.rectifies.is_some()).count();
    let pending: f64 = app.db.patients.iter().flat_map(|p| app.db.uninvoiced(p.id)).map(|e| e.amount()).sum();
    let w = kpi_w(ui, 4);
    ui.horizontal(|ui| {
        kpi(ui, w, "Facturado en el periodo", &money(total), &format!("{} facturas", list.len() - rects), ACCENT);
        ui.add_space(4.0);
        kpi(ui, w, "Rectificativas", &rects.to_string(), "anulaciones / correcciones", WARN);
        ui.add_space(4.0);
        kpi(ui, w, "Realizado sin facturar", &money(pending), "todos los pacientes", INFO);
        ui.add_space(4.0);
        let chain = app.db.verify_chain();
        kpi(ui, w, "Integridad del registro", if chain.is_ok() { "Correcta" } else { "ALTERADA" }, "huellas SHA-256 encadenadas", if chain.is_ok() { OK } else { DANGER });
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        let tbl = Table::new("inv", vec![("Número", 120.0), ("Fecha", 96.0), ("Tipo", 110.0), ("Cliente", 220.0), ("NIF", 110.0), ("Base", 100.0), ("IVA", 80.0), ("Total", 110.0), ("Huella", 150.0)]);
        tbl.header(ui);
        let mut open = None;
        egui::ScrollArea::vertical().id_salt("invl").auto_shrink([false, false]).show(ui, |ui| {
            if list.is_empty() {
                empty_state(ui, "Sin facturas en el periodo. Se emiten desde la ficha del paciente → «Cuenta y cobros».");
            }
            for (i, f) in list.iter().enumerate() {
                let kind = if crate::verifactu::cancelled(&app.db, f.id) {
                    ("Anulada (VERI*FACTU)", DANGER)
                } else if f.rectifies.is_some() {
                    ("Rectificativa", WARN)
                } else if f.rectified_by.is_some() {
                    ("Anulada", MUTED)
                } else {
                    ("Ordinaria", TEXT)
                };
                let cells = [
                    cell(f.code()),
                    cell(fmt_date(f.date)),
                    cellc(kind.0, kind.1),
                    cell(&f.client_name),
                    cell(&f.client_nif),
                    cell(money(f.base())),
                    cell(money(f.vat())),
                    cellc(money(f.total()), if f.total() < 0.0 { DANGER } else { TEXT }),
                    cellc(format!("{}…", &f.hash[..12.min(f.hash.len())]), MUTED),
                ];
                if tbl.row(ui, i, false, &cells).clicked() {
                    open = Some(f.id);
                }
            }
        });
        if let Some(id) = open {
            app.dialog = Dialog::Invoice(id);
        }
    });
}

/// Bloque de facturación dentro de la cuenta del paciente.
pub fn patient_block(app: &mut App, ui: &mut Ui, pid: u32) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let pending: Vec<MedEntry> = app.db.uninvoiced(pid).into_iter().cloned().collect();
        let key = egui::Id::unique(("inv_sel", pid));
        let mut sel: Vec<u32> = ui.data_mut(|d| d.get_temp(key)).unwrap_or_else(|| pending.iter().map(|e| e.id).collect());
        sel.retain(|id| pending.iter().any(|e| e.id == *id));
        ui.horizontal(|ui| {
            section(ui, "Facturación");
            right_aligned(ui, |ui| {
                let amount: f64 = pending.iter().filter(|e| sel.contains(&e.id)).map(|e| e.amount()).sum();
                if ui.add_enabled(!sel.is_empty(), egui::Button::new(RichText::new(format!("🧾 Emitir factura ({})", money(amount))).color(Color32::WHITE)).fill(ACCENT)).clicked() {
                    match app.db.issue_invoice(pid, &sel, app.today) {
                        Ok(id) => {
                            let code = app.db.invoice(id).map(|i| i.code()).unwrap_or_default();
                            app.db.audit("Emitir factura", &code);
                            app.changed();
                            app.notify(format!("Factura {} emitida.", app.db.invoice(id).map(|i| i.code()).unwrap_or_default()));
                            app.dialog = Dialog::Invoice(id);
                            sel.clear();
                        }
                        Err(e) => app.warn(e),
                    }
                }
            });
        });
        if pending.is_empty() {
            muted(ui, "No hay tratamientos realizados pendientes de facturar.");
        } else {
            muted(ui, "Tratamientos realizados sin facturar (marque los que incluir):");
            for e in &pending {
                let mut on = sel.contains(&e.id);
                let teeth: Vec<String> = e.teeth.iter().map(|t| t.to_string()).collect();
                if ui.checkbox(&mut on, format!("{} · {} {} · {}", fmt_date(e.date), e.description, teeth.join("-"), money(e.amount()))).changed() {
                    if on {
                        sel.push(e.id);
                    } else {
                        sel.retain(|x| *x != e.id);
                    }
                }
            }
        }
        ui.data_mut(|d| d.insert_temp(key, sel));
        let mut invs: Vec<Invoice> = app.db.invoices.iter().filter(|i| i.patient_id == pid).cloned().collect();
        if !invs.is_empty() {
            ui.add_space(6.0);
            muted(ui, "Facturas del paciente:");
            invs.sort_by_key(|i| std::cmp::Reverse(i.id));
            for f in invs {
                ui.horizontal(|ui| {
                    if ui.link(f.code()).clicked() {
                        app.dialog = Dialog::Invoice(f.id);
                    }
                    ui.label(format!("{} · {}", fmt_date(f.date), money(f.total())));
                    if f.rectified_by.is_some() {
                        pill(ui, "Anulada", MUTED);
                    }
                    if f.rectifies.is_some() {
                        pill(ui, "Rectificativa", WARN);
                    }
                });
            }
        }
    });
}

pub fn invoice_dialog(app: &mut App, ctx: &egui::Context, id: u32) -> Dialog {
    let Some(f) = app.db.invoice(id).cloned() else { return Dialog::None };
    let mut open = true;
    let mut rectify = false;
    let mut to_patient = false;
    let mut goto = None;
    crate::theme::modal("inv_dlg").show(ctx, |ui| {
        ui.set_width(860.0);
        ui.horizontal(|ui| {
            title(ui, &format!("{} {}", if f.rectifies.is_some() { "Factura rectificativa" } else { "Factura" }, f.code()), &format!("Fecha de expedición {} · emitida por {}", fmt_date(f.date), f.user));
            right_aligned(ui, |ui| {
                if f.rectified_by.is_some() {
                    pill(ui, "Anulada por rectificativa", DANGER);
                }
            });
        });
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Emisor");
                ui.label(&app.db.config.name);
                muted(ui, format!("NIF {}", app.db.config.nif));
                muted(ui, &app.db.config.address);
            });
            card(&mut cols[1], |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Cliente");
                ui.label(&f.client_name);
                muted(ui, if f.client_nif.is_empty() { "Sin NIF (factura simplificada)".to_string() } else { format!("NIF {}", f.client_nif) });
                muted(ui, &f.client_address);
            });
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            let tbl = Table::new("invlines", vec![("Código", 64.0), ("Concepto", 300.0), ("Piezas", 90.0), ("Uds.", 50.0), ("Precio", 100.0), ("Dto.", 60.0), ("IVA", 60.0), ("Importe", 100.0)]);
            tbl.header(ui);
            for (i, l) in f.lines.iter().enumerate() {
                let teeth: Vec<String> = l.teeth.iter().map(|t| t.to_string()).collect();
                tbl.row(ui, i, false, &[cell(&l.code), cell(&l.description), cell(teeth.join("-")), cell(l.units.to_string()), cell(money(l.price)), cell(format!("{:.0}%", l.discount_pct)), cell(format!("{:.0}%", l.vat_pct)), cell(money(l.base()))]);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(format!("Base imponible {}  ·  IVA {}", money(f.base()), money(f.vat()))).font(crate::theme::mdf(13.5)).color(MUTED));
                    ui.label(RichText::new("Servicios de asistencia sanitaria exentos de IVA (art. 20.Uno.3º Ley 37/1992).").font(crate::theme::mdf(12.3)).color(SUBTLE));
                });
                right_aligned(ui, |ui| {
                    let c = if f.total() < 0.0 { DANGER } else { ACCENT_DARK };
                    egui::Frame::new().fill(tint(c, 0.08)).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(16, 8)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // el diseño va de derecha a izquierda: primero el importe
                            ui.label(RichText::new(money(f.total())).font(crate::theme::bf(24.0)).color(c));
                            ui.label(RichText::new("TOTAL").font(crate::theme::sbf(12.5)).color(mix(c, MUTED, 0.4)));
                        });
                    });
                });
            });
        });
        if let Some(r) = f.rectifies {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Rectifica a la factura {} · motivo: {}", app.db.invoice(r).map(|i| i.code()).unwrap_or_default(), f.reason)).color(WARN));
                if ui.small_button("Ver original").clicked() {
                    goto = Some(r);
                }
            });
        }
        if let Some(r) = f.rectified_by {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Anulada por la rectificativa {}", app.db.invoice(r).map(|i| i.code()).unwrap_or_default())).color(DANGER));
                if ui.small_button("Ver rectificativa").clicked() {
                    goto = Some(r);
                }
            });
        }
        ui.add_space(6.0);
        // QR, leyenda VERI*FACTU, huellas y estado del envío a la AEAT
        crate::screens::verifactu::invoice_box(app, ui, &f);
        ui.add_space(10.0);
        if f.rectifies.is_none() && f.rectified_by.is_none() {
            ui.horizontal(|ui| {
                ui.add_sized([340.0, 36.0], egui::TextEdit::singleline(&mut app.lists.rect_reason).hint_text("Motivo de la rectificación"));
                if danger(ui, "Emitir rectificativa").clicked() {
                    rectify = true;
                }
            });
            ui.add_space(4.0);
        }
        let (sep, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 9.0), egui::Sense::hover());
        ui.painter().hline(sep.x_range(), sep.center().y, egui::Stroke::new(1.0, HAIRLINE));
        ui.horizontal(|ui| {
            if primary(ui, "🖨 Imprimir / PDF").clicked() {
                match crate::screens::centros::print_invoice(app, ui.ctx(), f.id) {
                    Ok(p) => app.notify(format!("Factura lista para imprimir: {}", p.display())),
                    Err(e) => app.warn(e),
                }
            }
            if ui.button("📄 Exportar a texto").clicked() {
                match export_txt(app, &f) {
                    Ok(p) => app.notify(format!("Factura exportada en {p}")),
                    Err(e) => app.warn(e),
                }
            }
            if ui.button("Ver ficha del paciente").clicked() {
                to_patient = true;
            }
            right_aligned(ui, |ui| {
                if ui.button("Cerrar").clicked() {
                    open = false;
                }
            });
        });
    });
    if rectify {
        let reason = app.lists.rect_reason.clone();
        match app.db.rectify_invoice(id, &reason) {
            Ok(r) => {
                let code = app.db.invoice(r).map(|i| i.code()).unwrap_or_default();
                app.db.audit("Factura rectificativa", &format!("{code}: {reason}"));
                app.lists.rect_reason.clear();
                app.changed();
                app.notify("Rectificativa emitida. Los tratamientos vuelven a estar pendientes de facturar.");
                return Dialog::Invoice(r);
            }
            Err(e) => app.warn(e),
        }
    }
    if let Some(g) = goto {
        return Dialog::Invoice(g);
    }
    if to_patient {
        app.open_patient(f.patient_id, PatTab::Cuenta);
        return Dialog::None;
    }
    if open { Dialog::Invoice(id) } else { app.dialog_take_or_none() }
}

fn out_dir() -> Result<std::path::PathBuf, String> {
    let dir = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| std::path::PathBuf::from(".")).join(crate::brand::DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn export_txt(app: &App, f: &Invoice) -> Result<String, String> {
    let path = out_dir()?.join(format!("factura_{}.txt", f.code()));
    let c = &app.db.config;
    let mut s = format!("{}\nNIF {}\n{}\n\n", c.name, c.nif, c.address);
    s.push_str(&format!("{} {}   Fecha: {}\n", if f.rectifies.is_some() { "FACTURA RECTIFICATIVA" } else { "FACTURA" }, f.code(), fmt_date(f.date)));
    s.push_str(&format!("Cliente: {}   NIF: {}\n{}\n\n", f.client_name, f.client_nif, f.client_address));
    for l in &f.lines {
        let teeth: Vec<String> = l.teeth.iter().map(|t| t.to_string()).collect();
        s.push_str(&format!("{:<6} {:<40} {:<10} {:>3} x {:>12}  {:>14}\n", l.code, l.description, teeth.join("-"), l.units, money(l.price), money(l.base())));
    }
    s.push_str(&format!("\nBase imponible: {}\nIVA: {} (exento art. 20.Uno.3º LIVA)\nTOTAL: {}\n", money(f.base()), money(f.vat()), money(f.total())));
    if let Some(r) = f.rectifies {
        s.push_str(&format!("Rectifica a: {}  Motivo: {}\n", app.db.invoice(r).map(|i| i.code()).unwrap_or_default(), f.reason));
    }
    s.push_str(&format!("\nHuella: {}\n", f.hash));
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

fn export_csv(app: &App) -> Result<String, String> {
    let path = out_dir()?.join(format!("facturas_{}_{}.csv", app.lists.inv_from.format("%Y%m%d"), app.lists.inv_to.format("%Y%m%d")));
    let mut s = String::from("Número;Fecha;Tipo;Cliente;NIF;Base;IVA;Total;Huella\n");
    let mut list: Vec<&Invoice> = app.db.invoices.iter().filter(|i| i.date >= app.lists.inv_from && i.date <= app.lists.inv_to).collect();
    list.sort_by_key(|i| (i.date, i.id));
    let n = |v: f64| format!("{v:.2}").replace('.', ",");
    for f in list {
        s.push_str(&format!("{};{};{};{};{};{};{};{};{}\n", f.code(), fmt_date(f.date), f.kind(), f.client_name.replace(';', ","), f.client_nif, n(f.base()), n(f.vat()), n(f.total()), f.hash));
    }
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}
