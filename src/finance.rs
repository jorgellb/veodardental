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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
    /// Código ATC (de la lista oficial): con él se reconocen el grupo del
    /// medicamento para los avisos (penicilinas J01C, antiinflamatorios M01A).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub atc: String,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// Avisos de seguridad que el profesional revisó antes de imprimir o
    /// emitir, quién y cuándo.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewed_warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reviewed_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<chrono::NaiveDateTime>,
}

impl Prescription {
    /// Emitida: con el código de la plataforma o por conexión directa. Ya es
    /// el registro de lo prescrito: no se cambia ni se borra.
    pub fn issued(&self) -> bool {
        self.srep.is_some() || !self.erx_code.trim().is_empty()
    }
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

/// Por qué no puede recetar este colaborador (`None`: puede). Solo recetan
/// médicos, odontólogos y podólogos (RD 1718/2010, art. 1; RDL 1/2015, art.
/// 79.1), de alta y con su número de colegiado.
pub fn collaborator_problem(c: &Collaborator) -> Option<String> {
    let spec = fold(&c.specialty);
    if ["higien", "auxiliar", "protes", "recepc", "administ"].iter().any(|k| spec.contains(k)) {
        return Some(format!("{} no puede recetar: solo recetan odontólogos, médicos y podólogos.", c.name));
    }
    if !c.active {
        return Some(format!("{} está de baja.", c.name));
    }
    let lic = c.license.trim();
    if lic.is_empty() || lic == "-" {
        return Some(format!("Falta el nº de colegiado de {} (Colaboradores).", c.name));
    }
    None
}

/// Avisos de seguridad de una receta según las alergias y alertas del paciente.
pub fn rx_warnings(p: &Patient, items: &[RxItem], drugs: &[Drug]) -> Vec<String> {
    let alg = fold(&p.allergies);
    let alerts = fold(&p.alerts);
    let mut w = vec![];
    for it in items {
        let name = fold(&it.drug);
        let mut group = drugs.iter().find(|d| d.name == it.drug).map(|d| fold(&d.group)).unwrap_or_default();
        // los de la lista oficial no están en el vademécum: su grupo sale del
        // código ATC (J01C penicilinas, M01A antiinflamatorios no esteroideos)
        let atc = it.atc.trim().to_uppercase();
        if atc.starts_with("J01C") {
            group.push_str(" penicilina");
        }
        if atc.starts_with("M01A") {
            group.push_str(" aine");
        }
        if alg.contains("penicilin") && (group.contains("penicilina") || name.contains("amoxicilin") || name.contains("penicilin")) {
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
    /// Anula un cobro equivocado: un cobro en negativo con la misma forma de
    /// pago (si es efectivo, sale de la caja abierta) y el motivo. Si pagaba una
    /// cuota, la cuota vuelve a estar por pagar. Los cobros de hoy (en efectivo,
    /// de la caja que sigue abierta) los anula quien lleva la caja; los de otro
    /// día o de una caja ya cerrada, solo Dirección (`admin`). Devuelve la anulación.
    pub fn cancel_payment(&mut self, id: u32, reason: &str, admin: bool) -> Result<u32, String> {
        let p = self.payments.iter().find(|p| p.id == id).cloned().ok_or("El cobro ya no existe.")?;
        let open = self.open_cashes().iter().map(|c| c.id).collect::<Vec<_>>();
        let current = p.date.date() == today() && (p.method != "Efectivo" || p.cash_session.is_some_and(|s| open.contains(&s)));
        if !admin && !current {
            return Err("Solo Dirección puede anular un cobro de otro día o de una caja ya cerrada.".into());
        }
        if reason.trim().is_empty() {
            return Err("Escriba el motivo de la anulación.".into());
        }
        if p.amount <= 0.0 || p.cancels.is_some() {
            return Err("Solo se anulan cobros (no anulaciones ni devoluciones).".into());
        }
        if self.payments.iter().any(|x| x.cancels == Some(id)) {
            return Err(format!("El recibo {} ya está anulado.", p.receipt_no));
        }
        if p.method == "Domiciliación" {
            return Err("Los cobros de una remesa se corrigen en Cobros › Remesas SEPA (recibo devuelto).".into());
        }
        let back = self.add_payment(p.patient_id, -p.amount, &p.method, &format!("Anulación del recibo {}: {}", p.receipt_no, reason.trim()));
        if let Some(x) = self.payments.iter_mut().find(|x| x.id == back) {
            x.cancels = Some(id);
        }
        if let Some((bid, i)) = p.installment {
            self.set_installment_paid(bid, i, false);
        }
        let who = self.patient_name(p.patient_id);
        self.audit("Anular cobro", &format!("{} · recibo {} · {} · {}", who, p.receipt_no, money(p.amount), reason.trim()));
        Ok(back)
    }

    /// Devuelve al paciente dinero que tiene a favor (anticipos de un
    /// tratamiento que no se hace, por ejemplo). Devuelve el movimiento.
    pub fn refund(&mut self, pid: u32, amount: f64, method: &str, reason: &str) -> Result<u32, String> {
        let credit = round2(-self.patient_balance(pid));
        if amount <= 0.0 {
            return Err("Indique el importe que se devuelve.".into());
        }
        if amount > credit + 0.005 {
            return Err(format!("El paciente tiene {} a favor: no se le puede devolver más.", money(credit.max(0.0))));
        }
        if reason.trim().is_empty() {
            return Err("Escriba el motivo de la devolución.".into());
        }
        let id = self.add_payment(pid, -round2(amount), method, &format!("Devolución de saldo a favor: {}", reason.trim()));
        let who = self.patient_name(pid);
        self.audit("Devolver dinero", &format!("{} · {} · {} · {}", who, money(amount), method, reason.trim()));
        Ok(id)
    }

    pub fn expenses_between(&self, from: NaiveDate, to: NaiveDate) -> Vec<&Expense> {
        self.expenses.iter().filter(|e| e.date >= from && e.date <= to).collect()
    }

    /// Al recibir un pedido o un trabajo de laboratorio se apunta el gasto.
    pub fn expense_from_order(&mut self, order_id: u32) {
        let Some(o) = self.orders.iter().find(|o| o.id == order_id).cloned() else { return };
        let id = self.new_id();
        let e = Expense {
            id,
            date: today(),
            supplier: self.supplier_name(o.supplier_id),
            concept: format!("Pedido #{}", o.id),
            category: "Material".into(),
            // lo recibido (un pedido cerrado con lo que llegó); los de antes, entero
            base: if o.lines.iter().any(|l| l.received > 0.0) { round2(o.lines.iter().map(|l| l.received * l.cost).sum()) } else { o.total() },
            vat_pct: 21.0,
            invoice_no: String::new(),
            method: "Transferencia".into(),
            paid: false,
            due: Some(today() + Duration::days(30)),
        };
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
        // el gasto lleva el número del trabajo: si dos puestos lo marcan como
        // recibido a la vez, crean el mismo registro y queda uno solo
        let id = if self.expenses.iter().any(|e| e.id == w.id) { self.new_id() } else { w.id };
        // las prótesis dentales a medida de laboratorio están exentas de IVA
        let e = Expense {
            id,
            date: today(),
            supplier: self.supplier_name(w.lab_id),
            concept: format!("{} · {} #L{}", w.description, self.patient_name(w.patient_id), w.id),
            category: "Laboratorio".into(),
            base: w.cost,
            vat_pct: 0.0,
            invoice_no: String::new(),
            method: "Transferencia".into(),
            paid: false,
            due: Some(today() + Duration::days(30)),
        };
        self.expenses.push(e);
    }

    /// Receta nueva. Si quien la crea no puede recetar (una higienista), el
    /// prescriptor propuesto es el doctor del paciente o el primero que pueda.
    pub fn new_prescription(&mut self, pid: u32, doctor_id: u32) -> u32 {
        let id = self.new_id();
        let doctor_id = if self.prescriber_problem(doctor_id).is_none() {
            doctor_id
        } else {
            let own = self.patient(pid).map(|p| p.doctor_id).filter(|d| self.prescriber_problem(*d).is_none());
            own.or_else(|| self.prescribers().first().copied()).unwrap_or(doctor_id)
        };
        self.prescriptions.push(Prescription {
            id,
            patient_id: pid,
            doctor_id,
            date: today(),
            diagnosis: String::new(),
            items: vec![],
            notes: String::new(),
            erx_code: String::new(),
            erx_platform: String::new(),
            erx_date: None,
            srep: None,
            reviewed_warnings: vec![],
            reviewed_by: String::new(),
            reviewed_at: None,
        });
        id
    }

    /// Por qué no puede recetar este profesional (`None`: puede). Solo
    /// recetan médicos, odontólogos y podólogos (RD 1718/2010, art. 1; RDL
    /// 1/2015, art. 79.1), y con su número de colegiado.
    pub fn prescriber_problem(&self, doctor_id: u32) -> Option<String> {
        let Some(c) = self.doctor(doctor_id) else { return Some("Elija quién receta.".into()) };
        collaborator_problem(c)
    }

    /// Profesionales que pueden recetar.
    pub fn prescribers(&self) -> Vec<u32> {
        self.collaborators.iter().filter(|c| self.prescriber_problem(c.id).is_none()).map(|c| c.id).collect()
    }

    /// Avisos de seguridad de la receta que el profesional aún no ha revisado.
    pub fn rx_unreviewed(&self, id: u32) -> Vec<String> {
        let Some(r) = self.prescriptions.iter().find(|r| r.id == id) else { return vec![] };
        let Some(p) = self.patient(r.patient_id) else { return vec![] };
        rx_warnings(p, &r.items, &self.drugs).into_iter().filter(|w| !r.reviewed_warnings.contains(w)).collect()
    }

    /// El profesional ha revisado los avisos de la receta (los decide él: el
    /// programa no cambia la prescripción). Queda quién y cuándo.
    pub fn review_rx_warnings(&mut self, id: u32, by: &str) {
        let Some(r) = self.prescriptions.iter().find(|r| r.id == id) else { return };
        let Some(p) = self.patient(r.patient_id) else { return };
        let all = rx_warnings(p, &r.items, &self.drugs);
        let who = p.full_name();
        if let Some(r) = self.prescriptions.iter_mut().find(|r| r.id == id) {
            r.reviewed_warnings = all.clone();
            r.reviewed_by = by.to_string();
            r.reviewed_at = Some(now());
        }
        self.audit("Avisos de receta revisados", &format!("receta {id} de {who} · {by} · {}", all.join(" | ")));
    }

    /// Borra una receta sin emitir; la emitida queda en la historia.
    pub fn delete_prescription(&mut self, id: u32) -> Result<(), String> {
        let r = self.prescriptions.iter().find(|r| r.id == id).ok_or("La receta ya no existe.")?;
        if r.issued() {
            return Err("La receta ya está emitida: no se borra, queda en la historia del paciente.".into());
        }
        self.prescriptions.retain(|r| r.id != id);
        Ok(())
    }
}

pub fn sample_finance(db: &mut Db, today: NaiveDate) {
    let e = |db: &mut Db, ago: i64, sup: &str, concept: &str, cat: &str, base: f64, vat: f64, paid: bool| {
        let id = db.new_id();
        db.expenses.push(Expense {
            id,
            date: today - Duration::days(ago),
            supplier: sup.into(),
            concept: concept.into(),
            category: cat.into(),
            base,
            vat_pct: vat,
            invoice_no: format!("F-{:05}", 1000 + id % 1000),
            method: "Transferencia".into(),
            paid,
            due: if paid { None } else { Some(today + Duration::days(10)) },
        });
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
            RxItem {
                notes: "Anticoagulado: evitar AINE".into(),
                ..rx_item_from_drug(&Drug { name: "Paracetamol 1 g".into(), form: "comprimidos".into(), dose: "1 comprimido".into(), frequency: "cada 8 horas".into(), days: 3, group: String::new() })
            },
        ];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrong_payment_is_cancelled_and_leaves_the_cash_balanced() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let mut db = Db::sample(d);
        let cash = db.open_cash().unwrap().clone();
        let before = db.cash_expected(&cash);
        let bal = db.patient_balance(110);
        // se tecleó 1000 en vez de 100
        let wrong = db.add_payment(110, 1000.0, "Efectivo", "Pago a cuenta");
        assert!(db.cancel_payment(wrong, " ", false).unwrap_err().contains("motivo"));
        let back = db.cancel_payment(wrong, "Importe mal tecleado", false).unwrap();
        let b = db.payments.iter().find(|p| p.id == back).unwrap().clone();
        assert_eq!((b.amount, b.method.as_str(), b.cancels), (-1000.0, "Efectivo", Some(wrong)));
        assert!(b.concept.contains("Importe mal tecleado") && b.concept.contains(&db.payments.iter().find(|p| p.id == wrong).unwrap().receipt_no));
        assert!((db.patient_balance(110) - bal).abs() < 0.005, "el saldo queda como estaba");
        assert!((db.cash_expected(db.open_cash().unwrap()) - before).abs() < 0.005, "y la caja también");
        assert!(db.cancel_payment(wrong, "otra vez", true).unwrap_err().contains("anulado"), "no se anula dos veces");
        assert!(db.cancel_payment(back, "x", true).is_err(), "una anulación no se anula");
        assert!(db.audit_log.iter().any(|a| a.action == "Anular cobro"));
        // un cobro de otro día (o de una caja ya cerrada) solo lo anula Dirección
        let old = db.payments.iter().find(|p| p.amount > 0.0 && p.date.date() < today() && p.method != "Domiciliación" && !db.payments.iter().any(|x| x.cancels == Some(p.id))).unwrap().id;
        assert!(db.cancel_payment(old, "error", false).unwrap_err().contains("Dirección"));
        db.cancel_payment(old, "error", true).unwrap();
        let card = db.add_payment(110, 50.0, "Tarjeta", "hoy");
        db.cancel_payment(card, "error", false).unwrap();
    }

    #[test]
    fn credit_is_refunded_up_to_what_the_patient_has_in_favour() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let mut db = Db::sample(d);
        let pid = 110;
        let bal = db.patient_balance(pid);
        db.add_payment(pid, bal + 300.0, "Tarjeta", "Anticipo implante");
        let credit = -db.patient_balance(pid);
        assert!((credit - 300.0).abs() < 0.005);
        assert!(db.refund(pid, credit + 1.0, "Tarjeta", "No sigue el tratamiento").unwrap_err().contains("a favor"));
        assert!(db.refund(pid, 100.0, "Tarjeta", "").unwrap_err().contains("motivo"));
        let r = db.refund(pid, credit, "Transferencia", "No sigue el tratamiento").unwrap();
        assert_eq!(db.payments.iter().find(|p| p.id == r).unwrap().amount, -300.0);
        assert!(db.patient_balance(pid).abs() < 0.005);
        assert!(db.audit_log.iter().any(|a| a.action == "Devolver dinero"));
    }

    /// Solo receta un odontólogo o un médico con nº de colegiado (RD 1718/2010,
    /// art. 1; RDL 1/2015, art. 79.1): ni una higienista ni quien no tenga número.
    #[test]
    fn only_a_dentist_or_doctor_with_licence_prescribes() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let mut db = Db::sample(d);
        let hyg = db.collaborators.iter().find(|c| fold(&c.specialty).contains("higien")).map(|c| c.id).unwrap();
        let dentist = db.collaborators.iter().find(|c| !fold(&c.specialty).contains("higien")).map(|c| c.id).unwrap();
        assert!(db.prescriber_problem(hyg).unwrap().contains("no puede recetar"));
        assert_eq!(db.prescriber_problem(dentist), None);
        db.collaborators.iter_mut().find(|c| c.id == dentist).unwrap().license = "-".into();
        assert!(db.prescriber_problem(dentist).unwrap().contains("colegiado"));
        db.collaborators.iter_mut().find(|c| c.id == dentist).unwrap().license = "28-0137".into();
        // la higienista crea la receta: el prescriptor propuesto es quien puede recetar
        let id = db.new_prescription(101, hyg);
        let r = db.prescriptions.iter().find(|r| r.id == id).unwrap();
        assert_eq!(db.prescriber_problem(r.doctor_id), None, "propone un prescriptor válido, no a la higienista");
    }

