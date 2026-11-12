//! Tarifas (mutuas/compañías) y catálogo de tratamientos.

use crate::app::App;
use crate::model::*;
use crate::theme::*;
use egui::{RichText, Ui};

pub fn show(app: &mut App, ui: &mut Ui) {
    title(ui, "Tarifas y tratamientos", "Compañías (mutuas), precios por tarifa y catálogo de actos");
    let w = ui.available_width();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(w * 0.57);
            tariffs(app, ui);
        });
        ui.vertical(|ui| {
            ui.set_width(w * 0.43 - 14.0);
            treatments(app, ui);
        });
    });
}

fn tariffs(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Tarifas / Mutuas");
            right_aligned(ui, |ui| {
                if ui.button("📥 Importar").on_hover_text("Importar tratamientos y tarifas desde ficheros Excel/CSV").clicked() {
                    app.imp.kind = crate::extra::ImportKind::Tratamientos;
                    app.screen = crate::app::Screen::Config;
                }
                if ui.button("＋ Nueva tarifa").clicked() {
                    let id = app.db.new_id();
                    app.db.tariffs.push(Tariff {
                        id,
                        name: format!("Nueva tarifa {id}"),
                        is_insurer: true,
                        discount_pct: 0.0,
                        prices: vec![],
                        phone: String::new(),
                        notes: String::new(),
                        valid_from: app.today,
                        valid_to: None,
                        inactive: false,
                    });
                    app.lists.tariff_sel = id;
                    app.changed();
                }
            });
        });
        let list: Vec<(u32, String, usize, bool)> = app
            .db
            .tariffs
            .iter()
            .map(|t| (t.id, t.name.clone(), app.db.patients.iter().filter(|p| p.tariff_id == t.id).count(), t.inactive))
            .collect();
        ui.horizontal_wrapped(|ui| {
            for (id, n, count, inactive) in &list {
                let txt = format!("{n} ({count}){}", if *inactive { " · inactiva" } else { "" });
                if ui.selectable_label(app.lists.tariff_sel == *id, txt).clicked() {
                    app.lists.tariff_sel = *id;
                }
            }
        });
        ui.separator();
        let sel = app.lists.tariff_sel;
        let base: Vec<Treatment> = app.db.treatments_for(sel).filter(|t| t.odg != OdgKind::Ausente).cloned().collect();
        let Some(t) = app.db.tariffs.iter_mut().find(|t| t.id == sel) else { return };
        let before = serde_json::to_string(t).unwrap_or_default();
        ui.horizontal(|ui| {
            field(ui, "Nombre", &mut t.name, 220.0);
            field(ui, "Teléfono", &mut t.phone, 120.0);
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut t.is_insurer, "Es compañía aseguradora");
            ui.checkbox(&mut t.inactive, "Inactiva");
            ui.label("Descuento general sobre la tarifa privada:");
            ui.add(egui::DragValue::new(&mut t.discount_pct).range(0.0..=100.0).suffix(" %"));
        });
        field(ui, "Observaciones", &mut t.notes, 440.0);
        ui.add_space(6.0);
        muted(ui, "Precio por tratamiento. Los marcados como «específico» no siguen el descuento general.");
        let ins = t.is_insurer;
        if ins {
            muted(ui, "«Paga cía.» es lo que la compañía abona a la clínica por cada acto: se usa en las liquidaciones a mutuas (pantalla Facturas).");
        }
        let mut cols = vec![("Código", 54.0), ("Tratamiento", 188.0), ("Privado", 84.0), ("Paga paciente", 104.0)];
        if ins {
            cols.push(("Paga cía.", 104.0));
        }
        cols.push(("", 40.0));
        let tbl = Table::new("tp", cols);
        tbl.header(ui);
        egui::ScrollArea::vertical().id_salt("tps").max_height(380.0).show(ui, |ui| {
            for tr in &base {
                let specific = t.prices.iter().position(|p| p.code == tr.code);
                ui.horizontal(|ui| {
                    lab(ui, 46.0, &tr.code);
                    lab(ui, 180.0, &tr.description);
                    lab(ui, 76.0, RichText::new(money(tr.price)).color(MUTED));
                    match specific {
                        Some(i) => {
                            ui.add_sized([96.0, 32.0], egui::DragValue::new(&mut t.prices[i].price).range(0.0..=100000.0).speed(1.0).fixed_decimals(2).suffix(" €"));
                            if ins {
                                ui.add_sized([96.0, 32.0], egui::DragValue::new(&mut t.prices[i].insurer_fee).range(0.0..=100000.0).speed(0.5).fixed_decimals(2).suffix(" €"));
                            }
                            if ui.small_button("✖").on_hover_text("Volver al descuento general").clicked() {
                                t.prices.remove(i);
                            }
                        }
                        None => {
                            let p = round2(tr.price * (1.0 - t.discount_pct / 100.0));
                            lab(ui, 96.0, money(p));
                            if ins {
                                lab(ui, 96.0, RichText::new("—").color(MUTED));
                            }
                            if ui.small_button("✎").on_hover_text("Fijar un precio específico (p. ej. 0 € si lo cubre la póliza)").clicked() {
                                t.prices.push(TariffPrice { code: tr.code.clone(), price: p, insurer_fee: 0.0 });
                            }
                        }
                    }
                });
            }
        });
        if serde_json::to_string(t).unwrap_or_default() != before {
            app.changed();
        }
    });
}

