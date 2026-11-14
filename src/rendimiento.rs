//! Mediciones de rendimiento reproducibles (T-F6-07, T-INI-08, T-F6-08).
//!
//! `tools/medir.sh` (o `veodardental --medir`, siempre con una raíz aislada)
//! mide con clínicas sintéticas, sin datos reales:
//! - una clínica grande: tamaño de los datos, memoria, guardado local de un
//!   cambio, lo que se envía al servidor por un cambio y la entrada de un
//!   puesto (datos comprimidos);
//! - varias clínicas (cada sociedad, su instalación: D-25): tamaño en disco,
//!   tiempo de abrir cada una y memoria con todas abiertas;
//! - concurrencia: varios puestos guardando a la vez en un servidor de
//!   verdad (en este ordenador), con lo que tarda cada guardado y la
//!   comprobación de que no se pierde ninguno.
//!
//! El resultado lleva la versión del programa y de los datos, la fecha y el
//! ordenador, y se guarda en `docs/autonomia/mediciones/`.

use crate::model::Db;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::time::Instant;

/// Clínica de unos 10 años de trabajo; `scale` 1,0 son 8 000 pacientes (unos
/// 220 MB de datos). Cada lista se llena repitiendo los registros de la
/// muestra con identificadores nuevos.
pub fn big_clinic(scale: f64) -> Db {
    let sample = serde_json::to_value(Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 4).unwrap_or_default())).unwrap_or_default();
    let targets = [
        ("patients", 8_000),
        ("entries", 100_000),
        ("appointments", 120_000),
        ("payments", 80_000),
        ("invoices", 40_000),
        ("budgets", 20_000),
        ("documents", 20_000),
        ("perio", 3_000),
        ("messages", 100_000),
        ("calls", 50_000),
        ("recalls", 8_000),
        ("health", 8_000),
        ("images", 30_000),
    ];
    let mut v = sample.clone();
    for (k, n) in targets {
        let n = ((n as f64) * scale).round().max(1.0) as usize;
        let Some(src) = sample[k].as_array().filter(|a| !a.is_empty()) else { continue };
        let out: Vec<Value> = (0..n)
            .map(|i| {
                let mut r = src[i % src.len()].clone();
                if r.get("id").is_some() {
                    r["id"] = Value::from(1_000_000 + i as u64);
                }
                r
            })
            .collect();
        v[k] = Value::Array(out);
    }
    serde_json::from_value(v).unwrap_or_else(|_| Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 4).unwrap_or_default()))
}

/// Memoria del proceso (Linux; 0 si no se sabe).
pub fn rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok()).map(|p| (p * 4096) as f64 / 1_000_000.0).unwrap_or(0.0)
}

#[derive(Clone, Debug, Serialize)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub version: String,
    pub data_version: u32,
    pub date: String,
    pub machine: String,
    pub scale: f64,
    pub patients: usize,
    pub clinics: usize,
    pub stations: usize,
    pub pushes: usize,
    pub metrics: Vec<Metric>,
}

impl Report {
    fn add(&mut self, name: &str, value: f64, unit: &str) {
        self.metrics.push(Metric { name: name.into(), value, unit: unit.into() });
    }

    #[cfg(test)]
    pub fn get(&self, name: &str) -> Option<f64> {
        self.metrics.iter().find(|m| m.name == name).map(|m| m.value)
    }

    /// Tabla para `docs/autonomia/mediciones/`.
    pub fn markdown(&self) -> String {
        let mut s = format!(
            "# Mediciones · VeodarDental {} (datos {})\n\n- Fecha: {}\n- Ordenador: {}\n- Clínica sintética: escala {} ({} pacientes)\n- Varias clínicas: {} · concurrencia: {} puestos × {} guardados\n- Cómo repetirlo: `tools/medir.sh` (ver `src/rendimiento.rs`)\n\n| Medición | Valor | Unidad |\n|---|---|---|\n",
            self.version, self.data_version, self.date, self.machine, self.scale, self.patients, self.clinics, self.stations, self.pushes
        );
        for m in &self.metrics {
            s.push_str(&format!("| {} | {} | {} |\n", m.name, if m.value.fract() == 0.0 { format!("{}", m.value) } else { format!("{:.3}", m.value) }, m.unit));
        }
        s
    }
}

fn secs(t: Instant) -> f64 {
    t.elapsed().as_secs_f64()
}

