//! WhatsApp automático: funcionamiento en segundo plano, ajustes
//! (Configuración), registro de confirmaciones (Comunicaciones) y marcas en
//! la agenda.

use crate::app::{App, PatTab};
use crate::model::*;
use crate::theme::*;
use crate::whatsapp::*;
use egui::{Color32, RichText, Ui};
use serde_json::Value;
use std::sync::mpsc::{Receiver, Sender};

pub const WA_GREEN: Color32 = Color32::from_rgb(37, 160, 90);

pub struct WaUi {
    /// Ajustes guardados en este ordenador.
    pub saved: WaSettings,
    /// Ajustes que se están editando.
    pub form: WaSettings,
    pub hook: Option<Hook>,
    pub hook_error: Option<String>,
    pub jobs: Option<Sender<(WaSettings, JobKind, Value)>>,
    pub results: Option<Receiver<JobResult>>,
    pub last_check: f64,
    pub test_to: String,
    pub test_result: Option<Result<String, String>>,
    pub show_secrets: bool,
    pub scroll_here: bool,
    pub check_result: Option<Result<String, String>>,
    /// Si el receptor de avisos escucha en toda la red local (se fija al arrancar).
    pub bound_lan: Option<bool>,
    sim: u32,
}

impl WaUi {
    /// `load`: leer los ajustes de este ordenador (no en capturas ni pruebas).
    pub fn new(load: bool) -> Self {
        let saved = if load { load_settings() } else { WaSettings::default() };
        WaUi { form: saved.clone(), saved, hook: None, hook_error: None, jobs: None, results: None, last_check: -1e9, test_to: String::new(), test_result: None, show_secrets: false, scroll_here: false, check_result: None, bound_lan: None, sim: 0 }
    }
}

fn me() -> String {
    crate::shared::machine_name()
}

/// Este ordenador es el que envía los WhatsApp automáticos.
pub fn is_sender(app: &App) -> bool {
    app.wa.saved.enabled && app.db.wa_sender == me()
}

fn ensure_worker(app: &mut App, ctx: &egui::Context) {
    if app.wa.jobs.is_none() {
        let c = ctx.clone();
        let (j, r) = start_worker(move || c.request_repaint());
        app.wa.jobs = Some(j);
        app.wa.results = Some(r);
    }
}

fn ensure_running(app: &mut App, ctx: &egui::Context) {
    ensure_worker(app, ctx);
    let (vt, sec) = (app.wa.saved.verify_token.clone(), app.wa.saved.app_secret.clone());
    match &app.wa.hook {
        Some(h) => {
            if let Ok(mut g) = h.cfg.lock() {
                g.verify_token = vt;
                g.app_secret = sec;
                g.accepting = true;
            }
        }
        None => {
            let c = ctx.clone();
            let lan = app.wa.saved.provider == Provider::Pasarela && app.wa.saved.gw_lan;
            match listen(app.wa.saved.port, lan, HookCfg { verify_token: vt, app_secret: sec, accepting: true, ..Default::default() }, move || c.request_repaint()) {
                Ok(h) => {
                    app.wa.bound_lan = Some(lan);
                    app.wa.hook = Some(h);
                    app.wa.hook_error = None;
                }
                Err(e) => app.wa.hook_error = Some(e),
            }
        }
    }
}

/// Al abrir el programa en el ordenador que envía.
pub fn start(app: &mut App, ctx: &egui::Context) {
    if !is_sender(app) || !app.persist {
        return;
    }
    if app.db.wa_recover_interrupted() > 0 {
        app.changed();
    }
    ensure_running(app, ctx);
}

