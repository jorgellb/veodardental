//! Lista oficial de medicamentos: el «Nomenclátor de prescripción» que
//! publica la AEMPS (listadomedicamentos.aemps.gob.es/prescripcion.zip, se
//! actualiza a diario). De cada presentación comercializada se guarda el
//! código nacional, el código DCPF del principio activo, la forma, la dosis,
//! la vía, el envase y si es estupefaciente o psicótropo: lo necesario para
//! la receta oficial sin teclear códigos.
//!
//! No se incluye con el programa: se descarga desde Configuración y se guarda
//! junto a los datos de la clínica.

use crate::model::fold;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;

pub const URL: &str = "https://listadomedicamentos.aemps.gob.es/prescripcion.zip";

/// Días a partir de los cuales se avisa de que la lista oficial cargada es antigua.
pub const STALE_DAYS: i64 = 60;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Medicine {
    /// Código nacional (6 cifras, sin el dígito de control).
    pub cn: String,
    pub name: String,
    pub dcpf: String,
    pub dcpf_name: String,
    pub dosage: String,
    pub pack: String,
    pub form: String,
    pub route: String,
    pub narcotic: bool,
    pub psychotropic: bool,
    pub generic: bool,
    pub atc: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Nomenclator {
    /// Fecha de la lista (cabecera del fichero oficial).
    pub date: String,
    pub medicines: Vec<Medicine>,
    /// Texto de búsqueda de cada medicamento (se calcula al cargar).
    #[serde(skip)]
    keys: Vec<String>,
}

/// Dígito de control del código nacional: el del EAN-13 «847000» + CN.
/// (Ejemplo oficial del DataMatrix: 818872 → 8188727.)
pub fn cn_check_digit(cn6: &str) -> Option<u32> {
    let digits: Vec<u32> = format!("847000{cn6}").chars().map(|c| c.to_digit(10)).collect::<Option<Vec<_>>>()?;
    if digits.len() != 12 {
        return None;
    }
    let sum: u32 = digits.iter().enumerate().map(|(i, d)| if i % 2 == 0 { *d } else { d * 3 }).sum();
    Some((10 - sum % 10) % 10)
}

/// Código nacional de 7 cifras (con dígito de control) a partir del de 6.
pub fn cn7(cn: &str) -> String {
    let c: String = cn.chars().filter(|c| c.is_ascii_digit()).collect();
    match c.len() {
        6 => cn_check_digit(&c).map(|d| format!("{c}{d}")).unwrap_or(c),
        _ => c,
    }
}

/// Las 6 primeras cifras: para comparar un código con o sin dígito de control.
pub fn cn6(cn: &str) -> String {
    cn.chars().filter(|c| c.is_ascii_digit()).take(6).collect()
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&").trim().to_string()
}

/// Texto de la primera etiqueta `<tag>…</tag>` dentro de un bloque.
fn tag<'a>(block: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let i = block.find(&open)? + open.len();
    let j = block[i..].find(&format!("</{name}>"))? + i;
    Some(&block[i..j])
}

/// Bloques `<record>…</record>` de un fichero XML de la AEMPS.
fn blocks<'a>(xml: &'a str, record: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{record}>"), format!("</{record}>"));
    let mut out = vec![];
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        let Some(j) = after.find(&close) else { break };
        out.push(&after[..j]);
        rest = &after[j + close.len()..];
    }
    out
}

fn dictionary(xml: &str, record: &str, key: &str, value: &str) -> HashMap<String, String> {
    blocks(xml, record).into_iter().filter_map(|b| Some((tag(b, key)?.trim().to_string(), unescape(tag(b, value)?)))).collect()
}

fn lower_first(s: &str) -> String {
    s.to_lowercase()
}

