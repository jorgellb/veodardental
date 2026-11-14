//! Comunicaciones automáticas (como el módulo de automatización del
//! original: SMS, email y WhatsApp) y plantillas de mensajes y documentos.

use crate::app::{App, PatTab};
use crate::comms::*;
use crate::model::*;
use crate::theme::*;
use egui::{Color32, CornerRadius, RichText, Ui};

pub struct CommsState {
    pub tab: usize,
    pub filter: Option<MsgState>,
    pub selected: Option<u32>,
    pub tpl_sel: Option<u32>,
    pub doc_tpl_sel: Option<u32>,
    pub last_skipped: usize,
    pub free_text: String,
    pub free_channel: Channel,
    /// Envío de correos en curso.
    pub outbox: Option<Outbox>,
    /// Correos que esperan a que termine el envío en curso.
    pub queued: Vec<u32>,
    /// Último envío de correos terminado: (cuándo, enviados, con error, automático).
    pub last_outbox: Option<(chrono::NaiveDateTime, usize, usize, bool)>,
    /// Pestaña de campañas.
    pub camp: crate::screens::campanas::CampUi,
}

impl Default for CommsState {
    fn default() -> Self {
        CommsState {
            tab: 0,
            filter: Some(MsgState::Pendiente),
            selected: None,
            tpl_sel: Some(1),
            doc_tpl_sel: Some(1),
            last_skipped: 0,
            free_text: String::new(),
            free_channel: Channel::WhatsApp,
            outbox: None,
            queued: vec![],
            last_outbox: None,
            camp: Default::default(),
        }
    }
}

/// Correos que se están enviando en segundo plano, uno tras otro.
pub struct Outbox {
    rx: std::sync::mpsc::Receiver<(u32, Result<(), crate::mail::SendError>)>,
    /// Marca con la que este puesto ha reservado los correos que envía.
    token: String,
    pub total: usize,
    pub done: usize,
    pub ok: usize,
    pub auto: bool,
}

/// Envía por correo (con el servidor de la clínica) los mensajes indicados.
pub fn start_outbox(app: &mut App, ids: Vec<u32>, auto: bool) {
    if ids.is_empty() || !app.db.mail.ready() {
        return;
    }
    if app.comms.outbox.is_some() {
        // ya hay un envío en marcha: estos salen en cuanto termine
        for id in ids {
            if !app.comms.queued.contains(&id) {
                app.comms.queued.push(id);
            }
        }
        return;
    }
    // primero se reservan (en el servidor, o con el fichero compartido
    // bloqueado): lo que otro puesto esté enviando no se envía otra vez
    let token = crate::portal::new_token();
    if let Err(e) = app.chain_op(crate::app::ChainOp::ClaimEmails { ids, token: token.clone() }) {
        app.warn(format!("No se han podido preparar los correos: {e}"));
        return;
    }
    let claimed = app.db.claimed_emails(&token);
    let jobs: Vec<(u32, String, String, String)> = claimed.iter().filter_map(|id| app.db.messages.iter().find(|m| m.id == *id).map(|m| (m.id, m.to.clone(), m.subject.clone(), m.text.clone()))).collect();
    if jobs.is_empty() {
        return;
    }
    let cfg = app.db.mail.clone();
    let total = jobs.len();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for (id, to, subject, text) in jobs {
            let r = crate::mail::send_tracked(&cfg, &to, &subject, &text, &[]);
            if tx.send((id, r)).is_err() {
                break;
            }
        }
    });
    app.comms.outbox = Some(Outbox { rx, token, total, done: 0, ok: 0, auto });
}

/// Hace ya el envío automático del día: genera los mensajes y manda los correos.
pub fn run_daily(app: &mut App) {
    app.db.mail.last_auto = Some(app.today);
    let (c, s) = app.db.generate_messages(app.today);
    app.comms.last_skipped = s;
    app.changed();
    let ids = app.db.pending_emails();
    if ids.is_empty() {
        if c > 0 {
            app.notify(format!("Comunicaciones del día: {c} mensajes nuevos para enviar por WhatsApp o SMS (ninguno por correo)."));
        }
        return;
    }
    start_outbox(app, ids, true);
}

