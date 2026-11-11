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
}

impl Claim {
    pub fn state(&self) -> &'static str {
        if self.paid + 0.005 >= self.total {
            "Cobrada"
        } else if self.paid > 0.005 {
            "Cobro parcial"
        } else {
            "Pendiente de cobro"
        }
    }
    pub fn pending(&self) -> f64 {
        round2((self.total - self.paid).max(0.0))
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

    fn claimed(&self, entry_id: u32) -> bool {
        self.claims.iter().any(|c| c.lines.iter().any(|l| l.entry_id == entry_id))
    }

    /// Actos realizados en el periodo pendientes de liquidar a la compañía.
    pub fn claimable(&self, tariff_id: u32, from: NaiveDate, to: NaiveDate) -> Vec<ClaimLine> {
        let mut v: Vec<ClaimLine> = self
            .entries
            .iter()
            .filter(|e| e.state == TxState::Realizado && e.date >= from && e.date <= to && !self.claimed(e.id))
            .filter_map(|e| {
                let p = self.patient(e.patient_id)?;
                if p.tariff_id != tariff_id {
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
        let n = self.claims.iter().filter(|c| c.date.year() == year).count() + 1;
        let id = self.new_id();
        let total = round2(lines.iter().map(|l| l.fee).sum());
        self.claims.push(Claim { id, number: format!("LIQ-{year}-{n:04}"), tariff_id, date: today, from, to, lines, total, paid: 0.0, paid_on: None, notes: String::new() });
        Some(id)
    }

    pub fn claim_paid(&mut self, id: u32, amount: f64, date: NaiveDate) {
        if let Some(c) = self.claims.iter_mut().find(|c| c.id == id) {
            c.paid = round2((c.paid + amount).min(c.total));
            c.paid_on = Some(date);
        }
    }

    /// Anula una liquidación: sus actos vuelven a estar pendientes.
    pub fn delete_claim(&mut self, id: u32) {
        self.claims.retain(|c| c.id != id);
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
            .map(|l| format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class='n'>{}</td></tr>", fmt_date(l.date), esc(&l.patient), esc(&l.policy_no), esc(&l.code), esc(&l.description), esc(&l.teeth), esc(&l.auth_no), esc(&money(l.fee))))
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
            let clean = |x: &str| x.replace([';', '\n'], " ");
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
        if let Some(t) = db.tariffs.iter_mut().find(|t| t.id == tid) {
            if let Some(p) = t.prices.iter_mut().find(|p| p.code == code) {
                p.insurer_fee = fee;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
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
        db.delete_claim(id);
        assert_eq!(db.claimable(2, from, d()).len(), pend.len());
    }

    #[test]
    fn private_tariff_never_claims() {
        let db = Db::sample(d());
        assert!(db.claimable(1, d() - chrono::Duration::days(3650), d()).is_empty());
        assert_eq!(db.insurer_fee(1, "0101"), 0.0);
    }
}
