//! Pruebas de interfaz: simulan teclado y ratón sobre la aplicación real.

use crate::app::App;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn harness() -> Harness<'static, App> {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).allow_missing_glyphs().build_eframe(|cc| {
        // modo demostración (no toca los datos guardados del usuario)
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = None;
        a
    });
    for _ in 0..5 {
        h.step();
    }
    h
}

#[test]
fn login_typing_and_enter() {
    let mut h = harness();
    assert!(h.state().user_id.is_none());
    h.event(egui::Event::Text("0000".into()));
    h.step();
    h.step();
    assert_eq!(h.state().login.pin, "0000", "el campo de contraseña no recibe el texto");
    h.get_by_label("Entrar").click();
    for _ in 0..4 {
        h.step();
    }
    assert!(h.state().user_id.is_some(), "no entra: {}", h.state().login.error);
}

#[test]
fn login_select_other_user_by_click() {
    let mut h = harness();
    h.get_by_label_contains("Dra. Ruiz").click();
    for _ in 0..3 {
        h.step();
    }
    let sel = h.state().login.selected.and_then(|id| h.state().db.user(id)).map(|u| u.name.clone());
    assert_eq!(sel.as_deref(), Some("Dra. Ruiz"), "no se puede seleccionar el usuario");
}

/// Igual que el inicio real: datos guardados en disco (XDG_DATA_HOME apunta
/// a una copia temporal en la prueba).
#[test]
#[ignore]
fn login_with_saved_data() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).allow_missing_glyphs().build_eframe(|cc| App::new(cc, None, None, None, None));
    for _ in 0..5 {
        h.step();
    }
    assert!(h.state().user_id.is_none());
    h.event(egui::Event::Text("0000".into()));
    h.step();
    h.step();
    assert_eq!(h.state().login.pin, "0000", "el campo de contraseña no recibe el texto");
    h.get_by_label("Entrar").click();
    for _ in 0..4 {
        h.step();
    }
    assert!(h.state().user_id.is_some(), "no entra: {}", h.state().login.error);
}

fn click_at(h: &mut Harness<'static, App>, x: f32, y: f32) {
    let p = egui::pos2(x, y);
    h.event(egui::Event::PointerMoved(p));
    h.step();
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.step();
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.step();
    h.step();
}

/// Clic real con el ratón (no por accesibilidad) sobre las tarjetas de usuario.
#[test]
fn login_pointer_click_selects_user() {
    let mut h = harness();
    let before = h.state().login.selected;
    // recorre una rejilla de puntos sobre la tarjeta buscando las fichas de usuario
    let mut changed = false;
    'outer: for y in (260..460).step_by(20) {
        for x in (440..1000).step_by(40) {
            click_at(&mut h, x as f32, y as f32);
            if h.state().login.selected != before {
                changed = true;
                break 'outer;
            }
        }
    }
    assert!(changed, "ningún clic de ratón cambia el usuario seleccionado");
}

/// Entrar usando solo el teclado numérico de la pantalla.
#[test]
fn login_with_onscreen_keypad() {
    let mut h = harness();
    h.get_by_label_contains("Dra. Ruiz").click();
    h.step();
    for _ in 0..4 {
        h.get_by_label("0").click();
        h.step();
    }
    assert_eq!(h.state().login.pin, "0000");
    h.get_by_label("Entrar").click();
    for _ in 0..4 {
        h.step();
    }
    let who = h.state().user_id.and_then(|id| h.state().db.user(id)).map(|u| u.name.clone());
    assert_eq!(who.as_deref(), Some("Dra. Ruiz"));
}

/// Tras entrar, la aplicación responde (se ve el menú y el aviso de cambiar contraseña).
#[test]
fn after_login_app_is_usable() {
    let mut h = harness();
    h.event(egui::Event::Text("0000".into()));
    h.step();
    h.key_press(egui::Key::Enter);
    for _ in 0..6 {
        h.step();
    }
    assert!(h.state().user_id.is_some(), "Enter no inicia sesión: {}", h.state().login.error);
    // el diálogo de cambio de contraseña se puede cerrar
    h.get_by_label("Ahora no").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(!h.state().login.show_change);
    h.get_by_label("Agenda").click();
    h.step();
}

/// Una cita nueva con «Repetir cada semana» crea la serie al guardar.
#[test]
fn weekly_repeat_creates_series() {
    let mut h = harness();
    h.event(egui::Event::Text("0000".into()));
    h.step();
    h.key_press(egui::Key::Enter);
    for _ in 0..6 {
        h.step();
    }
    h.get_by_label("Ahora no").click();
    for _ in 0..3 {
        h.step();
    }
    let before = h.state().db.appointments.len();
    h.state_mut().screen = crate::app::Screen::Agenda;
    h.state_mut().open_dialog_by_name("repetir");
    {
        // la serie empieza en un día laborable (en fin de semana se saltaría, como debe)
        let a = h.state_mut();
        let mut day = a.today;
        while !a.db.working_day(day) {
            day += chrono::Duration::days(1);
        }
        if let crate::app::Dialog::Appointment(d) = &mut a.dialog {
            d.a.date = day;
        }
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Guardar cita").click();
    for _ in 0..3 {
        h.step();
    }
    let after = h.state().db.appointments.len();
    assert!(after >= before + 3, "la serie no se creó: {before} → {after}");
}

/// Desde Facturas › Liquidaciones a mutuas se genera la liquidación con un clic.
#[test]
fn generate_insurer_claim_from_screen() {
    let mut h = harness();
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.screen = crate::app::Screen::Facturas;
        a.lists.fact_tab = 1;
        a.lists.inv_from = a.today - chrono::Duration::days(365);
    }
    for _ in 0..4 {
        h.step();
    }
    assert!(h.state().db.claims.is_empty());
    h.get_by_label_contains("Generar liquidación").click();
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(h.state().db.claims.len(), 1, "no se generó la liquidación");
    h.get_by_label("✔ Cobrada").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().db.claims[0].state(), "Cobrada");
}

/// Receta oficial: se registra el código de verificación y aparece en Configuración.
#[test]
fn official_prescription_records_cve() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 2400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.open_patient(101, crate::app::PatTab::Documentos);
        a.docs_mode = 1;
    }
    for _ in 0..4 {
        h.step();
    }
    let rid = h.state().db.prescriptions.iter().find(|r| r.patient_id == 101).map(|r| r.id).unwrap();
    assert!(h.state().db.prescriptions.iter().find(|r| r.id == rid).unwrap().erx_code.is_empty());
    h.state_mut().rx.cve = "ab12-cd34-ef56".into();
    h.step();
    h.get_by_label("Guardar como emitida").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().db.prescriptions.iter().find(|r| r.id == rid).unwrap().erx_code, "AB12-CD34-EF56");
    h.state_mut().screen = crate::app::Screen::Config;
    h.state_mut().lists.cfg_tab = crate::screens::config::CfgTab::Receta;
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Receta electrónica oficial");
}

/// Receta por conexión directa: se rellenan los códigos, se pone el
/// certificado, «Firmar y emitir», y una farmacia (como Nodofarma) la ve.
#[test]
fn direct_prescription_sign_and_issue() {
    // repositorio de recetas en este ordenador
    let dir = std::env::temp_dir().join(format!("ds_ui_srep_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let cfg = crate::srep::RepoConfig { port, key: "CLAVE".into(), allowed_ips: vec!["127.0.0.1".into()], require_cert_header: false, ..Default::default() };
    let store = dir.join("repositorio.json");
    let s2 = store.clone();
    let srv = std::thread::spawn(move || crate::srep::serve(server, cfg, s2));
    let p12 = dir.join("dentista.p12");
    std::fs::write(&p12, crate::firma::tests::test_p12("clave1")).unwrap();

    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 2600.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let rid;
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.db.erx.direct = true;
        a.db.erx.repo_addr = format!("127.0.0.1:{port}");
        a.db.erx.repo_key = "CLAVE".into();
        let r = a.db.prescriptions.iter_mut().find(|r| r.patient_id == 101).unwrap();
        rid = r.id;
        for (i, it) in r.items.iter_mut().enumerate() {
            it.cn = format!("70000{i}");
        }
        a.rx.cert_doctor = Some(r.doctor_id);
        a.rx.cert_path = p12.display().to_string();
        a.rx.password = "clave1".into();
        a.open_patient(101, crate::app::PatTab::Documentos);
        a.docs_mode = 1;
        a.rx.selected = Some(rid);
    }
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("✍ Firmar y emitir").click();
    for _ in 0..4 {
        h.step();
    }
    let st = h.state().rx.status.clone();
    let r = h.state().db.prescriptions.iter().find(|r| r.id == rid).unwrap().clone();
    let issued = r.srep.clone().unwrap_or_else(|| panic!("no se emitió: {st}"));
    assert!(issued.test && h.state().rx.password.is_empty(), "la contraseña no se queda en memoria");
    // la farmacia la ve
    let tx = crate::srep::new_id32();
    let (code, v) =
        crate::srep::farmacia_post(&format!("http://127.0.0.1:{port}"), &format!("/prescriptions/idFarmacia/F9/idAcceso/{}", issued.id_acceso), &[("idTransaccion", tx.as_str()), ("swNodo", "Nodofarma")], &serde_json::json!({}), false).unwrap();
    assert_eq!((code, v["codResultado"].as_str().unwrap()), (200, "CONOK"));
    assert_eq!(v["prescripciones"].as_array().unwrap().len(), r.items.len());
    // y el programa ve lo que hace la farmacia
    let id_receta = issued.issued[0].recetas[0].id_receta.clone();
    let ahora = crate::model::now().format("%d/%m/%Y %H:%M:%S").to_string();
    let body =
        serde_json::json!({"idReceta": id_receta, "idTransaccion": crate::srep::new_id32(), "idAccionFarmacia": "A1", "accion": 1, "idFarmacia": "F9", "envasesDispensados": 1, "fechaHoraAccion": ahora, "versionSoftware": {"swNodo": "Nodofarma"}});
    assert_eq!(crate::srep::farmacia_post(&format!("http://127.0.0.1:{port}"), "/receta", &[], &body, false).unwrap().1["codResultado"], "RACOK");
    h.get_by_label("↻ Ver qué han hecho las farmacias").click();
    for _ in 0..3 {
        h.step();
    }
    let s = h.state().db.prescriptions.iter().find(|r| r.id == rid).unwrap().srep.clone().unwrap();
    assert_eq!(s.status.get(&id_receta), Some(&3));
    assert!(s.events.iter().any(|e| e.contains("farmacia F9") && e.contains("Dispensación")));
    crate::srep::RepoClient::new(&format!("127.0.0.1:{port}"), "CLAVE").parar().unwrap();
    srv.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// El panel de puestos en red aparece en Configuración.
#[test]
fn network_panel_in_settings() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 3000.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.screen = crate::app::Screen::Config;
        a.lists.cfg_tab = crate::screens::config::CfgTab::Red;
    }
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("Conectar este puesto");
    h.get_by_label("Hacer de este ordenador el servidor");
}

/// Un aviso recibido aparece como nota y se contesta con un clic.
#[test]
fn chat_popup_reply() {
    let mut h = harness();
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Recep")).map(|u| u.id);
        a.db.chat.clear();
        a.db.send_msg("Dra. Ruiz", "Recepción", "Pase el siguiente paciente", None);
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Voy").click();
    for _ in 0..3 {
        h.step();
    }
    let db = &h.state().db;
    assert!(db.unread_msgs("Recepción").is_empty(), "el aviso sigue sin leer");
    assert!(db.chat.iter().any(|m| m.from == "Recepción" && m.to == "Dra. Ruiz" && m.text == "Voy"));
}

/// La ventana de avisos no se cierra al elegir destinatario ni al escribir.
#[test]
fn chat_window_stays_open_while_typing() {
    let mut h = harness();
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Recep")).map(|u| u.id);
        a.db.chat.clear();
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("💬").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(h.state().chat.open);
    h.get_by_label("Dra. Ruiz").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(h.state().chat.open, "se cerró al elegir destinatario");
    assert_eq!(h.state().chat.to, "Dra. Ruiz");
    h.get_all_by_role(egui::accesskit::Role::TextInput).last().unwrap().click();
    h.step();
    h.event(egui::Event::Text("Hola".into()));
    h.step();
    assert!(h.state().chat.open, "se cerró al escribir");
    assert_eq!(h.state().chat.text, "Hola");
    h.key_press(egui::Key::Enter);
    for _ in 0..3 {
        h.step();
    }
    assert!(h.state().chat.open, "se cerró al enviar");
    assert!(h.state().db.chat.iter().any(|m| m.to == "Dra. Ruiz" && m.text == "Hola"));
}

