//! Registro de jornada en pantalla (ver `crate::jornada`): fichar en Inicio,
//! y el registro de cada persona, sus correcciones y su exportación para Dirección.

use crate::app::App;
use crate::jornada::{Kind, csv, day_summary, effective, next_allowed};
use crate::theme::*;
use chrono::{Duration, NaiveDate, NaiveTime};
use egui::{RichText, Ui};

pub struct JornadaState {
    pub user: u32,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub fix_mark: Option<u32>,
    pub fix_kind: Kind,
    pub fix_day: NaiveDate,
    pub fix_time: String,
    pub reason: String,
    pub status: String,
}

impl JornadaState {
    pub fn new(today: NaiveDate) -> Self {
        JornadaState { user: 0, from: today - Duration::days(30), to: today, fix_mark: None, fix_kind: Kind::Entrada, fix_day: today, fix_time: "09:00".into(), reason: String::new(), status: String::new() }
    }
}

/// Inicio: fichar (lo de hoy de quien ha entrado).
pub fn punch_card(app: &mut App, ui: &mut Ui) {
    let Some(me) = app.user_id else { return };
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Registro de jornada");
        let mine: Vec<crate::jornada::Punch> = app.db.punches_for(me, false).into_iter().cloned().collect();
        let today = effective(&mine, me, app.today);
        ui.horizontal_wrapped(|ui| {
            for p in &today {
                pill(ui, &format!("{} {}", p.kind.label(), p.at.format("%H:%M")), MUTED);
            }
            if today.is_empty() {
                muted(ui, "Hoy aún no ha fichado.");
            }
        });
        ui.horizontal_wrapped(|ui| {
            for k in next_allowed(&today) {
                if primary(ui, &format!("Fichar: {}", k.label())).clicked() {
                    match app.db.punch(me, k, crate::model::now()) {
                        Ok(_) => {
                            app.changed();
                            app.notify(format!("{} a las {}.", k.label(), crate::model::now().format("%H:%M")));
                        }
                        Err(e) => app.warn(e),
                    }
                }
            }
        });
        let (_, _, paused, worked, _) = day_summary(&today);
        muted(ui, format!("Hoy: {} h {:02} min de trabajo y {paused} min de pausa (hasta la última marca). Para corregir algo, dígaselo a Dirección.", worked / 60, worked % 60));
    });
}

