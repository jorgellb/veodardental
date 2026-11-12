//! Pacientes: listado, ficha con pestañas (resumen, filiación, odontograma,
//! historial médico, presupuestos, cuenta, citas) y alta de paciente.

use crate::app::{App, Dialog, PatTab};
use crate::model::*;
use crate::screens::{agenda, cobros, odontograma};
use crate::theme::*;
use chrono::NaiveDate;
use egui::{Align2, Color32, CornerRadius, FontId, RichText, Sense, Ui, vec2};

pub fn search(db: &Db, q: &str, limit: usize) -> Vec<u32> {
    let q = fold(q.trim());
    if q.is_empty() {
        return db.patients.iter().take(limit).map(|p| p.id).collect();
    }
    let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
    db.patients
        .iter()
        .filter(|p| {
            let name = fold(&format!("{} {} {}", p.first_name, p.last_name, p.first_name));
            q.split_whitespace().all(|w| name.contains(w))
                || p.history_no.to_string() == q
                || fold(&p.nif).contains(&q)
                || (digits.len() >= 4
                    && (p.mobile.replace(' ', "").contains(&digits) || p.phone.replace(' ', "").contains(&digits)))
        })
        .take(limit)
        .map(|p| p.id)
        .collect()
}

pub fn blank_patient(db: &Db, today: NaiveDate) -> Patient {
    Patient {
        id: 0,
        history_no: db.next_history_no(),
        province: "Madrid".into(),
        tariff_id: 1,
        doctor_id: 1,
        registered: Some(today),
        accepts_sms: true,
        active: true,
        ..Default::default()
    }
}

/// Valida un NIF/NIE español. Devuelve None si es correcto o vacío.
pub fn nif_problem(nif: &str) -> Option<&'static str> {
    let s: String = nif.trim().to_uppercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if s.is_empty() {
        return None;
    }
    if s.len() != 9 {
        return Some("El NIF debe tener 9 caracteres");
    }
    let letters = "TRWAGMYFPDXBNJZSQVHLCKE";
    let first = s.chars().next().unwrap();
    let num_part: String = match first {
        'X' => format!("0{}", &s[1..8]),
        'Y' => format!("1{}", &s[1..8]),
        'Z' => format!("2{}", &s[1..8]),
        _ => s[..8].to_string(),
    };
    let Ok(n) = num_part.parse::<u32>() else { return Some("Formato de NIF no válido") };
    let expected = letters.chars().nth((n % 23) as usize).unwrap();
    if !s.ends_with(expected) {
        return Some("La letra del NIF no corresponde");
    }
    None
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let pid = match app.patient {
        Some(p) if !app.lists.patient_list_open && app.db.patient(p).is_some() => p,
        _ => {
            list(app, ui);
            return;
        }
    };
    if !tab_allowed(app, app.pat_tab) {
        app.pat_tab = PatTab::Resumen;
    }
    header(app, ui, pid);
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let tabs: Vec<(PatTab, &str)> = PatTab::ALL.iter().copied().filter(|(tab, _)| tab_allowed(app, *tab)).collect();
        // si las pestañas no caben (ventana estrecha) se aprietan: letra y márgenes algo menores
        let natural: f32 = tabs.iter().map(|(_, n)| ui.fonts_mut(|f| f.layout_no_wrap(n.to_string(), FontId::proportional(14.5), TEXT).size().x) + 24.0).sum();
        let (fs, pad) = if natural <= ui.available_width() { (14.5, 22.0) } else { (13.2, 12.0) };
        for (tab, name) in tabs {
            let sel = app.pat_tab == tab;
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.fonts_mut(|f| f.layout_no_wrap(name.into(), FontId::proportional(fs), TEXT).size().x) + pad, 42.0), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Tab, true, name));
            if sel {
                ui.painter().rect_filled(rect, CornerRadius::same(8), CARD);
                ui.painter().rect_filled(egui::Rect::from_min_size(rect.left_bottom() - vec2(0.0, 3.0), vec2(rect.width(), 3.0)), CornerRadius::same(2), ACCENT);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::same(8), Color32::from_rgb(232, 238, 244));
            }
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, name, if sel { crate::theme::sbf(fs) } else { FontId::proportional(fs) }, if sel { TEXT } else { MUTED });
            if resp.clicked() {
                app.pat_tab = tab;
            }
        }
    });
    ui.add_space(8.0);
    let mut sa = egui::ScrollArea::vertical().id_salt("pat_scroll").auto_shrink([false, false]);
    if let Some(y) = app.lists.scroll_to.take() {
        sa = sa.vertical_scroll_offset(y);
    }
    sa.show(ui, |ui| match app.pat_tab {
        PatTab::Resumen => resumen(app, ui, pid),
        PatTab::Filiacion => filiacion(app, ui, pid),
        PatTab::Salud => crate::screens::salud::patient_tab(app, ui, pid),
        PatTab::Odontograma => odontograma::show(app, ui, pid),
        PatTab::Periodontograma => crate::screens::perio::show(app, ui, pid),
        PatTab::Imagenes => crate::screens::imagenes::patient_tab(app, ui, pid),
        PatTab::Documentos => {
            // las recetas son datos clínicos (Recepción no las recibe)
            let clinical = app.can(crate::users::Perm::Clinico);
            if !clinical {
                app.docs_mode = 0;
            }
            ui.horizontal(|ui| {
                if ui.selectable_label(app.docs_mode == 0, "Documentos y consentimientos").clicked() {
                    app.docs_mode = 0;
                }
                if clinical && ui.selectable_label(app.docs_mode == 1, "Recetas").clicked() {
                    app.docs_mode = 1;
                }
            });
            ui.add_space(6.0);
            if app.docs_mode == 0 {
                crate::screens::documentos::patient_tab(app, ui, pid)
            } else {
                crate::screens::recetas::patient_view(app, ui, pid)
            }
        }
        PatTab::Historial => historial(app, ui, pid),
        PatTab::Presupuestos => crate::screens::presupuestos::patient_list(app, ui, pid),
        PatTab::Cuenta => cuenta(app, ui, pid),
        PatTab::Citas => citas(app, ui, pid),
    });
}

