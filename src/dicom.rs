//! Lectura de imágenes DICOM y de estudios CBCT (T-F5-06), sin diagnóstico:
//! solo se ven.
//!
//! - **Formato:** DICOM Parte 10 (preámbulo y «DICM») con sintaxis de
//!   transferencia sin comprimir: implícita o explícita *little endian*. Los
//!   equipos dentales las exportan así. Las comprimidas (JPEG, JPEG 2000, RLE)
//!   se rechazan con un aviso claro, sin intentar leerlas a medias.
//! - **Imagen:**
//!   - 8 o 16 bits, con o sin signo, en escala de grises (MONOCHROME1 y 2) o
//!     RGB de 8 bits;
//!   - con la pendiente y la ordenada de reescalado y la ventana del fichero, o
//!     la que se elija.
//! - **CBCT:**
//!   - una serie de cortes (una carpeta) o un fichero multicorte, ordenados por
//!     su posición;
//!   - vistas axial, coronal y sagital con la proporción real (tamaño de
//!     píxel y distancia entre cortes).
//! - **Orientación y escala:**
//!   - las letras de cada borde salen de `ImageOrientationPatient`;
//!   - los milímetros, de `PixelSpacing`.
//! - **Límites:**
//!   - [`MAX_BYTES`] de píxeles como mucho;
//!   - cualquier longitud o tamaño incoherente es un error, nunca un pánico
//!     (comprobado con bytes al azar).

use std::path::Path;

/// Memoria de un estudio como mucho (512 MB): la de los valores ya leídos (4
/// bytes por vóxel) y la del color, no solo lo que ocupa en el fichero.
pub const MAX_BYTES: usize = 512 << 20;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub rows: usize,
    pub cols: usize,
    pub frames: usize,
    pub bits: u16,
    pub signed: bool,
    pub samples: u16,
    /// Color por planos (`PlanarConfiguration` = 1).
    pub planar: bool,
    pub photometric: String,
    pub slope: f64,
    pub intercept: f64,
    pub window: Option<(f64, f64)>,
    /// mm entre filas y entre columnas.
    pub spacing: Option<(f64, f64)>,
    pub thickness: Option<f64>,
    pub position: Option<[f64; 3]>,
    /// Cosenos de dirección de las filas y de las columnas.
    pub orientation: Option<[f64; 6]>,
    pub patient: String,
    pub date: String,
    pub modality: String,
    pub instance: i64,
}

/// Una imagen (o un estudio de varios cortes) leída.
#[derive(Clone, Debug, PartialEq)]
pub struct Dicom {
    pub header: Header,
    /// Valores ya reescalados (en las de color, el gris medio), corte a corte.
    pub voxels: Vec<i32>,
    /// Píxeles RGB de 8 bits (solo en las de color).
    pub rgb: Option<Vec<u8>>,
}

const SUPPORTED: [&str; 2] = ["1.2.840.10008.1.2", "1.2.840.10008.1.2.1"];

fn compressed_name(ts: &str) -> &'static str {
    if ts.starts_with("1.2.840.10008.1.2.4.5") || ts.starts_with("1.2.840.10008.1.2.4.7") {
        "JPEG"
    } else if ts.starts_with("1.2.840.10008.1.2.4.9") {
        "JPEG 2000"
    } else if ts == "1.2.840.10008.1.2.5" {
        "RLE"
    } else if ts == "1.2.840.10008.1.2.2" {
        "big endian"
    } else {
        "desconocida"
    }
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u16(&mut self) -> Result<u16, String> {
        let s = self.b.get(self.pos..self.pos + 2).ok_or("Fichero DICOM cortado.")?;
        self.pos += 2;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, String> {
        let s = self.b.get(self.pos..self.pos + 4).ok_or("Fichero DICOM cortado.")?;
        self.pos += 4;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).ok_or("Longitud no válida en el DICOM.")?;
        let s = self.b.get(self.pos..end).ok_or("Fichero DICOM cortado.")?;
        self.pos = end;
        Ok(s)
    }
}

