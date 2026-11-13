//! Bancos: extractos (Norma 43 o Excel/CSV) y conciliación con cobros,
//! gastos y remesas. Incluye los paneles de pacientes duplicados y del
//! registro de cambios.

use crate::app::{App, PatTab};
use crate::bank::*;
use crate::model::*;
use crate::theme::*;
use egui::{RichText, Ui};

pub struct BankUi {
    pub account: u32,
    pub path: String,
    pub filter_pending: bool,
    pub selected: Option<u32>,
}

impl Default for BankUi {
    fn default() -> Self {
        BankUi { account: 1, path: String::new(), filter_pending: true, selected: None }
    }
}

fn read_statement(path: &str) -> Result<Vec<RawMove>, String> {
    let lower = path.to_lowercase();
    let head = std::fs::read(path).map_err(|e| format!("No se puede leer el fichero: {e}"))?;
    let looks_n43 = head.starts_with(b"11") && (lower.ends_with(".n43") || lower.ends_with(".aeb") || lower.ends_with(".txt") || lower.ends_with(".q43") || !lower.contains('.'));
    if looks_n43 || lower.ends_with(".n43") {
        let text: String = match String::from_utf8(head.clone()) {
            Ok(t) => t,
            Err(_) => head.iter().map(|&b| b as char).collect(),
        };
        parse_norma43(&text)
    } else {
        let sheet = crate::extra::read_table(path)?;
        parse_bank_table(&sheet)
    }
}

pub fn screen(app: &mut App, ui: &mut Ui) {
    title(ui, "Bancos", "Extractos bancarios y conciliación con cobros, gastos y remesas");
    if app.db.bank_accounts.is_empty() {
        app.db.bank_accounts.push(BankAccount { id: 1, name: "Cuenta principal".into(), iban: app.db.config.clinic_iban.clone(), opening_balance: 0.0 });
    }
    if !app.db.bank_accounts.iter().any(|a| a.id == app.bank.account) {
        app.bank.account = app.db.bank_accounts[0].id;
    }
    let acc = app.bank.account;
    let moves: Vec<BankMove> = app.db.bank_moves.iter().filter(|m| m.account_id == acc).cloned().collect();
    let pending = moves.iter().filter(|m| m.matched.is_none()).count();
    let w = kpi_w(ui, 3);
    ui.horizontal(|ui| {
        kpi(ui, w, "Saldo según extracto", &money(app.db.bank_balance(acc)), "saldo inicial + movimientos", ACCENT);
        ui.add_space(4.0);
        kpi(ui, w, "Pendientes de conciliar", &pending.to_string(), "sin cobro o gasto asociado", if pending > 0 { WARN } else { OK });
        ui.add_space(4.0);
        kpi(ui, w, "Conciliados", &(moves.len() - pending).to_string(), &format!("de {} movimientos", moves.len()), OK);
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label("Cuenta");
            let accounts: Vec<(u32, String)> = app.db.bank_accounts.iter().map(|a| (a.id, format!("{} · {}", a.name, crate::extra::iban_fmt(&a.iban)))).collect();
            let cur = accounts.iter().find(|a| a.0 == acc).map(|a| a.1.clone()).unwrap_or_default();
            crate::theme::combo("bk_acc").width(380.0).selected_text(cur).show_ui(ui, |ui| {
                for (id, n) in &accounts {
                    ui.selectable_value(&mut app.bank.account, *id, n);
                }
            });
            if let Some(a) = app.db.bank_accounts.iter_mut().find(|a| a.id == acc) {
                ui.label("Saldo inicial");
                if ui.add(egui::DragValue::new(&mut a.opening_balance).speed(10.0).suffix(" €")).changed() {
                    app.dirty = true;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add_sized([ui.available_width() - 420.0, 34.0], egui::TextEdit::singleline(&mut app.bank.path).hint_text("Extracto del banco: fichero Norma 43 (.n43/.txt) o Excel/CSV"));
            if primary(ui, "Importar extracto").clicked() {
                let p = app.bank.path.trim().trim_matches('"').to_string();
                match read_statement(&p) {
                    Ok(v) => {
                        let (a, d) = app.db.add_bank_moves(acc, &v);
                        app.changed();
                        match app.chain_op(crate::app::ChainOp::Reconcile) {
                            Ok(c) => app.notify(format!("{a} movimientos importados ({d} ya estaban) · {c} conciliados automáticamente")),
                            Err(e) => app.warn(format!("{a} movimientos importados ({d} ya estaban), pero no se han podido conciliar: {e}")),
                        }
                    }
                    Err(e) => app.warn(e),
                }
            }
            if ui.button("⟳ Conciliar automáticamente").clicked() {
                match app.chain_op(crate::app::ChainOp::Reconcile) {
                    Ok(c) => app.notify(format!("{c} movimientos conciliados")),
                    Err(e) => app.warn(e),
                }
            }
        });
    });
    ui.add_space(8.0);
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    let side = 400.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail - side - 24.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if ui.selectable_label(app.bank.filter_pending, "Pendientes").clicked() {
                        app.bank.filter_pending = true;
                    }
                    if ui.selectable_label(!app.bank.filter_pending, "Todos").clicked() {
                        app.bank.filter_pending = false;
                    }
                });
                let tbl = Table::new("bmv", vec![("Fecha", 96.0), ("Concepto", 280.0), ("Importe", 110.0), ("Conciliado con", 260.0)]);
                tbl.header(ui);
                let mut list: Vec<BankMove> = moves.iter().filter(|m| !app.bank.filter_pending || m.matched.is_none()).cloned().collect();
                list.sort_by_key(|m| std::cmp::Reverse((m.date, m.id)));
                if list.is_empty() {
                    empty_state(ui, if app.bank.filter_pending { "Todo conciliado." } else { "Sin movimientos. Importe un extracto." });
                }
                let mut pick = None;
                egui::ScrollArea::vertical().id_salt("bml").max_height(440.0).show(ui, |ui| {
                    for (i, m) in list.iter().enumerate() {
                        let desc = m.matched.as_ref().map(|x| app.db.describe_match(x)).unwrap_or_else(|| "— pendiente —".into());
                        if tbl
                            .row(ui, i, app.bank.selected == Some(m.id), &[cell(fmt_date(m.date)), cell(&m.concept), cellc(money(m.amount), if m.amount < 0.0 { DANGER } else { OK }), cellc(desc, if m.matched.is_some() { TEXT } else { WARN })])
                            .clicked()
                        {
                            pick = Some(m.id);
                        }
                    }
                });
                if pick.is_some() {
                    app.bank.selected = pick;
                }
            });
        });
        ui.vertical(|ui| {
            ui.set_width(side);
            detail(app, ui);
        });
    });
}

