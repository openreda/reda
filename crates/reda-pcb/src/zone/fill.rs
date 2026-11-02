//! Filling one Zone: its outline, cut back from the board edge, Keepouts, other Nets'
//! copper and higher-priority fills, thinned to the minimum width, joined to its Pads by
//! thermal spokes or solidly, with unconnected islands dropped.

use reda_model::geom::Millideg;
use reda_model::{FillShape, PadConnection, PcbDocument, Side, Uuid, Zone};

use super::poly::{
    self, Area, Contour, bounds, difference, dilated, edge_band, grown, intersection, ip, opened,
    overlaps, union,
};
use crate::copper::shape::{self as copper_shape, Core, P, Prim, Shape};
use crate::copper::{Copper, Item};
use crate::net_rules::clearance_between;

/// A fill already made on the same Layer by a Zone of higher priority.
pub(super) struct Filled<'a> {
    pub zone: &'a Zone,
    pub area: Area,
}

/// The fill of `zone`, as `i_overlay` shapes.
pub(super) fn fill_zone(
    pcb: &PcbDocument,
    copper: &Copper,
    zone: &Zone,
    before: &[Filled<'_>],
) -> Area {
    let layer = zone.layer;
    let mut area = poly::polygon(poly::contour(&zone.vertices));
    area = within_board(pcb, area);
    let keepouts: Vec<Vec<Contour>> = pcb
        .keepouts()
        .filter(|k| k.copper && k.layers.contains(&layer))
        .map(|k| vec![poly::contour(&k.vertices)])
        .collect();
    if !keepouts.is_empty() {
        area = difference(&area, &union(keepouts));
    }
    let Some((lo, hi)) = bounds(&area) else {
        return Vec::new();
    };

    let same_net = |net: Option<Uuid>| zone.net.is_some() && net == zone.net;
    let mut others: Vec<Vec<Contour>> = Vec::new();
    let mut gaps: Vec<Vec<Contour>> = Vec::new();
    let mut thermal_pads = Vec::new();
    let mut connectors: Vec<&Shape> = Vec::new();
    for (item, shape) in copper.pieces_on(layer) {
        let net = copper.net(item);
        if same_net(net) {
            connectors.push(shape);
            if zone.pad_connection == PadConnection::Thermal
                && let Item::Pad { footprint, pad } = item
            {
                gaps.extend(grown(shape, zone.thermal_gap.0));
                thermal_pads.push((footprint, pad));
            }
            continue;
        }
        let clearance = zone.clearance.max(clearance_between(pcb, zone.net, net));
        if !near(shape, clearance.0, lo, hi) {
            continue;
        }
        others.extend(grown(shape, clearance.0));
    }
    for prior in before {
        if same_net(prior.zone.net) {
            continue;
        }
        let clearance = zone
            .clearance
            .max(prior.zone.clearance)
            .max(clearance_between(pcb, zone.net, prior.zone.net));
        others.extend(dilated(&prior.area, clearance.0));
    }

    let allowed = difference(&area, &union(others));
    let mut fill = if gaps.is_empty() {
        allowed.clone()
    } else {
        difference(&allowed, &union(gaps))
    };
    fill = opened(&fill, zone.min_width.0 / 2);

    let spokes: Vec<Vec<Contour>> = thermal_pads
        .iter()
        .filter_map(|(footprint, pad)| spokes(pcb, zone, *footprint, *pad))
        .flatten()
        .map(|c| vec![c])
        .collect();
    let mut kept = Vec::new();
    for spoke in spokes {
        let piece = intersection(&vec![spoke], &allowed);
        if overlaps(&piece, &fill) {
            kept.extend(piece);
        }
    }
    if !kept.is_empty() {
        fill.extend(kept);
        fill = union(fill);
    }

    if zone.remove_islands && zone.net.is_some() {
        let touch: Vec<Area> = connectors
            .iter()
            .map(|s| union(grown(s, 1)))
            .chain(
                before
                    .iter()
                    .filter(|p| same_net(p.zone.net))
                    .map(|p| p.area.clone()),
            )
            .collect();
        fill.retain(|island| {
            let island = vec![island.clone()];
            touch.iter().any(|t| overlaps(&island, t))
        });
    }
    fill
}

/// Whether `shape` grown by `clearance` can reach the box `lo`–`hi`.
fn near(shape: &Shape, clearance: i64, lo: P, hi: P) -> bool {
    let env = shape.envelope();
    let (a, b) = (env.lower(), env.upper());
    a[0] - clearance <= hi[0]
        && b[0] + clearance >= lo[0]
        && a[1] - clearance <= hi[1]
        && b[1] + clearance >= lo[1]
}

/// `area` cut back to the board: inside the Board Outline and outside the Board Cutouts,
/// keeping the edge clearance from both. Without an outline, only the cutouts cut.
fn within_board(pcb: &PcbDocument, mut area: Area) -> Area {
    let edge = pcb.setup().rules.edge_clearance.0;
    let mut cut: Vec<Vec<Contour>> = Vec::new();
    if let Some(outline) = pcb.outline() {
        let board = poly::polygon(poly::contour(&outline.vertices));
        area = intersection(&area, &board);
        cut.extend(edge_band(&board, edge));
    }
    for cutout in pcb.cutouts() {
        let hole = poly::polygon(poly::contour(&cutout.vertices));
        cut.extend(dilated(&hole, edge));
    }
    if cut.is_empty() {
        area
    } else {
        difference(&area, &union(cut))
    }
}

/// The four thermal spokes of a Pad: bars of the spoke width from its centre along its
/// own axes, out past the thermal gap into the fill.
fn spokes(pcb: &PcbDocument, zone: &Zone, footprint: Uuid, pad: Uuid) -> Option<Vec<Contour>> {
    let instance = pcb.footprint(footprint)?;
    let copy = pcb.footprint_copy(instance.footprint)?;
    let pad = copy.footprint.pads.iter().find(|p| p.uuid == pad)?;
    let centre = copper_shape::board_point(instance, [pad.at.x.0, pad.at.y.0]);
    let angle = match instance.side {
        Side::Top => instance.rotation + pad.rotation,
        Side::Bottom => instance.rotation + Millideg(-pad.rotation.0),
    };
    let reach = zone.thermal_gap.0 + zone.min_width.0.max(zone.thermal_spoke_width.0);
    let half = zone.thermal_spoke_width.0 / 2;
    let (hw, hh) = (pad.width.0 / 2 + reach, pad.height.0 / 2 + reach);
    let bars = [
        [[0, -half], [hw, -half], [hw, half], [0, half]],
        [[-hw, -half], [0, -half], [0, half], [-hw, half]],
        [[-half, 0], [half, 0], [half, hh], [-half, hh]],
        [[-half, -hh], [half, -hh], [half, 0], [-half, 0]],
    ];
    Some(
        bars.iter()
            .map(|bar| {
                bar.iter()
                    .map(|p| {
                        let p = copper_shape::rotate(*p, angle);
                        ip([p[0] + centre[0], p[1] + centre[1]])
                    })
                    .collect()
            })
            .collect(),
    )
}

/// The fill as stored shapes.
pub(super) fn to_shapes(area: &Area) -> Vec<FillShape> {
    area.iter()
        .map(|shape| FillShape {
            outer: shape[0].iter().map(|p| poly::point(*p)).collect(),
            holes: shape[1..]
                .iter()
                .map(|h| h.iter().map(|p| poly::point(*p)).collect())
                .collect(),
        })
        .collect()
}

/// A shape of the fill as copper primitives: the triangles of its triangulation.
pub(crate) fn shape_copper(shape: &FillShape) -> Shape {
    use i_triangle::int::triangulatable::IntTriangulatable;
    let rings: Vec<Contour> = std::iter::once(&shape.outer)
        .chain(&shape.holes)
        .map(|ring| ring.iter().map(|p| ip([p.x.0, p.y.0])).collect())
        .collect();
    let triangulation = rings.triangulate().into_triangulation::<u32>();
    let points = triangulation.points;
    let prims = triangulation
        .indices
        .chunks(3)
        .map(|t| Prim {
            core: Core::Poly(
                t.iter()
                    .map(|i| {
                        let p = points[*i as usize];
                        [p.x, p.y]
                    })
                    .collect(),
            ),
            radius: 0,
        })
        .collect();
    Shape { prims }
}
