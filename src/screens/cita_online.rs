//! Cita online: publicación de la página, solicitudes que llegan (en la
//! agenda, para confirmarlas o rechazarlas) y su apartado de Configuración.

use crate::app::App;
use crate::booking::*;
use crate::model::*;
use crate::theme::*;
use egui::{Color32, CornerRadius, RichText, Stroke, Ui, vec2};
use std::sync::mpsc::Receiver;

#[derive(Default)]
pub struct BookingUi {
    pub server: Option<BookingServer>,
    /// TPV virtual de este ordenador (de `tpv.json`; se lee al publicar).
    pub tpv: Option<crate::pago::TpvSettings>,
    /// Pagos por internet: el elegido, la devolución que se va a pedir y la que está en curso.
    pub pay_sel: Option<u32>,
    pub refund_ask: Option<(u32, String, f64)>,
    pub refund_job: Option<crate::screens::pagos::RefundJob>,
    /// Puerto y red con los que se abrió la página.
    bound: Option<(u16, bool)>,
    pub error: Option<String>,
    last_publish: f64,
    /// Solicitud que se está rechazando y el motivo que se escribe.
    pub rejecting: Option<u32>,
    pub reject_text: String,
    /// Avisos por correo en curso: (nombre del paciente, resultado).
    mail_jobs: Vec<(String, Receiver<Result<(), String>>)>,
    /// Túnel de Cloudflare que publica la página en internet.
    pub tunnel: Option<crate::tunnel::Tunnel>,
    pub tunnel_error: Option<String>,
    /// Ya se intentó abrir el túnel al arrancar.
    tunnel_auto_tried: bool,
    /// Dónde está cloudflared (se busca una vez).
    cloudflared: Option<Option<std::path::PathBuf>>,
    pub show_token: bool,
    /// Comprobación desde internet en curso y su resultado.
    check: Option<Receiver<Result<String, String>>>,
    pub check_result: Option<Result<String, String>>,
}

fn me() -> String {
    crate::shared::machine_name()
}

/// Dirección de la página en este ordenador.
pub fn local_url(app: &App) -> String {
    let port = app.booking.server.as_ref().map(|s| s.port).unwrap_or(app.db.booking.port);
    format!("http://127.0.0.1:{port}/")
}

/// Dirección que se da a los pacientes (la pública si la hay).
pub fn patient_url(app: &App) -> String {
    let p = app.db.booking.public_url.trim();
    if p.is_empty() { local_url(app) } else { p.to_string() }
}

/// Avisa al paciente por correo de la confirmación o el rechazo (en segundo plano).
fn mail_patient(app: &mut App, id: u32) {
    if !app.db.booking.email_patient || !app.db.mail.ready() {
        return;
    }
    let Some(r) = app.db.booking_requests.iter().find(|r| r.id == id).cloned() else { return };
    let Some((to, subject, text, files)) = patient_mail(&app.db, &r) else { return };
    let cfg = app.db.mail.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::mail::send(&cfg, &to, &subject, &text, &files));
    });
    app.booking.mail_jobs.push((r.name.clone(), rx));
}

pub fn confirm(app: &mut App, id: u32) {
    // con varios puestos, la confirma uno solo (y solo ese avisa al paciente)
    match app.chain_op(crate::app::ChainOp::Confirm { request: id }) {
        Ok(_) => {
            let name = app.db.booking_requests.iter().find(|r| r.id == id).map(|r| r.name.clone()).unwrap_or_default();
            mail_patient(app, id);
            app.notify(format!("Cita de {name} confirmada y apuntada en la agenda."));
        }
        Err(e) => app.warn(e),
    }
}

pub fn reject(app: &mut App, id: u32, why: &str) {
    app.booking.rejecting = None;
    app.booking.reject_text.clear();
    match app.chain_op(crate::app::ChainOp::Reject { request: id, why: why.to_string() }) {
        Ok(_) => {
            mail_patient(app, id);
            app.notify("Solicitud rechazada.");
        }
        Err(e) => app.warn(e),
    }
}

/// Abre o cierra la página según la configuración, recoge las solicitudes y
/// mantiene al día los huecos libres que ve la página.
pub fn tick(app: &mut App, ctx: &egui::Context, time: f64) {
    // avisos por correo terminados
    let mut i = 0;
    while i < app.booking.mail_jobs.len() {
        match app.booking.mail_jobs[i].1.try_recv() {
            Ok(r) => {
                let (name, _) = app.booking.mail_jobs.remove(i);
                if let Err(e) = r {
                    app.warn(format!("No se pudo avisar por correo a {name}: {e}"));
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                app.booking.mail_jobs.remove(i);
            }
            Err(_) => i += 1,
        }
    }
    // huecos ofrecidos a la lista de espera que ya pasaron o que recepción ha ocupado
    if app.db.slot_offers.iter().any(|o| o.state == crate::slot_offer::OfferState::Abierta) && app.db.refresh_offers(now()) {
        app.changed();
    }
    let cfg = &app.db.booking;
    let want = cfg.enabled && cfg.station == me();
    // comprobación desde internet terminada
    if let Some(r) = app.booking.check.as_ref().and_then(|rx| rx.try_recv().ok()) {
        app.booking.check = None;
        app.booking.check_result = Some(r);
    }
    if !want {
        app.booking.server = None;
        app.booking.bound = None;
        app.booking.tunnel = None;
        return;
    }
    // túnel de Cloudflare al arrancar (si hay token y se pidió)
    if cfg.tunnel_auto && !cfg.tunnel_token.trim().is_empty() && app.booking.tunnel.is_none() && !app.booking.tunnel_auto_tried {
        app.booking.tunnel_auto_tried = true;
        start_tunnel(app, crate::tunnel::Mode::Named);
    }
    let cfg = &app.db.booking;
    let key = (cfg.port, cfg.lan);
    if app.booking.server.is_some() && app.booking.bound != Some(key) {
        app.booking.server = None;
    }
    if app.booking.server.is_none() {
        // un intento cada 10 s si el puerto está ocupado
        if app.booking.error.is_some() && time - app.booking.last_publish < 10.0 {
            return;
        }
        let c = ctx.clone();
        match listen(key.0, key.1, publish(&app.db, app.today), move || c.request_repaint()) {
            Ok(s) => {
                app.booking.server = Some(s);
                app.booking.bound = Some(key);
                app.booking.error = None;
            }
            Err(e) => {
                app.booking.error = Some(e);
                app.booking.last_publish = time;
                return;
            }
        }
        app.booking.last_publish = time;
    }
    let Some(srv) = app.booking.server.as_ref() else { return };
    let mut got = vec![];
    while let Ok(x) = srv.rx.try_recv() {
        got.push(x);
    }
    let shared = srv.shared.clone();
    let mut holds = vec![];
    let any = !got.is_empty();
    for ev in got {
        match ev {
            WebEvent::Booking(inc) => {
                holds.push(inc.hold);
                let mut r = inc.request;
                r.code = app.db.booking.reasons.iter().find(|x| x.name == r.reason).map(|x| x.code.clone()).unwrap_or_default();
                r.center_id = cid(app.db.booking.center_id);
                let (name, when) = (r.name.clone(), format!("{} a las {}", day_label(r.date), r.start.format("%H:%M")));
                let id = app.db.booking_receive(r);
                app.changed();
                if app.db.booking.auto_confirm {
                    confirm(app, id);
                } else {
                    app.notify(format!("Nueva cita pedida por internet: {name}, {when}."));
                }
            }
            WebEvent::Appt { appt, action } => link_action(app, appt, action, &mut holds),
            WebEvent::Budget { budget, action } => budget_action(app, budget, action),
            WebEvent::Doc { doc, action } => doc_action(app, doc, action),
            WebEvent::Offer { token, action } => offer_action(app, &token, action),
            WebEvent::Rx { patient } => {
                if app.db.rx_link_used(patient, crate::model::now()) {
                    app.changed();
                }
            }
            WebEvent::PayStarted { token, order, method } => {
                if app.db.pay_started(&token, &order, method, now()) {
                    app.changed();
                }
            }
            WebEvent::PayNotice(n) => pay_notice(app, &n),
        }
    }
    if any || time - app.booking.last_publish >= 2.0 {
        // cuestionarios de pacientes nuevos que ya tienen ficha
        if app.db.portal_attach_health() > 0 {
            app.changed();
        }
        let p = publish(&app.db, app.today);
        // el TPV de este ordenador (su clave no va en los datos de la clínica)
        let tpv = app.db.config.pay_online.enabled.then(|| app.booking.tpv.get_or_insert_with(crate::pago::load_settings).clone());
        let mut g = shared.lock().unwrap_or_else(|e| e.into_inner());
        g.published = p;
        g.tpv = tpv;
        g.held.retain(|h| !holds.contains(h));
        app.booking.last_publish = time;
    }
    ctx.request_repaint_after(std::time::Duration::from_secs(2));
}

/// Notificación de Redsys ya comprobada: se apunta una vez y se avisa.
fn pay_notice(app: &mut App, n: &crate::pago::Notice) {
    use crate::pago::Applied;
    match app.db.apply_notice(n, now()) {
        Ok(Applied::Paid { .. }) => app.notify(format!("Pago por internet recibido: {} (pedido {}).", crate::cents::Cents(n.amount), n.order)),
        Ok(Applied::PaidTwice { .. }) => app.warn(format!("Se ha pagado dos veces por internet (pedido {}): devuelva el segundo cobro en Cobros › Pagos por internet.", n.order)),
        Ok(Applied::Review(why)) => app.warn(format!("Pago por internet para revisar (pedido {}): {why}", n.order)),
        Ok(Applied::Denied(_) | Applied::Duplicate) => {}
        Err(e) => app.warn(format!("Notificación de pago por internet no apuntada: {e}")),
    }
    app.changed();
}

fn cloudflared(app: &mut App) -> Option<std::path::PathBuf> {
    if app.booking.cloudflared.is_none() {
        app.booking.cloudflared = Some(crate::tunnel::find(&app.db.booking.tunnel_exe));
    }
    app.booking.cloudflared.clone().flatten()
}

/// Abre el túnel de Cloudflare (con el token de la clínica, o una dirección temporal de prueba).
pub fn start_tunnel(app: &mut App, mode: crate::tunnel::Mode) {
    app.booking.tunnel = None;
    let Some(exe) = cloudflared(app) else {
        app.booking.tunnel_error = Some(format!("No se encuentra cloudflared en este ordenador. {}", crate::tunnel::install_hint()));
        return;
    };
    let port = app.booking.server.as_ref().map(|s| s.port).unwrap_or(app.db.booking.port);
    match crate::tunnel::start(&exe, mode, port, &app.db.booking.tunnel_token) {
        Ok(t) => {
            app.booking.tunnel = Some(t);
            app.booking.tunnel_error = None;
            app.booking.check_result = None;
        }
        Err(e) => app.booking.tunnel_error = Some(e),
    }
}

/// Comprueba en segundo plano que la dirección pública llega a la página.
pub fn check_from_internet(app: &mut App, base: String) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::tunnel::check_public(&base));
    });
    app.booking.check = Some(rx);
    app.booking.check_result = None;
}

