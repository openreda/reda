//! Where Units placed by a Command rather than by hand go: in a row beside what the sheet
//! already holds, so they cover nothing.

use reda_model::geom::{Nm, Point};
use reda_model::{SchematicSheet, Symbol, Uuid};

use crate::DEFAULT_SNAP_GRID;
use crate::edit::bounds::{content, symbol_extent};

/// The space left between Units in the row, and between the row and existing content.
const GAP: Nm = Nm::from_mil(400);

/// Where the row starts on an empty sheet.
const EMPTY_START: Nm = Nm::from_mil(2000);

/// An origin for each of `units` (a Symbol and the Unit of it to place), unturned and
/// unmirrored, in a row to the right of everything on `body`, its top level with the
/// content's (or at a fixed spot on an empty sheet). Each origin is on the snap grid.
pub fn beside_content(body: &SchematicSheet, units: &[(&Symbol, Uuid)]) -> Vec<Point> {
    let existing = content(body);
    let mut cursor = existing.map_or(EMPTY_START, |b| b.max.x + GAP);
    let level = existing.map_or(EMPTY_START, |b| b.min.y);
    units
        .iter()
        .map(|(symbol, unit)| {
            let extent = symbol_extent(symbol, *unit);
            let at = Point::new(
                snap_up(cursor - extent.min.x),
                snap_up(level - extent.min.y),
            );
            cursor = at.x + extent.max.x + GAP;
            at
        })
        .collect()
}

/// The first grid point at or above `value`.
fn snap_up(value: Nm) -> Nm {
    let grid = DEFAULT_SNAP_GRID.0;
    Nm(value.0.div_euclid(grid) * grid
        + if value.0.rem_euclid(grid) == 0 {
            0
        } else {
            grid
        })
}
