//! Horario de la clínica y de cada profesional, por día de la semana: qué días
//! se abre (también los sábados), con mañana y tarde si hace falta, y qué días
//! y horas pasa consulta cada profesional (el implantólogo de los miércoles…).
//! Lo usan la agenda, los huecos libres, la cita por internet, las citas
//! repetidas, los huecos ofrecidos a la lista de espera y los recordatorios.

use crate::model::*;
use chrono::{Datelike, NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};

/// Tramo de trabajo de un día (de `start` a `end`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shift {
    pub start: NaiveTime,
    pub end: NaiveTime,
}

impl Shift {
    pub fn new(start: NaiveTime, end: NaiveTime) -> Self {
        Shift { start, end }
    }
}

/// Horario de una semana: los tramos de cada día (0 = lunes … 6 = domingo);
/// un día sin tramos no se trabaja.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Week(pub [Vec<Shift>; 7]);

/// Nombre de cada día en plural, para los avisos («los sábados»).
pub const DAYS_PLURAL: [&str; 7] = ["lunes", "martes", "miércoles", "jueves", "viernes", "sábados", "domingos"];
/// Nombre corto de cada día, para la configuración.
pub const DAYS_SHORT: [&str; 7] = ["Lun", "Mar", "Mié", "Jue", "Vie", "Sáb", "Dom"];

impl Week {
    /// De lunes a viernes (o hasta el sábado, con `saturday`) de `open` a `close`.
    pub fn simple(open: NaiveTime, close: NaiveTime, saturday: bool) -> Self {
        let mut w = Week::default();
        for (i, d) in w.0.iter_mut().enumerate() {
            if i < 5 || (i == 5 && saturday) {
                d.push(Shift::new(open, close));
            }
        }
        w
    }

    pub fn day(&self, date: NaiveDate) -> &[Shift] {
        &self.0[date.weekday().num_days_from_monday() as usize]
    }

    /// Primera hora de apertura y última de cierre de la semana.
    pub fn span(&self) -> Option<(NaiveTime, NaiveTime)> {
        let all = self.0.iter().flatten();
        Some((all.clone().map(|s| s.start).min()?, all.map(|s| s.end).max()?))
    }

    /// Los tramos que tienen a la vez los dos horarios, día a día.
    pub fn within(&self, outer: &Week) -> Week {
        let mut w = Week::default();
        for i in 0..7 {
            w.0[i] = intersect(&self.0[i], &outer.0[i]);
        }
        w
    }
}

/// Tramos comunes a dos listas de tramos.
pub fn intersect(a: &[Shift], b: &[Shift]) -> Vec<Shift> {
    let mut out = vec![];
    for x in a {
        for y in b {
            let (s, e) = (x.start.max(y.start), x.end.min(y.end));
            if s < e {
                out.push(Shift::new(s, e));
            }
        }
    }
    out.sort_by_key(|s| s.start);
    out
}

/// «9:00 a 14:00 y 16:00 a 20:00».
pub fn shifts_text(v: &[Shift]) -> String {
    let parts: Vec<String> = v.iter().map(|s| format!("{} a {}", s.start.format("%-H:%M"), s.end.format("%-H:%M"))).collect();
    match parts.len() {
        0 => "cerrado".into(),
        1 => parts[0].clone(),
        n => format!("{} y {}", parts[..n - 1].join(", "), parts[n - 1]),
    }
}

impl Db {
    /// Horario de la clínica (sin configurar: de lunes a viernes, de la
    /// apertura al cierre, como antes).
    pub fn clinic_week(&self) -> Week {
        self.config.hours.clone().unwrap_or_else(|| Week::simple(self.config.open_time, self.config.close_time, false))
    }

    /// Horario de un profesional: el suyo dentro del de la clínica (sin el
    /// suyo, el de la clínica).
    pub fn doctor_week(&self, doctor_id: u32) -> Week {
        let clinic = self.clinic_week();
        match self.doctor(doctor_id).and_then(|c| c.hours.as_ref()) {
            Some(h) => h.within(&clinic),
            None => clinic,
        }
    }

    /// Tramos en que abre la clínica un día (ninguno si es festivo o no abre).
    pub fn clinic_day(&self, date: NaiveDate) -> Vec<Shift> {
        if self.holiday(date).is_some() {
            return vec![];
        }
        self.clinic_week().day(date).to_vec()
    }