/// Tarjeta «Publicar en internet» de Configuración › Cita online.
fn tunnel_card(app: &mut App, ui: &mut Ui) {
    use crate::tunnel::Mode;
    let me = me();
    let here = app.db.booking.enabled && app.db.booking.station == me;
    let exe = cloudflared(app);
    let st = app.booking.tunnel.as_mut().map(|t| (t.mode, t.snapshot()));
    let mut action = 0u8;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Publicar en internet");
            right_aligned(ui, |ui| match &st {
                Some((_, s)) if s.exited.is_some() => {
                    pill(ui, "Cerrado", DANGER);
                }
                Some((Mode::Named, s)) if s.connections > 0 => {
                    pill(ui, &format!("Conectado · {} {}", s.connections, if s.connections == 1 { "conexión" } else { "conexiones" }), OK);
                }
                Some((Mode::Quick, s)) if s.url.is_some() => {
                    pill(ui, "Dirección temporal activa", WARN);
                }
                Some(_) => {
                    pill(ui, "Conectando…", INFO);
                }
                None => {
                    pill(ui, "Sin publicar", MUTED);
                }
            });
        });
        muted(
            ui,
            "Con Cloudflare Tunnel la página se publica en una dirección https de la clínica sin abrir puertos en el router: este ordenador abre una conexión cifrada hacia Cloudflare. Es gratis con una cuenta de Cloudflare y el dominio de la clínica.",
        );
        ui.add_space(6.0);
        // cloudflared
        ui.horizontal_wrapped(|ui| {
            match &exe {
                Some(p) => {
                    ui.label(RichText::new("✔ cloudflared:").color(OK));
                    ui.label(RichText::new(p.display().to_string()).size(12.5).color(MUTED));
                }
                None => {
                    ui.label(RichText::new("No se encuentra cloudflared en este ordenador.").color(WARN));
                }
            }
            if ui.small_button("Buscar de nuevo").clicked() {
                action = 9;
            }
        });
        if exe.is_none() {
            ui.label(RichText::new(crate::tunnel::install_hint()).size(12.5).color(TEXT));
            let c = &mut app.db.booking;
            ui.horizontal(|ui| {
                ui.label(RichText::new("o su ruta:").size(12.5).color(MUTED));
                if ui.add(egui::TextEdit::singleline(&mut c.tunnel_exe).hint_text(if cfg!(windows) { r"C:\Program Files (x86)\cloudflared\cloudflared.exe" } else { "/usr/local/bin/cloudflared" }).desired_width(ui.available_width())).changed() {
                    action = 9;
                }
            });
        }
        ui.add_space(4.0);
        // token
        {
            let c = &mut app.db.booking;
            ui.label(RichText::new("Código (token) del túnel").font(mdf(12.8)).color(MUTED));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut c.tunnel_token).password(!app.booking.show_token).hint_text("eyJhIjoi…").desired_width((ui.available_width() - 120.0).max(120.0)));
                if ui.button(if app.booking.show_token { "Ocultar" } else { "Mostrar" }).clicked() {
                    app.booking.show_token = !app.booking.show_token;
                }
            });
            ui.checkbox(&mut c.tunnel_auto, "Abrir el túnel al arrancar el programa");
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let running = st.as_ref().is_some_and(|(m, s)| *m == Mode::Named && s.exited.is_none());
            if running {
                if ui.button("Desconectar").clicked() {
                    action = 2;
                }
            } else if ui
                .add_enabled(
                    here && exe.is_some() && !app.db.booking.tunnel_token.trim().is_empty(),
                    egui::Button::new(RichText::new("Conectar").color(Color32::WHITE).font(sbf(14.0))).fill(ACCENT).corner_radius(CornerRadius::same(10)).min_size(vec2(0.0, 34.0)),
                )
                .clicked()
            {
                action = 1;
            }
            let base = app.db.booking.public_url.trim().to_string();
            if app.booking.check.is_some() {
                ui.add(egui::Spinner::new().size(16.0));
                ui.label(RichText::new("Comprobando desde internet…").color(MUTED));
            } else if ui
                .add_enabled(base.starts_with("https://") && app.booking.server.is_some(), egui::Button::new("Comprobar desde internet"))
                .on_hover_text("Abre la dirección para los pacientes por internet y comprueba que llega a esta página")
                .clicked()
            {
                action = 3;
            }
            if st.as_ref().is_some_and(|(m, s)| *m == Mode::Quick && s.exited.is_none()) {
                if ui.button("Cerrar la dirección temporal").clicked() {
                    action = 2;
                }
            } else if ui
                .add_enabled(here && exe.is_some() && app.booking.server.is_some(), egui::Button::new("Probar con una dirección temporal"))
                .on_hover_text("Sin cuenta: Cloudflare da una dirección aleatoria que cambia cada vez (sólo para probar)")
                .clicked()
            {
                action = 4;
            }
        });
        if !here {
            ui.label(RichText::new("Active la cita online en este ordenador para poder publicarla.").size(12.5).color(MUTED));
        }
        if let Some(e) = &app.booking.tunnel_error {
            ui.label(RichText::new(e).size(12.5).color(DANGER));
        }
        if let Some((mode, s)) = &st {
            if let Some(e) = s.error.as_ref().or(s.exited.as_ref()) {
                ui.label(RichText::new(e).size(12.5).color(DANGER));
            }
            if *mode == Mode::Quick
                && let Some(u) = &s.url
            {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Dirección temporal:").size(12.5).color(MUTED));
                    ui.label(RichText::new(u).size(12.5).color(ACCENT_DARK));
                    if ui.small_button("Copiar").clicked() {
                        ui.ctx().copy_text(u.clone());
                    }
                    if ui.small_button("Abrir").clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(u.clone()));
                    }
                });
                ui.label(RichText::new("Cambia cada vez que se abre: no la use en enlaces que envíe a los pacientes.").size(12.0).color(WARN));
            }
            if !s.log.is_empty() {
                egui::CollapsingHeader::new(RichText::new("Ver el registro de cloudflared").size(12.5)).id_salt("cf_log").show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                        for l in s.log.iter().rev().take(25) {
                            ui.label(RichText::new(l).size(11.0).monospace().color(MUTED));
                        }
                    });
                });
            }
        }
        match &app.booking.check_result {
            Some(Ok(m)) => {
                ui.label(RichText::new(format!("✔ {m}")).size(13.0).color(OK));
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).size(12.5).color(DANGER));
            }
            None => {}
        }
        ui.add_space(4.0);
        egui::CollapsingHeader::new(RichText::new("Cómo crear el túnel (una sola vez)").font(sbf(13.0))).id_salt("cf_help").show(ui, |ui| {
            for (n, t) in [
                "Entre en Cloudflare con la cuenta de la clínica (es gratis) y añada el dominio de la clínica si aún no está.",
                "En Zero Trust › Redes › Túneles, pulse «Crear un túnel», elija «Cloudflared» y póngale un nombre (por ejemplo, VeodarDental).",
                "Copie el código que aparece tras «--token» (o en el comando de instalación) y péguelo arriba.",
                "En «Public hostname» ponga la dirección que verán los pacientes (por ejemplo, citas.suclinica.es) con servicio HTTP y URL localhost con el puerto de la página (8791 por defecto).",
                "Escriba esa misma dirección, con https://, en «Dirección para los pacientes», pulse «Conectar» y luego «Comprobar desde internet».",
            ]
            .iter()
            .enumerate()
            {
                ui.label(RichText::new(format!("{}. {t}", n + 1)).size(12.5).color(TEXT));
            }
            ui.label(RichText::new("Para recibir las respuestas de WhatsApp añada otro «Public hostname» que apunte al puerto del receptor de WhatsApp.").size(12.0).color(MUTED));
        });
    });
    match action {
        1 => start_tunnel(app, Mode::Named),
        2 => {
            app.booking.tunnel = None;
        }
        3 => {
            let base = app.db.booking.public_url.trim().to_string();
            check_from_internet(app, base);
        }
        4 => start_tunnel(app, Mode::Quick),
        9 => {
            app.booking.cloudflared = None;
            app.booking.tunnel_error = None;
        }
        _ => {}
    }
}

