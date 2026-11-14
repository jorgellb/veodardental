//! Configuración del centro, colaboradores y mapa de módulos.

use crate::app::{App, ConfirmAction, Dialog};
use crate::model::*;
use crate::theme::*;
use chrono::{NaiveTime, Timelike};
use egui::{Color32, RichText, Ui};

/// Apartados de Configuración, cada uno en su pestaña.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CfgTab {
    #[default]
    Clinica,
    Marcha,
    Gabinetes,
    Centros,
    Usuarios,
    Facturacion,
    Correo,
    CitaOnline,
    Telefonia,
    Whatsapp,
    Receta,
    Ia,
    Importar,
    Copias,
    Fichas,
    Duplicados,
    Registro,
    Red,
}

impl CfgTab {
    /// Pestañas agrupadas como en el menú lateral.
    pub const GROUPS: [(&'static str, &'static [CfgTab]); 7] = [
        ("Empezar", &[CfgTab::Marcha]),
        ("Clínica", &[CfgTab::Clinica, CfgTab::Gabinetes, CfgTab::Centros, CfgTab::Usuarios]),
        ("Facturación", &[CfgTab::Facturacion]),
        ("Comunicación", &[CfgTab::Correo, CfgTab::CitaOnline, CfgTab::Telefonia, CfgTab::Whatsapp]),
        ("Clínico", &[CfgTab::Receta, CfgTab::Ia]),
        ("Datos", &[CfgTab::Importar, CfgTab::Copias, CfgTab::Fichas, CfgTab::Duplicados, CfgTab::Registro]),
        ("Sistema", &[CfgTab::Red]),
    ];

    pub fn label(self) -> &'static str {
        match self {
            CfgTab::Clinica => "Datos de la clínica",
            CfgTab::Marcha => "Puesta en marcha",
            CfgTab::Gabinetes => "Gabinetes",
            CfgTab::Centros => "Centros",
            CfgTab::Usuarios => "Usuarios y permisos",
            CfgTab::Facturacion => "Facturación y cobros",
            CfgTab::Correo => "Correo electrónico",
            CfgTab::CitaOnline => "Cita online",
            CfgTab::Telefonia => "Telefonía",
            CfgTab::Whatsapp => "WhatsApp",
            CfgTab::Receta => "Receta electrónica",
            CfgTab::Ia => "Inteligencia artificial",
            CfgTab::Importar => "Importar datos",
            CfgTab::Copias => "Copias de seguridad",
            CfgTab::Fichas => "Exportar fichas",
            CfgTab::Duplicados => "Pacientes duplicados",
            CfgTab::Registro => "Registro de cambios",
            CfgTab::Red => "Trabajo en red",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            CfgTab::Clinica => "Nombre, NIF, dirección y contacto de la clínica, y el horario de la agenda.",
            CfgTab::Marcha => "Lo que falta para trabajar con la clínica real, comprobado con la configuración del programa.",
            CfgTab::Gabinetes => "Gabinetes o boxes de cada centro, que aparecen como columnas en la agenda.",
            CfgTab::Centros => "Varios centros con su agenda, gabinetes y caja; pacientes, historial y facturas compartidos.",
            CfgTab::Usuarios => "Quién entra en el programa, con qué perfil y contraseña, y el registro de accesos.",
            CfgTab::Facturacion => "Numeración de recibos, validez de presupuestos, cobro domiciliado SEPA y VERI*FACTU.",
            CfgTab::CitaOnline => "Página para que los pacientes pidan cita por internet con los huecos libres de la agenda.",
            CfgTab::Correo => "Servidor de correo para enviar desde el programa órdenes al laboratorio y documentos con adjuntos.",
            CfgTab::Telefonia => "Ficha del paciente en pantalla al sonar el teléfono, con la centralita o el programa de llamadas.",
            CfgTab::Whatsapp => "Recordatorios y confirmación automática de citas por WhatsApp.",
            CfgTab::Receta => "Receta oficial, lista de medicamentos de la AEMPS y conexión directa con las farmacias.",
            CfgTab::Ia => "Análisis de radiografías con varios proveedores, gratuitos y de pago, con respaldo si uno falla.",
            CfgTab::Importar => "Pacientes, tratamientos y tarifas desde GESDEN, Novaclinic u otro programa, o desde Excel.",
            CfgTab::Copias => "Copia automática diaria, copia manual, segunda copia cifrada en un disco USB o en la nube, y restauración.",
            CfgTab::Fichas => "Una carpeta con la ficha completa de cada paciente, lista para imprimir o archivar.",
            CfgTab::Duplicados => "Detecta pacientes repetidos y los une con todo su historial.",
            CfgTab::Registro => "Quién borró, emitió, importó, fusionó o restauró y cuándo; errores del programa e informe para soporte.",
            CfgTab::Red => "Este ordenador como servidor de la clínica o como puesto conectado a él.",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            CfgTab::Clinica => "clinica",
            CfgTab::Marcha => "marcha",
            CfgTab::Gabinetes => "gabinetes",
            CfgTab::Centros => "centros",
            CfgTab::Usuarios => "usuarios",
            CfgTab::Facturacion => "facturacion",
            CfgTab::Correo => "correo",
            CfgTab::CitaOnline => "web",
            CfgTab::Telefonia => "telefonia",
            CfgTab::Whatsapp => "whatsapp",
            CfgTab::Receta => "receta",
            CfgTab::Ia => "ia",
            CfgTab::Importar => "importar",
            CfgTab::Copias => "copias",
            CfgTab::Fichas => "fichas",
            CfgTab::Duplicados => "duplicados",
            CfgTab::Registro => "registro",
            CfgTab::Red => "red",
        }
    }

    /// Color del icono del apartado.
    fn color(self) -> Color32 {
        match self {
            CfgTab::Clinica | CfgTab::Marcha | CfgTab::Gabinetes | CfgTab::Centros | CfgTab::Usuarios => ACCENT,
            CfgTab::Facturacion => INFO,
            CfgTab::Correo | CfgTab::CitaOnline | CfgTab::Telefonia | CfgTab::Whatsapp => OK,
            CfgTab::Receta | CfgTab::Ia => VIOLET,
            CfgTab::Importar | CfgTab::Copias | CfgTab::Fichas | CfgTab::Duplicados | CfgTab::Registro => WARN,
            CfgTab::Red => Color32::from_rgb(71, 85, 105),
        }
    }
}

