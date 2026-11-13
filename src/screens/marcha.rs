//! Configuración › Puesta en marcha: lo que falta para trabajar con la
//! clínica real, comprobado con la configuración, y dónde se arregla.

use crate::app::{App, ConfirmAction, Dialog, Screen};
use crate::golive::{self, Check, Facts, Goto, Status};
use crate::theme::*;
use egui::{CornerRadius, RichText, Sense, Stroke, Ui, vec2};

#[derive(Default)]
pub struct MarchaUi {
    /// Lo leído del disco de este ordenador y cuándo.
    facts: Option<(f64, Facts)>,
}

/// Lo que se sabe de este ordenador; se vuelve a leer cada 5 segundos.
fn facts(app: &mut App, time: f64) -> Facts {
    if let Some((t, f)) = &app.marcha.facts
        && time >= *t
        && time - *t < 5.0
    {
        return f.clone();
    }
    let remote = crate::shared::remote_mode();
    let extra = app.db.config.backup_extra.trim().to_string();
    let f = Facts {
        now: crate::model::now(),
        last_backup: if remote { None } else { crate::tools::list_backups(&crate::tools::backup_dir()).iter().map(|b| b.when).max() },
        extra_found: !extra.is_empty() && std::path::Path::new(&extra).is_dir(),
        backup_key: crate::cifrado::current_key().is_some(),
        remote,
        network: remote || crate::shared::shared() || app.db.stations.len() > 1,
        cert: app.vf.has_cert(),
        whatsapp: app.wa.saved.enabled,
        disk: crate::disco::status(),
    };
    app.marcha.facts = Some((time, f.clone()));
    f
}

/// Pasos imprescindibles que faltan (para el aviso de Inicio).
pub fn pending(app: &mut App, time: f64) -> usize {
    let f = facts(app, time);
    golive::pending(&golive::checks(&app.db, &f))
}

/// Vacía la clínica tras guardar una copia. Devuelve si se ha hecho.
pub fn start_empty(app: &mut App) -> bool {
    if app.persist
        && let Err(e) = crate::tools::create_backup(&app.db, "antes de vaciar la clinica")
    {
        app.warn(format!("No se pudo guardar la copia de seguridad ({e}); no se ha borrado nada."));
        return false;
    }
    match app.db.start_empty() {
        Ok(n) => {
            app.patient = None;
            app.marcha.facts = None;
            app.notify(format!("Clínica vacía: se quitaron {n} pacientes y los datos de ejemplo.{}", if app.persist { " Antes se guardó una copia de seguridad." } else { "" }));
            true
        }
        Err(e) => {
            app.warn(e);
            false
        }
    }
}

fn confirm_text(app: &App) -> String {
    let real: Vec<String> = app.db.patients.iter().filter(|p| !golive::is_demo_patient(p)).map(|p| p.full_name()).collect();
    let mut t = format!(
        "Se quitarán los {} pacientes con sus citas, historial, presupuestos, facturas y cobros, y los proveedores, productos y movimientos de ejemplo. Se conservan la configuración, los tratamientos y sus precios, los profesionales, los gabinetes y los usuarios.",
        app.db.patients.len()
    );
    if !real.is_empty() {
        let names: Vec<String> = real.iter().take(3).cloned().collect();
        t.push_str(&format!(" Atención: {} no son de ejemplo ({}{}).", real.len(), names.join(", "), if real.len() > 3 { "…" } else { "" }));
    }
    if app.persist {
        t.push_str(" Antes se guarda una copia de seguridad, por si quiere volver atrás.");
    }
    t
}

