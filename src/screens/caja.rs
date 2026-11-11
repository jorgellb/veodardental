//! Control de caja: apertura, entradas/salidas, arqueo con desglose de
//! billetes y monedas, cierre con descuadre e histórico.

use crate::app::App;
use crate::model::*;
use crate::theme::*;
use egui::{RichText, Ui};

pub const DENOMS: [f64; 15] = [500.0, 200.0, 100.0, 50.0, 20.0, 10.0, 5.0, 2.0, 1.0, 0.5, 0.2, 0.1, 0.05, 0.02, 0.01];

pub fn show(app: &mut App, ui: &mut Ui) {
    title(ui, "Caja", "Arqueo y control del efectivo del día");
    match app.db.open_cash().cloned() {
        None => closed(app, ui),
        Some(s) => open(app, ui, s),
    }
    ui.add_space(10.0);
    history(app, ui);
}

fn closed(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "La caja está cerrada");
        muted(ui, "Abra la caja para empezar a registrar cobros en efectivo.");
        ui.horizontal(|ui| {
            ui.label("Saldo inicial (fondo de caja)");
            ui.add(egui::DragValue::new(&mut app.lists.cash_opening).range(0.0..=100000.0).speed(1.0).suffix(" €"));
            if primary(ui, "Abrir caja").clicked() {
                let id = app.db.new_id();
                app.db.cash.push(CashSession {
                    id,
                    opened: now(),
                    closed: None,
                    opening_balance: round2(app.lists.cash_opening),
                    moves: vec![],
                    counted: None,
                    state: CashState::Abierta,
                    user: app.db.config.current_user.clone(),
                    notes: String::new(),
                    center_id: cid(app.db.config.current_center),
                });
                app.lists.cash_counts = [0; 15];
                app.changed();
                app.notify("Caja abierta.");
            }
        });
    });
}

