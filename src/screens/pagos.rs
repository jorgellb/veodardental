//! Pago por internet en las pantallas (ver `crate::pago`): pedirlo desde el
//! presupuesto, la cuota o la cita; la lista en Cobros con lo que hay que
//! hacer (intentos sin confirmar, devoluciones), y la configuración.

use crate::app::App;
use crate::pago::{PayFor, PayRequest, PayState, RefundOutcome, TpvSettings};
use crate::theme::*;
use egui::{RichText, Ui};
use std::sync::mpsc::Receiver;

/// Devolución en curso: (petición, pedido, céntimos, resultado).
pub type RefundJob = (u32, String, i64, Receiver<RefundOutcome>);

fn link(app: &App, r: &PayRequest) -> String {
    format!("{}/pagar/{}", app.db.booking.public_url.trim().trim_end_matches('/'), r.token)
}

fn state_pill(ui: &mut Ui, r: &PayRequest, now: chrono::NaiveDateTime) {
    let (t, c) = match r.state {
        PayState::Pendiente if !r.uncertain(now).is_empty() => ("Sin confirmar: compruébelo", WARN),
        PayState::Pendiente => ("Pendiente de pago", INFO),
        PayState::Pagado if r.refund_due => ("Pagado · hay que devolver", DANGER),
        PayState::Pagado => ("Pagado", OK),
        PayState::Retenido => ("Señal retenida", MUTED),
        PayState::Devuelto => ("Devuelto", MUTED),
        PayState::Anulado => ("Anulado", MUTED),
    };
    pill(ui, t, c);
}

/// Fila «Pago por internet» de un presupuesto, una cuota o una cita.
pub fn pay_row(app: &mut App, ui: &mut Ui, what: PayFor, label: &str) {
    if !app.db.config.pay_online.enabled {
        return;
    }
    let now = crate::model::now();
    let req = app.db.pay_requests.iter().rev().find(|r| r.what == what && r.state != PayState::Anulado).cloned();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(label).font(sbf(13.0)).color(HEADING));
        match &req {
            Some(r) => {
                state_pill(ui, r, now);
                ui.label(RichText::new(crate::cents::Cents(r.amount).to_string()).size(12.5));
                if r.state == PayState::Pendiente && ui.small_button("Copiar enlace").on_hover_text("Para enviárselo al paciente por WhatsApp o correo").clicked() {
                    ui.ctx().copy_text(link(app, r));
                    app.notify("Enlace de pago copiado.");
                }
            }
            None => {
                if ui.small_button("Pedir pago por internet").clicked() {
                    match app.db.create_pay_request(what, None, now) {
                        Ok(id) => {
                            if let Some(r) = app.db.pay_request(id) {
                                ui.ctx().copy_text(link(app, r));
                            }
                            app.changed();
                            app.notify("Pago pedido: el enlace está copiado para enviárselo al paciente.");
                        }
                        Err(e) => app.warn(e),
                    }
                }
            }
        }
    });
}

