//! Registro de jornada (T-F4-06; ver `docs/normativa.md`, Estatuto de los
//! Trabajadores art. 34.9). Cada persona ficha lo suyo: entrada, inicio y fin
//! de pausa, y salida, a la hora del momento.
//! - **Correcciones trazables:** solo las hace Dirección, con motivo. Se
//!   añaden como una marca nueva que sustituye a otra, y la original se
//!   conserva. Nunca se borra ni se cambia una marca: en red, el servidor solo
//!   admite añadir, y pone él la hora de cada fichaje.
//! - **Acceso:** cada persona ve lo suyo y Dirección todo.
//! - **Exportación:** en CSV, para la persona trabajadora, sus representantes
//!   o la Inspección.
//!
//! Una pantalla de fichaje no demuestra el cumplimiento: falta la revisión
//! laboral (B-26).

use crate::model::Db;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Entrada,
    InicioPausa,
    FinPausa,
    Salida,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Entrada => "Entrada",
            Kind::InicioPausa => "Inicio de pausa",
            Kind::FinPausa => "Fin de pausa",
            Kind::Salida => "Salida",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Punch {
    pub id: u32,
    pub user_id: u32,
    pub at: NaiveDateTime,
    pub kind: Kind,
    #[serde(default)]
    pub station: u32,
    /// Marca a la que sustituye (una corrección); la original se conserva.
    #[serde(default)]
    pub corrects: Option<u32>,
    /// Corrección o marca añadida por Dirección: por qué y quién.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub by: String,
    /// Anula la marca de `corrects` sin poner otra en su lugar.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub void: bool,
}

/// Las marcas que valen de una persona un día (con las correcciones puestas),
/// por hora.
pub fn effective(list: &[Punch], user: u32, day: NaiveDate) -> Vec<Punch> {
    let replaced: std::collections::HashSet<u32> = list.iter().filter_map(|p| p.corrects).collect();
    let mut v: Vec<Punch> = list.iter().filter(|p| p.user_id == user && p.at.date() == day && !replaced.contains(&p.id) && !p.void).cloned().collect();
    v.sort_by_key(|p| (p.at, p.id));
    v
}

/// Qué toca fichar ahora según lo que ya hay hoy.
pub fn next_allowed(day: &[Punch]) -> Vec<Kind> {
    match day.last().map(|p| p.kind) {
        None | Some(Kind::Salida) => vec![Kind::Entrada],
        Some(Kind::Entrada) | Some(Kind::FinPausa) => vec![Kind::InicioPausa, Kind::Salida],
        Some(Kind::InicioPausa) => vec![Kind::FinPausa],
    }
}

/// Resumen de un día: (primera entrada, última salida, minutos de pausa,
/// minutos trabajados, ¿falta alguna marca?).
pub fn day_summary(day: &[Punch]) -> (Option<NaiveDateTime>, Option<NaiveDateTime>, i64, i64, bool) {
    let (mut worked, mut paused, mut open, mut pause_start, mut incomplete) = (0i64, 0i64, None::<NaiveDateTime>, None::<NaiveDateTime>, false);
    for p in day {
        match p.kind {
            Kind::Entrada => open = Some(p.at),
            Kind::InicioPausa => {
                if let Some(s) = open.take() {
                    worked += (p.at - s).num_minutes();
                }
                pause_start = Some(p.at);
            }
            Kind::FinPausa => {
                if let Some(s) = pause_start.take() {
                    paused += (p.at - s).num_minutes();
                }
                open = Some(p.at);
            }
            Kind::Salida => match open.take() {
                Some(s) => worked += (p.at - s).num_minutes(),
                None => incomplete = true,
            },
        }
    }
    incomplete |= open.is_some() || pause_start.is_some();
    let first = day.iter().find(|p| p.kind == Kind::Entrada).map(|p| p.at);
    let last = day.iter().rev().find(|p| p.kind == Kind::Salida).map(|p| p.at);
    (first, last, paused, worked, incomplete)
}

