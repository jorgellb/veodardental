//! Cuestionario de salud general y dental (anamnesis) del paciente.
//! Las respuestas que suponen un riesgo clínico generan avisos que pasan
//! a la ficha y se ven en toda la aplicación (agenda, recetas, cobros…).

use crate::model::*;
use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Part {
    General,
    Dental,
}

pub struct Question {
    pub key: &'static str,
    pub text: &'static str,
    pub part: Part,
    /// Pide detalle cuando la respuesta es «sí».
    pub detail: bool,
    /// Aviso que genera en la ficha.
    pub alert: Option<&'static str>,
    /// Solo para mujeres.
    pub female: bool,
    /// La respuesta va a «Alergias».
    pub allergy: bool,
}

const fn q(key: &'static str, text: &'static str, part: Part, detail: bool, alert: Option<&'static str>) -> Question {
    Question { key, text, part, detail, alert, female: false, allergy: false }
}

pub const QUESTIONS: &[Question] = &[
    q("trat_medico", "¿Está ahora en tratamiento médico o psicológico?", Part::General, true, None),
    q("medicacion", "¿Toma alguna medicación de forma habitual?", Part::General, true, Some("Medicación habitual")),
    q("anticoag", "¿Toma anticoagulantes o antiagregantes (Sintrom, aspirina…)?", Part::General, true, Some("Anticoagulado")),
    q("bifosf", "¿Toma o ha tomado medicación para la osteoporosis (bifosfonatos)?", Part::General, true, Some("Bifosfonatos")),
    q("corazon", "¿Tiene o ha tenido problemas de corazón (infarto, angina, soplo, marcapasos)?", Part::General, true, Some("Cardiopatía")),
    q("tension_alta", "¿Tiene la tensión alta?", Part::General, false, Some("Hipertensión")),
    q("tension_baja", "¿Tiene la tensión baja?", Part::General, false, None),
    q("diabetes", "¿Es diabético/a?", Part::General, false, Some("Diabetes")),
    q("respira", "¿Tiene asma u otra dificultad para respirar?", Part::General, false, Some("Asma / respiratorio")),
    q("higado", "¿Tiene o ha tenido problemas de hígado o intestino?", Part::General, true, Some("Hepatopatía")),
    q("contagiosa", "¿Tiene o ha tenido alguna enfermedad contagiosa (hepatitis, VIH, tuberculosis…)?", Part::General, true, Some("Enfermedad infecciosa")),
    q("cancer", "¿Ha tenido cáncer o ha recibido radioterapia o quimioterapia?", Part::General, true, Some("Radio/quimioterapia")),
    q("sangrado", "¿Sangra más de lo normal tras una herida, extracción o cirugía?", Part::General, false, Some("Riesgo de sangrado")),
    q("transfusion", "¿Ha recibido alguna transfusión de sangre?", Part::General, false, None),
    q("mareos", "¿Ha sufrido mareos o desmayos?", Part::General, false, Some("Mareos / desmayos")),
    q("epilepsia", "¿Padece epilepsia o convulsiones?", Part::General, false, Some("Epilepsia")),
    Question { key: "alergia_med", text: "¿Es alérgico/a a algún medicamento (penicilina, anestesia, antiinflamatorios…)?", part: Part::General, detail: true, alert: None, female: false, allergy: true },
    Question { key: "alergia_otra", text: "¿Tiene otras alergias (látex, metales, alimentos…)?", part: Part::General, detail: true, alert: None, female: false, allergy: true },
    Question { key: "embarazo", text: "¿Está embarazada o cree que puede estarlo?", part: Part::General, detail: false, alert: Some("Embarazo"), female: true, allergy: false },
    Question { key: "lactancia", text: "¿Está en periodo de lactancia?", part: Part::General, detail: false, alert: Some("Lactancia"), female: true, allergy: false },
    q("fuma", "¿Fuma?", Part::General, false, Some("Fumador/a")),
    q("encias_sangran", "¿Le sangran las encías al cepillarse?", Part::Dental, false, None),
    q("encias_dolor", "¿Tiene dolor, inflamación o retracción de encías?", Part::Dental, false, None),
    q("sensibilidad", "¿Tiene sensibilidad al frío, al calor o al dulce?", Part::Dental, true, None),
    q("movilidad", "¿Nota algún diente que se mueve?", Part::Dental, false, None),
    q("abscesos", "¿Ha tenido flemones o abscesos?", Part::Dental, false, None),
    q("comida", "¿Se le queda comida entre los dientes?", Part::Dental, false, None),
    q("bruxismo", "¿Aprieta o rechina los dientes (de día o de noche)?", Part::Dental, false, None),
    q("atm", "¿Tiene dolor o ruidos en la mandíbula al abrir la boca?", Part::Dental, false, None),
    q("ronca", "¿Ronca por la noche?", Part::Dental, false, None),
    q("protesis", "¿Lleva prótesis dental?", Part::Dental, false, None),
    q("ferula", "¿Usa férula de descarga?", Part::Dental, false, None),
    q("int_blanq", "¿Le interesaría hacerse un blanqueamiento?", Part::Dental, false, None),
    q("int_orto", "¿Le interesaría un tratamiento de ortodoncia?", Part::Dental, false, None),
    q("no_contento", "¿Hay algo que no le guste del aspecto de sus dientes?", Part::Dental, true, None),
];

