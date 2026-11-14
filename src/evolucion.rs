//! Notas de evolución (T-F5-04): plantillas por tratamiento, dictado con
//! transcripción y resumen opcional con IA. **Nada entra en la historia sin
//! que el profesional lo revise y lo confirme.**
//!
//! - **Procedencia:** cada nota dice de dónde sale su texto (escrito,
//!   plantilla, dictado con qué servicio, resumen con qué modelo) y quién lo
//!   confirmó y cuándo.
//! - **Sin hechos inventados:** antes de confirmar un resumen de la IA, se
//!   señalan los datos que no aparecen en el dictado (piezas, cifras, dosis,
//!   nombres de medicamentos) y hay que revisarlos ([`unsupported`]).
//! - **Mínimo de datos:**
//!   - el audio no se guarda; se lee para transcribirlo y se descarta;
//!   - del dictado solo queda su huella (SHA-256) como procedencia;
//!   - a la IA no va el nombre del paciente.
//! - **Confirmada, no se cambia:** se corrige con otra nota que la cita. El
//!   servidor no deja cambiar ni borrar una nota confirmada
//!   (`firmado::protect_json`).
//! - **Transcripción:** servicio compatible con la API de OpenAI
//!   (`/v1/audio/transcriptions`): el de la clínica o uno en el propio
//!   ordenador. Se configura aparte (`stt.json`) y, sin configurar, no hay
//!   dictado. El proveedor real está pendiente de elegir (B-15).

use crate::model::*;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    Escrito,
    Plantilla,
    Dictado,
    ResumenIa,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Origin::Escrito => "Escrito",
            Origin::Plantilla => "Plantilla",
            Origin::Dictado => "Dictado",
            Origin::ResumenIa => "Resumen con IA",
        }
    }
}

/// De dónde sale (parte de) el texto de una nota.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub origin: Origin,
    /// Plantilla, servicio de transcripción o modelo de IA.
    #[serde(default)]
    pub detail: String,
    pub at: NaiveDateTime,
    /// Huella SHA-256 del texto de origen (el dictado), que no se guarda.
    #[serde(default)]
    pub source_hash: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvoNote {
    pub id: u32,
    pub patient_id: u32,
    pub doctor_id: u32,
    /// Tratamiento al que se refiere.
    #[serde(default)]
    pub entry_id: Option<u32>,
    pub created: NaiveDateTime,
    pub text: String,
    #[serde(default)]
    pub provenance: Vec<Provenance>,
    /// Quién la confirmó y cuándo (sin confirmar es un borrador).
    #[serde(default)]
    pub confirmed: Option<(NaiveDateTime, String)>,
    /// Huella del texto confirmado.
    #[serde(default)]
    pub hash: String,
    /// Nota que corrige.
    #[serde(default)]
    pub corrects: Option<u32>,
}

impl EvoNote {
    pub fn is_confirmed(&self) -> bool {
        self.confirmed.is_some()
    }

    pub fn uses_ai(&self) -> bool {
        self.provenance.iter().any(|p| p.origin == Origin::ResumenIa)
    }
}

fn sha(text: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

// ───────────────────────────── plantillas ─────────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvoTemplate {
    pub id: u32,
    pub name: String,
    /// Tratamientos para los que se propone.
    #[serde(default)]
    pub kinds: Vec<OdgKind>,
    pub body: String,
}

