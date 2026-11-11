//! Recetas del paciente con vademécum básico y avisos de alergias,
//! anticoagulación y otras alertas médicas.

use crate::app::App;
use crate::finance::*;
use crate::model::*;
use crate::theme::*;
use egui::{Color32, CornerRadius, RichText, Ui};

#[derive(Default)]
pub struct RxState {
    pub selected: Option<u32>,
    pub drug: String,
    pub cve: String,
    /// Último mensaje del repositorio de recetas (conexión directa).
    pub status: String,
    /// Certificado del prescriptor en este ordenador y su contraseña (solo en memoria).
    pub cert_doctor: Option<u32>,
    pub cert_path: String,
    pub password: String,
    pub pin_on: bool,
    pub pin: String,
    /// Lista oficial de medicamentos (AEMPS), cargada la primera vez que hace falta.
    pub nomen: Option<crate::nomenclator::Nomenclator>,
    pub nomen_tried: bool,
    pub nomen_job: Option<std::sync::mpsc::Receiver<Result<crate::nomenclator::Nomenclator, String>>>,
    pub nomen_msg: String,
    pub nomen_zip: String,
    pub med_query: String,
    /// Última prueba del circuito con la farmacia simulada (cuándo y resultado).
    pub circuit: Option<(chrono::NaiveDateTime, Vec<crate::srep_cert::Step>)>,
    pub show_controls: bool,
    /// Dentistas que recetan: (nombre, nº de colegiado, certificado elegido), y cuándo se miró.
    pub prescribers: Option<(f64, Vec<(String, bool, bool)>)>,
    /// Envío de la hoja del paciente por correo: (receta, destinatario, resultado).
    pub mail_job: Option<(u32, String, std::sync::mpsc::Receiver<Result<(), String>>)>,
}

/// Recoge el resultado del envío de la hoja por correo (se llama en cada fotograma).
pub fn poll(app: &mut App, ctx: &egui::Context) {
    let Some((id, to, rx)) = app.rx.mail_job.as_ref() else { return };
    match rx.try_recv() {
        Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            app.rx.mail_job = None;
            app.warn("El envío de la hoja se interrumpió.");
        }
        Ok(r) => {
            let (id, to) = (*id, to.clone());
            app.rx.mail_job = None;
            match r {
                Ok(()) => {
                    app.db.audit("Hoja de la receta enviada por correo", &format!("receta {id} · {to}"));
                    app.changed();
                    app.notify(format!("Hoja de la receta enviada a {to}."));
                }
                Err(e) => app.warn(format!("No se pudo enviar la hoja: {e}")),
            }
        }
    }
}

fn ensure_nomen(app: &mut App) {
    if !app.rx.nomen_tried {
        app.rx.nomen_tried = true;
        app.rx.nomen = crate::nomenclator::load();
    }
    if let Some(rx) = &app.rx.nomen_job {
        if let Ok(res) = rx.try_recv() {
            app.rx.nomen_job = None;
            match res {
                Ok(n) => {
                    app.rx.nomen_msg = format!("Lista oficial cargada: {} medicamentos comercializados (lista del {}).", n.medicines.len(), n.date);
                    app.rx.nomen = Some(n);
                }
                Err(e) => app.rx.nomen_msg = e,
            }
        }
    }
}

/// Descarga (o lee de un fichero) y guarda la lista oficial, en segundo plano.
fn start_nomen_job(app: &mut App, ctx: &egui::Context, from_file: Option<String>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let bytes = match from_file {
            Some(p) => std::fs::read(p.trim().trim_matches('"')).map_err(|e| format!("No se puede leer el fichero: {e}")),
            None => crate::nomenclator::download(),
        };
        let res = bytes.and_then(|b| crate::nomenclator::read_zip(&b)).and_then(|n| crate::nomenclator::save(&n).map(|_| n));
        let _ = tx.send(res);
        ctx.request_repaint();
    });
    app.rx.nomen_job = Some(rx);
    app.rx.nomen_msg = "Preparando la lista oficial de medicamentos…".into();
}

/// Configuración › Receta: lista oficial de medicamentos de la AEMPS.
fn nomenclator_panel(app: &mut App, ui: &mut Ui) {
    ensure_nomen(app);
    let ctx = ui.ctx().clone();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Lista oficial de medicamentos (Nomenclátor de prescripción de la AEMPS)");
        muted(ui, "Con ella, al recetar se busca el medicamento por nombre o principio activo y se rellenan solos el código nacional, el principio activo, la forma, la dosis, la vía, el envase y si es estupefaciente o psicótropo. La AEMPS la actualiza cada día; conviene descargarla de vez en cuando.");
        ui.add_space(4.0);
        let busy = app.rx.nomen_job.is_some();
        ui.horizontal_wrapped(|ui| {
            match &app.rx.nomen {
                Some(n) => pill(ui, &format!("{} medicamentos · lista del {}", n.medicines.len(), n.date), OK),
                None => pill(ui, "Sin descargar", WARN),
            };
            if ui.add_enabled(!busy, egui::Button::new(RichText::new("⬇ Descargar la lista oficial").color(Color32::WHITE)).fill(ACCENT)).clicked() {
                start_nomen_job(app, &ctx, None);
            }
            ui.add_sized([300.0, 30.0], egui::TextEdit::singleline(&mut app.rx.nomen_zip).hint_text("o la ruta de un prescripcion.zip ya descargado"));
            if ui.add_enabled(!busy && !app.rx.nomen_zip.trim().is_empty(), egui::Button::new("Cargar fichero")).clicked() {
                let p = app.rx.nomen_zip.clone();
                start_nomen_job(app, &ctx, Some(p));
            }
            if busy {
                ui.spinner();
            }
        });
        if !app.rx.nomen_msg.is_empty() {
            ui.label(RichText::new(&app.rx.nomen_msg).color(if app.rx.nomen_msg.starts_with("Lista oficial cargada") || busy { INFO } else { DANGER }));
        }
    });
}

fn estado_color(e: u8) -> Color32 {
    match e {
        3 | 4 => OK,
        2 => DANGER,
        8 | 10 => WARN,
        5 | 11 => MUTED,
        _ => INFO,
    }
}

pub fn patient_view(app: &mut App, ui: &mut Ui, pid: u32) {
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    let list_w = 330.0;
    let mut list: Vec<Prescription> = app.db.prescriptions.iter().filter(|r| r.patient_id == pid).cloned().collect();
    list.sort_by_key(|r| std::cmp::Reverse((r.date, r.id)));
    if app.rx.selected.map(|s| !list.iter().any(|r| r.id == s)).unwrap_or(true) {
        app.rx.selected = list.first().map(|r| r.id);
    }
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(list_w);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    section(ui, "Recetas");
                    right_aligned(ui, |ui| {
                        if primary(ui, "＋ Nueva receta").clicked() {
                            let doc = app.user_id.and_then(|u| app.db.user(u)).and_then(|u| u.doctor_id).or_else(|| app.db.patient(pid).map(|p| p.doctor_id)).unwrap_or(1);
                            let id = app.db.new_prescription(pid, doc);
                            app.rx.selected = Some(id);
                            app.changed();
                        }
                    });
                });
                if list.is_empty() {
                    empty_state(ui, "Sin recetas.");
                }
                for r in &list {
                    let names: Vec<String> = r.items.iter().map(|i| i.drug.split_whitespace().next().unwrap_or("").to_string()).collect();
                    if ui.selectable_label(app.rx.selected == Some(r.id), format!("{} · {}", fmt_date(r.date), if names.is_empty() { "(vacía)".into() } else { names.join(", ") })).clicked() {
                        app.rx.selected = Some(r.id);
                    }
                }
            });
        });
        ui.vertical(|ui| {
            ui.set_width(avail - list_w - 30.0);
            if let Some(id) = app.rx.selected {
                editor(app, ui, id);
            }
        });
    });
}

