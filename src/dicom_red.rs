//! DICOM en red con los aparatos (TAC, ortopantomógrafo, sensores con red):
//! - **Lista de trabajo** (Modality Worklist, C-FIND): el aparato pide los
//!   pacientes citados y nadie tiene que teclearlos en él.
//! - **Recepción de imágenes** (Storage, C-STORE): el aparato manda lo que
//!   captura. Se guarda en la carpeta de entrada, una carpeta por serie, y de
//!   ahí va a la ficha por el número de historia y el nombre que lleva (ver `entrada`).
//! - **Verificación** (C-ECHO), para que el técnico pruebe la conexión.
//!
//! Lo atiende un solo ordenador de la clínica, el que se elija, en el puerto y
//! con el nombre DICOM (AE Title) que se configuren. DICOM no cifra ni pide
//! contraseña, así que:
//! - **Solo los aparatos de la lista:** cada uno con su nombre DICOM y, mejor,
//!   su dirección IP. Sin lista no se atiende a nadie (el servicio ni arranca).
//! - **Solo desde la red de la clínica:** direcciones privadas.
//! - **La lista de trabajo,** solo del día que se pide (hoy, si no se dice).
//! - **Lo que manda un aparato** se escribe en disco según llega (en memoria
//!   no se guarda una imagen entera), con un tope, y nunca pisa una imagen que
//!   ya estaba.
//!
//! Los nombres van en mayúsculas y sin tildes (sin juego de caracteres): así
//! los entienden también los aparatos antiguos.

use crate::model::{ApptState, Db, fold};
use chrono::NaiveDate;
use dicom_core::{DataElement, PrimitiveValue, VR, value::DataSetSequence};
use dicom_dictionary_std::{tags, uids};
use dicom_encoding::TransferSyntaxIndex;
use dicom_object::{FileMetaTableBuilder, InMemDicomObject};
use dicom_transfer_syntax_registry::TransferSyntaxRegistry;
use dicom_ul::{Pdu, pdu::PDataValue, pdu::PDataValueType};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Lo más que ocupa una imagen recibida (un TAC multicorte grande cabe).
const MAX_INSTANCE: u64 = 2 << 30;
/// Lo más que ocupa un comando o una pregunta a la lista de trabajo.
const MAX_SMALL: usize = 64 << 10;
/// Días que puede abarcar una pregunta a la lista de trabajo.
const MAX_DAYS: i64 = 7;
/// Conexiones a la vez como mucho.
const MAX_CONNECTIONS: usize = 8;

/// Configuración de la red DICOM de la clínica.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    /// El ordenador que atiende a los aparatos (su nombre de puesto).
    pub machine: String,
    pub port: u16,
    /// Nombre DICOM (AE Title) de VeodarDental, el que se pone en los aparatos.
    pub ae_title: String,
    /// Los aparatos admitidos, uno por línea: su nombre DICOM (AE Title) y,
    /// detrás, su dirección IP («CBCT1 192.168.1.50»). Sin ninguno no se atiende a nadie.
    pub allowed: String,
    /// Carpeta donde se guarda lo recibido (vacío: la de los datos de este
    /// ordenador). Mejor una compartida, para que los demás puestos lo abran.
    pub folder: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { enabled: false, machine: String::new(), port: 11112, ae_title: "VEODARDENTAL".into(), allowed: String::new(), folder: String::new() }
    }
}

impl Settings {
    pub fn is_default(&self) -> bool {
        *self == Settings::default()
    }

    /// Los aparatos de la lista: (nombre DICOM, dirección IP si se dio).
    pub fn devices(&self) -> Vec<(String, Option<std::net::IpAddr>)> {
        self.allowed
            .lines()
            .flat_map(|l| l.split([',', ';']))
            .filter_map(|entry| {
                let mut w = entry.split_whitespace();
                let ae = w.next()?.trim().to_uppercase();
                Some((ae, w.next().and_then(|ip| ip.parse().ok())))
            })
            .collect()
    }

    /// ¿Es de la lista este aparato, con este nombre DICOM y desde esta dirección?
    fn allows(&self, ae: &str, ip: Option<std::net::IpAddr>) -> bool {
        let ae = ae.trim().to_uppercase();
        self.devices().iter().any(|(a, want)| *a == ae && want.is_none_or(|w| Some(w) == ip))
    }
}

/// ¿Viene de la red de la clínica (direcciones privadas, o este mismo ordenador)?
fn local_network(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v) => v.is_private() || v.is_loopback() || v.is_link_local(),
        std::net::IpAddr::V6(v) => v.is_loopback() || (v.segments()[0] & 0xfe00) == 0xfc00 || (v.segments()[0] & 0xffc0) == 0xfe80 || v.to_ipv4_mapped().is_some_and(|m| m.is_private() || m.is_loopback()),
    }
}

/// Un paciente citado, tal como lo ve el aparato.
#[derive(Clone, Debug, PartialEq)]
pub struct WlItem {
    /// Número de la cita (Accession Number).
    pub accession: String,
    /// Número de historia (Patient ID).
    pub patient_id: String,
    /// APELLIDOS^NOMBRE, en mayúsculas y sin tildes.
    pub name: String,
    pub birth: String,
    pub sex: String,
    pub date: String,
    pub time: String,
    pub description: String,
    pub doctor: String,
    pub study_uid: String,
}

