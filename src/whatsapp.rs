//! WhatsApp automático con la plataforma oficial de Meta (WhatsApp Business,
//! Cloud API): recordatorio de cita con dos botones («Sí, confirmo» / «No
//! puedo ir») y lectura automática de la respuesta del paciente.
//!
//! Cómo funciona:
//! 1. Un solo ordenador de la clínica («el que envía») revisa cada minuto qué
//!    citas toca recordar (por defecto el día laborable anterior, en horario de
//!    envío) y manda la plantilla aprobada por Meta con nombre, clínica, fecha
//!    y hora. Así nunca se envía dos veces desde dos ordenadores.
//! 2. Meta avisa de cada respuesta (y de si el mensaje se entregó o se leyó) a
//!    una dirección web de ese ordenador, publicada en internet con un túnel
//!    https. Cada aviso se comprueba con la clave secreta de la app (firma
//!    HMAC-SHA256) y los repetidos se descartan (Meta reintenta 7 días).
//! 3. «Sí» → la cita queda Confirmada. «No» → se anula (libera el hueco), se
//!    crea una tarea para darle otra fecha y se avisa a recepción. Otro texto →
//!    tarea para que recepción lo lea. Opcionalmente se contesta «gracias».
//!
//! La respuesta se asocia a su cita por el identificador del mensaje al que
//! responde el paciente (funciona aunque Meta no envíe su teléfono); si no lo
//! trae, por su identificador de WhatsApp o por su número.

use crate::model::*;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

pub const API_BASE: &str = "https://graph.facebook.com/v27.0";
pub const WEBHOOK_PORT: u16 = 8896;
const KEEP_SEEN: usize = 3000;
const MAX_BODY: u64 = 1024 * 1024;

/// Texto de la plantilla que hay que crear en Meta (categoría Utilidad).
pub const TEMPLATE_TEXT: &str = "Hola {{nombre}}, le recordamos su cita en {{clinica}} el {{fecha}} a las {{hora}}. ¿Podrá venir?";
pub const BUTTON_YES: &str = "Sí, confirmo";
/// Principio del error de un envío del que no se sabe si llegó (la petición
/// salió y se cortó la conexión sin respuesta).
pub const UNCERTAIN: &str = "No se sabe si llegó";
pub const BUTTON_NO: &str = "No puedo ir";
/// Texto del recordatorio cuando se envía por la pasarela (mensaje normal).
pub const GW_TEXT: &str = "Hola [NOMBRE], le recordamos su cita en [CLINICA] el [FECHA] a las [HORA]. Por favor, responda SÍ para confirmarla o NO si no puede venir. ¡Gracias!";
/// Precio oficial de Meta en España por recordatorio entregado (Utilidad, octubre de 2026).
pub const META_EUR_PER_MSG: f64 = 0.0166;

/// Cómo se conecta el programa a WhatsApp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    /// Plataforma oficial de WhatsApp Business (Meta): de pago por mensaje entregado.
    #[default]
    Meta,
    /// WhatsApp de la clínica enlazado a una pasarela gratuita (no oficial).
    Pasarela,
}

/// Pasarelas gratuitas compatibles (se instalan en un ordenador de la clínica).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GwKind {
    #[default]
    Waha,
    Evolution,
}

impl GwKind {
    pub fn label(self) -> &'static str {
        match self {
            GwKind::Waha => "WAHA",
            GwKind::Evolution => "Evolution API",
        }
    }
    pub fn default_url(self) -> &'static str {
        match self {
            GwKind::Waha => "http://127.0.0.1:3000",
            GwKind::Evolution => "http://127.0.0.1:8080",
        }
    }
}

// ───────────────────────────── ajustes (solo en este ordenador) ─────────────────────────────

/// Ajustes del ordenador que envía. Llevan claves, así que no van en los
/// datos de la clínica sino en un fichero local de este ordenador.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WaSettings {
    pub enabled: bool,
    /// Identificador del número de teléfono de WhatsApp Business (Phone number ID).
    pub phone_number_id: String,
    /// Token de acceso permanente (usuario del sistema).
    pub token: String,
    /// Clave secreta de la app, para comprobar la firma de los avisos.
    pub app_secret: String,
    /// Token de verificación que se escribe en Meta al registrar la dirección.
    pub verify_token: String,
    pub template: String,
    pub language: String,
    /// Días laborables de antelación (1 = el día laborable anterior).
    pub days_before: u32,
    pub from_hour: u32,
    pub until_hour: u32,
    /// Contestar automáticamente «gracias» a quien confirma o anula.
    pub auto_reply: bool,
    /// Anular la cita (y liberar el hueco) cuando el paciente dice que no.
    pub cancel_on_no: bool,
    pub port: u16,
    pub api_base: String,
    pub provider: Provider,
    pub gw_kind: GwKind,
    /// Dirección de la pasarela (p. ej. http://127.0.0.1:3000).
    pub gw_url: String,
    /// Clave de la pasarela (X-Api-Key en WAHA, apikey en Evolution).
    pub gw_key: String,
    /// Sesión (WAHA) o instancia (Evolution).
    pub gw_session: String,
    /// Texto del recordatorio por la pasarela.
    pub gw_text: String,
    /// Pausa aleatoria entre recordatorios (segundos), para parecer un envío a mano.
    pub gw_min_pause: u32,
    pub gw_max_pause: u32,
    /// Máximo de recordatorios al día por la pasarela.
    pub gw_daily_max: u32,
    /// Escuchar los avisos en toda la red local (pasarela en Docker u otro ordenador).
    pub gw_lan: bool,
    /// La clínica acepta el riesgo de que WhatsApp bloquee el número.
    pub gw_risk_ok: bool,
}

impl Default for WaSettings {
    fn default() -> Self {
        WaSettings {
            enabled: false,
            phone_number_id: String::new(),
            token: String::new(),
            app_secret: String::new(),
            verify_token: String::new(),
            template: "recordatorio_cita".into(),
            language: "es".into(),
            days_before: 1,
            from_hour: 10,
            until_hour: 20,
            auto_reply: true,
            cancel_on_no: true,
            port: WEBHOOK_PORT,
            api_base: API_BASE.into(),
            provider: Provider::Meta,
            gw_kind: GwKind::Waha,
            gw_url: GwKind::Waha.default_url().into(),
            gw_key: String::new(),
            gw_session: "default".into(),
            gw_text: GW_TEXT.into(),
            gw_min_pause: 20,
            gw_max_pause: 60,
            gw_daily_max: 150,
            gw_lan: true,
            gw_risk_ok: false,
        }
    }
}

impl WaSettings {
    /// Qué falta para poder enviar.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut v = vec![];
        match self.provider {
            Provider::Meta => {
                if self.phone_number_id.trim().is_empty() {
                    v.push("identificador del número");
                }
                if self.token.trim().is_empty() {
                    v.push("token de acceso");
                }
                if self.app_secret.trim().is_empty() {
                    v.push("clave secreta de la app");
                }
                if self.template.trim().is_empty() {
                    v.push("nombre de la plantilla");
                }
            }
            Provider::Pasarela => {
                if self.gw_url.trim().is_empty() {
                    v.push("dirección de la pasarela");
                }
                if self.gw_session.trim().is_empty() {
                    v.push(if self.gw_kind == GwKind::Waha { "sesión de la pasarela" } else { "instancia de la pasarela" });
                }
                if self.gw_text.trim().is_empty() {
                    v.push("texto del recordatorio");
                }
                if self.gw_min_pause > self.gw_max_pause {
                    v.push("pausa mínima menor que la máxima");
                }
                if !self.gw_risk_ok {
                    v.push("aceptar el riesgo de bloqueo del número");
                }
            }
        }
        if self.from_hour >= self.until_hour {
            v.push("horario de envío válido");
        }
        v
    }
}

fn settings_path() -> PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("whatsapp.json")
}

pub fn load_settings() -> WaSettings {
    std::fs::read_to_string(settings_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_settings(s: &WaSettings) -> Result<(), String> {
    let p = settings_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

// ───────────────────────────── registro de mensajes (datos de la clínica) ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaState {
    Enviando,
    Enviado,
    Entregado,
    Leido,
    Confirmada,
    NoViene,
    Respuesta,
    Fallido,
}

impl WaState {
    pub fn label(self) -> &'static str {
        match self {
            WaState::Enviando => "Enviando",
            WaState::Enviado => "Enviado",
            WaState::Entregado => "Entregado",
            WaState::Leido => "Leído",
            WaState::Confirmada => "Confirmó",
            WaState::NoViene => "No puede venir",
            WaState::Respuesta => "Respondió (revisar)",
            WaState::Fallido => "No se pudo enviar",
        }
    }
    /// Enviado y todavía sin respuesta.
    pub fn waiting(self) -> bool {
        matches!(self, WaState::Enviado | WaState::Entregado | WaState::Leido)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaMessage {
    pub id: u32,
    pub appt_id: u32,
    pub patient_id: u32,
    /// Número al que se envió (internacional, sin «+»).
    pub to: String,
    /// Identificador que da Meta al mensaje (wamid).
    pub wamid: String,
    /// Identificador de WhatsApp del paciente (BSUID), cuando Meta lo comunica.
    pub user_id: String,
    pub created: NaiveDateTime,
    pub sent_at: Option<NaiveDateTime>,
    pub state: WaState,
    pub reply: String,
    pub reply_at: Option<NaiveDateTime>,
    pub error: String,
    /// Se cortó el envío sin respuesta: puede que haya llegado. No se reenvía
    /// solo (lo que conteste el paciente se recoge igual).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub uncertain: bool,
}

/// Parte propia de un identificador de mensaje: WAHA usa «true_<chat>_<id>».
pub fn core_id(s: &str) -> &str {
    s.split('_').nth(2).unwrap_or(s)
}

/// ¿Es el mismo mensaje aunque venga escrito de otra forma?
pub fn same_msg_id(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a == b || core_id(a) == core_id(b))
}

/// Número internacional sin «+» (34 + 9 cifras para España).
pub fn intl(number: &str) -> Option<String> {
    let d = crate::phone::normalize(number);
    match d.len() {
        9 => Some(format!("34{d}")),
        n if n > 9 => Some(d),
        _ => None,
    }
}

/// «lunes 9 de noviembre».
pub fn date_text(d: NaiveDate) -> String {
    format!("{} {} de {}", weekday_es(d), d.day(), month_es(d.month()))
}

/// ¿Pide no recibir más mensajes («BAJA», «STOP», «darme de baja»…)? Vale
/// también la palabra que dice el pie de cada idioma («BAIXA» en catalán y
/// gallego) y las formas habituales de decirlo en esos idiomas.
pub fn is_opt_out(text: &str) -> bool {
    let t = fold(text).replace(['\'', '’'], "");
    let t = t.trim().trim_matches(|c: char| !c.is_alphanumeric());
    const WORDS: [&str; 4] = ["baja", "baixa", "stop", "unsubscribe"];
    const PHRASES: [&str; 14] = [
        "de baja",
        "no quiero recibir",
        "no me envie",
        "no me mande",
        "no quiero mas mensajes",
        "no quiero mensajes",
        // catalán y gallego
        "de baixa",
        "no vull rebre",
        "no em envieu",
        "non quero recibir",
        "non me envien",
        // euskera e inglés
        "ez dut jaso nahi",
        "do not want to receive",
        "dont want to receive",
    ];
    WORDS.contains(&t) || PHRASES.iter().any(|k| t.contains(k))
}

/// ¿Dice que sí (Some(true)), que no (Some(false)) o no está claro (None)?
pub fn intent(text: &str) -> Option<bool> {
    let t = fold(text);
    let words: Vec<&str> = t.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has = |list: &[&str]| words.iter().any(|w| list.contains(w));
    let yes = has(&["si", "confirmo", "confirmar", "confirmada", "confirmado", "vale", "ok", "okay", "perfecto", "acuerdo", "estare", "ire", "voy", "claro", "correcto", "genial"]) || t.contains('👍') || t.contains('✅');
    let no = has(&["no", "anular", "anulo", "anula", "anulen", "cancelar", "cancelo", "cancela", "cancelen", "imposible", "cambiar", "aplazar", "retrasar", "mover"]) || t.contains('❌') || t.contains('👎');
    match (yes, no) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// Petición para enviar la plantilla de recordatorio (variables con nombre).
pub fn template_request(s: &WaSettings, to: &str, nombre: &str, clinica: &str, fecha: &str, hora: &str) -> Value {
    let p = |name: &str, v: &str| json!({ "type": "text", "parameter_name": name, "text": v });
    json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": to,
        "type": "template",
        "template": {
            "name": s.template.trim(),
            "language": { "code": s.language.trim() },
            "components": [{
                "type": "body",
                "parameters": [p("nombre", nombre), p("clinica", clinica), p("fecha", fecha), p("hora", hora)]
            }]
        }
    })
}

/// Mensaje de texto libre (solo en las 24 h siguientes a un mensaje del paciente).
pub fn text_request(to: &str, text: &str) -> Value {
    json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": to,
        "type": "text",
        "text": { "preview_url": false, "body": text }
    })
}

