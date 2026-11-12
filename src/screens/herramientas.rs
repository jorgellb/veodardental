//! Copias de seguridad, pizarra de sala de espera, listín telefónico y
//! consultas de pacientes.

use crate::app::{App, PatTab, Screen};
use crate::model::*;
use crate::theme::*;
use crate::tools::*;
use egui::{Align2, Color32, CornerRadius, FontId, RichText, Sense, Ui, vec2};
use std::path::PathBuf;

#[derive(Default)]
pub struct ToolsState {
    pub puesto: crate::screens::puestos::PuestoForm,
    pub query: Query,
    pub query_ids: Option<Vec<u32>>,
    pub query_treat: String,
    pub dir_q: String,
    pub contact_sel: Option<u32>,
    pub restore: Option<PathBuf>,
    /// Lo leído de la copia a restaurar (ruta, ¿cifrada?, resultado), para no leerla en cada fotograma.
    pub restore_read: Option<(PathBuf, bool, Result<Db, String>)>,
    /// Contraseña para abrir una copia cifrada al restaurar.
    pub restore_pw: String,
    /// Contraseña de las copias (al ponerla o cambiarla) y su repetición.
    pub key_pw: String,
    pub key_pw2: String,
    /// ¿La recuerda? Contraseña para probar.
    pub key_test: String,
    pub key_msg: Option<(bool, String)>,
    /// Estado de la segunda copia (cuándo se miró, carpeta, texto, ¿todo bien?).
    pub extra_info: Option<(f64, String, String, bool)>,
    pub board_full: bool,
    pub msg_text: String,
}

// ───────────────────────────── Copias de seguridad ─────────────────────────────

pub fn backups_panel(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Copias de seguridad");
        muted(ui, "Se hace una copia automática cada día al abrir el programa y se guardan las 30 últimas. Elija también una carpeta fuera de este ordenador (disco USB, carpeta en la nube…) para tener una segunda copia por si se estropea el disco.");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let before = app.db.config.backup_extra.clone();
            field(ui, "Carpeta de la segunda copia (disco USB o carpeta en la nube)", &mut app.db.config.backup_extra, 360.0);
            if before != app.db.config.backup_extra {
                app.dirty = true;
            }
        });
        second_copy_status(app, ui);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if primary(ui, "💾 Hacer copia ahora").clicked() {
                match create_backup(&app.db, "manual") {
                    Ok(p) => {
                        let enc = !app.db.config.backup_extra.trim().is_empty() && crate::cifrado::current_key().is_some();
                        app.notify(format!("Copia guardada: {}{}", p.display(), if enc { " (y la segunda copia, cifrada)" } else { "" }));
                    }
                    Err(e) => app.warn(e),
                }
                app.tools.extra_info = None;
            }
            if ui.button("📂 Abrir carpeta de copias").clicked() && let Err(e) = crate::screens::imagenes::open_folder(&backup_dir()) {
                app.warn(e);
            }
            muted(ui, backup_dir().display().to_string());
        });
        ui.add_space(6.0);
        let list = list_backups(&backup_dir());
        let tbl = Table::new("bk", vec![("Fecha", 170.0), ("Fichero", 380.0), ("Tamaño", 100.0), ("", 160.0)]);
        tbl.header(ui);
        if list.is_empty() {
            empty_state(ui, "Todavía no hay copias.");
        }
        let mut pick = None;
        for (i, b) in list.iter().take(10).enumerate() {
            let name = b.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let r = tbl.row(ui, i, false, &[cell(b.when.format("%d/%m/%Y %H:%M").to_string()), cell(name), cell(format!("{} KB", b.bytes / 1024)), cellc("Restaurar…", INFO)]);
            if r.clicked() {
                pick = Some(b.path.clone());
            }
        }
        if pick.is_some() {
            app.tools.restore = pick;
        }
        // restaurar desde otra ruta (p. ej. el USB)
        let key = egui::Id::unique("restore_path");
        let mut other: String = ui.data_mut(|d| d.get_temp(key)).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.add_sized([420.0, 34.0], egui::TextEdit::singleline(&mut other).hint_text("…o ruta de una copia guardada en otro sitio"));
            if ui.button("Restaurar esa copia").clicked() && !other.trim().is_empty() {
                app.tools.restore = Some(PathBuf::from(other.trim().trim_matches('"')));
            }
        });
        ui.data_mut(|d| d.insert_temp(key, other));
    });
    ui.add_space(10.0);
    password_card(app, ui);
    restore_dialog(app, ui.ctx());
}