/// Al entrar una llamada aparece quién llama y con un clic se abre su ficha.
#[test]
fn incoming_call_opens_patient_file() {
    let mut h = harness();
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Recep")).map(|u| u.id);
        a.screen = crate::app::Screen::Inicio;
        a.patient = Some(105);
        let mobile = a.db.patient(100).unwrap().mobile.clone();
        crate::screens::llamadas::incoming(a, &format!("+34 {mobile}"));
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Abrir ficha").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().patient, Some(100));
    assert_eq!(h.state().screen, crate::app::Screen::Pacientes);
    assert!(h.state().phone.popups.is_empty(), "el aviso debería cerrarse");
}

/// Un número desconocido ofrece dar de alta al paciente con ese teléfono.
#[test]
fn unknown_call_offers_new_patient() {
    let mut h = harness();
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Recep")).map(|u| u.id);
        a.screen = crate::app::Screen::Inicio;
        crate::screens::llamadas::incoming(a, "0034 699000111");
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("＋ Nuevo paciente con este teléfono").click();
    for _ in 0..3 {
        h.step();
    }
    match &h.state().dialog {
        crate::app::Dialog::NewPatient(p) => assert_eq!(p.mobile, "699 000 111"),
        _ => panic!("no se abrió el alta de paciente"),
    }
}

/// En Configuración se puede simular una llamada para comprobar el aviso.
#[test]
fn simulate_call_from_settings() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 4000.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.screen = crate::app::Screen::Config;
        a.lists.cfg_tab = crate::screens::config::CfgTab::Telefonia;
        a.phone.test_number = "610 200 100".into();
    }
    for _ in 0..4 {
        h.step();
    }
    let before = h.state().db.calls.len();
    h.get_by_label("Simular llamada").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().db.calls.len(), before + 1);
    assert_eq!(h.state().phone.popups.len(), 1);
    h.get_by_label("Abrir ficha");
}

/// En Comunicaciones › Confirmaciones por WhatsApp se puede simular que el
/// paciente no puede venir: la cita se anula y recepción recibe una tarea.
#[test]
fn whatsapp_log_simulated_no() {
    let mut h = harness();
    let (appt, tasks) = {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Recep")).map(|u| u.id);
        a.open_dialog_by_name("wa-log");
        let m = a.db.whatsapp.iter().find(|m| m.state.waiting()).cloned().expect("registro de ejemplo esperando respuesta");
        (m.appt_id, a.db.tasks.len())
    };
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("Simular «No»").click();
    for _ in 0..3 {
        h.step();
    }
    let a = h.state();
    assert_eq!(a.db.appointments.iter().find(|x| x.id == appt).unwrap().state, crate::model::ApptState::Anulada);
    assert_eq!(a.db.tasks.len(), tasks + 1);
    assert!(a.db.wa_for_appt(appt).is_some_and(|m| m.state == crate::whatsapp::WaState::NoViene));
}

/// La tarjeta de WhatsApp automático aparece en Configuración.
#[test]
fn whatsapp_settings_card() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 6000.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.screen = crate::app::Screen::Config;
        a.lists.cfg_tab = crate::screens::config::CfgTab::Whatsapp;
    }
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("Guardar y enviar desde este ordenador");
    h.get_by_label("Enviar prueba");
    // sin datos de Meta no se puede activar (y no se guarda nada)
    h.get_by_label("Guardar y enviar desde este ordenador").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(!h.state().wa.saved.enabled);
    assert!(h.state().db.wa_sender.is_empty());
}

/// La pasarela gratuita no se puede activar sin aceptar el riesgo de bloqueo.
#[test]
fn free_gateway_requires_accepting_the_risk() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 7000.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.screen = crate::app::Screen::Config;
        a.lists.cfg_tab = crate::screens::config::CfgTab::Whatsapp;
    }
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label_contains("pasarela gratuita (no oficial").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().wa.form.provider, crate::whatsapp::Provider::Pasarela);
    h.get_by_label("Entiendo el riesgo y quiero usar la pasarela gratuita");
    h.get_by_label("Guardar y enviar desde este ordenador").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(!h.state().wa.saved.enabled, "no debe activarse sin aceptar el riesgo");
    assert!(h.state().db.wa_sender.is_empty());
    assert!(h.state().wa.form.missing().iter().any(|m| m.contains("riesgo")));
}

/// VERI*FACTU: desde Facturas › VERI*FACTU, «Enviar ahora» remite los registros
/// (a un servicio de prueba que contesta como la AEAT) y cada factura queda
/// con su estado y su CSV; la rechazada muestra el motivo.
#[test]
fn verifactu_send_from_invoices_tab() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let srv = std::thread::spawn(move || {
        let mut rq = server.recv().unwrap();
        let mut body = String::new();
        rq.as_reader().read_to_string(&mut body).unwrap();
        let codes: Vec<String> = body.split("<sum1:IDFactura>").skip(1).map(|s| s.split("<sum1:NumSerieFactura>").nth(1).unwrap().split('<').next().unwrap().to_string()).collect();
        let mut lines = String::new();
        for (k, c) in codes.iter().enumerate() {
            let st = if k == 0 {
                "<tikR:EstadoRegistro>Incorrecto</tikR:EstadoRegistro><tikR:CodigoErrorRegistro>1104</tikR:CodigoErrorRegistro><tikR:DescripcionErrorRegistro>El NIF del destinatario no está identificado</tikR:DescripcionErrorRegistro>"
            } else {
                "<tikR:EstadoRegistro>Correcto</tikR:EstadoRegistro>"
            };
            lines += &format!("<tikR:RespuestaLinea><tikR:IDFactura><tik:NumSerieFactura>{c}</tik:NumSerieFactura></tikR:IDFactura>{st}</tikR:RespuestaLinea>");
        }
        let resp = format!(
            "<env:Envelope><env:Body><tikR:RespuestaRegFactuSistemaFacturacion><tikR:CSV>A-PRUEBA</tikR:CSV><tikR:TiempoEsperaEnvio>60</tikR:TiempoEsperaEnvio><tikR:EstadoEnvio>ParcialmenteCorrecto</tikR:EstadoEnvio>{lines}</tikR:RespuestaRegFactuSistemaFacturacion></env:Body></env:Envelope>"
        );
        rq.respond(tiny_http::Response::from_string(resp)).unwrap();
        codes.len()
    });
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 2400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let n = {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.db.verifactu.enabled = true;
        a.db.verifactu.since = None;
        a.db.verifactu.endpoint = format!("http://127.0.0.1:{port}/verifactu");
        a.screen = crate::app::Screen::Facturas;
        a.lists.fact_tab = 2;
        a.db.invoices.len()
    };
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label(&format!("Enviar ahora ({n})")).click();
    for _ in 0..300 {
        h.step();
        if h.state().vf.job.is_none() && h.state().db.invoices.iter().all(|i| !i.aeat.state.is_empty()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(srv.join().unwrap(), n, "se envían todas las facturas en un lote");
    let st: Vec<String> = crate::verifactu::chain_order(&h.state().db).iter().map(|i| i.aeat.state.clone()).collect();
    assert_eq!(st[0], "Incorrecto");
    assert!(st[1..].iter().all(|s| s == "Correcto"), "{st:?}");
    assert!(h.state().db.invoices.iter().filter(|i| i.aeat.state == "Correcto").all(|i| i.aeat.csv == "A-PRUEBA"));
    assert!(crate::verifactu::pending(&h.state().db, &h.state().db.verifactu).is_empty());
    // el motivo del rechazo queda apuntado en la factura (y se ve en la tabla)
    let first = crate::verifactu::chain_order(&h.state().db)[0].aeat.clone();
    assert_eq!(first.code, "1104");
    assert!(first.message.contains("no está identificado"));
    for _ in 0..3 {
        h.step();
    }
}

/// Configuración por pestañas: el menú de apartados cambia el contenido.
#[test]
fn settings_tabs_switch_content() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1200.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.screen = crate::app::Screen::Config;
    }
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().lists.cfg_tab, crate::screens::config::CfgTab::Clinica);
    // en «Datos de la clínica» no está la telefonía
    assert!(h.query_by_label_contains("Simular llamada").is_none());
    for (label, tab) in [("Telefonía", crate::screens::config::CfgTab::Telefonia), ("Inteligencia artificial", crate::screens::config::CfgTab::Ia), ("Gabinetes", crate::screens::config::CfgTab::Gabinetes)] {
        h.get_by_label(label).click();
        for _ in 0..3 {
            h.step();
        }
        assert_eq!(h.state().lists.cfg_tab, tab, "{label}");
    }
    // el gabinete nuevo se añade desde su pestaña
    let n = h.state().db.boxes.len();
    h.get_by_label("＋ Añadir gabinete").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().db.boxes.len(), n + 1);
}

