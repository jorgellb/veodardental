//! Importación de pacientes y tratamientos/precios desde Excel o CSV y desde
//! Microsoft SQL Server (GESDEN, Novaclinic u otros programas de gestión).

use crate::app::App;
use crate::extra::*;
use crate::model::*;
use crate::theme::*;
use egui::{RichText, Ui};
use std::sync::mpsc::Receiver;

/// Resultado de una lectura del SQL Server hecha en segundo plano.
pub enum SqlJob {
    Detected(Result<crate::mssql::Detected, String>),
    Catalog(Result<crate::gesden::GCatalogo, String>),
    Sheet(ImportKind, String, Result<Sheet, String>),
}

pub struct ImportState {
    pub path: String,
    pub kind: ImportKind,
    pub tariff: u32,
    pub sheet: Option<Sheet>,
    pub map: Vec<Option<usize>>,
    pub result: String,
    /// Conexión con el SQL Server del programa anterior (la contraseña solo en memoria).
    pub sql: crate::mssql::SqlSource,
    pub sql_status: String,
    pub sql_busy: Option<Receiver<SqlJob>>,
    pub sql_detected: Option<crate::mssql::Detected>,
    pub sql_table: String,
    pub sql_query: String,
    /// Catálogo de GESDEN leído del servidor.
    pub gesden: Option<crate::gesden::GCatalogo>,
    pub gesden_opt: crate::gesden::ImportOptions,
    pub gesden_result: String,
}

impl Default for ImportState {
    fn default() -> Self {
        ImportState {
            path: String::new(),
            kind: ImportKind::Pacientes,
            tariff: 1,
            sheet: None,
            map: vec![],
            result: String::new(),
            sql: Default::default(),
            sql_status: String::new(),
            sql_busy: None,
            sql_detected: None,
            sql_table: String::new(),
            sql_query: String::new(),
            gesden: None,
            gesden_opt: Default::default(),
            gesden_result: String::new(),
        }
    }
}

fn run_job(app: &mut App, ctx: &egui::Context, job: impl FnOnce(crate::mssql::SqlSource) -> SqlJob + Send + 'static) {
    let (tx, rx) = std::sync::mpsc::channel();
    let src = app.imp.sql.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(job(src));
        ctx.request_repaint();
    });
    app.imp.sql_busy = Some(rx);
    app.imp.sql_status = "Leyendo del servidor…".into();
}

fn poll_job(app: &mut App) {
    let Some(rx) = &app.imp.sql_busy else { return };
    let Ok(res) = rx.try_recv() else { return };
    app.imp.sql_busy = None;
    match res {
        SqlJob::Detected(Ok(d)) => {
            app.imp.sql_status = format!("Conectado: {} · base de datos {} · {} tablas{}.", d.version.trim(), d.database, d.tables.len(), d.program.map(|p| format!(" · programa reconocido: {p}")).unwrap_or_default());
            if app.imp.sql_table.is_empty() {
                app.imp.sql_table = d.tables.iter().max_by_key(|t| t.1).map(|t| t.0.clone()).unwrap_or_default();
            }
            app.imp.sql_detected = Some(d);
        }
        SqlJob::Catalog(Ok(c)) => {
            app.imp.sql_status = format!("Catálogo leído: {} tarifas y {} tratamientos.", c.tarifas.len(), c.tarifas.iter().map(|t| t.tratamientos.len()).sum::<usize>());
            app.imp.gesden = Some(c);
        }
        SqlJob::Sheet(kind, what, Ok(s)) => {
            app.imp.kind = kind;
            app.imp.map = guess_mapping(kind, &s.headers);
            app.imp.result = format!("{} filas leídas de {what}. Revise qué columna corresponde a cada dato y pulse «Importar».", s.rows.len());
            app.imp.sql_status = app.imp.result.clone();
            app.imp.sheet = Some(s);
        }
        SqlJob::Detected(Err(e)) | SqlJob::Catalog(Err(e)) | SqlJob::Sheet(_, _, Err(e)) => app.imp.sql_status = e,
    }
}

