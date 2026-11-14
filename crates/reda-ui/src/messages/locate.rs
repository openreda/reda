//! Where a Violation is on its Document's canvas: the point its marker sits on, and the
//! box "go to" zooms to.

use reda_engine::model::geom::{Bounds, Nm, Point};
use reda_engine::model::{Uuid, Violation};
use reda_engine::render::Scene;

/// The smallest box "go to" frames, so a Violation at one point is not zoomed in on
/// absurdly far: 10 mm.
const MIN_FRAME: i64 = 10_000_000;

/// Where a Violation is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location {
    /// The marker's point.
    pub at: Point,
    /// What to zoom to: the objects involved and the marker, at least 10 mm across.
    pub frame: (Point, Point),
}

/// Uses the check's stored marker position. Scene geometry only determines the zoom
/// frame; it never moves a marker after a design edit or an object's deletion.
pub fn locate(document: Uuid, scene: &Scene, violation: &Violation) -> Option<Location> {
    if violation.document != Some(document) {
        return None;
    }
    let at = violation.at?;
    let frame = (violation.objects.iter())
        .flat_map(|o| scene.items_of(*o))
        .filter_map(|i| i.primitive.bounds())
        .map(|(min, max)| Bounds { min, max })
        .fold(Bounds::of_point(at), Bounds::union);
    Some(Location {
        at,
        frame: at_least(frame.min, frame.max, MIN_FRAME),
    })
}

/// `lo`..`hi` grown about its centre to at least `min` nm on each side.
fn at_least(lo: Point, hi: Point, min: i64) -> (Point, Point) {
    let grow = |a: Nm, b: Nm| {
        let short = (min - (b.0 - a.0)).max(0);
        (Nm(a.0 - short / 2), Nm(b.0 + short - short / 2))
    };
    let (x0, x1) = grow(lo.x, hi.x);
    let (y0, y1) = grow(lo.y, hi.y);
    (Point::new(x0, y0), Point::new(x1, y1))
}