/// Recoge los envíos de correo y, en el ordenador que lo tiene encargado, hace el envío automático del día.
pub fn outbox_tick(app: &mut App, ctx: &egui::Context) {
    let mut finished = false;
    if let Some(ob) = app.comms.outbox.as_mut() {
        let mut results = vec![];
        loop {
            match ob.rx.try_recv() {
                Ok(r) => results.push(r),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }
        for (id, r) in &results {
            ob.done += 1;
            if r.is_ok() {
                ob.ok += 1;
            }
            app.db.email_outcome(*id, &ob.token, r);
        }
        if !results.is_empty() {
            app.dirty = true;
        }
        finished |= ob.done >= ob.total;
        if !finished {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
    if finished {
        if let Some(ob) = app.comms.outbox.take() {
            let failed = ob.done - ob.ok;
            app.comms.last_outbox = Some((now(), ob.ok, failed, ob.auto));
            app.changed();
            let what = if ob.auto { "Envío automático de comunicaciones" } else { "Correos" };
            if failed == 0 {
                app.notify(format!("{what}: {} enviados por correo.", ob.ok));
            } else {
                app.warn(format!("{what}: {} enviados y {failed} con error. Véalos en Comunicaciones (los pendientes con error).", ob.ok));
            }
        }
        // los que esperaban a que terminara este envío
        let next = std::mem::take(&mut app.comms.queued);
        if !next.is_empty() {
            start_outbox(app, next, false);
        }
    }
    // envío automático del día
    let m = &app.db.mail;
    if m.auto_send && m.ready() && m.auto_station == crate::shared::machine_name() && app.comms.outbox.is_none() && m.last_auto != Some(app.today) {
        use chrono::Timelike;
        if m.auto_window(chrono::Local::now().hour()) {
            run_daily(app);
        }
    }
}

pub fn enc_pub(s: &str) -> String {
    enc(s)
}

fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(*b as char),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

pub fn phone_intl(p: &str) -> String {
    let d: String = p.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() == 9 { format!("34{d}") } else { d }
}

pub fn channel_color(c: Channel) -> Color32 {
    match c {
        Channel::WhatsApp => Color32::from_rgb(37, 160, 90),
        Channel::Sms => INFO,
        Channel::Email => Color32::from_rgb(140, 90, 200),
    }
}

/// Abre el canal correspondiente con el mensaje ya escrito.
pub fn send(app: &mut App, ctx: &egui::Context, m: &Message) {
    match m.channel {
        Channel::WhatsApp => {
            let url = format!("https://wa.me/{}?text={}", phone_intl(&m.to), enc(&m.text));
            ctx.open_url(egui::OpenUrl::new_tab(url));
            app.notify("WhatsApp abierto con el mensaje escrito. Pulse enviar en WhatsApp.");
        }
        Channel::Sms if app.sms.enabled() => {
            // con la pasarela configurada, sale desde el programa (en segundo plano)
            if app.sms_job.is_some() {
                app.warn("Ya se está enviando otro SMS: espere un momento.");
                return;
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let (cfg, id, to, text, ctx2) = (app.sms.clone(), m.id, m.to.clone(), m.text.clone(), ctx.clone());
            std::thread::spawn(move || {
                let r = crate::sms::send(&cfg, &to, &text, &format!("veodardental-{id}"));
                let _ = tx.send((id, r));
                ctx2.request_repaint();
            });
            app.sms_job = Some(rx);
            app.notify("Enviando el SMS…");
            return;
        }
        Channel::Email if app.db.mail.ready() => {
            // con el correo configurado, sale directamente desde el programa
            start_outbox(app, vec![m.id], false);
            return;
        }
        Channel::Email => {
            let url = format!("mailto:{}?subject={}&body={}", m.to, enc(&m.subject), enc(&m.text));
            ctx.open_url(egui::OpenUrl::new_tab(url));
            app.notify("Correo preparado en su programa de email.");
        }
        Channel::Sms => {
            ctx.copy_text(m.text.clone());
            app.notify(format!("Texto del SMS copiado. Envíelo al {} desde su pasarela o móvil.", m.to));
        }
    }
    app.db.mark_sent(m.id);
    app.changed();
}

/// Recoge el resultado del SMS enviado por la pasarela.
fn poll_sms(app: &mut App) {
    let Some((id, r)) = app.sms_job.as_ref().and_then(|rx| rx.try_recv().ok()) else { return };
    app.sms_job = None;
    app.db.sms_outcome(id, &r);
    app.changed();
    match r {
        Ok(()) => app.notify("SMS enviado."),
        Err(e) => app.warn(e.text().to_string()),
    }
}

/// Pasarela de SMS (en este ordenador; la clave no va en los datos de la clínica).
pub fn sms_settings(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Pasarela de SMS");
        muted(
            ui,
            "Sin pasarela, los SMS se copian y se envían desde un móvil. Con la del proveedor que contrate la clínica (https), salen desde el programa: cada mensaje lleva su referencia como clave de idempotencia y, si no se sabe si salió, se pregunta antes de repetirlo.",
        );
        ui.horizontal_wrapped(|ui| {
            field(ui, "Dirección de la pasarela (https://…)", &mut app.sms.url, 340.0);
            field(ui, "Remitente", &mut app.sms.sender, 140.0);
        });
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Clave de la pasarela").color(MUTED));
                ui.add_sized([280.0, 34.0], egui::TextEdit::singleline(&mut app.sms.token).password(true));
            });
            ui.vertical(|ui| {
                ui.add_space(19.0);
                if primary(ui, "Guardar").clicked() {
                    match crate::sms::save_settings(&app.sms) {
                        Ok(()) => app.notify("Pasarela de SMS guardada en este ordenador."),
                        Err(e) => app.warn(e),
                    }
                }
            });
        });
    });
}

