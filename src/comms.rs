//! Comunicaciones automáticas con pacientes (recordatorios de cita, recalls,
//! cumpleaños, presupuestos pendientes) y documentos/consentimientos con firma.

use crate::model::*;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum Channel {
    Sms,
    Email,
    WhatsApp,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::WhatsApp, Channel::Sms, Channel::Email];
    pub fn label(self) -> &'static str {
        match self {
            Channel::Sms => "SMS",
            Channel::Email => "Email",
            Channel::WhatsApp => "WhatsApp",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum MsgKind {
    RecordatorioCita,
    Recall,
    Cumpleanos,
    PresupuestoPendiente,
    Libre,
    /// Aviso a la lista de espera de un hueco que ha quedado libre.
    HuecoLibre,
}

impl MsgKind {
    pub fn label(self) -> &'static str {
        match self {
            MsgKind::RecordatorioCita => "Recordatorio de cita",
            MsgKind::Recall => "Aviso de revisión (recall)",
            MsgKind::Cumpleanos => "Felicitación de cumpleaños",
            MsgKind::PresupuestoPendiente => "Seguimiento de presupuesto",
            MsgKind::Libre => "Mensaje libre",
            MsgKind::HuecoLibre => "Hueco libre (lista de espera)",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MsgTemplate {
    pub id: u32,
    pub name: String,
    pub kind: MsgKind,
    pub channel: Channel,
    /// Días de antelación (recordatorios) o de espera (presupuestos).
    pub days: i32,
    pub text: String,
    pub subject: String,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum MsgState {
    Pendiente,
    Enviado,
    Descartado,
}

impl MsgState {
    pub fn label(self) -> &'static str {
        match self {
            MsgState::Pendiente => "Pendiente",
            MsgState::Enviado => "Enviado",
            MsgState::Descartado => "Descartado",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: u32,
    pub patient_id: u32,
    pub kind: MsgKind,
    pub channel: Channel,
    pub to: String,
    pub subject: String,
    pub text: String,
    pub created: NaiveDateTime,
    pub sent_at: Option<NaiveDateTime>,
    pub state: MsgState,
    /// Cita, recall o presupuesto al que se refiere (evita duplicados).
    pub related: Option<u32>,
    pub user: String,
    /// Motivo del último intento de envío fallido (correo).
    #[serde(default)]
    pub error: String,
    /// Intentos de envío por correo.
    #[serde(default)]
    pub attempts: u32,
    /// Se cortó el envío sin confirmación: puede que haya llegado. No se
    /// reintenta solo; alguien decide si consta como enviado o se reenvía.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub uncertain: bool,
    /// Envío en curso: marca del puesto que lo ha reservado para enviarlo
    /// (así dos puestos no envían el mismo correo) y desde cuándo.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sending: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sending_since: Option<NaiveDateTime>,
    /// Quién lo dio por enviado.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sent_by: String,
}

/// Minutos tras los que un envío reservado por un puesto que no ha dicho
/// cómo terminó (se cerró o perdió la conexión) pasa a «no se sabe si llegó».
pub const SENDING_LEASE_MIN: i64 = 30;

/// Comodines con datos de salud: valen en los documentos, pero no salen en
/// un mensaje (SMS, WhatsApp o correo).
pub const CLINICAL_PLACEHOLDERS: [&str; 4] = ["[ALERTAS]", "[TRATAMIENTOS]", "[PENDIENTES]", "[FECHANAC]"];

/// Comodines que se sustituyen en un mensaje de esta clase. Los demás
/// llegarían tal cual al paciente.
pub fn msg_placeholders(kind: MsgKind) -> Vec<&'static str> {
    let mut v = vec!["[NOMBRE]", "[APELLIDOS]", "[CLINICA]", "[TELCLINICA]", "[DIRCLINICA]", "[NIF]", "[HISTORIAL]", "[CIUDAD]", "[DIRECCION]", "[DOCTOR]", "[FECHA]"];
    match kind {
        MsgKind::RecordatorioCita => v.extend(["[HORA]", "[MOTIVO]", "[ENLACE]"]),
        MsgKind::Recall => v.extend(["[MOTIVO]", "[ENLACE_CITA]"]),
        MsgKind::PresupuestoPendiente => v.extend(["[MOTIVO]", "[IMPORTE]", "[ENLACE_PRESUPUESTO]"]),
        _ => {}
    }
    v
}

/// Comodines de un texto que no se sustituirían en un mensaje de esta clase
/// (mal escritos, de otra clase o con datos de salud).
pub fn unknown_placeholders(text: &str, kind: MsgKind) -> Vec<String> {
    let ok = msg_placeholders(kind);
    let mut out: Vec<String> = vec![];
    let mut rest = text;
    while let Some(i) = rest.find('[') {
        let after = &rest[i + 1..];
        let Some(j) = after.find(']') else { break };
        let tok = &after[..j];
        if tok.chars().count() >= 3 && tok.chars().all(|c| c.is_uppercase() || c == '_') {
            let t = format!("[{tok}]");
            if !ok.contains(&t.as_str()) && !out.contains(&t) {
                out.push(t);
            }
        }
        rest = &after[j + 1..];
    }
    out
}

/// Clave de un recordatorio de los que se generan solos (de cita, de revisión,
/// de cumpleaños, de presupuesto): dos con la misma clave son el mismo aviso.
pub fn auto_message_key(m: &Message) -> Option<String> {
    let auto = matches!(m.kind, MsgKind::RecordatorioCita | MsgKind::Recall | MsgKind::Cumpleanos | MsgKind::PresupuestoPendiente);
    (auto && m.related.is_some()).then(|| format!("{:?}:{}:{}", m.kind, m.related.unwrap_or(0), m.patient_id))
}

pub fn default_msg_templates() -> Vec<MsgTemplate> {
    let t = |id, name: &str, kind, channel, days, subject: &str, text: &str| MsgTemplate { id, name: name.into(), kind, channel, days, subject: subject.into(), text: text.into(), active: true };
    vec![
        t(1, "Recordatorio de cita (WhatsApp)", MsgKind::RecordatorioCita, Channel::WhatsApp, 1, "", "Hola [NOMBRE], le recordamos su cita en [CLINICA] el [FECHA] a las [HORA] con [DOCTOR]. Si no puede venir, llámenos al [TELCLINICA]. ¡Gracias!"),
        t(2, "Recordatorio de cita (SMS)", MsgKind::RecordatorioCita, Channel::Sms, 1, "", "[CLINICA]: cita el [FECHA] a las [HORA]. Para cambiarla llame al [TELCLINICA]."),
        t(3, "Aviso de revisión", MsgKind::Recall, Channel::WhatsApp, 7, "", "Hola [NOMBRE], ya le toca su [MOTIVO] en [CLINICA]. Llámenos al [TELCLINICA] o responda a este mensaje para darle cita."),
        t(4, "Felicitación de cumpleaños", MsgKind::Cumpleanos, Channel::Sms, 0, "", "¡Feliz cumpleaños, [NOMBRE]! Todo el equipo de [CLINICA] le desea un gran día."),
        t(
            5,
            "Seguimiento de presupuesto",
            MsgKind::PresupuestoPendiente,
            Channel::Email,
            7,
            "Su presupuesto en [CLINICA]",
            "Estimado/a [NOMBRE]:\n\nLe recordamos que tiene pendiente el presupuesto «[MOTIVO]» por importe de [IMPORTE]. Si tiene cualquier duda, estaremos encantados de atenderle en el [TELCLINICA].\n\nUn saludo,\n[CLINICA]",
        ),
        t(
            6,
            "Recordatorio de cita (correo)",
            MsgKind::RecordatorioCita,
            Channel::Email,
            1,
            "Recordatorio: su cita en [CLINICA] el [FECHA] a las [HORA]",
            "Hola, [NOMBRE]:\n\nLe recordamos su cita en [CLINICA] el [FECHA] a las [HORA] con [DOCTOR].\n\nSi no puede venir, avísenos llamando al [TELCLINICA].\n\nUn saludo,\n[CLINICA]",
        ),
        t(7, "Aviso de revisión (correo)", MsgKind::Recall, Channel::Email, 7, "Le toca su [MOTIVO] en [CLINICA]", "Hola, [NOMBRE]:\n\nYa le toca su [MOTIVO]. Llámenos al [TELCLINICA] y le damos cita.\n\nUn saludo,\n[CLINICA]"),
    ]
}

// ───────────────────────────── documentos ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocTemplate {
    pub id: u32,
    pub name: String,
    pub category: String,
    pub body: String,
    /// Al firmarse marca el consentimiento de protección de datos del paciente.
    #[serde(default)]
    pub marks_gdpr: bool,
    /// Clases de tratamiento que piden este consentimiento al aceptar un presupuesto (ver `plan`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub for_kinds: Vec<crate::model::OdgKind>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PatientDoc {
    pub id: u32,
    pub patient_id: u32,
    pub template_id: u32,
    pub title: String,
    pub category: String,
    pub date: NaiveDate,
    pub body: String,
    pub doctor_id: u32,
    pub signed: bool,
    pub signer: String,
    pub signed_at: Option<NaiveDateTime>,
    /// Trazos de la firma manuscrita (coordenadas 0..1 en el recuadro).
    pub signature: Vec<Vec<[f32; 2]>>,
    /// Paciente para quien se firmó (al fusionar fichas, `patient_id` pasa a la que se conserva).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_patient: Option<u32>,
    /// Huella del contenido firmado y sello con firmante, fecha y trazo (ver `firmado`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub seal: String,
    /// Revocación del consentimiento (el documento y su firma se conservan).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<NaiveDateTime>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revoked_by: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revoke_reason: String,
}

pub fn default_doc_templates() -> Vec<DocTemplate> {
    use crate::model::OdgKind;
    let kinds = |id| match id {
        2 => vec![OdgKind::Extraccion],
        3 => vec![OdgKind::Implante],
        4 => vec![OdgKind::Endodoncia],
        _ => vec![],
    };
    let d = |id, name: &str, cat: &str, body: &str, gdpr| DocTemplate { id, name: name.into(), category: cat.into(), body: body.into(), marks_gdpr: gdpr, for_kinds: kinds(id) };
    vec![
        d(
            1,
            "Protección de datos (RGPD)",
            "Protección de datos",
            "D./Dña. [NOMBRE] [APELLIDOS], con NIF [NIF], queda informado/a de que sus datos personales y de salud serán tratados por [CLINICA] con la finalidad de prestarle asistencia sanitaria, gestionar su historia clínica y la facturación de los servicios.\n\nLa base legal es la prestación de asistencia sanitaria y el cumplimiento de obligaciones legales. Los datos se conservarán durante el tiempo exigido por la normativa sanitaria. Puede ejercer sus derechos de acceso, rectificación, supresión, oposición, limitación y portabilidad dirigiéndose a [CLINICA], [DIRCLINICA].\n\nAutorizo a recibir comunicaciones sobre mis citas y revisiones por SMS, correo electrónico o WhatsApp.\n\nEn [CIUDAD], a [FECHA].",
            true,
        ),
        d(
            2,
            "Consentimiento informado: extracción dental",
            "Consentimiento informado",
            "D./Dña. [NOMBRE] [APELLIDOS] (Nº de historia [HISTORIAL]) declara que [DOCTOR] le ha explicado el tratamiento de EXTRACCIÓN de la pieza [PIEZAS], su finalidad y las alternativas posibles.\n\nHa sido informado/a de los riesgos más habituales: dolor e inflamación postoperatoria, sangrado, hematoma, infección, alveolitis, lesión de piezas vecinas, comunicación con el seno maxilar en piezas superiores y alteraciones transitorias o permanentes de la sensibilidad en piezas inferiores.\n\nHa podido preguntar sus dudas, que han sido aclaradas, y CONSIENTE la realización del tratamiento. Puede revocar este consentimiento en cualquier momento.\n\nEn [CIUDAD], a [FECHA].",
            false,
        ),
        d(
            3,
            "Consentimiento informado: implantes",
            "Consentimiento informado",
            "D./Dña. [NOMBRE] [APELLIDOS] (Nº de historia [HISTORIAL]) ha sido informado/a por [DOCTOR] de la colocación de IMPLANTES dentales en la zona [PIEZAS], del procedimiento quirúrgico, del tiempo de integración y de la rehabilitación posterior.\n\nRiesgos: dolor, inflamación, hematoma, infección, falta de integración del implante, lesión nerviosa con alteración de la sensibilidad, comunicación con el seno maxilar y necesidad de injertos. El éxito depende también de la higiene, las revisiones periódicas y de no fumar.\n\nCONSIENTE la realización del tratamiento y se compromete a seguir las indicaciones y revisiones.\n\nEn [CIUDAD], a [FECHA].",
            false,
        ),
        d(
            4,
            "Consentimiento informado: endodoncia",
            "Consentimiento informado",
            "D./Dña. [NOMBRE] [APELLIDOS] ha sido informado/a por [DOCTOR] del tratamiento de ENDODONCIA de la pieza [PIEZAS], que consiste en limpiar, desinfectar y sellar los conductos radiculares.\n\nRiesgos: molestias postoperatorias, fractura de instrumentos dentro del conducto, perforaciones, fractura de la pieza y fracaso del tratamiento que pueda requerir repetirlo, cirugía o extracción. Tras la endodoncia suele ser necesaria una reconstrucción o corona.\n\nCONSIENTE la realización del tratamiento.\n\nEn [CIUDAD], a [FECHA].",
            false,
        ),
        d(
            5,
            "Justificante de asistencia",
            "Justificante",
            "[CLINICA] certifica que D./Dña. [NOMBRE] [APELLIDOS], con NIF [NIF], ha acudido a consulta en el día de hoy, [FECHA].\n\nY para que conste a los efectos oportunos, se expide el presente justificante.\n\n[DOCTOR]",
            false,
        ),
        d(
            6,
            "Informe clínico",
            "Informe",
            "INFORME CLÍNICO\n\nPaciente: [NOMBRE] [APELLIDOS] · Nº de historia [HISTORIAL] · Fecha de nacimiento [FECHANAC]\n\nTratamientos realizados:\n[TRATAMIENTOS]\n\nTratamientos pendientes:\n[PENDIENTES]\n\nAlertas médicas: [ALERTAS]\n\n[DOCTOR] · [FECHA]",
            false,
        ),
    ]
}

fn fmt_teeth(t: &[u8]) -> String {
    t.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("-")
}

impl Db {
    /// Sustituye los comodines de mensajes y documentos.
    pub fn fill(&self, text: &str, pid: u32, extra: &[(&str, String)]) -> String {
        let p = self.patient(pid).cloned().unwrap_or_default();
        let c = &self.config;
        let city = c.address.rsplit(' ').next().unwrap_or("").to_string();
        let done: Vec<String> = self.entries.iter().filter(|e| e.patient_id == pid && e.is_act()).map(|e| format!("· {} {} ({})", e.description, fmt_teeth(&e.teeth), fmt_date(e.date))).collect();
        let pend: Vec<String> = self.entries.iter().filter(|e| e.patient_id == pid && e.state == TxState::Previsto).map(|e| format!("· {} {}", e.description, fmt_teeth(&e.teeth))).collect();
        let mut s = text
            .replace("[NOMBRE]", &p.first_name)
            .replace("[APELLIDOS]", &p.last_name)
            .replace("[NIF]", if p.nif.is_empty() { "—" } else { &p.nif })
            .replace("[HISTORIAL]", &p.history_no.to_string())
            .replace("[FECHANAC]", &p.birth.map(fmt_date).unwrap_or_default())
            .replace("[DIRECCION]", &format!("{}, {} {}", p.address, p.zip, p.city))
            .replace("[CLINICA]", &c.name)
            .replace("[TELCLINICA]", &c.phone)
            .replace("[DIRCLINICA]", &c.address)
            .replace("[CIUDAD]", &city)
            .replace("[ALERTAS]", if p.alerts.is_empty() && p.allergies.is_empty() { "ninguna" } else { &p.alerts })
            .replace("[TRATAMIENTOS]", &if done.is_empty() { "· (ninguno)".to_string() } else { done.join("\n") })
            .replace("[PENDIENTES]", &if pend.is_empty() { "· (ninguno)".to_string() } else { pend.join("\n") });
        for (k, v) in extra {
            s = s.replace(k, v);
        }
        if !s.contains("[FECHA]") && !s.contains("[DOCTOR]") {
            return s;
        }
        s.replace("[FECHA]", &long_date(today())).replace("[DOCTOR]", &self.doctor(p.doctor_id).map(|d| d.name.clone()).unwrap_or_default())
    }

    /// Texto de un mensaje para el paciente: como `fill`, pero sin los datos de
    /// salud ([`CLINICAL_PLACEHOLDERS`]), que no salen por SMS, WhatsApp ni correo.
    pub fn fill_message(&self, text: &str, pid: u32, extra: &[(&str, String)]) -> String {
        let clean = CLINICAL_PLACEHOLDERS.iter().fold(text.to_string(), |s, c| s.replace(c, ""));
        self.fill(&clean, pid, extra)
    }

    /// Recordatorios y tareas que crean solos todos los puestos al arrancar:
    /// si dos puestos los crearon a la vez, sobran los repetidos (los que no
    /// cumplen `keep`; si ninguno, el más antiguo se queda). Devuelve cuántos.
    pub fn drop_auto_duplicates(&mut self, keep: impl Fn(u32) -> bool) -> usize {
        let before = self.messages.len() + self.tasks.len();
        // primero los que se quedan (`keep`), luego los más antiguos
        let mut order: Vec<&Message> = self.messages.iter().collect();
        order.sort_by_key(|m| (!keep(m.id), m.id));
        let mut seen = std::collections::HashSet::new();
        let drop_msgs: std::collections::HashSet<u32> = order.into_iter().filter(|m| auto_message_key(m).is_some_and(|k| !seen.insert(k))).map(|m| m.id).collect();
        self.messages.retain(|m| !drop_msgs.contains(&m.id));
        let mut tasks: Vec<&crate::tasks::Task> = self.tasks.iter().filter(|t| !t.auto.is_empty()).collect();
        tasks.sort_by_key(|t| (!keep(t.id), t.id));
        let mut seen_tasks = std::collections::HashSet::new();
        let drop_tasks: std::collections::HashSet<u32> = tasks.into_iter().filter(|t| !seen_tasks.insert(t.auto.clone())).map(|t| t.id).collect();
        self.tasks.retain(|t| !drop_tasks.contains(&t.id));
        before - self.messages.len() - self.tasks.len()
    }

    fn has_msg(&self, kind: MsgKind, related: Option<u32>, pid: u32) -> bool {
        self.messages.iter().any(|m| m.kind == kind && m.related == related && m.patient_id == pid)
    }

    fn contact(&self, pid: u32, ch: Channel) -> Option<String> {
        let p = self.patient(pid)?;
        // sin consentimiento, o con el tratamiento limitado o los datos bloqueados: nada
        if !p.accepts_gdpr || p.restricted.is_some() || p.blocked.is_some() {
            return None;
        }
        match ch {
            Channel::Sms if p.accepts_sms && !p.mobile.is_empty() => Some(p.mobile.clone()),
            Channel::WhatsApp if p.accepts_whatsapp && !p.mobile.is_empty() => Some(p.mobile.clone()),
            Channel::Email if p.accepts_email && !p.email.is_empty() => Some(p.email.clone()),
            // si no acepta el canal preferido, se intenta SMS
            Channel::WhatsApp | Channel::Email if p.accepts_sms && !p.mobile.is_empty() => Some(p.mobile.clone()),
            _ => None,
        }
    }

    /// El paciente acepta recibir correos y tiene dirección (para el envío automático por correo).
    fn reachable_by_email(&self, pid: u32) -> bool {
        self.patient(pid).is_some_and(|p| p.accepts_gdpr && p.accepts_email && p.email.contains('@'))
    }

    /// Se le puede escribir por correo (acepta y tiene dirección).
    pub fn can_email(&self, pid: u32) -> bool {
        self.reachable_by_email(pid)
    }

    /// Crea el mensaje. `link` es (texto, dirección) de un enlace que se añade al
    /// final cuando el mensaje sale por correo o WhatsApp y la plantilla no lo lleva ya.
    fn push_msg(&mut self, t: &MsgTemplate, pid: u32, related: Option<u32>, extra: &[(&str, String)], link: Option<(&str, String)>) -> bool {
        // avisos de esta clase que el paciente no quiere recibir (por ningún canal)
        if self.patient(pid).is_some_and(|p| p.no_messages.contains(&t.kind)) {
            return false;
        }
        let Some(mut to) = self.contact(pid, t.channel) else { return false };
        let p = self.patient(pid).cloned().unwrap_or_default();
        let mut ch = t.channel;
        if (ch == Channel::WhatsApp && !p.accepts_whatsapp) || (ch == Channel::Email && (!p.accepts_email || p.email.is_empty())) {
            ch = Channel::Sms;
            to = p.mobile.clone();
        }
        let id = self.new_id();
        let mut text = self.fill_message(&t.text, pid, extra);
        let mut subject = self.fill_message(&t.subject, pid, extra);
        if ch == Channel::Email && subject.trim().is_empty() {
            subject = format!("Mensaje de {}", self.config.name);
        }
        if let Some((label, url)) = link
            && self.booking.links_in_messages
            && ch != Channel::Sms
            && !url.is_empty()
            && !text.contains(&url)
        {
            text = format!("{}\n\n{label}: {url}", text.trim_end());
        }
        // en lo que no es la atención de su cita o revisión, cómo darse de baja
        if ch == Channel::Email && matches!(t.kind, MsgKind::Cumpleanos | MsgKind::PresupuestoPendiente) {
            text = format!("{}\n\n—\nSi no quiere recibir estos mensajes, respóndanos con la palabra BAJA o llámenos al {}.", text.trim_end(), self.config.phone);
        }
        self.messages.push(Message {
            id,
            patient_id: pid,
            kind: t.kind,
            channel: ch,
            to,
            subject,
            text,
            created: now(),
            sent_at: None,
            state: MsgState::Pendiente,
            related,
            user: self.config.current_user.clone(),
            error: String::new(),
            attempts: 0,
            uncertain: false,
            sending: String::new(),
            sending_since: None,
            sent_by: String::new(),
        });
        true
    }

    /// Correos pendientes que el programa puede enviar: con menos de 3
    /// intentos fallidos, sin un envío en curso en otro puesto y sin la duda
    /// de si llegaron.
    pub fn pending_emails(&self) -> Vec<u32> {
        self.messages.iter().filter(|m| m.state == MsgState::Pendiente && m.channel == Channel::Email && m.to.contains('@') && m.attempts < 3 && !m.uncertain && m.sending.is_empty()).map(|m| m.id).collect()
    }

    /// Reserva para enviarlos con la marca `token` los correos de `ids` que
    /// siguen pendientes y libres. En red lo hace el servidor con los datos al
    /// día: dos puestos no se quedan con el mismo correo.
    ///
    /// Un envío reservado hace más de [`SENDING_LEASE_MIN`] minutos sin saber
    /// cómo terminó pasa a «no se sabe si llegó». Devuelve cuántos ha reservado.
    pub fn claim_emails(&mut self, ids: &[u32], token: &str, at: NaiveDateTime) -> u32 {
        for m in self.messages.iter_mut().filter(|m| !m.sending.is_empty() && m.sending_since.is_none_or(|s| at - s > Duration::minutes(SENDING_LEASE_MIN))) {
            m.sending.clear();
            m.sending_since = None;
            if m.state == MsgState::Pendiente {
                m.uncertain = true;
                m.error = "El puesto que lo estaba enviando no terminó (se cerró o perdió la conexión): puede que haya llegado.".into();
            }
        }
        let free = self.pending_emails();
        let mut n = 0;
        for m in self.messages.iter_mut().filter(|m| ids.contains(&m.id) && free.contains(&m.id)) {
            m.sending = token.to_string();
            m.sending_since = Some(at);
            n += 1;
        }
        n
    }

    /// Correos reservados con esta marca (los que este puesto tiene que enviar).
    pub fn claimed_emails(&self, token: &str) -> Vec<u32> {
        self.messages.iter().filter(|m| !token.is_empty() && m.sending == token && m.state == MsgState::Pendiente).map(|m| m.id).collect()
    }

    /// Cómo terminó el envío de un correo reservado con `token`.
    pub fn email_outcome(&mut self, id: u32, token: &str, result: &Result<(), crate::mail::SendError>) {
        let Some(m) = self.messages.iter_mut().find(|m| m.id == id && m.sending == token) else { return };
        m.sending.clear();
        m.sending_since = None;
        match result {
            Ok(()) => self.mark_sent(id),
            Err(crate::mail::SendError::NotSent(e)) => self.email_failed(id, e),
            Err(crate::mail::SendError::Uncertain(e)) => {
                m.attempts += 1;
                m.uncertain = true;
                m.error = e.clone();
            }
        }
    }

    /// Resultado de enviar un SMS por la pasarela (como el de un correo):
    /// enviado, fallido (se puede reintentar) o incierto (no se reintenta solo).
    pub fn sms_outcome(&mut self, id: u32, result: &Result<(), crate::mail::SendError>) {
        match result {
            Ok(()) => self.mark_sent(id),
            Err(crate::mail::SendError::NotSent(e)) => self.email_failed(id, e),
            Err(crate::mail::SendError::Uncertain(e)) => {
                if let Some(m) = self.messages.iter_mut().find(|m| m.id == id) {
                    m.attempts += 1;
                    m.uncertain = true;
                    m.error = e.clone();
                }
            }
        }
    }

    /// Decide un correo del que no se sabía si llegó: consta como enviado, o
    /// vuelve a pendiente para enviarlo otra vez.
    pub fn resolve_uncertain(&mut self, id: u32, arrived: bool) {
        let Some(m) = self.messages.iter_mut().find(|m| m.id == id && m.uncertain) else { return };
        m.uncertain = false;
        if arrived {
            self.mark_sent(id);
        } else {
            m.error.clear();
            m.attempts = 0;
        }
    }

    /// Apunta un intento de envío por correo que ha fallado.
    pub fn email_failed(&mut self, id: u32, err: &str) {
        if let Some(m) = self.messages.iter_mut().find(|m| m.id == id) {
            m.attempts += 1;
            m.error = err.to_string();
        }
    }

    /// Genera los mensajes automáticos del día según las plantillas activas.
    /// Devuelve (creados, omitidos por falta de consentimiento o contacto).
    pub fn generate_messages(&mut self, day: NaiveDate) -> (usize, usize) {
        // a quien ya tiene cita no se le pide que la pida, y a quien la anuló, sí
        self.refresh_recalls(day);
        let mut created = 0;
        let mut skipped = 0;
        let mut templates: Vec<MsgTemplate> = self.msg_templates.iter().filter(|t| t.active).cloned().collect();
        // con el envío automático por correo, primero el correo a quien lo acepta;
        // a los demás les llega por la plantilla de WhatsApp o SMS, como siempre
        let auto_mail = self.mail.auto_send && self.mail.ready();
        if auto_mail {
            templates.sort_by_key(|t| t.channel != Channel::Email);
        }
        let mut done_appt_kind: Vec<(MsgKind, u32)> = vec![];
        for t in templates {
            match t.kind {
                MsgKind::RecordatorioCita => {
                    // `days` días laborables antes de la cita (como el WhatsApp
                    // automático): si el viernes se recuerdan las del lunes, no
                    // se pierden aunque el ordenador no se encienda el fin de
                    // semana. Con 0 días, las de hoy.
                    let due = |a: &Appointment| if t.days <= 0 { a.date == day } else { a.date > day && self.wa_send_day(a.date, t.days as u32) <= day };
                    let appts: Vec<Appointment> = self.appointments.iter().filter(|a| due(a) && a.patient_id.is_some() && !a.state.is_cancelled() && a.state != ApptState::Finalizada).cloned().collect();
                    for a in appts {
                        let pid = a.patient_id.unwrap();
                        // con WhatsApp automático activo, a estos pacientes se lo envía el programa
                        if self.wa_auto_on() && self.patient(pid).is_some_and(|p| p.accepts_gdpr && p.accepts_whatsapp && crate::whatsapp::intl(&p.mobile).is_some()) {
                            continue;
                        }
                        if done_appt_kind.contains(&(t.kind, a.id)) || self.has_msg(t.kind, Some(a.id), pid) {
                            continue;
                        }
                        if auto_mail && t.channel == Channel::Email && !self.reachable_by_email(pid) {
                            continue;
                        }
                        // enlace personal de la cita (si se usa en el texto o se añade al final, y la página está publicada)
                        let wants = t.text.contains("[ENLACE]") || (self.booking.links_in_messages && t.channel != Channel::Sms);
                        let link = if wants { self.appt_link_url(a.id).unwrap_or_default() } else { String::new() };
                        let extra = [
                            ("[FECHA]", format!("{} {}", weekday_es(a.date), a.date.format("%d/%m"))),
                            ("[HORA]", a.start.format("%H:%M").to_string()),
                            ("[DOCTOR]", self.doctor_alias(a.doctor_id)),
                            ("[MOTIVO]", a.reason.clone()),
                            ("[ENLACE]", link.clone()),
                        ];
                        if self.push_msg(&t, pid, Some(a.id), &extra, Some(("Confirme, cambie o anule su cita aquí", link))) {
                            created += 1;
                            done_appt_kind.push((t.kind, a.id));
                            if let Some(x) = self.appointments.iter_mut().find(|x| x.id == a.id) {
                                x.reminder_sent = true;
                            }
                        } else {
                            skipped += 1;
                        }
                    }
                }
                MsgKind::Recall => {
                    let limit = day + Duration::days(t.days.max(0) as i64);
                    let recs: Vec<Recall> = self.recalls.iter().filter(|r| self.recall_due(r) && r.date <= limit).cloned().collect();
                    for r in recs {
                        if self.has_msg(t.kind, Some(r.id), r.patient_id) {
                            continue;
                        }
                        if auto_mail && t.channel == Channel::Email && !self.reachable_by_email(r.patient_id) {
                            continue;
                        }
                        // la página para pedir cita por internet (si está publicada)
                        let page = self.booking_page_url().unwrap_or_default();
                        let extra = [("[MOTIVO]", r.reason.to_lowercase()), ("[DOCTOR]", self.doctor_alias(r.doctor_id)), ("[ENLACE_CITA]", page.clone())];
                        if self.push_msg(&t, r.patient_id, Some(r.id), &extra, Some(("Pida su cita por internet", page))) {
                            created += 1;
                        } else {
                            skipped += 1;
                        }
                    }
                }
                MsgKind::Cumpleanos => {
                    let pids: Vec<u32> = self.patients.iter().filter(|p| p.active && p.birth.map(|b| b.month() == day.month() && b.day() == day.day()).unwrap_or(false)).map(|p| p.id).collect();
                    for pid in pids {
                        let key = Some(day.year() as u32);
                        if self.has_msg(t.kind, key, pid) {
                            continue;
                        }
                        if auto_mail && t.channel == Channel::Email && !self.reachable_by_email(pid) {
                            continue;
                        }
                        if self.push_msg(&t, pid, key, &[], None) {
                            created += 1;
                        } else {
                            skipped += 1;
                        }
                    }
                }
                MsgKind::PresupuestoPendiente => {
                    let bs: Vec<Budget> = self.budgets.iter().filter(|b| b.state == BudgetState::Pendiente && (day - b.date).num_days() >= t.days as i64).cloned().collect();
                    for b in bs {
                        if self.has_msg(t.kind, Some(b.id), b.patient_id) {
                            continue;
                        }
                        if auto_mail && t.channel == Channel::Email && !self.reachable_by_email(b.patient_id) {
                            continue;
                        }
                        // enlace para revisar y firmar el presupuesto (si la página está publicada)
                        let wants = t.text.contains("[ENLACE_PRESUPUESTO]") || (self.booking.links_in_messages && t.channel != Channel::Sms);
                        let link = if wants { self.budget_link_url(b.id).unwrap_or_default() } else { String::new() };
                        let extra = [("[MOTIVO]", b.title.clone()), ("[IMPORTE]", money(b.total())), ("[DOCTOR]", self.doctor_alias(b.doctor_id)), ("[ENLACE_PRESUPUESTO]", link.clone())];
                        if self.push_msg(&t, b.patient_id, Some(b.id), &extra, Some(("Revise su presupuesto y acéptelo con su firma desde el móvil", link))) {
                            created += 1;
                        } else {
                            skipped += 1;
                        }
                    }
                }
                MsgKind::Libre | MsgKind::HuecoLibre => {}
            }
        }
        (created, skipped)
    }

    /// Marca un mensaje como enviado y actualiza recalls relacionados.
    pub fn mark_sent(&mut self, id: u32) {
        let user = self.config.current_user.clone();
        let Some(m) = self.messages.iter_mut().find(|m| m.id == id) else { return };
        m.state = MsgState::Enviado;
        m.sent_at = Some(now());
        m.error.clear();
        m.uncertain = false;
        m.sent_by = user;
        let (kind, related, ch) = (m.kind, m.related, m.channel);
        if kind == MsgKind::Recall
            && let Some(r) = self.recalls.iter_mut().find(|r| Some(r.id) == related)
        {
            r.state = RecallState::Contactado;
            r.channel = ch.label().into();
            r.contacted_on = Some(today());
            r.comment = format!("Aviso enviado por {}", ch.label());
        }
    }

    /// Crea un documento del paciente a partir de una plantilla.
    pub fn new_document(&mut self, pid: u32, template_id: u32, teeth: &[u8]) -> Option<u32> {
        let t = self.doc_templates.iter().find(|t| t.id == template_id)?.clone();
        let doctor = self.patient(pid).map(|p| p.doctor_id).unwrap_or(1);
        let extra = [("[PIEZAS]", if teeth.is_empty() { "indicada".to_string() } else { fmt_teeth(teeth) })];
        let body = self.fill(&t.body, pid, &extra);
        let id = self.new_id();
        self.documents.push(PatientDoc {
            id,
            patient_id: pid,
            template_id,
            title: t.name.clone(),
            category: t.category.clone(),
            date: today(),
            body,
            doctor_id: doctor,
            signed: false,
            signer: String::new(),
            signed_at: None,
            signature: vec![],
            ..Default::default()
        });
        Some(id)
    }

    pub fn sign_document(&mut self, id: u32, signer: &str, strokes: Vec<Vec<[f32; 2]>>) -> Result<(), String> {
        if strokes.iter().map(|s| s.len()).sum::<usize>() < 4 {
            return Err("Falta la firma del paciente.".into());
        }
        let Some(d) = self.documents.iter_mut().find(|d| d.id == id) else { return Err("Documento no encontrado".into()) };
        if d.signed {
            return Err("El documento ya está firmado.".into());
        }
        d.signed = true;
        d.signer = signer.to_string();
        d.signed_at = Some(now());
        d.signature = strokes;
        crate::firmado::seal_doc(d);
        let (pid, tid) = (d.patient_id, d.template_id);
        if self.doc_templates.iter().any(|t| t.id == tid && t.marks_gdpr)
            && let Some(p) = self.patient_mut(pid)
        {
            p.accepts_gdpr = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminders_generated_twice_are_left_once() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        let mut a = Db::sample(day);
        let mut b = a.clone();
        a.station = 1;
        b.station = 2;
        let (n, _) = a.generate_messages(day);
        b.generate_messages(day);
        assert!(n > 0);
        // lo que queda al juntar los dos: los de A (ya guardados) y los de B
        let theirs: std::collections::HashSet<u32> = a.messages.iter().map(|m| m.id).collect();
        let mut both = a.clone();
        let extra: Vec<Message> = b.messages.iter().filter(|m| !theirs.contains(&m.id)).cloned().collect();
        both.messages.extend(extra);
        assert_eq!(both.drop_auto_duplicates(|id| theirs.contains(&id)), n);
        assert_eq!(both.messages.len(), a.messages.len());
        assert!(both.messages.iter().all(|m| theirs.contains(&m.id)), "se quedan los ya guardados");
        // los escritos a mano no se tocan aunque se parezcan
        let mut m = both.messages[0].clone();
        m.id = both.new_id();
        m.kind = MsgKind::Libre;
        both.messages.push(m.clone());
        let id = both.new_id();
        both.messages.push(Message { id, ..m });
        assert_eq!(both.drop_auto_duplicates(|_| false), 0);
    }

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()
    }

    /// Base: solo una cita, de 102 (acepta todo), el día indicado.
    fn one_appointment(day: NaiveDate) -> (Db, u32) {
        let mut db = Db::sample(d());
        db.messages.clear();
        db.appointments.clear();
        let p = db.patients.iter_mut().find(|p| p.id == 102).unwrap();
        (p.accepts_gdpr, p.accepts_whatsapp, p.accepts_sms, p.mobile) = (true, true, true, "600 111 222".into());
        let id = db.new_id();
        db.appointments.push(Appointment {
            id,
            date: day,
            start: chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            minutes: 30,
            patient_id: Some(102),
            free_name: String::new(),
            phone: String::new(),
            doctor_id: 1,
            box_id: 1,
            reason: "Revisión".into(),
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
        (db, id)
    }

    /// El recordatorio «un día antes» es el día laborable anterior: el
    /// viernes se recuerdan las citas del lunes (antes, el viernes se
    /// recordaban las del sábado y las del lunes, el domingo, que con la
    /// clínica cerrada no llegaba a salir). Con un festivo en medio, igual.
    #[test]
    fn reminders_skip_weekends_and_holidays() {
        let friday = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();
        let monday = friday + Duration::days(3);
        assert!(!Db::sample(d()).working_day(friday + Duration::days(1)) && !Db::sample(d()).working_day(friday + Duration::days(2)), "fin de semana cerrado");
        let (mut db, appt) = one_appointment(monday);
        db.generate_messages(friday);
        assert!(db.messages.iter().any(|m| m.kind == MsgKind::RecordatorioCita && m.related == Some(appt)), "el viernes sale el de la cita del lunes");
        let n = db.messages.len();
        db.generate_messages(friday + Duration::days(1));
        db.generate_messages(monday);
        assert_eq!(db.messages.len(), n, "no se repite");
        // martes con el lunes festivo: también el viernes
        let (mut db, appt) = one_appointment(monday + Duration::days(1));
        db.holidays.push(crate::agenda_extra::Holiday { date: monday, name: "Fiesta".into() });
        db.generate_messages(friday);
        assert!(db.messages.iter().any(|m| m.related == Some(appt)));
        // y no antes de tiempo: la del miércoles, el viernes no
        let (mut db, appt) = one_appointment(monday + Duration::days(2));
        db.generate_messages(friday);
        assert!(!db.messages.iter().any(|m| m.related == Some(appt)));
    }

    /// Baja por finalidad: quien no quiere recordatorios de cita no los
    /// recibe por ningún canal, pero sí los demás avisos. Las felicitaciones
    /// y el seguimiento de presupuestos por correo dicen cómo darse de baja.
    #[test]
    fn a_patient_can_opt_out_of_one_kind_of_message() {
        let friday = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();
        let (mut db, appt) = one_appointment(friday + Duration::days(3));
        db.patients.iter_mut().find(|p| p.id == 102).unwrap().no_messages = vec![MsgKind::RecordatorioCita];
        let (_, skipped) = db.generate_messages(friday);
        assert!(!db.messages.iter().any(|m| m.related == Some(appt)), "no quiere recordatorios de cita");
        assert!(skipped > 0);
        let s = crate::whatsapp::WaSettings { enabled: true, ..Default::default() };
        assert!(db.wa_due(&s, friday.and_hms_opt(10, 30, 0).unwrap()).is_empty(), "tampoco por el WhatsApp automático");
        // un seguimiento de presupuesto por correo lleva cómo darse de baja
        let mut db = mail_db(true);
        db.generate_messages(d());
        let m = db.messages.iter().find(|m| m.kind == MsgKind::PresupuestoPendiente && m.channel == Channel::Email).unwrap();
        assert!(m.text.contains("BAJA"), "{}", m.text);
        let r = db.messages.iter().find(|m| m.kind == MsgKind::RecordatorioCita && m.channel == Channel::Email).unwrap();
        assert!(!r.text.contains("BAJA"), "un recordatorio de cita no es publicidad");
    }

    /// Dos puestos que envían a la vez no se quedan con el mismo correo; un
    /// envío cortado sin confirmación no se reintenta solo, y un puesto que
    /// se cerró a medias deja sus correos como «no se sabe si llegó».
    #[test]
    fn emails_are_claimed_once_and_uncertain_ones_wait_for_a_decision() {
        use crate::mail::SendError;
        let mut db = mail_db(true);
        db.generate_messages(d());
        let ids = db.pending_emails();
        assert!(ids.len() >= 2);
        let t0 = now();
        assert_eq!(db.claim_emails(&ids, "A", t0) as usize, ids.len());
        assert_eq!(db.claim_emails(&ids, "B", t0), 0, "ya los tiene otro puesto");
        assert!(db.pending_emails().is_empty() && db.claimed_emails("B").is_empty());
        assert_eq!(db.claimed_emails("A").len(), ids.len());
        let (a, b, c) = (ids[0], ids[1], ids.get(2).copied());
        // el resultado de otra marca no cuenta
        db.email_outcome(a, "B", &Ok(()));
        assert_eq!(db.messages.iter().find(|m| m.id == a).unwrap().state, MsgState::Pendiente);
        db.email_outcome(a, "A", &Ok(()));
        db.email_outcome(b, "A", &Err(SendError::Uncertain("se cortó".into())));
        let mb = db.messages.iter().find(|m| m.id == b).unwrap().clone();
        assert!(mb.uncertain && mb.state == MsgState::Pendiente && !db.pending_emails().contains(&b), "no se reintenta solo");
        // si llegó, consta como enviado; si no, vuelve a la cola
        db.resolve_uncertain(b, false);
        assert!(db.pending_emails().contains(&b));
        db.claim_emails(&[b], "A2", t0);
        db.email_outcome(b, "A2", &Err(SendError::Uncertain("se cortó".into())));
        db.config.current_user = "Recepción".into();
        db.resolve_uncertain(b, true);
        let mb = db.messages.iter().find(|m| m.id == b).unwrap();
        assert_eq!((mb.state, mb.sent_by.as_str(), mb.uncertain), (MsgState::Enviado, "Recepción", false));
        // un puesto que se cerró a medias: pasado el plazo, «no se sabe si llegó»
        if let Some(c) = c {
            db.claim_emails(&[c], "C", t0);
            assert_eq!(db.claim_emails(&[c], "D", t0 + Duration::minutes(SENDING_LEASE_MIN + 1)), 0);
            let mc = db.messages.iter().find(|m| m.id == c).unwrap();
            assert!(mc.uncertain && mc.sending.is_empty());
        }
        // lo que seguro que no salió sí se reintenta
        let id = db.pending_emails().first().copied();
        if let Some(id) = id {
            db.claim_emails(&[id], "E", t0);
            db.email_outcome(id, "E", &Err(SendError::NotSent("sin conexión".into())));
            assert!(db.pending_emails().contains(&id));
        }
    }

    /// Las plantillas de serie solo usan comodines de su clase; uno mal
    /// escrito, o de otra clase, se señala; y un dato de salud nunca sale en
    /// un mensaje aunque la plantilla lo lleve.
    #[test]
    fn templates_only_use_placeholders_that_are_filled_and_no_health_data() {
        for t in default_msg_templates() {
            assert!(unknown_placeholders(&format!("{} {}", t.subject, t.text), t.kind).is_empty(), "{}", t.name);
        }
        assert_eq!(unknown_placeholders("Hola [NOMBER], su [MOTIVO] a las [HORA]", MsgKind::Recall), vec!["[NOMBER]", "[HORA]"]);
        assert_eq!(unknown_placeholders("Alergias: [ALERTAS] [a] [x]", MsgKind::Libre), vec!["[ALERTAS]"]);
        let mut db = Db::sample(d());
        let p = db.patients.iter_mut().find(|p| p.id == 102).unwrap();
        p.alerts = "Alergia a la penicilina".into();
        let out = db.fill_message("Hola [NOMBRE]. Alertas: [ALERTAS]. Pendiente: [PENDIENTES]", 102, &[]);
        assert!(!out.contains("penicilina") && !out.contains('['), "{out}");
        assert!(db.fill("[ALERTAS]", 102, &[]).contains("penicilina"), "los documentos sí lo llevan");
        // y al generar los mensajes del día
        let (mut db, appt) = one_appointment(d() + Duration::days(1));
        db.patients.iter_mut().find(|p| p.id == 102).unwrap().alerts = "Alergia a la penicilina".into();
        for t in db.msg_templates.iter_mut().filter(|t| t.kind == MsgKind::RecordatorioCita) {
            t.text.push_str(" [ALERTAS]");
        }
        db.generate_messages(d());
        let m = db.messages.iter().find(|m| m.related == Some(appt)).unwrap();
        assert!(!m.text.contains("penicilina"), "{}", m.text);
    }

    #[test]
    fn reminders_respect_consent_and_no_duplicates() {
        let mut db = Db::sample(d());
        let (c1, _) = db.generate_messages(d());
        assert!(c1 > 0);
        let (c2, _) = db.generate_messages(d());
        assert_eq!(c2, 0, "no debe duplicar mensajes");
        for m in &db.messages {
            let p = db.patient(m.patient_id).unwrap();
            assert!(p.accepts_gdpr);
            assert!(!m.text.contains('['), "comodín sin sustituir: {}", m.text);
        }
    }

    /// Base para las pruebas del envío por correo: página publicada, correo configurado y dos
    /// pacientes con cita mañana: 102 acepta correo; 100 sólo WhatsApp.
    fn mail_db(auto: bool) -> Db {
        let mut db = Db::sample(d());
        db.messages.clear();
        db.booking.enabled = true;
        db.booking.public_url = "https://citas.example.com".into();
        db.mail = crate::mail::MailConfig { enabled: true, host: "smtp.example.com".into(), from_email: "clinica@example.com".into(), auto_send: auto, ..Default::default() };
        let tomorrow = d() + Duration::days(1);
        db.appointments.retain(|a| a.date != tomorrow);
        for p in db.patients.iter_mut().filter(|p| p.id == 100 || p.id == 102) {
            p.accepts_gdpr = true;
            p.accepts_whatsapp = true;
            p.accepts_sms = true;
            p.mobile = "600 111 222".into();
            p.accepts_email = p.id == 102;
        }
        for (k, pid) in [100u32, 102].into_iter().enumerate() {
            let id = db.new_id();
            db.appointments.push(Appointment {
                id,
                date: tomorrow,
                start: chrono::NaiveTime::from_hms_opt(10 + k as u32, 0, 0).unwrap(),
                minutes: 30,
                patient_id: Some(pid),
                free_name: String::new(),
                phone: String::new(),
                doctor_id: 1,
                box_id: 1,
                reason: "Revisión".into(),
                treatment_code: "0102".into(),
                state: ApptState::Planificada,
                notes: String::new(),
                arrived_at: None,
                seen_at: None,
                finished_at: None,
                reminder_sent: false,
                first_visit: false,
                center_id: 1,
            });
        }
        // 105, sin citas, acepta correo: le toca la revisión
        db.appointments.retain(|a| a.patient_id != Some(105) || a.date < d());
        for p in db.patients.iter_mut().filter(|p| p.id == 105) {
            p.accepts_gdpr = true;
            p.accepts_email = true;
            p.email = "paciente105@example.com".into();
        }
        // un presupuesto pendiente de hace 10 días, de 102
        let bid = db.new_id();
        db.budgets.push(Budget {
            id: bid,
            patient_id: 102,
            title: "Ortodoncia".into(),
            date: d() - Duration::days(10),
            doctor_id: 3,
            tariff_id: 1,
            state: BudgetState::Pendiente,
            lines: vec![BudgetLine { code: "0101".into(), description: "Estudio".into(), teeth: vec![], units: 1, price: 90.0, discount_pct: 0.0, odg: OdgKind::Ninguno, phase: 0 }],
            valid_days: 90,
            accepted_on: None,
            rejected_on: None,
            reject_reason: String::new(),
            notes: String::new(),
            plan: None,
            tp: Default::default(),
        });
        let rid = db.new_id();
        db.recalls.push(Recall { id: rid, patient_id: 105, date: d(), reason: "Higiene anual".into(), doctor_id: 4, state: RecallState::Pendiente, channel: String::new(), comment: String::new(), contacted_on: None, appt_id: None });
        // la de 102 no se le pide: ya tiene cita mañana
        let rid = db.new_id();
        db.recalls.push(Recall { id: rid, patient_id: 102, date: d(), reason: "Revisión semestral".into(), doctor_id: 1, state: RecallState::Pendiente, channel: String::new(), comment: String::new(), contacted_on: None, appt_id: None });
        db
    }

    #[test]
    fn automatic_email_goes_first_with_links() {
        let mut db = mail_db(true);
        db.generate_messages(d());
        let msg = |db: &Db, kind: MsgKind, pid: u32| db.messages.iter().find(|m| m.kind == kind && m.patient_id == pid).cloned().unwrap();
        let a = msg(&db, MsgKind::RecordatorioCita, 102);
        assert_eq!(a.channel, Channel::Email, "acepta correo: le llega por correo");
        assert!(a.text.contains("https://citas.example.com/mi-cita/"), "con el enlace de su cita: {}", a.text);
        assert!(!a.subject.is_empty() && !a.subject.contains('['));
        let b = msg(&db, MsgKind::RecordatorioCita, 100);
        assert_eq!(b.channel, Channel::WhatsApp, "sin correo, por WhatsApp como siempre (no por SMS)");
        assert!(b.text.contains("/mi-cita/"), "también con su enlace");
        let p = msg(&db, MsgKind::PresupuestoPendiente, 102);
        assert_eq!(p.channel, Channel::Email);
        assert!(p.text.contains("https://citas.example.com/presupuesto/"), "seguimiento con el enlace para firmarlo: {}", p.text);
        let r = msg(&db, MsgKind::Recall, 105);
        assert_eq!(r.channel, Channel::Email);
        assert!(!db.messages.iter().any(|m| m.kind == MsgKind::Recall && m.patient_id == 102), "quien ya tiene cita no recibe el «pida su cita»");
        assert_eq!(db.recalls.iter().find(|x| x.patient_id == 102 && x.reason == "Revisión semestral").unwrap().state, RecallState::Citado);
        assert!(r.text.contains("Pida su cita por internet: https://citas.example.com/"), "{}", r.text);
        assert!(db.pending_emails().contains(&a.id) && !db.pending_emails().contains(&b.id));
        // tras tres intentos fallidos ya no se reintenta solo
        for _ in 0..3 {
            db.email_failed(a.id, "buzón lleno");
        }
        assert!(!db.pending_emails().contains(&a.id));
        db.mark_sent(a.id);
        assert!(db.messages.iter().find(|m| m.id == a.id).unwrap().error.is_empty());
    }

    /// Los datos guardados con una versión anterior reciben las plantillas de correo una sola vez.
    #[test]
    fn old_data_gets_the_email_templates_once() {
        let mut db = Db::sample(d());
        db.msg_templates.retain(|t| t.channel != Channel::Email || t.kind == MsgKind::PresupuestoPendiente);
        db.msg_templates.push(MsgTemplate { id: 6, name: "Mía".into(), kind: MsgKind::Libre, channel: Channel::Sms, days: 0, text: "x".into(), subject: String::new(), active: true });
        db.data_version = 0;
        assert!(db.upgrade());
        let mails: Vec<&MsgTemplate> = db.msg_templates.iter().filter(|t| t.channel == Channel::Email && matches!(t.kind, MsgKind::RecordatorioCita | MsgKind::Recall)).collect();
        assert_eq!(mails.len(), 2);
        let mut ids: Vec<u32> = db.msg_templates.iter().map(|t| t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), db.msg_templates.len(), "sin repetir números de plantilla");
        let n = db.msg_templates.len();
        assert!(!db.upgrade(), "sólo una vez");
        assert_eq!(db.msg_templates.len(), n);
        // si la clínica ya tenía su propia plantilla de recordatorio por correo, no se duplica
        let mut db = Db::sample(d());
        db.data_version = 0;
        let before = db.msg_templates.len();
        db.upgrade();
        assert_eq!(db.msg_templates.len(), before);
    }

    #[test]
    fn without_automatic_email_order_is_unchanged() {
        let mut db = mail_db(false);
        db.booking.links_in_messages = false;
        db.generate_messages(d());
        let a = db.messages.iter().find(|m| m.kind == MsgKind::RecordatorioCita && m.patient_id == 102).unwrap();
        assert_eq!(a.channel, Channel::WhatsApp, "sin envío automático manda la primera plantilla (WhatsApp)");
        assert!(!a.text.contains("http"), "sin enlaces si la clínica no los quiere");
    }

    #[test]
    fn a_patient_who_left_gets_no_checkup_reminder() {
        let mut db = mail_db(true);
        db.patients.iter_mut().find(|p| p.id == 105).unwrap().active = false;
        db.generate_messages(d());
        assert!(!db.messages.iter().any(|m| m.kind == MsgKind::Recall && m.patient_id == 105), "de baja: sin avisos de revisión");
        let r = db.recalls.iter().find(|r| r.patient_id == 105).unwrap();
        assert!(!db.recall_due(r), "ni en la lista de llamadas");
        // si vuelve, vuelve a la lista
        db.patients.iter_mut().find(|p| p.id == 105).unwrap().active = true;
        let r = db.recalls.iter().find(|r| r.patient_id == 105).unwrap();
        assert!(db.recall_due(r));
    }

    #[test]
    fn sending_recall_message_marks_contacted() {
        let mut db = Db::sample(d());
        db.generate_messages(d());
        if let Some(m) = db.messages.iter().find(|m| m.kind == MsgKind::Recall).cloned() {
            db.mark_sent(m.id);
            let r = db.recalls.iter().find(|r| Some(r.id) == m.related).unwrap();
            assert_eq!(r.state, RecallState::Contactado);
        }
    }

    #[test]
    fn signing_gdpr_document_updates_patient() {
        let mut db = Db::sample(d());
        let pid = db.patients.iter().find(|p| !p.accepts_gdpr).unwrap().id;
        let id = db.new_document(pid, 1, &[]).unwrap();
        assert!(!db.documents.iter().find(|x| x.id == id).unwrap().body.contains("[NOMBRE]"));
        assert!(db.sign_document(id, "Paciente", vec![]).is_err());
        db.sign_document(id, "Paciente", vec![vec![[0.1, 0.5], [0.2, 0.4], [0.3, 0.6], [0.4, 0.5]]]).unwrap();
        assert!(db.patient(pid).unwrap().accepts_gdpr);
    }
}
