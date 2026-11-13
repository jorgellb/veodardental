//! Análisis de imágenes dentales con inteligencia artificial.
//!
//! La clínica da de alta «agentes»: un proveedor (OpenRouter, OpenAI, Google
//! Gemini, Anthropic, Groq, Mistral, Ollama en el propio ordenador o cualquier
//! servicio compatible con OpenAI), un modelo con visión, su clave y si es
//! gratuito o de pago. Al analizar se recorre la lista en orden según el modo
//! elegido (sólo gratuitos, sólo de pago, o primero los gratuitos y si fallan
//! los de pago): si un agente falla (límite de uso, sin saldo, caído, respuesta
//! no válida…) responde el siguiente. Se envía sólo la imagen (sin nombre ni
//! datos del paciente) y, si se quiere, la edad. El resultado es orientativo:
//! hallazgos por pieza y una propuesta de tratamiento que el dentista revisa.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

// ───────────────────────────── proveedores ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Provider {
    #[default]
    OpenRouter,
    OpenAi,
    Gemini,
    Anthropic,
    Groq,
    Mistral,
    Ollama,
    Custom,
}

impl Provider {
    pub const ALL: [Provider; 8] = [Provider::OpenRouter, Provider::Gemini, Provider::OpenAi, Provider::Anthropic, Provider::Groq, Provider::Mistral, Provider::Ollama, Provider::Custom];

    pub fn label(self) -> &'static str {
        match self {
            Provider::OpenRouter => "OpenRouter",
            Provider::OpenAi => "OpenAI",
            Provider::Gemini => "Google Gemini",
            Provider::Anthropic => "Anthropic (Claude)",
            Provider::Groq => "Groq",
            Provider::Mistral => "Mistral",
            Provider::Ollama => "Ollama (en este ordenador)",
            Provider::Custom => "Otro compatible con OpenAI",
        }
    }

    pub fn default_url(self) -> &'static str {
        match self {
            Provider::OpenRouter => "https://openrouter.ai/api/v1",
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            Provider::Anthropic => "https://api.anthropic.com/v1",
            Provider::Groq => "https://api.groq.com/openai/v1",
            Provider::Mistral => "https://api.mistral.ai/v1",
            Provider::Ollama => "http://localhost:11434/v1",
            Provider::Custom => "",
        }
    }

    /// Dónde se consigue la clave.
    pub fn key_url(self) -> &'static str {
        match self {
            Provider::OpenRouter => "https://openrouter.ai/keys",
            Provider::OpenAi => "https://platform.openai.com/api-keys",
            Provider::Gemini => "https://aistudio.google.com/apikey",
            Provider::Anthropic => "https://console.anthropic.com/settings/keys",
            Provider::Groq => "https://console.groq.com/keys",
            Provider::Mistral => "https://console.mistral.ai/api-keys",
            Provider::Ollama | Provider::Custom => "",
        }
    }

    pub fn needs_key(self) -> bool {
        !matches!(self, Provider::Ollama | Provider::Custom)
    }

    /// Nota corta sobre precio y privacidad del proveedor.
    pub fn note(self) -> &'static str {
        match self {
            Provider::OpenRouter => "Un solo sitio con cientos de modelos y una sola clave; los que terminan en «:free» son gratuitos (con límite diario) y «openrouter/free» elige solo uno gratuito disponible.",
            Provider::Gemini => "Tiene un nivel gratuito con límites; en ese nivel Google puede usar los datos para mejorar sus productos.",
            Provider::OpenAi => "De pago por uso.",
            Provider::Anthropic => "De pago por uso.",
            Provider::Groq => "Nivel gratuito con límites de uso.",
            Provider::Mistral => "Nivel gratuito para pruebas y planes de pago.",
            Provider::Ollama => "Gratuito y privado: el modelo funciona en este ordenador y la imagen no sale de la clínica (necesita un equipo potente).",
            Provider::Custom => "Cualquier servicio con el formato de OpenAI (LM Studio, vLLM, Azure…).",
        }
    }

    /// Modelos con visión sugeridos (modelo, gratuito). La lista real se puede consultar con «Ver modelos».
    pub fn suggested(self) -> &'static [(&'static str, bool)] {
        match self {
            Provider::OpenRouter => &[
                ("google/gemma-4-31b-it:free", true),
                ("nvidia/nemotron-3.5-super-vl:free", true),
                ("google/gemma-4-26b-a4b-it:free", true),
                ("openrouter/free", true),
                ("anthropic/claude-sonnet-5.5", false),
                ("google/gemini-3.8-flash", false),
                ("openai/gpt-5.6-luna", false),
                ("qwen/qwen3.8-flash", false),
            ],
            Provider::OpenAi => &[("gpt-5.6-luna", false), ("gpt-5.6-terra", false)],
            Provider::Gemini => &[("gemini-3.8-flash", false), ("gemini-3.5-flash-lite", false)],
            Provider::Anthropic => &[("claude-sonnet-5-5", false), ("claude-haiku-5-5", false), ("claude-opus-5-5", false)],
            Provider::Groq => &[("meta-llama/llama-4-scout-17b-16e-instruct", true)],
            Provider::Mistral => &[("mistral-medium-latest", false), ("mistral-small-latest", false)],
            Provider::Ollama => &[("qwen2.5vl", true), ("gemma3", true), ("llava", true)],
            Provider::Custom => &[],
        }
    }

    fn anthropic(self) -> bool {
        self == Provider::Anthropic
    }
}

/// Un agente: proveedor + modelo + clave, gratuito o de pago.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub id: u32,
    pub name: String,
    pub provider: Provider,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    pub model: String,
    pub free: bool,
    pub enabled: bool,
}

impl Agent {
    pub fn new(id: u32, provider: Provider, model: &str, free: bool) -> Self {
        Agent { id, name: format!("{} · {}", provider.label().split(' ').next().unwrap_or(""), short_model(model)), provider, base_url: provider.default_url().into(), api_key: String::new(), model: model.into(), free, enabled: true }
    }
    fn ready(&self) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err("no tiene modelo".into());
        }
        if self.base_url.trim().is_empty() {
            return Err("no tiene dirección del servicio".into());
        }
        if self.provider.needs_key() && self.api_key.trim().is_empty() {
            return Err("falta la clave".into());
        }
        Ok(())
    }
}