pub fn default_templates() -> Vec<EvoTemplate> {
    let t = |id, name: &str, kinds: Vec<OdgKind>, body: &str| EvoTemplate { id, name: name.into(), kinds, body: body.into() };
    vec![
        t(1, "Obturación", vec![OdgKind::Obturacion], "Pieza [PIEZAS]. Anestesia: ___. Aislamiento: ___. Caries ___ eliminada. Obturación con ___, caras ___. Oclusión comprobada. Indicaciones: ___."),
        t(
            2,
            "Endodoncia",
            vec![OdgKind::Endodoncia],
            "Pieza [PIEZAS]. Anestesia: ___. Aislamiento absoluto. Conductos: ___. Longitud de trabajo: ___. Irrigación: ___. Obturación de conductos con ___. Restauración provisional: ___. Próxima cita: ___.",
        ),
        t(3, "Extracción", vec![OdgKind::Extraccion], "Pieza [PIEZAS]. Consentimiento firmado. Anestesia: ___. Extracción ___ sin incidencias / con: ___. Hemostasia: ___. Indicaciones postoperatorias entregadas. Medicación: ___."),
        t(4, "Implante", vec![OdgKind::Implante], "Zona [PIEZAS]. Consentimiento firmado. Anestesia: ___. Implante ___ (diámetro ___, longitud ___, lote ___). Torque de inserción: ___. Sutura: ___. Indicaciones y medicación: ___."),
        t(5, "Higiene", vec![OdgKind::Higiene], "Tartrectomía supra y subgingival. Pulido. Índice de placa: ___. Sangrado al sondaje: ___. Instrucciones de higiene: ___. Próxima revisión: ___."),
        t(6, "Revisión", vec![], "Motivo: ___. Exploración: ___. Diagnóstico: ___. Plan: ___."),
    ]
}

/// Texto de la plantilla con las piezas del tratamiento.
pub fn fill_template(t: &EvoTemplate, teeth: &[u8]) -> String {
    let piezas = if teeth.is_empty() { "___".to_string() } else { teeth.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("-") };
    t.body.replace("[PIEZAS]", &piezas)
}

// ───────────────────────────── transcripción ─────────────────────────────

/// Servicio de transcripción de este ordenador (sin configurar: sin dictado).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SttSettings {
    /// Dirección base (por ejemplo `http://127.0.0.1:8080` o la del proveedor).
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub key: String,
}

impl SttSettings {
    pub fn enabled(&self) -> bool {
        !self.endpoint.trim().is_empty()
    }
    /// Nombre para la procedencia (sin la clave).
    pub fn label(&self) -> String {
        let host = self.endpoint.trim().trim_start_matches("https://").trim_start_matches("http://").split('/').next().unwrap_or("").to_string();
        if self.model.trim().is_empty() { host } else { format!("{host} · {}", self.model.trim()) }
    }
}

fn stt_path() -> std::path::PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("stt.json")
}

pub fn load_stt() -> SttSettings {
    std::fs::read_to_string(stt_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_stt(s: &SttSettings) -> Result<(), String> {
    let p = stt_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    crate::paths::write_private(&p, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())
}

/// Transcribe una grabación (`multipart/form-data`, como la API de OpenAI).
/// El audio solo está en memoria mientras dura.
pub fn transcribe(s: &SttSettings, audio: &[u8], file_name: &str) -> Result<String, String> {
    if !s.enabled() {
        return Err("No hay servicio de transcripción configurado.".into());
    }
    if audio.is_empty() || audio.len() > 25 << 20 {
        return Err("La grabación está vacía o pasa de 25 MB.".into());
    }
    let boundary = format!("----veodar{}", sha(&format!("{}{}", audio.len(), file_name))[..16].to_owned());
    let mut body: Vec<u8> = vec![];
    let field = |body: &mut Vec<u8>, name: &str, value: &str| body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    if !s.model.trim().is_empty() {
        field(&mut body, "model", s.model.trim());
    }
    field(&mut body, "language", "es");
    field(&mut body, "response_format", "json");
    let safe: String = file_name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.').collect();
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n", if safe.is_empty() { "audio.wav" } else { &safe }).as_bytes());
    body.extend_from_slice(audio);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let url = format!("{}/v1/audio/transcriptions", s.endpoint.trim().trim_end_matches('/').trim_end_matches("/v1"));
    let mut req = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(300)).build().post(&url).set("Content-Type", &format!("multipart/form-data; boundary={boundary}"));
    if !s.key.trim().is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", s.key.trim()));
    }
    let text = match req.send_bytes(&body) {
        Ok(r) => r.into_string().map_err(|e| format!("Respuesta no válida del servicio: {e}"))?,
        Err(ureq::Error::Status(code, r)) => return Err(format!("El servicio de transcripción respondió {code}: {}", r.into_string().unwrap_or_default().chars().take(200).collect::<String>())),
        Err(e) => return Err(format!("Sin conexión con el servicio de transcripción: {e}")),
    };
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|_| "Respuesta no válida del servicio de transcripción.".to_string())?;
    let t = v["text"].as_str().unwrap_or_default().trim().to_string();
    if t.is_empty() {
        return Err("La transcripción ha salido vacía.".into());
    }
    Ok(t)
}