/// VR con longitud de 4 bytes en la sintaxis explícita.
fn long_vr(vr: &[u8]) -> bool {
    matches!(vr, b"OB" | b"OW" | b"OF" | b"SQ" | b"UT" | b"UN" | b"UC" | b"UR" | b"OD" | b"OL" | b"OV" | b"SV" | b"UV")
}

/// Salta una secuencia de longitud indefinida (con sus elementos anidados).
fn skip_undefined(r: &mut Reader, explicit: bool, depth: u32) -> Result<(), String> {
    if depth > 16 {
        return Err("Secuencias anidadas demasiado hondas en el DICOM.".into());
    }
    loop {
        let (g, e) = (r.u16()?, r.u16()?);
        if (g, e) == (0xFFFE, 0xE0DD) {
            r.u32()?;
            return Ok(());
        }
        if (g, e) == (0xFFFE, 0xE000) {
            let len = r.u32()?;
            if len == u32::MAX {
                // elementos hasta el final del ítem
                loop {
                    let save = r.pos;
                    let (g2, e2) = (r.u16()?, r.u16()?);
                    if (g2, e2) == (0xFFFE, 0xE00D) {
                        r.u32()?;
                        break;
                    }
                    r.pos = save;
                    element(r, explicit, depth + 1)?;
                }
            } else {
                r.take(len as usize)?;
            }
            continue;
        }
        return Err("Secuencia mal formada en el DICOM.".into());
    }
}

/// Lee un elemento: (grupo, elemento, valor). El valor de una secuencia va vacío.
fn element<'a>(r: &mut Reader<'a>, explicit: bool, depth: u32) -> Result<(u16, u16, &'a [u8]), String> {
    let (g, e) = (r.u16()?, r.u16()?);
    let (sq, len) = if explicit && g != 0xFFFE {
        let vr = r.take(2)?;
        if long_vr(vr) {
            r.u16()?;
            (vr == b"SQ", r.u32()?)
        } else {
            (false, r.u16()? as u32)
        }
    } else {
        (false, r.u32()?)
    };
    if len == u32::MAX {
        // secuencia (o datos encapsulados, que no se admiten)
        if (g, e) == (0x7FE0, 0x0010) {
            return Err("Los píxeles están comprimidos (encapsulados): no se admiten.".into());
        }
        skip_undefined(r, explicit, depth)?;
        return Ok((g, e, &[]));
    }
    let v = r.take(len as usize)?;
    Ok((g, e, if sq { &[] } else { v }))
}

fn text(v: &[u8]) -> String {
    String::from_utf8_lossy(v).trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string()
}

fn numbers(v: &[u8]) -> Vec<f64> {
    text(v).split('\\').filter_map(|x| x.trim().parse().ok()).collect()
}

