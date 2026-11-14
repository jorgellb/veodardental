//! Inicio (pizarra): el día de un vistazo.

use crate::app::{App, Dialog, PatTab, Screen};
use crate::model::*;
use crate::screens::agenda::{ApptDraft, set_state};
use crate::theme::*;
use chrono::Datelike;
use egui::{CornerRadius, RichText, Sense, Ui, vec2};

pub fn show(app: &mut App, ui: &mut Ui) {
    let today = app.today;
    let hour = chrono::Local::now().format("%H").to_string().parse::<u32>().unwrap_or(10);
    let greet = if hour < 14 {
        "Buenos días"
    } else if hour < 21 {
        "Buenas tardes"
    } else {
        "Buenas noches"
    };
    title(ui, &format!("{greet}, {}", app.db.config.current_user), &format!("Resumen de {}", long_date(today)));
    crate::screens::jornada::punch_card(app, ui);
    ui.add_space(8.0);

    let center = cid(app.db.config.current_center);
    let day: Vec<Appointment> = app.db.appointments.iter().filter(|a| a.date == today && cid(a.center_id) == center).cloned().collect();
    let active = day.iter().filter(|a| !a.state.is_cancelled()).count();
    let waiting = day.iter().filter(|a| a.state == ApptState::SalaEspera).count();
    let cobrado: f64 = app.db.payments.iter().filter(|p| p.date.date() == today && cid(p.center_id) == center).map(|p| p.amount).sum();
    let pending: f64 = app.db.patients.iter().map(|p| app.db.patient_balance(p.id).max(0.0)).sum();
    let w = kpi_w(ui, 4);
    ui.horizontal(|ui| {
        kpi_icon(ui, w, "Citas de hoy", &active.to_string(), &format!("{} finalizadas", day.iter().filter(|a| a.state == ApptState::Finalizada).count()), INFO, Some(Screen::Agenda));
        ui.add_space(4.0);
        kpi_icon(ui, w, "En sala de espera", &waiting.to_string(), if waiting > 0 { "pacientes esperando" } else { "nadie esperando" }, WARN, Some(Screen::Pacientes));
        ui.add_space(4.0);
        kpi_icon(ui, w, "Cobrado hoy", &money(cobrado), &format!("{} cobros", app.db.payments.iter().filter(|p| p.date.date() == today && cid(p.center_id) == center).count()), OK, Some(Screen::Cobros));
        ui.add_space(4.0);
        kpi_icon(ui, w, "Pendiente de cobro", &money(pending), "total de pacientes", DANGER, Some(Screen::Facturas));
    });
    ui.add_space(10.0);
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail * 0.58);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    section(ui, "Pizarra del día");
                    right_aligned(ui, |ui| {
                        if ui.button("Abrir agenda").clicked() {
                            app.agenda.date = today;
                            app.screen = Screen::Agenda;
                        }
                    });
                });
                let mut list = day.clone();
                list.sort_by_key(|a| (a.start, a.doctor_id));
                if list.is_empty() {
                    empty_state(ui, "No hay citas hoy");
                }
                egui::ScrollArea::vertical().id_salt("pizarra").max_height(520.0).show(ui, |ui| {
                    for a in &list {
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
                        let p = ui.painter();
                        let sc = rgb(a.state.color());
                        if resp.hovered() {
                            p.rect_filled(rect, CornerRadius::same(12), ROW_HOVER);
                        }
                        p.hline(rect.x_range().shrink(8.0), rect.max.y - 0.5, egui::Stroke::new(1.0, HAIRLINE()));
                        // hora
                        p.text(rect.min + vec2(14.0, 13.0), egui::Align2::LEFT_TOP, a.start.format("%H:%M").to_string(), bf(16.0), if a.state.is_cancelled() { SUBTLE() } else { HEADING });
                        p.text(rect.min + vec2(14.0, 36.0), egui::Align2::LEFT_TOP, format!("{} min", a.minutes), mdf(12.0), SUBTLE());
                        // línea de tiempo con el estado de la cita
                        let x = rect.min.x + 80.0;
                        p.vline(x, rect.y_range(), egui::Stroke::new(2.0, HAIRLINE()));
                        p.circle_filled(egui::pos2(x, rect.center().y), 6.5, CARD);
                        p.circle_stroke(egui::pos2(x, rect.center().y), 5.5, egui::Stroke::new(2.6, sc));
                        let name = app.db.appt_name(a);
                        let dcol = app.db.doctor(a.doctor_id).map(|d| rgb(d.color)).unwrap_or(ACCENT);
                        avatar(p, egui::pos2(x + 34.0, rect.center().y), 17.0, &name, dcol);
                        let alert = a.patient_id.and_then(|id| app.db.patient(id)).map(|p| !p.alerts.is_empty() || !p.allergies.is_empty()).unwrap_or(false);
                        let has_next = a.state.next().is_some();
                        let pill_x = rect.max.x - if has_next { 262.0 } else { 140.0 };
                        let text_w = (pill_x - 12.0 - (x + 60.0)).max(80.0);
                        let clip = p.with_clip_rect(egui::Rect::from_min_size(egui::pos2(x + 60.0, rect.min.y), vec2(text_w, rect.height())));
                        clip.text(
                            egui::pos2(x + 62.0, rect.min.y + 13.0),
                            egui::Align2::LEFT_TOP,
                            format!("{}{}", name, if alert { "  ⚠" } else { "" }),
                            sbf(15.0),
                            if a.state.is_cancelled() {
                                MUTED()
                            } else if alert {
                                mix(DANGER, HEADING, 0.3)
                            } else {
                                HEADING
                            },
                        );
                        clip.text(egui::pos2(x + 62.0, rect.min.y + 36.0), egui::Align2::LEFT_TOP, format!("{} · {} · {}", a.reason, app.db.doctor_alias(a.doctor_id), app.db.box_name(a.box_id)), egui::FontId::proportional(13.0), MUTED());
                        let pr = egui::Rect::from_min_size(egui::pos2(pill_x, rect.min.y + 19.0), vec2(126.0, 26.0));
                        p.rect_filled(pr, CornerRadius::same(13), tint(sc, 0.11));
                        p.rect_stroke(pr, CornerRadius::same(13), egui::Stroke::new(1.0, tint(sc, 0.28)), egui::StrokeKind::Inside);
                        p.text(pr.center(), egui::Align2::CENTER_CENTER, a.state.label(), mdf(12.5), mix(sc, HEADING, 0.2));
                        // botón de siguiente paso
                        if let Some(next) = a.state.next() {
                            let br = egui::Rect::from_min_size(rect.right_top() + vec2(-126.0, 17.0), vec2(114.0, 30.0));
                            let bresp = ui.interact(br, ui.scope_id().with(("next", a.id)), Sense::click());
                            let label = match next {
                                ApptState::SalaEspera => "Llega  ›",
                                ApptState::EnConsulta => "A consulta  ›",
                                _ => "Finalizar  ›",
                            };
                            let hov = bresp.hovered();
                            ui.painter().rect_filled(br, CornerRadius::same(10), if hov { ACCENT } else { CARD });
                            ui.painter().rect_stroke(br, CornerRadius::same(10), egui::Stroke::new(1.0, if hov { ACCENT_DARK } else { tint(ACCENT, 0.4) }), egui::StrokeKind::Inside);
                            ui.painter().text(br.center(), egui::Align2::CENTER_CENTER, label, sbf(13.0), if hov { egui::Color32::WHITE } else { ACCENT_DARK });
                            if hov {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if bresp.clicked() {
                                set_state(&mut app.db, a.id, next);
                                app.changed();
                                continue;
                            }
                        }
                        if resp.clicked() {
                            app.dialog = Dialog::Appointment(ApptDraft::from(a));
                        }
                    }
                });
            });
        });
        ui.vertical(|ui| {
            ui.set_width(avail * 0.42 - 12.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Avisos");
                let overdue = app.db.recalls.iter().filter(|r| app.db.recall_due(r) && r.date <= today).count();
                let exp = app.db.budgets.iter().filter(|b| b.state == BudgetState::Pendiente && b.expires() < today).count();
                let pend_b = app.db.budgets.iter().filter(|b| b.state == BudgetState::Pendiente).count();
                let unconfirmed = app.db.appointments.iter().filter(|a| a.date == today + chrono::Duration::days(1) && cid(a.center_id) == center && a.state == ApptState::Planificada).count();
                let mut any = false;
                // lo que falta para trabajar con la clínica real (solo lo ve quien puede configurar)
                if app.can(crate::users::Perm::Configuracion) {
                    let left = crate::screens::marcha::pending(app, ui.input(|i| i.time));
                    if left > 0 {
                        any = true;
                        if aviso(ui, VIOLET, format!("Puesta en marcha: {} para trabajar con la clínica real", if left == 1 { "falta 1 paso".to_string() } else { format!("faltan {left} pasos") })) {
                            crate::screens::config::open(app, crate::screens::config::CfgTab::Marcha);
                        }
                    }
                }
                // copias de seguridad que fallan o que no se hacen (solo Dirección)
                if let Some(w) = app.backup_warning() {
                    any = true;
                    if aviso(ui, DANGER, w) {
                        crate::screens::config::open(app, crate::screens::config::CfgTab::Copias);
                    }
                }
                if app.db.open_cash().is_none() {
                    any = true;
                    if aviso(ui, DANGER, "La caja está cerrada: ábrala para cobrar en efectivo") {
                        app.screen = Screen::Caja;
                    }
                }
                if overdue > 0 {
                    any = true;
                    if aviso(ui, WARN, format!("{overdue} recalls vencidos por llamar")) {
                        app.screen = Screen::Recalls;
                    }
                }
                if unconfirmed > 0 {
                    any = true;
                    if aviso(ui, INFO, format!("{unconfirmed} citas de mañana sin confirmar")) {
                        app.agenda.date = today + chrono::Duration::days(1);
                        app.screen = Screen::Agenda;
                    }
                }
                let low = app.db.low_stock().len();
                if low > 0 {
                    any = true;
                    if aviso(ui, DANGER, format!("{low} productos del almacén bajo mínimo")) {
                        app.screen = Screen::Almacen;
                    }
                }
                let lots = app.db.expiring_lots(today + chrono::Duration::days(crate::screens::almacen::EXPIRY_DAYS));
                let expiring = lots.len();
                if expiring > 0 {
                    any = true;
                    let gone = lots.iter().any(|(_, l)| l.expiry.is_some_and(|e| e < today));
                    let text = match (expiring, gone) {
                        (1, true) => "1 lote caducado en el almacén".to_string(),
                        (n, true) => format!("{n} lotes del almacén caducados o a punto de caducar"),
                        (1, false) => format!("1 lote del almacén caduca en menos de {} días", crate::screens::almacen::EXPIRY_DAYS),
                        (n, false) => format!("{n} lotes del almacén caducan en menos de {} días", crate::screens::almacen::EXPIRY_DAYS),
                    };
                    if aviso(ui, WARN, text) {
                        app.screen = Screen::Almacen;
                    }
                }
                let late = app.db.lab_works.iter().filter(|w| w.overdue(today)).count();
                if late > 0 {
                    any = true;
                    if aviso(ui, WARN, format!("{late} trabajos de laboratorio retrasados")) {
                        app.screen = Screen::Laboratorio;
                    }
                }
                if pend_b > 0 {
                    any = true;
                    if aviso(ui, INFO, format!("{pend_b} presupuestos pendientes de aceptar ({exp} caducados)")) {
                        app.screen = Screen::Presupuestos;
                    }
                }
                if !any {
                    muted(ui, "Todo en orden.");
                }
            });
            ui.add_space(8.0);
            crate::screens::tareas::home_card(app, ui);
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Cumpleaños de hoy");
                let bdays: Vec<&Patient> = app.db.patients.iter().filter(|p| p.birth.map(|b| b.month() == today.month() && b.day() == today.day()).unwrap_or(false)).collect();
                if bdays.is_empty() {
                    muted(ui, "Ninguno.");
                }
                for p in bdays {
                    ui.label(format!("🎂 {} ({} años)", p.full_name(), p.age(today).unwrap_or(0)));
                }
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Pacientes recientes");
                let mut recent: Vec<(chrono::NaiveDate, u32)> = app.db.entries.iter().map(|e| (e.date, e.patient_id)).collect();
                recent.sort_by_key(|r| std::cmp::Reverse(r.0));
                let mut seen = vec![];
                for (_, pid) in recent {
                    if seen.contains(&pid) {
                        continue;
                    }
                    seen.push(pid);
                    if seen.len() > 6 {
                        break;
                    }
                    let bal = app.db.patient_balance(pid);
                    ui.horizontal(|ui| {
                        if ui.link(app.db.patient_name(pid)).clicked() {
                            app.open_patient(pid, PatTab::Resumen);
                        }
                        right_aligned(ui, |ui| {
                            if bal > 0.005 {
                                ui.label(RichText::new(money(bal)).color(DANGER).size(14.0));
                            }
                        });
                    });
                }
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Cobros de los últimos 6 meses");
                let data: Vec<(String, f64, f64)> = (0..6)
                    .rev()
                    .map(|i| {
                        let total = today.year() * 12 + today.month0() as i32 - i;
                        let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
                        let cob: f64 = app.db.payments.iter().filter(|p| p.date.year() == y && p.date.month() == m).map(|p| p.amount).sum();
                        let prod: f64 = app.db.entries.iter().filter(|e| e.is_act() && e.date.year() == y && e.date.month() == m).map(|e| e.amount()).sum();
                        (month_es(m)[..3].to_string(), cob, prod)
                    })
                    .collect();
                crate::screens::informes::chart(ui, &data, 170.0);
            });
        });
    });
}

/// Fila de aviso: punto de color, texto y flecha; devuelve si se pulsó.
fn aviso(ui: &mut Ui, color: egui::Color32, text: impl Into<String>) -> bool {
    let text = text.into();
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    // legible para lectores de pantalla (y para las pruebas)
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &text));
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(10), ROW_HOVER);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let c = rect.left_center() + vec2(18.0, 0.0);
    p.circle_filled(c, 11.0, tint(color, 0.13));
    p.circle_filled(c, 4.0, color);
    p.with_clip_rect(rect.shrink2(vec2(30.0, 0.0))).text(rect.left_center() + vec2(40.0, 0.0), egui::Align2::LEFT_CENTER, text, mdf(14.0), HEADING);
    p.text(rect.right_center() - vec2(14.0, 1.0), egui::Align2::RIGHT_CENTER, "›", sbf(18.0), if resp.hovered() { ACCENT } else { SUBTLE() });
    resp.clicked()
}
