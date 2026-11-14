//! Asistente de migración desde otro programa (ver `crate::migracion`):
//! hojas por clase de dato con sus columnas, simulacro con el informe de
//! conciliación, decisión de los posibles duplicados uno a uno, e
//! importación con copia de seguridad previa.

use crate::app::App;
use crate::migracion::{BalancePolicy, DupPolicy, Entity, Options, PatientRecon, ProblemKind, Profile, Report, Source, Table as Sheet, dry_run};
use crate::theme::*;
use egui::{RichText, Ui};
use std::collections::HashMap;

pub struct MigState {
    /// Nombre del programa anterior (va en los identificadores importados).
    pub source: String,
    pub paths: Vec<String>,
    /// Una hoja por clase de dato, en el orden de `Entity::ALL`.
    pub tables: Vec<Option<Sheet>>,
    /// Hoja cuyas columnas se están viendo.
    pub open: Option<usize>,
    pub duplicates: DupPolicy,
    pub decisions: HashMap<String, DupPolicy>,
    pub balance: BalancePolicy,
    pub doctor: u32,
    pub doc_root: String,
    /// Último informe: (simulacro, informe). Con `fresh`, es de lo que hay ahora.
    pub report: Option<(bool, Report)>,
    pub fresh: bool,
    pub status: String,
    pub show: Option<ProblemKind>,
    /// Nombre para guardar las columnas como perfil.
    pub profile: String,
    /// Carpeta con una hoja por clase de dato.
    pub folder: String,
}

impl Default for MigState {
    fn default() -> Self {
        MigState {
            source: String::new(),
            paths: vec![String::new(); Entity::ALL.len()],
            tables: vec![None; Entity::ALL.len()],
            open: None,
            duplicates: DupPolicy::Skip,
            decisions: HashMap::new(),
            balance: BalancePolicy::Report,
            doctor: 1,
            doc_root: String::new(),
            report: None,
            fresh: false,
            status: String::new(),
            show: None,
            profile: String::new(),
            folder: String::new(),
        }
    }
}

impl MigState {
    pub fn src(&self) -> Source {
        Source { name: self.source.trim().to_string(), tables: self.tables.iter().flatten().cloned().collect() }
    }

    pub fn options(&self, today: chrono::NaiveDate) -> Options {
        Options { duplicates: self.duplicates, decisions: self.decisions.clone(), balance: self.balance, doc_root: self.doc_root.clone(), ..Options::new(today, self.doctor) }
    }

    /// Pone la hoja leída de una clase de dato (también desde SQL Server).
    pub fn set_sheet(&mut self, i: usize, what: &str, sheet: crate::extra::Sheet) {
        let mut t = Sheet::new(Entity::ALL[i], sheet);
        t.origin = what.to_string();
        self.status = format!("{}: {} filas leídas de {what}.", Entity::ALL[i].label(), t.sheet.rows.len());
        self.tables[i] = Some(t);
        self.open = Some(i);
        self.fresh = false;
    }

