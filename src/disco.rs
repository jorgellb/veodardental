//! ¿Está cifrado el disco donde están los datos de la clínica?
//!
//! Los datos de salud de los pacientes, las claves de los servicios (correo,
//! WhatsApp, IA) y las copias diarias están en el disco del ordenador. Si
//! roban el ordenador o el disco, solo el cifrado del disco entero los
//! protege: BitLocker (Windows), FileVault (macOS) o LUKS (Linux). Aquí se
//! comprueba (una vez, en segundo plano: lanza un programa del sistema) para
//! avisar en la puesta en marcha y al arrancar el servidor.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Disk {
    /// Todavía comprobándolo.
    #[default]
    Checking,
    /// Cifrado (con qué).
    Encrypted(String),
    /// Sin cifrar (con la explicación).
    Plain(String),
    /// No se ha podido saber (por qué).
    Unknown(String),
}

/// En las pruebas de interfaz, un disco cifrado (no depende del ordenador
/// donde se pasan; la comprobación de verdad se prueba aparte).
#[cfg(test)]
pub fn status() -> Disk {
    Disk::Encrypted("pruebas".into())
}

/// Estado del disco de los datos; la primera vez lanza la comprobación y
/// devuelve `Checking` hasta que termina.
#[cfg(not(test))]
pub fn status() -> Disk {
    use std::sync::{Mutex, OnceLock};
    static STATE: OnceLock<Mutex<Disk>> = OnceLock::new();
    let mut first = false;
    let state = STATE.get_or_init(|| {
        first = true;
        Mutex::new(Disk::Checking)
    });
    if first {
        let dir = data_dir();
        std::thread::spawn(move || {
            let d = check(&dir);
            if let Ok(mut g) = state.lock() {
                *g = d;
            }
        });
    }
    state.lock().map(|g| g.clone()).unwrap_or_default()
}

/// Comprobación sin hilo (para el servidor, que la necesita al arrancar).
pub fn check_now() -> Disk {
    check(&data_dir())
}

/// Carpeta de los datos (o la de las copias, que también los llevan).
fn data_dir() -> PathBuf {
    let p = crate::model::Db::path();
    p.parent().map(Path::to_path_buf).unwrap_or(p)
}

/// La carpeta o la primera de sus carpetas superiores que exista.
fn existing(dir: &Path) -> PathBuf {
    dir.ancestors().find(|a| a.exists()).map(Path::to_path_buf).unwrap_or_else(|| dir.to_path_buf())
}

fn run(cmd: &mut Command) -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // sin ventana de consola
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(target_os = "windows")]
fn check(dir: &Path) -> Disk {
    use std::path::Component;
    let drive = match existing(dir).components().next() {
        Some(Component::Prefix(p)) => p.as_os_str().to_string_lossy().trim_end_matches('\\').to_string(),
        _ => "C:".to_string(),
    };
    if drive.starts_with("\\\\") {
        return Disk::Unknown(format!("Los datos están en una carpeta de red ({drive}): compruebe el cifrado en el ordenador que la comparte."));
    }
    // Propiedad del Explorador de Windows: no necesita permisos de administrador.
    let script = format!("(New-Object -ComObject Shell.Application).NameSpace('{drive}\\').Self.ExtendedProperty('System.Volume.BitLockerProtection')");
    match run(Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script])) {
        Some(out) => parse_bitlocker(&out, &drive),
        None => Disk::Unknown("No se ha podido preguntar a Windows por BitLocker.".into()),
    }
}

