//! Almacén: productos y stock, movimientos, pedidos a proveedores,
//! proveedores y consumos automáticos por tratamiento.

use crate::app::App;
use crate::model::*;
use crate::stock::*;
use crate::theme::*;
use egui::{RichText, Ui};

pub struct StockState {
    pub tab: usize,
    pub product: Option<u32>,
    pub filter: String,
    pub only_low: bool,
    pub qty: f64,
    pub reason: String,
    pub lot: String,
    /// Caducidad del lote que entra (como en la caja: 03/2029).
    pub expiry: String,
    pub order: Option<u32>,
    pub supplier: Option<u32>,
    pub cons_code: String,
    pub cons_product: u32,
    pub cons_qty: f64,
    pub lab_sel: Option<u32>,
    pub lab_filter_open: bool,
    /// Envío por correo de una orden de laboratorio en curso.
    pub lab_job: Option<crate::screens::laboratorio::LabJob>,
    /// Último resultado del envío digital: (trabajo, correcto, mensaje).
    pub lab_msg: Option<(u32, bool, String)>,
}

impl Default for StockState {
    fn default() -> Self {
        StockState {
            tab: 0,
            product: Some(9),
            filter: String::new(),
            only_low: false,
            qty: 1.0,
            reason: String::new(),
            lot: String::new(),
            expiry: String::new(),
            order: None,
            supplier: Some(1),
            cons_code: "0301".into(),
            cons_product: 4,
            cons_qty: 1.0,
            lab_sel: None,
            lab_filter_open: true,
            lab_job: None,
            lab_msg: None,
        }
    }
}

fn qty(v: f64) -> String {
    if (v - v.round()).abs() < 1e-6 { format!("{v:.0}") } else { format!("{v:.2}") }
}

pub fn screen(app: &mut App, ui: &mut Ui) {
    title(ui, "Almacén", "Material, stock mínimo, pedidos a proveedores y consumo automático al realizar tratamientos");
    let low = app.db.low_stock().len();
    let pend_orders = app.db.orders.iter().filter(|o| matches!(o.state, OrderState::Borrador | OrderState::Enviado)).count();
    let w = kpi_w(ui, 4);
    ui.horizontal(|ui| {
        kpi(ui, w, "Productos", &app.db.products.iter().filter(|p| p.active).count().to_string(), &format!("{} proveedores", app.db.suppliers.iter().filter(|s| !s.is_lab).count()), INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Bajo mínimo", &low.to_string(), if low > 0 { "conviene pedir" } else { "todo en orden" }, if low > 0 { DANGER } else { OK });
        ui.add_space(4.0);
        kpi(ui, w, "Valor del stock", &money(app.db.stock_value()), "a precio de coste", ACCENT);
        ui.add_space(4.0);
        kpi(ui, w, "Pedidos en curso", &pend_orders.to_string(), "borradores y enviados", WARN);
    });
    expiring(app, ui);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        for (i, n) in ["Productos", "Pedidos", "Proveedores", "Consumos por tratamiento", "Esterilización"].iter().enumerate() {
            if ui.selectable_label(app.stock.tab == i, *n).clicked() {
                app.stock.tab = i;
            }
        }
    });
    ui.add_space(8.0);
    match app.stock.tab {
        0 => products(app, ui),
        1 => orders(app, ui),
        2 => suppliers(app, ui),
        3 => consumptions(app, ui),
        _ => crate::screens::esterilizacion::panel(app, ui),
    }
}

/// Lotes que caducan en los próximos 60 días (o ya caducados) con existencias.
fn expiring(app: &mut App, ui: &mut Ui) {
    let list = app.db.expiring_lots(app.today + chrono::Duration::days(EXPIRY_DAYS));
    if list.is_empty() {
        return;
    }
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, &format!("Caducan en los próximos {EXPIRY_DAYS} días"));
        let mut pick = None;
        for (pid, l) in &list {
            let name = app.db.product(*pid).map(|p| p.name.clone()).unwrap_or_default();
            let gone = l.expiry.is_some_and(|e| e < app.today);
            ui.horizontal(|ui| {
                pill(ui, if gone { "Caducado" } else { "Caduca pronto" }, if gone { DANGER } else { WARN });
                if ui.link(format!("{name} · lote {} · {} · quedan {}", l.lot, l.expiry.map(fmt_expiry).unwrap_or_default(), qty(l.qty))).clicked() {
                    pick = Some(*pid);
                }
            });
        }
        if let Some(p) = pick {
            app.stock.product = Some(p);
            app.stock.tab = 0;
        }
    });
}

