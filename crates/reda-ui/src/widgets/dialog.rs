//! The dialog frame: a dialog or small window with its title row, the scrim under a modal
//! one, and the button row (docs/spec/v0.1/design-language.md#dialogs-and-settings-windows).
//!
//! A dialog is `surface.floating` with the window radius and shadow and the `dialog`
//! padding. Its title row holds the title in the `title` style and, when it can be
//! closed, the close ×. It is moved by its title row. Buttons are right-aligned in one row, the primary button last.
//!
//! ```ignore
//! widgets::window(id, tr!("waive-title")).open(&mut open).show(ctx, |ui| {
//!     ..
//!     widgets::button_row(ui, |ui| {
//!         // From the trailing edge: the primary button first.
//!         if ui.add(widgets::button(tr!("messages-waive")).primary()).clicked() { .. }
//!         if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() { .. }
//!     });
//! });
//! ```
//!
//! A modal dialog is [`crate::modal::Dialog`], which adds the scrim and takes the input.

use egui::{
    Align, Align2, Context, Frame, Id, InnerResponse, LayerId, Layout, Order, Pos2, Rect, Role,
    Sense, Stroke, Ui, UiBuilder, Vec2, vec2,
};

use super::glyph::Glyph;
use super::paint;
use crate::theme::{Tokens, TokensExt as _};

/// The frame of a dialog or window: `surface.floating`, a hairline rim, the window radius
/// and shadow, and the `dialog` padding.
pub fn dialog_frame(t: &Tokens) -> Frame {
    Frame::new()
        .fill(t.color.surface.floating)
        .stroke(Stroke::new(t.stroke.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.window)
        .shadow(t.elevation.window)
        .inner_margin(t.space.dialog)
}

/// Lays `surface.scrim` over the whole window on `layer`: what a modal dialog sits over.
/// Paint it before the dialog, on the dialog's own layer, so it is under the frame.
pub fn scrim(ctx: &Context, layer: LayerId) {
    let t = ctx.tokens();
    ctx.layer_painter(layer)
        .rect_filled(ctx.content_rect(), 0.0, t.color.surface.scrim);
}

/// The least width a title row needs: its title, a gap and the close ×.
fn title_width(ui: &Ui, t: &Tokens, title: &str) -> f32 {
    paint::width(ui, &t.ty.title, title) + t.space.gap + t.ty.title.line
}

/// Draws a title row in `row`: the title in the `title` style and, with `close`, the
/// close × at the trailing end. Returns whether the × was clicked.
fn title_at(ui: &mut Ui, row: Rect, title: &str, close: bool) -> bool {
    let t = ui.tokens();
    let mut clicked = false;
    let builder = UiBuilder::new()
        .max_rect(row)
        .layout(Layout::right_to_left(Align::Center));
    ui.scope_builder(builder, |ui| {
        if close {
            let button = super::glyph_button(Glyph::Close).side(t.ty.title.line);
            let response = ui.add(button);
            let name = crate::tr!("dialog-close-window");
            response.widget_info(|| egui::WidgetInfo::labeled(Role::Button, true, &name));
            clicked = response.clicked();
        }
        let rect = ui.available_rect_before_wrap();
        paint::text_clipped(ui, rect, &t.ty.title, title, t.color.text.primary);
    });
    clicked
}

/// A dialog's title row, as wide as there is room: for a frame drawn by hand (the
/// gallery). [`Window`] draws its own. Returns whether the close × was clicked.
pub fn title_row(ui: &mut Ui, title: &str, close: bool) -> bool {
    let t = ui.tokens();
    let width = ui.available_width().max(title_width(ui, &t, title));
    let (row, _) = ui.allocate_exact_size(vec2(width, t.ty.title.line), Sense::hover());
    let clicked = title_at(ui, row, title, close);
    ui.add_space((t.space.stack - ui.spacing().item_spacing.y).max(0.0));
    clicked
}

/// A dialog's button row: right-aligned, `gap` apart, `section` below the content.
///
/// The row runs from the trailing edge, so add the buttons from last to first: the one
/// primary button (or the destructive confirm) first, then Cancel, then any other. That
/// puts the primary button last, as the recipe has it, in every dialog.
pub fn button_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let t = ui.tokens();
    ui.add_space((t.space.section - ui.spacing().item_spacing.y).max(0.0));
    let size = vec2(ui.available_width(), t.control_height());
    ui.allocate_ui_with_layout(size, Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = t.space.gap;
        add(ui)
    })
    .inner
}

/// A dialog or a small window titled `title`. It opens at its content's size and can be
/// moved; [`Window::open`] gives it a close ×.
pub fn window<'open>(id: Id, title: impl Into<String>) -> Window<'open> {
    Window {
        id,
        title: title.into(),
        open: None,
        closable: false,
        resizable: false,
        width: None,
        min_size: None,
        default_pos: None,
        fixed_pos: None,
        centred: false,
        order: None,
    }
}

