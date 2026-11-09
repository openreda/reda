//! What a schematic canvas draws over its Scene in egui: the hover tooltip of a Wire (its
//! Net name, and the board name when Append Sheet Numbers changes it), the in-place
//! edit's text field and the "Paused" chip of a placement paused by Tab.

use egui::{Id, Key, Modifiers, Rect, Role, TextEdit, Ui, WidgetInfo};
use reda_engine::model::Uuid;
use reda_engine::model::geom::Point;
use reda_engine::schematic::connectivity::{net_at, project_nets};

use super::{Interaction, Invocation, in_place, point};
use crate::shell::Shell;
use crate::tr;

/// The tooltip for the Wire under the pointer on `document`'s canvas: its Net's name,
/// with the board name when that differs. `None` when the pointer is over no Wire or
/// something is in flight.
pub fn hover_tip(shell: &Shell, document: Uuid) -> Option<String> {
    let tab = shell.document_tab(document)?;
    if !matches!(tab.interaction, Interaction::Idle) {
        return None;
    }
    let (wire, cursor) = (tab.live.hovered?, tab.cursor?);
    let project = shell.engine().project()?;
    project.schematic_sheet(document)?.wire(wire)?;
    let net = net_at(project, document, wire, point(cursor))?;
    let name = net.name?;
    let board = project_nets(project)
        .into_iter()
        .find(|n| n.objects.contains(&wire))
        .and_then(|n| n.board_name)
        .filter(|b| *b != name);
    Some(match board {
        Some(board) => tr!("hover-net-board", net = name, board = board),
        None => tr!("hover-net", net = name),
    })
}

/// Draws the overlays of `document`'s canvas in `rect`; returns the Commands an in-place
/// edit commits.
pub fn decorate(ui: &Ui, shell: &mut Shell, document: Uuid, rect: Rect) -> Vec<Invocation> {
    let pointer = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p));
    if let (Some(at), Some(tip)) = (pointer, hover_tip(shell, document)) {
        chip(ui, Id::NULL.with(("wire-tip", document)), at, &tip);
    }
    let screen = |shell: &Shell, at: Point| {
        let camera = shell.document_tab(document)?.camera?;
        Some(camera.screen_point(rect, (at.x.0 as f64, at.y.0 as f64)))
    };
    if let Some(edit) = in_place::editing(shell, document).cloned() {
        let pos = screen(shell, edit.at).unwrap_or(rect.center());
        let label = crate::tr!("in-place-edit");
        let field = Field {
            text: edit.text,
            multiline: edit.multiline,
            error: edit.error,
        };
        return text_field(ui, shell, document, pos, &label, field);
    }
    if let Some((name, at)) = in_place::paused(shell, document) {
        let name = name.to_owned();
        let pos = screen(shell, at).unwrap_or(rect.center());
        let paused = crate::tr!("in-place-paused");
        chip(
            ui,
            Id::NULL.with(("paused", document)),
            rect.left_top(),
            &paused,
        );
        let label = crate::tr!("in-place-next-name");
        let field = Field {
            text: name,
            multiline: false,
            error: None,
        };
        return text_field(ui, shell, document, pos, &label, field);
    }
    Vec::new()
}

struct Field {
    text: String,
    multiline: bool,
    error: Option<String>,
}

/// The in-place text field at `pos`: Enter commits (Shift+Enter adds a line in a
/// multi-line one), Esc cancels.
fn text_field(
    ui: &Ui,
    shell: &mut Shell,
    document: Uuid,
    pos: egui::Pos2,
    label: &str,
    field: Field,
) -> Vec<Invocation> {
    let id = Id::NULL.with(("in-place", document));
    let mut text = field.text.clone();
    let (mut enter, mut escape) = (false, false);
    egui::Area::new(id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let focused = ui.memory(|m| m.has_focus(id));
                if focused {
                    escape = ui.input(|i| i.key_pressed(Key::Escape));
                    if field.multiline {
                        enter = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
                    }
                }
                let edit = if field.multiline {
                    TextEdit::multiline(&mut text).desired_rows(2)
                } else {
                    TextEdit::singleline(&mut text)
                };
                let response = ui.add(edit.id(id).desired_width(180.0));
                response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label));
                if !field.multiline
                    && response.lost_focus()
                    && ui.input(|i| i.key_pressed(Key::Enter))
                {
                    enter = true;
                }
                if !enter && !escape && !response.has_focus() {
                    response.request_focus();
                }
                if let Some(error) = &field.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });
        });
    if text != field.text {
        in_place::set_text(shell, document, text);
    }
    if escape {
        in_place::cancel_edit(shell, document);
        Vec::new()
    } else if enter {
        in_place::commit(shell, document)
    } else {
        Vec::new()
    }
}

/// A small popup by `at` holding `text`, labelled with it for AccessKit.
pub(super) fn chip(ui: &Ui, id: egui::Id, at: egui::Pos2, text: &str) {
    egui::Area::new(id)
        .order(egui::Order::Tooltip)
        .fixed_pos(at + egui::vec2(16.0, 16.0))
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let response = ui.label(text);
                response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Tooltip, true, text));
            });
        });
}