/// Cuántas copias hay en la carpeta de la segunda copia y si van cifradas
/// (se mira cada pocos segundos, que puede ser un disco USB o una carpeta en red).
fn second_copy_status(app: &mut App, ui: &mut Ui) {
    let extra = app.db.config.backup_extra.trim().to_string();
    if extra.is_empty() {
        return;
    }
    let time = ui.input(|i| i.time);
    let fresh = app.tools.extra_info.as_ref().is_some_and(|(t, dir, _, _)| *dir == extra && time >= *t && time - *t < 5.0);
    if !fresh {
        let dir = std::path::Path::new(&extra);
        let (text, good) = if !dir.is_dir() {
            ("No se encuentra esa carpeta: ¿está conectado el disco? La segunda copia se hará cuando esté.".to_string(), false)
        } else {
            let list = list_backups(dir);
            match list.first() {
                None => ("Todavía no hay copias en esa carpeta: pulse «Hacer copia ahora».".to_string(), false),
                Some(last) => {
                    let enc = list.iter().filter(|b| b.encrypted).count();
                    let plain = list.len() - enc;
                    let kind = match (enc, plain) {
                        (1, 0) => ", cifrada.".to_string(),
                        (_, 0) => ", todas cifradas.".to_string(),
                        (0, _) => ", sin cifrar.".to_string(),
                        (e, p) => format!(": {e} cifradas y {p} sin cifrar."),
                    };
                    let n = if list.len() == 1 { "1 copia".to_string() } else { format!("{} copias", list.len()) };
                    (format!("Segunda copia: {n}; la última, el {} a las {}{kind}", fmt_date(last.when.date()), last.when.format("%H:%M")), plain == 0)
                }
            }
        };
        app.tools.extra_info = Some((time, extra, text, good));
    }
    if let Some((_, _, text, good)) = &app.tools.extra_info {
        ui.label(RichText::new(text).size(13.5).color(if *good { OK } else { WARN }));
    }
}

/// Contraseña de las copias: la segunda copia se guarda cifrada con ella.
fn password_card(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Contraseña de las copias");
        if crate::shared::remote_mode() {
            muted(ui, "Este puesto está conectado al servidor de la clínica: las copias las hace el servidor, y la contraseña de las copias se pone en ese ordenador.");
            return;
        }
        let key = crate::cifrado::current_key();
        match &key {
            None => muted(ui, "La segunda copia lleva los datos de salud de todos los pacientes. Con una contraseña se guarda cifrada: sin ella nadie puede abrirla, ni en el disco ni en la nube. Las copias que ya hay en esa carpeta también se cifran."),
            Some(k) => {
                ui.label(RichText::new(format!("✔ La segunda copia se guarda cifrada desde el {}.", fmt_date(k.since.date()))).size(14.5).color(OK));
                muted(ui, "Para restaurar una copia cifrada en otro ordenador hará falta la contraseña. Si la cambia, las copias que ya hay en la segunda carpeta pasan a la nueva.");
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(if key.is_some() { "Nueva contraseña" } else { "Contraseña de las copias" }).size(13.5).color(MUTED));
                ui.add_sized([230.0, 34.0], egui::TextEdit::singleline(&mut app.tools.key_pw).password(true));
            });
            ui.vertical(|ui| {
                ui.label(RichText::new("Repítala").size(13.5).color(MUTED));
                ui.add_sized([230.0, 34.0], egui::TextEdit::singleline(&mut app.tools.key_pw2).password(true));
            });
            ui.vertical(|ui| {
                ui.add_space(19.0);
                if primary(ui, if key.is_some() { "Cambiar la contraseña" } else { "Cifrar la segunda copia" }).clicked() {
                    set_password(app, key.clone());
                }
            });
        });
        muted(ui, format!("Al menos {} caracteres. Apúntela y guárdela en un sitio seguro fuera del ordenador: sin ella, las copias cifradas no se pueden abrir.", crate::cifrado::MIN_LEN));
        if let Some(k) = &key {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("¿La recuerda?").size(13.5).color(MUTED));
                ui.add_sized([230.0, 34.0], egui::TextEdit::singleline(&mut app.tools.key_test).password(true).hint_text("Escríbala para probarla"));
                if ui.button("Probar").clicked() {
                    app.tools.key_msg = Some(if k.matches(&app.tools.key_test) { (true, "Es la contraseña de las copias.".into()) } else { (false, "No es la contraseña de las copias.".into()) });
                    app.tools.key_test.clear();
                }
            });
        }
        if let Some((ok, msg)) = &app.tools.key_msg {
            ui.label(RichText::new(msg).size(14.0).color(if *ok { OK } else { DANGER }));
        }
    });
}

