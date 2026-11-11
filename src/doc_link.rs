//! Documentos para firmar por internet (consentimientos informados,
//! protección de datos…): cada documento puede tener un enlace secreto con el
//! que el paciente lo lee con calma en el móvil, pregunta sus dudas y lo firma
//! con el dedo, o indica que prefiere firmarlo en la clínica.
//!
//! Como en los presupuestos, es una firma electrónica simple: el trazo, el
//! nombre y el NIF que escribe, la fecha y hora, la dirección de conexión y la
//! huella (SHA-256) del texto que se le mostró. Si la clínica cambia el texto
//! mientras el paciente lo tiene abierto, la huella no coincide y se le pide
//! que lo vuelva a abrir.

use crate::budget_link::{Strokes, clean_strokes, signature_svg};
use crate::comms::PatientDoc;
use crate::model::*;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocLink {
    pub token: String,
    pub doc_id: u32,
    pub created: Option<NaiveDateTime>,
    /// A quién se envió por correo.
    pub sent_to: String,
    /// Primera vez que el paciente lo abrió.
    pub viewed: Option<NaiveDateTime>,
    /// Cuándo lo firmó por internet.
    pub signed: Option<NaiveDateTime>,
    pub signer_nif: String,
    pub ip: String,
    /// Huella del texto firmado.
    pub doc_hash: String,
    pub questions: Vec<(NaiveDateTime, String)>,
    /// Prefiere firmarlo en la clínica (y por qué).
    pub declined: Option<NaiveDateTime>,
    pub decline_reason: String,
}

/// Texto que identifica el documento (lo que el paciente lee y firma).
pub fn doc_text(d: &PatientDoc) -> String {
    format!("Documento {} · paciente {} · {} · {}\n{}\n{}", d.id, d.patient_id, fmt_date(d.date), d.category, d.title, d.body)
}

pub fn doc_hash(d: &PatientDoc) -> String {
    Sha256::digest(doc_text(d).as_bytes()).iter().map(|x| format!("{x:02x}")).collect()
}

pub fn link_url(base: &str, token: &str) -> String {
    format!("{}/documento/{token}", base.trim().trim_end_matches('/'))
}

impl Db {
    pub fn doc_link(&self, doc_id: u32) -> Option<&DocLink> {
        self.doc_links.iter().find(|l| l.doc_id == doc_id)
    }

    pub fn doc_token(&mut self, doc_id: u32) -> String {
        if let Some(l) = self.doc_link(doc_id) {
            return l.token.clone();
        }
        let token = crate::portal::new_token();
        self.doc_links.push(DocLink { token: token.clone(), doc_id, created: Some(now()), ..Default::default() });
        token
    }

    fn dlink_mut(&mut self, doc_id: u32) -> Option<&mut DocLink> {
        self.doc_links.iter_mut().find(|l| l.doc_id == doc_id)
    }

    /// El paciente abrió el enlace por primera vez.
    pub fn doc_viewed(&mut self, doc_id: u32) -> bool {
        match self.dlink_mut(doc_id) {
            Some(l) if l.viewed.is_none() => {
                l.viewed = Some(now());
                true
            }
            _ => false,
        }
    }

    /// Firma desde el enlace: comprueba la huella y la firma, y firma el documento.
    pub fn doc_remote_sign(&mut self, doc_id: u32, signer: &str, nif: &str, strokes: Strokes, hash: &str, ip: &str) -> Result<(), String> {
        let d = self.documents.iter().find(|d| d.id == doc_id).cloned().ok_or("El documento ya no existe.")?;
        if d.signed {
            return Err("El documento ya está firmado.".into());
        }
        if doc_hash(&d) != hash {
            return Err("El documento ha cambiado mientras lo leía: vuelva a abrirlo, por favor.".into());
        }
        let strokes = clean_strokes(&strokes)?;
        let signer = signer.trim();
        if !(3..=80).contains(&signer.chars().count()) {
            return Err("Falta el nombre de quien firma.".into());
        }
        self.sign_document(doc_id, signer, strokes)?;
        let when = self.documents.iter().find(|d| d.id == doc_id).and_then(|d| d.signed_at);
        if let Some(l) = self.dlink_mut(doc_id) {
            l.signed = when.or(Some(now()));
            l.signer_nif = nif.trim().to_string();
            l.ip = ip.to_string();
            l.doc_hash = hash.to_string();
            l.declined = None;
        }
        Ok(())
    }

    /// El paciente prefiere firmarlo en la clínica.
    pub fn doc_remote_decline(&mut self, doc_id: u32, reason: &str) -> bool {
        if !self.documents.iter().any(|d| d.id == doc_id && !d.signed) {
            return false;
        }
        match self.dlink_mut(doc_id) {
            Some(l) => {
                l.declined = Some(now());
                l.decline_reason = reason.trim().to_string();
                true
            }
            None => false,
        }
    }

    pub fn doc_remote_question(&mut self, doc_id: u32, text: &str) -> bool {
        let t = text.trim();
        if t.is_empty() {
            return false;
        }
        match self.dlink_mut(doc_id) {
            Some(l) => {
                l.questions.push((now(), t.to_string()));
                true
            }
            None => false,
        }
    }

