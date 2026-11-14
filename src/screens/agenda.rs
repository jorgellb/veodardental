//! Agenda: book diario por doctor, vista semanal, flujo de recepción
//! (llega → sala de espera → consulta → finalizada), búsqueda de huecos,
//! lista de espera y arrastrar citas para reprogramarlas.

use crate::app::{App, ConfirmAction, Dialog, PatTab};
use crate::model::*;
use crate::screens::pacientes;
use crate::theme::*;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Timelike};
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dia,
    Semana,
}

pub struct AgendaState {
    pub date: NaiveDate,
    pub view: View,
    pub hidden: Vec<u32>,
    pub week_doctor: u32,
    pub px_per_min: f32,
    pub drag: Option<(u32, egui::Vec2)>,
    pub gap_doctor: u32,
    pub gap_minutes: u32,
    pub gap_afternoon: bool,
    pub gaps: Vec<(NaiveDate, NaiveTime)>,
    pub selected: Option<u32>,
    pub blocks_open: bool,
    pub blk_doctor: u32,
    pub blk_every: bool,
    pub blk_date: NaiveDate,
    pub blk_start: NaiveTime,
    pub blk_end: NaiveTime,
    pub blk_reason: String,
    pub hol_date: NaiveDate,
    pub hol_name: String,
}

impl AgendaState {
    pub fn new(d: NaiveDate) -> Self {
        Self {
            date: d,
            view: View::Dia,
            hidden: vec![],
            week_doctor: 1,
            px_per_min: 1.55,
            drag: None,
            gap_doctor: 1,
            gap_minutes: 30,
            gap_afternoon: false,
            gaps: vec![],
            selected: None,
            blocks_open: false,
            blk_doctor: 1,
            blk_every: false,
            blk_date: d,
            blk_start: NaiveTime::from_hms_opt(14, 0, 0).unwrap(),
            blk_end: NaiveTime::from_hms_opt(15, 0, 0).unwrap(),
            blk_reason: "Formación".into(),
            hol_date: d,
            hol_name: "Fiesta local".into(),
        }
    }
}

#[derive(Clone)]
pub struct ApptDraft {
    pub a: Appointment,
    pub is_new: bool,
    pub search: String,
    /// 0 = no se repite, 1 = cada semana, 2 = cada 2 semanas, 3 = cada mes.
    pub repeat: u8,
    pub repeat_count: u32,
    /// Cita original cuando esta es una reprogramación.
    pub from_id: Option<u32>,
}

impl ApptDraft {
    pub fn new_at(app: &App, date: NaiveDate, at: Option<(NaiveTime, u32)>, patient: Option<u32>) -> Self {
        let (start, doc) = at.unwrap_or_else(|| {
            let free = app.db.find_gaps(app.agenda.gap_doctor, date, 30, NaiveTime::from_hms_opt(9, 0, 0).unwrap(), app.db.config.close_time, 10);
            match free.first() {
                Some((_, t)) => (*t, app.agenda.gap_doctor),
                None => (NaiveTime::from_hms_opt(9, 0, 0).unwrap(), 1),
            }
        });
        let doc = patient.and_then(|p| at.is_none().then(|| app.db.patient(p).map(|x| x.doctor_id)).flatten()).unwrap_or(doc);
        let center = cid(app.db.config.current_center);
        let boxes: Vec<u32> = app.db.boxes.iter().filter(|b| cid(b.center_id) == center).map(|b| b.id).collect();
        let box_id = if boxes.contains(&doc) { doc } else { boxes.first().copied().unwrap_or(1) };
        ApptDraft {
            a: Appointment {
                id: 0,
                date,
                start,
                minutes: 30,
                patient_id: patient,
                free_name: String::new(),
                phone: String::new(),
                doctor_id: doc,
                box_id,
                reason: "Revisión".into(),
                treatment_code: "0102".into(),
                state: ApptState::Planificada,
                notes: String::new(),
                arrived_at: None,
                seen_at: None,
                finished_at: None,
                reminder_sent: false,
                first_visit: patient.is_none(),
                center_id: center,
            },
            is_new: true,
            search: String::new(),
            repeat: 0,
            repeat_count: 4,
            from_id: None,
        }
    }
    pub fn from(a: &Appointment) -> Self {
        ApptDraft { a: a.clone(), is_new: false, search: String::new(), repeat: 0, repeat_count: 4, from_id: None }
    }
}

/// Cambia el estado de una cita registrando las horas del flujo de recepción.
pub fn set_state(db: &mut Db, id: u32, st: ApptState) {
    let now = chrono::Local::now().time();
    let now = NaiveTime::from_hms_opt(now.hour(), now.minute(), 0).unwrap();
    let prev = db.appointments.iter().find(|a| a.id == id).map(|a| a.state);
    if st == ApptState::SalaEspera && prev != Some(ApptState::SalaEspera) {
        let from = db.config.current_user.clone();
        db.arrival_notice(id, &from);
    }
    if let Some(a) = db.appointments.iter_mut().find(|a| a.id == id) {
        a.state = st;
        match st {
            ApptState::SalaEspera | ApptState::Recepcion => {
                if a.arrived_at.is_none() {
                    a.arrived_at = Some(now);
                }
            }
            ApptState::EnConsulta => {
                if a.arrived_at.is_none() {
                    a.arrived_at = Some(now);
                }
                a.seen_at = Some(now);
            }
            ApptState::Finalizada => {
                if a.seen_at.is_none() {
                    a.seen_at = Some(now);
                }
                a.finished_at = Some(now);
            }
            _ => {}
        }
    }
    // si no acude o la anula, su recall vuelve a pendiente
    if st.is_cancelled() || prev.is_some_and(|p| p.is_cancelled()) {
        db.refresh_recalls(crate::model::today());
    }
}

fn hm(t: NaiveTime) -> String {
    t.format("%H:%M").to_string()
}

fn minutes_of(t: NaiveTime) -> i64 {
    (t.hour() * 60 + t.minute()) as i64
}

