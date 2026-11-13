//! Actualizaciones del programa (T-F3-02), en este orden:
//! 1. **Autenticidad e integridad:** el paquete trae un manifiesto firmado
//!    (Ed25519) con la versión, la versión de los datos y el tamaño y el
//!    SHA-256 de cada fichero. Sin firma válida, o con un fichero distinto,
//!    no se instala nada.
//! 2. **Copia previa** de los datos.
//! 3. **Cambio** de la carpeta del programa por la nueva (dos renombrados; la
//!    anterior se guarda para volver atrás).
//! 4. **Comprobación** del programa nuevo (`--comprobar`: abre los datos sin
//!    escribir). Si falla, se vuelve solo a la versión anterior.
//!
//! Cada paso se apunta en un diario. Si el ordenador se apaga entre dos
//! pasos, al arrancar [`recover`] deja la versión anterior entera, nunca
//! una mezcla. Los datos no se tocan hasta que la versión nueva arranca de
//! verdad: entonces se completan, con su propia copia previa (`Db::load_from`).
//!
//! **Orden en la clínica:** primero el servidor y después los puestos.
//! - Un puesto de otra versión no escribe en el servidor (`server::data_mismatch`).
//! - Lo que tenga sin enviar queda en la bandeja de su usuario (`server::outbox_path`) y se
//!   envía al actualizarlo.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Fichero del paquete con el manifiesto firmado ([`Signed`]).
pub const MANIFEST_FILE: &str = "manifiesto.json";

/// Claves públicas (Ed25519) con las que se firman las versiones. Está vacía
/// hasta que el responsable del producto genere la suya y guarde la privada
/// fuera del repositorio (B-25). Sin clave, no se acepta ninguna actualización.
pub const RELEASE_KEYS: &[[u8; 32]] = &[];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PackFile {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    /// Versión de los datos que usa (ver `model::DATA_VERSION`).
    pub data_version: u32,
    pub files: Vec<PackFile>,
}

/// Un paquete: el manifiesto tal como se firmó y su firma en hexadecimal.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signed {
    pub manifest: String,
    pub signature: String,
}

/// «1.2.3» → (1, 2, 3).
fn semver(v: &str) -> Option<(u32, u32, u32)> {
    let mut p = v.trim().trim_start_matches('v').split('.').map(|x| x.parse::<u32>().ok());
    Some((p.next()??, p.next()??, p.next().flatten().unwrap_or(0)))
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    s.len().is_multiple_of(2).then(|| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect::<Option<Vec<u8>>>())?
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// Comprueba la firma del manifiesto con alguna de las claves y que la
/// versión sea más nueva que `current`. Devuelve el manifiesto.
pub fn verify_manifest(p: &Signed, keys: &[[u8; 32]], current: &str) -> Result<Manifest, String> {
    if keys.is_empty() {
        return Err("Este programa no tiene clave de firma de versiones: no se aceptan actualizaciones automáticas (instale la versión nueva con su instalador).".into());
    }
    let sig = unhex(&p.signature).filter(|s| s.len() == 64).ok_or("La firma del paquete no es válida.")?;
    let ok = keys.iter().any(|k| ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, k).verify(p.manifest.as_bytes(), &sig).is_ok());
    if !ok {
        return Err("La firma del paquete no es de VeodarDental: no se instala.".into());
    }
    let m: Manifest = serde_json::from_str(&p.manifest).map_err(|e| format!("Manifiesto no válido: {e}"))?;
    for f in &m.files {
        let bad = f.name.is_empty() || f.name.contains(['/', '\\', ':']) || f.name == "." || f.name == "..";
        if bad {
            return Err(format!("Nombre de fichero no válido en el paquete: «{}».", f.name));
        }
    }
    if m.files.is_empty() {
        return Err("El paquete no trae ficheros.".into());
    }
    match (semver(&m.version), semver(current)) {
        (Some(new), Some(cur)) if new > cur => Ok(m),
        (Some(_), Some(_)) => Err(format!("La versión {} no es más nueva que la instalada ({current}).", m.version)),
        _ => Err(format!("Versión no válida: «{}».", m.version)),
    }
}

