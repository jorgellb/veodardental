//! Remesas SEPA: cobro domiciliado de las cuotas de financiación.

use crate::app::{App, PatTab};
use crate::extra::*;
use crate::model::*;
use crate::theme::*;
use egui::{RichText, Ui};

pub struct RemState2 {
    pub until: chrono::NaiveDate,
    pub collection: chrono::NaiveDate,
    /// Remesa abierta (se ven sus recibos) y recibo elegido en ella.
    pub open: Option<u32>,
    pub item: Option<usize>,
    pub reason: String,
}

/// Motivos de devolución más frecuentes (códigos SEPA).
const REASONS: [&str; 7] = [
    "AM04 Saldo insuficiente",
    "MD06 Devuelto a petición del titular",
    "AC04 Cuenta cancelada",
    "AC01 IBAN incorrecto",
    "MD01 Sin mandato",
    "MS02 Motivo no indicado por el titular",
    "Otro",
];

impl Default for RemState2 {
    fn default() -> Self {
        let t = today();
        RemState2 { until: t + chrono::Duration::days(30), collection: t + chrono::Duration::days(5), open: None, item: None, reason: REASONS[0].into() }
    }
}

pub fn view(app: &mut App, ui: &mut Ui) {
    let due = app.db.due_installments(app.rem.until);
    let ok: Vec<&DueInstallment> = due.iter().filter(|d| d.problem.is_none()).collect();
    let total_ok: f64 = ok.iter().map(|d| d.amount).sum();
    let w = kpi_w(ui, 3);
    ui.horizontal(|ui| {
        kpi(ui, w, "Cuotas para domiciliar", &ok.len().to_string(), &money(total_ok), ACCENT);
        ui.add_space(4.0);
        kpi(ui, w, "Cuotas con problemas", &(due.len() - ok.len()).to_string(), "falta IBAN o mandato", WARN);
        ui.add_space(4.0);
        let pend: f64 = app.db.remittances.iter().filter(|r| r.state == crate::extra::RemState::Generada).map(|r| r.collected()).sum();
        kpi(ui, w, "Remesas enviadas sin cobrar", &money(pend), "pendientes de abono del banco", INFO);
    });
    ui.add_space(8.0);
    if app.db.config.creditor_id.trim().is_empty() || !iban_valid(&app.db.config.clinic_iban) {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("⚠ Falta configurar el identificador de acreedor SEPA y el IBAN de la clínica (Configuración → Datos del centro).").color(WARN));
        });
        ui.add_space(8.0);
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            section(ui, "Cuotas vencidas o próximas");
            ui.label("hasta");
            crate::screens::date_picker(ui, "rem_until", &mut app.rem.until);
            ui.label("· fecha de cargo");
            crate::screens::date_picker(ui, "rem_col", &mut app.rem.collection);
        });
        let tbl = Table::new("due", vec![("Vence", 100.0), ("Paciente", 220.0), ("Presupuesto", 110.0), ("Cuota", 70.0), ("Importe", 100.0), ("Estado", 260.0)]);
        tbl.header(ui);
        if due.is_empty() {
            empty_state(ui, "No hay cuotas pendientes en el periodo.");
        }
        let mut open = None;
        for (i, d) in due.iter().enumerate() {
            let cells = [cell(fmt_date(d.due)), cell(app.db.patient_name(d.patient_id)), cell(format!("#{}", d.budget_id)), cell((d.index + 1).to_string()), cell(money(d.amount)), match &d.problem {
                None => cellc("Lista para remesar", OK),
                Some(p) => cellc(format!("⚠ {p} (pulse para completar)"), WARN),
            }];
            if tbl.row(ui, i, false, &cells).clicked() && d.problem.is_some() {
                open = Some(d.patient_id);
            }
        }
        if let Some(p) = open {
            app.open_patient(p, PatTab::Filiacion);
        }
        ui.add_space(8.0);
        if primary(ui, &format!("Generar remesa SEPA ({} cuotas · {})", ok.len(), money(total_ok))).clicked() {
            let (until, collection) = (app.rem.until, app.rem.collection);
            match app.chain_op(crate::app::ChainOp::Remit { until, collection }) {
                Ok(rid) => match write_xml(app, rid) {
                    Ok(p) => {
                        app.changed();
                        app.notify(format!("Remesa generada: {p}. Súbala a la banca online de la clínica."));
                    }
                    Err(e) => app.warn(e),
                },
                Err(e) => app.warn(e),
            }
        }
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Remesas generadas");
        let tbl = Table::new("rems", vec![("Nº", 60.0), ("Creada", 140.0), ("Cargo", 100.0), ("Cuotas", 70.0), ("Total", 110.0), ("Estado", 120.0), ("Fichero", 300.0)]);
        tbl.header(ui);
        let mut list = app.db.remittances.clone();
        list.reverse();
        if list.is_empty() {
            empty_state(ui, "Todavía no hay remesas.");
        }
        let mut collect = None;
        for (i, r) in list.iter().enumerate() {
            let cobrada = r.state == crate::extra::RemState::Cobrada;
            let back = r.items.iter().filter(|x| x.returned.is_some()).count();
            let state = match (cobrada, back) {
                (true, 0) => "Cobrada".to_string(),
                (false, 0) => "Enviada".to_string(),
                (true, n) => format!("Cobrada · {n} devuelto{}", if n > 1 { "s" } else { "" }),
                (false, n) => format!("Enviada · {n} devuelto{}", if n > 1 { "s" } else { "" }),
            };
            let total = if back > 0 { format!("{} de {}", money(r.collected()), money(r.total())) } else { money(r.total()) };
            let open = app.rem.open == Some(r.id);
            let resp = tbl.row(ui, i, open, &[cell(format!("#{}", r.id)), cell(r.created.format("%d/%m/%Y %H:%M").to_string()), cell(fmt_date(r.collection)), cell(r.items.len().to_string()), cell(total), cellc(state, if back > 0 { WARN } else if cobrada { OK } else { INFO }), cell(&r.file)]);
            let resp = resp.on_hover_text(if cobrada { "Pulse para ver los recibos y registrar los devueltos" } else { "Pulse para ver los recibos · clic derecho: marcar como cobrada cuando el banco abone la remesa" });
            if resp.clicked() {
                app.rem.open = if open { None } else { Some(r.id) };
                app.rem.item = None;
            }
            if !cobrada {
                resp.context_menu(|ui| {
                    if ui.button("✔ El banco la ha cobrado: registrar cobros").clicked() {
                        collect = Some(r.id);
                        ui.close();
                    }
                });
            }
            if open {
                items(app, ui, r);
            }
        }
        if let Some(rid) = collect {
            match app.chain_op(crate::app::ChainOp::Collected { remittance: rid }) {
                Ok(_) => {
                    app.changed();
                    app.notify("Remesa cobrada: cobros registrados y cuotas marcadas como pagadas.");
                }
                Err(e) => app.warn(e),
            }
        }
        muted(ui, "Cuando el banco abone la remesa, haga clic derecho sobre ella para registrar los cobros. Si el banco devuelve un recibo, pulse la remesa y regístrelo: la cuota vuelve a quedar pendiente, para la siguiente remesa o para cobrarla en la clínica.");
    });
}

