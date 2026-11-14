//! Ficha › Filiación › «Mi espacio» (ver `crate::portal_paciente`): dar el
//! acceso al paciente o a su responsable, copiar el enlace y revocarlo.

use crate::app::App;
use crate::portal_paciente::Holder;
use crate::theme::*;
use egui::{RichText, Ui};

pub fn card_for(app: &mut App, ui: &mut Ui, pid: u32) {
    if !app.db.booking.enabled {
        return;
    }
    let Some(p) = app.db.patient(pid).cloned() else { return };
    let nif: String = p.payer.nif.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
    let holders: Vec<(Holder, String)> = std::iter::once((Holder::Patient(pid), "el propio paciente".to_string())).chain((!nif.is_empty()).then(|| (Holder::Responsible(nif.clone()), format!("su responsable ({})", p.payer.name.trim())))).collect();
    let base = app.db.booking.public_url.trim().trim_end_matches('/').to_string();
    let now = crate::model::now();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Mi espacio (portal del paciente)");
        muted(ui, "Citas, presupuestos, facturas, documentos firmados y recetas desde el móvil. Se entra con el enlace y con el DNI/NIE y la fecha de nacimiento.");
        for (h, label) in holders {
            let current = app.db.portal_access.iter().find(|a| a.holder == h && a.revoked.is_none()).cloned();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("Acceso de {label}")).font(sbf(13.0)));
                match current {
                    Some(a) => {
                        pill(ui, &format!("activo desde el {}", a.created.format("%d/%m/%Y")), OK);
                        if ui.small_button("Copiar enlace").clicked() {
                            ui.ctx().copy_text(format!("{base}/portal/{}", a.token));
                            app.notify("Enlace de Mi espacio copiado.");
                        }
                        if ui.small_button("Revocar").on_hover_text("El enlace y sus sesiones dejan de valer").clicked() {
                            app.db.portal_revoke(a.id, now);
                            app.changed();
                            app.notify("Acceso revocado.");
                        }
                    }
                    None => {
                        if ui.small_button("Dar acceso").clicked() {
                            match app.db.portal_grant(h.clone(), now) {
                                Ok(id) => {
                                    if let Some(a) = app.db.portal_access.iter().find(|a| a.id == id) {
                                        ui.ctx().copy_text(format!("{base}/portal/{}", a.token));
                                    }
                                    app.changed();
                                    app.notify("Acceso dado: el enlace está copiado para enviárselo.");
                                }
                                Err(e) => app.warn(e),
                            }
                        }
                    }
                }
            });
        }
    });
    ui.add_space(8.0);
}
