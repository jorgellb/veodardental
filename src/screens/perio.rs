//! Periodontograma: sondaje de 6 puntos por pieza, recesión, sangrado,
//! placa, movilidad y furca, con comparación frente al sondaje anterior.
//!
//! Se dibuja como la ficha periodontal clásica: por cada arcada, las piezas a
//! escala vistas por vestibular y por palatino/lingual sobre una rejilla de
//! milímetros, con la línea del margen gingival, la del fondo de la bolsa y el
//! área entre ambas coloreada según la profundidad; arriba y abajo, las filas
//! de datos de cada punto de sondaje. Al pulsar una pieza se abre su ficha
//! para editarla junto al gráfico.

use crate::app::App;
use crate::model::*;
use crate::screens::odontograma::{LOWER_ADULT, UPPER_ADULT};
use crate::screens::teeth;
use crate::theme::*;
use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Align2, Color32, CornerRadius, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

/// Línea del margen gingival.
const MARGIN_C: Color32 = Color32::from_rgb(225, 29, 72);
/// Línea del fondo de la bolsa.
const POCKET_C: Color32 = Color32::from_rgb(37, 99, 235);

fn depth_color(d: u8) -> Color32 {
    match d {
        0..=3 => OK,
        4..=5 => WARN,
        _ => DANGER,
    }
}

/// Nombre de la pieza en palabras (FDI).
pub fn tooth_name(t: u8) -> String {
    let kind = match t % 10 {
        1 => "Incisivo central",
        2 => "Incisivo lateral",
        3 => "Canino",
        4 => "Primer premolar",
        5 => "Segundo premolar",
        6 => "Primer molar",
        7 => "Segundo molar",
        8 => "Tercer molar",
        _ => "Pieza",
    };
    let side = match t / 10 {
        1 => "superior derecho",
        2 => "superior izquierdo",
        3 => "inferior izquierdo",
        4 => "inferior derecho",
        _ => "",
    };
    format!("{kind} {side}")
}