/// Mayúsculas sin tildes ni caracteres que DICOM use como separadores.
fn plain(s: &str) -> String {
    fold(s).to_uppercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '.' || c == '\'' { c } else { ' ' }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Un identificador de estudio para una cita, siempre el mismo (raíz 2.25,
/// la de los identificadores sin organización: el número de un hash).
fn study_uid(appt: u32, date: NaiveDate) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(format!("veodardental-cita-{appt}-{date}").as_bytes());
    let n = u128::from_be_bytes(h[..16].try_into().unwrap_or([0; 16])) >> 1;
    format!("2.25.{n}")
}

/// Los pacientes citados entre dos fechas (las citas anuladas, aplazadas y
/// las de quien no vino no cuentan).
pub fn worklist(db: &Db, from: NaiveDate, to: NaiveDate) -> Vec<WlItem> {
    let mut v: Vec<WlItem> = db
        .appointments
        .iter()
        .filter(|a| a.date >= from && a.date <= to && !matches!(a.state, ApptState::Anulada | ApptState::Reprogramada | ApptState::NoAcude))
        .filter_map(|a| {
            let p = db.patient(a.patient_id?)?;
            Some(WlItem {
                accession: a.id.to_string(),
                patient_id: p.history_no.to_string(),
                name: format!("{}^{}", plain(&p.last_name), plain(&p.first_name)),
                birth: p.birth.map(|d| d.format("%Y%m%d").to_string()).unwrap_or_default(),
                sex: match p.sex.as_str() {
                    "F" => "F",
                    "M" => "M",
                    _ => "O",
                }
                .into(),
                date: a.date.format("%Y%m%d").to_string(),
                time: a.start.format("%H%M%S").to_string(),
                description: plain(&a.reason),
                doctor: db.doctor(a.doctor_id).map(|d| plain(&d.name)).unwrap_or_default(),
                study_uid: study_uid(a.id, a.date),
            })
        })
        .collect();
    v.sort_by(|a, b| (&a.date, &a.time).cmp(&(&b.date, &b.time)));
    v
}

/// Lo que pregunta el aparato.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    pub patient_id: String,
    pub name: String,
    /// AAAAMMDD, o un intervalo (AAAAMMDD-AAAAMMDD, -AAAAMMDD, AAAAMMDD-).
    pub date: String,
    pub modality: String,
    pub station: String,
}

