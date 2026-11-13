//! Instrumentos del piloto (T-F3-09): lo que tarda dar una cita, cobrar y
//! cerrar la caja, y en cuántos pasos (clics y teclas); las incidencias; las
//! caídas de la conexión con el servidor (disponibilidad); y las
//! restauraciones y sus ensayos.
//! - **Sin contenido clínico:** no se apunta qué paciente, qué tratamiento ni
//!   cuánto se cobró. Las notas de las incidencias se minimizan como el
//!   registro de errores.
//! - **Exportable** en CSV para comparar después.

use crate::model::Db;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Cita,
    Cobro,
    CierreCaja,
    Incidencia,
    /// Sin conexión con el servidor (con cuánto duró).
    Caida,
    Restauracion,
    Ensayo,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Cita => "Dar cita",
            Kind::Cobro => "Cobrar",
            Kind::CierreCaja => "Cerrar caja",
            Kind::Incidencia => "Incidencia",
            Kind::Caida => "Sin conexión",
            Kind::Restauracion => "Restauración",
            Kind::Ensayo => "Ensayo de restauración",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    pub id: u32,
    pub at: NaiveDateTime,
    pub kind: Kind,
    /// Milisegundos (de la tarea, o de la caída).
    #[serde(default)]
    pub ms: u64,
    #[serde(default)]
    pub steps: u32,
    #[serde(default)]
    pub station: u32,
    #[serde(default = "yes")]
    pub ok: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

fn yes() -> bool {
    true
}

/// Una tarea que se está midiendo (en memoria; al terminarla se apunta).
#[derive(Clone, Debug)]
pub struct Timing {
    pub kind: Kind,
    pub start: std::time::Instant,
    pub steps: u32,
}

impl Timing {
    pub fn new(kind: Kind) -> Timing {
        Timing { kind, start: std::time::Instant::now(), steps: 0 }
    }
}

impl Db {
    /// Apunta una medida del piloto.
    pub fn measure(&mut self, kind: Kind, ms: u64, steps: u32, ok: bool, note: &str) {
        let id = self.new_id();
        let note = if note.is_empty() { String::new() } else { crate::registro::scrub_with(note, &crate::registro::name_tokens(self)) };
        let station = self.station;
        self.measures.push(Measure { id, at: crate::model::now(), kind, ms, steps, station, ok, note });
    }
}

/// Mediana (la mitad tarda menos).
fn median(mut v: Vec<u64>) -> Option<u64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2 })
}

/// Resumen de un periodo: por tarea, cuántas, segundos y pasos (mediana);
/// incidencias; caídas y su tiempo; restauraciones y ensayos (bien/mal).
pub fn summary(list: &[Measure], from: NaiveDateTime, to: NaiveDateTime) -> Vec<(String, String)> {
    let r: Vec<&Measure> = list.iter().filter(|m| m.at >= from && m.at <= to).collect();
    let mut out = vec![];
    for k in [Kind::Cita, Kind::Cobro, Kind::CierreCaja] {
        let v: Vec<&&Measure> = r.iter().filter(|m| m.kind == k).collect();
        let secs = median(v.iter().map(|m| m.ms).collect()).map(|ms| format!("{:.1} s", ms as f64 / 1000.0)).unwrap_or_else(|| "—".into());
        let steps = median(v.iter().map(|m| m.steps as u64).collect()).map(|s| format!("{s} pasos")).unwrap_or_else(|| "—".into());
        out.push((k.label().to_string(), format!("{} veces · mediana {secs} y {steps}", v.len())));
    }
    let inc = r.iter().filter(|m| m.kind == Kind::Incidencia).count();
    out.push(("Incidencias".into(), inc.to_string()));
    let down: Vec<&&Measure> = r.iter().filter(|m| m.kind == Kind::Caida).collect();
    let total = down.iter().map(|m| m.ms).sum::<u64>();
    let span = (to - from).num_milliseconds().max(1) as f64;
    out.push(("Sin conexión con el servidor".into(), format!("{} veces · {} min en total · disponible el {:.2} %", down.len(), total / 60_000, 100.0 * (1.0 - total as f64 / span).max(0.0))));
    for k in [Kind::Restauracion, Kind::Ensayo] {
        let v: Vec<&&Measure> = r.iter().filter(|m| m.kind == k).collect();
        out.push((k.label().to_string(), format!("{} ({} bien, {} con fallos)", v.len(), v.iter().filter(|m| m.ok).count(), v.iter().filter(|m| !m.ok).count())));
    }
    out
}