/// Nombre corto de un modelo (sin el proveedor delante).
pub fn short_model(m: &str) -> String {
    m.rsplit('/').next().unwrap_or(m).to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Mode {
    /// Primero los gratuitos y, si ninguno responde, los de pago.
    #[default]
    FreeThenPaid,
    FreeOnly,
    PaidOnly,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::FreeOnly, Mode::FreeThenPaid, Mode::PaidOnly];
    pub fn label(self) -> &'static str {
        match self {
            Mode::FreeOnly => "Sólo gratuitos",
            Mode::FreeThenPaid => "Gratuitos y, si fallan, de pago",
            Mode::PaidOnly => "Sólo de pago",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Mode::FreeOnly => "Gratuitos",
            Mode::FreeThenPaid => "Gratuitos → pago",
            Mode::PaidOnly => "De pago",
        }
    }
}

fn default_timeout() -> u32 {
    90
}

fn default_max_paid() -> u32 {
    20
}

/// Versión de las instrucciones que se mandan al modelo (se guarda con cada
/// análisis: si cambian, se sabe con cuáles se hizo cada uno).
pub const PROMPT_VERSION: &str = "radiografia-1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IaConfig {
    pub enabled: bool,
    /// La clínica ha leído y acepta el aviso de protección de datos.
    pub accepted: bool,
    pub mode: Mode,
    pub agents: Vec<Agent>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u32,
    /// Enviar la edad del paciente (ayuda a distinguir dentición temporal).
    #[serde(default)]
    pub send_age: bool,
    /// Indicaciones adicionales para todos los análisis.
    #[serde(default)]
    pub extra: String,
    /// Análisis de pago como máximo al mes (0: ninguno). Los gratuitos no cuentan.
    #[serde(default = "default_max_paid")]
    pub max_paid_month: u32,
}

impl Default for IaConfig {
    fn default() -> Self {
        // agentes de ejemplo, listos en cuanto se pegue la clave
        IaConfig {
            enabled: false,
            accepted: false,
            mode: Mode::FreeThenPaid,
            agents: vec![
                Agent::new(1, Provider::OpenRouter, "google/gemma-4-31b-it:free", true),
                Agent::new(2, Provider::OpenRouter, "nvidia/nemotron-3.5-super-vl:free", true),
                // el «router» gratuito de OpenRouter elige él mismo un modelo gratuito disponible
                Agent::new(3, Provider::OpenRouter, "openrouter/free", true),
                Agent { enabled: false, ..Agent::new(4, Provider::Anthropic, "claude-sonnet-5-5", false) },
            ],
            timeout_secs: default_timeout(),
            send_age: true,
            extra: String::new(),
            max_paid_month: default_max_paid(),
        }
    }
}

impl IaConfig {
    pub fn next_agent_id(&self) -> u32 {
        self.agents.iter().map(|a| a.id).max().unwrap_or(0) + 1
    }
}

/// Agentes que se probarán, en orden, para un modo.
pub fn chain(cfg: &IaConfig, mode: Mode) -> Vec<Agent> {
    let on = |a: &&Agent| a.enabled;
    let free: Vec<Agent> = cfg.agents.iter().filter(on).filter(|a| a.free).cloned().collect();
    let paid: Vec<Agent> = cfg.agents.iter().filter(on).filter(|a| !a.free).cloned().collect();
    match mode {
        Mode::FreeOnly => free,
        Mode::PaidOnly => paid,
        Mode::FreeThenPaid => free.into_iter().chain(paid).collect(),
    }
}

// ───────────────────────────── resultado ─────────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Finding {
    pub tooth: Option<u8>,
    pub kind: String,
    #[serde(default)]
    pub faces: Vec<String>,
    pub description: String,
    pub severity: String,
    pub confidence: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Proposal {
    pub kind: String,
    pub teeth: Vec<u8>,
    #[serde(default)]
    pub faces: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Analysis {
    pub summary: String,
    pub quality: String,
    pub findings: Vec<Finding>,
    pub proposals: Vec<Proposal>,
}

/// Un intento con un agente.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attempt {
    pub agent: String,
    pub model: String,
    pub free: bool,
    pub ok: bool,
    pub message: String,
    pub millis: u64,
}

pub struct Outcome {
    pub result: Result<(Agent, Analysis), String>,
    pub attempts: Vec<Attempt>,
}

/// Análisis guardado en la ficha.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IaReport {
    pub id: u32,
    pub patient_id: u32,
    pub image_id: u32,
    pub when: chrono::NaiveDateTime,
    pub agent: String,
    pub model: String,
    pub free: bool,
    pub analysis: Analysis,
    #[serde(default)]
    pub attempts: Vec<Attempt>,
    #[serde(default)]
    pub user: String,
    /// Propuestas ya pasadas al plan de tratamiento.
    #[serde(default)]
    pub planned: Vec<usize>,
    /// Revisión del profesional. Hasta entonces es un borrador: no pasa al
    /// plan de tratamiento ni a las notas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<IaReview>,
    /// Versión de las instrucciones con que se pidió ([`PROMPT_VERSION`]).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt_version: String,
}

/// Lo que decidió el profesional sobre un análisis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IaReview {
    pub at: chrono::NaiveDateTime,
    pub by: String,
    /// Lo da por bueno (si no, lo descarta: queda guardado, pero no se usa).
    pub valid: bool,
}

/// Lo que se envía: la imagen (JPEG) y datos no identificativos.
pub struct Request {
    pub jpeg: Vec<u8>,
    pub kind: String,
    pub teeth: Vec<u8>,
    pub age: Option<u32>,
}

// ───────────────────────────── revisión y uso ─────────────────────────────

