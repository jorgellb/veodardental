//! Camino a las farmacias reales (receta electrónica privada).
//!
//! Para que cualquier farmacia dispense las recetas de VeodarDental, los
//! Consejos Generales tienen que certificar el sistema de prescripción y su
//! repositorio: un auditor revisa los controles de los «Requisitos de
//! auditoría de certificación para Sistemas y Repositorios» (funcionales FT,
//! de datos FD, de seguridad SC, de interoperabilidad IN y de protección de
//! datos PD) y se hace una prueba conjunta con Nodofarma. Aquí están:
//!
//! - la prueba del circuito completo con una farmacia simulada, sobre un
//!   repositorio en memoria (nunca el de verdad): los casos de uso que repasa
//!   el auditor en la revisión técnica;
//! - cómo cubre el programa cada control y qué falta;
//! - el expediente para pedir la certificación (una página imprimible).

use crate::model::*;
use crate::srep::*;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use serde_json::{Value, json};

/// Una prueba del circuito.
#[derive(Clone, Debug)]
pub struct Step {
    pub control: &'static str,
    pub what: &'static str,
    pub ok: bool,
    pub detail: String,
}

fn test_presc(acc: &str, cn: &str, envases: u32, ini: NaiveDate, pin: Option<&str>, hoy: NaiveDate) -> RPrescripcion {
    let id = new_id32();
    RPrescripcion {
        id_prescripcion: id.clone(),
        id_acceso: acc.into(),
        doctor_id: 0,
        paciente: RPaciente { nombre: "Paciente".into(), apellidos: "De Prueba".into(), fecha_nacimiento: "01/02/1980".into(), tipo_id: 1, dni_nie: "00000000T".into(), ..Default::default() },
        fecha_prescripcion: hoy,
        id_entidad: "0000".into(),
        id_centro: "1".into(),
        prescriptor: RPrescriptor { id: "000000".into(), nombre: "Dentista".into(), apellidos: "De Prueba".into(), especialidad: "Odontología".into(), email: "prueba@ejemplo.invalid".into(), telefono: "900000000".into() },
        producto: RProducto { cod_producto: cn.into(), tipo: 0, denominacion: format!("MEDICAMENTO DE PRUEBA {cn}"), dosificacion: "600 mg".into(), forma: "comprimido".into(), ..Default::default() },
        toma: 1.0,
        ud_toma: "comprimido".into(),
        frecuencia: 3.0,
        ud_frecuencia: "día".into(),
        duracion: 5,
        ud_duracion: "días".into(),
        pin_hash: pin.map(|p| pin_hash(&id, p)).unwrap_or_default(),
        recetas: vec![RReceta { id_receta: new_id32(), fecha_ini: ini, fecha_fin: ini + Duration::days(10), num_envases: envases, estado: 1, ..Default::default() }],
        pruebas: true,
        ..Default::default()
    }
}

/// Actividad de la farmacia simulada, como la manda Nodofarma.
fn act(id_receta: &str, accion: u8, envases: Option<i64>, extra: Value, id_accion: &str, hoy: NaiveDate) -> String {
    let mut v = json!({"idReceta": id_receta, "idTransaccion": new_id32(), "idAccionFarmacia": id_accion, "accion": accion, "idFarmacia": "F-PRUEBA",
        "fechaHoraAccion": format!("{} 10:00:00", hoy.format("%d/%m/%Y")), "versionSoftware": {"swNodo": "Farmacia simulada"}});
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

fn code(v: &Value) -> String {
    v["codResultado"].as_str().unwrap_or("?").to_string()
}

fn receta<'a>(s: &'a RepoStore, id: &str) -> Option<&'a RReceta> {
    s.prescripciones.iter().flat_map(|p| p.recetas.iter()).find(|r| r.id_receta == id)
}

fn offered(v: &Value, id_receta: &str) -> bool {
    v["prescripciones"].as_array().is_some_and(|a| a.iter().any(|p| p["recetas"].as_array().is_some_and(|r| r.iter().any(|x| x["idReceta"] == id_receta))))
}

