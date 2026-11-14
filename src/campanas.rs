//! Campañas a grupos de pacientes (T-F5-12). Sin envíos reales durante el
//! desarrollo: los mensajes van a la cola de siempre (Comunicaciones).
//!
//! - **Segmento:** edad, última visita, presupuesto pendiente y tratamiento
//!   hecho o no.
//! - **Consentimiento por finalidad:** solo quien ha dado su consentimiento
//!   expreso para promociones ([`Patient::marketing`], con fecha), además del
//!   de protección de datos y el del canal. Quien se da de baja de las
//!   campañas no las recibe. Las dos cosas se vuelven a mirar al pasar cada
//!   mensaje a la cola: una baja de última hora manda.
//! - **Límites:**
//!   - ninguna campaña al mismo paciente en los [`Campaign::min_days`]
//!     anteriores;
//!   - solo de [`Campaign::from_hour`] a [`Campaign::to_hour`], de lunes a
//!     viernes;
//!   - como mucho [`Campaign::per_day`] mensajes al día.
//! - **Previsualización:** cuántos la recibirán, cuántos se quedan fuera y
//!   por qué, y los primeros mensajes tal como saldrán. Al programarla, la
//!   lista queda fija.
//! - **Quién:** la programa o la cancela Dirección (también en el servidor,
//!   [`tick_only`]); cualquier puesto pasa a la cola los mensajes que tocan.
//! - **Resultados atribuibles, con un criterio explícito ([`CRITERION`]):**
//!   una cita se atribuye a la campaña si es del paciente, no existía cuando
//!   se le pasó el mensaje a la cola, no está anulada y su fecha está en los
//!   [`Campaign::attribution_days`] siguientes. Se dice también si acudió. Es
//!   una asociación, no una prueba de que la cita se deba a la campaña.

use crate::comms::{Channel, Message, MsgKind, MsgState};
use crate::model::*;
use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const CRITERION: &str =
    "Se cuenta una cita si es del paciente, no existía cuando se le pasó el mensaje a la cola, no está anulada y su fecha está dentro del plazo de atribución después del mensaje. Es una asociación, no una prueba de que la cita se deba a la campaña.";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    #[serde(default)]
    pub min_age: Option<u32>,
    #[serde(default)]
    pub max_age: Option<u32>,
    /// Última visita (cita finalizada) hace al menos estos días.
    #[serde(default)]
    pub last_visit_days: Option<u32>,
    #[serde(default)]
    pub pending_budget: bool,
    /// Con un tratamiento hecho de este código (vacío: da igual).
    #[serde(default)]
    pub with_code: String,
    /// Sin un tratamiento hecho de este código en el último año.
    #[serde(default)]
    pub without_code: String,
}

impl Segment {
    /// Usa los tratamientos (datos de salud): solo lo ajusta un perfil clínico.
    pub fn uses_treatments(&self) -> bool {
        !self.with_code.trim().is_empty() || !self.without_code.trim().is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CampState {
    #[default]
    Borrador,
    Programada,
    Terminada,
    Cancelada,
}

impl CampState {
    pub fn label(self) -> &'static str {
        match self {
            CampState::Borrador => "Borrador",
            CampState::Programada => "Programada",
            CampState::Terminada => "Terminada",
            CampState::Cancelada => "Cancelada",
        }
    }
}

/// Un destinatario: cuándo se le pasó el mensaje a la cola (o se vio que ya
/// no podía recibirlo) y qué citas tenía entonces.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recipient {
    pub patient_id: u32,
    #[serde(default)]
    pub sent: Option<NaiveDateTime>,
    /// Ninguno con `sent`: ya no podía recibirla (baja, consentimiento
    /// retirado…) o se canceló la campaña antes de enviarlo.
    #[serde(default)]
    pub message_id: Option<u32>,
    #[serde(default)]
    pub known_appts: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Campaign {
    pub id: u32,
    pub name: String,
    pub segment: Segment,
    pub channel: Channel,
    /// Asunto del correo (vacío: el nombre de la clínica).
    #[serde(default)]
    pub subject: String,
    pub text: String,
    #[serde(default)]
    pub state: CampState,
    pub min_days: u32,
    pub from_hour: u32,
    pub to_hour: u32,
    pub per_day: u32,
    pub attribution_days: u32,
    #[serde(default)]
    pub recipients: Vec<Recipient>,
    /// Quién la programó.
    #[serde(default)]
    pub created_by: String,
}

impl Campaign {
    pub fn new(id: u32, name: &str, channel: Channel, text: &str) -> Campaign {
        Campaign {
            id,
            name: name.into(),
            segment: Segment::default(),
            channel,
            subject: String::new(),
            text: text.into(),
            state: CampState::Borrador,
            min_days: 60,
            from_hour: 10,
            to_hour: 19,
            per_day: 100,
            attribution_days: 30,
            recipients: vec![],
            created_by: String::new(),
        }
    }