pub fn panel(app: &mut App, ui: &mut Ui) {
    let time = ui.input(|i| i.time);
    let f = facts(app, time);
    let list = golive::checks(&app.db, &f);
    let essential: Vec<&Check> = list.iter().filter(|c| c.essential).collect();
    let done = essential.iter().filter(|c| c.status == Status::Listo).count();
    let mut go = None;

    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let left = essential.len() - done;
        let (head, col) = if left == 0 { ("Todo listo para trabajar con la clínica real".to_string(), OK) } else { (format!("Faltan {left} de {} pasos imprescindibles", essential.len()), WARN) };
        ui.label(RichText::new(head).font(bf(19.0)).color(HEADING));
        ui.add_space(6.0);
        let w = ui.available_width().min(560.0);
        bar(ui, w, done as f32 / essential.len().max(1) as f32, col);
        ui.add_space(6.0);
        muted(ui, "Cada paso se comprueba con la configuración del programa, así que esta lista está siempre al día. Lo opcional puede activarlo cuando lo necesite.");
    });
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Imprescindible");
        for c in list.iter().filter(|c| c.essential) {
            if let Some(g) = row(ui, c) {
                go = Some(g);
            }
        }
    });
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Cuando lo necesite");
        for c in list.iter().filter(|c| !c.essential) {
            if let Some(g) = row(ui, c) {
                go = Some(g);
            }
        }
    });

    match go {
        Some(Goto::Cfg(t)) => crate::screens::config::open(app, t),
        Some(Goto::Colaboradores) => app.screen = Screen::Colaboradores,
        Some(Goto::Vaciar) => app.dialog = Dialog::Confirm { text: confirm_text(app), action: ConfirmAction::StartEmpty },
        Some(Goto::Marcar(key)) => {
            if let Some(i) = app.db.golive.iter().position(|t| t.key == key) {
                app.db.golive.remove(i);
            } else {
                let user = app.db.config.current_user.clone();
                app.db.golive.push(golive::Tick { key: key.into(), when: app.today, user });
            }
            app.changed();
        }
        Some(Goto::Nada) | None => {}
    }
}

/// Una fila: marca de estado, título con su etiqueta, explicación y botón
/// (cada una con su propio identificador, por su clave).
fn row(ui: &mut Ui, c: &Check) -> Option<Goto> {
    ui.push_id(c.key, |ui| row_body(ui, c)).inner
}

fn row_body(ui: &mut Ui, c: &Check) -> Option<Goto> {
    let col = match c.status {
        Status::Listo => OK,
        Status::Pendiente => WARN,
        Status::Opcional => SUBTLE,
    };
    let mut go = None;
    ui.add_space(4.0);
    egui::Frame::new().fill(ROW_ALT).stroke(Stroke::new(1.0, HAIRLINE)).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::symmetric(14, 11)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
            let p = ui.painter();
            let m = r.center();
            p.circle_filled(m, 14.0, tint(col, 0.14));
            match c.status {
                Status::Listo => {
                    p.line_segment([m + vec2(-5.5, 0.5), m + vec2(-1.5, 4.5)], Stroke::new(2.2, col));
                    p.line_segment([m + vec2(-1.5, 4.5), m + vec2(6.0, -4.5)], Stroke::new(2.2, col));
                }
                Status::Pendiente => {
                    p.line_segment([m + vec2(0.0, -6.0), m + vec2(0.0, 2.0)], Stroke::new(2.4, col));
                    p.circle_filled(m + vec2(0.0, 6.0), 1.5, col);
                }
                Status::Opcional => {
                    p.circle_stroke(m, 5.0, Stroke::new(1.8, col));
                }
            }
            ui.add_space(4.0);
            // sitio para el botón según lo que dice, y el resto para el texto
            let button_w = match c.goto {
                Goto::Vaciar => 250.0,
                Goto::Marcar(_) => 170.0,
                Goto::Nada => 0.0,
                _ => 140.0,
            };
            let text_w = (ui.available_width() - button_w - 10.0).max(220.0);
            let text = ui
                .vertical(|ui| {
                    ui.set_width(text_w);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(c.title).font(sbf(15.0)).color(HEADING));
                        pill(ui, c.status.label(), col);
                    });
                    ui.label(RichText::new(&c.detail).font(mdf(13.5)).color(MUTED));
                })
                .response;
            if button_w == 0.0 {
                return;
            }
            // el botón a la derecha, centrado con el texto
            let h = text.rect.height().max(30.0);
            ui.allocate_ui_with_layout(vec2(ui.available_width(), h), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let clicked = match c.goto {
                    Goto::Vaciar => danger(ui, "Empezar con la clínica vacía…").clicked(),
                    Goto::Marcar(_) if c.status == Status::Listo => secondary(ui, "Desmarcar").clicked(),
                    Goto::Marcar(_) => primary(ui, "Marcar como hecho").clicked(),
                    Goto::Nada => false,
                    _ if c.status == Status::Listo => secondary(ui, "Revisar  ›").clicked(),
                    // lo opcional, con un botón más discreto que lo imprescindible
                    _ if c.status == Status::Opcional => secondary(ui, "Configurar  ›").clicked(),
                    _ => primary(ui, "Configurar  ›").clicked(),
                };
                if clicked {
                    go = Some(c.goto);
                }
            });
        });
    });
    go
}
