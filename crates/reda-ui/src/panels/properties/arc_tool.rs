//! The arc Tool's options in Properties: how it places (docs/spec/v0.1/pcb.md#text-and-graphics),
//! kept in the `pcb.arc.mode` Preference.

use egui::{Role, Ui, WidgetInfo};
use reda_engine::preferences::ArcToolMode;
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::tools::Tool;
use crate::tr;

/// The options while the arc Tool runs on the board; `None` otherwise.
pub(super) fn tool_options(ui: &mut Ui, shell: &Shell) -> Option<Vec<(&'static str, Value)>> {
    if shell.tool() != Tool::Arc || !shell.board_is_active() {
        return None;
    }
    let mut out = Vec::new();
    ui.label(tr!(
        "properties-tool-options",
        tool = tr!("properties-tool-arc")
    ));
    ui.separator();
    let mode = shell.preferences().pcb.arc.mode;
    ui.label(tr!("properties-arc-mode"));
    for (m, title) in [
        (ArcToolMode::Edge, tr!("properties-arc-edge")),
        (ArcToolMode::Center, tr!("properties-arc-center")),
        (ArcToolMode::Circle, tr!("properties-arc-circle")),
    ] {
        let response = ui.radio(mode == m, &title);
        response.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, mode == m, &title));
        if response.clicked() && mode != m {
            out.push((
                "app.preferences.set",
                json!({"key": "pcb.arc.mode", "value": m}),
            ));
        }
    }
    Some(out)
}