/// Mide en `dir` (una carpeta de trabajo vacía; se borra lo que deja).
pub fn measure(scale: f64, clinics: usize, stations: usize, pushes: usize, dir: &Path) -> Result<Report, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut r = Report {
        version: env!("CARGO_PKG_VERSION").into(),
        data_version: crate::model::DATA_VERSION,
        date: crate::model::now().format("%Y-%m-%d %H:%M").to_string(),
        machine: format!("{} {} · {} núcleos", std::env::consts::OS, std::env::consts::ARCH, std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)),
        scale,
        patients: 0,
        clinics,
        stations,
        pushes,
        metrics: vec![],
    };
    // ── una clínica grande ──
    let t = Instant::now();
    let mut db = big_clinic(scale);
    r.patients = db.patients.len();
    r.add("crear la clínica sintética", secs(t), "s");
    r.add("memoria con la clínica abierta", rss_mb(), "MB");
    let t = Instant::now();
    let text = serde_json::to_string(&db).map_err(|e| e.to_string())?;
    r.add("datos enteros a texto", secs(t), "s");
    r.add("tamaño de los datos", text.len() as f64 / 1_000_000.0, "MB");
    // guardado local (un solo ordenador): fichero bloqueado y diario de cambios
    let path = dir.join("clinica.json");
    std::fs::write(&path, &text).map_err(|e| e.to_string())?;
    drop(text);
    let t = Instant::now();
    let mut st = crate::local::LocalStore::new(path.clone(), crate::local::lock(&path)?, &mut db)?;
    r.add("abrir con un solo ordenador", secs(t), "s");
    db.appointments[0].notes = "llamar antes".into();
    let t = Instant::now();
    st.save(&mut db)?;
    r.add("guardado local de un cambio", secs(t), "s");
    r.add("diario tras un cambio", std::fs::metadata(crate::sync::journal_path(&path)).map(|m| m.len()).unwrap_or(0) as f64 / 1000.0, "KB");
    if let Some(p) = db.patients.first_mut() {
        p.mobile = "600000000".into();
    }
    let t = Instant::now();
    st.save(&mut db)?;
    r.add("guardado local de un cambio en un paciente", secs(t), "s");
    let t = Instant::now();
    st.close(&db)?;
    r.add("cerrar (fichero completo)", secs(t), "s");
    // en red: lo que se envía por un cambio, y la entrada de un puesto
    let t = Instant::now();
    let synced = crate::sync::Snapshot::from_db(&db);
    r.add("partir en registros (servidor)", secs(t), "s");
    crate::sync::mark_synced(&mut db);
    db.appointments[0].notes = "otra nota".into();
    db.add_payment(db.patients[0].id, 40.0, "Tarjeta", "medición");
    let t = Instant::now();
    let changes = crate::sync::diff_db(&synced, &db);
    r.add("puesto: ver qué ha cambiado", secs(t), "s");
    let sent: usize = changes.iter().map(|c| c.after.as_ref().map_or(0, |a| a.len()) + c.before.as_ref().map_or(0, |b| b.len())).sum();
    r.add("cambio enviado (una cita y un cobro)", sent as f64 / 1000.0, "KB");
    let view = crate::vista::View::of(crate::users::Profile::Recepcion);
    let t = Instant::now();
    let body = synced.to_json_with(&|list, rec| view.show(list, rec));
    let z = crate::server::gzip(body.as_bytes());
    r.add("entrada de un puesto: preparar y comprimir", secs(t), "s");
    r.add("entrada de un puesto (recepción), comprimida", z.len() as f64 / 1_000_000.0, "MB");
    r.add("entrada de un puesto (recepción), sin comprimir", body.len() as f64 / 1_000_000.0, "MB");
    drop((body, z, synced));
    // ── varias clínicas, cada una su instalación ──
    let mut opened = vec![];
    let mut disk = 0u64;
    let base_mem = rss_mb();
    for i in 0..clinics {
        let p = dir.join(format!("clinica_{i}.json"));
        let c = big_clinic(scale);
        std::fs::write(&p, serde_json::to_string(&c).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        drop(c);
        disk += std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        let t = Instant::now();
        let (d, err) = Db::load_from(&p, false);
        if let Some(e) = err {
            return Err(e);
        }
        if i == 0 {
            r.add("abrir una clínica (del disco)", secs(t), "s");
        }
        opened.push(d);
    }
    r.add("varias clínicas: en disco", disk as f64 / 1_000_000.0, "MB");
    r.add("varias clínicas: memoria con todas abiertas a la vez", (rss_mb() - base_mem).max(0.0), "MB");
    drop(opened);
    // ── concurrencia: varios puestos guardando a la vez en un servidor ──
    let (lat, total, lost) = concurrency(&db, stations, pushes, &dir.join("servidor"))?;
    r.add("concurrencia: tiempo total", total, "s");
    r.add("concurrencia: guardado de un puesto, mediana", percentile(&lat, 0.5), "s");
    r.add("concurrencia: guardado de un puesto, percentil 95", percentile(&lat, 0.95), "s");
    r.add("concurrencia: guardados perdidos", lost as f64, "");
    let _ = std::fs::remove_dir_all(dir);
    Ok(r)
}

