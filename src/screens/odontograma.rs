//! Odontograma interactivo: arcadas adulta y temporal (numeración FDI),
//! cinco caras por pieza, herramientas de tratamiento y evolución por fechas.

use crate::app::App;
use crate::model::*;
use crate::screens::odg_paint::*;
use crate::theme::*;
use chrono::NaiveDate;
use egui::{Align2, Color32, CornerRadius, Painter, Rect, RichText, Sense, Shape, Stroke, Ui, vec2};

pub const UPPER_ADULT: [u8; 16] = [18, 17, 16, 15, 14, 13, 12, 11, 21, 22, 23, 24, 25, 26, 27, 28];
pub const LOWER_ADULT: [u8; 16] = [48, 47, 46, 45, 44, 43, 42, 41, 31, 32, 33, 34, 35, 36, 37, 38];
pub const UPPER_CHILD: [u8; 10] = [55, 54, 53, 52, 51, 61, 62, 63, 64, 65];
pub const LOWER_CHILD: [u8; 10] = [85, 84, 83, 82, 81, 71, 72, 73, 74, 75];

pub struct OdgState {
    pub tool: OdgKind,
    pub code: String,
    pub state: TxState,
    pub child: bool,
    pub faces: [bool; 5],
    pub selected_tooth: Option<u8>,
    pub bridge_start: Option<u8>,
    /// Ver la boca tal y como estaba en esta fecha (None = hoy).
    pub until: Option<NaiveDate>,
    pub doctor: u32,
    /// Aspecto oscuro tipo radiografía (por defecto, claro).
    pub dark: bool,
}

impl Default for OdgState {
    fn default() -> Self {
        Self { tool: OdgKind::Obturacion, code: "0301".into(), state: TxState::Previsto, child: false, faces: [false, false, true, false, false], selected_tooth: None, bridge_start: None, until: None, doctor: 1, dark: false }
    }
}

pub fn state_color(s: TxState) -> Color32 {
    match s {
        TxState::Realizado => Color32::from_rgb(37, 99, 235),
        TxState::Previsto => Color32::from_rgb(225, 29, 72),
        TxState::Previo => Color32::from_rgb(5, 150, 105),
    }
}

fn is_upper(t: u8) -> bool {
    matches!(t / 10, 1 | 2 | 5 | 6)
}

/// Pequeño logotipo (diente estilizado).
pub fn draw_logo(p: &Painter, r: Rect) {
    p.rect_filled(r, CornerRadius::same(9), ACCENT);
    let c = r.center();
    let s = r.width() / 34.0;
    let pts = vec![
        c + vec2(-9.0, -8.0) * s,
        c + vec2(-4.0, -11.0) * s,
        c + vec2(0.0, -8.5) * s,
        c + vec2(4.0, -11.0) * s,
        c + vec2(9.0, -8.0) * s,
        c + vec2(8.0, 1.0) * s,
        c + vec2(5.0, 11.0) * s,
        c + vec2(2.5, 11.0) * s,
        c + vec2(0.0, 3.0) * s,
        c + vec2(-2.5, 11.0) * s,
        c + vec2(-5.0, 11.0) * s,
        c + vec2(-8.0, 1.0) * s,
    ];
    p.add(Shape::closed_line(pts, Stroke::new(2.0 * s, Color32::WHITE)));
}

#[derive(Default, Clone)]
pub struct ToothView {
    pub absent: Option<TxState>,
    pub extracted: Option<TxState>,
    pub implant: Option<TxState>,
    pub crown: Option<TxState>,
    pub endo: Option<TxState>,
    pub post: Option<TxState>,
    pub apico: Option<TxState>,
    pub veneer: Option<TxState>,
    pub bracket: Option<TxState>,
    pub bridge: Option<TxState>,
    pub faces: [Option<TxState>; 5],
    pub sealant: Option<TxState>,
    pub count: usize,
}

