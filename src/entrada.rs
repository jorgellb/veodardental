//! Imágenes que llegan de los aparatos: lo que su programa deja en una
//! carpeta (y lo que mandan por DICOM en red, ver `dicom_red`).
//!
//! Para cada fichero nuevo se decide de qué paciente es:
//! - **Seguro:**
//!   - está en la subcarpeta con su número de historia;
//!   - o es un DICOM con su número de historia (`PatientID`) y su nombre.
//!   - Si el enlace trae lo nuevo solo, entra en su ficha sin preguntar.
//! - **Propuesto:** pide que alguien lo confirme en «Imágenes y enlaces».
//!   - Hay una pista: el nombre del fichero empieza por el número de historia,
//!     o el DICOM tiene el nombre y la fecha de nacimiento de un paciente.
//!   - O la carpeta y el fichero no dicen lo mismo.
//!   - Una imagen en la ficha equivocada es un error clínico, así que lo dudoso
//!     nunca entra solo.
//! - **Sin paciente:** espera a que alguien lo asigne.
//!
//! Un TAC llega como una carpeta de cortes: la carpeta entera es una imagen.
//! Lo que todavía se está escribiendo (cambiado hace menos de [`SETTLE`]) se
//! deja para la siguiente pasada.

use crate::model::{Db, ImageKind, fold};
use chrono::NaiveDate;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Lo que tiene que llevar sin cambios un fichero para leerlo (que el aparato haya terminado de escribirlo).
pub const SETTLE: Duration = Duration::from_secs(8);
/// Ficheros como mucho por carpeta y pasada (una carpeta enorme no para el programa).
const MAX_ENTRIES: usize = 5_000;

/// Un paciente, con lo que sirve para reconocerlo.
#[derive(Clone, Debug, Default)]
pub struct Who {
    pub id: u32,
    pub history_no: u32,
    pub name: String,
    pub nif: String,
    pub birth: Option<NaiveDate>,
}

/// Los pacientes de la clínica, para buscar en otro hilo.
pub fn people(db: &Db) -> Vec<Who> {
    db.patients.iter().map(|p| Who { id: p.id, history_no: p.history_no, name: format!("{} {}", p.first_name, p.last_name), nif: p.nif.clone(), birth: p.birth }).collect()
}

/// Una carpeta donde un aparato deja imágenes.
#[derive(Clone, Debug)]
pub struct Source {
    /// Nombre del enlace (o «Recepción DICOM»).
    pub name: String,
    /// Tipo de imagen del aparato, para los ficheros que no lo dicen.
    pub kind: ImageKind,
    pub base: PathBuf,
    /// Entra solo lo seguro, sin pulsar «Importar».
    pub auto: bool,
    /// La carpeta es de este paciente (la suya, después de abrir un aparato
    /// para él): lo que haya es suyo, salvo que un DICOM diga otra cosa.
    pub patient: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// De este paciente, sin duda.
    Sure(u32),
    /// Seguramente de este paciente; por qué hay que confirmarlo.
    Suggest(u32, String),
    /// No se sabe de quién es (por qué).
    Unknown(String),
}

impl Verdict {
    pub fn patient(&self) -> Option<u32> {
        match self {
            Verdict::Sure(p) | Verdict::Suggest(p, _) => Some(*p),
            Verdict::Unknown(_) => None,
        }
    }
}

/// Una imagen encontrada.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// El fichero, o la carpeta de la serie de un TAC.
    pub path: PathBuf,
    pub kind: ImageKind,
    pub source: String,
    pub auto: bool,
    pub verdict: Verdict,
    /// Lo que dice el propio fichero (paciente y fecha), si lo dice.
    pub label: String,
    /// La fecha del estudio, si el fichero la dice.
    pub date: Option<NaiveDate>,
}

/// Lo que se sabe de un DICOM para reconocer al paciente.
#[derive(Clone, Debug, Default)]
pub struct Tags {
    pub patient_id: String,
    pub name: String,
    pub birth: Option<NaiveDate>,
    pub modality: String,
    pub date: Option<NaiveDate>,
}

impl Tags {
    pub fn of(h: &crate::dicom::Header) -> Tags {
        let day = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y%m%d").ok();
        Tags { patient_id: h.patient_id.trim().to_string(), name: h.patient.clone(), birth: day(&h.birth), modality: h.modality.trim().to_uppercase(), date: day(&h.date) }
    }
}