fn editor(app: &mut App, ui: &mut Ui, id: u32) {
    ensure_nomen(app);
    let Some(r) = app.db.prescriptions.iter().find(|r| r.id == id).cloned() else { return };
    let p = app.db.patient(r.patient_id).cloned().unwrap_or_default();
    let warns = rx_warnings(&p, &r.items, &app.db.drugs);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, &format!("Receta de {}", p.full_name()));
            right_aligned(ui, |ui| {
                muted(ui, format!("{} · {}", fmt_date(r.date), app.db.doctor(r.doctor_id).map(|d| d.name.clone()).unwrap_or_default()));
            });
        });
        if !p.allergies.is_empty() || !p.alerts.is_empty() {
            ui.horizontal(|ui| {
                if !p.allergies.is_empty() {
                    pill(ui, &format!("Alergias: {}", p.allergies), DANGER);
                }
                if !p.alerts.is_empty() {
                    pill(ui, &format!("⚠ {}", p.alerts), WARN);
                }
            });
        }
        for w in &warns {
            egui::Frame::new().fill(tint(DANGER, 0.1)).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("⚠ {w}")).color(DANGER).family(sb()));
            });
        }
        let drugs = app.db.drugs.clone();
        let direct = app.db.erx.direct && r.srep.is_none();
        let mut remove = None;
        if let Some(rm) = app.db.prescriptions.iter_mut().find(|x| x.id == id) {
            let before = serde_json::to_string(rm).unwrap_or_default();
            ui.horizontal(|ui| {
                field(ui, "Diagnóstico / motivo", &mut rm.diagnosis, 420.0);
            });
            ui.add_space(4.0);
            for (i, it) in rm.items.iter_mut().enumerate() {
                egui::Frame::new().fill(ROW_ALT).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("℞ {}", it.drug)).family(sb()).size(15.5));
                        right_aligned(ui, |ui| {
                            if ui.small_button("✖").clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Dosis");
                        ui.add_sized([130.0, 32.0], egui::TextEdit::singleline(&mut it.dose));
                        ui.label("Pauta");
                        ui.add_sized([200.0, 32.0], egui::TextEdit::singleline(&mut it.frequency));
                        ui.label("Días");
                        ui.add(egui::DragValue::new(&mut it.days).range(1..=90));
                        ui.label("Envases");
                        ui.add(egui::DragValue::new(&mut it.units).range(1..=10));
                    });
                    ui.add_sized([ui.available_width(), 32.0], egui::TextEdit::singleline(&mut it.notes).hint_text("Indicaciones para el paciente"));
                    if direct {
                        official_item_fields(ui, i, it);
                    }
                });
                ui.add_space(4.0);
            }
            if let Some(i) = remove {
                rm.items.remove(i);
            }
            // buscador en la lista oficial de la AEMPS
            let hint = if app.rx.nomen.is_some() { "🔍 Buscar en la lista oficial: nombre, principio activo o código nacional" } else { "Lista oficial de medicamentos sin descargar (Configuración › Receta electrónica)" };
            ui.add_sized([ui.available_width().min(620.0), 34.0], egui::TextEdit::singleline(&mut app.rx.med_query).hint_text(hint));
            let found: Vec<crate::nomenclator::Medicine> = app.rx.nomen.as_ref().map(|n| n.search(&app.rx.med_query, 12).into_iter().cloned().collect()).unwrap_or_default();
            if !app.rx.med_query.trim().is_empty() && app.rx.nomen.is_some() && found.is_empty() {
                muted(ui, "Ningún medicamento comercializado coincide.");
            }
            for m in &found {
                ui.horizontal(|ui| {
                    if ui.selectable_label(false, RichText::new(format!("＋ {} · CN {}", m.name, m.cn))).on_hover_text(format!("{}\n{} · {}\nPrincipio activo: {}", m.dosage, m.form, m.route, m.dcpf_name)).clicked() {
                        rm.items.push(crate::nomenclator::rx_item(m));
                        app.rx.med_query.clear();
                    }
                    if m.narcotic {
                        pill(ui, "Estupefaciente", DANGER);
                    }
                    if m.psychotropic {
                        pill(ui, "Psicótropo", WARN);
                    }
                });
            }
            ui.horizontal(|ui| {
                let cur = if app.rx.drug.is_empty() { "Elegir medicamento…".to_string() } else { app.rx.drug.clone() };
                crate::theme::combo("rx_drug").width(340.0).selected_text(cur).show_ui(ui, |ui| {
                    let mut last = String::new();
                    let mut sorted = drugs.clone();
                    sorted.sort_by(|a, b| a.group.cmp(&b.group));
                    for d in &sorted {
                        if d.group != last {
                            ui.label(RichText::new(&d.group).size(12.0).color(MUTED));
                            last = d.group.clone();
                        }
                        ui.selectable_value(&mut app.rx.drug, d.name.clone(), &d.name);
                    }
                });
                if primary(ui, "Añadir").clicked() {
                    if let Some(d) = drugs.iter().find(|d| d.name == app.rx.drug) {
                        rm.items.push(rx_item_from_drug(d));
                    }
                }
            });
            ui.label(RichText::new("Observaciones").size(13.5).color(MUTED));
            ui.add(egui::TextEdit::multiline(&mut rm.notes).desired_rows(2).desired_width(f32::INFINITY));
            if serde_json::to_string(rm).unwrap_or_default() != before {
                app.dirty = true;
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("🖨 Imprimir / exportar").clicked() {
                let r2 = app.db.prescriptions.iter().find(|x| x.id == id).cloned().unwrap();
                match export_html(app, &r2) {
                    Ok(path) => {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(format!("file://{}", path.display())));
                        app.notify(format!("Receta exportada en {}", path.display()));
                    }
                    Err(e) => app.warn(e),
                }
            }
            if ui.button("Borrar receta").clicked() {
                app.db.prescriptions.retain(|x| x.id != id);
                app.rx.selected = None;
                app.changed();
            }
        });
    });
    ui.add_space(8.0);
    official_card(app, ui, id);
    let _ = Color32::WHITE;
}

