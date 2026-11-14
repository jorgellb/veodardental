//! Pantallas de VERI*FACTU: configuración y envíos a la AEAT (pestaña de
//! Facturas), código QR de cada factura y envío automático en segundo plano.

use crate::app::{App, Dialog};
use crate::model::*;
use crate::theme::*;
use crate::verifactu::{self, AeatStatus, Env, SendSummary};
use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Ui, vec2};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// Certificado de la clínica en ESTE ordenador (no se guarda con los datos compartidos).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct LocalCert {
    pub path: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub remember: bool,
}

fn cert_file() -> std::path::PathBuf {
    crate::paths::config().join(crate::brand::DIR).join("aeat.json")
}

fn load_cert() -> LocalCert {
    if cfg!(test) {
        return LocalCert::default();
    }
    std::fs::read_to_string(cert_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_cert(c: &LocalCert) {
    if cfg!(test) {
        return; // las pruebas no tocan la configuración del ordenador
    }
    let mut c = c.clone();
    if !c.remember {
        c.password.clear();
    }
    let f = cert_file();
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&f, serde_json::to_string_pretty(&c).unwrap_or_default());
}

type JobResult = (SendSummary, Vec<(verifactu::Key, AeatStatus)>);

#[derive(Default)]
pub struct VfUi {
    pub cert: Option<LocalCert>,
    pub job: Option<Receiver<JobResult>>,
    pub next_allowed: Option<Instant>,
    pub last: Option<SendSummary>,
    pub check: Option<Result<String, String>>,
    pub show_password: bool,
    /// Factura con el formulario de corrección abierto, y sus datos.
    pub fix: Option<u32>,
    pub fix_name: String,
    pub fix_nif: String,
    /// Factura con el formulario de anulación abierto, y el motivo.
    pub cancel: Option<u32>,
    pub cancel_reason: String,
}

impl VfUi {
    fn cert(&mut self) -> &mut LocalCert {
        self.cert.get_or_insert_with(load_cert)
    }

    /// ¿Se ha elegido en este ordenador el certificado de la clínica?
    pub fn has_cert(&mut self) -> bool {
        !self.cert().path.trim().is_empty()
    }
}

/// Lanza el envío de los pendientes en segundo plano.
fn start_send(app: &mut App, ctx: &egui::Context) {
    let cert = app.vf.cert().clone();
    let mut db = app.db.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    let use_tls = db.verifactu.endpoint.trim().is_empty() || db.verifactu.endpoint.starts_with("https");
    std::thread::spawn(move || {
        let id = if use_tls {
            match std::fs::read(cert.path.trim().trim_matches('"')).map_err(|e| format!("No se puede leer el certificado: {e}")).and_then(|b| verifactu::identity(&b, &cert.password)) {
                Ok(i) => Some(i),
                Err(e) => {
                    let _ = tx.send((SendSummary { error: e, wait_secs: 60, ..Default::default() }, vec![]));
                    ctx.request_repaint();
                    return;
                }
            }
        } else {
            None
        };
        let statuses = |db: &Db| -> Vec<(verifactu::Key, AeatStatus)> { verifactu::chain(db).iter().map(|i| (i.key(), i.aeat().clone())).collect() };
        let before = statuses(&db);
        let s = verifactu::send_pending(&mut db, id.as_ref());
        let changed: Vec<(verifactu::Key, AeatStatus)> = statuses(&db).into_iter().filter(|(k, st)| before.iter().find(|b| b.0 == *k).map(|b| &b.1 != st).unwrap_or(false)).collect();
        let _ = tx.send((s, changed));
        ctx.request_repaint();
    });
    app.vf.job = Some(rx);
}

fn pending_count(app: &App) -> usize {
    if !app.db.verifactu.active() {
        return 0;
    }
    verifactu::pending(&app.db, &app.db.verifactu).len()
}

/// Se llama en cada fotograma: recoge los envíos terminados y, con el envío
/// automático activado, remite los pendientes respetando la espera que pide la AEAT.
pub fn tick(app: &mut App, ctx: &egui::Context) {
    let done = app.vf.job.as_ref().and_then(|rx| rx.try_recv().ok());
    if let Some((s, changed)) = done {
        app.vf.job = None;
        for (k, st) in changed {
            if let Some(s) = verifactu::status_mut(&mut app.db, k) {
                *s = st;
            }
        }
        app.vf.next_allowed = Some(Instant::now() + Duration::from_secs(s.wait_secs.max(if s.error.is_empty() { 0 } else { 120 })));
        if s.sent > 0 {
            app.changed();
            if s.bad > 0 {
                app.warn(format!("VERI*FACTU: {} registros enviados, {} rechazados por la AEAT. Revíselos en Facturas › VERI*FACTU.", s.sent, s.bad));
            } else {
                app.notify(format!("VERI*FACTU: {} registros enviados a la AEAT.", s.sent));
            }
        } else if !s.error.is_empty() {
            app.dirty = true;
        }
        app.vf.last = Some(s);
    }
    let cfg = &app.db.verifactu;
    if !cfg.active() || !cfg.auto_send || app.vf.job.is_some() || app.user_id.is_none() || app.screenshot.is_some() {
        return;
    }
    if app.vf.next_allowed.map(|t| Instant::now() < t).unwrap_or(false) {
        return;
    }
    // un envío rechazado entero no se repite solo: daría el mismo error
    if verifactu::needs_review(&app.db).is_some() {
        return;
    }
    let c = app.vf.cert().clone();
    if c.path.trim().is_empty() || c.password.is_empty() {
        return;
    }
    // un solo ordenador envía a la AEAT (si enviaran dos, mandarían lo mismo a la vez)
    let me = crate::shared::machine_name();
    if !app.db.verifactu.may_send(&me) {
        return;
    }
    if pending_count(app) > 0 {
        if app.db.verifactu.sender.trim().is_empty() {
            app.db.verifactu.sender = me;
            app.changed();
        }
        start_send(app, ctx);
    }
    ctx.request_repaint_after(Duration::from_secs(30));
}

// ───────────────────────────── QR ─────────────────────────────

/// Dibuja un código QR (con su margen blanco) de `size` puntos de lado.
pub fn draw_qr(ui: &mut Ui, text: &str, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(6), Color32::WHITE);
    if let Some((w, bits)) = verifactu::qr_matrix(text) {
        let n = (w + 4) as f32;
        let m = size / n;
        for (i, dark) in bits.iter().enumerate() {
            if *dark {
                let x = rect.min.x + ((i % w) as f32 + 2.0) * m;
                let y = rect.min.y + ((i / w) as f32 + 2.0) * m;
                p.rect_filled(egui::Rect::from_min_size(egui::pos2(x, y), vec2(m + 0.3, m + 0.3)), 0.0, Color32::BLACK);
            }
        }
    }
    p.rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
    resp
}

