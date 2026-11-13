//! Datos de la clínica: pacientes, tratamientos, tarifas, citas, presupuestos,
//! cobros, caja y recalls. Todo se guarda en un único fichero JSON.

pub use crate::sync::Tracked;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Cómo se dibuja un tratamiento en el odontograma.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum OdgKind {
    Ninguno,
    Obturacion,
    Endodoncia,
    Extraccion,
    Corona,
    Implante,
    Carilla,
    Puente,
    PernoMunon,
    Apicectomia,
    ProtesisRemovible,
    Sellador,
    Ausente,
    Bracket,
    Higiene,
}

impl OdgKind {
    pub const ALL_TOOLS: [OdgKind; 13] = [
        OdgKind::Obturacion,
        OdgKind::Endodoncia,
        OdgKind::Extraccion,
        OdgKind::Corona,
        OdgKind::Implante,
        OdgKind::Carilla,
        OdgKind::Puente,
        OdgKind::PernoMunon,
        OdgKind::Apicectomia,
        OdgKind::ProtesisRemovible,
        OdgKind::Sellador,
        OdgKind::Ausente,
        OdgKind::Bracket,
    ];
    pub fn label(self) -> &'static str {
        match self {
            OdgKind::Ninguno => "General",
            OdgKind::Obturacion => "Obturación",
            OdgKind::Endodoncia => "Endodoncia",
            OdgKind::Extraccion => "Extracción",
            OdgKind::Corona => "Corona",
            OdgKind::Implante => "Implante",
            OdgKind::Carilla => "Carilla",
            OdgKind::Puente => "Puente",
            OdgKind::PernoMunon => "Perno-muñón",
            OdgKind::Apicectomia => "Apicectomía",
            OdgKind::ProtesisRemovible => "Prót. removible",
            OdgKind::Sellador => "Sellador",
            OdgKind::Ausente => "Ausencia",
            OdgKind::Bracket => "Bracket",
            OdgKind::Higiene => "Higiene",
        }
    }
    pub fn uses_faces(self) -> bool {
        matches!(self, OdgKind::Obturacion | OdgKind::Sellador)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Treatment {
    pub code: String,
    pub description: String,
    pub group: String,
    pub price: f64,
    pub minutes: u32,
    pub odg: OdgKind,
    pub per_tooth: bool,
    /// Meses tras los que se genera un recall automático al realizarlo (0 = no).
    pub recall_months: u32,
    pub active: bool,
    /// Tratamiento propio de una sola tarifa (como en GESDEN, donde cada
    /// tarifa tiene su lista): solo se ofrece a los pacientes de esa tarifa.
    #[serde(default)]
    pub only_tariff: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TariffPrice {
    pub code: String,
    pub price: f64,
    /// Lo que la compañía paga a la clínica por este acto (baremo).
    #[serde(default)]
    pub insurer_fee: f64,
}

/// Compañía (mutua) / tarifa. "Privado" es la tarifa base.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tariff {
    pub id: u32,
    pub name: String,
    pub is_insurer: bool,
    /// Descuento general sobre la tarifa base cuando no hay precio específico.
    pub discount_pct: f64,
    pub prices: Vec<TariffPrice>,
    pub phone: String,
    pub notes: String,
    pub valid_from: NaiveDate,
    pub valid_to: Option<NaiveDate>,
    pub inactive: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Collaborator {
    pub id: u32,
    pub code: String,
    pub alias: String,
    pub name: String,
    pub specialty: String,
    pub license: String,
    pub color: [u8; 3],
    pub commission_pct: f64,
    pub active: bool,
    pub phone: String,
    /// Centros en los que trabaja (vacío = todos).
    #[serde(default)]
    pub centers: Vec<u32>,
    /// La comisión se calcula sobre la producción menos el coste de laboratorio
    /// de sus tratamientos.
    #[serde(default)]
    pub deduct_lab: bool,
    /// La comisión cuenta también lo que paga la mutua por sus actos (el
    /// baremo), no solo lo que paga el paciente. Sin marcar, como siempre.
    #[serde(default)]
    pub commission_on_insurer: bool,
    /// Días y horas en que pasa consulta (vacío: el horario de la clínica).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<crate::horario::Week>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Boxroom {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub center_id: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Patient {
    pub id: u32,
    pub history_no: u32,
    pub first_name: String,
    pub last_name: String,
    pub nif: String,
    pub birth: Option<NaiveDate>,
    pub sex: String,
    pub phone: String,
    pub mobile: String,
    pub email: String,
    pub address: String,
    pub zip: String,
    pub city: String,
    pub province: String,
    pub tariff_id: u32,
    pub policy_no: String,
    pub doctor_id: u32,
    pub alerts: String,
    pub allergies: String,
    pub background: String,
    pub notes: String,
    pub registered: Option<NaiveDate>,
    pub accepts_gdpr: bool,
    pub accepts_sms: bool,
    pub accepts_email: bool,
    pub accepts_whatsapp: bool,
    /// Avisos que no quiere recibir por ningún canal (baja por finalidad):
    /// por ejemplo, felicitaciones o seguimiento de presupuestos.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_messages: Vec<crate::comms::MsgKind>,
    pub referral: String,
    pub profession: String,
    pub active: bool,
    pub discount_pct: f64,
    #[serde(default)]
    pub iban: String,
    #[serde(default)]
    pub sepa_signed: bool,
    #[serde(default)]
    pub sepa_mandate: String,
    #[serde(default)]
    pub sepa_date: Option<NaiveDate>,
    /// Quien paga y a cuyo nombre van las facturas (el padre, la madre o el
    /// tutor de un menor, o un familiar). Vacío: el propio paciente.
    #[serde(default, skip_serializing_if = "Payer::is_empty")]
    pub payer: Payer,
    /// Tratamiento de sus datos limitado (RGPD art. 18): no se le envían mensajes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restricted: Option<crate::rgpd::Mark>,
    /// Datos bloqueados (se conservan por obligación legal): fuera de las listas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<crate::rgpd::Mark>,
}

/// Persona responsable de un paciente: paga, recibe las facturas y, si el
/// paciente es menor, firma por él.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Payer {
    pub name: String,
    pub nif: String,
    /// Dirección para la factura (vacía: la del paciente).
    pub address: String,
    /// Padre, madre, tutor legal…
    pub relation: String,
}

impl Payer {
    pub fn is_empty(&self) -> bool {
        self.name.trim().is_empty() && self.nif.trim().is_empty()
    }
}

impl Db {
    /// Si el paciente es menor de 16 años, por él consiente y firma su
    /// representante: su nombre (con el parentesco), o vacío si no consta.
    /// `None` si firma el propio paciente.
    pub fn representative(&self, pid: u32, today: NaiveDate) -> Option<String> {
        let p = self.patient(pid)?;
        if !p.age(today).is_some_and(|a| a < 16) {
            return None;
        }
        let name = p.payer.name.trim();
        Some(match (name.is_empty(), p.payer.relation.trim()) {
            (true, _) => String::new(),
            (false, "") => name.to_string(),
            (false, r) => format!("{name} ({})", r.to_lowercase()),
        })
    }
}

impl Patient {
    pub fn full_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name).trim().to_string()
    }
    pub fn age(&self, today: NaiveDate) -> Option<i32> {
        self.birth.map(|b| {
            let mut a = today.year() - b.year();
            if (today.month(), today.day()) < (b.month(), b.day()) {
                a -= 1;
            }
            a
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum TxState {
    /// Existente al llegar (hecho en otra clínica).
    Previo,
    /// Propuesto / planificado.
    Previsto,
    Realizado,
}

impl TxState {
    pub fn label(self) -> &'static str {
        match self {
            TxState::Previo => "Previo",
            TxState::Previsto => "Previsto",
            TxState::Realizado => "Realizado",
        }
    }
}

/// Caras de un diente: Mesial, Distal, Oclusal/Incisal, Vestibular, Lingual/Palatina.
pub const FACES: [&str; 5] = ["M", "D", "O", "V", "L"];

/// Entrada médica (acto clínico del historial).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MedEntry {
    pub id: u32,
    pub patient_id: u32,
    pub date: NaiveDate,
    pub code: String,
    pub description: String,
    pub teeth: Vec<u8>,
    pub faces: Vec<String>,
    pub doctor_id: u32,
    pub state: TxState,
    pub price: f64,
    pub discount_pct: f64,
    pub units: u32,
    pub budget_id: Option<u32>,
    pub notes: String,
    pub odg: OdgKind,
    /// Factura en la que se ha incluido (None = pendiente de facturar).
    #[serde(default)]
    pub invoice_id: Option<u32>,
    /// Tarifa (mutua) del paciente cuando se hizo: el acto se liquida a esa
    /// compañía aunque el paciente cambie después. 0 = no se sabe (tratamientos
    /// de versiones anteriores): la tarifa actual del paciente.
    #[serde(default)]
    pub tariff_id: u32,
    /// Centro donde se hizo: el del puesto al apuntarlo o al marcarlo como
    /// hecho. 0 = de antes de guardarse (cuenta como el centro principal).
    #[serde(default)]
    pub center_id: u32,
    /// Hecho y cerrado en el programa anterior (viene de la migración): se
    /// facturó y se liquidó a la mutua allí, así que aquí no se factura ni se
    /// liquida otra vez. Sigue contando en el historial y en el saldo.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub settled_elsewhere: bool,
}

impl MedEntry {
    pub fn amount(&self) -> f64 {
        round2(self.price * self.units as f64 * (1.0 - self.discount_pct / 100.0))
    }

    /// Acto hecho de verdad: realizado y no un ajuste de saldo de la migración
    /// (que cuenta en lo que debe el paciente, pero no en la producción).
    pub fn is_act(&self) -> bool {
        self.state == TxState::Realizado && self.code != crate::migracion::ADJUST_CODE
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum BudgetState {
    Pendiente,
    Aceptado,
    Rechazado,
    Cerrado,
}

impl BudgetState {
    pub fn label(self) -> &'static str {
        match self {
            BudgetState::Pendiente => "Pendiente",
            BudgetState::Aceptado => "Aceptado",
            BudgetState::Rechazado => "Rechazado",
            BudgetState::Cerrado => "Cerrado",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetLine {
    pub code: String,
    pub description: String,
    pub teeth: Vec<u8>,
    pub units: u32,
    pub price: f64,
    pub discount_pct: f64,
    pub odg: OdgKind,
}

impl BudgetLine {
    pub fn amount(&self) -> f64 {
        round2(self.price * self.units as f64 * (1.0 - self.discount_pct / 100.0))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Budget {
    pub id: u32,
    pub patient_id: u32,
    pub title: String,
    pub date: NaiveDate,
    pub doctor_id: u32,
    pub tariff_id: u32,
    pub state: BudgetState,
    pub lines: Vec<BudgetLine>,
    pub valid_days: u32,
    pub accepted_on: Option<NaiveDate>,
    pub rejected_on: Option<NaiveDate>,
    pub reject_reason: String,
    pub notes: String,
    pub plan: Option<FinancePlan>,
}

impl Budget {
    pub fn total(&self) -> f64 {
        round2(self.lines.iter().map(|l| l.amount()).sum())
    }
    pub fn expires(&self) -> NaiveDate {
        self.date + Duration::days(self.valid_days as i64)
    }
}

/// Plan económico (financiación en cuotas).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FinancePlan {
    pub down_payment: f64,
    pub installments: u32,
    pub first_date: NaiveDate,
    pub every_months: u32,
    pub paid: Vec<bool>,
}

impl FinancePlan {
    pub fn installment_amount(&self, total: f64) -> f64 {
        if self.installments == 0 {
            return 0.0;
        }
        round2((total - self.down_payment).max(0.0) / self.installments as f64)
    }
    /// Importe de la cuota `i` (desde 0): todas como [`Self::installment_amount`]
    /// salvo la última, que completa el total (1.000 € en 3 cuotas: 333,33 +
    /// 333,33 + 333,34), para que no quede un céntimo de deuda al pagarlas todas.
    pub fn installment(&self, i: u32, total: f64) -> f64 {
        let each = self.installment_amount(total);
        if self.installments == 0 || i + 1 < self.installments {
            return each;
        }
        round2((total - self.down_payment).max(0.0) - each * (self.installments - 1) as f64)
    }
    /// «3 cuotas de 333,33 € (la última de 333,34 €)».
    pub fn installments_text(&self, total: f64) -> String {
        let each = self.installment_amount(total);
        let last = self.installment(self.installments.saturating_sub(1), total);
        let mut s = format!("{} cuotas de {}", self.installments, money(each));
        if self.installments > 1 && (last - each).abs() > 0.004 {
            s.push_str(&format!(" (la última de {})", money(last)));
        }
        s
    }
    pub fn due_date(&self, i: u32) -> NaiveDate {
        add_months(self.first_date, i * self.every_months.max(1))
    }
}

pub const PAY_METHODS: [&str; 5] = ["Efectivo", "Tarjeta", "Transferencia", "Financiación", "Bizum"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Payment {
    pub id: u32,
    pub patient_id: u32,
    pub date: NaiveDateTime,
    pub amount: f64,
    pub method: String,
    pub concept: String,
    pub receipt_no: String,
    pub user: String,
    pub cash_session: Option<u32>,
    #[serde(default)]
    pub center_id: u32,
    /// Cobro que anula este (en las anulaciones, que van en negativo).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancels: Option<u32>,
    /// Cuota que paga: (presupuesto, nº de cuota desde 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installment: Option<(u32, u32)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CashMove {
    pub when: NaiveDateTime,
    pub amount: f64,
    pub concept: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CashState {
    Abierta,
    Cerrada,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CashSession {
    pub id: u32,
    pub opened: NaiveDateTime,
    pub closed: Option<NaiveDateTime>,
    pub opening_balance: f64,
    pub moves: Vec<CashMove>,
    pub counted: Option<f64>,
    pub state: CashState,
    pub user: String,
    pub notes: String,
    #[serde(default)]
    pub center_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ApptState {
    Planificada,
    Confirmada,
    Recepcion,
    SalaEspera,
    EnConsulta,
    Finalizada,
    NoAcude,
    Anulada,
    Reprogramada,
}

impl ApptState {
    pub const ALL: [ApptState; 9] = [ApptState::Planificada, ApptState::Confirmada, ApptState::Recepcion, ApptState::SalaEspera, ApptState::EnConsulta, ApptState::Finalizada, ApptState::NoAcude, ApptState::Anulada, ApptState::Reprogramada];
    pub fn label(self) -> &'static str {
        match self {
            ApptState::Planificada => "Planificada",
            ApptState::Confirmada => "Confirmada",
            ApptState::Recepcion => "Recepción",
            ApptState::SalaEspera => "Sala de espera",
            ApptState::EnConsulta => "En consulta",
            ApptState::Finalizada => "Finalizada",
            ApptState::NoAcude => "No acude",
            ApptState::Anulada => "Anulada",
            ApptState::Reprogramada => "Reprogramada",
        }
    }
    pub fn color(self) -> [u8; 3] {
        match self {
            ApptState::Planificada => [120, 144, 176],
            ApptState::Confirmada => [56, 132, 214],
            ApptState::Recepcion => [142, 98, 214],
            ApptState::SalaEspera => [232, 152, 36],
            ApptState::EnConsulta => [214, 76, 120],
            ApptState::Finalizada => [46, 160, 104],
            ApptState::NoAcude => [204, 64, 64],
            ApptState::Anulada => [150, 150, 150],
            ApptState::Reprogramada => [170, 130, 90],
        }
    }
    /// Siguiente paso natural del flujo de recepción.
    pub fn next(self) -> Option<ApptState> {
        match self {
            ApptState::Planificada | ApptState::Confirmada | ApptState::Recepcion => Some(ApptState::SalaEspera),
            ApptState::SalaEspera => Some(ApptState::EnConsulta),
            ApptState::EnConsulta => Some(ApptState::Finalizada),
            _ => None,
        }
    }
    pub fn is_cancelled(self) -> bool {
        matches!(self, ApptState::Anulada | ApptState::Reprogramada | ApptState::NoAcude)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Appointment {
    pub id: u32,
    pub date: NaiveDate,
    pub start: NaiveTime,
    pub minutes: u32,
    pub patient_id: Option<u32>,
    /// Para pacientes aún no dados de alta.
    pub free_name: String,
    pub phone: String,
    pub doctor_id: u32,
    pub box_id: u32,
    pub reason: String,
    pub treatment_code: String,
    pub state: ApptState,
    pub notes: String,
    pub arrived_at: Option<NaiveTime>,
    pub seen_at: Option<NaiveTime>,
    pub finished_at: Option<NaiveTime>,
    pub reminder_sent: bool,
    pub first_visit: bool,
    #[serde(default)]
    pub center_id: u32,
}

impl Appointment {
    pub fn end(&self) -> NaiveTime {
        self.start + Duration::minutes(self.minutes as i64)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    pub id: u32,
    pub doctor_id: u32,
    /// None = todos los días laborables.
    pub date: Option<NaiveDate>,
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WaitItem {
    pub id: u32,
    pub patient_id: u32,
    pub doctor_id: u32,
    pub reason: String,
    pub since: NaiveDate,
    pub preference: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum RecallState {
    Pendiente,
    Contactado,
    Citado,
    Rechazado,
}

impl RecallState {
    pub fn label(self) -> &'static str {
        match self {
            RecallState::Pendiente => "Pendiente",
            RecallState::Contactado => "Contactado",
            RecallState::Citado => "Citado",
            RecallState::Rechazado => "Rechazado",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recall {
    pub id: u32,
    pub patient_id: u32,
    pub date: NaiveDate,
    pub reason: String,
    pub doctor_id: u32,
    pub state: RecallState,
    pub channel: String,
    pub comment: String,
    pub contacted_on: Option<NaiveDate>,
    /// Cita que lo atiende (cuando está «Citado»): si se anula, vuelve a pendiente.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appt_id: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClinicConfig {
    pub name: String,
    pub nif: String,
    pub address: String,
    pub phone: String,
    pub email: String,
    /// Primera hora de apertura y última de cierre (lo que abarca la agenda;
    /// con horario por días, se calculan de él).
    pub open_time: NaiveTime,
    pub close_time: NaiveTime,
    /// Horario por día de la semana (vacío: de lunes a viernes, de la apertura al cierre).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<crate::horario::Week>,
    pub slot_minutes: u32,
    pub receipt_series: String,
    pub next_receipt: u32,
    pub budget_valid_days: u32,
    pub users: Vec<String>,
    pub current_user: String,
    #[serde(default = "default_center")]
    pub current_center: u32,
    #[serde(default)]
    pub creditor_id: String,
    #[serde(default)]
    pub clinic_iban: String,
    #[serde(default)]
    pub backup_extra: String,
    #[serde(default)]
    pub board_initials: bool,
    /// Artículo de exención de IVA que llevarán las facturas nuevas (las
    /// emitidas guardan el suyo). Por defecto, el de siempre (20.Uno.3º).
    #[serde(default)]
    pub vat_exemption: VatExemption,
    /// Epígrafe del IAE y tipo de actividad para los libros registro (`libros`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub iae_epigrafe: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub actividad_tipo: String,
    /// Correo de soporte para enviar el informe de errores (vacío: no se envía).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub support_email: String,
    /// Perfiles de columnas de la migración (ver `migracion::Profile`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mig_profiles: Vec<crate::migracion::Profile>,
}

// Listas que empiezan con contenido (para los datos guardados por versiones que no las tenían).
fn tracked_connectors() -> Tracked<Vec<Connector>> {
    default_connectors().into()
}
fn tracked_msg_templates() -> Tracked<Vec<crate::comms::MsgTemplate>> {
    crate::comms::default_msg_templates().into()
}
fn tracked_doc_templates() -> Tracked<Vec<crate::comms::DocTemplate>> {
    crate::comms::default_doc_templates().into()
}
fn tracked_users() -> Tracked<Vec<crate::users::User>> {
    crate::users::default_users().into()
}
fn tracked_centers() -> Tracked<Vec<Center>> {
    default_centers().into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Db {
    pub config: ClinicConfig,
    pub treatments: Vec<Treatment>,
    pub tariffs: Tracked<Vec<Tariff>>,
    pub collaborators: Tracked<Vec<Collaborator>>,
    pub boxes: Tracked<Vec<Boxroom>>,
    pub patients: Tracked<Vec<Patient>>,
    pub entries: Tracked<Vec<MedEntry>>,
    pub budgets: Tracked<Vec<Budget>>,
    pub payments: Tracked<Vec<Payment>>,
    pub cash: Tracked<Vec<CashSession>>,
    pub appointments: Tracked<Vec<Appointment>>,
    pub blocks: Tracked<Vec<Block>>,
    pub waitlist: Tracked<Vec<WaitItem>>,
    pub recalls: Tracked<Vec<Recall>>,
    pub next_id: u32,
    #[serde(default)]
    pub invoices: Tracked<Vec<Invoice>>,
    #[serde(default)]
    pub perio: Tracked<Vec<PerioChart>>,
    #[serde(default)]
    pub images: Tracked<Vec<PatientImage>>,
    #[serde(default = "tracked_connectors")]
    pub connectors: Tracked<Vec<Connector>>,
    #[serde(default)]
    pub launches: Vec<LaunchLog>,
    #[serde(default = "tracked_msg_templates")]
    pub msg_templates: Tracked<Vec<crate::comms::MsgTemplate>>,
    #[serde(default)]
    pub messages: Tracked<Vec<crate::comms::Message>>,
    #[serde(default = "tracked_doc_templates")]
    pub doc_templates: Tracked<Vec<crate::comms::DocTemplate>>,
    #[serde(default)]
    pub documents: Tracked<Vec<crate::comms::PatientDoc>>,
    #[serde(default)]
    pub suppliers: Tracked<Vec<crate::stock::Supplier>>,
    #[serde(default)]
    pub products: Tracked<Vec<crate::stock::Product>>,
    #[serde(default)]
    pub stock_moves: Tracked<Vec<crate::stock::StockMove>>,
    #[serde(default)]
    pub consumptions: Vec<crate::stock::Consumption>,
    #[serde(default)]
    pub orders: Tracked<Vec<crate::stock::PurchaseOrder>>,
    #[serde(default)]
    pub lab_works: Tracked<Vec<crate::stock::LabWork>>,
    #[serde(default = "tracked_users")]
    pub users: Tracked<Vec<crate::users::User>>,
    #[serde(default)]
    pub access_log: Vec<crate::users::AccessLog>,
    #[serde(default)]
    pub expenses: Tracked<Vec<crate::finance::Expense>>,
    #[serde(default = "crate::finance::default_drugs")]
    pub drugs: Vec<crate::finance::Drug>,
    #[serde(default)]
    pub prescriptions: Tracked<Vec<crate::finance::Prescription>>,
    #[serde(default)]
    pub ortho: Tracked<Vec<crate::extra::OrthoCase>>,
    #[serde(default)]
    pub remittances: Tracked<Vec<crate::extra::Remittance>>,
    #[serde(default)]
    pub contacts: Tracked<Vec<crate::tools::Contact>>,
    #[serde(default)]
    pub bank_accounts: Tracked<Vec<crate::bank::BankAccount>>,
    #[serde(default)]
    pub bank_moves: Tracked<Vec<crate::bank::BankMove>>,
    #[serde(default)]
    pub audit_log: Vec<crate::bank::AuditEntry>,
    #[serde(default = "tracked_centers")]
    pub centers: Tracked<Vec<Center>>,
    #[serde(default)]
    pub holidays: Vec<crate::agenda_extra::Holiday>,
    #[serde(default)]
    pub health: Tracked<Vec<crate::health::HealthForm>>,
    #[serde(default)]
    pub claims: Tracked<Vec<crate::insurer::Claim>>,
    /// Nº de autorización de la compañía por acto (id del acto, número).
    #[serde(default)]
    pub auths: Vec<(u32, String)>,
    #[serde(default)]
    pub erx: crate::erx::ErxConfig,
    #[serde(default)]
    pub tasks: Tracked<Vec<crate::tasks::Task>>,
    /// Revisión del fichero (sube en cada guardado; sirve para detectar cambios de otros puestos).
    #[serde(default)]
    pub rev: u64,
    #[serde(default)]
    pub stations: Tracked<Vec<crate::shared::StationSeen>>,
    #[serde(default)]
    pub chat: Tracked<Vec<crate::chat::InternalMsg>>,
    #[serde(default)]
    pub calls: Tracked<Vec<crate::phone::CallLog>>,
    #[serde(default)]
    pub whatsapp: Tracked<Vec<crate::whatsapp::WaMessage>>,
    /// Avisos de Meta ya procesados (Meta reintenta durante 7 días).
    #[serde(default)]
    pub wa_seen: Vec<String>,
    /// Ordenador que envía los WhatsApp automáticos (vacío = desactivado).
    #[serde(default)]
    pub wa_sender: String,
    /// Remisión de los registros de facturación a la AEAT (VERI*FACTU).
    #[serde(default)]
    pub verifactu: crate::verifactu::VfConfig,
    /// Correo electrónico saliente (SMTP).
    #[serde(default)]
    pub mail: crate::mail::MailConfig,
    /// Registros de facturación posteriores a la factura (subsanaciones y anulaciones).
    #[serde(default)]
    pub vf_records: Tracked<Vec<crate::verifactu::VfRecord>>,
    /// Análisis de imágenes con inteligencia artificial: agentes y modo.
    #[serde(default)]
    pub ia: crate::ia::IaConfig,
    /// Análisis guardados.
    #[serde(default)]
    pub ia_reports: Tracked<Vec<crate::ia::IaReport>>,
    /// Cita online: página para que los pacientes pidan cita.
    #[serde(default)]
    pub booking: crate::booking::BookingConfig,
    /// Solicitudes de cita recibidas desde la página.
    #[serde(default)]
    pub booking_requests: Tracked<Vec<crate::booking::BookingRequest>>,
    /// Enlaces personales de las citas («Mi cita»).
    #[serde(default)]
    pub appt_links: Tracked<Vec<crate::portal::ApptLink>>,
    /// Presupuestos enviados para firmar por internet.
    #[serde(default)]
    pub budget_links: Tracked<Vec<crate::budget_link::BudgetLink>>,
    /// Documentos enviados para firmar por internet.
    #[serde(default)]
    pub doc_links: Tracked<Vec<crate::doc_link::DocLink>>,
    /// Huecos libres ofrecidos a la lista de espera.
    #[serde(default)]
    pub slot_offers: Tracked<Vec<crate::slot_offer::SlotOffer>>,
    /// Pasos de la puesta en marcha que la clínica marca a mano.
    #[serde(default)]
    pub golive: Vec<crate::golive::Tick>,
    /// Enlaces «Mis recetas» de los pacientes.
    #[serde(default)]
    pub rx_links: Tracked<Vec<crate::rx_link::RxLink>>,
    /// Lo importado de otro programa, con su identificador allí (`migracion`).
    #[serde(default)]
    pub imports: Tracked<Vec<crate::migracion::ImportRef>>,
    /// Registro de jornada: fichajes y correcciones (`jornada`).
    #[serde(default)]
    pub punches: Tracked<Vec<crate::jornada::Punch>>,
    /// Solicitudes de derechos de protección de datos (`rgpd`).
    #[serde(default)]
    pub rights_requests: Tracked<Vec<crate::rgpd::RightsRequest>>,
    /// Medidas del piloto (tiempos y pasos, incidencias, caídas, restauraciones).
    #[serde(default)]
    pub measures: Tracked<Vec<crate::medidas::Measure>>,
    /// Versión de los datos (para completar lo que añaden las versiones nuevas del programa).
    #[serde(default)]
    pub data_version: u32,
    /// Número de puesto de este ordenador (no se guarda).
    #[serde(skip)]
    pub station: u32,
}

// ───────────────────────────── Multicentro ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Center {
    pub id: u32,
    pub name: String,
    pub address: String,
    pub phone: String,
    pub color: [u8; 3],
    pub active: bool,
}

pub fn default_center() -> u32 {
    1
}

pub fn default_centers() -> Vec<Center> {
    vec![Center { id: 1, name: "Centro principal".into(), address: String::new(), phone: String::new(), color: [14, 150, 136], active: true }]
}

/// Los datos antiguos sin centro pertenecen al centro 1.
/// Versión actual de los datos (ver `Db::upgrade`).
pub const DATA_VERSION: u32 = 1;

pub fn cid(id: u32) -> u32 {
    if id == 0 { 1 } else { id }
}

impl Db {
    pub fn center_name(&self, id: u32) -> String {
        self.centers.iter().find(|c| c.id == cid(id)).map(|c| c.name.clone()).unwrap_or_else(|| "—".into())
    }
    pub fn multi_center(&self) -> bool {
        self.centers.iter().filter(|c| c.active).count() > 1
    }
    /// ¿Trabaja el colaborador en el centro indicado?
    pub fn works_in(&self, doctor_id: u32, center: u32) -> bool {
        self.doctor(doctor_id).map(|d| d.centers.is_empty() || d.centers.contains(&cid(center))).unwrap_or(false)
    }
}

// ───────────────────────────── Imágenes e integraciones ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ImageKind {
    Periapical,
    AletaMordida,
    Panoramica,
    Cefalometrica,
    Tac,
    FotoIntraoral,
    FotoExtraoral,
    Escaneo3D,
    Documento,
}

impl ImageKind {
    pub const ALL: [ImageKind; 9] = [ImageKind::Periapical, ImageKind::AletaMordida, ImageKind::Panoramica, ImageKind::Cefalometrica, ImageKind::Tac, ImageKind::FotoIntraoral, ImageKind::FotoExtraoral, ImageKind::Escaneo3D, ImageKind::Documento];
    pub fn label(self) -> &'static str {
        match self {
            ImageKind::Periapical => "Radiografía periapical",
            ImageKind::AletaMordida => "Aleta de mordida",
            ImageKind::Panoramica => "Ortopantomografía",
            ImageKind::Cefalometrica => "Telerradiografía lateral",
            ImageKind::Tac => "TAC / CBCT 3D",
            ImageKind::FotoIntraoral => "Foto intraoral",
            ImageKind::FotoExtraoral => "Foto extraoral",
            ImageKind::Escaneo3D => "Escáner intraoral (STL)",
            ImageKind::Documento => "Documento",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            ImageKind::Periapical => "Periapical",
            ImageKind::AletaMordida => "Aleta",
            ImageKind::Panoramica => "Panorámica",
            ImageKind::Cefalometrica => "Telerradiografía",
            ImageKind::Tac => "TAC 3D",
            ImageKind::FotoIntraoral => "Foto intraoral",
            ImageKind::FotoExtraoral => "Foto extraoral",
            ImageKind::Escaneo3D => "Escáner 3D",
            ImageKind::Documento => "Documento",
        }
    }
    pub fn is_xray(self) -> bool {
        matches!(self, ImageKind::Periapical | ImageKind::AletaMordida | ImageKind::Panoramica | ImageKind::Cefalometrica | ImageKind::Tac)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatientImage {
    pub id: u32,
    pub patient_id: u32,
    pub kind: ImageKind,
    pub date: NaiveDate,
    pub teeth: Vec<u8>,
    /// Fichero (png/jpg/bmp/dcm/stl) o carpeta de la serie (TAC).
    pub path: String,
    /// Programa de origen (enlace) o "Importada".
    pub source: String,
    pub notes: String,
    /// Imagen de demostración generada por la aplicación (sin fichero).
    #[serde(default)]
    pub demo: bool,
}

/// Enlace con un programa externo de imagen (radiovisiógrafo, panorámico,
/// TAC/CBCT, cámara intraoral, escáner…). Igual que el original: se escribe el
/// fichero puente IMCONECT.ini con los datos del paciente y se lanza el
/// programa con los parámetros indicados.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Connector {
    pub id: u32,
    pub name: String,
    pub vendor: String,
    pub kind: ImageKind,
    pub exe: String,
    /// Parámetros con comodines: [IDPAC] [NUMPAC] [NOMPAC] [APELPAC] [NIF] [FECNAC] [SEXO] [CARPETA]
    pub args: String,
    pub bridge_file: bool,
    pub bridge_path: String,
    /// Carpeta donde el programa deja las imágenes exportadas (para importarlas).
    pub image_folder: String,
    pub enabled: bool,
    pub notes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaunchLog {
    pub when: NaiveDateTime,
    pub patient_id: u32,
    pub connector: String,
    pub command: String,
    pub ok: bool,
    pub message: String,
}

pub fn default_connectors() -> Vec<Connector> {
    let c = |id: u32, name: &str, vendor: &str, kind: ImageKind, exe: &str, args: &str, notes: &str| Connector {
        id,
        name: name.into(),
        vendor: vendor.into(),
        kind,
        exe: exe.into(),
        args: args.into(),
        bridge_file: true,
        bridge_path: String::new(),
        image_folder: String::new(),
        enabled: false,
        notes: notes.into(),
    };
    vec![
        c(
            1,
            "Radiovisiógrafo (RVG)",
            "Carestream / Kodak Dental Imaging",
            ImageKind::Periapical,
            r"C:\Program Files (x86)\Carestream\Dental Imaging Software\TW.exe",
            "-P\\GESDEN\\[NUMPAC] -N[NOMPAC] [APELPAC]",
            "Captura de periapicales y aletas. Equivale al enlace enl_kodak del original.",
        ),
        c(2, "Ortopantomógrafo", "Carestream CS / Trophy", ImageKind::Panoramica, r"C:\Program Files (x86)\Carestream\Dental Imaging Software\TW.exe", "-P\\GESDEN\\[NUMPAC] -N[NOMPAC] [APELPAC]", "Panorámicas y telerradiografías."),
        c(3, "TAC / CBCT 3D", "CS 3D Imaging / visor 3D", ImageKind::Tac, r"C:\Program Files\Carestream\CS 3D Imaging\CS3DImaging.exe", "/patientid=[NUMPAC] /name=\"[APELPAC], [NOMPAC]\"", "Abre los estudios volumétricos del paciente."),
        c(4, "Cámara intraoral", "Sopro Imaging (Acteon)", ImageKind::FotoIntraoral, r"C:\Program Files\Sopro Imaging\SoproImaging.exe", "[IDPAC]", "Equivale al enlace enl_sopro del original."),
        c(5, "Sensores AIS", "Acteon Imaging Suite", ImageKind::Periapical, r"C:\AISsoftware\AISmain\AIS.exe", "[IDPAC]", "Equivale al enlace enl_ais del original."),
        c(6, "Planmeca Romexis", "Planmeca", ImageKind::Tac, r"C:\Program Files\Planmeca\Romexis\client\Romexis.exe", "-patientid [NUMPAC]", "Radiología 2D/3D Planmeca."),
        c(7, "Sidexis", "Dentsply Sirona", ImageKind::Panoramica, r"C:\Program Files\Sirona\Sidexis\SIDEXIS4.exe", "/PATID:[NUMPAC]", "Radiología Sirona (puente SLIDA)."),
        c(8, "Visor DICOM", "Visor genérico", ImageKind::Tac, "", "\"[CARPETA]\"", "Abre la carpeta de imágenes del paciente con un visor DICOM."),
        c(9, "Escáner intraoral", "Escáner 3D (STL)", ImageKind::Escaneo3D, "", "[NUMPAC]", "Programa del escáner; las mallas STL se importan a la galería."),
    ]
}

/// Texto del paciente para la línea de órdenes de un programa externo: solo
/// letras, cifras, espacios y los signos de un nombre (punto, coma, apóstrofo,
/// guion dentro de la palabra, º, ª, paréntesis). Sin comillas, barras ni dos
/// puntos, y ninguna palabra empieza por guion: un dato no puede añadir
/// opciones ni rutas al programa.
pub fn arg_safe(s: &str) -> String {
    let kept: String = s.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '.' | ',' | '\'' | '-' | 'º' | 'ª' | '(' | ')') { c } else { ' ' }).collect();
    kept.split_whitespace().map(|w| w.trim_start_matches(['-', '.', ',', '\''])).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Texto para una línea del fichero puente: sin saltos de línea ni otros
/// caracteres de control.
pub fn ini_safe(s: &str) -> String {
    s.split(char::is_control).filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ")
}

impl Db {
    /// Carpeta de imágenes del paciente (como IMG/PACIENTES del original).
    pub fn patient_image_dir(&self, pid: u32) -> PathBuf {
        let n = self.patient(pid).map(|p| p.history_no).unwrap_or(pid);
        crate::paths::data().join(crate::brand::DIR).join("imagenes").join(n.to_string())
    }

    /// Sustituye los comodines del enlace por los datos del paciente. Los
    /// textos del paciente (que pueden venir de la cita online) se limpian con
    /// [`arg_safe`] para que no puedan añadir opciones ni rutas al programa.
    pub fn expand_args(&self, template: &str, pid: u32) -> String {
        let p = self.patient(pid).cloned().unwrap_or_default();
        template
            .replace("[IDPAC]", &p.id.to_string())
            .replace("[NUMPAC]", &p.history_no.to_string())
            .replace("[NOMPAC]", &arg_safe(&p.first_name))
            .replace("[APELPAC]", &arg_safe(&p.last_name))
            .replace("[NIF]", &arg_safe(&p.nif))
            .replace("[FECNAC]", &p.birth.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default())
            .replace("[SEXO]", if p.sex == "F" { "M" } else { "H" })
            .replace("[CARPETA]", &self.patient_image_dir(pid).display().to_string())
    }

    /// Contenido del fichero puente IMCONECT.ini (mismo formato que el original).
    /// Ningún dato puede llevar saltos de línea (añadirían claves al fichero).
    pub fn bridge_ini(&self, pid: u32) -> String {
        let p = self.patient(pid).cloned().unwrap_or_default();
        format!(
            "[DATOS FILIACION]\r\nIdPac={}\r\nNumPac={}\r\nNombre={}\r\nApellidos={}\r\nDireccion={}\r\nPoblacion={}\r\nCP={}\r\nProvincia={}\r\nTelPrivado={}\r\nTelTrabajo={}\r\nFecNacim={}\r\nSexo={}\r\n",
            p.id,
            p.history_no,
            ini_safe(&p.first_name).to_uppercase(),
            ini_safe(&p.last_name).to_uppercase(),
            ini_safe(&p.address).to_uppercase(),
            ini_safe(&p.city).to_uppercase(),
            ini_safe(&p.zip),
            ini_safe(&p.province).to_uppercase(),
            ini_safe(&p.mobile),
            ini_safe(&p.phone),
            p.birth.map(|d| format!("{}/{}/{}", d.day(), d.month(), d.year())).unwrap_or_default(),
            if p.sex == "F" { "M" } else { "H" },
        )
    }
}

// ───────────────────────────── Facturación ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvoiceLine {
    pub code: String,
    pub description: String,
    pub teeth: Vec<u8>,
    pub units: u32,
    pub price: f64,
    pub discount_pct: f64,
    /// Los servicios odontológicos están exentos de IVA (0 %).
    pub vat_pct: f64,
}

impl InvoiceLine {
    pub fn base(&self) -> f64 {
        round2(self.price * self.units as f64 * (1.0 - self.discount_pct / 100.0))
    }
    pub fn vat(&self) -> f64 {
        round2(self.base() * self.vat_pct / 100.0)
    }
}

/// Artículo de la Ley 37/1992 por el que los servicios facturados están
/// exentos de IVA. Se guarda en cada factura al emitirla: cambiar el de la
/// clínica solo afecta a las nuevas (la decisión es de la gestoría).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VatExemption {
    /// Art. 20.Uno.3º: asistencia a personas físicas por profesionales médicos
    /// o sanitarios. Es la que citaban todas las facturas hasta ahora.
    #[default]
    Art20Uno3,
    /// Art. 20.Uno.5º: servicios de estomatólogos, odontólogos, mecánicos
    /// dentistas y protésicos dentales, y sus prótesis dentales.
    Art20Uno5,
}

impl VatExemption {
    pub const ALL: [VatExemption; 2] = [VatExemption::Art20Uno3, VatExemption::Art20Uno5];
    /// La que imprimían las facturas emitidas antes de poder elegirla.
    pub const LEGACY: VatExemption = VatExemption::Art20Uno3;

    pub fn article(self) -> &'static str {
        match self {
            VatExemption::Art20Uno3 => "20.Uno.3º",
            VatExemption::Art20Uno5 => "20.Uno.5º",
        }
    }

    /// Qué exime, en palabras de la ley (BOE-A-1992-28740, consultado el 13/11/2026).
    pub fn scope(self) -> &'static str {
        match self {
            VatExemption::Art20Uno3 => "asistencia a personas físicas por profesionales médicos o sanitarios",
            VatExemption::Art20Uno5 => "servicios de estomatólogos, odontólogos, mecánicos dentistas y protésicos dentales y sus prótesis dentales",
        }
    }

    /// Resumen corto para la leyenda.
    fn short(self) -> &'static str {
        match self {
            VatExemption::Art20Uno3 => "asistencia sanitaria",
            VatExemption::Art20Uno5 => "servicios de odontología y prótesis dentales",
        }
    }

    /// Leyenda de la factura. Con el 20.Uno.3º es, palabra por palabra, la
    /// que imprimían las facturas antes de poder elegirla.
    pub fn legend(self) -> String {
        format!("Operación exenta de IVA según el art. {} de la Ley 37/1992 ({}).", self.article(), self.short())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invoice {
    pub id: u32,
    pub series: String,
    pub number: u32,
    pub date: NaiveDate,
    pub created: NaiveDateTime,
    pub patient_id: u32,
    pub client_name: String,
    pub client_nif: String,
    pub client_address: String,
    /// Paciente atendido, cuando la factura va a otra persona (el padre de un menor…).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub patient_name: String,
    pub lines: Vec<InvoiceLine>,
    /// Si es rectificativa, factura que rectifica.
    pub rectifies: Option<u32>,
    pub rectified_by: Option<u32>,
    pub reason: String,
    pub entry_ids: Vec<u32>,
    /// Huella del registro anterior y de éste (cadena tipo VeriFactu).
    pub prev_hash: String,
    pub hash: String,
    pub user: String,
    /// Fecha, hora y huso horario de generación del registro (p. ej. 2026-11-07T20:15:03+01:00).
    #[serde(default)]
    pub gen_ts: String,
    /// Tipo de factura VERI*FACTU: F1 completa, F2 simplificada, R1/R5 rectificativas.
    #[serde(default)]
    pub tipo: String,
    /// NIF del emisor al emitirla (la huella no cambia si luego cambia el de la clínica).
    #[serde(default)]
    pub issuer_nif: String,
    /// Estado de la remisión a la AEAT.
    #[serde(default)]
    pub aeat: crate::verifactu::AeatStatus,
    /// Artículo de exención con que se emitió (ninguno: anterior a poder
    /// elegirlo, imprimía el 20.Uno.3º). No entra en la huella.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exemption: Option<VatExemption>,
}

impl Invoice {
    /// Leyenda de exención con que se emitió la factura.
    pub fn exemption_legend(&self) -> String {
        self.exemption.unwrap_or(VatExemption::LEGACY).legend()
    }

    pub fn code(&self) -> String {
        format!("{}-{:05}", self.series, self.number)
    }
    pub fn base(&self) -> f64 {
        round2(self.lines.iter().map(|l| l.base()).sum())
    }
    pub fn vat(&self) -> f64 {
        round2(self.lines.iter().map(|l| l.vat()).sum())
    }
    pub fn total(&self) -> f64 {
        round2(self.base() + self.vat())
    }
    pub fn kind(&self) -> &str {
        if !self.tipo.is_empty() {
            return &self.tipo;
        }
        if self.rectifies.is_some() { "R1" } else { "F1" }
    }
    /// Cadena canónica sobre la que se calcula la huella SHA-256.
    pub fn canonical(&self, issuer_nif: &str) -> String {
        format!(
            "IDEmisorFactura={}&NumSerieFactura={}&FechaExpedicionFactura={}&TipoFactura={}&CuotaTotal={:.2}&ImporteTotal={:.2}&Huella={}&FechaHoraHusoGenRegistro={}",
            if self.issuer_nif.is_empty() { issuer_nif } else { &self.issuer_nif },
            self.code(),
            self.date.format("%d-%m-%Y"),
            self.kind(),
            self.vat(),
            self.total(),
            self.prev_hash,
            crate::verifactu::gen_ts(self)
        )
    }
}

pub fn sha256_hex(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(s.as_bytes());
    d.iter().map(|b| format!("{b:02X}")).collect()
}

// ───────────────────────────── Periodontograma ─────────────────────────────

/// Seis puntos de sondaje por pieza: vestibular (D, centro, M) y lingual (D, centro, M).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PerioTooth {
    pub tooth: u8,
    pub depth: [u8; 6],
    pub recession: [u8; 6],
    pub bleeding: [bool; 6],
    pub plaque: [bool; 6],
    pub mobility: u8,
    pub furcation: u8,
    pub missing: bool,
}

impl PerioTooth {
    pub fn attachment(&self, i: usize) -> u8 {
        self.depth[i] + self.recession[i]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PerioChart {
    pub id: u32,
    pub patient_id: u32,
    pub date: NaiveDate,
    pub doctor_id: u32,
    pub teeth: Vec<PerioTooth>,
    pub notes: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PerioStats {
    pub sites: usize,
    pub mean_depth: f32,
    pub pockets4: usize,
    pub pockets6: usize,
    pub bop_pct: f32,
    pub plaque_pct: f32,
    pub mobile_teeth: usize,
}

impl PerioChart {
    pub fn stats(&self) -> PerioStats {
        let present: Vec<&PerioTooth> = self.teeth.iter().filter(|t| !t.missing).collect();
        let sites = present.len() * 6;
        if sites == 0 {
            return PerioStats::default();
        }
        let all = || present.iter().flat_map(|t| t.depth.iter());
        let sum: u32 = all().map(|d| *d as u32).sum();
        let bop = present.iter().flat_map(|t| t.bleeding.iter()).filter(|b| **b).count();
        let plq = present.iter().flat_map(|t| t.plaque.iter()).filter(|b| **b).count();
        PerioStats {
            sites,
            mean_depth: sum as f32 / sites as f32,
            pockets4: all().filter(|d| **d >= 4).count(),
            pockets6: all().filter(|d| **d >= 6).count(),
            bop_pct: bop as f32 * 100.0 / sites as f32,
            plaque_pct: plq as f32 * 100.0 / sites as f32,
            mobile_teeth: present.iter().filter(|t| t.mobility > 0).count(),
        }
    }
}

pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

pub fn add_months(d: NaiveDate, months: u32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + months as i32;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    let mut day = d.day();
    loop {
        if let Some(r) = NaiveDate::from_ymd_opt(y, m, day) {
            return r;
        }
        day -= 1;
    }
}

pub fn money(v: f64) -> String {
    let neg = v < -0.004;
    let cents = (v.abs() * 100.0).round() as i64;
    let ent = cents / 100;
    let dec = cents % 100;
    let s = ent.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    format!("{}{},{:02} €", if neg { "-" } else { "" }, out, dec)
}

pub fn fmt_date(d: NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}

pub fn weekday_es(d: NaiveDate) -> &'static str {
    ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"][d.weekday().num_days_from_monday() as usize]
}

pub fn month_es(m: u32) -> &'static str {
    ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"][(m - 1) as usize]
}

pub fn long_date(d: NaiveDate) -> String {
    format!("{} {} de {} de {}", weekday_es(d), d.day(), month_es(d.month()), d.year())
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

/// Texto para una celda de un CSV o de un texto que se pega en una hoja de
/// cálculo: sin separadores ni saltos de línea, y sin que pueda tomarse por
/// una fórmula (lo que empieza por =, +, -, @ o un tabulador lleva delante un
/// apóstrofo). Los datos vienen de personas o de otros programas.
pub fn sheet_cell(s: &str, sep: char) -> String {
    let clean: String = s.chars().map(|c| if c == sep || c == '\n' || c == '\r' || c == '\t' { ' ' } else { c }).collect();
    if s.trim_start_matches(' ').starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{clean}") } else { clean }
}

/// Quita acentos y pasa a minúsculas para búsquedas tolerantes.
pub fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'Á' | 'À' => 'a',
            'é' | 'è' | 'É' | 'È' => 'e',
            'í' | 'Í' => 'i',
            'ó' | 'ò' | 'Ó' | 'Ò' => 'o',
            'ú' | 'ü' | 'Ú' | 'Ü' => 'u',
            'ñ' | 'Ñ' => 'n',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

impl Db {
    pub fn path() -> PathBuf {
        // las pruebas nunca tocan los datos ni las copias de este ordenador
        if cfg!(test) {
            return std::env::temp_dir().join(format!("veodardental-pruebas-{}", std::process::id())).join("clinica.json");
        }
        if crate::shared::shared() {
            return PathBuf::from(crate::shared::station().data_dir.trim()).join("clinica.json");
        }
        Self::local_path()
    }

    /// Los datos en este ordenador (sin carpeta compartida).
    pub fn local_path() -> PathBuf {
        if cfg!(test) {
            return std::env::temp_dir().join(format!("veodardental-pruebas-{}", std::process::id())).join("clinica.json");
        }
        crate::paths::data().join(crate::brand::DIR).join("clinica.json")
    }

    /// Completa los datos guardados con lo que traen las versiones nuevas del programa
    /// (por ahora, las plantillas de correo de recordatorio y de revisión). Devuelve si cambió algo.
    pub fn upgrade(&mut self) -> bool {
        if self.data_version >= DATA_VERSION {
            return false;
        }
        use crate::comms::{Channel, MsgKind};
        for t in crate::comms::default_msg_templates().into_iter().filter(|t| t.channel == Channel::Email && matches!(t.kind, MsgKind::RecordatorioCita | MsgKind::Recall)) {
            if !self.msg_templates.iter().any(|x| x.kind == t.kind && x.channel == Channel::Email) {
                let id = self.msg_templates.iter().map(|x| x.id).max().unwrap_or(0) + 1;
                self.msg_templates.push(crate::comms::MsgTemplate { id, ..t });
            }
        }
        self.data_version = DATA_VERSION;
        true
    }

    /// Escribe los datos enteros en `path` (en un fichero aparte que luego
    /// ocupa su lugar, para no dejarlo a medias).
    pub fn write_full(db: &Db, path: &std::path::Path) -> Result<(), String> {
        use std::io::Write;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        {
            let mut f = crate::paths::create_private(&tmp).map_err(|e| e.to_string())?;
            // cifrado si los datos de este ordenador se cifran (ver `datos_cifrados`)
            let bytes = crate::datos_cifrados::to_disk(serde_json::to_string(db).map_err(|e| e.to_string())?.as_bytes())?;
            f.write_all(&bytes).map_err(|e| e.to_string())?;
            // en el disco antes de ocupar el lugar del bueno
            f.sync_all().map_err(|e| e.to_string())?;
        }
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn load_or_sample() -> (Db, Option<String>) {
        Self::load_from(&Self::path(), true)
    }

    /// Lee los datos de `p` con lo que haya en su diario de cambios. Con
    /// `rewrite` (quien los tiene bloqueados), deja el fichero completo y
    /// quita el diario; sin él, solo los lee. Si no hay fichero, los datos de
    /// ejemplo; si no se puede leer, también, con el error.
    pub fn load_from(p: &std::path::Path, rewrite: bool) -> (Db, Option<String>) {
        let p = p.to_path_buf();
        // un cambio de cifrado cortado a medias se termina o se deshace (solo quien los bloquea)
        if rewrite && let Err(e) = crate::datos_cifrados::finish_conversion(&p) {
            return (Db::sample(today()), Some(format!("No se pudieron abrir los datos de {}: {e}", p.display())));
        }
        // cifrado sin la clave, tocado o sin cifrar cuando se cifra: es un
        // error (nunca «no hay datos»), y no se guarda encima
        let read = crate::datos_cifrados::read_text(&p);
        if let Err(e) = &read
            && p.exists()
            && e.kind() == std::io::ErrorKind::Other
        {
            return (Db::sample(today()), Some(format!("No se pudieron abrir los datos de {}: {e}", p.display())));
        }
        if let Ok(txt) = read {
            return match serde_json::from_str::<Db>(&txt) {
                Ok(mut db) => {
                    // lo apuntado en el diario y que no llegó a reescribirse (de momento, en memoria)
                    let replayed = crate::sync::replay_journal(&mut db, &p);
                    // de una versión más nueva del programa: se leen, pero no se guardan
                    // (al guardar se perdería lo que esta versión no conoce)
                    if db.data_version > DATA_VERSION {
                        db.reset_hidden_credentials();
                        let msg = format!("Estos datos son de una versión más nueva de VeodarDental (datos {}; este programa, {DATA_VERSION}). Se abren sin guardar: actualice el programa en este ordenador.", db.data_version);
                        return (db, Some(msg));
                    }
                    let mut err = replayed.as_ref().err().map(|e| format!("No se pudo leer el diario de cambios de {}: {e}", crate::sync::journal_path(&p).display()));
                    // de una versión anterior: copia de seguridad antes de completarlos con lo de esta
                    if err.is_none()
                        && rewrite
                        && db.data_version < DATA_VERSION
                        && let Err(e) = crate::tools::create_backup(&db, "antes de actualizar los datos")
                    {
                        err = Some(format!("No se pudo hacer la copia de seguridad previa a la actualización de los datos ({e}): se abren sin guardar."));
                    }
                    if err.is_none() && rewrite {
                        if replayed.as_ref().is_ok_and(|n| *n > 0) {
                            err = Db::write_full(&db, &p).err();
                        }
                        if err.is_none() {
                            let _ = std::fs::remove_file(crate::sync::journal_path(&p));
                        }
                    }
                    db.upgrade();
                    db.reset_hidden_credentials();
                    (db, err)
                }
                Err(e) => (Db::sample(today()), Some(format!("No se pudo leer {}: {e}. Se cargaron datos de ejemplo.", p.display()))),
            };
        }
        (Db::sample(today()), None)
    }

    pub fn new_id(&mut self) -> u32 {
        self.next_id += 1;
        // En red, cada puesto usa identificadores que terminan en su número
        // (módulo 16), así dos puestos nunca crean el mismo.
        if self.station > 0 {
            while self.next_id % 16 != self.station % 16 {
                self.next_id += 1;
            }
        }
        self.next_id
    }

    pub fn patient(&self, id: u32) -> Option<&Patient> {
        self.patients.iter().find(|p| p.id == id)
    }
    pub fn patient_mut(&mut self, id: u32) -> Option<&mut Patient> {
        self.patients.iter_mut().find(|p| p.id == id)
    }
    pub fn patient_name(&self, id: u32) -> String {
        self.patient(id).map(|p| p.full_name()).unwrap_or_else(|| "—".into())
    }
    pub fn appt_name(&self, a: &Appointment) -> String {
        match a.patient_id {
            Some(id) => self.patient_name(id),
            None if a.free_name.is_empty() => "(sin paciente)".into(),
            None => format!("{} (nuevo)", a.free_name),
        }
    }
    pub fn doctor(&self, id: u32) -> Option<&Collaborator> {
        self.collaborators.iter().find(|c| c.id == id)
    }
    pub fn doctor_alias(&self, id: u32) -> String {
        self.doctor(id).map(|c| c.alias.clone()).unwrap_or_else(|| "—".into())
    }
    pub fn tariff(&self, id: u32) -> Option<&Tariff> {
        self.tariffs.iter().find(|t| t.id == id)
    }
    pub fn tariff_name(&self, id: u32) -> String {
        self.tariff(id).map(|t| t.name.clone()).unwrap_or_else(|| "Privado".into())
    }
    pub fn treatment(&self, code: &str) -> Option<&Treatment> {
        self.treatments.iter().find(|t| t.code == code)
    }
    /// ¿Se ofrece este tratamiento a un paciente de esta tarifa?
    pub fn treatment_offered(&self, t: &Treatment, tariff_id: u32) -> bool {
        t.active && t.only_tariff.is_none_or(|x| x == tariff_id)
    }
    /// Tratamientos que se pueden elegir para un paciente de esta tarifa.
    pub fn treatments_for(&self, tariff_id: u32) -> impl Iterator<Item = &Treatment> {
        self.treatments.iter().filter(move |t| self.treatment_offered(t, tariff_id))
    }
    pub fn box_name(&self, id: u32) -> String {
        self.boxes.iter().find(|b| b.id == id).map(|b| b.name.clone()).unwrap_or_default()
    }

    /// Precio de un tratamiento según la tarifa (mutua) indicada.
    pub fn price_for(&self, code: &str, tariff_id: u32) -> f64 {
        let base = self.treatment(code).map(|t| t.price).unwrap_or(0.0);
        match self.tariff(tariff_id) {
            Some(t) => match t.prices.iter().find(|p| p.code == code) {
                Some(p) => p.price,
                None => round2(base * (1.0 - t.discount_pct / 100.0)),
            },
            None => base,
        }
    }

    pub fn next_history_no(&self) -> u32 {
        self.patients.iter().map(|p| p.history_no).max().unwrap_or(1000) + 1
    }

    /// Números de historia repetidos (dos puestos dieron de alta pacientes a
    /// la vez): los pacientes para los que `keep` es falso reciben el
    /// siguiente libre (el número nombra su carpeta de imágenes). Devuelve
    /// cuántos ha cambiado.
    pub fn fix_history_numbers(&mut self, keep: impl Fn(u32) -> bool) -> usize {
        let mut count: HashMap<u32, usize> = HashMap::new();
        for p in self.patients.iter() {
            *count.entry(p.history_no).or_default() += 1;
        }
        if count.values().all(|n| *n <= 1) {
            return 0;
        }
        let mut next = self.next_history_no();
        let mut fixed = 0;
        for p in self.patients.iter_mut() {
            if count.get(&p.history_no).copied().unwrap_or(0) > 1 && !keep(p.id) {
                *count.entry(p.history_no).or_default() -= 1;
                p.history_no = next;
                next += 1;
                fixed += 1;
            }
        }
        fixed
    }

    pub fn patient_debit(&self, pid: u32) -> f64 {
        round2(self.entries.iter().filter(|e| e.patient_id == pid && e.state == TxState::Realizado).map(|e| e.amount()).sum())
    }
    pub fn patient_paid(&self, pid: u32) -> f64 {
        round2(self.payments.iter().filter(|p| p.patient_id == pid).map(|p| p.amount).sum())
    }
    /// Positivo = el paciente debe; negativo = tiene saldo a favor (anticipo).
    pub fn patient_balance(&self, pid: u32) -> f64 {
        round2(self.patient_debit(pid) - self.patient_paid(pid))
    }
    pub fn patient_planned(&self, pid: u32) -> f64 {
        round2(self.entries.iter().filter(|e| e.patient_id == pid && e.state == TxState::Previsto).map(|e| e.amount()).sum())
    }
    pub fn last_visit(&self, pid: u32) -> Option<NaiveDate> {
        self.appointments.iter().filter(|a| a.patient_id == Some(pid) && a.state == ApptState::Finalizada).map(|a| a.date).max()
    }
    pub fn next_appt(&self, pid: u32, from: NaiveDate) -> Option<&Appointment> {
        self.appointments.iter().filter(|a| a.patient_id == Some(pid) && a.date >= from && !a.state.is_cancelled() && a.state != ApptState::Finalizada).min_by_key(|a| (a.date, a.start))
    }

    /// Caja abierta del centro activo.
    pub fn open_cash(&self) -> Option<&CashSession> {
        let c = cid(self.config.current_center);
        self.cash.iter().find(|s| s.state == CashState::Abierta && cid(s.center_id) == c)
    }

    /// Cajas abiertas del centro activo: más de una si dos puestos la abrieron a la vez.
    pub fn open_cashes(&self) -> Vec<&CashSession> {
        let c = cid(self.config.current_center);
        self.cash.iter().filter(|s| s.state == CashState::Abierta && cid(s.center_id) == c).collect()
    }

    /// Abre la caja del centro activo con ese fondo. Devuelve su id.
    pub fn open_cash_session(&mut self, opening: f64) -> u32 {
        let id = self.new_id();
        self.cash.push(CashSession {
            id,
            opened: now(),
            closed: None,
            opening_balance: round2(opening),
            moves: vec![],
            counted: None,
            state: CashState::Abierta,
            user: self.config.current_user.clone(),
            notes: String::new(),
            center_id: cid(self.config.current_center),
        });
        id
    }

    /// Cierra la caja con el efectivo contado. Devuelve el descuadre (contado − esperado).
    pub fn close_cash_session(&mut self, id: u32, counted: f64) -> f64 {
        let Some(expected) = self.cash.iter().find(|c| c.id == id).map(|c| self.cash_expected(c)) else { return 0.0 };
        let diff = round2(counted - expected);
        if let Some(c) = self.cash.iter_mut().find(|c| c.id == id) {
            c.counted = Some(round2(counted));
            c.closed = Some(now());
            c.state = CashState::Cerrada;
            c.notes = if diff.abs() < 0.005 { "Cuadrada".into() } else { format!("Descuadre {}", money(diff)) };
        }
        diff
    }

    /// Cobros nuevos (`is_new`) que han caído en una caja que otro puesto
    /// cerró mientras tanto: el arqueo ya está hecho sin ellos, así que pasan a
    /// la caja abierta de su centro (o a ninguna). Devuelve cuántos.
    pub fn move_late_payments(&mut self, is_new: impl Fn(u32) -> bool) -> usize {
        let closed: Vec<u32> = self.cash.iter().filter(|c| c.state == CashState::Cerrada).map(|c| c.id).collect();
        let open: Vec<(u32, u32)> = self.cash.iter().filter(|c| c.state == CashState::Abierta).map(|c| (cid(c.center_id), c.id)).collect();
        let mut moved = 0;
        for p in self.payments.iter_mut() {
            if p.cash_session.is_some_and(|s| closed.contains(&s)) && is_new(p.id) {
                p.cash_session = open.iter().find(|(c, _)| *c == cid(p.center_id)).map(|x| x.1);
                moved += 1;
            }
        }
        moved
    }

    pub fn session_payments(&self, sid: u32) -> impl Iterator<Item = &Payment> {
        self.payments.iter().filter(move |p| p.cash_session == Some(sid))
    }

    /// Efectivo que debería haber en el cajón.
    pub fn cash_expected(&self, s: &CashSession) -> f64 {
        let cash_pay: f64 = self.session_payments(s.id).filter(|p| p.method == "Efectivo").map(|p| p.amount).sum();
        round2(s.opening_balance + cash_pay + s.moves.iter().map(|m| m.amount).sum::<f64>())
    }

    /// Número del recibo siguiente. En red, cada puesto tiene su serie (R2-00001,
    /// R3-00001…): el recibo se imprime al cobrar, así que su número tiene que
    /// ser único ya, sin esperar al servidor. En un solo ordenador, el contador
    /// de la configuración.
    fn next_receipt(&mut self) -> String {
        let r = self.peek_receipt();
        if self.station == 0 {
            self.config.next_receipt += 1;
        }
        r
    }

    /// El número que llevará el próximo recibo (sin gastarlo).
    pub fn peek_receipt(&self) -> String {
        if self.station == 0 {
            return format!("{}-{:05}", self.config.receipt_series, self.config.next_receipt);
        }
        let prefix = format!("{}{}-", self.config.receipt_series, self.station);
        let n = self.payments.iter().filter_map(|p| p.receipt_no.strip_prefix(&prefix)?.parse::<u32>().ok()).max().unwrap_or(0) + 1;
        format!("{prefix}{n:05}")
    }

    /// Registra un cobro; numera el recibo y lo asocia a la caja abierta.
    pub fn add_payment(&mut self, pid: u32, amount: f64, method: &str, concept: &str) -> u32 {
        let id = self.new_id();
        let receipt = self.next_receipt();
        let cash = self.open_cash().map(|c| c.id);
        self.payments.push(Payment {
            id,
            patient_id: pid,
            date: now(),
            amount: round2(amount),
            method: method.to_string(),
            concept: concept.to_string(),
            receipt_no: receipt,
            user: self.config.current_user.clone(),
            cash_session: cash,
            center_id: cid(self.config.current_center),
            cancels: None,
            installment: None,
        });
        id
    }

    /// Añade una entrada médica con el precio de la tarifa del paciente.
    #[allow(clippy::too_many_arguments)]
    pub fn add_entry(&mut self, pid: u32, code: &str, teeth: Vec<u8>, faces: Vec<String>, state: TxState, doctor_id: u32, date: NaiveDate) -> u32 {
        let tariff = self.patient(pid).map(|p| p.tariff_id).unwrap_or(1);
        let disc = self.patient(pid).map(|p| p.discount_pct).unwrap_or(0.0);
        let (desc, odg, per_tooth) = self.treatment(code).map(|t| (t.description.clone(), t.odg, t.per_tooth)).unwrap_or((code.to_string(), OdgKind::Ninguno, false));
        let price = if state == TxState::Previo { 0.0 } else { self.price_for(code, tariff) };
        let units = if per_tooth { teeth.len().max(1) as u32 } else { 1 };
        let id = self.new_id();
        self.entries.push(MedEntry {
            id,
            patient_id: pid,
            date,
            code: code.to_string(),
            description: desc,
            teeth,
            faces,
            doctor_id,
            state,
            price,
            discount_pct: disc,
            units,
            budget_id: None,
            notes: String::new(),
            odg,
            invoice_id: None,
            tariff_id: tariff,
            center_id: self.config.current_center,
            settled_elsewhere: false,
        });
        if state == TxState::Realizado {
            self.after_realized(id);
        }
        id
    }

    /// Apunta un tratamiento desde el odontograma. Si se marca como hecho algo
    /// que ya estaba previsto en esa pieza (de un presupuesto, por ejemplo),
    /// completa el previsto en vez de crear otro. Devuelve el tratamiento y si
    /// era uno previsto.
    #[allow(clippy::too_many_arguments)]
    pub fn chart_entry(&mut self, pid: u32, code: &str, teeth: Vec<u8>, faces: Vec<String>, state: TxState, doctor_id: u32, date: NaiveDate) -> (u32, bool) {
        if state == TxState::Realizado {
            let mut want = teeth.clone();
            want.sort_unstable();
            let planned = self
                .entries
                .iter()
                .filter(|e| e.patient_id == pid && e.state == TxState::Previsto && e.code == code)
                .filter(|e| {
                    let mut t = e.teeth.clone();
                    t.sort_unstable();
                    t == want
                })
                // primero lo presupuestado y lo más antiguo
                .min_by_key(|e| (e.budget_id.is_none(), e.date, e.id))
                .map(|e| e.id);
            if let Some(id) = planned {
                if !faces.is_empty()
                    && let Some(e) = self.entries.iter_mut().find(|e| e.id == id)
                {
                    e.faces = faces;
                }
                self.realize_entry(id, date, doctor_id);
                return (id, true);
            }
        }
        (self.add_entry(pid, code, teeth, faces, state, doctor_id, date), false)
    }

    /// Código de obturación según el número de caras (0301 una, 0302 dos, 0303
    /// tres o más), solo si en el catálogo de la clínica los tres son
    /// obturaciones: con otro catálogo (importado) se deja el elegido.
    pub fn filling_code(&self, code: &str, faces: usize) -> String {
        const CODES: [&str; 3] = ["0301", "0302", "0303"];
        let filling = |c: &str| self.treatment(c).is_some_and(|t| t.odg == OdgKind::Obturacion);
        if !CODES.contains(&code) || !CODES.iter().all(|c| filling(c)) {
            return code.to_string();
        }
        CODES[faces.clamp(1, 3) - 1].to_string()
    }

    /// Marca una entrada como realizada y genera el recall si procede.
    pub fn realize_entry(&mut self, id: u32, date: NaiveDate, doctor_id: u32) {
        let pid = self.entries.iter().find(|e| e.id == id).map(|e| e.patient_id);
        let tariff = pid.and_then(|p| self.patient(p)).map(|p| p.tariff_id);
        let center = self.config.current_center;
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.state = TxState::Realizado;
            e.date = date;
            // donde se hace (un presupuesto se acepta en un centro y se hace en otro)
            e.center_id = center;
            // la compañía que cubre al paciente el día que se hace
            if let Some(t) = tariff {
                e.tariff_id = t;
            }
            if doctor_id != 0 {
                e.doctor_id = doctor_id;
            }
        }
        self.after_realized(id);
        // Si todas las líneas del presupuesto están hechas, se cierra.
        let bid = self.entries.iter().find(|e| e.id == id).and_then(|e| e.budget_id);
        if let Some(bid) = bid {
            let pending = self.entries.iter().any(|e| e.budget_id == Some(bid) && e.state == TxState::Previsto);
            if !pending
                && let Some(b) = self.budgets.iter_mut().find(|b| b.id == bid)
                && b.state == BudgetState::Aceptado
            {
                b.state = BudgetState::Cerrado;
            }
        }
    }

    fn after_realized(&mut self, id: u32) {
        self.consume_for_entry(id);
        let Some(e) = self.entries.iter().find(|e| e.id == id).cloned() else { return };
        let Some(t) = self.treatment(&e.code).cloned() else { return };
        if t.recall_months == 0 {
            return;
        }
        let reason = match t.odg {
            OdgKind::Implante | OdgKind::Corona if t.group == "Implantología" => "Control de implante",
            OdgKind::Bracket => "Control de ortodoncia",
            OdgKind::Higiene if t.group == "Periodoncia" => "Revisión periodontal",
            OdgKind::Higiene => "Higiene anual",
            _ if t.group == "Periodoncia" => "Revisión periodontal",
            _ => "Revisión semestral",
        };
        let due = add_months(e.date, t.recall_months);
        let existing = self.recalls.iter_mut().find(|r| r.patient_id == e.patient_id && r.reason == reason && r.state == RecallState::Pendiente);
        match existing {
            Some(r) => {
                if r.date < due {
                    r.date = due;
                }
            }
            None => {
                let rid = self.new_id();
                self.recalls.push(Recall {
                    id: rid,
                    patient_id: e.patient_id,
                    date: due,
                    reason: reason.into(),
                    doctor_id: e.doctor_id,
                    state: RecallState::Pendiente,
                    channel: String::new(),
                    comment: format!("Generado al realizar: {}", t.description),
                    contacted_on: None,
                    appt_id: None,
                });
            }
        }
    }

    /// Recall al que hay que llamar: pendiente y de un paciente que no está de baja.
    pub fn recall_due(&self, r: &Recall) -> bool {
        r.state == RecallState::Pendiente && self.patient(r.patient_id).is_some_and(|p| p.active)
    }

    /// Pone los recalls al día con las citas. El pendiente de un paciente que
    /// ya tiene cita (en los 30 días siguientes a la fecha del recall, o antes)
    /// pasa a «Citado» con esa cita; el citado cuya cita se anula, se aplaza, se
    /// borra o a la que no acude, pasa a la siguiente cita del paciente o, si no
    /// tiene, vuelve a «Pendiente» para volver a llamarle. Devuelve cuántos cambia.
    pub fn refresh_recalls(&mut self, today: NaiveDate) -> usize {
        use std::collections::HashMap;
        let mut upcoming: HashMap<u32, Vec<(NaiveDate, NaiveTime, u32)>> = HashMap::new();
        let mut known: HashMap<u32, (NaiveDate, ApptState)> = HashMap::new();
        for a in self.appointments.iter() {
            known.insert(a.id, (a.date, a.state));
            if let Some(p) = a.patient_id
                && a.date >= today
                && !a.state.is_cancelled()
            {
                upcoming.entry(p).or_default().push((a.date, a.start, a.id));
            }
        }
        for v in upcoming.values_mut() {
            v.sort();
        }
        let next = |pid: u32, due: NaiveDate| upcoming.get(&pid).and_then(|v| v.iter().find(|(d, _, _)| due <= *d + Duration::days(30))).map(|(d, _, id)| (*d, *id));
        let mut n = 0;
        for r in self.recalls.iter_mut() {
            match (r.state, r.appt_id) {
                (RecallState::Pendiente, _) => {
                    if let Some((d, id)) = next(r.patient_id, r.date) {
                        r.state = RecallState::Citado;
                        r.appt_id = Some(id);
                        r.contacted_on.get_or_insert(today);
                        r.comment = format!("Cita el {}", fmt_date(d));
                        n += 1;
                    }
                }
                (RecallState::Citado, Some(aid)) => {
                    let gone = known.get(&aid).copied();
                    if gone.is_some_and(|(_, s)| !s.is_cancelled()) {
                        continue;
                    }
                    if let Some((d, id)) = next(r.patient_id, r.date) {
                        r.appt_id = Some(id);
                        r.comment = format!("Cita el {}", fmt_date(d));
                    } else {
                        r.state = RecallState::Pendiente;
                        r.appt_id = None;
                        r.comment = match gone {
                            Some((d, ApptState::NoAcude)) => format!("No acudió a la cita del {}", fmt_date(d)),
                            Some((d, ApptState::Reprogramada)) => format!("Se aplazó la cita del {} sin fecha nueva", fmt_date(d)),
                            Some((d, _)) => format!("Anuló la cita del {}", fmt_date(d)),
                            None => "Se borró su cita".into(),
                        };
                    }
                    n += 1;
                }
                _ => {}
            }
        }
        n
    }

    /// Acepta un presupuesto: pasa sus líneas al historial como "Previsto".
    pub fn accept_budget(&mut self, bid: u32, date: NaiveDate) {
        let Some(b) = self.budgets.iter().find(|b| b.id == bid).cloned() else { return };
        if b.state == BudgetState::Aceptado || b.state == BudgetState::Cerrado {
            return;
        }
        // cada línea, su tratamiento: dos líneas iguales (dos limpiezas) no se
        // quedan en uno
        let mut used: Vec<u32> = vec![];
        for l in &b.lines {
            let linked = self.entries.iter_mut().find(|e| e.patient_id == b.patient_id && e.state == TxState::Previsto && e.code == l.code && e.teeth == l.teeth && (e.budget_id.is_none() || e.budget_id == Some(bid)) && !used.contains(&e.id));
            if let Some(e) = linked {
                e.budget_id = Some(bid);
                e.price = l.price;
                e.units = l.units;
                e.discount_pct = l.discount_pct;
                used.push(e.id);
                continue;
            }
            let id = self.new_id();
            used.push(id);
            self.entries.push(MedEntry {
                id,
                patient_id: b.patient_id,
                date,
                code: l.code.clone(),
                description: l.description.clone(),
                teeth: l.teeth.clone(),
                faces: vec![],
                doctor_id: b.doctor_id,
                state: TxState::Previsto,
                price: l.price,
                discount_pct: l.discount_pct,
                units: l.units,
                budget_id: Some(bid),
                notes: String::new(),
                odg: l.odg,
                invoice_id: None,
                tariff_id: b.tariff_id,
                center_id: self.config.current_center,
                settled_elsewhere: false,
            });
        }
        // Recall de seguimiento cancelado al aceptar.
        for r in self.recalls.iter_mut() {
            if r.patient_id == b.patient_id && r.reason == "Seguimiento de presupuesto" && r.state == RecallState::Pendiente {
                r.state = RecallState::Citado;
                r.comment = "Presupuesto aceptado".into();
            }
        }
        if let Some(bm) = self.budgets.iter_mut().find(|x| x.id == bid) {
            bm.state = BudgetState::Aceptado;
            bm.accepted_on = Some(date);
            bm.rejected_on = None;
        }
    }

    pub fn reject_budget(&mut self, bid: u32, reason: &str) {
        if let Some(b) = self.budgets.iter_mut().find(|b| b.id == bid) {
            b.state = BudgetState::Rechazado;
            b.rejected_on = Some(today());
            b.reject_reason = reason.to_string();
        }
        for e in self.entries.iter_mut() {
            if e.budget_id == Some(bid) && e.state == TxState::Previsto {
                e.budget_id = None;
            }
        }
    }

    /// Crea un presupuesto con los tratamientos previstos sin presupuestar.
    pub fn budget_from_planned(&mut self, pid: u32) -> Option<u32> {
        let planned: Vec<MedEntry> = self.entries.iter().filter(|e| e.patient_id == pid && e.state == TxState::Previsto && e.budget_id.is_none()).cloned().collect();
        if planned.is_empty() {
            return None;
        }
        let p = self.patient(pid)?.clone();
        let id = self.new_id();
        let lines = planned.iter().map(|e| BudgetLine { code: e.code.clone(), description: e.description.clone(), teeth: e.teeth.clone(), units: e.units, price: e.price, discount_pct: e.discount_pct, odg: e.odg }).collect();
        self.budgets.push(Budget {
            id,
            patient_id: pid,
            title: "Plan de tratamiento".into(),
            date: today(),
            doctor_id: p.doctor_id,
            tariff_id: p.tariff_id,
            state: BudgetState::Pendiente,
            lines,
            valid_days: self.config.budget_valid_days,
            accepted_on: None,
            rejected_on: None,
            reject_reason: String::new(),
            notes: String::new(),
            plan: None,
        });
        for e in self.entries.iter_mut() {
            if planned.iter().any(|p| p.id == e.id) {
                e.budget_id = Some(id);
            }
        }
        Some(id)
    }

    /// Huecos libres de `minutes` para un doctor a partir de una fecha.
    pub fn find_gaps(&self, doctor_id: u32, from: NaiveDate, minutes: u32, earliest: NaiveTime, latest: NaiveTime, days: u32) -> Vec<(NaiveDate, NaiveTime)> {
        let mut out = vec![];
        let step = self.config.slot_minutes.max(5) as i64;
        for d in 0..days as i64 {
            let date = from + Duration::days(d);
            // en los tramos en que el profesional pasa consulta ese día (ninguno si es festivo)
            for shift in self.doctor_day(doctor_id, date) {
                let mut t = earliest.max(shift.start);
                let close = latest.min(shift.end);
                let mut guard = 0;
                while t + Duration::minutes(minutes as i64) <= close && guard < 400 {
                    guard += 1;
                    let end = t + Duration::minutes(minutes as i64);
                    if self.is_free(doctor_id, date, t, end, None) {
                        out.push((date, t));
                        if out.len() >= 12 {
                            return out;
                        }
                        t = end;
                        continue;
                    }
                    t += Duration::minutes(step);
                }
            }
        }
        out
    }

    /// El profesional está libre (sin otra cita ni bloqueo) y pasa consulta a esa hora.
    pub fn is_free(&self, doctor_id: u32, date: NaiveDate, s: NaiveTime, e: NaiveTime, ignore: Option<u32>) -> bool {
        !self.overlaps(doctor_id, date, s, e, ignore) && self.works_at(doctor_id, date, s, e)
    }

    /// El profesional ya tiene otra cita (o un bloqueo) que se cruza con esa hora.
    pub fn overlaps(&self, doctor_id: u32, date: NaiveDate, s: NaiveTime, e: NaiveTime, ignore: Option<u32>) -> bool {
        let busy = self.appointments.iter().any(|a| Some(a.id) != ignore && a.doctor_id == doctor_id && a.date == date && !a.state.is_cancelled() && a.start < e && s < a.end());
        let blocked = self.blocks.iter().any(|b| b.doctor_id == doctor_id && b.date.map(|d| d == date).unwrap_or(true) && b.start < e && s < b.end);
        busy || blocked
    }

    // ── facturación ──

    pub fn uninvoiced(&self, pid: u32) -> Vec<&MedEntry> {
        self.entries.iter().filter(|e| e.patient_id == pid && e.state == TxState::Realizado && e.invoice_id.is_none() && !e.settled_elsewhere && e.amount() > 0.0).collect()
    }

    pub fn invoice(&self, id: u32) -> Option<&Invoice> {
        self.invoices.iter().find(|i| i.id == id)
    }

    fn next_number(&self, series: &str) -> u32 {
        self.invoices.iter().filter(|i| i.series == series).map(|i| i.number).max().unwrap_or(0) + 1
    }

    /// Huella del último registro de la cadena (facturas, subsanaciones y anulaciones).
    fn last_hash(&self) -> String {
        crate::verifactu::last_hash(self)
    }

    fn seal(&mut self, mut inv: Invoice) -> u32 {
        inv.prev_hash = self.last_hash();
        inv.hash = sha256_hex(&inv.canonical(&self.config.nif));
        let id = inv.id;
        self.invoices.push(inv);
        id
    }

    /// Emite una factura con los tratamientos realizados indicados.
    /// La numeración es correlativa por serie y año, sin huecos.
    pub fn issue_invoice(&mut self, pid: u32, entry_ids: &[u32], date: NaiveDate) -> Result<u32, String> {
        let p = self.patient(pid).cloned().ok_or("Paciente no encontrado")?;
        let entries: Vec<MedEntry> = self.entries.iter().filter(|e| entry_ids.contains(&e.id) && e.patient_id == pid && e.state == TxState::Realizado && e.invoice_id.is_none() && !e.settled_elsewhere).cloned().collect();
        if entries.is_empty() {
            return Err("No hay tratamientos realizados pendientes de facturar.".into());
        }
        if let Some(last) = self.invoices.iter().filter(|i| i.series == format!("F{}", date.year())).map(|i| i.date).max()
            && date < last
        {
            return Err(format!("La fecha no puede ser anterior a la última factura de la serie ({}).", fmt_date(last)));
        }
        // a nombre del responsable (el padre o la madre de un menor…) si lo tiene
        let payer = (!p.payer.is_empty()).then(|| p.payer.clone());
        let (client_name, client_nif, client_address) = match &payer {
            Some(x) => (x.name.trim().to_string(), x.nif.trim().to_string(), if x.address.trim().is_empty() { format!("{}, {} {}", p.address, p.zip, p.city) } else { x.address.trim().to_string() }),
            None => (p.full_name(), p.nif.clone(), format!("{}, {} {}", p.address, p.zip, p.city)),
        };
        if client_nif.trim().is_empty() && entries.iter().map(|e| e.amount()).sum::<f64>() > 400.0 {
            return Err(if payer.is_some() {
                "Para facturas de más de 400 € hace falta el NIF de la persona a cuyo nombre va la factura (ficha del paciente › Filiación).".into()
            } else {
                "Para facturas de más de 400 € hace falta un NIF: el del paciente o, si es menor, el de su padre, madre o tutor (ficha del paciente › Filiación › Factura a nombre de).".into()
            });
        }
        let series = format!("F{}", date.year());
        let id = self.new_id();
        let stamp = Local::now();
        let inv = Invoice {
            id,
            number: self.next_number(&series),
            series,
            date,
            created: stamp.naive_local(),
            patient_id: pid,
            patient_name: if payer.is_some() { p.full_name() } else { String::new() },
            // sin NIF, factura simplificada
            tipo: if client_nif.trim().is_empty() { "F2".into() } else { "F1".into() },
            client_name,
            client_nif,
            client_address,
            lines: entries.iter().map(|e| InvoiceLine { code: e.code.clone(), description: e.description.clone(), teeth: e.teeth.clone(), units: e.units, price: e.price, discount_pct: e.discount_pct, vat_pct: 0.0 }).collect(),
            rectifies: None,
            rectified_by: None,
            reason: String::new(),
            entry_ids: entries.iter().map(|e| e.id).collect(),
            prev_hash: String::new(),
            hash: String::new(),
            user: self.config.current_user.clone(),
            gen_ts: stamp.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
            issuer_nif: self.config.nif.clone(),
            aeat: Default::default(),
            exemption: Some(self.config.vat_exemption),
        };
        let id = self.seal(inv);
        for e in self.entries.iter_mut() {
            if entry_ids.contains(&e.id) && e.invoice_id.is_none() && !e.settled_elsewhere && e.patient_id == pid && e.state == TxState::Realizado {
                e.invoice_id = Some(id);
            }
        }
        Ok(id)
    }

    /// Rectificativa por diferencias (anula la factura): importes en negativo.
    /// Los tratamientos quedan libres para volver a facturarse.
    pub fn rectify_invoice(&mut self, id: u32, reason: &str) -> Result<u32, String> {
        let orig = self.invoice(id).cloned().ok_or("Factura no encontrada")?;
        if orig.rectifies.is_some() {
            return Err("No se puede rectificar una factura rectificativa.".into());
        }
        if orig.rectified_by.is_some() {
            return Err("Esta factura ya está rectificada.".into());
        }
        if reason.trim().is_empty() {
            return Err("Indique el motivo de la rectificación.".into());
        }
        let date = today().max(orig.date);
        let series = format!("R{}", date.year());
        let rid = self.new_id();
        let stamp = Local::now();
        let inv = Invoice {
            id: rid,
            number: self.next_number(&series),
            series,
            date,
            created: stamp.naive_local(),
            lines: orig.lines.iter().map(|l| InvoiceLine { price: -l.price, ..l.clone() }).collect(),
            rectifies: Some(id),
            rectified_by: None,
            reason: reason.to_string(),
            entry_ids: vec![],
            prev_hash: String::new(),
            hash: String::new(),
            user: self.config.current_user.clone(),
            gen_ts: stamp.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
            // rectificativa de una simplificada: R5
            tipo: if orig.kind() == "F2" { "R5".into() } else { "R1".into() },
            issuer_nif: self.config.nif.clone(),
            aeat: Default::default(),
            ..orig.clone()
        };
        let rid = self.seal(inv);
        if let Some(o) = self.invoices.iter_mut().find(|i| i.id == id) {
            o.rectified_by = Some(rid);
        }
        for e in self.entries.iter_mut() {
            if e.invoice_id == Some(id) {
                e.invoice_id = None;
            }
        }
        Ok(rid)
    }

    /// Comprueba que ningún registro de facturación se ha alterado.
    /// Devuelve el número de registros correctos o el primero que falla.
    /// La cadena incluye las facturas y los registros posteriores (subsanaciones y anulaciones).
    pub fn verify_chain(&self) -> Result<usize, String> {
        crate::verifactu::verify(self)
    }

    pub fn patient_perio(&self, pid: u32) -> Vec<&PerioChart> {
        let mut v: Vec<&PerioChart> = self.perio.iter().filter(|c| c.patient_id == pid).collect();
        v.sort_by_key(|c| (c.date, c.id));
        v
    }

    /// Nuevo periodontograma: copia el anterior (si lo hay) o empieza en blanco,
    /// marcando como ausentes las piezas extraídas o ausentes del odontograma.
    pub fn new_perio(&mut self, pid: u32, doctor_id: u32) -> u32 {
        let gone: Vec<u8> = self.entries.iter().filter(|e| e.patient_id == pid && matches!(e.odg, OdgKind::Ausente | OdgKind::Extraccion) && e.state != TxState::Previsto).flat_map(|e| e.teeth.clone()).collect();
        let prev = self.patient_perio(pid).last().map(|c| c.teeth.clone());
        let teeth = match prev {
            Some(t) => t,
            None => {
                crate::screens::odontograma::UPPER_ADULT.iter().chain(crate::screens::odontograma::LOWER_ADULT.iter()).map(|&t| PerioTooth { tooth: t, depth: [2; 6], recession: [0; 6], missing: gone.contains(&t), ..Default::default() }).collect()
            }
        };
        let id = self.new_id();
        self.perio.push(PerioChart { id, patient_id: pid, date: today(), doctor_id, teeth, notes: String::new() });
        id
    }
}
