//! Imágenes y enlaces: galería de radiografías/fotos/TAC del paciente con
//! visor, y enlaces con los programas de los equipos de imagen (RVG,
//! ortopantomógrafo, TAC/CBCT, cámara intraoral, escáner) mediante el
//! fichero puente IMCONECT.ini, como hace el programa original.

use crate::app::{App, PatTab};
use crate::model::*;
use crate::theme::*;
use egui::{Align2, Color32, ColorImage, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, pos2, vec2};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Default)]
pub struct Adjust {
    pub brightness: i32,
    pub contrast: i32,
    pub invert: bool,
    pub rot: u8,
}

pub struct ImgState {
    pub selected: Option<u32>,
    pub filter: Option<ImageKind>,
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub adjust: Adjust,
    cache: HashMap<(u32, bool), (TextureHandle, [usize; 2], Adjust)>,
    pub new_path: String,
    pub new_kind: ImageKind,
    pub new_teeth: String,
    pub edit_connector: Option<u32>,
    /// Visor de DICOM/CBCT y STL (ver `visor3d`).
    pub v3d: crate::screens::visor3d::V3d,
    /// Imagen con la que se compara la elegida.
    pub compare: Option<u32>,
    /// Imágenes de los aparatos que esperan a que alguien diga de quién son.
    pub inbox: Vec<crate::entrada::Found>,
    /// Las que se han descartado (hasta cerrar el programa).
    pub dismissed: std::collections::HashSet<PathBuf>,
    /// La búsqueda en las carpetas de los aparatos, en otro hilo.
    scan: Option<std::sync::mpsc::Receiver<Vec<crate::entrada::Found>>>,
    last_scan: f64,
}

impl Default for ImgState {
    fn default() -> Self {
        ImgState {
            selected: None,
            filter: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            adjust: Adjust::default(),
            cache: HashMap::new(),
            new_path: String::new(),
            new_kind: ImageKind::Periapical,
            new_teeth: String::new(),
            edit_connector: Some(1),
            v3d: Default::default(),
            compare: None,
            inbox: vec![],
            dismissed: Default::default(),
            scan: None,
            last_scan: f64::NEG_INFINITY,
        }
    }
}

// ───────────────────────────── generación de imágenes de demostración ─────────────────────────────