fn state_color(st: &AeatStatus) -> Color32 {
    match st.state.as_str() {
        "Correcto" => OK,
        "AceptadoConErrores" => WARN,
        "Incorrecto" => DANGER,
        "NoAplica" => MUTED,
        _ => INFO,
    }
}

/// Recuadro VERI*FACTU del diálogo de una factura: QR, leyenda y estado del envío.
pub fn invoice_box(app: &mut App, ui: &mut Ui, f: &Invoice) {
    let enabled = app.db.verifactu.active();
    egui::Frame::new().fill(ROW_ALT).stroke(Stroke::new(1.0, HAIRLINE)).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::symmetric(18, 14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            if enabled {
                let url = verifactu::qr_url(&app.db.verifactu, f, &app.db.config.nif);
                draw_qr(ui, &url, 112.0).on_hover_text(&url);
                ui.add_space(8.0);
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Registro de facturación").font(sbf(14.5)).color(HEADING));
                    pill(ui, "VERI*FACTU", ACCENT);
                    pill(ui, &format!("Tipo {}", f.kind()), INFO);
                });
                if enabled {
                    ui.label(RichText::new("Factura verificable en la sede electrónica de la AEAT").font(mdf(13.0)).color(TEXT));
                    // manda el último registro de la factura (alta, subsanación o anulación)
                    let (what, st, created) = match verifactu::latest(&app.db, f.id) {
                        Some(verifactu::Item::Rec(r)) => (Some(r.label()), r.aeat.clone(), r.created),
                        _ => (None, f.aeat.clone(), f.created),
                    };
                    let before = app.db.verifactu.since.map(|s| created < s).unwrap_or(false);
                    ui.horizontal_wrapped(|ui| {
                        if verifactu::cancelled(&app.db, f.id) {
                            pill(ui, "Factura anulada", DANGER);
                        }
                        if let Some(w) = what {
                            pill(ui, w, INFO);
                        }
                        if before && st.state.is_empty() {
                            pill(ui, "Emitida antes de activar VERI*FACTU", MUTED);
                        } else {
                            pill(ui, st.label(), state_color(&st));
                        }
                        if !st.csv.is_empty() {
                            muted(ui, format!("CSV {}", st.csv));
                        }
                        if let Some(t) = st.sent_at {
                            muted(ui, format!("enviado el {}", t.format("%d/%m/%Y %H:%M")));
                        }
                    });
                    if !st.message.is_empty() && !st.sent_ok() {
                        ui.label(RichText::new(format!("{}{}", if st.code.is_empty() { String::new() } else { format!("Error {}: ", st.code) }, st.message)).font(mdf(12.5)).color(DANGER));
                    } else if st.state == "AceptadoConErrores" && !st.message.is_empty() {
                        ui.label(RichText::new(format!("Aviso {}: {}", st.code, st.message)).font(mdf(12.5)).color(WARN));
                    }
                }
                ui.label(RichText::new(format!("Huella: {}", f.hash)).monospace().size(12.0));
                ui.label(RichText::new(format!("Huella anterior: {}", if f.prev_hash.is_empty() { "(primer registro)".into() } else { f.prev_hash.clone() })).monospace().size(12.0).color(MUTED));
                if !enabled {
                    muted(ui, "La huella encadena cada factura con la anterior: cualquier cambio posterior se detecta con «Verificar integridad». Active el envío a la AEAT en Facturas › VERI*FACTU.");
                }
            });
        });
        if enabled {
            record_actions(app, ui, f);
        }
    });
}

