//! Modelos 3D en STL (T-F5-06): escaneos intraorales y modelos de
//! laboratorio. Se leen (binario o ASCII), se miden en milímetros y se ven
//! sombreados desde el ángulo que se elija. Solo se ven: sin diagnóstico.
//!
//! - **Límites:** [`MAX_TRIANGLES`]. Un número de triángulos que no cuadra con
//!   el tamaño del fichero, o valores no finitos, es un error, nunca un pánico.
//! - **Sin bloquear:** la vista se calcula en segundo plano (ver la pantalla de
//!   imágenes).

/// Triángulos como mucho (un escaneo de una arcada tiene del orden de 10⁵–10⁶).
pub const MAX_TRIANGLES: usize = 5_000_000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub tris: Vec<[[f32; 3]; 3]>,
}

impl Mesh {
    /// Caja que lo contiene: (mínimo, máximo), en las unidades del fichero (mm).
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for t in &self.tris {
            for v in t {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
        }
        if self.tris.is_empty() { ([0.0; 3], [0.0; 3]) } else { (lo, hi) }
    }

    /// Medidas en mm (ancho, alto, fondo).
    pub fn size(&self) -> [f32; 3] {
        let (lo, hi) = self.bounds();
        [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
    }
}

fn finite(t: &[[f32; 3]; 3]) -> bool {
    t.iter().flatten().all(|x| x.is_finite())
}

/// Lee un STL (binario o ASCII).
pub fn parse(b: &[u8]) -> Result<Mesh, String> {
    // binario: cabecera de 80 bytes, número de triángulos y 50 bytes por triángulo
    if b.len() >= 84 {
        let n = u32::from_le_bytes([b[80], b[81], b[82], b[83]]) as usize;
        if n.checked_mul(50).and_then(|x| x.checked_add(84)) == Some(b.len()) {
            if n > MAX_TRIANGLES {
                return Err(format!("El modelo tiene {n} triángulos y el límite es {MAX_TRIANGLES}."));
            }
            let mut tris = Vec::with_capacity(n);
            for c in b[84..].as_chunks::<50>().0 {
                let f = |i: usize| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
                let t = [[f(12), f(16), f(20)], [f(24), f(28), f(32)], [f(36), f(40), f(44)]];
                if !finite(&t) {
                    return Err("El modelo tiene coordenadas no válidas.".into());
                }
                tris.push(t);
            }
            return Ok(Mesh { tris });
        }
    }
    // ASCII: «solid … facet … vertex x y z … endsolid»
    let text = std::str::from_utf8(b).map_err(|_| "No es un STL: ni binario coherente ni texto.".to_string())?;
    if !text.trim_start().to_ascii_lowercase().starts_with("solid") {
        return Err("No es un STL (ni binario coherente ni texto «solid»).".into());
    }
    let mut tris = vec![];
    let mut cur: Vec<[f32; 3]> = vec![];
    for line in text.lines() {
        let mut w = line.split_whitespace();
        if w.next().map(|x| x.eq_ignore_ascii_case("vertex")) != Some(true) {
            continue;
        }
        let v: Vec<f32> = w.filter_map(|x| x.parse().ok()).collect();
        if v.len() != 3 || !v.iter().all(|x| x.is_finite()) {
            return Err(format!("Vértice no válido: «{}».", line.trim()));
        }
        cur.push([v[0], v[1], v[2]]);
        if cur.len() == 3 {
            tris.push([cur[0], cur[1], cur[2]]);
            cur.clear();
            if tris.len() > MAX_TRIANGLES {
                return Err(format!("El modelo pasa de {MAX_TRIANGLES} triángulos."));
            }
        }
    }
    if !cur.is_empty() {
        return Err("El STL está cortado: un triángulo sin terminar.".into());
    }
    if tris.is_empty() {
        return Err("El STL no tiene triángulos.".into());
    }
    Ok(Mesh { tris })
}

/// Tamaño del fichero como mucho: el de [`MAX_TRIANGLES`] en binario.
pub const MAX_FILE: u64 = 84 + MAX_TRIANGLES as u64 * 50;

/// Lee un modelo 3D: STL, PLY (texto o binario) u OBJ, según su extensión.
pub fn read(path: &std::path::Path) -> Result<Mesh, String> {
    let len = std::fs::metadata(path).map_err(|e| format!("No se puede leer {}: {e}", path.display()))?.len();
    if len > MAX_FILE {
        return Err(format!("El fichero ocupa {} MB y el límite es {} MB: expórtelo en STL binario o con menos detalle.", len >> 20, MAX_FILE >> 20));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("No se puede leer {}: {e}", path.display()))?;
    match path.extension().map(|x| x.to_string_lossy().to_lowercase()).as_deref() {
        Some("ply") => parse_ply(&bytes),
        Some("obj") => parse_obj(&bytes),
        _ => parse(&bytes),
    }
}

/// Une los vértices de cada cara en triángulos (en abanico) y comprueba que
/// existen y que no pasan del límite.
fn triangulate(verts: &[[f32; 3]], faces: &[Vec<usize>]) -> Result<Mesh, String> {
    let mut tris = vec![];
    for f in faces {
        if f.iter().any(|&i| i >= verts.len()) {
            return Err("Una cara usa un vértice que no existe.".into());
        }
        for k in 1..f.len().saturating_sub(1) {
            tris.push([verts[f[0]], verts[f[k]], verts[f[k + 1]]]);
            if tris.len() > MAX_TRIANGLES {
                return Err(format!("El modelo pasa de {MAX_TRIANGLES} triángulos."));
            }
        }
    }
    if tris.is_empty() {
        return Err("El modelo no tiene caras.".into());
    }
    Ok(Mesh { tris })
}

/// Bytes de un tipo de dato de PLY.
fn ply_size(t: &str) -> Option<usize> {
    Some(match t {
        "char" | "int8" | "uchar" | "uint8" => 1,
        "short" | "int16" | "ushort" | "uint16" => 2,
        "int" | "int32" | "uint" | "uint32" | "float" | "float32" => 4,
        "double" | "float64" => 8,
        _ => return None,
    })
}

/// Un número de PLY binario como f64.
fn ply_num(b: &[u8], t: &str, big: bool) -> f64 {
    macro_rules! get {
        ($ty:ty, $n:expr) => {{
            let mut a = [0u8; $n];
            a.copy_from_slice(&b[..$n]);
            (if big { <$ty>::from_be_bytes(a) } else { <$ty>::from_le_bytes(a) }) as f64
        }};
    }
    match t {
        "char" | "int8" => b[0] as i8 as f64,
        "uchar" | "uint8" => b[0] as f64,
        "short" | "int16" => get!(i16, 2),
        "ushort" | "uint16" => get!(u16, 2),
        "int" | "int32" => get!(i32, 4),
        "uint" | "uint32" => get!(u32, 4),
        "float" | "float32" => get!(f32, 4),
        _ => get!(f64, 8),
    }
}

/// Lee un PLY (el formato de muchos escáneres intraorales): en texto o en
/// binario, con los vértices y las caras; el color y lo demás se pasa por alto.
pub fn parse_ply(b: &[u8]) -> Result<Mesh, String> {
    let end = b.windows(10).position(|w| w == b"end_header").ok_or("No es un PLY (falta «end_header»).")?;
    let header = std::str::from_utf8(&b[..end]).map_err(|_| "Cabecera de PLY no válida.".to_string())?;
    if !header.starts_with("ply") {
        return Err("No es un PLY (no empieza por «ply»).".into());
    }
    // los datos empiezan tras el salto de línea de «end_header»
    let mut pos = end + 10;
    while pos < b.len() && (b[pos] == b'\r' || b[pos] == b' ') {
        pos += 1;
    }
    if b.get(pos) == Some(&b'\n') {
        pos += 1;
    }
    struct Element {
        name: String,
        count: usize,
        /// (tipo, nombre) o, en una lista, (tipo del número, tipo de cada dato).
        props: Vec<(String, String, Option<String>)>,
    }
    let mut format = "";
    let mut elements: Vec<Element> = vec![];
    for line in header.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["format", f, ..] => format = f,
            ["element", name, n] => elements.push(Element { name: name.to_string(), count: n.parse().map_err(|_| "Número de elementos no válido en el PLY.".to_string())?, props: vec![] }),
            ["property", "list", count_t, item_t, name] => elements.last_mut().ok_or("Propiedad sin elemento en el PLY.")?.props.push((count_t.to_string(), name.to_string(), Some(item_t.to_string()))),
            ["property", t, name] => elements.last_mut().ok_or("Propiedad sin elemento en el PLY.")?.props.push((t.to_string(), name.to_string(), None)),
            _ => {}
        }
    }
    let vcount = elements.iter().find(|e| e.name == "vertex").map(|e| e.count).unwrap_or(0);
    if vcount > MAX_TRIANGLES * 3 {
        return Err(format!("El modelo tiene {vcount} vértices: demasiados."));
    }
    let mut verts: Vec<[f32; 3]> = Vec::with_capacity(vcount);
    let mut faces: Vec<Vec<usize>> = vec![];
    match format {
        "ascii" => {
            let text = std::str::from_utf8(&b[pos..]).map_err(|_| "Datos de PLY no válidos.".to_string())?;
            let mut lines = text.lines().filter(|l| !l.trim().is_empty());
            for el in &elements {
                for _ in 0..el.count {
                    let line = lines.next().ok_or("El PLY está cortado.")?;
                    let nums: Vec<f64> = line.split_whitespace().map(|x| x.parse().map_err(|_| format!("Número no válido en el PLY: «{x}»."))).collect::<Result<_, _>>()?;
                    let mut i = 0;
                    let (mut xyz, mut face) = ([0f32; 3], vec![]);
                    for (t, name, item) in &el.props {
                        let _ = t;
                        if item.is_some() {
                            let n = *nums.get(i).ok_or("El PLY está cortado.")? as usize;
                            face = nums.get(i + 1..i + 1 + n).ok_or("El PLY está cortado.")?.iter().map(|x| *x as usize).collect();
                            i += 1 + n;
                        } else {
                            let v = *nums.get(i).ok_or("El PLY está cortado.")?;
                            if let Some(k) = ["x", "y", "z"].iter().position(|c| c == name) {
                                xyz[k] = v as f32;
                            }
                            i += 1;
                        }
                    }
                    match el.name.as_str() {
                        "vertex" => verts.push(xyz),
                        "face" => faces.push(face),
                        _ => {}
                    }
                }
            }
        }
        "binary_little_endian" | "binary_big_endian" => {
            let big = format == "binary_big_endian";
            for el in &elements {
                // cada registro ocupa al menos un byte: un número mayor que el fichero es mentira
                if el.props.is_empty() || el.count > b.len() {
                    if el.count > 0 && !el.props.is_empty() {
                        return Err("El PLY dice tener más datos de los que tiene.".into());
                    }
                    continue;
                }
                for _ in 0..el.count {
                    let (mut xyz, mut face) = ([0f32; 3], vec![]);
                    for (t, name, item) in &el.props {
                        let size = ply_size(t).ok_or_else(|| format!("Tipo de dato de PLY desconocido: {t}."))?;
                        let v = ply_num(b.get(pos..pos + size).ok_or("El PLY está cortado.")?, t, big);
                        pos += size;
                        match item {
                            Some(it) => {
                                let isz = ply_size(it).ok_or_else(|| format!("Tipo de dato de PLY desconocido: {it}."))?;
                                let n = v as usize;
                                if n > 64 {
                                    return Err("Una cara del PLY tiene demasiados vértices.".into());
                                }
                                for _ in 0..n {
                                    face.push(ply_num(b.get(pos..pos + isz).ok_or("El PLY está cortado.")?, it, big) as usize);
                                    pos += isz;
                                }
                            }
                            None => {
                                if let Some(k) = ["x", "y", "z"].iter().position(|c| c == name) {
                                    xyz[k] = v as f32;
                                }
                            }
                        }
                    }
                    match el.name.as_str() {
                        "vertex" => verts.push(xyz),
                        "face" => faces.push(face),
                        _ => {}
                    }
                }
            }
        }
        f => return Err(format!("Formato de PLY desconocido: «{f}».")),
    }
    if !verts.iter().flatten().all(|x| x.is_finite()) {
        return Err("El modelo tiene coordenadas no válidas.".into());
    }
    triangulate(&verts, &faces)
}

