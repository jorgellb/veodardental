//! Sincronización registro a registro con el servidor de la clínica.
//!
//! Los datos se parten en registros: cada lista con clave (pacientes, citas,
//! cobros… por su `id`; los enlaces, por su `token`) va registro a registro, y
//! lo demás (configuración, registros de accesos…) va por campos enteros.
//! Cada registro se guarda como su texto JSON compacto: así se comparan sin
//! convertir los datos enteros y ocupan en memoria lo mismo que en el disco.
//!
//! - El puesto envía solo lo que ha cambiado, cada cosa con cómo estaba
//!   (`antes`) y cómo está (`ahora`). Con eso el servidor combina cambios de
//!   varios puestos sin guardar copias antiguas de los datos.
//! - El servidor devuelve solo lo que ha cambiado desde la última vez que el
//!   puesto habló con él, y los puestos recogen así lo de los demás.

use crate::model::Db;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Campos de los datos de la clínica: las listas de registros (con el campo
/// que hace de clave) y los que van enteros. Si se añade un campo a [`Db`],
/// hay que ponerlo aquí (lo comprueba una prueba).
macro_rules! db_fields {
    (records: [$(($rf:ident, $rk:ident)),* $(,)?], plain: [$($pf:ident),* $(,)?] $(,)?) => {
        /// Listas que se sincronizan registro a registro.
        pub const RECORD_FIELDS: &[&str] = &[$(stringify!($rf)),*];
        /// Campos que se sincronizan enteros.
        pub const PLAIN_FIELDS: &[&str] = &[$(stringify!($pf)),*];

        fn records_of(db: &Db, field: &str) -> Option<Records> {
            match field {
                $(stringify!($rf) => Some(Records::collect(db.$rf.iter().map(|r| (to_json(&r.$rk), to_json(r))))),)*
                _ => None,
            }
        }

        fn plain_of(db: &Db, field: &str) -> Option<String> {
            match field {
                $(stringify!($pf) => Some(to_json(&db.$pf)),)*
                _ => None,
            }
        }

        /// Texto de un registro de los datos (si está).
        pub fn record_of(db: &Db, field: &str, key: &str) -> Option<String> {
            match field {
                $(stringify!($rf) => {
                    let want = parse_key(&db.$rf, |r| &r.$rk, key).ok()?;
                    db.$rf.iter().find(|r| r.$rk == want).map(to_json)
                })*
                _ => None,
            }
        }

        /// Pone (o, con `None`, quita) un registro en los datos.
        pub fn set_record(db: &mut Db, field: &str, key: &str, json: Option<&str>) -> Result<(), String> {
            match field {
                $(stringify!($rf) => {
                    let want = parse_key(&db.$rf, |r| &r.$rk, key).map_err(|e| format!("{field}: clave {key}: {e}"))?;
                    // lo que llega del servidor no cuenta como cambio del puesto
                    let list = db.$rf.untracked_mut();
                    let pos = list.iter().position(|r| r.$rk == want);
                    match (json, pos) {
                        (Some(j), Some(i)) => list[i] = serde_json::from_str(j).map_err(|e| format!("{field}: {e}"))?,
                        (Some(j), None) => list.push(serde_json::from_str(j).map_err(|e| format!("{field}: {e}"))?),
                        (None, Some(i)) => {
                            list.remove(i);
                        }
                        (None, None) => {}
                    }
                    Ok(())
                })*
                _ => Err(format!("campo desconocido: {field}")),
            }
        }

        /// Pone un campo entero (una lista de registros o un campo normal).
        pub fn set_field(db: &mut Db, field: &str, json: &str) -> Result<(), String> {
            match field {
                $(stringify!($rf) => db.$rf = serde_json::from_str(json).map_err(|e| format!("{field}: {e}"))?,)*
                $(stringify!($pf) => db.$pf = serde_json::from_str(json).map_err(|e| format!("{field}: {e}"))?,)*
                _ => return Err(format!("campo desconocido: {field}")),
            }
            Ok(())
        }

        /// El registro (o campo) tal como lo escribe el programa: comprueba que
        /// sirve para sus datos y deja los campos en su orden (para comparar textos).
        pub fn canonical(field: &str, json: &str) -> Result<String, String> {
            match field {
                $(stringify!($rf) => parse_item(|d: &Db| &d.$rf, json),)*
                $(stringify!($pf) => parse_value(|d: &Db| &d.$pf, json),)*
                _ => Err(format!("campo desconocido: {field}")),
            }
            .map_err(|e| format!("{field}: {e}"))
        }

        /// Lo que ha cambiado en `db` respecto a `synced`, sin partir los datos
        /// enteros: cada registro se escribe en el mismo búfer y se compara.
        pub fn diff_db(synced: &Snapshot, db: &Db) -> Vec<Change> {
            let mut out = vec![];
            let empty = Records::default();
            let mut buf: Vec<u8> = Vec::with_capacity(4096);
            $({
                let field = stringify!($rf);
                let b = synced.records.get(field).unwrap_or(&empty);
                let mut matched = 0usize;
                // las listas que nadie ha tocado desde el último envío no han cambiado
                if db.$rf.is_touched() {
                for r in db.$rf.iter() {
                    let k = to_json(&r.$rk);
                    buf.clear();
                    if serde_json::to_writer(&mut buf, r).is_err() {
                        continue;
                    }
                    let v = std::str::from_utf8(&buf).unwrap_or("");
                    match b.get(&k) {
                        Some(old) => {
                            matched += 1;
                            if old.as_str() != v {
                                out.push(Change { field: field.to_string(), key: Some(k), before: Some(old.clone()), after: Some(v.to_string()) });
                            }
                        }
                        None => out.push(Change { field: field.to_string(), key: Some(k), before: None, after: Some(v.to_string()) }),
                    }
                }
                // borrados (solo se buscan si faltan registros)
                if matched < b.map.len() {
                    let here: std::collections::HashSet<String> = db.$rf.iter().map(|r| to_json(&r.$rk)).collect();
                    for (k, v) in b.iter().filter(|(k, _)| !here.contains(*k)) {
                        out.push(Change { field: field.to_string(), key: Some(k.clone()), before: Some(v.clone()), after: None });
                    }
                }
                }
            })*
            $({
                let field = stringify!($pf);
                if field != "rev" {
                    let now = to_json(&db.$pf);
                    let before = synced.plain.get(field);
                    if before != Some(&now) {
                        out.push(Change { field: field.to_string(), key: None, before: before.cloned(), after: Some(now) });
                    }
                }
            })*
            out
        }

        /// Todo lo que hay en `db` ya está en el servidor: las listas dejan de
        /// estar marcadas como tocadas.
        pub fn mark_synced(db: &mut Db) {
            $(db.$rf.clear_touched();)*
        }

        /// Campo que hace de clave de los registros de una lista (`None` si el
        /// campo no es una lista con clave).
        pub fn key_field(field: &str) -> Option<&'static str> {
            match field {
                $(stringify!($rf) => Some(stringify!($rk)),)*
                _ => None,
            }
        }

        /// Quita todos los registros (para unos datos mínimos).
        fn clear_records(db: &mut Db) {
            $(db.$rf.clear();)*
        }
    };
}