/// Corregir y reenviar el registro de una factura, o anularla si se expidió por error.
fn record_actions(app: &mut App, ui: &mut Ui, f: &Invoice) {
    if verifactu::cancelled(&app.db, f.id) {
        return;
    }
    let st = match verifactu::latest(&app.db, f.id) {
        Some(it) => it.aeat().clone(),
        None => return,
    };
    let before = app.db.verifactu.since.map(|s| f.created < s).unwrap_or(false);
    let can_fix = matches!(st.state.as_str(), "AceptadoConErrores" | "Incorrecto");
    let can_cancel = f.rectified_by.is_none() && (!st.pending() || before);
    if !can_fix && !can_cancel {
        return;
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if can_fix && ui.button("Corregir datos y reenviar").on_hover_text("Genera un nuevo registro encadenado con los datos corregidos del destinatario").clicked() {
            let (name, nif) = app.db.patient(f.patient_id).map(|p| (p.full_name(), p.nif.clone())).unwrap_or((f.client_name.clone(), f.client_nif.clone()));
            app.vf.fix = Some(f.id);
            app.vf.fix_name = name;
            app.vf.fix_nif = nif;
            app.vf.cancel = None;
        }
        if can_cancel && ui.button("Anular (factura expedida por error)").on_hover_text("Registro de anulación: la factura deja de tener efecto y sus tratamientos se pueden volver a facturar").clicked() {
            app.vf.cancel = Some(f.id);
            app.vf.cancel_reason.clear();
            app.vf.fix = None;
        }
    });
    if app.vf.fix == Some(f.id) {
        egui::Frame::new().fill(tint(INFO, 0.05)).stroke(Stroke::new(1.0, tint(INFO, 0.25))).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            muted(
                ui,
                if st.state == "Incorrecto" {
                    "La AEAT rechazó el registro. Corrija los datos y se enviará de nuevo indicando el rechazo previo."
                } else {
                    "La AEAT aceptó el registro con errores. Corrija los datos y se enviará una subsanación."
                },
            );
            ui.horizontal(|ui| {
                field(ui, "Nombre del cliente", &mut app.vf.fix_name, 280.0);
                field(ui, "NIF", &mut app.vf.fix_nif, 130.0);
            });
            ui.horizontal(|ui| {
                if primary(ui, "Generar registro y reenviar").clicked() {
                    let (name, nif) = (app.vf.fix_name.trim().to_string(), app.vf.fix_nif.trim().to_uppercase());
                    if let Some(inv) = app.db.invoices.iter_mut().find(|i| i.id == f.id) {
                        inv.client_name = name;
                        inv.client_nif = nif;
                    }
                    match app.chain_op(crate::app::ChainOp::Correct { invoice: f.id }) {
                        Ok(_) => {
                            app.db.audit("VERI*FACTU: corrección", &f.code());
                            app.vf.fix = None;
                            app.changed();
                            app.notify("Registro de corrección generado: se enviará a la AEAT con los pendientes.");
                        }
                        Err(e) => app.warn(e),
                    }
                }
                if ui.button("Cancelar").clicked() {
                    app.vf.fix = None;
                }
            });
        });
    }
    if app.vf.cancel == Some(f.id) {
        egui::Frame::new().fill(tint(DANGER, 0.05)).stroke(Stroke::new(1.0, tint(DANGER, 0.25))).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            muted(ui, "Use la anulación sólo si la factura no debió expedirse (duplicada, a otro paciente…). Para cambiar importes o conceptos, emita una rectificativa.");
            ui.horizontal(|ui| {
                ui.add_sized([340.0, 36.0], egui::TextEdit::singleline(&mut app.vf.cancel_reason).hint_text("Motivo de la anulación"));
                if danger(ui, "Anular la factura").clicked() {
                    let reason = app.vf.cancel_reason.clone();
                    match app.chain_op(crate::app::ChainOp::Cancel { invoice: f.id, reason: reason.clone() }) {
                        Ok(_) => {
                            app.db.audit("VERI*FACTU: anulación", &format!("{}: {reason}", f.code()));
                            app.vf.cancel = None;
                            app.changed();
                            app.notify("Factura anulada: el registro de anulación se enviará a la AEAT. Sus tratamientos se pueden volver a facturar.");
                        }
                        Err(e) => app.warn(e),
                    }
                }
                if ui.button("Cancelar").clicked() {
                    app.vf.cancel = None;
                }
            });
        });
    }
}