// ───────────────────────────── resumen con IA ─────────────────────────────

/// Instrucciones para resumir un dictado sin añadir nada.
pub fn summary_prompt(transcript: &str) -> String {
    format!(
        "Eres un asistente de un dentista. Reescribe el dictado siguiente como una nota de evolución clínica breve y ordenada en español. \
         Usa SOLO la información del dictado: no añadas diagnósticos, piezas, cifras, dosis, medicamentos ni recomendaciones que no estén en él. \
         Si algo no se entiende, escribe «[no se entiende]». Devuelve solo el texto de la nota.\n\nDictado:\n{transcript}"
    )
}

/// El dictado sin el nombre del paciente (para mandarlo a la IA).
pub fn without_names(text: &str, p: &Patient) -> String {
    let names: Vec<String> = [&p.first_name, &p.last_name].iter().flat_map(|s| s.split_whitespace()).filter(|w| w.chars().count() >= 3).map(crate::model::fold).collect();
    text.split(' ').map(|w| if names.contains(&crate::model::fold(w.trim_matches(|c: char| !c.is_alphanumeric()))) { "[paciente]".to_string() } else { w.to_string() }).collect::<Vec<_>>().join(" ")
}

/// Datos de `summary` que no están en `source`: cifras (piezas, dosis,
/// medidas) y medicamentos del vademécum. Hay que revisarlos antes de confirmar.
pub fn unsupported(summary: &str, source: &str, drugs: &[String]) -> Vec<String> {
    let src = crate::model::fold(source);
    let numbers = |t: &str| -> Vec<String> {
        let mut v = vec![];
        let mut cur = String::new();
        for c in t.chars() {
            if c.is_ascii_digit() || (!cur.is_empty() && (c == ',' || c == '.')) {
                cur.push(c);
            } else if !cur.is_empty() {
                v.push(cur.trim_end_matches([',', '.']).to_string());
                cur.clear();
            }
        }
        if !cur.is_empty() {
            v.push(cur.trim_end_matches([',', '.']).to_string());
        }
        v
    };
    let src_numbers = numbers(&src);
    let mut out: Vec<String> = vec![];
    for n in numbers(&crate::model::fold(summary)) {
        if !src_numbers.contains(&n) && !out.contains(&n) {
            out.push(n);
        }
    }
    let sum = crate::model::fold(summary);
    for d in drugs {
        let f = crate::model::fold(d);
        if f.chars().count() >= 4 && sum.contains(&f) && !src.contains(&f) && !out.contains(d) {
            out.push(d.clone());
        }
    }
    out
}

// ───────────────────────────── las notas ─────────────────────────────

impl Db {
    /// Borrador de nota (no está en la historia hasta confirmarlo).
    pub fn evo_draft(&mut self, pid: u32, doctor: u32, entry: Option<u32>, text: &str, provenance: Vec<Provenance>, now: NaiveDateTime) -> u32 {
        let id = self.new_id();
        self.evo_notes.push(EvoNote { id, patient_id: pid, doctor_id: doctor, entry_id: entry, created: now, text: text.into(), provenance, confirmed: None, hash: String::new(), corrects: None });
        id
    }

    /// Cambia un borrador (una nota confirmada no se cambia: se corrige con otra).
    pub fn evo_edit(&mut self, id: u32, text: &str, add: Option<Provenance>) -> Result<(), String> {
        let n = self.evo_notes.iter_mut().find(|n| n.id == id).ok_or("La nota ya no existe.")?;
        if n.is_confirmed() {
            return Err("La nota ya está confirmada: para cambiarla, escriba una corrección.".into());
        }
        n.text = text.to_string();
        if let Some(p) = add {
            n.provenance.push(p);
        }
        Ok(())
    }

