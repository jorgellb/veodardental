//! Presupuestos: libro general, presupuestos del paciente y editor con
//! líneas, descuentos, aceptación/rechazo y plan de financiación.

use crate::app::{App, ConfirmAction, Dialog, PatTab};
use crate::model::*;
use crate::theme::*;
use egui::{Color32, RichText, Ui};

pub fn state_color(s: BudgetState) -> Color32 {
    match s {
        BudgetState::Pendiente => WARN,
        BudgetState::Aceptado => OK,
        BudgetState::Rechazado => DANGER,
        BudgetState::Cerrado => MUTED,
    }
}

pub fn new_budget(app: &mut App, pid: u32) -> u32 {
    let p = app.db.patient(pid).cloned().unwrap_or_default();
    let id = app.db.new_id();
    app.db.budgets.push(Budget {
        id,
        patient_id: pid,
        title: "Nuevo presupuesto".into(),
        date: app.today,
        doctor_id: p.doctor_id.max(1),
        tariff_id: p.tariff_id.max(1),
        state: BudgetState::Pendiente,
        lines: vec![],
        valid_days: app.db.config.budget_valid_days,
        accepted_on: None,
        rejected_on: None,
        reject_reason: String::new(),
        notes: String::new(),
        plan: None,
    });
    app.changed();
    id
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let all = &app.db.budgets;
    let pend: Vec<&Budget> = all.iter().filter(|b| b.state == BudgetState::Pendiente).collect();
    let acc = all.iter().filter(|b| matches!(b.state, BudgetState::Aceptado | BudgetState::Cerrado)).count();
    let rej = all.iter().filter(|b| b.state == BudgetState::Rechazado).count();
    let rate = if acc + rej > 0 { acc as f32 / (acc + rej) as f32 * 100.0 } else { 0.0 };
    let pend_amount: f64 = pend.iter().map(|b| b.total()).sum();
    let expired = pend.iter().filter(|b| b.expires() < app.today).count();
    title(ui, "Presupuestos", "Libro de presupuestos de la clínica");
    let w = kpi_w(ui, 4);
    ui.horizontal(|ui| {
        kpi(ui, w, "Pendientes de aceptar", &pend.len().to_string(), &format!("{} en juego", money(pend_amount)), WARN);
        ui.add_space(4.0);
        kpi(ui, w, "Aceptados", &acc.to_string(), "incluye cerrados", OK);
        ui.add_space(4.0);
        kpi(ui, w, "Tasa de aceptación", &format!("{rate:.0} %"), &format!("{rej} rechazados"), INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Caducados sin respuesta", &expired.to_string(), "conviene llamar", DANGER);
    });
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_sized([280.0, 34.0], egui::TextEdit::singleline(&mut app.lists.budget_text).hint_text("Buscar por paciente o título"));
            for (f, n) in [(None, "Todos"), (Some(BudgetState::Pendiente), "Pendientes"), (Some(BudgetState::Aceptado), "Aceptados"), (Some(BudgetState::Rechazado), "Rechazados"), (Some(BudgetState::Cerrado), "Cerrados")] {
                if ui.selectable_label(app.lists.budget_filter == f, n).clicked() {
                    app.lists.budget_filter = f;
                }
            }
        });
        ui.add_space(6.0);
        let q = fold(&app.lists.budget_text);
        let mut list: Vec<Budget> = app
            .db
            .budgets
            .iter()
            .filter(|b| app.lists.budget_filter.map(|f| b.state == f).unwrap_or(true))
            .filter(|b| q.is_empty() || fold(&app.db.patient_name(b.patient_id)).contains(&q) || fold(&b.title).contains(&q))
            .cloned()
            .collect();
        list.sort_by_key(|b| std::cmp::Reverse((b.date, b.id)));
        table(app, ui, &list, true);
    });
}