/// Abre Configuración en un apartado.
pub fn open(app: &mut App, tab: CfgTab) {
    app.screen = crate::app::Screen::Config;
    app.lists.cfg_tab = tab;
}

/// Configuración: menú de apartados a la izquierda (fijo) y el apartado elegido a la derecha.
pub fn show(app: &mut App, ui: &mut Ui) {
    title(ui, "Configuración", "");
    let nav_w = 252.0;
    let h = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        // menú de apartados
        ui.allocate_ui_with_layout(egui::vec2(nav_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            egui::Frame::new().fill(CARD).stroke(egui::Stroke::new(1.0, BORDER)).corner_radius(egui::CornerRadius::same(16)).inner_margin(egui::Margin::symmetric(10, 8)).shadow(shadow()).show(ui, |ui| {
                ui.set_width(nav_w - 20.0);
                egui::ScrollArea::vertical().id_salt("cfg_nav").auto_shrink([false, true]).max_height(h - 28.0).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (gi, (group, tabs)) in CfgTab::GROUPS.iter().enumerate() {
                        if gi > 0 {
                            ui.add_space(4.0);
                        }
                        // rótulo del grupo, compacto
                        let (gr, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 17.0), egui::Sense::hover());
                        ui.painter().text(egui::pos2(gr.min.x + 10.0, gr.center().y + 2.0), egui::Align2::LEFT_CENTER, group.to_uppercase(), sbf(10.6), SUBTLE);
                        ui.add_space(0.0);
                        for t in tabs.iter() {
                            if nav_item(ui, *t, app.lists.cfg_tab == *t) {
                                app.lists.cfg_tab = *t;
                            }
                        }
                    }
                });
            });
        });
        // contenido del apartado
        ui.vertical(|ui| {
            let tab = app.lists.cfg_tab;
            // cabecera del apartado
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(48.0, 48.0), egui::Sense::hover());
                ui.painter().rect_filled(r, egui::CornerRadius::same(14), tint(tab.color(), 0.13));
                crate::icons::settings_icon(ui.painter(), tab.icon(), r.center(), tab.color());
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(tab.label()).font(bf(22.0)).color(HEADING));
                    ui.label(RichText::new(tab.desc()).font(mdf(13.5)).color(MUTED));
                });
            });
            ui.add_space(12.0);
            let mut sa = egui::ScrollArea::vertical().id_salt(("cfg_tab", tab as u8)).auto_shrink([false, false]);
            if let Some(y) = app.lists.scroll_to.take() {
                sa = sa.vertical_scroll_offset(y);
            }
            sa.show(ui, |ui| {
                // hasta el borde visible (no por debajo de la barra de desplazamiento)
                let w = crate::theme::fit_width(ui);
                ui.set_width(w - 4.0);
                tab_content(app, ui, tab);
                ui.add_space(16.0);
            });
        });
    });
}

/// Un apartado del menú de Configuración. Devuelve si se ha pulsado.
fn nav_item(ui: &mut Ui, t: CfgTab, sel: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 29.0), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Tab, true, t.label()));
    let p = ui.painter();
    if sel {
        p.rect_filled(rect, egui::CornerRadius::same(10), ACCENT_SOFT);
        p.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 9.0), egui::vec2(3.0, rect.height() - 18.0)), egui::CornerRadius::same(2), ACCENT);
    } else if resp.hovered() {
        p.rect_filled(rect, egui::CornerRadius::same(10), ROW_HOVER);
    }
    let badge = egui::Rect::from_center_size(egui::pos2(rect.min.x + 26.0, rect.center().y), egui::vec2(26.0, 26.0));
    let col = if sel { ACCENT_DARK } else { t.color() };
    p.rect_filled(badge, egui::CornerRadius::same(9), if sel { CARD } else { tint(t.color(), 0.10) });
    crate::icons::settings_icon(p, t.icon(), badge.center(), col);
    p.text(egui::pos2(rect.min.x + 50.0, rect.center().y), egui::Align2::LEFT_CENTER, t.label(), if sel { sbf(14.0) } else { mdf(14.0) }, if sel { HEADING } else { TEXT });
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn tab_content(app: &mut App, ui: &mut Ui, tab: CfgTab) {
    match tab {
        CfgTab::Clinica => clinic_tab(app, ui),
        CfgTab::Marcha => crate::screens::marcha::panel(app, ui),
        CfgTab::Gabinetes => boxes_tab(app, ui),
        CfgTab::Centros => crate::screens::centros::centers_panel(app, ui),
        CfgTab::Usuarios => {
            crate::screens::login::users_admin(app, ui);
            ui.add_space(10.0);
            crate::screens::jornada::admin_panel(app, ui);
        }
        CfgTab::Facturacion => {
            billing_tab(app, ui);
            ui.add_space(10.0);
            books_card(app, ui);
        }
        CfgTab::Correo => crate::screens::correo::panel(app, ui),
        CfgTab::CitaOnline => {
            crate::screens::cita_online::panel(app, ui);
            ui.add_space(10.0);
            // las dos columnas de arriba pueden dejar el área más ancha que lo que se ve
            let w = (ui.clip_rect().right() - ui.cursor().left() - 8.0).max(320.0);
            ui.scope(|ui| {
                ui.set_max_width(w);
                crate::screens::pagos::settings(app, ui);
                ui.add_space(10.0);
                crate::screens::encuestas::settings(app, ui);
            });
        }
        CfgTab::Telefonia => crate::screens::llamadas::panel(app, ui),
        CfgTab::Whatsapp => {
            crate::screens::whatsapp::panel(app, ui);
            ui.add_space(10.0);
            crate::screens::comunicaciones::sms_settings(app, ui);
        }
        CfgTab::Receta => crate::screens::recetas::config_panel(app, ui),
        CfgTab::Ia => {
            crate::screens::ia::config_panel(app, ui);
            ui.add_space(10.0);
            let w = (ui.clip_rect().right() - ui.cursor().left() - 8.0).max(320.0);
            ui.scope(|ui| {
                ui.set_max_width(w);
                crate::screens::evolucion::settings(app, ui);
            });
        }
        CfgTab::Importar => crate::screens::importar::panel(app, ui),
        CfgTab::Copias => {
            crate::screens::herramientas::backups_panel(app, ui);
            ui.add_space(10.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "Fichero de datos");
                muted(ui, format!("Los datos de la clínica se guardan en {}", Db::path().display()));
                ui.add_space(4.0);
                if ui.button("Restaurar datos de ejemplo").on_hover_text("Borra todos los datos y carga la clínica de demostración").clicked() {
                    app.dialog = Dialog::Confirm { text: "Se borrarán todos los datos y se cargarán los de ejemplo. ¿Continuar?".into(), action: ConfirmAction::ResetSample };
                }
            });
        }
        CfgTab::Fichas => crate::screens::fichas::batch_panel(app, ui),
        CfgTab::Duplicados => crate::screens::bancos::duplicates_panel(app, ui),
        CfgTab::Registro => {
            crate::screens::bancos::audit_panel(app, ui);
            ui.add_space(10.0);
            crate::screens::soporte::panel(app, ui);
            ui.add_space(10.0);
            crate::screens::soporte::pilot_panel(app, ui);
        }
        CfgTab::Red => crate::screens::puestos::panel(app, ui),
    }
}