/// Días antes de la caducidad en los que se avisa.
pub const EXPIRY_DAYS: i64 = 60;

fn products(app: &mut App, ui: &mut Ui) {
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    let side = 380.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail - side - 24.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    // (a 1280 px, más estrecha: si no, el botón de la derecha tapa la casilla)
                    ui.add_sized([(ui.available_width() * 0.4).min(260.0), 34.0], egui::TextEdit::singleline(&mut app.stock.filter).hint_text("Buscar producto, familia o código"));
                    ui.checkbox(&mut app.stock.only_low, "Solo bajo mínimo");
                    right_aligned(ui, |ui| {
                        if ui.button("＋ Producto").clicked() {
                            let id = app.db.new_id();
                            app.db.products.push(Product {
                                id,
                                code: format!("P{id:03}"),
                                name: "Nuevo producto".into(),
                                family: "General".into(),
                                unit: "unidad".into(),
                                stock: 0.0,
                                min_stock: 1.0,
                                cost: 0.0,
                                supplier_id: app.db.suppliers.first().map(|s| s.id).unwrap_or(0),
                                location: String::new(),
                                active: true,
                                traced: false,
                            });
                            app.stock.product = Some(id);
                            app.changed();
                        }
                    });
                });
                ui.add_space(6.0);
                let q = fold(&app.stock.filter);
                let tbl = Table::new("prods", vec![("Código", 70.0), ("Producto", 230.0), ("Familia", 105.0), ("Stock", 70.0), ("Mínimo", 62.0), ("Pedido", 62.0), ("Coste", 76.0), ("Caduca", 76.0), ("Ubicación", 105.0)]);
                tbl.header(ui);
                // también por lote (si el fabricante retira uno, se busca aquí)
                let by_lot: std::collections::HashSet<u32> = if q.len() >= 3 { app.db.stock_moves.iter().filter(|m| fold(&m.lot).contains(&q)).map(|m| m.product_id).collect() } else { Default::default() };
                let list: Vec<Product> = app
                    .db
                    .products
                    .iter()
                    .filter(|p| q.is_empty() || fold(&p.name).contains(&q) || fold(&p.family).contains(&q) || fold(&p.code).contains(&q) || by_lot.contains(&p.id))
                    .filter(|p| !app.stock.only_low || p.stock <= p.min_stock)
                    .cloned()
                    .collect();
                let mut pick = None;
                egui::ScrollArea::vertical().id_salt("prodl").max_height(460.0).show(ui, |ui| {
                    for (i, p) in list.iter().enumerate() {
                        let low = p.stock <= p.min_stock;
                        let ord = app.db.on_order(p.id);
                        let exp = if p.traced { app.db.next_expiry(p.id) } else { None };
                        let exp_col = match exp {
                            Some(e) if e < app.today => DANGER,
                            Some(e) if e <= app.today + chrono::Duration::days(EXPIRY_DAYS) => WARN,
                            _ => TEXT,
                        };
                        let cells = [
                            cell(&p.code),
                            cellc(&p.name, if p.active { TEXT } else { MUTED }),
                            cell(&p.family),
                            cellc(format!("{} {}", qty(p.stock), if low { "⚠" } else { "" }), if low { DANGER } else { TEXT }),
                            cell(qty(p.min_stock)),
                            cellc(if ord > 0.0 { qty(ord) } else { String::new() }, INFO),
                            cell(money(p.cost)),
                            cellc(exp.map(fmt_expiry).unwrap_or_default(), exp_col),
                            cell(&p.location),
                        ];
                        if tbl.row(ui, i, app.stock.product == Some(p.id), &cells).clicked() {
                            pick = Some(p.id);
                        }
                    }
                });
                if pick.is_some() {
                    app.stock.product = pick;
                }
            });
        });
        ui.vertical(|ui| {
            ui.set_width(side);
            product_panel(app, ui);
        });
    });
}