/// Trabajo en segundo plano (cada fotograma; los recordatorios, cada minuto).
pub fn tick(app: &mut App, time: f64) {
    let mut changed = false;
    let mut results = vec![];
    if let Some(r) = &app.wa.results {
        while let Ok(x) = r.try_recv() {
            results.push(x);
        }
    }
    for r in results {
        match r.kind {
            JobKind::Reminder(log) => {
                app.db.wa_sent(log, r.result, app.today);
                app.db.wa_set_user(log, &r.extra);
                changed = true;
            }
            JobKind::Check => app.wa.check_result = Some(r.result),
            JobKind::Reply(log) => {
                if let Err(e) = r.result {
                    let who = app.db.whatsapp.iter().find(|m| m.id == log).map(|m| app.db.patient_name(m.patient_id)).unwrap_or_default();
                    app.warn(format!("No se pudo enviar la respuesta automática por WhatsApp a {who}: {e}"));
                }
            }
            JobKind::Test => app.wa.test_result = Some(r.result),
        }
    }
    let sender = is_sender(app) && app.persist;
    if let Some(h) = &app.wa.hook {
        // si otro ordenador ha pasado a enviar, este deja de aceptar avisos (Meta los reintenta allí)
        if let Ok(mut g) = h.cfg.lock() {
            g.accepting = sender;
        }
    }
    if sender {
        let s = app.wa.saved.clone();
        let mut events = vec![];
        if let Some(h) = &app.wa.hook {
            while let Ok(e) = h.rx.try_recv() {
                events.push(e);
            }
        }
        for e in events {
            let replies = app.db.wa_apply(&s, &e, now());
            changed = true;
            if let Some(j) = &app.wa.jobs {
                for (log, to, text) in replies {
                    let _ = j.send((s.clone(), JobKind::Reply(log), text_request(&to, &text)));
                }
            }
        }
        if time - app.wa.last_check >= 60.0 {
            app.wa.last_check = time;
            if let Some(j) = app.wa.jobs.clone() {
                for id in app.db.wa_due(&s, now()).into_iter().take(20) {
                    if let Some((log, body)) = app.db.wa_prepare(&s, id, crate::model::now()) {
                        let _ = j.send((s.clone(), JobKind::Reminder(log), body));
                        changed = true;
                    }
                }
            }
        }
    }
    if changed {
        app.changed();
    }
}

// ───────────────────────────── agenda ─────────────────────────────

/// Marca para la esquina del bloque de la cita en la agenda.
pub fn appt_badge(db: &Db, appt_id: u32) -> Option<(&'static str, Color32)> {
    Some(match db.wa_for_appt(appt_id)?.state {
        WaState::Enviando => ("💬…", MUTED),
        WaState::Enviado | WaState::Entregado => ("💬", MUTED),
        WaState::Leido => ("💬 leído", INFO),
        WaState::Confirmada => ("💬 ✔", OK),
        WaState::NoViene => ("💬 ✖", DANGER),
        WaState::Respuesta => ("💬 ?", WARN),
        WaState::Fallido => ("💬 ⚠", DANGER),
    })
}

/// Línea descriptiva para la ficha de la cita.
pub fn appt_line(db: &Db, appt_id: u32) -> Option<String> {
    let m = db.wa_for_appt(appt_id)?;
    let sent = m.sent_at.map(|d| format!(" · enviado {}", d.format("%d/%m %H:%M"))).unwrap_or_default();
    let reply = if m.reply.is_empty() { String::new() } else { format!(" · respuesta «{}» {}", m.reply, m.reply_at.map(|d| d.format("%H:%M").to_string()).unwrap_or_default()) };
    let err = if m.error.is_empty() { String::new() } else { format!(" · {}", m.error) };
    Some(format!("💬 WhatsApp: {}{sent}{reply}{err}", m.state.label()))
}

pub fn state_color(s: WaState) -> Color32 {
    match s {
        WaState::Confirmada => OK,
        WaState::NoViene | WaState::Fallido => DANGER,
        WaState::Respuesta => WARN,
        WaState::Leido => INFO,
        _ => MUTED,
    }
}

// ───────────────────────────── Configuración ─────────────────────────────