/// Cobros › Pagos por internet.
pub fn panel(app: &mut App, ui: &mut Ui) {
    let now = crate::model::now();
    // devolución terminada
    if let Some((id, order, amount, rx)) = app.booking.refund_job.take() {
        match rx.try_recv() {
            Ok(out) => {
                match app.db.record_refund(id, &order, amount, &out) {
                    Ok(m) => app.notify(m),
                    Err(e) => app.warn(e),
                }
                app.changed();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => app.booking.refund_job = Some((id, order, amount, rx)),
            Err(_) => app.warn("La devolución se interrumpió: compruébela en el portal del TPV."),
        }
    }
    title(ui, "Pagos por internet", "Presupuestos, cuotas y señales pagados con tarjeta o Bizum en la página del banco");
    if !app.db.config.pay_online.enabled {
        muted(ui, "El pago por internet está desactivado. Se activa en Configuración › Cita online, con los datos del TPV virtual que da el banco.");
        return;
    }
    let mut list: Vec<PayRequest> = app.db.pay_requests.iter().cloned().collect();
    list.sort_by_key(|r| std::cmp::Reverse(r.created));
    let todo = list.iter().filter(|r| (r.state == PayState::Pendiente && !r.uncertain(now).is_empty()) || r.refund_due || r.charges.iter().any(|c| c.refund_uncertain) || !r.note.is_empty() && r.state == PayState::Pendiente).count();
    if todo > 0 {
        ui.label(RichText::new(format!("{todo} pago(s) necesitan que alguien los revise.")).color(WARN));
    }
    let tbl = Table::new("pagos_web", vec![("Fecha", 110.0), ("Paciente", 200.0), ("Concepto", 280.0), ("Importe", 90.0), ("Estado", 190.0)]);
    tbl.header(ui);
    for (i, r) in list.iter().take(200).enumerate() {
        let (st, col) = match r.state {
            PayState::Pendiente if !r.uncertain(now).is_empty() => ("Sin confirmar", WARN),
            PayState::Pagado if r.refund_due => ("Hay que devolver", DANGER),
            s => (s.label(), if s == PayState::Pagado { OK } else { MUTED }),
        };
        let sel = app.booking.pay_sel == Some(r.id);
        if tbl.row(ui, i, sel, &[cell(r.created.format("%d/%m/%Y").to_string()), cell(app.db.patient_name(r.patient_id)), cell(&r.concept), cell(crate::cents::Cents(r.amount).to_string()), cellc(st, col)]).clicked() {
            app.booking.pay_sel = if sel { None } else { Some(r.id) };
        }
    }
    if list.is_empty() {
        muted(ui, "Aún no se ha pedido ningún pago por internet.");
    }
    if let Some(r) = app.booking.pay_sel.and_then(|id| list.iter().find(|r| r.id == id)).cloned() {
        ui.add_space(8.0);
        detail(app, ui, &r, now);
    }
}

fn detail(app: &mut App, ui: &mut Ui, r: &PayRequest, now: chrono::NaiveDateTime) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, &r.concept);
        ui.horizontal_wrapped(|ui| {
            state_pill(ui, r, now);
            ui.label(format!("{} · pedido el {} por {}", crate::cents::Cents(r.amount), r.created.format("%d/%m/%Y %H:%M"), if r.created_by.is_empty() { "—" } else { &r.created_by }));
        });
        if !r.note.is_empty() {
            ui.label(RichText::new(&r.note).color(WARN));
        }
        if r.state == PayState::Pendiente {
            ui.horizontal(|ui| {
                if ui.button("Copiar enlace").clicked() {
                    ui.ctx().copy_text(link(app, r));
                    app.notify("Enlace de pago copiado.");
                }
                if ui.button("Anular la petición").clicked() {
                    match app.db.cancel_pay_request(r.id) {
                        Ok(()) => {
                            app.changed();
                            app.notify("Petición de pago anulada.");
                        }
                        Err(e) => app.warn(e),
                    }
                }
            });
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Intentos de pago").font(sbf(13.0)));
        if r.attempts.is_empty() {
            muted(ui, "El paciente aún no ha intentado pagar.");
        }
        for a in &r.attempts {
            ui.horizontal_wrapped(|ui| {
                let res = match a.response {
                    Some(c) if c <= 99 => "autorizado".to_string(),
                    Some(c) => format!("no autorizado ({c:04})"),
                    None if !a.note.is_empty() => a.note.clone(),
                    None => "sin respuesta del banco".to_string(),
                };
                ui.label(format!("{} · pedido {} · {} · {res}", a.at.format("%d/%m %H:%M"), a.order, a.method.label()));
                let open = a.response.is_none() && a.note.is_empty() && (now - a.at).num_minutes() >= crate::pago::UNCERTAIN_MINUTES;
                if open {
                    muted(ui, "Compruébelo en el portal del TPV:");
                    for (charged, label) in [(true, "Se cobró"), (false, "No se cobró")] {
                        if ui.small_button(label).clicked() {
                            match app.db.settle_by_hand(r.id, &a.order, charged, now) {
                                Ok(_) => {
                                    app.changed();
                                    app.notify("Intento de pago resuelto.");
                                }
                                Err(e) => app.warn(e),
                            }
                        }
                    }
                }
            });
        }
        if !r.charges.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new("Cobros y devoluciones").font(sbf(13.0)));
        }
        let can = app.can(crate::users::Perm::Configuracion);
        for c in &r.charges {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Pedido {} · {} · {}{}", c.order, c.method.label(), crate::cents::Cents(c.amount), if c.refunded > 0 { format!(" · devuelto {}", crate::cents::Cents(c.refunded)) } else { String::new() }));
                if c.refund_uncertain {
                    ui.label(RichText::new("Devolución sin respuesta clara: compruébela en el portal del TPV.").color(WARN));
                    for (done, label) in [(true, "Se devolvió"), (false, "No se devolvió")] {
                        if can && ui.small_button(label).clicked() {
                            match app.db.settle_refund_by_hand(r.id, &c.order, c.amount - c.refunded, done) {
                                Ok(m) => app.notify(m),
                                Err(e) => app.warn(e),
                            }
                            app.changed();
                        }
                    }
                } else if c.amount > c.refunded && can && app.booking.refund_job.is_none() && ui.small_button("Devolver…").on_hover_text("Devuelve el importe por el TPV virtual (solo Dirección)").clicked() {
                    app.booking.refund_ask = Some((r.id, c.order.clone(), (c.amount - c.refunded) as f64 / 100.0));
                }
            });
        }
        if app.booking.refund_job.is_some() {
            muted(ui, "Pidiendo la devolución al banco…");
        }
        if let Some((id, order, mut eur)) = app.booking.refund_ask.clone()
            && id == r.id
        {
            ui.horizontal(|ui| {
                ui.label(format!("Devolver del pedido {order}:"));
                ui.add(egui::DragValue::new(&mut eur).range(0.01..=100_000.0).speed(0.1).suffix(" €"));
                app.booking.refund_ask = Some((id, order.clone(), eur));
                if danger(ui, "Devolver por el TPV").clicked() {
                    let cents = crate::cents::Cents::from_eur(eur).0;
                    match app.db.refundable(id, &order) {
                        Ok(max) if cents > 0 && cents <= max => {
                            let s = app.booking.tpv.get_or_insert_with(crate::pago::load_settings).clone();
                            let (tx, rx) = std::sync::mpsc::channel();
                            let o = order.clone();
                            std::thread::spawn(move || {
                                let _ = tx.send(crate::pago::send_refund(&s, &o, cents));
                            });
                            app.booking.refund_job = Some((id, order.clone(), cents, rx));
                            app.booking.refund_ask = None;
                        }
                        Ok(max) => app.warn(format!("Se puede devolver hasta {}.", crate::cents::Cents(max))),
                        Err(e) => app.warn(e),
                    }
                }
                if ui.button("Cancelar").clicked() {
                    app.booking.refund_ask = None;
                }
            });
        }
    });
}

