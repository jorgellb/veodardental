//! Pruebas de propiedades del dinero (T-R7-1): con miles de casos al azar
//! (semilla fija, así que se repiten igual), cada suma tiene que cuadrar al
//! céntimo y cada importe de línea tiene que ser el exacto, redondeado al
//! céntimo con las mitades lejos del cero.

use crate::cents::Cents;
use crate::model::*;
use crate::test_rng::Rng;

/// El importe exacto de una línea, con enteros: precio en céntimos ×
/// unidades × (100 − descuento %), con el descuento en centésimas de
/// punto, y redondeado al céntimo (mitades lejos del cero).
fn exact_line(price_cents: i64, units: i64, discount_bp: i64) -> i64 {
    let num = price_cents * units * (10_000 - discount_bp);
    let (q, r) = (num / 10_000, num % 10_000);
    if r.abs() * 2 >= 10_000 { q + num.signum() } else { q }
}

/// Un importe al azar en céntimos, a veces con medias de céntimo al aplicar
/// descuentos (precios acabados en 5, descuentos del 10, 30, 50 %).
fn price(rng: &mut Rng) -> i64 {
    match rng.below(4) {
        0 => rng.below(100) as i64 + 1,
        1 => (rng.below(2000) as i64) * 5 + 5,
        2 => rng.below(500_000) as i64 + 1,
        _ => rng.below(5_000) as i64 * 100 + 99,
    }
}

fn discount(rng: &mut Rng) -> i64 {
    [0, 500, 1000, 1250, 3000, 3333, 5000, 7500][rng.below(8)]
}

/// Los datos de ejemplo, de donde salen las plantillas de los registros.
fn sample() -> Db {
    Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 16).unwrap())
}

#[test]
fn line_amounts_are_exact_to_the_cent() {
    let mut rng = Rng(0x5eed_7101);
    let template = sample().entries[0].clone();
    let mut bad = vec![];
    for _ in 0..20_000 {
        let (p, u, d) = (price(&mut rng), rng.below(12) as i64 + 1, discount(&mut rng));
        let e = MedEntry { price: p as f64 / 100.0, units: u as u32, discount_pct: d as f64 / 100.0, exact: true, ..template.clone() };
        let got = Cents::from_eur(e.amount()).0;
        let want = exact_line(p, u, d);
        if got != want {
            bad.push(format!("{} € × {u} − {} %: {got} en vez de {want}", p as f64 / 100.0, d as f64 / 100.0));
        }
    }
    assert!(bad.is_empty(), "{} importes mal redondeados, por ejemplo: {:?}", bad.len(), &bad[..bad.len().min(5)]);
}

#[test]
fn installments_plus_down_payment_add_up_to_the_total() {
    let mut rng = Rng(0x5eed_7102);
    for _ in 0..20_000 {
        let total = price(&mut rng) * (rng.below(5) as i64 + 1);
        let down = if rng.below(3) == 0 { 0 } else { rng.below(total as usize) as i64 };
        let n = rng.below(60) as u32 + 1;
        let plan = FinancePlan { down_payment: down as f64 / 100.0, installments: n, first_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), every_months: 1, paid: vec![false; n as usize], exact: true };
        let t = total as f64 / 100.0;
        let parts: Vec<i64> = (0..n).map(|i| Cents::from_eur(plan.installment(i, t)).0).collect();
        assert_eq!(parts.iter().sum::<i64>() + down, total, "{t} € con {down} de entrada en {n} cuotas: {parts:?}");
        assert!(parts.iter().all(|c| *c >= 0), "ninguna cuota en negativo");
        let each = parts[0];
        assert!(parts.iter().all(|c| (c - each).abs() <= n as i64), "la última solo completa los céntimos");
    }
}

#[test]
fn budgets_and_invoices_add_up_line_by_line() {
    let mut rng = Rng(0x5eed_7103);
    let db = sample();
    let (tb, ti) = (db.budgets[0].clone(), db.invoices[0].clone());
    for _ in 0..3_000 {
        let n = rng.below(30) + 1;
        let lines: Vec<(i64, i64, i64, f64)> = (0..n).map(|_| (price(&mut rng), rng.below(5) as i64 + 1, discount(&mut rng), [0.0, 4.0, 10.0, 21.0][rng.below(4)])).collect();
        // presupuesto: el total es la suma de sus líneas
        let b = Budget {
            lines: lines
                .iter()
                .map(|(p, u, d, _)| BudgetLine { code: String::new(), description: String::new(), teeth: vec![], units: *u as u32, price: *p as f64 / 100.0, discount_pct: *d as f64 / 100.0, odg: OdgKind::Ninguno, phase: 0, exact: true })
                .collect(),
            ..tb.clone()
        };
        let want: i64 = lines.iter().map(|(p, u, d, _)| exact_line(*p, *u, *d)).sum();
        assert_eq!(Cents::from_eur(b.total()).0, want, "presupuesto de {n} líneas");
        // factura: base e IVA por línea, y el total es base + IVA
        let inv = Invoice {
            lines: lines.iter().map(|(p, u, d, v)| InvoiceLine { code: String::new(), description: String::new(), teeth: vec![], units: *u as u32, price: *p as f64 / 100.0, discount_pct: *d as f64 / 100.0, vat_pct: *v, exact: true }).collect(),
            ..ti.clone()
        };
        let base: i64 = inv.lines.iter().map(|l| Cents::from_eur(l.base()).0).sum();
        let vat: i64 = inv.lines.iter().map(|l| Cents::from_eur(l.vat()).0).sum();
        assert_eq!(Cents::from_eur(inv.base()).0, base, "la base es la suma de las bases");
        assert_eq!(Cents::from_eur(inv.vat()).0, vat, "el IVA es la suma del de cada línea");
        assert_eq!(Cents::from_eur(inv.total()).0, base + vat, "total = base + IVA");
    }
}

