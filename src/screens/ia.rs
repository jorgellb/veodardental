//! Análisis de imágenes con IA: configuración de agentes (Configuración) y
//! panel de análisis en el visor de imágenes del paciente.

use crate::app::{App, Screen};
use crate::ia::{self, Agent, IaReport, Mode, Provider};
use crate::model::*;
use crate::theme::*;
use egui::{Color32, CornerRadius, RichText, Stroke, Ui, vec2};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

/// Análisis en marcha (en segundo plano).
pub struct Job {
    pub image_id: u32,
    pub patient_id: u32,
    pub rx: Receiver<ia::Outcome>,
    pub progress: Arc<Mutex<String>>,
    pub started: std::time::Instant,
}

/// Modelos del proveedor que se están pidiendo: (agente, respuesta cuando llegue).
pub type ModelsJob = (u32, Receiver<Result<Vec<ia::ModelInfo>, String>>);

#[derive(Default)]
pub struct IaUi {
    pub job: Option<Job>,
    /// Modo elegido para el próximo análisis (si no, el de la configuración).
    pub mode: Option<Mode>,
    /// Último fallo: imagen, mensaje e intentos.
    pub error: Option<(u32, String, Vec<ia::Attempt>)>,
    pub edit: Option<u32>,
    pub show_key: bool,
    pub models: Option<(u32, Result<Vec<ia::ModelInfo>, String>)>,
    pub models_job: Option<ModelsJob>,
    pub model_filter: String,
    pub all_models: bool,
    pub only_free: bool,
    pub tests: HashMap<u32, Result<String, String>>,
    pub test_jobs: Vec<(u32, Receiver<Result<String, String>>)>,
    /// Propuestas que el dentista ha desmarcado (informe, índice).
    pub unchecked: HashSet<(u32, usize)>,
    pub new_provider: Provider,
}

/// Recoge lo que terminan los trabajos en segundo plano (se llama en cada fotograma).
pub fn poll(app: &mut App) {
    let finished = app.ia.job.as_ref().and_then(|j| j.rx.try_recv().ok().map(|o| (j.image_id, j.patient_id, o)));
    if let Some((image_id, patient_id, out)) = finished {
        app.ia.job = None;
        match out.result {
            Ok((agent, analysis)) => {
                let id = app.db.new_id();
                let user = app.db.config.current_user.clone();
                let n = analysis.findings.len();
                app.db.ia_reports.push(IaReport { id, patient_id, image_id, when: chrono::Local::now().naive_local(), agent: agent.name.clone(), model: agent.model.clone(), free: agent.free, analysis, attempts: out.attempts, user, planned: vec![] });
                app.ia.error = None;
                app.changed();
                app.notify(format!("Análisis listo: {n} hallazgos ({}).", agent.name));
            }
            Err(e) => app.ia.error = Some((image_id, e, out.attempts)),
        }
    }
    let mut i = 0;
    while i < app.ia.test_jobs.len() {
        if let Ok(r) = app.ia.test_jobs[i].1.try_recv() {
            let (id, _) = app.ia.test_jobs.remove(i);
            app.ia.tests.insert(id, r);
        } else {
            i += 1;
        }
    }
    let models = app.ia.models_job.as_ref().and_then(|(id, rx)| rx.try_recv().ok().map(|r| (*id, r)));
    if let Some(m) = models {
        app.ia.models_job = None;
        app.ia.models = Some(m);
    }
}

fn start_test(app: &mut App, ctx: &egui::Context, a: Agent) {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    let timeout = app.db.ia.timeout_secs;
    let id = a.id;
    std::thread::spawn(move || {
        let _ = tx.send(ia::test_agent(&a, timeout));
        ctx.request_repaint();
    });
    app.ia.tests.remove(&id);
    app.ia.test_jobs.push((id, rx));
}

fn start_models(app: &mut App, ctx: &egui::Context, a: Agent) {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    let id = a.id;
    std::thread::spawn(move || {
        let _ = tx.send(ia::list_models(&a));
        ctx.request_repaint();
    });
    app.ia.models = None;
    app.ia.models_job = Some((id, rx));
}

