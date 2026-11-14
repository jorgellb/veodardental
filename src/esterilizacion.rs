//! Esterilización (T-F5-08): ciclos del autoclave, lotes de instrumental y
//! su uso en cada paciente, para saber a quién afecta una incidencia.
//!
//! 1. **Ciclo:** autoclave, programa, número y hora; los lotes que lleva
//!    (bolsas o cajas con su contenido y su etiqueta).
//! 2. **Validación:** con los indicadores (químico, biológico, Bowie-Dick).
//!    Los lotes se pueden usar cuando el ciclo está validado y no están
//!    caducados.
//! 3. **Uso:** al usar un lote, queda ligado al paciente y, si se dice, a su
//!    tratamiento o su cita.
//! 4. **Incidencia** (por ejemplo, un biológico positivo días después):
//!    - el ciclo pasa a fallido;
//!    - sus lotes sin usar se retiran;
//!    - salen los pacientes en los que se usaron, con fecha y tratamiento
//!      ([`Db::affected_by_cycle`]).
//!
//! Todo queda en la auditoría.

use crate::model::*;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CycleState {
    #[default]
    EnCurso,
    Validado,
    Fallido,
}

impl CycleState {
    pub fn label(self) -> &'static str {
        match self {
            CycleState::EnCurso => "En curso",
            CycleState::Validado => "Validado",
            CycleState::Fallido => "Fallido",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cycle {
    pub id: u32,
    pub autoclave: String,
    /// Número del ciclo en ese autoclave.
    pub number: u32,
    pub program: String,
    pub started: NaiveDateTime,
    #[serde(default)]
    pub state: CycleState,
    /// Indicadores: químico, biológico (puede llegar días después) y Bowie-Dick.
    #[serde(default)]
    pub chemical: Option<bool>,
    #[serde(default)]
    pub biological: Option<bool>,
    #[serde(default)]
    pub bowie_dick: Option<bool>,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub incident: String,
}

/// Uso de un lote en un paciente.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PackUse {
    pub patient_id: u32,
    pub at: NaiveDateTime,
    #[serde(default)]
    pub entry_id: Option<u32>,
    #[serde(default)]
    pub appt_id: Option<u32>,
    pub by: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pack {
    pub id: u32,
    /// Etiqueta (única): ciclo y número de lote.
    pub code: String,
    pub cycle_id: u32,
    pub contents: String,
    pub expires: NaiveDate,
    #[serde(default)]
    pub used: Option<PackUse>,
    /// Retirado (ciclo fallido, envoltorio dañado…) y por qué.
    #[serde(default)]
    pub withdrawn: Option<String>,
}

/// Un paciente afectado por una incidencia: el lote, cuándo y en qué.
#[derive(Clone, Debug, PartialEq)]
pub struct Affected {
    pub patient_id: u32,
    pub pack: String,
    pub at: NaiveDateTime,
    pub treatment: String,
}

impl Db {
    pub fn cycle(&self, id: u32) -> Option<&Cycle> {
        self.sterile_cycles.iter().find(|c| c.id == id)
    }

    /// Ciclo nuevo con `packs` lotes (contenido de cada uno) que caducan a
    /// los `days` días.
    pub fn new_cycle(&mut self, autoclave: &str, program: &str, packs: &[String], days: u32, now: NaiveDateTime) -> Result<u32, String> {
        if autoclave.trim().is_empty() || packs.iter().all(|p| p.trim().is_empty()) {
            return Err("Indique el autoclave y al menos un lote con su contenido.".into());
        }
        let number = self.sterile_cycles.iter().filter(|c| c.autoclave == autoclave.trim()).map(|c| c.number).max().unwrap_or(0) + 1;
        let id = self.new_id();
        let by = self.config.current_user.clone();
        self.sterile_cycles.push(Cycle { id, autoclave: autoclave.trim().into(), number, program: program.trim().into(), started: now, state: CycleState::EnCurso, chemical: None, biological: None, bowie_dick: None, by, incident: String::new() });
        let prefix: String = autoclave.trim().chars().filter(|c| c.is_ascii_alphanumeric()).take(12).collect::<String>().to_uppercase();
        for (i, content) in packs.iter().filter(|p| !p.trim().is_empty()).enumerate() {
            let pid = self.new_id();
            // la etiqueta no se repite nunca (dos autoclaves con nombres parecidos)
            let base = format!("{prefix}-{number:04}-{:02}", i + 1);
            let mut code = base.clone();
            let mut n = 1;
            while self.sterile_packs.iter().any(|p| p.code.eq_ignore_ascii_case(&code)) {
                n += 1;
                code = format!("{base}-{n}");
            }
            self.sterile_packs.push(Pack { id: pid, code, cycle_id: id, contents: content.trim().into(), expires: now.date() + chrono::Duration::days(days as i64), used: None, withdrawn: None });
        }
        self.audit("Ciclo de esterilización", &format!("{} nº {number} · {} · {} lotes", autoclave.trim(), program.trim(), packs.len()));
        Ok(id)
    }

    /// Resultado de los indicadores: validado si todos los dados son buenos;
    /// fallido si alguno no lo es.
    pub fn validate_cycle(&mut self, id: u32, chemical: bool, bowie_dick: Option<bool>, now: NaiveDateTime) -> Result<CycleState, String> {
        let c = self.sterile_cycles.iter_mut().find(|c| c.id == id).ok_or("El ciclo ya no existe.")?;
        if c.state != CycleState::EnCurso {
            return Err("El ciclo ya está cerrado.".into());
        }
        c.chemical = Some(chemical);
        c.bowie_dick = bowie_dick;
        let ok = chemical && bowie_dick != Some(false);
        let label = format!("{} nº {}", c.autoclave, c.number);
        if ok {
            c.state = CycleState::Validado;
            self.audit("Validar ciclo de esterilización", &label);
            Ok(CycleState::Validado)
        } else {
            self.cycle_incident(id, "Indicador no conforme al terminar el ciclo", now)?;
            Ok(CycleState::Fallido)
        }
    }

    /// Resultado del indicador biológico (llega después): si es positivo, incidencia.
    pub fn biological_result(&mut self, id: u32, ok: bool, now: NaiveDateTime) -> Result<Vec<Affected>, String> {
        let c = self.sterile_cycles.iter_mut().find(|c| c.id == id).ok_or("El ciclo ya no existe.")?;
        c.biological = Some(ok);
        if ok {
            let label = format!("{} nº {}", c.autoclave, c.number);
            self.audit("Indicador biológico correcto", &label);
            return Ok(vec![]);
        }
        self.cycle_incident(id, "Indicador biológico positivo", now)
    }

    /// Incidencia en un ciclo: fallido, lotes sin usar retirados y lista de afectados.
    pub fn cycle_incident(&mut self, id: u32, why: &str, _now: NaiveDateTime) -> Result<Vec<Affected>, String> {
        let c = self.sterile_cycles.iter_mut().find(|c| c.id == id).ok_or("El ciclo ya no existe.")?;
        c.state = CycleState::Fallido;
        c.incident = why.trim().to_string();
        let label = format!("{} nº {}", c.autoclave, c.number);
        let mut withdrawn = 0;
        for p in self.sterile_packs.iter_mut().filter(|p| p.cycle_id == id && p.used.is_none() && p.withdrawn.is_none()) {
            p.withdrawn = Some(format!("Ciclo fallido: {}", why.trim()));
            withdrawn += 1;
        }
        let affected = self.affected_by_cycle(id);
        self.audit("Incidencia de esterilización", &format!("{label} · {} · {withdrawn} lotes retirados · {} pacientes afectados", why.trim(), affected.len()));
        Ok(affected)
    }

    /// Pacientes en los que se usaron lotes de un ciclo.
    pub fn affected_by_cycle(&self, id: u32) -> Vec<Affected> {
        let mut out: Vec<Affected> = self
            .sterile_packs
            .iter()
            .filter(|p| p.cycle_id == id)
            .filter_map(|p| {
                let u = p.used.as_ref()?;
                let treatment = u.entry_id.and_then(|e| self.entries.iter().find(|x| x.id == e)).map(|e| format!("{} {}", e.description, e.teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join("-"))).unwrap_or_default();
                Some(Affected { patient_id: u.patient_id, pack: p.code.clone(), at: u.at, treatment })
            })
            .collect();
        out.sort_by_key(|a| (a.at, a.patient_id));
        out
    }

    /// Por qué no se puede usar un lote (`None`: se puede).
    pub fn pack_problem(&self, code: &str, today: NaiveDate) -> Option<String> {
        let Some(p) = self.sterile_packs.iter().find(|p| p.code.eq_ignore_ascii_case(code.trim())) else { return Some("No hay ningún lote con esa etiqueta.".into()) };
        if let Some(w) = &p.withdrawn {
            return Some(format!("El lote está retirado: {w}."));
        }
        if p.used.is_some() {
            return Some("El lote ya se usó.".into());
        }
        if p.expires < today {
            return Some(format!("El lote caducó el {}.", fmt_date(p.expires)));
        }
        match self.cycle(p.cycle_id).map(|c| c.state) {
            Some(CycleState::Validado) => None,
            Some(CycleState::EnCurso) => Some("El ciclo de ese lote aún no está validado.".into()),
            _ => Some("El ciclo de ese lote es fallido.".into()),
        }
    }

    /// Usa un lote en un paciente (y su tratamiento o cita).
    pub fn use_pack(&mut self, code: &str, pid: u32, entry: Option<u32>, appt: Option<u32>, now: NaiveDateTime) -> Result<(), String> {
        if let Some(p) = self.pack_problem(code, now.date()) {
            return Err(p);
        }
        let by = self.config.current_user.clone();
        let p = self.sterile_packs.iter_mut().find(|p| p.code.eq_ignore_ascii_case(code.trim())).ok_or("No hay ningún lote con esa etiqueta.")?;
        p.used = Some(PackUse { patient_id: pid, at: now, entry_id: entry, appt_id: appt, by });
        let code = p.code.clone();
        self.audit("Usar lote esterilizado", &format!("{} · {code}", self.patient_name(pid)));
        Ok(())
    }

    /// Lotes usados en un paciente (trazabilidad desde su ficha).
    pub fn packs_of_patient(&self, pid: u32) -> Vec<&Pack> {
        self.sterile_packs.iter().filter(|p| p.used.as_ref().is_some_and(|u| u.patient_id == pid)).collect()
    }
}

/// Lista de afectados para exportar (CSV con fórmulas neutralizadas).
pub fn affected_csv(db: &Db, list: &[Affected]) -> String {
    let mut s = "Paciente;Teléfono;Lote;Fecha;Tratamiento\n".to_string();
    for a in list {
        let p = db.patient(a.patient_id);
        let cells = [p.map(|p| p.full_name()).unwrap_or_default(), p.map(|p| p.mobile.clone()).unwrap_or_default(), a.pack.clone(), a.at.format("%d/%m/%Y %H:%M").to_string(), a.treatment.clone()];
        s.push_str(&cells.iter().map(|c| crate::model::sheet_cell(c, ';')).collect::<Vec<_>>().join(";"));
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(d: u32, h: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
    }

    #[test]
    fn packs_are_only_used_from_a_validated_cycle_and_traced_to_the_patient() {
        let mut db = Db::sample(at(14, 9).date());
        let c = db.new_cycle("Autoclave 1", "134 ºC 5 min", &["Kit de exploración".into(), "Kit de extracción".into(), "Kit de higiene".into()], 30, at(10, 8)).unwrap();
        let codes: Vec<String> = db.sterile_packs.iter().filter(|p| p.cycle_id == c).map(|p| p.code.clone()).collect();
        assert_eq!(codes, vec!["AUTOCLAVE1-0001-01", "AUTOCLAVE1-0001-02", "AUTOCLAVE1-0001-03"]);
        assert!(db.use_pack(&codes[0], 101, None, None, at(10, 9)).unwrap_err().contains("no está validado"));
        assert_eq!(db.validate_cycle(c, true, Some(true), at(10, 9)).unwrap(), CycleState::Validado);
        let e = db.entries.iter().find(|e| e.patient_id == 101).map(|e| e.id);
        db.use_pack(&codes[0], 101, e, None, at(10, 10)).unwrap();
        assert!(db.use_pack(&codes[0], 102, None, None, at(10, 11)).unwrap_err().contains("ya se usó"), "un lote, una vez");
        db.use_pack("autoclave1-0001-02", 102, None, None, at(11, 10)).unwrap();
        // etiquetas que no se repiten aunque los autoclaves se llamen parecido
        let x = db.new_cycle("Autoclave-1", "134 ºC", &["Otro".into()], 30, at(10, 8)).unwrap();
        let cx = db.sterile_packs.iter().find(|p| p.cycle_id == x).unwrap().code.clone();
        assert_eq!(cx, "AUTOCLAVE1-0001-01-2");
        assert_eq!(db.packs_of_patient(101).len(), 1);
        // caducado
        assert!(db.pack_problem(&codes[2], at(14, 9).date() + chrono::Duration::days(40)).unwrap().contains("caducó"));
        // el número de ciclo sigue por autoclave
        let c2 = db.new_cycle("Autoclave 1", "121 ºC", &["Fresas".into()], 30, at(12, 8)).unwrap();
        assert_eq!(db.cycle(c2).unwrap().number, 2);
    }

    /// Un biológico positivo días después: ciclo fallido, lotes sin usar
    /// retirados y, como afectados, justo los pacientes en que se usaron.
    #[test]
    fn an_incident_lists_exactly_the_affected_patients_and_is_audited() {
        let mut db = Db::sample(at(14, 9).date());
        db.audit_log.clear();
        let bad = db.new_cycle("Autoclave 1", "134 ºC", &["A".into(), "B".into(), "C".into()], 30, at(10, 8)).unwrap();
        let good = db.new_cycle("Autoclave 2", "134 ºC", &["D".into()], 30, at(10, 8)).unwrap();
        db.validate_cycle(bad, true, None, at(10, 9)).unwrap();
        db.validate_cycle(good, true, None, at(10, 9)).unwrap();
        let code = |db: &Db, c: u32, i: usize| db.sterile_packs.iter().filter(|p| p.cycle_id == c).nth(i).unwrap().code.clone();
        let (a, b, unused, d) = (code(&db, bad, 0), code(&db, bad, 1), code(&db, bad, 2), code(&db, good, 0));
        db.use_pack(&a, 101, None, None, at(11, 10)).unwrap();
        db.use_pack(&b, 102, None, None, at(12, 10)).unwrap();
        db.use_pack(&d, 103, None, None, at(12, 11)).unwrap();
        let affected = db.biological_result(bad, false, at(13, 9)).unwrap();
        assert_eq!(affected.iter().map(|x| (x.patient_id, x.pack.clone())).collect::<Vec<_>>(), vec![(101, a), (102, b)], "solo los del ciclo fallido, por fecha");
        assert_eq!(db.cycle(bad).unwrap().state, CycleState::Fallido);
        assert!(db.pack_problem(&unused, at(13, 9).date()).unwrap().contains("retirado"));
        assert!(db.affected_by_cycle(good).iter().all(|x| x.patient_id == 103));
        assert!(db.audit_log.iter().any(|l| l.action == "Incidencia de esterilización" && l.detail.contains("2 pacientes afectados") && l.detail.contains("1 lotes retirados")));
        let csv = affected_csv(&db, &affected);
        assert_eq!(csv.lines().count(), 3);
        // fallido al terminar (químico malo): lo mismo, sin esperar al biológico
        let c3 = db.new_cycle("Autoclave 1", "134 ºC", &["E".into()], 30, at(13, 8)).unwrap();
        assert_eq!(db.validate_cycle(c3, false, None, at(13, 9)).unwrap(), CycleState::Fallido);
    }
}
