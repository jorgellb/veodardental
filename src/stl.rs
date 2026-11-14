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

pub fn read(path: &std::path::Path) -> Result<Mesh, String> {
    let len = std::fs::metadata(path).map_err(|e| format!("No se puede leer {}: {e}", path.display()))?.len();
    if len > MAX_FILE {
        return Err(format!("El fichero ocupa {} MB y el límite es {} MB: expórtelo en STL binario o con menos detalle.", len >> 20, MAX_FILE >> 20));
    }
    parse(&std::fs::read(path).map_err(|e| format!("No se puede leer {}: {e}", path.display()))?)
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
