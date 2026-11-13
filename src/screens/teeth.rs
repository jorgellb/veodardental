//! Dibujo de piezas dentales, vista vestibular y oclusal, en estilo
//! esquemático y limpio: siluetas anatómicas suaves (corona con sus cúspides,
//! una, dos o tres raíces con su curvatura), sombreado sutil por vértices,
//! contorno nítido y encía translúcida. Hay dos aspectos: claro (por defecto)
//! y oscuro tipo radiografía.
//!
//! Geometría: cada parte (corona, raíz, tornillo de implante) es la imagen de
//! un cuadrado unidad (a, s) mediante una función suave, así que se puede
//! rellenar con una malla en rejilla (con degradados) y recortar regiones
//! (caras de una obturación, carilla…) siguiendo exactamente la forma.

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    IncisorCentral,
    IncisorLateral,
    Canine,
    Premolar,
    Molar,
}

pub fn kind(t: u8) -> Kind {
    let u = t % 10;
    let child = t > 50;
    match (child, u) {
        (_, 1) => Kind::IncisorCentral,
        (_, 2) => Kind::IncisorLateral,
        (_, 3) => Kind::Canine,
        (false, 4) | (false, 5) => Kind::Premolar,
        _ => Kind::Molar,
    }
}

pub fn is_upper(t: u8) -> bool {
    matches!(t / 10, 1 | 2 | 5 | 6)
}

fn lower_incisor(t: u8) -> bool {
    !is_upper(t) && t % 10 <= 2
}

/// Cuadrantes que se dibujan a la izquierda de la línea media (derecha del
/// paciente): su cara mesial queda a la derecha.
pub fn mesial_right(t: u8) -> bool {
    matches!(t / 10, 1 | 4 | 5 | 8)
}

/// Proporción de la corona respecto a la altura de la vista vestibular.
pub const CROWN_H: f32 = 0.40;

// ───────────────────────────── colores ─────────────────────────────

/// Colores del dibujo: claro (por defecto) u oscuro tipo radiografía.
#[derive(Clone, Copy)]
pub struct Look {
    /// sombra, tono medio y brillo
    pub enamel: [Color32; 3],
    pub root: [Color32; 3],
    pub back_root: [Color32; 3],
    pub metal: [Color32; 3],
    pub outline: Color32,
    pub outline_w: f32,
    pub gum: Color32,
    pub gum_line: Color32,
    pub ghost: Color32,
    pub guide: Color32,
    pub fissure: Color32,
    pub bg: Color32,
    pub gloss: f32,
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub const LIGHT: Look = Look {
    enamel: [c(208, 203, 192), c(244, 241, 234), c(255, 255, 255)],
    root: [c(206, 191, 162), c(236, 226, 205), c(249, 243, 230)],
    back_root: [c(186, 172, 146), c(214, 203, 182), c(228, 220, 203)],
    metal: [c(112, 124, 142), c(184, 192, 205), c(242, 245, 250)],
    outline: c(120, 132, 150),
    outline_w: 1.15,
    gum: Color32::from_rgba_premultiplied(71, 36, 44, 77),
    gum_line: c(224, 120, 140),
    ghost: c(160, 172, 190),
    guide: Color32::from_rgba_premultiplied(42, 47, 55, 107),
    fissure: Color32::from_rgba_premultiplied(66, 62, 53, 112),
    bg: c(255, 255, 255),
    gloss: 0.75,
};

pub const DARK: Look = Look {
    enamel: [c(92, 96, 106), c(200, 204, 212), c(252, 252, 254)],
    root: [c(78, 78, 82), c(168, 168, 166), c(226, 226, 222)],
    back_root: [c(58, 58, 62), c(120, 120, 120), c(160, 160, 158)],
    metal: [c(84, 96, 116), c(160, 172, 190), c(236, 242, 250)],
    outline: c(34, 38, 46),
    outline_w: 1.0,
    gum: Color32::from_rgba_premultiplied(50, 22, 28, 61),
    gum_line: c(206, 104, 126),
    ghost: c(92, 104, 124),
    guide: Color32::from_rgba_premultiplied(50, 68, 95, 115),
    fissure: Color32::from_rgba_premultiplied(36, 36, 40, 153),
    bg: c(4, 5, 8),
    gloss: 0.45,
};

pub fn lerp_c(a: Color32, b: Color32, k: f32) -> Color32 {
    let k = k.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    Color32::from_rgba_premultiplied(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()), f(a.a(), b.a()))
}

pub fn with_alpha(c: Color32, a: f32) -> Color32 {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (255.0 * a.clamp(0.0, 1.0)) as u8)
}