/// Bloque HTML con el QR y la leyenda para la factura impresa (vacío si VERI*FACTU no está activo).
pub fn print_block(db: &Db, f: &Invoice) -> String {
    if !db.verifactu.active() {
        return String::new();
    }
    let url = verifactu::qr_url(&db.verifactu, f, &db.config.nif);
    format!("<div style='display:flex;align-items:center;gap:14px;margin:6px 0 14px 0'>{}<div><b>VERI*FACTU</b><br><span style='font-size:13px'>Factura verificable en la sede electrónica de la AEAT</span></div></div>", verifactu::qr_svg(&url, 35.0))
}

// ───────────────────────────── pestaña de Facturas ─────────────────────────────

pub fn panel(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    title(ui, "VERI*FACTU", "Remisión de los registros de facturación a la Agencia Tributaria (Real Decreto 1007/2023)");
    let cfg = app.db.verifactu.clone();
    let count = |st: &str| verifactu::chain(&app.db).iter().filter(|i| i.aeat().state == st).count();
    let pend = pending_count(app);
    let w = kpi_w(ui, 4);
    ui.horizontal(|ui| {
        kpi(ui, w, "Pendientes de enviar", &pend.to_string(), if cfg.active() { "se envían en orden" } else { "VERI*FACTU desactivado" }, INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Registradas en la AEAT", &count("Correcto").to_string(), "con su CSV", OK);
        ui.add_space(4.0);
        kpi(ui, w, "Aceptadas con errores", &count("AceptadoConErrores").to_string(), "revisar el aviso", WARN);
        ui.add_space(4.0);
        kpi(ui, w, "Rechazadas", &count("Incorrecto").to_string(), "corregir con rectificativa", DANGER);
    });
    ui.add_space(8.0);
    settings_card(app, ui, &ctx);
    ui.add_space(8.0);
    queue_card(app, ui, &ctx);
}

/// Territorio fiscal: común (VERI*FACTU) o foral. Solo cambia algo con la
/// confirmación de la gestoría (ver `foral`).
pub fn foral_card(app: &mut App, ui: &mut Ui) {
    let before = app.db.verifactu.foral.clone();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Territorio fiscal");
        let f = &mut app.db.verifactu.foral;
        let label = f.territory.map(|t| t.label()).unwrap_or("Territorio común (VERI*FACTU)");
        crate::theme::combo("foral_territory").width(320.0).selected_text(label).show_ui(ui, |ui| {
            if ui.selectable_label(f.territory.is_none(), "Territorio común (VERI*FACTU)").clicked() {
                *f = Default::default();
            }
            for t in crate::foral::Territory::ALL {
                if ui.selectable_label(f.territory == Some(t), t.label()).clicked() && f.territory != Some(t) {
                    f.territory = Some(t);
                    f.confirmed = None;
                }
            }
        });
        match (f.territory, &f.confirmed) {
            (None, _) => muted(ui, "Si la clínica tributa en Álava, Bizkaia, Gipuzkoa o Navarra, su gestoría lo confirma y se elige aquí: allí no aplica VERI*FACTU, sino el sistema de cada hacienda foral."),
            (Some(_), None) => {
                ui.label(RichText::new("Sin confirmar, no cambia nada: VERI*FACTU sigue como está.").color(WARN));
                ui.horizontal(|ui| {
                    let key = ui.scope_id().with("foral_who");
                    let mut who: String = ui.data_mut(|d| d.get_temp(key)).unwrap_or_default();
                    ui.add(egui::TextEdit::singleline(&mut who).hint_text("Gestoría que lo confirma").desired_width(240.0));
                    if ui.add_enabled(!who.trim().is_empty(), egui::Button::new("Lo confirma la gestoría")).clicked() {
                        f.confirmed = Some((crate::model::today(), who.trim().to_string()));
                        who.clear();
                    }
                    ui.data_mut(|d| d.insert_temp(key, who));
                });
            }
            (Some(t), Some((when, who))) => {
                ui.label(RichText::new(format!("{}: confirmado por {who} el {}.", t.label(), crate::model::fmt_date(*when))).font(sbf(14.0)));
                ui.label(RichText::new(crate::foral::pending_text(t)).color(DANGER));
            }
        }
    });
    if app.db.verifactu.foral != before {
        app.db.audit("Territorio fiscal", &format!("{:?} → {:?}", before.territory, app.db.verifactu.foral.territory));
        app.changed();
    }
}