pub const LAST_VISIT: [&str; 5] = ["No lo recuerda", "Menos de 1 año", "1 a 2 años", "2 a 4 años", "Más de 4 años"];
pub const AIDS: [&str; 7] = ["Cepillo manual", "Cepillo eléctrico", "Hilo dental", "Cepillo interdental", "Colutorio", "Irrigador", "Palillos"];
pub const REFERRALS: [&str; 8] = ["Recomendación", "Web", "Redes sociales", "Aseguradora", "Paso por la calle", "Buscador / mapas", "Convenio empresa", "Otro"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthForm {
    pub id: u32,
    pub patient_id: u32,
    pub date: NaiveDate,
    pub filled_by: String,
    pub reason: String,
    pub yes: BTreeMap<String, bool>,
    pub details: BTreeMap<String, String>,
    pub cigarettes: u32,
    pub brushings: u32,
    pub last_visit: usize,
    pub aids: Vec<String>,
    pub signed: bool,
    pub notes: String,
}

impl HealthForm {
    pub fn is(&self, key: &str) -> bool {
        self.yes.get(key).copied().unwrap_or(false)
    }
    pub fn detail(&self, key: &str) -> &str {
        self.details.get(key).map(|s| s.as_str()).unwrap_or("")
    }

    /// Avisos clínicos que se derivan de las respuestas.
    pub fn alerts(&self) -> Vec<String> {
        QUESTIONS
            .iter()
            .filter(|q| self.is(q.key))
            .filter_map(|q| {
                let a = q.alert?;
                let d = self.detail(q.key).trim();
                Some(if d.is_empty() || q.key == "medicacion" { a.to_string() } else { format!("{a} ({d})") })
            })
            .collect()
    }

    /// Alergias declaradas.
    pub fn allergies(&self) -> Vec<String> {
        QUESTIONS
            .iter()
            .filter(|q| q.allergy && self.is(q.key))
            .map(|q| {
                let d = self.detail(q.key).trim();
                if d.is_empty() { "sí (sin detallar)".to_string() } else { d.to_string() }
            })
            .collect()
    }

    /// Oportunidades de tratamiento que el paciente ha marcado.
    pub fn interests(&self) -> Vec<&'static str> {
        let mut v = vec![];
        if self.is("int_blanq") {
            v.push("Blanqueamiento");
        }
        if self.is("int_orto") {
            v.push("Ortodoncia");
        }
        if self.is("bruxismo") && !self.is("ferula") {
            v.push("Férula de descarga");
        }
        if self.is("encias_sangran") || self.is("encias_dolor") {
            v.push("Estudio periodontal");
        }
        v
    }
}

