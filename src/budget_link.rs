//! Presupuesto para firmar por internet: cada presupuesto puede tener un
//! enlace secreto con el que el paciente lo revisa desde el móvil, elige cómo
//! pagarlo (por tratamiento o en cuotas sin intereses), lo firma con el dedo y
//! lo acepta; también puede rechazarlo o hacer una pregunta.
//!
//! La firma es una firma electrónica simple: el trazo, el nombre y el NIF que
//! escribe, la fecha y hora, la dirección de conexión y la huella (SHA-256) del
//! presupuesto tal como se le mostró. Si la clínica cambia el presupuesto
//! mientras el paciente lo tiene abierto, la huella no coincide y se le pide que
//! lo vuelva a abrir.

use crate::model::*;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub type Strokes = Vec<Vec<[f32; 2]>>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BudgetLink {
    pub token: String,
    pub budget_id: u32,
    pub created: Option<NaiveDateTime>,
    /// A quién se envió por correo.
    pub sent_to: String,
    /// Primera vez que el paciente lo abrió.
    pub viewed: Option<NaiveDateTime>,
    pub decided: Option<NaiveDateTime>,
    pub accepted: bool,
    pub signer: String,
    pub signer_nif: String,
    pub signature: Strokes,
    /// Cuotas elegidas (0 = pago según se realizan los tratamientos).
    pub installments: u32,
    /// Motivo si lo rechazó.
    pub reason: String,
    pub questions: Vec<(NaiveDateTime, String)>,
    /// Huella del presupuesto firmado.
    pub doc_hash: String,
    pub ip: String,
    /// Documento firmado guardado en la ficha.
    pub doc_id: Option<u32>,
}

/// Texto que identifica el contenido del presupuesto (lo que el paciente ve y firma).
pub fn budget_text(db: &Db, b: &Budget) -> String {
    let mut s = format!("Presupuesto {} · {} · {} · válido hasta {} · paciente {} · {}\n", b.id, b.title, fmt_date(b.date), fmt_date(b.expires()), b.patient_id, db.config.name);
    for l in &b.lines {
        let teeth: Vec<String> = l.teeth.iter().map(|t| t.to_string()).collect();
        s += &format!("{}|{}|{}|{}|{:.2}|{:.2}|{:.2}\n", l.code, l.description, teeth.join("-"), l.units, l.price, l.discount_pct, l.amount());
    }
    s += &format!("TOTAL {:.2}\n", b.total());
    if let Some(p) = &b.plan {
        s += &format!("PLAN {:.2} {} {}\n", p.down_payment, p.installments, p.every_months);
    }
    s += &b.notes;
    s
}

pub fn budget_hash(db: &Db, b: &Budget) -> String {
    Sha256::digest(budget_text(db, b).as_bytes()).iter().map(|x| format!("{x:02x}")).collect()
}

pub fn link_url(base: &str, token: &str) -> String {
    format!("{}/presupuesto/{token}", base.trim().trim_end_matches('/'))
}

/// Firma con trazo suficiente y tamaño razonable; coordenadas dentro del recuadro.
pub fn clean_strokes(s: &Strokes) -> Result<Strokes, String> {
    let points: usize = s.iter().map(|x| x.len()).sum();
    if s.len() > 80 || points > 6000 {
        return Err("La firma es demasiado larga. Bórrela y vuelva a firmar.".into());
    }
    if points < 10 {
        return Err("Firme en el recuadro con el dedo o el ratón.".into());
    }
    Ok(s.iter().map(|st| st.iter().map(|p| [p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0)]).collect()).filter(|st: &Vec<[f32; 2]>| !st.is_empty()).collect())
}

/// Cuotas que se ofrecen para este importe: (número, importe de cada una, entrada).
/// La entrada sólo la lleva el plan que la clínica haya puesto en el presupuesto.
pub fn installment_options(db: &Db, b: &Budget) -> Vec<(u32, f64, f64)> {
    let total = b.total();
    let mut v: Vec<u32> = vec![];
    if let Some(p) = &b.plan {
        v.push(p.installments);
    }
    if total >= db.booking.finance_min {
        v.extend(db.booking.installment_options.iter().copied().filter(|n| (2..=60).contains(n)));
    }
    v.sort();
    v.dedup();
    v.into_iter()
        .filter(|n| *n > 0)
        .map(|n| {
            let down = b.plan.as_ref().filter(|p| p.installments == n).map(|p| p.down_payment.min(total)).unwrap_or(0.0);
            (n, round2((total - down) / n as f64), down)
        })
        .collect()
}