    /// Confirma una nota: entra en la historia. Si tiene un resumen de la IA
    /// con datos que no están en el dictado (`flagged`), hay que haberlos
    /// revisado (`reviewed`).
    pub fn evo_confirm(&mut self, id: u32, who: &str, flagged: &[String], reviewed: bool, now: NaiveDateTime) -> Result<(), String> {
        let n = self.evo_notes.iter_mut().find(|n| n.id == id).ok_or("La nota ya no existe.")?;
        if n.is_confirmed() {
            return Err("La nota ya está confirmada.".into());
        }
        if n.text.trim().is_empty() || n.text.contains("___") {
            return Err("Complete la nota (quedan huecos «___» de la plantilla) antes de confirmarla.".into());
        }
        if n.uses_ai() && !flagged.is_empty() && !reviewed {
            return Err(format!("El resumen de la IA menciona datos que no están en el dictado ({}): revíselos antes de confirmar.", flagged.join(", ")));
        }
        if (n.uses_ai() || n.provenance.iter().any(|p| p.origin == Origin::Dictado)) && !reviewed {
            return Err("Revise el texto del dictado o de la IA y marque que lo ha revisado.".into());
        }
        n.confirmed = Some((now, who.to_string()));
        n.hash = sha(&n.text);
        let (pid, nid) = (n.patient_id, n.id);
        self.audit("Confirmar nota de evolución", &format!("{} · nota #{nid}", self.patient_name(pid)));
        Ok(())
    }

    /// Borra un borrador (nunca una nota confirmada).
    pub fn evo_discard(&mut self, id: u32) -> Result<(), String> {
        if self.evo_notes.iter().any(|n| n.id == id && n.is_confirmed()) {
            return Err("Una nota confirmada no se borra: se corrige con otra.".into());
        }
        self.evo_notes.retain(|n| n.id != id);
        Ok(())
    }

    /// Corrección de una nota confirmada: un borrador nuevo que la cita.
    pub fn evo_correct(&mut self, id: u32, now: NaiveDateTime) -> Result<u32, String> {
        let n = self.evo_notes.iter().find(|n| n.id == id).cloned().ok_or("La nota ya no existe.")?;
        if !n.is_confirmed() {
            return Err("Un borrador se cambia directamente.".into());
        }
        let nid = self.evo_draft(n.patient_id, n.doctor_id, n.entry_id, "", vec![Provenance { origin: Origin::Escrito, detail: format!("Corrección de la nota #{id}"), at: now, source_hash: String::new() }], now);
        if let Some(x) = self.evo_notes.iter_mut().find(|x| x.id == nid) {
            x.corrects = Some(id);
        }
        Ok(nid)
    }
}

