//! Pintura del odontograma: piezas, encía, tratamientos, arcadas y botones,
//! en aspecto claro (por defecto) u oscuro tipo radiografía.

use crate::model::*;
use crate::screens::odontograma::{ToothView, state_color};
use crate::screens::teeth;
use crate::theme::*;
use egui::{Align2, Color32, CornerRadius, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

/// Colores del panel del odontograma (claro u oscuro).
pub struct Skin {
    pub bg: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub tile: Color32,
    pub tile_hover: Color32,
    pub tile_sel: Color32,
    pub tile_sel_line: Color32,
    pub tile_text: Color32,
    pub num: Color32,
    pub divider: Color32,
    pub sel_cell: Color32,
    pub sel_line: Color32,
    pub hover_cell: Color32,
    pub guide: Color32,
}

pub fn skin(dark: bool) -> Skin {
    if dark {
        Skin {
            bg: Color32::from_rgb(6, 8, 13),
            border: Color32::from_rgb(28, 34, 46),
            text: Color32::WHITE,
            muted: Color32::from_rgb(130, 146, 168),
            tile: Color32::from_rgb(18, 22, 30),
            tile_hover: Color32::from_rgb(30, 38, 52),
            tile_sel: Color32::from_rgb(16, 64, 112),
            tile_sel_line: Color32::from_rgb(110, 180, 255),
            tile_text: Color32::from_rgb(214, 222, 234),
            num: Color32::from_rgb(140, 156, 178),
            divider: Color32::from_rgb(40, 50, 66),
            sel_cell: Color32::from_rgb(18, 36, 60),
            sel_line: Color32::from_rgb(80, 150, 230),
            hover_cell: Color32::from_rgb(14, 22, 34),
            guide: Color32::from_rgba_unmultiplied(110, 180, 255, 190),
        }
    } else {
        Skin {
            bg: CARD,
            border: BORDER,
            text: HEADING,
            muted: MUTED,
            tile: Color32::from_rgb(248, 250, 252),
            tile_hover: Color32::from_rgb(240, 244, 248),
            tile_sel: ACCENT_SOFT,
            tile_sel_line: ACCENT,
            tile_text: TEXT,
            num: Color32::from_rgb(100, 116, 139),
            divider: Color32::from_rgb(226, 232, 240),
            sel_cell: Color32::from_rgb(236, 248, 246),
            sel_line: Color32::from_rgb(94, 196, 184),
            hover_cell: Color32::from_rgb(246, 249, 251),
            guide: Color32::from_rgba_unmultiplied(13, 148, 136, 200),
        }
    }
}

pub fn look(dark: bool) -> &'static teeth::Look {
    if dark { &teeth::DARK } else { &teeth::LIGHT }
}

/// La pieza ya no está en boca (ausente o extraída).
pub fn gone(v: &ToothView) -> bool {
    v.absent.is_some() || matches!(v.extracted, Some(TxState::Realizado) | Some(TxState::Previo))
}

fn edge_of(c: Color32) -> Stroke {
    Stroke::new(1.0, teeth::lerp_c(c, Color32::BLACK, 0.3))
}

/// Aspa (extracción prevista) con un halo para que se lea sobre cualquier fondo.
fn cross(p: &Painter, r: Rect, c: Color32, w: f32, look: &teeth::Look) {
    let lines = [(r.left_top(), r.right_bottom()), (r.right_top(), r.left_bottom())];
    for (a, b) in lines {
        p.line_segment([a, b], Stroke::new(w + 2.5, teeth::with_alpha(look.bg, 0.85)));
    }
    for (a, b) in lines {
        p.line_segment([a, b], Stroke::new(w, c));
        p.circle_filled(a, w / 2.0, c);
        p.circle_filled(b, w / 2.0, c);
    }
}