/// ¿La propuesta habla de piezas que no son las de la imagen? (si la imagen
/// dice qué piezas muestra).
pub fn outside_image(image_teeth: &[u8], p: &Proposal) -> bool {
    !image_teeth.is_empty() && p.teeth.iter().any(|t| !image_teeth.contains(t))
}

impl crate::model::Db {
    /// Análisis de pago hechos en el mes de `day`.
    pub fn ia_paid_this_month(&self, day: chrono::NaiveDate) -> u32 {
        use chrono::Datelike;
        self.ia_reports.iter().filter(|r| !r.free && r.when.date().year() == day.year() && r.when.date().month() == day.month()).count() as u32
    }

    /// Por qué el usuario que ha entrado no puede revisar un análisis (ha de
    /// ser un profesional colegiado, como para recetar). `None`: puede.
    pub fn ia_reviewer_problem(&self) -> Option<String> {
        let name = &self.config.current_user;
        let Some(u) = self.users.iter().find(|u| &u.name == name && u.active) else { return Some("Entre con su usuario para revisarlo.".into()) };
        let Some(d) = u.doctor_id.and_then(|d| self.doctor(d)) else { return Some("Lo revisa un profesional: su usuario no está unido a un colaborador.".into()) };
        crate::finance::collaborator_problem(d).map(|p| p.replace("no puede recetar", "no puede revisarlo").replace("solo recetan", "lo revisan"))
    }

    /// El profesional da por bueno (o descarta) un análisis.
    pub fn review_ia_report(&mut self, id: u32, valid: bool) -> Result<(), String> {
        if let Some(p) = self.ia_reviewer_problem() {
            return Err(p);
        }
        let by = self.config.current_user.clone();
        let r = self.ia_reports.iter_mut().find(|r| r.id == id).ok_or("El análisis ya no existe.")?;
        if r.review.is_some() {
            return Err("Este análisis ya está revisado.".into());
        }
        r.review = Some(IaReview { at: crate::model::now(), by: by.clone(), valid });
        let pid = r.patient_id;
        let what = if valid { "Análisis con IA revisado" } else { "Análisis con IA descartado" };
        let who = self.patient_name(pid);
        self.audit(what, &format!("{who} · por {by}"));
        Ok(())
    }

    /// Pasa al plan de tratamiento (como «previsto») las propuestas elegidas
    /// de un análisis revisado. Cada tratamiento dice de dónde viene. Devuelve
    /// cuántos ha añadido.
    pub fn plan_from_ia(&mut self, id: u32, chosen: &[usize], today: chrono::NaiveDate) -> Result<usize, String> {
        let r = self.ia_reports.iter().find(|r| r.id == id).cloned().ok_or("El análisis ya no existe.")?;
        let rev = r.review.clone().filter(|v| v.valid).ok_or("Revise antes el análisis: hasta entonces es un borrador.")?;
        let doctor = self.patient(r.patient_id).map(|p| p.doctor_id).unwrap_or(1);
        let mut added = vec![];
        for (i, p) in r.analysis.proposals.iter().enumerate() {
            if !chosen.contains(&i) || r.planned.contains(&i) {
                continue;
            }
            if let Some((code, desc)) = self.ia_treatment(p) {
                let faces = if proposal_kind(&p.kind) == Some(crate::model::OdgKind::Obturacion) { p.faces.clone() } else { vec![] };
                let e = self.add_entry(r.patient_id, &code, p.teeth.clone(), faces, crate::model::TxState::Previsto, doctor, today);
                if let Some(x) = self.entries.iter_mut().find(|x| x.id == e) {
                    x.notes = format!("Propuesto por IA ({}, {}) y revisado por {}.", r.agent, short_model(&r.model), rev.by);
                }
                added.push((i, desc));
            }
        }
        if let Some(rep) = self.ia_reports.iter_mut().find(|x| x.id == id) {
            rep.planned.extend(added.iter().map(|a| a.0));
        }
        let names: Vec<String> = added.iter().map(|a| a.1.clone()).collect();
        let who = self.patient_name(r.patient_id);
        self.audit("Plan desde análisis con IA", &format!("{who} · {}", names.join(", ")));
        Ok(added.len())
    }

    /// Tratamiento del catálogo para una propuesta: (código, descripción).
    pub fn ia_treatment(&self, p: &Proposal) -> Option<(String, String)> {
        use crate::model::{OdgKind, Treatment, fold};
        let found = match proposal_kind(&p.kind) {
            Some(OdgKind::Obturacion) => {
                // la obturación que corresponde al número de caras, si el catálogo las distingue
                let list: Vec<&Treatment> = self.treatments.iter().filter(|t| t.active && t.odg == OdgKind::Obturacion).collect();
                let n = p.faces.len().max(1);
                let want = if n == 1 { "1 cara".to_string() } else { format!("{n} caras") };
                list.iter().find(|t| fold(&t.description).contains(&want)).or(list.first()).copied()
            }
            Some(k) => self.treatments.iter().find(|t| t.active && t.odg == k),
            None => {
                let words = proposal_words(&p.kind);
                self.treatments.iter().find(|t| t.active && words.iter().any(|w| fold(&t.description).contains(&fold(w))))
            }
        };
        found.map(|t| (t.code.clone(), t.description.clone()))
    }
}

// ───────────────────────────── preparación ─────────────────────────────