    /// Los avisos de alergia y de anticoagulación valen también para los
    /// medicamentos de la lista oficial (por su código ATC), no solo para los
    /// del vademécum de la clínica.
    #[test]
    fn official_list_drugs_get_the_same_warnings() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let db = Db::sample(d);
        let mut p = db.patient(101).unwrap().clone();
        p.allergies = "Penicilina".into();
        p.alerts = "Anticoagulado (Sintrom)".into();
        let cloxa = RxItem { drug: "CLOXACILINA NORMON 500 mg CAPSULAS".into(), atc: "J01CF02".into(), ..Default::default() };
        let diclo = RxItem { drug: "DICLOFENACO NORMON 50 mg COMPRIMIDOS".into(), atc: "M01AB05".into(), ..Default::default() };
        let w = rx_warnings(&p, &[cloxa, diclo], &db.drugs);
        assert!(w.iter().any(|x| x.contains("penicilina") && x.contains("CLOXACILINA")), "{w:?}");
        assert!(w.iter().any(|x| x.contains("anticoagulado") && x.contains("DICLOFENACO")), "{w:?}");
        // el código ATC llega a la línea desde la lista oficial
        let m = crate::nomenclator::Medicine { name: "DICLOFENACO X".into(), atc: "M01AB05".into(), ..Default::default() };
        assert_eq!(crate::nomenclator::rx_item(&m).atc, "M01AB05");
    }

    /// El profesional revisa los avisos antes de imprimir o emitir, y queda
    /// apuntado quién y cuándo; si cambian los avisos, hay que revisarlos otra vez.
    #[test]
    fn warnings_are_reviewed_before_issuing() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let mut db = Db::sample(d);
        db.patient_mut(101).unwrap().allergies = "Penicilina".into();
        let id = db.new_prescription(101, 1);
        db.prescriptions.iter_mut().find(|r| r.id == id).unwrap().items.push(rx_item_from_drug(&db.drugs[0].clone()));
        assert_eq!(db.rx_unreviewed(id).len(), 1);
        db.review_rx_warnings(id, "Dra. Ruiz");
        assert!(db.rx_unreviewed(id).is_empty());
        let r = db.prescriptions.iter().find(|r| r.id == id).unwrap();
        assert_eq!(r.reviewed_by, "Dra. Ruiz");
        assert!(r.reviewed_at.is_some());
        assert!(db.audit_log.iter().any(|a| a.action == "Avisos de receta revisados"));
        // otro medicamento con aviso: hay que revisarlo de nuevo
        db.patient_mut(101).unwrap().alerts = "Anticoagulado".into();
        let ibu = db.drugs.iter().find(|x| x.name.starts_with("Ibuprofeno")).cloned().unwrap();
        db.prescriptions.iter_mut().find(|r| r.id == id).unwrap().items.push(rx_item_from_drug(&ibu));
        assert_eq!(db.rx_unreviewed(id).len(), 1);
    }

    /// Una receta emitida (con código de la plataforma o por conexión directa)
    /// no se borra ni se cambia: es el registro de lo prescrito.
    #[test]
    fn an_issued_prescription_is_kept_as_issued() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let mut db = Db::sample(d);
        let id = db.new_prescription(101, 1);
        db.prescriptions.iter_mut().find(|r| r.id == id).unwrap().items.push(rx_item_from_drug(&db.drugs[7].clone()));
        assert!(db.delete_prescription(id).is_ok(), "sin emitir se puede borrar");
        let id = db.new_prescription(101, 1);
        let r = db.prescriptions.iter_mut().find(|r| r.id == id).unwrap();
        r.items.push(rx_item_from_drug(&db.drugs[7].clone()));
        r.erx_code = "AB12CD34EF".into();
        assert!(db.prescriptions.iter().find(|r| r.id == id).unwrap().issued());
        assert!(db.delete_prescription(id).unwrap_err().contains("emitida"));
        // en red: un puesto no la cambia ni la borra; el estado en las farmacias sí se actualiza
        let cur = serde_json::to_string(db.prescriptions.iter().find(|r| r.id == id).unwrap()).unwrap();
        let mut changed: Prescription = serde_json::from_str(&cur).unwrap();
        changed.items[0].days = 30;
        let kept = crate::firmado::protect_json("prescriptions", &cur, Some(&serde_json::to_string(&changed).unwrap())).expect("se corrige");
        assert_eq!(serde_json::from_str::<Prescription>(&kept).unwrap().items[0].days, db.drugs[7].days);
        assert!(crate::firmado::protect_json("prescriptions", &cur, None).is_some(), "no se borra");
    }

    /// Una receta emitida no deja de estarlo: un puesto no puede borrar su
    /// código (y después cambiarla); solo corregirlo por otro.
    #[test]
    fn an_issued_prescription_cannot_be_un_issued() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let mut db = Db::sample(d);
        let id = db.new_prescription(101, 1);
        let r = db.prescriptions.iter_mut().find(|r| r.id == id).unwrap();
        r.items.push(rx_item_from_drug(&default_drugs()[7]));
        r.erx_code = "AB12CD34EF".into();
        let cur = serde_json::to_string(r).unwrap();
        let mut cleared: Prescription = serde_json::from_str(&cur).unwrap();
        cleared.erx_code.clear();
        let kept: Prescription = serde_json::from_str(&crate::firmado::protect_json("prescriptions", &cur, Some(&serde_json::to_string(&cleared).unwrap())).expect("se corrige")).unwrap();
        assert_eq!(kept.erx_code, "AB12CD34EF", "el código no se borra");
        let mut fixed: Prescription = serde_json::from_str(&cur).unwrap();
        fixed.erx_code = "ZZ98YY76XX".into();
        assert_eq!(crate::firmado::protect_json("prescriptions", &cur, Some(&serde_json::to_string(&fixed).unwrap())), None, "corregirlo por otro sí");
    }

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
        let e = Expense {
            id: 1,
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            supplier: String::new(),
            concept: String::new(),
            category: "Material".into(),
            base: 100.0,
            vat_pct: 21.0,
            invoice_no: String::new(),
            method: String::new(),
            paid: false,
            due: None,
        };
        assert_eq!(e.total(), 121.0);
    }
}