/// Lanza el análisis de una imagen en segundo plano.
fn start_analysis(app: &mut App, ctx: &egui::Context, img: &PatientImage, mode: Mode) {
    let rgb = match crate::screens::imagenes::rgb_for_ai(img) {
        Ok(x) => x,
        Err(e) => return app.warn(e),
    };
    let jpeg = match ia::jpeg_for_ai(&rgb) {
        Ok(j) => j,
        Err(e) => return app.warn(e),
    };
    let age = if app.db.ia.send_age { app.db.patient(img.patient_id).and_then(|p| p.age(app.today)).map(|a| a.max(0) as u32) } else { None };
    let req = ia::Request { jpeg, kind: img.kind.label().to_string(), teeth: img.teeth.clone(), age };
    let cfg = app.db.ia.clone();
    let progress = Arc::new(Mutex::new("Preparando la imagen…".to_string()));
    let (tx, rx) = std::sync::mpsc::channel();
    let p2 = progress.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let c2 = ctx.clone();
        let out = ia::analyze(&cfg, mode, &req, &move |m| {
            if let Ok(mut g) = p2.lock() {
                *g = m;
            }
            c2.request_repaint();
        });
        let _ = tx.send(out);
        ctx.request_repaint();
    });
    app.ia.error = None;
    app.ia.job = Some(Job { image_id: img.id, patient_id: img.patient_id, rx, progress, started: std::time::Instant::now() });
}

fn free_pill(ui: &mut Ui, free: bool) {
    if free {
        pill(ui, "Gratuito", OK);
    } else {
        pill(ui, "De pago", INFO);
    }
}

fn mode_picker(ui: &mut Ui, mode: &mut Mode) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for m in Mode::ALL {
            if ui.selectable_label(*mode == m, m.label()).clicked() {
                *mode = m;
            }
        }
    });
}

// ───────────────────────────── Configuración ─────────────────────────────

/// Configuración › Inteligencia artificial.
pub fn config_panel(app: &mut App, ui: &mut Ui) {
    poll(app);
    let ctx = ui.ctx().clone();
    let before = serde_json::to_string(&app.db.ia).unwrap_or_default();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Inteligencia artificial · análisis de radiografías");
            right_aligned(ui, |ui| {
                let ready = app.db.ia.agents.iter().filter(|a| a.enabled && (!a.provider.needs_key() || !a.api_key.trim().is_empty())).count();
                if app.db.ia.enabled {
                    pill(ui, &format!("Activo · {ready} agentes listos"), if ready > 0 { OK } else { WARN });
                } else {
                    pill(ui, "Desactivado", MUTED);
                }
            });
        });
        muted(ui, "Analiza radiografías y fotos del paciente: hallazgos por pieza y una propuesta de tratamiento que el dentista revisa. Puede usar varios proveedores a la vez; si uno falla, responde el siguiente de la lista.");
        ui.add_space(8.0);
        // aviso de protección de datos
        egui::Frame::new().fill(tint(WARN, 0.07)).stroke(Stroke::new(1.0, tint(WARN, 0.25))).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Protección de datos").font(sbf(14.0)).color(mix(WARN, HEADING, 0.4)));
            ui.label(RichText::new("Las radiografías son datos de salud. Al analizarlas se envían al proveedor elegido (salvo con Ollama, que funciona en este ordenador). Se envía sólo la imagen, sin nombre ni datos del paciente, pero algunas radiografías llevan el nombre impreso en la propia imagen. Antes de usarlo, compruebe que el proveedor firma un contrato de encargado del tratamiento (RGPD) y que sus condiciones admiten datos sanitarios: los modelos gratuitos suelen usar lo que reciben para entrenarse. Las claves se guardan con los datos de la clínica.").font(mdf(12.8)).color(TEXT));
            ui.checkbox(&mut app.db.ia.accepted, "He leído el aviso y la clínica lo acepta");
        });
        ui.add_space(8.0);
        let cfg = &mut app.db.ia;
        ui.add_enabled_ui(cfg.accepted, |ui| {
            ui.checkbox(&mut cfg.enabled, RichText::new("Activar el análisis con IA").font(sbf(14.0)));
        });
        if !cfg.accepted {
            cfg.enabled = false;
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Qué agentes se usan por defecto").font(mdf(13.0)).color(MUTED));
        mode_picker(ui, &mut cfg.mode);
        ui.add_space(10.0);
    });
    ui.add_space(8.0);
    agents_card(app, ui, &ctx);
    if serde_json::to_string(&app.db.ia).unwrap_or_default() != before {
        app.dirty = true;
    }
}

