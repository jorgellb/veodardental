//! Agenda avanzada: festivos, citas periódicas, hoja de agenda del día y
//! liquidación de honorarios por colaborador.

use crate::model::*;
use chrono::{Duration, NaiveDate};
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
        !self.clinic_day(d).is_empty()
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
            let start = if self.is_free(base.doctor_id, date, base.start, end, None) { Some(base.start) } else { self.find_gaps(base.doctor_id, date, base.minutes, self.config.open_time, self.config.close_time, 1).first().map(|g| g.1) };
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
        self.refresh_recalls(today());
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
                body.push_str(&format!(
                    "<tr><td><b>{}</b> ({} min)</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class='al'>{}</td></tr>",
                    a.start.format("%H:%M"),
                    a.minutes,
                    esc(&self.appt_name(a)),
                    esc(&phone),
                    esc(&a.reason),
                    esc(&self.box_name(a.box_id)),
                    esc(&alert)
                ));
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
    /// Coste de laboratorio de un tratamiento (sus trabajos de laboratorio).
    pub fn lab_cost_of(&self, entry_id: u32) -> f64 {
        round2(self.lab_works.iter().filter(|w| w.entry_id == Some(entry_id)).map(|w| w.cost).sum())
    }

    /// Lo que cuenta un acto para la comisión de quien lo hizo: lo que paga
    /// el paciente y, si el colaborador lo tiene marcado, también la mutua.
    pub fn commission_amount(&self, e: &MedEntry) -> f64 {
        let insurer = self.doctor(e.doctor_id).is_some_and(|d| d.commission_on_insurer);
        if insurer { round2(e.amount() + self.insurer_part(e).eur()) } else { e.amount() }
    }

    /// Liquidación de un colaborador: sus tratamientos realizados, lo producido
    /// que cuenta para su comisión (ver `commission_amount`), el laboratorio
    /// que se le descuenta (0 si no se le descuenta) y los honorarios.
    pub fn settlement(&self, doctor_id: u32, from: NaiveDate, to: NaiveDate) -> (Vec<&MedEntry>, f64, f64, f64) {
        let d = self.doctor(doctor_id);
        let pct = d.map(|d| d.commission_pct).unwrap_or(0.0);
        let mut v: Vec<&MedEntry> = self.entries.iter().filter(|e| e.doctor_id == doctor_id && e.state == TxState::Realizado && e.date >= from && e.date <= to).collect();
        v.sort_by_key(|e| (e.date, e.id));
        let prod: f64 = round2(v.iter().map(|e| self.commission_amount(e)).sum());
        let lab = if d.is_some_and(|d| d.deduct_lab) { round2(v.iter().map(|e| self.lab_cost_of(e.id)).sum()) } else { 0.0 };
        (v, prod, lab, round2((prod - lab) * pct / 100.0))
    }

    pub fn settlement_html(&self, doctor_id: u32, from: NaiveDate, to: NaiveDate) -> String {
        let d = self.doctor(doctor_id).cloned();
        let deduct = d.as_ref().is_some_and(|d| d.deduct_lab);
        let (rows, prod, lab, fee) = self.settlement(doctor_id, from, to);
        let lines: String = rows
            .iter()
            .map(|e| {
                let teeth: Vec<String> = e.teeth.iter().map(|t| t.to_string()).collect();
                let lab_cell = if deduct {
                    format!(
                        "<td class='n'>{}</td>",
                        esc(&match self.lab_cost_of(e.id) {
                            c if c > 0.0 => money(c),
                            _ => String::new(),
                        })
                    )
                } else {
                    String::new()
                };
                format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td>{lab_cell}<td class='n'>{}</td></tr>", fmt_date(e.date), esc(&self.patient_name(e.patient_id)), esc(&e.description), teeth.join("-"), esc(&money(self.commission_amount(e))))
            })
            .collect();
        let cols = if deduct { 5 } else { 4 };
        let lab_head = if deduct { "<th class='n'>Laboratorio</th>" } else { "" };
        let lab_rows = if deduct {
            format!(
                "<tr class='tot'><td colspan='{cols}' class='n'>Laboratorio</td><td class='n'>−{}</td></tr><tr class='tot'><td colspan='{cols}' class='n'>Base de los honorarios</td><td class='n'>{}</td></tr>",
                esc(&money(lab)),
                esc(&money(round2(prod - lab)))
            )
        } else {
            String::new()
        };
        let paid_by = if d.as_ref().is_some_and(|d| d.commission_on_insurer) { "lo que pagan el paciente y la mutua" } else { "lo que paga el paciente" };
        let basis = if deduct { format!(" sobre {paid_by}, menos el laboratorio") } else { format!(" sobre {paid_by}") };
        format!(
            "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Liquidación</title><style>body{{font-family:'Open Sans',Arial,sans-serif;max-width:820px;margin:32px auto;color:#1e293b}}h1{{font-size:21px}}table{{width:100%;border-collapse:collapse;margin-top:12px}}th{{text-align:left;font-size:12px;color:#475569;background:#eef3f8;padding:6px}}td{{padding:6px;border-bottom:1px solid #e2e8f0;font-size:13px}}.n{{text-align:right}}.tot td{{font-weight:700;border:none;font-size:15px}}</style></head><body><b>{clinic}</b><h1>Liquidación de honorarios · {name}</h1><p>Periodo del {from} al {to} · Nº colegiado {lic} · Comisión {pct:.0} %{basis}</p><table><tr><th>Fecha</th><th>Paciente</th><th>Tratamiento</th><th>Piezas</th>{lab_head}<th class='n'>Importe</th></tr>{lines}<tr class='tot'><td colspan='{cols}' class='n'>Producción</td><td class='n'>{prod}</td></tr>{lab_rows}<tr class='tot'><td colspan='{cols}' class='n'>Honorarios a liquidar</td><td class='n'>{fee}</td></tr></table><p style='margin-top:50px'>Conforme: ______________________</p></body></html>",
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

impl Db {
    /// Datos para la ficha de la persona de una cita de primera visita (sin
    /// ficha): nombre y apellidos, teléfono y, si la pidió por internet, su correo.
    pub fn patient_from_appt(&self, appt: u32) -> Option<Patient> {
        let a = self.appointments.iter().find(|a| a.id == appt && a.patient_id.is_none())?;
        let name = a.free_name.trim();
        let (first, last) = name.split_once(' ').map(|(f, l)| (f.trim(), l.trim())).unwrap_or((name, ""));
        let email = self.booking_requests.iter().find(|r| r.appointment_id == Some(appt)).map(|r| r.email.trim().to_string()).unwrap_or_default();
        Some(Patient { first_name: first.to_string(), last_name: last.to_string(), mobile: a.phone.trim().to_string(), email, doctor_id: a.doctor_id, ..Default::default() })
    }

    /// Ficha (activa) que ya tiene ese teléfono.
    pub fn patient_with_phone(&self, phone: &str) -> Option<u32> {
        self.patients.iter().filter(|p| p.active).find(|p| crate::booking::same_phone(&p.mobile, phone) || crate::booking::same_phone(&p.phone, phone)).map(|p| p.id)
    }

    /// Pone la ficha `pid` en la cita de primera visita `appt` y en las demás
    /// citas sin ficha de la misma persona (mismo nombre y teléfono), y en sus
    /// solicitudes por internet. Devuelve cuántas citas.
    pub fn link_first_visit(&mut self, appt: u32, pid: u32) -> usize {
        let Some(a) = self.appointments.iter().find(|a| a.id == appt).cloned() else { return 0 };
        let same = |x: &Appointment| x.id == appt || (x.patient_id.is_none() && fold(x.free_name.trim()) == fold(a.free_name.trim()) && crate::booking::same_phone(&x.phone, &a.phone));
        let mut ids = vec![];
        for x in self.appointments.iter_mut().filter(|x| same(x)) {
            x.patient_id = Some(pid);
            x.free_name.clear();
            ids.push(x.id);
        }
        for r in self.booking_requests.iter_mut().filter(|r| r.appointment_id.is_some_and(|i| ids.contains(&i))) {
            r.patient_id = Some(pid);
        }
        ids.len()
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

/// Carriles de las citas de una columna de la agenda: las que se solapan se
/// ponen una al lado de otra para que ninguna tape a otra. Cada cita (id,
/// inicio, fin en minutos) recibe su carril y cuántos carriles tiene su grupo
/// de citas solapadas; las que no se solapan con ninguna, (0, 1).
pub fn lanes(items: &[(u32, i64, i64)]) -> std::collections::HashMap<u32, (usize, usize)> {
    let mut v = items.to_vec();
    v.sort_by_key(|&(id, s, e)| (s, std::cmp::Reverse(e), id));
    let mut out = std::collections::HashMap::new();
    let mut i = 0;
    while i < v.len() {
        // grupo: citas que se solapan entre sí, directa o indirectamente
        let mut end = v[i].2;
        let mut j = i + 1;
        while j < v.len() && v[j].1 < end {
            end = end.max(v[j].2);
            j += 1;
        }
        // en cada carril, la primera cita que cabe después de la anterior
        let mut lane_end: Vec<i64> = vec![];
        let mut placed = vec![];
        for &(id, s, e) in &v[i..j] {
            let lane = match lane_end.iter().position(|&le| le <= s) {
                Some(l) => l,
                None => {
                    lane_end.push(e);
                    lane_end.len() - 1
                }
            };
            lane_end[lane] = e;
            placed.push((id, lane));
        }
        for (id, lane) in placed {
            out.insert(id, (lane, lane_end.len()));
        }
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_visit_becomes_a_patient_with_all_their_appointments() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let mut db = Db::sample(today);
        let mut a = db.appointments[0].clone();
        (a.id, a.patient_id, a.free_name, a.phone, a.first_visit, a.date) = (db.new_id(), None, "Lucía Gómez Ortega".into(), "+34 611 222 333".into(), true, today);
        db.appointments.push(a.clone());
        // otra cita suya (otro día) y la de otra persona
        let mut b = a.clone();
        (b.id, b.date) = (db.new_id(), today + Duration::days(7));
        db.appointments.push(b.clone());
        let mut c = a.clone();
        (c.id, c.free_name, c.phone) = (db.new_id(), "Pedro Ruiz".into(), "622 000 000".into());
        db.appointments.push(c.clone());
        // la pidió por internet con su correo
        let rid = db.new_id();
        db.booking_requests.push(crate::booking::BookingRequest {
            id: rid,
            received: now(),
            name: "Lucía Gómez Ortega".into(),
            phone: a.phone.clone(),
            email: "lucia@example.com".into(),
            reason: "Revisión".into(),
            minutes: 30,
            code: String::new(),
            doctor_id: a.doctor_id,
            date: a.date,
            start: a.start,
            notes: String::new(),
            first_visit: true,
            patient_id: None,
            state: crate::booking::ReqState::Confirmada,
            appointment_id: Some(a.id),
            answer: String::new(),
            center_id: 1,
            replaces: None,
        });
        let p = db.patient_from_appt(a.id).unwrap();
        assert_eq!((p.first_name.as_str(), p.last_name.as_str(), p.mobile.as_str(), p.email.as_str()), ("Lucía", "Gómez Ortega", "+34 611 222 333", "lucia@example.com"));
        assert_eq!(db.patient_with_phone("611 222 333"), None, "aún no tiene ficha");
        let pid = db.new_id();
        db.patients.push(Patient { id: pid, first_name: p.first_name.clone(), last_name: p.last_name.clone(), mobile: p.mobile.clone(), active: true, ..Default::default() });
        assert_eq!(db.link_first_visit(a.id, pid), 2, "sus dos citas");
        let pat = |id: u32, db: &Db| db.appointments.iter().find(|x| x.id == id).unwrap().patient_id;
        assert_eq!((pat(a.id, &db), pat(b.id, &db), pat(c.id, &db)), (Some(pid), Some(pid), None));
        assert!(db.booking_requests.iter().any(|r| r.appointment_id == Some(a.id) && r.patient_id == Some(pid)));
        // la próxima vez se encuentra por el teléfono
        assert_eq!(db.patient_with_phone("611222333"), Some(pid));
    }

    #[test]
    fn overlapping_appointments_go_side_by_side() {
        // 9:00-9:30 y 9:15-10:00 se solapan; 10:00-10:30 va sola (empieza cuando acaba la otra)
        let l = lanes(&[(1, 540, 570), (2, 555, 600), (3, 600, 630)]);
        assert_eq!(l[&1], (0, 2));
        assert_eq!(l[&2], (1, 2));
        assert_eq!(l[&3], (0, 1));
        // tres a la vez y una que cabe en el hueco de la primera
        let l = lanes(&[(1, 540, 560), (2, 540, 600), (3, 545, 600), (4, 565, 590)]);
        assert_eq!(l.values().map(|x| x.1).max(), Some(3));
        assert_eq!(l[&4].0, l[&1].0, "la cuarta usa el carril de la primera, que ya acabó");
        let mut seen: Vec<usize> = [1, 2, 3].iter().map(|id| l[id].0).collect();
        seen.sort();
        assert_eq!(seen, vec![0, 1, 2]);
        assert!(lanes(&[]).is_empty());
    }
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
        let (rows, prod, lab, fee) = db.settlement(2, d() - Duration::days(365), d());
        assert!(!rows.is_empty());
        assert_eq!((lab, fee), (0.0, round2(prod * 0.40)));
        assert!(db.settlement_html(2, d() - Duration::days(365), d()).contains("Honorarios a liquidar"));
    }

    #[test]
    fn settlement_can_take_off_the_lab_cost() {
        let mut db = Db::sample(d());
        let from = d() - Duration::days(365);
        // una corona del Dr. (id 2) de 600 € con 160 € de laboratorio
        let e = db.add_entry(110, "0802", vec![11], vec![], TxState::Realizado, 2, d());
        db.entries.iter_mut().find(|x| x.id == e).unwrap().price = 600.0;
        let w = db.new_lab_work(110, Some(e), 3, 7);
        db.lab_works.iter_mut().find(|x| x.id == w).unwrap().cost = 160.0;
        let (_, prod, lab, fee) = db.settlement(2, from, d());
        assert_eq!((lab, fee), (0.0, round2(prod * 0.40)), "sin marcar, como siempre");
        db.collaborators.iter_mut().find(|c| c.id == 2).unwrap().deduct_lab = true;
        let (_, prod2, lab, fee) = db.settlement(2, from, d());
        assert_eq!(prod2, prod);
        assert!(lab >= 160.0);
        assert_eq!(fee, round2((prod - lab) * 0.40));
        let html = db.settlement_html(2, from, d());
        assert!(html.contains("Base de los honorarios") && html.contains("160,00"), "{html}");
        assert!(db.day_sheet_html(d()).contains("<table>"));
    }
}
