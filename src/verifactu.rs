//! VERI*FACTU: remisión de los registros de facturación a la AEAT.
//!
//! Cada factura emitida genera un registro de alta encadenado (huella SHA-256
//! del registro anterior, según las especificaciones de la AEAT). Después
//! pueden añadirse a la misma cadena registros de subsanación (corregir los
//! datos de un registro aceptado con errores o volver a enviar uno rechazado)
//! y de anulación (factura expedida por error). Aquí se construye el XML
//! oficial (esquemas SuministroLR.xsd y SuministroInformacion.xsd), se envía
//! por el servicio web SOAP de la AEAT con el certificado electrónico de la
//! clínica (TLS con certificado de cliente), se lee la respuesta (estado de
//! cada registro y CSV) y se genera la URL del código QR de cada factura.

use crate::model::{Db, Invoice, sha256_hex};
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const NS_LR: &str = "https://www2.agenciatributaria.gob.es/static_files/common/internet/dep/aplicaciones/es/aeat/tike/cont/ws/SuministroLR.xsd";
const NS_SF: &str = "https://www2.agenciatributaria.gob.es/static_files/common/internet/dep/aplicaciones/es/aeat/tike/cont/ws/SuministroInformacion.xsd";

/// Entorno de la AEAT.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Env {
    /// Entorno de pruebas (preproducción): no tiene efectos fiscales.
    #[default]
    Pruebas,
    Produccion,
}

impl Env {
    pub fn label(self) -> &'static str {
        match self {
            Env::Pruebas => "Pruebas (sin efectos fiscales)",
            Env::Produccion => "Producción (real)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct VfConfig {
    /// Enviar los registros a la AEAT.
    pub enabled: bool,
    pub env: Env,
    /// Certificado de sello electrónico (servidor www10) en vez de certificado de persona o representante (www1).
    #[serde(default)]
    pub seal: bool,
    /// Se remiten los registros generados desde este momento (al activar).
    #[serde(default)]
    pub since: Option<NaiveDateTime>,
    /// Envío automático en segundo plano.
    #[serde(default)]
    pub auto_send: bool,
    /// Productor del programa (si se deja vacío, la propia clínica).
    #[serde(default)]
    pub producer_name: String,
    #[serde(default)]
    pub producer_nif: String,
    /// Identificador de esta instalación (se genera al activar).
    #[serde(default)]
    pub installation: String,
    /// Dirección alternativa del servicio (sólo para pruebas internas).
    #[serde(default)]
    pub endpoint: String,
    /// Ordenador que envía los registros a la AEAT (vacío: aún ninguno). Si
    /// enviaran dos, mandarían los mismos registros a la vez.
    #[serde(default)]
    pub sender: String,
}

impl VfConfig {
    /// ¿Envía este ordenador (`me`) los registros? El que está elegido o, si
    /// aún no hay ninguno, el que vaya a enviar (y queda elegido).
    pub fn may_send(&self, me: &str) -> bool {
        self.sender.trim().is_empty() || self.sender == me
    }

    pub fn url(&self) -> String {
        if !self.endpoint.trim().is_empty() {
            return self.endpoint.trim().to_string();
        }
        let host = match (self.env, self.seal) {
            (Env::Pruebas, false) => "https://prewww1.aeat.es",
            (Env::Pruebas, true) => "https://prewww10.aeat.es",
            (Env::Produccion, false) => "https://www1.agenciatributaria.gob.es",
            (Env::Produccion, true) => "https://www10.agenciatributaria.gob.es",
        };
        format!("{host}/wlpl/TIKE-CONT/ws/SistemaFacturacion/VerifactuSOAP")
    }

    /// Dirección del cotejo de la factura (la que lleva el código QR).
    pub fn qr_base(&self) -> &'static str {
        match self.env {
            Env::Pruebas => "https://prewww2.aeat.es/wlpl/TIKE-CONT/ValidarQR",
            Env::Produccion => "https://www2.agenciatributaria.gob.es/wlpl/TIKE-CONT/ValidarQR",
        }
    }
}

/// Estado de un registro en la AEAT.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct AeatStatus {
    /// "" (pendiente), "ErrorConexion", "Correcto", "AceptadoConErrores" o "Incorrecto".
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub csv: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub sent_at: Option<NaiveDateTime>,
    #[serde(default)]
    pub tries: u32,
}

impl AeatStatus {
    pub fn label(&self) -> &'static str {
        match self.state.as_str() {
            "Correcto" => "Registrada en la AEAT",
            "AceptadoConErrores" => "Aceptada con errores",
            "Incorrecto" => "Rechazada por la AEAT",
            "ErrorConexion" => "Reintentar (sin conexión)",
            _ => "Pendiente de enviar",
        }
    }
    pub fn sent_ok(&self) -> bool {
        matches!(self.state.as_str(), "Correcto" | "AceptadoConErrores")
    }
    pub fn pending(&self) -> bool {
        matches!(self.state.as_str(), "" | "ErrorConexion")
    }
}

/// Registro añadido a la cadena después de la factura.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecKind {
    /// Nuevo registro de alta que corrige o vuelve a enviar el de una factura.
    Subsanacion,
    /// Anulación de una factura expedida por error.
    Anulacion,
}

/// Registro de subsanación o de anulación. Guarda los datos de la factura
/// que entran en su huella, para poder comprobar la cadena siempre.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VfRecord {
    pub id: u32,
    pub invoice_id: u32,
    pub kind: RecKind,
    pub created: NaiveDateTime,
    pub gen_ts: String,
    pub prev_hash: String,
    pub hash: String,
    /// Subsanación «S» (corrige un registro que ya existe en la AEAT).
    #[serde(default)]
    pub subsanacion: bool,
    /// RechazoPrevio: "" (no se indica), "S" (el anterior fue rechazado) o "X".
    #[serde(default)]
    pub rechazo_previo: String,
    /// Anulación de una factura cuyo alta nunca llegó a registrarse en la AEAT.
    #[serde(default)]
    pub sin_registro_previo: bool,
    pub issuer_nif: String,
    pub num_serie: String,
    pub fecha: NaiveDate,
    pub tipo: String,
    pub cuota: f64,
    pub importe: f64,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub aeat: AeatStatus,
}

impl VfRecord {
    pub fn canonical(&self) -> String {
        match self.kind {
            RecKind::Anulacion => {
                format!("IDEmisorFacturaAnulada={}&NumSerieFacturaAnulada={}&FechaExpedicionFacturaAnulada={}&Huella={}&FechaHoraHusoGenRegistro={}", self.issuer_nif, self.num_serie, self.fecha.format("%d-%m-%Y"), self.prev_hash, self.gen_ts)
            }
            RecKind::Subsanacion => format!(
                "IDEmisorFactura={}&NumSerieFactura={}&FechaExpedicionFactura={}&TipoFactura={}&CuotaTotal={:.2}&ImporteTotal={:.2}&Huella={}&FechaHoraHusoGenRegistro={}",
                self.issuer_nif,
                self.num_serie,
                self.fecha.format("%d-%m-%Y"),
                self.tipo,
                self.cuota,
                self.importe,
                self.prev_hash,
                self.gen_ts
            ),
        }
    }
    pub fn label(&self) -> &'static str {
        match (self.kind, self.subsanacion) {
            (RecKind::Anulacion, _) => "Anulación",
            (_, true) => "Subsanación",
            _ => "Reenvío tras rechazo",
        }
    }
}

/// Un eslabón de la cadena: el alta de una factura o un registro posterior.
#[derive(Clone, Copy)]
pub enum Item<'a> {
    Inv(&'a Invoice),
    Rec(&'a VfRecord),
}

/// Clave de un eslabón (para los lotes de envío).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Inv(u32),
    Rec(u32),
}