fn detail(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let Some(id) = app.bank.selected else {
            section(ui, "Movimiento");
            muted(ui, "Seleccione un movimiento para conciliarlo a mano.");
            return;
        };
        let Some(m) = app.db.bank_moves.iter().find(|x| x.id == id).cloned() else { return };
        section(ui, &money(m.amount));
        muted(ui, format!("{} · {}", fmt_date(m.date), m.concept));
        ui.add_space(6.0);
        let mut set: Option<Option<Match>> = None;
        if let Some(cur) = &m.matched {
            ui.label(RichText::new(format!("✔ {}", app.db.describe_match(cur))).color(OK));
            if ui.button("Deshacer conciliación").clicked() {
                set = Some(None);
            }
        } else {
            section(ui, "Sugerencias");
            let mut any = false;
            if m.amount > 0.0 {
                let cands: Vec<(u32, String)> = app
                    .db
                    .payments
                    .iter()
                    .filter(|p| (p.amount - m.amount).abs() < 0.005 && (p.date.date() - m.date).num_days().abs() <= 15)
                    .map(|p| (p.id, format!("{} · {} · {} · {}", fmt_date(p.date.date()), app.db.patient_name(p.patient_id), p.method, p.receipt_no)))
                    .collect();
                for (pid, label) in cands {
                    any = true;
                    if ui.button(label).clicked() {
                        set = Some(Some(Match::Payments(vec![pid])));
                    }
                }
                if ui.button("Otro ingreso (no es de un paciente)").clicked() {
                    set = Some(Some(Match::Other("Otro ingreso".into())));
                }
            } else {
                let cands: Vec<(u32, String)> = app.db.expenses.iter().filter(|e| (e.total() + m.amount).abs() < 0.005).map(|e| (e.id, format!("{} · {} · {}", fmt_date(e.date), e.supplier, e.concept))).collect();
                for (eid, label) in cands {
                    any = true;
                    if ui.button(label).clicked() {
                        set = Some(Some(Match::Expense(eid)));
                        if let Some(e) = app.db.expenses.iter_mut().find(|e| e.id == eid) {
                            e.paid = true;
                        }
                    }
                }
                if primary(ui, "Crear gasto con este cargo").on_hover_text("Apunta el cargo en el libro de gastos (p. ej. comisiones o recibos)").clicked() {
                    let eid = app.db.new_id();
                    app.db.expenses.push(crate::finance::Expense {
                        id: eid,
                        date: m.date,
                        supplier: m.concept.chars().take(40).collect(),
                        concept: m.concept.clone(),
                        category: "Otros".into(),
                        base: round2(-m.amount),
                        vat_pct: 0.0,
                        invoice_no: m.reference.clone(),
                        method: "Domiciliación".into(),
                        paid: true,
                        due: None,
                    });
                    set = Some(Some(Match::Expense(eid)));
                }
            }
            if !any {
                muted(ui, "No hay cobros o gastos con ese importe.");
            }
        }
        if let Some(v) = set {
            if let Some(x) = app.db.bank_moves.iter_mut().find(|x| x.id == id) {
                x.matched = v;
            }
            app.db.audit("Conciliación manual", &format!("{} {}", fmt_date(m.date), money(m.amount)));
            app.changed();
        }
    });
}

