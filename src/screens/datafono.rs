//! Datáfono y financiación en la ficha del paciente (Cuenta) y los ajustes
//! del datáfono (ver `crate::datafono` y `crate::financiacion`).

use crate::app::App;
use crate::cents::Cents;
use crate::datafono::{CardKind, CardState, Reply};
use crate::financiacion::FinState;
use crate::theme::*;
use egui::{RichText, Ui};

type Job = std::sync::mpsc::Receiver<(u32, Result<Reply, String>)>;

#[derive(Default)]
pub struct CardUi {
    pub amount: String,
    pub auth: String,
    pub refund: String,
    job: Option<Job>,
    pub msg: Option<(bool, String)>,
    settings: Option<crate::datafono::Settings>,
    // financiación
    pub fin_provider: String,
    pub fin_amount: String,
    pub fin_months: u32,
    pub fin_ref: String,
    pub fin_received: String,
    pub fin_reason: String,
}

fn parse_eur(s: &str) -> Option<Cents> {
    s.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0).map(Cents::from_eur)
}

fn settings(app: &mut App) -> crate::datafono::Settings {
    app.card.settings.get_or_insert_with(crate::datafono::load_settings).clone()
}

/// Envía, pregunta o cancela en segundo plano (no bloquea la pantalla).
fn spawn(app: &mut App, id: u32, what: &'static str) {
    let Some(tx) = app.db.card_txs.iter().find(|t| t.id == id).cloned() else { return };
    let original = tx.original.and_then(|o| app.db.card_txs.iter().find(|t| t.id == o).cloned());
    let term = settings(app).terminal();
    let (s, r) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = match what {
            "send" => term.send(&tx, original.as_ref()),
            "query" => term.query(&tx),
            _ => term.cancel(&tx),
        };
        let _ = s.send((tx.id, out));
    });
    app.card.job = Some(r);
}

/// Lo que ha respondido el datáfono (si ya respondió).
fn poll(app: &mut App) {
    if let Some(rx) = &app.card.job
        && let Ok((id, r)) = rx.try_recv()
    {
        app.card.job = None;
        match app.db.card_apply(id, r, crate::model::now()) {
            Ok(s) => app.card.msg = Some((s != CardState::Desconocida, format!("Datáfono: {}", s.label()))),
            Err(e) => app.card.msg = Some((false, e)),
        }
        app.changed();
    }
}

fn start(app: &mut App, pid: u32, kind: CardKind, amount: Cents, original: Option<u32>) {
    let term = settings(app).terminal();
    let now = crate::model::now();
    match app.db.card_new(pid, kind, amount, original, term.name(), now).and_then(|id| app.db.card_sending(id, now).map(|_| id)) {
        Ok(id) => {
            app.changed();
            spawn(app, id, "send");
        }
        Err(e) => app.card.msg = Some((false, e)),
    }
}