/// Codificación base64 estándar.
pub fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = ((c[0] as u32) << 16) | ((*c.get(1).unwrap_or(&0) as u32) << 8) | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Imagen reducida (lado mayor 1568 px) y comprimida en JPEG. Así sólo viajan
/// los píxeles: ningún metadato del fichero original.
pub fn jpeg_for_ai(img: &image::RgbImage) -> Result<Vec<u8>, String> {
    let (w, h) = img.dimensions();
    let max = 1568u32;
    let img = if w.max(h) > max {
        let k = max as f32 / w.max(h) as f32;
        image::imageops::resize(img, ((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1), image::imageops::FilterType::Triangle)
    } else {
        img.clone()
    };
    let mut out = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
    enc.encode_image(&img).map_err(|e| format!("No se pudo preparar la imagen: {e}"))?;
    Ok(out)
}

const SYSTEM: &str = "Eres un asistente de apoyo al diagnóstico para dentistas. Analizas imágenes dentales (radiografías periapicales, de aleta de mordida, panorámicas, cortes de TAC y fotografías intraorales) y describes con prudencia lo que se ve. No das un diagnóstico definitivo: decide el dentista. Usa la numeración FDI de las piezas (11-48 permanentes, 51-85 temporales). Responde sólo con un objeto JSON válido, sin texto antes ni después.";

/// Instrucciones del análisis.
pub fn prompt(req: &Request, extra: &str) -> String {
    let mut s = format!("Tipo de imagen: {}.", req.kind);
    if !req.teeth.is_empty() {
        let t: Vec<String> = req.teeth.iter().map(|t| t.to_string()).collect();
        s += &format!(" Piezas que debería mostrar: {}.", t.join(", "));
    }
    if let Some(a) = req.age {
        s += &format!(" Edad del paciente: {a} años.");
    }
    s += r#"

Analiza la imagen y devuelve este JSON (en español):
{
  "resumen": "2 a 4 frases con lo más importante",
  "calidad": "buena, aceptable o deficiente; si no es buena, por qué",
  "hallazgos": [
    {"pieza": "36", "tipo": "caries | lesion_periapical | perdida_osea | resto_radicular | fractura | obturacion | endodoncia | corona | implante | ausencia | calculo | otro",
     "caras": ["O", "D"], "descripcion": "qué se ve y dónde", "gravedad": "baja | media | alta", "confianza": 0.7}
  ],
  "propuesta": [
    {"tipo": "obturacion | endodoncia | extraccion | corona | implante | puente | raspado | limpieza | revision | otro",
     "piezas": ["36"], "caras": ["O", "D"], "motivo": "por qué"}
  ]
}
Las caras son M (mesial), D (distal), O (oclusal o incisal), V (vestibular) y L (lingual o palatina). Incluye también los tratamientos ya hechos que se ven (obturaciones, endodoncias, coronas, implantes) como hallazgos de gravedad baja. Si no ves nada relevante, deja las listas vacías. Si la imagen no es dental o no se puede valorar, dilo en «resumen» y deja las listas vacías."#;
    if !extra.trim().is_empty() {
        s += &format!("\n\nIndicaciones de la clínica: {}", extra.trim());
    }
    s
}

// ───────────────────────────── lectura de la respuesta ─────────────────────────────

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn tooth_of(v: &Value) -> Option<u8> {
    let s = text_of(v);
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).take(2).collect();
    let t: u8 = digits.parse().ok()?;
    let (q, u) = (t / 10, t % 10);
    let ok = match q {
        1..=4 => (1..=8).contains(&u),
        5..=8 => (1..=5).contains(&u),
        _ => false,
    };
    ok.then_some(t)
}

fn list_of(v: Option<&Value>) -> Vec<Value> {
    match v {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::String(s)) if !s.trim().is_empty() => s.split([',', ' ', '-', '/']).filter(|x| !x.is_empty()).map(|x| Value::String(x.into())).collect(),
        Some(Value::Number(n)) => vec![Value::Number(n.clone())],
        _ => vec![],
    }
}

fn faces_of(v: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for f in list_of(v) {
        let f = text_of(&f).to_uppercase();
        let f = match f.as_str() {
            "I" | "INCISAL" | "OCLUSAL" => "O".to_string(),
            "P" | "PALATINA" | "LINGUAL" => "L".to_string(),
            "B" | "VESTIBULAR" | "BUCAL" => "V".to_string(),
            "MESIAL" => "M".to_string(),
            "DISTAL" => "D".to_string(),
            x => x.chars().take(1).collect(),
        };
        if ["M", "D", "O", "V", "L"].contains(&f.as_str()) && !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

fn fold_kind(s: &str) -> String {
    crate::model::fold(s).replace(' ', "_")
}

/// Lee la respuesta del modelo (tolera bloques ```json, texto alrededor y tipos variados).
pub fn parse_analysis(text: &str) -> Result<Analysis, String> {
    let start = text.find('{').ok_or("la respuesta no trae el resultado en el formato pedido")?;
    let end = text.rfind('}').ok_or("la respuesta está incompleta")?;
    if end <= start {
        return Err("la respuesta está incompleta".into());
    }
    let v: Value = serde_json::from_str(&text[start..=end]).map_err(|_| "la respuesta no es un JSON válido".to_string())?;
    let get = |k: &str| v.get(k).or_else(|| v.get(k.replace("resumen", "summary")));
    let summary = get("resumen").map(text_of).unwrap_or_default();
    let quality = v.get("calidad").map(text_of).unwrap_or_default();
    let findings: Vec<Finding> = list_of(v.get("hallazgos"))
        .iter()
        .map(|f| {
            let conf = match f.get("confianza") {
                Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) as f32,
                Some(Value::String(s)) => s.trim_end_matches('%').trim().replace(',', ".").parse::<f32>().unwrap_or(0.0),
                _ => 0.0,
            };
            Finding {
                tooth: f.get("pieza").and_then(tooth_of),
                kind: fold_kind(&f.get("tipo").map(text_of).unwrap_or_default()),
                faces: faces_of(f.get("caras")),
                description: f.get("descripcion").map(text_of).unwrap_or_default(),
                severity: crate::model::fold(&f.get("gravedad").map(text_of).unwrap_or_default()),
                confidence: if conf > 1.0 { (conf / 100.0).min(1.0) } else { conf.max(0.0) },
            }
        })
        .collect();
    let proposals: Vec<Proposal> = list_of(v.get("propuesta"))
        .iter()
        .map(|p| Proposal {
            kind: fold_kind(&p.get("tipo").map(text_of).unwrap_or_default()),
            teeth: list_of(p.get("piezas")).iter().filter_map(tooth_of).collect(),
            faces: faces_of(p.get("caras")),
            reason: p.get("motivo").map(text_of).unwrap_or_default(),
        })
        .collect();
    if summary.is_empty() && findings.is_empty() && proposals.is_empty() {
        return Err("la respuesta llegó vacía".into());
    }
    Ok(Analysis { summary, quality, findings, proposals })
}

// ───────────────────────────── conexión ─────────────────────────────

fn http(timeout: u32) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(timeout.max(10) as u64)).build()
}

