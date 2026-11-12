//! Enlace personal de cada cita («Mi cita»): una dirección secreta (un código
//! aleatorio de 128 bits) con la que el paciente, sin usuario ni contraseña,
//! confirma que irá, anula la cita, pide cambiarla a otra hora libre o rellena
//! el cuestionario de salud antes de la visita.
//!
//! La página sólo enseña los datos de esa cita y el nombre de pila: nunca el
//! historial ni las respuestas de salud anteriores (el cuestionario se envía,
//! no se consulta). Las acciones llegan al programa como las solicitudes de
//! cita online y se reflejan en la agenda y en la ficha.

use crate::booking::{Busy, Published};
use crate::health::{AIDS, HealthForm, LAST_VISIT, Part, QUESTIONS};
use crate::model::*;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApptLink {
    pub token: String,
    pub appointment_id: u32,
    pub created: NaiveDateTime,
    /// Cuándo confirmó el paciente que irá.
    #[serde(default)]
    pub confirmed: Option<NaiveDateTime>,
    /// Cuándo la anuló el paciente.
    #[serde(default)]
    pub cancelled: Option<NaiveDateTime>,
    /// Cuándo envió el cuestionario de salud.
    #[serde(default)]
    pub health_at: Option<NaiveDateTime>,
    /// Cuestionario de un paciente aún sin ficha: espera a que se le abra.
    #[serde(default)]
    pub health: Option<HealthForm>,
}

