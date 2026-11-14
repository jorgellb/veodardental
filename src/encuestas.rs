//! Encuestas de satisfacción y solicitud de reseñas (T-F5-09).
//!
//! - **Envío tras la visita:** con la plantilla «Encuesta de satisfacción»
//!   (Comunicaciones), unos días después de una cita finalizada.
//! - **Consentimiento y bajas:** como los demás avisos. Hace falta el
//!   consentimiento y el canal aceptado, y quien se da de baja de las
//!   encuestas (desde la propia encuesta o en su ficha) no las vuelve a recibir.
//! - **Sin duplicados:** una encuesta por cita, y ninguna si el paciente ya
//!   recibió otra en los últimos [`SurveyConfig::min_days`] días.
//! - **Resultados medibles** ([`Db::survey_stats`]): enviadas, respondidas,
//!   tasa de respuesta, NPS (promotores menos detractores) y media, en
//!   total y por profesional.
//! - **Reseña pública sin condiciones:** el enlace a la reseña (si la
//!   clínica lo pone) se ofrece a todos los que abren la encuesta, sea cual
//!   sea su nota, antes y después de responder. La política de contenido de
//!   Google Maps prohíbe pedir solo las reseñas positivas (ver
//!   `docs/normativa.md`).

use crate::model::*;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurveyConfig {
    /// Enlace para dejar una reseña pública (vacío: no se pide).
    #[serde(default)]
    pub review_url: String,
    /// Días sin otra encuesta al mismo paciente.
    #[serde(default = "default_min_days")]
    pub min_days: u32,
}

fn default_min_days() -> u32 {
    90
}

impl Default for SurveyConfig {
    fn default() -> Self {
        SurveyConfig { review_url: String::new(), min_days: default_min_days() }
    }
}