/// Tono de una superficie con volumen: luz desde arriba a la izquierda,
/// sombra suave en los bordes. `a` ∈ [-1,1] de lado a lado, `s` ∈ [0,1] a lo largo.
fn tone(pal: &[Color32; 3], a: f32, s: f32, depth: f32) -> Color32 {
    let d = (a + 0.38) / 1.38;
    let mut l = 1.0 - 0.62 * d * d;
    l -= depth * s;
    let rim = ((a.abs() - 0.80) / 0.20).clamp(0.0, 1.0);
    l -= 0.30 * rim * rim;
    let l = l.clamp(0.0, 1.0);
    if l < 0.62 { lerp_c(pal[0], pal[1], l / 0.62) } else { lerp_c(pal[1], pal[2], (l - 0.62) / 0.38) }
}

// ───────────────────────────── geometría ─────────────────────────────

/// Dónde y en qué sentido se dibuja una pieza en vista vestibular.
#[derive(Clone, Copy)]
pub struct Pose {
    pub t: u8,
    pub rect: Rect,
    /// raíces hacia arriba en pantalla
    pub up: bool,
}

impl Pose {
    pub fn new(t: u8, rect: Rect) -> Self {
        Pose { t, rect, up: is_upper(t) }
    }
    /// u ∈ [-0.5, 0.5] horizontal (fracción del ancho); v ∈ [0,1] del borde oclusal al ápice.
    pub fn p(&self, u: f32, v: f32) -> Pos2 {
        pos2(self.rect.center().x + u * self.rect.width(), self.y(v))
    }
    pub fn y(&self, v: f32) -> f32 {
        if self.up { self.rect.max.y - v * self.rect.height() } else { self.rect.min.y + v * self.rect.height() }
    }
    /// +1 si el ápice está hacia abajo en pantalla.
    pub fn apical(&self) -> f32 {
        if self.up { -1.0 } else { 1.0 }
    }
    /// +1 si la cara mesial queda a la derecha.
    pub fn mesial(&self) -> f32 {
        if mesial_right(self.t) { 1.0 } else { -1.0 }
    }
    pub fn w(&self) -> f32 {
        self.rect.width()
    }
}

fn smooth(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

fn interp(pts: &[(f32, f32)], s: f32) -> f32 {
    let s = s.clamp(0.0, 1.0);
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if s <= b.0 {
            let k = (s - a.0) / (b.0 - a.0).max(1e-4);
            return a.1 + (b.1 - a.1) * smooth(k);
        }
    }
    pts.last().map(|p| p.1).unwrap_or(0.3)
}

/// Semiancho de la corona (fracción del ancho) del borde oclusal (0) al cuello (1).
fn crown_prof(t: u8) -> &'static [(f32, f32)] {
    match kind(t) {
        Kind::IncisorCentral if lower_incisor(t) => &[(0.0, 0.27), (0.22, 0.305), (0.5, 0.295), (1.0, 0.215)],
        Kind::IncisorLateral if lower_incisor(t) => &[(0.0, 0.285), (0.22, 0.32), (0.5, 0.31), (1.0, 0.225)],
        Kind::IncisorCentral => &[(0.0, 0.40), (0.2, 0.455), (0.48, 0.45), (0.78, 0.39), (1.0, 0.315)],
        Kind::IncisorLateral => &[(0.0, 0.325), (0.22, 0.385), (0.5, 0.375), (1.0, 0.27)],
        Kind::Canine if !is_upper(t) => &[(0.0, 0.28), (0.32, 0.385), (0.58, 0.37), (1.0, 0.27)],
        Kind::Canine => &[(0.0, 0.30), (0.32, 0.425), (0.58, 0.41), (1.0, 0.30)],
        Kind::Premolar => &[(0.0, 0.33), (0.3, 0.44), (0.56, 0.42), (1.0, 0.29)],
        Kind::Molar => &[(0.0, 0.42), (0.26, 0.475), (0.56, 0.46), (1.0, 0.39)],
    }
}

/// Altura del borde oclusal (en fracción de la corona) en cada punto a ∈ [-1,1]:
/// esquinas redondeadas en incisivos, cúspide en caninos y premolares, dos
/// cúspides vestibulares en molares.
fn crown_edge(pose: &Pose, a: f32) -> f32 {
    let am = a * pose.mesial(); // positivo hacia mesial
    let corner = |start: f32, mes: f32, dis: f32| {
        let k = ((am.abs() - start) / (1.0 - start)).max(0.0);
        k * k * if am > 0.0 { mes } else { dis }
    };
    match kind(pose.t) {
        Kind::IncisorCentral if lower_incisor(pose.t) => corner(0.72, 0.05, 0.07),
        Kind::IncisorLateral if lower_incisor(pose.t) => corner(0.68, 0.06, 0.09),
        Kind::IncisorCentral => corner(0.66, 0.07, 0.15),
        Kind::IncisorLateral => corner(0.52, 0.11, 0.22),
        Kind::Canine => {
            let x = (am - 0.12).abs() / 1.12;
            0.30 * x.powf(1.35)
        }
        Kind::Premolar => {
            let x = (am - 0.05).abs() / 1.05;
            0.15 * x.powf(1.7)
        }
        Kind::Molar => {
            let m = (am.abs() - 0.46).abs() / 0.54;
            if am.abs() < 0.46 { 0.085 * (m / 0.852).powf(1.7) } else { 0.17 * m.powf(1.8) }
        }
    }
}