pub fn screen(app: &mut App, ui: &mut Ui) {
    poll_sms(app);
    ui.horizontal(|ui| {
        title(ui, "Comunicaciones", "Recordatorios de cita, avisos de revisión, cumpleaños y seguimiento de presupuestos por WhatsApp, SMS y email");
    });
    ui.horizontal(|ui| {
        for (i, n) in ["Mensajes", "Plantillas de mensajes", "Plantillas de documentos", "💬 Confirmaciones por WhatsApp", "📣 Campañas"].iter().enumerate() {
            if ui.selectable_label(app.comms.tab == i, *n).clicked() {
                app.comms.tab = i;
            }
        }
    });
    ui.add_space(8.0);
    match app.comms.tab {
        0 => messages(app, ui),
        1 => msg_templates(app, ui),
        3 => crate::screens::whatsapp::log_tab(app, ui),
        4 => crate::screens::campanas::tab(app, ui),
        _ => doc_templates(app, ui),
    }
}

fn messages(app: &mut App, ui: &mut Ui) {
    let today = app.today;
    let pend = app.db.messages.iter().filter(|m| m.state == MsgState::Pendiente).count();
    let sent_today = app.db.messages.iter().filter(|m| m.sent_at.map(|d| d.date() == today).unwrap_or(false)).count();
    let tomorrow = app.db.appointments.iter().filter(|a| a.date == today + chrono::Duration::days(1) && !a.state.is_cancelled()).count();
    let w = kpi_w(ui, 4);
    ui.horizontal(|ui| {
        kpi(ui, w, "Pendientes de enviar", &pend.to_string(), "revise y envíe", WARN);
        ui.add_space(4.0);
        kpi(ui, w, "Enviados hoy", &sent_today.to_string(), "WhatsApp, SMS y email", OK);
        ui.add_space(4.0);
        kpi(ui, w, "Citas de mañana", &tomorrow.to_string(), "para recordar", INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Sin consentimiento", &app.comms.last_skipped.to_string(), "no se les escribe (RGPD)", DANGER);
    });
    ui.add_space(8.0);
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    let side_w = 380.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail - side_w - 24.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    if primary(ui, "⟳ Generar mensajes de hoy").on_hover_text("Crea los recordatorios y avisos según las plantillas activas").clicked() {
                        let (c, s) = app.db.generate_messages(app.today);
                        app.comms.last_skipped = s;
                        app.changed();
                        app.notify(format!("{c} mensajes nuevos · {s} pacientes sin consentimiento o sin contacto"));
                    }
                    // los correos salen desde el programa con el servidor de la clínica
                    let emails = app.db.pending_emails();
                    if let Some(ob) = &app.comms.outbox {
                        ui.add(egui::Spinner::new().size(16.0));
                        ui.label(RichText::new(format!("Enviando correos: {} de {}…", ob.done, ob.total)).color(MUTED));
                    } else if !emails.is_empty()
                        && app.db.mail.ready()
                        && ui
                            .add(
                                egui::Button::new(RichText::new(format!("✉ Enviar por correo ({})", emails.len())).color(Color32::WHITE).font(sbf(14.0)))
                                    .fill(channel_color(Channel::Email))
                                    .corner_radius(CornerRadius::same(10))
                                    .min_size(egui::vec2(0.0, 36.0)),
                            )
                            .on_hover_text("Envía ahora todos los correos pendientes desde el programa")
                            .clicked()
                    {
                        start_outbox(app, emails, false);
                    }
                    ui.add_space(8.0);
                    for (f, n) in [(Some(MsgState::Pendiente), "Pendientes"), (Some(MsgState::Enviado), "Enviados"), (Some(MsgState::Descartado), "Descartados"), (None, "Todos")] {
                        if ui.selectable_label(app.comms.filter == f, n).clicked() {
                            app.comms.filter = f;
                        }
                    }
                });
                // estado del envío automático por correo
                ui.horizontal_wrapped(|ui| {
                    let m = &app.db.mail;
                    let me = crate::shared::machine_name();
                    let txt = if !m.ready() {
                        ("Los correos se abren uno a uno en su programa de correo: configure el correo de la clínica para enviarlos desde aquí.".to_string(), MUTED)
                    } else if !m.auto_send {
                        ("Envío automático por correo desactivado: los correos se envían con el botón «Enviar por correo».".to_string(), MUTED)
                    } else if m.auto_station != me {
                        (format!("Envío automático por correo: cada día entre las {}:00 y las {}:00 desde el ordenador «{}».", m.auto_hour, m.auto_until, m.auto_station), INFO)
                    } else if m.last_auto == Some(app.today) {
                        let last = app.comms.last_outbox.filter(|l| l.3).map(|l| format!(" ({} enviados{})", l.1, if l.2 > 0 { format!(", {} con error", l.2) } else { String::new() })).unwrap_or_default();
                        (format!("Envío automático por correo: hoy ya se ha hecho{last}. Mañana, a partir de las {}:00.", m.auto_hour), OK)
                    } else {
                        (format!("Envío automático por correo: hoy entre las {}:00 y las {}:00 desde este ordenador (si no da tiempo, mañana).", m.auto_hour, m.auto_until), INFO)
                    };
                    ui.label(RichText::new(txt.0).size(12.5).color(txt.1));
                    if ui.link(RichText::new("Ajustes ›").size(12.5)).clicked() {
                        crate::screens::config::open(app, crate::screens::config::CfgTab::Correo);
                    }
                });
                ui.add_space(6.0);
                let tbl = Table::new("msgs", vec![("Creado", 100.0), ("Paciente", 180.0), ("Tipo", 170.0), ("Canal", 85.0), ("Destino", 130.0), ("Estado", 90.0)]);
                tbl.header(ui);
                let mut list: Vec<Message> = app.db.messages.iter().filter(|m| app.comms.filter.map(|f| m.state == f).unwrap_or(true)).cloned().collect();
                list.sort_by_key(|m| std::cmp::Reverse((m.created, m.id)));
                if list.is_empty() {
                    empty_state(ui, "No hay mensajes con este filtro. Pulse «Generar mensajes de hoy».");
                }
                let mut pick = None;
                egui::ScrollArea::vertical().id_salt("msgl").max_height(470.0).show(ui, |ui| {
                    for (i, m) in list.iter().enumerate() {
                        let failed = m.state == MsgState::Pendiente && !m.error.is_empty() && !m.uncertain;
                        let st_col = match m.state {
                            MsgState::Pendiente if m.uncertain => WARN,
                            MsgState::Pendiente if failed => DANGER,
                            MsgState::Pendiente => WARN,
                            MsgState::Enviado => OK,
                            MsgState::Descartado => MUTED,
                        };
                        let cells = [
                            cell(m.created.format("%d/%m %H:%M").to_string()),
                            cell(app.db.patient_name(m.patient_id)),
                            cell(m.kind.label()),
                            cellc(m.channel.label(), channel_color(m.channel)),
                            cell(&m.to),
                            cellc(
                                if m.uncertain {
                                    "¿Llegó?"
                                } else if failed {
                                    "Con error"
                                } else {
                                    m.state.label()
                                },
                                st_col,
                            ),
                        ];
                        if tbl.row(ui, i, app.comms.selected == Some(m.id), &cells).clicked() {
                            pick = Some(m.id);
                        }
                    }
                });
                if pick.is_some() {
                    app.comms.selected = pick;
                }
            });
        });
        ui.vertical(|ui| {
            ui.set_width(side_w);
            detail(app, ui);
        });
    });
}

