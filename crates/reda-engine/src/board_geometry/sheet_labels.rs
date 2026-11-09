//! The derived sheet-name labels: each Schematic Sheet's name above that sheet's
//! Footprints that lie entirely outside the Board Outline. View-only and never saved, so
//! renaming a sheet updates its label, and the label goes once its parts are on the board.

use std::collections::BTreeMap;

use reda_model::geom::{Nm, Point};
use reda_model::{ComponentRef, DocumentKind, PcbDocument, Project, Uuid};
use reda_pcb::placed::footprint_bounds;
use reda_render::SheetLabel;

/// How far above its group's top edge a label's baseline sits: 1 mm.
const GAP: Nm = Nm(1_000_000);

/// One label per sheet with Footprints off the board, in sheet order.
pub(super) fn labels(project: &Project, pcb: &PcbDocument) -> Vec<SheetLabel> {
    let sheets = first_unit_sheets(project);
    let outline: Vec<Point> = pcb
        .outline()
        .map(|o| o.vertices.iter().map(|v| v.point()).collect())
        .unwrap_or_default();
    // Per sheet index: the box around its Footprints outside the outline.
    let mut groups: BTreeMap<usize, (Uuid, Point, Point)> = BTreeMap::new();
    for instance in pcb.footprints() {
        let Some((index, sheet)) = instance.component.as_ref().and_then(|c| sheets.get(c)) else {
            continue;
        };
        let b = footprint_bounds(pcb, instance);
        if !outside(&outline, b.min, b.max) {
            continue;
        }
        let group = groups.entry(*index).or_insert((*sheet, b.min, b.max));
        group.1 = Point::new(group.1.x.min(b.min.x), group.1.y.min(b.min.y));
        group.2 = Point::new(group.2.x.max(b.max.x), group.2.y.max(b.max.y));
    }
    groups
        .into_values()
        .filter_map(|(sheet, lo, hi)| {
            let name = project.document(sheet)?.title.clone();
            Some(SheetLabel {
                sheet,
                name,
                at: Point::new(lo.x, hi.y + GAP),
            })
        })
        .collect()
}

/// Each Component's sheet: its first Unit's (the lowest Symbol Unit, on the earliest
/// sheet), with the sheet's place among the Schematic Sheets.
fn first_unit_sheets(project: &Project) -> BTreeMap<ComponentRef, (usize, Uuid)> {
    let mut best: BTreeMap<ComponentRef, ((usize, usize), Uuid)> = BTreeMap::new();
    let sheets = project
        .documents()
        .filter(|d| d.kind == DocumentKind::Schematic)
        .filter_map(|d| project.schematic_sheet(d.uuid));
    for (sheet_index, sheet) in sheets.enumerate() {
        for unit in sheet.units() {
            let unit_index = sheet
                .part(unit.part)
                .and_then(|e| e.symbol.units.iter().position(|u| u.uuid == unit.unit))
                .unwrap_or(usize::MAX);
            let rank = (unit_index, sheet_index);
            if best.get(&unit.component).is_none_or(|(r, _)| rank < *r) {
                best.insert(unit.component.clone(), (rank, sheet.uuid));
            }
        }
    }
    best.into_iter()
        .map(|(key, ((_, sheet_index), sheet))| (key, (sheet_index, sheet)))
        .collect()
}

/// Whether the box `lo`..`hi` lies entirely outside the closed polygon `outline`; with no
/// outline, everything is outside.
fn outside(outline: &[Point], lo: Point, hi: Point) -> bool {
    if outline.len() < 3 {
        return true;
    }
    let corners = [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)];
    if corners.iter().any(|c| inside(outline, *c)) {
        return false;
    }
    let in_box = |p: &Point| lo.x <= p.x && p.x <= hi.x && lo.y <= p.y && p.y <= hi.y;
    if outline.iter().any(in_box) {
        return false;
    }
    let edges = |pts: &[Point]| -> Vec<(Point, Point)> {
        (0..pts.len())
            .map(|i| (pts[i], pts[(i + 1) % pts.len()]))
            .collect()
    };
    let box_edges = edges(&corners);
    !edges(outline)
        .iter()
        .any(|a| box_edges.iter().any(|b| crosses(*a, *b)))
}

/// Even-odd point in polygon.
fn inside(polygon: &[Point], p: Point) -> bool {
    let mut odd = false;
    let n = polygon.len();
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + n - 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let t = (p.y - a.y).0 as i128 * (b.x - a.x).0 as i128;
            let x = a.x.0 as i128 + t / (b.y - a.y).0 as i128;
            if (p.x.0 as i128) < x {
                odd = !odd;
            }
        }
    }
    odd
}

/// Whether segments `a` and `b` cross or touch.
fn crosses(a: (Point, Point), b: (Point, Point)) -> bool {
    let side = |p: Point, q: Point, r: Point| -> i128 {
        let v = (q.x - p.x).0 as i128 * (r.y - p.y).0 as i128
            - (q.y - p.y).0 as i128 * (r.x - p.x).0 as i128;
        v.signum()
    };
    let (d1, d2) = (side(b.0, b.1, a.0), side(b.0, b.1, a.1));
    let (d3, d4) = (side(a.0, a.1, b.0), side(a.0, a.1, b.1));
    d1 * d2 <= 0 && d3 * d4 <= 0 && !(d1 == 0 && d2 == 0 && d3 == 0 && d4 == 0)
}