/// Enlace de la cita (lo crea si hace falta): con la dirección pública o, sin ella, la de este ordenador.
pub fn appt_url(app: &mut App, appt: u32) -> String {
    let had = app.db.appt_link(appt).is_some();
    let t = app.db.appt_token(appt);
    if !had {
        app.changed();
    }
    let base = if app.db.booking.public_url.trim().is_empty() { local_url(app) } else { app.db.booking.public_url.clone() };
    crate::portal::link_url(&base, &t)
}

/// Envía al paciente por correo el enlace de su cita (con la cita para el calendario).
pub fn send_link_mail(app: &mut App, appt: u32) {
    let Some(a) = app.db.appointments.iter().find(|a| a.id == appt).cloned() else { return };
    let Some(p) = a.patient_id.and_then(|id| app.db.patient(id)).cloned() else { return };
    let url = appt_url(app, appt);
    let c = &app.db.config;
    let doctor = app.db.doctor(a.doctor_id).map(|d| d.alias.clone()).unwrap_or_default();
    let subject = format!("Su cita en {}: {} a las {}", c.name, day_label(a.date), a.start.format("%H:%M"));
    let text = format!(
        "Hola, {}:\n\nLe recordamos su cita:\n\n  {} a las {}\n  {} · {}\n  {}\n\nDesde este enlace puede confirmar que vendrá, cambiar la hora, anular la cita o rellenar el cuestionario de salud antes de venir:\n{url}\n\nUn saludo,\n{}\n{}",
        p.first_name,
        long_date(a.date),
        a.start.format("%H:%M"),
        a.reason,
        doctor,
        c.address,
        c.name,
        c.phone
    );
    let ics = ics_event(a.id, &c.name, &c.address, &c.phone, a.date, a.start, a.minutes, &format!("{} · {doctor}", a.reason));
    let files = vec![crate::mail::FileAttachment { name: "cita.ics".into(), mime: "text/calendar; charset=utf-8".into(), bytes: ics.into_bytes() }];
    let (cfg, to) = (app.db.mail.clone(), p.email.clone());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::mail::send(&cfg, &to, &subject, &text, &files));
    });
    app.booking.mail_jobs.push((p.full_name(), rx));
    app.notify(format!("Enviando el enlace de la cita a {}.", p.email));
}

/// Franja de la ventana de la cita con su enlace y lo que el paciente ha hecho con él.
pub fn appt_link_row(app: &mut App, ui: &mut Ui, appt: u32) {
    let link = app.db.appt_link(appt).cloned();
    let email = app.db.appointments.iter().find(|a| a.id == appt).and_then(|a| a.patient_id).and_then(|id| app.db.patient(id)).map(|p| p.email.trim().to_string()).unwrap_or_default();
    let mut act = 0u8;
    egui::Frame::new().fill(Color32::from_rgb(250, 249, 255)).stroke(Stroke::new(1.0, tint(VIOLET, 0.2))).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
            crate::icons::settings_icon(ui.painter(), "web", r.center(), VIOLET);
            ui.label(RichText::new("Enlace del paciente («Mi cita»)").font(sbf(13.5)).color(HEADING));
            match &link {
                None => {
                    ui.label(RichText::new("aún no enviado").size(12.5).color(MUTED));
                }
                Some(l) => {
                    if let Some(t) = l.confirmed {
                        pill(ui, &format!("Confirmó el {}", t.format("%d/%m %H:%M")), OK);
                    }
                    if let Some(t) = l.cancelled {
                        pill(ui, &format!("Anuló el {}", t.format("%d/%m %H:%M")), DANGER);
                    }
                    if let Some(t) = l.health_at {
                        pill(ui, &format!("Cuestionario de salud recibido el {}", t.format("%d/%m")), INFO);
                    }
                    if l.confirmed.is_none() && l.cancelled.is_none() && l.health_at.is_none() {
                        ui.label(RichText::new("creado; el paciente aún no ha hecho nada").size(12.5).color(MUTED));
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Copiar enlace").clicked() {
                act = 1;
            }
            let can_mail = !email.is_empty() && app.db.mail.ready();
            let hint = if email.is_empty() {
                "El paciente no tiene correo en su ficha"
            } else if !app.db.mail.ready() {
                "Configure el correo en Configuración › Correo electrónico"
            } else {
                "Le envía el enlace y la cita para su calendario"
            };
            if ui.add_enabled(can_mail, egui::Button::new("Enviar por correo")).on_hover_text(hint).on_disabled_hover_text(hint).clicked() {
                act = 2;
            }
            if ui.button("Abrir").on_hover_text("Ver la página tal como la ve el paciente").clicked() {
                act = 3;
            }
        });
    });
    match act {
        1 => {
            let url = appt_url(app, appt);
            ui.ctx().copy_text(url);
            app.notify("Enlace copiado: péguelo en un WhatsApp o un correo al paciente.");
        }
        2 => send_link_mail(app, appt),
        3 => {
            let t = app.db.appt_token(appt);
            let url = crate::portal::link_url(&local_url(app), &t);
            app.changed();
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
        _ => {}
    }
}

// ───────────────────────────── documentos para firmar ─────────────────────────────

/// Enlace del documento (lo crea si hace falta).
pub fn doc_url(app: &mut App, doc_id: u32) -> String {
    // (al enviarlo se renueva su plazo: cambia aunque ya existiera)
    let t = app.db.doc_token(doc_id);
    app.changed();
    let base = if app.db.booking.public_url.trim().is_empty() { local_url(app) } else { app.db.booking.public_url.clone() };
    crate::doc_link::link_url(&base, &t)
}

/// Envía al paciente el enlace para leer y firmar el documento.
pub fn send_doc_mail(app: &mut App, doc_id: u32) {
    let Some(d) = app.db.documents.iter().find(|d| d.id == doc_id).cloned() else { return };
    let Some(p) = app.db.patient(d.patient_id).cloned() else { return };
    let url = doc_url(app, doc_id);
    let c = &app.db.config;
    let subject = format!("Documento para firmar: {} · {}", d.title, c.name);
    let text = format!(
        "Hola, {}:\n\nLe enviamos el documento «{}» para que lo lea con calma antes de su tratamiento. Desde este enlace puede leerlo, preguntarnos cualquier duda y firmarlo con el dedo desde el móvil:\n{url}\n\nSi lo prefiere, también puede firmarlo en la clínica.\n\nUn saludo,\n{}\n{}",
        p.first_name, d.title, c.name, c.phone
    );
    let to = p.email.clone();
    if let Some(l) = app.db.doc_links.iter_mut().find(|l| l.doc_id == doc_id) {
        l.sent_to = to.clone();
    }
    app.changed();
    send_mail(app, p.full_name(), to.clone(), subject, text, vec![]);
    app.notify(format!("Enviando el documento a {to}."));
}

/// Lo que el paciente ha hecho con el enlace de un documento.
fn doc_action(app: &mut App, doc_id: u32, action: crate::doc_link::DocAction) {
    use crate::doc_link::DocAction;
    let Some(d) = app.db.documents.iter().find(|d| d.id == doc_id).cloned() else { return };
    let who = app.db.patient_name(d.patient_id);
    match action {
        DocAction::Viewed => {
            if app.db.doc_viewed(doc_id) {
                app.changed();
            }
        }
        DocAction::Sign { signer, nif, strokes, hash, ip } => match app.db.doc_remote_sign(doc_id, &signer, &nif, strokes, &hash, &ip) {
            Ok(()) => {
                app.changed();
                app.notify(format!("{who} ha firmado por internet «{}».", d.title));
                // copia firmada para el paciente
                let email = app.db.patient(d.patient_id).map(|p| p.email.trim().to_string()).unwrap_or_default();
                if app.db.mail.ready()
                    && !email.is_empty()
                    && let Some(signed) = app.db.documents.iter().find(|x| x.id == doc_id).cloned()
                {
                    let html = crate::doc_link::doc_html(&app.db, &signed);
                    let c = &app.db.config;
                    let subject = format!("{} firmado · {}", d.title, c.name);
                    let text = format!("Hola:\n\nGracias por firmar «{}». Le adjuntamos su copia firmada.\n\nUn saludo,\n{}\n{}", d.title, c.name, c.phone);
                    let name = format!("{}_firmado.html", fold(&d.title).replace(|ch: char| !ch.is_ascii_alphanumeric(), "_"));
                    let files = vec![crate::mail::FileAttachment { name, mime: "text/html; charset=utf-8".into(), bytes: html.into_bytes() }];
                    send_mail(app, who.clone(), email, subject, text, files);
                }
            }
            Err(e) => app.warn(format!("{who} intentó firmar por internet «{}», pero no se pudo registrar: {e} Llámele para firmarlo en la clínica.", d.title)),
        },
        DocAction::Decline(why) => {
            if app.db.doc_remote_decline(doc_id, &why) {
                app.changed();
                app.notify(format!("{who} prefiere firmar «{}» en la clínica{}.", d.title, if why.trim().is_empty() { String::new() } else { format!(": «{}»", why.trim()) }));
            }
        }
        DocAction::Question(q) => {
            if app.db.doc_remote_question(doc_id, &q) {
                app.changed();
                app.notify(format!("Pregunta de {who} sobre «{}»: «{q}»", d.title));
            }
        }
    }
}

