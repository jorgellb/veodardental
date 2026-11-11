//! Iconos de línea dibujados a mano (estilo uniforme para el menú lateral).

use crate::app::Screen;
use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};

fn pts(c: Pos2, s: f32, list: &[(f32, f32)]) -> Vec<Pos2> {
    list.iter().map(|(x, y)| c + vec2(x * s, y * s)).collect()
}

/// Dibuja el icono de una pantalla centrado en `c` (tamaño ~20 px).
pub fn nav_icon(p: &Painter, scr: Screen, c: Pos2, col: Color32) {
    let s = 1.0;
    let st = Stroke::new(1.7, col);
    let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(c + vec2(x0, y0), c + vec2(x1, y1));
    match scr {
        Screen::Inicio => {
            p.add(Shape::line(pts(c, s, &[(-9.0, -1.0), (0.0, -9.0), (9.0, -1.0)]), st));
            p.add(Shape::line(pts(c, s, &[(-6.5, -3.0), (-6.5, 8.0), (6.5, 8.0), (6.5, -3.0)]), st));
            p.rect_stroke(r(-2.5, 2.0, 2.5, 8.0), CornerRadius::same(1), st, StrokeKind::Middle);
        }
        Screen::Agenda => {
            p.rect_stroke(r(-8.5, -6.5, 8.5, 8.5), CornerRadius::same(3), st, StrokeKind::Middle);
            p.line_segment([c + vec2(-8.5, -2.0), c + vec2(8.5, -2.0)], st);
            p.line_segment([c + vec2(-4.0, -9.5), c + vec2(-4.0, -4.5)], st);
            p.line_segment([c + vec2(4.0, -9.5), c + vec2(4.0, -4.5)], st);
            for (x, y) in [(-4.0, 2.5), (0.0, 2.5), (4.0, 2.5), (-4.0, 5.5), (0.0, 5.5)] {
                p.circle_filled(c + vec2(x, y), 1.1, col);
            }
        }
        Screen::Pacientes => {
            p.circle_stroke(c + vec2(-3.0, -4.0), 3.5, st);
            p.add(Shape::line(pts(c, s, &[(-9.5, 8.0), (-9.0, 4.0), (-6.5, 1.5), (0.5, 1.5), (3.0, 4.0), (3.5, 8.0)]), st));
            p.circle_stroke(c + vec2(5.5, -3.0), 2.8, st);
            p.add(Shape::line(pts(c, s, &[(5.0, 1.5), (8.0, 2.0), (9.5, 4.5), (9.8, 7.5)]), st));
        }
        Screen::Presupuestos => {
            p.rect_stroke(r(-7.0, -7.5, 7.0, 9.0), CornerRadius::same(2), st, StrokeKind::Middle);
            p.rect_stroke(r(-3.5, -9.5, 3.5, -5.5), CornerRadius::same(1), st, StrokeKind::Middle);
            for y in [-1.5, 2.0, 5.5] {
                p.line_segment([c + vec2(-4.0, y), c + vec2(4.0, y)], st);
            }
        }
        Screen::Cobros => {
            p.rect_stroke(r(-9.5, -6.0, 9.5, 6.5), CornerRadius::same(2), st, StrokeKind::Middle);
            p.line_segment([c + vec2(-9.5, -2.0), c + vec2(9.5, -2.0)], Stroke::new(2.4, col));
            p.line_segment([c + vec2(-6.5, 3.0), c + vec2(-2.0, 3.0)], st);
        }
        Screen::Facturas => {
            p.add(Shape::line(pts(c, s, &[(-7.0, -9.0), (7.0, -9.0), (7.0, 9.0), (4.5, 7.5), (2.0, 9.0), (-0.5, 7.5), (-3.0, 9.0), (-5.5, 7.5), (-7.0, 9.0), (-7.0, -9.0)]), st));
            for y in [-5.0, -1.5, 2.0] {
                p.line_segment([c + vec2(-4.0, y), c + vec2(4.0, y)], st);
            }
        }
        Screen::Caja => {
            p.rect_stroke(r(-9.5, -1.0, 9.5, 8.5), CornerRadius::same(2), st, StrokeKind::Middle);
            p.add(Shape::line(pts(c, s, &[(-7.0, -1.0), (-5.0, -8.0), (5.0, -8.0), (7.0, -1.0)]), st));
            p.circle_stroke(c + vec2(0.0, 3.8), 2.2, st);
        }
        Screen::Recalls => {
            p.add(Shape::line(pts(c, s, &[(-7.5, 5.0), (-6.0, 2.5), (-6.0, -2.0), (-4.0, -6.5), (0.0, -8.0), (4.0, -6.5), (6.0, -2.0), (6.0, 2.5), (7.5, 5.0), (-7.5, 5.0)]), st));
            p.add(Shape::line(pts(c, s, &[(-2.5, 7.0), (0.0, 9.0), (2.5, 7.0)]), st));
        }
        Screen::Comunicaciones => {
            p.add(Shape::line(pts(c, s, &[(-9.0, -7.0), (9.0, -7.0), (9.0, 4.5), (-1.0, 4.5), (-5.5, 8.5), (-5.0, 4.5), (-9.0, 4.5), (-9.0, -7.0)]), st));
            for x in [-4.5, 0.0, 4.5] {
                p.circle_filled(c + vec2(x, -1.2), 1.2, col);
            }
        }
        Screen::Listin => {
            p.add(Shape::line(pts(c, s, &[(-7.0, -9.0), (-3.0, -9.0), (-1.0, -4.0), (-3.5, -2.0), (1.5, 3.5), (4.0, 1.0), (9.0, 3.0), (9.0, 7.0), (6.0, 9.5), (-1.0, 7.0), (-7.5, 0.5), (-9.5, -6.0), (-7.0, -9.0)]), st));
        }
        Screen::Pizarra => {
            p.rect_stroke(r(-10.0, -7.0, 10.0, 6.0), CornerRadius::same(2), st, StrokeKind::Middle);
            p.line_segment([c + vec2(-4.0, 9.5), c + vec2(4.0, 9.5)], st);
            p.line_segment([c + vec2(0.0, 6.0), c + vec2(0.0, 9.5)], st);
        }
        Screen::Imagenes => {
            p.rect_stroke(r(-9.5, -5.5, 9.5, 8.0), CornerRadius::same(3), st, StrokeKind::Middle);
            p.add(Shape::line(pts(c, s, &[(-4.5, -5.5), (-2.5, -8.5), (2.5, -8.5), (4.5, -5.5)]), st));
            p.circle_stroke(c + vec2(0.0, 1.3), 3.8, st);
            p.circle_filled(c + vec2(6.5, -2.8), 1.0, col);
        }
        Screen::Almacen => {
            p.add(Shape::line(pts(c, s, &[(-9.0, -4.0), (0.0, -9.0), (9.0, -4.0), (9.0, 6.0), (0.0, 10.0), (-9.0, 6.0), (-9.0, -4.0)]), st));
            p.add(Shape::line(pts(c, s, &[(-9.0, -4.0), (0.0, 0.5), (9.0, -4.0)]), st));
            p.line_segment([c + vec2(0.0, 0.5), c + vec2(0.0, 10.0)], st);
        }
        Screen::Laboratorio => {
            p.add(Shape::line(pts(c, s, &[(-7.0, -6.0), (-4.0, -9.0), (0.0, -7.0), (4.0, -9.0), (7.0, -6.0), (7.5, -1.0), (5.5, 4.0), (4.5, 9.0), (2.5, 9.0), (1.0, 2.5), (-1.0, 2.5), (-2.5, 9.0), (-4.5, 9.0), (-5.5, 4.0), (-7.5, -1.0), (-7.0, -6.0)]), st));
        }
        Screen::Ortodoncia => {
            for x in [-6.0, 0.0, 6.0] {
                p.rect_stroke(Rect::from_center_size(c + vec2(x, 0.0), vec2(4.5, 4.5)), CornerRadius::same(1), st, StrokeKind::Middle);
            }
            p.line_segment([c + vec2(-10.0, 0.0), c + vec2(10.0, 0.0)], st);
            p.add(Shape::line(pts(c, s, &[(-9.0, -6.0), (-4.0, -9.0), (4.0, -9.0), (9.0, -6.0)]), st));
            p.add(Shape::line(pts(c, s, &[(-9.0, 6.0), (-4.0, 9.0), (4.0, 9.0), (9.0, 6.0)]), st));
        }
        Screen::Bancos => {
            p.add(Shape::line(pts(c, s, &[(-10.0, -3.0), (0.0, -9.0), (10.0, -3.0), (-10.0, -3.0)]), st));
            for x in [-6.5, -2.0, 2.5, 7.0] {
                p.line_segment([c + vec2(x, -1.0), c + vec2(x, 6.0)], st);
            }
            p.line_segment([c + vec2(-10.0, 8.5), c + vec2(10.0, 8.5)], st);
        }
        Screen::Gastos => {
            p.rect_stroke(r(-8.0, -9.0, 8.0, 9.0), CornerRadius::same(2), st, StrokeKind::Middle);
            p.line_segment([c + vec2(-4.5, -4.5), c + vec2(4.5, -4.5)], st);
            p.line_segment([c + vec2(-4.5, -0.5), c + vec2(4.5, -0.5)], st);
            p.add(Shape::line(pts(c, s, &[(-4.5, 5.0), (-1.5, 3.0), (1.5, 5.5), (4.5, 3.0)]), st));
        }
        Screen::Tarifas => {
            p.add(Shape::line(pts(c, s, &[(-9.0, -9.0), (0.5, -9.0), (9.0, -0.5), (-0.5, 9.0), (-9.0, 0.5), (-9.0, -9.0)]), st));
            p.circle_stroke(c + vec2(-4.5, -4.5), 1.8, st);
        }
        Screen::Colaboradores => {
            p.circle_stroke(c + vec2(0.0, -4.5), 4.0, st);
            p.add(Shape::line(pts(c, s, &[(-8.0, 9.0), (-7.5, 4.5), (-4.5, 1.5), (4.5, 1.5), (7.5, 4.5), (8.0, 9.0)]), st));
            p.line_segment([c + vec2(0.0, 1.5), c + vec2(0.0, 5.5)], st);
        }
        Screen::Informes => {
            p.line_segment([c + vec2(-9.0, 8.5), c + vec2(9.0, 8.5)], st);
            p.rect_stroke(r(-7.0, 0.0, -3.0, 8.5), CornerRadius::same(1), st, StrokeKind::Middle);
            p.rect_stroke(r(-1.5, -7.0, 2.5, 8.5), CornerRadius::same(1), st, StrokeKind::Middle);
            p.rect_stroke(r(4.0, -3.0, 8.0, 8.5), CornerRadius::same(1), st, StrokeKind::Middle);
        }
        Screen::Config => {
            for i in 0..8 {
                let a = i as f32 / 8.0 * std::f32::consts::TAU;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * 6.5, c + d * 9.3], Stroke::new(2.6, col));
            }
            p.circle_stroke(c, 6.5, st);
            p.circle_stroke(c, 2.6, st);
        }
        Screen::Modulos => {
            for (x, y) in [(-5.0, -5.0), (5.0, -5.0), (-5.0, 5.0)] {
                p.rect_stroke(Rect::from_center_size(c + vec2(x, y), vec2(7.5, 7.5)), CornerRadius::same(2), st, StrokeKind::Middle);
            }
            p.line_segment([c + vec2(5.0, 1.5), c + vec2(5.0, 8.5)], st);
            p.line_segment([c + vec2(1.5, 5.0), c + vec2(8.5, 5.0)], st);
        }
    }
    let _ = pos2;
}

