//! Campañas a grupos de pacientes en Comunicaciones (ver `crate::campanas`):
//! borrador con su segmento y sus límites, previsualización, programación
//! (Dirección) y resultados con el criterio de atribución a la vista.

use crate::app::App;
use crate::campanas::{CRITERION, CampState, Campaign, Preview};
use crate::comms::{Channel, MsgKind};
use crate::theme::*;
use crate::users::Perm;
use egui::{RichText, Ui};

#[derive(Default)]
pub struct CampUi {
    pub selected: Option<u32>,
    /// La previsualización y la campaña con la que se hizo: se rehace al
    /// cambiar la campaña o al pulsar «Actualizar».
    preview: Option<(Campaign, Preview)>,
    pub msg: Option<(bool, String)>,
    /// Último minuto en que se miró si tocaba pasar mensajes a la cola.
    last_tick: Option<chrono::NaiveDateTime>,
}

/// Una vez por minuto: pasa a la cola los mensajes de las campañas
/// programadas que tocan (horario y límite de cada una). No envía nada.
pub fn tick(app: &mut App) {
    let now = crate::model::now();
    let minute = now.date().and_hms_opt(chrono::Timelike::hour(&now), chrono::Timelike::minute(&now), 0);
    if app.comms.camp.last_tick == minute || !app.db.campaigns.iter().any(|c| c.state == CampState::Programada) {
        return;
    }
    app.comms.camp.last_tick = minute;
    if app.db.campaign_tick(now) > 0 {
        app.changed();
    }
}

pub fn tab(app: &mut App, ui: &mut Ui) {
    muted(
        ui,
        "Promociones a grupos de pacientes. Solo les llegan a quienes han aceptado recibir promociones (Filiación › Protección de datos y comunicaciones) y no se han dado de baja de ellas. Los mensajes pasan a la cola de «Mensajes» dentro del horario y el límite de cada campaña; desde allí se envían como los demás.",
    );
    ui.add_space(6.0);
    if ui.button("➕ Nueva campaña").clicked() {
        let id = app.db.new_id();
        app.db.campaigns.push(Campaign::new(id, "Nueva campaña", Channel::WhatsApp, "Hola [NOMBRE], "));
        app.comms.camp.selected = Some(id);
        app.changed();
    }
    ui.add_space(6.0);
    if app.db.campaigns.is_empty() {
        muted(ui, "Todavía no hay campañas.");
        return;
    }
    egui::Grid::new("campaigns").striped(true).spacing([18.0, 6.0]).show(ui, |ui| {
        for h in ["Campaña", "Canal", "Estado", "Destinatarios"] {
            ui.label(RichText::new(h).size(12.5).color(MUTED));
        }
        ui.end_row();
        for c in app.db.campaigns.iter().rev() {
            if ui.selectable_label(app.comms.camp.selected == Some(c.id), &c.name).clicked() {
                app.comms.camp.selected = Some(c.id);
                app.comms.camp.msg = None;
            }
            ui.label(c.channel.label());
            ui.label(c.state.label());
            ui.label(if c.recipients.is_empty() { "—".to_string() } else { c.recipients.len().to_string() });
            ui.end_row();
        }
    });
    ui.add_space(8.0);
    if let Some((ok, m)) = &app.comms.camp.msg {
        ui.label(RichText::new(m).color(if *ok { OK } else { WARN }));
    }
    let Some(c) = app.comms.camp.selected.and_then(|id| app.db.campaigns.iter().find(|c| c.id == id)).cloned() else { return };
    if c.state == CampState::Borrador { draft(app, ui, c) } else { summary(app, ui, c) }
}

fn number(ui: &mut Ui, label: &str, v: &mut u32, range: std::ops::RangeInclusive<u32>) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(v).range(range));
    });
}

fn optional(ui: &mut Ui, label: &str, v: &mut Option<u32>, default: u32, range: std::ops::RangeInclusive<u32>) {
    ui.horizontal(|ui| {
        let mut on = v.is_some();
        if ui.checkbox(&mut on, label).changed() {
            *v = on.then_some(default);
        }
        if let Some(x) = v.as_mut() {
            ui.add(egui::DragValue::new(x).range(range));
        }
    });
}