fn agents_card(app: &mut App, ui: &mut Ui, ctx: &egui::Context) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Agentes");
            right_aligned(ui, |ui| {
                if primary(ui, "＋ Añadir agente").clicked() {
                    let p = app.ia.new_provider;
                    let (model, free) = p.suggested().first().copied().unwrap_or(("", false));
                    let id = app.db.ia.next_agent_id();
                    app.db.ia.agents.push(Agent::new(id, p, model, free));
                    app.ia.edit = Some(id);
                    app.ia.models = None;
                }
                let cur = app.ia.new_provider;
                crate::theme::combo("ia_newprov").width(200.0).selected_text(cur.label()).show_ui(ui, |ui| {
                    for p in Provider::ALL {
                        ui.selectable_value(&mut app.ia.new_provider, p, p.label());
                    }
                });
            });
        });
        muted(ui, "Se prueban en el orden de la lista: primero los gratuitos y después los de pago (según el modo). Con las flechas cambia el orden.");
        ui.add_space(6.0);
        let testing: Vec<u32> = app.ia.test_jobs.iter().map(|t| t.0).collect();
        let mut action: Option<(u32, &str)> = None;
        for (group_free, title) in [(true, "GRATUITOS"), (false, "DE PAGO")] {
            let ids: Vec<u32> = app.db.ia.agents.iter().filter(|a| a.free == group_free).map(|a| a.id).collect();
            ui.add_space(4.0);
            ui.label(RichText::new(format!("{title} · {}", ids.len())).font(sbf(11.5)).color(SUBTLE));
            if ids.is_empty() {
                muted(ui, if group_free { "Ninguno. Añada, por ejemplo, modelos «:free» de OpenRouter o Gemini." } else { "Ninguno." });
            }
            for (pos, id) in ids.iter().enumerate() {
                let Some(a) = app.db.ia.agents.iter().find(|a| a.id == *id).cloned() else { continue };
                let editing = app.ia.edit == Some(a.id);
                egui::Frame::new()
                    .fill(if editing { ACCENT_SOFT } else { ROW_ALT })
                    .stroke(Stroke::new(1.0, if editing { tint(ACCENT, 0.4) } else { HAIRLINE }))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            // número de orden
                            let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 12.0, if a.enabled { tint(ACCENT, 0.15) } else { HAIRLINE });
                            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, (pos + 1).to_string(), sbf(12.0), if a.enabled { ACCENT_DARK } else { SUBTLE });
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.label(RichText::new(&a.name).font(sbf(14.0)).color(if a.enabled { HEADING } else { SUBTLE }));
                                ui.label(RichText::new(format!("{} · {}", a.provider.label(), a.model)).font(mdf(12.3)).color(MUTED));
                            });
                            right_aligned(ui, |ui| {
                                if ui.small_button("Editar").clicked() {
                                    action = Some((a.id, if editing { "close" } else { "edit" }));
                                }
                                let busy = testing.contains(&a.id);
                                if ui.add_enabled(!busy, egui::Button::new("Probar").small()).on_hover_text("Envía una pregunta corta para comprobar la clave y el modelo").clicked() {
                                    action = Some((a.id, "test"));
                                }
                                if busy {
                                    ui.spinner();
                                } else if let Some(r) = app.ia.tests.get(&a.id) {
                                    match r {
                                        Ok(m) => ui.label(RichText::new(format!("✔ {m}")).font(mdf(12.0)).color(OK)),
                                        Err(e) => ui.label(RichText::new(format!("✖ {e}")).font(mdf(12.0)).color(DANGER)).on_hover_text(e),
                                    };
                                } else if a.provider.needs_key() && a.api_key.trim().is_empty() {
                                    pill(ui, "Falta la clave", WARN);
                                }
                                free_pill(ui, a.free);
                                let mut on = a.enabled;
                                if ui.checkbox(&mut on, "").on_hover_text("Activo").changed() {
                                    action = Some((a.id, if on { "on" } else { "off" }));
                                }
                                if ui.add_enabled(pos + 1 < ids.len(), egui::Button::new("↓").small()).on_hover_text("Bajar en el orden").clicked() {
                                    action = Some((a.id, "down"));
                                }
                                if ui.add_enabled(pos > 0, egui::Button::new("↑").small()).on_hover_text("Subir en el orden").clicked() {
                                    action = Some((a.id, "up"));
                                }
                            });
                        });
                        if editing {
                            ui.add_space(6.0);
                            agent_editor(app, ui, ctx, a.id);
                        }
                    });
                ui.add_space(4.0);
            }
        }
        if let Some((id, what)) = action {
            let agents = &mut app.db.ia.agents;
            match what {
                "edit" => {
                    app.ia.edit = Some(id);
                    app.ia.models = None;
                }
                "close" => app.ia.edit = None,
                "on" | "off" => {
                    if let Some(a) = agents.iter_mut().find(|a| a.id == id) {
                        a.enabled = what == "on";
                    }
                }
                "up" | "down" => {
                    // intercambia con el anterior/siguiente del mismo grupo (gratuito o de pago)
                    if let Some(i) = agents.iter().position(|a| a.id == id) {
                        let free = agents[i].free;
                        let other = if what == "up" { (0..i).rev().find(|&j| agents[j].free == free) } else { (i + 1..agents.len()).find(|&j| agents[j].free == free) };
                        if let Some(j) = other {
                            agents.swap(i, j);
                        }
                    }
                }
                _ => {
                    if let Some(a) = agents.iter().find(|a| a.id == id).cloned() {
                        start_test(app, ctx, a);
                    }
                }
            }
        }
        ui.add_space(6.0);
        ui.collapsing("Opciones avanzadas", |ui| {
            let cfg = &mut app.db.ia;
            ui.horizontal(|ui| {
                ui.label("Esperar cada respuesta hasta");
                ui.add(egui::DragValue::new(&mut cfg.timeout_secs).range(20..=300).suffix(" s"));
            });
            ui.checkbox(&mut cfg.send_age, "Enviar la edad del paciente (ayuda a distinguir la dentición temporal)");
            ui.label(RichText::new("Indicaciones para todos los análisis (opcional)").font(mdf(12.8)).color(MUTED));
            ui.add(egui::TextEdit::multiline(&mut cfg.extra).desired_rows(2).desired_width(f32::INFINITY).hint_text("Por ejemplo: fíjate especialmente en caries interproximales y en el nivel óseo."));
        });
    });
}