/// Franja del visor del documento: enviarlo para firmar y ver qué ha hecho el paciente.
pub fn doc_link_row(app: &mut App, ui: &mut Ui, doc_id: u32) {
    let link = app.db.doc_link(doc_id).cloned();
    let Some(d) = app.db.documents.iter().find(|d| d.id == doc_id).cloned() else { return };
    let email = app.db.patient(d.patient_id).map(|p| p.email.trim().to_string()).unwrap_or_default();
    let mut act = 0u8;
    egui::Frame::new().fill(Color32::from_rgb(250, 249, 255)).stroke(Stroke::new(1.0, tint(VIOLET, 0.2))).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
            crate::icons::settings_icon(ui.painter(), "web", r.center(), VIOLET);
            ui.label(RichText::new("Firma por internet").font(sbf(13.5)).color(HEADING));
            match &link {
                None => {
                    ui.label(RichText::new("el paciente lo lee y lo firma desde el móvil").size(12.5).color(MUTED));
                }
                Some(l) => {
                    if let Some(t) = l.signed.filter(|_| d.signed) {
                        pill(ui, &format!("Firmado por internet el {}", t.format("%d/%m %H:%M")), OK);
                    } else if d.signed {
                        pill(ui, "Firmado en la clínica", OK);
                    } else if let Some(t) = l.declined {
                        pill(ui, &format!("Prefiere firmarlo en la clínica ({})", t.format("%d/%m")), WARN);
                    } else if let Some(t) = l.viewed {
                        pill(ui, &format!("Lo leyó el {}", t.format("%d/%m %H:%M")), INFO);
                    } else if !l.sent_to.is_empty() {
                        pill(ui, &format!("Enviado a {}", l.sent_to), MUTED);
                    } else {
                        ui.label(RichText::new("enlace creado, aún sin abrir").size(12.5).color(MUTED));
                    }
                }
            }
        });
        if let Some(l) = &link {
            for (t, q) in l.questions.iter().rev().take(3) {
                ui.label(RichText::new(format!("❓ {} — «{q}»", t.format("%d/%m %H:%M"))).size(12.5).color(TEXT));
            }
            if l.declined.is_some() && !l.decline_reason.is_empty() {
                ui.label(RichText::new(format!("Comentario: «{}»", l.decline_reason)).size(12.5).color(MUTED));
            }
            if l.signed.is_some() && d.signed {
                ui.label(RichText::new(format!("Desde la dirección {} · huella {}…", if l.ip.is_empty() { "desconocida" } else { &l.ip }, l.doc_hash.chars().take(16).collect::<String>())).size(12.0).color(MUTED));
            }
        }
        if !d.signed {
            ui.horizontal(|ui| {
                if ui.button("Copiar enlace").clicked() {
                    act = 1;
                }
                let can_mail = !email.is_empty() && app.db.mail.ready();
                let hint = if email.is_empty() {
                    "El paciente no tiene correo en su ficha"
                } else if !app.db.mail.ready() {
                    "Configure el correo en Configuración › Correo electrónico"
                } else {
                    "Le envía el enlace para leerlo y firmarlo"
                };
                if ui.add_enabled(can_mail, egui::Button::new("Enviar para firmar")).on_hover_text(hint).on_disabled_hover_text(hint).clicked() {
                    act = 2;
                }
                if link.is_some() && ui.button("Abrir").on_hover_text("Ver la página tal como la ve el paciente").clicked() {
                    act = 3;
                }
            });
        }
    });
    match act {
        1 => {
            let url = doc_url(app, doc_id);
            ui.ctx().copy_text(url);
            app.notify("Enlace copiado: péguelo en un WhatsApp o un correo al paciente.");
        }
        2 => send_doc_mail(app, doc_id),
        3 => {
            let t = app.db.doc_token(doc_id);
            ui.ctx().open_url(egui::OpenUrl::new_tab(crate::doc_link::link_url(&local_url(app), &t)));
        }
        _ => {}
    }
}

// ───────────────────────────── presupuestos para firmar ─────────────────────────────

/// Enlace del presupuesto (lo crea si hace falta).
pub fn budget_url(app: &mut App, bid: u32) -> String {
    // (al enviarlo se renueva su plazo: cambia aunque ya existiera)
    let t = app.db.budget_token(bid);
    app.changed();
    let base = if app.db.booking.public_url.trim().is_empty() { local_url(app) } else { app.db.booking.public_url.clone() };
    crate::budget_link::link_url(&base, &t)
}

fn send_mail(app: &mut App, who: String, to: String, subject: String, text: String, files: Vec<crate::mail::FileAttachment>) {
    let cfg = app.db.mail.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::mail::send(&cfg, &to, &subject, &text, &files));
    });
    app.booking.mail_jobs.push((who, rx));
}

/// Envía al paciente el enlace para revisar y firmar el presupuesto.
pub fn send_budget_mail(app: &mut App, bid: u32) {
    let Some(b) = app.db.budgets.iter().find(|b| b.id == bid).cloned() else { return };
    let Some(p) = app.db.patient(b.patient_id).cloned() else { return };
    let url = budget_url(app, bid);
    let c = &app.db.config;
    let subject = format!("Su presupuesto de {}: {}", c.name, b.title);
    let text = format!(
        "Hola, {}:\n\nLe enviamos su presupuesto nº {} ({}) por un total de {}, válido hasta el {}.\n\nDesde este enlace puede revisarlo con calma, elegir cómo pagarlo y aceptarlo con su firma desde el móvil, o hacernos cualquier pregunta:\n{url}\n\nUn saludo,\n{}\n{}",
        p.first_name,
        b.id,
        b.title,
        money(b.total()),
        fmt_date(b.expires()),
        c.name,
        c.phone
    );
    let to = p.email.clone();
    if let Some(l) = app.db.budget_links.iter_mut().find(|l| l.budget_id == bid) {
        l.sent_to = to.clone();
    }
    app.changed();
    send_mail(app, p.full_name(), to.clone(), subject, text, vec![]);
    app.notify(format!("Enviando el presupuesto a {to}."));
}

/// Lo que el paciente ha hecho con el enlace de su presupuesto.
fn budget_action(app: &mut App, bid: u32, action: crate::budget_link::BudgetAction) {
    use crate::budget_link::BudgetAction;
    let Some(b) = app.db.budgets.iter().find(|b| b.id == bid).cloned() else { return };
    let who = app.db.patient_name(b.patient_id);
    match action {
        BudgetAction::Viewed => {
            if app.db.budget_viewed(bid) {
                app.changed();
            }
        }
        BudgetAction::Accept { signer, nif, strokes, installments, hash, ip } => match app.db.budget_remote_accept(bid, &signer, &nif, strokes, installments, &hash, &ip) {
            Ok(_) => {
                app.changed();
                let pay = installments.to_string();
                app.notify(format!("{who} ha aceptado y firmado por internet el presupuesto nº {bid} ({}){}.", money(b.total()), if installments > 0 { format!(", en {pay} cuotas") } else { String::new() }));
                // copia firmada para el paciente
                let email = app.db.patient(b.patient_id).map(|p| p.email.trim().to_string()).unwrap_or_default();
                if app.db.mail.ready()
                    && !email.is_empty()
                    && let Some(html) = crate::budget_link::signed_html(&app.db, bid)
                {
                    let c = &app.db.config;
                    let subject = format!("Presupuesto nº {bid} aceptado · {}", c.name);
                    let text = format!("Hola:\n\nGracias por aceptar el presupuesto nº {bid} ({}). Le adjuntamos la copia firmada. Le llamaremos para dar las citas.\n\nUn saludo,\n{}\n{}", b.title, c.name, c.phone);
                    let files = vec![crate::mail::FileAttachment { name: format!("presupuesto_{bid}_firmado.html"), mime: "text/html; charset=utf-8".into(), bytes: html.into_bytes() }];
                    send_mail(app, who.clone(), email, subject, text, files);
                }
            }
            Err(e) => app.warn(format!("{who} intentó aceptar por internet el presupuesto nº {bid}, pero no se pudo registrar: {e} Llámele para confirmarlo.")),
        },
        BudgetAction::Reject(why) => {
            if app.db.budget_remote_reject(bid, &why) {
                app.changed();
                app.notify(format!("{who} ha indicado por internet que no le interesa el presupuesto nº {bid}{}.", if why.trim().is_empty() { String::new() } else { format!(": «{}»", why.trim()) }));
            }
        }
        BudgetAction::Question(q) => {
            if app.db.budget_remote_question(bid, &q) {
                app.changed();
                app.notify(format!("Pregunta de {who} sobre el presupuesto nº {bid}: «{q}»"));
            }
        }
    }
}