    /// Lee de una carpeta las hojas que se reconocen por el nombre del
    /// fichero («pacientes.csv», «Citas 2026.xlsx»…). Devuelve las leídas.
    pub fn load_folder(&mut self, dir: &str) -> Result<usize, String> {
        let dir = dir.trim().trim_matches('"');
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir).map_err(|e| format!("No se puede leer la carpeta: {e}"))?.flatten().map(|e| e.path()).collect();
        files.sort();
        let words: [&[&str]; 7] = [&["pacient"], &["tratam"], &["presup"], &["cita"], &["cobro", "pago"], &["saldo", "deuda"], &["document", "imagen"]];
        let mut n = 0;
        let mut errors = vec![];
        for (i, ws) in words.iter().enumerate() {
            let hit = files.iter().find(|p| {
                let name = crate::model::fold(&p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default());
                ["csv", "xlsx", "xls", "xlsm", "ods", "txt"].iter().any(|x| name.ends_with(&format!(".{x}"))) && ws.iter().any(|w| name.contains(w))
            });
            let Some(path) = hit else { continue };
            let p = path.to_string_lossy().to_string();
            match crate::extra::read_table(&p) {
                Ok(sheet) => {
                    self.paths[i] = p.clone();
                    let what = path.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or(p);
                    self.set_sheet(i, &what, sheet);
                    n += 1;
                }
                Err(e) => errors.push(format!("{}: {e}", Entity::ALL[i].label())),
            }
        }
        if self.source.trim().is_empty()
            && let Some(name) = std::path::Path::new(dir).file_name()
        {
            self.source = name.to_string_lossy().to_string();
        }
        self.open = None;
        self.status = if n == 0 { "En esa carpeta no hay hojas con nombres como «pacientes», «citas» o «cobros».".into() } else { format!("{n} hojas leídas de la carpeta.") };
        if !errors.is_empty() {
            self.status = format!("{} {}", self.status, errors.join(" "));
        }
        Ok(n)
    }

    /// Lo que falta para poder importar.
    pub fn blockers(&self) -> Vec<String> {
        let mut v = vec![];
        if self.source.trim().is_empty() {
            v.push("Escriba el nombre del programa anterior.".to_string());
        }
        if self.tables[0].is_none() {
            v.push("Falta la hoja de pacientes.".into());
        }
        for t in self.tables.iter().flatten() {
            let m = t.missing();
            if !m.is_empty() {
                v.push(format!("{}: falta la columna de {}.", t.entity.label(), m.join(", ")));
            }
        }
        v
    }
}

