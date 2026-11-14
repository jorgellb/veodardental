//! Notas de evolución en la ficha (pestaña Historial): plantilla, dictado
//! desde una grabación, resumen con la IA, revisión y confirmación (ver
//! `crate::evolucion`).

use crate::app::App;
use crate::evolucion::{Origin, Provenance, unsupported};
use crate::theme::*;
use egui::{RichText, Ui};
use std::sync::mpsc::Receiver;

/// Resultado de un dictado o de un resumen: (origen, texto, servicio o modelo).
pub type JobResult = Result<(Origin, String, String), String>;

/// Estado de la pantalla: el borrador abierto y lo que está en curso.
#[derive(Default)]
pub struct EvoUi {
    /// Borrador que se está escribiendo.
    pub draft: Option<u32>,
    /// Ruta de la grabación que se va a transcribir.
    pub audio_path: String,
    /// Borrar la grabación al terminar.
    pub delete_audio: bool,
    /// Transcripción del dictado (solo en memoria: no se guarda).
    pub transcript: String,
    /// Propuesta de la IA y lo que menciona sin estar en el dictado.
    pub proposal: Option<(String, String, Vec<String>)>,
    pub reviewed: bool,
    pub job: Option<Receiver<JobResult>>,
}

fn now() -> chrono::NaiveDateTime {
    crate::model::now()
}

/// El recuadro de notas de evolución del paciente.
pub fn card_for(app: &mut App, ui: &mut Ui, pid: u32) {
    if !app.can(crate::users::Perm::Clinico) {
        return;
    }
    poll(app);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Notas de evolución");
            right_aligned(ui, |ui| {
                if app.evo.draft.is_none() && primary(ui, "＋ Nota nueva").clicked() {
                    let doctor = app.db.patient(pid).map(|p| p.doctor_id).unwrap_or(1);
                    let id = app.db.evo_draft(pid, doctor, None, "", vec![Provenance { origin: Origin::Escrito, detail: String::new(), at: now(), source_hash: String::new() }], now());
                    app.evo = crate::screens::evolucion::EvoUi { draft: Some(id), ..Default::default() };
                    app.changed();
                }
            });
        });
        if let Some(id) = app.evo.draft.filter(|id| app.db.evo_notes.iter().any(|n| n.id == *id && n.patient_id == pid && !n.is_confirmed())) {
            editor(app, ui, id);
            ui.separator();
        }
        let mut notes: Vec<crate::evolucion::EvoNote> = app.db.evo_notes.iter().filter(|n| n.patient_id == pid && n.is_confirmed()).cloned().collect();
        notes.sort_by_key(|n| std::cmp::Reverse(n.confirmed.as_ref().map(|c| c.0)));
        if notes.is_empty() {
            muted(ui, "Sin notas de evolución confirmadas.");
        }
        for n in notes.iter().take(30) {
            ui.horizontal_wrapped(|ui| {
                let (at, who) = n.confirmed.clone().unwrap_or_default();
                ui.label(RichText::new(format!("{} · {}", at.format("%d/%m/%Y %H:%M"), who)).font(sbf(13.0)));
                for p in &n.provenance {
                    if p.origin != Origin::Escrito {
                        pill(ui, &if p.detail.is_empty() { p.origin.label().to_string() } else { format!("{}: {}", p.origin.label(), p.detail) }, if p.origin == Origin::ResumenIa { VIOLET } else { INFO });
                    }
                }
                if let Some(c) = n.corrects {
                    pill(ui, &format!("Corrige la nota #{c}"), WARN);
                }
                if app.evo.draft.is_none() && ui.small_button("Corregir").on_hover_text("Una nota confirmada no se cambia: se escribe una corrección que la cita").clicked() {
                    match app.db.evo_correct(n.id, now()) {
                        Ok(d) => {
                            app.evo = EvoUi { draft: Some(d), ..Default::default() };
                            app.changed();
                        }
                        Err(e) => app.warn(e),
                    }
                }
            });
            ui.label(&n.text);
            ui.add_space(4.0);
        }
    });
}