fn rank(s: TxState) -> u8 {
    match s {
        TxState::Previo => 0,
        TxState::Realizado => 1,
        TxState::Previsto => 2,
    }
}
fn put(slot: &mut Option<TxState>, s: TxState) {
    // Lo planificado se superpone a lo realizado para que destaque.
    if slot.map(|x| rank(s) >= rank(x)).unwrap_or(true) {
        *slot = Some(s);
    }
}

fn build_views(entries: &[&MedEntry]) -> std::collections::HashMap<u8, ToothView> {
    let mut m: std::collections::HashMap<u8, ToothView> = Default::default();
    for e in entries {
        for &t in &e.teeth {
            let v = m.entry(t).or_default();
            v.count += 1;
            let s = e.state;
            match e.odg {
                OdgKind::Ausente => put(&mut v.absent, s),
                OdgKind::Extraccion => put(&mut v.extracted, s),
                OdgKind::Implante => put(&mut v.implant, s),
                OdgKind::Corona => put(&mut v.crown, s),
                OdgKind::Endodoncia => put(&mut v.endo, s),
                OdgKind::PernoMunon => put(&mut v.post, s),
                OdgKind::Apicectomia => put(&mut v.apico, s),
                OdgKind::Carilla => put(&mut v.veneer, s),
                OdgKind::Bracket => put(&mut v.bracket, s),
                OdgKind::Puente => put(&mut v.bridge, s),
                OdgKind::Sellador => put(&mut v.sealant, s),
                OdgKind::Obturacion => {
                    for f in &e.faces {
                        if let Some(i) = FACES.iter().position(|x| x == f) {
                            put(&mut v.faces[i], s);
                        }
                    }
                    if e.faces.is_empty() {
                        put(&mut v.faces[2], s);
                    }
                }
                _ => {}
            }
        }
    }
    m
}

fn options_card(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Herramienta");
        ui.label(RichText::new(app.odg.tool.label()).size(15.0).color(ACCENT));
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Tratamiento").color(MUTED));
            let tool = app.odg.tool;
            let options: Vec<(String, String)> = app.db.treatments.iter().filter(|t| t.active && t.odg == tool).map(|t| (t.code.clone(), format!("{} · {}", t.code, t.description))).collect();
            let current = options.iter().find(|o| o.0 == app.odg.code).map(|o| o.1.clone()).unwrap_or_else(|| "—".into());
            crate::theme::combo("odg_code").width(ui.available_width().min(290.0) - 12.0).selected_text(current).show_ui(ui, |ui| {
                for (c, l) in &options {
                    ui.selectable_value(&mut app.odg.code, c.clone(), l);
                }
            });
            ui.add_space(8.0);
            for s in [TxState::Previsto, TxState::Realizado, TxState::Previo] {
                let sel = app.odg.state == s;
                let col = state_color(s);
                let b = egui::Button::new(RichText::new(format!("● {}", s.label())).color(if sel { Color32::WHITE } else { col })).fill(if sel { col } else { tint(col, 0.12) }).corner_radius(CornerRadius::same(16));
                if ui.add(b).clicked() {
                    app.odg.state = s;
                }
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Doctor").color(MUTED));
            let docs: Vec<(u32, String)> = app.db.collaborators.iter().filter(|c| c.active).map(|c| (c.id, c.alias.clone())).collect();
            crate::theme::combo("odg_doc").selected_text(app.db.doctor_alias(app.odg.doctor)).show_ui(ui, |ui| {
                for (id, n) in &docs {
                    ui.selectable_value(&mut app.odg.doctor, *id, n);
                }
            });
            ui.checkbox(&mut app.odg.child, "Dentición temporal");
            if app.odg.tool.uses_faces() {
                ui.add_space(8.0);
                ui.label(RichText::new("Caras").color(MUTED));
                for (i, f) in FACES.iter().enumerate() {
                    ui.toggle_value(&mut app.odg.faces[i], *f).on_hover_text(match *f {
                        "M" => "Mesial",
                        "D" => "Distal",
                        "O" => "Oclusal / incisal",
                        "V" => "Vestibular",
                        _ => "Lingual / palatina",
                    });
                }
            }
            if app.odg.tool == OdgKind::Puente {
                muted(
                    ui,
                    match app.odg.bridge_start {
                        None => "Pulse el primer pilar del puente…",
                        Some(_) => "…y ahora el último pilar.",
                    },
                );
            }
        });
    });
}