fn hash(x: u32, y: u32, s: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ s.wrapping_mul(2_147_483_647);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

struct Canvas {
    w: usize,
    h: usize,
    v: Vec<f32>,
}

impl Canvas {
    fn new(w: usize, h: usize, base: f32) -> Self {
        Canvas { w, h, v: vec![base; w * h] }
    }
    /// Diente radiográfico suave: esmalte muy radiopaco, dentina, cámara
    /// pulpar y conductos radiolúcidos, ligamento periodontal oscuro.
    #[allow(clippy::too_many_arguments)]
    fn tooth(&mut self, cx: f32, edge_y: f32, w: f32, len: f32, up: bool, roots: u32, endo: bool, implant: bool, tilt: f32) {
        let dir = if up { -1.0 } else { 1.0 };
        let crown_f = 0.36;
        let x0 = (cx - w * 1.2 - len * tilt.abs()) as i32;
        let x1 = (cx + w * 1.2 + len * tilt.abs()) as i32;
        let (y0, y1) = if up { ((edge_y - len - 4.0) as i32, (edge_y + 6.0) as i32) } else { ((edge_y - 6.0) as i32, (edge_y + len + 4.0) as i32) };
        let smooth = |e0: f32, e1: f32, x: f32| {
            let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        for py in y0.max(0)..y1.min(self.h as i32) {
            for px in x0.max(0)..x1.min(self.w as i32) {
                let v = (py as f32 - edge_y) * dir / len; // 0 borde → 1 ápice
                if !(-0.02..=1.02).contains(&v) {
                    continue;
                }
                let u = px as f32 - (cx + tilt * v * len);
                let mut best: f32 = 0.0;
                if v < crown_f {
                    // corona abombada
                    let s = v / crown_f;
                    let hw = w * (0.40 + 0.10 * (s * std::f32::consts::PI).sin() - 0.04 * s);
                    let rnd = if s < 0.12 { (s / 0.12).sqrt() } else { 1.0 };
                    let hw = hw * (0.75 + 0.25 * rnd);
                    let d = u.abs() / hw;
                    if d < 1.06 {
                        let body = 1.0 - smooth(0.9, 1.06, d);
                        let enamel = smooth(0.55, 0.95, d) * 0.22 + (1.0 - smooth(0.0, 0.35, s)) * 0.18;
                        let mut a = (0.50 + enamel) * body;
                        // cámara pulpar
                        let pw = 0.30 * smooth(0.35, 0.75, s);
                        if d < pw {
                            a = if endo { a.max(0.95) } else { a * (0.55 + 0.45 * smooth(0.0, pw, d)) };
                        }
                        best = best.max(a);
                    }
                } else if implant {
                    let s = (v - crown_f) / (1.0 - crown_f);
                    let hw = w * (0.22 - 0.06 * s * s);
                    let d = u.abs() / hw;
                    if d < 1.0 && s < 0.92 {
                        let thread = if ((v * len) as i32 / 5) % 2 == 0 { 0.06 } else { -0.02 };
                        best = best.max((0.96 + thread) * (1.0 - smooth(0.85, 1.0, d)));
                    }
                } else {
                    let s = (v - crown_f) / (1.0 - crown_f);
                    let offs: Vec<f32> = match roots {
                        1 => vec![0.0],
                        2 => vec![-1.0, 1.0],
                        _ => vec![-1.0, 0.0, 1.0],
                    };
                    for o in offs {
                        let spread = if roots > 1 { w * 0.22 * (0.6 + s * 0.8) * o } else { 0.0 };
                        let base = if roots > 1 { w * 0.40 / roots as f32 * 1.2 } else { w * 0.38 };
                        let hw = base * (1.0 - s * 0.82).max(0.06);
                        let d = (u - spread).abs() / hw;
                        // ligamento periodontal: línea oscura alrededor de la raíz
                        if d < 1.18 {
                            let body = 1.0 - smooth(0.88, 1.0, d);
                            let mut a = 0.40 * body + 0.06 * (1.0 - d.min(1.0));
                            if d > 1.0 {
                                a = -0.06 * (1.0 - smooth(1.0, 1.18, d));
                            }
                            if d < 0.16 && s < 0.97 {
                                a = if endo { 0.95 } else { a * 0.5 };
                            }
                            best = if a < 0.0 && best == 0.0 { a } else { best.max(a) };
                        }
                    }
                }
                if best != 0.0 {
                    let i = py as usize * self.w + px as usize;
                    self.v[i] = (self.v[i] + best).clamp(0.0, 1.4);
                }
            }
        }
    }
    fn image(&self, seed: u32, color: Option<[f32; 3]>) -> ColorImage {
        let mut px = Vec::with_capacity(self.w * self.h * 4);
        for y in 0..self.h {
            for x in 0..self.w {
                let n = hash(x as u32 / 2, y as u32 / 2, seed) * 0.06 - 0.03;
                // viñeteado
                let dx = x as f32 / self.w as f32 - 0.5;
                let dy = y as f32 / self.h as f32 - 0.5;
                let vig = 1.0 - (dx * dx + dy * dy) * 0.9;
                let v = ((self.v[y * self.w + x] + n) * vig).clamp(0.0, 1.0);
                match color {
                    None => {
                        let g = (v * 255.0) as u8;
                        px.extend_from_slice(&[g, g, g, 255]);
                    }
                    Some(c) => {
                        px.extend_from_slice(&[(v * c[0] * 255.0).min(255.0) as u8, (v * c[1] * 255.0).min(255.0) as u8, (v * c[2] * 255.0).min(255.0) as u8, 255]);
                    }
                }
            }
        }
        ColorImage::from_rgba_unmultiplied([self.w, self.h], &px)
    }
}

fn demo_image(img: &PatientImage) -> ColorImage {
    let seed = img.id;
    let endo = fold(&img.notes).contains("endo") || fold(&img.notes).contains("conducto");
    match img.kind {
        ImageKind::Panoramica | ImageKind::Cefalometrica => {
            let (w, h) = (1200usize, 560usize);
            let mut c = Canvas::new(w, h, 0.10);
            // maxilar y mandíbula (hueso)
            for y in 0..h {
                for x in 0..w {
                    let fx = x as f32 / w as f32 - 0.5;
                    let fy = y as f32 / h as f32;
                    let curve = 0.5 + 0.22 * (fx * 2.0).powi(2);
                    let d = (fy - curve).abs();
                    let bone = (0.32 - d).max(0.0) * 1.1;
                    c.v[y * w + x] += bone + hash(x as u32 / 3, y as u32 / 3, seed) * 0.05;
                }
            }
            for i in 0..16 {
                let fx = (i as f32 + 0.5) / 16.0 - 0.5;
                let cx = w as f32 / 2.0 + fx * w as f32 * 0.86;
                let base = h as f32 * (0.5 + 0.22 * (fx * 2.0).powi(2));
                let molar = fx.abs() > 0.3;
                let tw = if molar { 56.0 } else { 40.0 };
                let roots = if molar { 2 } else { 1 };
                let tilt = -fx * 0.25;
                c.tooth(cx, base - 8.0, tw, 190.0, true, if molar { 3 } else { 1 }, endo && i == 2, false, tilt);
                let implant = img.patient_id == 101 && (i == 2 || i == 13);
                c.tooth(cx, base + 8.0, tw, 200.0, false, roots, false, implant, -tilt);
            }
            c.image(seed, None)
        }
        ImageKind::Periapical | ImageKind::AletaMordida => {
            let (w, h) = (420usize, 560usize);
            let mut c = Canvas::new(w, h, 0.14);
            for y in 0..h {
                for x in 0..w {
                    c.v[y * w + x] += 0.16 + hash(x as u32 / 5, y as u32 / 5, seed) * 0.07 + hash(x as u32 / 2, y as u32 / 2, seed + 7) * 0.04;
                }
            }
            let up = img.teeth.first().map(|t| matches!(t / 10, 1 | 2)).unwrap_or(false);
            let edge = if up { h as f32 * 0.88 } else { h as f32 * 0.12 };
            let molar = img.teeth.first().map(|t| t % 10 >= 6).unwrap_or(false);
            for (i, dx) in [-140.0f32, 0.0, 140.0].iter().enumerate() {
                let main = i == 1;
                let implant = main && fold(&img.notes).contains("implante");
                c.tooth(w as f32 / 2.0 + dx, edge, if molar { 120.0 } else { 90.0 }, 470.0, up, if molar { 2 } else { 1 }, main && endo, implant, 0.0);
            }
            c.image(seed, None)
        }
        ImageKind::Tac => {
            // montaje de cortes: axial, coronal, sagital y reconstrucción
            let (w, h) = (900usize, 640usize);
            let mut c = Canvas::new(w, h, 0.03);
            let panels = [(0usize, 0usize), (450, 0), (0, 320), (450, 320)];
            for (pi, (ox, oy)) in panels.iter().enumerate() {
                for y in 4..316 {
                    for x in 4..446 {
                        let fx = x as f32 / 446.0 - 0.5;
                        let fy = y as f32 / 316.0 - 0.5;
                        let v = match pi {
                            0 => {
                                // axial: arcada en U
                                let r = ((fx / 0.38).powi(2) + ((fy + 0.25) / 0.6).powi(2)).sqrt();
                                let band = (0.12 - (r - 0.75).abs()).max(0.0) * 6.0;
                                let ang = fx.atan2(fy + 0.25);
                                let teeth = ((ang * 9.0).sin().abs()).powf(0.4);
                                if fy + 0.25 > 0.0 || r < 0.6 { band * teeth } else { 0.0 }
                            }
                            1 | 2 => {
                                let bone = (0.35 - fy.abs()).max(0.0) * 1.4;
                                let tooth = if fx.abs() < 0.08 && fy.abs() < 0.32 { 0.6 } else { 0.0 };
                                bone * 0.6 + tooth
                            }
                            _ => {
                                let r = (fx * fx + fy * fy).sqrt();
                                let shade = (0.42 - r).max(0.0) * 2.2;
                                let teeth = if fy.abs() < 0.06 { ((fx * 40.0).sin().abs()).powf(0.3) * 0.5 } else { 0.0 };
                                shade + teeth
                            }
                        };
                        c.v[(oy + y) * w + ox + x] = 0.04 + v + hash(x as u32 / 2, y as u32 / 2, seed + pi as u32) * 0.04;
                    }
                }
            }
            c.image(seed, None)
        }
        ImageKind::FotoIntraoral | ImageKind::FotoExtraoral => {
            let (w, h) = (800usize, 520usize);
            let mut px = Vec::with_capacity(w * h * 4);
            for y in 0..h {
                for x in 0..w {
                    let fx = x as f32 / w as f32 - 0.5;
                    let fy = y as f32 / h as f32 - 0.5;
                    let lip = ((fx / 0.48).powi(2) + (fy / 0.42).powi(2)).sqrt();
                    let n = hash(x as u32 / 3, y as u32 / 3, seed) * 0.05;
                    let (r, g, b) = if lip > 1.0 {
                        (0.62, 0.36, 0.34)
                    } else {
                        let gum = (fy.abs() - 0.12).max(0.0) * 2.4;
                        let k = ((fx * 18.0).cos() * 0.5 + 0.5).powf(0.2);
                        let tooth = if fy.abs() < 0.16 { 0.95 * k + 0.25 * (1.0 - k) } else { 0.0 };
                        if tooth > 0.3 { (tooth, tooth * 0.97, tooth * 0.9) } else { (0.78 - gum * 0.2, 0.42, 0.45) }
                    };
                    px.extend_from_slice(&[((r + n) * 255.0).min(255.0) as u8, ((g + n) * 255.0).min(255.0) as u8, ((b + n) * 255.0).min(255.0) as u8, 255]);
                }
            }
            ColorImage::from_rgba_unmultiplied([w, h], &px)
        }
        _ => {
            let mut c = Canvas::new(600, 420, 0.2);
            for i in 0..8 {
                c.tooth(60.0 + i as f32 * 70.0, 120.0, 50.0, 160.0, false, 1, false, false, 0.0);
            }
            c.image(seed, Some([0.8, 0.85, 1.0]))
        }
    }
}

fn load_file(path: &str) -> Option<ColorImage> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    // limitar tamaño para la textura
    let img = if w > 2400 || h > 2400 {
        let k = 2400.0 / w.max(h) as f32;
        image::imageops::resize(&img, (w as f32 * k) as u32, (h as f32 * k) as u32, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    Some(ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

fn adjust(img: &ColorImage, a: Adjust) -> ColorImage {
    let [w, h] = img.size;
    let c = (a.contrast as f32 / 100.0 + 1.0).max(0.05);
    let b = a.brightness as f32 / 100.0;
    let f = |v: u8| -> u8 {
        let mut x = v as f32 / 255.0;
        x = (x - 0.5) * c + 0.5 + b;
        if a.invert {
            x = 1.0 - x;
        }
        (x.clamp(0.0, 1.0) * 255.0) as u8
    };
    let (nw, nh) = if a.rot % 2 == 1 { (h, w) } else { (w, h) };
    let mut px = vec![0u8; nw * nh * 4];
    for y in 0..h {
        for x in 0..w {
            let p = img.pixels[y * w + x];
            let (nx, ny) = match a.rot % 4 {
                1 => (h - 1 - y, x),
                2 => (w - 1 - x, h - 1 - y),
                3 => (y, w - 1 - x),
                _ => (x, y),
            };
            let i = (ny * nw + nx) * 4;
            px[i] = f(p.r());
            px[i + 1] = f(p.g());
            px[i + 2] = f(p.b());
            px[i + 3] = 255;
        }
    }
    ColorImage::from_rgba_unmultiplied([nw, nh], &px)
}

fn file_kind(path: &str) -> &'static str {
    let l = path.to_lowercase();
    if l.ends_with(".dcm") || l.ends_with(".dicom") || Path::new(path).is_dir() {
        "dicom"
    } else if l.ends_with(".stl") || l.ends_with(".ply") || l.ends_with(".obj") {
        "3d"
    } else {
        "img"
    }
}

/// Si una imagen se puede enviar al análisis con IA (sólo imágenes planas).
pub fn analyzable(img: &PatientImage) -> Result<(), String> {
    if img.demo {
        return Ok(());
    }
    match file_kind(&img.path) {
        "dicom" => Err("Los estudios DICOM o de TAC no se analizan directamente: exporte un corte como imagen (PNG o JPG) y añádalo a la galería.".into()),
        "3d" => Err("Los modelos 3D no se pueden analizar como imagen.".into()),
        _ if !Path::new(&img.path).exists() => Err("El fichero de la imagen ya no existe en esta ruta.".into()),
        _ => Ok(()),
    }
}

/// Píxeles de la imagen para el análisis con IA (sin metadatos del fichero).
pub fn rgb_for_ai(img: &PatientImage) -> Result<image::RgbImage, String> {
    analyzable(img)?;
    if img.demo {
        let c = demo_image(img);
        let [w, h] = c.size;
        let mut out = image::RgbImage::new(w as u32, h as u32);
        for (i, px) in c.pixels.iter().enumerate() {
            out.put_pixel((i % w) as u32, (i / w) as u32, image::Rgb([px.r(), px.g(), px.b()]));
        }
        return Ok(out);
    }
    image::open(&img.path).map(|i| i.to_rgb8()).map_err(|e| format!("No se pudo leer la imagen: {e}"))
}

impl ImgState {
    /// Textura de una imagen plana con los ajustes de la pantalla (para comparar).
    pub fn flat_texture(&mut self, ctx: &egui::Context, img: &PatientImage) -> Option<(egui::TextureId, [usize; 2])> {
        self.texture(ctx, img, true)
    }

    /// Textura de una imagen (con ajustes si `adjusted`), cacheada.
    fn texture(&mut self, ctx: &egui::Context, img: &PatientImage, adjusted: bool) -> Option<(egui::TextureId, [usize; 2])> {
        let a = if adjusted { self.adjust } else { Adjust::default() };
        if let Some((t, s, aa)) = self.cache.get(&(img.id, adjusted))
            && *aa == a
        {
            return Some((t.id(), *s));
        }
        let base = if img.demo {
            demo_image(img)
        } else if file_kind(&img.path) == "img" {
            load_file(&img.path)?
        } else {
            return None;
        };
        let ci = if a == Adjust::default() { base } else { adjust(&base, a) };
        let size = ci.size;
        let tex = ctx.load_texture(format!("img{}_{}", img.id, adjusted), ci, TextureOptions::LINEAR);
        let id = tex.id();
        self.cache.insert((img.id, adjusted), (tex, size, a));
        Some((id, size))
    }
}

// ───────────────────────────── enlaces (lanzar programas) ─────────────────────────────

fn split_args(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut q = false;
    for ch in s.chars() {
        match ch {
            '"' => q = !q,
            ' ' if !q => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn bridge_path(c: &Connector) -> PathBuf {
    if c.bridge_path.trim().is_empty() { crate::paths::data().join(crate::brand::DIR).join("IMCONECT.ini") } else { PathBuf::from(c.bridge_path.trim()) }
}

/// Lanza el programa del enlace para el paciente (para capturar o para ver lo
/// que ya tiene): escribe el fichero puente en su codificación, crea su
/// carpeta de imágenes y ejecuta el programa con los parámetros.
pub fn launch(app: &mut App, cid: u32, pid: u32, action: crate::enlaces::Action) {
    let Some(c) = app.db.connectors.iter().find(|c| c.id == cid).cloned() else { return };
    let dir = app.db.patient_image_dir(pid);
    let _ = std::fs::create_dir_all(&dir);
    let bp = bridge_path(&c);
    let bridge = bp.display().to_string();
    let args = crate::enlaces::args_for(&app.db, &c, pid, action, &bridge);
    let mut msg;
    let mut ok = false;
    if c.bridge_file {
        if let Some(d) = bp.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        msg = match std::fs::write(&bp, crate::enlaces::encode(&crate::enlaces::bridge_text(&app.db, &c, pid, &bridge), c.encoding)) {
            Ok(()) => format!("Fichero puente escrito en {}. ", bp.display()),
            Err(e) => format!("No se pudo escribir el fichero puente: {e}. "),
        };
    } else {
        msg = String::new();
    }
    if c.exe.trim().is_empty() {
        msg.push_str("Indique el programa a ejecutar en «Imágenes y enlaces».");
    } else if !Path::new(c.exe.trim()).exists() {
        msg.push_str(&format!("No se encuentra «{}» en este equipo.", c.exe.trim()));
    } else {
        let exe = PathBuf::from(c.exe.trim());
        let mut cmd = std::process::Command::new(&exe);
        cmd.args(split_args(&args));
        if let Some(d) = exe.parent() {
            cmd.current_dir(d);
        }
        match cmd.spawn() {
            Ok(_) => {
                ok = true;
                msg.push_str(&format!("{} abierto para {}.", c.name, app.db.patient_name(pid)));
            }
            Err(e) => msg.push_str(&format!("Error al abrir {}: {e}", c.name)),
        }
    }
    app.db.launches.push(LaunchLog { when: now(), patient_id: pid, connector: c.name.clone(), command: format!("{} {}", c.exe, args), ok, message: msg.clone() });
    if app.db.launches.len() > 300 {
        app.db.launches.remove(0);
    }
    app.changed();
    if ok { app.notify(msg) } else { app.warn(msg) }
}

pub fn open_folder(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(path).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

/// Las carpetas donde dejan imágenes los aparatos: las de los enlaces activos
/// y la de las imágenes de los pacientes (con una subcarpeta por número de historia).
fn sources(app: &App) -> Vec<crate::entrada::Source> {
    let mut v: Vec<crate::entrada::Source> =
        app.db.connectors.iter().filter(|c| c.enabled && !c.image_folder.trim().is_empty()).map(|c| crate::entrada::Source { name: c.name.clone(), kind: c.kind, base: PathBuf::from(c.image_folder.trim()), auto: c.auto_import }).collect();
    if let Some(base) = app.db.patient_image_dir(0).parent() {
        v.push(crate::entrada::Source { name: "Carpeta del paciente".into(), kind: ImageKind::Periapical, base: base.to_path_buf(), auto: false });
    }
    v
}

fn known_paths(app: &App) -> std::collections::HashSet<String> {
    app.db.images.iter().map(|i| i.path.clone()).filter(|p| !p.is_empty()).collect()
}

/// Pone una imagen encontrada en la galería de un paciente.
pub fn assign(app: &mut App, f: &crate::entrada::Found, pid: u32) {
    let path = f.path.display().to_string();
    app.img.inbox.retain(|x| x.path != f.path);
    if app.db.images.iter().any(|i| i.path == path) {
        return;
    }
    let id = app.db.new_id();
    app.db.images.push(PatientImage { id, patient_id: pid, kind: f.kind, date: f.date.unwrap_or(app.today), teeth: vec![], path, source: f.source.clone(), notes: String::new(), demo: false });
    app.changed();
}

/// Importa a la galería del paciente lo suyo que hayan dejado los aparatos: en
/// su carpeta y en las de los enlaces. Lo que podría ser suyo pero no es seguro
/// queda en «Por asignar», en «Imágenes y enlaces».
pub fn import_folder(app: &mut App, pid: u32) -> usize {
    // al pulsar «Importar» no se espera a que los ficheros lleven un rato sin cambiar
    let found = crate::entrada::scan(&sources(app), &crate::entrada::people(&app.db), &known_paths(app), std::time::SystemTime::now() + crate::entrada::SETTLE);
    let mut added = 0;
    for f in found {
        match f.verdict {
            crate::entrada::Verdict::Sure(p) if p == pid => {
                assign(app, &f, pid);
                added += 1;
            }
            _ if !app.img.inbox.iter().any(|x| x.path == f.path) && !app.img.dismissed.contains(&f.path) => app.img.inbox.push(f),
            _ => {}
        }
    }
    added
}

/// Cada poco, lo nuevo de las carpetas de los aparatos: lo seguro de los
/// enlaces que traen lo nuevo solos entra en su ficha, y lo demás queda en
/// «Por asignar». La búsqueda va en otro hilo (las carpetas pueden ser de red).
pub fn tick(app: &mut App, time: f64) {
    if !app.persist || app.user_id.is_none() || app.screenshot.is_some() {
        return;
    }
    if let Some(rx) = &app.img.scan {
        let Ok(found) = rx.try_recv() else { return };
        app.img.scan = None;
        let mut added = 0;
        for f in found {
            if app.img.dismissed.contains(&f.path) || app.db.images.iter().any(|i| i.path == f.path.display().to_string()) {
                continue;
            }
            match f.verdict {
                crate::entrada::Verdict::Sure(pid) if f.auto => {
                    assign(app, &f, pid);
                    added += 1;
                }
                _ => {
                    app.img.inbox.retain(|x| x.path != f.path);
                    app.img.inbox.push(f);
                }
            }
        }
        if added > 0 {
            app.notify(format!("{added} imágenes nuevas de los aparatos, ya en sus fichas."));
        }
        return;
    }
    if time - app.img.last_scan < 15.0 {
        return;
    }
    app.img.last_scan = time;
    let srcs: Vec<crate::entrada::Source> = sources(app).into_iter().filter(|s| s.auto).collect();
    if srcs.is_empty() {
        return;
    }
    let (people, known) = (crate::entrada::people(&app.db), known_paths(app));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::entrada::scan(&srcs, &people, &known, std::time::SystemTime::now()));
    });
    app.img.scan = Some(rx);
}

/// Las imágenes por asignar: de quién podrían ser y por qué, con los botones
/// para ponerlas en una ficha. `only` = solo las propuestas para ese paciente.
pub fn inbox_card(app: &mut App, ui: &mut Ui, only: Option<u32>) {
    let items: Vec<crate::entrada::Found> = app.img.inbox.iter().filter(|f| only.is_none_or(|p| f.verdict.patient() == Some(p))).cloned().collect();
    if items.is_empty() {
        return;
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, &format!("Imágenes de los aparatos por asignar ({})", items.len()));
        muted(ui, "No se sabe con seguridad de quién son: compruébelo antes de ponerlas en una ficha.");
        ui.add_space(4.0);
        let active = app.patient;
        for f in items.iter().take(30) {
            ui.horizontal_wrapped(|ui| {
                let name = f.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                ui.label(RichText::new(format!("{}  {}", kind_icon(f.kind), name)).family(sb()));
                ui.label(RichText::new(format!("{} · {}", f.kind.short(), f.source)).size(13.0).color(MUTED()));
                if !f.label.is_empty() {
                    ui.label(RichText::new(format!("El fichero dice: {}", f.label)).size(13.0).color(MUTED()));
                }
            });
            let why = match &f.verdict {
                crate::entrada::Verdict::Suggest(_, why) | crate::entrada::Verdict::Unknown(why) => why.clone(),
                crate::entrada::Verdict::Sure(_) => "Es de este paciente; el enlace no trae lo nuevo solo.".into(),
            };
            ui.label(RichText::new(why).size(13.0).color(WARN));
            ui.horizontal_wrapped(|ui| {
                if let Some(p) = f.verdict.patient()
                    && ui.button(format!("Poner en la ficha de {}", app.db.patient_name(p))).clicked()
                {
                    assign(app, f, p);
                }
                if let Some(p) = active.filter(|p| Some(*p) != f.verdict.patient())
                    && ui.button(format!("Poner en la del paciente activo ({})", app.db.patient_name(p))).clicked()
                {
                    assign(app, f, p);
                }
                if ui.button("Descartar").on_hover_text("No se pone en ninguna ficha (el fichero no se borra)").clicked() {
                    app.img.dismissed.insert(f.path.clone());
                    app.img.inbox.retain(|x| x.path != f.path);
                }
            });
            ui.add_space(6.0);
        }
    });
    ui.add_space(10.0);
}

// ───────────────────────────── pestaña del paciente ─────────────────────────────

pub fn patient_tab(app: &mut App, ui: &mut Ui, pid: u32) {
    // barra de enlaces
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            section(ui, "Capturar con");
            let list: Vec<(u32, String, ImageKind)> = app.db.connectors.iter().filter(|c| c.enabled).map(|c| (c.id, c.name.clone(), c.kind)).collect();
            // los que abren distinto para ver las imágenes que ya hay
            let view: Vec<u32> = app.db.connectors.iter().filter(|c| c.enabled && !c.view_args.trim().is_empty()).map(|c| c.id).collect();
            for (id, n, k) in list {
                if ui.add(egui::Button::new(RichText::new(format!("{}  {}", kind_icon(k), n))).corner_radius(CornerRadius::same(18))).on_hover_text("Abre el programa del equipo con los datos de este paciente, para una toma nueva").clicked() {
                    launch(app, id, pid, crate::enlaces::Action::Capture);
                }
                if view.contains(&id) && ui.small_button("Ver").on_hover_text(format!("Abre las imágenes de este paciente en el programa de {n}")).clicked() {
                    launch(app, id, pid, crate::enlaces::Action::View);
                }
            }
            if ui.button("⚙ Configurar enlaces").clicked() {
                app.screen = crate::app::Screen::Imagenes;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("⬇ Importar de la carpeta del paciente").clicked() {
                let n = import_folder(app, pid);
                let waiting = app.img.inbox.iter().filter(|f| f.verdict.patient() == Some(pid)).count();
                match (n, waiting) {
                    (0, 0) => app.notify("No hay imágenes nuevas de este paciente."),
                    (n, 0) => app.notify(format!("{n} imágenes importadas.")),
                    (n, w) => app.notify(format!("{n} imágenes importadas; {w} podrían ser suyas: compruébelas abajo.")),
                }
            }
            if ui.button("📂 Abrir carpeta del paciente").clicked() {
                let d = app.db.patient_image_dir(pid);
                if let Err(e) = open_folder(&d) {
                    app.warn(format!("No se pudo abrir la carpeta: {e}"));
                }
            }
            ui.separator();
            ui.add_sized([280.0, 32.0], egui::TextEdit::singleline(&mut app.img.new_path).hint_text("Ruta de un fichero (png, jpg, bmp, dcm, stl)"));
            crate::theme::combo("img_kind").width(180.0).selected_text(app.img.new_kind.short()).show_ui(ui, |ui| {
                for k in ImageKind::ALL {
                    ui.selectable_value(&mut app.img.new_kind, k, k.label());
                }
            });
            ui.add_sized([90.0, 32.0], egui::TextEdit::singleline(&mut app.img.new_teeth).hint_text("Piezas"));
            if primary(ui, "Añadir").clicked() {
                let path = app.img.new_path.trim().trim_matches('"').to_string();
                if path.is_empty() || !Path::new(&path).exists() {
                    app.warn("No se encuentra ese fichero.");
                } else {
                    let teeth: Vec<u8> = app.img.new_teeth.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).filter(|t| crate::screens::pacientes::valid_tooth(*t)).collect();
                    let id = app.db.new_id();
                    app.db.images.push(PatientImage { id, patient_id: pid, kind: app.img.new_kind, date: app.today, teeth, path, source: "Importada".into(), notes: String::new(), demo: false });
                    app.img.selected = Some(id);
                    app.img.new_path.clear();
                    app.img.new_teeth.clear();
                    app.changed();
                    app.notify("Imagen añadida a la galería.");
                }
            }
        });
    });
    ui.add_space(10.0);
    // lo que ha llegado de los aparatos y podría ser de este paciente
    inbox_card(app, ui, Some(pid));
    let mut list: Vec<PatientImage> = app.db.images.iter().filter(|i| i.patient_id == pid && app.img.filter.map(|f| i.kind == f).unwrap_or(true)).cloned().collect();
    list.sort_by_key(|i| std::cmp::Reverse((i.date, i.id)));
    if app.img.selected.map(|s| !list.iter().any(|i| i.id == s)).unwrap_or(true) {
        app.img.selected = list.first().map(|i| i.id);
    }
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    let gal_w = 330.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(gal_w);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(app.img.filter.is_none(), "Todas").clicked() {
                        app.img.filter = None;
                    }
                    for k in [ImageKind::Periapical, ImageKind::Panoramica, ImageKind::Tac, ImageKind::FotoIntraoral] {
                        if ui.selectable_label(app.img.filter == Some(k), k.short()).clicked() {
                            app.img.filter = Some(k);
                        }
                    }
                });
                ui.add_space(6.0);
                if list.is_empty() {
                    empty_state(ui, "Sin imágenes. Capture con un equipo o importe ficheros.");
                }
                let ctx = ui.ctx().clone();
                egui::ScrollArea::vertical().id_salt("gal").max_height(560.0).show(ui, |ui| {
                    for img in &list {
                        let sel = app.img.selected == Some(img.id);
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 86.0), Sense::click());
                        let p = ui.painter().clone();
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
                        let th = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(100.0, 70.0));
                        p.rect_filled(th, CornerRadius::same(6), Color32::BLACK);
                        if let Some((tid, size)) = app.img.texture(&ctx, img, false) {
                            let k = (th.width() / size[0] as f32).min(th.height() / size[1] as f32);
                            let r = Rect::from_center_size(th.center(), vec2(size[0] as f32 * k, size[1] as f32 * k));
                            p.image(tid, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                        } else {
                            p.text(th.center(), Align2::CENTER_CENTER, if file_kind(&img.path) == "3d" { "3D" } else { "DICOM" }, sbf(14.0), Color32::from_rgb(140, 200, 255));
                        }
                        let teeth: Vec<String> = img.teeth.iter().map(|t| t.to_string()).collect();
                        p.text(rect.min + vec2(118.0, 12.0), Align2::LEFT_TOP, img.kind.short(), sbf(14.5), TEXT);
                        p.text(rect.min + vec2(118.0, 34.0), Align2::LEFT_TOP, format!("{}{}", fmt_date(img.date), if teeth.is_empty() { String::new() } else { format!(" · pieza {}", teeth.join("-")) }), FontId::proportional(13.0), MUTED());
                        p.with_clip_rect(rect).text(rect.min + vec2(118.0, 54.0), Align2::LEFT_TOP, &img.source, FontId::proportional(12.5), ACCENT);
                        if resp.clicked() {
                            app.img.selected = Some(img.id);
                            app.img.zoom = 1.0;
                            app.img.pan = egui::Vec2::ZERO;
                            app.img.adjust = Adjust { invert: false, ..Adjust::default() };
                        }
                        ui.add_space(4.0);
                    }
                });
            });
        });
        ui.vertical(|ui| {
            ui.set_width(avail - gal_w - 48.0);
            if let Some(id) = app.img.selected {
                viewer(app, ui, id);
            }
        });
    });
}

