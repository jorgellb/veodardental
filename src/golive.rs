//! Puesta en marcha: lo que falta para trabajar con la clínica real.
//!
//! Cada paso se comprueba con la configuración de verdad (nada se marca a
//! mano), salvo lo que el programa no puede saber, como la revisión legal,
//! que la clínica marca cuando lo ha hecho. También está aquí «empezar con
//! la clínica vacía», que quita los pacientes y los datos de ejemplo.

use crate::model::*;
use crate::screens::config::CfgTab;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Cómo está un paso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Listo,
    /// Falta, y hace falta para trabajar con la clínica real.
    Pendiente,
    /// Sin configurar, pero no hace falta para empezar.
    Opcional,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Listo => "Listo",
            Status::Pendiente => "Pendiente",
            Status::Opcional => "Opcional",
        }
    }
}

/// Adónde lleva el botón de un paso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goto {
    Cfg(CfgTab),
    Colaboradores,
    /// Empezar con la clínica vacía.
    Vaciar,
    /// Paso que la clínica marca a mano (su clave).
    Marcar(&'static str),
    Nada,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub key: &'static str,
    pub title: &'static str,
    pub status: Status,
    /// Qué hay hecho o qué falta, en una o dos frases.
    pub detail: String,
    pub goto: Goto,
    /// Imprescindible para trabajar con la clínica real.
    pub essential: bool,
}

/// Lo que se sabe de este ordenador (se lee del disco cada pocos segundos).
#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub now: NaiveDateTime,
    /// Copia de seguridad más reciente en este ordenador.
    pub last_backup: Option<NaiveDateTime>,
    /// La carpeta de la segunda copia existe (el disco está conectado).
    pub extra_found: bool,
    /// Hay contraseña de copias en este ordenador (la segunda copia sale cifrada).
    pub backup_key: bool,
    /// Puesto conectado al servidor de la clínica (las copias las hace el servidor).
    pub remote: bool,
    /// Varios ordenadores trabajando juntos.
    pub network: bool,
    /// Certificado de la clínica para la AEAT elegido en este ordenador.
    pub cert: bool,
    /// WhatsApp activado en este ordenador.
    pub whatsapp: bool,
    /// Cifrado del disco de los datos de este ordenador.
    pub disk: crate::disco::Disk,
}

/// Paso marcado a mano por la clínica.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tick {
    pub key: String,
    pub when: NaiveDate,
    pub user: String,
}

/// Clave del paso de protección de datos y textos legales.
pub const LEGAL: &str = "legal";
/// Acción del registro de cambios al vaciar la clínica.
pub const EMPTY_ACTION: &str = "Empezar con la clínica vacía";

// ───────────────────────────── datos de ejemplo ─────────────────────────────

/// Lo que traen los datos de ejemplo, para reconocerlo.
struct Demo {
    patients: HashMap<u32, (String, String, String)>,
    doctors: Vec<(u32, String)>,
    suppliers: Vec<(u32, String)>,
    products: Vec<(u32, String)>,
    contacts: Vec<(u32, String)>,
    tariffs: Vec<(u32, String)>,
    centers: Vec<(u32, String)>,
    boxes: Vec<(u32, String)>,
    banks: Vec<(u32, String)>,
    config: ClinicConfig,
}

fn demo() -> &'static Demo {
    static DEMO: OnceLock<Demo> = OnceLock::new();
    DEMO.get_or_init(|| {
        let s = Db::sample(today());
        Demo {
            patients: s.patients.iter().map(|p| (p.id, (p.first_name.clone(), p.last_name.clone(), p.nif.clone()))).collect(),
            doctors: s.collaborators.iter().map(|c| (c.id, c.name.clone())).collect(),
            suppliers: s.suppliers.iter().map(|x| (x.id, x.name.clone())).collect(),
            products: s.products.iter().map(|x| (x.id, x.name.clone())).collect(),
            contacts: s.contacts.iter().map(|x| (x.id, x.name.clone())).collect(),
            tariffs: s.tariffs.iter().filter(|t| t.is_insurer).map(|t| (t.id, t.name.clone())).collect(),
            centers: s.centers.iter().map(|c| (c.id, c.name.clone())).collect(),
            boxes: s.boxes.iter().map(|b| (b.id, b.name.clone())).collect(),
            banks: s.bank_accounts.iter().map(|b| (b.id, b.iban.clone())).collect(),
            config: s.config.clone(),
        }
    })
}

fn is_demo(list: &[(u32, String)], id: u32, name: &str) -> bool {
    list.iter().any(|(i, n)| *i == id && n == name)
}

