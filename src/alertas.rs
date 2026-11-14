//! Alertas clínicas ampliadas (T-F5-07): profilaxis de endocarditis,
//! antirresortivos (bifosfonatos), anticoagulantes y embarazo.
//!
//! - **Qué hacen:** cada alerta dice qué comprobar, con su fuente citada. No
//!   recomienda tratamientos ni receta nada.
//! - **Cuándo saltan:** si el paciente tiene el riesgo en sus alertas (las del
//!   cuestionario de salud o los antecedentes) y se va a hacer un tratamiento
//!   que la alerta afecta.
//! - **Qué piden:** antes de dejar ese tratamiento como hecho, el profesional
//!   marca que la ha revisado. Queda quién y cuándo, y vale para ese día.
//! - **Las de antes siguen:** los avisos del cuestionario y los de las recetas
//!   (`finance`) siguen como estaban.
//! - **Revisión pendiente:** los textos deben revisarlos un odontólogo y la
//!   clínica antes de usarlos con pacientes ([`REVIEWED`], B-16, T-F5-07X).

use crate::model::{Db, OdgKind, fold};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// Los textos aún no los ha revisado un odontólogo (B-16).
pub const REVIEWED: bool = false;

pub struct Rule {
    pub key: &'static str,
    pub title: &'static str,
    /// Palabras (sin acentos) de las alertas o antecedentes que la activan.
    pub when: &'static [&'static str],
    /// Tratamientos a los que afecta (vacío: a todos).
    pub kinds: &'static [OdgKind],
    /// Qué comprobar (no es una recomendación terapéutica).
    pub check: &'static str,
    pub source: &'static str,
    pub url: &'static str,
}

use OdgKind::*;

pub const RULES: [Rule; 4] = [
    Rule {
        key: "endocarditis",
        title: "Cardiopatía: valorar el riesgo de endocarditis",
        when: &["cardiopat", "corazon", "valvul", "endocarditis", "soplo", "cardiaca congenita"],
        kinds: &[Extraccion, Implante, Endodoncia, Apicectomia, Higiene],
        check: "Compruebe si el paciente es de alto riesgo según la AHA: válvula protésica o material de reparación valvular, endocarditis previa, determinadas cardiopatías congénitas o trasplante cardíaco con valvulopatía. Solo en esos casos, y en procedimientos que manipulan la encía o la región periapical o perforan la mucosa, la guía contempla profilaxis antibiótica, que decide el profesional.",
        source: "Wilson WR et al. Prevention of Viridans Group Streptococcal Infective Endocarditis: A Scientific Statement From the American Heart Association. Circulation. 2021;143:e963–e978.",
        url: "https://pubmed.ncbi.nlm.nih.gov/33853363/",
    },
    Rule {
        key: "osteonecrosis",
        title: "Antirresortivos: riesgo de osteonecrosis de los maxilares",
        when: &["bifosfon", "denosumab", "antirresort", "zoledron", "alendron", "risedron", "ibandron"],
        kinds: &[Extraccion, Implante, Apicectomia],
        check: "Riesgo de osteonecrosis de los maxilares relacionada con medicamentos en los procedimientos que exponen o lesionan el hueso (extracciones, implantes, cirugía periapical). Anote el fármaco, la vía (oral o intravenosa), el motivo (osteoporosis o cáncer) y desde cuándo lo toma, y valore el tratamiento con la guía.",
        source: "Ruggiero SL et al. American Association of Oral and Maxillofacial Surgeons' Position Paper on Medication-Related Osteonecrosis of the Jaws—2022 Update. J Oral Maxillofac Surg. 2022;80(5):920-943.",
        url: "https://pubmed.ncbi.nlm.nih.gov/35300956/",
    },
    Rule {
        key: "anticoagulantes",
        title: "Anticoagulantes o antiagregantes: riesgo de sangrado",
        when: &["anticoag", "sintrom", "acenocumarol", "warfarin", "apixaban", "rivaroxaban", "dabigatran", "edoxaban", "heparina", "antiagreg", "clopidogrel", "riesgo de sangrado"],
        kinds: &[Extraccion, Implante, Apicectomia, Higiene],
        check: "Identifique el fármaco (antivitamina K como Sintrom, anticoagulante oral directo, heparina o antiagregante). La guía clasifica los tratamientos por su riesgo de sangrado e indica para cada fármaco qué comprobar (en los antivitamina K, un INR reciente) y qué hacer con la toma. No se interrumpe sin consultarlo con quien lo prescribe.",
        source: "Scottish Dental Clinical Effectiveness Programme (SDCEP). Management of Dental Patients Taking Anticoagulants or Antiplatelet Drugs. 2.ª ed., 2022.",
        url: "https://www.sdcep.org.uk/published-guidance/anticoagulants-and-antiplatelets/",
    },
    Rule {
        key: "embarazo",
        title: "Embarazo: radiografías y medicamentos",
        when: &["embaraz"],
        kinds: &[],
        check: "Valore la indicación de radiografías con los criterios de selección de pacientes de la ADA y la FDA, y antes de recetar revise la sección 4.6 (fertilidad, embarazo y lactancia) de la ficha técnica de cada medicamento en CIMA (AEMPS).",
        source: "ADA / FDA. Dental Radiographic Examinations: Recommendations for Patient Selection and Limiting Radiation Exposure. Y CIMA, Centro de Información de Medicamentos de la AEMPS (ficha técnica, sección 4.6).",
        url: "https://www.fda.gov/radiation-emitting-products/medical-x-ray-imaging/selection-patients-dental-radiographic-examinations",
    },
];

