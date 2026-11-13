//! Carpetas donde el programa lee y escribe: datos de la clínica, ajustes de
//! este ordenador (puesto, certificados, claves) y documentos.
//!
//! Todo el programa pasa por aquí. En las pruebas automáticas cada proceso usa
//! su propia carpeta temporal, y con la variable `VEODARDENTAL_RAIZ` (capturas,
//! mediciones, ensayos de instalación) todo va debajo de esa carpeta: así ni
//! las pruebas ni los ensayos leen o escriben los datos, ajustes, certificados
//! o copias de verdad de este ordenador.

use std::path::PathBuf;

/// Variable de entorno con una carpeta raíz aislada.
pub const ROOT_VAR: &str = "VEODARDENTAL_RAIZ";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Data,
    Config,
    Documents,
}

/// Raíz aislada: la de las pruebas o la de `VEODARDENTAL_RAIZ`.
fn root() -> Option<PathBuf> {
    if cfg!(test) {
        return Some(std::env::temp_dir().join(format!("veodardental-pruebas-{}", std::process::id())).join("raiz"));
    }
    std::env::var_os(ROOT_VAR).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn resolve(root: Option<PathBuf>, kind: Kind) -> PathBuf {
    if let Some(r) = root {
        return r.join(match kind {
            Kind::Data => "datos",
            Kind::Config => "ajustes",
            Kind::Documents => "documentos",
        });
    }
    let sys = match kind {
        Kind::Data => dirs::data_dir(),
        Kind::Config => dirs::config_dir(),
        Kind::Documents => dirs::document_dir().or_else(dirs::home_dir),
    };
    sys.unwrap_or_else(|| PathBuf::from("."))
}

/// Carpeta de datos del sistema (la del programa es `data().join(brand::DIR)`).
pub fn data() -> PathBuf {
    resolve(root(), Kind::Data)
}

/// Carpeta de ajustes del sistema (puesto, certificados, claves).
pub fn config() -> PathBuf {
    resolve(root(), Kind::Config)
}

/// Carpeta de documentos del usuario (o su carpeta personal).
pub fn documents() -> PathBuf {
    resolve(root(), Kind::Documents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_never_use_the_real_folders() {
        let tmp = std::env::temp_dir();
        for p in [data(), config(), documents(), crate::model::Db::path()] {
            assert!(p.starts_with(&tmp), "{} fuera de la carpeta temporal", p.display());
        }
        // y nunca dentro de las carpetas del programa de este ordenador
        for real in [dirs::data_dir(), dirs::config_dir(), dirs::document_dir(), dirs::home_dir()].into_iter().flatten() {
            let program = real.join(crate::brand::DIR);
            for p in [data(), config(), documents()] {
                assert!(!p.join(crate::brand::DIR).starts_with(&program) || program.starts_with(&tmp), "{}", p.display());
            }
        }
        assert!(root().is_some());
    }

    #[test]
    fn an_isolated_root_holds_every_folder() {
        let r = PathBuf::from("/aislado");
        assert_eq!(resolve(Some(r.clone()), Kind::Data), r.join("datos"));
        assert_eq!(resolve(Some(r.clone()), Kind::Config), r.join("ajustes"));
        assert_eq!(resolve(Some(r.clone()), Kind::Documents), r.join("documentos"));
    }
}
