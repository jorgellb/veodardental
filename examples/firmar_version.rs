//! Firma el paquete de una versión para `--actualizar` (ver
//! `src/actualizar.rs`): escribe `manifiesto.json` en la carpeta con la
//! versión, la versión de los datos y el tamaño y el SHA-256 de cada fichero,
//! firmado con Ed25519.
//!
//! La clave privada (PKCS#8 en hexadecimal) llega en la variable
//! `VEODARDENTAL_CLAVE_FIRMA`; nunca está en el repositorio. Se usa en la
//! integración continua (secreto del mismo nombre).
//!
//! `cargo run --example firmar_version -- <carpeta> <versión> [versión de datos]`

use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::json;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, version) = match args.as_slice() {
        [d, v, ..] => (std::path::PathBuf::from(d), v.trim_start_matches('v').to_string()),
        _ => return Err("Uso: firmar_version <carpeta> <versión> [versión de datos]".into()),
    };
    let data_version: u32 = args.get(2).and_then(|x| x.parse().ok()).unwrap_or(1);
    let key_hex = std::env::var("VEODARDENTAL_CLAVE_FIRMA").map_err(|_| "Falta la variable VEODARDENTAL_CLAVE_FIRMA.")?;
    let key_bytes: Vec<u8> = (0..key_hex.trim().len()).step_by(2).map(|i| u8::from_str_radix(&key_hex.trim()[i..i + 2], 16)).collect::<Result<_, _>>().map_err(|_| "La clave no está en hexadecimal.")?;
    let key = Ed25519KeyPair::from_pkcs8_maybe_unchecked(&key_bytes).map_err(|_| "La clave no es una Ed25519 en PKCS#8.")?;
    let mut files = vec![];
    let mut names: Vec<_> = std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n != "manifiesto.json").collect();
    names.sort();
    for name in names {
        let bytes = std::fs::read(dir.join(&name)).map_err(|e| e.to_string())?;
        files.push(json!({ "name": name, "size": bytes.len(), "sha256": hex(ring::digest::digest(&ring::digest::SHA256, &bytes).as_ref()) }));
    }
    let manifest = json!({ "version": version, "data_version": data_version, "files": files }).to_string();
    let signature = hex(key.sign(manifest.as_bytes()).as_ref());
    std::fs::write(dir.join("manifiesto.json"), json!({ "manifest": manifest, "signature": signature }).to_string()).map_err(|e| e.to_string())?;
    println!("Firmado: {} ficheros · clave pública {}", files.len(), hex(key.public_key().as_ref()));
    Ok(())
}
