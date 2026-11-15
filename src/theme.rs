//! Estilo visual y pequeños componentes reutilizables.
//!
//! Línea visual «gabinete claro»: el programa se apoya en un fondo blanco
//! esmalte con un punto de verde, y encima van las tarjetas, blancas como la
//! porcelana. Lo elegido (el apartado del menú, la pestaña) sube como una
//! pieza blanca en relieve. Acento verde quirófano. Títulos y cifras en
//! Bricolage Grotesque; el resto, en Inter. Todas las pantallas usan estas
//! piezas, así que cambiar aquí cambia todo el programa.

use egui::{Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Mesh, Painter, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

// ── paleta ──
/// Esmalte: el fondo de las páginas.
pub const BG: Color32 = Color32::from_rgb(240, 244, 242);
/// Porcelana: las tarjetas.
pub const CARD: Color32 = Color32::WHITE;
/// El menú lateral: el esmalte, un tono más hondo.
pub const SIDEBAR: Color32 = Color32::from_rgb(230, 236, 233);
/// Las pantallas oscuras (entrada, sin conexión, pizarra): verde casi negro.
pub const DEEP: Color32 = Color32::from_rgb(7, 21, 19);
pub const DEEP_TOP: Color32 = Color32::from_rgb(15, 43, 38);
/// Verde quirófano: con letra blanca encima se lee bien (contraste 5,6).
pub const ACCENT: Color32 = Color32::from_rgb(10, 117, 100);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(7, 90, 77);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(222, 240, 234);
pub const HEADING: Color32 = Color32::from_rgb(15, 30, 27);
pub const TEXT: Color32 = Color32::from_rgb(38, 52, 49);
/// Muted (más oscuro con el contraste alto: ver [`set_high_contrast`]).
#[allow(non_snake_case)]
pub fn MUTED() -> Color32 {
    palette(high_contrast()).muted
}
/// Subtle (más oscuro con el contraste alto: ver [`set_high_contrast`]).
#[allow(non_snake_case)]
pub fn SUBTLE() -> Color32 {
    palette(high_contrast()).subtle
}
/// Border (más oscuro con el contraste alto: ver [`set_high_contrast`]).
#[allow(non_snake_case)]
pub fn BORDER() -> Color32 {
    palette(high_contrast()).border
}
/// Hairline (más oscuro con el contraste alto: ver [`set_high_contrast`]).
#[allow(non_snake_case)]
pub fn HAIRLINE() -> Color32 {
    palette(high_contrast()).hairline
}
pub const DANGER: Color32 = Color32::from_rgb(220, 53, 69);
pub const WARN: Color32 = Color32::from_rgb(217, 119, 6);
pub const OK: Color32 = Color32::from_rgb(5, 150, 105);
pub const INFO: Color32 = Color32::from_rgb(37, 99, 235);
pub const VIOLET: Color32 = Color32::from_rgb(124, 58, 237);
/// Ámbar hondo de los contadores (sala de espera): la cifra blanca se lee.
pub const BADGE_WAIT: Color32 = Color32::from_rgb(180, 83, 9);
pub const ROW_ALT: Color32 = Color32::from_rgb(248, 250, 249);
pub const ROW_HOVER: Color32 = Color32::from_rgb(241, 247, 244);
/// Fondo de los botones al pasar por encima y de las cabeceras de tabla.
pub const WASH: Color32 = Color32::from_rgb(243, 247, 245);
/// Borde de los botones (algo más marcado que el de las tarjetas).
pub const CONTROL: Color32 = Color32::from_rgb(210, 220, 216);
pub const CONTROL_HOVER: Color32 = Color32::from_rgb(170, 189, 182);

static HIGH_CONTRAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// ¿Contraste alto en este puesto? (ver [`Display`])
pub fn high_contrast() -> bool {
    HIGH_CONTRAST.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_high_contrast(on: bool) {
    HIGH_CONTRAST.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Los colores secundarios con y sin contraste alto.
pub struct Palette {
    pub muted: Color32,
    pub subtle: Color32,
    pub border: Color32,
    pub hairline: Color32,
}

pub fn palette(high: bool) -> Palette {
    if high {
        Palette { muted: Color32::from_rgb(50, 64, 60), subtle: Color32::from_rgb(62, 76, 72), border: Color32::from_rgb(102, 116, 111), hairline: Color32::from_rgb(152, 165, 160) }
    } else {
        Palette { muted: Color32::from_rgb(88, 104, 99), subtle: Color32::from_rgb(132, 148, 143), border: Color32::from_rgb(221, 228, 225), hairline: Color32::from_rgb(232, 238, 235) }
    }
}

/// Contraste entre dos colores (WCAG 2.1: de 1 a 21), para las pruebas.
#[cfg(test)]
pub fn contrast_ratio(a: Color32, b: Color32) -> f64 {
    let lum = |c: Color32| {
        let ch = |x: u8| {
            let v = x as f64 / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    };
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn tint(c: Color32, a: f32) -> Color32 {
    // Mezcla con blanco: a=0 → blanco, a=1 → color.
    let m = |x: u8| (255.0 - (255.0 - x as f32) * a) as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

/// Mezcla dos colores (t=0 → a, t=1 → b).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

// ── tipografía (Inter; títulos y cifras grandes, Bricolage Grotesque) ──

/// Títulos y cifras grandes (de 18 px para arriba).
pub fn display() -> egui::FontFamily {
    egui::FontFamily::Name("display".into())
}
pub fn df(size: f32) -> FontId {
    FontId::new(size, display())
}

/// Seminegrita.
pub fn sb() -> egui::FontFamily {
    egui::FontFamily::Name("semibold".into())
}
/// Media: botones, etiquetas.
pub fn md() -> egui::FontFamily {
    egui::FontFamily::Name("medium".into())
}
/// Negrita: títulos y cifras.
pub fn bold() -> egui::FontFamily {
    egui::FontFamily::Name("bold".into())
}

pub fn sbf(size: f32) -> FontId {
    FontId::new(size, sb())
}
pub fn mdf(size: f32) -> FontId {
    FontId::new(size, md())
}
pub fn bf(size: f32) -> FontId {
    FontId::new(size, bold())
}

/// Sombra de las tarjetas: amplia, muy suave y teñida de verde.
pub fn shadow() -> egui::Shadow {
    egui::Shadow { offset: [0, 3], blur: 16, spread: 0, color: Color32::from_rgba_unmultiplied(16, 44, 37, 15) }
}

/// Sombra de una pieza elegida (menú, pestañas): corta, para que parezca en relieve.
pub fn lift() -> egui::Shadow {
    egui::Shadow { offset: [0, 1], blur: 5, spread: 0, color: Color32::from_rgba_unmultiplied(16, 44, 37, 30) }
}

/// Pieza elegida: blanca, en relieve, con un borde finísimo.
pub fn tile(p: &Painter, r: Rect, radius: u8) {
    let cr = CornerRadius::same(radius);
    p.add(lift().as_shape(r, cr));
    p.rect(r, cr, CARD, Stroke::new(1.0, Color32::from_rgba_unmultiplied(16, 44, 37, 18)), StrokeKind::Inside);
}

fn fonts(ctx: &egui::Context) {
    use std::sync::Arc;
    let mut f = egui::FontDefinitions::default();
    let add = |f: &mut egui::FontDefinitions, k: &str, bytes: &'static [u8]| {
        f.font_data.insert(k.into(), Arc::new(egui::FontData::from_static(bytes)));
    };
    add(&mut f, "inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add(&mut f, "inter_md", include_bytes!("../assets/fonts/Inter-Medium.ttf"));
    add(&mut f, "inter_sb", include_bytes!("../assets/fonts/Inter-SemiBold.ttf"));
    add(&mut f, "inter_b", include_bytes!("../assets/fonts/Inter-Bold.ttf"));
    add(&mut f, "bricolage", include_bytes!("../assets/fonts/BricolageGrotesque-SemiBold.ttf"));
    let prop = f.families.entry(egui::FontFamily::Proportional).or_default();
    prop.insert(0, "inter".into());
    // los iconos y emojis siguen saliendo de las fuentes de reserva de egui
    let fallback: Vec<String> = prop.iter().skip(1).cloned().collect();
    for (fam, key) in [(sb(), "inter_sb"), (md(), "inter_md"), (bold(), "inter_b"), (display(), "bricolage")] {
        let mut v = vec![key.to_string()];
        v.extend(fallback.clone());
        f.families.insert(fam, v);
    }
    ctx.set_fonts(f);
}

pub fn apply(ctx: &egui::Context) {
    fonts(ctx);
    let mut style = (*ctx.global_style()).clone();
    style.visuals = egui::Visuals::light();
    let v = &mut style.visuals;
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = Color32::from_rgb(252, 253, 253);
    v.faint_bg_color = ROW_ALT;
    v.code_bg_color = WASH;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = Color32::from_rgb(206, 234, 225);
    v.selection.stroke = Stroke::new(1.5, ACCENT);
    v.hyperlink_color = ACCENT_DARK;
    v.window_stroke = Stroke::new(1.0, BORDER());
    v.window_corner_radius = CornerRadius::same(18);
    v.menu_corner_radius = CornerRadius::same(12);
    v.popup_shadow = egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_rgba_unmultiplied(12, 36, 30, 28) };
    v.window_shadow = egui::Shadow { offset: [0, 18], blur: 60, spread: 0, color: Color32::from_rgba_unmultiplied(7, 21, 19, 50) };
    let radius = CornerRadius::same(10);
    let w = &mut v.widgets;
    // separadores y bordes de tarjetas
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER());
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;
    // botones normales: blancos con borde fino
    w.inactive.weak_bg_fill = CARD;
    w.inactive.bg_fill = Color32::from_rgb(229, 236, 233);
    w.inactive.bg_stroke = Stroke::new(1.0, CONTROL);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = radius;
    w.inactive.expansion = 0.0;
    w.hovered.weak_bg_fill = WASH;
    w.hovered.bg_fill = Color32::from_rgb(220, 231, 226);
    w.hovered.bg_stroke = Stroke::new(1.0, CONTROL_HOVER);
    w.hovered.fg_stroke = Stroke::new(1.5, HEADING);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;
    w.active.weak_bg_fill = ACCENT_SOFT;
    w.active.bg_fill = Color32::from_rgb(206, 234, 225);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, ACCENT_DARK);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;
    w.open.weak_bg_fill = WASH;
    w.open.bg_stroke = Stroke::new(1.0, ACCENT);
    w.open.corner_radius = radius;
    style.spacing.item_spacing = vec2(10.0, 9.0);
    style.spacing.button_padding = vec2(15.0, 8.0);
    style.spacing.interact_size = vec2(44.0, 34.0);
    style.spacing.combo_height = 340.0;
    style.spacing.icon_width = 18.0;
    style.spacing.icon_width_inner = 10.0;
    style.spacing.menu_margin = Margin::same(8);
    style.spacing.window_margin = Margin::same(22);
    style.spacing.scroll = egui::style::ScrollStyle::thin();
    use egui::TextStyle::*;
    style.text_styles.insert(Body, FontId::proportional(15.0));
    style.text_styles.insert(Button, mdf(14.5));
    style.text_styles.insert(Small, FontId::proportional(12.5));
    style.text_styles.insert(Heading, df(26.0));
    style.text_styles.insert(Monospace, FontId::monospace(13.5));
    ctx.set_global_style(style);
}

/// Pantalla de este puesto (T-F6-06): modo tableta (controles y texto más
/// grandes para el dedo) y contraste alto. Se guarda en este ordenador
/// (`pantalla.json`), no en los datos de la clínica.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Display {
    #[serde(default)]
    pub tablet: bool,
    #[serde(default)]
    pub high_contrast: bool,
}

fn display_path() -> std::path::PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("pantalla.json")
}

pub fn load_display() -> Display {
    if cfg!(test) {
        return Display::default();
    }
    std::fs::read_to_string(display_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_display(d: Display) -> Result<(), String> {
    if cfg!(test) {
        return Ok(());
    }
    let p = display_path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(&d).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Tamaño mínimo de lo que se toca con el dedo en el modo tableta.
pub const TOUCH: Vec2 = vec2(52.0, 48.0);

/// El estilo con el ajuste de pantalla de este puesto.
pub fn apply_display(ctx: &egui::Context, d: Display) {
    set_high_contrast(d.high_contrast);
    apply_style(ctx, d);
}

/// El estilo de un ajuste de pantalla, sin tocar el contraste (que es de todo
/// el programa). Lo usa también la ventana que se crea en las pruebas, que
/// corren a la vez: así no deshacen el contraste que otra prueba comprueba.
pub fn apply_style(ctx: &egui::Context, d: Display) {
    apply(ctx);
    let mut style = (*ctx.global_style()).clone();
    if d.tablet {
        style.spacing.interact_size = TOUCH;
        style.spacing.button_padding = vec2(20.0, 12.0);
        style.spacing.item_spacing = vec2(12.0, 12.0);
        style.spacing.icon_width = 24.0;
        style.spacing.icon_width_inner = 14.0;
        // barras de desplazamiento anchas y siempre visibles (flotantes: no
        // quitan sitio a la pantalla)
        let mut sc = egui::style::ScrollStyle::thin();
        sc.bar_width = 14.0;
        sc.floating_width = 14.0;
        sc.dormant_handle_opacity = 0.7;
        sc.dormant_background_opacity = 0.3;
        style.spacing.scroll = sc;
        use egui::TextStyle::*;
        style.text_styles.insert(Body, FontId::proportional(17.0));
        style.text_styles.insert(Button, mdf(16.5));
        style.text_styles.insert(Small, FontId::proportional(14.0));
    }
    if d.high_contrast {
        let v = &mut style.visuals;
        v.override_text_color = Some(Color32::from_rgb(10, 15, 25));
        v.selection.stroke = Stroke::new(2.5, ACCENT_DARK);
        v.window_stroke = Stroke::new(1.5, BORDER());
        let w = &mut v.widgets;
        w.noninteractive.bg_stroke = Stroke::new(1.5, BORDER());
        w.inactive.bg_stroke = Stroke::new(1.5, Color32::from_rgb(70, 86, 81));
        w.inactive.fg_stroke = Stroke::new(1.5, Color32::from_rgb(10, 15, 25));
        w.hovered.bg_stroke = Stroke::new(2.0, HEADING);
        w.active.bg_stroke = Stroke::new(2.0, ACCENT_DARK);
    }
    ctx.set_global_style(style);
}

/// Una línea de texto que no ensancha lo que la contiene: si no cabe, se
/// corta con «…» y el texto entero sale al pasar por encima.
pub fn line(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(egui::Label::new(text).truncate())
}

/// Degradado vertical (para el menú lateral y las cabeceras).
pub fn vgradient(p: &Painter, r: Rect, top: Color32, bottom: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(r.left_top(), top);
    m.colored_vertex(r.right_top(), top);
    m.colored_vertex(r.left_bottom(), bottom);
    m.colored_vertex(r.right_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 3, 2);
    p.add(m);
}

/// Flecha fina (chevron) para los desplegables, en lugar del triángulo macizo.
pub fn chevron(_ui: &Ui, rect: Rect, v: &egui::style::WidgetVisuals, open: bool) {
    let c = rect.center();
    let (w, h) = (4.6, 2.6);
    let col = if open {
        ACCENT
    } else if v.bg_stroke.color == ACCENT {
        ACCENT_DARK
    } else {
        MUTED()
    };
    let s = Stroke::new(1.6, col);
    let dir = if open { -1.0 } else { 1.0 };
    let pts = vec![pos2(c.x - w, c.y - h * dir), pos2(c.x, c.y + h * dir), pos2(c.x + w, c.y - h * dir)];
    _ui.painter().add(egui::Shape::line(pts, s));
}

/// Botón cuadrado con una flecha fina a la izquierda o a la derecha.
pub fn arrow_button(ui: &mut Ui, left: bool, hint: &str) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 34.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, hint));
    let p = ui.painter();
    let hov = resp.hovered();
    p.rect(r, CornerRadius::same(10), if hov { WASH } else { CARD }, Stroke::new(1.0, if hov { CONTROL_HOVER } else { CONTROL }), StrokeKind::Inside);
    let c = r.center();
    let d = if left { 1.0 } else { -1.0 };
    let col = if hov { ACCENT_DARK } else { TEXT };
    p.add(egui::Shape::line(vec![pos2(c.x + 2.2 * d, c.y - 4.8), pos2(c.x - 2.4 * d, c.y), pos2(c.x + 2.2 * d, c.y + 4.8)], Stroke::new(1.7, col)));
    if hov {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(hint)
}

/// Desplegable con el estilo de la aplicación.
pub fn combo(id: impl egui::AsIdSalt) -> egui::ComboBox<'static> {
    egui::ComboBox::from_id_salt(id).icon(chevron).truncate()
}

/// Diálogo modal con márgenes amplios, esquinas redondeadas y sombra profunda.
pub fn modal(id: &str) -> egui::Modal {
    egui::Modal::new(egui::Id::unique(id))
        .frame(Frame::new().fill(CARD).corner_radius(CornerRadius::same(20)).inner_margin(Margin::symmetric(30, 26)).stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(16, 44, 37, 18))).shadow(egui::Shadow {
            offset: [0, 24],
            blur: 64,
            spread: 0,
            color: Color32::from_rgba_unmultiplied(7, 21, 19, 62),
        }))
        .backdrop_color(Color32::from_rgba_unmultiplied(7, 21, 19, 112))
}