/// Datos de la clínica y horario de la agenda.
fn clinic_tab(app: &mut App, ui: &mut Ui) {
    let before = serde_json::to_string(&app.db.config).unwrap_or_default();
    ui.columns(2, |cols| {
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            let c = &mut app.db.config;
            section(ui, "Datos del centro");
            let w = (ui.available_width() / 1.1 - 4.0).min(420.0);
            field(ui, "Nombre comercial", &mut c.name, w);
            ui.horizontal(|ui| {
                field(ui, "NIF/CIF", &mut c.nif, (w / 2.0 - 10.0).max(120.0));
                field(ui, "Teléfono", &mut c.phone, (w / 2.0 - 10.0).max(120.0));
            });
            field(ui, "Dirección", &mut c.address, w);
            field(ui, "Email", &mut c.email, w);
        });
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            let c = &mut app.db.config;
            section(ui, "Agenda");
            ui.label(RichText::new("Intervalo de la agenda").size(13.5).color(MUTED));
            crate::theme::combo("slot").width(140.0).selected_text(format!("{} min", c.slot_minutes)).show_ui(ui, |ui| {
                for m in [5, 10, 15, 20, 30] {
                    ui.selectable_value(&mut c.slot_minutes, m, format!("{m} min"));
                }
            });
        });
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let mut week = app.db.clinic_week();
        let c = &mut app.db.config;
        section(ui, "Horario de la clínica");
        muted(ui, "Días y horas en que abre. La agenda, la cita por internet y los huecos libres solo ofrecen estas horas; el de cada profesional se pone en Colaboradores.");
        ui.add_space(4.0);
        if week_editor(ui, "clinic_week", &mut week, (c.open_time, c.close_time), "cerrado") {
            // la agenda abarca de la primera apertura al último cierre
            if let Some((o, e)) = week.span() {
                c.open_time = o;
                c.close_time = e;
            }
            c.hours = Some(week);
        }
    });
    if serde_json::to_string(&app.db.config).unwrap_or_default() != before {
        app.changed();
    }
}

/// Gabinetes de cada centro.
fn boxes_tab(app: &mut App, ui: &mut Ui) {
    let before = serde_json::to_string(&app.db.boxes).unwrap_or_default();
    let multi = app.db.multi_center();
    let mut remove = None;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Gabinetes / boxes");
            right_aligned(ui, |ui| {
                if primary(ui, "＋ Añadir gabinete").clicked() {
                    let id = app.db.new_id();
                    let center = cid(app.db.config.current_center);
                    app.db.boxes.push(Boxroom { id, name: format!("Gabinete {id}"), center_id: center });
                }
            });
        });
        ui.add_space(4.0);
        let names: Vec<(u32, String)> = app.db.centers.iter().map(|c| (c.id, c.name.clone())).collect();
        let used: Vec<u32> = app.db.appointments.iter().map(|a| a.box_id).collect();
        for b in app.db.boxes.iter_mut() {
            egui::Frame::new().fill(ROW_ALT).stroke(egui::Stroke::new(1.0, HAIRLINE)).corner_radius(egui::CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, egui::CornerRadius::same(9), tint(ACCENT, 0.12));
                    crate::icons::settings_icon(ui.painter(), "gabinetes", r.center(), ACCENT_DARK);
                    ui.add_sized([260.0, 34.0], egui::TextEdit::singleline(&mut b.name));
                    if multi {
                        let cur = names.iter().find(|c| c.0 == cid(b.center_id)).map(|c| c.1.clone()).unwrap_or_default();
                        crate::theme::combo(("box_center", b.id)).width(200.0).selected_text(cur).show_ui(ui, |ui| {
                            for (id, n) in &names {
                                ui.selectable_value(&mut b.center_id, *id, n);
                            }
                        });
                    }
                    right_aligned(ui, |ui| {
                        let in_use = used.contains(&b.id);
                        if ui.add_enabled(!in_use, egui::Button::new("Quitar")).on_hover_text(if in_use { "Tiene citas: no se puede quitar" } else { "Quitar este gabinete" }).clicked() {
                            remove = Some(b.id);
                        }
                    });
                });
            });
            ui.add_space(4.0);
        }
    });
    if let Some(id) = remove {
        app.db.boxes.retain(|b| b.id != id);
    }
    if serde_json::to_string(&app.db.boxes).unwrap_or_default() != before {
        app.changed();
    }
}