fn open(app: &mut App, ui: &mut Ui, s: CashSession) {
    let expected = app.db.cash_expected(&s);
    let pays: Vec<Payment> = app.db.session_payments(s.id).cloned().collect();
    let w = kpi_w(ui, 4);
    ui.horizontal(|ui| {
        kpi(ui, w, "Fondo inicial", &money(s.opening_balance), &format!("abierta {} por {}", s.opened.format("%d/%m %H:%M"), s.user), MUTED);
        ui.add_space(4.0);
        let cash: f64 = pays.iter().filter(|p| p.method == "Efectivo").map(|p| p.amount).sum();
        kpi(ui, w, "Cobros en efectivo", &money(cash), &format!("{} cobros", pays.iter().filter(|p| p.method == "Efectivo").count()), OK);
        ui.add_space(4.0);
        let other: f64 = pays.iter().filter(|p| p.method != "Efectivo").map(|p| p.amount).sum();
        kpi(ui, w, "Otros medios (tarjeta, etc.)", &money(other), "no entran en el cajón", INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Efectivo esperado", &money(expected), "fondo + efectivo ± movimientos", ACCENT);
    });
    ui.add_space(10.0);
    ui.columns(2, |cols| {
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Movimientos de la sesión");
            let tbl = Table::new("cashmov", vec![("Hora", 70.0), ("Concepto", 260.0), ("Forma", 100.0), ("Importe", 100.0)]);
            tbl.header(ui);
            let mut rows: Vec<(chrono::NaiveDateTime, String, String, f64)> = pays.iter().map(|p| (p.date, format!("{} · {}", app.db.patient_name(p.patient_id), p.receipt_no), p.method.clone(), p.amount)).collect();
            rows.extend(s.moves.iter().map(|m| (m.when, m.concept.clone(), if m.amount >= 0.0 { "Entrada".to_string() } else { "Salida".to_string() }, m.amount)));
            rows.sort_by_key(|r| r.0);
            for (i, r) in rows.iter().enumerate() {
                tbl.row(ui, i, false, &[cell(r.0.format("%H:%M").to_string()), cell(&r.1), cell(&r.2), cellc(money(r.3), if r.3 < 0.0 { DANGER } else { TEXT })]);
            }
            if rows.is_empty() {
                empty_state(ui, "Sin movimientos");
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_sized([200.0, 32.0], egui::TextEdit::singleline(&mut app.lists.cash_move_text).hint_text("Concepto (p. ej. compra material)"));
                ui.add(egui::DragValue::new(&mut app.lists.cash_move_amount).range(0.0..=10000.0).speed(1.0).suffix(" €"));
                let mut mv = None;
                if ui.button("＋ Entrada").clicked() {
                    mv = Some(app.lists.cash_move_amount);
                }
                if ui.button("－ Salida").clicked() {
                    mv = Some(-app.lists.cash_move_amount);
                }
                if let Some(a) = mv {
                    if a.abs() < 0.005 || app.lists.cash_move_text.trim().is_empty() {
                        app.warn("Indique concepto e importe.");
                    } else {
                        let concept = std::mem::take(&mut app.lists.cash_move_text);
                        if let Some(c) = app.db.cash.iter_mut().find(|c| c.id == s.id) {
                            c.moves.push(CashMove { when: now(), amount: round2(a), concept });
                        }
                        app.lists.cash_move_amount = 0.0;
                        app.changed();
                    }
                }
            });
        });
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Arqueo: recuento del efectivo");
            egui::Grid::new("denoms").num_columns(6).spacing([10.0, 4.0]).show(ui, |ui| {
                for (i, d) in DENOMS.iter().enumerate() {
                    let label = if *d >= 5.0 { format!("Billete {d:.0} €") } else if *d >= 1.0 { format!("Moneda {d:.0} €") } else { format!("Moneda {:.0} cts", d * 100.0) };
                    ui.label(RichText::new(label).size(14.0));
                    ui.add(egui::DragValue::new(&mut app.lists.cash_counts[i]).range(0..=9999));
                    if i % 2 == 1 {
                        ui.end_row();
                    } else {
                        ui.label("   ");
                    }
                }
            });
            let counted: f64 = round2(DENOMS.iter().zip(app.lists.cash_counts.iter()).map(|(d, n)| d * *n as f64).sum());
            let diff = round2(counted - expected);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Contado: {}", money(counted))).family(crate::theme::sb()));
                ui.label(RichText::new(format!("Esperado: {}", money(expected))).color(MUTED));
            });
            let (txt, col) = if diff.abs() < 0.005 {
                ("Caja cuadrada ✔".to_string(), OK)
            } else if diff > 0.0 {
                (format!("Sobran {}", money(diff)), WARN)
            } else {
                (format!("Faltan {}", money(-diff)), DANGER)
            };
            ui.label(RichText::new(txt).size(20.0).family(crate::theme::sb()).color(col));
            ui.add_space(6.0);
            if danger(ui, "Cerrar caja con este arqueo").clicked() {
                if let Some(c) = app.db.cash.iter_mut().find(|c| c.id == s.id) {
                    c.counted = Some(counted);
                    c.closed = Some(now());
                    c.state = CashState::Cerrada;
                    c.notes = if diff.abs() < 0.005 { "Cuadrada".into() } else { format!("Descuadre {}", money(diff)) };
                }
                app.lists.cash_opening = counted.min(200.0);
                app.changed();
                app.notify("Caja cerrada.");
            }
        });
    });
}

fn history(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Histórico de cajas");
        let tbl = Table::new("cashhist", vec![("Apertura", 140.0), ("Cierre", 140.0), ("Usuario", 110.0), ("Fondo", 100.0), ("Esperado", 110.0), ("Contado", 110.0), ("Resultado", 160.0)]);
        tbl.header(ui);
        let center = cid(app.db.config.current_center);
        let mut list: Vec<CashSession> = app.db.cash.iter().filter(|c| cid(c.center_id) == center).cloned().collect();
        list.sort_by_key(|c| std::cmp::Reverse(c.opened));
        for (i, c) in list.iter().enumerate() {
            let exp = app.db.cash_expected(c);
            tbl.row(ui, i, false, &[
                cell(c.opened.format("%d/%m/%Y %H:%M").to_string()),
                cell(c.closed.map(|d| d.format("%d/%m/%Y %H:%M").to_string()).unwrap_or_else(|| "abierta".into())),
                cell(&c.user),
                cell(money(c.opening_balance)),
                cell(money(exp)),
                cell(c.counted.map(money).unwrap_or_default()),
                cellc(if c.state == CashState::Abierta { "En curso".to_string() } else { c.notes.clone() }, if c.notes.starts_with("Descuadre") { DANGER } else { OK }),
            ]);
        }
    });
}