/// Pone (o cambia) la contraseña de las copias y pasa a ella las copias que ya
/// hay en la segunda carpeta.
fn set_password(app: &mut App, old: Option<crate::cifrado::BackupKey>) {
    if let Err(e) = crate::cifrado::check_password(&app.tools.key_pw, &app.tools.key_pw2) {
        app.tools.key_msg = Some((false, e));
        return;
    }
    let new = match crate::cifrado::BackupKey::new(&app.tools.key_pw, now()) {
        // al cambiarla se conserva desde cuándo se cifra
        Ok(k) => crate::cifrado::BackupKey { since: old.as_ref().map(|o| o.since).unwrap_or(k.since), ..k },
        Err(e) => {
            app.tools.key_msg = Some((false, e));
            return;
        }
    };
    if let Err(e) = crate::cifrado::set_key(Some(new.clone())) {
        app.tools.key_msg = Some((false, e));
        return;
    }
    app.tools.key_pw.clear();
    app.tools.key_pw2.clear();
    app.tools.extra_info = None;
    let extra = app.db.config.backup_extra.trim().to_string();
    let moved = if extra.is_empty() || !std::path::Path::new(&extra).is_dir() { Ok(0) } else { rekey_folder(std::path::Path::new(&extra), old.as_ref(), &new) };
    let first = old.is_none();
    app.db.audit(if first { "Cifrar las copias" } else { "Cambiar la contraseña de las copias" }, &match &moved {
        Ok(n) => format!("Copias de la segunda carpeta pasadas a la contraseña: {n}"),
        Err(e) => e.clone(),
    });
    app.changed();
    let head = if first { "Contraseña puesta: la segunda copia se guardará cifrada desde ahora" } else { "Contraseña cambiada" };
    let cloud = "Si la carpeta está en la nube, vacíe también su papelera, que guarda las copias antiguas.";
    app.tools.key_msg = Some(match moved {
        Ok(0) => (true, format!("{head}.")),
        Ok(1) => (true, format!("{head}, y la copia que ya había en la segunda carpeta está ahora con ella. {cloud}")),
        Ok(n) => (true, format!("{head}, y las {n} copias que ya había en la segunda carpeta están ahora con ella. {cloud}")),
        Err(e) => (false, format!("{head}, pero no se pudieron pasar todas las copias antiguas: {e}")),
    });
}

