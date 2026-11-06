//! REDA's own painted chrome, shared by the toolbar, the Active Bar and the Dock icon
//! strips: square icon buttons and bar separators (docs/spec/v0.1/app.md#window-shell).
//!
//! A button is flat until hovered; an active or toggled one shows the selection's accent,
//! a hovered one the hover background. A disabled one shows the icon's greyscale copy,
//! faded once by the theme's [`DISABLED_ALPHA`].

use egui::{Color32, CornerRadius, Image, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::theme::DISABLED_ALPHA;

/// The side of a toolbar or Active Bar button: a 24 px icon with 4 px around it.
pub const BAR_BUTTON: f32 = 32.0;
/// The side of a quick strip button in the top row, and the icon it holds: larger than
/// the rest of the UI, as they are hit often and from a distance.
pub const QUICK_BUTTON: f32 = 30.0;
pub const QUICK_ICON: f32 = 22.0;
/// The menu titles' and items' text size, and their button padding.
pub const MENU_TEXT: f32 = 14.5;
pub const MENU_PADDING: Vec2 = Vec2::new(8.0, 4.0);
/// The gap between bar buttons.
pub const BAR_SPACING: f32 = 2.0;
/// The width a bar separator takes.
pub const BAR_SEPARATOR: f32 = 7.0;

/// A square icon button `side` points wide with `image` centred in it at `icon` points.
/// `selected` shows it active; when not `enabled` it senses only hovering, for its
/// tooltip. Name it for AccessKit with [`crate::a11y::name`].
pub fn icon_button(
    ui: &mut Ui,
    image: Image<'static>,
    icon: f32,
    side: f32,
    selected: bool,
    enabled: bool,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), sense);
    if ui.is_rect_visible(rect) {
        let v = ui.visuals();
        let fill = if selected {
            v.selection.bg_fill
        } else if enabled && response.is_pointer_button_down_on() {
            v.widgets.active.weak_bg_fill
        } else if enabled && response.hovered() {
            v.widgets.hovered.weak_bg_fill
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
        let image = if enabled {
            image
        } else {
            image.tint(Color32::from_white_alpha((DISABLED_ALPHA * 255.0) as u8))
        };
        image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(icon)));
    }
    response
}

/// A thin vertical line between groups of bar buttons, `height` points tall.
pub fn bar_separator(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(BAR_SEPARATOR, height), Sense::hover());
    let stroke = Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
    let x = rect.center().x.round() + 0.5;
    let y = rect.y_range().shrink(height * 0.15);
    ui.painter().vline(x, y, stroke);
}
