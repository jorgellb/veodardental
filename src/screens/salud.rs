//! Pestaña «Salud» de la ficha (cuestionario de salud general y dental) e
//! informe de captación de pacientes por procedencia.

use crate::app::App;
use crate::health::*;
use crate::model::*;
use crate::theme::*;
use egui::{Color32, RichText, Ui};

fn print(app: &mut App, ctx: &egui::Context, pid: u32, filled: bool) {
    let form = if filled { app.db.latest_health(pid).cloned() } else { None };
    let html = app.db.health_html(pid, form.as_ref());
    let name = format!("cuestionario_salud_{pid}{}.html", if filled { "" } else { "_blanco" });
    match crate::screens::centros::open_html(ctx, &name, html) {
        Ok(_) => app.notify("Cuestionario abierto para imprimir y firmar."),
        Err(e) => app.warn(format!("No se pudo crear el cuestionario: {e}")),
    }
}

pub fn patient_tab(app: &mut App, ui: &mut Ui, pid: u32) {
    let Some(orig) = app.db.latest_health(pid).cloned() else {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Cuestionario de salud general y dental");
            muted(ui, "Este paciente aún no tiene cuestionario. Las respuestas de riesgo (anticoagulantes, alergias, corazón, embarazo…) se convierten en avisos de la ficha.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if primary(ui, "＋ Rellenar cuestionario").clicked() {
                    let by = app.db.patient(pid).map(|p| p.full_name()).unwrap_or_default();
                    app.db.new_health(pid, app.today, &by);
                    app.changed();
                }
                if ui.button("🖨 Imprimir en blanco").on_hover_text("Para que el paciente lo rellene a mano en la sala de espera").clicked() {
                    print(app, ui.ctx(), pid, false);
                }
            });
        });
        return;
    };
    let female = app.db.patient(pid).map(|p| p.sex == "F").unwrap_or(true);
    let mut f = orig.clone();
    // firmado: las respuestas quedan como se firmaron (para cambiarlas, «Nueva revisión»)
    let locked = orig.signed;
    // cabecera
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        // título y estado; los botones, en la misma línea si caben (en otra si no)
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Cuestionario de salud").size(19.0).family(sb()));
            pill(ui, &format!("del {}", fmt_date(f.date)), INFO);
            if f.signed {
                pill(ui, "Firmado ✔", OK);
            } else {
                pill(ui, "Sin firmar", WARN);
            }
            if app.db.health_status(pid, app.today).is_some_and(|s| s.contains("caducado")) {
                pill(ui, "Caducado", WARN).on_hover_text("Tiene más de un año: conviene una nueva revisión");
            }
            ui.add_space(12.0);
            {
                if ui.button("🖨 En blanco").clicked() {
                    print(app, ui.ctx(), pid, false);
                }
                if ui.button("🖨 Imprimir para firmar").clicked() {
                    print(app, ui.ctx(), pid, true);
                }
                if ui.button("↻ Nueva revisión").on_hover_text("Crea un cuestionario con fecha de hoy copiando las respuestas actuales").clicked() {
                    let by = f.filled_by.clone();
                    app.db.new_health(pid, app.today, &by);
                    app.changed();
                    app.notify("Nueva revisión del cuestionario creada: repase las respuestas con el paciente.");
                }
            }
        });
        // los campos a lo ancho que haya (field() los hace un 10 % más anchos)
        let w = (ui.available_width() - 40.0) / 1.1;
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!locked, |ui| {
                field(ui, "Cumplimentado por", &mut f.filled_by, (w * 0.4).min(240.0));
                field(ui, "Motivo de la consulta", &mut f.reason, (w * 0.6).min(360.0));
            });
        });
        if locked {
            let integrity = app.db.health_integrity(orig.id);
            let col = if integrity == crate::firmado::Integrity::Modified { DANGER } else { MUTED() };
            ui.label(RichText::new(integrity.label()).font(mdf(12.3)).color(col));
        } else {
            ui.checkbox(&mut f.signed, "Revisado y firmado por el paciente");
        }
        let alerts = f.alerts();
        let allergies = f.allergies();
        let interests = f.interests();
        if !alerts.is_empty() || !allergies.is_empty() {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Avisos para la ficha:").color(MUTED()));
                for a in &alerts {
                    pill(ui, &format!("⚠ {a}"), DANGER);
                }
                for a in &allergies {
                    pill(ui, &format!("Alergia: {a}"), DANGER);
                }
            });
        }
        if !interests.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Oportunidades:").color(MUTED()));
                for i in &interests {
                    pill(ui, i, ACCENT);
                }
            });
        }
    });
    ui.add_space(8.0);
    ui.columns(2, |cols| {
        for (col, (part, name)) in cols.iter_mut().zip([(Part::General, "Salud general"), (Part::Dental, "Salud bucodental")]) {
            card(col, |ui| {
                ui.set_width(ui.available_width());
                if locked {
                    ui.disable();
                }
                section(ui, name);
                for q in QUESTIONS.iter().filter(|q| q.part == part && (female || !q.female)) {
                    let mut yes = f.is(q.key);
                    let risky = q.alert.is_some() || q.allergy;
                    let text = RichText::new(q.text).color(if yes && risky { DANGER } else { TEXT });
                    if ui.checkbox(&mut yes, text).changed() {
                        f.yes.insert(q.key.to_string(), yes);
                    }
                    if yes && q.detail {
                        ui.horizontal(|ui| {
                            ui.add_space(30.0);
                            let d = f.details.entry(q.key.to_string()).or_default();
                            ui.add(egui::TextEdit::singleline(d).hint_text("¿Cuál? Indique el detalle").desired_width(ui.available_width() - 10.0));
                        });
                    }
                    if yes && q.key == "fuma" {
                        ui.horizontal(|ui| {
                            ui.add_space(30.0);
                            ui.label("Cigarrillos al día");
                            ui.add(egui::DragValue::new(&mut f.cigarettes).range(0..=80));
                        });
                    }
                }
                if part == Part::Dental {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Última visita al dentista");
                        crate::theme::combo("hf_visit").width(150.0).selected_text(LAST_VISIT[f.last_visit.min(4)]).show_ui(ui, |ui| {
                            for (i, t) in LAST_VISIT.iter().enumerate() {
                                ui.selectable_value(&mut f.last_visit, i, *t);
                            }
                        });
                        ui.add_space(10.0);
                        ui.label("Cepillados al día");
                        ui.add(egui::DragValue::new(&mut f.brushings).range(0..=6));
                    });
                    ui.label(RichText::new("Utiliza para su higiene").color(MUTED()));
                    ui.horizontal_wrapped(|ui| {
                        for a in AIDS {
                            let mut on = f.aids.iter().any(|x| x == a);
                            if ui.checkbox(&mut on, a).changed() {
                                if on {
                                    f.aids.push(a.to_string());
                                } else {
                                    f.aids.retain(|x| x != a);
                                }
                            }
                        }
                    });
                }
            });
        }
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Observaciones del profesional");
        ui.add(egui::TextEdit::multiline(&mut f.notes).desired_rows(2).desired_width(f32::INFINITY));
        let older: Vec<(u32, chrono::NaiveDate, bool)> = app.db.health.iter().filter(|h| h.patient_id == pid && h.id != f.id).map(|h| (h.id, h.date, h.signed)).collect();
        if !older.is_empty() {
            ui.add_space(6.0);
            muted(ui, format!("Revisiones anteriores: {}", older.iter().map(|o| format!("{}{}", fmt_date(o.1), if o.2 { " (firmada)" } else { "" })).collect::<Vec<_>>().join(" · ")));
        }
    });
    if locked {
        // de un cuestionario firmado solo cambian las observaciones del profesional
        let notes = std::mem::take(&mut f.notes);
        f = orig.clone();
        f.notes = notes;
    }
    if f != orig {
        let id = f.id;
        let sign = f.signed && !orig.signed;
        f.signed = orig.signed;
        if let Some(h) = app.db.health.iter_mut().find(|h| h.id == id) {
            *h = f;
        }
        if sign && let Err(e) = app.db.sign_health(id) {
            app.warn(e);
        }
        if app.db.apply_health(id) {
            app.notify("Avisos de salud añadidos a la ficha del paciente.");
        }
        app.changed();
    }
}