fn avatar(ui: &mut Ui, p: &Patient, size: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let col = if p.sex == "F" { Color32::from_rgb(214, 96, 140) } else { Color32::from_rgb(60, 130, 210) };
    ui.painter().circle_filled(r.center(), size / 2.0, tint(col, 0.2));
    let ini: String = [p.first_name.chars().next(), p.last_name.chars().next()].iter().flatten().collect();
    ui.painter().text(r.center(), Align2::CENTER_CENTER, ini, FontId::proportional(size * 0.4), col);
}

/// ¿Puede ver el usuario esta pestaña de la ficha? Cuenta y presupuestos,
/// con el permiso de economía; salud, periodontograma e imágenes, con el
/// clínico (Recepción no recibe esos datos: ver [`crate::vista`]).
fn tab_allowed(app: &App, tab: PatTab) -> bool {
    use crate::users::Perm;
    match tab {
        PatTab::Cuenta | PatTab::Presupuestos => app.can(Perm::Economia),
        PatTab::Salud | PatTab::Periodontograma | PatTab::Imagenes => app.can(Perm::Clinico),
        _ => true,
    }
}

/// Botones de la cabecera de la ficha (lista, ficha, RX, cita, cobro).
fn header_buttons(app: &mut App, ui: &mut Ui, pid: u32) {
    if ui.button("☰ Lista").on_hover_text("Volver a la lista de pacientes").clicked() {
        app.lists.patient_list_open = true;
    }
    // historia clínica e imágenes: solo con el permiso clínico
    if app.can(crate::users::Perm::Clinico) {
        clinical_buttons(app, ui, pid);
    }
    if ui.button("📅 Nueva cita").clicked() {
        let d = agenda::ApptDraft::new_at(app, app.today, None, Some(pid));
        app.dialog = Dialog::Appointment(d);
    }
    if app.can(crate::users::Perm::Economia) && primary(ui, "💶 Cobrar").clicked() {
        app.dialog = Dialog::Payment(cobros::PayDraft::new(app, pid));
    }
}