/// Tarjeta «Importar desde Microsoft SQL Server».
pub fn sql_panel(app: &mut App, ui: &mut Ui) {
    poll_job(app);
    let ctx = ui.ctx().clone();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Importar desde Microsoft SQL Server (GESDEN, Novaclinic u otros programas)");
        muted(ui, "Conecte con el servidor donde el programa anterior guarda sus datos. Solo se leen datos; en el programa anterior no se cambia nada. GESDEN y Novaclinic se reconocen solos; para otros programas elija la tabla y asigne las columnas.");
        ui.add_space(4.0);
        let busy = app.imp.sql_busy.is_some();
        let s = &mut app.imp.sql;
        ui.horizontal_wrapped(|ui| {
            field(ui, "Servidor (p. ej. SERVIDOR\\GESDEN o 192.168.1.10,1433)", &mut s.server, 300.0);
            field(ui, "Base de datos", &mut s.database, 140.0);
            field(ui, if s.windows_auth { "Usuario de Windows (DOMINIO\\usuario)" } else { "Usuario" }, &mut s.user, 170.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Contraseña").size(13.5).color(MUTED));
                ui.add_sized([150.0, 32.0], egui::TextEdit::singleline(&mut s.password).password(true));
            });
        });
        let mut connect = false;
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut s.windows_auth, "Usuario de Windows");
            ui.label(RichText::new("Cifrado").color(MUTED));
            crate::theme::combo("sql_enc").width(250.0).selected_text(["Sin cifrado (servidores antiguos)", "Si el servidor lo admite", "Obligatorio"][s.encrypt.min(2) as usize]).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.encrypt, 1, "Si el servidor lo admite");
                ui.selectable_value(&mut s.encrypt, 2, "Obligatorio");
                ui.selectable_value(&mut s.encrypt, 0, "Sin cifrado (servidores antiguos)");
            });
            connect = ui.add_enabled(!busy, egui::Button::new(RichText::new("🔌 Conectar y analizar").color(egui::Color32::WHITE)).fill(ACCENT)).clicked();
            if busy {
                ui.spinner();
            }
        });
        {
            if connect {
                app.imp.sql_detected = None;
                app.imp.gesden = None;
                run_job(app, &ctx, |src| SqlJob::Detected(crate::mssql::detect(&src)));
            }
        }
        if !app.imp.sql_status.is_empty() {
            let ok = app.imp.sql_detected.is_some() && !app.imp.sql_status.starts_with("No ") && !app.imp.sql_status.contains("rror");
            ui.label(RichText::new(&app.imp.sql_status).color(if ok { INFO } else if busy { MUTED } else { DANGER }));
        }
        let Some(d) = app.imp.sql_detected.clone() else { return };
        ui.add_space(6.0);
        if d.program.is_some() {
            ui.horizontal_wrapped(|ui| {
                pill(ui, &format!("{} pacientes", d.rows("Pacientes")), OK);
                pill(ui, &format!("{} tratamientos en tarifas", d.rows("TTos")), OK);
                pill(ui, &format!("{} citas", d.rows("DCitas")), INFO);
                pill(ui, &format!("{} tratamientos realizados", d.rows("TtosMed")), INFO);
                if ui.add_enabled(!busy, egui::Button::new("📋 Leer catálogo y tarifas")).clicked() {
                    run_job(app, &ctx, |src| SqlJob::Catalog(crate::mssql::gesden_catalog(&src)));
                }
                if ui.add_enabled(!busy, egui::Button::new("👥 Leer pacientes")).clicked() {
                    let cat = app.imp.gesden.clone();
                    run_job(app, &ctx, move |src| SqlJob::Sheet(ImportKind::Pacientes, "los pacientes de GESDEN".into(), crate::mssql::gesden_patients(&src, cat.as_ref())));
                }
            });
            muted(ui, "Importe primero el catálogo y las tarifas y después los pacientes, para que cada paciente quede con su tarifa.");
        }
        // cualquier programa: una tabla o una consulta
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Otra tabla:").color(MUTED));
            crate::theme::combo("sql_table").width(280.0).height(400.0).selected_text(if app.imp.sql_table.is_empty() { "Elegir tabla…".to_string() } else { app.imp.sql_table.clone() }).show_ui(ui, |ui| {
                for (t, n) in d.tables.iter().filter(|t| t.1 > 0) {
                    ui.selectable_value(&mut app.imp.sql_table, t.clone(), format!("{t} ({n} filas)"));
                }
            });
            for (kind, label) in [(ImportKind::Pacientes, "Leer como pacientes"), (ImportKind::Tratamientos, "Leer como tratamientos")] {
                if ui.add_enabled(!busy && !app.imp.sql_table.is_empty(), egui::Button::new(label)).clicked() {
                    let t = app.imp.sql_table.clone();
                    run_job(app, &ctx, move |src| SqlJob::Sheet(kind, format!("la tabla {t}"), crate::mssql::read_table(&src, &t, 200_000)));
                }
            }
        });
        ui.collapsing("Consulta SQL propia (avanzado)", |ui| {
            ui.add(egui::TextEdit::multiline(&mut app.imp.sql_query).desired_rows(3).desired_width(f32::INFINITY).hint_text("SELECT Codigo, Descripcion, Precio FROM Tratamientos"));
            ui.horizontal(|ui| {
                for (kind, label) in [(ImportKind::Pacientes, "Ejecutar para pacientes"), (ImportKind::Tratamientos, "Ejecutar para tratamientos")] {
                    if ui.add_enabled(!busy && !app.imp.sql_query.trim().is_empty(), egui::Button::new(label)).clicked() {
                        let q = app.imp.sql_query.clone();
                        run_job(app, &ctx, move |src| SqlJob::Sheet(kind, "la consulta".into(), crate::mssql::read_query(&src, &q, 200_000)));
                    }
                }
                muted(ui, "Solo consultas SELECT.");
            });
        });
    });
    if app.imp.gesden.is_some() {
        ui.add_space(10.0);
        gesden_panel(app, ui);
    }
}