/// Lee un OBJ: vértices («v x y z») y caras («f 1 2 3», también «1/1/1» e
/// índices negativos, que cuentan desde el último vértice).
pub fn parse_obj(b: &[u8]) -> Result<Mesh, String> {
    let text = String::from_utf8_lossy(b);
    let mut verts: Vec<[f32; 3]> = vec![];
    let mut faces: Vec<Vec<usize>> = vec![];
    for line in text.lines() {
        let mut w = line.split_whitespace();
        match w.next() {
            Some("v") => {
                let v: Vec<f32> = w.take(3).filter_map(|x| x.parse().ok()).collect();
                if v.len() != 3 || !v.iter().all(|x| x.is_finite()) {
                    return Err(format!("Vértice no válido: «{}».", line.trim()));
                }
                verts.push([v[0], v[1], v[2]]);
                if verts.len() > MAX_TRIANGLES * 3 {
                    return Err("El modelo tiene demasiados vértices.".into());
                }
            }
            Some("f") => {
                let mut face = vec![];
                for item in w {
                    let i: i64 = item.split('/').next().unwrap_or("").parse().map_err(|_| format!("Cara no válida: «{}».", line.trim()))?;
                    let idx = if i > 0 { i - 1 } else { verts.len() as i64 + i };
                    if idx < 0 {
                        return Err("Una cara usa un vértice que no existe.".into());
                    }
                    face.push(idx as usize);
                }
                faces.push(face);
            }
            _ => {}
        }
    }
    triangulate(&verts, &faces)
}