/// Numeraciones, cobro domiciliado y VERI*FACTU.
fn billing_tab(app: &mut App, ui: &mut Ui) {
    let before = serde_json::to_string(&app.db.config).unwrap_or_default();
    ui.columns(2, |cols| {
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            let c = &mut app.db.config;
            section(ui, "Numeraciones");
            ui.horizontal(|ui| {
                field(ui, "Serie de recibos", &mut c.receipt_series, 120.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Siguiente nº").size(13.5).color(MUTED));
                    ui.add(egui::DragValue::new(&mut c.next_receipt).range(1..=999_999));
                });
            });
            ui.vertical(|ui| {
                ui.label(RichText::new("Validez de los presupuestos").size(13.5).color(MUTED));
                ui.add(egui::DragValue::new(&mut c.budget_valid_days).range(1..=730).suffix(" días"));
            });
            ui.label(RichText::new("Las facturas se numeran solas, sin huecos.").font(mdf(12.8)).color(MUTED));
            ui.add_space(8.0);
            section(ui, "Exención de IVA en las facturas nuevas");
            for v in crate::model::VatExemption::ALL {
                ui.radio_value(&mut c.vat_exemption, v, format!("Art. {}: {}", v.article(), v.scope()));
            }
            ui.label(RichText::new("La elección es de la gestoría. Cada factura guarda la suya al emitirse: las ya emitidas no cambian.").font(mdf(12.8)).color(MUTED));
        });
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            let c = &mut app.db.config;
            section(ui, "Cobro domiciliado (SEPA)");
            field(ui, "Identificador de acreedor", &mut c.creditor_id, 200.0);
            field(ui, "IBAN de la clínica", &mut c.clinic_iban, 280.0);
            if !c.clinic_iban.trim().is_empty() && !crate::extra::iban_valid(&c.clinic_iban) {
                ui.label(RichText::new("⚠ El IBAN no es válido").color(DANGER));
            }
        });
    });
    if serde_json::to_string(&app.db.config).unwrap_or_default() != before {
        let was: crate::model::ClinicConfig = serde_json::from_str(&before).unwrap_or_else(|_| app.db.config.clone());
        if was.vat_exemption != app.db.config.vat_exemption {
            let msg = format!("Facturas nuevas: art. {} (antes {})", app.db.config.vat_exemption.article(), was.vat_exemption.article());
            app.db.audit("Cambiar la exención de IVA", &msg);
        }
        app.changed();
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("VERI*FACTU").font(sbf(16.0)).color(HEADING));
        right_aligned(ui, |ui| {
            if ui.button("Ver los envíos a la AEAT ›").clicked() {
                app.screen = crate::app::Screen::Facturas;
                app.lists.fact_tab = 2;
            }
        });
    });
    ui.add_space(4.0);
    crate::screens::verifactu::settings(app, ui);
}

/// Hora en un solo campo («09:30»: se escribe o se arrastra, de 5 en 5 minutos).
fn hm_edit(ui: &mut Ui, t: &mut NaiveTime) -> bool {
    let mut m = (t.hour() * 60 + t.minute()) as f64;
    let r = ui.add(egui::DragValue::new(&mut m).range(0.0..=1435.0).speed(2.0).custom_formatter(|v, _| format!("{:02}:{:02}", v as u32 / 60, v as u32 % 60)).custom_parser(|s| {
        let s = s.trim();
        let (h, mi) = s.split_once(':').unwrap_or((s, "0"));
        Some(h.trim().parse::<f64>().ok()? * 60.0 + mi.trim().parse::<f64>().ok()?)
    }));
    if r.changed() {
        let v = (m.round() as u32 / 5) * 5;
        *t = NaiveTime::from_hms_opt(v / 60, v % 60, 0).unwrap_or(*t);
    }
    r.changed()
}

/// Horario de una semana: cada día, si se trabaja, de qué hora a qué hora y,
/// con jornada partida, también la tarde. `default`: el horario de un día que
/// se marca. Devuelve si ha cambiado.
pub fn week_editor(ui: &mut Ui, id: impl egui::AsIdSalt, w: &mut crate::horario::Week, default: (NaiveTime, NaiveTime), off: &str) -> bool {
    use crate::horario::{DAYS_SHORT, Shift};
    let before = w.clone();
    let t = |h: u32| NaiveTime::from_hms_opt(h, 0, 0).unwrap();
    egui::Grid::new(id).spacing([8.0, 4.0]).show(ui, |ui| {
        for (d, name) in DAYS_SHORT.iter().enumerate() {
            let mut on = !w.0[d].is_empty();
            if ui.checkbox(&mut on, *name).changed() {
                w.0[d] = if on { vec![Shift::new(default.0, default.1)] } else { vec![] };
            }
            if w.0[d].is_empty() {
                ui.label(RichText::new(off).color(MUTED));
                ui.end_row();
                continue;
            }
            ui.horizontal(|ui| {
                hm_edit(ui, &mut w.0[d][0].start);
                ui.label("a");
                hm_edit(ui, &mut w.0[d][0].end);
            });
            let mut split = w.0[d].len() > 1;
            if ui.checkbox(&mut split, "y de").on_hover_text("Jornada partida: también por la tarde").changed() {
                if split {
                    let morning_end = w.0[d][0].end.min(t(14));
                    w.0[d][0].end = morning_end;
                    w.0[d].push(Shift::new(t(16).max(morning_end), t(20).max(morning_end)));
                } else {
                    w.0[d].truncate(1);
                }
            }
            if w.0[d].len() > 1 {
                ui.horizontal(|ui| {
                    hm_edit(ui, &mut w.0[d][1].start);
                    ui.label("a");
                    hm_edit(ui, &mut w.0[d][1].end);
                });
            }
            if w.0[d].iter().any(|s| s.start >= s.end) || (w.0[d].len() > 1 && w.0[d][1].start < w.0[d][0].end) {
                ui.label(RichText::new("⚠ revise las horas").color(DANGER).size(12.5));
            }
            ui.end_row();
        }
    });
    *w != before
}