/// Código aleatorio de 128 bits en hexadecimal.
pub fn new_token() -> String {
    use ring::rand::SecureRandom;
    let mut b = [0u8; 16];
    if ring::rand::SystemRandom::new().fill(&mut b).is_err() {
        // sin generador del sistema (no debería pasar): hora en nanosegundos mezclada
        let n = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u128;
        b = (n ^ 0x9e37_79b9_7f4a_7c15_f39c_c060_5ced_c834).to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Un código con el formato correcto (32 cifras hexadecimales).
pub fn valid_token(t: &str) -> bool {
    t.len() == 32 && t.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// Dirección del enlace a partir de la dirección base de la página.
pub fn link_url(base: &str, token: &str) -> String {
    format!("{}/mi-cita/{token}", base.trim().trim_end_matches('/'))
}

impl Db {
    pub fn appt_link(&self, appt_id: u32) -> Option<&ApptLink> {
        self.appt_links.iter().find(|l| l.appointment_id == appt_id)
    }

    /// Código del enlace de la cita (se crea la primera vez).
    pub fn appt_token(&mut self, appt_id: u32) -> String {
        if let Some(l) = self.appt_link(appt_id) {
            return l.token.clone();
        }
        let token = new_token();
        self.appt_links.push(ApptLink { token: token.clone(), appointment_id: appt_id, created: now(), confirmed: None, cancelled: None, health_at: None, health: None });
        token
    }

    /// Enlace para el paciente, si la clínica tiene la página publicada con dirección pública.
    pub fn appt_link_url(&mut self, appt_id: u32) -> Option<String> {
        if !self.booking.enabled || !self.booking.links || self.booking.public_url.trim().is_empty() {
            return None;
        }
        let t = self.appt_token(appt_id);
        Some(link_url(&self.booking.public_url, &t))
    }

    /// Enlace ya creado (sin crearlo), para textos que se preparan sin poder cambiar los datos.
    pub fn existing_link_url(&self, appt_id: u32) -> Option<String> {
        if !self.booking.enabled || !self.booking.links || self.booking.public_url.trim().is_empty() {
            return None;
        }
        self.appt_link(appt_id).map(|l| link_url(&self.booking.public_url, &l.token))
    }

    fn link_mut(&mut self, appt_id: u32) -> Option<&mut ApptLink> {
        self.appt_links.iter_mut().find(|l| l.appointment_id == appt_id)
    }

    /// El paciente confirma que irá.
    pub fn portal_confirm(&mut self, appt_id: u32) -> bool {
        let Some(a) = self.appointments.iter_mut().find(|a| a.id == appt_id) else { return false };
        if a.state.is_cancelled() || a.state == ApptState::Finalizada {
            return false;
        }
        if a.state == ApptState::Planificada {
            a.state = ApptState::Confirmada;
        }
        if let Some(l) = self.link_mut(appt_id) {
            l.confirmed = Some(now());
        }
        true
    }

    /// El paciente anula la cita.
    pub fn portal_cancel(&mut self, appt_id: u32, why: &str) -> bool {
        let Some(a) = self.appointments.iter_mut().find(|a| a.id == appt_id) else { return false };
        if a.state.is_cancelled() || a.state == ApptState::Finalizada {
            return false;
        }
        a.state = ApptState::Anulada;
        let note = format!("Anulada por el paciente por internet el {}{}", now().format("%d/%m/%Y %H:%M"), if why.trim().is_empty() { String::new() } else { format!(": {}", why.trim()) });
        a.notes = if a.notes.trim().is_empty() { note } else { format!("{}\n{note}", a.notes.trim()) };
        if let Some(l) = self.link_mut(appt_id) {
            l.cancelled = Some(now());
        }
        true
    }

    /// Guarda el cuestionario enviado por internet: en la ficha si el paciente
    /// ya la tiene (con sus avisos), o esperando a que se le abra.
    pub fn portal_health(&mut self, appt_id: u32, mut form: HealthForm) -> bool {
        let Some(a) = self.appointments.iter().find(|a| a.id == appt_id).cloned() else { return false };
        form.date = today();
        form.signed = false;
        form.filled_by = "El paciente, por internet".into();
        if form.notes.trim().is_empty() {
            form.notes = format!("Enviado por internet el {} con la cita del {}. Pendiente de revisar y firmar en la clínica.", now().format("%d/%m/%Y %H:%M"), fmt_date(a.date));
        }
        if let Some(l) = self.link_mut(appt_id) {
            l.health_at = Some(now());
        }
        match a.patient_id {
            Some(pid) => {
                form.id = self.new_id();
                form.patient_id = pid;
                let id = form.id;
                self.health.push(form);
                self.apply_health(id);
            }
            None => {
                form.patient_id = 0;
                if let Some(l) = self.link_mut(appt_id) {
                    l.health = Some(form);
                }
            }
        }
        true
    }

    /// Pasa a la ficha los cuestionarios de pacientes nuevos cuando ya tienen ficha. Devuelve cuántos.
    pub fn portal_attach_health(&mut self) -> usize {
        let waiting: Vec<(u32, u32)> = self
            .appt_links
            .iter()
            .filter(|l| l.health.is_some())
            .filter_map(|l| self.appointments.iter().find(|a| a.id == l.appointment_id).and_then(|a| a.patient_id).map(|p| (l.appointment_id, p)))
            .collect();
        for (appt, pid) in &waiting {
            let Some(mut f) = self.link_mut(*appt).and_then(|l| l.health.take()) else { continue };
            f.id = self.new_id();
            f.patient_id = *pid;
            let id = f.id;
            self.health.push(f);
            self.apply_health(id);
        }
        waiting.len()
    }

    /// Hace falta (o conviene) el cuestionario: paciente nuevo, sin cuestionario o con uno de hace más de un año.
    pub fn health_needed(&self, a: &Appointment) -> bool {
        match a.patient_id {
            None => true,
            Some(pid) => self.latest_health(pid).map(|h| h.date + Duration::days(365) < today()).unwrap_or(true),
        }
    }
}

// ───────────────────────────── lo que ve la página ─────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct PubLink {
    pub appt: u32,
    pub date: NaiveDate,
    pub start: NaiveTime,
    pub minutes: u32,
    pub reason: String,
    pub doctor: u32,
    pub doctor_name: String,
    pub first_name: String,
    /// planificada, confirmada, anulada, atendida
    pub state: &'static str,
    pub female: bool,
    pub health_needed: bool,
    pub health_sent: bool,
    pub change_pending: bool,
    /// Documentos del paciente que esperan su firma: (código del enlace, título).
    pub docs: Vec<(String, String)>,
}

fn state_key(s: ApptState) -> &'static str {
    match s {
        ApptState::Planificada => "planificada",
        ApptState::Confirmada | ApptState::Recepcion | ApptState::SalaEspera | ApptState::EnConsulta => "confirmada",
        ApptState::Finalizada => "atendida",
        _ => "anulada",
    }
}

/// Enlaces de las citas de ayer en adelante (las anteriores ya no se pueden gestionar).
pub fn publish_links(db: &Db, today: NaiveDate) -> HashMap<String, PubLink> {
    let mut m = HashMap::new();
    if !db.booking.links {
        return m;
    }
    for l in &db.appt_links {
        let Some(a) = db.appointments.iter().find(|a| a.id == l.appointment_id) else { continue };
        if a.date + Duration::days(1) < today {
            continue;
        }
        let p = a.patient_id.and_then(|id| db.patient(id));
        let first = p.map(|p| p.first_name.clone()).unwrap_or_else(|| a.free_name.split_whitespace().next().unwrap_or("").to_string());
        m.insert(
            l.token.clone(),
            PubLink {
                appt: a.id,
                date: a.date,
                start: a.start,
                minutes: a.minutes,
                reason: a.reason.clone(),
                doctor: a.doctor_id,
                doctor_name: db.doctor(a.doctor_id).map(|d| d.alias.clone()).unwrap_or_default(),
                first_name: first,
                state: state_key(a.state),
                female: p.map(|p| p.sex != "M").unwrap_or(true),
                health_needed: db.booking.ask_health && db.health_needed(a),
                health_sent: l.health_at.is_some(),
                change_pending: db.booking_requests.iter().any(|r| r.replaces == Some(a.id) && r.state == crate::booking::ReqState::Pendiente),
                docs: if db.booking.docs { a.patient_id.map(|pid| db.docs_to_sign(pid)).unwrap_or_default() } else { vec![] },
            },
        );
    }
    m
}

/// Se puede cambiar o anular por internet (con la antelación que pide la clínica).
pub fn can_modify(p: &Published, l: &PubLink, now: NaiveDateTime) -> bool {
    matches!(l.state, "planificada" | "confirmada") && l.date.and_time(l.start) - Duration::hours(p.cancel_hours as i64) > now
}

pub fn link_json(p: &Published, l: &PubLink, now: NaiveDateTime) -> serde_json::Value {
    let past = l.date.and_time(l.start) < now;
    let questions: Vec<serde_json::Value> = if l.health_needed && !l.health_sent && !past && l.state != "anulada" {
        QUESTIONS
            .iter()
            .filter(|q| l.female || !q.female)
            .map(|q| serde_json::json!({"clave": q.key, "texto": q.text, "parte": if q.part == Part::General { "general" } else { "dental" }, "detalle": q.detail}))
            .collect()
    } else {
        vec![]
    };
    serde_json::json!({
        "ok": true,
        "clinica": p.clinic, "direccion": p.address, "telefono": p.phone,
        "nombre": l.first_name,
        "fecha": l.date.format("%Y-%m-%d").to_string(),
        "etiqueta": crate::booking::day_label(l.date),
        "hora": l.start.format("%H:%M").to_string(),
        "minutos": l.minutes, "motivo": l.reason, "profesional": l.doctor_name,
        "estado": if past && l.state != "anulada" { "pasada" } else { l.state },
        "puede_confirmar": !past && l.state == "planificada",
        "puede_cambiar": can_modify(p, l, now) && !l.change_pending,
        "cambio_pendiente": l.change_pending,
        "documentos": if past || l.state == "anulada" { vec![] } else { l.docs.iter().map(|(t, title)| serde_json::json!({"titulo": title, "enlace": format!("../documento/{t}")})).collect::<Vec<_>>() },
        "horas_aviso": p.cancel_hours,
        "cuestionario": { "pedir": !questions.is_empty(), "enviado": l.health_sent, "preguntas": questions, "ultima_visita": LAST_VISIT, "ayudas": AIDS },
    })
}

/// Huecos para cambiar la cita: mismo profesional y duración, sin contar la propia cita.
pub fn change_slots(p: &Published, held: &[Busy], now: NaiveDateTime, l: &PubLink, date: NaiveDate) -> Vec<(NaiveTime, u32)> {
    let own: Busy = (l.doctor, l.date, (l.start.hour() * 60 + l.start.minute()) as i64, (l.start.hour() * 60 + l.start.minute()) as i64 + l.minutes as i64);
    crate::booking::slots_for(p, held, now, l.minutes, &[l.doctor], date, Some(own))
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct HealthBody {
    pub respuestas: BTreeMap<String, bool>,
    pub detalles: BTreeMap<String, String>,
    pub cigarrillos: u32,
    pub cepillados: u32,
    pub ultima_visita: usize,
    pub ayudas: Vec<String>,
    pub motivo: String,
    pub acepta: bool,
}

/// Comprueba el cuestionario recibido y lo convierte (sólo preguntas conocidas, textos acotados).
pub fn health_from(b: &HealthBody) -> Result<HealthForm, String> {
    if !b.acepta {
        return Err("Para enviarlo, confirme que los datos son ciertos.".into());
    }
    let clip = |s: &str, n: usize| s.trim().chars().take(n).collect::<String>();
    let mut yes = BTreeMap::new();
    let mut details = BTreeMap::new();
    for q in QUESTIONS {
        let y = b.respuestas.get(q.key).copied().unwrap_or(false);
        yes.insert(q.key.to_string(), y);
        if y && q.detail && let Some(d) = b.detalles.get(q.key) {
            let d = clip(d, 200);
            if !d.is_empty() {
                details.insert(q.key.to_string(), d);
            }
        }
    }
    Ok(HealthForm {
        id: 0,
        patient_id: 0,
        date: today(),
        filled_by: String::new(),
        reason: clip(&b.motivo, 200),
        yes,
        details,
        cigarettes: b.cigarrillos.min(80),
        brushings: b.cepillados.min(6),
        last_visit: b.ultima_visita.min(LAST_VISIT.len() - 1),
        aids: b.ayudas.iter().filter(|a| AIDS.contains(&a.as_str())).cloned().collect(),
        signed: false,
        notes: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_checked() {
        let a = new_token();
        let b = new_token();
        assert!(valid_token(&a) && valid_token(&b) && a != b);
        assert!(!valid_token("../../etc/passwd") && !valid_token(&a.to_uppercase()) && !valid_token(&a[..31]));
    }

    #[test]
    fn patient_confirms_cancels_and_sends_health() {
        let mut db = Db::sample(today());
        db.booking.enabled = true;
        db.booking.public_url = "https://citas.example.com/".into();
        let a = db.appointments.iter().find(|a| a.date >= today() && a.state == ApptState::Planificada && a.patient_id.is_some()).cloned().unwrap();
        let url = db.appt_link_url(a.id).unwrap();
        assert!(url.starts_with("https://citas.example.com/mi-cita/"));
        assert_eq!(db.appt_link_url(a.id).unwrap(), url, "el mismo enlace cada vez");
        assert!(db.portal_confirm(a.id));
        assert_eq!(db.appointments.iter().find(|x| x.id == a.id).unwrap().state, ApptState::Confirmada);
        // cuestionario: va a la ficha con sus avisos, sin firmar
        let mut b = HealthBody { acepta: true, ..Default::default() };
        b.respuestas.insert("anticoag".into(), true);
        b.detalles.insert("anticoag".into(), "Sintrom".into());
        b.respuestas.insert("inventada".into(), true);
        let f = health_from(&b).unwrap();
        assert!(!f.yes.contains_key("inventada"));
        let pid = a.patient_id.unwrap();
        assert!(db.portal_health(a.id, f));
        let h = db.latest_health(pid).unwrap();
        assert!(h.is("anticoag") && !h.signed && h.filled_by.contains("internet"));
        assert!(db.patient(pid).unwrap().alerts.contains("Anticoagulado"));
        assert!(db.portal_cancel(a.id, "Estoy de viaje"));
        let x = db.appointments.iter().find(|x| x.id == a.id).unwrap();
        assert_eq!(x.state, ApptState::Anulada);
        assert!(x.notes.contains("Estoy de viaje"));
        assert!(!db.portal_confirm(a.id), "anulada: ya no se confirma");
        assert!(health_from(&HealthBody::default()).is_err(), "hay que aceptar");
    }

    #[test]
    fn new_patient_health_waits_for_the_file() {
        let mut db = Db::sample(today());
        let a = db.appointments.iter().find(|a| a.patient_id.is_none()).cloned().unwrap();
        db.appt_token(a.id);
        let mut b = HealthBody { acepta: true, ..Default::default() };
        b.respuestas.insert("alergia_med".into(), true);
        b.detalles.insert("alergia_med".into(), "Penicilina".into());
        let n = db.health.len();
        assert!(db.portal_health(a.id, health_from(&b).unwrap()));
        assert_eq!(db.health.len(), n, "aún no hay ficha");
        assert_eq!(db.portal_attach_health(), 0);
        db.appointments.iter_mut().find(|x| x.id == a.id).unwrap().patient_id = Some(110);
        assert_eq!(db.portal_attach_health(), 1);
        assert!(db.patient(110).unwrap().allergies.contains("Penicilina"));
        assert!(db.appt_link(a.id).unwrap().health.is_none());
    }
}