/// Brillo radial suave: color en el centro que se desvanece hasta el borde,
/// en anillos concéntricos con caída suave (sin bandas visibles).
pub fn radial_glow(p: &Painter, center: egui::Pos2, radius: f32, color: Color32, alpha: f32) {
    const SEG: usize = 96;
    const RINGS: usize = 8;
    let mut m = Mesh::default();
    let col = |t: f32| {
        // caída tipo campana: 1 en el centro, 0 en el borde
        let a = (1.0 - t * t).powi(2) * alpha;
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (a * 255.0).round() as u8)
    };
    m.colored_vertex(center, col(0.0));
    for ring in 1..=RINGS {
        let t = ring as f32 / RINGS as f32;
        for s in 0..SEG {
            let ang = s as f32 / SEG as f32 * std::f32::consts::TAU;
            m.colored_vertex(center + egui::vec2(ang.cos(), ang.sin()) * radius * t, col(t));
        }
    }
    let idx = |ring: usize, s: usize| (1 + (ring - 1) * SEG + s % SEG) as u32;
    for s in 0..SEG {
        m.add_triangle(0, idx(1, s), idx(1, s + 1));
    }
    for ring in 1..RINGS {
        for s in 0..SEG {
            m.add_triangle(idx(ring, s), idx(ring + 1, s), idx(ring + 1, s + 1));
            m.add_triangle(idx(ring, s), idx(ring + 1, s + 1), idx(ring, s + 1));
        }
    }
    p.add(m);
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new().fill(CARD).stroke(Stroke::new(1.0, BORDER())).corner_radius(CornerRadius::same(18)).inner_margin(Margin::symmetric(22, 20)).shadow(shadow()).show(ui, add).inner
}

