//! Logotipo de VeodarDental como imagen: icono de la ventana y de los
//! instaladores (`veodardental --icono carpeta` escribe los PNG que usan).
//! Es el mismo dibujo que el de la barra lateral: diente blanco sobre un
//! cuadrado redondeado verde azulado.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Nombre del programa.
pub const NAME: &str = "VeodarDental";
/// Carpeta del programa dentro de las de datos, configuración y Documentos.
pub const DIR: &str = "VeodarDental";
/// Nombre que tenía antes el programa: sus carpetas se pasan al nombre nuevo.
pub const OLD_DIR: &str = "DentaSuite";

static MIGRATED: OnceLock<usize> = OnceLock::new();

/// Carpetas donde el programa guarda cosas: datos de la clínica, ajustes de
/// este ordenador y documentos (en Windows, las dos primeras son la misma).
fn bases() -> Vec<PathBuf> {
    vec![crate::paths::data(), crate::paths::config(), crate::paths::documents()]
}

/// Al abrir el programa con el nombre nuevo por primera vez, pasa las
/// carpetas del nombre antiguo (datos de la clínica y sus copias, imágenes,
/// puesto, certificados, contraseña de las copias, WhatsApp, recetas,
/// documentos…) al nombre nuevo, para que la clínica lo encuentre todo.
pub fn migrate_dirs() -> usize {
    let n = if cfg!(test) { 0 } else { migrate_in(&bases()).len() };
    let _ = MIGRATED.set(n);
    n
}

/// Cuántas carpetas se pasaron al nombre nuevo al abrir el programa.
pub fn migrated() -> usize {
    MIGRATED.get().copied().unwrap_or(0)
}

fn copy_all(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_all(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), &target)?;
        }
    }
    Ok(())
}

/// Pasa `base/DentaSuite` a `base/VeodarDental` en cada carpeta base, solo si
/// la nueva aún no existe (nunca mezcla ni pisa nada). Si no se puede mover
/// (por ejemplo, el programa antiguo está abierto en Windows), lo copia
/// entero y deja la antigua como estaba. Devuelve (antigua, nueva) de lo pasado.
pub fn migrate_in(bases: &[PathBuf]) -> Vec<(PathBuf, PathBuf)> {
    let mut done = vec![];
    let mut seen: Vec<PathBuf> = vec![];
    for b in bases {
        if seen.contains(b) {
            continue;
        }
        seen.push(b.clone());
        let (old, new) = (b.join(OLD_DIR), b.join(DIR));
        if !old.is_dir() || new.exists() {
            continue;
        }
        if std::fs::rename(&old, &new).is_ok() {
            done.push((old, new));
            continue;
        }
        // copia completa a una carpeta provisional y, solo si sale bien, se le pone el nombre
        let tmp = b.join(format!("{DIR}.pasando"));
        let _ = std::fs::remove_dir_all(&tmp);
        if copy_all(&old, &tmp).is_ok() && std::fs::rename(&tmp, &new).is_ok() {
            done.push((old, new));
        } else {
            let _ = std::fs::remove_dir_all(&tmp);
        }
    }
    done
}

const ACCENT: [u8; 3] = [14, 150, 136];

/// Contorno del diente en unidades del logotipo (cuadrado de 34 × 34, centro 0,0).
const TOOTH: [(f32, f32); 12] = [(-9.0, -8.0), (-4.0, -11.0), (0.0, -8.5), (4.0, -11.0), (9.0, -8.0), (8.0, 1.0), (5.0, 11.0), (2.5, 11.0), (0.0, 3.0), (-2.5, 11.0), (-5.0, 11.0), (-8.0, 1.0)];

fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let (wx, wy) = (p.0 - a.0, p.1 - a.1);
    let t = ((wx * vx + wy * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    let (dx, dy) = (p.0 - a.0 - t * vx, p.1 - a.1 - t * vy);
    (dx * dx + dy * dy).sqrt()
}

/// Píxeles RGBA del logotipo de `size` × `size` (con suavizado).
pub fn logo_rgba(size: u32) -> Vec<u8> {
    let n = size as f32;
    let radius = n * 9.0 / 42.0;
    let s = n / 34.0 * 0.82; // el diente ocupa lo mismo que en la barra lateral
    let stroke = 2.0 * s * 1.15;
    let mut out = vec![0u8; (size * size * 4) as usize];
    const SS: u32 = 4;
    for y in 0..size {
        for x in 0..size {
            let (mut bg, mut fg) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = x as f32 + (sx as f32 + 0.5) / SS as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SS as f32;
                    // cuadrado redondeado
                    let qx = (px - n / 2.0).abs() - (n / 2.0 - radius);
                    let qy = (py - n / 2.0).abs() - (n / 2.0 - radius);
                    let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
                    if d > 0.0 {
                        continue;
                    }
                    bg += 1;
                    let p = ((px - n / 2.0) / s, (py - n / 2.0) / s);
                    let dist = (0..TOOTH.len()).map(|i| seg_dist(p, TOOTH[i], TOOTH[(i + 1) % TOOTH.len()])).fold(f32::MAX, f32::min) * s;
                    if dist <= stroke / 2.0 {
                        fg += 1;
                    }
                }
            }
            let total = (SS * SS) as f32;
            let a = bg as f32 / total;
            let w = if bg > 0 { fg as f32 / bg as f32 } else { 0.0 };
            let i = ((y * size + x) * 4) as usize;
            for c in 0..3 {
                out[i + c] = (ACCENT[c] as f32 * (1.0 - w) + 255.0 * w).round() as u8;
            }
            out[i + 3] = (a * 255.0).round() as u8;
        }
    }
    out
}

