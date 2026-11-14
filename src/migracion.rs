//! Migración desde otro programa de gestión (GESDEN, Novaclinic, Clinic
//! Cloud, Dentalink u otro que exporte a Excel/CSV o guarde en SQL Server).
//!
//! El origen son hojas, una por clase de dato ([`Entity`]), con sus columnas
//! asignadas a los campos de VeodarDental ([`Entity::fields`]).
//! - **Identificador de origen:** cada registro lleva el suyo, y se guarda en
//!   `Db::imports`. Importar otra vez no duplica nada, y una importación cortada
//!   se termina repitiéndola.
//! - **Ninguna fila se pierde:** cada una se importa, ya estaba importada o
//!   sale en el informe con su número de fila y el porqué.
//! - **Duplicados:** un paciente del origen que se parece a uno que ya está
//!   aquí no se trata solo; se decide con [`DupPolicy`], para todos o uno a
//!   uno. Dos personas nunca se unen por compartir el NIF o el número de
//!   historia: para unirlos, el nombre tiene que ser el mismo.
//! - **Conciliación:** el informe compara el origen y el destino por paciente
//!   y en total (pacientes, deuda, citas futuras, presupuestos abiertos y
//!   tratamientos pendientes), con importes exactos en céntimos.
//! - **La deuda no se cuenta dos veces:** la que dice el programa anterior se
//!   compara con la que sale aquí de los tratamientos y cobros importados, y
//!   el ajuste (si se pide) es solo la diferencia.
//!
//! El origen solo se lee. El simulacro ([`dry_run`]) trabaja sobre una copia.
//! Lo de antes no gasta material ni crea avisos, y lo hecho allí no se vuelve
//! a facturar ni a liquidar a la mutua aquí (`MedEntry::settled_elsewhere`).

use crate::cents::Cents;
use crate::extra::Sheet;
use crate::model::*;
use chrono::{Datelike, NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Código del apunte que ajusta el saldo al del programa anterior: cuenta en
/// lo que debe el paciente, pero no es un acto (ver `MedEntry::is_act`).
pub const ADJUST_CODE: &str = "MIGSALDO";

/// Un registro importado: de qué programa, qué clase de dato y con qué
/// identificador allí, y qué registro es aquí.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImportRef {
    pub id: u32,
    pub source: String,
    pub entity: Entity,
    /// Tabla o fichero de donde salió (informativo: un registro es el mismo
    /// aunque se exporte otra vez con otro nombre).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub table: String,
    pub key: String,
    pub local: u32,
    /// Paciente que ya estaba aquí y al que se unió el del origen.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub linked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Entity {
    Pacientes,
    Tratamientos,
    Presupuestos,
    Citas,
    Cobros,
    Saldos,
    Documentos,
}

/// Un campo de VeodarDental: clave, nombre para la pantalla, si hace falta y
/// cómo suele llamarse la columna en las exportaciones (para asignarla sola).
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub required: bool,
    pub aliases: &'static [&'static str],
}

const fn f(key: &'static str, label: &'static str, required: bool, aliases: &'static [&'static str]) -> Field {
    Field { key, label, required, aliases }
}

const PATIENT: &[&str] = &["id paciente", "idpaciente", "id_paciente", "codigo paciente", "cod paciente", "paciente"];
const PACIENTES: &[Field] = &[
    f("id", "Identificador en el programa anterior", true, &["id paciente", "idpaciente", "id_paciente", "codigo paciente", "cod paciente", "codigo", "id"]),
    f("nombre", "Nombre", true, &["nombre"]),
    f("apellidos", "Apellidos", false, &["apellidos", "apellido"]),
    f("nif", "NIF / NIE", false, &["nif", "dni", "nie", "documento"]),
    f("nacimiento", "Fecha de nacimiento", false, &["fecha nacimiento", "nacimiento", "fecha nac", "f nac"]),
    f("sexo", "Sexo", false, &["sexo", "genero"]),
    f("movil", "Móvil", false, &["movil", "celular"]),
    f("telefono", "Teléfono", false, &["telefono", "tlf", "tfno"]),
    f("email", "Correo electrónico", false, &["email", "correo", "e-mail", "mail"]),
    f("direccion", "Dirección", false, &["direccion", "domicilio"]),
    f("cp", "Código postal", false, &["codigo postal", "cp"]),
    f("poblacion", "Población", false, &["poblacion", "localidad", "ciudad", "municipio"]),
    f("provincia", "Provincia", false, &["provincia"]),
    f("historia", "Nº de historia", false, &["nº historial", "n historial", "historial", "n historia", "num historia", "historia", "nhc"]),
    f("alta", "Fecha de alta", false, &["fecha alta", "alta"]),
    f("mutua", "Mutua o tarifa", false, &["mutua", "aseguradora", "compania", "tarifa"]),
    f("alertas", "Alertas médicas", false, &["alertas", "alerta", "antecedentes"]),
    f("alergias", "Alergias", false, &["alergias", "alergia"]),
    f("notas", "Observaciones", false, &["observaciones", "notas"]),
    f("responsable", "Responsable (quien paga o firma)", false, &["responsable", "tutor", "pagador"]),
    f("responsable_nif", "NIF del responsable", false, &["nif responsable", "dni responsable", "nif tutor", "dni tutor"]),
    f("responsable_relacion", "Relación con el paciente", false, &["relacion", "parentesco"]),
];
const TRATAMIENTOS: &[Field] = &[
    f("id", "Identificador del tratamiento", true, &["id tratamiento", "id_tratamiento", "id"]),
    f("paciente", "Identificador del paciente", true, PATIENT),
    f("fecha", "Fecha", true, &["fecha"]),
    f("descripcion", "Tratamiento", true, &["descripcion", "tratamiento", "concepto"]),
    f("codigo", "Código", false, &["codigo"]),
    f("piezas", "Piezas", false, &["piezas", "pieza", "dientes"]),
    f("importe", "Importe", true, &["importe", "precio", "total"]),
    f("estado", "Estado (hecho o pendiente)", false, &["estado", "realizado"]),
    f("doctor", "Doctor", false, &["doctor", "profesional", "odontologo"]),
];
const PRESUPUESTOS: &[Field] = &[
    f("id", "Identificador del presupuesto", true, &["id presupuesto", "id_presupuesto", "presupuesto", "id"]),
    f("linea", "Identificador de la línea", false, &["id linea", "id_linea", "linea"]),
    f("paciente", "Identificador del paciente", true, PATIENT),
    f("fecha", "Fecha", true, &["fecha"]),
    f("descripcion", "Línea (tratamiento)", true, &["descripcion", "tratamiento", "concepto"]),
    f("codigo", "Código", false, &["codigo"]),
    f("piezas", "Piezas", false, &["piezas", "pieza"]),
    f("importe", "Importe de la línea", true, &["importe", "precio", "total"]),
    f("titulo", "Título", false, &["titulo", "nombre presupuesto"]),
    f("estado", "Estado", false, &["estado"]),
    f("doctor", "Doctor", false, &["doctor", "profesional"]),
];
const CITAS: &[Field] = &[
    f("id", "Identificador de la cita", true, &["id cita", "id_cita", "id"]),
    f("paciente", "Identificador del paciente", true, PATIENT),
    f("fecha", "Fecha", true, &["fecha"]),
    f("hora", "Hora", true, &["hora", "inicio"]),
    f("minutos", "Duración (minutos)", false, &["duracion", "minutos"]),
    f("motivo", "Motivo", false, &["motivo", "observaciones", "concepto"]),
    f("doctor", "Doctor", false, &["doctor", "profesional"]),
    f("estado", "Estado", false, &["estado"]),
];
const COBROS: &[Field] = &[
    f("id", "Identificador del cobro", true, &["id cobro", "id_cobro", "recibo", "id"]),
    f("paciente", "Identificador del paciente", true, PATIENT),
    f("fecha", "Fecha", true, &["fecha"]),
    f("importe", "Importe", true, &["importe", "cantidad", "total"]),
    f("forma", "Forma de pago", false, &["forma de pago", "forma", "medio"]),
    f("concepto", "Concepto", false, &["concepto", "observaciones"]),
];
const SALDOS: &[Field] = &[f("paciente", "Identificador del paciente", true, PATIENT), f("saldo", "Saldo (positivo: debe)", true, &["saldo", "deuda", "pendiente"])];
const DOCUMENTOS: &[Field] = &[
    f("id", "Identificador del documento", true, &["id documento", "id_documento", "id"]),
    f("paciente", "Identificador del paciente", true, PATIENT),
    f("ruta", "Fichero (ruta)", true, &["ruta", "fichero", "archivo", "path"]),
    f("titulo", "Título", false, &["titulo", "descripcion", "nombre"]),
    f("fecha", "Fecha", false, &["fecha"]),
    f("tipo", "Tipo", false, &["tipo"]),
];

impl Entity {
    pub const ALL: [Entity; 7] = [Entity::Pacientes, Entity::Tratamientos, Entity::Presupuestos, Entity::Citas, Entity::Cobros, Entity::Saldos, Entity::Documentos];

    pub fn label(self) -> &'static str {
        match self {
            Entity::Pacientes => "Pacientes y responsables",
            Entity::Tratamientos => "Tratamientos hechos y pendientes",
            Entity::Presupuestos => "Presupuestos",
            Entity::Citas => "Citas",
            Entity::Cobros => "Cobros",
            Entity::Saldos => "Saldos",
            Entity::Documentos => "Documentos e imágenes",
        }
    }

    /// Nombre corto (para la lista de hojas).
    pub fn short(self) -> &'static str {
        match self {
            Entity::Pacientes => "Pacientes",
            Entity::Tratamientos => "Tratamientos",
            Entity::Presupuestos => "Presupuestos",
            Entity::Citas => "Citas",
            Entity::Cobros => "Cobros",
            Entity::Saldos => "Saldos",
            Entity::Documentos => "Documentos",
        }
    }

    pub fn fields(self) -> &'static [Field] {
        match self {
            Entity::Pacientes => PACIENTES,
            Entity::Tratamientos => TRATAMIENTOS,
            Entity::Presupuestos => PRESUPUESTOS,
            Entity::Citas => CITAS,
            Entity::Cobros => COBROS,
            Entity::Saldos => SALDOS,
            Entity::Documentos => DOCUMENTOS,
        }
    }
}

