//! Pantalla de acceso, bloqueo y cambio de contraseña; gestión de usuarios.

use crate::app::App;

use crate::theme::*;
use crate::users::*;
use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Ui, vec2};

#[derive(Default)]
pub struct LoginState {
    pub selected: Option<u32>,
    pub pin: String,
    pub error: String,
    pub old_pin: String,
    pub new_pin: String,
    pub new_pin2: String,
    pub show_change: bool,
    pub new_user: String,
    pub reset_pin: String,
    pub edit_user: Option<u32>,
    pub focus_pin: bool,
    /// Contraseña correcta de este usuario: falta el código de la app del móvil.
    pub pending_code: Option<u32>,
    pub code: String,
    /// Activación del código del móvil en curso: (usuario, clave nueva, código escrito).
    pub enroll: Option<(u32, String, String)>,
}

fn finish_login(app: &mut App, id: u32, time: f64, second_factor: bool) {
    app.user_id = Some(id);
    app.second_factor = second_factor;
    app.login.pending_code = None;
    app.login.code.clear();
    app.login.error.clear();
    app.last_activity = time;
    app.login.show_change = app.db.user(id).map(|u| u.must_change).unwrap_or(false);
    app.login.focus_pin = true;
    app.changed();
}

fn try_login(app: &mut App, time: f64) {
    let Some(id) = app.login.selected else {
        app.login.error = "Elija primero su usuario.".into();
        return;
    };
    let pin = std::mem::take(&mut app.login.pin);
    match app.db.login(id, &pin) {
        Ok(LoginStep::Done) => finish_login(app, id, time, false),
        Ok(LoginStep::NeedCode) => {
            app.login.pending_code = Some(id);
            app.login.code.clear();
            app.login.error.clear();
            app.login.focus_pin = true;
        }
        Err(e) => {
            app.login.error = e;
            app.login.focus_pin = true;
            app.changed();
        }
    }
}

fn try_code(app: &mut App, time: f64) {
    let Some(id) = app.login.pending_code else { return };
    let code = std::mem::take(&mut app.login.code);
    match app.db.login_code(id, &code, crate::totp::unix_now()) {
        Ok(()) => finish_login(app, id, time, true),
        Err(e) => {
            app.login.error = e;
            app.login.focus_pin = true;
            // bloqueado: se vuelve a la contraseña
            if app.db.user(id).is_some_and(|u| !u.active) {
                app.login.pending_code = None;
            }
            app.changed();
        }
    }
}

