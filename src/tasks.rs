//! Tareas y avisos internos del equipo (recepción, doctores, dirección).
//! Además de las tareas que crea cada usuario, el programa propone solo las
//! llamadas y gestiones que suelen olvidarse.

use crate::model::*;
use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: u32,
    pub created: NaiveDate,
    pub by: String,
    /// Usuario al que va dirigida (vacío = cualquiera del equipo).
    pub to: String,
    pub patient_id: Option<u32>,
    pub text: String,
    pub due: Option<NaiveDate>,
    pub done_on: Option<NaiveDate>,
    pub done_by: String,
    /// Clave de las tareas automáticas, para no repetirlas.
    pub auto: String,
}

impl Task {
    pub fn pending(&self) -> bool {
        self.done_on.is_none()
    }
    pub fn overdue(&self, today: NaiveDate) -> bool {
        self.pending() && self.due.is_some_and(|d| d < today)
    }
    pub fn for_user(&self, user: &str) -> bool {
        self.to.is_empty() || self.to == user
    }
}

impl Db {
    pub fn add_task(&mut self, today: NaiveDate, by: &str, to: &str, patient_id: Option<u32>, text: &str, due: Option<NaiveDate>) -> u32 {
        let id = self.new_id();
        self.tasks.push(Task { id, created: today, by: by.into(), to: to.into(), patient_id, text: text.trim().into(), due, done_on: None, done_by: String::new(), auto: String::new() });
        id
    }

    pub fn complete_task(&mut self, id: u32, today: NaiveDate, user: &str, done: bool) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            if done {
                t.done_on = Some(today);
                t.done_by = user.into();
            } else {
                t.done_on = None;
                t.done_by.clear();
            }
        }
    }

    /// Tareas pendientes visibles para un usuario, las más urgentes primero.
    pub fn pending_tasks(&self, user: &str) -> Vec<&Task> {
        let mut v: Vec<&Task> = self.tasks.iter().filter(|t| t.pending() && t.for_user(user)).collect();
        v.sort_by_key(|t| (t.due.unwrap_or(NaiveDate::MAX), t.id));
        v
    }

    fn has_future_appt(&self, pid: u32, today: NaiveDate) -> bool {
        self.appointments.iter().any(|a| a.patient_id == Some(pid) && a.date >= today && !a.state.is_cancelled() && a.state != ApptState::NoAcude)
    }

    /// Crea las tareas automáticas que falten. Devuelve cuántas se han creado.
    pub fn generate_tasks(&mut self, today: NaiveDate) -> usize {
        let mut new: Vec<(String, Option<u32>, String, NaiveDate)> = vec![];
        // 1) No acudió y no tiene otra cita
        for a in self.appointments.iter().filter(|a| a.state == ApptState::NoAcude && a.date < today + Duration::days(1) && a.date >= today - Duration::days(14)) {
            let Some(pid) = a.patient_id else { continue };
            if !self.has_future_appt(pid, today) {
                new.push((format!("noacude:{}", a.id), Some(pid), format!("Llamar para dar nueva cita: no acudió el {}", fmt_date(a.date)), today));
            }
        }
        // 2) Presupuestos sin respuesta tras 15 días
        for b in self.budgets.iter().filter(|b| b.state == BudgetState::Pendiente && b.date + Duration::days(15) <= today && b.expires() >= today) {
            new.push((format!("presu:{}", b.id), Some(b.patient_id), format!("Llamar para el presupuesto «{}» ({}) sin respuesta", b.title, money(b.total())), today));
        }
        // 3) Citas de mañana: cuestionario de salud y deuda pendiente
        let tomorrow = today + Duration::days(1);
        for a in self.appointments.iter().filter(|a| (a.date == today || a.date == tomorrow) && !a.state.is_cancelled()) {
            let Some(pid) = a.patient_id else { continue };
            if a.date == tomorrow
                && let Some(s) = self.health_status(pid, today)
            {
                new.push((format!("salud:{}", a.id), Some(pid), format!("{s}: pedirlo en la cita de mañana a las {}", a.start.format("%H:%M")), tomorrow));
            }
            let bal = self.patient_balance(pid);
            if bal > 0.005 {
                new.push((format!("deuda:{}", a.id), Some(pid), format!("Tiene {} pendiente: cobrar en la cita del {} a las {}", money(bal), fmt_date(a.date), a.start.format("%H:%M")), a.date));
            }
        }
        let mut n = 0;
        for (key, pid, text, due) in new {
            if self.tasks.iter().any(|t| t.auto == key) {
                continue;
            }
            let id = self.new_id();
            self.tasks.push(Task { id, created: today, by: "Automática".into(), to: String::new(), patient_id: pid, text, due: Some(due), done_on: None, done_by: String::new(), auto: key });
            n += 1;
        }
        n
    }
}

