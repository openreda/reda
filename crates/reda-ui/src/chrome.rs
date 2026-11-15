//! REDA's own painted chrome, shared by the toolbar and the Active Bar:
//! square icon buttons and bar separators (docs/spec/v0.1/app.md#window-shell).
//!
//! The button itself is the REDA Icon button (`widgets::icon_button`); what is left here
//! are the sizes of the bars that are not converted yet.

use egui::{Image, Response, Sense, Stroke, Ui, Vec2};

use crate::theme::TokensExt as _;
use crate::widgets;

/// The side of a toolbar or Active Bar button: a 24 px icon with 4 px around it.
pub const BAR_BUTTON: f32 = 32.0;
/// The side of a quick strip button in the top row, and the icon it holds: larger than
/// the rest of the UI, as they are hit often and from a distance.
pub const QUICK_BUTTON: f32 = 30.0;
pub const QUICK_ICON: f32 = 22.0;
/// The gap between bar buttons.
pub const BAR_SPACING: f32 = 2.0;
/// The width a bar separator takes.
pub const BAR_SEPARATOR: f32 = 7.0;

/// A square Icon button `side` points wide with `image` centred in it at `icon` points:
/// [`widgets::icon_button_image`] at the sizes of a bar that is not converted yet.
/// `selected` shows it on; when not `enabled` it senses only hovering, for its tooltip.
/// Name it for AccessKit with [`crate::a11y::name`].
pub fn icon_button(
    ui: &mut Ui,
    image: Image<'static>,
    icon: f32,
    side: f32,
    selected: bool,
    enabled: bool,
) -> Response {
    ui.add(
        widgets::icon_button_image(image)
            .icon_size(icon)
            .side(side)
            .on(selected)
            .enabled(enabled),
    )
}

/// A thin vertical line between groups of bar buttons, `height` points tall.
pub fn bar_separator(ui: &mut Ui, height: f32) {
    let t = ui.tokens();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(BAR_SEPARATOR, height), Sense::hover());
    let stroke = Stroke::new(t.stroke.hairline, t.color.stroke.hairline);
    let x = rect.center().x.round() + stroke.width / 2.0;
    let y = rect.y_range().shrink(height * 0.15);
    ui.painter().vline(x, y, stroke);
}
