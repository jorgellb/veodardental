//! Configuración › Trabajo en red: servidor de la clínica, puestos que se
//! conectan a él y, como alternativa sin servidor, carpeta compartida.

use crate::app::App;
use crate::server::{self, Remote};
use crate::shared::*;
use crate::theme::*;
use egui::{RichText, Ui};

pub struct PuestoForm {
    pub st: Station,
    /// Estado del servidor en este ordenador (se comprueba al pedirlo).
    pub server_up: Option<bool>,
}

impl Default for PuestoForm {
    fn default() -> Self {
        let mut st = station().clone();
        if st.number == 0 {
            st.number = 1;
        }
        if st.name.trim().is_empty() {
            st.name = machine_name();
        }
        Self { st, server_up: None }
    }
}

fn mode_text(app: &App) -> (String, egui::Color32) {
    if app.remote.is_some() && app.persist {
        (format!("Este puesto está conectado al servidor {} como puesto {}.", station().server, app.db.station), OK)
    } else if shared() {
        (format!("Este ordenador usa la carpeta compartida {} ({}).", station().data_dir, station_label()), INFO)
    } else {
        ("Los datos están solo en este ordenador.".to_string(), MUTED)
    }
}

pub fn panel(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Trabajo en red: servidor y puestos");
        let (t, c) = mode_text(app);
        ui.label(RichText::new(t).color(c).family(sb()));
        muted(
            ui,
            "Lo recomendado: un ordenador hace de servidor (guarda todos los datos y las copias de seguridad) y los ordenadores de recepción y gabinetes se conectan a él. Cada puesto ve al momento lo que hacen los demás. Úselo dentro de la red de la clínica (o con VPN entre centros).",
        );
        ui.add_space(8.0);
        ui.columns(2, |cols| {
            // ── servidor en este ordenador ──
            let ui = &mut cols[0];
            ui.label(RichText::new("1 · Servidor (en el ordenador principal)").family(sb()).size(16.0));
            if app.tools.puesto.server_up.is_none() {
                app.tools.puesto.server_up = Some(server::local_server_running());
            }
            let cfg = server::server_config();
            let addr = format!("{}:{}", server::local_ip().unwrap_or_else(|| "IP-de-este-ordenador".into()), cfg.port);
            if app.tools.puesto.server_up == Some(true) {
                ui.label(RichText::new("✔ Este ordenador es el servidor de la clínica").color(OK));
                ui.horizontal(|ui| {
                    ui.label("Dirección para los puestos:");
                    ui.label(RichText::new(&addr).family(sb()));
                    if ui.small_button("Copiar").clicked() {
                        ui.ctx().copy_text(addr.clone());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Clave de la clínica:");
                    ui.label(RichText::new(&cfg.key).family(sb()));
                    if ui.small_button("Copiar").clicked() {
                        ui.ctx().copy_text(cfg.key.clone());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Huella del servidor:");
                    ui.label(RichText::new(server::fingerprint(&cfg.pin())).monospace());
                })
                .response
                .on_hover_text("Al conectar un puesto, compruebe que ve esta misma huella: así sabe que habla con este servidor.");
                muted(ui, "Este ordenador debe estar encendido, con VeodarDental abierto, mientras se trabaje en los demás puestos.");
                if ui.button("Detener el servidor").clicked() {
                    let _ = server::local_remote(&cfg).stop();
                    app.tools.puesto.server_up = None;
                    app.notify("Servidor detenido. Los puestos no podrán guardar hasta que vuelva a arrancarse.");
                }
            } else {
                muted(ui, "Use la opción en el ordenador que vaya a guardar los datos (el más fiable, siempre encendido). El servidor empieza con los datos que tiene ahora este ordenador.");
                if primary(ui, "Hacer de este ordenador el servidor").clicked() {
                    // si este ordenador estaba en una carpeta compartida o era cliente, avisar
                    if remote_mode() && !server::is_local(&station().server) {
                        app.warn("Este puesto está conectado a otro servidor: desconéctelo primero.");
                    } else {
                        // el servidor abre los datos de este ordenador: esta ventana los
                        // deja guardados (con la anotación) y los suelta
                        app.db.audit("Servidor de datos activado", &addr);
                        app.changed();
                        app.release_local();
                        match server::ensure_server_config().and_then(|cfg| server::spawn_local_server().map(|_| cfg)) {
                            Ok(cfg) => {
                                let st = Station { server: format!("127.0.0.1:{}", cfg.port), key: cfg.key.clone(), data_dir: String::new(), number: 0, name: app.tools.puesto.st.name.clone(), server_pin: cfg.pin() };
                                match save_station(&st) {
                                    Ok(()) => {
                                        app.tools.puesto.server_up = None;
                                        app.notify("Servidor en marcha. Cierre y vuelva a abrir VeodarDental en este ordenador; luego conecte los demás puestos con la dirección y la clave que aparecerán aquí.");
                                    }
                                    Err(e) => app.warn(format!("No se pudo guardar el ajuste: {e}")),
                                }
                            }
                            Err(e) => {
                                app.reopen_local();
                                app.db.audit("No se pudo arrancar el servidor de datos", &e);
                                app.changed();
                                app.warn(format!("No se pudo arrancar el servidor: {e}"));
                            }
                        }
                    }
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Comprobar de nuevo").clicked() {
                    app.tools.puesto.server_up = None;
                }
            });

            // ── puesto cliente ──
            let ui = &mut cols[1];
            ui.label(RichText::new("2 · Puestos (recepción, gabinetes…)").family(sb()).size(16.0));
            let f = &mut app.tools.puesto;
            field(ui, "Dirección del servidor (p. ej. 192.168.1.10:8899)", &mut f.st.server, 300.0);
            field(ui, "Clave de la clínica", &mut f.st.key, 200.0);
            field(ui, "Nombre de este puesto", &mut f.st.name, 200.0);
            let form = f.st.clone();
            ui.horizontal(|ui| {
                if primary(ui, "Conectar este puesto").clicked() {
                    if form.server.trim().is_empty() || form.key.trim().is_empty() {
                        app.warn("Indique la dirección del servidor y la clave de la clínica.");
                    } else {
                        let r = Remote::new(&form.server, &form.key, 0);
                        match r.register(form.name.trim()) {
                            Ok(n) => {
                                let st = Station { data_dir: String::new(), number: n, server_pin: r.pinned(), ..form.clone() };
                                let print = server::fingerprint(&st.server_pin);
                                match save_station(&st) {
                                    Ok(()) => app.notify(format!(
                                        "Conexión correcta: este ordenador será el puesto {n}. Huella del servidor: {print} (debe ser la misma que muestra el servidor). Cierre y vuelva a abrir VeodarDental para trabajar con los datos del servidor."
                                    )),
                                    Err(e) => app.warn(format!("No se pudo guardar el ajuste: {e}")),
                                }
                            }
                            Err(e) => app.warn(e),
                        }
                    }
                }
                if (remote_mode() || shared()) && ui.button("Desconectar (datos solo en este ordenador)").clicked() {
                    let st = Station { server: String::new(), key: String::new(), data_dir: String::new(), server_pin: String::new(), ..station().clone() };
                    match save_station(&st) {
                        Ok(()) => app.notify("Al reiniciar, este puesto usará sus datos locales."),
                        Err(e) => app.warn(format!("No se pudo guardar el ajuste: {e}")),
                    }
                }
            });
        });
        if !app.db.stations.is_empty() {
            ui.add_space(6.0);
            let mut list = app.db.stations.clone();
            list.sort_by_key(|s| s.id);
            muted(ui, format!("Puestos registrados: {}", list.iter().map(|s| format!("{} · {} (visto {})", s.id, s.name, s.last_seen.format("%d/%m %H:%M"))).collect::<Vec<_>>().join("   ")));
        }
        ui.add_space(6.0);
        // carpeta compartida (T-F4-05): ya no para puestos nuevos; los que la usan pasan al servidor
        if shared() {
            ui.add_space(6.0);
            let dir = std::path::PathBuf::from(station().data_dir.trim());
            match crate::shared::migrated(&dir) {
                Some(m) => {
                    ui.label(RichText::new(format!("{} {m}", crate::shared::MOVED)).color(DANGER));
                }
                None => {
                    ui.label(
                        RichText::new(
                            "Este puesto usa una carpeta compartida: es un modo desaconsejado (reparte el fichero entero, con las contraseñas y las claves, a cualquiera que pueda abrir la carpeta) y se retira. Pase los datos a un servidor.",
                        )
                        .color(WARN),
                    );
                    if ui.button("Pasar los datos al servidor (este ordenador hará de servidor)").clicked() {
                        match crate::shared::move_to_server(&dir, &crate::model::Db::local_path(), &machine_name()) {
                            Ok(n) => {
                                let st = Station { data_dir: String::new(), ..station().clone() };
                                match save_station(&st) {
                                    Ok(()) => {
                                        app.db.audit("Carpeta compartida pasada al servidor", &format!("{n} pacientes desde {}", dir.display()));
                                        app.notify(format!("{n} pacientes pasados a este ordenador. Cierre y vuelva a abrir el programa, pulse «Hacer de este ordenador el servidor» y conecte los demás puestos."));
                                    }
                                    Err(e) => app.warn(format!("Datos pasados, pero no se pudo cambiar el ajuste de este puesto: {e}")),
                                }
                            }
                            Err(e) => app.warn(e),
                        }
                    }
                }
            }
        } else {
            ui.add_space(6.0);
            muted(ui, crate::shared::NO_NEW_SHARED);
        }
    });
    ui.add_space(8.0);
    crate::screens::grupo::keys_card(app, ui);
}