/// Título de columna para compararlo: sin acentos ni puntos, en minúsculas
/// y con un espacio entre palabras («D.N.I.» → «dni», «F. Nac.» → «f nac»).
fn header_words(s: &str) -> String {
    fold(&s.replace('.', "")).split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Columna de la hoja para cada campo de la clase de dato, por cómo se
/// llaman las columnas: primero el nombre exacto y después el que lo
/// contiene como palabras («Apellidos del paciente» no es «id»). Cada
/// columna se usa una vez.
pub fn auto_map(entity: Entity, headers: &[String]) -> Vec<Option<usize>> {
    let h: Vec<String> = headers.iter().map(|x| header_words(x)).collect();
    let contains = |name: &str, alias: &str| format!(" {name} ").contains(&format!(" {alias} "));
    let mut used = HashSet::new();
    let mut out = vec![None; entity.fields().len()];
    for exact in [true, false] {
        for (i, fd) in entity.fields().iter().enumerate() {
            if out[i].is_some() {
                continue;
            }
            let hit = fd.aliases.iter().find_map(|a| h.iter().enumerate().find(|(c, name)| !used.contains(c) && if exact { name.as_str() == *a } else { contains(name, a) }).map(|(c, _)| c));
            if let Some(c) = hit {
                used.insert(c);
                out[i] = Some(c);
            }
        }
    }
    out
}

/// Una hoja del origen con sus columnas asignadas.
#[derive(Clone, Debug)]
pub struct Table {
    pub entity: Entity,
    pub sheet: Sheet,
    pub map: Vec<Option<usize>>,
    /// De dónde se leyó (fichero, tabla o consulta).
    pub origin: String,
}

impl Table {
    /// Con las columnas asignadas solas.
    pub fn new(entity: Entity, sheet: Sheet) -> Table {
        let map = auto_map(entity, &sheet.headers);
        Table { entity, sheet, map, origin: String::new() }
    }

    /// Asigna las columnas como dice un perfil guardado (las que no estén en
    /// la hoja se quedan como estaban). Devuelve cuántas asignó.
    pub fn apply(&mut self, profile: &Profile) -> usize {
        let Some(cols) = profile.sheets.iter().find(|(e, _)| *e == self.entity).map(|(_, c)| c) else { return 0 };
        let h: Vec<String> = self.sheet.headers.iter().map(|x| header_words(x)).collect();
        let mut n = 0;
        for (i, fd) in self.entity.fields().iter().enumerate() {
            if let Some((_, head)) = cols.iter().find(|(k, _)| k == fd.key)
                && let Some(c) = h.iter().position(|x| *x == header_words(head))
            {
                self.map[i] = Some(c);
                n += 1;
            }
        }
        n
    }

    pub fn column(&self, key: &str) -> Option<usize> {
        let i = self.entity.fields().iter().position(|f| f.key == key)?;
        self.map.get(i).copied().flatten()
    }

    fn get(&self, row: &[String], key: &str) -> String {
        self.column(key).and_then(|c| row.get(c)).map(|s| s.trim().to_string()).unwrap_or_default()
    }

    /// Campos obligatorios sin columna.
    pub fn missing(&self) -> Vec<&'static str> {
        self.entity.fields().iter().enumerate().filter(|(i, f)| f.required && self.map.get(*i).copied().flatten().is_none()).map(|(_, f)| f.label).collect()
    }
}

/// El origen: el programa (sirve para los identificadores) y sus hojas.
#[derive(Clone, Debug, Default)]
pub struct Source {
    pub name: String,
    pub tables: Vec<Table>,
}

/// Perfil de columnas guardado: para cada clase de dato, qué columna (por su
/// título) va a cada campo. Se hace una vez con las exportaciones de un
/// programa y se elige las demás veces.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub sheets: Vec<(Entity, Vec<(String, String)>)>,
}

impl Profile {
    /// El perfil de unas hojas con sus columnas ya asignadas.
    pub fn from_tables<'a>(name: &str, tables: impl IntoIterator<Item = &'a Table>) -> Profile {
        let sheets = tables
            .into_iter()
            .map(|t| {
                let cols = t.entity.fields().iter().enumerate().filter_map(|(i, fd)| t.map.get(i).copied().flatten().and_then(|c| t.sheet.headers.get(c)).map(|h| (fd.key.to_string(), h.clone()))).collect();
                (t.entity, cols)
            })
            .collect();
        Profile { name: name.trim().to_string(), sheets }
    }
}

/// Qué hacer con un paciente del origen que se parece a uno que ya está aquí.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DupPolicy {
    /// No se importa y se explica (lo normal: lo decide la clínica).
    #[default]
    Skip,
    /// Es el mismo: su historia se importa en el que ya está. Solo con el
    /// mismo nombre; si solo coincide el NIF, no se une.
    Link,
    /// Son personas distintas: se crea otro.
    New,
}

impl DupPolicy {
    pub fn label(self) -> &'static str {
        match self {
            DupPolicy::Skip => "No importarlo",
            DupPolicy::Link => "Es el mismo: unirlo",
            DupPolicy::New => "Es otra persona: crearlo",
        }
    }
}

/// Qué hacer si la deuda que queda aquí no es la del programa anterior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BalancePolicy {
    /// Solo decirlo en el informe.
    #[default]
    Report,
    /// Apuntar un ajuste (código [`ADJUST_CODE`], en positivo o en negativo)
    /// con la diferencia, para que la deuda sea la misma. Nunca un cobro: no es
    /// dinero que entre. Una vez por paciente, y no en los unidos a uno que ya
    /// estaba (su saldo de aquí no es el del otro programa).
    Adjust,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub today: NaiveDate,
    /// Para los posibles duplicados sin decisión propia.
    pub duplicates: DupPolicy,
    /// Decisión para un paciente concreto (por su identificador en el origen).
    pub decisions: HashMap<String, DupPolicy>,
    pub balance: BalancePolicy,
    /// Doctor para lo que no dice cuál (o dice uno que no está).
    pub doctor_id: u32,
    /// Carpeta de los documentos con ruta relativa (vacía: tal cual).
    pub doc_root: String,
}