fn copy_row(app: &mut App, ui: &mut Ui, label: &str, text: &str) {
    ui.horizontal(|ui| {
        lab(ui, 210.0, RichText::new(label).color(MUTED));
        let w = (ui.available_width() - 100.0).max(200.0);
        lab(ui, w, RichText::new(text).family(sb())).on_hover_text(text);
        if ui.small_button("Copiar").clicked() {
            ui.ctx().copy_text(text.to_string());
            app.notify("Copiado.");
        }
    });
}

fn secret(ui: &mut Ui, label: &str, value: &mut String, show: bool, width: f32) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(13.0).color(MUTED));
        ui.add_sized([width, 34.0], egui::TextEdit::singleline(value).password(!show));
    });
}

fn meta_form(app: &mut App, ui: &mut Ui, sender_empty: bool) {
    egui::CollapsingHeader::new("Qué hace falta (se prepara una sola vez)").id_salt("wa_guide").default_open(!is_sender(app) && sender_empty).show(ui, |ui| {
        let steps = [
            "Cree la cuenta de WhatsApp Business en Meta (business.facebook.com) con un número de teléfono de la clínica que no se use en la aplicación normal de WhatsApp, y añada un método de pago.",
            "En el Administrador de WhatsApp cree una plantilla llamada «recordatorio_cita», categoría «Utilidad», idioma Español, con el texto de abajo y dos botones de respuesta rápida: «Sí, confirmo» y «No puedo ir». Espere a que Meta la apruebe.",
            "En la app de Meta para desarrolladores copie el identificador del número de teléfono, cree un token de acceso permanente (usuario del sistema) y copie la clave secreta de la app. Péguelos abajo.",
            "Para recibir las respuestas, publique este ordenador con un túnel https (por ejemplo Cloudflare Tunnel, gratuito) que apunte a la dirección local de abajo, y regístrela en Meta (Webhooks, campo «messages») con el token de verificación de abajo.",
            "Pulse «Guardar y enviar desde este ordenador» y envíe una prueba a su móvil.",
        ];
        for (i, t) in steps.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{}.", i + 1)).family(sb()));
                ui.label(*t);
            });
        }
        ui.add_space(4.0);
        copy_row(app, ui, "Texto de la plantilla", TEMPLATE_TEXT);
        copy_row(app, ui, "Botones", &format!("{BUTTON_YES} · {BUTTON_NO}"));
    });
    ui.add_space(6.0);
    let show = app.wa.show_secrets;
    {
        let f = &mut app.wa.form;
        ui.horizontal_wrapped(|ui| {
            field(ui, "Identificador del número (Phone number ID)", &mut f.phone_number_id, 260.0);
            field(ui, "Nombre de la plantilla", &mut f.template, 200.0);
            field(ui, "Idioma", &mut f.language, 70.0);
        });
        ui.horizontal_wrapped(|ui| {
            secret(ui, "Token de acceso permanente", &mut f.token, show, 380.0);
            secret(ui, "Clave secreta de la app", &mut f.app_secret, show, 260.0);
        });
    }
    ui.checkbox(&mut app.wa.show_secrets, "Mostrar las claves (se guardan solo en este ordenador)");
    let port = app.wa.form.port;
    copy_row(app, ui, "Dirección local para el túnel", &format!("http://127.0.0.1:{port}/whatsapp"));
    let vt = app.wa.form.verify_token.clone();
    copy_row(app, ui, "Token de verificación", &vt);
}