/// VERI*FACTU desde la ventana de la factura: una rechazada se corrige y se
/// vuelve a enviar; otra se anula por haberse expedido por error. Los dos
/// registros nuevos se encadenan y se envían juntos.
#[test]
fn verifactu_correct_and_cancel_from_invoice() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let srv = std::thread::spawn(move || {
        let mut rq = server.recv().unwrap();
        let mut body = String::new();
        rq.as_reader().read_to_string(&mut body).unwrap();
        // contesta «Correcto» a cada registro, con su tipo de operación
        let mut lines = String::new();
        for part in body.split("<sum:RegistroFactura>").skip(1) {
            let (op, tag) = if part.starts_with("<sum1:RegistroAnulacion>") { ("Anulacion", "NumSerieFacturaAnulada") } else { ("Alta", "NumSerieFactura") };
            let num = part.split(&format!("<sum1:{tag}>")).nth(1).unwrap().split('<').next().unwrap();
            lines += &format!(
                "<tikR:RespuestaLinea><tikR:IDFactura><tik:NumSerieFactura>{num}</tik:NumSerieFactura></tikR:IDFactura><tikR:Operacion><tik:TipoOperacion>{op}</tik:TipoOperacion></tikR:Operacion><tikR:EstadoRegistro>Correcto</tikR:EstadoRegistro></tikR:RespuestaLinea>"
            );
        }
        let resp = format!(
            "<env:Envelope><env:Body><tikR:RespuestaRegFactuSistemaFacturacion><tikR:CSV>A-CORR</tikR:CSV><tikR:TiempoEsperaEnvio>60</tikR:TiempoEsperaEnvio><tikR:EstadoEnvio>Correcto</tikR:EstadoEnvio>{lines}</tikR:RespuestaRegFactuSistemaFacturacion></env:Body></env:Envelope>"
        );
        rq.respond(tiny_http::Response::from_string(resp)).unwrap();
        body
    });
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 2400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let (rejected, good) = {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.db.verifactu.enabled = true;
        a.db.verifactu.since = None;
        a.db.verifactu.endpoint = format!("http://127.0.0.1:{port}/verifactu");
        let ids: Vec<u32> = crate::verifactu::chain_order(&a.db).iter().map(|i| i.id).collect();
        for (k, id) in ids.iter().enumerate() {
            let inv = a.db.invoices.iter_mut().find(|i| i.id == *id).unwrap();
            inv.aeat.state = if k == 0 { "Incorrecto".into() } else { "Correcto".into() };
            if k == 0 {
                inv.aeat.code = "1104".into();
                inv.aeat.message = "NIF no identificado".into();
            }
        }
        a.dialog = crate::app::Dialog::Invoice(ids[0]);
        (ids[0], *ids.last().unwrap())
    };
    for _ in 0..4 {
        h.step();
    }
    // corregir el NIF y reenviar
    h.get_by_label("Corregir datos y reenviar").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().vf.fix, Some(rejected));
    h.state_mut().vf.fix_nif = "12345678Z".into();
    h.get_by_label("Generar registro y reenviar").click();
    for _ in 0..3 {
        h.step();
    }
    let rec = h.state().db.vf_records.first().cloned().expect("no se generó el registro de corrección");
    assert_eq!(rec.kind, crate::verifactu::RecKind::Subsanacion);
    assert_eq!(rec.rechazo_previo, "S", "reenvío de un registro rechazado");
    assert_eq!(h.state().db.invoice(rejected).unwrap().client_nif, "12345678Z");
    assert!(h.state().db.verify_chain().is_ok());
    // anular otra factura
    h.state_mut().dialog = crate::app::Dialog::Invoice(good);
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Anular (factura expedida por error)").click();
    for _ in 0..3 {
        h.step();
    }
    h.state_mut().vf.cancel_reason = "Duplicada".into();
    h.get_by_label("Anular la factura").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(crate::verifactu::cancelled(&h.state().db, good));
    assert_eq!(h.state().db.vf_records.len(), 2);
    assert!(h.state().db.verify_chain().is_ok());
    // enviar los dos registros nuevos desde la pestaña VERI*FACTU
    {
        let a = h.state_mut();
        a.dialog = crate::app::Dialog::None;
        a.screen = crate::app::Screen::Facturas;
        a.lists.fact_tab = 2;
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Enviar ahora (2)").click();
    for _ in 0..300 {
        h.step();
        if h.state().vf.job.is_none() && h.state().db.vf_records.iter().all(|r| !r.aeat.state.is_empty()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let body = srv.join().unwrap();
    assert!(body.contains("<sum1:RechazoPrevio>S</sum1:RechazoPrevio>") && body.contains("<sum1:RegistroAnulacion>"));
    assert!(body.contains("<sum1:NIF>12345678Z</sum1:NIF>"), "con el NIF corregido");
    assert!(h.state().db.vf_records.iter().all(|r| r.aeat.state == "Correcto" && r.aeat.csv == "A-CORR"));
}

/// Análisis con IA desde la galería del paciente: el primer agente gratuito
/// está saturado y responde el segundo; el análisis se guarda y su propuesta
/// pasa al plan de tratamiento como «previsto».
#[test]
fn ai_analysis_falls_back_and_adds_to_plan() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let srv = std::thread::spawn(move || {
        let mut models = vec![];
        for _ in 0..2 {
            let mut rq = server.recv().unwrap();
            let mut body = String::new();
            rq.as_reader().read_to_string(&mut body).unwrap();
            let v: serde_json::Value = serde_json::from_str(&body).unwrap();
            let model = v["model"].as_str().unwrap_or("").to_string();
            assert!(body.contains("data:image/jpeg;base64,"), "debe llevar la imagen");
            assert!(!body.contains("Carlos") && !body.contains("Martínez"), "no debe llevar el nombre del paciente");
            let (code, resp) = if model == "saturado" {
                (429, serde_json::json!({"error": {"message": "rate limit"}}).to_string())
            } else {
                let good = r#"{"resumen":"Caries oclusal en 47.","calidad":"buena","hallazgos":[{"pieza":"47","tipo":"caries","caras":["O"],"descripcion":"radiolucidez oclusal","gravedad":"media","confianza":0.7}],"propuesta":[{"tipo":"obturacion","piezas":["47"],"caras":["O"],"motivo":"caries oclusal"}]}"#;
                (200, serde_json::json!({"choices": [{"message": {"content": good}}]}).to_string())
            };
            models.push(model);
            rq.respond(tiny_http::Response::from_string(resp).with_status_code(code)).unwrap();
        }
        models
    });
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 2400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let img_id;
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        let url = format!("http://127.0.0.1:{port}/v1");
        use crate::ia::{Agent, Mode, Provider};
        a.db.ia_reports.clear(); // sin el análisis de ejemplo
        a.db.ia.accepted = true;
        a.db.ia.enabled = true;
        a.db.ia.mode = Mode::FreeThenPaid;
        a.db.ia.agents = vec![
            Agent { base_url: url.clone(), api_key: "k".into(), ..Agent::new(1, Provider::OpenRouter, "saturado", true) },
            Agent { base_url: url.clone(), api_key: "k".into(), ..Agent::new(2, Provider::Custom, "bueno", true) },
            Agent { base_url: url, api_key: "k".into(), ..Agent::new(3, Provider::OpenAi, "caro", false) },
        ];
        img_id = a.db.images.iter().find(|i| i.patient_id == 101 && i.kind == crate::model::ImageKind::Periapical).map(|i| i.id).unwrap();
        a.open_patient(101, crate::app::PatTab::Imagenes);
        a.img.selected = Some(img_id);
    }
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("✨ Analizar esta imagen").click();
    for _ in 0..300 {
        h.step();
        if !h.state().db.ia_reports.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let r = h.state().db.ia_reports.first().cloned().expect("no se guardó el análisis");
    assert_eq!(r.image_id, img_id);
    assert_eq!(r.model, "bueno");
    assert_eq!(r.attempts.len(), 2, "el primero falló y respondió el segundo");
    assert_eq!(srv.join().unwrap(), vec!["saturado", "bueno"], "el de pago no se usa");
    let planned = |a: &App| a.db.entries.iter().filter(|e| e.patient_id == 101 && e.teeth == vec![47] && e.state == crate::model::TxState::Previsto).count();
    let before = planned(h.state());
    for _ in 0..3 {
        h.step();
    }
    // es un borrador: sin revisar, no pasa nada al plan
    h.get_by_label("Añadir al plan de tratamiento").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(planned(h.state()), before, "sin revisar, nada");
    // lo revisa una profesional colegiada, marca la propuesta y la pasa al plan
    {
        let a = h.state_mut();
        let u = a.db.users.iter().find(|u| u.doctor_id.and_then(|d| a.db.doctor(d)).is_some_and(|c| crate::finance::collaborator_problem(c).is_none())).cloned().unwrap();
        a.user_id = Some(u.id);
        a.db.config.current_user = u.name.clone();
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("✔ Revisado: lo doy por bueno").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(h.state().db.ia_reports[0].review.as_ref().is_some_and(|v| v.valid));
    {
        let a = h.state_mut();
        let rid = a.db.ia_reports[0].id;
        // (la casilla de la propuesta: las propuestas no vienen marcadas)
        a.ia.checked.insert((rid, 0));
    }
    h.step();
    h.get_by_label("Añadir al plan de tratamiento").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(planned(h.state()), before + 1, "la obturación propuesta pasa al plan");
    let last = h.state().db.entries.iter().rfind(|e| e.patient_id == 101 && e.teeth == vec![47]).unwrap();
    assert_eq!(last.odg, crate::model::OdgKind::Obturacion);
    assert_eq!(h.state().db.ia_reports[0].planned, vec![0]);
}

#[test]
fn lab_order_sent_by_email_with_attachments() {
    let (port, srv) = crate::mail::tests::fake_smtp();
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 2400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let (wid, label);
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.db.mail = crate::mail::MailConfig { enabled: true, host: "127.0.0.1".into(), port, security: crate::mail::Security::Ninguna, from_name: "Clínica".into(), from_email: "clinica@example.com".into(), ..Default::default() };
        let lab = a.db.suppliers.iter().find(|s| s.is_lab).map(|s| s.id).unwrap();
        a.db.suppliers.iter_mut().find(|s| s.id == lab).unwrap().email = "lab@example.com".into();
        wid = a.db.new_lab_work(101, None, lab, 7);
        let img = a.db.images.iter().find(|i| i.patient_id == 101 && i.kind == crate::model::ImageKind::Panoramica).unwrap();
        label = format!("Panorámica · {}", crate::model::fmt_date(img.date));
        a.stock.lab_sel = Some(wid);
        a.screen = crate::app::Screen::Laboratorio;
    }
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label(&label).click();
    h.get_by_label("Sin el nombre del paciente (iniciales y nº de historia)").click();
    h.step();
    let w = h.state().db.lab_works.iter().find(|w| w.id == wid).unwrap().clone();
    assert_eq!(w.attachments.len(), 1, "se marca la panorámica");
    assert!(w.anonymize);
    h.get_by_label("✉ Enviar por correo").click();
    for _ in 0..300 {
        h.step();
        if h.state().db.lab_works.iter().find(|w| w.id == wid).unwrap().order_sent.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let w = h.state().db.lab_works.iter().find(|w| w.id == wid).unwrap().clone();
    assert!(w.order_sent.is_some(), "no se envió: {:?}", h.state().stock.lab_msg);
    assert_eq!(w.order_to, "lab@example.com");
    let (cmds, data) = srv.join().unwrap();
    assert!(cmds.iter().any(|c| c.to_uppercase().starts_with("RCPT TO:<LAB@EXAMPLE.COM>")));
    assert!(data.contains("application/zip") && data.contains(&format!("Orden_{wid}_historia_")), "lleva el paquete con la orden");
    assert!(!data.contains("Carlos"), "sin el nombre del paciente");
}

#[test]
fn mail_settings_send_test_email() {
    let (port, srv) = crate::mail::tests::fake_smtp();
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1200.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.db.mail = crate::mail::MailConfig { host: "127.0.0.1".into(), port, security: crate::mail::Security::Ninguna, from_email: "clinica@example.com".into(), ..Default::default() };
        a.db.config.email = "recepcion@example.com".into();
        crate::screens::config::open(a, crate::screens::config::CfgTab::Correo);
    }
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("Enviar correos desde VeodarDental").click();
    h.step();
    assert!(h.state().db.mail.enabled);
    h.get_by_label("Enviar correo de prueba").click();
    for _ in 0..300 {
        h.step();
        if h.state().mail_ui.test_result.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(matches!(h.state().mail_ui.test_result, Some(Ok(_))), "{:?}", h.state().mail_ui.test_result);
    let (cmds, data) = srv.join().unwrap();
    assert!(cmds.iter().any(|c| c.to_uppercase().starts_with("RCPT TO:<RECEPCION@EXAMPLE.COM>")));
    assert!(data.contains("Subject: Correo de prueba de VeodarDental"));
}

#[test]
fn online_requests_confirm_and_reject_from_agenda() {
    use crate::booking::ReqState;
    let (port, srv) = crate::mail::tests::fake_smtp();
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1600.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let (laura, raul);
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.db.mail = crate::mail::MailConfig { enabled: true, host: "127.0.0.1".into(), port, security: crate::mail::Security::Ninguna, from_email: "clinica@example.com".into(), ..Default::default() };
        a.db.booking.email_patient = true;
        let mut reqs: Vec<_> = a.db.booking_requests.iter().filter(|r| r.state == ReqState::Pendiente).cloned().collect();
        reqs.sort_by_key(|r| (r.date, r.start));
        assert_eq!(reqs.len(), 2, "dos solicitudes de ejemplo");
        laura = reqs[0].id;
        raul = reqs[1].id;
        assert!(reqs[0].patient_id.is_some(), "Laura ya es paciente: se reconoce por el móvil");
        // sólo se avisa por correo a Laura (el servidor de prueba atiende un envío)
        a.db.booking_requests.iter_mut().find(|r| r.id == raul).unwrap().email.clear();
        a.agenda.date = reqs[0].date;
        a.screen = crate::app::Screen::Agenda;
    }
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("Citas pedidas por internet");
    h.get_all_by_label("Confirmar").next().unwrap().click();
    for _ in 0..3 {
        h.step();
    }
    let r = h.state().db.booking_requests.iter().find(|r| r.id == laura).unwrap().clone();
    assert_eq!(r.state, ReqState::Confirmada);
    let appt = h.state().db.appointments.iter().find(|a| Some(a.id) == r.appointment_id).unwrap().clone();
    assert_eq!((appt.patient_id, appt.date, appt.start, appt.doctor_id), (r.patient_id, r.date, r.start, r.doctor_id));
    let (cmds, data) = srv.join().unwrap();
    assert!(cmds.iter().any(|c| c.to_uppercase().starts_with("RCPT TO:<LAURA.FERNANDEZ@EXAMPLE.COM>")));
    assert!(data.contains("cita.ics") && data.contains("text/calendar"), "lleva la cita para el calendario");
    // rechazar la otra con un motivo
    h.get_by_label("Rechazar").click();
    h.step();
    h.step();
    h.state_mut().booking.reject_text = "Ese día la doctora no pasa consulta".into();
    h.get_by_label("Rechazar solicitud").click();
    for _ in 0..3 {
        h.step();
    }
    let r = h.state().db.booking_requests.iter().find(|r| r.id == raul).unwrap().clone();
    assert_eq!(r.state, ReqState::Rechazada);
    assert_eq!(r.answer, "Ese día la doctora no pasa consulta");
    assert_eq!(h.state().db.booking_pending(), 0);
    assert!(h.query_by_label("Citas pedidas por internet").is_none(), "sin pendientes no se muestra la tarjeta");
}

#[test]
fn patient_link_actions_reach_the_agenda() {
    use crate::booking::ReqState;
    use chrono::Datelike;
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1600.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let (id, token);
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.db.booking_requests.clear();
        let b = &mut a.db.booking;
        b.enabled = true;
        b.station = crate::shared::machine_name();
        b.port = 0;
        b.min_notice_hours = 0;
        b.cancel_hours = 0;
        b.public_url = "https://citas.example.com".into();
        let mut day = a.today + chrono::Duration::days(3);
        while day.weekday().num_days_from_monday() >= 5 || a.db.holidays.iter().any(|h| h.date == day) {
            day += chrono::Duration::days(1);
        }
        id = a.db.new_id();
        a.db.appointments.push(crate::model::Appointment {
            id,
            date: day,
            start: chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            minutes: 30,
            patient_id: Some(101),
            free_name: String::new(),
            phone: String::new(),
            doctor_id: 2,
            box_id: 2,
            reason: "Revisión implante".into(),
            treatment_code: "0102".into(),
            state: crate::model::ApptState::Planificada,
            notes: String::new(),
            arrived_at: None,
            seen_at: None,
            finished_at: None,
            reminder_sent: false,
            first_visit: false,
            center_id: 1,
        });
        token = a.db.appt_token(id);
        a.screen = crate::app::Screen::Agenda;
    }
    for _ in 0..4 {
        h.step();
    }
    let port = h.state().booking.server.as_ref().map(|s| s.port).expect("la página no se abrió");
    let post = |p: &str, v: serde_json::Value| {
        ureq::post(&format!("http://127.0.0.1:{port}/api/cita/{token}/{p}")).set("Content-Type", "application/json").send_string(&v.to_string()).map(|r| r.status()).unwrap_or_else(|e| match e {
            ureq::Error::Status(c, _) => c,
            _ => 0,
        })
    };
    let wait = |h: &mut Harness<'static, App>, f: &dyn Fn(&App) -> bool| {
        for _ in 0..200 {
            h.step();
            if f(h.state()) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    };
    // confirma que irá
    assert_eq!(post("confirmar", serde_json::json!({})), 200);
    assert!(wait(&mut h, &|a| a.db.appointments.iter().any(|x| x.id == id && x.state == crate::model::ApptState::Confirmada)));
    // rellena el cuestionario: va a su ficha, sin firmar, con sus avisos
    assert_eq!(post("salud", serde_json::json!({"respuestas": {"alergia_med": true}, "detalles": {"alergia_med": "Ibuprofeno"}, "acepta": true})), 200);
    assert!(wait(&mut h, &|a| a.db.latest_health(101).is_some_and(|f| f.filled_by.contains("internet"))));
    assert!(h.state().db.patient(101).unwrap().allergies.contains("Ibuprofeno"));
    // pide otra hora: llega a la agenda como cambio y recepción lo acepta
    let days: serde_json::Value = serde_json::from_str(&ureq::get(&format!("http://127.0.0.1:{port}/api/cita/{token}/huecos")).call().unwrap().into_string().unwrap()).unwrap();
    // el primer día con al menos dos horas libres (a última hora, hoy puede no quedar ninguna)
    let day = days["dias"].as_array().unwrap().iter().find(|d| d["horas"].as_array().is_some_and(|h| h.len() >= 2)).expect("sin días con huecos").clone();
    let (fecha, hora) = (day["fecha"].as_str().unwrap().to_string(), day["horas"][1]["hora"].as_str().unwrap().to_string());
    assert_eq!(post("cambiar", serde_json::json!({"fecha": fecha, "hora": hora})), 200);
    assert!(wait(&mut h, &|a| a.db.booking_requests.iter().any(|r| r.replaces == Some(id) && r.state == ReqState::Pendiente)));
    h.step();
    h.get_by_label("Citas pedidas por internet");
    h.get_all_by_label("Confirmar").next().unwrap().click();
    for _ in 0..3 {
        h.step();
    }
    let a = h.state().db.appointments.iter().find(|x| x.id == id).unwrap().clone();
    assert_eq!((a.date.format("%Y-%m-%d").to_string(), a.start.format("%H:%M").to_string()), (fecha, hora), "la cita se ha movido");
    assert!(a.notes.contains("Cambiada por el paciente"));
    // se mueve la misma cita: no se crea otra
    let req = h.state().db.booking_requests.iter().find(|r| r.replaces == Some(id)).unwrap().clone();
    assert_eq!((req.state, req.appointment_id), (ReqState::Confirmada, Some(id)));
    assert_eq!(h.state().db.appointments.iter().filter(|x| x.patient_id == Some(101) && x.date == a.date && x.start == a.start).count(), 1, "no se duplica");
    // en la ventana de la cita se ve lo que hizo el paciente
    h.state_mut().dialog = crate::app::Dialog::Appointment(crate::screens::agenda::ApptDraft::from(&a));
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Enlace del paciente («Mi cita»)");
    h.get_by_label("Copiar enlace");
    let l = h.state().db.appt_link(id).unwrap().clone();
    assert!(l.confirmed.is_some() && l.health_at.is_some() && l.cancelled.is_none());
}

