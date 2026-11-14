//! Bancos (cuentas, extractos Norma 43 / CSV, conciliación), fusión de
//! pacientes duplicados y registro de cambios (auditoría).

use crate::model::*;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
    /// La conciliación marcó como pagado el gasto o como cobrada la remesa: al
    /// deshacerla, eso también se deshace.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub set_by_match: bool,
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

/// Lo que la conciliación necesita saber, calculado una vez (y no por cada
/// movimiento recorriendo todos los cobros):
/// - lo ya conciliado con algún movimiento;
/// - los cobros anulados con sus anulaciones (no pasan por el banco);
/// - los cobros por importe y los de tarjeta por día.
#[derive(Default)]
struct Used<'a> {
    payments: HashSet<u32>,
    expenses: HashSet<u32>,
    remittances: HashSet<u32>,
    voided: HashSet<u32>,
    by_cents: HashMap<i64, Vec<&'a Payment>>,
    cards_by_day: HashMap<NaiveDate, Vec<&'a Payment>>,
}

fn cents(amount: f64) -> i64 {
    (amount * 100.0).round() as i64
}

/// Cada cobro, gasto o remesa que una conciliación usa.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Target {
    Payment(u32),
    Expense(u32),
    Remittance(u32),
}

fn targets(m: &Match) -> Vec<Target> {
    match m {
        Match::Payments(v) => v.iter().map(|p| Target::Payment(*p)).collect(),
        Match::Expense(e) => vec![Target::Expense(*e)],
        Match::Remittance(r) => vec![Target::Remittance(*r)],
        Match::Other(_) => vec![],
    }
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

/// Una cuenta de un extracto Norma 43 (registro 11), con sus movimientos.
struct N43Account {
    /// Entidad, oficina y número de cuenta (vacío si el registro 11 no se lee).
    key: String,
    moves: Vec<RawMove>,
    /// Tiene su registro 33 de totales, y cuadra.
    closed: bool,
}

/// Extracto bancario en formato AEB Norma 43 (registros 11, 22, 23 y 33),
/// cuenta a cuenta. Si a una cuenta le falta su registro 33 de totales, o no
/// cuadra con sus movimientos, el fichero está cortado o dañado y no se
/// importa nada.
fn parse_n43_accounts(text: &str) -> Result<Vec<N43Account>, String> {
    let mut accounts: Vec<N43Account> = vec![];
    // apuntes y céntimos al debe y al haber de la cuenta en curso
    let mut count = (0u32, 0i64, 0u32, 0i64);
    for (no, line) in text.lines().enumerate() {
        let l: Vec<char> = line.trim_end_matches(['\r', '\n']).chars().collect();
        if l.len() < 2 {
            continue;
        }
        let rec: String = l[0..2].iter().collect();
        let sub = |a: usize, b: usize| -> String { l.get(a..b.min(l.len())).map(|x| x.iter().collect()).unwrap_or_default() };
        match rec.as_str() {
            "11" => {
                accounts.push(N43Account { key: if l.len() >= 20 { sub(2, 20) } else { String::new() }, moves: vec![], closed: false });
                count = (0, 0, 0, 0);
            }
            "22" => {
                if l.len() < 42 {
                    return Err(format!("El movimiento de la línea {} está cortado: el fichero está incompleto o dañado. No se ha importado nada.", no + 1));
                }
                let date = yymmdd(&sub(10, 16)).ok_or("Fecha no válida en un registro 22")?;
                let debit = sub(27, 28) == "1";
                let cents: i64 = sub(28, 42).trim().parse().map_err(|_| "Importe no válido en un registro 22")?;
                if debit {
                    (count.0, count.1) = (count.0 + 1, count.1 + cents);
                } else {
                    (count.2, count.3) = (count.2 + 1, count.3 + cents);
                }
                let reference = format!("{} {}", sub(52, 64).trim(), sub(64, 80).trim()).trim().to_string();
                if accounts.is_empty() {
                    accounts.push(N43Account { key: String::new(), moves: vec![], closed: false });
                }
                let amount = if debit { -(cents as f64) } else { cents as f64 } / 100.0;
                accounts.last_mut().unwrap().moves.push(RawMove { date, amount, concept: String::new(), reference });
            }
            "23" => {
                if let Some(last) = accounts.last_mut().and_then(|a| a.moves.last_mut()) {
                    let c = format!("{} {}", sub(4, 42).trim(), sub(42, 80).trim()).trim().to_string();
                    if !c.is_empty() {
                        if !last.concept.is_empty() {
                            last.concept.push(' ');
                        }
                        last.concept.push_str(&c);
                    }
                }
            }
            "33" if l.len() >= 58 => {
                let n = |a, b| sub(a, b).trim().parse::<i64>().ok();
                let (Some(nd), Some(td), Some(nh), Some(th)) = (n(20, 25), n(25, 39), n(39, 44), n(44, 58)) else {
                    return Err(format!("Los totales de la línea {} no se pueden leer: el fichero está dañado. No se ha importado nada.", no + 1));
                };
                if nd != count.0 as i64 || td != count.1 || nh != count.2 as i64 || th != count.3 {
                    return Err(format!(
                        "Los totales de la cuenta {} no cuadran con sus movimientos ({} cargos y {} abonos en el fichero; el banco indica {nd} y {nh}): el fichero está incompleto o dañado. No se ha importado nada.",
                        sub(2, 20),
                        count.0,
                        count.2
                    ));
                }
                if let Some(a) = accounts.last_mut() {
                    a.closed = true;
                }
            }
            _ => {}
        }
    }
    for a in accounts.iter_mut() {
        for m in a.moves.iter_mut() {
            if m.concept.is_empty() {
                m.concept = m.reference.clone();
            }
        }
    }
    // con registro 11 (la cuenta), el 33 es obligatorio: sin él, el fichero se cortó
    if let Some(a) = accounts.iter().find(|a| !a.key.is_empty() && !a.closed) {
        return Err(format!("Al extracto de la cuenta {} le faltan los totales del final (registro 33): el fichero está incompleto. No se ha importado nada.", a.key));
    }
    accounts.retain(|a| !a.moves.is_empty());
    if accounts.is_empty() {
        return Err("No se han encontrado movimientos (registros 22) en el fichero Norma 43.".into());
    }
    Ok(accounts)
}

/// Movimientos de la cuenta con este IBAN en un extracto Norma 43, que puede
/// traer varias cuentas. Sin IBAN (o con uno que no es español), solo vale un
/// fichero de una cuenta.
pub fn parse_norma43_for(text: &str, iban: &str) -> Result<Vec<RawMove>, String> {
    let accounts = parse_n43_accounts(text)?;
    let iban: String = iban.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
    // ES·DC·entidad(4)·oficina(4)·DC(2)·cuenta(10)
    let key = (iban.len() == 24 && iban.starts_with("ES")).then(|| format!("{}{}", &iban[4..12], &iban[14..24]));
    if accounts.len() == 1 && (accounts[0].key.is_empty() || key.is_none()) {
        return Ok(accounts.into_iter().next().map(|a| a.moves).unwrap_or_default());
    }
    let known: Vec<String> = accounts.iter().map(|a| a.key.clone()).collect();
    let Some(key) = key else {
        return Err(format!("El fichero trae {} cuentas ({}): escriba el IBAN de esta cuenta para importar solo la suya.", accounts.len(), known.join(", ")));
    };
    match accounts.into_iter().find(|a| a.key == key) {
        Some(a) => Ok(a.moves),
        None => Err(format!("La cuenta {} no está en el fichero (trae las cuentas {}).", crate::extra::iban_fmt(&iban), known.join(", "))),
    }
}

/// Extracto en Excel/CSV con columnas de fecha, concepto e importe (o cargo y
/// abono por separado, aunque se llamen «Importe cargo» e «Importe abono»).
/// Si alguna fila con fecha o importe no se puede leer, no se importa nada y
/// se dice cuál: un movimiento perdido descuadraría el saldo.
pub fn parse_bank_table(sheet: &crate::extra::Sheet) -> Result<Vec<RawMove>, String> {
    let h: Vec<String> = sheet.headers.iter().map(|x| fold(x)).collect();
    let find = |keys: &[&str]| h.iter().position(|c| keys.iter().any(|k| c.contains(k)));
    let fd = find(&["fecha", "f. oper", "f.oper", "f. valor", "f.valor", "date"]).ok_or("Falta la columna de fecha")?;
    let fc = find(&["concepto", "descrip", "movimiento", "detalle"]).ok_or("Falta la columna de concepto")?;
    let (fcargo, fabono) = (find(&["cargo", "debe"]), find(&["abono", "haber"]));
    // (una columna con signo) o (cargo, abono)
    let (one, cargo, abono) = match (fcargo, fabono, find(&["importe", "cantidad", "amount"])) {
        (Some(c), Some(a), _) if c != a => (None, Some(c), Some(a)),
        (_, _, Some(i)) => (Some(i), None, None),
        (None, None, None) => return Err("Falta la columna del importe (o las de cargo y abono)".into()),
        (c, a, None) => (None, c, a),
    };
    let mut out = vec![];
    let mut bad: Vec<String> = vec![];
    for (n, r) in sheet.rows.iter().enumerate() {
        // la fila 1 son los títulos
        let row = n + 2;
        let cell = |i: Option<usize>| i.and_then(|i| r.get(i)).map(|s| s.trim()).unwrap_or("");
        let read = |i: Option<usize>| -> Result<Option<f64>, String> {
            let t = cell(i);
            if t.is_empty() { Ok(None) } else { crate::extra::parse_money(t).map(Some).ok_or_else(|| format!("fila {row}: importe «{t}» no válido")) }
        };
        let amount = match one {
            Some(i) => read(Some(i)),
            None => match (read(cargo), read(abono)) {
                (Err(e), _) | (_, Err(e)) => Err(e),
                (Ok(None), Ok(None)) => Ok(None),
                (Ok(c), Ok(a)) => Ok(Some(a.unwrap_or(0.0) - c.unwrap_or(0.0).abs())),
            },
        };
        let dtext = cell(Some(fd));
        match (parse_any_date(dtext), amount) {
            (Some(_), Err(e)) => bad.push(e),
            (Some(date), Ok(Some(amount))) if amount.abs() >= 0.005 => out.push(RawMove { date, amount, concept: cell(Some(fc)).to_string(), reference: String::new() }),
            // una fecha mal escrita (no una fila de totales o de títulos)
            (None, Ok(Some(_))) if dtext.chars().any(|c| c.is_ascii_digit()) && dtext.contains(['/', '-', '.']) => bad.push(format!("fila {row}: fecha «{dtext}» no válida")),
            _ => {}
        }
    }
    if !bad.is_empty() {
        let shown: Vec<&str> = bad.iter().take(5).map(|s| s.as_str()).collect();
        return Err(format!("No se ha importado nada: {} filas no se pueden leer ({}). Corríjalas o vuelva a exportar el extracto del banco.", bad.len(), shown.join("; ")));
    }
    if out.is_empty() {
        return Err("No se ha encontrado ningún movimiento con fecha e importe.".into());
    }
    Ok(out)
}

/// Fecha de un extracto: «05/11/2026», «5-11-2026», «2026-11-05», «05/11/26»
/// o el número de día de Excel. Solo fechas entre 1990 y 2100 (un año de dos
/// cifras no se toma como el año 26).
fn parse_any_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    let ok = |d: NaiveDate| (1990..=2100).contains(&d.year());
    let head = s.get(..10.min(s.len())).unwrap_or(s);
    for f in ["%d/%m/%Y", "%d-%m-%Y", "%d.%m.%Y", "%Y-%m-%d", "%Y/%m/%d", "%d/%m/%y", "%d-%m-%y", "%Y-%m-%d %H:%M:%S"] {
        for t in [head, s] {
            if let Ok(d) = NaiveDate::parse_from_str(t, f)
                && ok(d)
            {
                return Some(d);
            }
        }
    }
    s.parse::<f64>().ok().filter(|n| (1000.0..80000.0).contains(n)).and_then(|n| NaiveDate::from_ymd_opt(1899, 12, 30).map(|b| b + Duration::days(n as i64))).filter(|d| ok(*d))
}