/// Ficha clínica (imprimir, exportar) y programas de radiología.
fn clinical_buttons(app: &mut App, ui: &mut Ui, pid: u32) {
    crate::screens::fichas::patient_menu(app, ui, pid);
    ui.menu_button("📷 RX / TAC", |ui| {
        let list: Vec<(u32, String)> = app.db.connectors.iter().filter(|c| c.enabled).map(|c| (c.id, c.name.clone())).collect();
        for (id, n) in list {
            if ui.button(n).clicked() {
                crate::screens::imagenes::launch(app, id, pid);
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Ver galería de imágenes").clicked() {
            app.pat_tab = PatTab::Imagenes;
            ui.close();
        }
    });
}

/// Saldo del paciente como etiqueta de color.
fn balance_badge(ui: &mut Ui, bal: f64) {
    let (txt, col) = if bal > 0.005 {
        (format!("Pendiente de cobro: {}", money(bal)), DANGER)
    } else if bal < -0.005 {
        (format!("Saldo a favor: {}", money(-bal)), OK)
    } else {
        ("Cuenta al día".to_string(), OK)
    };
    egui::Frame::new().fill(tint(col, 0.09)).corner_radius(egui::CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(12, 5)).show(ui, |ui| {
        ui.add(egui::Label::new(RichText::new(txt).font(crate::theme::sbf(14.0)).color(mix(col, HEADING, 0.15))).wrap_mode(egui::TextWrapMode::Extend));
    });
}

fn header(app: &mut App, ui: &mut Ui, pid: u32) {
    let p = app.db.patient(pid).cloned().unwrap();
    let bal = app.db.patient_balance(pid);
    let mut go_health = false;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let narrow = ui.available_width() < 1040.0;
        ui.horizontal(|ui| {
            avatar(ui, &p, 56.0);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(p.full_name()).size(23.5).family(crate::theme::sb()));
                    pill(ui, &format!("Nº historial {}", p.history_no), INFO);
                    if !p.active {
                        pill(ui, "Inactivo", MUTED);
                    }
                });
                let age = p.age(app.today).map(|a| format!("{a} años")).unwrap_or_default();
                muted(ui, format!(
                    "{}  ·  {}  ·  Tarifa: {}  ·  Dr.: {}  ·  📞 {}",
                    age,
                    if p.nif.is_empty() { "sin NIF".into() } else { p.nif.clone() },
                    app.db.tariff_name(p.tariff_id),
                    app.db.doctor_alias(p.doctor_id),
                    p.mobile
                ));
            });
            right_aligned(ui, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    if !narrow {
                        ui.horizontal(|ui| header_buttons(app, ui, pid));
                        ui.add_space(6.0);
                    }
                    balance_badge(ui, bal);
                });
            });
        });
        // en ventanas estrechas los botones van en su propia fila, para no tapar el nombre
        if narrow {
            ui.add_space(4.0);
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 38.0), egui::Layout::right_to_left(egui::Align::Center), |ui| header_buttons(app, ui, pid));
        }
        // avisos clínicos y administrativos, en su propia fila (no se montan sobre el saldo)
        let any = !p.alerts.is_empty() || !p.allergies.is_empty() || !p.accepts_gdpr || app.db.health_status(pid, app.today).is_some();
        if any {
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(68.0);
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                if !p.alerts.is_empty() {
                    pill(ui, &format!("⚠ {}", p.alerts), DANGER);
                }
                if !p.allergies.is_empty() {
                    pill(ui, &format!("Alergia: {}", p.allergies), DANGER);
                }
                if !p.accepts_gdpr {
                    pill(ui, "Falta firma protección de datos", WARN);
                }
                if let Some(s) = app.db.health_status(pid, app.today) {
                    if !app.can(crate::users::Perm::Clinico) {
                        // Recepción no abre los cuestionarios: sabe que falta y lo pide
                        pill(ui, s, WARN).on_hover_text("Pídale que lo rellene con el enlace de su cita, por internet, o en el gabinete.");
                    } else if pill(ui, s, WARN).on_hover_text("Abrir el cuestionario de salud").interact(egui::Sense::click()).clicked() {
                        go_health = true;
                    }
                }
            });
        }
    });
    if go_health {
        app.pat_tab = PatTab::Salud;
    }
}

fn list(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        title(ui, "Pacientes", &format!("{} pacientes en la base de datos", app.db.patients.len()));
        right_aligned(ui, |ui| {
            if ui.button("📥 Importar pacientes").on_hover_text("Importar pacientes desde Excel/CSV de GESDEN").clicked() {
                app.imp.kind = crate::extra::ImportKind::Pacientes;
                app.screen = crate::app::Screen::Config;
            }
            if primary(ui, "＋ Nuevo paciente").clicked() {
                app.dialog = Dialog::NewPatient(Box::new(blank_patient(&app.db, app.today)));
            }
        });
    });
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_sized([340.0, 28.0], egui::TextEdit::singleline(&mut app.lists.patient_filter).hint_text("Filtrar por nombre, Nº historial, NIF o teléfono"));
            ui.checkbox(&mut app.lists.patient_only_debt, "Solo con deuda pendiente");
        });
        ui.add_space(6.0);
        let tbl = Table::new(
            "pats",
            vec![("Nº", 64.0), ("Paciente", 230.0), ("NIF", 110.0), ("Móvil", 120.0), ("Tarifa", 180.0), ("Doctor", 110.0), ("Última visita", 110.0), ("Próxima cita", 130.0), ("Saldo", 100.0)],
        );
        tbl.header(ui);
        let ids = search(&app.db, &app.lists.patient_filter, usize::MAX);
        let mut open = None;
        egui::ScrollArea::vertical().id_salt("pl").auto_shrink([false, false]).show(ui, |ui| {
            let mut i = 0;
            for pid in ids {
                let bal = app.db.patient_balance(pid);
                if app.lists.patient_only_debt && bal <= 0.005 {
                    continue;
                }
                let p = app.db.patient(pid).unwrap();
                let next = app.db.next_appt(pid, app.today).map(|a| format!("{} {}", fmt_date(a.date), a.start.format("%H:%M"))).unwrap_or_default();
                let cells = vec![
                    cell(p.history_no.to_string()),
                    cell(format!("{}{}", if p.alerts.is_empty() && p.allergies.is_empty() { "" } else { "⚠ " }, p.full_name())),
                    cell(&p.nif),
                    cell(&p.mobile),
                    cell(app.db.tariff_name(p.tariff_id)),
                    cell(app.db.doctor_alias(p.doctor_id)),
                    cell(app.db.last_visit(pid).map(fmt_date).unwrap_or_default()),
                    cell(next),
                    cellc(money(bal), if bal > 0.005 { DANGER } else { MUTED }),
                ];
                if tbl.row(ui, i, app.patient == Some(pid), &cells).clicked() {
                    open = Some(pid);
                }
                i += 1;
            }
        });
        if let Some(pid) = open {
            app.open_patient(pid, PatTab::Resumen);
        }
    });
}

