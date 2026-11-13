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
    /// Cómo van las copias de seguridad de este ordenador (en red, las del servidor).
    pub backup: crate::tools::BackupStatus,
    /// Últimos datos leídos o guardados (para combinar con otros puestos).
    pub base: serde_json::Value,
    /// En red: cómo están los datos en el servidor (lo último recibido o
    /// enviado), registro a registro, para enviar solo lo que cambie.
    pub synced: crate::sync::Snapshot,
    /// En red: para qué perfil son los datos recibidos (si entra otro, se recargan).
    pub loaded_view: Option<crate::vista::View>,
    /// Un solo ordenador: los datos abiertos para guardar (con su diario).
    pub local: Option<crate::local::LocalStore>,
    pub last_poll: f64,
    /// Conexión con el servidor de la clínica (puesto cliente).
    pub remote: Option<crate::server::Remote>,
    /// Sin conexión con el servidor: se muestra la pantalla de reintento.
    pub offline: Option<String>,
    /// Puesto conectado al servidor que aún no tiene los datos: solo conoce la
    /// clínica y sus usuarios hasta que alguien entra (el servidor da los datos
    /// con una sesión).
    pub awaiting_data: bool,
    /// Los servicios del arranque (avisos, WhatsApp, copia diaria…) esperan a
    /// tener los datos.
    pub pending_start: bool,
    pub screenshot: Option<Screenshot>,
    /// Cita de primera visita cuya ficha se está dando de alta (al guardarla,
    /// se le pone a esa cita y a las demás de esa persona).
    pub new_patient_for: Option<u32>,
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
        // un solo ordenador: los datos se bloquean para esta ventana (otra
        // ventana, o el servidor de datos, los abre sin guardar)
        let local_mode = persist && !crate::shared::shared() && !crate::shared::remote_mode();
        let lock = local_mode.then(|| crate::local::lock(&Db::path()));
        // aviso si el servidor tiene otra zona horaria
        let mut clock: Option<String> = None;
        let (mut db, mut err) = match (&lock, persist) {
            (_, false) => (Db::sample(today()), None),
            (Some(Err(_)), true) => Db::load_from(&Db::path(), false),
            _ => Db::load_or_sample(),
        };
        let mut local = None;
        match lock {
            // si no se han podido leer los datos, no se guarda encima de ellos
            Some(Ok(_)) if err.is_some() => persist = false,
            Some(Ok(l)) => match crate::local::LocalStore::new(Db::path(), l, &mut db) {
                Ok(s) => local = Some(s),
                Err(e) => {
                    err = Some(format!("No se pueden guardar los datos: {e}"));
                    persist = false;
                }
            },
            Some(Err(e)) => {
                err = Some(e);
                persist = false;
            }
            None => {}
        }
        if persist && crate::shared::shared() {
            if !Db::path().exists() {
                err = Some(format!("No se encuentra la carpeta compartida {}: se muestran datos de ejemplo sin guardar.", crate::shared::station().data_dir));
                persist = false;
            } else if err.is_some() {
                persist = false;
            }
            db.station = crate::shared::station().number;
        }
        // para combinar con otros puestos en la carpeta compartida
        let mut base = if crate::shared::shared() { serde_json::to_value(&db).unwrap_or_default() } else { serde_json::Value::Null };
        let mut remote = None;
        let mut offline = None;
        let mut awaiting_data = false;
        if persist && crate::shared::remote_mode() {
            let st = crate::shared::station();
            let local = crate::server::is_local(&st.server);
            if local && !crate::server::local_server_running() {
                let _ = crate::server::spawn_local_server();
            }
            // solo acepta el servidor cuya huella apuntó al darse de alta
            let mut r = crate::server::Remote::with_pin(&st.server, &st.key, 0, &st.server_pin);
            // hasta que alguien entre, el puesto no tiene los datos (ni los de este ordenador)
            persist = false;
            match crate::server::connect(&mut r, &crate::shared::machine_name(), if local { 20 } else { 1 }) {
                Ok(info) => {
                    clock = crate::server::clock_warning(info.utc_offset, chrono::Local::now().offset().local_minus_utc());
                    // puesto dado de alta con una versión anterior: apunta ahora la huella,
                    // ya comprobada (el servidor ha demostrado saber la clave de la clínica)
                    if st.server_pin.is_empty() && !r.pinned().is_empty() {
                        let _ = crate::shared::save_station(&crate::shared::Station { server_pin: r.pinned(), ..st.clone() });
                    }
                    db = login_shell(&info, &db.config.current_user, today());
                    db.station = r.station;
                    base = serde_json::Value::Null;
                    awaiting_data = true;
                    err = None;
                }
                Err(e) => offline = Some(e),
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
            backup: crate::tools::BackupStatus::from_dir(&crate::tools::backup_dir()),
            base,
            synced: crate::sync::Snapshot::default(),
            loaded_view: None,
            local,
            last_poll: 0.0,
            remote,
            offline,
            awaiting_data,
            pending_start: false,
            screenshot: shot.map(|path| Screenshot { path, frames: 0, state: Arc::new(AtomicU8::new(0)) }),
            new_patient_for: None,
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
        if let Some(w) = clock {
            app.toast = Some((w, 15.0, crate::theme::WARN));
        }
        // En modo captura se entra directamente como Recepción.
        if app.screenshot.is_some() {
            app.user_id = app.db.users.iter().find(|u| u.profile == crate::users::Profile::Recepcion).map(|u| u.id);
        }
        // el programa ha cambiado de nombre: se avisa de que sus datos siguen ahí
        if crate::brand::migrated() > 0 {
            app.notify(format!("El programa ahora se llama {}: sus datos, ajustes y documentos de {} están en las carpetas con el nombre nuevo.", crate::brand::NAME, crate::brand::OLD_DIR));
        }
        if app.awaiting_data || app.offline.is_some() {
            app.pending_start = true;
        } else {
            app.start_services(&cc.egui_ctx);
        }
        app
    }

    /// Lo que se hace al tener los datos: copia diaria, avisos y tareas del
    /// día, y los servicios de la centralita y de WhatsApp.
    fn start_services(&mut self, ctx: &egui::Context) {
        // copia de seguridad automática diaria
        if self.persist && crate::tools::needs_daily(self.today) {
            let done = crate::tools::create_backup(&self.db, "automatica");
            self.backup.record(&done, crate::model::now());
            if let Err(e) = done {
                self.save_error = Some(e);
            }
        }
        let (_, skipped) = self.db.generate_messages(self.today);
        self.db.generate_tasks(self.today);
        // avisos de llamada de la centralita (no en capturas ni pruebas)
        if self.screenshot.is_none() {
            screens::llamadas::start(self, ctx);
            screens::whatsapp::start(self, ctx);
        }
        self.comms.last_skipped = skipped;
        if self.db.patient(self.patient.unwrap_or(0)).is_none() {
            self.patient = self.db.patients.first().map(|p| p.id);
        }
    }

    /// ¿Están los datos de la clínica? (En red, después de entrar.)
    pub fn data_ready(&self) -> bool {
        !self.awaiting_data && self.offline.is_none()
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

    /// Aviso de las copias de seguridad para Dirección (en red, las del servidor;
    /// cada puesto con los datos en un servidor no hace las suyas de verdad).
    pub fn backup_warning(&self) -> Option<String> {
        if !self.persist || !self.can(crate::users::Perm::Configuracion) {
            return None;
        }
        let status = match &self.remote {
            Some(r) => r.backup.lock().ok().and_then(|b| b.clone())?,
            None => self.backup.clone(),
        };
        status.warning(crate::model::now()).map(|w| if self.remote.is_some() { format!("Servidor: {w}") } else { w })
    }

    /// En red: si este puesto no tiene cambios pendientes, recoge los de los demás.
    fn poll_shared(&mut self, time: f64) {
        if !self.persist || self.dirty || time - self.last_poll < 3.0 || self.session_missing() {
            return;
        }
        self.last_poll = time;
        if let Some(r) = self.remote.clone() {
            // solo lo que ha cambiado desde la última vez (casi siempre, nada)
            match r.pull(self.db.rev) {
                Ok(d) if d.full || !d.changes.is_empty() || d.rev != self.db.rev => match r.receive(&mut self.db, &mut self.synced, d) {
                    Ok(()) => self.save_error = None,
                    Err(e) => self.remote_error(e),
                },
                Ok(_) => self.save_error = None,
                Err(e) => {
                    self.remote_error(e);
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
        if !self.persist || !self.dirty || time - self.last_save < 1.5 || self.session_missing() {
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
                self.remote_error(e);
                // reintento más espaciado si falla (p. ej. sin conexión)
                self.last_save = time + 8.0;
            }
        }
    }

    /// En red, sin sesión con el servidor (hasta volver a entrar no se guarda ni se recoge nada).
    fn session_missing(&self) -> bool {
        self.remote.as_ref().is_some_and(|r| r.session.is_empty())
    }

    /// Error al hablar con el servidor. Si la sesión ya no vale, se vuelve a la
    /// pantalla de entrada; los cambios pendientes se envían al entrar.
    fn remote_error(&mut self, e: String) {
        if e == crate::server::SESSION_EXPIRED {
            if let Some(r) = self.remote.as_mut() {
                r.session.clear();
            }
            self.user_id = None;
            self.second_factor = false;
            self.dialog = Dialog::None;
            self.login.pending_code = None;
            self.login.error = e.clone();
        }
        self.save_error = Some(e);
    }

    /// Al cerrar la ventana: se guarda lo pendiente y, con los datos en este
    /// ordenador, el fichero queda completo.
    fn on_close(&mut self) {
        if self.persist && self.dirty && self.save_now().is_ok() {
            self.dirty = false;
        }
        if let Some(st) = self.local.as_mut() {
            let _ = st.close(&self.db);
        }
    }

    /// Antes de que el servidor de datos de este ordenador abra los datos: los
    /// deja guardados y completos y los suelta (esta ventana deja de guardar).
    pub fn release_local(&mut self) {
        if let Some(mut st) = self.local.take() {
            if self.dirty && st.save(&mut self.db).is_ok() {
                self.dirty = false;
            }
            let _ = st.close(&self.db);
            self.persist = false;
        }
    }

    /// Vuelve a abrir los datos de este ordenador para guardar (si el servidor no arrancó).
    pub fn reopen_local(&mut self) {
        if self.local.is_some() || crate::shared::shared() || crate::shared::remote_mode() {
            return;
        }
        match crate::local::lock(&Db::path()).and_then(|l| crate::local::LocalStore::new(Db::path(), l, &mut self.db)) {
            Ok(st) => {
                self.local = Some(st);
                self.persist = true;
            }
            Err(e) => self.save_error = Some(e),
        }
    }

    /// Una operación de la cadena de facturas (emitir, rectificar, subsanar,
    /// anular): en red la hace el servidor y con carpeta compartida se hace con
    /// el fichero bloqueado, para que dos puestos no saquen el mismo número ni
    /// rompan la cadena de VERI*FACTU. Devuelve el id creado.
    pub fn chain_op(&mut self, op: ChainOp) -> Result<u32, String> {
        let user = self.db.config.current_user.clone();
        let admin = self.can(crate::users::Perm::Configuracion);
        if let Some(r) = self.remote.clone() {
            // con el centro de este puesto (para la caja de los cobros)
            let mut v = op.json();
            v["centro"] = self.db.config.current_center.into();
            let id = r.chain_op(&mut self.db, &mut self.synced, &v)?;
            self.dirty = false;
            return Ok(id);
        }
        if crate::shared::shared() {
            let (_, id) = crate::shared::save_synced_with(&mut self.db, &mut self.base, &Db::path(), |db| op.run(db, &user, admin))?;
            self.dirty = false;
            return id;
        }
        let id = op.run(&mut self.db, &user, admin)?;
        self.changed();
        Ok(id)
    }

    /// Comprueba la contraseña: en el servidor de la clínica o en los datos de este ordenador.
    pub fn check_login(&mut self, id: u32, pin: &str) -> Result<crate::users::LoginStep, String> {
        let Some(r) = self.remote.clone() else { return self.db.login(id, pin) };
        match r.login(id, pin) {
            Ok(crate::server::Entry::Inside(session)) => {
                self.begin_session(id, session)?;
                Ok(crate::users::LoginStep::Done)
            }
            Ok(crate::server::Entry::NeedCode(previous)) => {
                self.login.previous = previous;
                Ok(crate::users::LoginStep::NeedCode)
            }
            Err(e) => {
                self.refresh_users();
                Err(e)
            }
        }
    }

    /// Segundo paso de la entrada: el código de la app del móvil.
    pub fn check_login_code(&mut self, id: u32, code: &str) -> Result<(), String> {
        let Some(r) = self.remote.clone() else { return self.db.login_code(id, code, crate::totp::unix_now()) };
        match r.login_code(id, code, &self.login.previous) {
            Ok(session) => {
                self.login.previous.clear();
                self.begin_session(id, session)
            }
            Err(e) => {
                self.refresh_users();
                Err(e)
            }
        }
    }

    /// Sesión nueva con el servidor: recoge sus datos o, si este puesto tenía
    /// cambios sin enviar (la sesión anterior caducó), los envía y combina.
    fn begin_session(&mut self, id: u32, session: String) -> Result<(), String> {
        let Some(r) = self.remote.as_mut() else { return Err("Sin servidor".into()) };
        r.session = session;
        let r = r.clone();
        let view = self.db.user(id).map(|u| crate::vista::View::of(u.profile)).unwrap_or(crate::vista::View::FULL);
        if self.awaiting_data || self.loaded_view != Some(view) {
            // primera vez, o entra un perfil que ve otras cosas: los datos que le tocan
            if !self.awaiting_data && self.dirty {
                r.push(&mut self.db, &mut self.synced)?;
            }
            let (d, synced) = r.load()?;
            self.db = crate::server::adopt(&self.db, d);
            self.synced = synced;
            self.loaded_view = Some(view);
        } else if self.dirty {
            r.push(&mut self.db, &mut self.synced)?;
        } else {
            // al volver a entrar, solo lo nuevo
            let d = r.pull(self.db.rev)?;
            r.receive(&mut self.db, &mut self.synced, d)?;
        }
        self.dirty = false;
        self.awaiting_data = false;
        self.persist = true;
        self.save_error = None;
        if let Some(name) = self.db.user(id).map(|u| u.name.clone()) {
            self.db.config.current_user = name;
        }
        Ok(())
    }

    /// Antes de tener los datos: vuelve a pedir los usuarios (por si alguno ha quedado bloqueado).
    fn refresh_users(&mut self) {
        if !self.awaiting_data {
            return;
        }
        if let Some(info) = self.remote.as_ref().and_then(|r| r.access().ok()) {
            *self.db.users = info.users;
        }
    }

    /// Tras cambiar algo en el servidor (contraseña, código del móvil): envía
    /// los cambios pendientes o recoge los datos nuevos.
    fn sync_with_server(&mut self) {
        let Some(r) = self.remote.clone() else { return };
        let res = if self.dirty { self.save_now().map(|_| ()) } else { r.pull(self.db.rev).and_then(|d| r.receive(&mut self.db, &mut self.synced, d)) };
        match res {
            Ok(()) => {
                self.dirty = false;
                self.save_error = None;
            }
            Err(e) => self.remote_error(e),
        }
    }

    /// Contraseña nueva de un usuario. Con `current`, la propia (comprobando la
    /// actual); sin ella, la inicial propia o, para Dirección, la de otro.
    /// `change_at_login`: el usuario tendrá que cambiarla al entrar.
    pub fn set_user_pin(&mut self, id: u32, current: Option<&str>, new: &str, change_at_login: bool) -> Result<(), String> {
        if let Some(r) = self.remote.clone() {
            r.set_pin(id, current, new, change_at_login)?;
            self.sync_with_server();
            return Ok(());
        }
        match current {
            Some(old) => self.db.change_pin(id, old, new)?,
            None => self.db.set_pin(id, new)?,
        }
        if let Some(u) = self.db.users.iter_mut().find(|u| u.id == id) {
            u.must_change = change_at_login;
        }
        self.changed();
        Ok(())
    }

    /// Activa el código del móvil de un usuario (comprobando un código de la app).
    pub fn enable_user_totp(&mut self, id: u32, secret: &str, code: &str) -> Result<(), String> {
        if let Some(r) = self.remote.clone() {
            r.totp_on(id, secret, code)?;
            self.sync_with_server();
            return Ok(());
        }
        self.db.enable_totp(id, secret, code, crate::totp::unix_now())?;
        self.changed();
        Ok(())
    }

    /// Quita el código del móvil de un usuario.
    pub fn disable_user_totp(&mut self, id: u32) -> Result<(), String> {
        if let Some(r) = self.remote.clone() {
            r.totp_off(id)?;
            self.sync_with_server();
            return Ok(());
        }
        let by = self.db.config.current_user.clone();
        self.db.disable_totp(id, &by);
        self.changed();
        Ok(())
    }

    /// Guarda en el servidor, en la carpeta compartida o en este ordenador.
    /// Devuelve cuántos datos coincidieron con cambios de otro puesto.
    pub fn save_now(&mut self) -> Result<usize, String> {
        if let Some(r) = self.remote.clone() {
            // solo lo que ha cambiado (si nada, no se envía nada)
            r.push(&mut self.db, &mut self.synced)?;
            return Ok(0);
        }
        // un solo ordenador: solo lo que ha cambiado, en el diario
        if let Some(st) = self.local.as_mut() {
            return st.save(&mut self.db).map(|_| 0);
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
            Ok(info) => {
                // los datos llegan al entrar con un usuario
                self.db = login_shell(&info, &self.db.config.current_user, self.today);
                self.db.station = r.station;
                self.base = serde_json::Value::Null;
                self.synced = crate::sync::Snapshot::default();
                self.loaded_view = None;
                self.remote = Some(r);
                self.awaiting_data = true;
                self.persist = false;
                self.offline = None;
                self.save_error = None;
                self.user_id = None;
                match crate::server::clock_warning(info.utc_offset, chrono::Local::now().offset().local_minus_utc()) {
                    Some(w) => self.toast = Some((w, 15.0, crate::theme::WARN)),
                    None => self.notify("Conectado con el servidor de la clínica."),
                }
            }
            Err(e) => self.offline = Some(e),
        }
    }

    /// El servidor tiene otra huella y quien está en el puesto ha comprobado
    /// que es la que muestra el servidor: se apunta la nueva y se reconecta.
    pub fn trust_server(&mut self, pin: &str) {
        let Some(old) = self.remote.clone() else { return };
        // el servidor nuevo también tiene que demostrar que sabe la clave de la clínica
        let fresh = crate::server::Remote::new(old.address(), old.key(), 0);
        if let Err(e) = fresh.access() {
            self.offline = Some(e);
            return;
        }
        if fresh.pinned() != pin {
            self.warn("La huella del servidor ha vuelto a cambiar: compruébela otra vez.");
            return;
        }
        let st = crate::shared::Station { server_pin: pin.to_string(), ..crate::shared::station().clone() };
        if let Err(e) = crate::shared::save_station(&st) {
            self.warn(format!("No se pudo guardar el ajuste: {e}"));
            return;
        }
        self.remote = Some(crate::server::Remote::with_pin(old.address(), old.key(), 0, pin));
        self.reconnect();
    }

    /// Para las pruebas: conecta este puesto con un servidor (queda en la
    /// pantalla de entrada, sin datos hasta entrar).
    #[cfg(test)]
    pub fn connect_to(&mut self, mut r: crate::server::Remote) -> Result<(), String> {
        let info = crate::server::connect(&mut r, "Puesto de prueba", 1)?;
        self.db = login_shell(&info, "", self.today);
        self.db.station = r.station;
        self.base = serde_json::Value::Null;
        self.synced = crate::sync::Snapshot::default();
        self.loaded_view = None;
        self.remote = Some(r);
        self.awaiting_data = true;
        self.pending_start = true;
        self.persist = false;
        self.user_id = None;
        Ok(())
    }
}

/// Operaciones que, con varios puestos, se hacen una tras otra (en el
/// servidor, o con la carpeta compartida bloqueada) para que dos puestos no
/// hagan lo mismo a la vez: la cadena de facturas, y lo que cobra o liquida
/// algo (remesas, liquidaciones a mutuas, conciliación del banco). Ver
/// [`App::chain_op`].
pub enum ChainOp {
    Issue {
        patient: u32,
        entries: Vec<u32>,
        date: chrono::NaiveDate,
    },
    Rectify {
        invoice: u32,
        reason: String,
    },
    Correct {
        invoice: u32,
    },
    Cancel {
        invoice: u32,
        reason: String,
    },
    /// Remesa SEPA con las cuotas domiciliables hasta `until` (las que aún
    /// estén sin remesar al hacerla).
    Remit {
        until: chrono::NaiveDate,
        collection: chrono::NaiveDate,
    },
    Collected {
        remittance: u32,
    },
    Returned {
        remittance: u32,
        item: usize,
        reason: String,
    },
    /// Liquidación a la mutua con los actos aún sin liquidar del periodo.
    Claim {
        tariff: u32,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
        date: chrono::NaiveDate,
    },
    /// Conciliación automática del banco. Devuelve cuántos movimientos.
    Reconcile,
    /// Extracto del banco: se añade sin repetir lo que ya hay, también si otro
    /// puesto acaba de importarlo (devuelve cuántos movimientos son nuevos).
    ImportBank {
        account: u32,
        moves: Vec<crate::bank::RawMove>,
    },
    /// Conciliación a mano de un movimiento del banco, si cuadra y nada de lo
    /// elegido está ya conciliado.
    MatchBank {
        bank_move: u32,
        with: crate::bank::Match,
    },
    /// Deshacer la conciliación de un movimiento, y lo que ella cambió.
    UnmatchBank {
        bank_move: u32,
    },
    /// Gasto nuevo con un cargo del banco, conciliado con él (devuelve el gasto).
    BankExpense {
        bank_move: u32,
    },
    /// Reserva de correos pendientes para enviarlos desde este puesto (con la
    /// marca `token`), para que otro no los envíe a la vez. Devuelve cuántos.
    ClaimEmails {
        ids: Vec<u32>,
        token: String,
    },
    /// Cita pedida por internet: se apunta en la agenda (devuelve la cita) o se rechaza.
    Confirm {
        request: u32,
    },
    Reject {
        request: u32,
        why: String,
    },
    /// Cobro en la clínica de una cuota (devuelve el cobro).
    PayInstallment {
        budget: u32,
        index: u32,
        method: String,
    },
    /// Anulación de un cobro equivocado (devuelve la anulación).
    CancelPayment {
        payment: u32,
        reason: String,
    },
    /// Devolución al paciente de dinero que tiene a favor.
    Refund {
        patient: u32,
        amount: f64,
        method: String,
        reason: String,
    },
    /// Fusión de dos fichas del mismo paciente (solo Dirección): todo lo de
    /// `remove`, también lo firmado, pasa a `keep`.
    MergePatients {
        keep: u32,
        remove: u32,
    },
}

impl ChainOp {
    /// Hace la operación con los datos al día. `admin`: la hace Dirección.
    pub(crate) fn run(&self, db: &mut Db, user: &str, admin: bool) -> Result<u32, String> {
        match self {
            ChainOp::Issue { patient, entries, date } => db.issue_invoice(*patient, entries, *date),
            ChainOp::Rectify { invoice, reason } => db.rectify_invoice(*invoice, reason),
            ChainOp::Correct { invoice } => crate::verifactu::create_correction(db, *invoice, user),
            ChainOp::Cancel { invoice, reason } => crate::verifactu::create_cancellation(db, *invoice, reason, user),
            ChainOp::Remit { until, collection } => {
                let due = db.due_installments(*until);
                if due.is_empty() {
                    return Err("No quedan cuotas que remesar: puede que otro puesto acabe de generar la remesa.".into());
                }
                db.create_remittance(&due, *collection)
            }
            ChainOp::Collected { remittance } => db.remittance_collected(*remittance).map(|_| *remittance),
            ChainOp::Returned { remittance, item, reason } => db.remittance_returned(*remittance, *item, reason).map(|_| *remittance),
            ChainOp::Claim { tariff, from, to, date } => {
                let id = db.create_claim(*tariff, *from, *to, *date).ok_or("No hay actos pendientes de liquidar en el periodo: puede que otro puesto acabe de liquidarlos.")?;
                let num = db.claims.iter().find(|c| c.id == id).map(|c| c.number.clone()).unwrap_or_default();
                let tname = db.tariff_name(*tariff);
                db.audit("Liquidación a mutua", &format!("{num} · {tname}"));
                Ok(id)
            }
            ChainOp::Reconcile => Ok(db.auto_reconcile() as u32),
            ChainOp::ImportBank { account, moves } => db.import_bank_moves(*account, moves),
            ChainOp::MatchBank { bank_move, with } => db.match_bank_move(*bank_move, with.clone()),
            ChainOp::UnmatchBank { bank_move } => db.unmatch_bank_move(*bank_move),
            ChainOp::BankExpense { bank_move } => db.expense_for_move(*bank_move),
            ChainOp::ClaimEmails { ids, token } => Ok(db.claim_emails(ids, token, crate::model::now())),
            ChainOp::Confirm { request } => db.booking_confirm(*request),
            ChainOp::Reject { request, why } => db.booking_reject(*request, why).map(|_| *request),
            ChainOp::PayInstallment { budget, index, method } => db.pay_installment(*budget, *index, method),
            ChainOp::CancelPayment { payment, reason } => db.cancel_payment(*payment, reason, admin),
            ChainOp::Refund { patient, amount, method, reason } => db.refund(*patient, *amount, method, reason),
            ChainOp::MergePatients { keep, remove } => {
                if !admin {
                    return Err("Solo Dirección puede fusionar pacientes.".into());
                }
                db.merge_patients(*keep, *remove).map(|_| *keep)
            }
        }
    }

    pub(crate) fn json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            ChainOp::Issue { patient, entries, date } => json!({ "accion": "emitir", "paciente": patient, "tratamientos": entries, "fecha": date }),
            ChainOp::Rectify { invoice, reason } => json!({ "accion": "rectificar", "factura": invoice, "motivo": reason }),
            ChainOp::Correct { invoice } => json!({ "accion": "subsanar", "factura": invoice }),
            ChainOp::Cancel { invoice, reason } => json!({ "accion": "anular", "factura": invoice, "motivo": reason }),
            ChainOp::Remit { until, collection } => json!({ "accion": "remesa", "hasta": until, "cargo": collection }),
            ChainOp::Collected { remittance } => json!({ "accion": "remesa_cobrada", "remesa": remittance }),
            ChainOp::Returned { remittance, item, reason } => json!({ "accion": "recibo_devuelto", "remesa": remittance, "recibo": item, "motivo": reason }),
            ChainOp::Claim { tariff, from, to, date } => json!({ "accion": "liquidacion", "tarifa": tariff, "desde": from, "hasta": to, "fecha": date }),
            ChainOp::Reconcile => json!({ "accion": "conciliar" }),
            ChainOp::ImportBank { account, moves } => json!({
                "accion": "importar_extracto",
                "cuenta": account,
                "movimientos": moves.iter().map(|m| json!({ "fecha": m.date, "centimos": (m.amount * 100.0).round() as i64, "concepto": m.concept, "referencia": m.reference })).collect::<Vec<_>>(),
            }),
            ChainOp::MatchBank { bank_move, with } => json!({ "accion": "conciliar_movimiento", "movimiento": bank_move, "con": with }),
            ChainOp::UnmatchBank { bank_move } => json!({ "accion": "deshacer_conciliacion", "movimiento": bank_move }),
            ChainOp::BankExpense { bank_move } => json!({ "accion": "gasto_del_cargo", "movimiento": bank_move }),
            ChainOp::ClaimEmails { ids, token } => json!({ "accion": "reservar_correos", "mensajes": ids, "marca": token }),
            ChainOp::Confirm { request } => json!({ "accion": "confirmar_cita", "solicitud": request }),
            ChainOp::Reject { request, why } => json!({ "accion": "rechazar_cita", "solicitud": request, "motivo": why }),
            ChainOp::PayInstallment { budget, index, method } => json!({ "accion": "cobrar_cuota", "presupuesto": budget, "cuota": index, "forma": method }),
            ChainOp::CancelPayment { payment, reason } => json!({ "accion": "anular_cobro", "cobro": payment, "motivo": reason }),
            ChainOp::Refund { patient, amount, method, reason } => json!({ "accion": "devolver", "paciente": patient, "importe": amount, "forma": method, "motivo": reason }),
            ChainOp::MergePatients { keep, remove } => json!({ "accion": "fusionar", "conservar": keep, "eliminar": remove }),
        }
    }

    /// La misma operación a partir de lo que manda un puesto al servidor.
    pub(crate) fn from_json(v: &serde_json::Value) -> Result<ChainOp, String> {
        let num = |k: &str| v[k].as_u64().map(|n| n as u32).ok_or_else(|| format!("Falta «{k}»."));
        let date = |k: &str| v[k].as_str().and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()).ok_or_else(|| format!("Falta «{k}»."));
        let text = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        Ok(match v["accion"].as_str().unwrap_or_default() {
            "emitir" => ChainOp::Issue {
                patient: num("paciente")?,
                entries: v["tratamientos"].as_array().ok_or("Faltan los tratamientos.")?.iter().map(|x| x.as_u64().map(|n| n as u32).ok_or("Tratamiento no válido.")).collect::<Result<_, _>>()?,
                date: date("fecha").unwrap_or_else(|_| crate::model::today()),
            },
            "rectificar" => ChainOp::Rectify { invoice: num("factura")?, reason: text("motivo") },
            "subsanar" => ChainOp::Correct { invoice: num("factura")? },
            "anular" => ChainOp::Cancel { invoice: num("factura")?, reason: text("motivo") },
            "remesa" => ChainOp::Remit { until: date("hasta")?, collection: date("cargo")? },
            "remesa_cobrada" => ChainOp::Collected { remittance: num("remesa")? },
            "recibo_devuelto" => ChainOp::Returned { remittance: num("remesa")?, item: num("recibo")? as usize, reason: text("motivo") },
            "liquidacion" => ChainOp::Claim { tariff: num("tarifa")?, from: date("desde")?, to: date("hasta")?, date: date("fecha")? },
            "conciliar" => ChainOp::Reconcile,
            "importar_extracto" => ChainOp::ImportBank { account: num("cuenta")?, moves: bank_moves_json(&v["movimientos"])? },
            "conciliar_movimiento" => ChainOp::MatchBank { bank_move: num("movimiento")?, with: bank_match_json(&v["con"])? },
            "deshacer_conciliacion" => ChainOp::UnmatchBank { bank_move: num("movimiento")? },
            "gasto_del_cargo" => ChainOp::BankExpense { bank_move: num("movimiento")? },
            "reservar_correos" => {
                let ids = v["mensajes"].as_array().ok_or("Faltan los mensajes.")?;
                if ids.len() > 10_000 {
                    return Err("Demasiados mensajes de una vez.".into());
                }
                let token = text("marca");
                if token.is_empty() || token.len() > 64 {
                    return Err("Falta la marca del envío.".into());
                }
                ChainOp::ClaimEmails { ids: ids.iter().map(|x| x.as_u64().map(|n| n as u32).ok_or("Mensaje no válido.")).collect::<Result<_, _>>()?, token }
            }
            "confirmar_cita" => ChainOp::Confirm { request: num("solicitud")? },
            "rechazar_cita" => ChainOp::Reject { request: num("solicitud")?, why: text("motivo") },
            "cobrar_cuota" => ChainOp::PayInstallment { budget: num("presupuesto")?, index: num("cuota")?, method: pay_method(&text("forma"))? },
            "anular_cobro" => ChainOp::CancelPayment { payment: num("cobro")?, reason: text("motivo") },
            "devolver" => ChainOp::Refund { patient: num("paciente")?, amount: v["importe"].as_f64().filter(|a| a.is_finite()).ok_or("Falta «importe».")?, method: pay_method(&text("forma"))?, reason: text("motivo") },
            "fusionar" => ChainOp::MergePatients { keep: num("conservar")?, remove: num("eliminar")? },
            _ => return Err("Acción no válida.".into()),
        })
    }

    /// Quién puede hacerla (con qué permiso; ninguno = todos), y qué datos usa
    /// (los que el servidor le prepara).
    pub(crate) fn needs(&self) -> (Option<crate::users::Perm>, &'static [&'static str]) {
        use crate::users::Perm::*;
        const CHAIN: &[&str] = &["invoices", "vf_records", "entries", "patients", "config", "verifactu", "next_id"];
        const REMIT: &[&str] = &["remittances", "budgets", "patients", "payments", "cash", "config", "audit_log", "next_id"];
        const CLAIM: &[&str] = &["claims", "entries", "patients", "tariffs", "auths", "config", "audit_log", "next_id"];
        const BANK: &[&str] = &["bank_moves", "bank_accounts", "expenses", "remittances", "budgets", "patients", "payments", "cash", "config", "audit_log", "next_id"];
        const MESSAGES: &[&str] = &["messages"];
        const BOOKING: &[&str] = &["booking_requests", "appointments", "boxes", "blocks", "holidays", "booking", "appt_links", "next_id"];
        // el saldo del paciente sale de sus tratamientos y sus cobros
        const PAY: &[&str] = &["payments", "budgets", "remittances", "entries", "patients", "cash", "config", "audit_log", "next_id"];
        const MERGE: &[&str] = &[
            "patients",
            "entries",
            "budgets",
            "payments",
            "recalls",
            "waitlist",
            "invoices",
            "perio",
            "images",
            "messages",
            "documents",
            "lab_works",
            "prescriptions",
            "ortho",
            "health",
            "ia_reports",
            "whatsapp",
            "rx_links",
            "appointments",
            "stock_moves",
            "tasks",
            "chat",
            "booking_requests",
            "remittances",
            "claims",
            "slot_offers",
            "calls",
            "config",
            "audit_log",
        ];
        match self {
            ChainOp::Issue { .. } | ChainOp::Rectify { .. } | ChainOp::Correct { .. } | ChainOp::Cancel { .. } => (Some(Facturacion), CHAIN),
            ChainOp::Remit { .. } | ChainOp::Collected { .. } | ChainOp::Returned { .. } => (Some(Economia), REMIT),
            ChainOp::Claim { .. } => (Some(Facturacion), CLAIM),
            ChainOp::Reconcile | ChainOp::ImportBank { .. } | ChainOp::MatchBank { .. } | ChainOp::UnmatchBank { .. } | ChainOp::BankExpense { .. } => (Some(Gastos), BANK),
            ChainOp::Confirm { .. } | ChainOp::Reject { .. } => (None, BOOKING),
            // como generar los mensajes del día: cualquier usuario
            ChainOp::ClaimEmails { .. } => (None, MESSAGES),
            ChainOp::PayInstallment { .. } => (Some(Economia), PAY),
            // sacar dinero (o quitar un cobro) es cosa de quien lleva la caja
            ChainOp::CancelPayment { .. } | ChainOp::Refund { .. } => (Some(Caja), PAY),
            // Dirección (se comprueba al hacerla); todo lo que puede ser de un paciente
            ChainOp::MergePatients { .. } => (Some(Configuracion), MERGE),
        }
    }
}

