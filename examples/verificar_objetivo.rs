//! Verificador del encargo de VEODARDENTAL_GOAL.md.
//!
//!     cargo run --example verificar_objetivo             # comprobación de cierre (falla si queda algo)
//!     cargo run --example verificar_objetivo -- --progreso  # informe sin exigir el cierre
//!
//! Comprueba la integridad de docs/autonomia/TAREAS.json, que cada línea con
//! requisitos del encargo esté cubierta por un requisito con tareas, que las
//! tareas hechas tengan evidencias registradas en EVIDENCIAS.md, que cada
//! bloqueo externo esté descrito por completo en BLOQUEOS.md con su
//! preparación terminada y que no quede ninguna tarea de ingeniería
//! pendiente. No sustituye a las pruebas del programa ni demuestra por sí
//! solo que un bloqueo sea cierto.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

const GOAL: &str = "VEODARDENTAL_GOAL.md";
const DIR: &str = "docs/autonomia";
const TYPES: [&str; 3] = ["ingenieria", "decision_externa", "validacion_externa"];
const STATES: [&str; 4] = ["PENDIENTE", "EN_CURSO", "HECHA", "BLOQUEADA_EXTERNA"];
/// Apartados de un bloqueo en BLOQUEOS.md.
const BLOCK_FIELDS: [&str; 7] = ["Falta:", "Por qué:", "Parte afectada:", "Preparación terminada:", "Responsable:", "Paso para desbloquear:", "Prueba posterior:"];

/// Líneas del encargo que contienen requisitos: las de los apartados 2 a 9
/// que no son títulos, bloques de código ni cabeceras de tabla.
fn requirement_lines(goal: &str) -> Vec<(usize, String)> {
    let mut out = vec![];
    let mut section: Option<u32> = None;
    let mut code = false;
    for (i, l) in goal.lines().enumerate() {
        if l.starts_with("```") {
            code = !code;
            continue;
        }
        if code {
            continue;
        }
        if let Some(rest) = l.strip_prefix("## ") {
            section = rest.split(['.', ' ']).next().and_then(|n| n.parse().ok());
            continue;
        }
        let t = l.trim();
        if l.starts_with("### ") || t.is_empty() || t.starts_with("| ---") || t.starts_with("| Orden") {
            continue;
        }
        if matches!(section, Some(2..=9)) {
            out.push((i + 1, t.to_string()));
        }
    }
    out
}

/// Identificadores de sección («## E-001 …») de un documento de seguimiento.
fn headings(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut current: Option<String> = None;
    for l in text.lines() {
        if let Some(h) = l.strip_prefix("## ") {
            let id = h.split_whitespace().next().unwrap_or("").trim_end_matches([':', '.']).to_string();
            map.insert(id.clone(), String::new());
            current = Some(id);
        } else if let Some(c) = &current {
            let body = map.get_mut(c).unwrap();
            body.push_str(l);
            body.push('\n');
        }
    }
    map
}