fn resumen(app: &mut App, ui: &mut Ui, pid: u32) {
    let p = app.db.patient(pid).cloned().unwrap();
    let w = kpi_w(ui, 3);
    ui.horizontal(|ui| {
        kpi(ui, w, "Facturado (realizado)", &money(app.db.patient_debit(pid)), "tratamientos realizados", INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Cobrado", &money(app.db.patient_paid(pid)), &format!("{} cobros", app.db.payments.iter().filter(|x| x.patient_id == pid).count()), OK);
        ui.add_space(4.0);
        kpi(ui, w, "Previsto pendiente de hacer", &money(app.db.patient_planned(pid)), "plan de tratamiento", WARN);
    });
    ui.add_space(10.0);
    ui.columns(2, |cols| {
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Datos de contacto");
            info_row(ui, "Móvil", &p.mobile);
            info_row(ui, "Teléfono", &p.phone);
            info_row(ui, "Email", &p.email);
            info_row(ui, "Dirección", &format!("{}, {} {}", p.address, p.zip, p.city));
            info_row(ui, "Póliza", if p.policy_no.is_empty() { "—" } else { &p.policy_no });
            info_row(ui, "Procedencia", &p.referral);
            info_row(ui, "Alta", &p.registered.map(fmt_date).unwrap_or_default());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let ok = |b: bool| if b { OK } else { MUTED };
                pill(ui, "SMS", ok(p.accepts_sms));
                pill(ui, "Email", ok(p.accepts_email));
                pill(ui, "WhatsApp", ok(p.accepts_whatsapp));
                pill(ui, "Protección de datos", ok(p.accepts_gdpr));
            });
        });
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Próximas citas");
            let mut future: Vec<&Appointment> = app
                .db
                .appointments
                .iter()
                .filter(|a| a.patient_id == Some(pid) && a.date >= app.today && !a.state.is_cancelled() && a.state != ApptState::Finalizada)
                .collect();
            future.sort_by_key(|a| (a.date, a.start));
            if future.is_empty() {
                muted(ui, "Sin citas programadas.");
            }
            for a in future.iter().take(4) {
                ui.horizontal(|ui| {
                    pill(ui, a.state.label(), rgb(a.state.color()));
                    ui.label(format!("{} {} · {} · {}", fmt_date(a.date), a.start.format("%H:%M"), a.reason, app.db.doctor_alias(a.doctor_id)));
                });
            }
            ui.add_space(8.0);
            section(ui, "Recalls pendientes");
            let recs: Vec<&Recall> = app.db.recalls.iter().filter(|r| r.patient_id == pid && r.state == RecallState::Pendiente).collect();
            if recs.is_empty() {
                muted(ui, "Ninguno.");
            }
            for r in recs {
                ui.label(format!("{} · {}", fmt_date(r.date), r.reason));
            }
            let calls: Vec<String> = app.db.calls_of(pid).iter().take(3).map(|c| format!("{} · {}", c.when.format("%d/%m/%Y %H:%M"), crate::phone::pretty(&c.number))).collect();
            if !calls.is_empty() {
                ui.add_space(8.0);
                section(ui, "Últimas llamadas");
                for c in calls {
                    ui.label(format!("📞 {c}"));
                }
            }
        });
    });
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Últimos tratamientos");
        let mut es: Vec<&MedEntry> = app.db.entries.iter().filter(|e| e.patient_id == pid).collect();
        es.sort_by_key(|e| std::cmp::Reverse((e.date, e.id)));
        entries_table(app, ui, &es.into_iter().take(8).cloned().collect::<Vec<_>>(), false);
    });
}

fn entries_table(app: &mut App, ui: &mut Ui, es: &[MedEntry], actions: bool) {
    let tbl = Table::new(
        "entries",
        vec![("Fecha", 96.0), ("Código", 64.0), ("Tratamiento", 260.0), ("Piezas", 110.0), ("Estado", 96.0), ("Doctor", 110.0), ("Importe", 96.0), ("Presup.", 70.0)],
    );
    tbl.header(ui);
    if es.is_empty() {
        empty_state(ui, "Sin tratamientos registrados");
    }
    let mut realize = None;
    let mut delete = None;
    for (i, e) in es.iter().enumerate() {
        let teeth: Vec<String> = e.teeth.iter().map(|t| t.to_string()).collect();
        let faces = if e.faces.is_empty() { String::new() } else { format!(" ({})", e.faces.join("")) };
        let cells = vec![
            cell(fmt_date(e.date)),
            cell(&e.code),
            cell(&e.description),
            cell(format!("{}{}", teeth.join("-"), faces)),
            cellc(e.state.label(), odontograma::state_color(e.state)),
            cell(app.db.doctor_alias(e.doctor_id)),
            cell(money(e.amount())),
            cell(e.budget_id.map(|b| format!("#{b}")).unwrap_or_default()),
        ];
        let r = tbl.row(ui, i, false, &cells);
        if actions {
            r.context_menu(|ui| {
                if e.state == TxState::Previsto && ui.button("✔ Marcar como realizado hoy").clicked() {
                    realize = Some(e.id);
                    ui.close();
                }
                if ui.button("✖ Borrar").clicked() {
                    delete = Some(e.id);
                    ui.close();
                }
            });
            if r.double_clicked() && e.state == TxState::Previsto {
                realize = Some(e.id);
            }
        }
    }
    if let Some(id) = realize {
        app.db.realize_entry(id, app.today, 0);
        app.changed();
        app.notify("Tratamiento realizado: se ha cargado en la cuenta del paciente.");
    }
    if let Some(id) = delete {
        app.dialog = Dialog::Confirm { text: "¿Borrar este tratamiento del historial?".into(), action: crate::app::ConfirmAction::DeleteEntry(id) };
    }
}

