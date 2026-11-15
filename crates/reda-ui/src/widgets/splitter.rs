//! The splitter at a Dock's inner edge (docs/spec/v0.1/design-language.md#docks).
//!
//! The gap between a Dock and what it borders is its splitter: nothing at rest, the
//! hover fill while the pointer is on it, and the accent while it is dragged.

use egui::{Stroke, Ui};

use crate::theme::TokensExt as _;

/// Runs `add`, which shows a resizable egui panel whose outer margin on its resize edge
/// is `gap` wide, with the panel's resize line drawn as REDA's splitter filling that gap.
/// egui takes the line from the `Ui`'s style, which is put back afterwards.
pub fn splitter<R>(ui: &mut Ui, gap: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let t = ui.tokens();
    let widgets = &mut ui.style_mut().visuals.widgets;
    let before = (widgets.hovered.fg_stroke, widgets.active.fg_stroke);
    widgets.hovered.fg_stroke = Stroke::new(gap, t.color.fill.hover);
    widgets.active.fg_stroke = Stroke::new(gap, t.color.accent.text);
    let inner = add(ui);
    let widgets = &mut ui.style_mut().visuals.widgets;
    (widgets.hovered.fg_stroke, widgets.active.fg_stroke) = before;
    inner
}