fn percentile(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    s[((s.len() - 1) as f64 * p).round() as usize]
}

/// Un servidor de verdad (en 127.0.0.1) y `stations` puestos que guardan a la
/// vez `pushes` cobros cada uno. Devuelve lo que tardó cada guardado, el
/// tiempo total y cuántos no llegaron al disco.
fn concurrency(db: &Db, stations: usize, pushes: usize, dir: &Path) -> Result<(Vec<f64>, f64, usize), String> {
    use crate::server::{Entry, Remote, connect, serve};
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join("clinica.json");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let addr = format!("127.0.0.1:{}", listener.local_addr().map_err(|e| e.to_string())?.port());
    let key = "CLAVE-MEDICION".to_string();
    let identity = crate::server::new_identity()?;
    let (p2, k2, initial) = (path.clone(), key.clone(), db.clone());
    let h = std::thread::spawn(move || serve(listener, p2, k2, identity, initial));
    let login = |name: &str| -> Result<Remote, String> {
        let mut r = Remote::new(&addr, &key, 0);
        connect(&mut r, name, 40)?;
        match r.login(2, "0000")? {
            Entry::Inside(s) => r.session = s,
            _ => return Err("La medición necesita entrar sin código del móvil.".into()),
        }
        Ok(r)
    };
    let remotes: Vec<Remote> = (0..stations).map(|i| login(&format!("Puesto {i}"))).collect::<Result<_, _>>()?;
    let (pid, next_id) = (db.patients[0].id, db.next_id);
    let t = Instant::now();
    let threads: Vec<_> = remotes
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            std::thread::spawn(move || -> Result<Vec<f64>, String> {
                // un puesto ligero: sin los datos enteros (en la clínica cada puesto es otro
                // ordenador), al día con la revisión del servidor; solo envía sus cobros
                let mut d = crate::sync::skeleton().clone();
                let mut s = crate::sync::Snapshot::from_db(&d);
                d.rev = r.pull(0)?.rev;
                (d.station, d.next_id) = (r.station, next_id + 100_000);
                crate::sync::mark_synced(&mut d);
                let mut lat = vec![];
                for j in 0..pushes {
                    d.add_payment(pid, 1.0, "Efectivo", &format!("medición {i}-{j}"));
                    let t = Instant::now();
                    r.push(&mut d, &mut s)?;
                    lat.push(t.elapsed().as_secs_f64());
                }
                Ok(lat)
            })
        })
        .collect();
    let mut lat = vec![];
    for th in threads {
        lat.extend(th.join().map_err(|_| "Un puesto de la medición falló.".to_string())??);
    }
    let total = secs(t);
    // lo que quedó en el disco del servidor (con su diario)
    let (disk, err) = Db::load_from(&path, false);
    if let Some(e) = err {
        return Err(e);
    }
    let lost = (0..stations).flat_map(|i| (0..pushes).map(move |j| format!("medición {i}-{j}"))).filter(|c| !disk.payments.iter().any(|p| &p.concept == c)).count();
    login("Parada")?.stop()?;
    let _ = h.join();
    Ok((lat, total, lost))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La medición, en pequeño, dentro de la batería: sale entera, con
    /// valores con sentido, y con varios puestos a la vez no se pierde
    /// ningún guardado.
    #[test]
    fn a_small_measurement_runs_and_is_complete() {
        let dir = std::env::temp_dir().join(format!("vd_medir_{}", std::process::id()));
        let r = measure(0.01, 2, 3, 4, &dir).unwrap();
        assert_eq!(r.patients, 80);
        for name in ["tamaño de los datos", "guardado local de un cambio", "cambio enviado (una cita y un cobro)", "entrada de un puesto (recepción), comprimida", "varias clínicas: en disco", "concurrencia: guardado de un puesto, mediana"] {
            let v = r.get(name).unwrap_or_else(|| panic!("falta «{name}»"));
            assert!(v.is_finite() && v > 0.0, "{name}: {v}");
        }
        assert_eq!(r.get("concurrencia: guardados perdidos"), Some(0.0), "con tres puestos a la vez no se pierde nada");
        assert!(r.get("cambio enviado (una cita y un cobro)").unwrap() < 10.0, "un cambio son pocos KB, no los datos enteros");
        assert!(r.markdown().contains("| guardado local de un cambio |"));
        assert!(!dir.exists(), "no deja nada");
    }
}
