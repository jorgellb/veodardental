//! Botones de ficha completa del paciente y exportación por lotes.

use crate::app::App;
use crate::theme::*;
use egui::Ui;

fn write_and_open(ctx: &egui::Context, name: &str, content: &str, open: bool) -> Result<std::path::PathBuf, String> {
    let dir = crate::record::out_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    if open {
        ctx.open_url(egui::OpenUrl::new_tab(format!("file://{}", path.display())));
    }
    Ok(path)
}

/// Menú «Ficha» en la cabecera del paciente.
pub fn patient_menu(app: &mut App, ui: &mut Ui, pid: u32) {
    ui.menu_button("📄 Ficha", |ui| {
        let Some(p) = app.db.patient(pid).cloned() else { return };
        let base = format!("{:05}_{}", p.history_no, crate::record::safe_name(&p.full_name()));
        if ui.button("🖨 Ficha completa (imprimir o guardar en PDF)").clicked() {
            let html = crate::record::record_html(&app.db, pid, app.today);
            match write_and_open(ui.ctx(), &format!("{base}.html"), &html, true) {
                Ok(_) => {
                    app.db.audit("Ficha completa impresa", &p.full_name());
                    app.changed();
                    app.notify("Ficha abierta: use «Imprimir › Guardar como PDF» del navegador si la quiere en PDF.");
                }
                Err(e) => app.warn(format!("No se pudo crear la ficha: {e}")),
            }
            ui.close();
        }
        if ui.button("⬇ Exportar sus datos (derecho de acceso / portabilidad)").on_hover_text("Fichero con todos los datos del paciente en formato estándar para entregárselo").clicked() {
            let json = crate::record::patient_data_json(&app.db, pid, app.today);
            match write_and_open(ui.ctx(), &format!("{base}_datos.json"), &json, false) {
                Ok(path) => {
                    app.db.audit("Exportación de datos del paciente (RGPD)", &p.full_name());
                    app.changed();
                    app.notify(format!("Datos exportados en {}", path.display()));
                }
                Err(e) => app.warn(format!("No se pudo exportar: {e}")),
            }
            ui.close();
        }
    });
}

/// Configuración: exportar las fichas de todos los pacientes.
pub fn batch_panel(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Exportar fichas de pacientes");
        muted(ui, "Crea una carpeta con la ficha completa de cada paciente (datos, salud, odontograma, tratamientos, cuenta y citas), lista para imprimir o archivar en PDF. Contiene datos de salud: guárdela en un lugar protegido.");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let n_active = app.db.patients.iter().filter(|p| p.active).count();
            let mut run = None;
            if primary(ui, &format!("Exportar pacientes activos ({n_active})")).clicked() {
                run = Some(true);
            }
            if ui.button(format!("Exportar todos ({})", app.db.patients.len())).clicked() {
                run = Some(false);
            }
            if let Some(only_active) = run {
                match crate::record::export_all(&app.db, &crate::record::out_dir(), app.today, only_active) {
                    Ok((folder, n)) => {
                        app.db.audit("Exportación de fichas", &format!("{n} fichas en {}", folder.display()));
                        app.changed();
                        app.notify(format!("{n} fichas exportadas en {}", folder.display()));
                        ui.ctx().open_url(egui::OpenUrl::new_tab(format!("file://{}", folder.display())));
                    }
                    Err(e) => app.warn(format!("No se pudo exportar: {e}")),
                }
            }
            muted(ui, format!("Carpeta: {}", crate::record::out_dir().display()));
        });
    });
}