db_fields! {
    records: [
        (tariffs, id), (collaborators, id), (boxes, id), (patients, id), (entries, id), (budgets, id),
        (payments, id), (cash, id), (appointments, id), (blocks, id), (waitlist, id), (recalls, id),
        (invoices, id), (perio, id), (images, id), (connectors, id), (msg_templates, id), (messages, id),
        (doc_templates, id), (documents, id), (suppliers, id), (products, id), (stock_moves, id),
        (orders, id), (lab_works, id), (users, id), (expenses, id), (prescriptions, id), (ortho, id),
        (remittances, id), (contacts, id), (bank_accounts, id), (bank_moves, id), (centers, id),
        (health, id), (claims, id), (tasks, id), (stations, id), (chat, id), (calls, id), (whatsapp, id),
        (vf_records, id), (ia_reports, id), (booking_requests, id), (slot_offers, id), (imports, id), (measures, id), (rights_requests, id), (punches, id),
        (appt_links, token), (budget_links, token), (doc_links, token), (rx_links, token),
    ],
    plain: [
        config, treatments, next_id, launches, consumptions, access_log, drugs, audit_log, holidays,
        auths, erx, rev, wa_seen, wa_sender, verifactu, mail, ia, booking, golive, data_version,
    ],
}

/// Lista de registros que sabe si se ha tocado desde que se envió al servidor:
/// cualquier acceso para cambiarla (añadir, quitar, `iter_mut`, `[i] = …`) la
/// marca. Así, al guardar, solo se miran las listas tocadas y no los datos
/// enteros. Se lee y se cambia como la lista de dentro.
#[derive(Clone, Default)]
pub struct Tracked<T> {
    inner: T,
    touched: bool,
}

impl<T> Tracked<T> {
    pub fn is_touched(&self) -> bool {
        self.touched
    }

    /// Para cambiarla sin marcarla (al poner lo que llega del servidor, que
    /// ya queda apuntado como sincronizado).
    pub fn untracked_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    fn clear_touched(&mut self) {
        self.touched = false;
    }
}

impl<T> std::ops::Deref for Tracked<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> std::ops::DerefMut for Tracked<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.touched = true;
        &mut self.inner
    }
}

impl<T> From<T> for Tracked<T> {
    fn from(inner: T) -> Self {
        Tracked { inner, touched: true }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Tracked<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: PartialEq> PartialEq for Tracked<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T: Serialize> Serialize for Tracked<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.inner.serialize(s)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Tracked<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(Tracked::from)
    }
}