pub fn show(app: &mut App, ui: &mut Ui, pid: u32) {
    let today = app.today;
    let until = app.odg.until;
    let entries: Vec<MedEntry> = app.db.entries.iter().filter(|e| e.patient_id == pid && until.map(|d| e.date <= d).unwrap_or(true)).filter(|e| until.is_none() || e.state != TxState::Previsto).cloned().collect();
    let refs: Vec<&MedEntry> = entries.iter().collect();
    let views = build_views(&refs);
    let img_teeth: Vec<u8> = app.db.images.iter().filter(|i| i.patient_id == pid).flat_map(|i| i.teeth.clone()).collect();

    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    let side_w = if avail < 1050.0 { 310.0 } else { 340.0 };
    let draw_w = (avail - side_w - 16.0).max(540.0);
    // con poco ancho, la navegación por visitas baja a su propia fila
    let narrow = draw_w < 760.0;
    let mut click = None;
    let mut dates: Vec<NaiveDate> = app.db.entries.iter().filter(|e| e.patient_id == pid).map(|e| e.date).collect();
    dates.sort();
    dates.dedup();
    let dark = app.odg.dark;
    let sk = skin(dark);
    ui.horizontal_top(|ui| {
        let mut frame = egui::Frame::new().fill(sk.bg).stroke(Stroke::new(1.0, sk.border)).corner_radius(CornerRadius::same(18)).inner_margin(egui::Margin::symmetric(18, 16));
        if !dark {
            frame = frame.shadow(crate::theme::shadow());
        }
        frame.show(ui, |ui| {
            ui.set_width(draw_w - 36.0);
            ui.vertical(|ui| {
                // cabecera: fecha de la visita, estado y navegación por la evolución
                ui.horizontal(|ui| {
                    let d = app.odg.until.unwrap_or(today);
                    ui.label(RichText::new(format!("{}", chrono::Datelike::day(&d))).font(crate::theme::bf(34.0)).color(sk.text));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.add_space(3.0);
                        ui.label(RichText::new(weekday_es(d)).font(crate::theme::sbf(14.5)).color(sk.text));
                        ui.label(RichText::new(format!("{} {}", month_es(chrono::Datelike::month(&d)), chrono::Datelike::year(&d))).font(crate::theme::mdf(12.5)).color(sk.muted));
                    });
                    ui.add_space(12.0);
                    let (lbl, col) = match app.odg.until {
                        Some(_) => ("Evolución · visita anterior", WARN),
                        None => ("Estado actual de la boca", OK),
                    };
                    pill(ui, lbl, col);
                    right_aligned(ui, |ui| {
                        let mut v = dark as usize;
                        segmented(ui, &["Claro", "Radiografía"], &mut v, &sk);
                        if (v == 1) != dark {
                            app.odg.dark = v == 1;
                        }
                        if !narrow {
                            ui.add_space(8.0);
                            visit_nav(app, ui, &dates, &sk);
                        }
                    });
                });
                if narrow {
                    // con poco ancho, la navegación por visitas va en su propia fila
                    ui.add_space(4.0);
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 36.0), egui::Layout::right_to_left(egui::Align::Center), |ui| visit_nav(app, ui, &dates, &sk));
                }
                ui.add_space(10.0);
                // barra de herramientas con iconos
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
                    let n = OdgKind::ALL_TOOLS.len() as f32;
                    let bw = ((ui.available_width() - 5.0 * (n - 1.0)) / n - 0.5).clamp(36.0, 76.0);
                    for k in OdgKind::ALL_TOOLS {
                        let sel = app.odg.tool == k;
                        if tool_button(ui, vec2(bw, 64.0), sel, k.label(), |p, r| tool_icon(p, k, r, dark), short_label(k), &sk).clicked() {
                            app.odg.tool = k;
                            app.odg.bridge_start = None;
                            if let Some(t) = app.db.treatments.iter().find(|t| t.odg == k && t.active) {
                                app.odg.code = t.code.clone();
                            }
                        }
                    }
                });
                ui.add_space(10.0);
                let (up, low): (&[u8], &[u8]) = if app.odg.child { (&UPPER_CHILD, &LOWER_CHILD) } else { (&UPPER_ADULT, &LOWER_ADULT) };
                let aw = draw_w - 36.0;
                // rótulos de lado (derecha e izquierda del paciente)
                let (lr, _) = ui.allocate_exact_size(vec2(aw, 14.0), Sense::hover());
                ui.painter().text(lr.left_center() + vec2(4.0, 0.0), Align2::LEFT_CENTER, "DERECHA", crate::theme::sbf(10.5), sk.muted);
                ui.painter().text(lr.right_center() - vec2(4.0, 0.0), Align2::RIGHT_CENTER, "IZQUIERDA", crate::theme::sbf(10.5), sk.muted);
                ui.painter().text(lr.center(), Align2::CENTER_CENTER, "ARCADA SUPERIOR", crate::theme::sbf(10.5), sk.muted);
                if let Some(c) = arch(ui, up, true, &views, app.odg.selected_tooth, aw, &img_teeth, dark) {
                    click = Some(c);
                }
                let (r, _) = ui.allocate_exact_size(vec2(aw, 12.0), Sense::hover());
                ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, sk.divider));
                if let Some(c) = arch(ui, low, false, &views, app.odg.selected_tooth, aw, &img_teeth, dark) {
                    click = Some(c);
                }
                let (lr, _) = ui.allocate_exact_size(vec2(aw, 14.0), Sense::hover());
                ui.painter().text(lr.center(), Align2::CENTER_CENTER, "ARCADA INFERIOR", crate::theme::sbf(10.5), sk.muted);
                ui.add_space(8.0);
                // leyenda: se parte en dos líneas si no cabe (así el lienzo no se ensancha)
                ui.horizontal_wrapped(|ui| {
                    ui.set_max_width(aw);
                    for s in [TxState::Realizado, TxState::Previsto, TxState::Previo] {
                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(4), state_color(s));
                        ui.label(RichText::new(s.label()).font(crate::theme::mdf(13.0)).color(sk.text));
                        ui.add_space(8.0);
                    }
                    ui.label(RichText::new("Pulse una pieza para aplicar la herramienta; en la vista oclusal, la cara a obturar").font(crate::theme::mdf(12.3)).color(sk.muted));
                });
            });
        });
        ui.vertical(|ui| {
            ui.set_width(side_w);
            options_card(app, ui);
            ui.add_space(8.0);
            side_panel(app, ui, pid, &entries);
        });
    });

    if let Some((t, face)) = click {
        if until.is_some() {
            app.warn("Está viendo una fecha pasada. Vuelva al estado actual (⏭) para editar.");
            return;
        }
        app.odg.selected_tooth = Some(t);
        apply_tool(app, pid, t, face, today);
    }
}