fn poll(app: &mut App) {
    let Some(rx) = app.evo.job.take() else { return };
    match rx.try_recv() {
        Ok(Ok((Origin::Dictado, text, detail))) => {
            // al borrador, como dictado; el texto del dictado queda solo en memoria
            if let Some(id) = app.evo.draft {
                let cur = app.db.evo_notes.iter().find(|n| n.id == id).map(|n| n.text.clone()).unwrap_or_default();
                let joined = if cur.trim().is_empty() { text.clone() } else { format!("{cur}\n{text}") };
                let p = Provenance { origin: Origin::Dictado, detail, at: now(), source_hash: sha(&text) };
                match app.db.evo_edit(id, &joined, Some(p)) {
                    Ok(()) => app.changed(),
                    Err(e) => app.warn(e),
                }
            }
            app.evo.transcript = text;
            app.evo.reviewed = false;
            app.notify("Dictado transcrito: revíselo antes de confirmar la nota.");
        }
        Ok(Ok((_, text, model))) => {
            let drugs: Vec<String> = app.db.drugs.iter().map(|d| d.name.clone()).collect();
            let flagged = unsupported(&text, &app.evo.transcript, &drugs);
            app.evo.proposal = Some((text, model, flagged));
            app.evo.reviewed = false;
        }
        Ok(Err(e)) => app.warn(e),
        Err(std::sync::mpsc::TryRecvError::Empty) => app.evo.job = Some(rx),
        Err(_) => app.warn("La tarea se interrumpió."),
    }
}

fn sha(text: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn editor(app: &mut App, ui: &mut Ui, id: u32) {
    let Some(n) = app.db.evo_notes.iter().find(|n| n.id == id).cloned() else { return };
    if let Some(c) = n.corrects {
        ui.label(RichText::new(format!("Corrección de la nota #{c}: la nota original se conserva.")).color(WARN));
    }
    // plantilla del tratamiento
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Plantilla").color(MUTED));
        let today: Vec<(u32, crate::model::OdgKind, Vec<u8>)> = app.db.entries.iter().filter(|e| e.patient_id == n.patient_id && e.date == app.today).map(|e| (e.id, e.odg, e.teeth.clone())).collect();
        let templates = app.db.config.evo_templates.clone();
        for t in &templates {
            // primero las de los tratamientos de hoy
            let hit = today.iter().find(|(_, k, _)| t.kinds.contains(k));
            if ui.small_button(if hit.is_some() { format!("★ {}", t.name) } else { t.name.clone() }).clicked() {
                let text = crate::evolucion::fill_template(t, hit.map(|h| h.2.as_slice()).unwrap_or(&[]));
                let joined = if n.text.trim().is_empty() { text } else { format!("{}\n{text}", n.text) };
                let p = Provenance { origin: Origin::Plantilla, detail: t.name.clone(), at: now(), source_hash: String::new() };
                if app.db.evo_edit(id, &joined, Some(p)).is_ok() {
                    if let (Some(h), Some(x)) = (hit, app.db.evo_notes.iter_mut().find(|x| x.id == id)) {
                        x.entry_id = Some(h.0);
                    }
                    app.changed();
                }
            }
        }
    });
    let mut text = n.text.clone();
    if ui.add(egui::TextEdit::multiline(&mut text).desired_rows(5).desired_width(f32::INFINITY).hint_text("Escriba la nota, use una plantilla o dicte. Los huecos «___» hay que completarlos.")).changed() && app.db.evo_edit(id, &text, None).is_ok() {
        app.changed();
    }
    // dictado desde una grabación
    let stt = crate::evolucion::load_stt();
    ui.horizontal_wrapped(|ui| {
        if stt.enabled() {
            ui.label(RichText::new("Dictado").color(MUTED));
            ui.add(egui::TextEdit::singleline(&mut app.evo.audio_path).hint_text("Ruta de la grabación (wav, mp3, m4a…)").desired_width(320.0));
            ui.checkbox(&mut app.evo.delete_audio, "Borrar la grabación al terminar");
            if app.evo.job.is_none() && ui.button("Transcribir").clicked() {
                let path = std::path::PathBuf::from(app.evo.audio_path.trim());
                let del = app.evo.delete_audio;
                let s = stt.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let out = std::fs::read(&path).map_err(|e| format!("No se puede leer la grabación: {e}")).and_then(|audio| {
                        let name = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                        crate::evolucion::transcribe(&s, &audio, &name)
                    });
                    if out.is_ok() && del {
                        let _ = std::fs::remove_file(&path);
                    }
                    let _ = tx.send(out.map(|t| (Origin::Dictado, t, s.label())));
                });
                app.evo.job = Some(rx);
            }
        } else {
            muted(ui, "Dictado: configure el servicio de transcripción en Configuración › IA.");
        }
        if app.evo.job.is_some() {
            ui.spinner();
        }
    });
    // resumen con la IA, solo si la clínica lo ha activado
    if app.db.config.evo_ai
        && !app.evo.transcript.is_empty()
        && app.evo.job.is_none()
        && app.evo.proposal.is_none()
        && ui.button("Proponer resumen con IA").on_hover_text("Se envía el dictado sin el nombre del paciente; el resumen es una propuesta que usted revisa").clicked()
    {
        match crate::ia::chain(&app.db.ia, app.db.ia.mode).into_iter().next() {
            Some(agent) => {
                let p = app.db.patient(n.patient_id).cloned().unwrap_or_default();
                let prompt = crate::evolucion::summary_prompt(&crate::evolucion::without_names(&app.evo.transcript, &p));
                let timeout = app.db.ia.timeout_secs;
                let model = crate::ia::short_model(&agent.model);
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::ia::call(&agent, None, &prompt, timeout).map(|t| (Origin::ResumenIa, t.trim().to_string(), model)));
                });
                app.evo.job = Some(rx);
            }
            None => app.warn("No hay ningún agente de IA activado (Configuración › IA)."),
        }
    }
    if let Some((prop, model, flagged)) = app.evo.proposal.clone() {
        egui::Frame::new().fill(tint(VIOLET, 0.06)).corner_radius(10).inner_margin(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(format!("Propuesta de la IA ({model}): revísela; no está en la historia.")).font(sbf(13.0)));
            ui.label(&prop);
            if !flagged.is_empty() {
                ui.label(RichText::new(format!("Menciona datos que no están en el dictado: {}.", flagged.join(", "))).color(DANGER));
            }
            ui.horizontal(|ui| {
                if ui.button("Usar la propuesta").clicked() {
                    let p = Provenance { origin: Origin::ResumenIa, detail: model.clone(), at: now(), source_hash: sha(&app.evo.transcript) };
                    if app.db.evo_edit(id, &prop, Some(p)).is_ok() {
                        app.changed();
                    }
                }
                if ui.button("Descartarla").clicked() {
                    app.evo.proposal = None;
                }
            });
        });
    }
    let needs_review = n.provenance.iter().any(|p| matches!(p.origin, Origin::Dictado | Origin::ResumenIa));
    if needs_review {
        ui.checkbox(&mut app.evo.reviewed, "He revisado el texto (dictado o propuesta de la IA) y es correcto");
    }
    ui.horizontal(|ui| {
        if primary(ui, "Confirmar y guardar en la historia").clicked() {
            let who = app.db.config.current_user.clone();
            let flagged = if n.uses_ai() { app.evo.proposal.as_ref().map(|p| p.2.clone()).unwrap_or_default() } else { vec![] };
            match app.db.evo_confirm(id, &who, &flagged, app.evo.reviewed, now()) {
                Ok(()) => {
                    // del dictado no queda nada en memoria
                    app.evo = EvoUi::default();
                    app.changed();
                    app.notify("Nota de evolución guardada en la historia.");
                }
                Err(e) => app.warn(e),
            }
        }
        if ui.button("Descartar el borrador").clicked() {
            if app.db.evo_discard(id).is_ok() {
                app.changed();
            }
            app.evo = EvoUi::default();
        }
    });
}

