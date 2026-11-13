//! Huecos libres para la lista de espera: cuando una cita se anula, el hueco
//! se ofrece a los pacientes que esperan con ese profesional (los que llevan
//! más tiempo y cuya preferencia de horario encaja). Cada uno recibe un enlace
//! personal; el primero que dice «lo quiero» se queda la cita y sale de la
//! lista de espera, y a los demás la página les dice que ya está ocupado.
//!
//! Sin la página publicada en internet el aviso pide que llamen a la clínica,
//! y la oferta se da por cerrada en cuanto recepción ocupa el hueco.

use crate::model::*;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OfferState {
    #[default]
    Abierta,
    /// Alguien se lo quedó (por el enlace o porque recepción ocupó el hueco).
    Ocupada,
    /// Pasó la hora límite sin que nadie lo cogiera.
    Caducada,
    /// La clínica la retiró.
    Retirada,
}

impl OfferState {
    pub fn label(self) -> &'static str {
        match self {
            OfferState::Abierta => "Ofrecido",
            OfferState::Ocupada => "Ocupado",
            OfferState::Caducada => "Caducado",
            OfferState::Retirada => "Retirado",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OfferInvite {
    pub token: String,
    pub patient_id: u32,
    /// Entrada de la lista de espera.
    pub wait_id: u32,
    /// Canal por el que se le avisó (Correo, WhatsApp, SMS) o vacío si no se le pudo avisar.
    pub channel: String,
    pub message_id: Option<u32>,
    pub viewed: Option<NaiveDateTime>,
    pub declined: Option<NaiveDateTime>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SlotOffer {
    pub id: u32,
    pub doctor_id: u32,
    pub date: Option<NaiveDate>,
    pub start: Option<NaiveTime>,
    pub minutes: u32,
    pub center_id: u32,
    pub created: Option<NaiveDateTime>,
    pub state: OfferState,
    pub invites: Vec<OfferInvite>,
    pub taken_by: Option<u32>,
    pub taken_at: Option<NaiveDateTime>,
    pub appointment_id: Option<u32>,
    /// Cita anulada que dejó el hueco.
    pub source_appt: Option<u32>,
}

impl SlotOffer {
    pub fn end(&self) -> Option<NaiveTime> {
        Some(self.start? + Duration::minutes(self.minutes as i64))
    }
}

/// ¿Le encaja el horario según su preferencia («mañanas», «tardes»…)?
pub fn fits_preference(pref: &str, start: NaiveTime) -> bool {
    let p = fold(pref);
    let afternoon = start.hour() >= 14;
    if p.contains("manana") && !p.contains("tarde") {
        return !afternoon;
    }
    if p.contains("tarde") && !p.contains("manana") {
        return afternoon;
    }
    true
}

pub fn link_url(base: &str, token: &str) -> String {
    format!("{}/hueco/{token}", base.trim().trim_end_matches('/'))
}

/// Un hueco de la agenda: de qué profesional, qué día, a qué hora, cuántos
/// minutos y en qué centro.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreeSlot {
    pub doctor: u32,
    pub date: NaiveDate,
    pub start: NaiveTime,
    pub minutes: u32,
    pub center: u32,
}

impl Db {
    /// Pacientes de la lista de espera a los que ofrecer el hueco, los que más llevan esperando primero.
    pub fn offer_candidates(&self, doctor: u32, date: NaiveDate, start: NaiveTime, minutes: u32, exclude: Option<u32>) -> Vec<WaitItem> {
        let end = start + Duration::minutes(minutes as i64);
        let mut v: Vec<WaitItem> = self
            .waitlist
            .iter()
            .filter(|w| w.doctor_id == doctor && Some(w.patient_id) != exclude && fits_preference(&w.preference, start))
            .filter(|w| self.patient(w.patient_id).is_some_and(|p| p.active))
            // que no tenga ya otra cita a esa hora
            .filter(|w| !self.appointments.iter().any(|a| a.patient_id == Some(w.patient_id) && a.date == date && !a.state.is_cancelled() && a.start < end && start < a.end()))
            .cloned()
            .collect();
        v.sort_by_key(|w| (w.since, w.id));
        v.truncate(self.booking.offer_max.clamp(1, 30) as usize);
        v
    }

    /// Crea la oferta de un hueco libre. No envía nada: eso lo hace el programa con `offer_messages`.
    pub fn create_offer(&mut self, slot: FreeSlot, source: Option<u32>, now: NaiveDateTime) -> Result<u32, String> {
        let FreeSlot { doctor, date, start, minutes, center } = slot;
        let end = start + Duration::minutes(minutes as i64);
        if date.and_time(start) < now + Duration::hours(self.booking.offer_min_hours as i64) {
            return Err("Ese hueco es demasiado pronto para avisar a la lista de espera.".into());
        }
        if !self.is_free(doctor, date, start, end, None) {
            return Err("Ese hueco ya no está libre.".into());
        }
        if self.slot_offers.iter().any(|o| o.state == OfferState::Abierta && o.doctor_id == doctor && o.date == Some(date) && o.start.is_some_and(|s| s < end && start < s + Duration::minutes(o.minutes as i64))) {
            return Err("Ese hueco ya se está ofreciendo.".into());
        }
        let source_pid = source.and_then(|id| self.appointments.iter().find(|a| a.id == id)).and_then(|a| a.patient_id);
        let cands = self.offer_candidates(doctor, date, start, minutes, source_pid);
        if cands.is_empty() {
            return Err("No hay nadie en la lista de espera de ese profesional a quien le encaje esa hora.".into());
        }
        let id = self.new_id();
        let invites = cands.iter().map(|w| OfferInvite { token: crate::portal::new_token(), patient_id: w.patient_id, wait_id: w.id, ..Default::default() }).collect();
        self.slot_offers.push(SlotOffer {
            id,
            doctor_id: doctor,
            date: Some(date),
            start: Some(start),
            minutes,
            center_id: cid(center),
            created: Some(now),
            state: OfferState::Abierta,
            invites,
            taken_by: None,
            taken_at: None,
            appointment_id: None,
            source_appt: source,
        });
        Ok(id)
    }

    /// Prepara el aviso de cada invitado (correo si lo acepta; si no, WhatsApp o SMS) y devuelve los mensajes creados.
    pub fn offer_messages(&mut self, offer_id: u32) -> Vec<u32> {
        use crate::comms::{Channel, Message, MsgKind, MsgState};
        let Some(o) = self.slot_offers.iter().find(|o| o.id == offer_id).cloned() else { return vec![] };
        let (Some(date), Some(start)) = (o.date, o.start) else { return vec![] };
        let base = self.booking.public_url.trim().to_string();
        let web = self.booking.enabled && base.starts_with("http");
        let doctor = self.doctor(o.doctor_id).map(|d| d.alias.clone()).unwrap_or_default();
        let (clinic, phone) = (self.config.name.clone(), self.config.phone.clone());
        let mut out = vec![];
        for (i, inv) in o.invites.iter().enumerate() {
            let Some(p) = self.patient(inv.patient_id).cloned() else { continue };
            let (ch, to) = if self.can_email(p.id) {
                (Channel::Email, p.email.clone())
            } else if p.accepts_gdpr && p.accepts_whatsapp && !p.mobile.trim().is_empty() {
                (Channel::WhatsApp, p.mobile.clone())
            } else if p.accepts_gdpr && p.accepts_sms && !p.mobile.trim().is_empty() {
                (Channel::Sms, p.mobile.clone())
            } else {
                continue;
            };
            let when = format!("el {} {} a las {}", weekday_es(date), date.format("%d/%m"), start.format("%H:%M"));
            let action =
                if web { format!("Si lo quiere, resérvelo aquí (se lo queda el primero que lo pida):\n{}", link_url(&base, &inv.token)) } else { format!("Si lo quiere, llámenos cuanto antes al {phone}: se lo daremos al primero que llame.") };
            let text = format!("Hola, {}: se ha quedado libre un hueco {when} con {doctor} en {clinic}, y está usted en nuestra lista de espera.\n\n{action}\n\nUn saludo,\n{clinic}", p.first_name);
            let id = self.new_id();
            self.messages.push(Message {
                id,
                patient_id: p.id,
                kind: MsgKind::HuecoLibre,
                channel: ch,
                to,
                subject: format!("Hueco libre {when} en {clinic}"),
                text,
                created: now(),
                sent_at: None,
                state: MsgState::Pendiente,
                related: Some(offer_id),
                user: self.config.current_user.clone(),
                error: String::new(),
                attempts: 0,
            });
            if let Some(x) = self.slot_offers.iter_mut().find(|x| x.id == offer_id) {
                x.invites[i].channel = ch.label().to_string();
                x.invites[i].message_id = Some(id);
            }
            out.push(id);
        }
        out
    }

    /// El paciente del enlace se queda el hueco: cita confirmada y fuera de la lista de espera.
    pub fn take_offer(&mut self, token: &str, now: NaiveDateTime) -> Result<u32, String> {
        let o = self.slot_offers.iter().find(|o| o.invites.iter().any(|i| i.token == token)).cloned().ok_or("Ese enlace ya no es válido.")?;
        let inv = o.invites.iter().find(|i| i.token == token).cloned().ok_or("Ese enlace ya no es válido.")?;
        if o.state != OfferState::Abierta {
            return Err("Lo sentimos, ese hueco ya no está disponible.".into());
        }
        let (Some(date), Some(start)) = (o.date, o.start) else { return Err("Oferta sin fecha.".into()) };
        if date.and_time(start) <= now {
            return Err("Ese hueco ya ha pasado.".into());
        }
        let end = start + Duration::minutes(o.minutes as i64);
        if !self.is_free(o.doctor_id, date, start, end, None) {
            if let Some(x) = self.slot_offers.iter_mut().find(|x| x.id == o.id) {
                x.state = OfferState::Ocupada;
            }
            return Err("Lo sentimos, ese hueco se acaba de ocupar.".into());
        }
        let reason = self.waitlist.iter().find(|w| w.id == inv.wait_id).map(|w| w.reason.clone()).unwrap_or_else(|| "Cita (lista de espera)".into());
        let center = cid(o.center_id);
        let boxes: Vec<u32> = self.boxes.iter().filter(|b| cid(b.center_id) == center).map(|b| b.id).collect();
        let box_id = boxes.iter().copied().find(|b| !self.appointments.iter().any(|a| a.box_id == *b && a.date == date && !a.state.is_cancelled() && a.start < end && start < a.end())).or(boxes.first().copied()).unwrap_or(1);
        let phone = self.patient(inv.patient_id).map(|p| if p.mobile.trim().is_empty() { p.phone.clone() } else { p.mobile.clone() }).unwrap_or_default();
        let appt = self.new_id();
        self.appointments.push(Appointment {
            id: appt,
            date,
            start,
            minutes: o.minutes,
            patient_id: Some(inv.patient_id),
            free_name: String::new(),
            phone,
            doctor_id: o.doctor_id,
            box_id,
            reason,
            treatment_code: String::new(),
            state: ApptState::Confirmada,
            notes: format!("Hueco libre ofrecido a la lista de espera; lo reservó el paciente el {}.", now.format("%d/%m/%Y %H:%M")),
            arrived_at: None,
            seen_at: None,
            finished_at: None,
            reminder_sent: false,
            first_visit: false,
            center_id: center,
        });
        self.waitlist.retain(|w| w.id != inv.wait_id);
        self.refresh_recalls(now.date());
        if let Some(x) = self.slot_offers.iter_mut().find(|x| x.id == o.id) {
            x.state = OfferState::Ocupada;
            x.taken_by = Some(inv.patient_id);
            x.taken_at = Some(now);
            x.appointment_id = Some(appt);
        }
        // los avisos que aún no salieron ya no hacen falta
        let pending: Vec<u32> = o.invites.iter().filter(|i| i.patient_id != inv.patient_id).filter_map(|i| i.message_id).collect();
        for m in self.messages.iter_mut().filter(|m| pending.contains(&m.id) && m.state == crate::comms::MsgState::Pendiente) {
            m.state = crate::comms::MsgState::Descartado;
        }
        Ok(appt)
    }

    pub fn decline_offer(&mut self, token: &str) -> bool {
        for o in self.slot_offers.iter_mut() {
            if let Some(i) = o.invites.iter_mut().find(|i| i.token == token) {
                if i.declined.is_none() {
                    i.declined = Some(now());
                    return true;
                }
                return false;
            }
        }
        false
    }

    pub fn offer_viewed(&mut self, token: &str) -> bool {
        for o in self.slot_offers.iter_mut() {
            if let Some(i) = o.invites.iter_mut().find(|i| i.token == token) {
                if i.viewed.is_none() {
                    i.viewed = Some(now());
                    return true;
                }
                return false;
            }
        }
        false
    }

    pub fn withdraw_offer(&mut self, id: u32) {
        if let Some(o) = self.slot_offers.iter_mut().find(|o| o.id == id && o.state == OfferState::Abierta) {
            o.state = OfferState::Retirada;
        }
    }

    /// Cierra las ofertas cuyo hueco ya pasó (o está a punto) o que recepción ya ha ocupado. Devuelve si cambió algo.
    pub fn refresh_offers(&mut self, now: NaiveDateTime) -> bool {
        let mut changed = false;
        let open: Vec<SlotOffer> = self.slot_offers.iter().filter(|o| o.state == OfferState::Abierta).cloned().collect();
        for o in open {
            let (Some(date), Some(start), Some(end)) = (o.date, o.start, o.end()) else { continue };
            let new = if date.and_time(start) <= now + Duration::minutes(30) {
                Some(OfferState::Caducada)
            } else if !self.is_free(o.doctor_id, date, start, end, None) {
                Some(OfferState::Ocupada)
            } else {
                None
            };
            if let Some(s) = new
                && let Some(x) = self.slot_offers.iter_mut().find(|x| x.id == o.id)
            {
                x.state = s;
                changed = true;
            }
        }
        changed
    }

    /// Citas anuladas de aquí en adelante cuyo hueco sigue libre y aún no se ha ofrecido: (cita, candidatos).
    pub fn freed_slots(&self, now: NaiveDateTime) -> Vec<(Appointment, usize)> {
        let mut v: Vec<(Appointment, usize)> = self
            .appointments
            .iter()
            .filter(|a| a.state == ApptState::Anulada && a.date.and_time(a.start) > now + Duration::hours(self.booking.offer_min_hours as i64))
            .filter(|a| !self.slot_offers.iter().any(|o| o.source_appt == Some(a.id) || (o.doctor_id == a.doctor_id && o.date == Some(a.date) && o.start == Some(a.start))))
            .filter(|a| self.is_free(a.doctor_id, a.date, a.start, a.end(), None))
            .map(|a| (a.clone(), self.offer_candidates(a.doctor_id, a.date, a.start, a.minutes, a.patient_id).len()))
            .filter(|(_, n)| *n > 0)
            .collect();
        v.sort_by_key(|(a, _)| (a.date, a.start));
        v
    }
}

// ───────────────────────────── lo que ve la página ─────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct PubOffer {
    pub offer_id: u32,
    pub first_name: String,
    pub date: Option<NaiveDate>,
    pub start: Option<NaiveTime>,
    pub minutes: u32,
    pub doctor: String,
    /// abierta, ocupada, suya, cerrada
    pub state: &'static str,
    pub viewed: bool,
}

/// Una entrada por enlace de invitado (las ofertas de los últimos días).
pub fn publish_offers(db: &Db, today: NaiveDate) -> HashMap<String, PubOffer> {
    let mut m = HashMap::new();
    for o in db.slot_offers.iter().filter(|o| o.date.is_some_and(|d| d + Duration::days(1) >= today)) {
        for i in &o.invites {
            let state = match o.state {
                OfferState::Abierta => "abierta",
                OfferState::Ocupada if o.taken_by == Some(i.patient_id) => "suya",
                OfferState::Ocupada => "ocupada",
                _ => "cerrada",
            };
            m.insert(
                i.token.clone(),
                PubOffer {
                    offer_id: o.id,
                    first_name: db.patient(i.patient_id).map(|p| p.first_name.clone()).unwrap_or_default(),
                    date: o.date,
                    start: o.start,
                    minutes: o.minutes,
                    doctor: db.doctor(o.doctor_id).map(|d| d.alias.clone()).unwrap_or_default(),
                    state,
                    viewed: i.viewed.is_some(),
                },
            );
        }
    }
    m
}

pub fn offer_json(p: &crate::booking::Published, o: &PubOffer) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "clinica": p.clinic, "direccion": p.address, "telefono": p.phone,
        "nombre": o.first_name,
        "etiqueta": o.date.map(crate::booking::day_label).unwrap_or_default(),
        "hora": o.start.map(|t| t.format("%H:%M").to_string()).unwrap_or_default(),
        "minutos": o.minutes, "profesional": o.doctor, "estado": o.state,
    })
}