/// ¿Es uno de los pacientes de ejemplo (sin cambiar)?
pub fn is_demo_patient(p: &Patient) -> bool {
    demo().patients.get(&p.id).is_some_and(|(f, l, n)| *f == p.first_name && *l == p.last_name && *n == p.nif)
}

pub fn demo_patients(db: &Db) -> usize {
    db.patients.iter().filter(|p| is_demo_patient(p)).count()
}

/// Facturas ya aceptadas por la AEAT en el envío real: no se pueden borrar.
pub fn sent_to_aeat(db: &Db) -> usize {
    if db.verifactu.env != crate::verifactu::Env::Produccion {
        return 0;
    }
    db.invoices.iter().filter(|i| matches!(i.aeat.state.as_str(), "Correcto" | "AceptadoConErrores")).count()
}

impl Db {
    /// Empieza con la clínica vacía: quita todos los pacientes con lo que
    /// cuelga de ellos (citas, historial, presupuestos, facturas, cobros,
    /// imágenes, documentos, mensajes...), los movimientos (caja, banco,
    /// almacén, gastos) y los proveedores, productos, contactos, cuentas,
    /// aseguradoras y centros de ejemplo. Conserva la configuración, los
    /// tratamientos y sus precios, la tarifa general, los profesionales,
    /// los gabinetes, los usuarios y las plantillas. Devuelve cuántos
    /// pacientes se quitaron.
    pub fn start_empty(&mut self) -> Result<usize, String> {
        let sent = sent_to_aeat(self);
        if sent > 0 {
            return Err(format!("Hay {sent} facturas aceptadas por la AEAT: no se pueden borrar, así que no se vacía la clínica."));
        }
        let d = demo();
        let n = self.patients.len();
        // Se nombran todos los campos: si se añade uno nuevo a los datos, hay que decidir aquí qué se hace con él.
        let Db {
            config,
            treatments: _,
            tariffs,
            collaborators,
            boxes,
            patients,
            entries,
            budgets,
            payments,
            cash,
            appointments,
            blocks,
            waitlist,
            recalls,
            next_id: _,
            invoices,
            perio,
            images,
            connectors: _,
            launches,
            msg_templates: _,
            messages,
            doc_templates: _,
            documents,
            suppliers,
            products,
            stock_moves,
            consumptions,
            orders,
            lab_works,
            users: _,
            access_log: _,
            expenses,
            drugs: _,
            prescriptions,
            ortho,
            remittances,
            contacts,
            bank_accounts,
            bank_moves,
            audit_log: _,
            centers,
            holidays: _,
            health,
            claims,
            auths,
            erx: _,
            tasks,
            rev: _,
            stations: _,
            chat,
            calls,
            whatsapp,
            wa_seen: _,
            wa_sender: _,
            verifactu: _,
            mail: _,
            vf_records,
            ia: _,
            ia_reports,
            booking: _,
            booking_requests,
            appt_links,
            budget_links,
            doc_links,
            slot_offers,
            golive: _,
            rx_links,
            imports,
            measures,
            rights_requests,
            pay_requests,
            evo_notes,
            alert_acks,
            sterile_cycles,
            sterile_packs,
            surveys,
            portal_access,
            campaigns,
            group_keys: _,
            card_txs,
            financings,
            auth_requests,
            punches,
            data_version: _,
            station: _,
        } = self;
        // los pacientes y todo lo suyo, y los movimientos
        patients.clear();
        entries.clear();
        budgets.clear();
        payments.clear();
        cash.clear();
        appointments.clear();
        // los bloqueos de un día concreto se van; los de todos los días (la comida) son horario y se quedan
        blocks.retain(|b| b.date.is_none());
        waitlist.clear();
        recalls.clear();
        invoices.clear();
        perio.clear();
        images.clear();
        launches.clear();
        messages.clear();
        documents.clear();
        stock_moves.clear();
        consumptions.clear();
        orders.clear();
        lab_works.clear();
        expenses.clear();
        prescriptions.clear();
        ortho.clear();
        remittances.clear();
        bank_moves.clear();
        health.clear();
        claims.clear();
        auths.clear();
        tasks.clear();
        chat.clear();
        calls.clear();
        whatsapp.clear();
        vf_records.clear();
        ia_reports.clear();
        booking_requests.clear();
        appt_links.clear();
        budget_links.clear();
        doc_links.clear();
        slot_offers.clear();
        rx_links.clear();
        imports.clear();
        measures.clear();
        rights_requests.clear();
        pay_requests.clear();
        evo_notes.clear();
        alert_acks.clear();
        sterile_cycles.clear();
        sterile_packs.clear();
        surveys.clear();
        portal_access.clear();
        campaigns.clear();
        card_txs.clear();
        financings.clear();
        auth_requests.clear();
        punches.clear();
        // lo de ejemplo que no es de ningún paciente
        suppliers.retain(|s| !is_demo(&d.suppliers, s.id, &s.name));
        products.retain(|p| !is_demo(&d.products, p.id, &p.name));
        for p in products.iter_mut() {
            if !suppliers.iter().any(|s| s.id == p.supplier_id) {
                p.supplier_id = 0;
            }
        }
        contacts.retain(|c| !is_demo(&d.contacts, c.id, &c.name));
        bank_accounts.retain(|b| !is_demo(&d.banks, b.id, &b.iban));
        tariffs.retain(|t| !(t.is_insurer && is_demo(&d.tariffs, t.id, &t.name)));
        // centros de ejemplo (siempre queda uno) y sus gabinetes
        let first = centers.iter().map(|c| c.id).min();
        let gone: Vec<u32> = centers.iter().filter(|c| Some(c.id) != first && is_demo(&d.centers, c.id, &c.name)).map(|c| c.id).collect();
        if let Some(first) = first {
            centers.retain(|c| !gone.contains(&c.id));
            boxes.retain(|b| !(gone.contains(&b.center_id) && is_demo(&d.boxes, b.id, &b.name)));
            for b in boxes.iter_mut() {
                if gone.contains(&b.center_id) {
                    b.center_id = first;
                }
            }
            for c in collaborators.iter_mut() {
                c.centers.retain(|x| !gone.contains(x));
            }
            if gone.contains(&config.current_center) {
                config.current_center = first;
            }
        }
        config.next_receipt = 1;
        self.audit(EMPTY_ACTION, &format!("Se quitaron {n} pacientes con sus citas, historial, presupuestos, facturas y cobros, y los datos de ejemplo"));
        Ok(n)
    }
}

