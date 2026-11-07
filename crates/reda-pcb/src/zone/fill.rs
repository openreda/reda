//! Filling one Zone: its outline, cut back from the board edge, Keepouts, other Nets'
//! copper and higher-priority fills, thinned to the minimum width, joined to its Pads by
//! thermal spokes or solidly, with unconnected islands dropped. A Pad's own
//! `zone_connection` values win over the Zone's; its spoke count and angle wait for the
//! `zone_connection` rule.

use crate::copper::shape::BoardShape as _;
use reda_model::geom::{Millideg, Nm};
use reda_model::{
    ConnectionStyle, FillShape, PadConnection, PadKind, PcbDocument, Uuid, Zone, ZoneConnection,
};

use super::poly::{
    self, Area, Contour, bounds, difference, dilated, edge_band, grown, intersection, ip, opened,
    overlaps, union,
};
use crate::copper::shape::{self as copper_shape, Core, P, Prim, Shape};
use crate::copper::{Copper, Item};
use crate::placed;
use crate::rules::{clearance_between, net_rules};

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
    area = within_board(pcb, zone, area);
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
            let frame = pad_frame(pcb, item);
            let connection = frame.as_ref().map(|f| f.connection).unwrap_or_default();
            let style = connection.style.unwrap_or(match zone.pad_connection {
                PadConnection::Thermal => ConnectionStyle::Relief,
                PadConnection::Solid => ConnectionStyle::Solid,
            });
            let gap = connection.gap.unwrap_or(zone.thermal_gap);
            match (style, frame) {
                (ConnectionStyle::Relief, Some(frame)) => {
                    connectors.push(shape);
                    gaps.extend(grown(shape, gap.0));
                    thermal_pads.push(frame);
                }
                (ConnectionStyle::None, Some(_)) => gaps.extend(grown(shape, gap.0)),
                _ => connectors.push(shape),
            }
            continue;
        }
        let clearance = zone
            .clearance
            .max(clearance_between(pcb, zone.net, net).unwrap_or(Nm::ZERO));
        if !near(shape, clearance.0, lo, hi) {
            continue;
        }
        others.extend(grown(shape, clearance.0));
    }
    // Bare (non-plated) holes are no copper, but the fill keeps its clearance from them.
    for hole in bare_holes(pcb) {
        if near(&hole, zone.clearance.0, lo, hi) {
            others.extend(grown(&hole, zone.clearance.0));
        }
    }
    for prior in before {
        if same_net(prior.zone.net) {
            continue;
        }
        let clearance = zone
            .clearance
            .max(prior.zone.clearance)
            .max(clearance_between(pcb, zone.net, prior.zone.net).unwrap_or(Nm::ZERO));
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
        .flat_map(|frame| spokes(zone, frame))
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
/// keeping the zone's Net's edge clearance from both. Without an outline, only the
/// cutouts cut.
fn within_board(pcb: &PcbDocument, zone: &Zone, mut area: Area) -> Area {
    let edge = net_rules(pcb, zone.net)
        .edge_clearance
        .unwrap_or(Nm::ZERO)
        .0;
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

/// Where a Pad sits on the board: its copper's centre, its turn, its copper's half sizes
/// and its own Zone connection values.
struct PadFrame {
    centre: P,
    angle: Millideg,
    half: (i64, i64),
    connection: ZoneConnection,
}

/// The frame of a Pad or Free Pad item; `None` for other copper.
fn pad_frame(pcb: &PcbDocument, item: Item) -> Option<PadFrame> {
    let placed = match item {
        Item::Pad { footprint, pad } => {
            let instance = pcb.footprint(footprint)?;
            placed::placed_pads(pcb, instance)
                .into_iter()
                .find(|p| p.pad == pad)?
        }
        Item::FreePad { uuid } => placed::placed_free_pad(pcb.free_pad(uuid)?),
        _ => return None,
    };
    let centre = placed.copper_center();
    Some(PadFrame {
        centre: [centre.x.0, centre.y.0],
        angle: placed.rotation,
        half: (placed.outline.width.0 / 2, placed.outline.height.0 / 2),
        connection: placed.zone_connection.unwrap_or_default(),
    })
}

/// The outlines of the non-plated Pads (Footprint and Free), which have no copper.
fn bare_holes(pcb: &PcbDocument) -> Vec<Shape> {
    let mut holes = Vec::new();
    for instance in pcb.footprints() {
        if let Some(copy) = pcb.footprint_copy(instance.footprint) {
            holes.extend(
                copy.footprint
                    .pads
                    .iter()
                    .filter(|p| p.kind == PadKind::NonPlated)
                    .map(|p| Shape::pad(instance, p)),
            );
        }
    }
    holes.extend(
        pcb.free_pads()
            .filter(|p| p.kind == PadKind::NonPlated)
            .map(Shape::free_pad),
    );
    holes
}

/// The four thermal spokes of a Pad: bars of the spoke width from its centre along its
/// own axes, out past the thermal gap into the fill.
fn spokes(zone: &Zone, frame: &PadFrame) -> Vec<Contour> {
    let gap = frame.connection.gap.unwrap_or(zone.thermal_gap);
    let width = frame
        .connection
        .spoke_width
        .unwrap_or(zone.thermal_spoke_width);
    let reach = gap.0 + zone.min_width.0.max(width.0);
    let half = width.0 / 2;
    let (hw, hh) = (frame.half.0 + reach, frame.half.1 + reach);
    let bars = [
        [[0, -half], [hw, -half], [hw, half], [0, half]],
        [[-hw, -half], [0, -half], [0, half], [-hw, half]],
        [[-half, 0], [half, 0], [half, hh], [-half, hh]],
        [[-half, -hh], [half, -hh], [half, 0], [-half, 0]],
    ];
    bars.iter()
        .map(|bar| {
            bar.iter()
                .map(|p| {
                    let p = copper_shape::rotate(*p, frame.angle);
                    ip([p[0] + frame.centre[0], p[1] + frame.centre[1]])
                })
                .collect()
        })
        .collect()
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