/// Iconos de los apartados de Configuración (mismo estilo de línea, ~20 px).
pub fn settings_icon(p: &Painter, key: &str, c: Pos2, col: Color32) {
    let s = 1.0;
    let st = Stroke::new(1.7, col);
    let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(c + vec2(x0, y0), c + vec2(x1, y1));
    match key {
        // edificio de la clínica con una cruz
        "clinica" => {
            p.rect_stroke(r(-8.5, -6.0, 8.5, 9.0), CornerRadius::same(2), st, StrokeKind::Middle);
            p.add(Shape::line(pts(c, s, &[(-10.0, -5.0), (0.0, -10.0), (10.0, -5.0)]), st));
            p.line_segment([c + vec2(0.0, -2.5), c + vec2(0.0, 3.5)], Stroke::new(2.0, col));
            p.line_segment([c + vec2(-3.0, 0.5), c + vec2(3.0, 0.5)], Stroke::new(2.0, col));
            p.line_segment([c + vec2(-2.0, 9.0), c + vec2(-2.0, 6.0)], st);
            p.line_segment([c + vec2(2.0, 9.0), c + vec2(2.0, 6.0)], st);
        }
        // sillón dental
        "gabinetes" => {
            p.add(Shape::line(pts(c, s, &[(-8.0, -8.0), (-6.0, 2.0), (6.0, 2.0), (9.0, -1.0)]), st));
            p.add(Shape::line(pts(c, s, &[(-6.0, 2.0), (-6.0, 5.0), (7.0, 5.0)]), st));
            p.line_segment([c + vec2(0.5, 5.0), c + vec2(0.5, 9.0)], st);
            p.line_segment([c + vec2(-4.0, 9.0), c + vec2(5.0, 9.0)], st);
            p.circle_stroke(c + vec2(-8.5, -9.0), 1.6, st);
        }
        // chincheta de mapa
        "centros" => {
            p.add(Shape::line(pts(c, s, &[(0.0, 10.0), (-6.5, 1.0), (-7.5, -3.5), (-5.5, -8.0), (0.0, -10.0), (5.5, -8.0), (7.5, -3.5), (6.5, 1.0), (0.0, 10.0)]), st));
            p.circle_stroke(c + vec2(0.0, -3.0), 2.6, st);
        }
        // candado con persona
        "usuarios" => {
            p.circle_stroke(c + vec2(-3.0, -4.5), 3.5, st);
            p.add(Shape::line(pts(c, s, &[(-9.5, 8.5), (-9.0, 4.0), (-6.0, 1.0), (0.0, 1.0)]), st));
            p.rect_stroke(r(2.0, 2.0, 9.5, 9.0), CornerRadius::same(1), st, StrokeKind::Middle);
            p.add(Shape::line(pts(c, s, &[(3.5, 2.0), (3.5, -0.5), (5.75, -2.5), (8.0, -0.5), (8.0, 2.0)]), st));
        }
        "facturacion" => nav_icon(p, Screen::Facturas, c, col),
        "telefonia" => nav_icon(p, Screen::Listin, c, col),
        // globo terráqueo (cita online)
        "web" => {
            p.circle_stroke(c, 9.0, st);
            p.add(Shape::ellipse_stroke(c, vec2(4.0, 9.0), st));
            p.line_segment([c + vec2(-9.0, 0.0), c + vec2(9.0, 0.0)], st);
            p.line_segment([c + vec2(-7.6, -4.6), c + vec2(7.6, -4.6)], Stroke::new(1.3, col));
            p.line_segment([c + vec2(-7.6, 4.6), c + vec2(7.6, 4.6)], Stroke::new(1.3, col));
        }
        // sobre con una flecha de envío
        "correo" => {
            p.rect_stroke(r(-9.0, -6.5, 9.0, 7.5), CornerRadius::same(2), st, StrokeKind::Middle);
            p.add(Shape::line(pts(c, s, &[(-8.5, -5.5), (0.0, 1.5), (8.5, -5.5)]), st));
        }
        "whatsapp" => {
            p.circle_stroke(c, 8.5, st);
            p.add(Shape::line(pts(c, s, &[(-7.0, 5.0), (-9.0, 9.5), (-4.0, 8.0)]), st));
            p.add(Shape::line(pts(c, s, &[(-3.5, -3.5), (-2.0, -4.0), (-1.0, -1.5), (-2.0, -0.5), (0.5, 2.5), (1.5, 1.5), (4.0, 2.5), (3.5, 4.0), (1.5, 4.5), (-2.5, 2.0), (-4.0, -1.0), (-3.5, -3.5)]), Stroke::new(1.3, col)));
        }
        // receta: hoja con «Rx»
        "receta" => {
            p.add(Shape::line(pts(c, s, &[(3.0, -9.0), (-7.0, -9.0), (-7.0, 9.0), (7.0, 9.0), (7.0, -5.0), (3.0, -9.0), (3.0, -5.0), (7.0, -5.0)]), st));
            p.add(Shape::line(pts(c, s, &[(-4.0, 6.0), (-4.0, -2.5), (-1.0, -2.5), (0.5, -1.0), (-1.0, 1.0), (-4.0, 1.0)]), Stroke::new(1.5, col)));
            p.line_segment([c + vec2(-1.0, 1.0), c + vec2(3.5, 6.0)], Stroke::new(1.5, col));
            p.line_segment([c + vec2(0.5, 6.0), c + vec2(3.5, 2.5)], Stroke::new(1.5, col));
        }
        // destellos (inteligencia artificial)
        "ia" => {
            let star = |cx: f32, cy: f32, k: f32| pts(c, s, &[(cx, cy - 7.0 * k), (cx + 1.6 * k, cy - 1.6 * k), (cx + 7.0 * k, cy), (cx + 1.6 * k, cy + 1.6 * k), (cx, cy + 7.0 * k), (cx - 1.6 * k, cy + 1.6 * k), (cx - 7.0 * k, cy), (cx - 1.6 * k, cy - 1.6 * k), (cx, cy - 7.0 * k)]);
            p.add(Shape::line(star(-2.0, 1.5, 1.15), st));
            p.add(Shape::line(star(6.5, -6.0, 0.55), Stroke::new(1.4, col)));
        }
        // bandeja con flecha hacia dentro
        "importar" => {
            p.add(Shape::line(pts(c, s, &[(-9.0, 2.0), (-9.0, 9.0), (9.0, 9.0), (9.0, 2.0)]), st));
            p.line_segment([c + vec2(0.0, -10.0), c + vec2(0.0, 4.0)], st);
            p.add(Shape::line(pts(c, s, &[(-4.5, -0.5), (0.0, 4.0), (4.5, -0.5)]), st));
        }
        // disco con flecha circular
        "copias" => {
            p.add(Shape::line(pts(c, s, &[(-8.0, -3.0), (-6.5, -7.0), (-2.0, -9.5), (3.5, -9.0), (7.5, -5.5), (9.0, 0.0), (7.5, 5.5), (3.5, 9.0), (-2.0, 9.5), (-6.5, 7.0), (-8.5, 3.0)]), st));
            p.add(Shape::line(pts(c, s, &[(-10.0, -6.5), (-8.0, -3.0), (-4.5, -4.5)]), st));
            p.add(Shape::line(pts(c, s, &[(0.0, -4.5), (0.0, 0.5), (3.5, 3.0)]), st));
        }
        // carpeta
        "fichas" => {
            p.add(Shape::line(pts(c, s, &[(-10.0, -7.0), (-3.5, -7.0), (-1.5, -4.5), (10.0, -4.5), (10.0, 8.0), (-10.0, 8.0), (-10.0, -7.0)]), st));
            p.line_segment([c + vec2(-10.0, -1.0), c + vec2(10.0, -1.0)], st);
        }
        // dos fichas superpuestas
        "duplicados" => {
            p.rect_stroke(r(-9.0, -8.0, 3.0, 4.0), CornerRadius::same(2), st, StrokeKind::Middle);
            p.rect_stroke(r(-3.0, -2.0, 9.0, 10.0), CornerRadius::same(2), st, StrokeKind::Middle);
            p.circle_stroke(c + vec2(3.0, 2.5), 2.0, st);
            p.line_segment([c + vec2(-0.5, 7.5), c + vec2(6.5, 7.5)], st);
        }
        // reloj con lista
        "registro" => {
            p.circle_stroke(c, 9.0, st);
            p.add(Shape::line(pts(c, s, &[(0.0, -5.5), (0.0, 0.0), (4.0, 2.5)]), st));
        }
        // portapapeles con marcas de hecho (puesta en marcha)
        "marcha" => {
            p.rect_stroke(r(-8.0, -7.5, 8.0, 10.0), CornerRadius::same(2), st, StrokeKind::Middle);
            p.rect_stroke(r(-3.5, -10.0, 3.5, -6.0), CornerRadius::same(1), st, StrokeKind::Middle);
            p.add(Shape::line(pts(c, s, &[(-5.0, -1.0), (-3.5, 0.5), (-1.0, -2.5)]), Stroke::new(1.5, col)));
            p.line_segment([c + vec2(1.0, -1.0), c + vec2(5.0, -1.0)], Stroke::new(1.5, col));
            p.add(Shape::line(pts(c, s, &[(-5.0, 5.0), (-3.5, 6.5), (-1.0, 3.5)]), Stroke::new(1.5, col)));
            p.line_segment([c + vec2(1.0, 5.0), c + vec2(5.0, 5.0)], Stroke::new(1.5, col));
        }
        // tres equipos conectados
        "red" => {
            p.rect_stroke(r(-4.0, -10.0, 4.0, -4.0), CornerRadius::same(1), st, StrokeKind::Middle);
            p.rect_stroke(r(-10.5, 4.0, -3.5, 9.5), CornerRadius::same(1), st, StrokeKind::Middle);
            p.rect_stroke(r(3.5, 4.0, 10.5, 9.5), CornerRadius::same(1), st, StrokeKind::Middle);
            p.add(Shape::line(pts(c, s, &[(0.0, -4.0), (0.0, 0.5), (-7.0, 0.5), (-7.0, 4.0)]), st));
            p.add(Shape::line(pts(c, s, &[(0.0, 0.5), (7.0, 0.5), (7.0, 4.0)]), st));
        }
        _ => nav_icon(p, Screen::Config, c, col),
    }
}