fn agent_editor(app: &mut App, ui: &mut Ui, ctx: &egui::Context, id: u32) {
    let Some(i) = app.db.ia.agents.iter().position(|a| a.id == id) else { return };
    let mut remove = false;
    let mut want_models = false;
    let mut want_test = false;
    {
        let show_key = &mut app.ia.show_key;
        let a = &mut app.db.ia.agents[i];
        ui.horizontal_wrapped(|ui| {
            field(ui, "Nombre", &mut a.name, 220.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Proveedor").font(mdf(12.8)).color(MUTED));
                let old = a.provider;
                crate::theme::combo(("ia_prov", id)).width(220.0).selected_text(a.provider.label()).show_ui(ui, |ui| {
                    for p in Provider::ALL {
                        ui.selectable_value(&mut a.provider, p, p.label());
                    }
                });
                if a.provider != old && (a.base_url.trim().is_empty() || a.base_url == old.default_url()) {
                    a.base_url = a.provider.default_url().into();
                    if let Some((m, f)) = a.provider.suggested().first() {
                        a.model = (*m).into();
                        a.free = *f;
                    }
                }
            });
            field(ui, "Dirección del servicio", &mut a.base_url, 300.0);
        });
        muted(ui, a.provider.note());
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(if a.provider.needs_key() { "Clave (API key)" } else { "Clave (si el servicio la pide)" }).font(mdf(12.8)).color(MUTED));
                ui.horizontal(|ui| {
                    ui.add_sized([300.0, 36.0], egui::TextEdit::singleline(&mut a.api_key).password(!*show_key).hint_text("Pegue aquí la clave"));
                    ui.toggle_value(show_key, if *show_key { "Ocultar" } else { "Ver" });
                    if !a.provider.key_url().is_empty() {
                        ui.hyperlink_to("Conseguir una clave ↗", a.provider.key_url());
                    }
                });
            });
        });
        ui.horizontal_wrapped(|ui| {
            field(ui, "Modelo (debe admitir imágenes)", &mut a.model, 330.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Sugeridos").font(mdf(12.8)).color(MUTED));
                crate::theme::combo(("ia_sug", id)).width(240.0).selected_text("Elegir…").show_ui(ui, |ui| {
                    for (m, f) in a.provider.suggested() {
                        if ui.selectable_label(a.model == *m, format!("{m}{}", if *f { "  · gratuito" } else { "" })).clicked() {
                            a.model = (*m).into();
                            a.free = *f;
                        }
                    }
                });
            });
            ui.vertical(|ui| {
                ui.label(" ");
                if ui.button("Ver modelos del proveedor").on_hover_text("Consulta la lista actual (en OpenRouter indica cuáles son gratuitos y cuáles admiten imágenes)").clicked() {
                    want_models = true;
                }
            });
        });
        muted(ui, "Los proveedores cambian sus modelos a menudo: si uno deja de existir, elija otro con «Ver modelos del proveedor».");
        ui.horizontal(|ui| {
            ui.label(RichText::new("Tipo").font(mdf(12.8)).color(MUTED));
            ui.radio_value(&mut a.free, true, "Gratuito");
            ui.radio_value(&mut a.free, false, "De pago");
            ui.add_space(12.0);
            ui.checkbox(&mut a.enabled, "Activo");
        });
    }
    // lista de modelos del proveedor
    if app.ia.models_job.as_ref().map(|m| m.0) == Some(id) {
        ui.horizontal(|ui| {
            ui.spinner();
            muted(ui, "Consultando la lista de modelos…");
        });
    }
    let mut pick: Option<(String, Option<bool>)> = None;
    if let Some((mid, res)) = &app.ia.models
        && *mid == id
    {
        match res {
            Err(e) => {
                ui.label(RichText::new(format!("No se pudo obtener la lista: {e}")).color(DANGER));
            }
            Ok(list) => {
                ui.horizontal(|ui| {
                    ui.add_sized([220.0, 32.0], egui::TextEdit::singleline(&mut app.ia.model_filter).hint_text("Filtrar modelos"));
                    ui.checkbox(&mut app.ia.only_free, "Sólo gratuitos");
                    ui.checkbox(&mut app.ia.all_models, "Mostrar también los que no admiten imágenes");
                });
                let f = fold(&app.ia.model_filter);
                let shown: Vec<&ia::ModelInfo> =
                    list.iter().filter(|m| f.is_empty() || fold(&m.id).contains(&f) || fold(&m.name).contains(&f)).filter(|m| !app.ia.only_free || m.free == Some(true)).filter(|m| app.ia.all_models || m.vision != Some(false)).collect();
                muted(ui, format!("{} modelos", shown.len()));
                egui::ScrollArea::vertical().id_salt(("ia_models", id)).max_height(220.0).show(ui, |ui| {
                    for m in shown.iter().take(300) {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(false, RichText::new(&m.id).font(mdf(13.0))).clicked() {
                                pick = Some((m.id.clone(), m.free));
                            }
                            match m.free {
                                Some(true) => pill(ui, "Gratuito", OK),
                                Some(false) => pill(ui, "De pago", INFO),
                                None => pill(ui, "Precio no indicado", MUTED),
                            };
                            if m.vision == Some(true) {
                                pill(ui, "Imágenes", ACCENT);
                            }
                        });
                    }
                });
            }
        }
    }
    if let Some((m, free)) = pick {
        let a = &mut app.db.ia.agents[i];
        a.model = m.clone();
        if let Some(f) = free {
            a.free = f;
        }
        if a.name.is_empty() || a.name.contains(" · ") {
            a.name = format!("{} · {}", a.provider.label().split(' ').next().unwrap_or(""), ia::short_model(&m));
        }
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if primary(ui, "Probar ahora").clicked() {
            want_test = true;
        }
        if ui.button("Cerrar").clicked() {
            app.ia.edit = None;
        }
        right_aligned(ui, |ui| {
            if danger(ui, "Quitar agente").clicked() {
                remove = true;
            }
        });
    });
    let a = app.db.ia.agents[i].clone();
    if want_models {
        start_models(app, ctx, a.clone());
    }
    if want_test {
        start_test(app, ctx, a);
    }
    if remove {
        app.db.ia.agents.retain(|a| a.id != id);
        app.ia.edit = None;
    }
}

