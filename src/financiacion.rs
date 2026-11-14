//! Financiación con una entidad financiera (T-F6-03).
//!
//! La clínica pide la financiación de un tratamiento para el paciente. La
//! entidad la aprueba o la deniega, el paciente firma y la entidad paga a la
//! clínica (a veces menos el coste que asume la clínica). Aquí se lleva su
//! estado con su historia y lo que pasa con el dinero:
//!
//! - **Al cobrarla** ([`Db::fin_paid`]), el paciente queda pagado por el total
//!   financiado: un cobro «Financiación», una sola vez. Lo que la entidad se
//!   queda es un gasto de la clínica («Comisión de financiación»).
//! - **Conciliación:** el ingreso de la entidad en el banco se concilia con la
//!   financiación (por lo recibido, no por el total).
//! - **Desistimiento:** el paciente puede desistir del crédito en los
//!   [`WITHDRAWAL_DAYS`] días siguientes a firmarlo. Si la entidad ya había
//!   pagado, se apunta la devolución a la entidad: el paciente vuelve a deber
//!   el tratamiento a la clínica.
//! - **Adaptador:** por ahora, manual. La clínica usa el portal de la entidad
//!   y apunta aquí cada paso con la referencia de la entidad. La conexión con
//!   una entidad concreta está pendiente de su especificación (B-19, T-F6-03X).
//!
//! Las altas, los contratos y las decisiones de financiación son de la
//! entidad: el programa no las toma ni las simula.

use crate::cents::Cents;
use crate::model::*;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

/// Días para desistir de un crédito al consumo desde la firma (Ley 16/2011,
/// artículo 28; citado de memoria, lo confirma la gestoría o el DPO).
pub const WITHDRAWAL_DAYS: i64 = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinState {
    Solicitada,
    Aprobada,
    Denegada,
    Firmada,
    /// La entidad ha pagado a la clínica.
    Pagada,
    Cancelada,
}