#[test]
fn budget_signed_online_reaches_the_patient_file() {
    use crate::model::BudgetState;
    let (mail_port, mail) = crate::mail::tests::fake_smtp();
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1800.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let (bid, pid);
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.db.booking_requests.clear();
        a.db.booking.enabled = true;
        a.db.booking.station = crate::shared::machine_name();
        a.db.booking.port = 0;
        a.db.mail = crate::mail::MailConfig { enabled: true, host: "127.0.0.1".into(), port: mail_port, security: crate::mail::Security::Ninguna, from_email: "clinica@example.com".into(), ..Default::default() };
        let today = a.today;
        a.db.budget_links.clear(); // sin el enlace de ejemplo
        let b = a.db.budgets.iter().find(|b| b.state == BudgetState::Pendiente && !b.lines.is_empty() && b.expires() >= today).cloned().expect("presupuesto pendiente");
        bid = b.id;
        pid = b.patient_id;
        a.db.patients.iter_mut().find(|p| p.id == pid).unwrap().email = "paciente@example.com".into();
        // la ventana del presupuesto crea y muestra el enlace
        a.dialog = crate::app::Dialog::Budget(bid);
    }
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("Firma por internet");
    h.get_by_label("Copiar enlace").click();
    for _ in 0..3 {
        h.step();
    }
    let token = h.state().db.budget_link(bid).expect("enlace creado").token.clone();
    let port = h.state().booking.server.as_ref().map(|s| s.port).expect("la página no se abrió");
    // la página tiene el enlace en cuanto se publica
    let url = format!("http://127.0.0.1:{port}/api/presupuesto/{token}");
    let mut info = String::new();
    for _ in 0..200 {
        h.step();
        if let Ok(r) = ureq::get(&url).call() {
            info = r.into_string().unwrap();
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let v: serde_json::Value = serde_json::from_str(&info).expect("el enlace no llegó a publicarse");
    let firma: Vec<Vec<[f32; 2]>> = vec![(0..25).map(|i| [i as f32 / 25.0, 0.4 + (i % 5) as f32 * 0.05]).collect()];
    let status = ureq::post(&format!("{url}/aceptar"))
        .set("Content-Type", "application/json")
        .send_string(&serde_json::json!({"nombre": "Ana García López", "firma": firma, "cuotas": 0, "huella": v["huella"], "acepta": true}).to_string())
        .map(|r| r.status())
        .unwrap_or(0);
    assert_eq!(status, 200);
    let mut ok = false;
    for _ in 0..200 {
        h.step();
        if h.state().db.budgets.iter().any(|b| b.id == bid && b.state == BudgetState::Aceptado) {
            ok = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(ok, "el presupuesto no quedó aceptado");
    let l = h.state().db.budget_link(bid).unwrap().clone();
    assert!(l.accepted && l.viewed.is_some() && l.signer == "Ana García López");
    let doc = h.state().db.documents.iter().find(|d| Some(d.id) == l.doc_id).unwrap().clone();
    assert!(doc.signed && doc.patient_id == pid && !doc.signature.is_empty());
    assert!(h.state().db.entries.iter().any(|e| e.budget_id == Some(bid)), "los tratamientos pasan al plan del paciente");
    // al paciente le llega la copia firmada
    let (cmds, data) = mail.join().unwrap();
    assert!(cmds.iter().any(|c| c.to_uppercase().starts_with("RCPT TO:<PACIENTE@EXAMPLE.COM>")));
    assert!(data.contains(&format!("presupuesto_{bid}_firmado.html")));
    for _ in 0..3 {
        h.step();
    }
    assert!(h.query_by_label_contains("Firmado por Ana García López").is_some());
    h.get_by_label("Ver la copia firmada");
}

/// El túnel de Cloudflare se abre solo al arrancar (con un «cloudflared» de mentira) y se cierra desde Configuración.
#[cfg(unix)]
#[test]
fn cloudflare_tunnel_opens_at_start_and_closes() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("veodardental-tunel-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("cloudflared");
    std::fs::write(&exe, "#!/bin/sh\necho \"2026-11-08T10:00:01Z INF Registered tunnel connection connIndex=0\" >&2\necho \"2026-11-08T10:00:01Z INF Registered tunnel connection connIndex=1\" >&2\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 2600.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        let b = &mut a.db.booking;
        b.enabled = true;
        b.station = crate::shared::machine_name();
        b.port = 0;
        b.tunnel_exe = exe.to_string_lossy().into_owned();
        b.tunnel_token = "token-de-prueba".into();
        b.tunnel_auto = true;
        crate::screens::config::open(a, crate::screens::config::CfgTab::CitaOnline);
    }
    let mut connected = false;
    for _ in 0..150 {
        h.step();
        if h.query_by_label_contains("Conectado · 2 conexiones").is_some() {
            connected = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(connected, "el túnel no se abrió solo al arrancar: {:?}", h.state().booking.tunnel_error);
    h.get_by_label("Desconectar").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(h.state().booking.tunnel.is_none());
    h.get_by_label("Sin publicar");
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_dir(&dir);
}

/// La protección de datos firmada por internet llega a la ficha (firmada, con la aceptación marcada) y el paciente recibe su copia.
#[test]
fn document_signed_online_reaches_the_patient_file() {
    let (mail_port, mail) = crate::mail::tests::fake_smtp();
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1800.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let (doc, pid);
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.db.booking_requests.clear();
        a.db.doc_links.clear();
        a.db.booking.enabled = true;
        a.db.booking.station = crate::shared::machine_name();
        a.db.booking.port = 0;
        a.db.mail = crate::mail::MailConfig { enabled: true, host: "127.0.0.1".into(), port: mail_port, security: crate::mail::Security::Ninguna, from_email: "clinica@example.com".into(), ..Default::default() };
        pid = 110;
        let p = a.db.patients.iter_mut().find(|p| p.id == pid).unwrap();
        p.email = "paciente@example.com".into();
        p.accepts_gdpr = false;
        let tid = a.db.doc_templates.iter().find(|t| t.marks_gdpr).map(|t| t.id).unwrap();
        doc = a.db.new_document(pid, tid, &[]).unwrap();
        a.docs.selected = Some(doc);
        a.docs_mode = 0;
        a.open_patient(pid, crate::app::PatTab::Documentos);
    }
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("Firma por internet");
    h.get_by_label("Copiar enlace").click();
    for _ in 0..3 {
        h.step();
    }
    let token = h.state().db.doc_link(doc).expect("enlace creado").token.clone();
    let port = h.state().booking.server.as_ref().map(|s| s.port).expect("la página no se abrió");
    let url = format!("http://127.0.0.1:{port}/api/documento/{token}");
    let mut info = String::new();
    for _ in 0..200 {
        h.step();
        if let Ok(r) = ureq::get(&url).call() {
            info = r.into_string().unwrap();
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let v: serde_json::Value = serde_json::from_str(&info).expect("el enlace no llegó a publicarse");
    let firma: Vec<Vec<[f32; 2]>> = vec![(0..25).map(|i| [i as f32 / 25.0, 0.4 + (i % 5) as f32 * 0.05]).collect()];
    let status =
        ureq::post(&format!("{url}/firmar")).set("Content-Type", "application/json").send_string(&serde_json::json!({"nombre": "Paciente Firmante", "firma": firma, "huella": v["huella"], "acepta": true}).to_string()).map(|r| r.status()).unwrap_or(0);
    assert_eq!(status, 200);
    let mut ok = false;
    for _ in 0..200 {
        h.step();
        if h.state().db.documents.iter().any(|d| d.id == doc && d.signed) {
            ok = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(ok, "el documento no quedó firmado");
    let d = h.state().db.documents.iter().find(|d| d.id == doc).unwrap().clone();
    assert_eq!(d.signer, "Paciente Firmante");
    assert!(!d.signature.is_empty());
    assert!(h.state().db.patient(pid).unwrap().accepts_gdpr, "la protección de datos queda aceptada en la ficha");
    let l = h.state().db.doc_link(doc).unwrap().clone();
    assert!(l.signed.is_some() && l.viewed.is_some() && l.ip == "127.0.0.1");
    // al paciente le llega la copia firmada
    let (cmds, data) = mail.join().unwrap();
    assert!(cmds.iter().any(|c| c.to_uppercase().starts_with("RCPT TO:<PACIENTE@EXAMPLE.COM>")));
    assert!(data.contains("_firmado.html"));
    for _ in 0..3 {
        h.step();
    }
    assert!(h.query_by_label_contains("Firmado por internet el").is_some());
}

/// El envío automático del día prepara los recordatorios y los manda por correo él solo, una vez al día.
#[test]
fn daily_emails_are_sent_automatically() {
    use crate::comms::{Channel, MsgKind, MsgState};
    let (port, srv) = crate::mail::tests::fake_smtp_many(2);
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1200.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let today;
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.db.messages.clear();
        today = a.today;
        // sólo el recordatorio por correo, y dos citas mañana de pacientes que aceptan correo
        for t in a.db.msg_templates.iter_mut() {
            t.active = t.kind == MsgKind::RecordatorioCita && t.channel == Channel::Email;
        }
        let tomorrow = today + chrono::Duration::days(1);
        // solo estas dos citas por venir (un viernes también tocan las del lunes)
        a.db.appointments.retain(|x| x.date <= today);
        for (k, pid) in [101u32, 102].into_iter().enumerate() {
            let p = a.db.patients.iter_mut().find(|p| p.id == pid).unwrap();
            p.accepts_gdpr = true;
            p.accepts_email = true;
            p.email = format!("p{pid}@example.com");
            let id = a.db.new_id();
            a.db.appointments.push(crate::model::Appointment {
                id,
                date: tomorrow,
                start: chrono::NaiveTime::from_hms_opt(9 + k as u32, 0, 0).unwrap(),
                minutes: 30,
                patient_id: Some(pid),
                free_name: String::new(),
                phone: String::new(),
                doctor_id: 1,
                box_id: 1,
                reason: "Revisión".into(),
                treatment_code: "0102".into(),
                state: crate::model::ApptState::Planificada,
                notes: String::new(),
                arrived_at: None,
                seen_at: None,
                finished_at: None,
                reminder_sent: false,
                first_visit: false,
                center_id: 1,
            });
        }
        a.db.mail = crate::mail::MailConfig {
            enabled: true,
            host: "127.0.0.1".into(),
            port,
            security: crate::mail::Security::Ninguna,
            from_email: "clinica@example.com".into(),
            auto_send: true,
            auto_hour: 0,
            auto_until: 24,
            auto_station: crate::shared::machine_name(),
            ..Default::default()
        };
        a.screen = crate::app::Screen::Comunicaciones;
    }
    let done = |a: &App| a.db.mail.last_auto == Some(a.today) && a.comms.outbox.is_none() && a.db.messages.len() == 2 && a.db.messages.iter().all(|m| m.state == MsgState::Enviado);
    let mut ok = false;
    for _ in 0..300 {
        h.step();
        if done(h.state()) {
            ok = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(ok, "no se hizo el envío automático: {:?}", h.state().db.messages.iter().map(|m| (m.channel, m.state, m.error.clone())).collect::<Vec<_>>());
    assert!(h.state().db.messages.iter().all(|m| m.channel == Channel::Email && m.kind == MsgKind::RecordatorioCita));
    let got = srv.join().unwrap();
    assert_eq!(got.len(), 2);
    for pid in [101, 102] {
        assert!(got.iter().any(|(cmds, data)| cmds.iter().any(|c| c.to_uppercase() == format!("RCPT TO:<P{pid}@EXAMPLE.COM>")) && data.contains("Recordatorio")), "falta el correo de {pid}");
    }
    // no se repite el mismo día
    for _ in 0..5 {
        h.step();
    }
    assert!(h.state().comms.outbox.is_none());
    assert_eq!(h.state().db.messages.len(), 2);
    assert!(h.query_by_label_contains("hoy ya se ha hecho (2 enviados)").is_some());
}

/// Un paciente anula por internet; el hueco se ofrece solo a la lista de espera y quien lo pide se lo queda.
#[test]
fn cancelled_slot_goes_to_the_waiting_list() {
    use chrono::Datelike;
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1600.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let (appt, token, day);
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.db.booking_requests.clear();
        a.db.slot_offers.clear();
        let b = &mut a.db.booking;
        b.enabled = true;
        b.station = crate::shared::machine_name();
        b.port = 0;
        b.min_notice_hours = 0;
        b.cancel_hours = 0;
        b.offer_slots = true;
        b.offer_min_hours = 1;
        b.public_url = "https://citas.example.com".into();
        let mut d = a.today + chrono::Duration::days(3);
        while d.weekday().num_days_from_monday() >= 5 || a.db.holidays.iter().any(|x| x.date == d) {
            d += chrono::Duration::days(1);
        }
        day = d;
        // la higienista sólo tiene, ese día, la cita de Carlos a las 10:00; Javier (106) espera con ella por las mañanas
        a.db.appointments.retain(|x| !(x.date == day && x.doctor_id == 4));
        appt = a.db.new_id();
        a.db.appointments.push(crate::model::Appointment {
            id: appt,
            date: day,
            start: chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            minutes: 45,
            patient_id: Some(101),
            free_name: String::new(),
            phone: String::new(),
            doctor_id: 4,
            box_id: 4,
            reason: "Higiene".into(),
            treatment_code: "0201".into(),
            state: crate::model::ApptState::Planificada,
            notes: String::new(),
            arrived_at: None,
            seen_at: None,
            finished_at: None,
            reminder_sent: false,
            first_visit: false,
            center_id: 1,
        });
        token = a.db.appt_token(appt);
        assert!(a.db.waitlist.iter().any(|w| w.patient_id == 106 && w.doctor_id == 4));
        a.screen = crate::app::Screen::Agenda;
        a.agenda.date = day;
    }
    for _ in 0..4 {
        h.step();
    }
    let port = h.state().booking.server.as_ref().map(|s| s.port).expect("la página no se abrió");
    let post = |p: &str| {
        ureq::post(&format!("http://127.0.0.1:{port}{p}")).set("Content-Type", "application/json").send_string("{}").map(|r| r.status()).unwrap_or_else(|e| match e {
            ureq::Error::Status(c, _) => c,
            _ => 0,
        })
    };
    let wait = |h: &mut Harness<'static, App>, f: &dyn Fn(&App) -> bool| {
        for _ in 0..200 {
            h.step();
            if f(h.state()) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    };
    // Carlos anula desde su enlace: el hueco se ofrece solo a Javier
    assert_eq!(post(&format!("/api/cita/{token}/anular")), 200);
    assert!(wait(&mut h, &|a| a.db.slot_offers.iter().any(|o| o.source_appt == Some(appt))), "no se ofreció el hueco");
    let offer = h.state().db.slot_offers.iter().find(|o| o.source_appt == Some(appt)).unwrap().clone();
    assert_eq!(offer.invites.iter().map(|i| i.patient_id).collect::<Vec<_>>(), vec![106]);
    assert!(h.state().db.messages.iter().any(|m| m.kind == crate::comms::MsgKind::HuecoLibre && m.patient_id == 106 && m.text.contains("https://citas.example.com/hueco/")), "aviso con su enlace");
    // Javier lo pide desde su enlace
    let t = offer.invites[0].token.clone();
    let mut published = false;
    for _ in 0..200 {
        h.step();
        if ureq::get(&format!("http://127.0.0.1:{port}/api/hueco/{t}")).call().is_ok() {
            published = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(published, "el enlace del hueco no llegó a publicarse");
    assert_eq!(post(&format!("/api/hueco/{t}/reservar")), 200);
    assert!(wait(&mut h, &|a| a.db.appointments.iter().any(|x| x.patient_id == Some(106) && x.date == day && x.doctor_id == 4 && x.state == crate::model::ApptState::Confirmada)), "no se le dio la cita");
    assert!(!h.state().db.waitlist.iter().any(|w| w.patient_id == 106 && w.doctor_id == 4), "sale de la lista de espera");
    for _ in 0..3 {
        h.step();
    }
    assert!(h.query_by_label_contains("Lo ha cogido").is_some(), "la agenda muestra quién se lo quedó");
}

/// Puesta en marcha: el aviso de Inicio lleva a la lista de lo que falta;
/// «Empezar con la clínica vacía» quita los pacientes de ejemplo sin tocar la
/// configuración, la revisión legal se marca a mano, y después todas las
/// pantallas siguen abriéndose con la clínica vacía.
#[test]
fn go_live_checklist_and_empty_clinic() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1300.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let treatments;
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.screen = crate::app::Screen::Inicio;
        treatments = a.db.treatments.len();
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label_contains("Puesta en marcha: faltan 7 pasos").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().screen, crate::app::Screen::Config);
    assert_eq!(h.state().lists.cfg_tab, crate::screens::config::CfgTab::Marcha);
    h.get_by_label("Faltan 7 de 8 pasos imprescindibles");
    h.get_by_label_contains("Siguen los profesionales de ejemplo");
    h.get_by_label("Empezar con la clínica vacía…").click();
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label_contains("Se quitarán los");
    h.get_by_label("Sí, continuar").click();
    for _ in 0..3 {
        h.step();
    }
    {
        let a = h.state();
        assert!(a.db.patients.is_empty() && a.db.appointments.is_empty() && a.db.invoices.is_empty());
        assert_eq!(a.db.treatments.len(), treatments);
        assert!(!a.db.users.is_empty() && !a.db.collaborators.is_empty());
        assert!(a.patient.is_none());
    }
    h.get_by_label("Faltan 6 de 8 pasos imprescindibles");
    h.get_by_label_contains("Sin datos de ejemplo");
    h.get_by_label("Marcar como hecho").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().db.golive.len(), 1);
    h.get_by_label("Faltan 5 de 8 pasos imprescindibles");
    // todas las pantallas con la clínica vacía
    for s in crate::app::Screen::NAV.iter().map(|x| x.0) {
        h.state_mut().screen = s;
        h.step();
        h.step();
    }
    for t in crate::screens::config::CfgTab::GROUPS.iter().flat_map(|g| g.1.iter()) {
        crate::screens::config::open(h.state_mut(), *t);
        h.step();
        h.step();
    }
    let p = crate::screens::pacientes::blank_patient(&h.state().db, h.state().today);
    assert!(p.first_name.is_empty());
}

/// Copias cifradas: se pone la contraseña de las copias en Configuración, la
/// copia que ya había en la segunda carpeta queda cifrada, y para restaurarla
/// en otro ordenador (sin la clave de este) se abre con la contraseña.
#[test]
fn backup_password_encrypts_the_second_copy() {
    let extra = std::env::temp_dir().join(format!("ds_ui_bk_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&extra);
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1700.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        crate::tools::create_backup_in(&a.db, &extra, "antigua").unwrap();
        a.db.config.backup_extra = extra.to_string_lossy().to_string();
        crate::screens::config::open(a, crate::screens::config::CfgTab::Copias);
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label_contains("sin cifrar");
    {
        let a = h.state_mut();
        a.tools.key_pw = "Sonrisa-2026".into();
        a.tools.key_pw2 = "Sonrisa-2026".into();
    }
    h.get_by_label("Cifrar la segunda copia").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(crate::cifrado::current_key().is_some(), "{:?}", h.state().tools.key_msg);
    let list = crate::tools::list_backups(&extra);
    assert_eq!(list.len(), 1);
    assert!(list[0].encrypted, "la copia antigua sigue sin cifrar");
    h.get_by_label_contains("se guarda cifrada desde");
    h.get_by_label_contains("1 copia; la última");
    h.get_by_label_contains(", cifrada.");
    // en otro ordenador no está la clave: hace falta la contraseña
    crate::cifrado::set_key(None).unwrap();
    h.state_mut().tools.restore = Some(list[0].path.clone());
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label(crate::tools::NEEDS_PASSWORD);
    h.state_mut().tools.restore_pw = "Sonrisa-2026".into();
    h.get_by_label("Abrir la copia").click();
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label_contains("La copia contiene:");
    h.get_by_label("Cancelar").click();
    for _ in 0..2 {
        h.step();
    }
    assert!(h.state().tools.restore.is_none());
    let _ = std::fs::remove_dir_all(&extra);
}

/// Camino a las farmacias reales: se prueba el circuito con una farmacia
/// simulada, se genera el expediente para pedir la certificación, se marca
/// la solicitud como enviada y se ven los 30 controles de la auditoría.
#[test]
fn erx_path_to_real_pharmacies() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1900.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        crate::screens::config::open(a, crate::screens::config::CfgTab::Receta);
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Probar con una farmacia simulada").click();
    for _ in 0..3 {
        h.step();
    }
    let (_, steps) = h.state().rx.circuit.clone().expect("no se ha probado el circuito");
    for s in &steps {
        assert!(s.ok, "{} · {} · {}", s.control, s.what, s.detail);
    }
    h.get_by_label_contains("pruebas correctas el");
    h.get_by_label("Generar el expediente").click();
    for _ in 0..3 {
        h.step();
    }
    let path = h.state().db.audit_log.iter().rev().find(|a| a.action.starts_with("Expediente de certificación")).map(|a| a.detail.clone()).expect("no se generó el expediente");
    let html = std::fs::read_to_string(&path).unwrap();
    assert!(html.contains("FT12") && html.contains("URLRepositorio") && html.contains(&format!("{} de {} correctas", steps.len(), steps.len())));
    let _ = std::fs::remove_file(&path);
    h.get_by_label("Ya la he enviado: marcarla con la fecha de hoy").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().db.erx.cert_requested, Some(h.state().today));
    h.get_by_label("Ver los controles").click();
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("IN03");
    h.get_by_label("Verificación interoperable de certificados");
}

/// La hoja de las recetas emitidas se envía al paciente por correo, con la
/// hoja adjunta (remisión telemática, control FT12).
#[test]
fn erx_patient_sheet_goes_by_email() {
    let (port, srv) = crate::mail::tests::fake_smtp();
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1300.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let rid;
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        let pid = a.db.patients[0].id;
        a.db.patients[0].email = "paciente@example.com".into();
        a.db.erx.direct = true;
        a.db.mail = crate::mail::MailConfig { enabled: true, host: "127.0.0.1".into(), port, security: crate::mail::Security::Ninguna, from_email: "clinica@example.com".into(), ..Default::default() };
        // con la página publicada, el correo lleva también el enlace «Mis recetas»
        a.db.booking.enabled = true;
        a.db.booking.public_url = "https://citas.ejemplo.es".into();
        a.db.rx_links.clear();
        rid = a.db.new_id();
        let today = a.today;
        let item = crate::srep::RPrescripcion {
            id_prescripcion: "a".repeat(32),
            id_acceso: "b".repeat(32),
            producto: crate::srep::RProducto { denominacion: "IBUPROFENO 600 MG 40 COMPRIMIDOS".into(), ..Default::default() },
            recetas: vec![crate::srep::RReceta { id_receta: "c".repeat(32), fecha_ini: today, fecha_fin: today + chrono::Duration::days(10), num_envases: 1, estado: 1, ..Default::default() }],
            ..Default::default()
        };
        let issue = crate::finance::SrepIssue { id_acceso: "b".repeat(32), issued: vec![item], issued_at: Some(crate::model::now()), signer: "Dra. Ruiz".into(), test: true, confidential: true, ..Default::default() };
        a.db.prescriptions.push(crate::finance::Prescription {
            id: rid,
            patient_id: pid,
            doctor_id: 1,
            date: today,
            diagnosis: "Dolor tras extracción".into(),
            items: vec![],
            notes: String::new(),
            erx_code: String::new(),
            erx_platform: String::new(),
            erx_date: None,
            srep: Some(issue),
            reviewed_warnings: vec![],
            reviewed_by: String::new(),
            reviewed_at: None,
        });
        a.patient = Some(pid);
        a.screen = crate::app::Screen::Pacientes;
        a.pat_tab = crate::app::PatTab::Documentos;
        a.docs_mode = 1;
        a.rx.selected = Some(rid);
    }
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("✉ Enviar la hoja por correo").click();
    for _ in 0..3 {
        h.step();
    }
    let (cmds, data) = srv.join().unwrap();
    for _ in 0..50 {
        h.step();
        if h.state().rx.mail_job.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(cmds.iter().any(|c| c.contains("paciente@example.com")), "{cmds:?}");
    assert!(data.contains(&format!("hoja_recetas_{rid}.html")), "la hoja no va adjunta");
    assert!(h.state().db.audit_log.iter().any(|a| a.action == "Hoja de la receta enviada por correo" && a.detail.contains("paciente@example.com")));
    // el enlace «Mis recetas» se creó para el correo, y el botón lo copia
    let pid = h.state().patient.unwrap();
    let token = h.state().db.rx_link(pid).map(|l| l.token.clone()).expect("el correo no llevaba el enlace «Mis recetas»");
    h.get_by_label("🔗 Enlace «Mis recetas»").click();
    for _ in 0..3 {
        h.step();
    }
    let toast = h.state().toast.as_ref().map(|t| t.0.clone()).unwrap_or_default();
    assert!(toast.contains(&format!("https://citas.ejemplo.es/recetas/{token}")), "{toast}");
    assert_eq!(h.state().db.rx_links.len(), 1, "un solo enlace por paciente");
}

/// Código del móvil: se activa en Usuarios y permisos con el código QR y un
/// código de la app; después, para entrar hacen falta la contraseña y el código.
#[test]
fn second_factor_login_with_the_phone_code() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1300.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let doc;
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        doc = a.db.users.iter().find(|u| u.name.starts_with("Dra. Ruiz")).map(|u| u.id).unwrap();
        crate::screens::config::open(a, crate::screens::config::CfgTab::Usuarios);
        a.login.edit_user = Some(doc);
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Activar el código del móvil").click();
    for _ in 0..3 {
        h.step();
    }
    let secret = h.state().login.enroll.clone().expect("no se ha empezado la activación").1;
    h.get_by_label_contains("Clave: ");
    let code = format!("{:06}", crate::totp::code_at(&secret, crate::totp::unix_now()).unwrap());
    h.state_mut().login.enroll.as_mut().unwrap().2 = code;
    h.get_by_label("Confirmar y activar").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().db.user(doc).unwrap().totp_secret, secret);
    // sale y entra la Dra. Ruiz: con la contraseña sola no basta
    {
        let a = h.state_mut();
        a.user_id = None;
        a.second_factor = false;
        a.login.selected = Some(doc);
        a.login.pin = "0000".into();
    }
    for _ in 0..2 {
        h.step();
    }
    h.get_by_label("Entrar").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(h.state().user_id.is_none(), "ha entrado sin el código del móvil");
    assert_eq!(h.state().login.pending_code, Some(doc));
    h.get_by_label_contains("Código del móvil de");
    // el código de la activación ya se usó: vale el siguiente de la app
    h.state_mut().login.code = format!("{:06}", crate::totp::code_at(&secret, crate::totp::unix_now() + crate::totp::STEP).unwrap());
    h.get_by_label("Entrar").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().user_id, Some(doc), "{}", h.state().login.error);
    assert!(h.state().second_factor);
}

/// Puesto en red: antes de entrar no tiene datos ni contraseñas; al entrar con
/// su usuario el servidor le da la sesión y los datos; si la sesión caduca,
/// vuelve a la entrada sin perder lo que tenía sin enviar.
#[test]
fn station_signs_in_through_the_server() {
    use crate::server::{HIDDEN, Remote, serve};
    let dir = std::env::temp_dir().join(format!("ds_ui_server_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("clinica.json");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    let initial = crate::model::Db::sample(crate::model::today());
    let p2 = path.clone();
    let identity = crate::server::new_identity().unwrap();
    let server = std::thread::spawn(move || serve(listener, p2, "CLAVE_UI".into(), identity, initial));

    let mut h = harness();
    h.state_mut().connect_to(Remote::new(&addr, "CLAVE_UI", 0)).unwrap();
    {
        let a = h.state();
        assert!(a.awaiting_data && !a.persist);
        assert!(a.db.patients.is_empty() && a.db.appointments.is_empty(), "sin entrar no hay datos de pacientes");
        assert!(a.db.users.iter().all(|u| u.pin_hash == HIDDEN && u.salt.is_empty()), "ni contraseñas");
    }
    // entra Recepción con su contraseña
    h.state_mut().login.selected = Some(2);
    h.step();
    h.event(egui::Event::Text("0000".into()));
    h.step();
    h.get_by_label("Entrar").click();
    for _ in 0..4 {
        h.step();
    }
    {
        let a = h.state();
        assert_eq!(a.user_id, Some(2), "no entra: {}", a.login.error);
        assert!(!a.awaiting_data && a.persist && !a.pending_start);
        assert!(!a.db.patients.is_empty(), "al entrar llegan los datos");
        assert_eq!(a.db.config.current_user, "Recepción");
    }
    // la contraseña inicial se cambia en el servidor
    h.state_mut().login.new_pin = "739154".into();
    h.state_mut().login.new_pin2 = "739154".into();
    h.step();
    h.get_by_label("Guardar").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(!h.state().login.show_change, "{}", h.state().login.error);
    let disk = || -> crate::model::Db { crate::sync::on_disk(&path) };
    assert!(crate::users::pin_matches(disk().user(2).unwrap(), "739154"));
    // un cambio del puesto llega al servidor
    {
        let a = h.state_mut();
        a.db.appointments[0].notes = "desde recepción".into();
        a.save_now().unwrap();
    }
    assert_eq!(disk().appointments[0].notes, "desde recepción");
    // la sesión deja de valer: vuelve a la entrada y el cambio pendiente se guarda al entrar
    {
        let a = h.state_mut();
        a.remote.as_mut().unwrap().session = "0".repeat(32);
        a.db.appointments[0].notes = "pendiente de enviar".into();
        a.changed();
        a.last_save = -10.0;
    }
    for _ in 0..3 {
        h.step();
    }
    assert!(h.state().user_id.is_none(), "con la sesión caducada vuelve a la entrada");
    assert!(h.state().dirty, "el cambio sigue pendiente");
    assert_eq!(h.state().login.error, crate::server::SESSION_EXPIRED);
    h.state_mut().login.selected = Some(2);
    h.state_mut().login.pin = "739154".into();
    h.step();
    h.get_by_label("Entrar").click();
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(h.state().user_id, Some(2), "{}", h.state().login.error);
    assert_eq!(disk().appointments[0].notes, "pendiente de enviar");
    assert!(!h.state().dirty);

    h.state().remote.clone().unwrap().stop().unwrap();
    server.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Recepción no ve las pestañas clínicas de la ficha ni las pantallas de
/// imágenes y ortodoncia; un doctor, sí.
#[test]
fn reception_does_not_see_clinical_tabs() {
    let mut h = harness();
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.profile == crate::users::Profile::Recepcion).map(|u| u.id);
        a.screen = crate::app::Screen::Pacientes;
        a.patient = a.db.patients.first().map(|p| p.id);
        a.lists.patient_list_open = false;
        a.pat_tab = crate::app::PatTab::Periodontograma;
    }
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().pat_tab, crate::app::PatTab::Resumen, "no se queda en una pestaña clínica");
    for tab in ["Salud", "Periodontograma", "Imágenes"] {
        assert!(h.query_by_label(tab).is_none(), "Recepción ve la pestaña {tab}");
    }
    h.get_by_label("Filiación");
    h.get_by_label("Documentos");
    assert!(!h.state().allowed(crate::app::Screen::Imagenes) && !h.state().allowed(crate::app::Screen::Ortodoncia));
    // un doctor sí
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.profile == crate::users::Profile::Doctor).map(|u| u.id);
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Periodontograma");
    assert!(h.state().allowed(crate::app::Screen::Imagenes));
}

/// Remesas SEPA: se abre una remesa cobrada, se elige un recibo y se registra
/// que el banco lo ha devuelto.
#[test]
fn returned_debit_from_the_remittances_screen() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 2400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        let bid = a.db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        a.db.accept_budget(bid, a.today);
        let due = a.db.due_installments(a.today + chrono::Duration::days(40));
        let rid = a.db.create_remittance(&due, a.today + chrono::Duration::days(5)).unwrap();
        a.db.remittance_collected(rid).unwrap();
        a.screen = crate::app::Screen::Cobros;
        a.cobros_tab = 1;
        a.rem.open = Some(rid);
        a.rem.item = Some(0);
        a
    });
    for _ in 0..5 {
        h.step();
    }
    let rid = h.state().rem.open.unwrap();
    let amount = h.state().db.remittances.iter().find(|r| r.id == rid).unwrap().items[0].amount;
    let bal = h.state().db.patient_balance(101);
    h.get_by_label_contains("Registrar la devolución").click();
    for _ in 0..4 {
        h.step();
    }
    let st = h.state();
    let it = &st.db.remittances.iter().find(|r| r.id == rid).unwrap().items[0];
    assert!(it.returned.is_some(), "no se registró la devolución");
    assert_eq!(it.return_reason, "AM04 Saldo insuficiente");
    assert!((st.db.patient_balance(101) - (bal + amount)).abs() < 0.001, "el paciente vuelve a deber la cuota");
    assert!(st.rem.item.is_none());
}