#[must_use = "call .show()"]
pub struct Window<'open> {
    id: Id,
    title: String,
    open: Option<&'open mut bool>,
    closable: bool,
    resizable: bool,
    width: Option<f32>,
    min_size: Option<Vec2>,
    default_pos: Option<Pos2>,
    fixed_pos: Option<Pos2>,
    centred: bool,
    order: Option<Order>,
}

/// What showing a [`Window`] gave.
pub struct Shown<R> {
    /// What the content returned.
    pub inner: R,
    /// The window's frame.
    pub response: egui::Response,
    /// The close × was clicked this frame.
    pub close_clicked: bool,
}

impl<'open> Window<'open> {
    /// Shows the close ×, which sets `open` to false; the window is not drawn while
    /// `open` is false.
    pub fn open(mut self, open: &'open mut bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Shows the close × without a flag: read [`Shown::close_clicked`].
    pub fn closable(mut self) -> Self {
        self.closable = true;
        self
    }

    /// Whether it can be resized by its edges.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// The width it opens at.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// The smallest it can be resized to.
    pub fn min_size(mut self, size: Vec2) -> Self {
        self.min_size = Some(size);
        self
    }

    /// Where its top left corner is when it first opens.
    pub fn default_pos(mut self, pos: Pos2) -> Self {
        self.default_pos = Some(pos);
        self
    }

    /// Where its top left corner stays: it cannot be moved.
    pub fn fixed_pos(mut self, pos: Pos2) -> Self {
        self.fixed_pos = Some(pos);
        self
    }

    /// Opens in the middle of the window.
    pub fn centred(mut self) -> Self {
        self.centred = true;
        self
    }

    /// The order of its layer; a modal dialog is in the foreground.
    pub fn order(mut self, order: Order) -> Self {
        self.order = Some(order);
        self
    }

    /// Shows the window for one frame; `None` while its `open` flag is false.
    pub fn show<R>(self, ctx: &Context, add: impl FnOnce(&mut Ui) -> R) -> Option<Shown<R>> {
        if self.open.as_deref() == Some(&false) {
            return None;
        }
        let t = ctx.tokens();
        let close = self.closable || self.open.is_some();
        let title = self.title;
        // REDA moves the window itself, by its title row only: egui moves a window with
        // no title bar from anywhere on it.
        let moved_id = self.id.with("moved-to");
        let centred = self.centred;
        let movable = self.fixed_pos.is_none();
        let mut window = egui::Window::new(title.as_str())
            .id(self.id)
            .movable(false)
            .title_bar(false)
            .collapsible(false)
            .frame(dialog_frame(&t))
            .resizable(self.resizable);
        if let Some(order) = self.order {
            window = window.order(order);
        }
        if let Some(width) = self.width {
            window = window.default_width(width);
        }
        if let Some(size) = self.min_size {
            window = window.min_size(size);
        }
        if self.centred {
            window = window
                .constrain(true)
                .pivot(Align2::CENTER_CENTER)
                .default_pos(ctx.content_rect().center());
        }
        if let Some(pos) = self.default_pos {
            window = window.default_pos(pos);
        }
        if let Some(pos) = self.fixed_pos {
            window = window.fixed_pos(pos);
        }
        if let Some(pos) = ctx.data_mut(|d| d.remove_temp::<Pos2>(moved_id)) {
            window = window.current_pos(pos);
        }
        let id = self.id;
        let mut close_clicked = false;
        let resizable = self.resizable;
        let InnerResponse { inner, response } = window.show(ctx, |ui| {
            // The title row is reserved first and drawn last, once the content has said
            // how wide the window is.
            let needed = vec2(title_width(ui, &t, &title), t.ty.title.line);
            let (reserved, _) = ui.allocate_exact_size(needed, Sense::hover());
            ui.add_space((t.space.stack - ui.spacing().item_spacing.y).max(0.0));
            let inner = add(ui);
            let right = if resizable && !ui.is_sizing_pass() {
                ui.max_rect().right()
            } else {
                ui.min_rect().right()
            };
            let row = reserved.with_max_x(right.max(reserved.right()));
            if movable {
                // The title row and the padding around it are where the window is held.
                let pad = t.space.dialog;
                let grip = row.expand2(vec2(pad, 0.0)).with_min_y(row.top() - pad);
                let held = ui.interact(grip, id.with("title-grip"), Sense::drag());
                let delta = held.drag_delta();
                if held.dragged()
                    && delta != Vec2::ZERO
                    && let Some(rect) = ui.ctx().memory(|m| m.area_rect(id))
                {
                    let pivot = if centred { rect.center() } else { rect.min };
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(moved_id, pivot + delta));
                    ui.ctx().request_repaint();
                }
            }
            close_clicked = title_at(ui, row, &title, close);
            inner
        })?;
        if close_clicked && let Some(open) = self.open {
            *open = false;
        }
        Some(Shown {
            inner: inner.expect("a dialog does not collapse"),
            response,
            close_clicked,
        })
    }
}
