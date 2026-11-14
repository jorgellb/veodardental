//! Configuración › Correo electrónico: servidor de correo saliente (SMTP)
//! para enviar desde el programa las órdenes al laboratorio y otros
//! documentos con adjuntos, con un correo de prueba.

use crate::app::App;
use crate::mail::{MailConfig, PRESETS, Security};
use crate::model::fmt_date;
use crate::theme::*;
use egui::{CornerRadius, RichText, Stroke, Ui};

#[derive(Default)]
pub struct MailUi {
    pub show_password: bool,
    /// Dirección a la que se manda el correo de prueba (por defecto, la de la clínica).
    pub test_to: Option<String>,
    pub test_job: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    /// Resultado del último correo de prueba.
    pub test_result: Option<Result<String, String>>,
}

/// Recoge el resultado del correo de prueba.
pub fn poll(app: &mut App, ctx: &egui::Context) {
    let Some(rx) = app.mail_ui.test_job.as_ref() else { return };
    match rx.try_recv() {
        Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            app.mail_ui.test_job = None;
            app.mail_ui.test_result = Some(Err("El envío se interrumpió.".into()));
        }
        Ok(r) => {
            app.mail_ui.test_job = None;
            let to = app.mail_ui.test_to.clone().unwrap_or_default();
            app.mail_ui.test_result = Some(r.map(|_| format!("Correo de prueba enviado a {to}. Compruebe que ha llegado (mire también en «correo no deseado»).")));
        }
    }
}

/// Tarjeta «Envío automático de las comunicaciones». Devuelve si se pidió hacer ya el envío de hoy.
fn auto_card(app: &mut App, ui: &mut Ui) -> bool {
    let me = crate::shared::machine_name();
    let mut run_now = false;
    let mut goto_templates = false;
    // qué plantillas salen por correo
    let by_mail: Vec<String> = app.db.msg_templates.iter().filter(|t| t.active && t.channel == crate::comms::Channel::Email).map(|t| t.name.clone()).collect();
    let sending = app.comms.outbox.is_some();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let c = &mut app.db.mail;
        ui.horizontal(|ui| {
            section(ui, "Envío automático de las comunicaciones");
            right_aligned(ui, |ui| {
                if !c.auto_send {
                    pill(ui, "Desactivado", MUTED());
                } else if c.auto_station != me {
                    pill(ui, "En otro ordenador", INFO);
                } else if c.last_auto == Some(app.today) {
                    pill(ui, "Hoy ya enviado", OK);
                } else {
                    pill(ui, "Activado", OK);
                }
            });
        });
        muted(
            ui,
            "Cada día, a la hora elegida, el programa prepara los recordatorios de cita, los avisos de revisión, las felicitaciones y los seguimientos de presupuesto, y envía por correo los de los pacientes que aceptan recibir correos. A los demás se les preparan por WhatsApp o SMS, como hasta ahora. Si la página de la clínica está publicada, cada correo lleva su enlace (gestionar la cita, pedir cita o firmar el presupuesto).",
        );
        ui.add_space(6.0);
        let was = c.auto_send;
        ui.add_enabled_ui(c.ready(), |ui| {
            ui.checkbox(&mut c.auto_send, RichText::new("Enviar las comunicaciones por correo automáticamente cada día").font(sbf(14.0)));
        });
        if c.auto_send && !was && c.auto_station.trim().is_empty() {
            c.auto_station = me.clone();
        }
        if !c.ready() {
            ui.label(RichText::new("Primero configure y active el correo de arriba.").size(12.5).color(MUTED()));
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Entre las");
            ui.add(egui::DragValue::new(&mut c.auto_hour).range(0..=22).suffix(":00"));
            ui.label("y las");
            ui.add(egui::DragValue::new(&mut c.auto_until).range(1..=24).suffix(":00"));
            ui.label(RichText::new("(si el ordenador está apagado, se envía al encenderlo; de noche, a la mañana siguiente)").size(12.5).color(MUTED()));
        });
        ui.horizontal_wrapped(|ui| {
            if c.auto_station.trim().is_empty() || c.auto_station == me {
                ui.label(RichText::new("Lo envía este ordenador.").size(13.0).color(TEXT));
            } else {
                ui.label(RichText::new(format!("Lo envía el ordenador «{}».", c.auto_station)).size(13.0).color(TEXT));
                if ui.button("Enviar desde este ordenador").clicked() {
                    c.auto_station = me.clone();
                }
            }
            if let Some(d) = c.last_auto {
                ui.label(RichText::new(format!("· Último envío automático: {}", fmt_date(d))).size(12.5).color(MUTED()));
            }
        });
        ui.add_space(4.0);
        if by_mail.is_empty() {
            ui.label(RichText::new("No hay plantillas activas por correo: active o cree alguna en Comunicaciones › Plantillas de mensajes.").size(12.5).color(WARN));
        } else {
            ui.label(RichText::new(format!("Plantillas por correo: {}.", by_mail.join(" · "))).size(12.5).color(MUTED()));
        }
        ui.horizontal(|ui| {
            if sending {
                ui.add(egui::Spinner::new().size(16.0));
                ui.label(RichText::new("Enviando…").color(MUTED()));
            } else if ui.add_enabled(c.ready(), egui::Button::new("Hacer ahora el envío de hoy")).on_hover_text("Prepara los mensajes de hoy y envía ya los de correo").clicked() {
                run_now = true;
            }
            if ui.button("Ver las plantillas").clicked() {
                goto_templates = true;
            }
        });
    });
    if goto_templates {
        app.screen = crate::app::Screen::Comunicaciones;
        app.comms.tab = 1;
    }
    run_now
}