/// Línea amelocementaria: se curva hacia el ápice en el centro de la pieza.
fn neck(a: f32) -> f32 {
    1.0 + 0.075 * (1.0 - a * a)
}

/// Punto de la corona: a ∈ [-1,1] de lado a lado, s ∈ [0,1] del borde oclusal al cuello.
pub fn crown_pt(pose: &Pose, a: f32, s: f32) -> Pos2 {
    let e = crown_edge(pose, a);
    let n = neck(a);
    let se = e + s * (n - e);
    let hw = interp(crown_prof(pose.t), se.min(1.0));
    pose.p(a * hw, se * CROWN_H)
}

/// Punto de la línea amelocementaria (para el periodontograma).
pub fn cej_pt(pose: &Pose, a: f32) -> Pos2 {
    crown_pt(pose, a, 1.0)
}

/// Milímetros a píxeles en esta vista (una corona mide unos 10 mm).
pub fn mm(pose: &Pose) -> f32 {
    pose.rect.height() * CROWN_H * 1.04 / 10.0
}

#[derive(Clone, Copy)]
struct RootDef {
    u0: f32,
    u1: f32,
    w0: f32,
    w1: f32,
    apex: f32,
    back: bool,
    bend: f32,
}

fn roots(t: u8) -> Vec<RootDef> {
    let up = is_upper(t);
    let child = t > 50;
    let r = |u0, u1, w0, w1, apex, back, bend| RootDef { u0, u1, w0, w1, apex, back, bend };
    match kind(t) {
        Kind::IncisorCentral if lower_incisor(t) => vec![r(0.0, 0.01, 0.205, 0.045, if child { 0.80 } else { 0.90 }, false, 0.03)],
        Kind::IncisorLateral if lower_incisor(t) => vec![r(0.0, 0.01, 0.215, 0.045, if child { 0.80 } else { 0.91 }, false, 0.03)],
        Kind::IncisorCentral => vec![r(0.0, 0.02, 0.30, 0.06, if child { 0.82 } else { 0.95 }, false, 0.04)],
        Kind::IncisorLateral => vec![r(0.0, 0.03, 0.26, 0.05, if child { 0.82 } else { 0.93 }, false, 0.07)],
        Kind::Canine => vec![r(0.0, 0.03, 0.285, 0.05, if child { 0.86 } else { 1.0 }, false, 0.05)],
        Kind::Premolar if up && t % 10 == 4 => vec![r(-0.12, -0.13, 0.155, 0.04, 0.92, false, 0.03), r(0.12, 0.12, 0.155, 0.04, 0.92, false, 0.03)],
        Kind::Premolar => vec![r(0.0, 0.03, 0.275, 0.05, 0.95, false, 0.05)],
        Kind::Molar if child => vec![r(-0.19, -0.38, 0.19, 0.04, 0.80, false, 0.0), r(0.19, 0.38, 0.19, 0.04, 0.80, false, 0.0)],
        Kind::Molar if up => vec![r(0.0, 0.02, 0.20, 0.06, 0.95, true, 0.0), r(-0.17, -0.27, 0.205, 0.05, 0.88, false, 0.04), r(0.17, 0.25, 0.205, 0.05, 0.86, false, 0.04)],
        Kind::Molar => vec![r(-0.17, -0.26, 0.215, 0.06, 0.92, false, 0.05), r(0.17, 0.24, 0.215, 0.06, 0.89, false, 0.05)],
    }
}

/// Punto de una raíz: a ∈ [-1,1] de lado a lado, s ∈ [0,1] del cuello al ápice (ápice redondeado).
fn root_pt(pose: &Pose, r: &RootDef, a: f32, s: f32) -> Pos2 {
    let v0 = CROWN_H * 0.97;
    let v = v0 + s * (r.apex - v0);
    let distal = -pose.mesial();
    let u = r.u0 + (r.u1 - r.u0) * smooth(s.powf(0.8)) + distal * r.bend * s.powi(3);
    // la raíz se estrecha poco a poco y termina en un ápice redondeado
    let tip = r.w0 * 0.46 + r.w1 * 0.6;
    let mut hw = r.w0 + (tip - r.w0) * s.powf(1.25);
    if s > 0.86 {
        let k = (s - 0.86) / 0.14;
        hw *= (1.0 - k * k).max(0.0).sqrt();
    }
    pose.p(u + a * hw, v)
}