/// Datáfono en la cuenta del paciente.
pub fn patient_card(app: &mut App, ui: &mut Ui, pid: u32) {
    poll(app);
    if app.card.job.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }
    let manual = settings(app).url.trim().is_empty();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Datáfono");
        muted(ui, if manual { "Datáfono sin conectar: cobre en el datáfono y apunte aquí el resultado con el número de autorización del justificante." } else { "Datáfono conectado: el cobro se apunta solo cuando el datáfono lo aprueba." });
        ui.horizontal(|ui| {
            ui.label("Importe");
            ui.add(egui::TextEdit::singleline(&mut app.card.amount).desired_width(90.0).hint_text("0,00"));
            let busy = app.card.job.is_some();
            if ui.add_enabled(!busy, egui::Button::new("Cobrar con datáfono")).clicked() {
                match parse_eur(&app.card.amount) {
                    Some(a) => {
                        start(app, pid, CardKind::Venta, a, None);
                        app.card.amount.clear();
                    }
                    None => app.card.msg = Some((false, "Escriba el importe.".into())),
                }
            }
            if busy {
                ui.spinner();
            }
        });
        if let Some((ok, m)) = &app.card.msg {
            ui.label(RichText::new(m).color(if *ok { OK } else { WARN }));
        }
        // sin respuesta del datáfono: no se sabe si se cobró hasta preguntar
        let unknown = app.db.card_to_check(crate::model::now()).iter().filter(|id| app.db.card_txs.iter().any(|t| t.id == **id && t.patient_id == pid)).count();
        if unknown > 0 {
            ui.label(RichText::new(format!("{unknown} operación(es) sin respuesta del datáfono: pulse «Comprobar» antes de volver a cobrar.")).color(WARN));
        }
        let txs: Vec<crate::datafono::CardTx> = app.db.card_txs.iter().rev().filter(|t| t.patient_id == pid).take(8).cloned().collect();
        for t in txs {
            ui.horizontal_wrapped(|ui| {
                let col = match t.state {
                    CardState::Aprobada => OK,
                    CardState::Denegada | CardState::Cancelada => MUTED,
                    CardState::Desconocida => WARN,
                    _ => INFO,
                };
                pill(ui, t.state.label(), col);
                ui.label(format!("{} {} · {} · {}", if t.kind == CardKind::Venta { "Cobro" } else { "Devolución" }, Cents(t.amount), t.created.format("%d/%m %H:%M"), t.reference));
                if !t.card.is_empty() {
                    ui.label(RichText::new(&t.card).color(MUTED));
                }
                let busy = app.card.job.is_some();
                match (t.state, t.provider.as_str()) {
                    (CardState::EnDatafono, "manual") => {
                        ui.add(egui::TextEdit::singleline(&mut app.card.auth).desired_width(90.0).hint_text("Autorización"));
                        if ui.button("Aprobada").clicked() {
                            let r = app.db.card_manual_result(t.id, true, &app.card.auth, crate::model::now());
                            app.card.msg = Some(match r {
                                Ok(s) => (true, format!("Datáfono: {}", s.label())),
                                Err(e) => (false, e),
                            });
                            app.card.auth.clear();
                            app.changed();
                        }
                        if ui.button("No pasó").clicked() {
                            let _ = app.db.card_manual_result(t.id, false, "", crate::model::now());
                            app.changed();
                        }
                    }
                    (CardState::EnDatafono | CardState::Desconocida, _) => {
                        if ui.add_enabled(!busy, egui::Button::new("Comprobar")).clicked() {
                            spawn(app, t.id, "query");
                        }
                        if ui.add_enabled(!busy, egui::Button::new("Cancelar")).clicked() {
                            spawn(app, t.id, "cancel");
                        }
                    }
                    (CardState::Aprobada, _) if t.kind == CardKind::Venta => {
                        ui.add(egui::TextEdit::singleline(&mut app.card.refund).desired_width(80.0).hint_text("Devolver"));
                        if ui.add_enabled(!busy, egui::Button::new("Devolver")).clicked() {
                            match parse_eur(&app.card.refund) {
                                Some(a) => {
                                    start(app, pid, CardKind::Devolucion, a, Some(t.id));
                                    app.card.refund.clear();
                                }
                                None => app.card.msg = Some((false, "Escriba el importe que se devuelve.".into())),
                            }
                        }
                    }
                    _ => {}
                }
            });
        }
    });
    ui.add_space(8.0);
    financing_card(app, ui, pid);
}

