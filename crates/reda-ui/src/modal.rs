//! Modal dialogs with a movable, resizable frame.
//!
//! A [`Dialog`] is the dialog frame of [`crate::widgets::window`] over the scrim: it
//! opens centred at its default size and blocks the rest of the window (nothing under the
//! scrim takes input), and it can be moved by its title and resized by its edges, down
//! to a minimum its content still fits. Its height follows its content: as the dialog
//! narrows, wrapped text reflows and the dialog grows taller; lists scroll in their own
//! scroll areas.
//!
//! End a dialog's content with [`buttons`]: every dialog's buttons are in that one row.

use egui::{Context, Id, LayerId, Order, Ui, Vec2};

use crate::widgets;

/// The smallest a dialog can be resized to, unless it asks for more.
pub const MIN_SIZE: Vec2 = Vec2::new(240.0, 120.0);
/// How far outside its frame a press still counts as on the dialog (its resize edges).
const EDGE: f32 = 8.0;

/// A modal dialog: centred, movable by its title and resizable by its edges.
#[must_use = "call `show`"]
pub struct Dialog {
    id: Id,
    title: String,
    width: f32,
    min_size: Vec2,
}

impl Dialog {
    /// A dialog titled `title`. Its title bar names it and is where it is moved from.
    pub fn new(id: Id, title: impl Into<String>) -> Dialog {
        Dialog {
            id,
            title: title.into(),
            width: 380.0,
            min_size: MIN_SIZE,
        }
    }

    /// The width it opens at.
    pub fn width(mut self, width: f32) -> Dialog {
        self.width = width;
        self
    }

    /// The smallest it can be resized to.
    pub fn min_size(mut self, size: Vec2) -> Dialog {
        self.min_size = size;
        self
    }

    /// The layer the dialog is drawn on.
    pub fn layer(&self) -> LayerId {
        LayerId::new(Order::Foreground, self.id)
    }

    /// Shows the dialog for one frame.
    pub fn show<T>(self, ctx: &Context, content: impl FnOnce(&mut Ui) -> T) -> DialogResponse<T> {
        let layer = self.layer();
        let is_top = ctx.memory(|m| m.top_modal_layer()) == Some(layer);
        let any_popup_open = egui::Popup::is_any_open(ctx);
        ctx.memory_mut(|m| m.set_modal_layer(layer));
        // The scrim goes first on the dialog's own layer, so it is under the frame.
        widgets::scrim(ctx, layer);
        let shown = widgets::window(self.id, self.title)
            .order(Order::Foreground)
            .closable()
            .resizable(true)
            .centred()
            .width(self.width)
            .min_size(self.min_size)
            .show(ctx, content)
            .expect("a dialog without an open flag is always shown");
        let (rect, inner, closed) = (shown.response.rect, shown.inner, shown.close_clicked);
        let outside = ctx.input(|i| {
            i.pointer.any_click()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !rect.expand(EDGE).contains(p))
        });
        DialogResponse {
            ctx: ctx.clone(),
            inner,
            closed,
            clicked_outside: is_top && !any_popup_open && outside,
            escape: is_top && !any_popup_open,
        }
    }
}

/// What showing a [`Dialog`] gave.
pub struct DialogResponse<T> {
    ctx: Context,
    /// What the content returned.
    pub inner: T,
    closed: bool,
    clicked_outside: bool,
    escape: bool,
}

impl<T> DialogResponse<T> {
    /// Whether the dialog should close: its close ×, a click outside it, or Escape while
    /// it is the topmost dialog and no popup is open.
    pub fn should_close(&self) -> bool {
        self.closed
            || self.clicked_outside
            || (self.escape
                && self
                    .ctx
                    .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)))
    }
}

/// A dialog's button row: right-aligned, with one spacing, the primary button last.
///
/// The row runs from the trailing edge, so add the buttons from last to first: the
/// primary button (or the destructive confirm), then Cancel, then any other.
pub fn buttons<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    widgets::button_row(ui, add)
}

/// Gives `response` the keyboard when its dialog opens: the Cancel of a destructive
/// confirm, so that Enter does not destroy anything.
pub fn focus_on_open(response: &egui::Response) {
    let id = response.id.with("focus-on-open");
    let pass = response.ctx.cumulative_pass_nr();
    let seen: Option<u64> = response.ctx.data(|d| d.get_temp(id));
    if seen.is_none_or(|seen| seen + 1 < pass) {
        response.request_focus();
    }
    response.ctx.data_mut(|d| d.insert_temp(id, pass));
}