/// Pieza en vista vestibular, sin tratamientos (o su hueco si falta).
pub fn side_base(p: &Painter, pose: &teeth::Pose, v: &ToothView, look: &teeth::Look) {
    if let Some(s) = v.implant {
        teeth::draw_implant(p, pose, look, state_color(s));
        teeth::draw_crown(p, pose, look, None);
        return;
    }
    if gone(v) {
        if v.bridge.is_none() {
            teeth::draw_ghost(p, pose, look, true);
        }
        return;
    }
    teeth::draw_tooth(p, pose, look, true);
}

/// Tratamientos sobre la vista vestibular.
pub fn side_marks(p: &Painter, pose: &teeth::Pose, v: &ToothView, look: &teeth::Look, gap: f32, links: (bool, bool)) {
    let w = pose.w();
    let m = pose.mesial();
    let present = !gone(v) || v.implant.is_some();
    if let Some(s) = v.apico {
        teeth::apex_cut(p, pose, look, state_color(s));
    }
    if let Some(s) = v.endo {
        let c = state_color(s);
        let lw = (w * 0.07).clamp(2.2, 4.0);
        for path in teeth::canals(pose) {
            p.add(Shape::line(path.clone(), Stroke::new(lw + 1.6, teeth::with_alpha(look.bg, 0.6))));
            p.add(Shape::line(path, Stroke::new(lw, c)));
        }
        teeth::pulp_chamber(p, pose, c, edge_of(c));
    }
    if let Some(s) = v.post {
        let c = state_color(s);
        if let Some(path) = teeth::canals(pose).first() {
            let half: Vec<Pos2> = path.iter().take(8).cloned().collect();
            let lw = (w * 0.15).clamp(4.0, 8.0);
            p.add(Shape::line(half.clone(), Stroke::new(lw + 2.0, c)));
            p.add(Shape::line(half, Stroke::new(lw, look.metal[1])));
        }
        teeth::crown_patch(p, pose, (-0.5, 0.5), (0.34, 0.97), look.metal[1], Stroke::new(1.6, c));
    }
    if present {
        if let Some(s) = v.crown {
            teeth::draw_crown(p, pose, look, Some(state_color(s)));
        }
        if let Some(s) = v.veneer {
            let c = state_color(s);
            teeth::crown_patch(p, pose, (-0.9, 0.9), (0.03, 0.93), teeth::with_alpha(c, 0.42), Stroke::new(1.8, c));
        }
        // las obturaciones también se ven por vestibular: oclusal arriba,
        // mesial y distal a los lados, cervical (vestibular) abajo
        let incisal = matches!(teeth::kind(pose.t), teeth::Kind::IncisorCentral | teeth::Kind::IncisorLateral | teeth::Kind::Canine);
        let sort = |a: f32, b: f32| (a.min(b), a.max(b));
        for (i, f) in v.faces.iter().enumerate() {
            let Some(s) = f else { continue };
            let c = state_color(*s);
            let region = match i {
                0 => Some((sort(m * 0.62, m * 0.97), (0.12, 0.72))),
                1 => Some((sort(-m * 0.62, -m * 0.97), (0.12, 0.72))),
                2 => Some(((-0.78, 0.78), (0.02, if incisal { 0.14 } else { 0.22 }))),
                3 => Some(((-0.48, 0.48), (0.58, 0.92))),
                _ => None,
            };
            if let Some((a, ss)) = region {
                teeth::crown_patch(p, pose, a, ss, c, edge_of(c));
            }
        }
        if let Some(s) = v.sealant {
            let c = state_color(s);
            for a in [-0.45f32, 0.0, 0.45] {
                p.circle_filled(teeth::crown_pt(pose, a, 0.08), (w * 0.045).max(1.8), c);
            }
        }
    }
    if let Some(s) = v.bridge {
        let c = state_color(s);
        // pilares y pónticos se ven como coronas del color del puente, unidas por un conector
        let y = pose.y(teeth::CROWN_H * 0.42);
        let h = (w * 0.10).clamp(3.0, 6.0);
        // el conector sólo va hacia las piezas vecinas que también son del puente
        let x0 = if links.0 { pose.rect.min.x - gap / 2.0 - 1.5 } else { pose.rect.center().x };
        let x1 = if links.1 { pose.rect.max.x + gap / 2.0 + 1.5 } else { pose.rect.center().x };
        if x1 > x0 {
            p.rect_filled(Rect::from_min_max(pos2(x0, y - h / 2.0), pos2(x1, y + h / 2.0)), CornerRadius::same(2), c);
        }
        if v.crown.is_none() {
            teeth::draw_crown(p, pose, look, Some(c));
        }
    }
    if let Some(s) = v.bracket {
        let c = state_color(s);
        let ctr = teeth::crown_pt(pose, 0.0, 0.52);
        p.line_segment([pos2(pose.rect.min.x - gap / 2.0 - 1.0, ctr.y), pos2(pose.rect.max.x + gap / 2.0 + 1.0, ctr.y)], Stroke::new(1.6, c));
        let b = Rect::from_center_size(ctr, vec2(w * 0.36, w * 0.30));
        p.rect_filled(b, CornerRadius::same(2), look.metal[2]);
        p.rect_stroke(b, CornerRadius::same(2), Stroke::new(1.4, c), StrokeKind::Inside);
        p.line_segment([b.left_center(), b.right_center()], Stroke::new(1.0, look.metal[0]));
    }
    if let Some(TxState::Previsto) = v.extracted {
        let r = pose.rect.shrink2(vec2(w * 0.08, pose.rect.height() * 0.1));
        cross(p, r, state_color(TxState::Previsto), (w * 0.07).clamp(2.4, 4.0), look);
    }
}

