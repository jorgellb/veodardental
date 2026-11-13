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
    pub kind: PayKind,
}

/// Qué se hace en el diálogo de cobro.
#[derive(Clone, Copy, PartialEq)]
pub enum PayKind {
    /// Cobro a cuenta (lo normal).
    Cobro,
    /// Cuota de un plan: (presupuesto, nº de cuota desde 0).
    Cuota(u32, u32),
    /// Devolver dinero que el paciente tiene a favor.
    Devolucion,
    /// Anular un cobro equivocado.
    Anulacion(u32),
}

impl PayDraft {
    pub fn new(app: &App, pid: u32) -> Self {
        let bal = app.db.patient_balance(pid).max(0.0);
        PayDraft { patient_id: pid, amount: bal, method: "Tarjeta".into(), concept: "Pago a cuenta".into(), kind: PayKind::Cobro }
    }

    /// Cobro en la clínica de una cuota (con el importe de la cuota).
    pub fn installment(app: &App, budget: u32, i: u32) -> Option<Self> {
        let b = app.db.budgets.iter().find(|b| b.id == budget)?;
        let amount = b.plan.as_ref()?.installment(i, b.total());
        Some(PayDraft { patient_id: b.patient_id, amount, method: "Tarjeta".into(), concept: format!("Cuota {} plan presupuesto #{budget}", i + 1), kind: PayKind::Cuota(budget, i) })
    }

    /// Devolución del saldo a favor del paciente.
    pub fn refund(app: &App, pid: u32) -> Self {
        let credit = (-app.db.patient_balance(pid)).max(0.0);
        PayDraft { patient_id: pid, amount: credit, method: "Efectivo".into(), concept: String::new(), kind: PayKind::Devolucion }
    }

    /// Anulación de un cobro.
    pub fn cancel(app: &App, payment: u32) -> Option<Self> {
        let p = app.db.payments.iter().find(|p| p.id == payment)?;
        Some(PayDraft { patient_id: p.patient_id, amount: p.amount, method: p.method.clone(), concept: String::new(), kind: PayKind::Anulacion(payment) })
    }
}