/// Importe de cada cuota para la página del paciente: «333,33 € (la última de 333,34 €)».
fn installment_text(total: f64, n: u32, each: f64, down: f64) -> String {
    let plan = FinancePlan { down_payment: down, installments: n, first_date: today(), every_months: 1, paid: vec![] };
    let last = plan.installment(n.saturating_sub(1), total);
    if n > 1 && (last - each).abs() > 0.004 { format!("{} (la última de {})", money(each), money(last)) } else { money(each) }
}

impl Db {
    pub fn budget_link(&self, bid: u32) -> Option<&BudgetLink> {
        self.budget_links.iter().find(|l| l.budget_id == bid)
    }

    /// Enlace para revisar y firmar el presupuesto, si la clínica tiene la página publicada (lo crea si hace falta).
    pub fn budget_link_url(&mut self, bid: u32) -> Option<String> {
        let base = self.booking.public_url.trim().to_string();
        if !self.booking.enabled || !self.booking.budgets || !base.starts_with("http") {
            return None;
        }
        if !self.budgets.iter().any(|b| b.id == bid && b.state == BudgetState::Pendiente) {
            return None;
        }
        let t = self.budget_token(bid);
        Some(link_url(&base, &t))
    }

    pub fn budget_token(&mut self, bid: u32) -> String {
        if let Some(l) = self.budget_link(bid) {
            return l.token.clone();
        }
        let token = crate::portal::new_token();
        self.budget_links.push(BudgetLink { token: token.clone(), budget_id: bid, created: Some(now()), ..Default::default() });
        token
    }

    fn blink_mut(&mut self, bid: u32) -> Option<&mut BudgetLink> {
        self.budget_links.iter_mut().find(|l| l.budget_id == bid)
    }

    /// El paciente abrió el enlace por primera vez.
    pub fn budget_viewed(&mut self, bid: u32) -> bool {
        match self.blink_mut(bid) {
            Some(l) if l.viewed.is_none() => {
                l.viewed = Some(now());
                true
            }
            _ => false,
        }
    }