// ───────────────────────────── avisos de Meta (webhook) ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum WaEvent {
    /// Mensaje del paciente (botón o texto).
    Message { id: String, from: String, user_id: String, name: String, context: String, text: String, button: bool },
    /// Cambio de estado de un mensaje enviado: sent, delivered, read, failed.
    Status { wamid: String, status: String, user_id: String, error: String },
}

fn jstr(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// Lee el cuerpo de un aviso de Meta.
pub fn parse_webhook(body: &str) -> Vec<WaEvent> {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return vec![] };
    let mut out = vec![];
    for entry in v["entry"].as_array().into_iter().flatten() {
        for change in entry["changes"].as_array().into_iter().flatten() {
            let val = &change["value"];
            let name = jstr(&val["contacts"][0]["profile"]["name"]);
            for m in val["messages"].as_array().into_iter().flatten() {
                let kind = m["type"].as_str().unwrap_or("");
                let (text, button) = match kind {
                    "button" => (if jstr(&m["button"]["text"]).is_empty() { jstr(&m["button"]["payload"]) } else { jstr(&m["button"]["text"]) }, true),
                    "text" => (jstr(&m["text"]["body"]), false),
                    "interactive" => {
                        let i = &m["interactive"];
                        let t = if i["type"] == "list_reply" { jstr(&i["list_reply"]["title"]) } else { jstr(&i["button_reply"]["title"]) };
                        (t, true)
                    }
                    "reaction" => (jstr(&m["reaction"]["emoji"]), false),
                    other => (format!("[{}]", if other.is_empty() { "mensaje" } else { other }), false),
                };
                out.push(WaEvent::Message { id: jstr(&m["id"]), from: jstr(&m["from"]), user_id: jstr(&m["from_user_id"]), name: name.clone(), context: jstr(&m["context"]["id"]), text, button });
            }
            for st in val["statuses"].as_array().into_iter().flatten() {
                let e = &st["errors"][0];
                let error = if e.is_null() {
                    String::new()
                } else {
                    let details = jstr(&e["error_data"]["details"]);
                    let title = if jstr(&e["title"]).is_empty() { jstr(&e["message"]) } else { jstr(&e["title"]) };
                    if details.is_empty() || details == title { title } else { format!("{title}: {details}") }
                };
                out.push(WaEvent::Status { wamid: jstr(&st["id"]), status: jstr(&st["status"]), user_id: jstr(&st["recipient_user_id"]), error });
            }
        }
    }
    out
}