fn product_panel(app: &mut App, ui: &mut Ui) {
    let Some(id) = app.stock.product else { return };
    let sups: Vec<(u32, String)> = app.db.suppliers.iter().filter(|s| !s.is_lab).map(|s| (s.id, s.name.clone())).collect();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let Some(p) = app.db.products.iter_mut().find(|p| p.id == id) else { return };
        let before = serde_json::to_string(p).unwrap_or_default();
        section(ui, &p.name.clone());
        let low = p.stock <= p.min_stock;
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} {}", qty(p.stock), p.unit)).size(26.0).family(sb()).color(if low { DANGER } else { TEXT }));
            if low {
                pill(ui, "Bajo mínimo", DANGER);
            }
        });
        ui.horizontal_wrapped(|ui| {
            field(ui, "Nombre", &mut p.name, 220.0);
            field(ui, "Código", &mut p.code, 90.0);
        });
        ui.horizontal_wrapped(|ui| {
            field(ui, "Familia", &mut p.family, 140.0);
            field(ui, "Unidad", &mut p.unit, 90.0);
            field(ui, "Ubicación", &mut p.location, 100.0);
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Mínimo");
            ui.add(egui::DragValue::new(&mut p.min_stock).range(0.0..=100000.0));
            ui.label("Coste");
            ui.add(egui::DragValue::new(&mut p.cost).range(0.0..=100000.0).speed(0.1).suffix(" €"));
            ui.checkbox(&mut p.active, "Activo");
        });
        ui.checkbox(&mut p.traced, "Lleva lote (implantes, pilares, injertos…)").on_hover_text("Se apunta el lote y la caducidad al recibirlo, y el lote que lleva cada paciente al colocárselo");
        ui.horizontal(|ui| {
            ui.label("Proveedor");
            let cur = sups.iter().find(|s| s.0 == p.supplier_id).map(|s| s.1.clone()).unwrap_or_else(|| "—".into());
            crate::theme::combo("p_sup").width(220.0).selected_text(cur).show_ui(ui, |ui| {
                for (sid, n) in &sups {
                    ui.selectable_value(&mut p.supplier_id, *sid, n);
                }
            });
        });
        if serde_json::to_string(p).unwrap_or_default() != before {
            app.dirty = true;
        }
    });
    ui.add_space(8.0);
    let traced = app.db.product(id).is_some_and(|p| p.traced);
    if traced {
        lots_card(app, ui, id);
        ui.add_space(8.0);
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Movimiento de almacén");
        ui.horizontal(|ui| {
            ui.label("Cantidad");
            ui.add(egui::DragValue::new(&mut app.stock.qty).range(0.0..=100000.0).speed(0.5));
            ui.add_sized([110.0, 34.0], egui::TextEdit::singleline(&mut app.stock.lot).hint_text(if traced { "Lote" } else { "Lote (opcional)" }));
            ui.add_sized([90.0, 34.0], egui::TextEdit::singleline(&mut app.stock.expiry).hint_text("Caduca")).on_hover_text("Como en la caja: 03/2029");
        });
        ui.add_sized([ui.available_width(), 34.0], egui::TextEdit::singleline(&mut app.stock.reason).hint_text("Motivo (p. ej. caducado, regularización)"));
        ui.horizontal(|ui| {
            let mut mv: Option<(f64, MoveKind)> = None;
            if primary(ui, "＋ Entrada").clicked() {
                mv = Some((app.stock.qty, MoveKind::Entrada));
            }
            if ui.button("－ Salida").clicked() {
                mv = Some((-app.stock.qty, MoveKind::Salida));
            }
            if ui.button("Fijar inventario").on_hover_text("Ajusta el stock a la cantidad indicada").clicked() {
                let cur = app.db.product(id).map(|p| p.stock).unwrap_or(0.0);
                mv = Some((app.stock.qty - cur, MoveKind::Ajuste));
            }
            if let Some((q, k)) = mv {
                if q.abs() < 1e-9 {
                    app.warn("Indique una cantidad.");
                } else {
                    let (reason, lot, expiry) = (app.stock.reason.clone(), app.stock.lot.clone(), app.stock.expiry.clone());
                    match app.db.manual_move(id, q, k, &reason, &lot, &expiry) {
                        Ok(()) => {
                            app.stock.reason.clear();
                            app.stock.lot.clear();
                            app.stock.expiry.clear();
                            app.changed();
                            app.notify(format!("{}: {} unidades", k.label(), qty(q)));
                        }
                        Err(e) => app.warn(e),
                    }
                }
            }
        });
        ui.add_space(6.0);
        section(ui, "Últimos movimientos");
        let mut moves: Vec<StockMove> = app.db.stock_moves.iter().filter(|m| m.product_id == id).cloned().collect();
        moves.sort_by_key(|m| std::cmp::Reverse(m.when));
        if moves.is_empty() {
            muted(ui, "Sin movimientos.");
        }
        for m in moves.iter().take(8) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{:+}", round2(m.qty))).family(sb()).color(if m.qty < 0.0 { DANGER } else { OK }));
                ui.vertical(|ui| {
                    ui.label(RichText::new(&m.reason).size(13.5));
                    let pat = m.patient_id.map(|p| format!(" · {}", app.db.patient_name(p))).unwrap_or_default();
                    muted(ui, format!("{} · {}{}{}", m.when.format("%d/%m/%Y"), m.kind.label(), pat, if m.lot.is_empty() { String::new() } else { format!(" · lote {}", m.lot) }));
                });
            });
        }
    });
}

