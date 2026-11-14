//! Grupo de clínicas en las pantallas (ver `crate::grupo` y D-25):
//! - en Configuración › Trabajo en red, las claves de grupo de esta clínica
//!   (Dirección);
//! - en Informes, la vista del grupo con el resumen de cada clínica y los
//!   totales.

use crate::app::App;
use crate::grupo::{Link, Summary, connection_text, consolidate, money, parse_connection};
use crate::theme::*;
use crate::users::Perm;
use egui::{RichText, Ui};

type Fetch = std::sync::mpsc::Receiver<Vec<(String, Result<Summary, String>)>>;

#[derive(Default)]
pub struct GroupUi {
    /// Para quién es la clave nueva y la última creada (se enseña una vez).
    key_name: String,
    pub shown: Option<String>,
    /// Clínicas del grupo en este ordenador (se leen al abrir la pestaña).
    pub links: Option<Vec<Link>>,
    pub new_name: String,
    pub new_text: String,
    pub msg: Option<(bool, String)>,
    job: Option<Fetch>,
    pub results: Vec<(String, Result<Summary, String>)>,
}

/// Dirección y huella con las que el grupo llega a este servidor.
fn this_server() -> Option<(String, String)> {
    if crate::server::local_server_running() {
        let cfg = crate::server::server_config();
        let addr = format!("{}:{}", crate::server::local_ip()?, cfg.port);
        return Some((addr, cfg.pin()));
    }
    let st = crate::shared::station();
    (crate::shared::remote_mode() && !st.server_pin.is_empty()).then(|| (st.server.clone(), st.server_pin.clone()))
}

/// Configuración › Trabajo en red: las claves con las que el grupo pide el
/// resumen de esta clínica.
pub fn keys_card(app: &mut App, ui: &mut Ui) {
    if !app.can(Perm::Configuracion) {
        return;
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Grupo de clínicas");
        muted(
            ui,
            "Si la clínica es de un grupo, el grupo puede ver su resumen (producción, cobros, facturación, citas y altas por centro, sin ningún dato de pacientes) con una clave que se crea aquí y se puede retirar cuando se quiera. Esa clave solo sirve para el resumen.",
        );
        let Some((addr, pin)) = this_server() else {
            muted(ui, "Hace falta el servidor de la clínica (arriba) para que el grupo pueda pedir el resumen.");
            return;
        };
        ui.horizontal(|ui| {
            ui.label("Para quién es");
            ui.add(egui::TextEdit::singleline(&mut app.grupo.key_name).hint_text("p. ej. Dirección del grupo").desired_width(220.0));
            if ui.button("Crear clave de grupo").clicked() {
                match app.db.group_key_new(&app.grupo.key_name) {
                    Ok((_, token)) => {
                        app.grupo.shown = Some(connection_text(&addr, &pin, &token));
                        app.grupo.key_name.clear();
                        app.changed();
                    }
                    Err(e) => app.warn(e),
                }
            }
        });
        if let Some(t) = app.grupo.shown.clone() {
            ui.label(RichText::new("Datos para el grupo (se enseñan solo ahora: cópielos y páselos por un medio seguro)").color(WARN));
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&t).monospace().size(12.0));
                if ui.small_button("Copiar").clicked() {
                    ui.ctx().copy_text(t.clone());
                }
                if ui.small_button("Hecho").clicked() {
                    app.grupo.shown = None;
                }
            });
        }
        let keys: Vec<crate::grupo::GroupKey> = app.db.group_keys.iter().filter(|k| k.revoked.is_none()).cloned().collect();
        for k in keys {
            ui.horizontal(|ui| {
                ui.label(format!("{} · creada el {} por {}", k.name, k.created.format("%d/%m/%Y"), k.by));
                if ui.small_button("Retirar").clicked() {
                    match app.db.group_key_revoke(k.id) {
                        Ok(()) => app.changed(),
                        Err(e) => app.warn(e),
                    }
                }
            });
        }
    });
}