fn restore_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(path) = app.tools.restore.clone() else { return };
    // la copia se lee una vez (y otra al escribir la contraseña), no en cada fotograma
    if app.tools.restore_read.as_ref().is_none_or(|(p, _, _)| *p != path) {
        app.tools.restore_pw.clear();
        app.tools.restore_read = Some((path.clone(), crate::cifrado::is_encrypted_file(&path), read_backup(&path, None)));
    }
    let Some((_, encrypted, check)) = app.tools.restore_read.take() else { return };
    let mut close = false;
    let mut retry = false;
    crate::theme::modal("restore").show(ctx, |ui| {
        ui.set_width(520.0);
        title(ui, "Restaurar copia de seguridad", &path.display().to_string());
        match &check {
            Err(e) if encrypted => {
                ui.label(RichText::new(e).color(if e == NEEDS_PASSWORD { TEXT } else { DANGER }));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let r = ui.add_sized([280.0, 34.0], egui::TextEdit::singleline(&mut app.tools.restore_pw).password(true).hint_text("Contraseña de las copias"));
                    if primary(ui, "Abrir la copia").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                        retry = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        close = true;
                    }
                });
            }
            Ok(db) => {
                ui.label(RichText::new(format!("La copia contiene: {}", summary(db))).color(OK));
                ui.label(RichText::new(format!("Datos actuales: {}", summary(&app.db))).color(MUTED));
                ui.add_space(6.0);
                ui.label(RichText::new("Los datos actuales se sustituirán por los de la copia. Antes se guardará automáticamente una copia de los datos actuales.").color(WARN));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if danger(ui, "Restaurar").clicked() {
                        match create_backup(&app.db, "antes de restaurar") {
                            Ok(_) => {
                                app.db = db.clone();
                                app.db.audit("Restaurar copia", &path.display().to_string());
                                app.changed();
                                app.patient = app.db.patients.first().map(|p| p.id);
                                app.user_id = None; // volver a identificarse con los usuarios de la copia
                                app.notify("Copia restaurada. Vuelva a entrar con su usuario.");
                                close = true;
                            }
                            Err(e) => app.warn(format!("No se restauró: no se pudo guardar antes la copia de seguridad ({e})")),
                        }
                    }
                    if ui.button("Cancelar").clicked() {
                        close = true;
                    }
                });
            }
            Err(e) => {
                ui.label(RichText::new(e).color(DANGER));
                if ui.button("Cerrar").clicked() {
                    close = true;
                }
            }
        }
    });
    if close {
        app.tools.restore = None;
        app.tools.restore_pw.clear();
        return;
    }
    let check = if retry { read_backup(&path, Some(&app.tools.restore_pw)) } else { check };
    if retry {
        app.tools.restore_pw.clear();
    }
    app.tools.restore_read = Some((path, encrypted, check));
}

// ───────────────────────────── Pizarra de sala de espera ─────────────────────────────

fn display_name(app: &App, a: &Appointment) -> String {
    let full = app.db.appt_name(a);
    if !app.db.config.board_initials {
        return full;
    }
    // privacidad: nombre e iniciales de los apellidos
    let mut parts = full.split_whitespace();
    let first = parts.next().unwrap_or("").to_string();
    let rest: String = parts.filter(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)).map(|w| format!("{}.", w.chars().next().unwrap())).collect::<Vec<_>>().join(" ");
    format!("{first} {rest}")
}

