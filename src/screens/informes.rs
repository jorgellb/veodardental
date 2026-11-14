//! Informes: pendientes de cobro, libro diario, producción y liquidación
//! de colaboradores, ingresos mensuales, presupuestos pendientes y el
//! resumen económico. Los totales salen de `crate::informes`, que define qué
//! entra en cada uno, con los mismos filtros de periodo, centro y profesional.
//! El cuadro de mando, de `crate::cuadro`, con sus fórmulas.

use crate::app::{App, Dialog, PatTab};
use crate::informes::Filter;
use crate::model::*;
use crate::theme::*;
use chrono::Datelike;
use egui::{Align2, CornerRadius, FontId, RichText, Sense, Ui, pos2, vec2};

const TABS: [&str; 10] = ["Pendientes de cobro", "Libro diario", "Producción", "Ingresos", "Presupuestos", "Captación", "Consultas", "Resumen económico", "Cuadro de mando", "Grupo de clínicas"];

/// El filtro de los informes con lo elegido en la pantalla.
fn filter(app: &App) -> Filter {
    Filter { from: app.lists.report_from, to: app.lists.report_to, center: app.lists.report_center, doctor: app.lists.report_doctor }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    title(ui, "Informes", "Listados de gestión de la clínica");
    ui.horizontal_wrapped(|ui| {
        for (i, t) in TABS.iter().enumerate() {
            if ui.selectable_label(app.lists.report_tab == i, *t).clicked() {
                app.lists.report_tab = i;
            }
        }
    });
    let tab = app.lists.report_tab;
    ui.horizontal_wrapped(|ui| {
        if matches!(tab, 1 | 2 | 5 | 7 | 8 | 9) {
            ui.label("Desde");
            crate::screens::date_picker(ui, "rep_from", &mut app.lists.report_from);
            ui.label("hasta");
            crate::screens::date_picker(ui, "rep_to", &mut app.lists.report_to);
        }
        if tab == 0 {
            ui.label("Deuda a");
            crate::screens::date_picker(ui, "rep_debt", &mut app.lists.report_debt_at);
        }
        if matches!(tab, 1 | 2 | 3 | 7 | 8) && app.db.multi_center() {
            ui.add_space(8.0);
            ui.label("Centro");
            // quien trabaja solo en unos centros, ve los suyos (sin «Todos»)
            let mine = app.user_id.and_then(|id| app.db.user(id)).map(|u| u.centers.iter().map(|c| crate::model::cid(*c)).collect::<Vec<u32>>()).unwrap_or_default();
            let centers: Vec<(u32, String)> = app.db.centers.iter().filter(|c| c.active && (mine.is_empty() || mine.contains(&c.id))).map(|c| (c.id, c.name.clone())).collect();
            if !mine.is_empty() && app.lists.report_center.is_none_or(|c| !mine.contains(&crate::model::cid(c))) {
                app.lists.report_center = centers.first().map(|x| x.0);
            }
            let cur = app.lists.report_center.and_then(|c| centers.iter().find(|x| x.0 == c)).map(|x| x.1.clone()).unwrap_or_else(|| "Todos".into());
            crate::theme::combo("rep_center").width(200.0).selected_text(cur).show_ui(ui, |ui| {
                if mine.is_empty() {
                    ui.selectable_value(&mut app.lists.report_center, None, "Todos");
                }
                for (id, n) in &centers {
                    ui.selectable_value(&mut app.lists.report_center, Some(*id), n);
                }
            });
        }
        if matches!(tab, 1 | 7 | 8) {
            ui.add_space(8.0);
            ui.label("Profesional");
            let docs: Vec<(u32, String)> = app.db.collaborators.iter().filter(|c| c.active || Some(c.id) == app.lists.report_doctor).map(|c| (c.id, c.alias.clone())).collect();
            let cur = app.lists.report_doctor.and_then(|d| docs.iter().find(|x| x.0 == d)).map(|x| x.1.clone()).unwrap_or_else(|| "Todos".into());
            crate::theme::combo("rep_doctor").width(180.0).selected_text(cur).show_ui(ui, |ui| {
                ui.selectable_value(&mut app.lists.report_doctor, None, "Todos");
                for (id, n) in &docs {
                    ui.selectable_value(&mut app.lists.report_doctor, Some(*id), n);
                }
            });
        }
    });
    ui.add_space(8.0);
    egui::ScrollArea::vertical().id_salt("rep").auto_shrink([false, false]).show(ui, |ui| match tab {
        0 => pending(app, ui),
        1 => daybook(app, ui),
        2 => production(app, ui),
        3 => monthly(app, ui),
        4 => budgets(app, ui),
        5 => crate::screens::salud::captacion(app, ui),
        6 => crate::screens::herramientas::consultas(app, ui),
        7 => summary(app, ui),
        9 => crate::screens::grupo::view(app, ui),
        _ => dashboard(app, ui),
    });
}

