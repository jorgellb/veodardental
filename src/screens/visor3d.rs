//! Visor de estudios DICOM/CBCT y modelos STL dentro del programa, y
//! comparación de dos radiografías (T-F5-06). Solo se ve: no hay
//! diagnóstico automático.
//!
//! - La carga y la vista del STL van en segundo plano: la pantalla no se queda
//!   parada con un estudio grande.
//! - **Escala:** una barra de 10 mm.
//! - **Orientación:** letras de los bordes en el corte axial, y el paciente y
//!   la fecha que dice el propio fichero, para comprobar que es el estudio de
//!   este paciente.
//! - **Comparación:** dos imágenes del mismo paciente, la más antigua a la
//!   izquierda, con su fecha, sus piezas, los días entre una y otra, y el mismo
//!   zoom y el mismo giro en las dos.

use crate::dicom::{Dicom, View};
use crate::model::*;
use crate::stl::Mesh;
use crate::theme::*;
use egui::{Align2, Color32, ColorImage, CornerRadius, FontId, Rect, RichText, TextureHandle, TextureOptions, Ui, pos2, vec2};
use std::sync::Arc;
use std::sync::mpsc::Receiver;

pub enum Loaded {
    Dicom(Arc<Dicom>),
    Mesh(Arc<Mesh>),
}

/// Vista que se ha pedido: (imagen, corte o ángulos, ventana).
type Key = (u32, u8, usize, i64, i64);
/// La vista hecha: su clave, la textura, su tamaño y los mm de un píxel.
type Tex = (Key, TextureHandle, [usize; 2], (f64, f64));

pub struct V3d {
    for_image: Option<u32>,
    job: Option<Receiver<Result<Loaded, String>>>,
    data: Option<Loaded>,
    pub error: Option<String>,
    pub view: View,
    pub index: usize,
    pub window: (f64, f64),
    pub yaw: f32,
    pub pitch: f32,
    tex: Option<Tex>,
    render: Option<(Key, Receiver<(ColorImage, f32)>)>,
}

impl Default for V3d {
    fn default() -> Self {
        V3d { for_image: None, job: None, data: None, error: None, view: View::Axial, index: 0, window: (0.0, 1.0), yaw: 30.0, pitch: -20.0, tex: None, render: None }
    }
}

fn is_3d(path: &str) -> Option<bool> {
    let l = path.to_lowercase();
    if l.ends_with(".stl") {
        Some(true)
    } else if l.ends_with(".dcm") || l.ends_with(".dicom") || std::path::Path::new(path).is_dir() {
        Some(false)
    } else {
        None
    }
}

/// ¿Se ve con este visor?
pub fn handles(img: &PatientImage) -> bool {
    !img.demo && is_3d(&img.path).is_some()
}

impl V3d {
    /// ¿Está cargando?
    pub fn loading(&self) -> bool {
        self.job.is_some()
    }

    fn ensure(&mut self, img: &PatientImage) {
        if self.for_image == Some(img.id) {
            return;
        }
        *self = V3d { for_image: Some(img.id), ..Default::default() };
        let path = std::path::PathBuf::from(&img.path);
        let mesh = is_3d(&img.path) == Some(true);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let out = if mesh { crate::stl::read(&path).map(|m| Loaded::Mesh(Arc::new(m))) } else { crate::dicom::read_series(&path).map(|d| Loaded::Dicom(Arc::new(d))) };
            let _ = tx.send(out);
        });
        self.job = Some(rx);
    }

    fn poll(&mut self) {
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(Ok(d)) => {
                    if let Loaded::Dicom(x) = &d {
                        self.window = x.default_window();
                        self.index = x.slices(View::Axial) / 2;
                    }
                    self.data = Some(d);
                    self.job = None;
                }
                Ok(Err(e)) => {
                    self.error = Some(e);
                    self.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => {
                    self.error = Some("La carga se interrumpió.".into());
                    self.job = None;
                }
            }
        }
    }
}