/// Informes › Grupo de clínicas: el resumen de cada clínica del grupo y los
/// totales, para el periodo de los informes.
pub fn view(app: &mut App, ui: &mut Ui) {
    if !app.can(Perm::Informes) {
        muted(ui, "La vista del grupo es para Dirección.");
        return;
    }
    let g = &mut app.grupo;
    let links = g.links.get_or_insert_with(crate::grupo::load_links).clone();
    if let Some(rx) = &g.job
        && let Ok(r) = rx.try_recv()
    {
        g.results = r;
        g.job = None;
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Clínicas del grupo");
        muted(ui, "Cada clínica da sus datos de conexión (Configuración › Trabajo en red › Grupo de clínicas). Se guardan solo en este ordenador. El resumen no lleva datos de pacientes.");
        for (i, l) in links.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&l.name).family(sb()));
                ui.label(RichText::new(&l.address).color(MUTED));
                if ui.small_button("Quitar").clicked() {
                    let mut v = links.clone();
                    v.remove(i);
                    g.msg = crate::grupo::save_links(&v).err().map(|e| (false, e));
                    g.links = Some(v);
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut g.new_name).hint_text("Nombre de la clínica").desired_width(180.0));
            ui.add(egui::TextEdit::singleline(&mut g.new_text).hint_text("veodardental-grupo:…").desired_width(360.0));
            if ui.button("Añadir clínica").clicked() {
                match parse_connection(&g.new_name, &g.new_text) {
                    Ok(l) => {
                        let mut v = links.clone();
                        v.retain(|x| x.address != l.address);
                        v.push(l);
                        g.msg = Some(match crate::grupo::save_links(&v) {
                            Ok(()) => (true, "Clínica añadida.".into()),
                            Err(e) => (false, e),
                        });
                        g.links = Some(v);
                        g.new_name.clear();
                        g.new_text.clear();
                    }
                    Err(e) => g.msg = Some((false, e)),
                }
            }
        });
        if let Some((ok, m)) = &g.msg {
            ui.label(RichText::new(m).color(if *ok { OK } else { WARN }));
        }
    });
    let (from, to) = (app.lists.report_from, app.lists.report_to);
    let g = &mut app.grupo;
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let busy = g.job.is_some();
        if ui.add_enabled(!busy && !links.is_empty(), egui::Button::new("Actualizar el resumen del grupo")).clicked() {
            let (tx, rx) = std::sync::mpsc::channel();
            let list = links.clone();
            std::thread::spawn(move || {
                let out = list.iter().map(|l| (l.name.clone(), crate::server::group_summary(l, from, to))).collect();
                let _ = tx.send(out);
            });
            g.job = Some(rx);
        }
        if busy {
            ui.spinner();
            muted(ui, "Pidiendo el resumen a cada clínica…");
        }
    });
    if g.results.is_empty() {
        return;
    }
    let ok: Vec<Summary> = g.results.iter().filter_map(|(_, r)| r.as_ref().ok().cloned()).collect();
    for (name, r) in &g.results {
        if let Err(e) = r {
            ui.label(RichText::new(format!("{name}: {e}")).color(WARN));
        }
    }
    ui.add_space(6.0);
    egui::Grid::new("group_summary").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
        for h in ["Clínica", "Centro", "Actos", "Producción", "Cobrado", "Citas", "Acudió", "No acudió"] {
            ui.label(RichText::new(h).size(12.5).color(MUTED));
        }
        ui.end_row();
        for s in &ok {
            for (center, f) in &s.centers {
                ui.label(&s.clinic);
                ui.label(center);
                ui.label(f.acts.to_string());
                ui.label(money(f.production));
                ui.label(money(f.collected));
                ui.label(f.appointments.to_string());
                ui.label(f.attended.to_string());
                ui.label(f.no_shows.to_string());
                ui.end_row();
            }
        }
        let (t, _, _) = consolidate(&ok);
        ui.label(RichText::new("Total del grupo").family(sb()));
        ui.label("");
        ui.label(RichText::new(t.acts.to_string()).family(sb()));
        ui.label(RichText::new(money(t.production)).family(sb()));
        ui.label(RichText::new(money(t.collected)).family(sb()));
        ui.label(RichText::new(t.appointments.to_string()).family(sb()));
        ui.label(RichText::new(t.attended.to_string()).family(sb()));
        ui.label(RichText::new(t.no_shows.to_string()).family(sb()));
        ui.end_row();
    });
    let (_, invoiced, new_patients) = consolidate(&ok);
    ui.add_space(4.0);
    ui.label(format!("Facturado: {} · pacientes nuevos: {}", money(invoiced), new_patients));
    for s in &ok {
        muted(ui, format!("{} ({}): facturado {}, pacientes nuevos {}", s.clinic, s.nif, money(s.invoiced), s.new_patients));
    }
    muted(
        ui,
        "Cada clínica calcula sus cifras como en sus informes: producción de los tratamientos realizados (paciente y mutua), cobrado neto (con anulaciones y devoluciones), facturado neto (con rectificativas) y citas sin las anuladas ni las reprogramadas.",
    );
}