pub fn panel(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Migración completa desde otro programa");
        muted(
            ui,
            "Trae los pacientes con sus responsables, tratamientos hechos y pendientes, presupuestos, citas, cobros, saldos y documentos, con el identificador que tenían allí. Primero, el simulacro: no cambia nada y da el informe de conciliación. Después, la importación, con copia de seguridad previa. Se puede repetir (lo ya importado no se duplica) y el programa anterior solo se lee. Hágala con los demás puestos cerrados.",
        );
        ui.add_space(6.0);
        let mut dirty = false;
        ui.horizontal_wrapped(|ui| {
            dirty |= field(ui, "Programa anterior", &mut app.imp.mig.source, 220.0).changed();
            dirty |= field(ui, "Carpeta de los documentos (si la ruta es relativa)", &mut app.imp.mig.doc_root, 340.0).changed();
        });
        ui.horizontal_wrapped(|ui| {
            let w = (fit_width(ui) - 200.0).clamp(160.0, 520.0);
            ui.add_sized([w, 32.0], egui::TextEdit::singleline(&mut app.imp.mig.folder).hint_text("Carpeta con todas las hojas (pacientes, tratamientos, citas, cobros…)"));
            if ui.button("📁 Leer la carpeta").clicked() {
                let dir = app.imp.mig.folder.clone();
                if let Err(e) = app.imp.mig.load_folder(&dir) {
                    app.imp.mig.status = e;
                }
                dirty = true;
            }
        });
        ui.add_space(4.0);
        let busy = app.imp.sql_busy.is_some();
        let sql = app.imp.sql_detected.is_some();
        // lo que dejan a la ruta las demás columnas (nombre, botones y estado)
        let path_w = (fit_width(ui) - if sql { 520.0 } else { 430.0 }).clamp(150.0, 520.0);
        egui::Grid::new("mig_sheets").num_columns(if sql { 7 } else { 6 }).spacing([8.0, 6.0]).show(ui, |ui| {
            for (i, e) in Entity::ALL.into_iter().enumerate() {
                sheet_row(app, ui, &ctx, (i, e, path_w), busy, sql, &mut dirty);
                ui.end_row();
            }
        });
        if sql {
            let gesden = app.imp.sql_detected.as_ref().is_some_and(|d| d.program.is_some());
            ui.horizontal_wrapped(|ui| {
                muted(ui, "«Desde SQL» lee la tabla o la consulta elegidas arriba, en la tarjeta de SQL Server.");
                if gesden && ui.add_enabled(!busy, egui::Button::new("👥 Pacientes de GESDEN / Novaclinic")).on_hover_text("Con su identificador interno, nº de historia, datos de contacto y mutua").clicked() {
                    super::importar::run_mig_gesden(app, &ctx);
                    dirty = true;
                }
            });
        }
        dirty |= profiles(app, ui);
        if let Some(i) = app.imp.mig.open
            && app.imp.mig.tables[i].is_some()
        {
            dirty |= columns(app, ui, i);
        }
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            let m = &mut app.imp.mig;
            ui.label(RichText::new("Posibles duplicados:").color(MUTED()));
            crate::theme::combo("mig_dup").width(210.0).selected_text(m.duplicates.label()).show_ui(ui, |ui| {
                for d in [DupPolicy::Skip, DupPolicy::Link, DupPolicy::New] {
                    dirty |= ui.selectable_value(&mut m.duplicates, d, d.label()).changed();
                }
            });
            ui.label(RichText::new("Saldos:").color(MUTED()));
            let bl = |b: BalancePolicy| if b == BalancePolicy::Report { "Solo informar de la diferencia" } else { "Ajustar al del programa anterior" };
            crate::theme::combo("mig_bal").width(250.0).selected_text(bl(m.balance)).show_ui(ui, |ui| {
                for b in [BalancePolicy::Report, BalancePolicy::Adjust] {
                    dirty |= ui.selectable_value(&mut m.balance, b, bl(b)).changed();
                }
            });
            ui.label(RichText::new("Doctor si no dice:").color(MUTED()));
            let cur = app.db.doctor(m.doctor).map(|d| d.name.clone()).unwrap_or_else(|| "—".into());
            crate::theme::combo("mig_doc").width(200.0).selected_text(cur).show_ui(ui, |ui| {
                for d in app.db.collaborators.iter().filter(|d| d.active) {
                    dirty |= ui.selectable_value(&mut m.doctor, d.id, &d.name).changed();
                }
            });
        });
        if dirty {
            app.imp.mig.fresh = false;
        }
        ui.add_space(6.0);
        let blockers = app.imp.mig.blockers();
        let (simulate, import) = ui
            .horizontal_wrapped(|ui| {
                let s = ui.add_enabled(blockers.is_empty(), egui::Button::new(RichText::new("🔍 Simulacro (no cambia nada)").color(egui::Color32::WHITE)).fill(ACCENT)).clicked();
                let ready = blockers.is_empty() && app.imp.mig.fresh && app.imp.mig.report.as_ref().is_some_and(|r| r.0);
                let i = ui.add_enabled(ready, egui::Button::new("⬇ Importar lo del simulacro")).on_disabled_hover_text("Haga antes el simulacro con lo que hay ahora.").clicked();
                (s, i)
            })
            .inner;
        for b in &blockers {
            ui.label(RichText::new(b).color(MUTED()));
        }
        if simulate {
            let rep = dry_run(&app.db, &app.imp.mig.src(), &app.imp.mig.options(app.today));
            app.imp.mig.status = format!("Simulacro: {}", rep.summary());
            app.imp.mig.report = Some((true, rep));
            app.imp.mig.fresh = true;
        }
        if import {
            run_import(app);
        }
        if !app.imp.mig.status.is_empty() {
            ui.label(RichText::new(&app.imp.mig.status).color(INFO));
        }
        report(app, ui);
    });
}

fn sheet_row(app: &mut App, ui: &mut Ui, ctx: &egui::Context, (i, e, w): (usize, Entity, f32), busy: bool, sql: bool, dirty: &mut bool) {
    {
        ui.label(RichText::new(e.short()).family(sb())).on_hover_text(e.label());
        ui.add_sized([w, 32.0], egui::TextEdit::singleline(&mut app.imp.mig.paths[i]).hint_text("Fichero xlsx, ods o csv"));
        if ui.button("Leer").clicked() {
            let p = app.imp.mig.paths[i].trim().trim_matches('"').to_string();
            match crate::extra::read_table(&p) {
                Ok(s) => app.imp.mig.set_sheet(i, &p, s),
                Err(err) => app.imp.mig.status = format!("{}: {err}", e.label()),
            }
            *dirty = true;
        }
        if sql && ui.add_enabled(!busy && (!app.imp.sql_table.is_empty() || !app.imp.sql_query.trim().is_empty()), egui::Button::new("Desde SQL")).clicked() {
            let (q, t) = (app.imp.sql_query.trim().to_string(), app.imp.sql_table.clone());
            super::importar::run_mig_job(app, ctx, i, q, t);
            *dirty = true;
        }
        let m = &mut app.imp.mig;
        match &m.tables[i] {
            Some(t) => {
                let missing = t.missing();
                if missing.is_empty() {
                    let n = t.sheet.rows.len();
                    pill(ui, &if n == 1 { "1 fila".to_string() } else { format!("{n} filas") }, OK);
                } else {
                    pill(ui, "faltan columnas", DANGER).on_hover_text(missing.join(", "));
                }
                if ui.selectable_label(m.open == Some(i), "Columnas").clicked() {
                    m.open = if m.open == Some(i) { None } else { Some(i) };
                }
                if ui.small_button("✖").on_hover_text("Quitar esta hoja").clicked() {
                    m.tables[i] = None;
                    *dirty = true;
                }
            }
            None => {
                ui.label(RichText::new(if i == 0 { "obligatoria" } else { "opcional" }).color(SUBTLE()));
            }
        }
    }
}