/// Pacientes que deben a la fecha elegida (lo realizado hasta ese día menos
/// lo cobrado hasta ese día). Los anticipos se dicen aparte: no compensan.
fn pending(app: &mut App, ui: &mut Ui) {
    let at = app.lists.report_debt_at;
    let rows: Vec<(Patient, f64)> = app.db.balances_at(at).into_iter().filter(|r| r.1.0 > 0).filter_map(|(pid, b)| Some((app.db.patient(pid)?.clone(), b.eur()))).collect();
    let debt = app.db.report_debt(at);
    // último cobro de cada paciente, de una pasada
    let mut last: std::collections::HashMap<u32, chrono::NaiveDate> = std::collections::HashMap::new();
    for p in app.db.payments.iter().filter(|p| p.date.date() <= at) {
        let d = last.entry(p.patient_id).or_insert(p.date.date());
        *d = (*d).max(p.date.date());
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            section(ui, &format!("Pacientes con deuda pendiente a {}", fmt_date(at)));
            right_aligned(ui, |ui| {
                ui.label(RichText::new(format!("Total pendiente: {}", debt.owed)).family(crate::theme::sb()).color(DANGER));
            });
        });
        muted(ui, format!("{} pacientes deben {} · otros tienen {} a favor (anticipos), que no se restan de la deuda.", debt.debtors, debt.owed, debt.credit));
        let tbl = Table::new("pend", vec![("Nº", 70.0), ("Paciente", 240.0), ("Móvil", 130.0), ("Tarifa", 180.0), ("Último cobro", 120.0), ("Pendiente", 120.0)]);
        tbl.header(ui);
        let mut open = None;
        for (i, (p, bal)) in rows.iter().enumerate() {
            let lastp = last.get(&p.id).map(|d| fmt_date(*d)).unwrap_or_else(|| "nunca".into());
            if tbl.row(ui, i, false, &[cell(p.history_no.to_string()), cell(p.full_name()), cell(&p.mobile), cell(app.db.tariff_name(p.tariff_id)), cell(lastp), cellc(money(*bal), DANGER)]).clicked() {
                open = Some(p.id);
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

fn daybook(app: &mut App, ui: &mut Ui) {
    let f = filter(app);
    let es: Vec<MedEntry> = app.db.report_acts(&f).into_iter().cloned().collect();
    let prod = app.db.report_production(&f);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            section(ui, "Libro diario (tratamientos realizados)");
            right_aligned(ui, |ui| {
                ui.label(RichText::new(format!("{} actos · {}", prod.acts, prod.total())).family(crate::theme::sb()));
            });
        });
        muted(ui, format!("Paciente {} · mutua {} (el baremo que paga la compañía)", prod.patient, prod.insurer));
        let tbl = Table::new("diario", vec![("Fecha", 96.0), ("Nº", 60.0), ("Paciente", 200.0), ("Código", 64.0), ("Tratamiento", 220.0), ("Piezas", 80.0), ("Doctor", 110.0), ("Tarifa", 140.0), ("Paciente", 96.0), ("Mutua", 90.0)]);
        tbl.header(ui);
        for (i, e) in es.iter().enumerate() {
            let p = app.db.patient(e.patient_id).cloned().unwrap_or_default();
            let teeth: Vec<String> = e.teeth.iter().map(|t| t.to_string()).collect();
            let ins = app.db.insurer_part(e);
            tbl.row(
                ui,
                i,
                false,
                &[
                    cell(fmt_date(e.date)),
                    cell(p.history_no.to_string()),
                    cell(p.full_name()),
                    cell(&e.code),
                    cell(&e.description),
                    cell(teeth.join("-")),
                    cell(app.db.doctor_alias(e.doctor_id)),
                    cell(app.db.tariff_name(if e.tariff_id != 0 { e.tariff_id } else { p.tariff_id })),
                    cell(money(e.amount())),
                    cell(if ins.is_zero() { String::new() } else { ins.to_string() }),
                ],
            );
        }
        if es.is_empty() {
            empty_state(ui, "Sin actos en el periodo");
        }
    });
}

