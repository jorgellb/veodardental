//! Laboratorio protésico: trabajos enviados, fechas previstas, pruebas,
//! recepción, repeticiones y coste por laboratorio.

use crate::app::{App, PatTab};
use crate::model::*;
use crate::stock::*;
use crate::theme::*;
use chrono::Datelike;
use egui::{Color32, RichText, Ui};

pub fn state_color(s: LabState) -> Color32 {
    match s {
        LabState::Pendiente => MUTED,
        LabState::Enviado => INFO,
        LabState::EnPrueba => Color32::from_rgb(142, 98, 214),
        LabState::Recibido => ACCENT,
        LabState::Colocado => OK,
        LabState::Repeticion => DANGER,
    }
}

pub fn screen(app: &mut App, ui: &mut Ui) {
    let today = app.today;
    title(ui, "Laboratorio", "Trabajos de prótesis enviados al laboratorio y su seguimiento");
    let open = app.db.lab_works.iter().filter(|w| w.state.open()).count();
    let late = app.db.lab_works.iter().filter(|w| w.overdue(today)).count();
    let month_cost: f64 = app.db.lab_works.iter().filter(|w| w.sent.year() == today.year() && w.sent.month() == today.month()).map(|w| w.cost).sum();
    let ready = app.db.lab_works.iter().filter(|w| w.state == LabState::Recibido).count();
    let w = kpi_w(ui, 4);
    ui.horizontal(|ui| {
        kpi(ui, w, "Trabajos en curso", &open.to_string(), "en laboratorio o en prueba", INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Retrasados", &late.to_string(), "fecha prevista superada", DANGER);
        ui.add_space(4.0);
        kpi(ui, w, "Listos para colocar", &ready.to_string(), "recibidos en clínica", OK);
        ui.add_space(4.0);
        kpi(ui, w, "Coste del mes", &money(month_cost), "trabajos enviados este mes", ACCENT);
    });
    ui.add_space(8.0);
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    let side = 360.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail - side - 24.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if ui.selectable_label(app.stock.lab_filter_open, "En curso").clicked() {
                        app.stock.lab_filter_open = true;
                    }
                    if ui.selectable_label(!app.stock.lab_filter_open, "Todos").clicked() {
                        app.stock.lab_filter_open = false;
                    }
                });
                ui.add_space(6.0);
                let tbl = Table::new("labs", vec![("Enviado", 100.0), ("Paciente", 150.0), ("Trabajo", 150.0), ("Piezas", 50.0), ("Laboratorio", 120.0), ("Prevista", 100.0), ("Estado", 135.0)]);
                tbl.header(ui);
                let mut list: Vec<LabWork> = app.db.lab_works.iter().filter(|w| !app.stock.lab_filter_open || w.state.open() || w.state == LabState::Recibido).cloned().collect();
                list.sort_by_key(|w| (!w.overdue(today), w.due));
                if list.is_empty() {
                    empty_state(ui, "No hay trabajos en curso.");
                }
                let mut pick = None;
                for (i, w) in list.iter().enumerate() {
                    let late = w.overdue(today);
                    let teeth: Vec<String> = w.teeth.iter().map(|t| t.to_string()).collect();
                    let cells = [
                        cell(fmt_date(w.sent)),
                        cell(app.db.patient_name(w.patient_id)),
                        cell(&w.description),
                        cell(teeth.join("-")),
                        cell(app.db.supplier_name(w.lab_id)),
                        cellc(fmt_date(w.due), if late { DANGER } else { TEXT }),
                        cellc(format!("{}{}", w.state.label(), if late { " ⚠" } else { "" }), state_color(w.state)),
                    ];
                    if tbl.row(ui, i, app.stock.lab_sel == Some(w.id), &cells).clicked() {
                        pick = Some(w.id);
                    }
                }
                if pick.is_some() {
                    app.stock.lab_sel = pick;
                } else if app.stock.lab_sel.is_none() {
                    app.stock.lab_sel = list.first().map(|w| w.id);
                }
            });
            ui.add_space(8.0);
            new_work(app, ui);
        });
        ui.vertical(|ui| {
            ui.set_width(side);
            detail(app, ui);
        });
    });
}