/// Cada fichero del manifiesto está en `dir` con su tamaño y su SHA-256, y
/// no hay ninguno más.
pub fn check_files(dir: &Path, m: &Manifest) -> Result<(), String> {
    for f in &m.files {
        let bytes = std::fs::read(dir.join(&f.name)).map_err(|e| format!("Falta {} en el paquete: {e}", f.name))?;
        if bytes.len() as u64 != f.size || sha256_hex(&bytes) != f.sha256.to_lowercase() {
            return Err(format!("{} no es el de la versión {} (dañado o cambiado): no se instala.", f.name, m.version));
        }
    }
    let extra: Vec<String> = std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n != MANIFEST_FILE && !m.files.iter().any(|f| &f.name == n)).collect();
    if !extra.is_empty() {
        return Err(format!("El paquete trae ficheros que no están en su manifiesto: {}.", extra.join(", ")));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    /// Paquete comprobado y preparado al lado de la instalación.
    Prepared,
    /// Copia de los datos hecha.
    BackedUp,
    /// La carpeta del programa ya es la nueva (la anterior, guardada).
    Swapped,
}

/// Diario de una actualización en curso (al lado de la carpeta del programa).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Journal {
    pub step: Step,
    pub from: String,
    pub to: String,
    pub backup: Option<PathBuf>,
}

/// Cómo terminó una actualización (o lo que se encontró al arrancar).
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Versión nueva instalada y comprobada (la anterior queda para volver atrás).
    Updated { backup: PathBuf },
    /// No había llegado a cambiar nada: se dejó como estaba.
    Aborted,
    /// Se volvió a la versión anterior (por qué).
    RolledBack(String),
    /// Nada que hacer.
    Nothing,
}

fn sibling(dir: &Path, suffix: &str) -> PathBuf {
    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    dir.with_file_name(format!("{name}.{suffix}"))
}

pub fn journal_path(dir: &Path) -> PathBuf {
    sibling(dir, "actualizacion.json")
}

fn write_journal(dir: &Path, j: &Journal) -> Result<(), String> {
    let p = journal_path(dir);
    let tmp = p.with_extension("tmp");
    crate::paths::write_private(&tmp, serde_json::to_string(j).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        std::fs::copy(e.path(), to.join(e.file_name())).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Corte simulado (para las pruebas de fallo entre pasos): tras qué paso.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cut {
    After(Step),
    /// Entre los dos renombrados del cambio (sin carpeta del programa).
    MidSwap,
}

/// Instala la versión del paquete `pkg` (ya descomprimido) en `dir`:
/// comprueba, hace la copia (`backup`), cambia las carpetas y comprueba la
/// nueva (`check`). Si la comprobación falla, vuelve a la anterior.
pub fn apply(dir: &Path, (pkg, signed): (&Path, &Signed), keys: &[[u8; 32]], current: &str, backup: impl FnOnce() -> Result<PathBuf, String>, check: impl FnOnce(&Path) -> Result<(), String>, cut: Option<Cut>) -> Result<Outcome, String> {
    if journal_path(dir).exists() {
        return Err("Hay otra actualización a medias: arranque el programa para terminarla o deshacerla.".into());
    }
    let m = verify_manifest(signed, keys, current)?;
    check_files(pkg, &m)?;
    let (staged, previous) = (sibling(dir, "nueva"), sibling(dir, "anterior"));
    let _ = std::fs::remove_dir_all(&staged);
    copy_dir(pkg, &staged)?;
    // lo copiado también tiene que ser lo firmado
    check_files(&staged, &m)?;
    let mut j = Journal { step: Step::Prepared, from: current.to_string(), to: m.version.clone(), backup: None };
    write_journal(dir, &j)?;
    if cut == Some(Cut::After(Step::Prepared)) {
        return Err("corte simulado".into());
    }
    let b = backup().map_err(|e| {
        let _ = std::fs::remove_file(journal_path(dir));
        let _ = std::fs::remove_dir_all(&staged);
        format!("No se actualiza: no se pudo hacer la copia de seguridad previa ({e}).")
    })?;
    j.step = Step::BackedUp;
    j.backup = Some(b.clone());
    write_journal(dir, &j)?;
    if cut == Some(Cut::After(Step::BackedUp)) {
        return Err("corte simulado".into());
    }
    let _ = std::fs::remove_dir_all(&previous);
    std::fs::rename(dir, &previous).map_err(|e| format!("No se pudo apartar la versión instalada: {e}"))?;
    if cut == Some(Cut::MidSwap) {
        return Err("corte simulado".into());
    }
    if let Err(e) = std::fs::rename(&staged, dir) {
        let _ = std::fs::rename(&previous, dir);
        let _ = std::fs::remove_file(journal_path(dir));
        return Err(format!("No se pudo poner la versión nueva (se queda la anterior): {e}"));
    }
    j.step = Step::Swapped;
    write_journal(dir, &j)?;
    if cut == Some(Cut::After(Step::Swapped)) {
        return Err("corte simulado".into());
    }
    if let Err(e) = check(dir) {
        roll_back(dir)?;
        return Ok(Outcome::RolledBack(format!("La versión {} no arrancó bien ({e}): se ha vuelto a la {current}.", m.version)));
    }
    let _ = std::fs::remove_file(journal_path(dir));
    // para volver atrás: qué versión de los datos usaba la anterior (la de este programa)
    let info = serde_json::json!({ "version": current, "datos": crate::model::DATA_VERSION, "copia": b });
    let _ = crate::paths::write_private(&sibling(dir, "anterior.json"), info.to_string().as_bytes());
    Ok(Outcome::Updated { backup: b })
}

/// Vuelve a la versión anterior guardada al lado.
fn roll_back(dir: &Path) -> Result<(), String> {
    let (previous, failed) = (sibling(dir, "anterior"), sibling(dir, "fallida"));
    if !previous.exists() {
        return Err("No está la versión anterior para volver a ella.".into());
    }
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&failed);
        std::fs::rename(dir, &failed).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&previous, dir).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(journal_path(dir));
    Ok(())
}