fn production(app: &mut App, ui: &mut Ui) {
    let f = Filter { doctor: None, ..filter(app) };
    let by_center = f.center.is_some();
    // (alias, producción, a liquidar, % y base, color)
    let rows: Vec<(String, crate::informes::Production, f64, String, [u8; 3])> = app
        .db
        .collaborators
        .iter()
        .map(|c| {
            let prod = app.db.report_production(&Filter { doctor: Some(c.id), ..f });
            let (_, _, _, fee) = app.db.settlement(c.id, f.from, f.to);
            let lab = if c.deduct_lab { " − lab." } else { "" };
            let ins = if c.commission_on_insurer { " (con mutua)" } else { "" };
            (c.alias.clone(), prod, fee, format!("{:.0} %{lab}{ins}", c.commission_pct), c.color)
        })
        .collect();
    let max = rows.iter().map(|r| r.1.total().eur()).fold(1.0, f64::max);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Producción por colaborador y liquidación de honorarios");
        muted(
            ui,
            if by_center {
                "La producción es la del centro elegido; la liquidación de honorarios es siempre de todos los centros."
            } else {
                "Producción: lo que paga el paciente más lo que paga la mutua. La comisión es sobre lo que paga el paciente, salvo en los colaboradores marcados «con mutua»."
            },
        );
        let tbl = Table::new("prod", vec![("Colaborador", 150.0), ("Actos", 64.0), ("Paciente", 110.0), ("Mutua", 100.0), ("Total", 110.0), ("Comisión", 130.0), ("A liquidar", 110.0), ("", 200.0)]);
        tbl.header(ui);
        let k = tbl.scale(ui);
        for (i, (n, prod, liq, pct, col)) in rows.iter().enumerate() {
            let fee = if by_center { "—".to_string() } else { money(*liq) };
            let r = tbl.row(ui, i, false, &[cell(n), cell(prod.acts.to_string()), cell(prod.patient.to_string()), cell(prod.insurer.to_string()), cell(prod.total().to_string()), cell(pct), cellc(fee, ACCENT), cell("")]);
            let x0 = r.rect.min.x + 14.0 + k * (150.0 + 64.0 + 110.0 + 100.0 + 110.0 + 130.0 + 110.0);
            let bar_r = egui::Rect::from_min_size(pos2(x0, r.rect.center().y - 6.0), vec2(((r.rect.max.x - x0 - 8.0).max(0.0)) * (prod.total().eur() / max) as f32, 12.0));
            ui.painter().rect_filled(bar_r, CornerRadius::same(4), rgb(*col));
        }
        let total: crate::cents::Cents = rows.iter().map(|r| r.1.total()).sum();
        let fees: f64 = rows.iter().map(|r| r.2).sum();
        ui.add_space(6.0);
        if by_center {
            ui.label(RichText::new(format!("Producción del centro {total}")).family(crate::theme::sb()));
        } else {
            ui.label(RichText::new(format!("Producción total {} · honorarios {} · margen clínica {}", total, money(fees), money(total.eur() - fees))).family(crate::theme::sb()));
        }
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
    let center = app.lists.report_center;
    let data: Vec<(String, f64, f64)> = (0..12)
        .rev()
        .map(|i| {
            let total = today.year() * 12 + today.month0() as i32 - i;
            let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
            let first = chrono::NaiveDate::from_ymd_opt(y, m, 1).unwrap();
            let last = add_months(first, 1) - chrono::Duration::days(1);
            let f = Filter { center, ..Filter::period(first, last) };
            (format!("{} {}", &month_es(m)[..3], y % 100), app.db.report_collected(&f).net.eur(), app.db.report_production(&f).total().eur())
        })
        .collect();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Ingresos (cobros) y producción de los últimos 12 meses");
        muted(ui, "Cobrado: neto de anulaciones y devoluciones. Producido: lo que paga el paciente más lo que paga la mutua.");
        chart(ui, &data, 300.0);
        ui.horizontal(|ui| {
            pill(ui, "■ Cobrado", ACCENT);
            pill(ui, "■ Producido", INFO);
        });
    });
}

