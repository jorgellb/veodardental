//! Totales económicos de los informes, con los mismos filtros en todas las
//! pantallas que los muestran: periodo, centro y profesional. Se suman en
//! céntimos enteros ([`Cents`]).
//!
//! Qué entra en cada total:
//! - **Cobrado:** los cobros con fecha dentro del periodo, ambos días incluidos.
//!   La fecha es la local del ordenador que registró el cobro (puesto o
//!   servidor). Incluye lo cobrado por remesa (domiciliación). Las anulaciones
//!   y devoluciones (a pacientes o recibos devueltos) van en negativo, en el
//!   día en que se hacen. El neto es el dinero que de verdad entró. Los cobros
//!   no son de un profesional, así que el filtro de profesional no les afecta.
//! - **Producción:** los tratamientos realizados con fecha dentro del periodo,
//!   en el centro donde se hicieron.
//!   - La parte del paciente es su importe con su tarifa y su descuento.
//!   - La parte de la mutua es el baremo que paga la compañía por el acto, si
//!     el paciente era de una mutua el día que se hizo.
//!   - Lo previsto y lo de otros periodos no entra.
//! - **Facturado:** las facturas con fecha de expedición dentro del periodo.
//!   Las rectificativas van en negativo, y el neto es facturas menos
//!   rectificativas. Las facturas no son de un centro ni de un profesional:
//!   esos filtros no les afectan.
//! - **Deuda a una fecha:** por paciente, lo realizado hasta esa fecha menos
//!   lo cobrado hasta esa fecha.
//!   - Se suman por separado lo que deben los pacientes con saldo deudor y los
//!     anticipos de los que tienen saldo a favor: lo uno no compensa lo otro.
//!   - La deuda es del paciente, no de un centro.
//! - **Mutuas:**
//!   - liquidado: las liquidaciones con fecha dentro del periodo;
//!   - cobrado: lo cobrado de las liquidaciones con fecha de cobro dentro del
//!     periodo;
//!   - pendiente de cobro: lo que queda hoy de todas;
//!   - sin liquidar: el baremo de los actos realizados en el periodo que aún
//!     no están en ninguna liquidación.

use crate::cents::Cents;
use crate::model::*;
use chrono::NaiveDate;
use std::collections::HashMap;

/// Periodo (ambos días incluidos) y, si se eligen, centro y profesional.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filter {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub center: Option<u32>,
    pub doctor: Option<u32>,
}

impl Filter {
    pub fn period(from: NaiveDate, to: NaiveDate) -> Filter {
        Filter { from, to, center: None, doctor: None }
    }

    fn day(&self, d: NaiveDate) -> bool {
        d >= self.from && d <= self.to
    }

    /// (0 = centro principal, como en el resto del programa)
    fn center(&self, c: u32) -> bool {
        self.center.is_none_or(|f| cid(f) == cid(c))
    }
}