pub fn show(app: &mut App, ui: &mut Ui, pid: u32) {
    let charts: Vec<(u32, chrono::NaiveDate)> = app.db.patient_perio(pid).iter().map(|c| (c.id, c.date)).collect();
    if app.lists.perio_sel.map(|s| !charts.iter().any(|c| c.0 == s)).unwrap_or(true) {
        app.lists.perio_sel = charts.last().map(|c| c.0);
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Periodontograma");
            ui.add_space(10.0);
            for (id, d) in &charts {
                if ui.selectable_label(app.lists.perio_sel == Some(*id), fmt_date(*d)).clicked() {
                    app.lists.perio_sel = Some(*id);
                }
            }
            right_aligned(ui, |ui| {
                if primary(ui, "＋ Nuevo sondaje").on_hover_text("Parte del último sondaje para editar sólo lo que cambia").clicked() {
                    let doc = app.db.patient(pid).map(|p| p.doctor_id).unwrap_or(1);
                    let id = app.db.new_perio(pid, doc);
                    app.lists.perio_sel = Some(id);
                    app.changed();
                    app.notify("Nuevo periodontograma creado a partir del anterior.");
                }
            });
        });
    });
    let Some(sel) = app.lists.perio_sel else {
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            empty_state(ui, "Este paciente no tiene periodontogramas. Pulse «Nuevo sondaje» para empezar.");
        });
        return;
    };
    let chart = app.db.perio.iter().find(|c| c.id == sel).cloned().unwrap();
    let prev = app.db.patient_perio(pid).into_iter().filter(|c| (c.date, c.id) < (chart.date, chart.id)).last().cloned();
    let st = chart.stats();
    let pst = prev.as_ref().map(|p| p.stats());
    ui.add_space(8.0);
    let w = kpi_w(ui, 5);
    let delta = |now: f32, before: Option<f32>, unit: &str| -> String {
        match before {
            Some(b) => {
                let d = now - b;
                if d.abs() < 0.05 { "sin cambios".into() } else { format!("{}{:.1}{unit} vs anterior", if d > 0.0 { "+" } else { "" }, d) }
            }
            None => "primer sondaje".into(),
        }
    };
    let better = |now: f32, before: Option<f32>, bad: Color32| match before {
        Some(b) if now < b - 0.05 => OK,
        Some(b) if now > b + 0.05 => DANGER,
        _ => bad,
    };
    ui.horizontal(|ui| {
        kpi(ui, w, "Profundidad media", &format!("{:.1} mm", st.mean_depth), &format!("{} · {} puntos", delta(st.mean_depth, pst.map(|p| p.mean_depth), " mm"), st.sites), better(st.mean_depth, pst.map(|p| p.mean_depth), if st.mean_depth >= 3.5 { WARN } else { OK }));
        ui.add_space(4.0);
        kpi(ui, w, "Bolsas ≥ 4 mm", &st.pockets4.to_string(), &delta(st.pockets4 as f32, pst.map(|p| p.pockets4 as f32), ""), better(st.pockets4 as f32, pst.map(|p| p.pockets4 as f32), WARN));
        ui.add_space(4.0);
        kpi(ui, w, "Bolsas ≥ 6 mm", &st.pockets6.to_string(), &delta(st.pockets6 as f32, pst.map(|p| p.pockets6 as f32), ""), better(st.pockets6 as f32, pst.map(|p| p.pockets6 as f32), DANGER));
        ui.add_space(4.0);
        kpi(ui, w, "Sangrado al sondaje", &format!("{:.0} %", st.bop_pct), &delta(st.bop_pct, pst.map(|p| p.bop_pct), " %"), better(st.bop_pct, pst.map(|p| p.bop_pct), DANGER));
        ui.add_space(4.0);
        kpi(ui, w, "Índice de placa", &format!("{:.0} %", st.plaque_pct), &format!("{} piezas con movilidad", st.mobile_teeth), INFO);
    });
    ui.add_space(8.0);
    let mut clicked = None;
    let mut anchor = None;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        legend(ui, prev.is_some());
        ui.add_space(6.0);
        for (list, upper) in [(&UPPER_ADULT, true), (&LOWER_ADULT, false)] {
            let (c, a) = arch_block(ui, list, upper, &chart, prev.as_ref(), app.lists.perio_tooth);
            if c.is_some() {
                clicked = c;
            }
            if a.is_some() {
                anchor = a;
            }
            ui.add_space(10.0);
        }
    });
    if let Some(t) = clicked {
        app.lists.perio_tooth = if app.lists.perio_tooth == Some(t) { None } else { Some(t) };
    }
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Notas del sondaje");
        if let Some(c) = app.db.perio.iter_mut().find(|c| c.id == sel) {
            if ui.add(egui::TextEdit::multiline(&mut c.notes).desired_rows(3).desired_width(f32::INFINITY)).changed() {
                app.dirty = true;
            }
        }
    });
    if let (Some(t), Some(col)) = (app.lists.perio_tooth, anchor) {
        editor_popover(app, ui, sel, t, col);
    }
}

/// Leyenda del gráfico.
fn legend(ui: &mut Ui, with_prev: bool) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
        let line = |ui: &mut Ui, c: Color32, text: &str, dashed: bool| {
            let (r, _) = ui.allocate_exact_size(vec2(22.0, 14.0), Sense::hover());
            if dashed {
                ui.painter().extend(Shape::dashed_line(&[r.left_center(), r.right_center()], Stroke::new(1.6, c), 4.0, 3.0));
            } else {
                ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(2.2, c));
            }
            ui.label(RichText::new(text).font(mdf(12.8)).color(TEXT));
            ui.add_space(6.0);
        };
        line(ui, MARGIN_C, "Margen gingival", false);
        line(ui, POCKET_C, "Fondo de bolsa", false);
        if with_prev {
            line(ui, Color32::from_rgb(120, 132, 150), "Sondaje anterior", true);
        }
        let dot = |ui: &mut Ui, c: Color32, text: &str| {
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 14.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 4.0, c);
            ui.label(RichText::new(text).font(mdf(12.8)).color(TEXT));
            ui.add_space(6.0);
        };
        dot(ui, DANGER, "Sangrado");
        dot(ui, INFO, "Placa");
        ui.add_space(8.0);
        for (c, n) in [(OK, "≤ 3 mm"), (WARN, "4–5 mm"), (DANGER, "≥ 6 mm")] {
            pill(ui, n, c);
        }
        right_aligned(ui, |ui| {
            ui.label(RichText::new("Pulse una pieza para editarla").font(mdf(12.5)).color(SUBTLE));
        });
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Mobility,
    Furcation,
    Bleeding,
    Plaque,
    Recession,
    Depth,
}