/// Configuración de VERI*FACTU y del certificado (también en Configuración › Facturación).
pub fn settings(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    foral_card(app, ui);
    ui.add_space(8.0);
    settings_card(app, ui, &ctx);
}

fn settings_card(app: &mut App, ui: &mut Ui, ctx: &egui::Context) {
    let before = serde_json::to_string(&app.db.verifactu).unwrap_or_default();
    let cert_before = app.vf.cert().clone();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Configuración");
        let was = app.db.verifactu.enabled;
        let cfg = &mut app.db.verifactu;
        ui.checkbox(&mut cfg.enabled, RichText::new("Enviar los registros de facturación a la AEAT (VERI*FACTU)").font(sbf(14.0)));
        if cfg.enabled && !was {
            if cfg.since.is_none() {
                cfg.since = Some(now());
            }
            if cfg.installation.is_empty() {
                cfg.installation = format!("VD-{}", now().format("%Y%m%d%H%M%S"));
            }
        }
        muted(ui, "Cada factura que se emita se envía a la AEAT con su huella encadenada y lleva impreso el código QR con la leyenda «VERI*FACTU». Las facturas emitidas antes de activarlo no se envían.");
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Entorno").font(mdf(13.0)).color(MUTED));
            for e in [Env::Pruebas, Env::Produccion] {
                if ui.selectable_label(cfg.env == e, e.label()).clicked() {
                    cfg.env = e;
                }
            }
            ui.add_space(12.0);
            ui.label(RichText::new("Certificado").font(mdf(13.0)).color(MUTED));
            ui.radio_value(&mut cfg.seal, false, "De la persona o representante");
            ui.radio_value(&mut cfg.seal, true, "Sello electrónico de la empresa");
        });
        if cfg.env == Env::Produccion {
            ui.label(RichText::new("En producción los envíos tienen efectos fiscales. Use antes el entorno de pruebas.").font(mdf(12.8)).color(WARN));
        }
        if let Some(s) = cfg.since {
            ui.horizontal_wrapped(|ui| {
                muted(ui, format!("Se envían las facturas generadas desde el {}.", s.format("%d/%m/%Y %H:%M")));
                if ui.small_button("Enviar también las anteriores").on_hover_text("Incluye en el envío todas las facturas de la cadena").clicked() {
                    cfg.since = None;
                }
            });
        }
        ui.checkbox(&mut cfg.auto_send, "Enviar automáticamente en segundo plano (cada vez que se emite una factura)");
        let me = crate::shared::machine_name();
        ui.horizontal_wrapped(|ui| {
            if cfg.may_send(&me) {
                ui.label(RichText::new("Envía a la AEAT este ordenador (con el certificado de la clínica).").size(13.0).color(TEXT));
            } else {
                ui.label(RichText::new(format!("Envía a la AEAT el ordenador «{}»: los demás no envían, para no mandar los mismos registros dos veces.", cfg.sender)).size(13.0).color(TEXT));
                if ui.button("Enviar desde este ordenador").on_hover_text("Este ordenador necesita el certificado de la clínica").clicked() {
                    cfg.sender = me.clone();
                }
            }
        });
        ui.add_space(6.0);
        ui.collapsing("Datos del programa (productor del sistema informático)", |ui| {
            muted(ui, "Si se deja vacío, figura la propia clínica como productora del programa.");
            ui.horizontal(|ui| {
                field(ui, "Nombre o razón social", &mut cfg.producer_name, 280.0);
                field(ui, "NIF", &mut cfg.producer_nif, 130.0);
            });
            muted(ui, format!("Programa: VeodarDental {} · instalación {}", env!("CARGO_PKG_VERSION"), if cfg.installation.is_empty() { "—" } else { &cfg.installation }));
        });
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Certificado electrónico de la clínica (en este ordenador)");
        muted(ui, "Se usa para identificarse ante la AEAT al enviar. Es el certificado del titular, de su representante o el sello de la empresa, en un fichero .p12 o .pfx exportado con su clave privada.");
        let show = app.vf.show_password;
        let c = app.vf.cert();
        ui.horizontal_wrapped(|ui| {
            field(ui, "Fichero del certificado (.p12 / .pfx)", &mut c.path, 340.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Contraseña").font(mdf(12.8)).color(MUTED));
                ui.add_sized([180.0, 36.0], egui::TextEdit::singleline(&mut c.password).password(!show));
            });
        });
        let mut toggle = false;
        let mut check = false;
        ui.horizontal(|ui| {
            ui.checkbox(&mut c.remember, "Recordar la contraseña en este ordenador");
            if ui.small_button(if show { "Ocultar" } else { "Ver" }).clicked() {
                toggle = true;
            }
            if ui.button("Comprobar certificado").clicked() {
                check = true;
            }
        });
        if toggle {
            app.vf.show_password = !show;
        }
        if check {
            let c = app.vf.cert().clone();
            app.vf.check = Some(std::fs::read(c.path.trim().trim_matches('"')).map_err(|e| format!("No se puede leer el fichero: {e}")).and_then(|b| verifactu::identity(&b, &c.password)).map(|i| format!("Certificado válido: {}", i.subject)));
        }
        match &app.vf.check {
            Some(Ok(m)) => {
                ui.label(RichText::new(format!("✔ {m}")).color(OK));
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("✖ {e}")).color(DANGER));
            }
            None => {}
        }
    });
    if serde_json::to_string(&app.db.verifactu).unwrap_or_default() != before {
        app.dirty = true;
    }
    let cert_now = app.vf.cert().clone();
    if cert_now != cert_before {
        save_cert(&cert_now);
    }
    let _ = ctx;
}

