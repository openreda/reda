//! One frame of primary-button pointer input on a canvas, as events in Document
//! coordinates. Right- and middle-button drags are left to the camera (pan).

use egui::{Modifiers, PointerButton, Rect, Response, Ui};

use crate::canvas::Camera;

/// A Document point in nm, y up.
type P = (f64, f64);

/// What the pointer did on a canvas this frame, for the active Tool.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pointer {
    /// The pointer is over the canvas with no button held.
    Hover(P),
    /// A primary click without movement, with the modifiers held.
    Click { at: P, modifiers: Modifiers },
    /// A primary drag began at `from` (where the button went down) and is now at `at`.
    DragStart {
        from: P,
        at: P,
        modifiers: Modifiers,
    },
    /// A primary drag moved on.
    Drag(P),
    /// The primary button was released, ending a drag.
    DragEnd(P),
}

/// The events of this frame's input on the canvas `response` laid out in `rect`, viewed
/// through `camera`.
pub fn pointer(ui: &Ui, response: &Response, camera: &Camera, rect: Rect) -> Vec<Pointer> {
    let doc = |p: egui::Pos2| camera.document_point(rect, p);
    let (modifiers, origin, latest) = ui.input(|i| {
        (
            i.modifiers,
            i.pointer.press_origin(),
            i.pointer.latest_pos(),
        )
    });
    let mut out = Vec::new();
    let Some(at) = latest.or(response.hover_pos()).map(doc) else {
        return out;
    };
    if response.drag_started_by(PointerButton::Primary) {
        out.push(Pointer::DragStart {
            from: origin.map_or(at, doc),
            at,
            modifiers,
        });
    } else if response.drag_stopped_by(PointerButton::Primary) {
        out.push(Pointer::DragEnd(at));
    } else if response.dragged_by(PointerButton::Primary) {
        out.push(Pointer::Drag(at));
    } else if response.clicked_by(PointerButton::Primary) {
        out.push(Pointer::Click { at, modifiers });
    } else if response.hovered() {
        out.push(Pointer::Hover(at));
    }
    out
}