/// Más movimientos que esto no trae un extracto: se importa por partes.
const MAX_BANK_MOVES: usize = 50_000;

/// Movimientos de un extracto que manda un puesto (acotados).
fn bank_moves_json(v: &serde_json::Value) -> Result<Vec<crate::bank::RawMove>, String> {
    let list = v.as_array().ok_or("Faltan los movimientos del extracto.")?;
    if list.len() > MAX_BANK_MOVES {
        return Err(format!("El extracto trae más de {MAX_BANK_MOVES} movimientos: impórtelo por partes."));
    }
    list.iter()
        .map(|m| {
            let date = m["fecha"].as_str().and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()).ok_or("Un movimiento no tiene una fecha válida.")?;
            let cents = m["centimos"].as_i64().filter(|c| c.abs() < 1_000_000_000_000).ok_or("Un movimiento no tiene un importe válido.")?;
            let text = |k: &str| m[k].as_str().unwrap_or_default().chars().take(500).collect::<String>();
            Ok(crate::bank::RawMove { date, amount: cents as f64 / 100.0, concept: text("concepto"), reference: text("referencia") })
        })
        .collect()
}

/// Con qué se concilia un movimiento, según lo manda un puesto (acotado).
fn bank_match_json(v: &serde_json::Value) -> Result<crate::bank::Match, String> {
    let m: crate::bank::Match = serde_json::from_value(v.clone()).map_err(|_| "Falta con qué se concilia el movimiento.")?;
    if let crate::bank::Match::Payments(p) = &m
        && p.len() > 5_000
    {
        return Err("Demasiados cobros en una conciliación.".into());
    }
    Ok(m)
}