fn u16_of(v: &[u8]) -> Option<u16> {
    v.get(..2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}

/// Lee un fichero DICOM ya cargado (en el programa se lee con [`read`]).
#[cfg(test)]
pub fn parse(bytes: &[u8]) -> Result<Dicom, String> {
    parse_within(bytes, MAX_BYTES)
}

/// Lee un fichero DICOM sin pasar de `budget` bytes de memoria para la imagen.
fn parse_within(bytes: &[u8], budget: usize) -> Result<Dicom, String> {
    if bytes.len() < 132 || &bytes[128..132] != b"DICM" {
        return Err("No es un fichero DICOM (falta la marca «DICM»).".into());
    }
    let mut r = Reader { b: bytes, pos: 132 };
    let mut ts = String::new();
    // cabecera de meta (grupo 0002), siempre explícita
    while r.pos + 4 <= bytes.len() && u16::from_le_bytes([bytes[r.pos], bytes[r.pos + 1]]) == 0x0002 {
        let (_, e, v) = element(&mut r, true, 0)?;
        if e == 0x0010 {
            ts = text(v);
        }
    }
    if !SUPPORTED.contains(&ts.as_str()) {
        return Err(format!("Compresión {} ({ts}) no admitida: exporte el estudio desde el programa del equipo como DICOM sin comprimir.", compressed_name(&ts)));
    }
    let explicit = ts == "1.2.840.10008.1.2.1";
    let mut h = Header { frames: 1, slope: 1.0, samples: 1, photometric: "MONOCHROME2".into(), ..Default::default() };
    let mut pixels: Option<&[u8]> = None;
    while r.pos < bytes.len() {
        let (g, e, v) = element(&mut r, explicit, 0)?;
        match (g, e) {
            (0x0008, 0x0020) => h.date = text(v),
            (0x0008, 0x0060) => h.modality = text(v),
            (0x0010, 0x0010) => h.patient = text(v).replace('^', " ").trim().to_string(),
            (0x0018, 0x0050) => h.thickness = numbers(v).first().copied(),
            (0x0020, 0x0013) => h.instance = numbers(v).first().copied().unwrap_or(0.0) as i64,
            (0x0020, 0x0032) => h.position = <[f64; 3]>::try_from(numbers(v)).ok(),
            (0x0020, 0x0037) => h.orientation = <[f64; 6]>::try_from(numbers(v)).ok(),
            (0x0028, 0x0002) => h.samples = u16_of(v).unwrap_or(1),
            (0x0028, 0x0004) => h.photometric = text(v),
            (0x0028, 0x0006) => h.planar = u16_of(v) == Some(1),
            (0x0028, 0x0008) => h.frames = numbers(v).first().copied().unwrap_or(1.0).max(1.0) as usize,
            (0x0028, 0x0010) => h.rows = u16_of(v).unwrap_or(0) as usize,
            (0x0028, 0x0011) => h.cols = u16_of(v).unwrap_or(0) as usize,
            (0x0028, 0x0030) => h.spacing = numbers(v).get(..2).map(|s| (s[0], s[1])),
            (0x0028, 0x0100) => h.bits = u16_of(v).unwrap_or(0),
            (0x0028, 0x0103) => h.signed = u16_of(v) == Some(1),
            (0x0028, 0x1050) => {
                if let Some(c) = numbers(v).first() {
                    h.window = Some((*c, h.window.map(|w| w.1).unwrap_or(0.0)));
                }
            }
            (0x0028, 0x1051) => {
                if let Some(w) = numbers(v).first() {
                    h.window = Some((h.window.map(|c| c.0).unwrap_or(0.0), *w));
                }
            }
            (0x0028, 0x1052) => h.intercept = numbers(v).first().copied().unwrap_or(0.0),
            (0x0028, 0x1053) => h.slope = numbers(v).first().copied().unwrap_or(1.0),
            (0x7FE0, 0x0010) => pixels = Some(v),
            _ => {}
        }
    }
    let px = pixels.ok_or("El DICOM no tiene imagen.")?;
    if h.rows == 0 || h.cols == 0 {
        return Err("El DICOM no dice el tamaño de la imagen.".into());
    }
    let bytes_px = match (h.bits, h.samples) {
        (8, 1) => 1,
        (16, 1) => 2,
        (8, 3) => 3,
        (b, s) => return Err(format!("Formato de píxel no admitido ({b} bits, {s} muestras).")),
    };
    let n = h.rows.checked_mul(h.cols).and_then(|x| x.checked_mul(h.frames)).ok_or("Imagen demasiado grande.")?;
    let need = n.checked_mul(bytes_px).ok_or("Imagen demasiado grande.")?;
    // lo que de verdad se reserva: 4 bytes por vóxel y, en color, los bytes RGB
    let alloc = n.checked_mul(4).and_then(|x| x.checked_add(if bytes_px == 3 { need } else { 0 })).ok_or("Imagen demasiado grande.")?;
    if alloc > budget {
        return Err(format!("El estudio necesita {} MB de memoria y el límite es {} MB.", alloc >> 20, budget >> 20));
    }
    if px.len() < need {
        return Err("Faltan píxeles: el fichero está cortado.".into());
    }
    let mut rgb = None;
    let voxels: Vec<i32> = match bytes_px {
        1 => px[..need].iter().map(|v| rescale(&h, if h.signed { *v as i8 as i32 } else { *v as i32 })).collect(),
        2 => px[..need]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                let u = u16::from_le_bytes([c[0], c[1]]);
                rescale(&h, if h.signed { u as i16 as i32 } else { u as i32 })
            })
            .collect(),
        _ => {
            // color: entrelazado (RGBRGB…) o por planos (RR…GG…BB…)
            let data: Vec<u8> = if h.planar {
                let plane = n;
                (0..n).flat_map(|i| [px[i], px[plane + i], px[2 * plane + i]]).collect()
            } else {
                px[..need].to_vec()
            };
            let gray = data.as_chunks::<3>().0.iter().map(|c| (c[0] as i32 + c[1] as i32 + c[2] as i32) / 3).collect();
            rgb = Some(data);
            gray
        }
    };
    Ok(Dicom { header: h, voxels, rgb })
}