/// Al arrancar: si una actualización quedó a medias (apagón, cierre), deja
/// entera la versión anterior. Los datos no se han tocado (la copia previa
/// queda por si acaso).
pub fn recover(dir: &Path) -> Result<Outcome, String> {
    let jp = journal_path(dir);
    let Some(j) = std::fs::read_to_string(&jp).ok().and_then(|t| serde_json::from_str::<Journal>(&t).ok()) else {
        // sin diario (o ilegible): solo puede faltar la carpeta entre los dos renombrados
        if !dir.exists() && sibling(dir, "anterior").exists() {
            roll_back(dir)?;
            return Ok(Outcome::RolledBack("Una actualización se cortó al cambiar de versión: se ha vuelto a la anterior.".into()));
        }
        let _ = std::fs::remove_file(&jp);
        return Ok(Outcome::Nothing);
    };
    match j.step {
        Step::Prepared | Step::BackedUp if dir.exists() => {
            let _ = std::fs::remove_dir_all(sibling(dir, "nueva"));
            let _ = std::fs::remove_file(&jp);
            Ok(Outcome::Aborted)
        }
        _ => {
            roll_back(dir)?;
            Ok(Outcome::RolledBack(format!("La actualización a la versión {} se cortó: se ha vuelto a la {}.", j.to, j.from)))
        }
    }
}

/// Vuelta atrás a mano tras una actualización hecha: vuelve la versión
/// anterior. Si la nueva cambió la versión de los datos, la anterior los abre
/// sin guardar: entonces hay que restaurar la copia previa (se pierde lo hecho
/// desde la actualización), y esto lo dice.
pub fn undo(dir: &Path, data_version_now: u32, data_version_before: u32) -> Result<String, String> {
    roll_back(dir)?;
    Ok(if data_version_now > data_version_before {
        "Vuelta a la versión anterior. Los datos ya son de la nueva: la anterior los abre sin guardar. Restaure la copia «antes de actualizar» (se pierde lo hecho desde entonces) o vuelva a la versión nueva.".into()
    } else {
        "Vuelta a la versión anterior. Los datos siguen valiendo tal cual.".into()
    })
}

/// Carpeta propia del programa (la del ejecutable, si se llama como el
/// programa). Una instalación del sistema (`/usr/bin`, un paquete .deb) no
/// se toca: se actualiza con su gestor de paquetes.
pub fn install_dir() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let name = crate::model::fold(&dir.file_name()?.to_string_lossy());
    name.starts_with(&crate::model::fold(crate::brand::NAME)).then_some(dir)
}