/// Título de pantalla con subtítulo.
pub fn title(ui: &mut Ui, text: &str, sub: &str) {
    // siempre en vertical (título y debajo la explicación), también dentro de
    // una fila con botones a la derecha
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        ui.add_space(2.0);
        ui.label(RichText::new(text).font(df(28.0)).color(HEADING));
        if !sub.is_empty() {
            ui.label(RichText::new(sub).font(mdf(14.0)).color(MUTED()));
        }
        ui.add_space(12.0);
    });
}

pub fn section(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(sbf(16.5)).color(HEADING));
    ui.add_space(4.0);
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).color(MUTED()).size(13.8));
}

/// Etiqueta de color redondeada.
pub fn pill(ui: &mut Ui, text: &str, color: Color32) -> egui::Response {
    let font = mdf(12.5);
    let fg = mix(color, HEADING, 0.12);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, fg);
    let size = galley.size() + vec2(22.0, 10.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    // legible para lectores de pantalla (y para las pruebas)
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
    let r = CornerRadius::same((size.y / 2.0) as u8);
    ui.painter().rect_filled(rect, r, tint(color, 0.11));
    ui.painter().rect_stroke(rect, r, Stroke::new(1.0, tint(color, 0.22)), StrokeKind::Inside);
    ui.painter().galley(pos2(rect.min.x + 11.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    resp
}

/// Iniciales sobre un círculo de color (avatar).
pub fn avatar(p: &Painter, c: egui::Pos2, radius: f32, name: &str, color: Color32) {
    let initials: String = name.split_whitespace().filter_map(|w| w.chars().next()).filter(|c| c.is_alphabetic()).take(2).collect::<String>().to_uppercase();
    p.circle_filled(c, radius, tint(color, 0.16));
    p.circle_stroke(c, radius, Stroke::new(1.0, tint(color, 0.35)));
    p.text(c, Align2::CENTER_CENTER, initials, sbf(radius * 0.78), mix(color, HEADING, 0.25));
}

/// Tarjeta de indicador (KPI).
pub fn kpi(ui: &mut Ui, width: f32, label: &str, value: &str, sub: &str, color: Color32) {
    kpi_icon(ui, width, label, value, sub, color, None);
}

/// Indicador con el icono de una sección del programa.
pub fn kpi_icon(ui: &mut Ui, width: f32, label: &str, value: &str, sub: &str, color: Color32, icon: Option<crate::app::Screen>) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 118.0), Sense::hover());
    let p = ui.painter();
    let r = CornerRadius::same(18);
    p.add(shadow().as_shape(rect, r));
    p.rect_filled(rect, r, CARD);
    p.rect_stroke(rect, r, Stroke::new(1.0, BORDER()), StrokeKind::Inside);
    // icono en un cuadrado redondeado de color (solo si hay sitio)
    let x = match icon {
        Some(s) if width >= 210.0 => {
            let badge = Rect::from_min_size(rect.min + vec2(20.0, 20.0), vec2(42.0, 42.0));
            p.rect_filled(badge, CornerRadius::same(12), tint(color, 0.12));
            crate::icons::nav_icon(p, s, badge.center(), color);
            76.0
        }
        _ => {
            // sin icono: una pequeña marca de color junto a la etiqueta
            p.rect_filled(Rect::from_min_size(rect.min + vec2(20.0, 25.0), vec2(8.0, 8.0)), CornerRadius::same(3), color);
            20.0
        }
    };
    let label_x = if x == 20.0 { 36.0 } else { x };
    let clip = p.with_clip_rect(rect.shrink(2.0));
    clip.text(rect.min + vec2(label_x, 20.0), Align2::LEFT_TOP, label, mdf(13.2), MUTED());
    // el importe se reduce para que quepa entero en la tarjeta
    let room = rect.width() - x - 16.0;
    let mut size = 26.0;
    while size > 17.0 && p.layout_no_wrap(value.to_string(), df(size), HEADING).size().x > room {
        size -= 1.0;
    }
    clip.text(rect.min + vec2(x, 37.0 + (26.0 - size) * 0.6), Align2::LEFT_TOP, value, df(size), HEADING);
    if !sub.is_empty() {
        let sub_pos = rect.min + vec2(20.0, 88.0);
        clip.circle_filled(sub_pos + vec2(3.0, 8.0), 3.0, color);
        clip.text(sub_pos + vec2(12.0, 0.0), Align2::LEFT_TOP, sub, mdf(12.8), mix(color, HEADING, 0.15));
    }
}