/// Lotes de un producto: lo que queda de cada uno, su caducidad y los
/// pacientes que lo llevan.
fn lots_card(app: &mut App, ui: &mut Ui, id: u32) {
    let mut open = None;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Lotes");
        let mut lots = app.db.lots(id);
        if lots.is_empty() {
            muted(ui, "Aún no hay lotes: se apuntan al recibir los pedidos o en las entradas a mano.");
            return;
        }
        // primero los que quedan, del que antes caduca
        lots.sort_by_key(|l| (l.qty <= 1e-9, l.expiry.unwrap_or(chrono::NaiveDate::MAX)));
        for l in lots {
            let who: Vec<(u32, String, String)> = app
                .db
                .lot_patients(&l.lot)
                .iter()
                .filter(|m| m.product_id == id && m.lot == l.lot)
                .filter_map(|m| {
                    let teeth = m.entry_id.and_then(|e| app.db.entries.iter().find(|x| x.id == e)).map(|e| e.teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join("-")).unwrap_or_default();
                    m.patient_id.map(|p| (p, app.db.patient_name(p), format!("{}{}", fmt_date(m.when.date()), if teeth.is_empty() { String::new() } else { format!(" · {teeth}") })))
                })
                .collect();
            let gone = l.expiry.is_some_and(|e| e < app.today);
            let soon = l.expiry.is_some_and(|e| e <= app.today + chrono::Duration::days(EXPIRY_DAYS));
            let head = format!("{} · quedan {}{} · {} paciente{}", l.lot, qty(l.qty), l.expiry.map(|e| format!(" · caduca {}", fmt_expiry(e))).unwrap_or_default(), who.len(), if who.len() == 1 { "" } else { "s" });
            let col = if gone && l.qty > 1e-9 {
                DANGER
            } else if soon && l.qty > 1e-9 {
                WARN
            } else {
                TEXT
            };
            egui::CollapsingHeader::new(RichText::new(head).size(13.5).color(col)).id_salt(("lot", id, l.lot.clone())).show(ui, |ui| {
                if who.is_empty() {
                    muted(ui, "Ningún paciente lo lleva.");
                }
                for (pid, name, when) in who {
                    ui.horizontal(|ui| {
                        if ui.link(&name).clicked() {
                            open = Some(pid);
                        }
                        muted(ui, when);
                    });
                }
            });
        }
    });
    if let Some(p) = open {
        app.open_patient(p, crate::app::PatTab::Historial);
    }
}