impl SurveyConfig {
    pub fn is_default(&self) -> bool {
        *self == SurveyConfig::default()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Survey {
    pub id: u32,
    pub token: String,
    pub patient_id: u32,
    pub appt_id: u32,
    pub doctor_id: u32,
    pub sent: NaiveDateTime,
    /// Nota de 0 a 10 («¿recomendaría…?») y comentario.
    #[serde(default)]
    pub score: Option<u8>,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub answered: Option<NaiveDateTime>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub sent: usize,
    pub answered: usize,
    pub promoters: usize,
    pub passives: usize,
    pub detractors: usize,
    pub sum: u32,
}

impl Stats {
    /// Respondidas ÷ enviadas, en %.
    pub fn response_pct(&self) -> Option<f64> {
        (self.sent > 0).then(|| self.answered as f64 * 100.0 / self.sent as f64)
    }
    /// NPS = % de promotores (9-10) − % de detractores (0-6), de las respondidas.
    pub fn nps(&self) -> Option<f64> {
        (self.answered > 0).then(|| (self.promoters as f64 - self.detractors as f64) * 100.0 / self.answered as f64)
    }
    pub fn mean(&self) -> Option<f64> {
        (self.answered > 0).then(|| self.sum as f64 / self.answered as f64)
    }
    fn add(&mut self, s: &Survey) {
        self.sent += 1;
        if let Some(x) = s.score {
            self.answered += 1;
            self.sum += x as u32;
            match x {
                9..=10 => self.promoters += 1,
                7..=8 => self.passives += 1,
                _ => self.detractors += 1,
            }
        }
    }
}

/// Lo que se publica de una encuesta (sin datos de salud).
#[derive(Clone, Debug, PartialEq)]
pub struct PubSurvey {
    /// Idioma del paciente (castellano si no tiene).
    pub lang: crate::i18n::Lang,
    pub first_name: String,
    pub answered: bool,
    pub review_url: String,
}

impl Db {
    /// ¿Se le puede enviar una encuesta por esta cita? (sin duplicados)
    pub fn survey_due(&self, a: &Appointment, now: NaiveDateTime) -> bool {
        let Some(pid) = a.patient_id else { return false };
        if a.state != ApptState::Finalizada || self.surveys.iter().any(|s| s.appt_id == a.id) {
            return false;
        }
        let gap = self.config.surveys.min_days as i64;
        !self.surveys.iter().any(|s| s.patient_id == pid && (now - s.sent).num_days() < gap)
    }

    /// Encuesta nueva de una cita (antes de mandar el aviso).
    pub fn new_survey(&mut self, a: &Appointment, now: NaiveDateTime) -> Option<(u32, String)> {
        let pid = a.patient_id?;
        let id = self.new_id();
        let token = crate::portal::new_token();
        self.surveys.push(Survey { id, token: token.clone(), patient_id: pid, appt_id: a.id, doctor_id: a.doctor_id, sent: now, score: None, comment: String::new(), answered: None });
        Some((id, token))
    }

    /// Respuesta desde la página (una sola vez).
    pub fn survey_answer(&mut self, token: &str, score: u8, comment: &str, now: NaiveDateTime) -> Result<(), String> {
        if score > 10 {
            return Err("La nota va de 0 a 10.".into());
        }
        let s = self.surveys.iter_mut().find(|s| s.token == token).ok_or("Encuesta desconocida.")?;
        if s.answered.is_some() {
            return Err("Esta encuesta ya está respondida.".into());
        }
        s.score = Some(score);
        s.comment = comment.chars().take(2000).collect();
        s.answered = Some(now);
        Ok(())
    }

    /// Baja de las encuestas (desde la encuesta o en la ficha).
    pub fn survey_unsubscribe(&mut self, pid: u32) {
        if let Some(p) = self.patients.iter_mut().find(|p| p.id == pid)
            && !p.no_messages.contains(&crate::comms::MsgKind::Encuesta)
        {
            p.no_messages.push(crate::comms::MsgKind::Encuesta);
        }
    }

    /// Resultados de las encuestas enviadas en el periodo: total y por profesional.
    pub fn survey_stats(&self, from: NaiveDate, to: NaiveDate) -> (Stats, Vec<(u32, Stats)>) {
        let mut total = Stats::default();
        let mut by: Vec<(u32, Stats)> = vec![];
        for s in self.surveys.iter().filter(|s| s.sent.date() >= from && s.sent.date() <= to) {
            total.add(s);
            match by.iter_mut().find(|(d, _)| *d == s.doctor_id) {
                Some((_, st)) => st.add(s),
                None => {
                    let mut st = Stats::default();
                    st.add(s);
                    by.push((s.doctor_id, st));
                }
            }
        }
        by.sort_by_key(|(d, _)| *d);
        (total, by)
    }
}

pub fn publish_surveys(db: &Db, now: NaiveDateTime) -> std::collections::HashMap<String, PubSurvey> {
    db.surveys
        .iter()
        .filter(|s| (now - s.sent).num_days() <= 60)
        .map(|s| {
            let first = db.patient(s.patient_id).map(|p| p.first_name.clone()).unwrap_or_default();
            let lang = db.patient(s.patient_id).and_then(|p| p.lang).unwrap_or_default();
            (s.token.clone(), PubSurvey { lang, first_name: first, answered: s.answered.is_some(), review_url: db.config.surveys.review_url.trim().to_string() })
        })
        .collect()
}

/// La página de la encuesta, en el idioma del paciente. El enlace a la
/// reseña sale igual para todos, antes y después de responder, sea cual sea
/// la nota.
pub fn survey_page(clinic: &str, token: &str, s: &PubSurvey, done: bool) -> String {
    use crate::i18n::t;
    let e = crate::booking::esc_html;
    let lang = s.lang;
    let review = if s.review_url.starts_with("https://") {
        format!("<p class=\"review\">{} <a href=\"{}\" rel=\"noopener noreferrer\" target=\"_blank\">{}</a>.</p>", e(t(lang, "Si quiere, también puede contar su experiencia en una reseña pública:")), e(&s.review_url), e(t(lang, "dejar una reseña")))
    } else {
        String::new()
    };
    let body = if done || s.answered {
        format!("<p class=\"ok\">{}</p>{review}", e(t(lang, "Gracias por responder.")))
    } else {
        let radios: String = (0..=10).map(|n| format!("<label class=\"n\"><input type=\"radio\" name=\"puntuacion\" value=\"{n}\" required>{n}</label>")).collect();
        let question = t(lang, "Del 0 al 10, ¿con qué probabilidad recomendaría [CLINICA] a un familiar o amigo?").replace("[CLINICA]", clinic);
        format!(
            "<form method=\"post\" action=\"/api/encuesta/{tk}\"><p>{q}</p><div class=\"scale\">{radios}</div><p><textarea name=\"comentario\" maxlength=\"2000\" placeholder=\"{ph}\"></textarea></p><button class=\"main\">{send}</button></form>{review}<form method=\"post\" action=\"/api/encuesta/{tk}\"><input type=\"hidden\" name=\"baja\" value=\"1\"><button class=\"link\">{off}</button></form>",
            tk = e(token),
            q = e(&question),
            ph = e(t(lang, "¿Algo que quiera contarnos? (opcional)")),
            send = e(t(lang, "Enviar")),
            off = e(t(lang, "No quiero recibir más encuestas"))
        )
    };
    format!(
        "<!doctype html><html lang=\"{l}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"robots\" content=\"noindex\"><meta name=\"referrer\" content=\"no-referrer\"><title>{c}</title><style>body{{margin:0;font-family:system-ui,Arial,sans-serif;color:#1e293b;background:#f4f7fb}}.wrap{{max-width:560px;margin:0 auto;padding:28px 18px}}.card{{background:#fff;border:1px solid #e2e8f0;border-radius:20px;padding:22px}}.scale{{display:flex;flex-wrap:wrap;gap:6px}}.n{{border:1px solid #cbd5e1;border-radius:10px;padding:8px 10px}}textarea{{width:100%;min-height:80px;border-radius:12px;border:1px solid #cbd5e1;padding:10px;font:inherit}}.main{{width:100%;background:#0d9488;border:0;color:#fff;border-radius:14px;padding:14px;font-size:16px;font-weight:600}}.link{{background:none;border:0;color:#64748b;text-decoration:underline;margin-top:14px}}.ok{{color:#16a34a;font-weight:600}}</style></head><body><div class=\"wrap\"><div class=\"card\"><h1>{c}</h1><p>{hi}{n}. {intro}</p>{body}</div></div></body></html>",
        c = e(clinic),
        l = lang.code(),
        hi = e(t(lang, "Hola")),
        n = if s.first_name.is_empty() { String::new() } else { format!(", {}", e(&s.first_name)) },
        intro = e(t(lang, "Nos ayuda mucho saber cómo fue su visita."))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 14).unwrap().and_hms_opt(10, 0, 0).unwrap()
    }

    #[test]
    fn surveys_are_not_duplicated_and_results_are_measured() {
        let mut db = Db::sample(now().date());
        db.surveys.clear();
        let mut a = db.appointments.iter().find(|a| a.patient_id == Some(101)).unwrap().clone();
        a.state = ApptState::Finalizada;
        assert!(db.survey_due(&a, now()));
        let (_, t1) = db.new_survey(&a, now()).unwrap();
        assert!(!db.survey_due(&a, now()), "una por cita");
        let mut b = a.clone();
        b.id = 999_999;
        assert!(!db.survey_due(&b, now() + chrono::Duration::days(30)), "ni otra al mismo paciente antes de 90 días");
        assert!(db.survey_due(&b, now() + chrono::Duration::days(91)));
        b.state = ApptState::NoAcude;
        assert!(!db.survey_due(&b, now() + chrono::Duration::days(91)), "solo tras una visita");
        db.survey_answer(&t1, 9, "Muy bien", now()).unwrap();
        assert!(db.survey_answer(&t1, 2, "", now()).is_err(), "se responde una vez");
        // más respuestas sintéticas: 10, 7, 3 y una sin responder
        for (i, sc) in [Some(10u8), Some(7), Some(3), None].into_iter().enumerate() {
            let mut x = a.clone();
            x.id = 900_000 + i as u32;
            x.patient_id = Some(102 + i as u32);
            let (_, t) = db.new_survey(&x, now()).unwrap();
            if let Some(sc) = sc {
                db.survey_answer(&t, sc, "", now()).unwrap();
            }
        }
        let (st, by) = db.survey_stats(now().date(), now().date());
        assert_eq!((st.sent, st.answered, st.promoters, st.passives, st.detractors), (5, 4, 2, 1, 1));
        assert_eq!(st.nps(), Some(25.0), "(2 − 1) ÷ 4");
        assert_eq!(st.mean(), Some(29.0 / 4.0));
        assert_eq!(st.response_pct(), Some(80.0));
        assert_eq!(by.len(), 1);
        db.survey_unsubscribe(101);
        assert!(db.patient(101).unwrap().no_messages.contains(&crate::comms::MsgKind::Encuesta));
    }

    /// El aviso sale tras la visita, a quien lo acepta, una vez; quien se da
    /// de baja no lo recibe; sin consentimiento no queda encuesta.
    #[test]
    fn the_survey_message_respects_consent_unsubscribe_and_duplicates() {
        let day = now().date();
        let mut db = Db::sample(day);
        db.surveys.clear();
        db.messages.clear();
        db.booking.enabled = true;
        db.booking.public_url = "https://citas.clinica.es".into();
        for t in db.msg_templates.iter_mut() {
            t.active = t.kind == crate::comms::MsgKind::Encuesta;
        }
        let tpl = db.msg_templates.iter().find(|t| t.kind == crate::comms::MsgKind::Encuesta).unwrap().clone();
        assert_eq!(tpl.days, 1);
        // tres visitas finalizadas ayer: con consentimiento, sin él y dado de baja
        let base = db.appointments.iter().find(|a| a.patient_id.is_some()).unwrap().clone();
        let mut ids = vec![];
        for (i, pid) in [101u32, 102, 103].into_iter().enumerate() {
            let mut a = base.clone();
            (a.id, a.patient_id, a.date, a.state) = (800_000 + i as u32, Some(pid), day - chrono::Duration::days(1), ApptState::Finalizada);
            db.appointments.push(a);
            ids.push(pid);
        }
        for p in db.patients.iter_mut() {
            match p.id {
                101 => (p.accepts_gdpr, p.accepts_whatsapp, p.mobile) = (true, true, "600000001".into()),
                102 => p.accepts_gdpr = false,
                103 => {
                    (p.accepts_gdpr, p.accepts_whatsapp, p.mobile) = (true, true, "600000003".into());
                    p.no_messages.push(crate::comms::MsgKind::Encuesta);
                }
                _ => {}
            }
        }
        db.generate_messages(day);
        let ours = |db: &Db| db.messages.iter().filter(|m| m.kind == crate::comms::MsgKind::Encuesta && ids.contains(&m.patient_id)).map(|m| (m.patient_id, m.text.clone())).collect::<Vec<_>>();
        let sent = ours(&db);
        assert_eq!(sent.iter().map(|m| m.0).collect::<Vec<_>>(), vec![101], "solo a quien acepta y no se ha dado de baja");
        assert!(sent[0].1.contains("https://citas.clinica.es/encuesta/"));
        assert!(!db.surveys.iter().any(|s| s.patient_id == 102 || s.patient_id == 103), "sin aviso no queda encuesta");
        assert_eq!(db.surveys.len(), db.messages.iter().filter(|m| m.kind == crate::comms::MsgKind::Encuesta).count(), "una encuesta por aviso");
        // otra vez el mismo día y al siguiente: nada duplicado
        let total = db.messages.len();
        db.generate_messages(day);
        db.generate_messages(day + chrono::Duration::days(1));
        assert_eq!(ours(&db).len(), 1);
        assert!(db.messages.len() >= total);
        assert_eq!(db.surveys.len(), db.messages.iter().filter(|m| m.kind == crate::comms::MsgKind::Encuesta).count(), "una encuesta por aviso");
    }

    /// Una encuesta respondida no se retoca desde un puesto (los resultados
    /// tienen que ser de verdad); una sin responder sí se puede cambiar.
    #[test]
    fn an_answered_survey_cannot_be_retouched_by_a_station() {
        let s = Survey { id: 1, token: "t".into(), patient_id: 101, appt_id: 2, doctor_id: 1, sent: now(), score: Some(3), comment: "Mal".into(), answered: Some(now()) };
        let cur = serde_json::to_string(&s).unwrap();
        let better = serde_json::to_string(&Survey { score: Some(10), comment: "Genial".into(), ..s.clone() }).unwrap();
        assert_eq!(crate::firmado::protect_json("surveys", &cur, Some(&better)), Some(cur.clone()));
        assert_eq!(crate::firmado::protect_json("surveys", &cur, None), Some(cur.clone()), "ni se borra");
        let open = serde_json::to_string(&Survey { score: None, answered: None, ..s }).unwrap();
        assert_eq!(crate::firmado::protect_json("surveys", &open, Some(&better)), None);
    }

    /// El enlace a la reseña lo ve todo el que abre la encuesta, con
    /// cualquier nota: no se pide solo a quien está contento.
    #[test]
    fn the_review_link_is_offered_to_everyone_whatever_the_score() {
        let s = PubSurvey { lang: crate::i18n::Lang::Es, first_name: "Ana".into(), answered: false, review_url: "https://g.page/r/clinica/review".into() };
        let before = survey_page("Clínica", &"a".repeat(32), &s, false);
        let after_low = survey_page("Clínica", &"a".repeat(32), &PubSurvey { answered: true, ..s.clone() }, true);
        for page in [&before, &after_low] {
            assert!(page.contains("https://g.page/r/clinica/review"), "{page}");
        }
        assert!(before.contains("name=\"baja\""));
        assert!(!survey_page("C", "t", &PubSurvey { review_url: "javascript:alert(1)".into(), ..s }, false).contains("javascript:"), "solo enlaces https");
    }
}
