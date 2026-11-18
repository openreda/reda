//! Both Selection Filter surfaces read the same per-Document working state.

use crate::shell::Shell;
use crate::working_state::{FilterKind, filter_kinds};
use reda_engine::model::Uuid;
use serde_json::{Value, json};

use crate::widgets::{self, Glyph, Tooltip as _};

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
            if ui.add(widgets::button(label)).clicked() {
                commands.push((
                    "selection.filter.set",
                    json!({"document":document,"all":on}),
                ));
            }
        }
    });
    for &kind in filter_kinds(ty) {
        let mut on = filter.allows(kind);
        let label = crate::tr!("selection-filter-kind", kind = kind_label(kind));
        if ui.add(widgets::checkbox(&mut on, label)).changed() {
            commands.push((
                "selection.filter.set",
                json!({"document":document,"kinds":{kind.name():on}}),
            ));
        }
    }
    commands
}

/// The Active Bar owns this square button, so its responsive overflow keeps working.
pub fn button(ui: &mut egui::Ui, shell: &Shell, document: Uuid) -> Option<(String, Value)> {
    let title = crate::tr!("selection-filter-title");
    let button = widgets::glyph_button(Glyph::Funnel);
    let response = ui.push_id(("selection-filter-button", document), |ui| ui.add(button));
    let response = response.inner;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &title));
    let response = response.tooltip(title);
    let mut command = None;
    widgets::menu(&response).show(|ui| {
        command = controls(ui, shell, document)
            .into_iter()
            .next()
            .map(|(id, p)| (id.into(), p));
    });
    command
}

pub fn kind_label(kind: FilterKind) -> String {
    crate::i18n::translate(
        &format!("selection-kind-{}", kind.name().replace('_', "-")),
        &[],
    )
}
