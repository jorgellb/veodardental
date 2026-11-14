//! Cita online: una página web, servida por VeodarDental, en la que los
//! pacientes eligen motivo, profesional, día y hora entre los huecos libres
//! de la agenda y dejan sus datos. La solicitud llega a recepción, que la
//! confirma (o se confirma sola si así se ha configurado) y se avisa al
//! paciente por correo con la cita para su calendario.
//!
//! La página sólo muestra horas libres: nunca nombres ni datos de otros
//! pacientes. El servidor no toca los datos de la clínica: trabaja con una
//! copia de la disponibilidad que el programa le actualiza cada pocos segundos.

use crate::model::*;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Motivo de consulta que se puede pedir por internet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookReason {
    pub name: String,
    pub minutes: u32,
    /// Tratamiento con el que se apunta la cita (código de la tarifa).
    pub code: String,
    /// Profesionales que lo atienden (vacío = todos los que se ofrecen).
    pub doctors: Vec<u32>,
    pub active: bool,
}

impl Default for BookReason {
    fn default() -> Self {
        BookReason { name: String::new(), minutes: 30, code: String::new(), doctors: vec![], active: true }
    }
}

fn reason(name: &str, minutes: u32, code: &str, doctors: &[u32]) -> BookReason {
    BookReason { name: name.into(), minutes, code: code.into(), doctors: doctors.to_vec(), active: true }
}