/// Configuración del pago por internet: la de la clínica y el TPV de este ordenador.
pub fn settings(app: &mut App, ui: &mut Ui) {
    let before = app.db.config.pay_online.clone();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Pago por internet (TPV virtual y Bizum)");
        muted(ui, "El paciente paga en la página segura del banco (Redsys): la clínica no ve ni guarda datos de tarjetas. Hace falta la página de citas publicada en internet y el TPV virtual que da el banco.");
        let c = &mut app.db.config.pay_online;
        ui.checkbox(&mut c.enabled, "Pedir pagos por internet (presupuestos, cuotas y señales)");
        ui.checkbox(&mut c.bizum, "Ofrecer también Bizum (si el banco lo tiene activado en el TPV)");
        ui.horizontal(|ui| {
            ui.label("Señal de reserva de una cita");
            let mut eur = c.deposit.amount as f64 / 100.0;
            if ui.add(egui::DragValue::new(&mut eur).range(0.0..=1000.0).speed(1.0).suffix(" €")).changed() {
                c.deposit.amount = crate::cents::Cents::from_eur(eur).0;
            }
            ui.label("se devuelve si se anula con al menos");
            ui.add(egui::DragValue::new(&mut c.deposit.refund_hours).range(0..=720).suffix(" h"));
        });
        if c.deposit.amount > 0 {
            ui.label(RichText::new(format!("Lo que verá el paciente: «{}»", c.deposit.text())).size(12.5).color(MUTED));
        }
    });
    if app.db.config.pay_online != before {
        app.changed();
    }
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "TPV virtual de este ordenador");
        muted(ui, "Los datos que da el banco. Se guardan solo en este ordenador (el que publica la página de citas), nunca en los datos de la clínica.");
        let s = app.booking.tpv.get_or_insert_with(crate::pago::load_settings);
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Entorno");
            for e in [crate::pago::Env::Pruebas, crate::pago::Env::Real] {
                changed |= ui.selectable_value(&mut s.env, e, e.label()).changed();
            }
        });
        egui::Grid::new("tpv_cfg").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Número de comercio (FUC)");
            changed |= ui.add(egui::TextEdit::singleline(&mut s.merchant).desired_width(140.0)).changed();
            ui.end_row();
            ui.label("Terminal");
            changed |= ui.add(egui::TextEdit::singleline(&mut s.terminal).desired_width(60.0)).changed();
            ui.end_row();
            ui.label("Clave de firma");
            changed |= ui.add(egui::TextEdit::singleline(&mut s.key).password(true).desired_width(320.0)).changed();
            ui.end_row();
        });
        let problem = s.problem();
        let s2: TpvSettings = s.clone();
        if let Some(p) = problem {
            ui.label(RichText::new(p).color(WARN));
        } else if s2.env == crate::pago::Env::Real {
            ui.label(RichText::new("Entorno real: los pagos son cargos de verdad.").color(DANGER));
        }
        if changed && let Err(e) = crate::pago::save_settings(&s2) {
            app.warn(format!("No se pudo guardar el TPV: {e}"));
        }
    });
}
