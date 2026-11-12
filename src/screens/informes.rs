//! Informes: pendientes de cobro, libro diario, producción y liquidación
//! de colaboradores, ingresos mensuales y presupuestos pendientes.

use crate::app::{App, Dialog, PatTab};
use crate::model::*;
use crate::theme::*;
use chrono::Datelike;
use egui::{Align2, CornerRadius, FontId, RichText, Sense, Ui, pos2, vec2};

const TABS: [&str; 7] = ["Pendientes de cobro", "Libro diario", "Producción", "Ingresos", "Presupuestos", "Captación", "Consultas"];

pub fn show(app: &mut App, ui: &mut Ui) {
    title(ui, "Informes", "Listados de gestión de la clínica");
    ui.horizontal_wrapped(|ui| {
        for (i, t) in TABS.iter().enumerate() {
            if ui.selectable_label(app.lists.report_tab == i, *t).clicked() {
                app.lists.report_tab = i;
            }
        }
        if matches!(app.lists.report_tab, 1 | 2 | 5) {
            ui.separator();
            ui.label("Desde");
            crate::screens::date_picker(ui, "rep_from", &mut app.lists.report_from);
            ui.label("hasta");
            crate::screens::date_picker(ui, "rep_to", &mut app.lists.report_to);
        }
    });
    ui.add_space(8.0);
    egui::ScrollArea::vertical().id_salt("rep").auto_shrink([false, false]).show(ui, |ui| match app.lists.report_tab {
        0 => pending(app, ui),
        1 => daybook(app, ui),
        2 => production(app, ui),
        3 => monthly(app, ui),
        4 => budgets(app, ui),
        5 => crate::screens::salud::captacion(app, ui),
        _ => crate::screens::herramientas::consultas(app, ui),
    });
}

fn pending(app: &mut App, ui: &mut Ui) {
    let mut rows: Vec<(u32, f64)> = app.db.patients.iter().map(|p| (p.id, app.db.patient_balance(p.id))).filter(|r| r.1 > 0.005).collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    let total: f64 = rows.iter().map(|r| r.1).sum();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Pacientes con deuda pendiente");
            right_aligned(ui, |ui| {
                ui.label(RichText::new(format!("Total pendiente: {}", money(total))).family(crate::theme::sb()).color(DANGER));
            });
        });
        let tbl = Table::new("pend", vec![("Nº", 70.0), ("Paciente", 240.0), ("Móvil", 130.0), ("Tarifa", 180.0), ("Último cobro", 120.0), ("Pendiente", 120.0)]);
        tbl.header(ui);
        let mut open = None;
        for (i, (pid, bal)) in rows.iter().enumerate() {
            let p = app.db.patient(*pid).unwrap();
            let last = app.db.payments.iter().filter(|x| x.patient_id == *pid).map(|x| x.date.date()).max();
            if tbl.row(ui, i, false, &[cell(p.history_no.to_string()), cell(p.full_name()), cell(&p.mobile), cell(app.db.tariff_name(p.tariff_id)), cell(last.map(fmt_date).unwrap_or_else(|| "nunca".into())), cellc(money(*bal), DANGER)]).clicked() {
                open = Some(*pid);
            }
        }
        if rows.is_empty() {
            empty_state(ui, "Ningún paciente debe dinero 🎉");
        }
        if let Some(pid) = open {
            app.open_patient(pid, PatTab::Cuenta);
        }
    });
}

fn in_range(app: &App, d: chrono::NaiveDate) -> bool {
    d >= app.lists.report_from && d <= app.lists.report_to
}