/// Datos de la receta oficial de un medicamento (conexión directa).
fn official_item_fields(ui: &mut Ui, i: usize, it: &mut RxItem) {
    ui.add_space(4.0);
    ui.label(RichText::new("Datos para la receta oficial").size(12.5).color(MUTED));
    ui.horizontal_wrapped(|ui| {
        crate::theme::combo(("rx_tipo", i)).width(170.0).selected_text(["Medicamento", "Producto sanitario", "Nutrición", "Vacuna individualizada", "Fórmula magistral"][it.product_type.min(4) as usize]).show_ui(ui, |ui| {
            for (v, l) in [(0u8, "Medicamento"), (1, "Producto sanitario"), (4, "Fórmula magistral")] {
                ui.selectable_value(&mut it.product_type, v, l);
            }
        });
        if it.product_type == 4 {
            ui.add_sized([340.0, 30.0], egui::TextEdit::singleline(&mut it.composition).hint_text("Composición de la fórmula"));
        } else {
            ui.add_sized([120.0, 30.0], egui::TextEdit::singleline(&mut it.cn).hint_text("Código nacional"));
            ui.label(RichText::new("o").color(MUTED));
            ui.add_sized([150.0, 30.0], egui::TextEdit::singleline(&mut it.active_code).hint_text("Cód. principio activo"));
            ui.add_sized([100.0, 30.0], egui::TextEdit::singleline(&mut it.dosage).hint_text("Dosis (600 mg)"));
        }
        ui.add_sized([120.0, 30.0], egui::TextEdit::singleline(&mut it.form).hint_text("Forma"));
        ui.add_sized([90.0, 30.0], egui::TextEdit::singleline(&mut it.route).hint_text("Vía"));
        ui.add_sized([140.0, 30.0], egui::TextEdit::singleline(&mut it.pack).hint_text("Envase (40 comprimidos)"));
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Tomar");
        ui.add(egui::DragValue::new(&mut it.take).range(0.0..=100.0).speed(0.25));
        ui.add_sized([110.0, 30.0], egui::TextEdit::singleline(&mut it.take_unit).hint_text("comprimido"));
        ui.add(egui::DragValue::new(&mut it.per).range(0.0..=24.0).speed(0.25));
        ui.label("veces al");
        ui.add_sized([70.0, 30.0], egui::TextEdit::singleline(&mut it.per_unit).hint_text("día"));
        ui.checkbox(&mut it.narcotic, "Estupefaciente");
        ui.checkbox(&mut it.psychotropic, "Psicótropo");
    });
}

/// Emisión por conexión directa: firmar con el certificado del dentista,
/// enviar al repositorio, hoja para el paciente y seguimiento de las farmacias.
fn direct_card(app: &mut App, ui: &mut Ui, id: u32) {
    let Some(r) = app.db.prescriptions.iter().find(|r| r.id == id).cloned() else { return };
    let certified = app.db.erx.certified();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let missing = crate::erx::direct_missing(&app.db, &r);
        ui.horizontal(|ui| {
            section(ui, "Receta oficial electrónica · conexión directa");
            right_aligned(ui, |ui| match &r.srep {
                Some(s) if s.test => pill(ui, "Emitida en PRUEBAS", WARN),
                Some(_) => pill(ui, "Emitida y firmada", OK),
                None if missing.is_empty() => pill(ui, "Lista para firmar", INFO),
                None => pill(ui, "Faltan datos", WARN),
            });
        });
        if let Some(s) = r.srep.clone() {
            issued_view(app, ui, &r, &s);
            return;
        }
        if !missing.is_empty() {
            ui.label(RichText::new(format!("Para emitirla falta: {}.", missing.join("; "))).color(DANGER));
        }
        if app.rx.cert_doctor != Some(r.doctor_id) {
            app.rx.cert_doctor = Some(r.doctor_id);
            app.rx.cert_path = crate::erx::cert_path(r.doctor_id);
            app.rx.password.clear();
        }
        ui.horizontal_wrapped(|ui| {
            field(ui, &format!("Certificado de firma de {} en este ordenador (.p12 / .pfx)", app.db.doctor_alias(r.doctor_id)), &mut app.rx.cert_path, 420.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Contraseña del certificado").size(13.5).color(MUTED));
                ui.add_sized([180.0, 32.0], egui::TextEdit::singleline(&mut app.rx.password).password(true));
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut app.rx.pin_on, "Tratamiento confidencial: el paciente elige un PIN de 4 cifras que dirá en la farmacia");
            if app.rx.pin_on {
                ui.add_sized([70.0, 30.0], egui::TextEdit::singleline(&mut app.rx.pin).password(true).hint_text("PIN"));
            }
        });
        if !certified {
            muted(ui, "El sistema aún no está certificado por el CGCOM: la receta se firma y se guarda en el repositorio en modo de pruebas y la hoja sale marcada «PRUEBAS». Sirve para comprobar todo el circuito, no para la farmacia.");
        }
        ui.add_space(4.0);
        let ready = missing.is_empty() && !app.rx.cert_path.trim().is_empty() && !app.rx.password.is_empty();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(ready, egui::Button::new(RichText::new("✍ Firmar y emitir").color(Color32::WHITE)).fill(ACCENT)).clicked() {
                match issue(app, id) {
                    Ok((m, hip)) => {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(format!("file://{}", hip.display())));
                        app.rx.status = m.clone();
                        app.notify(m);
                    }
                    Err(e) => app.rx.status = e,
                }
            }
            if !app.rx.status.is_empty() {
                ui.label(RichText::new(&app.rx.status).color(if app.rx.status.starts_with("Receta firmada") { OK } else { DANGER }));
            }
        });
    });
}

/// Firma con el certificado del prescriptor y envía al repositorio.
fn issue(app: &mut App, id: u32) -> Result<(String, std::path::PathBuf), String> {
    crate::erx::second_factor_ok(&app.db.erx, app.second_factor)?;
    let r = app.db.prescriptions.iter().find(|r| r.id == id).cloned().ok_or("Receta no encontrada")?;
    let pin = if app.rx.pin_on {
        if !crate::srep::pin_valid(app.rx.pin.trim()) {
            return Err("El PIN de confidencialidad debe tener 4 cifras.".into());
        }
        Some(app.rx.pin.trim().to_string())
    } else {
        None
    };
    let path = app.rx.cert_path.trim().trim_matches('"').to_string();
    let bytes = std::fs::read(&path).map_err(|e| format!("No se puede leer el certificado {path}: {e}"))?;
    let signer = crate::firma::load_p12(&bytes, &app.rx.password)?;
    if !signer.info.valid_at(crate::model::now()) {
        return Err(format!("El certificado de {} está caducado o aún no es válido.", signer.info.cn));
    }
    // comprobación de la habilitación del colegiado (obligatoria con el sistema certificado)
    if app.db.erx.certified() {
        let lic = app.db.doctor(r.doctor_id).map(|d| d.license.clone()).unwrap_or_default();
        let c = crate::colegiado::Consejo::Dentistas;
        match crate::colegiado::consultar(c.default_url(), c, &lic)? {
            (true, _) => {}
            (false, m) => return Err(format!("El Consejo indica que el colegiado {lic} no puede recetar{}.", if m.is_empty() { String::new() } else { format!(": {m}") })),
        }
    }
    let _ = crate::erx::set_cert_path(r.doctor_id, &path);
    let acc = app.db.erx.access_ids.entry(r.patient_id).or_insert_with(crate::srep::new_id32).clone();
    let mut list = crate::erx::build_srep(&app.db, &r, &acc, pin.as_deref(), app.today);
    crate::erx::sign_all(&mut list, &signer, chrono::Utc::now())?;
    let client = crate::srep::RepoClient::new(&app.db.erx.repo_addr, &app.db.erx.repo_key);
    let n = client.emitir(&list)?;
    let status = list.iter().flat_map(|p| p.recetas.iter().map(|x| (x.id_receta.clone(), x.estado_en(app.today)))).collect();
    let issue = crate::finance::SrepIssue { id_acceso: acc, issued: list, issued_at: Some(crate::model::now()), signer: signer.info.cn.clone(), test: !app.db.erx.certified(), confidential: pin.is_some(), status, checked_at: Some(crate::model::now()), events: vec![] };
    if let Some(x) = app.db.prescriptions.iter_mut().find(|x| x.id == id) {
        x.srep = Some(issue.clone());
        x.erx_date = Some(app.today);
    }
    app.db.audit("Receta electrónica firmada y emitida (conexión directa)", &format!("receta {id} · {n} receta(s) · firmante {}", signer.info.cn));
    app.rx.password.clear();
    app.rx.pin.clear();
    app.rx.pin_on = false;
    app.changed();
    let hip = export_hip(app, &r, &issue)?;
    Ok((format!("Receta firmada y enviada al repositorio: {n} receta(s). Hoja del paciente en {}", hip.display()), hip))
}