/// Botones para recorrer la evolución por visitas (en un diseño de derecha a izquierda).
fn visit_nav(app: &mut App, ui: &mut Ui, dates: &[NaiveDate], sk: &Skin) {
    let idx = app.odg.until.and_then(|d| dates.iter().position(|x| *x == d));
    if nav_button(ui, false, true, "Estado actual", sk).clicked() {
        app.odg.until = None;
    }
    if nav_button(ui, false, false, "Visita siguiente", sk).clicked() {
        app.odg.until = match idx {
            Some(i) if i + 1 < dates.len() => dates.get(i + 1).copied(),
            _ => None,
        };
    }
    if nav_button(ui, true, false, "Visita anterior", sk).clicked() {
        app.odg.until = match idx {
            Some(0) => dates.first().copied(),
            Some(i) => dates.get(i - 1).copied(),
            None => dates.last().copied(),
        };
    }
    if nav_button(ui, true, true, "Primera visita", sk).clicked() {
        app.odg.until = dates.first().copied();
    }
}

pub(crate) fn apply_tool(app: &mut App, pid: u32, t: u8, face: Option<usize>, today: NaiveDate) {
    let tool = app.odg.tool;
    let code = app.odg.code.clone();
    if app.db.treatment(&code).map(|x| x.odg != tool).unwrap_or(true) {
        app.warn("Seleccione un tratamiento para esta herramienta.");
        return;
    }
    let state = app.odg.state;
    let doc = app.odg.doctor;
    let teeth: Vec<u8>;
    let mut faces: Vec<String> = vec![];
    if tool == OdgKind::Puente {
        match app.odg.bridge_start {
            None => {
                app.odg.bridge_start = Some(t);
                return;
            }
            Some(a) => {
                app.odg.bridge_start = None;
                let row: Vec<u8> = if app.odg.child { UPPER_CHILD.iter().chain(LOWER_CHILD.iter()).copied().collect() } else { UPPER_ADULT.iter().chain(LOWER_ADULT.iter()).copied().collect() };
                let ia = row.iter().position(|x| *x == a);
                let ib = row.iter().position(|x| *x == t);
                match (ia, ib) {
                    (Some(i), Some(j)) if is_upper(a) == is_upper(t) => {
                        let (i, j) = (i.min(j), i.max(j));
                        teeth = row[i..=j].to_vec();
                    }
                    _ => {
                        app.warn("Los pilares del puente deben estar en la misma arcada.");
                        return;
                    }
                }
            }
        }
    } else if tool == OdgKind::ProtesisRemovible {
        let arch: &[u8] = match (app.odg.child, is_upper(t)) {
            (false, true) => &UPPER_ADULT,
            (false, false) => &LOWER_ADULT,
            (true, true) => &UPPER_CHILD,
            (true, false) => &LOWER_CHILD,
        };
        // Se aplica a las piezas ausentes/extraídas de la arcada, o a la pieza pulsada.
        let gone: Vec<u8> = arch.iter().copied().filter(|x| app.db.entries.iter().any(|e| e.patient_id == pid && e.teeth.contains(x) && matches!(e.odg, OdgKind::Ausente | OdgKind::Extraccion) && e.state != TxState::Previsto)).collect();
        teeth = if gone.is_empty() { vec![t] } else { gone };
    } else {
        teeth = vec![t];
        if tool.uses_faces() {
            if let Some(f) = face {
                faces.push(FACES[f].to_string());
            } else {
                for (i, on) in app.odg.faces.iter().enumerate() {
                    if *on {
                        faces.push(FACES[i].to_string());
                    }
                }
            }
            if faces.is_empty() {
                faces.push("O".into());
            }
            // código de obturación según el número de caras (solo entre obturaciones del catálogo)
            if tool == OdgKind::Obturacion {
                app.odg.code = app.db.filling_code(&code, faces.len());
            }
        }
    }
    let code = app.odg.code.clone();
    // previsto o realizado, con la fecha de hoy (la de cuando se apunta)
    let date = today;
    // si ya estaba previsto en esa pieza (presupuesto), se completa ese
    let (id, reused) = app.db.chart_entry(pid, &code, teeth.clone(), faces, state, doc, date);
    app.changed();
    let e = app.db.entries.iter().find(|e| e.id == id).cloned();
    if let Some(e) = e {
        let teeth_s: Vec<String> = teeth.iter().map(|x| x.to_string()).collect();
        let what = if reused { "Hecho el previsto:" } else { state.label() };
        app.notify(format!("{what} {} en pieza {} · {}", e.description, teeth_s.join("-"), money(e.amount())));
    }
}

