//! The schematic canvas's keys, each running a Command: Delete, Mod+X/C/V (also the OS
//! cut, copy and paste events), Mod+D duplicate, Mod+A select all, M Move, D Drag,
//! G cycle the grid, and Escape back to Select.
//!
//! The bindings follow the v0.1 default scheme (docs/spec/v0.1/app.md#keymap). The Keymap
//! (#94) replaces this table with its data file; the Commands stay the same.

use egui::{Context, Event, Key, Modifiers};
use reda_engine::model::Uuid;
use serde_json::json;

use super::{Invocation, cancel, selectable};
use crate::canvas::interact::snap_to_grid;
use crate::shell::Shell;
use crate::tools::Tool;

/// What a key on the canvas asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Delete,
    Cut,
    Copy,
    Paste,
    Duplicate,
    SelectAll,
    Move,
    Drag,
    CycleGrid,
    Escape,
}

/// Takes this frame's canvas keys from `ctx`, unless a text field has the keyboard.
pub fn take(ctx: &Context) -> Vec<KeyAction> {
    if ctx.egui_wants_keyboard_input() {
        return Vec::new();
    }
    ctx.input_mut(|i| {
        let mut out = Vec::new();
        // The OS turns Mod+X/C/V into clipboard events before they are keys.
        for event in &i.events {
            match event {
                Event::Cut => out.push(KeyAction::Cut),
                Event::Copy => out.push(KeyAction::Copy),
                Event::Paste(_) => out.push(KeyAction::Paste),
                _ => {}
            }
        }
        let table = [
            (Modifiers::COMMAND, Key::X, KeyAction::Cut),
            (Modifiers::COMMAND, Key::C, KeyAction::Copy),
            (Modifiers::COMMAND, Key::V, KeyAction::Paste),
            (Modifiers::COMMAND, Key::D, KeyAction::Duplicate),
            (Modifiers::COMMAND, Key::A, KeyAction::SelectAll),
            (Modifiers::NONE, Key::Delete, KeyAction::Delete),
            (Modifiers::NONE, Key::M, KeyAction::Move),
            (Modifiers::NONE, Key::D, KeyAction::Drag),
            (Modifiers::NONE, Key::G, KeyAction::CycleGrid),
            (Modifiers::NONE, Key::Escape, KeyAction::Escape),
        ];
        for (modifiers, key, action) in table {
            if i.consume_key(modifiers, key) && !out.contains(&action) {
                out.push(action);
            }
        }
        out
    })
}

/// The Commands `actions` run on `document`'s canvas.
pub fn run(shell: &mut Shell, document: Uuid, actions: &[KeyAction]) -> Vec<Invocation> {
    let mut out = Vec::new();
    for &action in actions {
        let selected: Vec<Uuid> = {
            let on_sheet = shell
                .engine()
                .project()
                .and_then(|p| p.schematic_sheet(document))
                .map(selectable)
                .unwrap_or_default();
            shell
                .selection()
                .objects
                .iter()
                .copied()
                .filter(|o| on_sheet.contains(o))
                .collect()
        };
        let objects = json!({ "objects": selected });
        let some = !selected.is_empty();
        let command = match action {
            KeyAction::Delete if some => Some(("schematic.delete", objects)),
            KeyAction::Cut if some => Some(("schematic.cut", objects)),
            KeyAction::Copy if some => Some(("schematic.copy", objects)),
            KeyAction::Duplicate if some => Some(("schematic.duplicate", objects)),
            KeyAction::Paste => shell.clipboard().map(|clipboard| {
                let mut params = json!({"document": document, "clipboard": clipboard});
                if let Some(at) = shell.document_tab(document).and_then(|t| t.cursor) {
                    let at = snap_to_grid(at, shell.grid(document));
                    params["at"] = json!({"x": at.x.0, "y": at.y.0});
                }
                ("schematic.paste", params)
            }),
            KeyAction::SelectAll => {
                let mut all: Vec<Uuid> = shell
                    .engine()
                    .project()
                    .and_then(|p| p.schematic_sheet(document))
                    .map(selectable)
                    .unwrap_or_default()
                    .into_iter()
                    .collect();
                all.sort();
                Some(("selection.set", json!({ "objects": all })))
            }
            KeyAction::Move => Some(("tool.activate:move", json!({}))),
            KeyAction::Drag => Some(("tool.activate:drag", json!({}))),
            KeyAction::CycleGrid => Some(("view.grid.cycle", json!({ "document": document }))),
            KeyAction::Escape => {
                cancel(shell, document);
                (shell.tool() != Tool::Select).then(|| ("tool.activate:select", json!({})))
            }
            _ => None,
        };
        out.extend(command.map(|(id, params)| (id.to_owned(), params)));
    }
    out
}