/// Resumen económico del periodo, con qué entra en cada total.
fn summary(app: &mut App, ui: &mut Ui) {
    let f = filter(app);
    let c = app.db.report_collected(&f);
    let p = app.db.report_production(&f);
    let inv = app.db.report_invoiced(&f);
    let debt = app.db.report_debt(f.to);
    let ins = app.db.report_insurers(&f);
    let w = kpi_w(ui, 4);
    ui.horizontal(|ui| {
        kpi(ui, w, "Cobrado (neto)", &c.net.to_string(), &format!("{} movimientos", c.count), ACCENT);
        ui.add_space(4.0);
        kpi(ui, w, "Producción", &p.total().to_string(), &format!("{} actos", p.acts), INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Facturado (neto)", &inv.net.to_string(), &format!("{} facturas · {} rectificativas", inv.invoices, inv.rectifying), OK);
        ui.add_space(4.0);
        kpi(ui, w, &format!("Deuda a {}", fmt_date(f.to)), &debt.owed.to_string(), &format!("{} pacientes", debt.debtors), DANGER);
    });
    ui.add_space(8.0);
    let line = |ui: &mut Ui, label: &str, v: crate::cents::Cents| {
        ui.horizontal(|ui| {
            ui.label(label);
            right_aligned(ui, |ui| {
                ui.label(RichText::new(v.to_string()).family(crate::theme::sb()));
            });
        });
    };
    let half = (ui.available_width() - 12.0) / 2.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(half);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Cobrado");
                muted(ui, "Cobros con fecha en el periodo (y del centro elegido). Las anulaciones y devoluciones restan el día en que se hacen. El profesional no cuenta: los cobros no son de un profesional.");
                for (m, v) in c.by_method.iter().filter(|(m, v)| !v.is_zero() || crate::informes::METHODS[..5].contains(&m.as_str())) {
                    line(ui, m, *v);
                }
                ui.separator();
                line(ui, "Cobros", c.charged);
                line(ui, "Anulaciones", c.cancelled);
                line(ui, "Devoluciones y recibos devueltos", c.returned);
                line(ui, "Neto", c.net);
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Facturado");
                muted(ui, "Facturas con fecha de expedición en el periodo; las rectificativas, en negativo. Las facturas no son de un centro ni de un profesional.");
                line(ui, &format!("{} facturas", inv.invoices), inv.issued);
                line(ui, &format!("{} rectificativas", inv.rectifying), inv.rectified);
                line(ui, "Base imponible", inv.base);
                line(ui, "IVA", inv.vat);
                line(ui, "Neto", inv.net);
            });
        });
        ui.vertical(|ui| {
            ui.set_width(half);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Producción");
                muted(ui, "Tratamientos realizados con fecha en el periodo, en el centro donde se hicieron y del profesional elegido. Lo previsto no entra.");
                line(ui, "Lo que paga el paciente", p.patient);
                line(ui, "Lo que paga la mutua (baremo)", p.insurer);
                line(ui, "Total", p.total());
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Mutuas");
                muted(ui, "Liquidaciones con fecha en el periodo, cobradas en el periodo, pendientes de cobro hoy y baremo de actos del periodo aún sin liquidar.");
                line(ui, "Liquidado", ins.claimed);
                line(ui, "Cobrado de las mutuas", ins.collected);
                line(ui, "Pendiente de cobro", ins.pending);
                line(ui, "Sin liquidar todavía", ins.unclaimed);
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, &format!("Deuda a {}", fmt_date(f.to)));
                muted(ui, "Por paciente, lo realizado hasta ese día menos lo cobrado hasta ese día. Los anticipos no restan de la deuda.");
                line(ui, &format!("Deben ({} pacientes)", debt.debtors), debt.owed);
                line(ui, "Saldo a favor de pacientes", debt.credit);
            });
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

