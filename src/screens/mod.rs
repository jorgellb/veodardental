pub mod agenda;
pub mod almacen;
pub mod avisos;
pub mod bancos;
pub mod caja;
pub mod centros;
pub mod cobros;
pub mod comunicaciones;
pub mod config;
pub mod documentos;
pub mod facturas;
pub mod fichas;
pub mod gastos;
pub mod herramientas;
pub mod imagenes;
pub mod importar;
pub mod informes;
pub mod inicio;
pub mod laboratorio;
pub mod llamadas;
pub mod marcha;
pub mod mutuas;
pub mod login;
pub mod odontograma;
pub mod ortodoncia;
pub mod pacientes;
pub mod perio;
pub mod presupuestos;
pub mod puestos;
pub mod recalls;
pub mod recetas;
pub mod remesas;
pub mod salud;
pub mod tareas;
pub mod tarifas;
pub mod whatsapp;
pub mod ia;
pub mod verifactu;
pub mod correo;
pub mod cita_online;
pub mod odg_paint;
pub mod teeth;

use crate::app::{App, ConfirmAction, Dialog};
use crate::theme::*;
use chrono::NaiveDate;
use egui::RichText;

/// Estado de filtros y selecciones de las pantallas de listado.
pub struct ListsState {
    pub patient_list_open: bool,
    pub patient_filter: String,
    pub patient_only_debt: bool,
    pub budget_filter: Option<crate::model::BudgetState>,
    pub budget_text: String,
    pub recall_filter: Option<crate::model::RecallState>,
    pub recall_until_today: bool,
    pub report_from: NaiveDate,
    pub report_to: NaiveDate,
    pub report_tab: usize,
    pub tariff_sel: u32,
    pub treat_filter: String,
    pub treat_sel: Option<String>,
    pub collab_sel: Option<u32>,
    pub cash_counts: [u32; 15],
    pub cash_move_amount: f64,
    pub cash_move_text: String,
    pub cash_opening: f64,
    /// Caja que se ve si hay más de una abierta (dos puestos la abrieron a la vez).
    pub cash_view: Option<u32>,
    pub pay_from: NaiveDate,
    pub pay_to: NaiveDate,
    pub pay_method: Option<String>,
    pub history_filter: Option<crate::model::TxState>,
    pub reject_reason: String,
    pub inv_from: NaiveDate,
    pub inv_to: NaiveDate,
    pub inv_text: String,
    pub fact_tab: u8,
    pub claim_tariff: u32,
    pub rect_reason: String,
    pub perio_sel: Option<u32>,
    pub perio_tooth: Option<u8>,
    /// Desplazamiento inicial de la ficha del paciente (para capturas de pantalla).
    pub scroll_to: Option<f32>,
    /// Apartado abierto en Configuración.
    pub cfg_tab: crate::screens::config::CfgTab,
}

impl Default for ListsState {
    fn default() -> Self {
        let t = crate::model::today();
        Self {
            patient_list_open: false,
            patient_filter: String::new(),
            patient_only_debt: false,
            budget_filter: None,
            budget_text: String::new(),
            recall_filter: Some(crate::model::RecallState::Pendiente),
            recall_until_today: false,
            report_from: t - chrono::Duration::days(90),
            report_to: t,
            report_tab: 0,
            tariff_sel: 2,
            treat_filter: String::new(),
            treat_sel: None,
            collab_sel: Some(1),
            cash_counts: [0; 15],
            cash_move_amount: 0.0,
            cash_move_text: String::new(),
            cash_opening: 150.0,
            cash_view: None,
            pay_from: t - chrono::Duration::days(30),
            pay_to: t,
            pay_method: None,
            history_filter: None,
            reject_reason: String::new(),
            inv_from: chrono::NaiveDate::from_ymd_opt(chrono::Datelike::year(&t), 1, 1).unwrap_or(t),
            inv_to: t,
            inv_text: String::new(),
            fact_tab: 0,
            claim_tariff: 2,
            rect_reason: String::new(),
            perio_sel: None,
            perio_tooth: None,
            scroll_to: None,
            cfg_tab: Default::default(),
        }
    }
}

/// Selector de fecha compacto: día anterior / siguiente / hoy y edición dd/mm/aaaa.
pub fn date_picker(ui: &mut egui::Ui, id: &str, d: &mut NaiveDate) -> bool {
    let mut changed = false;
    ui.push_id(id, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if crate::theme::arrow_button(ui, true, "Día anterior").clicked() {
                *d -= chrono::Duration::days(1);
                changed = true;
            }
            let key = ui.scope_id().with("txt");
            let mut txt: String = ui.data_mut(|m| m.get_temp::<String>(key)).unwrap_or_else(|| crate::model::fmt_date(*d));
            let r = ui.add_sized([112.0, 34.0], egui::TextEdit::singleline(&mut txt).align(egui::Align2::CENTER_TOP));
            if r.changed() {
                if let Ok(nd) = NaiveDate::parse_from_str(txt.trim(), "%d/%m/%Y") {
                    *d = nd;
                    changed = true;
                }
                ui.data_mut(|m| m.insert_temp(key, txt));
            } else if !r.has_focus() {
                ui.data_mut(|m| m.remove::<String>(key));
            }
            if crate::theme::arrow_button(ui, false, "Día siguiente").clicked() {
                *d += chrono::Duration::days(1);
                changed = true;
            }
        });
    });
    changed
}