/// El profesional ha revisado una alerta de un paciente (vale ese día).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlertAck {
    pub id: u32,
    pub patient_id: u32,
    pub rule: String,
    pub by: String,
    pub at: NaiveDateTime,
}

impl Rule {
    pub fn affects(&self, kind: OdgKind) -> bool {
        self.kinds.is_empty() || self.kinds.contains(&kind)
    }
}

impl Db {
    /// Lo que dice el paciente de sus riesgos: alertas y antecedentes.
    fn risk_text(&self, pid: u32) -> String {
        self.patient(pid).map(|p| fold(&format!("{} {}", p.alerts, p.background))).unwrap_or_default()
    }

    /// Las alertas que tiene el paciente (por sus riesgos).
    pub fn patient_rules(&self, pid: u32) -> Vec<&'static Rule> {
        let t = self.risk_text(pid);
        RULES.iter().filter(|r| r.when.iter().any(|w| t.contains(w))).collect()
    }

    pub fn alert_acked_today(&self, pid: u32, rule: &str, now: NaiveDateTime) -> Option<&AlertAck> {
        self.alert_acks.iter().filter(|a| a.patient_id == pid && a.rule == rule && a.at.date() == now.date()).max_by_key(|a| a.at)
    }

    /// Alertas sin revisar hoy que afectan a un tratamiento de esta clase:
    /// hasta revisarlas, no se deja como hecho.
    pub fn alerts_blocking(&self, pid: u32, kind: OdgKind, now: NaiveDateTime) -> Vec<&'static Rule> {
        self.patient_rules(pid).into_iter().filter(|r| r.affects(kind) && self.alert_acked_today(pid, r.key, now).is_none()).collect()
    }

    /// El profesional marca que ha revisado la alerta.
    pub fn ack_alert(&mut self, pid: u32, rule: &str, who: &str, now: NaiveDateTime) -> Result<(), String> {
        if !RULES.iter().any(|r| r.key == rule) {
            return Err("Esa alerta no existe.".into());
        }
        let id = self.new_id();
        self.alert_acks.push(AlertAck { id, patient_id: pid, rule: rule.into(), by: who.into(), at: now });
        self.audit("Revisar alerta clínica", &format!("{} · {rule}", self.patient_name(pid)));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 14).unwrap().and_hms_opt(10, 0, 0).unwrap()
    }

    #[test]
    fn each_rule_cites_its_source_and_triggers_with_its_risk() {
        for r in &RULES {
            assert!(!r.source.is_empty() && r.url.starts_with("https://"), "{}", r.key);
            // ninguna receta: ni dosis ni pautas
            assert!(!r.check.contains(" mg") && !r.check.to_lowercase().contains("cada 8"), "{}", r.key);
        }
        let mut db = Db::sample(now().date());
        let p = db.patients.iter_mut().find(|p| p.id == 102).unwrap();
        (p.alerts, p.background) = ("Cardiopatía (válvula protésica), Bifosfonatos".into(), String::new());
        let keys: Vec<&str> = db.patient_rules(102).iter().map(|r| r.key).collect();
        assert_eq!(keys, vec!["endocarditis", "osteonecrosis"]);
        // lo que afecta: una extracción sí; una obturación, no
        assert_eq!(db.alerts_blocking(102, OdgKind::Extraccion, now()).len(), 2);
        assert!(db.alerts_blocking(102, OdgKind::Obturacion, now()).is_empty());
        // también por los antecedentes escritos a mano
        let p = db.patients.iter_mut().find(|p| p.id == 102).unwrap();
        (p.alerts, p.background) = (String::new(), "Toma Sintrom desde 2020".into());
        assert_eq!(db.patient_rules(102)[0].key, "anticoagulantes");
    }

    #[test]
    fn the_professional_reviews_the_alert_and_it_counts_that_day() {
        let mut db = Db::sample(now().date());
        let p = db.patients.iter_mut().find(|p| p.id == 102).unwrap();
        (p.alerts, p.background) = ("Embarazo".into(), String::new());
        assert_eq!(db.alerts_blocking(102, OdgKind::Obturacion, now()).len(), 1, "el embarazo afecta a cualquier tratamiento");
        db.ack_alert(102, "embarazo", "Dra. Ruiz", now()).unwrap();
        assert!(db.alerts_blocking(102, OdgKind::Obturacion, now()).is_empty());
        assert_eq!(db.alert_acked_today(102, "embarazo", now()).unwrap().by, "Dra. Ruiz");
        // al día siguiente, otra vez
        assert_eq!(db.alerts_blocking(102, OdgKind::Obturacion, now() + chrono::Duration::days(1)).len(), 1);
        assert!(db.ack_alert(102, "inventada", "x", now()).is_err());
        const { assert!(!REVIEWED) };
    }
}