/// Franja de la ventana del presupuesto: enviarlo para firmar y ver qué ha hecho el paciente.
pub fn budget_link_row(app: &mut App, ui: &mut Ui, bid: u32) {
    let link = app.db.budget_link(bid).cloned();
    let email = app.db.budgets.iter().find(|b| b.id == bid).and_then(|b| app.db.patient(b.patient_id)).map(|p| p.email.trim().to_string()).unwrap_or_default();
    let pending = app.db.budgets.iter().any(|b| b.id == bid && b.state == BudgetState::Pendiente);
    let mut act = 0u8;
    egui::Frame::new().fill(Color32::from_rgb(250, 249, 255)).stroke(Stroke::new(1.0, tint(VIOLET, 0.2))).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
            crate::icons::settings_icon(ui.painter(), "web", r.center(), VIOLET);
            ui.label(RichText::new("Firma por internet").font(sbf(13.5)).color(HEADING));
            match &link {
                None => {
                    ui.label(RichText::new("el paciente lo revisa y lo firma desde el móvil").size(12.5).color(MUTED));
                }
                Some(l) => {
                    if l.accepted {
                        pill(ui, &format!("Firmado por {} el {}", l.signer, l.decided.map(|t| t.format("%d/%m %H:%M").to_string()).unwrap_or_default()), OK);
                        if l.installments > 0 {
                            pill(ui, &format!("{} cuotas", l.installments), INFO);
                        }
                    } else if l.decided.is_some() {
                        pill(ui, "No le interesa", DANGER);
                    } else if let Some(t) = l.viewed {
                        pill(ui, &format!("Lo vio el {}", t.format("%d/%m %H:%M")), INFO);
                    } else if !l.sent_to.is_empty() {
                        pill(ui, &format!("Enviado a {}", l.sent_to), MUTED);
                    } else {
                        ui.label(RichText::new("enlace creado, aún sin abrir").size(12.5).color(MUTED));
                    }
                }
            }
        });
        if let Some(l) = &link {
            for (t, q) in l.questions.iter().rev().take(3) {
                ui.label(RichText::new(format!("❓ {} — «{q}»", t.format("%d/%m %H:%M"))).size(12.5).color(TEXT));
            }
            if !l.accepted && l.decided.is_some() && !l.reason.is_empty() {
                ui.label(RichText::new(format!("Motivo: «{}»", l.reason)).size(12.5).color(MUTED));
            }
        }
        ui.horizontal(|ui| {
            if pending {
                if ui.button("Copiar enlace").clicked() {
                    act = 1;
                }
                let can_mail = !email.is_empty() && app.db.mail.ready();
                let hint = if email.is_empty() {
                    "El paciente no tiene correo en su ficha"
                } else if !app.db.mail.ready() {
                    "Configure el correo en Configuración › Correo electrónico"
                } else {
                    "Le envía el enlace para revisarlo y firmarlo"
                };
                if ui.add_enabled(can_mail, egui::Button::new("Enviar por correo")).on_hover_text(hint).on_disabled_hover_text(hint).clicked() {
                    act = 2;
                }
            }
            if link.is_some() && ui.button("Abrir").on_hover_text("Ver la página tal como la ve el paciente").clicked() {
                act = 3;
            }
            if link.as_ref().is_some_and(|l| l.accepted) && ui.button("Ver la copia firmada").clicked() {
                act = 4;
            }
        });
    });
    match act {
        1 => {
            let url = budget_url(app, bid);
            ui.ctx().copy_text(url);
            app.notify("Enlace copiado: péguelo en un WhatsApp o un correo al paciente.");
        }
        2 => send_budget_mail(app, bid),
        3 => {
            let t = app.db.budget_token(bid);
            ui.ctx().open_url(egui::OpenUrl::new_tab(crate::budget_link::link_url(&local_url(app), &t)));
        }
        4 => {
            if let Some(html) = crate::budget_link::signed_html(&app.db, bid)
                && let Err(e) = crate::screens::centros::open_html(ui.ctx(), &format!("presupuesto_{bid}_firmado.html"), html)
            {
                app.warn(e);
            }
        }
        _ => {}
    }
}

// ───────────────────────────── huecos libres para la lista de espera ─────────────────────────────

/// Ofrece a la lista de espera el hueco de una cita anulada. Devuelve un resumen para recepción.
pub fn offer_freed_slot(app: &mut App, appt_id: u32) -> Result<String, String> {
    let a = app.db.appointments.iter().find(|a| a.id == appt_id).cloned().ok_or("La cita ya no existe.")?;
    offer_slot(app, a.doctor_id, a.date, a.start, a.minutes, a.center_id, Some(appt_id))
}

/// Crea la oferta, prepara los avisos y envía ya los de correo (los de WhatsApp o SMS quedan en Comunicaciones).
pub fn offer_slot(app: &mut App, doctor: u32, date: chrono::NaiveDate, start: chrono::NaiveTime, minutes: u32, center: u32, source: Option<u32>) -> Result<String, String> {
    let id = app.db.create_offer(crate::slot_offer::FreeSlot { doctor, date, start, minutes, center }, source, now())?;
    let msgs = app.db.offer_messages(id);
    app.changed();
    let emails: Vec<u32> = msgs.iter().copied().filter(|m| app.db.messages.iter().any(|x| x.id == *m && x.channel == crate::comms::Channel::Email)).collect();
    let others = msgs.len() - emails.len();
    let invited = app.db.slot_offers.iter().find(|o| o.id == id).map(|o| o.invites.len()).unwrap_or(0);
    let mut by_mail = 0;
    if app.db.mail.ready() && !emails.is_empty() {
        by_mail = emails.len();
        crate::screens::comunicaciones::start_outbox(app, emails, false);
    }
    let mut parts = vec![];
    if by_mail > 0 {
        parts.push(format!("{by_mail} por correo"));
    }
    if others > 0 {
        parts.push(format!("{others} por WhatsApp o SMS (envíelos desde Comunicaciones)"));
    }
    let unreachable = invited - msgs.len();
    if unreachable > 0 {
        parts.push(format!("{unreachable} sin forma de avisar (sin consentimiento o sin contacto)"));
    }
    Ok(format!("Hueco ofrecido a {invited} paciente(s) de la lista de espera{}.", if parts.is_empty() { String::new() } else { format!(": {}", parts.join(", ")) }))
}

/// Lo que el paciente ha hecho con el enlace del hueco libre.
fn offer_action(app: &mut App, token: &str, action: crate::slot_offer::OfferAction) {
    use crate::slot_offer::OfferAction;
    let Some(o) = app.db.slot_offers.iter().find(|o| o.invites.iter().any(|i| i.token == token)).cloned() else { return };
    let Some(inv) = o.invites.iter().find(|i| i.token == token).cloned() else { return };
    let who = app.db.patient_name(inv.patient_id);
    let when = match (o.date, o.start) {
        (Some(d), Some(s)) => format!("{} a las {}", day_label(d), s.format("%H:%M")),
        _ => String::new(),
    };
    match action {
        OfferAction::Viewed => {
            if app.db.offer_viewed(token) {
                app.changed();
            }
        }
        OfferAction::Decline => {
            if app.db.decline_offer(token) {
                app.changed();
            }
        }
        OfferAction::Take => match app.db.take_offer(token, now()) {
            Ok(appt) => {
                app.changed();
                app.notify(format!("{who} se ha quedado el hueco libre del {when} (lista de espera): cita confirmada en la agenda."));
                // confirmación por correo, con la cita para el calendario y su enlace
                if app.db.mail.ready()
                    && app.db.can_email(inv.patient_id)
                    && let Some(a) = app.db.appointments.iter().find(|a| a.id == appt).cloned()
                {
                    let link = app.db.appt_link_url(appt).map(|u| format!("\n\nDesde este enlace puede confirmar, cambiar o anular la cita:\n{u}")).unwrap_or_default();
                    let c = &app.db.config;
                    let doctor = app.db.doctor(a.doctor_id).map(|d| d.alias.clone()).unwrap_or_default();
                    let first = app.db.patient(inv.patient_id).map(|p| p.first_name.clone()).unwrap_or_default();
                    let subject = format!("Cita confirmada en {}: {} a las {}", c.name, day_label(a.date), a.start.format("%H:%M"));
                    let text = format!(
                        "Hola, {first}:\n\nEl hueco es suyo. Su cita queda confirmada:\n\n  {} a las {}\n  con {doctor}\n  {}{link}\n\nSi no puede venir, avísenos llamando al {}.\n\nUn saludo,\n{}",
                        long_date(a.date),
                        a.start.format("%H:%M"),
                        c.address,
                        c.phone,
                        c.name
                    );
                    let ics = ics_event(a.id, &c.name, &c.address, &c.phone, a.date, a.start, a.minutes, &format!("Cita con {doctor}"));
                    let to = app.db.patient(inv.patient_id).map(|p| p.email.clone()).unwrap_or_default();
                    send_mail(app, who.clone(), to, subject, text, vec![crate::mail::FileAttachment { name: "cita.ics".into(), mime: "text/calendar; charset=utf-8".into(), bytes: ics.into_bytes() }]);
                }
            }
            Err(e) => app.warn(format!("{who} quiso el hueco libre del {when}, pero no se pudo reservar: {e} Llámele para buscarle otro.")),
        },
    }
}