    /// Tramos en que pasa consulta un profesional un día.
    pub fn doctor_day(&self, doctor_id: u32, date: NaiveDate) -> Vec<Shift> {
        if self.holiday(date).is_some() {
            return vec![];
        }
        self.doctor_week(doctor_id).day(date).to_vec()
    }

    /// El profesional pasa consulta todo el rato de `start` a `end` ese día.
    pub fn works_at(&self, doctor_id: u32, date: NaiveDate, start: NaiveTime, end: NaiveTime) -> bool {
        self.doctor_day(doctor_id, date).iter().any(|s| s.start <= start && end <= s.end)
    }

    /// Otra cita (no anulada) que ocupa ese gabinete del centro a esa hora.
    pub fn room_taken(&self, box_id: u32, center: u32, date: NaiveDate, start: NaiveTime, end: NaiveTime, ignore: Option<u32>) -> Option<&Appointment> {
        self.appointments.iter().find(|a| Some(a.id) != ignore && a.box_id == box_id && cid(a.center_id) == cid(center) && a.date == date && !a.state.is_cancelled() && a.start < end && start < a.end())
    }

    /// Por qué no se puede dar cita a esa hora con ese profesional según los
    /// horarios (festivo, la clínica no abre, el profesional no está…).
    /// `None` si está dentro de su horario.
    pub fn schedule_problem(&self, doctor_id: u32, date: NaiveDate, start: NaiveTime, end: NaiveTime) -> Option<String> {
        if self.works_at(doctor_id, date, start, end) {
            return None;
        }
        if let Some(h) = self.holiday(date) {
            return Some(format!("Es festivo: {}.", h.name));
        }
        let wd = date.weekday().num_days_from_monday() as usize;
        let clinic = self.clinic_week();
        if clinic.0[wd].is_empty() {
            return Some(format!("La clínica no abre los {}.", DAYS_PLURAL[wd]));
        }
        if !clinic.0[wd].iter().any(|s| s.start <= start && end <= s.end) {
            return Some(format!("Fuera del horario de la clínica (los {}, {}).", DAYS_PLURAL[wd], shifts_text(&clinic.0[wd])));
        }
        let who = self.doctor_alias(doctor_id);
        let mine = &self.doctor_week(doctor_id).0[wd];
        if mine.is_empty() {
            return Some(format!("{who} no pasa consulta los {}.", DAYS_PLURAL[wd]));
        }
        Some(format!("{who} pasa consulta los {} de {}.", DAYS_PLURAL[wd], shifts_text(mine)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    /// Un lunes cualquiera sin festivo.
    fn monday(db: &Db) -> NaiveDate {
        let mut d = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        while d.weekday().num_days_from_monday() != 0 || db.holiday(d).is_some() {
            d += chrono::Duration::days(1);
        }
        d
    }

    #[test]
    fn without_hours_it_is_monday_to_friday_as_before() {
        let db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap());
        let mon = monday(&db);
        let (open, close) = (db.config.open_time, db.config.close_time);
        assert!(db.works_at(1, mon, open, close));
        assert!(!db.works_at(1, mon + chrono::Duration::days(5), open, open + chrono::Duration::minutes(30)), "sábado");
        assert!(db.schedule_problem(1, mon + chrono::Duration::days(5), open, open + chrono::Duration::minutes(30)).unwrap().contains("sábados"));
    }

    #[test]
    fn saturdays_split_shifts_and_part_time_doctors() {
        let mut db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap());
        let mon = monday(&db);
        let (wed, fri, sat) = (mon + chrono::Duration::days(2), mon + chrono::Duration::days(4), mon + chrono::Duration::days(5));
        // jornada partida de lunes a jueves, viernes solo mañana, sábado por la mañana
        let mut w = Week::default();
        for d in 0..4 {
            w.0[d] = vec![Shift::new(t(9, 0), t(14, 0)), Shift::new(t(16, 0), t(20, 0))];
        }
        w.0[4] = vec![Shift::new(t(9, 0), t(15, 0))];
        w.0[5] = vec![Shift::new(t(9, 0), t(14, 0))];
        db.config.hours = Some(w);
        assert!(db.works_at(1, sat, t(10, 0), t(10, 30)), "el sábado por la mañana, sí");
        assert!(!db.works_at(1, mon, t(14, 0), t(14, 30)), "a mediodía, no");
        assert!(db.schedule_problem(1, mon, t(14, 0), t(14, 30)).unwrap().contains("9:00 a 14:00 y 16:00 a 20:00"));
        assert!(db.schedule_problem(1, fri, t(16, 0), t(16, 30)).unwrap().contains("viernes"));
        // el implantólogo solo los miércoles por la tarde
        let mut nav = Week::default();
        nav.0[2] = vec![Shift::new(t(15, 0), t(21, 0))];
        db.collaborators.iter_mut().find(|c| c.id == 2).unwrap().hours = Some(nav);
        assert!(db.works_at(2, wed, t(16, 0), t(17, 0)));
        assert!(!db.works_at(2, wed, t(15, 0), t(16, 0)), "dentro del horario de la clínica: desde las 16:00");
        assert!(db.schedule_problem(2, mon, t(10, 0), t(10, 30)).unwrap().contains("no pasa consulta los lunes"));
        assert!(db.schedule_problem(2, wed, t(10, 0), t(10, 30)).unwrap().contains("los miércoles de 16:00 a 20:00"));
        // los huecos libres lo respetan
        let gaps = db.find_gaps(2, mon, 60, t(8, 0), t(21, 0), 7);
        assert!(!gaps.is_empty() && gaps.iter().all(|(d, s)| *d == wed && *s >= t(16, 0)), "{gaps:?}");
        let sat_gaps = db.find_gaps(1, sat, 30, t(8, 0), t(21, 0), 1);
        assert!(!sat_gaps.is_empty() && sat_gaps.iter().all(|(_, s)| *s < t(14, 0)));
        // un festivo, cerrado aunque sea laborable
        db.holidays.push(crate::agenda_extra::Holiday { date: wed, name: "Fiesta local".into() });
        assert!(db.schedule_problem(2, wed, t(16, 0), t(17, 0)).unwrap().contains("Fiesta local"));
        assert!(!db.working_day(wed) && db.working_day(sat));
    }

    #[test]
    fn a_room_used_by_another_doctor_is_flagged() {
        let mut db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap());
        let mon = monday(&db);
        db.appointments.retain(|a| a.date != mon);
        let mut a = db.appointments[0].clone();
        (a.id, a.date, a.start, a.minutes, a.doctor_id, a.box_id, a.center_id, a.state) = (db.new_id(), mon, t(10, 0), 60, 1, 1, 1, ApptState::Planificada);
        db.appointments.push(a.clone());
        // otro doctor en el mismo gabinete a las 10:30
        let other = db.room_taken(1, 1, mon, t(10, 30), t(11, 0), None).map(|x| x.id);
        assert_eq!(other, Some(a.id));
        assert!(db.room_taken(1, 1, mon, t(11, 0), t(11, 30), None).is_none(), "justo al acabar, libre");
        assert!(db.room_taken(2, 1, mon, t(10, 30), t(11, 0), None).is_none(), "otro gabinete");
        assert!(db.room_taken(1, 1, mon, t(10, 30), t(11, 0), Some(a.id)).is_none(), "la propia cita no cuenta");
        db.appointments.iter_mut().find(|x| x.id == a.id).unwrap().state = ApptState::Anulada;
        assert!(db.room_taken(1, 1, mon, t(10, 30), t(11, 0), None).is_none(), "una anulada no ocupa");
    }

    #[test]
    fn shifts_are_intersected_and_described() {
        let a = vec![Shift::new(t(9, 0), t(14, 0)), Shift::new(t(16, 0), t(20, 0))];
        let b = vec![Shift::new(t(12, 0), t(18, 0))];
        assert_eq!(intersect(&a, &b), vec![Shift::new(t(12, 0), t(14, 0)), Shift::new(t(16, 0), t(18, 0))]);
        let mut w = Week::default();
        w.0[0] = a.clone();
        assert_eq!(w.span(), Some((t(9, 0), t(20, 0))));
        assert_eq!(shifts_text(&a), "9:00 a 14:00 y 16:00 a 20:00");
        assert_eq!(shifts_text(&[]), "cerrado");
    }
}