/// Forma de pago de las que hay (las que llegan de un puesto se comprueban).
fn pay_method(m: &str) -> Result<String, String> {
    crate::model::PAY_METHODS.iter().find(|x| **x == m).map(|x| x.to_string()).ok_or_else(|| format!("Forma de pago no válida: «{m}»."))
}

/// Datos de un puesto conectado a un servidor antes de entrar: solo el nombre
/// de la clínica y sus usuarios (sin contraseñas), sin pacientes ni nada más.
pub fn login_shell(info: &crate::server::AccessInfo, last_user: &str, today: chrono::NaiveDate) -> Db {
    let mut db = Db::sample(today);
    let _ = db.start_empty();
    *db.users = info.users.clone();
    db.config.name = info.clinic.clone();
    db.config.current_user = last_user.to_string();
    db
}

impl App {
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
                let col = if sel {
                    Color32::WHITE
                } else if resp.hovered() {
                    Color32::from_rgb(222, 230, 241)
                } else {
                    soft
                };
                crate::icons::nav_icon(p, scr, r.min + vec2(24.0, r.height() / 2.0), if sel { Color32::from_rgb(94, 234, 212) } else { col });
                p.text(r.min + vec2(46.0, r.height() / 2.0), Align2::LEFT_CENTER, name, if sel { theme::sbf(14.5) } else { theme::mdf(14.5) }, col);
                // contador
                let badge = match scr {
                    Screen::Recalls => self.db.recalls.iter().filter(|r| self.db.recall_due(r) && r.date <= self.today).count(),
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
        let tier = if total > 1120.0 {
            0
        } else if total > 1000.0 {
            1
        } else {
            2
        };
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
            if let Some(pid) = self.patient
                && let Some(p) = self.db.patient(pid).cloned()
            {
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
            // imágenes (y su análisis con IA) y ortodoncia son datos clínicos
            Screen::Imagenes | Screen::Ortodoncia => self.can(Clinico),
            _ => true,
        }
    }