/// El lienzo del visor (en `rect`) y sus controles debajo.
pub fn show(st: &mut V3d, ui: &mut Ui, img: &PatientImage, rect: Rect, patient_name: &str) {
    st.ensure(img);
    st.poll();
    let ctx = ui.ctx().clone();
    let p = ui.painter().with_clip_rect(rect);
    p.rect_filled(rect, CornerRadius::same(14), Color32::BLACK);
    let light = Color32::from_rgb(170, 200, 240);
    if let Some(e) = &st.error {
        p.text(rect.center(), Align2::CENTER_CENTER, e, sbf(15.0), Color32::from_rgb(240, 140, 140));
        return;
    }
    if st.loading() {
        p.text(rect.center(), Align2::CENTER_CENTER, "Cargando el estudio…", sbf(16.0), light);
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        return;
    }
    let id = img.id;
    let (key, frame, mm) = match &st.data {
        Some(Loaded::Dicom(d)) => {
            let view_n = st.view as u8;
            let key: Key = (id, view_n, st.index, st.window.0 as i64, st.window.1 as i64);
            if st.tex.as_ref().map(|t| t.0) != Some(key) {
                let s = d.slice(st.view, st.index, st.window);
                let ci = ColorImage::from_gray([s.w, s.h], &s.gray);
                let size = ci.size;
                st.tex = Some((key, ctx.load_texture(format!("v3d{id}"), ci, TextureOptions::LINEAR), size, s.mm));
            }
            let t = st.tex.as_ref().unwrap();
            (key, Some((t.1.id(), t.2)), t.3)
        }
        Some(Loaded::Mesh(m)) => {
            let key: Key = (id, 9, 0, st.yaw as i64, st.pitch as i64);
            // la vista nueva se calcula en segundo plano; mientras, la anterior
            if let Some((k, rx)) = &st.render
                && let Ok((ci, mm)) = rx.try_recv()
            {
                let size = ci.size;
                st.tex = Some((*k, ctx.load_texture(format!("v3d{id}"), ci, TextureOptions::LINEAR), size, (mm as f64, mm as f64)));
                st.render = None;
            }
            if st.render.is_none() && st.tex.as_ref().map(|t| t.0) != Some(key) {
                let (m2, yaw, pitch) = (m.clone(), st.yaw, st.pitch);
                let (tx, rx) = std::sync::mpsc::channel();
                let c2 = ctx.clone();
                std::thread::spawn(move || {
                    let (px, mm) = crate::stl::render(&m2, 512, yaw, pitch);
                    let _ = tx.send((ColorImage::from_rgba_unmultiplied([512, 512], &px), mm));
                    c2.request_repaint();
                });
                st.render = Some((key, rx));
            }
            match &st.tex {
                Some(t) => (key, Some((t.1.id(), t.2)), t.3),
                None => {
                    p.text(rect.center(), Align2::CENTER_CENTER, "Preparando la vista…", sbf(16.0), light);
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    return;
                }
            }
        }
        None => return,
    };
    let _ = key;
    let Some((tid, size)) = frame else { return };
    // proporción real: mm de un píxel en horizontal y en vertical
    let (wmm, hmm) = (size[0] as f64 * mm.0, size[1] as f64 * mm.1);
    let k = ((rect.width() as f64 * 0.94) / wmm).min((rect.height() as f64 * 0.88) / hmm) as f32;
    let r = Rect::from_center_size(rect.center(), vec2(wmm as f32 * k, hmm as f32 * k));
    p.image(tid, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    // barra de 10 mm
    let bar = 10.0 * k;
    let y = rect.bottom() - 18.0;
    p.line_segment([pos2(rect.left() + 16.0, y), pos2(rect.left() + 16.0 + bar, y)], egui::Stroke::new(3.0, Color32::WHITE));
    p.text(pos2(rect.left() + 16.0, y - 6.0), Align2::LEFT_BOTTOM, "10 mm", FontId::proportional(12.0), Color32::WHITE);
    if let Some(Loaded::Dicom(d)) = &st.data {
        let h = &d.header;
        if st.view == View::Axial {
            let [l, rr, t, b] = d.edges();
            let f = FontId::proportional(15.0);
            p.text(pos2(r.left() - 4.0, r.center().y), Align2::RIGHT_CENTER, l, f.clone(), light);
            p.text(pos2(r.right() + 4.0, r.center().y), Align2::LEFT_CENTER, rr, f.clone(), light);
            p.text(pos2(r.center().x, r.top() - 2.0), Align2::CENTER_BOTTOM, t, f.clone(), light);
            p.text(pos2(r.center().x, r.bottom() + 2.0), Align2::CENTER_TOP, b, f, light);
        }
        let who = if h.patient.is_empty() { "sin nombre en el fichero".to_string() } else { h.patient.clone() };
        let warn = !h.patient.is_empty() && !same_person(&h.patient, patient_name);
        p.text(rect.left_top() + vec2(12.0, 10.0), Align2::LEFT_TOP, format!("Fichero: {who} · {} · {}", h.date, h.modality), FontId::proportional(12.5), if warn { Color32::from_rgb(250, 160, 90) } else { light });
        if warn {
            p.text(rect.left_top() + vec2(12.0, 28.0), Align2::LEFT_TOP, format!("¡Atención! El nombre del fichero no coincide con el de la ficha ({patient_name})."), FontId::proportional(12.5), Color32::from_rgb(250, 160, 90));
        }
    }
}

/// ¿El nombre del fichero DICOM es el del paciente? (sin acentos ni orden)
pub fn same_person(dicom: &str, ficha: &str) -> bool {
    let words = |s: &str| -> Vec<String> {
        let mut v: Vec<String> = fold(s).split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 1).map(String::from).collect();
        v.sort();
        v
    };
    let (a, b) = (words(dicom), words(ficha));
    !a.is_empty() && a.iter().all(|w| b.contains(w))
}