pub fn pay_dialog(app: &mut App, ctx: &egui::Context, mut d: PayDraft) -> Dialog {
    let mut open = true;
    let mut save = false;
    let bal = app.db.patient_balance(d.patient_id);
    let (head, button) = match d.kind {
        PayKind::Cobro => ("Registrar cobro", "Cobrar"),
        PayKind::Cuota(..) => ("Cobrar cuota", "Cobrar"),
        PayKind::Devolucion => ("Devolver dinero al paciente", "Devolver"),
        PayKind::Anulacion(_) => ("Anular cobro", "Anular el cobro"),
    };
    crate::theme::modal("pay_dlg").show(ctx, |ui| {
        ui.set_width(460.0);
        title(ui, head, &app.db.patient_name(d.patient_id));
        match d.kind {
            PayKind::Cobro => {
                ui.label(if bal > 0.005 {
                    RichText::new(format!("Pendiente de cobro: {}", money(bal))).color(DANGER).family(crate::theme::sb())
                } else {
                    RichText::new(format!("Cuenta al día (saldo {}). El cobro quedará como anticipo.", money(-bal))).color(OK)
                });
                let planned = app.db.patient_planned(d.patient_id);
                if planned > 0.0 {
                    muted(ui, format!("Tratamientos previstos por realizar: {}", money(planned)));
                }
            }
            PayKind::Cuota(_, i) => {
                ui.label(RichText::new(format!("Cuota {} · {}", i + 1, money(d.amount))).family(crate::theme::sb()));
                muted(ui, "Elija cómo la paga: si es en efectivo, entra en la caja.");
            }
            PayKind::Devolucion => {
                ui.label(RichText::new(format!("Tiene a favor: {}", money((-bal).max(0.0)))).color(OK).family(crate::theme::sb()));
            }
            PayKind::Anulacion(pid) => {
                if let Some(p) = app.db.payments.iter().find(|p| p.id == pid) {
                    ui.label(RichText::new(format!("Recibo {} · {} · {} · {}", p.receipt_no, fmt_date(p.date.date()), money(p.amount), p.method)).family(crate::theme::sb()));
                    muted(ui, format!("{}. Se apunta un cobro en negativo con la misma forma de pago{}.", p.concept, if p.method == "Efectivo" { " (sale de la caja abierta)" } else { "" }));
                }
            }
        }
        ui.add_space(8.0);
        let fixed = matches!(d.kind, PayKind::Cuota(..) | PayKind::Anulacion(_));
        ui.horizontal(|ui| {
            ui.label("Importe");
            ui.add_enabled(!fixed, egui::DragValue::new(&mut d.amount).range(0.0..=100000.0).speed(1.0).suffix(" €"));
            if d.kind == PayKind::Cobro && bal > 0.005 && ui.small_button("Todo lo pendiente").clicked() {
                d.amount = bal;
            }
        });
        if !matches!(d.kind, PayKind::Anulacion(_)) {
            ui.label(RichText::new(if d.kind == PayKind::Devolucion { "Cómo se devuelve" } else { "Forma de pago" }).size(13.5).color(MUTED));
            ui.horizontal_wrapped(|ui| {
                for m in PAY_METHODS {
                    // la financiación la usa la financiera, no el paciente que paga su cuota
                    if m == "Financiación" && d.kind != PayKind::Cobro {
                        continue;
                    }
                    let sel = d.method == m;
                    if ui.add(egui::Button::new(RichText::new(m).color(if sel { Color32::WHITE } else { TEXT })).fill(if sel { ACCENT } else { Color32::from_rgb(236, 240, 245) })).clicked() {
                        d.method = m.into();
                    }
                }
            });
        }
        match d.kind {
            PayKind::Cobro => {
                field(ui, "Concepto", &mut d.concept, 420.0);
            }
            PayKind::Cuota(..) => {}
            PayKind::Devolucion | PayKind::Anulacion(_) => {
                field(ui, "Motivo", &mut d.concept, 420.0);
            }
        }
        if d.method == "Efectivo" && app.db.open_cash().is_none() {
            ui.label(RichText::new("⚠ No hay caja abierta: el efectivo no se asignará a ninguna caja.").color(WARN));
        }
        ui.add_space(4.0);
        muted(ui, format!("Recibo nº {}", app.db.peek_receipt()));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let fill = if matches!(d.kind, PayKind::Anulacion(_) | PayKind::Devolucion) { DANGER } else { ACCENT };
            if ui.add_enabled(d.amount > 0.0, egui::Button::new(RichText::new(button).color(Color32::WHITE)).fill(fill)).clicked() {
                save = true;
            }
            if ui.button("Cancelar").clicked() {
                open = false;
            }
        });
    });
    if save {
        // la cuota, la anulación y la devolución, con los datos al día (en red las
        // hace el servidor): así no se cobra dos veces ni se anula dos veces
        let done = match d.kind {
            PayKind::Cobro => {
                let id = app.db.add_payment(d.patient_id, d.amount, &d.method, &d.concept);
                app.changed();
                Ok(id)
            }
            PayKind::Cuota(budget, index) => app.chain_op(crate::app::ChainOp::PayInstallment { budget, index, method: d.method.clone() }),
            PayKind::Devolucion => app.chain_op(crate::app::ChainOp::Refund { patient: d.patient_id, amount: d.amount, method: d.method.clone(), reason: d.concept.clone() }),
            PayKind::Anulacion(payment) => app.chain_op(crate::app::ChainOp::CancelPayment { payment, reason: d.concept.clone() }),
        };
        match done {
            Ok(_) => {
                app.notify(match d.kind {
                    PayKind::Cobro => format!("Cobro registrado: {} ({})", money(d.amount), d.method),
                    PayKind::Cuota(_, i) => format!("Cuota {} cobrada: {} ({})", i + 1, money(d.amount), d.method),
                    PayKind::Devolucion => format!("Devolución registrada: {} ({})", money(d.amount), d.method),
                    PayKind::Anulacion(_) => format!("Cobro anulado: {}", money(d.amount)),
                });
                return app.dialog_take_or_none();
            }
            Err(e) => app.warn(e),
        }
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
            if primary(ui, "＋ Cobrar al paciente activo").clicked()
                && let Some(pid) = app.patient
            {
                app.dialog = Dialog::Payment(PayDraft::new(app, pid));
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
    let total: crate::cents::Cents = list.iter().map(|p| crate::cents::Cents::from_eur(p.amount)).sum();
    let by_method = by_method(&list);
    let w = kpi_w(ui, by_method.len() + 1);
    ui.horizontal(|ui| {
        kpi(ui, w, "Total cobrado", &total.to_string(), &format!("{} cobros", list.len()), ACCENT);
        for (m, s) in &by_method {
            ui.add_space(4.0);
            kpi(ui, w, if *m == "Domiciliación" { "Remesas" } else { m }, &s.to_string(), "", INFO);
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

/// Lo cobrado por forma de pago: las de la clínica y, si hay, lo cobrado por
/// remesa (domiciliación). Suman el total de la lista.
fn by_method(list: &[Payment]) -> Vec<(&'static str, crate::cents::Cents)> {
    crate::informes::METHODS.iter().map(|m| (*m, list.iter().filter(|p| p.method == *m).map(|p| crate::cents::Cents::from_eur(p.amount)).sum::<crate::cents::Cents>())).filter(|(m, s)| PAY_METHODS.contains(m) || !s.is_zero()).collect()
}

fn filtered(app: &App) -> Vec<Payment> {
    let mut v: Vec<Payment> = app.db.payments.iter().filter(|p| p.date.date() >= app.lists.pay_from && p.date.date() <= app.lists.pay_to).filter(|p| app.lists.pay_method.as_ref().map(|m| &p.method == m).unwrap_or(true)).cloned().collect();
    v.sort_by_key(|p| std::cmp::Reverse(p.date));
    v
}

fn export_csv(app: &App) -> Result<String, String> {
    let dir = crate::paths::documents().join(crate::brand::DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("cobros_{}_{}.csv", app.lists.pay_from.format("%Y%m%d"), app.lists.pay_to.format("%Y%m%d")));
    let mut s = String::from("Fecha;Recibo;Paciente;Concepto;Forma de pago;Usuario;Importe\n");
    for p in filtered(app) {
        s.push_str(&format!("{};{};{};{};{};{};{}\n", p.date.format("%d/%m/%Y %H:%M"), p.receipt_no, app.db.patient_name(p.patient_id), p.concept.replace(';', ","), p.method, p.user, format!("{:.2}", p.amount).replace('.', ",")));
    }
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Antes las casillas por forma de pago eran solo las cinco de la clínica:
    /// lo cobrado por remesa entraba en el total pero en ninguna casilla, y las
    /// casillas no sumaban el total.
    #[test]
    fn the_payment_methods_add_up_to_the_total() {
        let mut db = Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 6).unwrap());
        db.add_payment(101, 50.0, "Domiciliación", "Cuota remesa");
        let list: Vec<Payment> = db.payments.to_vec();
        let total: crate::cents::Cents = list.iter().map(|p| crate::cents::Cents::from_eur(p.amount)).sum();
        let shown = by_method(&list);
        assert!(shown.iter().any(|(m, s)| *m == "Domiciliación" && s.0 >= 5000));
        assert_eq!(shown.iter().map(|x| x.1).sum::<crate::cents::Cents>(), total);
        // sin remesas, solo las cinco de siempre
        let plain: Vec<Payment> = list.into_iter().filter(|p| p.method != "Domiciliación").collect();
        assert_eq!(by_method(&plain).len(), PAY_METHODS.len());
    }
}