/// Añade a `field` (texto separado por « · ») las entradas que falten.
fn merge(field: &mut String, items: &[String]) -> bool {
    let mut changed = false;
    for it in items {
        let key = it.split(" (").next().unwrap_or(it).to_lowercase();
        if !field.to_lowercase().contains(&key) {
            if !field.trim().is_empty() {
                field.push_str(" · ");
            }
            field.push_str(it);
            changed = true;
        }
    }
    changed
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

impl Db {
    pub fn latest_health(&self, pid: u32) -> Option<&HealthForm> {
        self.health.iter().filter(|h| h.patient_id == pid).max_by_key(|h| (h.date, h.id))
    }

    /// Nuevo cuestionario (copia del anterior para que solo haya que revisarlo).
    pub fn new_health(&mut self, pid: u32, today: NaiveDate, by: &str) -> u32 {
        let id = self.new_id();
        let mut f = self.latest_health(pid).cloned().unwrap_or(HealthForm {
            id,
            patient_id: pid,
            date: today,
            filled_by: String::new(),
            reason: String::new(),
            yes: BTreeMap::new(),
            details: BTreeMap::new(),
            cigarettes: 0,
            brushings: 2,
            last_visit: 0,
            aids: vec!["Cepillo manual".into()],
            signed: false,
            notes: String::new(),
        });
        f.id = id;
        f.date = today;
        f.signed = false;
        f.filled_by = by.to_string();
        self.health.push(f);
        id
    }

    /// Pasa a la ficha los avisos y alergias del cuestionario. Devuelve si cambió algo.
    pub fn apply_health(&mut self, form_id: u32) -> bool {
        let Some(f) = self.health.iter().find(|h| h.id == form_id).cloned() else { return false };
        let Some(p) = self.patients.iter_mut().find(|p| p.id == f.patient_id) else { return false };
        let a = merge(&mut p.alerts, &f.alerts());
        let b = merge(&mut p.allergies, &f.allergies().into_iter().filter(|x| !x.starts_with("sí")).collect::<Vec<_>>());
        a || b
    }

    /// Estado del cuestionario para avisar en la ficha.
    pub fn health_status(&self, pid: u32, today: NaiveDate) -> Option<&'static str> {
        match self.latest_health(pid) {
            None => Some("Falta cuestionario de salud"),
            Some(h) if h.date + Duration::days(365) < today => Some("Cuestionario de salud caducado"),
            Some(h) if !h.signed => Some("Cuestionario sin firmar"),
            _ => None,
        }
    }

    /// Pacientes por procedencia dados de alta en el periodo, con su facturación.
    pub fn referral_stats(&self, from: NaiveDate, to: NaiveDate) -> Vec<(String, usize, f64)> {
        let mut m: BTreeMap<String, (usize, f64)> = BTreeMap::new();
        for p in &self.patients {
            let Some(r) = p.registered else { continue };
            if r < from || r > to {
                continue;
            }
            let k = if p.referral.trim().is_empty() { "Sin indicar".to_string() } else { p.referral.trim().to_string() };
            let prod: f64 = self.entries.iter().filter(|e| e.patient_id == p.id && e.state == TxState::Realizado).map(|e| e.amount()).sum();
            let e = m.entry(k).or_default();
            e.0 += 1;
            e.1 += prod;
        }
        let mut v: Vec<(String, usize, f64)> = m.into_iter().map(|(k, (n, p))| (k, n, round2(p))).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.total_cmp(&a.2)));
        v
    }

    /// Cuestionario imprimible (relleno o en blanco) con espacio para la firma.
    pub fn health_html(&self, pid: u32, form: Option<&HealthForm>) -> String {
        let p = self.patient(pid);
        let female = p.map(|p| p.sex == "F").unwrap_or(true);
        let mut rows = String::new();
        for (part, name) in [(Part::General, "Salud general"), (Part::Dental, "Salud bucodental")] {
            rows.push_str(&format!("<h2>{name}</h2><table>"));
            for q in QUESTIONS.iter().filter(|q| q.part == part && (female || !q.female)) {
                let (si, no, det) = match form {
                    Some(f) => {
                        let y = f.is(q.key);
                        (if y { "☒" } else { "☐" }, if y { "☐" } else { "☒" }, esc(f.detail(q.key)))
                    }
                    None => ("☐", "☐", String::new()),
                };
                let det = if q.detail { format!("<div class='det'>{}</div>", if det.is_empty() { "¿Cuál? ______________________________".to_string() } else { det }) } else { String::new() };
                rows.push_str(&format!("<tr><td>{}{det}</td><td class='c'>{si} Sí</td><td class='c'>{no} No</td></tr>", esc(q.text)));
            }
            rows.push_str("</table>");
        }
        let (reason, cig, brush, visit, aids, by, date) = match form {
            Some(f) => (esc(&f.reason), f.cigarettes.to_string(), f.brushings.to_string(), LAST_VISIT[f.last_visit.min(4)].to_string(), esc(&f.aids.join(", ")), esc(&f.filled_by), fmt_date(f.date)),
            None => ("".into(), "____".into(), "____".into(), "________________".into(), "".into(), "".into(), "____/____/______".into()),
        };
        format!(
            "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Cuestionario de salud</title><style>body{{font-family:'Open Sans',Arial,sans-serif;max-width:820px;margin:28px auto;color:#1e293b;font-size:13px}}h1{{font-size:21px;margin:6px 0}}h2{{font-size:15px;margin:18px 0 4px;color:#0e7c71;border-bottom:2px solid #0e9688}}table{{width:100%;border-collapse:collapse}}td{{padding:5px 6px;border-bottom:1px solid #e2e8f0;vertical-align:top}}td.c{{width:60px;white-space:nowrap}}.det{{color:#475569;font-size:12px;margin-top:2px}}.box{{background:#f4f8fb;border-radius:8px;padding:10px;margin-top:10px}}.firma{{margin-top:44px;display:flex;gap:80px}}.firma div{{border-top:1px solid #94a3b8;width:260px;padding-top:6px;font-size:12px;color:#64748b}}@media print{{body{{margin:10mm}}}}</style></head><body><b>{clinic}</b><h1>Cuestionario de salud general y dental</h1><div class='box'>Paciente: <b>{name}</b> · Nº historial {hist} · Fecha: {date}<br>Motivo de la consulta: {reason}<br>Última visita al dentista: {visit} · Cepillados al día: {brush} · Cigarrillos al día: {cig}<br>Utiliza: {aids}</div>{rows}<p>Declaro que los datos anteriores son ciertos y me comprometo a comunicar cualquier cambio en mi estado de salud.</p><div class='firma'><div>Firma del paciente o tutor{by}</div><div>Revisado por el profesional</div></div></body></html>",
            clinic = esc(&self.config.name),
            name = esc(&p.map(|p| p.full_name()).unwrap_or_default()),
            hist = p.map(|p| p.history_no).unwrap_or(0),
            by = if by.is_empty() { String::new() } else { format!(" · {by}") },
        )
    }
}

