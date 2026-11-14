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
        BudgetState::Cerrado => MUTED(),
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
        tp: Default::default(),
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
        let mut list: Vec<Budget> =
            app.db.budgets.iter().filter(|b| app.lists.budget_filter.map(|f| b.state == f).unwrap_or(true)).filter(|b| q.is_empty() || fold(&app.db.patient_name(b.patient_id)).contains(&q) || fold(&b.title).contains(&q)).cloned().collect();
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
            cellc(fmt_date(exp), if b.state == BudgetState::Pendiente && exp < app.today { DANGER } else { MUTED() }),
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
                if n > 0
                    && ui.button(format!("Desde previstos del odontograma ({n})")).clicked()
                    && let Some(id) = app.db.budget_from_planned(pid)
                {
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
    // plan de tratamiento: alternativa nueva, versión nueva o abrir otra del plan
    let mut plan_act: Option<u8> = None;
    let mut open_other: Option<u32> = None;
    // una versión sustituida se conserva tal cual
    let editable = b.state == BudgetState::Pendiente && b.tp.replaced_by.is_none();
    // con cuotas cobradas o en una remesa, el plan ya no se toca (ni se borra el presupuesto)
    let locked = app.db.plan_locked(id);
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
                        ui.label(RichText::new("Doctor").size(13.5).color(MUTED()));
                        crate::theme::combo("b_doc").width(130.0).selected_text(app.db.doctor_alias(b.doctor_id)).show_ui(ui, |ui| {
                            for c in &app.db.collaborators {
                                ui.selectable_value(&mut b.doctor_id, c.id, &c.alias);
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Tarifa").size(13.5).color(MUTED()));
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
                        ui.label(RichText::new("Validez (días)").size(13.5).color(MUTED()));
                        ui.add(egui::DragValue::new(&mut b.valid_days).range(1..=730));
                    });
                });
            });
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Líneas");
            let tbl = Table::new("blines", vec![("Código", 64.0), ("Tratamiento", 250.0), ("Piezas", 110.0), ("Fase", 60.0), ("Uds.", 70.0), ("Precio", 100.0), ("Dto. %", 80.0), ("Importe", 110.0), ("", 40.0)]);
            tbl.header(ui);
            let mut remove = None;
            for (i, l) in b.lines.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    lab(ui, 60.0, &l.code);
                    lab(ui, 246.0, &l.description);
                    let teeth: Vec<String> = l.teeth.iter().map(|t| t.to_string()).collect();
                    lab(ui, 106.0, teeth.join("-"));
                    ui.add_enabled_ui(editable, |ui| {
                        let mut ph = l.phase as u32 + 1;
                        if ui.add_sized([56.0, 32.0], egui::DragValue::new(&mut ph).range(1..=9)).on_hover_text("Fase del plan de tratamiento").changed() {
                            l.phase = (ph - 1) as u8;
                        }
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
                if let Some(why) = &locked {
                    ui.label(RichText::new(format!("🔒 El plan ya no se puede cambiar: {why}.")).color(WARN).size(13.0));
                }
                ui.add_enabled_ui(b.state != BudgetState::Cerrado && locked.is_none(), |ui| {
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
        ui.add_space(6.0);
        plan_card(app, ui, &mut b, editable, &mut plan_act, &mut open_other);
        // enviarlo para firmar por internet (con la página de cita online activada)
        if app.db.booking.enabled && app.db.booking.budgets && (b.state == BudgetState::Pendiente || app.db.budget_link(b.id).is_some()) {
            ui.add_space(6.0);
            crate::screens::cita_online::budget_link_row(app, ui, b.id);
        }
        // pago por internet del presupuesto y de la próxima cuota sin pagar
        if !matches!(b.state, BudgetState::Rechazado | BudgetState::Cerrado) {
            crate::screens::pagos::pay_row(app, ui, crate::pago::PayFor::Budget(b.id), "Pago por internet");
            if let Some(i) = b.plan.as_ref().and_then(|pl| (0..pl.installments).find(|i| !pl.paid.get(*i as usize).copied().unwrap_or(false))) {
                crate::screens::pagos::pay_row(app, ui, crate::pago::PayFor::Installment(b.id, i), &format!("Cuota {} por internet", i + 1));
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if b.state == BudgetState::Pendiente {
                let problem = app.db.accept_problem(id).filter(|_| !b.lines.is_empty());
                let label = if b.tp.alt.is_empty() { "✔ Aceptar presupuesto".to_string() } else { format!("✔ Aceptar la alternativa {}", b.tp.alt) };
                let r = ui.add_enabled(problem.is_none(), egui::Button::new(RichText::new(label).color(egui::Color32::WHITE)).fill(ACCENT));
                let r = match &problem {
                    Some(p) => r.on_disabled_hover_text(p),
                    None => r.on_hover_text("Pasa los tratamientos al historial del paciente como previstos y descarta las demás alternativas del plan"),
                };
                if r.clicked() {
                    accept = true;
                }
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
                if ui.add_enabled(locked.is_none(), egui::Button::new(RichText::new("Borrar").color(egui::Color32::WHITE)).fill(DANGER)).on_disabled_hover_text("Tiene cuotas cobradas o en una remesa").clicked() {
                    delete = true;
                }
            });
        });
    });
    if serde_json::to_string(&b).unwrap_or_default() != original {
        let was = app.db.budgets.iter().find(|x| x.id == id).and_then(|x| x.plan.clone());
        if locked.is_some() {
            // por si acaso: el plan bloqueado se queda como estaba
            b.plan = was.clone();
        }
        if b.state == BudgetState::Aceptado && b.plan != was {
            let text = b.plan.as_ref().map(|p| p.installments_text(b.total())).unwrap_or_else(|| "sin plan".into());
            app.db.audit("Cambiar plan de cuotas", &format!("presupuesto #{id} · {} · {text}", app.db.patient_name(b.patient_id)));
        }
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
        let who = app.db.config.current_user.clone();
        let r = match app.db.accept_plan(id, &who, app.today) {
            Ok(r) => r,
            Err(e) => {
                app.warn(e);
                return Dialog::Budget(id);
            }
        };
        let mut msg = "Presupuesto aceptado: tratamientos añadidos como previstos.".to_string();
        if !r.discarded.is_empty() {
            msg.push_str(&format!(" Descartadas {} alternativa(s) del plan.", r.discarded.len()));
        }
        if !r.consents.is_empty() {
            msg.push_str(&format!(" {} consentimiento(s) preparados para firmar en Documentos.", r.consents.len()));
        }
        if let Some(pl) = &b.plan
            && pl.down_payment > 0.0
        {
            msg.push_str(&format!(" Recuerde cobrar la entrada de {}.", money(pl.down_payment)));
        }
        app.notify(msg);
        app.changed();
        return Dialog::Budget(id);
    }
    if let Some(other) = open_other {
        return Dialog::Budget(other);
    }
    if let Some(act) = plan_act {
        let made = if act == 0 { app.db.new_alternative(id) } else { app.db.new_version(id) };
        match made {
            Ok(n) => {
                app.changed();
                app.notify(if act == 0 { "Alternativa creada: cámbiela como haga falta." } else { "Versión nueva creada: la anterior se conserva tal cual." });
                return Dialog::Budget(n);
            }
            Err(e) => app.warn(e),
        }
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

/// Plan de tratamiento: fases con su subtotal, alternativas y versiones (ver `crate::plan`).
fn plan_card(app: &App, ui: &mut Ui, b: &mut Budget, editable: bool, act: &mut Option<u8>, open_other: &mut Option<u32>) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Plan de tratamiento");
            let label = b.plan_label();
            if !label.is_empty() {
                pill(ui, &label, INFO);
            }
        });
        if let Some(n) = b.tp.replaced_by {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sustituido por una versión más nueva: se conserva tal cual y ya no se puede aceptar.").color(WARN));
                if ui.small_button("Abrir la versión nueva").clicked() {
                    *open_other = Some(n);
                }
            });
        }
        if let Some(n) = b.tp.discarded_for {
            ui.label(RichText::new(format!("Descartado: se eligió otra alternativa del plan (#{n}).")).color(MUTED()));
        }
        // fases con su subtotal (suman exactamente el total)
        let phases = b.phase_totals();
        if phases.len() > 1 {
            for (p, _name, sum) in &phases {
                ui.horizontal(|ui| {
                    let i = *p as usize;
                    if b.tp.phases.len() <= i {
                        b.tp.phases.resize(i + 1, String::new());
                    }
                    ui.add_enabled(editable, egui::TextEdit::singleline(&mut b.tp.phases[i]).hint_text(format!("Fase {}", i + 1)).desired_width(220.0));
                    ui.label(RichText::new(sum.to_string()).family(crate::theme::sb()));
                });
            }
            ui.label(RichText::new(format!("Total del plan: {}", b.total_cents())).family(crate::theme::sb()));
        } else {
            muted(ui, "Ponga cada línea en su fase (columna «Fase») para ver el plan por fases con su subtotal.");
        }
        // las demás alternativas del plan
        let others: Vec<(u32, String, String, BudgetState)> = app.db.plan_alternatives(b.id).iter().filter(|x| x.id != b.id).map(|x| (x.id, x.tp.alt.clone(), x.total_cents().to_string(), x.state)).collect();
        for (oid, alt, total, st) in others {
            ui.horizontal(|ui| {
                ui.label(format!("Alternativa {alt} · {total} · {}", st.label()));
                if ui.small_button("Abrir").clicked() {
                    *open_other = Some(oid);
                }
            });
        }
        if b.state == BudgetState::Pendiente && b.tp.replaced_by.is_none() {
            ui.horizontal(|ui| {
                if ui.button("Crear alternativa").on_hover_text("Una copia en el mismo plan para cambiarla (por ejemplo, un puente en vez de un implante)").clicked() {
                    *act = Some(0);
                }
                if ui.button("Versión nueva").on_hover_text("Para cambiarlo después de presentarlo: la versión anterior se conserva tal cual").clicked() {
                    *act = Some(1);
                }
            });
        }
    });
}

