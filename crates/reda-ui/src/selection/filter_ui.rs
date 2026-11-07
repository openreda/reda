//! Both Selection Filter surfaces read the same per-Document working state.

use crate::{shell::Shell, working_state::filter_kinds};
use reda_engine::model::Uuid;
use serde_json::{Value, json};

pub fn controls(ui: &mut egui::Ui, shell: &Shell, document: Uuid) -> Vec<(&'static str, Value)> {
    let Some(ty) = crate::bars::document_type(shell, document) else {
        return Vec::new();
    };
    let filter = shell.selection_filter(document);
    let mut commands = Vec::new();
    ui.horizontal(|ui| {
        for (label, on) in [
            (crate::tr!("selection-filter-all"), true),
            (crate::tr!("selection-filter-none"), false),
        ] {
            if ui.button(label).clicked() {
                commands.push((
                    "selection.filter.set",
                    json!({"document":document,"all":on}),
                ));
            }
        }
    });
    for kind in filter_kinds(ty) {
        let mut on = filter.allows(kind);
        if ui
            .checkbox(
                &mut on,
                crate::tr!("selection-filter-kind", kind = kind_label(kind)),
            )
            .changed()
        {
            commands.push((
                "selection.filter.set",
                json!({"document":document,"kinds":{*kind:on}}),
            ));
        }
    }
    commands
}

pub fn properties(ui: &mut egui::Ui, shell: &Shell) -> Vec<(&'static str, Value)> {
    let Some(document) = shell.active_document().map(|d| d.document) else {
        return Vec::new();
    };
    if crate::bars::document_type(shell, document).is_none() {
        return Vec::new();
    }
    ui.strong(crate::tr!("selection-filter-title"));
    controls(ui, shell, document)
}

/// The Active Bar owns this square button, so its responsive overflow keeps working.
pub fn button(ui: &mut egui::Ui, shell: &Shell, document: Uuid) -> Option<(String, Value)> {
    let title = crate::tr!("selection-filter-title");
    let (_, rect) = ui.allocate_space(egui::Vec2::splat(crate::chrome::BAR_BUTTON));
    let response = ui.interact(
        rect,
        ui.scope_id().with(("selection-filter-button", document)),
        egui::Sense::click(),
    );
    let c = rect.center();
    let color = if response.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    ui.painter().add(egui::Shape::line(
        vec![
            c + egui::vec2(-8.0, -7.0),
            c + egui::vec2(8.0, -7.0),
            c + egui::vec2(2.0, 0.0),
            c + egui::vec2(2.0, 7.0),
            c + egui::vec2(-2.0, 7.0),
            c + egui::vec2(-2.0, 0.0),
            c + egui::vec2(-8.0, -7.0),
        ],
        egui::Stroke::new(1.5, color),
    ));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &title));
    let response = response.on_hover_text(title);
    let mut command = None;
    egui::Popup::menu(&response).show(|ui| {
        command = controls(ui, shell, document)
            .into_iter()
            .next()
            .map(|(id, p)| (id.into(), p));
    });
    command
}

pub fn kind_label(kind: &str) -> String {
    crate::i18n::translate(&format!("selection-kind-{}", kind.replace('_', "-")), &[])
}