pub fn collaborators(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        title(ui, "Colaboradores", "Doctores, higienistas y personal clínico");
        right_aligned(ui, |ui| {
            if primary(ui, "＋ Nuevo colaborador").clicked() {
                let id = app.db.new_id();
                app.db.collaborators.push(Collaborator {
                    id,
                    code: format!("{id:02}"),
                    alias: format!("Dr. Nuevo {id}"),
                    name: String::new(),
                    specialty: "Odontología general".into(),
                    license: String::new(),
                    color: [110, 90, 200],
                    commission_pct: 30.0,
                    active: true,
                    phone: String::new(),
                    centers: vec![],
                    deduct_lab: false,
                    commission_on_insurer: false,
                    hours: None,
                });
                app.lists.collab_sel = Some(id);
                app.changed();
            }
        });
    });
    ui.columns(2, |cols| {
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            let tbl = Table::new("cols", vec![("Cód.", 50.0), ("Alias", 130.0), ("Nombre", 190.0), ("Especialidad", 180.0), ("Estado", 80.0)]);
            tbl.header(ui);
            let mut pick = None;
            for (i, c) in app.db.collaborators.iter().enumerate() {
                if tbl.row(ui, i, app.lists.collab_sel == Some(c.id), &[cell(&c.code), cellc(&c.alias, rgb(c.color)), cell(&c.name), cell(&c.specialty), cellc(if c.active { "Activo" } else { "Baja" }, if c.active { OK } else { MUTED })]).clicked() {
                    pick = Some(c.id);
                }
            }
            if pick.is_some() {
                app.lists.collab_sel = pick;
            }
        });
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            let Some(id) = app.lists.collab_sel else {
                muted(ui, "Seleccione un colaborador.");
                return;
            };
            let today = app.today;
            let month_prod: f64 = app.db.entries.iter().filter(|e| e.doctor_id == id && e.is_act() && e.date >= today - chrono::Duration::days(30)).map(|e| e.amount()).sum();
            let appts_today = app.db.appointments.iter().filter(|a| a.doctor_id == id && a.date == today && !a.state.is_cancelled()).count();
            let clinic = app.db.clinic_week();
            let clinic_span = clinic.span().unwrap_or((app.db.config.open_time, app.db.config.close_time));
            let Some(c) = app.db.collaborators.iter_mut().find(|c| c.id == id) else { return };
            let before = serde_json::to_string(c).unwrap_or_default();
            section(ui, &c.alias.clone());
            muted(ui, format!("Producción últimos 30 días: {} · citas hoy: {}", money(month_prod), appts_today));
            ui.horizontal_wrapped(|ui| {
                field(ui, "Alias (agenda)", &mut c.alias, 160.0);
                field(ui, "Nombre completo", &mut c.name, 220.0);
            });
            ui.horizontal_wrapped(|ui| {
                field(ui, "Especialidad", &mut c.specialty, 200.0);
                field(ui, "Nº colegiado", &mut c.license, 120.0);
                field(ui, "Teléfono", &mut c.phone, 120.0);
            });
            ui.horizontal(|ui| {
                ui.label("Color en agenda");
                let mut col = Color32::from_rgb(c.color[0], c.color[1], c.color[2]);
                if ui.color_edit_button_srgba(&mut col).changed() {
                    c.color = [col.r(), col.g(), col.b()];
                }
                ui.checkbox(&mut c.active, "Activo");
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Comisión");
                ui.add(egui::DragValue::new(&mut c.commission_pct).range(0.0..=100.0).suffix(" %"));
                ui.checkbox(&mut c.deduct_lab, "descontando el laboratorio").on_hover_text("La comisión se calcula sobre lo producido menos el coste de laboratorio de sus tratamientos");
                ui.checkbox(&mut c.commission_on_insurer, "también sobre lo que paga la mutua").on_hover_text("Sin marcar, la comisión es solo sobre lo que paga el paciente; marcada, cuenta también el baremo que paga la mutua por sus actos");
            });
            ui.add_space(6.0);
            section(ui, "Horario de consulta");
            let mut own = c.hours.is_some();
            ui.horizontal(|ui| {
                ui.radio_value(&mut own, false, "El de la clínica");
                ui.radio_value(&mut own, true, "Sus días y horas");
            });
            match (own, c.hours.is_some()) {
                (true, false) => c.hours = Some(clinic.clone()),
                (false, true) => c.hours = None,
                _ => {}
            }
            if let Some(h) = &mut c.hours {
                muted(ui, "Solo se le dan citas (también por internet) en estos días y horas, y siempre dentro del horario de la clínica.");
                week_editor(ui, ("doc_week", id), h, clinic_span, "no pasa consulta");
            }
            if serde_json::to_string(c).unwrap_or_default() != before {
                app.changed();
            }
        });
    });
}