fn orders(app: &mut App, ui: &mut Ui) {
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail * 0.5);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    section(ui, "Pedidos a proveedores");
                    right_aligned(ui, |ui| {
                        if primary(ui, "Proponer pedido").on_hover_text("Crea pedidos con lo que está bajo mínimo").clicked() {
                            let ids = app.db.propose_orders();
                            if ids.is_empty() {
                                app.notify("No hace falta pedir nada: todo está por encima del mínimo o ya pedido.");
                            } else {
                                app.stock.order = ids.first().copied();
                                app.changed();
                                app.notify(format!("{} pedidos propuestos. Revíselos y envíelos.", ids.len()));
                            }
                        }
                    });
                });
                let tbl = Table::new("ords", vec![("Nº", 60.0), ("Fecha", 96.0), ("Proveedor", 200.0), ("Líneas", 60.0), ("Total", 100.0), ("Estado", 100.0)]);
                tbl.header(ui);
                let mut list = app.db.orders.clone();
                list.sort_by_key(|o| std::cmp::Reverse((o.date, o.id)));
                let mut pick = None;
                for (i, o) in list.iter().enumerate() {
                    let col = match o.state {
                        OrderState::Borrador => MUTED,
                        OrderState::Enviado => WARN,
                        OrderState::Recibido => OK,
                        OrderState::Cancelado => DANGER,
                    };
                    if tbl
                        .row(ui, i, app.stock.order == Some(o.id), &[cell(format!("#{}", o.id)), cell(fmt_date(o.date)), cell(app.db.supplier_name(o.supplier_id)), cell(o.lines.len().to_string()), cell(money(o.total())), cellc(o.state.label(), col)])
                        .clicked()
                    {
                        pick = Some(o.id);
                    }
                }
                if list.is_empty() {
                    empty_state(ui, "Sin pedidos.");
                }
                if pick.is_some() {
                    app.stock.order = pick;
                }
            });
        });
        ui.vertical(|ui| {
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                let Some(id) = app.stock.order else {
                    muted(ui, "Seleccione un pedido.");
                    return;
                };
                let Some(o) = app.db.orders.iter().find(|o| o.id == id).cloned() else { return };
                section(ui, &format!("Pedido #{} · {}", o.id, app.db.supplier_name(o.supplier_id)));
                muted(ui, format!("{} · {}", fmt_date(o.date), o.state.label()));
                let editable = o.state == OrderState::Borrador;
                // al recibir se ajusta lo que llega de verdad, con su lote
                let receiving = o.state == OrderState::Enviado;
                let mut remove = None;
                let mut split = None;
                let mut touched = false;
                let prods: Vec<(u32, String, bool)> = app.db.products.iter().map(|p| (p.id, p.name.clone(), p.traced)).collect();
                if receiving && o.lines.iter().any(|l| prods.iter().any(|p| p.0 == l.product_id && p.2)) {
                    muted(ui, "Al recibir, escriba el lote y la caducidad que vienen en la caja. Si llegan dos lotes del mismo producto, use «＋ otro lote».");
                }
                if let Some(om) = app.db.orders.iter_mut().find(|x| x.id == id) {
                    for (i, l) in om.lines.iter_mut().enumerate() {
                        let traced = prods.iter().any(|p| p.0 == l.product_id && p.2);
                        ui.horizontal(|ui| {
                            lab(ui, 230.0, prods.iter().find(|p| p.0 == l.product_id).map(|p| p.1.clone()).unwrap_or_default());
                            ui.add_enabled_ui(editable || receiving, |ui| {
                                touched |= ui.add(egui::DragValue::new(&mut l.qty).range(0.0..=100000.0)).on_hover_text(if receiving { "Cantidad que llega" } else { "Cantidad" }).changed();
                                touched |= ui.add(egui::DragValue::new(&mut l.cost).range(0.0..=100000.0).speed(0.1).suffix(" €")).changed();
                            });
                            lab(ui, 90.0, RichText::new(money(l.qty * l.cost)).family(sb()));
                            if editable && ui.small_button("✖").clicked() {
                                remove = Some(i);
                            }
                        });
                        if (receiving && traced) || !l.lot.is_empty() {
                            ui.horizontal(|ui| {
                                ui.add_space(12.0);
                                ui.add_enabled_ui(receiving, |ui| {
                                    touched |= ui.add_sized([130.0, 30.0], egui::TextEdit::singleline(&mut l.lot).hint_text("Lote")).changed();
                                    touched |= ui.add_sized([90.0, 30.0], egui::TextEdit::singleline(&mut l.expiry).hint_text("Caduca")).on_hover_text("Como en la caja: 03/2029").changed();
                                });
                                if receiving && l.qty > 1.0 && ui.small_button("＋ otro lote").on_hover_text("Separa una unidad en otra línea con su lote").clicked() {
                                    split = Some(i);
                                }
                            });
                        }
                    }
                    if let Some(i) = remove {
                        om.lines.remove(i);
                        touched = true;
                    }
                    if let Some(i) = split {
                        om.lines[i].qty -= 1.0;
                        let mut n = om.lines[i].clone();
                        n.qty = 1.0;
                        n.lot.clear();
                        n.expiry.clear();
                        om.lines.insert(i + 1, n);
                        touched = true;
                    }
                }
                if touched {
                    app.dirty = true;
                }
                ui.label(RichText::new(format!("Total {}", money(app.db.orders.iter().find(|x| x.id == id).map(|x| x.total()).unwrap_or(0.0)))).size(18.0).family(sb()).color(ACCENT));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    match o.state {
                        OrderState::Borrador => {
                            if primary(ui, "✉ Enviar al proveedor").clicked() {
                                let email = app.db.suppliers.iter().find(|s| s.id == o.supplier_id).map(|s| s.email.clone()).unwrap_or_default();
                                let body = app.db.order_text(id);
                                let url = format!("mailto:{}?subject={}&body={}", email, crate::screens::comunicaciones::enc_pub(&format!("Pedido {} - {}", id, app.db.config.name)), crate::screens::comunicaciones::enc_pub(&body));
                                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                                if let Some(om) = app.db.orders.iter_mut().find(|x| x.id == id) {
                                    om.state = OrderState::Enviado;
                                }
                                app.changed();
                            }
                            if ui.button("Marcar enviado").clicked() {
                                if let Some(om) = app.db.orders.iter_mut().find(|x| x.id == id) {
                                    om.state = OrderState::Enviado;
                                }
                                app.changed();
                            }
                        }
                        OrderState::Enviado if primary(ui, "✔ Recibir mercancía").clicked() => match app.db.receive_order(id) {
                            Ok(()) => {
                                app.changed();
                                app.notify("Pedido recibido: el material ha entrado en el almacén.");
                            }
                            Err(e) => app.warn(e),
                        },
                        _ => {}
                    }
                    if matches!(o.state, OrderState::Borrador | OrderState::Enviado) && ui.button("Cancelar pedido").clicked() {
                        if let Some(om) = app.db.orders.iter_mut().find(|x| x.id == id) {
                            om.state = OrderState::Cancelado;
                        }
                        app.changed();
                    }
                    if ui.button("📋 Copiar texto").clicked() {
                        ui.ctx().copy_text(app.db.order_text(id));
                        app.notify("Texto del pedido copiado.");
                    }
                });
                if editable {
                    ui.add_space(6.0);
                    let key = egui::Id::unique(("addline", id));
                    let mut sel: u32 = ui.data_mut(|d| d.get_temp(key)).unwrap_or(prods.first().map(|p| p.0).unwrap_or(0));
                    ui.horizontal(|ui| {
                        let cur = prods.iter().find(|p| p.0 == sel).map(|p| p.1.clone()).unwrap_or_default();
                        crate::theme::combo("ord_add").width(240.0).selected_text(cur).show_ui(ui, |ui| {
                            for (pid, n, _) in &prods {
                                ui.selectable_value(&mut sel, *pid, n);
                            }
                        });
                        if ui.button("＋ Añadir línea").clicked() {
                            let cost = app.db.product(sel).map(|p| p.cost).unwrap_or(0.0);
                            if let Some(om) = app.db.orders.iter_mut().find(|x| x.id == id) {
                                om.lines.push(OrderLine { product_id: sel, qty: 1.0, cost, lot: String::new(), expiry: String::new() });
                            }
                            app.changed();
                        }
                    });
                    ui.data_mut(|d| d.insert_temp(key, sel));
                }
            });
        });
    });
}