    /// Documentos sin firmar del paciente que ya tienen enlace (para la página «Mi cita»).
    pub fn docs_to_sign(&self, pid: u32) -> Vec<(String, String)> {
        self.documents
            .iter()
            .filter(|d| d.patient_id == pid && !d.signed)
            .filter_map(|d| self.doc_link(d.id).filter(|l| l.declined.is_none()).map(|l| (l.token.clone(), d.title.clone())))
            .collect()
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Documento listo para imprimir o guardar en PDF, con la firma y, si se firmó
/// por internet, los datos que la acreditan.
pub fn doc_html(db: &Db, d: &PatientDoc) -> String {
    let p = db.patient(d.patient_id).cloned().unwrap_or_default();
    let doctor = db.doctor(d.doctor_id).cloned();
    let remote = db.doc_link(d.id).filter(|l| l.signed.is_some() && d.signed);
    let sig = if d.signed { signature_svg(&d.signature, 320.0, 120.0) } else { "<div style='height:120px'></div>".into() };
    let evidence = match remote {
        Some(l) => format!(
            "<div class='ev'>Firmado electrónicamente por internet por <b>{}</b>{} el {} desde la dirección {}.<br>Huella del documento (SHA-256): <code>{}</code></div>",
            esc(&d.signer),
            if l.signer_nif.is_empty() { String::new() } else { format!(" (NIF {})", esc(&l.signer_nif)) },
            l.signed.map(|t| t.format("%d/%m/%Y a las %H:%M").to_string()).unwrap_or_default(),
            esc(if l.ip.is_empty() { "desconocida" } else { &l.ip }),
            esc(&l.doc_hash)
        ),
        None => String::new(),
    };
    format!(
        "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>{title}</title><style>body{{font-family:'Open Sans',Arial,sans-serif;max-width:760px;margin:40px auto;color:#222;line-height:1.55;padding:0 20px}}h1{{font-size:20px;letter-spacing:.5px}}.head{{border-bottom:2px solid #0e9688;padding-bottom:10px;margin-bottom:24px}}.body{{white-space:pre-wrap}}.firmas{{display:flex;gap:60px;margin-top:40px;flex-wrap:wrap}}.box{{border-top:1px solid #999;padding-top:6px;font-size:13px;color:#555}}.ev{{background:#f4f8fb;border-radius:8px;padding:10px 12px;margin-top:20px;font-size:12.5px;color:#334155}}code{{font-size:11px;word-break:break-all}}@media print{{body{{margin:20mm}}}}</style></head><body><div class='head'><b>{clinic}</b><br><small>{addr} · {phone} · NIF {nif}</small></div><h1>{title_up}</h1><p><small>Paciente: {pname} · Nº historia {hist} · NIF {pnif} · Fecha {date}</small></p><div class='body'>{body}</div><div class='firmas'><div>{sig}<div class='box'>Firma del paciente: {signer}{when}</div></div><div><div style='height:120px'></div><div class='box'>{doc} · Nº colegiado {lic}</div></div></div>{evidence}</body></html>",
        title = esc(&d.title),
        clinic = esc(&db.config.name),
        addr = esc(&db.config.address),
        phone = esc(&db.config.phone),
        nif = esc(&db.config.nif),
        title_up = esc(&d.title.to_uppercase()),
        pname = esc(&p.full_name()),
        hist = p.history_no,
        pnif = esc(&p.nif),
        date = fmt_date(d.date),
        body = esc(&d.body),
        signer = esc(if d.signed { &d.signer } else { "(sin firmar)" }),
        when = d.signed_at.map(|t| format!(" · {}", t.format("%d/%m/%Y %H:%M"))).unwrap_or_default(),
        doc = esc(&doctor.as_ref().map(|c| c.name.clone()).unwrap_or_default()),
        lic = esc(&doctor.as_ref().map(|c| c.license.clone()).unwrap_or_default()),
    )
}

// ───────────────────────────── lo que ve la página ─────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct PubDoc {
    pub doc_id: u32,
    pub first_name: String,
    pub title: String,
    pub category: String,
    pub body: String,
    pub date: NaiveDate,
    pub doctor: String,
    /// pendiente, firmado, en_clinica
    pub state: &'static str,
    pub hash: String,
    pub signed_by: String,
    pub signed_at: Option<NaiveDateTime>,
    pub viewed: bool,
}

pub fn publish_docs(db: &Db) -> HashMap<String, PubDoc> {
    let mut m = HashMap::new();
    if !db.booking.docs {
        return m;
    }
    for l in &db.doc_links {
        let Some(d) = db.documents.iter().find(|d| d.id == l.doc_id) else { continue };
        let doctor = db.doctor(d.doctor_id).map(|c| if c.license.trim().is_empty() || c.license == "-" { c.name.clone() } else { format!("{} (nº col. {})", c.name, c.license) }).unwrap_or_default();
        m.insert(
            l.token.clone(),
            PubDoc {
                doc_id: d.id,
                first_name: db.patient(d.patient_id).map(|p| p.first_name.clone()).unwrap_or_default(),
                title: d.title.clone(),
                category: d.category.clone(),
                body: d.body.clone(),
                date: d.date,
                doctor,
                state: if d.signed {
                    "firmado"
                } else if l.declined.is_some() {
                    "en_clinica"
                } else {
                    "pendiente"
                },
                hash: doc_hash(d),
                signed_by: if d.signed { d.signer.clone() } else { String::new() },
                signed_at: if d.signed { d.signed_at } else { None },
                viewed: l.viewed.is_some(),
            },
        );
    }
    m
}

pub fn doc_json(p: &crate::booking::Published, d: &PubDoc) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "clinica": p.clinic, "direccion": p.address, "telefono": p.phone,
        "nombre": d.first_name, "titulo": d.title, "categoria": d.category,
        "texto": d.body, "fecha": fmt_date(d.date), "profesional": d.doctor,
        "estado": d.state, "huella": d.hash,
        "firmado_por": d.signed_by,
        "firmado_el": d.signed_at.map(|t| t.format("%d/%m/%Y %H:%M").to_string()),
    })
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct SignBody {
    pub nombre: String,
    pub nif: String,
    pub firma: Strokes,
    pub huella: String,
    pub acepta: bool,
}

