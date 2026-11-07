//! Filling one Zone: its outline, cut back from the board edge, Keepouts, other Nets'
//! copper and higher-priority fills, thinned to the minimum width, joined to the Pads and
//! Vias of its Net as the `zone_connection` rule says, with islands dropped as
//! `remove_unconnected` and `min_island_area` say. `pour_over` decides whether same-Net
//! Tracks and Zones merge with the fill or keep clear of it.

use crate::copper::shape::BoardShape as _;
use reda_model::geom::{Millideg, Nm};
use reda_model::{
    ConnectionStyle, ConnectionTarget, FillShape, PadKind, PcbDocument, PourOver, Uuid, Zone,
    ZoneConnection,
};

use super::poly::{
    self, Area, Contour, bounds, difference, dilated, edge_band, grown, intersection, ip, opened,
    overlaps, union,
};
use crate::copper::shape::{self as copper_shape, Core, P, Prim, Shape};
use crate::copper::{Copper, Item};
use crate::placed;
use crate::rules::{Connection, clearance_between, net_rules, zone_connection};

/// A fill already made on the same Layer by a Zone of higher priority.
pub(super) struct Filled<'a> {
    pub zone: &'a Zone,
    pub area: Area,
}

/// The clearance `zone` keeps from copper on `net`: the rule's, or the Zone's own where
/// that is larger.
fn clearance_to(pcb: &PcbDocument, zone: &Zone, net: Option<Uuid>) -> Nm {
    let rule = clearance_between(pcb, zone.net, net).unwrap_or(Nm::ZERO);
    zone.clearance.map_or(rule, |own| own.max(rule))
}

/// The narrowest copper `zone` may leave: its own, else its Net's `track_width` minimum.
pub(super) fn min_width(pcb: &PcbDocument, zone: &Zone) -> Nm {
    zone.min_width.unwrap_or_else(|| {
        let range = net_rules(pcb, zone.net).track_width;
        range.min.unwrap_or(range.preferred)
    })
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

    let min_width = min_width(pcb, zone);
    let same_net = |net: Option<Uuid>| zone.net.is_some() && net == zone.net;
    let mut others: Vec<Vec<Contour>> = Vec::new();
    let mut gaps: Vec<Vec<Contour>> = Vec::new();
    let mut relieved = Vec::new();
    let mut connectors: Vec<&Shape> = Vec::new();
    for (item, shape) in copper.pieces_on(layer) {
        let net = copper.net(item);
        if same_net(net) {
            if let Some(frame) = frame(pcb, zone, item) {
                match frame.connection.style {
                    ConnectionStyle::Relief => {
                        connectors.push(shape);
                        gaps.extend(grown(shape, frame.connection.gap.0));
                        relieved.push(frame);
                    }
                    ConnectionStyle::None => gaps.extend(grown(shape, frame.connection.gap.0)),
                    ConnectionStyle::Solid => connectors.push(shape),
                }
                continue;
            }
            if zone.pour_over == PourOver::All {
                connectors.push(shape);
                continue;
            }
        }
        let clearance = clearance_to(pcb, zone, net);
        if !near(shape, clearance.0, lo, hi) {
            continue;
        }
        others.extend(grown(shape, clearance.0));
    }
    // Bare (non-plated) holes are no copper, but the fill keeps its clearance from them.
    let hole_clearance = clearance_to(pcb, zone, None);
    for hole in bare_holes(pcb) {
        if near(&hole, hole_clearance.0, lo, hi) {
            others.extend(grown(&hole, hole_clearance.0));
        }
    }
    let merges = |prior: &Filled<'_>| same_net(prior.zone.net) && zone.pour_over != PourOver::None;
    for prior in before {
        if merges(prior) {
            continue;
        }
        let rule = clearance_between(pcb, zone.net, prior.zone.net).unwrap_or(Nm::ZERO);
        let clearance = [zone.clearance, prior.zone.clearance]
            .into_iter()
            .flatten()
            .fold(rule, Nm::max);
        others.extend(dilated(&prior.area, clearance.0));
    }

    let allowed = difference(&area, &union(others));
    let mut fill = if gaps.is_empty() {
        allowed.clone()
    } else {
        difference(&allowed, &union(gaps))
    };
    fill = opened(&fill, min_width.0 / 2);

    let spokes: Vec<Vec<Contour>> = relieved
        .iter()
        .flat_map(|frame| spokes(frame, min_width))
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

    if zone.remove_unconnected && zone.net.is_some() {
        let touch: Vec<Area> = connectors
            .iter()
            .map(|s| union(grown(s, 1)))
            .chain(before.iter().filter(|p| merges(p)).map(|p| p.area.clone()))
            .collect();
        fill.retain(|island| {
            let island = vec![island.clone()];
            touch.iter().any(|t| overlaps(&island, t))
        });
    }
    if zone.min_island_area > 0 {
        fill.retain(|island| shape_area(island) >= zone.min_island_area as f64);
    }
    fill
}