pub fn show(app: &mut App, ui: &mut Ui) {
    // ── cabecera ──
    ui.horizontal(|ui| {
        title(ui, "Agenda", &format!("{} · {}", long_date(app.agenda.date), if app.agenda.view == View::Dia { "book diario" } else { "vista semanal" }));
        right_aligned(ui, |ui| {
            if ui.button("📺 Pizarra").on_hover_text("Pantalla para la televisión de la sala de espera").clicked() {
                app.screen = crate::app::Screen::Pizarra;
            }
            if ui.button("🖨 Imprimir día").on_hover_text("Hoja con las citas del día por doctor").clicked() {
                let html = app.db.day_sheet_html(app.agenda.date);
                match crate::screens::centros::open_html(ui.ctx(), &format!("agenda_{}.html", app.agenda.date.format("%Y%m%d")), html) {
                    Ok(_) => app.notify("Agenda del día abierta para imprimir."),
                    Err(e) => app.warn(format!("No se pudo crear la hoja: {e}")),
                }
            }
            if ui.button("🗓 Festivos y bloqueos").clicked() {
                app.agenda.blocks_open = true;
                app.agenda.blk_date = app.agenda.date;
                app.agenda.hol_date = app.agenda.date;
            }
            if primary(ui, "＋ Nueva cita").clicked() {
                let d = ApptDraft::new_at(app, app.agenda.date, None, None);
                app.dialog = Dialog::Appointment(d);
            }
        });
    });
    ui.horizontal(|ui| {
        if ui.button("Hoy").clicked() {
            app.agenda.date = app.today;
        }
        if app.agenda.view == View::Semana && ui.button("« Semana").clicked() {
            app.agenda.date -= Duration::days(7);
        }
        crate::screens::date_picker(ui, "agd_date", &mut app.agenda.date);
        if app.agenda.view == View::Semana && ui.button("Semana »").clicked() {
            app.agenda.date += Duration::days(7);
        }
        ui.add_space(14.0);
        if ui.selectable_label(app.agenda.view == View::Dia, "Día").clicked() {
            app.agenda.view = View::Dia;
        }
        if ui.selectable_label(app.agenda.view == View::Semana, "Semana").clicked() {
            app.agenda.view = View::Semana;
        }
    });
    ui.add_space(4.0);
    if let Some(h) = app.db.holiday(app.agenda.date).filter(|_| app.agenda.view == View::Dia) {
        ui.label(RichText::new(format!("🎉 Festivo: {} — no se dan citas este día", h.name)).color(WARN).family(crate::theme::sb()));
    }
    if app.agenda.blocks_open {
        blocks_dialog(app, ui.ctx());
    }
    let side_w = 340.0;
    let avail = ui.available_size();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(avail.x - side_w - 14.0);
            // filtros de doctor
            ui.horizontal_wrapped(|ui| {
                let center = cid(app.db.config.current_center);
                let docs: Vec<Collaborator> = app.db.collaborators.iter().filter(|c| c.active && app.db.works_in(c.id, center)).cloned().collect();
                for c in &docs {
                    if app.agenda.view == View::Dia {
                        let mut on = !app.agenda.hidden.contains(&c.id);
                        let dc = rgb(c.color);
                        let b = egui::Button::new(RichText::new(format!("●  {}", c.alias)).font(mdf(13.5)).color(if on { mix(dc, HEADING, 0.35) } else { SUBTLE() }))
                            .fill(if on { tint(dc, 0.13) } else { CARD })
                            .stroke(Stroke::new(1.0, if on { tint(dc, 0.45) } else { BORDER() }))
                            .corner_radius(CornerRadius::same(17))
                            .min_size(vec2(0.0, 34.0));
                        if ui.add(b).clicked() {
                            on = !on;
                            if on {
                                app.agenda.hidden.retain(|x| *x != c.id);
                            } else {
                                app.agenda.hidden.push(c.id);
                            }
                        }
                    } else {
                        let sel = app.agenda.week_doctor == c.id;
                        let b = egui::Button::new(RichText::new(&c.alias).color(if sel { Color32::WHITE } else { rgb(c.color) })).fill(if sel { rgb(c.color) } else { tint(rgb(c.color), 0.12) }).corner_radius(CornerRadius::same(12));
                        if ui.add(b).clicked() {
                            app.agenda.week_doctor = c.id;
                        }
                    }
                }
                right_aligned(ui, |ui| {
                    ui.add(egui::Slider::new(&mut app.agenda.px_per_min, 0.8..=2.5).show_value(false).text("Zoom"));
                });
            });
            ui.add_space(4.0);
            egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, BORDER())).corner_radius(CornerRadius::same(12)).show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("agd_scroll").auto_shrink([false, false]).max_height(avail.y - 70.0).show(ui, |ui| {
                    grid(app, ui);
                });
            });
        });
        ui.vertical(|ui| {
            ui.set_width(side_w);
            let mut side_sa = egui::ScrollArea::vertical().id_salt("agd_side").max_height(avail.y - 10.0);
            if let Some(y) = app.lists.scroll_to.take() {
                side_sa = side_sa.vertical_scroll_offset(y);
            }
            side_sa.show(ui, |ui| {
                ui.set_width(side_w - 8.0);
                side(app, ui);
            });
        });
    });
}

struct Col {
    doctor: u32,
    date: NaiveDate,
    title: String,
    color: Color32,
}