/// Hoja de información al paciente de lo emitido (HTML).
fn hip_for(app: &App, s: &crate::finance::SrepIssue) -> String {
    let e = &app.db.erx;
    let (rep, ent) = (e.id_repositorio_or_test(), e.id_entidad_or_test());
    let sistema = if e.id_sistema.trim().is_empty() { "PRUEBAS".to_string() } else { e.id_sistema.trim().to_string() };
    let h = crate::srep_doc::HipHeader { clinic: &app.db.config.name, clinic_address: &app.db.config.address, clinic_phone: &app.db.config.phone, id_repositorio: &rep, id_entidad: &ent, id_sistema: &sistema, test_mode: s.test, confidential: s.confidential, signer: &s.signer };
    crate::srep_doc::hip_html(&h, &s.issued)
}

/// Carpeta de documentos del programa (en las pruebas, la temporal).
fn docs_dir(sub: &str) -> Result<std::path::PathBuf, String> {
    let docs = if cfg!(test) { Some(std::env::temp_dir()) } else { dirs::document_dir().or_else(dirs::home_dir) };
    let dir = docs.unwrap_or_else(|| std::path::PathBuf::from(".")).join(crate::brand::DIR).join(sub);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn export_hip(app: &App, r: &Prescription, s: &crate::finance::SrepIssue) -> Result<std::path::PathBuf, String> {
    let hist = app.db.patient(r.patient_id).map(|p| p.history_no).unwrap_or(0);
    let path = docs_dir("recetas")?.join(format!("hoja_paciente_{hist}_{}.html", r.id));
    std::fs::write(&path, hip_for(app, s)).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Enlace «Mis recetas» del paciente (se crea la primera vez).
fn rx_url(app: &mut App, patient_id: u32) -> String {
    let had = app.db.rx_link(patient_id).is_some();
    let t = app.db.rx_token(patient_id);
    if !had {
        app.changed();
    }
    crate::rx_link::link_url(&crate::screens::cita_online::patient_url(app), &t)
}

/// Envía al paciente, por correo, la hoja de información de sus recetas
/// (remisión telemática, control FT12). El PIN de confidencialidad no va en el correo.
fn mail_hip(app: &mut App, r: &Prescription, s: &crate::finance::SrepIssue, ctx: &egui::Context) {
    let Some(p) = app.db.patient(r.patient_id).cloned() else { return };
    let to = p.email.trim().to_string();
    if to.is_empty() || !app.db.mail.ready() {
        return;
    }
    // con la página publicada en internet, el enlace «Mis recetas» va en el correo
    let web = if app.db.booking.enabled && !app.db.booking.public_url.trim().is_empty() && crate::rx_link::can_verify(&p) {
        format!("\n\nTambién puede ver sus recetas, en qué estado están y esta hoja en el móvil, entrando con su DNI o NIE y su fecha de nacimiento: {}", rx_url(app, p.id))
    } else {
        String::new()
    };
    let c = &app.db.config;
    let conf = if s.confidential { "\n\nSu tratamiento es confidencial: en la farmacia diga el PIN que eligió en la consulta." } else { "" };
    let test = if s.test { "\n\n(Hoja de prueba: el sistema todavía no está certificado y estas recetas no valen en la farmacia.)" } else { "" };
    let subject = format!("Sus recetas · {}", c.name);
    let body = format!(
        "Hola, {}:\n\nLe enviamos la hoja de información de su tratamiento con sus recetas electrónicas (en el archivo adjunto). En la farmacia muestre la hoja, en papel o en el móvil, y su DNI: el farmacéutico lee uno de los códigos y ve sus recetas.{conf}{web}{test}\n\n{}\n{}",
        p.first_name.trim(),
        c.name,
        c.phone
    );
    let files = vec![crate::mail::FileAttachment { name: format!("hoja_recetas_{}.html", r.id), mime: "text/html".into(), bytes: hip_for(app, s).into_bytes() }];
    let cfg = app.db.mail.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let dest = to.clone();
    let repaint = ctx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(crate::mail::send(&cfg, &dest, &subject, &body, &files));
        repaint.request_repaint();
    });
    app.rx.mail_job = Some((r.id, to, rx));
}

/// Recetas emitidas: estado en las farmacias, hoja, seguimiento y anulación.
fn issued_view(app: &mut App, ui: &mut Ui, r: &Prescription, s: &crate::finance::SrepIssue) {
    muted(ui, format!("Firmada por {} el {}{}.", s.signer, s.issued_at.map(|t| t.format("%d/%m/%Y %H:%M").to_string()).unwrap_or_default(), s.checked_at.map(|t| format!(" · estado consultado {}", t.format("%d/%m %H:%M"))).unwrap_or_default()));
    let blocked: Vec<String> = s.issued.iter().flat_map(|p| p.recetas.iter().filter(|x| s.status.get(&x.id_receta) == Some(&2)).map(move |_| p.producto.denominacion.clone())).collect();
    if !blocked.is_empty() {
        ui.label(RichText::new(format!("⚠ Una farmacia ha bloqueado cautelarmente: {}. Revise el tratamiento.", blocked.join(", "))).color(DANGER).family(sb()));
    }
    let tbl = Table::new("srep_rec", vec![("Medicamento", 300.0), ("Se recoge", 190.0), ("Envases", 80.0), ("Estado", 230.0)]);
    tbl.header(ui);
    let mut i = 0;
    for p in &s.issued {
        for x in &p.recetas {
            let e = s.status.get(&x.id_receta).copied().unwrap_or(1);
            tbl.row(ui, i, false, &[cell(&p.producto.denominacion), cell(format!("{} – {}", x.fecha_ini.format("%d/%m/%Y"), x.fecha_fin.format("%d/%m/%Y"))), cell(x.num_envases.to_string()), cellc(crate::srep::estado_texto(e), estado_color(e))]);
            i += 1;
        }
    }
    for ev in s.events.iter().rev().take(6) {
        muted(ui, ev);
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button("🖨 Hoja para el paciente").clicked() {
            match export_hip(app, r, s) {
                Ok(p) => {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(format!("file://{}", p.display())));
                    app.notify(format!("Hoja guardada en {}", p.display()));
                }
                Err(e) => app.warn(e),
            }
        }
        // remisión telemática de la hoja al paciente (FT12)
        let email = app.db.patient(r.patient_id).map(|p| p.email.trim().to_string()).unwrap_or_default();
        let sending = app.rx.mail_job.as_ref().is_some_and(|j| j.0 == r.id);
        let can = !email.is_empty() && app.db.mail.ready() && app.rx.mail_job.is_none();
        let why = if email.is_empty() {
            "El paciente no tiene correo en su ficha".to_string()
        } else if !app.db.mail.ready() {
            "Configure el correo de la clínica en Configuración › Correo electrónico".to_string()
        } else {
            format!("Se envía a {email}, con la hoja adjunta (sin el PIN de confidencialidad)")
        };
        if ui.add_enabled(can, egui::Button::new(if sending { "Enviando la hoja…" } else { "✉ Enviar la hoja por correo" })).on_hover_text(&why).on_disabled_hover_text(&why).clicked() {
            mail_hip(app, r, s, ui.ctx());
        }
        // «Mis recetas»: el paciente las ve en el móvil con su DNI/NIE y su fecha de nacimiento (SC03)
        let verifiable = app.db.patient(r.patient_id).is_some_and(crate::rx_link::can_verify);
        let why = if !verifiable {
            "Falta el DNI/NIE o la fecha de nacimiento en la ficha del paciente"
        } else if !app.db.booking.enabled {
            "Active la página de Cita online (Configuración › Cita online) para que el enlace funcione"
        } else {
            "Copia su enlace personal: ve sus recetas, en qué estado están y la hoja para la farmacia en el móvil, entrando con su DNI/NIE y su fecha de nacimiento"
        };
        if ui.add_enabled(verifiable && app.db.booking.enabled, egui::Button::new("🔗 Enlace «Mis recetas»")).on_hover_text(why).on_disabled_hover_text(why).clicked() {
            let url = rx_url(app, r.patient_id);
            ui.ctx().copy_text(url.clone());
            app.notify(format!("Enlace «Mis recetas» copiado: {url}"));
        }
        if ui.button("↻ Ver qué han hecho las farmacias").clicked() {
            refresh(app, r.id);
        }
        let pending = s.status.values().any(|e| matches!(e, 0 | 1 | 2 | 8 | 10));
        if ui.add_enabled(pending, egui::Button::new("Anular lo que no se ha dispensado")).clicked() {
            let client = crate::srep::RepoClient::new(&app.db.erx.repo_addr, &app.db.erx.repo_key);
            let mut n = 0;
            let mut err = None;
            for p in &s.issued {
                match client.anular(&p.id_prescripcion, None, "Anulada por el prescriptor") {
                    Ok(k) => n += k,
                    Err(e) => err = Some(e),
                }
            }
            app.db.audit("Receta electrónica anulada por el prescriptor", &format!("receta {} · {n} receta(s)", r.id));
            refresh(app, r.id);
            match err {
                Some(e) if n == 0 => app.warn(e),
                _ => app.notify(format!("{n} receta(s) anuladas: las farmacias ya no las ven.")),
            }
        }
    });
    if let Some((v, n)) = app.db.rx_link(r.patient_id).and_then(|l| l.viewed.map(|v| (v, l.accesses))) {
        muted(ui, format!("El paciente entró en «Mis recetas» por última vez el {} a las {} ({n} {} en total).", fmt_date(v.date()), v.format("%H:%M"), if n == 1 { "vez" } else { "veces" }));
    }
}

