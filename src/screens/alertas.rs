//! Alertas clínicas en la ficha (ver `crate::alertas`): el recuadro con cada
//! alerta, su fuente y el visto del profesional, y la comprobación antes de
//! dejar un tratamiento como hecho.

use crate::app::App;
use crate::model::OdgKind;
use crate::theme::*;
use egui::{RichText, Ui};

/// ¿Se puede dejar como hecho un tratamiento de esta clase? Si hay alertas
/// sin revisar hoy, avisa y no.
pub fn allow(app: &mut App, pid: u32, kind: OdgKind) -> bool {
    let block = app.db.alerts_blocking(pid, kind, crate::model::now());
    if block.is_empty() {
        return true;
    }
    let names: Vec<&str> = block.iter().map(|r| r.title).collect();
    app.warn(format!("Antes de dejarlo como hecho, revise las alertas clínicas de la ficha: {}.", names.join("; ")));
    false
}

/// La clase de tratamiento de un código.
pub fn kind_of(app: &App, code: &str) -> OdgKind {
    app.db.treatment(code).map(|t| t.odg).unwrap_or(OdgKind::Ninguno)
}

/// Recuadro de alertas del paciente (si tiene alguna): una línea por
/// alerta con su estado, y el detalle (qué comprobar y la fuente) al abrirlo.
pub fn card_for(app: &mut App, ui: &mut Ui, pid: u32) {
    let rules = app.db.patient_rules(pid);
    if rules.is_empty() || !app.can(crate::users::Perm::Clinico) {
        return;
    }
    let now = crate::model::now();
    let key = ui.make_persistent_id(("alertas", pid));
    let mut open: bool = ui.data_mut(|d| d.get_temp(key)).unwrap_or(false);
    egui::Frame::new().fill(tint(WARN, 0.07)).stroke(egui::Stroke::new(1.0, tint(WARN, 0.4))).corner_radius(10).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("⚠ Alertas clínicas").font(sbf(13.5)).color(HEADING));
            for r in &rules {
                ui.label(RichText::new(r.title).size(13.0));
                match app.db.alert_acked_today(pid, r.key, now) {
                    Some(a) => {
                        pill(ui, &format!("revisada por {} a las {}", a.by, a.at.format("%H:%M")), OK);
                    }
                    None => {
                        if ui.small_button("La he revisado").on_hover_text("Queda anotado quién y cuándo; vale para hoy").clicked() {
                            let who = app.db.config.current_user.clone();
                            if app.db.ack_alert(pid, r.key, &who, now).is_ok() {
                                app.changed();
                            }
                        }
                    }
                }
            }
            if ui.small_button(if open { "Ocultar detalles" } else { "Detalles y fuentes" }).clicked() {
                open = !open;
            }
        });
        if open {
            if !crate::alertas::REVIEWED {
                ui.label(RichText::new("Textos pendientes de revisión por un odontólogo de la clínica.").size(12.0).color(MUTED()));
            }
            for r in &rules {
                ui.add_space(4.0);
                ui.label(RichText::new(r.title).font(sbf(13.0)));
                ui.label(RichText::new(r.check).size(13.0));
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("Fuente: {}", r.source)).size(12.0).color(MUTED()));
                    ui.hyperlink_to(RichText::new("ver").size(12.0), r.url);
                });
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(key, open));
    ui.add_space(6.0);
}