/// Icono de la ventana (barra de tareas, Alt+Tab).
pub fn window_icon() -> egui::IconData {
    let size = 128;
    egui::IconData { rgba: logo_rgba(size), width: size, height: size }
}

/// Escribe los PNG del icono (32, 128, 256 y 512 px) en una carpeta.
pub fn write_icons(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut out = vec![];
    for size in [32u32, 128, 256, 512] {
        let img = image::RgbaImage::from_raw(size, size, logo_rgba(size)).ok_or("imagen no válida")?;
        let p = dir.join(format!("icono-{size}.png"));
        img.save(&p).map_err(|e| e.to_string())?;
        out.push(p);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_has_rounded_corners_and_a_white_tooth() {
        let n = 64u32;
        let px = logo_rgba(n);
        let at = |x: u32, y: u32| &px[((y * n + x) * 4) as usize..((y * n + x) * 4 + 4) as usize];
        assert_eq!(at(0, 0)[3], 0, "esquina transparente");
        assert_eq!(at(n / 2, 2), &[ACCENT[0], ACCENT[1], ACCENT[2], 255], "fondo del color de la marca");
        // algún píxel blanco del contorno del diente en el centro
        let white = (0..n).flat_map(|y| (0..n).map(move |x| (x, y))).filter(|(x, y)| at(*x, *y)[..3] == [255, 255, 255]).count();
        assert!(white > 40, "{white}");
        assert_eq!(window_icon().width, 128);
    }

    #[test]
    fn old_folders_move_to_the_new_name_once_and_never_overwrite() {
        let root = std::env::temp_dir().join(format!("vd_migr_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (datos, ajustes, docs) = (root.join("datos"), root.join("ajustes"), root.join("docs"));
        // datos de la clínica con su copia y una imagen, y los ajustes del puesto
        std::fs::create_dir_all(datos.join(OLD_DIR).join("copias")).unwrap();
        std::fs::write(datos.join(OLD_DIR).join("clinica.json"), "{\"pacientes\":1}").unwrap();
        std::fs::write(datos.join(OLD_DIR).join("copias").join("clinica_1.json"), "x").unwrap();
        std::fs::create_dir_all(ajustes.join(OLD_DIR)).unwrap();
        std::fs::write(ajustes.join(OLD_DIR).join("puesto.json"), "{}").unwrap();
        // en Documentos ya hay una carpeta con el nombre nuevo: no se toca
        std::fs::create_dir_all(docs.join(OLD_DIR)).unwrap();
        std::fs::write(docs.join(OLD_DIR).join("receta.html"), "antigua").unwrap();
        std::fs::create_dir_all(docs.join(DIR)).unwrap();
        std::fs::write(docs.join(DIR).join("receta.html"), "nueva").unwrap();
        let done = migrate_in(&[datos.clone(), ajustes.clone(), docs.clone(), datos.clone()]);
        assert_eq!(done.len(), 2);
        assert_eq!(std::fs::read_to_string(datos.join(DIR).join("clinica.json")).unwrap(), "{\"pacientes\":1}");
        assert!(datos.join(DIR).join("copias").join("clinica_1.json").exists());
        assert!(ajustes.join(DIR).join("puesto.json").exists());
        assert!(!datos.join(OLD_DIR).exists(), "movida, no duplicada");
        assert_eq!(std::fs::read_to_string(docs.join(DIR).join("receta.html")).unwrap(), "nueva");
        assert!(docs.join(OLD_DIR).join("receta.html").exists());
        // la segunda vez ya no hay nada que pasar
        assert!(migrate_in(&[datos.clone(), ajustes.clone(), docs.clone()]).is_empty());
        // copia completa cuando no se puede mover (aquí, la copia por dentro)
        let other = root.join("otro");
        std::fs::create_dir_all(other.join(OLD_DIR).join("a").join("b")).unwrap();
        std::fs::write(other.join(OLD_DIR).join("a").join("b").join("f.txt"), "z").unwrap();
        copy_all(&other.join(OLD_DIR), &other.join(DIR)).unwrap();
        assert_eq!(std::fs::read_to_string(other.join(DIR).join("a").join("b").join("f.txt")).unwrap(), "z");
        let _ = std::fs::remove_dir_all(&root);
    }
}