/// Comodines de DICOM: * (cualquier cosa) y ? (un carácter), sin distinguir
/// mayúsculas ni tildes, y mirando solo letras y cifras (sin «^» ni espacios).
fn wildcard(pattern: &str, value: &str) -> bool {
    let norm = |s: &str, wild: bool| -> Vec<char> { fold(s).to_uppercase().chars().filter(|c| c.is_ascii_alphanumeric() || (wild && (*c == '*' || *c == '?'))).collect() };
    fn go(p: &[char], v: &[char]) -> bool {
        match (p.first(), v.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], v) || (!v.is_empty() && go(p, &v[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &v[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &v[1..]),
            _ => false,
        }
    }
    go(&norm(pattern, true), &norm(value, false))
}

impl Query {
    /// La pregunta con las fechas acotadas: sin fecha, hoy; un intervalo, como
    /// mucho de [`MAX_DAYS`] días desde su principio.
    pub fn bounded(mut self, today: NaiveDate) -> Query {
        let day = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y%m%d").ok();
        let fmt = |d: NaiveDate| d.format("%Y%m%d").to_string();
        self.date = match self.date.trim().split_once('-') {
            None if self.date.trim().is_empty() => fmt(today),
            None => self.date.trim().to_string(),
            Some((a, b)) => {
                let from = day(a).or_else(|| day(b).map(|e| e - chrono::Duration::days(MAX_DAYS))).unwrap_or(today);
                let to = day(b).unwrap_or(from + chrono::Duration::days(MAX_DAYS)).min(from + chrono::Duration::days(MAX_DAYS));
                format!("{}-{}", fmt(from), fmt(to))
            }
        };
        self
    }

    pub fn matches(&self, it: &WlItem) -> bool {
        let id_ok = self.patient_id.trim().is_empty() || self.patient_id.trim() == it.patient_id;
        let name_ok = {
            let q = self.name.replace('^', "");
            q.trim().is_empty() || q.trim() == "*" || wildcard(&self.name.replace('^', "*"), &it.name.replace('^', ""))
        };
        let date_ok = match self.date.trim().split_once('-') {
            None if self.date.trim().is_empty() => true,
            None => self.date.trim() == it.date,
            Some((a, b)) => (a.is_empty() || it.date.as_str() >= a) && (b.is_empty() || it.date.as_str() <= b),
        };
        id_ok && name_ok && date_ok
    }
}

fn s(v: &str) -> PrimitiveValue {
    PrimitiveValue::from(v.to_string())
}

/// Un texto del objeto DICOM (vacío si no está).
fn text(obj: &InMemDicomObject, tag: dicom_core::Tag) -> String {
    obj.element_opt(tag).ok().flatten().and_then(|e| e.to_str().ok()).map(|v| v.trim().to_string()).unwrap_or_default()
}

/// La pregunta que llega en un C-FIND de la lista de trabajo.
pub fn query_of(identifier: &InMemDicomObject) -> Query {
    let mut q = Query { patient_id: text(identifier, tags::PATIENT_ID), name: text(identifier, tags::PATIENT_NAME), ..Default::default() };
    if let Some(item) = identifier.element_opt(tags::SCHEDULED_PROCEDURE_STEP_SEQUENCE).ok().flatten().and_then(|e| e.items()).and_then(|i| i.first()) {
        q.date = text(item, tags::SCHEDULED_PROCEDURE_STEP_START_DATE);
        q.modality = text(item, tags::MODALITY);
        q.station = text(item, tags::SCHEDULED_STATION_AE_TITLE);
    }
    q
}

/// La respuesta de la lista de trabajo para un paciente citado.
pub fn answer(it: &WlItem, q: &Query) -> InMemDicomObject {
    let step = InMemDicomObject::from_element_iter([
        DataElement::new(tags::MODALITY, VR::CS, s(if q.modality.is_empty() { "OT" } else { &q.modality })),
        DataElement::new(tags::SCHEDULED_STATION_AE_TITLE, VR::AE, s(&q.station)),
        DataElement::new(tags::SCHEDULED_PROCEDURE_STEP_START_DATE, VR::DA, s(&it.date)),
        DataElement::new(tags::SCHEDULED_PROCEDURE_STEP_START_TIME, VR::TM, s(&it.time)),
        DataElement::new(tags::SCHEDULED_PERFORMING_PHYSICIAN_NAME, VR::PN, s(&it.doctor)),
        DataElement::new(tags::SCHEDULED_PROCEDURE_STEP_DESCRIPTION, VR::LO, s(&it.description)),
        DataElement::new(tags::SCHEDULED_PROCEDURE_STEP_ID, VR::SH, s(&it.accession)),
    ]);
    InMemDicomObject::from_element_iter([
        DataElement::new(tags::ACCESSION_NUMBER, VR::SH, s(&it.accession)),
        DataElement::new(tags::PATIENT_NAME, VR::PN, s(&it.name)),
        DataElement::new(tags::PATIENT_ID, VR::LO, s(&it.patient_id)),
        DataElement::new(tags::PATIENT_BIRTH_DATE, VR::DA, s(&it.birth)),
        DataElement::new(tags::PATIENT_SEX, VR::CS, s(&it.sex)),
        DataElement::new(tags::STUDY_INSTANCE_UID, VR::UI, s(&it.study_uid)),
        DataElement::new(tags::REQUESTED_PROCEDURE_DESCRIPTION, VR::LO, s(&it.description)),
        DataElement::new(tags::SCHEDULED_PROCEDURE_STEP_SEQUENCE, VR::SQ, DataSetSequence::from(vec![step])),
        DataElement::new(tags::REQUESTED_PROCEDURE_ID, VR::SH, s(&it.accession)),
    ])
}

/// Clases de imagen que se reciben: las de los aparatos dentales y las genéricas.
const STORAGE: [&str; 15] = [
    uids::CT_IMAGE_STORAGE,
    uids::ENHANCED_CT_IMAGE_STORAGE,
    uids::DIGITAL_X_RAY_IMAGE_STORAGE_FOR_PRESENTATION,
    uids::DIGITAL_X_RAY_IMAGE_STORAGE_FOR_PROCESSING,
    uids::DIGITAL_INTRA_ORAL_X_RAY_IMAGE_STORAGE_FOR_PRESENTATION,
    uids::DIGITAL_INTRA_ORAL_X_RAY_IMAGE_STORAGE_FOR_PROCESSING,
    uids::COMPUTED_RADIOGRAPHY_IMAGE_STORAGE,
    uids::SECONDARY_CAPTURE_IMAGE_STORAGE,
    uids::MULTI_FRAME_GRAYSCALE_BYTE_SECONDARY_CAPTURE_IMAGE_STORAGE,
    uids::MULTI_FRAME_GRAYSCALE_WORD_SECONDARY_CAPTURE_IMAGE_STORAGE,
    uids::MULTI_FRAME_TRUE_COLOR_SECONDARY_CAPTURE_IMAGE_STORAGE,
    uids::VL_PHOTOGRAPHIC_IMAGE_STORAGE,
    uids::ENCAPSULATED_PDF_STORAGE,
    uids::ENCAPSULATED_STL_STORAGE,
    uids::X_RAY3_D_CRANIOFACIAL_IMAGE_STORAGE,
];

/// Un identificador DICOM como nombre de fichero: solo cifras y puntos.
fn uid_name(uid: &str) -> Option<String> {
    let u = uid.trim().trim_end_matches('\0');
    (!u.is_empty() && u.len() <= 64 && u.chars().all(|c| c.is_ascii_digit() || c == '.') && !u.starts_with('.')).then(|| u.to_string())
}

/// Un comando DIMSE (en «implicit VR little endian», como todos).
fn command(elements: Vec<DataElement<InMemDicomObject>>) -> Vec<u8> {
    let obj = InMemDicomObject::command_from_element_iter(elements);
    let mut out = vec![];
    let ts = dicom_transfer_syntax_registry::entries::IMPLICIT_VR_LITTLE_ENDIAN.erased();
    let _ = obj.write_dataset_with_ts(&mut out, &ts);
    out
}

fn us(v: u16) -> PrimitiveValue {
    PrimitiveValue::from(v)
}

fn response(field: u16, msg_id: u16, class: &str, instance: Option<&str>, status: u16, with_data: bool) -> Vec<u8> {
    let mut e = vec![
        DataElement::new(tags::AFFECTED_SOP_CLASS_UID, VR::UI, s(class)),
        DataElement::new(tags::COMMAND_FIELD, VR::US, us(field)),
        DataElement::new(tags::MESSAGE_ID_BEING_RESPONDED_TO, VR::US, us(msg_id)),
        DataElement::new(tags::COMMAND_DATA_SET_TYPE, VR::US, us(if with_data { 0x0102 } else { 0x0101 })),
        DataElement::new(tags::STATUS, VR::US, us(status)),
    ];
    if let Some(i) = instance {
        e.push(DataElement::new(tags::AFFECTED_SOP_INSTANCE_UID, VR::UI, s(i)));
    }
    command(e)
}

fn pdata(pc: u8, kind: PDataValueType, data: Vec<u8>) -> Pdu {
    Pdu::PData { data: vec![PDataValue { presentation_context_id: pc, value_type: kind, is_last: true, data }] }
}

/// Guarda una imagen recibida (sus datos, tal como llegaron, están en `part`)
/// como fichero DICOM en la carpeta de entrada: una carpeta por serie y un
/// fichero por imagen. Se escribe aparte y se mueve al terminar (la búsqueda
/// nunca ve uno a medias), y una imagen que ya estaba no se pisa.
fn store(inbox: &Path, part: &Path, class: &str, instance: &str, ts_uid: &str) -> Result<PathBuf, String> {
    let instance = uid_name(instance).ok_or("Identificador de imagen no válido.")?;
    let class = uid_name(class).ok_or("Clase de imagen no válida.")?;
    let ts = uid_name(ts_uid).ok_or("Sintaxis de transferencia no válida.")?;
    let meta = FileMetaTableBuilder::new().media_storage_sop_class_uid(class).media_storage_sop_instance_uid(instance.clone()).transfer_syntax(ts).build().map_err(|e| e.to_string())?;
    let file = part.with_extension("dcm.parcial");
    {
        let mut out = std::io::BufWriter::new(std::fs::File::create(&file).map_err(|e| e.to_string())?);
        out.write_all(&[0u8; 128]).and_then(|_| out.write_all(b"DICM")).map_err(|e| e.to_string())?;
        meta.write(&mut out).map_err(|e| e.to_string())?;
        std::io::copy(&mut std::fs::File::open(part).map_err(|e| e.to_string())?, &mut out).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    let _ = std::fs::remove_file(part);
    // la serie, de la cabecera de lo ya escrito
    let series = crate::dicom::read_header(&file).ok().and_then(|h| uid_name(&h.series)).unwrap_or_else(|| "sueltas".into());
    let dir = inbox.join(series);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{instance}.dcm"));
    if path.exists() {
        // ya la tenía (un reenvío): se queda la que había
        let _ = std::fs::remove_file(&file);
        return Ok(path);
    }
    std::fs::rename(&file, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Lo que comparten el servicio y sus conexiones.
struct Shared {
    settings: Settings,
    inbox: PathBuf,
    worklist: Arc<Mutex<Vec<WlItem>>>,
    /// El día de la clínica, para las preguntas sin fecha.
    today: Mutex<NaiveDate>,
    received: AtomicUsize,
    /// Para los nombres de los ficheros que se están recibiendo.
    counter: AtomicUsize,
}

/// Solo los nombres DICOM de la lista: a los demás se les rechaza la conexión.
struct Allowed(Settings);

impl dicom_ul::association::server::AccessControl for Allowed {
    fn check_access(&self, _this: &str, calling: &str, _called: &str, _user: Option<&dicom_ul::pdu::UserIdentity>) -> Result<(), dicom_ul::pdu::AssociationRJServiceUserReason> {
        let ae = calling.trim().to_uppercase();
        if self.0.devices().iter().any(|(a, _)| *a == ae) { Ok(()) } else { Err(dicom_ul::pdu::AssociationRJServiceUserReason::CallingAETitleNotRecognized) }
    }
}

fn cmd_u16(cmd: &InMemDicomObject, tag: dicom_core::Tag) -> u16 {
    cmd.element_opt(tag).ok().flatten().and_then(|e| e.to_int::<u16>().ok()).unwrap_or(0)
}

/// Atiende una conexión de un aparato hasta que la cierra. `ip` es la
/// dirección desde la que llega.
fn serve<S: Read + Write + dicom_ul::association::CloseSocket>(mut assoc: dicom_ul::ServerAssociation<S>, sh: &Shared, ip: Option<std::net::IpAddr>) -> Result<(), String> {
    use dicom_ul::association::Association;
    // el nombre ya se comprobó al conectar; ahora, que venga de su dirección
    if !sh.settings.allows(assoc.peer_ae_title(), ip) {
        let _ = assoc.abort();
        return Err("Aparato no admitido desde esa dirección.".into());
    }
    let mut cmd_buf: Vec<u8> = vec![];
    let mut small: Vec<u8> = vec![];
    // la imagen que se está recibiendo: su fichero y lo que lleva
    let mut part: Option<(PathBuf, std::fs::File, u64)> = None;
    let mut pending: Option<(InMemDicomObject, u8)> = None;
    let implicit = dicom_transfer_syntax_registry::entries::IMPLICIT_VR_LITTLE_ENDIAN.erased();
    let recv = sh.inbox.join(".recibiendo");
    let result = (|| -> Result<(), String> {
        loop {
            let pdu = assoc.receive().map_err(|e| e.to_string())?;
            match pdu {
                Pdu::PData { data } => {
                    for pdv in data {
                        let pc = pdv.presentation_context_id;
                        match pdv.value_type {
                            PDataValueType::Command => {
                                if cmd_buf.len() + pdv.data.len() > MAX_SMALL {
                                    return Err("Comando demasiado grande.".into());
                                }
                                cmd_buf.extend(pdv.data);
                                if !pdv.is_last {
                                    continue;
                                }
                                let cmd = InMemDicomObject::read_dataset_with_ts(cmd_buf.as_slice(), &implicit).map_err(|e| e.to_string())?;
                                cmd_buf.clear();
                                small.clear();
                                if let Some((f, _, _)) = part.take() {
                                    let _ = std::fs::remove_file(f);
                                }
                                if cmd_u16(&cmd, tags::COMMAND_DATA_SET_TYPE) != 0x0101 {
                                    pending = Some((cmd, pc));
                                } else if cmd_u16(&cmd, tags::COMMAND_FIELD) == 0x0030 {
                                    // C-ECHO
                                    assoc.send(&pdata(pc, PDataValueType::Command, response(0x8030, cmd_u16(&cmd, tags::MESSAGE_ID), uids::VERIFICATION, None, 0, false))).map_err(|e| e.to_string())?;
                                }
                            }
                            PDataValueType::Data => {
                                let Some((cmd, cmd_pc)) = pending.as_ref() else { continue };
                                let (field, id, cmd_pc) = (cmd_u16(cmd, tags::COMMAND_FIELD), cmd_u16(cmd, tags::MESSAGE_ID), *cmd_pc);
                                let (class, instance) = (text(cmd, tags::AFFECTED_SOP_CLASS_UID), text(cmd, tags::AFFECTED_SOP_INSTANCE_UID));
                                if field == 0x0001 {
                                    // C-STORE: los datos, a disco según llegan
                                    if part.is_none() {
                                        std::fs::create_dir_all(&recv).map_err(|e| e.to_string())?;
                                        let n = sh.counter.fetch_add(1, Ordering::Relaxed);
                                        // con una extensión que la búsqueda nunca toma, en una carpeta que se salta
                                        let f = recv.join(format!("{}-{n}.parcial", std::process::id()));
                                        let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&f).map_err(|e| e.to_string())?;
                                        part = Some((f, file, 0));
                                    }
                                    let Some((_, file, len)) = part.as_mut() else { continue };
                                    *len += pdv.data.len() as u64;
                                    if *len > MAX_INSTANCE {
                                        return Err("Imagen demasiado grande.".into());
                                    }
                                    file.write_all(&pdv.data).map_err(|e| e.to_string())?;
                                } else {
                                    if small.len() + pdv.data.len() > MAX_SMALL {
                                        return Err("Pregunta demasiado grande.".into());
                                    }
                                    small.extend(pdv.data);
                                }
                                if !pdv.is_last {
                                    continue;
                                }
                                pending = None;
                                let ts = assoc.presentation_contexts().iter().find(|p| p.id == cmd_pc).map(|p| p.transfer_syntax.clone()).unwrap_or_default();
                                match field {
                                    0x0001 => {
                                        let status = match part.take() {
                                            Some((f, file, _)) => {
                                                drop(file);
                                                match store(&sh.inbox, &f, &class, &instance, &ts) {
                                                    Ok(_) => {
                                                        sh.received.fetch_add(1, Ordering::Relaxed);
                                                        0x0000
                                                    }
                                                    Err(_) => {
                                                        let _ = std::fs::remove_file(&f);
                                                        0xA700
                                                    }
                                                }
                                            }
                                            None => 0xA700,
                                        };
                                        assoc.send(&pdata(cmd_pc, PDataValueType::Command, response(0x8001, id, &class, Some(&instance), status, false))).map_err(|e| e.to_string())?;
                                    }
                                    // C-FIND de la lista de trabajo
                                    0x0020 => {
                                        let tsx = TransferSyntaxRegistry.get(&ts).ok_or("Sintaxis desconocida.")?;
                                        let identifier = InMemDicomObject::read_dataset_with_ts(small.as_slice(), tsx).map_err(|e| e.to_string())?;
                                        let today = sh.today.lock().map(|d| *d).unwrap_or_else(|_| chrono::Local::now().date_naive());
                                        let q = query_of(&identifier).bounded(today);
                                        let items: Vec<WlItem> = sh.worklist.lock().map(|w| w.iter().filter(|i| q.matches(i)).cloned().collect()).unwrap_or_default();
                                        for it in items.iter().take(200) {
                                            let mut body = vec![];
                                            answer(it, &q).write_dataset_with_ts(&mut body, tsx).map_err(|e| e.to_string())?;
                                            assoc.send(&pdata(cmd_pc, PDataValueType::Command, response(0x8020, id, &class, None, 0xFF00, true))).map_err(|e| e.to_string())?;
                                            assoc.send(&pdata(cmd_pc, PDataValueType::Data, body)).map_err(|e| e.to_string())?;
                                        }
                                        assoc.send(&pdata(cmd_pc, PDataValueType::Command, response(0x8020, id, &class, None, 0x0000, false))).map_err(|e| e.to_string())?;
                                    }
                                    _ => {}
                                }
                                small.clear();
                            }
                        }
                    }
                }
                Pdu::ReleaseRQ => {
                    let _ = assoc.send(&Pdu::ReleaseRP);
                    return Ok(());
                }
                Pdu::AbortRQ { .. } => return Ok(()),
                _ => {}
            }
        }
    })();
    // lo que quedó a medias no se queda en el disco
    if let Some((f, file, _)) = part.take() {
        drop(file);
        let _ = std::fs::remove_file(f);
    }
    if result.is_err() {
        let _ = assoc.abort();
    }
    result
}

/// El servicio en marcha. Se para al soltarlo.
pub struct Service {
    stop: Arc<AtomicBool>,
    shared: Arc<Shared>,
    pub port: u16,
    /// El hilo que acepta conexiones: al parar se espera a que suelte el puerto.
    listener: Option<std::thread::JoinHandle<()>>,
}

impl Service {
    /// Arranca el servicio en el puerto de la configuración (0: uno libre, para las pruebas).
    pub fn start(settings: &Settings, inbox: PathBuf) -> Result<Service, String> {
        let listener = TcpListener::bind(("0.0.0.0", settings.port)).map_err(|e| format!("No se puede atender en el puerto {}: {e}", settings.port))?;
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(settings.port);
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&inbox).map_err(|e| format!("No se puede crear {}: {e}", inbox.display()))?;
        let stop = Arc::new(AtomicBool::new(false));
        if settings.devices().is_empty() {
            return Err("Añada antes los aparatos admitidos (su nombre DICOM y su dirección IP).".into());
        }
        let shared = Arc::new(Shared { settings: settings.clone(), inbox, worklist: Arc::new(Mutex::new(vec![])), today: Mutex::new(chrono::Local::now().date_naive()), received: AtomicUsize::new(0), counter: AtomicUsize::new(0) });
        let (stop2, sh) = (stop.clone(), shared.clone());
        let active = Arc::new(AtomicUsize::new(0));
        let listener = std::thread::spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if active.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                            continue;
                        }
                        let (sh, active) = (sh.clone(), active.clone());
                        active.fetch_add(1, Ordering::Relaxed);
                        std::thread::spawn(move || {
                            let _ = handle(stream, &sh);
                            active.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(150)),
                    Err(_) => std::thread::sleep(Duration::from_millis(500)),
                }
            }
        });
        Ok(Service { stop, shared, port, listener: Some(listener) })
    }

    /// Los citados que verá el aparato (se renueva desde los datos de la
    /// clínica) y el día de hoy de la clínica.
    pub fn set_worklist(&self, items: Vec<WlItem>, today: NaiveDate) {
        if let Ok(mut w) = self.shared.worklist.lock() {
            *w = items;
        }
        if let Ok(mut t) = self.shared.today.lock() {
            *t = today;
        }
    }

    /// Imágenes recibidas desde que arrancó.
    pub fn received(&self) -> usize {
        self.shared.received.load(Ordering::Relaxed)
    }

    pub fn settings(&self) -> &Settings {
        &self.shared.settings
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // el puerto queda libre al volver (para arrancar otra vez con otra configuración)
        if let Some(t) = self.listener.take() {
            let _ = t.join();
        }
    }
}