/// Parte de la tarjeta «Lista de espera» de la agenda: huecos que han quedado libres y ofertas en curso.
pub fn offers_card(app: &mut App, ui: &mut Ui) {
    use crate::slot_offer::OfferState;
    let now = now();
    let freed = app.db.freed_slots(now);
    let mut offers: Vec<crate::slot_offer::SlotOffer> = app.db.slot_offers.iter().filter(|o| o.date.is_some_and(|d| d + chrono::Duration::days(2) >= app.today)).cloned().collect();
    offers.sort_by_key(|o| std::cmp::Reverse(o.created));
    if freed.is_empty() && offers.is_empty() {
        return;
    }
    let mut offer: Option<u32> = None;
    let mut withdraw: Option<u32> = None;
    ui.add_space(6.0);
    ui.separator();
    if !freed.is_empty() {
        ui.label(RichText::new("Huecos que han quedado libres").font(sbf(13.5)).color(HEADING));
        for (a, n) in freed.iter().take(5) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} {} · {} · {}", weekday_es(a.date).chars().take(3).collect::<String>(), a.date.format("%d/%m"), a.start.format("%H:%M"), app.db.doctor_alias(a.doctor_id))).size(13.0).color(TEXT));
                right_aligned(ui, |ui| {
                    if ui
                        .add(egui::Button::new(RichText::new(format!("Ofrecer ({n})")).size(12.5).color(Color32::WHITE)).fill(VIOLET).corner_radius(CornerRadius::same(8)))
                        .on_hover_text(format!("Avisa a {n} paciente(s) de la lista de espera; se lo queda el primero que lo pida"))
                        .clicked()
                    {
                        offer = Some(a.id);
                    }
                });
            });
        }
    }
    if !offers.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new("Huecos ofrecidos").font(sbf(13.5)).color(HEADING));
        for o in offers.iter().take(5) {
            let (Some(d), Some(s)) = (o.date, o.start) else { continue };
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{} {} · {} · {}", weekday_es(d).chars().take(3).collect::<String>(), d.format("%d/%m"), s.format("%H:%M"), app.db.doctor_alias(o.doctor_id))).size(13.0).color(TEXT));
                match o.state {
                    OfferState::Abierta => {
                        let seen = o.invites.iter().filter(|i| i.viewed.is_some()).count();
                        pill(ui, &format!("Ofrecido a {}{}", o.invites.len(), if seen > 0 { format!(" · {seen} lo han visto") } else { String::new() }), VIOLET);
                        if ui.small_button("Retirar").on_hover_text("Ya no se ofrece (por ejemplo, porque lo ha dado por teléfono)").clicked() {
                            withdraw = Some(o.id);
                        }
                    }
                    OfferState::Ocupada => {
                        let txt = match o.taken_by {
                            Some(pid) => format!("Lo ha cogido {}", app.db.patient_name(pid)),
                            None => "Ocupado".into(),
                        };
                        pill(ui, &txt, OK);
                    }
                    st => {
                        pill(ui, st.label(), MUTED);
                    }
                }
            });
        }
    }
    if let Some(id) = offer {
        match offer_freed_slot(app, id) {
            Ok(s) => app.notify(s),
            Err(e) => app.warn(e),
        }
    }
    if let Some(id) = withdraw {
        app.db.withdraw_offer(id);
        app.changed();
    }
}

/// Lo que el paciente ha hecho desde el enlace de su cita.
fn link_action(app: &mut App, appt: u32, action: ApptAction, holds: &mut Vec<Busy>) {
    let Some(a) = app.db.appointments.iter().find(|a| a.id == appt).cloned() else { return };
    let p = a.patient_id.and_then(|id| app.db.patient(id)).cloned();
    let who = p.as_ref().map(|p| p.full_name()).unwrap_or_else(|| a.free_name.clone());
    let when = format!("{} a las {}", day_label(a.date), a.start.format("%H:%M"));
    match action {
        ApptAction::Confirm => {
            if app.db.portal_confirm(appt) {
                app.changed();
                app.notify(format!("{who} ha confirmado por internet su cita del {when}."));
            }
        }
        ApptAction::Cancel(why) => {
            if app.db.portal_cancel(appt, &why) {
                app.changed();
                // el hueco, a la lista de espera (si la clínica lo quiere así)
                let offered = if app.db.booking.offer_slots { offer_freed_slot(app, appt).ok() } else { None };
                let waiting = app.db.waitlist.iter().filter(|w| w.doctor_id == a.doctor_id).count();
                let tail = match offered {
                    Some(s) => format!(" {s}"),
                    None if waiting > 0 => format!(" Hay {waiting} paciente(s) en lista de espera con ese profesional."),
                    None => String::new(),
                };
                // la señal pagada: se devuelve o se retiene según la política
                let deposit = app.db.deposit_on_cancel(appt, true, now()).map(|m| format!(" {m}")).unwrap_or_default();
                app.notify(format!("{who} ha anulado por internet su cita del {when}.{tail}{deposit}"));
            }
        }
        ApptAction::Change(inc) => {
            holds.push(inc.hold);
            let mut r = inc.request;
            r.patient_id = a.patient_id;
            r.name = who.clone();
            r.phone = p.as_ref().map(|p| if p.mobile.trim().is_empty() { p.phone.clone() } else { p.mobile.clone() }).unwrap_or_else(|| a.phone.clone());
            r.email = p.as_ref().map(|p| p.email.clone()).unwrap_or_default();
            r.code = a.treatment_code.clone();
            r.center_id = cid(a.center_id);
            r.notes = format!("Quiere cambiar su cita del {when}.");
            let new = format!("{} a las {}", day_label(r.date), r.start.format("%H:%M"));
            let id = app.db.booking_receive(r);
            app.changed();
            if app.db.booking.auto_confirm {
                confirm(app, id);
            } else {
                app.notify(format!("{who} pide por internet cambiar su cita del {when} al {new}."));
            }
        }
        ApptAction::Health(form) => {
            if app.db.portal_health(appt, form) {
                app.changed();
                app.notify(format!("{who} ha rellenado por internet el cuestionario de salud."));
            }
        }
    }
}

fn ago(t: chrono::NaiveDateTime) -> String {
    let m = (now() - t).num_minutes();
    match m {
        ..=0 => "ahora".into(),
        1..=59 => format!("hace {m} min"),
        60..=1439 => format!("hace {} h", m / 60),
        _ => format!("el {}", t.format("%d/%m")),
    }
}

/// Tarjeta de la agenda con las citas pedidas por internet que esperan respuesta.
pub fn agenda_card(app: &mut App, ui: &mut Ui) {
    let mut list: Vec<BookingRequest> = app.db.booking_requests.iter().filter(|r| r.state == ReqState::Pendiente).cloned().collect();
    if list.is_empty() {
        return;
    }
    list.sort_by_key(|r| (r.date, r.start));
    let mut action: Option<(u32, u8)> = None;
    egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, tint(VIOLET, 0.35))).corner_radius(CornerRadius::same(16)).inner_margin(egui::Margin::same(16)).shadow(shadow()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), egui::Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(9), tint(VIOLET, 0.12));
            crate::icons::settings_icon(ui.painter(), "web", r.center(), VIOLET);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("Citas pedidas por internet").font(sbf(15.0)).color(HEADING));
                ui.label(RichText::new(format!("{} por confirmar", list.len())).font(mdf(12.5)).color(MUTED));
            });
        });
        for r in list.iter().take(6) {
            ui.add_space(8.0);
            egui::Frame::new().fill(Color32::from_rgb(250, 249, 255)).stroke(Stroke::new(1.0, tint(VIOLET, 0.16))).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&r.name).font(sbf(14.0)).color(HEADING));
                    right_aligned(ui, |ui| {
                        ui.label(RichText::new(ago(r.received)).size(11.5).color(SUBTLE));
                    });
                });
                let doc = app.db.doctor(r.doctor_id).map(|d| d.alias.clone()).unwrap_or_default();
                if let Some(old) = r.replaces.and_then(|id| app.db.appointments.iter().find(|a| a.id == id)) {
                    ui.horizontal(|ui| {
                        pill(ui, "Cambio de hora", INFO);
                        ui.label(RichText::new(format!("antes: {} {} · {}", weekday_es(old.date).chars().take(3).collect::<String>(), old.date.format("%d/%m"), old.start.format("%H:%M"))).size(12.0).color(MUTED).strikethrough());
                    });
                }
                ui.label(RichText::new(format!("{} {} · {}", weekday_es(r.date).chars().take(3).collect::<String>(), r.date.format("%d/%m"), r.start.format("%H:%M"))).font(sbf(13.5)).color(mix(VIOLET, HEADING, 0.35)));
                ui.label(RichText::new(format!("{} · {doc} · {} min", r.reason, r.minutes)).size(12.5).color(TEXT));
                let who = match r.patient_id {
                    Some(pid) => format!("Ficha: {}", app.db.patient_name(pid)),
                    None if r.first_visit => "Paciente nuevo (primera visita)".into(),
                    None => "Sin ficha con ese teléfono".into(),
                };
                ui.label(RichText::new(format!("{} · {}", r.phone, who)).size(12.0).color(MUTED));
                if !r.notes.trim().is_empty() && r.replaces.is_none() {
                    ui.label(RichText::new(format!("«{}»", r.notes.trim())).size(12.0).italics().color(TEXT));
                }
                if !app.db.is_free(r.doctor_id, r.date, r.start, r.end(), r.replaces) {
                    ui.label(RichText::new("⚠ Esa hora ya está ocupada en la agenda").size(12.0).color(DANGER));
                }
                if app.booking.rejecting == Some(r.id) {
                    ui.add(egui::TextEdit::singleline(&mut app.booking.reject_text).hint_text("Motivo (se le dirá al paciente)").desired_width(f32::INFINITY));
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new("Rechazar solicitud").color(Color32::WHITE).size(13.0)).fill(DANGER).corner_radius(CornerRadius::same(8))).clicked() {
                            action = Some((r.id, 2));
                        }
                        if ui.button("Cancelar").clicked() {
                            action = Some((r.id, 3));
                        }
                    });
                } else {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new("Confirmar").color(Color32::WHITE).font(sbf(13.0))).fill(ACCENT).corner_radius(CornerRadius::same(8))).on_hover_text("Apunta la cita en la agenda y avisa al paciente").clicked() {
                            action = Some((r.id, 0));
                        }
                        if ui.button(RichText::new("Ver el día").size(13.0)).clicked() {
                            action = Some((r.id, 4));
                        }
                        if ui.button(RichText::new("Rechazar").size(13.0).color(DANGER)).clicked() {
                            action = Some((r.id, 1));
                        }
                    });
                }
            });
        }
        if list.len() > 6 {
            ui.add_space(4.0);
            muted(ui, format!("Y {} más: véalas en Configuración › Cita online.", list.len() - 6));
        }
    });
    ui.add_space(8.0);
    if let Some((id, a)) = action {
        match a {
            0 => confirm(app, id),
            1 => {
                app.booking.rejecting = Some(id);
                app.booking.reject_text.clear();
            }
            2 => {
                let why = app.booking.reject_text.clone();
                reject(app, id, &why);
            }
            3 => app.booking.rejecting = None,
            _ => {
                if let Some(r) = app.db.booking_requests.iter().find(|r| r.id == id) {
                    app.agenda.date = r.date;
                    app.agenda.view = crate::screens::agenda::View::Dia;
                    app.agenda.hidden.retain(|d| *d != r.doctor_id);
                }
            }
        }
    }
}