/// Mapa de módulos del programa original y su estado en esta versión.
pub fn modules(app: &mut App, ui: &mut Ui) {
    title(ui, "Más módulos", "Áreas del programa original y su estado en esta versión");
    let done = [
        ("Pacientes / filiación", "Ficha completa, alertas médicas, protección de datos, búsqueda tolerante a acentos y aviso de duplicados"),
        ("Agenda (book diario y semanal)", "Columnas por doctor, bloqueos, arrastrar para mover, huecos libres, lista de espera, situaciones de cita"),
        ("Pizarra / recepción", "Flujo llega → sala de espera → consulta → finalizada con tiempos de espera"),
        ("Odontograma", "Dientes dibujados con su forma real (coronas, raíces, encía) en aspecto claro o tipo radiografía: adulto y temporal, 5 caras, 13 herramientas con iconos y evolución por visitas"),
        ("Imágenes, radiografías y TAC", "Galería con visor (zoom, brillo, contraste, negativo, giro) y enlaces con RVG, ortopantomógrafo, CBCT, cámaras y escáneres mediante IMCONECT.ini"),
        ("Historial médico", "Entradas médicas con precio por tarifa, antecedentes y notas"),
        ("Presupuestos", "Líneas, descuentos, aceptación/rechazo, caducidad, financiación en cuotas"),
        ("Plan económico", "Cuotas con vencimientos y cobro de cada cuota"),
        ("Cobros y recibos", "Recibos numerados por serie, hoja de cobros y exportación CSV"),
        ("Control de caja", "Apertura, entradas/salidas, arqueo por billetes y monedas, descuadres"),
        ("Recalls", "Generación automática al realizar tratamientos y seguimiento de contacto"),
        ("Mutuas / tarifas / tratamientos", "Precio específico o descuento general por compañía"),
        ("Colaboradores y liquidación", "Producción y honorarios por porcentaje"),
        ("Facturación", "Serie correlativa por año, rectificativas, huella SHA-256 encadenada y verificación de integridad"),
        ("VERI*FACTU", "Registros de facturación con el formato oficial de la AEAT, envío con el certificado de la clínica (pruebas o producción), respuesta con CSV por factura, código QR y leyenda en cada factura"),
        ("Periodontograma", "Ficha periodontal a escala en milímetros: margen gingival, fondo de bolsa coloreado por profundidad, sangrado, placa, movilidad, furca y comparación con el sondaje anterior"),
        ("Multicentro", "Varios centros con su agenda, gabinetes, caja y colaboradores; pacientes, historial y facturas compartidos; selector en la barra superior"),
        ("Presupuestos y facturas imprimibles", "Documentos con formato listos para imprimir o guardar en PDF"),
        ("Bancos y conciliación", "Extractos Norma 43 o Excel/CSV, conciliación automática con transferencias, liquidaciones de tarjeta, remesas y gastos"),
        ("Fusión de pacientes duplicados", "Detección por NIF, nombre o móvil y unión de todo el historial con copia de seguridad previa"),
        ("Registro de cambios", "Quién borró, emitió, importó, fusionó o restauró y cuándo"),
        (
            "Copias de seguridad",
            "Copia automática diaria (se guarda un año, escalonada: diarias, semanales y mensuales), aviso si falla, copia manual, segunda copia en un disco USB o en la nube cifrada con la contraseña de las copias (AES-256; sin ella no se puede leer ni modificar) y restauración comprobada, también de copias cifradas en otro ordenador",
        ),
        ("Pizarra de sala de espera", "Pantalla para la televisión: paciente llamado a consulta con su gabinete y lista de espera, con opción de mostrar solo iniciales"),
        ("Listín telefónico", "Contactos de la clínica, colaboradores, proveedores, mutuas y pacientes en una sola búsqueda"),
        ("Telefonía", "Al sonar el teléfono aparece quién llama con su próxima cita, avisos y saldo; registro de llamadas"),
        ("WhatsApp automático", "Recordatorio con botones «Sí, confirmo» / «No puedo ir» el día laborable anterior y lectura automática de la respuesta (plataforma oficial de Meta)"),
        ("Consultas de pacientes", "Listas por edad, tarifa, doctor, última visita, deuda, presupuestos o tratamientos; exportación y mensajes"),
        ("Importación de datos", "Pacientes y tratamientos con precios desde Excel o CSV, sin duplicar pacientes"),
        ("Importación desde SQL Server", "Conexión de solo lectura con GESDEN y Novaclinic (catálogo, tarifas y pacientes) o con cualquier otro programa eligiendo tabla o consulta"),
        (
            "Receta oficial: repositorio propio",
            "Repositorio de recetas con los servicios para farmacias de Nodofarma, firma CAdES con el certificado del dentista, comprobación del colegiado y hoja del paciente con DataMatrix; pendiente de la certificación oficial",
        ),
        ("Ortodoncia", "Casos con aparatología, clase, objetivos, controles con higiene y recall automático del siguiente control"),
        ("Remesas SEPA", "Cuotas domiciliadas con IBAN y mandato validados, fichero bancario pain.008 y registro de cobros"),
        (
            "Usuarios, contraseñas y permisos",
            "Acceso con contraseña y, si se activa, con el código de 6 cifras de una app del móvil (doble factor, que se puede exigir para firmar recetas); perfiles (dirección, doctor, higienista, recepción), bloqueo por inactividad e intentos y registro de accesos",
        ),
        ("Gastos", "Facturas recibidas, IVA soportado, vencimientos, resultado mensual; pedidos y laboratorio se apuntan solos"),
        ("Recetas", "Vademécum básico con avisos de alergias, anticoagulación y otras alertas; receta imprimible"),
        ("Almacén y pedidos", "Stock con mínimos, consumo automático por tratamiento, propuesta de pedido, envío y recepción, proveedores"),
        ("Laboratorio protésico", "Trabajos enviados, color y material, fechas previstas, pruebas, repeticiones, retrasos y avisos en la agenda"),
        ("Comunicaciones automáticas", "Recordatorios de cita, recalls, cumpleaños y presupuestos por WhatsApp, SMS y email, respetando el consentimiento RGPD"),
        ("Documentos y consentimientos", "Plantillas editables, firma manuscrita en pantalla o tableta y exportación imprimible"),
        ("Informes", "Pendientes, libro diario, producción, ingresos mensuales, presupuestos pendientes"),
        ("Análisis de radiografías con IA", "Hallazgos por pieza y propuesta de tratamiento con varios proveedores (OpenRouter, Gemini, OpenAI, Claude, Groq, Mistral, Ollama…); agentes gratuitos y de pago con respaldo automático si uno falla"),
        ("Envío digital al laboratorio", "Orden de trabajo con el esquema de las piezas y los escaneados, fotos y radiografías del paciente en un paquete, enviada por correo desde el programa"),
        ("Correo electrónico", "Servidor de correo propio (Gmail, Outlook, IONOS, OVH o el de la clínica) con adjuntos y correo de prueba"),
        ("Cita online", "Página para pedir cita desde el móvil o la web con los huecos libres de la agenda; las solicitudes se confirman o rechazan en la agenda y el paciente recibe la cita por correo para su calendario; cartel con código QR"),
        ("Enlace «Mi cita»", "Enlace personal de cada cita para que el paciente confirme, anule, pida otra hora o rellene el cuestionario de salud desde casa; llega a la agenda y a la ficha, y se puede poner en los recordatorios con [ENLACE]"),
        (
            "Presupuestos para firmar por internet",
            "El paciente revisa el presupuesto en el móvil, elige pagarlo por tratamiento o en cuotas sin intereses y lo acepta con su firma; queda aceptado, con sus tratamientos en el plan, el documento firmado en la ficha y una copia firmada por correo; también puede preguntar o decir que no le interesa",
        ),
        (
            "Publicar en internet",
            "Túnel de Cloudflare integrado: la página de la clínica (cita online, «Mi cita», presupuestos) sale en una dirección https propia sin abrir puertos en el router; se abre solo al arrancar y se comprueba desde internet con un botón",
        ),
        (
            "Consentimientos para firmar por internet",
            "Consentimientos informados, protección de datos y otros documentos se envían con un enlace: el paciente los lee con calma, pregunta sus dudas y los firma con el dedo (o elige firmarlos en la clínica); aparecen también en «Mi cita», quedan firmados en la ficha con la prueba de la firma y le llega una copia por correo",
        ),
        (
            "Envío automático por correo",
            "Cada día, a la hora elegida y desde un solo ordenador, se preparan los recordatorios de cita, avisos de revisión, felicitaciones y seguimientos de presupuesto y se envían por correo con el servidor de la clínica, cada uno con su enlace (gestionar la cita, pedir cita o firmar el presupuesto); a quien no acepta correo le llegan por WhatsApp o SMS como siempre, y los fallos quedan anotados para reintentarlos",
        ),
        (
            "Mis recetas por internet",
            "Enlace personal para que el paciente vea en el móvil sus recetas electrónicas, en qué estado están en las farmacias y la hoja con los códigos para enseñarla al farmacéutico; entra con su DNI/NIE y su fecha de nacimiento, el enlace se bloquea tras 5 intentos fallidos y cada acceso queda registrado",
        ),
        (
            "Puesta en marcha",
            "Lista de lo que falta para trabajar con la clínica real (datos de la clínica, profesionales, contraseñas, copias, VERI*FACTU, protección de datos y lo opcional), comprobada con la configuración y con un botón a cada sitio; con «Empezar con la clínica vacía» se quitan los pacientes y datos de ejemplo tras guardar una copia, sin tocar tratamientos, tarifas, profesionales ni usuarios",
        ),
        (
            "Huecos libres para la lista de espera",
            "Cuando se anula una cita (sola si es por internet, con un botón desde la agenda), el hueco se ofrece a quienes esperan con ese profesional y les encaja la hora, empezando por quien más lleva esperando; cada uno recibe su enlace y el primero que lo pide se queda la cita confirmada y sale de la lista; los demás ven que ya está ocupado",
        ),
    ];
    let soon = [
        ("TicketBAI (País Vasco)", "Registros para las haciendas forales de Álava, Bizkaia y Gipuzkoa"),
        (
            "Receta electrónica con farmacias reales",
            "Todo preparado en el programa: firma, repositorio, hoja por correo, circuito completo probado con una farmacia simulada y expediente para pedir la certificación (Configuración › Receta electrónica › Camino a las farmacias reales). Falta que los Consejos Generales certifiquen el sistema, la prueba conjunta con Nodofarma y publicar el repositorio en un servidor siempre encendido",
        ),
        ("Datáfono y financiación externa", "Cobro con datáfono integrado y financiación de presupuestos con entidades"),
        ("Portales de aseguradoras", "Envío directo de autorizaciones y liquidaciones a las compañías"),
    ];
    integrations(app, ui);
    ui.add_space(10.0);
    ui.columns(2, |cols| {
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Disponible en esta versión");
            for (n, d) in done {
                ui.horizontal(|ui| {
                    pill(ui, "✔", OK);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(n).family(crate::theme::sb()));
                        muted(ui, d);
                    });
                });
            }
        });
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Próximamente");
            for (n, d) in soon {
                ui.horizontal(|ui| {
                    pill(ui, "…", WARN);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(n).family(crate::theme::sb()));
                        muted(ui, d);
                    });
                });
            }
        });
    });
}

