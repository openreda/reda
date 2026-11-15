//! Tooltips and the key chip (docs/spec/v0.1/design-language.md#tooltips).
//!
//! A tooltip is a small popup holding either a name and a key chip, or a title and a
//! sentence in small muted text. Add one to a widget's response:
//!
//! ```ignore
//! use crate::widgets::Tooltip as _;
//!
//! ui.add(widgets::icon_button("track")).tooltip_key("Route Track", "X");
//! response.tooltip("Zoom to fit");
//! response.tooltip_titled("Out of sync", "The PCB has not taken the last changes.");
//! ui.add_enabled(false, widgets::button("Paste")).disabled_tooltip("Nothing is copied");
//! ```
//!
//! The popup is egui's, so the wait before it shows and the chaining from one tooltip to
//! the next are the `motion` tokens the generated `Style` carries.

use egui::{Align2, Frame, Margin, Response, Sense, Stroke, Ui, Widget, vec2};

use super::paint;
use crate::theme::{Tokens, TokensExt as _};

/// The widest a tooltip's sentence runs before it wraps.
pub const TOOLTIP_WIDTH: f32 = 260.0;
/// How far a key chip's fill stands off its text sideways, and how much taller than the
/// small line it is.
const CHIP_PAD_X: f32 = 5.0;
const CHIP_PAD_Y: f32 = 2.0;
/// Between a tooltip's title and its sentence.
const TITLE_GAP: f32 = 2.0;

/// The popup a tooltip is shown in: `surface.floating`, a hairline rim and the popup
/// shadow, with the control radius and a control's padding, as the component gallery
/// drew it.
pub fn tooltip_frame(t: &Tokens) -> Frame {
    Frame::new()
        .fill(t.color.surface.floating)
        .stroke(Stroke::new(t.stroke.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.control)
        .shadow(t.elevation.popup)
        .inner_margin(Margin::symmetric(
            t.space.control_x as i8,
            t.space.control_y as i8,
        ))
}

/// A key chip: a shortcut in small muted text on `fill.control` with the mark radius.
pub fn key_chip(keys: impl Into<String>) -> KeyChip {
    KeyChip { keys: keys.into() }
}

#[must_use = "add it with ui.add"]
pub struct KeyChip {
    keys: String,
}

impl Widget for KeyChip {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        let width = paint::width(ui, &t.ty.small, &self.keys) + 2.0 * CHIP_PAD_X;
        let size = vec2(width, t.ty.small.line + CHIP_PAD_Y);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &self.keys));
        if ui.is_rect_visible(rect) {
            ui.painter()
                .rect_filled(rect, t.radius.mark, t.color.fill.control);
            let (style, ink) = (&t.ty.small, t.color.text.muted);
            paint::text_in(ui, rect, Align2::CENTER_CENTER, style, &self.keys, ink);
        }
        response
    }
}

/// What a tooltip holds.
#[derive(Clone, Copy)]
pub struct TooltipText<'a> {
    /// The name or the title, in body text.
    pub title: &'a str,
    /// The key that runs the named thing, shown as a chip after the name.
    pub key: Option<&'a str>,
    /// A sentence under the title, in small muted text.
    pub sentence: Option<&'a str>,
}

/// Draws a tooltip's contents. The gallery draws them in a [`tooltip_frame`] in place; a
/// widget's own tooltip comes from the [`Tooltip`] methods.
pub fn tooltip_contents(ui: &mut Ui, text: TooltipText<'_>) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing = vec2(t.space.gap, TITLE_GAP);
    ui.set_max_width(TOOLTIP_WIDTH);
    ui.horizontal(|ui| {
        if !text.title.is_empty() {
            ui.label(t.ty.body.rich(text.title).color(t.color.text.primary));
        }
        if let Some(key) = text.key.filter(|key| !key.is_empty()) {
            ui.add(key_chip(key));
        }
    });
    if let Some(sentence) = text.sentence.filter(|s| !s.is_empty()) {
        ui.label(t.ty.small.rich(sentence).color(t.color.text.muted));
    }
}