fn rescale(h: &Header, v: i32) -> i32 {
    (v as f64 * h.slope + h.intercept).round() as i32
}

/// Lee un fichero.
pub fn read(path: &Path) -> Result<Dicom, String> {
    read_within(path, MAX_BYTES)
}

/// Lee un fichero sin pasar de `budget`: ni el fichero ni la imagen.
fn read_within(path: &Path, budget: usize) -> Result<Dicom, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("No se puede leer {}: {e}", path.display()))?;
    if meta.len() > budget as u64 {
        return Err(format!("El fichero ocupa {} MB y el límite es {} MB.", meta.len() >> 20, budget >> 20));
    }
    parse_within(&std::fs::read(path).map_err(|e| format!("No se puede leer {}: {e}", path.display()))?, budget)
}

/// Un estudio CBCT: una carpeta de cortes (o un fichero multicorte).
pub fn read_series(path: &Path) -> Result<Dicom, String> {
    if path.is_file() {
        return read(path);
    }
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(path).map_err(|e| format!("No se puede leer la carpeta: {e}"))?.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    files.sort();
    let mut slices: Vec<Dicom> = vec![];
    let mut used = 0usize;
    for f in files {
        // cada corte, con lo que queda del presupuesto (antes de leerlo)
        let left = MAX_BYTES.saturating_sub(used);
        match read_within(&f, left) {
            Ok(d) => {
                used += d.voxels.len() * 4;
                slices.push(d);
            }
            Err(e) if e.contains("límite") => return Err(format!("El estudio pasa de {} MB de memoria.", MAX_BYTES >> 20)),
            // los ficheros que no son DICOM (índices, miniaturas) no cuentan
            Err(_) => continue,
        }
    }
    let first = slices.first().ok_or("La carpeta no tiene cortes DICOM.")?.header.clone();
    if slices.iter().any(|s| s.header.rows != first.rows || s.header.cols != first.cols || s.header.frames != 1) {
        return Err("Los cortes no tienen todos el mismo tamaño: no son de un mismo estudio.".into());
    }
    // por su posición a lo largo de la normal del corte (o su número)
    let normal = first.orientation.map(|o| [o[1] * o[5] - o[2] * o[4], o[2] * o[3] - o[0] * o[5], o[0] * o[4] - o[1] * o[3]]);
    let key = |d: &Dicom| match (normal, d.header.position) {
        (Some(n), Some(p)) => n[0] * p[0] + n[1] * p[1] + n[2] * p[2],
        _ => d.header.instance as f64,
    };
    slices.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal));
    let gap = if slices.len() > 1 { (key(&slices[1]) - key(&slices[0])).abs() } else { 0.0 };
    let mut h = first.clone();
    h.frames = slices.len();
    if gap > 0.0 {
        h.thickness = Some(gap);
    }
    // sin copias de más: el volumen se reserva justo
    let mut voxels = Vec::with_capacity(used / 4);
    for s in slices {
        voxels.extend(s.voxels);
    }
    Ok(Dicom { header: h, voxels, rgb: None })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Axial,
    Coronal,
    Sagital,
}

/// Un corte en grises de 8 bits con la ventana (centro, anchura), su tamaño
/// y los mm de un píxel (ancho, alto) para la escala y la proporción.
pub struct Slice {
    pub w: usize,
    pub h: usize,
    pub gray: Vec<u8>,
    pub mm: (f64, f64),
}