impl<T> FromIterator<T> for Tracked<Vec<T>> {
    fn from_iter<I: IntoIterator<Item = T>>(it: I) -> Self {
        Tracked::from(it.into_iter().collect::<Vec<T>>())
    }
}

impl<'a, T> IntoIterator for &'a Tracked<Vec<T>> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Tracked<Vec<T>> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.touched = true;
        self.inner.iter_mut()
    }
}

impl<T> IntoIterator for Tracked<Vec<T>> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

fn to_json<T: Serialize + ?Sized>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

/// La clave de un registro, con su tipo (el del campo `get` de los registros de la lista).
fn parse_key<T, K: DeserializeOwned>(_: &[T], _get: fn(&T) -> &K, key: &str) -> Result<K, String> {
    serde_json::from_str(key).map_err(|e| e.to_string())
}

fn parse_item<T: DeserializeOwned + Serialize>(_: fn(&Db) -> &Vec<T>, json: &str) -> Result<String, String> {
    serde_json::from_str::<T>(json).map(|v| to_json(&v)).map_err(|e| e.to_string())
}

fn parse_value<T: DeserializeOwned + Serialize>(_: fn(&Db) -> &T, json: &str) -> Result<String, String> {
    serde_json::from_str::<T>(json).map(|v| to_json(&v)).map_err(|e| e.to_string())
}

/// Registros de una lista, en su orden, por su clave (texto JSON de la clave).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Records {
    order: Vec<String>,
    map: HashMap<String, String>,
}

impl Records {
    fn collect(it: impl Iterator<Item = (String, String)>) -> Records {
        let mut r = Records::default();
        for (k, v) in it {
            // si una clave se repitiera, vale el primero
            if !r.map.contains_key(&k) {
                r.order.push(k.clone());
                r.map.insert(k, v);
            }
        }
        r
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.map.get(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.order.iter().filter_map(|k| self.map.get(k).map(|v| (k, v)))
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.map.contains_key(key).then(|| self.order.iter().position(|k| k == key)).flatten()
    }

    /// Pone el registro en el sitio `pos` de la lista (o lo cambia si ya está).
    fn put_at(&mut self, key: &str, json: String, pos: usize) {
        if self.map.insert(key.to_string(), json).is_none() {
            self.order.insert(pos.min(self.order.len()), key.to_string());
        }
    }

    fn set(&mut self, key: &str, json: Option<String>) {
        match json {
            Some(j) => {
                if self.map.insert(key.to_string(), j).is_none() {
                    self.order.push(key.to_string());
                }
            }
            None => {
                if self.map.remove(key).is_some() {
                    self.order.retain(|k| k != key);
                }
            }
        }
    }

    /// La lista como texto JSON, pasando cada registro por `f` (que puede quitarlo).
    fn to_json_with(&self, f: &dyn Fn(&str) -> Option<String>) -> String {
        let mut s = String::with_capacity(self.map.values().map(|v| v.len() + 1).sum::<usize>() + 2);
        s.push('[');
        let mut first = true;
        for (_, v) in self.iter() {
            let Some(r) = f(v) else { continue };
            if !first {
                s.push(',');
            }
            first = false;
            s.push_str(&r);
        }
        s.push(']');
        s
    }
}

/// Los datos de la clínica partidos en registros y campos (texto JSON).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub records: HashMap<String, Records>,
    pub plain: HashMap<String, String>,
}

impl Snapshot {
    pub fn from_db(db: &Db) -> Snapshot {
        Snapshot { records: RECORD_FIELDS.iter().filter_map(|f| Some((f.to_string(), records_of(db, f)?))).collect(), plain: PLAIN_FIELDS.iter().filter_map(|f| Some((f.to_string(), plain_of(db, f)?))).collect() }
    }

    /// Los datos enteros como texto JSON, pasando cada registro por `f(lista,
    /// registro)`, que puede cambiarlo o quitarlo (para mandarlos a un puesto
    /// sin contraseñas y sin lo que su perfil no ve).
    pub fn to_json_with(&self, f: &dyn Fn(&str, &str) -> Option<String>) -> String {
        let mut out = String::from("{");
        let mut first = true;
        let mut put = |name: &str, value: String| {
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&to_json(name));
            out.push(':');
            out.push_str(&value);
        };
        let empty = Records::default();
        for name in RECORD_FIELDS {
            let r = self.records.get(*name).unwrap_or(&empty);
            put(name, r.to_json_with(&|rec| f(name, rec)));
        }
        for name in PLAIN_FIELDS {
            if let Some(v) = self.plain.get(*name) {
                put(name, v.clone());
            }
        }
        out.push('}');
        out
    }

    pub fn to_json(&self) -> String {
        self.to_json_with(&|_, rec| Some(rec.to_string()))
    }

    pub fn to_db(&self) -> Result<Db, String> {
        serde_json::from_str(&self.to_json()).map_err(|e| format!("datos no válidos: {e}"))
    }

    pub fn rev(&self) -> u64 {
        self.plain.get("rev").and_then(|r| r.parse().ok()).unwrap_or(0)
    }

    pub fn set_rev(&mut self, rev: u64) {
        self.plain.insert("rev".into(), rev.to_string());
    }

    /// Un registro (con clave) o un campo entero (sin ella).
    pub fn get(&self, field: &str, key: Option<&str>) -> Option<&String> {
        match key {
            Some(k) => self.records.get(field)?.get(k),
            None => self.plain.get(field),
        }
    }

    pub fn set(&mut self, field: &str, key: Option<&str>, json: Option<String>) {
        match key {
            Some(k) => self.records.entry(field.to_string()).or_default().set(k, json),
            None => match json {
                Some(j) => {
                    self.plain.insert(field.to_string(), j);
                }
                None => {
                    self.plain.remove(field);
                }
            },
        }
    }
}