/// Tarjeta «Catálogo de GESDEN»: resumen de lo leído y botón para importarlo.
pub fn gesden_panel(app: &mut App, ui: &mut Ui) {
    use crate::gesden::*;
    // se saca del estado mientras se dibuja para no copiar miles de tratamientos en cada fotograma
    let Some(res) = app.imp.gesden.take() else { return };
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Catálogo de tratamientos y tarifas de GESDEN");
        let cat = &res;
        muted(ui, format!("Leído de {}. Trae los tratamientos con sus grupos, precios, duración y dibujo en el odontograma, y las tarifas.", cat.origen));
        ui.add_space(6.0);
        let Some(base) = base_tariff(cat) else {
            ui.label(RichText::new("No hay ninguna tarifa activa con tratamientos.").color(DANGER));
            return;
        };
        let others: Vec<&GTarifa> = cat.tarifas.iter().filter(|t| t.id != base.id && t.active()).collect();
        let inactive = cat.tarifas.iter().filter(|t| !t.active()).count();
        ui.horizontal_wrapped(|ui| {
            pill(ui, &format!("Tarifa general: {} · {} tratamientos ({} activos)", pretty_text(base.nombre.trim().trim_matches('-').trim()), base.tratamientos.len(), base.active_treatments()), OK);
            for t in &others {
                pill(ui, &format!("{} · {} tratamientos", pretty_text(&t.nombre), t.tratamientos.len()), INFO);
            }
            pill(ui, &format!("{inactive} mutuas o tarifas dadas de baja"), MUTED);
        });
        ui.add_space(6.0);
        // vista previa con los nombres como quedarán
        let pretty = app.imp.gesden_opt.pretty;
        let sample: Vec<&GTto> = base.tratamientos.iter().filter(|t| t.inactivo == 0).take(6).collect();
        let tbl = Table::new("ges_prev", vec![("Código", 110.0), ("Tratamiento", 380.0), ("Grupo", 190.0), ("Odontograma", 140.0), ("Precio", 90.0)]);
        tbl.header(ui);
        for (i, t) in sample.iter().enumerate() {
            let name = if pretty { pretty_text(&t.desc_medico) } else { t.desc_medico.clone() };
            let group = if pretty { pretty_text(&t.grupo) } else { t.grupo.clone() };
            tbl.row(ui, i, false, &[cell(&t.codigo), cell(name), cell(group), cell(odg_from_gesden(&t.tipo_odg).label()), cell(money(t.precio))]);
        }
        ui.add_space(6.0);
        let o = &mut app.imp.gesden_opt;
        ui.checkbox(&mut o.replace_catalog, "Sustituir el catálogo actual: los tratamientos que no estén en GESDEN dejan de ofrecerse (la historia de los pacientes no cambia)");
        ui.checkbox(&mut o.include_inactive_tariffs, format!("Traer también las {inactive} mutuas y tarifas dadas de baja en GESDEN (quedan inactivas)"));
        ui.checkbox(&mut o.pretty, "Escribir los nombres con minúsculas y tildes (en GESDEN están en mayúsculas)");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if primary(ui, "📥 Importar catálogo de GESDEN").clicked() {
                let backup = crate::tools::create_backup(&app.db, "antes de importar GESDEN");
                match app.db.import_gesden_catalog(cat, app.imp.gesden_opt) {
                    Ok(r) => {
                        let msg = r.summary();
                        app.db.audit("Importación del catálogo de GESDEN", &msg);
                        app.imp.gesden_result = match backup {
                            Ok(p) => format!("{msg} Copia de seguridad previa: {}", p.display()),
                            Err(e) => format!("{msg} (No se pudo hacer la copia previa: {e})"),
                        };
                        app.changed();
                        app.notify("Catálogo de GESDEN importado.");
                    }
                    Err(e) => app.imp.gesden_result = e,
                }
            }
            muted(ui, "Se puede repetir: lo ya importado se actualiza sin duplicarse.");
        });
        if !app.imp.gesden_result.is_empty() {
            ui.label(RichText::new(&app.imp.gesden_result).color(INFO));
        }
    });
    app.imp.gesden = Some(res);
}