/// Servidor: una nota confirmada no cambia ni se borra desde un puesto.
pub fn keep_confirmed(cur: &EvoNote, _after: Option<&EvoNote>) -> EvoNote {
    cur.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 14).unwrap().and_hms_opt(10, 0, 0).unwrap()
    }

    fn prov(origin: Origin) -> Provenance {
        Provenance { origin, detail: "prueba".into(), at: now(), source_hash: String::new() }
    }

    #[test]
    fn nothing_enters_the_history_without_review_and_confirmation() {
        let mut db = Db::sample(now().date());
        let t = &default_templates()[1];
        let text = fill_template(t, &[46]);
        assert!(text.contains("Pieza 46") && text.contains("___"));
        let id = db.evo_draft(101, 1, None, &text, vec![prov(Origin::Plantilla)], now());
        assert!(db.evo_confirm(id, "Dra. Ruiz", &[], true, now()).unwrap_err().contains("___"), "la plantilla sin completar no se confirma");
        db.evo_edit(id, "Pieza 46. Anestesia articaína. Conductos MV, ML y D. Obturación con gutapercha.", None).unwrap();
        db.evo_confirm(id, "Dra. Ruiz", &[], false, now()).unwrap();
        let n = db.evo_notes.iter().find(|n| n.id == id).unwrap();
        assert!(n.is_confirmed() && n.hash == sha(&n.text));
        // confirmada: ni se cambia ni se borra; se corrige con otra que la cita
        assert!(db.evo_edit(id, "otro texto", None).is_err());
        assert!(db.evo_discard(id).is_err());
        let c = db.evo_correct(id, now()).unwrap();
        assert_eq!(db.evo_notes.iter().find(|n| n.id == c).unwrap().corrects, Some(id));
        // dictado: hay que marcar que se ha revisado
        let d = db.evo_draft(101, 1, None, "Pieza 36 obturada con composite.", vec![prov(Origin::Dictado)], now());
        assert!(db.evo_confirm(d, "Dra. Ruiz", &[], false, now()).unwrap_err().contains("Revise"));
        db.evo_confirm(d, "Dra. Ruiz", &[], true, now()).unwrap();
    }

    #[test]
    fn an_ai_summary_with_facts_not_in_the_dictation_is_flagged() {
        let dictation = "obturación de la 36 con composite, anestesia con articaína, sin incidencias";
        let drugs = vec!["Amoxicilina".to_string(), "Articaína".to_string(), "Ibuprofeno".to_string()];
        assert!(unsupported("Obturación de la pieza 36 con composite. Anestesia con articaína. Sin incidencias.", dictation, &drugs).is_empty());
        let bad = unsupported("Obturación de la 36 y la 37 con composite. Articaína. Se pauta ibuprofeno 600 mg.", dictation, &drugs);
        assert_eq!(bad, vec!["37".to_string(), "600".to_string(), "Ibuprofeno".to_string()]);
        let mut db = Db::sample(now().date());
        let id = db.evo_draft(101, 1, None, "Obturación de la 36 y la 37.", vec![prov(Origin::Dictado), prov(Origin::ResumenIa)], now());
        assert!(db.evo_confirm(id, "Dra. Ruiz", &bad, false, now()).unwrap_err().contains("no están en el dictado"));
        db.evo_confirm(id, "Dra. Ruiz", &bad, true, now()).unwrap();
        // a la IA no va el nombre del paciente
        let p = db.patient(101).unwrap().clone();
        let t = without_names(&format!("{} viene por dolor en la 36", p.first_name), &p);
        assert!(t.starts_with("[paciente]") && !t.contains(&p.first_name));
        assert!(summary_prompt("x").contains("SOLO la información del dictado"));
    }

    /// Doble de prueba del servicio de transcripción: comprueba lo que recibe.
    #[test]
    fn dictation_is_transcribed_by_the_configured_service() {
        let srv = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", srv.server_addr().to_ip().unwrap());
        let h = std::thread::spawn(move || {
            let mut req = srv.recv_timeout(std::time::Duration::from_secs(10)).unwrap().unwrap();
            let mut body = vec![];
            req.as_reader().read_to_end(&mut body).unwrap();
            let ct = req.headers().iter().find(|h| h.field.equiv("Content-Type")).map(|h| h.value.to_string()).unwrap_or_default();
            let auth = req.headers().iter().find(|h| h.field.equiv("Authorization")).map(|h| h.value.to_string()).unwrap_or_default();
            let path = req.url().to_string();
            let _ = req.respond(tiny_http::Response::from_string(r#"{"text": " Obturación de la 36 con composite. "}"#));
            (path, ct, auth, String::from_utf8_lossy(&body).to_string())
        });
        let s = SttSettings { endpoint: format!("{url}/v1"), model: "whisper-1".into(), key: "clave".into() };
        let audio = b"RIFF....WAVEfmt ficticio";
        assert_eq!(transcribe(&s, audio, "dictado 1.wav").unwrap(), "Obturación de la 36 con composite.");
        let (path, ct, auth, body) = h.join().unwrap();
        assert_eq!(path, "/v1/audio/transcriptions");
        assert!(ct.starts_with("multipart/form-data; boundary="));
        assert_eq!(auth, "Bearer clave");
        assert!(body.contains("name=\"model\"\r\n\r\nwhisper-1") && body.contains("filename=\"dictado1.wav\"") && body.contains("WAVEfmt ficticio"));
        assert!(transcribe(&SttSettings::default(), audio, "a.wav").is_err());
        assert!(transcribe(&SttSettings { endpoint: "http://127.0.0.1:9".into(), ..Default::default() }, audio, "a.wav").unwrap_err().contains("Sin conexión"));
        assert_eq!(s.label(), "127.0.0.1:".to_string() + url.rsplit(':').next().unwrap() + " · whisper-1");
    }
}