impl Db {
    /// Ficha `user` ahora (`at`, la hora del momento).
    pub fn punch(&mut self, user: u32, kind: Kind, at: NaiveDateTime) -> Result<u32, String> {
        let today = effective(&self.punches, user, at.date());
        if !next_allowed(&today).contains(&kind) {
            return Err(format!("Ahora no toca «{}»: {}.", kind.label(), next_allowed(&today).iter().map(|k| k.label()).collect::<Vec<_>>().join(" o ")));
        }
        let id = self.new_id();
        let station = self.station;
        self.punches.push(Punch { id, user_id: user, at, kind, station, corrects: None, reason: String::new(), by: String::new(), void: false });
        Ok(id)
    }

    /// Corrección de Dirección: sustituye (o anula, sin `kind`) la marca
    /// `original`, o añade una que faltaba (sin `original`). Siempre con motivo.
    #[allow(clippy::too_many_arguments)]
    pub fn correct_punch(&mut self, original: Option<u32>, user: u32, kind: Option<Kind>, at: NaiveDateTime, reason: &str, is_admin: bool) -> Result<u32, String> {
        if !is_admin {
            return Err("Solo Dirección corrige el registro de jornada.".into());
        }
        if reason.trim().is_empty() {
            return Err("Escriba el motivo de la corrección.".into());
        }
        if let Some(o) = original
            && !self.punches.iter().any(|p| p.id == o && p.user_id == user)
        {
            return Err("La marca que se corrige no existe.".into());
        }
        if original.is_none() && kind.is_none() {
            return Err("Diga qué marca falta.".into());
        }
        let id = self.new_id();
        let by = self.config.current_user.clone();
        let station = self.station;
        self.punches.push(Punch { id, user_id: user, at, kind: kind.unwrap_or(Kind::Entrada), station, corrects: original, reason: reason.trim().into(), by, void: kind.is_none() });
        self.audit("Corrección del registro de jornada", &format!("{} (motivo: {})", self.user(user).map(|u| u.name.clone()).unwrap_or_default(), reason.trim()));
        Ok(id)
    }

    /// Lo que puede ver `viewer`: lo suyo, o todo si es Dirección.
    pub fn punches_for(&self, viewer: u32, is_admin: bool) -> Vec<&Punch> {
        self.punches.iter().filter(|p| is_admin || p.user_id == viewer).collect()
    }
}