fn small_label(ui: &mut Ui, t: &str) {
    ui.label(RichText::new(t).font(mdf(12.8)).color(MUTED()));
}

pub fn panel(app: &mut App, ui: &mut Ui) {
    let before = serde_json::to_string(&app.db.mail).unwrap_or_default();
    let clinic_mail = app.db.config.email.clone();
    let clinic_name = app.db.config.name.clone();
    let mut send_test = None;
    ui.columns(2, |cols| {
        // ── servidor ──
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            let c = &mut app.db.mail;
            ui.horizontal(|ui| {
                section(ui, "Correo saliente (SMTP)");
                right_aligned(ui, |ui| {
                    if c.ready() {
                        pill(ui, "Activo", OK);
                    } else if c.enabled {
                        pill(ui, "Faltan datos", WARN);
                    } else {
                        pill(ui, "Desactivado", MUTED());
                    }
                });
            });
            muted(ui, "Con el correo configurado, las órdenes de trabajo salen del programa directamente, con los escaneados y radiografías adjuntos.");
            ui.add_space(6.0);
            ui.checkbox(&mut c.enabled, RichText::new("Enviar correos desde VeodarDental").font(sbf(14.0)));
            ui.add_space(6.0);
            small_label(ui, "Proveedor (rellena el servidor y el puerto)");
            ui.horizontal_wrapped(|ui| {
                for (name, host, port, sec) in PRESETS {
                    let sel = c.host.eq_ignore_ascii_case(host);
                    let b = egui::Button::new(RichText::new(name).size(12.8).color(if sel { ACCENT_DARK } else { TEXT }))
                        .fill(if sel { ACCENT_SOFT } else { CARD })
                        .stroke(Stroke::new(1.0, if sel { ACCENT } else { BORDER() }))
                        .corner_radius(CornerRadius::same(10));
                    if ui.add(b).clicked() {
                        c.host = host.into();
                        c.port = port;
                        c.security = sec;
                    }
                }
            });
            ui.add_space(6.0);
            let w = (ui.available_width() / 1.1 - 4.0).min(440.0);
            field(ui, "Servidor SMTP", &mut c.host, w);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    small_label(ui, "Seguridad");
                    crate::theme::combo("mail_sec").width((w * 0.62).max(180.0)).selected_text(c.security.label()).show_ui(ui, |ui| {
                        for s in Security::ALL {
                            if ui.selectable_value(&mut c.security, s, s.label()).clicked() {
                                c.port = s.default_port();
                            }
                        }
                    });
                });
                ui.vertical(|ui| {
                    small_label(ui, "Puerto");
                    ui.add(egui::DragValue::new(&mut c.port).range(1..=65535));
                });
            });
            ui.add_space(2.0);
            field(ui, "Usuario (normalmente la dirección de correo)", &mut c.user, w);
            ui.vertical(|ui| {
                small_label(ui, "Contraseña o contraseña de aplicación");
                ui.add_space(-3.0);
                ui.horizontal(|ui| {
                    ui.add_sized([w * 1.1 - 90.0, 36.0], egui::TextEdit::singleline(&mut c.password).password(!app.mail_ui.show_password).margin(egui::vec2(10.0, 8.0)));
                    let t = if app.mail_ui.show_password { "Ocultar" } else { "Mostrar" };
                    if ui.button(t).clicked() {
                        app.mail_ui.show_password = !app.mail_ui.show_password;
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                field(ui, "Nombre del remitente", &mut c.from_name, (w / 2.0 - 10.0).max(150.0));
                field(ui, "Dirección del remitente", &mut c.from_email, (w / 2.0 - 10.0).max(150.0));
            });
            if c.from_name.is_empty() {
                c.from_name = clinic_name.clone();
            }
            ui.add_space(4.0);
            ui.checkbox(&mut c.bcc_self, "Enviarme una copia oculta de cada correo");
            if c.security == Security::Ninguna {
                ui.label(RichText::new("Sin cifrar, la contraseña y los datos viajan legibles: úselo sólo con un servidor dentro de la clínica.").size(12.5).color(DANGER));
            }
        });
        // ── prueba y consejos ──
        let ui = &mut cols[1];
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Correo de prueba");
            muted(ui, "Envía un correo corto para comprobar que el servidor, el usuario y la contraseña son correctos.");
            ui.add_space(6.0);
            let to = app.mail_ui.test_to.get_or_insert_with(|| clinic_mail.clone());
            let w = (ui.available_width() / 1.1 - 4.0).min(380.0);
            field(ui, "Enviar a", to, w);
            ui.add_space(6.0);
            let ok = app.db.mail.ready();
            if app.mail_ui.test_job.is_some() {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(18.0));
                    ui.label(RichText::new("Enviando…").color(MUTED()));
                });
            } else if ui.add_enabled(ok, egui::Button::new(RichText::new("Enviar correo de prueba").color(egui::Color32::WHITE).font(sbf(14.5))).fill(ACCENT).corner_radius(CornerRadius::same(10)).min_size(egui::vec2(0.0, 38.0))).clicked() {
                send_test = Some(app.mail_ui.test_to.clone().unwrap_or_default());
            }
            if !ok {
                ui.label(RichText::new("Active el envío e indique el servidor y la dirección del remitente.").size(12.5).color(MUTED()));
            }
            match &app.mail_ui.test_result {
                Some(Ok(m)) => {
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("✔ {m}")).size(13.0).color(OK));
                }
                Some(Err(e)) => {
                    ui.add_space(4.0);
                    ui.label(RichText::new(e).size(13.0).color(DANGER));
                }
                None => {}
            }
        });
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Antes de empezar");
            for (t, d) in [
                ("Gmail y Outlook", "Piden una «contraseña de aplicación» (no la contraseña de siempre): se crea en la seguridad de la cuenta, con la verificación en dos pasos activada."),
                ("Correo del dominio de la clínica", "Su proveedor de alojamiento le da el servidor SMTP, el puerto y si usa STARTTLS o SSL/TLS."),
                ("Protección de datos", "Las órdenes llevan datos de salud: use un correo profesional, envíe a la dirección que el laboratorio le haya dado y, si lo prefiere, marque «sin el nombre del paciente» en la orden."),
            ] {
                ui.add_space(4.0);
                ui.label(RichText::new(t).font(sbf(13.5)).color(HEADING));
                ui.label(RichText::new(d).font(mdf(12.8)).color(TEXT));
            }
        });
    });
    ui.add_space(8.0);
    let run_now = auto_card(app, ui);
    if run_now {
        crate::screens::comunicaciones::run_daily(app);
    }
    if let Some(to) = send_test {
        let cfg: MailConfig = app.db.mail.clone();
        let name = app.db.config.name.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let text = format!("Este es un correo de prueba de VeodarDental.\n\nSi lo recibe, el envío de correo de {name} está bien configurado: las órdenes de trabajo para el laboratorio se pueden enviar desde el programa con sus adjuntos.");
            let _ = tx.send(crate::mail::send(&cfg, &to, "Correo de prueba de VeodarDental", &text, &[]));
        });
        app.mail_ui.test_job = Some(rx);
        app.mail_ui.test_result = None;
        ui.ctx().request_repaint();
    }
    if serde_json::to_string(&app.db.mail).unwrap_or_default() != before {
        app.changed();
    }
}