/// CSV (separado por «;», como lo abre una hoja de cálculo en español).
pub fn csv(list: &[Measure]) -> String {
    let mut s = String::from("fecha;hora;tipo;segundos;pasos;puesto;resultado;nota\n");
    for m in list {
        let note = crate::model::sheet_cell(&m.note, ';');
        s += &format!("{};{};{};{:.1};{};{};{};{}\n", m.at.format("%d/%m/%Y"), m.at.format("%H:%M:%S"), m.kind.label(), m.ms as f64 / 1000.0, m.steps, m.station, if m.ok { "bien" } else { "con fallos" }, note);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(h: u32, m: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 13).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    fn mk(id: u32, when: NaiveDateTime, kind: Kind, ms: u64, steps: u32, ok: bool) -> Measure {
        Measure { id, at: when, kind, ms, steps, station: 1, ok, note: String::new() }
    }

    #[test]
    fn the_summary_gives_medians_availability_and_restorations() {
        let list = vec![
            mk(1, at(9, 0), Kind::Cita, 20_000, 8, true),
            mk(2, at(9, 5), Kind::Cita, 40_000, 12, true),
            mk(3, at(9, 9), Kind::Cita, 30_000, 10, true),
            mk(4, at(10, 0), Kind::Cobro, 15_000, 5, true),
            mk(5, at(11, 0), Kind::Caida, 6 * 60_000, 0, true),
            mk(6, at(12, 0), Kind::Incidencia, 0, 0, true),
            mk(7, at(13, 0), Kind::Ensayo, 0, 0, true),
            mk(8, at(13, 5), Kind::Restauracion, 0, 0, false),
            mk(9, at(23, 0), Kind::Cita, 1, 1, true),
        ];
        let s = summary(&list, at(8, 0), at(20, 0));
        let get = |k: &str| s.iter().find(|x| x.0 == k).unwrap().1.clone();
        assert_eq!(get("Dar cita"), "3 veces · mediana 30.0 s y 10 pasos", "la de las 23:00 queda fuera");
        assert_eq!(get("Cobrar"), "1 veces · mediana 15.0 s y 5 pasos");
        assert_eq!(get("Cerrar caja"), "0 veces · mediana — y —");
        assert_eq!(get("Incidencias"), "1");
        assert_eq!(get("Sin conexión con el servidor"), "1 veces · 6 min en total · disponible el 99.17 %");
        assert_eq!(get("Ensayo de restauración"), "1 (1 bien, 0 con fallos)");
        assert_eq!(get("Restauración"), "1 (0 bien, 1 con fallos)");
    }

    /// En lo apuntado y en lo exportado no hay datos de pacientes: ni sus
    /// nombres en las notas ni nada de su historia o lo que pagaron.
    #[test]
    fn measures_carry_no_patient_data() {
        let mut db = Db::sample(crate::model::today());
        let p = db.patients[0].clone();
        db.measure(Kind::Cobro, 12_345, 6, true, "");
        db.measure(Kind::Incidencia, 0, 0, true, &format!("La impresora no imprimió el recibo de {} {} (DNI {})", p.first_name, p.last_name, p.nif));
        let out = csv(&db.measures);
        assert!(out.starts_with("fecha;hora;tipo;segundos;pasos;puesto;resultado;nota\n"));
        assert!(out.contains(";Cobrar;12.3;6;"), "{out}");
        assert!(out.contains("La impresora no imprimió el recibo de <nombre> <nombre>"), "{out}");
        for secret in [&p.first_name, &p.last_name] {
            assert!(!out.contains(secret.as_str()), "{secret}");
        }
        assert!(p.nif.is_empty() || !out.contains(&p.nif));
        // una nota que empieza como una fórmula no se ejecuta al abrir el CSV
        db.measure(Kind::Incidencia, 0, 0, true, "=1+1");
        db.measure(Kind::Incidencia, 0, 0, true, "+SUMA(A1)");
        let out = csv(&db.measures);
        assert!(out.contains(";'=1+1\n") && out.contains(";'+SUMA(A1)\n"), "{out}");
        assert_eq!(crate::model::sheet_cell("-2+3;x\ny", ';'), "'-2+3 x y");
        assert_eq!(crate::model::sheet_cell("normal", ';'), "normal");
        let json = serde_json::to_string(&db.measures).unwrap();
        assert!(!json.contains("patient") && !json.contains("amount"), "{json}");
    }
}
