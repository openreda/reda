//! Zones: the fill from the fill cache, then the outline, which is always drawn (shelved
//! Zones are hidden); then Zone Cutouts, outlined on their Layer, and Keepouts, outlined
//! on each copper Layer they apply to (a line Keepout by the edges of the area it covers).

use std::collections::BTreeMap;

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{KeepoutShape, Uuid, ZoneFills};

use super::{Builder, ZoneFillState};
use crate::arc;
use crate::color::BoardColor;
use crate::polygon::triangulate_with_holes;
use crate::scene::{Primitive, SceneLayer};

/// The width of a Zone's outline.
pub(super) const ZONE_OUTLINE: Nm = Nm(100_000);

pub(super) fn build(
    b: &mut Builder,
    fills: Option<&ZoneFills>,
    states: &BTreeMap<Uuid, ZoneFillState>,
) {
    let pcb = b.pcb;
    for zone in pcb.active_zones() {
        let state = states.get(&zone.uuid).copied().unwrap_or_default();
        let shapes = fills
            .and_then(|f| f.zones.get(&zone.uuid))
            .map(|f| &f.shapes);
        let Some(layer) = b.layer(zone.layer) else {
            continue;
        };
        let color = match state {
            ZoneFillState::Filled => BoardColor::layer(layer.kind, layer.side),
            ZoneFillState::Stale => BoardColor::stale_fill(layer.side),
            ZoneFillState::Unfilled => continue,
        };
        let first = b.scene.items.len();
        for shape in shapes.into_iter().flatten() {
            let mesh = triangulate_with_holes(&shape.outer, &shape.holes);
            if !mesh.is_empty() {
                let layer = SceneLayer::Board(zone.layer);
                b.push(zone.uuid, layer, color, Primitive::Triangles(mesh));
            }
        }
        if state == ZoneFillState::Filled {
            b.net_overlay(zone.net, first);
        }
    }
    for zone in pcb.active_zones() {
        let points = arc::contour(pcb.zone_vertices(zone));
        if !points.is_empty() {
            b.polyline(zone.uuid, zone.layer, &points, ZONE_OUTLINE, true);
        }
    }
    for cutout in pcb.zone_cutouts() {
        let points = arc::contour(&cutout.vertices);
        b.polyline(cutout.uuid, cutout.layer, &points, ZONE_OUTLINE, true);
    }
    for keepout in pcb.keepouts() {
        let outlines = keepout_outlines(&keepout.shape);
        for layer in pcb.keepout_layers(keepout) {
            for (points, closed) in &outlines {
                b.polyline(keepout.uuid, layer, points, ZONE_OUTLINE, *closed);
            }
        }
    }
}

/// The lines that outline a Keepout: a polygon's contour, or for a line, the two sides of
/// each segment and a circle round each vertex, so the outline bounds the area it covers.
fn keepout_outlines(shape: &KeepoutShape) -> Vec<(Vec<Point>, bool)> {
    let (vertices, width) = match shape {
        KeepoutShape::Polygon { vertices } => return vec![(arc::contour(vertices), true)],
        KeepoutShape::Line { vertices, width } => (vertices, *width),
    };
    let mut centre: Vec<Point> = Vec::new();
    for pair in vertices.windows(2) {
        let (from, to) = (pair[0].point(), pair[1].point());
        let piece = if pair[0].sweep == Millideg::ZERO {
            vec![from, to]
        } else {
            arc::through(from, to, pair[0].sweep)
        };
        if !centre.is_empty() {
            centre.pop();
        }
        centre.extend(piece);
    }
    let r = width.0 as f64 / 2.0;
    let mut out = Vec::new();
    for w in centre.windows(2) {
        let (dx, dy) = ((w[1].x.0 - w[0].x.0) as f64, (w[1].y.0 - w[0].y.0) as f64);
        let len = dx.hypot(dy);
        if len == 0.0 {
            continue;
        }
        let n = |p: Point, s: f64| {
            Point::new(
                Nm(p.x.0 + (-dy / len * r * s).round() as i64),
                Nm(p.y.0 + (dx / len * r * s).round() as i64),
            )
        };
        out.push((vec![n(w[0], 1.0), n(w[1], 1.0)], false));
        out.push((vec![n(w[0], -1.0), n(w[1], -1.0)], false));
    }
    for v in vertices {
        let p = v.point();
        let start = Point::new(Nm(p.x.0 + width.0 / 2), p.y);
        out.push((arc::about(p, start, Millideg(360_000)), false));
    }
    out
}
