//! Cuadro de mando (T-F5-01): indicadores de gestión con su fórmula, su
//! denominador y lo que no entra, con los filtros de los informes (periodo,
//! ambos días incluidos; centro y profesional, ver [`crate::informes::Filter`]).
//! Todo se cuenta en enteros (minutos, citas, céntimos) y cada proporción
//! guarda numerador y denominador: sin denominador no hay valor (no es un 0 %).
//!
//! - **Ocupación de cada profesional** = minutos citados ÷ minutos disponibles.
//!   - Citados: sus citas del periodo salvo las anuladas y las reprogramadas.
//!     Las de «no acude» cuentan, porque el hueco estaba reservado.
//!   - Disponibles: los tramos de su horario de cada día ([`Db::doctor_day`],
//!     sin los festivos) menos sus bloqueos de agenda.
//!   - Puede pasar del 100 % si se cita por encima del horario.
//!   - Su horario no es de un centro, así que el filtro de centro no le afecta.
//! - **Ocupación de cada gabinete** = minutos citados en el gabinete ÷ minutos
//!   que abre la clínica ([`Db::clinic_day`]), con el filtro de centro.
//! - **Ausencias** = citas «no acude» ÷ (atendidas + «no acude»), de las citas
//!   del periodo.
//!   - Atendidas: las finalizadas.
//!   - Las que aún no se han cerrado (planificadas, en sala…) se cuentan
//!     aparte y no entran.
//! - **Aceptación de presupuestos** = aceptados ÷ presentados, en número y en
//!   importe.
//!   - Presentados: los presupuestos con fecha en el periodo, sin las
//!     versiones sustituidas por otra ni las alternativas descartadas al
//!     elegir otra del mismo plan (ver `plan`).
//!   - Aceptados: los aceptados y los cerrados.
//!   - Los presupuestos no son de un centro.
//! - **Producción por hora de sillón** = producción del periodo
//!   ([`Db::report_production`]) ÷ horas de las citas finalizadas, en total y
//!   por profesional.
//! - **Recalls que terminan en cita** = recalls citados ÷ recalls con fecha
//!   prevista en el periodo, de los de ese profesional si se filtra. Los
//!   recalls no son de un centro.
//! - **Comparación:** el periodo anterior de la misma duración ([`previous`]).
//! - **Objetivos:** los de la clínica ([`Goals`]), cada uno opcional.

use crate::cents::Cents;
use crate::informes::Filter;
use crate::model::*;
use chrono::{Duration, NaiveDate, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

/// Numerador y denominador (minutos, citas o céntimos).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ratio {
    pub num: i64,
    pub den: i64,
}

impl Ratio {
    pub fn pct(&self) -> Option<f64> {
        (self.den != 0).then(|| self.num as f64 * 100.0 / self.den as f64)
    }

