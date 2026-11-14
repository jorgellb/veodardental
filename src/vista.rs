//! Lo que recibe cada perfil del servidor de la clínica.
//!
//! Recepción no necesita los datos clínicos para su trabajo (citas, cobros,
//! facturas, presupuestos, documentos para firmar), así que el servidor no se
//! los manda: ni periodontogramas, imágenes y sus análisis, recetas y
//! ortodoncia, ni las respuestas de los cuestionarios de salud, ni las notas
//! clínicas de los pacientes y de los tratamientos. De los cuestionarios
//! recibe solo la fecha, quién lo rellenó y si está firmado, para pedirlo
//! cuando falta o ha caducado. Sí recibe los tratamientos con su precio (para
//! cobrar y facturar), las alertas y alergias (para avisar) y los documentos
//! (para que el paciente los firme, también por internet). Doctores,
//! higienistas y Dirección lo reciben todo.
//!
//! Lo que manda un puesto que no ve algo no puede borrarlo ni cambiarlo: el
//! servidor pone lo que no ve tal como está. En las listas que no recibe no
//! puede escribir nada (tampoco añadir: una receta o una imagen las crea quien
//! las ve). Los cuestionarios que rellenan los pacientes en la página de la
//! clínica sí los guarda, porque esa lista la ve (sin las respuestas).
//!
//! Un usuario de unos centros (ver `User::centers` y D-25) no recibe las
//! cajas de los demás, y lo que manda de los demás centros (citas, cajas,
//! cobros, citas pedidas por internet y huecos ofrecidos) no se acepta. Los
//! pacientes, su historia y su saldo son de toda la clínica.

use crate::users::{Perm, Profile, User};
use serde_json::Value;
use std::sync::Arc;

/// Listas de un centro: en las de los demás centros, un usuario de unos
/// centros no escribe.
pub const CENTER_LISTS: [&str; 5] = ["appointments", "cash", "payments", "booking_requests", "slot_offers"];

/// Listas de un centro que un usuario de otros centros no recibe.
const CENTER_HIDDEN: [&str; 1] = ["cash"];

/// Listas clínicas que no recibe quien no tiene el permiso clínico.
const CLINICAL_LISTS: [&str; 8] = ["perio", "images", "ia_reports", "prescriptions", "ortho", "evo_notes", "alert_acks", "sterile_packs"];

/// Campos clínicos de otros registros (se mandan vacíos). De los cuestionarios
/// de salud quedan la fecha, quién lo rellenó y si está firmado.
const CLINICAL_FIELDS: [(&str, &[&str]); 3] = [("patients", &["background", "notes"]), ("entries", &["notes"]), ("health", &["reason", "yes", "details", "cigarettes", "brushings", "last_visit", "aids", "notes"])];

/// Centro de un registro (sin centro, el principal).
pub fn center_of(json: &str) -> u32 {
    serde_json::from_str::<Value>(json).ok().and_then(|v| v["center_id"].as_u64()).map(|c| crate::model::cid(c as u32)).unwrap_or(1)
}

/// Un dato vacío del mismo tipo (texto vacío, cero, lista o mapa vacíos).
fn empty_like(v: &Value) -> Value {
    match v {
        Value::String(_) => Value::String(String::new()),
        Value::Number(_) => Value::from(0),
        Value::Bool(_) => Value::Bool(false),
        Value::Array(_) => Value::Array(vec![]),
        Value::Object(_) => Value::Object(serde_json::Map::new()),
        Value::Null => Value::Null,
    }
}

/// Qué ve un perfil.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    clinical: bool,
    /// Registro de jornada: solo el de este usuario (quien no es Dirección).
    own: Option<u32>,
    /// Centros en los que trabaja (ninguno: todos).
    centers: Option<Arc<Vec<u32>>>,
}

impl View {
    /// Todo (Dirección, doctores, higienistas; y sin servidor, todos).
    pub const FULL: View = View { clinical: true, own: None, centers: None };

    pub fn of(p: Profile) -> View {
        View { clinical: p.can(Perm::Clinico), own: None, centers: None }
    }

    /// Lo de su perfil y, si trabaja solo en unos centros, de esos centros.
    pub fn of_user(u: &User) -> View {
        let mut v = View::of(u.profile);
        if !u.centers.is_empty() {
            let mut c: Vec<u32> = u.centers.iter().map(|c| crate::model::cid(*c)).collect();
            c.sort_unstable();
            c.dedup();
            v.centers = Some(Arc::new(c));
        }
        v
    }

    /// Del registro de jornada, solo lo de `user`.
    pub fn only_own(self, user: u32) -> View {
        View { own: Some(user), ..self }
    }

    pub fn is_full(&self) -> bool {
        self.clinical && self.centers.is_none()
    }

    /// Centros a los que se limita (ninguno: todos).
    pub fn centers(&self) -> Option<&[u32]> {
        self.centers.as_deref().map(|c| c.as_slice())
    }