/// Lo que el paciente hace con el enlace del documento.
pub enum DocAction {
    Viewed,
    Sign { signer: String, nif: String, strokes: Strokes, hash: String, ip: String },
    Decline(String),
    Question(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig() -> Strokes {
        vec![(0..24).map(|i| [i as f32 / 24.0, 0.5 + (i as f32 / 2.0).sin() * 0.2]).collect()]
    }

    /// Un consentimiento nuevo de un paciente, con su enlace.
    fn consent(db: &mut Db) -> u32 {
        let tid = db.doc_templates.iter().find(|t| t.name.contains("extracción")).map(|t| t.id).unwrap();
        let id = db.new_document(101, tid, &[38]).unwrap();
        db.doc_token(id);
        id
    }

    #[test]
    fn signed_online_with_evidence() {
        let mut db = Db::sample(today());
        let id = consent(&mut db);
        let d = db.documents.iter().find(|d| d.id == id).unwrap().clone();
        let hash = doc_hash(&d);
        assert!(db.doc_remote_sign(id, "Carlos Martínez Sanz", "", sig(), "otra", "1.2.3.4").is_err(), "otra huella");
        assert!(db.doc_remote_sign(id, "Carlos Martínez Sanz", "", vec![], &hash, "1.2.3.4").is_err(), "sin firma");
        assert!(db.doc_remote_sign(id, "", "", sig(), &hash, "1.2.3.4").is_err(), "sin nombre");
        db.doc_remote_sign(id, "Carlos Martínez Sanz", "12345678Z", sig(), &hash, "1.2.3.4").unwrap();
        let d = db.documents.iter().find(|d| d.id == id).unwrap().clone();
        assert!(d.signed && d.signer == "Carlos Martínez Sanz" && !d.signature.is_empty());
        let html = doc_html(&db, &d);
        assert!(html.contains("por internet") && html.contains("1.2.3.4") && html.contains(&hash) && html.contains("<polyline"));
        assert!(db.doc_remote_sign(id, "Carlos", "", sig(), &hash, "").is_err(), "no se firma dos veces");
        assert!(db.docs_to_sign(101).iter().all(|(_, t)| t != &d.title), "ya no está pendiente");
    }

    #[test]
    fn gdpr_document_marks_consent() {
        let mut db = Db::sample(today());
        let tid = db.doc_templates.iter().find(|t| t.marks_gdpr).map(|t| t.id).unwrap();
        let pid = db.patients.iter().find(|p| !p.accepts_gdpr).map(|p| p.id).unwrap_or(110);
        db.patients.iter_mut().find(|p| p.id == pid).unwrap().accepts_gdpr = false;
        let id = db.new_document(pid, tid, &[]).unwrap();
        db.doc_token(id);
        let hash = doc_hash(db.documents.iter().find(|d| d.id == id).unwrap());
        db.doc_remote_sign(id, "Paciente de Prueba", "", sig(), &hash, "").unwrap();
        assert!(db.patient(pid).unwrap().accepts_gdpr, "firmar la protección de datos la marca en la ficha");
    }

    #[test]
    fn decline_questions_and_pending_list() {
        let mut db = Db::sample(today());
        let id = consent(&mut db);
        let title = db.documents.iter().find(|d| d.id == id).unwrap().title.clone();
        assert!(db.docs_to_sign(101).iter().any(|(_, t)| *t == title));
        assert!(db.doc_remote_question(id, "¿Me dolerá después?"));
        assert!(db.doc_remote_decline(id, "Prefiero hablarlo con la doctora"));
        let l = db.doc_link(id).unwrap();
        assert!(l.declined.is_some() && l.questions.len() == 1);
        assert!(db.docs_to_sign(101).iter().all(|(_, t)| *t != title), "si prefiere firmarlo en la clínica, no se le vuelve a pedir");
    }
}