/// `--actualizar <carpeta del paquete>`: instala la versión del paquete en
/// la carpeta del programa, con copia previa y comprobación.
pub fn run_cli(pkg: &Path) -> Result<String, String> {
    let dir = install_dir().ok_or("Esta instalación es del sistema: actualícela con su instalador o su gestor de paquetes.")?;
    let signed: Signed = serde_json::from_str(&std::fs::read_to_string(pkg.join(MANIFEST_FILE)).map_err(|e| format!("El paquete no trae {MANIFEST_FILE}: {e}"))?).map_err(|e| format!("{MANIFEST_FILE} no válido: {e}"))?;
    let exe = std::env::current_exe().ok().and_then(|e| e.file_name().map(|n| n.to_owned())).ok_or("No se sabe cómo se llama el programa.")?;
    let backup = || {
        let (db, err) = crate::model::Db::load_from(&crate::model::Db::path(), false);
        if let Some(e) = err {
            return Err(e);
        }
        crate::tools::create_backup(&db, "antes de actualizar el programa")
    };
    let check = |d: &Path| {
        let out = std::process::Command::new(d.join(&exe)).arg("--comprobar").output().map_err(|e| e.to_string())?;
        if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
    };
    match apply(&dir, (pkg, &signed), RELEASE_KEYS, env!("CARGO_PKG_VERSION"), backup, check, None)? {
        Outcome::Updated { backup } => Ok(format!("Actualizado. Copia previa de los datos: {}. Cierre y vuelva a abrir el programa.", backup.display())),
        Outcome::RolledBack(m) => Err(m),
        _ => Ok(String::new()),
    }
}