fn add_line(app: &App, ui: &mut Ui, b: &mut Budget) {
    let key = egui::Id::unique(("bline", b.id));
    let first = app.db.treatments_for(b.tariff_id).find(|t| t.odg != OdgKind::Ausente).map(|t| t.code.clone()).unwrap_or_default();
    let mut st: (String, String) = ui.data_mut(|d| d.get_temp(key)).unwrap_or((first, String::new()));
    ui.horizontal(|ui| {
        ui.label(RichText::new("Añadir línea:").color(MUTED()));
        let tariff = b.tariff_id;
        crate::screens::treatment_picker(ui, &format!("bl_tto{}", b.id), &app.db, tariff, &mut st.0, 320.0, |t| t.odg != OdgKind::Ausente, |t| format!("{} · {} · {}", t.code, t.description, money(app.db.price_for(&t.code, tariff))));
        ui.add_sized([110.0, 32.0], egui::TextEdit::singleline(&mut st.1).hint_text("Piezas: 36 37"));
        if ui.button("＋ Añadir").clicked()
            && let Some(t) = app.db.treatment(&st.0).filter(|t| app.db.treatment_offered(t, tariff))
        {
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
                phase: b.lines.last().map(|l| l.phase).unwrap_or(0),
            });
            st.1.clear();
        }
    });
    ui.data_mut(|d| d.insert_temp(key, st));
}

fn export_txt(app: &App, b: &Budget) -> Result<String, String> {
    let dir = crate::paths::documents().join(crate::brand::DIR);
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