/// Prueba el circuito completo con una farmacia simulada sobre un
/// repositorio en memoria: emitir, consultar, confidencialidad, plazos,
/// dispensar (parcial y total), anular la dispensación, bloquear, seguir
/// y anular por el dentista, errores del diseño técnico, plan del
/// tratamiento y hoja del paciente.
pub fn circuit_test(hoy: NaiveDate) -> Vec<Step> {
    let mut out = Vec::new();
    let mut step = |control: &'static str, what: &'static str, ok: bool, detail: String| out.push(Step { control, what, ok, detail });
    let ahora = hoy.and_hms_opt(11, 0, 0).unwrap_or_default();
    let acc = new_id32();
    let mut s = RepoStore::default();
    let normal = test_presc(&acc, "6543210", 2, hoy, None, hoy);
    let secreta = test_presc(&acc, "1112223", 1, hoy, Some("4321"), hoy);
    let futura = test_presc(&acc, "7778889", 1, hoy + Duration::days(20), None, hoy);
    let (rn, rs, rf) = (normal.recetas[0].id_receta.clone(), secreta.recetas[0].id_receta.clone(), futura.recetas[0].id_receta.clone());
    let ids = vec![normal.id_prescripcion.clone(), secreta.id_prescripcion.clone(), futura.id_prescripcion.clone()];

    // emitir (y reenviar sin duplicar)
    let first = s.emitir(vec![normal.clone(), secreta.clone(), futura.clone()]);
    let again = s.emitir(vec![normal.clone()]);
    step("FT01 · FD01", "Emitir tres recetas en el repositorio (y reenviarlas sin duplicarlas)", first == Ok(3) && again == Ok(0), format!("emitidas {:?}, reenvío {:?}", first.unwrap_or(0), again.unwrap_or(0)));

    // la farmacia consulta con el identificador del paciente
    let q = |pin: &str| Query { id_farmacia: "F-PRUEBA".into(), id_acceso: acc.clone(), id_transaccion: new_id32(), pin: pin.into(), sw_nodo: "Farmacia simulada".into(), ..Default::default() };
    let count = |v: &Value| v["prescripciones"].as_array().map(|a| a.len()).unwrap_or(0);
    let (st, v) = s.consultar(&q(""), "", hoy);
    step(
        "FT13 · IN01",
        "La farmacia consulta con el identificador del paciente y recibe sus recetas con todos los datos",
        st == 200 && code(&v) == "CONOK" && count(&v) == 2 && v["datosPaciente"]["dniNie"] == "00000000T",
        format!("{} recetas a la vista, resultado {}", count(&v), code(&v)),
    );

    // confidencialidad con PIN
    let (sin, con, mal) = (count(&s.consultar(&q(""), "", hoy).1), count(&s.consultar(&q("4321"), "", hoy).1), count(&s.consultar(&q("0000"), "", hoy).1));
    step("FT09", "El tratamiento confidencial solo se ve con el PIN que dice el paciente", sin == 2 && con == 3 && mal == 2, format!("sin PIN {sin}, con su PIN {con}, con un PIN equivocado {mal}"));

    // plazos de la dispensación
    let (_, v, _) = s.registrar(&act(&rf, 1, Some(1), json!({}), "X1", hoy), ahora);
    let late = receta(&s, &rf).map(|r| r.estado_en(hoy + Duration::days(40)));
    step("FT04", "Una receta a futuro no se dispensa antes de su fecha y caduca al pasar el plazo", code(&v) == "ERR037" && late == Some(5), format!("antes de tiempo: {}; pasado el plazo: {}", code(&v), late.map(estado_texto).unwrap_or("?")));

    // dispensación parcial, reintento y resto
    let (_, v1, _) = s.registrar(&act(&rn, 1, Some(1), json!({}), "D1", hoy), ahora);
    let e1 = receta(&s, &rn).map(|r| r.estado).unwrap_or(0);
    let (_, retry, _) = s.registrar(&act(&rn, 1, Some(1), json!({}), "D1", hoy), ahora);
    let counted = receta(&s, &rn).map(|r| r.dispensados).unwrap_or(0);
    let (_, v2, _) = s.registrar(&act(&rn, 1, Some(1), json!({}), "D2", hoy), ahora);
    let e2 = receta(&s, &rn).map(|r| r.estado).unwrap_or(0);
    step(
        "FT14",
        "Dispensar 1 de 2 envases, repetir el aviso sin contarlo dos veces y dispensar el resto",
        code(&v1) == "RACOK" && e1 == 8 && code(&retry) == "RACOK" && counted == 1 && code(&v2) == "RACOK" && e2 == 3,
        format!("{} tras la primera, {} envase(s) tras repetir, {} al final", estado_texto(e1), counted, estado_texto(e2)),
    );
    let still = offered(&s.consultar(&q(""), "", hoy).1, &rn);
    let (_, d) = s.dispensadas(&q(""), "", hoy);
    let listed = d["recetas"].as_array().is_some_and(|a| a.iter().any(|x| x["idReceta"] == rn.as_str()));
    step("FT14", "Lo dispensado deja de ofrecerse y queda en las dispensaciones de esa farmacia", !still && listed, format!("se sigue ofreciendo: {}; en sus dispensaciones: {}", if still { "sí" } else { "no" }, if listed { "sí" } else { "no" }));

    // la farmacia anula su última dispensación
    let (_, v, _) = s.registrar(&act(&rn, 3, None, json!({"causaAnulacion": 2}), "D2", hoy), ahora);
    let e = receta(&s, &rn).map(|r| r.estado).unwrap_or(0);
    step("FT14", "La farmacia anula su última dispensación y la receta vuelve a quedar a medias", code(&v) == "RACOK" && e == 8, format!("{} · {}", code(&v), estado_texto(e)));

    // bloqueo cautelar
    let (_, v, _) = s.registrar(&act(&rs, 0, None, json!({"causaBloqueo": 1, "observaciones": "Alergia referida por el paciente"}), "B1", hoy), ahora);
    let e = receta(&s, &rs).map(|r| r.estado).unwrap_or(0);
    step("FT14", "La farmacia bloquea cautelarmente una receta (posible alergia)", code(&v) == "RACOK" && e == 2, format!("{} · {}", code(&v), estado_texto(e)));

    // seguimiento del dentista
    let seg = s.seguimiento(&ids, hoy);
    let saw_disp = seg.acciones.iter().any(|a| a.accion == 1 && a.id_receta == rn);
    let saw_block = seg.acciones.iter().any(|a| a.accion == 0 && a.id_receta == rs);
    step("FT08", "El dentista ve en el seguimiento lo que han hecho las farmacias", saw_disp && saw_block && seg.estados.get(&rs) == Some(&2), format!("{} actuaciones de farmacia a la vista", seg.acciones.len()));

    // anulación por el dentista
    let n = s.anular(&futura.id_prescripcion, None, "Cambio de tratamiento", hoy);
    let gone = !offered(&s.consultar(&q(""), "", hoy).1, &rf);
    let st = s.seguimiento(&ids, hoy).estados.get(&rf).copied();
    step("FT08", "El dentista anula lo que no se ha dispensado y las farmacias dejan de verlo", n == Ok(1) && gone && st == Some(11), format!("anuladas {}, estado {}", n.unwrap_or(0), st.map(estado_texto).unwrap_or("?")));

    // errores del diseño técnico
    let mut bad = q("");
    bad.id_transaccion.clear();
    let (e_tx, e_json, e_unknown) = (code(&s.consultar(&bad, "", hoy).1), code(&s.registrar("no es json", ahora).1), code(&s.registrar(&act(&new_id32(), 1, Some(1), json!({}), "Z", hoy), ahora).1));
    step("IN01", "Ante datos incorrectos el repositorio responde con los códigos de error del diseño técnico", e_tx == "ERR016" && e_json == "ERR004" && e_unknown == "ERR036", format!("sin transacción {e_tx}, mensaje roto {e_json}, receta desconocida {e_unknown}"));

    // plan del tratamiento (art. 8.3 del RD 1718/2010)
    let plan = crate::erx::plan(hoy, 400, 20);
    let monthly = plan.windows(2).all(|w| (w[1].0 - w[0].0).num_days() <= 31);
    let within_year = plan.last().is_some_and(|l| l.0 < hoy + Duration::days(365));
    let windows = plan.iter().all(|(a, b, _)| (*b - *a).num_days() == 10);
    step("FT03", "Un tratamiento largo se reparte en recetas de un mes como mucho y no pasa de un año", !plan.is_empty() && monthly && within_year && windows, format!("{} recetas, la última se recoge desde el {}", plan.len(), plan.last().map(|l| fmt_date(l.0)).unwrap_or_default()));

    // hoja del paciente
    let zeros = "0".repeat(32);
    let h = crate::srep_doc::HipHeader { clinic: "Clínica de prueba", clinic_address: "", clinic_phone: "", id_repositorio: &zeros, id_entidad: "0000", id_sistema: "PRUEBAS", test_mode: true, confidential: false, signer: "Dentista De Prueba" };
    let html = crate::srep_doc::hip_html(&h, &[normal, secreta, futura]);
    let codes = html.matches("<svg").count();
    step("FT11 · PD01", "Hoja para el paciente con un código DataMatrix por receta y la cláusula de protección de datos", codes > 3 && html.contains("Protección de datos"), format!("{codes} códigos dibujados"));
    out
}