impl<'a> Item<'a> {
    pub fn key(&self) -> Key {
        match self {
            Item::Inv(i) => Key::Inv(i.id),
            Item::Rec(r) => Key::Rec(r.id),
        }
    }
    pub fn created(&self) -> NaiveDateTime {
        match self {
            Item::Inv(i) => i.created,
            Item::Rec(r) => r.created,
        }
    }
    fn order(&self) -> (NaiveDateTime, u32) {
        match self {
            Item::Inv(i) => (i.created, i.id),
            Item::Rec(r) => (r.created, r.id),
        }
    }
    pub fn hash(&self) -> &'a str {
        match self {
            Item::Inv(i) => &i.hash,
            Item::Rec(r) => &r.hash,
        }
    }
    pub fn prev_hash(&self) -> &'a str {
        match self {
            Item::Inv(i) => &i.prev_hash,
            Item::Rec(r) => &r.prev_hash,
        }
    }
    pub fn aeat(&self) -> &'a AeatStatus {
        match self {
            Item::Inv(i) => &i.aeat,
            Item::Rec(r) => &r.aeat,
        }
    }
    pub fn invoice_id(&self) -> u32 {
        match self {
            Item::Inv(i) => i.id,
            Item::Rec(r) => r.invoice_id,
        }
    }
    pub fn num_serie(&self) -> String {
        match self {
            Item::Inv(i) => i.code(),
            Item::Rec(r) => r.num_serie.clone(),
        }
    }
    fn fecha(&self) -> NaiveDate {
        match self {
            Item::Inv(i) => i.date,
            Item::Rec(r) => r.fecha,
        }
    }
    fn issuer(&self, nif: &str) -> String {
        match self {
            Item::Inv(i) => nif_norm(if i.issuer_nif.is_empty() { nif } else { &i.issuer_nif }),
            Item::Rec(r) => nif_norm(&r.issuer_nif),
        }
    }
    pub fn canonical(&self, nif: &str) -> String {
        match self {
            Item::Inv(i) => i.canonical(nif),
            Item::Rec(r) => r.canonical(),
        }
    }
    /// Operación con la que la AEAT identifica el registro en su respuesta.
    fn operation(&self) -> &'static str {
        match self {
            Item::Rec(r) if r.kind == RecKind::Anulacion => "Anulacion",
            _ => "Alta",
        }
    }
    /// Nombre para las listas.
    pub fn what(&self) -> String {
        match self {
            Item::Inv(i) => i.code(),
            Item::Rec(r) => format!("{} · {}", r.num_serie, r.label().to_lowercase()),
        }
    }
}

/// Toda la cadena de registros en el orden en que se generaron.
pub fn chain(db: &Db) -> Vec<Item<'_>> {
    let mut v: Vec<Item> = db.invoices.iter().map(Item::Inv).chain(db.vf_records.iter().map(Item::Rec)).collect();
    v.sort_by_key(|i| i.order());
    v
}

/// Huella del último registro de la cadena.
pub fn last_hash(db: &Db) -> String {
    chain(db).last().map(|i| i.hash().to_string()).unwrap_or_default()
}

/// Comprueba que ningún registro se ha alterado. Devuelve cuántos hay o el primero que falla.
pub fn verify(db: &Db) -> Result<usize, String> {
    let list = chain(db);
    let mut prev = String::new();
    for it in &list {
        if it.prev_hash() != prev {
            return Err(format!("El registro de {} no enlaza con el anterior.", it.what()));
        }
        if sha256_hex(&it.canonical(&db.config.nif)) != it.hash() {
            return Err(format!("El registro de {} ha sido modificado después de generarse.", it.what()));
        }
        prev = it.hash().to_string();
    }
    Ok(list.len())
}

/// Último registro de una factura (el que manda su estado).
pub fn latest(db: &Db, invoice_id: u32) -> Option<Item<'_>> {
    chain(db).into_iter().rfind(|i| i.invoice_id() == invoice_id)
}

/// La factura se anuló (con un registro de anulación que la AEAT no rechazó).
pub fn cancelled(db: &Db, invoice_id: u32) -> bool {
    db.vf_records.iter().any(|r| r.invoice_id == invoice_id && r.kind == RecKind::Anulacion && r.aeat.state != "Incorrecto")
}

// ───────────────────────────── utilidades ─────────────────────────────

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => {}
            c => o.push(c),
        }
    }
    o
}