pub fn login_screen(app: &mut App, ui: &mut Ui) {
    let time = ui.input(|i| i.time);
    let users: Vec<User> = app.db.users.iter().filter(|u| u.active || u.profile == Profile::Direccion).cloned().collect();
    if app.login.selected.is_none() {
        app.login.selected = users.iter().find(|u| u.name == app.db.config.current_user).or(users.first()).map(|u| u.id);
        app.login.focus_pin = true;
    }
    let card_w = 600.0f32.min(ui.available_width() - 40.0);
    let top = ((ui.available_height() - 700.0) / 2.0).max(16.0);
    egui::ScrollArea::vertical().id_salt("login_scroll").auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(top);
        ui.vertical_centered(|ui| {
            egui::Frame::new()
                .fill(CARD)
                .corner_radius(CornerRadius::same(22))
                .inner_margin(egui::Margin::symmetric(36, 32))
                .shadow(egui::Shadow { offset: [0, 18], blur: 48, spread: 0, color: Color32::from_rgba_unmultiplied(0, 0, 0, 70) })
                .show(ui, |ui| {
                    ui.set_width(card_w - 72.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::hover());
                            ui.painter().rect_filled(r, CornerRadius::same(13), ACCENT_SOFT);
                            crate::screens::odontograma::draw_logo(ui.painter(), r.shrink(7.0));
                            ui.add_space(4.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.label(RichText::new("VeodarDental").font(bf(22.0)).color(HEADING));
                                ui.label(RichText::new(&app.db.config.name).font(mdf(13.0)).color(MUTED));
                            });
                        });
                        ui.add_space(26.0);
                        ui.label(RichText::new("Bienvenido de nuevo").font(bf(25.0)).color(HEADING));
                        ui.add_space(2.0);
                        ui.label(RichText::new("Elija su usuario e introduzca la contraseña para entrar.").font(mdf(14.0)).color(MUTED));
                        ui.add_space(18.0);
                        let cols = 3usize;
                        let gap = 10.0;
                        let tile_w = ((ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32).floor();
                        for row in users.chunks(cols) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = vec2(gap, gap);
                                for u in row {
                                    if user_tile(ui, app, u, tile_w) {
                                        app.login.selected = Some(u.id);
                                        app.login.pin.clear();
                                        app.login.pending_code = None;
                                        app.login.code.clear();
                                        app.login.error.clear();
                                        app.login.focus_pin = true;
                                    }
                                }
                            });
                            ui.add_space(gap - ui.spacing().item_spacing.y);
                        }
                        ui.add_space(14.0);
                        let who = app.login.selected.and_then(|id| app.db.user(id)).map(|u| u.name.clone()).unwrap_or_default();
                        // segundo paso: el código de 6 cifras de la app del móvil
                        let coding = app.login.pending_code.is_some();
                        if coding {
                            ui.label(RichText::new(format!("Código del móvil de {who}")).font(mdf(13.0)).color(MUTED));
                            ui.label(RichText::new("Abra la app de autenticación del móvil y escriba el código de 6 cifras de VeodarDental.").font(mdf(12.5)).color(SUBTLE));
                        } else {
                            ui.label(RichText::new(format!("Contraseña de {who}")).font(mdf(13.0)).color(MUTED));
                        }
                        ui.add_space(2.0);
                        let r = if coding {
                            ui.add_sized([ui.available_width(), 46.0], egui::TextEdit::singleline(&mut app.login.code).id_salt("login_code").char_limit(6).hint_text("000000").font(sbf(18.0)).margin(vec2(14.0, 11.0)))
                        } else {
                            ui.add_sized(
                                [ui.available_width(), 46.0],
                                egui::TextEdit::singleline(&mut app.login.pin).id_salt("login_pin").password(true).hint_text("••••").font(sbf(18.0)).margin(vec2(14.0, 11.0)),
                            )
                        };
                        if app.login.focus_pin {
                            r.request_focus();
                            app.login.focus_pin = false;
                        }
                        let enter = (r.has_focus() || r.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if !app.login.error.is_empty() {
                            ui.add_space(4.0);
                            ui.label(RichText::new(&app.login.error).font(mdf(13.5)).color(DANGER));
                        }
                        ui.add_space(12.0);
                        // teclado numérico en pantalla (sirve también en pantallas táctiles)
                        let mut entrar = false;
                        let keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "⌫"];
                        let kgap = 6.0;
                        let kw = ((ui.available_width() - kgap * (keys.len() as f32 - 1.0)) / keys.len() as f32).floor();
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(kgap, kgap);
                            for k in keys {
                                let b = egui::Button::new(RichText::new(k).font(sbf(16.0)).color(TEXT))
                                    .fill(Color32::from_rgb(246, 248, 251))
                                    .stroke(Stroke::new(1.0, HAIRLINE))
                                    .min_size(vec2(kw, 42.0))
                                    .corner_radius(CornerRadius::same(10));
                                if ui.add(b).clicked() {
                                    let (t, max) = if coding { (&mut app.login.code, 6) } else { (&mut app.login.pin, 32) };
                                    if k == "⌫" {
                                        t.pop();
                                    } else if t.chars().count() < max {
                                        t.push_str(k);
                                    }
                                }
                            }
                        });
                        ui.add_space(16.0);
                        let er = ui.add_sized([ui.available_width(), 48.0], egui::Button::new(RichText::new("Entrar").font(sbf(16.0)).color(Color32::WHITE)).fill(ACCENT).stroke(Stroke::NONE).corner_radius(CornerRadius::same(12)));
                        if er.hovered() {
                            ui.painter().rect_filled(er.rect, CornerRadius::same(12), Color32::from_rgba_unmultiplied(0, 0, 0, 18));
                        }
                        if er.clicked() {
                            entrar = true;
                        }
                        if entrar || enter {
                            if coding {
                                try_code(app, time);
                            } else {
                                try_login(app, time);
                            }
                        }
                        ui.add_space(14.0);
                        if coding {
                            if ui.link("← Volver a la contraseña").clicked() {
                                app.login.pending_code = None;
                                app.login.code.clear();
                                app.login.error.clear();
                                app.login.focus_pin = true;
                            }
                        } else {
                            ui.horizontal(|ui| {
                                let (d, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                                ui.painter().circle_filled(d.center(), 3.0, SUBTLE);
                                ui.label(RichText::new("Contraseña inicial de todos los usuarios: 0000 (se pedirá cambiarla al entrar).").font(mdf(12.5)).color(SUBTLE));
                            });
                        }
                    });
                });
        });
    });
}