/// Financiación en la cuenta del paciente.
fn financing_card(app: &mut App, ui: &mut Ui, pid: u32) {
    let now = crate::model::now();
    let today = app.today;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Financiación con una entidad");
        muted(ui, "Se pide en el portal de la entidad y aquí se apunta cada paso. Al pagar la entidad, el paciente queda pagado por el total y lo que no llega es la comisión de la clínica.");
        if app.card.fin_months == 0 {
            app.card.fin_months = 12;
        }
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut app.card.fin_provider).desired_width(160.0).hint_text("Entidad"));
            ui.add(egui::TextEdit::singleline(&mut app.card.fin_amount).desired_width(90.0).hint_text("Importe"));
            ui.add(egui::DragValue::new(&mut app.card.fin_months).range(1..=120).suffix(" meses"));
            if ui.button("Apuntar solicitud").clicked() {
                let r = match parse_eur(&app.card.fin_amount) {
                    Some(a) => app.db.fin_new(pid, None, &app.card.fin_provider, a, app.card.fin_months, now),
                    None => Err("Escriba el importe financiado.".into()),
                };
                match r {
                    Ok(_) => {
                        app.card.fin_amount.clear();
                        app.card.msg = None;
                        app.changed();
                    }
                    Err(e) => app.card.msg = Some((false, e)),
                }
            }
        });
        let list: Vec<crate::financiacion::Financing> = app.db.financings.iter().rev().filter(|f| f.patient_id == pid).take(6).cloned().collect();
        for f in list {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                pill(
                    ui,
                    f.state.label(),
                    if matches!(f.state, FinState::Pagada) {
                        OK
                    } else if matches!(f.state, FinState::Denegada | FinState::Cancelada) {
                        MUTED
                    } else {
                        INFO
                    },
                );
                ui.label(format!("{} · {} en {} meses {}", f.provider, Cents(f.amount), f.months, f.provider_ref));
                if let Some((d, r)) = f.paid {
                    ui.label(RichText::new(format!("pagó {} el {}", Cents(r), d.format("%d/%m/%Y"))).color(MUTED));
                }
            });
            ui.horizontal_wrapped(|ui| {
                if !f.state.next().is_empty() {
                    ui.add(egui::TextEdit::singleline(&mut app.card.fin_ref).desired_width(110.0).hint_text("Referencia"));
                }
                for to in f.state.next() {
                    if ui.button(format!("Marcar {}", to.label().to_lowercase())).clicked() {
                        let r = app.db.fin_step(f.id, *to, &app.card.fin_ref, today, now);
                        app.card.msg = r.err().map(|e| (false, e));
                        app.changed();
                    }
                }
                if f.state == FinState::Firmada {
                    ui.add(egui::TextEdit::singleline(&mut app.card.fin_received).desired_width(90.0).hint_text("Recibido"));
                    if ui.button("Pagada por la entidad").clicked() {
                        let r = match parse_eur(&app.card.fin_received) {
                            Some(a) => app.db.fin_paid(f.id, today, a, now).map(|_| ()),
                            None => Err("Escriba lo que ha llegado de la entidad.".into()),
                        };
                        app.card.msg = r.err().map(|e| (false, e));
                        app.changed();
                    }
                }
                if !matches!(f.state, FinState::Denegada | FinState::Cancelada) {
                    ui.add(egui::TextEdit::singleline(&mut app.card.fin_reason).desired_width(140.0).hint_text("Motivo para cancelar"));
                    if ui.button("Cancelar").clicked() {
                        let r = app.db.fin_cancel(f.id, &app.card.fin_reason, today, now);
                        app.card.msg = r.err().map(|e| (false, e));
                        app.changed();
                    }
                }
                for m in app.db.fin_bank_candidates(f.id) {
                    let label = app.db.bank_moves.iter().find(|x| x.id == m).map(|x| format!("Conciliar con el ingreso del {}", x.date.format("%d/%m/%Y"))).unwrap_or_default();
                    if ui.button(label).clicked() {
                        let r = app.db.fin_match_bank(f.id, m);
                        app.card.msg = r.err().map(|e| (false, e));
                        app.changed();
                    }
                }
            });
        }
    });
}

/// Configuración: el datáfono de este puesto (fichero privado).
pub fn settings_card(app: &mut App, ui: &mut Ui) {
    let mut s = settings(app);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Datáfono de este puesto");
        muted(ui, "Vacío: datáfono sin conectar (se apunta el resultado a mano). Con un datáfono conectado, su dirección y su clave, que se guardan solo en este ordenador.");
        ui.horizontal_wrapped(|ui| {
            ui.label("Dirección");
            ui.add(egui::TextEdit::singleline(&mut s.url).desired_width(280.0).hint_text("https://…"));
            ui.label("Clave");
            ui.add(egui::TextEdit::singleline(&mut s.token).password(true).desired_width(160.0));
            if ui.button("Guardar").clicked() {
                app.card.msg = Some(match s.problem() {
                    Some(p) => (false, p.into()),
                    None => match crate::datafono::save_settings(&s) {
                        Ok(()) => (true, "Datáfono guardado.".into()),
                        Err(e) => (false, e),
                    },
                });
            }
        });
        if let Some(p) = s.problem() {
            ui.label(RichText::new(p).color(WARN));
        }
    });
    app.card.settings = Some(s);
}