    /// Lo que impide programarla así.
    pub fn problem(&self) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some("Póngale un nombre.".into());
        }
        if self.text.trim().is_empty() {
            return Some("Escriba el mensaje.".into());
        }
        let mut bad = crate::comms::unknown_placeholders(&self.text, MsgKind::Campana);
        bad.extend(crate::comms::unknown_placeholders(&self.subject, MsgKind::Campana));
        if !bad.is_empty() {
            return Some(format!("Comodines que no se pueden usar en una campaña: {}.", bad.join(", ")));
        }
        if self.from_hour < 8 || self.to_hour > 22 || self.from_hour >= self.to_hour {
            return Some("El horario de envío tiene que estar entre las 8 y las 22.".into());
        }
        if self.per_day == 0 {
            return Some("Ponga cuántos mensajes como mucho al día.".into());
        }
        if !(1..=365).contains(&self.attribution_days) {
            return Some("El plazo de atribución va de 1 a 365 días.".into());
        }
        if self.segment.min_age.zip(self.segment.max_age).is_some_and(|(a, b)| a > b) {
            return Some("La edad mínima es mayor que la máxima.".into());
        }
        None
    }
}

/// Por qué un paciente del segmento se queda fuera.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Excluded {
    SinConsentimiento,
    DeBaja,
    SinContacto,
    CampanaReciente,
}

