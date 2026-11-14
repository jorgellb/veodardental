//! Autorizaciones de la compañía en la cuenta del paciente asegurado, y el
//! envío y la respuesta de las liquidaciones en Mutuas (ver
//! `crate::autorizaciones` e `insurer`).

use crate::app::{App, ChainOp};
use crate::autorizaciones::AuthState;
use crate::model::TxState;
use crate::theme::*;
use egui::{RichText, Ui};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct AuthUi {
    pub chosen: BTreeSet<u32>,
    pub channel: String,
    pub ext_ref: String,
    pub number: String,
    pub valid_days: u32,
    pub reason: String,
    pub msg: Option<(bool, String)>,
    /// Respuesta de una liquidación: (liquidación, actos rechazados con su motivo).
    pub claim: Option<u32>,
    pub rejected: BTreeMap<u32, String>,
    pub claim_raw: String,
}

/// Autorizaciones de la compañía en la cuenta del paciente.
pub fn patient_card(app: &mut App, ui: &mut Ui, pid: u32) {
    let Some(p) = app.db.patient(pid).cloned() else { return };
    if !app.db.tariff(p.tariff_id).is_some_and(|t| t.is_insurer) {
        return;
    }
    let today = app.today;
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, &format!("Autorizaciones de {}", app.db.tariff_name(p.tariff_id)));
        muted(ui, "Se piden por el portal o el correo de la compañía; aquí se apunta cada paso y lo que responde. Un tratamiento va en una sola solicitud, y el número de autorización sale en la liquidación.");
        // tratamientos previstos sin autorización pedida
        let free: Vec<(u32, String)> = app.db.entries.iter().filter(|e| e.patient_id == pid && e.state == TxState::Previsto && app.db.auth_for_entry(e.id).is_none()).map(|e| (e.id, format!("{} {}", e.code, e.description))).collect();
        if !free.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for (id, label) in &free {
                    let mut on = app.auth_ui.chosen.contains(id);
                    if ui.checkbox(&mut on, label).changed() {
                        if on {
                            app.auth_ui.chosen.insert(*id);
                        } else {
                            app.auth_ui.chosen.remove(id);
                        }
                    }
                }
            });
            if ui.button("Pedir autorización").clicked() {
                let entries: Vec<u32> = app.auth_ui.chosen.iter().copied().filter(|e| free.iter().any(|(id, _)| id == e)).collect();
                let r = app.chain_op(crate::app::ChainOp::AuthRequest { patient: pid, entries });
                app.auth_ui.msg = Some(match r {
                    Ok(_) => (true, "Solicitud creada: envíela por el portal de la compañía y apúntelo.".into()),
                    Err(e) => (false, e),
                });
                app.auth_ui.chosen.clear();
            }
        }
        if let Some((ok, m)) = &app.auth_ui.msg {
            ui.label(RichText::new(m).color(if *ok { OK } else { WARN }));
        }
        let list: Vec<crate::autorizaciones::AuthRequest> = app.db.auth_requests.iter().rev().filter(|r| r.patient_id == pid).take(6).cloned().collect();
        for r in list {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let shown = r.shown(today);
                pill(
                    ui,
                    shown,
                    if shown == "Aprobada" {
                        OK
                    } else if matches!(shown, "Denegada" | "Anulada" | "Caducada") {
                        MUTED()
                    } else {
                        INFO
                    },
                );
                ui.label(RichText::new(&r.reference).family(sb()));
                ui.label(r.lines.iter().map(|l| format!("{} {}", l.code, l.description)).collect::<Vec<_>>().join(" · "));
                if !r.number.is_empty() {
                    ui.label(RichText::new(format!("nº {}", r.number)).color(MUTED()));
                }
                if let Some(d) = r.valid_until {
                    ui.label(RichText::new(format!("válida hasta el {}", d.format("%d/%m/%Y"))).color(MUTED()));
                }
            });
            ui.horizontal_wrapped(|ui| match r.state {
                AuthState::Borrador => {
                    ui.add(egui::TextEdit::singleline(&mut app.auth_ui.channel).desired_width(150.0).hint_text("Portal, correo…"));
                    ui.add(egui::TextEdit::singleline(&mut app.auth_ui.ext_ref).desired_width(120.0).hint_text("Referencia"));
                    if ui.button("Enviada").clicked() {
                        let (c, e) = (app.auth_ui.channel.clone(), app.auth_ui.ext_ref.clone());
                        app.auth_ui.msg = app.chain_op(ChainOp::AuthSent { req: r.id, channel: c, external_ref: e }).err().map(|e| (false, e));
                    }
                }
                AuthState::Enviada | AuthState::Aprobada | AuthState::Parcial => {
                    ui.add(egui::TextEdit::singleline(&mut app.auth_ui.number).desired_width(120.0).hint_text("Nº de autorización"));
                    ui.add(egui::DragValue::new(&mut app.auth_ui.valid_days).range(0..=730).prefix("válida ").suffix(" días"));
                    if ui.button("Autorizada").clicked() {
                        let (n, days) = (app.auth_ui.number.clone(), app.auth_ui.valid_days);
                        let valid = (days > 0).then(|| today + chrono::Duration::days(days as i64));
                        app.auth_ui.msg = app.chain_op(ChainOp::AuthAnswer { req: r.id, approved: true, number: n, denied: vec![], valid_until: valid, note: String::new(), raw: String::new() }).err().map(|e| (false, e));
                    }
                    if ui.button("Denegada").clicked() {
                        app.auth_ui.msg = app.chain_op(ChainOp::AuthAnswer { req: r.id, approved: false, number: String::new(), denied: vec![], valid_until: None, note: String::new(), raw: String::new() }).err().map(|e| (false, e));
                    }
                }
                _ => {}
            });
            if r.state.live() {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut app.auth_ui.reason).desired_width(180.0).hint_text("Motivo para anular"));
                    if ui.button("Anular").clicked() {
                        let why = app.auth_ui.reason.clone();
                        app.auth_ui.msg = app.chain_op(ChainOp::AuthCancel { req: r.id, reason: why }).err().map(|e| (false, e));
                    }
                });
            }
        }
    });
}