/// Pregunta al repositorio el estado de cada receta y lo que han hecho las farmacias.
fn refresh(app: &mut App, id: u32) {
    let Some(s) = app.db.prescriptions.iter().find(|x| x.id == id).and_then(|x| x.srep.clone()) else { return };
    let client = crate::srep::RepoClient::new(&app.db.erx.repo_addr, &app.db.erx.repo_key);
    let ids: Vec<String> = s.issued.iter().map(|p| p.id_prescripcion.clone()).collect();
    match client.seguimiento(&ids) {
        Ok(seg) => {
            let events: Vec<String> = seg
                .acciones
                .iter()
                .map(|a| {
                    let what = s.issued.iter().find(|p| p.recetas.iter().any(|x| x.id_receta == a.id_receta)).map(|p| p.producto.denominacion.clone()).unwrap_or_default();
                    let extra = match a.accion {
                        1 | 2 | 3 => format!(" · {} envase(s)", a.envases),
                        0 => format!(" · {}", a.causa.map(crate::srep::causa_bloqueo_texto).unwrap_or("")),
                        _ => String::new(),
                    };
                    format!("{} · farmacia {} · {} de {what}{extra}{}", a.recibida.format("%d/%m %H:%M"), a.id_farmacia, crate::srep::accion_texto(a.accion), if a.resultado == "RACOK" { String::new() } else { format!(" (rechazada: {})", a.resultado) })
                })
                .collect();
            let blocked = seg.estados.values().any(|e| *e == 2) && !s.status.values().any(|e| *e == 2);
            if let Some(x) = app.db.prescriptions.iter_mut().find(|x| x.id == id).and_then(|x| x.srep.as_mut()) {
                x.status = seg.estados.into_iter().collect();
                x.events = events;
                x.checked_at = Some(crate::model::now());
            }
            app.changed();
            if blocked {
                app.warn("Una farmacia ha bloqueado cautelarmente una de estas recetas.");
            } else {
                app.notify("Estado de las recetas actualizado.");
            }
        }
        Err(e) => app.warn(e),
    }
}

/// Emisión de la receta oficial en la plataforma homologada y registro del CVE.
fn official_card(app: &mut App, ui: &mut Ui, id: u32) {
    if app.db.erx.direct {
        direct_card(app, ui, id);
        return;
    }
    let Some(r) = app.db.prescriptions.iter().find(|r| r.id == id).cloned() else { return };
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Receta oficial electrónica");
            right_aligned(ui, |ui| {
                if !r.erx_code.is_empty() {
                    pill(ui, &format!("Emitida · CVE {}", r.erx_code), OK);
                } else if crate::erx::doctor_ready(&app.db, r.doctor_id) {
                    pill(ui, "Lista para emitir", INFO);
                } else {
                    pill(ui, "Falta configurar", WARN);
                }
            });
        });
        if !r.erx_code.is_empty() {
            muted(ui, format!("Emitida el {} en {}. La farmacia la valida con el código de verificación.", r.erx_date.map(fmt_date).unwrap_or_default(), r.erx_platform));
            if ui.button("Quitar el código (se emitió por error)").clicked() {
                if let Some(x) = app.db.prescriptions.iter_mut().find(|x| x.id == id) {
                    x.erx_code.clear();
                    x.erx_date = None;
                }
                app.db.audit("Receta electrónica: código retirado", &format!("receta {id}"));
                app.changed();
            }
            return;
        }
        let pending: Vec<(&str, bool, &str)> = crate::erx::doctor_readiness(&app.db, r.doctor_id);
        let missing = crate::erx::missing_data(&app.db, &r);
        if pending.iter().any(|p| !p.1) {
            ui.label(RichText::new(format!("Para que {} pueda emitir recetas oficiales falta:", app.db.doctor_alias(r.doctor_id))).color(WARN));
            for (what, ok, how) in &pending {
                if !ok {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("• {what}")).family(sb()));
                        muted(ui, *how);
                    });
                }
            }
        }
        if !missing.is_empty() {
            ui.label(RichText::new(format!("Datos obligatorios que faltan en la receta: {}", missing.join(", "))).color(DANGER));
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let can_open = !app.db.erx.url.trim().is_empty();
            if ui.add_enabled(can_open && missing.is_empty(), egui::Button::new(RichText::new("1 · Copiar datos y abrir la plataforma").color(Color32::WHITE)).fill(ACCENT)).on_hover_text("Copia paciente, prescriptor y medicamentos para pegarlos en la plataforma").clicked() {
                ui.ctx().copy_text(crate::erx::transfer_text(&app.db, &r));
                ui.ctx().open_url(egui::OpenUrl::new_tab(app.db.erx.url.trim().to_string()));
                app.notify("Datos copiados: péguelos en la plataforma, firme la receta y anote aquí el código de verificación.");
            }
            ui.label("2 · Código de verificación (CVE)");
            ui.add_sized([200.0, 32.0], egui::TextEdit::singleline(&mut app.rx.cve).hint_text("Lo da la plataforma"));
            if ui.button("Guardar como emitida").clicked() {
                let code = app.rx.cve.trim().to_uppercase();
                if !crate::erx::cve_valid(&code) {
                    app.warn("El código de verificación no parece válido (mínimo 8 letras o números).");
                } else {
                    let platform = app.db.erx.platform.clone();
                    if let Some(x) = app.db.prescriptions.iter_mut().find(|x| x.id == id) {
                        x.erx_code = code.clone();
                        x.erx_platform = platform;
                        x.erx_date = Some(app.today);
                    }
                    app.db.audit("Receta electrónica emitida", &format!("receta {id} · CVE {code}"));
                    app.rx.cve.clear();
                    app.changed();
                    app.notify("Receta oficial registrada.");
                }
            }
        });
        muted(ui, "La receta oficial se firma y se envía desde la plataforma homologada; aquí queda guardado el código para la historia y la copia del paciente.");
    });
}

