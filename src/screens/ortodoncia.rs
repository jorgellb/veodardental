//! Ortodoncia: casos, aparatología, controles periódicos e higiene.

use crate::app::{App, PatTab};
use crate::extra::*;
use crate::model::*;
use crate::theme::*;
use egui::{Color32, RichText, Ui};

pub struct OrthoUi {
    pub selected: Option<u32>,
    pub only_active: bool,
    pub work: String,
    pub wire: String,
    pub hygiene: bool,
    pub weeks: u32,
    pub notes: String,
}

impl Default for OrthoUi {
    fn default() -> Self {
        OrthoUi { selected: None, only_active: true, work: "Cambio de arco".into(), wire: String::new(), hygiene: true, weeks: 5, notes: String::new() }
    }
}

pub fn state_color(s: OrthoState) -> Color32 {
    match s {
        OrthoState::Estudio => MUTED,
        OrthoState::Activo => INFO,
        OrthoState::Retencion => ACCENT,
        OrthoState::Finalizado => OK,
        OrthoState::Abandonado => DANGER,
    }
}

pub fn screen(app: &mut App, ui: &mut Ui) {
    let today = app.today;
    ui.horizontal(|ui| {
        title(ui, "Ortodoncia", "Casos en tratamiento, controles periódicos e higiene");
        right_aligned(ui, |ui| {
            if primary(ui, "＋ Nuevo caso para el paciente activo").clicked() && let Some(pid) = app.patient {
                let id = app.db.new_id();
                app.db.ortho.push(OrthoCase { id, patient_id: pid, doctor_id: 3, start: today, months: 18, appliance: APPLIANCES[0].into(), arches: "Ambas arcadas".into(), class: "Clase I".into(), diagnosis: String::new(), objectives: String::new(), state: OrthoState::Estudio, fee: 0.0, visits: vec![] });
                app.ortho.selected = Some(id);
                app.changed();
            }
        });
    });
    let active: Vec<&OrthoCase> = app.db.ortho.iter().filter(|c| c.state == OrthoState::Activo).collect();
    let late = active.iter().filter(|c| c.next_control().map(|d| d < today).unwrap_or(false)).count();
    let retention = app.db.ortho.iter().filter(|c| c.state == OrthoState::Retencion).count();
    let study = app.db.ortho.iter().filter(|c| c.state == OrthoState::Estudio).count();
    let w = kpi_w(ui, 4);
    ui.horizontal(|ui| {
        kpi(ui, w, "Casos activos", &active.len().to_string(), "con aparatología", INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Controles atrasados", &late.to_string(), "fecha de control superada", DANGER);
        ui.add_space(4.0);
        kpi(ui, w, "En estudio", &study.to_string(), "pendientes de empezar", MUTED);
        ui.add_space(4.0);
        kpi(ui, w, "En retención", &retention.to_string(), "revisiones de retenedor", ACCENT);
    });
    ui.add_space(8.0);
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    let side = 440.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail - side - 24.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if ui.selectable_label(app.ortho.only_active, "En curso").clicked() {
                        app.ortho.only_active = true;
                    }
                    if ui.selectable_label(!app.ortho.only_active, "Todos").clicked() {
                        app.ortho.only_active = false;
                    }
                });
                let mut list: Vec<OrthoCase> = app.db.ortho.iter().filter(|c| !app.ortho.only_active || matches!(c.state, OrthoState::Estudio | OrthoState::Activo | OrthoState::Retencion)).cloned().collect();
                list.sort_by_key(|c| c.next_control().unwrap_or(c.start));
                if list.is_empty() {
                    empty_state(ui, "No hay casos.");
                }
                if app.ortho.selected.is_none() {
                    app.ortho.selected = list.first().map(|c| c.id);
                }
                for c in &list {
                    let sel = app.ortho.selected == Some(c.id);
                    let fr = egui::Frame::new().fill(if sel { Color32::from_rgb(214, 238, 234) } else { ROW_ALT }).corner_radius(egui::CornerRadius::same(10)).inner_margin(egui::Margin::same(12));
                    let resp = fr
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(app.db.patient_name(c.patient_id)).family(sb()).size(15.0));
                                pill(ui, c.state.label(), state_color(c.state));
                                right_aligned(ui, |ui| {
                                    if let Some(n) = c.next_control() {
                                        let late = n < today && c.state == OrthoState::Activo;
                                        ui.label(RichText::new(format!("Próximo control {}", fmt_date(n))).color(if late { DANGER } else { MUTED }).size(13.5));
                                    }
                                });
                            });
                            muted(ui, format!("{} · {} · {}", c.appliance, c.class, c.arches));
                            let pr = c.progress(today);
                            ui.horizontal(|ui| {
                                bar(ui, ui.available_width() - 160.0, pr, state_color(c.state));
                                muted(ui, format!("{:.0}% de {} meses", pr * 100.0, c.months));
                            });
                        })
                        .response
                        .interact(egui::Sense::click());
                    if resp.clicked() {
                        app.ortho.selected = Some(c.id);
                    }
                    ui.add_space(4.0);
                }
            });
        });
        ui.vertical(|ui| {
            ui.set_width(side);
            detail(app, ui);
        });
    });
}

