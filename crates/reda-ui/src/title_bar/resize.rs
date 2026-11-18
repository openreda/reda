//! The invisible edge-resize strip of an undecorated window, on Windows and Wayland
//! (docs/spec/v0.1/app.md#title-bar, design-language.md#title-bar): an undecorated winit
//! window has no resize borders, so REDA keeps a strip inside the window's edge,
//! [`RESIZE_EDGE`] wide along the edges and reaching [`RESIZE_CORNER`] along each of the
//! two edges that meet at a corner. A press there hands the resize to the OS or the
//! compositor (`ViewportCommand::BeginResize`). The strip is off while the window is
//! maximised or full screen.
//!
//! Where the strip is is a pure function of the window's rect ([`zones`], [`hit`]), so it
//! is tested without a window.

use egui::viewport::ResizeDirection;
use egui::{Area, CursorIcon, Id, Order, PointerButton, Pos2, Rect, Sense, pos2};

use super::{RESIZE_CORNER, RESIZE_EDGE};

/// The strip's twelve pieces in `window`, corners first: each corner is two rects, one
/// along each edge, and then the four edges between them.
pub fn zones(window: Rect) -> [(ResizeDirection, Rect); 12] {
    use ResizeDirection::*;
    let (l, t, r, b) = (window.left(), window.top(), window.right(), window.bottom());
    let (e, c) = (RESIZE_EDGE, RESIZE_CORNER);
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(pos2(x0, y0), pos2(x1, y1));
    [
        (NorthWest, rect(l, t, l + c, t + e)),
        (NorthWest, rect(l, t, l + e, t + c)),
        (NorthEast, rect(r - c, t, r, t + e)),
        (NorthEast, rect(r - e, t, r, t + c)),
        (SouthWest, rect(l, b - e, l + c, b)),
        (SouthWest, rect(l, b - c, l + e, b)),
        (SouthEast, rect(r - c, b - e, r, b)),
        (SouthEast, rect(r - e, b - c, r, b)),
        (North, rect(l, t, r, t + e)),
        (South, rect(l, b - e, r, b)),
        (West, rect(l, t, l + e, b)),
        (East, rect(r - e, t, r, b)),
    ]
}

/// Where a press at `pointer` resizes `window` from, if it is on the strip. Each piece
/// holds its top and left edges and not its bottom and right ones, so neighbouring
/// pieces never both hold a point and the strip is exactly 6 and 12 px.
pub fn hit(window: Rect, pointer: Pos2) -> Option<ResizeDirection> {
    let holds = |r: &Rect| {
        (r.left()..r.right()).contains(&pointer.x) && (r.top()..r.bottom()).contains(&pointer.y)
    };
    zones(window)
        .into_iter()
        .find(|(_, r)| holds(r))
        .map(|(direction, _)| direction)
}

/// The pointer shown over the strip resizing towards `direction`.
pub fn cursor(direction: ResizeDirection) -> CursorIcon {
    match direction {
        ResizeDirection::North => CursorIcon::ResizeNorth,
        ResizeDirection::South => CursorIcon::ResizeSouth,
        ResizeDirection::East => CursorIcon::ResizeEast,
        ResizeDirection::West => CursorIcon::ResizeWest,
        ResizeDirection::NorthEast => CursorIcon::ResizeNorthEast,
        ResizeDirection::SouthEast => CursorIcon::ResizeSouthEast,
        ResizeDirection::NorthWest => CursorIcon::ResizeNorthWest,
        ResizeDirection::SouthWest => CursorIcon::ResizeSouthWest,
    }
}