fn gateway_form(app: &mut App, ui: &mut Ui) {
    egui::Frame::new().fill(tint(WARN, 0.12)).corner_radius(egui::CornerRadius::same(10)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("⚠ Gratis, pero con riesgo de perder el número").family(sb()).color(WARN));
        ui.label("Funciona como WhatsApp Web: un programa gratuito (la pasarela) se enlaza con el WhatsApp de la clínica escaneando un código QR, y VeodarDental envía y lee los mensajes a través de él. WhatsApp no permite este uso y puede bloquear el número sin avisar, sobre todo si se envían muchos mensajes seguidos o algún paciente lo marca como spam; si se bloquea, los pacientes no podrán escribir a la clínica por WhatsApp. VeodarDental envía despacio (pausa al azar entre mensajes y un máximo al día) para reducir el riesgo, pero no lo elimina. La opción oficial de Meta no tiene este riesgo.");
        ui.checkbox(&mut app.wa.form.gw_risk_ok, "Entiendo el riesgo y quiero usar la pasarela gratuita");
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Pasarela:");
        for k in [GwKind::Waha, GwKind::Evolution] {
            let f = &mut app.wa.form;
            if ui.selectable_label(f.gw_kind == k, k.label()).clicked() && f.gw_kind != k {
                if f.gw_url == f.gw_kind.default_url() {
                    f.gw_url = k.default_url().into();
                }
                if k == GwKind::Waha && f.gw_session == "clinica" {
                    f.gw_session = "default".into();
                }
                if k == GwKind::Evolution && f.gw_session == "default" {
                    f.gw_session = "clinica".into();
                }
                f.gw_kind = k;
            }
        }
        muted(ui, "las dos son gratuitas y de código abierto");
    });
    if app.wa.form.gw_key.is_empty() {
        app.wa.form.gw_key = crate::server::new_key();
    }
    let token = app.wa.form.verify_token.clone();
    let key = app.wa.form.gw_key.clone();
    let port = app.wa.form.port;
    let hook_docker = format!("http://host.docker.internal:{port}/pasarela?clave={token}");
    let hook_lan = format!("http://{}:{port}/pasarela?clave={token}", crate::server::local_ip().unwrap_or_else(|| "IP-de-este-ordenador".into()));
    let kind = app.wa.form.gw_kind;
    egui::CollapsingHeader::new("Cómo se instala (una sola vez)").id_salt("gw_guide").default_open(!is_sender(app)).show(ui, |ui| {
        let steps: Vec<String> = match kind {
            GwKind::Waha => vec![
                "Instale Docker Desktop (gratuito) en el ordenador de recepción.".into(),
                "Abra una ventana de comandos y arranque la pasarela con la orden de abajo (cópiela tal cual; ya lleva la clave y la dirección de avisos).".into(),
                format!("Abra http://127.0.0.1:3000/dashboard (usuario «clinica», contraseña {key}), inicie la sesión «default» y escanee el código QR con el WhatsApp de la clínica (en el móvil: Ajustes › Dispositivos vinculados)."),
                "Pulse «Comprobar conexión», marque la casilla del riesgo y pulse «Guardar y enviar desde este ordenador».".into(),
            ],
            GwKind::Evolution => vec![
                "Instale Evolution API (gratuita) con Docker en este ordenador o en el servidor.".into(),
                "Cree una instancia (por ejemplo «clinica») y conéctela escaneando el código QR con el WhatsApp de la clínica.".into(),
                "En la instancia active el webhook con la dirección de avisos de abajo y los eventos MESSAGES_UPSERT y MESSAGES_UPDATE.".into(),
                "Copie aquí la dirección, la clave (apikey) y el nombre de la instancia; pulse «Comprobar conexión» y «Guardar y enviar desde este ordenador».".into(),
            ],
        };
        for (i, t) in steps.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{}.", i + 1)).family(sb()));
                ui.label(t.as_str());
            });
        }
        ui.add_space(4.0);
        if kind == GwKind::Waha {
            let cmd = format!(
                "docker run -d --name waha --restart unless-stopped -p 3000:3000 --add-host=host.docker.internal:host-gateway -e \"WAHA_API_KEY={key}\" -e WAHA_DASHBOARD_USERNAME=clinica -e \"WAHA_DASHBOARD_PASSWORD={key}\" -e \"WHATSAPP_HOOK_URL={hook_docker}\" -e WHATSAPP_HOOK_EVENTS=message,message.ack devlikeapro/waha"
            );
            copy_row(app, ui, "Orden para arrancarla", &cmd);
        }
        copy_row(app, ui, "Avisos (pasarela en Docker)", &hook_docker);
        copy_row(app, ui, "Avisos (en otro ordenador)", &hook_lan);
    });
    ui.add_space(6.0);
    let show = app.wa.show_secrets;
    {
        let f = &mut app.wa.form;
        ui.horizontal_wrapped(|ui| {
            field(ui, "Dirección de la pasarela", &mut f.gw_url, 260.0);
            secret(ui, "Clave de la pasarela", &mut f.gw_key, show, 220.0);
            let label = if f.gw_kind == GwKind::Waha { "Sesión" } else { "Instancia" };
            field(ui, label, &mut f.gw_session, 140.0);
        });
    }
    ui.checkbox(&mut app.wa.show_secrets, "Mostrar las claves (se guardan solo en este ordenador)");
    ui.label(RichText::new("Texto del recordatorio (puede usar [NOMBRE], [CLINICA], [FECHA], [HORA], [DOCTOR] y [ENLACE], el enlace para confirmar, cambiar o anular la cita)").size(13.0).color(MUTED));
    ui.add(egui::TextEdit::multiline(&mut app.wa.form.gw_text).desired_rows(2).desired_width(f32::INFINITY));
    let f = &mut app.wa.form;
    ui.horizontal_wrapped(|ui| {
        ui.label("Pausa entre mensajes: de");
        ui.add(egui::DragValue::new(&mut f.gw_min_pause).range(0..=600).suffix(" s"));
        ui.label("a");
        ui.add(egui::DragValue::new(&mut f.gw_max_pause).range(0..=900).suffix(" s"));
        ui.add_space(12.0);
        ui.label("Máximo al día:");
        ui.add(egui::DragValue::new(&mut f.gw_daily_max).range(1..=1000).suffix(" mensajes"));
    });
    ui.checkbox(&mut f.gw_lan, "La pasarela está en Docker o en otro ordenador (recibir sus avisos desde la red local)");
}