/// Malla en rejilla de una región (a0..a1, s0..s1) de una parte.
fn grid(f: &dyn Fn(f32, f32) -> Pos2, col: &dyn Fn(f32, f32) -> Color32, (a0, a1): (f32, f32), (s0, s1): (f32, f32), na: usize, ns: usize) -> Mesh {
    let mut m = Mesh::default();
    for j in 0..=ns {
        let s = s0 + (s1 - s0) * j as f32 / ns as f32;
        for i in 0..=na {
            let a = a0 + (a1 - a0) * i as f32 / na as f32;
            m.vertices.push(Vertex { pos: f(a, s), uv: WHITE_UV, color: col(a, s) });
        }
    }
    let w = (na + 1) as u32;
    for j in 0..ns as u32 {
        for i in 0..na as u32 {
            let k = j * w + i;
            m.add_triangle(k, k + 1, k + w);
            m.add_triangle(k + 1, k + w + 1, k + w);
        }
    }
    m
}

/// Contorno cerrado de una región (a0..a1, s0..s1).
fn boundary(f: &dyn Fn(f32, f32) -> Pos2, (a0, a1): (f32, f32), (s0, s1): (f32, f32), n: usize) -> Vec<Pos2> {
    let mut pts = Vec::with_capacity(4 * n);
    for i in 0..n {
        pts.push(f(a0 + (a1 - a0) * i as f32 / n as f32, s0));
    }
    for i in 0..n {
        pts.push(f(a1, s0 + (s1 - s0) * i as f32 / n as f32));
    }
    for i in 0..n {
        pts.push(f(a1 - (a1 - a0) * i as f32 / n as f32, s1));
    }
    for i in 0..n {
        pts.push(f(a0, s1 - (s1 - s0) * i as f32 / n as f32));
    }
    pts
}

/// Contorno cerrado de la corona.
pub fn crown_outline(pose: &Pose) -> Vec<Pos2> {
    boundary(&|a, s| crown_pt(pose, a, s), (-1.0, 1.0), (0.0, 1.0), 18)
}

fn root_outline(pose: &Pose, r: &RootDef) -> Vec<Pos2> {
    boundary(&|a, s| root_pt(pose, r, a, s), (-1.0, 1.0), (0.0, 1.0), 20)
}

/// Brillo suave (reflejo del esmalte).
fn gloss(p: &Painter, c: Pos2, rx: f32, ry: f32, alpha: f32) {
    let mut m = Mesh::default();
    let seg = 24;
    m.vertices.push(Vertex { pos: c, uv: WHITE_UV, color: with_alpha(Color32::WHITE, alpha) });
    for ring in 1..=3 {
        let k = ring as f32 / 3.0;
        let a = alpha * (1.0 - k).powf(1.4);
        for i in 0..seg {
            let t = i as f32 / seg as f32 * std::f32::consts::TAU;
            m.vertices.push(Vertex { pos: c + vec2(t.cos() * rx * k, t.sin() * ry * k), uv: WHITE_UV, color: with_alpha(Color32::WHITE, a) });
        }
    }
    let idx = |ring: u32, i: u32| 1 + (ring - 1) * seg + i % seg;
    for i in 0..seg {
        m.add_triangle(0, idx(1, i), idx(1, i + 1));
    }
    for ring in 1..3 {
        for i in 0..seg {
            m.add_triangle(idx(ring, i), idx(ring + 1, i), idx(ring + 1, i + 1));
            m.add_triangle(idx(ring, i), idx(ring + 1, i + 1), idx(ring, i + 1));
        }
    }
    p.add(Shape::mesh(m));
}

/// Pieza completa en vista vestibular: raíces (si `with_roots`) y corona.
/// Primero se trazan los contornos con doble grosor y encima los rellenos:
/// así se ve sólo la silueta exterior, sin líneas donde las raíces se unen.
pub fn draw_tooth(p: &Painter, pose: &Pose, look: &Look, with_roots: bool) {
    let ow = look.outline_w * 2.0;
    if with_roots {
        let rs = roots(pose.t);
        for back in [true, false] {
            let group: Vec<&RootDef> = rs.iter().filter(|r| r.back == back).collect();
            for r in &group {
                p.add(Shape::closed_line(root_outline(pose, r), Stroke::new(ow, look.outline)));
            }
            let pal = if back { look.back_root } else { look.root };
            for r in &group {
                p.add(Shape::mesh(grid(&|a, s| root_pt(pose, r, a, s), &|a, s| tone(&pal, a, s, 0.22), (-1.0, 1.0), (0.0, 1.0), 8, 22)));
            }
        }
    }
    draw_crown(p, pose, look, None);
}

