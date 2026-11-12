#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agenda_extra;
mod app;
mod bank;
mod booking;
mod budget_link;
mod doc_link;
mod brand;
mod chat;
mod cifrado;
mod disco;
mod colegiado;
mod comms;
mod erx;
mod extra;
mod finance;
mod firma;
mod gesden;
mod golive;
mod ia;
mod mail;
mod health;
mod insurer;
mod icons;
mod lab_order;
mod model;
mod mssql;
mod nomenclator;
mod phone;
mod portal;
mod record;
mod rx_link;
mod sample;
mod server;
mod srep;
mod srep_cert;
mod srep_doc;
mod sync;
mod stock;
mod tasks;
mod screens;
mod shared;
mod slot_offer;
mod theme;
mod tools;
mod totp;
mod tunnel;
mod users;
mod verifactu;
mod vista;
mod whatsapp;
#[cfg(test)]
mod ui_tests;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // el programa se llamaba DentaSuite: sus carpetas pasan al nombre nuevo la
    // primera vez que se abre (no en las capturas automáticas, que no tocan nada)
    if !args.iter().any(|a| a == "--screenshot") {
        let n = brand::migrate_dirs();
        if n > 0 {
            eprintln!("{n} carpetas de {} pasadas a {}", brand::OLD_DIR, brand::NAME);
        }
    }
    if args.iter().any(|a| a == "--servidor") {
        if let Err(e) = server::run() {
            eprintln!("{e}");
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--repositorio-recetas") {
        if let Err(e) = srep::run() {
            eprintln!("{e}");
        }
        return Ok(());
    }
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    // iconos para los instaladores
    if let Some(dir) = arg("--icono") {
        match brand::write_icons(std::path::Path::new(&dir)) {
            Ok(files) => files.iter().for_each(|f| println!("{}", f.display())),
            Err(e) => eprintln!("{e}"),
        }
        return Ok(());
    }
    let shot = arg("--screenshot").map(PathBuf::from);
    // Aviso de llamada desde la centralita: si VeodarDental ya está abierto en
    // este ordenador se le pasa el número y no se abre otra ventana.
    let call_arg = arg("--llamada");
    if let Some(n) = &call_arg {
        if shot.is_none() && phone::forward(n, phone::CALL_PORT).is_ok() {
            return Ok(());
        }
    }
    let login_flag = args.iter().any(|x| x == "--login");
    let recetas_flag = args.iter().any(|x| x == "--recetas");
    let dark_flag = args.iter().any(|x| x == "--radiografia");
    let screen = arg("--screen").and_then(|s| app::Screen::from_name(&s));
    let patient = arg("--patient").and_then(|s| s.parse().ok());
    let tab = arg("--tab").and_then(|s| {
        let f = model::fold(&s);
        app::PatTab::ALL.iter().find(|(_, n)| model::fold(n).starts_with(&f)).map(|x| x.0)
    });

    let size: [f32; 2] = arg("--size")
        .and_then(|s| s.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?])))
        .unwrap_or([1440.0, 900.0]);
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("VeodarDental — Gestión de clínica dental")
            .with_inner_size(size)
            .with_min_inner_size([1100.0, 680.0])
            .with_app_id("veodardental")
            .with_icon(std::sync::Arc::new(brand::window_icon())),
        ..Default::default()
    };
    // suavizado de bordes (MSAA) para los dibujos del odontograma y el periodontograma
    options.multisampling = 4;
    if shot.is_some() {
        // Para la captura no se espera al refresco de la pantalla: con la pantalla
        // apagada o bloqueada cada fotograma esperaría casi un segundo.
        options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
    }
    eframe::run_native(
        "VeodarDental",
        options,
        Box::new(move |cc| {
            let mut a = app::App::new(cc, shot, screen, patient, tab);
            if login_flag {
                a.user_id = None;
            }
            if let Some(u) = arg("--user") {
                a.user_id = a.db.users.iter().find(|x| model::fold(&x.name).starts_with(&model::fold(&u))).map(|x| x.id);
                if let Some(n) = a.user_id.and_then(|id| a.db.user(id)).map(|x| x.name.clone()) {
                    a.db.config.current_user = n;
                }
            }
            if let Some(t) = arg("--fact-tab").and_then(|s| s.parse::<u8>().ok()) {
                a.lists.fact_tab = t;
                a.lists.inv_from = a.today - chrono::Duration::days(365);
            }
            if let Some(t) = arg("--pieza").and_then(|s| s.parse::<u8>().ok()) {
                a.lists.perio_tooth = Some(t);
                a.odg.selected_tooth = Some(t);
            }
            if let Some(y) = arg("--bajar").and_then(|s| s.parse::<f32>().ok()) {
                a.lists.scroll_to = Some(y);
            }
            if let Some(k) = arg("--imagen") {
                let k = model::fold(&k);
                a.img.selected = a.db.images.iter().find(|i| Some(i.patient_id) == a.patient && model::fold(i.kind.label()).starts_with(&k)).map(|i| i.id);
            }
            if let Some(n) = arg("--agente").and_then(|s| s.parse::<u32>().ok()) {
                a.ia.edit = Some(n);
            }
            // VERI*FACTU activado (entorno de pruebas), para ver el QR y la pestaña de envíos
            if std::env::args().any(|x| x == "--verifactu") {
                a.db.verifactu.enabled = true;
                a.db.verifactu.since = None;
                a.db.verifactu.installation = "VD-DEMO".into();
                // con «--verifactu rechazada», la primera factura aparece rechazada (para ver la corrección)
                if arg("--verifactu").as_deref() == Some("rechazada") {
                    if let Some(id) = verifactu::chain_order(&a.db).first().map(|i| i.id) {
                        if let Some(inv) = a.db.invoices.iter_mut().find(|i| i.id == id) {
                            inv.aeat.state = "Incorrecto".into();
                            inv.aeat.code = "1104".into();
                            inv.aeat.message = "El NIF del destinatario no está identificado en el censo de la AEAT.".into();
                        }
                    }
                }
            }
            if dark_flag {
                a.odg.dark = true;
            }
            // agenda en un día concreto («--fecha 2026-11-10»; «--fecha online» = el de la primera cita pedida por internet)
            if let Some(f) = arg("--fecha") {
                let d = if f == "online" { a.db.booking_requests.iter().map(|r| r.date).min() } else { chrono::NaiveDate::parse_from_str(&f, "%Y-%m-%d").ok() };
                if let Some(d) = d {
                    a.agenda.date = d;
                }
            }
            // cita online publicada desde este ordenador (para verla en Configuración)
            if std::env::args().any(|x| x == "--cita-online") {
                a.db.booking.enabled = true;
                a.db.booking.station = shared::machine_name();
            }
            // dos huecos libres de ejemplo (uno sin ofrecer y otro ya ofrecido) para ver la lista de espera de la agenda
            if std::env::args().any(|x| x == "--huecos") {
                use chrono::Datelike;
                let mut d = a.today + chrono::Duration::days(2);
                while d.weekday().num_days_from_monday() >= 5 || a.db.holidays.iter().any(|h| h.date == d) {
                    d += chrono::Duration::days(1);
                }
                let t = |h: u32, m: u32| chrono::NaiveTime::from_hms_opt(h, m, 0).unwrap();
                for (doc, pid, start, mins) in [(1u32, 100u32, t(16, 30), 30u32), (4, 103, t(10, 0), 45)] {
                    a.db.appointments.retain(|x| !(x.date == d && x.doctor_id == doc && x.start < start + chrono::Duration::minutes(mins as i64) && start < x.end()));
                    let id = a.db.new_id();
                    a.db.appointments.push(model::Appointment { id, date: d, start, minutes: mins, patient_id: Some(pid), free_name: String::new(), phone: String::new(), doctor_id: doc, box_id: doc, reason: "Revisión".into(), treatment_code: String::new(), state: model::ApptState::Anulada, notes: "Anulada por el paciente".into(), arrived_at: None, seen_at: None, finished_at: None, reminder_sent: false, first_visit: false, center_id: 1 });
                    if doc == 4 {
                        if let Ok(o) = a.db.create_offer(doc, d, start, mins, 1, Some(id), model::now()) {
                            a.db.offer_messages(o);
                        }
                    }
                }
                a.agenda.date = d;
            }
            // código del móvil: «--doble-factor» abre su activación para la Dra. Ruiz;
            // «--doble-factor entrar», el segundo paso de la pantalla de acceso
            if std::env::args().any(|x| x == "--doble-factor") {
                if let Some(id) = a.db.users.iter().find(|u| u.name.starts_with("Dra. Ruiz")).map(|u| u.id) {
                    let secret = totp::new_secret().unwrap_or_default();
                    if arg("--doble-factor").as_deref() == Some("entrar") {
                        if let Some(u) = a.db.users.iter_mut().find(|u| u.id == id) {
                            u.totp_secret = secret;
                        }
                        a.user_id = None;
                        a.login.selected = Some(id);
                        a.login.pending_code = Some(id);
                    } else {
                        a.screen = app::Screen::Config;
                        a.lists.cfg_tab = screens::config::CfgTab::Usuarios;
                        a.login.edit_user = Some(id);
                        a.login.enroll = Some((id, secret, String::new()));
                    }
                }
            }
            if recetas_flag {
                a.docs_mode = 1;
            }
            if let Some(r) = arg("--report").and_then(|s| s.parse::<usize>().ok()) {
                a.lists.report_tab = r;
                a.lists.report_from = a.today - chrono::Duration::days(3650);
            }
            if let Some(d) = arg("--dialog") {
                a.open_dialog_by_name(&d);
            }
            if arg("--scroll").as_deref() == Some("telefonia") {
                a.phone.scroll_here = true;
                a.lists.cfg_tab = screens::config::CfgTab::Telefonia;
            }
            if arg("--scroll").as_deref() == Some("whatsapp") {
                a.wa.scroll_here = true;
                a.lists.cfg_tab = screens::config::CfgTab::Whatsapp;
            }
            // abrir un apartado de Configuración por su nombre (p. ej. «--ajustes receta»)
            if let Some(name) = arg("--ajustes") {
                let f = model::fold(&name);
                if let Some(t) = screens::config::CfgTab::GROUPS.iter().flat_map(|g| g.1.iter()).find(|t| model::fold(t.label()).contains(&f)) {
                    screens::config::open(&mut a, *t);
                }
            }
            if let Some(n) = call_arg {
                screens::llamadas::incoming(&mut a, &n);
            }
            Ok(Box::new(a))
        }),
    )
}