impl Row {
    fn label(self) -> &'static str {
        match self {
            Row::Mobility => "Movilidad",
            Row::Furcation => "Furca",
            Row::Bleeding => "Sangrado",
            Row::Plaque => "Placa",
            Row::Recession => "Recesión",
            Row::Depth => "Sondaje",
        }
    }
}

const ROW_H: f32 = 17.0;
const LABEL_W: f32 = 112.0;
const MID_GAP: f32 = 10.0;

/// Furca: sólo en molares y primeros premolares superiores.
fn has_furcation(t: u8) -> bool {
    let u = t % 10;
    u >= 6 || (matches!(t / 10, 1 | 2) && u == 4)
}

/// Una arcada completa: tablas, piezas por vestibular y por palatino/lingual.
/// Devuelve la pieza pulsada y el rectángulo de la columna elegida.
fn arch_block(ui: &mut Ui, arch: &[u8], upper: bool, chart: &PerioChart, prev: Option<&PerioChart>, sel: Option<u8>) -> (Option<u8>, Option<Rect>) {
    use Row::*;
    let width = ui.available_width();
    let tw = ((width - LABEL_W - MID_GAP - 4.0) / arch.len() as f32).floor();
    let tooth_w = tw - 6.0;
    let tooth_h = (tooth_w * 2.4).clamp(90.0, 150.0);
    let band_h = tooth_h + 8.0;
    let full: [Row; 6] = [Mobility, Furcation, Bleeding, Plaque, Recession, Depth];
    let short: [Row; 4] = [Bleeding, Plaque, Recession, Depth];
    // filas de fuera hacia dentro (las de abajo se leen al revés)
    let (top_rows, bottom_rows): (Vec<Row>, Vec<Row>) = if upper {
        (full.to_vec(), short.iter().rev().cloned().collect())
    } else {
        (short.to_vec(), full.iter().rev().cloned().collect())
    };
    let title_h = 22.0;
    let num_h = 26.0;
    let h = title_h + top_rows.len() as f32 * ROW_H + 4.0 + band_h + num_h + band_h + 4.0 + bottom_rows.len() as f32 * ROW_H;
    let (rect, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    let p = ui.painter().with_clip_rect(rect.expand(2.0));
    let x_of = |i: usize| rect.min.x + LABEL_W + i as f32 * tw + if i >= arch.len() / 2 { MID_GAP } else { 0.0 };
    let mut y = rect.min.y;
    // título
    p.text(pos2(rect.min.x, y + 9.0), Align2::LEFT_CENTER, if upper { "ARCADA SUPERIOR" } else { "ARCADA INFERIOR" }, sbf(11.5), MUTED);
    y += title_h;
    let top_table = Rect::from_min_size(pos2(rect.min.x, y), vec2(width, top_rows.len() as f32 * ROW_H));
    y = top_table.max.y + 4.0;
    let band1 = Rect::from_min_size(pos2(rect.min.x, y), vec2(width, band_h));
    y = band1.max.y;
    let nums = Rect::from_min_size(pos2(rect.min.x, y), vec2(width, num_h));
    y = nums.max.y;
    let band2 = Rect::from_min_size(pos2(rect.min.x, y), vec2(width, band_h));
    y = band2.max.y + 4.0;
    let bottom_table = Rect::from_min_size(pos2(rect.min.x, y), vec2(width, bottom_rows.len() as f32 * ROW_H));
    // vestibular arriba en la superior; lingual arriba en la inferior
    let (top_side, bottom_side) = if upper { (0usize, 3usize) } else { (3usize, 0usize) };

    // columnas: interacción y fondo
    let mut clicked = None;
    let mut anchor = None;
    for (i, &t) in arch.iter().enumerate() {
        let col = Rect::from_min_max(pos2(x_of(i), top_table.min.y - 2.0), pos2(x_of(i) + tw, bottom_table.max.y + 2.0));
        let resp = ui.interact(col, ui.scope_id().with(("perio_col", t, upper)), Sense::click());
        if sel == Some(t) {
            p.rect_filled(col, CornerRadius::same(8), Color32::from_rgb(236, 248, 246));
            p.rect_stroke(col, CornerRadius::same(8), Stroke::new(1.2, Color32::from_rgb(94, 196, 184)), StrokeKind::Inside);
            anchor = Some(col);
        } else if resp.hovered() {
            p.rect_filled(col, CornerRadius::same(8), Color32::from_rgb(246, 249, 251));
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            clicked = Some(t);
        }
    }
    // tablas
    table(&p, top_table, &top_rows, arch, chart, top_side, &x_of, tw);
    table(&p, bottom_table, &bottom_rows, arch, chart, bottom_side, &x_of, tw);
    // bandas de dibujo
    band(&p, band1, arch, upper, chart, prev, top_side, &x_of, tw, tooth_w, tooth_h, false, if upper { "Vestibular" } else { "Lingual" });
    band(&p, band2, arch, upper, chart, prev, bottom_side, &x_of, tw, tooth_w, tooth_h, true, if upper { "Palatino" } else { "Vestibular" });
    // números de las piezas
    for (i, &t) in arch.iter().enumerate() {
        let c = pos2(x_of(i) + tw / 2.0, nums.center().y);
        let missing = chart.teeth.iter().find(|x| x.tooth == t).map(|x| x.missing).unwrap_or(true);
        if sel == Some(t) {
            p.rect_filled(Rect::from_center_size(c, vec2(30.0, 20.0)), CornerRadius::same(10), ACCENT);
            p.text(c, Align2::CENTER_CENTER, t.to_string(), sbf(13.0), Color32::WHITE);
        } else {
            p.text(c, Align2::CENTER_CENTER, t.to_string(), sbf(13.0), if missing { SUBTLE } else { HEADING });
        }
    }
    // línea media
    let mx = x_of(arch.len() / 2) - MID_GAP / 2.0;
    p.extend(Shape::dashed_line(&[pos2(mx, top_table.min.y), pos2(mx, bottom_table.max.y)], Stroke::new(1.0, Color32::from_rgb(214, 222, 232)), 5.0, 4.0));
    (clicked, anchor)
}

/// Puntos de sondaje en el orden en que se ven de izquierda a derecha
/// (los datos van distal, centro, mesial).
fn screen_sites(t: u8) -> [usize; 3] {
    if teeth::mesial_right(t) { [0, 1, 2] } else { [2, 1, 0] }
}

#[allow(clippy::too_many_arguments)]
fn table(p: &Painter, r: Rect, rows: &[Row], arch: &[u8], chart: &PerioChart, side: usize, x_of: &dyn Fn(usize) -> f32, tw: f32) {
    for (k, row) in rows.iter().enumerate() {
        let ry = r.min.y + k as f32 * ROW_H;
        let rr = Rect::from_min_size(pos2(r.min.x, ry), vec2(r.width(), ROW_H));
        if k % 2 == 0 {
            p.rect_filled(Rect::from_min_max(pos2(rr.min.x + LABEL_W - 4.0, rr.min.y), rr.max), CornerRadius::same(4), Color32::from_rgba_unmultiplied(241, 245, 249, 150));
        }
        p.text(pos2(rr.min.x + 2.0, rr.center().y), Align2::LEFT_CENTER, row.label(), mdf(11.8), MUTED);
        for (i, &t) in arch.iter().enumerate() {
            let Some(pt) = chart.teeth.iter().find(|x| x.tooth == t) else { continue };
            if pt.missing {
                continue;
            }
            let cx = x_of(i) + tw / 2.0;
            match row {
                Row::Mobility => {
                    let col = if pt.mobility > 0 { INFO } else { Color32::from_rgb(196, 204, 216) };
                    p.text(pos2(cx, rr.center().y), Align2::CENTER_CENTER, pt.mobility.to_string(), sbf(11.5), col);
                }
                Row::Furcation => {
                    if has_furcation(t) {
                        let txt = ["0", "I", "II", "III"].get(pt.furcation as usize).copied().unwrap_or("?");
                        let col = if pt.furcation > 0 { WARN } else { Color32::from_rgb(196, 204, 216) };
                        if pt.furcation > 0 {
                            p.rect_filled(Rect::from_center_size(pos2(cx, rr.center().y), vec2(24.0, 14.0)), CornerRadius::same(7), tint(WARN, 0.14));
                        }
                        p.text(pos2(cx, rr.center().y), Align2::CENTER_CENTER, txt, sbf(11.0), col);
                    }
                }
                _ => {
                    for (k2, &s) in screen_sites(t).iter().enumerate() {
                        let idx = side + s;
                        let sx = cx + (k2 as f32 - 1.0) * tw * 0.29;
                        let c = pos2(sx, rr.center().y);
                        match row {
                            Row::Bleeding => {
                                if pt.bleeding[idx] {
                                    p.circle_filled(c, 3.6, DANGER);
                                } else {
                                    p.circle_stroke(c, 2.6, Stroke::new(1.0, Color32::from_rgb(214, 222, 232)));
                                }
                            }
                            Row::Plaque => {
                                if pt.plaque[idx] {
                                    p.circle_filled(c, 3.6, INFO);
                                } else {
                                    p.circle_stroke(c, 2.6, Stroke::new(1.0, Color32::from_rgb(214, 222, 232)));
                                }
                            }
                            Row::Recession => {
                                let v = pt.recession[idx];
                                p.text(c, Align2::CENTER_CENTER, v.to_string(), mdf(11.3), if v > 0 { mix(WARN, HEADING, 0.3) } else { Color32::from_rgb(170, 180, 194) });
                            }
                            _ => {
                                let d = pt.depth[idx];
                                let font = if d >= 4 { bf(11.8) } else { mdf(11.3) };
                                p.text(c, Align2::CENTER_CENTER, d.to_string(), font, if d >= 4 { depth_color(d) } else { TEXT });
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Un punto de sondaje ya situado en pantalla.
#[derive(Clone, Copy)]
struct Site {
    x: f32,
    margin: f32,
    pocket: f32,
    depth: u8,
    bleeding: bool,
}

/// Una banda: piezas a escala, rejilla de milímetros, encía, bolsas y líneas.
#[allow(clippy::too_many_arguments)]
fn band(p: &Painter, r: Rect, arch: &[u8], _upper: bool, chart: &PerioChart, prev: Option<&PerioChart>, side: usize, x_of: &dyn Fn(usize) -> f32, tw: f32, tooth_w: f32, tooth_h: f32, second: bool, label: &str) {
    let look = &teeth::LIGHT;
    // en la superior la primera banda tiene las raíces arriba; en la inferior, la segunda
    let pose_of = |i: usize, t: u8| {
        let rect = Rect::from_min_size(pos2(x_of(i) + (tw - tooth_w) / 2.0, r.min.y + 4.0), vec2(tooth_w, tooth_h));
        // la banda de arriba lleva las raíces arriba y la de abajo, abajo: las coronas se miran
        teeth::Pose { up: !second, ..teeth::Pose::new(t, rect) }
    };
    p.text(pos2(r.min.x + 2.0, r.center().y), Align2::LEFT_CENTER, label, sbf(12.0), HEADING);
    let Some(first) = arch.first() else { return };
    let p0 = pose_of(0, *first);
    let mm = teeth::mm(&p0);
    let ap = p0.apical();
    let cej_y = p0.y(teeth::CROWN_H);
    let x_start = r.min.x + LABEL_W - 4.0;
    let x_end = r.max.x;
    // rejilla de milímetros (detrás de las piezas)
    let grid_line = |p: &Painter, alpha: f32| {
        for k in 0..=12 {
            let yy = cej_y + ap * k as f32 * mm;
            if !r.y_range().contains(yy) {
                continue;
            }
            let strong = k % 3 == 0;
            let c = if k == 0 { Color32::from_rgba_unmultiplied(100, 116, 139, (alpha * 120.0) as u8) } else { Color32::from_rgba_unmultiplied(148, 163, 184, (alpha * if strong { 90.0 } else { 45.0 }) as u8) };
            p.line_segment([pos2(x_start, yy), pos2(x_end, yy)], Stroke::new(if k == 0 { 1.0 } else { 0.7 }, c));
        }
    };
    grid_line(p, 1.0);
    // escala a la izquierda (0, 3, 6, 9, 12 mm)
    for k in [3, 6, 9, 12] {
        let yy = cej_y + ap * k as f32 * mm;
        if r.y_range().contains(yy) {
            p.text(pos2(x_start - 6.0, yy), Align2::RIGHT_CENTER, format!("{k}"), mdf(9.5), SUBTLE);
        }
    }
    // piezas
    for (i, &t) in arch.iter().enumerate() {
        let pose = pose_of(i, t);
        let missing = chart.teeth.iter().find(|x| x.tooth == t).map(|x| x.missing).unwrap_or(true);
        if missing {
            teeth::draw_ghost(p, &pose, look, true);
        } else {
            teeth::draw_tooth(p, &pose, look, true);
        }
    }
    // la rejilla se ve también, muy suave, sobre las raíces
    grid_line(p, 0.45);
    // puntos de sondaje, por tramos continuos (se cortan en las piezas ausentes)
    let sites_of = |c: &PerioChart| -> Vec<Vec<Site>> {
        let mut runs: Vec<Vec<Site>> = vec![];
        let mut cur: Vec<Site> = vec![];
        for (i, &t) in arch.iter().enumerate() {
            match c.teeth.iter().find(|x| x.tooth == t).filter(|x| !x.missing) {
                Some(pt) => {
                    let pose = pose_of(i, t);
                    let m = pose.mesial();
                    for s in screen_sites(t) {
                        let a = match s {
                            0 => -m * 0.80,
                            1 => 0.0,
                            _ => m * 0.80,
                        };
                        let cej = teeth::cej_pt(&pose, a);
                        let idx = side + s;
                        let margin = cej.y + ap * pt.recession[idx] as f32 * mm;
                        let pocket = margin + ap * pt.depth[idx] as f32 * mm;
                        cur.push(Site { x: cej.x, margin, pocket, depth: pt.depth[idx], bleeding: pt.bleeding[idx] });
                    }
                }
                None => {
                    if !cur.is_empty() {
                        runs.push(std::mem::take(&mut cur));
                    }
                }
            }
        }
        if !cur.is_empty() {
            runs.push(cur);
        }
        runs
    };
    let runs = sites_of(chart);
    let clip = p.with_clip_rect(r);
    for run in &runs {
        // encía: desde el margen hacia el ápice, desvaneciéndose
        let mut m = Mesh::default();
        for s in run {
            m.vertices.push(Vertex { pos: pos2(s.x, s.margin), uv: WHITE_UV, color: Color32::from_rgba_premultiplied(74, 38, 46, 80) });
            m.vertices.push(Vertex { pos: pos2(s.x, s.margin + ap * 5.0 * mm), uv: WHITE_UV, color: Color32::TRANSPARENT });
        }
        for k in 0..run.len().saturating_sub(1) as u32 {
            let i = k * 2;
            m.add_triangle(i, i + 1, i + 2);
            m.add_triangle(i + 1, i + 3, i + 2);
        }
        clip.add(Shape::mesh(m));
        // bolsa: área entre el margen y el fondo, coloreada por profundidad
        let mut b = Mesh::default();
        for w in run.windows(2) {
            let d = w[0].depth.max(w[1].depth);
            let col = match d {
                0..=3 => Color32::from_rgba_unmultiplied(37, 99, 235, 46),
                4..=5 => Color32::from_rgba_unmultiplied(217, 119, 6, 72),
                _ => Color32::from_rgba_unmultiplied(220, 38, 38, 78),
            };
            let base = b.vertices.len() as u32;
            for q in [pos2(w[0].x, w[0].margin), pos2(w[1].x, w[1].margin), pos2(w[1].x, w[1].pocket), pos2(w[0].x, w[0].pocket)] {
                b.vertices.push(Vertex { pos: q, uv: WHITE_UV, color: col });
            }
            b.add_triangle(base, base + 1, base + 2);
            b.add_triangle(base, base + 2, base + 3);
        }
        clip.add(Shape::mesh(b));
    }
    // sondaje anterior (discontinuo)
    if let Some(pc) = prev {
        for run in sites_of(pc) {
            let pts: Vec<Pos2> = run.iter().map(|s| pos2(s.x, s.pocket)).collect();
            if pts.len() > 1 {
                clip.extend(Shape::dashed_line(&pts, Stroke::new(1.3, Color32::from_rgb(120, 132, 150)), 4.0, 3.0));
            }
        }
    }
    for run in &runs {
        let margin: Vec<Pos2> = run.iter().map(|s| pos2(s.x, s.margin)).collect();
        let pocket: Vec<Pos2> = run.iter().map(|s| pos2(s.x, s.pocket)).collect();
        clip.add(Shape::line(margin, Stroke::new(2.0, MARGIN_C)));
        clip.add(Shape::line(pocket, Stroke::new(2.0, POCKET_C)));
        for s in run {
            if s.depth >= 4 {
                clip.circle_filled(pos2(s.x, s.pocket), 3.2, depth_color(s.depth));
                clip.circle_stroke(pos2(s.x, s.pocket), 3.2, Stroke::new(1.0, Color32::WHITE));
            }
            if s.bleeding {
                clip.circle_filled(pos2(s.x, s.margin), 3.0, DANGER);
                clip.circle_stroke(pos2(s.x, s.margin), 3.0, Stroke::new(1.0, Color32::WHITE));
            }
        }
    }
}

/// Ficha de la pieza elegida, flotando junto a su columna.
fn editor_popover(app: &mut App, ui: &mut Ui, sel: u32, t: u8, col: Rect) {
    let screen = ui.ctx().content_rect();
    let w = 340.0;
    let x = if col.center().x < screen.center().x { col.max.x + 10.0 } else { col.min.x - 10.0 - w };
    let y = (col.min.y + 30.0).max(screen.min.y + 8.0);
    let mut close = false;
    let mut changed = false;
    egui::Area::new(egui::Id::unique("perio_editor")).order(egui::Order::Foreground).fixed_pos(pos2(x, y)).constrain(true).show(ui.ctx(), |ui| {
        egui::Frame::new()
            .fill(CARD)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(CornerRadius::same(16))
            .inner_margin(egui::Margin::symmetric(18, 16))
            .shadow(egui::Shadow { offset: [0, 14], blur: 40, spread: 0, color: Color32::from_rgba_unmultiplied(15, 30, 60, 46) })
            .show(ui, |ui| {
                ui.set_width(w - 36.0);
                let Some(chart) = app.db.perio.iter_mut().find(|c| c.id == sel) else { return };
                let Some(pt) = chart.teeth.iter_mut().find(|x| x.tooth == t) else { return };
                let before = serde_json::to_string(&*pt).unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(RichText::new(format!("Pieza {t}")).font(bf(19.0)).color(HEADING));
                        ui.label(RichText::new(tooth_name(t)).font(mdf(12.5)).color(MUTED));
                    });
                    right_aligned(ui, |ui| {
                        if ui.add(egui::Button::new(RichText::new("✕").size(13.0)).min_size(vec2(30.0, 30.0)).corner_radius(CornerRadius::same(15))).on_hover_text("Cerrar").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.checkbox(&mut pt.missing, "Pieza ausente");
                if !pt.missing {
                    let sites = ["Distal", "Centro", "Mesial"];
                    let upper = teeth::is_upper(t);
                    for (band, name) in [(0usize, "Vestibular"), (3usize, if upper { "Palatino" } else { "Lingual" })] {
                        ui.add_space(6.0);
                        ui.label(RichText::new(name).font(sbf(13.5)).color(HEADING));
                        egui::Grid::new(("pg", band)).num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
                            ui.label("");
                            for s in sites {
                                ui.label(RichText::new(s).font(mdf(11.5)).color(MUTED));
                            }
                            ui.end_row();
                            ui.label(RichText::new("Sondaje").font(mdf(12.5)));
                            for s in 0..3 {
                                ui.add(egui::DragValue::new(&mut pt.depth[band + s]).range(0..=15).suffix(" mm"));
                            }
                            ui.end_row();
                            ui.label(RichText::new("Recesión").font(mdf(12.5)));
                            for s in 0..3 {
                                ui.add(egui::DragValue::new(&mut pt.recession[band + s]).range(0..=15).suffix(" mm"));
                            }
                            ui.end_row();
                            ui.label(RichText::new("Sangrado").font(mdf(12.5)));
                            for s in 0..3 {
                                ui.checkbox(&mut pt.bleeding[band + s], "");
                            }
                            ui.end_row();
                            ui.label(RichText::new("Placa").font(mdf(12.5)));
                            for s in 0..3 {
                                ui.checkbox(&mut pt.plaque[band + s], "");
                            }
                            ui.end_row();
                        });
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Movilidad").font(mdf(12.5)));
                        ui.add(egui::DragValue::new(&mut pt.mobility).range(0..=3));
                        if has_furcation(t) {
                            ui.add_space(8.0);
                            ui.label(RichText::new("Furca").font(mdf(12.5)));
                            ui.add(egui::DragValue::new(&mut pt.furcation).range(0..=3));
                        }
                    });
                    let ins: Vec<String> = (0..6).map(|i| pt.attachment(i).to_string()).collect();
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Nivel de inserción (mm): {}", ins.join(" · "))).font(mdf(12.0)).color(MUTED));
                }
                changed = serde_json::to_string(&*pt).unwrap_or_default() != before;
            });
    });
    if changed {
        app.changed();
    }
    if close || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.lists.perio_tooth = None;
    }
}