fn grid(app: &mut App, ui: &mut Ui) {
    let cols: Vec<Col> = match app.agenda.view {
        View::Dia => app
            .db
            .collaborators
            .iter()
            .filter(|c| c.active && !app.agenda.hidden.contains(&c.id) && app.db.works_in(c.id, cid(app.db.config.current_center)))
            .map(|c| Col { doctor: c.id, date: app.agenda.date, title: format!("{} · {}", c.alias, c.specialty), color: rgb(c.color) })
            .collect(),
        View::Semana => {
            let monday = app.agenda.date - Duration::days(app.agenda.date.weekday().num_days_from_monday() as i64);
            let c = app.db.doctor(app.agenda.week_doctor).cloned();
            // de lunes a sábado (y el domingo, si la clínica abre)
            let days = if app.db.clinic_week().0[6].is_empty() { 6 } else { 7 };
            (0..days)
                .map(|i| {
                    let d = monday + Duration::days(i);
                    Col { doctor: app.agenda.week_doctor, date: d, title: format!("{} {}", weekday_es(d), d.format("%d/%m")), color: c.as_ref().map(|c| rgb(c.color)).unwrap_or(ACCENT) }
                })
                .collect()
        }
    };
    if cols.is_empty() {
        empty_state(ui, "Seleccione al menos un doctor.");
        return;
    }
    let open = minutes_of(app.db.config.open_time);
    let close = minutes_of(app.db.config.close_time);
    let ppm = app.agenda.px_per_min;
    let head_h = 34.0;
    let time_w = 54.0;
    let width = ui.available_width();
    let col_w = ((width - time_w) / cols.len() as f32).max(120.0);
    let height = head_h + (close - open) as f32 * ppm + 10.0;
    let (rect, bg_resp) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let p = ui.painter().clone();
    let y_of = |m: i64| rect.min.y + head_h + (m - open) as f32 * ppm;
    let now = chrono::Local::now().time();

    // cabeceras y fondo
    for (i, c) in cols.iter().enumerate() {
        let x = rect.min.x + time_w + i as f32 * col_w;
        let hr = Rect::from_min_size(pos2(x, rect.min.y), vec2(col_w, head_h));
        let is_today = c.date == app.today;
        p.rect_filled(hr.shrink(2.0), CornerRadius::same(6), if is_today && app.agenda.view == View::Semana { tint(ACCENT, 0.15) } else { tint(c.color, 0.12) });
        p.with_clip_rect(hr.shrink2(vec2(6.0, 0.0)).with_max_x(hr.max.x - 22.0)).text(hr.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, &c.title, FontId::proportional(14.5), TEXT);
        let n = app.db.appointments.iter().filter(|a| a.date == c.date && a.doctor_id == c.doctor && !a.state.is_cancelled()).count();
        p.text(hr.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, n.to_string(), FontId::proportional(12.5), MUTED());
        if i > 0 {
            p.line_segment([pos2(x, rect.min.y + head_h), pos2(x, rect.max.y)], Stroke::new(1.0, BORDER()));
        }
        // fuera del horario del profesional (o de la clínica), en gris
        if app.db.holiday(c.date).is_none() {
            let shifts = app.db.doctor_day(c.doctor, c.date);
            let off = Color32::from_rgb(236, 239, 243);
            let mut from = open;
            for (a, b) in shifts.iter().map(|s| (minutes_of(s.start), minutes_of(s.end))).chain(std::iter::once((close, close))) {
                if a > from {
                    p.rect_filled(Rect::from_min_max(pos2(x + 1.0, y_of(from)), pos2(x + col_w - 1.0, y_of(a.min(close)))), CornerRadius::ZERO, off);
                }
                from = from.max(b);
            }
            if shifts.is_empty() {
                let why = if app.db.clinic_day(c.date).is_empty() { "Cerrado" } else { "No pasa consulta" };
                p.text(pos2(x + col_w / 2.0, y_of(open) + 40.0), Align2::CENTER_CENTER, why, FontId::proportional(14.0), MUTED());
            }
        }
        if let Some(h) = app.db.holiday(c.date) {
            let r = Rect::from_min_max(pos2(x + 1.0, y_of(open)), pos2(x + col_w - 1.0, y_of(close)));
            p.rect_filled(r, CornerRadius::ZERO, Color32::from_rgb(253, 246, 232));
            p.text(pos2(r.center().x, r.min.y + 40.0), Align2::CENTER_CENTER, format!("Festivo · {}", h.name), FontId::proportional(14.0), WARN);
        }
    }
    let mut m = open;
    while m <= close {
        let y = y_of(m);
        let full = m % 60 == 0;
        p.line_segment([pos2(rect.min.x + time_w, y), pos2(rect.max.x, y)], Stroke::new(1.0, if full { Color32::from_rgb(222, 228, 236) } else { Color32::from_rgb(242, 245, 248) }));
        if m % 30 == 0 && m < close {
            p.text(pos2(rect.min.x + time_w - 8.0, y), Align2::RIGHT_TOP, format!("{:02}:{:02}", m / 60, m % 60), FontId::proportional(if full { 12.0 } else { 10.5 }), if full { TEXT } else { MUTED() });
        }
        m += 15;
    }
    // bloqueos
    for (i, c) in cols.iter().enumerate() {
        let x = rect.min.x + time_w + i as f32 * col_w;
        for b in app.db.blocks.iter().filter(|b| b.doctor_id == c.doctor && b.date.map(|d| d == c.date).unwrap_or(true)) {
            let r = Rect::from_min_max(pos2(x + 3.0, y_of(minutes_of(b.start))), pos2(x + col_w - 3.0, y_of(minutes_of(b.end))));
            p.rect_filled(r, CornerRadius::same(4), Color32::from_rgb(238, 240, 244));
            let ph = p.with_clip_rect(r);
            let mut xx = r.min.x - r.height();
            while xx < r.max.x {
                ph.line_segment([pos2(xx, r.max.y), pos2(xx + r.height(), r.min.y)], Stroke::new(1.0, Color32::from_rgb(222, 226, 232)));
                xx += 9.0;
            }
            p.text(r.center(), Align2::CENTER_CENTER, &b.reason, FontId::proportional(13.5), MUTED());
        }
    }
    // citas pedidas por internet, aún por confirmar: contorno discontinuo
    let violet = crate::theme::VIOLET;
    for r in app.db.booking_requests.iter().filter(|r| r.state == crate::booking::ReqState::Pendiente && cid(r.center_id) == cid(app.db.config.current_center)) {
        let Some(ci) = cols.iter().position(|c| c.date == r.date && c.doctor == r.doctor_id) else { continue };
        let x = rect.min.x + time_w + ci as f32 * col_w;
        let rr = Rect::from_min_max(pos2(x + 4.0, y_of(minutes_of(r.start)) + 1.0), pos2(x + col_w - 4.0, y_of(minutes_of(r.end())) - 1.0));
        p.rect_filled(rr, CornerRadius::same(8), Color32::from_rgba_unmultiplied(124, 58, 237, 14));
        let pts = [rr.left_top(), rr.right_top(), rr.right_bottom(), rr.left_bottom(), rr.left_top()];
        for w in pts.windows(2) {
            p.extend(egui::Shape::dashed_line(&[w[0], w[1]], Stroke::new(1.4, tint(violet, 0.7)), 5.0, 4.0));
        }
        let clip = p.with_clip_rect(rr.shrink(3.0));
        clip.text(rr.min + vec2(10.0, 5.0), Align2::LEFT_TOP, format!("{} {} · por internet", hm(r.start), r.name), FontId::proportional(13.0), mix(violet, HEADING, 0.3));
        if rr.height() > 34.0 {
            clip.text(rr.min + vec2(10.0, 23.0), Align2::LEFT_TOP, format!("{} · por confirmar", r.reason), FontId::proportional(12.0), MUTED());
        }
        ui.interact(rr, ui.scope_id().with(("req", r.id)), Sense::hover()).on_hover_text("Cita pedida por internet: confírmela o recházela en el panel de la derecha.");
    }
    // línea de la hora actual
    for (i, c) in cols.iter().enumerate() {
        if c.date == app.today {
            let nm = (now.hour() * 60 + now.minute()) as i64;
            if nm >= open && nm <= close {
                let x = rect.min.x + time_w + i as f32 * col_w;
                let y = y_of(nm);
                p.line_segment([pos2(x, y), pos2(x + col_w, y)], Stroke::new(2.0, DANGER));
                p.circle_filled(pos2(x + 3.0, y), 4.0, DANGER);
            }
        }
    }

    // citas
    let appts: Vec<Appointment> = app.db.appointments.iter().filter(|a| cid(a.center_id) == cid(app.db.config.current_center) && cols.iter().any(|c| c.date == a.date && c.doctor == a.doctor_id)).cloned().collect();
    // las que se solapan (dadas a la vez a propósito, o por dos puestos o la
    // cita online), una al lado de otra; las anuladas, debajo de las demás
    let mut lane_of = std::collections::HashMap::new();
    for c in &cols {
        let items: Vec<(u32, i64, i64)> = appts.iter().filter(|a| a.date == c.date && a.doctor_id == c.doctor && !a.state.is_cancelled()).map(|a| (a.id, minutes_of(a.start), minutes_of(a.end()))).collect();
        lane_of.extend(crate::agenda_extra::lanes(&items));
    }
    let mut appts = appts;
    appts.sort_by_key(|a| !a.state.is_cancelled());
    let mut open_dialog: Option<Appointment> = None;
    let mut drop: Option<(u32, NaiveDate, u32, NaiveTime)> = None;
    for a in &appts {
        let Some(ci) = cols.iter().position(|c| c.date == a.date && c.doctor == a.doctor_id) else { continue };
        let x = rect.min.x + time_w + ci as f32 * col_w;
        let (lane, n) = lane_of.get(&a.id).copied().unwrap_or((0, 1));
        let lane_w = (col_w - 8.0) / n as f32;
        let x0 = x + 4.0 + lane as f32 * lane_w;
        let mut r = Rect::from_min_max(pos2(x0, y_of(minutes_of(a.start)) + 1.0), pos2(x0 + lane_w - if n > 1 { 2.0 } else { 0.0 }, y_of(minutes_of(a.end())) - 1.0));
        let id = ui.scope_id().with(("appt", a.id));
        let resp = ui.interact(r, id, Sense::click_and_drag());
        if resp.drag_started() {
            app.agenda.drag = Some((a.id, egui::Vec2::ZERO));
        }
        if let Some((did, off)) = &mut app.agenda.drag
            && *did == a.id
        {
            if resp.dragged() {
                *off += resp.drag_delta();
            }
            r = r.translate(*off);
            if resp.drag_stopped() {
                let delta_min = ((off.y / ppm) / 15.0).round() as i64 * 15;
                let dcol = (off.x / col_w).round() as i64;
                let nci = (ci as i64 + dcol).clamp(0, cols.len() as i64 - 1) as usize;
                let ns = a.start + Duration::minutes(delta_min);
                if delta_min != 0 || nci != ci {
                    drop = Some((a.id, cols[nci].date, cols[nci].doctor, ns));
                }
                app.agenda.drag = None;
            }
        }
        let doc_col = app.db.doctor(a.doctor_id).map(|c| rgb(c.color)).unwrap_or(ACCENT);
        let st_col = rgb(a.state.color());
        let cancelled = a.state.is_cancelled();
        let fill = if cancelled { Color32::from_rgb(245, 246, 248) } else { tint(doc_col, 0.12) };
        let sel_or_hover = app.agenda.selected == Some(a.id) || resp.hovered();
        if sel_or_hover && !cancelled {
            p.add(egui::Shadow { offset: [0, 3], blur: 10, spread: 0, color: Color32::from_rgba_unmultiplied(15, 30, 60, 30) }.as_shape(r, CornerRadius::same(8)));
        }
        p.rect_filled(r, CornerRadius::same(8), fill);
        p.rect_stroke(r, CornerRadius::same(8), Stroke::new(if sel_or_hover { 1.5 } else { 1.0 }, if sel_or_hover { doc_col } else { tint(doc_col, 0.32) }), StrokeKind::Inside);
        p.rect_filled(Rect::from_min_size(r.min + vec2(3.0, 3.0), vec2(3.5, (r.height() - 6.0).max(2.0))), CornerRadius::same(2), st_col);
        let clip = p.with_clip_rect(r.shrink(2.0));
        let name = app.db.appt_name(a);
        let alert = a.patient_id.and_then(|id| app.db.patient(id)).map(|p| !p.alerts.is_empty() || !p.allergies.is_empty()).unwrap_or(false);
        let tc = if cancelled { MUTED() } else { TEXT };
        // en un carril estrecho (citas solapadas), primero el nombre: la hora ya
        // se ve por dónde empieza la cita
        let mut line1 = if r.width() < 150.0 { format!("{name} · {}", hm(a.start)) } else { format!("{} {}", hm(a.start), name) };
        if alert {
            line1 = format!("⚠ {line1}");
        }
        if a.first_visit {
            line1.push_str("  ★");
        }
        // respuesta al recordatorio de WhatsApp, en la esquina
        let wa_badge = if a.state != ApptState::SalaEspera { crate::screens::whatsapp::appt_badge(&app.db, a.id) } else { None };
        let mut line1_right = r.right() - 4.0;
        // tiempo de espera en la esquina: el nombre se corta antes
        let wait_text = if a.state == ApptState::SalaEspera && a.date == app.today { a.arrived_at.map(|arr| format!("⏱ {}′", (minutes_of(now.with_second(0).unwrap_or(now)) - minutes_of(arr)).max(0))) } else { None };
        if let Some(t) = &wait_text {
            let gw = p.layout_no_wrap(t.clone(), mdf(12.0), WARN).size().x;
            line1_right = (r.right() - gw - 10.0).max(r.left() + 40.0);
        }
        let mut draw_badge = None;
        if let Some((mark, col)) = wa_badge {
            let g = p.layout_no_wrap(mark.to_string(), FontId::proportional(12.5), col);
            let br = Rect::from_min_max(pos2(r.right() - g.size().x - 12.0, r.top() + 3.0), pos2(r.right() - 2.0, r.top() + 5.0 + g.size().y));
            if br.left() > r.left() + 40.0 {
                line1_right = br.left() - 4.0;
                draw_badge = Some((br, g, col));
            }
        }
        let line1_clip = p.with_clip_rect(Rect::from_min_max(r.min, pos2(line1_right, r.bottom())));
        line1_clip.text(r.min + vec2(12.0, 4.0), Align2::LEFT_TOP, line1, mdf(13.5), if cancelled { tc } else { mix(doc_col, HEADING, 0.7) });
        if let Some((br, g, col)) = draw_badge {
            clip.rect_filled(br, CornerRadius::same(5), Color32::WHITE);
            clip.galley(br.min + vec2(4.0, 1.0), g, col);
        }
        if r.height() > 30.0 {
            clip.text(r.min + vec2(12.0, 21.0), Align2::LEFT_TOP, format!("{} · {}", a.reason, a.state.label()), FontId::proportional(12.5), if cancelled { MUTED() } else { mix(st_col, HEADING, 0.15) });
        }
        if let Some(t) = wait_text {
            clip.text(r.right_top() + vec2(-6.0, 5.0), Align2::RIGHT_TOP, t, mdf(12.0), WARN);
        }
        if resp.hovered() && app.agenda.drag.is_none() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        let r2 = resp.on_hover_ui(|ui| {
            ui.label(RichText::new(app.db.appt_name(a)).family(crate::theme::sb()));
            ui.label(format!("{}–{} · {}", hm(a.start), hm(a.end()), a.reason));
            ui.label(format!("{} · {}", app.db.doctor_alias(a.doctor_id), app.db.box_name(a.box_id)));
            ui.label(format!("Estado: {}", a.state.label()));
            if let Some(t) = crate::screens::whatsapp::appt_line(&app.db, a.id) {
                ui.label(RichText::new(t).color(crate::screens::whatsapp::WA_GREEN));
            }
            if !a.notes.is_empty() {
                ui.label(&a.notes);
            }
            muted(ui, "Clic para abrir · arrastre para mover");
        });
        if r2.clicked() {
            app.agenda.selected = Some(a.id);
            open_dialog = Some(a.clone());
        }
        r2.context_menu(|ui| {
            for st in ApptState::ALL {
                if ui.button(format!("● {}", st.label())).clicked() {
                    set_state(&mut app.db, a.id, st);
                    app.dirty = true;
                    ui.close();
                }
            }
        });
    }
    if let Some((id, date, doc, start)) = drop {
        let minutes = app.db.appointments.iter().find(|a| a.id == id).map(|a| a.minutes).unwrap_or(30);
        let end = start + Duration::minutes(minutes as i64);
        if let Some(why) = app.db.schedule_problem(doc, date, start, end) {
            app.warn(why);
        } else if app.db.overlaps(doc, date, start, end, Some(id)) {
            app.warn("Ese hueco está ocupado o bloqueado.");
        } else {
            if let Some(a) = app.db.appointments.iter_mut().find(|a| a.id == id) {
                a.date = date;
                a.start = start;
                a.doctor_id = doc;
            }
            app.changed();
            let moved = format!("Cita movida a {} {} con {}", fmt_date(date), hm(start), app.db.doctor_alias(doc));
            let m = app.db.appointments.iter().find(|a| a.id == id).cloned();
            match m.as_ref().and_then(|m| app.db.room_taken(m.box_id, m.center_id, m.date, m.start, m.end(), Some(id)).filter(|o| o.doctor_id != m.doctor_id)) {
                Some(o) => {
                    let why = room_text(&app.db, o);
                    app.warn(format!("{moved}. {why} Cambie el gabinete en la cita."));
                }
                None => app.notify(moved),
            }
        }
    }
    // clic en hueco vacío → nueva cita
    if bg_resp.clicked()
        && open_dialog.is_none()
        && let Some(pp) = bg_resp.interact_pointer_pos()
        && let Some(slot) = slot_at(pp, rect, time_w, col_w, head_h, ppm, open, &cols)
    {
        let (date, doc, t) = slot;
        let mut d = ApptDraft::new_at(app, date, Some((t, doc)), app.patient);
        d.a.patient_id = None;
        d.a.first_visit = false;
        app.dialog = Dialog::Appointment(d);
    }
    if let Some(a) = open_dialog {
        app.dialog = Dialog::Appointment(ApptDraft::from(&a));
    }
    if bg_resp.hovered()
        && let Some(pp) = bg_resp.hover_pos()
        && let Some((_, _, t)) = slot_at(pp, rect, time_w, col_w, head_h, ppm, open, &cols)
    {
        let ci = ((pp.x - rect.min.x - time_w) / col_w).floor();
        let x = rect.min.x + time_w + ci * col_w;
        let y = y_of(minutes_of(t));
        let r = Rect::from_min_size(pos2(x + 4.0, y), vec2(col_w - 8.0, 15.0 * ppm));
        p.rect_stroke(r, CornerRadius::same(4), Stroke::new(1.0, ACCENT), StrokeKind::Inside);
        p.text(r.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, format!("＋ {}", hm(t)), FontId::proportional(12.5), ACCENT);
    }
}