/// Configuración › Receta electrónica: plataforma y requisitos de cada dentista.
pub fn config_panel(app: &mut App, ui: &mut Ui) {
    let before = app.db.erx.clone();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Receta electrónica oficial");
        muted(ui, "En la sanidad privada la receta oficial se emite a través de una plataforma de prescripción homologada por los Consejos de dentistas, médicos y farmacéuticos (no del sistema de la comunidad autónoma, que es solo para la sanidad pública). La plataforma valida al colegiado, firma la receta y la hace dispensable en cualquier farmacia.");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            field(ui, "Plataforma homologada que usa la clínica", &mut app.db.erx.platform, 300.0);
            field(ui, "Dirección web para emitir recetas", &mut app.db.erx.url, 380.0);
        });
        ui.add_space(6.0);
        let docs: Vec<(u32, String, String)> = app.db.collaborators.iter().filter(|c| c.active).map(|c| (c.id, c.alias.clone(), c.license.clone())).collect();
        egui::Grid::new("erx_docs").num_columns(5).spacing([22.0, 8.0]).show(ui, |ui| {
            for h in ["Profesional", "Nº colegiado", "Certificado de firma", "Alta en la plataforma", "Estado"] {
                ui.label(RichText::new(h).color(MUTED).size(13.0));
            }
            ui.end_row();
            for (id, alias, lic) in docs {
                let mut d = app.db.erx.doctor(id);
                ui.label(RichText::new(&alias).family(sb()));
                ui.label(if lic.trim().is_empty() || lic.trim() == "-" { RichText::new("sin número").color(WARN) } else { RichText::new(lic) });
                let a = ui.checkbox(&mut d.certificate, "Tiene").changed();
                let b = ui.checkbox(&mut d.registered, "Dado de alta").changed();
                if a || b {
                    app.db.erx.set_doctor(d);
                }
                if crate::erx::doctor_ready(&app.db, id) {
                    pill(ui, "Puede recetar", OK);
                } else {
                    pill(ui, "Pendiente", WARN);
                }
                ui.end_row();
            }
        });
    });
    ui.add_space(10.0);
    nomenclator_panel(app, ui);
    ui.add_space(10.0);
    real_pharmacies(app, ui);
    ui.add_space(10.0);
    direct_panel(app, ui);
    if app.db.erx != before {
        app.changed();
    }
}

/// Dentistas que recetan: (nombre, tiene nº de colegiado, tiene certificado elegido
/// en este ordenador). Se mira cada pocos segundos (el certificado está en un fichero).
fn prescribers(app: &mut App, time: f64) -> Vec<(String, bool, bool)> {
    if let Some((t, v)) = &app.rx.prescribers {
        if time >= *t && time - *t < 5.0 {
            return v.clone();
        }
    }
    let v: Vec<(String, bool, bool)> = app.db.collaborators.iter().filter(|c| c.active && c.license.trim() != "-").map(|c| (c.alias.clone(), !c.license.trim().is_empty(), !crate::erx::cert_path(c.id).trim().is_empty())).collect();
    app.rx.prescribers = Some((time, v.clone()));
    v
}

/// Un paso del camino: marca de hecho o pendiente, título, explicación y botón.
fn phase(ui: &mut Ui, done: bool, title: &str, detail: &str, button: Option<&str>) -> bool {
    let col = if done { OK } else { WARN };
    let mut clicked = false;
    ui.add_space(3.0);
    egui::Frame::new().fill(ROW_ALT).stroke(egui::Stroke::new(1.0, HAIRLINE)).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(12, 9)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
            let (p, m) = (ui.painter(), r.center());
            p.circle_filled(m, 12.0, tint(col, 0.14));
            if done {
                p.line_segment([m + egui::vec2(-5.0, 0.5), m + egui::vec2(-1.5, 4.0)], egui::Stroke::new(2.0, col));
                p.line_segment([m + egui::vec2(-1.5, 4.0), m + egui::vec2(5.5, -4.0)], egui::Stroke::new(2.0, col));
            } else {
                p.line_segment([m + egui::vec2(0.0, -5.5), m + egui::vec2(0.0, 1.5)], egui::Stroke::new(2.2, col));
                p.circle_filled(m + egui::vec2(0.0, 5.5), 1.4, col);
            }
            ui.add_space(4.0);
            // el texto ocupa todo lo que deja el botón (medido con su letra)
            let bw = button.map(|b| ui.painter().layout_no_wrap(b.to_string(), egui::TextStyle::Button.resolve(ui.style()), TEXT).size().x + 2.0 * ui.spacing().button_padding.x + 24.0).unwrap_or(0.0);
            let tw = (ui.available_width() - bw - 10.0).max(220.0);
            let text = ui
                .vertical(|ui| {
                    ui.set_width(tw);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(title).family(sb()).size(14.5).color(HEADING));
                    ui.label(RichText::new(detail).size(13.5).color(MUTED));
                })
                .response;
            if let Some(b) = button {
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), text.rect.height().max(26.0)), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(b).clicked() {
                        clicked = true;
                    }
                });
            }
        });
    });
    clicked
}

