//! Libro de gastos: facturas de proveedores, laboratorio, alquiler,
//! nóminas…; IVA soportado, pendientes de pago y resultado del periodo.

use crate::app::App;
use crate::finance::*;
use crate::model::*;
use crate::theme::*;
use chrono::Datelike;
use egui::{RichText, Ui};

pub struct GastosState {
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
    pub category: Option<String>,
    pub selected: Option<u32>,
}

impl Default for GastosState {
    fn default() -> Self {
        let t = today();
        GastosState { from: chrono::NaiveDate::from_ymd_opt(t.year(), t.month(), 1).unwrap_or(t) - chrono::Duration::days(90), to: t, category: None, selected: None }
    }
}

pub fn screen(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        title(ui, "Gastos", "Facturas recibidas, IVA soportado y resultado de la clínica");
        right_aligned(ui, |ui| {
            if primary(ui, "＋ Nuevo gasto").clicked() {
                let id = app.db.new_id();
                app.db.expenses.push(Expense {
                    id,
                    date: app.today,
                    supplier: String::new(),
                    concept: "Nuevo gasto".into(),
                    category: "Otros".into(),
                    base: 0.0,
                    vat_pct: 21.0,
                    invoice_no: String::new(),
                    method: "Transferencia".into(),
                    paid: false,
                    due: Some(app.today + chrono::Duration::days(30)),
                });
                app.gastos.selected = Some(id);
                app.changed();
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
        ui.horizontal_wrapped(|ui| {
            ui.label("Desde");
            crate::screens::date_picker(ui, "g_from", &mut app.gastos.from);
            ui.label("hasta");
            crate::screens::date_picker(ui, "g_to", &mut app.gastos.to);
            ui.separator();
            if ui.selectable_label(app.gastos.category.is_none(), "Todas").clicked() {
                app.gastos.category = None;
            }
            for c in EXPENSE_CATEGORIES {
                if ui.selectable_label(app.gastos.category.as_deref() == Some(c), c).clicked() {
                    app.gastos.category = Some(c.into());
                }
            }
        });
    });
    ui.add_space(8.0);
    let (from, to) = (app.gastos.from, app.gastos.to);
    let mut list: Vec<Expense> = app.db.expenses_between(from, to).into_iter().filter(|e| app.gastos.category.as_ref().map(|c| &e.category == c).unwrap_or(true)).cloned().collect();
    list.sort_by_key(|e| std::cmp::Reverse((e.date, e.id)));
    let total: f64 = list.iter().map(|e| e.total()).sum();
    let vat: f64 = list.iter().map(|e| e.vat()).sum();
    let unpaid: f64 = app.db.expenses.iter().filter(|e| !e.paid).map(|e| e.total()).sum();
    let income: f64 = app.db.payments.iter().filter(|p| p.date.date() >= from && p.date.date() <= to).map(|p| p.amount).sum();
    let w = kpi_w(ui, 4);
    ui.horizontal(|ui| {
        kpi(ui, w, "Gastos del periodo", &money(total), &format!("{} facturas", list.len()), DANGER);
        ui.add_space(4.0);
        kpi(ui, w, "IVA soportado", &money(vat), "deducible según actividad", INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Pendiente de pago", &money(unpaid), "todas las fechas", WARN);
        ui.add_space(4.0);
        let res = income - total;
        kpi(ui, w, "Resultado (cobros − gastos)", &money(res), &format!("cobrado {}", money(income)), if res >= 0.0 { OK } else { DANGER });
    });
    ui.add_space(8.0);
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    let side = 380.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail - side - 24.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                let tbl = Table::new("exp", vec![("Fecha", 96.0), ("Proveedor", 180.0), ("Concepto", 200.0), ("Categoría", 120.0), ("Total", 100.0), ("Estado", 100.0)]);
                tbl.header(ui);
                if list.is_empty() {
                    empty_state(ui, "Sin gastos en el periodo.");
                }
                let mut pick = None;
                egui::ScrollArea::vertical().id_salt("expl").max_height(330.0).show(ui, |ui| {
                    for (i, e) in list.iter().enumerate() {
                        let late = !e.paid && e.due.map(|d| d < app.today).unwrap_or(false);
                        let cells = [
                            cell(fmt_date(e.date)),
                            cell(&e.supplier),
                            cell(&e.concept),
                            cell(&e.category),
                            cell(money(e.total())),
                            cellc(
                                if e.paid {
                                    "Pagado"
                                } else if late {
                                    "Vencido"
                                } else {
                                    "Pendiente"
                                },
                                if e.paid {
                                    OK
                                } else if late {
                                    DANGER
                                } else {
                                    WARN
                                },
                            ),
                        ];
                        if tbl.row(ui, i, app.gastos.selected == Some(e.id), &cells).clicked() {
                            pick = Some(e.id);
                        }
                    }
                });
                if pick.is_some() {
                    app.gastos.selected = pick;
                }
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Ingresos y gastos de los últimos 6 meses");
                let t = app.today;
                let data: Vec<(String, f64, f64)> = (0..6)
                    .rev()
                    .map(|i| {
                        let total = t.year() * 12 + t.month0() as i32 - i;
                        let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
                        let inc: f64 = app.db.payments.iter().filter(|p| p.date.year() == y && p.date.month() == m).map(|p| p.amount).sum();
                        let exp: f64 = app.db.expenses.iter().filter(|e| e.date.year() == y && e.date.month() == m).map(|e| e.total()).sum();
                        (month_es(m)[..3].to_string(), inc, exp)
                    })
                    .collect();
                crate::screens::informes::chart(ui, &data, 190.0);
                ui.horizontal(|ui| {
                    pill(ui, "■ Cobrado", ACCENT);
                    pill(ui, "■ Gastos", INFO);
                });
            });
        });
        ui.vertical(|ui| {
            ui.set_width(side);
            editor(app, ui);
        });
    });
}