fn historial(app: &mut App, ui: &mut Ui, pid: u32) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Historial médico (entradas médicas)");
            right_aligned(ui, |ui| {
                for (f, n) in [(Some(TxState::Previo), "Previos"), (Some(TxState::Previsto), "Previstos"), (Some(TxState::Realizado), "Realizados"), (None, "Todos")] {
                    if ui.selectable_label(app.lists.history_filter == f, n).clicked() {
                        app.lists.history_filter = f;
                    }
                }
            });
        });
        add_entry_row(app, ui, pid);
        ui.add_space(6.0);
        let mut es: Vec<MedEntry> = app
            .db
            .entries
            .iter()
            .filter(|e| e.patient_id == pid && app.lists.history_filter.map(|f| e.state == f).unwrap_or(true))
            .cloned()
            .collect();
        es.sort_by_key(|e| std::cmp::Reverse((e.date, e.id)));
        entries_table(app, ui, &es, true);
        muted(ui, "Doble clic en un tratamiento previsto para marcarlo como realizado · clic derecho para más opciones.");
    });
    ui.add_space(10.0);
    if !app.can(crate::users::Perm::Clinico) {
        return;
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Antecedentes y notas clínicas");
        if let Some(p) = app.db.patient_mut(pid) {
            ui.label(RichText::new("Antecedentes médicos").size(13.5).color(MUTED));
            let a = ui.add(egui::TextEdit::multiline(&mut p.background).desired_rows(3).desired_width(f32::INFINITY));
            ui.label(RichText::new("Notas").size(13.5).color(MUTED));
            let b = ui.add(egui::TextEdit::multiline(&mut p.notes).desired_rows(3).desired_width(f32::INFINITY));
            if a.changed() || b.changed() {
                app.changed();
            }
        }
    });
}

fn add_entry_row(app: &mut App, ui: &mut Ui, pid: u32) {
    let key = egui::Id::unique(("add_entry", pid));
    let tariff = app.db.patient(pid).map(|p| p.tariff_id).unwrap_or(1);
    let first = app.db.treatments_for(tariff).next().map(|t| t.code.clone()).unwrap_or_default();
    let mut st: (String, String, TxState, u32) = ui.data_mut(|d| d.get_temp(key)).unwrap_or((first, String::new(), TxState::Realizado, app.db.patient(pid).map(|p| p.doctor_id).unwrap_or(1)));
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Añadir:").color(MUTED));
        crate::screens::treatment_picker(ui, &format!("add_tto{pid}"), &app.db, tariff, &mut st.0, 300.0, |_| true, |t| format!("{} · {}", t.code, t.description));
        ui.add_sized([110.0, 34.0], egui::TextEdit::singleline(&mut st.1).hint_text("Piezas: 16 17"));
        crate::theme::combo("add_state").width(100.0).selected_text(st.2.label()).show_ui(ui, |ui| {
            for s in [TxState::Realizado, TxState::Previsto, TxState::Previo] {
                ui.selectable_value(&mut st.2, s, s.label());
            }
        });
        crate::theme::combo("add_doc").width(110.0).selected_text(app.db.doctor_alias(st.3)).show_ui(ui, |ui| {
            for c in &app.db.collaborators {
                ui.selectable_value(&mut st.3, c.id, &c.alias);
            }
        });
        muted(ui, format!("Precio tarifa: {}", money(app.db.price_for(&st.0, tariff))));
        if primary(ui, "Añadir").clicked() {
            let teeth: Vec<u8> = st.1.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).filter(|t| valid_tooth(*t)).collect();
            let needs = app.db.treatment(&st.0).map(|t| t.per_tooth).unwrap_or(false);
            if app.db.treatment(&st.0).is_none_or(|t| !app.db.treatment_offered(t, tariff)) {
                app.warn("Elija un tratamiento de la lista.");
            } else if needs && teeth.is_empty() {
                app.warn("Este tratamiento necesita al menos una pieza (p. ej. 16).");
            } else {
                app.db.add_entry(pid, &st.0, teeth, vec![], st.2, st.3, app.today);
                app.changed();
                st.1.clear();
                app.notify("Tratamiento añadido al historial.");
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(key, st));
}

pub fn valid_tooth(t: u8) -> bool {
    let (q, u) = (t / 10, t % 10);
    ((1..=4).contains(&q) && (1..=8).contains(&u)) || ((5..=8).contains(&q) && (1..=5).contains(&u))
}

