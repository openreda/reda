//! Auto-pan (docs/spec/v0.1/editing.md#navigation): while an operation is in flight (a
//! drag, a move, a Wire or route being drawn), a cursor near or past the canvas edge pans
//! the view, in the style the `editing.auto_pan` Preference picks.
//!
//! - **Off:** never.
//! - **Ballistic** (the default): the further into the edge band, or past the edge, the
//!   faster, up to four times the speed.
//! - **Fixed Size Jump:** a steady speed anywhere in the band.
//! - **Re-Center:** the view centres on the cursor once it reaches the band, then waits
//!   until the cursor has left the band before it does again.
//!
//! Shift held pans three times faster. [`AutoPan::step`] is pure: the canvas feeds it the
//! pointer and the frame time and applies what it returns.

use egui::{Pos2, Rect, Vec2, vec2};
use reda_engine::preferences::{AutoPan as AutoPanPreference, AutoPanStyle};

/// How deep the band along each canvas edge is, in points.
pub const EDGE: f32 = 24.0;
/// How much faster Shift pans.
pub const SHIFT_FACTOR: f32 = 3.0;
/// Ballistic panning tops out at this many times the speed.
const MAX_BALLISTIC: f32 = 4.0;

/// What one frame of auto-pan does to the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pan {
    /// Nothing.
    Still,
    /// Move the view this far, in points on screen: positive x shows more of what is to
    /// the right.
    By(Vec2),
    /// Centre the view on this screen point.
    Center(Pos2),
}

/// The auto-pan state of one canvas: only Re-Center remembers anything between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AutoPan {
    /// Re-Center has centred and waits for the cursor to leave the band.
    latched: bool,
}

impl AutoPan {
    /// One frame `dt` seconds long, with the pointer at `pointer` (anywhere, even outside
    /// the canvas `rect`) and Shift `shift`.
    pub fn step(
        &mut self,
        prefs: &AutoPanPreference,
        rect: Rect,
        pointer: Pos2,
        shift: bool,
        dt: f32,
    ) -> Pan {
        // How far into the band (0 at its inner edge, 1 on the canvas edge, more past it)
        // along each axis, signed toward the edge.
        let depth = |p: f32, lo: f32, hi: f32| -> f32 {
            if p < lo + EDGE {
                -((lo + EDGE - p) / EDGE)
            } else if p > hi - EDGE {
                (p - (hi - EDGE)) / EDGE
            } else {
                0.0
            }
        };
        let d = vec2(
            depth(pointer.x, rect.left(), rect.right()),
            depth(pointer.y, rect.top(), rect.bottom()),
        );
        if d == Vec2::ZERO {
            self.latched = false;
            return Pan::Still;
        }
        let speed = prefs.speed.max(0.0) as f32 * if shift { SHIFT_FACTOR } else { 1.0 };
        match prefs.style {
            AutoPanStyle::Off => Pan::Still,
            AutoPanStyle::Ballistic => {
                let v = d.clamp(Vec2::splat(-MAX_BALLISTIC), Vec2::splat(MAX_BALLISTIC));
                Pan::By(v * speed * dt)
            }
            AutoPanStyle::FixedSizeJump => {
                // `f32::signum(0.0)` is 1, so an axis outside the band is kept at 0.
                let sign = |v: f32| if v == 0.0 { 0.0 } else { v.signum() };
                Pan::By(vec2(sign(d.x), sign(d.y)) * speed * dt)
            }
            AutoPanStyle::ReCenter if self.latched => Pan::Still,
            AutoPanStyle::ReCenter => {
                self.latched = true;
                Pan::Center(pointer)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn prefs(style: AutoPanStyle) -> AutoPanPreference {
        AutoPanPreference {
            style,
            speed: 1000.0,
        }
    }

    const RECT: Rect = Rect {
        min: pos2(0.0, 0.0),
        max: pos2(400.0, 300.0),
    };

    fn by(pan: Pan) -> Vec2 {
        match pan {
            Pan::By(v) => v,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn away_from_the_edges_nothing_pans() {
        for style in [
            AutoPanStyle::Ballistic,
            AutoPanStyle::FixedSizeJump,
            AutoPanStyle::ReCenter,
        ] {
            let mut a = AutoPan::default();
            assert_eq!(
                a.step(&prefs(style), RECT, pos2(200.0, 150.0), false, 0.1),
                Pan::Still
            );
        }
    }

    #[test]
    fn off_never_pans() {
        let mut a = AutoPan::default();
        let off = prefs(AutoPanStyle::Off);
        assert_eq!(
            a.step(&off, RECT, pos2(399.0, 150.0), true, 0.1),
            Pan::Still
        );
        assert_eq!(
            a.step(&off, RECT, pos2(-50.0, 150.0), true, 0.1),
            Pan::Still
        );
    }

    #[test]
    fn ballistic_speeds_up_the_further_past_the_edge_and_shift_is_faster() {
        let mut a = AutoPan::default();
        let p = prefs(AutoPanStyle::Ballistic);
        // Half way into the right band: half the speed, to the right.
        assert_eq!(
            by(a.step(&p, RECT, pos2(388.0, 150.0), false, 0.1)),
            vec2(50.0, 0.0)
        );
        // On the edge: the full speed.
        assert_eq!(
            by(a.step(&p, RECT, pos2(400.0, 150.0), false, 0.1)),
            vec2(100.0, 0.0)
        );
        // Past the edge, up to four times.
        assert_eq!(
            by(a.step(&p, RECT, pos2(424.0, 150.0), false, 0.1)),
            vec2(200.0, 0.0)
        );
        assert_eq!(
            by(a.step(&p, RECT, pos2(900.0, 150.0), false, 0.1)),
            vec2(400.0, 0.0)
        );
        // The top left corner pans up and left; Shift triples it.
        assert_eq!(
            by(a.step(&p, RECT, pos2(0.0, 0.0), true, 0.1)),
            vec2(-300.0, -300.0)
        );
    }

    #[test]
    fn fixed_size_jump_keeps_one_speed_in_the_band() {
        let mut a = AutoPan::default();
        let p = prefs(AutoPanStyle::FixedSizeJump);
        for x in [380.0, 400.0, 600.0] {
            assert_eq!(
                by(a.step(&p, RECT, pos2(x, 150.0), false, 0.1)),
                vec2(100.0, 0.0)
            );
        }
        assert_eq!(
            by(a.step(&p, RECT, pos2(200.0, 290.0), true, 0.1)),
            vec2(0.0, 300.0)
        );
    }

    #[test]
    fn re_center_centres_once_until_the_cursor_leaves_the_band() {
        let mut a = AutoPan::default();
        let p = prefs(AutoPanStyle::ReCenter);
        let edge = pos2(395.0, 150.0);
        assert_eq!(a.step(&p, RECT, edge, false, 0.1), Pan::Center(edge));
        assert_eq!(a.step(&p, RECT, edge, false, 0.1), Pan::Still);
        assert_eq!(a.step(&p, RECT, pos2(200.0, 150.0), false, 0.1), Pan::Still);
        assert_eq!(a.step(&p, RECT, edge, false, 0.1), Pan::Center(edge));
    }
}