/// Configuración › Cita online.
pub fn panel(app: &mut App, ui: &mut Ui) {
    let before = serde_json::to_string(&app.db.booking).unwrap_or_default();
    let me = me();
    let running = app.booking.server.is_some();
    let url = patient_url(app);
    let local = local_url(app);
    let stats = app.booking.server.as_ref().map(|s| s.shared.lock().map(|g| (g.received, g.refused)).unwrap_or_default());
    let docs: Vec<(u32, String, String)> = app.db.collaborators.iter().filter(|d| d.active).map(|d| (d.id, d.alias.clone(), d.specialty.clone())).collect();
    let mut poster = false;
    let mut open_page = false;
    let mut goto_mail = false;
    ui.columns(2, |cols| {
        // ── página ──
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            let c = &mut app.db.booking;
            ui.horizontal(|ui| {
                section(ui, "Página de cita online");
                right_aligned(ui, |ui| {
                    if !c.enabled {
                        pill(ui, "Desactivada", MUTED);
                    } else if c.station != me {
                        pill(ui, "En otro ordenador", INFO);
                    } else if running {
                        pill(ui, "Publicada", OK);
                    } else {
                        pill(ui, "Sin abrir", DANGER);
                    }
                });
            });
            muted(ui, "Los pacientes eligen motivo, profesional, día y hora entre los huecos libres de la agenda. La página sólo muestra horas libres: nunca nombres ni datos de otros pacientes.");
            ui.add_space(6.0);
            let was = c.enabled;
            ui.checkbox(&mut c.enabled, RichText::new("Permitir pedir cita por internet").font(sbf(14.0)));
            if c.enabled && !was && c.station.trim().is_empty() {
                c.station = me.clone();
            }
            if c.enabled && c.station != me {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("La publica el ordenador «{}».", c.station)).size(13.0).color(TEXT));
                    if ui.button("Publicar desde este ordenador").clicked() {
                        c.station = me.clone();
                    }
                });
            }
            if let Some(e) = &app.booking.error {
                ui.label(RichText::new(e).size(12.5).color(DANGER));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Puerto").font(mdf(12.8)).color(MUTED));
                    ui.add(egui::DragValue::new(&mut c.port).range(1024..=65535));
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("En este ordenador").font(mdf(12.8)).color(MUTED));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&local).font(mdf(13.5)).color(HEADING));
                        if ui.add_enabled(running, egui::Button::new("Abrir la página")).clicked() {
                            open_page = true;
                        }
                    });
                });
            });
            ui.checkbox(&mut c.lan, "Atender también a otros equipos de la red (túnel o servidor web en otro ordenador)");
            ui.add_space(4.0);
            let w = (ui.available_width() / 1.1 - 4.0).min(460.0);
            field(ui, "Dirección para los pacientes (https)", &mut c.public_url, w);
            ui.label(RichText::new("Para abrirla desde casa hace falta una dirección https que llegue a este ordenador: un túnel (Cloudflare Tunnel, ngrok…) o el servidor de la web de la clínica. Sirve el mismo túnel que para WhatsApp. Póngala como enlace «Pedir cita» en su web, en Google o en las redes.").size(12.0).color(MUTED));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                crate::screens::verifactu::draw_qr(ui, &url, 112.0);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Enlace para los pacientes").font(sbf(13.5)).color(HEADING));
                    ui.label(RichText::new(&url).size(12.5).color(ACCENT_DARK));
                    ui.add_space(4.0);
                    if ui.button("Copiar el enlace").clicked() {
                        ui.ctx().copy_text(url.clone());
                    }
                    if ui.button("Cartel con el QR").on_hover_text("Cartel para imprimir y poner en el mostrador o la sala de espera").clicked() {
                        poster = true;
                    }
                });
            });
            if let Some((rec, refu)) = stats {
                ui.add_space(4.0);
                muted(ui, format!("Desde que se abrió: {rec} solicitudes recibidas · {refu} rechazadas por el sistema (datos incorrectos, hora ya ocupada o envíos repetidos)."));
            }
        });
        cols[0].add_space(8.0);
        // ── publicar en internet (túnel de Cloudflare) ──
        tunnel_card(app, &mut cols[0]);
        cols[0].add_space(8.0);
        // ── normas ──
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            let c = &mut app.db.booking;
            section(ui, "Normas");
            ui.horizontal(|ui| {
                lab(ui, 150.0, RichText::new("Plazo máximo").color(TEXT));
                ui.add(egui::DragValue::new(&mut c.days_ahead).range(1..=180).suffix(" días"));
                ui.label(RichText::new("por delante").color(MUTED));
            });
            ui.horizontal(|ui| {
                lab(ui, 150.0, RichText::new("Antelación mínima").color(TEXT));
                ui.add(egui::DragValue::new(&mut c.min_notice_hours).range(0..=168).suffix(" h"));
                ui.label(RichText::new("antes de la cita").color(MUTED));
            });
            ui.add_space(4.0);
            ui.label(RichText::new("Profesionales que se ofrecen").font(mdf(12.8)).color(MUTED));
            ui.horizontal_wrapped(|ui| {
                for (id, alias, _) in &docs {
                    let on = c.doctors.is_empty() || c.doctors.contains(id);
                    let b = egui::Button::new(RichText::new(alias).size(13.0).color(if on { ACCENT_DARK } else { MUTED })).fill(if on { ACCENT_SOFT } else { CARD }).stroke(Stroke::new(1.0, if on { ACCENT } else { BORDER })).corner_radius(CornerRadius::same(14));
                    if ui.add(b).clicked() {
                        if c.doctors.is_empty() {
                            c.doctors = docs.iter().map(|d| d.0).filter(|d| d != id).collect();
                        } else if on {
                            c.doctors.retain(|d| d != id);
                        } else {
                            c.doctors.push(*id);
                        }
                        if docs.iter().all(|d| c.doctors.contains(&d.0)) {
                            c.doctors.clear();
                        }
                    }
                }
            });
            ui.add_space(6.0);
            ui.checkbox(&mut c.auto_confirm, "Confirmar las citas sin revisarlas (se apuntan solas en la agenda)");
            if !c.auto_confirm {
                ui.label(RichText::new("Las solicitudes aparecen en la agenda para confirmarlas o rechazarlas con un clic, y su hueco queda reservado mientras tanto.").size(12.0).color(MUTED));
            }
            ui.checkbox(&mut c.email_patient, "Avisar al paciente por correo al confirmar o rechazar (con la cita para su calendario)");
            if c.email_patient && !app.db.mail.ready() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("El correo aún no está configurado.").size(12.0).color(WARN));
                    if ui.link(RichText::new("Configurar el correo ›").size(12.0)).clicked() {
                        goto_mail = true;
                    }
                });
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Texto de bienvenida").font(mdf(12.8)).color(MUTED));
            ui.add(egui::TextEdit::multiline(&mut c.intro).desired_rows(2).desired_width(f32::INFINITY));
        });
        cols[0].add_space(8.0);
        // ── enlace de cada cita ──
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            let c = &mut app.db.booking;
            section(ui, "Enlace personal de cada cita");
            muted(ui, "Cada cita tiene un enlace secreto («Mi cita») con el que el paciente confirma que irá, anula, pide otra hora o rellena el cuestionario de salud desde casa, sin usuario ni contraseña. Va en el correo de confirmación, en los recordatorios con [ENLACE] y se puede copiar desde la cita.");
            ui.add_space(4.0);
            ui.checkbox(&mut c.links, "Dar a cada cita su enlace personal");
            ui.checkbox(&mut c.links_in_messages, "Añadir los enlaces a los recordatorios, avisos de revisión y seguimientos de presupuesto (correo y WhatsApp)");
            ui.add_enabled_ui(c.links, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Se puede anular o cambiar hasta");
                    ui.add(egui::DragValue::new(&mut c.cancel_hours).range(0..=168).suffix(" h"));
                    ui.label("antes (después, llamando)");
                });
                ui.checkbox(&mut c.ask_health, "Pedir el cuestionario de salud a quien no lo tiene o lo tiene de hace más de un año");
                ui.label(RichText::new("Las respuestas llegan a la ficha sin firmar, para revisarlas con el paciente; las de riesgo (anticoagulantes, alergias…) se convierten en avisos. La página nunca muestra respuestas anteriores.").size(12.0).color(MUTED));
            });
            if c.links && c.public_url.trim().is_empty() {
                ui.label(RichText::new("Falta la dirección para los pacientes (https): sin ella el enlace sólo abre en este ordenador.").size(12.0).color(WARN));
            }
        });
        // ── motivos ──
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            let c = &mut app.db.booking;
            ui.horizontal(|ui| {
                section(ui, "Qué se puede pedir");
                right_aligned(ui, |ui| {
                    if ui.button("＋ Añadir motivo").clicked() {
                        c.reasons.push(BookReason { name: "Nuevo motivo".into(), ..Default::default() });
                    }
                });
            });
            muted(ui, "Cada motivo con su duración y quién lo atiende (ninguno marcado = cualquiera de los que se ofrecen).");
            let mut remove = None;
            for (i, r) in c.reasons.iter_mut().enumerate() {
                ui.add_space(6.0);
                egui::Frame::new().fill(if r.active { ROW_ALT } else { CARD }).stroke(Stroke::new(1.0, HAIRLINE)).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut r.active, "").on_hover_text("Se ofrece en la página");
                        ui.add(egui::TextEdit::singleline(&mut r.name).desired_width((ui.available_width() - 16.0).max(80.0)));
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.add(egui::DragValue::new(&mut r.minutes).range(5..=240).speed(1.0).suffix(" min"));
                        ui.add_space(6.0);
                        for (id, alias, _) in &docs {
                            let on = r.doctors.contains(id);
                            let b = egui::Button::new(RichText::new(alias).size(12.0).color(if on { Color32::WHITE } else { MUTED })).fill(if on { ACCENT } else { CARD }).stroke(Stroke::new(1.0, if on { ACCENT } else { BORDER })).corner_radius(CornerRadius::same(10));
                            if ui.add(b).clicked() {
                                if on {
                                    r.doctors.retain(|d| d != id);
                                } else {
                                    r.doctors.push(*id);
                                }
                            }
                        }
                        ui.add_space(6.0);
                        if ui.button("🗑").on_hover_text("Quitar este motivo").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
            if let Some(i) = remove {
                c.reasons.remove(i);
            }
        });
        cols[1].add_space(8.0);
        // ── presupuestos para firmar ──
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            let c = &mut app.db.booking;
            section(ui, "Firmar por internet");
            muted(ui, "Presupuestos, consentimientos y documentos se envían con un enlace: el paciente los lee en el móvil, pregunta sus dudas y los firma con el dedo. Quedan firmados en la ficha y le llega una copia por correo.");
            ui.add_space(4.0);
            ui.checkbox(&mut c.docs, "Permitir firmar consentimientos y documentos por internet");
            ui.checkbox(&mut c.budgets, "Permitir aceptar y firmar presupuestos por internet");
            ui.add_enabled_ui(c.budgets, |ui| {
                ui.label("Cuotas sin intereses que se ofrecen");
                ui.horizontal_wrapped(|ui| {
                    for n in [2u32, 3, 4, 6, 10, 12, 18, 24] {
                        let on = c.installment_options.contains(&n);
                        let b = egui::Button::new(RichText::new(n.to_string()).size(13.0).color(if on { Color32::WHITE } else { MUTED })).fill(if on { ACCENT } else { CARD }).stroke(Stroke::new(1.0, if on { ACCENT } else { BORDER })).corner_radius(CornerRadius::same(10)).min_size(vec2(34.0, 26.0));
                        if ui.add(b).clicked() {
                            if on {
                                c.installment_options.retain(|x| *x != n);
                            } else {
                                c.installment_options.push(n);
                                c.installment_options.sort();
                            }
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("A partir de");
                    ui.add(egui::DragValue::new(&mut c.finance_min).range(0.0..=100000.0).speed(10.0).suffix(" €"));
                    ui.label(RichText::new("de presupuesto").color(MUTED));
                });
                ui.label(RichText::new("Si el presupuesto ya tiene un plan de financiación, también se ofrece ese.").size(12.0).color(MUTED));
            });
        });
        cols[1].add_space(8.0);
        // ── huecos libres para la lista de espera ──
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            let c = &mut app.db.booking;
            section(ui, "Huecos libres para la lista de espera");
            muted(ui, "Cuando se anula una cita, el hueco se ofrece a los pacientes que esperan con ese profesional y a quienes les encaja la hora (mañanas o tardes), empezando por quien más lleva esperando. Se lo queda el primero que lo pide desde su enlace; sin página publicada, el aviso les pide que llamen.");
            ui.add_space(4.0);
            ui.checkbox(&mut c.offer_slots, "Ofrecerlo automáticamente cuando un paciente anula su cita por internet");
            ui.horizontal_wrapped(|ui| {
                lab(ui, 210.0, RichText::new("Pacientes avisados por hueco").color(TEXT));
                ui.add(egui::DragValue::new(&mut c.offer_max).range(1..=30));
            });
            ui.horizontal_wrapped(|ui| {
                lab(ui, 210.0, RichText::new("Sólo con al menos").color(TEXT));
                ui.add(egui::DragValue::new(&mut c.offer_min_hours).range(1..=72).suffix(" h"));
                ui.label(RichText::new("de margen").color(MUTED));
            });
            ui.label(RichText::new("Los huecos de anulaciones hechas en la clínica se ofrecen con un botón desde la agenda («Lista de espera»).").size(12.0).color(MUTED));
        });
        cols[1].add_space(8.0);
        // ── historial ──
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Últimas solicitudes");
            let mut list: Vec<BookingRequest> = app.db.booking_requests.to_vec();
            list.sort_by_key(|r| std::cmp::Reverse(r.received));
            if list.is_empty() {
                muted(ui, "Aún no ha llegado ninguna.");
            }
            for r in list.iter().take(12) {
                ui.horizontal(|ui| {
                    let col = match r.state {
                        ReqState::Pendiente => VIOLET,
                        ReqState::Confirmada => OK,
                        ReqState::Rechazada => DANGER,
                    };
                    pill(ui, if r.replaces.is_some() && r.state == ReqState::Pendiente { "Cambio" } else { r.state.label() }, col);
                    ui.add(egui::Label::new(RichText::new(format!("{} · {} {}", r.name, r.date.format("%d/%m"), r.start.format("%H:%M"))).size(13.0).color(TEXT)).truncate())
                        .on_hover_text(format!("{} · recibida {}", r.reason, ago(r.received)));
                });
            }
        });
    });
    if open_page {
        ui.ctx().open_url(egui::OpenUrl::new_tab(local.clone()));
    }
    if goto_mail {
        crate::screens::config::open(app, crate::screens::config::CfgTab::Correo);
    }
    if poster {
        let html = poster_html(&app.db, &url);
        if let Err(e) = crate::screens::centros::open_html(ui.ctx(), "cartel_cita_online.html", html) {
            app.warn(e);
        }
    }
    if serde_json::to_string(&app.db.booking).unwrap_or_default() != before {
        app.changed();
    }
}