/// Fila del cuadro de mando: nombre, valor, anterior, objetivo, si se cumple y fórmula.
type KpiRow = (&'static str, String, String, String, Option<bool>, String);

fn pct_text(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.1} %").replace('.', ",")).unwrap_or_else(|| "—".into())
}

/// Cuadro de mando: cada indicador con su fórmula, el periodo anterior y el
/// objetivo de la clínica (ver `crate::cuadro`).
fn dashboard(app: &mut App, ui: &mut Ui) {
    use crate::cuadro::{meets, previous};
    let f = filter(app);
    let k = app.db.dashboard(&f);
    let before = app.db.dashboard(&previous(&f));
    let g = app.db.config.kpi_goals.clone();
    let per_hour = |c: Option<crate::cents::Cents>| c.map(|c| c.to_string()).unwrap_or_else(|| "—".into());
    let per_hour_eur = |c: Option<crate::cents::Cents>| c.map(|c| c.0 as f64 / 100.0);
    let rows: Vec<KpiRow> = vec![
        (
            "Ocupación de profesionales",
            pct_text(k.doctors_total.pct()),
            pct_text(before.doctors_total.pct()),
            pct_text(g.occupancy_pct),
            meets(k.doctors_total.pct(), g.occupancy_pct, true),
            format!("{} min citados ÷ {} min de horario (sin festivos ni bloqueos; las anuladas y reprogramadas no cuentan)", k.doctors_total.num, k.doctors_total.den),
        ),
        ("Ocupación de gabinetes", pct_text(k.boxes_total.pct()), pct_text(before.boxes_total.pct()), "—".into(), None, format!("{} min citados ÷ {} min que abre la clínica, sumando los gabinetes", k.boxes_total.num, k.boxes_total.den)),
        (
            "Ausencias («no acude»)",
            pct_text(k.no_shows.rate().pct()),
            pct_text(before.no_shows.rate().pct()),
            g.no_show_pct.map(|x| format!("≤ {}", pct_text(Some(x)))).unwrap_or_else(|| "—".into()),
            meets(k.no_shows.rate().pct(), g.no_show_pct, false),
            format!("{} «no acude» ÷ ({} finalizadas + {} «no acude»); {} sin cerrar no entran", k.no_shows.missed, k.no_shows.attended, k.no_shows.missed, k.no_shows.unclosed),
        ),
        (
            "Aceptación de presupuestos",
            pct_text(k.acceptance.count.pct()),
            pct_text(before.acceptance.count.pct()),
            pct_text(g.acceptance_pct),
            meets(k.acceptance.count.pct(), g.acceptance_pct, true),
            format!("{} aceptados o cerrados ÷ {} presentados en el periodo ({} rechazados, {} pendientes)", k.acceptance.count.num, k.acceptance.count.den, k.acceptance.rejected, k.acceptance.pending),
        ),
        (
            "Aceptación en importe",
            pct_text(k.acceptance.amount.pct()),
            pct_text(before.acceptance.amount.pct()),
            "—".into(),
            None,
            format!("{} aceptados ÷ {} presentados", crate::cents::Cents(k.acceptance.amount.num), crate::cents::Cents(k.acceptance.amount.den)),
        ),
        (
            "Producción por hora de sillón",
            per_hour(k.chair_total.per_hour()),
            per_hour(before.chair_total.per_hour()),
            g.per_hour_eur.map(|x| crate::cents::Cents::from_eur(x).to_string()).unwrap_or_else(|| "—".into()),
            meets(per_hour_eur(k.chair_total.per_hour()), g.per_hour_eur, true),
            format!("{} de producción ÷ {} min de citas finalizadas", k.chair_total.production, k.chair_total.minutes),
        ),
        (
            "Recalls que terminan en cita",
            pct_text(k.recalls.rate().pct()),
            pct_text(before.recalls.rate().pct()),
            pct_text(g.recall_pct),
            meets(k.recalls.rate().pct(), g.recall_pct, true),
            format!("{} citados ÷ {} con fecha prevista en el periodo ({} rechazados)", k.recalls.booked, k.recalls.due, k.recalls.rejected),
        ),
    ];
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Indicadores");
        muted(ui, "Cada valor dice cómo se calcula. Sin denominador se muestra «—»: no es un 0 %. El periodo anterior es el de la misma duración justo antes.");
        ui.add_space(4.0);
        // la primera columna, con su fórmula, ocupa lo que dejan las otras
        let first = (ui.available_width() - 420.0).max(280.0);
        // con etiquetas (no pintado): se leen con un lector de pantalla
        egui::Grid::new("kpi").num_columns(5).striped(true).spacing([18.0, 6.0]).show(ui, |ui| {
            for h in ["Indicador", "Valor", "Anterior", "Objetivo", ""] {
                ui.label(RichText::new(h).font(sbf(13.0)).color(MUTED));
            }
            ui.end_row();
            for (name, v, prev, goal, ok, how) in rows {
                ui.vertical(|ui| {
                    ui.set_width(first);
                    ui.label(RichText::new(name).font(sbf(14.0)));
                    ui.label(RichText::new(how).size(12.5).color(MUTED));
                });
                ui.label(RichText::new(v).font(sbf(15.0)));
                ui.label(prev);
                ui.label(goal);
                match ok {
                    Some(true) => ui.label(RichText::new("Cumple").color(OK)),
                    Some(false) => ui.label(RichText::new("No cumple").color(DANGER)),
                    None => ui.label(""),
                };
                ui.end_row();
            }
        });
    });
    ui.add_space(8.0);
    let half = (ui.available_width() - 12.0) / 2.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(half);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Por profesional");
                muted(ui, "Su horario no es de un centro: el filtro de centro no cambia su ocupación.");
                let tbl = Table::new("kpi_doc", vec![("Profesional", 160.0), ("Ocupación", 90.0), ("Citado / horario", 140.0), ("Por hora", 100.0)]);
                tbl.header(ui);
                for (i, o) in k.doctors.iter().enumerate() {
                    let name = app.db.doctor(o.id).map(|c| c.alias.clone()).unwrap_or_else(|| format!("#{}", o.id));
                    let hour = k.chair.iter().find(|c| c.id == o.id).and_then(|c| c.per_hour());
                    tbl.row(ui, i, false, &[cell(name), cell(pct_text(o.minutes.pct())), cell(format!("{} / {} min", o.minutes.num, o.minutes.den)), cell(per_hour(hour))]);
                }
            });
        });
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.set_width(half);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Por gabinete");
                let tbl = Table::new("kpi_box", vec![("Gabinete", 160.0), ("Ocupación", 90.0), ("Citado / abierto", 140.0)]);
                tbl.header(ui);
                for (i, o) in k.boxes.iter().enumerate() {
                    let name = app.db.boxes.iter().find(|b| b.id == o.id).map(|b| b.name.clone()).unwrap_or_default();
                    tbl.row(ui, i, false, &[cell(name), cell(pct_text(o.minutes.pct())), cell(format!("{} / {} min", o.minutes.num, o.minutes.den))]);
                }
            });
        });
    });
    ui.add_space(8.0);
    crate::screens::encuestas::results(app, ui, f.from, f.to);
    ui.add_space(8.0);
    goals_card(app, ui);
}