fn detail(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let Some(id) = app.comms.selected else {
            section(ui, "Mensaje");
            muted(ui, "Seleccione un mensaje de la lista para revisarlo y enviarlo.");
            return;
        };
        let Some(m) = app.db.messages.iter().find(|m| m.id == id).cloned() else { return };
        section(ui, m.kind.label());
        ui.horizontal(|ui| {
            pill(ui, m.channel.label(), channel_color(m.channel));
            ui.label(RichText::new(app.db.patient_name(m.patient_id)).family(sb()));
        });
        muted(ui, format!("Para: {}", m.to));
        ui.add_space(6.0);
        // burbuja de chat
        egui::Frame::new().fill(if m.channel == Channel::WhatsApp { Color32::from_rgb(220, 248, 198) } else { Color32::from_rgb(238, 242, 248) }).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(mm) = app.db.messages.iter_mut().find(|x| x.id == id) {
                if mm.channel == Channel::Email {
                    ui.add(egui::TextEdit::singleline(&mut mm.subject).desired_width(f32::INFINITY));
                }
                let enabled = mm.state == MsgState::Pendiente;
                let r = ui.add_enabled(enabled, egui::TextEdit::multiline(&mut mm.text).desired_rows(5).desired_width(f32::INFINITY).frame(egui::Frame::NONE));
                if r.changed() {
                    app.dirty = true;
                }
            }
        });
        let len = m.text.chars().count();
        if m.channel == Channel::Sms {
            muted(ui, format!("{len} caracteres · {} SMS", len.div_ceil(160).max(1)));
        }
        ui.add_space(8.0);
        let ctx = ui.ctx().clone();
        if m.state == MsgState::Pendiente && !m.error.is_empty() && !m.uncertain {
            ui.label(RichText::new(format!("No se pudo enviar ({} intento{}): {}", m.attempts, if m.attempts == 1 { "" } else { "s" }, m.error)).size(12.5).color(DANGER));
            ui.add_space(4.0);
        }
        let sending = app.comms.outbox.is_some();
        match m.state {
            // no se sabe si llegó: no se reenvía solo, lo decide recepción
            MsgState::Pendiente if m.uncertain => {
                ui.label(RichText::new(format!("No se sabe si llegó. {}", m.error)).size(12.5).color(WARN));
                muted(ui, "Si el paciente lo ha recibido (o la copia está en el correo de la clínica), márquelo como enviado; si no, vuelva a enviarlo.");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Sí, consta como enviado").clicked() {
                        app.db.resolve_uncertain(id, true);
                        app.changed();
                    }
                    if ui.button("No llegó: volver a enviar").clicked() {
                        app.db.resolve_uncertain(id, false);
                        app.changed();
                    }
                });
            }
            MsgState::Pendiente if !m.sending.is_empty() && !sending => {
                muted(ui, "Se está enviando desde otro puesto.");
            }
            MsgState::Pendiente => {
                ui.horizontal_wrapped(|ui| {
                    let label = match m.channel {
                        Channel::WhatsApp => "Enviar por WhatsApp",
                        Channel::Email if app.db.mail.ready() => "Enviar por correo",
                        Channel::Email => "Abrir en el correo",
                        Channel::Sms => "Copiar SMS y marcar enviado",
                    };
                    if sending && m.channel == Channel::Email && app.db.mail.ready() {
                        ui.add(egui::Spinner::new().size(16.0));
                        ui.label(RichText::new("Enviando…").color(MUTED));
                    } else if primary(ui, label).clicked() {
                        let current = app.db.messages.iter().find(|x| x.id == id).cloned().unwrap();
                        send(app, &ctx, &current);
                    }
                    if ui.button("Marcar enviado").clicked() {
                        app.db.mark_sent(id);
                        app.changed();
                    }
                    if ui.button("Descartar").clicked() {
                        if let Some(mm) = app.db.messages.iter_mut().find(|x| x.id == id) {
                            mm.state = MsgState::Descartado;
                        }
                        app.changed();
                    }
                });
            }
            _ => {
                muted(ui, format!("{} {}", m.state.label(), m.sent_at.map(|d| format!("el {}", d.format("%d/%m/%Y %H:%M"))).unwrap_or_default()));
            }
        }
        ui.add_space(4.0);
        if ui.link("Abrir ficha del paciente").clicked() {
            app.open_patient(m.patient_id, PatTab::Citas);
        }
    });
}

