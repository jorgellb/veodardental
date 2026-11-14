//! Facturación a compañías aseguradoras (mutuas): cada acto realizado a un
//! paciente asegurado genera lo que la compañía paga a la clínica según su
//! baremo. Los actos pendientes se agrupan en liquidaciones periódicas que se
//! imprimen/exportan, y se controla lo que la compañía ha pagado.

use crate::model::*;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaimLine {
    pub entry_id: u32,
    pub patient_id: u32,
    pub patient: String,
    pub policy_no: String,
    pub date: NaiveDate,
    pub code: String,
    pub description: String,
    pub teeth: String,
    pub auth_no: String,
    pub fee: f64,
    /// Rechazado por la compañía (motivo): no cuenta en lo que paga y el acto
    /// se puede liquidar de nuevo (corregido) en otra liquidación.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejected: Option<String>,
}

/// Envío de una liquidación a la compañía (por su portal, por correo…).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaimSent {
    pub at: chrono::NaiveDateTime,
    pub channel: String,
    #[serde(default)]
    pub external_ref: String,
    pub by: String,
}

/// Respuesta de la compañía a una liquidación: qué actos rechaza y por qué,
/// y lo que mandó tal cual (acotado).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaimAnswer {
    pub at: chrono::NaiveDateTime,
    pub rejected: Vec<(u32, String)>,
    #[serde(default)]
    pub raw: String,
    pub by: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claim {
    pub id: u32,
    pub number: String,
    pub tariff_id: u32,
    pub date: NaiveDate,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub lines: Vec<ClaimLine>,
    pub total: f64,
    pub paid: f64,
    pub paid_on: Option<NaiveDate>,
    pub notes: String,
    /// Anulada (fecha y motivo): se conserva, pero sus actos vuelven a estar
    /// pendientes de liquidar y no cuenta como pendiente de cobro.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled: Option<(NaiveDate, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sent: Vec<ClaimSent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<ClaimAnswer>,
}

impl Claim {
    /// Lo que debe pagar la compañía: el total sin los actos rechazados.
    pub fn expected(&self) -> f64 {
        round2(self.total - self.lines.iter().filter(|l| l.rejected.is_some()).map(|l| l.fee).sum::<f64>())
    }