pub fn primary(ui: &mut Ui, text: &str) -> egui::Response {
    // al pasar por encima, un verde algo más hondo (egui da lo del fotograma anterior)
    let hov = ui.is_enabled() && ui.ctx().read_response(ui.next_auto_id()).is_some_and(|r| r.hovered());
    let fill = if hov { ACCENT_DARK } else { ACCENT };
    ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE).font(sbf(14.5))).fill(fill).stroke(Stroke::new(1.0, fill)).corner_radius(CornerRadius::same(10)).min_size(vec2(0.0, 38.0)))
}

/// Botón secundario: blanco con borde.
pub fn secondary(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).font(mdf(14.5)).color(HEADING)).corner_radius(CornerRadius::same(10)).min_size(vec2(0.0, 38.0)))
}

pub fn danger(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE).font(sbf(14.5))).fill(DANGER).stroke(Stroke::new(1.0, mix(DANGER, HEADING, 0.2))).corner_radius(CornerRadius::same(10)).min_size(vec2(0.0, 38.0)))
}

/// Campo con etiqueta encima.
pub fn field(ui: &mut Ui, label: &str, value: &mut String, width: f32) -> egui::Response {
    // en una fila que se parte, si no cabe en lo que queda, a la línea
    // siguiente (egui no sabe partir un bloque vertical como este)
    let want = width * 1.1;
    if ui.layout().main_wrap && ui.available_size_before_wrap().x < want && ui.min_rect().width() > 1.0 {
        ui.end_row();
    }
    // y lo que quede en la fila: el último campo se estrecha en vez de salirse
    // de la ventana (a 1280 px de ancho)
    let w = want.min(ui.available_width().max(60.0));
    ui.vertical(|ui| {
        ui.label(RichText::new(label).font(mdf(12.8)).color(MUTED()));
        ui.add_space(-3.0);
        ui.add_sized([w, 36.0], egui::TextEdit::singleline(value).margin(vec2(10.0, 8.0)))
    })
    .inner
}