/// Lo que el paciente hace con el enlace del hueco.
pub enum OfferAction {
    Viewed,
    Take,
    Decline,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn when(d: NaiveDate, h: u32) -> (NaiveDate, NaiveTime) {
        (d, NaiveTime::from_hms_opt(h, 0, 0).unwrap())
    }

    /// El primer día laborable (ni fin de semana ni festivo) desde `from`.
    fn workday(db: &Db, mut from: NaiveDate) -> NaiveDate {
        use chrono::Datelike;
        while from.weekday().num_days_from_monday() >= 5 || db.holidays.iter().any(|h| h.date == from) {
            from += Duration::days(1);
        }
        from
    }

    /// Un día laborable dentro de unos días sin citas de la higienista (doctor 4); en la lista de
    /// espera del ejemplo está Javier (106, «Mañanas») con ella.
    fn setup() -> (Db, NaiveDateTime, NaiveDate) {
        let mut db = Db::sample(today());
        let now = today().and_hms_opt(9, 0, 0).unwrap();
        let day = workday(&db, today() + Duration::days(5));
        db.appointments.retain(|a| !(a.date == day && a.doctor_id == 4));
        db.slot_offers.clear();
        (db, now, day)
    }

    #[test]
    fn preferences_are_respected() {
        let t = |h| NaiveTime::from_hms_opt(h, 0, 0).unwrap();
        assert!(fits_preference("Mañanas", t(10)) && !fits_preference("Mañanas", t(17)));
        assert!(fits_preference("Tardes", t(17)) && !fits_preference("tardes", t(9)));
        assert!(fits_preference("Cualquier hora", t(9)) && fits_preference("mañana o tarde", t(18)));
    }