impl FinState {
    pub fn label(self) -> &'static str {
        match self {
            FinState::Solicitada => "Solicitada",
            FinState::Aprobada => "Aprobada",
            FinState::Denegada => "Denegada",
            FinState::Firmada => "Firmada",
            FinState::Pagada => "Pagada a la clínica",
            FinState::Cancelada => "Cancelada",
        }
    }

    /// A qué estados se puede pasar desde este (a mano). «Pagada» y
    /// «Cancelada» tienen su propia operación.
    pub fn next(self) -> &'static [FinState] {
        match self {
            FinState::Solicitada => &[FinState::Aprobada, FinState::Denegada],
            FinState::Aprobada => &[FinState::Firmada, FinState::Denegada],
            _ => &[],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FinEvent {
    pub at: NaiveDateTime,
    pub state: FinState,
    pub note: String,
    pub by: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Financing {
    pub id: u32,
    pub patient_id: u32,
    #[serde(default)]
    pub budget_id: Option<u32>,
    /// Entidad financiera.
    pub provider: String,
    /// Referencia de la entidad (la de su portal).
    #[serde(default)]
    pub provider_ref: String,
    /// Total financiado, en céntimos.
    pub amount: i64,
    pub months: u32,
    pub state: FinState,
    #[serde(default)]
    pub signed: Option<NaiveDate>,
    /// Cuándo pagó la entidad y cuánto llegó (en céntimos).
    #[serde(default)]
    pub paid: Option<(NaiveDate, i64)>,
    #[serde(default)]
    pub payment_id: Option<u32>,
    #[serde(default)]
    pub fee_expense: Option<u32>,
    /// Devolución a la entidad al desistir después de que pagara.
    #[serde(default)]
    pub refund_payment_id: Option<u32>,
    #[serde(default)]
    pub bank_move: Option<u32>,
    pub history: Vec<FinEvent>,
}

impl Db {
    fn fin(&self, id: u32) -> Result<Financing, String> {
        self.financings.iter().find(|f| f.id == id).cloned().ok_or_else(|| "La financiación ya no existe.".into())
    }

    fn fin_event(&mut self, id: u32, state: FinState, note: &str, now: NaiveDateTime) {
        let by = self.config.current_user.clone();
        if let Some(f) = self.financings.iter_mut().find(|f| f.id == id) {
            f.state = state;
            f.history.push(FinEvent { at: now, state, note: note.trim().chars().take(200).collect(), by });
        }
    }

    /// Solicitud nueva (la clínica la ha pedido a la entidad).
    pub fn fin_new(&mut self, pid: u32, budget: Option<u32>, provider: &str, amount: Cents, months: u32, now: NaiveDateTime) -> Result<u32, String> {
        if self.patient(pid).is_none() {
            return Err("El paciente ya no existe.".into());
        }
        if provider.trim().is_empty() {
            return Err("Diga la entidad financiera.".into());
        }
        if amount.0 <= 0 {
            return Err("Indique el importe financiado.".into());
        }
        if !(1..=120).contains(&months) {
            return Err("El plazo va de 1 a 120 meses.".into());
        }
        if let Some(b) = budget
            && !self.budgets.iter().any(|x| x.id == b && x.patient_id == pid)
        {
            return Err("Ese presupuesto no es de este paciente.".into());
        }
        let id = self.new_id();
        self.financings.push(Financing {
            id,
            patient_id: pid,
            budget_id: budget,
            provider: provider.trim().into(),
            provider_ref: String::new(),
            amount: amount.0,
            months,
            state: FinState::Solicitada,
            signed: None,
            paid: None,
            payment_id: None,
            fee_expense: None,
            refund_payment_id: None,
            bank_move: None,
            history: vec![],
        });
        self.fin_event(id, FinState::Solicitada, &format!("{} en {months} meses", amount), now);
        let who = self.patient_name(pid);
        self.audit("Financiación solicitada", &format!("{who} · {} · {}", provider.trim(), amount));
        Ok(id)
    }

    /// Un paso de la entidad: aprobada, denegada o firmada (con la fecha de la
    /// firma), con su referencia.
    pub fn fin_step(&mut self, id: u32, to: FinState, provider_ref: &str, date: NaiveDate, now: NaiveDateTime) -> Result<(), String> {
        let f = self.fin(id)?;
        if !f.state.next().contains(&to) {
            return Err(format!("De «{}» no se pasa a «{}».", f.state.label(), to.label()));
        }
        if let Some(x) = self.financings.iter_mut().find(|x| x.id == id) {
            if !provider_ref.trim().is_empty() {
                x.provider_ref = provider_ref.trim().chars().take(60).collect();
            }
            if to == FinState::Firmada {
                x.signed = Some(date);
            }
        }
        self.fin_event(id, to, provider_ref, now);
        Ok(())
    }

    /// La entidad ha pagado: el paciente queda pagado por el total (una sola
    /// vez) y lo que no llegó es la comisión de la clínica.
    pub fn fin_paid(&mut self, id: u32, date: NaiveDate, received: Cents, now: NaiveDateTime) -> Result<u32, String> {
        let f = self.fin(id)?;
        if f.payment_id.is_some() || f.state == FinState::Pagada {
            return Err("El pago de la entidad ya está apuntado.".into());
        }
        if f.state != FinState::Firmada {
            return Err("La entidad paga una financiación firmada.".into());
        }
        if received.0 <= 0 || received.0 > f.amount {
            return Err(format!("Lo recibido tiene que estar entre 0 y el total financiado ({}).", Cents(f.amount)));
        }
        let pid = self.add_payment(f.patient_id, Cents(f.amount).eur(), "Financiación", format!("Financiación {} {}", f.provider, f.provider_ref).trim());
        if let Some(p) = self.payments.iter_mut().find(|p| p.id == pid) {
            p.date = date.and_time(now.time());
        }
        let fee = f.amount - received.0;
        let fee_id = (fee > 0).then(|| {
            let eid = self.new_id();
            self.expenses.push(crate::finance::Expense {
                id: eid,
                date,
                supplier: f.provider.clone(),
                concept: format!("Coste de la financiación {}", f.provider_ref).trim().to_string(),
                category: "Comisión de financiación".into(),
                base: Cents(fee).eur(),
                vat_pct: 0.0,
                invoice_no: String::new(),
                method: "Descontado del pago".into(),
                paid: true,
                due: None,
            });
            eid
        });
        if let Some(x) = self.financings.iter_mut().find(|x| x.id == id) {
            x.paid = Some((date, received.0));
            x.payment_id = Some(pid);
            x.fee_expense = fee_id;
        }
        self.fin_event(id, FinState::Pagada, &format!("recibido {}", received), now);
        let who = self.patient_name(f.patient_id);
        self.audit("Financiación pagada por la entidad", &format!("{who} · {} · total {} · recibido {}", f.provider, Cents(f.amount), received));
        Ok(pid)
    }

    /// Cancelar: antes de pagar la entidad, sin más; después, solo si el
    /// paciente desiste en plazo, con la devolución a la entidad.
    pub fn fin_cancel(&mut self, id: u32, reason: &str, date: NaiveDate, now: NaiveDateTime) -> Result<(), String> {
        let f = self.fin(id)?;
        if reason.trim().is_empty() {
            return Err("Escriba el motivo.".into());
        }
        match f.state {
            FinState::Denegada | FinState::Cancelada => return Err("Ya está cerrada.".into()),
            FinState::Pagada => {
                let signed = f.signed.ok_or("Falta la fecha de la firma.")?;
                if (date - signed).num_days() > WITHDRAWAL_DAYS {
                    return Err(format!("Han pasado más de {WITHDRAWAL_DAYS} días desde la firma: ya no se puede desistir. Hable con la entidad."));
                }
                if f.refund_payment_id.is_none() {
                    let pid = self.add_payment(f.patient_id, -Cents(f.amount).eur(), "Financiación", &format!("Desistimiento: devolución a {}", f.provider));
                    if let Some(x) = self.financings.iter_mut().find(|x| x.id == id) {
                        x.refund_payment_id = Some(pid);
                    }
                }
            }
            _ => {}
        }
        self.fin_event(id, FinState::Cancelada, reason, now);
        let who = self.patient_name(f.patient_id);
        self.audit("Financiación cancelada", &format!("{who} · {} · {}", f.provider, reason.trim()));
        Ok(())
    }

    /// Ingresos del banco que pueden ser el pago de la entidad: sin conciliar,
    /// por lo recibido y cerca de la fecha (15 días).
    pub fn fin_bank_candidates(&self, id: u32) -> Vec<u32> {
        let Ok(f) = self.fin(id) else { return vec![] };
        let Some((date, received)) = f.paid.filter(|_| f.bank_move.is_none()) else { return vec![] };
        self.bank_moves.iter().filter(|m| m.matched.is_none() && Cents::from_eur(m.amount).0 == received && (m.date - date).num_days().abs() <= 15).map(|m| m.id).collect()
    }

    /// Concilia el ingreso de la entidad con la financiación.
    pub fn fin_match_bank(&mut self, id: u32, bank_move: u32) -> Result<(), String> {
        let f = self.fin(id)?;
        if f.bank_move.is_some() {
            return Err("Ya está conciliada.".into());
        }
        if !self.fin_bank_candidates(id).contains(&bank_move) {
            return Err("Ese movimiento no es el pago de esta financiación (importe, fecha o ya conciliado).".into());
        }
        self.match_bank_move(bank_move, crate::bank::Match::Other(format!("Financiación {} {}", f.provider, f.provider_ref).trim().to_string()))?;
        if let Some(x) = self.financings.iter_mut().find(|x| x.id == id) {
            x.bank_move = Some(bank_move);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    fn at(day: u32) -> NaiveDateTime {
        d(day).and_hms_opt(10, 0, 0).unwrap()
    }

    fn fin_payments(db: &Db, pid: u32) -> Vec<f64> {
        db.payments.iter().filter(|p| p.patient_id == pid && p.method == "Financiación" && (p.concept.starts_with("Financiación ") || p.concept.starts_with("Desistimiento"))).map(|p| p.amount).collect()
    }

    /// De la solicitud al pago, con la comisión como gasto y el cobro una vez;
    /// y la conciliación con el ingreso de la entidad (lo recibido).
    #[test]
    fn a_financing_goes_from_request_to_payment_once() {
        let mut db = Db::sample(d(2));
        db.financings.clear();
        let before = db.patient_balance(105);
        let id = db.fin_new(105, None, "Financiera Ejemplo", Cents(120_000), 12, at(2)).unwrap();
        assert!(db.fin_step(id, FinState::Firmada, "", d(3), at(3)).is_err(), "sin aprobar, no se firma");
        assert!(db.fin_paid(id, d(4), Cents(110_000), at(4)).is_err(), "sin firmar, no paga");
        db.fin_step(id, FinState::Aprobada, "FE-778", d(3), at(3)).unwrap();
        db.fin_step(id, FinState::Firmada, "", d(5), at(5)).unwrap();
        assert!(db.fin_paid(id, d(9), Cents(130_000), at(9)).is_err(), "no llega más de lo financiado");
        db.fin_paid(id, d(9), Cents(114_000), at(9)).unwrap();
        assert!(db.fin_paid(id, d(9), Cents(114_000), at(9)).is_err(), "una sola vez");
        assert_eq!(fin_payments(&db, 105), vec![1200.0], "el paciente, pagado por el total");
        assert!((db.patient_balance(105) - (before - 1200.0)).abs() < 0.005);
        let f = db.financings.iter().find(|f| f.id == id).unwrap().clone();
        let fee = db.expenses.iter().find(|e| Some(e.id) == f.fee_expense).unwrap();
        assert_eq!((fee.base, fee.category.as_str()), (60.0, "Comisión de financiación"), "lo que no llegó es la comisión");
        assert_eq!(f.history.iter().map(|e| e.state).collect::<Vec<_>>(), vec![FinState::Solicitada, FinState::Aprobada, FinState::Firmada, FinState::Pagada]);
        assert_eq!(f.provider_ref, "FE-778");
        // conciliación: el ingreso de lo recibido, no del total
        db.add_bank_moves(
            1,
            &[crate::bank::RawMove { date: d(10), amount: 1140.0, concept: "TRANSF FINANCIERA EJEMPLO FE-778".into(), reference: String::new() }, crate::bank::RawMove { date: d(10), amount: 1200.0, concept: "OTRO".into(), reference: String::new() }],
        );
        let cands = db.fin_bank_candidates(id);
        assert_eq!(cands.len(), 1);
        let mv = cands[0];
        assert_eq!(db.bank_moves.iter().find(|m| m.id == mv).unwrap().amount, 1140.0);
        db.fin_match_bank(id, mv).unwrap();
        assert!(db.fin_match_bank(id, mv).is_err(), "una vez");
        assert!(db.bank_moves.iter().find(|m| m.id == mv).unwrap().matched.is_some());
        assert!(db.fin_bank_candidates(id).is_empty());
    }

    /// Desistir: antes del pago, sin más; después, en plazo y con la
    /// devolución a la entidad (el paciente vuelve a deber); fuera de plazo, no.
    #[test]
    fn withdrawing_follows_the_deadline_and_returns_the_money() {
        let mut db = Db::sample(d(2));
        db.financings.clear();
        let a = db.fin_new(106, None, "Financiera Ejemplo", Cents(50_000), 6, at(2)).unwrap();
        assert!(db.fin_cancel(a, " ", d(3), at(3)).is_err(), "con su motivo");
        db.fin_cancel(a, "El paciente lo pagará en la clínica", d(3), at(3)).unwrap();
        assert!(db.fin_cancel(a, "otra vez", d(3), at(3)).is_err());
        assert!(fin_payments(&db, 106).is_empty(), "sin dinero de por medio");
        // pagada y desistida en plazo
        let before = db.patient_balance(106);
        let b = db.fin_new(106, None, "Financiera Ejemplo", Cents(80_000), 10, at(2)).unwrap();
        db.fin_step(b, FinState::Aprobada, "R1", d(2), at(2)).unwrap();
        db.fin_step(b, FinState::Firmada, "", d(3), at(3)).unwrap();
        db.fin_paid(b, d(6), Cents(80_000), at(6)).unwrap();
        db.fin_cancel(b, "Desiste del crédito", d(15), at(15)).unwrap();
        assert_eq!(fin_payments(&db, 106), vec![800.0, -800.0], "el cobro y la devolución a la entidad");
        assert!((db.patient_balance(106) - before).abs() < 0.005, "el paciente vuelve a deber lo de antes");
        // fuera de plazo
        let c = db.fin_new(106, None, "Financiera Ejemplo", Cents(30_000), 3, at(2)).unwrap();
        db.fin_step(c, FinState::Aprobada, "R2", d(2), at(2)).unwrap();
        db.fin_step(c, FinState::Firmada, "", d(2), at(2)).unwrap();
        db.fin_paid(c, d(4), Cents(30_000), at(4)).unwrap();
        assert!(db.fin_cancel(c, "tarde", d(17), at(17)).unwrap_err().contains("14 días"));
        // validaciones
        assert!(db.fin_new(106, None, " ", Cents(100), 3, at(2)).is_err());
        assert!(db.fin_new(106, None, "X", Cents(0), 3, at(2)).is_err());
        assert!(db.fin_new(106, None, "X", Cents(100), 0, at(2)).is_err());
        let other = db.budgets.iter().find(|b| b.patient_id != 106).unwrap().id;
        assert!(db.fin_new(106, Some(other), "X", Cents(100), 3, at(2)).is_err(), "presupuesto de otro paciente, no");
    }
}
