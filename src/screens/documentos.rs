//! Documentos del paciente: consentimientos informados, protección de datos,
//! justificantes e informes, con firma manuscrita en pantalla o tableta y
//! exportación a HTML imprimible.

use crate::app::App;
use crate::comms::*;
use crate::model::*;
use crate::theme::*;
use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

pub struct DocsState {
    pub selected: Option<u32>,
    pub new_template: u32,
    pub new_teeth: String,
    pub strokes: Vec<Vec<[f32; 2]>>,
    pub signer: String,
    /// Revocación que se está anotando: documento, quién revoca y motivo.
    pub revoke: Option<(u32, String, String)>,
}

impl Default for DocsState {
    fn default() -> Self {
        DocsState { selected: None, new_template: 1, new_teeth: String::new(), strokes: vec![], signer: String::new(), revoke: None }
    }
}

pub fn patient_tab(app: &mut App, ui: &mut Ui, pid: u32) {
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    let list_w = 360.0;
    let mut docs: Vec<PatientDoc> = app.db.documents.iter().filter(|d| d.patient_id == pid).cloned().collect();
    docs.sort_by_key(|d| std::cmp::Reverse((d.date, d.id)));
    if app.docs.selected.map(|s| !docs.iter().any(|d| d.id == s)).unwrap_or(true) {
        app.docs.selected = docs.first().map(|d| d.id);
    }
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(list_w);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Nuevo documento");
                let tpls: Vec<(u32, String)> = app.db.doc_templates.iter().map(|t| (t.id, t.name.clone())).collect();
                let cur = tpls.iter().find(|t| t.0 == app.docs.new_template).map(|t| t.1.clone()).unwrap_or_else(|| "—".into());
                crate::theme::combo("doc_tpl").width(ui.available_width() - 10.0).selected_text(cur).show_ui(ui, |ui| {
                    for (id, n) in &tpls {
                        ui.selectable_value(&mut app.docs.new_template, *id, n);
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_sized([150.0, 34.0], egui::TextEdit::singleline(&mut app.docs.new_teeth).hint_text("Piezas (opcional)"));
                    if primary(ui, "Crear documento").clicked() {
                        let teeth: Vec<u8> = app.docs.new_teeth.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).filter(|t| crate::screens::pacientes::valid_tooth(*t)).collect();
                        if let Some(id) = app.db.new_document(pid, app.docs.new_template, &teeth) {
                            app.docs.selected = Some(id);
                            app.docs.strokes.clear();
                            app.docs.new_teeth.clear();
                            app.changed();
                            app.notify("Documento creado con los datos del paciente. Revíselo y fírmelo.");
                        }
                    }
                });
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Documentos del paciente");
                if docs.is_empty() {
                    empty_state(ui, "Sin documentos.");
                }
                for d in &docs {
                    let sel = app.docs.selected == Some(d.id);
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::click());
                    let p = ui.painter();
                    p.rect_filled(
                        rect,
                        CornerRadius::same(10),
                        if sel {
                            Color32::from_rgb(214, 238, 234)
                        } else if resp.hovered() {
                            ROW_HOVER
                        } else {
                            ROW_ALT
                        },
                    );
                    p.with_clip_rect(rect.shrink(4.0)).text(rect.min + vec2(12.0, 10.0), egui::Align2::LEFT_TOP, &d.title, sbf(14.5), TEXT);
                    p.text(rect.min + vec2(12.0, 34.0), egui::Align2::LEFT_TOP, format!("{} · {}", fmt_date(d.date), d.category), egui::FontId::proportional(12.5), MUTED);
                    let (t, c) = if d.revoked_at.is_some() {
                        ("Revocado", DANGER)
                    } else if crate::firmado::doc_integrity(d) == crate::firmado::Integrity::Modified {
                        ("⚠ Cambiado tras firmar", DANGER)
                    } else if d.signed {
                        ("Firmado", OK)
                    } else {
                        ("Sin firmar", WARN)
                    };
                    p.text(rect.right_top() + vec2(-12.0, 36.0), egui::Align2::RIGHT_TOP, t, sbf(12.5), c);
                    if resp.clicked() {
                        app.docs.selected = Some(d.id);
                        app.docs.strokes.clear();
                    }
                    ui.add_space(4.0);
                }
            });
        });
        ui.vertical(|ui| {
            ui.set_width(avail - list_w - 30.0);
            if let Some(id) = app.docs.selected {
                viewer(app, ui, id);
            }
        });
    });
}