/// CSV de un periodo (separado por «;»): una fila por persona y día, y debajo
/// cada corrección con su motivo.
pub fn csv(db: &Db, users: &[u32], from: NaiveDate, to: NaiveDate) -> String {
    let cell = |s: &str| crate::model::sheet_cell(s, ';');
    let mut s = String::from("persona;fecha;entrada;salida;pausas (min);trabajado (min);incompleto\n");
    let mut day = from;
    while day <= to {
        for u in users {
            let marks = effective(&db.punches, *u, day);
            if marks.is_empty() {
                continue;
            }
            let (first, last, paused, worked, incomplete) = day_summary(&marks);
            let name = db.user(*u).map(|x| x.name.clone()).unwrap_or_default();
            let hm = |t: Option<NaiveDateTime>| t.map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
            s += &format!("{};{};{};{};{paused};{worked};{}\n", cell(&name), day.format("%d/%m/%Y"), hm(first), hm(last), if incomplete { "sí" } else { "no" });
        }
        day = day.succ_opt().unwrap_or(to);
        if day == to && from == to {
            break;
        }
    }
    let fixes: Vec<&Punch> = db.punches.iter().filter(|p| users.contains(&p.user_id) && (p.corrects.is_some() || !p.reason.is_empty()) && p.at.date() >= from && p.at.date() <= to).collect();
    if !fixes.is_empty() {
        s += "\ncorrecciones;fecha y hora;marca;sustituye a;motivo;por\n";
        for p in fixes {
            let name = db.user(p.user_id).map(|x| x.name.clone()).unwrap_or_default();
            let what = if p.void { "anulada".to_string() } else { p.kind.label().to_string() };
            let orig = p.corrects.and_then(|o| db.punches.iter().find(|x| x.id == o)).map(|o| format!("{} {}", o.kind.label(), o.at.format("%d/%m/%Y %H:%M"))).unwrap_or_else(|| "(faltaba)".into());
            s += &format!("{};{};{what};{orig};{};{}\n", cell(&name), p.at.format("%d/%m/%Y %H:%M"), cell(&p.reason), cell(&p.by));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 13).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn a_day_with_a_pause_adds_up_and_the_order_is_kept() {
        let mut db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 13).unwrap());
        let u = 2;
        assert!(db.punch(u, Kind::Salida, t(8, 0)).unwrap_err().contains("Entrada"), "sin entrar no se sale");
        db.punch(u, Kind::Entrada, t(9, 0)).unwrap();
        assert!(db.punch(u, Kind::Entrada, t(9, 1)).is_err(), "dos entradas seguidas, no");
        db.punch(u, Kind::InicioPausa, t(11, 0)).unwrap();
        assert!(db.punch(u, Kind::Salida, t(11, 10)).is_err(), "en pausa, primero el fin de pausa");
        db.punch(u, Kind::FinPausa, t(11, 30)).unwrap();
        db.punch(u, Kind::Salida, t(14, 0)).unwrap();
        let day = effective(&db.punches, u, t(0, 0).date());
        assert_eq!(day_summary(&day), (Some(t(9, 0)), Some(t(14, 0)), 30, 270, false));
        // otra persona, otro registro
        assert!(effective(&db.punches, 3, t(0, 0).date()).is_empty());
    }

    /// Las correcciones las hace Dirección, con motivo; la marca original se
    /// conserva y la corrección cuenta en los totales y en la exportación.
    #[test]
    fn corrections_keep_the_original_and_say_why() {
        let mut db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 13).unwrap());
        let u = 2;
        let e = db.punch(u, Kind::Entrada, t(9, 40)).unwrap();
        assert!(db.correct_punch(Some(e), u, Some(Kind::Entrada), t(9, 0), "Olvidó fichar al llegar", false).unwrap_err().contains("Solo Dirección"));
        assert!(db.correct_punch(Some(e), u, Some(Kind::Entrada), t(9, 0), " ", true).unwrap_err().contains("motivo"));
        db.correct_punch(Some(e), u, Some(Kind::Entrada), t(9, 0), "Olvidó fichar al llegar", true).unwrap();
        db.correct_punch(None, u, Some(Kind::Salida), t(17, 0), "Salida no fichada (confirmada por la persona)", true).unwrap();
        assert!(db.punches.iter().any(|p| p.id == e && p.at == t(9, 40)), "la original sigue");
        let day = effective(&db.punches, u, t(0, 0).date());
        assert_eq!(day_summary(&day).3, 8 * 60);
        let out = csv(&db, &[u], t(0, 0).date(), t(0, 0).date());
        assert!(out.contains(";13/11/2026;09:00;17:00;0;480;no\n"), "{out}");
        assert!(out.contains("Olvidó fichar al llegar") && out.contains("Entrada 13/11/2026 09:40") && out.contains("(faltaba)"), "{out}");
        assert!(db.audit_log.iter().filter(|a| a.action == "Corrección del registro de jornada").count() == 2);
        // cada persona ve lo suyo; Dirección, todo
        db.punch(3, Kind::Entrada, t(8, 0)).unwrap();
        assert!(db.punches_for(u, false).iter().all(|p| p.user_id == u));
        assert!(db.punches_for(1, true).iter().any(|p| p.user_id == 3));
    }
}