fn kind_icon(k: ImageKind) -> &'static str {
    match k {
        ImageKind::Tac => "🧊",
        ImageKind::FotoIntraoral | ImageKind::FotoExtraoral => "📷",
        ImageKind::Escaneo3D => "🦷",
        _ => "☢",
    }
}

fn viewer(app: &mut App, ui: &mut Ui, id: u32) {
    let Some(img) = app.db.images.iter().find(|i| i.id == id).cloned() else { return };
    if let Some(other) = app.img.compare.filter(|o| *o != id && app.db.images.iter().any(|i| i.id == *o && i.patient_id == img.patient_id)) {
        crate::screens::visor3d::compare(app, ui, id, other);
        return;
    }
    let in_viewer = crate::screens::visor3d::handles(&img);
    // controles
    let vw = ui.available_width();
    card(ui, |ui| {
        ui.set_width(vw - 40.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(img.kind.label()).family(sb()).size(17.0));
            pill(ui, &fmt_date(img.date), INFO);
            right_aligned(ui, |ui| {
                if ui.button("Restablecer").clicked() {
                    app.img.adjust = Adjust::default();
                    app.img.zoom = 1.0;
                    app.img.pan = egui::Vec2::ZERO;
                }
                if ui.button("↻ Girar").clicked() {
                    app.img.adjust.rot = (app.img.adjust.rot + 1) % 4;
                }
                ui.toggle_value(&mut app.img.adjust.invert, "Negativo");
                // comparar con otra imagen plana del mismo paciente
                let others: Vec<(u32, String)> =
                    app.db.images.iter().filter(|o| o.patient_id == img.patient_id && o.id != img.id && (o.demo || file_kind(&o.path) == "img")).map(|o| (o.id, format!("{} · {}", fmt_date(o.date), o.kind.label()))).collect();
                if !in_viewer && !others.is_empty() {
                    crate::theme::combo("img_compare").width(220.0).selected_text("Comparar con…").show_ui(ui, |ui| {
                        for (oid, label) in others {
                            if ui.selectable_label(false, label).clicked() {
                                app.img.compare = Some(oid);
                            }
                        }
                    });
                }
            });
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = ((vw - 330.0) / 3.0).clamp(60.0, 160.0);
            ui.label("Zoom");
            ui.add(egui::Slider::new(&mut app.img.zoom, 0.5..=4.0).show_value(false));
            ui.label("Brillo");
            ui.add(egui::Slider::new(&mut app.img.adjust.brightness, -60..=60).show_value(false));
            ui.label("Contraste");
            ui.add(egui::Slider::new(&mut app.img.adjust.contrast, -60..=150).show_value(false));
        });
    });
    ui.add_space(8.0);
    // lienzo
    let h = (ui.available_height() - 150.0).clamp(320.0, 620.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(vw, h), Sense::click_and_drag());
    let p = ui.painter().with_clip_rect(rect);
    p.rect_filled(rect, CornerRadius::same(14), Color32::BLACK);
    if resp.dragged() {
        app.img.pan += resp.drag_delta();
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            app.img.zoom = (app.img.zoom * (1.0 + scroll * 0.002)).clamp(0.5, 6.0);
        }
    }
    if resp.double_clicked() {
        app.img.zoom = 1.0;
        app.img.pan = egui::Vec2::ZERO;
    }
    let ctx = ui.ctx().clone();
    if in_viewer {
        let name = app.db.patient_name(img.patient_id);
        crate::screens::visor3d::show(&mut app.img.v3d, ui, &img, rect, &name);
        ui.add_space(6.0);
        crate::screens::visor3d::controls(&mut app.img.v3d, ui);
    } else {
        match app.img.texture(&ctx, &img, true) {
            Some((tid, size)) => {
                let k = (rect.width() / size[0] as f32).min(rect.height() / size[1] as f32) * 0.96 * app.img.zoom;
                let r = Rect::from_center_size(rect.center() + app.img.pan, vec2(size[0] as f32 * k, size[1] as f32 * k));
                p.image(tid, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                if img.demo {
                    p.text(rect.left_bottom() + vec2(12.0, -12.0), Align2::LEFT_BOTTOM, "Imagen de demostración", FontId::proportional(12.0), Color32::from_rgb(150, 160, 175));
                }
            }
            None => {
                let txt = match file_kind(&img.path) {
                    "3d" => "Modelo 3D (STL): ábralo con el programa del escáner",
                    _ => "Estudio DICOM / TAC: ábralo con el visor 3D",
                };
                p.text(rect.center() - vec2(0.0, 16.0), Align2::CENTER_CENTER, txt, sbf(16.0), Color32::from_rgb(170, 200, 240));
                if !img.path.is_empty() && !Path::new(&img.path).exists() {
                    p.text(rect.center() + vec2(0.0, 14.0), Align2::CENTER_CENTER, "(el fichero ya no existe en esta ruta)", FontId::proportional(13.0), Color32::from_rgb(230, 120, 120));
                }
            }
        }
    }
    p.rect_stroke(rect, CornerRadius::same(14), Stroke::new(1.0, BORDER()), StrokeKind::Inside);
    ui.add_space(8.0);
    // datos
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let Some(m) = app.db.images.iter_mut().find(|i| i.id == id) else { return };
        let before = serde_json::to_string(m).unwrap_or_default();
        let mut open_with = None;
        let mut delete = false;
        ui.horizontal_wrapped(|ui| {
            crate::theme::combo("img_kind_edit").width(200.0).selected_text(m.kind.label()).show_ui(ui, |ui| {
                for k in ImageKind::ALL {
                    ui.selectable_value(&mut m.kind, k, k.label());
                }
            });
            let mut t: String = m.teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(" ");
            ui.label(RichText::new("Piezas").color(MUTED()));
            if ui.add_sized([110.0, 32.0], egui::TextEdit::singleline(&mut t)).changed() {
                m.teeth = t.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).filter(|t| crate::screens::pacientes::valid_tooth(*t)).collect();
            }
            ui.label(RichText::new("Notas").color(MUTED()));
            ui.add_sized([260.0, 32.0], egui::TextEdit::singleline(&mut m.notes));
            muted(ui, format!("Origen: {}", m.source));
            right_aligned(ui, |ui| {
                if danger(ui, "Quitar").clicked() {
                    delete = true;
                }
                if (matches!(m.kind, ImageKind::Tac | ImageKind::Escaneo3D) || file_kind(&m.path) != "img") && ui.button("Abrir en visor 3D").clicked() {
                    open_with = Some(m.kind);
                }
            });
        });
        let changed = serde_json::to_string(m).unwrap_or_default() != before;
        if changed {
            app.changed();
        }
        if delete {
            app.db.images.retain(|i| i.id != id);
            app.img.selected = None;
            app.changed();
            app.notify("Imagen quitada de la galería (el fichero original no se borra).");
        }
        if let Some(k) = open_with {
            let cid = app.db.connectors.iter().find(|c| c.enabled && c.kind == k).or_else(|| app.db.connectors.iter().find(|c| c.kind == ImageKind::Tac)).map(|c| c.id);
            if let Some(cid) = cid {
                launch(app, cid, img.patient_id, crate::enlaces::Action::View);
            }
        }
    });
    // análisis con inteligencia artificial
    if let Some(cur) = app.db.images.iter().find(|i| i.id == id).cloned() {
        ui.add_space(8.0);
        crate::screens::ia::image_panel(app, ui, &cur);
    }
}