/// Corona sola (también para pónticos y coronas protésicas teñidas de un color).
pub fn draw_crown(p: &Painter, pose: &Pose, look: &Look, tint: Option<Color32>) {
    let f = |a: f32, s: f32| crown_pt(pose, a, s);
    let line = match tint {
        Some(c) => Stroke::new(2.0 + look.outline_w, lerp_c(c, Color32::BLACK, 0.12)),
        None => Stroke::new(look.outline_w * 2.0, look.outline),
    };
    p.add(Shape::closed_line(crown_outline(pose), line));
    let pal = match tint {
        Some(c) => [lerp_c(look.enamel[0], c, 0.55), lerp_c(look.enamel[1], c, 0.30), lerp_c(look.enamel[2], c, 0.08)],
        None => look.enamel,
    };
    p.add(Shape::mesh(grid(&f, &|a, s| tone(&pal, a, s, 0.10), (-1.0, 1.0), (0.0, 1.0), 12, 14)));
    // reflejo del esmalte
    let g = crown_pt(pose, -0.38, 0.38);
    gloss(p, g, pose.w() * 0.11, pose.rect.height() * CROWN_H * 0.20, look.gloss);
    // línea amelocementaria apenas marcada
    let cej: Vec<Pos2> = (0..=16).map(|i| crown_pt(pose, -0.92 + 1.84 * i as f32 / 16.0, 0.985)).collect();
    p.add(Shape::line(cej, Stroke::new(0.8, with_alpha(look.outline, 0.35))));
}

/// Rellena una región de la corona (caras de una obturación, carilla…).
pub fn crown_patch(p: &Painter, pose: &Pose, a: (f32, f32), s: (f32, f32), fill: Color32, edge: Stroke) {
    let f = |aa: f32, ss: f32| crown_pt(pose, aa, ss);
    if edge.width > 0.0 {
        p.add(Shape::closed_line(boundary(&f, a, s, 8), edge));
    }
    let light = lerp_c(fill, with_alpha(Color32::WHITE, fill.a() as f32 / 255.0), 0.28);
    p.add(Shape::mesh(grid(&f, &|aa, ss| lerp_c(light, fill, ((aa + 1.0) * 0.25 + ss * 0.5).clamp(0.0, 1.0)), a, s, 6, 6)));
}

/// Contorno discontinuo de una pieza que falta.
pub fn draw_ghost(p: &Painter, pose: &Pose, look: &Look, with_roots: bool) {
    let st = Stroke::new(1.1, look.ghost);
    let mut crown = crown_outline(pose);
    crown.push(crown[0]);
    p.extend(Shape::dashed_line(&crown, st, 4.0, 3.0));
    if with_roots {
        for r in roots(pose.t).iter().filter(|r| !r.back) {
            let mut pts: Vec<Pos2> = (0..=20).map(|i| root_pt(pose, r, -1.0, 0.12 + 0.88 * i as f32 / 20.0)).collect();
            pts.extend((0..=20).map(|i| root_pt(pose, r, 1.0, 1.0 - 0.88 * i as f32 / 20.0)));
            p.extend(Shape::dashed_line(&pts, st, 4.0, 3.0));
        }
    }
}

/// Recorridos de los conductos (cámara pulpar → ápice), para endodoncias.
pub fn canals(pose: &Pose) -> Vec<Vec<Pos2>> {
    roots(pose.t)
        .iter()
        .filter(|r| !r.back || roots(pose.t).len() == 1)
        .map(|r| {
            let mut v = vec![crown_pt(pose, r.u0 * 1.4, 0.62)];
            v.extend((0..=14).map(|i| root_pt(pose, r, 0.0, 0.04 + 0.88 * i as f32 / 14.0)));
            v
        })
        .collect()
}

/// Cámara pulpar (región de la corona).
pub fn pulp_chamber(p: &Painter, pose: &Pose, fill: Color32, edge: Stroke) {
    let wide = matches!(kind(pose.t), Kind::Molar);
    let a = if wide { 0.42 } else { 0.30 };
    crown_patch(p, pose, (-a, a), (0.50, 0.97), fill, edge);
}

/// Apicectomía: se corta el ápice (se difumina) y se marca la línea de corte.
pub fn apex_cut(p: &Painter, pose: &Pose, look: &Look, color: Color32) {
    for r in roots(pose.t).iter().filter(|r| !r.back) {
        let f = |a: f32, s: f32| root_pt(pose, r, a, s);
        p.add(Shape::mesh(grid(&f, &|_, _| with_alpha(look.bg, 0.82), (-1.15, 1.15), (0.84, 1.0), 4, 4)));
        let l = root_pt(pose, r, -1.6, 0.84);
        let rr = root_pt(pose, r, 1.6, 0.84);
        p.line_segment([l, rr], Stroke::new(2.6, color));
    }
}