fn new_work(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Nuevo trabajo de laboratorio");
        let Some(pid) = app.patient else {
            muted(ui, "Seleccione un paciente con el buscador de arriba.");
            return;
        };
        ui.label(RichText::new(format!("Paciente: {}", app.db.patient_name(pid))).family(sb()));
        let pending: Vec<(u32, String)> = app.db.prosthetic_without_lab(pid).iter().map(|e| (e.id, format!("{} {} ({})", e.description, e.teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join("-"), e.state.label()))).collect();
        let labs: Vec<(u32, String)> = app.db.suppliers.iter().filter(|s| s.is_lab).map(|s| (s.id, s.name.clone())).collect();
        let key = egui::Id::unique("newlab");
        let mut st: (Option<u32>, u32, i64) = ui.data_mut(|d| d.get_temp(key)).unwrap_or((pending.first().map(|p| p.0), labs.first().map(|l| l.0).unwrap_or(0), 7));
        ui.horizontal_wrapped(|ui| {
            let cur = st.0.and_then(|id| pending.iter().find(|p| p.0 == id)).map(|p| p.1.clone()).unwrap_or_else(|| "Trabajo libre".into());
            crate::theme::combo("nl_e").width(300.0).selected_text(cur).show_ui(ui, |ui| {
                ui.selectable_value(&mut st.0, None, "Trabajo libre");
                for (id, n) in &pending {
                    ui.selectable_value(&mut st.0, Some(*id), n);
                }
            });
            let curl = labs.iter().find(|l| l.0 == st.1).map(|l| l.1.clone()).unwrap_or_default();
            crate::theme::combo("nl_l").width(200.0).selected_text(curl).show_ui(ui, |ui| {
                for (id, n) in &labs {
                    ui.selectable_value(&mut st.1, *id, n);
                }
            });
            ui.label("Plazo");
            ui.add(egui::DragValue::new(&mut st.2).range(1..=60).suffix(" días"));
            if primary(ui, "Enviar al laboratorio").clicked() {
                if labs.is_empty() {
                    app.warn("Dé de alta un laboratorio en Almacén → Proveedores.");
                } else {
                    let id = app.db.new_lab_work(pid, st.0, st.1, st.2);
                    app.stock.lab_sel = Some(id);
                    app.changed();
                    app.notify("Trabajo registrado en el laboratorio.");
                    st.0 = None;
                }
            }
        });
        if pending.is_empty() {
            muted(ui, "Este paciente no tiene coronas, puentes, carillas o prótesis pendientes de enviar.");
        }
        ui.data_mut(|d| d.insert_temp(key, st));
    });
}

