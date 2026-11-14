//! Encuestas de satisfacción en las pantallas (ver `crate::encuestas`): los
//! resultados en el cuadro de mando y la configuración.

use crate::app::App;
use crate::theme::*;
use egui::{RichText, Ui};

fn pct(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.0}")).unwrap_or_else(|| "—".into())
}

/// Resultados del periodo, en total y por profesional, con cómo se calculan.
pub fn results(app: &mut App, ui: &mut Ui, from: chrono::NaiveDate, to: chrono::NaiveDate) {
    let (t, by) = app.db.survey_stats(from, to);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Satisfacción (encuestas)");
        if t.sent == 0 {
            muted(ui, "No se enviaron encuestas en el periodo. Se activan en Comunicaciones (plantilla «Encuesta de satisfacción»).");
            return;
        }
        ui.label(RichText::new(format!("NPS {} · nota media {} · {} respondidas de {} enviadas ({} %)", pct(t.nps()), t.mean().map(|m| format!("{m:.1}")).unwrap_or_else(|| "—".into()), t.answered, t.sent, pct(t.response_pct()))).font(sbf(14.0)));
        muted(ui, format!("NPS = % de promotores (9-10: {}) − % de detractores (0-6: {}) de las respondidas; pasivos (7-8): {}.", t.promoters, t.detractors, t.passives));
        for (d, s) in by {
            ui.label(format!("{} · NPS {} · {} de {} respondidas", app.db.doctor_alias(d), pct(s.nps()), s.answered, s.sent));
        }
    });
}

/// Configuración: enlace de la reseña y días entre encuestas.
pub fn settings(app: &mut App, ui: &mut Ui) {
    let before = app.db.config.surveys.clone();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        section(ui, "Encuestas de satisfacción y reseñas");
        muted(
            ui,
            "La encuesta se envía con la plantilla «Encuesta de satisfacción» de Comunicaciones, a quien lo acepta, una vez por visita. El enlace a la reseña pública se ofrece a todos, sea cual sea su nota: pedirlo solo a quien está contento lo prohíbe la política de Google.",
        );
        let c = &mut app.db.config.surveys;
        ui.horizontal_wrapped(|ui| {
            ui.label("Enlace para dejar una reseña");
            ui.add(egui::TextEdit::singleline(&mut c.review_url).hint_text("https://g.page/r/…/review").desired_width(340.0));
        });
        ui.horizontal(|ui| {
            ui.label("Días sin otra encuesta al mismo paciente");
            ui.add(egui::DragValue::new(&mut c.min_days).range(7..=365));
        });
        if !c.review_url.trim().is_empty() && !c.review_url.trim().starts_with("https://") {
            ui.label(RichText::new("El enlace tiene que empezar por https://").color(WARN));
        }
    });
    if app.db.config.surveys != before {
        app.changed();
    }
}
