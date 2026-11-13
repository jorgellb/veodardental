//! Repositorio de recetas del Sistema de Receta Electrónica Privada (SREP).
//!
//! Sigue el «Diseño técnico e-receta privada – Servicios Repositorios
//! v2.04.1» de los Consejos Generales (médicos, dentistas, podólogos y
//! farmacéuticos): cada sistema de prescripción certificado guarda sus recetas
//! en un repositorio al que llegan las farmacias de toda España a través de
//! Nodofarma (CGCOF) con tres servicios REST/JSON:
//!
//! ```text
//! POST {base}/prescriptions/idFarmacia/{f}/idAcceso/{a}  recetas que se pueden dispensar
//! POST {base}/receta                                     registrar una actividad de la farmacia
//! POST {base}/receta/idFarmacia/{f}/idAcceso/{a}         recetas dispensadas en esa farmacia
//! ```
//!
//! Nodofarma entra cifrado y con reconocimiento mutuo de certificados: este
//! servicio se publica detrás de un proxy TLS (nginx) que comprueba el
//! certificado de Nodofarma y lo indica con la cabecera `X-SSL-Client-Verify`.
//!
//! La cara interna, con la clave de la clínica, la usa el propio programa:
//! `POST /interno/emitir`, `/interno/anular`, `/interno/seguimiento`,
//! `GET /interno/estado` y `POST /interno/parar`.
//!
//! Todo lo que pasa queda en un registro encadenado con huellas SHA-256
//! (custodia, control SC05) junto al fichero del repositorio.