fn table(app: &mut App, ui: &mut Ui, list: &[Budget], with_patient: bool) {
    let mut cols = vec![("Nº", 60.0), ("Fecha", 96.0)];
    if with_patient {
        cols.push(("Paciente", 200.0));
    }
    cols.extend([("Título", 250.0), ("Doctor", 110.0), ("Tarifa", 160.0), ("Estado", 100.0), ("Validez", 100.0), ("Importe", 110.0)]);
    let tbl = Table::new("budgets", cols);
    tbl.header(ui);
    if list.is_empty() {
        empty_state(ui, "No hay presupuestos");
    }
    let mut open = None;
    for (i, b) in list.iter().enumerate() {
        let exp = b.expires();
        let mut cells = vec![cell(format!("#{}", b.id)), cell(fmt_date(b.date))];
        if with_patient {
            cells.push(cell(app.db.patient_name(b.patient_id)));
        }
        cells.extend([
            cell(&b.title),
            cell(app.db.doctor_alias(b.doctor_id)),
            cell(app.db.tariff_name(b.tariff_id)),
            cellc(b.state.label(), state_color(b.state)),
            cellc(fmt_date(exp), if b.state == BudgetState::Pendiente && exp < app.today { DANGER } else { MUTED }),
            cell(money(b.total())),
        ]);
        if tbl.row(ui, i, false, &cells).clicked() {
            open = Some(b.id);
        }
    }
    if let Some(id) = open {
        app.dialog = Dialog::Budget(id);
    }
}

pub fn patient_list(app: &mut App, ui: &mut Ui, pid: u32) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Presupuestos del paciente");
            right_aligned(ui, |ui| {
                if primary(ui, "＋ Nuevo presupuesto").clicked() {
                    let id = new_budget(app, pid);
                    app.dialog = Dialog::Budget(id);
                }
                let n = app.db.entries.iter().filter(|e| e.patient_id == pid && e.state == TxState::Previsto && e.budget_id.is_none()).count();
                if n > 0 && ui.button(format!("Desde previstos del odontograma ({n})")).clicked() && let Some(id) = app.db.budget_from_planned(pid) {
                    app.changed();
                    app.dialog = Dialog::Budget(id);
                }
            });
        });
        let mut list: Vec<Budget> = app.db.budgets.iter().filter(|b| b.patient_id == pid).cloned().collect();
        list.sort_by_key(|b| std::cmp::Reverse((b.date, b.id)));
        table(app, ui, &list, false);
    });
}