// ───────────────────────────── visor de imágenes ─────────────────────────────

/// Tratamiento del catálogo para una propuesta (código y descripción).
fn treatment_for(db: &Db, p: &ia::Proposal) -> Option<(String, String)> {
    let found = match ia::proposal_kind(&p.kind) {
        Some(OdgKind::Obturacion) => {
            // la obturación que corresponde al número de caras, si el catálogo las distingue
            let list: Vec<&Treatment> = db.treatments.iter().filter(|t| t.active && t.odg == OdgKind::Obturacion).collect();
            let n = p.faces.len().max(1);
            let want = if n == 1 { "1 cara".to_string() } else { format!("{n} caras") };
            list.iter().find(|t| fold(&t.description).contains(&want)).or(list.first()).copied()
        }
        Some(k) => db.treatments.iter().find(|t| t.active && t.odg == k),
        None => {
            let words = ia::proposal_words(&p.kind);
            db.treatments.iter().find(|t| t.active && words.iter().any(|w| fold(&t.description).contains(&fold(w))))
        }
    };
    found.map(|t| (t.code.clone(), t.description.clone()))
}

/// Panel «Análisis con IA» bajo el visor de una imagen.
pub fn image_panel(app: &mut App, ui: &mut Ui, img: &PatientImage) {
    poll(app);
    let ctx = ui.ctx().clone();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "✨ Análisis con IA");
            pill(ui, "Orientativo", WARN);
        });
        if !app.db.ia.enabled {
            ui.horizontal_wrapped(|ui| {
                muted(ui, "El análisis de imágenes con inteligencia artificial está desactivado.");
                if app.allowed(Screen::Config) && ui.button("Configurar").clicked() {
                    crate::screens::config::open(app, crate::screens::config::CfgTab::Ia);
                }
            });
        } else {
            analyze_controls(app, ui, img, &ctx);
        }
        saved_reports(app, ui, img);
    });
}