/// Columnas de una hoja, con el dato de la primera fila al lado.
fn columns(app: &mut App, ui: &mut Ui, i: usize) -> bool {
    let mut changed = false;
    let Some(t) = app.imp.mig.tables[i].as_mut() else { return false };
    ui.add_space(4.0);
    ui.label(RichText::new(format!("Columnas de «{}»", t.entity.label())).family(sb()));
    let opts: Vec<(Option<usize>, String)> = std::iter::once((None, "— no hay —".to_string())).chain(t.sheet.headers.iter().enumerate().filter(|(_, h)| !h.trim().is_empty()).map(|(c, h)| (Some(c), h.clone()))).collect();
    let first = t.sheet.rows.first().cloned().unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        for (k, fd) in t.entity.fields().iter().enumerate() {
            ui.vertical(|ui| {
                ui.set_width(250.0);
                ui.label(RichText::new(format!("{}{}", fd.label, if fd.required { " *" } else { "" })).color(if fd.required { HEADING } else { MUTED() }));
                let cur = opts.iter().find(|o| o.0 == t.map[k]).map(|o| o.1.clone()).unwrap_or_default();
                crate::theme::combo(("mig_col", i, k)).width(240.0).selected_text(cur).show_ui(ui, |ui| {
                    for (v, n) in &opts {
                        changed |= ui.selectable_value(&mut t.map[k], *v, n).changed();
                    }
                });
                let sample = t.map[k].and_then(|c| first.get(c)).cloned().unwrap_or_default();
                ui.label(RichText::new(if sample.is_empty() { " ".to_string() } else { format!("p. ej. {sample}") }).color(SUBTLE()).size(12.0));
            });
        }
    });
    // vista previa: las primeras filas como se van a leer
    let cols: Vec<(&str, usize)> = t.entity.fields().iter().enumerate().filter_map(|(k, fd)| t.map[k].map(|c| (fd.label, c))).collect();
    if !cols.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(format!("Vista previa ({} de {} filas)", t.sheet.rows.len().min(3), t.sheet.rows.len())).color(MUTED()));
        egui::ScrollArea::horizontal().id_salt(("mig_prev", i)).show(ui, |ui| {
            egui::Grid::new(("mig_prev_g", i)).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                for (name, _) in &cols {
                    ui.label(RichText::new(*name).family(sb()).size(12.5));
                }
                ui.end_row();
                for row in t.sheet.rows.iter().take(3) {
                    for (_, c) in &cols {
                        let v: String = row.get(*c).map(|v| v.chars().take(40).collect()).unwrap_or_default();
                        ui.label(v);
                    }
                    ui.end_row();
                }
            });
        });
    }
    changed
}