/// «Dar cita» desde un recall de higiene: con el profesional del recall y el
/// tratamiento de higiene del catálogo de la clínica (no los de los datos de ejemplo).
#[test]
fn recall_appointment_uses_the_clinic_catalog() {
    let h = harness();
    let mut a = h.into_state();
    // la clínica llama a su limpieza «HIG» (50 min) y la hace el Dr. con el id 2
    let t = a.db.treatments.iter_mut().find(|t| t.code == "0201").unwrap();
    t.code = "HIG".into();
    t.minutes = 50;
    let r = crate::model::Recall { id: 1, patient_id: 110, date: a.today, reason: "Higiene anual".into(), doctor_id: 2, state: crate::model::RecallState::Pendiente, channel: String::new(), comment: String::new(), contacted_on: None, appt_id: None };
    let d = crate::screens::recalls::appointment_for(&a, &r);
    assert_eq!((d.a.doctor_id, d.a.treatment_code.as_str(), d.a.minutes, d.a.patient_id), (2, "HIG", 50, Some(110)));
    // un profesional que ya no está: el habitual del paciente
    a.db.collaborators.iter_mut().find(|c| c.id == 2).unwrap().active = false;
    a.db.patient_mut(110).unwrap().doctor_id = 1;
    let d = crate::screens::recalls::appointment_for(&a, &r);
    assert_eq!(d.a.doctor_id, 1);
}