fn cut(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

fn amount(v: f64) -> String {
    let v = if v.abs() < 0.005 { 0.0 } else { v };
    format!("{v:.2}")
}

pub fn nif_norm(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase()
}

fn now_stamp() -> (NaiveDateTime, String) {
    let t = chrono::Local::now();
    (t.naive_local(), t.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
}

/// Codifica un valor para una URL (RFC 3986).
pub fn url_encode(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

/// URL del código QR de una factura (cotejo en la sede de la AEAT).
pub fn qr_url(cfg: &VfConfig, inv: &Invoice, issuer_nif: &str) -> String {
    let nif = if inv.issuer_nif.is_empty() { issuer_nif } else { &inv.issuer_nif };
    format!("{}?nif={}&numserie={}&fecha={}&importe={}", cfg.qr_base(), url_encode(&nif_norm(nif)), url_encode(&inv.code()), inv.date.format("%d-%m-%Y"), amount(inv.total()))
}

/// Fecha y hora de generación del registro de alta de una factura (con huso horario).
pub fn gen_ts(inv: &Invoice) -> String {
    if inv.gen_ts.is_empty() { inv.created.format("%Y-%m-%dT%H:%M:%S+01:00").to_string() } else { inv.gen_ts.clone() }
}

// ───────────────────────────── nuevos registros ─────────────────────────────

/// Genera un registro que corrige el de una factura: subsanación si la AEAT lo
/// aceptó (con o sin errores) o reenvío si lo rechazó. Los datos que se corrigen
/// (nombre y NIF del destinatario) se cambian antes en la factura.
pub fn create_correction(db: &mut Db, invoice_id: u32, user: &str) -> Result<u32, String> {
    let inv = db.invoice(invoice_id).cloned().ok_or("Factura no encontrada.")?;
    if cancelled(db, invoice_id) {
        return Err("La factura está anulada.".into());
    }
    let last = latest(db, invoice_id).ok_or("Factura sin registro.")?;
    let st = last.aeat().clone();
    if st.pending() {
        return Err("El último registro de esta factura todavía no se ha enviado: espere la respuesta de la AEAT.".into());
    }
    let last_was_correction = matches!(last, Item::Rec(r) if r.kind == RecKind::Subsanacion && r.subsanacion);
    let (subsanacion, rechazo_previo) = match st.state.as_str() {
        // aceptado: subsanación de un registro que ya existe en la AEAT
        "Correcto" | "AceptadoConErrores" => (true, String::new()),
        // rechazado: nuevo envío indicando el rechazo previo
        _ => (last_was_correction, "S".to_string()),
    };
    let (created, gen_ts) = now_stamp();
    let mut r = VfRecord {
        id: db.new_id(),
        invoice_id,
        kind: RecKind::Subsanacion,
        created,
        gen_ts,
        prev_hash: last_hash(db),
        hash: String::new(),
        subsanacion,
        rechazo_previo,
        sin_registro_previo: false,
        issuer_nif: nif_norm(if inv.issuer_nif.is_empty() { &db.config.nif } else { &inv.issuer_nif }),
        num_serie: inv.code(),
        fecha: inv.date,
        tipo: inv.kind().to_string(),
        cuota: inv.vat(),
        importe: inv.total(),
        reason: String::new(),
        user: user.to_string(),
        aeat: AeatStatus::default(),
    };
    r.hash = sha256_hex(&r.canonical());
    let id = r.id;
    db.vf_records.push(r);
    Ok(id)
}

/// Anula una factura expedida por error: registro de anulación encadenado. Los
/// tratamientos quedan libres para facturarse de nuevo.
pub fn create_cancellation(db: &mut Db, invoice_id: u32, reason: &str, user: &str) -> Result<u32, String> {
    let inv = db.invoice(invoice_id).cloned().ok_or("Factura no encontrada.")?;
    if cancelled(db, invoice_id) {
        return Err("La factura ya está anulada.".into());
    }
    if reason.trim().is_empty() {
        return Err("Indique por qué se anula la factura.".into());
    }
    if inv.rectified_by.is_some() {
        return Err("La factura ya tiene una rectificativa: no se puede anular.".into());
    }
    let last = latest(db, invoice_id).ok_or("Factura sin registro.")?;
    if last.aeat().pending() && db.verifactu.since.map(|s| last.created() >= s).unwrap_or(true) {
        return Err("El registro de esta factura todavía no se ha enviado: espere la respuesta de la AEAT antes de anularla.".into());
    }
    // ¿llegó a registrarse el alta en la AEAT?
    let registered = chain(db).iter().any(|i| i.invoice_id() == invoice_id && i.operation() == "Alta" && i.aeat().sent_ok());
    let rejected_cancel = db.vf_records.iter().any(|r| r.invoice_id == invoice_id && r.kind == RecKind::Anulacion && r.aeat.state == "Incorrecto");
    let (created, gen_ts) = now_stamp();
    let mut r = VfRecord {
        id: db.new_id(),
        invoice_id,
        kind: RecKind::Anulacion,
        created,
        gen_ts,
        prev_hash: last_hash(db),
        hash: String::new(),
        subsanacion: false,
        rechazo_previo: if rejected_cancel { "S".into() } else { String::new() },
        sin_registro_previo: !registered,
        issuer_nif: nif_norm(if inv.issuer_nif.is_empty() { &db.config.nif } else { &inv.issuer_nif }),
        num_serie: inv.code(),
        fecha: inv.date,
        tipo: inv.kind().to_string(),
        cuota: inv.vat(),
        importe: inv.total(),
        reason: reason.trim().to_string(),
        user: user.to_string(),
        aeat: AeatStatus::default(),
    };
    r.hash = sha256_hex(&r.canonical());
    let id = r.id;
    db.vf_records.push(r);
    for e in db.entries.iter_mut() {
        if e.invoice_id == Some(invoice_id) {
            e.invoice_id = None;
        }
    }
    Ok(id)
}

// ───────────────────────────── registros XML ─────────────────────────────

/// Descripción de la operación (los tratamientos de la factura).
fn description(inv: &Invoice) -> String {
    if inv.rectifies.is_some() {
        return cut(&format!("Rectificación: {}", if inv.reason.is_empty() { "anulación de la factura" } else { &inv.reason }), 500);
    }
    let names: Vec<String> = inv.lines.iter().map(|l| l.description.clone()).collect();
    let d = format!("Servicios de odontología: {}", names.join(", "));
    cut(&d, 500)
}

/// Desglose: líneas exentas (E1, exentas por el art. 20; el número concreto va en la leyenda de cada factura) y, si las hubiera, sujetas por tipo de IVA.
fn breakdown(inv: &Invoice) -> String {
    let mut groups: Vec<(f64, f64, f64)> = vec![]; // (tipo, base, cuota)
    for l in &inv.lines {
        match groups.iter_mut().find(|g| (g.0 - l.vat_pct).abs() < 0.001) {
            Some(g) => {
                g.1 += l.base();
                g.2 += l.vat();
            }
            None => groups.push((l.vat_pct, l.base(), l.vat())),
        }
    }
    let mut x = String::new();
    for (rate, base, quota) in groups {
        x += "<sum1:DetalleDesglose><sum1:Impuesto>01</sum1:Impuesto><sum1:ClaveRegimen>01</sum1:ClaveRegimen>";
        if rate.abs() < 0.001 {
            x += &format!("<sum1:OperacionExenta>E1</sum1:OperacionExenta><sum1:BaseImponibleOimporteNoSujeto>{}</sum1:BaseImponibleOimporteNoSujeto>", amount(base));
        } else {
            x += &format!(
                "<sum1:CalificacionOperacion>S1</sum1:CalificacionOperacion><sum1:TipoImpositivo>{}</sum1:TipoImpositivo><sum1:BaseImponibleOimporteNoSujeto>{}</sum1:BaseImponibleOimporteNoSujeto><sum1:CuotaRepercutida>{}</sum1:CuotaRepercutida>",
                amount(rate),
                amount(base),
                amount(quota)
            );
        }
        x += "</sum1:DetalleDesglose>";
    }
    x
}

fn chaining(db: &Db, prev: Option<Item>) -> String {
    match prev {
        Some(p) => format!(
            "<sum1:Encadenamiento><sum1:RegistroAnterior><sum1:IDEmisorFactura>{}</sum1:IDEmisorFactura><sum1:NumSerieFactura>{}</sum1:NumSerieFactura><sum1:FechaExpedicionFactura>{}</sum1:FechaExpedicionFactura><sum1:Huella>{}</sum1:Huella></sum1:RegistroAnterior></sum1:Encadenamiento>",
            p.issuer(&db.config.nif),
            esc(&p.num_serie()),
            p.fecha().format("%d-%m-%Y"),
            p.hash()
        ),
        None => "<sum1:Encadenamiento><sum1:PrimerRegistro>S</sum1:PrimerRegistro></sum1:Encadenamiento>".into(),
    }
}

fn system(db: &Db, cfg: &VfConfig) -> String {
    let c = &db.config;
    let (pname, pnif) = if cfg.producer_nif.trim().is_empty() { (c.name.clone(), c.nif.clone()) } else { (cfg.producer_name.clone(), cfg.producer_nif.clone()) };
    format!(
        "<sum1:SistemaInformatico><sum1:NombreRazon>{}</sum1:NombreRazon><sum1:NIF>{}</sum1:NIF><sum1:NombreSistemaInformatico>VeodarDental</sum1:NombreSistemaInformatico><sum1:IdSistemaInformatico>VD</sum1:IdSistemaInformatico><sum1:Version>{}</sum1:Version><sum1:NumeroInstalacion>{}</sum1:NumeroInstalacion><sum1:TipoUsoPosibleSoloVerifactu>S</sum1:TipoUsoPosibleSoloVerifactu><sum1:TipoUsoPosibleMultiOT>N</sum1:TipoUsoPosibleMultiOT><sum1:IndicadorMultiplesOT>N</sum1:IndicadorMultiplesOT></sum1:SistemaInformatico>",
        esc(&cut(&pname, 120)),
        nif_norm(&pnif),
        env!("CARGO_PKG_VERSION"),
        esc(&cut(if cfg.installation.is_empty() { "1" } else { &cfg.installation }, 100))
    )
}

/// Registro de alta (elemento sum1:RegistroAlta) de una factura o de su subsanación.
pub fn alta_xml(db: &Db, cfg: &VfConfig, inv: &Invoice, prev: Option<Item>, rec: Option<&VfRecord>) -> String {
    let c = &db.config;
    let issuer = nif_norm(if inv.issuer_nif.is_empty() { &c.nif } else { &inv.issuer_nif });
    let date = |i: &Invoice| i.date.format("%d-%m-%Y").to_string();
    let mut x = String::from("<sum1:RegistroAlta><sum1:IDVersion>1.0</sum1:IDVersion>");
    x += &format!("<sum1:IDFactura><sum1:IDEmisorFactura>{issuer}</sum1:IDEmisorFactura><sum1:NumSerieFactura>{}</sum1:NumSerieFactura><sum1:FechaExpedicionFactura>{}</sum1:FechaExpedicionFactura></sum1:IDFactura>", esc(&inv.code()), date(inv));
    x += &format!("<sum1:NombreRazonEmisor>{}</sum1:NombreRazonEmisor>", esc(&cut(&c.name, 120)));
    if let Some(r) = rec {
        if r.subsanacion {
            x += "<sum1:Subsanacion>S</sum1:Subsanacion>";
        }
        if !r.rechazo_previo.is_empty() {
            x += &format!("<sum1:RechazoPrevio>{}</sum1:RechazoPrevio>", r.rechazo_previo);
        }
    }
    let kind = inv.kind();
    x += &format!("<sum1:TipoFactura>{kind}</sum1:TipoFactura>");
    if kind.starts_with('R') {
        x += "<sum1:TipoRectificativa>I</sum1:TipoRectificativa>";
        if let Some(orig) = inv.rectifies.and_then(|id| db.invoice(id)) {
            x += &format!(
                "<sum1:FacturasRectificadas><sum1:IDFacturaRectificada><sum1:IDEmisorFactura>{issuer}</sum1:IDEmisorFactura><sum1:NumSerieFactura>{}</sum1:NumSerieFactura><sum1:FechaExpedicionFactura>{}</sum1:FechaExpedicionFactura></sum1:IDFacturaRectificada></sum1:FacturasRectificadas>",
                esc(&orig.code()),
                date(orig)
            );
        }
    }
    x += &format!("<sum1:DescripcionOperacion>{}</sum1:DescripcionOperacion>", esc(&description(inv)));
    if matches!(kind, "F1" | "R1") && !inv.client_nif.trim().is_empty() {
        x += &format!("<sum1:Destinatarios><sum1:IDDestinatario><sum1:NombreRazon>{}</sum1:NombreRazon><sum1:NIF>{}</sum1:NIF></sum1:IDDestinatario></sum1:Destinatarios>", esc(&cut(&inv.client_name, 120)), nif_norm(&inv.client_nif));
    }
    x += &format!("<sum1:Desglose>{}</sum1:Desglose>", breakdown(inv));
    x += &format!("<sum1:CuotaTotal>{}</sum1:CuotaTotal><sum1:ImporteTotal>{}</sum1:ImporteTotal>", amount(inv.vat()), amount(inv.total()));
    x += &chaining(db, prev);
    x += &system(db, cfg);
    let (ts, hash) = match rec {
        Some(r) => (r.gen_ts.clone(), r.hash.clone()),
        None => (gen_ts(inv), inv.hash.clone()),
    };
    x += &format!("<sum1:FechaHoraHusoGenRegistro>{ts}</sum1:FechaHoraHusoGenRegistro><sum1:TipoHuella>01</sum1:TipoHuella><sum1:Huella>{hash}</sum1:Huella></sum1:RegistroAlta>");
    x
}

/// Registro de anulación (elemento sum1:RegistroAnulacion).
pub fn anulacion_xml(db: &Db, cfg: &VfConfig, r: &VfRecord, prev: Option<Item>) -> String {
    let mut x = String::from("<sum1:RegistroAnulacion><sum1:IDVersion>1.0</sum1:IDVersion>");
    x += &format!(
        "<sum1:IDFactura><sum1:IDEmisorFacturaAnulada>{}</sum1:IDEmisorFacturaAnulada><sum1:NumSerieFacturaAnulada>{}</sum1:NumSerieFacturaAnulada><sum1:FechaExpedicionFacturaAnulada>{}</sum1:FechaExpedicionFacturaAnulada></sum1:IDFactura>",
        r.issuer_nif,
        esc(&r.num_serie),
        r.fecha.format("%d-%m-%Y")
    );
    if r.sin_registro_previo {
        x += "<sum1:SinRegistroPrevio>S</sum1:SinRegistroPrevio>";
    }
    if r.rechazo_previo == "S" {
        x += "<sum1:RechazoPrevio>S</sum1:RechazoPrevio>";
    }
    x += &chaining(db, prev);
    x += &system(db, cfg);
    x += &format!("<sum1:FechaHoraHusoGenRegistro>{}</sum1:FechaHoraHusoGenRegistro><sum1:TipoHuella>01</sum1:TipoHuella><sum1:Huella>{}</sum1:Huella></sum1:RegistroAnulacion>", r.gen_ts, r.hash);
    x
}

/// XML de un eslabón de la cadena.
fn item_xml(db: &Db, cfg: &VfConfig, it: Item, prev: Option<Item>) -> String {
    match it {
        Item::Inv(inv) => alta_xml(db, cfg, inv, prev, None),
        Item::Rec(r) => match r.kind {
            RecKind::Anulacion => anulacion_xml(db, cfg, r, prev),
            RecKind::Subsanacion => match db.invoice(r.invoice_id) {
                Some(inv) => alta_xml(db, cfg, inv, prev, Some(r)),
                None => String::new(),
            },
        },
    }
}

/// Facturas en el orden de la cadena (por momento de generación).
pub fn chain_order(db: &Db) -> Vec<&Invoice> {
    let mut list: Vec<&Invoice> = db.invoices.iter().collect();
    list.sort_by_key(|i| (i.created, i.id));
    list
}

/// Registros pendientes de remitir (en orden de la cadena), con su registro anterior.
pub fn pending(db: &Db, cfg: &VfConfig) -> Vec<(Key, Option<Key>)> {
    let list = chain(db);
    let mut out = vec![];
    for (k, it) in list.iter().enumerate() {
        let after = cfg.since.map(|s| it.created() >= s).unwrap_or(true);
        if after && it.aeat().pending() {
            out.push((it.key(), if k > 0 { Some(list[k - 1].key()) } else { None }));
        }
    }
    out
}

pub fn find(db: &Db, k: Key) -> Option<Item<'_>> {
    match k {
        Key::Inv(id) => db.invoice(id).map(Item::Inv),
        Key::Rec(id) => db.vf_records.iter().find(|r| r.id == id).map(Item::Rec),
    }
}

/// Sobre SOAP con un lote de registros (como máximo 1000).
pub fn envelope(db: &Db, cfg: &VfConfig, batch: &[(Key, Option<Key>)]) -> String {
    let c = &db.config;
    let mut x =
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><soapenv:Envelope xmlns:soapenv=\"http://schemas.xmlsoap.org/soap/envelope/\" xmlns:sum=\"{NS_LR}\" xmlns:sum1=\"{NS_SF}\"><soapenv:Header/><soapenv:Body><sum:RegFactuSistemaFacturacion>");
    x += &format!("<sum:Cabecera><sum1:ObligadoEmision><sum1:NombreRazon>{}</sum1:NombreRazon><sum1:NIF>{}</sum1:NIF></sum1:ObligadoEmision></sum:Cabecera>", esc(&cut(&c.name, 120)), nif_norm(&c.nif));
    for (k, prev) in batch.iter().take(1000) {
        if let Some(it) = find(db, *k) {
            let p = prev.and_then(|p| find(db, p));
            x += &format!("<sum:RegistroFactura>{}</sum:RegistroFactura>", item_xml(db, cfg, it, p));
        }
    }
    x += "</sum:RegFactuSistemaFacturacion></soapenv:Body></soapenv:Envelope>";
    x
}

// ───────────────────────────── respuesta ─────────────────────────────

/// Textos de todos los elementos con ese nombre local (sin prefijo de espacio de nombres).
fn texts<'a>(xml: &'a str, local: &str) -> Vec<&'a str> {
    let mut out = vec![];
    let mut rest = xml;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        if tag.starts_with('/') || tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        let name = tag.split_whitespace().next().unwrap_or("").trim_end_matches('/');
        let lname = name.rsplit(':').next().unwrap_or(name);
        if lname == local && !tag.ends_with('/') {
            let body = &rest[(end + 1).min(rest.len())..];
            let close = format!("</{name}>");
            if let Some(j) = body.find(&close) {
                out.push(&body[..j]);
            }
        }
    }
    out
}