fn daybook(app: &mut App, ui: &mut Ui) {
    let mut es: Vec<MedEntry> = app.db.entries.iter().filter(|e| e.state == TxState::Realizado && in_range(app, e.date)).cloned().collect();
    es.sort_by_key(|e| (e.date, e.id));
    let total: f64 = es.iter().map(|e| e.amount()).sum();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Libro diario (tratamientos realizados)");
            right_aligned(ui, |ui| {
                ui.label(RichText::new(format!("{} actos · {}", es.len(), money(total))).family(crate::theme::sb()));
            });
        });
        let tbl = Table::new("diario", vec![("Fecha", 96.0), ("Nº", 60.0), ("Paciente", 200.0), ("Código", 64.0), ("Tratamiento", 240.0), ("Piezas", 90.0), ("Doctor", 110.0), ("Tarifa", 150.0), ("Importe", 100.0)]);
        tbl.header(ui);
        for (i, e) in es.iter().enumerate() {
            let p = app.db.patient(e.patient_id).cloned().unwrap_or_default();
            let teeth: Vec<String> = e.teeth.iter().map(|t| t.to_string()).collect();
            tbl.row(ui, i, false, &[cell(fmt_date(e.date)), cell(p.history_no.to_string()), cell(p.full_name()), cell(&e.code), cell(&e.description), cell(teeth.join("-")), cell(app.db.doctor_alias(e.doctor_id)), cell(app.db.tariff_name(if e.tariff_id != 0 { e.tariff_id } else { p.tariff_id })), cell(money(e.amount()))]);
        }
        if es.is_empty() {
            empty_state(ui, "Sin actos en el periodo");
        }
    });
}

fn production(app: &mut App, ui: &mut Ui) {
    let rows: Vec<(String, usize, f64, f64, [u8; 3])> = app
        .db
        .collaborators
        .iter()
        .map(|c| {
            let (mine, prod, _, fee) = app.db.settlement(c.id, app.lists.report_from, app.lists.report_to);
            (c.alias.clone(), mine.len(), prod, fee, c.color)
        })
        .collect();
    let max = rows.iter().map(|r| r.2).fold(1.0, f64::max);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Producción por colaborador y liquidación de honorarios");
        let tbl = Table::new("prod", vec![("Colaborador", 160.0), ("Actos", 80.0), ("Producción", 130.0), ("% comisión", 100.0), ("A liquidar", 130.0), ("", 360.0)]);
        tbl.header(ui);
        for (i, (n, count, prod, liq, col)) in rows.iter().enumerate() {
            let c = app.db.collaborators.get(i);
            let pct = c.map(|c| c.commission_pct).unwrap_or(0.0);
            let lab = if c.is_some_and(|c| c.deduct_lab) { " − lab." } else { "" };
            let r = tbl.row(ui, i, false, &[cell(n), cell(count.to_string()), cell(money(*prod)), cell(format!("{pct:.0} %{lab}")), cellc(money(*liq), ACCENT), cell("")]);
            let bar_r = egui::Rect::from_min_size(pos2(r.rect.min.x + 680.0, r.rect.center().y - 6.0), vec2(340.0 * (*prod / max) as f32, 12.0));
            ui.painter().rect_filled(bar_r, CornerRadius::same(4), rgb(*col));
        }
        let tp: f64 = rows.iter().map(|r| r.2).sum();
        let tl: f64 = rows.iter().map(|r| r.3).sum();
        ui.add_space(6.0);
        ui.label(RichText::new(format!("Producción total {} · honorarios {} · margen clínica {}", money(tp), money(tl), money(tp - tl))).family(crate::theme::sb()));
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("🖨 Imprimir liquidación de:");
            let docs: Vec<(u32, String)> = app.db.collaborators.iter().filter(|c| c.active).map(|c| (c.id, c.alias.clone())).collect();
            for (id, alias) in docs {
                if ui.button(&alias).clicked() {
                    let (from, to) = (app.lists.report_from, app.lists.report_to);
                    let html = app.db.settlement_html(id, from, to);
                    match crate::screens::centros::open_html(ui.ctx(), &format!("liquidacion_{id}_{}.html", to.format("%Y%m%d")), html) {
                        Ok(_) => app.notify(format!("Liquidación de {alias} abierta para imprimir.")),
                        Err(e) => app.warn(format!("No se pudo crear la liquidación: {e}")),
                    }
                }
            }
        });
    });
}