/// The area of one shape (outer contour less holes), in square nanometres.
pub(super) fn shape_area(shape: &[Contour]) -> f64 {
    let ring = |ring: &Contour| {
        let n = ring.len();
        ((0..n)
            .map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                a.x as f64 * b.y as f64 - b.x as f64 * a.y as f64
            })
            .sum::<f64>()
            / 2.0)
            .abs()
    };
    let mut rings = shape.iter();
    let outer = rings.next().map_or(0.0, ring);
    outer - rings.map(ring).sum::<f64>()
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

/// Where a Pad or Via sits on the board: its copper's centre, its turn, its copper's half
/// sizes and how the fill joins it.
struct Frame {
    centre: P,
    angle: Millideg,
    half: (i64, i64),
    connection: Connection,
}

/// The frame of a Pad, Free Pad or Via item, with its resolved Zone connection; `None`
/// for other copper.
fn frame(pcb: &PcbDocument, zone: &Zone, item: Item) -> Option<Frame> {
    let placed = match item {
        Item::Pad { footprint, pad } => {
            let instance = pcb.footprint(footprint)?;
            placed::placed_pads(pcb, instance)
                .into_iter()
                .find(|p| p.pad == pad)?
        }
        Item::FreePad { uuid } => placed::placed_free_pad(pcb.free_pad(uuid)?),
        Item::Via { uuid } => {
            let via = pcb.via(uuid)?;
            let r = via.diameter.0 / 2;
            return Some(Frame {
                centre: [via.at.x.0, via.at.y.0],
                angle: Millideg::ZERO,
                half: (r, r),
                connection: zone_connection(pcb, zone.net, ConnectionTarget::Via, None),
            });
        }
        _ => return None,
    };
    let target = match placed.kind {
        PadKind::Smd => ConnectionTarget::Smd,
        PadKind::PlatedThroughHole | PadKind::NonPlated => ConnectionTarget::ThroughHole,
    };
    let own: Option<ZoneConnection> = placed.zone_connection;
    let centre = placed.copper_center();
    Some(Frame {
        centre: [centre.x.0, centre.y.0],
        angle: placed.rotation,
        half: (placed.outline.width.0 / 2, placed.outline.height.0 / 2),
        connection: zone_connection(pcb, zone.net, target, own.as_ref()),
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

/// The spokes of a relieved Pad or Via: 2 or 4 bars of the spoke width from its centre,
/// turned by the connection's angle in its own frame, out past the gap into the fill.
/// With 2 spokes they run along the frame's x axis (turned by the angle).
fn spokes(frame: &Frame, min_width: Nm) -> Vec<Contour> {
    let c = frame.connection;
    let reach = (c.gap.0 + min_width.0.max(c.spoke_width.0)) as f64;
    let half = (c.spoke_width.0 / 2) as f64;
    let (hw, hh) = (frame.half.0 as f64, frame.half.1 as f64);
    let step = 360_000 / i64::from(c.spokes.max(1));
    (0..i64::from(c.spokes))
        .map(|k| {
            let a = ((c.angle.0 + k * step) as f64 / 1000.0).to_radians();
            let (dx, dy) = (a.cos(), a.sin());
            // How far the direction runs inside the copper's box before it leaves it.
            let exit = match (dx.abs() < 1e-9, dy.abs() < 1e-9) {
                (true, _) => hh,
                (_, true) => hw,
                _ => (hw / dx.abs()).min(hh / dy.abs()),
            };
            let len = exit + reach;
            let (nx, ny) = (-dy * half, dx * half);
            [
                (nx, ny),
                (dx * len + nx, dy * len + ny),
                (dx * len - nx, dy * len - ny),
                (-nx, -ny),
            ]
            .iter()
            .map(|&(x, y)| {
                let p = copper_shape::rotate([x.round() as i64, y.round() as i64], frame.angle);
                ip([p[0] + frame.centre[0], p[1] + frame.centre[1]])
            })
            .collect()
        })
        .collect()
}

/// Stored shapes back as an area.
pub(super) fn to_area(shapes: &[FillShape]) -> Area {
    shapes
        .iter()
        .map(|shape| {
            std::iter::once(&shape.outer)
                .chain(&shape.holes)
                .map(|ring| ring.iter().map(|p| ip([p.x.0, p.y.0])).collect())
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