fn draft(app: &mut App, ui: &mut Ui, before: Campaign) {
    let mut c = before.clone();
    let clinical = app.can(Perm::Clinico);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Borrador");
        ui.horizontal(|ui| {
            ui.label("Nombre");
            ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(320.0));
            ui.add_space(12.0);
            ui.label("Canal");
            crate::theme::combo("camp_channel").width(120.0).selected_text(c.channel.label()).show_ui(ui, |ui| {
                for ch in Channel::ALL {
                    ui.selectable_value(&mut c.channel, ch, ch.label());
                }
            });
        });
        if c.channel == Channel::Email {
            ui.horizontal(|ui| {
                ui.label("Asunto");
                ui.add(egui::TextEdit::singleline(&mut c.subject).hint_text("vacío: el nombre de la clínica").desired_width(420.0));
            });
        }
        ui.label("Mensaje");
        ui.add(egui::TextEdit::multiline(&mut c.text).desired_rows(4).desired_width(f32::INFINITY));
        muted(ui, format!("Comodines: {}. Al final se añade siempre cómo darse de baja.", crate::comms::msg_placeholders(MsgKind::Campana).join(" ")));
        ui.add_space(6.0);
        section(ui, "A quién");
        optional(ui, "Edad desde", &mut c.segment.min_age, 18, 0..=120);
        optional(ui, "Edad hasta", &mut c.segment.max_age, 65, 0..=120);
        optional(ui, "Última visita hace al menos (días)", &mut c.segment.last_visit_days, 180, 1..=3650);
        ui.checkbox(&mut c.segment.pending_budget, "Con un presupuesto pendiente");
        if clinical {
            ui.horizontal(|ui| {
                ui.label("Con el tratamiento hecho (código)");
                ui.add(egui::TextEdit::singleline(&mut c.segment.with_code).desired_width(90.0));
                ui.add_space(12.0);
                ui.label("Sin él en el último año (código)");
                ui.add(egui::TextEdit::singleline(&mut c.segment.without_code).desired_width(90.0));
            });
            muted(ui, "Elegir por tratamiento usa datos de salud: el texto del mensaje no los menciona.");
        } else if c.segment.uses_treatments() {
            muted(ui, format!("Por tratamiento (lo ajusta un perfil clínico): con «{}», sin «{}» en el último año.", c.segment.with_code, c.segment.without_code));
        }
        ui.add_space(6.0);
        section(ui, "Límites");
        number(ui, "Sin otra campaña al mismo paciente en los días anteriores", &mut c.min_days, 0..=365);
        ui.horizontal(|ui| {
            ui.label("De lunes a viernes, de");
            ui.add(egui::DragValue::new(&mut c.from_hour).range(8..=21).suffix(" h"));
            ui.label("a");
            ui.add(egui::DragValue::new(&mut c.to_hour).range(9..=22).suffix(" h"));
        });
        number(ui, "Como mucho, mensajes al día", &mut c.per_day, 1..=5000);
        number(ui, "Plazo de atribución de citas (días)", &mut c.attribution_days, 1..=365);
    });
    if c != before {
        if let Some(x) = app.db.campaigns.iter_mut().find(|x| x.id == c.id) {
            *x = c.clone();
        }
        app.changed();
    }
    // previsualización (se rehace al cambiar la campaña)
    let now = crate::model::now();
    if app.comms.camp.preview.as_ref().is_none_or(|(k, _)| *k != c) {
        app.comms.camp.preview = Some((c.clone(), app.db.campaign_preview(&c, now)));
    }
    let prev = app.comms.camp.preview.as_ref().map(|(_, p)| p.clone()).unwrap_or_default();
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Previsualización");
        ui.label(RichText::new(format!("La recibirán {} pacientes.", prev.recipients.len())).font(sbf(14.0)));
        for (why, n) in &prev.excluded {
            muted(ui, format!("Se quedan fuera {n}: {}.", why.label()));
        }
        for (pid, text) in &prev.samples {
            ui.add_space(4.0);
            ui.label(RichText::new(app.db.patient(*pid).map(|p| p.full_name()).unwrap_or_default()).size(12.5).color(MUTED));
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(text);
            });
        }
        ui.add_space(6.0);
        if let Some(p) = c.problem() {
            ui.label(RichText::new(p).color(WARN));
        }
        ui.horizontal(|ui| {
            if ui.button("Actualizar").clicked() {
                app.comms.camp.preview = None;
            }
            if app.can(Perm::Configuracion) {
                if primary(ui, "Programar campaña").clicked() {
                    app.comms.camp.msg = Some(match app.db.campaign_schedule(c.id, now) {
                        Ok(n) => (true, format!("Programada para {n} pacientes. Los mensajes irán pasando a la cola de «Mensajes» en su horario.")),
                        Err(e) => (false, e),
                    });
                    app.changed();
                }
            } else {
                muted(ui, "La programa Dirección.");
            }
            if danger(ui, "Borrar borrador").clicked() {
                app.db.campaigns.retain(|x| x.id != c.id);
                app.comms.camp.selected = None;
                app.changed();
            }
        });
    });
}

fn summary(app: &mut App, ui: &mut Ui, c: Campaign) {
    let r = app.db.campaign_results(c.id);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, &format!("{} · {}", c.name, c.state.label()));
        muted(ui, format!("{} · programada por {} · de lunes a viernes de {} a {} h, como mucho {} al día", c.channel.label(), if c.created_by.is_empty() { "—" } else { &c.created_by }, c.from_hour, c.to_hour, c.per_day));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(&c.text);
        });
        ui.add_space(6.0);
        section(ui, "Resultados");
        ui.label(format!("Pasados a la cola: {} · por pasar: {} · ya no podían recibirla: {}", r.queued, r.pending, r.skipped));
        ui.label(RichText::new(format!("Citas atribuidas: {} (acudió a {}) en {} días", r.appts, r.attended, c.attribution_days)).font(sbf(14.0)));
        muted(ui, CRITERION);
        if c.state == CampState::Programada {
            ui.add_space(6.0);
            if app.can(Perm::Configuracion) {
                if danger(ui, "Cancelar campaña").clicked() {
                    app.comms.camp.msg = Some(match app.db.campaign_cancel(c.id) {
                        Ok(n) => (true, format!("Cancelada. Se han descartado {n} mensajes que estaban en la cola sin enviar.")),
                        Err(e) => (false, e),
                    });
                    app.changed();
                }
            } else {
                muted(ui, "La cancela Dirección.");
            }
        }
    });
}