pub fn panel(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        if app.wa.scroll_here {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
            app.wa.scroll_here = false;
        }
        section(ui, "WhatsApp automático: confirmación de citas");
        let sender = app.db.wa_sender.clone();
        let saved_gw = app.wa.saved.provider == Provider::Pasarela;
        if is_sender(app) {
            let via = if saved_gw { format!("por la pasarela gratuita {}", app.wa.saved.gw_kind.label()) } else { "por la plataforma oficial de Meta".to_string() };
            let hook = match (&app.wa.hook, &app.wa.hook_error) {
                (Some(h), _) => format!("recibe las respuestas en el puerto local {}", h.port),
                (None, Some(e)) => format!("⚠ NO recibe respuestas: {e}"),
                _ => "se está poniendo en marcha".into(),
            };
            ui.label(RichText::new(format!("✔ Activo: este ordenador envía los recordatorios {via} y {hook}.")).color(OK));
            let want_lan = saved_gw && app.wa.saved.gw_lan;
            if app.wa.bound_lan.is_some_and(|b| b != want_lan) {
                ui.label(RichText::new("Cierre y vuelva a abrir VeodarDental para aplicar el cambio en la recepción de avisos.").color(WARN));
            }
        } else if !sender.is_empty() {
            ui.label(RichText::new(format!("Activo en el ordenador «{sender}»: desde allí se envían los recordatorios y se leen las respuestas.")).color(INFO));
        } else {
            ui.label(RichText::new("Desactivado: los recordatorios de WhatsApp se envían a mano desde Comunicaciones.").color(MUTED));
        }
        muted(ui, "El día laborable anterior a cada cita, el programa envía el recordatorio y lee la respuesta: si confirma, la cita pasa a Confirmada; si no puede, se anula, se libera el hueco y se crea una tarea para darle otra fecha.");
        ui.add_space(6.0);
        if app.wa.form.verify_token.is_empty() {
            app.wa.form.verify_token = crate::server::new_key().to_lowercase();
        }
        ui.label(RichText::new("Forma de conexión").family(sb()));
        let cents = format!("{:.2}", META_EUR_PER_MSG * 100.0).replace('.', ",");
        let meta_txt = format!("Plataforma oficial de Meta: {cents} céntimos por recordatorio entregado (unos {:.0} € al mes con 400 citas), sin riesgo de bloqueo", META_EUR_PER_MSG * 400.0);
        ui.radio_value(&mut app.wa.form.provider, Provider::Meta, meta_txt);
        ui.radio_value(&mut app.wa.form.provider, Provider::Pasarela, "WhatsApp normal de la clínica con una pasarela gratuita (no oficial, con riesgo de bloqueo)");
        ui.add_space(6.0);
        match app.wa.form.provider {
            Provider::Meta => meta_form(app, ui, sender.is_empty()),
            Provider::Pasarela => gateway_form(app, ui),
        }
        ui.add_space(4.0);
        {
            let f = &mut app.wa.form;
            ui.horizontal_wrapped(|ui| {
                ui.label("Enviar");
                ui.add(egui::DragValue::new(&mut f.days_before).range(1..=3));
                ui.label("día(s) laborable(s) antes de la cita, entre las");
                ui.add(egui::DragValue::new(&mut f.from_hour).range(6..=22).suffix(" h"));
                ui.label("y las");
                ui.add(egui::DragValue::new(&mut f.until_hour).range(7..=23).suffix(" h"));
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut f.auto_reply, "Contestar «gracias» automáticamente");
                ui.checkbox(&mut f.cancel_on_no, "Si dice que no, anular la cita y liberar el hueco");
            });
        }
        ui.add_space(6.0);
        let me = me();
        ui.horizontal_wrapped(|ui| {
            let label = if !sender.is_empty() && sender != me { "Pasar el envío a este ordenador" } else { "Guardar y enviar desde este ordenador" };
            if primary(ui, label).clicked() {
                let missing = app.wa.form.missing();
                if !missing.is_empty() {
                    app.warn(format!("Falta: {}.", missing.join(", ")));
                } else {
                    let mut s = app.wa.form.clone();
                    s.enabled = true;
                    match save_settings(&s) {
                        Ok(()) => {
                            app.wa.saved = s.clone();
                            app.wa.form = s;
                            app.db.wa_sender = me.clone();
                            app.db.audit("WhatsApp automático activado", &me);
                            ensure_running(app, ui.ctx());
                            app.wa.last_check = -1e9;
                            app.changed();
                            app.notify("WhatsApp automático activado en este ordenador.");
                        }
                        Err(e) => app.warn(format!("No se pudieron guardar los ajustes: {e}")),
                    }
                }
            }
            if (app.wa.saved.enabled || sender == me) && ui.button("Desactivar").clicked() {
                let mut s = app.wa.saved.clone();
                s.enabled = false;
                let _ = save_settings(&s);
                app.wa.saved = s;
                if app.db.wa_sender == me {
                    app.db.wa_sender.clear();
                }
                if let Some(h) = &app.wa.hook && let Ok(mut g) = h.cfg.lock() {
                    g.accepting = false;
                }
                app.db.audit("WhatsApp automático desactivado", &me);
                app.changed();
                app.notify("WhatsApp automático desactivado.");
            }
            if ui.button("Comprobar conexión").clicked() {
                ensure_worker(app, ui.ctx());
                if let Some(j) = &app.wa.jobs {
                    let _ = j.send((app.wa.form.clone(), JobKind::Check, Value::Null));
                }
                app.wa.check_result = None;
            }
            match &app.wa.check_result {
                Some(Ok(t)) => {
                    ui.label(RichText::new(format!("✔ {t}")).color(OK));
                }
                Some(Err(e)) => {
                    ui.add(egui::Label::new(RichText::new(format!("⚠ {e}")).color(DANGER)).truncate());
                }
                None => {}
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Enviar una prueba a:");
            ui.add_sized([180.0, 32.0], egui::TextEdit::singleline(&mut app.wa.test_to).hint_text("Su móvil"));
            if ui.button("Enviar prueba").clicked() {
                let f = app.wa.form.clone();
                let missing: Vec<&str> = f.missing().into_iter().filter(|m| *m != "clave secreta de la app").collect();
                match intl(&app.wa.test_to) {
                    _ if !missing.is_empty() => app.warn(format!("Falta: {}.", missing.join(", "))),
                    None => app.warn("Escriba un número de móvil válido."),
                    Some(to) => {
                        ensure_worker(app, ui.ctx());
                        let fecha = date_text(app.today + chrono::Duration::days(1));
                        let body = match f.provider {
                            Provider::Meta => template_request(&f, &to, "Prueba", &app.db.config.name, &fecha, "10:00"),
                            Provider::Pasarela => {
                                let text = f.gw_text.replace("[NOMBRE]", "Prueba").replace("[CLINICA]", &app.db.config.name).replace("[FECHA]", &fecha).replace("[HORA]", "10:00").replace("[DOCTOR]", "");
                                text_request(&to, &text)
                            }
                        };
                        if let Some(j) = &app.wa.jobs {
                            let _ = j.send((f, JobKind::Test, body));
                        }
                        app.wa.test_result = None;
                        app.notify("Enviando prueba…");
                    }
                }
            }
            match &app.wa.test_result {
                Some(Ok(_)) => {
                    ui.label(RichText::new("✔ Enviada: revise el móvil y conteste para probar la respuesta").color(OK));
                }
                Some(Err(e)) => {
                    ui.add(egui::Label::new(RichText::new(format!("⚠ {e}")).color(DANGER)).truncate());
                }
                None => {}
            }
        });
    });
}