/// Camino a las farmacias reales: lo que hace falta para que los Consejos
/// certifiquen el sistema, con la prueba del circuito y el expediente.
fn real_pharmacies(app: &mut App, ui: &mut Ui) {
    use crate::srep_cert::Kind;
    let docs = prescribers(app, ui.input(|i| i.time));
    let tally = crate::srep_cert::tally();
    let n = |k: Kind| tally.iter().find(|t| t.0 == k).map(|t| t.1).unwrap_or(0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Camino a las farmacias reales");
        muted(ui, "Para que cualquier farmacia de España dispense estas recetas, los Consejos Generales tienen que certificar el sistema: un auditor revisa 30 controles y se hace una prueba conjunta con Nodofarma. Estos son los pasos, y lo que falta en cada uno.");
        ui.add_space(4.0);
        // 1. el circuito
        let (done, text) = match &app.rx.circuit {
            Some((when, steps)) => {
                let ok = steps.iter().filter(|s| s.ok).count();
                (ok == steps.len(), format!("{ok} de {} pruebas correctas el {} a las {}: emitir, consultar, PIN de confidencialidad, plazos, dispensar en parte y del todo, anular, bloquear, seguimiento, errores, plan del tratamiento y hoja del paciente.", steps.len(), fmt_date(when.date()), when.format("%H:%M")))
            }
            None => (false, "Pruebe el circuito completo con una farmacia simulada, en memoria y sin tocar el repositorio de verdad: son los casos de uso que repasa el auditor.".to_string()),
        };
        if phase(ui, done, "1. El circuito completo funciona", &text, Some("Probar con una farmacia simulada")) {
            let steps = crate::srep_cert::circuit_test(app.today);
            let bad: Vec<String> = steps.iter().filter(|s| !s.ok).map(|s| format!("{} ({})", s.what, s.detail)).collect();
            app.rx.circuit = Some((crate::model::now(), steps));
            if bad.is_empty() {
                app.notify("Circuito completo probado con la farmacia simulada: todo correcto.");
            } else {
                app.warn(format!("Fallan {} pruebas del circuito: {}", bad.len(), bad.join("; ")));
            }
        }
        if let Some((_, steps)) = &app.rx.circuit {
            for s in steps.iter().filter(|s| !s.ok) {
                ui.label(RichText::new(format!("✗ {} · {}: {}", s.control, s.what, s.detail)).color(DANGER).size(13.5));
            }
        }
        // 2. los controles
        let text = format!(
            "{} de los 30 controles los cubre el programa; {} en parte ({}), {} dependen de dónde se instale el repositorio y {} de la documentación de la clínica.",
            n(Kind::Programa),
            n(Kind::Parcial),
            crate::srep_cert::partial_titles(),
            n(Kind::Instalacion),
            n(Kind::Documento)
        );
        if phase(ui, false, "2. Controles de la auditoría", &text, Some(if app.rx.show_controls { "Ocultar los controles" } else { "Ver los controles" })) {
            app.rx.show_controls = !app.rx.show_controls;
        }
        if app.rx.show_controls {
            egui::Frame::new().inner_margin(egui::Margin { left: 42, right: 8, top: 4, bottom: 4 }).show(ui, |ui| {
                for c in crate::srep_cert::CONTROLS.iter() {
                    let col = match c.kind {
                        Kind::Programa => OK,
                        Kind::Parcial => WARN,
                        Kind::Instalacion | Kind::Documento => INFO,
                        Kind::Opcional => SUBTLE,
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(c.code).family(sb()).size(13.5).color(HEADING));
                        ui.label(RichText::new(c.title).size(13.5).color(TEXT));
                        pill(ui, c.kind.label(), col);
                    });
                    ui.label(RichText::new(c.how).size(13.0).color(MUTED));
                    ui.add_space(4.0);
                }
            });
        }
        // 3. el repositorio publicado
        let host = app.db.erx.public_host.trim().to_string();
        let text = if host.is_empty() {
            "Falta publicar el repositorio en un servidor siempre encendido (24 horas, todo el año), con un nombre público y certificado TLS. Más abajo están el servicio y la configuración del proxy.".to_string()
        } else {
            format!("Nodofarma llegará a https://{host}/repositorio/api/v1/. Tiene que estar en un servidor siempre encendido, con certificado TLS y el proxy que da el programa (más abajo).")
        };
        phase(ui, !host.is_empty(), "3. Repositorio publicado las 24 horas", &text, None);
        // 4. los dentistas
        let missing: Vec<String> = docs.iter().filter(|d| !(d.1 && d.2)).map(|d| format!("{} ({})", d.0, if !d.1 { "nº de colegiado" } else { "certificado de firma" })).collect();
        let ready = docs.len() - missing.len();
        let text = if docs.is_empty() {
            "No hay dentistas activos que receten.".to_string()
        } else if missing.is_empty() {
            format!("{ready} con nº de colegiado y su certificado de firma elegido en este ordenador.")
        } else {
            format!("Listos {ready} de {}. Falta: {}. El certificado se elige al firmar la primera receta.", docs.len(), missing.join(", "))
        };
        phase(ui, !docs.is_empty() && missing.is_empty(), "4. Dentistas con colegiado y certificado", &text, None);
        // 5. la solicitud
        let sent = app.db.erx.cert_requested;
        let text = match sent {
            Some(d) => format!("Solicitud enviada el {}. Si cambia algo, genere de nuevo el expediente con los datos de ahora.", fmt_date(d)),
            None => "Genere el expediente (datos del sistema, dentistas, cómo cumple cada control y las pruebas del circuito), envíelo a los Consejos con la documentación de cada control y márquelo como enviado.".to_string(),
        };
        if phase(ui, sent.is_some(), "5. Solicitud de certificación", &text, Some("Generar el expediente")) {
            match write_dossier(app) {
                Ok(p) => {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(format!("file://{}", p.display())));
                    app.notify(format!("Expediente guardado en {}", p.display()));
                }
                Err(e) => app.warn(e),
            }
        }
        ui.horizontal(|ui| {
            ui.add_space(44.0);
            let label = if sent.is_some() { "Desmarcar la solicitud" } else { "Ya la he enviado: marcarla con la fecha de hoy" };
            if ui.link(label).clicked() {
                app.db.erx.cert_requested = if sent.is_some() { None } else { Some(app.today) };
                if sent.is_none() {
                    app.db.audit("Solicitud de certificación de la receta electrónica enviada", "");
                }
            }
        });
        // 6. certificado
        let certified = app.db.erx.certified();
        let text = if certified {
            "Identificadores de certificación puestos: las recetas valen en cualquier farmacia y las hojas ya no llevan la marca PRUEBAS."
        } else {
            "Cuando los Consejos certifiquen el sistema y se supere la prueba conjunta con Nodofarma, escriba más abajo idSistema, idEntidadSanitaria e idRepositorio: desde ese momento las recetas valen en cualquier farmacia."
        };
        phase(ui, certified, "6. Certificado: recetas para las farmacias reales", text, None);
    });
}

/// Escribe el expediente de certificación (con una prueba del circuito de
/// hoy) en Documentos y devuelve dónde.
fn write_dossier(app: &mut App) -> Result<std::path::PathBuf, String> {
    let now = crate::model::now();
    let fresh = app.rx.circuit.as_ref().is_some_and(|(w, _)| w.date() == now.date());
    if !fresh {
        app.rx.circuit = Some((now, crate::srep_cert::circuit_test(app.today)));
    }
    let steps = app.rx.circuit.as_ref().map(|c| c.1.clone()).unwrap_or_default();
    let html = crate::srep_cert::dossier_html(&app.db, &steps, crate::srep_cert::local_log_check(), now);
    let path = docs_dir("certificacion")?.join(format!("expediente_receta_electronica_{}.html", now.format("%Y%m%d_%H%M")));
    std::fs::write(&path, html).map_err(|e| format!("No se pudo guardar el expediente: {e}"))?;
    app.db.audit("Expediente de certificación de la receta electrónica generado", &path.display().to_string());
    app.changed();
    Ok(path)
}