    /// Aceptación firmada desde el enlace. Devuelve el documento firmado de la ficha.
    #[allow(clippy::too_many_arguments)]
    pub fn budget_remote_accept(&mut self, bid: u32, signer: &str, nif: &str, strokes: Strokes, installments: u32, hash: &str, ip: &str) -> Result<u32, String> {
        let b = self.budgets.iter().find(|b| b.id == bid).cloned().ok_or("El presupuesto ya no existe.")?;
        if b.state != BudgetState::Pendiente {
            return Err("Este presupuesto ya no está pendiente.".into());
        }
        if b.expires() < today() {
            return Err("El presupuesto ha caducado.".into());
        }
        if b.lines.is_empty() {
            return Err("El presupuesto no tiene tratamientos.".into());
        }
        if budget_hash(self, &b) != hash {
            return Err("El presupuesto ha cambiado mientras lo revisaba: vuelva a abrirlo, por favor.".into());
        }
        let strokes = clean_strokes(&strokes)?;
        if installments > 0 && !installment_options(self, &b).iter().any(|o| o.0 == installments) {
            return Err("Esa forma de pago no está disponible.".into());
        }
        let when = now();
        if installments > 0
            && let Some(x) = self.budgets.iter_mut().find(|x| x.id == bid)
        {
            let down = x.plan.as_ref().filter(|p| p.installments == installments).map(|p| p.down_payment).unwrap_or(0.0);
            x.plan = Some(FinancePlan { down_payment: down, installments, first_date: add_months(today(), 1), every_months: 1, paid: vec![false; installments as usize] });
        }
        self.accept_budget(bid, today());
        let b = self.budgets.iter().find(|b| b.id == bid).cloned().unwrap_or(b);
        let pay = pay_text(&b);
        let lines: Vec<String> = b
            .lines
            .iter()
            .map(|l| {
                let teeth: Vec<String> = l.teeth.iter().map(|t| t.to_string()).collect();
                format!("· {}{} — {}", l.description, if teeth.is_empty() { String::new() } else { format!(" ({})", teeth.join("-")) }, money(l.amount()))
            })
            .collect();
        let body = format!(
            "Presupuesto nº {} «{}» del {}.\n\n{}\n\nTotal: {}\nForma de pago: {}\n\nAcepto el presupuesto y autorizo a {} a realizar los tratamientos indicados.\n\nFirmado electrónicamente por {}{} el {} desde la dirección {}. Huella del presupuesto (SHA-256): {}.",
            b.id,
            b.title,
            fmt_date(b.date),
            lines.join("\n"),
            money(b.total()),
            pay,
            self.config.name,
            signer.trim(),
            if nif.trim().is_empty() { String::new() } else { format!(" (NIF {})", nif.trim()) },
            when.format("%d/%m/%Y a las %H:%M"),
            if ip.is_empty() { "desconocida" } else { ip },
            hash
        );
        let doc = self.new_id();
        self.documents.push(crate::comms::PatientDoc {
            id: doc,
            patient_id: b.patient_id,
            template_id: 0,
            title: format!("Presupuesto nº {} aceptado y firmado por internet", b.id),
            category: "Presupuesto".into(),
            date: today(),
            body,
            doctor_id: b.doctor_id,
            signed: true,
            signer: signer.trim().to_string(),
            signed_at: Some(when),
            signature: strokes.clone(),
        });
        if let Some(l) = self.blink_mut(bid) {
            l.decided = Some(when);
            l.accepted = true;
            l.signer = signer.trim().to_string();
            l.signer_nif = nif.trim().to_string();
            l.signature = strokes;
            l.installments = installments;
            l.doc_hash = hash.to_string();
            l.ip = ip.to_string();
            l.doc_id = Some(doc);
        }
        Ok(doc)
    }

    pub fn budget_remote_reject(&mut self, bid: u32, reason: &str) -> bool {
        if !self.budgets.iter().any(|b| b.id == bid && b.state == BudgetState::Pendiente) {
            return false;
        }
        let why = if reason.trim().is_empty() { "Rechazado por el paciente por internet".to_string() } else { format!("Por internet: {}", reason.trim()) };
        self.reject_budget(bid, &why);
        if let Some(l) = self.blink_mut(bid) {
            l.decided = Some(now());
            l.accepted = false;
            l.reason = reason.trim().to_string();
        }
        true
    }

    pub fn budget_remote_question(&mut self, bid: u32, text: &str) -> bool {
        let t = text.trim();
        if t.is_empty() {
            return false;
        }
        match self.blink_mut(bid) {
            Some(l) => {
                l.questions.push((now(), t.to_string()));
                true
            }
            None => false,
        }
    }
}

