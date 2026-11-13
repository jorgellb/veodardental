//! Cifrado de los datos de la clínica en este ordenador (T-F4-04): el
//! fichero de datos, su diario de cambios y las copias locales, con
//! AES-256-GCM (`ring`, sin criptografía propia).
//!
//! - **Claves:** aleatorias de 256 bits y con un identificador, guardadas en
//!   la configuración de este ordenador (`datos_claves.json`, solo para el
//!   usuario), no en la carpeta de los datos. Se guarda también la de antes,
//!   así que lo cifrado con ella (copias, diario) se sigue abriendo.
//! - **Rotación:** una clave nueva y todo reescrito con ella.
//! - **Recuperación:** la clave se guarda aparte (otro disco); con ella se
//!   abren los datos o una copia en otro ordenador.
//! - **Integridad:** un byte cambiado y el fichero no se abre, y entonces
//!   tampoco se guarda encima.
//! - **Migración recuperable en los dos sentidos:** copia previa, fichero nuevo
//!   comprobado y cambio atómico. Un corte en cualquier punto deja un fichero
//!   que se abre: lo cifrado y lo sin cifrar se leen igual.
//!
//! **Límite:** las imágenes están en carpetas que comparten los programas de
//! radiología (puentes), y cifrarlas allí rompería esos programas. Las protege
//! el cifrado del disco (`disco`), que se comprueba en la puesta en marcha.

use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"VDDATOS1";
const ID_LEN: usize = 8;
const HEADER: usize = 8 + ID_LEN + NONCE_LEN;

/// Claves de los datos: la que cifra lo nuevo (si hay) y todas las de antes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DataKeys {
    /// Identificador de la clave con que se cifra lo nuevo; vacío: no se cifra.
    pub current: String,
    /// (identificador, clave) en hexadecimal.
    pub keys: Vec<(String, String)>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

impl DataKeys {
    pub fn on(&self) -> bool {
        !self.current.is_empty()
    }

    fn get(&self, id: &str) -> Option<[u8; 32]> {
        self.keys.iter().find(|(i, _)| i == id).and_then(|(_, k)| unhex(k)).and_then(|k| k.try_into().ok())
    }

    /// Añade una clave nueva y la pone para lo que se escriba a partir de ahora.
    pub fn with_new_key(&self) -> Result<DataKeys, String> {
        let rng = SystemRandom::new();
        let (mut id, mut key) = ([0u8; ID_LEN], [0u8; 32]);
        rng.fill(&mut id).map_err(|_| "Sin generador de números aleatorios.")?;
        rng.fill(&mut key).map_err(|_| "Sin generador de números aleatorios.")?;
        let mut k = self.clone();
        k.keys.push((hex(&id), hex(&key)));
        k.current = hex(&id);
        Ok(k)
    }

    /// Las mismas claves, sin cifrar lo nuevo.
    pub fn off(&self) -> DataKeys {
        DataKeys { current: String::new(), ..self.clone() }
    }
}

pub fn is_sealed(bytes: &[u8]) -> bool {
    bytes.len() >= HEADER && &bytes[..8] == MAGIC
}

/// Cifra con la clave actual de `keys`.
pub fn seal(plain: &[u8], keys: &DataKeys) -> Result<Vec<u8>, String> {
    let k = keys.get(&keys.current).ok_or("No hay clave para cifrar los datos.")?;
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new().fill(&mut nonce).map_err(|_| "Sin generador de números aleatorios.")?;
    let mut out = Vec::with_capacity(HEADER + plain.len() + 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&unhex(&keys.current).ok_or("Clave dañada.")?);
    out.extend_from_slice(&nonce);
    let mut body = plain.to_vec();
    let key = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &k).map_err(|_| "Clave no válida.")?);
    key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(&out[..HEADER]), &mut body).map_err(|_| "No se pudieron cifrar los datos.")?;
    out.extend_from_slice(&body);
    Ok(out)
}

