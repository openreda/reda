//! Where new Footprints go: each Sync places them in a fresh cluster to the right of the
//! Board Outline and of every Footprint already placed (at the origin when there is
//! neither), one row per Schematic Sheet in sheet order, sorted by Designator within it.

use reda_model::geom::{Bounds, Nm, Point};
use reda_model::{Footprint, PcbDocument, Shape};

use super::components::SchComponent;

/// The space between the board (or earlier clusters) and a new cluster.
const CLUSTER_GAP: Nm = Nm::from_mm(5);
/// The space between Footprints, and between rows, within a cluster.
const SPACING: Nm = Nm::from_mm(2);

fn union_all(boxes: impl IntoIterator<Item = Bounds>) -> Option<Bounds> {
    boxes.into_iter().reduce(Bounds::union)
}

/// A box around a Footprint's Pads and drawing, relative to its origin, at any Pad
/// rotation. A Footprint with neither is a point.
fn footprint_bounds(footprint: &Footprint) -> Bounds {
    let pads = footprint.pads.iter().map(|p| {
        // Half the diagonal bounds the Pad at any rotation.
        let half = Nm(p.width.0.max(p.height.0));
        Bounds::around(p.at, Nm(half.0 * 3 / 4 + 1))
    });
    let graphics = footprint
        .graphics
        .iter()
        .filter_map(|g| shape_bounds(&g.shape));
    union_all(pads.chain(graphics)).unwrap_or(Bounds::of_point(Point::new(Nm(0), Nm(0))))
}

fn shape_bounds(shape: &Shape) -> Option<Bounds> {
    match shape {
        Shape::Line { start, end } | Shape::Rect { start, end } => {
            Some(Bounds::of_point(*start).union(Bounds::of_point(*end)))
        }
        Shape::Arc { center, start, .. } => {
            let (dx, dy) = ((start.x - center.x).0 as f64, (start.y - center.y).0 as f64);
            Some(Bounds::around(*center, Nm(dx.hypot(dy).ceil() as i64)))
        }
        Shape::Circle { center, radius } => Some(Bounds::around(*center, *radius)),
        Shape::Polygon { points } => union_all(points.iter().copied().map(Bounds::of_point)),
        Shape::Text { at, .. } => Some(Bounds::of_point(*at)),
    }
}

/// What a new cluster must stay clear of: the Board Outline and every placed Footprint.
fn occupied(pcb: &PcbDocument) -> Option<Bounds> {
    let outline = pcb
        .outline()
        .into_iter()
        .flat_map(|o| o.vertices.iter().map(|v| Bounds::of_point(v.point())));
    let footprints = pcb.footprints().filter_map(|i| {
        let copy = pcb.footprint_copy(i.footprint)?;
        let b = footprint_bounds(&copy.footprint);
        // Any rotation or side stays within the box's bounding circle.
        let r = [b.min.x.0, b.min.y.0, b.max.x.0, b.max.y.0]
            .into_iter()
            .map(i64::abs)
            .max()
            .unwrap_or(0);
        Some(Bounds::around(i.at, Nm(r * 3 / 2)))
    });
    union_all(outline.chain(footprints))
}

/// The origin of each new Footprint, in the order given (which must be sheet order, then
/// Designator order).
pub(crate) fn place(pcb: &PcbDocument, new: &[&SchComponent]) -> Vec<Point> {
    let (left, mut top) = match occupied(pcb) {
        Some(b) => (b.max.x + CLUSTER_GAP, b.max.y),
        None => (Nm(0), Nm(0)),
    };
    let mut out = Vec::with_capacity(new.len());
    let mut x = left;
    let mut row_height = Nm(0);
    let mut sheet = new.first().map(|c| c.sheet_index);
    for c in new {
        if Some(c.sheet_index) != sheet {
            sheet = Some(c.sheet_index);
            top = top - row_height - SPACING;
            x = left;
            row_height = Nm(0);
        }
        let b = footprint_bounds(&c.copy.footprint);
        let at = Point::new(x - b.min.x, top - b.max.y);
        out.push(at);
        let placed = b.moved(at);
        x = placed.max.x + SPACING;
        row_height = row_height.max(placed.max.y - placed.min.y);
    }
    out
}