/// HMAC-SHA256 (RFC 2104).
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut k = [0u8; 64];
    if key.len() > 64 {
        let h = Sha256::digest(key);
        k[..32].copy_from_slice(&h);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(msg);
    let ih = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(ih);
    let oh = outer.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&oh);
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Comprueba la cabecera `X-Hub-Signature-256: sha256=<hex>`.
pub fn verify_signature(app_secret: &str, body: &[u8], header: &str) -> bool {
    let Some(given) = header.trim().strip_prefix("sha256=") else { return false };
    if app_secret.is_empty() {
        return false;
    }
    let want = hex(&hmac_sha256(app_secret.as_bytes(), body));
    let given = given.trim().to_ascii_lowercase();
    // comparación en tiempo constante
    given.len() == want.len() && given.bytes().zip(want.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

fn percent_decode(s: &str) -> String {
    let hexd = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hexd(b[i + 1]), hexd(b[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn query_param(url: &str, key: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    q.split('&').find_map(|kv| kv.split_once('=').filter(|(k, _)| percent_decode(k) == key).map(|(_, v)| percent_decode(v)))
}

/// Respuesta a la verificación de Meta (GET): el «challenge» si el token coincide.
pub fn verify_challenge(url: &str, verify_token: &str) -> Option<String> {
    let mode = query_param(url, "hub.mode")?;
    let token = query_param(url, "hub.verify_token")?;
    let challenge = query_param(url, "hub.challenge")?;
    (mode == "subscribe" && !verify_token.is_empty() && token == verify_token).then_some(challenge)
}

// ───────────────────────────── lógica sobre los datos ─────────────────────────────

impl Db {
    /// Hay un ordenador enviando WhatsApp automáticos.
    pub fn wa_auto_on(&self) -> bool {
        !self.wa_sender.trim().is_empty()
    }

    /// Último WhatsApp de una cita.
    pub fn wa_for_appt(&self, appt_id: u32) -> Option<&WaMessage> {
        self.whatsapp.iter().filter(|m| m.appt_id == appt_id).max_by_key(|m| m.id)
    }

    /// Día en que toca enviar: `n` días laborables antes de la cita.
    pub fn wa_send_day(&self, date: NaiveDate, n: u32) -> NaiveDate {
        let mut d = date;
        let mut k = 0;
        let mut guard = 0;
        while k < n.max(1) && guard < 60 {
            d -= Duration::days(1);
            guard += 1;
            if self.working_day(d) {
                k += 1;
            }
        }
        d
    }

    /// Citas a las que toca enviar el recordatorio ahora.
    pub fn wa_due(&self, s: &WaSettings, now: NaiveDateTime) -> Vec<u32> {
        let today = now.date();
        let h = now.hour();
        if h < s.from_hour || h >= s.until_hour {
            return vec![];
        }
        let mut v: Vec<&Appointment> = self
            .appointments
            .iter()
            .filter(|a| a.date >= today && a.state == ApptState::Planificada)
            .filter(|a| a.date.and_time(a.start) - now >= Duration::hours(2))
            .filter(|a| self.wa_send_day(a.date, s.days_before) <= today)
            .filter(|a| self.wa_for_appt(a.id).is_none())
            .filter(|a| a.patient_id.and_then(|p| self.patient(p)).is_some_and(|p| p.active && p.accepts_gdpr && p.accepts_whatsapp && !p.no_messages.contains(&crate::comms::MsgKind::RecordatorioCita) && intl(&p.mobile).is_some()))
            .collect();
        v.sort_by_key(|a| (a.date, a.start));
        let mut ids: Vec<u32> = v.into_iter().map(|a| a.id).collect();
        if s.provider == Provider::Pasarela {
            // límite diario para no llamar la atención de WhatsApp
            let used = self.whatsapp.iter().filter(|m| m.created.date() == today).count() as u32;
            ids.truncate(s.gw_daily_max.saturating_sub(used) as usize);
        }
        ids
    }

    /// Texto del recordatorio para la pasarela, con los datos de la cita.
    pub fn wa_text(&self, s: &WaSettings, a: &Appointment) -> String {
        let text = s.gw_text.replace("[CLINICA]", &self.clinic_for(a));
        let extra = [("[FECHA]", date_text(a.date)), ("[HORA]", a.start.format("%H:%M").to_string()), ("[DOCTOR]", self.doctor_alias(a.doctor_id)), ("[MOTIVO]", a.reason.clone()), ("[ENLACE]", self.existing_link_url(a.id).unwrap_or_default())];
        self.fill_message(&text, a.patient_id.unwrap_or(0), &extra)
    }

    fn clinic_for(&self, a: &Appointment) -> String {
        if self.multi_center() { self.center_name(cid(a.center_id)) } else { self.config.name.clone() }
    }

    /// Apunta el recordatorio («Enviando») y devuelve (id del registro, petición).
    pub fn wa_prepare(&mut self, s: &WaSettings, appt_id: u32, when: NaiveDateTime) -> Option<(u32, Value)> {
        let a = self.appointments.iter().find(|a| a.id == appt_id)?.clone();
        let p = self.patient(a.patient_id?)?.clone();
        let to = intl(&p.mobile)?;
        if s.gw_text.contains("[ENLACE]") {
            // crea el enlace de la cita para ponerlo en el mensaje
            self.appt_link_url(appt_id);
        }
        let body = match s.provider {
            Provider::Meta => template_request(s, &to, &p.first_name, &self.clinic_for(&a), &date_text(a.date), &a.start.format("%H:%M").to_string()),
            Provider::Pasarela => text_request(&to, &self.wa_text(s, &a)),
        };
        let id = self.new_id();
        self.whatsapp.push(WaMessage {
            id,
            appt_id,
            patient_id: p.id,
            to,
            wamid: String::new(),
            user_id: String::new(),
            created: when,
            sent_at: None,
            state: WaState::Enviando,
            reply: String::new(),
            reply_at: None,
            error: String::new(),
            uncertain: false,
        });
        Some((id, body))
    }

    /// Resultado del envío de un recordatorio.
    pub fn wa_sent(&mut self, log_id: u32, result: Result<String, String>, today: NaiveDate) {
        let Some(m) = self.whatsapp.iter_mut().find(|m| m.id == log_id) else { return };
        match result {
            Ok(wamid) => {
                m.wamid = wamid;
                m.sent_at = Some(now());
                if m.state == WaState::Enviando {
                    m.state = WaState::Enviado;
                }
                let appt = m.appt_id;
                if let Some(a) = self.appointments.iter_mut().find(|a| a.id == appt) {
                    a.reminder_sent = true;
                }
            }
            Err(e) if e.starts_with(UNCERTAIN) => {
                // puede que haya salido: se da por enviado (sus respuestas se
                // recogen por su número) y no se reenvía, pero recepción lo sabe
                m.state = WaState::Enviado;
                m.uncertain = true;
                m.error = e.clone();
                let (pid, appt) = (m.patient_id, m.appt_id);
                if let Some(a) = self.appointments.iter_mut().find(|a| a.id == appt) {
                    a.reminder_sent = true;
                }
                let when = self.appointments.iter().find(|a| a.id == appt).map(|a| format!("{} a las {}", fmt_date(a.date), a.start.format("%H:%M"))).unwrap_or_default();
                let who = self.patient_name(pid);
                self.add_task(today, "WhatsApp", "", Some(pid), &format!("No se sabe si llegó el WhatsApp de la cita del {when} de {who} (se cortó la conexión al enviarlo): si contesta, se recogerá; si no, llamar para confirmarla"), Some(today));
            }
            Err(e) => {
                m.state = WaState::Fallido;
                m.error = e.clone();
                let (pid, appt) = (m.patient_id, m.appt_id);
                let when = self.appointments.iter().find(|a| a.id == appt).map(|a| format!("{} a las {}", fmt_date(a.date), a.start.format("%H:%M"))).unwrap_or_default();
                let who = self.patient_name(pid);
                self.add_task(today, "WhatsApp", "", Some(pid), &format!("No se pudo enviar el WhatsApp de su cita del {when}: llamar a {who} para confirmarla ({e})"), Some(today));
            }
        }
    }

    /// Identificador oculto de WhatsApp (@lid) del paciente, para reconocer sus respuestas.
    pub fn wa_set_user(&mut self, log_id: u32, user_id: &str) {
        if let Some(m) = self.whatsapp.iter_mut().find(|m| m.id == log_id)
            && !user_id.is_empty()
        {
            m.user_id = user_id.to_string();
        }
    }

    /// Registros de envíos interrumpidos (el programa se cerró mientras enviaba).
    pub fn wa_recover_interrupted(&mut self) -> usize {
        let mut n = 0;
        for m in self.whatsapp.iter_mut().filter(|m| m.state == WaState::Enviando) {
            m.state = WaState::Fallido;
            m.error = "Envío interrumpido (el programa se cerró); compruebe si llegó".into();
            n += 1;
        }
        n
    }

    fn wa_find(&self, ev_context: &str, user_id: &str, from: &str, today: NaiveDate) -> Option<usize> {
        if !ev_context.is_empty()
            && let Some(i) = self.whatsapp.iter().position(|m| same_msg_id(&m.wamid, ev_context))
        {
            return Some(i);
        }
        let open = |m: &WaMessage| (m.state.waiting() || matches!(m.state, WaState::Confirmada | WaState::NoViene | WaState::Respuesta)) && self.appointments.iter().find(|a| a.id == m.appt_id).is_some_and(|a| a.date >= today);
        if !user_id.is_empty()
            && let Some((i, _)) = self.whatsapp.iter().enumerate().filter(|(_, m)| m.user_id == user_id && open(m)).max_by_key(|(_, m)| m.id)
        {
            return Some(i);
        }
        let num = intl(from)?;
        self.whatsapp.iter().enumerate().filter(|(_, m)| m.to == num && open(m)).max_by_key(|(_, m)| m.id).map(|(i, _)| i)
    }

    /// El paciente pide no recibir más WhatsApp: se le quita el canal (a todos
    /// los pacientes con ese número), queda en el registro de cambios y
    /// recepción lo sabe. Devuelve la confirmación a enviar, si se contesta.
    fn wa_opt_out(&mut self, s: &WaSettings, found: Option<usize>, from: &str, text: &str, today: NaiveDate) -> Vec<(u32, String, String)> {
        let mut pids = self.patients_by_phone(from);
        if let Some(i) = found
            && !pids.contains(&self.whatsapp[i].patient_id)
        {
            pids.push(self.whatsapp[i].patient_id);
        }
        let mut names = vec![];
        for pid in &pids {
            if let Some(p) = self.patient_mut(*pid)
                && p.accepts_whatsapp
            {
                p.accepts_whatsapp = false;
                names.push(p.full_name());
            }
        }
        if names.is_empty() {
            return vec![];
        }
        let who = names.join(", ");
        self.audit("Baja de WhatsApp", &format!("{who} · respondió «{}»", text.trim()));
        self.add_task(today, "WhatsApp", "", pids.first().copied(), &format!("{who} ha pedido no recibir más WhatsApp (respondió «{}»): ya no se le envían. Si quiere otro canal, apúntelo en su ficha.", text.trim()), Some(today));
        let to = found.map(|i| self.whatsapp[i].to.clone()).or_else(|| intl(from)).unwrap_or_default();
        let log = found.map(|i| self.whatsapp[i].id).unwrap_or(0);
        if s.auto_reply && !to.is_empty() { vec![(log, to, "Hecho: no le enviaremos más mensajes por WhatsApp. Si cambia de opinión, díganoslo en la clínica.".to_string())] } else { vec![] }
    }

    /// Aplica un aviso de Meta. Devuelve las respuestas automáticas a enviar
    /// (id del registro, número, texto).
    pub fn wa_apply(&mut self, s: &WaSettings, ev: &WaEvent, now: NaiveDateTime) -> Vec<(u32, String, String)> {
        let today = now.date();
        match ev {
            WaEvent::Status { wamid, status, user_id, error } => {
                let Some(m) = self.whatsapp.iter_mut().find(|m| same_msg_id(&m.wamid, wamid)) else { return vec![] };
                if !user_id.is_empty() {
                    m.user_id = user_id.clone();
                }
                let before = m.state;
                match status.as_str() {
                    "sent" if before == WaState::Enviando => m.state = WaState::Enviado,
                    "delivered" if matches!(before, WaState::Enviando | WaState::Enviado) => m.state = WaState::Entregado,
                    "read" if matches!(before, WaState::Enviando | WaState::Enviado | WaState::Entregado) => m.state = WaState::Leido,
                    "failed" if before == WaState::Enviando || before.waiting() => {
                        m.state = WaState::Fallido;
                        m.error = if error.is_empty() { "Meta no pudo entregarlo".into() } else { error.clone() };
                        let (pid, e) = (m.patient_id, m.error.clone());
                        let who = self.patient_name(pid);
                        self.add_task(today, "WhatsApp", "", Some(pid), &format!("El WhatsApp de recordatorio no llegó a {who} ({e}): llamar para confirmar la cita"), Some(today));
                    }
                    _ => {}
                }
                vec![]
            }
            WaEvent::Message { id, from, user_id, name, context, text, .. } => {
                if !id.is_empty() {
                    if self.wa_seen.iter().any(|x| x == id) {
                        return vec![];
                    }
                    self.wa_seen.push(id.clone());
                    if self.wa_seen.len() > KEEP_SEEN {
                        let extra = self.wa_seen.len() - KEEP_SEEN;
                        self.wa_seen.drain(0..extra);
                    }
                }
                let found = self.wa_find(context, user_id, from, today);
                if is_opt_out(text) {
                    return self.wa_opt_out(s, found, from, text, today);
                }
                let Some(i) = found else {
                    // Con la pasarela, recepción ya ve estos chats en su propio WhatsApp.
                    if s.provider == Provider::Pasarela {
                        return vec![];
                    }
                    // No responde a un recordatorio: que recepción lo lea.
                    let pid = self.patients_by_phone(from).first().copied();
                    let who = pid.map(|p| self.patient_name(p)).unwrap_or_else(|| if name.is_empty() { crate::phone::pretty(from) } else { format!("{name} ({})", crate::phone::pretty(from)) });
                    self.add_task(today, "WhatsApp", "", pid, &format!("Mensaje de WhatsApp de {who}: «{text}»"), Some(today));
                    self.send_msg("WhatsApp", "", &format!("WhatsApp de {who}: «{text}»"), pid);
                    return vec![];
                };
                let decision = intent(text);
                let prev = self.whatsapp[i].state;
                if decision.is_none() && matches!(prev, WaState::Confirmada | WaState::NoViene) {
                    // Ya había contestado: un mensaje sin «sí» ni «no» no cambia su respuesta.
                    if s.provider == Provider::Meta {
                        let pid = self.whatsapp[i].patient_id;
                        let who = self.patient_name(pid);
                        self.add_task(today, "WhatsApp", "", Some(pid), &format!("Mensaje de WhatsApp de {who}: «{text}»"), Some(today));
                        self.send_msg("WhatsApp", "", &format!("WhatsApp de {who}: «{text}»"), Some(pid));
                    }
                    return vec![];
                }
                let m = &mut self.whatsapp[i];
                m.reply = text.clone();
                m.reply_at = Some(now);
                if !user_id.is_empty() {
                    m.user_id = user_id.clone();
                }
                let (log, to, appt_id, pid) = (m.id, m.to.clone(), m.appt_id, m.patient_id);
                m.state = match decision {
                    Some(true) => WaState::Confirmada,
                    Some(false) => WaState::NoViene,
                    None => WaState::Respuesta,
                };
                let new = m.state;
                let Some(a) = self.appointments.iter().find(|a| a.id == appt_id).cloned() else { return vec![] };
                let who = self.patient_name(pid);
                let first = self.patient(pid).map(|p| p.first_name.clone()).unwrap_or_default();
                let when = format!("{} a las {}", date_text(a.date), a.start.format("%H:%M"));
                let mut replies = vec![];
                match decision {
                    Some(true) => {
                        // Si antes dijo que no y la cita se anuló, solo se recupera si
                        // el hueco sigue libre.
                        let mut taken = false;
                        let restore = match a.state {
                            ApptState::Planificada => true,
                            ApptState::Anulada if prev == WaState::NoViene => {
                                let free = self.is_free(a.doctor_id, a.date, a.start, a.end(), Some(a.id));
                                taken = !free;
                                free
                            }
                            _ => false,
                        };
                        if restore && let Some(x) = self.appointments.iter_mut().find(|x| x.id == appt_id) {
                            x.state = ApptState::Confirmada;
                        }
                        if prev == WaState::NoViene {
                            let extra = if taken { ", pero su hueco ya está ocupado: llamar para darle otro" } else { ": su cita vuelve a estar confirmada" };
                            self.send_msg("WhatsApp", "", &format!("{who} ahora SÍ podrá venir el {when} (WhatsApp){extra}"), Some(pid));
                            if taken {
                                self.add_task(today, "WhatsApp", "", Some(pid), &format!("{who} ahora sí puede venir el {when}, pero el hueco ya está ocupado: llamar para darle otra cita"), Some(today));
                            }
                        }
                        if s.auto_reply && prev != new {
                            replies.push((log, to, format!("¡Gracias, {first}! Le esperamos el {when}.")));
                        }
                    }
                    Some(false) => {
                        if s.cancel_on_no
                            && let Some(x) = self.appointments.iter_mut().find(|x| x.id == appt_id)
                            && matches!(x.state, ApptState::Planificada | ApptState::Confirmada)
                        {
                            x.state = ApptState::Anulada;
                            let note = format!("Anulada por el paciente por WhatsApp el {}", now.format("%d/%m %H:%M"));
                            x.notes = if x.notes.trim().is_empty() { note } else { format!("{}\n{note}", x.notes) };
                            self.refresh_recalls(today);
                        }
                        if prev != new {
                            self.add_task(today, "WhatsApp", "", Some(pid), &format!("{who} no puede venir el {when} (avisó por WhatsApp): llamar para darle otra cita"), Some(today));
                            self.send_msg("WhatsApp", "", &format!("{who} no podrá venir a su cita del {when} (WhatsApp)"), Some(pid));
                            if s.auto_reply {
                                replies.push((log, to, format!("Gracias por avisarnos, {first}. Le llamaremos para buscar otra fecha.")));
                            }
                        }
                    }
                    None => {
                        self.add_task(today, "WhatsApp", "", Some(pid), &format!("Revisar respuesta de WhatsApp de {who} sobre su cita del {when}: «{text}»"), Some(today));
                        self.send_msg("WhatsApp", "", &format!("{who} ha respondido al recordatorio: «{text}»"), Some(pid));
                    }
                }
                replies
            }
        }
    }
}

// ───────────────────────────── envío (internet) ─────────────────────────────

/// Envía un mensaje (por Meta o por la pasarela) y devuelve su identificador.
pub fn post_message(s: &WaSettings, body: &Value) -> Result<String, String> {
    if s.provider == Provider::Pasarela {
        let to = jstr(&body["to"]);
        let text = if body["text"].is_string() { jstr(&body["text"]) } else { jstr(&body["text"]["body"]) };
        return gateway_send(s, &to, &text);
    }
    let url = format!("{}/{}/messages", s.api_base.trim().trim_end_matches('/'), s.phone_number_id.trim());
    let agent = ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(10)).timeout(std::time::Duration::from_secs(30)).build();
    let res = agent.post(&url).set("Authorization", &format!("Bearer {}", s.token.trim())).set("Content-Type", "application/json").send_string(&body.to_string());
    let read = |r: ureq::Response| {
        let mut t = String::new();
        r.into_reader().take(MAX_BODY).read_to_string(&mut t).map(|_| t).map_err(|e| e.to_string())
    };
    match res {
        Ok(r) => {
            let t = read(r)?;
            // Meta lo ha aceptado (200), aunque la respuesta no se entienda
            let v: Value = serde_json::from_str(&t).map_err(|_| format!("{UNCERTAIN}: Meta lo aceptó pero su respuesta no se entiende"))?;
            v["messages"][0]["id"].as_str().map(|x| x.to_string()).ok_or_else(|| format!("{UNCERTAIN}: Meta lo aceptó sin devolver su identificador"))
        }
        Err(ureq::Error::Status(code, r)) => {
            let t = read(r).unwrap_or_default();
            let v: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
            let msg = jstr(&v["error"]["message"]);
            let details = jstr(&v["error"]["error_data"]["details"]);
            let ecode = v["error"]["code"].as_i64().map(|c| format!(" [{c}]")).unwrap_or_default();
            let text = match (msg.is_empty(), details.is_empty()) {
                (true, _) => format!("error {code}"),
                (false, true) => msg,
                (false, false) => format!("{msg}: {details}"),
            };
            Err(format!("Meta rechazó el mensaje{ecode}: {text}"))
        }
        Err(ureq::Error::Transport(t)) => Err(transport_error("WhatsApp", &t)),
    }
}

/// Un error de red al enviar: si la petición no llegó a salir (no hay
/// conexión, nombre o dirección), no se envió; si salió y se cortó sin
/// respuesta, no se sabe ([`UNCERTAIN`]).
fn transport_error(to: &str, t: &ureq::Transport) -> String {
    use ureq::ErrorKind::*;
    match t.kind() {
        Dns | ConnectionFailed | InvalidUrl | UnknownScheme | InsecureRequestHttpsOnly | InvalidProxyUrl | ProxyConnect | ProxyUnauthorized => format!("Sin conexión con {to}: {t}"),
        _ => format!("{UNCERTAIN}: se cortó la conexión con {to} al enviarlo ({t})"),
    }
}

// ───────────────────────────── pasarela gratuita (WAHA / Evolution API) ─────────────────────────────

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(5)).timeout(std::time::Duration::from_secs(30)).build()
}

fn read_body(r: ureq::Response) -> String {
    let mut t = String::new();
    let _ = r.into_reader().take(MAX_BODY).read_to_string(&mut t);
    t
}

/// Parte de una dirección web (sesión o instancia) codificada.
fn seg(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn gw_error(code: u16, t: &str) -> String {
    if matches!(code, 401 | 403) {
        return format!("la pasarela no acepta la clave ({code})");
    }
    let v: Value = serde_json::from_str(t).unwrap_or(Value::Null);
    let mut msg = jstr(&v["message"]);
    if msg.is_empty() {
        msg = match &v["response"]["message"] {
            Value::Array(a) => a.iter().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join("; "),
            other => jstr(other),
        };
    }
    if msg.is_empty() {
        msg = jstr(&v["error"]);
    }
    if msg.is_empty() {
        msg = t.chars().take(160).collect();
    }
    format!("la pasarela respondió {code}: {msg}")
}

/// Identificador del mensaje en la respuesta de la pasarela.
pub fn gw_message_id(v: &Value) -> Option<String> {
    [&v["id"]["_serialized"], &v["id"], &v["key"]["id"], &v["id"]["id"], &v["message"]["key"]["id"]].iter().find_map(|x| x.as_str().filter(|s| !s.is_empty()).map(String::from))
}

/// Envía un mensaje normal por la pasarela.
pub fn gateway_send(s: &WaSettings, to: &str, text: &str) -> Result<String, String> {
    let base = s.gw_url.trim().trim_end_matches('/');
    let sess = s.gw_session.trim();
    let (url, body, header) = match s.gw_kind {
        GwKind::Waha => (format!("{base}/api/sendText"), json!({ "session": sess, "chatId": format!("{to}@c.us"), "text": text }), "X-Api-Key"),
        GwKind::Evolution => (format!("{base}/message/sendText/{}", seg(sess)), json!({ "number": to, "text": text }), "apikey"),
    };
    let mut req = agent().post(&url).set("Content-Type", "application/json");
    if !s.gw_key.trim().is_empty() {
        req = req.set(header, s.gw_key.trim());
    }
    match req.send_string(&body.to_string()) {
        Ok(r) => {
            let v: Value = serde_json::from_str(&read_body(r)).unwrap_or(Value::Null);
            gw_message_id(&v).ok_or_else(|| format!("{UNCERTAIN}: la pasarela lo aceptó sin devolver su identificador"))
        }
        Err(ureq::Error::Status(code, r)) => Err(format!("No se envió: {}", gw_error(code, &read_body(r)))),
        Err(ureq::Error::Transport(t)) if !transport_error("la pasarela", &t).starts_with(UNCERTAIN) => Err("No hay conexión con la pasarela de WhatsApp: compruebe que está encendida".into()),
        Err(ureq::Error::Transport(t)) => Err(transport_error("la pasarela", &t)),
    }
}

fn gw_get(s: &WaSettings, path: &str) -> Result<Value, (u16, String)> {
    let base = s.gw_url.trim().trim_end_matches('/');
    let header = if s.gw_kind == GwKind::Waha { "X-Api-Key" } else { "apikey" };
    let mut req = agent().get(&format!("{base}{path}"));
    if !s.gw_key.trim().is_empty() {
        req = req.set(header, s.gw_key.trim());
    }
    match req.call() {
        Ok(r) => Ok(serde_json::from_str(&read_body(r)).unwrap_or(Value::Null)),
        Err(ureq::Error::Status(code, r)) => Err((code, read_body(r))),
        Err(_) => Err((0, String::new())),
    }
}

/// WAHA: identificador oculto (@lid) del número, para reconocer sus respuestas.
pub fn gateway_lid(s: &WaSettings, to: &str) -> Option<String> {
    if s.gw_kind != GwKind::Waha || to.is_empty() {
        return None;
    }
    let v = gw_get(s, &format!("/api/{}/lids/pn/{}", seg(s.gw_session.trim()), seg(to))).ok()?;
    v["lid"].as_str().filter(|x| !x.is_empty()).map(String::from)
}

/// ¿Está el WhatsApp de la clínica conectado a la pasarela?
pub fn gateway_status(s: &WaSettings) -> Result<String, String> {
    let sess = s.gw_session.trim();
    let res = match s.gw_kind {
        GwKind::Waha => gw_get(s, &format!("/api/sessions/{}", seg(sess))),
        GwKind::Evolution => gw_get(s, &format!("/instance/connectionState/{}", seg(sess))),
    };
    match res {
        Ok(v) => {
            let st = match s.gw_kind {
                GwKind::Waha => jstr(&v["status"]),
                GwKind::Evolution => jstr(&v["instance"]["state"]),
            };
            match st.as_str() {
                "WORKING" | "open" => Ok("WhatsApp conectado a la pasarela".into()),
                "SCAN_QR_CODE" | "connecting" => Err("falta vincular el WhatsApp: escanee el código QR en la pasarela".into()),
                "STARTING" => Err("la sesión se está iniciando; pruebe en unos segundos".into()),
                "" => Err("la pasarela no indica el estado de la sesión".into()),
                other => Err(format!("WhatsApp no está conectado (estado {other})")),
            }
        }
        Err((404, _)) => Err(format!("no existe «{sess}» en la pasarela")),
        Err((0, _)) => Err("No hay conexión con la pasarela: compruebe que está encendida y la dirección".into()),
        Err((code, t)) => Err(gw_error(code, &t)),
    }
}

/// Comprueba el acceso a la plataforma de Meta (número y token).
pub fn meta_status(s: &WaSettings) -> Result<String, String> {
    let url = format!("{}/{}?fields=display_phone_number,verified_name", s.api_base.trim().trim_end_matches('/'), s.phone_number_id.trim());
    match agent().get(&url).set("Authorization", &format!("Bearer {}", s.token.trim())).call() {
        Ok(r) => {
            let v: Value = serde_json::from_str(&read_body(r)).unwrap_or(Value::Null);
            Ok(format!("Conectado a Meta: {} ({})", jstr(&v["verified_name"]), jstr(&v["display_phone_number"])))
        }
        Err(ureq::Error::Status(code, r)) => {
            let v: Value = serde_json::from_str(&read_body(r)).unwrap_or(Value::Null);
            Err(format!("Meta respondió {code}: {}", jstr(&v["error"]["message"])))
        }
        Err(e) => Err(format!("Sin conexión con Meta: {e}")),
    }
}

/// Pausa aleatoria entre `min` y `max` segundos.
fn pause_secs(min: u32, max: u32) -> u32 {
    if max <= min {
        return min;
    }
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    min + n % (max - min + 1)
}

fn jid_parts(jid: &str) -> (String, String) {
    let (user, server) = jid.split_once('@').unwrap_or((jid, ""));
    let user = user.split(':').next().unwrap_or("");
    match server {
        "c.us" | "s.whatsapp.net" | "" => (user.chars().filter(|c| c.is_ascii_digit()).collect(), String::new()),
        _ => (String::new(), jid.to_string()),
    }
}

/// Lee un aviso de la pasarela (WAHA o Evolution API).
pub fn parse_gateway(body: &str) -> Vec<WaEvent> {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return vec![] };
    let event = jstr(&v["event"]).to_lowercase().replace('_', ".");
    let mut out = vec![];
    match event.as_str() {
        // WAHA
        "message" => {
            let p = &v["payload"];
            if p["fromMe"].as_bool() == Some(true) {
                return out;
            }
            let (from, mut user) = jid_parts(&jstr(&p["from"]));
            if from.is_empty() && user.is_empty() {
                user = jstr(&p["from"]);
            }
            let context = if p["replyTo"].is_string() { jstr(&p["replyTo"]) } else { jstr(&p["replyTo"]["id"]) };
            let name = [&p["_data"]["notifyName"], &p["_data"]["pushName"], &p["_data"]["Info"]["PushName"]].iter().find_map(|x| x.as_str().map(String::from)).unwrap_or_default();
            out.push(WaEvent::Message { id: jstr(&p["id"]), from, user_id: user, name, context, text: jstr(&p["body"]), button: false });
        }
        "message.ack" => {
            let p = &v["payload"];
            let name = jstr(&p["ackName"]);
            let status = match (name.as_str(), p["ack"].as_i64()) {
                ("ERROR", _) | (_, Some(-1)) => "failed",
                ("SERVER", _) | (_, Some(1)) => "sent",
                ("DEVICE", _) | (_, Some(2)) => "delivered",
                ("READ" | "PLAYED", _) | (_, Some(3 | 4)) => "read",
                _ => "",
            };
            if !status.is_empty() {
                out.push(WaEvent::Status { wamid: jstr(&p["id"]), status: status.into(), user_id: String::new(), error: if status == "failed" { "la pasarela no pudo entregarlo".into() } else { String::new() } });
            }
        }
        // Evolution API
        "messages.upsert" => {
            let items: Vec<&Value> = match &v["data"] {
                Value::Array(a) => a.iter().collect(),
                other => vec![other],
            };
            for d in items {
                let k = &d["key"];
                if k["fromMe"].as_bool() == Some(true) {
                    continue;
                }
                let jid = [&k["remoteJid"], &k["remoteJidAlt"], &k["senderPn"]].iter().map(|x| jstr(x)).find(|x| x.ends_with("@s.whatsapp.net") || x.ends_with("@c.us")).unwrap_or_else(|| jstr(&k["remoteJid"]));
                let (from, user) = jid_parts(&jid);
                let m = &d["message"];
                let text = [
                    &m["conversation"],
                    &m["extendedTextMessage"]["text"],
                    &m["buttonsResponseMessage"]["selectedDisplayText"],
                    &m["templateButtonReplyMessage"]["selectedDisplayText"],
                    &m["listResponseMessage"]["title"],
                    &m["reactionMessage"]["text"],
                ]
                .iter()
                .find_map(|x| x.as_str().filter(|s| !s.is_empty()).map(String::from))
                .unwrap_or_else(|| format!("[{}]", jstr(&d["messageType"])));
                let context = [&m["extendedTextMessage"]["contextInfo"]["stanzaId"], &d["contextInfo"]["stanzaId"]].iter().find_map(|x| x.as_str().map(String::from)).unwrap_or_default();
                out.push(WaEvent::Message { id: jstr(&k["id"]), from, user_id: user, name: jstr(&d["pushName"]), context, text, button: false });
            }
        }
        "messages.update" => {
            let items: Vec<&Value> = match &v["data"] {
                Value::Array(a) => a.iter().collect(),
                other => vec![other],
            };
            for d in items {
                let status = match jstr(&d["status"]).as_str() {
                    "SERVER_ACK" => "sent",
                    "DELIVERY_ACK" => "delivered",
                    "READ" | "PLAYED" => "read",
                    "ERROR" => "failed",
                    _ => "",
                };
                let wamid = if jstr(&d["keyId"]).is_empty() { jstr(&d["key"]["id"]) } else { jstr(&d["keyId"]) };
                if !status.is_empty() && !wamid.is_empty() {
                    out.push(WaEvent::Status { wamid, status: status.into(), user_id: String::new(), error: if status == "failed" { "la pasarela no pudo entregarlo".into() } else { String::new() } });
                }
            }
        }
        _ => {}
    }
    out
}

#[derive(Clone, Debug)]
pub enum JobKind {
    /// Recordatorio de cita (id del registro).
    Reminder(u32),
    /// Respuesta automática (id del registro).
    Reply(u32),
    /// Mensaje de prueba.
    Test,
    /// Comprobar la conexión.
    Check,
}

pub struct JobResult {
    pub kind: JobKind,
    pub result: Result<String, String>,
    /// Identificador oculto del paciente (@lid), si la pasarela lo da.
    pub extra: String,
}

/// Hilo que envía los mensajes de uno en uno, sin bloquear la ventana. Por la
/// pasarela espera una pausa aleatoria entre recordatorios.
pub fn start_worker(wake: impl Fn() + Send + 'static) -> (Sender<(WaSettings, JobKind, Value)>, Receiver<JobResult>) {
    let (jtx, jrx) = std::sync::mpsc::channel::<(WaSettings, JobKind, Value)>();
    let (rtx, rrx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for (s, kind, body) in jrx {
            if s.provider == Provider::Pasarela {
                let secs = match kind {
                    JobKind::Reminder(_) => pause_secs(s.gw_min_pause, s.gw_max_pause),
                    JobKind::Reply(_) => pause_secs(2.min(s.gw_max_pause), 5.min(s.gw_max_pause)),
                    _ => 0,
                };
                if secs > 0 {
                    std::thread::sleep(std::time::Duration::from_secs(secs as u64));
                }
            }
            let (result, extra) = match kind {
                JobKind::Check => (if s.provider == Provider::Pasarela { gateway_status(&s) } else { meta_status(&s) }, String::new()),
                JobKind::Reminder(_) if s.provider == Provider::Pasarela => {
                    let r = post_message(&s, &body);
                    let lid = if r.is_ok() { gateway_lid(&s, &jstr(&body["to"])).unwrap_or_default() } else { String::new() };
                    (r, lid)
                }
                _ => (post_message(&s, &body), String::new()),
            };
            if rtx.send(JobResult { kind, result, extra }).is_err() {
                break;
            }
            wake();
        }
    });
    (jtx, rrx)
}

// ───────────────────────────── recepción de avisos (webhook) ─────────────────────────────

/// Configuración que el receptor consulta en cada aviso (se puede cambiar en marcha).
#[derive(Clone, Debug, Default)]
pub struct HookCfg {
    pub verify_token: String,
    pub app_secret: String,
    /// Si es falso se responde 503 y Meta lo reintentará más tarde.
    pub accepting: bool,
    pub received: u64,
    pub rejected: u64,
}

pub struct Hook {
    pub port: u16,
    pub rx: Receiver<WaEvent>,
    pub cfg: Arc<Mutex<HookCfg>>,
}

fn const_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Escucha los avisos de Meta en `/whatsapp` (el túnel https de este ordenador
/// los reenvía aquí) y los de la pasarela en `/pasarela?clave=…`. Con `lan`
/// escucha en toda la red local (pasarela en Docker o en otro ordenador).
pub fn listen(port: u16, lan: bool, cfg: HookCfg, wake: impl Fn() + Send + 'static) -> Result<Hook, String> {
    let host = if lan { "0.0.0.0" } else { "127.0.0.1" };
    let server = tiny_http::Server::http((host, port)).map_err(|e| format!("No se pudo abrir el puerto {port}: {e}"))?;
    let real = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(port);
    let (tx, rx) = std::sync::mpsc::channel();
    let shared = Arc::new(Mutex::new(cfg));
    let c = shared.clone();
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let url = req.url().to_string();
            let path = url.split('?').next().unwrap_or("").trim_end_matches('/').to_string();
            let reply = |req: tiny_http::Request, code: u16, text: &str| {
                let h = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(text).with_status_code(code).with_header(h));
            };
            if path == "/pasarela" || path.starts_with("/pasarela/") {
                let cfg = c.lock().map(|g| g.clone()).unwrap_or_default();
                if *req.method() != tiny_http::Method::Post {
                    reply(req, 405, "método no permitido");
                    continue;
                }
                let key = query_param(&url, "clave").or_else(|| req.headers().iter().find(|h| h.field.equiv("X-Clave")).map(|h| h.value.as_str().to_string())).unwrap_or_default();
                if cfg.verify_token.is_empty() || !const_eq(&key, &cfg.verify_token) {
                    if let Ok(mut g) = c.lock() {
                        g.rejected += 1;
                    }
                    reply(req, 403, "clave incorrecta");
                    continue;
                }
                if !cfg.accepting {
                    reply(req, 503, "no disponible");
                    continue;
                }
                let mut body = Vec::new();
                if req.as_reader().take(MAX_BODY + 1).read_to_end(&mut body).is_err() || body.len() as u64 > MAX_BODY {
                    reply(req, 413, "demasiado grande");
                    continue;
                }
                let mut ok = true;
                for e in parse_gateway(&String::from_utf8_lossy(&body)) {
                    ok &= tx.send(e).is_ok();
                }
                if let Ok(mut g) = c.lock() {
                    g.received += 1;
                }
                wake();
                reply(req, if ok { 200 } else { 503 }, if ok { "ok" } else { "cerrando" });
                continue;
            }
            if path != "/whatsapp" {
                if path.is_empty() {
                    reply(req, 200, "VeodarDental · WhatsApp");
                } else {
                    reply(req, 404, "no existe");
                }
                continue;
            }
            let cfg = c.lock().map(|g| g.clone()).unwrap_or_default();
            let method = req.method().clone();
            match method {
                tiny_http::Method::Get => match verify_challenge(&url, &cfg.verify_token) {
                    Some(ch) => reply(req, 200, &ch),
                    None => reply(req, 403, "token de verificación incorrecto"),
                },
                tiny_http::Method::Post => {
                    if !cfg.accepting {
                        reply(req, 503, "no disponible");
                        continue;
                    }
                    let sig = req.headers().iter().find(|h| h.field.equiv("X-Hub-Signature-256")).map(|h| h.value.as_str().to_string()).unwrap_or_default();
                    let mut body = Vec::new();
                    if req.as_reader().take(MAX_BODY + 1).read_to_end(&mut body).is_err() || body.len() as u64 > MAX_BODY {
                        reply(req, 413, "demasiado grande");
                        continue;
                    }
                    if !verify_signature(&cfg.app_secret, &body, &sig) {
                        if let Ok(mut g) = c.lock() {
                            g.rejected += 1;
                        }
                        reply(req, 403, "firma no válida");
                        continue;
                    }
                    let events = parse_webhook(&String::from_utf8_lossy(&body));
                    let mut ok = true;
                    for e in events {
                        ok &= tx.send(e).is_ok();
                    }
                    if let Ok(mut g) = c.lock() {
                        g.received += 1;
                    }
                    wake();
                    if ok {
                        reply(req, 200, "ok");
                    } else {
                        reply(req, 503, "cerrando");
                    }
                }
                _ => reply(req, 405, "método no permitido"),
            }
        }
    });
    Ok(Hook { port: real, rx, cfg: shared })
}