/// Implante: tornillo de titanio con espiras, cuello y pilar.
pub fn draw_implant(p: &Painter, pose: &Pose, look: &Look, color: Color32) {
    let v0 = CROWN_H * 0.97;
    let apex = 0.90;
    let f = |a: f32, s: f32| {
        let v = v0 + s * (apex - v0);
        let mut hw = 0.205 - 0.055 * s * s;
        if s > 0.88 {
            let k = (s - 0.88) / 0.12;
            hw *= (1.0 - k * k).max(0.0).sqrt().max(0.25);
        }
        pose.p(a * hw, v)
    };
    p.add(Shape::closed_line(boundary(&f, (-1.0, 1.0), (0.0, 1.0), 14), Stroke::new(3.0, color)));
    p.add(Shape::mesh(grid(&f, &|a, s| tone(&look.metal, a, s, 0.12), (-1.0, 1.0), (0.0, 1.0), 8, 16)));
    // espiras
    let thread = Stroke::new(1.1, with_alpha(look.metal[0], 0.9));
    let n = 9;
    for k in 0..n {
        let s = 0.10 + k as f32 * 0.085;
        if s + 0.04 > 0.95 {
            break;
        }
        p.line_segment([f(-1.0, s), f(1.0, s + 0.04)], thread);
    }
    // pilar (cuello pulido)
    let a = pose.p(-0.15, CROWN_H * 0.86);
    let b = pose.p(0.15, CROWN_H * 1.0);
    let r = Rect::from_two_pos(a, b);
    p.rect_filled(r, 2.0, look.metal[2]);
    p.rect_stroke(r, 2.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
}

/// Encía translúcida sobre las raíces, con el borde festoneado (papilas entre piezas).
/// `cells`: cada pieza de la arcada con si está presente; `gap`: hueco entre piezas.
pub fn draw_gum(p: &Painter, cells: &[(Pose, bool)], gap: f32, look: &Look) {
    if cells.is_empty() {
        return;
    }
    let mut top: Vec<Pos2> = vec![];
    let mut bottom: Vec<Pos2> = vec![];
    let fade = 0.30;
    for (i, (pose, present)) in cells.iter().enumerate() {
        // límites a medio camino de las piezas vecinas (continua también en la línea media)
        let x0 = if i > 0 { (cells[i - 1].0.rect.max.x + pose.rect.min.x) / 2.0 } else { pose.rect.min.x - gap / 2.0 };
        let x1 = cells.get(i + 1).map(|n| (pose.rect.max.x + n.0.rect.min.x) / 2.0).unwrap_or(pose.rect.max.x + gap / 2.0);
        let n = 14;
        for i in 0..=n {
            let f = i as f32 / n as f32;
            let bump = (std::f32::consts::PI * f).sin().max(0.0).powf(1.3);
            // donde falta la pieza, la encía sigue lisa (sin papilas), sin saltos con las vecinas
            let v = CROWN_H * (1.0 + if *present { 0.11 } else { 0.03 } * bump) - 0.035;
            let x = x0 + (x1 - x0) * f;
            top.push(pos2(x, pose.y(v)));
            bottom.push(pos2(x, pose.y(v + fade)));
        }
    }
    let mut m = Mesh::default();
    for (t, b) in top.iter().zip(&bottom) {
        m.vertices.push(Vertex { pos: *t, uv: WHITE_UV, color: look.gum });
        m.vertices.push(Vertex { pos: *b, uv: WHITE_UV, color: Color32::TRANSPARENT });
    }
    for i in 0..top.len().saturating_sub(1) as u32 {
        let k = i * 2;
        m.add_triangle(k, k + 1, k + 2);
        m.add_triangle(k + 1, k + 3, k + 2);
    }
    p.add(Shape::mesh(m));
    p.add(Shape::line(top, Stroke::new(1.5, with_alpha(look.gum_line, 0.85))));
}

// ───────────────────────────── vista oclusal ─────────────────────────────

/// Forma oclusal: semiejes (fracción del rectángulo), exponente de superelipse
/// y tamaño de la cara oclusal/incisal central.
fn occl_shape(t: u8) -> (f32, f32, f32, f32, f32) {
    match kind(t) {
        Kind::IncisorCentral | Kind::IncisorLateral if lower_incisor(t) => (0.30, 0.27, 2.4, 0.56, 0.36),
        Kind::IncisorCentral => (0.45, 0.30, 2.6, 0.62, 0.34),
        Kind::IncisorLateral => (0.37, 0.28, 2.4, 0.60, 0.34),
        Kind::Canine => (0.37, 0.38, 2.2, 0.42, 0.42),
        Kind::Premolar => (0.37, 0.44, 2.6, 0.44, 0.42),
        Kind::Molar if t > 50 => (0.42, 0.40, 3.0, 0.44, 0.44),
        Kind::Molar => (0.46, 0.44, 3.2, 0.44, 0.44),
    }
}

/// Radio (normalizado) de una superelipse de exponente n en la dirección θ.
fn se_r(n: f32, kx: f32, ky: f32, th: f32) -> f32 {
    1.0 / ((th.cos().abs() / kx).powf(n) + (th.sin().abs() / ky).powf(n)).powf(1.0 / n)
}

fn occl_point(t: u8, rect: Rect, th: f32, k: f32) -> Pos2 {
    let (fx, fy, n, _, _) = occl_shape(t);
    let r = se_r(n, 1.0, 1.0, th) * k;
    rect.center() + vec2(th.cos() * r * rect.width() * fx, th.sin() * r * rect.height() * fy)
}

fn inner_point(t: u8, rect: Rect, th: f32) -> Pos2 {
    let (fx, fy, n, kx, ky) = occl_shape(t);
    let r = se_r(n, kx, ky, th);
    rect.center() + vec2(th.cos() * r * rect.width() * fx, th.sin() * r * rect.height() * fy)
}

/// Contorno oclusal (k = 1 el borde).
pub fn occl_outline(t: u8, rect: Rect, k: f32) -> Vec<Pos2> {
    (0..64).map(|i| occl_point(t, rect, i as f32 / 64.0 * std::f32::consts::TAU, k)).collect()
}

/// Vista oclusal: superficie con volumen, cinco caras marcadas y fisuras.
pub fn draw_occlusal(p: &Painter, t: u8, rect: Rect, look: &Look) {
    let seg = 64;
    let rings = [0.0f32, 0.3, 0.55, 0.75, 0.88, 0.96, 1.0];
    let mut m = Mesh::default();
    for (ri, k) in rings.iter().enumerate() {
        let count = if ri == 0 { 1 } else { seg };
        for i in 0..count {
            let th = i as f32 / seg as f32 * std::f32::consts::TAU;
            let pos = occl_point(t, rect, th, *k);
            // luz desde arriba a la izquierda
            let dir = (th - 3.93).cos();
            let l = (1.0 - 0.55 * k * k + 0.16 * k * dir).clamp(0.0, 1.0);
            let col = if l < 0.62 { lerp_c(look.enamel[0], look.enamel[1], l / 0.62) } else { lerp_c(look.enamel[1], look.enamel[2], (l - 0.62) / 0.38) };
            m.vertices.push(Vertex { pos, uv: WHITE_UV, color: col });
        }
    }
    for i in 0..seg as u32 {
        m.add_triangle(0, 1 + i, 1 + (i + 1) % seg as u32);
    }
    for r in 1..rings.len() as u32 - 1 {
        let a = 1 + (r - 1) * seg as u32;
        let b = 1 + r * seg as u32;
        for i in 0..seg as u32 {
            let n = (i + 1) % seg as u32;
            m.add_triangle(a + i, b + i, b + n);
            m.add_triangle(a + i, b + n, a + n);
        }
    }
    p.add(Shape::closed_line(occl_outline(t, rect, 1.0), Stroke::new(look.outline_w * 2.0, look.outline)));
    p.add(Shape::mesh(m));
    // fisuras (muy suaves)
    let c0 = rect.center();
    let (fx, fy, _, kx, ky) = occl_shape(t);
    let (ix, iy) = (rect.width() * fx * kx, rect.height() * fy * ky);
    let fis = Stroke::new(1.0, look.fissure);
    match kind(t) {
        Kind::Molar => {
            p.add(Shape::line(vec![c0 + vec2(-ix * 0.75, -iy * 0.10), c0 + vec2(-ix * 0.2, iy * 0.05), c0 + vec2(ix * 0.25, -iy * 0.05), c0 + vec2(ix * 0.75, iy * 0.1)], fis));
            p.line_segment([c0 + vec2(-ix * 0.2, iy * 0.05), c0 + vec2(-ix * 0.1, -iy * 0.75)], fis);
            p.line_segment([c0 + vec2(ix * 0.25, -iy * 0.05), c0 + vec2(ix * 0.15, iy * 0.75)], fis);
        }
        Kind::Premolar => {
            p.add(Shape::line(vec![c0 + vec2(-ix * 0.7, iy * 0.08), c0 + vec2(0.0, -iy * 0.04), c0 + vec2(ix * 0.7, iy * 0.08)], fis));
        }
        _ => {}
    }
    face_lines(p, t, rect, look.guide, 1.0);
}

/// Líneas que separan las cinco caras.
fn face_lines(p: &Painter, t: u8, rect: Rect, color: Color32, w: f32) {
    let st = Stroke::new(w, color);
    let inner: Vec<Pos2> = (0..64).map(|i| inner_point(t, rect, i as f32 / 64.0 * std::f32::consts::TAU)).collect();
    p.add(Shape::closed_line(inner, st));
    for deg in [45.0f32, 135.0, 225.0, 315.0] {
        let th = deg.to_radians();
        p.line_segment([inner_point(t, rect, th), occl_point(t, rect, th, 0.97)], st);
    }
}

/// Contorno marcado de las caras (al pasar el ratón o con la pieza elegida).
pub fn face_guides(p: &Painter, t: u8, rect: Rect, color: Color32) {
    face_lines(p, t, rect, color, 1.4);
}

/// Contorno discontinuo de la vista oclusal de una pieza que falta.
pub fn occl_ghost(p: &Painter, t: u8, rect: Rect, look: &Look) {
    let mut pts = occl_outline(t, rect, 1.0);
    pts.push(pts[0]);
    p.extend(Shape::dashed_line(&pts, Stroke::new(1.1, look.ghost), 4.0, 3.0));
}

/// Índice de cara (0=M, 1=D, 2=O, 3=V, 4=L) en un punto de la vista oclusal.
pub fn face_at(t: u8, rect: Rect, pos: Pos2) -> Option<usize> {
    let (fx, fy, n, kx, ky) = occl_shape(t);
    let d = pos - rect.center();
    let nx = d.x / (rect.width() * fx);
    let ny = d.y / (rect.height() * fy);
    let norm = (nx.abs().powf(n) + ny.abs().powf(n)).powf(1.0 / n);
    if norm > 1.18 {
        return None;
    }
    let inner = ((nx / kx).abs().powf(n) + (ny / ky).abs().powf(n)).powf(1.0 / n);
    if inner < 1.0 {
        return Some(2);
    }
    Some(side_face(t, ny.atan2(nx)))
}

fn side_face(t: u8, ang: f32) -> usize {
    use std::f32::consts::PI;
    let a = (ang + 2.0 * PI) % (2.0 * PI);
    let deg = a.to_degrees();
    let mesial_right = mesial_right(t);
    let up = is_upper(t);
    // 0: derecha, 1: abajo, 2: izquierda, 3: arriba
    let side = if !(45.0..315.0).contains(&deg) {
        0
    } else if deg < 135.0 {
        1
    } else if deg < 225.0 {
        2
    } else {
        3
    };
    match side {
        0 => {
            if mesial_right {
                0
            } else {
                1
            }
        }
        2 => {
            if mesial_right {
                1
            } else {
                0
            }
        }
        1 => {
            if up {
                4
            } else {
                3
            }
        }
        _ => {
            if up {
                3
            } else {
                4
            }
        }
    }
}

/// Rellena una cara de la vista oclusal con un color (con borde y ligero degradado).
pub fn fill_face(p: &Painter, t: u8, rect: Rect, face: usize, color: Color32) {
    use std::f32::consts::PI;
    let light = lerp_c(color, Color32::WHITE, 0.22);
    let edge = Stroke::new(1.0, lerp_c(color, Color32::BLACK, 0.25));
    if face == 2 {
        let seg = 48;
        let mut m = Mesh::default();
        m.vertices.push(Vertex { pos: rect.center(), uv: WHITE_UV, color: light });
        let pts: Vec<Pos2> = (0..seg).map(|i| inner_point(t, rect, i as f32 / seg as f32 * 2.0 * PI)).collect();
        for q in &pts {
            m.vertices.push(Vertex { pos: *q, uv: WHITE_UV, color });
        }
        for i in 0..seg as u32 {
            m.add_triangle(0, 1 + i, 1 + (i + 1) % seg as u32);
        }
        p.add(Shape::closed_line(pts, edge));
        p.add(Shape::mesh(m));
        return;
    }
    for side in 0..4 {
        let start = match side {
            0 => -45.0f32,
            1 => 45.0,
            2 => 135.0,
            _ => 225.0,
        };
        let mid = (start + 45.0).to_radians();
        if side_face(t, mid) != face {
            continue;
        }
        let seg = 16;
        let mut m = Mesh::default();
        let mut outer = vec![];
        let mut inner = vec![];
        for s in 0..=seg {
            let th = (start + 90.0 * s as f32 / seg as f32).to_radians();
            let a = inner_point(t, rect, th);
            let b = occl_point(t, rect, th, 0.985);
            inner.push(a);
            outer.push(b);
            m.vertices.push(Vertex { pos: a, uv: WHITE_UV, color: light });
            m.vertices.push(Vertex { pos: b, uv: WHITE_UV, color });
        }
        for s in 0..seg as u32 {
            let i = s * 2;
            m.add_triangle(i, i + 1, i + 2);
            m.add_triangle(i + 1, i + 3, i + 2);
        }
        let mut ring = outer;
        ring.extend(inner.into_iter().rev());
        p.add(Shape::closed_line(ring, edge));
        p.add(Shape::mesh(m));
    }
}

/// Rellena toda la superficie oclusal (coronas) con un tinte.
pub fn occl_tint(p: &Painter, t: u8, rect: Rect, color: Color32) {
    let pts = occl_outline(t, rect, 0.99);
    let mut m = Mesh::default();
    m.vertices.push(Vertex { pos: rect.center(), uv: WHITE_UV, color: with_alpha(color, 0.18) });
    for q in &pts {
        m.vertices.push(Vertex { pos: *q, uv: WHITE_UV, color: with_alpha(color, 0.42) });
    }
    let n = pts.len() as u32;
    for i in 0..n {
        m.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(m));
    p.add(Shape::closed_line(occl_outline(t, rect, 1.0), Stroke::new(2.2, color)));
}