/// Recibir en Almacén › Pedidos unos implantes que llegan en dos lotes.
#[test]
fn receive_implants_in_two_lots_from_the_orders_screen() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.screen = crate::app::Screen::Almacen;
        a.stock.tab = 1;
        a.stock.order = a.db.orders.iter().find(|o| o.state == crate::stock::OrderState::Enviado).map(|o| o.id);
        a
    });
    for _ in 0..5 {
        h.step();
    }
    let oid = h.state().stock.order.unwrap();
    let before = h.state().db.product(9).unwrap().stock;
    // sin lote no entra
    h.get_by_label_contains("Recibir mercancía").click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().db.orders.iter().find(|o| o.id == oid).unwrap().state, crate::stock::OrderState::Enviado);
    // llegan 3 de un lote y 1 de otro
    h.get_by_label("＋ otro lote").click();
    for _ in 0..3 {
        h.step();
    }
    {
        let o = h.state_mut().db.orders.iter_mut().find(|o| o.id == oid).unwrap();
        assert_eq!(o.lines.iter().map(|l| l.qty).collect::<Vec<_>>(), vec![3.0, 1.0]);
        o.lines[0].lot = "A111".into();
        o.lines[0].expiry = "05/2030".into();
        o.lines[1].lot = "B222".into();
        o.lines[1].expiry = "09/2031".into();
    }
    h.step();
    h.get_by_label_contains("Recibir mercancía").click();
    for _ in 0..3 {
        h.step();
    }
    let db = &h.state().db;
    assert_eq!(db.orders.iter().find(|o| o.id == oid).unwrap().state, crate::stock::OrderState::Recibido);
    assert!((db.product(9).unwrap().stock - (before + 4.0)).abs() < 1e-6);
    let lots: Vec<(String, f64)> = db.lots(9).into_iter().filter(|l| l.lot == "A111" || l.lot == "B222").map(|l| (l.lot, l.qty)).collect();
    assert_eq!(lots, vec![("A111".to_string(), 3.0), ("B222".to_string(), 1.0)]);
}