/// Vista oclusal sin tratamientos.
pub fn occl_base(p: &Painter, t: u8, occl: Rect, v: &ToothView, look: &teeth::Look) {
    if gone(v) && v.implant.is_none() {
        if v.bridge.is_none() {
            teeth::occl_ghost(p, t, occl, look);
        }
    } else {
        teeth::draw_occlusal(p, t, occl, look);
    }
}

/// Cómo se dibuja la vista oclusal: líneas guía de las caras (con su color),
/// separación entre piezas y si la pieza se une con la de su izquierda y la
/// de su derecha (puentes, férulas).
#[derive(Clone, Copy)]
pub struct OcclStyle {
    pub guides: Option<Color32>,
    pub gap: f32,
    pub links: (bool, bool),
}

/// Tratamientos sobre la vista oclusal.
pub fn occl_marks(p: &Painter, t: u8, occl: Rect, v: &ToothView, look: &teeth::Look, style: OcclStyle) {
    let OcclStyle { guides, gap, links } = style;
    let present = !gone(v) || v.implant.is_some();
    let r = occl.width().min(occl.height());
    if present {
        if let Some(s) = v.crown {
            teeth::occl_tint(p, t, occl, state_color(s));
        }
        if let Some(s) = v.veneer {
            teeth::fill_face(p, t, occl, 3, teeth::with_alpha(state_color(s), 0.45));
        }
        for (i, f) in v.faces.iter().enumerate() {
            if let Some(s) = f {
                teeth::fill_face(p, t, occl, i, state_color(*s));
            }
        }
        if let Some(g) = guides {
            teeth::face_guides(p, t, occl, g);
        }
        if let Some(s) = v.sealant {
            let c = state_color(s);
            for dx in [-0.18f32, 0.0, 0.18] {
                p.circle_filled(occl.center() + vec2(dx * occl.width(), 0.0), (r * 0.05).max(1.6), c);
            }
        }
        if let Some(s) = v.endo {
            let c = state_color(s);
            p.circle_filled(occl.center(), r * 0.11, c);
            p.circle_stroke(occl.center(), r * 0.11 + 1.0, Stroke::new(1.0, teeth::with_alpha(Color32::WHITE, 0.8)));
        }
        if let Some(s) = v.post {
            let b = Rect::from_center_size(occl.center(), vec2(r * 0.26, r * 0.26));
            p.rect_filled(b, CornerRadius::same(2), look.metal[1]);
            p.rect_stroke(b, CornerRadius::same(2), Stroke::new(1.4, state_color(s)), StrokeKind::Inside);
        }
        if let Some(s) = v.implant {
            let c = state_color(s);
            p.circle_filled(occl.center(), r * 0.24, look.metal[1]);
            p.circle_stroke(occl.center(), r * 0.24, Stroke::new(1.6, c));
            let hex: Vec<Pos2> = (0..6).map(|i| occl.center() + egui::Vec2::angled(i as f32 * std::f32::consts::TAU / 6.0) * r * 0.11).collect();
            p.add(Shape::closed_line(hex, Stroke::new(1.2, look.metal[0])));
        }
        if let Some(s) = v.bracket {
            let b = Rect::from_center_size(occl.center() + vec2(0.0, if teeth::is_upper(t) { -r * 0.32 } else { r * 0.32 }), vec2(r * 0.3, r * 0.16));
            p.rect_filled(b, CornerRadius::same(2), state_color(s));
        }
    }
    if let Some(s) = v.bridge {
        let c = state_color(s);
        let h = (r * 0.10).clamp(3.0, 5.0);
        let y = occl.center().y;
        let x0 = if links.0 { occl.min.x - gap / 2.0 - 1.5 } else { occl.center().x };
        let x1 = if links.1 { occl.max.x + gap / 2.0 + 1.5 } else { occl.center().x };
        if x1 > x0 {
            p.rect_filled(Rect::from_min_max(pos2(x0, y - h / 2.0), pos2(x1, y + h / 2.0)), CornerRadius::same(2), c);
        }
        if v.crown.is_none() {
            if gone(v) && v.implant.is_none() {
                teeth::draw_occlusal(p, t, occl, look);
            }
            teeth::occl_tint(p, t, occl, c);
        }
    }
    if let Some(TxState::Previsto) = v.extracted {
        cross(p, occl.shrink(occl.width() * 0.16), state_color(TxState::Previsto), (r * 0.07).clamp(2.2, 3.6), look);
    }
}