fn show(response: &Response, disabled: bool, add: impl FnOnce(&mut Ui)) {
    let mut tip = if disabled {
        egui::Tooltip::for_disabled(response)
    } else {
        egui::Tooltip::for_enabled(response)
    };
    let t = response.ctx.tokens();
    tip.popup = tip.popup.frame(tooltip_frame(&t));
    tip.show(add);
}

/// Makes the tooltip's text the accessible description of the widget, and its name if it
/// has none (an Icon button), as egui's own text tooltip does.
fn describe(response: &Response, text: &str) {
    // Only widgets that already have a node: a hover-only rect gets none.
    if !response.ctx.has_accesskit_node(response.id) {
        return;
    }
    response.ctx.accesskit_node_builder(response.id, |node| {
        if node.description().is_none() {
            node.set_description(text);
        }
        if node.role() != egui::accesskit::Role::Label
            && node.labelled_by().is_empty()
            && node.label().is_none_or(str::is_empty)
        {
            node.set_label(text);
        }
    });
}

/// Tooltips on a widget's response.
pub trait Tooltip: Sized {
    /// A name, or a short line, shown when the widget is hovered. It is also the
    /// widget's accessible description.
    fn tooltip(self, text: impl AsRef<str>) -> Self;

    /// A name and the key that runs it.
    fn tooltip_key(self, name: impl AsRef<str>, key: impl AsRef<str>) -> Self;

    /// A title and a sentence in small muted text.
    fn tooltip_titled(self, title: impl AsRef<str>, sentence: impl AsRef<str>) -> Self;

    /// Any contents, in the tooltip's popup: for a tooltip that holds more than text.
    /// Build them with [`tooltip_contents`] where they fit it.
    fn tooltip_ui(self, add: impl FnOnce(&mut Ui)) -> Self;

    /// Shown when the widget is hovered while disabled: why it cannot be used.
    fn disabled_tooltip(self, text: impl AsRef<str>) -> Self;

    /// Any contents, shown while the widget is disabled.
    fn disabled_tooltip_ui(self, add: impl FnOnce(&mut Ui)) -> Self;
}

impl Tooltip for Response {
    fn tooltip(self, text: impl AsRef<str>) -> Self {
        let title = text.as_ref();
        describe(&self, title);
        self.tooltip_ui(|ui| {
            let text = TooltipText {
                title,
                key: None,
                sentence: None,
            };
            tooltip_contents(ui, text);
        })
    }

    fn tooltip_key(self, name: impl AsRef<str>, key: impl AsRef<str>) -> Self {
        let (title, key) = (name.as_ref(), key.as_ref());
        describe(&self, title);
        self.tooltip_ui(|ui| {
            let text = TooltipText {
                title,
                key: Some(key),
                sentence: None,
            };
            tooltip_contents(ui, text);
        })
    }

    fn tooltip_titled(self, title: impl AsRef<str>, sentence: impl AsRef<str>) -> Self {
        let (title, sentence) = (title.as_ref(), sentence.as_ref());
        describe(&self, sentence);
        self.tooltip_ui(|ui| {
            let text = TooltipText {
                title,
                key: None,
                sentence: Some(sentence),
            };
            tooltip_contents(ui, text);
        })
    }

    fn tooltip_ui(self, add: impl FnOnce(&mut Ui)) -> Self {
        show(&self, false, add);
        self
    }

    fn disabled_tooltip(self, text: impl AsRef<str>) -> Self {
        let title = text.as_ref();
        self.disabled_tooltip_ui(|ui| {
            let text = TooltipText {
                title,
                key: None,
                sentence: None,
            };
            tooltip_contents(ui, text);
        })
    }

    fn disabled_tooltip_ui(self, add: impl FnOnce(&mut Ui)) -> Self {
        show(&self, true, add);
        self
    }
}
