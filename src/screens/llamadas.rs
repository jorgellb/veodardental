//! Aviso de llamada entrante (ficha del paciente al sonar el teléfono),
//! ajustes de telefonía y registro de llamadas.

use crate::app::{App, Dialog, PatTab};
use crate::model::*;
use crate::phone::*;
use crate::theme::*;
use egui::{Align2, Color32, CornerRadius, RichText, Ui, vec2};
use std::sync::mpsc::Receiver;

#[derive(Default)]
pub struct CallState {
    pub rx: Option<Receiver<String>>,
    /// Puerto local en el que este puesto recibe los avisos.
    pub port: Option<u16>,
    pub error: Option<String>,
    /// Llamadas en pantalla: (id del registro, número, hora).
    pub popups: Vec<(u32, String, chrono::NaiveDateTime)>,
    pub test_number: String,
    /// Llevar la pantalla de Configuración hasta la tarjeta de telefonía.
    pub scroll_here: bool,
}

/// Empieza a recibir avisos de llamada en este ordenador.
pub fn start(app: &mut App, ctx: &egui::Context) {
    let c = ctx.clone();
    match listen(CALL_PORT, move || c.request_repaint()) {
        Ok((port, rx)) => {
            app.phone.port = Some(port);
            app.phone.rx = Some(rx);
        }
        Err(e) => app.phone.error = Some(e),
    }
}

fn me(app: &App) -> String {
    app.user_id.and_then(|u| app.db.user(u)).map(|u| u.name.clone()).unwrap_or_else(|| app.db.config.current_user.clone())
}

/// Llega una llamada: se apunta en el registro y se muestra el aviso.
pub fn incoming(app: &mut App, number: &str) {
    let who = me(app);
    let id = app.db.log_call(number, &who);
    app.phone.popups.retain(|p| !same_number(&p.1, number) || normalize(number).is_empty());
    app.phone.popups.insert(0, (id, number.trim().to_string(), now()));
    app.phone.popups.truncate(3);
    app.changed();
}

/// Recoge los avisos que hayan llegado y retira los de hace más de 15 minutos.
pub fn poll(app: &mut App, ctx: &egui::Context) {
    let mut got = vec![];
    if let Some(rx) = &app.phone.rx {
        while let Ok(n) = rx.try_recv() {
            got.push(n);
        }
    }
    for n in got {
        incoming(app, &n);
        if app.screenshot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Critical));
        }
    }
    let limit = now() - chrono::Duration::minutes(15);
    app.phone.popups.retain(|p| p.2 > limit);
}

enum Act {
    Close(u32),
    Open(u32, u32),
    Appt(u32, u32),
    NewPatient(u32, String),
}