/// Fila «etiqueta: valor» de una ficha, con una línea fina debajo.
pub fn info_row(ui: &mut Ui, label: &str, value: &str) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
    let p = ui.painter();
    p.text(pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, label, mdf(13.4), MUTED());
    let vx = rect.left() + 130.0f32.min(w * 0.4);
    let mut job = egui::text::LayoutJob::simple_singleline(if value.is_empty() { "—".to_string() } else { value.to_string() }, mdf(14.2), HEADING);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.right() - vx);
    let g = p.layout_job(job);
    p.galley(pos2(vx, rect.center().y - g.size().y / 2.0), g, HEADING);
    p.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, HAIRLINE()));
    ui.add_space(-ui.spacing().item_spacing.y + 2.0);
}

/// Tabla sencilla con cabecera, separadores finos, hover y clic de fila.
/// Ancho del contenido de una página: lo disponible, pero sin pasar del borde
/// visible ni del de la ventana (menos el margen del panel y la barra de
/// desplazamiento). A 1280 px, así nada se sale por la derecha.
pub fn fit_width(ui: &Ui) -> f32 {
    let x = ui.cursor().min.x;
    ui.available_width().min(ui.clip_rect().max.x - x - 6.0).min(ui.ctx().content_rect().max.x - x - 40.0).max(0.0)
}