/// Tareas de ejemplo para el modo demostración.
pub fn sample_tasks(db: &mut Db, today: NaiveDate) {
    db.add_task(today, "Dirección", "Recepción", None, "Revisar el pedido de guantes con el proveedor", Some(today));
    db.add_task(today - Duration::days(2), "Dr. Navarro", "Recepción", Some(101), "Pedir a la aseguradora la autorización del implante 36", Some(today - Duration::days(1)));
    db.add_task(today, "Recepción", "Dra. Ruiz", Some(100), "Llamar a la paciente con el resultado de la radiografía", Some(today + Duration::days(1)));
    db.generate_tasks(today);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
    }

    #[test]
    fn automatic_tasks_are_created_once() {
        let mut db = Db::sample(d());
        let before = db.tasks.len();
        // la muestra ya las generó: repetir no crea duplicados
        assert_eq!(db.generate_tasks(d()), 0);
        assert_eq!(db.tasks.len(), before);
        // un paciente sin más citas que no acudió ayer genera un aviso; uno con otra cita no
        let pid = db.patients.iter().map(|p| p.id).find(|&p| !db.has_future_appt(p, d())).unwrap();
        let mut a = db.appointments[0].clone();
        a.id = db.new_id();
        a.patient_id = Some(pid);
        a.date = d() - Duration::days(1);
        a.state = ApptState::NoAcude;
        db.appointments.push(a.clone());
        assert_eq!(db.generate_tasks(d()), 1);
        assert!(db.tasks.iter().any(|t| t.auto == format!("noacude:{}", a.id) && t.patient_id == Some(pid)));
        assert!(!db.tasks.iter().any(|t| t.auto.starts_with("noacude:") && t.patient_id == Some(104)), "la paciente 104 ya tiene otra cita hoy");
        // completar una automática no hace que vuelva a aparecer
        let id = db.tasks.iter().find(|t| !t.auto.is_empty()).unwrap().id;
        db.complete_task(id, d(), "Recepción", true);
        assert_eq!(db.generate_tasks(d()), 0);
        assert!(!db.tasks.iter().find(|t| t.id == id).unwrap().pending());
    }

    #[test]
    fn a_recall_follows_its_appointment() {
        let mut db = Db::sample(d());
        let pid = db.patients.iter().map(|p| p.id).find(|&p| !db.has_future_appt(p, d())).unwrap();
        db.recalls.retain(|r| r.patient_id != pid);
        let rid = db.new_id();
        db.recalls.push(Recall {
            id: rid,
            patient_id: pid,
            date: d() - Duration::days(10),
            reason: "Higiene anual".into(),
            doctor_id: 4,
            state: RecallState::Pendiente,
            channel: String::new(),
            comment: String::new(),
            contacted_on: None,
            appt_id: None,
        });
        let r = |db: &Db| db.recalls.iter().find(|r| r.id == rid).unwrap().clone();
        let mut a = db.appointments[0].clone();
        a.id = db.new_id();
        a.patient_id = Some(pid);
        a.date = d() + Duration::days(3);
        a.state = ApptState::Planificada;
        db.appointments.push(a.clone());
        db.refresh_recalls(d());
        assert_eq!((r(&db).state, r(&db).appt_id), (RecallState::Citado, Some(a.id)));
        // anula la cita por internet: vuelve a pendiente para volver a llamarle
        assert!(db.portal_cancel(a.id, ""));
        assert_eq!((r(&db).state, r(&db).appt_id), (RecallState::Pendiente, None));
        assert!(r(&db).comment.contains("Anuló"), "{}", r(&db).comment);
        // con otra cita y luego no acude: igual
        let mut b = a.clone();
        b.id = db.new_id();
        b.date = d() + Duration::days(5);
        db.appointments.push(b.clone());
        db.refresh_recalls(d());
        assert_eq!(r(&db).appt_id, Some(b.id));
        let mut c = b.clone();
        c.id = db.new_id();
        c.date = d() + Duration::days(9);
        db.appointments.push(c.clone());
        // si no acude a una pero tiene otra, sigue citado con la otra
        db.appointments.iter_mut().find(|x| x.id == b.id).unwrap().state = ApptState::NoAcude;
        db.refresh_recalls(d());
        assert_eq!((r(&db).state, r(&db).appt_id), (RecallState::Citado, Some(c.id)));
        db.appointments.retain(|x| x.id != c.id);
        db.refresh_recalls(d());
        assert_eq!(r(&db).state, RecallState::Pendiente, "sin cita, otra vez a la lista");
        // una cita a la que sí fue deja el recall citado aunque pase el día
        let mut e = a.clone();
        e.id = db.new_id();
        e.date = d() + Duration::days(1);
        db.appointments.push(e.clone());
        db.refresh_recalls(d());
        db.appointments.iter_mut().find(|x| x.id == e.id).unwrap().state = ApptState::Finalizada;
        db.refresh_recalls(d() + Duration::days(10));
        assert_eq!(r(&db).state, RecallState::Citado);
    }

    #[test]
    fn tasks_are_filtered_by_user_and_sorted() {
        let db = Db::sample(d());
        let rec = db.pending_tasks("Recepción");
        let ruiz = db.pending_tasks("Dra. Ruiz");
        assert!(rec.iter().all(|t| t.to.is_empty() || t.to == "Recepción"));
        assert!(ruiz.iter().any(|t| t.to == "Dra. Ruiz"));
        assert!(!rec.iter().any(|t| t.to == "Dra. Ruiz"));
        let dues: Vec<NaiveDate> = rec.iter().map(|t| t.due.unwrap_or(NaiveDate::MAX)).collect();
        assert!(dues.windows(2).all(|w| w[0] <= w[1]));
        assert!(rec.iter().any(|t| t.overdue(d())));
    }
}