/// El número de historia con el que empieza un nombre, si lo siguen un
/// separador y algo más («1002_pano.jpg», «1002-03.png»), y no es una fecha.
fn leading_number(stem: &str) -> Option<u32> {
    let digits: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
    let rest = &stem[digits.len()..];
    if digits.is_empty() || digits.len() > 9 || !rest.starts_with(['_', '-', ' ', '.']) {
        return None;
    }
    // 2026-11-15… o 20261115_… son fechas, no pacientes
    let date_like = (digits.len() == 4 && rest.len() >= 3 && rest[1..3].chars().all(|c| c.is_ascii_digit())) || digits.len() == 8;
    if date_like {
        return None;
    }
    digits.parse().ok()
}

/// De quién es un fichero. `base` es la carpeta del aparato y `path`, lo encontrado en ella.
pub fn judge(people: &[Who], base: &Path, path: &Path, tags: Option<&Tags>) -> Verdict {
    let by_no = |n: u32| people.iter().find(|w| w.history_no == n);
    let same = |w: &Who, name: &str| crate::screens::visor3d::same_person(name, &w.name);
    // la subcarpeta con el número de historia (base/1002/…)
    let folder = path.strip_prefix(base).ok().and_then(|rel| rel.components().next()).and_then(|c| c.as_os_str().to_str()).filter(|_| path.parent() != Some(base) || path.is_dir()).and_then(|s| s.parse::<u32>().ok()).and_then(by_no);
    // lo que dice el DICOM
    let dicom_id = tags.and_then(|t| {
        let id = t.patient_id.trim();
        id.parse::<u32>().ok().and_then(by_no).or_else(|| (!id.is_empty()).then(|| people.iter().find(|w| !w.nif.is_empty() && fold(&w.nif) == fold(id))).flatten())
    });
    let dicom_name_ok = |w: &Who| tags.is_none_or(|t| t.name.trim().is_empty() || same(w, &t.name));
    if let Some(w) = folder {
        return match (tags, dicom_id) {
            (Some(_), Some(d)) if d.id != w.id => Verdict::Suggest(w.id, format!("Está en la carpeta de {} pero el fichero dice que es de {}", w.name, d.name)),
            _ if !dicom_name_ok(w) => Verdict::Suggest(w.id, format!("Está en la carpeta de {} pero el fichero dice «{}»", w.name, tags.map(|t| t.name.as_str()).unwrap_or(""))),
            _ => Verdict::Sure(w.id),
        };
    }
    if let Some(w) = dicom_id {
        return if dicom_name_ok(w) { Verdict::Sure(w.id) } else { Verdict::Suggest(w.id, format!("El número de historia es de {}, pero el nombre del fichero es «{}»", w.name, tags.map(|t| t.name.as_str()).unwrap_or(""))) };
    }
    if let Some(t) = tags.filter(|t| !t.name.trim().is_empty()) {
        let hits: Vec<&Who> = people.iter().filter(|w| same(w, &t.name) && (t.birth.is_none() || w.birth.is_none() || t.birth == w.birth)).collect();
        if let [w] = hits.as_slice() {
            return Verdict::Suggest(w.id, format!("El fichero dice «{}»{}: compruebe que es de {}", t.name, t.birth.map(|b| format!(", nacido el {}", b.format("%d/%m/%Y"))).unwrap_or_default(), w.name));
        }
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if let Some(w) = leading_number(stem).and_then(by_no) {
        return Verdict::Suggest(w.id, format!("El nombre del fichero empieza por {}, el número de historia de {}", w.history_no, w.name));
    }
    Verdict::Unknown(match tags {
        Some(t) if !t.name.trim().is_empty() || !t.patient_id.is_empty() => format!("Ningún paciente con el número {} o el nombre «{}»", t.patient_id, t.name),
        _ => "El fichero no dice de qué paciente es".into(),
    })
}

/// En la carpeta de un paciente: suyo, salvo que el DICOM sea de otro.
pub fn judge_in_patient_folder(people: &[Who], pid: u32, tags: Option<&Tags>) -> Verdict {
    let Some(me) = people.iter().find(|w| w.id == pid) else { return Verdict::Unknown("El paciente ya no está.".into()) };
    if let Some(t) = tags {
        let other = t.patient_id.trim().parse::<u32>().ok().and_then(|n| people.iter().find(|w| w.history_no == n)).filter(|w| w.id != pid);
        if let Some(o) = other {
            return Verdict::Suggest(pid, format!("Está en la carpeta de {} pero el fichero dice que es de {}", me.name, o.name));
        }
        if !t.name.trim().is_empty() && !crate::screens::visor3d::same_person(&t.name, &me.name) {
            return Verdict::Suggest(pid, format!("Está en la carpeta de {} pero el fichero dice «{}»", me.name, t.name));
        }
    }
    Verdict::Sure(pid)
}

/// El tipo de imagen: lo que dice el DICOM, luego el nombre del fichero y si no, el del aparato.
pub fn kind_of(path: &Path, tags: Option<&Tags>, default: ImageKind) -> ImageKind {
    let by_modality = tags.and_then(|t| match t.modality.as_str() {
        "CT" => Some(ImageKind::Tac),
        "PX" => Some(ImageKind::Panoramica),
        "IO" => Some(ImageKind::Periapical),
        "XC" | "OP" | "ES" => Some(ImageKind::FotoIntraoral),
        _ => None,
    });
    let name = fold(&path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
    let by_name = if name.contains("pano") || name.contains("opg") || name.contains("orto") {
        Some(ImageKind::Panoramica)
    } else if name.contains("ceph") || name.contains("tele") || name.contains("cefal") {
        Some(ImageKind::Cefalometrica)
    } else if name.contains("foto") || name.contains("intra") || name.contains("photo") {
        Some(ImageKind::FotoIntraoral)
    } else {
        None
    };
    let ext = path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
    let by_ext = match ext.as_str() {
        "stl" | "ply" | "obj" => Some(ImageKind::Escaneo3D),
        "pdf" => Some(ImageKind::Documento),
        _ if path.is_dir() => Some(ImageKind::Tac),
        _ => None,
    };
    by_modality.or(by_ext).or(by_name).unwrap_or(default)
}

/// ¿Es un fichero que puede ir a la galería?
pub fn wanted(path: &Path) -> bool {
    let ext = path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" | "dcm" | "dicom" | "stl" | "ply" | "obj" | "pdf") || (ext.is_empty() && looks_dicom(path))
}

/// Algunos aparatos guardan los DICOM sin extensión: se mira la marca «DICM».
fn looks_dicom(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 132];
    path.is_file() && std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head[128..] == b"DICM"
}

fn is_dicom(path: &Path) -> bool {
    let ext = path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
    ext == "dcm" || ext == "dicom" || (ext.is_empty() && looks_dicom(path))
}

/// ¿Cambió hace poco (el aparato puede estar escribiéndolo todavía)?
fn fresh(path: &Path, now: SystemTime) -> bool {
    std::fs::metadata(path).and_then(|m| m.modified()).map(|t| now.duration_since(t).unwrap_or_default() < SETTLE).unwrap_or(true)
}

/// Una carpeta que es una serie de un TAC: varios DICOM y nada más que lo parezca imagen.
fn is_series(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return false };
    let mut dicoms = 0;
    for e in rd.flatten().take(MAX_ENTRIES) {
        let p = e.path();
        if p.is_dir() {
            return false;
        }
        if is_dicom(&p) {
            dicoms += 1;
        } else if wanted(&p) {
            return false;
        }
    }
    dicoms >= 2
}