/// Un cambio: un registro (con `key`) o un campo entero, cómo estaba y cómo está.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Change {
    #[serde(rename = "campo")]
    pub field: String,
    #[serde(rename = "clave", default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(rename = "antes", default)]
    pub before: Option<String>,
    #[serde(rename = "ahora", default)]
    pub after: Option<String>,
}

/// Lo que ha cambiado de `base` a `ours` (la revisión no cuenta: la pone el servidor).
pub fn diff(base: &Snapshot, ours: &Snapshot) -> Vec<Change> {
    let mut out = vec![];
    let empty = Records::default();
    for f in RECORD_FIELDS {
        let (b, o) = (base.records.get(*f).unwrap_or(&empty), ours.records.get(*f).unwrap_or(&empty));
        for (k, v) in o.iter() {
            if b.get(k) != Some(v) {
                out.push(Change { field: f.to_string(), key: Some(k.clone()), before: b.get(k).cloned(), after: Some(v.clone()) });
            }
        }
        for (k, v) in b.iter() {
            if o.get(k).is_none() {
                out.push(Change { field: f.to_string(), key: Some(k.clone()), before: Some(v.clone()), after: None });
            }
        }
    }
    for f in PLAIN_FIELDS.iter().filter(|f| **f != "rev") {
        if base.plain.get(*f) != ours.plain.get(*f) {
            out.push(Change { field: f.to_string(), key: None, before: base.plain.get(*f).cloned(), after: ours.plain.get(*f).cloned() });
        }
    }
    out
}

fn parse(s: &str) -> Result<Value, String> {
    serde_json::from_str(s).map_err(|e| e.to_string())
}

/// Qué se ha tocado: campo y, si es un registro, su clave.
pub type Touched = Vec<(String, Option<String>)>;

/// Resultado de aplicar los cambios de un puesto.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Qué se ha tocado (campo y clave del registro).
    pub touched: Touched,
    /// Datos que ambos lados cambiaron a la vez (gana el puesto que envía).
    pub conflicts: usize,
    /// Cómo estaba cada cosa tocada (para deshacerlo si algo falla después).
    pub previous: Undo,
}

/// Cómo estaba una cosa antes de tocarla: campo, clave, valor y, si era un
/// registro, su sitio en la lista.
pub type UndoEntry = (String, Option<String>, Option<String>, Option<usize>);
pub type Undo = Vec<UndoEntry>;

/// Cómo está ahora `field`/`key` (para poder volver a ello).
pub fn before(cur: &Snapshot, field: &str, key: Option<&str>) -> UndoEntry {
    let pos = key.and_then(|k| cur.records.get(field)?.position(k));
    (field.to_string(), key.map(str::to_string), cur.get(field, key).cloned(), pos)
}

/// Deshace lo tocado (en orden inverso), con cada registro en su sitio.
pub fn undo(cur: &mut Snapshot, previous: Undo) {
    for (field, key, value, pos) in previous.into_iter().rev() {
        match (&key, pos) {
            (Some(k), Some(p)) if value.is_some() => cur.records.entry(field).or_default().put_at(k, value.unwrap_or_default(), p),
            _ => cur.set(&field, key.as_deref(), value),
        }
    }
}

/// ¿Son coherentes los cambios? Cada registro tiene que llevar dentro la misma
/// clave con la que viene (si no, se colarían dos registros con el mismo `id`),
/// las listas con clave van registro a registro y los demás campos, enteros.
pub fn check_keys(changes: &[Change]) -> Result<(), String> {
    for ch in changes {
        match (key_field(&ch.field), &ch.key) {
            (Some(kf), Some(key)) => {
                for json in [&ch.before, &ch.after].into_iter().flatten() {
                    let inner = parse(json)?.get(kf).map(|k| k.to_string());
                    if inner.as_deref() != Some(key.as_str()) {
                        return Err(format!("{}: el registro {key} lleva dentro otra clave", ch.field));
                    }
                }
            }
            (None, None) if PLAIN_FIELDS.contains(&ch.field.as_str()) => {}
            _ => return Err(format!("{}: cambio no válido", ch.field)),
        }
    }
    Ok(())
}