fn endpoint(a: &Agent, path: &str) -> String {
    format!("{}/{}", a.base_url.trim().trim_end_matches('/'), path)
}

/// Mensaje claro a partir de un error del servicio.
fn status_message(code: u16, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body).ok().and_then(|v| v.pointer("/error/message").or_else(|| v.get("error")).or_else(|| v.get("message")).map(text_of)).unwrap_or_else(|| body.chars().take(160).collect());
    let what = match code {
        400 => "petición rechazada (¿el modelo admite imágenes?)",
        401 | 403 => "clave no válida o sin permiso",
        402 => "sin saldo en la cuenta",
        404 => "modelo o dirección no encontrados",
        408 | 504 => "el servicio tardó demasiado",
        413 => "imagen demasiado grande para este servicio",
        429 => "límite de uso alcanzado",
        500..=599 => "el servicio falló",
        _ => "error del servicio",
    };
    let detail: String = detail.chars().take(160).collect();
    if detail.trim().is_empty() { format!("{what} ({code})") } else { format!("{what} ({code}): {detail}") }
}

fn send(req: ureq::Request, body: &Value) -> Result<Value, String> {
    match req.set("Content-Type", "application/json").send_string(&body.to_string()) {
        Ok(r) => {
            let txt = r.into_string().map_err(|e| format!("respuesta ilegible: {e}"))?;
            serde_json::from_str(&txt).map_err(|_| "respuesta del servicio no válida".to_string())
        }
        Err(ureq::Error::Status(code, r)) => Err(status_message(code, &r.into_string().unwrap_or_default())),
        Err(ureq::Error::Transport(t)) => Err(match t.kind() {
            ureq::ErrorKind::Dns => "no se encuentra el servicio (¿hay conexión a internet?)".into(),
            ureq::ErrorKind::ConnectionFailed => "no se puede conectar con el servicio".into(),
            ureq::ErrorKind::Io => "se cortó la conexión o tardó demasiado".into(),
            _ => format!("fallo de conexión: {t}"),
        }),
    }
}

/// Llama a un agente con texto y, si se da, una imagen JPEG en base64. Devuelve el texto de la respuesta.
pub fn call(a: &Agent, image_b64: Option<&str>, text: &str, timeout: u32) -> Result<String, String> {
    a.ready()?;
    let h = http(timeout);
    if a.provider.anthropic() {
        let mut content = vec![];
        if let Some(img) = image_b64 {
            content.push(json!({"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": img}}));
        }
        content.push(json!({"type": "text", "text": text}));
        let body = json!({"model": a.model.trim(), "max_tokens": 2000, "system": SYSTEM, "messages": [{"role": "user", "content": content}]});
        let req = h.post(&endpoint(a, "messages")).set("x-api-key", a.api_key.trim()).set("anthropic-version", "2023-06-01");
        let v = send(req, &body)?;
        let out: String = v.get("content").and_then(|c| c.as_array()).map(|c| c.iter().filter_map(|p| p.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        if out.trim().is_empty() {
            return Err(v.pointer("/error/message").map(text_of).unwrap_or_else(|| "respuesta vacía".into()));
        }
        return Ok(out);
    }
    // formato de OpenAI (OpenRouter, OpenAI, Gemini, Groq, Mistral, Ollama…)
    let mut content = vec![json!({"type": "text", "text": text})];
    if let Some(img) = image_b64 {
        let url = format!("data:image/jpeg;base64,{img}");
        content.push(if a.provider == Provider::Mistral { json!({"type": "image_url", "image_url": url}) } else { json!({"type": "image_url", "image_url": {"url": url}}) });
    }
    let body = json!({
        "model": a.model.trim(),
        "temperature": 0.2,
        "max_tokens": 2000,
        "messages": [{"role": "system", "content": SYSTEM}, {"role": "user", "content": content}],
    });
    let mut req = h.post(&endpoint(a, "chat/completions"));
    if !a.api_key.trim().is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", a.api_key.trim()));
    }
    if a.provider == Provider::OpenRouter {
        req = req.set("X-Title", "VeodarDental");
    }
    let v = send(req, &body)?;
    // OpenRouter a veces contesta 200 con un error dentro
    if let Some(e) = v.get("error") {
        let code = e.get("code").and_then(|c| c.as_u64()).unwrap_or(0) as u16;
        return Err(status_message(code, &json!({"error": e}).to_string()));
    }
    let out = v.pointer("/choices/0/message/content").map(|c| match c {
        Value::Array(parts) => parts.iter().filter_map(|p| p.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join("\n"),
        other => text_of(other),
    });
    match out {
        Some(t) if !t.trim().is_empty() => Ok(t),
        _ => Err("respuesta vacía".into()),
    }
}

/// Prueba rápida de un agente (sin imagen).
pub fn test_agent(a: &Agent, timeout: u32) -> Result<String, String> {
    let t0 = Instant::now();
    let r = call(a, None, "Responde únicamente con la palabra: OK", timeout.min(60))?;
    let r: String = r.trim().chars().take(40).collect();
    Ok(format!("responde «{r}» en {:.1} s", t0.elapsed().as_secs_f32()))
}

/// Analiza una imagen recorriendo los agentes en orden hasta que uno responda
/// bien. `paid_left`: análisis de pago que quedan este mes (con 0, los de pago
/// no se consultan).
pub fn analyze(cfg: &IaConfig, mode: Mode, req: &Request, paid_left: u32, progress: &dyn Fn(String)) -> Outcome {
    let all = chain(cfg, mode);
    let mut attempts = vec![];
    let skipped = all.iter().filter(|a| !a.free && paid_left == 0).count();
    let list: Vec<Agent> = all.into_iter().filter(|a| a.free || paid_left > 0).collect();
    if skipped > 0 {
        attempts.push(Attempt { agent: "Agentes de pago".into(), model: String::new(), free: false, ok: false, message: format!("no se consultan: se ha llegado al máximo de {} análisis de pago este mes", cfg.max_paid_month), millis: 0 });
    }
    if list.is_empty() && skipped > 0 {
        return Outcome { result: Err(format!("Se ha llegado al máximo de {} análisis de pago este mes (Configuración › Inteligencia artificial).", cfg.max_paid_month)), attempts };
    }
    if list.is_empty() {
        return Outcome { result: Err(format!("No hay agentes activos para «{}». Añádalos en Configuración › Inteligencia artificial.", mode.label())), attempts };
    }
    let img = b64(&req.jpeg);
    let text = prompt(req, &cfg.extra);
    for (i, a) in list.iter().enumerate() {
        progress(format!("Consultando {} ({} de {}): {}…", a.name, i + 1, list.len(), short_model(&a.model)));
        let t0 = Instant::now();
        let r = call(a, Some(&img), &text, cfg.timeout_secs).and_then(|t| parse_analysis(&t));
        let millis = t0.elapsed().as_millis() as u64;
        match r {
            Ok(an) => {
                attempts.push(Attempt { agent: a.name.clone(), model: a.model.clone(), free: a.free, ok: true, message: "respondió".into(), millis });
                return Outcome { result: Ok((a.clone(), an)), attempts };
            }
            Err(e) => attempts.push(Attempt { agent: a.name.clone(), model: a.model.clone(), free: a.free, ok: false, message: e, millis }),
        }
    }
    Outcome { result: Err("Ningún agente pudo analizar la imagen.".into()), attempts }
}

/// Modelo de la lista de un proveedor.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    /// Gratuito (sólo se sabe en OpenRouter).
    pub free: Option<bool>,
    /// Admite imágenes (sólo se sabe en OpenRouter).
    pub vision: Option<bool>,
}