fn side_panel(app: &mut App, ui: &mut Ui, pid: u32, entries: &[MedEntry]) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        match app.odg.selected_tooth {
            Some(t) => {
                section(ui, &format!("Pieza {t}"));
                let list: Vec<&MedEntry> = entries.iter().filter(|e| e.teeth.contains(&t)).collect();
                if list.is_empty() {
                    muted(ui, "Sin tratamientos en esta pieza.");
                }
                let mut realize = None;
                let mut delete = None;
                let mut undo = None;
                for e in list {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, state_color(e.state));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&e.description).size(14.5));
                            let f = if e.faces.is_empty() { String::new() } else { format!(" · caras {}", e.faces.join("")) };
                            muted(ui, format!("{} · {}{} · {}", fmt_date(e.date), e.state.label(), f, money(e.amount())));
                        });
                        right_aligned(ui, |ui| {
                            if ui.small_button("✖").on_hover_text("Borrar").clicked() {
                                delete = Some(e.id);
                            }
                            if e.state == TxState::Previsto && ui.small_button("✔").on_hover_text("Marcar como realizado hoy").clicked() {
                                realize = Some(e.id);
                            }
                            if e.state == TxState::Realizado && e.invoice_id.is_none() && !e.settled_elsewhere && ui.small_button("↶").on_hover_text("Volver a previsto (se marcó por error)").clicked() {
                                undo = Some(e.id);
                            }
                        });
                    });
                }
                // imágenes de la pieza (radiografías, TAC, fotos)
                let imgs: Vec<(u32, String)> = app.db.images.iter().filter(|i| i.patient_id == pid && i.teeth.contains(&t)).map(|i| (i.id, format!("{} · {}", i.kind.short(), fmt_date(i.date)))).collect();
                if !imgs.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Imágenes de la pieza").family(crate::theme::sb()).size(14.0));
                    for (iid, label) in imgs {
                        if ui.link(format!("📷 {label}")).clicked() {
                            app.img.selected = Some(iid);
                            app.pat_tab = crate::app::PatTab::Imagenes;
                        }
                    }
                }
                if let Some(id) = realize {
                    let doc = app.odg.doctor;
                    app.db.realize_entry(id, app.today, doc);
                    app.changed();
                    app.notify("Tratamiento realizado y cargado en la cuenta del paciente.");
                }
                if let Some(id) = delete {
                    crate::screens::ask_delete_entry(app, id);
                }
                if let Some(id) = undo {
                    match app.db.unrealize_entry(id) {
                        Ok(()) => {
                            app.changed();
                            app.notify("Tratamiento vuelto a previsto: el material vuelve al almacén.");
                        }
                        Err(e) => app.warn(e),
                    }
                }
            }
            None => {
                section(ui, "Resumen de la boca");
                muted(ui, "Pulse una pieza para ver su detalle.");
            }
        }
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let planned: Vec<&MedEntry> = entries.iter().filter(|e| e.state == TxState::Previsto).collect();
        let unbudgeted = planned.iter().filter(|e| e.budget_id.is_none()).count();
        section(ui, "Tratamientos previstos");
        let total: f64 = planned.iter().map(|e| e.amount()).sum();
        ui.label(RichText::new(format!("{} tratamientos · {}", planned.len(), money(total))).color(MUTED));
        ui.add_space(4.0);
        egui::ScrollArea::vertical().max_height(180.0).id_salt("odg_planned").show(ui, |ui| {
            for e in &planned {
                let teeth: Vec<String> = e.teeth.iter().map(|x| x.to_string()).collect();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} {}", e.description, teeth.join("-"))).size(14.0));
                    right_aligned(ui, |ui| {
                        ui.label(RichText::new(money(e.amount())).size(14.0));
                        if e.budget_id.is_some() {
                            ui.label(RichText::new("📋").size(12.5)).on_hover_text("Ya presupuestado");
                        }
                    });
                });
            }
        });
        ui.add_space(6.0);
        ui.add_enabled_ui(unbudgeted > 0, |ui| {
            if primary(ui, &format!("Crear presupuesto con previstos ({unbudgeted})")).clicked()
                && let Some(bid) = app.db.budget_from_planned(pid)
            {
                app.changed();
                app.dialog = crate::app::Dialog::Budget(bid);
                app.notify("Presupuesto creado a partir de los tratamientos previstos.");
            }
        });
    });
}