/// Controles del visor (debajo del lienzo).
pub fn controls(st: &mut V3d, ui: &mut Ui) {
    match &st.data {
        Some(Loaded::Dicom(d)) => {
            let d = d.clone();
            ui.horizontal_wrapped(|ui| {
                for (v, label) in [(View::Axial, "Axial"), (View::Coronal, "Coronal"), (View::Sagital, "Sagital")] {
                    if (d.header.frames > 1 || v == View::Axial) && ui.selectable_label(st.view == v, label).clicked() && st.view != v {
                        st.view = v;
                        st.index = d.slices(v) / 2;
                    }
                }
                let n = d.slices(st.view);
                if n > 1 {
                    ui.label("Corte");
                    let mut i = st.index as u32 + 1;
                    if ui.add(egui::Slider::new(&mut i, 1..=n as u32)).changed() {
                        st.index = i as usize - 1;
                    }
                }
                ui.label("Centro");
                ui.add(egui::DragValue::new(&mut st.window.0).speed(5.0));
                ui.label("Anchura");
                ui.add(egui::DragValue::new(&mut st.window.1).range(1.0..=100_000.0).speed(5.0));
                if ui.small_button("Ventana del fichero").clicked() {
                    st.window = d.default_window();
                }
            });
            let h = &d.header;
            muted(
                ui,
                format!(
                    "{}×{} píxeles{}, {} · píxel de {}",
                    h.cols,
                    h.rows,
                    if h.frames > 1 { format!(" × {} cortes", h.frames) } else { String::new() },
                    h.photometric,
                    h.spacing.map(|s| format!("{:.3}×{:.3} mm", s.1, s.0)).unwrap_or_else(|| "tamaño desconocido (escala aproximada)".into())
                ),
            );
        }
        Some(Loaded::Mesh(m)) => {
            let s = m.size();
            ui.horizontal_wrapped(|ui| {
                ui.label("Girar");
                ui.add(egui::Slider::new(&mut st.yaw, -180.0..=180.0).suffix("°"));
                ui.label("Inclinar");
                ui.add(egui::Slider::new(&mut st.pitch, -90.0..=90.0).suffix("°"));
                if ui.small_button("De frente").clicked() {
                    (st.yaw, st.pitch) = (0.0, 0.0);
                }
                if ui.small_button("Oclusal").clicked() {
                    (st.yaw, st.pitch) = (0.0, -90.0);
                }
            });
            muted(ui, format!("{} triángulos · {:.1} × {:.1} × {:.1} mm", m.tris.len(), s[0], s[1], s[2]));
        }
        None => {}
    }
}