/// Dibuja una arcada y devuelve el clic (pieza, cara).
#[allow(clippy::too_many_arguments)]
pub fn arch(ui: &mut Ui, teeth_list: &[u8], upper: bool, views: &std::collections::HashMap<u8, ToothView>, sel: Option<u8>, width: f32, images: &[u8], dark: bool) -> Option<(u8, Option<usize>)> {
    let look = look(dark);
    let sk = skin(dark);
    let n = teeth_list.len() as f32;
    let gap = 4.0;
    let tw = ((width - 30.0) / n - gap).clamp(28.0, 66.0);
    let side_h = tw * 2.35;
    let occ = tw * 1.0;
    let total_w = n * (tw + gap) + 10.0;
    let h = side_h + occ + 44.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    let x0 = rect.center().x - total_w / 2.0;
    let p = ui.painter().clone();
    let mut clicked = None;
    struct Cell {
        t: u8,
        side: Rect,
        occl: Rect,
        num_y: f32,
        hovered: bool,
        v: ToothView,
    }
    let mut cells = vec![];
    for (i, &t) in teeth_list.iter().enumerate() {
        let mut x = x0 + i as f32 * (tw + gap);
        if i as f32 >= n / 2.0 {
            x += 10.0;
        }
        let (num_y, side, occl) = if upper {
            let side = Rect::from_min_size(pos2(x, rect.min.y + 24.0), vec2(tw, side_h));
            let occl = Rect::from_min_size(pos2(x, side.max.y + 10.0), vec2(tw, occ));
            (rect.min.y + 10.0, side, occl)
        } else {
            let occl = Rect::from_min_size(pos2(x, rect.min.y + 4.0), vec2(tw, occ));
            let side = Rect::from_min_size(pos2(x, occl.max.y + 10.0), vec2(tw, side_h));
            (side.max.y + 14.0, side, occl)
        };
        let resp = ui.interact(side.union(occl), ui.scope_id().with(("tooth", t)), Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            let face = resp.interact_pointer_pos().and_then(|pp| if occl.contains(pp) { teeth::face_at(t, occl, pp) } else { None });
            clicked = Some((t, face));
        }
        cells.push(Cell { t, side, occl, num_y, hovered: resp.hovered(), v: views.get(&t).cloned().unwrap_or_default() });
    }
    // 1) fondo de la pieza elegida o señalada
    for c in &cells {
        let r = c.side.union(c.occl).expand2(vec2(2.0, 6.0));
        if sel == Some(c.t) {
            p.rect_filled(r, CornerRadius::same(10), sk.sel_cell);
            p.rect_stroke(r, CornerRadius::same(10), Stroke::new(1.2, sk.sel_line), StrokeKind::Inside);
        } else if c.hovered {
            p.rect_filled(r, CornerRadius::same(10), sk.hover_cell);
        }
    }
    // 2) piezas
    for c in &cells {
        side_base(&p, &teeth::Pose::new(c.t, c.side), &c.v, look);
        occl_base(&p, c.t, c.occl, &c.v, look);
    }
    // 3) encía translúcida con sus papilas
    let gum: Vec<(teeth::Pose, bool)> = cells.iter().map(|c| (teeth::Pose::new(c.t, c.side), !gone(&c.v) || c.v.implant.is_some() || c.v.bridge.is_some())).collect();
    teeth::draw_gum(&p, &gum, gap, look);
    // 4) tratamientos
    for (i, c) in cells.iter().enumerate() {
        let linked = |j: Option<usize>| j.and_then(|j| cells.get(j)).map(|n| n.v.bridge.is_some()).unwrap_or(false);
        let links = (linked(i.checked_sub(1)), linked(Some(i + 1)));
        side_marks(&p, &teeth::Pose::new(c.t, c.side), &c.v, look, gap, links);
        let guides = if c.hovered || sel == Some(c.t) { Some(sk.guide) } else { None };
        occl_marks(&p, c.t, c.occl, &c.v, look, OcclStyle { guides, gap, links });
    }
    // 5) números
    for c in &cells {
        let pos = pos2(c.side.center().x, c.num_y);
        if sel == Some(c.t) {
            p.rect_filled(Rect::from_center_size(pos, vec2(28.0, 19.0)), CornerRadius::same(10), ACCENT);
            p.text(pos, Align2::CENTER_CENTER, c.t.to_string(), sbf(12.5), Color32::WHITE);
        } else {
            p.text(pos, Align2::CENTER_CENTER, c.t.to_string(), sbf(12.5), if c.v.count > 0 { sk.text } else { sk.num });
        }
        if c.v.count > 0 {
            p.circle_filled(pos2(c.side.max.x - 2.0, c.num_y - 6.0), 2.6, ACCENT);
        }
        if images.contains(&c.t) {
            let r = Rect::from_center_size(pos2(c.side.min.x + 4.0, c.num_y), vec2(9.0, 7.0));
            p.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.1, INFO), StrokeKind::Inside);
            p.circle_stroke(r.center(), 1.8, Stroke::new(1.0, INFO));
        }
    }
    // línea media
    let mx = x0 + (n / 2.0) * (tw + gap) + 3.0;
    p.extend(Shape::dashed_line(&[pos2(mx, rect.min.y + 18.0), pos2(mx, rect.max.y - 6.0)], Stroke::new(1.0, sk.divider), 5.0, 4.0));
    clicked
}

