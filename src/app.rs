//! Ventana principal: barra lateral de módulos, barra superior con búsqueda
//! de pacientes, barra de estado y despacho a cada pantalla.

use crate::model::*;
use crate::screens;
use crate::theme::{self, *};
use chrono::NaiveDate;
use egui::{Align2, Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui, vec2};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Inicio,
    Pacientes,
    Agenda,
    Presupuestos,
    Cobros,
    Facturas,
    Caja,
    Recalls,
    Comunicaciones,
    Listin,
    Pizarra,
    Imagenes,
    Almacen,
    Laboratorio,
    Ortodoncia,
    Gastos,
    Bancos,
    Tarifas,
    Colaboradores,
    Informes,
    Config,
    Modulos,
}

impl Screen {
    pub const NAV: [(Screen, &'static str, &'static str); 21] = [
        (Screen::Inicio, "🏠", "Inicio"),
        (Screen::Agenda, "📅", "Agenda"),
        (Screen::Pacientes, "👥", "Pacientes"),
        (Screen::Presupuestos, "📋", "Presupuestos"),
        (Screen::Cobros, "💳", "Cobros"),
        (Screen::Facturas, "📄", "Facturas"),
        (Screen::Caja, "💰", "Caja"),
        (Screen::Recalls, "🔔", "Recalls"),
        (Screen::Comunicaciones, "💬", "Comunicaciones"),
        (Screen::Listin, "☎", "Listín telefónico"),
        (Screen::Imagenes, "📷", "Imágenes y enlaces"),
        (Screen::Almacen, "📦", "Almacén"),
        (Screen::Laboratorio, "🦷", "Laboratorio"),
        (Screen::Ortodoncia, "😁", "Ortodoncia"),
        (Screen::Gastos, "🧾", "Gastos"),
        (Screen::Bancos, "🏦", "Bancos"),
        (Screen::Tarifas, "🏷", "Tarifas y tratamientos"),
        (Screen::Colaboradores, "👤", "Colaboradores"),
        (Screen::Informes, "📊", "Informes"),
        (Screen::Config, "⚙", "Configuración"),
        (Screen::Modulos, "➕", "Más módulos"),
    ];
    pub fn from_name(s: &str) -> Option<Screen> {
        let f = fold(s);
        if f.starts_with("pizarra") {
            return Some(Screen::Pizarra);
        }
        Screen::NAV.iter().find(|(_, _, n)| fold(n).starts_with(&f)).map(|x| x.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PatTab {
    Resumen,
    Filiacion,
    Salud,
    Odontograma,
    Periodontograma,
    Imagenes,
    Documentos,
    Historial,
    Presupuestos,
    Cuenta,
    Citas,
}

impl PatTab {
    pub const ALL: [(PatTab, &'static str); 11] = [
        (PatTab::Resumen, "Resumen"),
        (PatTab::Filiacion, "Filiación"),
        (PatTab::Salud, "Salud"),
        (PatTab::Odontograma, "Odontograma"),
        (PatTab::Periodontograma, "Periodontograma"),
        (PatTab::Imagenes, "Imágenes"),
        (PatTab::Documentos, "Documentos"),
        (PatTab::Historial, "Historial"),
        (PatTab::Presupuestos, "Presupuestos"),
        (PatTab::Cuenta, "Cuenta"),
        (PatTab::Citas, "Citas"),
    ];
}

/// Diálogos modales activos.
pub enum Dialog {
    None,
    Appointment(screens::agenda::ApptDraft),
    Payment(screens::cobros::PayDraft),
    NewPatient(Box<Patient>),
    Budget(u32),
    Invoice(u32),
    Confirm { text: String, action: ConfirmAction },
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    DeleteAppointment(u32),
    DeleteEntry(u32),
    DeleteBudget(u32),
    ResetSample,
    StartEmpty,
}

pub struct Screenshot {
    pub path: PathBuf,
    pub frames: u32,
    /// 0 = pendiente, 1 = pedida, 2 = guardada, 3 = error
    pub state: Arc<AtomicU8>,
}

pub struct App {
    pub db: Db,
    pub today: NaiveDate,
    pub screen: Screen,
    pub patient: Option<u32>,
    pub pat_tab: PatTab,
    pub search: String,
    pub dialog: Dialog,
    pub toast: Option<(String, f64, Color32)>,
    pub dirty: bool,
    pub persist: bool,
    pub last_save: f64,
    pub save_error: Option<String>,
    /// Últimos datos leídos o guardados (para combinar con otros puestos).
    pub base: serde_json::Value,
    pub last_poll: f64,
    /// Conexión con el servidor de la clínica (puesto cliente).
    pub remote: Option<crate::server::Remote>,
    /// Sin conexión con el servidor: se muestra la pantalla de reintento.
    pub offline: Option<String>,
    pub screenshot: Option<Screenshot>,
    pub agenda: screens::agenda::AgendaState,
    pub odg: screens::odontograma::OdgState,
    pub lists: screens::ListsState,
    pub img: screens::imagenes::ImgState,
    pub comms: screens::comunicaciones::CommsState,
    pub docs: screens::documentos::DocsState,
    pub stock: screens::almacen::StockState,
    pub gastos: screens::gastos::GastosState,
    pub rx: screens::recetas::RxState,
    pub ia: screens::ia::IaUi,
    pub vf: screens::verifactu::VfUi,
    pub mail_ui: screens::correo::MailUi,
    pub booking: screens::cita_online::BookingUi,
    pub marcha: screens::marcha::MarchaUi,
    pub tasks: screens::tareas::TaskState,
    pub chat: screens::avisos::ChatState,
    pub phone: screens::llamadas::CallState,
    pub wa: screens::whatsapp::WaUi,
    pub login: screens::login::LoginState,
    pub user_id: Option<u32>,
    /// Esta sesión entró con el código de la app del móvil (segundo factor).
    pub second_factor: bool,
    pub last_activity: f64,
    pub docs_mode: u8,
    pub imp: screens::importar::ImportState,
    pub ortho: screens::ortodoncia::OrthoUi,
    pub rem: screens::remesas::RemState2,
    pub cobros_tab: u8,
    pub tools: screens::herramientas::ToolsState,
    pub bank: screens::bancos::BankUi,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>, start: Option<Screen>, patient: Option<u32>, tab: Option<PatTab>) -> Self {
        theme::apply(&cc.egui_ctx);
        let live = shot.is_none();
        let mut persist = shot.is_none();
        let (mut db, mut err) = if persist { Db::load_or_sample() } else { (Db::sample(today()), None) };
        if persist && crate::shared::shared() {
            if !Db::path().exists() {
                err = Some(format!("No se encuentra la carpeta compartida {}: se muestran datos de ejemplo sin guardar.", crate::shared::station().data_dir));
                persist = false;
            } else if err.is_some() {
                persist = false;
            }
            db.station = crate::shared::station().number;
        }
        let mut base = serde_json::to_value(&db).unwrap_or_default();
        let mut remote = None;
        let mut offline = None;
        if persist && crate::shared::remote_mode() {
            let st = crate::shared::station();
            let local = crate::server::is_local(&st.server);
            if local && !crate::server::local_server_running() {
                let _ = crate::server::spawn_local_server();
            }
            let mut r = crate::server::Remote::new(&st.server, &st.key, 0);
            match crate::server::connect(&mut r, &crate::shared::machine_name(), if local { 20 } else { 1 }) {
                Ok((d, v)) => {
                    db = crate::server::adopt(&db, d);
                    db.station = r.station;
                    base = v;
                    err = None;
                }
                Err(e) => {
                    offline = Some(e);
                    persist = false;
                }
            }
            remote = Some(r);
        }
        let mut app = App {
            db,
            today: today(),
            screen: start.unwrap_or(Screen::Inicio),
            patient: patient.or(Some(100)),
            pat_tab: tab.unwrap_or(PatTab::Resumen),
            search: String::new(),
            dialog: Dialog::None,
            toast: None,
            dirty: false,
            persist,
            last_save: 0.0,
            save_error: err,
            base,
            last_poll: 0.0,
            remote,
            offline,
            screenshot: shot.map(|path| Screenshot { path, frames: 0, state: Arc::new(AtomicU8::new(0)) }),
            agenda: screens::agenda::AgendaState::new(today()),
            odg: screens::odontograma::OdgState::default(),
            lists: screens::ListsState::default(),
            img: screens::imagenes::ImgState::default(),
            comms: screens::comunicaciones::CommsState::default(),
            docs: screens::documentos::DocsState::default(),
            stock: screens::almacen::StockState::default(),
            gastos: screens::gastos::GastosState::default(),
            rx: screens::recetas::RxState::default(),
            ia: screens::ia::IaUi::default(),
            vf: screens::verifactu::VfUi::default(),
            mail_ui: screens::correo::MailUi::default(),
            booking: screens::cita_online::BookingUi::default(),
            marcha: screens::marcha::MarchaUi::default(),
            tasks: screens::tareas::TaskState::new(today()),
            chat: screens::avisos::ChatState::default(),
            phone: screens::llamadas::CallState::default(),
            wa: screens::whatsapp::WaUi::new(live),
            login: screens::login::LoginState::default(),
            user_id: None,
            second_factor: false,
            last_activity: 0.0,
            docs_mode: 0,
            imp: screens::importar::ImportState::default(),
            ortho: screens::ortodoncia::OrthoUi::default(),
            rem: screens::remesas::RemState2::default(),
            cobros_tab: 0,
            tools: screens::herramientas::ToolsState::default(),
            bank: screens::bancos::BankUi::default(),
        };
        // En modo captura se entra directamente como Recepción.
        if app.screenshot.is_some() {
            app.user_id = app.db.users.iter().find(|u| u.profile == crate::users::Profile::Recepcion).map(|u| u.id);
        }
        // el programa ha cambiado de nombre: se avisa de que sus datos siguen ahí
        if crate::brand::migrated() > 0 {
            app.notify(format!("El programa ahora se llama {}: sus datos, ajustes y documentos de {} están en las carpetas con el nombre nuevo.", crate::brand::NAME, crate::brand::OLD_DIR));
        }
        // copia de seguridad automática diaria
        if app.persist && crate::tools::needs_daily(app.today) {
            if let Err(e) = crate::tools::create_backup(&app.db, "automatica") {
                app.save_error = Some(e);
            }
        }
        let (_, skipped) = app.db.generate_messages(app.today);
        app.db.generate_tasks(app.today);
        // avisos de llamada de la centralita (no en capturas ni pruebas)
        if app.screenshot.is_none() {
            screens::llamadas::start(&mut app, &cc.egui_ctx);
            screens::whatsapp::start(&mut app, &cc.egui_ctx);
        }
        app.comms.last_skipped = skipped;
        if app.db.patient(app.patient.unwrap_or(0)).is_none() {
            app.patient = app.db.patients.first().map(|p| p.id);
        }
        app
    }

    /// Abre un diálogo al arrancar (útil para revisar pantallas): cita, cobro, paciente, presupuesto.
    pub fn open_dialog_by_name(&mut self, name: &str) {
        let pid = self.patient.unwrap_or(100);
        self.dialog = match name {
            "cita" => {
                let a = self.db.appointments.iter().find(|a| a.date == self.today && a.state == ApptState::SalaEspera).cloned();
                match a {
                    Some(a) => Dialog::Appointment(screens::agenda::ApptDraft::from(&a)),
                    None => Dialog::Appointment(screens::agenda::ApptDraft::new_at(self, self.today, None, Some(pid))),
                }
            }
            "factura" => match self.db.invoices.first().map(|i| i.id) {
                Some(id) => Dialog::Invoice(id),
                None => Dialog::None,
            },
            "avisos" => {
                let to = self.user_id.and_then(|u| self.db.user(u)).map(|u| u.name.clone()).unwrap_or_default();
                self.db.send_msg("Dra. Ruiz", &to, "Pase el siguiente paciente", None);
                self.db.send_msg("Dr. Navarro", "", "Necesito ayuda en el gabinete 2", Some(101));
                Dialog::None
            }
            "chat" => {
                self.chat.open = true;
                self.chat.to = "Dra. Ruiz".into();
                Dialog::None
            }
            "llamada" => {
                let n = self.db.patient(101).map(|p| p.mobile.clone()).unwrap_or_default();
                screens::llamadas::incoming(self, &format!("+34 {n}"));
                Dialog::None
            }
            "llamada-desconocida" => {
                screens::llamadas::incoming(self, "699 000 111");
                Dialog::None
            }
            "wa-pasarela" => {
                screens::config::open(self, screens::config::CfgTab::Whatsapp);
                self.wa.form.provider = crate::whatsapp::Provider::Pasarela;
                self.wa.scroll_here = true;
                Dialog::None
            }
            "wa-agenda" => {
                self.screen = Screen::Agenda;
                let mut d = self.today + chrono::Duration::days(1);
                while !self.db.working_day(d) {
                    d += chrono::Duration::days(1);
                }
                self.agenda.date = d;
                Dialog::None
            }
            "wa-log" => {
                self.screen = Screen::Comunicaciones;
                self.comms.tab = 3;
                Dialog::None
            }
            "tareas" => {
                self.tasks.open = true;
                Dialog::None
            }
            "festivos" => {
                self.agenda.blocks_open = true;
                Dialog::None
            }
            "repetir" => {
                let mut d = screens::agenda::ApptDraft::new_at(self, self.today, None, Some(pid));
                d.repeat = 1;
                Dialog::Appointment(d)
            }
            "cobro" => Dialog::Payment(screens::cobros::PayDraft::new(self, pid)),
            "paciente" => Dialog::NewPatient(Box::new(screens::pacientes::blank_patient(&self.db, self.today))),
            "presupuesto" => match self.db.budgets.iter().find(|b| b.patient_id == pid).map(|b| b.id) {
                Some(id) => Dialog::Budget(id),
                None => Dialog::None,
            },
            _ => Dialog::None,
        };
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), 3.5, ACCENT));
    }
    pub fn warn(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), 4.5, DANGER));
    }
    pub fn changed(&mut self) {
        self.dirty = true;
    }
    pub fn open_patient(&mut self, id: u32, tab: PatTab) {
        self.patient = Some(id);
        self.pat_tab = tab;
        self.screen = Screen::Pacientes;
        self.lists.patient_list_open = false;
    }

    /// En red: si este puesto no tiene cambios pendientes, recoge los de los demás.
    fn poll_shared(&mut self, time: f64) {
        if !self.persist || self.dirty || time - self.last_poll < 3.0 {
            return;
        }
        self.last_poll = time;
        if let Some(r) = self.remote.clone() {
            match r.rev() {
                Ok(rev) if rev != self.db.rev => match r.load() {
                    Ok((d, v)) => {
                        self.db = crate::server::adopt(&self.db, d);
                        self.base = v;
                        self.save_error = None;
                    }
                    Err(e) => self.save_error = Some(e),
                },
                Ok(_) => self.save_error = None,
                Err(e) => {
                    self.save_error = Some(e);
                    // sin conexión: no insistir cada 3 s para no frenar la pantalla
                    self.last_poll = time + 12.0;
                }
            }
            return;
        }
        if !crate::shared::shared() {
            return;
        }
        if let Some((db, v)) = crate::shared::reload_if_changed(&self.db, &Db::path()) {
            self.db = db;
            self.base = v;
        }
    }

    fn autosave(&mut self, time: f64) {
        if !self.persist || !self.dirty || time - self.last_save < 1.5 {
            return;
        }
        match self.save_now() {
            Ok(n) => {
                self.dirty = false;
                self.save_error = None;
                if n > 0 {
                    self.notify(format!("Se han combinado cambios de otro puesto ({n} datos tocados a la vez: se mantiene lo de este puesto)."));
                }
                self.last_save = time;
            }
            Err(e) => {
                self.save_error = Some(e);
                // reintento más espaciado si falla (p. ej. sin conexión)
                self.last_save = time + 8.0;
            }
        }
    }

    /// Guarda en el servidor, en la carpeta compartida o en este ordenador.
    /// Devuelve cuántos datos coincidieron con cambios de otro puesto.
    pub fn save_now(&mut self) -> Result<usize, String> {
        if let Some(r) = self.remote.clone() {
            let (d, v) = r.save(&self.db)?;
            self.db = d;
            self.base = v;
            return Ok(0);
        }
        match crate::shared::save_synced(&mut self.db, &mut self.base, &Db::path())? {
            crate::shared::Synced::Merged(n) => Ok(n),
            crate::shared::Synced::Saved => Ok(0),
        }
    }

    /// Reintenta la conexión con el servidor.
    pub fn reconnect(&mut self) {
        let Some(mut r) = self.remote.clone() else { return };
        match crate::server::connect(&mut r, &crate::shared::machine_name(), if crate::server::is_local(&crate::shared::station().server) { 12 } else { 1 }) {
            Ok((d, v)) => {
                self.db = crate::server::adopt(&self.db, d);
                self.db.station = r.station;
                self.base = v;
                self.remote = Some(r);
                self.persist = true;
                self.offline = None;
                self.save_error = None;
                self.notify("Conectado con el servidor de la clínica.");
            }
            Err(e) => self.offline = Some(e),
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(s) = &mut self.screenshot else { return };
        s.frames += 1;
        ctx.request_repaint();
        let st = s.state.load(Ordering::SeqCst);
        if st == 0 && s.frames >= 120 {
            s.state.store(1, Ordering::SeqCst);
            let path = s.path.clone();
            let flag = s.state.clone();
            ctx.request_screenshot(move |img| {
                let [w, h] = img.size;
                let mut buf = Vec::with_capacity(w * h * 4);
                for p in &img.pixels {
                    buf.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
                }
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let ok = image::save_buffer(&path, &buf, w as u32, h as u32, image::ColorType::Rgba8).is_ok();
                if !ok {
                    eprintln!("No se pudo guardar la captura en {}", path.display());
                }
                flag.store(if ok { 2 } else { 3 }, Ordering::SeqCst);
            });
        } else if st >= 2 || s.frames > 900 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        // grupos del menú (las pantallas sin permiso no se muestran)
        const GROUPS: [(&str, &[Screen]); 5] = [
            ("", &[Screen::Inicio, Screen::Agenda, Screen::Pacientes]),
            ("Clínica", &[Screen::Imagenes, Screen::Ortodoncia, Screen::Laboratorio, Screen::Recalls, Screen::Comunicaciones, Screen::Listin]),
            ("Economía", &[Screen::Presupuestos, Screen::Cobros, Screen::Facturas, Screen::Caja, Screen::Gastos, Screen::Bancos]),
            ("Gestión", &[Screen::Almacen, Screen::Tarifas, Screen::Colaboradores, Screen::Informes]),
            ("Sistema", &[Screen::Config, Screen::Modulos]),
        ];
        let soft = Color32::from_rgb(170, 186, 208);
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
            crate::screens::odontograma::draw_logo(ui.painter(), r);
            ui.add_space(2.0);
            ui.vertical(|ui| {
                ui.add_space(1.0);
                ui.label(RichText::new("VeodarDental").color(Color32::WHITE).font(theme::bf(18.5)));
                ui.add_space(-1.0);
                ui.label(RichText::new("Gestión de clínica dental").color(Color32::from_rgb(110, 214, 198)).font(theme::mdf(12.0)));
            });
        });
        ui.add_space(16.0);
        for (group, screens) in GROUPS {
            let visible: Vec<Screen> = screens.iter().copied().filter(|s| self.allowed(*s)).collect();
            if visible.is_empty() {
                continue;
            }
            if !group.is_empty() {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.label(RichText::new(group.to_uppercase()).font(theme::sbf(10.8)).color(Color32::from_rgb(98, 118, 146)));
                });
                ui.add_space(2.0);
            }
            for scr in visible {
                let name = Screen::NAV.iter().find(|n| n.0 == scr).map(|n| n.2).unwrap_or("");
                let sel = self.screen == scr;
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
                let r = rect.shrink2(vec2(12.0, 1.5));
                let p = ui.painter();
                if sel {
                    p.rect_filled(r, CornerRadius::same(10), Color32::from_rgba_unmultiplied(45, 212, 191, 30));
                    p.rect_stroke(r, CornerRadius::same(10), Stroke::new(1.0, Color32::from_rgba_unmultiplied(45, 212, 191, 46)), egui::StrokeKind::Inside);
                    p.rect_filled(egui::Rect::from_min_size(r.min + vec2(0.0, 9.0), vec2(3.0, r.height() - 18.0)), CornerRadius::same(2), Color32::from_rgb(45, 212, 191));
                } else if resp.hovered() {
                    p.rect_filled(r, CornerRadius::same(10), Color32::from_rgba_unmultiplied(255, 255, 255, 12));
                }
                let col = if sel { Color32::WHITE } else if resp.hovered() { Color32::from_rgb(222, 230, 241) } else { soft };
                crate::icons::nav_icon(p, scr, r.min + vec2(24.0, r.height() / 2.0), if sel { Color32::from_rgb(94, 234, 212) } else { col });
                p.text(r.min + vec2(46.0, r.height() / 2.0), Align2::LEFT_CENTER, name, if sel { theme::sbf(14.5) } else { theme::mdf(14.5) }, col);
                // contador
                let badge = match scr {
                    Screen::Recalls => self.db.recalls.iter().filter(|r| r.state == RecallState::Pendiente && r.date <= self.today).count(),
                    Screen::Presupuestos => self.db.budgets.iter().filter(|b| b.state == BudgetState::Pendiente).count(),
                    Screen::Agenda => self.db.appointments.iter().filter(|a| a.date == self.today && a.state == ApptState::SalaEspera).count(),
                    _ => 0,
                };
                let mut right = r.right_center().x - 10.0;
                if badge > 0 {
                    let txt = badge.to_string();
                    let bw = 12.0 + 7.5 * txt.len() as f32;
                    let br = egui::Rect::from_center_size(egui::pos2(right - bw.max(22.0) / 2.0, r.center().y), vec2(bw.max(22.0), 20.0));
                    let bc = if scr == Screen::Agenda { Color32::from_rgb(245, 158, 11) } else { Color32::from_rgb(20, 184, 166) };
                    p.rect_filled(br, CornerRadius::same(10), bc);
                    p.text(br.center(), Align2::CENTER_CENTER, txt, theme::sbf(11.5), Color32::WHITE);
                    right = br.min.x - 5.0;
                }
                // citas pedidas por internet por confirmar (en violeta)
                let online = if scr == Screen::Agenda { self.db.booking_pending() } else { 0 };
                if online > 0 {
                    let txt = online.to_string();
                    let bw = (12.0 + 7.5 * txt.len() as f32).max(22.0);
                    let br = egui::Rect::from_center_size(egui::pos2(right - bw / 2.0, r.center().y), vec2(bw, 20.0));
                    p.rect_filled(br, CornerRadius::same(10), Color32::from_rgb(139, 92, 246));
                    p.text(br.center(), Align2::CENTER_CENTER, txt, theme::sbf(11.5), Color32::WHITE);
                }
                if resp.clicked() {
                    self.screen = scr;
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        }
        // usuario en la parte de abajo
        ui.add_space(18.0);
        let user = self.user_id.and_then(|u| self.db.user(u)).cloned();
        if let Some(u) = user {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::hover());
            let r = rect.shrink2(vec2(12.0, 4.0));
            let p = ui.painter();
            p.rect_filled(r, CornerRadius::same(12), Color32::from_rgba_unmultiplied(255, 255, 255, 10));
            theme::avatar(p, r.left_center() + vec2(24.0, 0.0), 16.0, &u.name, Color32::from_rgb(94, 234, 212));
            p.text(r.left_center() + vec2(48.0, -9.0), Align2::LEFT_CENTER, &u.name, theme::sbf(13.5), Color32::WHITE);
            p.text(r.left_center() + vec2(48.0, 9.0), Align2::LEFT_CENTER, u.profile.label(), theme::mdf(11.8), soft);
        }
        ui.add_space(12.0);
    }

    fn topbar(&mut self, ui: &mut Ui) {
        // en ventanas estrechas el buscador y la ficha del paciente se compactan
        let total = ui.available_width();
        let tier = if total > 1120.0 { 0 } else if total > 1000.0 { 1 } else { 2 };
        ui.horizontal_centered(|ui| {
            ui.add_space(4.0);
            if self.db.multi_center() {
                let cur = self.db.config.current_center;
                let list: Vec<(u32, String)> = self.db.centers.iter().filter(|c| c.active).map(|c| (c.id, c.name.clone())).collect();
                let mut sel = crate::model::cid(cur);
                crate::theme::combo("center_sel").width(if tier == 2 { 130.0 } else { 160.0 }).selected_text(RichText::new(self.db.center_name(sel)).family(crate::theme::sb())).show_ui(ui, |ui| {
                    for (id, n) in &list {
                        ui.selectable_value(&mut sel, *id, n);
                    }
                });
                if sel != crate::model::cid(cur) {
                    self.db.config.current_center = sel;
                    self.changed();
                    self.notify(format!("Trabajando en {}", self.db.center_name(sel)));
                }
            } else {
                ui.label(RichText::new(&self.db.config.name).font(theme::sbf(16.0)).color(HEADING));
            }
            ui.add_space(if tier == 2 { 6.0 } else { 14.0 });
            // Búsqueda global de pacientes (píldora con lupa)
            let resp = Frame::new()
                .fill(Color32::from_rgb(241, 244, 248))
                .stroke(Stroke::new(1.0, Color32::from_rgb(229, 234, 241)))
                .corner_radius(CornerRadius::same(20))
                .inner_margin(Margin { left: 36, right: 12, top: 0, bottom: 0 })
                .show(ui, |ui| {
                    let sw = [230.0, 170.0, 96.0][tier];
                    let r = ui.add_sized([sw, 38.0], egui::TextEdit::singleline(&mut self.search).frame(Frame::NONE).align(egui::Align2::LEFT_CENTER).hint_text("Buscar paciente, Nº historial, NIF o teléfono"));
                    let c = r.rect.left_center() - vec2(20.0, 0.0);
                    ui.painter().circle_stroke(c - vec2(1.5, 1.5), 5.5, Stroke::new(1.8, SUBTLE));
                    ui.painter().line_segment([c + vec2(2.6, 2.6), c + vec2(6.5, 6.5)], Stroke::new(2.0, SUBTLE));
                    r
                })
                .inner;
            if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
                resp.request_focus();
            }
            if !self.search.trim().is_empty() {
                let results = screens::pacientes::search(&self.db, &self.search, 8);
                let pos = resp.rect.left_bottom() + vec2(0.0, 4.0);
                let mut chosen = None;
                egui::Area::new(egui::Id::unique("search_results")).fixed_pos(pos).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                    Frame::popup(ui.style()).inner_margin(Margin::same(6)).show(ui, |ui| {
                        ui.set_width(380.0);
                        if results.is_empty() {
                            muted(ui, "Sin resultados");
                        }
                        for pid in results {
                            if let Some(p) = self.db.patient(pid) {
                                let txt = format!("{}  ·  Nº {}  ·  {}", p.full_name(), p.history_no, p.mobile);
                                if ui.add_sized([370.0, 34.0], egui::Button::new(txt).frame(false)).clicked() {
                                    chosen = Some(pid);
                                }
                            }
                        }
                    });
                });
                if let Some(pid) = chosen {
                    self.search.clear();
                    self.open_patient(pid, PatTab::Resumen);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.search.clear();
                }
            }
            ui.add_space(if tier == 2 { 4.0 } else { 8.0 });
            if let Some(pid) = self.patient {
                if let Some(p) = self.db.patient(pid).cloned() {
                    let alert = !p.alerts.is_empty() || !p.allergies.is_empty();
                    let short: String = format!("{} {}", p.first_name, p.last_name.split_whitespace().next().unwrap_or("")).chars().take([20, 12, 0][tier]).collect();
                    let txt = if tier == 2 { format!("      {}", if alert { "⚠" } else { "" }) } else { format!("        {}{}", short.trim(), if alert { "  ⚠" } else { "" }) };
                    let col = if alert { DANGER } else { ACCENT };
                    let b = egui::Button::new(RichText::new(txt).font(theme::mdf(14.0)).color(if alert { DANGER } else { HEADING }))
                        .fill(tint(col, 0.09))
                        .stroke(Stroke::new(1.0, tint(col, 0.25)))
                        .corner_radius(CornerRadius::same(19))
                        .min_size(vec2(0.0, 38.0));
                    let r = ui.add(b);
                    theme::avatar(ui.painter(), r.rect.left_center() + vec2(19.0, 0.0), 13.0, &p.full_name(), col);
                    let r = if alert { r.on_hover_text(format!("{}\nAlertas: {} {}", p.full_name(), p.alerts, p.allergies)) } else { r.on_hover_text(format!("Paciente activo: {}", p.full_name())) };
                    if r.clicked() {
                        self.open_patient(pid, self.pat_tab);
                    }
                }
            }
            theme::right_aligned(ui, |ui| {
                ui.add_space(8.0);
                if theme::primary(ui, "＋ Nueva cita").clicked() {
                    let d = screens::agenda::ApptDraft::new_at(self, self.today, None, None);
                    self.dialog = Dialog::Appointment(d);
                }
                if theme::secondary(ui, if tier == 2 { "＋" } else { "＋ Paciente" }).on_hover_text("Nuevo paciente").clicked() {
                    self.dialog = Dialog::NewPatient(Box::new(screens::pacientes::blank_patient(&self.db, self.today)));
                }
                ui.add_space(4.0);
                screens::tareas::bell(self, ui);
                screens::avisos::button(self, ui);
                let uname = self.user_id.and_then(|u| self.db.user(u)).map(|u| u.name.clone()).unwrap_or_default();
                let warn = self.user_id.and_then(|u| self.db.user(u)).map(|u| u.must_change).unwrap_or(false);
                // el usuario va como iniciales (su nombre completo está en el menú lateral)
                let initials: String = uname.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase();
                ui.menu_button(RichText::new(format!("{}{}", if warn { "⚠ " } else { "" }, initials)).font(theme::sbf(13.5)).color(if warn { WARN } else { ACCENT_DARK }), |ui| {
                    if ui.button("🔑 Cambiar contraseña").clicked() {
                        self.login.show_change = true;
                        ui.close();
                    }
                    if ui.button("🔒 Bloquear pantalla").clicked() {
                        self.lock("Pantalla bloqueada");
                        ui.close();
                    }
                });
            });
        });
    }

    pub fn profile(&self) -> crate::users::Profile {
        self.user_id.and_then(|u| self.db.user(u)).map(|u| u.profile).unwrap_or(crate::users::Profile::Recepcion)
    }

    pub fn can(&self, p: crate::users::Perm) -> bool {
        self.profile().can(p)
    }

    /// Pantallas permitidas según el perfil del usuario.
    pub fn allowed(&self, s: Screen) -> bool {
        use crate::users::Perm::*;
        match s {
            Screen::Caja => self.can(Caja),
            Screen::Facturas => self.can(Facturacion),
            Screen::Cobros | Screen::Presupuestos => self.can(Economia),
            Screen::Informes => self.can(Informes),
            Screen::Gastos | Screen::Bancos => self.can(Gastos),
            Screen::Config | Screen::Colaboradores | Screen::Tarifas => self.can(Configuracion),
            Screen::Almacen => self.can(Almacen),
            _ => true,
        }
    }

    fn lock(&mut self, reason: &str) {
        self.second_factor = false;
        if let Some(u) = self.user_id.take() {
            self.db.logout(u, reason);
            self.changed();
        }
        self.dialog = Dialog::None;
        self.login.pin.clear();
    }

    fn statusbar(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Usuario: {}", self.db.config.current_user)).size(12.5).color(MUTED));
            ui.separator();
            let cname = if self.db.multi_center() { self.db.center_name(self.db.config.current_center) } else { self.db.config.name.clone() };
            ui.label(RichText::new(format!("Centro: {cname}")).size(12.5).color(MUTED));
            ui.separator();
            match self.db.open_cash() {
                Some(c) => {
                    let exp = self.db.cash_expected(c);
                    ui.label(RichText::new(format!("● Caja abierta · efectivo esperado {}", money(exp))).size(12.5).color(OK));
                }
                None => {
                    ui.label(RichText::new("● Caja cerrada").size(12.5).color(DANGER));
                }
            }
            ui.separator();
            let waiting = self.db.appointments.iter().filter(|a| a.date == self.today && crate::model::cid(a.center_id) == crate::model::cid(self.db.config.current_center) && a.state == ApptState::SalaEspera).count();
            ui.label(RichText::new(format!("Sala de espera: {waiting}")).size(12.5).color(if waiting > 0 { WARN } else { MUTED }));
            theme::right_aligned(ui, |ui| {
                ui.add_space(6.0);
                if !self.persist {
                    ui.label(RichText::new("Modo demostración (sin guardar)").size(12.5).color(MUTED));
                } else if let Some(e) = &self.save_error {
                    ui.label(RichText::new(format!("⚠ {e}")).size(12.5).color(DANGER));
                } else if self.dirty {
                    ui.label(RichText::new("Guardando…").size(12.5).color(MUTED));
                } else {
                    ui.label(RichText::new("✔ Datos guardados").size(12.5).color(OK))
                        .on_hover_text(Db::path().display().to_string());
                }
                if self.persist && self.remote.is_some() {
                    ui.separator();
                    ui.label(RichText::new(format!("🖧 Servidor de la clínica · puesto {}", self.db.station)).size(12.5).color(INFO))
                        .on_hover_text(crate::shared::station().server.clone());
                } else if self.persist && crate::shared::shared() {
                    ui.separator();
                    ui.label(RichText::new(format!("🖧 En red · {}", crate::shared::station_label())).size(12.5).color(INFO));
                }
            });
        });
    }

    fn toast(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.stable_dt).min(0.1) as f64;
        if let Some((text, left, color)) = &mut self.toast {
            *left -= dt;
            let rect = ctx.content_rect();
            egui::Area::new(egui::Id::unique("toast"))
                .fixed_pos(rect.center_bottom() - vec2(180.0, 70.0))
                .order(egui::Order::Tooltip)
                .show(ctx, |ui| {
                    // aviso oscuro con una marca del color del mensaje
                    let accent = *color;
                    Frame::new()
                        .fill(Color32::from_rgb(17, 27, 44))
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(Margin { left: 40, right: 20, top: 13, bottom: 13 })
                        .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_rgba_unmultiplied(10, 20, 40, 60) })
                        .show(ui, |ui| {
                            ui.set_min_width(330.0);
                            ui.set_max_width(560.0);
                            let r = ui.label(RichText::new(text.as_str()).font(theme::mdf(14.0)).color(Color32::from_rgb(236, 241, 247)));
                            let c = egui::pos2(r.rect.min.x - 20.0, r.rect.center().y);
                            ui.painter().circle_filled(c, 9.0, mix(accent, Color32::WHITE, 0.15));
                            ui.painter().circle_filled(c, 3.5, Color32::WHITE);
                        });
                });
            ctx.request_repaint();
            if *left <= 0.0 {
                self.toast = None;
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let time = ctx.input(|i| i.time);
        if ctx.input(|i| !i.events.is_empty() || i.pointer.is_moving()) {
            self.last_activity = time;
        }
        if self.user_id.is_some() && self.screenshot.is_none() && time - self.last_activity > 15.0 * 60.0 {
            self.lock("Bloqueo automático por inactividad");
        }
        if self.offline.is_some() {
            egui::CentralPanel::default().frame(Frame::new().fill(SIDEBAR)).show(ui, |ui| screens::puestos::offline_screen(self, ui));
            self.toast(&ctx);
            self.handle_screenshot(&ctx);
            return;
        }
        if self.user_id.is_none() {
            egui::CentralPanel::default().frame(Frame::new().fill(SIDEBAR)).show(ui, |ui| {
                // fondo con degradado y dos brillos suaves de color
                let r = ui.max_rect();
                theme::vgradient(ui.painter(), r, Color32::from_rgb(17, 38, 66), Color32::from_rgb(8, 17, 31));
                let p = ui.painter();
                theme::radial_glow(p, r.left_top() + vec2(r.width() * 0.16, r.height() * 0.12), r.width() * 0.42, Color32::from_rgb(20, 184, 166), 0.16);
                theme::radial_glow(p, r.right_bottom() - vec2(r.width() * 0.1, r.height() * 0.08), r.width() * 0.34, Color32::from_rgb(59, 130, 246), 0.12);
                p.text(r.center_bottom() - vec2(0.0, 26.0), Align2::CENTER_CENTER, "VeodarDental · gestión de clínica dental", theme::mdf(12.5), Color32::from_rgb(110, 132, 160));
                screens::login::login_screen(self, ui)
            });
            // con la pantalla bloqueada la llamada se apunta, pero no se muestra
            screens::llamadas::poll(self, &ctx);
            // y los WhatsApp automáticos siguen funcionando
            screens::whatsapp::tick(self, time);
            screens::cita_online::tick(self, &ctx, time);
            // y el envío automático de comunicaciones por correo
            screens::comunicaciones::outbox_tick(self, &ctx);
            self.toast(&ctx);
            self.autosave(time);
            if self.persist && self.dirty && ctx.input(|i| i.viewport().close_requested()) {
                let _ = self.save_now();
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            self.handle_screenshot(&ctx);
            return;
        }
        if !self.allowed(self.screen) {
            self.screen = Screen::Inicio;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(30));
        if self.screen == Screen::Pizarra && self.tools.board_full {
            egui::CentralPanel::default().frame(Frame::new().fill(egui::Color32::from_rgb(12, 26, 44)).inner_margin(Margin::same(28))).show(ui, |ui| screens::herramientas::pizarra(self, ui));
            screens::whatsapp::tick(self, time);
            screens::cita_online::tick(self, &ctx, time);
            screens::comunicaciones::outbox_tick(self, &ctx);
            self.autosave(time);
            self.handle_screenshot(&ctx);
            return;
        }
        egui::Panel::left("nav")
            .exact_size(258.0)
            .resizable(false)
            .frame(Frame::new().fill(SIDEBAR))
            .show(ui, |ui| {
                let r = ui.max_rect();
                theme::vgradient(ui.painter(), r, SIDEBAR_TOP, SIDEBAR);
                egui::ScrollArea::vertical().id_salt("nav_scroll").show(ui, |ui| self.sidebar(ui));
            });
        egui::Panel::top("top")
            .exact_size(66.0)
            .frame(Frame::new().fill(CARD).stroke(Stroke::new(1.0, BORDER)).inner_margin(Margin::symmetric(20, 0)).shadow(egui::Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_rgba_unmultiplied(15, 30, 60, 8) }))
            .show(ui, |ui| self.topbar(ui));
        egui::Panel::bottom("status")
            .exact_size(32.0)
            .frame(Frame::new().fill(Color32::from_rgb(250, 251, 253)).stroke(Stroke::new(1.0, BORDER)).inner_margin(Margin::symmetric(14, 0)))
            .show(ui, |ui| self.statusbar(ui));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(26, 22)))
            .show(ui, |ui| match self.screen {
                Screen::Inicio | Screen::Caja | Screen::Tarifas | Screen::Colaboradores | Screen::Modulos | Screen::Imagenes | Screen::Almacen | Screen::Laboratorio | Screen::Gastos | Screen::Ortodoncia | Screen::Listin | Screen::Bancos => {
                    let mut sa = egui::ScrollArea::vertical().id_salt(("page", self.screen as u8)).auto_shrink([false, false]);
                    if let Some(y) = self.lists.scroll_to.take() {
                        sa = sa.vertical_scroll_offset(y);
                    }
                    sa.show(ui, |ui| match self.screen {
                        Screen::Inicio => screens::inicio::show(self, ui),
                        Screen::Caja => screens::caja::show(self, ui),
                        Screen::Tarifas => screens::tarifas::show(self, ui),
                        Screen::Colaboradores => screens::config::collaborators(self, ui),
                        Screen::Imagenes => screens::imagenes::screen(self, ui),
                        Screen::Almacen => screens::almacen::screen(self, ui),
                        Screen::Laboratorio => screens::laboratorio::screen(self, ui),
                        Screen::Gastos => screens::gastos::screen(self, ui),
                        Screen::Ortodoncia => screens::ortodoncia::screen(self, ui),
                        Screen::Listin => screens::herramientas::listin(self, ui),
                        Screen::Bancos => screens::bancos::screen(self, ui),
                        _ => screens::config::modules(self, ui),
                    });
                }
                // Configuración: menú de apartados fijo y el apartado con su propio desplazamiento
                Screen::Config => screens::config::show(self, ui),
                Screen::Pacientes => screens::pacientes::show(self, ui),
                Screen::Agenda => screens::agenda::show(self, ui),
                Screen::Presupuestos => screens::presupuestos::show(self, ui),
                Screen::Cobros => screens::cobros::show(self, ui),
                Screen::Facturas => screens::facturas::show(self, ui),
                Screen::Recalls => screens::recalls::show(self, ui),
                Screen::Comunicaciones => screens::comunicaciones::screen(self, ui),
                Screen::Pizarra => screens::herramientas::pizarra(self, ui),
                Screen::Informes => screens::informes::show(self, ui),
            });
        screens::dialogs(self, &ctx);
        screens::avisos::window(self, &ctx);
        screens::avisos::popups(self, &ctx);
        screens::llamadas::poll(self, &ctx);
        screens::llamadas::popups(self, &ctx);
        // los análisis con IA terminan aunque se haya cambiado de pantalla
        screens::ia::poll(self);
        // envíos por correo (órdenes de laboratorio y correo de prueba)
        screens::laboratorio::poll(self, &ctx);
        screens::correo::poll(self, &ctx);
        screens::recetas::poll(self, &ctx);
        // VERI*FACTU: recoge los envíos y, si está activado, envía los pendientes
        screens::verifactu::tick(self, &ctx);
        screens::whatsapp::tick(self, time);
        // cita online: solicitudes que llegan y huecos libres de la página
        screens::cita_online::tick(self, &ctx, time);
        // correos que se están enviando y envío automático del día
        screens::comunicaciones::outbox_tick(self, &ctx);
        screens::login::change_pin_dialog(self, &ctx);
        self.toast(&ctx);
        self.poll_shared(time);
        self.autosave(time);
        if (crate::shared::shared() || self.remote.is_some()) && self.persist {
            ctx.request_repaint_after(std::time::Duration::from_secs(3));
        }
        if self.dirty && self.persist {
            ctx.request_repaint_after(std::time::Duration::from_millis(1600));
        }
        if self.persist && self.dirty && ctx.input(|i| i.viewport().close_requested()) {
            let _ = self.save_now();
            self.dirty = false;
        }
        self.handle_screenshot(&ctx);
    }
}