/// Perfiles de columnas: elegir uno guardado o guardar el de ahora.
fn profiles(app: &mut App, ui: &mut Ui) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Perfil de columnas:").color(MUTED()));
        let names: Vec<String> = app.db.config.mig_profiles.iter().map(|p| p.name.clone()).collect();
        crate::theme::combo("mig_prof").width(220.0).selected_text(if names.is_empty() { "ninguno guardado".to_string() } else { "Aplicar uno…".to_string() }).show_ui(ui, |ui| {
            for n in &names {
                if ui.selectable_label(false, n).clicked()
                    && let Some(p) = app.db.config.mig_profiles.iter().find(|p| &p.name == n).cloned()
                {
                    let k: usize = app.imp.mig.tables.iter_mut().flatten().map(|t| t.apply(&p)).sum();
                    app.imp.mig.status = format!("Perfil «{n}»: {k} columnas asignadas.");
                    changed = true;
                }
            }
        });
        ui.add_sized([200.0, 32.0], egui::TextEdit::singleline(&mut app.imp.mig.profile).hint_text("Nombre del perfil"));
        let can = !app.imp.mig.profile.trim().is_empty() && app.imp.mig.tables.iter().any(|t| t.is_some());
        if ui.add_enabled(can, egui::Button::new("Guardar las columnas como perfil")).clicked() {
            let p = Profile::from_tables(&app.imp.mig.profile, app.imp.mig.tables.iter().flatten());
            let list = &mut app.db.config.mig_profiles;
            list.retain(|x| x.name != p.name);
            app.imp.mig.status = format!("Perfil «{}» guardado.", p.name);
            list.push(p);
            app.changed();
        }
    });
    changed
}

fn run_import(app: &mut App) {
    // una migración no se deshace sola: antes, copia de seguridad
    let backup = if app.persist {
        match crate::tools::create_backup(&app.db, "antes de la migración") {
            Ok(p) => format!(" Copia de seguridad previa: {}.", p.display()),
            Err(e) => {
                app.imp.mig.status = format!("No se ha importado nada: no se pudo hacer la copia de seguridad previa ({e}).");
                return;
            }
        }
    } else {
        String::new()
    };
    let rep = app.db.migrate(&app.imp.mig.src(), &app.imp.mig.options(app.today));
    let summary = format!("Migración de {} terminada: {}", app.imp.mig.source.trim(), rep.summary());
    app.imp.mig.status = format!("{summary}{backup}");
    app.imp.mig.report = Some((false, rep));
    app.imp.mig.fresh = false;
    app.changed();
    app.notify(summary);
}