fn filiacion(app: &mut App, ui: &mut Ui, pid: u32) {
    let tariffs: Vec<(u32, String)> = app.db.tariffs.iter().map(|t| (t.id, t.name.clone())).collect();
    let docs: Vec<(u32, String)> = app.db.collaborators.iter().map(|c| (c.id, c.alias.clone())).collect();
    let Some(p) = app.db.patients.iter_mut().find(|p| p.id == pid) else { return };
    let before = serde_json::to_string(p).unwrap_or_default();
    patient_form(ui, p, &tariffs, &docs);
    let after = serde_json::to_string(p).unwrap_or_default();
    if before != after {
        app.changed();
    }
}

pub fn patient_form(ui: &mut Ui, p: &mut Patient, tariffs: &[(u32, String)], docs: &[(u32, String)]) {
    ui.columns(2, |cols| {
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Datos personales");
            ui.horizontal(|ui| {
                field(ui, "Nombre", &mut p.first_name, 160.0);
                field(ui, "Apellidos", &mut p.last_name, 220.0);
            });
            ui.horizontal(|ui| {
                field(ui, "NIF / NIE", &mut p.nif, 120.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Fecha de nacimiento").size(13.5).color(MUTED));
                    let mut d = p.birth.unwrap_or_else(|| NaiveDate::from_ymd_opt(1990, 1, 1).unwrap());
                    if crate::screens::date_picker(ui, "birth", &mut d) {
                        p.birth = Some(d);
                    }
                });
                ui.vertical(|ui| {
                    ui.label(RichText::new("Sexo").size(13.5).color(MUTED));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut p.sex, "F".to_string(), "Mujer");
                        ui.radio_value(&mut p.sex, "M".to_string(), "Hombre");
                    });
                });
            });
            if let Some(err) = nif_problem(&p.nif) {
                ui.label(RichText::new(format!("⚠ {err}")).color(DANGER).size(13.5));
            }
            ui.horizontal(|ui| {
                field(ui, "Móvil", &mut p.mobile, 130.0);
                field(ui, "Teléfono", &mut p.phone, 130.0);
                field(ui, "Email", &mut p.email, 186.0);
            });
            ui.horizontal(|ui| {
                field(ui, "Dirección", &mut p.address, 220.0);
                field(ui, "C.P.", &mut p.zip, 70.0);
                field(ui, "Población", &mut p.city, 140.0);
            });
            ui.horizontal(|ui| {
                field(ui, "Provincia", &mut p.province, 128.0);
                field(ui, "Profesión", &mut p.profession, 144.0);
                field(ui, "Procedencia", &mut p.referral, 128.0);
                ui.menu_button("▾", |ui| {
                    for r in crate::health::REFERRALS {
                        if ui.button(r).clicked() {
                            p.referral = r.to_string();
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("Elegir procedencia");
            });
        });
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Datos administrativos");
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Tarifa / Mutua").size(13.5).color(MUTED));
                    let cur = tariffs.iter().find(|t| t.0 == p.tariff_id).map(|t| t.1.clone()).unwrap_or_default();
                    crate::theme::combo("pf_tar").width(200.0).selected_text(cur).show_ui(ui, |ui| {
                        for (id, n) in tariffs {
                            ui.selectable_value(&mut p.tariff_id, *id, n);
                        }
                    });
                });
                field(ui, "Nº de póliza", &mut p.policy_no, 140.0);
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Doctor responsable").size(13.5).color(MUTED));
                    let cur = docs.iter().find(|t| t.0 == p.doctor_id).map(|t| t.1.clone()).unwrap_or_default();
                    crate::theme::combo("pf_doc").width(200.0).selected_text(cur).show_ui(ui, |ui| {
                        for (id, n) in docs {
                            ui.selectable_value(&mut p.doctor_id, *id, n);
                        }
                    });
                });
                ui.vertical(|ui| {
                    ui.label(RichText::new("Descuento %").size(13.5).color(MUTED));
                    ui.add(egui::DragValue::new(&mut p.discount_pct).range(0.0..=100.0).speed(0.5));
                });
                ui.vertical(|ui| {
                    ui.label(RichText::new("Estado").size(13.5).color(MUTED));
                    ui.checkbox(&mut p.active, "Paciente activo");
                });
            });
            ui.add_space(6.0);
            section(ui, "Domiciliación bancaria (SEPA)");
            ui.horizontal(|ui| {
                field(ui, "IBAN", &mut p.iban, 250.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Mandato").size(13.5).color(MUTED));
                    ui.checkbox(&mut p.sepa_signed, "Firmado");
                });
            });
            if !p.iban.trim().is_empty() {
                if crate::extra::iban_valid(&p.iban) {
                    ui.label(RichText::new(format!("✔ {}", crate::extra::iban_fmt(&p.iban))).color(OK).size(13.0));
                } else {
                    ui.label(RichText::new("⚠ El IBAN no es válido").color(DANGER).size(13.0));
                }
            }
            if p.sepa_signed && p.sepa_date.is_none() {
                p.sepa_date = Some(crate::model::today());
            }
            ui.add_space(6.0);
            section(ui, "Alertas médicas");
            field(ui, "Alertas (se muestran en toda la aplicación)", &mut p.alerts, 380.0);
            field(ui, "Alergias", &mut p.allergies, 380.0);
            ui.add_space(6.0);
            section(ui, "Protección de datos y comunicaciones");
            ui.checkbox(&mut p.accepts_gdpr, "Ha firmado el consentimiento de protección de datos");
            ui.horizontal(|ui| {
                ui.checkbox(&mut p.accepts_sms, "SMS");
                ui.checkbox(&mut p.accepts_email, "Email");
                ui.checkbox(&mut p.accepts_whatsapp, "WhatsApp");
            });
        });
    });
}