fn monthly(app: &mut App, ui: &mut Ui) {
    let today = app.today;
    let months: Vec<(i32, u32)> = (0..12)
        .rev()
        .map(|i| {
            let d = add_months(chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap(), 0);
            let total = d.year() * 12 + d.month0() as i32 - i;
            (total.div_euclid(12), total.rem_euclid(12) as u32 + 1)
        })
        .collect();
    let data: Vec<(String, f64, f64)> = months
        .iter()
        .map(|(y, m)| {
            let cob: f64 = app.db.payments.iter().filter(|p| p.date.year() == *y && p.date.month() == *m).map(|p| p.amount).sum();
            let prod: f64 = app.db.entries.iter().filter(|e| e.state == TxState::Realizado && e.date.year() == *y && e.date.month() == *m).map(|e| e.amount()).sum();
            (format!("{} {}", &month_es(*m)[..3], y % 100), cob, prod)
        })
        .collect();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Ingresos (cobros) y producción de los últimos 12 meses");
        chart(ui, &data, 300.0);
        ui.horizontal(|ui| {
            pill(ui, "■ Cobrado", ACCENT);
            pill(ui, "■ Producido", INFO);
        });
    });
}

pub fn chart(ui: &mut Ui, data: &[(String, f64, f64)], h: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let p = ui.painter();
    let max = data.iter().map(|d| d.1.max(d.2)).fold(100.0, f64::max) * 1.1;
    let plot = egui::Rect::from_min_max(rect.min + vec2(60.0, 10.0), rect.max - vec2(10.0, 26.0));
    for i in 0..=4 {
        let v = max * i as f64 / 4.0;
        let y = plot.max.y - plot.height() * i as f32 / 4.0;
        p.line_segment([pos2(plot.min.x, y), pos2(plot.max.x, y)], egui::Stroke::new(1.0, BORDER));
        p.text(pos2(plot.min.x - 6.0, y), Align2::RIGHT_CENTER, format!("{:.0} €", v), FontId::proportional(12.5), MUTED);
    }
    let n = data.len().max(1) as f32;
    let slot = plot.width() / n;
    for (i, (label, a, b)) in data.iter().enumerate() {
        let x = plot.min.x + slot * i as f32;
        let bw = (slot * 0.32).min(26.0);
        for (k, (v, c)) in [(*a, ACCENT), (*b, INFO)].iter().enumerate() {
            let hh = plot.height() * (*v / max) as f32;
            let r = egui::Rect::from_min_size(pos2(x + slot / 2.0 - bw + k as f32 * bw, plot.max.y - hh), vec2(bw - 2.0, hh));
            p.rect_filled(r, CornerRadius::same(3), *c);
        }
        p.text(pos2(x + slot / 2.0, plot.max.y + 6.0), Align2::CENTER_TOP, label, FontId::proportional(12.5), MUTED);
    }
}

fn budgets(app: &mut App, ui: &mut Ui) {
    let mut list: Vec<Budget> = app.db.budgets.iter().filter(|b| b.state == BudgetState::Pendiente).cloned().collect();
    list.sort_by_key(|b| b.date);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Presupuestos pendientes de aceptar (para seguimiento telefónico)");
        let tbl = Table::new("bp", vec![("Fecha", 96.0), ("Paciente", 220.0), ("Móvil", 130.0), ("Título", 260.0), ("Días", 70.0), ("Importe", 110.0)]);
        tbl.header(ui);
        let mut open = None;
        for (i, b) in list.iter().enumerate() {
            let p = app.db.patient(b.patient_id).cloned().unwrap_or_default();
            let days = (app.today - b.date).num_days();
            if tbl.row(ui, i, false, &[cell(fmt_date(b.date)), cell(p.full_name()), cell(&p.mobile), cell(&b.title), cellc(days.to_string(), if days > 30 { DANGER } else { TEXT }), cell(money(b.total()))]).clicked() {
                open = Some(b.id);
            }
        }
        if list.is_empty() {
            empty_state(ui, "No hay presupuestos pendientes");
        }
        if let Some(id) = open {
            app.dialog = Dialog::Budget(id);
        }
    });
}