fn report(app: &mut App, ui: &mut Ui) {
    let Some((simulated, rep)) = app.imp.mig.report.clone() else { return };
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        section(ui, if simulated { "Informe del simulacro" } else { "Informe de la migración" });
        if !app.imp.mig.fresh && simulated {
            pill(ui, "los datos han cambiado: repita el simulacro", WARN);
        }
        if ui.button("📋 Copiar el informe").on_hover_text("Para guardarlo o pegarlo en una hoja de cálculo").clicked() {
            ui.ctx().copy_text(rep.text(app.imp.mig.source.trim(), simulated));
        }
    });
    let w = (fit_width(ui) / 5.0).clamp(90.0, 230.0);
    let tbl = Table::new("mig_counts", vec![("Clase de dato", w * 1.6), ("Filas", w * 0.6), ("Nuevos", w * 0.6), ("Ya estaban", w * 0.7), ("Sin importar", w * 0.7)]);
    tbl.header(ui);
    for (i, e) in Entity::ALL.into_iter().enumerate() {
        let (r, a, y, l) = rep.counts(e);
        if r == 0 {
            continue;
        }
        tbl.row(ui, i, false, &[cell(e.label()), cell(r.to_string()), cell(a.to_string()), cell(y.to_string()), cellc(l.to_string(), if l > 0 { DANGER } else { TEXT })]);
    }
    ui.add_space(6.0);
    let tbl = Table::new("mig_recon", vec![("Conciliación", w * 1.6), ("Programa anterior", w * 1.5), ("VeodarDental", w * 1.1)]);
    tbl.header(ui);
    for (i, (what, a, b)) in rep.totals_rows().into_iter().enumerate() {
        tbl.row(ui, i, false, &[cell(what), cell(a), cell(b)]);
    }
    if rep.totals.adjusted > 0 {
        muted(ui, format!("{} ajustes de saldo apuntados, por {} en total (un apunte «Saldo del programa anterior», que no es producción ni se factura).", rep.totals.adjusted, rep.totals.adjustments));
    }
    // posibles duplicados, uno a uno
    if !rep.duplicates.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(format!("Posibles duplicados ({}): decida cada uno y repita el simulacro", rep.duplicates.len())).family(sb()));
        egui::ScrollArea::vertical().id_salt("mig_dups").max_height(220.0).show(ui, |ui| {
            for d in &rep.duplicates {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Fila {} · {} {} parece el nº {} ({})", d.row, d.key, d.name, d.history_no, d.why));
                    let m = &mut app.imp.mig;
                    let cur = m.decisions.get(&d.key).copied().unwrap_or(m.duplicates);
                    crate::theme::combo(("mig_d", d.row)).width(210.0).selected_text(cur.label()).show_ui(ui, |ui| {
                        for p in [DupPolicy::Skip, DupPolicy::Link, DupPolicy::New] {
                            if p == DupPolicy::Link && !d.same_person {
                                continue;
                            }
                            if ui.selectable_label(cur == p, p.label()).clicked() {
                                m.decisions.insert(d.key.clone(), p);
                                m.fresh = false;
                            }
                        }
                    });
                    if !d.same_person {
                        ui.label(RichText::new("solo coincide el NIF: no se puede unir").color(SUBTLE()));
                    }
                });
            }
        });
    }
    // pacientes que no cuadran
    let diff: Vec<&PatientRecon> = rep.patients.iter().filter(|p| p.differs()).collect();
    if !diff.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(format!("Pacientes que no cuadran ({})", diff.len())).family(sb()));
        egui::ScrollArea::vertical().id_salt("mig_diff").max_height(200.0).show(ui, |ui| {
            for p in diff {
                ui.label(format!("{} {}: {}", p.key, p.name, Report::differences(p)));
            }
        });
    }
    // filas con problemas
    if !rep.problems.is_empty() {
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Filas con problemas y avisos").family(sb()));
            let m = &mut app.imp.mig;
            for (k, name, color) in [(None, "Todos", MUTED()), (Some(ProblemKind::Error), "Errores", DANGER), (Some(ProblemKind::Excluded), "Sin importar", WARN), (Some(ProblemKind::Warning), "Avisos", INFO)] {
                let n = k.map(|k| rep.count(k)).unwrap_or(rep.problems.len());
                if ui.selectable_label(m.show == k, RichText::new(format!("{name} ({n})")).color(color)).clicked() {
                    m.show = k;
                }
            }
        });
        let show = app.imp.mig.show;
        egui::ScrollArea::vertical().id_salt("mig_probs").max_height(260.0).show(ui, |ui| {
            for p in rep.problems.iter().filter(|p| show.is_none_or(|k| k == p.kind)).take(2000) {
                let row = if p.row == 0 { "toda la hoja".to_string() } else { format!("fila {}", p.row) };
                let color = match p.kind {
                    ProblemKind::Error => DANGER,
                    ProblemKind::Excluded => WARN,
                    ProblemKind::Warning => TEXT,
                };
                ui.label(RichText::new(format!("{} · {row}: {}", p.entity.label(), p.reason)).color(color));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DATOS SINTÉTICOS: una carpeta con hojas inventadas se lee de una vez,
    /// cada una por el nombre de su fichero.
    #[test]
    fn a_folder_loads_each_sheet_by_its_name() {
        let dir = std::env::temp_dir().join(format!("veodardental_carpeta_{}", std::process::id())).join("PROGRAMA_X");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Pacientes.csv"), "Id;Nombre\nS1;Ana\n").unwrap();
        std::fs::write(dir.join("citas 2026.csv"), "Id;IdPaciente;Fecha;Hora\nC1;S1;01/12/2026;10:00\n").unwrap();
        std::fs::write(dir.join("otra cosa.csv"), "a;b\n1;2\n").unwrap();
        let mut m = MigState::default();
        assert_eq!(m.load_folder(&dir.to_string_lossy()), Ok(2));
        assert_eq!(m.source, "PROGRAMA_X");
        assert!(m.tables[0].is_some() && m.tables[3].is_some() && m.tables[1].is_none());
        assert_eq!(m.tables[3].as_ref().map(|t| t.origin.as_str()), Some("citas 2026.csv"));
        assert!(m.blockers().is_empty(), "{:?}", m.blockers());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