/// Selector de tratamiento con buscador. Con un catálogo real (el de GESDEN
/// tiene cientos de actos) una lista sin buscar es imposible de usar: se
/// escribe parte del nombre, del código o del grupo y se elige. Solo ofrece
/// los tratamientos de la tarifa del paciente. Devuelve si cambió.
#[allow(clippy::too_many_arguments)]
pub fn treatment_picker(
    ui: &mut egui::Ui,
    id: &str,
    db: &crate::model::Db,
    tariff_id: u32,
    code: &mut String,
    width: f32,
    keep: impl Fn(&crate::model::Treatment) -> bool,
    show: impl Fn(&crate::model::Treatment) -> String,
) -> bool {
    use crate::model::fold;
    let mut changed = false;
    let cur = db.treatment(code).filter(|t| db.treatment_offered(t, tariff_id)).map(&show).unwrap_or_else(|| "Elegir tratamiento…".into());
    let qkey = egui::Id::unique(("tpick_q", id));
    crate::theme::combo(("tpick", id))
        .width(width)
        .height(440.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .selected_text(cur)
        .show_ui(ui, |ui| {
            let mut q: String = ui.data_mut(|d| d.get_temp(qkey)).unwrap_or_default();
            let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text("🔍 Buscar por nombre, código o grupo").desired_width(width.max(280.0)));
            if r.changed() {
                ui.data_mut(|d| d.insert_temp(qkey, q.clone()));
            }
            if q.is_empty() && !r.has_focus() {
                r.request_focus();
            }
            let f = fold(q.trim());
            let mut last = String::new();
            let mut n = 0;
            for t in db.treatments_for(tariff_id).filter(|t| keep(t)) {
                if !f.is_empty() && !(fold(&t.description).contains(&f) || fold(&t.group).contains(&f) || fold(&t.code).contains(&f)) {
                    continue;
                }
                if t.group != last {
                    ui.label(RichText::new(&t.group).size(12.5).color(MUTED));
                    last = t.group.clone();
                }
                if ui.selectable_label(*code == t.code, show(t)).clicked() {
                    *code = t.code.clone();
                    changed = true;
                    ui.data_mut(|d| d.remove::<String>(qkey));
                    ui.close();
                }
                n += 1;
            }
            if n == 0 {
                ui.label(RichText::new("Ningún tratamiento coincide con la búsqueda").color(MUTED));
            }
        });
    changed
}

pub fn dialogs(app: &mut App, ctx: &egui::Context) {
    if app.tasks.open && matches!(app.dialog, Dialog::None) {
        tareas::window(app, ctx);
    }
    let dialog = std::mem::replace(&mut app.dialog, Dialog::None);
    app.dialog = match dialog {
        Dialog::None => Dialog::None,
        Dialog::Appointment(d) => agenda::appt_dialog(app, ctx, d),
        Dialog::Payment(d) => cobros::pay_dialog(app, ctx, d),
        Dialog::NewPatient(p) => pacientes::new_dialog(app, ctx, p),
        Dialog::Budget(id) => presupuestos::edit_dialog(app, ctx, id),
        Dialog::Invoice(id) => facturas::invoice_dialog(app, ctx, id),
        Dialog::Confirm { text, action } => {
            let mut keep = true;
            crate::theme::modal("confirm").show(ctx, |ui| {
                ui.set_width(360.0);
                section(ui, "Confirmar");
                ui.label(&text);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if danger(ui, "Sí, continuar").clicked() {
                        run_confirm(app, action.clone());
                        keep = false;
                    }
                    if ui.button("Cancelar").clicked() {
                        keep = false;
                    }
                });
            });
            if keep { Dialog::Confirm { text, action } } else { app.dialog_take_or_none() }
        }
    };
}

impl App {
    /// Tras cerrar un diálogo, conserva cualquier otro que se haya abierto mientras tanto.
    pub fn dialog_take_or_none(&mut self) -> Dialog {
        std::mem::replace(&mut self.dialog, Dialog::None)
    }
}

/// Pregunta antes de borrar un tratamiento del historial (o avisa de por qué
/// no se puede: está facturado o liquidado a la mutua).
pub fn ask_delete_entry(app: &mut App, id: u32) {
    if let Err(e) = app.db.can_delete_entry(id) {
        app.warn(e);
        return;
    }
    let returns = app.db.stock_moves.iter().any(|m| m.entry_id == Some(id));
    let text = if returns { "¿Borrar este tratamiento del historial? El material que gastó vuelve al almacén." } else { "¿Borrar este tratamiento del historial?" };
    app.dialog = Dialog::Confirm { text: text.into(), action: ConfirmAction::DeleteEntry(id) };
}

fn run_confirm(app: &mut App, action: ConfirmAction) {
    match action {
        ConfirmAction::DeleteAppointment(id) => {
            app.db.audit("Borrar cita", &format!("cita #{id}"));
            app.db.appointments.retain(|a| a.id != id);
            app.db.refresh_recalls(app.today);
            app.notify("Cita eliminada.");
        }
        ConfirmAction::DeleteEntry(id) => {
            if let Err(e) = app.db.delete_entry(id) {
                app.warn(e);
                return;
            }
            app.notify("Tratamiento borrado del historial.");
        }
        ConfirmAction::DeleteBudget(id) => {
            if let Some(why) = app.db.plan_locked(id) {
                app.warn(format!("No se puede borrar: {why}."));
                return;
            }
            app.db.audit("Borrar presupuesto", &format!("presupuesto #{id}"));
            app.db.budgets.retain(|b| b.id != id);
            for e in app.db.entries.iter_mut() {
                if e.budget_id == Some(id) {
                    e.budget_id = None;
                }
            }
            app.notify("Presupuesto borrado.");
        }
        ConfirmAction::ResetSample => {
            app.db = crate::model::Db::sample(app.today);
            app.patient = app.db.patients.first().map(|p| p.id);
            app.notify("Datos de ejemplo restaurados.");
        }
        ConfirmAction::StartEmpty => {
            if !marcha::start_empty(app) {
                return;
            }
        }
    }
    app.changed();
}