    fn lock(&mut self, reason: &str) {
        // lo pendiente se envía con la sesión de quien lo hizo (si entra otro
        // perfil, podría no tener permiso para guardarlo)
        if self.remote.is_some() && self.persist && self.dirty && self.save_now().is_ok() {
            self.dirty = false;
        }
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
                    ui.label(RichText::new("✔ Datos guardados").size(12.5).color(OK)).on_hover_text(Db::path().display().to_string());
                }
                if self.persist && self.remote.is_some() {
                    ui.separator();
                    ui.label(RichText::new(format!("🖧 Servidor de la clínica · puesto {}", self.db.station)).size(12.5).color(INFO)).on_hover_text(crate::shared::station().server.clone());
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
            egui::Area::new(egui::Id::unique("toast")).fixed_pos(rect.center_bottom() - vec2(180.0, 70.0)).order(egui::Order::Tooltip).show(ctx, |ui| {
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
        // en red, los servicios del arranque esperan a que lleguen los datos
        if self.pending_start && self.data_ready() {
            self.pending_start = false;
            self.start_services(&ctx);
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
            // con la pantalla bloqueada (y los datos ya cargados) los servicios siguen
            if self.data_ready() {
                // la llamada se apunta, pero no se muestra
                screens::llamadas::poll(self, &ctx);
                // los WhatsApp automáticos y la cita online siguen funcionando
                screens::whatsapp::tick(self, time);
                screens::cita_online::tick(self, &ctx, time);
                // y el envío automático de comunicaciones por correo
                screens::comunicaciones::outbox_tick(self, &ctx);
            }
            self.toast(&ctx);
            self.autosave(time);
            if ctx.input(|i| i.viewport().close_requested()) {
                self.on_close();
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
        egui::Panel::left("nav").exact_size(258.0).resizable(false).frame(Frame::new().fill(SIDEBAR)).show(ui, |ui| {
            let r = ui.max_rect();
            theme::vgradient(ui.painter(), r, SIDEBAR_TOP, SIDEBAR);
            egui::ScrollArea::vertical().id_salt("nav_scroll").show(ui, |ui| self.sidebar(ui));
        });
        egui::Panel::top("top")
            .exact_size(66.0)
            .frame(Frame::new().fill(CARD).stroke(Stroke::new(1.0, BORDER)).inner_margin(Margin::symmetric(20, 0)).shadow(egui::Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_rgba_unmultiplied(15, 30, 60, 8) }))
            .show(ui, |ui| self.topbar(ui));
        egui::Panel::bottom("status").exact_size(32.0).frame(Frame::new().fill(Color32::from_rgb(250, 251, 253)).stroke(Stroke::new(1.0, BORDER)).inner_margin(Margin::symmetric(14, 0))).show(ui, |ui| self.statusbar(ui));
        egui::CentralPanel::default().frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(26, 22))).show(ui, |ui| match self.screen {
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
        if ctx.input(|i| i.viewport().close_requested()) {
            self.on_close();
        }
        self.handle_screenshot(&ctx);
    }
}