    pub fn state(&self) -> &'static str {
        if self.cancelled.is_some() {
            "Anulada"
        } else if self.paid + 0.005 >= self.expected() {
            "Cobrada"
        } else if self.paid > 0.005 {
            "Cobro parcial"
        } else {
            "Pendiente de cobro"
        }
    }
    pub fn pending(&self) -> f64 {
        if self.cancelled.is_some() {
            return 0.0;
        }
        round2((self.expected() - self.paid).max(0.0))
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

impl Db {
    /// Lo que paga la compañía a la clínica por un acto (0 si no hay baremo).
    pub fn insurer_fee(&self, tariff_id: u32, code: &str) -> f64 {
        self.tariff(tariff_id).filter(|t| t.is_insurer).and_then(|t| t.prices.iter().find(|p| p.code == code)).map(|p| p.insurer_fee).unwrap_or(0.0)
    }

    pub fn auth_no(&self, entry_id: u32) -> String {
        self.auths.iter().find(|a| a.0 == entry_id).map(|a| a.1.clone()).unwrap_or_default()
    }

    pub fn set_auth_no(&mut self, entry_id: u32, no: &str) {
        self.auths.retain(|a| a.0 != entry_id);
        if !no.trim().is_empty() {
            self.auths.push((entry_id, no.trim().to_string()));
        }
    }

    pub(crate) fn claimed(&self, entry_id: u32) -> bool {
        // un acto rechazado por la compañía se puede liquidar de nuevo
        self.claims.iter().any(|c| c.cancelled.is_none() && c.lines.iter().any(|l| l.entry_id == entry_id && l.rejected.is_none()))
    }

    /// Actos realizados en el periodo pendientes de liquidar a la compañía.
    pub fn claimable(&self, tariff_id: u32, from: NaiveDate, to: NaiveDate) -> Vec<ClaimLine> {
        let mut v: Vec<ClaimLine> = self
            .entries
            .iter()
            .filter(|e| e.state == TxState::Realizado && !e.settled_elsewhere && e.date >= from && e.date <= to && !self.claimed(e.id))
            .filter_map(|e| {
                let p = self.patient(e.patient_id)?;
                // la compañía del paciente cuando se hizo el acto
                let t = if e.tariff_id != 0 { e.tariff_id } else { p.tariff_id };
                if t != tariff_id {
                    return None;
                }
                let fee = round2(self.insurer_fee(tariff_id, &e.code) * e.units.max(1) as f64);
                if fee <= 0.0 {
                    return None;
                }
                Some(ClaimLine {
                    entry_id: e.id,
                    patient_id: p.id,
                    patient: p.full_name(),
                    policy_no: p.policy_no.clone(),
                    date: e.date,
                    code: e.code.clone(),
                    description: e.description.clone(),
                    teeth: e.teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join("-"),
                    auth_no: self.auth_no(e.id),
                    fee,
                    rejected: None,
                })
            })
            .collect();
        v.sort_by(|a, b| (a.date, &a.patient).cmp(&(b.date, &b.patient)));
        v
    }

    /// Agrupa los actos pendientes en una liquidación numerada.
    pub fn create_claim(&mut self, tariff_id: u32, from: NaiveDate, to: NaiveDate, today: NaiveDate) -> Option<u32> {
        let lines = self.claimable(tariff_id, from, to);
        if lines.is_empty() {
            return None;
        }
        let year = today.year();
        // la siguiente a la más alta del año (si se anula una, su número no se repite)
        let prefix = format!("LIQ-{year}-");
        let n = self.claims.iter().filter_map(|c| c.number.strip_prefix(&prefix)?.parse::<u32>().ok()).max().unwrap_or(0) + 1;
        let id = self.new_id();
        let total = round2(lines.iter().map(|l| l.fee).sum());
        self.claims.push(Claim { id, number: format!("LIQ-{year}-{n:04}"), tariff_id, date: today, from, to, lines, total, paid: 0.0, paid_on: None, notes: String::new(), cancelled: None, sent: vec![], answers: vec![] });
        Some(id)
    }

    pub fn claim_paid(&mut self, id: u32, amount: f64, date: NaiveDate) {
        if let Some(c) = self.claims.iter_mut().find(|c| c.id == id) {
            c.paid = round2((c.paid + amount).min(c.expected()));
            c.paid_on = Some(date);
        }
    }

    /// Enviada a la compañía (por su portal, por correo…), con su referencia.
    pub fn claim_sent(&mut self, id: u32, channel: &str, external_ref: &str, now: chrono::NaiveDateTime) -> Result<(), String> {
        let by = self.config.current_user.clone();
        let c = self.claims.iter_mut().find(|c| c.id == id).ok_or("La liquidación ya no existe.")?;
        if c.cancelled.is_some() {
            return Err("Esa liquidación está anulada.".into());
        }
        if channel.trim().is_empty() {
            return Err("Diga por dónde se envió (portal, correo…).".into());
        }
        c.sent.push(ClaimSent { at: now, channel: channel.trim().chars().take(40).collect(), external_ref: external_ref.trim().chars().take(60).collect(), by });
        Ok(())
    }

    /// Respuesta de la compañía: los actos que rechaza, con su motivo. Se
    /// guarda con las anteriores; esos actos dejan de contar en lo que paga y
    /// se pueden liquidar de nuevo.
    pub fn claim_answer(&mut self, id: u32, rejected: &[(u32, String)], raw: &str, now: chrono::NaiveDateTime) -> Result<f64, String> {
        let by = self.config.current_user.clone();
        let c = self.claims.iter_mut().find(|c| c.id == id).ok_or("La liquidación ya no existe.")?;
        if c.cancelled.is_some() {
            return Err("Esa liquidación está anulada.".into());
        }
        if c.sent.is_empty() {
            return Err("Primero apunte que se ha enviado.".into());
        }
        for (eid, why) in rejected {
            if !c.lines.iter().any(|l| l.entry_id == *eid) {
                return Err("Uno de los actos rechazados no está en esta liquidación.".into());
            }
            if why.trim().is_empty() {
                return Err("Escriba el motivo de cada rechazo.".into());
            }
        }
        let expected_after = round2(c.total - c.lines.iter().filter(|l| l.rejected.is_some() || rejected.iter().any(|(e, _)| *e == l.entry_id)).map(|l| l.fee).sum::<f64>());
        if c.paid > expected_after + 0.005 {
            return Err("Ya se ha cobrado más de lo que quedaría: revise los cobros de esta liquidación.".into());
        }
        for l in c.lines.iter_mut() {
            if let Some((_, why)) = rejected.iter().find(|(e, _)| *e == l.entry_id) {
                l.rejected = Some(why.trim().chars().take(200).collect());
            }
        }
        c.answers.push(ClaimAnswer { at: now, rejected: rejected.iter().map(|(e, w)| (*e, w.trim().to_string())).collect(), raw: raw.chars().take(crate::autorizaciones::RAW_MAX).collect(), by });
        let (number, expected) = (c.number.clone(), c.expected());
        self.audit("Respuesta de liquidación", &format!("{number} · {} actos rechazados", rejected.len()));
        Ok(expected)
    }

    /// Anula una liquidación: queda guardada como anulada, con fecha y
    /// motivo, y sus actos vuelven a estar pendientes de liquidar. Con algo
    /// cobrado, no se anula. Su número no se vuelve a usar.
    pub fn cancel_claim(&mut self, id: u32, reason: &str, today: NaiveDate) -> Result<(), String> {
        let c = self.claims.iter_mut().find(|c| c.id == id).ok_or("La liquidación ya no existe.")?;
        if c.cancelled.is_some() {
            return Err("Esa liquidación ya está anulada.".into());
        }
        if c.paid > 0.005 {
            return Err("Tiene cobros apuntados: no se puede anular.".into());
        }
        c.cancelled = Some((today, reason.trim().to_string()));
        let number = c.number.clone();
        self.audit("Liquidación anulada", &format!("{number} · {}", reason.trim()));
        Ok(())
    }

    /// Pendiente de liquidar (todo el histórico) y pendiente de cobro, por compañía.
    pub fn insurer_summary(&self) -> Vec<(u32, String, usize, f64, f64)> {
        let far = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
        self.tariffs
            .iter()
            .filter(|t| t.is_insurer && !t.inactive)
            .map(|t| {
                let pend = self.claimable(t.id, far, end);
                let owed: f64 = self.claims.iter().filter(|c| c.tariff_id == t.id).map(|c| c.pending()).sum();
                (t.id, t.name.clone(), pend.len(), round2(pend.iter().map(|l| l.fee).sum()), round2(owed))
            })
            .collect()
    }

    pub fn claim_html(&self, id: u32) -> String {
        let Some(c) = self.claims.iter().find(|c| c.id == id) else { return String::new() };
        let t = self.tariff(c.tariff_id);
        let rows: String = c
            .lines
            .iter()
            .map(|l| {
                format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class='n'>{}</td></tr>",
                    fmt_date(l.date),
                    esc(&l.patient),
                    esc(&l.policy_no),
                    esc(&l.code),
                    esc(&l.description),
                    esc(&l.teeth),
                    esc(&l.auth_no),
                    esc(&money(l.fee))
                )
            })
            .collect();
        let cfg = &self.config;
        format!(
            "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>{num}</title><style>body{{font-family:'Open Sans',Arial,sans-serif;max-width:980px;margin:30px auto;color:#1e293b}}.head{{display:flex;justify-content:space-between;border-bottom:3px solid #0e9688;padding-bottom:10px}}h1{{font-size:21px;margin:18px 0 4px}}.muted{{color:#64748b;font-size:13px}}table{{width:100%;border-collapse:collapse;margin-top:12px}}th{{background:#eef3f8;color:#475569;font-size:11px;text-transform:uppercase;text-align:left;padding:6px}}td{{padding:6px;border-bottom:1px solid #e2e8f0;font-size:12.5px}}.n{{text-align:right}}.tot td{{font-weight:700;font-size:15px;border:none}}@media print{{body{{margin:10mm}}}}</style></head><body><div class='head'><div><b>{clinic}</b><br><span class='muted'>{addr} · NIF {nif}</span></div><div class='muted' style='text-align:right'>{num}<br>{date}</div></div><h1>Liquidación de actos a {ins}</h1><p class='muted'>Periodo del {from} al {to} · {n} actos</p><table><tr><th>Fecha</th><th>Paciente</th><th>Póliza</th><th>Código</th><th>Acto</th><th>Piezas</th><th>Autorización</th><th class='n'>Importe</th></tr>{rows}<tr class='tot'><td colspan='7' class='n'>Total a liquidar</td><td class='n'>{total}</td></tr></table></body></html>",
            num = esc(&c.number),
            clinic = esc(&cfg.name),
            addr = esc(&cfg.address),
            nif = esc(&cfg.nif),
            date = fmt_date(c.date),
            ins = esc(&t.map(|t| t.name.clone()).unwrap_or_default()),
            from = fmt_date(c.from),
            to = fmt_date(c.to),
            n = c.lines.len(),
            total = esc(&money(c.total)),
        )
    }

    /// Fichero CSV (separado por «;») para subir al portal de la compañía.
    pub fn claim_csv(&self, id: u32) -> String {
        let Some(c) = self.claims.iter().find(|c| c.id == id) else { return String::new() };
        let mut s = String::from("Liquidacion;Fecha;Paciente;Poliza;Codigo;Acto;Piezas;Autorizacion;Importe\n");
        for l in &c.lines {
            let clean = |x: &str| crate::model::sheet_cell(x, ';');
            let fee = format!("{:.2}", l.fee).replace('.', ",");
            s.push_str(&format!("{};{};{};{};{};{};{};{};{fee}\n", c.number, l.date.format("%d/%m/%Y"), clean(&l.patient), clean(&l.policy_no), l.code, clean(&l.description), l.teeth, clean(&l.auth_no)));
        }
        s
    }
}