// ───────────────────────────── comprobaciones ─────────────────────────────

/// NIF, NIE o CIF español con su letra o dígito de control correcto.
pub fn nif_ok(s: &str) -> bool {
    let s: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    if s.len() != 9 {
        return false;
    }
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(|c| c.is_ascii_digit());
    const LETTERS: &[u8] = b"TRWAGMYFPDXBNJZSQVHLCKE";
    let dni = |num: &str, letter: u8| num.parse::<u32>().map(|n| LETTERS[(n % 23) as usize] == letter).unwrap_or(false);
    match b[0] {
        b'0'..=b'9' => digits(0..8) && dni(&s[..8], b[8]),
        b'X' | b'Y' | b'Z' => digits(1..8) && dni(&format!("{}{}", b[0] - b'X', &s[1..8]), b[8]),
        b'K' | b'L' | b'M' => digits(1..8) && dni(&s[1..8], b[8]),
        c if b"ABCDEFGHJNPQRSUVW".contains(&c) => {
            if !digits(1..8) {
                return false;
            }
            let sum: u32 = b[1..8]
                .iter()
                .enumerate()
                .map(|(i, x)| {
                    let x = (x - b'0') as u32;
                    if i % 2 == 0 { (x * 2) / 10 + (x * 2) % 10 } else { x }
                })
                .sum();
            let k = ((10 - sum % 10) % 10) as usize;
            let (letter, digit) = (b"JABCDEFGHI"[k], b'0' + k as u8);
            match c {
                b'P' | b'Q' | b'R' | b'S' | b'N' | b'W' => b[8] == letter,
                b'A' | b'B' | b'E' | b'H' => b[8] == digit,
                _ => b[8] == letter || b[8] == digit,
            }
        }
        _ => false,
    }
}

/// «a», «a y b», «a, b y c».
fn join_es(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [rest @ .., last] => format!("{} y {}", rest.join(", "), last),
    }
}

fn when(t: NaiveDateTime) -> String {
    format!("{} a las {}", fmt_date(t.date()), t.format("%H:%M"))
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max - 1).collect::<String>()) }
}