/// Configuración › IA: el servicio de transcripción de este ordenador y el
/// resumen de los dictados con la IA.
pub fn settings(app: &mut App, ui: &mut Ui) {
    let before = app.db.config.evo_ai;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Dictado de notas de evolución");
        muted(ui, "La grabación se transcribe con un servicio compatible con la API de OpenAI (/v1/audio/transcriptions): uno en este ordenador o el que contrate la clínica. El audio no se guarda en el programa.");
        let key = ui.scope_id().with("stt");
        let mut s: crate::evolucion::SttSettings = ui.data_mut(|d| d.get_temp(key)).unwrap_or_else(crate::evolucion::load_stt);
        let mut changed = false;
        egui::Grid::new("stt_cfg").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Dirección del servicio");
            changed |= ui.add(egui::TextEdit::singleline(&mut s.endpoint).hint_text("http://127.0.0.1:8080").desired_width(320.0)).changed();
            ui.end_row();
            ui.label("Modelo");
            changed |= ui.add(egui::TextEdit::singleline(&mut s.model).hint_text("whisper-1").desired_width(200.0)).changed();
            ui.end_row();
            ui.label("Clave (si la pide)");
            changed |= ui.add(egui::TextEdit::singleline(&mut s.key).password(true).desired_width(320.0)).changed();
            ui.end_row();
        });
        if changed && let Err(e) = crate::evolucion::save_stt(&s) {
            app.warn(format!("No se pudo guardar: {e}"));
        }
        ui.data_mut(|d| d.insert_temp(key, s));
        ui.checkbox(&mut app.db.config.evo_ai, "Proponer un resumen de los dictados con la IA (sin el nombre del paciente; el profesional lo revisa)");
        muted(ui, "Antes de activarlo con un proveedor externo, confírmelo con su responsable de protección de datos: el dictado lleva datos de salud.");
    });
    if app.db.config.evo_ai != before {
        app.changed();
    }
}