pub fn pay_text(b: &Budget) -> String {
    match &b.plan {
        Some(p) => format!(
            "{}{}, sin intereses, {} (la primera el {})",
            if p.down_payment > 0.0 { format!("entrada de {} y ", money(p.down_payment)) } else { String::new() },
            p.installments_text(b.total()),
            if p.every_months <= 1 { "una al mes".to_string() } else { format!("una cada {} meses", p.every_months) },
            fmt_date(p.first_date)
        ),
        None => "según se realizan los tratamientos".into(),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// La firma como dibujo SVG.
pub fn signature_svg(s: &Strokes, w: f32, h: f32) -> String {
    let mut out = format!("<svg xmlns='http://www.w3.org/2000/svg' width='{w}' height='{h}' viewBox='0 0 {w} {h}'>");
    for st in s {
        if st.len() < 2 {
            continue;
        }
        let pts: Vec<String> = st.iter().map(|p| format!("{:.1},{:.1}", p[0] * w, p[1] * h)).collect();
        out += &format!("<polyline points='{}' fill='none' stroke='#14286e' stroke-width='2.2' stroke-linecap='round' stroke-linejoin='round'/>", pts.join(" "));
    }
    out + "</svg>"
}

/// Copia firmada del presupuesto (para el paciente y para imprimir).
pub fn signed_html(db: &Db, bid: u32) -> Option<String> {
    let b = db.budgets.iter().find(|b| b.id == bid)?;
    let l = db.budget_link(bid).filter(|l| l.accepted)?;
    let p = db.patient(b.patient_id).cloned().unwrap_or_default();
    let rows: String = b
        .lines
        .iter()
        .map(|x| {
            let teeth: Vec<String> = x.teeth.iter().map(|t| t.to_string()).collect();
            format!("<tr><td>{}</td><td>{}</td><td class='n'>{}</td><td class='n'>{}</td><td class='n'>{}</td></tr>", esc(&x.description), teeth.join("-"), x.units, esc(&money(x.price)), esc(&money(x.amount())))
        })
        .collect();
    let when = l.decided.map(|t| t.format("%d/%m/%Y a las %H:%M").to_string()).unwrap_or_default();
    Some(format!(
        "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Presupuesto {id} firmado</title><style>
body{{font-family:Inter,Segoe UI,Arial,sans-serif;max-width:820px;margin:32px auto;color:#1e293b;padding:0 20px}}
.top{{display:flex;justify-content:space-between;border-bottom:3px solid #0d9488;padding-bottom:12px}} h1{{font-size:22px;margin:18px 0 4px}}
.muted{{color:#64748b;font-size:13px}} table{{width:100%;border-collapse:collapse;margin-top:12px}} th{{background:#eef3f8;color:#475569;font-size:12px;text-transform:uppercase;text-align:left;padding:8px}}
td{{padding:8px;border-bottom:1px solid #e2e8f0;font-size:14px}} td.n,th.n{{text-align:right}} tr.t td{{font-weight:700;font-size:16px;border:none}}
.box{{background:#f4f8fb;border-radius:10px;padding:12px 14px;margin-top:14px;font-size:13.5px;line-height:1.5}} .sig{{border:1px solid #cbd5e1;border-radius:10px;padding:8px;display:inline-block;background:#fff}}
.ok{{display:inline-block;background:#e8f7ee;color:#166534;border-radius:999px;padding:4px 12px;font-weight:600;font-size:13px}} code{{font-size:11px;word-break:break-all;color:#475569}}
</style></head><body>
<div class='top'><div><b style='font-size:19px'>{clinic}</b><div class='muted'>{addr} · {phone} · NIF {cnif}</div></div><div><span class='ok'>Aceptado y firmado</span></div></div>
<h1>Presupuesto nº {id} · {title}</h1><div class='muted'>Del {date} · paciente {pname} (historia nº {hist})</div>
<table><tr><th>Tratamiento</th><th>Piezas</th><th class='n'>Uds.</th><th class='n'>Precio</th><th class='n'>Importe</th></tr>{rows}<tr class='t'><td colspan='4' class='n'>TOTAL</td><td class='n'>{total}</td></tr></table>
<div class='box'><b>Forma de pago:</b> {pay}.<br>Servicios sanitarios exentos de IVA.</div>
<div class='box'>Acepto el presupuesto y autorizo a {clinic} a realizar los tratamientos indicados.<br><br>
<div class='sig'>{svg}</div><br>
Firmado electrónicamente por <b>{signer}</b>{nif} el {when} desde la dirección {ip}.<br>Huella del presupuesto (SHA-256): <code>{hash}</code></div>
</body></html>",
        id = b.id,
        clinic = esc(&db.config.name),
        addr = esc(&db.config.address),
        phone = esc(&db.config.phone),
        cnif = esc(&db.config.nif),
        title = esc(&b.title),
        date = fmt_date(b.date),
        pname = esc(&p.full_name()),
        hist = p.history_no,
        total = esc(&money(b.total())),
        pay = esc(&pay_text(b)),
        svg = signature_svg(&l.signature, 320.0, 120.0),
        signer = esc(&l.signer),
        nif = if l.signer_nif.is_empty() { String::new() } else { format!(" (NIF {})", esc(&l.signer_nif)) },
        when = when,
        ip = esc(if l.ip.is_empty() { "desconocida" } else { &l.ip }),
        hash = esc(&l.doc_hash),
    ))
}

// ───────────────────────────── lo que ve la página ─────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct PubBudget {
    pub bid: u32,
    pub first_name: String,
    pub title: String,
    pub date: NaiveDate,
    pub expires: NaiveDate,
    pub doctor: String,
    /// descripción, piezas, unidades, precio, descuento %, importe
    pub lines: Vec<(String, String, u32, f64, f64, f64)>,
    pub total: f64,
    pub notes: String,
    /// pendiente, aceptado, rechazado, cerrado
    pub state: &'static str,
    pub options: Vec<(u32, f64, f64)>,
    pub hash: String,
    pub signed_by: String,
    pub signed_at: Option<NaiveDateTime>,
    pub viewed: bool,
}

pub fn publish_budgets(db: &Db) -> HashMap<String, PubBudget> {
    let mut m = HashMap::new();
    if !db.booking.budgets {
        return m;
    }
    for l in &db.budget_links {
        let Some(b) = db.budgets.iter().find(|b| b.id == l.budget_id) else { continue };
        let first = db.patient(b.patient_id).map(|p| p.first_name.clone()).unwrap_or_default();
        m.insert(
            l.token.clone(),
            PubBudget {
                bid: b.id,
                first_name: first,
                title: b.title.clone(),
                date: b.date,
                expires: b.expires(),
                doctor: db.doctor(b.doctor_id).map(|d| d.alias.clone()).unwrap_or_default(),
                lines: b.lines.iter().map(|x| (x.description.clone(), x.teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", "), x.units, x.price, x.discount_pct, x.amount())).collect(),
                total: b.total(),
                notes: b.notes.clone(),
                state: match b.state {
                    BudgetState::Pendiente => "pendiente",
                    BudgetState::Aceptado => "aceptado",
                    BudgetState::Rechazado => "rechazado",
                    BudgetState::Cerrado => "cerrado",
                },
                options: installment_options(db, b),
                hash: budget_hash(db, b),
                signed_by: if l.accepted { l.signer.clone() } else { String::new() },
                signed_at: if l.accepted { l.decided } else { None },
                viewed: l.viewed.is_some(),
            },
        );
    }
    m
}

pub fn budget_json(p: &crate::booking::Published, b: &PubBudget, now: NaiveDateTime) -> serde_json::Value {
    let expired = b.state == "pendiente" && b.expires < now.date();
    serde_json::json!({
        "ok": true,
        "clinica": p.clinic, "direccion": p.address, "telefono": p.phone,
        "nombre": b.first_name, "numero": b.bid, "titulo": b.title,
        "fecha": fmt_date(b.date), "valido_hasta": fmt_date(b.expires), "profesional": b.doctor,
        "lineas": b.lines.iter().map(|l| serde_json::json!({"tratamiento": l.0, "piezas": l.1, "unidades": l.2, "precio": money(l.3), "descuento": if l.4 > 0.0 { format!("{:.0} %", l.4) } else { String::new() }, "importe": money(l.5)})).collect::<Vec<_>>(),
        "total": money(b.total),
        "notas": b.notes,
        "estado": if expired { "caducado" } else { b.state },
        "cuotas": b.options.iter().map(|(n, a, d)| serde_json::json!({"n": n, "importe": installment_text(b.total, *n, *a, *d), "entrada": if *d > 0.0 { money(*d) } else { String::new() }})).collect::<Vec<_>>(),
        "huella": b.hash,
        "firmado_por": b.signed_by,
        "firmado_el": b.signed_at.map(|t| t.format("%d/%m/%Y %H:%M").to_string()),
    })
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct AcceptBody {
    pub nombre: String,
    pub nif: String,
    pub firma: Strokes,
    pub cuotas: u32,
    pub huella: String,
    pub acepta: bool,
}

/// Lo que el paciente hace con el enlace del presupuesto.
pub enum BudgetAction {
    Viewed,
    Accept { signer: String, nif: String, strokes: Strokes, installments: u32, hash: String, ip: String },
    Reject(String),
    Question(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un presupuesto pendiente (sin el enlace de ejemplo, para empezar de cero).
    fn pending(db: &mut Db) -> u32 {
        db.budget_links.clear();
        db.budgets.iter().find(|b| b.state == BudgetState::Pendiente && !b.lines.is_empty() && b.expires() >= today()).map(|b| b.id).expect("presupuesto pendiente de ejemplo")
    }

    fn sig() -> Strokes {
        vec![(0..20).map(|i| [i as f32 / 20.0, 0.5 + (i as f32 / 3.0).sin() * 0.3]).collect()]
    }

    #[test]
    fn signed_acceptance_with_installments() {
        let mut db = Db::sample(today());
        let bid = pending(&mut db);
        db.budget_token(bid);
        let b = db.budgets.iter().find(|b| b.id == bid).unwrap().clone();
        let hash = budget_hash(&db, &b);
        let opts = installment_options(&db, &b);
        // con la huella de un presupuesto distinto, no
        assert!(db.budget_remote_accept(bid, "Ana García López", "", sig(), 0, "otra", "1.2.3.4").is_err());
        // sin firma, no
        assert!(db.budget_remote_accept(bid, "Ana García López", "", vec![], 0, &hash, "1.2.3.4").is_err());
        let n = opts.first().map(|o| o.0).unwrap_or(0);
        let doc = db.budget_remote_accept(bid, "Ana García López", "12345678Z", sig(), n, &hash, "1.2.3.4").unwrap();
        let b = db.budgets.iter().find(|b| b.id == bid).unwrap();
        assert_eq!(b.state, BudgetState::Aceptado);
        if n > 0 {
            assert_eq!(b.plan.as_ref().unwrap().installments, n);
        }
        let d = db.documents.iter().find(|d| d.id == doc).unwrap();
        assert!(d.signed && d.signature.len() == 1 && d.body.contains(&hash) && d.body.contains("1.2.3.4"));
        assert!(db.entries.iter().any(|e| e.budget_id == Some(bid)), "los tratamientos pasan al plan");
        let html = signed_html(&db, bid).unwrap();
        assert!(html.contains("Ana García López") && html.contains("<polyline") && html.contains("12345678Z"));
        // no se acepta dos veces
        assert!(db.budget_remote_accept(bid, "Ana", "", sig(), 0, &hash, "").is_err());
    }

    #[test]
    fn installments_respect_the_deposit() {
        let mut db = Db::sample(today());
        let bid = pending(&mut db);
        let total = db.budgets.iter().find(|b| b.id == bid).unwrap().total();
        db.booking.finance_min = 0.0;
        db.booking.installment_options = vec![3, 12];
        db.budgets.iter_mut().find(|b| b.id == bid).unwrap().plan = Some(FinancePlan { down_payment: 100.0, installments: 6, first_date: today(), every_months: 1, paid: vec![false; 6] });
        let b = db.budgets.iter().find(|b| b.id == bid).unwrap().clone();
        let o = installment_options(&db, &b);
        assert_eq!(o.iter().map(|x| x.0).collect::<Vec<_>>(), vec![3, 6, 12]);
        for (n, each, down) in o {
            // lo que se paga en total es siempre el importe del presupuesto
            assert!((each * n as f64 + down - total).abs() < 0.05 * n as f64, "{n} cuotas");
            assert_eq!(down > 0.0, n == 6, "la entrada sólo en el plan de la clínica");
        }
    }

    #[test]
    fn reject_and_questions() {
        let mut db = Db::sample(today());
        let bid = pending(&mut db);
        db.budget_token(bid);
        assert!(db.budget_remote_question(bid, "¿Se puede pagar con tarjeta?"));
        assert!(db.budget_remote_reject(bid, "Prefiero pensarlo"));
        let b = db.budgets.iter().find(|b| b.id == bid).unwrap();
        assert_eq!(b.state, BudgetState::Rechazado);
        assert!(b.reject_reason.contains("Prefiero pensarlo"));
        assert_eq!(db.budget_link(bid).unwrap().questions.len(), 1);
        assert!(!db.budget_remote_reject(bid, "otra vez"));
    }

    #[test]
    fn strokes_are_checked() {
        assert!(clean_strokes(&vec![vec![[0.1, 0.1]]]).is_err());
        let big: Strokes = (0..100).map(|_| vec![[0.5, 0.5]; 2]).collect();
        assert!(clean_strokes(&big).is_err());
        let s = clean_strokes(&vec![(0..12).map(|i| [i as f32, -1.0]).collect()]).unwrap();
        assert!(s[0].iter().all(|p| (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1])));
    }
}