pub fn short_label(k: OdgKind) -> &'static str {
    match k {
        OdgKind::Obturacion => "Obturar",
        OdgKind::Endodoncia => "Endodoncia",
        OdgKind::Extraccion => "Extraer",
        OdgKind::Corona => "Corona",
        OdgKind::Implante => "Implante",
        OdgKind::Carilla => "Carilla",
        OdgKind::Puente => "Puente",
        OdgKind::PernoMunon => "Perno",
        OdgKind::Apicectomia => "Apicect.",
        OdgKind::ProtesisRemovible => "Removible",
        OdgKind::Sellador => "Sellador",
        OdgKind::Ausente => "Ausente",
        OdgKind::Bracket => "Bracket",
        _ => "",
    }
}

/// Icono de herramienta: una pieza con el tratamiento dibujado.
pub fn tool_icon(p: &Painter, k: OdgKind, rect: Rect, dark: bool) {
    let look = look(dark);
    let mut v = ToothView::default();
    let s = Some(TxState::Realizado);
    let t: u8 = match k {
        OdgKind::Carilla | OdgKind::Bracket | OdgKind::Corona => 11,
        OdgKind::Endodoncia | OdgKind::PernoMunon | OdgKind::Apicectomia => 43,
        OdgKind::Implante | OdgKind::Extraccion | OdgKind::Ausente | OdgKind::Puente | OdgKind::ProtesisRemovible => 45,
        _ => 36,
    };
    match k {
        OdgKind::Obturacion => {
            v.faces[2] = s;
            v.faces[0] = s;
        }
        OdgKind::Sellador => v.sealant = s,
        OdgKind::Endodoncia => v.endo = s,
        OdgKind::Extraccion => v.extracted = Some(TxState::Previsto),
        OdgKind::Corona => v.crown = s,
        OdgKind::Implante => v.implant = s,
        OdgKind::Carilla => v.veneer = s,
        OdgKind::Puente | OdgKind::ProtesisRemovible => {
            v.bridge = s;
            v.absent = s;
        }
        OdgKind::PernoMunon => v.post = s,
        OdgKind::Apicectomia => v.apico = s,
        OdgKind::Ausente => v.absent = s,
        OdgKind::Bracket => v.bracket = s,
        _ => {}
    }
    if k.uses_faces() {
        let sq = Rect::from_center_size(rect.center(), vec2(rect.height() * 0.92, rect.height() * 0.92));
        occl_base(p, t, sq, &v, look);
        occl_marks(p, t, sq, &v, look, OcclStyle { guides: None, gap: 0.0, links: (false, false) });
        return;
    }
    // los iconos van siempre con la corona arriba
    let h = rect.height();
    let side = Rect::from_center_size(rect.center(), vec2(h / 2.0, h));
    let pose = teeth::Pose { up: false, ..teeth::Pose::new(t, side) };
    let gap = (rect.width() - side.width()).max(0.0);
    if k == OdgKind::ProtesisRemovible {
        // base rosada de la prótesis con un diente y un retenedor metálico
        let base = Rect::from_min_max(pos2(rect.min.x + 2.0, teeth::crown_pt(&pose, 0.0, 0.85).y), pos2(rect.max.x - 2.0, rect.min.y + h * 0.66));
        p.rect_filled(base, CornerRadius::same(5), Color32::from_rgb(240, 150, 168));
        p.rect_stroke(base, CornerRadius::same(5), Stroke::new(1.0, Color32::from_rgb(200, 96, 120)), StrokeKind::Inside);
        teeth::draw_crown(p, &pose, look, None);
        let y = teeth::crown_pt(&pose, 0.0, 0.55).y;
        p.add(Shape::line(vec![pos2(base.min.x + 1.0, y + 4.0), pos2(base.min.x + 3.0, y - 1.0), pos2(side.min.x + 2.0, y - 1.0)], Stroke::new(1.4, look.metal[0])));
        p.add(Shape::line(vec![pos2(base.max.x - 1.0, y + 4.0), pos2(base.max.x - 3.0, y - 1.0), pos2(side.max.x - 2.0, y - 1.0)], Stroke::new(1.4, look.metal[0])));
        return;
    }
    side_base(p, &pose, &v, look);
    side_marks(p, &pose, &v, look, gap, (true, true));
}