/// Cobrar una cuota en efectivo desde la ficha (entra en la caja) y anularla.
#[test]
fn installment_paid_in_cash_and_cancelled_from_the_dialog() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        let bid = a.db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        a.db.accept_budget(bid, a.today);
        a.patient = Some(101);
        a.screen = crate::app::Screen::Pacientes;
        a.pat_tab = crate::app::PatTab::Cuenta;
        let mut d = crate::screens::cobros::PayDraft::installment(&a, bid, 0).unwrap();
        d.method = "Efectivo".into();
        a.dialog = crate::app::Dialog::Payment(d);
        a
    });
    for _ in 0..4 {
        h.step();
    }
    let cash_before = {
        let a = h.state();
        a.db.cash_expected(a.db.open_cash().unwrap())
    };
    h.get_all_by_label("Cobrar").last().unwrap().click();
    for _ in 0..3 {
        h.step();
    }
    let (pay, amount) = {
        let a = h.state();
        let p = a.db.payments.iter().rev().find(|p| p.installment.is_some()).expect("cuota cobrada").clone();
        assert_eq!(p.method, "Efectivo");
        assert!((a.db.cash_expected(a.db.open_cash().unwrap()) - (cash_before + p.amount)).abs() < 0.005, "en la caja");
        (p.id, p.amount)
    };
    // se anula con su motivo
    {
        let a = h.state_mut();
        let mut d = crate::screens::cobros::PayDraft::cancel(a, pay).unwrap();
        d.concept = "Cobrada por error".into();
        a.dialog = crate::app::Dialog::Payment(d);
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Anular el cobro").click();
    for _ in 0..3 {
        h.step();
    }
    let a = h.state();
    let back = a.db.payments.iter().find(|p| p.cancels == Some(pay)).expect("anulación");
    assert_eq!(back.amount, -amount);
    assert!((a.db.cash_expected(a.db.open_cash().unwrap()) - cash_before).abs() < 0.005, "la caja vuelve a cuadrar");
}

/// Una cita de primera visita: «Dar de alta a este paciente» abre su ficha con
/// los datos de la cita y, al guardarla, la cita queda unida a ella.
#[test]
fn first_visit_appointment_creates_the_patient_file() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Recep")).map(|u| u.id);
        let mut x = a.db.appointments[0].clone();
        (x.id, x.patient_id, x.free_name, x.phone, x.first_visit) = (a.db.new_id(), None, "Nuria Prieto Gil".into(), "655 444 333".into(), true);
        a.db.appointments.push(x.clone());
        a.screen = crate::app::Screen::Agenda;
        a.dialog = crate::app::Dialog::Appointment(crate::screens::agenda::ApptDraft { a: x, is_new: false, search: String::new(), repeat: 0, repeat_count: 4, from_id: None });
        a
    });
    for _ in 0..4 {
        h.step();
    }
    let appt = match &h.state().dialog {
        crate::app::Dialog::Appointment(d) => d.a.id,
        _ => panic!("sin diálogo de cita"),
    };
    h.get_by_label_contains("Dar de alta a este paciente").click();
    for _ in 0..3 {
        h.step();
    }
    match &h.state().dialog {
        crate::app::Dialog::NewPatient(p) => assert_eq!((p.first_name.as_str(), p.last_name.as_str(), p.mobile.as_str()), ("Nuria", "Prieto Gil", "655 444 333")),
        _ => panic!("no se abrió la ficha nueva"),
    }
    h.get_by_label("Guardar paciente").click();
    for _ in 0..3 {
        h.step();
    }
    let a = h.state();
    let pid = a.db.patients.iter().find(|p| p.mobile == "655 444 333").expect("ficha creada").id;
    assert_eq!(a.db.appointments.iter().find(|x| x.id == appt).unwrap().patient_id, Some(pid), "la cita queda unida a la ficha");
    assert!(a.new_patient_for.is_none());
}

/// Con una cuota cobrada, el plan del presupuesto queda bloqueado: quitar
/// «Pagar a plazos» no hace nada y lo explica.
#[test]
fn a_plan_with_a_paid_installment_is_locked_in_the_budget() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        let bid = a.db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        a.db.accept_budget(bid, a.today);
        a.db.pay_installment(bid, 0, "Tarjeta").unwrap();
        a.screen = crate::app::Screen::Presupuestos;
        a.dialog = crate::app::Dialog::Budget(bid);
        a
    });
    for _ in 0..4 {
        h.step();
    }
    let bid = match h.state().dialog {
        crate::app::Dialog::Budget(id) => id,
        _ => panic!("sin presupuesto abierto"),
    };
    let _ = h.get_by_label_contains("El plan ya no se puede cambiar");
    h.get_by_label("Pagar a plazos").click();
    for _ in 0..3 {
        h.step();
    }
    let b = h.state().db.budgets.iter().find(|b| b.id == bid).unwrap().clone();
    assert!(b.plan.is_some_and(|p| p.paid[0]), "el plan sigue, con su cuota cobrada");
}

/// Importar pacientes desde un CSV en Configuración › Importar: una nota de
/// varias líneas no crea pacientes de más y otra persona con un nº de
/// historial ya usado recibe su propia ficha.
#[test]
fn import_patients_from_a_csv_in_the_settings_screen() {
    let dir = std::env::temp_dir().join("veodardental_ui_import");
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("pacientes.csv");
    std::fs::write(&csv, "Nº historial;Apellidos;Nombre;Teléfono;Notas\n1001;Martínez Ruiz;Pedro;612 345 678;\"Alérgico a la penicilina\nTomar antibiótico antes\"\n9001;Puig;Núria;;\n").unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 1400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        crate::screens::config::open(&mut a, crate::screens::config::CfgTab::Importar);
        a.imp.path = csv.to_string_lossy().into_owned();
        a
    });
    for _ in 0..3 {
        h.step();
    }
    let ana = h.state().db.patients.iter().find(|p| p.history_no == 1001).unwrap().clone();
    let n = h.state().db.patients.len();
    h.get_by_label("Leer fichero").click();
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Importar 2 filas").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(h.get_all_by_label_contains("ya era de otra persona").count() > 0, "el resumen lo explica");
    let a = h.state();
    assert_eq!(a.db.patients.len(), n + 2);
    let pedro = a.db.patients.iter().find(|p| p.full_name() == "Pedro Martínez Ruiz").unwrap();
    assert_ne!(pedro.history_no, 1001);
    assert_eq!((pedro.mobile.as_str(), pedro.notes.as_str()), ("612 345 678", "Alérgico a la penicilina\nTomar antibiótico antes"));
    let now = a.db.patients.iter().find(|p| p.id == ana.id).unwrap();
    assert_eq!((now.full_name(), &now.mobile, &now.notes), (ana.full_name(), &ana.mobile, &ana.notes));
}

/// Configuración › Facturación: la exención de IVA de las facturas nuevas se
/// elige con su artículo, queda en el registro de cambios y no toca las ya
/// emitidas.
#[test]
fn the_vat_exemption_for_new_invoices_is_chosen_in_the_settings() {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        crate::screens::config::open(&mut a, crate::screens::config::CfgTab::Facturacion);
        a
    });
    for _ in 0..3 {
        h.step();
    }
    let before: Vec<Option<crate::model::VatExemption>> = h.state().db.invoices.iter().map(|i| i.exemption).collect();
    h.get_by_label_contains("Art. 20.Uno.5º").click();
    for _ in 0..3 {
        h.step();
    }
    let a = h.state();
    assert_eq!(a.db.config.vat_exemption, crate::model::VatExemption::Art20Uno5);
    assert!(a.db.audit_log.iter().any(|x| x.action == "Cambiar la exención de IVA" && x.detail.contains("20.Uno.5º")));
    assert_eq!(a.db.invoices.iter().map(|i| i.exemption).collect::<Vec<_>>(), before, "las emitidas no cambian");
}

/// Ficha › Documentos: revocar un consentimiento firmado desde la pantalla. El
/// documento y su firma se quedan y la revocación queda anotada.
#[test]
fn a_signed_consent_is_revoked_from_the_patient_file() {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 1400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Recep")).map(|u| u.id);
        let tid = a.db.doc_templates.iter().find(|t| t.name.contains("extracción")).map(|t| t.id).unwrap();
        let id = a.db.new_document(101, tid, &[38]).unwrap();
        a.db.sign_document(id, "Carlos Martínez Sanz", vec![(0..24).map(|i| [i as f32 / 24.0, 0.5]).collect()]).unwrap();
        a.open_patient(101, crate::app::PatTab::Documentos);
        a.docs_mode = 0;
        a.docs.selected = Some(id);
        a
    });
    for _ in 0..4 {
        h.step();
    }
    let id = h.state().docs.selected.unwrap();
    h.get_by_label_contains("sin cambios desde que se firmó");
    h.get_by_label("Revocar…").click();
    for _ in 0..3 {
        h.step();
    }
    h.state_mut().docs.revoke.as_mut().unwrap().2 = "Prefiere no operarse".into();
    h.step();
    h.get_by_label("Anotar revocación").click();
    for _ in 0..3 {
        h.step();
    }
    let d = h.state().db.documents.iter().find(|d| d.id == id).unwrap().clone();
    assert_eq!((d.revoked_by.as_str(), d.revoke_reason.as_str()), ("Carlos Martínez Sanz", "Prefiere no operarse"));
    assert!(d.signed && !d.signature.is_empty());
    h.get_by_label_contains("Revocado el");
}