// ───────────────────────────── controles de la auditoría ─────────────────────────────

/// Cómo cubre el programa un control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Lo hace el programa (y se puede probar).
    Programa,
    /// Lo hace en parte: falta algo que se dice.
    Parcial,
    /// Depende de dónde y cómo se instale el repositorio.
    Instalacion,
    /// Lo aporta la clínica o el titular del sistema (procedimientos, documentos).
    Documento,
    /// Control opcional que no aplica.
    Opcional,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Programa => "En el programa",
            Kind::Parcial => "En parte",
            Kind::Instalacion => "En la instalación",
            Kind::Documento => "Documentación",
            Kind::Opcional => "No aplica",
        }
    }
}

pub struct Control {
    pub code: &'static str,
    /// Resumen del control (el texto exacto está en el documento de requisitos).
    pub title: &'static str,
    pub kind: Kind,
    /// Cómo lo cubre VeodarDental.
    pub how: &'static str,
}

const fn c(code: &'static str, title: &'static str, kind: Kind, how: &'static str) -> Control {
    Control { code, title, kind, how }
}

/// Los 30 controles de la auditoría de certificación.
pub const CONTROLS: [Control; 30] = [
    c("FT01", "Medicamentos y productos de cada receta", Kind::Programa, "Cada medicamento o producto de la receta del programa se emite como una prescripción propia, con sus recetas (dispensaciones) según el plan."),
    c("FT02", "Relación de medicamentos y productos prescribibles", Kind::Programa, "Se eligen del Nomenclátor oficial de la AEMPS (solo los comercializados, con su código nacional y marcados los estupefacientes y psicótropos) o como fórmula magistral con su composición."),
    c("FT03", "Plan terapéutico", Kind::Programa, "Las recetas siguen el plan del tratamiento: dispensaciones de un mes como mucho y plan de un año como mucho (art. 8.3 del RD 1718/2010)."),
    c("FT04", "Periodicidad de la dispensación", Kind::Programa, "Cada receta tiene su plazo de recogida; el repositorio no deja dispensarla antes y la da por caducada al pasar el plazo."),
    c("FT05", "Control de estupefacientes y psicótropos", Kind::Programa, "Vienen marcados del Nomenclátor y se limitan: en los estupefacientes, como mucho 3 meses de tratamiento y 4 envases por receta."),
    c("FT06", "Habilitación del colegiado", Kind::Programa, "Con el sistema certificado, antes de firmar se consulta el servicio del Consejo (Dentistas, Médicos o Podólogos) para comprobar que el colegiado puede prescribir."),
    c("FT07", "Aseguramiento del paciente", Kind::Parcial, "La receta lleva la identificación del paciente (DNI/NIE, tarjeta sanitaria o representante). Falta enviar los datos de su aseguradora o mutualidad si el diseño funcional los pide."),
    c("FT08", "Seguimiento de las dispensaciones", Kind::Programa, "Desde la receta, el dentista ve qué ha dispensado o bloqueado cada farmacia y puede anular lo que queda."),
    c("FT09", "Protección y confidencialidad en la dispensación", Kind::Programa, "El paciente puede pedir tratamiento confidencial: la farmacia solo lo ve si el paciente le dice su PIN de 4 cifras."),
    c("FT10", "Soportes de la receta (art. 3.1 del RD 1718/2010)", Kind::Documento, "La receta oficial es electrónica y firmada; el programa también imprime la receta en papel cuando no se emite por la vía electrónica. Hay que justificarlo con el texto del control."),
    c("FT11", "Impresión de la hoja de información al paciente", Kind::Programa, "La hoja lleva el código del sistema, un DataMatrix por receta, el plan, la posología y la cláusula de protección de datos."),
    c("FT12", "Envío telemático de la hoja al paciente", Kind::Programa, "La hoja se envía por correo electrónico al paciente desde la receta. Es una página accesible: se amplía y la leen los lectores de pantalla."),
    c("FT13", "Información completa para la farmacia y el paciente", Kind::Programa, "El repositorio entrega a Nodofarma los datos del paciente, del dentista, del producto, la posología y el estado de cada receta. Falta la prueba conjunta con Nodofarma."),
    c("FT14", "Registro de la actividad de dispensación", Kind::Programa, "Se guardan dispensaciones (también parciales y con sustitución), anulaciones, bloqueos y fórmulas magistrales, sin contar dos veces un aviso repetido."),
    c("FD01", "Contenidos mínimos de la receta", Kind::Programa, "Antes de firmar se comprueban los datos obligatorios del paciente, del dentista y de cada medicamento; si falta alguno, no se emite."),
    c("FD02", "Firma electrónica", Kind::Programa, "Cada prescripción se firma (CAdES) con el certificado cualificado personal del dentista, cuya vigencia se comprueba antes de firmar."),
    c("SC01", "Acceso del prescriptor al sistema", Kind::Programa, "Cada usuario entra con su contraseña personal y, como segundo factor, con el código de 6 cifras de una app de autenticación del móvil (TOTP, RFC 6238); la clínica puede exigirlo para firmar recetas. Bloqueo tras 5 intentos fallidos y por inactividad, registro de accesos, y cada receta firmada además con el certificado cualificado del dentista."),
    c("SC02", "Seguridad de la información de la receta", Kind::Programa, "Recetas firmadas, registro encadenado de toda la actividad, registro de cambios y copias diarias, la de fuera del ordenador cifrada."),
    c("SC03", "Acceso del paciente", Kind::Programa, "Con su enlace personal «Mis recetas», el paciente ve en el móvil sus recetas, su estado y la hoja para la farmacia. Como el DNI electrónico no está generalizado, entra con el mecanismo alternativo que admite el control: el enlace secreto más su DNI/NIE y su fecha de nacimiento (de los que solo se publica una huella), con bloqueo de media hora tras 5 intentos fallidos y registro de cada acceso."),
    c("SC04", "Seguridad del equipo de acceso", Kind::Instalacion, "Los ordenadores entran con usuario y contraseña. La clínica debe tenerlos actualizados, con antivirus, bloqueo de pantalla y disco cifrado."),
    c("SC05", "Custodia y conservación", Kind::Programa, "Toda la actividad del repositorio queda en un registro encadenado con huellas SHA-256 que el programa comprueba, y se guarda en las copias."),
    c("SC06", "Disponibilidad 24 horas, todo el año", Kind::Instalacion, "El repositorio es un servicio aparte (veodardental --repositorio-recetas) que debe estar en un servidor siempre encendido o en un alojamiento con vigilancia y copia."),
    c("SC07", "Acceso del sistema de prescripción al repositorio", Kind::Programa, "La parte interna del repositorio exige la clave de la clínica y solo atiende al propio ordenador o a las direcciones permitidas."),
    c("SC08", "Acceso que da el repositorio a Nodofarma", Kind::Instalacion, "Se publica detrás de un proxy TLS con el certificado del repositorio y reconocimiento mutuo del certificado de Nodofarma; el programa da la configuración."),
    c("SC09", "Acceso del sistema de dispensación al repositorio", Kind::Instalacion, "El repositorio solo atiende lo que el proxy ha verificado con el certificado de Nodofarma y desde las direcciones permitidas."),
    c("IN01", "Normas de los servicios del repositorio", Kind::Programa, "Servicios REST/JSON del diseño técnico v2.04.1 (consultar, registrar la actividad y dispensadas) con sus códigos de resultado y de error."),
    c("IN02", "Autenticación delegada", Kind::Opcional, "Control opcional: el programa no delega la autenticación en un sistema externo."),
    c("IN03", "Verificación interoperable de certificados", Kind::Parcial, "Se comprueban la vigencia y los datos del certificado. Falta consultar al prestador si está revocado (OCSP o lista de revocación)."),
    c("PD01", "Cláusula de protección de datos", Kind::Programa, "La hoja del paciente lleva la cláusula: responsable, finalidad, base legal (art. 9.2.h del RGPD), destinatarios y derechos."),
    c("PD02", "Medidas de seguridad por confidencialidad", Kind::Documento, "El programa da acceso por usuario, registro de accesos y de cambios, PIN de confidencialidad y copias cifradas; la clínica aporta su registro de actividades y su análisis de riesgos."),
];

