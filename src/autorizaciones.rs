//! Autorizaciones de las aseguradoras (T-F6-04): la clínica pide a la
//! compañía que autorice unos tratamientos de un paciente asegurado, y la
//! compañía responde (aprueba, deniega o aprueba una parte) con un número de
//! autorización y, a veces, una fecha de caducidad.
//!
//! - **Sin duplicados:** un acto está como mucho en una solicitud viva (no
//!   denegada ni anulada). La solicitud la crea el servidor
//!   (`ChainOp::AuthRequest`), así que dos puestos no piden lo mismo a la vez.
//! - **Respuestas guardadas:** todas, con su fecha, quién las apuntó y el
//!   texto que mandó la compañía, también cuando corrige una anterior.
//! - **El número de autorización** pasa a los actos aprobados (y sale en la
//!   liquidación). Si la compañía lo deniega después, o se anula, se quita.
//! - **Caducidad:** pasada la fecha de validez, la autorización consta como
//!   caducada.
//! - **Canal:** el portal de la compañía o el correo, que se apuntan con la
//!   referencia de la compañía. La conexión directa con cada compañía está
//!   pendiente (B-20, T-F6-04X); el contrato está en `docs/aseguradoras.md`.

use crate::model::*;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthState {
    Borrador,
    Enviada,
    Aprobada,
    /// Aprobada solo una parte de los actos.
    Parcial,
    Denegada,
    Anulada,
}