/// Configuración › Usuarios: el registro de cada persona (Dirección).
pub fn admin_panel(app: &mut App, ui: &mut Ui) {
    if !app.can(crate::users::Perm::Configuracion) {
        return;
    }
    if app.jornada.user == 0 {
        app.jornada.user = app.user_id.unwrap_or(1);
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Registro de jornada");
        muted(ui, "Cada persona ficha lo suyo a la hora del momento. Las correcciones las hace Dirección con motivo y la marca original se conserva. El registro no se borra.");
        let users: Vec<(u32, String)> = app.db.users.iter().map(|u| (u.id, u.name.clone())).collect();
        ui.horizontal_wrapped(|ui| {
            let cur = users.iter().find(|u| u.0 == app.jornada.user).map(|u| u.1.clone()).unwrap_or_default();
            crate::theme::combo("jor_user").width(200.0).selected_text(cur).show_ui(ui, |ui| {
                for (id, n) in &users {
                    ui.selectable_value(&mut app.jornada.user, *id, n);
                }
            });
            ui.label(RichText::new("Del").color(MUTED));
            crate::screens::date_picker(ui, "jor_from", &mut app.jornada.from);
            ui.label(RichText::new("al").color(MUTED));
            crate::screens::date_picker(ui, "jor_to", &mut app.jornada.to);
            if ui.button("💾 Exportar (CSV) a Documentos").clicked() {
                let ids: Vec<u32> = users.iter().map(|u| u.0).collect();
                let p = crate::paths::documents().join(format!("registro_jornada_{}_{}.csv", app.jornada.from.format("%Y%m%d"), app.jornada.to.format("%Y%m%d")));
                app.jornada.status =
                    match std::fs::create_dir_all(crate::paths::documents()).map_err(|e| e.to_string()).and_then(|_| crate::paths::write_private(&p, csv(&app.db, &ids, app.jornada.from, app.jornada.to).as_bytes()).map_err(|e| e.to_string())) {
                        Ok(()) => format!("Exportado en {}.", p.display()),
                        Err(e) => format!("No se pudo exportar: {e}"),
                    };
            }
        });
        let u = app.jornada.user;
        let mut day = app.jornada.to;
        let mut shown = 0;
        egui::ScrollArea::vertical().id_salt("jor_days").max_height(240.0).show(ui, |ui| {
            while day >= app.jornada.from && shown < 62 {
                let marks = effective(&app.db.punches, u, day);
                if !marks.is_empty() {
                    let (_, _, paused, worked, incomplete) = day_summary(&marks);
                    let list: Vec<String> = marks.iter().map(|p| format!("{} {}", p.kind.label(), p.at.format("%H:%M"))).collect();
                    ui.label(RichText::new(format!("{} · {} · {} h {:02} min (pausas {paused} min){}", day.format("%d/%m/%Y"), list.join(", "), worked / 60, worked % 60, if incomplete { " · incompleto" } else { "" })).color(if incomplete {
                        WARN
                    } else {
                        TEXT
                    }));
                    shown += 1;
                }
                day -= Duration::days(1);
            }
        });
        // corrección
        ui.add_space(6.0);
        ui.label(RichText::new("Corregir").family(sb()));
        let day_marks = effective(&app.db.punches, u, app.jornada.fix_day);
        ui.horizontal_wrapped(|ui| {
            crate::screens::date_picker(ui, "jor_fix_day", &mut app.jornada.fix_day);
            let cur = app.jornada.fix_mark.and_then(|id| day_marks.iter().find(|p| p.id == id)).map(|p| format!("{} {}", p.kind.label(), p.at.format("%H:%M"))).unwrap_or_else(|| "Falta una marca".into());
            crate::theme::combo("jor_fix_mark").width(200.0).selected_text(cur).show_ui(ui, |ui| {
                ui.selectable_value(&mut app.jornada.fix_mark, None, "Falta una marca");
                for p in &day_marks {
                    ui.selectable_value(&mut app.jornada.fix_mark, Some(p.id), format!("{} {}", p.kind.label(), p.at.format("%H:%M")));
                }
            });
            crate::theme::combo("jor_fix_kind").width(160.0).selected_text(app.jornada.fix_kind.label()).show_ui(ui, |ui| {
                for k in [Kind::Entrada, Kind::InicioPausa, Kind::FinPausa, Kind::Salida] {
                    ui.selectable_value(&mut app.jornada.fix_kind, k, k.label());
                }
            });
            ui.add_sized([70.0, 32.0], egui::TextEdit::singleline(&mut app.jornada.fix_time).hint_text("HH:MM"));
            ui.add_sized([260.0, 32.0], egui::TextEdit::singleline(&mut app.jornada.reason).hint_text("Motivo (obligatorio)"));
            let go = ui.button("Guardar la corrección").clicked();
            let void = app.jornada.fix_mark.is_some() && ui.button("Anular esa marca").clicked();
            if go || void {
                let at = NaiveTime::parse_from_str(app.jornada.fix_time.trim(), "%H:%M").ok().map(|t| app.jornada.fix_day.and_time(t));
                let admin = app.can(crate::users::Perm::Configuracion);
                let r = match at {
                    None => Err("Hora no válida (HH:MM).".to_string()),
                    Some(at) => app.db.correct_punch(app.jornada.fix_mark, u, (!void).then_some(app.jornada.fix_kind), at, &app.jornada.reason.clone(), admin),
                };
                match r {
                    Ok(_) => {
                        app.jornada.reason.clear();
                        app.jornada.fix_mark = None;
                        app.changed();
                        app.jornada.status = "Corrección guardada; la marca original se conserva.".into();
                    }
                    Err(e) => app.jornada.status = e,
                }
            }
        });
        if !app.jornada.status.is_empty() {
            ui.label(RichText::new(&app.jornada.status).color(INFO));
        }
    });
}