/// Avisos de llamada en pantalla.
pub fn popups(app: &mut App, ctx: &egui::Context) {
    if app.phone.popups.is_empty() {
        return;
    }
    let list = app.phone.popups.clone();
    let today = app.today;
    let mut act = None;
    egui::Area::new(egui::Id::unique("call_popups")).anchor(Align2::CENTER_TOP, vec2(131.0, 76.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        for (id, number, when) in &list {
            egui::Frame::new()
                .fill(CARD)
                .stroke(egui::Stroke::new(2.0, INFO))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(egui::Margin::same(14))
                .shadow(egui::Shadow { offset: [0, 6], blur: 22, spread: 0, color: Color32::from_black_alpha(55) })
                .show(ui, |ui| {
                    ui.set_width(540.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("📞 Llamada entrante").family(sb()).size(17.0).color(INFO));
                        ui.label(RichText::new(pretty(number)).family(sb()).size(17.0));
                        right_aligned(ui, |ui| {
                            muted(ui, when.format("%H:%M").to_string());
                        });
                    });
                    let log = app.db.calls.iter().find(|c| c.id == *id).cloned();
                    let pats = log.as_ref().map(|l| l.patients.clone()).unwrap_or_default();
                    if pats.len() > 1 {
                        muted(ui, format!("Este número lo comparten {} pacientes", pats.len()));
                    }
                    for pid in pats.iter().take(3) {
                        let Some(p) = app.db.patient(*pid).cloned() else { continue };
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(RichText::new(p.full_name()).family(sb()).size(19.0)).truncate());
                            right_aligned(ui, |ui| {
                                if ui.button("Nueva cita").clicked() {
                                    act = Some(Act::Appt(*id, p.id));
                                }
                                if primary(ui, "Abrir ficha").clicked() {
                                    act = Some(Act::Open(*id, p.id));
                                }
                            });
                        });
                        let age = p.age(today).map(|a| format!(" · {a} años")).unwrap_or_default();
                        muted(ui, format!("Nº historial {}{} · {}", p.history_no, age, app.db.tariff_name(p.tariff_id)));
                        if !p.alerts.is_empty() || !p.allergies.is_empty() {
                            let alg = if p.allergies.is_empty() { String::new() } else { format!(" · Alergia: {}", p.allergies) };
                            ui.label(RichText::new(format!("⚠ {}{alg}", p.alerts)).color(DANGER).size(13.5));
                        }
                        let next = app.db.next_appt(p.id, today).map(|a| format!("Próxima cita: {} a las {} con {}", fmt_date(a.date), a.start.format("%H:%M"), app.db.doctor_alias(a.doctor_id))).unwrap_or_else(|| "Sin próxima cita".into());
                        let last = app.db.last_visit(p.id).map(|d| format!(" · Última visita {}", fmt_date(d))).unwrap_or_default();
                        ui.label(RichText::new(format!("{next}{last}")).size(14.5));
                        let bal = app.db.patient_balance(p.id);
                        let pend = app.db.budgets.iter().filter(|b| b.patient_id == p.id && b.state == BudgetState::Pendiente).count();
                        let health = app.db.health_status(p.id, today);
                        if bal > 0.005 || pend > 0 || health.is_some() {
                            ui.horizontal_wrapped(|ui| {
                                if bal > 0.005 {
                                    pill(ui, &format!("Debe {}", money(bal)), DANGER);
                                }
                                if pend > 0 {
                                    pill(ui, &format!("{pend} presupuesto(s) sin aceptar"), WARN);
                                }
                                if let Some(s) = health {
                                    pill(ui, s, WARN);
                                }
                            });
                        }
                    }
                    if pats.is_empty() {
                        ui.separator();
                        let other = log.as_ref().map(|l| l.other.clone()).unwrap_or_default();
                        if other.is_empty() {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Número no registrado").size(16.0));
                                right_aligned(ui, |ui| {
                                    if !normalize(number).is_empty() && primary(ui, "＋ Nuevo paciente con este teléfono").clicked() {
                                        act = Some(Act::NewPatient(*id, number.clone()));
                                    }
                                });
                            });
                        } else {
                            ui.label(RichText::new(other).family(sb()).size(17.0));
                        }
                    }
                    ui.add_space(6.0);
                    if ui.button("Cerrar").clicked() {
                        act = Some(Act::Close(*id));
                    }
                });
            ui.add_space(8.0);
        }
    });
    let Some(a) = act else { return };
    // qué hacer después de cerrar el aviso
    type Then = Box<dyn FnOnce(&mut App)>;
    let (id, then): (u32, Then) = match a {
        Act::Close(id) => (id, Box::new(|_| {})),
        Act::Open(id, pid) => (id, Box::new(move |app: &mut App| app.open_patient(pid, PatTab::Resumen))),
        Act::Appt(id, pid) => (
            id,
            Box::new(move |app: &mut App| {
                let d = crate::screens::agenda::ApptDraft::new_at(app, app.today, None, Some(pid));
                app.dialog = Dialog::Appointment(d);
            }),
        ),
        Act::NewPatient(id, number) => (
            id,
            Box::new(move |app: &mut App| {
                let mut p = crate::screens::pacientes::blank_patient(&app.db, app.today);
                p.mobile = pretty(&number);
                app.dialog = Dialog::NewPatient(Box::new(p));
            }),
        ),
    };
    app.phone.popups.retain(|p| p.0 != id);
    then(app);
}