fn suppliers(app: &mut App, ui: &mut Ui) {
    let avail = ui.available_width().min(ui.clip_rect().max.x - ui.cursor().min.x - 6.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail * 0.5);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    section(ui, "Proveedores y laboratorios");
                    right_aligned(ui, |ui| {
                        if ui.button("＋ Nuevo").clicked() {
                            let id = app.db.new_id();
                            app.db.suppliers.push(Supplier { id, name: "Nuevo proveedor".into(), nif: String::new(), phone: String::new(), email: String::new(), contact: String::new(), is_lab: false, notes: String::new() });
                            app.stock.supplier = Some(id);
                            app.changed();
                        }
                    });
                });
                let tbl = Table::new("sups", vec![("Nombre", 220.0), ("Tipo", 110.0), ("Teléfono", 120.0), ("Email", 200.0)]);
                tbl.header(ui);
                let mut pick = None;
                for (i, s) in app.db.suppliers.iter().enumerate() {
                    if tbl.row(ui, i, app.stock.supplier == Some(s.id), &[cell(&s.name), cellc(if s.is_lab { "Laboratorio" } else { "Proveedor" }, if s.is_lab { INFO } else { TEXT }), cell(&s.phone), cell(&s.email)]).clicked() {
                        pick = Some(s.id);
                    }
                }
                if pick.is_some() {
                    app.stock.supplier = pick;
                }
            });
        });
        ui.vertical(|ui| {
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                let Some(id) = app.stock.supplier else { return };
                let Some(s) = app.db.suppliers.iter_mut().find(|s| s.id == id) else { return };
                let before = serde_json::to_string(s).unwrap_or_default();
                section(ui, "Datos del proveedor");
                field(ui, "Nombre", &mut s.name, 300.0);
                ui.horizontal(|ui| {
                    field(ui, "NIF", &mut s.nif, 120.0);
                    field(ui, "Teléfono", &mut s.phone, 140.0);
                });
                field(ui, "Email (para enviar pedidos)", &mut s.email, 300.0);
                field(ui, "Persona de contacto", &mut s.contact, 300.0);
                ui.checkbox(&mut s.is_lab, "Es un laboratorio protésico");
                ui.label(RichText::new("Notas").size(13.5).color(MUTED));
                ui.add(egui::TextEdit::multiline(&mut s.notes).desired_rows(3).desired_width(f32::INFINITY));
                if serde_json::to_string(s).unwrap_or_default() != before {
                    app.dirty = true;
                }
            });
        });
    });
}