/// Comodines de los documentos (estos sí pueden llevar datos de salud).
const DOC_PLACEHOLDERS: &str = "[NOMBRE] [APELLIDOS] [NIF] [FECHANAC] [DIRECCION] [CIUDAD] [HISTORIAL] [FECHA] [DOCTOR] [CLINICA] [DIRCLINICA] [TELCLINICA] [PIEZAS] [ALERTAS] [TRATAMIENTOS] [PENDIENTES]";

/// Los comodines de esta clase de mensaje y, si el texto lleva otros, aviso:
/// llegarían tal cual al paciente (o son datos de salud, que no se envían).
fn placeholder_help(ui: &mut Ui, text: &str, kind: MsgKind) {
    muted(ui, format!("Comodines: {}", crate::comms::msg_placeholders(kind).join(" ")));
    let bad = crate::comms::unknown_placeholders(text, kind);
    if !bad.is_empty() {
        let clinical: Vec<&String> = bad.iter().filter(|b| crate::comms::CLINICAL_PLACEHOLDERS.contains(&b.as_str())).collect();
        let other: Vec<&String> = bad.iter().filter(|b| !crate::comms::CLINICAL_PLACEHOLDERS.contains(&b.as_str())).collect();
        if !other.is_empty() {
            ui.label(RichText::new(format!("No se sustituyen en este mensaje y llegarían tal cual: {}", other.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "))).size(12.5).color(DANGER));
        }
        if !clinical.is_empty() {
            ui.label(RichText::new(format!("Son datos de salud y no se envían por mensaje (se quitan): {}", clinical.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "))).size(12.5).color(DANGER));
        }
    }
}

fn msg_templates(app: &mut App, ui: &mut Ui) {
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail * 0.4);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    section(ui, "Plantillas");
                    right_aligned(ui, |ui| {
                        if ui.button("＋ Nueva").clicked() {
                            let id = app.db.new_id();
                            app.db.msg_templates.push(MsgTemplate { id, name: "Nueva plantilla".into(), kind: MsgKind::Libre, channel: Channel::WhatsApp, days: 1, text: "Hola [NOMBRE], ".into(), subject: String::new(), active: true });
                            app.comms.tpl_sel = Some(id);
                            app.changed();
                        }
                    });
                });
                let list = app.db.msg_templates.clone();
                for t in &list {
                    ui.horizontal(|ui| {
                        pill(ui, t.channel.label(), channel_color(t.channel));
                        if ui.selectable_label(app.comms.tpl_sel == Some(t.id), RichText::new(&t.name).color(if t.active { TEXT } else { MUTED })).clicked() {
                            app.comms.tpl_sel = Some(t.id);
                        }
                    });
                }
            });
        });
        ui.vertical(|ui| {
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                let pid = app.patient;
                let Some(id) = app.comms.tpl_sel else { return };
                let mut preview = String::new();
                let mut remove = false;
                if let Some(t) = app.db.msg_templates.iter_mut().find(|t| t.id == id) {
                    let before = serde_json::to_string(t).unwrap_or_default();
                    section(ui, "Editar plantilla");
                    ui.checkbox(&mut t.active, "Activa (se usa al generar los mensajes del día)");
                    field(ui, "Nombre", &mut t.name, 380.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Tipo").size(13.5).color(MUTED));
                            crate::theme::combo("mt_kind").width(240.0).selected_text(t.kind.label()).show_ui(ui, |ui| {
                                for k in [MsgKind::RecordatorioCita, MsgKind::Recall, MsgKind::Cumpleanos, MsgKind::PresupuestoPendiente, MsgKind::Libre] {
                                    ui.selectable_value(&mut t.kind, k, k.label());
                                }
                            });
                        });
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Canal").size(13.5).color(MUTED));
                            ui.horizontal(|ui| {
                                for c in Channel::ALL {
                                    ui.selectable_value(&mut t.channel, c, c.label());
                                }
                            });
                        });
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(match t.kind {
                                    MsgKind::RecordatorioCita => "Días antes de la cita",
                                    MsgKind::Recall => "Días de antelación",
                                    MsgKind::PresupuestoPendiente => "Días sin respuesta",
                                    MsgKind::Encuesta => "Días después de la visita",
                                    _ => "Días",
                                })
                                .size(13.5)
                                .color(MUTED),
                            );
                            ui.add(egui::DragValue::new(&mut t.days).range(0..=60));
                        });
                    });
                    if t.channel == Channel::Email {
                        field(ui, "Asunto", &mut t.subject, 380.0);
                    }
                    ui.label(RichText::new("Texto").size(13.5).color(MUTED));
                    ui.add(egui::TextEdit::multiline(&mut t.text).desired_rows(6).desired_width(f32::INFINITY));
                    let all = format!("{} {}", t.subject, t.text);
                    placeholder_help(ui, &all, t.kind);
                    if serde_json::to_string(t).unwrap_or_default() != before {
                        app.dirty = true;
                    }
                    preview = t.text.clone();
                    if ui.button("Quitar plantilla").clicked() {
                        remove = true;
                    }
                }
                if remove {
                    app.db.msg_templates.retain(|t| t.id != id);
                    app.comms.tpl_sel = None;
                    app.changed();
                    return;
                }
                if let Some(pid) = pid {
                    ui.add_space(8.0);
                    section(ui, &format!("Vista previa con {}", app.db.patient_name(pid)));
                    let site = {
                        let b = app.db.booking.public_url.trim().trim_end_matches('/');
                        if b.is_empty() { "https://citas.suclinica.es".to_string() } else { b.to_string() }
                    };
                    let extra = [
                        ("[HORA]", "10:30".to_string()),
                        ("[MOTIVO]", "revisión".to_string()),
                        ("[IMPORTE]", money(450.0)),
                        ("[ENLACE]", format!("{site}/mi-cita/3f9c…")),
                        ("[ENLACE_CITA]", format!("{site}/")),
                        ("[ENLACE_PRESUPUESTO]", format!("{site}/presupuesto/8a1e…")),
                    ];
                    let txt = app.db.fill_message(&preview, pid, &extra);
                    egui::Frame::new().fill(Color32::from_rgb(220, 248, 198)).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(txt);
                    });
                }
            });
        });
    });
}