/// Modelos disponibles en el proveedor de un agente.
pub fn list_models(a: &Agent) -> Result<Vec<ModelInfo>, String> {
    let h = http(30);
    let mut req = h.get(&endpoint(a, "models"));
    if a.provider.anthropic() {
        req = req.set("x-api-key", a.api_key.trim()).set("anthropic-version", "2023-06-01");
    } else if !a.api_key.trim().is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", a.api_key.trim()));
    }
    let v: Value = match req.call() {
        Ok(r) => serde_json::from_str(&r.into_string().map_err(|e| e.to_string())?).map_err(|_| "lista de modelos no válida".to_string())?,
        Err(ureq::Error::Status(code, r)) => return Err(status_message(code, &r.into_string().unwrap_or_default())),
        Err(e) => return Err(format!("no se pudo consultar la lista: {e}")),
    };
    let data = v.get("data").or_else(|| v.get("models")).and_then(|d| d.as_array()).cloned().unwrap_or_default();
    let mut out: Vec<ModelInfo> = data
        .iter()
        .filter_map(|m| {
            let id = m.get("id").or_else(|| m.get("name")).map(text_of)?;
            let id = id.trim_start_matches("models/").to_string();
            let name = m.get("name").or_else(|| m.get("display_name")).map(text_of).unwrap_or_else(|| id.clone());
            let price = |k: &str| m.pointer(&format!("/pricing/{k}")).map(text_of).and_then(|p| p.parse::<f64>().ok());
            let free = match (price("prompt"), price("completion")) {
                (Some(p), Some(c)) => Some(p == 0.0 && c == 0.0),
                _ if a.provider == Provider::Ollama => Some(true),
                _ if id.ends_with(":free") => Some(true),
                _ => None,
            };
            let vision = m.pointer("/architecture/input_modalities").and_then(|x| x.as_array()).map(|x| x.iter().any(|i| i.as_str() == Some("image")));
            Some(ModelInfo { id, name, free, vision })
        })
        .collect();
    out.sort_by(|x, y| y.free.unwrap_or(false).cmp(&x.free.unwrap_or(false)).then(x.id.cmp(&y.id)));
    Ok(out)
}

/// Tratamiento del catálogo que corresponde a una propuesta de la IA.
pub fn proposal_kind(kind: &str) -> Option<crate::model::OdgKind> {
    use crate::model::OdgKind::*;
    Some(match kind {
        k if k.starts_with("obtura") || k.contains("empaste") => Obturacion,
        k if k.starts_with("endodon") => Endodoncia,
        k if k.starts_with("extrac") || k.contains("exodon") => Extraccion,
        k if k.starts_with("corona") => Corona,
        k if k.starts_with("implant") => Implante,
        k if k.starts_with("puente") => Puente,
        k if k.contains("sellad") => Sellador,
        _ => return None,
    })
}

/// Palabras para buscar en el catálogo las propuestas sin herramienta del odontograma.
pub fn proposal_words(kind: &str) -> &'static [&'static str] {
    match kind {
        k if k.starts_with("raspado") || k.contains("curetaje") => &["raspado", "curetaje"],
        k if k.starts_with("limpieza") || k.contains("tartrec") || k.contains("profilax") => &["limpieza", "tartrectom", "profilaxis", "higiene"],
        k if k.starts_with("revision") => &["revisión", "revision", "control"],
        _ => &[],
    }
}