// ───────────────────────────── Comunicaciones › Confirmaciones ─────────────────────────────

pub fn log_tab(app: &mut App, ui: &mut Ui) {
    let today = app.today;
    let since = now() - chrono::Duration::days(7);
    let list: Vec<WaMessage> = app.db.whatsapp.to_vec();
    let n = |f: &dyn Fn(&WaMessage) -> bool| list.iter().filter(|m| f(m)).count();
    let sent = n(&|m| m.sent_at.is_some_and(|d| d >= since));
    let waiting = n(&|m| m.state.waiting());
    let yes = n(&|m| m.state == WaState::Confirmada);
    let no = n(&|m| m.state == WaState::NoViene);
    let review = n(&|m| matches!(m.state, WaState::Respuesta | WaState::Fallido));
    let w = kpi_w(ui, 5);
    ui.horizontal(|ui| {
        kpi(ui, w, "Enviados (7 días)", &sent.to_string(), "recordatorios", WA_GREEN);
        ui.add_space(4.0);
        kpi(ui, w, "Esperando respuesta", &waiting.to_string(), "aún no contestan", INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Confirmaron", &yes.to_string(), "cita confirmada", OK);
        ui.add_space(4.0);
        kpi(ui, w, "No pueden venir", &no.to_string(), "hueco liberado", DANGER);
        ui.add_space(4.0);
        kpi(ui, w, "Para revisar", &review.to_string(), "respuestas y fallos", WARN);
    });
    ui.add_space(8.0);
    let live = is_sender(app);
    let auto = app.db.wa_auto_on();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Confirmaciones por WhatsApp");
            right_aligned(ui, |ui| {
                if app.allowed(crate::app::Screen::Config) && ui.link("⚙ Configurar WhatsApp automático").clicked() {
                    app.screen = crate::app::Screen::Config;
                    app.wa.scroll_here = true;
                }
            });
        });
        if !auto {
            muted(ui, "El envío automático está desactivado. Estos registros son de ejemplo: puede simular la respuesta del paciente para ver qué hace el programa.");
        }
        let mut rows: Vec<(WaMessage, Option<Appointment>)> = list.iter().map(|m| (m.clone(), app.db.appointments.iter().find(|a| a.id == m.appt_id).cloned())).collect();
        rows.retain(|(_, a)| a.as_ref().is_some_and(|a| a.date >= today - chrono::Duration::days(7)));
        rows.sort_by_key(|(m, a)| (a.as_ref().map(|a| (a.date, a.start)), m.id));
        if rows.is_empty() {
            empty_state(ui, "Todavía no se ha enviado ningún recordatorio por WhatsApp.");
            return;
        }
        let tbl = Table::new("walog", vec![("Cita", 150.0), ("Paciente", 210.0), ("Estado", 170.0), ("Respuesta", 240.0), ("Enviado", 110.0), ("", 250.0)]);
        tbl.header(ui);
        let s = tbl.scale(ui) * 0.98;
        let mut act: Option<(u32, Option<bool>)> = None;
        let mut open = None;
        let mut retry = None;
        for (m, a) in &rows {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(12.0);
                let when = a.as_ref().map(|a| format!("{} {}", fmt_date(a.date), a.start.format("%H:%M"))).unwrap_or_default();
                lab(ui, 150.0 * s, when);
                if lab(ui, 210.0 * s, RichText::new(app.db.patient_name(m.patient_id)).color(ACCENT)).interact(egui::Sense::click()).on_hover_text("Abrir ficha").clicked() {
                    open = Some(m.patient_id);
                }
                ui.scope(|ui| {
                    ui.set_width(170.0 * s);
                    pill(ui, m.state.label(), state_color(m.state)).on_hover_text(if m.error.is_empty() { m.to.clone() } else { m.error.clone() });
                });
                let reply = if m.reply.is_empty() { "—".to_string() } else { format!("«{}» {}", m.reply, m.reply_at.map(|d| d.format("%H:%M").to_string()).unwrap_or_default()) };
                lab(ui, 240.0 * s, reply);
                lab(ui, 110.0 * s, RichText::new(m.sent_at.map(|d| d.format("%d/%m %H:%M").to_string()).unwrap_or_default()).color(MUTED));
                ui.spacing_mut().item_spacing.x = 6.0;
                if !live && (m.state.waiting() || m.state == WaState::Respuesta) {
                    if ui.small_button("Simular «Sí»").on_hover_text("Ver qué pasa si el paciente confirma").clicked() {
                        act = Some((m.id, Some(true)));
                    }
                    if ui.small_button("Simular «No»").on_hover_text("Ver qué pasa si el paciente no puede venir").clicked() {
                        act = Some((m.id, Some(false)));
                    }
                }
                if live && m.state == WaState::Fallido && ui.small_button("Reintentar").clicked() {
                    retry = Some(m.id);
                }
            });
        }
        if let Some(pid) = open {
            app.open_patient(pid, PatTab::Citas);
        }
        if let Some(id) = retry {
            app.db.whatsapp.retain(|m| m.id != id);
            app.wa.last_check = -1e9;
            app.changed();
            app.notify("Se volverá a enviar en unos segundos si la cita sigue pendiente.");
        }
        if let Some((id, yes)) = act && let Some(m) = app.db.whatsapp.iter().find(|m| m.id == id).cloned() {
            app.wa.sim += 1;
            let text = if yes == Some(true) { BUTTON_YES } else { BUTTON_NO };
            let ev = WaEvent::Message { id: format!("sim{}-{}", id, app.wa.sim), from: String::new(), user_id: String::new(), name: String::new(), context: m.wamid.clone(), text: text.into(), button: true };
            let s = WaSettings { auto_reply: false, ..app.wa.saved.clone() };
            app.db.wa_apply(&s, &ev, now());
            app.changed();
            app.notify(if yes == Some(true) { "Simulado: el paciente confirma y la cita pasa a Confirmada." } else { "Simulado: el paciente no puede venir; la cita se anula y hay una tarea para darle otra." });
        }
    });
}