impl Options {
    pub fn new(today: NaiveDate, doctor_id: u32) -> Options {
        Options { today, duplicates: DupPolicy::Skip, decisions: HashMap::new(), balance: BalancePolicy::Report, doctor_id, doc_root: String::new() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    /// La fila no se puede leer (falta un dato o no tiene sentido).
    Error,
    /// No se importa por una razón (su paciente no está, o es un duplicado).
    Excluded,
    /// Se importa, pero con algo que conviene saber.
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub entity: Entity,
    /// Fila de la hoja (la 1 son los títulos); 0 = de toda la hoja.
    pub row: usize,
    pub kind: ProblemKind,
    pub reason: String,
}

/// Paciente del origen que se parece a uno que ya está aquí.
#[derive(Clone, Debug, PartialEq)]
pub struct Duplicate {
    pub key: String,
    pub row: usize,
    pub name: String,
    pub local: u32,
    pub history_no: u32,
    /// Mismo nombre (y NIF o fecha de nacimiento): se puede unir.
    pub same_person: bool,
    pub why: String,
    pub decision: DupPolicy,
}

/// Conciliación de un paciente: lo que dice el origen y lo que hay aquí.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatientRecon {
    pub key: String,
    pub local: u32,
    pub name: String,
    /// Deuda según el programa anterior (sin hoja de saldos: no se sabe).
    pub debt_source: Option<Cents>,
    pub debt_dest: Cents,
    pub future_appts_source: usize,
    pub future_appts_dest: usize,
    pub open_budgets_source: usize,
    pub open_budgets_dest: usize,
    pub pending_source: usize,
    pub pending_dest: usize,
    /// Unido a un paciente que ya estaba (lo de aquí cuenta también).
    pub linked: bool,
}

impl PatientRecon {
    /// Hay alguna diferencia entre el origen y aquí.
    pub fn differs(&self) -> bool {
        self.debt_source.is_some_and(|d| d != self.debt_dest) || self.future_appts_source != self.future_appts_dest || self.open_budgets_source != self.open_budgets_dest || self.pending_source != self.pending_dest
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Totals {
    /// Filas de pacientes en el origen.
    pub patients_source: usize,
    /// Pacientes del origen que están aquí (nuevos o unidos).
    pub patients_dest: usize,
    /// Deuda del origen, toda (de la hoja de saldos).
    pub debt_source: Cents,
    /// La parte de pacientes que no se importaron (está en los problemas).
    pub debt_excluded: Cents,
    /// Deuda aquí de los pacientes importados.
    pub debt_dest: Cents,
    pub future_appts_source: usize,
    pub future_appts_dest: usize,
    pub open_budgets_source: usize,
    pub open_budgets_dest: usize,
    pub pending_source: usize,
    pub pending_dest: usize,
    /// Ajustes de saldo apuntados (con `BalancePolicy::Adjust`).
    pub adjusted: usize,
    pub adjustments: Cents,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// Filas leídas de cada clase.
    pub rows: BTreeMap<Entity, usize>,
    pub added: BTreeMap<Entity, usize>,
    /// Ya importados antes (no se repiten).
    pub already: BTreeMap<Entity, usize>,
    pub problems: Vec<Problem>,
    pub duplicates: Vec<Duplicate>,
    pub patients: Vec<PatientRecon>,
    /// Hay hoja de saldos (si no, la deuda del origen no se sabe).
    pub balances_given: bool,
    pub totals: Totals,
}

impl Report {
    pub fn count(&self, kind: ProblemKind) -> usize {
        self.problems.iter().filter(|p| p.kind == kind).count()
    }

    /// Filas de una clase que no se importan (con su porqué en los problemas).
    pub fn left_out(&self, e: Entity) -> usize {
        self.problems.iter().filter(|p| p.entity == e && p.row > 0 && p.kind != ProblemKind::Warning).count()
    }

    fn n(m: &BTreeMap<Entity, usize>, e: Entity) -> usize {
        m.get(&e).copied().unwrap_or(0)
    }

    /// (filas, nuevos, ya estaban, sin importar) de una clase.
    pub fn counts(&self, e: Entity) -> (usize, usize, usize, usize) {
        (Report::n(&self.rows, e), Report::n(&self.added, e), Report::n(&self.already, e), self.left_out(e))
    }

    /// Una línea: lo nuevo, lo que ya estaba y lo que no se importa.
    pub fn summary(&self) -> String {
        let (added, already) = (self.added.values().sum::<usize>(), self.already.values().sum::<usize>());
        let left: usize = Entity::ALL.iter().map(|e| self.left_out(*e)).sum();
        let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
        format!(
            "{}, {} y {} ({}, {}); {}.",
            n(added, "registro nuevo", "registros nuevos"),
            n(already, "ya importado antes", "ya importados antes"),
            n(left, "fila sin importar", "filas sin importar"),
            n(self.count(ProblemKind::Error), "con error", "con error"),
            n(self.count(ProblemKind::Excluded), "excluida", "excluidas"),
            n(self.count(ProblemKind::Warning), "aviso", "avisos")
        )
    }

    /// Filas de la conciliación en total: (concepto, origen, aquí).
    pub fn totals_rows(&self) -> Vec<(&'static str, String, String)> {
        let t = &self.totals;
        let debt_source = if self.balances_given { (t.debt_source - t.debt_excluded).to_string() } else { "sin hoja de saldos".into() };
        let mut v = vec![("Pacientes", t.patients_source.to_string(), t.patients_dest.to_string()), ("Deuda de los pacientes", debt_source, t.debt_dest.to_string())];
        if !t.debt_excluded.is_zero() {
            v.push(("Deuda de pacientes sin importar", t.debt_excluded.to_string(), "—".into()));
        }
        v.extend([
            ("Citas futuras", t.future_appts_source.to_string(), t.future_appts_dest.to_string()),
            ("Presupuestos abiertos", t.open_budgets_source.to_string(), t.open_budgets_dest.to_string()),
            ("Tratamientos pendientes", t.pending_source.to_string(), t.pending_dest.to_string()),
        ]);
        v
    }

    /// Lo que no cuadra de un paciente, en palabras.
    pub fn differences(p: &PatientRecon) -> String {
        let mut v = vec![];
        if let Some(d) = p.debt_source.filter(|d| *d != p.debt_dest) {
            v.push(format!("deuda {d} allí y {} aquí", p.debt_dest));
        }
        for (what, a, b) in [("citas futuras", p.future_appts_source, p.future_appts_dest), ("presupuestos abiertos", p.open_budgets_source, p.open_budgets_dest), ("tratamientos pendientes", p.pending_source, p.pending_dest)] {
            if a != b {
                v.push(format!("{what}: {a} allí y {b} aquí"));
            }
        }
        if p.linked && !v.is_empty() {
            v.push("unido a uno que ya estaba: lo de aquí también cuenta".into());
        }
        v.join("; ")
    }

    /// El informe entero, en texto (para guardarlo o pegarlo).
    pub fn text(&self, source: &str, simulated: bool) -> String {
        let mut s = format!("{} de {source}\n{}\n\n", if simulated { "Simulacro de la migración (no se cambió nada)" } else { "Migración" }, self.summary());
        s += "Clase de dato\tFilas\tNuevos\tYa estaban\tSin importar\n";
        for e in Entity::ALL {
            let (r, a, y, l) = self.counts(e);
            s += &format!("{}\t{r}\t{a}\t{y}\t{l}\n", e.label());
        }
        s += "\nConciliación\tPrograma anterior\tVeodarDental\n";
        for (what, a, b) in self.totals_rows() {
            s += &format!("{what}\t{a}\t{b}\n");
        }
        if self.totals.adjusted > 0 {
            s += &format!("Ajustes de saldo apuntados\t{}\t{}\n", self.totals.adjusted, self.totals.adjustments);
        }
        let diff: Vec<&PatientRecon> = self.patients.iter().filter(|p| p.differs()).collect();
        if !diff.is_empty() {
            s += &format!("\nPacientes que no cuadran ({})\n", diff.len());
            for p in diff {
                let c = |x: &str| sheet_cell(x, '\t');
                s += &format!("{} {} (nº {}): {}\n", c(&p.key), c(&p.name), p.local, c(&Report::differences(p)));
            }
        }
        if !self.duplicates.is_empty() {
            s += &format!("\nPosibles duplicados ({})\n", self.duplicates.len());
            for d in &self.duplicates {
                let c = |x: &str| sheet_cell(x, '\t');
                s += &format!("Fila {}: {} {} parece el nº de historia {} ({}) → {}\n", d.row, c(&d.key), c(&d.name), d.history_no, d.why, d.decision.label());
            }
        }
        if !self.problems.is_empty() {
            s += "\nFilas con problemas y avisos\n";
            for p in &self.problems {
                let kind = match p.kind {
                    ProblemKind::Error => "Error",
                    ProblemKind::Excluded => "Sin importar",
                    ProblemKind::Warning => "Aviso",
                };
                let row = if p.row == 0 { "toda la hoja".to_string() } else { format!("fila {}", p.row) };
                s += &format!("{} · {row} · {kind}: {}\n", p.entity.label(), sheet_cell(&p.reason, '\t'));
            }
        }
        s
    }
}

/// Fecha de un movimiento (cita, cobro, tratamiento). Con año de dos cifras,
/// del 2000 en adelante: una cita de «03/02/27» es de 2027 (las fechas de
/// nacimiento van con `extra::parse_date`, que mira el año en curso).
fn parse_day(s: &str) -> Option<NaiveDate> {
    let d = crate::extra::parse_date(s)?;
    let date = s.trim().split([' ', 'T']).next().unwrap_or("");
    let parts: Vec<&str> = date.split(['/', '-', '.']).collect();
    let two_digits = parts.len() == 3 && parts[0].len() != 4 && parts[2].len() == 2;
    if two_digits && d.year() < 2000 { d.with_year(d.year() + 100) } else { Some(d) }
}

/// Hora sola o con la fecha delante («09:45», «1899-12-30 09:45:00», «9.45»).
fn parse_time(s: &str) -> Option<NaiveTime> {
    s.trim().split([' ', 'T']).find_map(|p| ["%H:%M", "%H:%M:%S", "%H.%M"].iter().find_map(|f| NaiveTime::parse_from_str(p, f).ok()))
}

/// Piezas en numeración FDI (11 a 48, y 51 a 85 de leche).
fn teeth(s: &str) -> Vec<u8> {
    s.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse::<u8>().ok()).filter(|t| matches!(t / 10, 1..=4) && (1..=8).contains(&(t % 10)) || matches!(t / 10, 5..=8) && (1..=5).contains(&(t % 10))).collect()
}

fn norm_nif(s: &str) -> String {
    s.trim().to_uppercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

fn norm_name(s: &str) -> String {
    fold(s).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Ruta de otro ordenador o de dispositivo de Windows: `\\servidor\…`,
/// `//servidor/…` o con las barras mezcladas, `\\?\UNC\…`, `\\.\…` o `\??\…`.
fn is_remote(path: &str) -> bool {
    let p = path.trim_start();
    let sep = |c: Option<char>| matches!(c, Some('\\') | Some('/'));
    let mut c = p.chars();
    (sep(c.next()) && sep(c.next())) || p.replace('/', "\\").starts_with("\\??\\")
}

/// ¿Está `path` dentro de la carpeta `root`? Sin «..», sin distinguir
/// mayúsculas y con las barras de los dos tipos.
fn under(path: &str, root: &str) -> bool {
    let norm = |s: &str| s.trim().replace('/', "\\").to_lowercase();
    let (p, r) = (norm(path), norm(root));
    let r = r.trim_end_matches('\\');
    !r.is_empty() && p.starts_with(&format!("{r}\\")) && !p.split('\\').any(|c| c == "..")
}

/// ¿Dice «pendiente» o «hecho»?
fn is_pending(estado: &str) -> Option<bool> {
    let e = fold(estado);
    if e.is_empty() {
        return None;
    }
    if ["pend", "previst", "planific", "presupuest"].iter().any(|k| e.contains(k)) || ["p", "0", "no", "n"].contains(&e.as_str()) {
        return Some(true);
    }
    if ["hech", "realiz", "termin", "finaliz"].iter().any(|k| e.contains(k)) || ["r", "1", "si", "s", "h"].contains(&e.as_str()) {
        return Some(false);
    }
    None
}

fn is_cancelled(estado: &str) -> bool {
    let e = fold(estado);
    ["anul", "cancel", "rechaz", "borr"].iter().any(|k| e.contains(k))
}

fn no_show(estado: &str) -> bool {
    let e = fold(estado);
    ["no acud", "falt", "no vino", "no asist", "no present"].iter().any(|k| e.contains(k))
}

/// Forma de pago de aquí para la del programa anterior.
fn pay_method(s: &str) -> Option<&'static str> {
    let f = fold(s);
    let has = |ks: &[&str]| ks.iter().any(|k| f.contains(k));
    if f.trim().is_empty() || has(&["efect", "metal", "contado", "cash"]) {
        Some("Efectivo")
    } else if has(&["tarj", "tpv", "visa", "datafono", "card"]) {
        Some("Tarjeta")
    } else if has(&["transf"]) {
        Some("Transferencia")
    } else if has(&["financ"]) {
        Some("Financiación")
    } else if has(&["bizum"]) {
        Some("Bizum")
    } else if has(&["domic", "recibo", "remesa"]) {
        Some("Domiciliación")
    } else {
        None
    }
}

/// Qué quiere decir una «M» suelta en la columna de sexo: si en la misma
/// columna hay «H» o «V», mujer; si hay «F», hombre (male); si no, no se sabe.
fn m_means(t: &Table) -> Option<&'static str> {
    let c = t.column("sexo")?;
    let vals: HashSet<String> = t.sheet.rows.iter().filter_map(|r| r.get(c)).map(|v| fold(v.trim())).collect();
    let hv = ["h", "v", "hombre", "varon"].iter().any(|x| vals.contains(*x));
    let f = ["f", "female", "femenino"].iter().any(|x| vals.contains(*x));
    match (hv, f) {
        (true, false) => Some("F"),
        (false, true) => Some("M"),
        _ => None,
    }
}

fn sex_of(v: &str, m: Option<&'static str>) -> Option<&'static str> {
    match fold(v.trim()).as_str() {
        "" => Some(""),
        "h" | "v" | "hombre" | "varon" | "masculino" | "male" => Some("M"),
        "f" | "mujer" | "femenino" | "female" => Some("F"),
        "m" => m,
        _ => None,
    }
}

/// Lo que dice el origen de un paciente, para la conciliación.
#[derive(Default)]
struct SourceCounts {
    future_appts: usize,
    open_budgets: HashSet<String>,
    pending: usize,
}

/// Lo que se lleva durante una importación.
struct Run<'a> {
    source: &'a str,
    opt: &'a Options,
    rep: Report,
    /// paciente del origen → paciente de aquí (también los de antes)
    pmap: HashMap<String, u32>,
    /// pacientes de aquí que vienen de este origen
    own: HashSet<u32>,
    /// pacientes del origen que no están aquí, y por qué
    left_out: HashMap<String, String>,
    /// pacientes del origen de esta importación, en orden
    keys: Vec<String>,
    seen: HashSet<String>,
    debt: HashMap<String, Cents>,
    counts: HashMap<String, SourceCounts>,
    /// nombres del origen que no están aquí: (clase, nombre) → filas
    unknown: BTreeMap<(Entity, String), usize>,
    missing_files: Vec<String>,
}

impl Run<'_> {
    fn problem(&mut self, entity: Entity, i: usize, kind: ProblemKind, reason: String) {
        self.rep.problems.push(Problem { entity, row: i + 2, kind, reason });
    }

    fn note(&mut self, entity: Entity, kind: ProblemKind, reason: String) {
        self.rep.problems.push(Problem { entity, row: 0, kind, reason });
    }

    fn involve(&mut self, key: &str) {
        if self.pmap.contains_key(key) && self.seen.insert(key.to_string()) {
            self.keys.push(key.to_string());
        }
    }

    fn count(map: &mut BTreeMap<Entity, usize>, e: Entity) {
        *map.entry(e).or_default() += 1;
    }
}

impl Db {
    fn imported(&self, source: &str, entity: Entity, key: &str) -> Option<u32> {
        self.imports.iter().find(|r| r.source == source && r.entity == entity && r.key == key).map(|r| r.local)
    }

    fn remember(&mut self, source: &str, entity: Entity, table: &str, key: &str, local: u32, linked: bool) {
        let id = self.new_id();
        self.imports.push(ImportRef { id, source: source.to_string(), entity, table: table.to_string(), key: key.to_string(), local, linked });
    }

    /// Doctor por su alias o su nombre en el programa anterior.
    fn doctor_named(&self, name: &str) -> Option<u32> {
        let n = norm_name(name);
        self.collaborators.iter().find(|c| norm_name(&c.alias) == n || norm_name(&c.name) == n || norm_name(&c.name).contains(&n)).map(|c| c.id)
    }

    fn tariff_named(&self, name: &str) -> Option<u32> {
        let n = norm_name(name);
        self.tariffs.iter().find(|t| norm_name(&t.name) == n).or_else(|| self.tariffs.iter().find(|t| norm_name(&t.name).contains(&n))).map(|t| t.id)
    }

    /// Paciente de aquí (que no venga de este mismo origen) que se parece al
    /// del origen: el mismo nombre con el mismo NIF o la misma fecha de
    /// nacimiento es la misma persona; solo el mismo NIF, no (hijos con el
    /// NIF de su madre, errores al teclear).
    fn lookalike(&self, own: &HashSet<u32>, nif: &str, name: &str, birth: Option<NaiveDate>) -> Option<(u32, u32, bool, &'static str)> {
        let others = || self.patients.iter().filter(|p| !own.contains(&p.id));
        let same_nif = |p: &Patient| !nif.is_empty() && norm_nif(&p.nif) == nif;
        let same_name = |p: &Patient| !name.is_empty() && norm_name(&p.full_name()) == name;
        others()
            .find(|p| same_name(p) && same_nif(p))
            .map(|p| (p.id, p.history_no, true, "mismo nombre y NIF"))
            .or_else(|| others().find(|p| same_name(p) && birth.is_some() && p.birth == birth).map(|p| (p.id, p.history_no, true, "mismo nombre y fecha de nacimiento")))
            .or_else(|| others().find(|p| same_nif(p)).map(|p| (p.id, p.history_no, false, "mismo NIF y otro nombre")))
    }

