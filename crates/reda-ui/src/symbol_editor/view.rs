//! The Symbol editor's strip over the shared canvas: the Unit tabs and the Text Tool's
//! typing. The Pins are edited in the Pins Panel ([`crate::panels::pins`]). Nothing here
//! runs a Command: edits come back as Invocations for the window to run after the frame.

use egui::{Id, Role, Ui, WidgetInfo};
use reda_engine::model::Uuid;
use serde_json::json;

use super::{canvas, commit_text, set_text, shown_unit, symbol_of};
use crate::element_editor::{self, Invocation};
use crate::shell::Shell;
use crate::tr;

/// Draws the panels of `symbol`'s tab into `ui`, leaving the rest for the canvas.
pub(crate) fn panels(ui: &mut Ui, shell: &mut Shell, symbol: Uuid) -> Vec<Invocation> {
    let mut out = Vec::new();
    egui::Panel::top(Id::NULL.with(("symbol-units", symbol)))
        .resizable(false)
        .show(ui, |ui| unit_tabs(ui, shell, symbol, &mut out));
    out
}

fn unit_tabs(ui: &mut Ui, shell: &mut Shell, symbol: Uuid, out: &mut Vec<Invocation>) {
    let Some(s) = symbol_of(shell.engine(), symbol).cloned() else {
        return;
    };
    let shown = shown_unit(shell, symbol);
    let locked = element_editor::read_only(shell.engine(), symbol);
    ui.horizontal(|ui| {
        for unit in &s.units {
            let label = tr!("symbol-unit-tab", unit = unit.name.as_str());
            if ui
                .selectable_label(shown == Some(unit.uuid), label)
                .clicked()
            {
                out.push((
                    "library.symbol.unit.show".to_owned(),
                    json!({"symbol": symbol, "unit": unit.uuid}),
                ));
            }
        }
        let add = ui.add_enabled(!locked, egui::Button::new(tr!("symbol-unit-add")));
        if add.clicked() {
            out.push((
                "library.symbol.unit.add".to_owned(),
                json!({ "symbol": symbol }),
            ));
        }
        if locked {
            ui.weak(tr!("symbol-read-only"));
        }
        text_entry(ui, shell, symbol, out);
    });
}

/// The Text Tool's typing: Enter places the text, Escape or leaving the field drops it.
fn text_entry(ui: &mut Ui, shell: &mut Shell, symbol: Uuid, out: &mut Vec<Invocation>) {
    let Some((at, mut text)) = canvas(shell, symbol).text else {
        return;
    };
    ui.separator();
    let edit = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(Id::NULL.with(("symbol-text", symbol)))
            .hint_text(tr!("symbol-text-hint"))
            .desired_width(200.0),
    );
    edit.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, tr!("symbol-text-hint")));
    if !edit.has_focus() && !edit.lost_focus() {
        edit.request_focus();
    }
    if edit.lost_focus() {
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            set_text(shell, symbol, Some((at, text.clone())));
            out.extend(commit_text(shell, symbol, &text));
        } else {
            set_text(shell, symbol, None);
        }
    } else {
        set_text(shell, symbol, Some((at, text)));
    }
}