/// Tarjeta de un usuario en la pantalla de acceso: avatar con iniciales,
/// nombre y perfil. Devuelve si se ha pulsado.
fn user_tile(ui: &mut Ui, app: &App, u: &User, w: f32) -> bool {
    let sel = app.login.selected == Some(u.id);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 62.0), Sense::click());
    let label = format!("{} — {}", u.name, if u.active { u.profile.label() } else { "Bloqueado" });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    let color = u.doctor_id.and_then(|d| app.db.doctor(d)).map(|d| rgb(d.color)).unwrap_or(match u.profile {
        Profile::Direccion => VIOLET,
        Profile::Recepcion => INFO,
        _ => ACCENT,
    });
    let p = ui.painter();
    let (fill, stroke) = if sel {
        (ACCENT_SOFT, Stroke::new(1.5, ACCENT))
    } else if resp.hovered() {
        (Color32::from_rgb(248, 250, 252), Stroke::new(1.0, mix(BORDER, ACCENT, 0.35)))
    } else {
        (CARD, Stroke::new(1.0, BORDER))
    };
    p.rect(r, CornerRadius::same(14), fill, stroke, egui::StrokeKind::Inside);
    avatar(p, r.left_center() + vec2(29.0, 0.0), 17.0, &u.name, if u.active { color } else { SUBTLE });
    let x = r.left() + 56.0;
    let max_w = r.right() - x - if sel { 28.0 } else { 10.0 };
    let one_line = |text: &str, font: egui::FontId, color: Color32| {
        let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(max_w);
        p.layout_job(job)
    };
    let name = one_line(&u.name, sbf(14.5), if sel { ACCENT_DARK } else { HEADING });
    p.galley(egui::pos2(x, r.center().y - name.size().y + 1.0), name, HEADING);
    let sub = if u.active { u.profile.label() } else { "Bloqueado" };
    let sub = one_line(sub, mdf(12.3), if u.active { MUTED } else { DANGER });
    p.galley(egui::pos2(x, r.center().y + 2.0), sub, MUTED);
    if sel {
        let c = egui::pos2(r.right() - 17.0, r.center().y);
        p.circle_filled(c, 8.0, ACCENT);
        p.line_segment([c + vec2(-3.6, 0.2), c + vec2(-1.0, 2.8)], Stroke::new(1.8, Color32::WHITE));
        p.line_segment([c + vec2(-1.0, 2.8), c + vec2(3.8, -2.6)], Stroke::new(1.8, Color32::WHITE));
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// Diálogo de cambio de contraseña (obligatorio la primera vez).
pub fn change_pin_dialog(app: &mut App, ctx: &egui::Context) {
    if !app.login.show_change {
        return;
    }
    let Some(uid) = app.user_id else { return };
    let must = app.db.user(uid).map(|u| u.must_change).unwrap_or(false);
    crate::theme::modal("chpin").show(ctx, |ui| {
        ui.set_width(420.0);
        title(ui, "Cambiar contraseña", if must { "Está usando la contraseña inicial. Elija una propia para proteger los datos de los pacientes." } else { "" });
        if !must {
            ui.label(RichText::new("Contraseña actual").color(MUTED));
            ui.add_sized([380.0, 36.0], egui::TextEdit::singleline(&mut app.login.old_pin).password(true));
        }
        ui.label(RichText::new("Nueva contraseña").color(MUTED));
        ui.add_sized([380.0, 36.0], egui::TextEdit::singleline(&mut app.login.new_pin).password(true));
        ui.label(RichText::new("Repita la nueva contraseña").color(MUTED));
        ui.add_sized([380.0, 36.0], egui::TextEdit::singleline(&mut app.login.new_pin2).password(true));
        if !app.login.error.is_empty() {
            ui.label(RichText::new(&app.login.error).color(DANGER));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if primary(ui, "Guardar").clicked() {
                if app.login.new_pin != app.login.new_pin2 {
                    app.login.error = "Las contraseñas no coinciden.".into();
                } else {
                    let r = if must { app.db.set_pin(uid, &app.login.new_pin) } else { app.db.change_pin(uid, &app.login.old_pin, &app.login.new_pin) };
                    match r {
                        Ok(()) => {
                            app.login.show_change = false;
                            app.login.error.clear();
                            app.login.old_pin.clear();
                            app.login.new_pin.clear();
                            app.login.new_pin2.clear();
                            app.changed();
                            app.notify("Contraseña cambiada.");
                        }
                        Err(e) => app.login.error = e,
                    }
                }
            }
            if ui.button(if must { "Ahora no" } else { "Cancelar" }).clicked() {
                app.login.show_change = false;
                app.login.error.clear();
            }
        });
    });
}