/// Cartel para la sala de espera o el mostrador con el código QR de la página.
pub fn poster_html(db: &Db, url: &str) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Pida su cita por internet</title><style>
body{{font-family:Inter,Segoe UI,Arial,sans-serif;color:#0b1324;margin:0;display:flex;justify-content:center}}
.p{{width:180mm;margin:20mm auto;text-align:center;border:2px solid #0d9488;border-radius:14mm;padding:16mm 12mm}}
h1{{font-size:34pt;margin:0 0 4mm;letter-spacing:-.5pt}} .s{{font-size:15pt;color:#475569;margin-bottom:10mm}}
.q{{display:inline-block;padding:6mm;border-radius:8mm;background:#f0fdfa}} .u{{font-size:13pt;color:#0f766e;margin-top:6mm;word-break:break-all}}
.c{{margin-top:10mm;font-size:13pt;color:#334155}} @media print{{.p{{margin:10mm auto}}}}
</style></head><body><div class='p'><h1>Pida su cita por internet</h1><div class='s'>Escanee el código con la cámara del móvil, elija el día y la hora, y listo.</div>
<div class='q'>{}</div><div class='u'>{}</div><div class='c'><b>{}</b><br>{} · {}</div></div></body></html>",
        crate::verifactu::qr_svg(url, 80.0),
        esc(url),
        esc(&db.config.name),
        esc(&db.config.address),
        esc(&db.config.phone)
    )
}
