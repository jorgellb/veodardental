//! Derechos de protección de datos del paciente, en su ficha (ver `crate::rgpd`).

use crate::app::App;
use crate::rgpd::{Capacity, RightKind};
use crate::theme::*;
use egui::{RichText, Ui};

pub struct RightsForm {
    pub kind: RightKind,
    pub requester: String,
    pub capacity: Capacity,
    pub proof: String,
    pub reason: String,
    pub answer: String,
}

impl Default for RightsForm {
    fn default() -> Self {
        RightsForm { kind: RightKind::Acceso, requester: String::new(), capacity: Capacity::Paciente, proof: String::new(), reason: String::new(), answer: String::new() }
    }
}

pub fn panel(app: &mut App, ui: &mut Ui, pid: u32) {
    let admin = app.can(crate::users::Perm::Configuracion);
    let Some(p) = app.db.patient(pid).cloned() else { return };
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Protección de datos");
        if let Some(m) = &p.restricted {
            ui.label(RichText::new(format!("Tratamiento limitado desde el {} ({}): no se le envían mensajes.", m.since.format("%d/%m/%Y"), m.reason)).color(WARN));
        }
        if let Some(m) = &p.blocked {
            ui.label(RichText::new(format!("Datos bloqueados desde el {} ({}): fuera de las listas; solo Dirección los desbloquea.", m.since.format("%d/%m/%Y"), m.reason)).color(DANGER));
        }
        ui.horizontal_wrapped(|ui| {
            let w = (fit_width(ui) - 420.0).clamp(160.0, 360.0);
            ui.add_sized([w, 32.0], egui::TextEdit::singleline(&mut app.rights.reason).hint_text("Motivo (para limitar o bloquear)"));
            let reason = app.rights.reason.trim().to_string();
            let r = if p.restricted.is_some() {
                ui.button("Quitar la limitación").clicked().then(|| app.db.set_restriction(pid, None))
            } else {
                ui.add_enabled(!reason.is_empty(), egui::Button::new("Limitar el tratamiento")).clicked().then(|| app.db.set_restriction(pid, Some(&reason)))
            };
            let b = if p.blocked.is_some() {
                ui.add_enabled(admin, egui::Button::new("Desbloquear")).clicked().then(|| app.db.set_block(pid, None, admin))
            } else {
                ui.add_enabled(!reason.is_empty(), egui::Button::new("Bloquear los datos")).clicked().then(|| app.db.set_block(pid, Some(&reason), admin))
            };
            for res in [r, b].into_iter().flatten() {
                match res {
                    Ok(()) => {
                        app.rights.reason.clear();
                        app.changed();
                    }
                    Err(e) => app.warn(e),
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("⬇ Exportar todo (ZIP con ficheros y manifiesto)").clicked() {
                let dest = crate::record::out_dir().join(format!("{:05}_{}_derechos_{}.zip", p.history_no, crate::record::safe_name(&p.full_name()), app.today.format("%Y%m%d")));
                let who = app.db.config.current_user.clone();
                match crate::rgpd::export_zip(&app.db, pid, &dest, &who) {
                    Ok(n) => {
                        app.db.audit("Exportación íntegra de datos (RGPD)", &format!("historia nº {}", p.history_no));
                        app.changed();
                        app.notify(format!("{n} ficheros exportados en {}", dest.display()));
                    }
                    Err(e) => app.warn(format!("No se pudo exportar: {e}")),
                }
            }
            let problems = app.db.suppression_problems(pid, app.today);
            ui.add_enabled(problems.is_empty(), egui::Button::new("Suprimir los datos")).on_disabled_hover_text(problems.join("\n"));
        });
        // solicitudes de derechos
        ui.add_space(6.0);
        ui.label(RichText::new("Solicitudes de derechos").family(sb()));
        let late = app.db.overdue_requests(app.today).iter().filter(|r| r.patient_id == pid).count();
        if late > 0 {
            ui.label(RichText::new(format!("{late} sin contestar y fuera de plazo (un mes desde que se recibieron).")).color(DANGER));
        }
        let list: Vec<crate::rgpd::RightsRequest> = app.db.rights_requests.iter().filter(|r| r.patient_id == pid).cloned().collect();
        for r in &list {
            ui.horizontal_wrapped(|ui| {
                let late = r.resolved.is_none() && r.due < app.today;
                ui.label(RichText::new(format!("{} · {} · {} ({}) · plazo {}", r.received.format("%d/%m/%Y"), r.kind.label(), r.requester, r.capacity.label(), r.due.format("%d/%m/%Y"))).color(if late { DANGER } else { TEXT }));
                match &r.resolved {
                    Some((d, a)) => {
                        ui.label(RichText::new(format!("contestada el {}: {a}", d.format("%d/%m/%Y"))).color(OK));
                    }
                    None => {
                        if ui.small_button("Marcar contestada").clicked() {
                            let answer = if app.rights.answer.trim().is_empty() { "Contestada".to_string() } else { app.rights.answer.trim().to_string() };
                            if app.db.resolve_request(r.id, &answer, app.today).is_ok() {
                                app.rights.answer.clear();
                                app.changed();
                            }
                        }
                    }
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            crate::theme::combo("rgpd_kind").width(200.0).selected_text(app.rights.kind.label()).show_ui(ui, |ui| {
                for k in RightKind::ALL {
                    ui.selectable_value(&mut app.rights.kind, k, k.label());
                }
            });
            crate::theme::combo("rgpd_cap").width(260.0).selected_text(app.rights.capacity.label()).show_ui(ui, |ui| {
                for c in [Capacity::Paciente, Capacity::RepresentanteLegal, Capacity::Autorizado] {
                    ui.selectable_value(&mut app.rights.capacity, c, c.label());
                }
            });
            ui.add_sized([200.0, 32.0], egui::TextEdit::singleline(&mut app.rights.requester).hint_text("Quién lo pide"));
            ui.add_sized([220.0, 32.0], egui::TextEdit::singleline(&mut app.rights.proof).hint_text("Con qué lo acredita"));
            if ui.button("Registrar solicitud").clicked() {
                let f = &app.rights;
                match app.db.register_request(pid, f.kind, &f.requester.clone(), f.capacity, &f.proof.clone(), app.today) {
                    Ok(_) => {
                        app.rights = Default::default();
                        app.changed();
                        app.notify("Solicitud registrada: hay un mes para contestarla.");
                    }
                    Err(e) => app.warn(e),
                }
            }
        });
        ui.add_sized([fit_width(ui).min(520.0), 32.0], egui::TextEdit::singleline(&mut app.rights.answer).hint_text("Respuesta dada (al marcar una como contestada)"));
    });
}