fn editor(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let Some(id) = app.gastos.selected else {
            section(ui, "Gasto");
            muted(ui, "Seleccione un gasto o cree uno nuevo. Los pedidos recibidos y los trabajos de laboratorio se apuntan solos.");
            return;
        };
        let mut delete = false;
        if let Some(e) = app.db.expenses.iter_mut().find(|e| e.id == id) {
            let before = serde_json::to_string(e).unwrap_or_default();
            section(ui, "Editar gasto");
            ui.horizontal(|ui| {
                lab(ui, 70.0, RichText::new("Fecha").color(MUTED));
                crate::screens::date_picker(ui, "e_date", &mut e.date);
            });
            field(ui, "Proveedor", &mut e.supplier, 300.0);
            field(ui, "Concepto", &mut e.concept, 300.0);
            ui.horizontal(|ui| {
                field(ui, "Nº factura", &mut e.invoice_no, 120.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Categoría").size(13.5).color(MUTED));
                    crate::theme::combo("e_cat").width(150.0).selected_text(&e.category).show_ui(ui, |ui| {
                        for c in EXPENSE_CATEGORIES {
                            ui.selectable_value(&mut e.category, c.to_string(), c);
                        }
                    });
                });
            });
            ui.horizontal(|ui| {
                ui.label("Base");
                ui.add(egui::DragValue::new(&mut e.base).range(-100000.0..=1000000.0).speed(1.0).suffix(" €"));
                ui.label("IVA");
                crate::theme::combo("e_vat").width(70.0).selected_text(format!("{:.0} %", e.vat_pct)).show_ui(ui, |ui| {
                    for v in [0.0, 4.0, 10.0, 21.0] {
                        ui.selectable_value(&mut e.vat_pct, v, format!("{v:.0} %"));
                    }
                });
            });
            ui.label(RichText::new(format!("Total {}", money(e.total()))).family(sb()).size(18.0).color(ACCENT));
            ui.horizontal(|ui| {
                ui.checkbox(&mut e.paid, "Pagado");
                crate::theme::combo("e_met").width(140.0).selected_text(&e.method).show_ui(ui, |ui| {
                    for m in ["Transferencia", "Domiciliación", "Tarjeta", "Efectivo"] {
                        ui.selectable_value(&mut e.method, m.to_string(), m);
                    }
                });
            });
            if !e.paid {
                let mut d = e.due.unwrap_or(e.date);
                ui.horizontal(|ui| {
                    lab(ui, 90.0, RichText::new("Vencimiento").color(MUTED));
                    if crate::screens::date_picker(ui, "e_due", &mut d) {
                        e.due = Some(d);
                    }
                });
            }
            if serde_json::to_string(e).unwrap_or_default() != before {
                app.dirty = true;
            }
            if ui.button("Borrar gasto").clicked() {
                delete = true;
            }
        }
        if delete {
            app.db.expenses.retain(|e| e.id != id);
            app.gastos.selected = None;
            app.changed();
        }
    });
}

fn export_csv(app: &App) -> Result<String, String> {
    let dir = crate::paths::documents().join(crate::brand::DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("gastos_{}_{}.csv", app.gastos.from.format("%Y%m%d"), app.gastos.to.format("%Y%m%d")));
    let n = |v: f64| format!("{v:.2}").replace('.', ",");
    let mut s = String::from("Fecha;Proveedor;Nº factura;Concepto;Categoría;Base;IVA %;IVA;Total;Pagado\n");
    for e in app.db.expenses_between(app.gastos.from, app.gastos.to) {
        s.push_str(&format!(
            "{};{};{};{};{};{};{:.0};{};{};{}\n",
            fmt_date(e.date),
            e.supplier.replace(';', ","),
            e.invoice_no,
            e.concept.replace(';', ","),
            e.category,
            n(e.base),
            e.vat_pct,
            n(e.vat()),
            n(e.total()),
            if e.paid { "Sí" } else { "No" }
        ));
    }
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}