/// Descifra con la clave que diga la cabecera (la actual o una anterior).
pub fn unseal(bytes: &[u8], keys: &DataKeys) -> Result<Vec<u8>, String> {
    if !is_sealed(bytes) {
        return Err("No son datos cifrados por el programa.".into());
    }
    let id = hex(&bytes[8..8 + ID_LEN]);
    let k = keys.get(&id).ok_or("Los datos están cifrados con una clave que no está en este ordenador: use la clave de recuperación de los datos.")?;
    let nonce: [u8; NONCE_LEN] = bytes[8 + ID_LEN..HEADER].try_into().map_err(|_| "Cabecera dañada.")?;
    let mut body = bytes[HEADER..].to_vec();
    let key = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &k).map_err(|_| "Clave no válida.")?);
    let plain = key.open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(&bytes[..HEADER]), &mut body).map_err(|_| "Los datos cifrados están dañados o se han cambiado: no se abren.".to_string())?;
    Ok(plain.to_vec())
}

// ───────────────────────────── las claves de este ordenador ─────────────────────────────

#[cfg(not(test))]
fn keys_path() -> PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("datos_claves.json")
}

#[cfg(not(test))]
static KEYS: std::sync::Mutex<Option<DataKeys>> = std::sync::Mutex::new(None);

#[cfg(test)]
thread_local! {
    // en las pruebas, cada una con las suyas y sin tocar la configuración del ordenador
    static KEYS: std::cell::RefCell<DataKeys> = std::cell::RefCell::new(DataKeys::default());
}