fn analyze_controls(app: &mut App, ui: &mut Ui, img: &PatientImage, ctx: &egui::Context) {
    {
        let analyzable = crate::screens::imagenes::analyzable(img);
        if let Err(why) = &analyzable {
            muted(ui, why.clone());
        }
        let busy_here = app.ia.job.as_ref().map(|j| j.image_id == img.id).unwrap_or(false);
        let busy_other = app.ia.job.is_some() && !busy_here;
        let mut mode = app.ia.mode.unwrap_or(app.db.ia.mode);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Usar").font(mdf(13.0)).color(MUTED));
            for m in Mode::ALL {
                let n = ia::chain(&app.db.ia, m).len();
                if ui.selectable_label(mode == m, format!("{} ({n})", m.short())).on_hover_text(m.label()).clicked() {
                    mode = m;
                }
            }
            ui.add_space(8.0);
            let can = analyzable.is_ok() && app.ia.job.is_none();
            if ui.add_enabled(can, egui::Button::new(RichText::new("✨ Analizar esta imagen").font(sbf(14.0)).color(Color32::WHITE)).fill(ACCENT).min_size(vec2(0.0, 36.0))).clicked() {
                start_analysis(app, ctx, img, mode);
            }
        });
        if mode != app.db.ia.mode || app.ia.mode.is_some() {
            app.ia.mode = Some(mode);
        }
        muted(ui, "Se envía sólo la imagen, sin nombre ni datos del paciente.");
        if busy_here {
            if let Some(j) = &app.ia.job {
                let msg = j.progress.lock().map(|g| g.clone()).unwrap_or_default();
                let secs = j.started.elapsed().as_secs();
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(format!("{msg}  ·  {secs} s")).font(mdf(13.0)).color(TEXT));
                });
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            }
        } else if busy_other {
            muted(ui, "Hay otro análisis en marcha; espere a que termine.");
        }
        if let Some((iid, e, attempts)) = &app.ia.error
            && *iid == img.id
        {
            egui::Frame::new().fill(tint(DANGER, 0.06)).stroke(Stroke::new(1.0, tint(DANGER, 0.25))).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(e).font(sbf(13.5)).color(DANGER));
                attempts_list(ui, attempts);
            });
        }
    }
}

