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
    let on_empty = pressed_on_empty(ctx, background);
    if background.drag_started_by(PointerButton::Primary) && on_empty {
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

/// Whether the primary press now held (or pressed this frame) began on the bar's empty
/// space itself, and not on a button above it. egui hands a drag that starts on a click-only widget (a quick
/// strip button, a menu title) to the drag-sensing background under it, which would
/// move the window from a button. Runs every frame the bar drags, to see the press.
fn pressed_on_empty(ctx: &egui::Context, background: &Response) -> bool {
    let id = background.id.with("pressed-on-empty");
    if ctx.input(|i| i.pointer.primary_pressed()) {
        // Pressed this frame: nothing clickable but the background is under the pointer.
        let over = ctx.interaction_snapshot(|s| s.hovered.clone());
        let clickable = |id: &egui::Id| {
            *id != background.id
                && ctx
                    .read_response(*id)
                    .is_some_and(|r| r.sense.senses_click())
        };
        let empty = background.contains_pointer() && !over.iter().any(clickable);
        ctx.data_mut(|d| d.insert_temp(id, empty));
        return empty;
    }
    ctx.data(|d| d.get_temp(id)).unwrap_or(false)
}