/// Las claves de los datos de este ordenador (se leen una vez).
pub fn keys() -> DataKeys {
    #[cfg(not(test))]
    {
        let mut g = KEYS.lock().unwrap_or_else(|e| e.into_inner());
        g.get_or_insert_with(|| std::fs::read_to_string(keys_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()).clone()
    }
    #[cfg(test)]
    {
        KEYS.with(|k| k.borrow().clone())
    }
}

pub fn set_keys(k: &DataKeys) -> Result<(), String> {
    #[cfg(not(test))]
    {
        let p = keys_path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let tmp = p.with_extension("tmp");
        crate::paths::write_private(&tmp, serde_json::to_string_pretty(k).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
        *KEYS.lock().unwrap_or_else(|e| e.into_inner()) = Some(k.clone());
    }
    #[cfg(test)]
    KEYS.with(|x| *x.borrow_mut() = k.clone());
    Ok(())
}

/// Lo que se escribe en el disco: cifrado si los datos de este ordenador se cifran.
pub fn to_disk(plain: &[u8]) -> Result<Vec<u8>, String> {
    let k = keys();
    if k.on() { seal(plain, &k) } else { Ok(plain.to_vec()) }
}

/// Lo que se lee del disco: descifrado si estaba cifrado.
pub fn from_disk(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if is_sealed(&bytes) { unseal(&bytes, &keys()) } else { Ok(bytes) }
}

/// Una línea del diario para el disco («@» y el texto cifrado en hexadecimal).
pub fn line_to_disk(line: &str) -> Result<String, String> {
    let k = keys();
    if k.on() { Ok(format!("@{}", hex(&seal(line.as_bytes(), &k)?))) } else { Ok(line.to_string()) }
}

/// Una línea del diario leída del disco (`None`: no se puede leer).
pub fn line_from_disk(line: &str) -> Option<String> {
    match line.strip_prefix('@') {
        Some(h) => String::from_utf8(unseal(&unhex(h)?, &keys()).ok()?).ok(),
        None => Some(line.to_string()),
    }
}

/// Lee un fichero de datos (cifrado o no) como texto.
pub fn read_text(path: &Path) -> std::io::Result<String> {
    let b = from_disk(std::fs::read(path)?).map_err(std::io::Error::other)?;
    String::from_utf8(b).map_err(std::io::Error::other)
}

/// Cambia el cifrado del fichero de datos `data` a las claves `new`
/// (cifrar, descifrar o rotar):
/// 1. escribe el fichero nuevo al lado y comprueba que se lee igual;
/// 2. guarda las claves (con las de antes);
/// 3. pone el fichero nuevo en su sitio (cambio atómico) y vacía el diario,
///    porque el fichero ya lo tiene todo.
///
/// La copia previa la hace quien llama. `db` son los datos al día.
pub fn convert(data: &Path, db: &crate::model::Db, new: &DataKeys) -> Result<(), String> {
    let plain = serde_json::to_string(db).map_err(|e| e.to_string())?;
    let bytes = if new.on() { seal(plain.as_bytes(), new)? } else { plain.clone().into_bytes() };
    let tmp = data.with_extension("json.nuevo");
    crate::paths::write_private(&tmp, &bytes).map_err(|e| e.to_string())?;
    let back = if is_sealed(&bytes) { unseal(&std::fs::read(&tmp).map_err(|e| e.to_string())?, new)? } else { std::fs::read(&tmp).map_err(|e| e.to_string())? };
    if back != plain.as_bytes() {
        let _ = std::fs::remove_file(&tmp);
        return Err("El fichero nuevo no se lee igual: no se ha cambiado nada.".into());
    }
    set_keys(new)?;
    std::fs::rename(&tmp, data).map_err(|e| format!("No se pudo poner el fichero nuevo (se queda el anterior, que se sigue abriendo): {e}"))?;
    let _ = crate::paths::create_private(&crate::sync::journal_path(data));
    Ok(())
}

/// Guarda la clave de recuperación de los datos en `dest`, nunca en la carpeta de los datos.
pub fn export_keys(dest: &Path, data_dir: &Path) -> Result<(), String> {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    if canon(parent).starts_with(canon(data_dir)) {
        return Err("No guarde la clave junto a los datos: quien tuviera los datos tendría también la clave.".into());
    }
    let text = serde_json::to_string_pretty(&serde_json::json!({ "claves_de_los_datos": keys() })).map_err(|e| e.to_string())?;
    crate::paths::write_private(dest, text.as_bytes()).map_err(|e| e.to_string())
}

/// Recupera las claves desde una clave de recuperación (se suman a las que haya).
pub fn import_keys(path: &Path) -> Result<(), String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?).map_err(|_| "No es una clave de recuperación de los datos.".to_string())?;
    let k: DataKeys = serde_json::from_value(v["claves_de_los_datos"].clone()).map_err(|_| "No es una clave de recuperación de los datos.".to_string())?;
    let mut mine = keys();
    for (id, key) in k.keys {
        if !mine.keys.iter().any(|(i, _)| *i == id) {
            mine.keys.push((id, key));
        }
    }
    if !mine.on() {
        mine.current = k.current;
    }
    set_keys(&mine)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Db;

    fn dir(n: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ds_cifdatos_{n}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Cifrar, rotar y descifrar el fichero de datos con su diario: los datos
    /// son siempre los mismos, lo de antes se sigue leyendo y en el disco no
    /// se ve nada de los pacientes.
    #[test]
    fn data_is_encrypted_rotated_and_decrypted_without_loss() {
        set_keys(&DataKeys::default()).unwrap();
        let d = dir("ciclo");
        let data = d.join("clinica.json");
        let mut db = Db::sample(crate::model::today());
        Db::write_full(&db, &data).unwrap();
        let name = db.patients[0].last_name.clone();
        assert!(std::fs::read_to_string(&data).unwrap().contains(&name));
        // cifrar
        let k1 = keys().with_new_key().unwrap();
        convert(&data, &db, &k1).unwrap();
        let raw = std::fs::read(&data).unwrap();
        assert!(is_sealed(&raw) && !String::from_utf8_lossy(&raw).contains(&name));
        assert_eq!(Db::load_from(&data, false).0.patients.len(), db.patients.len());
        // un cambio en el diario, cifrado también
        let mut st = crate::local::LocalStore::new(data.clone(), crate::local::lock(&data).unwrap(), &mut db).unwrap();
        db.patients[0].notes = "nota tras cifrar".into();
        st.save(&mut db).unwrap();
        let journal = std::fs::read_to_string(crate::sync::journal_path(&data)).unwrap();
        assert!(journal.starts_with('@') && !journal.contains("nota tras cifrar"));
        drop(st);
        assert_eq!(Db::load_from(&data, false).0.patients[0].notes, "nota tras cifrar");
        // rotar: clave nueva; lo de la anterior se sigue leyendo
        let old_backup = d.join("copia_vieja.json");
        Db::write_full(&db, &old_backup).unwrap();
        let k2 = keys().with_new_key().unwrap();
        let full = Db::load_from(&data, false).0;
        convert(&data, &full, &k2).unwrap();
        assert_ne!(k1.current, k2.current);
        assert_eq!(std::fs::read(crate::sync::journal_path(&data)).unwrap().len(), 0, "el diario, vacío: el fichero lo tiene todo");
        assert_eq!(Db::load_from(&data, false).0.patients[0].notes, "nota tras cifrar");
        assert_eq!(Db::load_from(&old_backup, false).0.patients.len(), db.patients.len(), "lo cifrado con la clave de antes");
        // descifrar
        convert(&data, &full, &keys().off()).unwrap();
        assert!(std::fs::read_to_string(&data).unwrap().contains("nota tras cifrar"));
        set_keys(&DataKeys::default()).unwrap();
        let _ = std::fs::remove_dir_all(d);
    }

    /// Un byte cambiado: no se abre (y no se guardaría encima). Sin la clave,
    /// tampoco; con la de recuperación, sí. La clave no se guarda con los datos.
    #[test]
    fn tampering_is_detected_and_the_recovery_key_opens_the_data() {
        set_keys(&DataKeys::default()).unwrap();
        let d = dir("integridad");
        let data = d.join("clinica.json");
        let db = Db::sample(crate::model::today());
        let k = keys().with_new_key().unwrap();
        convert(&data, &db, &k).unwrap();
        let mut raw = std::fs::read(&data).unwrap();
        let n = raw.len();
        raw[n - 3] ^= 1;
        std::fs::write(d.join("tocado.json"), &raw).unwrap();
        let (_, err) = Db::load_from(&d.join("tocado.json"), false);
        assert!(err.unwrap().contains("dañados o se han cambiado"));
        // la clave de recuperación, nunca en la carpeta de los datos
        assert!(export_keys(&d.join("clave.json"), &d).unwrap_err().contains("junto a los datos"));
        let apart = dir("llavero").join("clave_datos.json");
        export_keys(&apart, &d).unwrap();
        // otro ordenador: sin claves no abre; con la de recuperación, sí
        set_keys(&DataKeys::default()).unwrap();
        assert!(Db::load_from(&data, false).1.unwrap().contains("clave de recuperación"));
        import_keys(&apart).unwrap();
        assert_eq!(Db::load_from(&data, false).0.patients.len(), db.patients.len());
        // y un corte a mitad de la conversión deja el fichero de antes, que se abre
        std::fs::write(data.with_extension("json.nuevo"), b"a medias").unwrap();
        assert_eq!(Db::load_from(&data, false).0.patients.len(), db.patients.len());
        set_keys(&DataKeys::default()).unwrap();
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn journal_lines_round_trip_and_mix() {
        set_keys(&DataKeys::default()).unwrap();
        assert_eq!(line_to_disk("{\"rev\":1}").unwrap(), "{\"rev\":1}");
        let k = keys().with_new_key().unwrap();
        set_keys(&k).unwrap();
        let enc = line_to_disk("{\"rev\":2}").unwrap();
        assert!(enc.starts_with('@'));
        assert_eq!(line_from_disk(&enc).as_deref(), Some("{\"rev\":2}"));
        assert_eq!(line_from_disk("{\"rev\":1}").as_deref(), Some("{\"rev\":1}"), "lo de antes, sin cifrar, también");
        assert_eq!(line_from_disk("@zz"), None);
        set_keys(&DataKeys::default()).unwrap();
    }
}