/// Busca lo nuevo en las carpetas de los aparatos: la carpeta, sus
/// subcarpetas (las de cada paciente) y las de las series. Se salta lo que ya
/// está en las galerías (`known`) y lo que aún se está escribiendo.
pub fn scan(sources: &[Source], people: &[Who], known: &HashSet<String>, now: SystemTime) -> Vec<Found> {
    let mut out = vec![];
    let mut seen = HashSet::new();
    for src in sources {
        let mut visit: Vec<(PathBuf, u8)> = vec![(src.base.clone(), 0)];
        while let Some((dir, depth)) = visit.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten().take(MAX_ENTRIES) {
                let p = e.path();
                // las carpetas ocultas (como la de lo que se está recibiendo por la red) no se miran
                if e.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let item = if p.is_dir() {
                    if is_series(&p) {
                        p
                    } else {
                        if depth < 2 {
                            visit.push((p, depth + 1));
                        }
                        continue;
                    }
                } else if wanted(&p) {
                    p
                } else {
                    continue;
                };
                let key = item.display().to_string();
                if known.contains(&key) || !seen.insert(key) || fresh(&item, now) {
                    continue;
                }
                // en una serie, la cabecera del primer corte
                let first = if item.is_dir() { std::fs::read_dir(&item).ok().and_then(|rd| rd.flatten().map(|e| e.path()).filter(|p| is_dicom(p)).min()) } else { Some(item.clone()) };
                let tags = first.filter(|f| is_dicom(f)).and_then(|f| crate::dicom::read_header(&f).ok()).map(|h| Tags::of(&h));
                let verdict = match src.patient {
                    Some(pid) => judge_in_patient_folder(people, pid, tags.as_ref()),
                    None => judge(people, &src.base, &item, tags.as_ref()),
                };
                let label = tags.as_ref().map(|t| [t.name.trim().to_string(), t.date.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default()].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" · ")).unwrap_or_default();
                out.push(Found { kind: kind_of(&item, tags.as_ref(), src.kind), path: item, source: src.name.clone(), auto: src.auto, verdict, label, date: tags.and_then(|t| t.date) });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people() -> Vec<Who> {
        vec![
            Who { id: 100, history_no: 1001, name: "Ana García López".into(), nif: "10000000Z".into(), birth: NaiveDate::from_ymd_opt(1985, 1, 1) },
            Who { id: 101, history_no: 1002, name: "Carlos Martínez Sanz".into(), nif: "10731117F".into(), birth: NaiveDate::from_ymd_opt(1972, 2, 2) },
        ]
    }

    fn tags(id: &str, name: &str) -> Tags {
        Tags { patient_id: id.into(), name: name.into(), birth: NaiveDate::from_ymd_opt(1972, 2, 2), modality: "CT".into(), date: None }
    }

    #[test]
    fn only_what_is_certain_goes_into_a_record_by_itself() {
        let w = people();
        let base = Path::new("/aparato");
        // la carpeta con el número de historia
        assert_eq!(judge(&w, base, Path::new("/aparato/1002/pano.jpg"), None), Verdict::Sure(101));
        // un DICOM con su número y su nombre
        assert_eq!(judge(&w, base, Path::new("/aparato/x.dcm"), Some(&tags("1002", "MARTINEZ SANZ CARLOS"))), Verdict::Sure(101));
        // por el DNI también
        assert_eq!(judge(&w, base, Path::new("/aparato/x.dcm"), Some(&tags("10731117F", "Martinez Sanz Carlos"))), Verdict::Sure(101));
        // la carpeta dice uno y el fichero otro: se pregunta
        assert!(matches!(judge(&w, base, Path::new("/aparato/1001/x.dcm"), Some(&tags("1002", "MARTINEZ SANZ CARLOS"))), Verdict::Suggest(100, _)));
        // el número es de un paciente y el nombre de otra persona: se pregunta
        assert!(matches!(judge(&w, base, Path::new("/aparato/x.dcm"), Some(&tags("1002", "PEREZ LUIS"))), Verdict::Suggest(101, _)));
        // solo el nombre y la fecha de nacimiento: se propone
        assert!(matches!(judge(&w, base, Path::new("/aparato/x.dcm"), Some(&tags("", "Martinez Sanz^Carlos"))), Verdict::Suggest(101, _)));
        // el nombre del fichero empieza por un número de historia: se propone
        assert!(matches!(judge(&w, base, Path::new("/aparato/1002_pano.jpg"), None), Verdict::Suggest(101, _)));
        // pero una fecha no es un paciente
        assert!(matches!(judge(&w, base, Path::new("/aparato/2026-11-15.jpg"), None), Verdict::Unknown(_)));
        assert!(matches!(judge(&w, base, Path::new("/aparato/20261115_1.jpg"), None), Verdict::Unknown(_)));
        assert!(matches!(judge(&w, base, Path::new("/aparato/x.dcm"), Some(&tags("77", "NADIE"))), Verdict::Unknown(_)));
    }

    #[test]
    fn in_a_patient_folder_everything_is_theirs_unless_a_dicom_says_otherwise() {
        let w = people();
        assert_eq!(judge_in_patient_folder(&w, 101, None), Verdict::Sure(101));
        assert_eq!(judge_in_patient_folder(&w, 101, Some(&tags("1002", "MARTINEZ SANZ CARLOS"))), Verdict::Sure(101));
        assert!(matches!(judge_in_patient_folder(&w, 101, Some(&tags("1001", "GARCIA LOPEZ ANA"))), Verdict::Suggest(101, _)));
        assert!(matches!(judge_in_patient_folder(&w, 101, Some(&tags("", "PEREZ LUIS"))), Verdict::Suggest(101, _)));
    }

    #[test]
    fn the_type_comes_from_the_file_before_the_device() {
        let t = |m: &str| Tags { modality: m.into(), ..Default::default() };
        assert_eq!(kind_of(Path::new("a.dcm"), Some(&t("CT")), ImageKind::Periapical), ImageKind::Tac);
        assert_eq!(kind_of(Path::new("a.dcm"), Some(&t("PX")), ImageKind::Periapical), ImageKind::Panoramica);
        assert_eq!(kind_of(Path::new("pano_1.jpg"), None, ImageKind::Periapical), ImageKind::Panoramica);
        assert_eq!(kind_of(Path::new("scan.ply"), None, ImageKind::FotoIntraoral), ImageKind::Escaneo3D);
        // lo que no dice nada, del tipo del aparato (una cámara hace fotos)
        assert_eq!(kind_of(Path::new("IMG_0001.jpg"), None, ImageKind::FotoIntraoral), ImageKind::FotoIntraoral);
    }

    /// Un DICOM mínimo (cabecera y 2×2 píxeles) con el paciente dado.
    fn dicom(id: &str, name: &str) -> Vec<u8> {
        let el = |b: &mut Vec<u8>, g: u16, e: u16, vr: &str, v: &[u8]| {
            let mut v = v.to_vec();
            if v.len() % 2 == 1 {
                v.push(b' ');
            }
            b.extend(g.to_le_bytes());
            b.extend(e.to_le_bytes());
            b.extend(vr.as_bytes());
            if matches!(vr, "OB" | "OW") {
                b.extend([0, 0]);
                b.extend((v.len() as u32).to_le_bytes());
            } else {
                b.extend((v.len() as u16).to_le_bytes());
            }
            b.extend(v);
        };
        let mut b = vec![0u8; 128];
        b.extend(b"DICM");
        el(&mut b, 0x0002, 0x0010, "UI", b"1.2.840.10008.1.2.1\0");
        el(&mut b, 0x0008, 0x0060, "CS", b"CT");
        el(&mut b, 0x0010, 0x0010, "PN", name.as_bytes());
        el(&mut b, 0x0010, 0x0020, "LO", id.as_bytes());
        el(&mut b, 0x0010, 0x0030, "DA", b"19720202");
        el(&mut b, 0x0028, 0x0010, "US", &2u16.to_le_bytes());
        el(&mut b, 0x0028, 0x0011, "US", &2u16.to_le_bytes());
        el(&mut b, 0x0028, 0x0100, "US", &16u16.to_le_bytes());
        el(&mut b, 0x7FE0, 0x0010, "OW", &[0u8; 8]);
        b
    }

    #[test]
    fn a_device_folder_is_read_without_waiting_for_files_still_being_written() {
        let dir = std::env::temp_dir().join(format!("vd_entrada_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("1002")).unwrap();
        std::fs::create_dir_all(dir.join("serie")).unwrap();
        std::fs::write(dir.join("1002").join("pano.jpg"), b"jpg").unwrap();
        std::fs::write(dir.join("suelto.dcm"), dicom("1002", "MARTINEZ SANZ^CARLOS")).unwrap();
        std::fs::write(dir.join("serie").join("1"), dicom("1001", "GARCIA LOPEZ^ANA")).unwrap();
        std::fs::write(dir.join("serie").join("2"), dicom("1001", "GARCIA LOPEZ^ANA")).unwrap();
        std::fs::write(dir.join("notas.txt"), b"no").unwrap();
        // lo que se está recibiendo por la red, aunque parezca un DICOM, no se toma
        std::fs::create_dir_all(dir.join(".recibiendo")).unwrap();
        std::fs::write(dir.join(".recibiendo").join("1-0"), dicom("1002", "MARTINEZ SANZ^CARLOS")).unwrap();
        std::fs::write(dir.join("a_medias.dcm.parcial"), dicom("1002", "MARTINEZ SANZ^CARLOS")).unwrap();
        let src = Source { name: "TAC".into(), kind: ImageKind::Periapical, base: dir.clone(), auto: true, patient: None };
        // recién escritos: todavía no
        assert!(scan(std::slice::from_ref(&src), &people(), &HashSet::new(), SystemTime::now()).is_empty());
        let later = SystemTime::now() + SETTLE * 2;
        let found = scan(std::slice::from_ref(&src), &people(), &HashSet::new(), later);
        let by = |end: &str| found.iter().find(|f| f.path.ends_with(end)).unwrap_or_else(|| panic!("falta {end}: {found:?}"));
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(by("pano.jpg").verdict, Verdict::Sure(101));
        assert_eq!(by("pano.jpg").kind, ImageKind::Panoramica);
        assert_eq!(by("suelto.dcm").verdict, Verdict::Sure(101));
        assert_eq!(by("suelto.dcm").kind, ImageKind::Tac);
        // la serie de cortes es una sola imagen (sus ficheros no tienen extensión)
        assert_eq!(by("serie").verdict, Verdict::Sure(100));
        assert_eq!(by("serie").label, "GARCIA LOPEZ ANA");
        // lo que ya está en una galería no se vuelve a proponer
        let known: HashSet<String> = found.iter().map(|f| f.path.display().to_string()).collect();
        assert!(scan(&[src], &people(), &known, later).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