/// Panel de pacientes duplicados (Configuración).
pub fn duplicates_panel(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Pacientes duplicados");
        muted(ui, "Pacientes que parecen la misma persona (mismo NIF, nombre o móvil). Al fusionar, todo el historial pasa al que se conserva.");
        let dups = app.db.find_duplicates();
        if dups.is_empty() {
            ui.label(RichText::new("✔ No se han encontrado duplicados.").color(OK));
        }
        let mut merge = None;
        let mut open = None;
        for (a, b, reason) in dups.iter().take(20) {
            let (pa, pb) = (app.db.patient(*a).cloned().unwrap_or_default(), app.db.patient(*b).cloned().unwrap_or_default());
            ui.horizontal_wrapped(|ui| {
                pill(ui, reason, WARN);
                if ui.link(format!("{} (Nº {})", pa.full_name(), pa.history_no)).clicked() {
                    open = Some(*a);
                }
                ui.label("y");
                if ui.link(format!("{} (Nº {})", pb.full_name(), pb.history_no)).clicked() {
                    open = Some(*b);
                }
                if ui.button(format!("Fusionar conservando Nº {}", pa.history_no)).clicked() {
                    merge = Some((*a, *b));
                }
            });
        }
        if let Some((k, r)) = merge {
            let _ = crate::tools::create_backup(&app.db, "antes de fusionar");
            match app.db.merge_patients(k, r) {
                Ok(()) => {
                    if app.patient == Some(r) {
                        app.patient = Some(k);
                    }
                    app.changed();
                    app.notify("Pacientes fusionados (se hizo antes una copia de seguridad).");
                }
                Err(e) => app.warn(e),
            }
        }
        if let Some(p) = open {
            app.open_patient(p, PatTab::Resumen);
        }
    });
}

/// Registro de cambios (Configuración).
pub fn audit_panel(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Registro de cambios");
        let tbl = Table::new("audit", vec![("Fecha", 160.0), ("Usuario", 150.0), ("Acción", 200.0), ("Detalle", 420.0)]);
        tbl.header(ui);
        let mut list = app.db.audit_log.clone();
        list.reverse();
        if list.is_empty() {
            empty_state(ui, "Sin cambios registrados todavía.");
        }
        for (i, a) in list.iter().take(30).enumerate() {
            tbl.row(ui, i, false, &[cell(a.when.format("%d/%m/%Y %H:%M").to_string()), cell(&a.user), cell(&a.action), cell(&a.detail)]);
        }
    });
}
