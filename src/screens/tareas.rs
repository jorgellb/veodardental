//! Ventana de tareas del equipo, campana de la barra superior y tarjeta de Inicio.

use crate::app::{App, PatTab};
use crate::model::*;
use crate::tasks::Task;
use crate::theme::*;
use egui::{Color32, RichText, Ui};

pub struct TaskState {
    pub open: bool,
    pub text: String,
    pub to: String,
    pub with_due: bool,
    pub due: chrono::NaiveDate,
    pub link_patient: bool,
    pub all_users: bool,
    pub show_done: bool,
}

impl TaskState {
    pub fn new(today: chrono::NaiveDate) -> Self {
        Self { open: false, text: String::new(), to: String::new(), with_due: true, due: today, link_patient: false, all_users: false, show_done: false }
    }
}

fn user_name(app: &App) -> String {
    app.user_id.and_then(|u| app.db.user(u)).map(|u| u.name.clone()).unwrap_or_else(|| app.db.config.current_user.clone())
}

/// Botón de la barra superior con el número de tareas pendientes.
pub fn bell(app: &mut App, ui: &mut Ui) {
    let me = user_name(app);
    let pend = app.db.pending_tasks(&me);
    let n = pend.len();
    let late = pend.iter().any(|t| t.overdue(app.today));
    let txt = if n == 0 { "🔔".to_string() } else { format!("🔔 {n}") };
    let b = egui::Button::new(RichText::new(txt).color(if late { DANGER } else if n > 0 { WARN } else { TEXT })).fill(if late { tint(DANGER, 0.12) } else if n > 0 { tint(WARN, 0.12) } else { Color32::TRANSPARENT });
    if ui.add(b).on_hover_text(if late { "Hay tareas vencidas" } else { "Tareas y avisos del equipo" }).clicked() {
        app.tasks.open = true;
        let made = app.db.generate_tasks(app.today);
        if made > 0 {
            app.changed();
        }
    }
}

fn task_row(app: &mut App, ui: &mut Ui, t: &Task, me: &str) -> Option<(u32, bool)> {
    let mut out = None;
    ui.horizontal(|ui| {
        let mut done = !t.pending();
        if ui.checkbox(&mut done, "").on_hover_text(if done { "Marcar como pendiente" } else { "Marcar como hecha" }).changed() {
            out = Some((t.id, done));
        }
        ui.vertical(|ui| {
            let col = if !t.pending() { MUTED } else if t.overdue(app.today) { DANGER } else { TEXT };
            let mut text = RichText::new(&t.text).color(col);
            if !t.pending() {
                text = text.strikethrough();
            }
            ui.label(text);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if let Some(pid) = t.patient_id && ui.link(RichText::new(format!("👤 {}", app.db.patient_name(pid))).size(13.0)).clicked() {
                    app.open_patient(pid, PatTab::Resumen);
                    app.tasks.open = false;
                }
                if let Some(d) = t.due {
                    let txt = if d == app.today { "hoy".to_string() } else { fmt_date(d) };
                    ui.label(RichText::new(format!("📅 {txt}")).size(13.0).color(if t.overdue(app.today) { DANGER } else { MUTED }));
                }
                let to = if t.to.is_empty() { "todo el equipo".to_string() } else if t.to == me { "para mí".to_string() } else { format!("para {}", t.to) };
                ui.label(RichText::new(format!("{to} · de {}", t.by)).size(13.0).color(MUTED));
                if let Some(d) = t.done_on {
                    ui.label(RichText::new(format!("hecha el {} por {}", fmt_date(d), t.done_by)).size(13.0).color(OK));
                }
            });
        });
    });
    out
}