/// Todos los pasos, primero los imprescindibles.
pub fn checks(db: &Db, f: &Facts) -> Vec<Check> {
    use Status::*;
    let d = demo();
    let mut v = Vec::new();
    let mut add = |key, title, essential, goto, (status, detail): (Status, String)| v.push(Check { key, title, status, detail, goto, essential });

    // datos de la clínica
    {
        let c = &db.config;
        let (mut sample, mut missing) = (vec![], vec![]);
        for (name, val, demo_val) in [("el nombre", &c.name, &d.config.name), ("el NIF", &c.nif, &d.config.nif), ("la dirección", &c.address, &d.config.address), ("el teléfono", &c.phone, &d.config.phone), ("el email", &c.email, &d.config.email)] {
            if val.trim().is_empty() {
                missing.push(name.to_string());
            } else if val.trim() == demo_val.as_str() || fold(val).contains("ejemplo") {
                sample.push(name.to_string());
            }
        }
        add(
            "clinica",
            "Datos de la clínica",
            true,
            Goto::Cfg(CfgTab::Clinica),
            if !sample.is_empty() {
                (Pendiente, format!("Siguen los datos de ejemplo: cambie {} por los de la clínica (salen en facturas, recibos y documentos).", join_es(&sample)))
            } else if !missing.is_empty() {
                (Pendiente, format!("{} {} (salen en facturas, recibos y documentos).", if missing.len() > 1 { "Faltan" } else { "Falta" }, join_es(&missing)))
            } else if !nif_ok(&c.nif) {
                (Pendiente, format!("El NIF «{}» no es válido: revise la letra o el número final.", c.nif.trim()))
            } else {
                (Listo, format!("{} · NIF {}", c.name.trim(), c.nif.trim()))
            },
        );
    }

    // datos de ejemplo
    {
        let demo_n = demo_patients(db);
        let real_n = db.patients.len() - demo_n;
        let sent = sent_to_aeat(db);
        let (st, goto) = if demo_n == 0 {
            ((Listo, if db.patients.is_empty() { "Sin datos de ejemplo: la clínica está lista para sus pacientes.".to_string() } else { format!("{} pacientes, ninguno de ejemplo.", db.patients.len()) }), Goto::Nada)
        } else if sent > 0 {
            ((Pendiente, format!("Quedan {demo_n} pacientes de ejemplo, pero ya hay facturas aceptadas por la AEAT y la clínica no se puede vaciar.")), Goto::Nada)
        } else {
            let also = if real_n > 0 { format!(" Atención: también hay {real_n} pacientes que no son de ejemplo, y se quitarían igual.") } else { String::new() };
            (
                (
                    Pendiente,
                    format!(
                        "El programa trae {demo_n} pacientes de ejemplo con citas, historial, presupuestos y facturas. Antes de trabajar de verdad, vacíe la clínica: se conservan la configuración, los tratamientos y sus precios, los profesionales y los usuarios.{also}"
                    ),
                ),
                Goto::Vaciar,
            )
        };
        add("ejemplo", "Datos de ejemplo", true, goto, st);
    }

    // profesionales
    {
        let active: Vec<&Collaborator> = db.collaborators.iter().filter(|c| c.active).collect();
        let sample: Vec<String> = active.iter().filter(|c| is_demo(&d.doctors, c.id, &c.name)).map(|c| c.alias.clone()).collect();
        let no_license: Vec<String> = active.iter().filter(|c| c.license.trim().is_empty() && !fold(&c.specialty).contains("higien")).map(|c| c.alias.clone()).collect();
        add(
            "profesionales",
            "Profesionales",
            true,
            Goto::Colaboradores,
            if active.is_empty() {
                (Pendiente, "No hay ningún profesional activo: añada los de la clínica (son las columnas de la agenda).".into())
            } else if !sample.is_empty() {
                (Pendiente, format!("Siguen los profesionales de ejemplo ({}): ponga los nombres y el nº de colegiado de los de la clínica, o desactívelos.", join_es(&sample)))
            } else if !no_license.is_empty() {
                (Pendiente, format!("Falta el nº de colegiado de {} (hace falta para las recetas).", join_es(&no_license)))
            } else {
                (Listo, if active.len() == 1 { "1 profesional activo.".into() } else { format!("{} profesionales activos.", active.len()) })
            },
        );
    }

    // usuarios y contraseñas
    {
        let active = db.users.iter().filter(|u| u.active).count();
        let initial: Vec<String> = db.users.iter().filter(|u| u.active && u.must_change).map(|u| u.name.clone()).collect();
        add(
            "usuarios",
            "Usuarios y contraseñas",
            true,
            Goto::Cfg(CfgTab::Usuarios),
            if initial.is_empty() {
                (Listo, format!("{active} usuarios activos, cada uno con su contraseña."))
            } else {
                (Pendiente, format!("Con la contraseña inicial: {}. Cada uno elige la suya al entrar; desactive los que no se vayan a usar.", join_es(&initial)))
            },
        );
    }

    // copias de seguridad
    {
        let extra = db.config.backup_extra.trim();
        let st = if f.remote {
            if extra.is_empty() {
                (Pendiente, "El servidor de la clínica hace una copia cada día, pero falta una segunda copia en otro disco o en la nube, por si el servidor se estropea.".to_string())
            } else {
                (Listo, format!("Copia diaria en el servidor de la clínica y segunda copia en {extra}."))
            }
        } else {
            match f.last_backup.filter(|b| f.now - *b < Duration::days(2)) {
                None => (Pendiente, "No hay ninguna copia de los últimos días en este ordenador: haga una ahora.".to_string()),
                Some(b) if extra.is_empty() => {
                    (Pendiente, format!("Se hace una copia cada día en este ordenador (la última, el {}), pero si se estropea el disco se pierden las dos: elija una segunda carpeta en un disco externo o en la nube.", when(b)))
                }
                Some(_) if !f.extra_found => (Pendiente, format!("No se encuentra la carpeta de la segunda copia ({extra}): ¿está conectado el disco?")),
                Some(_) if !f.backup_key => (Pendiente, "La segunda copia sale del ordenador sin cifrar, con los datos de salud de todos los pacientes: póngale una contraseña para que se guarde cifrada.".to_string()),
                Some(b) => (Listo, format!("Última copia el {}; segunda copia cifrada en {extra}.", when(b))),
            }
        };
        add("copias", "Copias de seguridad", true, Goto::Cfg(CfgTab::Copias), st);
    }

    // cifrado del disco (los datos, las copias y las claves de los servicios están en él)
    {
        use crate::disco::Disk;
        let (essential, st) = match &f.disk {
            Disk::Encrypted(how) => (true, (Listo, format!("{how}: si roban el ordenador o el disco, los datos no se pueden leer."))),
            Disk::Plain(why) => (true, (Pendiente, format!("{why} Si roban el ordenador o el disco, se pueden leer los datos de salud de todos los pacientes y las claves del correo, de WhatsApp y de la IA."))),
            Disk::Unknown(why) => (false, (Opcional, why.clone())),
            Disk::Checking => (false, (Opcional, "Comprobando el cifrado del disco…".to_string())),
        };
        add("disco", "Cifrado del disco", essential, Goto::Nada, st);
    }

    // VERI*FACTU
    {
        let vf = &db.verifactu;
        let st = if let Some(t) = crate::foral::foral_territory(&vf.foral) {
            (Pendiente, crate::foral::pending_text(t).to_string())
        } else if !vf.enabled {
            (Pendiente, "Sin activar. La ley obliga a que el programa de facturación registre cada factura y la envíe a la AEAT: pregunte a su asesor desde qué fecha le toca y actívelo con el certificado de la clínica.".to_string())
        } else if vf.env == crate::verifactu::Env::Pruebas {
            (Pendiente, "Activado en el entorno de pruebas de la AEAT, que no tiene efectos fiscales. Cuando las pruebas vayan bien, cambie al envío real.".to_string())
        } else if !f.cert {
            (Pendiente, "En envío real, pero este ordenador no tiene el certificado de la clínica: elíjalo aquí o compruebe que lo tiene el ordenador que envía.".to_string())
        } else {
            (Listo, format!("Envío real a la AEAT{}.", if vf.auto_send { ", automático al emitir cada factura" } else { ", a mano desde Facturas" }))
        };
        add("verifactu", "Facturas a la AEAT (VERI*FACTU)", true, Goto::Cfg(CfgTab::Facturacion), st);
    }

    // protección de datos y textos legales (se marca a mano)
    {
        let st = match db.golive.iter().find(|t| t.key == LEGAL) {
            Some(t) => (Listo, format!("Revisado el {}{}.", fmt_date(t.when), if t.user.is_empty() { String::new() } else { format!(" por {}", t.user) })),
            None => (Pendiente, "Revise con su asesor los consentimientos y la información de protección de datos que da el programa, y firme el contrato de encargado del tratamiento con cada servicio externo que active (correo, WhatsApp, inteligencia artificial, publicación en internet). Márquelo cuando esté hecho.".to_string()),
        };
        add(LEGAL, "Protección de datos y textos legales", true, Goto::Marcar(LEGAL), st);
    }

    // ── opcionales ──

    // datos de otro programa (solo cuenta lo importado después de vaciar la clínica)
    {
        let since = db.audit_log.iter().rposition(|a| a.action == EMPTY_ACTION).map(|i| i + 1).unwrap_or(0);
        let st = match db.audit_log[since..].iter().rev().find(|a| a.action.starts_with("Importación")) {
            Some(a) => (Listo, format!("Importado el {}: {}", fmt_date(a.when.date()), short(&a.detail, 90))),
            None => (Opcional, "Si viene de GESDEN u otro programa, traiga sus pacientes, tratamientos y tarifas (después de vaciar la clínica de ejemplo).".to_string()),
        };
        add("importar", "Sus datos de otro programa", false, Goto::Cfg(CfgTab::Importar), st);
    }

    // correo
    {
        let m = &db.mail;
        let st = if m.ready() {
            (Listo, format!("Envía desde {}{}.", m.from_email.trim(), if m.auto_send { format!("; los avisos automáticos salen cada día entre las {}:00 y las {}:00", m.auto_hour, m.auto_until) } else { String::new() }))
        } else {
            (Opcional, "Para mandar presupuestos, recordatorios, enlaces y órdenes al laboratorio por correo, con la cuenta de la clínica.".to_string())
        };
        add("correo", "Correo electrónico", false, Goto::Cfg(CfgTab::Correo), st);
    }

    // cita online y firma por internet
    {
        let b = &db.booking;
        let url = b.public_url.trim();
        let st = if b.enabled && url.starts_with("https://") {
            (Listo, format!("Publicada en {url}."))
        } else if b.enabled {
            (Opcional, "Activada solo dentro de la clínica: publíquela en internet para que los pacientes la vean desde casa.".to_string())
        } else {
            (Opcional, "Para que los pacientes pidan cita, la confirmen y firmen presupuestos y consentimientos desde el móvil.".to_string())
        };
        add("online", "Cita online y firma por internet", false, Goto::Cfg(CfgTab::CitaOnline), st);
    }

    // WhatsApp
    add(
        "whatsapp",
        "WhatsApp",
        false,
        Goto::Cfg(CfgTab::Whatsapp),
        if f.whatsapp || !db.wa_sender.trim().is_empty() { (Listo, "Recordatorios por WhatsApp activados.".into()) } else { (Opcional, "Recordatorios y confirmación de citas por WhatsApp (hace falta una cuenta de WhatsApp Business).".into()) },
    );

    // inteligencia artificial
    {
        let n = db.ia.agents.iter().filter(|a| a.enabled && (!a.provider.needs_key() || !a.api_key.trim().is_empty())).count();
        add(
            "ia",
            "Inteligencia artificial",
            false,
            Goto::Cfg(CfgTab::Ia),
            if db.ia.enabled && n > 0 {
                (Listo, if n == 1 { "1 servicio de análisis preparado.".into() } else { format!("{n} servicios de análisis preparados.") })
            } else {
                (Opcional, "Análisis de radiografías con inteligencia artificial (hace falta un proveedor y su contrato de datos).".into())
            },
        );
    }

    // receta electrónica
    {
        let e = &db.erx;
        add(
            "receta",
            "Receta electrónica",
            false,
            Goto::Cfg(CfgTab::Receta),
            if e.direct {
                (Listo, "Conexión directa con las farmacias.".into())
            } else if !e.platform.trim().is_empty() {
                (Listo, format!("Con {}.", e.platform.trim()))
            } else {
                (Opcional, "Para hacer recetas oficiales con una plataforma de prescripción homologada.".into())
            },
        );
    }

    // varios ordenadores
    add(
        "red",
        "Varios ordenadores",
        false,
        Goto::Cfg(CfgTab::Red),
        if f.network { (Listo, "Este ordenador trabaja en red con los demás puestos de la clínica.".into()) } else { (Opcional, "Si la clínica usa el programa en más de un ordenador, conéctelos con el servidor de la clínica.".into()) },
    );
    v
}