/// Ancho que queda hasta el borde visible (el de la ventana o el panel): una
/// fila de tabla no pasa de ahí aunque el espacio disponible diga más.
fn visible_width(ui: &Ui) -> f32 {
    ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 4.0).max(0.0)
}

pub struct Table<'a> {
    #[allow(dead_code)]
    pub id: &'a str,
    pub cols: Vec<(&'a str, f32)>,
    pub row_h: f32,
}

impl<'a> Table<'a> {
    pub fn new(id: &'a str, cols: Vec<(&'a str, f32)>) -> Self {
        Self { id, cols, row_h: 40.0 }
    }

    /// Factor para que las columnas ocupen todo el ancho disponible. Se
    /// estrechan hasta la mitad antes que salirse del panel (cada celda recorta
    /// su texto): a 1280 px de ancho, una tabla no empuja el panel de al lado
    /// fuera de la ventana.
    pub fn scale(&self, ui: &Ui) -> f32 {
        let w: f32 = self.cols.iter().map(|c| c.1).sum();
        (visible_width(ui) / w.max(1.0)).clamp(0.5, 2.5)
    }

    pub fn header(&self, ui: &mut Ui) {
        let k = self.scale(ui);
        let w: f32 = self.cols.iter().map(|c| c.1).sum::<f32>() * k;
        let (rect, _) = ui.allocate_exact_size(vec2(w.max(visible_width(ui)), 34.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(8), WASH);
        let mut x = rect.min.x + 14.0;
        for (name, cw) in &self.cols {
            let clip = Rect::from_min_size(pos2(x, rect.min.y), vec2(cw * k - 6.0, rect.height()));
            p.with_clip_rect(clip).text(pos2(x, rect.center().y), Align2::LEFT_CENTER, name.to_uppercase(), sbf(11.3), SUBTLE());
            x += cw * k;
        }
        ui.add_space(-4.0);
    }

    /// Dibuja una fila; devuelve la respuesta de clic.
    pub fn row(&self, ui: &mut Ui, idx: usize, selected: bool, cells: &[(String, Color32)]) -> egui::Response {
        let _ = idx;
        let k = self.scale(ui);
        let w: f32 = self.cols.iter().map(|c| c.1).sum::<f32>() * k;
        let (rect, resp) = ui.allocate_exact_size(vec2(w.max(visible_width(ui)), self.row_h), Sense::click());
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, CornerRadius::same(8), tint(ACCENT, 0.10));
            p.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 8.0), vec2(3.0, rect.height() - 16.0)), CornerRadius::same(2), ACCENT);
        } else if resp.hovered() {
            p.rect_filled(rect, CornerRadius::same(8), ROW_HOVER);
        }
        p.hline(rect.x_range().shrink(6.0), rect.max.y - 0.5, Stroke::new(1.0, HAIRLINE()));
        let mut x = rect.min.x + 14.0;
        for (i, (_, cw)) in self.cols.iter().enumerate() {
            let cw = cw * k;
            if let Some((txt, col)) = cells.get(i) {
                let clip = Rect::from_min_size(pos2(x, rect.min.y), vec2(cw - 8.0, rect.height()));
                let font = if i == 0 && selected { mdf(14.3) } else { FontId::proportional(14.3) };
                p.with_clip_rect(clip).text(pos2(x, rect.center().y), Align2::LEFT_CENTER, txt, font, *col);
            }
            x += cw;
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        resp
    }
}