/// Objetivos de la clínica (cada uno opcional).
fn goals_card(app: &mut App, ui: &mut Ui) {
    let before = app.db.config.kpi_goals.clone();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Objetivos");
        muted(ui, "Los pone la clínica; sin objetivo no se marca nada.");
        let g = &mut app.db.config.kpi_goals;
        ui.horizontal_wrapped(|ui| {
            for (label, v, max, unit) in [
                ("Ocupación mínima", &mut g.occupancy_pct, 100.0, "%"),
                ("Ausencias máximas", &mut g.no_show_pct, 100.0, "%"),
                ("Aceptación mínima", &mut g.acceptance_pct, 100.0, "%"),
                ("Producción por hora", &mut g.per_hour_eur, 100_000.0, "€"),
                ("Recalls citados", &mut g.recall_pct, 100.0, "%"),
            ] {
                let mut on = v.is_some();
                if ui.checkbox(&mut on, label).changed() {
                    *v = on.then_some(if unit == "%" { 50.0 } else { 60.0 });
                }
                if let Some(x) = v.as_mut() {
                    ui.add(egui::DragValue::new(x).range(0.0..=max).speed(1.0).suffix(format!(" {unit}")));
                }
                ui.add_space(12.0);
            }
        });
    });
    if app.db.config.kpi_goals != before {
        app.changed();
    }
}