impl Db {
    pub fn audit(&mut self, action: &str, detail: &str) {
        let user = self.config.current_user.clone();
        self.audit_log.push(AuditEntry { when: now(), user, action: action.into(), detail: detail.into() });
        if self.audit_log.len() > 5000 {
            self.audit_log.remove(0);
        }
    }

    /// Añade movimientos al extracto de una cuenta, sin repetir los ya
    /// importados. Dos movimientos iguales (misma fecha, importe y concepto)
    /// son dos: al reimportar se cuenta cuántos de cada había.
    pub fn add_bank_moves(&mut self, account_id: u32, moves: &[RawMove]) -> (usize, usize) {
        type Key = (NaiveDate, i64, String);
        let key = |date: NaiveDate, amount: f64, concept: &str| -> Key { (date, (amount * 100.0).round() as i64, fold(concept)) };
        let mut had: HashMap<Key, usize> = HashMap::new();
        for b in self.bank_moves.iter().filter(|b| b.account_id == account_id) {
            *had.entry(key(b.date, b.amount, &b.concept)).or_default() += 1;
        }
        let mut seen: HashMap<Key, usize> = HashMap::new();
        let mut added = 0;
        let mut dup = 0;
        for m in moves {
            let k = key(m.date, m.amount, &m.concept);
            let n = seen.entry(k.clone()).or_default();
            *n += 1;
            if *n <= had.get(&k).copied().unwrap_or(0) {
                dup += 1;
                continue;
            }
            let id = self.new_id();
            self.bank_moves.push(BankMove { id, account_id, date: m.date, amount: round2(m.amount), concept: m.concept.clone(), reference: m.reference.clone(), matched: None, set_by_match: false });
            added += 1;
        }
        self.audit("Importar extracto", &format!("{added} movimientos nuevos, {dup} repetidos"));
        (added, dup)
    }

    /// Importa un extracto en la cuenta. En red lo hace el servidor con los
    /// datos al día, así que dos puestos que importan el mismo extracto no lo
    /// duplican. Devuelve cuántos movimientos son nuevos.
    pub fn import_bank_moves(&mut self, account_id: u32, moves: &[RawMove]) -> Result<u32, String> {
        if !self.bank_accounts.iter().any(|a| a.id == account_id) {
            return Err("La cuenta del banco ya no existe.".into());
        }
        if moves.is_empty() {
            return Err("El extracto no trae movimientos.".into());
        }
        Ok(self.add_bank_moves(account_id, moves).0 as u32)
    }