/// Nombre legible de un tipo de hallazgo.
pub fn finding_label(kind: &str) -> String {
    match kind {
        "caries" => "Caries",
        "lesion_periapical" => "Lesión periapical",
        "perdida_osea" => "Pérdida ósea",
        "resto_radicular" => "Resto radicular",
        "fractura" => "Fractura",
        "obturacion" => "Obturación",
        "endodoncia" => "Endodoncia",
        "corona" => "Corona",
        "implante" => "Implante",
        "ausencia" => "Ausencia",
        "calculo" => "Cálculo / sarro",
        "" => "Hallazgo",
        other => return other.replace('_', " "),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_known_values() {
        assert_eq!(b64(b""), "");
        assert_eq!(b64(b"f"), "Zg==");
        assert_eq!(b64(b"fo"), "Zm8=");
        assert_eq!(b64(b"foo"), "Zm9v");
        assert_eq!(b64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn reads_loose_answers() {
        let txt = "Claro, aquí está:\n```json\n{\"resumen\": \"Caries distal en 36.\", \"calidad\": \"buena\", \"hallazgos\": [{\"pieza\": 36, \"tipo\": \"Caries\", \"caras\": \"D, O\", \"descripcion\": \"radiolucidez\", \"gravedad\": \"Media\", \"confianza\": \"80%\"}, {\"pieza\": \"pieza 99\", \"tipo\": \"otro\"}], \"propuesta\": [{\"tipo\": \"Obturación\", \"piezas\": \"36\", \"caras\": [\"distal\", \"oclusal\"], \"motivo\": \"caries\"}]}\n```";
        let a = parse_analysis(txt).unwrap();
        assert_eq!(a.summary, "Caries distal en 36.");
        assert_eq!(a.findings[0].tooth, Some(36));
        assert_eq!(a.findings[0].kind, "caries");
        assert_eq!(a.findings[0].faces, vec!["D", "O"]);
        assert!((a.findings[0].confidence - 0.8).abs() < 1e-4);
        assert_eq!(a.findings[1].tooth, None, "99 no es una pieza");
        assert_eq!(a.proposals[0].teeth, vec![36]);
        assert_eq!(a.proposals[0].faces, vec!["D", "O"]);
        assert_eq!(proposal_kind(&a.proposals[0].kind), Some(crate::model::OdgKind::Obturacion));
        assert!(parse_analysis("No puedo ver la imagen").is_err());
    }

    #[test]
    fn chain_respects_mode_and_order() {
        let cfg = IaConfig {
            agents: vec![Agent::new(1, Provider::OpenRouter, "a:free", true), Agent::new(2, Provider::OpenAi, "b", false), Agent::new(3, Provider::Gemini, "c", true), Agent { enabled: false, ..Agent::new(4, Provider::Groq, "d", true) }],
            ..Default::default()
        };
        let ids = |m| chain(&cfg, m).iter().map(|a| a.id).collect::<Vec<_>>();
        assert_eq!(ids(Mode::FreeOnly), vec![1, 3]);
        assert_eq!(ids(Mode::PaidOnly), vec![2]);
        assert_eq!(ids(Mode::FreeThenPaid), vec![1, 3, 2]);
    }

    /// Servicio de prueba: el modelo «saturado» contesta 429, «roto» un texto
    /// sin JSON y «bueno» un análisis válido. Cuenta las llamadas por modelo.
    fn fake_service(n: usize) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let h = std::thread::spawn(move || {
            let mut seen = vec![];
            for _ in 0..n {
                let Ok(mut rq) = server.recv() else { break };
                let mut body = String::new();
                rq.as_reader().read_to_string(&mut body).unwrap();
                let v: Value = serde_json::from_str(&body).unwrap();
                let model = v["model"].as_str().unwrap_or("").to_string();
                let anthropic = rq.url().ends_with("/messages");
                let has_image = body.contains("data:image/jpeg;base64,") || body.contains("\"media_type\":\"image/jpeg\"");
                let auth_ok = rq.headers().iter().any(|h| (h.field.equiv("Authorization") && h.value.as_str() == "Bearer clave") || (h.field.equiv("x-api-key") && h.value.as_str() == "clave"));
                seen.push(model.clone());
                let good = r#"{"resumen":"Sin hallazgos graves.","calidad":"buena","hallazgos":[{"pieza":"46","tipo":"caries","caras":["O"],"descripcion":"incipiente","gravedad":"baja","confianza":0.6}],"propuesta":[{"tipo":"obturacion","piezas":["46"],"caras":["O"],"motivo":"caries"}]}"#;
                let (code, resp) = match model.as_str() {
                    _ if !auth_ok => (401, json!({"error": {"message": "invalid key"}}).to_string()),
                    _ if !has_image => (400, json!({"error": {"message": "no image"}}).to_string()),
                    "saturado" => (429, json!({"error": {"message": "Rate limit exceeded: free-models-per-day"}}).to_string()),
                    "roto" => (200, json!({"choices": [{"message": {"content": "Lo siento, no puedo."}}]}).to_string()),
                    _ if anthropic => (200, json!({"content": [{"type": "text", "text": good}]}).to_string()),
                    _ => (200, json!({"choices": [{"message": {"content": good}}]}).to_string()),
                };
                rq.respond(tiny_http::Response::from_string(resp).with_status_code(code)).unwrap();
            }
            seen
        });
        (format!("http://127.0.0.1:{port}/v1"), h)
    }

    fn agent(id: u32, p: Provider, url: &str, model: &str, free: bool) -> Agent {
        Agent { base_url: url.into(), api_key: "clave".into(), ..Agent::new(id, p, model, free) }
    }

    fn request() -> Request {
        let img = image::RgbImage::from_pixel(64, 48, image::Rgb([120, 120, 120]));
        Request { jpeg: jpeg_for_ai(&img).unwrap(), kind: "Radiografía periapical".into(), teeth: vec![46], age: Some(40) }
    }

    #[test]
    fn falls_back_to_next_free_agent_and_never_pays() {
        let (url, h) = fake_service(3);
        let cfg = IaConfig {
            agents: vec![agent(1, Provider::OpenRouter, &url, "saturado", true), agent(2, Provider::OpenRouter, &url, "roto", true), agent(3, Provider::Custom, &url, "bueno", true), agent(4, Provider::OpenAi, &url, "caro", false)],
            ..Default::default()
        };
        let out = analyze(&cfg, Mode::FreeThenPaid, &request(), 10, &|_| {});
        let (a, an) = out.result.unwrap();
        assert_eq!(a.id, 3, "responde el tercer agente gratuito");
        assert_eq!(an.proposals[0].teeth, vec![46]);
        assert_eq!(out.attempts.len(), 3);
        assert!(out.attempts[0].message.contains("límite de uso"), "{}", out.attempts[0].message);
        assert!(!out.attempts[1].ok);
        assert_eq!(h.join().unwrap(), vec!["saturado", "roto", "bueno"], "el de pago no se llega a usar");
    }

    #[test]
    fn paid_agents_answer_when_free_ones_fail() {
        let (url, h) = fake_service(3);
        let cfg = IaConfig { agents: vec![agent(1, Provider::OpenRouter, &url, "saturado", true), agent(2, Provider::Anthropic, &url, "claude", false)], ..Default::default() };
        // sólo gratuitos: falla y no se toca el de pago
        let out = analyze(&cfg, Mode::FreeOnly, &request(), 10, &|_| {});
        assert!(out.result.is_err());
        assert_eq!(out.attempts.len(), 1);
        // gratuitos y luego de pago: responde el de pago (formato de Anthropic)
        let out = analyze(&cfg, Mode::FreeThenPaid, &request(), 10, &|_| {});
        let (a, an) = out.result.unwrap();
        assert_eq!(a.id, 2);
        assert_eq!(an.findings[0].tooth, Some(46));
        assert_eq!(h.join().unwrap(), vec!["saturado", "saturado", "claude"]);
    }

    /// Lista real de OpenRouter (pública, sin clave). Se ejecuta a mano: `cargo test -- --ignored openrouter`.
    #[test]
    #[ignore]
    fn openrouter_live_model_list() {
        let a = Agent::new(1, Provider::OpenRouter, "", true);
        let list = list_models(&a).expect("lista de OpenRouter");
        let free_vision: Vec<&ModelInfo> = list.iter().filter(|m| m.free == Some(true) && m.vision == Some(true)).collect();
        for m in &free_vision {
            println!("GRATIS+VISION {}", m.id);
        }
        for (m, _) in Provider::OpenRouter.suggested() {
            println!("SUGERIDO {m}: {}", if list.iter().any(|x| x.id == *m) { "existe" } else { "NO EXISTE" });
        }
        assert!(!free_vision.is_empty());
    }

    /// Sin análisis de pago que queden este mes, los agentes de pago no se
    /// consultan (los gratuitos sí), y queda dicho en los intentos.
    #[test]
    fn paid_agents_are_not_called_past_the_monthly_limit() {
        let (url, h) = fake_service(1);
        let mut cfg = IaConfig { enabled: true, accepted: true, agents: vec![agent(1, Provider::Custom, &url, "de-pago", false)], max_paid_month: 3, ..Default::default() };
        let out = analyze(&cfg, Mode::FreeThenPaid, &request(), 0, &|_| {});
        assert!(out.result.unwrap_err().contains("máximo de 3"));
        assert!(out.attempts.iter().any(|a| a.message.contains("no se consultan")));
        // con margen, sí
        let out = analyze(&cfg, Mode::FreeThenPaid, &request(), 1, &|_| {});
        assert!(out.result.is_ok());
        assert_eq!(h.join().unwrap().len(), 1, "una sola llamada, la permitida");
        cfg.max_paid_month = 0;
        assert!(analyze(&cfg, Mode::PaidOnly, &request(), 0, &|_| {}).result.is_err());
    }

    /// Un análisis es un borrador hasta que lo revisa un profesional
    /// colegiado: antes no pasa al plan. Las propuestas pasan solo si se
    /// eligen, y cada tratamiento dice de dónde viene.
    #[test]
    fn an_analysis_is_a_draft_until_a_professional_reviews_it() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        let mut db = crate::model::Db::sample(day);
        let r = db.ia_reports[0].clone();
        assert!(r.analysis.proposals.len() >= 2 && r.review.is_none());
        assert!(db.plan_from_ia(r.id, &[0], day).unwrap_err().contains("borrador"));
        // recepción no lo revisa
        db.config.current_user = db.users.iter().find(|u| u.doctor_id.is_none()).unwrap().name.clone();
        assert!(db.review_ia_report(r.id, true).is_err());
        let dentist = db.users.iter().find(|u| u.doctor_id.and_then(|d| db.doctor(d)).is_some_and(|c| crate::finance::collaborator_problem(c).is_none())).unwrap().name.clone();
        db.config.current_user = dentist.clone();
        db.review_ia_report(r.id, true).unwrap();
        assert!(db.review_ia_report(r.id, false).is_err(), "una vez revisado, no se cambia");
        let n = db.entries.len();
        let idx = r.analysis.proposals.iter().position(|p| db.ia_treatment(p).is_some()).expect("una propuesta con tratamiento");
        assert_eq!(db.plan_from_ia(r.id, &[idx], day).unwrap(), 1, "solo la elegida");
        assert_eq!(db.entries.len(), n + 1);
        let e = db.entries.last().unwrap();
        assert!(e.notes.contains("Propuesto por IA") && e.notes.contains(&dentist), "{}", e.notes);
        assert_eq!(db.plan_from_ia(r.id, &[idx], day).unwrap(), 0, "no se añade dos veces");
        assert!(db.audit_log.iter().any(|a| a.action == "Análisis con IA revisado"));
        // propuestas de piezas que no salen en la imagen, señaladas
        let p = Proposal { kind: "caries".into(), teeth: vec![48], faces: vec![], reason: String::new() };
        assert!(outside_image(&[11, 12], &p) && !outside_image(&[], &p) && !outside_image(&[47, 48], &p));
    }

    #[test]
    fn agent_without_key_is_skipped_without_calling() {
        let cfg = IaConfig { agents: vec![Agent { base_url: "http://127.0.0.1:9/v1".into(), ..Agent::new(1, Provider::OpenRouter, "x:free", true) }], ..Default::default() };
        let out = analyze(&cfg, Mode::FreeOnly, &request(), 10, &|_| {});
        assert!(out.result.is_err());
        assert_eq!(out.attempts[0].message, "falta la clave");
    }
}