fn draw_strokes(p: &egui::Painter, rect: Rect, strokes: &[Vec<[f32; 2]>], col: Color32) {
    for s in strokes {
        let pts: Vec<Pos2> = s.iter().map(|q| pos2(rect.min.x + q[0] * rect.width(), rect.min.y + q[1] * rect.height())).collect();
        if pts.len() > 1 {
            p.add(Shape::line(pts, Stroke::new(2.4, col)));
        }
    }
}

fn viewer(app: &mut App, ui: &mut Ui, id: u32) {
    let Some(d) = app.db.documents.iter().find(|d| d.id == id).cloned() else { return };
    let vw = ui.available_width();
    // «papel»
    egui::Frame::new().fill(Color32::WHITE).stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(40, 30)).shadow(shadow()).show(ui, |ui| {
        ui.set_width(vw - 80.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
            crate::screens::odontograma::draw_logo(ui.painter(), r);
            ui.vertical(|ui| {
                ui.label(RichText::new(&app.db.config.name).family(sb()).size(16.0));
                muted(ui, format!("{} · {}", app.db.config.address, app.db.config.phone));
            });
        });
        ui.add_space(14.0);
        ui.label(RichText::new(d.title.to_uppercase()).family(sb()).size(18.0));
        ui.add_space(10.0);
        egui::ScrollArea::vertical().id_salt(("docbody", id)).max_height(330.0).show(ui, |ui| {
            ui.label(RichText::new(&d.body).size(15.0).color(Color32::from_rgb(40, 44, 52)));
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            // firma del paciente
            ui.vertical(|ui| {
                let rep = app.db.representative(d.patient_id, app.today);
                ui.label(RichText::new(if rep.is_some() { "Firma del padre, madre o tutor (paciente menor de 16 años)" } else { "Firma del paciente" }).size(13.0).color(MUTED));
                let (rect, resp) = ui.allocate_exact_size(vec2(320.0, 120.0), Sense::drag());
                let p = ui.painter();
                p.rect_filled(rect, CornerRadius::same(8), if d.signed { Color32::WHITE } else { Color32::from_rgb(248, 250, 253) });
                p.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, if d.signed { BORDER } else { ACCENT }), StrokeKind::Inside);
                p.line_segment([rect.left_bottom() + vec2(16.0, -26.0), rect.right_bottom() + vec2(-16.0, -26.0)], Stroke::new(1.0, Color32::from_rgb(210, 216, 226)));
                if d.signed {
                    draw_strokes(p, rect, &d.signature, Color32::from_rgb(20, 40, 110));
                } else {
                    if resp.drag_started() {
                        app.docs.strokes.push(vec![]);
                    }
                    if resp.dragged()
                        && let (Some(pp), Some(last)) = (resp.interact_pointer_pos(), app.docs.strokes.last_mut())
                    {
                        let q = [((pp.x - rect.min.x) / rect.width()).clamp(0.0, 1.0), ((pp.y - rect.min.y) / rect.height()).clamp(0.0, 1.0)];
                        last.push(q);
                    }
                    draw_strokes(p, rect, &app.docs.strokes, Color32::from_rgb(20, 40, 110));
                    if app.docs.strokes.is_empty() {
                        p.text(rect.center(), egui::Align2::CENTER_CENTER, "Firme aquí con el ratón o la tableta", egui::FontId::proportional(13.0), MUTED);
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                    }
                }
            });
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Profesional").size(13.0).color(MUTED));
                ui.label(RichText::new(app.db.doctor(d.doctor_id).map(|c| c.name.clone()).unwrap_or_default()).family(sb()));
                muted(ui, app.db.doctor(d.doctor_id).map(|c| format!("Nº colegiado {}", c.license)).unwrap_or_default());
                if d.signed {
                    ui.add_space(8.0);
                    ui.label(RichText::new(format!("✔ Firmado por {}", d.signer)).color(OK).family(sb()));
                    muted(ui, d.signed_at.map(|t| t.format("%d/%m/%Y %H:%M").to_string()).unwrap_or_default());
                    let integrity = app.db.doc_integrity(id);
                    let col = match integrity {
                        crate::firmado::Integrity::Modified => DANGER,
                        crate::firmado::Integrity::Intact => OK,
                        _ => MUTED,
                    };
                    ui.label(RichText::new(integrity.label()).font(mdf(12.3)).color(col));
                    if let Some(t) = d.revoked_at {
                        ui.label(RichText::new(format!("Revocado el {} por {}", t.format("%d/%m/%Y %H:%M"), d.revoked_by)).color(DANGER).family(sb()));
                        muted(ui, format!("Motivo: {}", d.revoke_reason));
                    }
                }
            });
        });
    });
    // firmarlo por internet (con la página de la clínica activada)
    if app.db.booking.enabled && app.db.booking.docs && (!d.signed || app.db.doc_link(id).is_some()) {
        ui.add_space(8.0);
        crate::screens::cita_online::doc_link_row(app, ui, id);
    }
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            if !d.signed {
                // un menor de 16 años no firma: firma su representante
                let rep = app.db.representative(d.patient_id, app.today);
                let def = match &rep {
                    Some(r) => r.clone(),
                    None => app.db.patient_name(d.patient_id),
                };
                if app.docs.signer.is_empty() {
                    app.docs.signer = def;
                }
                if rep.as_deref() == Some("") {
                    ui.label(RichText::new("Es menor de 16 años: firma su padre, madre o tutor (anótelo en Filiación › Responsable).").color(WARN));
                }
                ui.label(RichText::new("Firmante").color(MUTED));
                ui.add_sized([220.0, 34.0], egui::TextEdit::singleline(&mut app.docs.signer));
                if primary(ui, "✍ Firmar documento").clicked() {
                    let strokes = std::mem::take(&mut app.docs.strokes);
                    let signer = app.docs.signer.clone();
                    match app.db.sign_document(id, &signer, strokes.clone()) {
                        Ok(()) => {
                            app.changed();
                            app.notify("Documento firmado y guardado.");
                        }
                        Err(e) => {
                            app.docs.strokes = strokes;
                            app.warn(e);
                        }
                    }
                }
                if ui.button("Borrar firma").clicked() {
                    app.docs.strokes.clear();
                }
            }
            if ui.button("🖨 Exportar / imprimir").on_hover_text("Genera una página HTML lista para imprimir o guardar como PDF").clicked() {
                match export_html(app, &d) {
                    Ok(path) => {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(format!("file://{}", path.display())));
                        app.notify(format!("Documento exportado en {}", path.display()));
                    }
                    Err(e) => app.warn(e),
                }
            }
            if ui.button("✉ Enviar por email").clicked() {
                let to = app.db.patient(d.patient_id).map(|p| p.email.clone()).unwrap_or_default();
                let url = format!("mailto:{}?subject={}&body={}", to, crate::screens::comunicaciones::enc_pub(&d.title), crate::screens::comunicaciones::enc_pub(&d.body));
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
            if !d.signed && ui.button("Quitar").clicked() {
                app.db.documents.retain(|x| x.id != id);
                app.docs.selected = None;
                app.changed();
            }
            // el paciente (o su representante) retira un consentimiento firmado
            if d.signed && d.revoked_at.is_none() && app.docs.revoke.as_ref().is_none_or(|r| r.0 != id) && ui.button("Revocar…").on_hover_text("El paciente retira este consentimiento: el documento y su firma se conservan").clicked() {
                let by = app.db.representative(d.patient_id, app.today).filter(|r| !r.is_empty()).unwrap_or_else(|| app.db.patient_name(d.patient_id));
                app.docs.revoke = Some((id, by, String::new()));
            }
        });
        if let Some((rid, by, reason)) = app.docs.revoke.as_mut()
            && *rid == id
        {
            ui.add_space(6.0);
            let mut go = false;
            let mut cancel = false;
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Revoca").color(MUTED));
                ui.add_sized([200.0, 32.0], egui::TextEdit::singleline(by));
                ui.label(RichText::new("Motivo").color(MUTED));
                ui.add_sized([260.0, 32.0], egui::TextEdit::singleline(reason).hint_text("Lo que indica el paciente"));
                go = primary(ui, "Anotar revocación").clicked();
                cancel = ui.button("Cancelar").clicked();
            });
            if go {
                let (b, r) = (by.clone(), reason.clone());
                match app.db.revoke_document(id, &b, &r) {
                    Ok(()) => {
                        app.docs.revoke = None;
                        app.changed();
                        app.notify("Revocación anotada: el documento y su firma se conservan.");
                    }
                    Err(e) => app.warn(e),
                }
            } else if cancel {
                app.docs.revoke = None;
            }
        }
    });
}

/// Documento imprimible (con la firma y, si se firmó por internet, los datos que la acreditan).
fn export_html(app: &App, d: &PatientDoc) -> Result<std::path::PathBuf, String> {
    let dir = crate::paths::documents().join(crate::brand::DIR).join("documentos");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = app.db.patient(d.patient_id).cloned().unwrap_or_default();
    let path = dir.join(format!("{}_{}_{}.html", p.history_no, d.id, fold(&d.title).replace(|c: char| !c.is_ascii_alphanumeric(), "_")));
    std::fs::write(&path, crate::doc_link::doc_html(&app.db, d)).map_err(|e| e.to_string())?;
    Ok(path)
}
