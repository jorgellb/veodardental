//! Cobros: diálogo de cobro (con recibo numerado) y hoja de cobros.

use crate::app::{App, Dialog, PatTab};
use crate::model::*;
use crate::theme::*;
use egui::{Color32, RichText, Ui};

#[derive(Clone)]
pub struct PayDraft {
    pub patient_id: u32,
    pub amount: f64,
    pub method: String,
    pub concept: String,
}

impl PayDraft {
    pub fn new(app: &App, pid: u32) -> Self {
        let bal = app.db.patient_balance(pid).max(0.0);
        PayDraft { patient_id: pid, amount: bal, method: "Tarjeta".into(), concept: "Pago a cuenta".into() }
    }
}

pub fn pay_dialog(app: &mut App, ctx: &egui::Context, mut d: PayDraft) -> Dialog {
    let mut open = true;
    let mut save = false;
    let bal = app.db.patient_balance(d.patient_id);
    crate::theme::modal("pay_dlg").show(ctx, |ui| {
        ui.set_width(460.0);
        title(ui, "Registrar cobro", &app.db.patient_name(d.patient_id));
        ui.label(if bal > 0.005 {
            RichText::new(format!("Pendiente de cobro: {}", money(bal))).color(DANGER).family(crate::theme::sb())
        } else {
            RichText::new(format!("Cuenta al día (saldo {}). El cobro quedará como anticipo.", money(-bal))).color(OK)
        });
        let planned = app.db.patient_planned(d.patient_id);
        if planned > 0.0 {
            muted(ui, format!("Tratamientos previstos por realizar: {}", money(planned)));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Importe");
            ui.add(egui::DragValue::new(&mut d.amount).range(0.0..=100000.0).speed(1.0).suffix(" €"));
            if bal > 0.005 && ui.small_button("Todo lo pendiente").clicked() {
                d.amount = bal;
            }
        });
        ui.label(RichText::new("Forma de pago").size(13.5).color(MUTED));
        ui.horizontal_wrapped(|ui| {
            for m in PAY_METHODS {
                let sel = d.method == m;
                if ui.add(egui::Button::new(RichText::new(m).color(if sel { Color32::WHITE } else { TEXT })).fill(if sel { ACCENT } else { Color32::from_rgb(236, 240, 245) })).clicked() {
                    d.method = m.into();
                }
            }
        });
        field(ui, "Concepto", &mut d.concept, 420.0);
        if d.method == "Efectivo" && app.db.open_cash().is_none() {
            ui.label(RichText::new("⚠ No hay caja abierta: el efectivo no se asignará a ninguna caja.").color(WARN));
        }
        ui.add_space(4.0);
        muted(ui, format!("Recibo nº {}-{:05}", app.db.config.receipt_series, app.db.config.next_receipt));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(d.amount > 0.0, egui::Button::new(RichText::new("Cobrar").color(Color32::WHITE)).fill(ACCENT)).clicked() {
                save = true;
            }
            if ui.button("Cancelar").clicked() {
                open = false;
            }
        });
    });
    if save {
        app.db.add_payment(d.patient_id, d.amount, &d.method, &d.concept);
        app.changed();
        app.notify(format!("Cobro registrado: {} ({})", money(d.amount), d.method));
        return app.dialog_take_or_none();
    }
    if open { Dialog::Payment(d) } else { app.dialog_take_or_none() }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if ui.selectable_label(app.cobros_tab == 0, "Hoja de cobros").clicked() {
            app.cobros_tab = 0;
        }
        if ui.selectable_label(app.cobros_tab == 1, "Remesas SEPA (cuotas domiciliadas)").clicked() {
            app.cobros_tab = 1;
        }
    });
    ui.add_space(6.0);
    if app.cobros_tab == 1 {
        egui::ScrollArea::vertical().id_salt("rem_scroll").auto_shrink([false, false]).show(ui, |ui| crate::screens::remesas::view(app, ui));
        return;
    }
    ui.horizontal(|ui| {
        title(ui, "Hoja de cobros", "Cobros registrados por fecha y forma de pago");
        right_aligned(ui, |ui| {
            if primary(ui, "＋ Cobrar al paciente activo").clicked() {
                if let Some(pid) = app.patient {
                    app.dialog = Dialog::Payment(PayDraft::new(app, pid));
                }
            }
            if ui.button("⬇ Exportar CSV").clicked() {
                match export_csv(app) {
                    Ok(p) => app.notify(format!("Exportado en {p}")),
                    Err(e) => app.warn(e),
                }
            }
        });
    });
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().button_padding.x = 11.0;
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(RichText::new("Desde").font(crate::theme::mdf(13.5)).color(MUTED));
            crate::screens::date_picker(ui, "pay_from", &mut app.lists.pay_from);
            ui.label(RichText::new("hasta").font(crate::theme::mdf(13.5)).color(MUTED));
            crate::screens::date_picker(ui, "pay_to", &mut app.lists.pay_to);
            ui.add_space(6.0);
            if ui.selectable_label(app.lists.pay_method.is_none(), "Todas").clicked() {
                app.lists.pay_method = None;
            }
            for m in PAY_METHODS {
                if ui.selectable_label(app.lists.pay_method.as_deref() == Some(m), m).clicked() {
                    app.lists.pay_method = Some(m.into());
                }
            }
        });
    });
    ui.add_space(8.0);
    let list = filtered(app);
    let total: f64 = list.iter().map(|p| p.amount).sum();
    let w = kpi_w(ui, PAY_METHODS.len() + 1);
    ui.horizontal(|ui| {
        kpi(ui, w, "Total cobrado", &money(total), &format!("{} cobros", list.len()), ACCENT);
        for m in PAY_METHODS {
            ui.add_space(4.0);
            let s: f64 = list.iter().filter(|p| p.method == m).map(|p| p.amount).sum();
            kpi(ui, w, m, &money(s), "", INFO);
        }
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        let tbl = Table::new("pays", vec![("Fecha", 140.0), ("Recibo", 120.0), ("Paciente", 220.0), ("Concepto", 280.0), ("Forma de pago", 130.0), ("Usuario", 110.0), ("Importe", 110.0)]);
        tbl.header(ui);
        let mut open = None;
        egui::ScrollArea::vertical().id_salt("payl").auto_shrink([false, false]).show(ui, |ui| {
            if list.is_empty() {
                empty_state(ui, "Sin cobros en el periodo");
            }
            for (i, p) in list.iter().enumerate() {
                let cells = [cell(p.date.format("%d/%m/%Y %H:%M").to_string()), cell(&p.receipt_no), cell(app.db.patient_name(p.patient_id)), cell(&p.concept), cell(&p.method), cell(&p.user), cellc(money(p.amount), OK)];
                if tbl.row(ui, i, false, &cells).clicked() {
                    open = Some(p.patient_id);
                }
            }
        });
        if let Some(pid) = open {
            app.open_patient(pid, PatTab::Cuenta);
        }
    });
}

fn filtered(app: &App) -> Vec<Payment> {
    let mut v: Vec<Payment> = app
        .db
        .payments
        .iter()
        .filter(|p| p.date.date() >= app.lists.pay_from && p.date.date() <= app.lists.pay_to)
        .filter(|p| app.lists.pay_method.as_ref().map(|m| &p.method == m).unwrap_or(true))
        .cloned()
        .collect();
    v.sort_by_key(|p| std::cmp::Reverse(p.date));
    v
}

fn export_csv(app: &App) -> Result<String, String> {
    let dir = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| std::path::PathBuf::from(".")).join(crate::brand::DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("cobros_{}_{}.csv", app.lists.pay_from.format("%Y%m%d"), app.lists.pay_to.format("%Y%m%d")));
    let mut s = String::from("Fecha;Recibo;Paciente;Concepto;Forma de pago;Usuario;Importe\n");
    for p in filtered(app) {
        s.push_str(&format!(
            "{};{};{};{};{};{};{}\n",
            p.date.format("%d/%m/%Y %H:%M"),
            p.receipt_no,
            app.db.patient_name(p.patient_id),
            p.concept.replace(';', ","),
            p.method,
            p.user,
            format!("{:.2}", p.amount).replace('.', ",")
        ));
    }
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}