#[allow(clippy::too_many_arguments)]
fn slot_at(pp: Pos2, rect: Rect, time_w: f32, col_w: f32, head_h: f32, ppm: f32, open: i64, cols: &[Col]) -> Option<(NaiveDate, u32, NaiveTime)> {
    let ci = ((pp.x - rect.min.x - time_w) / col_w).floor();
    if ci < 0.0 || ci as usize >= cols.len() || pp.y < rect.min.y + head_h {
        return None;
    }
    let m = open + (((pp.y - rect.min.y - head_h) / ppm) as i64 / 15) * 15;
    let t = NaiveTime::from_hms_opt((m / 60) as u32, (m % 60) as u32, 0)?;
    let c = &cols[ci as usize];
    Some((c.date, c.doctor, t))
}

fn side(app: &mut App, ui: &mut Ui) {
    // citas pedidas por internet que esperan respuesta
    crate::screens::cita_online::agenda_card(app, ui);
    let date = app.agenda.date;
    // Resumen del día
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Resumen del día");
        let day: Vec<&Appointment> = app.db.appointments.iter().filter(|a| a.date == date && cid(a.center_id) == cid(app.db.config.current_center)).collect();
        let total = day.iter().filter(|a| !a.state.is_cancelled()).count();
        let done = day.iter().filter(|a| a.state == ApptState::Finalizada).count();
        let missed = day.iter().filter(|a| a.state == ApptState::NoAcude).count();
        ui.label(format!("{total} citas · {done} finalizadas · {missed} no acuden"));
        bar(ui, ui.available_width(), if total > 0 { done as f32 / total as f32 } else { 0.0 }, OK);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for st in [ApptState::Planificada, ApptState::Confirmada, ApptState::SalaEspera, ApptState::EnConsulta, ApptState::Finalizada] {
                let n = day.iter().filter(|a| a.state == st).count();
                pill(ui, &format!("{} {n}", st.label()), rgb(st.color()));
            }
        });
    });
    ui.add_space(8.0);
    // Sala de espera / en consulta
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Recepción");
        let mut list: Vec<Appointment> = app
            .db
            .appointments
            .iter()
            .filter(|a| a.date == date && cid(a.center_id) == cid(app.db.config.current_center) && matches!(a.state, ApptState::Planificada | ApptState::Confirmada | ApptState::Recepcion | ApptState::SalaEspera | ApptState::EnConsulta))
            .cloned()
            .collect();
        list.sort_by_key(|a| (a.state != ApptState::EnConsulta, a.state != ApptState::SalaEspera, a.start));
        if list.is_empty() {
            muted(ui, "No hay pacientes pendientes.");
        }
        egui::ScrollArea::vertical().id_salt("recep").max_height(250.0).show(ui, |ui| {
            for a in list.iter().take(12) {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, rgb(a.state.color()));
                    ui.vertical(|ui| {
                        ui.set_max_width(210.0);
                        ui.add(egui::Label::new(RichText::new(format!("{} {}", hm(a.start), app.db.appt_name(a))).size(14.5)).truncate());
                        muted(ui, format!("{} · {}", a.state.label(), app.db.doctor_alias(a.doctor_id)));
                    });
                    right_aligned(ui, |ui| {
                        if let Some(next) = a.state.next() {
                            let label = match next {
                                ApptState::SalaEspera => "Llega",
                                ApptState::EnConsulta => "Pasa",
                                _ => "Fin",
                            };
                            if ui.small_button(label).on_hover_text(format!("Cambiar a «{}»", next.label())).clicked() {
                                set_state(&mut app.db, a.id, next);
                                app.dirty = true;
                            }
                        }
                    });
                });
            }
        });
    });
    ui.add_space(8.0);
    // Búsqueda de huecos
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Buscar hueco libre");
        ui.horizontal(|ui| {
            let docs: Vec<(u32, String)> = app.db.collaborators.iter().filter(|c| c.active).map(|c| (c.id, c.alias.clone())).collect();
            crate::theme::combo("gap_doc").width(120.0).selected_text(app.db.doctor_alias(app.agenda.gap_doctor)).show_ui(ui, |ui| {
                for (id, n) in &docs {
                    ui.selectable_value(&mut app.agenda.gap_doctor, *id, n);
                }
            });
            ui.add(egui::DragValue::new(&mut app.agenda.gap_minutes).range(10..=240).speed(5).suffix(" min"));
        });
        ui.checkbox(&mut app.agenda.gap_afternoon, "Solo por la tarde");
        if ui.button("🔍 Buscar desde la fecha mostrada").clicked() {
            let (from, to) = if app.agenda.gap_afternoon { (NaiveTime::from_hms_opt(15, 30, 0).unwrap(), app.db.config.close_time) } else { (app.db.config.open_time, app.db.config.close_time) };
            let start = date.max(app.today);
            app.agenda.gaps = app.db.find_gaps(app.agenda.gap_doctor, start, app.agenda.gap_minutes, from, to, 21);
            if app.agenda.gaps.is_empty() {
                app.warn("No hay huecos en las próximas 3 semanas.");
            }
        }
        let mut pick = None;
        for (d, t) in app.agenda.gaps.iter().take(6) {
            if ui.link(format!("{} {} · {}", weekday_es(*d), fmt_date(*d), hm(*t))).clicked() {
                pick = Some((*d, *t));
            }
        }
        if let Some((d, t)) = pick {
            app.agenda.date = d;
            let mut dr = ApptDraft::new_at(app, d, Some((t, app.agenda.gap_doctor)), app.patient);
            dr.a.minutes = app.agenda.gap_minutes;
            app.dialog = Dialog::Appointment(dr);
        }
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Lista de espera");
        let list = app.db.waitlist.clone();
        if list.is_empty() {
            muted(ui, "Vacía.");
        }
        let mut remove = None;
        for w in &list {
            // nombre y botones en una línea; el motivo, debajo y a todo lo ancho (sin pisar los botones)
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(app.db.patient_name(w.patient_id)).size(14.5)).truncate());
                right_aligned(ui, |ui| {
                    if ui.small_button("✔").on_hover_text("Ya citado: quitar de la lista").clicked() {
                        remove = Some(w.id);
                    }
                    if ui.small_button("Citar").clicked() {
                        let mut dr = ApptDraft::new_at(app, app.agenda.date, None, Some(w.patient_id));
                        dr.a.doctor_id = w.doctor_id;
                        dr.a.reason = w.reason.clone();
                        app.dialog = Dialog::Appointment(dr);
                    }
                });
            });
            ui.label(RichText::new(format!("{} · {} · {}", w.reason, app.db.doctor_alias(w.doctor_id), w.preference)).size(12.5).color(MUTED()));
            ui.add_space(4.0);
        }
        if let Some(id) = remove {
            app.db.waitlist.retain(|w| w.id != id);
            app.changed();
        }
        if let Some(pid) = app.patient
            && ui.button(format!("＋ Añadir a {}", app.db.patient_name(pid))).clicked()
        {
            let id = app.db.new_id();
            let doc = app.db.patient(pid).map(|p| p.doctor_id).unwrap_or(1);
            app.db.waitlist.push(WaitItem { id, patient_id: pid, doctor_id: doc, reason: "Adelantar cita".into(), since: app.today, preference: "Cualquier hora".into() });
            app.changed();
        }
        // huecos que han quedado libres y los que ya se han ofrecido
        crate::screens::cita_online::offers_card(app, ui);
    });
}