pub fn panel(app: &mut App, ui: &mut Ui) {
    sql_panel(app, ui);
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Importar datos desde Excel o CSV");
        muted(ui, "Sirve para traer pacientes y el catálogo de tratamientos con sus precios desde listados del programa anterior (xlsx, xls, ods o csv).");
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let before = app.imp.kind;
            ui.selectable_value(&mut app.imp.kind, ImportKind::Pacientes, "Pacientes");
            ui.selectable_value(&mut app.imp.kind, ImportKind::Tratamientos, "Tratamientos y precios");
            if before != app.imp.kind {
                if let Some(s) = &app.imp.sheet {
                    app.imp.map = guess_mapping(app.imp.kind, &s.headers);
                }
            }
            if app.imp.kind == ImportKind::Tratamientos {
                ui.label("Precios de la tarifa");
                let cur = app.db.tariff_name(app.imp.tariff);
                crate::theme::combo("imp_tar").width(200.0).selected_text(cur).show_ui(ui, |ui| {
                    for t in &app.db.tariffs {
                        ui.selectable_value(&mut app.imp.tariff, t.id, &t.name);
                    }
                });
            }
        });
        ui.horizontal(|ui| {
            ui.add_sized([ui.available_width() - 150.0, 34.0], egui::TextEdit::singleline(&mut app.imp.path).hint_text("Ruta del fichero, p. ej. C:\\Exportaciones\\pacientes.xlsx"));
            if primary(ui, "Leer fichero").clicked() {
                let p = app.imp.path.trim().trim_matches('"').to_string();
                match read_table(&p) {
                    Ok(s) => {
                        app.imp.map = guess_mapping(app.imp.kind, &s.headers);
                        app.imp.result = format!("{} filas leídas. Revise qué columna corresponde a cada dato.", s.rows.len());
                        app.imp.sheet = Some(s);
                    }
                    Err(e) => {
                        app.imp.sheet = None;
                        app.imp.result = e;
                    }
                }
            }
        });
        if !app.imp.result.is_empty() {
            ui.label(RichText::new(&app.imp.result).color(if app.imp.sheet.is_some() { INFO } else { DANGER }));
        }
        let Some(sheet) = app.imp.sheet.clone() else { return };
        ui.add_space(6.0);
        // correspondencia de columnas
        let opts: Vec<(Option<usize>, String)> = std::iter::once((None, "— no importar —".to_string()))
            .chain(sheet.headers.iter().enumerate().filter(|(_, h)| !h.is_empty()).map(|(i, h)| (Some(i), h.clone())))
            .collect();
        egui::Grid::new("imp_map").num_columns(4).spacing([14.0, 6.0]).show(ui, |ui| {
            for (i, (name, _)) in targets(app.imp.kind).iter().enumerate() {
                if app.imp.map.len() <= i {
                    app.imp.map.resize(i + 1, None);
                }
                ui.label(RichText::new(*name).family(sb()));
                let cur = opts.iter().find(|o| o.0 == app.imp.map[i]).map(|o| o.1.clone()).unwrap_or_default();
                crate::theme::combo(("imp_col", i)).width(200.0).selected_text(cur).show_ui(ui, |ui| {
                    for (v, n) in &opts {
                        ui.selectable_value(&mut app.imp.map[i], *v, n);
                    }
                });
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(8.0);
        // vista previa
        section(ui, "Vista previa");
        let cols: Vec<(String, usize)> = targets(app.imp.kind).iter().enumerate().filter_map(|(i, (n, _))| app.imp.map.get(i).copied().flatten().map(|c| (n.to_string(), c))).collect();
        if cols.is_empty() {
            muted(ui, "Elija al menos una columna.");
        } else {
            let w = (ui.available_width() / cols.len() as f32).max(90.0);
            let tbl = Table::new("imp_prev", cols.iter().map(|(n, _)| (n.as_str(), w)).collect());
            tbl.header(ui);
            for (i, row) in sheet.rows.iter().take(5).enumerate() {
                let cells: Vec<(String, egui::Color32)> = cols.iter().map(|(_, c)| cell(row.get(*c).cloned().unwrap_or_default())).collect();
                tbl.row(ui, i, false, &cells);
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if primary(ui, &format!("Importar {} filas", sheet.rows.len())).clicked() {
                let r = match app.imp.kind {
                    ImportKind::Pacientes => app.db.import_patients(&sheet, &app.imp.map),
                    ImportKind::Tratamientos => app.db.import_treatments(&sheet, &app.imp.map, app.imp.tariff),
                };
                app.imp.result = format!("Importación terminada: {} nuevos, {} actualizados, {} filas omitidas.", r.added, r.updated, r.skipped);
                app.imp.sheet = None;
                let msg = app.imp.result.clone();
                app.db.audit("Importación", &msg);
                app.changed();
                app.notify(app.imp.result.clone());
            }
            muted(ui, "Los pacientes que ya existen (mismo NIF o Nº de historial) no se duplican: solo se completan sus datos vacíos.");
        });
    });
}