/// Pasos imprescindibles que faltan.
pub fn pending(v: &[Check]) -> usize {
    v.iter().filter(|c| c.essential && c.status != Status::Listo).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 11).unwrap()
    }

    fn get<'a>(v: &'a [Check], key: &str) -> &'a Check {
        v.iter().find(|c| c.key == key).unwrap()
    }

    fn ready_facts() -> Facts {
        let now = day().and_hms_opt(12, 0, 0).unwrap();
        Facts { now, last_backup: Some(now - Duration::hours(3)), extra_found: true, backup_key: true, cert: true, disk: crate::disco::Disk::Encrypted("BitLocker activado en C:".into()), ..Default::default() }
    }

    #[test]
    fn spanish_tax_ids() {
        for ok in ["12345678Z", "12.345.678-z", "X1234567L", "A58818501", "Q2826000H", "B00000000"] {
            assert!(nif_ok(ok), "{ok} debería ser válido");
        }
        for bad in ["12345678A", "X1234567A", "A58818502", "Q2826000A", "B0000000", "", "ABCDEFGHI"] {
            assert!(!nif_ok(bad), "{bad} no debería ser válido");
        }
    }

    #[test]
    fn demo_clinic_needs_the_essentials() {
        let db = Db::sample(day());
        let v = checks(&db, &Facts::default());
        for key in ["clinica", "ejemplo", "profesionales", "usuarios", "copias", "verifactu", LEGAL] {
            assert_eq!(get(&v, key).status, Status::Pendiente, "{key}: {}", get(&v, key).detail);
            assert!(get(&v, key).essential);
        }
        assert_eq!(pending(&v), 7);
        assert_eq!(get(&v, "ejemplo").goto, Goto::Vaciar);
        assert!(get(&v, "clinica").detail.contains("el NIF"), "{}", get(&v, "clinica").detail);
        assert!(get(&v, "profesionales").detail.contains("Dra. Ruiz"));
        assert!(get(&v, "usuarios").detail.contains("Recepción"));
        // lo opcional no cuenta como pendiente
        assert!(v.iter().filter(|c| !c.essential).all(|c| c.status != Status::Pendiente));
    }

    /// T-PROC-01. Ningún servicio real se activa solo. Tanto con los datos de
    /// ejemplo como con la clínica vacía, y con los ajustes de cada
    /// ordenador sin tocar, están apagados o en pruebas:
    /// - la AEAT (VERI*FACTU), el correo, el TPV del banco y la receta electrónica;
    /// - la IA, sin claves, y la transcripción;
    /// - WhatsApp, los SMS, el datáfono (a mano) y las páginas públicas.
    ///
    /// Ninguna campaña está programada.
    #[test]
    fn every_real_service_starts_switched_off() {
        let sample = Db::sample(day());
        let mut empty = sample.clone();
        empty.start_empty().unwrap();
        for db in [&sample, &empty] {
            assert!(!db.verifactu.enabled && !db.verifactu.auto_send && db.verifactu.env == crate::verifactu::Env::Pruebas, "AEAT");
            assert!(!db.mail.enabled && !db.mail.auto_send, "correo");
            assert!(!db.ia.enabled && db.ia.agents.iter().all(|a| a.api_key.is_empty()), "IA");
            assert!(!db.booking.enabled, "páginas públicas");
            assert!(!db.config.pay_online.enabled, "pago por internet");
            assert!(db.erx.url.is_empty() && db.erx.repo_addr.is_empty(), "receta electrónica");
            assert!(db.campaigns.iter().all(|c| c.state != crate::campanas::CampState::Programada), "campañas");
        }
        // los ajustes de cada ordenador, de serie
        assert!(!crate::whatsapp::WaSettings::default().enabled, "WhatsApp");
        assert!(!crate::sms::SmsSettings::default().enabled(), "SMS");
        assert!(!crate::evolucion::SttSettings::default().enabled(), "transcripción");
        assert!(crate::datafono::Settings::default().url.is_empty(), "datáfono: a mano");
        let tpv = crate::pago::TpvSettings::default();
        assert!(tpv.env == crate::pago::Env::Pruebas && tpv.problem().is_some(), "TPV: en pruebas y sin datos");
    }

    #[test]
    fn empty_clinic_keeps_the_setup() {
        let mut db = Db::sample(day());
        let (treatments, users, doctors, templates) = (db.treatments.len(), db.users.len(), db.collaborators.len(), db.msg_templates.len());
        // un proveedor y un producto de la clínica, que se quedan
        db.suppliers.push(crate::stock::Supplier { id: 90, name: "Mi depósito".into(), nif: String::new(), phone: String::new(), email: String::new(), contact: String::new(), is_lab: false, notes: String::new(), auto_send: false });
        let mut mine = db.products[0].clone();
        mine.id = 90;
        mine.name = "Guantes de la clínica".into();
        mine.supplier_id = 1;
        db.products.push(mine);
        let patients = db.patients.len();
        assert!(patients > 0 && !db.appointments.is_empty() && !db.invoices.is_empty() && !db.payments.is_empty());
        assert_eq!(db.start_empty(), Ok(patients));
        assert!(db.patients.is_empty() && db.appointments.is_empty() && db.invoices.is_empty() && db.payments.is_empty() && db.entries.is_empty() && db.budgets.is_empty() && db.cash.is_empty());
        assert!(db.messages.is_empty() && db.images.is_empty() && db.documents.is_empty() && db.recalls.is_empty() && db.stock_moves.is_empty());
        assert_eq!((db.treatments.len(), db.users.len(), db.collaborators.len(), db.msg_templates.len()), (treatments, users, doctors, templates));
        // de ejemplo fuera; lo de la clínica se queda, sin el proveedor de ejemplo
        assert_eq!(db.suppliers.iter().map(|s| s.id).collect::<Vec<_>>(), vec![90]);
        assert_eq!(db.products.len(), 1);
        assert_eq!(db.products[0].supplier_id, 0);
        assert!(db.contacts.is_empty() && db.bank_accounts.is_empty());
        assert_eq!(db.tariffs.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["Privado"]);
        assert_eq!(db.centers.len(), 1);
        assert!(db.boxes.iter().all(|b| b.center_id == db.centers[0].id) && !db.boxes.is_empty());
        assert!(db.collaborators.iter().all(|c| c.centers.iter().all(|x| *x == db.centers[0].id)));
        assert_eq!(db.config.next_receipt, 1);
        assert_eq!(db.audit_log.last().map(|a| a.action.as_str()), Some(EMPTY_ACTION));
        let v = checks(&db, &Facts::default());
        assert_eq!(get(&v, "ejemplo").status, Status::Listo);
        // el programa sigue funcionando con la clínica vacía (se guarda y se lee)
        let back: Db = serde_json::from_str(&serde_json::to_string(&db).unwrap()).unwrap();
        assert!(back.patients.is_empty());
    }

    #[test]
    fn invoices_accepted_by_the_tax_agency_are_never_deleted() {
        let mut db = Db::sample(day());
        db.verifactu.env = crate::verifactu::Env::Produccion;
        db.invoices[0].aeat.state = "Correcto".into();
        let before = (db.patients.len(), db.invoices.len());
        assert!(db.start_empty().is_err());
        assert_eq!((db.patients.len(), db.invoices.len()), before);
        let v = checks(&db, &Facts::default());
        assert_eq!(get(&v, "ejemplo").goto, Goto::Nada);
        // en el entorno de pruebas no cuenta
        db.verifactu.env = crate::verifactu::Env::Pruebas;
        assert!(db.start_empty().is_ok());
    }

    #[test]
    fn a_configured_clinic_is_ready() {
        let mut db = Db::sample(day());
        db.start_empty().unwrap();
        let c = &mut db.config;
        c.name = "Clínica Dental Sonrisa".into();
        c.nif = "B12345674".into();
        c.address = "C/ Real 3, 28400 Collado Villalba".into();
        c.phone = "91 851 00 00".into();
        c.email = "hola@sonrisa.es".into();
        c.backup_extra = "/media/usb/copias".into();
        assert!(nif_ok(&c.nif));
        for d in db.collaborators.iter_mut() {
            d.name.push_str(" (real)");
            d.license = "28-1234".into();
        }
        for u in db.users.iter_mut() {
            u.must_change = false;
        }
        db.verifactu.enabled = true;
        db.verifactu.env = crate::verifactu::Env::Produccion;
        let f = ready_facts();
        let v = checks(&db, &f);
        assert_eq!(pending(&v), 1, "solo falta la revisión legal: {:?}", v.iter().filter(|c| c.essential && c.status != Status::Listo).map(|c| (c.key, &c.detail)).collect::<Vec<_>>());
        db.golive.push(Tick { key: LEGAL.into(), when: day(), user: "Dirección".into() });
        let v = checks(&db, &f);
        assert_eq!(pending(&v), 0);
        assert!(get(&v, LEGAL).detail.contains("Dirección"));
        // sin el disco de la segunda copia, vuelve a faltar
        let v = checks(&db, &Facts { extra_found: false, ..ready_facts() });
        assert_eq!(get(&v, "copias").status, Status::Pendiente);
        assert!(get(&v, "copias").detail.contains("conectado"));
        // segunda copia sin contraseña: los datos de salud salen sin cifrar
        let v = checks(&db, &Facts { backup_key: false, ..ready_facts() });
        assert_eq!(get(&v, "copias").status, Status::Pendiente);
        assert!(get(&v, "copias").detail.contains("sin cifrar"));
        assert!(get(&checks(&db, &f), "copias").detail.contains("cifrada"));
        // disco sin cifrar: falta (si no se sabe, solo se avisa)
        let v = checks(&db, &Facts { disk: crate::disco::Disk::Plain("C: no está cifrado.".into()), ..ready_facts() });
        assert_eq!(get(&v, "disco").status, Status::Pendiente);
        assert!(get(&v, "disco").detail.contains("datos de salud"));
        let v = checks(&db, &Facts { disk: crate::disco::Disk::Unknown("carpeta de red".into()), ..ready_facts() });
        assert!(!get(&v, "disco").essential);
        assert_eq!(get(&checks(&db, &f), "disco").status, Status::Listo);
        // NIF mal escrito
        db.config.nif = "B12345670".into();
        let v = checks(&db, &f);
        assert!(get(&v, "clinica").detail.contains("no es válido"));
    }

    #[test]
    fn only_imports_after_emptying_count() {
        let mut db = Db::sample(day());
        db.audit("Importación", "300 pacientes de GESDEN");
        assert_eq!(get(&checks(&db, &Facts::default()), "importar").status, Status::Listo);
        db.start_empty().unwrap();
        assert_eq!(get(&checks(&db, &Facts::default()), "importar").status, Status::Opcional);
        db.audit("Importación", "1.200 pacientes nuevos");
        let v = checks(&db, &Facts::default());
        assert_eq!(get(&v, "importar").status, Status::Listo);
        assert!(get(&v, "importar").detail.contains("1.200"));
    }
}