    /// Lo ya conciliado y los cobros anulados, calculado una vez.
    fn bank_used(&self) -> Used<'_> {
        let mut u = Used::default();
        for m in self.bank_moves.iter() {
            match &m.matched {
                Some(Match::Payments(v)) => u.payments.extend(v.iter().copied()),
                Some(Match::Expense(e)) => {
                    u.expenses.insert(*e);
                }
                Some(Match::Remittance(r)) => {
                    u.remittances.insert(*r);
                }
                _ => {}
            }
        }
        for p in self.payments.iter() {
            if let Some(c) = p.cancels {
                u.voided.insert(p.id);
                u.voided.insert(c);
            }
            u.by_cents.entry(cents(p.amount)).or_default().push(p);
            if p.method == "Tarjeta" {
                u.cards_by_day.entry(p.date.date()).or_default().push(p);
            }
        }
        u
    }

    /// Lo que puede ser un movimiento del banco según sus reglas:
    /// - cobros por transferencia, Bizum o financiación del mismo importe (±3 días);
    /// - liquidaciones de tarjeta (suma del día, con sus anulaciones);
    /// - remesas SEPA por su total (o por lo cobrado);
    /// - recibos devueltos, dinero devuelto a un paciente por transferencia o
    ///   Bizum, y gastos.
    ///
    /// Sin lo ya conciliado ni los cobros anulados (no pasan por el banco).
    fn auto_candidates(&self, m: &BankMove, u: &Used) -> Vec<Match> {
        let near = |d: NaiveDate, days: i64| (m.date - d).num_days().abs() <= days;
        let same = |a: f64| (a - m.amount).abs() < 0.005;
        let single = |p: &Payment| !u.payments.contains(&p.id) && !u.voided.contains(&p.id);
        // los cobros de ese importe (los importes van al céntimo)
        let same_amount: &[&Payment] = u.by_cents.get(&cents(m.amount)).map(|v| v.as_slice()).unwrap_or_default();
        let mut out = vec![];
        if m.amount > 0.0 {
            for r in self.remittances.iter().filter(|r| (same(r.total()) || same(r.collected())) && near(r.collection, 5) && !u.remittances.contains(&r.id)) {
                out.push(Match::Remittance(r.id));
            }
            for p in same_amount.iter().filter(|p| matches!(p.method.as_str(), "Transferencia" | "Bizum" | "Financiación") && near(p.date.date(), 3) && single(p)) {
                out.push(Match::Payments(vec![p.id]));
            }
            // liquidación del datáfono: suma de cobros con tarjeta de un día
            for back in 0..=3 {
                let day = m.date - Duration::days(back);
                let cards: Vec<&&Payment> = u.cards_by_day.get(&day).map(|v| v.iter().filter(|p| !u.payments.contains(&p.id)).collect()).unwrap_or_default();
                let sum: f64 = cards.iter().map(|p| p.amount).sum();
                if !cards.is_empty() && same(sum) {
                    out.push(Match::Payments(cards.iter().map(|p| p.id).collect()));
                }
            }
        } else {
            // recibo de una remesa devuelto por el banco
            for p in same_amount.iter().filter(|p| p.method == "Domiciliación" && p.cancels.is_none() && near(p.date.date(), 15) && !u.payments.contains(&p.id)) {
                out.push(Match::Payments(vec![p.id]));
            }
            // dinero devuelto a un paciente
            for p in same_amount.iter().filter(|p| matches!(p.method.as_str(), "Transferencia" | "Bizum") && near(p.date.date(), 3) && single(p)) {
                out.push(Match::Payments(vec![p.id]));
            }
            for e in self.expenses.iter().filter(|e| same(-e.total()) && near(e.due.unwrap_or(e.date), 40) && !u.expenses.contains(&e.id)) {
                out.push(Match::Expense(e.id));
            }
        }
        out
    }

    /// Plan de la conciliación automática: (movimiento, con qué) para los que
    /// tienen una sola posibilidad que ningún otro movimiento pendiente
    /// comparte, y los dudosos (más de una posibilidad, o una que también
    /// querría otro movimiento), que elige la clínica.
    fn auto_plan(&self) -> (Vec<(u32, Match)>, Vec<u32>) {
        let used = self.bank_used();
        let pending: Vec<(u32, Vec<Match>)> = self.bank_moves.iter().filter(|m| m.matched.is_none()).map(|m| (m.id, self.auto_candidates(m, &used))).filter(|(_, c)| !c.is_empty()).collect();
        let mut wanted: HashMap<Target, usize> = HashMap::new();
        for (_, c) in &pending {
            let mine: HashSet<Target> = c.iter().flat_map(targets).collect();
            for t in mine {
                *wanted.entry(t).or_default() += 1;
            }
        }
        let (mut sure, mut doubtful) = (vec![], vec![]);
        for (id, mut c) in pending {
            if c.len() == 1 && targets(&c[0]).iter().all(|t| wanted.get(t) == Some(&1)) {
                sure.push((id, c.remove(0)));
            } else {
                doubtful.push(id);
            }
        }
        (sure, doubtful)
    }

    /// Para conciliar a mano: lo de las reglas y, además, cualquier cobro o
    /// devolución del mismo importe en ±15 días y, para un cargo, cualquier
    /// gasto de ese importe. Nada ya conciliado con otro movimiento.
    pub fn match_candidates(&self, move_id: u32) -> Vec<(Match, String)> {
        let Some(m) = self.bank_moves.iter().find(|x| x.id == move_id) else { return vec![] };
        let used = self.bank_used();
        let same = |a: f64| (a - m.amount).abs() < 0.005;
        let mut list = self.auto_candidates(m, &used);
        let mut more = vec![];
        // los cobros de una remesa se concilian con la remesa
        let same_amount = used.by_cents.get(&cents(m.amount)).map(|v| v.as_slice()).unwrap_or_default();
        for p in same_amount.iter().filter(|p| !(p.method == "Domiciliación" && p.amount > 0.0) && (p.date.date() - m.date).num_days().abs() <= 15 && !used.payments.contains(&p.id) && !used.voided.contains(&p.id)) {
            more.push(Match::Payments(vec![p.id]));
        }
        if m.amount < 0.0 {
            for e in self.expenses.iter().filter(|e| same(-e.total()) && !used.expenses.contains(&e.id)) {
                more.push(Match::Expense(e.id));
            }
        }
        for c in more {
            if !list.contains(&c) {
                list.push(c);
            }
        }
        list.into_iter()
            .map(|c| {
                let label = self.candidate_label(&c);
                (c, label)
            })
            .collect()
    }

    /// Cómo se ofrece una posibilidad para elegirla (con su fecha).
    fn candidate_label(&self, c: &Match) -> String {
        match c {
            Match::Payments(v) if v.len() == 1 => self
                .payments
                .iter()
                .find(|p| p.id == v[0])
                .map(|p| format!("{} · {} · {} · {}{}", fmt_date(p.date.date()), self.patient_name(p.patient_id), p.method, p.receipt_no, if p.amount < 0.0 { " (devolución)" } else { "" }))
                .unwrap_or_default(),
            Match::Payments(v) => {
                let day = self.payments.iter().find(|p| Some(&p.id) == v.first()).map(|p| fmt_date(p.date.date())).unwrap_or_default();
                format!("Liquidación de tarjeta del {day} ({} cobros)", v.len())
            }
            Match::Expense(id) => self.expenses.iter().find(|e| e.id == *id).map(|e| format!("{} · {} · {}", fmt_date(e.date), e.supplier, e.concept)).unwrap_or_default(),
            Match::Remittance(id) => self.remittances.iter().find(|r| r.id == *id).map(|r| format!("Remesa SEPA #{id} · cargo del {} · {} recibos", fmt_date(r.collection), r.items.len())).unwrap_or_default(),
            Match::Other(s) => s.clone(),
        }
    }

    /// Movimientos sin conciliar que casan con más de una cosa (o con algo que
    /// también casa con otro movimiento): los elige la clínica.
    pub fn doubtful_moves(&self) -> Vec<u32> {
        self.auto_plan().1
    }

    /// Lo elegido cuadra con el movimiento y no está conciliado con otro.
    fn check_match(&self, m: &BankMove, f: &Match, used: &Used) -> Result<(), String> {
        let same = |a: f64| (a - m.amount).abs() < 0.005;
        let elsewhere = "ya está conciliado con otro movimiento (puede que desde otro puesto)";
        match f {
            Match::Payments(v) => {
                if v.is_empty() {
                    return Err("Elija al menos un cobro.".into());
                }
                let mut sum = 0.0;
                for (i, pid) in v.iter().enumerate() {
                    if v[..i].contains(pid) {
                        return Err("Hay un cobro repetido.".into());
                    }
                    let p = self.payments.iter().find(|p| p.id == *pid).ok_or("Uno de los cobros ya no existe.")?;
                    if used.payments.contains(pid) {
                        return Err(format!("El recibo {} {elsewhere}.", p.receipt_no));
                    }
                    // (en una liquidación de tarjeta, las anulaciones del día sí cuentan)
                    if v.len() == 1 && used.voided.contains(pid) {
                        return Err(format!("El recibo {} está anulado: no pasa por el banco.", p.receipt_no));
                    }
                    sum += p.amount;
                }
                if !same(sum) {
                    return Err(format!("Lo elegido suma {} y el movimiento es de {}.", money(sum), money(m.amount)));
                }
            }
            Match::Expense(eid) => {
                let e = self.expenses.iter().find(|e| e.id == *eid).ok_or("El gasto ya no existe.")?;
                if used.expenses.contains(eid) {
                    return Err(format!("Ese gasto {elsewhere}."));
                }
                if !same(-e.total()) {
                    return Err(format!("El gasto es de {} y el movimiento de {}.", money(e.total()), money(m.amount)));
                }
            }
            Match::Remittance(rid) => {
                let r = self.remittances.iter().find(|r| r.id == *rid).ok_or("La remesa ya no existe.")?;
                if used.remittances.contains(rid) {
                    return Err(format!("Esa remesa {elsewhere}."));
                }
                if !(same(r.total()) || same(r.collected())) {
                    return Err(format!("La remesa es de {} y el movimiento de {}.", money(r.total()), money(m.amount)));
                }
            }
            Match::Other(s) => {
                if s.trim().is_empty() || s.chars().count() > 120 {
                    return Err("Escriba qué es el movimiento (hasta 120 letras).".into());
                }
            }
        }
        Ok(())
    }

    fn apply_match(&mut self, id: u32, f: Match) {
        let mut set_now = false;
        if let Match::Expense(eid) = &f
            && let Some(e) = self.expenses.iter_mut().find(|e| e.id == *eid)
            && !e.paid
        {
            e.paid = true;
            set_now = true;
        }
        if let Match::Remittance(rid) = &f {
            // (si ya estaba cobrada, sigue así y deshacer no la toca)
            set_now = self.remittance_collected(*rid).is_ok();
        }
        if let Some(x) = self.bank_moves.iter_mut().find(|x| x.id == id) {
            x.matched = Some(f);
            x.set_by_match = set_now;
        }
    }

    /// Concilia a mano un movimiento con lo elegido, si cuadra y no está
    /// conciliado con otro. En red lo comprueba el servidor con los datos al
    /// día, así que dos puestos no concilian lo mismo dos veces.
    pub fn match_bank_move(&mut self, id: u32, f: Match) -> Result<u32, String> {
        let m = self.bank_moves.iter().find(|x| x.id == id).cloned().ok_or("El movimiento ya no existe.")?;
        if m.matched.is_some() {
            return Err("Este movimiento ya está conciliado (puede que desde otro puesto).".into());
        }
        self.check_match(&m, &f, &self.bank_used())?;
        let f = match f {
            Match::Other(s) => Match::Other(s.trim().to_string()),
            f => f,
        };
        let what = self.describe_match(&f);
        self.apply_match(id, f);
        self.audit("Conciliación manual", &format!("{} {} · {what}", fmt_date(m.date), money(m.amount)));
        Ok(id)
    }

    /// Apunta en el libro de gastos un cargo que no tenía gasto (comisiones,
    /// recibos) y lo concilia con él. Devuelve el gasto.
    pub fn expense_for_move(&mut self, id: u32) -> Result<u32, String> {
        let m = self.bank_moves.iter().find(|x| x.id == id).cloned().ok_or("El movimiento ya no existe.")?;
        if m.matched.is_some() {
            return Err("Este movimiento ya está conciliado (puede que desde otro puesto).".into());
        }
        if m.amount >= 0.0 {
            return Err("Solo un cargo del banco puede ser un gasto.".into());
        }
        let eid = self.new_id();
        self.expenses.push(crate::finance::Expense {
            id: eid,
            date: m.date,
            supplier: m.concept.chars().take(40).collect(),
            concept: m.concept.clone(),
            category: "Otros".into(),
            base: round2(-m.amount),
            vat_pct: 0.0,
            invoice_no: m.reference.clone(),
            method: "Domiciliación".into(),
            paid: false,
            due: None,
        });
        self.apply_match(id, Match::Expense(eid));
        self.audit("Conciliación manual", &format!("{} {} · gasto nuevo", fmt_date(m.date), money(m.amount)));
        Ok(eid)
    }

    /// Deshace una conciliación y lo que ella cambió:
    /// - el gasto que marcó como pagado vuelve a pendiente;
    /// - la remesa que marcó como cobrada vuelve a generada, con sus cobros
    ///   anulados (no borrados).
    ///
    /// Lo que ya estaba pagado o cobrado antes de conciliar sigue así.
    pub fn unmatch_bank_move(&mut self, id: u32) -> Result<u32, String> {
        let m = self.bank_moves.iter().find(|x| x.id == id).cloned().ok_or("El movimiento ya no existe.")?;
        let f = m.matched.clone().ok_or("Este movimiento no está conciliado (puede que lo hayan deshecho desde otro puesto).")?;
        let what = self.describe_match(&f);
        if m.set_by_match {
            match &f {
                Match::Expense(eid) => {
                    if let Some(e) = self.expenses.iter_mut().find(|e| e.id == *eid) {
                        e.paid = false;
                    }
                }
                Match::Remittance(rid) => self.remittance_uncollected(*rid, "conciliación del banco deshecha")?,
                _ => {}
            }
        }
        if let Some(x) = self.bank_moves.iter_mut().find(|x| x.id == id) {
            x.matched = None;
            x.set_by_match = false;
        }
        self.audit("Deshacer conciliación", &format!("{} {} · {what}", fmt_date(m.date), money(m.amount)));
        Ok(id)
    }

    /// Concilia automáticamente los movimientos sin duda (ver `auto_plan`);
    /// los dudosos quedan para elegirlos a mano.
    pub fn auto_reconcile(&mut self) -> usize {
        let (sure, _) = self.auto_plan();
        let n = sure.len();
        for (id, f) in sure {
            self.apply_match(id, f);
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

    /// Parejas de pacientes que parecen la misma persona: mismo NIF; mismo
    /// nombre (con la misma fecha de nacimiento o sin ella); o mismo móvil y
    /// apellidos. Se agrupan por esos datos en vez de comparar cada paciente con
    /// todos (con miles de pacientes eso tardaba minutos).
    pub fn find_duplicates(&self) -> Vec<(u32, u32, String)> {
        use std::collections::{BTreeMap, HashMap};
        let ps = &self.patients;
        let mut by_nif: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_mobile: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for (i, p) in ps.iter().enumerate() {
            let nif = p.nif.trim().to_uppercase();
            if !nif.is_empty() {
                by_nif.entry(nif).or_default().push(i);
            }
            by_name.entry(fold(&p.full_name())).or_default().push(i);
            let mobile = p.mobile.replace(' ', "");
            if !mobile.is_empty() {
                by_mobile.entry((mobile, fold(&p.last_name))).or_default().push(i);
            }
        }
        // (posición del primero, del segundo) → motivo; el primero que se encuentra manda
        let mut pairs: BTreeMap<(usize, usize), &str> = BTreeMap::new();
        let mut add = |groups: &HashMap<_, Vec<usize>>, reason: &'static str, ok: &dyn Fn(usize, usize) -> bool| {
            for g in groups.values().filter(|g| g.len() > 1) {
                for (n, &i) in g.iter().enumerate() {
                    for &j in &g[n + 1..] {
                        if ok(i, j) {
                            pairs.entry((i.min(j), i.max(j))).or_insert(reason);
                        }
                    }
                }
            }
        };
        add(&by_nif, "mismo NIF", &|_, _| true);
        add(&by_name, "mismo nombre", &|i, j| ps[i].birth == ps[j].birth || ps[i].birth.is_none() || ps[j].birth.is_none());
        let by_mobile: HashMap<String, Vec<usize>> = by_mobile.into_iter().map(|((m, l), v)| (format!("{m}|{l}"), v)).collect();
        add(&by_mobile, "mismo móvil y apellidos", &|_, _| true);
        pairs.into_iter().map(|((i, j), r)| (ps[i].id, ps[j].id, r.to_string())).collect()
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
        // todo lo suyo (lo clínico, lo económico y los avisos) pasa al que se conserva
        move_pid!(entries, budgets, payments, recalls, waitlist, invoices, perio, images, messages, documents, lab_works, prescriptions, ortho, health, ia_reports, whatsapp, rx_links, rights_requests, pay_requests, evo_notes, alert_acks, surveys);
        macro_rules! move_opt_pid {
            ($($v:ident),*) => { $( for x in self.$v.iter_mut() { if x.patient_id == Some(remove) { x.patient_id = Some(keep); } } )* };
        }
        move_opt_pid!(appointments, stock_moves, tasks, chat, booking_requests);
        for rm in self.remittances.iter_mut() {
            for it in rm.items.iter_mut().filter(|it| it.patient_id == remove) {
                it.patient_id = keep;
            }
        }
        for c in self.claims.iter_mut() {
            for l in c.lines.iter_mut().filter(|l| l.patient_id == remove) {
                l.patient_id = keep;
            }
        }
        for p in self.sterile_packs.iter_mut() {
            if let Some(u) = p.used.as_mut().filter(|u| u.patient_id == remove) {
                u.patient_id = keep;
            }
        }
        for o in self.slot_offers.iter_mut() {
            for i in o.invites.iter_mut().filter(|i| i.patient_id == remove) {
                i.patient_id = keep;
            }
        }
        for c in self.calls.iter_mut().filter(|c| c.patients.contains(&remove)) {
            c.patients.retain(|p| *p != remove && *p != keep);
            c.patients.push(keep);
        }
        // el paciente de otro programa que era el quitado ahora es el que se
        // conserva, como uno que ya estaba (su saldo ya no es solo el de allí)
        for i in self.imports.iter_mut().filter(|i| i.entity == crate::migracion::Entity::Pacientes && i.local == remove) {
            i.local = keep;
            i.linked = true;
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
        db.appointments.push(Appointment {
            id,
            date: today,
            start,
            minutes: 40,
            patient_id: Some(pid),
            free_name: String::new(),
            phone: String::new(),
            doctor_id: 2,
            box_id: 5,
            reason: reason.into(),
            treatment_code: code.into(),
            state: ApptState::Planificada,
            notes: String::new(),
            arrived_at: None,
            seen_at: None,
            finished_at: None,
            reminder_sent: true,
            first_visit: false,
            center_id: 2,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()
    }

    /// Todo lo que en los datos se refiere al paciente `pid` (por su número).
    fn references(v: &serde_json::Value, pid: u64, path: &str, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    let here = format!("{path}.{k}");
                    let mine = |y: &serde_json::Value| y.as_u64() == Some(pid);
                    if (k == "patient_id" || k == "patient") && mine(x) {
                        out.push(here.clone());
                    }
                    if k == "patients" && x.as_array().is_some_and(|a| a.iter().any(mine)) {
                        out.push(here.clone());
                    }
                    references(x, pid, &here, out);
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| references(x, pid, path, out)),
            _ => {}
        }
    }

    /// La búsqueda de antes, comparando todos con todos (para comprobar la nueva).
    fn duplicates_one_by_one(db: &Db) -> Vec<(u32, u32, String)> {
        let mut out = vec![];
        let ps = &db.patients;
        for i in 0..ps.len() {
            for j in i + 1..ps.len() {
                let (a, b) = (&ps[i], &ps[j]);
                let reason = if !a.nif.trim().is_empty() && a.nif.trim().to_uppercase() == b.nif.trim().to_uppercase() {
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

    #[test]
    fn duplicates_are_found_quickly_in_a_big_clinic() {
        let mut db = Db::sample(d());
        // unos cuantos repetidos de cada clase
        let base = db.patients[0].clone();
        let mut x = base.clone();
        (x.id, x.history_no, x.first_name, x.mobile) = (9001, 9001, "Otro".into(), "600 999 999".into());
        db.patients.push(x);
        let mut y = db.patients[1].clone();
        (y.id, y.history_no, y.nif, y.birth) = (9002, 9002, String::new(), None);
        db.patients.push(y);
        let mut z = db.patients[2].clone();
        (z.id, z.history_no, z.nif, z.first_name, z.mobile) = (9003, 9003, String::new(), "Zacarías".into(), db.patients[2].mobile.replace(' ', ""));
        db.patients.push(z);
        assert_eq!(db.find_duplicates(), duplicates_one_by_one(&db), "lo mismo que comparando uno a uno");
        assert!(db.find_duplicates().len() >= 3);
        // una clínica con 20.000 pacientes
        let proto = db.patients[0].clone();
        for i in 0..20_000u32 {
            let mut p = proto.clone();
            p.id = 100_000 + i;
            p.history_no = 100_000 + i;
            p.first_name = format!("Nombre{i}");
            p.last_name = format!("Apellido{} Segundo{}", i % 977, i % 13);
            p.nif = format!("{:08}X", 20_000_000 + i);
            p.mobile = format!("6{:08}", 10_000_000 + i);
            db.patients.push(p);
        }
        let t = std::time::Instant::now();
        let found = db.find_duplicates();
        assert!(t.elapsed() < std::time::Duration::from_secs(1), "tarda {:?}", t.elapsed());
        assert!(found.len() >= 3);
    }

    #[test]
    fn merging_moves_everything_of_the_removed_patient() {
        let mut db = Db::sample(d());
        let (keep, remove) = (100u32, 101u32);
        // de todo un poco del paciente que se va: tarea, aviso interno, cita pedida
        // por internet, liquidación a la mutua, llamada… (lo demás ya está en la muestra)
        db.add_task(d(), "Recepción", "", Some(remove), "Llamar", None);
        db.send_msg("Recepción", "", "Ha llamado", Some(remove));
        let rid = db.new_id();
        db.booking_requests.push(crate::booking::BookingRequest {
            id: rid,
            received: now(),
            name: "Carlos".into(),
            phone: "613 207 111".into(),
            email: String::new(),
            reason: "Revisión".into(),
            minutes: 30,
            code: String::new(),
            doctor_id: 1,
            date: d(),
            start: chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            notes: String::new(),
            first_visit: false,
            patient_id: Some(remove),
            state: crate::booking::ReqState::Confirmada,
            appointment_id: None,
            answer: String::new(),
            center_id: 1,
            replaces: None,
        });
        let tariff = db.patient(remove).unwrap().tariff_id;
        db.create_claim(tariff, d() - chrono::Duration::days(800), d(), d());
        if let Some(c) = db.calls.first_mut() {
            c.patients = vec![remove];
        }
        let before: Vec<String> = {
            let mut v = vec![];
            references(&serde_json::to_value(&db).unwrap(), remove as u64, "", &mut v);
            v
        };
        assert!(before.len() > 10, "la prueba tiene datos del paciente: {before:?}");
        db.merge_patients(keep, remove).unwrap();
        let mut left = vec![];
        references(&serde_json::to_value(&db).unwrap(), remove as u64, "", &mut left);
        assert!(left.is_empty(), "queda del paciente eliminado: {left:?}");
        // y lo suyo está ahora en la ficha que se conserva
        assert!(!db.patient_lots(keep).is_empty(), "sus implantes, con su lote");
        assert!(db.tasks.iter().any(|t| t.patient_id == Some(keep) && t.text == "Llamar"));
    }

    fn n43_line22(date: &str, debe: bool, cents: u64, r1: &str) -> String {
        format!("22{:4}{:4}{}{}{:2}{:3}{}{:014}{:010}{:<12}{:<16}", "", "0001", date, date, "02", "000", if debe { "1" } else { "2" }, cents, 0, r1, "")
    }

    #[test]
    fn norma43_parsing() {
        let txt = format!("11...cabecera\n{}\n2301TRANSFERENCIA DE ANA GARCIA LOPEZ        \n{}\n2301RECIBO LUZ\n33...\n88...\n", n43_line22("261103", false, 30000, "REF1"), n43_line22("261104", true, 9425, "REF2"));
        let v = parse_norma43_for(&txt, "").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].amount, 300.0);
        assert_eq!(v[0].date, NaiveDate::from_ymd_opt(2026, 11, 3).unwrap());
        assert!(v[0].concept.contains("ANA GARCIA"));
        assert_eq!(v[1].amount, -94.25);
        assert!(parse_norma43_for("hola", "").is_err());
    }

    fn n43_11(bank: &str, office: &str, acc: &str) -> String {
        format!("11{bank}{office}{acc}2611012611302{:014}978{}{:<26}{:3}", 100000, "3", "CLINICA DENTAL", "")
    }

    fn n43_33(bank: &str, office: &str, acc: &str, n_debe: u32, debe: u64, n_haber: u32, haber: u64) -> String {
        format!("33{bank}{office}{acc}{n_debe:05}{debe:014}{n_haber:05}{haber:014}2{:014}978{:4}", 0, "")
    }

    /// Un fichero Norma 43 con varias cuentas: solo se importan los de la
    /// cuenta elegida (por su IBAN); y si los totales de control no cuadran
    /// (fichero cortado o dañado), no se importa nada.
    #[test]
    fn norma43_with_several_accounts_and_control_totals() {
        let a = format!("{}\n{}\n2301TRANSFERENCIA ANA\n{}\n{}\n", n43_11("2100", "0418", "0200051332"), n43_line22("261103", false, 30000, "R1"), n43_line22("261104", true, 9425, "R2"), n43_33("2100", "0418", "0200051332", 1, 9425, 1, 30000));
        let b = format!("{}\n{}\n{}\n", n43_11("2100", "0418", "0200099999"), n43_line22("261105", false, 777, "R3"), n43_33("2100", "0418", "0200099999", 0, 0, 1, 777));
        let txt = format!("{a}{b}88\n");
        assert_eq!(n43_11("2100", "0418", "0200051332").len(), 80);
        assert_eq!(n43_33("2100", "0418", "0200051332", 1, 1, 1, 1).len(), 80);
        let v = parse_norma43_for(&txt, "ES91 2100 0418 4502 0005 1332").unwrap();
        assert_eq!(v.iter().map(|m| m.amount).collect::<Vec<_>>(), vec![300.0, -94.25], "solo la cuenta elegida");
        assert!(parse_norma43_for(&txt, "ES7921000813610123456789").unwrap_err().contains("no está en el fichero"));
        assert!(parse_norma43_for(&txt, "").unwrap_err().contains("trae 2 cuentas"), "sin elegir cuenta, avisa");
        // totales que no cuadran: fichero cortado
        let cut = txt.replacen(&n43_line22("261104", true, 9425, "R2"), "", 1);
        assert!(parse_norma43_for(&cut, "ES91 2100 0418 4502 0005 1332").unwrap_err().contains("no cuadran"));
    }

    /// Dos movimientos iguales del mismo día (dos transferencias de 50 €) son
    /// dos; volver a importar el mismo extracto no repite ninguno.
    #[test]
    fn identical_moves_in_a_statement_are_all_kept_and_not_reimported() {
        let mut db = Db::sample(d());
        let m = RawMove { date: d(), amount: 50.0, concept: "TRANSFERENCIA RECIBIDA".into(), reference: String::new() };
        assert_eq!(db.add_bank_moves(1, &[m.clone(), m.clone()]), (2, 0));
        assert_eq!(db.add_bank_moves(1, &[m.clone(), m.clone()]), (0, 2));
        assert_eq!(db.add_bank_moves(1, &[m.clone(), m.clone(), m.clone()]), (1, 2), "el tercero sí es nuevo");
    }

    /// Si un movimiento casa con varios cobros, o un cobro con varios
    /// movimientos, no se elige al azar: queda para elegirlo a mano, entre lo
    /// que no está ya conciliado, y lo elegido tiene que cuadrar.
    #[test]
    fn ambiguous_matches_are_left_for_the_user() {
        let mut db = Db::sample(d());
        db.bank_moves.clear();
        let mv = |c: &str, amount: f64| RawMove { date: today(), amount, concept: c.into(), reference: String::new() };
        let p1 = db.add_payment(102, 57.0, "Transferencia", "Pago");
        let p2 = db.add_payment(103, 57.0, "Transferencia", "Pago");
        db.add_bank_moves(1, &[mv("TRANSFERENCIA", 57.0)]);
        assert_eq!(db.auto_reconcile(), 0, "dos cobros posibles: no elige uno");
        let id = db.bank_moves[0].id;
        assert_eq!(db.doubtful_moves(), vec![id]);
        assert_eq!(db.match_candidates(id).len(), 2);
        db.match_bank_move(id, Match::Payments(vec![p1])).unwrap();
        assert!(db.doubtful_moves().is_empty());
        assert!(db.match_bank_move(id, Match::Payments(vec![p2])).unwrap_err().contains("ya está conciliado"), "no se concilia dos veces");
        // otro movimiento de 57 €: el cobro ya conciliado no se ofrece ni se acepta
        db.add_bank_moves(1, &[mv("TRANSFERENCIA DE OTRO", 57.0)]);
        let id2 = db.bank_moves.iter().find(|m| m.concept.ends_with("OTRO")).unwrap().id;
        assert!(db.match_bank_move(id2, Match::Payments(vec![p1])).unwrap_err().contains("con otro movimiento"));
        let offered: Vec<Match> = db.match_candidates(id2).into_iter().map(|c| c.0).collect();
        assert_eq!(offered, vec![Match::Payments(vec![p2])]);
        assert_eq!(db.auto_reconcile(), 1, "y ahora ya no hay duda");
        // un cobro y dos movimientos que casan con él: tampoco elige uno
        let p3 = db.add_payment(104, 61.0, "Bizum", "Pago");
        let p4 = db.add_payment(104, 40.0, "Bizum", "Pago");
        db.add_bank_moves(1, &[mv("BIZUM ANA", 61.0), mv("BIZUM LUIS", 61.0)]);
        assert_eq!(db.auto_reconcile(), 0, "dos movimientos para un solo cobro");
        assert_eq!(db.doubtful_moves().len(), 2);
        let id3 = db.bank_moves.iter().find(|m| m.concept == "BIZUM ANA").unwrap().id;
        assert!(db.match_bank_move(id3, Match::Payments(vec![p4])).unwrap_err().contains("suma"), "lo elegido tiene que cuadrar");
        db.match_bank_move(id3, Match::Payments(vec![p3])).unwrap();
        assert!(db.doubtful_moves().is_empty(), "el otro ya no tiene con qué");
    }

    /// Lo devuelto a un paciente por transferencia se concilia con su cargo
    /// (no hace falta apuntarlo como gasto, que lo contaría dos veces); un
    /// cobro anulado no pasa por el banco y no se concilia.
    #[test]
    fn refunds_are_reconciled_and_cancelled_payments_are_not() {
        let mut db = Db::sample(d());
        db.bank_moves.clear();
        let p = db.add_payment(102, 80.0, "Transferencia", "Pago");
        let back = db.add_payment(102, -30.0, "Transferencia", "Devolución de saldo a favor: sobraba");
        // un cobro de 45 € apuntado dos veces: el repetido se anula
        let twice = db.add_payment(103, 45.0, "Transferencia", "Pago");
        let good = db.add_payment(103, 45.0, "Transferencia", "Pago");
        db.cancel_payment(twice, "apuntado dos veces", true).unwrap();
        let mv = |c: &str, amount: f64| RawMove { date: today(), amount, concept: c.into(), reference: String::new() };
        db.add_bank_moves(1, &[mv("TRANSF ANA", 80.0), mv("TRANSF DEVOLUCION", -30.0), mv("TRANSF LUIS", 45.0)]);
        assert_eq!(db.auto_reconcile(), 3);
        let with = |c: &str| db.bank_moves.iter().find(|m| m.concept == c).unwrap().matched.clone();
        assert_eq!(with("TRANSF ANA"), Some(Match::Payments(vec![p])));
        assert_eq!(with("TRANSF DEVOLUCION"), Some(Match::Payments(vec![back])));
        assert_eq!(with("TRANSF LUIS"), Some(Match::Payments(vec![good])), "con el bueno, no con el anulado");
        let id = db.bank_moves.iter().find(|m| m.concept == "TRANSF LUIS").unwrap().id;
        db.unmatch_bank_move(id).unwrap();
        assert!(db.match_bank_move(id, Match::Payments(vec![twice])).unwrap_err().contains("anulado"));
    }

    /// Deshacer una conciliación deshace lo que cambió: el gasto que marcó
    /// como pagado vuelve a pendiente (el que ya estaba pagado, no).
    #[test]
    fn undoing_a_reconciliation_restores_what_it_changed() {
        let mut db = Db::sample(d());
        db.bank_moves.clear();
        let exp = db.expenses.iter().find(|e| !e.paid).cloned().unwrap();
        db.add_bank_moves(1, &[RawMove { date: exp.date, amount: -exp.total(), concept: "PAGO PROVEEDOR".into(), reference: String::new() }]);
        db.auto_reconcile();
        assert!(db.expenses.iter().find(|e| e.id == exp.id).unwrap().paid);
        let id = db.bank_moves[0].id;
        db.unmatch_bank_move(id).unwrap();
        assert!(db.bank_moves[0].matched.is_none());
        assert!(!db.expenses.iter().find(|e| e.id == exp.id).unwrap().paid, "vuelve a pendiente");
        assert!(db.unmatch_bank_move(id).unwrap_err().contains("no está conciliado"));
        // un gasto que ya estaba pagado sigue pagado al deshacer
        db.expenses.iter_mut().find(|e| e.id == exp.id).unwrap().paid = true;
        db.match_bank_move(id, Match::Expense(exp.id)).unwrap();
        db.unmatch_bank_move(id).unwrap();
        assert!(db.expenses.iter().find(|e| e.id == exp.id).unwrap().paid);
        assert!(db.audit_log.iter().any(|a| a.action == "Deshacer conciliación"));
        // un cargo sin gasto: se crea, una vez
        let eid = db.expense_for_move(id).unwrap();
        assert!(db.expense_for_move(id).unwrap_err().contains("ya está conciliado"));
        assert_eq!(db.bank_moves[0].matched, Some(Match::Expense(eid)));
        assert!(db.expenses.iter().find(|e| e.id == eid).unwrap().paid);
    }

    /// Una remesa que marcó como cobrada la conciliación vuelve a estar sin
    /// cobrar al deshacerla: sus cobros se anulan (no se borran) y el saldo de
    /// los pacientes vuelve a lo de antes. Cobrada a mano antes, sigue cobrada.
    #[test]
    fn undoing_a_remittance_match_uncollects_it() {
        let mut db = Db::sample(d());
        db.bank_moves.clear();
        let bid = db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        db.accept_budget(bid, d());
        let due = db.due_installments(d() + Duration::days(40));
        let rid = db.create_remittance(&due, d() + Duration::days(5)).unwrap();
        let rem = |db: &Db| db.remittances.iter().find(|r| r.id == rid).unwrap().clone();
        let balance = db.patient_balance(101);
        let pays = db.payments.len();
        db.add_bank_moves(1, &[RawMove { date: d() + Duration::days(5), amount: rem(&db).total(), concept: "ABONO REMESA".into(), reference: String::new() }]);
        assert_eq!(db.auto_reconcile(), 1);
        assert_eq!(rem(&db).state, crate::extra::RemState::Cobrada);
        assert!((db.patient_balance(101) - balance).abs() > 0.005);
        let id = db.bank_moves[0].id;
        db.unmatch_bank_move(id).unwrap();
        assert_eq!(rem(&db).state, crate::extra::RemState::Generada);
        assert!((db.patient_balance(101) - balance).abs() < 0.005, "el saldo vuelve a lo de antes");
        assert_eq!(db.payments.len(), pays + 2 * rem(&db).items.len(), "los cobros quedan, con su anulación");
        // volver a conciliarla la cobra otra vez, y otra vez se puede deshacer
        db.match_bank_move(id, Match::Remittance(rid)).unwrap();
        assert_eq!(rem(&db).state, crate::extra::RemState::Cobrada);
        db.unmatch_bank_move(id).unwrap();
        assert!((db.patient_balance(101) - balance).abs() < 0.005);
        // cobrada a mano antes de conciliar: deshacer no la toca
        db.remittance_collected(rid).unwrap();
        db.match_bank_move(id, Match::Remittance(rid)).unwrap();
        db.unmatch_bank_move(id).unwrap();
        assert_eq!(rem(&db).state, crate::extra::RemState::Cobrada);
    }

    /// Importes de extractos con signo y separadores de cualquier banco.
    #[test]
    fn statement_amounts_keep_their_sign_and_cents() {
        use crate::extra::parse_money;
        for (s, v) in
            [("1.234,56", 1234.56), ("1,234.56", 1234.56), ("-12,50", -12.5), ("12,50-", -12.5), ("(12,50)", -12.5), ("−12,50", -12.5), ("1.234.567", 1234567.0), ("12,50 €", 12.5), ("-1 234,56 EUR", -1234.56), ("+30", 30.0), ("94.25", 94.25)]
        {
            assert_eq!(parse_money(s), Some(v), "{s}");
        }
        for s in ["", "abc", "2026-11-05", "12/50"] {
            assert_eq!(parse_money(s), None, "{s}");
        }
    }

    fn sheet(headers: &[&str], rows: &[&[&str]]) -> crate::extra::Sheet {
        crate::extra::Sheet { headers: headers.iter().map(|s| s.to_string()).collect(), rows: rows.iter().map(|r| r.iter().map(|s| s.to_string()).collect()).collect() }
    }

    /// Extractos en Excel/CSV: cargo y abono en columnas aunque se llamen
    /// «Importe…», años de dos cifras, y si una fila no se puede leer no se
    /// importa nada (un movimiento perdido descuadra el saldo).
    #[test]
    fn statement_tables_are_read_whole_or_not_at_all() {
        let t = sheet(&["Fecha", "Concepto", "Importe cargo", "Importe abono", "Saldo"], &[&["05/11/26", "RECIBO LUZ", "94,25", "", "1.000,00"], &["06/11/26", "TRANSFERENCIA ANA", "", "300,00", "1.300,00"], &["", "Saldo final", "", "", "1.300,00"]]);
        let v = parse_bank_table(&t).unwrap();
        assert_eq!(v.iter().map(|m| (m.date, m.amount)).collect::<Vec<_>>(), vec![(NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(), -94.25), (NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(), 300.0)]);
        let one = sheet(&["F. Valor", "Descripción", "Importe (€)"], &[&["05/11/2026", "COMISION", "−3,50"], &["Total", "", "−3,50"]]);
        assert_eq!(parse_bank_table(&one).unwrap().iter().map(|m| m.amount).collect::<Vec<_>>(), vec![-3.5]);
        let bad = sheet(&["Fecha", "Concepto", "Importe"], &[&["05/11/2026", "A", "10,00"], &["31/02/2026", "B", "20,00"], &["07/11/2026", "C", "veinte"]]);
        let e = parse_bank_table(&bad).unwrap_err();
        assert!(e.contains("No se ha importado nada") && e.contains("fila 3") && e.contains("fila 4"), "{e}");
        assert!(parse_bank_table(&sheet(&["Fecha", "Concepto", "Saldo"], &[&["05/11/2026", "A", "10"]])).unwrap_err().contains("importe"));
    }

    /// Un Norma 43 cortado antes de los totales de la cuenta, o con un
    /// movimiento cortado, no se importa a medias.
    #[test]
    fn a_cut_norma43_file_is_not_imported() {
        let full = format!("{}\n{}\n{}\n88\n", n43_11("2100", "0418", "0200051332"), n43_line22("261103", false, 30000, "R1"), n43_33("2100", "0418", "0200051332", 0, 0, 1, 30000));
        assert_eq!(parse_norma43_for(&full, "").unwrap().len(), 1);
        let no_totals = format!("{}\n{}\n", n43_11("2100", "0418", "0200051332"), n43_line22("261103", false, 30000, "R1"));
        assert!(parse_norma43_for(&no_totals, "").unwrap_err().contains("registro 33"));
        let cut_move = format!("{}\n{}\n", n43_11("2100", "0418", "0200051332"), &n43_line22("261103", false, 30000, "R1")[..35]);
        assert!(parse_norma43_for(&cut_move, "").unwrap_err().contains("línea 2"));
    }

    #[test]
    fn reconcile_transfer_cards_and_expense() {
        let mut db = Db::sample(d());
        // liquidación de tarjeta del día: suma de cobros con tarjeta de hoy
        let card_sum: f64 = db.payments.iter().filter(|p| p.method == "Tarjeta" && p.date.date() == d()).map(|p| p.amount).sum();
        let exp = db.expenses.iter().find(|e| !e.paid).cloned().unwrap();
        db.add_bank_moves(
            1,
            &[RawMove { date: d() + Duration::days(1), amount: card_sum, concept: "LIQUIDACION TPV".into(), reference: String::new() }, RawMove { date: exp.date, amount: -exp.total(), concept: "PAGO PROVEEDOR".into(), reference: String::new() }],
        );
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
    fn reconcile_a_remittance_with_a_returned_debit() {
        let mut db = Db::sample(d());
        let bid = db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        db.accept_budget(bid, d());
        let due = db.due_installments(d() + Duration::days(40));
        let rid = db.create_remittance(&due, d() + Duration::days(5)).unwrap();
        db.remittance_collected(rid).unwrap();
        let it = db.remittances.iter().find(|r| r.id == rid).unwrap().items[0].clone();
        db.remittance_returned(rid, 0, "").unwrap();
        // el banco abona la remesa entera y días después carga la devolución
        let total = db.remittances.iter().find(|r| r.id == rid).unwrap().total();
        db.add_bank_moves(
            1,
            &[
                RawMove { date: d() + Duration::days(5), amount: total, concept: "ABONO REMESA".into(), reference: String::new() },
                RawMove { date: d() + Duration::days(9), amount: -it.amount, concept: "DEVOLUCION RECIBO".into(), reference: String::new() },
            ],
        );
        db.auto_reconcile();
        assert!(db.bank_moves.iter().any(|m| m.concept == "ABONO REMESA" && m.matched == Some(Match::Remittance(rid))));
        let back = db.payments.iter().find(|p| p.amount == -it.amount && p.concept.starts_with("Devolución")).unwrap().id;
        assert!(db.bank_moves.iter().any(|m| m.concept == "DEVOLUCION RECIBO" && m.matched == Some(Match::Payments(vec![back]))));
    }

    #[test]
    fn a_remittance_credited_without_the_returned_debits_is_reconciled() {
        let mut db = Db::sample(d());
        let bid = db.budgets.iter().find(|b| b.patient_id == 101 && b.plan.is_some()).unwrap().id;
        db.accept_budget(bid, d());
        let due = db.due_installments(d() + Duration::days(40));
        let rid = db.create_remittance(&due, d() + Duration::days(5)).unwrap();
        db.remittance_returned(rid, 0, "").unwrap();
        let net = db.remittances.iter().find(|r| r.id == rid).unwrap().collected();
        db.add_bank_moves(1, &[RawMove { date: d() + Duration::days(5), amount: net, concept: "ABONO REMESA".into(), reference: String::new() }]);
        db.auto_reconcile();
        assert_eq!(db.remittances.iter().find(|r| r.id == rid).unwrap().state, crate::extra::RemState::Cobrada);
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

#[cfg(test)]
mod big_bank_tests {
    use super::*;

    /// Una clínica grande: 100.000 cobros en tres años y 300 movimientos del
    /// banco sin conciliar (semilla fija).
    fn big_bank() -> Db {
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let mut db = Db::sample(start);
        db.bank_moves.clear();
        let methods = ["Efectivo", "Tarjeta", "Transferencia", "Bizum", "Financiación"];
        let mut seed: u64 = 42;
        let mut next = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for i in 0..100_000u32 {
            let id = 1_000_000 + i;
            let day = start + Duration::days((next() % 1095) as i64);
            let cents = 1_000 + (next() % 60_000) as i64;
            let method = methods[(next() % 5) as usize];
            db.payments.push(Payment {
                id,
                patient_id: 101,
                date: day.and_hms_opt(10, 0, 0).unwrap(),
                amount: cents as f64 / 100.0,
                method: method.into(),
                concept: "Pago".into(),
                receipt_no: format!("R{id}"),
                user: String::new(),
                cash_session: None,
                center_id: 1,
                cancels: None,
                installment: None,
            });
        }
        let moves: Vec<RawMove> = (0..300)
            .map(|i| {
                let p = &db.payments[db.payments.len() - 1 - (i * 37) % 90_000];
                RawMove { date: p.date.date() + Duration::days(1), amount: if i % 3 == 0 { -p.amount } else { p.amount }, concept: format!("MOVIMIENTO {i}"), reference: String::new() }
            })
            .collect();
        db.add_bank_moves(1, &moves);
        db
    }

    /// La pantalla de Bancos calcula los dudosos en cada fotograma: con
    /// 100.000 cobros y 300 movimientos pendientes tardaba unos 700 ms (la
    /// pantalla se quedaba congelada). Con los índices, unos 15 ms.
    #[test]
    fn finding_doubtful_moves_in_a_big_clinic_is_quick() {
        let mut db = big_bank();
        let t = std::time::Instant::now();
        let doubtful = db.doubtful_moves();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        assert!(ms < 200.0, "{ms:.0} ms");
        let id = db.bank_moves[1].id;
        assert!(!db.match_candidates(id).is_empty(), "su cobro se ofrece");
        // lo que concilia sola es lo que no es dudoso, y después no queda nada sin duda
        let pending = db.bank_moves.iter().filter(|m| m.matched.is_none()).count();
        let n = db.auto_reconcile();
        assert!(n > 0 && n + doubtful.len() <= pending);
        assert_eq!(db.auto_reconcile(), 0);
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use crate::test_rng::Rng;

    /// Extractos Norma 43 al azar (registros con campos cualquiera y ficheros
    /// reales estropeados): nunca se cae, y lo que lee tiene fechas e importes
    /// con sentido.
    #[test]
    fn norma43_survives_random_files() {
        let mut rng = Rng(0x5eed_0043);
        let rec = |rng: &mut Rng| -> String {
            let kind = ["11", "22", "23", "33", "88", "24", "xx"][rng.below(7)];
            let len = rng.below(90);
            let alphabet: Vec<char> = "0123456789 ABCDEFGHIJKLMNñ-+.,".chars().collect();
            let body: String = (0..len).map(|_| alphabet[rng.below(alphabet.len())]).collect();
            format!("{kind}{body}")
        };
        let real =
            "11210004180200051332261101261130200000000100000978CLINICA DENTAL\n22    000126110326110302000200000000030000000000000000R1\n2301TRANSFERENCIA ANA\n33210004180200051332000000000000000000010000000000300002000000000000000978    \n88\n";
        for i in 0..3_000 {
            let text = if i % 2 == 0 {
                let n = rng.below(12);
                (0..n).map(|_| rec(&mut rng)).collect::<Vec<_>>().join("\n")
            } else {
                String::from_utf8_lossy(&rng.mutate(real.as_bytes())).into_owned()
            };
            let iban = ["", "ES9121000418450200051332", "ES00", "XX12"][rng.below(4)];
            if let Ok(moves) = parse_norma43_for(&text, iban) {
                for m in moves {
                    assert!(m.amount.is_finite() && m.amount.abs() < 1e13, "{}", m.amount);
                    assert!((2000..2100).contains(&m.date.year()));
                }
            }
        }
    }

    /// Hojas de extracto e importes al azar: nunca se cae, y lo que lee son
    /// importes finitos, distintos de cero y con fecha razonable.
    #[test]
    fn statement_tables_and_amounts_survive_random_input() {
        let mut rng = Rng(0x5eed_0c5f);
        let headers = ["Fecha", "F. Valor", "Concepto", "Descripción", "Importe", "Importe cargo", "Importe abono", "Cargo", "Abono", "Saldo", "Otra"];
        let cell = |rng: &mut Rng| -> String {
            match rng.below(6) {
                0 => format!("{:02}/{:02}/{}", rng.below(35), rng.below(14), [26, 2026, 1899, 99999][rng.below(4)]),
                1 => format!("{}{},{:02}", ["", "-", "−", "("][rng.below(4)], rng.below(100_000), rng.below(100)),
                2 => format!("{}.{}", rng.below(1000), rng.below(1000)),
                3 => String::new(),
                4 => (0..rng.below(20)).map(|_| char::from_u32(32 + rng.below(9000) as u32).unwrap_or('?')).collect(),
                _ => rng.below(80000).to_string(),
            }
        };
        for _ in 0..2_000 {
            let n = 1 + rng.below(5);
            let hs: Vec<String> = (0..n).map(|_| headers[rng.below(headers.len())].to_string()).collect();
            let rows: Vec<Vec<String>> = (0..rng.below(8)).map(|_| (0..n).map(|_| cell(&mut rng)).collect()).collect();
            if let Ok(moves) = parse_bank_table(&crate::extra::Sheet { headers: hs, rows }) {
                for m in moves {
                    assert!(m.amount.is_finite() && m.amount.abs() >= 0.005);
                    assert!((1990..=2100).contains(&m.date.year()));
                }
            }
            let s = cell(&mut rng);
            if let Some(v) = crate::extra::parse_money(&s) {
                assert!(v.is_finite(), "{s}");
            }
        }
    }
}
