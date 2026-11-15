//! What a dragged tab shows (docs/spec/v0.1/design-language.md#the-accent,
//! #lists-trees-and-tables): the preview of where it would land, the drop-target buttons
//! and the chip that follows the pointer.
//!
//! A drop target is the accent subtle tint inside a 2 px accent line. A drop the layout
//! would refuse shows the same recipe in the error tone.

use egui::{Align2, Painter, Rect, Stroke, StrokeKind, Ui, vec2};

use super::paint;
use crate::theme::{Tokens, over};

/// The side of a drop-target button.
pub const DROP_BUTTON: f32 = 28.0;
/// How far a drop-target button's mark stands in from its edges.
const MARK_INSET: f32 = 6.0;
/// How much further the mark of "into this group" stands in.
const WHOLE_INSET: f32 = 3.0;

/// Which part of a group or of the window a drop-target button stands for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropPart {
    /// Among the group's tabs.
    Whole,
    Left,
    Right,
    Up,
    Down,
}

/// The preview of a drop over `rect`, the part of the window the tab would fill: the
/// accent subtle tint inside the accent line, or the error tone's when the layout would
/// refuse the drop.
pub fn drop_preview(painter: &Painter, t: &Tokens, rect: Rect, refused: bool) {
    let tone = if refused {
        t.color.error
    } else {
        t.color.accent
    };
    painter.rect(
        rect,
        t.radius.card,
        tone.subtle,
        Stroke::new(t.stroke.accent_line, tone.text),
        StrokeKind::Inside,
    );
}

/// A drop-target button in `rect`: a square on the floating surface with a mark for the
/// `part` the tab would take. While the pointer is `over` it, it is a drop target: the
/// accent subtle tint inside the accent line.
pub fn drop_button(painter: &Painter, t: &Tokens, rect: Rect, part: DropPart, over_it: bool) {
    let c = &t.color;
    let (fill, rim) = if over_it {
        (
            over(c.accent.subtle, c.surface.floating),
            Stroke::new(t.stroke.accent_line, c.accent.text),
        )
    } else {
        (
            c.surface.floating,
            Stroke::new(t.stroke.hairline, c.stroke.strong),
        )
    };
    painter.rect(rect, t.radius.control, fill, rim, StrokeKind::Inside);
    let inner = rect.shrink(MARK_INSET);
    let mark = match part {
        DropPart::Left => inner.with_max_x(inner.center().x),
        DropPart::Right => inner.with_min_x(inner.center().x),
        DropPart::Up => inner.with_max_y(inner.center().y),
        DropPart::Down => inner.with_min_y(inner.center().y),
        DropPart::Whole => inner.shrink(WHOLE_INSET),
    };
    painter.rect_filled(mark, t.radius.mark / 2.0, c.accent.text);
}

/// The dragged tab's title on a chip whose top left corner is at `at`: the floating
/// surface under the popup shadow, with a control's radius and padding.
pub fn drag_chip(ui: &Ui, t: &Tokens, at: egui::Pos2, title: &str) {
    let c = &t.color;
    let pad = vec2(t.space.control_x, t.space.control_y);
    let text = paint::galley(ui, &t.ty.body, title, c.text.primary);
    let chip = Rect::from_min_size(at, text.size() + 2.0 * pad);
    let p = ui.painter();
    p.add(t.elevation.popup.as_shape(chip, t.radius.control));
    p.rect(
        chip,
        t.radius.control,
        c.surface.floating,
        Stroke::new(t.stroke.hairline, c.stroke.hairline),
        StrokeKind::Inside,
    );
    paint::text_in(
        ui,
        chip,
        Align2::CENTER_CENTER,
        &t.ty.body,
        title,
        c.text.primary,
    );
}
