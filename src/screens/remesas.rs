//! Remesas SEPA: cobro domiciliado de las cuotas de financiación.

use crate::app::{App, PatTab};
use crate::extra::*;
use crate::model::*;
use crate::theme::*;
use egui::{RichText, Ui};

pub struct RemState2 {
    pub until: chrono::NaiveDate,
    pub collection: chrono::NaiveDate,
}

impl Default for RemState2 {
    fn default() -> Self {
        let t = today();
        RemState2 { until: t + chrono::Duration::days(30), collection: t + chrono::Duration::days(5) }
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
        let pend: f64 = app.db.remittances.iter().filter(|r| r.state == crate::extra::RemState::Generada).map(|r| r.total()).sum();
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
            let col = app.rem.collection;
            match app.db.create_remittance(&due, col) {
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
            let resp = tbl.row(ui, i, false, &[cell(format!("#{}", r.id)), cell(r.created.format("%d/%m/%Y %H:%M").to_string()), cell(fmt_date(r.collection)), cell(r.items.len().to_string()), cell(money(r.total())), cellc(if cobrada { "Cobrada" } else { "Enviada" }, if cobrada { OK } else { INFO }), cell(&r.file)]);
            if !cobrada {
                resp.on_hover_text("Clic derecho: marcar como cobrada cuando el banco abone la remesa").context_menu(|ui| {
                    if ui.button("✔ El banco la ha cobrado: registrar cobros").clicked() {
                        collect = Some(r.id);
                        ui.close();
                    }
                });
            }
        }
        if let Some(rid) = collect {
            match app.db.remittance_collected(rid) {
                Ok(()) => {
                    app.changed();
                    app.notify("Remesa cobrada: cobros registrados y cuotas marcadas como pagadas.");
                }
                Err(e) => app.warn(e),
            }
        }
        muted(ui, "Cuando el banco abone la remesa, haga clic derecho sobre ella para registrar los cobros. Las devoluciones se cobran de nuevo desde la cuenta del paciente.");
    });
}

fn write_xml(app: &mut App, rid: u32) -> Result<String, String> {
    let dir = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| std::path::PathBuf::from(".")).join(crate::brand::DIR).join("remesas");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("remesa_SEPA_{rid}.xml"));
    std::fs::write(&path, app.db.sepa_xml(rid)).map_err(|e| e.to_string())?;
    let s = path.display().to_string();
    if let Some(r) = app.db.remittances.iter_mut().find(|r| r.id == rid) {
        r.file = s.clone();
    }
    Ok(s)
}
