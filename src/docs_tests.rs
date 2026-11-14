//! Las guías (`docs/instalacion.md`, `docs/restauracion.md`, `docs/guia_breve.md`)
//! comprobadas contra el programa (T-F3-08): cada texto entre «» es algo que se
//! ve en pantalla y tiene que estar en el código, y cada opción `--…` tiene que
//! existir en `main.rs`. Si se cambia un botón, esta prueba avisa de la guía.
//! Y lo que cita `docs/revision_externa` existe (T-F4-09). Y el seguimiento
//! (`docs/autonomia`) cita evidencias y commits que existen (T-PROC-02).

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
        let docs = ["README.md", "gestoria.md", "dpo.md", "auditoria_seguridad.md", "traducciones.md", "invariantes.md"];
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

    /// T-PROC-02. El ciclo de cada tarea queda anotado:
    /// - cada fila de lo hecho en `PROGRESO.md` cita evidencias que existen
    ///   en `EVIDENCIAS.md` y commits que existen en el repositorio (si es
    ///   un clon completo);
    /// - cada evidencia desde la E-078 lleva la batería entera (formato,
    ///   clippy y pruebas) con código de salida 0 y su resultado.
    #[test]
    fn every_task_row_cites_its_evidence_and_commit_and_the_battery_is_recorded() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let progress = std::fs::read_to_string(root.join("docs/autonomia/PROGRESO.md")).unwrap();
        let evidence = std::fs::read_to_string(root.join("docs/autonomia/EVIDENCIAS.md")).unwrap();
        let git = |args: &[&str]| std::process::Command::new("git").args(args).current_dir(root).output().ok().filter(|o| o.status.success());
        // en un clon parcial (como el de la integración continua) no están los commits antiguos
        let full_clone = root.join(".git").exists() && git(&["rev-parse", "--is-shallow-repository"]).is_some_and(|o| o.stdout.starts_with(b"false"));
        let table = progress.split("## Hecho y probado").nth(1).unwrap().split("\n## ").next().unwrap();
        let (mut rows, mut commits, mut missing) = (0, 0, vec![]);
        for line in table.lines().filter(|l| l.starts_with("| ") && !l.starts_with("| Tarea")) {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            assert!(cells.len() >= 5, "fila incompleta: {line}");
            rows += 1;
            let evs: Vec<&str> = cells[2].split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).filter(|t| t.starts_with("E-0")).collect();
            if evs.is_empty() {
                missing.push(format!("sin evidencia: {line}"));
            }
            for e in evs {
                if !evidence.contains(&format!("## {e} ")) {
                    missing.push(format!("{e} no está en EVIDENCIAS ({})", cells[1]));
                }
            }
            for h in cells[3].split('`').skip(1).step_by(2) {
                commits += 1;
                if h.len() < 7 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                    missing.push(format!("commit no válido: {h}"));
                } else if full_clone && git(&["cat-file", "-e", &format!("{h}^{{commit}}")]).is_none() {
                    missing.push(format!("el commit {h} no existe ({})", cells[1]));
                }
            }
        }
        let mut batteries = 0;
        for part in evidence.split("\n## E-").skip(1) {
            let n: u32 = part.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap();
            if n < 78 {
                continue;
            }
            batteries += 1;
            for cmd in ["cargo fmt --all -- --check", "cargo clippy --workspace --all-targets --locked -- -D warnings", "cargo test --workspace --all-targets --locked"] {
                if !part.contains(&format!("| `{cmd}` | 0 |")) {
                    missing.push(format!("E-{n:03}: falta «{cmd}» con código 0"));
                }
            }
            if !part.contains("pasan (") || part.contains("| PENDIENTE |") {
                missing.push(format!("E-{n:03}: falta el resultado de las pruebas"));
            }
        }
        assert!(rows > 80 && commits > 80 && batteries >= 7, "{rows} filas, {commits} commits, {batteries} baterías");
        assert!(missing.is_empty(), "{missing:#?}");
    }
}