/// Conexión directa con el Sistema de Receta Electrónica Privada: el
/// repositorio de recetas al que llegan las farmacias por Nodofarma.
fn direct_panel(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Receta oficial por conexión directa (sistema de prescripción propio)");
        muted(ui, "El programa guarda las recetas firmadas en un repositorio propio y las farmacias de toda España las consultan, dispensan o bloquean a través de Nodofarma, siguiendo el diseño técnico de los Consejos Generales (servicios del repositorio v2.04.1). Para funcionar con farmacias reales hace falta la certificación del sistema y del repositorio por el CGCOM y el Consejo de Dentistas (auditoría de los controles FT, FD, SC, IN y PD), que asigna los identificadores y entrega el certificado de Nodofarma.");
        ui.add_space(6.0);
        let e = &mut app.db.erx;
        if e.repo_addr.trim().is_empty() {
            e.repo_addr = format!("127.0.0.1:{}", crate::srep::DEFAULT_PORT);
        }
        if e.repo_key.trim().is_empty() {
            e.repo_key = crate::server::new_key();
        }
        ui.horizontal_wrapped(|ui| {
            field(ui, "Repositorio de recetas (dirección)", &mut e.repo_addr, 220.0);
            field(ui, "Clave del repositorio", &mut e.repo_key, 160.0);
            field(ui, "Nombre público para Nodofarma", &mut e.public_host, 240.0);
        });
        ui.horizontal_wrapped(|ui| {
            field(ui, "idSistema (64, lo asigna el CGCOM)", &mut e.id_sistema, 300.0);
            field(ui, "idEntidadSanitaria", &mut e.id_entidad, 130.0);
            field(ui, "idRepositorio (32)", &mut e.id_repositorio, 260.0);
        });
        let certified = e.certified();
        ui.checkbox(&mut e.require_2fa, "Para firmar recetas, pedir el código del móvil al entrar (doble factor del dentista; se activa en Usuarios y permisos)");
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut e.direct, RichText::new("Emitir las recetas oficiales por esta conexión directa").family(sb()));
            if certified {
                pill(ui, "Identificadores de certificación completos", OK);
            } else {
                pill(ui, "Sin certificar: solo pruebas, las hojas salen marcadas «PRUEBAS»", WARN);
            }
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let local = crate::server::is_local(&app.db.erx.repo_addr);
            if ui.add_enabled(local, egui::Button::new("▶ Iniciar el repositorio en este ordenador")).on_disabled_hover_text("Solo en el ordenador cuya dirección es la del repositorio").clicked() {
                let e = &app.db.erx;
                let port = e.repo_addr.rsplit(':').next().and_then(|p| p.trim().parse().ok()).unwrap_or(crate::srep::DEFAULT_PORT);
                let cfg = crate::srep::RepoConfig { port, key: e.repo_key.trim().to_string(), id_repositorio: e.id_repositorio.trim().to_string(), allowed_ips: vec!["127.0.0.1".into(), "::1".into()], require_cert_header: true, base_path: "/repositorio/api/v1".into(), lan: false };
                match crate::srep::save_config(&cfg).and_then(|_| crate::srep::spawn_local()) {
                    Ok(()) => app.rx.status = "Repositorio arrancando; pulse «Comprobar» en unos segundos.".into(),
                    Err(err) => app.rx.status = format!("No se pudo iniciar: {err}"),
                }
            }
            if ui.button("Comprobar").clicked() {
                let c = crate::srep::RepoClient::new(&app.db.erx.repo_addr, &app.db.erx.repo_key);
                app.rx.status = match c.estado() {
                    Ok(v) => format!("Repositorio funcionando ({}) · {} recetas guardadas · {} actuaciones de farmacias.", v["version"].as_str().unwrap_or(""), v["recetas"], v["acciones"]),
                    Err(err) => err,
                };
            }
            if ui.button("📋 Copiar configuración del proxy (nginx)").on_hover_text("TLS con el certificado del repositorio y verificación del certificado de Nodofarma").clicked() {
                let port = app.db.erx.repo_addr.rsplit(':').next().and_then(|p| p.trim().parse().ok()).unwrap_or(crate::srep::DEFAULT_PORT);
                ui.ctx().copy_text(crate::srep::nginx_snippet(&app.db.erx.public_host, "/repositorio/api/v1", port));
                app.rx.status = "Configuración del proxy copiada.".into();
            }
        });
        if !app.rx.status.is_empty() {
            ui.label(RichText::new(&app.rx.status).color(INFO));
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Correo de cada prescriptor (va en la receta; si se deja vacío, el de la clínica)").size(13.5).color(MUTED));
        let docs: Vec<(u32, String)> = app.db.collaborators.iter().filter(|c| c.active && c.license.trim() != "-").map(|c| (c.id, c.alias.clone())).collect();
        ui.horizontal_wrapped(|ui| {
            for (id, alias) in docs {
                let mut d = app.db.erx.doctor(id);
                ui.label(RichText::new(alias).family(sb()));
                if ui.add_sized([210.0, 30.0], egui::TextEdit::singleline(&mut d.email).hint_text(app.db.config.email.clone())).changed() {
                    app.db.erx.set_doctor(d);
                }
            }
        });
        let host =if app.db.erx.public_host.trim().is_empty() { "su-servidor".to_string() } else { app.db.erx.public_host.trim().to_string() };
        muted(ui, format!("Dirección que se comunica al CGCOM (URLRepositorio): https://{host}/repositorio/api/v1/ · Las recetas se firman con el certificado personal de cada dentista (CAdES) y la hoja del paciente lleva un DataMatrix por receta."));
    });
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn export_html(app: &App, r: &Prescription) -> Result<std::path::PathBuf, String> {
    let dir = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| std::path::PathBuf::from(".")).join(crate::brand::DIR).join("recetas");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = app.db.patient(r.patient_id).cloned().unwrap_or_default();
    let d = app.db.doctor(r.doctor_id).cloned();
    let path = dir.join(format!("receta_{}_{}.html", p.history_no, r.id));
    let items: String = r
        .items
        .iter()
        .map(|i| format!("<li><b>{}</b><br>{} {} durante {} días · {} envase(s){}</li>", esc(&i.drug), esc(&i.dose), esc(&i.frequency), i.days, i.units, if i.notes.is_empty() { String::new() } else { format!("<br><i>{}</i>", esc(&i.notes)) }))
        .collect();
    let html = format!(
        "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Receta</title><style>body{{font-family:'Open Sans',Arial,sans-serif;max-width:700px;margin:40px auto;color:#222}}.head{{border-bottom:2px solid #0e9688;padding-bottom:8px;margin-bottom:20px}}li{{margin-bottom:14px}}.firma{{margin-top:60px;border-top:1px solid #999;width:260px;padding-top:6px;font-size:13px}}</style></head><body><div class='head'><b>{clinic}</b><br><small>{addr} · {phone}</small></div><h2>Receta médica</h2><p>Paciente: <b>{pname}</b> · NIF {nif} · Fecha {date}<br>Diagnóstico: {diag}</p><ol>{items}</ol><p>{notes}</p>{cve}<div class='firma'>{doc}<br>Nº colegiado {lic}</div></body></html>",
        clinic = esc(&app.db.config.name),
        addr = esc(&app.db.config.address),
        phone = esc(&app.db.config.phone),
        pname = esc(&p.full_name()),
        nif = esc(&p.nif),
        date = fmt_date(r.date),
        diag = esc(&r.diagnosis),
        items = items,
        notes = esc(&r.notes),
        doc = esc(&d.as_ref().map(|x| x.name.clone()).unwrap_or_default()),
        lic = esc(&d.as_ref().map(|x| x.license.clone()).unwrap_or_default()),
        cve = if r.erx_code.is_empty() {
            "<p style='color:#b45309'><b>Copia informativa:</b> no es la receta oficial para la farmacia.</p>".to_string()
        } else {
            format!("<p><b>Receta electrónica emitida</b> en {} · Código de verificación: <b>{}</b></p>", esc(&r.erx_platform), esc(&r.erx_code))
        },
    );
    std::fs::write(&path, html).map_err(|e| e.to_string())?;
    Ok(path)
}