/// Registros de ejemplo para el modo demostración.
pub fn sample_whatsapp(db: &mut Db, today: NaiveDate) {
    let tomorrow: Vec<(u32, u32)> = db
        .appointments
        .iter()
        .filter(|a| a.date > today && a.patient_id.is_some())
        .filter(|a| a.patient_id.and_then(|p| db.patient(p)).is_some_and(|p| p.accepts_whatsapp && p.accepts_gdpr && intl(&p.mobile).is_some()))
        .map(|a| (a.id, a.patient_id.unwrap()))
        .take(2)
        .collect();
    for (i, (appt, pid)) in tomorrow.into_iter().enumerate() {
        let to = db.patient(pid).and_then(|p| intl(&p.mobile)).unwrap_or_default();
        let id = db.new_id();
        let sent = today.and_hms_opt(10, 2 + i as u32, 0).unwrap();
        let confirmed = db.appointments.iter().find(|a| a.id == appt).is_some_and(|a| a.state == ApptState::Confirmada);
        db.whatsapp.push(WaMessage {
            id,
            appt_id: appt,
            patient_id: pid,
            to,
            wamid: format!("wamid.demo{id}"),
            user_id: String::new(),
            created: sent,
            sent_at: Some(sent),
            state: if confirmed { WaState::Confirmada } else { WaState::Leido },
            reply: if confirmed { BUTTON_YES.into() } else { String::new() },
            reply_at: confirmed.then(|| sent + Duration::minutes(14)),
            error: String::new(),
            uncertain: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn friday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 6).unwrap()
    }

    fn at(d: NaiveDate, h: u32, m: u32) -> NaiveDateTime {
        d.and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap())
    }

    fn cfg() -> WaSettings {
        WaSettings { enabled: true, phone_number_id: "1065".into(), token: "TOKEN".into(), app_secret: "secreto".into(), verify_token: "verifica".into(), ..Default::default() }
    }

    #[test]
    fn hmac_matches_rfc4231() {
        assert_eq!(hex(&hmac_sha256(&[0x0b; 20], b"Hi There")), "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7");
        assert_eq!(hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        assert_eq!(hex(&hmac_sha256(&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First")), "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54");
        let body = br#"{"object":"whatsapp_business_account"}"#;
        let sig = format!("sha256={}", hex(&hmac_sha256(b"secreto", body)));
        assert!(verify_signature("secreto", body, &sig));
        assert!(verify_signature("secreto", body, &sig.to_uppercase().replace("SHA256=", "sha256=")));
        assert!(!verify_signature("otro", body, &sig));
        assert!(!verify_signature("secreto", b"{}", &sig));
        assert!(!verify_signature("", body, &sig));
        assert!(!verify_signature("secreto", body, "md5=123"));
    }

    #[test]
    fn spanish_replies_are_understood() {
        for t in ["Sí, confirmo", "si", "Vale, allí estaré", "ok 👍", "Confirmo la cita", "Sí"] {
            assert_eq!(intent(t), Some(true), "{t}");
        }
        for t in ["No puedo ir", "no", "Quiero anular la cita", "Tengo que cancelar", "imposible ese día", "¿Podemos cambiar la hora?"] {
            assert_eq!(intent(t), Some(false), "{t}");
        }
        for t in ["¿A qué hora era?", "Hola", "No sé si podré, sí que intentaré", "[image]"] {
            assert_eq!(intent(t), None, "{t}");
        }
    }

    #[test]
    fn numbers_and_dates() {
        assert_eq!(intl("610 200 100").as_deref(), Some("34610200100"));
        assert_eq!(intl("+34 610200100").as_deref(), Some("34610200100"));
        assert_eq!(intl("+33 6 12 34 56 78").as_deref(), Some("33612345678"));
        assert_eq!(intl("1234"), None);
        assert_eq!(date_text(NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()), "lunes 9 de noviembre");
        let v = template_request(&cfg(), "34610200100", "Ana", "Clínica", "lunes 9 de noviembre", "10:30");
        assert_eq!(v["template"]["name"], "recordatorio_cita");
        assert_eq!(v["template"]["language"]["code"], "es");
        assert_eq!(v["template"]["components"][0]["parameters"][0]["parameter_name"], "nombre");
        assert_eq!(v["template"]["components"][0]["parameters"][3]["text"], "10:30");
        assert_eq!(text_request("34", "hola")["text"]["body"], "hola");
    }

    #[test]
    fn webhook_payloads_are_read() {
        // ejemplo de la documentación de Meta (respuesta con botón)
        let button = r#"{"object":"whatsapp_business_account","entry":[{"id":"102290129340398","changes":[{"value":{"messaging_product":"whatsapp","metadata":{"display_phone_number":"15550783881","phone_number_id":"106540352242922"},"contacts":[{"profile":{"name":"Sheena Nelson"},"wa_id":"16505551234","user_id":"US.13491208655302741918"}],"messages":[{"context":{"from":"15550783881","id":"wamid.ORIGINAL"},"from":"16505551234","from_user_id":"US.13491208655302741918","id":"wamid.REPLY","timestamp":"1739321024","type":"button","button":{"payload":"Sí, confirmo","text":"Sí, confirmo"}}]},"field":"messages"}]}]}"#;
        let ev = parse_webhook(button);
        assert_eq!(
            ev,
            vec![WaEvent::Message { id: "wamid.REPLY".into(), from: "16505551234".into(), user_id: "US.13491208655302741918".into(), name: "Sheena Nelson".into(), context: "wamid.ORIGINAL".into(), text: "Sí, confirmo".into(), button: true }]
        );
        let status = r#"{"entry":[{"changes":[{"value":{"statuses":[{"id":"wamid.X","status":"failed","timestamp":"1","recipient_id":"16505551234","recipient_user_id":"US.1","errors":[{"code":131026,"title":"Message undeliverable","error_data":{"details":"Receiver is incapable of receiving this message"}}]},{"id":"wamid.Y","status":"read","recipient_user_id":"US.2"}]}}]}]}"#;
        let ev = parse_webhook(status);
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0], WaEvent::Status { wamid: "wamid.X".into(), status: "failed".into(), user_id: "US.1".into(), error: "Message undeliverable: Receiver is incapable of receiving this message".into() });
        let text = r#"{"entry":[{"changes":[{"value":{"messages":[{"from":"34610200100","id":"wamid.T","type":"text","text":{"body":"no puedo"}}]}}]}]}"#;
        assert!(matches!(&parse_webhook(text)[0], WaEvent::Message { text, button: false, .. } if text == "no puedo"));
        assert!(parse_webhook("esto no es json").is_empty());
        assert_eq!(verify_challenge("/whatsapp?hub.mode=subscribe&hub.challenge=1158201444&hub.verify_token=verifica", "verifica").as_deref(), Some("1158201444"));
        assert_eq!(verify_challenge("/whatsapp?hub.mode=subscribe&hub.challenge=1&hub.verify_token=mal", "verifica"), None);
        assert_eq!(verify_challenge("/whatsapp?hub.mode=subscribe&hub.challenge=1&hub.verify_token=", ""), None);
    }

    #[test]
    fn reminders_go_out_the_working_day_before() {
        let mut db = Db::sample(friday());
        let s = cfg();
        db.whatsapp.clear();
        // las citas «de mañana» de la muestra caen el lunes: se envían el viernes
        let due = db.wa_due(&s, at(friday(), 10, 30));
        assert!(!due.is_empty());
        for id in &due {
            let a = db.appointments.iter().find(|a| a.id == *id).unwrap();
            assert_eq!(a.state, ApptState::Planificada);
            let p = db.patient(a.patient_id.unwrap()).unwrap();
            assert!(p.accepts_whatsapp && p.accepts_gdpr);
            assert!(db.wa_send_day(a.date, 1) <= friday());
        }
        assert!(db.wa_due(&s, at(friday(), 9, 0)).is_empty(), "fuera del horario de envío");
        assert!(db.wa_due(&s, at(friday(), 20, 0)).is_empty());
        let monday = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        assert_eq!(db.wa_send_day(monday, 1), friday());
        assert_eq!(db.wa_send_day(monday, 2), friday() - Duration::days(1));
        // festivo el viernes → se adelanta al jueves
        db.holidays.push(crate::agenda_extra::Holiday { date: friday(), name: "Fiesta".into() });
        assert_eq!(db.wa_send_day(monday, 1), friday() - Duration::days(1));
        // una vez preparado no vuelve a salir
        let first = due[0];
        let (log, body) = db.wa_prepare(&s, first, now()).unwrap();
        assert_eq!(body["type"], "template");
        assert!(!db.wa_due(&s, at(friday(), 10, 31)).contains(&first));
        assert_eq!(db.whatsapp.iter().find(|m| m.id == log).unwrap().state, WaState::Enviando);
        db.wa_sent(log, Ok("wamid.A".into()), friday());
        assert!(db.appointments.iter().find(|a| a.id == first).unwrap().reminder_sent);
        assert_eq!(db.wa_recover_interrupted(), 0);
    }

    fn sent_reminder(db: &mut Db) -> (u32, u32) {
        let s = cfg();
        db.whatsapp.clear();
        db.chat.clear();
        let appt = db.wa_due(&s, at(friday(), 10, 30))[0];
        let (log, _) = db.wa_prepare(&s, appt, now()).unwrap();
        db.wa_sent(log, Ok("wamid.SENT".into()), friday());
        (appt, log)
    }

    fn reply(id: &str, text: &str, context: &str) -> WaEvent {
        WaEvent::Message { id: id.into(), from: String::new(), user_id: "US.9".into(), name: "Paciente".into(), context: context.into(), text: text.into(), button: true }
    }

    /// Un envío que salió y se cortó sin respuesta: no se reenvía, la
    /// respuesta del paciente se recoge igual y recepción lo sabe.
    #[test]
    fn an_uncertain_send_is_not_retried_and_replies_still_count() {
        let mut db = Db::sample(friday());
        let s = cfg();
        db.whatsapp.clear();
        let appt = db.wa_due(&s, at(friday(), 10, 30))[0];
        let (log, _) = db.wa_prepare(&s, appt, now()).unwrap();
        let tasks = db.tasks.len();
        db.wa_sent(log, Err(format!("{UNCERTAIN}: se cortó la conexión")), friday());
        let m = db.whatsapp.iter().find(|m| m.id == log).unwrap().clone();
        assert!(m.uncertain && m.state == WaState::Enviado);
        assert!(!db.wa_due(&s, at(friday(), 10, 31)).contains(&appt), "no se reenvía");
        assert_eq!(db.tasks.len(), tasks + 1);
        assert!(db.tasks.last().unwrap().text.contains("No se sabe si llegó"));
        // el paciente contesta (sin identificador del mensaje): se recoge por su número
        let p = db.appointments.iter().find(|a| a.id == appt).unwrap().patient_id.unwrap();
        let from = db.patient(p).unwrap().mobile.clone();
        let ev = WaEvent::Message { id: "in9".into(), from, user_id: String::new(), name: String::new(), context: String::new(), text: "Sí".into(), button: false };
        db.wa_apply(&s, &ev, at(friday(), 11, 0));
        assert_eq!(db.appointments.iter().find(|a| a.id == appt).unwrap().state, ApptState::Confirmada);
    }

    /// El pie de baja de cada idioma dice una palabra («BAJA», «BAIXA»…):
    /// esa palabra, contestada tal cual, tiene que dar de baja. Y las formas
    /// habituales de pedirlo en cada idioma también.
    #[test]
    fn the_word_each_footer_asks_for_is_an_opt_out() {
        use crate::i18n::{Lang, tr};
        for lang in Lang::ALL {
            let footer = tr(lang, "Si no quiere recibir estos mensajes, respóndanos con la palabra BAJA o llámenos al [TELCLINICA].").replace("[TELCLINICA]", "944 000 000");
            let words: Vec<&str> = footer.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 4 && w.chars().all(|c| c.is_uppercase())).collect();
            assert!(!words.is_empty(), "{lang:?}: el pie dice qué palabra contestar: {footer}");
            for w in words {
                for reply in [w.to_string(), w.to_lowercase(), format!("{w}."), format!(" {} ", w.to_lowercase())] {
                    assert!(is_opt_out(&reply), "{lang:?}: «{reply}» tiene que dar de baja");
                }
            }
        }
        for t in ["Vull donar-me de baixa", "No vull rebre més missatges", "Quero darme de baixa", "Non quero recibir máis mensaxes", "Ez dut jaso nahi", "Unsubscribe", "I don't want to receive these"] {
            assert!(is_opt_out(t), "{t}");
        }
    }

    /// «BAJA» por WhatsApp: no se le envían más WhatsApp (a ese número),
    /// queda en el registro de cambios y recepción lo sabe.
    #[test]
    fn replying_baja_stops_whatsapp() {
        for t in ["BAJA", "baja.", "Stop", "Quiero darme de baja", "no quiero recibir más mensajes"] {
            assert!(is_opt_out(t), "{t}");
        }
        for t in ["No", "Sí", "No puedo ir", "¿Me da cita?", "No vull canviar la cita", "Non podo ir"] {
            assert!(!is_opt_out(t), "{t}");
        }
        let mut db = Db::sample(friday());
        let s = WaSettings { auto_reply: true, ..cfg() };
        let (appt, _) = sent_reminder(&mut db);
        let pid = db.appointments.iter().find(|a| a.id == appt).unwrap().patient_id.unwrap();
        let r = db.wa_apply(&s, &reply("in5", "BAJA", "wamid.SENT"), at(friday(), 11, 0));
        assert!(!db.patient(pid).unwrap().accepts_whatsapp);
        assert_eq!(db.appointments.iter().find(|a| a.id == appt).unwrap().state, ApptState::Planificada, "no anula la cita");
        assert!(db.audit_log.iter().any(|a| a.action == "Baja de WhatsApp"));
        assert!(db.tasks.iter().any(|t| t.text.contains("no recibir más WhatsApp")));
        assert_eq!(r.len(), 1, "se le confirma");
        assert!(db.wa_due(&s, at(friday(), 10, 30)).iter().all(|a| db.appointments.iter().find(|x| x.id == *a).unwrap().patient_id != Some(pid)));
    }

    /// Si la petición no llegó a salir, no se envió; si salió y se cortó sin
    /// respuesta, no se sabe (no se reintenta).
    #[test]
    fn network_errors_tell_not_sent_from_uncertain() {
        // nadie escuchando: no salió
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let s = WaSettings { api_base: format!("http://127.0.0.1:{closed}/v27.0"), ..cfg() };
        let e = post_message(&s, &text_request("34600111222", "hola")).unwrap_err();
        assert!(!e.starts_with(UNCERTAIN), "{e}");
        // recibe la petición y se corta sin contestar: no se sabe
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = std::io::Read::read(&mut c, &mut buf);
        });
        let s = WaSettings { api_base: format!("http://127.0.0.1:{port}/v27.0"), ..cfg() };
        let e = post_message(&s, &text_request("34600111222", "hola")).unwrap_err();
        assert!(e.starts_with(UNCERTAIN), "{e}");
        h.join().unwrap();
    }

    #[test]
    fn yes_confirms_and_no_cancels() {
        let mut db = Db::sample(friday());
        let s = cfg();
        let (appt, log) = sent_reminder(&mut db);
        let now = at(friday(), 11, 0);
        // estados de entrega
        db.wa_apply(&s, &WaEvent::Status { wamid: "wamid.SENT".into(), status: "read".into(), user_id: "US.9".into(), error: String::new() }, now);
        db.wa_apply(&s, &WaEvent::Status { wamid: "wamid.SENT".into(), status: "delivered".into(), user_id: String::new(), error: String::new() }, now);
        assert_eq!(db.whatsapp.iter().find(|m| m.id == log).unwrap().state, WaState::Leido, "no retrocede");
        // «Sí»
        let r = db.wa_apply(&s, &reply("in1", BUTTON_YES, "wamid.SENT"), now);
        assert_eq!(db.appointments.iter().find(|a| a.id == appt).unwrap().state, ApptState::Confirmada);
        assert_eq!(r.len(), 1);
        assert!(r[0].2.starts_with("¡Gracias"));
        // el mismo aviso repetido (Meta reintenta) no hace nada
        assert!(db.wa_apply(&s, &reply("in1", BUTTON_YES, "wamid.SENT"), now).is_empty());
        // luego dice que no: se anula, tarea y aviso a recepción
        let tasks = db.tasks.len();
        let r = db.wa_apply(&s, &reply("in2", BUTTON_NO, "wamid.SENT"), now);
        let a = db.appointments.iter().find(|a| a.id == appt).unwrap();
        assert_eq!(a.state, ApptState::Anulada);
        assert!(a.notes.contains("WhatsApp"));
        assert_eq!(db.tasks.len(), tasks + 1);
        assert!(db.chat.iter().any(|m| m.text.contains("no podrá venir")));
        assert!(r[0].2.contains("Le llamaremos"));
        assert_eq!(db.whatsapp.iter().find(|m| m.id == log).unwrap().state, WaState::NoViene);
        // y al final sí puede: se recupera la cita
        db.wa_apply(&s, &reply("in3", "Al final sí, confirmo", "wamid.SENT"), now);
        assert_eq!(db.appointments.iter().find(|a| a.id == appt).unwrap().state, ApptState::Confirmada);
    }

    #[test]
    fn unclear_or_unrelated_messages_go_to_reception() {
        let mut db = Db::sample(friday());
        let s = WaSettings { auto_reply: false, cancel_on_no: false, ..cfg() };
        let (appt, log) = sent_reminder(&mut db);
        let now = at(friday(), 12, 0);
        let tasks = db.tasks.len();
        // texto sin botón y sin contexto: se asocia por el identificador del paciente
        db.wa_apply(&s, &WaEvent::Status { wamid: "wamid.SENT".into(), status: "delivered".into(), user_id: "US.77".into(), error: String::new() }, now);
        let ev = WaEvent::Message { id: "t1".into(), from: String::new(), user_id: "US.77".into(), name: String::new(), context: String::new(), text: "¿Tengo que ir en ayunas?".into(), button: false };
        assert!(db.wa_apply(&s, &ev, now).is_empty());
        assert_eq!(db.whatsapp.iter().find(|m| m.id == log).unwrap().state, WaState::Respuesta);
        assert_eq!(db.appointments.iter().find(|a| a.id == appt).unwrap().state, ApptState::Planificada);
        assert_eq!(db.tasks.len(), tasks + 1);
        // «no» sin anular automáticamente: la cita sigue, pero hay tarea
        let to = db.whatsapp.iter().find(|m| m.id == log).unwrap().to.clone();
        let ev = WaEvent::Message { id: "t2".into(), from: to, user_id: String::new(), name: String::new(), context: String::new(), text: "no puedo".into(), button: false };
        assert!(db.wa_apply(&s, &ev, now).is_empty(), "sin respuesta automática");
        assert_eq!(db.appointments.iter().find(|a| a.id == appt).unwrap().state, ApptState::Planificada);
        assert_eq!(db.whatsapp.iter().find(|m| m.id == log).unwrap().state, WaState::NoViene);
        // mensaje de alguien sin recordatorio pendiente
        let ev = WaEvent::Message { id: "t3".into(), from: "34699000111".into(), user_id: String::new(), name: "Luis".into(), context: String::new(), text: "Hola, quería pedir cita".into(), button: false };
        db.wa_apply(&s, &ev, now);
        assert!(db.tasks.iter().any(|t| t.text.contains("quería pedir cita")));
        // fallo de entrega → tarea para llamar
        db.wa_apply(&s, &WaEvent::Status { wamid: "wamid.SENT".into(), status: "failed".into(), user_id: String::new(), error: "x".into() }, now);
        assert_eq!(db.whatsapp.iter().find(|m| m.id == log).unwrap().state, WaState::NoViene, "un fallo no borra la respuesta");
    }

    #[test]
    fn manual_reminders_do_not_duplicate_automatic_ones() {
        let sunday = friday() + Duration::days(2);
        let handled = |db: &Db, pid: u32| db.patient(pid).is_some_and(|p| p.accepts_gdpr && p.accepts_whatsapp && intl(&p.mobile).is_some());
        // sin WhatsApp automático: recordatorio manual para todos
        let mut db = Db::sample(friday());
        db.messages.clear();
        db.generate_messages(sunday);
        let manual: Vec<u32> = db.messages.iter().filter(|m| m.kind == crate::comms::MsgKind::RecordatorioCita).map(|m| m.patient_id).collect();
        assert!(manual.iter().any(|p| handled(&db, *p)), "la muestra debería tener pacientes con WhatsApp");
        // con WhatsApp automático: los que aceptan WhatsApp ya no salen en la lista manual
        let mut db = Db::sample(friday());
        db.messages.clear();
        db.wa_sender = "Recepción".into();
        db.generate_messages(sunday);
        assert!(db.messages.iter().filter(|m| m.kind == crate::comms::MsgKind::RecordatorioCita).all(|m| !handled(&db, m.patient_id)));
    }

    fn gw_cfg(url: &str, kind: GwKind) -> WaSettings {
        WaSettings {
            provider: Provider::Pasarela,
            gw_kind: kind,
            gw_url: url.into(),
            gw_key: "K".into(),
            gw_session: if kind == GwKind::Waha { "default".into() } else { "clinica".into() },
            gw_min_pause: 0,
            gw_max_pause: 0,
            gw_risk_ok: true,
            verify_token: "clave1".into(),
            ..Default::default()
        }
    }

    #[test]
    fn gateway_payloads_are_read() {
        // WAHA: mensaje entrante, mensaje propio (se ignora), id oculto (@lid) y acuse de lectura
        let waha = r#"{"id":"evt_1","timestamp":1741249702485,"event":"message","session":"default","payload":{"id":"false_34610200100@c.us_AAA","timestamp":1667561485,"from":"34610200100@c.us","fromMe":false,"to":"34911111111@c.us","body":"Sí","hasMedia":false,"replyTo":null,"_data":{"notifyName":"Ana"}},"engine":"WEBJS"}"#;
        assert_eq!(parse_gateway(waha), vec![WaEvent::Message { id: "false_34610200100@c.us_AAA".into(), from: "34610200100".into(), user_id: String::new(), name: "Ana".into(), context: String::new(), text: "Sí".into(), button: false }]);
        assert!(parse_gateway(&waha.replace("\"fromMe\":false", "\"fromMe\":true")).is_empty());
        let lid = r#"{"event":"message","payload":{"id":"false_999@lid_B","from":"999@lid","fromMe":false,"body":"no puedo","replyTo":{"id":"ABC123","body":"Hola"}}}"#;
        assert!(matches!(&parse_gateway(lid)[0], WaEvent::Message { from, user_id, context, .. } if from.is_empty() && user_id == "999@lid" && context == "ABC123"));
        let ack = r#"{"event":"message.ack","payload":{"id":"true_34610200100@c.us_ABC123","from":"34610200100@c.us","ack":3,"ackName":"READ"}}"#;
        assert_eq!(parse_gateway(ack), vec![WaEvent::Status { wamid: "true_34610200100@c.us_ABC123".into(), status: "read".into(), user_id: String::new(), error: String::new() }]);
        // Evolution API: mensaje (con dispositivo en el identificador), respuesta citando y estado
        let evo = r#"{"event":"messages.upsert","instance":"clinica","data":{"key":{"remoteJid":"34610200100:12@s.whatsapp.net","fromMe":false,"id":"3EB0X"},"pushName":"Ana","message":{"conversation":"No puedo ir"},"messageType":"conversation"},"apikey":"K"}"#;
        assert_eq!(parse_gateway(evo), vec![WaEvent::Message { id: "3EB0X".into(), from: "34610200100".into(), user_id: String::new(), name: "Ana".into(), context: String::new(), text: "No puedo ir".into(), button: false }]);
        let quoted = r#"{"event":"MESSAGES_UPSERT","data":{"key":{"remoteJid":"1@lid","remoteJidAlt":"34610200100@s.whatsapp.net","fromMe":false,"id":"Q1"},"message":{"extendedTextMessage":{"text":"vale","contextInfo":{"stanzaId":"BAE5"}}}}}"#;
        assert!(matches!(&parse_gateway(quoted)[0], WaEvent::Message { from, context, text, .. } if from == "34610200100" && context == "BAE5" && text == "vale"));
        let upd = r#"{"event":"messages.update","data":{"keyId":"BAE5","remoteJid":"34610200100@s.whatsapp.net","fromMe":true,"status":"DELIVERY_ACK"}}"#;
        assert_eq!(parse_gateway(upd), vec![WaEvent::Status { wamid: "BAE5".into(), status: "delivered".into(), user_id: String::new(), error: String::new() }]);
        assert!(parse_gateway("{}").is_empty() && parse_gateway("nada").is_empty());
        assert!(same_msg_id("true_34610200100@c.us_ABC123", "ABC123"));
        assert!(!same_msg_id("", ""));
    }

    #[test]
    fn gateway_mode_is_slow_safe_and_quiet() {
        let s = gw_cfg("http://127.0.0.1:1", GwKind::Waha);
        assert!(s.missing().is_empty(), "{:?}", s.missing());
        let not_ok = WaSettings { gw_risk_ok: false, ..s.clone() };
        assert!(not_ok.missing().iter().any(|m| m.contains("riesgo")));
        let mut db = Db::sample(friday());
        db.whatsapp.clear();
        // límite diario
        let one = WaSettings { gw_daily_max: 1, ..s.clone() };
        let due = db.wa_due(&one, at(friday(), 10, 30));
        assert_eq!(due.len(), 1);
        let (log, body) = db.wa_prepare(&one, due[0], at(friday(), 10, 30)).unwrap();
        assert!(db.wa_due(&one, at(friday(), 10, 31)).is_empty(), "ya se envió el máximo de hoy");
        // el texto lleva los datos de la cita y pide SÍ/NO
        let text = jstr(&body["text"]["body"]);
        assert!(text.contains("Responda SÍ") || text.contains("responda SÍ"), "{text}");
        assert!(!text.contains('['), "comodines sin rellenar: {text}");
        db.wa_sent(log, Ok("true_34x@c.us_ABC".into()), friday());
        // los chats normales de la clínica no generan tareas
        let tasks = db.tasks.len();
        db.wa_apply(&s, &WaEvent::Message { id: "n1".into(), from: "34699000111".into(), user_id: String::new(), name: "Luis".into(), context: String::new(), text: "Hola, ¿abren el sábado?".into(), button: false }, at(friday(), 11, 0));
        assert_eq!(db.tasks.len(), tasks);
        // un «sí» citando el recordatorio confirma; un mensaje posterior sin sí/no no lo cambia
        let appt = db.whatsapp.iter().find(|m| m.id == log).unwrap().appt_id;
        db.wa_apply(&s, &WaEvent::Message { id: "n2".into(), from: String::new(), user_id: String::new(), name: String::new(), context: "ABC".into(), text: "SI".into(), button: false }, at(friday(), 11, 1));
        assert_eq!(db.appointments.iter().find(|a| a.id == appt).unwrap().state, ApptState::Confirmada);
        let to = db.whatsapp.iter().find(|m| m.id == log).unwrap().to.clone();
        db.wa_apply(&s, &WaEvent::Message { id: "n3".into(), from: to, user_id: String::new(), name: String::new(), context: String::new(), text: "¿Puedo ir con mi hijo?".into(), button: false }, at(friday(), 11, 2));
        assert_eq!(db.whatsapp.iter().find(|m| m.id == log).unwrap().state, WaState::Confirmada);
        assert_eq!(db.tasks.len(), tasks, "con la pasarela recepción ya lo ve en su WhatsApp");
        assert!(pause_secs(20, 60) >= 20 && pause_secs(20, 60) <= 60);
    }

    /// Simula una pasarela WAHA y otra Evolution API en este ordenador.
    /// T-R7-6. El aviso de la pasarela no acepta más de `MAX_BODY`.
    #[test]
    fn the_gateway_hook_refuses_oversized_bodies() {
        let hook = listen(0, false, HookCfg { verify_token: "clave1".into(), accepting: true, ..Default::default() }, || {}).unwrap();
        let big = "x".repeat(MAX_BODY as usize + 10);
        let code = match agent().post(&format!("http://127.0.0.1:{}/pasarela?clave=clave1", hook.port)).send_string(&big) {
            Ok(r) => r.status(),
            Err(ureq::Error::Status(c, _)) => c,
            Err(e) => panic!("{e}"),
        };
        assert_eq!(code, 413);
    }

    #[test]
    fn gateway_round_trip_with_fake_waha_and_evolution() {
        let gw = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = gw.server_addr().to_ip().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for mut req in gw.incoming_requests() {
                let key = req.headers().iter().find(|h| h.field.equiv("X-Api-Key") || h.field.equiv("apikey")).map(|h| h.value.as_str().to_string()).unwrap_or_default();
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let url = req.url().to_string();
                seen2.lock().unwrap().push((url.clone(), key.clone(), body.clone()));
                let r = if key != "K" {
                    tiny_http::Response::from_string(r#"{"statusCode":401,"message":"Unauthorized"}"#).with_status_code(401)
                } else if url == "/api/sendText" {
                    let v: Value = serde_json::from_str(&body).unwrap();
                    tiny_http::Response::from_string(format!(r#"{{"id":"true_{}_ABC123","body":"x","fromMe":true}}"#, v["chatId"].as_str().unwrap()))
                } else if url.starts_with("/api/default/lids/pn/") {
                    tiny_http::Response::from_string(r#"{"lid":"999@lid","pn":"34x@c.us"}"#)
                } else if url == "/api/sessions/default" {
                    tiny_http::Response::from_string(r#"{"name":"default","status":"WORKING"}"#)
                } else if url == "/message/sendText/clinica" {
                    tiny_http::Response::from_string(r#"{"key":{"remoteJid":"34610200100@s.whatsapp.net","fromMe":true,"id":"BAE5XYZ"},"status":"PENDING"}"#)
                } else if url == "/instance/connectionState/clinica" {
                    tiny_http::Response::from_string(r#"{"instance":{"instanceName":"clinica","state":"open"}}"#)
                } else {
                    tiny_http::Response::from_string(r#"{"statusCode":404,"message":"Not Found"}"#).with_status_code(404)
                };
                let _ = req.respond(r);
            }
        });
        let base = format!("http://127.0.0.1:{port}");
        let s = gw_cfg(&base, GwKind::Waha);
        assert_eq!(gateway_status(&s).unwrap(), "WhatsApp conectado a la pasarela");
        assert!(gateway_status(&WaSettings { gw_key: "MAL".into(), ..s.clone() }).unwrap_err().contains("clave"));
        assert!(gateway_status(&WaSettings { gw_session: "otra".into(), ..s.clone() }).unwrap_err().contains("no existe"));
        assert!(gateway_status(&WaSettings { gw_url: "http://127.0.0.1:1".into(), ..s.clone() }).unwrap_err().contains("No hay conexión"));

        let mut db = Db::sample(friday());
        db.whatsapp.clear();
        let appt = db.wa_due(&s, at(friday(), 10, 30))[0];
        let (log, body) = db.wa_prepare(&s, appt, now()).unwrap();
        let (jobs, results) = start_worker(|| {});
        jobs.send((s.clone(), JobKind::Reminder(log), body)).unwrap();
        let r = results.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert_eq!(r.extra, "999@lid");
        let wamid = r.result.clone().unwrap();
        assert!(wamid.ends_with("_ABC123"));
        db.wa_sent(log, r.result, friday());
        db.wa_set_user(log, &r.extra);
        {
            let seen = seen.lock().unwrap();
            let (url, key, body) = &seen.iter().find(|x| x.0 == "/api/sendText").unwrap().clone();
            assert_eq!((url.as_str(), key.as_str()), ("/api/sendText", "K"));
            let v: Value = serde_json::from_str(body).unwrap();
            assert_eq!(v["session"], "default");
            assert!(v["chatId"].as_str().unwrap().ends_with("@c.us"));
        }
        // aviso de la pasarela al programa: sin clave se rechaza; con clave, el «sí» del @lid confirma
        let hook = listen(0, false, HookCfg { verify_token: "clave1".into(), accepting: true, ..Default::default() }, || {}).unwrap();
        let hook_url = format!("http://127.0.0.1:{}/pasarela", hook.port);
        let payload = r#"{"event":"message","session":"default","payload":{"id":"false_999@lid_IN1","from":"999@lid","fromMe":false,"body":"Sí, allí estaré"}}"#;
        let a = agent();
        assert!(matches!(a.post(&format!("{hook_url}?clave=mal")).send_string(payload), Err(ureq::Error::Status(403, _))));
        assert!(matches!(a.get(&format!("{hook_url}?clave=clave1")).call(), Err(ureq::Error::Status(405, _))));
        assert_eq!(a.post(&format!("{hook_url}?clave=clave1")).send_string(payload).unwrap().status(), 200);
        let ev = hook.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let replies = db.wa_apply(&s, &ev, at(friday(), 11, 0));
        assert_eq!(db.appointments.iter().find(|x| x.id == appt).unwrap().state, ApptState::Confirmada);
        // la respuesta automática sale también por la pasarela
        let (rlog, to, text) = replies[0].clone();
        jobs.send((s.clone(), JobKind::Reply(rlog), text_request(&to, &text))).unwrap();
        assert!(results.recv_timeout(std::time::Duration::from_secs(10)).unwrap().result.is_ok());
        // Evolution API
        let e = gw_cfg(&base, GwKind::Evolution);
        assert_eq!(gateway_send(&e, "34610200100", "hola").unwrap(), "BAE5XYZ");
        assert!(gateway_status(&e).is_ok());
        let seen = seen.lock().unwrap();
        let evo = seen.iter().find(|x| x.0 == "/message/sendText/clinica").unwrap();
        assert_eq!(evo.1, "K");
        assert!(evo.2.contains("\"number\":\"34610200100\""));
    }

    /// Simula los servidores de Meta en este ordenador: envío y respuesta firmada.
    #[test]
    fn full_round_trip_with_fake_meta() {
        // «Graph API» falsa
        let graph = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let gport = graph.server_addr().to_ip().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for (n, mut req) in graph.incoming_requests().enumerate() {
                let auth = req.headers().iter().find(|h| h.field.equiv("Authorization")).map(|h| h.value.as_str().to_string()).unwrap_or_default();
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                seen2.lock().unwrap().push((req.url().to_string(), auth.clone(), body));
                let resp = if auth == "Bearer TOKEN" {
                    tiny_http::Response::from_string(format!(r#"{{"messaging_product":"whatsapp","contacts":[{{"input":"x","wa_id":"x"}}],"messages":[{{"id":"wamid.FAKE{n}"}}]}}"#))
                } else {
                    tiny_http::Response::from_string(r#"{"error":{"message":"Invalid OAuth access token","type":"OAuthException","code":190}}"#).with_status_code(401)
                };
                let _ = req.respond(resp);
            }
        });
        let s = WaSettings { api_base: format!("http://127.0.0.1:{gport}/v27.0"), ..cfg() };
        let mut db = Db::sample(friday());
        db.whatsapp.clear();
        let appt = db.wa_due(&s, at(friday(), 10, 30))[0];
        let (log, body) = db.wa_prepare(&s, appt, now()).unwrap();
        let (jobs, results) = start_worker(|| {});
        jobs.send((s.clone(), JobKind::Reminder(log), body)).unwrap();
        let res = results.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        let wamid = res.result.clone().unwrap();
        db.wa_sent(log, res.result, friday());
        {
            let seen = seen.lock().unwrap();
            assert_eq!(seen[0].0, "/v27.0/1065/messages");
            assert_eq!(seen[0].1, "Bearer TOKEN");
            assert!(seen[0].2.contains("\"recordatorio_cita\""));
        }
        // token incorrecto: error legible
        let bad = WaSettings { token: "MAL".into(), ..s.clone() };
        let err = post_message(&bad, &text_request("34", "x")).unwrap_err();
        assert!(err.contains("190") && err.contains("Invalid OAuth"), "{err}");

        // receptor de avisos
        let hook = listen(0, false, HookCfg { verify_token: "verifica".into(), app_secret: "secreto".into(), accepting: true, ..Default::default() }, || {}).unwrap();
        let base = format!("http://127.0.0.1:{}", hook.port);
        let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(5)).build();
        let ch = agent.get(&format!("{base}/whatsapp?hub.mode=subscribe&hub.challenge=4242&hub.verify_token=verifica")).call().unwrap().into_string().unwrap();
        assert_eq!(ch, "4242");
        assert!(matches!(agent.get(&format!("{base}/whatsapp?hub.mode=subscribe&hub.challenge=1&hub.verify_token=mal")).call(), Err(ureq::Error::Status(403, _))));
        let payload = format!(
            r#"{{"object":"whatsapp_business_account","entry":[{{"id":"1","changes":[{{"value":{{"messages":[{{"context":{{"id":"{wamid}"}},"from":"34000","id":"wamid.IN1","type":"button","button":{{"payload":"No puedo ir","text":"No puedo ir"}}}}]}},"field":"messages"}}]}}]}}"#
        );
        // sin firma o con firma falsa: rechazado
        assert!(matches!(agent.post(&format!("{base}/whatsapp")).send_string(&payload), Err(ureq::Error::Status(403, _))));
        assert!(matches!(agent.post(&format!("{base}/whatsapp")).set("X-Hub-Signature-256", "sha256=00").send_string(&payload), Err(ureq::Error::Status(403, _))));
        assert!(hook.rx.try_recv().is_err());
        let sig = format!("sha256={}", hex(&hmac_sha256(b"secreto", payload.as_bytes())));
        let r = agent.post(&format!("{base}/whatsapp")).set("X-Hub-Signature-256", &sig).set("Content-Type", "application/json").send_string(&payload).unwrap();
        assert_eq!(r.status(), 200);
        let ev = hook.rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let replies = db.wa_apply(&s, &ev, at(friday(), 11, 0));
        assert_eq!(db.appointments.iter().find(|a| a.id == appt).unwrap().state, ApptState::Anulada);
        // la respuesta automática sale por el mismo camino
        let (rlog, to, text) = replies[0].clone();
        jobs.send((s.clone(), JobKind::Reply(rlog), text_request(&to, &text))).unwrap();
        assert!(results.recv_timeout(std::time::Duration::from_secs(10)).unwrap().result.is_ok());
        assert!(seen.lock().unwrap()[2].2.contains("Le llamaremos"));
        // con el receptor en pausa Meta recibe 503 (y lo reintentará)
        hook.cfg.lock().unwrap().accepting = false;
        assert!(matches!(agent.post(&format!("{base}/whatsapp")).set("X-Hub-Signature-256", &sig).send_string(&payload), Err(ureq::Error::Status(503, _))));
    }
}