/// Comparación de dos imágenes planas del mismo paciente.
pub fn compare(app: &mut crate::app::App, ui: &mut Ui, a: u32, b: u32) {
    let (Some(x), Some(y)) = (app.db.images.iter().find(|i| i.id == a).cloned(), app.db.images.iter().find(|i| i.id == b).cloned()) else { return };
    if x.patient_id != y.patient_id {
        ui.label(RichText::new("Solo se comparan imágenes del mismo paciente.").color(DANGER));
        return;
    }
    let (old, new) = if (x.date, x.id) <= (y.date, y.id) { (x, y) } else { (y, x) };
    let name = app.db.patient_name(old.patient_id);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            section(ui, &format!("Comparación · {name}"));
            pill(ui, &format!("{} días entre una y otra", (new.date - old.date).num_days()), INFO);
            right_aligned(ui, |ui| {
                if ui.button("Cerrar la comparación").clicked() {
                    app.img.compare = None;
                }
                if ui.button("↻ Girar las dos").clicked() {
                    app.img.adjust.rot = (app.img.adjust.rot + 1) % 4;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("Zoom (el mismo en las dos)");
            ui.add(egui::Slider::new(&mut app.img.zoom, 0.5..=4.0).show_value(false));
        });
    });
    let w = (ui.available_width() - 12.0) / 2.0;
    let h = (ui.available_height() - 40.0).clamp(300.0, 560.0);
    ui.horizontal_top(|ui| {
        for (label, img) in [("Antes", &old), ("Después", &new)] {
            ui.vertical(|ui| {
                ui.set_width(w);
                let teeth = if img.teeth.is_empty() { String::new() } else { format!(" · piezas {}", img.teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join("-")) };
                ui.label(RichText::new(format!("{label}: {} · {}{teeth}", fmt_date(img.date), img.kind.label())).font(sbf(13.5)));
                let (rect, _) = ui.allocate_exact_size(vec2(w, h), egui::Sense::hover());
                let p = ui.painter().with_clip_rect(rect);
                p.rect_filled(rect, CornerRadius::same(12), Color32::BLACK);
                let ctx = ui.ctx().clone();
                match app.img.flat_texture(&ctx, img) {
                    Some((tid, size)) => {
                        let k = (rect.width() / size[0] as f32).min(rect.height() / size[1] as f32) * 0.96 * app.img.zoom;
                        let r = Rect::from_center_size(rect.center() + app.img.pan, vec2(size[0] as f32 * k, size[1] as f32 * k));
                        p.image(tid, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        p.text(rect.center(), Align2::CENTER_CENTER, "Esta imagen no se puede mostrar aquí", sbf(14.0), Color32::from_rgb(170, 200, 240));
                    }
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El nombre del fichero DICOM se compara con el de la ficha sin acentos,
    /// mayúsculas ni orden; si no coincide, el visor avisa.
    #[test]
    fn the_name_in_the_file_is_checked_against_the_patient() {
        assert!(same_person("GARCIA LOPEZ ANA", "Ana García López"));
        assert!(same_person("Garcia Ana", "Ana García López"));
        assert!(!same_person("MARTINEZ PEDRO", "Ana García López"));
        assert!(!same_person("", "Ana García López"));
    }
}