fn handle(stream: TcpStream, sh: &Shared) -> Result<(), String> {
    // solo desde la red de la clínica
    let ip = stream.peer_addr().ok().map(|a| a.ip());
    if !ip.is_some_and(local_network) {
        return Err("Conexión desde fuera de la red de la clínica.".into());
    }
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    let mut options = dicom_ul::association::ServerAssociationOptions::new()
        .ae_access_control(Allowed(sh.settings.clone()))
        .ae_title(sh.settings.ae_title.clone())
        .read_timeout(Duration::from_secs(120))
        .with_abstract_syntax(uids::VERIFICATION)
        .with_abstract_syntax(uids::MODALITY_WORKLIST_INFORMATION_MODEL_FIND);
    for uid in STORAGE {
        options = options.with_abstract_syntax(uid);
    }
    for ts in TransferSyntaxRegistry.iter() {
        if !ts.is_unsupported() {
            options = options.with_transfer_syntax(ts.uid());
        }
    }
    let assoc = options.establish(stream).map_err(|e| e.to_string())?;
    serve(assoc, sh, ip)
}

/// La carpeta donde se guarda lo que mandan los aparatos.
pub fn inbox_dir(s: &Settings) -> PathBuf {
    if s.folder.trim().is_empty() { crate::paths::data().join(crate::brand::DIR).join("imagenes").join("entrada_dicom") } else { PathBuf::from(s.folder.trim()) }
}