/// Formas de pago en el orden en que se muestran (las de la clínica y la
/// domiciliación de las remesas).
pub const METHODS: [&str; 6] = ["Efectivo", "Tarjeta", "Transferencia", "Financiación", "Bizum", "Domiciliación"];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Collected {
    /// Neto por forma de pago, en el orden de [`METHODS`] (y después
    /// cualquier otra que haya en los datos).
    pub by_method: Vec<(String, Cents)>,
    /// Cobros (sin anulaciones ni devoluciones).
    pub charged: Cents,
    /// Anulaciones de cobros (en negativo).
    pub cancelled: Cents,
    /// Devoluciones a pacientes y recibos devueltos por el banco (en negativo).
    pub returned: Cents,
    pub net: Cents,
    pub count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Production {
    pub acts: usize,
    pub patient: Cents,
    pub insurer: Cents,
}

impl Production {
    pub fn total(&self) -> Cents {
        self.patient + self.insurer
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Invoiced {
    pub invoices: usize,
    pub rectifying: usize,
    /// Total de las facturas (sin rectificativas).
    pub issued: Cents,
    /// Total de las rectificativas (en negativo).
    pub rectified: Cents,
    pub base: Cents,
    pub vat: Cents,
    pub net: Cents,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Debt {
    /// Lo que deben los pacientes con saldo deudor.
    pub owed: Cents,
    pub debtors: usize,
    /// Saldo a favor (anticipos) de los que han pagado de más.
    pub credit: Cents,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Insurers {
    pub claimed: Cents,
    pub collected: Cents,
    pub pending: Cents,
    pub unclaimed: Cents,
}

impl Db {
    /// Cobrado en el periodo y el centro (ver el módulo).
    pub fn report_collected(&self, f: &Filter) -> Collected {
        let mut out = Collected::default();
        let mut by: HashMap<&str, Cents> = HashMap::new();
        for p in self.payments.iter().filter(|p| f.day(p.date.date()) && f.center(p.center_id)) {
            let a = Cents::from_eur(p.amount);
            *by.entry(p.method.as_str()).or_default() += a;
            if p.cancels.is_some() {
                out.cancelled += a;
            } else if a.0 < 0 {
                out.returned += a;
            } else {
                out.charged += a;
            }
            out.net += a;
            out.count += 1;
        }
        out.by_method = METHODS.iter().map(|m| (m.to_string(), by.remove(m).unwrap_or_default())).collect();
        let mut rest: Vec<(String, Cents)> = by.into_iter().map(|(m, a)| (m.to_string(), a)).collect();
        rest.sort();
        out.by_method.extend(rest);
        out
    }

    /// Tratamientos realizados en el periodo con el filtro (para el libro
    /// diario y la producción).
    pub fn report_acts(&self, f: &Filter) -> Vec<&MedEntry> {
        let mut v: Vec<&MedEntry> = self.entries.iter().filter(|e| e.is_act() && f.day(e.date) && f.center(e.center_id) && f.doctor.is_none_or(|d| d == e.doctor_id)).collect();
        v.sort_by_key(|e| (e.date, e.id));
        v
    }

    /// Lo que paga la mutua por un acto (0 si el paciente no era de una mutua).
    pub fn insurer_part(&self, e: &MedEntry) -> Cents {
        let t = if e.tariff_id != 0 { e.tariff_id } else { self.patient(e.patient_id).map(|p| p.tariff_id).unwrap_or(0) };
        Cents::from_eur(self.insurer_fee(t, &e.code)) * e.units.max(1) as i64
    }

    /// Producción del periodo con el filtro (ver el módulo).
    pub fn report_production(&self, f: &Filter) -> Production {
        let mut out = Production::default();
        for e in self.report_acts(f) {
            out.acts += 1;
            out.patient += Cents::from_eur(e.amount());
            out.insurer += self.insurer_part(e);
        }
        out
    }

    /// Facturado en el periodo (ver el módulo).
    pub fn report_invoiced(&self, f: &Filter) -> Invoiced {
        let mut out = Invoiced::default();
        for i in self.invoices.iter().filter(|i| f.day(i.date)) {
            let t = Cents::from_eur(i.total());
            if i.rectifies.is_some() {
                out.rectifying += 1;
                out.rectified += t;
            } else {
                out.invoices += 1;
                out.issued += t;
            }
            out.base += Cents::from_eur(i.base());
            out.vat += Cents::from_eur(i.vat());
            out.net += t;
        }
        out
    }

    /// Saldo de cada paciente a una fecha: lo realizado hasta ese día menos lo
    /// cobrado hasta ese día (positivo = debe). Solo los que no están a cero.
    pub fn balances_at(&self, day: NaiveDate) -> Vec<(u32, Cents)> {
        let mut bal: HashMap<u32, Cents> = HashMap::new();
        for e in self.entries.iter().filter(|e| e.state == TxState::Realizado && e.date <= day) {
            *bal.entry(e.patient_id).or_default() += Cents::from_eur(e.amount());
        }
        for p in self.payments.iter().filter(|p| p.date.date() <= day) {
            *bal.entry(p.patient_id).or_default() -= Cents::from_eur(p.amount);
        }
        let mut v: Vec<(u32, Cents)> = bal.into_iter().filter(|(_, b)| !b.is_zero()).collect();
        v.sort_by_key(|(pid, b)| (std::cmp::Reverse(*b), *pid));
        v
    }

    /// Deuda y anticipos a una fecha (ver el módulo).
    pub fn report_debt(&self, day: NaiveDate) -> Debt {
        let mut out = Debt::default();
        for (_, b) in self.balances_at(day) {
            if b.0 > 0 {
                out.owed += b;
                out.debtors += 1;
            } else {
                out.credit -= b;
            }
        }
        out
    }

    /// Mutuas en el periodo (ver el módulo).
    pub fn report_insurers(&self, f: &Filter) -> Insurers {
        let mut out = Insurers::default();
        for c in self.claims.iter().filter(|c| c.cancelled.is_none()) {
            if f.day(c.date) {
                out.claimed += Cents::from_eur(c.total);
            }
            if c.paid_on.is_some_and(|d| f.day(d)) {
                out.collected += Cents::from_eur(c.paid);
            }
            out.pending += Cents::from_eur(c.pending());
        }
        for e in self.report_acts(f).into_iter().filter(|e| !self.claimed(e.id)) {
            out.unclaimed += self.insurer_part(e);
        }
        out
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    pub(crate) use super::tests::{day, november};
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    pub(crate) fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    fn eur(c: Cents) -> f64 {
        c.eur()
    }

    /// Día y hora.
    fn at(d: NaiveDate, h: u32, mi: u32, s: u32) -> chrono::NaiveDateTime {
        d.and_time(NaiveTime::from_hms_opt(h, mi, s).unwrap())
    }

    /// Un cobro con su fecha y hora y su centro.
    fn pay(db: &mut Db, pid: u32, amount: f64, method: &str, when: chrono::NaiveDateTime, center: u32) -> u32 {
        let id = db.add_payment(pid, amount, method, "Prueba");
        let p = db.payments.iter_mut().find(|p| p.id == id).unwrap();
        p.date = when;
        p.center_id = center;
        p.cash_session = None;
        id
    }

    /// Un tratamiento con su precio, centro y estado.
    #[allow(clippy::too_many_arguments)]
    fn act(db: &mut Db, pid: u32, code: &str, doctor: u32, center: u32, d: NaiveDate, price: f64, disc: f64, state: TxState) -> u32 {
        let id = db.add_entry(pid, code, vec![], vec![], TxState::Previsto, doctor, d);
        let e = db.entries.iter_mut().find(|e| e.id == id).unwrap();
        (e.price, e.discount_pct, e.units, e.center_id, e.state) = (price, disc, 1, center, state);
        id
    }

    /// Operaciones sintéticas de noviembre de 2026 en dos centros, con dos
    /// profesionales, una mutua, anulaciones, devoluciones, una remesa
    /// devuelta, facturas y una rectificativa. Los totales esperados de las
    /// pruebas están calculados a mano a partir de esta lista, no con el
    /// código que se prueba.
    pub(crate) fn november() -> (Db, [u32; 3]) {
        let mut db = Db::sample(day(11, 1));
        db.entries.clear();
        db.payments.clear();
        db.invoices.clear();
        db.vf_records.clear();
        db.claims.clear();
        db.remittances.clear();
        let (a, b, m) = (101, 102, 103);
        // la mutua: copago 15 € y baremo 30 € en la obturación; 0 € y 20 € en la revisión
        let mut t = db.tariffs.iter().find(|t| t.id == 1).unwrap().clone();
        (t.id, t.name, t.is_insurer, t.discount_pct) = (77, "Mutua de prueba".into(), true, 0.0);
        t.prices = vec![TariffPrice { code: "0302".into(), price: 15.0, insurer_fee: 30.0 }, TariffPrice { code: "0101".into(), price: 0.0, insurer_fee: 20.0 }];
        db.tariffs.push(t);
        for p in db.patients.iter_mut().filter(|p| [a, b, m].contains(&p.id)) {
            p.tariff_id = if p.id == m { 77 } else { 1 };
            p.discount_pct = 0.0;
        }
        use TxState::*;
        let e1 = act(&mut db, a, "0102", 1, 1, day(11, 3), 100.0, 0.0, Realizado);
        act(&mut db, a, "0102", 2, 2, day(11, 10), 250.50, 0.0, Realizado);
        let e3 = act(&mut db, b, "0102", 1, 1, day(11, 20), 80.0, 10.0, Realizado);
        act(&mut db, m, "0302", 2, 1, day(11, 15), 15.0, 0.0, Realizado);
        act(&mut db, m, "0101", 2, 2, day(11, 16), 0.0, 0.0, Realizado);
        act(&mut db, a, "0102", 1, 1, day(10, 30), 40.0, 0.0, Realizado);
        act(&mut db, b, "0102", 1, 1, day(11, 25), 500.0, 0.0, Previsto);
        act(&mut db, a, "0102", 1, 1, day(12, 2), 60.0, 0.0, Realizado);
        for e in db.entries.iter_mut() {
            e.tariff_id = if e.patient_id == m { 77 } else { 1 };
        }
        pay(&mut db, a, 100.0, "Efectivo", at(day(11, 3), 10, 0, 0), 1);
        pay(&mut db, a, 200.0, "Tarjeta", at(day(11, 10), 12, 0, 0), 2);
        pay(&mut db, b, 72.0, "Transferencia", at(day(11, 21), 9, 0, 0), 1);
        let twice = pay(&mut db, b, 72.0, "Transferencia", at(day(11, 21), 9, 5, 0), 1);
        let back = db.cancel_payment(twice, "apuntado dos veces", true).unwrap();
        let p = db.payments.iter_mut().find(|p| p.id == back).unwrap();
        (p.date, p.center_id) = (day(11, 22).and_hms_opt(11, 0, 0).unwrap(), 1);
        pay(&mut db, a, -10.0, "Transferencia", at(day(11, 28), 13, 0, 0), 1);
        pay(&mut db, m, 15.0, "Bizum", at(day(11, 15), 18, 0, 0), 1);
        pay(&mut db, a, 50.0, "Domiciliación", at(day(11, 5), 8, 0, 0), 1);
        pay(&mut db, a, -50.0, "Domiciliación", at(day(11, 12), 8, 0, 0), 1);
        // en el límite del periodo: el último segundo de octubre y el primero de diciembre
        pay(&mut db, b, 30.0, "Efectivo", at(day(10, 31), 23, 59, 59), 1);
        pay(&mut db, b, 20.0, "Efectivo", at(day(12, 1), 0, 0, 0), 2);
        // facturas: A (100 €) y B (72 €); la de A se rectifica el 26
        let fa = db.issue_invoice(a, &[e1], day(11, 3)).unwrap();
        db.issue_invoice(b, &[e3], day(11, 20)).unwrap();
        let r = db.rectify_invoice(fa, "error en el cliente").unwrap();
        db.invoices.iter_mut().find(|i| i.id == r).unwrap().date = day(11, 26);
        (db, [a, b, m])
    }

    fn nov() -> Filter {
        Filter::period(day(11, 1), day(11, 30))
    }

    #[test]
    fn collected_matches_the_operations() {
        let (db, _) = november();
        let c = db.report_collected(&nov());
        let by: Vec<(&str, f64)> = c.by_method.iter().map(|(m, a)| (m.as_str(), eur(*a))).collect();
        assert_eq!(by, vec![("Efectivo", 100.0), ("Tarjeta", 200.0), ("Transferencia", 62.0), ("Financiación", 0.0), ("Bizum", 15.0), ("Domiciliación", 0.0)]);
        assert_eq!((eur(c.charged), eur(c.cancelled), eur(c.returned), eur(c.net), c.count), (509.0, -72.0, -60.0, 377.0, 9));
        let sum: Cents = c.by_method.iter().map(|x| x.1).sum();
        assert_eq!(sum, c.net, "las formas de pago suman el total");
        // por centro: 1 y 2 suman el total; el filtro de profesional no cambia nada
        let c1 = db.report_collected(&Filter { center: Some(1), ..nov() });
        let c2 = db.report_collected(&Filter { center: Some(2), ..nov() });
        assert_eq!((eur(c1.net), eur(c2.net)), (177.0, 200.0));
        assert_eq!(db.report_collected(&Filter { doctor: Some(1), ..nov() }), c);
        // un día suelto: los dos límites, incluidos
        assert_eq!(eur(db.report_collected(&Filter::period(day(10, 31), day(10, 31))).net), 30.0);
        assert_eq!(eur(db.report_collected(&Filter::period(day(12, 1), day(12, 1))).net), 20.0);
    }

    #[test]
    fn production_matches_the_operations() {
        let (db, _) = november();
        let p = |f: Filter| {
            let x = db.report_production(&f);
            (x.acts, eur(x.patient), eur(x.insurer), eur(x.total()))
        };
        assert_eq!(p(nov()), (5, 437.5, 50.0, 487.5), "sin lo previsto ni lo de otros meses");
        assert_eq!(p(Filter { doctor: Some(1), ..nov() }), (2, 172.0, 0.0, 172.0));
        assert_eq!(p(Filter { doctor: Some(2), ..nov() }), (3, 265.5, 50.0, 315.5));
        assert_eq!(p(Filter { center: Some(1), ..nov() }), (3, 187.0, 30.0, 217.0));
        assert_eq!(p(Filter { center: Some(2), ..nov() }), (2, 250.5, 20.0, 270.5));
        assert_eq!(p(Filter { center: Some(1), doctor: Some(2), ..nov() }), (1, 15.0, 30.0, 45.0));
        // la liquidación del colaborador usa los mismos actos (sin filtro de centro)
        let (acts, prod, _, _) = db.settlement(2, day(11, 1), day(11, 30));
        assert_eq!((acts.len(), prod), (3, 265.5));
    }

    #[test]
    fn invoiced_matches_the_operations() {
        let (db, _) = november();
        let i = db.report_invoiced(&nov());
        assert_eq!((i.invoices, i.rectifying), (2, 1));
        assert_eq!((eur(i.issued), eur(i.rectified), eur(i.net), eur(i.base), eur(i.vat)), (172.0, -100.0, 72.0, 72.0, 0.0));
        assert_eq!(db.report_invoiced(&Filter { center: Some(2), doctor: Some(1), ..nov() }), i, "las facturas no son de un centro");
    }

    #[test]
    fn debt_at_a_cut_off_date() {
        let (db, [a, b, _]) = november();
        let d = |m, dd| {
            let x = db.report_debt(day(m, dd));
            (eur(x.owed), x.debtors, eur(x.credit))
        };
        assert_eq!(d(10, 31), (40.0, 1, 30.0));
        assert_eq!(d(11, 30), (100.5, 1, 30.0), "A debe 100,50 y B tiene 30 a favor; M está a cero");
        assert_eq!(d(12, 31), (160.5, 1, 50.0));
        let at = db.balances_at(day(11, 30));
        assert_eq!(at, vec![(a, Cents(10050)), (b, Cents(-3000))]);
        // hoy (después de todo) coincide con el saldo de la ficha
        assert_eq!(db.balances_at(day(12, 31)).iter().find(|x| x.0 == a).unwrap().1.eur(), db.patient_balance(a));
    }

    #[test]
    fn insurers_match_the_operations() {
        let (mut db, _) = november();
        let i = |db: &Db, f: Filter| {
            let x = db.report_insurers(&f);
            (eur(x.claimed), eur(x.collected), eur(x.pending), eur(x.unclaimed))
        };
        assert_eq!(i(&db, nov()), (0.0, 0.0, 0.0, 50.0), "sin liquidar: 30 + 20");
        let id = db.create_claim(77, day(11, 1), day(11, 30), day(11, 30)).unwrap();
        assert_eq!(i(&db, nov()), (50.0, 0.0, 50.0, 0.0));
        db.claim_paid(id, 50.0, day(12, 5));
        assert_eq!(i(&db, nov()), (50.0, 0.0, 0.0, 0.0), "se cobró en diciembre");
        assert_eq!(i(&db, Filter::period(day(12, 1), day(12, 31))), (0.0, 50.0, 0.0, 0.0));
    }
}

#[cfg(test)]
mod commission_tests {
    use super::tests_support::*;

    /// La comisión es sobre lo que paga el paciente, salvo en el colaborador
    /// marcado «también sobre lo que paga la mutua».
    #[test]
    fn the_commission_counts_the_insurer_part_only_if_marked() {
        let (mut db, _) = november();
        let nov = (day(11, 1), day(11, 30));
        assert_eq!(db.settlement(2, nov.0, nov.1).1, 265.5);
        db.collaborators.iter_mut().find(|c| c.id == 2).unwrap().commission_on_insurer = true;
        let (_, prod, _, fee) = db.settlement(2, nov.0, nov.1);
        assert_eq!(prod, 315.5, "más 30 + 20 de la mutua");
        let pct = db.collaborators.iter().find(|c| c.id == 2).unwrap().commission_pct;
        assert_eq!(fee, crate::model::round2(315.5 * pct / 100.0));
        assert!(db.settlement_html(2, nov.0, nov.1).contains("lo que pagan el paciente y la mutua"));
    }
}
