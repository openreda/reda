//! Cluster packing: where Sync puts new Footprints, and where Arrange Outside Board puts
//! the ones it re-clusters, so both look alike.
//!
//! A cluster goes to the right of the Board Outline and of every Footprint already placed,
//! or at the origin when there is neither. It is one block per Schematic Sheet, stacked top
//! to bottom in the order given. Inside a block the Footprints fill rows left to right in
//! the order given, on the top side at rotation 0, with [`GAP`] between their boxes. A
//! row's target width is √(sum of the block's box areas) × 1.5. See
//! `docs/spec/v0.1/sync.md#placement-of-new-footprints`.

use std::collections::BTreeSet;

use reda_model::geom::{Bounds, Nm, Point};
use reda_model::{Footprint, PcbDocument, Uuid};

use crate::placed::{footprint_bounds, rotate, shape_bounds};

/// The space between neighbouring boxes in a block, and between its rows.
pub const GAP: Nm = Nm::from_mm(1);
/// The space between the board (or what is already placed) and a cluster, and between
/// stacked blocks, leaving room for a block's sheet-name label.
pub const BLOCK_GAP: Nm = Nm::from_mm(5);

/// The box a Footprint takes in a cluster, relative to its origin, on the top side at
/// rotation 0: its Courtyard's, or the box around its Pads and graphics when it has no
/// Courtyard. A Footprint with neither is a point at its origin.
pub fn footprint_box(footprint: &Footprint) -> Bounds {
    let courtyard = footprint
        .graphics
        .iter()
        .filter(|g| g.layer.is_courtyard())
        .filter_map(|g| shape_bounds(&g.shape))
        .reduce(Bounds::union);
    courtyard.unwrap_or_else(|| {
        let pads = footprint.pads.iter().map(|p| {
            let corners = p
                .outline()
                .box_corners()
                .map(|c| rotate(c + p.at, p.at, p.rotation));
            Bounds::of_points(corners).expect("four corners")
        });
        let graphics = footprint
            .graphics
            .iter()
            .filter_map(|g| shape_bounds(&g.shape));
        pads.chain(graphics)
            .reduce(Bounds::union)
            .unwrap_or(Bounds::of_point(Point::default()))
    })
}

/// The origin of each Footprint in `blocks`, block by block in the order given, for a new
/// cluster on `pcb`. It stays clear of the Board Outline and of every placed Footprint
/// except those in `moving` (the ones being re-clustered).
pub fn pack(
    pcb: &PcbDocument,
    moving: &BTreeSet<Uuid>,
    blocks: &[Vec<&Footprint>],
) -> Vec<Vec<Point>> {
    let (left, mut top) = match occupied(pcb, moving) {
        Some(b) => (b.max.x + BLOCK_GAP, b.max.y),
        None => (Nm::ZERO, Nm::ZERO),
    };
    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        let boxes: Vec<Bounds> = block.iter().map(|f| footprint_box(f)).collect();
        let (origins, bottom) = pack_block(&boxes, left, top);
        out.push(origins);
        if !boxes.is_empty() {
            top = bottom - BLOCK_GAP;
        }
    }
    out
}

/// Packs `boxes` into rows from the top-left corner (`left`, `top`). Returns each box's
/// origin and the block's bottom edge.
fn pack_block(boxes: &[Bounds], left: Nm, top: Nm) -> (Vec<Point>, Nm) {
    let width = row_width(boxes);
    let mut out = Vec::with_capacity(boxes.len());
    let (mut x, mut row_top) = (left, top);
    let mut row_height = Nm::ZERO;
    for b in boxes {
        if x > left && x + b.width() - left > width {
            x = left;
            row_top = row_top - row_height - GAP;
            row_height = Nm::ZERO;
        }
        out.push(Point::new(x - b.min.x, row_top - b.max.y));
        x = x + b.width() + GAP;
        row_height = row_height.max(b.height());
    }
    (out, row_top - row_height)
}

/// A row's target width: √(sum of the boxes' areas) × 1.5, in whole nanometres.
fn row_width(boxes: &[Bounds]) -> Nm {
    let area: u128 = boxes
        .iter()
        .map(|b| b.width().0.unsigned_abs() as u128 * b.height().0.unsigned_abs() as u128)
        .sum();
    Nm((isqrt(area) * 3 / 2) as i64)
}

/// ⌊√n⌋, exactly.
fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = (n as f64).sqrt() as u128;
    while x * x > n {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= n {
        x += 1;
    }
    x
}

/// What a new cluster must stay clear of: the Board Outline and every placed Footprint
/// not in `moving`.
fn occupied(pcb: &PcbDocument, moving: &BTreeSet<Uuid>) -> Option<Bounds> {
    let outline = pcb
        .outline()
        .and_then(|o| Bounds::of_points(o.vertices.iter().map(|v| v.point())));
    let footprints = pcb
        .footprints()
        .filter(|i| !moving.contains(&i.uuid))
        .map(|i| footprint_bounds(pcb, i));
    outline.into_iter().chain(footprints).reduce(Bounds::union)
}