pub fn new_dialog(app: &mut App, ctx: &egui::Context, mut p: Box<Patient>) -> Dialog {
    let tariffs: Vec<(u32, String)> = app.db.tariffs.iter().map(|t| (t.id, t.name.clone())).collect();
    let docs: Vec<(u32, String)> = app.db.collaborators.iter().map(|c| (c.id, c.alias.clone())).collect();
    let mut open = true;
    let mut save = false;
    crate::theme::modal("new_patient").show(ctx, |ui| {
        ui.set_width(1150.0);
        title(ui, "Nuevo paciente", &format!("Se asignará el Nº de historial {}", p.history_no));
        patient_form(ui, &mut p, &tariffs, &docs);
        // posibles duplicados
        let dups: Vec<String> = if p.last_name.len() >= 3 || p.nif.len() >= 5 || p.mobile.len() >= 6 {
            app.db
                .patients
                .iter()
                .filter(|x| {
                    (!p.nif.is_empty() && fold(&x.nif) == fold(&p.nif))
                        || (!p.mobile.is_empty() && x.mobile.replace(' ', "") == p.mobile.replace(' ', ""))
                        || (p.last_name.len() >= 3 && fold(&x.full_name()) == fold(&p.full_name()))
                })
                .map(|x| format!("{} (Nº {})", x.full_name(), x.history_no))
                .collect()
        } else {
            vec![]
        };
        if !dups.is_empty() {
            ui.label(RichText::new(format!("⚠ Posible duplicado: {}", dups.join(", "))).color(WARN));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let ok = !p.first_name.trim().is_empty() && !p.last_name.trim().is_empty();
            if ui.add_enabled(ok, egui::Button::new(RichText::new("Guardar paciente").color(Color32::WHITE)).fill(ACCENT)).clicked() {
                save = true;
            }
            if ui.button("Cancelar").clicked() {
                open = false;
            }
            if !ok {
                muted(ui, "Nombre y apellidos son obligatorios.");
            }
        });
    });
    if save {
        let id = app.db.new_id();
        p.id = id;
        p.history_no = app.db.next_history_no();
        app.db.patients.push(*p);
        app.changed();
        app.open_patient(id, PatTab::Resumen);
        app.notify("Paciente dado de alta.");
        return app.dialog_take_or_none();
    }
    if open { Dialog::NewPatient(p) } else { app.dialog_take_or_none() }
}

fn cuenta(app: &mut App, ui: &mut Ui, pid: u32) {
    let w = kpi_w(ui, 3);
    let debit = app.db.patient_debit(pid);
    let paid = app.db.patient_paid(pid);
    ui.horizontal(|ui| {
        kpi(ui, w, "Debe (tratamientos realizados)", &money(debit), "", INFO);
        ui.add_space(4.0);
        kpi(ui, w, "Haber (cobros)", &money(paid), "", OK);
        ui.add_space(4.0);
        let bal = debit - paid;
        kpi(ui, w, "Saldo", &money(bal), if bal > 0.005 { "pendiente de cobro" } else if bal < -0.005 { "a favor del paciente" } else { "al día" }, if bal > 0.005 { DANGER } else { OK });
    });
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section(ui, "Movimientos de la cuenta");
            right_aligned(ui, |ui| {
                if primary(ui, "💶 Registrar cobro").clicked() {
                    app.dialog = Dialog::Payment(cobros::PayDraft::new(app, pid));
                }
            });
        });
        // Movimientos: cargos y abonos con saldo acumulado.
        let mut movs: Vec<(chrono::NaiveDateTime, String, f64, f64)> = vec![];
        for e in app.db.entries.iter().filter(|e| e.patient_id == pid && e.state == TxState::Realizado) {
            let teeth: Vec<String> = e.teeth.iter().map(|t| t.to_string()).collect();
            movs.push((e.date.and_hms_opt(0, 0, 0).unwrap(), format!("{} {}", e.description, teeth.join("-")), e.amount(), 0.0));
        }
        for p in app.db.payments.iter().filter(|p| p.patient_id == pid) {
            movs.push((p.date, format!("Cobro {} · {} · {}", p.receipt_no, p.method, p.concept), 0.0, p.amount));
        }
        movs.sort_by_key(|m| m.0);
        let tbl = Table::new("acc", vec![("Fecha", 100.0), ("Concepto", 420.0), ("Debe", 110.0), ("Haber", 110.0), ("Saldo", 110.0)]);
        tbl.header(ui);
        let mut bal = 0.0;
        for (i, (d, c, de, ha)) in movs.iter().enumerate() {
            bal += de - ha;
            tbl.row(ui, i, false, &[
                cell(fmt_date(d.date())),
                cell(c),
                cell(if *de > 0.0 { money(*de) } else { String::new() }),
                cellc(if *ha > 0.0 { money(*ha) } else { String::new() }, OK),
                cellc(money(bal), if bal > 0.005 { DANGER } else { TEXT }),
            ]);
        }
        if movs.is_empty() {
            empty_state(ui, "Sin movimientos");
        }
    });
    ui.add_space(10.0);
    crate::screens::facturas::patient_block(app, ui, pid);
    // Planes de financiación de presupuestos aceptados.
    let plans: Vec<(u32, String, f64, FinancePlan)> = app
        .db
        .budgets
        .iter()
        .filter(|b| b.patient_id == pid && b.plan.is_some())
        .map(|b| (b.id, b.title.clone(), b.total(), b.plan.clone().unwrap()))
        .collect();
    for (bid, t, total, plan) in plans {
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            section(ui, &format!("Plan económico · {t}"));
            muted(ui, format!("Total {} · entrada {} · {} cuotas de {}", money(total), money(plan.down_payment), plan.installments, money(plan.installment_amount(total))));
            let mut pay_idx = None;
            ui.horizontal_wrapped(|ui| {
                for i in 0..plan.installments {
                    let paid = plan.paid.get(i as usize).copied().unwrap_or(false);
                    let due = plan.due_date(i);
                    let overdue = !paid && due < app.today;
                    let col = if paid { OK } else if overdue { DANGER } else { MUTED };
                    egui::Frame::new().fill(tint(col, 0.1)).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("Cuota {}", i + 1)).family(crate::theme::sb()).color(col));
                            ui.label(RichText::new(fmt_date(due)).size(13.5));
                            if paid {
                                ui.label(RichText::new("Pagada").size(13.5).color(OK));
                            } else if ui.small_button("Cobrar").clicked() {
                                pay_idx = Some(i);
                            }
                        });
                    });
                }
            });
            if let Some(i) = pay_idx {
                let amount = plan.installment_amount(total);
                app.db.add_payment(pid, amount, "Financiación", &format!("Cuota {} plan presupuesto #{bid}", i + 1));
                if let Some(b) = app.db.budgets.iter_mut().find(|b| b.id == bid) && let Some(pl) = &mut b.plan {
                    if pl.paid.len() < pl.installments as usize {
                        pl.paid.resize(pl.installments as usize, false);
                    }
                    pl.paid[i as usize] = true;
                }
                app.changed();
                app.notify(format!("Cuota {} cobrada: {}", i + 1, money(amount)));
            }
        });
    }
}

