//! Zones: the fill from the fill cache, then the outline, which is always drawn; then
//! Keepouts, outlined on each copper Layer they apply to.

use std::collections::BTreeMap;

use reda_model::geom::Nm;
use reda_model::{Uuid, ZoneFills};

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
    for zone in pcb.zones() {
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
        for shape in shapes.into_iter().flatten() {
            let mesh = triangulate_with_holes(&shape.outer, &shape.holes);
            if !mesh.is_empty() {
                let layer = SceneLayer::Board(zone.layer);
                b.push(zone.uuid, layer, color, Primitive::Triangles(mesh));
            }
        }
    }
    for zone in pcb.zones() {
        let points = arc::contour(&zone.vertices);
        b.polyline(zone.uuid, zone.layer, &points, ZONE_OUTLINE, true);
    }
    for keepout in pcb.keepouts() {
        let points = arc::contour(&keepout.vertices);
        for layer in &keepout.layers {
            b.polyline(keepout.uuid, *layer, &points, ZONE_OUTLINE, true);
        }
    }
}