/// Configuración › Telefonía.
pub fn panel(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        if app.phone.scroll_here {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
            app.phone.scroll_here = false;
        }
        section(ui, "Telefonía: ficha del paciente al recibir una llamada");
        match (app.phone.port, app.phone.error.clone()) {
            (Some(p), _) => ui.label(RichText::new(format!("✔ Este puesto recibe los avisos de llamada (puerto local {p}).")).color(OK)),
            (None, Some(e)) => ui.label(RichText::new(format!("⚠ Los avisos de llamada no están activos en este puesto: {e}")).color(WARN)),
            _ => ui.label(RichText::new("Desactivado en el modo demostración.").color(MUTED())),
        };
        muted(
            ui,
            "En el programa de teléfono del ordenador (softphone) o en la centralita, busque la opción «al recibir una llamada, ejecutar un programa» o «abrir una dirección» y ponga una de estas dos. Cambie NÚMERO por la palabra con la que su programa indica el número que llama.",
        );
        ui.add_space(4.0);
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "veodardental".into());
        let rows = [("Ejecutar programa", format!("\"{exe}\" --llamada NÚMERO")), ("Abrir dirección", format!("http://127.0.0.1:{}/llamada?numero=NÚMERO", app.phone.port.unwrap_or(CALL_PORT)))];
        for (label, text) in rows {
            ui.horizontal(|ui| {
                lab(ui, 150.0, RichText::new(label).color(MUTED()));
                let w = (ui.available_width() - 100.0).max(220.0);
                lab(ui, w, RichText::new(&text).family(sb())).on_hover_text(&text);
                if ui.small_button("Copiar").clicked() {
                    ui.ctx().copy_text(text.clone());
                    app.notify("Copiado: péguelo en el programa de teléfono.");
                }
            });
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Probar sin llamar:");
            ui.add_sized([200.0, 32.0], egui::TextEdit::singleline(&mut app.phone.test_number).hint_text("Número que llama"));
            if ui.button("Simular llamada").clicked() {
                let n = app.phone.test_number.trim().to_string();
                if normalize(&n).is_empty() {
                    app.warn("Escriba un número de teléfono.");
                } else {
                    incoming(app, &n);
                }
            }
        });
    });
}

/// Listín › Llamadas recibidas.
pub fn recent_calls(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Llamadas recibidas");
        let mut list = app.db.calls.clone();
        list.sort_by_key(|c| std::cmp::Reverse(c.id));
        let can_config = app.allowed(crate::app::Screen::Config);
        if list.is_empty() {
            muted(ui, "Aún no hay llamadas. Para que aparezcan al sonar el teléfono, conecte la centralita o el programa de teléfono.");
        }
        if can_config && ui.link("⚙ Configurar telefonía").clicked() {
            crate::screens::config::open(app, crate::screens::config::CfgTab::Telefonia);
            app.phone.scroll_here = true;
        }
        if list.is_empty() {
            return;
        }
        let mut open = None;
        for c in list.iter().take(12) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(c.when.format("%d/%m %H:%M").to_string()).color(MUTED()).size(13.5));
                ui.label(RichText::new(pretty(&c.number)).size(14.0));
                match c.patients.first() {
                    Some(pid) => {
                        let name = app.db.patient_name(*pid);
                        let extra = if c.patients.len() > 1 { format!(" (+{})", c.patients.len() - 1) } else { String::new() };
                        if ui.link(RichText::new(format!("{name}{extra}")).size(14.0)).clicked() {
                            open = Some(*pid);
                        }
                    }
                    None => {
                        ui.add(egui::Label::new(RichText::new(if c.other.is_empty() { "desconocido".to_string() } else { c.other.clone() }).color(MUTED()).size(14.0)).truncate());
                    }
                }
            });
        }
        if let Some(pid) = open {
            app.open_patient(pid, PatTab::Resumen);
        }
    });
}