fn first<'a>(xml: &'a str, local: &str) -> Option<&'a str> {
    texts(xml, local).into_iter().next().map(|s| s.trim())
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct LineResult {
    pub num_serie: String,
    /// Alta o Anulacion.
    pub operation: String,
    pub state: String,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Reply {
    /// Correcto, ParcialmenteCorrecto o Incorrecto.
    pub state: String,
    pub csv: String,
    pub wait_secs: u64,
    pub lines: Vec<LineResult>,
}

/// Lee la respuesta de la AEAT (o el error SOAP).
pub fn parse_reply(xml: &str) -> Result<Reply, String> {
    if let Some(f) = first(xml, "faultstring") {
        return Err(format!("La AEAT rechazó el envío: {}", unescape(f)));
    }
    let state = first(xml, "EstadoEnvio").ok_or("La respuesta de la AEAT no tiene el formato esperado.")?.to_string();
    let lines = texts(xml, "RespuestaLinea")
        .iter()
        .map(|l| LineResult {
            num_serie: first(l, "NumSerieFactura").or_else(|| first(l, "NumSerieFacturaAnulada")).map(unescape).unwrap_or_default(),
            operation: first(l, "TipoOperacion").unwrap_or("").to_string(),
            state: first(l, "EstadoRegistro").unwrap_or("").to_string(),
            code: first(l, "CodigoErrorRegistro").unwrap_or("").to_string(),
            message: first(l, "DescripcionErrorRegistro").map(unescape).unwrap_or_default(),
        })
        .collect();
    Ok(Reply { state, csv: first(xml, "CSV").unwrap_or("").to_string(), wait_secs: first(xml, "TiempoEsperaEnvio").and_then(|t| t.parse().ok()).unwrap_or(60), lines })
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

pub fn status_mut(db: &mut Db, k: Key) -> Option<&mut AeatStatus> {
    match k {
        Key::Inv(id) => db.invoices.iter_mut().find(|i| i.id == id).map(|i| &mut i.aeat),
        Key::Rec(id) => db.vf_records.iter_mut().find(|r| r.id == id).map(|r| &mut r.aeat),
    }
}

/// Apunta la respuesta en cada registro del lote. Devuelve (correctos, con errores, rechazados).
pub fn apply_reply(db: &mut Db, batch: &[(Key, Option<Key>)], r: &Reply) -> (usize, usize, usize) {
    let now = crate::model::now();
    let (mut ok, mut warn, mut bad) = (0, 0, 0);
    for (k, _) in batch {
        let Some((code, op)) = find(db, *k).map(|it| (it.num_serie(), it.operation())) else { continue };
        let line = r.lines.iter().find(|l| l.num_serie == code && (l.operation.is_empty() || l.operation == op)).cloned();
        let Some(st) = status_mut(db, *k) else { continue };
        st.tries += 1;
        st.sent_at = Some(now);
        match line {
            Some(l) => {
                // un registro ya presentado antes (duplicado) cuenta como registrado
                let dup = l.code == "3000";
                st.state = if dup { "Correcto".into() } else { l.state.clone() };
                st.code = l.code.clone();
                st.message = l.message.clone();
                if st.sent_ok() {
                    st.csv = r.csv.clone();
                }
            }
            None => {
                st.state = if r.state == "Correcto" { "Correcto".into() } else { "Incorrecto".into() };
                st.message = if r.state == "Correcto" { String::new() } else { "La AEAT no devolvió el resultado de este registro.".into() };
                if st.state == "Correcto" {
                    st.csv = r.csv.clone();
                }
            }
        }
        match st.state.as_str() {
            "Correcto" => ok += 1,
            "AceptadoConErrores" => warn += 1,
            _ => bad += 1,
        }
    }
    (ok, warn, bad)
}

/// Marca un lote que no llegó a la AEAT (se reintentará).
pub fn mark_connection_error(db: &mut Db, batch: &[(Key, Option<Key>)], msg: &str) {
    for (k, _) in batch {
        if let Some(st) = status_mut(db, *k) {
            st.state = "ErrorConexion".into();
            st.message = msg.to_string();
            st.tries += 1;
        }
    }
}

// ───────────────────────────── envío ─────────────────────────────

/// Certificado de la clínica para identificarse ante la AEAT.
pub struct Identity {
    pub tls: Arc<rustls::ClientConfig>,
    pub subject: String,
}

/// Abre el certificado (.p12/.pfx) y prepara la conexión segura con él.
pub fn identity(p12: &[u8], password: &str) -> Result<Identity, String> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    let (key, chain, subject) = crate::firma::load_p12_der(p12, password)?;
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let certs: Vec<CertificateDer<'static>> = chain.into_iter().map(CertificateDer::from).collect();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_client_auth_cert(certs, PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)))
        .map_err(|e| format!("El certificado no sirve para identificarse: {e}"))?;
    Ok(Identity { tls: Arc::new(tls), subject })
}

/// Envía un sobre SOAP y devuelve la respuesta en texto.
pub fn post(url: &str, body: &str, id: Option<&Identity>) -> Result<String, String> {
    let mut b = ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(15)).timeout(std::time::Duration::from_secs(120));
    if let Some(i) = id {
        b = b.tls_config(i.tls.clone());
    }
    let agent = b.build();
    match agent.post(url).set("Content-Type", "text/xml; charset=UTF-8").set("SOAPAction", "\"\"").send_string(body) {
        Ok(r) => r.into_string().map_err(|e| format!("Respuesta ilegible: {e}")),
        // los errores SOAP llegan con código 500 y un <faultstring>
        Err(ureq::Error::Status(code, r)) => {
            let t = r.into_string().unwrap_or_default();
            if t.contains("faultstring") || t.contains("EstadoEnvio") { Ok(t) } else { Err(format!("La AEAT respondió con el código {code}.")) }
        }
        Err(ureq::Error::Transport(t)) => Err(match t.kind() {
            ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed => "No se puede conectar con la AEAT (¿hay conexión a internet?).".into(),
            _ => format!("Fallo de conexión con la AEAT: {t}"),
        }),
    }
}