    fn add(&mut self, o: Ratio) {
        self.num += o.num;
        self.den += o.den;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Occupancy {
    /// Profesional o gabinete.
    pub id: u32,
    /// Minutos citados ÷ minutos disponibles.
    pub minutes: Ratio,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoShows {
    pub missed: usize,
    pub attended: usize,
    /// Del periodo, aún sin cerrar (no entran).
    pub unclosed: usize,
}

impl NoShows {
    pub fn rate(&self) -> Ratio {
        Ratio { num: self.missed as i64, den: (self.missed + self.attended) as i64 }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Acceptance {
    /// Aceptados ÷ presentados (número).
    pub count: Ratio,
    /// Aceptados ÷ presentados (céntimos).
    pub amount: Ratio,
    pub rejected: usize,
    pub pending: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChairHour {
    /// Profesional (0 en el total).
    pub id: u32,
    pub production: Cents,
    /// Minutos de las citas finalizadas.
    pub minutes: i64,
}

impl ChairHour {
    /// Producción por hora (sin minutos, no hay valor).
    pub fn per_hour(&self) -> Option<Cents> {
        (self.minutes > 0).then(|| Cents(self.production.0 * 60 / self.minutes))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recalls {
    pub due: usize,
    pub booked: usize,
    pub rejected: usize,
}

impl Recalls {
    pub fn rate(&self) -> Ratio {
        Ratio { num: self.booked as i64, den: self.due as i64 }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dashboard {
    pub doctors: Vec<Occupancy>,
    pub doctors_total: Ratio,
    pub boxes: Vec<Occupancy>,
    pub boxes_total: Ratio,
    pub no_shows: NoShows,
    pub acceptance: Acceptance,
    pub chair: Vec<ChairHour>,
    pub chair_total: ChairHour,
    pub recalls: Recalls,
}

/// Objetivos de la clínica (vacío: sin objetivo).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Goals {
    #[serde(default)]
    pub occupancy_pct: Option<f64>,
    /// Como mucho.
    #[serde(default)]
    pub no_show_pct: Option<f64>,
    #[serde(default)]
    pub acceptance_pct: Option<f64>,
    #[serde(default)]
    pub per_hour_eur: Option<f64>,
    #[serde(default)]
    pub recall_pct: Option<f64>,
}

impl Goals {
    pub fn is_empty(&self) -> bool {
        *self == Goals::default()
    }
}

/// ¿Se cumple el objetivo? `None` sin valor o sin objetivo.
pub fn meets(value: Option<f64>, goal: Option<f64>, higher_is_better: bool) -> Option<bool> {
    let (v, g) = (value?, goal?);
    Some(if higher_is_better { v >= g } else { v <= g })
}

/// El periodo anterior de la misma duración, con los mismos filtros.
pub fn previous(f: &Filter) -> Filter {
    let days = (f.to - f.from).num_days() + 1;
    Filter { from: f.from - Duration::days(days), to: f.from - Duration::days(1), ..*f }
}

fn mins(t: NaiveTime) -> i64 {
    (t.hour() * 60 + t.minute()) as i64
}

/// Minutos de los tramos menos lo que tapan los bloqueos (que pueden solaparse).
fn free_minutes(shifts: &[crate::horario::Shift], blocks: &[(i64, i64)]) -> i64 {
    let mut total = 0;
    for s in shifts {
        let (a, b) = (mins(s.start), mins(s.end));
        let mut cut: Vec<(i64, i64)> = blocks.iter().map(|&(x, y)| (x.max(a), y.min(b))).filter(|(x, y)| x < y).collect();
        cut.sort();
        let (mut covered, mut reach) = (0, a);
        for (x, y) in cut {
            let x = x.max(reach);
            if y > x {
                covered += y - x;
                reach = y;
            }
        }
        total += (b - a).max(0) - covered;
    }
    total
}

fn days(f: &Filter) -> impl Iterator<Item = NaiveDate> + '_ {
    f.from.iter_days().take_while(move |d| *d <= f.to)
}

impl Db {
    /// Minutos en que pasa consulta un profesional un día, sin sus bloqueos.
    pub fn doctor_free_minutes(&self, doctor_id: u32, date: NaiveDate) -> i64 {
        let blocks: Vec<(i64, i64)> = self.blocks.iter().filter(|b| b.doctor_id == doctor_id && b.date.is_none_or(|d| d == date)).map(|b| (mins(b.start), mins(b.end))).collect();
        free_minutes(&self.doctor_day(doctor_id, date), &blocks)
    }

    /// Los indicadores del periodo con el filtro (ver el módulo).
    pub fn dashboard(&self, f: &Filter) -> Dashboard {
        let mut out = Dashboard::default();
        let in_period: Vec<&Appointment> = self.appointments.iter().filter(|a| f.day(a.date)).collect();
        let booked = |a: &&&Appointment| !matches!(a.state, ApptState::Anulada | ApptState::Reprogramada);
        // ocupación de los profesionales (su horario no es de un centro)
        let mut docs: Vec<u32> = self.collaborators.iter().filter(|c| c.active).map(|c| c.id).collect();
        for a in &in_period {
            if !docs.contains(&a.doctor_id) {
                docs.push(a.doctor_id);
            }
        }
        docs.retain(|d| f.doctor.is_none_or(|x| x == *d));
        docs.sort();
        for d in docs {
            let num = in_period.iter().filter(booked).filter(|a| a.doctor_id == d).map(|a| a.minutes as i64).sum();
            let den = days(f).map(|day| self.doctor_free_minutes(d, day)).sum();
            let r = Ratio { num, den };
            out.doctors_total.add(r);
            out.doctors.push(Occupancy { id: d, minutes: r });
        }
        // ocupación de los gabinetes (con el centro)
        let open: i64 = days(f).map(|day| free_minutes(&self.clinic_day(day), &[])).sum();
        for b in self.boxes.iter().filter(|b| f.center(b.center_id)) {
            let num = in_period.iter().filter(booked).filter(|a| a.box_id == b.id && cid(a.center_id) == cid(b.center_id) && f.doctor.is_none_or(|d| d == a.doctor_id)).map(|a| a.minutes as i64).sum();
            let r = Ratio { num, den: open };
            out.boxes_total.add(r);
            out.boxes.push(Occupancy { id: b.id, minutes: r });
        }
        // ausencias, y minutos atendidos para la hora de sillón
        let mut attended: Vec<(u32, i64)> = vec![];
        for a in in_period.iter().filter(|a| f.center(a.center_id) && f.doctor.is_none_or(|d| d == a.doctor_id)) {
            match a.state {
                ApptState::Finalizada => {
                    out.no_shows.attended += 1;
                    match attended.iter_mut().find(|(d, _)| *d == a.doctor_id) {
                        Some((_, m)) => *m += a.minutes as i64,
                        None => attended.push((a.doctor_id, a.minutes as i64)),
                    }
                }
                ApptState::NoAcude => out.no_shows.missed += 1,
                ApptState::Anulada | ApptState::Reprogramada => {}
                _ => out.no_shows.unclosed += 1,
            }
        }
        // presupuestos presentados
        for b in self.budgets.iter().filter(|b| f.day(b.date) && f.doctor.is_none_or(|d| d == b.doctor_id) && b.tp.replaced_by.is_none() && b.tp.discarded_for.is_none()) {
            let amount = Cents::from_eur(b.total()).0;
            out.acceptance.count.den += 1;
            out.acceptance.amount.den += amount;
            match b.state {
                BudgetState::Aceptado | BudgetState::Cerrado => {
                    out.acceptance.count.num += 1;
                    out.acceptance.amount.num += amount;
                }
                BudgetState::Rechazado => out.acceptance.rejected += 1,
                BudgetState::Pendiente => out.acceptance.pending += 1,
            }
        }
        // producción por hora de sillón
        let prod = self.report_production(f);
        out.chair_total = ChairHour { id: 0, production: prod.total(), minutes: attended.iter().map(|(_, m)| m).sum() };
        attended.sort();
        for (d, m) in attended {
            let p = self.report_production(&Filter { doctor: Some(d), ..*f });
            out.chair.push(ChairHour { id: d, production: p.total(), minutes: m });
        }
        // recalls
        for r in self.recalls.iter().filter(|r| f.day(r.date) && f.doctor.is_none_or(|d| d == r.doctor_id)) {
            out.recalls.due += 1;
            match r.state {
                RecallState::Citado => out.recalls.booked += 1,
                RecallState::Rechazado => out.recalls.rejected += 1,
                _ => {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::horario::Week;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    /// Operaciones sintéticas de la semana del lunes 16 al domingo 22 de
    /// noviembre de 2026. Los valores esperados de las pruebas están
    /// calculados a mano a partir de esta lista, no con el código que se prueba.
    ///
    /// - Clínica: de lunes a viernes, de 9:00 a 14:00 (300 minutos), con el
    ///   miércoles 18 festivo. Abre 4 × 300 = 1200 minutos.
    /// - Profesional 1: el horario de la clínica, con un bloqueo el jueves 19
    ///   de 10:00 a 11:00 y otro, solapado, de 10:30 a 11:30. Disponible:
    ///   1200 − 90 = 1110 minutos.
    /// - Profesional 2: solo los lunes, de 9:00 a 11:00. Disponible: 120.
    /// - Citas del 1 (gabinete 1): finalizadas de 60, 60 y 30; «no acude» de
    ///   30; anulada de 60; planificada de 45 (sin cerrar). Citados:
    ///   60 + 60 + 30 + 30 + 45 = 225.
    /// - Citas del 2 (gabinete 2): finalizada de 60; reprogramada de 30.
    ///   Citados: 60.
    /// - Una cita del 1 el lunes 23 (fuera del periodo).
    /// - Producción: el 1 hace 100 € y 50,50 €; el 2, 80 € (y 999 € el 23, fuera).
    /// - Presupuestos: aceptado de 300 €, cerrado de 200 €, rechazado de 100 €
    ///   y pendiente de 400 € en el periodo; uno aceptado el 15 (fuera).
    /// - Recalls: 4 en el periodo, 2 citados, 1 rechazado y 1 pendiente.
    fn week() -> (Db, Filter) {
        let mut db = Db::sample(day(16));
        // de los datos de ejemplo solo se toman las formas de los registros
        let (appt, budget, recall) = (db.appointments[0].clone(), db.budgets.iter().find(|b| !b.lines.is_empty()).unwrap().clone(), db.recalls[0].clone());
        db.appointments.clear();
        db.blocks.clear();
        db.holidays.clear();
        db.entries.clear();
        db.budgets.clear();
        db.recalls.clear();
        db.config.hours = Some(Week::simple(t(9, 0), t(14, 0), false));
        db.holidays.push(crate::agenda_extra::Holiday { date: day(18), name: "Festivo local".into() });
        for c in db.collaborators.iter_mut() {
            c.active = c.id == 1 || c.id == 2;
            c.hours = None;
        }
        let mut monday = Week::default();
        monday.0[0] = vec![crate::horario::Shift::new(t(9, 0), t(11, 0))];
        db.collaborators.iter_mut().find(|c| c.id == 2).unwrap().hours = Some(monday);
        for (s, e) in [(t(10, 0), t(11, 0)), (t(10, 30), t(11, 30))] {
            let id = db.blocks.len() as u32 + 1;
            db.blocks.push(Block { id, doctor_id: 1, date: Some(day(19)), start: s, end: e, reason: "Reunión".into() });
        }
        db.boxes.clear();
        db.boxes.push(Boxroom { id: 1, name: "Gabinete 1".into(), center_id: 0 });
        db.boxes.push(Boxroom { id: 2, name: "Gabinete 2".into(), center_id: 0 });
        use ApptState::*;
        for (d, doc, bx, min, st) in [
            (16, 1, 1, 60, Finalizada),
            (17, 1, 1, 60, Finalizada),
            (19, 1, 1, 30, Finalizada),
            (20, 1, 1, 30, NoAcude),
            (20, 1, 1, 60, Anulada),
            (20, 1, 1, 45, Planificada),
            (16, 2, 2, 60, Finalizada),
            (16, 2, 2, 30, Reprogramada),
            (23, 1, 1, 60, Finalizada),
        ] {
            let mut a = appt.clone();
            (a.id, a.date, a.start, a.minutes, a.doctor_id, a.box_id, a.state, a.center_id) = (db.appointments.len() as u32 + 1, day(d), t(9, 0), min, doc, bx, st, 0);
            db.appointments.push(a);
        }
        for (d, doc, price) in [(16, 1, 100.0), (17, 1, 50.50), (16, 2, 80.0), (23, 2, 999.0)] {
            let id = db.add_entry(101, "0102", vec![], vec![], TxState::Previsto, doc, day(d));
            let e = db.entries.iter_mut().find(|e| e.id == id).unwrap();
            (e.price, e.discount_pct, e.units, e.center_id, e.state, e.tariff_id) = (price, 0.0, 1, 0, TxState::Realizado, 1);
        }
        for p in db.patients.iter_mut().filter(|p| p.id == 101) {
            (p.tariff_id, p.discount_pct) = (1, 0.0);
        }
        for (d, st, price) in [(16, BudgetState::Aceptado, 300.0), (17, BudgetState::Cerrado, 200.0), (18, BudgetState::Rechazado, 100.0), (19, BudgetState::Pendiente, 400.0), (15, BudgetState::Aceptado, 1000.0)] {
            let mut b = budget.clone();
            (b.id, b.date, b.state, b.doctor_id) = (db.budgets.len() as u32 + 1, day(d), st, 1);
            b.lines = vec![BudgetLine { price, units: 1, discount_pct: 0.0, ..b.lines[0].clone() }];
            db.budgets.push(b);
        }
        for (d, st) in [(16, RecallState::Citado), (17, RecallState::Citado), (18, RecallState::Rechazado), (19, RecallState::Pendiente)] {
            let mut r = recall.clone();
            (r.id, r.date, r.state, r.doctor_id) = (db.recalls.len() as u32 + 1, day(d), st, 1);
            db.recalls.push(r);
        }
        (db, Filter::period(day(16), day(22)))
    }

    #[test]
    fn each_indicator_matches_the_operations() {
        let (db, f) = week();
        let k = db.dashboard(&f);
        assert_eq!(k.doctors, vec![Occupancy { id: 1, minutes: Ratio { num: 225, den: 1110 } }, Occupancy { id: 2, minutes: Ratio { num: 60, den: 120 } }]);
        assert_eq!(k.doctors_total, Ratio { num: 285, den: 1230 });
        assert_eq!(k.boxes, vec![Occupancy { id: 1, minutes: Ratio { num: 225, den: 1200 } }, Occupancy { id: 2, minutes: Ratio { num: 60, den: 1200 } }]);
        assert_eq!(k.no_shows, NoShows { missed: 1, attended: 4, unclosed: 1 });
        assert_eq!(k.no_shows.rate().pct(), Some(20.0));
        assert_eq!(k.acceptance.count, Ratio { num: 2, den: 4 });
        assert_eq!(k.acceptance.amount, Ratio { num: 50_000, den: 100_000 });
        assert_eq!((k.acceptance.rejected, k.acceptance.pending), (1, 1));
        // 230,50 € en 210 minutos (60 + 60 + 30 + 60): 65,857… € la hora
        assert_eq!(k.chair_total, ChairHour { id: 0, production: Cents(23_050), minutes: 210 });
        assert_eq!(k.chair_total.per_hour(), Some(Cents(6_585)));
        assert_eq!(k.chair, vec![ChairHour { id: 1, production: Cents(15_050), minutes: 150 }, ChairHour { id: 2, production: Cents(8_000), minutes: 60 }]);
        assert_eq!(k.chair[0].per_hour(), Some(Cents(6_020)));
        assert_eq!(k.recalls, Recalls { due: 4, booked: 2, rejected: 1 });
        assert_eq!(k.recalls.rate().pct(), Some(50.0));
    }

    #[test]
    fn filters_comparison_and_goals() {
        let (db, f) = week();
        // solo el profesional 2
        let k = db.dashboard(&Filter { doctor: Some(2), ..f });
        assert_eq!(k.doctors_total, Ratio { num: 60, den: 120 });
        assert_eq!(k.boxes_total, Ratio { num: 60, den: 2400 });
        assert_eq!(k.no_shows, NoShows { missed: 0, attended: 1, unclosed: 0 });
        assert_eq!(k.acceptance.count, Ratio { num: 0, den: 0 });
        assert_eq!(k.acceptance.count.pct(), None, "sin presupuestos no hay porcentaje, no es un 0 %");
        assert_eq!(k.chair_total, ChairHour { id: 0, production: Cents(8_000), minutes: 60 });
        // otro centro: ningún gabinete, y la ocupación de los profesionales igual
        let k = db.dashboard(&Filter { center: Some(9), ..f });
        assert!(k.boxes.is_empty());
        assert_eq!(k.doctors_total, Ratio { num: 285, den: 1230 });
        // el periodo anterior, del lunes 9 al domingo 15
        let p = previous(&f);
        assert_eq!((p.from, p.to), (day(9), day(15)));
        assert_eq!(db.dashboard(&p).acceptance.count, Ratio { num: 1, den: 1 });
        // objetivos
        assert_eq!(meets(Some(20.0), Some(15.0), false), Some(false));
        assert_eq!(meets(Some(50.0), Some(40.0), true), Some(true));
        assert_eq!(meets(None, Some(40.0), true), None);
        assert_eq!(meets(Some(50.0), None, true), None);
    }

    #[test]
    fn overlapping_blocks_are_not_counted_twice() {
        let s = [crate::horario::Shift::new(t(9, 0), t(14, 0)), crate::horario::Shift::new(t(16, 0), t(18, 0))];
        assert_eq!(free_minutes(&s, &[]), 420);
        assert_eq!(free_minutes(&s, &[(600, 660), (630, 690), (1000, 1100)]), 420 - 90 - 80);
        assert_eq!(free_minutes(&s, &[(0, 1440)]), 0);
    }
}