    #[test]
    fn first_to_answer_gets_the_slot() {
        let (mut db, now, day) = setup();
        // otro paciente en espera con la higienista, más reciente
        let wid = db.new_id();
        db.waitlist.push(WaitItem { id: wid, patient_id: 102, doctor_id: 4, reason: "Limpieza".into(), since: today(), preference: "Cualquier hora".into() });
        let (d, s) = when(day, 10);
        // por la tarde no le encaja a Javier («Mañanas»)
        assert_eq!(db.offer_candidates(4, d, NaiveTime::from_hms_opt(17, 0, 0).unwrap(), 45, None).iter().map(|w| w.patient_id).collect::<Vec<_>>(), vec![102]);
        let id = db.create_offer(FreeSlot { doctor: 4, date: d, start: s, minutes: 45, center: 1 }, None, now).unwrap();
        assert!(db.create_offer(FreeSlot { doctor: 4, date: d, start: s, minutes: 45, center: 1 }, None, now).is_err(), "no se ofrece dos veces");
        let o = db.slot_offers.iter().find(|o| o.id == id).unwrap().clone();
        assert_eq!(o.invites.iter().map(|i| i.patient_id).collect::<Vec<_>>(), vec![106, 102], "primero quien más lleva esperando");
        let msgs = db.offer_messages(id);
        assert!(!msgs.is_empty());
        let first = o.invites[1].token.clone();
        let second = o.invites[0].token.clone();
        // tenía pendiente su higiene y ninguna otra cita
        db.appointments.retain(|a| a.patient_id != Some(102) || a.date < today());
        let rid = db.new_id();
        db.recalls.push(crate::model::Recall {
            id: rid,
            patient_id: 102,
            date: d,
            reason: "Higiene anual".into(),
            doctor_id: 4,
            state: crate::model::RecallState::Pendiente,
            channel: String::new(),
            comment: String::new(),
            contacted_on: None,
            appt_id: None,
        });
        let appt = db.take_offer(&first, now).unwrap();
        assert_eq!(db.recalls.iter().find(|r| r.id == rid).unwrap().appt_id, Some(appt), "su recall queda citado");
        let a = db.appointments.iter().find(|a| a.id == appt).unwrap();
        assert_eq!((a.patient_id, a.date, a.start, a.state), (Some(102), d, s, ApptState::Confirmada));
        assert!(!db.waitlist.iter().any(|w| w.id == wid), "sale de la lista de espera");
        assert!(db.take_offer(&second, now).is_err(), "el segundo llega tarde");
        let o = db.slot_offers.iter().find(|o| o.id == id).unwrap();
        assert_eq!((o.state, o.taken_by), (OfferState::Ocupada, Some(102)));
        // el aviso que no había salido se descarta
        if let Some(mid) = o.invites[0].message_id {
            assert_ne!(db.messages.iter().find(|m| m.id == mid).unwrap().state, crate::comms::MsgState::Pendiente);
        }
    }