pub fn cell(s: impl Into<String>) -> (String, Color32) {
    (s.into(), TEXT)
}

pub fn cellc(s: impl Into<String>, c: Color32) -> (String, Color32) {
    (s.into(), c)
}

pub fn right_aligned<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.with_layout(Layout::right_to_left(Align::Center), add).inner
}

/// Estado vacío: círculo suave y texto centrado.
pub fn empty_state(ui: &mut Ui, text: &str) {
    ui.add_space(22.0);
    ui.vertical_centered(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 22.0, ACCENT_SOFT);
        ui.painter().circle_stroke(r.center(), 9.0, Stroke::new(2.0, mix(ACCENT, SUBTLE(), 0.4)));
        ui.add_space(6.0);
        ui.label(RichText::new(text).color(MUTED()).size(14.5));
    });
    ui.add_space(22.0);
}

/// Barra horizontal de progreso simple.
pub fn bar(ui: &mut Ui, width: f32, frac: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 8.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(226, 233, 230));
    let mut r = rect;
    r.set_width(rect.width() * frac.clamp(0.0, 1.0));
    ui.painter().rect_filled(r, CornerRadius::same(4), color);
}

/// Ancho de cada tarjeta KPI para que `n` quepan en una fila
/// (separadas por `add_space(4)` más el espaciado normal).
pub fn kpi_w(ui: &Ui, n: usize) -> f32 {
    // entre tarjetas: el espacio entre elementos más los 4 px que añade cada pantalla
    let gap = 4.0 + ui.spacing().item_spacing.x + 1.0;
    ((ui.available_width() - gap * (n as f32 - 1.0)) / n as f32).floor().max(120.0)
}