/// Cuestionarios de ejemplo para el modo demostración.
pub fn sample_health(db: &mut Db, today: NaiveDate) {
    let set = |db: &mut Db, pid: u32, days: i64, yes: &[&str], det: &[(&str, &str)], signed: bool| {
        let id = db.new_health(pid, today - Duration::days(days), "El propio paciente");
        if let Some(f) = db.health.iter_mut().find(|h| h.id == id) {
            for k in yes {
                f.yes.insert(k.to_string(), true);
            }
            for (k, v) in det {
                f.details.insert(k.to_string(), v.to_string());
            }
            f.signed = signed;
            f.reason = "Revisión general".into();
            f.last_visit = 2;
        }
        db.apply_health(id);
    };
    set(db, 100, 40, &["medicacion", "tension_alta", "alergia_med", "int_blanq"], &[("medicacion", "Enalapril 10 mg"), ("alergia_med", "Penicilina")], true);
    set(db, 101, 420, &["anticoag", "corazon", "encias_sangran"], &[("anticoag", "Sintrom"), ("corazon", "Fibrilación auricular")], true);
    set(db, 103, 10, &["bruxismo", "int_orto", "fuma"], &[], false);
    set(db, 104, 90, &["diabetes", "encias_sangran", "movilidad"], &[], true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
    }

    #[test]
    fn answers_become_alerts_without_duplicates() {
        let mut db = Db::sample(d());
        let pid = 110;
        let before = db.patient(pid).unwrap().alerts.clone();
        let id = db.new_health(pid, d(), "Paciente");
        let f = db.health.iter_mut().find(|h| h.id == id).unwrap();
        f.yes.insert("anticoag".into(), true);
        f.details.insert("anticoag".into(), "Sintrom".into());
        f.yes.insert("alergia_med".into(), true);
        f.details.insert("alergia_med".into(), "Látex".into());
        f.yes.insert("int_blanq".into(), true);
        assert!(db.apply_health(id));
        let p = db.patient(pid).unwrap();
        assert!(p.alerts.contains("Anticoagulado (Sintrom)"), "{} / antes {}", p.alerts, before);
        assert!(p.allergies.contains("Látex"));
        // segunda vez no cambia nada
        assert!(!db.apply_health(id));
        assert_eq!(db.latest_health(pid).unwrap().interests(), vec!["Blanqueamiento"]);
    }

    #[test]
    fn status_and_copy_of_previous() {
        let mut db = Db::sample(d());
        assert_eq!(db.health_status(101, d()), Some("Cuestionario de salud caducado"));
        assert_eq!(db.health_status(103, d()), Some("Cuestionario sin firmar"));
        assert_eq!(db.health_status(100, d()), None);
        let id = db.new_health(101, d(), "Paciente");
        let f = db.health.iter().find(|h| h.id == id).unwrap();
        assert!(f.is("anticoag") && !f.signed && f.date == d());
        let html = db.health_html(101, None);
        assert!(html.contains("Cuestionario de salud") && !html.contains("embarazada") == (db.patient(101).unwrap().sex != "F"));
    }

    #[test]
    fn referral_report_counts_patients() {
        let db = Db::sample(d());
        let v = db.referral_stats(d() - Duration::days(3650), d());
        let total: usize = v.iter().map(|r| r.1).sum();
        assert_eq!(total, db.patients.iter().filter(|p| p.registered.is_some_and(|r| r <= d())).count());
    }
}