/// Informe: de dónde vienen los pacientes nuevos y cuánto han generado.
pub fn captacion(app: &mut App, ui: &mut Ui) {
    let rows = app.db.referral_stats(app.lists.report_from, app.lists.report_to);
    let total: usize = rows.iter().map(|r| r.1).sum();
    let max = rows.iter().map(|r| r.1).max().unwrap_or(1).max(1);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Captación: pacientes nuevos por procedencia");
        muted(ui, format!("{} pacientes dados de alta entre el {} y el {}", total, fmt_date(app.lists.report_from), fmt_date(app.lists.report_to)));
        ui.add_space(6.0);
        if rows.is_empty() {
            empty_state(ui, "No hay altas en este periodo.");
            return;
        }
        let tbl = Table::new("captacion", vec![("Procedencia", 220.0), ("Pacientes", 100.0), ("% del total", 110.0), ("Producción generada", 170.0), ("", 360.0)]);
        tbl.header(ui);
        let palette = [ACCENT, INFO, Color32::from_rgb(214, 96, 140), WARN, Color32::from_rgb(110, 90, 200), OK, MUTED(), DANGER];
        for (i, (k, n, prod)) in rows.iter().enumerate() {
            let r = tbl.row(ui, i, false, &[cell(k), cell(n.to_string()), cell(format!("{:.0} %", *n as f64 * 100.0 / total.max(1) as f64)), cell(money(*prod)), cell("")]);
            let w = (r.rect.width() * 0.3) * (*n as f32 / max as f32);
            let x0 = r.rect.max.x - r.rect.width() * 0.32;
            ui.painter().rect_filled(egui::Rect::from_min_size(egui::pos2(x0, r.rect.center().y - 7.0), egui::vec2(w.max(3.0), 14.0)), egui::CornerRadius::same(4), palette[i % palette.len()]);
        }
        ui.add_space(8.0);
        if let Some(best) = rows.iter().max_by(|a, b| a.2.total_cmp(&b.2)) {
            ui.label(RichText::new(format!("La procedencia que más producción ha generado es «{}» ({}).", best.0, money(best.2))).family(sb()));
        }
        muted(ui, "La procedencia se indica en la filiación del paciente (campo «Procedencia»).");
    });
}