impl AuthState {
    pub fn label(self) -> &'static str {
        match self {
            AuthState::Borrador => "Sin enviar",
            AuthState::Enviada => "Enviada",
            AuthState::Aprobada => "Aprobada",
            AuthState::Parcial => "Aprobada en parte",
            AuthState::Denegada => "Denegada",
            AuthState::Anulada => "Anulada",
        }
    }

    /// Ocupa sus actos (no se pueden pedir en otra).
    pub fn live(self) -> bool {
        !matches!(self, AuthState::Denegada | AuthState::Anulada)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthLine {
    pub entry_id: u32,
    pub code: String,
    pub description: String,
    pub teeth: String,
    /// Lo que respondió la compañía de este acto (ninguno: sin respuesta).
    #[serde(default)]
    pub approved: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthAnswer {
    pub at: NaiveDateTime,
    pub state: AuthState,
    pub number: String,
    #[serde(default)]
    pub valid_until: Option<NaiveDate>,
    /// Actos denegados (los demás, aprobados).
    #[serde(default)]
    pub denied: Vec<u32>,
    #[serde(default)]
    pub note: String,
    /// Lo que mandó la compañía, tal cual (acotado).
    #[serde(default)]
    pub raw: String,
    pub by: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthRequest {
    pub id: u32,
    pub patient_id: u32,
    pub tariff_id: u32,
    pub reference: String,
    pub lines: Vec<AuthLine>,
    pub state: AuthState,
    #[serde(default)]
    pub channel: String,
    /// Referencia de la compañía (la de su portal o su correo).
    #[serde(default)]
    pub external_ref: String,
    #[serde(default)]
    pub sent_at: Option<NaiveDateTime>,
    #[serde(default)]
    pub number: String,
    #[serde(default)]
    pub valid_until: Option<NaiveDate>,
    #[serde(default)]
    pub answers: Vec<AuthAnswer>,
    pub created: NaiveDateTime,
    pub by: String,
    #[serde(default)]
    pub cancelled: Option<(NaiveDateTime, String)>,
}

impl AuthRequest {
    /// Estado para mostrar (con la caducidad a la fecha `today`).
    pub fn shown(&self, today: NaiveDate) -> &'static str {
        if matches!(self.state, AuthState::Aprobada | AuthState::Parcial) && self.valid_until.is_some_and(|d| d < today) {
            return "Caducada";
        }
        self.state.label()
    }
}

/// Lo más largo que se guarda de una respuesta de la compañía.
pub const RAW_MAX: usize = 8000;

impl Db {
    /// La solicitud viva en la que está un acto (si alguna).
    pub fn auth_for_entry(&self, entry_id: u32) -> Option<&AuthRequest> {
        self.auth_requests.iter().find(|r| r.state.live() && r.lines.iter().any(|l| l.entry_id == entry_id && l.approved != Some(false)))
    }

    /// Solicitud nueva (sin enviar) para estos actos del paciente, a su compañía.
    pub fn auth_new(&mut self, pid: u32, entries: &[u32], now: NaiveDateTime) -> Result<u32, String> {
        let p = self.patient(pid).cloned().ok_or("El paciente ya no existe.")?;
        let tariff = self.tariff(p.tariff_id).filter(|t| t.is_insurer).map(|t| t.id).ok_or("El paciente no es de una compañía aseguradora (Filiación › Tarifa).")?;
        if entries.is_empty() {
            return Err("Elija los tratamientos que hay que autorizar.".into());
        }
        let mut lines = vec![];
        for (i, eid) in entries.iter().enumerate() {
            if entries[..i].contains(eid) {
                return Err("Hay un tratamiento repetido.".into());
            }
            let e = self.entries.iter().find(|e| e.id == *eid && e.patient_id == pid).ok_or("Uno de los tratamientos no es de este paciente.")?;
            if self.claimed(e.id) {
                return Err(format!("«{}» ya está en una liquidación.", e.description));
            }
            if let Some(r) = self.auth_for_entry(e.id) {
                return Err(format!("«{}» ya está en la solicitud {} ({}).", e.description, r.reference, r.state.label().to_lowercase()));
            }
            lines.push(AuthLine { entry_id: e.id, code: e.code.clone(), description: e.description.clone(), teeth: e.teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join("-"), approved: None });
        }
        let id = self.new_id();
        let by = self.config.current_user.clone();
        self.auth_requests.push(AuthRequest {
            id,
            patient_id: pid,
            tariff_id: tariff,
            reference: format!("AUT-{id}"),
            lines,
            state: AuthState::Borrador,
            channel: String::new(),
            external_ref: String::new(),
            sent_at: None,
            number: String::new(),
            valid_until: None,
            answers: vec![],
            created: now,
            by,
            cancelled: None,
        });
        let who = self.patient_name(pid);
        let ins = self.tariff_name(tariff);
        self.audit("Autorización pedida", &format!("{who} · {ins} · {} actos", entries.len()));
        Ok(id)
    }

    /// Enviada a la compañía (por su portal o por correo), con su referencia.
    pub fn auth_sent(&mut self, id: u32, channel: &str, external_ref: &str, now: NaiveDateTime) -> Result<(), String> {
        let r = self.auth_requests.iter_mut().find(|r| r.id == id).ok_or("La solicitud ya no existe.")?;
        if !matches!(r.state, AuthState::Borrador | AuthState::Enviada) {
            return Err("Esta solicitud ya tiene respuesta.".into());
        }
        if channel.trim().is_empty() {
            return Err("Diga por dónde se envió (portal, correo…).".into());
        }
        r.state = AuthState::Enviada;
        r.channel = channel.trim().chars().take(40).collect();
        if !external_ref.trim().is_empty() {
            r.external_ref = external_ref.trim().chars().take(60).collect();
        }
        r.sent_at = Some(now);
        Ok(())
    }

    /// Respuesta de la compañía: aprobada (todo menos `denied`) o denegada
    /// (todo). Se guarda con las anteriores; el número pasa a los actos aprobados.
    #[allow(clippy::too_many_arguments)]
    pub fn auth_answer(&mut self, id: u32, approved: bool, number: &str, denied: &[u32], valid_until: Option<NaiveDate>, note: &str, raw: &str, now: NaiveDateTime) -> Result<AuthState, String> {
        let r = self.auth_requests.iter().find(|r| r.id == id).cloned().ok_or("La solicitud ya no existe.")?;
        if matches!(r.state, AuthState::Borrador | AuthState::Anulada) {
            return Err(if r.state == AuthState::Borrador { "Primero apunte que se ha enviado.".into() } else { "Esta solicitud está anulada.".into() });
        }
        if denied.iter().any(|d| !r.lines.iter().any(|l| l.entry_id == *d)) {
            return Err("Uno de los actos denegados no está en esta solicitud.".into());
        }
        let all_denied = !approved || r.lines.iter().all(|l| denied.contains(&l.entry_id));
        if !all_denied && number.trim().is_empty() {
            return Err("Escriba el número de autorización de la compañía.".into());
        }
        let state = if all_denied {
            AuthState::Denegada
        } else if denied.is_empty() {
            AuthState::Aprobada
        } else {
            AuthState::Parcial
        };
        let number = if all_denied { String::new() } else { number.trim().chars().take(40).collect::<String>() };
        let by = self.config.current_user.clone();
        // el número, en los actos aprobados; fuera de los denegados (si era el de esta)
        let old = r.number.clone();
        for l in &r.lines {
            let ok = !all_denied && !denied.contains(&l.entry_id);
            if ok {
                self.set_auth_no(l.entry_id, &number);
            } else if !old.is_empty() && self.auth_no(l.entry_id) == old {
                self.set_auth_no(l.entry_id, "");
            }
        }
        let x = self.auth_requests.iter_mut().find(|x| x.id == id).ok_or("La solicitud ya no existe.")?;
        for l in x.lines.iter_mut() {
            l.approved = Some(!all_denied && !denied.contains(&l.entry_id));
        }
        x.state = state;
        x.number = number.clone();
        x.valid_until = if all_denied { None } else { valid_until };
        x.answers.push(AuthAnswer { at: now, state, number, valid_until, denied: denied.to_vec(), note: note.trim().chars().take(300).collect(), raw: raw.chars().take(RAW_MAX).collect(), by });
        let who = self.patient_name(r.patient_id);
        self.audit("Respuesta de autorización", &format!("{who} · {} · {}", r.reference, state.label()));
        Ok(state)
    }

    /// Anular la solicitud (con motivo): sus actos quedan libres y sin su número.
    pub fn auth_cancel(&mut self, id: u32, reason: &str, now: NaiveDateTime) -> Result<(), String> {
        let r = self.auth_requests.iter().find(|r| r.id == id).cloned().ok_or("La solicitud ya no existe.")?;
        if r.state == AuthState::Anulada {
            return Err("Ya está anulada.".into());
        }
        if reason.trim().is_empty() {
            return Err("Escriba el motivo.".into());
        }
        if r.lines.iter().any(|l| self.claimed(l.entry_id)) {
            return Err("Algún acto ya está en una liquidación: no se anula.".into());
        }
        for l in &r.lines {
            if !r.number.is_empty() && self.auth_no(l.entry_id) == r.number {
                self.set_auth_no(l.entry_id, "");
            }
        }
        if let Some(x) = self.auth_requests.iter_mut().find(|x| x.id == id) {
            x.state = AuthState::Anulada;
            x.cancelled = Some((now, reason.trim().chars().take(200).collect()));
        }
        self.audit("Autorización anulada", &format!("{} · {}", r.reference, reason.trim()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(d: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap().and_hms_opt(10, 0, 0).unwrap()
    }

    /// Un paciente de una compañía con tres tratamientos previstos.
    fn insured() -> (Db, u32, Vec<u32>) {
        let mut db = Db::sample(at(2).date());
        db.auth_requests.clear();
        let t = db.tariffs.iter().find(|t| t.is_insurer).unwrap().id;
        let pid = db.patients[0].id;
        db.patients.iter_mut().find(|p| p.id == pid).unwrap().tariff_id = t;
        let code = db.tariffs.iter().find(|x| x.id == t).unwrap().prices.iter().find(|p| p.insurer_fee > 0.0).unwrap().code.clone();
        let ids = (0..3).map(|_| db.add_entry(pid, &code, vec![36], vec![], TxState::Previsto, 1, at(2).date())).collect();
        (db, pid, ids)
    }

    #[test]
    fn an_authorization_is_asked_once_and_its_answers_are_kept() {
        let (mut db, pid, e) = insured();
        let id = db.auth_new(pid, &e[..2], at(2)).unwrap();
        assert!(db.auth_new(pid, &e[1..], at(2)).unwrap_err().contains("ya está en la solicitud"), "un acto, en una sola solicitud viva");
        assert!(db.auth_new(pid, &[e[2], e[2]], at(2)).is_err(), "sin repetir");
        assert!(db.auth_answer(id, true, "N1", &[], None, "", "", at(3)).is_err(), "primero, enviada");
        assert!(db.auth_sent(id, " ", "", at(2)).is_err());
        db.auth_sent(id, "Portal de la compañía", "WEB-55", at(2)).unwrap();
        // aprobada en parte, con caducidad: el número solo en lo aprobado
        let valid = Some(at(30).date());
        assert!(db.auth_answer(id, true, "  ", &[e[1]], valid, "", "", at(3)).is_err(), "con su número");
        assert_eq!(db.auth_answer(id, true, "AUT-778", &[e[1]], valid, "Falta radiografía", "{\"estado\":\"parcial\"}", at(3)).unwrap(), AuthState::Parcial);
        assert_eq!((db.auth_no(e[0]).as_str(), db.auth_no(e[1]).as_str()), ("AUT-778", ""));
        // el denegado queda libre para otra solicitud
        let again = db.auth_new(pid, &[e[1]], at(4)).unwrap();
        db.auth_sent(again, "Correo", "", at(4)).unwrap();
        // la compañía corrige: ahora todo aprobado; se guardan las dos respuestas
        assert_eq!(db.auth_answer(id, true, "AUT-779", &[], valid, "Corrige", "", at(5)).unwrap(), AuthState::Aprobada);
        let r = db.auth_requests.iter().find(|r| r.id == id).unwrap().clone();
        assert_eq!(r.answers.len(), 2, "las dos respuestas");
        assert_eq!(r.answers[0].raw, "{\"estado\":\"parcial\"}", "lo que mandó la compañía, tal cual");
        assert_eq!(db.auth_no(e[0]), "AUT-779");
        assert_eq!(r.shown(at(20).date()), "Aprobada");
        assert_eq!(r.shown(at(30).date() + chrono::Duration::days(1)), "Caducada");
        // denegada después: fuera los números
        assert_eq!(db.auth_answer(id, false, "", &[], None, "Revisada", "", at(6)).unwrap(), AuthState::Denegada);
        assert_eq!(db.auth_no(e[0]), "");
        assert!(db.auth_for_entry(e[0]).is_none(), "denegada, el acto queda libre");
        // anular: con motivo, una vez, y los actos sin su número
        assert_eq!(db.auth_answer(again, true, "C-1", &[], None, "", "", at(7)).unwrap(), AuthState::Aprobada);
        assert!(db.auth_cancel(again, " ", at(8)).is_err());
        db.auth_cancel(again, "El paciente no se lo hace", at(8)).unwrap();
        assert!(db.auth_cancel(again, "otra", at(8)).is_err());
        assert_eq!(db.auth_no(e[1]), "");
        assert!(db.audit_log.iter().any(|a| a.action == "Respuesta de autorización"));
    }

    #[test]
    fn only_insured_patients_and_their_own_treatments() {
        let (mut db, pid, e) = insured();
        let other = db.patients.iter().find(|p| p.id != pid && !db.tariff(p.tariff_id).is_some_and(|t| t.is_insurer)).unwrap().id;
        assert!(db.auth_new(other, &e, at(2)).unwrap_err().contains("compañía"));
        let not_mine = db.entries.iter().find(|x| x.patient_id != pid).unwrap().id;
        assert!(db.auth_new(pid, &[e[0], not_mine], at(2)).is_err());
        assert!(db.auth_new(pid, &[], at(2)).is_err());
    }
}