pub fn pizarra(app: &mut App, ui: &mut Ui) {
    let today = app.today;
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect.expand(18.0), 0.0, Color32::from_rgb(12, 26, 44));
    ui.horizontal(|ui| {
        let title = if app.db.multi_center() { app.db.center_name(app.db.config.current_center) } else { app.db.config.name.clone() };
        ui.label(RichText::new(title).family(sb()).size(30.0).color(Color32::WHITE));
        right_aligned(ui, |ui| {
            if ui.button(if app.tools.board_full { "Salir de pantalla completa" } else { "Pantalla completa (TV)" }).clicked() {
                app.tools.board_full = !app.tools.board_full;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(app.tools.board_full));
            }
            if !app.tools.board_full && ui.button("Volver a la agenda").clicked() {
                app.screen = Screen::Agenda;
            }
            let mut ini = app.db.config.board_initials;
            if ui.checkbox(&mut ini, RichText::new("Mostrar solo iniciales").color(Color32::WHITE)).changed() {
                app.db.config.board_initials = ini;
                app.changed();
            }
            ui.label(RichText::new(chrono::Local::now().format("%H:%M").to_string()).size(34.0).family(sb()).color(Color32::from_rgb(120, 220, 200)));
        });
    });
    ui.add_space(14.0);
    let mut called: Vec<Appointment> = app.db.appointments.iter().filter(|a| a.date == today && cid(a.center_id) == cid(app.db.config.current_center) && a.state == ApptState::EnConsulta).cloned().collect();
    called.sort_by_key(|a| std::cmp::Reverse(a.seen_at.unwrap_or(a.start)));
    let mut waiting: Vec<Appointment> = app.db.appointments.iter().filter(|a| a.date == today && cid(a.center_id) == cid(app.db.config.current_center) && matches!(a.state, ApptState::SalaEspera | ApptState::Recepcion)).cloned().collect();
    waiting.sort_by_key(|a| a.start);
    let w = ui.available_width();
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.label(RichText::new("PASE A CONSULTA").size(20.0).family(sb()).color(Color32::from_rgb(120, 220, 200)));
        ui.add_space(8.0);
        if called.is_empty() {
            ui.label(RichText::new("—").size(40.0).color(Color32::from_rgb(90, 110, 140)));
        }
        for (i, a) in called.iter().take(4).enumerate() {
            let big = i == 0;
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), if big { 150.0 } else { 80.0 }), Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, CornerRadius::same(18), if big { Color32::from_rgb(14, 150, 136) } else { Color32::from_rgb(24, 44, 70) });
            p.text(r.left_center() + vec2(26.0, if big { -18.0 } else { 0.0 }), Align2::LEFT_CENTER, display_name(app, a), sbf(if big { 46.0 } else { 26.0 }), Color32::WHITE);
            let gab = app.db.box_name(a.box_id);
            if big {
                p.text(r.left_center() + vec2(26.0, 38.0), Align2::LEFT_CENTER, format!("→ {}  ·  {}", gab, app.db.doctor_alias(a.doctor_id)), FontId::proportional(26.0), Color32::from_rgb(220, 255, 248));
            } else {
                p.text(r.right_center() - vec2(26.0, 0.0), Align2::RIGHT_CENTER, gab, FontId::proportional(22.0), Color32::from_rgb(170, 200, 230));
            }
            ui.add_space(10.0);
        }
        let ui = &mut cols[1];
        ui.label(RichText::new("EN SALA DE ESPERA").size(20.0).family(sb()).color(Color32::from_rgb(255, 196, 90)));
        ui.add_space(8.0);
        if waiting.is_empty() {
            ui.label(RichText::new("Nadie esperando").size(26.0).color(Color32::from_rgb(90, 110, 140)));
        }
        for a in waiting.iter().take(9) {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, CornerRadius::same(14), Color32::from_rgb(24, 44, 70));
            p.text(r.left_center() + vec2(22.0, 0.0), Align2::LEFT_CENTER, display_name(app, a), sbf(24.0), Color32::WHITE);
            p.text(r.right_center() - vec2(22.0, 0.0), Align2::RIGHT_CENTER, format!("cita {}", a.start.format("%H:%M")), FontId::proportional(20.0), Color32::from_rgb(170, 190, 215));
            ui.add_space(8.0);
        }
    });
    let _ = w;
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(5));
}

// ───────────────────────────── Listín telefónico ─────────────────────────────

