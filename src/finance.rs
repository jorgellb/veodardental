//! Gastos de la clínica y recetas (prescripción con avisos de alergias e
//! interacciones habituales en odontología).

use crate::model::*;

use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};

pub const EXPENSE_CATEGORIES: [&str; 10] = ["Material", "Laboratorio", "Alquiler", "Nóminas", "Suministros", "Seguros", "Marketing", "Mantenimiento", "Impuestos y tasas", "Otros"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Expense {
    pub id: u32,
    pub date: NaiveDate,
    pub supplier: String,
    pub concept: String,
    pub category: String,
    pub base: f64,
    pub vat_pct: f64,
    pub invoice_no: String,
    pub method: String,
    pub paid: bool,
    pub due: Option<NaiveDate>,
}

impl Expense {
    pub fn vat(&self) -> f64 {
        round2(self.base * self.vat_pct / 100.0)
    }
    pub fn total(&self) -> f64 {
        round2(self.base + self.vat())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Drug {
    pub name: String,
    pub form: String,
    pub dose: String,
    pub frequency: String,
    pub days: u32,
    pub group: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RxItem {
    /// Denominación del medicamento (lo que se imprime).
    pub drug: String,
    pub dose: String,
    pub frequency: String,
    pub days: u32,
    /// Envases para todo el tratamiento.
    pub units: u32,
    pub notes: String,
    // ── datos de la receta oficial (conexión directa con el SREP) ──
    /// Código nacional del medicamento (prescripción por marca).
    #[serde(default)]
    pub cn: String,
    /// Código DCPF del principio activo (prescripción por principio activo).
    #[serde(default)]
    pub active_code: String,
    /// Composición de una fórmula magistral.
    #[serde(default)]
    pub composition: String,
    /// 0 medicamento, 1 producto sanitario, 2 nutrición, 3 vacuna individualizada, 4 fórmula magistral.
    #[serde(default)]
    pub product_type: u8,
    /// Forma farmacéutica (comprimido, cápsula, colutorio…).
    #[serde(default)]
    pub form: String,
    /// Dosis por unidad («600 mg»).
    #[serde(default)]
    pub dosage: String,
    #[serde(default)]
    pub route: String,
    /// Unidades por envase («40 comprimidos»).
    #[serde(default)]
    pub pack: String,
    /// Posología estructurada: `take` `take_unit` `per` veces al `per_unit`.
    #[serde(default)]
    pub take: f64,
    #[serde(default)]
    pub take_unit: String,
    #[serde(default)]
    pub per: f64,
    #[serde(default)]
    pub per_unit: String,
    #[serde(default)]
    pub narcotic: bool,
    #[serde(default)]
    pub psychotropic: bool,
}

/// Lo que queda de una receta emitida por conexión directa (SREP).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SrepIssue {
    pub id_acceso: String,
    /// Copia de lo emitido y firmado (para reimprimir la hoja y la custodia).
    pub issued: Vec<crate::srep::RPrescripcion>,
    pub issued_at: Option<chrono::NaiveDateTime>,
    pub signer: String,
    pub test: bool,
    pub confidential: bool,
    /// Último estado conocido de cada receta (id → estado del repositorio).
    #[serde(default)]
    pub status: std::collections::BTreeMap<String, u8>,
    #[serde(default)]
    pub checked_at: Option<chrono::NaiveDateTime>,
    /// Actividad de las farmacias, en texto («06/11 · Farmacia F1 · Dispensación · 1 envase»).
    #[serde(default)]
    pub events: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prescription {
    pub id: u32,
    pub patient_id: u32,
    pub doctor_id: u32,
    pub date: NaiveDate,
    pub diagnosis: String,
    pub items: Vec<RxItem>,
    pub notes: String,
    /// Código de verificación (CVE) devuelto por la plataforma de receta electrónica.
    #[serde(default)]
    pub erx_code: String,
    #[serde(default)]
    pub erx_platform: String,
    #[serde(default)]
    pub erx_date: Option<NaiveDate>,
    /// Emitida por conexión directa con el Sistema de Receta Electrónica Privada.
    #[serde(default)]
    pub srep: Option<SrepIssue>,
}

/// «cada 8 horas» → 3 veces al día; «cada 12 h» → 2; «cada 24 horas» → 1.
pub fn per_day_from_text(freq: &str) -> Option<f64> {
    let f = fold(freq);
    let digits: String = f.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    let n: f64 = digits.parse().ok()?;
    if f.contains("hora") || f.contains(" h") || f.ends_with('h') {
        (n > 0.0 && n <= 24.0).then(|| (24.0 / n * 100.0).round() / 100.0)
    } else if f.contains("al dia") || f.contains("veces") {
        Some(n)
    } else {
        None
    }
}

/// Completa los datos estructurados de una línea a partir del vademécum.
pub fn rx_item_from_drug(d: &Drug) -> RxItem {
    let dosage = d.name.split_whitespace().skip_while(|w| !w.chars().next().is_some_and(|c| c.is_ascii_digit())).collect::<Vec<_>>().join(" ");
    let unit = d.dose.split_whitespace().skip(1).collect::<Vec<_>>().join(" ");
    RxItem {
        drug: d.name.clone(),
        dose: d.dose.clone(),
        frequency: d.frequency.clone(),
        days: d.days,
        units: 1,
        form: d.form.trim_end_matches('s').to_string(),
        dosage,
        route: if d.form.contains("colutorio") || d.form.contains("gel") { "bucal".into() } else { "oral".into() },
        take: d.dose.split_whitespace().next().and_then(|x| x.replace(',', ".").parse().ok()).unwrap_or(1.0),
        take_unit: if unit.is_empty() { d.form.trim_end_matches('s').to_string() } else { unit },
        per: per_day_from_text(&d.frequency).unwrap_or(1.0),
        per_unit: "día".into(),
        ..Default::default()
    }
}

pub fn default_drugs() -> Vec<Drug> {
    let d = |name: &str, form: &str, dose: &str, freq: &str, days: u32, group: &str| Drug { name: name.into(), form: form.into(), dose: dose.into(), frequency: freq.into(), days, group: group.into() };
    vec![
        d("Amoxicilina 500 mg", "cápsulas", "1 cápsula", "cada 8 horas", 7, "Antibiótico (penicilina)"),
        d("Amoxicilina/ácido clavulánico 875/125 mg", "comprimidos", "1 comprimido", "cada 8 horas", 7, "Antibiótico (penicilina)"),
        d("Clindamicina 300 mg", "cápsulas", "1 cápsula", "cada 8 horas", 7, "Antibiótico"),
        d("Azitromicina 500 mg", "comprimidos", "1 comprimido", "cada 24 horas", 3, "Antibiótico"),
        d("Metronidazol 500 mg", "comprimidos", "1 comprimido", "cada 8 horas", 7, "Antibiótico"),
        d("Ibuprofeno 600 mg", "comprimidos", "1 comprimido", "cada 8 horas con comida", 3, "Antiinflamatorio (AINE)"),
        d("Dexketoprofeno 25 mg", "comprimidos", "1 comprimido", "cada 8 horas", 3, "Antiinflamatorio (AINE)"),
        d("Paracetamol 1 g", "comprimidos", "1 comprimido", "cada 8 horas", 3, "Analgésico"),
        d("Metamizol 575 mg", "cápsulas", "1 cápsula", "cada 8 horas", 3, "Analgésico"),
        d("Omeprazol 20 mg", "cápsulas", "1 cápsula", "cada 24 horas en ayunas", 5, "Protector gástrico"),
        d("Clorhexidina 0,12% colutorio", "colutorio", "15 ml", "cada 12 horas, 30 segundos", 10, "Antiséptico"),
        d("Gel de clorhexidina 0,2%", "gel", "aplicar en la zona", "cada 12 horas", 7, "Antiséptico"),
        d("Dexametasona 4 mg", "comprimidos", "1 comprimido", "cada 24 horas", 2, "Corticoide"),
    ]
}

/// Avisos de seguridad de una receta según las alergias y alertas del paciente.
pub fn rx_warnings(p: &Patient, items: &[RxItem], drugs: &[Drug]) -> Vec<String> {
    let alg = fold(&p.allergies);
    let alerts = fold(&p.alerts);
    let mut w = vec![];
    for it in items {
        let name = fold(&it.drug);
        let group = drugs.iter().find(|d| d.name == it.drug).map(|d| fold(&d.group)).unwrap_or_default();
        if alg.contains("penicilin") && (group.contains("penicilina") || name.contains("amoxicilin")) {
            w.push(format!("ALERGIA: el paciente es alérgico a penicilina y se prescribe {}", it.drug));
        }
        if (alg.contains("aine") || alg.contains("ibuprofeno") || alg.contains("aspirina")) && group.contains("aine") {
            w.push(format!("ALERGIA a antiinflamatorios y se prescribe {}", it.drug));
        }
        if !alg.is_empty() && alg.split(|c: char| !c.is_alphanumeric()).filter(|t| t.len() > 4).any(|t| name.contains(t)) {
            w.push(format!("Posible alergia a {}", it.drug));
        }
        if (alerts.contains("anticoag") || alerts.contains("sintrom") || alerts.contains("acenocumarol")) && group.contains("aine") {
            w.push(format!("Paciente anticoagulado: {} aumenta el riesgo de sangrado (valore paracetamol)", it.drug));
        }
        if alerts.contains("embaraz") && (name.contains("metronidazol") || group.contains("aine")) {
            w.push(format!("Embarazo: revise la indicación de {}", it.drug));
        }
        if alerts.contains("hipertens") && group.contains("aine") && it.days > 5 {
            w.push(format!("Hipertensión: evite tratamientos largos con {}", it.drug));
        }
    }
    w.sort();
    w.dedup();
    w
}

impl Db {
    pub fn expenses_between(&self, from: NaiveDate, to: NaiveDate) -> Vec<&Expense> {
        self.expenses.iter().filter(|e| e.date >= from && e.date <= to).collect()
    }

    /// Al recibir un pedido o un trabajo de laboratorio se apunta el gasto.
    pub fn expense_from_order(&mut self, order_id: u32) {
        let Some(o) = self.orders.iter().find(|o| o.id == order_id).cloned() else { return };
        let id = self.new_id();
        let e = Expense { id, date: today(), supplier: self.supplier_name(o.supplier_id), concept: format!("Pedido #{}", o.id), category: "Material".into(), base: o.total(), vat_pct: 21.0, invoice_no: String::new(), method: "Transferencia".into(), paid: false, due: Some(today() + Duration::days(30)) };
        self.expenses.push(e);
    }

    /// Gasto de un trabajo de laboratorio ya recibido: se apunta en cuanto tiene
    /// coste (aunque se escriba después, al llegar la factura del laboratorio) y
    /// se corrige si el coste cambia mientras no esté pagado.
    pub fn sync_lab_expense(&mut self, work_id: u32) {
        let Some(w) = self.lab_works.iter().find(|w| w.id == work_id).cloned() else { return };
        if !matches!(w.state, crate::stock::LabState::Recibido | crate::stock::LabState::Colocado) {
            return;
        }
        let tag = format!("#L{}", w.id);
        match self.expenses.iter().position(|e| e.concept.ends_with(&tag)) {
            Some(i) if !self.expenses[i].paid => {
                if w.cost > 0.0 {
                    self.expenses[i].base = round2(w.cost);
                } else {
                    self.expenses.remove(i);
                }
            }
            Some(_) => {}
            None => self.expense_from_lab(work_id),
        }
    }

    fn expense_from_lab(&mut self, work_id: u32) {
        let Some(w) = self.lab_works.iter().find(|w| w.id == work_id).cloned() else { return };
        if w.cost <= 0.0 || self.expenses.iter().any(|e| e.concept.ends_with(&format!("#L{}", w.id))) {
            return;
        }
        let id = self.new_id();
        // las prótesis dentales a medida de laboratorio están exentas de IVA
        let e = Expense { id, date: today(), supplier: self.supplier_name(w.lab_id), concept: format!("{} · {} #L{}", w.description, self.patient_name(w.patient_id), w.id), category: "Laboratorio".into(), base: w.cost, vat_pct: 0.0, invoice_no: String::new(), method: "Transferencia".into(), paid: false, due: Some(today() + Duration::days(30)) };
        self.expenses.push(e);
    }

    pub fn new_prescription(&mut self, pid: u32, doctor_id: u32) -> u32 {
        let id = self.new_id();
        self.prescriptions.push(Prescription { id, patient_id: pid, doctor_id, date: today(), diagnosis: String::new(), items: vec![], notes: String::new(), erx_code: String::new(), erx_platform: String::new(), erx_date: None, srep: None });
        id
    }
}

pub fn sample_finance(db: &mut Db, today: NaiveDate) {
    let e = |db: &mut Db, ago: i64, sup: &str, concept: &str, cat: &str, base: f64, vat: f64, paid: bool| {
        let id = db.new_id();
        db.expenses.push(Expense { id, date: today - Duration::days(ago), supplier: sup.into(), concept: concept.into(), category: cat.into(), base, vat_pct: vat, invoice_no: format!("F-{:05}", 1000 + id % 1000), method: "Transferencia".into(), paid, due: if paid { None } else { Some(today + Duration::days(10)) } });
    };
    for m in 0..4 {
        let o = m * 30;
        e(db, 2 + o, "Inmobiliaria Ejemplo", "Alquiler del local", "Alquiler", 650.0, 21.0, true);
        e(db, 5 + o, "Compañía eléctrica", "Electricidad", "Suministros", 85.0 + m as f64 * 6.0, 21.0, true);
        e(db, 1 + o, "Gestoría", "Nóminas del mes", "Nóminas", 900.0, 0.0, true);
    }
    e(db, 37, "Depósito Dental Ejemplo", "Pedido de material", "Material", 130.0, 21.0, true);
    e(db, 12, "Laboratorio Dental Arte", "Corona zirconio · Laura", "Laboratorio", 160.0, 0.0, false);
    e(db, 20, "Agencia Ejemplo", "Campaña en redes sociales", "Marketing", 300.0, 21.0, true);
    e(db, 8, "Técnico Ejemplo", "Revisión del compresor", "Mantenimiento", 95.0, 21.0, false);
    // receta de ejemplo
    let id = db.new_prescription(101, 2);
    if let Some(r) = db.prescriptions.iter_mut().find(|r| r.id == id) {
        r.date = today - Duration::days(15);
        r.diagnosis = "Cirugía de implante 36".into();
        r.items = vec![
            RxItem { notes: "Alérgico a penicilina".into(), ..rx_item_from_drug(&Drug { name: "Clindamicina 300 mg".into(), form: "cápsulas".into(), dose: "1 cápsula".into(), frequency: "cada 8 horas".into(), days: 7, group: String::new() }) },
            RxItem { notes: "Anticoagulado: evitar AINE".into(), ..rx_item_from_drug(&Drug { name: "Paracetamol 1 g".into(), form: "comprimidos".into(), dose: "1 comprimido".into(), frequency: "cada 8 horas".into(), days: 3, group: String::new() }) },
        ];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allergy_and_anticoagulant_warnings() {
        let db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap());
        let p = db.patient(101).unwrap().clone(); // alérgico a penicilina y anticoagulado
        let items = vec![
            RxItem { drug: "Amoxicilina 500 mg".into(), dose: "1".into(), frequency: "8h".into(), days: 7, units: 1, ..Default::default() },
            RxItem { drug: "Ibuprofeno 600 mg".into(), dose: "1".into(), frequency: "8h".into(), days: 3, units: 1, ..Default::default() },
        ];
        let w = rx_warnings(&p, &items, &db.drugs);
        assert!(w.iter().any(|x| x.contains("ALERGIA")));
        assert!(w.iter().any(|x| x.contains("anticoagulado")));
        let ok = vec![RxItem { drug: "Paracetamol 1 g".into(), dose: "1".into(), frequency: "8h".into(), days: 3, units: 1, ..Default::default() }];
        assert!(rx_warnings(&p, &ok, &db.drugs).is_empty());
    }

    #[test]
    fn structured_posology_from_vademecum() {
        assert_eq!(per_day_from_text("cada 8 horas"), Some(3.0));
        assert_eq!(per_day_from_text("cada 12 horas, 30 segundos"), Some(2.0));
        assert_eq!(per_day_from_text("cada 24 horas en ayunas"), Some(1.0));
        assert_eq!(per_day_from_text("3 veces al día"), Some(3.0));
        assert_eq!(per_day_from_text("según dolor"), None);
        let d = default_drugs().into_iter().find(|d| d.name == "Ibuprofeno 600 mg").unwrap();
        let it = rx_item_from_drug(&d);
        assert_eq!((it.dosage.as_str(), it.form.as_str(), it.take, it.take_unit.as_str(), it.per), ("600 mg", "comprimido", 1.0, "comprimido", 3.0));
        let c = default_drugs().into_iter().find(|d| d.name.starts_with("Clorhexidina")).unwrap();
        assert_eq!(rx_item_from_drug(&c).route, "bucal");
    }

    #[test]
    fn expense_totals() {
        let e = Expense { id: 1, date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), supplier: String::new(), concept: String::new(), category: "Material".into(), base: 100.0, vat_pct: 21.0, invoice_no: String::new(), method: String::new(), paid: false, due: None };
        assert_eq!(e.total(), 121.0);
    }
}