/// Pantalla cuando el puesto no puede conectar con el servidor.
pub fn offline_screen(app: &mut App, ui: &mut Ui) {
    let msg = app.offline.clone().unwrap_or_default();
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.18);
        egui::Frame::new().fill(CARD).corner_radius(egui::CornerRadius::same(16)).inner_margin(egui::Margin::same(28)).show(ui, |ui| {
            ui.set_width(560.0);
            ui.label(RichText::new("Sin conexión con el servidor de la clínica").size(24.0).family(sb()));
            ui.add_space(6.0);
            ui.label(RichText::new(&msg).color(DANGER));
            ui.add_space(6.0);
            muted(ui, format!("Servidor: {}", station().server));
            muted(ui, "Compruebe que el ordenador servidor está encendido, con VeodarDental abierto, y que este ordenador está en la misma red. Para no perder datos, este puesto no deja trabajar sin conexión.");
            // el servidor tiene otra huella: se enseña para compararla con la que muestra el servidor
            let changed = msg == server::SERVER_CHANGED;
            let seen = app.remote.as_ref().map(|r| r.server_pin()).unwrap_or_default();
            if changed && !seen.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(format!("Huella que conocía este puesto: {}", server::fingerprint(&station().server_pin))).monospace());
                ui.label(RichText::new(format!("Huella del servidor ahora:      {}", server::fingerprint(&seen))).monospace().color(WARN));
                muted(ui, "En el ordenador servidor, la huella aparece en Configuración › Puestos (o al arrancarlo). Si coincide con la de ahora, puede confiar en él.");
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if primary(ui, "Reintentar").clicked() {
                    app.reconnect();
                }
                if changed && !seen.is_empty() && ui.button("La huella coincide: confiar en este servidor").clicked() {
                    app.trust_server(&seen);
                }
            });
        });
    });
}