/// Controles que el programa cubre solo en parte, por su título («a, b y c»).
pub fn partial_titles() -> String {
    let t: Vec<String> = CONTROLS.iter().filter(|c| c.kind == Kind::Parcial).map(|c| c.title.to_lowercase()).collect();
    match t.as_slice() {
        [] => String::new(),
        [a] => a.clone(),
        [rest @ .., last] => format!("{} y {}", rest.join(", "), last),
    }
}

/// Cuántos controles hay de cada clase.
pub fn tally() -> Vec<(Kind, usize)> {
    [Kind::Programa, Kind::Parcial, Kind::Instalacion, Kind::Documento, Kind::Opcional].into_iter().map(|k| (k, CONTROLS.iter().filter(|c| c.kind == k).count())).collect()
}

// ───────────────────────────── registro del repositorio en este ordenador ─────────────────────────────

/// Comprueba el registro encadenado del repositorio de este ordenador, si
/// lo hay: (líneas, primera línea rota).
pub fn local_log_check() -> Option<(usize, Option<usize>)> {
    let path = store_path().with_file_name("repositorio_actividad.jsonl");
    path.exists().then(|| verify_log(&path))
}

// ───────────────────────────── expediente ─────────────────────────────

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Documentos de los Consejos Generales en los que se basa.
pub const DOCUMENTS: [&str; 7] = [
    "Diseño funcional e-receta privada v2.03",
    "Requisitos de auditoría de certificación para Sistemas y Repositorios",
    "Requisitos de los Procedimientos de certificación de Sistemas y Repositorios",
    "Diseño técnico e-receta privada – Servicios Repositorios v2.04.1",
    "Diseño técnico e-receta privada – Servicios Nodofarma v2.07.1",
    "Diseño técnico Servicio Consulta de CN Sustituto v1.0",
    "Condiciones de los Sistemas y Repositorios",
];