    /// ¿Trabaja en este centro? (0 es el principal)
    pub fn center_ok(&self, center: u32) -> bool {
        self.centers().is_none_or(|c| c.contains(&crate::model::cid(center)))
    }

    /// ¿Es un registro de un centro en el que no trabaja?
    pub fn other_center(&self, list: &str, json: &str) -> bool {
        self.centers.is_some() && CENTER_LISTS.contains(&list) && !self.center_ok(center_of(json))
    }

    /// ¿No recibe esta lista entera?
    pub fn hides(&self, list: &str) -> bool {
        !self.clinical && CLINICAL_LISTS.contains(&list)
    }

    fn hidden_fields(&self, list: &str) -> &'static [&'static str] {
        if self.clinical {
            return &[];
        }
        CLINICAL_FIELDS.iter().find(|(l, _)| *l == list).map(|(_, f)| *f).unwrap_or(&[])
    }

    /// Un registro para este perfil: `None` si no lo recibe; los campos que no
    /// ve, vacíos.
    pub fn show(&self, list: &str, json: &str) -> Option<String> {
        if self.hides(list) {
            return None;
        }
        if CENTER_HIDDEN.contains(&list) && self.other_center(list, json) {
            return None;
        }
        if list == "punches"
            && let Some(me) = self.own
            && serde_json::from_str::<Value>(json).ok().and_then(|v| v["user_id"].as_u64()) != Some(me as u64)
        {
            return None;
        }
        let fields = self.hidden_fields(list);
        if fields.is_empty() {
            return Some(json.to_string());
        }
        let mut v: Value = serde_json::from_str(json).ok()?;
        for f in fields {
            if let Some(x) = v.get(*f) {
                v[*f] = empty_like(x);
            }
        }
        crate::sync::canonical(list, &v.to_string()).ok()
    }

    /// Un registro que manda este perfil: lo que no ve, tal como está en el
    /// servidor (`current`), para que no lo borre ni lo cambie sin verlo.
    pub fn restore(&self, list: &str, json: &str, current: Option<&String>) -> Result<String, String> {
        let fields = self.hidden_fields(list);
        let Some(cur) = current.filter(|_| !fields.is_empty()) else { return Ok(json.to_string()) };
        let mut v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let c: Value = serde_json::from_str(cur).map_err(|e| e.to_string())?;
        for f in fields {
            if let Some(x) = c.get(*f) {
                v[*f] = x.clone();
            }
        }
        crate::sync::canonical(list, &v.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reception_does_not_get_clinical_data() {
        let reception = View::of(Profile::Recepcion);
        assert!(!reception.is_full());
        for list in ["perio", "images", "ia_reports", "prescriptions", "ortho", "evo_notes", "alert_acks", "sterile_packs"] {
            assert!(reception.hides(list), "{list}");
            assert_eq!(reception.show(list, "{}"), None);
        }
        for list in ["patients", "entries", "appointments", "budgets", "payments", "invoices", "documents"] {
            assert!(!reception.hides(list), "{list}");
        }
        for p in [Profile::Direccion, Profile::Doctor, Profile::Higienista] {
            assert!(View::of(p).is_full());
        }
        let db = crate::model::Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 4).unwrap());
        let mut p = db.patients[0].clone();
        p.background = "Hipertensión".into();
        p.notes = "Ansiedad en el sillón".into();
        p.alerts = "Anticoagulado".into();
        let json = serde_json::to_string(&p).unwrap();
        let seen: crate::model::Patient = serde_json::from_str(&reception.show("patients", &json).unwrap()).unwrap();
        assert_eq!((seen.background.as_str(), seen.notes.as_str()), ("", ""));
        assert_eq!(seen.alerts, "Anticoagulado", "las alertas sí");
        assert_eq!(seen.phone, p.phone);
        // al volver, lo que no ve queda como estaba
        let mut edited = seen.clone();
        edited.phone = "600000000".into();
        let back: crate::model::Patient = serde_json::from_str(&reception.restore("patients", &serde_json::to_string(&edited).unwrap(), Some(&json)).unwrap()).unwrap();
        assert_eq!((back.background.as_str(), back.notes.as_str(), back.phone.as_str()), ("Hipertensión", "Ansiedad en el sillón", "600000000"));
        assert_eq!(View::FULL.show("patients", &json).unwrap(), json);
        // de los cuestionarios, solo la fecha, quién y si está firmado
        let mut f = db.health[0].clone();
        f.notes = "Toma Sintrom".into();
        f.yes.insert("anticoagulantes".into(), true);
        f.signed = true;
        let json = serde_json::to_string(&f).unwrap();
        let seen: crate::health::HealthForm = serde_json::from_str(&reception.show("health", &json).unwrap()).unwrap();
        assert!(seen.notes.is_empty() && seen.yes.is_empty() && seen.details.is_empty() && seen.aids.is_empty());
        assert_eq!((seen.date, seen.signed, seen.patient_id), (f.date, true, f.patient_id));
    }
}