fn detail(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let Some(id) = app.stock.lab_sel else {
            section(ui, "Trabajo");
            muted(ui, "Seleccione un trabajo de la lista.");
            return;
        };
        let labs: Vec<(u32, String)> = app.db.suppliers.iter().filter(|s| s.is_lab).map(|s| (s.id, s.name.clone())).collect();
        let today = app.today;
        let mut set_state = None;
        let mut goto = None;
        let mut order = None;
        let mut settings = false;
        // las imágenes son datos clínicos (Recepción no las recibe)
        let clinical = app.can(crate::users::Perm::Clinico);
        let mut cost_changed = false;
        if let Some(w) = app.db.lab_works.iter_mut().find(|w| w.id == id) {
            let before = serde_json::to_string(w).unwrap_or_default();
            let cost_before = w.cost;
            section(ui, &w.description.clone());
            ui.horizontal(|ui| {
                pill(ui, w.state.label(), state_color(w.state));
                if w.overdue(today) {
                    pill(ui, &format!("Retrasado {} días", (today - w.due).num_days()), DANGER);
                }
            });
            ui.add_space(4.0);
            field(ui, "Descripción", &mut w.description, 260.0);
            ui.horizontal(|ui| {
                field(ui, "Color", &mut w.shade, 70.0);
                field(ui, "Material", &mut w.material, 170.0);
            });
            ui.horizontal(|ui| {
                ui.label("Laboratorio");
                let cur = labs.iter().find(|l| l.0 == w.lab_id).map(|l| l.1.clone()).unwrap_or_default();
                crate::theme::combo("lw_l").width(150.0).selected_text(cur).show_ui(ui, |ui| {
                    for (lid, n) in &labs {
                        ui.selectable_value(&mut w.lab_id, *lid, n);
                    }
                });
            });
            ui.horizontal(|ui| {
                lab(ui, 100.0, RichText::new("Enviado").color(MUTED));
                crate::screens::date_picker(ui, "lw_sent", &mut w.sent);
            });
            ui.horizontal(|ui| {
                lab(ui, 100.0, RichText::new("Prevista").color(MUTED));
                crate::screens::date_picker(ui, "lw_due", &mut w.due);
            });
            ui.horizontal(|ui| {
                ui.label("Coste");
                ui.add(egui::DragValue::new(&mut w.cost).range(0.0..=100000.0).speed(1.0).suffix(" €"));
            });
            ui.label(RichText::new("Indicaciones para el laboratorio").size(13.5).color(MUTED));
            ui.add(egui::TextEdit::multiline(&mut w.notes).desired_rows(3).desired_width(f32::INFINITY));
            ui.add_space(6.0);
            ui.label(RichText::new("Cambiar estado").size(13.5).color(MUTED));
            ui.horizontal_wrapped(|ui| {
                for s in LabState::ALL {
                    let sel = w.state == s;
                    let c = state_color(s);
                    if ui.add(egui::Button::new(RichText::new(s.label()).size(13.0).color(if sel { Color32::WHITE } else { c })).fill(if sel { c } else { tint(c, 0.1) }).corner_radius(egui::CornerRadius::same(12))).clicked() {
                        set_state = Some(s);
                    }
                }
            });
            ui.add_space(6.0);
            if ui.button("Ver ficha del paciente").clicked() {
                goto = Some(w.patient_id);
            }
            // ── Envío digital: orden con el esquema de las piezas y los archivos del paciente ──
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(RichText::new("Envío digital al laboratorio").family(sb()).size(15.0));
            match w.order_sent {
                Some(t) => {
                    ui.horizontal_wrapped(|ui| {
                        pill(ui, "Orden enviada", OK);
                        ui.label(RichText::new(format!("{} · {}", t.format("%d/%m/%Y %H:%M"), w.order_to)).size(12.5).color(MUTED));
                    });
                }
                None => muted(ui, "La orden aún no se ha enviado."),
            }
            // correo del laboratorio (se puede escribir aquí mismo)
            if let Some(s) = app.db.suppliers.iter_mut().find(|s| s.id == w.lab_id) {
                ui.horizontal(|ui| {
                    lab(ui, 100.0, RichText::new("Correo").color(MUTED));
                    if ui.add(egui::TextEdit::singleline(&mut s.email).hint_text("pedidos@laboratorio.es").desired_width(ui.available_width())).changed() {
                        app.dirty = true;
                    }
                });
            }
            ui.add_space(2.0);
            ui.label(RichText::new("Adjuntar de la galería del paciente").size(13.5).color(MUTED));
            if !clinical {
                muted(ui, format!("Las imágenes las adjunta un doctor desde su puesto (este puesto no recibe imágenes){}.", if w.attachments.is_empty() { String::new() } else { format!("; esta orden lleva {}", w.attachments.len()) }));
            }
            let imgs: Vec<&PatientImage> = app.db.images.iter().filter(|i| clinical && i.patient_id == w.patient_id).collect();
            if clinical && imgs.is_empty() {
                muted(ui, "El paciente no tiene escaneados, fotos ni radiografías en su galería.");
            }
            for img in imgs {
                let mut on = w.attachments.contains(&img.id);
                let teeth: Vec<String> = img.teeth.iter().map(|t| t.to_string()).collect();
                let folder = !img.demo && std::path::Path::new(&img.path).is_dir();
                let text = format!("{}{} · {}{}", img.kind.short(), if teeth.is_empty() { String::new() } else { format!(" {}", teeth.join("-")) }, fmt_date(img.date), if folder { " (carpeta)" } else { "" });
                if ui.checkbox(&mut on, RichText::new(text).size(13.0)).changed() {
                    if on {
                        w.attachments.push(img.id);
                    } else {
                        w.attachments.retain(|x| *x != img.id);
                    }
                }
            }
            ui.checkbox(&mut w.anonymize, RichText::new("Sin el nombre del paciente (iniciales y nº de historia)").size(13.0));
            ui.add_space(4.0);
            let sending = app.stock.lab_job.as_ref().map(|j| j.work == w.id).unwrap_or(false);
            if sending {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(16.0));
                    ui.label(RichText::new("Enviando la orden…").color(MUTED));
                });
            } else {
                let b = egui::Button::new(RichText::new("✉ Enviar por correo").color(Color32::WHITE).font(sbf(14.5))).fill(ACCENT).stroke(egui::Stroke::new(1.0, ACCENT_DARK)).corner_radius(egui::CornerRadius::same(10));
                if ui.add_sized([ui.available_width(), 38.0], b).clicked() {
                    order = Some(OrderAction::Send);
                }
            }
            ui.columns(2, |c| {
                let w2 = c[0].available_width();
                if c[0].add_sized([w2, 32.0], egui::Button::new("Ver orden")).clicked() {
                    order = Some(OrderAction::View);
                }
                if c[1].add_sized([w2, 32.0], egui::Button::new("Guardar ZIP")).clicked() {
                    order = Some(OrderAction::Zip);
                }
            });
            if !app.db.mail.ready() {
                ui.label(RichText::new("Sin correo configurado en el programa: se abrirá su programa de correo y se guardará el paquete para adjuntarlo.").size(12.0).color(MUTED));
                if ui.link(RichText::new("Configurar el correo ›").size(12.5)).clicked() {
                    settings = true;
                }
            }
            if let Some((_, ok, msg)) = app.stock.lab_msg.as_ref().filter(|m| m.0 == w.id) {
                ui.label(RichText::new(msg).size(12.5).color(if *ok { OK } else { DANGER }));
            }
            if serde_json::to_string(w).unwrap_or_default() != before {
                app.dirty = true;
            }
            cost_changed = (w.cost - cost_before).abs() > 1e-9;
        }
        // el coste que se escribe cuando ya está recibido va a Gastos
        if cost_changed {
            app.db.sync_lab_expense(id);
        }
        if let Some(s) = set_state {
            app.db.set_lab_state(id, s);
            app.changed();
        }
        if let Some(p) = goto {
            app.open_patient(p, PatTab::Citas);
        }
        if settings {
            crate::screens::config::open(app, crate::screens::config::CfgTab::Correo);
        }
        if let Some(a) = order {
            order_action(app, ui.ctx(), id, a);
        }
    });
}