fn doc_templates(app: &mut App, ui: &mut Ui) {
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail * 0.36);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    section(ui, "Documentos y consentimientos");
                    right_aligned(ui, |ui| {
                        if ui.button("＋ Nuevo").clicked() {
                            let id = app.db.new_id();
                            app.db.doc_templates.push(DocTemplate {
                                id,
                                name: "Nuevo documento".into(),
                                category: "Consentimiento informado".into(),
                                body: "D./Dña. [NOMBRE] [APELLIDOS] …\n\nEn [CIUDAD], a [FECHA].".into(),
                                marks_gdpr: false,
                                for_kinds: vec![],
                                translation_of: None,
                                validated: None,
                            });
                            app.comms.doc_tpl_sel = Some(id);
                            app.changed();
                        }
                    });
                });
                let list = app.db.doc_templates.clone();
                for t in &list {
                    ui.horizontal(|ui| {
                        if ui.selectable_label(app.comms.doc_tpl_sel == Some(t.id), &t.name).clicked() {
                            app.comms.doc_tpl_sel = Some(t.id);
                        }
                        muted(ui, &t.category);
                    });
                }
            });
        });
        ui.vertical(|ui| {
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                let Some(id) = app.comms.doc_tpl_sel else { return };
                let mut remove = false;
                if let Some(t) = app.db.doc_templates.iter_mut().find(|t| t.id == id) {
                    let before = serde_json::to_string(t).unwrap_or_default();
                    section(ui, "Editar plantilla de documento");
                    ui.horizontal(|ui| {
                        field(ui, "Nombre", &mut t.name, 320.0);
                        field(ui, "Categoría", &mut t.category, 200.0);
                    });
                    ui.checkbox(&mut t.marks_gdpr, "Al firmarlo, registra el consentimiento de protección de datos del paciente");
                    ui.label(RichText::new("Texto del documento").size(13.5).color(MUTED));
                    ui.add(egui::TextEdit::multiline(&mut t.body).desired_rows(16).desired_width(f32::INFINITY));
                    muted(ui, format!("Comodines: {DOC_PLACEHOLDERS}"));
                    if serde_json::to_string(t).unwrap_or_default() != before {
                        app.dirty = true;
                    }
                    if ui.button("Quitar plantilla").clicked() {
                        remove = true;
                    }
                }
                if remove {
                    app.db.doc_templates.retain(|t| t.id != id);
                    app.comms.doc_tpl_sel = None;
                    app.changed();
                }
            });
        });
    });
}

