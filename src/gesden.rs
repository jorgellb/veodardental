//! Importación del catálogo de tratamientos y de las tarifas de GESDEN.
//!
//! Los datos se leen del SQL Server de la clínica con [`crate::mssql`]:
//! tablas `TTos` (tratamientos de cada tarifa), `TGrupos` (grupos),
//! `TTipoOdg` (cómo se dibuja en el odontograma), `TTipoIVA` y `Clientes` de
//! tipo «M» (tarifas y mutuas). El programa no lleva datos de ninguna clínica.
//!
//! En GESDEN cada tarifa tiene su propia lista de tratamientos. Aquí la
//! tarifa con más pacientes pasa a ser el catálogo general («Privado»); en
//! las demás, los tratamientos con el mismo código son precios propios de
//! esa tarifa y los que solo existen en ella se añaden como tratamientos
//! exclusivos de la tarifa (no aparecen a los pacientes de otras tarifas).

use crate::model::*;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct GCatalogo {
    #[serde(default)]
    pub origen: String,
    pub tarifas: Vec<GTarifa>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GTarifa {
    pub id: i64,
    #[serde(default)]
    pub codigo: String,
    pub nombre: String,
    #[serde(default)]
    pub inactiva: u8,
    #[serde(default)]
    pub telefono: String,
    #[serde(default)]
    pub pacientes: u32,
    #[serde(default)]
    pub tratamientos: Vec<GTto>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GTto {
    pub codigo: String,
    pub desc_medico: String,
    #[serde(default)]
    pub grupo: String,
    #[serde(default)]
    pub tipo_odg: String,
    #[serde(default)]
    pub precio: f64,
    #[serde(default)]
    pub minutos: u32,
    /// 0 = acto por diente, 1 = acto por tratamiento, 2 = tratamiento genérico.
    #[serde(default)]
    pub piezas: u8,
    #[serde(default)]
    pub inactivo: u8,
}

impl GTarifa {
    pub fn active(&self) -> bool {
        self.inactiva == 0
    }
    pub fn active_treatments(&self) -> usize {
        self.tratamientos.iter().filter(|t| t.inactivo == 0).count()
    }
}

#[cfg(test)]
pub fn parse(json: &str) -> Result<GCatalogo, String> {
    serde_json::from_str(json).map_err(|e| format!("El catálogo de GESDEN no se puede leer: {e}"))
}

/// Cómo dibuja el odontograma cada tipo de GESDEN (`TTipoOdg.Descripcio`).
pub fn odg_from_gesden(tipo: &str) -> OdgKind {
    let t = fold(tipo.trim());
    match t.as_str() {
        "endodoncia" => OdgKind::Endodoncia,
        "obturaciones" | "obturacion" => OdgKind::Obturacion,
        "apicectomia" => OdgKind::Apicectomia,
        "implante" => OdgKind::Implante,
        "perno-munon" | "perno munon" => OdgKind::PernoMunon,
        "corona" => OdgKind::Corona,
        "puente" => OdgKind::Puente,
        "protesis removible" => OdgKind::ProtesisRemovible,
        "sellado" => OdgKind::Sellador,
        "carilla" => OdgKind::Carilla,
        "raspado" | "higiene" => OdgKind::Higiene,
        "extraccion corona" | "extraccion pieza" => OdgKind::Extraccion,
        "brackets" | "bandas" => OdgKind::Bracket,
        _ => OdgKind::Ninguno,
    }
}

/// Palabras que se escriben con tilde o en mayúsculas al pasar los textos de
/// GESDEN (todo en mayúsculas) a mayúscula inicial.
const ACCENTS: &[(&str, &str)] = &[
    ("ingles", "inglés"),
    ("cirugia", "cirugía"),
    ("protesis", "prótesis"),
    ("estetica", "estética"),
    ("radiologia", "radiología"),
    ("odontologia", "odontología"),
    ("implantologia", "implantología"),
    ("oclusion", "oclusión"),
    ("diagnosticas", "diagnósticas"),
    ("diagnostico", "diagnóstico"),
    ("odontopediatria", "odontopediatría"),
    ("exodoncias", "exodoncias"),
    ("sesion", "sesión"),
    ("clinica", "clínica"),
    ("acrilica", "acrílica"),
    ("acrilico", "acrílico"),
    ("esqueletica", "esquelética"),
    ("esqueletico", "esquelético"),
    ("metalica", "metálica"),
    ("metalico", "metálico"),
    ("ceramica", "cerámica"),
    ("ceramico", "cerámico"),
    ("panoramica", "panorámica"),
    ("tecnica", "técnica"),
    ("periapical", "periapical"),
    ("revision", "revisión"),
    ("extraccion", "extracción"),
    ("reconstruccion", "reconstrucción"),
    ("obturacion", "obturación"),
    ("apicectomia", "apicectomía"),
    ("ferula", "férula"),
    ("munon", "muñón"),
    ("composicion", "composición"),
    ("articulacion", "articulación"),
    ("tartrectomia", "tartrectomía"),
];

const UPPER_WORDS: [&str; 12] = ["atm", "tac", "cbct", "rx", "cad", "cam", "cad/cam", "pfm", "dvd", "ii", "iii", "iv"];

/// «BLANQUEAMIENTO EN CLÍNICA» → «Blanqueamiento en clínica». Respeta las
/// palabras con cifras (2X4, 3D) y siglas conocidas (ATM, TAC…).
pub fn pretty_text(s: &str) -> String {
    let s = s.trim();
    let letters: Vec<char> = s.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() || letters.iter().any(|c| c.is_lowercase()) {
        // ya está escrito con minúsculas: se respeta tal cual
        return s.to_string();
    }
    let mut out = Vec::new();
    for (i, w) in s.split(' ').enumerate() {
        if w.is_empty() {
            continue;
        }
        let lower = w.to_lowercase();
        let bare = fold(&lower);
        let word = if w.chars().any(|c| c.is_ascii_digit()) || UPPER_WORDS.contains(&bare.trim_matches(|c: char| !c.is_alphanumeric() && c != '/')) {
            w.to_string()
        } else {
            // tildes que faltan en el original
            let core = bare.trim_matches(|c: char| !c.is_alphanumeric());
            match ACCENTS.iter().find(|(k, _)| *k == core) {
                Some((k, v)) if lower.contains(k) => lower.replacen(k, v, 1),
                _ => lower,
            }
        };
        out.push(if i == 0 || out.is_empty() { capitalize(&word) } else { word });
    }
    out.join(" ")
}

fn capitalize(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImportOptions {
    /// Los tratamientos que ya había y no están en GESDEN dejan de ofrecerse
    /// (siguen en la historia de los pacientes).
    pub replace_catalog: bool,
    /// También las tarifas y mutuas dadas de baja en GESDEN.
    pub include_inactive_tariffs: bool,
    /// Pasar los textos en mayúsculas a mayúscula inicial con tildes.
    pub pretty: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions { replace_catalog: true, include_inactive_tariffs: false, pretty: true }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    pub base_tariff: String,
    pub treatments_added: usize,
    pub treatments_updated: usize,
    pub treatments_hidden: usize,
    pub tariffs_added: usize,
    pub tariffs_updated: usize,
    pub tariff_prices: usize,
    pub skipped_inactive_tariffs: usize,
}

impl ImportReport {
    pub fn summary(&self) -> String {
        format!(
            "Catálogo de GESDEN importado: {} tratamientos nuevos y {} actualizados (tarifa base «{}»), {} tarifas nuevas y {} actualizadas con {} precios propios{}{}.",
            self.treatments_added,
            self.treatments_updated,
            self.base_tariff,
            self.tariffs_added,
            self.tariffs_updated,
            self.tariff_prices,
            if self.treatments_hidden > 0 { format!("; {} tratamientos anteriores dejan de ofrecerse (siguen en la historia)", self.treatments_hidden) } else { String::new() },
            if self.skipped_inactive_tariffs > 0 { format!("; {} tarifas dadas de baja no se han traído", self.skipped_inactive_tariffs) } else { String::new() }
        )
    }
}

fn marker(id: i64) -> String {
    format!("[GESDEN #{id}]")
}

/// La tarifa general: la activa con más pacientes.
pub fn base_tariff(cat: &GCatalogo) -> Option<&GTarifa> {
    cat.tarifas.iter().filter(|t| t.active() && !t.tratamientos.is_empty()).max_by_key(|t| (t.pacientes, t.tratamientos.len()))
}

/// Un tratamiento por código: el activo antes que el dado de baja; si hay
/// dos activos con el mismo código, el segundo recibe un sufijo.
fn dedup(list: &[GTto]) -> Vec<(String, GTto)> {
    let mut sorted: Vec<&GTto> = list.iter().collect();
    sorted.sort_by_key(|t| (t.codigo.trim().to_uppercase(), t.inactivo));
    let mut out: Vec<(String, GTto)> = vec![];
    for t in sorted {
        let code = t.codigo.trim().to_string();
        if code.is_empty() {
            continue;
        }
        match out.iter().filter(|(c, _)| c.split('~').next() == Some(code.as_str())).count() {
            0 => out.push((code, t.clone())),
            n if t.inactivo == 0 => out.push((format!("{code}~{}", n + 1), t.clone())),
            _ => {} // repetido y dado de baja: no aporta nada
        }
    }
    out
}

fn tariff_name(t: &GTarifa) -> String {
    let n = t.nombre.trim().trim_matches('-').trim();
    if n.is_empty() { format!("Tarifa {}", t.id) } else { n.to_string() }
}

fn is_insurer(t: &GTarifa) -> bool {
    let n = fold(&t.nombre);
    !(n.contains("tarifa") || n.contains("tratamientos") || n.contains("ingles") || n.contains("privad"))
}

impl Db {
    /// Trae el catálogo y las tarifas de GESDEN. Se puede repetir: lo ya
    /// importado se actualiza sin duplicarse.
    pub fn import_gesden_catalog(&mut self, cat: &GCatalogo, opt: ImportOptions) -> Result<ImportReport, String> {
        let base = base_tariff(cat).ok_or("El catálogo de GESDEN no tiene ninguna tarifa activa con tratamientos.")?.clone();
        let text = |s: &str| if opt.pretty { pretty_text(s) } else { s.trim().to_string() };
        let mut r = ImportReport { base_tariff: tariff_name(&base), ..Default::default() };
        let mut imported_codes: Vec<String> = vec![];

        // 1. tarifa general → catálogo
        for (code, g) in dedup(&base.tratamientos) {
            let odg = odg_from_gesden(&g.tipo_odg);
            let t = Treatment {
                code: code.clone(),
                description: text(&g.desc_medico),
                group: if g.grupo.trim().is_empty() { "Varios".into() } else { text(&g.grupo) },
                price: round2(g.precio.max(0.0)),
                minutes: if g.minutos > 0 { g.minutos } else { 30 },
                odg,
                per_tooth: g.piezas == 0,
                recall_months: if odg == OdgKind::Higiene { 6 } else { 0 },
                active: g.inactivo == 0,
                only_tariff: None,
            };
            match self.treatments.iter_mut().find(|x| x.code == code) {
                Some(x) => {
                    let recall = x.recall_months;
                    *x = t;
                    if recall > 0 {
                        x.recall_months = recall;
                    }
                    r.treatments_updated += 1;
                }
                None => {
                    self.treatments.push(t);
                    r.treatments_added += 1;
                }
            }
            imported_codes.push(code);
        }
        if opt.replace_catalog {
            for t in self.treatments.iter_mut() {
                if t.active && t.odg != OdgKind::Ausente && t.only_tariff.is_none() && !imported_codes.contains(&t.code) {
                    t.active = false;
                    r.treatments_hidden += 1;
                }
            }
        }
        if let Some(t) = self.tariffs.iter_mut().find(|t| t.id == 1) && !t.notes.contains(&marker(base.id)) {
            t.notes = format!("{} {}", marker(base.id), t.notes.trim()).trim().to_string();
        }

        // 2. el resto de tarifas
        for g in cat.tarifas.iter().filter(|t| t.id != base.id) {
            if !g.active() && !opt.include_inactive_tariffs {
                r.skipped_inactive_tariffs += 1;
                continue;
            }
            let tag = marker(g.id);
            let tid = match self.tariffs.iter().find(|t| t.notes.contains(&tag)) {
                Some(t) => {
                    r.tariffs_updated += 1;
                    t.id
                }
                None => {
                    let id = self.tariffs.iter().map(|t| t.id).max().unwrap_or(1) + 1;
                    self.tariffs.push(Tariff {
                        id,
                        name: text(&tariff_name(g)),
                        is_insurer: is_insurer(g),
                        discount_pct: 0.0,
                        prices: vec![],
                        phone: g.telefono.trim().to_string(),
                        notes: format!("{tag} Importada de GESDEN{}", if g.codigo.trim().is_empty() { String::new() } else { format!(" (código {})", g.codigo.trim()) }),
                        valid_from: today(),
                        valid_to: None,
                        inactive: !g.active(),
                    });
                    r.tariffs_added += 1;
                    id
                }
            };
            for (code, gt) in dedup(&g.tratamientos) {
                let shared = self.treatments.iter().any(|t| t.code == code && t.only_tariff.is_none());
                if !shared {
                    let odg = odg_from_gesden(&gt.tipo_odg);
                    let t = Treatment {
                        code: code.clone(),
                        description: text(&gt.desc_medico),
                        group: if gt.grupo.trim().is_empty() { "Varios".into() } else { text(&gt.grupo) },
                        price: round2(gt.precio.max(0.0)),
                        minutes: if gt.minutos > 0 { gt.minutos } else { 30 },
                        odg,
                        per_tooth: gt.piezas == 0,
                        recall_months: 0,
                        active: gt.inactivo == 0,
                        only_tariff: Some(tid),
                    };
                    match self.treatments.iter_mut().find(|x| x.code == code) {
                        Some(x) if x.only_tariff == Some(tid) => *x = t,
                        Some(_) => continue, // código de otra tarifa exclusiva: no se pisa
                        None => self.treatments.push(t),
                    }
                }
                if gt.inactivo == 0 && let Some(t) = self.tariffs.iter_mut().find(|t| t.id == tid) {
                    let price = round2(gt.precio.max(0.0));
                    match t.prices.iter_mut().find(|p| p.code == code) {
                        Some(p) => p.price = price,
                        None => t.prices.push(TariffPrice { code: code.clone(), price, insurer_fee: 0.0 }),
                    }
                    r.tariff_prices += 1;
                }
            }
        }
        // grupos ordenados para las listas
        self.treatments.sort_by_key(|a| (a.only_tariff.is_some(), fold(&a.group), fold(&a.description)));
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    const SMALL: &str = r#"{"origen":"prueba","tarifas":[
      {"id":2227,"codigo":"","nombre":"- TARIFA PRIVADA -","inactiva":0,"pacientes":50,"tratamientos":[
        {"codigo":"OBT1","desc_medico":"OBTURACION SIMPLE","grupo":"OBTURACIONES","tipo_odg":"Obturaciones","precio":55.0,"piezas":0,"inactivo":0},
        {"codigo":"BLANQ","desc_medico":"BLANQUEAMIENTO EN CLÍNICA POR SESIÓN","grupo":"ESTETICA DENTAL","tipo_odg":"Genérico","precio":240.0,"piezas":2,"inactivo":0},
        {"codigo":"BLANQ","desc_medico":"BLANQUEAMIENTO ANTIGUO","grupo":"ESTETICA DENTAL","tipo_odg":"Genérico","precio":200.0,"piezas":2,"inactivo":1},
        {"codigo":"VIEJO","desc_medico":"TRATAMIENTO DE BAJA","grupo":"VARIOS","tipo_odg":"Genérico","precio":10.0,"piezas":2,"inactivo":1}
      ]},
      {"id":699,"codigo":"","nombre":"TRATAMIENTOS INGLES","inactiva":0,"pacientes":20,"tratamientos":[
        {"codigo":"OBT1","desc_medico":"SIMPLE FILLING","grupo":"OBTURACIONES","tipo_odg":"Obturaciones","precio":60.0,"piezas":0,"inactivo":0},
        {"codigo":"1PBRIDGE","desc_medico":"3 PIECE METAL AND PORCELAIN FIXED BRIDGE","grupo":"PROTESIS FIJA","tipo_odg":"Puente","precio":690.0,"piezas":1,"inactivo":0}
      ]},
      {"id":83,"codigo":"AV2","nombre":"AVANTSALUD - ALMUDENA SEGUROS 2013","inactiva":1,"pacientes":8,"tratamientos":[
        {"codigo":"AV-OBT","desc_medico":"OBTURACION","grupo":"OBTURACIONES","tipo_odg":"Obturaciones","precio":20.0,"piezas":0,"inactivo":0}
      ]}
    ]}"#;

    fn db() -> Db {
        Db::sample(NaiveDate::from_ymd_opt(2026, 11, 6).unwrap())
    }

    #[test]
    fn odontogram_types_map_to_tools() {
        assert_eq!(odg_from_gesden("Obturaciones"), OdgKind::Obturacion);
        assert_eq!(odg_from_gesden("Extracción Pieza"), OdgKind::Extraccion);
        assert_eq!(odg_from_gesden("Extracción Corona"), OdgKind::Extraccion);
        assert_eq!(odg_from_gesden("Perno-Muñón"), OdgKind::PernoMunon);
        assert_eq!(odg_from_gesden("Prótesis Removible"), OdgKind::ProtesisRemovible);
        assert_eq!(odg_from_gesden("Raspado"), OdgKind::Higiene);
        assert_eq!(odg_from_gesden("Brackets"), OdgKind::Bracket);
        assert_eq!(odg_from_gesden("Genérico"), OdgKind::Ninguno);
    }

    #[test]
    fn uppercase_texts_become_readable() {
        assert_eq!(pretty_text("BLANQUEAMIENTO EN CLÍNICA POR SESIÓN"), "Blanqueamiento en clínica por sesión");
        assert_eq!(pretty_text("BRACKETS 2X4 AUTOLIGABLES"), "Brackets 2X4 autoligables");
        assert_eq!(pretty_text("CIRUGIA"), "Cirugía");
        assert_eq!(pretty_text("PROTESIS FIJA"), "Prótesis fija");
        assert_eq!(pretty_text("ATM"), "ATM");
        assert_eq!(pretty_text("AÑADIR PIEZA A PRÓTESIS ACRÍLICA"), "Añadir pieza a prótesis acrílica");
        assert_eq!(pretty_text("Ya escrito bien"), "Ya escrito bien");
    }

    #[test]
    fn imports_base_catalog_and_tariffs_without_duplicates() {
        let cat = parse(SMALL).unwrap();
        let mut db = db();
        let r = db.import_gesden_catalog(&cat, ImportOptions::default()).unwrap();
        assert_eq!(r.base_tariff, "TARIFA PRIVADA");
        assert_eq!(r.treatments_added, 3, "OBT1, BLANQ (activo) y VIEJO");
        let blanq = db.treatment("BLANQ").unwrap();
        assert_eq!(blanq.price, 240.0, "el activo gana al dado de baja con el mismo código");
        assert_eq!(blanq.description, "Blanqueamiento en clínica por sesión");
        assert!(!db.treatment("VIEJO").unwrap().active);
        let obt = db.treatment("OBT1").unwrap();
        assert!(obt.per_tooth && obt.odg == OdgKind::Obturacion);
        // el catálogo de ejemplo deja de ofrecerse, salvo la ausencia del odontograma
        assert!(!db.treatment("0301").unwrap().active);
        assert!(db.treatment("9001").unwrap().active);
        assert!(r.treatments_hidden > 0);
        // tarifa en inglés: precio propio del código común y tratamiento exclusivo
        let eng = db.tariffs.iter().find(|t| t.name == "Tratamientos inglés").expect("tarifa creada");
        assert!(!eng.is_insurer);
        assert_eq!(db.price_for("OBT1", eng.id), 60.0);
        assert_eq!(db.price_for("OBT1", 1), 55.0);
        let bridge = db.treatment("1PBRIDGE").unwrap();
        assert_eq!(bridge.only_tariff, Some(eng.id));
        assert!(db.treatment_offered(bridge, eng.id));
        assert!(!db.treatment_offered(bridge, 1), "no se ofrece a los pacientes de otra tarifa");
        // la mutua dada de baja no se trae por defecto
        assert_eq!(r.skipped_inactive_tariffs, 1);
        assert!(db.treatment("AV-OBT").is_none());
        // repetir no duplica
        let n_t = db.treatments.len();
        let n_tar = db.tariffs.len();
        let r2 = db.import_gesden_catalog(&cat, ImportOptions::default()).unwrap();
        assert_eq!((db.treatments.len(), db.tariffs.len()), (n_t, n_tar));
        assert_eq!(r2.treatments_added, 0);
        assert_eq!(r2.tariffs_updated, 1);
        // con las tarifas de baja
        let r3 = db.import_gesden_catalog(&cat, ImportOptions { include_inactive_tariffs: true, ..Default::default() }).unwrap();
        assert_eq!(r3.tariffs_added, 1);
        let av = db.tariffs.iter().find(|t| t.notes.contains("[GESDEN #83]")).unwrap();
        assert!(av.inactive && av.is_insurer);
    }
}