pub fn edit_dialog(app: &mut App, ctx: &egui::Context, id: u32) -> Dialog {
    let Some(mut b) = app.db.budgets.iter().find(|b| b.id == id).cloned() else { return Dialog::None };
    let original = serde_json::to_string(&b).unwrap_or_default();
    let mut open = true;
    let mut accept = false;
    let mut reject = false;
    let mut delete = false;
    let mut to_patient = false;
    let editable = b.state == BudgetState::Pendiente;
    crate::theme::modal("budget_dlg").show(ctx, |ui| {
        ui.set_width(980.0);
        ui.horizontal(|ui| {
            title(ui, &format!("Presupuesto #{} · {}", b.id, app.db.patient_name(b.patient_id)), &format!("Creado el {} · válido hasta {}", fmt_date(b.date), fmt_date(b.expires())));
            right_aligned(ui, |ui| {
                pill(ui, b.state.label(), state_color(b.state));
            });
        });
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add_enabled_ui(editable, |ui| {
                ui.horizontal(|ui| {
                    field(ui, "Título", &mut b.title, 320.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Doctor").size(13.5).color(MUTED));
                        crate::theme::combo("b_doc").width(130.0).selected_text(app.db.doctor_alias(b.doctor_id)).show_ui(ui, |ui| {
                            for c in &app.db.collaborators {
                                ui.selectable_value(&mut b.doctor_id, c.id, &c.alias);
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Tarifa").size(13.5).color(MUTED));
                        let before = b.tariff_id;
                        crate::theme::combo("b_tar").width(190.0).selected_text(app.db.tariff_name(b.tariff_id)).show_ui(ui, |ui| {
                            for t in &app.db.tariffs {
                                ui.selectable_value(&mut b.tariff_id, t.id, &t.name);
                            }
                        });
                        if before != b.tariff_id {
                            for l in b.lines.iter_mut() {
                                l.price = app.db.price_for(&l.code, b.tariff_id);
                            }
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Validez (días)").size(13.5).color(MUTED));
                        ui.add(egui::DragValue::new(&mut b.valid_days).range(1..=730));
                    });
                });
            });
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Líneas");
            let tbl = Table::new("blines", vec![("Código", 64.0), ("Tratamiento", 300.0), ("Piezas", 110.0), ("Uds.", 70.0), ("Precio", 100.0), ("Dto. %", 80.0), ("Importe", 110.0), ("", 40.0)]);
            tbl.header(ui);
            let mut remove = None;
            for (i, l) in b.lines.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    lab(ui, 60.0, &l.code);
                    lab(ui, 296.0, &l.description);
                    let teeth: Vec<String> = l.teeth.iter().map(|t| t.to_string()).collect();
                    lab(ui, 106.0, teeth.join("-"));
                    ui.add_enabled_ui(editable, |ui| {
                        ui.add_sized([66.0, 32.0], egui::DragValue::new(&mut l.units).range(1..=99));
                        ui.add_sized([96.0, 32.0], egui::DragValue::new(&mut l.price).range(0.0..=100000.0).speed(1.0).suffix(" €"));
                        ui.add_sized([76.0, 32.0], egui::DragValue::new(&mut l.discount_pct).range(0.0..=100.0).speed(0.5).suffix(" %"));
                    });
                    lab(ui, 106.0, RichText::new(money(l.amount())).family(crate::theme::sb()));
                    if editable && ui.small_button("✖").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                b.lines.remove(i);
            }
            if editable {
                add_line(app, ui, &mut b);
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.add_enabled_ui(editable, |ui| {
                    ui.label("Descuento global:");
                    let key = egui::Id::unique(("gdisc", b.id));
                    let mut g: f64 = ui.data_mut(|d| d.get_temp(key)).unwrap_or(0.0);
                    ui.add(egui::DragValue::new(&mut g).range(0.0..=100.0).suffix(" %"));
                    if ui.button("Aplicar a todas las líneas").clicked() {
                        for l in b.lines.iter_mut() {
                            l.discount_pct = g;
                        }
                    }
                    ui.data_mut(|d| d.insert_temp(key, g));
                });
                right_aligned(ui, |ui| {
                    ui.label(RichText::new(format!("TOTAL {}", money(b.total()))).size(22.5).family(crate::theme::sb()).color(ACCENT));
                    let gross: f64 = b.lines.iter().map(|l| l.price * l.units as f64).sum();
                    if gross > b.total() + 0.005 {
                        muted(ui, format!("Bruto {} · ahorro {}", money(gross), money(gross - b.total())));
                    }
                });
            });
        });
        ui.add_space(6.0);
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Financiación");
                let mut has = b.plan.is_some();
                ui.add_enabled_ui(b.state != BudgetState::Cerrado, |ui| {
                    if ui.checkbox(&mut has, "Pagar a plazos").changed() {
                        b.plan = if has { Some(FinancePlan { down_payment: 0.0, installments: 6, first_date: add_months(app.today, 1), every_months: 1, paid: vec![false; 6] }) } else { None };
                    }
                    let total = b.total();
                    if let Some(pl) = &mut b.plan {
                        ui.horizontal(|ui| {
                            ui.label("Entrada");
                            ui.add(egui::DragValue::new(&mut pl.down_payment).range(0.0..=total).speed(10.0).suffix(" €"));
                            ui.label("Cuotas");
                            if ui.add(egui::DragValue::new(&mut pl.installments).range(1..=60)).changed() {
                                pl.paid.resize(pl.installments as usize, false);
                            }
                            ui.label("cada");
                            ui.add(egui::DragValue::new(&mut pl.every_months).range(1..=12).suffix(" mes"));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Primera cuota:");
                            crate::screens::date_picker(ui, "plan_first", &mut pl.first_date);
                        });
                        ui.label(RichText::new(pl.installments_text(total)).family(crate::theme::sb()));
                        muted(ui, format!("Última cuota: {}", fmt_date(pl.due_date(pl.installments.saturating_sub(1)))));
                    }
                });
            });
            card(&mut cols[1], |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Notas y seguimiento");
                ui.add(egui::TextEdit::multiline(&mut b.notes).desired_rows(3).desired_width(f32::INFINITY).hint_text("Observaciones para el paciente"));
                if let Some(d) = b.accepted_on {
                    ui.label(RichText::new(format!("Aceptado el {}", fmt_date(d))).color(OK));
                }
                if let Some(d) = b.rejected_on {
                    ui.label(RichText::new(format!("Rechazado el {}: {}", fmt_date(d), b.reject_reason)).color(DANGER));
                }
                if b.state == BudgetState::Pendiente {
                    ui.horizontal(|ui| {
                        ui.add_sized([220.0, 32.0], egui::TextEdit::singleline(&mut app.lists.reject_reason).hint_text("Motivo de rechazo"));
                        if ui.button("Rechazar").clicked() {
                            reject = true;
                        }
                    });
                }
            });
        });
        // enviarlo para firmar por internet (con la página de cita online activada)
        if app.db.booking.enabled && app.db.booking.budgets && (b.state == BudgetState::Pendiente || app.db.budget_link(b.id).is_some()) {
            ui.add_space(6.0);
            crate::screens::cita_online::budget_link_row(app, ui, b.id);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if b.state == BudgetState::Pendiente && primary(ui, "✔ Aceptar presupuesto").on_hover_text("Pasa los tratamientos al historial del paciente como previstos").clicked() {
                accept = true;
            }
            if ui.button("Guardar y cerrar").clicked() {
                open = false;
            }
            if ui.button("Ver ficha del paciente").clicked() {
                to_patient = true;
            }
            if primary(ui, "🖨 Imprimir / PDF").clicked() {
                match crate::screens::centros::print_budget(app, ui.ctx(), b.id) {
                    Ok(p) => app.notify(format!("Presupuesto listo para imprimir: {}", p.display())),
                    Err(e) => app.warn(e),
                }
            }
            if ui.button("📄 Exportar a texto").clicked() {
                match export_txt(app, &b) {
                    Ok(p) => app.notify(format!("Presupuesto exportado en {p}")),
                    Err(e) => app.warn(format!("No se pudo exportar: {e}")),
                }
            }
            right_aligned(ui, |ui| {
                if danger(ui, "Borrar").clicked() {
                    delete = true;
                }
            });
        });
    });
    if serde_json::to_string(&b).unwrap_or_default() != original {
        if let Some(x) = app.db.budgets.iter_mut().find(|x| x.id == id) {
            *x = b.clone();
        }
        app.changed();
    }
    if accept {
        if b.lines.is_empty() {
            app.warn("El presupuesto no tiene líneas.");
            return Dialog::Budget(id);
        }
        app.db.accept_budget(id, app.today);
        if let Some(pl) = &b.plan {
            if pl.down_payment > 0.0 {
                app.notify(format!("Presupuesto aceptado. Recuerde cobrar la entrada de {}.", money(pl.down_payment)));
            } else {
                app.notify("Presupuesto aceptado: tratamientos añadidos como previstos.");
            }
        } else {
            app.notify("Presupuesto aceptado: tratamientos añadidos como previstos.");
        }
        app.changed();
        return Dialog::Budget(id);
    }
    if reject {
        let reason = std::mem::take(&mut app.lists.reject_reason);
        app.db.reject_budget(id, if reason.is_empty() { "Sin especificar" } else { &reason });
        app.changed();
        app.notify("Presupuesto marcado como rechazado.");
        return Dialog::Budget(id);
    }
    if delete {
        return Dialog::Confirm { text: format!("¿Borrar el presupuesto #{id}?"), action: ConfirmAction::DeleteBudget(id) };
    }
    if to_patient {
        app.open_patient(b.patient_id, PatTab::Presupuestos);
        return Dialog::None;
    }
    if open { Dialog::Budget(id) } else { app.dialog_take_or_none() }
}

