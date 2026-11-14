//! Las guías (`docs/instalacion.md`, `docs/restauracion.md`, `docs/guia_breve.md`)
//! comprobadas contra el programa (T-F3-08): cada texto entre «» es algo que se
//! ve en pantalla y tiene que estar en el código, y cada opción `--…` tiene que
//! existir en `main.rs`. Si se cambia un botón, esta prueba avisa de la guía.
//! Y lo que cita `docs/revision_externa` existe (T-F4-09).

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn sources(dir: &Path, out: &mut String) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                sources(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("docs_tests.rs") {
                out.push_str(&std::fs::read_to_string(&p).unwrap());
            }
        }
    }

    #[test]
    fn the_guides_only_name_what_the_program_has() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut code = String::new();
        sources(&root.join("src"), &mut code);
        let main = std::fs::read_to_string(root.join("src/main.rs")).unwrap();
        let mut missing = vec![];
        let mut seen = 0;
        for doc in ["docs/instalacion.md", "docs/restauracion.md", "docs/guia_breve.md"] {
            let text = std::fs::read_to_string(root.join(doc)).unwrap();
            for part in text.split('«').skip(1) {
                let Some(label) = part.split('»').next() else { continue };
                seen += 1;
                if !code.contains(label) {
                    missing.push(format!("{doc}: «{label}»"));
                }
            }
            for part in text.split("`--").skip(1) {
                let flag = format!("--{}", part.split(|c: char| !(c.is_alphanumeric() || c == '-')).next().unwrap_or(""));
                seen += 1;
                if !main.contains(&format!("\"{flag}\"")) {
                    missing.push(format!("{doc}: {flag}"));
                }
            }
        }
        assert!(seen > 40, "las guías citan lo que hay en pantalla: {seen}");
        assert!(missing.is_empty(), "no están en el programa: {missing:#?}");
    }

    /// Lo que cita la carpeta para los revisores externos (T-F4-09) existe:
    /// cada fichero `src/…`, cada prueba `modulo::tests::nombre`, cada
    /// evidencia E-0NN y cada bloqueo B-NN. Así no se les remite a algo que
    /// ya no está.
    #[test]
    fn the_review_package_only_cites_what_exists() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut code = String::new();
        sources(&root.join("src"), &mut code);
        // esta prueba también se cita
        code.push_str(include_str!("docs_tests.rs"));
        let evidence = std::fs::read_to_string(root.join("docs/autonomia/EVIDENCIAS.md")).unwrap();
        let blocks = std::fs::read_to_string(root.join("docs/autonomia/BLOQUEOS.md")).unwrap();
        let word = |c: char| c.is_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '.' || c == '-';
        let mut missing = vec![];
        let (mut files, mut tests, mut refs) = (0, 0, 0);
        let docs = ["README.md", "gestoria.md", "dpo.md", "auditoria_seguridad.md"];
        for doc in docs {
            let text = std::fs::read_to_string(root.join("docs/revision_externa").join(doc)).unwrap();
            for part in text.split('`').skip(1).step_by(2) {
                let t = part.trim();
                if t.starts_with("src/") && t.ends_with(".rs") {
                    files += 1;
                    if !root.join(t).is_file() {
                        missing.push(format!("{doc}: {t}"));
                    }
                } else if let Some((_, name)) = t.split_once("::tests::")
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    tests += 1;
                    if !code.contains(&format!("fn {name}(")) {
                        missing.push(format!("{doc}: prueba {t}"));
                    }
                }
            }
            for tok in text.split(|c: char| !word(c)) {
                let tok = tok.trim_end_matches(['.', ':', ',']);
                let (list, prefix) = if tok.starts_with("E-0") {
                    (&evidence, "## ")
                } else if tok.starts_with("B-") {
                    (&blocks, "## ")
                } else {
                    continue;
                };
                if tok.len() < 4 || !tok[2..].chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                refs += 1;
                if !list.contains(&format!("{prefix}{tok} ")) {
                    missing.push(format!("{doc}: {tok}"));
                }
            }
        }
        assert!(files > 15 && tests > 30 && refs > 30, "la carpeta cita ficheros, pruebas y evidencias: {files} {tests} {refs}");
        assert!(missing.is_empty(), "no existen: {missing:#?}");
    }
}