/// Bloque de comunicaciones dentro de la ficha (pestaña Citas).
pub fn patient_block(app: &mut App, ui: &mut Ui, pid: u32) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Comunicaciones con el paciente");
        let p = app.db.patient(pid).cloned().unwrap_or_default();
        if !p.accepts_gdpr {
            ui.label(RichText::new("⚠ Sin consentimiento de protección de datos: no se le enviarán mensajes automáticos.").color(WARN));
        }
        ui.horizontal(|ui| {
            for c in Channel::ALL {
                ui.selectable_value(&mut app.comms.free_channel, c, c.label());
            }
            ui.add_sized([ui.available_width() - 110.0, 34.0], egui::TextEdit::singleline(&mut app.comms.free_text).hint_text("Escriba un mensaje para el paciente…"));
            if primary(ui, "Enviar").clicked() && !app.comms.free_text.trim().is_empty() {
                let ch = app.comms.free_channel;
                let to = if ch == Channel::Email { p.email.clone() } else { p.mobile.clone() };
                if to.is_empty() {
                    app.warn("El paciente no tiene ese dato de contacto.");
                } else {
                    let id = app.db.new_id();
                    let text = app.db.fill_message(&std::mem::take(&mut app.comms.free_text), pid, &[]);
                    let m = Message {
                        id,
                        patient_id: pid,
                        kind: MsgKind::Libre,
                        channel: ch,
                        to,
                        subject: app.db.config.name.clone(),
                        text,
                        created: now(),
                        sent_at: None,
                        state: MsgState::Pendiente,
                        related: None,
                        user: app.db.config.current_user.clone(),
                        error: String::new(),
                        attempts: 0,
                        uncertain: false,
                        sending: String::new(),
                        sending_since: None,
                        sent_by: String::new(),
                    };
                    app.db.messages.push(m.clone());
                    let ctx = ui.ctx().clone();
                    send(app, &ctx, &m);
                }
            }
        });
        let mut list: Vec<Message> = app.db.messages.iter().filter(|m| m.patient_id == pid).cloned().collect();
        list.sort_by_key(|m| std::cmp::Reverse(m.created));
        for m in list.iter().take(6) {
            ui.horizontal(|ui| {
                pill(ui, m.channel.label(), channel_color(m.channel));
                ui.label(RichText::new(format!("{} · {}", m.created.format("%d/%m %H:%M"), m.kind.label())).size(14.0));
                muted(ui, m.state.label());
            });
        }
        if list.is_empty() {
            muted(ui, "Sin mensajes todavía.");
        }
    });
}