fn add_line(app: &App, ui: &mut Ui, b: &mut Budget) {
    let key = egui::Id::unique(("bline", b.id));
    let first = app.db.treatments_for(b.tariff_id).find(|t| t.odg != OdgKind::Ausente).map(|t| t.code.clone()).unwrap_or_default();
    let mut st: (String, String) = ui.data_mut(|d| d.get_temp(key)).unwrap_or((first, String::new()));
    ui.horizontal(|ui| {
        ui.label(RichText::new("Añadir línea:").color(MUTED));
        let tariff = b.tariff_id;
        crate::screens::treatment_picker(ui, &format!("bl_tto{}", b.id), &app.db, tariff, &mut st.0, 320.0, |t| t.odg != OdgKind::Ausente, |t| format!("{} · {} · {}", t.code, t.description, money(app.db.price_for(&t.code, tariff))));
        ui.add_sized([110.0, 32.0], egui::TextEdit::singleline(&mut st.1).hint_text("Piezas: 36 37"));
        if ui.button("＋ Añadir").clicked() && let Some(t) = app.db.treatment(&st.0).filter(|t| app.db.treatment_offered(t, tariff)) {
            let teeth: Vec<u8> = st.1.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).filter(|t| crate::screens::pacientes::valid_tooth(*t)).collect();
            let units = if t.per_tooth { teeth.len().max(1) as u32 } else { 1 };
            b.lines.push(BudgetLine {
                code: t.code.clone(),
                description: t.description.clone(),
                teeth,
                units,
                price: app.db.price_for(&t.code, b.tariff_id),
                discount_pct: app.db.patient(b.patient_id).map(|p| p.discount_pct).unwrap_or(0.0),
                odg: t.odg,
            });
            st.1.clear();
        }
    });
    ui.data_mut(|d| d.insert_temp(key, st));
}

