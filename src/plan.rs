//! Plan de tratamiento por fases y alternativas (T-F5-03), del odontograma
//! al presupuesto en un flujo continuo:
//! 1. Los tratamientos previstos en el odontograma pasan a un presupuesto
//!    ([`Db::budget_from_planned`]); es la alternativa A del plan.
//! 2. **Fases:** cada línea va en una fase (higiene, conservadora, prótesis…).
//!    Los subtotales se suman en céntimos y dan exactamente el total.
//! 3. **Alternativas:** copias del presupuesto en el mismo plan (B, C…) para
//!    cambiar lo que haga falta, por ejemplo un puente en vez de un implante.
//! 4. **Versiones:** cambiar un presupuesto ya presentado es hacer una versión
//!    nueva. La anterior se conserva tal cual (también si estaba firmada) y ya
//!    no se puede aceptar.
//! 5. **Aceptación explícita** de una alternativa ([`Db::accept_plan`]): las
//!    demás del plan quedan descartadas. Los previstos que solo eran de las
//!    descartadas se quitan y los comunes se reutilizan, así que no queda nada
//!    duplicado.
//! 6. **Consentimientos:** al aceptar se crean, para firmar, los que piden sus
//!    tratamientos (plantillas con su clase de tratamiento). Los ya firmados no
//!    se tocan y no se duplican.

use crate::cents::Cents;
use crate::model::*;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Lo que hace de un presupuesto parte de un plan.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanInfo {
    /// Plan al que pertenece (el id de su primer presupuesto).
    #[serde(default)]
    pub group: Option<u32>,
    /// Nombre de la alternativa («A», «B»… o el que se le dé).
    #[serde(default)]
    pub alt: String,
    /// Versión (0 o 1: la primera).
    #[serde(default)]
    pub version: u32,
    /// Versión nueva que sustituye a esta.
    #[serde(default)]
    pub replaced_by: Option<u32>,
    /// Alternativa que se eligió en su lugar.
    #[serde(default)]
    pub discarded_for: Option<u32>,
    #[serde(default)]
    pub accepted_by: String,
    /// Nombres de las fases (la fase n de una línea es `phases[n]`).
    #[serde(default)]
    pub phases: Vec<String>,
}

impl PlanInfo {
    pub fn is_default(&self) -> bool {
        *self == PlanInfo::default()
    }
}

impl Budget {
    /// Total exacto en céntimos (la suma de las líneas).
    pub fn total_cents(&self) -> Cents {
        self.lines.iter().map(|l| Cents::from_eur(l.amount())).fold(Cents(0), |a, b| a + b)
    }

    pub fn phase_name(&self, p: u8) -> String {
        self.tp.phases.get(p as usize).filter(|s| !s.trim().is_empty()).cloned().unwrap_or_else(|| format!("Fase {}", p as u32 + 1))
    }

    /// Subtotal de cada fase con líneas, en orden: suman exactamente el total.
    pub fn phase_totals(&self) -> Vec<(u8, String, Cents)> {
        let mut out: Vec<(u8, String, Cents)> = vec![];
        let mut phases: Vec<u8> = self.lines.iter().map(|l| l.phase).collect();
        phases.sort();
        phases.dedup();
        for p in phases {
            let sum = self.lines.iter().filter(|l| l.phase == p).map(|l| Cents::from_eur(l.amount())).fold(Cents(0), |a, b| a + b);
            out.push((p, self.phase_name(p), sum));
        }
        out
    }

    pub fn version(&self) -> u32 {
        self.tp.version.max(1)
    }

    /// «Alternativa B · versión 2» (vacío fuera de un plan).
    pub fn plan_label(&self) -> String {
        let mut v = vec![];
        if !self.tp.alt.is_empty() {
            v.push(format!("Alternativa {}", self.tp.alt));
        }
        if self.version() > 1 {
            v.push(format!("versión {}", self.version()));
        }
        v.join(" · ")
    }
}

/// Lo que ha hecho la aceptación.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Accepted {
    /// Alternativas descartadas.
    pub discarded: Vec<u32>,
    /// Previstos quitados, que solo eran de las descartadas.
    pub removed: usize,
    /// Consentimientos creados para firmar.
    pub consents: Vec<u32>,
}

fn next_alt(used: &[String]) -> String {
    ('A'..='Z').map(|c| c.to_string()).find(|c| !used.contains(c)).unwrap_or_else(|| format!("{}", used.len() + 1))
}