fn detail(app: &mut App, ui: &mut Ui) {
    let Some(id) = app.ortho.selected else { return };
    let today = app.today;
    let mut add = false;
    let mut goto = None;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let Some(c) = app.db.ortho.iter_mut().find(|c| c.id == id) else { return };
        let before = serde_json::to_string(c).unwrap_or_default();
        let pid = c.patient_id;
        ui.horizontal(|ui| {
            section(ui, "Caso de ortodoncia");
            right_aligned(ui, |ui| {
                if ui.link("Ficha del paciente").clicked() {
                    goto = Some(pid);
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("Aparato");
            crate::theme::combo("o_app").width(220.0).selected_text(&c.appliance).show_ui(ui, |ui| {
                for a in APPLIANCES {
                    ui.selectable_value(&mut c.appliance, a.to_string(), a);
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("Clase");
            crate::theme::combo("o_cls").width(170.0).selected_text(&c.class).show_ui(ui, |ui| {
                for a in ["Clase I", "Clase II división 1", "Clase II división 2", "Clase III"] {
                    ui.selectable_value(&mut c.class, a.to_string(), a);
                }
            });
            ui.label("Meses");
            ui.add(egui::DragValue::new(&mut c.months).range(1..=60));
        });
        ui.horizontal(|ui| {
            ui.label("Inicio");
            crate::screens::date_picker(ui, "o_start", &mut c.start);
            ui.label("Honorarios");
            ui.add(egui::DragValue::new(&mut c.fee).range(0.0..=50000.0).speed(10.0).suffix(" €"));
        });
        ui.label(RichText::new("Diagnóstico").size(13.5).color(MUTED));
        ui.add(egui::TextEdit::multiline(&mut c.diagnosis).desired_rows(2).desired_width(f32::INFINITY));
        ui.label(RichText::new("Objetivos").size(13.5).color(MUTED));
        ui.add(egui::TextEdit::multiline(&mut c.objectives).desired_rows(2).desired_width(f32::INFINITY));
        ui.horizontal_wrapped(|ui| {
            for s in OrthoState::ALL {
                let sel = c.state == s;
                let col = state_color(s);
                if ui.add(egui::Button::new(RichText::new(s.label()).size(13.0).color(if sel { Color32::WHITE } else { col })).fill(if sel { col } else { tint(col, 0.1) }).corner_radius(egui::CornerRadius::same(12))).clicked() {
                    c.state = s;
                }
            }
        });
        muted(ui, format!("Higiene correcta en el {:.0}% de los controles", c.hygiene_pct()));
        if serde_json::to_string(c).unwrap_or_default() != before {
            app.dirty = true;
        }
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Registrar control");
        ui.horizontal(|ui| {
            crate::theme::combo("o_work").width(200.0).selected_text(&app.ortho.work).show_ui(ui, |ui| {
                for w in ["Colocación de aparatología", "Cambio de arco", "Activación", "Entrega de alineadores", "Elásticos", "Reposición de bracket", "Retirada de aparatología", "Revisión de retención"] {
                    ui.selectable_value(&mut app.ortho.work, w.to_string(), w);
                }
            });
            ui.add_sized([150.0, 32.0], egui::TextEdit::singleline(&mut app.ortho.wire).hint_text("Arco / nº alineador"));
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut app.ortho.hygiene, "Higiene correcta");
            ui.label("Próximo en");
            ui.add(egui::DragValue::new(&mut app.ortho.weeks).range(1..=26).suffix(" semanas"));
        });
        ui.add_sized([ui.available_width(), 32.0], egui::TextEdit::singleline(&mut app.ortho.notes).hint_text("Observaciones"));
        if primary(ui, "Guardar control y programar el siguiente").clicked() {
            add = true;
        }
        ui.add_space(6.0);
        section(ui, "Historial de controles");
        let visits = app.db.ortho.iter().find(|c| c.id == id).map(|c| c.visits.clone()).unwrap_or_default();
        if visits.is_empty() {
            muted(ui, "Sin controles todavía.");
        }
        for v in visits.iter().rev() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(fmt_date(v.date)).family(sb()).size(13.5));
                ui.label(RichText::new(format!("{}{}", v.work, if v.wire.is_empty() { String::new() } else { format!(" · {}", v.wire) })).size(13.5));
                right_aligned(ui, |ui| {
                    pill(ui, if v.hygiene_ok { "Higiene ✔" } else { "Mala higiene" }, if v.hygiene_ok { OK } else { WARN });
                });
            });
        }
    });
    if add {
        let v = OrthoVisit { date: today, work: app.ortho.work.clone(), wire: std::mem::take(&mut app.ortho.wire), hygiene_ok: app.ortho.hygiene, next_weeks: app.ortho.weeks, notes: std::mem::take(&mut app.ortho.notes) };
        app.db.add_ortho_visit(id, v);
        app.changed();
        app.notify(format!("Control registrado. Recall de ortodoncia en {} semanas.", app.ortho.weeks));
    }
    if let Some(p) = goto {
        app.open_patient(p, PatTab::Citas);
    }
}