pub fn listin(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        title(ui, "Listín telefónico", "Contactos de la clínica, colaboradores, proveedores, mutuas y pacientes");
        right_aligned(ui, |ui| {
            if primary(ui, "＋ Nuevo contacto").clicked() {
                let id = app.db.new_id();
                app.db.contacts.push(Contact { id, name: "Nuevo contacto".into(), company: String::new(), category: "General".into(), phone: String::new(), mobile: String::new(), email: String::new(), notes: String::new() });
                app.tools.contact_sel = Some(id);
                app.changed();
            }
        });
    });
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    let side = 380.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail - side - 24.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add_sized([ui.available_width(), 38.0], egui::TextEdit::singleline(&mut app.tools.dir_q).hint_text("🔍 Buscar por nombre, empresa o teléfono (los pacientes aparecen al buscar)"));
                ui.add_space(6.0);
                let tbl = Table::new("dir", vec![("Tipo", 110.0), ("Nombre", 240.0), ("Detalle", 200.0), ("Teléfono", 140.0), ("Email", 220.0)]);
                tbl.header(ui);
                let list = app.db.directory(&app.tools.dir_q);
                if list.is_empty() {
                    empty_state(ui, "Sin resultados.");
                }
                let mut act = None;
                egui::ScrollArea::vertical().id_salt("dirl").max_height(520.0).show(ui, |ui| {
                    for (i, e) in list.iter().enumerate() {
                        let col = match e.kind {
                            "Paciente" => INFO,
                            "Contacto" => ACCENT,
                            "Laboratorio" | "Proveedor" => WARN,
                            _ => TEXT,
                        };
                        let r = tbl.row(ui, i, e.contact_id.is_some() && e.contact_id == app.tools.contact_sel, &[cellc(e.kind, col), cell(&e.name), cell(&e.detail), cell(&e.phone), cell(&e.email)]);
                        if r.clicked() {
                            act = Some(e.clone());
                        }
                    }
                });
                if let Some(e) = act {
                    if let Some(pid) = e.patient_id {
                        app.open_patient(pid, PatTab::Resumen);
                    } else if let Some(cid) = e.contact_id {
                        app.tools.contact_sel = Some(cid);
                    } else if !e.phone.is_empty() {
                        ui.ctx().copy_text(e.phone.clone());
                        app.notify(format!("Teléfono de {} copiado: {}", e.name, e.phone));
                    }
                }
            });
        });
        ui.vertical(|ui| {
            ui.set_width(side);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                let Some(id) = app.tools.contact_sel else {
                    section(ui, "Contacto");
                    muted(ui, "Seleccione un contacto para editarlo. Al pulsar sobre un colaborador o proveedor se copia su teléfono.");
                    return;
                };
                let mut remove = false;
                if let Some(c) = app.db.contacts.iter_mut().find(|c| c.id == id) {
                    let before = serde_json::to_string(c).unwrap_or_default();
                    section(ui, "Contacto");
                    field(ui, "Nombre", &mut c.name, 300.0);
                    field(ui, "Empresa", &mut c.company, 300.0);
                    field(ui, "Categoría", &mut c.category, 300.0);
                    ui.horizontal(|ui| {
                        field(ui, "Teléfono", &mut c.phone, 130.0);
                        field(ui, "Móvil", &mut c.mobile, 130.0);
                    });
                    field(ui, "Email", &mut c.email, 300.0);
                    ui.label(RichText::new("Notas").size(13.5).color(MUTED));
                    ui.add(egui::TextEdit::multiline(&mut c.notes).desired_rows(3).desired_width(f32::INFINITY));
                    if serde_json::to_string(c).unwrap_or_default() != before {
                        app.dirty = true;
                    }
                    if ui.button("Quitar contacto").clicked() {
                        remove = true;
                    }
                }
                if remove {
                    app.db.contacts.retain(|c| c.id != id);
                    app.tools.contact_sel = None;
                    app.changed();
                }
            });
            ui.add_space(10.0);
            crate::screens::llamadas::recent_calls(app, ui);
        });
    });
}

// ───────────────────────────── Consultas de pacientes ─────────────────────────────

fn opt_num(ui: &mut Ui, label: &str, v: &mut Option<u32>, max: u32) {
    ui.label(label);
    let mut on = v.is_some();
    if ui.checkbox(&mut on, "").changed() {
        *v = if on { Some(0) } else { None };
    }
    if let Some(x) = v {
        ui.add(egui::DragValue::new(x).range(0..=max));
    }
}