fn citas(app: &mut App, ui: &mut Ui, pid: u32) {
    ui.columns(2, |cols| {
        card(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                section(ui, "Citas");
                right_aligned(ui, |ui| {
                    if ui.button("＋ Nueva cita").clicked() {
                        let d = agenda::ApptDraft::new_at(app, app.today, None, Some(pid));
                        app.dialog = Dialog::Appointment(d);
                    }
                });
            });
            let mut list: Vec<Appointment> = app.db.appointments.iter().filter(|a| a.patient_id == Some(pid)).cloned().collect();
            list.sort_by_key(|a| std::cmp::Reverse((a.date, a.start)));
            if list.is_empty() {
                empty_state(ui, "Sin citas");
            }
            let mut edit = None;
            for a in list {
                ui.horizontal(|ui| {
                    pill(ui, a.state.label(), rgb(a.state.color()));
                    let txt = format!("{} {} · {} · {}", fmt_date(a.date), a.start.format("%H:%M"), a.reason, app.db.doctor_alias(a.doctor_id));
                    if ui.add(egui::Label::new(txt).sense(Sense::click())).on_hover_text("Abrir cita").clicked() {
                        edit = Some(a.clone());
                    }
                });
            }
            if let Some(a) = edit {
                app.dialog = Dialog::Appointment(agenda::ApptDraft::from(&a));
            }
        });
        card(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Recalls del paciente");
            let list: Vec<Recall> = app.db.recalls.iter().filter(|r| r.patient_id == pid).cloned().collect();
            if list.is_empty() {
                muted(ui, "Sin recalls.");
            }
            for r in &list {
                ui.horizontal(|ui| {
                    pill(ui, r.state.label(), crate::screens::recalls::state_color(r.state));
                    ui.label(format!("{} · {}", fmt_date(r.date), r.reason));
                });
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for (reason, months) in [("Revisión semestral", 6), ("Higiene anual", 12)] {
                    if ui.button(format!("＋ {reason}")).clicked() {
                        let id = app.db.new_id();
                        let doc = app.db.patient(pid).map(|p| p.doctor_id).unwrap_or(1);
                        app.db.recalls.push(Recall {
                            id,
                            patient_id: pid,
                            date: add_months(app.today, months),
                            reason: reason.into(),
                            doctor_id: doc,
                            state: RecallState::Pendiente,
                            channel: String::new(),
                            comment: String::new(),
                            contacted_on: None,
                        });
                        app.changed();
                        app.notify("Recall programado.");
                    }
                }
            });
        });
    });
    ui.add_space(10.0);
    crate::screens::laboratorio::patient_block(app, ui, pid);
    ui.add_space(10.0);
    crate::screens::comunicaciones::patient_block(app, ui, pid);
}