fn treatments(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Catálogo de tratamientos");
            right_aligned(ui, |ui| {
                if ui.button("＋ Nuevo tratamiento").clicked() {
                    let max = app.db.treatments.iter().filter_map(|t| t.code.parse::<u32>().ok()).filter(|c| *c < 9000).max().unwrap_or(1000);
                    let code = format!("{:04}", max + 1);
                    app.db.treatments.push(Treatment {
                        code: code.clone(),
                        description: "Nuevo tratamiento".into(),
                        group: "Otros".into(),
                        price: 0.0,
                        minutes: 30,
                        odg: OdgKind::Ninguno,
                        per_tooth: false,
                        recall_months: 0,
                        active: true,
                        only_tariff: None,
                    });
                    app.lists.treat_sel = Some(code);
                    app.changed();
                }
            });
        });
        ui.add_sized([300.0, 34.0], egui::TextEdit::singleline(&mut app.lists.treat_filter).hint_text("Buscar tratamiento o grupo"));
        let q = fold(&app.lists.treat_filter);
        let tbl = Table::new("tt", vec![("Código", 60.0), ("Descripción", 230.0), ("Grupo", 120.0), ("Precio", 90.0), ("Min.", 50.0)]);
        tbl.header(ui);
        let mut pick = None;
        egui::ScrollArea::vertical().id_salt("tts").max_height(300.0).show(ui, |ui| {
            for (i, t) in app.db.treatments.iter().filter(|t| q.is_empty() || fold(&t.description).contains(&q) || fold(&t.group).contains(&q) || fold(&t.code).contains(&q)).enumerate() {
                let sel = app.lists.treat_sel.as_deref() == Some(t.code.as_str());
                let group = match t.only_tariff {
                    Some(id) => format!("{} · solo {}", t.group, app.db.tariff_name(id)),
                    None => t.group.clone(),
                };
                let cells = [cell(&t.code), cellc(&t.description, if t.active { TEXT } else { MUTED }), cell(group), cell(money(t.price)), cell(t.minutes.to_string())];
                if tbl.row(ui, i, sel, &cells).clicked() {
                    pick = Some(t.code.clone());
                }
            }
        });
        if pick.is_some() {
            app.lists.treat_sel = pick;
        }
        ui.separator();
        let Some(code) = app.lists.treat_sel.clone() else {
            muted(ui, "Seleccione un tratamiento para editarlo.");
            return;
        };
        let Some(t) = app.db.treatments.iter_mut().find(|t| t.code == code) else { return };
        let before = serde_json::to_string(t).unwrap_or_default();
        ui.horizontal(|ui| {
            field(ui, "Descripción", &mut t.description, 260.0);
            field(ui, "Grupo", &mut t.group, 140.0);
        });
        ui.horizontal(|ui| {
            ui.label("Precio privado");
            ui.add(egui::DragValue::new(&mut t.price).range(0.0..=100000.0).speed(1.0).suffix(" €"));
            ui.label("Duración");
            ui.add(egui::DragValue::new(&mut t.minutes).range(0..=480).speed(5).suffix(" min"));
            ui.label("Recall a los");
            ui.add(egui::DragValue::new(&mut t.recall_months).range(0..=36).suffix(" meses"));
        });
        ui.horizontal(|ui| {
            ui.label("Dibujo en odontograma:");
            crate::theme::combo("t_odg").selected_text(t.odg.label()).show_ui(ui, |ui| {
                ui.selectable_value(&mut t.odg, OdgKind::Ninguno, "General (sin pieza)");
                for k in OdgKind::ALL_TOOLS {
                    ui.selectable_value(&mut t.odg, k, k.label());
                }
                ui.selectable_value(&mut t.odg, OdgKind::Higiene, "Higiene");
            });
            ui.checkbox(&mut t.per_tooth, "Se cobra por pieza");
            ui.checkbox(&mut t.active, "Activo");
        });
        if serde_json::to_string(t).unwrap_or_default() != before {
            app.changed();
        }
    });
}
