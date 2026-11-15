//! What a window REDA decorates itself does with its bar: the bar's clicks become window
//! commands (docs/spec/v0.1/app.md#title-bar, ADR-0014).
//!
//! Used by the main window and by each floating window, inside its own viewport, so
//! every command goes to the window the bar is in.

use egui::{PointerButton, Pos2, Response};

use super::Chrome;
use crate::widgets::WindowAction;

/// Carries out a frame of the bar in the window `ctx` draws: the window control
/// clicked, and on the bar's empty stretches (`background`) a drag that moves the
/// window, a double-click ([`super::double_click`]: maximise or restore, or on macOS
/// what System Settings asks), and a right-click, whose position (in points) is
/// returned for the system window menu where the platform can show one.
pub fn drive(
    ctx: &egui::Context,
    chrome: Chrome,
    clicked: Option<WindowAction>,
    background: &Response,
) -> Option<Pos2> {
    if let Some(action) = clicked {
        ctx.send_viewport_cmd(action.command());
    }
    if !chrome.drags {
        return None;
    }
    if background.drag_started_by(PointerButton::Primary) {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if background.double_clicked()
        && let Some(command) = super::double_click(chrome.maximised)
    {
        ctx.send_viewport_cmd(command);
    }
    if background.secondary_clicked() {
        return background.interact_pointer_pos();
    }
    None
}
