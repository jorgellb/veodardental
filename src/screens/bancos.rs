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

/// Movimientos del extracto para la cuenta con este IBAN (un Norma 43 puede
/// traer varias cuentas).
fn read_statement(path: &str, iban: &str) -> Result<Vec<RawMove>, String> {
    let lower = path.to_lowercase();
    let head = std::fs::read(path).map_err(|e| format!("No se puede leer el fichero: {e}"))?;
    let looks_n43 = head.starts_with(b"11") && (lower.ends_with(".n43") || lower.ends_with(".aeb") || lower.ends_with(".txt") || lower.ends_with(".q43") || !lower.contains('.'));
    if looks_n43 || lower.ends_with(".n43") {
        let text: String = match String::from_utf8(head.clone()) {
            Ok(t) => t,
            Err(_) => head.iter().map(|&b| b as char).collect(),
        };
        parse_norma43_for(&text, iban)
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
    let doubtful: Vec<u32> = app.db.doubtful_moves();
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
            if ui.button("+ Nueva cuenta").clicked() {
                let id = app.db.new_id();
                let n = app.db.bank_accounts.len() + 1;
                app.db.bank_accounts.push(BankAccount { id, name: format!("Cuenta {n}"), iban: String::new(), opening_balance: 0.0 });
                app.bank.account = id;
                app.dirty = true;
            }
        });
        // un extracto Norma 43 puede traer varias cuentas: se importa la de este IBAN
        if let Some(a) = app.db.bank_accounts.iter_mut().find(|a| a.id == acc) {
            ui.horizontal_wrapped(|ui| {
                ui.label("Nombre");
                if ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(200.0)).changed() {
                    app.dirty = true;
                }
                ui.label("IBAN");
                if ui.add(egui::TextEdit::singleline(&mut a.iban).desired_width(260.0).hint_text("ES00 0000 0000 0000 0000 0000")).changed() {
                    app.dirty = true;
                }
                if !a.iban.trim().is_empty() && !crate::extra::iban_valid(&a.iban) {
                    ui.label(RichText::new("IBAN no válido").color(DANGER));
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add_sized([ui.available_width() - 420.0, 34.0], egui::TextEdit::singleline(&mut app.bank.path).hint_text("Extracto del banco: fichero Norma 43 (.n43/.txt) o Excel/CSV"));
            if primary(ui, "Importar extracto").clicked() {
                let p = app.bank.path.trim().trim_matches('"').to_string();
                let iban = app.db.bank_accounts.iter().find(|a| a.id == acc).map(|a| a.iban.clone()).unwrap_or_default();
                match read_statement(&p, &iban) {
                    Ok(v) => {
                        let total = v.len();
                        // en el servidor (o con el fichero compartido bloqueado): sin repetir lo que ya hay
                        match app.chain_op(crate::app::ChainOp::ImportBank { account: acc, moves: v }) {
                            Ok(a) => {
                                let d = total.saturating_sub(a as usize);
                                match app.chain_op(crate::app::ChainOp::Reconcile) {
                                    Ok(c) => {
                                        let n = app.db.doubtful_moves().len();
                                        let doubt = if n > 0 { format!(" · {n} dudosos: elija con qué conciliarlos") } else { String::new() };
                                        app.notify(format!("{a} movimientos importados ({d} ya estaban) · {c} conciliados automáticamente{doubt}"))
                                    }
                                    Err(e) => app.warn(format!("{a} movimientos importados ({d} ya estaban), pero no se han podido conciliar: {e}")),
                                }
                            }
                            Err(e) => app.warn(e),
                        }
                    }
                    Err(e) => app.warn(e),
                }
            }
            if ui.button("⟳ Conciliar automáticamente").on_hover_text("Solo lo que casa sin duda; lo dudoso queda para elegirlo").clicked() {
                match app.chain_op(crate::app::ChainOp::Reconcile) {
                    Ok(c) => {
                        let n = app.db.doubtful_moves().len();
                        app.notify(if n > 0 { format!("{c} movimientos conciliados · {n} dudosos: elija con qué conciliarlos") } else { format!("{c} movimientos conciliados") })
                    }
                    Err(e) => app.warn(e),
                }
            }
        });
    });
    ui.add_space(8.0);
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    // a 1280 px de ancho, un panel más estrecho para que la fecha se lea entera
    let side = if avail < 1100.0 { 340.0 } else { 400.0 };
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
                let tbl = Table::new("bmv", vec![("Fecha", 110.0), ("Concepto", 240.0), ("Importe", 110.0), ("Conciliado con", 220.0)]);
                tbl.header(ui);
                let mut list: Vec<BankMove> = moves.iter().filter(|m| !app.bank.filter_pending || m.matched.is_none()).cloned().collect();
                list.sort_by_key(|m| std::cmp::Reverse((m.date, m.id)));
                if list.is_empty() {
                    empty_state(ui, if app.bank.filter_pending { "Todo conciliado." } else { "Sin movimientos. Importe un extracto." });
                }
                let mut pick = None;
                egui::ScrollArea::vertical().id_salt("bml").max_height(440.0).show(ui, |ui| {
                    for (i, m) in list.iter().enumerate() {
                        let desc = match &m.matched {
                            Some(x) => app.db.describe_match(x),
                            None if doubtful.contains(&m.id) => "— dudoso: elija con qué —".into(),
                            None => "— pendiente —".into(),
                        };
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
    use crate::app::ChainOp;
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
        let mut op: Option<ChainOp> = None;
        if let Some(cur) = &m.matched {
            ui.label(RichText::new(format!("✔ {}", app.db.describe_match(cur))).color(OK));
            let undo_hint = match cur {
                Match::Remittance(_) if m.set_by_match => "La remesa vuelve a estar sin cobrar: sus cobros se anulan (quedan en el historial) y las cuotas vuelven a pendientes.",
                Match::Expense(_) if m.set_by_match => "El gasto vuelve a estar pendiente de pago.",
                _ => "El movimiento vuelve a pendiente de conciliar.",
            };
            if ui.button("Deshacer conciliación").on_hover_text(undo_hint).clicked() {
                op = Some(ChainOp::UnmatchBank { bank_move: id });
            }
        } else {
            let cands = app.db.match_candidates(id);
            section(ui, if cands.len() > 1 { "Elija con qué se concilia" } else { "Sugerencias" });
            if cands.len() > 1 {
                muted(ui, "Hay varias posibilidades con ese importe: no se concilia sola.");
            }
            for (c, label) in &cands {
                if ui.add(egui::Button::new(label).wrap()).clicked() {
                    op = Some(ChainOp::MatchBank { bank_move: id, with: c.clone() });
                }
            }
            if cands.is_empty() {
                muted(ui, if m.amount > 0.0 { "No hay cobros con ese importe." } else { "No hay gastos ni devoluciones con ese importe." });
            }
            ui.add_space(4.0);
            if m.amount > 0.0 {
                if ui.button("Otro ingreso (no es de un paciente)").clicked() {
                    op = Some(ChainOp::MatchBank { bank_move: id, with: Match::Other("Otro ingreso".into()) });
                }
            } else {
                if primary(ui, "Crear gasto con este cargo").on_hover_text("Apunta el cargo en el libro de gastos (p. ej. comisiones o recibos)").clicked() {
                    op = Some(ChainOp::BankExpense { bank_move: id });
                }
                if ui.button("Traspaso u otro cargo (no es un gasto)").clicked() {
                    op = Some(ChainOp::MatchBank { bank_move: id, with: Match::Other("Traspaso u otro cargo".into()) });
                }
            }
        }
        // en el servidor (o con el fichero compartido bloqueado): lo que otro
        // puesto acaba de conciliar no se concilia dos veces
        if let Some(op) = op
            && let Err(e) = app.chain_op(op)
        {
            app.warn(e);
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
            // en el servidor (o con el fichero compartido bloqueado): solo Dirección
            match app.chain_op(crate::app::ChainOp::MergePatients { keep: k, remove: r }) {
                Ok(_) => {
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