impl Db {
    /// Los presupuestos vigentes del mismo plan (sin las versiones sustituidas).
    pub fn plan_alternatives(&self, bid: u32) -> Vec<&Budget> {
        let Some(g) = self.budgets.iter().find(|b| b.id == bid).and_then(|b| b.tp.group) else { return vec![] };
        self.budgets.iter().filter(|b| b.tp.group == Some(g) && b.tp.replaced_by.is_none()).collect()
    }

    fn copy_budget(&mut self, b: &Budget) -> u32 {
        let id = self.new_id();
        let mut n = b.clone();
        n.id = id;
        n.state = BudgetState::Pendiente;
        n.date = today();
        n.accepted_on = None;
        n.rejected_on = None;
        n.reject_reason.clear();
        n.tp.replaced_by = None;
        n.tp.discarded_for = None;
        n.tp.accepted_by.clear();
        // el plan de cuotas, sin cuotas pagadas
        if let Some(p) = &mut n.plan {
            p.paid = vec![false; p.installments as usize];
        }
        self.budgets.push(n);
        id
    }

    /// Alternativa nueva del plan del presupuesto (una copia para cambiarla).
    pub fn new_alternative(&mut self, bid: u32) -> Result<u32, String> {
        let b = self.budgets.iter().find(|b| b.id == bid).cloned().ok_or("El presupuesto ya no existe.")?;
        let g = b.tp.group.unwrap_or(bid);
        if self.budgets.iter().any(|x| x.tp.group == Some(g) && matches!(x.state, BudgetState::Aceptado | BudgetState::Cerrado)) {
            return Err("Ya se ha aceptado una alternativa de este plan.".into());
        }
        // el original pasa a ser la alternativa A del plan
        if let Some(x) = self.budgets.iter_mut().find(|x| x.id == bid) {
            x.tp.group = Some(g);
            if x.tp.alt.is_empty() {
                x.tp.alt = "A".into();
            }
        }
        let used: Vec<String> = self.budgets.iter().filter(|x| x.tp.group == Some(g)).map(|x| x.tp.alt.clone()).collect();
        let mut copy = b.clone();
        copy.tp.group = Some(g);
        copy.tp.alt = next_alt(&used);
        copy.tp.version = 1;
        copy.title = format!("{} (alternativa {})", b.title.split(" (alternativa").next().unwrap_or(&b.title), copy.tp.alt);
        let id = self.copy_budget(&copy);
        self.audit("Alternativa de presupuesto", &format!("{} · presupuesto #{bid} → #{id} ({})", self.patient_name(b.patient_id), copy.tp.alt));
        Ok(id)
    }

    /// Versión nueva de un presupuesto pendiente: la anterior se conserva tal
    /// cual y ya no se puede aceptar. Los previstos que tenía pasan a la nueva.
    pub fn new_version(&mut self, bid: u32) -> Result<u32, String> {
        let b = self.budgets.iter().find(|b| b.id == bid).cloned().ok_or("El presupuesto ya no existe.")?;
        if b.state != BudgetState::Pendiente {
            return Err("Solo se hace una versión nueva de un presupuesto pendiente.".into());
        }
        if let Some(n) = b.tp.replaced_by {
            return Err(format!("Ya tiene una versión más nueva (#{n})."));
        }
        let mut copy = b.clone();
        copy.tp.version = b.version() + 1;
        let id = self.copy_budget(&copy);
        if let Some(x) = self.budgets.iter_mut().find(|x| x.id == bid) {
            x.tp.replaced_by = Some(id);
        }
        for e in self.entries.iter_mut().filter(|e| e.budget_id == Some(bid) && e.state == TxState::Previsto) {
            e.budget_id = Some(id);
        }
        // el enlace para firmar por internet era de la versión anterior
        self.budget_links.retain(|l| l.budget_id != bid || l.accepted || l.decided.is_some());
        self.audit("Versión nueva de presupuesto", &format!("{} · presupuesto #{bid} → #{id} (versión {})", self.patient_name(b.patient_id), copy.tp.version));
        Ok(id)
    }