/// Resultado de un envío, para mostrarlo.
#[derive(Clone, Debug, Default)]
pub struct SendSummary {
    pub sent: usize,
    pub ok: usize,
    pub warn: usize,
    pub bad: usize,
    pub csv: String,
    pub wait_secs: u64,
    pub error: String,
}

/// Envía todos los pendientes (en lotes de 1000) y apunta los resultados.
pub fn send_pending(db: &mut Db, id: Option<&Identity>) -> SendSummary {
    let cfg = db.verifactu.clone();
    let mut sum = SendSummary { wait_secs: 60, ..Default::default() };
    let all = pending(db, &cfg);
    for batch in all.chunks(1000) {
        let body = envelope(db, &cfg, batch);
        match post(&cfg.url(), &body, id).and_then(|t| parse_reply(&t)) {
            Ok(r) => {
                let (ok, warn, bad) = apply_reply(db, batch, &r);
                sum.sent += batch.len();
                sum.ok += ok;
                sum.warn += warn;
                sum.bad += bad;
                sum.csv = r.csv.clone();
                sum.wait_secs = r.wait_secs;
            }
            Err(e) => {
                mark_connection_error(db, batch, &e);
                sum.error = e;
                break;
            }
        }
    }
    sum
}

/// Matriz del código QR (ISO/IEC 18004, corrección de errores M), o None si no cabe.
pub fn qr_matrix(text: &str) -> Option<(usize, Vec<bool>)> {
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M).ok()?;
    let w = code.width();
    let bits = code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect();
    Some((w, bits))
}