/// Texto alineado a la izquierda en una celda de ancho fijo (recorta si no cabe).
pub fn lab(ui: &mut Ui, w: f32, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.allocate_ui_with_layout(vec2(w, 30.0), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_width(w);
        ui.add(egui::Label::new(text).truncate())
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contraste (WCAG 2.1) sobre el blanco de las tarjetas y el fondo: el
    /// texto secundario, al menos 4,5 (AA); con contraste alto, 7 (AAA) el
    /// texto y 3 los bordes (componentes de la interfaz).
    #[test]
    fn the_secondary_colours_meet_the_contrast_levels() {
        for bg in [CARD, BG] {
            let n = palette(false);
            assert!(contrast_ratio(n.muted, bg) >= 4.5, "texto secundario normal: {:.2}", contrast_ratio(n.muted, bg));
            assert!(contrast_ratio(TEXT, bg) >= 7.0 && contrast_ratio(HEADING, bg) >= 7.0);
            let h = palette(true);
            for (name, c) in [("secundario", h.muted), ("tenue", h.subtle)] {
                assert!(contrast_ratio(c, bg) >= 7.0, "{name} con contraste alto: {:.2}", contrast_ratio(c, bg));
            }
            assert!(contrast_ratio(h.border, bg) >= 3.0, "bordes con contraste alto: {:.2}", contrast_ratio(h.border, bg));
            assert!(contrast_ratio(h.hairline, bg) > contrast_ratio(n.hairline, bg));
        }
        assert!((contrast_ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
    }

    /// La letra blanca de los botones verdes y de los contadores del menú se
    /// lee (AA: 4,5), y el texto secundario del menú, sobre su esmalte, también.
    #[test]
    fn white_text_reads_on_the_green_buttons_and_the_menu_counters() {
        for (name, c) in [("verde", ACCENT), ("verde hondo", ACCENT_DARK), ("ámbar", BADGE_WAIT), ("violeta", VIOLET), ("oscuro", DEEP)] {
            let k = contrast_ratio(Color32::WHITE, c);
            assert!(k >= 4.5, "letra blanca sobre {name}: {k:.2}");
        }
        assert!(contrast_ratio(palette(false).muted, SIDEBAR) >= 4.5);
        assert!(contrast_ratio(HEADING, SIDEBAR) >= 7.0);
    }
}