/// La dirección de este ordenador en la red de la clínica (la que se pone en
/// los aparatos). No envía nada: solo pregunta por qué interfaz saldría.
pub fn local_ip() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("192.0.2.1:9").ok()?;
    sock.local_addr().ok().map(|a| a.ip().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dicom_ul::association::ClientAssociationOptions;

    fn item(id: &str, name: &str, date: &str) -> WlItem {
        WlItem {
            accession: "7".into(),
            patient_id: id.into(),
            name: name.into(),
            birth: "19720202".into(),
            sex: "M".into(),
            date: date.into(),
            time: "093000".into(),
            description: "TAC".into(),
            doctor: "DR NAVARRO".into(),
            study_uid: "2.25.1".into(),
        }
    }

    #[test]
    fn the_device_finds_who_is_booked() {
        let carlos = item("1002", "MARTINEZ SANZ^CARLOS", "20261116");
        let q = |name: &str, date: &str, id: &str| Query { name: name.into(), date: date.into(), patient_id: id.into(), ..Default::default() };
        assert!(q("", "", "").matches(&carlos), "sin filtros, todos");
        assert!(q("", "20261116", "").matches(&carlos));
        assert!(!q("", "20261117", "").matches(&carlos));
        assert!(q("", "20261115-20261120", "").matches(&carlos));
        assert!(q("", "-20261116", "").matches(&carlos));
        assert!(!q("", "20261117-", "").matches(&carlos));
        assert!(q("MARTINEZ*", "", "").matches(&carlos));
        assert!(q("mart?nez*", "", "").matches(&carlos), "sin distinguir mayúsculas");
        assert!(q("MARTINEZ SANZ^CARLOS", "", "").matches(&carlos));
        assert!(!q("GARCIA*", "", "").matches(&carlos));
        assert!(q("", "", "1002").matches(&carlos));
        assert!(!q("", "", "1001").matches(&carlos));
    }

    #[test]
    fn a_question_without_date_is_about_today_and_ranges_are_short() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let q = |date: &str| Query { date: date.into(), ..Default::default() }.bounded(today).date;
        assert_eq!(q(""), "20261116");
        assert_eq!(q("20261120"), "20261120");
        assert_eq!(q("20261101-20261231"), "20261101-20261108", "como mucho una semana");
        assert_eq!(q("20261110-"), "20261110-20261117");
        assert_eq!(q("-20261120"), "20261113-20261120");
        let carlos = item("1002", "MARTINEZ SANZ^CARLOS", "20261117");
        assert!(!Query::default().bounded(today).matches(&carlos), "sin fecha, solo los de hoy");
    }

    #[test]
    fn only_listed_devices_from_their_address_and_from_the_clinic_network() {
        let s = Settings { allowed: "CBCT1 192.168.1.50\npano".into(), ..Default::default() };
        let ip = |x: &str| Some(x.parse::<std::net::IpAddr>().unwrap());
        assert!(s.allows("cbct1", ip("192.168.1.50")));
        assert!(!s.allows("CBCT1", ip("192.168.1.51")), "con su dirección");
        assert!(s.allows("PANO", ip("192.168.1.77")), "sin dirección en la lista, desde cualquiera de la red");
        assert!(!s.allows("OTRO", ip("192.168.1.50")));
        assert!(!Settings::default().allows("PANO", ip("192.168.1.50")), "sin lista, nadie");
        assert!(local_network("192.168.1.2".parse().unwrap()) && local_network("10.0.0.1".parse().unwrap()) && local_network("127.0.0.1".parse().unwrap()));
        assert!(!local_network("8.8.8.8".parse().unwrap()) && !local_network("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn the_worklist_comes_from_the_appointments_in_plain_capitals() {
        let db = Db::sample(chrono::NaiveDate::from_ymd_opt(2026, 11, 16).unwrap());
        let day = db.appointments.iter().find(|a| a.patient_id.is_some() && !matches!(a.state, ApptState::Anulada | ApptState::Reprogramada | ApptState::NoAcude)).map(|a| a.date).unwrap();
        let wl = worklist(&db, day, day);
        assert!(!wl.is_empty());
        for it in &wl {
            assert!(it.name.contains('^') && it.name.is_ascii(), "{}", it.name);
            assert_eq!(it.date, day.format("%Y%m%d").to_string());
            assert!(it.study_uid.starts_with("2.25.") && it.study_uid.len() <= 64);
        }
        // el mismo identificador de estudio para la misma cita
        assert_eq!(worklist(&db, day, day), wl);
    }

    /// Un aparato de verdad, por la red: verificación, lista de trabajo y una imagen.
    #[test]
    fn a_device_checks_the_link_asks_the_worklist_and_sends_an_image() {
        let inbox = std::env::temp_dir().join(format!("vd_dicom_red_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&inbox);
        // sin aparatos en la lista, el servicio no arranca
        assert!(Service::start(&Settings { enabled: true, port: 0, ..Default::default() }, inbox.clone()).is_err());
        let settings = Settings { enabled: true, port: 0, allowed: "TAC-1 127.0.0.1\nPANO 10.9.9.9".into(), ..Default::default() };
        let svc = Service::start(&settings, inbox.clone()).unwrap();
        svc.set_worklist(vec![item("1002", "MARTINEZ SANZ^CARLOS", "20261116"), item("1001", "GARCIA LOPEZ^ANA", "20261117")], NaiveDate::from_ymd_opt(2026, 11, 16).unwrap());
        let addr = format!("127.0.0.1:{}", svc.port);
        let ts = "1.2.840.10008.1.2.1";
        let implicit = dicom_transfer_syntax_registry::entries::IMPLICIT_VR_LITTLE_ENDIAN.erased();
        let explicit = TransferSyntaxRegistry.get(ts).unwrap();
        let connect = |calling: &str| {
            ClientAssociationOptions::new()
                .calling_ae_title(calling)
                .called_ae_title("VEODARDENTAL")
                .with_presentation_context(uids::VERIFICATION, vec![ts])
                .with_presentation_context(uids::MODALITY_WORKLIST_INFORMATION_MODEL_FIND, vec![ts])
                .with_presentation_context(uids::CT_IMAGE_STORAGE, vec![ts])
                .establish(addr.as_str())
        };
        let read = |data: &[u8]| InMemDicomObject::read_dataset_with_ts(data, &implicit).unwrap();
        let field = |o: &InMemDicomObject, t| o.element(t).unwrap().to_int::<u16>().unwrap();
        let mut a = connect("TAC-1").expect("el aparato admitido se conecta");
        let pc = |a: &dicom_ul::ClientAssociation<TcpStream>, syntax: &str| a.presentation_contexts().iter().find(|p| p.abstract_syntax == syntax).unwrap().id;
        // 1) C-ECHO
        let echo = command(vec![
            DataElement::new(tags::AFFECTED_SOP_CLASS_UID, VR::UI, s(uids::VERIFICATION)),
            DataElement::new(tags::COMMAND_FIELD, VR::US, us(0x0030)),
            DataElement::new(tags::MESSAGE_ID, VR::US, us(1)),
            DataElement::new(tags::COMMAND_DATA_SET_TYPE, VR::US, us(0x0101)),
        ]);
        a.send(&pdata(pc(&a, uids::VERIFICATION), PDataValueType::Command, echo)).unwrap();
        let Pdu::PData { data } = a.receive().unwrap() else { panic!("sin respuesta al eco") };
        let r = read(&data[0].data);
        assert_eq!((field(&r, tags::COMMAND_FIELD), field(&r, tags::STATUS)), (0x8030, 0));
        // 2) C-FIND: los citados el 16
        let find_pc = pc(&a, uids::MODALITY_WORKLIST_INFORMATION_MODEL_FIND);
        let find = command(vec![
            DataElement::new(tags::AFFECTED_SOP_CLASS_UID, VR::UI, s(uids::MODALITY_WORKLIST_INFORMATION_MODEL_FIND)),
            DataElement::new(tags::COMMAND_FIELD, VR::US, us(0x0020)),
            DataElement::new(tags::MESSAGE_ID, VR::US, us(2)),
            DataElement::new(tags::PRIORITY, VR::US, us(0)),
            DataElement::new(tags::COMMAND_DATA_SET_TYPE, VR::US, us(0x0102)),
        ]);
        let step = InMemDicomObject::from_element_iter([DataElement::new(tags::SCHEDULED_PROCEDURE_STEP_START_DATE, VR::DA, s("20261116")), DataElement::new(tags::MODALITY, VR::CS, s("CT"))]);
        let ask = InMemDicomObject::from_element_iter([DataElement::new(tags::PATIENT_NAME, VR::PN, s("")), DataElement::new(tags::SCHEDULED_PROCEDURE_STEP_SEQUENCE, VR::SQ, DataSetSequence::from(vec![step]))]);
        let mut body = vec![];
        ask.write_dataset_with_ts(&mut body, explicit).unwrap();
        a.send(&pdata(find_pc, PDataValueType::Command, find)).unwrap();
        a.send(&pdata(find_pc, PDataValueType::Data, body)).unwrap();
        let mut found = vec![];
        loop {
            let Pdu::PData { data } = a.receive().unwrap() else { panic!("respuesta inesperada") };
            let pdv = &data[0];
            if pdv.value_type == PDataValueType::Data {
                found.push(InMemDicomObject::read_dataset_with_ts(pdv.data.as_slice(), explicit).unwrap());
                continue;
            }
            if field(&read(&pdv.data), tags::STATUS) == 0 {
                break;
            }
        }
        assert_eq!(found.len(), 1, "solo el citado ese día");
        assert_eq!(text(&found[0], tags::PATIENT_ID), "1002");
        assert_eq!(text(&found[0], tags::PATIENT_NAME), "MARTINEZ SANZ^CARLOS");
        // 3) C-STORE: una imagen de un TAC
        let image = InMemDicomObject::from_element_iter([
            DataElement::new(tags::SOP_CLASS_UID, VR::UI, s(uids::CT_IMAGE_STORAGE)),
            DataElement::new(tags::SOP_INSTANCE_UID, VR::UI, s("1.2.3.4.5")),
            DataElement::new(tags::MODALITY, VR::CS, s("CT")),
            DataElement::new(tags::PATIENT_NAME, VR::PN, s("MARTINEZ SANZ^CARLOS")),
            DataElement::new(tags::PATIENT_ID, VR::LO, s("1002")),
            DataElement::new(tags::SERIES_INSTANCE_UID, VR::UI, s("1.2.3.4")),
            DataElement::new(tags::ROWS, VR::US, us(2)),
            DataElement::new(tags::COLUMNS, VR::US, us(2)),
            DataElement::new(tags::BITS_ALLOCATED, VR::US, us(16)),
            DataElement::new(tags::PIXEL_DATA, VR::OW, PrimitiveValue::U16(dicom_core::smallvec::smallvec![0u16, 1, 2, 3])),
        ]);
        let mut body = vec![];
        image.write_dataset_with_ts(&mut body, explicit).unwrap();
        let store_pc = pc(&a, uids::CT_IMAGE_STORAGE);
        let st = command(vec![
            DataElement::new(tags::AFFECTED_SOP_CLASS_UID, VR::UI, s(uids::CT_IMAGE_STORAGE)),
            DataElement::new(tags::COMMAND_FIELD, VR::US, us(0x0001)),
            DataElement::new(tags::MESSAGE_ID, VR::US, us(3)),
            DataElement::new(tags::PRIORITY, VR::US, us(0)),
            DataElement::new(tags::COMMAND_DATA_SET_TYPE, VR::US, us(0x0000)),
            DataElement::new(tags::AFFECTED_SOP_INSTANCE_UID, VR::UI, s("1.2.3.4.5")),
        ]);
        // llega en dos trozos, como llega un TAC grande
        let half = body.len() / 2;
        a.send(&pdata(store_pc, PDataValueType::Command, st.clone())).unwrap();
        a.send(&Pdu::PData { data: vec![PDataValue { presentation_context_id: store_pc, value_type: PDataValueType::Data, is_last: false, data: body[..half].to_vec() }] }).unwrap();
        a.send(&pdata(store_pc, PDataValueType::Data, body[half..].to_vec())).unwrap();
        let Pdu::PData { data } = a.receive().unwrap() else { panic!("sin respuesta al envío") };
        let r = read(&data[0].data);
        assert_eq!((field(&r, tags::COMMAND_FIELD), field(&r, tags::STATUS)), (0x8001, 0));
        let saved = inbox.join("1.2.3.4").join("1.2.3.4.5.dcm");
        let h = crate::dicom::read_header(&saved).expect("se guarda como DICOM");
        assert_eq!((h.patient_id.as_str(), h.patient.as_str()), ("1002", "MARTINEZ SANZ CARLOS"));
        // la misma imagen otra vez, con otro paciente: no pisa la que había
        let mut other = vec![];
        let fake = InMemDicomObject::from_element_iter([
            DataElement::new(tags::SOP_CLASS_UID, VR::UI, s(uids::CT_IMAGE_STORAGE)),
            DataElement::new(tags::SOP_INSTANCE_UID, VR::UI, s("1.2.3.4.5")),
            DataElement::new(tags::PATIENT_ID, VR::LO, s("9999")),
            DataElement::new(tags::SERIES_INSTANCE_UID, VR::UI, s("1.2.3.4")),
        ]);
        fake.write_dataset_with_ts(&mut other, explicit).unwrap();
        a.send(&pdata(store_pc, PDataValueType::Command, st)).unwrap();
        a.send(&pdata(store_pc, PDataValueType::Data, other)).unwrap();
        let _ = a.receive().unwrap();
        assert_eq!(crate::dicom::read_header(&saved).unwrap().patient_id, "1002", "la primera se queda");
        let _ = a.release();
        assert!(std::fs::read_dir(inbox.join(".recibiendo")).map(|d| d.count()).unwrap_or(0) == 0, "no quedan ficheros a medias");
        assert_eq!(svc.received(), 2);
        // un aparato que no está en la lista no llega a conectarse
        assert!(connect("OTRO").is_err(), "el aparato no admitido se rechaza");
        // uno de la lista, pero desde otra dirección: se corta
        if let Ok(mut b) = connect("PANO") {
            let echo = command(vec![
                DataElement::new(tags::AFFECTED_SOP_CLASS_UID, VR::UI, s(uids::VERIFICATION)),
                DataElement::new(tags::COMMAND_FIELD, VR::US, us(0x0030)),
                DataElement::new(tags::MESSAGE_ID, VR::US, us(9)),
                DataElement::new(tags::COMMAND_DATA_SET_TYPE, VR::US, us(0x0101)),
            ]);
            let _ = b.send(&pdata(1, PDataValueType::Command, echo));
            assert!(!matches!(b.receive(), Ok(Pdu::PData { .. })), "desde otra dirección no contesta");
        }
        drop(svc);
        let _ = std::fs::remove_dir_all(&inbox);
    }
}
