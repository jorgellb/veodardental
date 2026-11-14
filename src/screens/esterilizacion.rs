//! Almacén › Esterilización (ver `crate::esterilizacion`): ciclos y lotes,
//! uso de un lote en un paciente, incidencias y pacientes afectados.

use crate::app::App;
use crate::esterilizacion::{CycleState, affected_csv};
use crate::theme::*;
use egui::{RichText, Ui};

/// Lo que se está escribiendo en la pantalla.
#[derive(Clone, Default)]
struct Form {
    autoclave: String,
    program: String,
    packs: String,
    days: u32,
    code: String,
    patient: String,
    incident: String,
}

pub fn panel(app: &mut App, ui: &mut Ui) {
    let key = ui.make_persistent_id("esterilizacion");
    let mut f: Form = ui.data_mut(|d| d.get_temp(key)).unwrap_or(Form { autoclave: "Autoclave 1".into(), program: "134 ºC".into(), days: 180, ..Default::default() });
    let now = crate::model::now();
    // nuevo ciclo
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Ciclo nuevo");
        ui.horizontal_wrapped(|ui| {
            ui.label("Autoclave");
            ui.add(egui::TextEdit::singleline(&mut f.autoclave).desired_width(140.0));
            ui.label("Programa");
            ui.add(egui::TextEdit::singleline(&mut f.program).desired_width(120.0));
            ui.label("Caducidad");
            ui.add(egui::DragValue::new(&mut f.days).range(1..=365).suffix(" días"));
        });
        ui.add(egui::TextEdit::multiline(&mut f.packs).desired_rows(2).desired_width(f32::INFINITY).hint_text("Un lote por línea, con su contenido (p. ej. «Kit de extracción»)"));
        if primary(ui, "Registrar ciclo y etiquetas").clicked() {
            let packs: Vec<String> = f.packs.lines().map(String::from).collect();
            match app.db.new_cycle(&f.autoclave, &f.program, &packs, f.days, now) {
                Ok(_) => {
                    f.packs.clear();
                    app.changed();
                    app.notify("Ciclo registrado: valídelo con los indicadores al terminar.");
                }
                Err(e) => app.warn(e),
            }
        }
    });
    ui.add_space(8.0);
    // usar un lote
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Usar un lote en un paciente");
        ui.horizontal_wrapped(|ui| {
            ui.label("Etiqueta del lote");
            ui.add(egui::TextEdit::singleline(&mut f.code).desired_width(150.0).hint_text("AUTOCLAVE1-0001-01"));
            ui.label("Nº de historia");
            ui.add(egui::TextEdit::singleline(&mut f.patient).desired_width(90.0));
            if let Some(p) = app.db.pack_problem(&f.code, app.today).filter(|_| !f.code.trim().is_empty()) {
                ui.label(RichText::new(p).color(DANGER));
            }
            if ui.button("Registrar uso").clicked() {
                let pid = f.patient.trim().parse::<u32>().ok().and_then(|n| app.db.patients.iter().find(|p| p.history_no == n).map(|p| p.id));
                match pid {
                    Some(pid) => match app.db.use_pack(&f.code, pid, None, None, now) {
                        Ok(()) => {
                            app.changed();
                            app.notify(format!("Lote {} usado en {}.", f.code.trim(), app.db.patient_name(pid)));
                            f.code.clear();
                        }
                        Err(e) => app.warn(e),
                    },
                    None => app.warn("No hay ningún paciente con ese número de historia."),
                }
            }
        });
    });
    ui.add_space(8.0);
    // ciclos
    let mut cycles: Vec<crate::esterilizacion::Cycle> = app.db.sterile_cycles.iter().cloned().collect();
    cycles.sort_by_key(|c| std::cmp::Reverse(c.started));
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Ciclos");
        if cycles.is_empty() {
            muted(ui, "Aún no hay ciclos registrados.");
        }
        for c in cycles.iter().take(60) {
            let packs: Vec<&crate::esterilizacion::Pack> = app.db.sterile_packs.iter().filter(|p| p.cycle_id == c.id).collect();
            let used = packs.iter().filter(|p| p.used.is_some()).count();
            ui.horizontal_wrapped(|ui| {
                pill(
                    ui,
                    c.state.label(),
                    match c.state {
                        CycleState::Validado => OK,
                        CycleState::Fallido => DANGER,
                        CycleState::EnCurso => WARN,
                    },
                );
                ui.label(format!("{} nº {} · {} · {} · {} lotes ({used} usados)", c.autoclave, c.number, c.program, c.started.format("%d/%m/%Y %H:%M"), packs.len()));
                if !c.incident.is_empty() {
                    ui.label(RichText::new(&c.incident).color(DANGER));
                }
            });
            let id = c.id;
            ui.horizontal_wrapped(|ui| {
                if c.state == CycleState::EnCurso {
                    if ui.small_button("Indicadores correctos").clicked() {
                        let r = app.db.validate_cycle(id, true, Some(true), now).map(|_| vec![]);
                        report(app, r);
                    }
                    if ui.small_button("Indicador químico o Bowie-Dick no conforme").clicked() {
                        let r = app.db.validate_cycle(id, false, None, now).map(|_| app.db.affected_by_cycle(id));
                        report(app, r);
                    }
                }
                if c.state == CycleState::Validado && c.biological.is_none() {
                    if ui.small_button("Biológico negativo (correcto)").clicked() {
                        let r = app.db.biological_result(id, true, now);
                        report(app, r);
                    }
                    if ui.small_button("Biológico positivo").on_hover_text("Ciclo fallido: se retiran sus lotes sin usar y salen los pacientes afectados").clicked() {
                        let r = app.db.biological_result(id, false, now);
                        report(app, r);
                    }
                }
                if c.state != CycleState::Fallido && ui.small_button("Notificar incidencia…").clicked() {
                    f.incident = format!("{id}");
                }
                if c.state == CycleState::Fallido && used > 0 && ui.small_button("Exportar afectados (CSV)").clicked() {
                    let list = app.db.affected_by_cycle(id);
                    let dir = crate::paths::documents().join(crate::brand::DIR);
                    let p = dir.join(format!("afectados_ciclo_{}_{}.csv", c.autoclave.replace(' ', "_"), c.number));
                    // con nombres y teléfonos de pacientes: solo para el usuario
                    match std::fs::create_dir_all(&dir).and_then(|_| crate::paths::write_private(&p, affected_csv(&app.db, &list).as_bytes())) {
                        Ok(()) => app.notify(format!("Exportado en {}", p.display())),
                        Err(e) => app.warn(format!("No se pudo exportar: {e}")),
                    }
                }
            });
            if f.incident.split('|').next() == Some(&id.to_string()) {
                ui.horizontal(|ui| {
                    let mut why = f.incident.split_once('|').map(|x| x.1.to_string()).unwrap_or_default();
                    ui.add(egui::TextEdit::singleline(&mut why).hint_text("Qué ha pasado").desired_width(320.0));
                    f.incident = format!("{id}|{why}");
                    if danger(ui, "Marcar el ciclo como fallido").clicked() && !why.trim().is_empty() {
                        let r = app.db.cycle_incident(id, &why, now);
                        report(app, r);
                        f.incident.clear();
                    }
                });
            }
            if c.state == CycleState::Fallido {
                for a in app.db.affected_by_cycle(id) {
                    ui.label(RichText::new(format!("  Afectado: {} · lote {} · {} {}", app.db.patient_name(a.patient_id), a.pack, a.at.format("%d/%m/%Y %H:%M"), a.treatment)).color(DANGER));
                }
            }
            ui.separator();
        }
    });
    ui.data_mut(|d| d.insert_temp(key, f));
}

