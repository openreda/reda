//! The surfaces things are drawn on: the card, the floating surface and the Dock flyout
//! (docs/spec/v0.1/design-language.md#shape-and-elevation, #docks, #panels).
//!
//! Each is an `egui::Frame` built from tokens, to `.show(ui, ..)` content in. A shadow
//! appears only here and on the popups and windows of `menu.rs`, `tooltip.rs` and
//! `dialog.rs`: a card takes `elevation.card`, what floats over the window takes
//! `elevation.popup`, and a dialog or window takes `elevation.window`.

use egui::{Align2, Color32, Frame, Mesh, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use super::paint;
use crate::theme::{Tokens, TokensExt as _};

/// Between cards, and from cards to the window edge. A flyout keeps the same gap from its
/// strip and from the window's edges.
pub const CARD_GAP: f32 = 6.0;

fn rim(t: &Tokens) -> Stroke {
    Stroke::new(t.stroke.hairline, t.color.stroke.hairline)
}

/// A card: a tab group's body, a Dock's Panel, a section of a page. `surface.card` with a
/// hairline rim, the card radius, the 1 px soft shadow and the `panel` padding.
pub fn card_frame(t: &Tokens) -> Frame {
    Frame::new()
        .fill(t.color.surface.card)
        .stroke(rim(t))
        .corner_radius(t.radius.card)
        .shadow(t.elevation.card)
        .inner_margin(t.space.panel)
}

/// Paints a card that fills `rect`, for content placed by hand: the soft shadow, the
/// surface and the rim. A tab group's body is one.
pub fn paint_card(ui: &Ui, t: &Tokens, rect: Rect) {
    let p = ui.painter();
    p.add(t.elevation.card.as_shape(rect, t.radius.card));
    p.rect(
        rect,
        t.radius.card,
        t.color.surface.card,
        rim(t),
        StrokeKind::Inside,
    );
}

/// Rounds what was painted square inside the card `rect`, such as a GPU canvas, which
/// cannot be clipped round: paints the window's base over the card's corners outside its
/// radius, then the rim again over the content's edge.
pub fn mask_card_corners(ui: &Ui, t: &Tokens, rect: Rect) {
    corner_masks(ui, rect, t.radius.card, t.color.surface.base);
    ui.painter()
        .rect_stroke(rect, t.radius.card, rim(t), StrokeKind::Inside);
}

/// The four wedges between `rect`'s square corners and its corners rounded by `radius`,
/// filled with `color`.
fn corner_masks(ui: &Ui, rect: Rect, radius: f32, color: Color32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    /// Segments per quarter circle.
    const STEPS: u32 = 12;
    // Each corner, the way into the rect from it, and the angle its arc starts at.
    let corners = [
        (rect.left_top(), vec2(1.0, 1.0), PI),
        (rect.right_top(), vec2(-1.0, 1.0), 1.5 * PI),
        (rect.right_bottom(), vec2(-1.0, -1.0), 0.0),
        (rect.left_bottom(), vec2(1.0, -1.0), FRAC_PI_2),
    ];
    let mut mesh = Mesh::default();
    for (corner, inward, start) in corners {
        let centre = corner + inward * radius;
        let first = mesh.vertices.len() as u32;
        mesh.colored_vertex(corner, color);
        for step in 0..=STEPS {
            let a = start + step as f32 / STEPS as f32 * FRAC_PI_2;
            mesh.colored_vertex(centre + radius * vec2(a.cos(), a.sin()), color);
        }
        for step in 0..STEPS {
            mesh.add_triangle(first, first + 1 + step, first + 2 + step);
        }
    }
    ui.painter().add(mesh);
}

/// A floating surface: a bar on the canvas, a hint, anything that floats over the window
/// and is not a menu, a tooltip or a dialog. `surface.floating` with a hairline rim, the
/// popup radius and shadow, and the `menu` padding.
pub fn floating_frame(t: &Tokens) -> Frame {
    Frame::new()
        .fill(t.color.surface.floating)
        .stroke(rim(t))
        .corner_radius(t.radius.popup)
        .shadow(t.elevation.popup)
        .inner_margin(t.space.menu)
}

/// The open flyout of a collapsed Dock: the Dock's card, floating over the window beside
/// its strip. The card's surface, rim, radius and `panel` padding, under the popup shadow
/// because it floats. Place it [`CARD_GAP`] from the strip.
pub fn flyout_frame(t: &Tokens) -> Frame {
    card_frame(t).shadow(t.elevation.popup)
}

/// The title row of a flyout: the Panel's Icon and its name in the `title` style, then
/// the `stack` gap. A collapsed Dock's strip shows Icons only, so the flyout names what
/// it holds.
pub fn flyout_title(ui: &mut Ui, icon: Option<&str>, title: &str) {
    let t = ui.tokens();
    let size = vec2(ui.available_width(), t.ty.title.line);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let mut left = rect.left();
    if let Some(image) =
        icon.and_then(|key| crate::icons::image(key, t.icon.small, ui.ctx().theme(), true))
    {
        let at = pos2(left + t.icon.small / 2.0, rect.center().y);
        image.paint_at(ui, Rect::from_center_size(at, Vec2::splat(t.icon.small)));
        left += t.icon.small + super::ICON_GAP;
    }
    let label = rect.with_min_x(left);
    let ink = t.color.text.primary;
    paint::text_in(ui, label, Align2::LEFT_CENTER, &t.ty.title, title, ink);
    ui.add_space((t.space.stack - ui.spacing().item_spacing.y).max(0.0));
}