/// La caja: lo esperado es la apertura más los cobros en efectivo y los
/// movimientos, al céntimo; una anulación deja el saldo como estaba.
#[test]
fn cash_and_cancellations_balance_to_the_cent() {
    let mut rng = Rng(0x5eed_7104);
    let mut db = sample();
    db.cash.clear();
    let s = db.open_cash_session(100.0);
    let mut want = 10_000i64;
    let pid = db.patients[0].id;
    let before = Cents::from_eur(db.patient_balance(pid));
    for _ in 0..2_000 {
        let a = price(&mut rng);
        let cash = rng.below(2) == 0;
        let id = db.add_payment(pid, a as f64 / 100.0, if cash { "Efectivo" } else { "Tarjeta" }, "propiedad");
        if cash {
            want += a;
        }
        if rng.below(10) == 0 {
            db.cancel_payment(id, "propiedad", true).unwrap();
            if cash {
                want -= a;
            }
        }
    }
    let session = db.cash.iter().find(|c| c.id == s).unwrap().clone();
    assert_eq!(Cents::from_eur(db.cash_expected(&session)).0, want, "la caja esperada, al céntimo");
    let paid: i64 = db.payments.iter().filter(|p| p.patient_id == pid && p.concept.contains("propiedad")).map(|p| Cents::from_eur(p.amount).0).sum();
    assert_eq!(Cents::from_eur(db.patient_balance(pid)), before - Cents(paid), "el saldo del paciente baja justo lo cobrado (y lo anulado no cuenta)");
}

/// Una remesa: su total es la suma de sus recibos, al céntimo.
#[test]
fn a_remittance_adds_up_its_items() {
    let mut rng = Rng(0x5eed_7105);
    for _ in 0..2_000 {
        let n = rng.below(200) + 1;
        let amounts: Vec<i64> = (0..n).map(|_| price(&mut rng)).collect();
        let r = crate::extra::Remittance {
            id: 1,
            created: crate::model::now(),
            collection: crate::model::today(),
            items: amounts
                .iter()
                .map(|a| crate::extra::RemItem {
                    patient_id: 1,
                    budget_id: 1,
                    installment: 0,
                    amount: *a as f64 / 100.0,
                    iban: String::new(),
                    mandate: String::new(),
                    mandate_date: crate::model::today(),
                    end_to_end: String::new(),
                    returned: None,
                    return_reason: String::new(),
                })
                .collect(),
            state: crate::extra::RemState::Generada,
            file: String::new(),
        };
        assert_eq!(Cents::from_eur(r.total()).0, amounts.iter().sum::<i64>());
    }
}

/// Lo de antes no cambia (D-29): una factura guardada sin la marca conserva su
/// total y su huella VERI*FACTU aunque su línea sea de las que el cálculo de
/// antes redondeaba mal; una nueva lleva el importe exacto.
#[test]
fn old_records_keep_their_amounts_and_the_invoice_chain_its_hashes() {
    let db = sample();
    let mut inv = db.invoices[0].clone();
    inv.lines = vec![InvoiceLine { code: "X".into(), description: "x".into(), teeth: vec![], units: 2, price: 27.45, discount_pct: 5.0, vat_pct: 0.0, exact: false }];
    let legacy_json = serde_json::to_string(&inv).unwrap();
    assert!(!legacy_json.contains("exact"), "una factura de antes no lleva la marca");
    let back: Invoice = serde_json::from_str(&legacy_json).unwrap();
    assert_eq!(back.total(), 52.15, "como siempre (aunque lo exacto sea 52,16)");
    assert_eq!(back.canonical("B12345678"), inv.canonical("B12345678"), "la misma huella");
    let mut new = inv.clone();
    new.lines[0].exact = true;
    assert_eq!(new.total(), 52.16, "la nueva, exacta");
    // y la cadena de facturas de los datos de ejemplo sigue íntegra
    assert!(crate::verifactu::verify(&db).is_ok());
    // los tratamientos y los planes de antes, igual
    let mut e = db.entries[0].clone();
    (e.price, e.units, e.discount_pct, e.exact) = (27.45, 2, 5.0, false);
    assert_eq!(e.amount(), 52.15);
    let old_plan: FinancePlan = serde_json::from_str(r#"{"down_payment":0.0,"installments":58,"first_date":"2026-01-01","every_months":1,"paid":[]}"#).unwrap();
    assert!(!old_plan.exact, "un plan de antes, sin la marca");
}