/// Aplica en el servidor los cambios de un puesto sobre los datos actuales,
/// combinándolos con lo que otros puestos hayan cambiado mientras (como
/// [`crate::shared::merge`], registro a registro). Si algún registro no es
/// válido no se aplica nada.
pub fn apply(cur: &mut Snapshot, changes: &[Change]) -> Result<Outcome, String> {
    check_keys(changes)?;
    let mut out = Outcome::default();
    let mut results: Vec<(String, Option<String>, Option<String>)> = vec![];
    for ch in changes {
        if ch.field == "rev" {
            continue;
        }
        let current = cur.get(&ch.field, ch.key.as_deref()).cloned();
        let result = if ch.key.is_none() && ch.field == "next_id" {
            // los identificadores nunca bajan
            let n = |s: &Option<String>| s.as_deref().and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
            Some(n(&ch.after).max(n(&current)).to_string())
        } else if let (None, Some(cap)) = (&ch.key, crate::shared::append_only_cap(&ch.field)) {
            // registros que solo crecen: se juntan las entradas nuevas de los dos
            let arr = |s: &Option<String>| s.as_deref().and_then(|x| serde_json::from_str::<Vec<Value>>(x).ok()).unwrap_or_default();
            let before = ch.before.as_deref().map(parse).transpose()?;
            Some(crate::shared::merge_append(before.as_ref(), &arr(&ch.after), &arr(&current), cap).to_string())
        } else if current == ch.before {
            // nadie lo ha tocado mientras
            ch.after.clone()
        } else {
            match (&ch.before, &ch.after, &current) {
                // lo ha borrado este puesto pero otro lo cambió: se conserva
                (_, None, Some(_)) => {
                    out.conflicts += 1;
                    current.clone()
                }
                // otro lo borró: si este no lo había cambiado, queda borrado
                (Some(b), Some(a), None) if a == b => None,
                (_, Some(a), None) => {
                    out.conflicts += 1;
                    Some(a.clone())
                }
                // los dos lo cambiaron: dato a dato
                (b, Some(a), Some(c)) => {
                    let b = b.as_deref().map(parse).transpose()?;
                    let (a, c) = (parse(a)?, parse(c)?);
                    let m = crate::shared::merge(b.as_ref(), &a, &c, &mut out.conflicts);
                    Some(m.to_string())
                }
                (_, None, None) => None,
            }
        };
        let result = result.map(|r| canonical(&ch.field, &r)).transpose()?;
        if result != current {
            results.push((ch.field.clone(), ch.key.clone(), result));
        }
    }
    for (field, key, value) in results {
        out.previous.push(before(cur, &field, key.as_deref()));
        cur.set(&field, key.as_deref(), value);
        out.touched.push((field, key));
    }
    Ok(out)
}

/// Pone en los datos del puesto lo que llega del servidor, y lo apunta en
/// `synced` (lo último que se sabe del servidor) tal como lo escribiría el
/// puesto, para que no parezca un cambio suyo.
pub fn apply_to_db(db: &mut Db, synced: &mut Snapshot, changes: &[Change]) -> Result<(), String> {
    for ch in changes {
        match &ch.key {
            Some(k) => {
                set_record(db, &ch.field, k, ch.after.as_deref())?;
                synced.set(&ch.field, Some(k), record_of(db, &ch.field, k));
            }
            None => {
                if let Some(a) = &ch.after {
                    set_field(db, &ch.field, a)?;
                    synced.set(&ch.field, None, plain_of(db, &ch.field));
                }
            }
        }
    }
    Ok(())
}

// ───────────────────────────── diario ─────────────────────────────
//
// El servidor no reescribe el fichero de datos entero en cada guardado: apunta
// en el diario (`clinica.cambios`, una línea por revisión) cómo han quedado
// los registros tocados, y el fichero entero se reescribe de vez en cuando.
// Al abrir los datos, lo del diario que sea más nuevo que el fichero se pone
// en los datos (por si el servidor se cerró sin reescribirlo).

/// Diario de cambios de un fichero de datos.
pub fn journal_path(data: &std::path::Path) -> std::path::PathBuf {
    data.with_extension("cambios")
}

/// Línea del diario: la revisión y cómo han quedado los registros tocados.
pub fn journal_line(rev: u64, snap: &Snapshot, touched: &[(String, Option<String>)]) -> String {
    let changes: Vec<Change> = touched.iter().map(|(f, k)| Change { field: f.clone(), key: k.clone(), before: None, after: snap.get(f, k.as_deref()).cloned() }).collect();
    serde_json::json!({ "rev": rev, "cambios": changes }).to_string()
}