    /// Importa el origen (ver el módulo). Devuelve el informe con la
    /// conciliación. Se puede repetir: lo ya importado no se duplica.
    pub fn migrate(&mut self, src: &Source, opt: &Options) -> Report {
        let source = if src.name.trim().is_empty() { "Origen" } else { src.name.trim() };
        let pmap: HashMap<String, u32> = self.imports.iter().filter(|r| r.source == source && r.entity == Entity::Pacientes).map(|r| (r.key.clone(), r.local)).collect();
        let own = self.imports.iter().filter(|r| r.source == source && r.entity == Entity::Pacientes && !r.linked).map(|r| r.local).collect();
        let mut run = Run { source, opt, rep: Report::default(), pmap, own, left_out: HashMap::new(), keys: vec![], seen: HashSet::new(), debt: HashMap::new(), counts: HashMap::new(), unknown: BTreeMap::new(), missing_files: vec![] };
        run.rep.balances_given = src.tables.iter().any(|t| t.entity == Entity::Saldos);
        for t in src.tables.iter().filter(|t| t.entity == Entity::Pacientes) {
            self.migrate_patients(&mut run, t);
        }
        for t in src.tables.iter().filter(|t| t.entity != Entity::Pacientes) {
            self.migrate_rows(&mut run, t);
        }
        for ((e, name), n) in std::mem::take(&mut run.unknown) {
            let what = match e {
                Entity::Pacientes => format!("la mutua «{name}» no está en Tarifas: {n} pacientes quedan con la tarifa privada"),
                Entity::Cobros => format!("la forma de pago «{name}» no es de las de aquí: {n} cobros la conservan tal cual"),
                _ => format!("el doctor «{name}» no está en Colaboradores: sus {n} filas van al doctor de su paciente"),
            };
            run.note(e, ProblemKind::Warning, what);
        }
        if let Some(first) = run.missing_files.first().cloned() {
            let n = run.missing_files.len();
            run.note(Entity::Documentos, ProblemKind::Warning, format!("{n} ficheros no están en este ordenador (se guarda su ruta; por ejemplo, {first})"));
        }
        self.reconcile(&mut run);
        let mut rep = run.rep;
        let added: usize = rep.added.values().sum();
        self.audit("Migración", &format!("{source}: {added} registros nuevos, {} ya importados, {} filas sin importar", rep.already.values().sum::<usize>(), Entity::ALL.iter().map(|e| rep.left_out(*e)).sum::<usize>()));
        rep.duplicates.sort_by_key(|d| d.row);
        rep
    }

    fn migrate_patients(&mut self, run: &mut Run, t: &Table) {
        let e = Entity::Pacientes;
        let (source, opt) = (run.source, run.opt);
        let m = m_means(t);
        let mut ambiguous_sex = 0;
        let mut keys_here: HashMap<String, usize> = HashMap::new();
        for (i, row) in t.sheet.rows.iter().enumerate() {
            Run::count(&mut run.rep.rows, e);
            let key = t.get(row, "id");
            if key.is_empty() {
                run.problem(e, i, ProblemKind::Error, "sin identificador".into());
                continue;
            }
            if let Some(prev) = keys_here.insert(key.clone(), i) {
                run.problem(e, i, ProblemKind::Error, format!("paciente {key}: el identificador se repite (fila {}); no se importa dos veces", prev + 2));
                keys_here.insert(key.clone(), prev);
                continue;
            }
            let first = crate::extra::title_case(&t.get(row, "nombre"));
            if first.is_empty() {
                run.problem(e, i, ProblemKind::Error, format!("paciente {key}: sin nombre"));
                run.left_out.insert(key, "sin nombre".into());
                continue;
            }
            if run.pmap.contains_key(&key) {
                Run::count(&mut run.rep.already, e);
                run.involve(&key);
                continue;
            }
            let last = crate::extra::title_case(&t.get(row, "apellidos"));
            let nif = norm_nif(&t.get(row, "nif"));
            let birth_txt = t.get(row, "nacimiento");
            let birth = crate::extra::parse_date(&birth_txt).filter(|b| *b <= opt.today && b.year() >= 1900);
            if !birth_txt.is_empty() && birth.is_none() {
                run.problem(e, i, ProblemKind::Warning, format!("paciente {key}: fecha de nacimiento «{birth_txt}» no válida; se deja en blanco"));
            }
            let name = format!("{first} {last}").trim().to_string();
            if let Some((local, hist, same_person, why)) = self.lookalike(&run.own, &nif, &norm_name(&name), birth) {
                let decision = opt.decisions.get(&key).copied().unwrap_or(opt.duplicates);
                run.rep.duplicates.push(Duplicate { key: key.clone(), row: i + 2, name: name.clone(), local, history_no: hist, same_person, why: why.into(), decision });
                match (decision, same_person) {
                    (DupPolicy::Skip, _) => {
                        run.problem(e, i, ProblemKind::Excluded, format!("paciente {key}: parece el nº de historia {hist} ({why}); no se importa sin decidirlo"));
                        run.left_out.insert(key, format!("parece el nº de historia {hist}"));
                        continue;
                    }
                    (DupPolicy::Link, true) => {
                        run.pmap.insert(key.clone(), local);
                        self.remember(source, e, &t.origin, &key, local, true);
                        run.problem(e, i, ProblemKind::Warning, format!("paciente {key}: unido al nº de historia {hist} ({why})"));
                        Run::count(&mut run.rep.added, e);
                        run.involve(&key);
                        continue;
                    }
                    (DupPolicy::Link, false) => {
                        run.problem(e, i, ProblemKind::Excluded, format!("paciente {key}: comparte el NIF con el nº de historia {hist}, pero el nombre es otro; dos personas no se unen por el NIF (para importarlo, «crearlo»)"));
                        run.left_out.insert(key, format!("comparte el NIF con el nº de historia {hist}"));
                        continue;
                    }
                    (DupPolicy::New, _) => run.problem(e, i, ProblemKind::Warning, format!("paciente {key}: se crea aparte del nº de historia {hist} ({why})")),
                }
            }
            let wanted = t.get(row, "historia").parse::<u32>().ok().filter(|h| *h > 0);
            let history_no = match wanted {
                Some(h) => match self.patients.iter().find(|p| p.history_no == h) {
                    None => h,
                    Some(p) => {
                        let (other, next) = (p.full_name(), self.next_history_no());
                        run.problem(e, i, ProblemKind::Warning, format!("paciente {key}: el nº de historia {h} ya es de {other}; se le da el {next} (no se unen)"));
                        next
                    }
                },
                None => self.next_history_no(),
            };
            let mutua = t.get(row, "mutua");
            let tariff_id = match self.tariff_named(&mutua) {
                Some(id) if !mutua.is_empty() => id,
                _ => {
                    if !mutua.is_empty() && fold(&mutua) != "privado" {
                        *run.unknown.entry((e, mutua)).or_default() += 1;
                    }
                    1
                }
            };
            let sex = sex_of(&t.get(row, "sexo"), m).unwrap_or_else(|| {
                ambiguous_sex += 1;
                ""
            });
            let id = self.new_id();
            let payer = Payer { name: crate::extra::title_case(&t.get(row, "responsable")), nif: norm_nif(&t.get(row, "responsable_nif")), address: String::new(), relation: t.get(row, "responsable_relacion") };
            self.patients.push(Patient {
                id,
                history_no,
                first_name: first,
                last_name: last,
                nif: nif.clone(),
                birth,
                sex: sex.into(),
                phone: t.get(row, "telefono"),
                mobile: t.get(row, "movil"),
                email: t.get(row, "email"),
                address: t.get(row, "direccion"),
                zip: t.get(row, "cp"),
                city: crate::extra::title_case(&t.get(row, "poblacion")),
                province: crate::extra::title_case(&t.get(row, "provincia")),
                tariff_id,
                doctor_id: opt.doctor_id,
                alerts: t.get(row, "alertas"),
                allergies: t.get(row, "alergias"),
                notes: t.get(row, "notas"),
                registered: crate::extra::parse_date(&t.get(row, "alta")).or(Some(opt.today)),
                active: true,
                payer,
                ..Default::default()
            });
            if !nif.is_empty()
                && let Some(p) = self.patients.iter().find(|p| p.id != id && norm_nif(&p.nif) == nif)
            {
                let (h, n) = (p.history_no, p.full_name());
                run.problem(e, i, ProblemKind::Warning, format!("paciente {key}: comparte el NIF con {n} (nº {h}); quedan como dos pacientes"));
            }
            run.pmap.insert(key.clone(), id);
            run.own.insert(id);
            self.remember(source, e, &t.origin, &key, id, false);
            Run::count(&mut run.rep.added, e);
            run.involve(&key);
        }
        if ambiguous_sex > 0 {
            run.note(e, ProblemKind::Warning, format!("{ambiguous_sex} pacientes con un sexo que no se entiende (una «M» sin saber si es mujer o male): se deja en blanco"));
        }
    }