/// Mutuas: apuntar el envío de una liquidación y la respuesta de la compañía
/// (los actos que rechaza, con su motivo).
pub fn claim_controls(app: &mut App, ui: &mut Ui, c: &crate::insurer::Claim) {
    if c.cancelled.is_some() {
        return;
    }
    if c.sent.is_empty() {
        if ui.button("Enviada").on_hover_text("Apunte que se ha enviado a la compañía (por su portal o por correo)").clicked() {
            let r = app.chain_op(ChainOp::ClaimSent { claim: c.id, channel: "Portal o fichero de la compañía".into(), external_ref: String::new() });
            if let Err(e) = r {
                app.warn(e);
            }
        }
    } else if ui.button("Respuesta…").on_hover_text("Apunte los actos que rechaza la compañía").clicked() {
        app.auth_ui.claim = if app.auth_ui.claim == Some(c.id) { None } else { Some(c.id) };
        app.auth_ui.rejected.clear();
    }
}

/// Debajo de la lista de liquidaciones: la respuesta que se está apuntando.
pub fn claim_answer_panel(app: &mut App, ui: &mut Ui) {
    let Some(id) = app.auth_ui.claim else { return };
    let Some(c) = app.db.claims.iter().find(|c| c.id == id).cloned() else { return };
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, &format!("Respuesta de la compañía a {}", c.number));
        for l in c.lines.iter().filter(|l| l.rejected.is_none()) {
            ui.horizontal_wrapped(|ui| {
                let mut on = app.auth_ui.rejected.contains_key(&l.entry_id);
                if ui.checkbox(&mut on, format!("{} · {} · {} {}", l.date.format("%d/%m/%Y"), l.patient, l.code, l.description)).changed() {
                    if on {
                        app.auth_ui.rejected.insert(l.entry_id, String::new());
                    } else {
                        app.auth_ui.rejected.remove(&l.entry_id);
                    }
                }
                if let Some(why) = app.auth_ui.rejected.get_mut(&l.entry_id) {
                    ui.add(egui::TextEdit::singleline(why).desired_width(220.0).hint_text("Motivo del rechazo"));
                }
            });
        }
        for l in c.lines.iter().filter(|l| l.rejected.is_some()) {
            muted(ui, format!("Rechazado: {} {} · {}", l.code, l.description, l.rejected.clone().unwrap_or_default()));
        }
        ui.add(egui::TextEdit::multiline(&mut app.auth_ui.claim_raw).desired_rows(2).desired_width(f32::INFINITY).hint_text("Lo que ha respondido la compañía (se guarda tal cual)"));
        if ui.button("Guardar la respuesta").clicked() {
            let rejected: Vec<(u32, String)> = app.auth_ui.rejected.iter().map(|(k, v)| (*k, v.clone())).collect();
            let raw = app.auth_ui.claim_raw.clone();
            match app.chain_op(ChainOp::ClaimAnswer { claim: id, rejected, raw }) {
                Ok(_) => {
                    let expected = app.db.claims.iter().find(|c| c.id == id).map(|c| c.expected()).unwrap_or_default();
                    app.notify(format!("Respuesta guardada. La compañía debe pagar {}.", crate::model::money(expected)));
                    app.auth_ui.claim = None;
                    app.auth_ui.rejected.clear();
                    app.auth_ui.claim_raw.clear();
                }
                Err(e) => app.warn(e),
            }
        }
    });
}
