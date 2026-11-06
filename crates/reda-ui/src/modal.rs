//! Modal dialogs with a movable, resizable frame.
//!
//! egui's stock modal is fixed in the middle of the window. A [`Dialog`] opens centred at
//! its default size like one, and blocks the rest of the window the same way (a backdrop,
//! and nothing under it takes input), but it can be moved by its title and resized by its
//! edges, down to a minimum its content still fits. Its height follows its content: as
//! the dialog narrows, wrapped text reflows and the dialog grows taller; lists scroll in
//! their own scroll areas.

use egui::{Align2, Color32, Context, Id, LayerId, Order, Rect, Ui, Vec2};

/// The backdrop's colour, as egui's stock modal has it.
const BACKDROP: Color32 = Color32::from_black_alpha(100);
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
        let screen = ctx.content_rect();
        let is_top = ctx.memory(|m| m.top_modal_layer()) == Some(layer);
        let any_popup_open = egui::Popup::is_any_open(ctx);
        ctx.memory_mut(|m| m.set_modal_layer(layer));
        // The backdrop goes first on the dialog's own layer, so it is under the frame.
        ctx.layer_painter(layer).rect_filled(screen, 0.0, BACKDROP);
        let window = egui::Window::new(self.title)
            .id(self.id)
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(true)
            .constrain(true)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(screen.center())
            .default_width(self.width)
            .min_size(self.min_size);
        let shown = window.show(ctx, |ui| {
            let inner = content(ui);
            // Room for the resize grip, so it never sits on the last row's buttons.
            ui.add_space(ui.visuals().resize_corner_size);
            inner
        });
        let (rect, inner) = match shown {
            Some(shown) => (shown.response.rect, shown.inner),
            None => (Rect::NOTHING, None),
        };
        let inner = inner.expect("dialogs do not collapse");
        let outside = ctx.input(|i| {
            i.pointer.any_click()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !rect.expand(EDGE).contains(p))
        });
        DialogResponse {
            ctx: ctx.clone(),
            inner,
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
    clicked_outside: bool,
    escape: bool,
}

impl<T> DialogResponse<T> {
    /// Whether the dialog should close: a click outside it, or Escape while it is the
    /// topmost dialog and no popup is open.
    pub fn should_close(&self) -> bool {
        self.clicked_outside
            || (self.escape
                && self
                    .ctx
                    .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)))
    }
}