    /// Por qué no se puede aceptar un presupuesto (`None`: se puede).
    pub fn accept_problem(&self, bid: u32) -> Option<String> {
        let b = self.budgets.iter().find(|b| b.id == bid)?;
        if b.state != BudgetState::Pendiente {
            return Some("El presupuesto no está pendiente.".into());
        }
        if let Some(n) = b.tp.replaced_by {
            return Some(format!("Hay una versión más nueva de este presupuesto (#{n}): acepte esa."));
        }
        if b.lines.is_empty() {
            return Some("El presupuesto no tiene líneas.".into());
        }
        if let Some(g) = b.tp.group
            && let Some(x) = self.budgets.iter().find(|x| x.tp.group == Some(g) && x.id != bid && matches!(x.state, BudgetState::Aceptado | BudgetState::Cerrado))
        {
            return Some(format!("Ya se aceptó la alternativa {} de este plan.", x.tp.alt));
        }
        None
    }

    /// Acepta expresamente un presupuesto (la alternativa elegida de su plan):
    /// descarta las demás sin dejar previstos duplicados y prepara los
    /// consentimientos que piden sus tratamientos.
    pub fn accept_plan(&mut self, bid: u32, who: &str, date: NaiveDate) -> Result<Accepted, String> {
        if let Some(p) = self.accept_problem(bid) {
            return Err(p);
        }
        let b = self.budgets.iter().find(|b| b.id == bid).cloned().ok_or("El presupuesto ya no existe.")?;
        let mut out = Accepted::default();
        // las demás alternativas pendientes del plan, descartadas
        let others: Vec<u32> = self.plan_alternatives(bid).iter().filter(|x| x.id != bid && x.state == BudgetState::Pendiente).map(|x| x.id).collect();
        let mut released: Vec<u32> = vec![];
        for o in &others {
            if let Some(x) = self.budgets.iter_mut().find(|x| x.id == *o) {
                x.state = BudgetState::Rechazado;
                x.rejected_on = Some(date);
                x.reject_reason = format!("Se eligió la alternativa {}", b.tp.alt);
                x.tp.discarded_for = Some(bid);
            }
            for e in self.entries.iter_mut().filter(|e| e.budget_id == Some(*o) && e.state == TxState::Previsto) {
                e.budget_id = None;
                released.push(e.id);
            }
        }
        out.discarded = others;
        // la elegida reutiliza los previstos iguales (de cualquier alternativa)
        self.accept_budget(bid, date);
        // los previstos que solo eran de las descartadas, fuera (no están facturados ni hechos)
        let gone: Vec<u32> = self.entries.iter().filter(|e| released.contains(&e.id) && e.budget_id.is_none() && e.state == TxState::Previsto && e.invoice_id.is_none()).map(|e| e.id).collect();
        self.entries.retain(|e| !gone.contains(&e.id));
        out.removed = gone.len();
        if let Some(x) = self.budgets.iter_mut().find(|x| x.id == bid) {
            x.tp.accepted_by = who.to_string();
        }
        out.consents = self.prepare_consents(&b);
        let alt = if b.tp.alt.is_empty() { String::new() } else { format!(" (alternativa {})", b.tp.alt) };
        self.audit(
            "Aceptar presupuesto",
            &format!("{} · presupuesto #{bid}{alt} · {} · aceptado por {who}{}", self.patient_name(b.patient_id), b.total_cents(), if out.discarded.is_empty() { String::new() } else { format!(" · descartadas {:?}", out.discarded) }),
        );
        Ok(out)
    }