    #[test]
    fn offers_close_when_reception_fills_the_slot_or_time_passes() {
        let (mut db, now, day) = setup();
        let (d, s) = when(day, 9);
        let id = db.create_offer(FreeSlot { doctor: 4, date: d, start: s, minutes: 45, center: 1 }, None, now).unwrap();
        assert!(!db.refresh_offers(now));
        // recepción da ese hueco por teléfono
        let aid = db.new_id();
        db.appointments.push(Appointment {
            id: aid,
            date: d,
            start: s,
            minutes: 45,
            patient_id: Some(110),
            free_name: String::new(),
            phone: String::new(),
            doctor_id: 4,
            box_id: 4,
            reason: "x".into(),
            treatment_code: String::new(),
            state: ApptState::Planificada,
            notes: String::new(),
            arrived_at: None,
            seen_at: None,
            finished_at: None,
            reminder_sent: false,
            first_visit: false,
            center_id: 1,
        });
        assert!(db.refresh_offers(now));
        assert_eq!(db.slot_offers.iter().find(|o| o.id == id).unwrap().state, OfferState::Ocupada);
        // y una oferta cuyo hueco ya llega caduca
        let (d2, s2) = when(workday(&db, day + Duration::days(7)), 11);
        db.appointments.retain(|a| !(a.date == d2 && a.doctor_id == 4));
        let id2 = db.create_offer(FreeSlot { doctor: 4, date: d2, start: s2, minutes: 45, center: 1 }, None, now).unwrap();
        db.refresh_offers(d2.and_time(s2) - Duration::minutes(10));
        assert_eq!(db.slot_offers.iter().find(|o| o.id == id2).unwrap().state, OfferState::Caducada);
        // demasiado pronto o sin nadie en espera: no se ofrece
        assert!(db.create_offer(FreeSlot { doctor: 4, date: today(), start: NaiveTime::from_hms_opt(9, 30, 0).unwrap(), minutes: 45, center: 1 }, None, now).is_err());
        assert!(db.create_offer(FreeSlot { doctor: 3, date: d, start: s, minutes: 30, center: 1 }, None, now).is_err(), "nadie espera a la ortodoncista");
    }
}
