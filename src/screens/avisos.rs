//! Avisos instantáneos: botón 💬 de la barra superior y notas emergentes.

use crate::app::{App, PatTab};
use crate::theme::*;
use egui::{Align2, Color32, CornerRadius, RichText, Ui, vec2};

#[derive(Default)]
pub struct ChatState {
    /// Ventana de avisos abierta.
    pub open: bool,
    pub to: String,
    pub text: String,
    /// Último aviso por el que ya se llamó la atención (parpadeo en la barra de tareas).
    pub alerted: u32,
}

fn me(app: &App) -> String {
    app.user_id.and_then(|u| app.db.user(u)).map(|u| u.name.clone()).unwrap_or_else(|| app.db.config.current_user.clone())
}

fn send(app: &mut App, text: &str) {
    let (from, to) = (me(app), app.chat.to.clone());
    app.db.send_msg(&from, &to, text, None);
    app.changed();
    app.notify(format!("Aviso enviado a {}.", if to.is_empty() { "todos".to_string() } else { to }));
}

/// Botón 💬 de la barra superior: abre o cierra la ventana de avisos.
pub fn button(app: &mut App, ui: &mut Ui) {
    let b = egui::Button::new("💬").fill(if app.chat.open { ACCENT_SOFT } else { Color32::TRANSPARENT });
    if ui.add(b).on_hover_text("Avisos al equipo (recepción, gabinetes)").clicked() {
        app.chat.open = !app.chat.open;
    }
}

/// Ventana de avisos: se queda abierta mientras se elige destinatario y se escribe.
pub fn window(app: &mut App, ctx: &egui::Context) {
    if !app.chat.open {
        return;
    }
    let user = me(app);
    let mut open = true;
    egui::Window::new("Avisos al equipo").id(egui::Id::unique("chat_window")).open(&mut open).collapsible(false).resizable(false).anchor(Align2::RIGHT_TOP, vec2(-18.0, 76.0)).default_width(420.0).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.label(RichText::new("Para").color(MUTED()).size(13.5));
        ui.horizontal_wrapped(|ui| {
            let users: Vec<String> = app.db.users.iter().filter(|u| u.active && u.name != user).map(|u| u.name.clone()).collect();
            if ui.selectable_label(app.chat.to.is_empty(), "Todos").clicked() {
                app.chat.to.clear();
            }
            for u in users {
                if ui.selectable_label(app.chat.to == u, &u).clicked() {
                    app.chat.to = u;
                }
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Frases rápidas (se envían con un clic)").color(MUTED()).size(13.5));
        ui.horizontal_wrapped(|ui| {
            for q in crate::chat::QUICK {
                if ui.button(q).clicked() {
                    send(app, q);
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let resp = ui.add(egui::TextEdit::singleline(&mut app.chat.text).hint_text("Escriba un aviso y pulse Intro").desired_width(320.0));
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Enviar").clicked() || enter) && !app.chat.text.trim().is_empty() {
                let t = std::mem::take(&mut app.chat.text);
                send(app, &t);
                resp.request_focus();
            }
        });
        ui.separator();
        let mut recent: Vec<crate::chat::InternalMsg> = app.db.chat.iter().filter(|m| m.from == user || m.for_user(&user)).cloned().collect();
        recent.sort_by_key(|m| std::cmp::Reverse(m.id));
        if recent.is_empty() {
            muted(ui, "Sin avisos recientes.");
        }
        egui::ScrollArea::vertical().id_salt("chat_recent").max_height(220.0).show(ui, |ui| {
            for m in recent.iter().take(30) {
                let mine = m.from == user;
                let dir = if mine { format!("→ {}", if m.to.is_empty() { "Todos" } else { &m.to }) } else { format!("← {}", m.from) };
                ui.label(RichText::new(format!("{}  {}  ·  {}", m.when.format("%H:%M"), dir, m.text)).size(14.0).color(if mine { MUTED() } else { TEXT }));
            }
        });
    });
    if !open {
        app.chat.open = false;
    }
}

/// Notas emergentes con los avisos sin leer del usuario.
pub fn popups(app: &mut App, ctx: &egui::Context) {
    let user = me(app);
    let unread: Vec<crate::chat::InternalMsg> = app.db.unread_msgs(&user).into_iter().cloned().collect();
    if unread.is_empty() {
        return;
    }
    let newest = unread.iter().map(|m| m.id).max().unwrap_or(0);
    if newest > app.chat.alerted {
        app.chat.alerted = newest;
        if app.screenshot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
        }
    }
    let mut read = vec![];
    let mut reply: Option<(String, String)> = None;
    let mut open_p = None;
    egui::Area::new(egui::Id::unique("chat_popups")).anchor(Align2::RIGHT_BOTTOM, vec2(-18.0, -48.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_width(360.0);
        for m in unread.iter().rev().take(3).rev() {
            egui::Frame::new()
                .fill(CARD)
                .stroke(egui::Stroke::new(2.0, ACCENT))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(12))
                .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(40) })
                .show(ui, |ui| {
                    ui.set_width(336.0);
                    ui.set_max_width(336.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("💬 {}", m.from)).family(sb()));
                        ui.label(RichText::new(m.when.format("%H:%M").to_string()).color(MUTED()).size(13.0));
                    });
                    ui.label(RichText::new(&m.text).size(16.0));
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("✔ Visto").clicked() {
                            read.push(m.id);
                        }
                        if ui.button("Voy").clicked() {
                            read.push(m.id);
                            reply = Some((m.from.clone(), "Voy".into()));
                        }
                        if ui.button("Un momento").clicked() {
                            read.push(m.id);
                            reply = Some((m.from.clone(), "Un momento, ahora voy".into()));
                        }
                        if let Some(pid) = m.patient_id
                            && ui.button("Ficha").clicked()
                        {
                            read.push(m.id);
                            open_p = Some(pid);
                        }
                    });
                });
            ui.add_space(8.0);
        }
        if unread.len() > 3 {
            muted(ui, format!("y {} avisos más", unread.len() - 3));
        }
    });
    if !read.is_empty() {
        for id in read {
            app.db.mark_read(id, &user);
        }
        app.changed();
    }
    if let Some((to, text)) = reply {
        app.db.send_msg(&user, &to, &text, None);
        app.changed();
    }
    if let Some(pid) = open_p {
        app.open_patient(pid, PatTab::Resumen);
    }
}
