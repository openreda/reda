//! Snapping the cursor: to the snap grid, or onto a nearby electrical target (a Pin, a
//! Wire end or a Wire on a sheet; a Pad, Track end or Via centre on a board).

use reda_engine::model::geom::{Nm, Point};

use super::hit::closest_on_segment;

/// What the cursor can snap onto besides the grid. The editor fills it from its Document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapTargets {
    /// Points, which win over segments: Pins, Wire ends.
    pub points: Vec<Point>,
    /// Segments the cursor can land anywhere along, at a grid step where the segment is
    /// horizontal or vertical: Wires.
    pub segments: Vec<(Point, Point)>,
}

/// Where the cursor snapped to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapped {
    pub at: Point,
    /// It snapped onto an electrical target rather than the grid.
    pub electrical: bool,
}

/// The grid point nearest `at` (nm), on a grid of `pitch` through the origin.
pub fn snap_to_grid(at: (f64, f64), pitch: Nm) -> Point {
    let g = pitch.0.max(1) as f64;
    Point::new(
        Nm(((at.0 / g).round() * g) as i64),
        Nm(((at.1 / g).round() * g) as i64),
    )
}

impl SnapTargets {
    /// `at` (nm) snapped: onto the nearest point target within `radius` nm, else onto the
    /// nearest segment within `radius`, else to the grid of `pitch`.
    pub fn snap(&self, at: (f64, f64), pitch: Nm, radius: f64) -> Snapped {
        let dist = |p: (f64, f64)| (p.0 - at.0).hypot(p.1 - at.1);
        let point = self
            .points
            .iter()
            .map(|p| (dist(f(*p)), *p))
            .filter(|(d, _)| *d <= radius)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p)) = point {
            return Snapped {
                at: p,
                electrical: true,
            };
        }
        let on_segment = self
            .segments
            .iter()
            .map(|&(a, b)| {
                let c = closest_on_segment(at, f(a), f(b));
                (dist(c), a, b, c)
            })
            .filter(|(d, ..)| *d <= radius)
            .min_by(|x, y| x.0.total_cmp(&y.0));
        if let Some((_, a, b, c)) = on_segment {
            let grid = snap_to_grid(c, pitch);
            let (lo, hi) = (
                Point::new(a.x.min(b.x), a.y.min(b.y)),
                Point::new(a.x.max(b.x), a.y.max(b.y)),
            );
            let at = if a.x == b.x {
                Point::new(a.x, grid.y.clamp(lo.y, hi.y))
            } else if a.y == b.y {
                Point::new(grid.x.clamp(lo.x, hi.x), a.y)
            } else {
                Point::new(Nm(c.0.round() as i64), Nm(c.1.round() as i64))
            };
            return Snapped {
                at,
                electrical: true,
            };
        }
        Snapped {
            at: snap_to_grid(at, pitch),
            electrical: false,
        }
    }
}

fn f(p: Point) -> (f64, f64) {
    (p.x.0 as f64, p.y.0 as f64)
}
