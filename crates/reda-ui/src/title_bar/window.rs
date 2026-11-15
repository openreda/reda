//! What a window REDA decorates itself does with its bar and its edge: the bar's clicks
//! become window commands, the edge resizes, and a rounded window gets its rim
//! (docs/spec/v0.1/app.md#title-bar, design-language.md#title-bar, ADR-0014).
//!
//! Used by the main window and by each floating window, inside its own viewport, so
//! every command goes to the window the bar is in.

use egui::{Id, LayerId, Order, PointerButton, Pos2, Response, Stroke, StrokeKind};

use super::{Chrome, resize};
use crate::theme::TokensExt as _;
use crate::widgets::WindowAction;

/// Carries out a frame of the bar in the window `ctx` draws: the window control
/// clicked, and on the bar's empty stretches (`background`) a drag that moves the
/// window, a double-click that maximises or restores it, and a right-click, whose
/// position (in points) is returned for the system window menu where the platform
/// can show one.
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
    if background.double_clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!chrome.maximised));
    }
    if background.secondary_clicked() {
        return background.interact_pointer_pos();
    }
    None
}

/// The edge of the window `ctx` draws, where REDA decorates it: the invisible resize
/// strip while it resizes, and a hairline rim along a rounded window's edge, in place
/// of the shadow a compositor that draws none (GNOME) leaves out.
pub fn edge(ctx: &egui::Context, chrome: Chrome) {
    resize::show(ctx, chrome.resizes);
    if chrome.corner <= 0.0 {
        return;
    }
    let t = ctx.tokens();
    let layer = LayerId::new(Order::Foreground, Id::NULL.with("reda-window-rim"));
    ctx.layer_painter(layer).rect_stroke(
        ctx.content_rect(),
        chrome.corner,
        Stroke::new(t.stroke.hairline, t.color.stroke.hairline),
        StrokeKind::Inside,
    );
}