/// Conexiones con equipos y servicios externos: qué hay y dónde se configura
/// cada una (antes estaba en «Imágenes y enlaces» como lista informativa).
fn integrations(app: &mut App, ui: &mut Ui) {
    use crate::app::Screen;
    let items: [(&str, &str, Screen, &str); 11] = [
        ("Radiología y TAC", "RVG, ortopantomógrafo, CBCT, cámaras y escáneres: se abren con el paciente activo", Screen::Imagenes, "Imágenes y enlaces"),
        ("Análisis de radiografías con IA", "Agentes gratuitos y de pago de varios proveedores, con respaldo si uno falla", Screen::Config, "Configuración"),
        ("WhatsApp, SMS y email", "Recordatorios y confirmación automática de citas con la plataforma oficial de Meta", Screen::Config, "Configuración"),
        ("Telefonía", "Ficha del paciente al recibir una llamada, con la centralita o el programa de teléfono", Screen::Config, "Configuración"),
        ("Receta electrónica", "Certificado del dentista, colegiado y conexión con el repositorio de recetas", Screen::Config, "Configuración"),
        ("Trabajo en red", "Varios puestos de la clínica conectados al ordenador principal", Screen::Config, "Configuración"),
        ("Importar desde otros programas", "GESDEN, Novaclinic u otro programa con SQL Server, y Excel o CSV", Screen::Config, "Configuración"),
        ("Facturación VERI*FACTU", "Envío de cada factura a la AEAT con el certificado de la clínica y código QR en la factura", Screen::Facturas, "Facturas"),
        ("Remesas SEPA", "Cuotas domiciliadas y fichero para el banco", Screen::Cobros, "Cobros"),
        ("Bancos", "Extractos Norma 43 o Excel y conciliación automática", Screen::Bancos, "Bancos"),
        ("Laboratorio protésico", "Seguimiento de trabajos, pruebas y avisos en la agenda", Screen::Laboratorio, "Laboratorio"),
    ];
    let mut go = None;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Integraciones");
        muted(ui, "Conexiones con equipos y servicios externos, y dónde se configura cada una.");
        ui.add_space(8.0);
        let gap = 12.0;
        let col_w = ((ui.available_width() - gap) / 2.0).floor();
        for pair in items.chunks(2) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (name, desc, screen, place) in pair {
                    egui::Frame::new().fill(ROW_ALT).stroke(egui::Stroke::new(1.0, HAIRLINE)).corner_radius(egui::CornerRadius::same(14)).inner_margin(egui::Margin::symmetric(14, 12)).show(ui, |ui| {
                        ui.set_width(col_w - 30.0);
                        ui.set_min_height(58.0);
                        ui.horizontal_top(|ui| {
                            // icono de la sección donde se configura
                            let (r, _) = ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
                            ui.painter().rect_filled(r, egui::CornerRadius::same(10), tint(ACCENT, 0.12));
                            crate::icons::nav_icon(ui.painter(), *screen, r.center(), ACCENT_DARK);
                            ui.add_space(4.0);
                            let text_w = ui.available_width() - 84.0;
                            ui.vertical(|ui| {
                                ui.set_width(text_w);
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.label(RichText::new(*name).font(crate::theme::sbf(14.5)).color(HEADING));
                                ui.label(RichText::new(*desc).font(crate::theme::mdf(12.8)).color(MUTED));
                                ui.label(RichText::new(format!("Se configura en {place}")).font(crate::theme::mdf(12.0)).color(ACCENT_DARK));
                            });
                            // el texto deja un hueco fijo a la derecha para el botón
                            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 34.0), egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                if app.allowed(*screen) {
                                    if ui.button("Abrir ›").on_hover_text(format!("Ir a {place}")).clicked() {
                                        go = Some((*screen, *name));
                                    }
                                } else {
                                    ui.label(RichText::new("Sin permiso").font(crate::theme::mdf(12.0)).color(SUBTLE)).on_hover_text("Su perfil de usuario no puede abrir esta sección");
                                }
                            });
                        });
                    });
                }
            });
            ui.add_space(gap - ui.spacing().item_spacing.y);
        }
    });
    if let Some((s, name)) = go {
        // en Configuración se abre directamente el apartado que corresponde
        let tab = match name {
            n if n.contains("IA") => Some(CfgTab::Ia),
            n if n.starts_with("WhatsApp") => Some(CfgTab::Whatsapp),
            "Telefonía" => Some(CfgTab::Telefonia),
            "Receta electrónica" => Some(CfgTab::Receta),
            "Trabajo en red" => Some(CfgTab::Red),
            n if n.starts_with("Importar") => Some(CfgTab::Importar),
            _ => None,
        };
        match (s, tab) {
            (crate::app::Screen::Config, Some(t)) => open(app, t),
            (crate::app::Screen::Facturas, _) => {
                app.screen = s;
                app.lists.fact_tab = 2;
            }
            _ => app.screen = s,
        }
    }
}