/// Ventana de tareas.
pub fn window(app: &mut App, ctx: &egui::Context) {
    let me = user_name(app);
    let mut close = false;
    crate::theme::modal("tasks_win").show(ctx, |ui| {
        ui.set_width(760.0);
        ui.horizontal(|ui| {
            title(ui, "Tareas", "Avisos del equipo y llamadas pendientes");
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut app.tasks.all_users, "Ver las de todo el equipo");
            ui.checkbox(&mut app.tasks.show_done, "Mostrar hechas");
        });
        ui.add_space(4.0);
        let mut list: Vec<Task> = app
            .db
            .tasks
            .iter()
            .filter(|t| (app.tasks.all_users || t.for_user(&me)) && (app.tasks.show_done || t.pending()))
            .cloned()
            .collect();
        list.sort_by_key(|t| (!t.pending(), t.due.unwrap_or(chrono::NaiveDate::MAX), t.id));
        let mut change = None;
        egui::ScrollArea::vertical().id_salt("tasks_list").max_height(380.0).show(ui, |ui| {
            if list.is_empty() {
                empty_state(ui, "No hay tareas pendientes.");
            }
            for t in &list {
                if let Some(c) = task_row(app, ui, t, &me) {
                    change = Some(c);
                }
                ui.separator();
            }
        });
        if let Some((id, done)) = change {
            app.db.complete_task(id, app.today, &me, done);
            app.changed();
        }
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Nueva tarea");
            ui.add(egui::TextEdit::singleline(&mut app.tasks.text).hint_text("Qué hay que hacer (p. ej. llamar para confirmar la cita del jueves)").desired_width(f32::INFINITY));
            ui.horizontal_wrapped(|ui| {
                ui.label("Para");
                let users: Vec<String> = app.db.users.iter().filter(|u| u.active).map(|u| u.name.clone()).collect();
                let sel = if app.tasks.to.is_empty() { "Todo el equipo".to_string() } else { app.tasks.to.clone() };
                crate::theme::combo("task_to").selected_text(sel).show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.tasks.to, String::new(), "Todo el equipo");
                    for u in users {
                        ui.selectable_value(&mut app.tasks.to, u.clone(), u);
                    }
                });
                ui.checkbox(&mut app.tasks.with_due, "Para el día");
                if app.tasks.with_due {
                    crate::screens::date_picker(ui, "task_due", &mut app.tasks.due);
                }
                if let Some(pid) = app.patient {
                    ui.checkbox(&mut app.tasks.link_patient, format!("Sobre {}", app.db.patient_name(pid)));
                }
            });
            ui.horizontal(|ui| {
                if primary(ui, "＋ Añadir tarea").clicked() {
                    if app.tasks.text.trim().is_empty() {
                        app.warn("Escriba qué hay que hacer.");
                    } else {
                        let pid = if app.tasks.link_patient { app.patient } else { None };
                        let due = app.tasks.with_due.then_some(app.tasks.due);
                        let (text, to) = (app.tasks.text.clone(), app.tasks.to.clone());
                        app.db.add_task(app.today, &me, &to, pid, &text, due);
                        app.tasks.text.clear();
                        app.changed();
                        app.notify("Tarea añadida.");
                    }
                }
                right_aligned(ui, |ui| {
                    if ui.button("Cerrar").clicked() {
                        close = true;
                    }
                });
            });
        });
    });
    if close {
        app.tasks.open = false;
    }
}

/// Tarjeta de Inicio con las tareas más urgentes del usuario.
pub fn home_card(app: &mut App, ui: &mut Ui) {
    let me = user_name(app);
    let list: Vec<Task> = app.db.pending_tasks(&me).into_iter().take(5).cloned().collect();
    let total = app.db.pending_tasks(&me).len();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Mis tareas");
            right_aligned(ui, |ui| {
                if ui.button(if total > list.len() { format!("Ver las {total}") } else { "Abrir".into() }).clicked() {
                    app.tasks.open = true;
                }
            });
        });
        if list.is_empty() {
            muted(ui, "Nada pendiente.");
        }
        let mut change = None;
        for t in &list {
            if let Some(c) = task_row(app, ui, t, &me) {
                change = Some(c);
            }
        }
        if let Some((id, done)) = change {
            app.db.complete_task(id, app.today, &me, done);
            app.changed();
        }
    });
}
