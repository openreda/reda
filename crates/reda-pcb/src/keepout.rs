//! Keepouts as geometry: the area a Keepout covers, the clearance copper keeps from it,
//! and which Keepouts an object enters. Zone fill, DRC and the live routing check share
//! these, so they agree on what is allowed.
//!
//! Copper keeps the normal clearance from a Keepout: the clearance between its Net and no
//! Net. The board-wide `clearance` rule's `zero_keepout_clearance` makes every Keepout
//! edge-exact. Touching an edge-exact Keepout is allowed; overlapping it never is.

use reda_model::geom::{Millideg, Nm, Point, Vertex};
use reda_model::{Keepout, KeepoutShape, PadKind, PcbDocument, Restriction, RuleField, RuleKind, Uuid};

use crate::copper::shape::{BoardShape as _, Shape, arc_points};
use reda_model::geom::grown::TOUCH;
use crate::copper::{Item, Proposal};
use crate::rules::{clearance_between, resolve_pair};
use crate::zone::poly::{self, Area, Contour};

/// The area `keepout` covers: inside its polygon, or within half the width of its line.
pub(crate) fn area(keepout: &Keepout) -> Area {
    match &keepout.shape {
        KeepoutShape::Polygon { vertices } => poly::polygon(poly::contour(vertices)),
        KeepoutShape::Line { vertices, width } => {
            poly::union(poly::grown(&centreline(vertices, *width), 0))
        }
    }
}

/// Polygons containing every point within `distance` of `keepout`'s area.
pub(crate) fn grown(keepout: &Keepout, distance: i64) -> Vec<Vec<Contour>> {
    match &keepout.shape {
        KeepoutShape::Polygon { .. } => poly::dilated(&area(keepout), distance),
        KeepoutShape::Line { vertices, width } => {
            poly::grown(&centreline(vertices, *width), distance)
        }
    }
}

/// A line Keepout's copper-like shape: its centreline, arcs as chords, grown by half the
/// width.
fn centreline(vertices: &[Vertex], width: Nm) -> Shape {
    let mut points = Vec::new();
    for pair in vertices.windows(2) {
        let (from, to) = (pair[0].point(), pair[1].point());
        if pair[0].sweep == Millideg::ZERO {
            points.push(from);
        } else {
            let arc = arc_points(from, to, pair[0].sweep);
            points.extend_from_slice(&arc[..arc.len() - 1]);
        }
    }
    points.extend(vertices.last().map(Vertex::point));
    Shape::chain(&points, width.0 / 2)
}

/// Whether the board-wide `clearance` rule makes every Keepout edge-exact.
pub fn zero_clearance(pcb: &PcbDocument) -> bool {
    pcb.board_rule(RuleKind::Clearance)
        .values
        .zero_keepout_clearance
        .unwrap_or(false)
}

/// The clearance copper on `net` keeps from a Keepout, and the rule record it comes from:
/// the clearance between `net` and no Net, or 0 under `zero_keepout_clearance`.
pub fn clearance(pcb: &PcbDocument, net: Option<Uuid>) -> (Nm, Uuid) {
    let board = pcb.board_rule(RuleKind::Clearance).uuid;
    if zero_clearance(pcb) {
        return (Nm::ZERO, board);
    }
    let required = clearance_between(pcb, net, None).unwrap_or(Nm::ZERO);
    let rule = resolve_pair(pcb, net, None, RuleKind::Clearance, RuleField::Value)
        .map_or(board, |s| s.rule);
    (required, rule)
}

/// The kind of object a piece of copper is, as Keepouts restrict it; `None` for Zone
/// fills, which are cut from Keepouts when they are made.
pub(crate) fn restriction(pcb: &PcbDocument, item: Item) -> Option<Restriction> {
    let pad = |kind: PadKind| match kind {
        PadKind::Smd => Restriction::SmdPads,
        PadKind::PlatedThroughHole | PadKind::NonPlated => Restriction::ThtPads,
    };
    match item {
        Item::Track { .. } => Some(Restriction::Tracks),
        Item::Via { .. } => Some(Restriction::Vias),
        Item::Zone { .. } => None,
        Item::FreePad { uuid } => pcb.free_pad(uuid).map(|p| pad(p.kind)),
        Item::Pad { footprint, pad: id } => {
            let instance = pcb.footprint(footprint)?;
            let copy = pcb.footprint_copy(instance.footprint)?;
            copy.footprint
                .pads
                .iter()
                .find(|p| p.uuid == id)
                .map(|p| pad(p.kind))
        }
    }
}

/// An object too close to, or inside, a Keepout that restricts it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KeepoutHit {
    pub keepout: Uuid,
    /// The copper Layers where it comes too close.
    pub layers: Vec<Uuid>,
    /// The clearance required.
    pub required: Nm,
    /// The gap there is, 0 where they overlap.
    pub actual: Nm,
    /// The closest points, on the Keepout and on the object.
    pub points: [Point; 2],
    /// The rule record the clearance comes from.
    pub rule: Uuid,
}