pub fn appt_dialog(app: &mut App, ctx: &egui::Context, mut d: ApptDraft) -> Dialog {
    let mut open = true;
    let mut save = false;
    let mut delete = false;
    let mut resched = false;
    let mut to_patient = None;
    // primera visita: dar de alta su ficha, o usar la que ya hay con su teléfono
    let mut make_patient = false;
    let mut use_patient: Option<u32> = None;
    crate::theme::modal("appt_dialog").show(ctx, |ui| {
        ui.set_width(790.0);
        title(ui, if d.is_new { "Nueva cita" } else { "Editar cita" }, &long_date(d.a.date));
        // paciente
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Paciente");
            match d.a.patient_id {
                Some(pid) => {
                    let p = app.db.patient(pid).cloned();
                    ui.horizontal(|ui| {
                        if let Some(p) = &p {
                            ui.label(RichText::new(format!("{} · Nº {}", p.full_name(), p.history_no)).family(crate::theme::sb()));
                            muted(ui, format!("📞 {} · {}", p.mobile, app.db.tariff_name(p.tariff_id)));
                        }
                        right_aligned(ui, |ui| {
                            if ui.button("Cambiar").clicked() {
                                d.a.patient_id = None;
                            }
                            if ui.button("Abrir ficha").clicked() {
                                to_patient = Some(pid);
                            }
                        });
                    });
                    if let Some(p) = &p {
                        if !p.active {
                            ui.label(RichText::new("⚠ Este paciente está de baja.").color(DANGER));
                        }
                        if !p.alerts.is_empty() || !p.allergies.is_empty() {
                            ui.label(RichText::new(format!("⚠ {} {}", p.alerts, p.allergies)).color(DANGER));
                        }
                        if let Some(w) = app.db.pending_lab_for(pid) {
                            ui.label(RichText::new(format!("⚠ Trabajo de laboratorio sin recibir: {} (previsto {})", w.description, fmt_date(w.due))).color(WARN));
                        }
                        let bal = app.db.patient_balance(pid);
                        if bal > 0.005 {
                            ui.label(RichText::new(format!("Tiene {} pendiente de cobro", money(bal))).color(WARN));
                        }
                    }
                }
                None => {
                    ui.add_sized([400.0, 34.0], egui::TextEdit::singleline(&mut d.search).hint_text("Buscar paciente existente…"));
                    if !d.search.trim().is_empty() {
                        for pid in pacientes::search(&app.db, &d.search, 5) {
                            let n = app.db.patient(pid).map(|p| format!("{} · Nº {} · {}", p.full_name(), p.history_no, p.mobile)).unwrap_or_default();
                            if ui.selectable_label(false, n).clicked() {
                                d.a.patient_id = Some(pid);
                                d.a.first_visit = false;
                                d.search.clear();
                            }
                        }
                    }
                    ui.horizontal(|ui| {
                        field(ui, "…o paciente nuevo: nombre", &mut d.a.free_name, 240.0);
                        field(ui, "Teléfono", &mut d.a.phone, 140.0);
                    });
                    if !d.a.free_name.is_empty() {
                        d.a.first_visit = true;
                    }
                    // la cita ya guardada de alguien sin ficha (primera visita, pedida por internet…)
                    if !d.is_new && !d.a.free_name.trim().is_empty() {
                        match app.db.patient_with_phone(&d.a.phone) {
                            Some(pid) => {
                                let n = app.db.patient(pid).map(|p| format!("{} (Nº {})", p.full_name(), p.history_no)).unwrap_or_default();
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(format!("Ya hay una ficha con ese teléfono: {n}")).color(WARN));
                                    if ui.button("Usar esta ficha").clicked() {
                                        use_patient = Some(pid);
                                    }
                                });
                            }
                            None => {
                                if primary(ui, "＋ Dar de alta a este paciente").on_hover_text("Abre su ficha con los datos de la cita; al guardarla, sus citas quedan unidas a ella").clicked() {
                                    make_patient = true;
                                }
                            }
                        }
                    }
                }
            }
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Fecha").size(13.5).color(MUTED()));
                    crate::screens::date_picker(ui, "appt_date", &mut d.a.date);
                });
                ui.vertical(|ui| {
                    ui.label(RichText::new("Hora").size(13.5).color(MUTED()));
                    ui.horizontal(|ui| {
                        let mut h = d.a.start.hour();
                        let mut m = d.a.start.minute();
                        ui.add(egui::DragValue::new(&mut h).range(7..=21).custom_formatter(|v, _| format!("{:02}", v as u32)));
                        ui.label(":");
                        ui.add(egui::DragValue::new(&mut m).range(0..=55).speed(0.3).custom_formatter(|v, _| format!("{:02}", v as u32)));
                        let m = (m / 5) * 5;
                        d.a.start = NaiveTime::from_hms_opt(h, m, 0).unwrap_or(d.a.start);
                    });
                });
                ui.vertical(|ui| {
                    ui.label(RichText::new("Duración").size(13.5).color(MUTED()));
                    ui.add(egui::DragValue::new(&mut d.a.minutes).range(5..=480).speed(5).suffix(" min"));
                });
                ui.vertical(|ui| {
                    ui.label(RichText::new("Doctor").size(13.5).color(MUTED()));
                    crate::theme::combo("dlg_doc").width(150.0).selected_text(app.db.doctor_alias(d.a.doctor_id)).show_ui(ui, |ui| {
                        for c in app.db.collaborators.iter().filter(|c| c.active) {
                            ui.selectable_value(&mut d.a.doctor_id, c.id, &c.alias);
                        }
                    });
                });
                ui.vertical(|ui| {
                    ui.label(RichText::new("Gabinete").size(13.5).color(MUTED()));
                    crate::theme::combo("dlg_box").width(140.0).selected_text(app.db.box_name(d.a.box_id)).show_ui(ui, |ui| {
                        for b in app.db.boxes.iter().filter(|b| cid(b.center_id) == cid(app.db.config.current_center)) {
                            ui.selectable_value(&mut d.a.box_id, b.id, &b.name);
                        }
                    });
                });
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Tratamiento").size(13.5).color(MUTED()));
                    let before = d.a.treatment_code.clone();
                    let tariff = d.a.patient_id.and_then(|pid| app.db.patient(pid)).map(|p| p.tariff_id).unwrap_or(1);
                    crate::screens::treatment_picker(ui, "dlg_tto", &app.db, tariff, &mut d.a.treatment_code, 260.0, |t| t.minutes > 0, |t| format!("{} ({} min)", t.description, t.minutes));
                    if before != d.a.treatment_code
                        && let Some(t) = app.db.treatment(&d.a.treatment_code)
                    {
                        d.a.minutes = t.minutes;
                        d.a.reason = t.description.clone();
                    }
                });
                field(ui, "Motivo", &mut d.a.reason, 300.0);
            });
            ui.label(RichText::new("Situación").size(13.5).color(MUTED()));
            ui.horizontal_wrapped(|ui| {
                for st in ApptState::ALL {
                    let sel = d.a.state == st;
                    let c = rgb(st.color());
                    let b = egui::Button::new(RichText::new(st.label()).font(crate::theme::mdf(13.5)).color(if sel { Color32::WHITE } else { mix(c, HEADING, 0.25) }))
                        .fill(if sel { c } else { tint(c, 0.08) })
                        .stroke(egui::Stroke::new(1.0, if sel { c } else { tint(c, 0.28) }))
                        .corner_radius(CornerRadius::same(17))
                        .min_size(egui::vec2(0.0, 32.0));
                    if ui.add(b).clicked() {
                        d.a.state = st;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut d.a.reminder_sent, "Recordatorio enviado");
                ui.checkbox(&mut d.a.first_visit, "Primera visita");
                if d.is_new && d.from_id.is_none() {
                    ui.add_space(10.0);
                    ui.label("Repetir");
                    let names = ["No", "Cada semana", "Cada 2 semanas", "Cada mes"];
                    crate::theme::combo("appt_rep").selected_text(names[d.repeat as usize % 4]).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            ui.selectable_value(&mut d.repeat, i as u8, *n);
                        }
                    });
                    if d.repeat > 0 {
                        ui.add(egui::DragValue::new(&mut d.repeat_count).range(2..=52).suffix(" citas"));
                    }
                }
                if let Some(t) = d.a.arrived_at {
                    muted(ui, format!("Llegó {}", hm(t)));
                }
                if let Some(t) = d.a.seen_at {
                    muted(ui, format!("· Pasó {}", hm(t)));
                }
                if let Some(t) = d.a.finished_at {
                    muted(ui, format!("· Terminó {}", hm(t)));
                }
            });
            if !d.is_new
                && let Some(t) = crate::screens::whatsapp::appt_line(&app.db, d.a.id)
            {
                ui.label(RichText::new(t).color(crate::screens::whatsapp::WA_GREEN));
            }
            ui.label(RichText::new("Observaciones").size(13.5).color(MUTED()));
            ui.add(egui::TextEdit::multiline(&mut d.a.notes).desired_rows(2).desired_width(f32::INFINITY));
            // enlace personal de la cita (cita online activada)
            if !d.is_new && app.db.booking.enabled && app.db.booking.links {
                ui.add_space(6.0);
                crate::screens::cita_online::appt_link_row(app, ui, d.a.id);
            }
        });
        // avisos
        let end = d.a.end();
        if !d.a.state.is_cancelled() {
            // festivo, la clínica no abre o el profesional no pasa consulta (dice cuál)
            if let Some(why) = app.db.schedule_problem(d.a.doctor_id, d.a.date, d.a.start, end) {
                ui.label(RichText::new(format!("⚠ {why}")).color(WARN));
            }
            if app.db.overlaps(d.a.doctor_id, d.a.date, d.a.start, end, (!d.is_new).then_some(d.a.id)) {
                ui.label(RichText::new(format!("⚠ Se solapa con otra cita o un bloqueo de {}.", app.db.doctor_alias(d.a.doctor_id))).color(WARN));
            }
            // el sillón: dos doctores en el mismo gabinete a la vez
            if let Some(o) = app.db.room_taken(d.a.box_id, d.a.center_id, d.a.date, d.a.start, end, (!d.is_new).then_some(d.a.id)).filter(|o| o.doctor_id != d.a.doctor_id) {
                ui.label(RichText::new(format!("⚠ {}", room_text(&app.db, o))).color(WARN));
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let ok = d.a.patient_id.is_some() || !d.a.free_name.trim().is_empty();
            if ui.add_enabled(ok, egui::Button::new(RichText::new("Guardar cita").color(Color32::WHITE)).fill(ACCENT)).clicked() {
                save = true;
            }
            if ui.button("Cancelar").clicked() {
                open = false;
            }
            if !d.is_new {
                if ui.button("↷ Reprogramar").on_hover_text("Pasa la cita a otro día: esta queda como «Reprogramada»").clicked() {
                    resched = true;
                }
                right_aligned(ui, |ui| {
                    if danger(ui, "Borrar cita").clicked() {
                        delete = true;
                    }
                });
            }
            if !d.is_new && d.a.patient_id.is_some() && app.db.config.pay_online.deposit.amount > 0 && !d.a.state.is_cancelled() {
                crate::screens::pagos::pay_row(app, ui, crate::pago::PayFor::Deposit(d.a.id), "Señal por internet");
            }
            if let Some(f) = d.from_id {
                muted(ui, format!("Reprogramación de la cita del {}", app.db.appointments.iter().find(|a| a.id == f).map(|a| format!("{} {}", fmt_date(a.date), hm(a.start))).unwrap_or_default()));
            }
        });
    });
    if let Some(pid) = to_patient {
        app.open_patient(pid, PatTab::Resumen);
        return Dialog::None;
    }
    if let Some(pid) = use_patient {
        // con lo que se haya cambiado en la cita
        if let Some(a) = app.db.appointments.iter_mut().find(|a| a.id == d.a.id) {
            *a = d.a.clone();
        }
        let n = app.db.link_first_visit(d.a.id, pid);
        d.a.patient_id = Some(pid);
        d.a.free_name.clear();
        app.changed();
        app.notify(format!("{} unida a la ficha de {}.", if n == 1 { "La cita queda".to_string() } else { format!("Sus {n} citas quedan") }, app.db.patient_name(pid)));
        return Dialog::Appointment(d);
    }
    if make_patient {
        if let Some(a) = app.db.appointments.iter_mut().find(|a| a.id == d.a.id) {
            *a = d.a.clone();
        }
        app.changed();
        let mut p = pacientes::blank_patient(&app.db, app.today);
        if let Some(x) = app.db.patient_from_appt(d.a.id) {
            (p.first_name, p.last_name, p.mobile, p.email, p.doctor_id) = (x.first_name, x.last_name, x.mobile, x.email, x.doctor_id);
        }
        app.new_patient_for = Some(d.a.id);
        return Dialog::NewPatient(Box::new(p));
    }
    if resched && let Some(n) = app.db.reschedule_draft(d.a.id) {
        app.agenda.date = n.date;
        return Dialog::Appointment(ApptDraft { a: n, is_new: true, search: String::new(), repeat: 0, repeat_count: 4, from_id: Some(d.a.id) });
    }
    if delete {
        return Dialog::Confirm { text: "¿Borrar definitivamente esta cita? (Si el paciente no viene, use mejor la situación «No acude».)".into(), action: ConfirmAction::DeleteAppointment(d.a.id) };
    }
    if save {
        let st = d.a.state;
        if d.is_new {
            d.a.id = app.db.new_id();
            app.db.appointments.push(d.a.clone());
            if let Some(f) = d.from_id
                && let Some(o) = app.db.appointments.iter_mut().find(|o| o.id == f)
            {
                o.state = ApptState::Reprogramada;
            }
            let rep = match d.repeat {
                1 => Some(crate::agenda_extra::Repeat::Weekly(1)),
                2 => Some(crate::agenda_extra::Repeat::Weekly(2)),
                3 => Some(crate::agenda_extra::Repeat::Monthly),
                _ => None,
            };
            if let Some(rep) = rep {
                let r = app.db.create_series(d.a.id, rep, d.repeat_count);
                let mut msg = format!("Serie creada: {} citas desde el {}", r.created.len() + 1, fmt_date(d.a.date));
                if !r.skipped.is_empty() {
                    let s: Vec<String> = r.skipped.iter().map(|(dt, why)| format!("{} ({why})", fmt_date(*dt))).collect();
                    msg.push_str(&format!(" · sin hueco: {}", s.join(", ")));
                }
                app.notify(msg);
            } else {
                app.notify(format!("Cita creada: {} {} con {}", fmt_date(d.a.date), hm(d.a.start), app.db.doctor_alias(d.a.doctor_id)));
            }
            app.measure_done(crate::medidas::Kind::Cita);
        } else if let Some(a) = app.db.appointments.iter_mut().find(|a| a.id == d.a.id) {
            let prev = a.state;
            *a = d.a.clone();
            if prev != st {
                set_state(&mut app.db, d.a.id, st);
            }
            app.notify("Cita actualizada.");
            // señal pagada: anulada en la agenda, se devuelve; si no acude, se conserva
            if prev != st
                && st.is_cancelled()
                && let Some(m) = app.db.deposit_on_cancel(d.a.id, st == ApptState::NoAcude, crate::model::now())
            {
                app.notify(m);
            }
        }
        // el recall pendiente del paciente queda citado con esta cita (y, si se
        // anula o se aplaza, vuelve a pendiente)
        app.db.refresh_recalls(app.today);
        app.agenda.date = d.a.date;
        app.changed();
        return app.dialog_take_or_none();
    }
    if open { Dialog::Appointment(d) } else { app.dialog_take_or_none() }
}