pub fn consultas(app: &mut App, ui: &mut Ui) {
    let today = app.today;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Consulta de pacientes");
        muted(ui, "Combine criterios para obtener listas de pacientes (campañas, revisiones pendientes, deudas…) y expórtelas o envíeles un mensaje.");
        ui.add_space(6.0);
        let tariffs: Vec<(u32, String)> = app.db.tariffs.iter().map(|t| (t.id, t.name.clone())).collect();
        let docs: Vec<(u32, String)> = app.db.collaborators.iter().map(|c| (c.id, c.alias.clone())).collect();
        let q = &mut app.tools.query;
        ui.horizontal_wrapped(|ui| {
            opt_num(ui, "Edad desde", &mut q.age_min, 120);
            opt_num(ui, "hasta", &mut q.age_max, 120);
            ui.separator();
            ui.label("Sexo");
            crate::theme::combo("q_sex").width(90.0).selected_text(match q.sex.as_deref() { Some("F") => "Mujer", Some("M") => "Hombre", _ => "Todos" }).show_ui(ui, |ui| {
                ui.selectable_value(&mut q.sex, None, "Todos");
                ui.selectable_value(&mut q.sex, Some("F".into()), "Mujer");
                ui.selectable_value(&mut q.sex, Some("M".into()), "Hombre");
            });
            ui.separator();
            ui.label("Tarifa");
            let cur = q.tariff.and_then(|t| tariffs.iter().find(|x| x.0 == t)).map(|x| x.1.clone()).unwrap_or_else(|| "Todas".into());
            crate::theme::combo("q_tar").width(170.0).selected_text(cur).show_ui(ui, |ui| {
                ui.selectable_value(&mut q.tariff, None, "Todas");
                for (id, n) in &tariffs {
                    ui.selectable_value(&mut q.tariff, Some(*id), n);
                }
            });
            ui.label("Doctor");
            let curd = q.doctor.and_then(|t| docs.iter().find(|x| x.0 == t)).map(|x| x.1.clone()).unwrap_or_else(|| "Todos".into());
            crate::theme::combo("q_doc").width(130.0).selected_text(curd).show_ui(ui, |ui| {
                ui.selectable_value(&mut q.doctor, None, "Todos");
                for (id, n) in &docs {
                    ui.selectable_value(&mut q.doctor, Some(*id), n);
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            opt_num(ui, "Sin venir desde hace (meses)", &mut q.no_visit_months, 120);
            ui.separator();
            ui.label("Deuda mínima");
            let mut on = q.min_debt.is_some();
            if ui.checkbox(&mut on, "").changed() {
                q.min_debt = if on { Some(1.0) } else { None };
            }
            if let Some(d) = &mut q.min_debt {
                ui.add(egui::DragValue::new(d).range(0.0..=100000.0).suffix(" €"));
            }
            ui.separator();
            ui.checkbox(&mut q.pending_budget, "Con presupuesto pendiente");
            ui.checkbox(&mut q.only_active, "Solo activos");
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Tratamiento");
            let curt = if app.tools.query_treat.is_empty() { "Cualquiera".to_string() } else { app.db.treatment(&app.tools.query_treat).map(|t| t.description.clone()).unwrap_or_default() };
            crate::theme::combo("q_tto").width(260.0).selected_text(curt).show_ui(ui, |ui| {
                ui.selectable_value(&mut app.tools.query_treat, String::new(), "Cualquiera");
                for t in &app.db.treatments {
                    ui.selectable_value(&mut app.tools.query_treat, t.code.clone(), format!("{} · {}", t.code, t.description));
                }
            });
            let q = &mut app.tools.query;
            crate::theme::combo("q_tst").width(120.0).selected_text(q.treatment_state.map(|s| s.label()).unwrap_or("en cualquier estado")).show_ui(ui, |ui| {
                ui.selectable_value(&mut q.treatment_state, None, "en cualquier estado");
                for s in [TxState::Realizado, TxState::Previsto] {
                    ui.selectable_value(&mut q.treatment_state, Some(s), s.label());
                }
            });
            ui.separator();
            ui.label("Población");
            ui.add_sized([140.0, 32.0], egui::TextEdit::singleline(&mut q.city));
            ui.label("Contactable por");
            crate::theme::combo("q_ch").width(110.0).selected_text(q.channel_ok.unwrap_or("Indiferente")).show_ui(ui, |ui| {
                ui.selectable_value(&mut q.channel_ok, None, "Indiferente");
                for c in ["WhatsApp", "SMS", "Email"] {
                    ui.selectable_value(&mut q.channel_ok, Some(c), c);
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if primary(ui, "🔍 Buscar pacientes").clicked() {
                app.tools.query.treatment = if app.tools.query_treat.is_empty() { None } else { Some(app.tools.query_treat.clone()) };
                app.tools.query_ids = Some(app.db.run_query(&app.tools.query, today));
            }
            if ui.button("Limpiar criterios").clicked() {
                app.tools.query = Query::default();
                app.tools.query_treat.clear();
                app.tools.query_ids = None;
            }
        });
    });
    let Some(ids) = app.tools.query_ids.clone() else { return };
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, &format!("{} pacientes encontrados", ids.len()));
            right_aligned(ui, |ui| {
                if ui.button("⬇ Exportar CSV").clicked() {
                    let dir = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from(".")).join(crate::brand::DIR);
                    let _ = std::fs::create_dir_all(&dir);
                    let path = dir.join(format!("consulta_pacientes_{}.csv", now().format("%Y%m%d_%H%M")));
                    match std::fs::write(&path, app.db.query_csv(&ids, today)) {
                        Ok(()) => app.notify(format!("Exportado en {}", path.display())),
                        Err(e) => app.warn(e.to_string()),
                    }
                }
            });
        });
        // envío de un mensaje a todos (solo con consentimiento)
        ui.horizontal(|ui| {
            ui.add_sized([ui.available_width() - 250.0, 34.0], egui::TextEdit::singleline(&mut app.tools.msg_text).hint_text("Mensaje para todos, p. ej. «Hola [NOMBRE], te esperamos para tu revisión…»"));
            if ui.button("Preparar mensajes (WhatsApp/SMS)").clicked() && !app.tools.msg_text.trim().is_empty() {
                let mut n = 0;
                let mut skipped = 0;
                for id in &ids {
                    let Some(p) = app.db.patient(*id).cloned() else { continue };
                    if !p.accepts_gdpr || p.mobile.is_empty() || !(p.accepts_whatsapp || p.accepts_sms) {
                        skipped += 1;
                        continue;
                    }
                    let ch = if p.accepts_whatsapp { crate::comms::Channel::WhatsApp } else { crate::comms::Channel::Sms };
                    let mid = app.db.new_id();
                    let text = app.db.fill(&app.tools.msg_text, *id, &[]);
                    app.db.messages.push(crate::comms::Message { id: mid, patient_id: *id, kind: crate::comms::MsgKind::Libre, channel: ch, to: p.mobile.clone(), subject: String::new(), text, created: now(), sent_at: None, state: crate::comms::MsgState::Pendiente, related: None, user: app.db.config.current_user.clone(), error: String::new(), attempts: 0 });
                    n += 1;
                }
                app.changed();
                app.notify(format!("{n} mensajes preparados en Comunicaciones · {skipped} pacientes sin consentimiento o sin móvil"));
            }
        });
        let tbl = Table::new("qres", vec![("Nº", 70.0), ("Paciente", 230.0), ("Edad", 60.0), ("Móvil", 130.0), ("Población", 140.0), ("Tarifa", 170.0), ("Última visita", 110.0), ("Saldo", 100.0)]);
        tbl.header(ui);
        let mut open = None;
        egui::ScrollArea::vertical().id_salt("qrl").max_height(420.0).show(ui, |ui| {
            for (i, id) in ids.iter().enumerate() {
                let Some(p) = app.db.patient(*id) else { continue };
                let bal = app.db.patient_balance(*id);
                if tbl.row(ui, i, false, &[cell(p.history_no.to_string()), cell(p.full_name()), cell(p.age(today).map(|a| a.to_string()).unwrap_or_default()), cell(&p.mobile), cell(&p.city), cell(app.db.tariff_name(p.tariff_id)), cell(app.db.last_visit(*id).map(fmt_date).unwrap_or_else(|| "nunca".into())), cellc(money(bal), if bal > 0.005 { DANGER } else { MUTED })]).clicked() {
                    open = Some(*id);
                }
            }
        });
        if let Some(id) = open {
            app.open_patient(id, PatTab::Resumen);
        }
    });
}