#[derive(Clone, Copy)]
enum OrderAction {
    View,
    Zip,
    Send,
}

/// Envío por correo en segundo plano de una orden de laboratorio.
pub struct LabJob {
    pub work: u32,
    pub to: String,
    pub rx: std::sync::mpsc::Receiver<Result<(), String>>,
}

fn lab_dir() -> std::path::PathBuf {
    crate::paths::documents().join(crate::brand::DIR).join("laboratorio")
}

fn save_zip(name: &str, bytes: &[u8]) -> Result<std::path::PathBuf, String> {
    let dir = if cfg!(test) { std::env::temp_dir().join("veodardental-laboratorio") } else { lab_dir() };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| format!("No se pudo guardar {}: {e}", path.display()))?;
    Ok(path)
}

fn order_action(app: &mut App, ctx: &egui::Context, id: u32, a: OrderAction) {
    let Some(w) = app.db.lab_works.iter().find(|w| w.id == id).cloned() else { return };
    let to = app.db.suppliers.iter().find(|s| s.id == w.lab_id).map(|s| s.email.trim().to_string()).unwrap_or_default();
    match a {
        OrderAction::View => {
            if let Err(e) = crate::screens::centros::open_html(ctx, &format!("orden_laboratorio_{id}.html"), crate::lab_order::order_html(&app.db, &w)) {
                app.warn(e);
            }
        }
        OrderAction::Zip | OrderAction::Send => {
            // las imágenes no están en este puesto: la orden saldría sin ellas
            if !w.attachments.is_empty() && !app.can(crate::users::Perm::Clinico) {
                app.stock.lab_msg = Some((id, false, format!("Esta orden lleva {} imágenes, que este puesto no recibe: prepárela o envíela desde el puesto de un doctor.", w.attachments.len())));
                return;
            }
            let (zip, warnings) = match crate::lab_order::package(&app.db, &w) {
                Ok(x) => x,
                Err(e) => {
                    app.stock.lab_msg = Some((id, false, format!("No se pudo preparar el paquete: {e}")));
                    return;
                }
            };
            let note = if warnings.is_empty() { String::new() } else { format!(" Sin incluir: {}", warnings.join(" ")) };
            let name = crate::lab_order::package_name(&app.db, &w);
            let send = matches!(a, OrderAction::Send);
            if send && to.is_empty() {
                app.stock.lab_msg = Some((id, false, "Escriba el correo del laboratorio para enviarle la orden.".into()));
                return;
            }
            if send && app.db.mail.ready() {
                let cfg = app.db.mail.clone();
                let (subject, body) = crate::lab_order::mail_text(&app.db, &w);
                let files = vec![crate::mail::FileAttachment { name, mime: "application/zip".into(), bytes: zip }];
                let (tx, rx) = std::sync::mpsc::channel();
                let dest = to.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::mail::send(&cfg, &dest, &subject, &body, &files));
                });
                app.stock.lab_job = Some(LabJob { work: id, to, rx });
                app.stock.lab_msg = if note.is_empty() { None } else { Some((id, true, note.trim().to_string())) };
                ctx.request_repaint();
                return;
            }
            match save_zip(&name, &zip) {
                Err(e) => app.stock.lab_msg = Some((id, false, e)),
                Ok(path) => {
                    if !cfg!(test) {
                        let _ = crate::screens::imagenes::open_folder(path.parent().unwrap_or(&path));
                    }
                    if send {
                        // sin servidor de correo: se abre el programa de correo del equipo y se adjunta el paquete a mano
                        let (subject, body) = crate::lab_order::mail_text(&app.db, &w);
                        let body = format!("{body}\n\n(Adjunte el fichero {name})");
                        let url = format!("mailto:{to}?subject={}&body={}", crate::screens::comunicaciones::enc_pub(&subject), crate::screens::comunicaciones::enc_pub(&body));
                        ctx.open_url(egui::OpenUrl::new_tab(url));
                        app.stock.lab_msg = Some((id, true, format!("Paquete guardado en {}: adjúntelo al correo que se ha abierto.{note}", path.display())));
                    } else {
                        app.stock.lab_msg = Some((id, true, format!("Paquete guardado en {}.{note}", path.display())));
                    }
                }
            }
        }
    }
}

