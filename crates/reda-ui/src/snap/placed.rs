//! The Guide level of snapping (docs/spec/v0.3/grids-guides.md#snapping): after Hotspots,
//! before alignment guides and the grid.
//!
//! - A point Guide and the crossing of two line Guides are snap points, caught on both
//!   axes as a Hotspot is.
//! - A line Guide catches the cursor within the Snap Distance of it. Along the line the
//!   other axis still follows the grid: the snap is where the Guide crosses the grid line
//!   nearest the cursor's foot on it, or the foot itself when Gridless.

use reda_engine::model::Uuid;
use reda_engine::model::geom::{Millideg, Nm, Point};

use super::Grid;

/// An enabled Guide as the snap sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacedGuide {
    pub object: Uuid,
    /// The point a line runs through, or the point itself.
    pub at: Point,
    /// A line's direction; `None` for a point Guide.
    pub angle: Option<Millideg>,
}

impl PlacedGuide {
    /// The snap's view of a stored Guide.
    pub fn of(guide: &reda_engine::model::Guide) -> PlacedGuide {
        PlacedGuide {
            object: guide.uuid,
            at: guide.at,
            angle: guide.angle,
        }
    }

    /// The point of the Guide nearest `p`: its foot on a line, or the point itself.
    pub fn nearest(&self, p: Point) -> Point {
        match self.foot((p.x.0 as f64, p.y.0 as f64)) {
            Some((foot, _)) => round(foot),
            None => self.at,
        }
    }

    fn origin(&self) -> (f64, f64) {
        (self.at.x.0 as f64, self.at.y.0 as f64)
    }

    /// A line's unit direction, exact on the axes.
    fn direction(&self) -> Option<(f64, f64)> {
        let a = self.angle?.0.rem_euclid(180_000);
        Some(match a {
            0 => (1.0, 0.0),
            90_000 => (0.0, 1.0),
            _ => {
                let t = (a as f64 / 1000.0).to_radians();
                (t.cos(), t.sin())
            }
        })
    }

    /// The foot of `at` on the line, and how far `at` is from it.
    fn foot(&self, at: (f64, f64)) -> Option<((f64, f64), f64)> {
        let (d, o) = (self.direction()?, self.origin());
        let s = (at.0 - o.0) * d.0 + (at.1 - o.1) * d.1;
        let foot = (o.0 + s * d.0, o.1 + s * d.1);
        Some((foot, (at.0 - foot.0).hypot(at.1 - foot.1)))
    }
}

/// The nearest point Guide or crossing of two line Guides caught by `at` within `capture`
/// on both axes, with the Guide it belongs to.
pub(super) fn point(at: (f64, f64), guides: &[PlacedGuide], capture: Nm) -> Option<(Point, Uuid)> {
    let r = capture.0 as f64;
    let lines: Vec<(&PlacedGuide, (f64, f64))> = guides
        .iter()
        .filter_map(|g| Some((g, g.direction()?)))
        .collect();
    let mut points: Vec<((f64, f64), Uuid)> = guides
        .iter()
        .filter(|g| g.angle.is_none())
        .map(|g| (g.origin(), g.object))
        .collect();
    for (i, (a, da)) in lines.iter().enumerate() {
        for (b, db) in &lines[i + 1..] {
            let det = da.0 * db.1 - da.1 * db.0;
            if det.abs() < 1e-12 {
                continue;
            }
            let (oa, ob) = (a.origin(), b.origin());
            let s = ((ob.0 - oa.0) * db.1 - (ob.1 - oa.1) * db.0) / det;
            points.push(((oa.0 + s * da.0, oa.1 + s * da.1), a.object));
        }
    }
    let d = |p: (f64, f64)| (p.0 - at.0).hypot(p.1 - at.1);
    points
        .into_iter()
        .filter(|(p, _)| (p.0 - at.0).abs() <= r && (p.1 - at.1).abs() <= r)
        .min_by(|a, b| d(a.0).total_cmp(&d(b.0)))
        .map(|(p, object)| (round(p), object))
}

/// Where `at` lands on the nearest line Guide within `capture` of it: the Guide's crossing
/// with the grid line nearest its foot, or the foot when `grid` is `None`.
pub(super) fn on_line(
    at: (f64, f64),
    guides: &[PlacedGuide],
    capture: Nm,
    grid: Option<Grid>,
) -> Option<(Point, Uuid)> {
    let (guide, foot) = guides
        .iter()
        .filter_map(|g| g.foot(at).map(|(foot, d)| (g, foot, d)))
        .filter(|(_, _, d)| *d <= capture.0 as f64)
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(g, foot, _)| (g, foot))?;
    let Some(grid) = grid else {
        return Some((round(foot), guide.object));
    };
    let (o, d) = (guide.origin(), guide.direction()?);
    let near = grid.nearest(foot);
    let mut crossings = Vec::new();
    // With a vertical grid line, unless the Guide runs along it.
    if d.0.abs() > 1e-12 {
        let x = near.x.0 as f64;
        crossings.push((x, o.1 + (x - o.0) * d.1 / d.0));
    }
    // With a horizontal grid line, unless the Guide runs along it.
    if d.1.abs() > 1e-12 {
        let y = near.y.0 as f64;
        crossings.push((o.0 + (y - o.1) * d.0 / d.1, y));
    }
    let from_foot = |p: &(f64, f64)| (p.0 - foot.0).hypot(p.1 - foot.1);
    let best = crossings
        .into_iter()
        .min_by(|a, b| from_foot(a).total_cmp(&from_foot(b)))?;
    Some((round(best), guide.object))
}

fn round(p: (f64, f64)) -> Point {
    Point::new(Nm(p.0.round() as i64), Nm(p.1.round() as i64))
}