/// Activación del código del móvil: código QR para la app, la clave para
/// escribirla a mano y un código de prueba para confirmar que la app la tiene.
fn enroll_card(app: &mut App, ui: &mut Ui, id: u32) {
    let Some((_, secret, _)) = app.login.enroll.clone() else { return };
    let name = app.db.user(id).map(|u| u.name.clone()).unwrap_or_default();
    let uri = crate::totp::uri(&secret, &format!("{name} ({})", app.db.config.name), "VeodarDental");
    let (mut confirm, mut cancel) = (false, false);
    ui.add_space(6.0);
    egui::Frame::new().fill(ROW_ALT).stroke(Stroke::new(1.0, HAIRLINE)).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            let _ = crate::screens::verifactu::draw_qr(ui, &uri, 176.0);
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                ui.label(RichText::new(format!("Activar el código del móvil de {name}")).family(sb()).size(15.5).color(HEADING));
                muted(ui, "1. Instale en el móvil una app de autenticación: Google Authenticator, Microsoft Authenticator, FreeOTP…");
                muted(ui, "2. En la app, añada una cuenta escaneando este código QR (o escriba la clave a mano).");
                muted(ui, "3. Escriba aquí el código de 6 cifras que muestra la app para confirmarlo.");
                ui.label(RichText::new(format!("Clave: {}", crate::totp::grouped(&secret))).monospace().size(13.5).color(TEXT));
                ui.horizontal(|ui| {
                    if let Some(e) = app.login.enroll.as_mut() {
                        ui.add_sized([130.0, 34.0], egui::TextEdit::singleline(&mut e.2).id_salt("totp_enroll").char_limit(6).hint_text("000000"));
                    }
                    if primary(ui, "Confirmar y activar").clicked() {
                        confirm = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        cancel = true;
                    }
                });
            });
        });
    });
    if cancel {
        app.login.enroll = None;
    }
    if confirm {
        let code = app.login.enroll.as_ref().map(|e| e.2.clone()).unwrap_or_default();
        match app.db.enable_totp(id, &secret, &code, crate::totp::unix_now()) {
            Ok(()) => {
                app.login.enroll = None;
                // quien lo acaba de activar para sí mismo ya ha demostrado tener el móvil
                if app.user_id == Some(id) {
                    app.second_factor = true;
                }
                app.changed();
                app.notify(format!("Código del móvil activado: desde ahora {name} entrará con su contraseña y el código de la app."));
            }
            Err(e) => app.warn(e),
        }
    }
}