    fn migrate_rows(&mut self, run: &mut Run, t: &Table) {
        let e = t.entity;
        let (source, opt) = (run.source, run.opt);
        // un presupuesto son varias filas (una por línea) con el mismo identificador
        let mut lines: HashMap<String, usize> = HashMap::new();
        for (i, row) in t.sheet.rows.iter().enumerate() {
            Run::count(&mut run.rep.rows, e);
            let pkey = t.get(row, "paciente");
            let key = if e == Entity::Saldos { pkey.clone() } else { t.get(row, "id") };
            if key.is_empty() || pkey.is_empty() {
                run.problem(e, i, ProblemKind::Error, "sin identificador o sin paciente".into());
                continue;
            }
            let Some(&pid) = run.pmap.get(&pkey) else {
                let why = match run.left_out.get(&pkey) {
                    Some(w) => format!("el paciente {pkey} no se importó ({w})"),
                    None => format!("el paciente {pkey} no está en la hoja de pacientes"),
                };
                if e == Entity::Saldos
                    && let Some(v) = crate::extra::parse_money(&t.get(row, "saldo"))
                {
                    let v = Cents::from_eur(v);
                    run.rep.totals.debt_source += v;
                    run.rep.totals.debt_excluded += v;
                    run.problem(e, i, ProblemKind::Excluded, format!("saldo de {v}: {why}"));
                } else {
                    run.problem(e, i, ProblemKind::Excluded, format!("{key}: {why}"));
                }
                continue;
            };
            run.involve(&pkey);
            if e == Entity::Saldos {
                let Some(v) = crate::extra::parse_money(&t.get(row, "saldo")) else {
                    run.problem(e, i, ProblemKind::Error, format!("paciente {pkey}: saldo no válido «{}»", t.get(row, "saldo")));
                    continue;
                };
                *run.debt.entry(pkey).or_default() += Cents::from_eur(v);
                run.rep.totals.debt_source += Cents::from_eur(v);
                continue;
            }
            let date = match e {
                Entity::Documentos if t.get(row, "fecha").is_empty() => Some(opt.today),
                _ => parse_day(&t.get(row, "fecha")),
            };
            let Some(date) = date else {
                run.problem(e, i, ProblemKind::Error, format!("{key}: fecha no válida «{}»", t.get(row, "fecha")));
                continue;
            };
            let estado = t.get(row, "estado");
            // lo que dice el origen (para la conciliación), aunque ya estuviera importado
            let c = run.counts.entry(pkey.clone()).or_default();
            match e {
                Entity::Citas if date >= opt.today && !is_cancelled(&estado) && !no_show(&estado) => c.future_appts += 1,
                Entity::Presupuestos if !is_cancelled(&estado) && !fold(&estado).contains("cerr") => {
                    c.open_budgets.insert(key.clone());
                }
                Entity::Tratamientos if is_pending(&estado).unwrap_or(date > opt.today) => c.pending += 1,
                _ => {}
            }
            let ref_key = if e == Entity::Presupuestos {
                let line = t.get(row, "linea");
                let n = lines.entry(key.clone()).or_default();
                *n += 1;
                format!("{key}#{}", if line.is_empty() { n.to_string() } else { line })
            } else {
                key.clone()
            };
            if self.imported(source, e, &ref_key).is_some() {
                Run::count(&mut run.rep.already, e);
                continue;
            }
            let doctor_txt = t.get(row, "doctor");
            let own_doctor = self.patient(pid).map(|p| p.doctor_id).unwrap_or(opt.doctor_id);
            let doctor = if doctor_txt.is_empty() {
                own_doctor
            } else {
                self.doctor_named(&doctor_txt).unwrap_or_else(|| {
                    *run.unknown.entry((e, doctor_txt.clone())).or_default() += 1;
                    own_doctor
                })
            };
            let tariff = self.patient(pid).map(|p| p.tariff_id).unwrap_or(1);
            let amount = crate::extra::parse_money(&t.get(row, "importe"));
            if matches!(e, Entity::Tratamientos | Entity::Presupuestos | Entity::Cobros) && amount.is_none() {
                run.problem(e, i, ProblemKind::Error, format!("{key}: importe no válido «{}»", t.get(row, "importe")));
                continue;
            }
            let v = round2(amount.unwrap_or(0.0));
            let local = match e {
                Entity::Tratamientos => {
                    let pending = is_pending(&estado).unwrap_or(date > opt.today);
                    let code = t.get(row, "codigo");
                    let id = self.new_id();
                    self.entries.push(MedEntry {
                        id,
                        patient_id: pid,
                        date,
                        code: if code.is_empty() { "MIG".into() } else { code },
                        description: t.get(row, "descripcion"),
                        teeth: teeth(&t.get(row, "piezas")),
                        faces: vec![],
                        doctor_id: doctor,
                        state: if pending { TxState::Previsto } else { TxState::Realizado },
                        price: v,
                        discount_pct: 0.0,
                        units: 1,
                        budget_id: None,
                        notes: format!("Importado de {source} ({key})"),
                        odg: OdgKind::Ninguno,
                        invoice_id: None,
                        tariff_id: tariff,
                        center_id: 0,
                        // lo hecho allí se facturó y se liquidó allí; lo pendiente se hará aquí
                        settled_elsewhere: !pending,
                    });
                    id
                }
                Entity::Presupuestos => {
                    let line = BudgetLine { code: t.get(row, "codigo"), description: t.get(row, "descripcion"), teeth: teeth(&t.get(row, "piezas")), units: 1, price: v, discount_pct: 0.0, odg: OdgKind::Ninguno, phase: 0 };
                    match self.imported(source, e, &key).and_then(|bid| self.budgets.iter_mut().find(|b| b.id == bid)) {
                        Some(b) => {
                            b.lines.push(line);
                            b.id
                        }
                        None => {
                            let id = self.new_id();
                            let title = t.get(row, "titulo");
                            let s = fold(&estado);
                            let state = if is_cancelled(&estado) {
                                BudgetState::Rechazado
                            } else if s.contains("acept") {
                                BudgetState::Aceptado
                            } else if s.contains("cerr") || s.contains("termin") {
                                BudgetState::Cerrado
                            } else {
                                BudgetState::Pendiente
                            };
                            self.budgets.push(Budget {
                                id,
                                patient_id: pid,
                                title: if title.is_empty() { format!("Presupuesto {key} ({source})") } else { title },
                                date,
                                doctor_id: doctor,
                                tariff_id: tariff,
                                state,
                                lines: vec![line],
                                valid_days: 90,
                                accepted_on: (state == BudgetState::Aceptado).then_some(date),
                                rejected_on: (state == BudgetState::Rechazado).then_some(date),
                                reject_reason: String::new(),
                                notes: format!("Importado de {source} ({key})"),
                                plan: None,
                                tp: Default::default(),
                            });
                            self.remember(source, e, &t.origin, &key, id, false);
                            id
                        }
                    }
                }
                Entity::Citas => {
                    let Some(start) = parse_time(&t.get(row, "hora")) else {
                        run.problem(e, i, ProblemKind::Error, format!("{key}: hora no válida «{}»", t.get(row, "hora")));
                        continue;
                    };
                    let minutes = t.get(row, "minutos").parse::<u32>().ok().filter(|m| (5..=600).contains(m)).unwrap_or(30);
                    let state = if is_cancelled(&estado) {
                        ApptState::Anulada
                    } else if no_show(&estado) {
                        ApptState::NoAcude
                    } else if date < opt.today {
                        ApptState::Finalizada
                    } else if fold(&estado).contains("confirm") {
                        ApptState::Confirmada
                    } else {
                        ApptState::Planificada
                    };
                    let id = self.new_id();
                    let box_id = self.boxes.first().map(|b| b.id).unwrap_or(1);
                    self.appointments.push(Appointment {
                        id,
                        date,
                        start,
                        minutes,
                        patient_id: Some(pid),
                        free_name: String::new(),
                        phone: String::new(),
                        doctor_id: doctor,
                        box_id,
                        reason: t.get(row, "motivo"),
                        treatment_code: String::new(),
                        state,
                        notes: format!("Importada de {source} ({key})"),
                        arrived_at: None,
                        seen_at: None,
                        finished_at: None,
                        // las pasadas no llevan recordatorio; las futuras, el de siempre
                        reminder_sent: date < opt.today,
                        first_visit: false,
                        center_id: 1,
                    });
                    id
                }
                Entity::Cobros => {
                    if v == 0.0 {
                        run.problem(e, i, ProblemKind::Error, format!("{key}: cobro de 0 €"));
                        continue;
                    }
                    let forma = t.get(row, "forma");
                    let method = match pay_method(&forma) {
                        Some(m) => m.to_string(),
                        None => {
                            *run.unknown.entry((e, forma.clone())).or_default() += 1;
                            crate::extra::title_case(&forma)
                        }
                    };
                    let concept = t.get(row, "concepto");
                    let id = self.new_id();
                    // el recibo es el del programa anterior: no gasta la numeración de aquí,
                    // y sin caja: no descuadra la de hoy
                    self.payments.push(Payment {
                        id,
                        patient_id: pid,
                        date: date.and_hms_opt(12, 0, 0).unwrap_or_default(),
                        amount: v,
                        method,
                        concept: if concept.is_empty() { format!("Cobro importado de {source}") } else { concept },
                        receipt_no: format!("{source}-{key}"),
                        user: format!("Importación {source}"),
                        cash_session: None,
                        center_id: 0,
                        cancels: None,
                        installment: None,
                    });
                    id
                }
                Entity::Documentos => {
                    let raw = t.get(row, "ruta");
                    if raw.is_empty() {
                        run.problem(e, i, ProblemKind::Error, format!("{key}: sin fichero"));
                        continue;
                    }
                    let p = std::path::Path::new(&raw);
                    // la ruta se conserva: el fichero se queda donde está (el origen solo se lee)
                    let relative = !raw.starts_with(['/', '\\']) && !raw.contains(':');
                    let path = if relative && !opt.doc_root.trim().is_empty() { std::path::Path::new(opt.doc_root.trim()).join(p).to_string_lossy().to_string() } else { raw.clone() };
                    // una ruta de red solo si está en la carpeta que la clínica dice que es
                    // suya: abrirla (o solo mirarla) conecta con ese servidor y le da las
                    // credenciales del usuario de Windows
                    let remote = is_remote(&path);
                    if remote && !under(&path, &opt.doc_root) {
                        run.problem(
                            e,
                            i,
                            ProblemKind::Excluded,
                            format!("{key}: «{path}» está en otro ordenador, fuera de la carpeta de documentos indicada; no se importa (abrirlo daría a ese ordenador las credenciales de quien lo abra). Si es de la clínica, indique esa carpeta"),
                        );
                        continue;
                    }
                    if !remote && !std::path::Path::new(&path).exists() {
                        run.missing_files.push(path.clone());
                    }
                    let ext = p.extension().map(|x| fold(&x.to_string_lossy())).unwrap_or_default();
                    let tipo = fold(&t.get(row, "tipo"));
                    let picture = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "bmp" | "tif" | "tiff");
                    let kind = match ext.as_str() {
                        "stl" | "ply" | "obj" => ImageKind::Escaneo3D,
                        "dcm" => ImageKind::Tac,
                        _ if picture && tipo.contains("foto") => ImageKind::FotoIntraoral,
                        _ if picture && (tipo.contains("panor") || tipo.contains("orto")) => ImageKind::Panoramica,
                        _ if picture && (tipo.contains("rx") || tipo.contains("radio") || tipo.contains("periap")) => ImageKind::Periapical,
                        _ => ImageKind::Documento,
                    };
                    let id = self.new_id();
                    self.images.push(PatientImage { id, patient_id: pid, kind, date, teeth: vec![], path, source: format!("Importada de {source}"), notes: t.get(row, "titulo"), demo: false });
                    id
                }
                Entity::Pacientes | Entity::Saldos => continue,
            };
            self.remember(source, e, &t.origin, &ref_key, local, false);
            Run::count(&mut run.rep.added, e);
        }
    }

    /// Conciliación de los pacientes de esta importación (y el ajuste de
    /// saldo, si se pide).
    fn reconcile(&mut self, run: &mut Run) {
        let (source, opt) = (run.source, run.opt);
        let given = run.rep.balances_given;
        run.rep.totals.patients_source = run.rep.rows.get(&Entity::Pacientes).copied().unwrap_or(0);
        for key in std::mem::take(&mut run.keys) {
            let local = run.pmap[&key];
            let linked = self.imports.iter().any(|r| r.source == source && r.entity == Entity::Pacientes && r.key == key && r.linked);
            // con hoja de saldos, quien no sale en ella no debe nada allí
            let target = given.then(|| run.debt.get(&key).copied().unwrap_or_default());
            if opt.balance == BalancePolicy::Adjust
                && let Some(target) = target
                && self.imported(source, Entity::Saldos, &key).is_none()
            {
                let diff = target - Cents::from_eur(self.patient_balance(local));
                if linked {
                    if !diff.is_zero() {
                        run.note(Entity::Saldos, ProblemKind::Warning, format!("paciente {key}: unido a uno que ya estaba; su saldo no se ajusta solo (diferencia de {diff})"));
                    }
                } else if !diff.is_zero() {
                    let id = self.new_id();
                    let tariff = self.patient(local).map(|p| p.tariff_id).unwrap_or(1);
                    self.entries.push(MedEntry {
                        id,
                        patient_id: local,
                        date: opt.today,
                        code: ADJUST_CODE.into(),
                        description: format!("Saldo del programa anterior ({source})"),
                        teeth: vec![],
                        faces: vec![],
                        doctor_id: opt.doctor_id,
                        state: TxState::Realizado,
                        price: diff.eur(),
                        discount_pct: 0.0,
                        units: 1,
                        budget_id: None,
                        notes: format!("Ajuste de la migración: la deuda en {source} era {target}"),
                        odg: OdgKind::Ninguno,
                        invoice_id: None,
                        tariff_id: tariff,
                        center_id: 0,
                        settled_elsewhere: true,
                    });
                    run.rep.totals.adjusted += 1;
                    run.rep.totals.adjustments += diff;
                    self.remember(source, Entity::Saldos, "ajuste de saldo", &key, id, false);
                }
            }
            let c = run.counts.remove(&key).unwrap_or_default();
            let p = PatientRecon {
                key: key.clone(),
                local,
                name: self.patient_name(local),
                debt_source: target,
                debt_dest: Cents::from_eur(self.patient_balance(local)),
                future_appts_source: c.future_appts,
                future_appts_dest: self.appointments.iter().filter(|a| a.patient_id == Some(local) && a.date >= opt.today && !a.state.is_cancelled() && a.state != ApptState::Finalizada).count(),
                open_budgets_source: c.open_budgets.len(),
                open_budgets_dest: self.budgets.iter().filter(|b| b.patient_id == local && matches!(b.state, BudgetState::Pendiente | BudgetState::Aceptado)).count(),
                pending_source: c.pending,
                pending_dest: self.entries.iter().filter(|e| e.patient_id == local && e.state == TxState::Previsto).count(),
                linked,
            };
            let t = &mut run.rep.totals;
            t.patients_dest += 1;
            t.debt_dest += p.debt_dest;
            t.future_appts_source += p.future_appts_source;
            t.future_appts_dest += p.future_appts_dest;
            t.open_budgets_source += p.open_budgets_source;
            t.open_budgets_dest += p.open_budgets_dest;
            t.pending_source += p.pending_source;
            t.pending_dest += p.pending_dest;
            run.rep.patients.push(p);
        }
    }
}