/// Botón de la barra de herramientas del odontograma (icono y nombre).
pub fn tool_button(ui: &mut Ui, size: egui::Vec2, selected: bool, tip: &str, paint: impl FnOnce(&Painter, Rect), label: &str, sk: &Skin) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    let p = ui.painter();
    let (fill, line) = if selected {
        (sk.tile_sel, Stroke::new(1.5, sk.tile_sel_line))
    } else if resp.hovered() {
        (sk.tile_hover, Stroke::new(1.0, sk.divider))
    } else {
        (sk.tile, Stroke::new(1.0, sk.divider))
    };
    p.rect(rect, CornerRadius::same(10), fill, line, StrokeKind::Inside);
    let icon = Rect::from_min_max(rect.min + vec2(4.0, 5.0), pos2(rect.max.x - 4.0, rect.max.y - if label.is_empty() { 4.0 } else { 19.0 }));
    paint(p, icon);
    if !label.is_empty() {
        let col = if selected { sk.tile_sel_line } else { sk.tile_text };
        // el nombre se reduce para caber en el botón
        let mut size = 11.2;
        while size > 8.6 && p.layout_no_wrap(label.to_string(), mdf(size), col).size().x > rect.width() - 6.0 {
            size -= 0.3;
        }
        p.text(pos2(rect.center().x, rect.max.y - 10.0), Align2::CENTER_CENTER, label, mdf(size), col);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tip)
}