/// Línea del diario a partir de los cambios (cómo han quedado).
pub fn journal_line_of(rev: u64, changes: &[Change]) -> String {
    let after: Vec<Change> = changes.iter().map(|c| Change { field: c.field.clone(), key: c.key.clone(), before: None, after: c.after.clone() }).collect();
    serde_json::json!({ "rev": rev, "cambios": after }).to_string()
}

#[derive(Deserialize)]
struct JournalLine {
    rev: u64,
    #[serde(rename = "cambios")]
    changes: Vec<Change>,
}

/// Pone en `db` lo del diario de `data` que sea más nuevo que `db`. Una línea
/// a medio escribir (el ordenador se apagó mientras) se descarta. Devuelve
/// cuántas revisiones se han puesto.
pub fn replay_journal(db: &mut Db, data: &std::path::Path) -> Result<usize, String> {
    let Ok(text) = std::fs::read_to_string(journal_path(data)) else { return Ok(0) };
    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        // una línea que no se lee es la última, cortada por un apagón, o un
        // daño: lo de después no se aplica (quedaría un hueco sin avisar)
        let Ok(j) = serde_json::from_str::<JournalLine>(line) else { break };
        if j.rev <= db.rev {
            continue;
        }
        for ch in &j.changes {
            match (&ch.key, &ch.after) {
                (Some(k), after) => set_record(db, &ch.field, k, after.as_deref())?,
                (None, Some(a)) => set_field(db, &ch.field, a)?,
                (None, None) => {}
            }
        }
        db.rev = j.rev;
        n += 1;
    }
    Ok(n)
}

/// Para las pruebas: los datos tal como quedan en el disco (fichero y diario).
#[cfg(test)]
pub fn on_disk(data: &std::path::Path) -> Db {
    let mut db: Db = serde_json::from_str(&std::fs::read_to_string(data).unwrap()).unwrap();
    replay_journal(&mut db, data).unwrap();
    db
}

/// Unos datos mínimos (sin registros) para usar solo algunos campos.
pub fn skeleton() -> Db {
    static SKELETON: std::sync::OnceLock<Db> = std::sync::OnceLock::new();
    SKELETON
        .get_or_init(|| {
            let mut db = Db::sample(crate::model::today());
            clear_records(&mut db);
            db
        })
        .clone()
}

/// Los datos mínimos con estos campos sacados de `snap`.
pub fn partial_db(snap: &Snapshot, fields: &[&str]) -> Result<Db, String> {
    let mut db = skeleton();
    for f in fields {
        let json = match snap.records.get(*f) {
            Some(r) => r.to_json_with(&|s| Some(s.to_string())),
            None => match snap.plain.get(*f) {
                Some(p) => p.clone(),
                None => continue,
            },
        };
        set_field(&mut db, f, &json)?;
    }
    Ok(db)
}