// ───────────────────────────── pantalla de enlaces ─────────────────────────────

pub fn screen(app: &mut App, ui: &mut Ui) {
    title(ui, "Imágenes y enlaces", "Conexión con los equipos de radiología, TAC, cámaras y escáneres");
    let w = kpi_w(ui, 4);
    let total = app.db.images.len();
    let xr = app.db.images.iter().filter(|i| i.kind.is_xray()).count();
    let tac = app.db.images.iter().filter(|i| i.kind == ImageKind::Tac).count();
    let active = app.db.connectors.iter().filter(|c| c.enabled).count();
    ui.horizontal(|ui| {
        kpi(ui, w, "Imágenes en galerías", &total.to_string(), "todos los pacientes", INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Radiografías", &xr.to_string(), "periapicales, panorámicas…", ACCENT);
        ui.add_space(4.0);
        kpi(ui, w, "Estudios TAC / CBCT", &tac.to_string(), "volumétricos 3D", WARN);
        ui.add_space(4.0);
        kpi(ui, w, "Enlaces activos", &active.to_string(), &format!("de {} configurados", app.db.connectors.len()), OK);
    });
    ui.add_space(10.0);
    inbox_card(app, ui, None);
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail * 0.56);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    section(ui, "Equipos de imagen");
                    right_aligned(ui, |ui| {
                        if ui.button("＋ Nuevo enlace").clicked() {
                            let id = app.db.new_id();
                            app.db.connectors.push(Connector {
                                id,
                                name: "Nuevo enlace".into(),
                                vendor: String::new(),
                                kind: ImageKind::Periapical,
                                exe: String::new(),
                                args: "[NUMPAC]".into(),
                                view_args: String::new(),
                                bridge_file: true,
                                bridge_path: String::new(),
                                bridge_template: String::new(),
                                encoding: Default::default(),
                                image_folder: String::new(),
                                auto_import: false,
                                enabled: true,
                                notes: String::new(),
                            });
                            app.img.edit_connector = Some(id);
                            app.changed();
                        }
                    });
                });
                muted(ui, "Al abrir un equipo se escribe el fichero IMCONECT.ini con los datos del paciente (mismo formato que el programa original) y se lanza el programa con sus parámetros.");
                ui.add_space(6.0);
                let list = app.db.connectors.clone();
                for c in &list {
                    let sel = app.img.edit_connector == Some(c.id);
                    let found = !c.exe.trim().is_empty() && Path::new(c.exe.trim()).exists();
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
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
                    p.text(rect.min + vec2(14.0, rect.height() / 2.0), Align2::LEFT_CENTER, kind_icon(c.kind), FontId::proportional(22.0), if c.enabled { ACCENT } else { MUTED() });
                    p.text(rect.min + vec2(50.0, 10.0), Align2::LEFT_TOP, &c.name, sbf(15.0), if c.enabled { TEXT } else { MUTED() });
                    p.text(rect.min + vec2(50.0, 32.0), Align2::LEFT_TOP, format!("{} · {}", c.vendor, c.kind.short()), FontId::proportional(13.0), MUTED());
                    let (st, col) = if !c.enabled {
                        ("Desactivado", MUTED())
                    } else if found {
                        ("Programa encontrado", OK)
                    } else {
                        ("No instalado en este equipo", WARN)
                    };
                    p.text(rect.right_center() - vec2(14.0, 0.0), Align2::RIGHT_CENTER, st, sbf(13.0), col);
                    if resp.clicked() {
                        app.img.edit_connector = Some(c.id);
                    }
                    ui.add_space(4.0);
                }
            });
        });
        ui.vertical(|ui| {
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                let pid = app.patient;
                let Some(cid) = app.img.edit_connector else {
                    muted(ui, "Seleccione un equipo para configurarlo.");
                    return;
                };
                let mut test = false;
                let mut remove = false;
                if let Some(c) = app.db.connectors.iter_mut().find(|c| c.id == cid) {
                    let before = serde_json::to_string(c).unwrap_or_default();
                    section(ui, &c.name.clone());
                    ui.checkbox(&mut c.enabled, "Activo (aparece en la ficha del paciente)");
                    field(ui, "Nombre", &mut c.name, 300.0);
                    field(ui, "Fabricante / programa", &mut c.vendor, 300.0);
                    ui.label(RichText::new("Tipo de imagen").size(13.5).color(MUTED()));
                    crate::theme::combo("con_kind").width(300.0).selected_text(c.kind.label()).show_ui(ui, |ui| {
                        for k in ImageKind::ALL {
                            ui.selectable_value(&mut c.kind, k, k.label());
                        }
                    });
                    field(ui, "Programa (ruta del ejecutable)", &mut c.exe, 330.0);
                    field(ui, "Parámetros para capturar", &mut c.args, 330.0);
                    field(ui, "Parámetros para ver sus imágenes (vacío: los de capturar)", &mut c.view_args, 330.0);
                    muted(ui, crate::enlaces::WILDCARDS);
                    ui.checkbox(&mut c.bridge_file, "Escribir un fichero puente antes de abrirlo");
                    if c.bridge_file {
                        field(ui, "Ruta del fichero puente (vacío = IMCONECT.ini en la carpeta de datos)", &mut c.bridge_path, 330.0);
                        ui.label(RichText::new("Codificación del fichero puente").size(13.5).color(MUTED()));
                        crate::theme::combo("con_enc").width(300.0).selected_text(c.encoding.label()).show_ui(ui, |ui| {
                            for e in crate::enlaces::Encoding::ALL {
                                ui.selectable_value(&mut c.encoding, e, e.label());
                            }
                        });
                        ui.label(RichText::new("Contenido del fichero puente (vacío = IMCONECT.ini)").size(13.5).color(MUTED()));
                        ui.add_sized([330.0, 70.0], egui::TextEdit::multiline(&mut c.bridge_template).font(egui::TextStyle::Monospace).hint_text("PN=[NUMPAC]\nLN=[APELPAC]…"));
                    }
                    field(ui, "Carpeta de exportación de imágenes (para importar)", &mut c.image_folder, 330.0);
                    if !c.image_folder.trim().is_empty() {
                        ui.checkbox(&mut c.auto_import, "Traer lo nuevo de esa carpeta sin pulsar «Importar»");
                    }
                    if !c.notes.is_empty() {
                        muted(ui, c.notes.clone());
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if primary(ui, "Probar con el paciente activo").clicked() {
                            test = true;
                        }
                        if ui.button("Quitar").clicked() {
                            remove = true;
                        }
                    });
                    if serde_json::to_string(c).unwrap_or_default() != before {
                        app.dirty = true;
                    }
                }
                if test {
                    match pid {
                        Some(p) => launch(app, cid, p, crate::enlaces::Action::Capture),
                        None => app.warn("Seleccione antes un paciente."),
                    }
                }
                if remove {
                    app.db.connectors.retain(|c| c.id != cid);
                    app.img.edit_connector = None;
                    app.changed();
                }
                if let Some(p) = pid
                    && let Some(c) = app.db.connectors.iter().find(|c| c.id == cid)
                {
                    let bp = bridge_path(c).display().to_string();
                    ui.add_space(6.0);
                    ui.collapsing("Ver lo que se enviará con el paciente activo", |ui| {
                        ui.label(RichText::new(format!("{} {}", c.exe.trim(), crate::enlaces::args_for(&app.db, c, p, crate::enlaces::Action::Capture, &bp))).monospace().size(12.5));
                        if c.bridge_file {
                            ui.label(RichText::new(crate::enlaces::bridge_text(&app.db, c, p, &bp).replace('\r', "")).monospace().size(12.5));
                        }
                    });
                }
            });
        });
    });
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Registro de aperturas");
        let tbl = Table::new("launch", vec![("Fecha", 140.0), ("Paciente", 200.0), ("Equipo", 180.0), ("Resultado", 420.0)]);
        tbl.header(ui);
        let mut logs = app.db.launches.clone();
        logs.reverse();
        if logs.is_empty() {
            empty_state(ui, "Todavía no se ha abierto ningún equipo.");
        }
        let mut open = None;
        for (i, l) in logs.iter().take(20).enumerate() {
            if tbl.row(ui, i, false, &[cell(l.when.format("%d/%m/%Y %H:%M").to_string()), cell(app.db.patient_name(l.patient_id)), cell(&l.connector), cellc(&l.message, if l.ok { OK } else { WARN })]).clicked() {
                open = Some(l.patient_id);
            }
        }
        if let Some(p) = open {
            app.open_patient(p, PatTab::Imagenes);
        }
    });
}
