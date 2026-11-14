//! «Mi espacio», el portal del paciente (T-F5-10): sus citas, presupuestos,
//! facturas, documentos firmados y recetas, desde el móvil.
//!
//! - **Acceso:** un enlace secreto (128 bits) por titular. El titular es:
//!   - el propio paciente, desde los 16 años;
//!   - o su responsable (quien figura en «Factura a nombre de», por su NIF),
//!     que ve a todos los pacientes de los que es responsable.
//! - **Entrada:**
//!   - con el DNI/NIE y la fecha de nacimiento, del paciente o, para un
//!     responsable, su NIF y la fecha de nacimiento de uno de sus pacientes;
//!   - lo publicado solo guarda su huella;
//!   - 5 fallos bloquean el enlace media hora.
//! - **Sesión:**
//!   - un código aleatorio en una *cookie* `HttpOnly` y `SameSite=Strict`
//!     (además `Secure` con https);
//!   - caduca a los [`IDLE_MINUTES`] sin uso y a las [`MAX_MINUTES`];
//!   - se cierra con «Salir».
//! - **Autorización por objeto:** cada factura o documento se busca solo
//!   entre los pacientes de ese titular. Otro identificador da «no existe» (no
//!   se dice si es de otro), así que cambiar un número en la dirección no
//!   enseña nada.
//! - **Revocación:** la clínica revoca el acceso y, al publicar, el enlace y
//!   sus sesiones dejan de valer.