use chrono::{Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

pub const SW_REPOSITORIO: &str = concat!("VeodarDental Repositorio v", env!("CARGO_PKG_VERSION"));
pub const DEFAULT_PORT: u16 = 8898;
/// Una receta caducada se sigue mostrando a la farmacia durante este tiempo.
const CADUCADAS_DIAS: i64 = 90;

// ───────────────────────────── datos ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RPaciente {
    pub nombre: String,
    pub apellidos: String,
    /// DD/MM/AAAA
    pub fecha_nacimiento: String,
    /// 0 tarjeta sanitaria, 1 DNI/NIE/pasaporte, 2 DNI/NIE del representante.
    pub tipo_id: u8,
    #[serde(default)]
    pub cip_tsi: String,
    #[serde(default)]
    pub dni_nie: String,
    #[serde(default)]
    pub dni_nie_representante: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RPrescriptor {
    /// Número de colegiado.
    pub id: String,
    pub nombre: String,
    pub apellidos: String,
    #[serde(default)]
    pub especialidad: String,
    pub email: String,
    pub telefono: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RProducto {
    /// Código nacional (prescripción por marca), hasta 7 cifras.
    #[serde(default)]
    pub cod_producto: String,
    /// 0 medicamento, 1 producto sanitario, 2 nutrición, 3 vacuna individualizada, 4 fórmula magistral.
    pub tipo: u8,
    /// Código DCPF del principio activo (prescripción por principio activo).
    #[serde(default)]
    pub principio_activo: String,
    /// Composición de una fórmula magistral o vacuna sin código nacional.
    #[serde(default)]
    pub composicion: String,
    pub denominacion: String,
    pub estupefaciente: bool,
    pub psicotropo: bool,
    pub dosificacion: String,
    pub forma: String,
    #[serde(default)]
    pub via: String,
    #[serde(default)]
    pub formato: String,
    #[serde(default)]
    pub observaciones: String,
    /// 0 lactantes, 1 niño, 2 adulto.
    #[serde(default)]
    pub destinatario: Option<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RReceta {
    pub id_receta: String,
    pub fecha_ini: NaiveDate,
    pub fecha_fin: NaiveDate,
    pub num_envases: u32,
    #[serde(default)]
    pub dispensados: u32,
    /// Estado guardado (ver [`estado_texto`]); el efectivo depende de la fecha.
    pub estado: u8,
    /// Farmacia que elabora la fórmula o que dispensó por última vez.
    #[serde(default)]
    pub farmacia: String,
    #[serde(default)]
    pub obs_bloqueo: String,
    #[serde(default)]
    pub cn_dispensado: String,
    #[serde(default)]
    pub composicion_dispensada: String,
    #[serde(default)]
    pub fecha_dispensacion: Option<NaiveDate>,
    /// Anulada por el prescriptor: deja de ofrecerse a las farmacias.
    #[serde(default)]
    pub anulada: bool,
}

impl RReceta {
    /// Estado que se ve hoy: las dispensables pasan a «a futuro» o «caducada».
    pub fn estado_en(&self, hoy: NaiveDate) -> u8 {
        if self.anulada {
            return 11;
        }
        match self.estado {
            0 | 1 => {
                if hoy < self.fecha_ini {
                    0
                } else if hoy > self.fecha_fin {
                    5
                } else {
                    1
                }
            }
            8 | 10 if hoy > self.fecha_fin => 5,
            e => e,
        }
    }
    pub fn quedan(&self) -> u32 {
        self.num_envases.saturating_sub(self.dispensados)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RPrescripcion {
    pub id_prescripcion: String,
    /// Identificador del paciente en este repositorio (el mismo en todas sus recetas).
    pub id_acceso: String,
    /// Receta del programa y línea de la que sale.
    pub receta_local: u32,
    pub linea: usize,
    pub doctor_id: u32,
    pub paciente: RPaciente,
    pub fecha_prescripcion: NaiveDate,
    pub id_entidad: String,
    #[serde(default)]
    pub id_centro: String,
    pub prescriptor: RPrescriptor,
    pub producto: RProducto,
    pub toma: f64,
    pub ud_toma: String,
    pub frecuencia: f64,
    pub ud_frecuencia: String,
    pub duracion: u32,
    pub ud_duracion: String,
    #[serde(default)]
    pub observaciones: String,
    /// Huella del PIN de confidencialidad (vacío si no tiene).
    #[serde(default)]
    pub pin_hash: String,
    /// Firma CAdES (base64) del documento.
    #[serde(default)]
    pub firma: String,
    #[serde(default)]
    pub firmante: String,
    /// Documento firmado (JSON).
    #[serde(default)]
    pub documento: String,
    pub recetas: Vec<RReceta>,
    #[serde(default)]
    pub anulada: bool,
    #[serde(default)]
    pub motivo_anulacion: String,
    #[serde(default)]
    pub pruebas: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RAccion {
    pub id_accion_farmacia: String,
    pub id_transaccion: String,
    pub id_receta: String,
    /// 0 bloquear, 1 dispensar, 2 sustituir, 3 anular, 4 elaborar FM, 5 anular FM.
    pub accion: u8,
    pub id_farmacia: String,
    pub fecha_hora: String,
    pub recibida: NaiveDateTime,
    #[serde(default)]
    pub envases: u32,
    #[serde(default)]
    pub cod_producto: String,
    #[serde(default)]
    pub composicion: String,
    #[serde(default)]
    pub causa: Option<u8>,
    #[serde(default)]
    pub descripcion: String,
    #[serde(default)]
    pub observaciones: String,
    pub resultado: String,
    /// Dispensación anulada después por la farmacia.
    #[serde(default)]
    pub deshecha: bool,
    #[serde(default)]
    pub estado_previo: u8,
    #[serde(default)]
    pub cn_previo: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RepoStore {
    #[serde(default)]
    pub prescripciones: Vec<RPrescripcion>,
    #[serde(default)]
    pub acciones: Vec<RAccion>,
}

pub fn estado_texto(e: u8) -> &'static str {
    match e {
        0 => "Dispensable a futuro",
        1 => "Dispensable",
        2 => "Bloqueada cautelarmente",
        3 => "Dispensada",
        4 => "Dispensada con sustitución",
        5 => "Caducada",
        6 => "Pendiente de visado",
        7 => "Visado rechazado",
        8 => "Dispensada parcialmente",
        9 => "Fórmula magistral en elaboración",
        10 => "Dispensada parcialmente con sustitución",
        11 => "Anulada por el prescriptor",
        _ => "Desconocido",
    }
}

pub fn accion_texto(a: u8) -> &'static str {
    match a {
        0 => "Bloqueo cautelar",
        1 => "Dispensación",
        2 => "Dispensación con sustitución",
        3 => "Anulación de la dispensación",
        4 => "Elaboración de fórmula magistral",
        5 => "Anulación de la elaboración",
        _ => "Acción",
    }
}

pub fn causa_bloqueo_texto(c: u8) -> &'static str {
    match c {
        0 => "Dosis superior a la máxima indicada",
        1 => "Posible alergia o intolerancia",
        2 => "Contraindicación",
        3 => "Tratamiento ya finalizado",
        _ => "Otros",
    }
}

/// Identificador aleatorio de 32 caracteres hexadecimales.
pub fn new_id32() -> String {
    use ring::rand::SecureRandom;
    let mut b = [0u8; 16];
    if ring::rand::SystemRandom::new().fill(&mut b).is_err() {
        let seed = format!("{:?}{}", std::time::SystemTime::now(), std::process::id());
        b.copy_from_slice(&Sha256::digest(seed.as_bytes())[..16]);
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn pin_hash(id_prescripcion: &str, pin: &str) -> String {
    let h = Sha256::digest(format!("srep-pin:{id_prescripcion}:{}", pin.trim()).as_bytes());
    h.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn pin_valid(pin: &str) -> bool {
    pin.len() == 4 && pin.chars().all(|c| c.is_ascii_digit())
}

// ───────────────────────────── formato de los servicios ─────────────────────────────

fn dmy(d: NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}

fn sw(sw_nodo: &str) -> Value {
    json!({"swNodo": sw_nodo, "swRepositorio": SW_REPOSITORIO})
}

/// «JSON Resultado Mensaje» (Anexo 3).
pub fn mensaje(code: &str, text: &str, tx: &str, sw_nodo: &str) -> Value {
    json!({"codResultado": code, "message": text, "idTransaccion": tx, "versionSoftware": sw(sw_nodo)})
}

fn paciente_json(p: &RPaciente) -> Value {
    json!({
        "nombre": p.nombre, "apellidos": p.apellidos, "fechaNacimiento": p.fecha_nacimiento,
        "tipoIdPaciente": p.tipo_id, "cipTsi": p.cip_tsi, "dniNie": p.dni_nie, "dniNieRepresentante": p.dni_nie_representante
    })
}

fn receta_json(r: &RReceta, estado: u8, id_accion: Option<&str>) -> Value {
    let mut v = json!({"idReceta": r.id_receta, "fechaIni": dmy(r.fecha_ini), "fechaFin": dmy(r.fecha_fin), "numEnvases": r.num_envases, "estado": estado});
    if let Some(d) = r.fecha_dispensacion {
        v["fechaDispensacion"] = dmy(d).into();
        v["cantidadDispensada"] = r.dispensados.into();
    }
    if !r.cn_dispensado.is_empty() {
        v["cnProductoDispensado"] = r.cn_dispensado.clone().into();
    }
    if !r.composicion_dispensada.is_empty() {
        v["composicion"] = r.composicion_dispensada.clone().into();
    }
    if !r.obs_bloqueo.is_empty() {
        v["observacionesBloqueo"] = r.obs_bloqueo.clone().into();
    }
    if let Some(a) = id_accion {
        v["idAccionFarmacia"] = a.into();
        v["identificadores"] = json!([]);
    }
    v
}

fn prescripcion_json(p: &RPrescripcion, recetas: Vec<Value>, hoy: NaiveDate) -> Value {
    let pr = &p.producto;
    let mut producto = json!({
        "codProducto": pr.cod_producto, "tipoProducto": pr.tipo, "principioActivo": pr.principio_activo,
        "composicion": pr.composicion, "denominacion": pr.denominacion, "esEstupefaciente": pr.estupefaciente,
        "esPsicotropo": pr.psicotropo, "dosificacion": pr.dosificacion, "formaFarmaceutica": pr.forma,
        "viaAdministracion": pr.via, "formato": pr.formato, "observaciones": pr.observaciones
    });
    if let Some(d) = pr.destinatario {
        producto["destinatario"] = d.into();
    }
    let d = &p.prescriptor;
    let mut v = json!({
        "idPrescripcion": p.id_prescripcion,
        "fechaPrescripcion": dmy(p.fecha_prescripcion),
        "idEntidadSanitaria": p.id_entidad,
        "idCentroPrescripcion": p.id_centro,
        "requiereVisado": false,
        "datosPosologia": {"toma": p.toma, "udMedidaToma": p.ud_toma, "frecuencia": p.frecuencia, "udMedidaFrecuencia": p.ud_frecuencia},
        "datosPrescriptor": {
            "idPrescriptor": d.id, "tipoIdPrescriptor": 0, "nombre": d.nombre, "apellidos": d.apellidos,
            "especialidad": d.especialidad, "correoElectronicoPrescriptor": d.email, "telefonoPrescriptor": d.telefono
        },
        "producto": producto,
        "recetas": recetas,
        "duracion": {"duracion": p.duracion, "udMedidaDuracion": p.ud_duracion},
        "observaciones": p.observaciones,
    });
    if let Some(next) = p.recetas.iter().filter(|r| r.estado_en(hoy) == 0).map(|r| r.fecha_ini).min() {
        v["fechaProximaDispensacion"] = dmy(next).into();
    }
    v
}

/// Parámetros de las consultas de Nodofarma.
#[derive(Clone, Debug, Default)]
pub struct Query {
    pub id_farmacia: String,
    pub id_acceso: String,
    pub id_transaccion: String,
    pub id_repositorio: String,
    pub pin: String,
    pub sw_nodo: String,
}

fn tx_valid(tx: &str) -> bool {
    !tx.is_empty() && tx.len() <= 64 && tx.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

type Fail = (u16, &'static str, &'static str);

fn check_query(q: &Query, own_repo: &str) -> Result<(), Fail> {
    if q.id_transaccion.trim().is_empty() {
        return Err((400, "ERR016", "idTransacción nulo o vacío"));
    }
    if !tx_valid(q.id_transaccion.trim()) {
        return Err((400, "ERR029", "IdTransaccion no tiene el formato correcto"));
    }
    if q.sw_nodo.trim().is_empty() {
        return Err((400, "ERR015", "swNodo nulo o vacío"));
    }
    if q.id_farmacia.trim().is_empty() {
        return Err((400, "ERR009", "IdFarmacia nulo o vacío"));
    }
    if q.id_acceso.trim().is_empty() {
        return Err((400, "ERR012", "idAcceso nulo o vacío"));
    }
    if !q.pin.trim().is_empty() && !pin_valid(q.pin.trim()) {
        return Err((400, "ERR018", "PinConfidencialidad no tiene el formato correcto"));
    }
    if !q.id_repositorio.trim().is_empty() && !own_repo.is_empty() && q.id_repositorio.trim() != own_repo {
        return Err((400, "ERR096", "Alguno de los parámetros recibidos no es correcto. No se ha enviado correctamente alguno de los parámetros."));
    }
    Ok(())
}

fn pin_ok(p: &RPrescripcion, pin: &str) -> bool {
    p.pin_hash.is_empty() || (!pin.trim().is_empty() && pin_hash(&p.id_prescripcion, pin) == p.pin_hash)
}

impl RepoStore {
    fn find_receta(&self, id: &str) -> Option<(usize, usize)> {
        self.prescripciones.iter().enumerate().find_map(|(i, p)| p.recetas.iter().position(|r| r.id_receta == id).map(|j| (i, j)))
    }

    /// Servicio 3.1: recetas que el paciente puede llevarse en esta farmacia.
    pub fn consultar(&self, q: &Query, own_repo: &str, hoy: NaiveDate) -> (u16, Value) {
        if let Err((s, c, t)) = check_query(q, own_repo) {
            return (s, mensaje(c, t, &q.id_transaccion, &q.sw_nodo));
        }
        let mut paciente = None;
        let mut out = vec![];
        for p in self.prescripciones.iter().filter(|p| p.id_acceso == q.id_acceso.trim() && !p.anulada && pin_ok(p, &q.pin)) {
            let recs: Vec<Value> = p
                .recetas
                .iter()
                .filter_map(|r| {
                    let e = r.estado_en(hoy);
                    let reciente = e != 5 || hoy - r.fecha_fin <= Duration::days(CADUCADAS_DIAS);
                    let show = (matches!(e, 0 | 1 | 2 | 5 | 6 | 7 | 8 | 10) && reciente) || (e == 9 && r.farmacia == q.id_farmacia.trim());
                    show.then(|| receta_json(r, e, None))
                })
                .collect();
            if recs.is_empty() {
                continue;
            }
            paciente.get_or_insert_with(|| p.paciente.clone());
            out.push(prescripcion_json(p, recs, hoy));
        }
        match paciente {
            None => (200, mensaje("ERR017", "No existen prescripciones activas para el paciente indicado", &q.id_transaccion, &q.sw_nodo)),
            Some(pa) => (
                200,
                json!({
                    "idTransaccion": q.id_transaccion, "codResultado": "CONOK", "descResultado": "Operación realizada correctamente",
                    "datosPaciente": paciente_json(&pa), "prescripciones": out, "versionSoftware": sw(&q.sw_nodo)
                }),
            ),
        }
    }

    /// Servicio 3.3: recetas del paciente dispensadas en esta farmacia (último año).
    pub fn dispensadas(&self, q: &Query, own_repo: &str, hoy: NaiveDate) -> (u16, Value) {
        if let Err((s, c, t)) = check_query(q, own_repo) {
            return (s, mensaje(c, t, &q.id_transaccion, &q.sw_nodo));
        }
        let mut out = vec![];
        for p in self.prescripciones.iter().filter(|p| p.id_acceso == q.id_acceso.trim() && pin_ok(p, &q.pin)) {
            for r in p.recetas.iter().filter(|r| matches!(r.estado, 3 | 4 | 8 | 10)) {
                let Some(last) = self.last_dispensation(&r.id_receta) else { continue };
                if last.id_farmacia != q.id_farmacia.trim() || r.fecha_dispensacion.is_none_or(|d| hoy - d > Duration::days(365)) {
                    continue;
                }
                out.push(receta_json(r, r.estado, Some(&last.id_accion_farmacia)));
            }
        }
        if out.is_empty() {
            return (200, mensaje("ERR085", "No existen recetas en estado Dispensado para el paciente indicado", &q.id_transaccion, &q.sw_nodo));
        }
        (200, json!({"idTransaccion": q.id_transaccion, "codResultado": "CONOK", "descResultado": "Operación realizada correctamente", "recetas": out, "versionSoftware": sw(&q.sw_nodo)}))
    }

    fn last_dispensation(&self, id_receta: &str) -> Option<&RAccion> {
        self.acciones.iter().rev().find(|a| a.id_receta == id_receta && matches!(a.accion, 1 | 2) && a.resultado == "RACOK" && !a.deshecha)
    }

    fn next_dispensation(&self, pi: usize, hoy: NaiveDate) -> String {
        self.prescripciones[pi].recetas.iter().filter(|r| matches!(r.estado_en(hoy), 0 | 1)).map(|r| r.fecha_ini.max(hoy)).min().map(dmy).unwrap_or_else(|| "31/12/9999".into())
    }

    /// Servicio 3.2: registrar una actividad de la farmacia sobre una receta.
    /// Devuelve el estado HTTP, la respuesta y si el repositorio cambió.
    pub fn registrar(&mut self, body: &str, ahora: NaiveDateTime) -> (u16, Value, bool) {
        let hoy = ahora.date();
        let v: Value = match serde_json::from_str::<Value>(body) {
            Ok(v) if v.is_object() => v.get("accionFarmacia").filter(|x| x.is_object()).cloned().unwrap_or(v),
            _ => return (400, mensaje("ERR004", "JSON no válido", "", ""), false),
        };
        let s = |k: &str| v.get(k).map(|x| x.as_str().map(|s| s.trim().to_string()).unwrap_or_else(|| if x.is_null() { String::new() } else { x.to_string() })).unwrap_or_default();
        let n = |k: &str| v.get(k).and_then(|x| x.as_i64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok())));
        let tx = s("idTransaccion");
        let sw_nodo = v.pointer("/versionSoftware/swNodo").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let bad = |c: &str, t: &str| (400, mensaje(c, t, &tx, &sw_nodo), false);
        if tx.is_empty() {
            return bad("ERR016", "idTransacción nulo o vacío");
        }
        if !tx_valid(&tx) {
            return bad("ERR029", "IdTransaccion no tiene el formato correcto");
        }
        if sw_nodo.is_empty() {
            return bad("ERR015", "swNodo nulo o vacío");
        }
        let Some(accion) = n("accion") else { return bad("ERR025", "Acción nulo o vacío") };
        if !(0..=5).contains(&accion) {
            return bad("ERR026", "Acción tiene que ser 0, 1, 2, 3, 4, 5");
        }
        let accion = accion as u8;
        let id_receta = s("idReceta");
        if id_receta.is_empty() {
            return bad("ERR021", "idReceta nulo o vacío");
        }
        let id_accion = s("idAccionFarmacia");
        if id_accion.is_empty() {
            return bad("ERR022", "idAccionFarmacia nulo o vacío");
        }
        let fh = s("fechaHoraAccion");
        if fh.is_empty() {
            return bad("ERR032", "FechaHoraAccion nulo o vacío");
        }
        let Ok(when) = NaiveDateTime::parse_from_str(&fh, "%d/%m/%Y %H:%M:%S") else {
            return bad("ERR033", "FechaHoraAccion no tiene el formato DD/MM/AAAA HH:MM:SS o no existe");
        };
        if when > ahora + Duration::minutes(10) {
            return bad("ERR034", "FechaHoraAccion es superior a la fecha del sistema");
        }
        let farmacia = s("idFarmacia");
        let ok = |store: &RepoStore, pi: usize| {
            let mut m = mensaje("RACOK", "Operación realizada correctamente", &tx, &sw_nodo);
            m["fechaProximaDispensacion"] = store.next_dispensation(pi, hoy).into();
            m
        };
        // una repetición de la misma acción (reintento de Nodofarma) no se aplica dos veces
        if accion != 3 && self.acciones.iter().any(|a| a.id_accion_farmacia == id_accion && a.accion == accion && a.id_receta == id_receta && a.resultado == "RACOK") {
            let pi = self.find_receta(&id_receta).map(|x| x.0).unwrap_or(0);
            return (200, ok(self, pi), false);
        }
        let mut acc = RAccion {
            id_accion_farmacia: id_accion.clone(),
            id_transaccion: tx.clone(),
            id_receta: id_receta.clone(),
            accion,
            id_farmacia: farmacia.clone(),
            fecha_hora: fh.clone(),
            recibida: ahora,
            envases: n("envasesDispensados").unwrap_or(0).max(0) as u32,
            cod_producto: s("codProductoDispensacion"),
            composicion: s("composicion"),
            causa: None,
            descripcion: s("descSustitucion"),
            observaciones: s("observaciones"),
            resultado: String::new(),
            deshecha: false,
            estado_previo: 0,
            cn_previo: String::new(),
        };
        let fail = |store: &mut RepoStore, mut acc: RAccion, c: &str, t: &str| {
            acc.resultado = c.to_string();
            store.acciones.push(acc);
            (200, mensaje(c, t, &tx, &sw_nodo), true)
        };
        if farmacia.is_empty() {
            return fail(self, acc, "ERR038", "No se ha especificado el identificador de farmacia para la acción");
        }
        let Some((pi, ri)) = self.find_receta(&id_receta) else {
            return fail(self, acc, "ERR036", "IdReceta no existe en la BBDD");
        };
        let prod = self.prescripciones[pi].producto.clone();
        let anulada_presc = self.prescripciones[pi].anulada;
        let r = self.prescripciones[pi].recetas[ri].clone();
        let e = if anulada_presc { 11 } else { r.estado_en(hoy) };
        let es_fm = matches!(prod.tipo, 3 | 4);
        match accion {
            1 | 2 => {
                if e == 11 {
                    return fail(self, acc, "ERR037", "Receta no dispensable");
                }
                if e == 5 {
                    return fail(self, acc, "ERR040", "La receta ha caducado y no puede ser dispensada");
                }
                if matches!(e, 3 | 4) {
                    return fail(self, acc, "ERR042", "La receta ya ha sido dispensada");
                }
                if e == 9 && r.farmacia != farmacia {
                    return fail(self, acc, "ERR094", "La fórmula magistral está siendo elaborada por otra farmacia.");
                }
                if !matches!(e, 1 | 8 | 9 | 10) {
                    return fail(self, acc, "ERR037", "Receta no dispensable");
                }
                let Some(env) = n("envasesDispensados") else { return fail(self, acc, "ERR027", "envasesDispensados nulo o vacío") };
                if env <= 0 {
                    return fail(self, acc, "ERR045", "La cantidad de envases de una dispensación no puede ser 0");
                }
                if env as u32 > r.quedan() {
                    return fail(self, acc, "ERR043", "La cantidad de envases indicada excede a la especificada en la receta");
                }
                let cod = acc.cod_producto.clone();
                if !cod.is_empty() && !(cod.len() <= 7 && cod.chars().all(|c| c.is_ascii_digit())) {
                    return fail(self, acc, "ERR053", "CodProductoDispensacion no tiene el formato correcto");
                }
                if !cod.is_empty() && !acc.composicion.is_empty() {
                    return fail(self, acc, "ERR059", "Uno de los dos campos debe ir relleno y el otro vacío, código de producto o composición.");
                }
                let same_cn = crate::nomenclator::cn6(&cod) == crate::nomenclator::cn6(&prod.cod_producto);
                if accion == 1 && !cod.is_empty() && !prod.cod_producto.is_empty() && !same_cn {
                    return fail(self, acc, "ERR055", "El código de producto no es el prescrito");
                }
                if accion == 2 {
                    if es_fm {
                        return fail(self, acc, "ERR137", "No es posible realizar sustituciones de prescripciones de Vacunas o Fórmulas Magistrales");
                    }
                    if cod.is_empty() {
                        return fail(self, acc, "ERR052", "CodProductoDispensacion nulo o vacío");
                    }
                    if same_cn {
                        return fail(self, acc, "ERR062", "CodProductoDispensacion debe ser DISTINTO al de la prescripción en una dispensación CON sustitución");
                    }
                    let Some(causa) = n("causaSustitucion") else { return fail(self, acc, "ERR064", "CausaSustitucion nulo o vacío") };
                    if !(2..=4).contains(&causa) {
                        return fail(self, acc, "ERR065", "CausaSustitucion tiene que ser 2, 3 o 4");
                    }
                    if causa == 4 && acc.descripcion.is_empty() {
                        return fail(self, acc, "ERR066", "DescSustitucion nulo o vacío");
                    }
                    if acc.descripcion.chars().count() > 255 {
                        return fail(self, acc, "ERR067", "DescSustitucion es superior a lo permitido");
                    }
                    acc.causa = Some(causa as u8);
                }
                acc.envases = env as u32;
                acc.estado_previo = r.estado;
                acc.cn_previo = r.cn_dispensado.clone();
                let rr = &mut self.prescripciones[pi].recetas[ri];
                rr.dispensados += env as u32;
                rr.fecha_dispensacion = Some(hoy);
                rr.farmacia = farmacia.clone();
                if !cod.is_empty() {
                    rr.cn_dispensado = cod;
                } else if rr.cn_dispensado.is_empty() {
                    rr.cn_dispensado = prod.cod_producto.clone();
                }
                if !acc.composicion.is_empty() {
                    rr.composicion_dispensada = acc.composicion.clone();
                }
                let complete = rr.dispensados >= rr.num_envases;
                let substituted = accion == 2 || matches!(rr.estado, 4 | 10);
                rr.estado = match (substituted, complete) {
                    (false, true) => 3,
                    (false, false) => 8,
                    (true, true) => 4,
                    (true, false) => 10,
                };
            }
            0 => {
                if !matches!(e, 1 | 8 | 10) {
                    return fail(self, acc, "ERR037", "Receta no dispensable");
                }
                let Some(causa) = n("causaBloqueo") else { return fail(self, acc, "ERR082", "CausaBloqueo nulo o vacío") };
                if !(0..=4).contains(&causa) {
                    return fail(self, acc, "ERR083", "CausaBloqueo tiene que ser 0, 1, 2, 3 o 4");
                }
                if acc.observaciones.chars().count() > 255 {
                    return fail(self, acc, "ERR084", "Observaciones es superior a lo permitido");
                }
                acc.causa = Some(causa as u8);
                acc.estado_previo = r.estado;
                let obs = acc.observaciones.clone();
                let rr = &mut self.prescripciones[pi].recetas[ri];
                rr.estado = 2;
                rr.obs_bloqueo = if obs.is_empty() { causa_bloqueo_texto(causa as u8).to_string() } else { format!("{}: {obs}", causa_bloqueo_texto(causa as u8)) };
            }
            3 => {
                let Some(prev) = self.acciones.iter().rposition(|a| a.id_accion_farmacia == id_accion && a.id_receta == id_receta && matches!(a.accion, 1 | 2) && a.resultado == "RACOK" && !a.deshecha) else {
                    return fail(self, acc, "ERR129", "IdAccionFarmacia no existente");
                };
                if !matches!(r.estado, 3 | 4 | 8 | 10) {
                    return fail(self, acc, "ERR068", "La receta no está dispensada");
                }
                let p = self.acciones[prev].clone();
                if p.id_farmacia != farmacia {
                    return fail(self, acc, "ERR134", "La receta ha sido dispensada por otra farmacia");
                }
                if hoy - p.recibida.date() > Duration::days(10) {
                    return fail(self, acc, "ERR071", "La dispensación receta supera el tiempo máximo transcurrido para poder ser cancelada");
                }
                if self.last_dispensation(&id_receta).map(|a| a.id_accion_farmacia.as_str()) != Some(id_accion.as_str()) {
                    return fail(self, acc, "ERR075", "Receta no anulable dado que no se trata de la última dispensación");
                }
                if let Some(c) = n("causaAnulacion") {
                    if !(0..=6).contains(&c) {
                        return fail(self, acc, "ERR077", "CausaAnulacion tiene que ser 0, 1, 2, 3, 4, 5 o 6");
                    }
                    acc.causa = Some(c as u8);
                }
                acc.envases = p.envases;
                self.acciones[prev].deshecha = true;
                let rr = &mut self.prescripciones[pi].recetas[ri];
                rr.dispensados = rr.dispensados.saturating_sub(p.envases);
                rr.estado = p.estado_previo;
                rr.cn_dispensado = p.cn_previo.clone();
                if rr.dispensados == 0 {
                    rr.fecha_dispensacion = None;
                    rr.farmacia.clear();
                    rr.composicion_dispensada.clear();
                }
            }
            4 => {
                if !es_fm {
                    return fail(self, acc, "ERR143", "Acción permitida únicamente para productos de tipo Vacuna o Fórmulas Magistrales");
                }
                if e == 9 {
                    return if r.farmacia == farmacia { fail(self, acc, "ERR139", "El producto se encuentra en estado de Preparación") } else { fail(self, acc, "ERR094", "La fórmula magistral está siendo elaborada por otra farmacia.") };
                }
                if e == 5 {
                    return fail(self, acc, "ERR040", "La receta ha caducado y no puede ser dispensada");
                }
                if matches!(e, 3 | 4) {
                    return fail(self, acc, "ERR042", "La receta ya ha sido dispensada");
                }
                if e != 1 {
                    return fail(self, acc, "ERR037", "Receta no dispensable");
                }
                acc.estado_previo = r.estado;
                let rr = &mut self.prescripciones[pi].recetas[ri];
                rr.estado = 9;
                rr.farmacia = farmacia.clone();
            }
            _ => {
                if !es_fm {
                    return fail(self, acc, "ERR079", "El tipo de producto indicado en la prescripción no es adecuado para la acción");
                }
                if e != 9 {
                    return fail(self, acc, "ERR037", "Receta no dispensable");
                }
                if r.farmacia != farmacia {
                    return fail(self, acc, "ERR141", "Otra farmacia inició la preparación");
                }
                let rr = &mut self.prescripciones[pi].recetas[ri];
                rr.estado = if rr.dispensados > 0 { 8 } else { 1 };
                rr.farmacia.clear();
            }
        }
        acc.resultado = "RACOK".into();
        self.acciones.push(acc);
        (200, ok(self, pi), true)
    }

    // ───── cara interna (sistema de prescripción) ─────

    /// Guarda recetas firmadas. Repetir el envío de las mismas no las duplica.
    pub fn emitir(&mut self, list: Vec<RPrescripcion>) -> Result<usize, String> {
        for p in &list {
            if p.id_prescripcion.len() != 32 || p.id_acceso.len() != 32 {
                return Err("Identificadores de prescripción o de acceso no válidos (32 caracteres).".into());
            }
            if p.recetas.is_empty() {
                return Err(format!("La prescripción de {} no tiene recetas.", p.producto.denominacion));
            }
            if p.recetas.iter().any(|r| r.id_receta.len() != 32 || r.num_envases == 0 || r.fecha_fin < r.fecha_ini) {
                return Err("Hay una receta con datos no válidos.".into());
            }
            if let Some(old) = self.prescripciones.iter().find(|x| x.id_prescripcion == p.id_prescripcion) && old.documento != p.documento {
                return Err("Ya existe otra prescripción con ese identificador.".into());
            }
        }
        let mut n = 0;
        for p in list {
            if !self.prescripciones.iter().any(|x| x.id_prescripcion == p.id_prescripcion) {
                n += p.recetas.len();
                self.prescripciones.push(p);
            }
        }
        Ok(n)
    }

    /// El prescriptor anula una receta (o toda la prescripción) que aún no se ha dispensado.
    pub fn anular(&mut self, id_prescripcion: &str, id_receta: Option<&str>, motivo: &str, hoy: NaiveDate) -> Result<usize, String> {
        let p = self.prescripciones.iter_mut().find(|p| p.id_prescripcion == id_prescripcion).ok_or("La prescripción no está en el repositorio.")?;
        let mut n = 0;
        let mut preparing = false;
        for r in p.recetas.iter_mut().filter(|r| id_receta.is_none_or(|x| x == r.id_receta)) {
            match r.estado_en(hoy) {
                0 | 1 | 2 | 5 | 6 | 7 | 8 | 10 => {
                    r.anulada = true;
                    n += 1;
                }
                9 => preparing = true,
                _ => {}
            }
        }
        if n == 0 {
            return Err(if preparing { "La farmacia está preparando la fórmula: no se puede anular desde aquí.".into() } else { "No queda nada por anular: ya está dispensada o anulada.".into() });
        }
        if id_receta.is_none() || p.recetas.iter().all(|r| r.anulada || matches!(r.estado, 3 | 4)) {
            p.anulada = id_receta.is_none();
            p.motivo_anulacion = motivo.to_string();
        }
        Ok(n)
    }

    pub fn seguimiento(&self, ids: &[String], hoy: NaiveDate) -> Seguimiento {
        let prescripciones: Vec<RPrescripcion> = self.prescripciones.iter().filter(|p| ids.contains(&p.id_prescripcion)).cloned().collect();
        let recetas: Vec<&str> = prescripciones.iter().flat_map(|p| p.recetas.iter().map(|r| r.id_receta.as_str())).collect();
        let estados = prescripciones.iter().flat_map(|p| p.recetas.iter().map(|r| (r.id_receta.clone(), if p.anulada { 11 } else { r.estado_en(hoy) }))).collect();
        let acciones = self.acciones.iter().filter(|a| recetas.contains(&a.id_receta.as_str())).cloned().collect();
        Seguimiento { prescripciones, estados, acciones }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Seguimiento {
    pub prescripciones: Vec<RPrescripcion>,
    pub estados: HashMap<String, u8>,
    pub acciones: Vec<RAccion>,
}

// ───────────────────────────── servicio HTTP ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RepoConfig {
    pub port: u16,
    /// Clave de la clínica para la cara interna.
    pub key: String,
    /// Identificador del repositorio que asigna el CGCOM al certificarlo.
    pub id_repositorio: String,
    /// Direcciones que pueden usar los servicios de las farmacias (la del proxy TLS).
    pub allowed_ips: Vec<String>,
    /// Exigir que el proxy confirme el certificado de Nodofarma (cabecera X-SSL-Client-Verify: SUCCESS).
    pub require_cert_header: bool,
    /// Prefijo de la ruta pública, p. ej. «/repositorio/api/v1».
    #[serde(default)]
    pub base_path: String,
    /// Escuchar en la red local (puestos de la clínica y proxy en otro equipo).
    #[serde(default)]
    pub lan: bool,
}

fn config_path() -> PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("repositorio.json")
}

pub fn load_config() -> Option<RepoConfig> {
    std::fs::read_to_string(config_path()).ok().and_then(|t| serde_json::from_str(&t).ok())
}

pub fn save_config(c: &RepoConfig) -> Result<(), String> {
    let p = config_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(c).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn store_path() -> PathBuf {
    crate::model::Db::path().with_file_name("srep").join("repositorio.json")
}

pub fn load_store(path: &std::path::Path) -> RepoStore {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn persist(path: &std::path::Path, store: &RepoStore) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(store).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Registro de actividad encadenado: cada línea lleva la huella de la anterior.
struct ActivityLog {
    path: PathBuf,
    last: String,
}

impl ActivityLog {
    fn open(path: PathBuf) -> Self {
        let last = std::fs::read_to_string(&path).ok().and_then(|t| t.lines().last().and_then(|l| serde_json::from_str::<Value>(l).ok()).and_then(|v| v["huella"].as_str().map(|s| s.to_string()))).unwrap_or_default();
        ActivityLog { path, last }
    }
    fn add(&mut self, entry: Value) {
        let body = json!({"t": crate::model::now().format("%Y-%m-%dT%H:%M:%S").to_string(), "anterior": self.last, "dato": entry});
        let huella: String = Sha256::digest(body.to_string().as_bytes()).iter().map(|x| format!("{x:02x}")).collect();
        let mut line = body;
        line["huella"] = huella.clone().into();
        use std::io::Write;
        if let Some(d) = self.path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = writeln!(f, "{line}");
            self.last = huella;
        }
    }
}

/// Comprueba que ninguna línea del registro se haya tocado: (líneas, primera rota).
pub fn verify_log(path: &std::path::Path) -> (usize, Option<usize>) {
    let Ok(t) = std::fs::read_to_string(path) else { return (0, None) };
    let mut prev = String::new();
    for (i, l) in t.lines().enumerate() {
        let Ok(mut v) = serde_json::from_str::<Value>(l) else { return (i, Some(i + 1)) };
        let h = v["huella"].as_str().unwrap_or("").to_string();
        if let Some(o) = v.as_object_mut() {
            o.remove("huella");
        }
        let calc: String = Sha256::digest(v.to_string().as_bytes()).iter().map(|x| format!("{x:02x}")).collect();
        if calc != h || v["anterior"].as_str().unwrap_or("") != prev {
            return (i, Some(i + 1));
        }
        prev = h;
    }
    (t.lines().count(), None)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_map(url: &str) -> HashMap<String, String> {
    url.split_once('?')
        .map(|(_, q)| q.split('&').filter_map(|kv| kv.split_once('=').map(|(k, v)| (percent_decode(k), percent_decode(v)))).collect())
        .unwrap_or_default()
}

fn respond(req: tiny_http::Request, code: u16, body: &Value) {
    let h = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..]).unwrap();
    let _ = req.respond(tiny_http::Response::from_string(body.to_string()).with_status_code(code).with_header(h));
}

fn header<'a>(req: &'a tiny_http::Request, name: &'static str) -> Option<&'a str> {
    req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

/// Ruta pública → (servicio, idFarmacia, idAcceso).
fn route(path: &str, base: &str) -> Option<(&'static str, String, String)> {
    let base = base.trim().trim_end_matches('/');
    let p = if !base.is_empty() && path.starts_with(base) { &path[base.len()..] } else { path };
    let parts: Vec<&str> = p.trim_matches('/').split('/').collect();
    match parts.as_slice() {
        ["receta"] => Some(("registrar", String::new(), String::new())),
        ["prescriptions", "idFarmacia", f, "idAcceso", a] => Some(("consultar", percent_decode(f), percent_decode(a))),
        ["receta", "idFarmacia", f, "idAcceso", a] => Some(("dispensadas", percent_decode(f), percent_decode(a))),
        _ => None,
    }
}

/// Atiende peticiones hasta recibir /interno/parar.
pub fn serve(server: tiny_http::Server, cfg: RepoConfig, path: PathBuf) {
    let mut store = load_store(&path);
    let mut log = ActivityLog::open(path.with_file_name("repositorio_actividad.jsonl"));
    for mut req in server.incoming_requests() {
        let url = req.url().to_string();
        let route_path = url.split('?').next().unwrap_or("").to_string();
        let method = req.method().clone();
        let ip = req.remote_addr().map(|a| a.ip().to_string()).unwrap_or_default();
        let mut body = String::new();
        let _ = req.as_reader().read_to_string(&mut body);
        let ahora = crate::model::now();
        let hoy = ahora.date();
        if let Some(op) = route_path.strip_prefix("/interno/") {
            if cfg.key.is_empty() || header(&req, "X-Clave") != Some(cfg.key.as_str()) {
                respond(req, 401, &json!({"error": "Clave de la clínica incorrecta"}));
                continue;
            }
            let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            match (method, op) {
                (tiny_http::Method::Get, "estado") => {
                    let recetas: usize = store.prescripciones.iter().map(|p| p.recetas.len()).sum();
                    respond(req, 200, &json!({"version": SW_REPOSITORIO, "id_repositorio": cfg.id_repositorio, "prescripciones": store.prescripciones.len(), "recetas": recetas, "acciones": store.acciones.len()}));
                }
                (tiny_http::Method::Post, "emitir") => match serde_json::from_value::<Vec<RPrescripcion>>(v).map_err(|e| format!("datos no válidos: {e}")).and_then(|l| store.emitir(l)) {
                    Ok(n) => {
                        let _ = persist(&path, &store);
                        log.add(json!({"op": "emitir", "recetas": n}));
                        respond(req, 200, &json!({"ok": true, "recetas": n}));
                    }
                    Err(e) => respond(req, 400, &json!({"error": e})),
                },
                (tiny_http::Method::Post, "anular") => {
                    let idp = v["id_prescripcion"].as_str().unwrap_or("").to_string();
                    let idr = v["id_receta"].as_str().map(|s| s.to_string());
                    match store.anular(&idp, idr.as_deref(), v["motivo"].as_str().unwrap_or(""), hoy) {
                        Ok(n) => {
                            let _ = persist(&path, &store);
                            log.add(json!({"op": "anular", "prescripcion": idp, "receta": idr, "recetas": n}));
                            respond(req, 200, &json!({"ok": true, "recetas": n}));
                        }
                        Err(e) => respond(req, 400, &json!({"error": e})),
                    }
                }
                (tiny_http::Method::Post, "seguimiento") => {
                    let ids: Vec<String> = v["ids"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
                    respond(req, 200, &serde_json::to_value(store.seguimiento(&ids, hoy)).unwrap_or_default());
                }
                (tiny_http::Method::Post, "parar") => {
                    respond(req, 200, &json!({"ok": true}));
                    break;
                }
                _ => respond(req, 404, &json!({"error": "no existe"})),
            }
            continue;
        }
        // servicios para las farmacias (Nodofarma)
        let allowed = cfg.allowed_ips.iter().any(|a| a.trim() == "*" || a.trim() == ip) || (cfg.allowed_ips.is_empty() && (ip == "127.0.0.1" || ip == "::1"));
        if !allowed || (cfg.require_cert_header && header(&req, "X-SSL-Client-Verify") != Some("SUCCESS")) {
            log.add(json!({"op": "acceso rechazado", "ip": ip, "ruta": route_path}));
            respond(req, 403, &mensaje("ERR001", "El certificado es incorrecto", "", ""));
            continue;
        }
        let Some((svc, f, a)) = route(&route_path, &cfg.base_path).filter(|_| method == tiny_http::Method::Post) else {
            respond(req, 404, &mensaje("ERR096", "Url incorrecta. La dirección url es incorrecta.", "", ""));
            continue;
        };
        let qm = query_map(&url);
        let g = |k: &str| qm.get(k).cloned().unwrap_or_default();
        let q = Query { id_farmacia: f, id_acceso: a, id_transaccion: g("idTransaccion"), id_repositorio: g("idRepositorio"), pin: g("pin"), sw_nodo: g("swNodo") };
        let (code, resp) = match svc {
            "consultar" => store.consultar(&q, &cfg.id_repositorio, hoy),
            "dispensadas" => store.dispensadas(&q, &cfg.id_repositorio, hoy),
            _ => {
                let (c, r, changed) = store.registrar(&body, ahora);
                if changed {
                    let _ = persist(&path, &store);
                }
                (c, r)
            }
        };
        log.add(json!({"op": svc, "farmacia": q.id_farmacia, "transaccion": resp["idTransaccion"].clone(), "resultado": resp["codResultado"].clone()}));
        respond(req, code, &resp);
    }
}

/// Modo `--repositorio-recetas`: el repositorio funcionando en este ordenador.
pub fn run() -> Result<(), String> {
    let cfg = load_config().filter(|c| !c.key.is_empty()).ok_or("Falta la configuración del repositorio: actívelo en Configuración › Receta electrónica.")?;
    let host = if cfg.lan { "0.0.0.0" } else { "127.0.0.1" };
    let server = tiny_http::Server::http((host, cfg.port)).map_err(|e| format!("No se puede abrir el puerto {}: {e}", cfg.port))?;
    let path = store_path();
    println!("VeodarDental · repositorio de recetas (SREP)");
    println!("  Recetas: {}", path.display());
    println!("  Escuchando en {host}:{}", cfg.port);
    serve(server, cfg, path);
    Ok(())
}

pub fn spawn_local() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe).arg("--repositorio-recetas").spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ───────────────────────────── cliente interno ─────────────────────────────

#[derive(Clone)]
pub struct RepoClient {
    base: String,
    key: String,
    agent: ureq::Agent,
}

fn err_text(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401, _) => "La clave del repositorio de recetas no es correcta.".into(),
        ureq::Error::Status(code, r) => {
            let t = r.into_string().unwrap_or_default();
            serde_json::from_str::<Value>(&t).ok().and_then(|v| v["error"].as_str().map(|s| s.to_string())).unwrap_or_else(|| format!("error {code} del repositorio"))
        }
        ureq::Error::Transport(_) => "No hay conexión con el repositorio de recetas.".into(),
    }
}

impl RepoClient {
    pub fn new(addr: &str, key: &str) -> Self {
        let a = addr.trim().trim_end_matches('/');
        let base = if a.starts_with("http://") || a.starts_with("https://") { a.to_string() } else { format!("http://{a}") };
        let agent = ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(2)).timeout(std::time::Duration::from_secs(15)).build();
        RepoClient { base, key: key.trim().to_string(), agent }
    }

    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let req = self.agent.request(method, &format!("{}{}", self.base, path)).set("X-Clave", &self.key);
        let resp = match body {
            Some(b) => req.set("Content-Type", "application/json").send_string(&b.to_string()),
            None => req.call(),
        }
        .map_err(err_text)?;
        let t = resp.into_string().map_err(|e| e.to_string())?;
        serde_json::from_str(&t).map_err(|e| e.to_string())
    }

    pub fn estado(&self) -> Result<Value, String> {
        self.call("GET", "/interno/estado", None)
    }
    pub fn emitir(&self, list: &[RPrescripcion]) -> Result<usize, String> {
        let v = self.call("POST", "/interno/emitir", Some(&serde_json::to_value(list).map_err(|e| e.to_string())?))?;
        Ok(v["recetas"].as_u64().unwrap_or(0) as usize)
    }
    pub fn anular(&self, id_prescripcion: &str, id_receta: Option<&str>, motivo: &str) -> Result<usize, String> {
        let v = self.call("POST", "/interno/anular", Some(&json!({"id_prescripcion": id_prescripcion, "id_receta": id_receta, "motivo": motivo})))?;
        Ok(v["recetas"].as_u64().unwrap_or(0) as usize)
    }
    pub fn seguimiento(&self, ids: &[String]) -> Result<Seguimiento, String> {
        let v = self.call("POST", "/interno/seguimiento", Some(&json!({ "ids": ids })))?;
        serde_json::from_value(v).map_err(|e| e.to_string())
    }
    #[cfg(test)]
    pub fn parar(&self) -> Result<(), String> {
        self.call("POST", "/interno/parar", Some(&json!({}))).map(|_| ())
    }
}

/// Llamada como lo haría Nodofarma (para el simulador de farmacia y las pruebas):
/// devuelve el estado HTTP y el JSON aunque sea un error.
#[cfg(test)]
pub fn farmacia_post(base_url: &str, path: &str, query: &[(&str, &str)], body: &Value, cert_ok: bool) -> Result<(u16, Value), String> {
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(15)).build();
    let mut req = agent.post(&format!("{}{}", base_url.trim_end_matches('/'), path)).set("Content-Type", "application/json");
    for (k, v) in query {
        req = req.query(k, v);
    }
    if cert_ok {
        req = req.set("X-SSL-Client-Verify", "SUCCESS");
    }
    let (code, resp) = match req.send_string(&body.to_string()) {
        Ok(r) => (r.status(), r),
        Err(ureq::Error::Status(c, r)) => (c, r),
        Err(e) => return Err(err_text(e)),
    };
    let t = resp.into_string().map_err(|e| e.to_string())?;
    Ok((code, serde_json::from_str(&t).unwrap_or(Value::String(t))))
}

/// Configuración para publicar el repositorio a Nodofarma con nginx
/// (TLS con el certificado del repositorio y verificación del de Nodofarma).
pub fn nginx_snippet(public_host: &str, base_path: &str, port: u16) -> String {
    let host = if public_host.trim().is_empty() { "recetas.su-clinica.es" } else { public_host.trim() };
    let base = if base_path.trim().is_empty() { "/repositorio/api/v1".to_string() } else { base_path.trim().trim_end_matches('/').to_string() };
    format!(
        "server {{\n    listen 443 ssl;\n    server_name {host};\n    ssl_protocols TLSv1.2 TLSv1.3;\n    ssl_certificate     /etc/ssl/repositorio/certificado_repositorio.pem;\n    ssl_certificate_key /etc/ssl/repositorio/clave_repositorio.pem;\n    # certificado (clave pública) de Nodofarma que entrega el CGCOF al certificar el repositorio\n    ssl_client_certificate /etc/ssl/repositorio/nodofarma_ca.pem;\n    ssl_verify_client on;\n\n    location {base}/ {{\n        proxy_pass http://127.0.0.1:{port}{base}/;\n        proxy_set_header X-SSL-Client-Verify $ssl_client_verify;\n        proxy_set_header X-SSL-Client-S-DN $ssl_client_s_dn;\n    }}\n    location /interno/ {{ return 404; }}\n}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, dd: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, dd).unwrap()
    }
    fn at(day: NaiveDate) -> NaiveDateTime {
        day.and_hms_opt(11, 0, 0).unwrap()
    }

    fn presc(acc: &str, cn: &str, tipo: u8, envases: u32, ini: NaiveDate, pin: Option<&str>) -> RPrescripcion {
        let id = new_id32();
        RPrescripcion {
            id_prescripcion: id.clone(),
            id_acceso: acc.into(),
            receta_local: 1,
            linea: 0,
            doctor_id: 1,
            paciente: RPaciente { nombre: "Ana".into(), apellidos: "Prueba Ejemplo".into(), fecha_nacimiento: "01/02/1980".into(), tipo_id: 1, dni_nie: "00000000T".into(), ..Default::default() },
            fecha_prescripcion: ini,
            id_entidad: "1234".into(),
            id_centro: "1".into(),
            prescriptor: RPrescriptor { id: "280000".into(), nombre: "Laura".into(), apellidos: "Dentista".into(), especialidad: "Odontología".into(), email: "c@ejemplo.invalid".into(), telefono: "900000000".into() },
            producto: RProducto { cod_producto: cn.into(), tipo, denominacion: "IBUPROFENO 600 MG 40 COMPRIMIDOS".into(), composicion: if tipo == 4 { "Clorhexidina 0,2% csp 200 ml".into() } else { String::new() }, dosificacion: "600 mg".into(), forma: "comprimido".into(), ..Default::default() },
            toma: 1.0,
            ud_toma: "comprimido".into(),
            frecuencia: 3.0,
            ud_frecuencia: "día".into(),
            duracion: 5,
            ud_duracion: "días".into(),
            pin_hash: pin.map(|p| pin_hash(&id, p)).unwrap_or_default(),
            recetas: vec![RReceta { id_receta: new_id32(), fecha_ini: ini, fecha_fin: ini + Duration::days(10), num_envases: envases, estado: 1, ..Default::default() }],
            ..Default::default()
        }
    }

    fn q(acc: &str, f: &str, pin: &str) -> Query {
        Query { id_farmacia: f.into(), id_acceso: acc.into(), id_transaccion: new_id32(), pin: pin.into(), sw_nodo: "Nodofarma v2".into(), ..Default::default() }
    }

    fn act(id_receta: &str, accion: u8, f: &str, envases: Option<i64>, extra: Value, id_accion: &str) -> String {
        let mut v = json!({"idReceta": id_receta, "idTransaccion": new_id32(), "idAccionFarmacia": id_accion, "accion": accion, "idFarmacia": f,
            "fechaHoraAccion": "06/11/2026 11:00:00", "versionSoftware": {"swNodo": "Nodofarma v2"}});
        if let Some(e) = envases {
            v["envasesDispensados"] = e.into();
        }
        if let Some(o) = extra.as_object() {
            for (k, x) in o {
                v[k] = x.clone();
            }
        }
        v.to_string()
    }

    fn code(v: &Value) -> &str {
        v["codResultado"].as_str().unwrap_or("")
    }

    #[test]
    fn pharmacy_lifecycle_follows_the_standard() {
        let hoy = d(2026, 11, 6);
        let acc = new_id32();
        let mut s = RepoStore::default();
        let a = presc(&acc, "6543210", 0, 2, hoy, None);
        let secreta = presc(&acc, "1112223", 0, 1, hoy, Some("4321"));
        let futura = presc(&acc, "7778889", 0, 1, hoy + Duration::days(20), None);
        let fm = presc(&acc, "", 4, 1, hoy, None);
        let (ra, rf) = (a.recetas[0].id_receta.clone(), fm.recetas[0].id_receta.clone());
        assert_eq!(s.emitir(vec![a.clone(), secreta.clone(), futura, fm]).unwrap(), 4);
        // repetir el envío no duplica
        assert_eq!(s.emitir(vec![a.clone()]).unwrap(), 0);

        // consulta: la confidencial solo con su PIN
        let (st, v) = s.consultar(&q(&acc, "F1", ""), "", hoy);
        assert_eq!((st, code(&v)), (200, "CONOK"));
        assert_eq!(v["prescripciones"].as_array().unwrap().len(), 3);
        assert_eq!(v["datosPaciente"]["dniNie"], "00000000T");
        let estados: Vec<i64> = v["prescripciones"].as_array().unwrap().iter().map(|p| p["recetas"][0]["estado"].as_i64().unwrap()).collect();
        assert!(estados.contains(&0), "la futura sale como dispensable a futuro");
        let (_, v) = s.consultar(&q(&acc, "F1", "4321"), "", hoy);
        assert_eq!(v["prescripciones"].as_array().unwrap().len(), 4);
        let (_, v) = s.consultar(&q(&acc, "F1", "0000"), "", hoy);
        assert_eq!(v["prescripciones"].as_array().unwrap().len(), 3, "un PIN equivocado no enseña la confidencial");
        // errores de parámetros
        let mut bad = q(&acc, "F1", "");
        bad.id_transaccion.clear();
        assert_eq!(code(&s.consultar(&bad, "", hoy).1), "ERR016");
        assert_eq!(code(&s.consultar(&q(&acc, "F1", "12a"), "", hoy).1), "ERR018");
        assert_eq!(code(&s.consultar(&q(&new_id32(), "F1", ""), "", hoy).1), "ERR017");

        // dispensación parcial, producto equivocado y resto
        let (_, v, _) = s.registrar(&act(&ra, 1, "F1", Some(1), json!({}), "D1"), at(hoy));
        assert_eq!(code(&v), "RACOK");
        assert_eq!(s.prescripciones[0].recetas[0].estado, 8);
        let (_, v, _) = s.registrar(&act(&ra, 1, "F1", Some(1), json!({}), "D1"), at(hoy));
        assert_eq!((code(&v), s.prescripciones[0].recetas[0].dispensados), ("RACOK", 1), "un reintento no cuenta dos veces");
        assert_eq!(code(&s.registrar(&act(&ra, 1, "F1", Some(1), json!({"codProductoDispensacion": "9999999"}), "D2"), at(hoy)).1), "ERR055");
        assert_eq!(code(&s.registrar(&act(&ra, 1, "F1", Some(5), json!({}), "D2"), at(hoy)).1), "ERR043");
        assert_eq!(code(&s.registrar(&act(&ra, 1, "F1", Some(0), json!({}), "D2"), at(hoy)).1), "ERR045");
        assert_eq!(code(&s.registrar(&act(&ra, 1, "F1", Some(1), json!({}), "D2"), at(hoy)).1), "RACOK");
        assert_eq!(s.prescripciones[0].recetas[0].estado, 3);
        assert_eq!(code(&s.registrar(&act(&ra, 1, "F1", Some(1), json!({}), "D3"), at(hoy)).1), "ERR042");
        // ya no se ofrece, pero sale en las dispensadas de esa farmacia
        let (_, v) = s.consultar(&q(&acc, "F1", ""), "", hoy);
        assert!(!v["prescripciones"].as_array().unwrap().iter().any(|p| p["recetas"][0]["idReceta"] == ra.as_str()));
        let (_, v) = s.dispensadas(&q(&acc, "F1", ""), "", hoy);
        assert_eq!(v["recetas"][0]["idAccionFarmacia"], "D2");
        assert_eq!(code(&s.dispensadas(&q(&acc, "F2", ""), "", hoy).1), "ERR085");

        // anulación de la dispensación: otra farmacia, no la última, y la buena
        assert_eq!(code(&s.registrar(&act(&ra, 3, "F2", None, json!({}), "D2"), at(hoy)).1), "ERR134");
        assert_eq!(code(&s.registrar(&act(&ra, 3, "F1", None, json!({}), "D1"), at(hoy)).1), "ERR075");
        assert_eq!(code(&s.registrar(&act(&ra, 3, "F1", None, json!({"causaAnulacion": 2}), "D2"), at(hoy)).1), "RACOK");
        assert_eq!((s.prescripciones[0].recetas[0].estado, s.prescripciones[0].recetas[0].dispensados), (8, 1));
        assert_eq!(code(&s.registrar(&act(&ra, 3, "F1", None, json!({}), "NOEXISTE"), at(hoy)).1), "ERR129");

        // sustitución
        assert_eq!(code(&s.registrar(&act(&ra, 2, "F1", Some(1), json!({"codProductoDispensacion": "6543210", "causaSustitucion": 3}), "S1"), at(hoy)).1), "ERR062");
        assert_eq!(code(&s.registrar(&act(&ra, 2, "F1", Some(1), json!({"codProductoDispensacion": "1234567", "causaSustitucion": 1}), "S1"), at(hoy)).1), "ERR065");
        assert_eq!(code(&s.registrar(&act(&ra, 2, "F1", Some(1), json!({"codProductoDispensacion": "1234567", "causaSustitucion": 4}), "S1"), at(hoy)).1), "ERR066");
        assert_eq!(code(&s.registrar(&act(&ra, 2, "F1", Some(1), json!({"codProductoDispensacion": "1234567", "causaSustitucion": 3}), "S1"), at(hoy)).1), "RACOK");
        assert_eq!((s.prescripciones[0].recetas[0].estado, s.prescripciones[0].recetas[0].cn_dispensado.as_str()), (4, "1234567"));

        // fórmula magistral reservada por la farmacia que la elabora
        assert_eq!(code(&s.registrar(&act(&rf, 4, "F1", None, json!({}), "E1"), at(hoy)).1), "RACOK");
        let (_, v) = s.consultar(&q(&acc, "F2", ""), "", hoy);
        assert!(!v["prescripciones"].as_array().unwrap().iter().any(|p| p["recetas"][0]["idReceta"] == rf.as_str()), "otra farmacia no la ve");
        assert_eq!(code(&s.registrar(&act(&rf, 1, "F2", Some(1), json!({"composicion": "x"}), "E2"), at(hoy)).1), "ERR094");
        assert_eq!(code(&s.registrar(&act(&rf, 5, "F2", None, json!({}), "E3"), at(hoy)).1), "ERR141");
        assert_eq!(code(&s.registrar(&act(&rf, 2, "F1", Some(1), json!({"codProductoDispensacion": "1234567", "causaSustitucion": 3}), "E4"), at(hoy)).1), "ERR137");
        assert_eq!(code(&s.registrar(&act(&rf, 1, "F1", Some(1), json!({"composicion": "Clorhexidina 0,2%"}), "E5"), at(hoy)).1), "RACOK");
        assert_eq!(code(&s.registrar(&act(&ra, 4, "F1", None, json!({}), "E6"), at(hoy)).1), "ERR143");

        // bloqueo cautelar y caducidad
        let ids = s.prescripciones[1].recetas[0].id_receta.clone();
        assert_eq!(code(&s.registrar(&act(&ids, 0, "F1", None, json!({"causaBloqueo": 9}), "B1"), at(hoy)).1), "ERR083");
        assert_eq!(code(&s.registrar(&act(&ids, 0, "F1", None, json!({"causaBloqueo": 1, "observaciones": "Alergia referida"}), "B1"), at(hoy)).1), "RACOK");
        assert!(s.prescripciones[1].recetas[0].obs_bloqueo.contains("alergia"));
        let fut = s.prescripciones[2].recetas[0].id_receta.clone();
        assert_eq!(code(&s.registrar(&act(&fut, 1, "F1", Some(1), json!({}), "X1"), at(hoy)).1), "ERR037", "todavía no se puede dispensar");
        let tarde = hoy + Duration::days(40);
        assert_eq!(s.prescripciones[2].recetas[0].estado_en(tarde), 5);
        let late = act(&fut, 1, "F1", Some(1), json!({}), "X2").replace("06/11/2026", &tarde.format("%d/%m/%Y").to_string());
        assert_eq!(code(&s.registrar(&late, at(tarde)).1), "ERR040");

        // parámetros de la actividad
        assert_eq!(code(&s.registrar("no es json", at(hoy)).1), "ERR004");
        assert_eq!(code(&s.registrar(&act(&ra, 7, "F1", None, json!({}), "Z"), at(hoy)).1), "ERR026");
        let mut v: Value = serde_json::from_str(&act(&ra, 1, "F1", Some(1), json!({}), "Z")).unwrap();
        v["fechaHoraAccion"] = "2026-11-06".into();
        assert_eq!(code(&s.registrar(&v.to_string(), at(hoy)).1), "ERR033");
        v["fechaHoraAccion"] = "06/11/2027 10:00:00".into();
        assert_eq!(code(&s.registrar(&v.to_string(), at(hoy)).1), "ERR034");
        assert_eq!(code(&s.registrar(&act(&new_id32(), 1, "F1", Some(1), json!({}), "Z"), at(hoy)).1), "ERR036");

        // el prescriptor anula lo que queda
        let fid = s.prescripciones[2].id_prescripcion.clone();
        assert_eq!(s.anular(&fid, None, "Cambio de tratamiento", hoy).unwrap(), 1);
        assert!(s.anular(&fid, None, "otra vez", hoy).is_err());
        let seg = s.seguimiento(&[a.id_prescripcion.clone(), fid], hoy);
        assert_eq!(seg.estados[&ra], 4);
        assert!(seg.acciones.iter().any(|x| x.accion == 3 && x.resultado == "RACOK"));
    }

    #[test]
    fn service_over_http_with_proxy_check() {
        let dir = std::env::temp_dir().join(format!("ds_srep_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("repositorio.json");
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let cfg = RepoConfig { port, key: "CLAVE".into(), id_repositorio: "a".repeat(32), allowed_ips: vec!["127.0.0.1".into()], require_cert_header: true, base_path: "/repositorio/api/v1".into(), lan: false };
        let p2 = path.clone();
        let h = std::thread::spawn(move || serve(server, cfg, p2));
        let addr = format!("127.0.0.1:{port}");
        assert!(RepoClient::new(&addr, "MAL").estado().unwrap_err().contains("clave"));
        let c = RepoClient::new(&addr, "CLAVE");
        let hoy = crate::model::today();
        let acc = new_id32();
        let p = presc(&acc, "6543210", 0, 1, hoy, None);
        assert_eq!(c.emitir(std::slice::from_ref(&p)).unwrap(), 1);
        assert_eq!(c.estado().unwrap()["recetas"], 1);
        let base = format!("http://{addr}/repositorio/api/v1");
        let tx = new_id32();
        let qs = [("idTransaccion", tx.as_str()), ("swNodo", "Nodofarma v2"), ("idRepositorio", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")];
        let ruta = format!("/prescriptions/idFarmacia/F1/idAcceso/{acc}");
        // sin el certificado de Nodofarma confirmado por el proxy no entra
        let (st, v) = farmacia_post(&base, &ruta, &qs, &json!({}), false).unwrap();
        assert_eq!((st, code(&v)), (403, "ERR001"));
        let (st, v) = farmacia_post(&base, &ruta, &qs, &json!({"datamatrix": "", "pista1": ""}), true).unwrap();
        assert_eq!((st, code(&v)), (200, "CONOK"), "{v}");
        assert_eq!(v["idTransaccion"], tx.as_str());
        let id_receta = v["prescripciones"][0]["recetas"][0]["idReceta"].as_str().unwrap().to_string();
        let ahora = crate::model::now().format("%d/%m/%Y %H:%M:%S").to_string();
        let body = json!({"idReceta": id_receta, "idTransaccion": new_id32(), "idAccionFarmacia": "ACC1", "accion": 1, "idFarmacia": "F1", "envasesDispensados": 1, "fechaHoraAccion": ahora, "versionSoftware": {"swNodo": "Nodofarma v2"}});
        let (st, v) = farmacia_post(&base, "/receta", &[], &body, true).unwrap();
        assert_eq!((st, code(&v)), (200, "RACOK"), "{v}");
        assert_eq!(v["fechaProximaDispensacion"], "31/12/9999");
        // parámetro obligatorio vacío → 400
        let (st, v) = farmacia_post(&base, &ruta, &[("swNodo", "Nodofarma v2")], &json!({}), true).unwrap();
        assert_eq!((st, code(&v)), (400, "ERR016"));
        // el programa ve la dispensación
        let seg = c.seguimiento(std::slice::from_ref(&p.id_prescripcion)).unwrap();
        assert_eq!(seg.estados[&id_receta], 3);
        assert_eq!(seg.acciones[0].id_farmacia, "F1");
        // la cara interna no se abre sin clave
        assert_eq!(farmacia_post(&format!("http://{addr}"), "/interno/estado", &[], &json!({}), true).unwrap().0, 401);
        c.parar().unwrap();
        h.join().unwrap();
        // lo guardado y el registro encadenado
        let disk = load_store(&path);
        assert_eq!(disk.acciones.len(), 1);
        let (lines, broken) = verify_log(&path.with_file_name("repositorio_actividad.jsonl"));
        assert!(lines >= 4 && broken.is_none(), "{lines} {broken:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nginx_config_points_to_the_service() {
        let t = nginx_snippet("recetas.clinica.es", "/repositorio/api/v1", 8898);
        assert!(t.contains("ssl_verify_client on;") && t.contains("proxy_pass http://127.0.0.1:8898/repositorio/api/v1/;") && t.contains("X-SSL-Client-Verify"));
    }
}
