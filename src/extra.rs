//! Importación de datos (Excel/CSV), ortodoncia (casos y controles) y
//! remesas bancarias SEPA (adeudos directos, pain.008).

use crate::model::*;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

// ───────────────────────────── Importación ─────────────────────────────

/// Tabla leída de un fichero: cabecera + filas.
#[derive(Clone, Debug, Default)]
pub struct Sheet {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub fn read_table(path: &str) -> Result<Sheet, String> {
    let lower = path.to_lowercase();
    let mut raw: Vec<Vec<String>> = if lower.ends_with(".xlsx") || lower.ends_with(".xls") || lower.ends_with(".ods") || lower.ends_with(".xlsm") {
        use calamine::Reader;
        let mut wb = calamine::open_workbook_auto(path).map_err(|e| format!("No se puede abrir el Excel: {e}"))?;
        let range = wb.worksheet_range_at(0).ok_or("El libro no tiene hojas")?.map_err(|e| format!("No se puede leer la hoja: {e}"))?;
        range.rows().map(|r| r.iter().map(|c| c.to_string().trim().to_string()).collect()).collect()
    } else {
        let bytes = std::fs::read(path).map_err(|e| format!("No se puede leer el fichero: {e}"))?;
        let text = match String::from_utf8(bytes.clone()) {
            Ok(t) => t,
            Err(_) => bytes.iter().map(|&b| b as char).collect(), // Latin-1 (Excel en español)
        };
        let first = text.lines().next().unwrap_or("");
        let sep = [';', '\t', ','].into_iter().max_by_key(|c| first.matches(*c).count()).unwrap_or(';');
        text.lines().filter(|l| !l.trim().is_empty()).map(|l| split_csv(l, sep)).collect()
    };
    // la cabecera es la primera fila con al menos 2 celdas con texto
    // la cabecera es, entre las primeras filas, la que tiene más celdas de texto
    let text_cells = |r: &Vec<String>| r.iter().filter(|c| !c.is_empty() && c.parse::<f64>().is_err()).count();
    let hi = (0..raw.len().min(15)).max_by_key(|&i| (text_cells(&raw[i]), usize::MAX - i)).filter(|&i| text_cells(&raw[i]) >= 2).ok_or("No se encuentra la fila de títulos de las columnas")?;
    let mut headers = raw[hi].clone();
    raw.drain(..=hi);
    raw.retain(|r| r.iter().any(|c| !c.is_empty()));
    // Normalizar cabeceras combinadas de GESDEN (ej. "Nº Historial / Paciente" con celda adyacente vacía)
    for i in 0..headers.len() {
        let hl = fold(&headers[i]);
        if hl.contains("historial") && hl.contains("paciente") && i + 1 < headers.len() && headers[i + 1].trim().is_empty() {
            headers[i] = "Nº Historial".to_string();
            headers[i + 1] = "Paciente / Nombre".to_string();
        }
    }
    Ok(Sheet { headers, rows: raw })
}

fn split_csv(line: &str, sep: char) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut q = false;
    for ch in line.chars() {
        match ch {
            '"' => q = !q,
            c if c == sep && !q => out.push(std::mem::take(&mut cur).trim().to_string()),
            c => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportKind {
    Pacientes,
    Tratamientos,
}

/// Campos destino de cada tipo de importación y palabras clave para detectarlos.
pub fn targets(kind: ImportKind) -> Vec<(&'static str, &'static [&'static str])> {
    match kind {
        ImportKind::Pacientes => vec![
            ("Nº historial", &["historia", "historial", "numpac", "n.h", "nº h", "numero"]),
            ("Nombre", &["nombre", "paciente", "cliente", "razon social"]),
            ("Apellidos", &["apellido"]),
            ("NIF", &["nif", "dni", "nie", "documento", "d.n.i", "n.i.f", "n.i.e"]),
            ("Fecha nacimiento", &["nacim", "f.nac", "fecnac"]),
            ("Sexo", &["sexo"]),
            ("Móvil", &["movil", "móvil", "celular"]),
            ("Teléfono", &["telefono", "teléfono", "tel."]),
            ("Email", &["mail", "correo"]),
            ("Dirección", &["direcc", "domicilio", "calle"]),
            ("C.P.", &["postal", "c.p", "cp"]),
            ("Población", &["poblac", "localidad", "ciudad", "municipio"]),
            ("Provincia", &["provincia"]),
            ("Mutua / tarifa", &["mutua", "tarifa", "compañ", "aseguradora"]),
            ("Notas", &["observ", "notas"]),
        ],
        ImportKind::Tratamientos => vec![
            ("Código", &["codigo", "código", "cod"]),
            ("Descripción", &["descrip", "tratamiento", "concepto"]),
            ("Grupo", &["grupo", "familia", "especialidad"]),
            ("Precio", &["precio", "importe", "pvp", "tarifa"]),
            ("Duración (min)", &["durac", "minutos", "tiempo"]),
        ],
    }
}

/// Propone qué columna del fichero corresponde a cada campo.
pub fn guess_mapping(kind: ImportKind, headers: &[String]) -> Vec<Option<usize>> {
    let h: Vec<String> = headers.iter().map(|x| fold(x)).collect();
    let mut used = vec![false; h.len()];
    targets(kind)
        .iter()
        .map(|(name, keys)| {
            let pos = h.iter().enumerate().position(|(i, col)| {
                !used[i] && keys.iter().any(|k| col.contains(&fold(k))) && !(*name == "Nombre" && col.contains("apellido"))
            });
            if let Some(p) = pos {
                used[p] = true;
            }
            pos
        })
        .collect()
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    for f in ["%d/%m/%Y", "%d-%m-%Y", "%Y-%m-%d", "%d/%m/%y", "%Y-%m-%d %H:%M:%S", "%d/%m/%Y %H:%M:%S"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, f) {
            return Some(d);
        }
        if let Ok(d) = NaiveDateTime::parse_from_str(s, f) {
            return Some(d.date());
        }
    }
    // número de serie de Excel
    if let Ok(n) = s.parse::<f64>() && (1000.0..80000.0).contains(&n) {
        return NaiveDate::from_ymd_opt(1899, 12, 30).map(|b| b + Duration::days(n as i64));
    }
    None
}

pub fn parse_money(s: &str) -> Option<f64> {
    let t: String = s.chars().filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.' || *c == '-').collect();
    if t.is_empty() {
        return None;
    }
    // 1.234,56 → 1234.56 ; 1234.56 → 1234.56
    let t = if t.contains(',') { t.replace('.', "").replace(',', ".") } else { t };
    t.parse().ok()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
}

impl Db {
    /// Importa pacientes. Si ya existe (mismo NIF o Nº historial) se completan
    /// sus datos vacíos en lugar de duplicarlo.
    pub fn import_patients(&mut self, sheet: &Sheet, map: &[Option<usize>]) -> ImportResult {
        let mut r = ImportResult::default();
        let get = |row: &Vec<String>, i: usize| -> String { map.get(i).copied().flatten().and_then(|c| row.get(c)).cloned().unwrap_or_default() };
        for row in &sheet.rows {
            let mut first = get(row, 1);
            let mut last = get(row, 2);
            let mut hist_cell = get(row, 0);
            if first.is_empty() && last.is_empty() && hist_cell.chars().any(|c| c.is_alphabetic()) {
                // columna combinada «Nº Historial / Paciente»: «1234 - APELLIDOS, NOMBRE»
                let digits: String = hist_cell.chars().take_while(|c| c.is_ascii_digit() || c.is_whitespace()).collect();
                let rest = hist_cell[digits.len()..].trim_start_matches([' ', '-', '/', '.']).trim().to_string();
                first = rest;
                hist_cell = digits.trim().to_string();
            }
            if last.is_empty() && first.contains(',') {
                // «APELLIDOS, NOMBRE»
                let (a, b) = first.split_once(',').unwrap();
                last = a.trim().to_string();
                first = b.trim().to_string();
            } else if last.is_empty() && !first.is_empty() {
                // Si no hay coma (ej. «GALERA CASQUET MARINA» o «AZNAR DE HARO MATILDE»)
                let parts: Vec<String> = first.split_whitespace().map(|s| s.to_string()).collect();
                if parts.len() >= 3 {
                    first = parts.last().cloned().unwrap_or_default();
                    last = parts[..parts.len() - 1].join(" ");
                } else if parts.len() == 2 {
                    let f = parts[1].clone();
                    let l = parts[0].clone();
                    first = f;
                    last = l;
                }
            }
            if first.is_empty() && last.is_empty() {
                r.skipped += 1;
                continue;
            }
            let hist: Option<u32> = hist_cell.trim().parse::<f64>().ok().map(|x| x as u32).filter(|x| *x > 0);
            let nif = get(row, 3).to_uppercase().replace(['-', ' ', '.'], "");
            let tariff_name = get(row, 13);
            let tariff_id = if tariff_name.is_empty() {
                1
            } else {
                match self.tariffs.iter().find(|t| fold(&t.name) == fold(&tariff_name)) {
                    Some(t) => t.id,
                    None => {
                        let id = self.new_id();
                        self.tariffs.push(Tariff { id, name: tariff_name.clone(), is_insurer: true, discount_pct: 0.0, prices: vec![], phone: String::new(), notes: "Creada al importar".into(), valid_from: today(), valid_to: None, inactive: false });
                        id
                    }
                }
            };
            let sex = fold(&get(row, 5));
            let existing = self.patients.iter_mut().find(|p| (!nif.is_empty() && p.nif == nif) || (hist.is_some() && Some(p.history_no) == hist));
            let fill = |dst: &mut String, v: String| {
                if dst.trim().is_empty() && !v.trim().is_empty() {
                    *dst = v;
                }
            };
            match existing {
                Some(p) => {
                    fill(&mut p.mobile, get(row, 6));
                    fill(&mut p.phone, get(row, 7));
                    fill(&mut p.email, get(row, 8));
                    fill(&mut p.address, get(row, 9));
                    fill(&mut p.zip, get(row, 10));
                    fill(&mut p.city, get(row, 11));
                    fill(&mut p.province, get(row, 12));
                    if p.birth.is_none() {
                        p.birth = parse_date(&get(row, 4));
                    }
                    r.updated += 1;
                }
                None => {
                    let id = self.new_id();
                    let history_no = hist.filter(|h| !self.patients.iter().any(|p| p.history_no == *h)).unwrap_or_else(|| self.next_history_no());
                    self.patients.push(Patient {
                        id,
                        history_no,
                        first_name: title_case(&first),
                        last_name: title_case(&last),
                        nif,
                        birth: parse_date(&get(row, 4)),
                        sex: if sex.starts_with('m') && !sex.starts_with("mas") || sex.starts_with('f') { "F".into() } else if sex.is_empty() { String::new() } else { "M".into() },
                        mobile: get(row, 6),
                        phone: get(row, 7),
                        email: get(row, 8).to_lowercase(),
                        address: get(row, 9),
                        zip: get(row, 10),
                        city: title_case(&get(row, 11)),
                        province: title_case(&get(row, 12)),
                        tariff_id,
                        doctor_id: 1,
                        notes: get(row, 14),
                        registered: Some(today()),
                        accepts_sms: true,
                        active: true,
                        ..Default::default()
                    });
                    r.added += 1;
                }
            }
        }
        r
    }

    /// Importa tratamientos. Con `tariff_id` = 1 (Privado) crea/actualiza el
    /// catálogo; con otra tarifa fija los precios específicos de esa mutua.
    pub fn import_treatments(&mut self, sheet: &Sheet, map: &[Option<usize>], tariff_id: u32) -> ImportResult {
        let mut r = ImportResult::default();
        let get = |row: &Vec<String>, i: usize| -> String { map.get(i).copied().flatten().and_then(|c| row.get(c)).cloned().unwrap_or_default() };
        for row in &sheet.rows {
            let desc = get(row, 1);
            let mut code = get(row, 0);
            if desc.is_empty() && code.is_empty() {
                r.skipped += 1;
                continue;
            }
            let price = parse_money(&get(row, 3));
            if code.is_empty() {
                code = self.treatments.iter().find(|t| fold(&t.description) == fold(&desc)).map(|t| t.code.clone()).unwrap_or_else(|| format!("I{:04}", self.treatments.len() + 1));
            }
            if tariff_id > 1 {
                let Some(p) = price else {
                    r.skipped += 1;
                    continue;
                };
                if self.treatment(&code).is_none() {
                    r.skipped += 1;
                    continue;
                }
                if let Some(t) = self.tariffs.iter_mut().find(|t| t.id == tariff_id) {
                    match t.prices.iter_mut().find(|x| x.code == code) {
                        Some(x) => {
                            x.price = p;
                            r.updated += 1;
                        }
                        None => {
                            t.prices.push(TariffPrice { code: code.clone(), price: p, insurer_fee: 0.0 });
                            r.added += 1;
                        }
                    }
                }
                continue;
            }
            let minutes = get(row, 4).trim().parse::<f64>().ok().map(|m| m as u32);
            match self.treatments.iter_mut().find(|t| t.code == code) {
                Some(t) => {
                    if !desc.is_empty() {
                        t.description = desc.clone();
                    }
                    if let Some(p) = price {
                        t.price = p;
                    }
                    if let Some(m) = minutes {
                        t.minutes = m;
                    }
                    r.updated += 1;
                }
                None => {
                    let group = get(row, 2);
                    let odg = guess_odg(&desc);
                    self.treatments.push(Treatment {
                        code: code.clone(),
                        description: desc.clone(),
                        group: if group.is_empty() { "Importados".into() } else { group },
                        price: price.unwrap_or(0.0),
                        minutes: minutes.unwrap_or(30),
                        odg,
                        per_tooth: odg != OdgKind::Ninguno,
                        recall_months: 0,
                        active: true,
                        only_tariff: None,
                    });
                    r.added += 1;
                }
            }
        }
        r
    }
}

fn title_case(s: &str) -> String {
    if s.chars().any(|c| c.is_lowercase()) {
        return s.trim().to_string();
    }
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Deduce cómo dibujar un tratamiento en el odontograma por su nombre.
pub fn guess_odg(desc: &str) -> OdgKind {
    let d = fold(desc);
    let has = |k: &str| d.contains(k);
    if has("obtura") || has("composite") || has("empaste") || has("reconstruc") {
        OdgKind::Obturacion
    } else if has("endodon") || has("conducto") {
        OdgKind::Endodoncia
    } else if has("extrac") || has("exodon") {
        OdgKind::Extraccion
    } else if has("implante") {
        OdgKind::Implante
    } else if has("corona") || has("funda") {
        OdgKind::Corona
    } else if has("carilla") {
        OdgKind::Carilla
    } else if has("puente") {
        OdgKind::Puente
    } else if has("perno") || has("munon") {
        OdgKind::PernoMunon
    } else if has("apicect") {
        OdgKind::Apicectomia
    } else if has("protesis") || has("esqueletic") || has("dentadura") {
        OdgKind::ProtesisRemovible
    } else if has("sellador") {
        OdgKind::Sellador
    } else {
        OdgKind::Ninguno
    }
}

// ───────────────────────────── Ortodoncia ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum OrthoState {
    Estudio,
    Activo,
    Retencion,
    Finalizado,
    Abandonado,
}

impl OrthoState {
    pub const ALL: [OrthoState; 5] = [OrthoState::Estudio, OrthoState::Activo, OrthoState::Retencion, OrthoState::Finalizado, OrthoState::Abandonado];
    pub fn label(self) -> &'static str {
        match self {
            OrthoState::Estudio => "En estudio",
            OrthoState::Activo => "Tratamiento activo",
            OrthoState::Retencion => "Retención",
            OrthoState::Finalizado => "Finalizado",
            OrthoState::Abandonado => "Abandonado",
        }
    }
}

pub const APPLIANCES: [&str; 6] = ["Brackets metálicos", "Brackets estéticos", "Alineadores transparentes", "Aparato removible", "Disyuntor / ortopedia", "Retenedor"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrthoVisit {
    pub date: NaiveDate,
    pub work: String,
    pub wire: String,
    pub hygiene_ok: bool,
    pub next_weeks: u32,
    pub notes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrthoCase {
    pub id: u32,
    pub patient_id: u32,
    pub doctor_id: u32,
    pub start: NaiveDate,
    pub months: u32,
    pub appliance: String,
    pub arches: String,
    pub class: String,
    pub diagnosis: String,
    pub objectives: String,
    pub state: OrthoState,
    pub fee: f64,
    pub visits: Vec<OrthoVisit>,
}

impl OrthoCase {
    pub fn progress(&self, today: NaiveDate) -> f32 {
        let total = (self.months.max(1) as f32) * 30.4;
        ((today - self.start).num_days() as f32 / total).clamp(0.0, 1.0)
    }
    pub fn next_control(&self) -> Option<NaiveDate> {
        self.visits.iter().max_by_key(|v| v.date).map(|v| v.date + Duration::weeks(v.next_weeks as i64))
    }
    pub fn hygiene_pct(&self) -> f32 {
        if self.visits.is_empty() {
            return 100.0;
        }
        self.visits.iter().filter(|v| v.hygiene_ok).count() as f32 * 100.0 / self.visits.len() as f32
    }
}

impl Db {
    /// Registra un control de ortodoncia y programa el recall del siguiente.
    pub fn add_ortho_visit(&mut self, case_id: u32, v: OrthoVisit) {
        let Some(c) = self.ortho.iter_mut().find(|c| c.id == case_id) else { return };
        let pid = c.patient_id;
        let doc = c.doctor_id;
        let due = v.date + Duration::weeks(v.next_weeks as i64);
        if c.state == OrthoState::Estudio {
            c.state = OrthoState::Activo;
        }
        c.visits.push(v);
        c.visits.sort_by_key(|x| x.date);
        match self.recalls.iter_mut().find(|r| r.patient_id == pid && r.reason == "Control de ortodoncia" && r.state == RecallState::Pendiente) {
            Some(r) => r.date = due,
            None => {
                let id = self.new_id();
                self.recalls.push(Recall { id, patient_id: pid, date: due, reason: "Control de ortodoncia".into(), doctor_id: doc, state: RecallState::Pendiente, channel: String::new(), comment: "Programado desde el caso de ortodoncia".into(), contacted_on: None, appt_id: None });
            }
        }
    }
}

// ───────────────────────────── Remesas SEPA ─────────────────────────────

/// Valida un IBAN (longitud por país y dígitos de control mod 97).
pub fn iban_valid(iban: &str) -> bool {
    let s: String = iban.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();
    if s.len() < 15 || s.len() > 34 || !s.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    if s.starts_with("ES") && s.len() != 24 {
        return false;
    }
    let rearr = format!("{}{}", &s[4..], &s[..4]);
    let mut rem: u32 = 0;
    for ch in rearr.chars() {
        let v = if ch.is_ascii_digit() { ch.to_digit(10).unwrap().to_string() } else { (ch as u32 - 'A' as u32 + 10).to_string() };
        for d in v.chars() {
            rem = (rem * 10 + d.to_digit(10).unwrap()) % 97;
        }
    }
    rem == 1
}

pub fn iban_fmt(iban: &str) -> String {
    let s: String = iban.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();
    s.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).to_string()).collect::<Vec<_>>().join(" ")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemItem {
    pub patient_id: u32,
    pub budget_id: u32,
    pub installment: u32,
    pub amount: f64,
    pub iban: String,
    pub mandate: String,
    pub mandate_date: NaiveDate,
    pub end_to_end: String,
    /// El banco ha devuelto el recibo (fecha y motivo).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returned: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub return_reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemState {
    Generada,
    Cobrada,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Remittance {
    pub id: u32,
    pub created: NaiveDateTime,
    pub collection: NaiveDate,
    pub items: Vec<RemItem>,
    pub state: RemState,
    pub file: String,
}

impl Remittance {
    /// Lo que se pidió al banco (el total del fichero).
    pub fn total(&self) -> f64 {
        round2(self.items.iter().map(|i| i.amount).sum())
    }
    /// Lo cobrado: sin los recibos devueltos.
    pub fn collected(&self) -> f64 {
        round2(self.items.iter().filter(|i| i.returned.is_none()).map(|i| i.amount).sum())
    }
}

/// Cuota domiciliable pendiente.
#[derive(Clone, Debug)]
pub struct DueInstallment {
    pub patient_id: u32,
    pub budget_id: u32,
    pub index: u32,
    pub due: NaiveDate,
    pub amount: f64,
    pub problem: Option<String>,
}

impl Db {
    /// Cuotas de planes de financiación pendientes hasta `until`, sin remesar.
    pub fn due_installments(&self, until: NaiveDate) -> Vec<DueInstallment> {
        let mut out = vec![];
        for b in &self.budgets {
            let Some(pl) = &b.plan else { continue };
            if !matches!(b.state, BudgetState::Aceptado | BudgetState::Cerrado) {
                continue;
            }
            for i in 0..pl.installments {
                let paid = pl.paid.get(i as usize).copied().unwrap_or(false);
                let due = pl.due_date(i);
                if paid || due > until {
                    continue;
                }
                // un recibo devuelto se puede volver a domiciliar
                let remitted = self.remittances.iter().any(|r| r.items.iter().any(|x| x.budget_id == b.id && x.installment == i && x.returned.is_none()));
                if remitted {
                    continue;
                }
                let p = self.patient(b.patient_id);
                let problem = match p {
                    Some(p) if p.iban.trim().is_empty() => Some("sin IBAN".to_string()),
                    Some(p) if !iban_valid(&p.iban) => Some("IBAN no válido".to_string()),
                    Some(p) if !p.sepa_signed => Some("sin mandato SEPA firmado".to_string()),
                    None => Some("paciente no encontrado".to_string()),
                    _ => None,
                };
                out.push(DueInstallment { patient_id: b.patient_id, budget_id: b.id, index: i, due, amount: pl.installment(i, b.total()), problem });
            }
        }
        out.sort_by_key(|d| d.due);
        out
    }

    /// Crea una remesa con las cuotas válidas indicadas y genera el XML.
    pub fn create_remittance(&mut self, items: &[DueInstallment], collection: NaiveDate) -> Result<u32, String> {
        if self.config.creditor_id.trim().is_empty() || !iban_valid(&self.config.clinic_iban) {
            return Err("Configure el identificador de acreedor SEPA y el IBAN de la clínica en Configuración.".into());
        }
        let valid: Vec<&DueInstallment> = items.iter().filter(|d| d.problem.is_none()).collect();
        if valid.is_empty() {
            return Err("No hay cuotas domiciliables (revise IBAN y mandatos).".into());
        }
        let id = self.new_id();
        let rem_items: Vec<RemItem> = valid
            .iter()
            .map(|d| {
                let p = self.patient(d.patient_id).cloned().unwrap_or_default();
                RemItem {
                    patient_id: d.patient_id,
                    budget_id: d.budget_id,
                    installment: d.index,
                    amount: d.amount,
                    iban: p.iban.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase(),
                    mandate: if p.sepa_mandate.is_empty() { format!("DS{:06}", p.history_no) } else { p.sepa_mandate.clone() },
                    mandate_date: p.sepa_date.unwrap_or(today()),
                    end_to_end: format!("R{}-{}-{}", id, d.budget_id, d.index + 1),
                    returned: None,
                    return_reason: String::new(),
                }
            })
            .collect();
        self.remittances.push(Remittance { id, created: now(), collection, items: rem_items, state: RemState::Generada, file: String::new() });
        Ok(id)
    }

    /// XML de adeudos directos SEPA (pain.008.001.02, esquema CORE).
    pub fn sepa_xml(&self, rid: u32) -> String {
        let Some(r) = self.remittances.iter().find(|r| r.id == rid) else { return String::new() };
        let c = &self.config;
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let ascii = |s: &str| -> String { fold(s).to_uppercase().chars().map(|ch| if ch.is_ascii_alphanumeric() || " /-?:().,'+".contains(ch) { ch } else { ' ' }).take(70).collect() };
        let msg = format!("VD-{}-{}", rid, r.created.format("%Y%m%d%H%M%S"));
        let mut tx = String::new();
        for it in &r.items {
            let p = self.patient(it.patient_id).cloned().unwrap_or_default();
            tx.push_str(&format!(
                "<DrctDbtTxInf><PmtId><EndToEndId>{e2e}</EndToEndId></PmtId><InstdAmt Ccy=\"EUR\">{amt:.2}</InstdAmt><DrctDbtTx><MndtRltdInf><MndtId>{mid}</MndtId><DtOfSgntr>{md}</DtOfSgntr></MndtRltdInf></DrctDbtTx><DbtrAgt><FinInstnId><Othr><Id>NOTPROVIDED</Id></Othr></FinInstnId></DbtrAgt><Dbtr><Nm>{name}</Nm></Dbtr><DbtrAcct><Id><IBAN>{iban}</IBAN></Id></DbtrAcct><RmtInf><Ustrd>{concept}</Ustrd></RmtInf></DrctDbtTxInf>",
                e2e = esc(&it.end_to_end),
                amt = it.amount,
                mid = esc(&it.mandate),
                md = it.mandate_date.format("%Y-%m-%d"),
                name = esc(&ascii(&p.full_name())),
                iban = esc(&it.iban),
                concept = esc(&ascii(&format!("Cuota {} tratamiento dental presupuesto {}", it.installment + 1, it.budget_id))),
            ));
        }
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:pain.008.001.02\"><CstmrDrctDbtInitn><GrpHdr><MsgId>{msg}</MsgId><CreDtTm>{cre}</CreDtTm><NbOfTxs>{n}</NbOfTxs><CtrlSum>{sum:.2}</CtrlSum><InitgPty><Nm>{cn}</Nm></InitgPty></GrpHdr><PmtInf><PmtInfId>{msg}-1</PmtInfId><PmtMtd>DD</PmtMtd><NbOfTxs>{n}</NbOfTxs><CtrlSum>{sum:.2}</CtrlSum><PmtTpInf><SvcLvl><Cd>SEPA</Cd></SvcLvl><LclInstrm><Cd>CORE</Cd></LclInstrm><SeqTp>RCUR</SeqTp></PmtTpInf><ReqdColltnDt>{col}</ReqdColltnDt><Cdtr><Nm>{cn}</Nm></Cdtr><CdtrAcct><Id><IBAN>{ciban}</IBAN></Id></CdtrAcct><CdtrAgt><FinInstnId><Othr><Id>NOTPROVIDED</Id></Othr></FinInstnId></CdtrAgt><CdtrSchmeId><Id><PrvtId><Othr><Id>{cid}</Id><SchmeNm><Prtry>SEPA</Prtry></SchmeNm></Othr></PrvtId></Id></CdtrSchmeId>{tx}</PmtInf></CstmrDrctDbtInitn></Document>\n",
            msg = msg,
            cre = r.created.format("%Y-%m-%dT%H:%M:%S"),
            n = r.items.len(),
            sum = r.total(),
            cn = esc(&ascii(&c.name)),
            col = r.collection.format("%Y-%m-%d"),
            ciban = esc(&c.clinic_iban.chars().filter(|x| !x.is_whitespace()).collect::<String>().to_uppercase()),
            cid = esc(c.creditor_id.trim()),
            tx = tx,
        )
    }

    /// El banco ha cobrado la remesa: se registran los cobros y las cuotas pagadas.
    pub fn remittance_collected(&mut self, rid: u32) -> Result<(), String> {
        let r = self.remittances.iter().find(|r| r.id == rid).cloned().ok_or("Remesa no encontrada")?;
        if r.state == RemState::Cobrada {
            return Err("La remesa ya está marcada como cobrada.".into());
        }
        for it in r.items.iter().filter(|i| i.returned.is_none()) {
            let pid = it.patient_id;
            self.add_payment(pid, it.amount, "Domiciliación", &format!("Cuota {} remesa SEPA #{}", it.installment + 1, rid));
            self.set_installment_paid(it.budget_id, it.installment, true);
        }
        if let Some(x) = self.remittances.iter_mut().find(|x| x.id == rid) {
            x.state = RemState::Cobrada;
        }
        Ok(())
    }

    /// El banco ha devuelto el recibo `idx` de la remesa (el paciente no lo ha
    /// pagado): si ya se había registrado el cobro, se deshace con un cobro en
    /// negativo; la cuota vuelve a estar pendiente (para la siguiente remesa o
    /// para cobrarla en la clínica).
    pub fn remittance_returned(&mut self, rid: u32, idx: usize, reason: &str) -> Result<(), String> {
        let r = self.remittances.iter().find(|r| r.id == rid).ok_or("Remesa no encontrada")?;
        let it = r.items.get(idx).cloned().ok_or("Recibo no encontrado en la remesa")?;
        if it.returned.is_some() {
            return Err("Ese recibo ya está marcado como devuelto.".into());
        }
        let collected = r.state == RemState::Cobrada;
        if let Some(x) = self.remittances.iter_mut().find(|r| r.id == rid).and_then(|r| r.items.get_mut(idx)) {
            x.returned = Some(today());
            x.return_reason = reason.trim().to_string();
        }
        if collected {
            self.add_payment(it.patient_id, -it.amount, "Domiciliación", &format!("Devolución cuota {} remesa SEPA #{rid}", it.installment + 1));
            self.set_installment_paid(it.budget_id, it.installment, false);
        }
        let why = if reason.trim().is_empty() { String::new() } else { format!(" ({})", reason.trim()) };
        self.audit("Recibo SEPA devuelto", &format!("{} · cuota {} · {} · remesa #{rid}{why}", self.patient_name(it.patient_id), it.installment + 1, money(it.amount)));
        Ok(())
    }

    /// Remesa enviada al banco, aún sin cobrar, en la que va la cuota `i` del
    /// presupuesto (si va en alguna): el banco se la cobrará al paciente.
    pub fn installment_remittance(&self, budget_id: u32, i: u32) -> Option<u32> {
        self.remittances.iter().find(|r| r.state == RemState::Generada && r.items.iter().any(|x| x.budget_id == budget_id && x.installment == i && x.returned.is_none())).map(|r| r.id)
    }

    /// Por qué ya no se puede cambiar el plan de cuotas de un presupuesto (o
    /// borrarlo): alguna cuota está cobrada o va en una remesa al banco.
    pub fn plan_locked(&self, budget_id: u32) -> Option<String> {
        let plan = self.budgets.iter().find(|b| b.id == budget_id)?.plan.as_ref()?;
        if let Some(i) = plan.paid.iter().position(|p| *p) {
            return Some(format!("la cuota {} ya está cobrada", i + 1));
        }
        (0..plan.installments).find_map(|i| self.installment_remittance(budget_id, i).map(|r| format!("la cuota {} va en la remesa #{r}", i + 1)))
    }

    /// Cobra en la clínica una cuota de un plan (con la forma de pago que use
    /// el paciente: si es efectivo, cuenta en la caja). Devuelve el cobro.
    pub fn pay_installment(&mut self, budget_id: u32, i: u32, method: &str) -> Result<u32, String> {
        let b = self.budgets.iter().find(|b| b.id == budget_id).cloned().ok_or("El presupuesto ya no existe.")?;
        let plan = b.plan.clone().ok_or("El presupuesto no tiene plan de cuotas.")?;
        if i >= plan.installments {
            return Err("Esa cuota no existe.".into());
        }
        if plan.paid.get(i as usize).copied().unwrap_or(false) {
            return Err(format!("La cuota {} ya está pagada.", i + 1));
        }
        if let Some(rid) = self.installment_remittance(budget_id, i) {
            return Err(format!("La cuota {} va en la remesa #{rid} que se ha enviado al banco: si el banco la devuelve, regístrelo en Remesas SEPA y se podrá cobrar aquí.", i + 1));
        }
        let id = self.add_payment(b.patient_id, plan.installment(i, b.total()), method, &format!("Cuota {} plan presupuesto #{budget_id}", i + 1));
        if let Some(p) = self.payments.iter_mut().find(|p| p.id == id) {
            p.installment = Some((budget_id, i));
        }
        self.set_installment_paid(budget_id, i, true);
        Ok(id)
    }

    /// Marca la cuota `i` del plan del presupuesto como pagada (o no).
    pub fn set_installment_paid(&mut self, budget_id: u32, i: u32, paid: bool) {
        if let Some(b) = self.budgets.iter_mut().find(|b| b.id == budget_id) && let Some(pl) = &mut b.plan {
            if pl.paid.len() < pl.installments as usize {
                pl.paid.resize(pl.installments as usize, false);
            }
            if let Some(x) = pl.paid.get_mut(i as usize) {
                *x = paid;
            }
        }
    }
}

pub fn sample_extra(db: &mut Db, today: NaiveDate) {
    // ortodoncia
    let mk = |db: &mut Db, pid: u32, start_ago: i64, months: u32, app: &str, class: &str, diag: &str, fee: f64, visits: &[(i64, &str, &str, bool)]| {
        let id = db.new_id();
        db.ortho.push(OrthoCase {
            id,
            patient_id: pid,
            doctor_id: 3,
            start: today - Duration::days(start_ago),
            months,
            appliance: app.into(),
            arches: "Ambas arcadas".into(),
            class: class.into(),
            diagnosis: diag.into(),
            objectives: "Alinear y nivelar, corregir resalte, obtener clase I canina y molar.".into(),
            state: OrthoState::Activo,
            fee,
            visits: visits.iter().map(|(ago, work, wire, hy)| OrthoVisit { date: today - Duration::days(*ago), work: (*work).into(), wire: (*wire).into(), hygiene_ok: *hy, next_weeks: 5, notes: String::new() }).collect(),
        });
    };
    mk(db, 103, 260, 24, "Brackets metálicos", "Clase II división 1", "Apiñamiento moderado superior e inferior, resalte aumentado.", 2900.0, &[(250, "Colocación de brackets", "0.014 NiTi", true), (215, "Cambio de arco", "0.016 NiTi", true), (180, "Cambio de arco", "0.016x0.022 NiTi", false), (35, "Activación y elásticos clase II", "0.019x0.025 acero", true)]);
    mk(db, 116, 90, 18, "Alineadores transparentes", "Clase I", "Apiñamiento leve anterior, diastemas.", 3200.0, &[(85, "Entrega alineadores 1-3", "Alineador 3", true), (45, "Revisión y attachments", "Alineador 9", true)]);
    let id = db.new_id();
    db.ortho.push(OrthoCase { id, patient_id: 111, doctor_id: 3, start: today, months: 20, appliance: "Disyuntor / ortopedia".into(), arches: "Superior".into(), class: "Clase III".into(), diagnosis: "Mordida cruzada posterior bilateral.".into(), objectives: "Expansión maxilar.".into(), state: OrthoState::Estudio, fee: 1800.0, visits: vec![] });
    // datos SEPA de ejemplo (IBAN de prueba válido)
    db.config.creditor_id = "ES12000B00000000".into();
    db.config.clinic_iban = "ES9121000418450200051332".into();
    if let Some(p) = db.patient_mut(101) {
        p.iban = "ES7921000813610123456789".into();
        p.sepa_signed = true;
        p.sepa_mandate = "DS001002".into();
        p.sepa_date = Some(today - Duration::days(10));
    }
    let _ = Datelike::year(&today);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()
    }

    #[test]
    fn iban_validation() {
        assert!(iban_valid("ES91 2100 0418 4502 0005 1332"));
        assert!(iban_valid("ES7921000813610123456789"));
        assert!(!iban_valid("ES9121000418450200051333"));
        assert!(!iban_valid("ES12"));
        assert_eq!(iban_fmt("es9121000418450200051332"), "ES91 2100 0418 4502 0005 1332");
    }

    #[test]
    fn import_patients_from_csv_without_duplicates() {
        let dir = std::env::temp_dir().join("ds_import_test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pac.csv");
        std::fs::write(&p, "Nº Historia;Apellidos;Nombre;D.N.I.;F. Nacimiento;Teléfono móvil;E-mail;Población;Mutua\n5001;PÉREZ GÓMEZ;LUIS;12345678Z;03/04/1980;600111222;LUIS@MAIL.ES;MADRID;Aseguradora Nueva\n1001;García López;Ana;;;611222333;;;\n").unwrap();
        let sheet = read_table(p.to_str().unwrap()).unwrap();
        let map = guess_mapping(ImportKind::Pacientes, &sheet.headers);
        assert_eq!(map[1], Some(2)); // Nombre
        assert_eq!(map[2], Some(1)); // Apellidos
        let mut db = Db::sample(d());
        let n = db.patients.len();
        let r = db.import_patients(&sheet, &map);
        assert_eq!(r.added, 1);
        assert_eq!(r.updated, 1); // Nº historial 1001 ya existe
        assert_eq!(db.patients.len(), n + 1);
        let luis = db.patients.iter().find(|x| x.nif == "12345678Z").unwrap();
        assert_eq!(luis.first_name, "Luis");
        assert_eq!(luis.last_name, "Pérez Gómez");
        assert_eq!(luis.birth, NaiveDate::from_ymd_opt(1980, 4, 3));
        assert!(db.tariffs.iter().any(|t| t.name == "Aseguradora Nueva"));
    }

    #[test]
    fn import_treatments_and_tariff_prices() {
        let sheet = Sheet { headers: vec!["Código".into(), "Descripción".into(), "Precio".into()], rows: vec![vec!["0301".into(), "Obturación simple".into(), "60,00 €".into()], vec!["".into(), "Corona cerámica sobre diente".into(), "1.250,50".into()]] };
        let map = guess_mapping(ImportKind::Tratamientos, &sheet.headers);
        let mut db = Db::sample(d());
        let r = db.import_treatments(&sheet, &map, 1);
        assert_eq!((r.added, r.updated), (1, 1));
        assert_eq!(db.treatment("0301").unwrap().price, 60.0);
        let new = db.treatments.iter().find(|t| t.description.starts_with("Corona cerámica")).unwrap();
        assert_eq!(new.price, 1250.5);
        assert_eq!(new.odg, OdgKind::Corona);
        let r2 = db.import_treatments(&sheet, &map, 2);
        assert_eq!(r2.added + r2.updated, 2);
        assert_eq!(db.price_for("0301", 2), 60.0);
    }

    #[test]
    fn sepa_remittance_flow() {
        let mut db = Db::sample(d());
        // aceptar el presupuesto financiado del paciente 101
        let bid = db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        db.accept_budget(bid, d());
        let due = db.due_installments(d() + Duration::days(40));
        assert!(!due.is_empty());
        assert!(due.iter().all(|x| x.problem.is_none()));
        let rid = db.create_remittance(&due, d() + Duration::days(5)).unwrap();
        let xml = db.sepa_xml(rid);
        assert!(xml.contains("pain.008.001.02"));
        assert!(xml.contains("<IBAN>ES7921000813610123456789</IBAN>"));
        assert!(db.due_installments(d() + Duration::days(40)).is_empty(), "no se deben volver a remesar");
        let paid_before = db.patient_paid(101);
        db.remittance_collected(rid).unwrap();
        assert!(db.patient_paid(101) > paid_before);
        assert!(db.remittance_collected(rid).is_err());
    }

    #[test]
    fn installments_add_up_to_the_total() {
        let pl = FinancePlan { down_payment: 0.0, installments: 3, first_date: d(), every_months: 1, paid: vec![] };
        let all: Vec<f64> = (0..3).map(|i| pl.installment(i, 1000.0)).collect();
        assert_eq!(all, vec![333.33, 333.33, 333.34]);
        let pl = FinancePlan { down_payment: 100.0, installments: 7, first_date: d(), every_months: 1, paid: vec![] };
        let sum: f64 = (0..7).map(|i| pl.installment(i, 1234.56)).sum();
        assert!((sum - 1134.56).abs() < 0.001, "{sum}");
        // un presupuesto de 1.000 € en 3 cuotas, todas cobradas: no queda deuda
        let mut db = Db::sample(d());
        let bid = db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        db.accept_budget(bid, d());
        let due = db.due_installments(d() + Duration::days(3650));
        for x in &due {
            db.add_payment(x.patient_id, x.amount, "Financiación", "cuota");
        }
        let b = db.budgets.iter().find(|b| b.id == bid).unwrap();
        let pl = b.plan.as_ref().unwrap();
        let sum: f64 = due.iter().map(|x| x.amount).sum();
        assert!((sum + pl.down_payment - b.total()).abs() < 0.001, "las cuotas suman {sum} y el total es {}", b.total());
    }

    #[test]
    fn a_plan_with_paid_or_remitted_installments_cannot_change() {
        let mut db = Db::sample(d());
        let bid = db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        db.accept_budget(bid, d());
        assert_eq!(db.plan_locked(bid), None, "sin cobrar nada, se puede cambiar");
        // una cuota en una remesa
        let due: Vec<_> = db.due_installments(d() + Duration::days(400)).into_iter().filter(|x| x.budget_id == bid).take(1).collect();
        let rid = db.create_remittance(&due, d() + Duration::days(5)).unwrap();
        assert!(db.plan_locked(bid).unwrap().contains(&format!("remesa #{rid}")));
        // o cobrada en la clínica
        let mut db2 = Db::sample(d());
        db2.accept_budget(bid, d());
        db2.pay_installment(bid, 0, "Tarjeta").unwrap();
        assert!(db2.plan_locked(bid).unwrap().contains("cobrada"));
    }

    #[test]
    fn an_installment_paid_in_cash_counts_in_the_cash() {
        let mut db = Db::sample(d());
        let bid = db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        db.accept_budget(bid, d());
        let cash = db.open_cash().unwrap().clone();
        let before = db.cash_expected(&cash);
        let total = db.budgets.iter().find(|b| b.id == bid).unwrap().total();
        let each = db.budgets.iter().find(|b| b.id == bid).unwrap().plan.clone().unwrap().installment(0, total);
        let pay = db.pay_installment(bid, 0, "Efectivo").unwrap();
        let p = db.payments.iter().find(|p| p.id == pay).unwrap().clone();
        assert_eq!((p.amount, p.method.as_str(), p.installment), (each, "Efectivo", Some((bid, 0))));
        assert!((db.cash_expected(db.open_cash().unwrap()) - (before + each)).abs() < 0.005, "el efectivo de la cuota está en la caja");
        assert!(db.budgets.iter().find(|b| b.id == bid).unwrap().plan.as_ref().unwrap().paid[0]);
        assert!(db.pay_installment(bid, 0, "Efectivo").unwrap_err().contains("pagada"), "no se cobra dos veces");
        // una cuota que ya va en una remesa al banco no se cobra en la clínica
        let due = db.due_installments(d() + Duration::days(400));
        let rid = db.create_remittance(&due, d() + Duration::days(5)).unwrap();
        let item = db.remittances.iter().find(|r| r.id == rid).unwrap().items[0].clone();
        assert!(db.pay_installment(item.budget_id, item.installment, "Tarjeta").unwrap_err().contains("remesa"));
        // anular el cobro de una cuota la deja otra vez por pagar
        db.cancel_payment(pay, "se equivocó de cuota", false).unwrap();
        assert!(!db.budgets.iter().find(|b| b.id == bid).unwrap().plan.as_ref().unwrap().paid[0]);
    }

    #[test]
    fn a_returned_debit_is_owed_again_and_goes_in_the_next_remittance() {
        let mut db = Db::sample(d());
        let bid = db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        db.accept_budget(bid, d());
        let due = db.due_installments(d() + Duration::days(40));
        let rid = db.create_remittance(&due, d() + Duration::days(5)).unwrap();
        db.remittance_collected(rid).unwrap();
        let bal = db.patient_balance(101);
        let it = db.remittances.iter().find(|r| r.id == rid).unwrap().items[0].clone();
        db.remittance_returned(rid, 0, "AM04 fondos insuficientes").unwrap();
        // vuelve a deber la cuota, que ya no está pagada
        assert!((db.patient_balance(101) - (bal + it.amount)).abs() < 0.001);
        let pl = db.budgets.iter().find(|b| b.id == bid).unwrap().plan.clone().unwrap();
        assert!(!pl.paid[it.installment as usize]);
        assert!(db.payments.iter().any(|p| p.amount == -it.amount && p.method == "Domiciliación" && p.concept.contains("Devolución")));
        // dos veces no
        assert!(db.remittance_returned(rid, 0, "").is_err());
        // la cuota se puede volver a domiciliar
        let again = db.due_installments(d() + Duration::days(40));
        assert!(again.iter().any(|x| x.budget_id == bid && x.index == it.installment));
        // lo cobrado de la remesa es lo que no se ha devuelto
        let r = db.remittances.iter().find(|r| r.id == rid).unwrap();
        assert!((r.collected() - (r.total() - it.amount)).abs() < 0.001);
    }

    #[test]
    fn a_debit_returned_before_marking_the_remittance_collected_is_not_charged() {
        let mut db = Db::sample(d());
        let bid = db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        db.accept_budget(bid, d());
        let due = db.due_installments(d() + Duration::days(40));
        let rid = db.create_remittance(&due, d() + Duration::days(5)).unwrap();
        let pays = db.payments.len();
        let n = db.remittances.iter().find(|r| r.id == rid).unwrap().items.len();
        db.remittance_returned(rid, 0, "").unwrap();
        assert_eq!(db.payments.len(), pays, "sin cobrar aún, no hay nada que deshacer");
        db.remittance_collected(rid).unwrap();
        assert_eq!(db.payments.len(), pays + n - 1, "la devuelta no se cobra");
        let it = &db.remittances.iter().find(|r| r.id == rid).unwrap().items[0];
        let pl = db.budgets.iter().find(|b| b.id == bid).unwrap().plan.clone().unwrap();
        assert!(!pl.paid[it.installment as usize]);
    }

    #[test]
    fn an_installment_in_a_sent_remittance_is_not_charged_again() {
        let mut db = Db::sample(d());
        let bid = db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        db.accept_budget(bid, d());
        let due = db.due_installments(d() + Duration::days(40));
        let i = due[0].index;
        assert_eq!(db.installment_remittance(bid, i), None);
        let rid = db.create_remittance(&due, d() + Duration::days(5)).unwrap();
        assert_eq!(db.installment_remittance(bid, i), Some(rid), "enviada al banco: no se cobra en la clínica");
        db.remittance_returned(rid, 0, "").unwrap();
        assert_eq!(db.installment_remittance(bid, i), None, "devuelta: se puede cobrar");
    }

    #[test]
    fn ortho_visit_schedules_recall() {
        let mut db = Db::sample(d());
        let cid = db.ortho.iter().find(|c| c.state == OrthoState::Estudio).unwrap().id;
        db.add_ortho_visit(cid, OrthoVisit { date: d(), work: "Cementado".into(), wire: String::new(), hygiene_ok: true, next_weeks: 4, notes: String::new() });
        let c = db.ortho.iter().find(|c| c.id == cid).unwrap();
        assert_eq!(c.state, OrthoState::Activo);
        assert!(db.recalls.iter().any(|r| r.patient_id == c.patient_id && r.reason == "Control de ortodoncia" && r.date == d() + Duration::weeks(4)));
    }
}

#[cfg(test)]
mod probe {
    /// Muestra solo las cabeceras de un fichero real (DS_PROBE=ruta).
    #[test]
    #[ignore]
    fn print_headers() {
        let p = std::env::var("DS_PROBE").unwrap();
        let s = super::read_table(&p).unwrap();
        println!("HEADERS {:?} ROWS {}", s.headers, s.rows.len());
        println!("MAP {:?}", super::guess_mapping(super::ImportKind::Pacientes, &s.headers));
    }
}