fn export_txt(app: &App, b: &Budget) -> Result<String, String> {
    let dir = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| std::path::PathBuf::from(".")).join(crate::brand::DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("presupuesto_{}.txt", b.id));
    let p = app.db.patient(b.patient_id).cloned().unwrap_or_default();
    let mut s = String::new();
    s.push_str(&format!("{}\n{}\n{}\n\n", app.db.config.name, app.db.config.address, app.db.config.phone));
    s.push_str(&format!("PRESUPUESTO Nº {}  ·  {}\n", b.id, fmt_date(b.date)));
    s.push_str(&format!("Paciente: {}  (Nº historial {})\n", p.full_name(), p.history_no));
    s.push_str(&format!("Doctor: {}   Tarifa: {}\n\n", app.db.doctor_alias(b.doctor_id), app.db.tariff_name(b.tariff_id)));
    for l in &b.lines {
        let teeth: Vec<String> = l.teeth.iter().map(|t| t.to_string()).collect();
        s.push_str(&format!("{:<6} {:<40} {:<12} {:>3} x {:>12}  -{:>4.1}%  {:>14}\n", l.code, l.description, teeth.join("-"), l.units, money(l.price), l.discount_pct, money(l.amount())));
    }
    s.push_str(&format!("\nTOTAL: {}\nVálido hasta {}\n", money(b.total()), fmt_date(b.expires())));
    if let Some(pl) = &b.plan {
        s.push_str(&format!("Financiación: entrada {} + {}\n", money(pl.down_payment), pl.installments_text(b.total())));
    }
    if !b.notes.is_empty() {
        s.push_str(&format!("\n{}\n", b.notes));
    }
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}