/// A Keepout's shape for measuring: a line as copper-like capsules, a polygon as its area
/// and its edges.
enum Measure {
    Line(Shape),
    Polygon { area: Area, edges: Shape, corners: Vec<Point> },
}

impl Measure {
    fn of(keepout: &Keepout) -> Measure {
        match &keepout.shape {
            KeepoutShape::Line { vertices, width } => Measure::Line(centreline(vertices, *width)),
            KeepoutShape::Polygon { vertices } => {
                let area = area(keepout);
                Measure::Polygon {
                    edges: poly::boundary(&area),
                    corners: vertices.iter().map(Vertex::point).collect(),
                    area,
                }
            }
        }
    }

    fn bounds(&self) -> (crate::copper::shape::P, crate::copper::shape::P) {
        match self {
            Measure::Line(shape) => shape.bounds(),
            Measure::Polygon { edges, .. } => edges.bounds(),
        }
    }

    /// The exact gap to `copper`, negative where they overlap, and the closest points.
    fn gap(&self, copper: &Shape) -> (f64, [Point; 2]) {
        match self {
            Measure::Line(shape) => (shape.distance(copper), shape.closest_points(copper)),
            Measure::Polygon {
                area,
                edges,
                corners,
            } => {
                let d = edges.distance(copper);
                let points = edges.closest_points(copper);
                if d < 0.0 {
                    return (d, points);
                }
                // Wholly inside the Keepout, or wholly around it.
                let inside = copper
                    .prims
                    .iter()
                    .flat_map(|p| p.core.points())
                    .map(|p| Point::new(Nm(p[0]), Nm(p[1])))
                    .find(|p| poly::inside(area, *p))
                    .or_else(|| {
                        corners.iter().copied().find(|c| {
                            copper.distance(&Shape::disc(*c, Nm::ZERO)) < -TOUCH
                        })
                    });
                match inside {
                    Some(p) => (-1.0, [p, p]),
                    None => (d, points),
                }
            }
        }
    }
}

/// The Keepouts restricting `kind` that copper `pieces` (each on a copper Layer) on `net`
/// enters or comes closer to than the clearance, sorted by Keepout. At no clearance,
/// touching the edge passes.
pub(crate) fn entered(
    pcb: &PcbDocument,
    pieces: &[(Uuid, &Shape)],
    kind: Restriction,
    net: Option<Uuid>,
) -> Vec<KeepoutHit> {
    let mut out = Vec::new();
    let mut need = None;
    for keepout in pcb.keepouts().filter(|k| k.restricts(kind)) {
        let on: Vec<&(Uuid, &Shape)> = pieces
            .iter()
            .filter(|(l, _)| keepout.on_layer(*l))
            .collect();
        if on.is_empty() {
            continue;
        }
        let (required, rule) = *need.get_or_insert_with(|| clearance(pcb, net));
        let measure = Measure::of(keepout);
        let bounds = measure.bounds();
        let mut layers = Vec::new();
        let mut closest: Option<(f64, [Point; 2])> = None;
        for (layer, shape) in on {
            if !poly::boxes_within(bounds, shape.bounds(), required.0) {
                continue;
            }
            let (gap, points) = measure.gap(shape);
            if gap + TOUCH < required.0 as f64 {
                layers.push(*layer);
                if closest.is_none_or(|(g, _)| gap < g) {
                    closest = Some((gap, points));
                }
            }
        }
        if let Some((gap, points)) = closest {
            out.push(KeepoutHit {
                keepout: keepout.uuid,
                layers,
                required,
                actual: Nm(gap.max(0.0).floor() as i64),
                points,
                rule,
            });
        }
    }
    out
}

/// The Keepouts `proposal` would enter or come closer to than the clearance, sorted: a
/// Track where one restricts `tracks` on its Layer, a Via where one restricts `vias` on
/// any copper Layer.
pub fn keepouts_hit(pcb: &PcbDocument, proposal: &Proposal) -> Vec<Uuid> {
    let (shape, layers, kind, net) = match *proposal {
        Proposal::Track {
            layer,
            geometry,
            width,
            net,
        } => (
            Shape::track(&geometry, width),
            vec![layer],
            Restriction::Tracks,
            net,
        ),
        Proposal::Via { at, diameter, net } => (
            Shape::disc(at, diameter),
            pcb.copper_layers().map(|l| l.uuid).collect(),
            Restriction::Vias,
            net,
        ),
    };
    let pieces: Vec<(Uuid, &Shape)> = layers.into_iter().map(|l| (l, &shape)).collect();
    entered(pcb, &pieces, kind, net)
        .into_iter()
        .map(|h| h.keepout)
        .collect()
}
