//! Where a Violation is on its Document's canvas: the point its marker sits on, and the
//! box "go to" zooms to.

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{Project, Uuid, Violation, ViolationArg};
use reda_engine::render::Scene;
use reda_engine::schematic::connectivity::pin_position;

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

/// Where `violation` is in `document`, drawn as `scene`. The marker goes, in order of
/// preference, on the point its `x`/`y` arguments name (a dangling end), on the first
/// Pin it names (a Unit followed by one of its Pins), or on the middle of the last object
/// it names that `scene` draws. `None` when none of these is on the Document.
pub fn locate(
    project: &Project,
    document: Uuid,
    scene: &Scene,
    violation: &Violation,
) -> Option<Location> {
    let mut points = Vec::new();
    if let (Some(ViolationArg::Int(x)), Some(ViolationArg::Int(y))) =
        (violation.args.get("x"), violation.args.get("y"))
    {
        points.push(Point::new(Nm(*x), Nm(*y)));
    }
    if let Some(sheet) = project.schematic_sheet(document) {
        for pair in violation.objects.windows(2) {
            let Some(unit) = sheet.unit(pair[0]) else {
                continue;
            };
            let Some(entry) = sheet.part(unit.part) else {
                continue;
            };
            if let Some(pin) = entry
                .symbol
                .unit_pins(unit.unit)
                .find(|p| p.uuid == pair[1])
            {
                points.push(pin_position(unit, pin));
            }
        }
    }
    let objects: Vec<(Point, Point)> = violation
        .objects
        .iter()
        .filter_map(|o| bounds(scene.items_of(*o).filter_map(|i| i.primitive.bounds())))
        .collect();
    let at = match points.first() {
        Some(p) => *p,
        None => {
            let (lo, hi) = *objects.last()?;
            Point::new(Nm((lo.x.0 + hi.x.0) / 2), Nm((lo.y.0 + hi.y.0) / 2))
        }
    };
    let (lo, hi) =
        bounds(objects.into_iter().chain(points.iter().map(|p| (*p, *p)))).unwrap_or((at, at));
    Some(Location {
        at,
        frame: at_least(lo, hi, MIN_FRAME),
    })
}

/// The box around every box in `boxes`.
fn bounds(boxes: impl Iterator<Item = (Point, Point)>) -> Option<(Point, Point)> {
    boxes.reduce(|(alo, ahi), (blo, bhi)| {
        (
            Point::new(alo.x.min(blo.x), alo.y.min(blo.y)),
            Point::new(ahi.x.max(bhi.x), ahi.y.max(bhi.y)),
        )
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