#[cfg(target_os = "macos")]
fn check(_dir: &Path) -> Disk {
    match run(Command::new("fdesetup").arg("status")) {
        Some(out) => parse_fdesetup(&out),
        None => Disk::Unknown("No se ha podido preguntar a macOS por FileVault.".into()),
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn check(dir: &Path) -> Disk {
    let dir = existing(dir);
    let Some(source) = run(Command::new("findmnt").args(["-no", "SOURCE", "--target"]).arg(&dir)) else {
        return Disk::Unknown("No se ha podido saber en qué disco están los datos (falta findmnt).".into());
    };
    // los subvolúmenes de btrfs salen como /dev/x[/subvolumen]
    let device = source.split('[').next().unwrap_or("").trim().to_string();
    if !device.starts_with("/dev/") {
        return Disk::Unknown(format!("Los datos están en {device}: compruebe el cifrado en el ordenador o el disco donde están."));
    }
    match run(Command::new("lsblk").args(["-s", "-no", "TYPE"]).arg(&device)) {
        Some(types) => parse_lsblk(&types, &device),
        None => Disk::Unknown("No se ha podido preguntar por el disco (falta lsblk).".into()),
    }
}

/// Valor de `System.Volume.BitLockerProtection`: 1 activado, 3 cifrando,
/// 6 activado y bloqueado; 2 desactivado, 4 descifrando, 5 en pausa, 8
/// cifrado del dispositivo pendiente de activar (la clave aún no protege).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_bitlocker(out: &str, drive: &str) -> Disk {
    match out.trim() {
        "1" | "6" => Disk::Encrypted(format!("BitLocker activado en {drive}")),
        "3" => Disk::Encrypted(format!("BitLocker cifrando {drive}")),
        "5" => Disk::Plain(format!("BitLocker está en pausa en {drive}: reanude la protección.")),
        "8" => Disk::Plain(format!("El cifrado del dispositivo de {drive} está preparado pero sin activar: inicie sesión con una cuenta Microsoft o actívelo en Configuración › Privacidad y seguridad.")),
        "2" | "4" => Disk::Plain(format!("{drive} no está cifrado: active BitLocker (o el cifrado del dispositivo) en Configuración › Privacidad y seguridad.")),
        "" | "0" => Disk::Plain(format!("{drive} no está cifrado y esta edición de Windows no trae BitLocker: use Windows Pro o un programa de cifrado de disco.")),
        x => Disk::Unknown(format!("Windows da un estado de BitLocker desconocido ({x}).")),
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn parse_fdesetup(out: &str) -> Disk {
    if out.contains("FileVault is On") || out.contains("Encryption in progress") {
        Disk::Encrypted("FileVault activado".into())
    } else if out.contains("FileVault is Off") || out.contains("Decryption in progress") {
        Disk::Plain("El disco no está cifrado: active FileVault en Ajustes del Sistema › Privacidad y seguridad.".into())
    } else {
        Disk::Unknown(format!("macOS da un estado de FileVault desconocido ({}).", out.lines().next().unwrap_or("")))
    }
}

/// Tipos de los dispositivos por debajo del de los datos (`lsblk -s`): con
/// uno «crypt» (dm-crypt, LUKS) está cifrado.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
pub fn parse_lsblk(types: &str, device: &str) -> Disk {
    if types.lines().any(|t| t.trim() == "crypt") {
        Disk::Encrypted(format!("LUKS ({device})"))
    } else {
        Disk::Plain(format!("{device} no está cifrado: para cifrarlo hace falta reinstalar con «cifrar el disco» (LUKS) o mover los datos a un disco cifrado."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_system_answer() {
        assert!(matches!(parse_bitlocker("1\r\n", "C:"), Disk::Encrypted(_)));
        assert!(matches!(parse_bitlocker("2", "C:"), Disk::Plain(_)));
        assert!(matches!(parse_bitlocker("5", "C:"), Disk::Plain(m) if m.contains("pausa")));
        assert!(matches!(parse_bitlocker("8", "C:"), Disk::Plain(m) if m.contains("sin activar")));
        assert!(matches!(parse_bitlocker("", "C:"), Disk::Plain(_)));
        assert!(matches!(parse_bitlocker("77", "C:"), Disk::Unknown(_)));
        assert!(matches!(parse_fdesetup("FileVault is On."), Disk::Encrypted(_)));
        assert!(matches!(parse_fdesetup("FileVault is Off."), Disk::Plain(_)));
        assert!(matches!(parse_lsblk("part\ncrypt\npart\ndisk", "/dev/mapper/luks-1"), Disk::Encrypted(_)));
        assert!(matches!(parse_lsblk("part\ndisk", "/dev/nvme0n1p3"), Disk::Plain(_)));
    }

    #[test]
    fn checks_this_computer() {
        // en el ordenador de las pruebas da una respuesta (la que sea), sin colgarse
        let d = check(&std::env::temp_dir());
        assert_ne!(d, Disk::Checking);
    }
}