/// `--volver-atras`: vuelve a la versión anterior guardada al lado.
pub fn undo_cli() -> Result<String, String> {
    let dir = install_dir().ok_or("Esta instalación es del sistema: vuelva a la versión anterior con su instalador.")?;
    let info: serde_json::Value = std::fs::read_to_string(sibling(&dir, "anterior.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).ok_or("No hay una versión anterior guardada.")?;
    let before = info["datos"].as_u64().unwrap_or(0) as u32;
    let now = crate::model::Db::load_from(&crate::model::Db::path(), false).0.data_version;
    let msg = undo(&dir, now, before)?;
    Ok(match info["copia"].as_str() {
        Some(c) if now > before => format!("{msg} Copia: {c}."),
        _ => msg,
    })
}

/// `--comprobar`: abre los datos sin escribir. Es lo que se comprueba de la
/// versión nueva antes de dejarla puesta.
pub fn self_check() -> Result<String, String> {
    let (db, err) = crate::model::Db::load_from(&crate::model::Db::path(), false);
    match err {
        Some(e) => Err(e),
        None => Ok(format!("VeodarDental {} · datos {} · {} pacientes", env!("CARGO_PKG_VERSION"), db.data_version, db.patients.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::KeyPair;

    fn keypair() -> ring::signature::Ed25519KeyPair {
        let rng = ring::rand::SystemRandom::new();
        ring::signature::Ed25519KeyPair::from_pkcs8(ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap().as_ref()).unwrap()
    }

    fn public(k: &ring::signature::Ed25519KeyPair) -> [u8; 32] {
        k.public_key().as_ref().try_into().unwrap()
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// Paquete de PRUEBA con un «programa» de mentira y su manifiesto firmado.
    fn package(root: &Path, k: &ring::signature::Ed25519KeyPair, version: &str) -> (PathBuf, Signed) {
        let pkg = root.join(format!("paquete-{version}"));
        std::fs::create_dir_all(&pkg).unwrap();
        let files = [("veodardental", format!("programa {version}")), ("version.txt", version.to_string())];
        let mut list = vec![];
        for (n, body) in &files {
            std::fs::write(pkg.join(n), body).unwrap();
            list.push(PackFile { name: n.to_string(), size: body.len() as u64, sha256: sha256_hex(body.as_bytes()) });
        }
        let manifest = serde_json::to_string(&Manifest { version: version.into(), data_version: crate::model::DATA_VERSION, files: list }).unwrap();
        let signature = hex(k.sign(manifest.as_bytes()).as_ref());
        (pkg, Signed { manifest, signature })
    }

    fn installed(root: &Path, version: &str) -> PathBuf {
        let dir = root.join("VeodarDental");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("veodardental"), format!("programa {version}")).unwrap();
        std::fs::write(dir.join("version.txt"), version).unwrap();
        dir
    }

    fn version_in(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("version.txt")).unwrap()
    }

    fn root(name: &str) -> PathBuf {
        let r = std::env::temp_dir().join(format!("ds_actualizar_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&r);
        std::fs::create_dir_all(&r).unwrap();
        r
    }

    #[test]
    fn only_a_signed_complete_and_newer_package_is_accepted() {
        let r = root("firma");
        let k = keypair();
        let (pkg, signed) = package(&r, &k, "0.3.0");
        let keys = [public(&k)];
        assert!(verify_manifest(&signed, &[], "0.2.0").unwrap_err().contains("no tiene clave"), "sin clave, nada");
        assert!(RELEASE_KEYS.is_empty(), "la clave de verdad la pone el responsable del producto (B-25)");
        let m = verify_manifest(&signed, &keys, "0.2.0").unwrap();
        assert!(check_files(&pkg, &m).is_ok());
        assert!(verify_manifest(&signed, &[public(&keypair())], "0.2.0").unwrap_err().contains("no es de VeodarDental"), "otra clave");
        let mut forged = signed.clone();
        forged.manifest = forged.manifest.replace("0.3.0", "0.4.0");
        assert!(verify_manifest(&forged, &keys, "0.2.0").is_err(), "manifiesto cambiado");
        assert!(verify_manifest(&signed, &keys, "0.3.0").unwrap_err().contains("no es más nueva"));
        std::fs::write(pkg.join("veodardental"), "programa 0.3.0 con un cambio").unwrap();
        assert!(check_files(&pkg, &m).unwrap_err().contains("dañado o cambiado"));
        std::fs::write(pkg.join("veodardental"), "programa 0.3.0").unwrap();
        std::fs::write(pkg.join("extra.dll"), "x").unwrap();
        assert!(check_files(&pkg, &m).unwrap_err().contains("extra.dll"));
        // nombres con ruta, no
        let bad = Manifest { version: "0.3.0".into(), data_version: 1, files: vec![PackFile { name: "../fuera".into(), size: 0, sha256: String::new() }] };
        let text = serde_json::to_string(&bad).unwrap();
        let s = Signed { signature: hex(k.sign(text.as_bytes()).as_ref()), manifest: text };
        assert!(verify_manifest(&s, &keys, "0.2.0").unwrap_err().contains("no válido"));
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn an_update_installs_checks_and_rolls_back_by_itself_if_the_new_version_fails() {
        let r = root("aplicar");
        let k = keypair();
        let keys = [public(&k)];
        let (pkg, signed) = package(&r, &k, "0.3.0");
        let dir = installed(&r, "0.2.0");
        let data = r.join("clinica.json");
        std::fs::write(&data, "datos").unwrap();
        let backup = || {
            let b = r.join("copia_antes_de_actualizar.json");
            std::fs::copy(&data, &b).map_err(|e| e.to_string())?;
            Ok(b)
        };
        // la nueva no arranca: se vuelve sola a la anterior
        let out = apply(&dir, (&pkg, &signed), &keys, "0.2.0", backup, |_| Err("no abre los datos".into()), None).unwrap();
        assert!(matches!(out, Outcome::RolledBack(ref m) if m.contains("se ha vuelto a la 0.2.0")), "{out:?}");
        assert_eq!(version_in(&dir), "0.2.0");
        assert!(!journal_path(&dir).exists());
        // ahora sí
        let out = apply(&dir, (&pkg, &signed), &keys, "0.2.0", backup, |d| if version_in(d) == "0.3.0" { Ok(()) } else { Err("otra".into()) }, None).unwrap();
        assert_eq!(out, Outcome::Updated { backup: r.join("copia_antes_de_actualizar.json") });
        assert_eq!(version_in(&dir), "0.3.0");
        assert_eq!(std::fs::read_to_string(&data).unwrap(), "datos", "los datos, sin tocar");
        assert_eq!(version_in(&sibling(&dir, "anterior")), "0.2.0", "la anterior, guardada");
        let info: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(sibling(&dir, "anterior.json")).unwrap()).unwrap();
        assert_eq!((info["version"].as_str(), info["datos"].as_u64()), (Some("0.2.0"), Some(crate::model::DATA_VERSION as u64)), "y con qué datos trabajaba");
        // y volver atrás a mano
        assert!(undo(&dir, 1, 1).unwrap().contains("siguen valiendo"));
        assert_eq!(version_in(&dir), "0.2.0");
        // sin copia previa, no se actualiza
        let e = apply(&dir, (&pkg, &signed), &keys, "0.2.0", || Err("disco lleno".into()), |_| Ok(()), None).unwrap_err();
        assert!(e.contains("copia de seguridad previa"), "{e}");
        assert_eq!(version_in(&dir), "0.2.0");
        assert!(!journal_path(&dir).exists() && !sibling(&dir, "nueva").exists());
        let _ = std::fs::remove_dir_all(r);
    }

    /// Fallo entre cada dos pasos (apagón): al arrancar, la versión anterior
    /// entera y los datos intactos; nunca una mezcla de las dos.
    #[test]
    fn a_cut_between_any_two_steps_leaves_the_previous_version_whole() {
        let k = keypair();
        let keys = [public(&k)];
        for cut in [Cut::After(Step::Prepared), Cut::After(Step::BackedUp), Cut::MidSwap, Cut::After(Step::Swapped)] {
            let r = root(&format!("{cut:?}").replace(['(', ')'], "_"));
            let (pkg, signed) = package(&r, &k, "0.3.0");
            let dir = installed(&r, "0.2.0");
            let data = r.join("clinica.json");
            std::fs::write(&data, "datos").unwrap();
            let b = r.join("copia.json");
            assert!(apply(&dir, (&pkg, &signed), &keys, "0.2.0", || std::fs::copy(&data, &b).map(|_| b.clone()).map_err(|e| e.to_string()), |_| Ok(()), Some(cut)).is_err());
            // mientras está a medias, no empieza otra
            if cut != Cut::MidSwap {
                assert!(apply(&dir, (&pkg, &signed), &keys, "0.2.0", || Ok(b.clone()), |_| Ok(()), None).unwrap_err().contains("a medias"), "{cut:?}");
            }
            let out = recover(&dir).unwrap();
            match cut {
                Cut::After(Step::Prepared) | Cut::After(Step::BackedUp) => assert_eq!(out, Outcome::Aborted, "{cut:?}"),
                _ => assert!(matches!(out, Outcome::RolledBack(_)), "{cut:?}: {out:?}"),
            }
            assert_eq!(version_in(&dir), "0.2.0", "{cut:?}");
            assert_eq!(std::fs::read_to_string(dir.join("veodardental")).unwrap(), "programa 0.2.0", "{cut:?}: entera");
            assert_eq!(std::fs::read_to_string(&data).unwrap(), "datos", "{cut:?}");
            assert!(!journal_path(&dir).exists(), "{cut:?}");
            assert_eq!(recover(&dir).unwrap(), Outcome::Nothing, "{cut:?}: y ya no queda nada a medias");
            // y después se puede actualizar de nuevo
            let out = apply(&dir, (&pkg, &signed), &keys, "0.2.0", || Ok(b.clone()), |_| Ok(()), None).unwrap();
            assert!(matches!(out, Outcome::Updated { .. }), "{cut:?}");
            assert_eq!(version_in(&dir), "0.3.0");
            let _ = std::fs::remove_dir_all(r);
        }
    }

    #[test]
    fn undoing_after_the_data_changed_says_what_to_restore() {
        let r = root("deshacer");
        let dir = installed(&r, "0.3.0");
        installed(&r.join("x"), "0.2.0");
        std::fs::rename(r.join("x").join("VeodarDental"), sibling(&dir, "anterior")).unwrap();
        assert!(undo(&dir, 2, 1).unwrap().contains("Restaure la copia"));
        assert_eq!(version_in(&dir), "0.2.0");
        assert!(undo(&dir, 1, 1).unwrap_err().contains("No está la versión anterior"));
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn versions_compare_by_numbers() {
        assert!(semver("0.10.0") > semver("0.9.9"));
        assert_eq!(semver("v1.2"), Some((1, 2, 0)));
        assert_eq!(semver("uno"), None);
    }
}