/// Análisis guardados de una imagen (el más reciente, abierto).
fn saved_reports(app: &mut App, ui: &mut Ui, img: &PatientImage) {
    {
        let mut reports: Vec<IaReport> = app.db.ia_reports.iter().filter(|r| r.image_id == img.id).cloned().collect();
        reports.sort_by_key(|r| std::cmp::Reverse(r.when));
        for (k, r) in reports.iter().enumerate() {
            ui.add_space(8.0);
            let head = format!("{} · {} · {} hallazgos", r.when.format("%d/%m/%Y %H:%M"), r.agent, r.analysis.findings.len());
            egui::CollapsingHeader::new(RichText::new(head).font(sbf(13.5))).id_salt(("ia_rep", r.id)).default_open(k == 0).show(ui, |ui| {
                report_view(app, ui, r, img);
            });
        }
    }
}

fn attempts_list(ui: &mut Ui, attempts: &[ia::Attempt]) {
    for (i, t) in attempts.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("{}.", i + 1)).font(mdf(12.3)).color(SUBTLE));
            ui.label(RichText::new(&t.agent).font(sbf(12.3)).color(TEXT));
            free_pill(ui, t.free);
            ui.label(RichText::new(if t.ok { "respondió".to_string() } else { t.message.clone() }).font(mdf(12.3)).color(if t.ok { OK } else { DANGER }));
            ui.label(RichText::new(format!("{:.1} s", t.millis as f32 / 1000.0)).font(mdf(12.0)).color(SUBTLE));
        });
    }
}

fn severity_color(s: &str) -> Color32 {
    match s {
        s if s.starts_with("alt") => DANGER,
        s if s.starts_with("med") => WARN,
        _ => OK,
    }
}