/// Gestión de usuarios y registro de accesos (Configuración).
pub fn users_admin(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Usuarios y permisos");
            right_aligned(ui, |ui| {
                if ui.button("＋ Nuevo usuario").clicked() {
                    let id = app.db.users.iter().map(|u| u.id).max().unwrap_or(0) + 1;
                    let name = if app.login.new_user.trim().is_empty() { format!("Usuario {id}") } else { std::mem::take(&mut app.login.new_user).trim().to_string() };
                    app.db.users.push(new_user(id, &name, Profile::Recepcion, None, "0000"));
                    app.login.edit_user = Some(id);
                    app.changed();
                }
                ui.add_sized([180.0, 34.0], egui::TextEdit::singleline(&mut app.login.new_user).hint_text("Nombre"));
            });
        });
        let docs: Vec<(u32, String)> = app.db.collaborators.iter().map(|c| (c.id, c.alias.clone())).collect();
        let me = app.user_id;
        let list = app.db.users.clone();
        for u in &list {
            ui.horizontal(|ui| {
                let sel = app.login.edit_user == Some(u.id);
                if ui.selectable_label(sel, RichText::new(&u.name).family(sb())).clicked() {
                    app.login.edit_user = Some(u.id);
                }
                pill(ui, u.profile.label(), INFO);
                if !u.active {
                    pill(ui, "Bloqueado", DANGER);
                }
                if u.must_change {
                    pill(ui, "Contraseña inicial", WARN);
                }
                if !u.totp_secret.is_empty() {
                    pill(ui, "Código del móvil", OK);
                }
            });
        }
        if let Some(id) = app.login.edit_user {
            ui.separator();
            let mut reset = false;
            let (mut totp_on, mut totp_off) = (false, false);
            if let Some(u) = app.db.users.iter_mut().find(|u| u.id == id) {
                let before = serde_json::to_string(u).unwrap_or_default();
                ui.horizontal(|ui| {
                    field(ui, "Nombre", &mut u.name, 200.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Perfil").size(13.5).color(MUTED));
                        crate::theme::combo("u_prof").width(210.0).selected_text(u.profile.label()).show_ui(ui, |ui| {
                            for p in Profile::ALL {
                                ui.selectable_value(&mut u.profile, p, p.label());
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Colaborador").size(13.5).color(MUTED));
                        let cur = u.doctor_id.and_then(|d| docs.iter().find(|x| x.0 == d)).map(|x| x.1.clone()).unwrap_or_else(|| "—".into());
                        crate::theme::combo("u_doc").width(150.0).selected_text(cur).show_ui(ui, |ui| {
                            ui.selectable_value(&mut u.doctor_id, None, "—");
                            for (d, n) in &docs {
                                ui.selectable_value(&mut u.doctor_id, Some(*d), n);
                            }
                        });
                    });
                });
                ui.horizontal(|ui| {
                    if Some(u.id) != me {
                        ui.checkbox(&mut u.active, "Activo");
                    }
                    ui.add_sized([160.0, 34.0], egui::TextEdit::singleline(&mut app.login.reset_pin).password(true).hint_text("Nueva contraseña"));
                    if ui.button("Asignar contraseña").clicked() {
                        reset = true;
                    }
                });
                // segundo factor: el código de 6 cifras de una app del móvil
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Código del móvil al entrar").family(sb()));
                    if u.totp_secret.is_empty() {
                        pill(ui, "Sin activar", SUBTLE);
                        if ui.button("Activar el código del móvil").on_hover_text("Además de la contraseña, al entrar pedirá el código de 6 cifras de una app de autenticación del móvil").clicked() {
                            totp_on = true;
                        }
                    } else {
                        pill(ui, "Activado", OK);
                        if ui.button("Quitar el código del móvil").on_hover_text("Por ejemplo, si ha perdido el teléfono; después se puede activar de nuevo").clicked() {
                            totp_off = true;
                        }
                    }
                });
                let perms: Vec<&str> = [(Perm::Economia, "cobros y presupuestos"), (Perm::Caja, "caja"), (Perm::Facturacion, "facturas"), (Perm::Informes, "informes"), (Perm::Gastos, "gastos"), (Perm::Almacen, "almacén"), (Perm::Configuracion, "configuración y usuarios")]
                    .iter()
                    .filter(|(p, _)| u.profile.can(*p))
                    .map(|(_, n)| *n)
                    .collect();
                muted(ui, format!("Puede acceder a: agenda, pacientes, historia clínica{}{}", if perms.is_empty() { "" } else { ", " }, perms.join(", ")));
                if serde_json::to_string(u).unwrap_or_default() != before {
                    app.dirty = true;
                }
            }
            if totp_off {
                let by = app.db.config.current_user.clone();
                app.db.disable_totp(id, &by);
                app.login.enroll = None;
                app.changed();
                app.notify("Código del móvil quitado: al entrar solo se pedirá la contraseña.");
            }
            if totp_on {
                match crate::totp::new_secret() {
                    Ok(s) => app.login.enroll = Some((id, s, String::new())),
                    Err(e) => app.warn(e),
                }
            }
            if app.login.enroll.as_ref().is_some_and(|e| e.0 == id) {
                enroll_card(app, ui, id);
            }
            if reset {
                let pin = std::mem::take(&mut app.login.reset_pin);
                match app.db.set_pin(id, &pin) {
                    Ok(()) => {
                        if let Some(u) = app.db.users.iter_mut().find(|u| u.id == id) {
                            u.must_change = Some(id) != me;
                            u.active = true;
                        }
                        app.changed();
                        app.notify("Contraseña asignada. El usuario deberá cambiarla al entrar.");
                    }
                    Err(e) => app.warn(e),
                }
            }
        }
    });
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Registro de accesos");
        let tbl = Table::new("acc", vec![("Fecha", 160.0), ("Usuario", 180.0), ("Evento", 360.0)]);
        tbl.header(ui);
        let mut list = app.db.access_log.clone();
        list.reverse();
        if list.is_empty() {
            empty_state(ui, "Sin registros.");
        }
        for (i, l) in list.iter().take(25).enumerate() {
            tbl.row(ui, i, false, &[cell(l.when.format("%d/%m/%Y %H:%M:%S").to_string()), cell(&l.user), cellc(&l.event, if l.ok { TEXT } else { DANGER })]);
        }
    });
}