/// Recoge el resultado de un envío por correo (aunque se haya cambiado de pantalla).
pub fn poll(app: &mut App, ctx: &egui::Context) {
    let Some(job) = app.stock.lab_job.as_ref() else { return };
    match job.rx.try_recv() {
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            let id = job.work;
            app.stock.lab_job = None;
            app.stock.lab_msg = Some((id, false, "El envío se interrumpió.".into()));
        }
        Ok(r) => {
            let (id, to) = (job.work, job.to.clone());
            app.stock.lab_job = None;
            match r {
                Ok(()) => {
                    let mut pending = false;
                    if let Some(w) = app.db.lab_works.iter_mut().find(|w| w.id == id) {
                        w.order_sent = Some(chrono::Local::now().naive_local());
                        w.order_to = to.clone();
                        pending = w.state == LabState::Pendiente;
                    }
                    if pending {
                        app.db.set_lab_state(id, LabState::Enviado);
                    }
                    let keep = app.stock.lab_msg.take().filter(|m| m.0 == id).map(|m| format!(" {}", m.2)).unwrap_or_default();
                    app.stock.lab_msg = Some((id, true, format!("Orden enviada a {to}.{keep}")));
                    app.changed();
                    app.notify(format!("Orden de trabajo enviada a {to}."));
                }
                Err(e) => app.stock.lab_msg = Some((id, false, e)),
            }
        }
    }
}

/// Bloque en la ficha del paciente (pestaña Citas).
pub fn patient_block(app: &mut App, ui: &mut Ui, pid: u32) {
    let list: Vec<LabWork> = app.db.lab_works.iter().filter(|w| w.patient_id == pid).cloned().collect();
    if list.is_empty() && app.db.prosthetic_without_lab(pid).is_empty() {
        return;
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Trabajos de laboratorio");
            right_aligned(ui, |ui| {
                if ui.button("Abrir laboratorio").clicked() {
                    app.screen = crate::app::Screen::Laboratorio;
                }
            });
        });
        for w in &list {
            ui.horizontal(|ui| {
                pill(ui, w.state.label(), state_color(w.state));
                ui.label(format!("{} · {} · prevista {}", w.description, app.db.supplier_name(w.lab_id), fmt_date(w.due)));
                if w.overdue(app.today) {
                    pill(ui, "Retrasado", DANGER);
                }
            });
        }
        let n = app.db.prosthetic_without_lab(pid).len();
        if n > 0 {
            muted(ui, format!("{n} tratamientos protésicos aún sin enviar al laboratorio."));
        }
    });
}