fn queue_card(app: &mut App, ui: &mut Ui, ctx: &egui::Context) {
    let pend = pending_count(app);
    let mut open = None;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        if let Some(m) = verifactu::needs_review(&app.db) {
            ui.label(RichText::new(format!("{m}. El envío automático está parado: corrija la causa y pulse «Enviar ahora».")).color(DANGER));
        }
        ui.horizontal(|ui| {
            section(ui, "Envíos");
            right_aligned(ui, |ui| {
                let waiting = app.vf.next_allowed.map(|t| t.saturating_duration_since(Instant::now()).as_secs()).filter(|s| *s > 0);
                let busy = app.vf.job.is_some();
                let mine = app.db.verifactu.may_send(&crate::shared::machine_name());
                let enabled = app.db.verifactu.active() && pend > 0 && !busy && waiting.is_none() && mine;
                let why = if mine { "No hay nada pendiente de enviar ahora.".to_string() } else { format!("Envía a la AEAT el ordenador «{}» (se cambia en la configuración de VERI*FACTU).", app.db.verifactu.sender) };
                if ui.add_enabled(enabled, egui::Button::new(RichText::new(format!("Enviar ahora ({pend})")).color(Color32::WHITE)).fill(ACCENT)).on_disabled_hover_text(why).clicked() {
                    let c = app.vf.cert().clone();
                    let local = !app.db.verifactu.endpoint.trim().is_empty() && !app.db.verifactu.endpoint.starts_with("https");
                    if !local && (c.path.trim().is_empty() || c.password.is_empty()) {
                        app.warn("Indique el certificado electrónico de la clínica y su contraseña.");
                    } else {
                        if app.db.verifactu.sender.trim().is_empty() {
                            app.db.verifactu.sender = crate::shared::machine_name();
                            app.changed();
                        }
                        start_send(app, ctx);
                    }
                }
                if busy {
                    ui.spinner();
                    muted(ui, "Enviando a la AEAT…");
                    ctx.request_repaint_after(Duration::from_millis(500));
                } else if let Some(s) = waiting {
                    muted(ui, format!("La AEAT pide esperar {s} s entre envíos"));
                    ctx.request_repaint_after(Duration::from_secs(1));
                }
            });
        });
        if let Some(s) = &app.vf.last {
            if !s.error.is_empty() {
                ui.label(RichText::new(format!("Último envío: {}", s.error)).color(DANGER));
            } else if s.sent > 0 {
                ui.label(
                    RichText::new(format!("Último envío: {} registros · {} correctos · {} con errores · {} rechazados{}", s.sent, s.ok, s.warn, s.bad, if s.csv.is_empty() { String::new() } else { format!(" · CSV {}", s.csv) })).color(if s.bad > 0 {
                        WARN
                    } else {
                        OK
                    }),
                );
            }
        }
        if let Some(t) = crate::foral::foral_territory(&app.db.verifactu.foral) {
            muted(ui, format!("Territorio foral confirmado ({}): VERI*FACTU no aplica y no se envía nada a la AEAT.", t.label()));
        } else if !app.db.verifactu.enabled {
            muted(ui, "VERI*FACTU está desactivado: las facturas se encadenan con su huella pero no se envían.");
        }
        ui.add_space(4.0);
        let tbl = Table::new("vf", vec![("Factura", 120.0), ("Fecha", 90.0), ("Registro", 110.0), ("Cliente", 200.0), ("Total", 90.0), ("Estado", 170.0), ("CSV / aviso", 300.0)]);
        tbl.header(ui);
        let since = app.db.verifactu.since;
        let enabled = app.db.verifactu.active();
        // toda la cadena: altas de facturas, subsanaciones y anulaciones (lo último arriba)
        let rows: Vec<(u32, [(String, Color32); 7])> = verifactu::chain(&app.db)
            .into_iter()
            .rev()
            .take(200)
            .filter_map(|it| {
                let f = app.db.invoice(it.invoice_id())?;
                let st = it.aeat();
                let before = since.map(|s| it.created() < s).unwrap_or(false) && st.state.is_empty();
                let (label, col) = if !enabled && st.state.is_empty() {
                    ("Sin enviar", MUTED)
                } else if before {
                    ("Anterior a VERI*FACTU", MUTED)
                } else {
                    (st.label(), state_color(st))
                };
                let note = if !st.csv.is_empty() && st.sent_ok() {
                    st.csv.clone()
                } else if !st.message.is_empty() {
                    format!("{} {}", st.code, st.message).trim().to_string()
                } else {
                    String::new()
                };
                let kind = match it {
                    verifactu::Item::Rec(r) => r.label().to_string(),
                    verifactu::Item::Inv(_) => f.kind().to_string(),
                };
                Some((
                    f.id,
                    [
                        cell(f.code()),
                        cell(fmt_date(f.date)),
                        cellc(kind, if matches!(it, verifactu::Item::Rec(_)) { INFO } else { TEXT }),
                        cell(&f.client_name),
                        cell(money(f.total())),
                        cellc(label, col),
                        cellc(note, if st.sent_ok() { MUTED } else { DANGER }),
                    ],
                ))
            })
            .collect();
        if rows.is_empty() {
            empty_state(ui, "Todavía no hay facturas.");
        }
        for (i, (id, cells)) in rows.iter().enumerate() {
            if tbl.row(ui, i, false, cells).clicked() {
                open = Some(*id);
            }
        }
        ui.add_space(6.0);
        muted(ui, "Si la AEAT rechaza un registro o lo acepta con errores (por ejemplo, por un NIF que no existe), abra la factura y use «Corregir datos y reenviar». Si la factura no debió expedirse, «Anular».");
    });
    if let Some(id) = open {
        app.dialog = Dialog::Invoice(id);
    }
}