/// Libros registro de IVA e IRPF en el formato de la AEAT (ver `crate::libros`).
fn books_card(app: &mut App, ui: &mut Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Libros registro para la gestoría (formato de la AEAT)");
        muted(ui, "Facturas emitidas y gastos del año en el formato electrónico común de la AEAT (XLSX), el que se importa en el Pre303 y en Renta WEB. Revise el resultado con su gestoría: los conceptos de gasto son una propuesta.");
        let before = (app.db.config.iae_epigrafe.clone(), app.db.config.actividad_tipo.clone());
        ui.horizontal_wrapped(|ui| {
            field(ui, "Epígrafe del IAE de la clínica", &mut app.db.config.iae_epigrafe, 140.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Tipo de actividad").size(13.0).color(MUTED));
                let cur = crate::libros::Activity::of(&app.db).kind;
                crate::theme::combo("libros_tipo")
                    .width(320.0)
                    .selected_text(match cur.as_str() {
                        "03" => "03 · Actividades empresariales",
                        "04" => "04 · Profesionales artísticas o deportivas",
                        _ => "05 · Resto de actividades profesionales",
                    })
                    .show_ui(ui, |ui| {
                        for (k, n) in [("05", "05 · Resto de actividades profesionales"), ("03", "03 · Actividades empresariales"), ("04", "04 · Profesionales artísticas o deportivas")] {
                            ui.selectable_value(&mut app.db.config.actividad_tipo, k.to_string(), n);
                        }
                    });
            });
            ui.vertical(|ui| {
                ui.label(RichText::new("Año").size(13.0).color(MUTED));
                ui.add(egui::DragValue::new(&mut app.lists.books_year).range(2000..=2100));
            });
        });
        if before != (app.db.config.iae_epigrafe.clone(), app.db.config.actividad_tipo.clone()) {
            app.changed();
        }
        if primary(ui, "Generar los libros registro").clicked() {
            let dir = crate::paths::documents().join(crate::brand::DIR);
            app.lists.books_result = Some(crate::libros::export(&app.db, app.lists.books_year, &dir));
        }
        match &app.lists.books_result {
            Some(Ok((path, problems))) => {
                ui.label(RichText::new(format!("Guardado en {}", path.display())).color(OK));
                if problems.is_empty() {
                    muted(ui, "Pasa todas las validaciones del diseño oficial que se comprueban aquí.");
                }
                for p in problems.iter().take(30) {
                    ui.label(RichText::new(p).color(WARN));
                }
                if problems.len() > 30 {
                    muted(ui, format!("… y {} más.", problems.len() - 30));
                }
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(DANGER));
            }
            None => {}
        }
    });
}