pub fn default_reasons() -> Vec<BookReason> {
    vec![
        reason("Primera visita y revisión", 30, "0101", &[]),
        reason("Limpieza dental (higiene)", 45, "0201", &[]),
        reason("Urgencia o dolor", 30, "", &[]),
        reason("Revisión periódica", 20, "0102", &[]),
        reason("Consulta de ortodoncia", 30, "", &[]),
        reason("Consulta de implantes o estética", 30, "", &[]),
    ]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookingConfig {
    pub enabled: bool,
    /// Ordenador que publica la página (como el que envía los WhatsApp).
    pub station: String,
    pub port: u16,
    /// Escuchar en toda la red local (el túnel o el servidor web está en otro equipo).
    pub lan: bool,
    /// Dirección pública con la que los pacientes abren la página (la del túnel https).
    pub public_url: String,
    /// Texto de bienvenida de la página.
    pub intro: String,
    pub reasons: Vec<BookReason>,
    /// Profesionales que se ofrecen (vacío = todos los activos del centro).
    pub doctors: Vec<u32>,
    /// Hasta cuántos días por delante se puede pedir.
    pub days_ahead: u32,
    /// Antelación mínima en horas.
    pub min_notice_hours: u32,
    /// Las solicitudes se convierten en cita sin que recepción las revise.
    pub auto_confirm: bool,
    /// Avisar al paciente por correo al confirmar o rechazar (si hay correo configurado).
    pub email_patient: bool,
    /// Centro al que van las citas (0 = el centro principal).
    pub center_id: u32,
    /// Enlace personal de cada cita para confirmarla, anularla, cambiarla o rellenar el cuestionario de salud.
    pub links: bool,
    /// Hasta cuántas horas antes se puede anular o cambiar por internet.
    pub cancel_hours: u32,
    /// Pedir el cuestionario de salud a quien no lo tiene (o lo tiene de hace más de un año).
    pub ask_health: bool,
    /// Presupuestos para revisar y firmar por internet.
    pub budgets: bool,
    /// Cuotas sin intereses que se ofrecen al aceptar un presupuesto.
    pub installment_options: Vec<u32>,
    /// Importe mínimo para ofrecer cuotas.
    pub finance_min: f64,
    /// Código (token) del túnel de Cloudflare que publica la página en internet.
    pub tunnel_token: String,
    /// Abrir el túnel al arrancar el programa.
    pub tunnel_auto: bool,
    /// Ruta de cloudflared si no está en el sitio habitual.
    pub tunnel_exe: String,
    /// Consentimientos y documentos para firmar por internet.
    pub docs: bool,
    /// Añadir el enlace que corresponda (cita, página de cita online, presupuesto)
    /// a los recordatorios, avisos de revisión y seguimientos que salen por correo o WhatsApp.
    pub links_in_messages: bool,
    /// Ofrecer a la lista de espera el hueco de una cita que el paciente anula por internet.
    pub offer_slots: bool,
    /// Cuántos pacientes de la lista de espera se avisan, como mucho, por cada hueco.
    pub offer_max: u32,
    /// No ofrecer huecos con menos de estas horas por delante.
    pub offer_min_hours: u32,
}

impl Default for BookingConfig {
    fn default() -> Self {
        BookingConfig {
            enabled: false,
            station: String::new(),
            port: 8791,
            lan: false,
            public_url: String::new(),
            intro: "Elija el motivo de la consulta, el día y la hora que mejor le vengan. Le confirmaremos la cita lo antes posible.".into(),
            reasons: default_reasons(),
            doctors: vec![],
            days_ahead: 30,
            min_notice_hours: 12,
            auto_confirm: false,
            email_patient: true,
            center_id: 0,
            links: true,
            cancel_hours: 24,
            ask_health: true,
            budgets: true,
            installment_options: vec![3, 6, 12],
            finance_min: 300.0,
            tunnel_token: String::new(),
            tunnel_auto: true,
            tunnel_exe: String::new(),
            docs: true,
            links_in_messages: true,
            offer_slots: true,
            offer_max: 5,
            offer_min_hours: 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReqState {
    Pendiente,
    Confirmada,
    Rechazada,
}

impl ReqState {
    pub fn label(self) -> &'static str {
        match self {
            ReqState::Pendiente => "Por confirmar",
            ReqState::Confirmada => "Confirmada",
            ReqState::Rechazada => "Rechazada",
        }
    }
}

/// Solicitud de cita hecha desde la página.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BookingRequest {
    pub id: u32,
    pub received: NaiveDateTime,
    pub name: String,
    pub phone: String,
    pub email: String,
    pub reason: String,
    pub minutes: u32,
    pub code: String,
    pub doctor_id: u32,
    pub date: NaiveDate,
    pub start: NaiveTime,
    pub notes: String,
    /// El paciente dice que es su primera visita.
    pub first_visit: bool,
    /// Ficha que coincide por teléfono o correo.
    pub patient_id: Option<u32>,
    pub state: ReqState,
    pub appointment_id: Option<u32>,
    /// Motivo del rechazo (se le dice al paciente).
    #[serde(default)]
    pub answer: String,
    #[serde(default)]
    pub center_id: u32,
    /// Cambio de una cita que ya existía (pedido desde su enlace).
    #[serde(default)]
    pub replaces: Option<u32>,
}

impl BookingRequest {
    pub fn end(&self) -> NaiveTime {
        self.start + Duration::minutes(self.minutes as i64)
    }
}

pub fn digits(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Mismo teléfono (se comparan las 9 últimas cifras: con o sin prefijo).
pub fn same_phone(a: &str, b: &str) -> bool {
    let (a, b) = (digits(a), digits(b));
    a.len() >= 9 && b.len() >= 9 && a[a.len() - 9..] == b[b.len() - 9..]
}

fn mins(t: NaiveTime) -> i64 {
    (t.hour() * 60 + t.minute()) as i64
}

fn at(m: i64) -> NaiveTime {
    NaiveTime::from_hms_opt((m / 60).clamp(0, 23) as u32, (m % 60) as u32, 0).unwrap_or_default()
}

// ───────────────────────────── disponibilidad publicada ─────────────────────────────

#[derive(Clone, Debug, Default, Serialize)]
pub struct PubReason {
    pub name: String,
    pub minutes: u32,
    pub doctors: Vec<u32>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PubDoctor {
    pub id: u32,
    pub name: String,
    pub specialty: String,
}

/// Hueco ocupado: profesional, día, inicio y fin (en minutos del día).
pub type Busy = (u32, NaiveDate, i64, i64);

/// Lo que la página necesita saber (sin datos de pacientes).
#[derive(Clone, Debug, Default)]
pub struct Published {
    pub clinic: String,
    pub address: String,
    pub phone: String,
    pub intro: String,
    pub reasons: Vec<PubReason>,
    pub doctors: Vec<PubDoctor>,
    pub open: i64,
    pub close: i64,
    /// Tramos en que pasa consulta cada profesional, por día de la semana (en minutos del día).
    pub hours: HashMap<u32, [Vec<(i64, i64)>; 7]>,
    pub step: i64,
    pub days_ahead: u32,
    pub min_notice_hours: u32,
    pub auto_confirm: bool,
    pub closed: Vec<NaiveDate>,
    pub busy: Vec<Busy>,
    /// Bloqueos de todos los días (comida…): profesional, inicio, fin.
    pub daily: Vec<(u32, i64, i64)>,
    pub accepting: bool,
    /// Enlaces personales de las citas (por código).
    pub links: HashMap<String, crate::portal::PubLink>,
    pub cancel_hours: u32,
    /// Presupuestos para firmar (por código).
    pub budgets: HashMap<String, crate::budget_link::PubBudget>,
    /// Documentos para firmar (por código).
    pub docs: HashMap<String, crate::doc_link::PubDoc>,
    /// Huecos libres ofrecidos a la lista de espera (por código de cada invitado).
    pub offers: HashMap<String, crate::slot_offer::PubOffer>,
    /// «Mis recetas» de cada paciente con enlace (por código).
    pub rx: HashMap<String, crate::rx_link::PubRx>,
    /// Peticiones de pago por internet (por código).
    pub pays: HashMap<String, crate::pago::PubPay>,
    /// Dirección pública de la página (para que Redsys avise y devuelva al paciente).
    pub public_url: String,
}

/// Profesionales que se ofrecen en la página.
pub fn offered_doctors(db: &Db) -> Vec<u32> {
    let c = &db.booking;
    let center = cid(if c.center_id == 0 { 1 } else { c.center_id });
    db.collaborators.iter().filter(|d| d.active && db.works_in(d.id, center) && (c.doctors.is_empty() || c.doctors.contains(&d.id))).map(|d| d.id).collect()
}

/// Prepara la disponibilidad para la página a partir de la agenda.
pub fn publish(db: &Db, today: NaiveDate) -> Published {
    let c = &db.booking;
    let docs = offered_doctors(db);
    let last = today + Duration::days(c.days_ahead.min(180) as i64 + 1);
    let reasons: Vec<PubReason> = c
        .reasons
        .iter()
        .filter(|r| r.active && !r.name.trim().is_empty())
        .map(|r| PubReason { name: r.name.clone(), minutes: r.minutes.clamp(5, 240), doctors: docs.iter().copied().filter(|d| r.doctors.is_empty() || r.doctors.contains(d)).collect() })
        .filter(|r| !r.doctors.is_empty())
        .collect();
    let mut busy: Vec<Busy> = db.appointments.iter().filter(|a| a.date >= today && a.date <= last && !a.state.is_cancelled() && docs.contains(&a.doctor_id)).map(|a| (a.doctor_id, a.date, mins(a.start), mins(a.start) + a.minutes as i64)).collect();
    busy.extend(db.blocks.iter().filter_map(|b| b.date.filter(|d| *d >= today && *d <= last).map(|d| (b.doctor_id, d, mins(b.start), mins(b.end)))));
    // las solicitudes por confirmar guardan su hueco
    busy.extend(db.booking_requests.iter().filter(|r| r.state == ReqState::Pendiente && r.date >= today).map(|r| (r.doctor_id, r.date, mins(r.start), mins(r.start) + r.minutes as i64)));
    Published {
        clinic: db.config.name.clone(),
        address: db.config.address.clone(),
        phone: db.config.phone.clone(),
        intro: c.intro.clone(),
        reasons,
        doctors: db.collaborators.iter().filter(|d| docs.contains(&d.id)).map(|d| PubDoctor { id: d.id, name: d.alias.clone(), specialty: d.specialty.clone() }).collect(),
        open: mins(db.clinic_week().span().map(|s| s.0).unwrap_or(db.config.open_time)),
        close: mins(db.clinic_week().span().map(|s| s.1).unwrap_or(db.config.close_time)),
        hours: docs.iter().map(|d| (*d, db.doctor_week(*d).0.map(|day| day.iter().map(|s| (mins(s.start), mins(s.end))).collect()))).collect(),
        step: db.config.slot_minutes.clamp(10, 60) as i64,
        days_ahead: c.days_ahead.min(180),
        min_notice_hours: c.min_notice_hours,
        auto_confirm: c.auto_confirm,
        closed: db.holidays.iter().filter(|h| h.date >= today && h.date <= last).map(|h| h.date).collect(),
        busy,
        daily: db.blocks.iter().filter(|b| b.date.is_none()).map(|b| (b.doctor_id, mins(b.start), mins(b.end))).collect(),
        accepting: c.enabled,
        links: crate::portal::publish_links(db, today),
        cancel_hours: c.cancel_hours,
        budgets: crate::budget_link::publish_budgets(db, today),
        docs: crate::doc_link::publish_docs(db, today),
        offers: crate::slot_offer::publish_offers(db, today),
        rx: crate::rx_link::publish_rx(db, today),
        pays: crate::pago::publish_pays(db, today.and_time(chrono::Local::now().naive_local().time())),
        public_url: c.public_url.trim().trim_end_matches('/').to_string(),
    }
}

/// Horas libres de un día para un motivo: (hora, profesional que la atiende).
/// `doctor` 0 = cualquiera (el primero libre a esa hora).
pub fn slots(p: &Published, held: &[Busy], now: NaiveDateTime, reason: usize, doctor: u32, date: NaiveDate) -> Vec<(NaiveTime, u32)> {
    let Some(r) = p.reasons.get(reason) else { return vec![] };
    let cands: Vec<u32> = r.doctors.iter().copied().filter(|d| doctor == 0 || *d == doctor).collect();
    slots_for(p, held, now, r.minutes, &cands, date, None)
}

/// Horas libres de `minutes` minutos con alguno de `cands`; `ignore` es un hueco
/// ocupado que no cuenta (la propia cita cuando se cambia de hora).
pub fn slots_for(p: &Published, held: &[Busy], now: NaiveDateTime, minutes: u32, cands: &[u32], date: NaiveDate, ignore: Option<Busy>) -> Vec<(NaiveTime, u32)> {
    let today = now.date();
    if date < today || date > today + Duration::days(p.days_ahead as i64) || p.closed.contains(&date) {
        return vec![];
    }
    let wd = date.weekday().num_days_from_monday() as usize;
    let earliest = now + Duration::hours(p.min_notice_hours as i64);
    let len = minutes as i64;
    let mut out = vec![];
    let mut t = p.open;
    while t + len <= p.close {
        let start = date.and_time(at(t));
        if start >= earliest {
            let free = |d: u32| {
                let e = t + len;
                // dentro de su horario ese día de la semana
                let works = p.hours.get(&d).is_some_and(|w| w[wd].iter().any(|(a, b)| *a <= t && e <= *b));
                works && !p.busy.iter().chain(held.iter()).any(|b| Some(*b) != ignore && b.0 == d && b.1 == date && b.2 < e && t < b.3) && !p.daily.iter().any(|b| b.0 == d && b.1 < e && t < b.2)
            };
            if let Some(d) = cands.iter().copied().find(|d| free(*d)) {
                out.push((at(t), d));
            }
        }
        t += p.step;
    }
    out
}

/// «lunes 10 de noviembre».
pub fn day_label(d: NaiveDate) -> String {
    format!("{} {} de {}", weekday_es(d), d.day(), month_es(d.month()))
}

// ───────────────────────────── solicitudes en la agenda ─────────────────────────────

impl Db {
    /// Dirección pública de la página para pedir cita (si está publicada).
    pub fn booking_page_url(&self) -> Option<String> {
        let u = self.booking.public_url.trim().trim_end_matches('/');
        (self.booking.enabled && u.starts_with("http")).then(|| format!("{u}/"))
    }

    pub fn booking_pending(&self) -> usize {
        self.booking_requests.iter().filter(|r| r.state == ReqState::Pendiente).count()
    }

    /// Guarda una solicitud recibida y la relaciona con la ficha del paciente si la hay.
    pub fn booking_receive(&mut self, mut r: BookingRequest) -> u32 {
        r.id = self.new_id();
        r.state = ReqState::Pendiente;
        let mail = r.email.trim().to_lowercase();
        let known = r.patient_id;
        r.patient_id = known.or_else(|| {
            self.patients
                .iter()
                .filter(|p| p.active)
                .find(|p| same_phone(&p.mobile, &r.phone) || same_phone(&p.phone, &r.phone))
                .or_else(|| if mail.is_empty() { None } else { self.patients.iter().find(|p| p.active && p.email.trim().to_lowercase() == mail) })
                .map(|p| p.id)
        });
        let id = r.id;
        self.booking_requests.push(r);
        id
    }

    /// Convierte la solicitud en cita. Devuelve la cita creada.
    pub fn booking_confirm(&mut self, id: u32) -> Result<u32, String> {
        let r = self.booking_requests.iter().find(|r| r.id == id).cloned().ok_or("La solicitud ya no existe.")?;
        if r.state != ReqState::Pendiente {
            return Err("La solicitud ya se atendió.".into());
        }
        // fuera de horario (p. ej. se cambió el horario del profesional después de pedirla)
        if let Some(why) = self.schedule_problem(r.doctor_id, r.date, r.start, r.end()) {
            return Err(format!("{why} Llame al paciente para darle otra hora, o rechace la solicitud."));
        }
        if !self.is_free(r.doctor_id, r.date, r.start, r.end(), r.replaces) {
            return Err("Esa hora ya está ocupada en la agenda: mueva la cita a otra hora al llamar al paciente, o rechace la solicitud.".into());
        }
        // cambio de hora de una cita que ya existía
        if let Some(old) = r.replaces {
            let a = self.appointments.iter_mut().find(|a| a.id == old).ok_or("La cita que se quería cambiar ya no existe.")?;
            if a.state.is_cancelled() || a.state == ApptState::Finalizada {
                return Err("La cita que se quería cambiar ya está anulada o atendida.".into());
            }
            let note = format!("Cambiada por el paciente por internet (antes: {} {}).", fmt_date(a.date), a.start.format("%H:%M"));
            a.notes = if a.notes.trim().is_empty() { note } else { format!("{}\n{note}", a.notes.trim()) };
            a.date = r.date;
            a.start = r.start;
            a.doctor_id = r.doctor_id;
            a.state = ApptState::Confirmada;
            a.reminder_sent = false;
            if let Some(x) = self.booking_requests.iter_mut().find(|x| x.id == id) {
                x.state = ReqState::Confirmada;
                x.appointment_id = Some(old);
            }
            return Ok(old);
        }
        let center = cid(r.center_id);
        // primer gabinete del centro sin otra cita a esa hora
        let boxes: Vec<u32> = self.boxes.iter().filter(|b| cid(b.center_id) == center).map(|b| b.id).collect();
        let box_id = boxes.iter().copied().find(|b| !self.appointments.iter().any(|a| a.box_id == *b && a.date == r.date && !a.state.is_cancelled() && a.start < r.end() && r.start < a.end())).or(boxes.first().copied()).unwrap_or(1);
        let appt = self.new_id();
        let notes = if r.notes.trim().is_empty() { "Pedida por internet.".to_string() } else { format!("Pedida por internet: {}", r.notes.trim()) };
        self.appointments.push(Appointment {
            id: appt,
            date: r.date,
            start: r.start,
            minutes: r.minutes,
            patient_id: r.patient_id,
            free_name: if r.patient_id.is_some() { String::new() } else { r.name.clone() },
            phone: r.phone.clone(),
            doctor_id: r.doctor_id,
            box_id,
            reason: r.reason.clone(),
            treatment_code: r.code.clone(),
            state: ApptState::Confirmada,
            notes,
            arrived_at: None,
            seen_at: None,
            finished_at: None,
            reminder_sent: false,
            first_visit: r.patient_id.is_none(),
            center_id: center,
        });
        if let Some(x) = self.booking_requests.iter_mut().find(|x| x.id == id) {
            x.state = ReqState::Confirmada;
            x.appointment_id = Some(appt);
        }
        if self.booking.links {
            self.appt_token(appt);
        }
        self.refresh_recalls(today());
        Ok(appt)
    }

    pub fn booking_reject(&mut self, id: u32, why: &str) -> Result<(), String> {
        let x = self.booking_requests.iter_mut().find(|x| x.id == id).ok_or("La solicitud ya no existe.")?;
        if x.state != ReqState::Pendiente {
            return Err("La solicitud ya se atendió.".into());
        }
        x.state = ReqState::Rechazada;
        x.answer = why.trim().to_string();
        Ok(())
    }
}

fn ics_text(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n")
}

/// Cita en formato de calendario (.ics).
#[allow(clippy::too_many_arguments)]
pub fn ics_event(uid: u32, clinic: &str, address: &str, phone: &str, date: NaiveDate, start: NaiveTime, minutes: u32, what: &str) -> String {
    let f = |d: NaiveDate, t: NaiveTime| format!("{}T{}", d.format("%Y%m%d"), t.format("%H%M%S"));
    let end = date.and_time(start) + Duration::minutes(minutes as i64);
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//VeodarDental//Cita online//ES\r\nMETHOD:PUBLISH\r\nBEGIN:VEVENT\r\nUID:cita-{uid}@veodardental\r\nDTSTAMP:{stamp}\r\nDTSTART:{}\r\nDTEND:{}\r\nSUMMARY:{}\r\nLOCATION:{}\r\nDESCRIPTION:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        f(date, start),
        f(end.date(), end.time()),
        ics_text(&format!("Cita en {clinic}")),
        ics_text(address),
        ics_text(&format!("{what}. Si no puede acudir, llame al {phone}."))
    )
}

/// La cita de una solicitud confirmada, para el calendario del paciente.
pub fn ics(db: &Db, r: &BookingRequest) -> String {
    let doctor = db.doctor(r.doctor_id).map(|d| d.alias.clone()).unwrap_or_default();
    ics_event(r.appointment_id.unwrap_or(r.id), &db.config.name, &db.config.address, &db.config.phone, r.date, r.start, r.minutes, &format!("{} · {doctor}", r.reason))
}

/// Correo para el paciente al confirmar (con la cita para el calendario) o rechazar.
pub fn patient_mail(db: &Db, r: &BookingRequest) -> Option<(String, String, String, Vec<crate::mail::FileAttachment>)> {
    let to = r.email.trim();
    if to.is_empty() {
        return None;
    }
    let c = &db.config;
    let first = r.name.split_whitespace().next().unwrap_or("").to_string();
    let doctor = db.doctor(r.doctor_id).map(|d| d.alias.clone()).unwrap_or_default();
    let link = r.appointment_id.and_then(|a| db.existing_link_url(a)).map(|u| format!("\n\nDesde este enlace puede confirmar que vendrá, cambiar la hora, anular la cita o rellenar el cuestionario de salud antes de venir:\n{u}")).unwrap_or_default();
    match r.state {
        ReqState::Confirmada => Some((
            to.to_string(),
            if r.replaces.is_some() { format!("Cambio de cita en {}: {} a las {}", c.name, day_label(r.date), r.start.format("%H:%M")) } else { format!("Cita confirmada en {}: {} a las {}", c.name, day_label(r.date), r.start.format("%H:%M")) },
            format!(
                "Hola, {first}:\n\n{}\n\n  {} a las {}\n  {} · {}\n  {}{link}\n\nAdjuntamos la cita para que la guarde en su calendario. Si no puede acudir, avísenos llamando al {}.\n\nUn saludo,\n{}",
                if r.replaces.is_some() { "Hemos cambiado su cita. La nueva hora es:" } else { "Su cita queda confirmada." },
                long_date(r.date),
                r.start.format("%H:%M"),
                r.reason,
                doctor,
                c.address,
                c.phone,
                c.name
            ),
            vec![crate::mail::FileAttachment { name: "cita.ics".into(), mime: "text/calendar; charset=utf-8".into(), bytes: ics(db, r).into_bytes() }],
        )),
        ReqState::Rechazada => {
            let link = if db.booking.public_url.trim().is_empty() { String::new() } else { format!("elegir otra hora en {} o ", db.booking.public_url.trim()) };
            Some((
                to.to_string(),
                format!("Su solicitud de cita en {}", c.name),
                format!(
                    "Hola, {first}:\n\nLo sentimos, no podemos darle la cita del {} a las {}{}.\n\nPuede {link}llamarnos al {} y le buscamos otro hueco.\n\nUn saludo,\n{}",
                    long_date(r.date),
                    r.start.format("%H:%M"),
                    if r.answer.is_empty() { String::new() } else { format!(": {}", r.answer) },
                    c.phone,
                    c.name
                ),
                vec![],
            ))
        }
        ReqState::Pendiente => None,
    }
}

// ───────────────────────────── servidor de la página ─────────────────────────────

const MAX_BODY: u64 = 16 * 1024;
const PAGE: &str = include_str!("booking_page.html");
const PORTAL: &str = include_str!("portal_page.html");
const BUDGET_PAGE: &str = include_str!("budget_page.html");
const DOC_PAGE: &str = include_str!("doc_page.html");
const OFFER_PAGE: &str = include_str!("offer_page.html");
const RX_PAGE: &str = include_str!("rx_page.html");

/// Una solicitud recibida y el hueco que se reservó para ella.
pub struct Incoming {
    pub request: BookingRequest,
    pub hold: Busy,
}

/// Lo que el paciente hace desde el enlace de su cita.
pub enum ApptAction {
    Confirm,
    Cancel(String),
    /// Pide cambiarla a otra hora (una solicitud con `replaces`).
    Change(Incoming),
    Health(crate::health::HealthForm),
}

/// Lo que llega de la página al programa.
pub enum WebEvent {
    Booking(Incoming),
    Appt {
        appt: u32,
        action: ApptAction,
    },
    Budget {
        budget: u32,
        action: crate::budget_link::BudgetAction,
    },
    Doc {
        doc: u32,
        action: crate::doc_link::DocAction,
    },
    Offer {
        token: String,
        action: crate::slot_offer::OfferAction,
    },
    /// El paciente ha entrado en «Mis recetas».
    Rx {
        patient: u32,
    },
    /// El paciente va a pagar (pedido nuevo en Redsys).
    PayStarted {
        token: String,
        order: String,
        method: crate::pago::Method,
    },
    /// Notificación de Redsys con firma comprobada.
    PayNotice(crate::pago::Notice),
}

#[derive(Default)]
pub struct Shared {
    pub published: Published,
    /// Huecos de solicitudes que el programa aún no ha recogido.
    pub held: Vec<Busy>,
    pub received: u64,
    pub refused: u64,
    hits: HashMap<String, Vec<Instant>>,
    /// TPV virtual de este ordenador (la clave no sale de aquí).
    pub tpv: Option<crate::pago::TpvSettings>,
}

pub struct BookingServer {
    pub port: u16,
    pub rx: Receiver<WebEvent>,
    pub shared: Arc<Mutex<Shared>>,
    server: Arc<tiny_http::Server>,
}

impl Drop for BookingServer {
    fn drop(&mut self) {
        self.server.unblock();
    }
}

fn query(url: &str, key: &str) -> Option<String> {
    url.split_once('?')?.1.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BookBody {
    motivo: usize,
    profesional: u32,
    fecha: String,
    hora: String,
    nombre: String,
    telefono: String,
    email: String,
    notas: String,
    primera: bool,
    acepta: bool,
    /// Campo trampa: las personas no lo ven; los robots lo rellenan.
    web: String,
}

fn respond(req: tiny_http::Request, code: u16, ctype: &str, body: String) {
    let mut r = tiny_http::Response::from_string(body).with_status_code(code);
    for (k, v) in [
        ("Content-Type", ctype),
        ("Cache-Control", "no-store"),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
        ("Content-Security-Policy", "default-src 'self'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src 'self' data:; frame-ancestors *"),
    ] {
        if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
            r = r.with_header(h);
        }
    }
    let _ = req.respond(r);
}

fn json(req: tiny_http::Request, code: u16, v: serde_json::Value) {
    respond(req, code, "application/json; charset=utf-8", v.to_string());
}

fn client_ip(req: &tiny_http::Request) -> String {
    let remote = req.remote_addr().map(|a| a.ip());
    let local = remote.map(|ip| ip.is_loopback()).unwrap_or(true);
    if local {
        // detrás del túnel: la dirección real viene en la cabecera
        for h in ["CF-Connecting-IP", "X-Forwarded-For", "X-Real-IP"] {
            if let Some(v) = req.headers().iter().find(|x| x.field.equiv(h))
                && let Some(first) = v.value.as_str().split(',').next()
            {
                return first.trim().to_string();
            }
        }
    }
    remote.map(|ip| ip.to_string()).unwrap_or_default()
}

/// Demasiadas peticiones desde la misma dirección: `max` en `secs` segundos.
fn too_many(sh: &mut Shared, key: &str, max: usize, secs: u64) -> bool {
    let now = Instant::now();
    let v = sh.hits.entry(key.to_string()).or_default();
    v.retain(|t| now.duration_since(*t).as_secs() < secs);
    if v.len() >= max {
        return true;
    }
    v.push(now);
    if sh.hits.len() > 5000 {
        sh.hits.retain(|_, v| !v.is_empty());
    }
    false
}

pub fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn info_json(p: &Published) -> serde_json::Value {
    serde_json::json!({
        "clinica": p.clinic, "direccion": p.address, "telefono": p.phone, "intro": p.intro,
        "abierta": p.accepting, "confirmacion_inmediata": p.auto_confirm, "dias": p.days_ahead,
        "motivos": p.reasons.iter().enumerate().map(|(i, r)| serde_json::json!({"id": i, "nombre": r.name, "minutos": r.minutes, "profesionales": r.doctors})).collect::<Vec<_>>(),
        "profesionales": p.doctors.iter().map(|d| serde_json::json!({"id": d.id, "nombre": d.name, "especialidad": d.specialty})).collect::<Vec<_>>(),
    })
}

/// Los próximos días con horas libres a partir de `from` (como mucho `n` días con huecos).
pub fn days_json(p: &Published, held: &[Busy], now: NaiveDateTime, reason: usize, doctor: u32, from: NaiveDate, n: usize) -> serde_json::Value {
    days_with(p, now, from, n, |d| slots(p, held, now, reason, doctor, d))
}

/// Los próximos `n` días con horas según `f`.
pub fn days_with(p: &Published, now: NaiveDateTime, from: NaiveDate, n: usize, f: impl Fn(NaiveDate) -> Vec<(NaiveTime, u32)>) -> serde_json::Value {
    let mut days = vec![];
    let mut d = from.max(now.date());
    let last = now.date() + Duration::days(p.days_ahead as i64);
    let mut next = None;
    while d <= last {
        let s = f(d);
        if !s.is_empty() {
            if days.len() == n {
                next = Some(d);
                break;
            }
            days.push(serde_json::json!({
                "fecha": d.format("%Y-%m-%d").to_string(),
                "etiqueta": day_label(d),
                "corta": format!("{} {}", weekday_es(d).chars().take(3).collect::<String>(), d.day()),
                "horas": s.iter().map(|(t, doc)| serde_json::json!({"hora": t.format("%H:%M").to_string(), "profesional": doc})).collect::<Vec<_>>(),
            }));
        }
        d += Duration::days(1);
    }
    serde_json::json!({ "dias": days, "siguiente": next.map(|d| d.format("%Y-%m-%d").to_string()) })
}

fn check_body(b: &BookBody) -> Result<(), String> {
    if !b.acepta {
        return Err("Para pedir la cita hay que aceptar el tratamiento de sus datos.".into());
    }
    let n = b.nombre.trim().chars().count();
    if !(3..=80).contains(&n) {
        return Err("Escriba su nombre y apellidos.".into());
    }
    let d = digits(&b.telefono);
    if !(9..=15).contains(&d.len()) || b.telefono.chars().count() > 25 {
        return Err("El teléfono no parece correcto.".into());
    }
    let e = b.email.trim();
    if !e.is_empty() && (e.len() > 120 || !e.contains('@') || !e.rsplit('@').next().unwrap_or("").contains('.') || e.contains(char::is_whitespace)) {
        return Err("El correo electrónico no parece correcto.".into());
    }
    if b.notas.chars().count() > 500 {
        return Err("El comentario es demasiado largo (500 caracteres como mucho).".into());
    }
    Ok(())
}

/// Procesa una solicitud de cita: comprueba los datos y que la hora siga libre, y la reserva.
fn book(sh: &mut Shared, now: NaiveDateTime, b: &BookBody) -> Result<(BookingRequest, Busy), (u16, String)> {
    check_body(b).map_err(|e| (400, e))?;
    let p = &sh.published;
    if !p.accepting {
        return Err((503, "En este momento no se pueden pedir citas por internet. Llámenos, por favor.".into()));
    }
    let r = p.reasons.get(b.motivo).ok_or((400, "Elija el motivo de la consulta.".to_string()))?;
    let date = NaiveDate::parse_from_str(&b.fecha, "%Y-%m-%d").map_err(|_| (400, "Elija el día.".to_string()))?;
    let time = NaiveTime::parse_from_str(&b.hora, "%H:%M").map_err(|_| (400, "Elija la hora.".to_string()))?;
    let free = slots(p, &sh.held, now, b.motivo, b.profesional, date);
    let Some((_, doctor)) = free.iter().find(|(t, _)| *t == time) else {
        return Err((409, "Lo sentimos, esa hora acaba de ocuparse. Elija otra, por favor.".into()));
    };
    if sh.held.len() > 300 {
        return Err((503, "Hay muchas solicitudes pendientes. Llámenos, por favor.".into()));
    }
    let hold = (*doctor, date, mins(time), mins(time) + r.minutes as i64);
    let req = BookingRequest {
        id: 0,
        received: now,
        name: b.nombre.trim().to_string(),
        phone: b.telefono.trim().to_string(),
        email: b.email.trim().to_string(),
        reason: r.name.clone(),
        minutes: r.minutes,
        code: String::new(),
        doctor_id: *doctor,
        date,
        start: time,
        notes: b.notas.trim().to_string(),
        first_visit: b.primera,
        patient_id: None,
        state: ReqState::Pendiente,
        appointment_id: None,
        answer: String::new(),
        center_id: 0,
        replaces: None,
    };
    Ok((req, hold))
}

fn read_body(req: &mut tiny_http::Request) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    if req.as_reader().take(MAX_BODY + 1).read_to_end(&mut body).is_err() || body.len() as u64 > MAX_BODY {
        return None;
    }
    Some(body)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ActionBody {
    motivo: String,
    fecha: String,
    hora: String,
}

/// Enlace personal de una cita: `/mi-cita/{código}` (la página) y `/api/cita/{código}[/acción]`.
#[allow(clippy::too_many_arguments)]
fn portal(mut req: tiny_http::Request, sh: &Mutex<Shared>, tx: &Sender<WebEvent>, wake: &dyn Fn(), ip: &str, now: NaiveDateTime, path: &str, url: &str) {
    use crate::portal::{can_modify, change_slots, health_from, link_json, valid_token};
    use serde_json::json;
    let lock = || sh.lock().unwrap_or_else(|e| e.into_inner());
    if path.starts_with("/mi-cita/") {
        let g = lock();
        let html = PORTAL.replace("{{CLINICA}}", &esc_html(&g.published.clinic));
        drop(g);
        return respond(req, 200, "text/html; charset=utf-8", html);
    }
    let rest = path.trim_start_matches("/api/cita/");
    let (token, action) = rest.split_once('/').unwrap_or((rest, ""));
    let method = req.method().clone();
    let mut g = lock();
    let link = if valid_token(token) && g.published.accepting { g.published.links.get(token).cloned() } else { None };
    let Some(link) = link else {
        // muchos códigos equivocados desde la misma dirección: se le hace esperar
        let blocked = too_many(&mut g, &format!("bad:{ip}"), 20, 3600);
        g.refused += 1;
        drop(g);
        return json(req, if blocked { 429 } else { 404 }, json!({"ok": false, "error": "Este enlace no es válido o ya ha caducado. Llame a la clínica, por favor."}));
    };
    let fail = |req: tiny_http::Request, code: u16, e: &str| json(req, code, json!({"ok": false, "error": e}));
    match (method, action) {
        (tiny_http::Method::Get, "") => {
            let v = link_json(&g.published, &link, now);
            drop(g);
            json(req, 200, v);
        }
        (tiny_http::Method::Get, "cita.ics") => {
            let p = &g.published;
            let body = ics_event(link.appt, &p.clinic, &p.address, &p.phone, link.date, link.start, link.minutes, &format!("{} · {}", link.reason, link.doctor_name));
            drop(g);
            let mut r = tiny_http::Response::from_string(body);
            for (k, v) in [("Content-Type", "text/calendar; charset=utf-8"), ("Content-Disposition", "attachment; filename=\"cita.ics\""), ("Cache-Control", "no-store")] {
                if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
                    r = r.with_header(h);
                }
            }
            let _ = req.respond(r);
        }
        (tiny_http::Method::Get, "huecos") => {
            let from = query(url, "desde").and_then(|v| NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok()).unwrap_or(now.date());
            let v = days_with(&g.published, now, from, 5, |d| change_slots(&g.published, &g.held, now, &link, d));
            drop(g);
            json(req, 200, v);
        }
        (tiny_http::Method::Post, act @ ("confirmar" | "anular" | "cambiar" | "salud")) => {
            if too_many(&mut g, &format!("act:{token}"), 20, 3600) {
                drop(g);
                return fail(req, 429, "Demasiados intentos. Llame a la clínica, por favor.");
            }
            drop(g);
            let Some(body) = read_body(&mut req) else { return fail(req, 413, "Solicitud demasiado grande.") };
            let mut g = lock();
            let p = &g.published;
            let past = link.date.and_time(link.start) < now;
            let event = match act {
                "confirmar" => {
                    if link.state == "confirmada" {
                        drop(g);
                        return json(req, 200, json!({"ok": true, "mensaje": "Su cita ya estaba confirmada. ¡Le esperamos!"}));
                    }
                    if past || link.state != "planificada" {
                        drop(g);
                        return fail(req, 409, "Esta cita ya no se puede confirmar.");
                    }
                    if let Some(l) = g.published.links.get_mut(token) {
                        l.state = "confirmada";
                    }
                    ApptAction::Confirm
                }
                "anular" => {
                    if !can_modify(p, &link, now) {
                        let msg = format!("Quedan menos de {} horas para la cita: para anularla, llame al {}.", p.cancel_hours, p.phone);
                        drop(g);
                        return fail(req, 409, &msg);
                    }
                    let b: ActionBody = serde_json::from_slice(&body).unwrap_or_default();
                    if let Some(l) = g.published.links.get_mut(token) {
                        l.state = "anulada";
                    }
                    // la hora queda libre en la página
                    let own: Busy = (link.doctor, link.date, mins(link.start), mins(link.start) + link.minutes as i64);
                    g.published.busy.retain(|b| *b != own);
                    ApptAction::Cancel(b.motivo.trim().chars().take(300).collect())
                }
                "cambiar" => {
                    if !can_modify(p, &link, now) || link.change_pending {
                        let msg = if link.change_pending {
                            "Ya ha pedido un cambio para esta cita: le avisaremos en cuanto lo revisemos.".to_string()
                        } else {
                            format!("Quedan menos de {} horas para la cita: para cambiarla, llame al {}.", p.cancel_hours, p.phone)
                        };
                        drop(g);
                        return fail(req, 409, &msg);
                    }
                    let b: ActionBody = serde_json::from_slice(&body).unwrap_or_default();
                    let (Ok(date), Ok(time)) = (NaiveDate::parse_from_str(&b.fecha, "%Y-%m-%d"), NaiveTime::parse_from_str(&b.hora, "%H:%M")) else {
                        drop(g);
                        return fail(req, 400, "Elija el día y la hora.");
                    };
                    let free = change_slots(p, &g.held, now, &link, date);
                    let Some((_, doctor)) = free.iter().find(|(t, _)| *t == time).copied() else {
                        drop(g);
                        return fail(req, 409, "Lo sentimos, esa hora acaba de ocuparse. Elija otra, por favor.");
                    };
                    let hold = (doctor, date, mins(time), mins(time) + link.minutes as i64);
                    g.held.push(hold);
                    if let Some(l) = g.published.links.get_mut(token) {
                        l.change_pending = true;
                    }
                    let request = BookingRequest {
                        id: 0,
                        received: now,
                        name: link.first_name.clone(),
                        phone: String::new(),
                        email: String::new(),
                        reason: link.reason.clone(),
                        minutes: link.minutes,
                        code: String::new(),
                        doctor_id: doctor,
                        date,
                        start: time,
                        notes: String::new(),
                        first_visit: false,
                        patient_id: None,
                        state: ReqState::Pendiente,
                        appointment_id: None,
                        answer: String::new(),
                        center_id: 0,
                        replaces: Some(link.appt),
                    };
                    ApptAction::Change(Incoming { request, hold })
                }
                _ => {
                    if past || link.state == "anulada" || link.health_sent {
                        drop(g);
                        return fail(req, 409, if link.health_sent { "Ya hemos recibido su cuestionario. ¡Gracias!" } else { "Esta cita ya no admite el cuestionario." });
                    }
                    let Ok(b) = serde_json::from_slice::<crate::portal::HealthBody>(&body) else {
                        drop(g);
                        return fail(req, 400, "Cuestionario no válido.");
                    };
                    let form = match health_from(&b) {
                        Ok(f) => f,
                        Err(e) => {
                            drop(g);
                            return fail(req, 400, &e);
                        }
                    };
                    if let Some(l) = g.published.links.get_mut(token) {
                        l.health_sent = true;
                    }
                    ApptAction::Health(form)
                }
            };
            let auto = g.published.auto_confirm;
            g.received += 1;
            drop(g);
            let msg = match &event {
                ApptAction::Confirm => format!("¡Gracias, {}! Su cita queda confirmada. Le esperamos.", link.first_name),
                ApptAction::Cancel(_) => "Su cita ha quedado anulada. Si quiere otra, puede pedirla cuando quiera.".into(),
                ApptAction::Change(_) if auto => "Su cita se ha cambiado a la nueva hora.".into(),
                ApptAction::Change(_) => "Hemos recibido su petición de cambio. Le confirmaremos la nueva hora lo antes posible; mientras tanto, su cita sigue como estaba.".into(),
                ApptAction::Health(_) => "¡Gracias! Hemos recibido su cuestionario. Lo revisaremos con usted en la consulta.".into(),
            };
            if tx.send(WebEvent::Appt { appt: link.appt, action: event }).is_err() {
                return fail(req, 503, "No se pudo guardar. Llámenos, por favor.");
            }
            wake();
            json(req, 200, json!({"ok": true, "mensaje": msg}));
        }
        _ => {
            drop(g);
            fail(req, 405, "Método no permitido.");
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TextBody {
    texto: String,
}

/// Hueco libre para la lista de espera: `/hueco/{código}` (la página) y `/api/hueco/{código}[/reservar|/no-me-viene-bien]`.
/// El primero que lo reserva se lo queda: en cuanto uno lo pide, el hueco sale como ocupado para todos los demás.
#[allow(clippy::too_many_arguments)]
fn offer_portal(mut req: tiny_http::Request, sh: &Mutex<Shared>, tx: &Sender<WebEvent>, wake: &dyn Fn(), ip: &str, now: NaiveDateTime, path: &str) {
    use crate::slot_offer::{OfferAction, offer_json};
    use serde_json::json;
    let lock = || sh.lock().unwrap_or_else(|e| e.into_inner());
    if path.starts_with("/hueco/") {
        let g = lock();
        let html = OFFER_PAGE.replace("{{CLINICA}}", &esc_html(&g.published.clinic));
        drop(g);
        return respond(req, 200, "text/html; charset=utf-8", html);
    }
    let rest = path.trim_start_matches("/api/hueco/");
    let (token, action) = rest.split_once('/').unwrap_or((rest, ""));
    let method = req.method().clone();
    let mut g = lock();
    let found = if crate::portal::valid_token(token) && g.published.accepting { g.published.offers.get(token).cloned() } else { None };
    let Some(o) = found else {
        let blocked = too_many(&mut g, &format!("bad:{ip}"), 20, 3600);
        g.refused += 1;
        drop(g);
        return json(req, if blocked { 429 } else { 404 }, json!({"ok": false, "error": "Este enlace no es válido o ya ha caducado. Llame a la clínica, por favor."}));
    };
    let fail = |req: tiny_http::Request, code: u16, e: &str| json(req, code, json!({"ok": false, "error": e}));
    let passed = match (o.date, o.start) {
        (Some(d), Some(s)) => d.and_time(s) <= now,
        _ => true,
    };
    match (method, action) {
        (tiny_http::Method::Get, "") => {
            let mut v = offer_json(&g.published, &o);
            if passed && o.state == "abierta" {
                v["estado"] = json!("cerrada");
            }
            let first_view = !o.viewed;
            if let Some(x) = g.published.offers.get_mut(token) {
                x.viewed = true;
            }
            drop(g);
            if first_view {
                let _ = tx.send(WebEvent::Offer { token: token.to_string(), action: OfferAction::Viewed });
                wake();
            }
            json(req, 200, v);
        }
        (tiny_http::Method::Post, "reservar") => {
            if too_many(&mut g, &format!("off:{token}"), 10, 3600) {
                drop(g);
                return fail(req, 429, "Demasiados intentos. Llame a la clínica, por favor.");
            }
            match o.state {
                "suya" => {
                    drop(g);
                    return json(req, 200, json!({"ok": true, "mensaje": "Ya tenía este hueco reservado. ¡Le esperamos!"}));
                }
                "abierta" if !passed => {}
                _ => {
                    drop(g);
                    return fail(req, 409, "Lo sentimos, este hueco ya se ha ocupado. Seguirá en la lista de espera y le avisaremos del próximo.");
                }
            }
            // el primero se lo queda: para los demás invitados, ocupado
            for x in g.published.offers.values_mut().filter(|x| x.offer_id == o.offer_id) {
                x.state = "ocupada";
            }
            if let Some(x) = g.published.offers.get_mut(token) {
                x.state = "suya";
            }
            g.received += 1;
            drop(g);
            let _ = read_body(&mut req);
            if tx.send(WebEvent::Offer { token: token.to_string(), action: OfferAction::Take }).is_err() {
                return fail(req, 503, "No se pudo guardar. Llámenos, por favor.");
            }
            wake();
            json(req, 200, json!({"ok": true, "mensaje": format!("¡Hecho, {}! El hueco es suyo: su cita queda reservada. Le enviaremos la confirmación.", o.first_name)}));
        }
        (tiny_http::Method::Post, "no-me-viene-bien") => {
            drop(g);
            let _ = read_body(&mut req);
            let _ = tx.send(WebEvent::Offer { token: token.to_string(), action: OfferAction::Decline });
            wake();
            json(req, 200, json!({"ok": true, "mensaje": "De acuerdo. Seguirá en la lista de espera y le avisaremos del próximo hueco."}));
        }
        (tiny_http::Method::Get, "cita.ics") if o.state == "suya" => {
            let p = &g.published;
            let (Some(d), Some(s)) = (o.date, o.start) else {
                drop(g);
                return fail(req, 404, "No existe.");
            };
            let body = ics_event(o.offer_id, &p.clinic, &p.address, &p.phone, d, s, o.minutes, &format!("Cita con {}", o.doctor));
            drop(g);
            let mut r = tiny_http::Response::from_string(body);
            for (k, v) in [("Content-Type", "text/calendar; charset=utf-8"), ("Content-Disposition", "attachment; filename=\"cita.ics\""), ("Cache-Control", "no-store")] {
                if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
                    r = r.with_header(h);
                }
            }
            let _ = req.respond(r);
        }
        _ => {
            drop(g);
            fail(req, 405, "Método no permitido.");
        }
    }
}

/// Documento para firmar: `/documento/{código}` (la página) y `/api/documento/{código}[/firmar|/en-clinica|/consulta]`.
fn doc_portal(mut req: tiny_http::Request, sh: &Mutex<Shared>, tx: &Sender<WebEvent>, wake: &dyn Fn(), ip: &str, path: &str) {
    use crate::doc_link::{DocAction, SignBody, doc_json};
    use serde_json::json;
    let lock = || sh.lock().unwrap_or_else(|e| e.into_inner());
    if path.starts_with("/documento/") {
        let g = lock();
        let html = DOC_PAGE.replace("{{CLINICA}}", &esc_html(&g.published.clinic));
        drop(g);
        return respond(req, 200, "text/html; charset=utf-8", html);
    }
    let rest = path.trim_start_matches("/api/documento/");
    let (token, action) = rest.split_once('/').unwrap_or((rest, ""));
    let method = req.method().clone();
    let mut g = lock();
    let found = if crate::portal::valid_token(token) && g.published.accepting { g.published.docs.get(token).cloned() } else { None };
    let Some(d) = found else {
        let blocked = too_many(&mut g, &format!("bad:{ip}"), 20, 3600);
        g.refused += 1;
        drop(g);
        return json(req, if blocked { 429 } else { 404 }, json!({"ok": false, "error": "Este enlace no es válido o ya ha caducado. Llame a la clínica, por favor."}));
    };
    let fail = |req: tiny_http::Request, code: u16, e: &str| json(req, code, json!({"ok": false, "error": e}));
    match (method, action) {
        (tiny_http::Method::Get, "") => {
            let v = doc_json(&g.published, &d);
            let first_view = !d.viewed;
            if let Some(x) = g.published.docs.get_mut(token) {
                x.viewed = true;
            }
            drop(g);
            if first_view {
                let _ = tx.send(WebEvent::Doc { doc: d.doc_id, action: DocAction::Viewed });
                wake();
            }
            json(req, 200, v);
        }
        (tiny_http::Method::Post, act @ ("firmar" | "en-clinica" | "consulta")) => {
            if too_many(&mut g, &format!("doc:{token}"), 15, 3600) {
                drop(g);
                return fail(req, 429, "Demasiados intentos. Llame a la clínica, por favor.");
            }
            drop(g);
            let Some(body) = read_body(&mut req) else { return fail(req, 413, "Solicitud demasiado grande.") };
            let (action, msg) = match act {
                "firmar" => {
                    if d.state == "firmado" {
                        return fail(req, 409, "Este documento ya está firmado.");
                    }
                    let Ok(s) = serde_json::from_slice::<SignBody>(&body) else { return fail(req, 400, "Solicitud no válida.") };
                    if !s.acepta {
                        return fail(req, 400, "Marque la casilla para firmar el documento.");
                    }
                    let name = s.nombre.trim();
                    if !(3..=80).contains(&name.chars().count()) {
                        return fail(req, 400, "Escriba el nombre y apellidos de quien firma.");
                    }
                    if s.nif.chars().count() > 20 {
                        return fail(req, 400, "El NIF no parece correcto.");
                    }
                    if s.huella != d.hash {
                        return fail(req, 409, "El documento ha cambiado mientras lo leía: vuelva a abrir el enlace, por favor.");
                    }
                    let strokes = match crate::budget_link::clean_strokes(&s.firma) {
                        Ok(x) => x,
                        Err(e) => return fail(req, 400, &e),
                    };
                    let mut g = lock();
                    if let Some(x) = g.published.docs.get_mut(token) {
                        x.state = "firmado";
                        x.signed_by = name.to_string();
                        x.signed_at = Some(chrono::Local::now().naive_local());
                    }
                    g.received += 1;
                    drop(g);
                    (DocAction::Sign { signer: name.to_string(), nif: s.nif.trim().to_string(), strokes, hash: s.huella, ip: ip.to_string() }, format!("¡Gracias, {}! Documento firmado. Le enviaremos una copia firmada.", d.first_name))
                }
                "en-clinica" => {
                    if d.state == "firmado" {
                        return fail(req, 409, "Este documento ya está firmado.");
                    }
                    let t: TextBody = serde_json::from_slice(&body).unwrap_or_default();
                    let mut g = lock();
                    if let Some(x) = g.published.docs.get_mut(token) {
                        x.state = "en_clinica";
                    }
                    drop(g);
                    (DocAction::Decline(t.texto.trim().chars().take(300).collect()), "De acuerdo: lo firmará en la clínica, donde podrá resolver sus dudas con el profesional.".into())
                }
                _ => {
                    let t: TextBody = serde_json::from_slice(&body).unwrap_or_default();
                    let text: String = t.texto.trim().chars().take(600).collect();
                    if text.is_empty() {
                        return fail(req, 400, "Escriba su pregunta.");
                    }
                    (DocAction::Question(text), "Hemos recibido su pregunta. Le contestaremos lo antes posible.".into())
                }
            };
            if tx.send(WebEvent::Doc { doc: d.doc_id, action }).is_err() {
                return fail(req, 503, "No se pudo guardar. Llámenos, por favor.");
            }
            wake();
            json(req, 200, json!({"ok": true, "mensaje": msg}));
        }
        _ => {
            drop(g);
            fail(req, 405, "Método no permitido.");
        }
    }
}

/// ¿Ha llegado `key` al máximo de intentos de la ventana? (sin apuntar uno más)
fn at_limit(sh: &mut Shared, key: &str, max: usize, secs: u64) -> bool {
    let now = Instant::now();
    sh.hits.get_mut(key).is_some_and(|v| {
        v.retain(|t| now.duration_since(*t).as_secs() < secs);
        v.len() >= max
    })
}

/// Fallos seguidos que bloquean un enlace de «Mis recetas», y durante cuánto.
pub const RX_MAX_FAILS: usize = 5;
const RX_LOCK_SECS: u64 = 30 * 60;

/// «Mis recetas»: `/recetas/{código}` (la página, sin datos), `GET /api/recetas/{código}`
/// (solo la clínica y si se puede entrar) y `POST /api/recetas/{código}[/hoja/{n}]` con el
/// DNI/NIE y la fecha de nacimiento: las recetas, o la hoja para la farmacia.
fn rx_portal(mut req: tiny_http::Request, sh: &Mutex<Shared>, tx: &Sender<WebEvent>, wake: &dyn Fn(), ip: &str, now: NaiveDateTime, path: &str) {
    use serde_json::json;
    let lock = || sh.lock().unwrap_or_else(|e| e.into_inner());
    if path.starts_with("/recetas/") {
        let g = lock();
        let html = RX_PAGE.replace("{{CLINICA}}", &esc_html(&g.published.clinic));
        drop(g);
        return respond(req, 200, "text/html; charset=utf-8", html);
    }
    let rest = path.trim_start_matches("/api/recetas/");
    let (token, action) = rest.split_once('/').unwrap_or((rest, ""));
    let method = req.method().clone();
    let mut g = lock();
    let found = if crate::portal::valid_token(token) && g.published.accepting { g.published.rx.get(token).cloned() } else { None };
    let Some(d) = found else {
        let blocked = too_many(&mut g, &format!("bad:{ip}"), 20, 3600);
        g.refused += 1;
        drop(g);
        return json(req, if blocked { 429 } else { 404 }, json!({"ok": false, "error": "Este enlace no es válido o ya ha caducado. Llame a la clínica, por favor."}));
    };
    let fail = |req: tiny_http::Request, code: u16, e: &str| json(req, code, json!({"ok": false, "error": e}));
    match method {
        tiny_http::Method::Get if action.is_empty() => {
            let p = &g.published;
            let v = json!({"ok": true, "clinica": p.clinic, "direccion": p.address, "telefono": p.phone, "verificable": !d.check.is_empty()});
            drop(g);
            json(req, 200, v);
        }
        tiny_http::Method::Post if action.is_empty() || action.starts_with("hoja/") => {
            let fails = format!("rxfail:{token}");
            if at_limit(&mut g, &fails, RX_MAX_FAILS, RX_LOCK_SECS) || too_many(&mut g, &format!("rxip:{ip}"), 60, 3600) {
                drop(g);
                return fail(req, 429, "Por seguridad, este enlace está bloqueado durante media hora por demasiados intentos. Vuelva a probar más tarde o llame a la clínica.");
            }
            drop(g);
            let Some(body) = read_body(&mut req) else { return fail(req, 413, "Solicitud demasiado grande.") };
            let login: crate::rx_link::RxLogin = serde_json::from_slice(&body).unwrap_or_default();
            if !login.matches(token, &d) {
                let mut g = lock();
                too_many(&mut g, &fails, RX_MAX_FAILS + 1, RX_LOCK_SECS);
                let left = RX_MAX_FAILS.saturating_sub(g.hits.get(&fails).map(|v| v.len()).unwrap_or(0));
                g.refused += 1;
                drop(g);
                let msg = if d.check.is_empty() {
                    "Para ver sus recetas por internet la clínica necesita su DNI/NIE y su fecha de nacimiento en su ficha. Llámenos, por favor.".to_string()
                } else if left == 0 {
                    "Los datos no coinciden. Por seguridad, el enlace queda bloqueado durante media hora.".to_string()
                } else {
                    format!("Los datos no coinciden con los de su ficha: revise el DNI/NIE y la fecha de nacimiento (le quedan {left} intentos).")
                };
                return fail(req, 403, &msg);
            }
            if action.is_empty() {
                let g = lock();
                let v = crate::rx_link::rx_json(&g.published, &d, now.date());
                drop(g);
                if tx.send(WebEvent::Rx { patient: d.patient_id }).is_ok() {
                    wake();
                }
                json(req, 200, v);
            } else {
                let i = action.trim_start_matches("hoja/").parse::<usize>().ok();
                let g = lock();
                let sheet = i.and_then(|i| crate::rx_link::rx_sheet(&g.published, &d, i));
                drop(g);
                match sheet {
                    Some(html) => json(req, 200, json!({"ok": true, "html": html})),
                    None => fail(req, 404, "Esa receta no existe."),
                }
            }
        }
        _ => {
            drop(g);
            fail(req, 405, "Método no permitido.");
        }
    }
}

/// Pago por internet (ver `pago`):
/// - `GET /pagar/{código}`: la página, con lo que se paga y cómo va;
/// - `POST /api/pagar/{código}/iniciar` (`metodo=tarjeta|bizum`): pedido nuevo y formulario firmado hacia Redsys;
/// - `POST /api/tpv/notificacion`: la notificación de Redsys, que solo cuenta con la firma bien.
fn pay_portal(mut req: tiny_http::Request, sh: &Mutex<Shared>, tx: &Sender<WebEvent>, wake: &dyn Fn(), ip: &str, path: &str, url: &str) {
    use crate::pago::{Method, PayState, parse_form, pay_form, pay_page, read_notice};
    let lock = || sh.lock().unwrap_or_else(|e| e.into_inner());
    let text = |req: tiny_http::Request, code: u16, t: &str| respond(req, code, "text/plain; charset=utf-8", t.to_string());
    if path == "/api/tpv/notificacion" {
        if *req.method() != tiny_http::Method::Post {
            return text(req, 405, "Método no permitido.");
        }
        let Some(body) = read_body(&mut req) else { return text(req, 413, "Demasiado grande.") };
        let f = parse_form(&String::from_utf8_lossy(&body));
        let s = lock().tpv.clone();
        let got = s
            .ok_or_else(|| "Sin TPV configurado.".to_string())
            .and_then(|s| read_notice(&s, f.get("Ds_SignatureVersion").map(String::as_str).unwrap_or(""), f.get("Ds_MerchantParameters").map(String::as_str).unwrap_or(""), f.get("Ds_Signature").map(String::as_str).unwrap_or("")));
        return match got {
            Ok(n) => {
                if tx.send(WebEvent::PayNotice(n)).is_err() {
                    return text(req, 503, "No se pudo guardar.");
                }
                wake();
                text(req, 200, "OK")
            }
            Err(e) => {
                let mut g = lock();
                g.refused += 1;
                too_many(&mut g, &format!("bad:{ip}"), 20, 3600);
                drop(g);
                text(req, 400, &e)
            }
        };
    }
    let rest = path.trim_start_matches("/api/pagar/").trim_start_matches("/pagar/");
    let (token, action) = rest.split_once('/').unwrap_or((rest, ""));
    let mut g = lock();
    let found = if crate::portal::valid_token(token) && g.published.accepting { g.published.pays.get(token).cloned() } else { None };
    let Some(p) = found else {
        let blocked = too_many(&mut g, &format!("bad:{ip}"), 20, 3600);
        g.refused += 1;
        drop(g);
        return text(req, if blocked { 429 } else { 404 }, "Este enlace no es válido o ya ha caducado. Llame a la clínica, por favor.");
    };
    let public = g.published.public_url.clone();
    let tpv = g.tpv.clone().filter(|s| s.problem().is_none() && public.starts_with("http"));
    let clinic = g.published.clinic.clone();
    drop(g);
    match (req.method().clone(), path.starts_with("/pagar/"), action) {
        (tiny_http::Method::Get, true, "") => {
            let html = pay_page(&clinic, token, &p, query(url, "resultado").as_deref(), tpv.is_some());
            respond(req, 200, "text/html; charset=utf-8", html);
        }
        (tiny_http::Method::Post, false, "iniciar") => {
            let Some(s) = tpv else { return text(req, 503, "El pago por internet no está disponible ahora. Llame a la clínica, por favor.") };
            if p.state != PayState::Pendiente {
                return text(req, 409, "Este pago ya no está pendiente.");
            }
            let Some(body) = read_body(&mut req) else { return text(req, 413, "Demasiado grande.") };
            let method = match parse_form(&String::from_utf8_lossy(&body)).get("metodo").map(String::as_str) {
                Some("bizum") if p.bizum => Method::Bizum,
                Some("tarjeta") => Method::Tarjeta,
                _ => return text(req, 400, "Forma de pago no válida."),
            };
            {
                let mut g = lock();
                if too_many(&mut g, &format!("pay:{token}"), 10, 3600) {
                    g.refused += 1;
                    drop(g);
                    return text(req, 429, "Demasiados intentos de pago. Llame a la clínica, por favor.");
                }
            }
            let order = crate::pago::new_order(p.id);
            // el importe es el de la petición, nunca uno que mande el navegador
            match pay_form(&s, &order, p.amount, method, &format!("{public}/api/tpv/notificacion"), &format!("{public}/pagar/{token}"), &p.concept) {
                Ok(form) => {
                    if tx.send(WebEvent::PayStarted { token: token.to_string(), order, method }).is_err() {
                        return text(req, 503, "No se pudo preparar el pago. Llame a la clínica, por favor.");
                    }
                    wake();
                    respond(req, 200, "text/html; charset=utf-8", form.html());
                }
                Err(e) => text(req, 503, &e),
            }
        }
        _ => text(req, 405, "Método no permitido."),
    }
}

/// Presupuesto para firmar: `/presupuesto/{código}` (la página) y `/api/presupuesto/{código}[/acción]`.
#[allow(clippy::too_many_arguments)]
fn budget_portal(mut req: tiny_http::Request, sh: &Mutex<Shared>, tx: &Sender<WebEvent>, wake: &dyn Fn(), ip: &str, now: NaiveDateTime, path: &str) {
    use crate::budget_link::{AcceptBody, BudgetAction, budget_json, clean_strokes};
    use serde_json::json;
    let lock = || sh.lock().unwrap_or_else(|e| e.into_inner());
    if path.starts_with("/presupuesto/") {
        let g = lock();
        let html = BUDGET_PAGE.replace("{{CLINICA}}", &esc_html(&g.published.clinic));
        drop(g);
        return respond(req, 200, "text/html; charset=utf-8", html);
    }
    let rest = path.trim_start_matches("/api/presupuesto/");
    let (token, action) = rest.split_once('/').unwrap_or((rest, ""));
    let method = req.method().clone();
    let mut g = lock();
    let found = if crate::portal::valid_token(token) && g.published.accepting { g.published.budgets.get(token).cloned() } else { None };
    let Some(b) = found else {
        let blocked = too_many(&mut g, &format!("bad:{ip}"), 20, 3600);
        g.refused += 1;
        drop(g);
        return json(req, if blocked { 429 } else { 404 }, json!({"ok": false, "error": "Este enlace no es válido o ya ha caducado. Llame a la clínica, por favor."}));
    };
    let fail = |req: tiny_http::Request, code: u16, e: &str| json(req, code, json!({"ok": false, "error": e}));
    match (method, action) {
        (tiny_http::Method::Get, "") => {
            let v = budget_json(&g.published, &b, now);
            let first_view = !b.viewed;
            if let Some(x) = g.published.budgets.get_mut(token) {
                x.viewed = true;
            }
            drop(g);
            if first_view {
                let _ = tx.send(WebEvent::Budget { budget: b.bid, action: BudgetAction::Viewed });
                wake();
            }
            json(req, 200, v);
        }
        (tiny_http::Method::Post, act @ ("aceptar" | "rechazar" | "consulta")) => {
            if too_many(&mut g, &format!("bud:{token}"), 15, 3600) {
                drop(g);
                return fail(req, 429, "Demasiados intentos. Llame a la clínica, por favor.");
            }
            drop(g);
            let Some(body) = read_body(&mut req) else { return fail(req, 413, "Solicitud demasiado grande.") };
            let pending = b.state == "pendiente" && b.expires >= now.date();
            let (action, msg) = match act {
                "aceptar" => {
                    if !pending {
                        return fail(req, 409, "Este presupuesto ya no se puede aceptar por internet. Llame a la clínica, por favor.");
                    }
                    let Ok(a) = serde_json::from_slice::<AcceptBody>(&body) else { return fail(req, 400, "Solicitud no válida.") };
                    if !a.acepta {
                        return fail(req, 400, "Marque la casilla para aceptar el presupuesto.");
                    }
                    let name = a.nombre.trim();
                    if !(3..=80).contains(&name.chars().count()) {
                        return fail(req, 400, "Escriba su nombre y apellidos.");
                    }
                    if a.nif.chars().count() > 20 {
                        return fail(req, 400, "El NIF no parece correcto.");
                    }
                    if a.huella != b.hash {
                        return fail(req, 409, "El presupuesto ha cambiado mientras lo revisaba: vuelva a abrir el enlace, por favor.");
                    }
                    if a.cuotas > 0 && !b.options.iter().any(|o| o.0 == a.cuotas) {
                        return fail(req, 400, "Esa forma de pago no está disponible.");
                    }
                    let strokes = match clean_strokes(&a.firma) {
                        Ok(s) => s,
                        Err(e) => return fail(req, 400, &e),
                    };
                    let mut g = lock();
                    if let Some(x) = g.published.budgets.get_mut(token) {
                        x.state = "aceptado";
                        x.signed_by = name.to_string();
                        x.signed_at = Some(now);
                    }
                    g.received += 1;
                    drop(g);
                    (
                        BudgetAction::Accept { signer: name.to_string(), nif: a.nif.trim().to_string(), strokes, installments: a.cuotas, hash: a.huella, ip: ip.to_string() },
                        format!("¡Gracias, {}! Presupuesto aceptado y firmado. Le enviaremos una copia firmada y le llamaremos para dar las citas.", b.first_name),
                    )
                }
                "rechazar" => {
                    if !pending {
                        return fail(req, 409, "Este presupuesto ya no está pendiente.");
                    }
                    let t: TextBody = serde_json::from_slice(&body).unwrap_or_default();
                    let mut g = lock();
                    if let Some(x) = g.published.budgets.get_mut(token) {
                        x.state = "rechazado";
                    }
                    drop(g);
                    (BudgetAction::Reject(t.texto.trim().chars().take(300).collect()), "Entendido. Hemos anotado que no le interesa por ahora. Si cambia de idea, llámenos.".into())
                }
                _ => {
                    let t: TextBody = serde_json::from_slice(&body).unwrap_or_default();
                    let text: String = t.texto.trim().chars().take(600).collect();
                    if text.is_empty() {
                        return fail(req, 400, "Escriba su pregunta.");
                    }
                    (BudgetAction::Question(text), "Hemos recibido su pregunta. Le contestaremos lo antes posible.".into())
                }
            };
            if tx.send(WebEvent::Budget { budget: b.bid, action }).is_err() {
                return fail(req, 503, "No se pudo guardar. Llámenos, por favor.");
            }
            wake();
            json(req, 200, json!({"ok": true, "mensaje": msg}));
        }
        _ => {
            drop(g);
            fail(req, 405, "Método no permitido.");
        }
    }
}

/// Abre la página de cita online en `port` (con `lan`, en toda la red local).
pub fn listen(port: u16, lan: bool, published: Published, wake: impl Fn() + Send + 'static) -> Result<BookingServer, String> {
    let host = if lan { "0.0.0.0" } else { "127.0.0.1" };
    let server = Arc::new(tiny_http::Server::http((host, port)).map_err(|e| format!("No se pudo abrir el puerto {port}: {e}"))?);
    let real = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(port);
    let (tx, rx) = channel();
    let shared = Arc::new(Mutex::new(Shared { published, ..Default::default() }));
    let (s, sh) = (server.clone(), shared.clone());
    std::thread::spawn(move || {
        for mut req in s.incoming_requests() {
            let url = req.url().to_string();
            let path = url.split('?').next().unwrap_or("").trim_end_matches('/').to_string();
            let ip = client_ip(&req);
            let method = req.method().clone();
            // aunque una petición fallara a medias, las demás siguen funcionando
            let mut g = sh.lock().unwrap_or_else(|e| e.into_inner());
            if too_many(&mut g, &format!("all:{ip}"), 240, 60) {
                g.refused += 1;
                drop(g);
                json(req, 429, serde_json::json!({"ok": false, "error": "Demasiadas peticiones. Espere un momento."}));
                continue;
            }
            let now = chrono::Local::now().naive_local();
            match (method, path.as_str()) {
                (tiny_http::Method::Get, "" | "/cita") => {
                    let html = PAGE.replace("{{CLINICA}}", &esc_html(&g.published.clinic));
                    drop(g);
                    respond(req, 200, "text/html; charset=utf-8", html);
                }
                (tiny_http::Method::Get, "/api/datos") => {
                    let v = info_json(&g.published);
                    drop(g);
                    json(req, 200, v);
                }
                (tiny_http::Method::Get, "/api/huecos") => {
                    let reason = query(&url, "motivo").and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
                    let doctor = query(&url, "profesional").and_then(|v| v.parse().ok()).unwrap_or(0);
                    let from = query(&url, "desde").and_then(|v| NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok()).unwrap_or(now.date());
                    let v = days_json(&g.published, &g.held, now, reason, doctor, from, 5);
                    drop(g);
                    json(req, 200, v);
                }
                (tiny_http::Method::Post, "/api/reservar") => {
                    if too_many(&mut g, &format!("book:{ip}"), 5, 3600) {
                        g.refused += 1;
                        drop(g);
                        json(req, 429, serde_json::json!({"ok": false, "error": "Ya ha enviado varias solicitudes. Si necesita otra cita, llámenos."}));
                        continue;
                    }
                    drop(g);
                    let mut body = Vec::new();
                    if req.as_reader().take(MAX_BODY + 1).read_to_end(&mut body).is_err() || body.len() as u64 > MAX_BODY {
                        json(req, 413, serde_json::json!({"ok": false, "error": "Solicitud demasiado grande."}));
                        continue;
                    }
                    let Ok(b) = serde_json::from_slice::<BookBody>(&body) else {
                        json(req, 400, serde_json::json!({"ok": false, "error": "Solicitud no válida."}));
                        continue;
                    };
                    let mut g = sh.lock().unwrap_or_else(|e| e.into_inner());
                    if !b.web.is_empty() {
                        // robot: se le contesta como si nada, pero no se guarda
                        g.refused += 1;
                        drop(g);
                        json(req, 200, serde_json::json!({"ok": true, "estado": "pendiente", "mensaje": "Solicitud recibida."}));
                        continue;
                    }
                    match book(&mut g, now, &b) {
                        Ok((request, hold)) => {
                            let auto = g.published.auto_confirm;
                            g.held.push(hold);
                            g.received += 1;
                            drop(g);
                            let sent = tx.send(WebEvent::Booking(Incoming { request, hold })).is_ok();
                            wake();
                            if sent {
                                let msg = if auto { "Su cita ha quedado reservada." } else { "Hemos recibido su solicitud. Le confirmaremos la cita lo antes posible." };
                                json(req, 200, serde_json::json!({"ok": true, "estado": if auto { "confirmada" } else { "pendiente" }, "mensaje": msg}));
                            } else {
                                json(req, 503, serde_json::json!({"ok": false, "error": "No se pudo guardar la solicitud. Llámenos, por favor."}));
                            }
                        }
                        Err((code, e)) => {
                            g.refused += 1;
                            drop(g);
                            json(req, code, serde_json::json!({"ok": false, "error": e}));
                        }
                    }
                }
                (_, x) if x.starts_with("/mi-cita/") || x.starts_with("/api/cita/") => {
                    drop(g);
                    portal(req, &sh, &tx, &wake, &ip, now, &path, &url);
                }
                (_, x) if x.starts_with("/hueco/") || x.starts_with("/api/hueco/") => {
                    drop(g);
                    offer_portal(req, &sh, &tx, &wake, &ip, now, &path);
                }
                (_, x) if x.starts_with("/documento/") || x.starts_with("/api/documento/") => {
                    drop(g);
                    doc_portal(req, &sh, &tx, &wake, &ip, &path);
                }
                (_, x) if x.starts_with("/recetas/") || x.starts_with("/api/recetas/") => {
                    drop(g);
                    rx_portal(req, &sh, &tx, &wake, &ip, now, &path);
                }
                (_, x) if x.starts_with("/presupuesto/") || x.starts_with("/api/presupuesto/") => {
                    drop(g);
                    budget_portal(req, &sh, &tx, &wake, &ip, now, &path);
                }
                (_, x) if x.starts_with("/pagar/") || x.starts_with("/api/pagar/") || x == "/api/tpv/notificacion" => {
                    drop(g);
                    pay_portal(req, &sh, &tx, &wake, &ip, &path, &url);
                }
                (_, "/api/reservar" | "/api/datos" | "/api/huecos") => {
                    drop(g);
                    json(req, 405, serde_json::json!({"ok": false, "error": "Método no permitido."}));
                }
                _ => {
                    drop(g);
                    respond(req, 404, "text/plain; charset=utf-8", "No existe.".into());
                }
            }
        }
    });
    Ok(BookingServer { port: real, rx, shared, server })
}

/// Datos de ejemplo: motivos con sus profesionales y dos solicitudes por confirmar.
pub fn sample_booking(db: &mut Db, today: NaiveDate) {
    db.booking.reasons = vec![
        reason("Primera visita y revisión", 30, "0101", &[1, 2]),
        reason("Limpieza dental (higiene)", 45, "0201", &[4]),
        reason("Urgencia o dolor", 30, "", &[1, 2]),
        reason("Revisión periódica", 20, "0102", &[1, 2]),
        reason("Consulta de ortodoncia", 30, "", &[3]),
        reason("Consulta de implantes o estética", 30, "", &[2]),
    ];
    db.booking.public_url = "https://citas.clinica-ejemplo.es".into();
    // un presupuesto enviado para firmar: el paciente lo abrió ayer y preguntó una duda
    if let Some(bid) = db.budgets.iter().find(|b| b.patient_id == 101 && b.state == BudgetState::Pendiente).map(|b| b.id) {
        db.budget_token(bid);
        let at = |h: u32, m: u32| (today - Duration::days(1)).and_hms_opt(h, m, 0).unwrap_or_default();
        if let Some(l) = db.budget_links.iter_mut().find(|l| l.budget_id == bid) {
            l.sent_to = "carlos.martinez@example.com".into();
            l.viewed = Some(at(21, 14));
            l.questions.push((at(21, 20), "¿La corona se puede pagar cuando me la pongan?".into()));
        }
    }
    // un paciente de hoy confirmó ayer su cita desde el enlace
    if let Some(a) = db.appointments.iter().find(|a| a.date == today && a.state == ApptState::SalaEspera).map(|a| a.id) {
        db.appt_token(a);
        if let Some(l) = db.appt_links.iter_mut().find(|l| l.appointment_id == a) {
            l.confirmed = Some((today - Duration::days(1)).and_hms_opt(19, 42, 0).unwrap_or_default());
        }
    }
    let demo = [("Laura Fernández Ortiz", "616 214 122", "laura.fernandez@example.com", 1usize, 4u32, ""), ("Raúl Gómez Peña", "644 812 330", "raul.gomez@example.com", 0, 1, "Me sangran las encías al cepillarme.")];
    let mut from = today + Duration::days(1);
    for (k, (name, phone, mail, ri, doc, notes)) in demo.into_iter().enumerate() {
        let r = db.booking.reasons[ri].clone();
        let Some((date, start)) = db.find_gaps(doc, from, r.minutes, at(10 * 60 + 30 * k as i64), db.config.close_time, 10).first().copied() else { continue };
        db.booking_receive(BookingRequest {
            id: 0,
            received: now() - Duration::minutes(40 + 95 * k as i64),
            name: name.into(),
            phone: phone.into(),
            email: mail.into(),
            reason: r.name.clone(),
            minutes: r.minutes,
            code: r.code.clone(),
            doctor_id: doc,
            date,
            start,
            notes: notes.into(),
            first_visit: k == 1,
            patient_id: None,
            state: ReqState::Pendiente,
            appointment_id: None,
            answer: String::new(),
            center_id: 1,
            replaces: None,
        });
        from = date;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monday_after(d: NaiveDate) -> NaiveDate {
        let mut x = d + Duration::days(1);
        while x.weekday().num_days_from_monday() != 0 {
            x += Duration::days(1);
        }
        x
    }

    #[test]
    fn free_slots_respect_agenda() {
        let today = today();
        let mut db = Db::sample(today);
        let day = monday_after(today) + Duration::days(7);
        db.holidays.retain(|h| h.date != day);
        db.booking.enabled = true;
        db.booking.min_notice_hours = 0;
        db.appointments.retain(|a| a.date != day);
        let ri = db.booking.reasons.iter().position(|r| r.name.starts_with("Limpieza")).unwrap();
        let p = publish(&db, today);
        let now = today.and_hms_opt(8, 0, 0).unwrap();
        let s = slots(&p, &[], now, ri, 0, day);
        assert!(!s.is_empty());
        assert!(s.iter().all(|(_, d)| *d == 4), "limpieza: sólo la higienista");
        // la comida (14:00-15:30) no se ofrece
        assert!(!s.iter().any(|(t, _)| mins(*t) + 45 > 14 * 60 && mins(*t) < 15 * 60 + 30));
        // una cita ocupa su hueco
        let first = s[0].0;
        let id = db.new_id();
        db.appointments.push(Appointment {
            id,
            date: day,
            start: first,
            minutes: 45,
            patient_id: Some(100),
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
        let p = publish(&db, today);
        assert!(!slots(&p, &[], now, ri, 0, day).iter().any(|(t, _)| *t == first));
        // ni fines de semana, ni festivos, ni más allá del plazo
        assert!(slots(&p, &[], now, ri, 0, day + Duration::days(5)).is_empty());
        db.holidays.push(crate::agenda_extra::Holiday { date: day, name: "Fiesta".into() });
        assert!(slots(&publish(&db, today), &[], now, ri, 0, day).is_empty());
        assert!(slots(&p, &[], now, ri, 0, today + Duration::days(200)).is_empty());
    }

    #[test]
    fn online_slots_follow_the_clinic_and_doctor_hours() {
        use crate::horario::{Shift, Week};
        let today = today();
        let mut db = Db::sample(today);
        let mon = monday_after(today) + Duration::days(7);
        let (wed, sat) = (mon + Duration::days(2), mon + Duration::days(5));
        db.holidays.retain(|h| h.date < mon || h.date > sat);
        db.booking.enabled = true;
        db.booking.min_notice_hours = 0;
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        // la clínica abre también el sábado por la mañana; el Dr. Navarro (2) solo los miércoles por la tarde
        let mut w = Week::simple(t(9, 0), t(20, 0), false);
        w.0[5] = vec![Shift::new(t(9, 0), t(14, 0))];
        db.config.hours = Some(w);
        let mut nav = Week::default();
        nav.0[2] = vec![Shift::new(t(16, 0), t(20, 0))];
        db.collaborators.iter_mut().find(|c| c.id == 2).unwrap().hours = Some(nav);
        let p = publish(&db, today);
        let now = today.and_hms_opt(8, 0, 0).unwrap();
        let ri = p.reasons.iter().position(|r| r.doctors.contains(&2)).expect("un motivo con el Dr. Navarro");
        // el sábado hay huecos, solo por la mañana
        let s = slots(&p, &[], now, ri, 0, sat);
        assert!(!s.is_empty() && s.iter().all(|(h, _)| *h < t(14, 0)), "{s:?}");
        // con el Dr. Navarro: el lunes nada; el miércoles, de 16:00 en adelante
        assert!(slots(&p, &[], now, ri, 2, mon).is_empty());
        let w = slots(&p, &[], now, ri, 2, wed);
        assert!(!w.is_empty() && w.iter().all(|(h, d)| *d == 2 && *h >= t(16, 0)), "{w:?}");
    }

    #[test]
    fn request_matches_patient_and_becomes_appointment() {
        let today = today();
        let mut db = Db::sample(today);
        let pid = db.patients[2].id;
        let phone = format!("+34 {}", db.patients[2].mobile);
        let day = monday_after(today) + Duration::days(14);
        db.holidays.retain(|h| h.date != day);
        let (_, start) = db.find_gaps(1, day, 30, db.config.open_time, db.config.close_time, 1)[0];
        let req = BookingRequest {
            id: 0,
            received: now(),
            name: "Laura".into(),
            phone,
            email: "laura@example.com".into(),
            reason: "Revisión".into(),
            minutes: 30,
            code: "0102".into(),
            doctor_id: 1,
            date: day,
            start,
            notes: "Prefiero por la mañana".into(),
            first_visit: false,
            patient_id: None,
            state: ReqState::Pendiente,
            appointment_id: None,
            answer: String::new(),
            center_id: 1,
            replaces: None,
        };
        // tenía pendiente su revisión: al confirmar la cita deja de recibir el «pida su cita»
        db.appointments.retain(|a| a.patient_id != Some(pid) || a.date < today);
        db.recalls.retain(|r| r.patient_id != pid);
        let rid = db.new_id();
        db.recalls.push(Recall { id: rid, patient_id: pid, date: today, reason: "Revisión semestral".into(), doctor_id: 1, state: RecallState::Pendiente, channel: String::new(), comment: String::new(), contacted_on: None, appt_id: None });
        let id = db.booking_receive(req);
        assert_eq!(db.booking_requests.iter().find(|r| r.id == id).unwrap().patient_id, Some(pid), "se reconoce por el teléfono");
        let appt = db.booking_confirm(id).unwrap();
        let rec = db.recalls.iter().find(|r| r.id == rid).unwrap();
        assert_eq!((rec.state, rec.appt_id), (RecallState::Citado, Some(appt)));
        let a = db.appointments.iter().find(|a| a.id == appt).unwrap();
        assert_eq!((a.patient_id, a.date, a.start, a.state), (Some(pid), day, start, ApptState::Confirmada));
        assert!(a.notes.contains("Prefiero por la mañana"));
        assert!(db.booking_confirm(id).is_err(), "no se confirma dos veces");
        let r = db.booking_requests.iter().find(|r| r.id == id).unwrap().clone();
        let (to, subject, body, files) = patient_mail(&db, &r).unwrap();
        assert_eq!(to, "laura@example.com");
        assert!(subject.contains("Cita confirmada") && body.contains(&start.format("%H:%M").to_string()));
        assert!(String::from_utf8(files[0].bytes.clone()).unwrap().contains(&format!("DTSTART:{}T", day.format("%Y%m%d"))));
    }

    /// Sirve la página de ejemplo en el puerto 18791 durante unos minutos (para verla en un navegador).
    #[test]
    #[ignore]
    fn serve_demo_page() {
        let today = today();
        let mut db = Db::sample(today);
        db.booking.enabled = true;
        db.booking.cancel_hours = 2;
        // enlaces de las próximas citas, para probar «Mi cita»
        let next: Vec<u32> = db.appointments.iter().filter(|a| a.date > today && !a.state.is_cancelled()).map(|a| a.id).take(3).collect();
        for id in next {
            let t = db.appt_token(id);
            println!("enlace: http://127.0.0.1:18791/mi-cita/{t}");
        }
        // y de los presupuestos pendientes, para probar la firma
        let pend: Vec<u32> = db.budgets.iter().filter(|b| b.state == BudgetState::Pendiente && !b.lines.is_empty()).map(|b| b.id).take(2).collect();
        for id in pend {
            let t = db.budget_token(id);
            println!("presupuesto: http://127.0.0.1:18791/presupuesto/{t}");
        }
        // y un consentimiento para firmar
        if let Some(tid) = db.doc_templates.iter().find(|t| t.name.contains("implantes")).map(|t| t.id)
            && let Some(doc) = db.new_document(101, tid, &[36])
        {
            let t = db.doc_token(doc);
            println!("documento: http://127.0.0.1:18791/documento/{t}");
        }
        // y un hueco libre de la higienista ofrecido a la lista de espera
        let mut day = today + Duration::days(4);
        while day.weekday().num_days_from_monday() >= 5 || db.holidays.iter().any(|h| h.date == day) {
            day += Duration::days(1);
        }
        db.appointments.retain(|a| !(a.date == day && a.doctor_id == 4));
        if let Ok(id) = db.create_offer(crate::slot_offer::FreeSlot { doctor: 4, date: day, start: NaiveTime::from_hms_opt(10, 0, 0).unwrap(), minutes: 45, center: 1 }, None, now())
            && let Some(i) = db.slot_offers.iter().find(|o| o.id == id).and_then(|o| o.invites.first())
        {
            println!("hueco: http://127.0.0.1:18791/hueco/{}", i.token);
        }
        // y «Mis recetas» de Ana con una receta electrónica emitida
        let pid = db.patients[0].id;
        db.patients[0].nif = "12345678Z".into();
        db.patients[0].birth = NaiveDate::from_ymd_opt(1985, 2, 7);
        crate::rx_link::sample_issue(&mut db, pid, today);
        let t = db.rx_token(pid);
        println!("mis recetas: http://127.0.0.1:18791/recetas/{t} (DNI 12345678Z, nacida el 07/02/1985)");
        let srv = listen(18791, false, publish(&db, today), || {}).unwrap();
        let t = std::time::Instant::now();
        while t.elapsed().as_secs() < 400 {
            match srv.rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(WebEvent::Booking(inc)) => println!("solicitud: {} {} {}", inc.request.name, inc.request.date, inc.request.start),
                Ok(WebEvent::Appt { appt, .. }) => println!("acción en la cita {appt}"),
                Ok(WebEvent::Budget { budget, .. }) => println!("acción en el presupuesto {budget}"),
                Ok(WebEvent::Doc { doc, .. }) => println!("acción en el documento {doc}"),
                Ok(WebEvent::Offer { token, .. }) => println!("acción en el hueco {token}"),
                Ok(WebEvent::Rx { patient }) => println!("el paciente {patient} ha entrado en «Mis recetas»"),
                Ok(WebEvent::PayStarted { order, .. }) => println!("pago iniciado: pedido {order}"),
                Ok(WebEvent::PayNotice(n)) => println!("notificación del pedido {}", n.order),
                Err(_) => {}
            }
        }
    }

    /// Pago por internet de principio a fin con un Redsys simulado: el
    /// importe lo pone el programa, una notificación sin firma válida no
    /// cuenta y una repetida no cobra dos veces.
    #[test]
    fn online_payment_from_the_page_to_the_signed_notice() {
        use crate::pago::{Applied, Method, PayFor, PayState, SIGNATURE_VERSION};
        let today = today();
        let now = chrono::Local::now().naive_local();
        let mut db = Db::sample(today);
        db.booking.enabled = true;
        db.booking.public_url = "https://citas.clinica.es/".into();
        db.config.pay_online = crate::pago::PayConfig { enabled: true, bizum: true, deposit: Default::default() };
        db.pay_requests.clear();
        let bid = db.budgets.iter().find(|b| b.state == BudgetState::Pendiente && b.total() > 0.0).map(|b| b.id).unwrap();
        let id = db.create_pay_request(PayFor::Budget(bid), Some(4250), now).unwrap();
        let token = db.pay_request(id).unwrap().token.clone();
        let srv = listen(0, false, publish(&db, today), || {}).unwrap();
        let base = format!("http://127.0.0.1:{}", srv.port);
        let call = |m: &str, p: &str, body: Option<&str>| {
            let r = ureq::request(m, &format!("{base}{p}")).set("Content-Type", "application/x-www-form-urlencoded");
            let res = match body {
                Some(b) => r.send_string(b),
                None => r.call(),
            };
            match res {
                Ok(r) => (r.status(), r.into_string().unwrap()),
                Err(ureq::Error::Status(c, r)) => (c, r.into_string().unwrap()),
                Err(e) => panic!("{e}"),
            }
        };
        // sin TPV en este ordenador, la página lo dice y no deja pagar
        let (code, page) = call("GET", &format!("/pagar/{token}"), None);
        assert_eq!(code, 200);
        assert!(page.contains("42,50") && page.contains("no está disponible"), "{page}");
        assert_eq!(call("POST", &format!("/api/pagar/{token}/iniciar"), Some("metodo=tarjeta")).0, 503);
        let s = crate::pago::tests::settings();
        srv.shared.lock().unwrap().tpv = Some(s.clone());
        let (_, page) = call("GET", &format!("/pagar/{token}"), None);
        assert!(page.contains("Pagar con tarjeta") && page.contains("Pagar con Bizum"));
        // el pedido: formulario firmado hacia Redsys con el importe de la petición
        let (code, form) = call("POST", &format!("/api/pagar/{token}/iniciar"), Some("metodo=bizum&importe=1"));
        assert_eq!(code, 200, "{form}");
        assert!(form.contains("https://sis-t.redsys.es:25443/sis/realizarPago") && form.contains(SIGNATURE_VERSION));
        let field = |name: &str| form.split(&format!("name=\"{name}\" value=\"")).nth(1).unwrap().split('"').next().unwrap().to_string();
        let m = crate::pago::verify(&s.key, &field("Ds_MerchantParameters"), &field("Ds_Signature")).unwrap();
        assert_eq!(m["DS_MERCHANT_AMOUNT"], "4250", "el importe es el de la petición, no el que mande el navegador");
        assert_eq!(m["DS_MERCHANT_MERCHANTURL"], "https://citas.clinica.es/api/tpv/notificacion");
        let order = m["DS_MERCHANT_ORDER"].as_str().unwrap().to_string();
        let started = srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let WebEvent::PayStarted { token: t, order: o, method } = started else { panic!() };
        assert!(t == token && o == order && method == Method::Bizum);
        assert!(db.pay_started(&t, &o, method, now));
        // notificación falsificada (otra clave): no cuenta
        let other = crate::pago::TpvSettings { key: "otraClaveDeOtroComercio00000000".into(), ..s.clone() };
        let (p, sig) = crate::pago::tests::redsys_says(&other, &order, 4250, "0000", &[]);
        let post = |p: &str, sig: &str| call("POST", "/api/tpv/notificacion", Some(&format!("Ds_SignatureVersion={SIGNATURE_VERSION}&Ds_MerchantParameters={p}&Ds_Signature={sig}")));
        assert_eq!(post(&p, &sig).0, 400);
        assert!(srv.rx.recv_timeout(std::time::Duration::from_millis(300)).is_err());
        // la buena, dos veces
        let (p, sig) = crate::pago::tests::redsys_says(&s, &order, 4250, "0000", &[("Ds_ProcessedPayMethod", "68")]);
        assert_eq!(post(&p, &sig), (200, "OK".into()));
        assert_eq!(post(&p, &sig).0, 200);
        let pays = db.payments.len();
        for _ in 0..2 {
            let WebEvent::PayNotice(n) = srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() else { panic!() };
            db.apply_notice(&n, now).unwrap();
        }
        assert_eq!(db.payments.len(), pays + 1, "una notificación repetida no cobra dos veces");
        assert_eq!(db.pay_request(id).unwrap().state, PayState::Pagado);
        assert!(matches!(db.apply_notice(&crate::pago::Notice { order: order.clone(), amount: 4250, response: 0, auth: String::new(), bizum: true }, now).unwrap(), Applied::Duplicate));
        // ya pagado: la página lo dice y no deja pagar otra vez
        srv.shared.lock().unwrap().published = publish(&db, today);
        let (_, page) = call("GET", &format!("/pagar/{token}?resultado=ok"), None);
        assert!(page.contains("Pago recibido") && !page.contains("Pagar con"));
        assert_eq!(call("POST", &format!("/api/pagar/{token}/iniciar"), Some("metodo=tarjeta")).0, 409);
        // y un código inventado no lleva a nada
        assert_eq!(call("GET", &format!("/pagar/{}", "a".repeat(32)), None).0, 404);
    }

    #[test]
    fn budget_link_page_and_signature() {
        use crate::budget_link::BudgetAction;
        let today = today();
        let mut db = Db::sample(today);
        db.booking.enabled = true;
        db.budget_links.clear(); // sin el enlace de ejemplo
        let bid = db.budgets.iter().find(|b| b.state == BudgetState::Pendiente && !b.lines.is_empty() && b.expires() >= today).map(|b| b.id).unwrap();
        let token = db.budget_token(bid);
        let srv = listen(0, false, publish(&db, today), || {}).unwrap();
        let base = format!("http://127.0.0.1:{}", srv.port);
        let call = |m: &str, p: &str, body: Option<serde_json::Value>| {
            let r = ureq::request(m, &format!("{base}{p}")).set("Content-Type", "application/json");
            let res = match body {
                Some(b) => r.send_string(&b.to_string()),
                None => r.call(),
            };
            match res {
                Ok(r) => (r.status(), r.into_string().unwrap()),
                Err(ureq::Error::Status(c, r)) => (c, r.into_string().unwrap()),
                Err(e) => panic!("{e}"),
            }
        };
        let (code, page) = call("GET", &format!("/presupuesto/{token}"), None);
        assert_eq!(code, 200);
        assert!(page.contains("Aceptar y firmar"));
        let (code, info) = call("GET", &format!("/api/presupuesto/{token}"), None);
        assert_eq!(code, 200, "{info}");
        let v: serde_json::Value = serde_json::from_str(&info).unwrap();
        assert_eq!(v["estado"], "pendiente");
        assert!(!v["lineas"].as_array().unwrap().is_empty());
        let p = db.patient(db.budgets.iter().find(|b| b.id == bid).unwrap().patient_id).unwrap().clone();
        assert!(!info.contains(&p.last_name) && !info.contains(&p.nif), "sin apellidos ni NIF del paciente");
        // la primera visita se avisa una sola vez
        assert!(matches!(srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), WebEvent::Budget { action: BudgetAction::Viewed, .. }));
        call("GET", &format!("/api/presupuesto/{token}"), None);
        assert!(srv.rx.recv_timeout(std::time::Duration::from_millis(200)).is_err());
        let hash = v["huella"].as_str().unwrap().to_string();
        let firma: Vec<Vec<[f32; 2]>> = vec![(0..30).map(|i| [i as f32 / 30.0, 0.5]).collect()];
        // sin firma, con otra huella o sin aceptar: no
        assert_eq!(call("POST", &format!("/api/presupuesto/{token}/aceptar"), Some(serde_json::json!({"nombre": "Ana García", "firma": [], "huella": hash, "acepta": true}))).0, 400);
        assert_eq!(call("POST", &format!("/api/presupuesto/{token}/aceptar"), Some(serde_json::json!({"nombre": "Ana García", "firma": firma, "huella": "x", "acepta": true}))).0, 409);
        assert_eq!(call("POST", &format!("/api/presupuesto/{token}/aceptar"), Some(serde_json::json!({"nombre": "Ana García", "firma": firma, "huella": hash}))).0, 400);
        // una pregunta
        assert_eq!(call("POST", &format!("/api/presupuesto/{token}/consulta"), Some(serde_json::json!({"texto": "¿Puedo pagar con tarjeta?"}))).0, 200);
        assert!(matches!(srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), WebEvent::Budget { action: BudgetAction::Question(q), .. } if q.contains("tarjeta")));
        // aceptar y firmar
        let (code, text) = call("POST", &format!("/api/presupuesto/{token}/aceptar"), Some(serde_json::json!({"nombre": "Ana García López", "nif": "12345678Z", "firma": firma, "cuotas": 0, "huella": hash, "acepta": true})));
        assert_eq!(code, 200, "{text}");
        match srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            WebEvent::Budget { budget, action: BudgetAction::Accept { signer, strokes, ip, .. } } => {
                assert_eq!(budget, bid);
                assert_eq!(signer, "Ana García López");
                assert_eq!(strokes[0].len(), 30);
                assert_eq!(ip, "127.0.0.1");
            }
            _ => panic!("se esperaba la aceptación"),
        }
        // ya no se puede aceptar ni rechazar otra vez
        assert_eq!(call("POST", &format!("/api/presupuesto/{token}/rechazar"), Some(serde_json::json!({"texto": "no"}))).0, 409);
        let (_, info) = call("GET", &format!("/api/presupuesto/{token}"), None);
        assert!(info.contains("\"estado\":\"aceptado\""));
        assert_eq!(call("GET", &format!("/api/presupuesto/{}", "a".repeat(32)), None).0, 404);
    }

    #[test]
    fn appointment_link_page_and_actions() {
        let today = today();
        let mut db = Db::sample(today);
        db.booking.enabled = true;
        db.booking.min_notice_hours = 0;
        db.booking.cancel_hours = 0;
        // una cita de Carlos (cuestionario de hace más de un año) dentro de unos días
        let mut day = today + Duration::days(3);
        while day.weekday().num_days_from_monday() >= 5 || db.holidays.iter().any(|h| h.date == day) {
            day += Duration::days(1);
        }
        let id = db.new_id();
        db.appointments.push(Appointment {
            id,
            date: day,
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            minutes: 30,
            patient_id: Some(101),
            free_name: String::new(),
            phone: String::new(),
            doctor_id: 2,
            box_id: 2,
            reason: "Revisión implante".into(),
            treatment_code: "0102".into(),
            state: ApptState::Planificada,
            notes: "Paciente muy nervioso".into(),
            arrived_at: None,
            seen_at: None,
            finished_at: None,
            reminder_sent: false,
            first_visit: false,
            center_id: 1,
        });
        let token = db.appt_token(id);
        let srv = listen(0, false, publish(&db, today), || {}).unwrap();
        let base = format!("http://127.0.0.1:{}", srv.port);
        let call = |m: &str, p: &str, body: Option<serde_json::Value>| {
            let r = ureq::request(m, &format!("{base}{p}")).set("Content-Type", "application/json");
            let res = match body {
                Some(b) => r.send_string(&b.to_string()),
                None => r.call(),
            };
            match res {
                Ok(r) => (r.status(), r.into_string().unwrap()),
                Err(ureq::Error::Status(c, r)) => (c, r.into_string().unwrap()),
                Err(e) => panic!("{e}"),
            }
        };
        let (code, page) = call("GET", &format!("/mi-cita/{token}"), None);
        assert_eq!(code, 200);
        assert!(page.contains("Mi cita"));
        let (code, info) = call("GET", &format!("/api/cita/{token}"), None);
        assert_eq!(code, 200, "{info}");
        let v: serde_json::Value = serde_json::from_str(&info).unwrap();
        assert_eq!((v["nombre"].as_str(), v["estado"].as_str(), v["puede_confirmar"].as_bool()), (Some("Carlos"), Some("planificada"), Some(true)));
        assert!(v["cuestionario"]["pedir"].as_bool().unwrap(), "su cuestionario tiene más de un año");
        // ni apellidos, ni sus respuestas de salud anteriores, ni notas internas
        assert!(!info.contains("Martínez") && !info.contains("Fibrilación") && !info.contains("nervioso"), "{info}");
        // un código que no existe
        assert_eq!(call("GET", &format!("/api/cita/{}", "0".repeat(32)), None).0, 404);
        assert_eq!(call("GET", "/api/cita/../../datos", None).0, 404);
        // confirmar
        assert_eq!(call("POST", &format!("/api/cita/{token}/confirmar"), Some(serde_json::json!({}))).0, 200);
        assert!(matches!(srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), WebEvent::Appt { appt, action: ApptAction::Confirm } if appt == id));
        // cambiar a otra hora libre (una sola petición a la vez)
        let (_, days) = call("GET", &format!("/api/cita/{token}/huecos"), None);
        let days: serde_json::Value = serde_json::from_str(&days).unwrap();
        let (fecha, hora) = (days["dias"][0]["fecha"].as_str().unwrap().to_string(), days["dias"][0]["horas"][0]["hora"].as_str().unwrap().to_string());
        let (code, text) = call("POST", &format!("/api/cita/{token}/cambiar"), Some(serde_json::json!({"fecha": fecha, "hora": hora})));
        assert_eq!(code, 200, "{text}");
        match srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            WebEvent::Appt { action: ApptAction::Change(inc), .. } => {
                assert_eq!(inc.request.replaces, Some(id));
                assert_eq!(inc.request.start.format("%H:%M").to_string(), hora);
                assert_eq!(inc.request.doctor_id, 2, "con el mismo profesional");
            }
            _ => panic!("se esperaba un cambio"),
        }
        assert_eq!(call("POST", &format!("/api/cita/{token}/cambiar"), Some(serde_json::json!({"fecha": fecha, "hora": hora}))).0, 409);
        // cuestionario de salud
        assert_eq!(call("POST", &format!("/api/cita/{token}/salud"), Some(serde_json::json!({"respuestas": {"diabetes": true}}))).0, 400, "hay que aceptar");
        let (code, text) = call("POST", &format!("/api/cita/{token}/salud"), Some(serde_json::json!({"respuestas": {"diabetes": true, "alergia_med": true}, "detalles": {"alergia_med": "Ibuprofeno"}, "acepta": true})));
        assert_eq!(code, 200, "{text}");
        match srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            WebEvent::Appt { action: ApptAction::Health(f), .. } => assert!(f.is("diabetes") && f.detail("alergia_med") == "Ibuprofeno"),
            _ => panic!("se esperaba el cuestionario"),
        }
        assert_eq!(call("POST", &format!("/api/cita/{token}/salud"), Some(serde_json::json!({"acepta": true}))).0, 409, "una sola vez");
        // para el calendario
        let (code, ics) = call("GET", &format!("/api/cita/{token}/cita.ics"), None);
        assert_eq!(code, 200);
        assert!(ics.contains("BEGIN:VCALENDAR") && ics.contains(&format!("DTSTART:{}T090000", day.format("%Y%m%d"))));
        // anular
        assert_eq!(call("POST", &format!("/api/cita/{token}/anular"), Some(serde_json::json!({"motivo": "Estoy de viaje"}))).0, 200);
        match srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            WebEvent::Appt { action: ApptAction::Cancel(why), .. } => assert_eq!(why, "Estoy de viaje"),
            _ => panic!("se esperaba la anulación"),
        }
        let (_, info) = call("GET", &format!("/api/cita/{token}"), None);
        assert!(info.contains("\"estado\":\"anulada\""));
    }

    #[test]
    fn slot_offer_first_come_first_served() {
        use crate::slot_offer::OfferAction;
        let today = today();
        let mut db = Db::sample(today);
        db.booking.enabled = true;
        let mut day = today + Duration::days(4);
        while day.weekday().num_days_from_monday() >= 5 || db.holidays.iter().any(|h| h.date == day) {
            day += Duration::days(1);
        }
        db.appointments.retain(|a| !(a.date == day && a.doctor_id == 4));
        let wid = db.new_id();
        db.waitlist.push(WaitItem { id: wid, patient_id: 102, doctor_id: 4, reason: "Limpieza".into(), since: today, preference: "Cualquier hora".into() });
        let id = db.create_offer(crate::slot_offer::FreeSlot { doctor: 4, date: day, start: NaiveTime::from_hms_opt(10, 0, 0).unwrap(), minutes: 45, center: 1 }, None, now()).unwrap();
        let o = db.slot_offers.iter().find(|o| o.id == id).unwrap().clone();
        assert_eq!(o.invites.len(), 2);
        let (t1, t2) = (o.invites[0].token.clone(), o.invites[1].token.clone());
        let srv = listen(0, false, publish(&db, today), || {}).unwrap();
        let base = format!("http://127.0.0.1:{}", srv.port);
        let call = |m: &str, p: &str| {
            let r = ureq::request(m, &format!("{base}{p}")).set("Content-Type", "application/json");
            let res = if m == "POST" { r.send_string("{}") } else { r.call() };
            match res {
                Ok(r) => (r.status(), r.into_string().unwrap()),
                Err(ureq::Error::Status(c, r)) => (c, r.into_string().unwrap()),
                Err(e) => panic!("{e}"),
            }
        };
        let (code, page) = call("GET", &format!("/hueco/{t1}"));
        assert_eq!(code, 200);
        assert!(page.contains("Sí, lo quiero"));
        let (_, info) = call("GET", &format!("/api/hueco/{t1}"));
        assert!(info.contains("\"estado\":\"abierta\""), "{info}");
        assert!(matches!(srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), WebEvent::Offer { action: OfferAction::Viewed, .. }));
        // el segundo invitado lo pide antes que el primero: se lo queda él
        assert_eq!(call("POST", &format!("/api/hueco/{t2}/reservar")).0, 200);
        match srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            WebEvent::Offer { token, action: OfferAction::Take } => assert_eq!(token, t2),
            _ => panic!("se esperaba la reserva"),
        }
        assert_eq!(call("POST", &format!("/api/hueco/{t1}/reservar")).0, 409, "el que llega tarde no lo consigue");
        assert!(call("GET", &format!("/api/hueco/{t1}")).1.contains("\"estado\":\"ocupada\""));
        assert!(call("GET", &format!("/api/hueco/{t2}")).1.contains("\"estado\":\"suya\""));
        let (code, ics) = call("GET", &format!("/api/hueco/{t2}/cita.ics"));
        assert_eq!(code, 200);
        assert!(ics.contains("BEGIN:VCALENDAR"));
        assert_eq!(call("GET", &format!("/api/hueco/{}", "c".repeat(32))).0, 404);
    }

    #[test]
    fn my_prescriptions_link_needs_id_and_birth_date() {
        let today = today();
        let mut db = Db::sample(today);
        db.booking.enabled = true;
        db.rx_links.clear();
        let pid = db.patients[0].id;
        db.patients[0].nif = "12345678Z".into();
        db.patients[0].birth = NaiveDate::from_ymd_opt(1985, 2, 7);
        let (first, last) = (db.patients[0].first_name.clone(), db.patients[0].last_name.clone());
        crate::rx_link::sample_issue(&mut db, pid, today);
        let token = db.rx_token(pid);
        let srv = listen(0, false, publish(&db, today), || {}).unwrap();
        let base = format!("http://127.0.0.1:{}", srv.port);
        let call = |m: &str, p: &str, body: Option<serde_json::Value>| {
            let r = ureq::request(m, &format!("{base}{p}")).set("Content-Type", "application/json");
            let res = match body {
                Some(b) => r.send_string(&b.to_string()),
                None => r.call(),
            };
            match res {
                Ok(r) => (r.status(), r.into_string().unwrap()),
                Err(ureq::Error::Status(c, r)) => (c, r.into_string().unwrap()),
                Err(e) => panic!("{e}"),
            }
        };
        // ni la página ni la primera consulta llevan datos del paciente
        let (code, page) = call("GET", &format!("/recetas/{token}"), None);
        assert_eq!(code, 200);
        assert!(!page.contains("AMOXICILINA") && !page.contains(&last));
        let (code, info) = call("GET", &format!("/api/recetas/{token}"), None);
        assert_eq!(code, 200);
        assert!(info.contains("\"verificable\":true") && !info.contains("AMOXICILINA") && !info.contains(&first), "{info}");
        // datos que no coinciden
        let bad = serde_json::json!({"dni": "12345678Z", "nacimiento": "08/02/1985"});
        let (code, text) = call("POST", &format!("/api/recetas/{token}"), Some(bad.clone()));
        assert_eq!(code, 403);
        assert!(text.contains("le quedan 4"), "{text}");
        // los buenos, escritos de otra forma
        let good = serde_json::json!({"dni": "12.345.678-z", "nacimiento": "1985-02-07"});
        let (code, text) = call("POST", &format!("/api/recetas/{token}"), Some(good.clone()));
        assert_eq!(code, 200, "{text}");
        assert!(text.contains("AMOXICILINA") && text.contains("Dispensada parcialmente") && text.contains(&first));
        assert!(matches!(srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), WebEvent::Rx { patient } if patient == pid));
        let (code, sheet) = call("POST", &format!("/api/recetas/{token}/hoja/0"), Some(good.clone()));
        assert_eq!(code, 200);
        assert!(sheet.contains("svg"), "la hoja con sus códigos");
        assert_eq!(call("POST", &format!("/api/recetas/{token}/hoja/9"), Some(good.clone())).0, 404);
        assert_eq!(call("POST", &format!("/api/recetas/{token}/hoja/0"), Some(bad.clone())).0, 403, "la hoja también pide los datos");
        // tras 5 fallos queda bloqueado media hora, también con los datos buenos
        for _ in 0..3 {
            assert_eq!(call("POST", &format!("/api/recetas/{token}"), Some(bad.clone())).0, 403);
        }
        assert_eq!(call("POST", &format!("/api/recetas/{token}"), Some(good.clone())).0, 429);
        // un enlace que no existe
        assert_eq!(call("GET", &format!("/api/recetas/{}", "0".repeat(32)), None).0, 404);
    }

    #[test]
    fn document_link_page_and_signature() {
        use crate::doc_link::DocAction;
        let today = today();
        let mut db = Db::sample(today);
        db.booking.enabled = true;
        db.doc_links.clear();
        let tid = db.doc_templates.iter().find(|t| t.name.contains("implantes")).map(|t| t.id).unwrap();
        let doc = db.new_document(101, tid, &[36]).unwrap();
        let token = db.doc_token(doc);
        // y una cita próxima de Carlos con su enlace: el documento aparece en «Mi cita»
        let mut day = today + Duration::days(2);
        while day.weekday().num_days_from_monday() >= 5 {
            day += Duration::days(1);
        }
        let appt = db.new_id();
        db.appointments.push(Appointment {
            id: appt,
            date: day,
            start: NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            minutes: 60,
            patient_id: Some(101),
            free_name: String::new(),
            phone: String::new(),
            doctor_id: 2,
            box_id: 2,
            reason: "Implante 36".into(),
            treatment_code: "0701".into(),
            state: ApptState::Planificada,
            notes: String::new(),
            arrived_at: None,
            seen_at: None,
            finished_at: None,
            reminder_sent: false,
            first_visit: false,
            center_id: 1,
        });
        let appt_token = Some(db.appt_token(appt));
        let srv = listen(0, false, publish(&db, today), || {}).unwrap();
        let base = format!("http://127.0.0.1:{}", srv.port);
        let call = |m: &str, p: &str, body: Option<serde_json::Value>| {
            let r = ureq::request(m, &format!("{base}{p}")).set("Content-Type", "application/json");
            let res = match body {
                Some(b) => r.send_string(&b.to_string()),
                None => r.call(),
            };
            match res {
                Ok(r) => (r.status(), r.into_string().unwrap()),
                Err(ureq::Error::Status(c, r)) => (c, r.into_string().unwrap()),
                Err(e) => panic!("{e}"),
            }
        };
        let (code, page) = call("GET", &format!("/documento/{token}"), None);
        assert_eq!(code, 200);
        assert!(page.contains("Firmar el documento"));
        let (code, info) = call("GET", &format!("/api/documento/{token}"), None);
        assert_eq!(code, 200, "{info}");
        let v: serde_json::Value = serde_json::from_str(&info).unwrap();
        assert_eq!(v["estado"], "pendiente");
        assert!(v["texto"].as_str().unwrap().contains("IMPLANTES"), "el texto completo del consentimiento");
        assert!(matches!(srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), WebEvent::Doc { action: DocAction::Viewed, .. }));
        call("GET", &format!("/api/documento/{token}"), None);
        assert!(srv.rx.recv_timeout(std::time::Duration::from_millis(200)).is_err(), "la lectura se avisa una vez");
        if let Some(t) = appt_token {
            let (_, mi) = call("GET", &format!("/api/cita/{t}"), None);
            let m: serde_json::Value = serde_json::from_str(&mi).unwrap();
            assert!(m["documentos"].as_array().unwrap().iter().any(|d| d["enlace"].as_str().unwrap().ends_with(&token)), "{mi}");
        }
        let hash = v["huella"].as_str().unwrap().to_string();
        let firma: Vec<Vec<[f32; 2]>> = vec![(0..30).map(|i| [i as f32 / 30.0, 0.5]).collect()];
        assert_eq!(call("POST", &format!("/api/documento/{token}/firmar"), Some(serde_json::json!({"nombre": "Carlos Martínez Sanz", "firma": firma, "huella": hash}))).0, 400, "sin marcar la casilla");
        assert_eq!(call("POST", &format!("/api/documento/{token}/firmar"), Some(serde_json::json!({"nombre": "Carlos Martínez Sanz", "firma": firma, "huella": "otra", "acepta": true}))).0, 409);
        assert_eq!(call("POST", &format!("/api/documento/{token}/consulta"), Some(serde_json::json!({"texto": "¿Cuánto dura la cirugía?"}))).0, 200);
        assert!(matches!(srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), WebEvent::Doc { action: DocAction::Question(q), .. } if q.contains("cirugía")));
        let (code, text) = call("POST", &format!("/api/documento/{token}/firmar"), Some(serde_json::json!({"nombre": "Carlos Martínez Sanz", "nif": "12345678Z", "firma": firma, "huella": hash, "acepta": true})));
        assert_eq!(code, 200, "{text}");
        match srv.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            WebEvent::Doc { doc: d, action: DocAction::Sign { signer, strokes, ip, .. } } => {
                assert_eq!(d, doc);
                assert_eq!(signer, "Carlos Martínez Sanz");
                assert_eq!(strokes[0].len(), 30);
                assert_eq!(ip, "127.0.0.1");
            }
            _ => panic!("se esperaba la firma"),
        }
        assert_eq!(call("POST", &format!("/api/documento/{token}/firmar"), Some(serde_json::json!({"nombre": "Carlos Martínez Sanz", "firma": firma, "huella": hash, "acepta": true}))).0, 409, "no se firma dos veces");
        assert_eq!(call("POST", &format!("/api/documento/{token}/en-clinica"), Some(serde_json::json!({}))).0, 409);
        assert_eq!(call("GET", &format!("/api/documento/{}", "b".repeat(32)), None).0, 404);
    }

    #[test]
    fn web_page_books_a_free_slot() {
        let today = today();
        let mut db = Db::sample(today);
        db.booking.enabled = true;
        db.booking.min_notice_hours = 0;
        let srv = listen(0, false, publish(&db, today), || {}).unwrap();
        let base = format!("http://127.0.0.1:{}", srv.port);
        let get = |p: &str| ureq::get(&format!("{base}{p}")).call().unwrap().into_string().unwrap();
        let page = get("/");
        assert!(page.contains(&db.config.name) && page.contains("api/reservar"));
        let info: serde_json::Value = serde_json::from_str(&get("/api/datos")).unwrap();
        assert!(info["motivos"].as_array().unwrap().len() >= 5);
        assert!(!get("/api/datos").contains("García"), "no se publica ningún paciente");
        let days: serde_json::Value = serde_json::from_str(&get("/api/huecos?motivo=0&profesional=0")).unwrap();
        // el primer día con al menos dos horas libres (a última hora, hoy puede quedar una sola)
        let d0 = days["dias"].as_array().unwrap().iter().find(|d| d["horas"].as_array().is_some_and(|h| h.len() >= 2)).expect("sin días con huecos");
        let (fecha, hora) = (d0["fecha"].as_str().unwrap().to_string(), d0["horas"][0]["hora"].as_str().unwrap().to_string());
        let post = |v: serde_json::Value| match ureq::post(&format!("{base}/api/reservar")).set("Content-Type", "application/json").send_string(&v.to_string()) {
            Ok(r) => (r.status(), r.into_string().unwrap()),
            Err(ureq::Error::Status(c, r)) => (c, r.into_string().unwrap()),
            Err(e) => panic!("{e}"),
        };
        let body = serde_json::json!({"motivo": 0, "profesional": 0, "fecha": fecha, "hora": hora, "nombre": "Raúl Gómez", "telefono": "644 812 330", "email": "raul@example.com", "acepta": true});
        let (code, text) = post(serde_json::json!({"motivo": 0, "profesional": 0, "fecha": fecha, "hora": hora, "nombre": "Raúl Gómez", "telefono": "644 812 330"}));
        assert_eq!(code, 400, "sin aceptar la protección de datos: {text}");
        let (code, text) = post(body.clone());
        assert_eq!(code, 200, "{text}");
        let Ok(WebEvent::Booking(inc)) = srv.rx.recv_timeout(std::time::Duration::from_secs(5)) else { panic!("no llegó la solicitud") };
        assert_eq!(inc.request.name, "Raúl Gómez");
        assert_eq!(inc.request.start.format("%H:%M").to_string(), hora);
        // la misma hora ya no se puede pedir otra vez (con el mismo profesional)
        let mut again = body.clone();
        again["profesional"] = serde_json::json!(inc.request.doctor_id);
        let (code, _) = post(again);
        assert_eq!(code, 409);
        // el campo trampa se acepta en apariencia pero no llega
        let mut bot = body.clone();
        bot["web"] = serde_json::json!("http://spam");
        bot["hora"] = serde_json::json!(d0["horas"][1]["hora"].clone());
        let (code, _) = post(bot);
        assert_eq!(code, 200);
        assert!(srv.rx.recv_timeout(std::time::Duration::from_millis(300)).is_err());
        let missing = ureq::get(&format!("{base}/nada")).call().err().map(|e| match e {
            ureq::Error::Status(c, _) => c,
            _ => 0,
        });
        assert_eq!(missing, Some(404));
    }
}