fn str_list(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

fn main() -> ExitCode {
    let progress = std::env::args().any(|a| a == "--progreso");
    let mut errors: Vec<String> = vec![];
    let read = |p: &str| std::fs::read_to_string(p).map_err(|e| format!("no se puede leer {p}: {e}"));
    let (goal, tasks_txt, evid_txt, block_txt) = match (read(GOAL), read(&format!("{DIR}/TAREAS.json")), read(&format!("{DIR}/EVIDENCIAS.md")), read(&format!("{DIR}/BLOQUEOS.md"))) {
        (Ok(a), Ok(b), Ok(c), Ok(d)) => (a, b, c, d),
        (a, b, c, d) => {
            for e in [a.err(), b.err(), c.err(), d.err()].into_iter().flatten() {
                eprintln!("ERROR {e}");
            }
            return ExitCode::FAILURE;
        }
    };
    for doc in ["ESTADO_INICIAL.md", "PROGRESO.md", "DECISIONES.md"] {
        if std::fs::metadata(format!("{DIR}/{doc}")).is_err() {
            errors.push(format!("falta {DIR}/{doc}"));
        }
    }
    let inv: Value = match serde_json::from_str(&tasks_txt) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ERROR TAREAS.json no es JSON válido: {e}");
            return ExitCode::FAILURE;
        }
    };
    // el inventario corresponde a esta versión del encargo
    let sha: String = Sha256::digest(goal.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    if inv["fuente_sha256"].as_str() != Some(sha.as_str()) {
        errors.push("el encargo ha cambiado desde que se hizo el inventario: revise los requisitos y actualice fuente_sha256".into());
    }
    // requisitos: únicos y cubriendo cada línea del encargo
    let reqs = inv["requisitos"].as_array().cloned().unwrap_or_default();
    let mut req_ids = BTreeSet::new();
    let mut anchors: Vec<(String, String)> = vec![];
    for r in &reqs {
        let id = r["id"].as_str().unwrap_or("").to_string();
        let anchor = r["ancla"].as_str().unwrap_or("").to_string();
        if id.is_empty() || anchor.len() < 20 || !req_ids.insert(id.clone()) {
            errors.push(format!("requisito sin id, repetido o con ancla demasiado corta: {id}"));
        }
        anchors.push((id, anchor));
    }
    let lines = requirement_lines(&goal);
    for (n, l) in &lines {
        if !anchors.iter().any(|(_, a)| l.starts_with(a.as_str())) {
            errors.push(format!("la línea {n} del encargo no está cubierta por ningún requisito: «{}…»", l.chars().take(60).collect::<String>()));
        }
    }
    for (id, a) in &anchors {
        if !lines.iter().any(|(_, l)| l.starts_with(a.as_str())) {
            errors.push(format!("el requisito {id} no corresponde a ninguna línea del encargo"));
        }
    }
    // tareas
    let tasks = inv["tareas"].as_array().cloned().unwrap_or_default();
    let evid = headings(&evid_txt);
    let blocks = headings(&block_txt);
    let mut ids = BTreeSet::new();
    let by_id: BTreeMap<String, &Value> = tasks.iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), t)).collect();
    let mut covered: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    let (mut done, mut pending, mut blocked) = (vec![], vec![], vec![]);
    for t in &tasks {
        let id = t["id"].as_str().unwrap_or("");
        let kind = t["tipo"].as_str().unwrap_or("");
        let state = t["estado"].as_str().unwrap_or("");
        if id.is_empty() || !ids.insert(id.to_string()) {
            errors.push(format!("tarea sin id o repetida: «{id}»"));
        }
        for f in ["titulo", "criterio"] {
            if t[f].as_str().is_none_or(|s| s.trim().is_empty()) {
                errors.push(format!("{id}: falta «{f}»"));
            }
        }
        if t["fase"].is_null() {
            errors.push(format!("{id}: falta la fase"));
        }
        if !TYPES.contains(&kind) {
            errors.push(format!("{id}: tipo no válido «{kind}»"));
        }
        if !STATES.contains(&state) {
            errors.push(format!("{id}: estado no válido «{state}»"));
        }
        let reqs_of = str_list(&t["requisitos"]);
        if reqs_of.is_empty() {
            errors.push(format!("{id}: sin requisito de origen"));
        }
        for r in &reqs_of {
            if !req_ids.contains(r) {
                errors.push(format!("{id}: requisito desconocido {r}"));
            }
            covered.entry(r.clone()).or_default().push(state);
        }
        for d in str_list(&t["dependencias"]) {
            if !by_id.contains_key(&d) {
                errors.push(format!("{id}: dependencia desconocida {d}"));
            }
        }
        let ev = str_list(&t["evidencia"]);
        match state {
            "HECHA" => {
                if ev.is_empty() {
                    errors.push(format!("{id}: hecha sin evidencia"));
                }
                for e in &ev {
                    if e.starts_with("E-") && !evid.contains_key(e) {
                        errors.push(format!("{id}: la evidencia {e} no está en EVIDENCIAS.md"));
                    }
                }
                done.push(id.to_string());
            }
            "BLOQUEADA_EXTERNA" => {
                if kind == "ingenieria" {
                    errors.push(format!("{id}: una tarea de ingeniería no puede quedar bloqueada; separe la parte externa en otra tarea"));
                }
                let b = t["bloqueo"].as_str().unwrap_or("");
                match blocks.get(b) {
                    None => errors.push(format!("{id}: el bloqueo «{b}» no está descrito en BLOQUEOS.md")),
                    Some(body) => {
                        for f in BLOCK_FIELDS {
                            let filled = body.lines().any(|l| l.contains(f) && l.split(f).nth(1).is_some_and(|v| !v.trim().is_empty()));
                            if !filled {
                                errors.push(format!("{id}: al bloqueo {b} le falta «{f}»"));
                            }
                        }
                    }
                }
                for d in str_list(&t["dependencias"]) {
                    if let Some(dt) = by_id.get(&d)
                        && dt["tipo"] == "ingenieria"
                        && dt["estado"] != "HECHA"
                    {
                        pending.push(format!("{id}: su preparación {d} no está terminada"));
                    }
                }
                blocked.push(id.to_string());
            }
            _ => {
                if kind == "ingenieria" {
                    pending.push(format!("{id}: {} ({state})", t["titulo"].as_str().unwrap_or("")));
                } else {
                    errors.push(format!("{id}: tarea externa en estado {state}; debe quedar BLOQUEADA_EXTERNA con su bloqueo o HECHA con evidencia"));
                }
            }
        }
    }
    for r in &req_ids {
        if !covered.contains_key(r) {
            errors.push(format!("el requisito {r} no tiene tareas"));
        }
    }
    let full = covered.values().filter(|s| s.iter().all(|x| *x == "HECHA")).count();
    let external = covered.values().filter(|s| !s.iter().all(|x| *x == "HECHA") && s.iter().all(|x| *x == "HECHA" || *x == "BLOQUEADA_EXTERNA")).count();
    println!("Verificador del objetivo de VeodarDental");
    println!("  líneas del encargo con requisitos: {}", lines.len());
    println!("  requisitos: {} (con tareas: {})", req_ids.len(), covered.len());
    println!("  requisitos satisfechos por completo: {full}; solo con validaciones externas pendientes: {external}; con ingeniería pendiente: {}", req_ids.len() - full - external);
    println!("  tareas: {} · hechas: {} · ingeniería pendiente: {} · bloqueadas por dependencia externa: {}", tasks.len(), done.len(), pending.len(), blocked.len());
    for e in &errors {
        println!("ERROR {e}");
    }
    if !progress {
        for p in &pending {
            println!("PENDIENTE {p}");
        }
    }
    let ok = errors.is_empty() && (progress || pending.is_empty());
    println!("{}", if ok { if progress { "RESULTADO: inventario íntegro (modo progreso)" } else { "RESULTADO: condición de cierre cumplida" } } else { "RESULTADO: NO cumple" });
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