/// Recibos de la remesa `r`, con la devolución del elegido.
fn items(app: &mut App, ui: &mut Ui, r: &Remittance) {
    let cobrada = r.state == crate::extra::RemState::Cobrada;
    egui::Frame::new().fill(tint(INFO, 0.05)).corner_radius(egui::CornerRadius::same(8)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let tbl = Table::new("rem_items", vec![("Paciente", 220.0), ("Presupuesto", 100.0), ("Cuota", 60.0), ("Importe", 100.0), ("Estado", 320.0)]);
        tbl.header(ui);
        for (i, it) in r.items.iter().enumerate() {
            let state = match &it.returned {
                Some(d) if it.return_reason.is_empty() => cellc(format!("Devuelto el {}", fmt_date(*d)), DANGER),
                Some(d) => cellc(format!("Devuelto el {} · {}", fmt_date(*d), it.return_reason), DANGER),
                None if cobrada => cellc("Cobrado", OK),
                None => cellc("Pendiente del banco", INFO),
            };
            let sel = app.rem.item == Some(i);
            let resp = tbl.row(ui, i, sel, &[cell(app.db.patient_name(it.patient_id)), cell(format!("#{}", it.budget_id)), cell((it.installment + 1).to_string()), cell(money(it.amount)), state]);
            if it.returned.is_none() && resp.on_hover_text("Pulse si el banco ha devuelto este recibo").clicked() {
                app.rem.item = if sel { None } else { Some(i) };
            }
        }
        let Some(i) = app.rem.item else { return };
        let Some(it) = r.items.get(i).filter(|x| x.returned.is_none()) else { return };
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("Recibo devuelto: {} · cuota {} · {}", app.db.patient_name(it.patient_id), it.installment + 1, money(it.amount))).family(crate::theme::sb()));
            ui.label("Motivo");
            crate::theme::combo("rem_reason").width(280.0).selected_text(&app.rem.reason).show_ui(ui, |ui| {
                for m in REASONS {
                    ui.selectable_value(&mut app.rem.reason, m.to_string(), m);
                }
            });
            if primary(ui, "↩ Registrar la devolución").clicked() {
                let reason = if app.rem.reason == "Otro" { String::new() } else { app.rem.reason.clone() };
                match app.chain_op(crate::app::ChainOp::Returned { remittance: r.id, item: i, reason }) {
                    Ok(_) => {
                        app.changed();
                        app.rem.item = None;
                        app.notify(if cobrada { "Devolución registrada: se ha anulado el cobro y la cuota vuelve a estar pendiente." } else { "Devolución registrada: ese recibo no se cobrará al registrar la remesa." });
                    }
                    Err(e) => app.warn(e),
                }
            }
        });
        if cobrada {
            muted(ui, "Se registra un cobro en negativo («Devolución») en la cuenta del paciente: vuelve a deber la cuota, que se podrá incluir en la siguiente remesa o cobrar en la clínica.");
        }
    });
}

fn write_xml(app: &mut App, rid: u32) -> Result<String, String> {
    let dir = crate::paths::documents().join(crate::brand::DIR).join("remesas");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("remesa_SEPA_{rid}.xml"));
    std::fs::write(&path, app.db.sepa_xml(rid)).map_err(|e| e.to_string())?;
    let s = path.display().to_string();
    if let Some(r) = app.db.remittances.iter_mut().find(|r| r.id == rid) {
        r.file = s.clone();
    }
    Ok(s)
}