/// Lee el contenido de los ficheros oficiales (ya descomprimidos).
pub fn parse(prescripcion: &str, formas: &str, vias: &str, dcpf: &str) -> Nomenclator {
    let formas = dictionary(formas, "formasfarmaceuticas", "codigoformafarmaceutica", "formafarmaceutica");
    let vias = dictionary(vias, "viasadministracion", "codigoviaadministracion", "viaadministracion");
    let dcpf = dictionary(dcpf, "dcpf", "codigodcpf", "nombredcpf");
    let date = tag(prescripcion, "listprescriptiondate").unwrap_or_default().trim().to_string();
    let flag = |b: &str, t: &str| tag(b, t).map(|x| x.trim() == "1").unwrap_or(false);
    let mut medicines: Vec<Medicine> = blocks(prescripcion, "prescription")
        .into_iter()
        .filter(|b| flag(b, "sw_comercializado") && !flag(b, "sw_uso_hospitalario") && !flag(b, "sw_envase_clinico"))
        .filter_map(|b| {
            let cn = tag(b, "cod_nacion")?.trim().to_string();
            let code_dcpf = tag(b, "cod_dcpf").unwrap_or_default().trim().to_string();
            Some(Medicine {
                name: unescape(tag(b, "des_prese").or_else(|| tag(b, "des_nomco"))?),
                dcpf_name: dcpf.get(&code_dcpf).cloned().unwrap_or_default(),
                dcpf: code_dcpf,
                dosage: unescape(tag(b, "des_dosific").unwrap_or_default()),
                pack: unescape(tag(b, "nro_conte").unwrap_or_default()),
                form: tag(b, "cod_forfar").and_then(|c| formas.get(c.trim())).map(|f| lower_first(f)).unwrap_or_default(),
                route: tag(b, "cod_via_admin").and_then(|c| vias.get(c.trim())).map(|f| lower_first(f)).unwrap_or_default(),
                narcotic: flag(b, "sw_estupefaciente"),
                psychotropic: flag(b, "sw_psicotropo"),
                generic: flag(b, "sw_generico"),
                atc: tag(b, "cod_atc").unwrap_or_default().trim().to_string(),
                cn,
            })
        })
        .collect();
    medicines.sort_by(|a, b| a.name.cmp(&b.name));
    let mut n = Nomenclator { date, medicines, keys: vec![] };
    n.index();
    n
}

/// Lee el `prescripcion.zip` oficial.
pub fn read_zip(bytes: &[u8]) -> Result<Nomenclator, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("El fichero no es el zip de la AEMPS: {e}"))?;
    let mut get = |name: &str| -> Result<String, String> {
        let mut f = z.by_name(name).map_err(|_| format!("Falta {name} en el zip de la AEMPS."))?;
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| e.to_string())?;
        Ok(s)
    };
    let p = get("Prescripcion.xml")?;
    let f = get("DICCIONARIO_FORMA_FARMACEUTICA.xml")?;
    let v = get("DICCIONARIO_VIAS_ADMINISTRACION.xml")?;
    let d = get("DICCIONARIO_DCPF.xml").unwrap_or_default();
    let n = parse(&p, &f, &v, &d);
    if n.medicines.is_empty() {
        return Err("El fichero de la AEMPS no trae medicamentos comercializados.".into());
    }
    Ok(n)
}

/// Descarga la lista oficial de la AEMPS.
pub fn download() -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(15)).timeout(std::time::Duration::from_secs(300)).build();
    // el servidor de la AEMPS rechaza los programas que no se presentan como navegador
    let r = agent.get(URL).set("User-Agent", "Mozilla/5.0 (VeodarDental; nomenclator)").call().map_err(|e| format!("No se pudo descargar la lista de la AEMPS: {e}"))?;
    let mut buf = vec![];
    r.into_reader().take(200 * 1024 * 1024).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

pub fn path() -> PathBuf {
    crate::model::Db::path().with_file_name("nomenclator.json")
}

pub fn save(n: &Nomenclator) -> Result<(), String> {
    let p = path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, serde_json::to_string(n).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

pub fn load() -> Option<Nomenclator> {
    let mut n: Nomenclator = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok())?;
    n.index();
    Some(n)
}

impl Nomenclator {
    /// Días que tiene la lista cargada (`None`: fecha ilegible).
    pub fn age_days(&self, today: chrono::NaiveDate) -> Option<i64> {
        let d = chrono::NaiveDate::parse_from_str(self.date.trim(), "%Y-%m-%d").ok()?;
        Some((today - d).num_days())
    }

