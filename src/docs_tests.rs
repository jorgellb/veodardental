//! Las guías (`docs/instalacion.md`, `docs/restauracion.md`, `docs/guia_breve.md`)
//! comprobadas contra el programa (T-F3-08): cada texto entre «» es algo que se
//! ve en pantalla y tiene que estar en el código, y cada opción `--…` tiene que
//! existir en `main.rs`. Si se cambia un botón, esta prueba avisa de la guía.

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
}
