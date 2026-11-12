//! Bancos (cuentas, extractos Norma 43 / CSV, conciliación), fusión de
//! pacientes duplicados y registro de cambios (auditoría).

use crate::model::*;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BankAccount {
    pub id: u32,
    pub name: String,
    pub iban: String,
    pub opening_balance: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Match {
    Payments(Vec<u32>),
    Expense(u32),
    Remittance(u32),
    Other(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BankMove {
    pub id: u32,
    pub account_id: u32,
    pub date: NaiveDate,
    pub amount: f64,
    pub concept: String,
    pub reference: String,
    pub matched: Option<Match>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEntry {
    pub when: NaiveDateTime,
    pub user: String,
    pub action: String,
    pub detail: String,
}

/// Movimiento leído de un extracto (antes de guardarlo).
#[derive(Clone, Debug, PartialEq)]
pub struct RawMove {
    pub date: NaiveDate,
    pub amount: f64,
    pub concept: String,
    pub reference: String,
}

fn yymmdd(s: &str) -> Option<NaiveDate> {
    if s.len() != 6 {
        return None;
    }
    let y: i32 = s[0..2].parse().ok()?;
    let m: u32 = s[2..4].parse().ok()?;
    let d: u32 = s[4..6].parse().ok()?;
    NaiveDate::from_ymd_opt(2000 + y, m, d)
}

/// Extracto bancario en formato AEB Norma 43 (registros 22 y 23).
pub fn parse_norma43(text: &str) -> Result<Vec<RawMove>, String> {
    let mut out: Vec<RawMove> = vec![];
    for line in text.lines() {
        let l: Vec<char> = line.trim_end_matches(['\r', '\n']).chars().collect();
        if l.len() < 2 {
            continue;
        }
        let rec: String = l[0..2].iter().collect();
        let sub = |a: usize, b: usize| -> String { l.get(a..b.min(l.len())).map(|x| x.iter().collect()).unwrap_or_default() };
        match rec.as_str() {
            "22" => {
                if l.len() < 42 {
                    continue;
                }
                let date = yymmdd(&sub(10, 16)).ok_or("Fecha no válida en un registro 22")?;
                let sign = if sub(27, 28) == "1" { -1.0 } else { 1.0 };
                let cents: i64 = sub(28, 42).trim().parse().map_err(|_| "Importe no válido en un registro 22")?;
                let reference = format!("{} {}", sub(52, 64).trim(), sub(64, 80).trim()).trim().to_string();
                out.push(RawMove { date, amount: sign * cents as f64 / 100.0, concept: String::new(), reference });
            }
            "23" => {
                if let Some(last) = out.last_mut() {
                    let c = format!("{} {}", sub(4, 42).trim(), sub(42, 80).trim()).trim().to_string();
                    if !c.is_empty() {
                        if !last.concept.is_empty() {
                            last.concept.push(' ');
                        }
                        last.concept.push_str(&c);
                    }
                }
            }
            _ => {}
        }
    }
    if out.is_empty() {
        return Err("No se han encontrado movimientos (registros 22) en el fichero Norma 43.".into());
    }
    for m in out.iter_mut() {
        if m.concept.is_empty() {
            m.concept = m.reference.clone();
        }
    }
    Ok(out)
}

/// Extracto en Excel/CSV con columnas de fecha, concepto e importe
/// (o cargo/abono por separado).
pub fn parse_bank_table(sheet: &crate::extra::Sheet) -> Result<Vec<RawMove>, String> {
    let h: Vec<String> = sheet.headers.iter().map(|x| fold(x)).collect();
    let find = |keys: &[&str]| h.iter().position(|c| keys.iter().any(|k| c.contains(k)));
    let fd = find(&["fecha", "f. oper", "date"]).ok_or("Falta la columna de fecha")?;
    let fc = find(&["concepto", "descrip", "movimiento", "detalle"]).ok_or("Falta la columna de concepto")?;
    let fi = find(&["importe", "cantidad", "amount"]);
    let fcargo = find(&["cargo", "debe"]);
    let fabono = find(&["abono", "haber"]);
    let mut out = vec![];
    for r in &sheet.rows {
        let Some(date) = r.get(fd).and_then(|s| parse_any_date(s)) else { continue };
        let amount = match fi {
            Some(i) => r.get(i).and_then(|s| crate::extra::parse_money(s)),
            None => {
                let c = fcargo.and_then(|i| r.get(i)).and_then(|s| crate::extra::parse_money(s)).unwrap_or(0.0);
                let a = fabono.and_then(|i| r.get(i)).and_then(|s| crate::extra::parse_money(s)).unwrap_or(0.0);
                Some(a - c.abs())
            }
        };
        let Some(amount) = amount else { continue };
        out.push(RawMove { date, amount, concept: r.get(fc).cloned().unwrap_or_default(), reference: String::new() });
    }
    if out.is_empty() {
        return Err("No se ha encontrado ningún movimiento con fecha e importe.".into());
    }
    Ok(out)
}

fn parse_any_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    for f in ["%d/%m/%Y", "%d-%m-%Y", "%Y-%m-%d", "%d/%m/%y", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(d) = NaiveDate::parse_from_str(s.get(..10.min(s.len())).unwrap_or(s), f) {
            return Some(d);
        }
        if let Ok(d) = NaiveDate::parse_from_str(s, f) {
            return Some(d);
        }
    }
    s.parse::<f64>().ok().filter(|n| (1000.0..80000.0).contains(n)).and_then(|n| NaiveDate::from_ymd_opt(1899, 12, 30).map(|b| b + Duration::days(n as i64)))
}

impl Db {
    pub fn audit(&mut self, action: &str, detail: &str) {
        let user = self.config.current_user.clone();
        self.audit_log.push(AuditEntry { when: now(), user, action: action.into(), detail: detail.into() });
        if self.audit_log.len() > 5000 {
            self.audit_log.remove(0);
        }
    }

    /// Añade movimientos al extracto de una cuenta, sin repetir los ya importados.
    pub fn add_bank_moves(&mut self, account_id: u32, moves: &[RawMove]) -> (usize, usize) {
        let mut added = 0;
        let mut dup = 0;
        for m in moves {
            let exists = self.bank_moves.iter().any(|b| b.account_id == account_id && b.date == m.date && (b.amount - m.amount).abs() < 0.005 && fold(&b.concept) == fold(&m.concept));
            if exists {
                dup += 1;
                continue;
            }
            let id = self.new_id();
            self.bank_moves.push(BankMove { id, account_id, date: m.date, amount: round2(m.amount), concept: m.concept.clone(), reference: m.reference.clone(), matched: None });
            added += 1;
        }
        self.audit("Importar extracto", &format!("{added} movimientos nuevos, {dup} repetidos"));
        (added, dup)
    }

    fn payment_used(&self, pid: u32) -> bool {
        self.bank_moves.iter().any(|m| matches!(&m.matched, Some(Match::Payments(v)) if v.contains(&pid)))
    }

    /// Concilia automáticamente: cobros por transferencia/Bizum del mismo
    /// importe (±3 días), liquidaciones de tarjeta (suma del día), remesas SEPA
    /// por su total y gastos por importe (los marca como pagados).
    pub fn auto_reconcile(&mut self) -> usize {
        let mut n = 0;
        let ids: Vec<u32> = self.bank_moves.iter().filter(|m| m.matched.is_none()).map(|m| m.id).collect();
        for id in ids {
            let m = self.bank_moves.iter().find(|x| x.id == id).cloned().unwrap();
            let near = |d: NaiveDate, days: i64| (m.date - d).num_days().abs() <= days;
            let mut found: Option<Match> = None;
            if m.amount > 0.0 {
                // remesa SEPA
                if let Some(r) = self.remittances.iter().find(|r| (r.total() - m.amount).abs() < 0.005 && near(r.collection, 5) && !self.bank_moves.iter().any(|b| b.matched == Some(Match::Remittance(r.id)))) {
                    found = Some(Match::Remittance(r.id));
                }
                // cobro individual
                if found.is_none()
                    && let Some(p) = self.payments.iter().find(|p| matches!(p.method.as_str(), "Transferencia" | "Bizum" | "Financiación") && (p.amount - m.amount).abs() < 0.005 && near(p.date.date(), 3) && !self.payment_used(p.id))
                {
                    found = Some(Match::Payments(vec![p.id]));
                }
                // liquidación del datáfono: suma de cobros con tarjeta de un día
                if found.is_none() {
                    for back in 0..=3 {
                        let day = m.date - Duration::days(back);
                        let cards: Vec<&Payment> = self.payments.iter().filter(|p| p.method == "Tarjeta" && p.date.date() == day && !self.payment_used(p.id)).collect();
                        let sum: f64 = cards.iter().map(|p| p.amount).sum();
                        if !cards.is_empty() && (sum - m.amount).abs() < 0.005 {
                            found = Some(Match::Payments(cards.iter().map(|p| p.id).collect()));
                            break;
                        }
                    }
                }
            } else if let Some(e) = self.expenses.iter().find(|e| (e.total() + m.amount).abs() < 0.005 && near(e.due.unwrap_or(e.date), 40) && !self.bank_moves.iter().any(|b| b.matched == Some(Match::Expense(e.id)))) {
                found = Some(Match::Expense(e.id));
            }
            if let Some(f) = found {
                if let Match::Expense(eid) = &f && let Some(e) = self.expenses.iter_mut().find(|e| e.id == *eid) {
                    e.paid = true;
                }
                if let Match::Remittance(rid) = &f {
                    let _ = self.remittance_collected(*rid);
                }
                if let Some(x) = self.bank_moves.iter_mut().find(|x| x.id == id) {
                    x.matched = Some(f);
                }
                n += 1;
            }
        }
        if n > 0 {
            self.audit("Conciliación automática", &format!("{n} movimientos conciliados"));
        }
        n
    }

    pub fn bank_balance(&self, account_id: u32) -> f64 {
        let open = self.bank_accounts.iter().find(|a| a.id == account_id).map(|a| a.opening_balance).unwrap_or(0.0);
        round2(open + self.bank_moves.iter().filter(|m| m.account_id == account_id).map(|m| m.amount).sum::<f64>())
    }

    pub fn describe_match(&self, m: &Match) -> String {
        match m {
            Match::Payments(v) if v.len() == 1 => self.payments.iter().find(|p| p.id == v[0]).map(|p| format!("Cobro {} · {}", p.receipt_no, self.patient_name(p.patient_id))).unwrap_or_default(),
            Match::Payments(v) => format!("Liquidación de tarjeta ({} cobros)", v.len()),
            Match::Expense(id) => self.expenses.iter().find(|e| e.id == *id).map(|e| format!("Gasto · {} · {}", e.supplier, e.concept)).unwrap_or_default(),
            Match::Remittance(id) => format!("Remesa SEPA #{id}"),
            Match::Other(s) => s.clone(),
        }
    }

    // ── fusión de duplicados ──

    /// Parejas de pacientes que parecen la misma persona.
    pub fn find_duplicates(&self) -> Vec<(u32, u32, String)> {
        let mut out = vec![];
        let ps = &self.patients;
        for i in 0..ps.len() {
            for j in i + 1..ps.len() {
                let (a, b) = (&ps[i], &ps[j]);
                let reason = if !a.nif.is_empty() && a.nif == b.nif {
                    Some("mismo NIF")
                } else if fold(&a.full_name()) == fold(&b.full_name()) && (a.birth == b.birth || a.birth.is_none() || b.birth.is_none()) {
                    Some("mismo nombre")
                } else if !a.mobile.is_empty() && a.mobile.replace(' ', "") == b.mobile.replace(' ', "") && fold(&a.last_name) == fold(&b.last_name) {
                    Some("mismo móvil y apellidos")
                } else {
                    None
                };
                if let Some(r) = reason {
                    out.push((a.id, b.id, r.to_string()));
                }
            }
        }
        out
    }

    /// Une `remove` en `keep`: pasa todo su historial y completa los datos vacíos.
    pub fn merge_patients(&mut self, keep: u32, remove: u32) -> Result<(), String> {
        if keep == remove {
            return Err("Elija dos pacientes distintos.".into());
        }
        let r = self.patient(remove).cloned().ok_or("Paciente a fusionar no encontrado")?;
        let k = self.patient_mut(keep).ok_or("Paciente a conservar no encontrado")?;
        let fill = |dst: &mut String, v: &String| {
            if dst.trim().is_empty() {
                *dst = v.clone();
            }
        };
        fill(&mut k.nif, &r.nif);
        fill(&mut k.mobile, &r.mobile);
        fill(&mut k.phone, &r.phone);
        fill(&mut k.email, &r.email);
        fill(&mut k.address, &r.address);
        fill(&mut k.zip, &r.zip);
        fill(&mut k.city, &r.city);
        fill(&mut k.province, &r.province);
        fill(&mut k.iban, &r.iban);
        if k.birth.is_none() {
            k.birth = r.birth;
        }
        for (dst, src) in [(&mut k.alerts, &r.alerts), (&mut k.allergies, &r.allergies), (&mut k.notes, &r.notes), (&mut k.background, &r.background)] {
            if !src.trim().is_empty() && !dst.contains(src.as_str()) {
                if !dst.is_empty() {
                    dst.push_str(". ");
                }
                dst.push_str(src);
            }
        }
        k.accepts_gdpr |= r.accepts_gdpr;
        k.sepa_signed |= r.sepa_signed;
        let kname = k.full_name();
        macro_rules! move_pid {
            ($($v:ident),*) => { $( for x in self.$v.iter_mut() { if x.patient_id == remove { x.patient_id = keep; } } )* };
        }
        move_pid!(entries, budgets, payments, recalls, waitlist, invoices, perio, images, messages, documents, lab_works, prescriptions, ortho);
        for a in self.appointments.iter_mut() {
            if a.patient_id == Some(remove) {
                a.patient_id = Some(keep);
            }
        }
        for rm in self.remittances.iter_mut() {
            for it in rm.items.iter_mut() {
                if it.patient_id == remove {
                    it.patient_id = keep;
                }
            }
        }
        self.patients.retain(|p| p.id != remove);
        self.audit("Fusión de pacientes", &format!("{} (Nº {}) unido a {}", r.full_name(), r.history_no, kname));
        Ok(())
    }
}

pub fn sample_bank(db: &mut Db, today: NaiveDate) {
    sample_centers(db, today);
    *db.bank_accounts = vec![BankAccount { id: 1, name: "Cuenta principal".into(), iban: "ES9121000418450200051332".into(), opening_balance: 12500.0 }];
    // extracto de ejemplo: una transferencia de paciente, la liquidación de tarjeta y un recibo
    let card_day = today - Duration::days(1);
    let mut moves = vec![];
    if let Some(p) = db.payments.iter().find(|p| p.method == "Transferencia") {
        moves.push(RawMove { date: p.date.date() + Duration::days(1), amount: p.amount, concept: format!("TRANSFERENCIA DE {}", fold(&db.patient_name(p.patient_id)).to_uppercase()), reference: String::new() });
    }
    moves.push(RawMove { date: card_day, amount: -94.25, concept: "RECIBO COMPAÑIA ELECTRICA".into(), reference: String::new() });
    moves.push(RawMove { date: today - Duration::days(2), amount: 75.0, concept: "INGRESO EFECTIVO OFICINA".into(), reference: String::new() });
    let id_save = db.audit_log.len();
    db.add_bank_moves(1, &moves);
    db.auto_reconcile();
    db.audit_log.truncate(id_save);
}

fn sample_centers(db: &mut Db, today: NaiveDate) {
    *db.centers = vec![
        Center { id: 1, name: "Centro (Madrid)".into(), address: db.config.address.clone(), phone: db.config.phone.clone(), color: [14, 150, 136], active: true },
        Center { id: 2, name: "Clínica Norte (Alcobendas)".into(), address: "Av. Ejemplo 20, 28100 Alcobendas".into(), phone: "91 000 00 01".into(), color: [56, 132, 214], active: true },
    ];
    db.boxes.push(Boxroom { id: 5, name: "Gabinete Norte".into(), center_id: 2 });
    for d in db.collaborators.iter_mut() {
        d.centers = if d.id == 2 { vec![1, 2] } else { vec![1] };
    }
    for (h, m, pid, reason, code) in [(18u32, 0u32, 104u32, "Revisión", "0102"), (18, 45, 115, "Valoración implante", "0101")] {
        let id = db.new_id();
        let start = chrono::NaiveTime::from_hms_opt(h, m, 0).unwrap();
        db.appointments.push(Appointment { id, date: today, start, minutes: 40, patient_id: Some(pid), free_name: String::new(), phone: String::new(), doctor_id: 2, box_id: 5, reason: reason.into(), treatment_code: code.into(), state: ApptState::Planificada, notes: String::new(), arrived_at: None, seen_at: None, finished_at: None, reminder_sent: true, first_visit: false, center_id: 2 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()
    }

    fn n43_line22(date: &str, debe: bool, cents: u64, r1: &str) -> String {
        format!("22{:4}{:4}{}{}{:2}{:3}{}{:014}{:010}{:<12}{:<16}", "", "0001", date, date, "02", "000", if debe { "1" } else { "2" }, cents, 0, r1, "")
    }

    #[test]
    fn norma43_parsing() {
        let txt = format!("11...cabecera\n{}\n2301TRANSFERENCIA DE ANA GARCIA LOPEZ        \n{}\n2301RECIBO LUZ\n33...\n88...\n", n43_line22("261103", false, 30000, "REF1"), n43_line22("261104", true, 9425, "REF2"));
        let v = parse_norma43(&txt).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].amount, 300.0);
        assert_eq!(v[0].date, NaiveDate::from_ymd_opt(2026, 11, 3).unwrap());
        assert!(v[0].concept.contains("ANA GARCIA"));
        assert_eq!(v[1].amount, -94.25);
        assert!(parse_norma43("hola").is_err());
    }

    #[test]
    fn reconcile_transfer_cards_and_expense() {
        let mut db = Db::sample(d());
        // liquidación de tarjeta del día: suma de cobros con tarjeta de hoy
        let card_sum: f64 = db.payments.iter().filter(|p| p.method == "Tarjeta" && p.date.date() == d()).map(|p| p.amount).sum();
        let exp = db.expenses.iter().find(|e| !e.paid).cloned().unwrap();
        db.add_bank_moves(1, &[RawMove { date: d() + Duration::days(1), amount: card_sum, concept: "LIQUIDACION TPV".into(), reference: String::new() }, RawMove { date: exp.date, amount: -exp.total(), concept: "PAGO PROVEEDOR".into(), reference: String::new() }]);
        let n = db.auto_reconcile();
        assert!(n >= 2, "debe conciliar la liquidación del TPV y el gasto (fueron {n})");
        assert!(db.bank_moves.iter().any(|m| m.concept.starts_with("TRANSFERENCIA") && matches!(m.matched, Some(Match::Payments(_)))), "la transferencia de ejemplo queda conciliada");
        assert!(db.expenses.iter().find(|e| e.id == exp.id).unwrap().paid);
        // reimportar no duplica
        let (added, dup) = db.add_bank_moves(1, &[RawMove { date: d() + Duration::days(1), amount: card_sum, concept: "LIQUIDACION TPV".into(), reference: String::new() }]);
        assert_eq!((added, dup), (0, 1));
        assert_eq!(db.auto_reconcile(), 0);
    }

    #[test]
    fn centers_separate_agenda_and_cash() {
        let mut db = Db::sample(d());
        assert!(db.multi_center());
        assert!(db.works_in(2, 2) && !db.works_in(1, 2));
        db.config.current_center = 1;
        assert!(db.open_cash().is_some());
        db.config.current_center = 2;
        assert!(db.open_cash().is_none(), "la caja del centro 1 no es la del centro 2");
        let pid = db.add_payment(100, 10.0, "Efectivo", "prueba");
        let p = db.payments.iter().find(|p| p.id == pid).unwrap();
        assert_eq!(p.center_id, 2);
        assert!(p.cash_session.is_none());
        let t = chrono::NaiveTime::from_hms_opt(18, 10, 0).unwrap();
        assert!(!db.is_free(2, d(), t, t + Duration::minutes(20), None));
    }

    #[test]
    fn merge_duplicate_patients() {
        let mut db = Db::sample(d());
        let mut dupe = db.patient(100).unwrap().clone();
        dupe.id = 9999;
        dupe.history_no = 9999;
        dupe.mobile = String::new();
        dupe.allergies = "Ibuprofeno".into();
        db.patients.push(dupe);
        db.add_entry(9999, "0102", vec![], vec![], TxState::Realizado, 1, d());
        assert!(db.find_duplicates().iter().any(|(a, b, _)| (*a == 100 && *b == 9999) || (*a == 9999 && *b == 100)));
        let before = db.entries.iter().filter(|e| e.patient_id == 100).count();
        db.merge_patients(100, 9999).unwrap();
        assert!(db.patient(9999).is_none());
        assert_eq!(db.entries.iter().filter(|e| e.patient_id == 100).count(), before + 1);
        assert!(db.patient(100).unwrap().allergies.contains("Ibuprofeno"));
        assert!(db.audit_log.iter().any(|a| a.action == "Fusión de pacientes"));
    }
}
