//! Libro de recalls: revisiones periódicas a las que hay que llamar.

use crate::app::{App, Dialog, PatTab};
use crate::model::*;
use crate::screens::agenda::ApptDraft;
use crate::theme::*;
use egui::{Color32, RichText, Ui};

pub fn state_color(s: RecallState) -> Color32 {
    match s {
        RecallState::Pendiente => WARN,
        RecallState::Contactado => INFO,
        RecallState::Citado => OK,
        RecallState::Rechazado => MUTED,
    }
}

/// Cita para atender un recall: con quien hizo el tratamiento (si sigue en la
/// clínica; si no, el profesional del paciente) y, si es de higiene, con el
/// tratamiento de higiene del catálogo y su duración, en el primer hueco del día.
pub fn appointment_for(app: &App, r: &Recall) -> ApptDraft {
    let date = app.today.max(r.date);
    let doctor = if app.db.collaborators.iter().any(|c| c.id == r.doctor_id && c.active) { r.doctor_id } else { app.db.patient(r.patient_id).map(|p| p.doctor_id).unwrap_or(r.doctor_id) };
    let hygiene = if fold(&r.reason).contains("higiene") { app.db.treatments.iter().find(|t| t.active && t.odg == OdgKind::Higiene && t.group != "Periodoncia") } else { None };
    let minutes = hygiene.map(|t| t.minutes).filter(|m| *m > 0).unwrap_or(30);
    let at = app.db.find_gaps(doctor, date, minutes, app.db.config.open_time, app.db.config.close_time, 1).into_iter().find(|g| g.0 == date).map(|g| (g.1, doctor));
    let mut d = ApptDraft::new_at(app, date, at, Some(r.patient_id));
    d.a.doctor_id = doctor;
    d.a.reason = r.reason.clone();
    if let Some(t) = hygiene {
        d.a.treatment_code = t.code.clone();
        d.a.minutes = minutes;
    }
    d
}

pub fn show(app: &mut App, ui: &mut Ui) {
    // los de pacientes de baja no se cuentan ni se listan (vuelven si se reactiva)
    let pending: Vec<&Recall> = app.db.recalls.iter().filter(|r| app.db.recall_due(r)).collect();
    let overdue = pending.iter().filter(|r| r.date < app.today).count();
    let week = pending.iter().filter(|r| r.date >= app.today && r.date <= app.today + chrono::Duration::days(7)).count();
    title(ui, "Recalls", "Pacientes a los que hay que llamar para su próxima revisión");
    let w = kpi_w(ui, 3);
    ui.horizontal(|ui| {
        kpi(ui, w, "Pendientes", &pending.len().to_string(), "en total", WARN);
        ui.add_space(4.0);
        kpi(ui, w, "Vencidos", &overdue.to_string(), "fecha ya pasada", DANGER);
        ui.add_space(4.0);
        kpi(ui, w, "Esta semana", &week.to_string(), "próximos 7 días", INFO);
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.horizontal(|ui| {
            for (f, n) in [(Some(RecallState::Pendiente), "Pendientes"), (Some(RecallState::Contactado), "Contactados"), (Some(RecallState::Citado), "Citados"), (Some(RecallState::Rechazado), "Rechazados"), (None, "Todos")] {
                if ui.selectable_label(app.lists.recall_filter == f, n).clicked() {
                    app.lists.recall_filter = f;
                }
            }
            ui.separator();
            ui.checkbox(&mut app.lists.recall_until_today, "Solo vencidos hasta hoy");
        });
        ui.add_space(6.0);
        let tbl = Table::new("recalls", vec![("Fecha", 96.0), ("Paciente", 210.0), ("Móvil", 120.0), ("Motivo", 190.0), ("Doctor", 110.0), ("Estado", 100.0), ("Canal", 90.0), ("Comentario", 220.0)]);
        tbl.header(ui);
        let mut list: Vec<Recall> =
            app.db.recalls.iter().filter(|r| app.lists.recall_filter.map(|f| r.state == f && (f != RecallState::Pendiente || app.db.recall_due(r))).unwrap_or(true)).filter(|r| !app.lists.recall_until_today || r.date <= app.today).cloned().collect();
        list.sort_by_key(|r| r.date);
        if list.is_empty() {
            empty_state(ui, "No hay recalls con este filtro");
        }
        let mut action: Option<(u32, &str)> = None;
        egui::ScrollArea::vertical().id_salt("rcl").auto_shrink([false, false]).show(ui, |ui| {
            for (i, r) in list.iter().enumerate() {
                let p = app.db.patient(r.patient_id).cloned().unwrap_or_default();
                let cells = [
                    cellc(fmt_date(r.date), if r.date < app.today && r.state == RecallState::Pendiente { DANGER } else { TEXT }),
                    cellc(if p.active { p.full_name() } else { format!("{} (de baja)", p.full_name()) }, if p.active { TEXT } else { MUTED }),
                    cell(&p.mobile),
                    cell(&r.reason),
                    cell(app.db.doctor_alias(r.doctor_id)),
                    cellc(r.state.label(), state_color(r.state)),
                    cell(&r.channel),
                    cell(&r.comment),
                ];
                let resp = tbl.row(ui, i, false, &cells);
                resp.context_menu(|ui| {
                    for (k, label) in [("tel", "📞 Llamado por teléfono"), ("sms", "✉ Enviado SMS"), ("wa", "💬 Enviado WhatsApp"), ("cita", "📅 Dar cita"), ("no", "✖ No desea cita"), ("ficha", "👤 Abrir ficha")] {
                        if ui.button(label).clicked() {
                            action = Some((r.id, k));
                            ui.close();
                        }
                    }
                });
                if resp.clicked() {
                    action = Some((r.id, "cita"));
                }
            }
        });
        muted(ui, "Clic para dar cita · clic derecho para registrar el contacto.");
        if let Some((id, k)) = action {
            let r = app.db.recalls.iter().find(|r| r.id == id).cloned().unwrap();
            match k {
                "cita" => {
                    app.dialog = Dialog::Appointment(appointment_for(app, &r));
                }
                "ficha" => app.open_patient(r.patient_id, PatTab::Citas),
                _ => {
                    if let Some(x) = app.db.recalls.iter_mut().find(|x| x.id == id) {
                        x.contacted_on = Some(app.today);
                        match k {
                            "no" => {
                                x.state = RecallState::Rechazado;
                                x.comment = "No desea cita".into();
                            }
                            _ => {
                                x.state = RecallState::Contactado;
                                x.channel = match k {
                                    "tel" => "Teléfono",
                                    "sms" => "SMS",
                                    _ => "WhatsApp",
                                }
                                .into();
                                x.comment = format!("Contactado el {}", fmt_date(app.today));
                            }
                        }
                    }
                    app.changed();
                }
            }
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Los recalls se generan solos al realizar higienes, revisiones, implantes, periodoncia u ortodoncia; al dar cita se marcan como «Citado».").size(13.5).color(MUTED));
}