/// «El gabinete Gabinete 1 lo ocupa la Dra. Ruiz de 10:00 a 11:00 (Ana García).»
fn room_text(db: &Db, o: &Appointment) -> String {
    let room = db.boxes.iter().find(|b| b.id == o.box_id).map(|b| b.name.clone()).unwrap_or_else(|| format!("nº {}", o.box_id));
    let who = o.patient_id.map(|p| db.patient_name(p)).unwrap_or_else(|| o.free_name.clone());
    format!("El gabinete «{room}» ya lo ocupa {} de {} a {} ({who}).", db.doctor_alias(o.doctor_id), hm(o.start), hm(o.end()))
}

fn time_combo(ui: &mut Ui, id: &str, t: &mut NaiveTime, open: NaiveTime, close: NaiveTime) {
    crate::theme::combo(id).width(80.0).selected_text(hm(*t)).show_ui(ui, |ui| {
        let mut x = open;
        while x <= close {
            ui.selectable_value(t, x, hm(x));
            x += Duration::minutes(15);
        }
    });
}

/// Ventana para gestionar bloqueos de agenda (por doctor) y festivos.
fn blocks_dialog(app: &mut App, ctx: &egui::Context) {
    let mut close = false;
    let (open_t, close_t) = (app.db.config.open_time, app.db.config.close_time);
    crate::theme::modal("blocks_dialog").show(ctx, |ui| {
        ui.set_width(880.0);
        title(ui, "Festivos y bloqueos", "Días sin citas y franjas reservadas de cada doctor");
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            section(ui, "Bloqueos de agenda");
            let docs: Vec<(u32, String)> = app.db.collaborators.iter().filter(|c| c.active).map(|c| (c.id, c.alias.clone())).collect();
            let mut del = None;
            egui::ScrollArea::vertical().id_salt("blk_list").max_height(220.0).show(ui, |ui| {
                for b in &app.db.blocks {
                    ui.horizontal(|ui| {
                        let when = b.date.map(fmt_date).unwrap_or_else(|| "Todos los días".into());
                        if ui.small_button("🗑").on_hover_text("Quitar bloqueo").clicked() {
                            del = Some(b.id);
                        }
                        ui.add(egui::Label::new(format!("{} · {} · {}–{} · {}", app.db.doctor_alias(b.doctor_id), when, hm(b.start), hm(b.end), b.reason)).truncate());
                    });
                }
                if app.db.blocks.is_empty() {
                    muted(ui, "Sin bloqueos.");
                }
            });
            if let Some(id) = del {
                app.db.blocks.retain(|b| b.id != id);
                app.changed();
            }
            ui.separator();
            let s = &mut app.agenda;
            ui.horizontal(|ui| {
                ui.label("Doctor");
                crate::theme::combo("blk_doc").selected_text(docs.iter().find(|d| d.0 == s.blk_doctor).map(|d| d.1.clone()).unwrap_or_default()).show_ui(ui, |ui| {
                    for (id, n) in &docs {
                        ui.selectable_value(&mut s.blk_doctor, *id, n);
                    }
                });
                ui.checkbox(&mut s.blk_every, "Todos los días");
            });
            if !s.blk_every {
                ui.horizontal(|ui| {
                    ui.label("Día");
                    crate::screens::date_picker(ui, "blk_date", &mut s.blk_date);
                });
            }
            ui.horizontal(|ui| {
                ui.label("De");
                time_combo(ui, "blk_s", &mut s.blk_start, open_t, close_t);
                ui.label("a");
                time_combo(ui, "blk_e", &mut s.blk_end, open_t, close_t);
            });
            ui.add(egui::TextEdit::singleline(&mut s.blk_reason).hint_text("Motivo (formación, comida, quirófano…)").desired_width(f32::INFINITY));
            if ui.add(egui::Button::new(RichText::new("＋ Añadir bloqueo").color(Color32::WHITE)).fill(ACCENT)).clicked() {
                if s.blk_end <= s.blk_start {
                    app.warn("La hora final debe ser posterior a la inicial.");
                } else {
                    let b = Block { id: 0, doctor_id: s.blk_doctor, date: (!s.blk_every).then_some(s.blk_date), start: s.blk_start, end: s.blk_end, reason: s.blk_reason.trim().to_string() };
                    let id = app.db.new_id();
                    app.db.blocks.push(Block { id, ..b });
                    app.changed();
                    app.notify("Bloqueo añadido.");
                }
            }

            let ui = &mut cols[1];
            section(ui, "Festivos");
            let mut hdel = None;
            let mut hs: Vec<(usize, NaiveDate, String)> = app.db.holidays.iter().enumerate().map(|(i, h)| (i, h.date, h.name.clone())).collect();
            hs.sort_by_key(|h| h.1);
            egui::ScrollArea::vertical().id_salt("hol_list").max_height(220.0).show(ui, |ui| {
                for (i, d, n) in &hs {
                    ui.horizontal(|ui| {
                        if ui.small_button("🗑").on_hover_text("Quitar festivo").clicked() {
                            hdel = Some(*i);
                        }
                        ui.add(egui::Label::new(format!("{} · {}", fmt_date(*d), n)).truncate());
                    });
                }
                if hs.is_empty() {
                    muted(ui, "Sin festivos.");
                }
            });
            if let Some(i) = hdel {
                app.db.holidays.remove(i);
                app.changed();
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Día");
                crate::screens::date_picker(ui, "hol_date", &mut app.agenda.hol_date);
            });
            ui.add(egui::TextEdit::singleline(&mut app.agenda.hol_name).hint_text("Nombre del festivo").desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if primary(ui, "＋ Añadir festivo").clicked() {
                    let date = app.agenda.hol_date;
                    let busy = app.db.appointments.iter().filter(|a| a.date == date && !a.state.is_cancelled()).count();
                    app.db.holidays.retain(|h| h.date != date);
                    app.db.holidays.push(crate::agenda_extra::Holiday { date, name: app.agenda.hol_name.trim().to_string() });
                    app.changed();
                    if busy > 0 {
                        app.warn(format!("Festivo añadido, pero ese día ya hay {busy} citas: conviene reprogramarlas."));
                    } else {
                        app.notify("Festivo añadido.");
                    }
                }
                let year = app.agenda.hol_date.year();
                if ui.button(format!("Nacionales {year}")).on_hover_text("Añade los festivos nacionales que falten de ese año").clicked() {
                    let mut n = 0;
                    for h in crate::agenda_extra::default_holidays(year) {
                        if app.db.holiday(h.date).is_none() {
                            app.db.holidays.push(h);
                            n += 1;
                        }
                    }
                    app.changed();
                    app.notify(format!("{n} festivos añadidos."));
                }
            });
        });
        ui.add_space(8.0);
        if ui.button("Cerrar").clicked() {
            close = true;
        }
    });
    if close {
        app.agenda.blocks_open = false;
    }
}