/// Botón de navegación por las visitas: flechas dibujadas (una, o dos con tope).
pub fn nav_button(ui: &mut Ui, left: bool, double: bool, tip: &str, sk: &Skin) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(36.0, 34.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    let p = ui.painter();
    let hov = resp.hovered();
    p.rect(r, CornerRadius::same(10), if hov { sk.tile_hover } else { sk.tile }, Stroke::new(1.0, sk.divider), StrokeKind::Inside);
    let col = if hov { sk.tile_sel_line } else { sk.text };
    let d = if left { 1.0 } else { -1.0 };
    let cy = r.center().y;
    let chevron = |cx: f32| vec![pos2(cx + 2.4 * d, cy - 5.0), pos2(cx - 2.4 * d, cy), pos2(cx + 2.4 * d, cy + 5.0)];
    if double {
        p.line_segment([pos2(r.center().x - 5.5 * d, cy - 5.5), pos2(r.center().x - 5.5 * d, cy + 5.5)], Stroke::new(1.7, col));
        p.add(Shape::line(chevron(r.center().x + 1.5 * d), Stroke::new(1.7, col)));
    } else {
        p.add(Shape::line(chevron(r.center().x), Stroke::new(1.7, col)));
    }
    if hov {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tip)
}

/// Interruptor de dos opciones (aspecto claro / oscuro).
pub fn segmented(ui: &mut Ui, options: &[&str], selected: &mut usize, sk: &Skin) {
    let font = mdf(12.5);
    let widths: Vec<f32> = options.iter().map(|o| ui.painter().layout_no_wrap(o.to_string(), font.clone(), sk.text).size().x + 22.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 6.0;
    let (r, _) = ui.allocate_exact_size(vec2(total, 34.0), Sense::hover());
    ui.painter().rect(r, CornerRadius::same(17), sk.tile, Stroke::new(1.0, sk.divider), StrokeKind::Inside);
    let mut x = r.min.x + 3.0;
    for (i, o) in options.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(x, r.min.y + 3.0), vec2(widths[i], r.height() - 6.0));
        let resp = ui.interact(cell, ui.scope_id().with(("seg", i)), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, *o));
        let on = *selected == i;
        if on {
            ui.painter().rect_filled(cell, CornerRadius::same(14), sk.tile_sel);
            ui.painter().rect_stroke(cell, CornerRadius::same(14), Stroke::new(1.0, sk.tile_sel_line), StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(cell, CornerRadius::same(14), sk.tile_hover);
        }
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, *o, font.clone(), if on { sk.tile_sel_line } else { sk.tile_text });
        if resp.clicked() {
            *selected = i;
        }
        x += widths[i];
    }
}