fn consumptions(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Material que se descuenta al realizar cada tratamiento");
        muted(ui, "Al marcar un tratamiento como realizado se descuenta automáticamente del almacén (cantidad × piezas).");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let cur = app.db.treatment(&app.stock.cons_code).map(|t| format!("{} · {}", t.code, t.description)).unwrap_or_default();
            crate::theme::combo("cons_t").width(300.0).selected_text(cur).show_ui(ui, |ui| {
                for t in app.db.treatments.iter().filter(|t| t.active) {
                    ui.selectable_value(&mut app.stock.cons_code, t.code.clone(), format!("{} · {}", t.code, t.description));
                }
            });
            let curp = app.db.product(app.stock.cons_product).map(|p| p.name.clone()).unwrap_or_default();
            crate::theme::combo("cons_p").width(260.0).selected_text(curp).show_ui(ui, |ui| {
                for p in &app.db.products {
                    ui.selectable_value(&mut app.stock.cons_product, p.id, &p.name);
                }
            });
            ui.add(egui::DragValue::new(&mut app.stock.cons_qty).range(0.01..=100.0).speed(0.05));
            if primary(ui, "Añadir").clicked() {
                app.db.consumptions.push(Consumption { code: app.stock.cons_code.clone(), product_id: app.stock.cons_product, qty: app.stock.cons_qty });
                app.changed();
            }
        });
        ui.add_space(6.0);
        let tbl = Table::new("cons", vec![("Código", 80.0), ("Tratamiento", 280.0), ("Producto", 280.0), ("Cantidad", 100.0), ("", 60.0)]);
        tbl.header(ui);
        let list = app.db.consumptions.clone();
        let mut remove = None;
        egui::ScrollArea::vertical().id_salt("consl").max_height(420.0).show(ui, |ui| {
            for (i, c) in list.iter().enumerate() {
                let r = tbl.row(
                    ui,
                    i,
                    false,
                    &[cell(&c.code), cell(app.db.treatment(&c.code).map(|t| t.description.clone()).unwrap_or_default()), cell(app.db.product(c.product_id).map(|p| p.name.clone()).unwrap_or_default()), cell(qty(c.qty)), cellc("quitar", DANGER)],
                );
                if r.clicked() && r.interact_pointer_pos().map(|p| p.x > r.rect.max.x - 90.0).unwrap_or(false) {
                    remove = Some(i);
                }
            }
        });
        if let Some(i) = remove {
            app.db.consumptions.remove(i);
            app.changed();
        }
    });
}
