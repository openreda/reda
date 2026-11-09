//! What a schematic canvas draws over its Scene in egui: the hover tooltip of a Wire (its
//! Net name, and the board name when Append Sheet Numbers changes it).

use egui::{Rect, Ui};
use reda_engine::model::Uuid;
use reda_engine::schematic::connectivity::{net_at, project_nets};

use super::{Interaction, point};
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

/// Draws the overlays of `document`'s canvas in `rect`.
pub fn decorate(ui: &Ui, shell: &Shell, document: Uuid, rect: Rect) {
    let pointer = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p));
    if let (Some(at), Some(tip)) = (pointer, hover_tip(shell, document)) {
        chip(ui, egui::Id::NULL.with(("wire-tip", document)), at, &tip);
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
