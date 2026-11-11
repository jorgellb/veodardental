//! Agenda avanzada: festivos, citas periódicas, hoja de agenda del día y
//! liquidación de honorarios por colaborador.

use crate::model::*;
use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Holiday {
    pub date: NaiveDate,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    Weekly(u32),
    Monthly,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SeriesResult {
    pub created: Vec<NaiveDate>,
    pub skipped: Vec<(NaiveDate, String)>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

impl Db {
    pub fn holiday(&self, d: NaiveDate) -> Option<&Holiday> {
        self.holidays.iter().find(|h| h.date == d)
    }

    /// Día laborable para la agenda (ni fin de semana ni festivo).
    pub fn working_day(&self, d: NaiveDate) -> bool {
        d.weekday().num_days_from_monday() < 5 && self.holiday(d).is_none()
    }

    /// Crea las citas siguientes de una serie periódica a partir de una cita
    /// ya guardada. Si un día cae en festivo o el hueco está ocupado, busca
    /// otra hora libre ese mismo día; si no la hay, lo omite y lo explica.
    pub fn create_series(&mut self, base_id: u32, rep: Repeat, count: u32) -> SeriesResult {
        let mut res = SeriesResult::default();
        let Some(base) = self.appointments.iter().find(|a| a.id == base_id).cloned() else { return res };
        for i in 1..count.max(1) {
            let date = match rep {
                Repeat::Weekly(w) => base.date + Duration::weeks((w.max(1) * i) as i64),
                Repeat::Monthly => add_months(base.date, i),
            };
            if !self.working_day(date) {
                let why = match self.holiday(date) {
                    Some(h) => format!("festivo: {}", h.name),
                    None => "fin de semana".into(),
                };
                res.skipped.push((date, why));
                continue;
            }
            let end = base.start + Duration::minutes(base.minutes as i64);
            let start = if self.is_free(base.doctor_id, date, base.start, end, None) {
                Some(base.start)
            } else {
                self.find_gaps(base.doctor_id, date, base.minutes, self.config.open_time, self.config.close_time, 1).first().map(|g| g.1)
            };
            let Some(start) = start else {
                res.skipped.push((date, "agenda completa".into()));
                continue;
            };
            let id = self.new_id();
            let mut a = base.clone();
            a.id = id;
            a.date = date;
            a.start = start;
            a.state = ApptState::Planificada;
            a.arrived_at = None;
            a.seen_at = None;
            a.finished_at = None;
            a.reminder_sent = false;
            a.first_visit = false;
            self.appointments.push(a);
            res.created.push(date);
        }
        res
    }

    /// Reprograma una cita: la marca como «Reprogramada» y devuelve una copia
    /// lista para elegir nueva fecha (sin guardar).
    #[allow(dead_code)]
    pub fn reschedule(&mut self, id: u32) -> Option<Appointment> {
        let n = self.reschedule_draft(id)?;
        if let Some(a) = self.appointments.iter_mut().find(|a| a.id == id) {
            a.state = ApptState::Reprogramada;
        }
        Some(n)
    }

    /// Propone la nueva cita (primer hueco libre desde el día siguiente) sin
    /// tocar la original.
    pub fn reschedule_draft(&self, id: u32) -> Option<Appointment> {
        let a = self.appointments.iter().find(|a| a.id == id)?;
        let mut n = a.clone();
        n.id = 0;
        n.state = ApptState::Planificada;
        n.arrived_at = None;
        n.seen_at = None;
        n.finished_at = None;
        n.reminder_sent = false;
        n.notes = format!("Reprogramada desde el {} {}", fmt_date(a.date), a.start.format("%H:%M"));
        let gaps = self.find_gaps(n.doctor_id, n.date + Duration::days(1), n.minutes, self.config.open_time, self.config.close_time, 30);
        if let Some((d, t)) = gaps.first() {
            n.date = *d;
            n.start = *t;
        }
        Some(n)
    }

    /// Hoja imprimible con las citas del día (por doctor) del centro activo.
    pub fn day_sheet_html(&self, date: NaiveDate) -> String {
        let center = cid(self.config.current_center);
        let mut body = String::new();
        for d in self.collaborators.iter().filter(|c| c.active && self.works_in(c.id, center)) {
            let mut list: Vec<&Appointment> = self.appointments.iter().filter(|a| a.date == date && a.doctor_id == d.id && cid(a.center_id) == center && !a.state.is_cancelled()).collect();
            if list.is_empty() {
                continue;
            }
            list.sort_by_key(|a| a.start);
            body.push_str(&format!("<h2>{} <small>{}</small></h2><table><tr><th>Hora</th><th>Paciente</th><th>Teléfono</th><th>Motivo</th><th>Gabinete</th><th>Alertas</th></tr>", esc(&d.alias), esc(&d.specialty)));
            for a in list {
                let (phone, alert) = match a.patient_id.and_then(|p| self.patient(p)) {
                    Some(p) => (p.mobile.clone(), format!("{} {}", p.alerts, p.allergies).trim().to_string()),
                    None => (a.phone.clone(), String::new()),
                };
                body.push_str(&format!("<tr><td><b>{}</b> ({} min)</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class='al'>{}</td></tr>", a.start.format("%H:%M"), a.minutes, esc(&self.appt_name(a)), esc(&phone), esc(&a.reason), esc(&self.box_name(a.box_id)), esc(&alert)));
            }
            body.push_str("</table>");
        }
        if body.is_empty() {
            body = "<p>No hay citas.</p>".into();
        }
        format!(
            "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Agenda {d}</title><style>body{{font-family:'Open Sans',Arial,sans-serif;margin:28px;color:#1e293b}}h1{{font-size:22px}}h2{{font-size:17px;margin-top:22px;border-bottom:2px solid #0e9688}}small{{color:#64748b;font-weight:normal}}table{{width:100%;border-collapse:collapse}}th{{text-align:left;font-size:12px;color:#475569;background:#eef3f8;padding:6px}}td{{padding:6px;border-bottom:1px solid #e2e8f0;font-size:13px}}.al{{color:#c0392b}}@media print{{body{{margin:10mm}}}}</style></head><body><h1>Agenda del {d} · {c}</h1>{body}</body></html>",
            d = long_date(date),
            c = esc(&self.center_name(center)),
            body = body
        )
    }

    /// Liquidación de honorarios de un colaborador en un periodo.
    pub fn settlement(&self, doctor_id: u32, from: NaiveDate, to: NaiveDate) -> (Vec<&MedEntry>, f64, f64) {
        let pct = self.doctor(doctor_id).map(|d| d.commission_pct).unwrap_or(0.0);
        let mut v: Vec<&MedEntry> = self.entries.iter().filter(|e| e.doctor_id == doctor_id && e.state == TxState::Realizado && e.date >= from && e.date <= to).collect();
        v.sort_by_key(|e| (e.date, e.id));
        let prod: f64 = round2(v.iter().map(|e| e.amount()).sum());
        (v, prod, round2(prod * pct / 100.0))
    }

    pub fn settlement_html(&self, doctor_id: u32, from: NaiveDate, to: NaiveDate) -> String {
        let d = self.doctor(doctor_id).cloned();
        let (rows, prod, fee) = self.settlement(doctor_id, from, to);
        let lines: String = rows
            .iter()
            .map(|e| {
                let teeth: Vec<String> = e.teeth.iter().map(|t| t.to_string()).collect();
                format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class='n'>{}</td></tr>", fmt_date(e.date), esc(&self.patient_name(e.patient_id)), esc(&e.description), teeth.join("-"), esc(&money(e.amount())))
            })
            .collect();
        format!(
            "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Liquidación</title><style>body{{font-family:'Open Sans',Arial,sans-serif;max-width:820px;margin:32px auto;color:#1e293b}}h1{{font-size:21px}}table{{width:100%;border-collapse:collapse;margin-top:12px}}th{{text-align:left;font-size:12px;color:#475569;background:#eef3f8;padding:6px}}td{{padding:6px;border-bottom:1px solid #e2e8f0;font-size:13px}}.n{{text-align:right}}.tot td{{font-weight:700;border:none;font-size:15px}}</style></head><body><b>{clinic}</b><h1>Liquidación de honorarios · {name}</h1><p>Periodo del {from} al {to} · Nº colegiado {lic} · Comisión {pct:.0} %</p><table><tr><th>Fecha</th><th>Paciente</th><th>Tratamiento</th><th>Piezas</th><th class='n'>Importe</th></tr>{lines}<tr class='tot'><td colspan='4' class='n'>Producción</td><td class='n'>{prod}</td></tr><tr class='tot'><td colspan='4' class='n'>Honorarios a liquidar</td><td class='n'>{fee}</td></tr></table><p style='margin-top:50px'>Conforme: ______________________</p></body></html>",
            clinic = esc(&self.config.name),
            name = esc(&d.as_ref().map(|x| x.name.clone()).unwrap_or_default()),
            from = fmt_date(from),
            to = fmt_date(to),
            lic = esc(&d.as_ref().map(|x| x.license.clone()).unwrap_or_default()),
            pct = d.as_ref().map(|x| x.commission_pct).unwrap_or(0.0),
            lines = lines,
            prod = esc(&money(prod)),
            fee = esc(&money(fee)),
        )
    }
}

/// Festivos nacionales de ejemplo para el año indicado (editables).
pub fn default_holidays(year: i32) -> Vec<Holiday> {
    let h = |m: u32, d: u32, n: &str| Holiday { date: NaiveDate::from_ymd_opt(year, m, d).unwrap(), name: n.into() };
    vec![
        h(1, 1, "Año Nuevo"),
        h(1, 6, "Epifanía del Señor"),
        h(5, 1, "Fiesta del Trabajo"),
        h(8, 15, "Asunción de la Virgen"),
        h(10, 12, "Fiesta Nacional de España"),
        h(11, 1, "Todos los Santos"),
        h(12, 6, "Día de la Constitución"),
        h(12, 8, "Inmaculada Concepción"),
        h(12, 25, "Navidad"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()
    }

    #[test]
    fn weekly_series_skips_holidays_and_conflicts() {
        let mut db = Db::sample(d());
        db.holidays = default_holidays(2026);
        // cita base: miércoles 4/11/2026 a las 13:00 con la Dra. Ruiz
        let id = db.new_id();
        let mut a = db.appointments.iter().find(|a| a.doctor_id == 1).unwrap().clone();
        a.id = id;
        a.date = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        a.start = NaiveTime::from_hms_opt(13, 0, 0).unwrap();
        a.minutes = 30;
        db.appointments.push(a);
        // 4 semanas: 25/11, 2/12, 9/12, 16/12 (8/12 es festivo pero es martes → no afecta)
        let r = db.create_series(id, Repeat::Weekly(1), 4);
        assert_eq!(r.created.len(), 3);
        // una serie que cae en festivo (Navidad: viernes 25/12)
        let id2 = db.new_id();
        let mut b = db.appointments.iter().find(|x| x.id == id).unwrap().clone();
        b.id = id2;
        b.date = NaiveDate::from_ymd_opt(2026, 12, 18).unwrap();
        db.appointments.push(b);
        let r2 = db.create_series(id2, Repeat::Weekly(1), 2);
        assert_eq!(r2.created.len(), 0);
        assert!(r2.skipped[0].1.contains("Navidad"));
    }

    #[test]
    fn reschedule_marks_original() {
        let mut db = Db::sample(d());
        let a = db.appointments.iter().find(|a| a.state == ApptState::Planificada).unwrap().clone();
        let n = db.reschedule(a.id).unwrap();
        assert_eq!(db.appointments.iter().find(|x| x.id == a.id).unwrap().state, ApptState::Reprogramada);
        assert!(n.date > a.date);
        assert!(db.is_free(n.doctor_id, n.date, n.start, n.start + Duration::minutes(n.minutes as i64), None));
    }

    #[test]
    fn settlement_matches_production() {
        let db = Db::sample(d());
        let (rows, prod, fee) = db.settlement(2, d() - Duration::days(365), d());
        assert!(!rows.is_empty());
        assert_eq!(fee, round2(prod * 0.40));
        assert!(db.settlement_html(2, d() - Duration::days(365), d()).contains("Honorarios a liquidar"));
        assert!(db.day_sheet_html(d()).contains("<table>"));
    }
}