/// Simulacro: la misma importación sobre una copia (los datos no cambian).
pub fn dry_run(db: &Db, src: &Source, opt: &Options) -> Report {
    let mut copy = db.clone();
    copy.migrate(src, opt)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 13).unwrap()
    }

    fn sheet(headers: &[&str], rows: &[&[&str]]) -> Sheet {
        Sheet { headers: headers.iter().map(|s| s.to_string()).collect(), rows: rows.iter().map(|r| r.iter().map(|s| s.to_string()).collect()).collect() }
    }

    /// DATOS SINTÉTICOS de un «programa anterior» inventado (no es el formato
    /// de GESDEN ni de ningún otro, y no son personas reales):
    /// - P1 y P2, nuevos (madre e hijo);
    /// - P3, el mismo nombre y NIF que el paciente 101 de ejemplo;
    /// - P4, otro nombre con el NIF del 101 (no es él);
    /// - una fila sin identificador.
    pub(crate) fn source(db: &Db) -> Source {
        let p101 = db.patient(101).unwrap().clone();
        Source {
            name: "PRUEBA".into(),
            tables: vec![
                Table::new(
                    Entity::Pacientes,
                    sheet(
                        &["IdPaciente", "Nombre", "Apellidos", "D.N.I.", "Fecha Nac.", "Sexo", "Movil", "Alergias", "Responsable", "Parentesco", "Nº Historia"],
                        &[
                            &["P1", "LUCÍA", "PÉREZ GÓMEZ", "11111111-H", "03/04/1980", "M", "600000001", "Penicilina", "", "", "7001"],
                            &["P2", "Mario", "Pérez Ruiz", "", "15/09/2015", "H", "600000002", "", "Lucía Pérez Gómez", "Madre", &p101.history_no.to_string()],
                            &["P3", &p101.first_name, &p101.last_name, &p101.nif, "", "", "", "", "", "", ""],
                            &["P4", "Otra", "Persona", &p101.nif, "31/02/1990", "", "", "", "", "", ""],
                            &["", "Sin", "Id", "", "", "", "", "", "", "", ""],
                        ],
                    ),
                ),
                Table::new(
                    Entity::Tratamientos,
                    sheet(
                        &["Id", "IdPaciente", "Fecha", "Tratamiento", "Piezas", "Importe", "Estado", "Doctor"],
                        &[
                            &["T1", "P1", "10/01/2026", "Obturación", "36", "60,00", "Realizado", ""],
                            &["T2", "P1", "20/11/2026", "Endodoncia", "46", "250,00", "Pendiente", ""],
                            &["T3", "P2", "05/05/2026", "Sellado", "16 26", "40", "R", "Dr. Nadie"],
                            &["T4", "P3", "01/02/2026", "Limpieza", "", "50", "R", ""],
                            &["T5", "P1", "fecha mala", "X", "", "1", "R", ""],
                            &["T6", "P9", "01/02/2026", "Limpieza", "", "50", "R", ""],
                        ],
                    ),
                ),
                Table::new(
                    Entity::Presupuestos,
                    sheet(
                        &["Id", "IdPaciente", "Fecha", "Tratamiento", "Importe", "Estado"],
                        &[&["B1", "P1", "01/10/2026", "Corona", "450", "Pendiente"], &["B2", "P2", "01/09/2026", "Ortodoncia", "1800", "Rechazado"], &["B1", "P1", "01/10/2026", "Perno", "120", "Pendiente"]],
                    ),
                ),
                Table::new(
                    Entity::Citas,
                    sheet(
                        &["Id", "IdPaciente", "Fecha", "Hora", "Duración", "Motivo", "Estado"],
                        &[
                            &["C1", "P1", "20/11/2026", "10:00", "45", "Endodoncia", ""],
                            &["C2", "P1", "10/01/2026", "09:30", "30", "Revisión", ""],
                            &["C3", "P2", "25/11/26", "17:15", "", "Revisión", "Anulada"],
                            &["C4", "P2", "02/12/2026", "hora mala", "", "", ""],
                            &["C5", "P2", "03/02/27", "16:00", "", "Revisión", ""],
                        ],
                    ),
                ),
                Table::new(
                    Entity::Cobros,
                    sheet(&["Id", "IdPaciente", "Fecha", "Importe", "Forma de pago"], &[&["R1", "P1", "10/01/2026", "60,00", "Tarjeta"], &["R2", "P2", "05/05/2026", "20,00", "Efectivo"], &["R3", "P2", "06/05/2026", "5,00", "Cheque"]]),
                ),
                Table::new(Entity::Saldos, sheet(&["IdPaciente", "Saldo"], &[&["P2", "30,00"], &["P4", "12,00"]])),
                Table::new(
                    Entity::Documentos,
                    sheet(
                        &["Id", "IdPaciente", "Ruta", "Titulo", "Tipo"],
                        &[&["D1", "P1", "C:\\Clinica\\Rx\\P1\\pano.jpg", "Panorámica", "Panorámica"], &["D2", "P1", "C:\\Clinica\\Docs\\consentimiento.pdf", "Consentimiento", ""], &["D3", "P1", "\\\\servidor-inexistente\\docs\\x.pdf", "En red", ""]],
                    ),
                ),
            ],
        }
    }

    fn opt() -> Options {
        Options::new(day(), 1)
    }

    fn has(rep: &Report, e: Entity, row: usize, k: ProblemKind, s: &str) -> bool {
        rep.problems.iter().any(|p| p.entity == e && p.row == row && p.kind == k && p.reason.contains(s))
    }

    #[test]
    fn columns_are_assigned_by_their_usual_names() {
        let m = auto_map(Entity::Pacientes, &["IdPaciente".into(), "Nombre".into(), "Apellidos del paciente".into(), "D.N.I.".into(), "Móvil".into()]);
        assert_eq!(&m[..4], &[Some(0), Some(1), Some(2), Some(3)], "«Apellidos del paciente» no es el identificador");
        assert_eq!(m[6], Some(4), "móvil");
        let t = Table::new(Entity::Citas, sheet(&["Id", "Paciente", "Fecha"], &[]));
        assert_eq!(t.missing(), vec!["Hora"]);
    }

    /// Importación completa de los datos sintéticos: lo importado, lo que no
    /// y por qué (con su fila), y la conciliación por paciente y en total.
    #[test]
    fn a_full_migration_with_its_reconciliation() {
        let mut db = Db::sample(day());
        let before = db.patients.len();
        let src = source(&db);
        let rep = db.migrate(&src, &opt());
        assert_eq!(rep.added.get(&Entity::Pacientes), Some(&2), "P3 y P4 se parecen al 101: no se importan sin decidirlo");
        assert_eq!(db.patients.len(), before + 2);
        let lucia = db.patients.iter().find(|p| p.nif == "11111111H").unwrap().clone();
        assert_eq!((lucia.first_name.as_str(), lucia.last_name.as_str(), lucia.allergies.as_str(), lucia.history_no), ("Lucía", "Pérez Gómez", "Penicilina", 7001));
        assert_eq!(lucia.sex, "F", "en una columna con «H», la «M» es mujer");
        let mario = db.patients.iter().find(|p| p.first_name == "Mario").unwrap().clone();
        assert_eq!((mario.payer.name.as_str(), mario.payer.relation.as_str(), mario.sex.as_str()), ("Lucía Pérez Gómez", "Madre", "M"));
        assert_ne!(mario.history_no, db.patient(101).unwrap().history_no, "un nº de historia repetido no une a nadie");
        // problemas, con su fila
        assert!(has(&rep, Entity::Pacientes, 3, ProblemKind::Warning, "ya es de"));
        assert!(has(&rep, Entity::Pacientes, 4, ProblemKind::Excluded, "mismo nombre y NIF"));
        assert!(has(&rep, Entity::Pacientes, 5, ProblemKind::Excluded, "mismo NIF y otro nombre"));
        assert!(has(&rep, Entity::Pacientes, 6, ProblemKind::Error, "sin identificador"));
        assert!(has(&rep, Entity::Tratamientos, 5, ProblemKind::Excluded, "P3 no se importó (parece el nº de historia"));
        assert!(has(&rep, Entity::Tratamientos, 6, ProblemKind::Error, "fecha no válida"));
        assert!(has(&rep, Entity::Tratamientos, 7, ProblemKind::Excluded, "P9 no está en la hoja de pacientes"));
        assert!(has(&rep, Entity::Tratamientos, 0, ProblemKind::Warning, "«Dr. Nadie»"));
        assert!(has(&rep, Entity::Citas, 5, ProblemKind::Error, "hora no válida"));
        assert!(has(&rep, Entity::Cobros, 0, ProblemKind::Warning, "«Cheque»"));
        assert!(has(&rep, Entity::Saldos, 3, ProblemKind::Excluded, "saldo de 12,00"));
        assert!(has(&rep, Entity::Documentos, 0, ProblemKind::Warning, "2 ficheros no están"), "la ruta de red no se mira");
        assert!(has(&rep, Entity::Documentos, 4, ProblemKind::Excluded, "está en otro ordenador"), "ni se importa sin decir que es de la clínica");
        assert_eq!(rep.duplicates.iter().map(|d| (d.key.as_str(), d.same_person)).collect::<Vec<_>>(), vec![("P3", true), ("P4", false)]);
        // ninguna fila se pierde: importada, ya estaba o explicada
        for e in Entity::ALL.into_iter().filter(|e| *e != Entity::Saldos) {
            let n = |m: &BTreeMap<Entity, usize>| m.get(&e).copied().unwrap_or(0);
            assert_eq!(n(&rep.rows), n(&rep.added) + n(&rep.already) + rep.left_out(e), "{e:?}");
        }
        // lo de antes no gasta material, y no se factura ni se liquida otra vez
        let t1 = db.entries.iter().find(|e| e.patient_id == lucia.id && e.description == "Obturación").unwrap().clone();
        assert_eq!((t1.state, t1.price, t1.teeth.clone(), t1.settled_elsewhere), (TxState::Realizado, 60.0, vec![36], true));
        assert!(!db.stock_moves.iter().any(|m| m.entry_id == Some(t1.id)));
        assert!(db.uninvoiced(lucia.id).is_empty());
        let t2 = db.entries.iter().find(|e| e.patient_id == lucia.id && e.description == "Endodoncia").unwrap();
        assert!(t2.state == TxState::Previsto && !t2.settled_elsewhere, "lo pendiente se hará y se facturará aquí");
        // presupuesto de dos líneas (separadas en la hoja): uno, con las dos
        let b = db.budgets.iter().find(|b| b.patient_id == lucia.id).unwrap();
        assert_eq!((b.lines.len(), b.total(), b.state), (2, 570.0, BudgetState::Pendiente));
        // citas: el año de dos cifras es del 2000 en adelante; la anulada, anulada
        let mut c = db.appointments.iter().filter(|a| a.patient_id == Some(mario.id)).map(|a| (a.date, a.state)).collect::<Vec<_>>();
        c.sort_by_key(|x| x.0);
        assert_eq!(c, vec![(NaiveDate::from_ymd_opt(2026, 11, 25).unwrap(), ApptState::Anulada), (NaiveDate::from_ymd_opt(2027, 2, 3).unwrap(), ApptState::Planificada)]);
        // cobros con el recibo del programa anterior y sin caja
        let r1 = db.payments.iter().find(|p| p.receipt_no == "PRUEBA-R1").unwrap();
        assert_eq!((r1.patient_id, r1.method.as_str(), r1.cash_session), (lucia.id, "Tarjeta", None));
        // documentos e imágenes: la ruta se conserva
        assert!(db.images.iter().any(|i| i.patient_id == lucia.id && i.kind == ImageKind::Panoramica && i.path.ends_with("pano.jpg")));
        assert!(db.images.iter().any(|i| i.patient_id == lucia.id && i.kind == ImageKind::Documento));
        // conciliación por paciente: P1 no sale en la hoja de saldos, así que allí no debe nada
        let r = |key: &str| rep.patients.iter().find(|p| p.key == key).unwrap().clone();
        let p1 = r("P1");
        assert_eq!((p1.future_appts_source, p1.future_appts_dest, p1.open_budgets_source, p1.open_budgets_dest, p1.pending_source, p1.pending_dest), (1, 1, 1, 1, 1, 1));
        assert_eq!((p1.debt_source, p1.debt_dest), (Some(Cents(0)), Cents(0)), "60 hecho − 60 cobrado");
        let p2 = r("P2");
        assert_eq!((p2.debt_source, p2.debt_dest), (Some(Cents(3000)), Cents(1500)), "el origen dice 30; aquí quedan 40 − 20 − 5");
        assert_eq!(p2.future_appts_dest, 1, "la anulada no cuenta");
        assert!(p2.differs() && !p1.differs());
        let t = &rep.totals;
        assert_eq!((t.patients_source, t.patients_dest), (5, 2));
        assert_eq!((t.debt_source, t.debt_excluded, t.debt_dest), (Cents(4200), Cents(1200), Cents(1500)));
        assert!(db.audit_log.iter().any(|a| a.action == "Migración"));
    }

    /// Repetirla no duplica nada; una importación cortada (solo pacientes y
    /// tratamientos) se termina repitiendo la completa, aunque las filas
    /// vengan en otro orden.
    #[test]
    fn repeating_or_finishing_a_migration_never_duplicates() {
        let mut db = Db::sample(day());
        let full = source(&db);
        let partial = Source { name: full.name.clone(), tables: full.tables[..2].to_vec() };
        db.migrate(&partial, &opt());
        let (p, e) = (db.patients.len(), db.entries.len());
        let rep = db.migrate(&full, &opt());
        assert_eq!((db.patients.len(), db.entries.len()), (p, e), "lo de antes no se repite");
        assert_eq!(rep.already.get(&Entity::Pacientes), Some(&2));
        assert_eq!(rep.already.get(&Entity::Tratamientos), Some(&3));
        assert_eq!(rep.added.get(&Entity::Citas), Some(&4), "y se termina lo que faltaba");
        let counts = |db: &Db| (db.patients.len(), db.entries.len(), db.budgets.iter().map(|b| b.lines.len()).sum::<usize>(), db.appointments.len(), db.payments.len(), db.images.len(), db.imports.len());
        let before = counts(&db);
        let mut shuffled = full.clone();
        for t in shuffled.tables.iter_mut() {
            t.sheet.rows.reverse();
        }
        let again = db.migrate(&shuffled, &opt());
        assert_eq!(counts(&db), before);
        assert!(again.added.is_empty(), "{:?}", again.added);
        assert_eq!(again.totals, rep.totals, "la misma conciliación");
    }

    /// Un posible duplicado se decide (para todos o uno a uno): se omite, se
    /// une al que ya está o se crea otro. Con solo el NIF igual, nunca se une.
    #[test]
    fn duplicates_are_decided_and_a_shared_nif_never_joins_two_people() {
        let before = Db::sample(day()).patients.len();
        let mut db = Db::sample(day());
        let link = Options { duplicates: DupPolicy::Link, ..opt() };
        let rep = db.migrate(&source(&db), &link);
        assert_eq!(db.patients.len(), before + 2, "P3 es el 101; P4 no se une ni se crea");
        assert!(db.entries.iter().any(|e| e.patient_id == 101 && e.description == "Limpieza"), "la historia de P3, en el 101");
        assert!(has(&rep, Entity::Pacientes, 5, ProblemKind::Excluded, "dos personas no se unen por el NIF"));
        assert!(rep.patients.iter().any(|p| p.key == "P3" && p.linked && p.local == 101));
        // uno a uno: P4 es otra persona (se crea) y P3 no se importa
        let mut db = Db::sample(day());
        let one_by_one = Options { duplicates: DupPolicy::Link, decisions: HashMap::from([("P4".to_string(), DupPolicy::New), ("P3".to_string(), DupPolicy::Skip)]), ..opt() };
        let rep = db.migrate(&source(&db), &one_by_one);
        assert_eq!(db.patients.len(), before + 3);
        assert!(has(&rep, Entity::Pacientes, 5, ProblemKind::Warning, "se crea aparte"));
        assert!(has(&rep, Entity::Pacientes, 4, ProblemKind::Excluded, "no se importa sin decidirlo"));
        let p4 = db.patients.iter().find(|p| p.first_name == "Otra").unwrap();
        assert_ne!(p4.id, 101);
        assert_eq!(p4.birth, None, "el 31 de febrero no existe");
    }

    /// Con el ajuste de saldo, la deuda aquí es la del programa anterior, con
    /// un apunte que lo dice (no un cobro, ni un acto: no es producción ni se
    /// factura); y no se ajusta dos veces.
    #[test]
    fn the_balance_adjustment_matches_the_old_debt_once() {
        let mut db = Db::sample(day());
        let adj = Options { balance: BalancePolicy::Adjust, ..opt() };
        let pays = db.payments.len();
        let produced = db.report_production(&crate::informes::Filter::period(day(), day())).patient;
        let rep = db.migrate(&source(&db), &adj);
        let p2 = rep.patients.iter().find(|p| p.key == "P2").unwrap();
        assert_eq!((p2.debt_source, p2.debt_dest), (Some(Cents(3000)), Cents(3000)));
        assert_eq!((rep.totals.adjusted, rep.totals.adjustments), (1, Cents(1500)));
        assert_eq!(rep.totals.debt_dest + rep.totals.debt_excluded, rep.totals.debt_source, "cuadra, y lo que falta está explicado");
        assert_eq!(db.payments.len(), pays + 3, "solo los tres cobros importados");
        let e = db.entries.iter().find(|e| e.code == ADJUST_CODE).unwrap().clone();
        assert_eq!((e.price, e.date), (15.0, day()));
        assert!(!e.is_act() && db.uninvoiced(e.patient_id).is_empty());
        assert_eq!(db.report_production(&crate::informes::Filter::period(day(), day())).patient, produced, "el ajuste no es producción");
        let rep2 = db.migrate(&source(&db), &adj);
        assert_eq!(db.entries.iter().filter(|e| e.code == ADJUST_CODE).count(), 1);
        assert_eq!(rep2.totals.adjusted, 0);
    }

    /// Un paciente unido a uno que ya estaba no se ajusta solo: su saldo de
    /// aquí no es el del otro programa.
    #[test]
    fn a_linked_patient_is_never_adjusted() {
        let mut db = Db::sample(day());
        let mut src = source(&db);
        src.tables[5].sheet.rows.push(vec!["P3".into(), "999".into()]);
        let opt = Options { balance: BalancePolicy::Adjust, duplicates: DupPolicy::Link, ..opt() };
        let rep = db.migrate(&src, &opt);
        assert!(!db.entries.iter().any(|e| e.code == ADJUST_CODE && e.patient_id == 101));
        assert!(has(&rep, Entity::Saldos, 0, ProblemKind::Warning, "paciente P3: unido a uno que ya estaba"));
    }

    /// Tras unir dos pacientes, lo importado sigue al que se conserva, que
    /// cuenta como uno que ya estaba (sin ajuste de saldo solo).
    #[test]
    fn merging_patients_keeps_their_origin() {
        let mut db = Db::sample(day());
        db.migrate(&source(&db), &opt());
        let lucia = db.patients.iter().find(|p| p.nif == "11111111H").unwrap().id;
        db.merge_patients(101, lucia).unwrap();
        let again = db.migrate(&source(&db), &opt());
        assert_eq!(again.added.get(&Entity::Pacientes), None, "P1 es ahora el 101: no se vuelve a crear");
        assert!(again.patients.iter().any(|p| p.key == "P1" && p.local == 101 && p.linked));
        assert!(has(&again, Entity::Pacientes, 4, ProblemKind::Excluded, "parece el nº de historia"), "P3 sigue pareciéndose al 101");
        assert!(again.added.is_empty());
        assert_eq!(parse_time("30.12.1899 09:45"), NaiveTime::from_hms_opt(9, 45, 0));
    }

    /// El simulacro no cambia los datos y da el mismo informe que la importación.
    #[test]
    fn a_dry_run_changes_nothing_and_predicts_the_result() {
        let db = Db::sample(day());
        let before = serde_json::to_string(&db).unwrap();
        let sim = dry_run(&db, &source(&db), &opt());
        assert_eq!(serde_json::to_string(&db).unwrap(), before);
        let mut real = db.clone();
        let rep = real.migrate(&source(&db), &opt());
        assert_eq!(sim, rep);
    }

    /// Las columnas de los pacientes de GESDEN (las de la consulta conocida,
    /// `mssql::GESDEN_PACIENTES`, más `IdPac`) se asignan solas.
    #[test]
    fn gesden_patient_columns_are_assigned_by_themselves() {
        let sql = crate::mssql::GESDEN_PACIENTES.replacen("SELECT ", "SELECT P.IdPac AS [Id paciente], ", 1);
        let headers: Vec<String> = sql.split("AS [").skip(1).filter_map(|x| x.split(']').next()).map(String::from).collect();
        assert_eq!(headers.len(), 16, "{headers:?}");
        let t = Table::new(Entity::Pacientes, Sheet { headers: headers.clone(), rows: vec![] });
        let col = |k: &str| t.column(k).map(|c| headers[c].as_str());
        assert_eq!(col("id"), Some("Id paciente"), "el identificador que no se repite, no el nº de historia");
        assert_eq!(col("historia"), Some("Nº historial"));
        for (k, h) in [
            ("nombre", "Nombre"),
            ("apellidos", "Apellidos"),
            ("nif", "NIF"),
            ("nacimiento", "Fecha nacimiento"),
            ("sexo", "Sexo"),
            ("movil", "Móvil"),
            ("telefono", "Teléfono"),
            ("email", "Email"),
            ("direccion", "Dirección"),
            ("cp", "C.P."),
            ("poblacion", "Población"),
            ("provincia", "Provincia"),
            ("mutua", "Mutua / tarifa"),
            ("notas", "Notas"),
        ] {
            assert_eq!(col(k), Some(h), "{k}");
        }
        assert!(t.missing().is_empty());
    }

    /// Un perfil guarda las columnas por su título y se aplica a otra
    /// exportación con las columnas en otro orden.
    #[test]
    fn a_saved_profile_assigns_the_columns_again() {
        let mut t = Table::new(Entity::Cobros, sheet(&["Num", "Cliente", "Dia", "Euros", "Medio"], &[]));
        assert_eq!(t.missing().len(), 4, "títulos que no se adivinan");
        for (k, c) in [("id", 0), ("paciente", 1), ("fecha", 2), ("importe", 3), ("forma", 4)] {
            let i = Entity::Cobros.fields().iter().position(|f| f.key == k).unwrap();
            t.map[i] = Some(c);
        }
        let p = Profile::from_tables("Programa X", [&t]);
        let back: Profile = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        let mut other = Table::new(Entity::Cobros, sheet(&["Medio", "Euros", "Dia", "Cliente", "Num", "Otra"], &[]));
        assert_eq!(other.apply(&back), 5);
        assert!(other.missing().is_empty());
        assert_eq!(other.column("importe"), Some(1));
        assert_eq!(Table::new(Entity::Citas, sheet(&["Num"], &[])).apply(&back), 0, "otra clase de dato: nada");
    }

    /// DATOS SINTÉTICOS de extremo a extremo: ficheros CSV inventados en una
    /// carpeta temporal, leídos con el lector de siempre; el simulacro no toca
    /// los datos ni los ficheros, la importación hace lo mismo que el
    /// simulacro, cada registro guarda programa, fichero e identificador, y
    /// el informe cuadra en todo (deuda incluida, sin contarla dos veces).
    #[test]
    fn an_end_to_end_synthetic_migration_squares() {
        let dir = std::env::temp_dir().join(format!("veodardental_migracion_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let files = [
            (
                Entity::Pacientes,
                "pacientes.csv",
                "Id;Nombre;Apellidos;DNI;Fecha nacimiento;Sexo;Movil;Responsable;Parentesco\nS1;Ana;Sintética Uno;00000001R;01/02/1985;M;600111001;;\nS2;Bruno;Sintético Dos;;10/10/2016;H;600111002;Ana Sintética Uno;Madre\nS3;Carla;Sintética Tres;00000003A;05/06/1970;M;;;\n",
            ),
            (
                Entity::Tratamientos,
                "tratamientos.csv",
                "Id;IdPaciente;Fecha;Tratamiento;Piezas;Importe;Estado\nT1;S1;10/03/2026;Obturación;26;55,00;Realizado\nT2;S1;01/12/2026;Corona;;400,00;Pendiente\nT3;S2;15/04/2026;Sellado;16 26 36 46;80,00;Realizado\nT4;S3;20/05/2026;Limpieza;;45,00;Realizado\n",
            ),
            (Entity::Presupuestos, "presupuestos.csv", "Id;IdPaciente;Fecha;Tratamiento;Importe;Estado\nB1;S1;01/11/2026;Corona;400;Aceptado\nB2;S3;01/09/2026;Implante;1200;Pendiente\nB2;S3;01/09/2026;Corona sobre implante;600;Pendiente\n"),
            (Entity::Citas, "citas.csv", "Id;IdPaciente;Fecha;Hora;Duracion;Motivo;Estado\nC1;S1;01/12/2026;10:00;60;Corona;Confirmada\nC2;S2;15/04/2026;17:00;30;Sellado;\nC3;S3;18/11/2026;09:00;30;Revisión;\n"),
            (Entity::Cobros, "cobros.csv", "Id;IdPaciente;Fecha;Importe;Forma de pago\nR1;S1;10/03/2026;55,00;Tarjeta\nR2;S2;15/04/2026;50,00;Efectivo\nR3;S3;20/05/2026;45,00;Bizum\n"),
            (Entity::Saldos, "saldos.csv", "IdPaciente;Saldo\nS2;30,00\n"),
        ];
        let mut tables = vec![];
        let mut stamps = vec![];
        for (e, name, text) in files {
            let path = dir.join(name);
            std::fs::write(&path, text).unwrap();
            stamps.push((path.clone(), std::fs::read(&path).unwrap(), std::fs::metadata(&path).unwrap().modified().unwrap()));
            let mut t = Table::new(e, crate::extra::read_table(&path.to_string_lossy()).unwrap());
            t.origin = name.to_string();
            assert!(t.missing().is_empty(), "{name}: {:?}", t.missing());
            tables.push(t);
        }
        let src = Source { name: "SINTETICO".into(), tables };
        let mut db = Db::sample(day());
        let snapshot = serde_json::to_string(&db).unwrap();
        let sim = dry_run(&db, &src, &opt());
        assert_eq!(serde_json::to_string(&db).unwrap(), snapshot, "el simulacro no cambia nada");
        assert!(sim.problems.is_empty(), "{:?}", sim.problems);
        let rep = db.migrate(&src, &opt());
        assert_eq!(rep, sim, "la importación hace lo del simulacro");
        for (path, bytes, when) in stamps {
            assert_eq!(std::fs::read(&path).unwrap(), bytes, "el origen solo se lee");
            assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), when);
        }
        // todo cuadra, paciente a paciente y en total
        assert_eq!(rep.patients.len(), 3);
        assert!(rep.patients.iter().all(|p| !p.differs()), "{:?}", rep.patients);
        let t = &rep.totals;
        assert_eq!((t.patients_source, t.patients_dest), (3, 3));
        assert_eq!((t.debt_source, t.debt_dest, t.debt_excluded), (Cents(3000), Cents(3000), Cents(0)), "80 − 50 de S2; S1 y S3, a cero");
        assert_eq!((t.future_appts_source, t.future_appts_dest, t.open_budgets_source, t.open_budgets_dest, t.pending_source, t.pending_dest), (2, 2, 2, 2, 1, 1));
        assert_eq!(rep.counts(Entity::Presupuestos), (3, 3, 0, 0), "3 líneas en 2 presupuestos");
        // cada registro, con su programa, su fichero y su identificador
        let r = db.imports.iter().find(|r| r.entity == Entity::Cobros && r.key == "R3").unwrap();
        assert_eq!((r.source.as_str(), r.table.as_str()), ("SINTETICO", "cobros.csv"));
        assert_eq!(db.payments.iter().find(|p| p.id == r.local).map(|p| (p.amount, p.method.as_str())), Some((45.0, "Bizum")));
        let s2 = db.imports.iter().find(|r| r.entity == Entity::Pacientes && r.key == "S2").unwrap().local;
        assert_eq!(db.patient(s2).map(|p| (p.payer.name.as_str(), p.sex.as_str())), Some(("Ana Sintética Uno", "M")));
        // y otra vez: nada nuevo, y el mismo informe
        let again = db.migrate(&src, &opt());
        assert!(again.added.is_empty());
        assert_eq!(again.totals, rep.totals);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El informe en texto lleva los totales, lo que no cuadra y cada problema.
    #[test]
    fn the_report_reads_as_text() {
        let db = Db::sample(day());
        let rep = dry_run(&db, &source(&db), &opt());
        let t = rep.text("PRUEBA", true);
        assert!(t.starts_with("Simulacro de la migración (no se cambió nada) de PRUEBA"));
        assert!(t.contains("Pacientes y responsables\t5\t2\t0\t3"));
        assert!(t.contains("Deuda de los pacientes\t30,00"));
        assert!(t.contains("Deuda de pacientes sin importar\t12,00"));
        assert!(t.contains("2 registros nuevos") || t.contains("registros nuevos"));
        assert!(t.contains("P2 Mario Pérez Ruiz") && t.contains("deuda 30,00"));
        assert!(t.contains("Fila 5: P4 Otra Persona parece el nº de historia"));
        assert!(t.contains("Tratamientos hechos y pendientes · fila 7 · Sin importar: T6: el paciente P9 no está en la hoja de pacientes"));
        // lo que viene de los ficheros no puede tomarse por una fórmula al pegarlo
        let mut db2 = Db::sample(day());
        let mut src = source(&db2);
        src.tables[0].sheet.rows[1][1] = "=HIPERVINCULO(\"http://x\")".into();
        let t2 = db2.migrate(&src, &opt()).text("PRUEBA", false);
        assert!(t2.contains("'=Hipervinculo") || t2.contains("'=HIPERVINCULO"), "{t2}");
        assert!(!t2.lines().any(|l| l.split('\t').any(|c| c.trim_start().starts_with('='))));
    }

    /// Las rutas de otro ordenador se reconocen con cualquier forma, y solo
    /// valen dentro de la carpeta de documentos indicada.
    #[test]
    fn network_paths_are_recognised_and_only_trusted_inside_the_documents_folder() {
        for p in [r"\\srv\rx\a.jpg", "//srv/rx/a.jpg", r"\/srv\rx", r"/\srv\rx", r"\\?\UNC\srv\rx\a", r"\\.\pipe\x", r"\??\UNC\srv\rx", " \\\\srv\\x"] {
            assert!(is_remote(p), "{p}");
        }
        for p in [r"C:\Clinica\a.jpg", "/home/clinica/a.jpg", "rx/a.jpg", r"\Clinica\a.jpg"] {
            assert!(!is_remote(p), "{p}");
        }
        assert!(under(r"\\SRV\Rx\P1\a.jpg", r"\\srv\rx"));
        assert!(under("//srv/rx/P1/a.jpg", r"\\srv\rx\"));
        assert!(!under(r"\\srv\rx\..\..\otro\a.jpg", r"\\srv\rx"), "sin salir con «..»");
        assert!(!under(r"\\srv\rxotra\a.jpg", r"\\srv\rx"), "otra carpeta que empieza igual");
        assert!(!under(r"\\?\UNC\srv\rx\a.jpg", r"\\srv\rx"));
        assert!(!under(r"\\srv\rx\a.jpg", ""));
        // con la carpeta de la clínica indicada, sus rutas (y las relativas) entran
        let mut db = Db::sample(day());
        let mut src = source(&db);
        src.tables.retain(|t| matches!(t.entity, Entity::Pacientes | Entity::Documentos));
        src.tables[1].sheet.rows = vec![
            vec!["E1".into(), "P1".into(), r"\\SERVIDOR-CLINICA\Rx\P1\a.jpg".into(), "".into(), "".into()],
            vec!["E2".into(), "P1".into(), r"P1\b.jpg".into(), "".into(), "".into()],
            vec!["E3".into(), "P1".into(), r"\\otro\Rx\c.jpg".into(), "".into(), "".into()],
            vec!["E4".into(), "P1".into(), r"..\..\d.jpg".into(), "".into(), "".into()],
        ];
        let rep = db.migrate(&src, &Options { doc_root: r"\\servidor-clinica\rx".into(), ..opt() });
        assert_eq!(rep.counts(Entity::Documentos), (4, 2, 0, 2), "{:?}", rep.problems);
        assert!(db.images.iter().any(|i| i.path.ends_with("b.jpg") && i.path.to_lowercase().starts_with(r"\\servidor-clinica\rx")));
    }

    #[test]
    fn sexes_dates_and_methods_are_read_carefully() {
        let t = Table::new(Entity::Pacientes, sheet(&["Id", "Nombre", "Sexo"], &[&["1", "A", "M"], &["2", "B", "F"]]));
        assert_eq!(m_means(&t), Some("M"), "con «F» en la columna, «M» es male");
        let t = Table::new(Entity::Pacientes, sheet(&["Id", "Nombre", "Sexo"], &[&["1", "A", "M"]]));
        assert_eq!(sex_of("M", m_means(&t)), None, "sola, no se sabe");
        assert_eq!(parse_day("03/02/27"), NaiveDate::from_ymd_opt(2027, 2, 3));
        assert_eq!(parse_day("2027-02-03 10:00"), NaiveDate::from_ymd_opt(2027, 2, 3));
        assert_eq!(parse_time("1899-12-30 09:45:00"), NaiveTime::from_hms_opt(9, 45, 0));
        assert_eq!(teeth("11, 36-48 85 19 99"), vec![11, 36, 48, 85]);
        assert_eq!((pay_method("TPV"), pay_method("Recibo domiciliado"), pay_method("Cheque")), (Some("Tarjeta"), Some("Domiciliación"), None));
    }
}