/// Baremos de ejemplo: lo que pagan las compañías de demostración por acto.
pub fn sample_insurer(db: &mut Db) {
    let fees: [(u32, &str, f64); 7] = [(2, "0101", 12.0), (2, "0102", 10.0), (2, "0201", 18.0), (2, "0501", 25.0), (2, "0103", 15.0), (3, "0101", 10.0), (3, "0201", 20.0)];
    for (tid, code, fee) in fees {
        if let Some(t) = db.tariffs.iter_mut().find(|t| t.id == tid)
            && let Some(p) = t.prices.iter_mut().find(|p| p.code == code)
        {
            p.insurer_fee = fee;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
    }

    /// La compañía rechaza un acto (T-F6-04): se guarda su respuesta, deja de
    /// contar en lo que paga y el acto se puede liquidar de nuevo, una vez.
    #[test]
    fn a_rejected_act_leaves_the_claim_and_can_be_claimed_again_once() {
        let mut db = Db::sample(d());
        let from = d() - chrono::Duration::days(365);
        let id = db.create_claim(2, from, d(), d()).unwrap();
        let c = db.claims.iter().find(|c| c.id == id).unwrap().clone();
        assert!(c.lines.len() >= 2, "hacen falta dos actos");
        let (bad, fee) = (c.lines[0].entry_id, c.lines[0].fee);
        let now = d().and_hms_opt(10, 0, 0).unwrap();
        assert!(db.claim_answer(id, &[(bad, "Sin autorización".into())], "", now).is_err(), "primero, enviada");
        assert!(db.claim_sent(id, " ", "", now).is_err());
        db.claim_sent(id, "Portal de la compañía", "LOTE-7", now).unwrap();
        assert!(db.claim_answer(id, &[(bad, " ".into())], "", now).is_err(), "con su motivo");
        assert!(db.claim_answer(id, &[(99_999, "x".into())], "", now).is_err(), "de esta liquidación");
        let expected = db.claim_answer(id, &[(bad, "Sin autorización".into())], "{\"rechazos\":1}", now).unwrap();
        assert_eq!(expected, round2(c.total - fee));
        let c2 = db.claims.iter().find(|x| x.id == id).unwrap().clone();
        assert_eq!((c2.answers.len(), c2.answers[0].raw.as_str()), (1, "{\"rechazos\":1}"), "la respuesta, tal cual");
        assert_eq!(c2.pending(), expected, "lo que debe la compañía, sin el rechazado");
        // el rechazado se liquida de nuevo (corregido), una sola vez
        let again = db.claimable(2, from, d());
        assert_eq!(again.iter().map(|l| l.entry_id).collect::<Vec<_>>(), vec![bad]);
        let id2 = db.create_claim(2, from, d(), d()).unwrap();
        assert!(db.claimable(2, from, d()).is_empty(), "ya no queda pendiente");
        assert!(db.claims.iter().find(|x| x.id == id2).unwrap().lines.iter().all(|l| l.entry_id == bad));
        // pagada la primera por lo esperado: cobrada
        db.claim_paid(id, expected, d());
        assert_eq!(db.claims.iter().find(|x| x.id == id).unwrap().state(), "Cobrada");
        // no se rechaza algo que deje lo cobrado por encima de lo que queda
        let other = c.lines[1].entry_id;
        db.claim_sent(id2, "Correo", "", now).unwrap();
        assert!(db.claim_answer(id, &[(other, "x".into())], "", now).unwrap_err().contains("cobrado"));
    }

    #[test]
    fn claim_collects_insured_acts_once() {
        let mut db = Db::sample(d());
        let from = d() - chrono::Duration::days(365);
        let pend = db.claimable(2, from, d());
        assert!(!pend.is_empty(), "no hay actos de la compañía de ejemplo");
        assert!(pend.iter().all(|l| l.fee > 0.0 && db.patient(l.patient_id).unwrap().tariff_id == 2));
        let id = db.create_claim(2, from, d(), d()).unwrap();
        let c = db.claims.iter().find(|c| c.id == id).unwrap().clone();
        assert_eq!(c.lines.len(), pend.len());
        assert_eq!(c.total, round2(pend.iter().map(|l| l.fee).sum()));
        // ya no quedan pendientes
        assert!(db.claimable(2, from, d()).is_empty());
        assert!(db.create_claim(2, from, d(), d()).is_none());
        db.claim_paid(id, c.total / 2.0, d());
        assert_eq!(db.claims[0].state(), "Cobro parcial");
        db.claim_paid(id, c.total, d());
        assert_eq!(db.claims[0].state(), "Cobrada");
        assert!(db.claim_html(id).contains(&c.number));
        assert_eq!(db.claim_csv(id).lines().count(), c.lines.len() + 1);
        // anularla devuelve los actos a pendientes
        db.claims.iter_mut().find(|c| c.id == id).unwrap().paid = 0.0;
        db.cancel_claim(id, "prueba", d()).unwrap();
        assert_eq!(db.claimable(2, from, d()).len(), pend.len());
    }

    #[test]
    fn acts_go_to_the_insurer_the_patient_had_when_they_were_done() {
        let mut db = Db::sample(d());
        let from = d() - chrono::Duration::days(365);
        let pend = db.claimable(2, from, d());
        let pid = pend[0].patient_id;
        let mine = pend.iter().filter(|l| l.patient_id == pid).count();
        // el paciente se cambia a la tarifa privada: sus actos de antes siguen siendo de la mutua
        db.patient_mut(pid).unwrap().tariff_id = 1;
        assert_eq!(db.claimable(2, from, d()).len(), pend.len());
        // un acto de después no es de la mutua
        let code = pend[0].code.clone();
        db.add_entry(pid, &code, vec![], vec![], TxState::Realizado, 1, d());
        assert_eq!(db.claimable(2, from, d()).len(), pend.len());
        // un acto previsto con la mutua y hecho ya con la tarifa privada, tampoco
        db.patient_mut(pid).unwrap().tariff_id = 2;
        let e = db.add_entry(pid, &code, vec![], vec![], TxState::Previsto, 1, d());
        db.patient_mut(pid).unwrap().tariff_id = 1;
        db.realize_entry(e, d(), 0);
        assert_eq!(db.claimable(2, from, d()).len(), pend.len());
        assert!(mine > 0);
    }

    #[test]
    fn claim_numbers_are_not_repeated_after_cancelling_one() {
        let mut db = Db::sample(d());
        let from = d() - chrono::Duration::days(365);
        let mid = d() - chrono::Duration::days(60);
        let a = db.create_claim(2, from, mid, d()).unwrap();
        let b = db.create_claim(2, mid + chrono::Duration::days(1), d(), d()).unwrap();
        db.cancel_claim(a, "prueba", d()).unwrap();
        db.add_entry(db.claimable(2, from, d())[0].patient_id, "0101", vec![], vec![], TxState::Realizado, 1, d());
        let c = db.create_claim(2, from, d(), d()).unwrap();
        let num = |id: u32| db.claims.iter().find(|x| x.id == id).unwrap().number.clone();
        assert_ne!(num(b), num(c));
    }

    /// Anular una liquidación no la borra: queda anulada, con fecha y motivo
    /// (sus actos vuelven a pendientes), no cuenta como pendiente de cobro ni
    /// como liquidado, y su número no se vuelve a usar aunque fuera la última.
    #[test]
    fn a_cancelled_claim_stays_on_record_and_its_number_is_not_reused() {
        let mut db = Db::sample(d());
        let from = d() - chrono::Duration::days(365);
        let pend = db.claimable(2, from, d()).len();
        let a = db.create_claim(2, from, d(), d()).unwrap();
        let number = db.claims.iter().find(|c| c.id == a).unwrap().number.clone();
        db.cancel_claim(a, "error en el periodo", d()).unwrap();
        let c = db.claims.iter().find(|c| c.id == a).expect("sigue guardada").clone();
        assert_eq!((c.state(), c.pending()), ("Anulada", 0.0));
        assert_eq!(c.cancelled, Some((d(), "error en el periodo".to_string())));
        assert_eq!(db.claimable(2, from, d()).len(), pend, "sus actos vuelven a pendientes");
        assert!(db.cancel_claim(a, "otra vez", d()).is_err());
        assert!(db.audit_log.iter().any(|x| x.action == "Liquidación anulada" && x.detail.contains(&number)));
        let b = db.create_claim(2, from, d(), d()).unwrap();
        assert_ne!(db.claims.iter().find(|c| c.id == b).unwrap().number, number, "otro número");
        // una con cobros no se anula
        db.claim_paid(b, 1.0, d());
        assert!(db.cancel_claim(b, "x", d()).is_err());
    }

    #[test]
    fn private_tariff_never_claims() {
        let db = Db::sample(d());
        assert!(db.claimable(1, d() - chrono::Duration::days(3650), d()).is_empty());
        assert_eq!(db.insurer_fee(1, "0101"), 0.0);
    }
}