impl Dicom {
    /// Ventana del fichero o, si no tiene, la de los valores.
    pub fn default_window(&self) -> (f64, f64) {
        match self.header.window {
            Some((c, w)) if w > 0.0 => (c, w),
            _ => {
                let (lo, hi) = self.voxels.iter().fold((i32::MAX, i32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
                if lo > hi { (0.0, 1.0) } else { ((lo as f64 + hi as f64) / 2.0, (hi - lo).max(1) as f64) }
            }
        }
    }

    pub fn slices(&self, v: View) -> usize {
        let h = &self.header;
        match v {
            View::Axial => h.frames,
            View::Coronal => h.rows,
            View::Sagital => h.cols,
        }
    }

    pub fn slice(&self, v: View, index: usize, window: (f64, f64)) -> Slice {
        let h = &self.header;
        let (sr, sc) = h.spacing.unwrap_or((1.0, 1.0));
        let sz = h.thickness.unwrap_or(sr);
        let (c, w) = (window.0, window.1.max(1.0));
        let mono1 = h.photometric.eq_ignore_ascii_case("MONOCHROME1");
        let g = |x: i32| -> u8 {
            let t = ((x as f64 - (c - w / 2.0)) / w).clamp(0.0, 1.0);
            let t = if mono1 { 1.0 - t } else { t };
            (t * 255.0) as u8
        };
        let at = |z: usize, y: usize, x: usize| self.voxels.get((z * h.rows + y) * h.cols + x).copied().unwrap_or(0);
        let i = index.min(self.slices(v).saturating_sub(1));
        match v {
            View::Axial => Slice { w: h.cols, h: h.rows, gray: (0..h.rows).flat_map(|y| (0..h.cols).map(move |x| (y, x))).map(|(y, x)| g(at(i, y, x))).collect(), mm: (sc, sr) },
            // de arriba (último corte) a abajo
            View::Coronal => Slice { w: h.cols, h: h.frames, gray: (0..h.frames).rev().flat_map(|z| (0..h.cols).map(move |x| (z, x))).map(|(z, x)| g(at(z, i, x))).collect(), mm: (sc, sz) },
            View::Sagital => Slice { w: h.rows, h: h.frames, gray: (0..h.frames).rev().flat_map(|z| (0..h.rows).map(move |y| (z, y))).map(|(z, y)| g(at(z, y, i))).collect(), mm: (sr, sz) },
        }
    }

    /// Letras de los bordes izquierdo, derecho, superior e inferior del corte
    /// axial («?» sin orientación en el fichero).
    pub fn edges(&self) -> [String; 4] {
        let Some(o) = self.header.orientation else { return ["?".into(), "?".into(), "?".into(), "?".into()] };
        let row = [o[0], o[1], o[2]];
        let col = [o[3], o[4], o[5]];
        let neg = |v: [f64; 3]| [-v[0], -v[1], -v[2]];
        [letters(neg(row)), letters(row), letters(neg(col)), letters(col)]
    }
}

/// Letras del sistema del paciente (LPS de DICOM) hacia donde apunta `v`:
/// R/L (derecha, izquierda), A/P (anterior, posterior), H/F (cabeza, pies).
pub fn letters(v: [f64; 3]) -> String {
    let mut parts: Vec<(f64, char)> = vec![(v[0], if v[0] > 0.0 { 'L' } else { 'R' }), (v[1], if v[1] > 0.0 { 'P' } else { 'A' }), (v[2], if v[2] > 0.0 { 'H' } else { 'F' })];
    parts.sort_by(|a, b| b.0.abs().partial_cmp(&a.0.abs()).unwrap_or(std::cmp::Ordering::Equal));
    parts.iter().filter(|p| p.0.abs() > 0.3).map(|p| p.1).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn el(out: &mut Vec<u8>, explicit: bool, g: u16, e: u16, vr: &str, v: &[u8]) {
        out.extend(g.to_le_bytes());
        out.extend(e.to_le_bytes());
        let mut v = v.to_vec();
        if v.len() % 2 == 1 {
            v.push(if vr == "OB" || vr == "US" { 0 } else { b' ' });
        }
        if explicit || g == 0x0002 {
            out.extend(vr.as_bytes());
            if long_vr(vr.as_bytes()) {
                out.extend([0, 0]);
                out.extend((v.len() as u32).to_le_bytes());
            } else {
                out.extend((v.len() as u16).to_le_bytes());
            }
        } else {
            out.extend((v.len() as u32).to_le_bytes());
        }
        out.extend(v);
    }

    /// DICOM sintético: `rows`×`cols`, 16 bits, píxel (y, x) = f(y, x).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn synth(explicit: bool, rows: u16, cols: u16, z: f64, instance: i64, f: impl Fn(usize, usize) -> u16, ts: &str, extra: &[(u16, u16, &str, Vec<u8>)]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b.extend(b"DICM");
        el(&mut b, true, 0x0002, 0x0010, "UI", ts.as_bytes());
        el(&mut b, explicit, 0x0008, 0x0020, "DA", b"20261114");
        el(&mut b, explicit, 0x0008, 0x0060, "CS", b"CT");
        // una secuencia de longitud indefinida que hay que saltar
        b.extend(0x0008u16.to_le_bytes());
        b.extend(0x1140u16.to_le_bytes());
        if explicit {
            b.extend(b"SQ");
            b.extend([0, 0]);
        }
        b.extend(u32::MAX.to_le_bytes());
        b.extend(0xFFFEu16.to_le_bytes());
        b.extend(0xE000u16.to_le_bytes());
        b.extend(u32::MAX.to_le_bytes());
        el(&mut b, explicit, 0x0008, 0x1150, "UI", b"1.2.3");
        b.extend(0xFFFEu16.to_le_bytes());
        b.extend(0xE00Du16.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(0xFFFEu16.to_le_bytes());
        b.extend(0xE0DDu16.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        el(&mut b, explicit, 0x0010, 0x0010, "PN", b"PRUEBA^SINTETICA");
        el(&mut b, explicit, 0x0020, 0x0013, "IS", instance.to_string().as_bytes());
        el(&mut b, explicit, 0x0020, 0x0032, "DS", format!("0\\0\\{z}").as_bytes());
        el(&mut b, explicit, 0x0020, 0x0037, "DS", b"1\\0\\0\\0\\1\\0");
        el(&mut b, explicit, 0x0028, 0x0002, "US", &1u16.to_le_bytes());
        el(&mut b, explicit, 0x0028, 0x0004, "CS", b"MONOCHROME2");
        el(&mut b, explicit, 0x0028, 0x0010, "US", &rows.to_le_bytes());
        el(&mut b, explicit, 0x0028, 0x0011, "US", &cols.to_le_bytes());
        el(&mut b, explicit, 0x0028, 0x0030, "DS", b"0.2\\0.25");
        el(&mut b, explicit, 0x0028, 0x0100, "US", &16u16.to_le_bytes());
        el(&mut b, explicit, 0x0028, 0x0103, "US", &0u16.to_le_bytes());
        for (g, e, vr, v) in extra {
            el(&mut b, explicit, *g, *e, vr, v);
        }
        let px: Vec<u8> = (0..rows as usize).flat_map(|y| (0..cols as usize).map(move |x| (y, x))).flat_map(|(y, x)| f(y, x).to_le_bytes()).collect();
        el(&mut b, explicit, 0x7FE0, 0x0010, "OW", &px);
        b
    }

    #[test]
    fn synthetic_images_are_read_with_scale_window_and_orientation() {
        for explicit in [true, false] {
            let ts = if explicit { "1.2.840.10008.1.2.1" } else { "1.2.840.10008.1.2" };
            let b = synth(explicit, 4, 6, 0.0, 1, |y, x| (y * 100 + x) as u16, ts, &[(0x0028, 0x1052, "DS", b"-1000".to_vec()), (0x0028, 0x1053, "DS", b"2".to_vec())]);
            let d = parse(&b).unwrap();
            assert_eq!((d.header.rows, d.header.cols, d.header.frames), (4, 6, 1));
            assert_eq!(d.header.spacing, Some((0.2, 0.25)));
            assert_eq!(d.header.patient, "PRUEBA SINTETICA");
            // reescalado: 2·v − 1000
            assert_eq!(d.voxels[6 + 3], 2 * 103 - 1000, "fila 1, columna 3");
            let s = d.slice(View::Axial, 0, (-1000.0 + 300.0, 600.0));
            assert_eq!((s.w, s.h, s.mm), (6, 4, (0.25, 0.2)));
            assert_eq!(s.gray[0], 0, "el valor más bajo, negro");
            assert_eq!(s.gray[3 * 6 + 5], (((2 * 305 - 1000) as f64 + 1000.0) / 600.0 * 255.0) as u8);
            // filas hacia la izquierda del paciente y columnas hacia atrás
            assert_eq!(d.edges(), ["R".to_string(), "L".to_string(), "A".to_string(), "P".to_string()]);
        }
        assert_eq!(letters([0.0, 0.0, 1.0]), "H");
        assert_eq!(letters([0.7, -0.7, 0.0]), "LA");
    }

    #[test]
    fn a_cbct_series_is_ordered_and_resliced_with_its_proportions() {
        let dir = std::env::temp_dir().join(format!("ds_dicom_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // cinco cortes guardados desordenados, cada uno con su número en los píxeles
        for (name, z) in [("c.dcm", 2.0), ("a.dcm", 4.0), ("e.dcm", 0.0), ("b.dcm", 3.0), ("d.dcm", 1.0)] {
            let b = synth(true, 3, 4, z * 0.4, 0, move |_, _| z as u16, "1.2.840.10008.1.2.1", &[]);
            std::fs::write(dir.join(name), b).unwrap();
        }
        std::fs::write(dir.join("LEEME.txt"), b"no es un DICOM").unwrap();
        let v = read_series(&dir).unwrap();
        assert_eq!(v.header.frames, 5);
        assert!((v.header.thickness.unwrap() - 0.4).abs() < 1e-9, "distancia entre cortes");
        let first: Vec<i32> = (0..5).map(|z| v.voxels[z * 12]).collect();
        assert_eq!(first, vec![0, 1, 2, 3, 4], "ordenados por su posición");
        let cor = v.slice(View::Coronal, 1, (2.0, 4.0));
        assert_eq!((cor.w, cor.h, cor.mm), (4, 5, (0.25, 0.4)));
        assert!(cor.gray[0] > cor.gray[4 * 4], "arriba el último corte");
        assert_eq!(v.slices(View::Sagital), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_compressed_huge_or_random_files_are_refused_without_panicking() {
        assert!(parse(b"hola").unwrap_err().contains("DICM"));
        let jpeg = synth(true, 2, 2, 0.0, 1, |_, _| 0, "1.2.840.10008.1.2.4.50", &[]);
        assert!(parse(&jpeg).unwrap_err().contains("JPEG"));
        let good = synth(true, 8, 8, 0.0, 1, |y, x| (y + x) as u16, "1.2.840.10008.1.2.1", &[]);
        assert!(parse(&good[..good.len() - 10]).is_err(), "cortado");
        // más de lo que se admite: no se intenta reservar
        let huge = synth(true, 2, 2, 0.0, 1, |_, _| 0, "1.2.840.10008.1.2.1", &[(0x0028, 0x0008, "IS", b"999999".to_vec())]);
        assert!(parse(&huge).is_err());
        // el límite es de memoria (4 bytes por vóxel), no de bytes del fichero:
        // 200×200×5000 de 16 bits son 400 MB en el fichero y 800 MB en memoria
        let big = synth(true, 200, 200, 0.0, 1, |_, _| 0, "1.2.840.10008.1.2.1", &[(0x0028, 0x0008, "IS", b"5000".to_vec())]);
        assert!(parse(&big).unwrap_err().contains("memoria"));
        assert!(parse_within(&good, 100).unwrap_err().contains("límite"), "con un presupuesto más pequeño, tampoco");
        let mut tall = good.clone();
        // filas = 65535 (el valor de «Rows» está tras su cabecera explícita)
        let at = tall.windows(4).position(|w| w == [0x28, 0x00, 0x10, 0x00]).unwrap() + 8;
        tall[at..at + 2].copy_from_slice(&65535u16.to_le_bytes());
        assert!(parse(&tall).is_err());
        // bytes al azar y mutaciones: error o imagen, nunca un pánico
        let mut rng = crate::test_rng::Rng(5);
        for _ in 0..3000 {
            let b = rng.mutate(&good);
            let _ = parse(&b);
            let n = rng.below(400);
            let mut r = vec![0u8; 128];
            r.extend(b"DICM");
            r.extend(rng.bytes(n));
            let _ = parse(&r);
        }
    }
}