use crate::i18n::{Lang, t};
use crate::model::*;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const IDLE_MINUTES: i64 = 20;
pub const MAX_MINUTES: i64 = 120;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Holder {
    Patient(u32),
    /// Responsable, por su NIF (normalizado).
    Responsible(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortalAccess {
    pub id: u32,
    pub token: String,
    pub holder: Holder,
    /// Los pacientes autorizados al darlo (un responsable no gana pacientes
    /// después sin que se le dé otro acceso).
    #[serde(default)]
    pub patients: Vec<u32>,
    pub created: NaiveDateTime,
    #[serde(default)]
    pub revoked: Option<NaiveDateTime>,
}

fn norm(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase()
}

impl Db {
    /// Los pacientes que puede ver un titular hoy: el propio paciente desde
    /// los 16 años; un responsable, los menores de 16 que le tienen como
    /// responsable (al cumplirlos, dejan de salir). Se vuelve a comprobar al
    /// publicar: si cambia la ficha, cambia lo que ve.
    pub fn portal_patients(&self, h: &Holder, today: NaiveDate) -> Vec<u32> {
        match h {
            Holder::Patient(pid) => self.patient(*pid).filter(|p| p.age(today).is_none_or(|a| a >= 16) && p.blocked.is_none()).map(|p| vec![p.id]).unwrap_or_default(),
            Holder::Responsible(nif) => self.patients.iter().filter(|p| nif.len() >= 5 && norm(&p.payer.nif) == *nif && p.blocked.is_none() && p.age(today).is_some_and(|a| a < 16)).map(|p| p.id).collect(),
        }
    }

    /// Enlace nuevo de «Mi espacio» para un paciente o su responsable.
    pub fn portal_grant(&mut self, h: Holder, now: NaiveDateTime) -> Result<u32, String> {
        let today = now.date();
        let pats = self.portal_patients(&h, today);
        if pats.is_empty() {
            return Err(match h {
                Holder::Patient(_) => "El paciente tiene menos de 16 años (entra su responsable) o sus datos están bloqueados.".into(),
                Holder::Responsible(_) => "Ningún menor de 16 años tiene a ese responsable (NIF en «Factura a nombre de»).".into(),
            });
        }
        let can = match &h {
            Holder::Patient(pid) => self.patient(*pid).is_some_and(crate::rx_link::can_verify),
            Holder::Responsible(nif) => nif.len() >= 5 && pats.iter().any(|p| self.patient(*p).is_some_and(|x| x.birth.is_some())),
        };
        if !can {
            return Err("Faltan el DNI/NIE o la fecha de nacimiento en la ficha: sin ellos no se puede entrar.".into());
        }
        // un acceso vigente por titular: el anterior se revoca
        for a in self.portal_access.iter_mut().filter(|a| a.holder == h && a.revoked.is_none()) {
            a.revoked = Some(now);
        }
        let id = self.new_id();
        self.portal_access.push(PortalAccess { id, token: crate::portal::new_token(), holder: h.clone(), patients: pats.clone(), created: now, revoked: None });
        self.audit("Dar acceso a «Mi espacio»", &format!("{h:?} · pacientes {pats:?}"));
        Ok(id)
    }

    pub fn portal_revoke(&mut self, id: u32, now: NaiveDateTime) {
        if let Some(a) = self.portal_access.iter_mut().find(|a| a.id == id && a.revoked.is_none()) {
            a.revoked = Some(now);
            let h = format!("{:?}", a.holder);
            self.audit("Revocar acceso a «Mi espacio»", &h);
        }
    }
}

// ───────────────────────────── lo publicado ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct PubInvoice {
    pub id: u32,
    pub code: String,
    pub date: NaiveDate,
    pub total: f64,
    pub lines: Vec<(String, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PubDoc {
    pub id: u32,
    pub title: String,
    pub date: NaiveDate,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PubPatient {
    pub id: u32,
    pub first_name: String,
    /// Citas próximas: fecha, hora, profesional y su enlace («Mi cita»).
    pub appts: Vec<(NaiveDate, String, String, Option<String>)>,
    /// Presupuestos: título, total, estado y su enlace para firmar.
    pub budgets: Vec<(String, f64, String, Option<String>)>,
    pub invoices: Vec<PubInvoice>,
    pub docs: Vec<PubDoc>,
    /// «Mis recetas» (con su propia entrada).
    pub rx: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PubPortal {
    pub access_id: u32,
    /// Idioma de las páginas (el del primer paciente).
    pub lang: crate::i18n::Lang,
    /// Huellas válidas para entrar (DNI/NIE y fecha de nacimiento).
    pub checks: Vec<String>,
    pub patients: Vec<PubPatient>,
}

impl PubPortal {
    /// Una factura de uno de sus pacientes (si no, nada: «no existe»).
    pub fn invoice(&self, id: u32) -> Option<&PubInvoice> {
        self.patients.iter().flat_map(|p| &p.invoices).find(|i| i.id == id)
    }
    pub fn doc(&self, id: u32) -> Option<&PubDoc> {
        self.patients.iter().flat_map(|p| &p.docs).find(|d| d.id == id)
    }
}

pub fn publish_portal(db: &Db, today: NaiveDate) -> HashMap<String, PubPortal> {
    let mut out = HashMap::new();
    for a in db.portal_access.iter().filter(|a| a.revoked.is_none()) {
        // los de hoy y, además, de los que se autorizaron al darlo
        let pats: Vec<u32> = db.portal_patients(&a.holder, today).into_iter().filter(|p| a.patients.contains(p)).collect();
        if pats.is_empty() {
            continue;
        }
        // sin un DNI/NIE y una fecha de nacimiento válidos no se puede entrar
        let checks: Vec<String> = match &a.holder {
            Holder::Patient(pid) => db.patient(*pid).filter(|p| crate::rx_link::can_verify(p)).and_then(|p| Some(crate::rx_link::verifier(&a.token, &p.nif, p.birth?))).into_iter().collect(),
            Holder::Responsible(nif) if nif.len() >= 5 => pats.iter().filter_map(|pid| db.patient(*pid)?.birth).map(|b| crate::rx_link::verifier(&a.token, nif, b)).collect(),
            Holder::Responsible(_) => vec![],
        };
        if checks.is_empty() {
            continue;
        }
        let patients = pats
            .iter()
            .filter_map(|pid| {
                let p = db.patient(*pid)?;
                let appts = db
                    .appointments
                    .iter()
                    .filter(|x| x.patient_id == Some(*pid) && x.date >= today && !x.state.is_cancelled())
                    .map(|x| (x.date, x.start.format("%H:%M").to_string(), db.doctor_alias(x.doctor_id), db.appt_links.iter().find(|l| l.appointment_id == x.id).map(|l| format!("/mi-cita/{}", l.token))))
                    .collect();
                let budgets = db
                    .budgets
                    .iter()
                    .filter(|b| b.patient_id == *pid && b.tp.replaced_by.is_none())
                    .map(|b| (b.title.clone(), b.total(), b.state.label().to_string(), db.budget_links.iter().find(|l| l.budget_id == b.id).map(|l| format!("/presupuesto/{}", l.token))))
                    .collect();
                let invoices = db
                    .invoices
                    .iter()
                    .filter(|i| i.patient_id == *pid)
                    .map(|i| PubInvoice { id: i.id, code: i.code(), date: i.date, total: i.total(), lines: i.lines.iter().map(|l| (l.description.clone(), round2(l.base() + l.vat()))).collect() })
                    .collect();
                let docs = db.documents.iter().filter(|d| d.patient_id == *pid && d.signed && d.revoked_at.is_none()).map(|d| PubDoc { id: d.id, title: d.title.clone(), date: d.date, body: d.body.clone() }).collect();
                let rx = db.rx_links.iter().find(|l| l.patient_id == *pid).map(|l| format!("/recetas/{}", l.token));
                Some(PubPatient { id: *pid, first_name: p.first_name.clone(), appts, budgets, invoices, docs, rx })
            })
            .collect();
        let lang = pats.first().and_then(|p| db.patient(*p)).and_then(|p| p.lang).unwrap_or_default();
        out.insert(a.token.clone(), PubPortal { access_id: a.id, lang, checks, patients });
    }
    out
}

/// ¿Coinciden los datos escritos con los de la ficha? (el NIF y la fecha de nacimiento)
pub fn login_ok(token: &str, p: &PubPortal, nif: &str, birth: &str) -> bool {
    if norm(nif).len() < 5 {
        return false;
    }
    crate::rx_link::parse_birth(birth).is_some_and(|b| {
        let v = crate::rx_link::verifier(token, &norm(nif), b);
        p.checks.iter().any(|c| crate::users::ct_eq(c.as_bytes(), v.as_bytes()))
    })
}

/// Una sesión abierta (en la memoria del servidor de la página).
#[derive(Clone, Debug)]
pub struct Session {
    pub token: String,
    pub access_id: u32,
    pub started: NaiveDateTime,
    pub last: NaiveDateTime,
}

impl Session {
    pub fn alive(&self, now: NaiveDateTime) -> bool {
        (now - self.last).num_minutes() < IDLE_MINUTES && (now - self.started).num_minutes() < MAX_MINUTES
    }
}

// ───────────────────────────── las páginas ─────────────────────────────

const STYLE: &str = "body{margin:0;font-family:system-ui,Arial,sans-serif;color:#1e293b;background:#f4f7fb}.wrap{max-width:640px;margin:0 auto;padding:24px 16px}.card{background:#fff;border:1px solid #e2e8f0;border-radius:18px;padding:18px;margin-bottom:12px}h1{font-size:21px}h2{font-size:17px;margin:0 0 8px}input{width:100%;border:1px solid #cbd5e1;border-radius:12px;padding:11px;font-size:16px;margin:4px 0 10px}button{width:100%;background:#0d9488;border:0;color:#fff;border-radius:14px;padding:13px;font-size:16px;font-weight:600}.mu{color:#64748b}.err{color:#dc2626}a{color:#0d9488}li{margin:4px 0}";

fn page(lang: Lang, clinic: &str, body: &str) -> String {
    let e = crate::booking::esc_html;
    let me = t(lang, "Mi espacio");
    format!(
        "<!doctype html><html lang=\"{l}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"robots\" content=\"noindex\"><meta name=\"referrer\" content=\"no-referrer\"><title>{me} · {c}</title><style>{STYLE}</style></head><body><div class=\"wrap\"><h1>{c} · {me}</h1>{body}</div></body></html>",
        c = e(clinic),
        l = lang.code()
    )
}

pub fn login_page(lang: Lang, clinic: &str, token: &str, error: Option<&'static str>) -> String {
    let e = crate::booking::esc_html;
    let err = error.map(|x| format!("<p class=\"err\">{}</p>", e(t(lang, x)))).unwrap_or_default();
    page(
        lang,
        clinic,
        &format!(
            "<div class=\"card\"><p>{intro}</p>{err}<form method=\"post\" action=\"/api/portal/{tk}/entrar\"><input name=\"dni\" autocomplete=\"off\" placeholder=\"{dni}\" required><input name=\"nacimiento\" placeholder=\"{birth}\" required><button>{go}</button></form></div>",
            intro = e(t(lang, "Para entrar, escriba su DNI/NIE y su fecha de nacimiento (si entra por un menor a su cargo, su DNI/NIE y la fecha de nacimiento del menor).")),
            tk = e(token),
            dni = e(t(lang, "DNI/NIE")),
            birth = e(t(lang, "Fecha de nacimiento (dd/mm/aaaa)")),
            go = e(t(lang, "Entrar"))
        ),
    )
}

pub fn home_page(clinic: &str, token: &str, p: &PubPortal) -> String {
    let e = crate::booking::esc_html;
    let lang = p.lang;
    let money = |x: f64| crate::i18n::money(lang, crate::cents::Cents::from_eur(x).0);
    let mut b = String::new();
    for x in &p.patients {
        b.push_str(&format!("<div class=\"card\"><h2>{}</h2>", e(&x.first_name)));
        let link = |l: &Option<String>, label: &'static str| l.as_ref().map(|u| format!(" · <a href=\"{}\">{}</a>", e(u), e(t(lang, label)))).unwrap_or_default();
        b.push_str(&format!("<p class=\"mu\">{}</p><ul>", e(t(lang, "Próximas citas"))));
        if x.appts.is_empty() {
            b.push_str(&format!("<li class=\"mu\">{}</li>", e(t(lang, "Ninguna"))));
        }
        for (d, h, doc, l) in &x.appts {
            b.push_str(&format!("<li>{} · {} · {}{}</li>", crate::i18n::long_date(lang, *d), e(h), e(doc), link(l, "confirmar o cambiar")));
        }
        b.push_str(&format!("</ul><p class=\"mu\">{}</p><ul>", e(t(lang, "Presupuestos"))));
        for (title, total, st, l) in &x.budgets {
            b.push_str(&format!("<li>{} · {} · {}{}</li>", e(title), money(*total), e(st), link(l, "ver y firmar")));
        }
        b.push_str(&format!("</ul><p class=\"mu\">{}</p><ul>", e(t(lang, "Facturas"))));
        for i in &x.invoices {
            b.push_str(&format!("<li><a href=\"/api/portal/{tk}/factura/{}\">{} · {}</a> · {}</li>", i.id, e(&i.code), fmt_date(i.date), money(i.total), tk = e(token)));
        }
        b.push_str(&format!("</ul><p class=\"mu\">{}</p><ul>", e(t(lang, "Documentos firmados"))));
        for d in &x.docs {
            b.push_str(&format!("<li><a href=\"/api/portal/{tk}/documento/{}\">{}</a> · {}</li>", d.id, e(&d.title), fmt_date(d.date), tk = e(token)));
        }
        b.push_str("</ul>");
        if let Some(r) = &x.rx {
            b.push_str(&format!("<p><a href=\"{}\">{}</a></p>", e(r), e(t(lang, "Mis recetas"))));
        }
        b.push_str("</div>");
    }
    b.push_str(&format!("<form method=\"post\" action=\"/api/portal/{}/salir\"><button>{}</button></form>", e(token), e(t(lang, "Salir"))));
    page(lang, clinic, &b)
}

pub fn invoice_page(lang: Lang, clinic: &str, i: &PubInvoice) -> String {
    let e = crate::booking::esc_html;
    let money = |x: f64| crate::i18n::money(lang, crate::cents::Cents::from_eur(x).0);
    let lines: String = i.lines.iter().map(|(d, a)| format!("<li>{} · {}</li>", e(d), money(*a))).collect();
    page(lang, clinic, &format!("<div class=\"card\"><h2>{} {}</h2><p class=\"mu\">{}</p><ul>{lines}</ul><p><b>{} {}</b></p></div>", e(t(lang, "Factura")), e(&i.code), fmt_date(i.date), e(t(lang, "Total")), money(i.total)))
}

/// Un documento firmado: tal como se firmó (en su idioma original).
pub fn doc_page(lang: Lang, clinic: &str, d: &PubDoc) -> String {
    let e = crate::booking::esc_html;
    page(lang, clinic, &format!("<div class=\"card\"><h2>{}</h2><p class=\"mu\">{} · {}</p><p style=\"white-space:pre-wrap\">{}</p></div>", e(&d.title), e(t(lang, "Firmado")), fmt_date(d.date), e(&d.body)))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 14).unwrap().and_hms_opt(10, 0, 0).unwrap()
    }

    /// T-R7-6. Una sesión del portal caduca a los `IDLE_MINUTES` sin uso y,
    /// aunque se use, a las `MAX_MINUTES`.
    #[test]
    fn a_portal_session_expires_when_idle_and_after_its_maximum() {
        let m = chrono::Duration::minutes;
        let s = Session { token: "t".into(), access_id: 1, started: now(), last: now() };
        assert!(s.alive(now() + m(IDLE_MINUTES - 1)));
        assert!(!s.alive(now() + m(IDLE_MINUTES)), "sin uso, caduca");
        let busy = Session { last: now() + m(MAX_MINUTES - 1), ..s.clone() };
        assert!(busy.alive(now() + m(MAX_MINUTES - 1)));
        assert!(!busy.alive(now() + m(MAX_MINUTES)), "aunque se use, caduca a las dos horas");
    }

    #[test]
    fn a_holder_sees_only_their_patients_and_a_responsible_sees_their_dependents() {
        let mut db = Db::sample(now().date());
        db.portal_access.clear();
        // dos menores a cargo de la misma madre, y otro paciente adulto
        for (pid, age) in [(105u32, 10), (106, 12)] {
            let p = db.patients.iter_mut().find(|p| p.id == pid).unwrap();
            p.birth = Some(now().date() - chrono::Duration::days(365 * age + 3));
            p.payer = Payer { name: "Marta Gil".into(), nif: "12345678-Z".into(), address: String::new(), relation: "Madre".into() };
        }
        let adult = db.patients.iter_mut().find(|p| p.id == 101).unwrap();
        (adult.nif, adult.birth) = ("11111111H".into(), Some(chrono::NaiveDate::from_ymd_opt(1980, 5, 1).unwrap()));
        assert!(db.portal_grant(Holder::Patient(105), now()).is_err(), "un menor de 16 no tiene acceso propio");
        let mom = db.portal_grant(Holder::Responsible("12345678Z".into()), now()).unwrap();
        let own = db.portal_grant(Holder::Patient(101), now()).unwrap();
        let pubs = publish_portal(&db, now().date());
        let get = |id: u32| pubs.values().find(|p| p.access_id == id).unwrap();
        assert_eq!(get(mom).patients.iter().map(|p| p.id).collect::<Vec<_>>(), vec![105, 106]);
        assert_eq!(get(own).patients.iter().map(|p| p.id).collect::<Vec<_>>(), vec![101]);
        // la entrada: el NIF del titular y la fecha de nacimiento (con o sin separadores)
        let token = db.portal_access.iter().find(|a| a.id == own).unwrap().token.clone();
        assert!(login_ok(&token, get(own), "11111111-h", "01/05/1980"));
        assert!(!login_ok(&token, get(own), "11111111H", "02/05/1980"));
        let mtok = db.portal_access.iter().find(|a| a.id == mom).unwrap().token.clone();
        let b105 = db.patient(105).unwrap().birth.unwrap().format("%d/%m/%Y").to_string();
        assert!(login_ok(&mtok, get(mom), "12345678Z", &b105));
        assert!(!login_ok(&token, get(mom), "12345678Z", &b105), "la huella va unida al enlace");
        // un adulto con el mismo responsable no entra (aunque el responsable pague sus facturas)
        let adult = db.patients.iter_mut().find(|p| p.id == 102).unwrap();
        (adult.payer, adult.birth) = (Payer { nif: "12345678Z".into(), ..Default::default() }, Some(chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap()));
        assert_eq!(publish_portal(&db, now().date()).values().find(|p| p.access_id == mom).unwrap().patients.len(), 2, "el adulto no sale");
        // un menor que se añade después tampoco, sin otro acceso
        let late = db.patients.iter_mut().find(|p| p.id == 107).unwrap();
        (late.payer, late.birth) = (Payer { nif: "12345678Z".into(), ..Default::default() }, Some(now().date() - chrono::Duration::days(365 * 5)));
        assert_eq!(publish_portal(&db, now().date()).values().find(|p| p.access_id == mom).unwrap().patients.len(), 2, "ni el que no estaba al darlo");
        // al cumplir 16, el responsable deja de verlo
        let later = now().date() + chrono::Duration::days(365 * 7);
        assert_eq!(publish_portal(&db, later).values().find(|p| p.access_id == mom).map(|p| p.patients.len()).unwrap_or(0), 0);
        // sin DNI en la ficha (borrado después), el acceso no se publica: no se entra con el DNI vacío
        db.patients.iter_mut().find(|p| p.id == 101).unwrap().nif.clear();
        assert!(!publish_portal(&db, now().date()).values().any(|p| p.access_id == own));
        assert!(!login_ok(&token, get(own), "", "01/05/1980"), "un DNI vacío nunca vale");
        // un acceso nuevo revoca el anterior del mismo titular; revocado, no se publica
        db.patients.iter_mut().find(|p| p.id == 101).unwrap().nif = "11111111H".into();
        let own2 = db.portal_grant(Holder::Patient(101), now()).unwrap();
        assert!(db.portal_access.iter().find(|a| a.id == own).unwrap().revoked.is_some());
        db.portal_revoke(own2, now());
        assert!(!publish_portal(&db, now().date()).values().any(|p| p.access_id == own || p.access_id == own2));
    }
}