/// Puts the strip around the window `ctx` draws, above everything else in it: over the
/// strip the pointer shows the resize cursor, and a primary press starts a resize there
/// instead of reaching what is under it. Draws nothing. Call it once a frame in each
/// undecorated window, with `on` false while the window is maximised or full screen.
pub fn show(ctx: &egui::Context, on: bool) {
    if !on {
        return;
    }
    let window = ctx.content_rect();
    let id = Id::NULL.with("reda-resize-strip");
    Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(window.min)
        .movable(false)
        .sense(Sense::hover())
        .constrain(false)
        .show(ctx, |ui| {
            let pointer = ui.input(|i| i.pointer.hover_pos());
            let under = pointer.and_then(|p| hit(window, p));
            // The pieces overlap (a corner piece lies over its edge's), and egui hovers
            // only one of them: whichever piece is hovered, the direction is `hit`'s.
            let mut hovered = false;
            for (i, (_, rect)) in zones(window).into_iter().enumerate() {
                hovered |= ui.interact(rect, id.with(i), Sense::drag()).hovered();
            }
            if let Some(direction) = under.filter(|_| hovered) {
                ui.ctx().set_cursor_icon(cursor(direction));
                if ui.input(|i| i.pointer.button_pressed(PointerButton::Primary)) {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ResizeDirection::*;

    /// A window 800 by 600 at (100, 50).
    fn window() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), egui::vec2(800.0, 600.0))
    }

    #[test]
    fn the_edges_resize_within_six_px_of_the_edge() {
        let w = window();
        assert_eq!(hit(w, pos2(500.0, 50.0)), Some(North));
        assert_eq!(hit(w, pos2(500.0, 55.9)), Some(North));
        assert_eq!(hit(w, pos2(500.0, 56.0)), None);
        assert_eq!(hit(w, pos2(500.0, 649.0)), Some(South));
        assert_eq!(hit(w, pos2(500.0, 643.9)), None);
        assert_eq!(hit(w, pos2(100.0, 300.0)), Some(West));
        assert_eq!(hit(w, pos2(106.0, 300.0)), None);
        assert_eq!(hit(w, pos2(894.5, 300.0)), Some(East));
        assert_eq!(hit(w, pos2(893.9, 300.0)), None);
    }

    #[test]
    fn the_corners_reach_twelve_px_along_each_edge() {
        let w = window();
        // Top left: 12 along the top edge, 12 down the left edge, 6 deep.
        assert_eq!(hit(w, pos2(100.0, 50.0)), Some(NorthWest));
        assert_eq!(hit(w, pos2(111.9, 52.0)), Some(NorthWest));
        assert_eq!(hit(w, pos2(112.0, 52.0)), Some(North));
        assert_eq!(hit(w, pos2(102.0, 61.9)), Some(NorthWest));
        assert_eq!(hit(w, pos2(102.0, 62.0)), Some(West));
        // Inside the corner square but off both 6 px strips is not the strip.
        assert_eq!(hit(w, pos2(108.0, 58.0)), None);
        assert_eq!(hit(w, pos2(890.0, 51.0)), Some(NorthEast));
        assert_eq!(hit(w, pos2(897.0, 60.0)), Some(NorthEast));
        assert_eq!(hit(w, pos2(887.0, 51.0)), Some(North));
        assert_eq!(hit(w, pos2(101.0, 640.0)), Some(SouthWest));
        assert_eq!(hit(w, pos2(110.0, 647.0)), Some(SouthWest));
        assert_eq!(hit(w, pos2(101.0, 637.0)), Some(West));
        assert_eq!(hit(w, pos2(899.0, 649.0)), Some(SouthEast));
        assert_eq!(hit(w, pos2(889.0, 646.0)), Some(SouthEast));
        assert_eq!(hit(w, pos2(896.0, 639.0)), Some(SouthEast));
        assert_eq!(hit(w, pos2(896.0, 637.0)), Some(East));
    }

    #[test]
    fn the_inside_and_the_outside_are_not_the_strip() {
        let w = window();
        assert_eq!(hit(w, pos2(500.0, 300.0)), None);
        assert_eq!(hit(w, pos2(99.0, 300.0)), None);
        assert_eq!(hit(w, pos2(500.0, 49.0)), None);
        assert_eq!(hit(w, pos2(900.0, 300.0)), None);
        assert_eq!(hit(w, pos2(500.0, 650.0)), None);
    }

    #[test]
    fn each_direction_shows_its_own_cursor() {
        assert_eq!(cursor(North), CursorIcon::ResizeNorth);
        assert_eq!(cursor(SouthWest), CursorIcon::ResizeSouthWest);
        assert_eq!(cursor(East), CursorIcon::ResizeEast);
        assert_eq!(cursor(NorthWest), CursorIcon::ResizeNorthWest);
    }
}