/// Vista sombreada (gris, RGBA de `size`×`size`), girada `yaw` y `pitch`
/// grados, con proyección ortogonal y luz desde el observador. Devuelve
/// también los mm que mide un píxel, para la escala.
pub fn render(m: &Mesh, size: usize, yaw: f32, pitch: f32) -> (Vec<u8>, f32) {
    let mut px = vec![0u8; size * size * 4];
    for p in px.as_chunks_mut::<4>().0 {
        p.copy_from_slice(&[245, 247, 250, 255]);
    }
    if m.tris.is_empty() || size == 0 {
        return (px, 0.0);
    }
    let (lo, hi) = m.bounds();
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let s = m.size();
    let extent = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt().max(1e-6);
    let k = size as f32 * 0.9 / extent;
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    let rot = |v: [f32; 3]| -> [f32; 3] {
        let (x, y, z) = (v[0] - c[0], v[1] - c[1], v[2] - c[2]);
        let (x, z) = (x * cy + z * sy, -x * sy + z * cy);
        let (y, z) = (y * cp - z * sp, y * sp + z * cp);
        [x * k + size as f32 / 2.0, size as f32 / 2.0 - y * k, z]
    };
    let mut depth = vec![f32::MIN; size * size];
    for t in &m.tris {
        let [a, b, d] = [rot(t[0]), rot(t[1]), rot(t[2])];
        // normal en la vista: luz de frente
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [d[0] - a[0], d[1] - a[1], d[2] - a[2]]);
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if nl == 0.0 {
            continue;
        }
        let shade = (n[2] / nl).abs();
        let g = (60.0 + 180.0 * shade) as u8;
        let (x0, x1) = (a[0].min(b[0]).min(d[0]).floor().max(0.0) as usize, a[0].max(b[0]).max(d[0]).ceil().min(size as f32 - 1.0).max(0.0) as usize);
        let (y0, y1) = (a[1].min(b[1]).min(d[1]).floor().max(0.0) as usize, a[1].max(b[1]).max(d[1]).ceil().min(size as f32 - 1.0).max(0.0) as usize);
        let area = (b[0] - a[0]) * (d[1] - a[1]) - (b[1] - a[1]) * (d[0] - a[0]);
        if area.abs() < 1e-9 {
            continue;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px_, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((b[0] - px_) * (d[1] - py) - (b[1] - py) * (d[0] - px_)) / area;
                let w1 = ((d[0] - px_) * (a[1] - py) - (d[1] - py) * (a[0] - px_)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a[2] + w1 * b[2] + w2 * d[2];
                let i = y * size + x;
                if z > depth[i] {
                    depth[i] = z;
                    px[i * 4..i * 4 + 4].copy_from_slice(&[g, g, (g as u16 + 8).min(255) as u8, 255]);
                }
            }
        }
    }
    (px, 1.0 / k)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// T-R7-6. Un fichero que pasa del límite no se lee (ni se intenta): un
    /// fichero «hueco» que dice ocupar más de la cuenta sin gastar disco.
    #[test]
    fn a_file_over_the_limit_is_refused_before_reading_it() {
        let p = std::env::temp_dir().join(format!("vd_stl_grande_{}.stl", std::process::id()));
        let f = std::fs::File::create(&p).unwrap();
        f.set_len(MAX_FILE + 1).unwrap();
        drop(f);
        let e = read(&p).unwrap_err();
        let _ = std::fs::remove_file(&p);
        assert!(e.contains("límite"), "{e}");
    }

    #[test]
    fn ply_and_obj_from_intraoral_scanners_are_read_too() {
        // PLY en texto: un cuadrado de 10 mm (una cara de 4 vértices = 2 triángulos), con color
        let ascii = b"ply\nformat ascii 1.0\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0 255 0 0\n10 0 0 255 0 0\n10 10 0 255 0 0\n0 10 0 255 0 0\n4 0 1 2 3\n";
        let m = parse_ply(ascii).unwrap();
        assert_eq!(m.tris.len(), 2);
        assert_eq!(m.size(), [10.0, 10.0, 0.0]);
        // el mismo en binario (little endian), como lo exportan Medit o 3Shape
        let mut bin = b"ply\nformat binary_little_endian 1.0\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nelement face 1\nproperty list uchar uint vertex_indices\nend_header\n".to_vec();
        for v in [[0f32, 0., 0.], [10., 0., 0.], [10., 10., 0.], [0., 10., 0.]] {
            for x in v {
                bin.extend(x.to_le_bytes());
            }
            bin.push(200);
        }
        bin.push(4);
        for i in [0u32, 1, 2, 3] {
            bin.extend(i.to_le_bytes());
        }
        assert_eq!(parse_ply(&bin).unwrap(), m);
        // cortado o con índices que no existen: error, no pánico
        assert!(parse_ply(&bin[..bin.len() - 3]).is_err());
        assert!(parse_ply(b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n3 0 1 2\n").is_err());
        // OBJ, con «v/vt/vn» e índices negativos
        let obj = b"# cuadrado\nv 0 0 0\nv 10 0 0\nv 10 10 0\nv 0 10 0\nvn 0 0 1\nf 1//1 2//1 3//1\nf -4 -2 -1\n";
        let o = parse_obj(obj).unwrap();
        assert_eq!(o.tris.len(), 2);
        assert_eq!(o.size(), [10.0, 10.0, 0.0]);
        assert!(parse_obj(b"v 0 0 0\nf 1 2 3\n").is_err());
        // bytes al azar: nunca un pánico
        let mut seed = 7u32;
        for _ in 0..200 {
            let junk: Vec<u8> = (0..200)
                .map(|_| {
                    seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                    (seed >> 16) as u8
                })
                .collect();
            let mut with_header = b"ply\nformat binary_little_endian 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n".to_vec();
            with_header.extend(&junk);
            let _ = parse_ply(&with_header);
            let _ = parse_obj(&junk);
        }
        // un elemento sin datos que dice tener miles de millones de registros no cuelga el programa
        assert!(parse_ply(b"ply\nformat binary_little_endian 1.0\nelement nada 4000000000\nelement vertex 4000000000\nproperty float x\nend_header\n").is_err());
    }

    /// Un cubo de `l` mm (12 triángulos).
    pub(crate) fn cube(l: f32) -> Vec<[[f32; 3]; 3]> {
        let v = |x: u8, y: u8, z: u8| [x as f32 * l, y as f32 * l, z as f32 * l];
        let q = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]| [[a, b, c], [a, c, d]];
        let mut t = vec![];
        t.extend(q(v(0, 0, 0), v(1, 0, 0), v(1, 1, 0), v(0, 1, 0)));
        t.extend(q(v(0, 0, 1), v(1, 0, 1), v(1, 1, 1), v(0, 1, 1)));
        t.extend(q(v(0, 0, 0), v(1, 0, 0), v(1, 0, 1), v(0, 0, 1)));
        t.extend(q(v(0, 1, 0), v(1, 1, 0), v(1, 1, 1), v(0, 1, 1)));
        t.extend(q(v(0, 0, 0), v(0, 1, 0), v(0, 1, 1), v(0, 0, 1)));
        t.extend(q(v(1, 0, 0), v(1, 1, 0), v(1, 1, 1), v(1, 0, 1)));
        t
    }

    pub(crate) fn binary(tris: &[[[f32; 3]; 3]]) -> Vec<u8> {
        let mut b = vec![0u8; 80];
        b.extend((tris.len() as u32).to_le_bytes());
        for t in tris {
            b.extend([0u8; 12]);
            for v in t {
                for x in v {
                    b.extend(x.to_le_bytes());
                }
            }
            b.extend([0u8; 2]);
        }
        b
    }

    fn ascii(tris: &[[[f32; 3]; 3]]) -> String {
        let mut s = "solid prueba\n".to_string();
        for t in tris {
            s.push_str("facet normal 0 0 0\nouter loop\n");
            for v in t {
                s.push_str(&format!("vertex {} {} {}\n", v[0], v[1], v[2]));
            }
            s.push_str("endloop\nendfacet\n");
        }
        s + "endsolid prueba\n"
    }

    #[test]
    fn binary_and_ascii_models_are_read_and_measured() {
        let c = cube(12.5);
        for m in [parse(&binary(&c)).unwrap(), parse(ascii(&c).as_bytes()).unwrap()] {
            assert_eq!(m.tris.len(), 12);
            assert_eq!(m.size(), [12.5, 12.5, 12.5]);
        }
        let (px, mm) = render(&parse(&binary(&c)).unwrap(), 64, 30.0, 20.0);
        assert_eq!(px.len(), 64 * 64 * 4);
        assert!(px.as_chunks::<4>().0.iter().any(|p| p[0] < 245), "se ve el modelo");
        assert!(mm > 0.0 && mm < 1.0, "la escala: {mm} mm por píxel");
    }

    #[test]
    fn invalid_cut_or_huge_models_are_refused_without_panicking() {
        let good = binary(&cube(10.0));
        assert!(parse(&good[..good.len() - 7]).is_err(), "cortado");
        assert!(parse(b"hola").is_err());
        assert!(parse(b"solid x\nvertex 1 2\nendsolid").is_err());
        assert!(parse(b"solid x\nvertex 1 2 3\nvertex 1 2 3\nendsolid").unwrap_err().contains("cortado"));
        let mut nan = good.clone();
        nan[84 + 12..84 + 16].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(parse(&nan).is_err());
        // una cabecera que dice más triángulos de los que caben: no se reserva
        let mut lie = vec![0u8; 84];
        lie[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&lie).is_err());
        let mut rng = crate::test_rng::Rng(3);
        for _ in 0..2000 {
            let _ = parse(&rng.mutate(&good));
            let n = rng.below(300);
            let _ = parse(&rng.bytes(n));
        }
    }
}