/// Expediente para pedir la certificación: datos del sistema, prescriptores,
/// controles, pruebas del circuito y lo que falta. Página HTML imprimible.
pub fn dossier_html(db: &Db, steps: &[Step], log: Option<(usize, Option<usize>)>, now: NaiveDateTime) -> String {
    let e = &db.erx;
    let host = e.public_host.trim();
    let url = if host.is_empty() { "pendiente: falta el nombre público del repositorio".to_string() } else { format!("https://{host}/repositorio/api/v1/") };
    let id = |v: &str| if v.trim().is_empty() { "pendiente (lo asigna la certificación)".to_string() } else { v.trim().to_string() };
    let c = &db.config;
    let mut h = String::new();
    h.push_str("<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Expediente de certificación · receta electrónica privada</title><style>body{font-family:'Open Sans',Arial,sans-serif;max-width:980px;margin:32px auto;color:#1e293b;font-size:14px;line-height:1.45}h1{font-size:23px;margin-bottom:4px}h2{font-size:17px;margin-top:28px;border-bottom:2px solid #0d9488;padding-bottom:4px}table{border-collapse:collapse;width:100%;margin-top:8px}td,th{border:1px solid #d6dde6;padding:6px 8px;vertical-align:top;text-align:left}th{background:#f1f5f9}.ok{color:#047857;font-weight:600}.no{color:#b91c1c;font-weight:600}.k{white-space:nowrap}.muted{color:#64748b}@media print{body{margin:0}}</style></head><body>");
    h.push_str(&format!(
        "<h1>Expediente para la certificación del sistema de receta electrónica privada</h1><p class='muted'>VeodarDental {} · {} · preparado el {} a las {}</p>",
        env!("CARGO_PKG_VERSION"),
        esc(SW_REPOSITORIO),
        fmt_date(now.date()),
        now.format("%H:%M")
    ));
    // 1. datos
    h.push_str("<h2>1. Sistema de prescripción y repositorio</h2><table>");
    for (k, v) in [
        ("Titular", format!("{} · NIF {}", c.name, c.nif)),
        ("Dirección y contacto", format!("{} · {} · {}", c.address, c.phone, c.email)),
        ("Sistema de prescripción", format!("VeodarDental {} (programa de gestión de clínicas dentales)", env!("CARGO_PKG_VERSION"))),
        ("Repositorio de recetas", format!("{SW_REPOSITORIO}, servicio propio del programa")),
        ("URLRepositorio", url),
        ("idSistema", id(&e.id_sistema)),
        ("idEntidadSanitaria", id(&e.id_entidad)),
        ("idRepositorio", id(&e.id_repositorio)),
        ("Firma de las recetas", "CAdES con el certificado cualificado personal de cada prescriptor".to_string()),
        ("Acceso de Nodofarma", "HTTPS con reconocimiento mutuo de certificados a través de un proxy TLS".to_string()),
    ] {
        h.push_str(&format!("<tr><th class='k'>{}</th><td>{}</td></tr>", esc(k), esc(&v)));
    }
    h.push_str("</table><p class='muted'>Documentos de referencia de los Consejos Generales: ");
    h.push_str(&DOCUMENTS.iter().map(|d| esc(d)).collect::<Vec<_>>().join(" · "));
    h.push_str(". Normativa: Real Decreto 1718/2010, sobre receta médica y órdenes de dispensación.</p>");
    // 2. prescriptores
    h.push_str("<h2>2. Prescriptores</h2><table><tr><th>Profesional</th><th>Nº de colegiado</th><th>Especialidad</th><th>Certificado de firma en este ordenador</th></tr>");
    let mut any = false;
    for d in db.collaborators.iter().filter(|d| d.active && !d.license.trim().is_empty() && d.license.trim() != "-") {
        any = true;
        let cert = !crate::erx::cert_path(d.id).trim().is_empty();
        h.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td><td class='{}'>{}</td></tr>", esc(&d.name), esc(&d.license), esc(&d.specialty), if cert { "ok" } else { "no" }, if cert { "Elegido" } else { "Falta" }));
    }
    if !any {
        h.push_str("<tr><td colspan='4' class='no'>No hay profesionales con número de colegiado.</td></tr>");
    }
    h.push_str("</table>");
    // 3. controles
    let t = tally();
    h.push_str(&format!(
        "<h2>3. Controles de la auditoría</h2><p>{}.</p><table><tr><th>Control</th><th>Qué se revisa (resumen)</th><th>Cómo lo cubre VeodarDental</th><th>Situación</th></tr>",
        t.iter().filter(|x| x.1 > 0).map(|(k, n)| format!("{}: {n}", k.label())).collect::<Vec<_>>().join(" · ")
    ));
    for ctl in CONTROLS.iter() {
        let cls = match ctl.kind {
            Kind::Programa => "ok",
            Kind::Parcial => "no",
            _ => "",
        };
        h.push_str(&format!("<tr><td class='k'><b>{}</b></td><td>{}</td><td>{}</td><td class='k {cls}'>{}</td></tr>", ctl.code, esc(ctl.title), esc(ctl.how), ctl.kind.label()));
    }
    h.push_str("</table>");
    // 4. pruebas
    let ok = steps.iter().filter(|s| s.ok).count();
    h.push_str(&format!(
        "<h2>4. Prueba del circuito con una farmacia simulada</h2><p>Hecha el {} a las {} sobre un repositorio de prueba en memoria, con los casos de uso de la revisión técnica: <b>{ok} de {} correctas</b>.</p><table><tr><th>Control</th><th>Prueba</th><th>Resultado</th><th></th></tr>",
        fmt_date(now.date()),
        now.format("%H:%M"),
        steps.len()
    ));
    for s in steps {
        h.push_str(&format!("<tr><td class='k'>{}</td><td>{}</td><td>{}</td><td class='{}'>{}</td></tr>", esc(s.control), esc(s.what), esc(&s.detail), if s.ok { "ok" } else { "no" }, if s.ok { "Correcta" } else { "Falla" }));
    }
    h.push_str("</table>");
    // 5. custodia en este ordenador
    h.push_str("<h2>5. Custodia del repositorio en este ordenador</h2><p>");
    h.push_str(&match log {
        None => "En este ordenador no hay registro del repositorio (el repositorio no se ha usado aquí).".to_string(),
        Some((n, None)) => format!("<span class='ok'>Registro de actividad íntegro</span>: {n} anotaciones encadenadas con huellas SHA-256, sin ninguna alterada."),
        Some((n, Some(bad))) => format!("<span class='no'>Registro de actividad alterado</span>: de {n} anotaciones, la cadena se rompe en la {bad}."),
    });
    h.push_str("</p>");
    // 6. pendiente
    h.push_str("<h2>6. Pendiente antes de la certificación</h2><ul>");
    if host.is_empty() {
        h.push_str("<li>Publicar el repositorio en un servidor siempre encendido, con nombre público y certificado TLS (SC06, SC08, SC09).</li>");
    }
    for ctl in CONTROLS.iter().filter(|c| matches!(c.kind, Kind::Parcial | Kind::Instalacion | Kind::Documento)) {
        h.push_str(&format!("<li><b>{}</b> · {}: {}</li>", ctl.code, esc(ctl.title), esc(ctl.how)));
    }
    if steps.iter().any(|s| !s.ok) {
        h.push_str("<li class='no'>Revisar las pruebas del circuito que fallan.</li>");
    }
    h.push_str("<li>Prueba conjunta con Nodofarma en su entorno de preproducción (FT13, FT14).</li><li>Solicitud a los Consejos Generales con este expediente y la documentación acreditativa de cada control.</li></ul>");
    h.push_str("<p class='muted'>Los títulos de los controles son un resumen; el texto que revisa el auditor es el del documento de requisitos de los Consejos Generales.</p></body></html>");
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_whole_circuit_passes_with_a_simulated_pharmacy() {
        let steps = circuit_test(NaiveDate::from_ymd_opt(2026, 11, 11).unwrap());
        assert!(steps.len() >= 12);
        for s in &steps {
            assert!(s.ok, "{} · {} · {}", s.control, s.what, s.detail);
        }
    }

    #[test]
    fn every_audit_control_is_listed_once() {
        let mut codes: Vec<&str> = CONTROLS.iter().map(|c| c.code).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), 30);
        for p in ["FT", "FD", "SC", "IN", "PD"] {
            assert!(codes.iter().any(|c| c.starts_with(p)));
        }
        assert_eq!(tally().iter().map(|t| t.1).sum::<usize>(), 30);
        // lo que falta se dice: revocación de certificados y disponibilidad 24 horas
        for code in ["IN03", "SC06"] {
            assert!(CONTROLS.iter().any(|c| c.code == code && c.kind != Kind::Programa));
        }
        // el acceso del paciente ya lo cubre el programa («Mis recetas»)
        assert!(CONTROLS.iter().any(|c| c.code == "SC03" && c.kind == Kind::Programa));
    }

    #[test]
    fn the_dossier_has_the_system_the_controls_and_the_tests() {
        let db = Db::sample(NaiveDate::from_ymd_opt(2026, 11, 11).unwrap());
        let now = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap().and_hms_opt(9, 30, 0).unwrap();
        let steps = circuit_test(now.date());
        let html = dossier_html(&db, &steps, Some((12, None)), now);
        assert!(html.contains("URLRepositorio") && html.contains("pendiente"));
        assert!(html.contains("FT14") && html.contains("PD02") && html.contains("IN03"));
        assert!(html.contains(&format!("{} de {} correctas", steps.len(), steps.len())));
        assert!(html.contains("íntegro"));
        assert!(html.contains("Elena Ruiz Prado"), "los prescriptores con su colegiado");
        assert!(!html.contains("<script"));
    }
}
