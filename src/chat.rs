//! Avisos instantáneos entre puestos y usuarios (recepción ↔ gabinetes).
//! Viajan con los datos de la clínica: con servidor llegan a los demás
//! puestos en pocos segundos; en un solo ordenador, al cambiar de usuario.

use crate::model::*;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InternalMsg {
    pub id: u32,
    pub when: NaiveDateTime,
    pub from: String,
    /// Usuario destinatario (vacío = todos).
    pub to: String,
    pub text: String,
    pub patient_id: Option<u32>,
    /// Usuarios que ya lo han visto.
    pub read_by: Vec<String>,
}

impl InternalMsg {
    pub fn for_user(&self, user: &str) -> bool {
        self.from != user && (self.to.is_empty() || self.to == user)
    }
}

/// Frases rápidas de uso diario.
pub const QUICK: [&str; 6] = ["Paciente en sala de espera", "Pase el siguiente paciente", "Necesito ayuda en el gabinete", "Tiene una llamada", "El paciente está pagando en recepción", "Prepare el gabinete, por favor"];

const KEEP: usize = 500;

impl Db {
    pub fn send_msg(&mut self, from: &str, to: &str, text: &str, patient_id: Option<u32>) -> u32 {
        let id = self.new_id();
        self.chat.push(InternalMsg { id, when: now(), from: from.into(), to: to.into(), text: text.trim().into(), patient_id, read_by: vec![from.into()] });
        if self.chat.len() > KEEP {
            let extra = self.chat.len() - KEEP;
            self.chat.drain(0..extra);
        }
        id
    }

    pub fn unread_msgs(&self, user: &str) -> Vec<&InternalMsg> {
        self.chat.iter().filter(|m| m.for_user(user) && !m.read_by.iter().any(|u| u == user)).collect()
    }

    pub fn mark_read(&mut self, id: u32, user: &str) {
        if let Some(m) = self.chat.iter_mut().find(|m| m.id == id)
            && !m.read_by.iter().any(|u| u == user)
        {
            m.read_by.push(user.into());
        }
    }

    /// Aviso automático al doctor cuando su paciente llega a la sala de espera.
    pub fn arrival_notice(&mut self, appt_id: u32, from: &str) {
        let Some(a) = self.appointments.iter().find(|a| a.id == appt_id).cloned() else { return };
        let who = self.appt_name(&a);
        let doctors: Vec<String> = self.users.iter().filter(|u| u.active && u.doctor_id == Some(a.doctor_id)).map(|u| u.name.clone()).collect();
        let text = format!("{who} ha llegado (cita de las {}): en sala de espera", a.start.format("%H:%M"));
        if doctors.is_empty() {
            return;
        }
        for d in doctors {
            if !self.chat.iter().any(|m| m.to == d && m.text == text) {
                self.send_msg(from, &d, &text, a.patient_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_reach_the_right_people() {
        let mut db = Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 6).unwrap());
        db.chat.clear();
        db.send_msg("Recepción", "Dra. Ruiz", "Pase el siguiente paciente", None);
        db.send_msg("Dra. Ruiz", "", "Necesito ayuda en el gabinete", None);
        assert_eq!(db.unread_msgs("Dra. Ruiz").len(), 1);
        assert_eq!(db.unread_msgs("Recepción").len(), 1, "recibe el mensaje a todos, no el suyo");
        assert_eq!(db.unread_msgs("Dr. Navarro").len(), 1);
        let id = db.unread_msgs("Dra. Ruiz")[0].id;
        db.mark_read(id, "Dra. Ruiz");
        assert!(db.unread_msgs("Dra. Ruiz").is_empty());
    }

    #[test]
    fn arrival_notifies_the_doctor_once() {
        let mut db = Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 6).unwrap());
        db.chat.clear();
        let doc_user = db.users.iter().find(|u| u.doctor_id.is_some()).unwrap().clone();
        let a = db.appointments.iter().find(|a| a.doctor_id == doc_user.doctor_id.unwrap() && a.patient_id.is_some()).unwrap().id;
        db.arrival_notice(a, "Recepción");
        db.arrival_notice(a, "Recepción");
        let got = db.unread_msgs(&doc_user.name);
        assert_eq!(got.len(), 1);
        assert!(got[0].text.contains("sala de espera"));
    }
}