fn report(app: &mut App, r: Result<Vec<crate::esterilizacion::Affected>, String>) {
    match r {
        Ok(list) if list.is_empty() => app.notify("Ciclo actualizado."),
        Ok(list) => app.warn(format!("Ciclo fallido: {} paciente(s) afectados. La lista está en el ciclo.", list.len())),
        Err(e) => app.warn(e),
    }
    app.changed();
}

/// En la ficha (Historial): los lotes esterilizados usados en el paciente.
pub fn patient_card(app: &mut App, ui: &mut Ui, pid: u32) {
    let packs: Vec<(String, String, String, CycleState)> = app
        .db
        .packs_of_patient(pid)
        .into_iter()
        .map(|p| {
            let when = p.used.as_ref().map(|u| u.at.format("%d/%m/%Y %H:%M").to_string()).unwrap_or_default();
            let state = app.db.cycle(p.cycle_id).map(|c| c.state).unwrap_or_default();
            (p.code.clone(), p.contents.clone(), when, state)
        })
        .collect();
    if packs.is_empty() {
        return;
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Instrumental esterilizado usado");
        for (code, contents, when, state) in packs {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{when} · lote {code} · {contents}"));
                if state == CycleState::Fallido {
                    pill(ui, "Ciclo fallido: paciente afectado", DANGER);
                }
            });
        }
    });
    ui.add_space(8.0);
}