/// Código QR como SVG (para la factura impresa).
pub fn qr_svg(text: &str, size_mm: f32) -> String {
    let Some((w, bits)) = qr_matrix(text) else { return String::new() };
    let q = 4; // margen de silencio
    let n = w + 2 * q;
    let mut path = String::new();
    for (i, dark) in bits.iter().enumerate() {
        if *dark {
            path += &format!("M{},{}h1v1h-1z", i % w + q, i / w + q);
        }
    }
    format!("<svg xmlns='http://www.w3.org/2000/svg' width='{size_mm}mm' height='{size_mm}mm' viewBox='0 0 {n} {n}' shape-rendering='crispEdges'><rect width='{n}' height='{n}' fill='#fff'/><path d='{path}' fill='#000'/></svg>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_computer_sends_to_the_aeat() {
        let mut c = VfConfig::default();
        assert!(c.may_send("RECEPCION") && c.may_send("DESPACHO"), "sin elegido, cualquiera (y queda elegido)");
        c.sender = "RECEPCION".into();
        assert!(c.may_send("RECEPCION"));
        assert!(!c.may_send("DESPACHO"), "los demás no envían");
    }

    /// Ejemplos del documento de la AEAT «Detalle de las especificaciones técnicas para generación de la huella».
    #[test]
    fn official_hash_examples() {
        let first = "IDEmisorFactura=89890001K&NumSerieFactura=12345678/G33&FechaExpedicionFactura=01-01-2024&TipoFactura=F1&CuotaTotal=12.35&ImporteTotal=123.45&Huella=&FechaHoraHusoGenRegistro=2024-01-01T19:20:30+01:00";
        assert_eq!(sha256_hex(first), "3C464DAF61ACB827C65FDA19F352A4E3BDC2C640E9E9FC4CC058073F38F12F60");
        let second = "IDEmisorFactura=89890001K&NumSerieFactura=12345679/G34&FechaExpedicionFactura=01-01-2024&TipoFactura=F1&CuotaTotal=12.35&ImporteTotal=123.45&Huella=3C464DAF61ACB827C65FDA19F352A4E3BDC2C640E9E9FC4CC058073F38F12F60&FechaHoraHusoGenRegistro=2024-01-01T19:20:35+01:00";
        assert_eq!(sha256_hex(second), "F7B94CFD8924EDFF273501B01EE5153E4CE8F259766F88CF6ACB8935802A2B97");
    }

    fn db_with_invoices() -> Db {
        let mut db = Db::sample(crate::model::today());
        db.verifactu = VfConfig { enabled: true, installation: "INST-1".into(), ..Default::default() };
        db
    }

    /// La referencia de exención se guarda en cada factura al emitirla: cambiar
    /// la de la clínica solo afecta a las nuevas y las ya emitidas conservan la
    /// que imprimieron (las anteriores a esta opción, 20.Uno.3º).
    #[test]
    fn the_exemption_article_stays_with_each_invoice() {
        use crate::model::VatExemption;
        let mut db = db_with_invoices();
        assert_eq!(db.config.vat_exemption, VatExemption::Art20Uno3, "por defecto no cambia nada");
        let pid = db.patients.iter().find(|p| !p.nif.is_empty()).map(|p| p.id).unwrap();
        let today = crate::model::today();
        let code = db.treatments.iter().find(|t| t.active).map(|t| t.code.clone()).unwrap();
        let issue = |db: &mut Db| {
            let e = db.add_entry(pid, &code, vec![], vec![], crate::model::TxState::Realizado, 1, today);
            db.issue_invoice(pid, &[e], today).unwrap()
        };
        // factura emitida antes de existir la opción: sin referencia guardada
        let old = issue(&mut db);
        let json = serde_json::to_string(db.invoice(old).unwrap()).unwrap();
        let mut legacy: crate::model::Invoice = serde_json::from_str(&json.replace(r#","exemption":"Art20Uno3""#, "")).unwrap();
        legacy.exemption = None;
        assert_eq!(legacy.exemption_legend(), "Operación exenta de IVA según el art. 20.Uno.3º de la Ley 37/1992 (asistencia sanitaria).", "el texto que imprimía");
        assert_eq!(db.invoice(old).unwrap().exemption_legend(), legacy.exemption_legend(), "con la opción por defecto no cambia nada");
        // la clínica cambia la referencia: solo las nuevas la llevan
        db.config.vat_exemption = VatExemption::Art20Uno5;
        let new = issue(&mut db);
        assert!(db.invoice(new).unwrap().exemption_legend().contains("art. 20.Uno.5º de la Ley 37/1992"));
        assert!(db.invoice(old).unwrap().exemption_legend().contains("20.Uno.3º"), "la factura emitida no cambia");
        // la rectificativa cita la misma que la factura que rectifica
        let r = db.rectify_invoice(old, "Error").unwrap();
        assert!(db.invoice(r).unwrap().exemption_legend().contains("20.Uno.3º"));
        // la huella no depende de la referencia y la cadena sigue íntegra
        assert!(db.verify_chain().is_ok());
        // datos guardados por versiones anteriores: sin la opción, 20.Uno.3º
        let cfg: crate::model::ClinicConfig = serde_json::from_value({
            let mut v = serde_json::to_value(&db.config).unwrap();
            v.as_object_mut().unwrap().remove("vat_exemption");
            v
        })
        .unwrap();
        assert_eq!(cfg.vat_exemption, VatExemption::Art20Uno3);
    }

    #[test]
    fn new_invoices_keep_time_zone_and_type() {
        let mut db = db_with_invoices();
        let pid = db.patients.iter().find(|p| !p.nif.is_empty()).map(|p| p.id).unwrap();
        let today = crate::model::today();
        let code = db.treatments.iter().find(|t| t.active).map(|t| t.code.clone()).unwrap();
        let e = db.add_entry(pid, &code, vec![], vec![], crate::model::TxState::Realizado, 1, today);
        let id = db.issue_invoice(pid, &[e], today).unwrap();
        let inv = db.invoice(id).unwrap();
        assert_eq!(inv.kind(), "F1");
        assert_eq!(inv.gen_ts.len(), 25, "{}", inv.gen_ts);
        assert!(inv.canonical(&db.config.nif).ends_with(&format!("FechaHoraHusoGenRegistro={}", inv.gen_ts)));
        assert!(db.verify_chain().is_ok());
        // rectificativa: R1 que apunta a la original
        let r = db.rectify_invoice(id, "Error en el importe").unwrap();
        assert_eq!(db.invoice(r).unwrap().kind(), "R1");
        let x = alta_xml(&db, &db.verifactu, db.invoice(r).unwrap(), db.invoice(id).map(Item::Inv), None);
        assert!(x.contains("<sum1:TipoFactura>R1</sum1:TipoFactura><sum1:TipoRectificativa>I</sum1:TipoRectificativa><sum1:FacturasRectificadas>"));
        assert!(x.contains(&format!("<sum1:NumSerieFactura>{}</sum1:NumSerieFactura>", db.invoice(id).unwrap().code())));
        assert!(db.verify_chain().is_ok());
    }

    #[test]
    fn alta_record_follows_schema_order() {
        let db = db_with_invoices();
        let list = chain_order(&db);
        let first = list[0];
        let x = alta_xml(&db, &db.verifactu, first, None, None);
        let order = ["IDVersion", "IDFactura", "NombreRazonEmisor", "TipoFactura", "DescripcionOperacion", "Desglose", "CuotaTotal", "ImporteTotal", "Encadenamiento", "SistemaInformatico", "FechaHoraHusoGenRegistro", "TipoHuella", "Huella"];
        let mut pos = 0;
        for tag in order {
            let p = x[pos..].find(&format!("<sum1:{tag}>")).unwrap_or_else(|| panic!("falta o está fuera de orden: {tag}"));
            pos += p;
        }
        assert!(x.contains("<sum1:PrimerRegistro>S</sum1:PrimerRegistro>"));
        assert!(x.contains("<sum1:OperacionExenta>E1</sum1:OperacionExenta>"), "asistencia sanitaria exenta");
        assert!(x.contains(&format!("<sum1:Huella>{}</sum1:Huella></sum1:RegistroAlta>", first.hash)));
        // el segundo enlaza con el primero
        let x2 = alta_xml(&db, &db.verifactu, list[1], Some(Item::Inv(first)), None);
        assert!(x2.contains(&format!("<sum1:RegistroAnterior><sum1:IDEmisorFactura>{}</sum1:IDEmisorFactura><sum1:NumSerieFactura>{}</sum1:NumSerieFactura>", nif_norm(&db.config.nif), first.code())));
        assert!(x2.contains(&format!("<sum1:Huella>{}</sum1:Huella></sum1:RegistroAnterior>", first.hash)));
        let env = envelope(&db, &db.verifactu, &pending(&db, &db.verifactu));
        assert!(env.starts_with("<?xml") && env.contains("<sum:Cabecera><sum1:ObligadoEmision>"));
        assert_eq!(env.matches("<sum:RegistroFactura>").count(), db.invoices.len());
    }

    /// Lote con todos los tipos de registro (alta, rectificativa, subsanación,
    /// reenvío tras rechazo y anulación) para validarlo con el esquema oficial:
    /// `VF_DUMP=/ruta/lote.xml cargo test -- --ignored dump_batch` y luego
    /// `xmllint --schema SuministroLR.xsd lote.xml`.
    #[test]
    #[ignore]
    fn dump_batch_for_schema_validation() {
        let mut db = db_with_invoices();
        let ids: Vec<u32> = chain_order(&db).iter().map(|i| i.id).collect();
        db.rectify_invoice(ids[0], "Prueba").unwrap();
        for (k, id) in ids.iter().enumerate() {
            db.invoices.iter_mut().find(|i| i.id == *id).unwrap().aeat.state = if k == 1 { "Incorrecto".into() } else { "AceptadoConErrores".into() };
        }
        if let Some(i) = db.invoices.iter_mut().find(|i| i.rectifies.is_some()) {
            i.aeat.state = "Correcto".into();
        }
        create_correction(&mut db, ids[0], "t").unwrap(); // subsanación
        create_correction(&mut db, ids[1], "t").unwrap(); // reenvío tras rechazo
        create_cancellation(&mut db, ids[2], "Emitida por error", "t").unwrap();
        db.verifactu.since = None;
        let all: Vec<(Key, Option<Key>)> = {
            let list = chain(&db);
            list.iter().enumerate().map(|(k, it)| (it.key(), if k > 0 { Some(list[k - 1].key()) } else { None })).collect()
        };
        let env = envelope(&db, &db.verifactu, &all);
        let start = env.find("<sum:RegFactuSistemaFacturacion>").unwrap();
        let end = env.find("</soapenv:Body>").unwrap();
        let inner = env[start..end].replacen("<sum:RegFactuSistemaFacturacion>", &format!("<sum:RegFactuSistemaFacturacion xmlns:sum=\"{NS_LR}\" xmlns:sum1=\"{NS_SF}\">"), 1);
        let path = std::env::var("VF_DUMP").unwrap_or_else(|_| std::env::temp_dir().join("vf_lote.xml").display().to_string());
        std::fs::write(&path, format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>{inner}")).unwrap();
        println!("LOTE {path}");
    }

    #[test]
    fn corrections_and_cancellations_join_the_chain() {
        let mut db = db_with_invoices();
        let ids: Vec<u32> = chain_order(&db).iter().map(|i| i.id).collect();
        // sin respuesta de la AEAT todavía no se puede corregir
        assert!(create_correction(&mut db, ids[0], "t").is_err());
        db.invoices.iter_mut().find(|i| i.id == ids[0]).unwrap().aeat.state = "AceptadoConErrores".into();
        db.invoices.iter_mut().find(|i| i.id == ids[1]).unwrap().aeat.state = "Incorrecto".into();
        // aceptada con errores → subsanación (S, sin rechazo previo)
        let s = create_correction(&mut db, ids[0], "Dirección").unwrap();
        let rs = db.vf_records.iter().find(|r| r.id == s).unwrap().clone();
        assert!(rs.subsanacion && rs.rechazo_previo.is_empty());
        // rechazada → nuevo envío con RechazoPrevio S
        let r2 = create_correction(&mut db, ids[1], "Dirección").unwrap();
        let rr = db.vf_records.iter().find(|r| r.id == r2).unwrap().clone();
        assert!(!rr.subsanacion && rr.rechazo_previo == "S");
        assert_eq!(rr.prev_hash, rs.hash, "cada registro nuevo enlaza con el último de la cadena");
        // anulación de una factura registrada
        let inv3 = ids[2];
        db.invoices.iter_mut().find(|i| i.id == inv3).unwrap().aeat.state = "Correcto".into();
        let entries_before = db.entries.iter().filter(|e| e.invoice_id == Some(inv3)).count();
        assert!(create_cancellation(&mut db, inv3, " ", "t").is_err(), "pide el motivo");
        let a = create_cancellation(&mut db, inv3, "Emitida por error", "t").unwrap();
        let ra = db.vf_records.iter().find(|r| r.id == a).unwrap().clone();
        assert!(!ra.sin_registro_previo);
        assert!(ra.canonical().starts_with(&format!("IDEmisorFacturaAnulada={}&NumSerieFacturaAnulada=", ra.issuer_nif)));
        assert!(cancelled(&db, inv3));
        assert!(entries_before == 0 || db.entries.iter().all(|e| e.invoice_id != Some(inv3)), "los tratamientos quedan libres");
        assert!(create_cancellation(&mut db, inv3, "otra vez", "t").is_err());
        assert!(create_correction(&mut db, inv3, "t").is_err(), "una factura anulada no se corrige");
        // la cadena completa (facturas y registros) sigue siendo íntegra
        assert_eq!(db.verify_chain().unwrap(), db.invoices.len() + 3);
        // una factura nueva enlaza con el último registro (la anulación)
        let pid = db.patients.iter().find(|p| !p.nif.is_empty()).map(|p| p.id).unwrap();
        let code = db.treatments.iter().find(|t| t.active).map(|t| t.code.clone()).unwrap();
        let e = db.add_entry(pid, &code, vec![], vec![], crate::model::TxState::Realizado, 1, crate::model::today());
        let n = db.issue_invoice(pid, &[e], crate::model::today()).unwrap();
        assert_eq!(db.invoice(n).unwrap().prev_hash, ra.hash);
        assert!(db.verify_chain().is_ok());
        // el XML lleva las marcas oficiales
        let xs = alta_xml(&db, &db.verifactu, db.invoice(ids[0]).unwrap(), None, Some(&rs));
        assert!(xs.contains("</sum1:NombreRazonEmisor><sum1:Subsanacion>S</sum1:Subsanacion><sum1:TipoFactura>"));
        assert!(xs.contains(&format!("<sum1:Huella>{}</sum1:Huella></sum1:RegistroAlta>", rs.hash)));
        let xa = anulacion_xml(&db, &db.verifactu, &ra, None);
        assert!(xa.starts_with("<sum1:RegistroAnulacion><sum1:IDVersion>1.0</sum1:IDVersion><sum1:IDFactura><sum1:IDEmisorFacturaAnulada>"));
        // si se altera un registro posterior, la verificación lo detecta
        db.vf_records[0].importe += 1.0;
        assert!(db.verify_chain().is_err());
    }

    #[test]
    fn qr_url_is_encoded_and_drawable() {
        let db = db_with_invoices();
        let inv = chain_order(&db)[0];
        let u = qr_url(&db.verifactu, inv, &db.config.nif);
        assert!(u.starts_with("https://prewww2.aeat.es/wlpl/TIKE-CONT/ValidarQR?nif="));
        assert!(u.contains(&format!("&numserie={}", inv.code())));
        assert_eq!(url_encode("12345678/G33 &x"), "12345678%2FG33%20%26x");
        let (w, bits) = qr_matrix(&u).unwrap();
        assert_eq!(bits.len(), w * w);
        assert!(qr_svg(&u, 35.0).starts_with("<svg"));
    }

    const REPLY: &str = r#"<env:Envelope xmlns:env="http://schemas.xmlsoap.org/soap/envelope/"><env:Body><tikR:RespuestaRegFactuSistemaFacturacion xmlns:tikR="x" xmlns:tik="y"><tikR:CSV>A-ABCDEF1234567</tikR:CSV><tikR:Cabecera><tik:ObligadoEmision><tik:NombreRazon>C</tik:NombreRazon><tik:NIF>B00000000</tik:NIF></tik:ObligadoEmision></tikR:Cabecera><tikR:TiempoEsperaEnvio>60</tikR:TiempoEsperaEnvio><tikR:EstadoEnvio>ParcialmenteCorrecto</tikR:EstadoEnvio><tikR:RespuestaLinea><tikR:IDFactura><tik:IDEmisorFactura>B00000000</tik:IDEmisorFactura><tik:NumSerieFactura>NUM1</tik:NumSerieFactura><tik:FechaExpedicionFactura>01-01-2026</tik:FechaExpedicionFactura></tikR:IDFactura><tikR:Operacion><tik:TipoOperacion>Alta</tik:TipoOperacion></tikR:Operacion><tikR:EstadoRegistro>Correcto</tikR:EstadoRegistro></tikR:RespuestaLinea><tikR:RespuestaLinea><tikR:IDFactura><tik:IDEmisorFactura>B00000000</tik:IDEmisorFactura><tik:NumSerieFactura>NUM2</tik:NumSerieFactura><tik:FechaExpedicionFactura>01-01-2026</tik:FechaExpedicionFactura></tikR:IDFactura><tikR:EstadoRegistro>Incorrecto</tikR:EstadoRegistro><tikR:CodigoErrorRegistro>1100</tikR:CodigoErrorRegistro><tikR:DescripcionErrorRegistro>Valor o tipo incorrecto del campo: NIF &amp; nombre</tikR:DescripcionErrorRegistro></tikR:RespuestaLinea></tikR:RespuestaRegFactuSistemaFacturacion></env:Body></env:Envelope>"#;

    #[test]
    fn reads_aeat_reply_and_faults() {
        let r = parse_reply(REPLY).unwrap();
        assert_eq!(r.state, "ParcialmenteCorrecto");
        assert_eq!(r.csv, "A-ABCDEF1234567");
        assert_eq!(r.wait_secs, 60);
        assert_eq!(r.lines.len(), 2);
        assert_eq!(r.lines[0].operation, "Alta");
        assert_eq!(r.lines[1], LineResult { num_serie: "NUM2".into(), operation: String::new(), state: "Incorrecto".into(), code: "1100".into(), message: "Valor o tipo incorrecto del campo: NIF & nombre".into() });
        let fault = "<soapenv:Envelope><soapenv:Body><soapenv:Fault><faultcode>env:Client</faultcode><faultstring>Codigo[4102].El XML no cumple el esquema</faultstring></soapenv:Fault></soapenv:Body></soapenv:Envelope>";
        assert!(parse_reply(fault).unwrap_err().contains("4102"));
    }

    /// Envío completo contra un servicio de prueba que contesta como la AEAT.
    #[test]
    fn sends_pending_and_records_results() {
        let mut db = db_with_invoices();
        let codes: Vec<String> = chain_order(&db).iter().map(|i| i.code()).collect();
        assert!(codes.len() >= 2);
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let c2 = codes.clone();
        let h = std::thread::spawn(move || {
            let mut rq = server.recv().unwrap();
            let mut body = String::new();
            rq.as_reader().read_to_string(&mut body).unwrap();
            // todas correctas salvo la segunda
            let mut lines = String::new();
            for (k, c) in c2.iter().enumerate() {
                let st = if k == 1 {
                    "<tikR:EstadoRegistro>Incorrecto</tikR:EstadoRegistro><tikR:CodigoErrorRegistro>1100</tikR:CodigoErrorRegistro><tikR:DescripcionErrorRegistro>NIF no identificado</tikR:DescripcionErrorRegistro>"
                } else {
                    "<tikR:EstadoRegistro>Correcto</tikR:EstadoRegistro>"
                };
                lines += &format!("<tikR:RespuestaLinea><tikR:IDFactura><tik:NumSerieFactura>{c}</tik:NumSerieFactura></tikR:IDFactura>{st}</tikR:RespuestaLinea>");
            }
            let resp = format!(
                "<env:Envelope><env:Body><tikR:RespuestaRegFactuSistemaFacturacion><tikR:CSV>CSV123</tikR:CSV><tikR:TiempoEsperaEnvio>60</tikR:TiempoEsperaEnvio><tikR:EstadoEnvio>ParcialmenteCorrecto</tikR:EstadoEnvio>{lines}</tikR:RespuestaRegFactuSistemaFacturacion></env:Body></env:Envelope>"
            );
            rq.respond(tiny_http::Response::from_string(resp)).unwrap();
            body
        });
        db.verifactu.endpoint = format!("http://127.0.0.1:{port}/verifactu");
        let s = send_pending(&mut db, None);
        let body = h.join().unwrap();
        assert!(body.contains("RegFactuSistemaFacturacion") && body.matches("<sum1:RegistroAlta>").count() == codes.len());
        assert_eq!(s.sent, codes.len());
        assert_eq!(s.bad, 1);
        assert_eq!(s.ok, codes.len() - 1);
        let second = db.invoices.iter().find(|i| i.code() == codes[1]).unwrap();
        assert_eq!(second.aeat.state, "Incorrecto");
        assert_eq!(second.aeat.message, "NIF no identificado");
        let first = db.invoices.iter().find(|i| i.code() == codes[0]).unwrap();
        assert_eq!(first.aeat.csv, "CSV123");
        // ya no quedan pendientes (los rechazados no se reenvían solos)
        assert!(pending(&db, &db.verifactu).is_empty());
        // sin conexión: queda para reintentar
        let mut db2 = db_with_invoices();
        db2.verifactu.endpoint = "http://127.0.0.1:9/verifactu".into();
        let s2 = send_pending(&mut db2, None);
        assert!(!s2.error.is_empty());
        assert_eq!(pending(&db2, &db2.verifactu).len(), db2.invoices.len());
    }
}