/// Lo que ha cambiado en `fields` de `db` respecto a `snap` (como valores nuevos).
pub fn changes_in(snap: &Snapshot, db: &Db, fields: &[&str]) -> Vec<Change> {
    let mut part = Snapshot::default();
    let mut base = Snapshot::default();
    for f in fields {
        if let Some(r) = records_of(db, f) {
            part.records.insert(f.to_string(), r);
            base.records.insert(f.to_string(), snap.records.get(*f).cloned().unwrap_or_default());
        } else if let Some(p) = plain_of(db, f) {
            part.plain.insert(f.to_string(), p);
            if let Some(b) = snap.plain.get(*f) {
                base.plain.insert(f.to_string(), b.clone());
            }
        }
    }
    diff(&base, &part)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn db() -> Db {
        Db::sample(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap())
    }

    #[test]
    fn every_field_of_the_data_is_listed_once() {
        let v = serde_json::to_value(db()).unwrap();
        let mut keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        keys.sort();
        let mut listed: Vec<&str> = RECORD_FIELDS.iter().chain(PLAIN_FIELDS).copied().collect();
        listed.sort();
        assert_eq!(keys.iter().map(|s| s.as_str()).collect::<Vec<_>>(), listed, "falta (o sobra) un campo de Db en sync.rs");
        // las listas con clave tienen la clave en todos los registros
        for f in RECORD_FIELDS {
            assert!(v[*f].as_array().unwrap().iter().all(|r| r.get("id").is_some() || r.get("token").is_some()), "{f}");
        }
    }

    #[test]
    fn round_trip_keeps_everything() {
        let d = db();
        let snap = Snapshot::from_db(&d);
        let back: Value = serde_json::from_str(&snap.to_json()).unwrap();
        // como texto (lo que va al disco y por la red): `to_value` pasaría los f32 a f64
        let orig: Value = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        for (k, v) in orig.as_object().unwrap() {
            assert_eq!(back.get(k), Some(v), "el campo {k} cambia al ir y volver");
        }
        assert_eq!(back.as_object().unwrap().len(), orig.as_object().unwrap().len());
        assert_eq!(snap.to_db().unwrap().patients.len(), d.patients.len());
    }

    #[test]
    fn only_what_changed_is_sent() {
        let mut d = db();
        let base = Snapshot::from_db(&d);
        assert!(diff(&base, &Snapshot::from_db(&d)).is_empty(), "sin cambios no se manda nada");
        d.appointments[0].notes = "nota".into();
        let gone = d.payments.remove(0).id;
        let id = d.new_id();
        let mut p = d.patients[0].clone();
        p.id = id;
        d.patients.push(p);
        d.config.name = "Otra".into();
        let ch = diff(&base, &Snapshot::from_db(&d));
        let what: Vec<(String, Option<String>, bool, bool)> = ch.iter().map(|c| (c.field.clone(), c.key.clone(), c.before.is_some(), c.after.is_some())).collect();
        assert!(what.contains(&("appointments".into(), Some(d.appointments[0].id.to_string()), true, true)));
        assert!(what.contains(&("payments".into(), Some(gone.to_string()), true, false)));
        assert!(what.contains(&("patients".into(), Some(id.to_string()), false, true)));
        assert!(what.contains(&("config".into(), None, true, true)));
        assert!(what.contains(&("next_id".into(), None, true, true)));
        assert_eq!(ch.len(), 5, "{what:?}");
    }

    #[test]
    fn changes_from_two_stations_are_combined() {
        let start = db();
        let server = Snapshot::from_db(&start);
        let mut cur = server.clone();
        // A cambia las notas de una cita; B, la hora de la misma cita y borra un cobro
        let mut a = start.clone();
        a.appointments[0].notes = "de A".into();
        let mut b = start.clone();
        b.appointments[0].minutes += 15;
        let paid = b.payments.remove(0).id;
        apply(&mut cur, &diff(&server, &Snapshot::from_db(&a))).unwrap();
        let o = apply(&mut cur, &diff(&server, &Snapshot::from_db(&b))).unwrap();
        assert_eq!(o.conflicts, 0);
        let fin = cur.to_db().unwrap();
        assert_eq!(fin.appointments[0].notes, "de A");
        assert_eq!(fin.appointments[0].minutes, start.appointments[0].minutes + 15);
        assert!(fin.payments.iter().all(|p| p.id != paid));
        // A borra la cita que B ha cambiado: se conserva
        let mut a2 = start.clone();
        let appt = a2.appointments.remove(0).id;
        let o = apply(&mut cur, &diff(&server, &Snapshot::from_db(&a2))).unwrap();
        assert_eq!(o.conflicts, 1);
        assert!(cur.to_db().unwrap().appointments.iter().any(|x| x.id == appt));
        // los identificadores nunca bajan
        let mut low = start.clone();
        low.next_id = 1;
        let mut high = start.clone();
        high.next_id = start.next_id + 50;
        apply(&mut cur, &diff(&server, &Snapshot::from_db(&high))).unwrap();
        apply(&mut cur, &diff(&server, &Snapshot::from_db(&low))).unwrap();
        assert_eq!(cur.to_db().unwrap().next_id, start.next_id + 50);
    }

    #[test]
    fn direct_comparison_finds_the_same_changes() {
        let mut d = db();
        let base = Snapshot::from_db(&d);
        assert!(diff_db(&base, &d).is_empty());
        d.appointments[0].notes = "nota".into();
        d.payments.remove(0);
        let id = d.new_id();
        let mut p = d.patients[0].clone();
        p.id = id;
        d.patients.push(p);
        d.config.name = "Otra".into();
        let key = |c: &Change| (c.field.clone(), c.key.clone());
        let mut a = diff(&base, &Snapshot::from_db(&d));
        let mut b = diff_db(&base, &d);
        a.sort_by_key(key);
        b.sort_by_key(key);
        assert_eq!(a, b);
    }

    #[test]
    fn untouched_lists_are_not_looked_at() {
        let mut d = db();
        let base = Snapshot::from_db(&d);
        mark_synced(&mut d);
        assert!(!d.appointments.is_touched());
        // leer no marca; cambiar, sí
        let _ = d.appointments.iter().count();
        let _ = d.appointments[0].notes.len();
        assert!(!d.appointments.is_touched());
        d.appointments[0].notes = "nota".into();
        assert!(d.appointments.is_touched());
        assert!(!d.patients.is_touched());
        assert_eq!(diff_db(&base, &d).len(), 1);
        // lo que llega del servidor no marca
        mark_synced(&mut d);
        let p = serde_json::to_string(&d.patients[0]).unwrap();
        let key = d.patients[0].id.to_string();
        set_record(&mut d, "patients", &key, Some(&p)).unwrap();
        assert!(!d.patients.is_touched());
        // un cambio en una lista no marcada (no debería pasar) no se ve: por eso
        // al cargar los datos todas empiezan marcadas
        let loaded: Db = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert!(loaded.patients.is_touched() && loaded.appointments.is_touched());
    }

    #[test]
    fn the_journal_brings_back_what_was_not_rewritten() {
        let dir = std::env::temp_dir().join(format!("ds_journal_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.join("clinica.json");
        let mut d = db();
        d.rev = 10;
        let mut snap = Snapshot::from_db(&d);
        // dos revisiones apuntadas en el diario, sin reescribir el fichero
        let id = d.appointments[0].id.to_string();
        let mut a = d.appointments[0].clone();
        a.notes = "en el diario".into();
        snap.set("appointments", Some(&id), Some(serde_json::to_string(&a).unwrap()));
        let gone = d.payments[0].id.to_string();
        snap.set("payments", Some(&gone), None);
        let mut lines = journal_line(11, &snap, &[("appointments".into(), Some(id.clone()))]);
        lines.push('\n');
        lines.push_str(&journal_line(12, &snap, &[("payments".into(), Some(gone.clone()))]));
        // y una línea a medio escribir
        lines.push_str("\n{\"rev\": 13, \"camb");
        std::fs::write(journal_path(&data), lines).unwrap();
        assert_eq!(replay_journal(&mut d, &data).unwrap(), 2);
        assert_eq!(d.rev, 12);
        assert_eq!(d.appointments[0].notes, "en el diario");
        assert!(d.payments.iter().all(|p| p.id.to_string() != gone));
        // lo que ya está en el fichero no se vuelve a poner
        assert_eq!(replay_journal(&mut d, &data).unwrap(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_record_must_carry_its_own_key() {
        let d = db();
        let mut cur = Snapshot::from_db(&d);
        let before = cur.clone();
        let p = serde_json::to_string(&d.patients[0]).unwrap();
        // el paciente con id 100 mandado con la clave de otro: dos pacientes con el mismo id
        let wrong = Change { field: "patients".into(), key: Some("999999".into()), before: None, after: Some(p.clone()) };
        assert!(apply(&mut cur, &[wrong]).unwrap_err().contains("otra clave"));
        // una lista con clave sin clave, o un campo normal con clave
        assert!(apply(&mut cur, &[Change { field: "patients".into(), key: None, before: None, after: Some(format!("[{p}]")) }]).is_err());
        let config = cur.plain.get("config").cloned();
        assert!(apply(&mut cur, &[Change { field: "config".into(), key: Some("1".into()), before: None, after: config }]).is_err());
        assert!(apply(&mut cur, &[Change { field: "inventado".into(), key: None, before: None, after: Some("1".into()) }]).is_err());
        assert_eq!(cur, before);
        // los enlaces van por su token
        let mut d2 = d.clone();
        let mut l = d2.appt_links[0].clone();
        l.token = "f".repeat(32);
        d2.appt_links.push(l);
        assert!(apply(&mut cur, &diff(&before, &Snapshot::from_db(&d2))).is_ok());
    }

    #[test]
    fn what_was_applied_can_be_undone() {
        let start = db();
        let mut cur = Snapshot::from_db(&start);
        let before = cur.clone();
        let mut d = start.clone();
        d.appointments[0].notes = "nota".into();
        d.payments.remove(0);
        let id = d.new_id();
        let mut p = d.patients[0].clone();
        p.id = id;
        d.patients.push(p);
        d.config.name = "Otra".into();
        let o = apply(&mut cur, &diff(&before, &Snapshot::from_db(&d))).unwrap();
        assert_ne!(cur, before);
        undo(&mut cur, o.previous);
        assert_eq!(cur, before, "todo como estaba");
    }

    #[test]
    fn invalid_records_are_refused_whole() {
        let start = db();
        let mut cur = Snapshot::from_db(&start);
        let before = cur.clone();
        let bad = vec![
            Change { field: "config".into(), key: None, before: cur.plain.get("config").cloned(), after: Some("{\"nombre\": 3}".into()) },
            Change { field: "patients".into(), key: Some("999999".into()), before: None, after: Some("{\"id\": \"no\"}".into()) },
        ];
        assert!(apply(&mut cur, &bad).is_err());
        assert_eq!(cur, before, "no se aplica nada");
    }

    #[test]
    fn changes_reach_the_station_data() {
        let start = db();
        let mut station = start.clone();
        let mut synced = Snapshot::from_db(&station);
        let mut other = start.clone();
        other.patients[1].first_name = "Cambiado".into();
        other.appt_links.clear();
        let ch = diff(&synced, &Snapshot::from_db(&other));
        apply_to_db(&mut station, &mut synced, &ch).unwrap();
        assert_eq!(station.patients[1].first_name, "Cambiado");
        assert!(station.appt_links.is_empty());
        assert!(diff(&synced, &Snapshot::from_db(&station)).is_empty(), "lo recibido no parece un cambio del puesto");
    }
}