impl Excluded {
    pub fn label(self) -> &'static str {
        match self {
            Excluded::SinConsentimiento => "sin consentimiento para promociones",
            Excluded::DeBaja => "de baja de las campañas",
            Excluded::SinContacto => "sin el canal aceptado o sin teléfono o correo",
            Excluded::CampanaReciente => "con otra campaña reciente",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preview {
    pub recipients: Vec<u32>,
    pub excluded: Vec<(Excluded, usize)>,
    /// Los primeros mensajes tal como saldrán: (paciente, texto).
    pub samples: Vec<(u32, String)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Results {
    pub queued: usize,
    pub pending: usize,
    /// Ya no podían recibirla cuando les tocaba (baja, consentimiento…), o
    /// se canceló antes de enviarlo.
    pub skipped: usize,
    /// Citas atribuidas ([`CRITERION`]) y a cuántas acudió el paciente.
    pub appts: usize,
    pub attended: usize,
}

/// Lo que hace falta para decidir muchos pacientes de una vez (una pasada
/// por cada lista, no una por paciente).
struct Facts {
    last_visit: HashMap<u32, NaiveDate>,
    pending_budget: HashSet<u32>,
    /// Pacientes con el código hecho (alguna vez) y con él hecho en el último año.
    with_code: HashSet<u32>,
    without_code_recent: HashSet<u32>,
    /// Último mensaje de otra campaña a cada paciente.
    last_campaign: HashMap<u32, NaiveDateTime>,
}

impl Db {
    fn facts(&self, c: &Campaign, today: NaiveDate) -> Facts {
        let s = &c.segment;
        let mut f = Facts { last_visit: HashMap::new(), pending_budget: HashSet::new(), with_code: HashSet::new(), without_code_recent: HashSet::new(), last_campaign: HashMap::new() };
        if s.last_visit_days.is_some() {
            for a in self.appointments.iter().filter(|a| a.state == ApptState::Finalizada) {
                if let Some(pid) = a.patient_id {
                    let e = f.last_visit.entry(pid).or_insert(a.date);
                    *e = (*e).max(a.date);
                }
            }
        }
        if s.pending_budget {
            f.pending_budget = self.budgets.iter().filter(|b| b.state == BudgetState::Pendiente).map(|b| b.patient_id).collect();
        }
        let (with, without) = (s.with_code.trim(), s.without_code.trim());
        if !with.is_empty() || !without.is_empty() {
            let year_ago = today - chrono::Duration::days(365);
            for e in self.entries.iter().filter(|e| e.is_act()) {
                if !with.is_empty() && e.code == with {
                    f.with_code.insert(e.patient_id);
                }
                if !without.is_empty() && e.code == without && e.date >= year_ago {
                    f.without_code_recent.insert(e.patient_id);
                }
            }
        }
        for x in self.campaigns.iter().filter(|x| x.id != c.id) {
            for r in x.recipients.iter().filter(|r| r.message_id.is_some()) {
                if let Some(s) = r.sent {
                    let e = f.last_campaign.entry(r.patient_id).or_insert(s);
                    *e = (*e).max(s);
                }
            }
        }
        f
    }

    fn in_segment(&self, p: &Patient, s: &Segment, f: &Facts, today: NaiveDate) -> bool {
        if !p.active || p.blocked.is_some() || p.restricted.is_some() {
            return false;
        }
        let age = p.age(today);
        if s.min_age.is_some_and(|m| age.is_none_or(|a| a < m as i32)) || s.max_age.is_some_and(|m| age.is_none_or(|a| a > m as i32)) {
            return false;
        }
        if let Some(d) = s.last_visit_days
            && f.last_visit.get(&p.id).is_none_or(|l| (today - *l).num_days() < d as i64)
        {
            return false;
        }
        if s.pending_budget && !f.pending_budget.contains(&p.id) {
            return false;
        }
        if !s.with_code.trim().is_empty() && !f.with_code.contains(&p.id) {
            return false;
        }
        s.without_code.trim().is_empty() || !f.without_code_recent.contains(&p.id)
    }

    fn exclusion(&self, p: &Patient, c: &Campaign, f: &Facts, now: NaiveDateTime) -> Option<Excluded> {
        if p.marketing.is_none() || !p.accepts_gdpr || p.restricted.is_some() || p.blocked.is_some() {
            return Some(Excluded::SinConsentimiento);
        }
        if p.no_messages.contains(&MsgKind::Campana) {
            return Some(Excluded::DeBaja);
        }
        let ok = match c.channel {
            Channel::WhatsApp => p.accepts_whatsapp && !p.mobile.trim().is_empty(),
            Channel::Sms => p.accepts_sms && !p.mobile.trim().is_empty(),
            Channel::Email => p.accepts_email && p.email.contains('@'),
        };
        if !ok {
            return Some(Excluded::SinContacto);
        }
        f.last_campaign.get(&p.id).is_some_and(|s| (now - *s).num_days() < c.min_days as i64).then_some(Excluded::CampanaReciente)
    }

    /// Quién la recibiría, quién no y por qué, y los primeros mensajes.
    pub fn campaign_preview(&self, c: &Campaign, now: NaiveDateTime) -> Preview {
        let f = self.facts(c, now.date());
        let mut out = Preview::default();
        let mut ex: BTreeMap<Excluded, usize> = BTreeMap::new();
        for p in self.patients.iter().filter(|p| self.in_segment(p, &c.segment, &f, now.date())) {
            match self.exclusion(p, c, &f, now) {
                Some(e) => *ex.entry(e).or_default() += 1,
                None => out.recipients.push(p.id),
            }
        }
        out.excluded = ex.into_iter().collect();
        out.samples = out.recipients.iter().take(3).map(|pid| (*pid, self.campaign_text(c, *pid))).collect();
        out
    }

    /// El texto que le llega: el de la campaña con sus datos (sin datos de
    /// salud) y cómo darse de baja, en su idioma.
    fn campaign_text(&self, c: &Campaign, pid: u32) -> String {
        let lang = self.patient(pid).and_then(|p| p.lang).unwrap_or_default();
        let foot = crate::i18n::tr(lang, "Si no quiere recibir estos mensajes, respóndanos con la palabra BAJA o llámenos al [TELCLINICA].").replace("[TELCLINICA]", &self.config.phone);
        format!("{}\n\n—\n{foot}", self.fill_message(&c.text, pid, &[]).trim_end())
    }

    /// Programar: la lista de destinatarios queda fija (la de la previsualización).
    pub fn campaign_schedule(&mut self, id: u32, now: NaiveDateTime) -> Result<usize, String> {
        let c = self.campaigns.iter().find(|c| c.id == id).cloned().ok_or("La campaña ya no existe.")?;
        if c.state != CampState::Borrador {
            return Err("Solo se programa un borrador.".into());
        }
        if let Some(e) = c.problem() {
            return Err(e);
        }
        let prev = self.campaign_preview(&c, now);
        if prev.recipients.is_empty() {
            return Err("Nadie del segmento puede recibirla.".into());
        }
        let n = prev.recipients.len();
        let who = self.config.current_user.clone();
        if let Some(x) = self.campaigns.iter_mut().find(|x| x.id == id) {
            x.recipients = prev.recipients.into_iter().map(|patient_id| Recipient { patient_id, sent: None, message_id: None, known_appts: vec![] }).collect();
            x.state = CampState::Programada;
            x.created_by = who;
        }
        self.audit("Programar campaña", &format!("{} · {n} destinatarios", c.name));
        Ok(n)
    }

    /// Cancelar una campaña programada: lo que no ha pasado a la cola ya no
    /// pasa, y lo que está en la cola sin enviar se descarta. Devuelve cuántos
    /// mensajes se descartan.
    pub fn campaign_cancel(&mut self, id: u32) -> Result<usize, String> {
        let c = self.campaigns.iter_mut().find(|c| c.id == id).ok_or("La campaña ya no existe.")?;
        if c.state != CampState::Programada {
            return Err("Solo se cancela una campaña programada.".into());
        }
        c.state = CampState::Cancelada;
        let name = c.name.clone();
        let mut dropped = HashSet::new();
        for m in self.messages.iter_mut().filter(|m| m.kind == MsgKind::Campana && m.related == Some(id) && m.state == MsgState::Pendiente && m.sending.is_empty()) {
            m.state = MsgState::Descartado;
            dropped.insert(m.id);
        }
        if let Some(c) = self.campaigns.iter_mut().find(|c| c.id == id) {
            for r in c.recipients.iter_mut().filter(|r| r.message_id.is_some_and(|m| dropped.contains(&m))) {
                r.message_id = None;
            }
        }
        let n = dropped.len();
        self.audit("Cancelar campaña", &format!("{name} · {n} mensajes sin enviar descartados"));
        Ok(n)
    }

    /// Pasa a la cola los mensajes que tocan ahora (horario, días laborables y
    /// límite diario). Devuelve cuántos. No envía nada: la cola se envía como
    /// siempre, desde Comunicaciones.
    pub fn campaign_tick(&mut self, now: NaiveDateTime) -> usize {
        let workday = now.weekday().num_days_from_monday() < 5;
        let ids: Vec<u32> = self.campaigns.iter().filter(|c| c.state == CampState::Programada && workday && now.hour() >= c.from_hour && now.hour() < c.to_hour && c.recipients.iter().any(|r| r.sent.is_none())).map(|c| c.id).collect();
        if ids.is_empty() {
            return 0;
        }
        let mut appts: HashMap<u32, Vec<u32>> = HashMap::new();
        for a in self.appointments.iter() {
            if let Some(pid) = a.patient_id {
                appts.entry(pid).or_default().push(a.id);
            }
        }
        let mut made = 0;
        for id in ids {
            let Some(c) = self.campaigns.iter().find(|c| c.id == id).cloned() else { continue };
            let f = self.facts(&c, now.date());
            let today = c.recipients.iter().filter(|r| r.message_id.is_some() && r.sent.is_some_and(|s| s.date() == now.date())).count() as u32;
            let mut budget = c.per_day.saturating_sub(today);
            let mut updated = c.recipients.clone();
            for r in updated.iter_mut().filter(|r| r.sent.is_none()) {
                if budget == 0 {
                    break;
                }
                // se vuelve a mirar ahora: una baja o un consentimiento retirado mandan
                let ok = self.patient(r.patient_id).is_some_and(|p| self.exclusion(p, &c, &f, now).is_none());
                r.message_id = if ok { Some(self.campaign_message(&c, r.patient_id, now)) } else { None };
                r.sent = Some(now);
                r.known_appts = appts.get(&r.patient_id).cloned().unwrap_or_default();
                if ok {
                    made += 1;
                    budget -= 1;
                }
            }
            if let Some(x) = self.campaigns.iter_mut().find(|x| x.id == id) {
                if updated.iter().all(|r| r.sent.is_some()) {
                    x.state = CampState::Terminada;
                }
                x.recipients = updated;
            }
        }
        made
    }

    /// El mensaje en la cola (o el que ya puso otro puesto).
    fn campaign_message(&mut self, c: &Campaign, pid: u32, now: NaiveDateTime) -> u32 {
        if let Some(m) = self.messages.iter().find(|m| m.kind == MsgKind::Campana && m.related == Some(c.id) && m.patient_id == pid) {
            return m.id;
        }
        let p = self.patient(pid).cloned().unwrap_or_default();
        let to = if c.channel == Channel::Email { p.email.clone() } else { p.mobile.clone() };
        let subject = if c.channel != Channel::Email {
            String::new()
        } else if c.subject.trim().is_empty() {
            self.config.name.clone()
        } else {
            self.fill_message(&c.subject, pid, &[])
        };
        let text = self.campaign_text(c, pid);
        let id = self.new_id();
        let user = self.config.current_user.clone();
        self.messages.push(Message {
            id,
            patient_id: pid,
            kind: MsgKind::Campana,
            channel: c.channel,
            to,
            subject,
            text,
            created: now,
            sent_at: None,
            state: MsgState::Pendiente,
            related: Some(c.id),
            user,
            error: String::new(),
            attempts: 0,
            uncertain: false,
            sending: String::new(),
            sending_since: None,
            sent_by: String::new(),
        });
        id
    }

    /// Resultados con el criterio del módulo ([`CRITERION`]).
    pub fn campaign_results(&self, id: u32) -> Results {
        let Some(c) = self.campaigns.iter().find(|c| c.id == id) else { return Results::default() };
        let mut by_patient: HashMap<u32, Vec<&Appointment>> = HashMap::new();
        for a in self.appointments.iter().filter(|a| !a.state.is_cancelled()) {
            if let Some(pid) = a.patient_id {
                by_patient.entry(pid).or_default().push(a);
            }
        }
        let mut r = Results::default();
        for x in &c.recipients {
            match (x.sent, x.message_id) {
                (Some(s), Some(_)) => {
                    r.queued += 1;
                    let until = s.date() + chrono::Duration::days(c.attribution_days as i64);
                    for a in by_patient.get(&x.patient_id).into_iter().flatten().filter(|a| !x.known_appts.contains(&a.id) && a.date >= s.date() && a.date <= until) {
                        r.appts += 1;
                        if a.state == ApptState::Finalizada {
                            r.attended += 1;
                        }
                    }
                }
                (Some(_), None) => r.skipped += 1,
                (None, _) => r.pending += 1,
            }
        }
        r
    }
}

/// Lo que puede cambiar en una campaña un usuario que no es de Dirección: lo
/// que hace [`Db::campaign_tick`] (marcar destinatarios como pasados a la cola
/// y darla por terminada) y sus borradores. Programarla, cancelarla, cambiar
/// a quién llega o qué dice, o borrarla, no.
pub fn tick_only(current: Option<&Campaign>, after: Option<&Campaign>) -> bool {
    let draft = |c: &Campaign| c.state == CampState::Borrador && c.recipients.is_empty();
    match (current, after) {
        (None, Some(a)) => draft(a),
        (Some(c), Some(a)) if draft(c) => draft(a),
        (Some(c), Some(a)) => {
            let same = |x: &Campaign| (x.id, x.name.clone(), x.segment.clone(), x.channel, x.subject.clone(), x.text.clone(), x.min_days, x.from_hour, x.to_hour, x.per_day, x.attribution_days, x.created_by.clone());
            let open = c.state == CampState::Programada;
            let state_ok = a.state == c.state || (open && a.state == CampState::Terminada && a.recipients.iter().all(|r| r.sent.is_some()));
            let recipients_ok = c.recipients.len() == a.recipients.len() && c.recipients.iter().zip(&a.recipients).all(|(x, y)| x.patient_id == y.patient_id && (x == y || (open && x.sent.is_none() && y.sent.is_some())));
            same(c) == same(a) && state_ok && recipients_ok
        }
        (Some(c), None) => draft(c),
        (None, None) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(d: u32, h: u32) -> NaiveDateTime {
        // noviembre de 2026: el 14 es sábado y el 16, lunes
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
    }

    /// Todos con WhatsApp y consentimiento; de los cinco primeros, el segundo
    /// sin consentimiento para promociones, el tercero de baja y el cuarto sin
    /// móvil.
    fn clinic() -> (Db, Vec<u32>) {
        let mut db = Db::sample(at(16, 9).date());
        db.campaigns.clear();
        db.messages.clear();
        let ids: Vec<u32> = db.patients.iter().filter(|p| p.active && p.blocked.is_none() && p.restricted.is_none()).take(5).map(|p| p.id).collect();
        for p in db.patients.iter_mut() {
            (p.marketing, p.accepts_gdpr, p.accepts_whatsapp, p.mobile) = (Some(at(1, 9).date()), true, true, "600000000".into());
            p.no_messages.clear();
        }
        for (i, pid) in ids.iter().enumerate() {
            let p = db.patients.iter_mut().find(|p| p.id == *pid).unwrap();
            match i {
                1 => p.marketing = None,
                2 => p.no_messages.push(MsgKind::Campana),
                3 => p.mobile.clear(),
                _ => {}
            }
        }
        (db, ids)
    }

    fn camp(db: &mut Db, name: &str) -> Campaign {
        let c = Campaign::new(db.new_id(), name, Channel::WhatsApp, "Hola [NOMBRE], este mes en [CLINICA]…");
        db.campaigns.push(c.clone());
        c
    }

    fn put(db: &mut Db, c: &Campaign) {
        if let Some(x) = db.campaigns.iter_mut().find(|x| x.id == c.id) {
            *x = c.clone();
        }
    }

    fn get(db: &Db, id: u32) -> Campaign {
        db.campaigns.iter().find(|x| x.id == id).unwrap().clone()
    }

    #[test]
    fn the_preview_shows_who_and_why_not_and_scheduling_freezes_the_list() {
        let (mut db, ids) = clinic();
        // otra campaña le llegó al quinto hace 10 días
        let mut old = camp(&mut db, "Anterior");
        old.recipients = vec![Recipient { patient_id: ids[4], sent: Some(at(6, 11)), message_id: Some(1), known_appts: vec![] }];
        old.state = CampState::Terminada;
        put(&mut db, &old);
        let c = camp(&mut db, "Blanqueamiento");
        let prev = db.campaign_preview(&c, at(16, 9));
        assert!(prev.recipients.contains(&ids[0]));
        for (i, pid) in ids.iter().enumerate().skip(1) {
            assert!(!prev.recipients.contains(pid), "el {i} se queda fuera");
        }
        let ex: BTreeMap<Excluded, usize> = prev.excluded.iter().cloned().collect();
        assert_eq!((ex[&Excluded::SinConsentimiento], ex[&Excluded::DeBaja], ex[&Excluded::SinContacto], ex[&Excluded::CampanaReciente]), (1, 1, 1, 1));
        let first = db.patient(prev.samples[0].0).unwrap().first_name.clone();
        assert!(prev.samples[0].1.starts_with(&format!("Hola {first}, este mes en {}", db.config.name)), "con sus datos: {}", prev.samples[0].1);
        assert!(prev.samples[0].1.contains("BAJA"), "y cómo darse de baja");
        // un comodín con datos de salud o de otra clase no se programa
        let mut bad = camp(&mut db, "Mal");
        bad.text = "Hola [NOMBRE], su [ALERTAS]".into();
        put(&mut db, &bad);
        assert!(db.campaign_schedule(bad.id, at(16, 9)).unwrap_err().contains("Comodines"));
        let n = db.campaign_schedule(c.id, at(16, 9)).unwrap();
        assert_eq!(n, prev.recipients.len());
        assert!(db.campaign_schedule(c.id, at(16, 9)).is_err(), "una vez");
        // quien da su consentimiento después no entra: la lista quedó fija
        db.patients.iter_mut().find(|p| p.id == ids[1]).unwrap().marketing = Some(at(16, 10).date());
        assert!(!get(&db, c.id).recipients.iter().any(|r| r.patient_id == ids[1]));
        assert!(db.audit_log.iter().any(|a| a.action == "Programar campaña"));
        assert!(db.messages.is_empty(), "programar no pone nada en la cola");
    }

    #[test]
    fn segments_by_age_last_visit_budget_and_treatment() {
        let (mut db, _) = clinic();
        let today = at(16, 9);
        let mut c = camp(&mut db, "Segmento");
        let all = db.campaign_preview(&c, today).recipients;
        let age = |db: &Db, pid: u32| db.patient(pid).unwrap().age(today.date());
        c.segment.min_age = Some(40);
        let older = db.campaign_preview(&c, today).recipients;
        assert!(!older.is_empty() && older.len() < all.len(), "{} de {}", older.len(), all.len());
        assert!(older.iter().all(|p| age(&db, *p).is_some_and(|a| a >= 40)));
        c.segment = Segment { pending_budget: true, ..Default::default() };
        let with_budget = db.campaign_preview(&c, today).recipients;
        assert!(!with_budget.is_empty());
        assert!(with_budget.iter().all(|p| db.budgets.iter().any(|b| b.patient_id == *p && b.state == BudgetState::Pendiente)));
        // con un tratamiento hecho, y sin él en el último año
        let e = db.entries.iter().find(|e| e.is_act() && all.contains(&e.patient_id)).unwrap().clone();
        c.segment = Segment { with_code: e.code.clone(), ..Default::default() };
        let done = db.campaign_preview(&c, today).recipients;
        assert!(done.contains(&e.patient_id));
        assert!(done.iter().all(|p| db.entries.iter().any(|x| x.patient_id == *p && x.code == e.code && x.is_act())));
        c.segment = Segment { without_code: e.code.clone(), ..Default::default() };
        let not_recent = db.campaign_preview(&c, today).recipients;
        let recent = db.entries.iter().any(|x| x.patient_id == e.patient_id && x.code == e.code && x.is_act() && x.date >= today.date() - chrono::Duration::days(365));
        assert_eq!(not_recent.contains(&e.patient_id), !recent);
        // última visita hace al menos 6 meses
        c.segment = Segment { last_visit_days: Some(180), ..Default::default() };
        for p in db.campaign_preview(&c, today).recipients {
            let last = db.appointments.iter().filter(|a| a.patient_id == Some(p) && a.state == ApptState::Finalizada).map(|a| a.date).max().unwrap();
            assert!((today.date() - last).num_days() >= 180);
        }
        c.segment = Segment { min_age: Some(50), max_age: Some(20), ..Default::default() };
        assert!(c.problem().is_some());
    }

    #[test]
    fn queueing_respects_hours_weekdays_daily_limit_and_late_unsubscribes() {
        let (mut db, _) = clinic();
        let mut c = camp(&mut db, "Revisión anual");
        c.per_day = 2;
        put(&mut db, &c);
        let n = db.campaign_schedule(c.id, at(16, 9)).unwrap();
        assert!(n >= 4, "destinatarios: {n}");
        assert_eq!(db.campaign_tick(at(16, 9)), 0, "antes de la hora, nada");
        assert_eq!(db.campaign_tick(at(16, 19)), 0, "después, tampoco");
        assert_eq!(db.campaign_tick(at(21, 12)), 0, "en sábado, nada");
        assert_eq!(db.campaign_tick(at(16, 12)), 2, "como mucho dos al día");
        assert_eq!(db.campaign_tick(at(16, 15)), 0, "ese día, ya no");
        // uno se da de baja antes de que le toque: no se le pasa
        let quit = get(&db, c.id).recipients.iter().find(|r| r.sent.is_none()).unwrap().patient_id;
        db.patients.iter_mut().find(|p| p.id == quit).unwrap().no_messages.push(MsgKind::Campana);
        db.campaigns.iter_mut().find(|x| x.id == c.id).unwrap().per_day = 1000;
        assert_eq!(db.campaign_tick(at(17, 12)), n - 3);
        assert_eq!(db.campaign_tick(at(18, 12)), 0, "nada dos veces");
        let msgs: Vec<&Message> = db.messages.iter().filter(|m| m.related == Some(c.id)).collect();
        assert_eq!(msgs.len(), n - 1, "a todos menos al que se dio de baja");
        assert!(!msgs.iter().any(|m| m.patient_id == quit));
        assert!(msgs.iter().all(|m| m.text.contains("BAJA") && m.state == MsgState::Pendiente && m.sent_at.is_none()), "en la cola, sin enviar");
        let r = db.campaign_results(c.id);
        assert_eq!((r.queued, r.skipped, r.pending), (n - 1, 1, 0));
        assert_eq!(get(&db, c.id).state, CampState::Terminada);
        // y no le llega otra a nadie antes de `min_days`
        let other = camp(&mut db, "Otra");
        assert!(db.campaign_preview(&other, at(18, 9)).recipients.is_empty());
        assert!(!db.campaign_preview(&other, at(18, 9) + chrono::Duration::days(61)).recipients.is_empty());
    }

    #[test]
    fn cancelling_stops_what_is_left_and_drops_what_is_unsent() {
        let (mut db, _) = clinic();
        let mut c = camp(&mut db, "Implantes");
        c.per_day = 1;
        put(&mut db, &c);
        let n = db.campaign_schedule(c.id, at(16, 9)).unwrap();
        assert_eq!(db.campaign_tick(at(16, 12)), 1);
        assert_eq!(db.campaign_cancel(c.id), Ok(1), "el de la cola, sin enviar, se descarta");
        assert!(db.messages.iter().filter(|m| m.related == Some(c.id)).all(|m| m.state == MsgState::Descartado));
        assert_eq!(db.campaign_tick(at(17, 12)), 0, "y no sale nada más");
        let r = db.campaign_results(c.id);
        assert_eq!((r.queued, r.skipped, r.pending), (0, 1, n - 1));
        assert!(db.campaign_cancel(c.id).is_err());
    }

    /// Dos puestos con la misma campaña: un mensaje por paciente.
    #[test]
    fn two_stations_queue_one_message_per_patient() {
        let (mut db, _) = clinic();
        let c = camp(&mut db, "Higiene");
        db.campaign_schedule(c.id, at(16, 9)).unwrap();
        let mut other = db.clone();
        db.campaign_tick(at(16, 12));
        // el otro puesto aún no sabe que se marcaron, pero ya ve los mensajes
        other.messages = db.messages.clone();
        other.campaign_tick(at(16, 12));
        let mut seen = HashSet::new();
        assert!(other.messages.iter().filter(|m| m.related == Some(c.id)).all(|m| seen.insert(m.patient_id)));
        assert_eq!(other.messages.iter().filter_map(crate::comms::auto_message_key).count(), seen.len(), "y la clave de duplicados los reconoce");
    }

    /// Resultados: solo las citas nuevas tras el mensaje y dentro del plazo.
    #[test]
    fn results_attribute_only_new_appointments_within_the_window() {
        let (mut db, ids) = clinic();
        let pid = ids[0];
        let base = db.appointments.iter().find(|a| a.patient_id.is_some()).unwrap().clone();
        let mut before = base.clone();
        (before.id, before.patient_id, before.date, before.state) = (db.new_id(), Some(pid), at(20, 9).date(), ApptState::Planificada);
        db.appointments.push(before);
        let c = camp(&mut db, "Higiene");
        db.campaign_schedule(c.id, at(16, 9)).unwrap();
        db.campaign_tick(at(16, 12));
        assert!(get(&db, c.id).recipients.iter().any(|r| r.patient_id == pid && r.message_id.is_some()));
        assert_eq!(db.campaign_results(c.id).appts, 0, "lo que ya tenían no cuenta");
        // después del mensaje: una en el plazo (y acude), otra fuera y otra anulada
        for (days, st) in [(10i64, ApptState::Finalizada), (45, ApptState::Planificada), (5, ApptState::Anulada)] {
            let mut a = base.clone();
            (a.id, a.patient_id, a.date, a.state) = (db.new_id(), Some(pid), at(16, 9).date() + chrono::Duration::days(days), st);
            db.appointments.push(a);
        }
        let r = db.campaign_results(c.id);
        assert_eq!((r.appts, r.attended), (1, 1), "ni la que ya tenía, ni la de fuera de plazo, ni la anulada");
    }

    #[test]
    fn only_direction_schedules_or_changes_a_campaign() {
        let (mut db, _) = clinic();
        let draft = camp(&mut db, "Borrador");
        assert!(tick_only(None, Some(&draft)), "un borrador lo hace cualquiera");
        let mut edited = draft.clone();
        edited.text = "Otro texto".into();
        assert!(tick_only(Some(&draft), Some(&edited)));
        db.campaign_schedule(draft.id, at(16, 9)).unwrap();
        let sched = get(&db, draft.id);
        assert!(!tick_only(Some(&draft), Some(&sched)), "programarla, no");
        db.campaign_tick(at(16, 12));
        let ticked = get(&db, draft.id);
        assert!(tick_only(Some(&sched), Some(&ticked)), "pasar mensajes a la cola, sí");
        let mut more = sched.clone();
        more.recipients.push(Recipient { patient_id: 99_999, sent: None, message_id: None, known_appts: vec![] });
        assert!(!tick_only(Some(&sched), Some(&more)), "añadir destinatarios, no");
        let mut swapped = sched.clone();
        swapped.recipients[0].patient_id = 99_999;
        assert!(!tick_only(Some(&sched), Some(&swapped)), "cambiarlos, no");
        let mut texted = sched.clone();
        texted.text = "Otra cosa".into();
        assert!(!tick_only(Some(&sched), Some(&texted)), "cambiar el texto, no");
        let mut cancelled = sched.clone();
        cancelled.state = CampState::Cancelada;
        assert!(!tick_only(Some(&sched), Some(&cancelled)), "cancelarla, no");
        assert!(!tick_only(Some(&sched), None), "borrarla, no");
        let mut unsent = ticked.clone();
        unsent.recipients[0].sent = None;
        assert!(!tick_only(Some(&ticked), Some(&unsent)), "ni deshacer lo pasado a la cola");
    }
}