    /// Consentimientos que piden los tratamientos del presupuesto: uno por
    /// plantilla, con las piezas de sus líneas; no se crea si ya hay uno de
    /// esa plantilla por firmar, o firmado y sin revocar.
    pub fn prepare_consents(&mut self, b: &Budget) -> Vec<u32> {
        let mut out = vec![];
        let templates: Vec<(u32, Vec<OdgKind>)> = self.doc_templates.iter().filter(|t| !t.for_kinds.is_empty()).map(|t| (t.id, t.for_kinds.clone())).collect();
        for (tid, kinds) in templates {
            let mut teeth: Vec<u8> = b.lines.iter().filter(|l| kinds.contains(&l.odg)).flat_map(|l| l.teeth.clone()).collect();
            if !b.lines.iter().any(|l| kinds.contains(&l.odg)) {
                continue;
            }
            teeth.sort();
            teeth.dedup();
            // el mismo texto de piezas que pone `new_document`
            let piezas = if teeth.is_empty() { "indicada".to_string() } else { teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join("-") };
            let have = self.documents.iter().any(|d| d.patient_id == b.patient_id && d.template_id == tid && d.revoked_at.is_none() && d.body.contains(&piezas));
            if have {
                continue;
            }
            if let Some(id) = self.new_document(b.patient_id, tid, &teeth) {
                out.push(id);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 14).unwrap()
    }

    fn line(code: &str, teeth: &[u8], price: f64, odg: OdgKind, phase: u8) -> BudgetLine {
        BudgetLine { code: code.into(), description: format!("Tratamiento {code}"), teeth: teeth.to_vec(), units: 1, price, discount_pct: 0.0, odg, phase }
    }

    /// Un paciente con tres previstos en el odontograma y un presupuesto
    /// hecho con ellos (la alternativa A).
    fn setup() -> (Db, u32, u32) {
        let mut db = Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 14).unwrap());
        let pid = 101;
        db.entries.retain(|e| !(e.patient_id == pid && e.state == TxState::Previsto));
        db.budgets.retain(|b| b.patient_id != pid);
        db.documents.retain(|d| d.patient_id != pid);
        for (code, teeth, odg) in [("0101", vec![], OdgKind::Higiene), ("0901", vec![36u8], OdgKind::Implante), ("0401", vec![46u8], OdgKind::Endodoncia)] {
            let id = db.add_entry(pid, code, teeth, vec![], TxState::Previsto, 1, day());
            let e = db.entries.iter_mut().find(|e| e.id == id).unwrap();
            e.odg = odg;
        }
        let a = db.budget_from_planned(pid).unwrap();
        (db, pid, a)
    }

    #[test]
    fn phases_add_up_exactly_to_the_total() {
        let b = Budget {
            lines: vec![line("1", &[], 33.33, OdgKind::Higiene, 0), line("2", &[11], 66.67, OdgKind::Obturacion, 1), line("3", &[12], 0.01, OdgKind::Obturacion, 1), BudgetLine { discount_pct: 33.0, ..line("4", &[13], 10.0, OdgKind::Corona, 2) }],
            ..setup().0.budgets[0].clone()
        };
        let phases = b.phase_totals();
        assert_eq!(phases.iter().map(|p| p.0).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(phases.iter().fold(Cents(0), |a, p| a + p.2), b.total_cents());
        assert_eq!(b.total_cents(), Cents(3333 + 6667 + 1 + 670));
        assert_eq!(b.phase_name(1), "Fase 2");
        // con muchas líneas al azar, la suma de las fases es siempre el total
        let mut rng = crate::test_rng::Rng(7);
        for _ in 0..200 {
            let n = rng.below(12) + 1;
            let lines: Vec<BudgetLine> =
                (0..n).map(|i| BudgetLine { units: rng.below(4) as u32 + 1, discount_pct: rng.below(50) as f64, ..line(&i.to_string(), &[], rng.below(100_000) as f64 / 100.0, OdgKind::Ninguno, rng.below(4) as u8) }).collect();
            let b = Budget { lines, ..b.clone() };
            assert_eq!(b.phase_totals().iter().fold(Cents(0), |a, p| a + p.2), b.total_cents());
            assert_eq!(b.total_cents(), Cents::from_eur(b.total()), "el total exacto coincide con el que se imprime");
        }
    }

    #[test]
    fn choosing_an_alternative_discards_the_others_without_duplicates() {
        let (mut db, pid, a) = setup();
        assert_eq!(db.entries.iter().filter(|e| e.patient_id == pid && e.budget_id == Some(a)).count(), 3);
        // B: un puente en vez del implante; la higiene y la endodoncia igual
        let b = db.new_alternative(a).unwrap();
        assert_eq!(db.budgets.iter().find(|x| x.id == a).unwrap().tp.alt, "A");
        let bud = db.budgets.iter_mut().find(|x| x.id == b).unwrap();
        assert_eq!((bud.tp.alt.as_str(), bud.tp.group), ("B", Some(a)));
        bud.lines.retain(|l| l.odg != OdgKind::Implante);
        bud.lines.push(line("0801", &[35, 36, 37], 900.0, OdgKind::Puente, 1));
        assert_eq!(db.plan_alternatives(a).len(), 2);
        // se elige B
        let r = db.accept_plan(b, "Dirección", day()).unwrap();
        assert_eq!(r.discarded, vec![a]);
        assert_eq!(r.removed, 1, "el implante solo era de A");
        let pend: Vec<&MedEntry> = db.entries.iter().filter(|e| e.patient_id == pid && e.state == TxState::Previsto).collect();
        assert_eq!(pend.len(), 3, "higiene y endodoncia reutilizadas, y el puente: sin duplicados");
        assert!(pend.iter().all(|e| e.budget_id == Some(b)));
        assert!(!pend.iter().any(|e| e.odg == OdgKind::Implante));
        let ra = db.budgets.iter().find(|x| x.id == a).unwrap();
        assert_eq!((ra.state, ra.tp.discarded_for), (BudgetState::Rechazado, Some(b)));
        // nada más se puede aceptar ni crear en ese plan
        assert!(db.accept_plan(a, "Dirección", day()).is_err());
        assert!(db.accept_plan(b, "Dirección", day()).is_err(), "no se acepta dos veces");
        assert!(db.new_alternative(b).is_err());
        assert_eq!(db.budgets.iter().find(|x| x.id == b).unwrap().tp.accepted_by, "Dirección");
    }

    #[test]
    fn a_new_version_keeps_the_old_one_and_only_the_new_one_is_accepted() {
        let (mut db, pid, a) = setup();
        let v2 = db.new_version(a).unwrap();
        assert_eq!(db.budgets.iter().find(|x| x.id == v2).unwrap().version(), 2);
        assert!(db.accept_problem(a).unwrap().contains("más nueva"));
        assert!(db.new_version(a).is_err());
        // los previstos pasan a la versión nueva
        assert_eq!(db.entries.iter().filter(|e| e.patient_id == pid && e.budget_id == Some(v2)).count(), 3);
        let old = db.budgets.iter().find(|x| x.id == a).unwrap().clone();
        db.budgets.iter_mut().find(|x| x.id == v2).unwrap().lines[0].price = 1.0;
        db.accept_plan(v2, "Dirección", day()).unwrap();
        assert_eq!(db.entries.iter().filter(|e| e.patient_id == pid && e.state == TxState::Previsto).count(), 3, "sin duplicados");
        let after = db.budgets.iter().find(|x| x.id == a).unwrap();
        assert_eq!((after.lines.len(), after.state, after.lines[0].price), (old.lines.len(), BudgetState::Pendiente, old.lines[0].price), "la versión anterior se conserva tal cual");
    }

    /// Datos de la versión 1: los consentimientos de siempre reciben su clase
    /// de tratamiento; los que ha cambiado la clínica, no.
    #[test]
    fn old_data_gets_the_consent_kinds_on_upgrade() {
        let mut db = setup().0;
        db.data_version = 1;
        for t in db.doc_templates.iter_mut() {
            t.for_kinds.clear();
        }
        db.doc_templates.iter_mut().find(|t| t.id == 4).unwrap().name = "Endodoncia (texto de la clínica)".into();
        assert!(db.upgrade());
        let kinds = |id: u32| db.doc_templates.iter().find(|t| t.id == id).unwrap().for_kinds.clone();
        assert_eq!((kinds(2), kinds(3)), (vec![OdgKind::Extraccion], vec![OdgKind::Implante]));
        assert!(kinds(4).is_empty(), "una plantilla cambiada por la clínica no se toca");
        assert_eq!(db.data_version, DATA_VERSION);
    }

    #[test]
    fn accepting_prepares_the_right_consents_and_leaves_signed_ones_alone() {
        let (mut db, pid, a) = setup();
        for t in db.doc_templates.iter_mut() {
            t.for_kinds = match t.id {
                3 => vec![OdgKind::Implante],
                4 => vec![OdgKind::Endodoncia],
                _ => vec![],
            };
        }
        // ya firmó el de endodoncia de la 46
        let signed = db.new_document(pid, 4, &[46]).unwrap();
        db.sign_document(signed, "Paciente", vec![vec![[0.1, 0.1], [0.2, 0.2], [0.3, 0.3], [0.4, 0.4], [0.5, 0.5]]]).unwrap();
        let before = db.documents.iter().find(|d| d.id == signed).unwrap().clone();
        let r = db.accept_plan(a, "Dra. Ruiz", day()).unwrap();
        assert_eq!(r.consents.len(), 1, "solo el de implantes: el de endodoncia ya está firmado");
        let d = db.documents.iter().find(|d| d.id == r.consents[0]).unwrap();
        assert!(d.template_id == 3 && !d.signed && d.body.contains("36"));
        assert_eq!(db.documents.iter().find(|d| d.id == signed).unwrap(), &before, "el firmado no se toca");
        // otro presupuesto con implantes: el consentimiento pendiente no se duplica
        let again = db.prepare_consents(&db.budgets.iter().find(|x| x.id == a).unwrap().clone());
        assert!(again.is_empty());
    }
}