/// Ficha › Salud: con el cuestionario firmado, las respuestas no se pueden
/// cambiar (para eso está «Nueva revisión»); las observaciones sí.
#[test]
fn a_signed_health_form_cannot_be_edited_in_the_file() {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 1600.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        let today = a.today;
        let id = a.db.new_health(101, today, "El propio paciente");
        a.db.health.iter_mut().find(|f| f.id == id).unwrap().yes.insert("diabetes".into(), false);
        a.db.sign_health(id).unwrap();
        a.open_patient(101, crate::app::PatTab::Salud);
        a
    });
    for _ in 0..4 {
        h.step();
    }
    let form = h.state().db.latest_health(101).unwrap().clone();
    assert!(form.signed);
    h.get_by_label("¿Es diabético/a?").click();
    for _ in 0..3 {
        h.step();
    }
    let after = h.state().db.latest_health(101).unwrap().clone();
    assert_eq!(after.yes, form.yes, "las respuestas firmadas no cambian");
    assert_eq!(h.state().db.health_integrity(form.id), crate::firmado::Integrity::Intact);
    h.get_by_label_contains("sin cambios desde que se firmó");
}

/// Recetas: la higienista crea la receta y queda como prescriptor quien puede
/// recetar; con avisos de alergia no se imprime hasta que los revisa un
/// profesional que receta, y queda quién los revisó.
#[test]
fn a_prescription_needs_a_prescriber_and_reviewed_warnings() {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 1600.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Hig")).map(|u| u.id);
        a.db.config.current_user = "Hig. Campos".into();
        a.db.patient_mut(101).unwrap().allergies = "Penicilina".into();
        a.open_patient(101, crate::app::PatTab::Documentos);
        a.docs_mode = 1;
        a
    });
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("＋ Nueva receta").click();
    for _ in 0..3 {
        h.step();
    }
    let id = h.state().rx.selected.unwrap();
    let doctor = h.state().db.prescriptions.iter().find(|r| r.id == id).unwrap().doctor_id;
    assert_eq!(h.state().db.prescriber_problem(doctor), None, "la higienista no queda como prescriptor");
    {
        let a = h.state_mut();
        let amox = a.db.drugs.iter().find(|d| d.name.starts_with("Amoxicilina 500")).cloned().unwrap();
        a.db.prescriptions.iter_mut().find(|r| r.id == id).unwrap().items.push(crate::finance::rx_item_from_drug(&amox));
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("🖨 Imprimir / exportar").click();
    for _ in 0..2 {
        h.step();
    }
    assert!(h.state().toast.as_ref().is_some_and(|t| t.0.contains("Revise antes los avisos")), "no se imprime sin revisar");
    // la higienista no los puede dar por revisados: los revisa quien receta
    assert!(h.query_by_label("He revisado los avisos").is_none());
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name == "Dra. Ruiz").map(|u| u.id);
        a.db.config.current_user = "Dra. Ruiz".into();
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("He revisado los avisos").click();
    for _ in 0..3 {
        h.step();
    }
    let r = h.state().db.prescriptions.iter().find(|r| r.id == id).unwrap().clone();
    assert_eq!(r.reviewed_by, "Dra. Ruiz");
    assert!(h.state().db.rx_unreviewed(id).is_empty());
    h.get_by_label_contains("Avisos revisados por Dra. Ruiz");
}

/// Odontograma: marcar como hecho lo que estaba previsto en esa pieza completa
/// el previsto (no crea otro), y «↶» lo devuelve a previsto con su material.
#[test]
fn the_chart_completes_planned_treatments_and_can_undo_them() {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 1400.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dra")).map(|u| u.id);
        a.open_patient(110, crate::app::PatTab::Odontograma);
        a
    });
    for _ in 0..3 {
        h.step();
    }
    let planned = {
        let a = h.state_mut();
        let today = a.today;
        let id = a.db.add_entry(110, "0302", vec![16], vec![], crate::model::TxState::Previsto, 1, today);
        a.db.entries.iter_mut().find(|e| e.id == id).unwrap().budget_id = Some(4242);
        a.odg.tool = crate::model::OdgKind::Obturacion;
        a.odg.code = "0302".into();
        a.odg.state = crate::model::TxState::Realizado;
        a.odg.faces = [true, false, true, false, false];
        id
    };
    let n = h.state().db.entries.len();
    {
        let a = h.state_mut();
        let today = a.today;
        crate::screens::odontograma::apply_tool(a, 110, 16, None, today);
        a.odg.selected_tooth = Some(16);
    }
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().db.entries.len(), n, "no crea otro tratamiento");
    assert_eq!(h.state().db.entries.iter().find(|e| e.id == planned).unwrap().state, crate::model::TxState::Realizado);
    h.get_by_label("↶").click();
    for _ in 0..3 {
        h.step();
    }
    let e = h.state().db.entries.iter().find(|e| e.id == planned).unwrap().clone();
    assert_eq!((e.state, e.budget_id), (crate::model::TxState::Previsto, Some(4242)));
}

/// Bancos: un movimiento que casa con dos cobros no se concilia solo; se
/// ofrecen los dos, se elige uno y se puede deshacer (todo con el servidor
/// o el fichero bloqueado, como en la clínica).
#[test]
fn a_doubtful_bank_move_is_chosen_by_hand_and_can_be_undone() {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    let (id, p1) = {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.screen = crate::app::Screen::Bancos;
        let p1 = a.db.add_payment(102, 57.0, "Transferencia", "Pago");
        a.db.add_payment(103, 57.0, "Transferencia", "Pago");
        let today = a.today;
        a.db.add_bank_moves(1, &[crate::bank::RawMove { date: today, amount: 57.0, concept: "TRANSFERENCIA RECIBIDA".into(), reference: String::new() }]);
        let id = a.db.bank_moves.iter().find(|m| m.concept == "TRANSFERENCIA RECIBIDA").unwrap().id;
        a.bank.selected = Some(id);
        (id, p1)
    };
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("⟳ Conciliar automáticamente").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(h.state().db.bank_moves.iter().find(|m| m.id == id).unwrap().matched.is_none(), "dos cobros posibles: no se concilia solo");
    assert_eq!(h.state().db.doubtful_moves(), vec![id]);
    h.get_by_label("Elija con qué se concilia");
    let receipt = h.state().db.payments.iter().find(|p| p.id == p1).unwrap().receipt_no.clone();
    h.get_by_label_contains(&receipt).click();
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().db.bank_moves.iter().find(|m| m.id == id).unwrap().matched, Some(crate::bank::Match::Payments(vec![p1])));
    h.get_by_label("Deshacer conciliación").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(h.state().db.bank_moves.iter().find(|m| m.id == id).unwrap().matched.is_none());
}

/// Informes: el resumen económico dice qué entra en cada total y responde a
/// los filtros de centro y profesional; los cobros por forma de pago, con
/// las remesas, suman el total.
#[test]
fn the_economic_summary_shows_its_totals_and_filters() {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
        let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
        a.screenshot = None;
        a
    });
    {
        let a = h.state_mut();
        a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        a.screen = crate::app::Screen::Informes;
        a.lists.report_tab = 7;
        a.lists.report_from = a.today - chrono::Duration::days(365);
    }
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Lo que paga la mutua (baremo)");
    h.get_by_label("Devoluciones y recibos devueltos");
    h.get_by_label("Profesional");
    let (net, f) = {
        let a = h.state();
        let f = crate::informes::Filter { from: a.lists.report_from, to: a.lists.report_to, center: None, doctor: None };
        (a.db.report_collected(&f).net, f)
    };
    assert!(!h.query_all_by_label(&net.to_string()).collect::<Vec<_>>().is_empty(), "el neto cobrado: {net}");
    // con un centro elegido, sus cobros
    h.state_mut().lists.report_center = Some(2);
    for _ in 0..3 {
        h.step();
    }
    let c2 = h.state().db.report_collected(&crate::informes::Filter { center: Some(2), ..f }).net;
    assert_ne!(c2, net, "los datos de ejemplo tienen cobros en los dos centros");
    assert!(!h.query_all_by_label(&c2.to_string()).collect::<Vec<_>>().is_empty(), "el neto del centro 2: {c2}");
}

/// Lo que se sale a la derecha de la ventana (o empieza fuera por la
/// izquierda) en la pantalla que se ve: (nombre del elemento, su borde derecho).
fn offscreen(h: &Harness<'static, App>, width: f32) -> Vec<(String, f32)> {
    use egui_kittest::kittest::NodeT;
    let mut out: Vec<(String, f32)> = h
        .query_all_by(|_| true)
        .filter_map(|n| {
            // el texto de una casilla de escribir más largo que ella no se sale: la
            // casilla lo recorta y se desplaza al escribir
            let an = n.accesskit_node();
            let in_input = an.parent().is_some_and(|p| matches!(p.role(), egui::accesskit::Role::TextInput | egui::accesskit::Role::MultilineTextInput));
            if in_input {
                return None;
            }
            // (la ventana de prueba va a 1 píxel por punto)
            let r = an.bounding_box()?;
            let label = an.label().or_else(|| an.value()).unwrap_or_else(|| {
                let kids: Vec<String> = an.children().take(4).map(|c| c.label().or_else(|| c.value()).unwrap_or_else(|| format!("{:?}", c.role()))).collect();
                format!("{:?} x0={:.0} ancho={:.0}: {}", an.role(), r.x0, r.width(), kids.join(" | "))
            });
            ((r.x1 as f32 > width + 1.0 || r.x0 < -1.0) && r.width() > 0.0 && r.height() > 0.0).then(|| (format!("{label}» (desde x={:.0}, y={:.0}", r.x0, r.y0), r.x1 as f32))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// Todas las pantallas y todas las pestañas de la ficha a 1280×720 y
/// 1280×800: nada se sale por los lados (lo que no cabe de alto se desplaza).
#[test]
fn every_screen_fits_1280_wide() {
    let mut problems: Vec<String> = vec![];
    for (w, hgt) in [(1280.0, 720.0), (1280.0, 800.0)] {
        let mut h = Harness::builder().with_size(egui::vec2(w, hgt)).with_accessibility_check(false).allow_missing_glyphs().build_eframe(|cc| {
            let mut a = App::new(cc, Some(std::env::temp_dir().join("veodardental_test.png")), None, None, None);
            a.screenshot = None;
            a
        });
        {
            let a = h.state_mut();
            a.user_id = a.db.users.iter().find(|u| u.name.starts_with("Dir")).map(|u| u.id);
        }
        for (screen, _, name) in crate::app::Screen::NAV {
            h.state_mut().screen = screen;
            for _ in 0..3 {
                h.step();
            }
            for (label, x) in offscreen(&h, w) {
                problems.push(format!("{w}×{hgt} {name}: «{label}» hasta x={x:.0}"));
            }
        }
        for (tab, name) in crate::app::PatTab::ALL {
            h.state_mut().open_patient(101, tab);
            for _ in 0..3 {
                h.step();
            }
            for (label, x) in offscreen(&h, w) {
                problems.push(format!("{w}×{hgt} ficha › {name}: «{label}» hasta x={x:.0}"));
            }
        }
        // las pestañas de Informes y los apartados de Configuración
        for tab in 0..8 {
            {
                let a = h.state_mut();
                a.screen = crate::app::Screen::Informes;
                a.lists.report_tab = tab;
            }
            for _ in 0..3 {
                h.step();
            }
            for (label, x) in offscreen(&h, w) {
                problems.push(format!("{w}×{hgt} Informes › pestaña {tab}: «{label}» hasta x={x:.0}"));
            }
        }
        use crate::screens::config::CfgTab::*;
        for tab in [Clinica, Marcha, Gabinetes, Centros, Usuarios, Facturacion, Correo, CitaOnline, Telefonia, Whatsapp, Receta, Ia, Importar, Copias, Fichas, Duplicados, Registro, Red] {
            {
                let a = h.state_mut();
                a.screen = crate::app::Screen::Config;
                a.lists.cfg_tab = tab;
            }
            for _ in 0..3 {
                h.step();
            }
            for (label, x) in offscreen(&h, w) {
                problems.push(format!("{w}×{hgt} Configuración › {}: «{label}» hasta x={x:.0}", tab.label()));
            }
        }
    }
    assert!(problems.is_empty(), "{} elementos fuera:\n{}", problems.len(), problems.join("\n"));
}
