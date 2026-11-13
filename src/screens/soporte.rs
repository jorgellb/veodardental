//! Errores del programa e informe para soporte (ver `crate::registro`): se
//! prepara a mano, se ve entero y solo se copia, se guarda o se envía con
//! una acción de quien lo ha revisado.

use crate::app::App;
use crate::theme::*;
use egui::{RichText, Ui};

#[derive(Default)]
pub struct SupportState {
    pub report: Option<String>,
    pub consent: bool,
    pub status: String,
}

pub fn panel(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Errores del programa e informe para soporte");
        muted(
            ui,
            "Los errores se apuntan en este ordenador sin datos de pacientes: se quitan nombres, documentos, teléfonos, correos, cuentas y rutas. Nada se envía solo: el informe se prepara aquí, se ve entero y solo sale si usted lo copia, lo guarda o lo envía.",
        );
        ui.add_space(6.0);
        if primary(ui, "Preparar informe para soporte").clicked() {
            app.support.report = Some(crate::registro::support_report(&app.db, &crate::registro::dir()));
            app.support.consent = false;
            app.support.status.clear();
        }
        let Some(report) = app.support.report.clone() else { return };
        ui.add_space(6.0);
        ui.label(RichText::new("Esto es todo lo que lleva:").color(MUTED));
        egui::ScrollArea::vertical().id_salt("soporte_prev").max_height(260.0).show(ui, |ui| {
            let mut text = report.clone();
            ui.add(egui::TextEdit::multiline(&mut text).interactive(false).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("📋 Copiar").clicked() {
                ui.ctx().copy_text(report.clone());
                app.support.status = "Copiado: péguelo en el correo o el mensaje a soporte.".into();
            }
            if ui.button("💾 Guardar en Documentos").clicked() {
                let p = crate::paths::documents().join(format!("informe_soporte_{}.txt", crate::model::now().format("%Y%m%d_%H%M")));
                app.support.status = match std::fs::create_dir_all(crate::paths::documents()).map_err(|e| e.to_string()).and_then(|_| crate::paths::write_private(&p, report.as_bytes()).map_err(|e| e.to_string())) {
                    Ok(()) => format!("Guardado en {}.", p.display()),
                    Err(e) => format!("No se pudo guardar: {e}"),
                };
            }
        });
        ui.horizontal_wrapped(|ui| {
            let mut to = app.db.config.support_email.clone();
            if field(ui, "Correo de soporte", &mut to, 260.0).changed() {
                app.db.config.support_email = to;
                app.changed();
            }
        });
        ui.checkbox(&mut app.support.consent, "He revisado el informe y acepto enviarlo a esa dirección");
        let can = app.support.consent && app.db.config.support_email.contains('@');
        if ui.add_enabled(can, egui::Button::new("✉ Enviar por correo")).clicked() {
            app.support.status = match crate::registro::send_support(&app.db.mail, &app.db.config.support_email, &report, app.support.consent) {
                Ok(()) => "Enviado a soporte.".into(),
                Err(e) => format!("No se ha enviado: {e}"),
            };
        }
        if !app.support.status.is_empty() {
            ui.label(RichText::new(&app.support.status).color(INFO));
        }
    });
}