    /// Lista con más de 60 días (o sin fecha legible): conviene descargar la
    /// actualizada de la AEMPS antes de recetar con ella.
    pub fn stale(&self, today: chrono::NaiveDate) -> bool {
        self.age_days(today).is_none_or(|d| d > STALE_DAYS)
    }

    /// Prepara la búsqueda rápida (una vez, al cargar).
    pub fn index(&mut self) {
        self.keys = self.medicines.iter().map(|m| fold(&format!("{} {} {}", m.name, m.dcpf_name, m.cn))).collect();
    }

    /// Busca por nombre, principio activo o código nacional (las palabras en cualquier orden).
    pub fn search(&self, q: &str, max: usize) -> Vec<&Medicine> {
        let words: Vec<String> = fold(q).split_whitespace().map(|w| w.to_string()).collect();
        if words.is_empty() {
            return vec![];
        }
        self.medicines
            .iter()
            .enumerate()
            .filter(|(i, m)| {
                let own;
                let hay = match self.keys.get(*i) {
                    Some(k) => k.as_str(),
                    None => {
                        own = fold(&format!("{} {} {}", m.name, m.dcpf_name, m.cn));
                        own.as_str()
                    }
                };
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .map(|(_, m)| m)
            .take(max)
            .collect()
    }
}

/// Línea de receta con los datos oficiales del medicamento.
pub fn rx_item(m: &Medicine) -> crate::finance::RxItem {
    let unit = m.form.split_whitespace().next().unwrap_or("unidad").to_string();
    crate::finance::RxItem {
        drug: m.name.clone(),
        dose: format!("1 {unit}"),
        frequency: "cada 8 horas".into(),
        days: 5,
        units: 1,
        cn: m.cn.clone(),
        active_code: m.dcpf.clone(),
        form: m.form.clone(),
        dosage: m.dosage.clone(),
        route: m.route.clone(),
        pack: m.pack.clone(),
        take: 1.0,
        take_unit: unit,
        per: 3.0,
        per_unit: "día".into(),
        narcotic: m.narcotic,
        psychotropic: m.psychotropic,
        atc: m.atc.clone(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La lista oficial se actualiza: si la cargada tiene más de 60 días, se avisa.
    #[test]
    fn an_old_official_list_is_flagged() {
        let n = Nomenclator { date: "2026-08-01".into(), ..Default::default() };
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();
        assert_eq!(n.age_days(today), Some(104));
        assert!(n.stale(today));
        let fresh = Nomenclator { date: "2026-11-06".into(), ..Default::default() };
        assert!(!fresh.stale(today));
        assert!(Nomenclator { date: "sin fecha".into(), ..Default::default() }.stale(today), "sin fecha legible también se avisa");
    }

    const P: &str = r#"<?xml version="1.0" encoding="UTF-8"?><aemps_prescripcion><header><listprescriptiondate>2026-11-06</listprescriptiondate></header>
<prescription><cod_nacion>654321</cod_nacion><des_nomco>IBUPROFENO EJEMPLO 600 mg</des_nomco><des_prese>IBUPROFENO EJEMPLO 600 mg COMPRIMIDOS RECUBIERTOS EFG, 40 comprimidos</des_prese><cod_dcpf>111</cod_dcpf><des_dosific>600 mg</des_dosific><nro_conte>40 comprimidos</nro_conte><sw_psicotropo>0</sw_psicotropo><sw_estupefaciente>0</sw_estupefaciente><sw_generico>1</sw_generico><sw_envase_clinico>0</sw_envase_clinico><sw_uso_hospitalario>0</sw_uso_hospitalario><sw_comercializado>1</sw_comercializado><formasfarmaceuticas><cod_forfar>5</cod_forfar><viasadministracion><cod_via_admin>48</cod_via_admin></viasadministracion></formasfarmaceuticas><atc><cod_atc>M01AE01</cod_atc></atc></prescription>
<prescription><cod_nacion>700001</cod_nacion><des_prese>TRAMADOL &amp; PARACETAMOL EJEMPLO, 20 comprimidos</des_prese><cod_dcpf>222</cod_dcpf><sw_psicotropo>1</sw_psicotropo><sw_estupefaciente>0</sw_estupefaciente><sw_comercializado>1</sw_comercializado><formasfarmaceuticas><cod_forfar>5</cod_forfar></formasfarmaceuticas></prescription>
<prescription><cod_nacion>700002</cod_nacion><des_prese>NO COMERCIALIZADO</des_prese><sw_comercializado>0</sw_comercializado></prescription>
<prescription><cod_nacion>700003</cod_nacion><des_prese>SOLO HOSPITAL</des_prese><sw_comercializado>1</sw_comercializado><sw_uso_hospitalario>1</sw_uso_hospitalario></prescription>
</aemps_prescripcion>"#;
    const F: &str = "<x><formasfarmaceuticas><codigoformafarmaceutica>5</codigoformafarmaceutica><formafarmaceutica>COMPRIMIDO RECUBIERTO CON PELÍCULA</formafarmaceutica></formasfarmaceuticas></x>";
    const V: &str = "<x><viasadministracion><codigoviaadministracion>48</codigoviaadministracion><viaadministracion>VÍA ORAL</viaadministracion></viasadministracion></x>";
    const D: &str = "<x><dcpf><codigodcpf>111</codigodcpf><nombredcpf>Ibuprofeno 600 mg 40 comprimidos</nombredcpf></dcpf></x>";

    #[test]
    fn national_code_check_digit() {
        assert_eq!(cn7("818872"), "8188727", "ejemplo del DataMatrix oficial");
        assert_eq!(cn7("654321"), "6543214");
        assert_eq!(cn7("8188727"), "8188727");
        assert_eq!(cn6("8188727"), "818872");
    }

    #[test]
    fn reads_marketed_medicines_with_their_codes() {
        let n = parse(P, F, V, D);
        assert_eq!(n.date, "2026-11-06");
        assert_eq!(n.medicines.len(), 2, "sin los no comercializados ni los de hospital");
        let ibu = n.medicines.iter().find(|m| m.cn == "654321").unwrap();
        assert_eq!((ibu.form.as_str(), ibu.route.as_str(), ibu.dosage.as_str(), ibu.pack.as_str()), ("comprimido recubierto con película", "vía oral", "600 mg", "40 comprimidos"));
        assert_eq!(ibu.dcpf_name, "Ibuprofeno 600 mg 40 comprimidos");
        assert!(ibu.generic && !ibu.narcotic);
        assert!(n.medicines.iter().any(|m| m.name.starts_with("TRAMADOL & PARACETAMOL") && m.psychotropic));
        assert_eq!(n.search("600 ibuprofeno", 10).len(), 1);
        assert_eq!(n.search("654321", 10).len(), 1);
        assert!(n.search("", 10).is_empty());
        let it = rx_item(ibu);
        assert_eq!((it.cn.as_str(), it.active_code.as_str(), it.take_unit.as_str()), ("654321", "111", "comprimido"));
    }

    #[test]
    fn reads_the_official_zip() {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            for (name, body) in [("Prescripcion.xml", P), ("DICCIONARIO_FORMA_FARMACEUTICA.xml", F), ("DICCIONARIO_VIAS_ADMINISTRACION.xml", V), ("DICCIONARIO_DCPF.xml", D)] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        let n = read_zip(buf.get_ref()).unwrap();
        assert_eq!(n.medicines.len(), 2);
        assert!(read_zip(b"no es un zip").is_err());
    }

    /// Con la lista real: DS_NOMENCLATOR=/ruta/prescripcion.zip
    #[test]
    #[ignore]
    fn real_official_list() {
        let Ok(p) = std::env::var("DS_NOMENCLATOR") else { return };
        let n = read_zip(&std::fs::read(p).unwrap()).unwrap();
        println!("{} medicamentos · lista del {}", n.medicines.len(), n.date);
        let r = n.search("amoxicilina 500 capsulas", 5);
        assert!(!r.is_empty());
        for m in r {
            println!("{} · {} · {} · {} · {}", m.cn, m.name, m.form, m.route, m.dcpf_name);
        }
        assert!(n.medicines.iter().any(|m| m.narcotic));
    }
}