fn report_view(app: &mut App, ui: &mut Ui, r: &IaReport, img: &PatientImage) {
    ui.horizontal_wrapped(|ui| {
        free_pill(ui, r.free);
        muted(ui, format!("Modelo {}", r.model));
        let failed = r.attempts.iter().filter(|a| !a.ok).count();
        if failed > 0 {
            let resp = ui.label(RichText::new(format!("respondió al {}.º intento", failed + 1)).font(mdf(12.5)).color(WARN));
            let detail: Vec<String> = r.attempts.iter().map(|a| format!("{}: {}", a.agent, if a.ok { "respondió" } else { &a.message })).collect();
            resp.on_hover_text(detail.join("\n"));
        }
        if !r.user.is_empty() {
            muted(ui, format!("· pedido por {}", r.user));
        }
    });
    let an = &r.analysis;
    if !an.summary.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(&an.summary).font(mdf(14.0)).color(HEADING));
    }
    if !an.quality.is_empty() {
        muted(ui, format!("Calidad de la imagen: {}", an.quality));
    }
    if !an.findings.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new("HALLAZGOS").font(sbf(11.5)).color(SUBTLE));
        for f in &an.findings {
            egui::Frame::new().fill(ROW_ALT).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), egui::Sense::hover());
                    let c = severity_color(&f.severity);
                    ui.painter().circle_filled(rect.center(), 16.0, tint(c, 0.14));
                    ui.painter().circle_stroke(rect.center(), 16.0, Stroke::new(1.2, tint(c, 0.5)));
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, f.tooth.map(|t| t.to_string()).unwrap_or_else(|| "—".into()), sbf(12.5), mix(c, HEADING, 0.3));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(ia::finding_label(&f.kind)).font(sbf(13.5)).color(HEADING));
                            if !f.faces.is_empty() {
                                muted(ui, format!("caras {}", f.faces.join("")));
                            }
                            if !f.severity.is_empty() {
                                pill(ui, &format!("gravedad {}", f.severity), severity_color(&f.severity));
                            }
                            if f.confidence > 0.0 {
                                muted(ui, format!("confianza {:.0} %", f.confidence * 100.0));
                            }
                        });
                        if !f.description.is_empty() {
                            ui.label(RichText::new(&f.description).font(mdf(12.8)).color(MUTED));
                        }
                    });
                });
            });
            ui.add_space(3.0);
        }
    }
    let mut plan = false;
    if !an.proposals.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new("PROPUESTA DE TRATAMIENTO").font(sbf(11.5)).color(SUBTLE));
        for (i, p) in an.proposals.iter().enumerate() {
            let done = r.planned.contains(&i);
            let tx = treatment_for(&app.db, p);
            ui.horizontal_wrapped(|ui| {
                let mut on = !app.ia.unchecked.contains(&(r.id, i)) && !done && tx.is_some();
                if ui.add_enabled(!done && tx.is_some(), egui::Checkbox::without_text(&mut on)).changed() {
                    if on {
                        app.ia.unchecked.remove(&(r.id, i));
                    } else {
                        app.ia.unchecked.insert((r.id, i));
                    }
                }
                let teeth: Vec<String> = p.teeth.iter().map(|t| t.to_string()).collect();
                let mut txt = ia::finding_label(&p.kind);
                if !teeth.is_empty() {
                    txt += &format!(" · {}", teeth.join(", "));
                }
                if !p.faces.is_empty() {
                    txt += &format!(" ({})", p.faces.join(""));
                }
                ui.label(RichText::new(txt).font(sbf(13.5)).color(HEADING));
                match &tx {
                    Some((code, desc)) => muted(ui, format!("→ {code} {desc}")),
                    None => muted(ui, "→ sin tratamiento equivalente en el catálogo"),
                }
                if done {
                    pill(ui, "En el plan", OK);
                }
            });
            if !p.reason.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.label(RichText::new(&p.reason).font(mdf(12.5)).color(MUTED));
                });
            }
        }
        ui.add_space(4.0);
        let any = an.proposals.iter().enumerate().any(|(i, p)| !r.planned.contains(&i) && !app.ia.unchecked.contains(&(r.id, i)) && treatment_for(&app.db, p).is_some());
        ui.horizontal(|ui| {
            if ui
                .add_enabled(any, egui::Button::new(RichText::new("Añadir al plan de tratamiento").color(Color32::WHITE)).fill(ACCENT))
                .on_hover_text("Se añaden como «previsto» al historial y al odontograma; luego puede hacer el presupuesto")
                .clicked()
            {
                plan = true;
            }
        });
    }
    let mut copy = false;
    let mut delete = false;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("Copiar el resumen a las notas de la imagen").clicked() {
            copy = true;
        }
        right_aligned(ui, |ui| {
            if ui.small_button("Borrar este análisis").clicked() {
                delete = true;
            }
        });
    });
    ui.label(RichText::new("Resultado generado por inteligencia artificial: puede equivocarse. El diagnóstico y el plan los decide el dentista.").font(mdf(11.8)).color(SUBTLE));
    if plan {
        let doctor = app.db.patient(r.patient_id).map(|p| p.doctor_id).unwrap_or(1);
        let mut added = vec![];
        for (i, p) in an.proposals.iter().enumerate() {
            if r.planned.contains(&i) || app.ia.unchecked.contains(&(r.id, i)) {
                continue;
            }
            if let Some((code, desc)) = treatment_for(&app.db, p) {
                let faces = if ia::proposal_kind(&p.kind) == Some(OdgKind::Obturacion) { p.faces.clone() } else { vec![] };
                app.db.add_entry(r.patient_id, &code, p.teeth.clone(), faces, TxState::Previsto, doctor, app.today);
                added.push((i, desc));
            }
        }
        if let Some(rep) = app.db.ia_reports.iter_mut().find(|x| x.id == r.id) {
            rep.planned.extend(added.iter().map(|a| a.0));
        }
        let names: Vec<String> = added.iter().map(|a| a.1.clone()).collect();
        app.db.audit("Plan desde análisis con IA", &format!("{} · {}", app.db.patient_name(r.patient_id), names.join(", ")));
        app.changed();
        app.notify(format!("{} tratamientos añadidos al plan como «previsto».", added.len()));
    }
    if copy {
        if let Some(m) = app.db.images.iter_mut().find(|i| i.id == img.id) {
            let note = format!("IA ({}): {}", r.when.format("%d/%m/%Y"), an.summary);
            m.notes = if m.notes.trim().is_empty() { note } else { format!("{} · {note}", m.notes.trim()) };
        }
        app.changed();
        app.notify("Resumen copiado a las notas de la imagen.");
    }
    if delete {
        app.db.ia_reports.retain(|x| x.id != r.id);
        app.changed();
    }
}
