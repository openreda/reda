//! The clearance query: would proposed copper come too close to copper of another Net?
//!
//! The required clearance between two pieces of copper is the larger of their Net Classes'
//! clearances (the board-wide rule where a class sets none). Copper on no Net keeps its
//! clearance from everything, other unassigned copper included.

use crate::copper::shape::BoardShape as _;
use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, TrackGeometry, Uuid};
use rstar::AABB;
use schemars::JsonSchema;
use serde::Serialize;

use super::shape::Shape;
use super::{Copper, Item};
use crate::rules::{clearance_between_on, largest, resolve_pair_on};
use reda_model::{RuleField, RuleKind};

/// Copper the routing Tool is about to add.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proposal {
    /// A Track segment on one copper Layer.
    Track {
        layer: Uuid,
        geometry: TrackGeometry,
        width: Nm,
        net: Option<Uuid>,
    },
    /// A Via on the copper Layers of its span.
    Via {
        span: reda_model::LayerSpan,
        /// Layers traversed by the bore without a copper land.
        omitted: Vec<Uuid>,
        at: Point,
        diameter: reda_model::ViaDiameter,
        hole: Nm,
        net: Option<Uuid>,
    },
}

impl Proposal {
    /// Whether the proposal contains copper on this Layer of its span.
    pub fn has_land(&self, layer: Uuid) -> bool {
        !matches!(self, Self::Via { omitted, .. } if omitted.contains(&layer))
    }

    /// The physical span and retained land diameters of a proposed Via, in board order.
    pub fn via_layers(&self, pcb: &PcbDocument) -> Vec<(Uuid, Option<Nm>)> {
        let Self::Via { span, diameter, .. } = *self else {
            return Vec::new();
        };
        pcb.span_layers(span)
            .into_iter()
            .map(|layer| {
                (
                    layer,
                    self.has_land(layer).then(|| diameter.on(span, layer)),
                )
            })
            .collect()
    }

    /// The physical outline on every traversed Layer, including the bare bore.
    pub(crate) fn pieces(&self, pcb: &PcbDocument) -> (Vec<(Uuid, Shape)>, Option<Uuid>) {
        match *self {
            Proposal::Track {
                layer,
                geometry,
                width,
                net,
            } => (vec![(layer, Shape::track(&geometry, width))], net),
            Proposal::Via { at, hole, net, .. } => (
                self.via_layers(pcb)
                    .into_iter()
                    .map(|(layer, land)| (layer, Shape::disc(at, land.unwrap_or(hole))))
                    .collect(),
                net,
            ),
        }
    }
}

/// Copper of another Net that a proposal comes too close to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ClearanceHit {
    pub item: Item,
    /// The copper Layer where they come too close.
    pub layer: Uuid,
    /// The other copper's Net; left out for none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    /// The clearance the rules require: the larger of the two Net Classes'.
    pub required: Nm,
    /// The gap there is, 0 where the copper overlaps.
    pub actual: Nm,
}

impl Copper {
    /// The physical proposal for a new Via, including connections from unfinished Tracks.
    pub fn propose_via(
        &self,
        pcb: &PcbDocument,
        at: Point,
        net: Option<Uuid>,
        via: crate::route::RouteVia,
        route_ends: &[(Uuid, Point)],
    ) -> Proposal {
        let intent = super::lands::ViaIntent {
            via: None,
            at,
            net,
            span: via.span,
            diameter: via.diameter,
            mode: via.unused_layers,
            connection: None,
        };
        Proposal::Via {
            at,
            net,
            span: via.span,
            diameter: via.diameter,
            hole: via.hole,
            omitted: intent.omitted(pcb, &self.land_connections, route_ends),
        }
    }

    /// Every piece of copper that `proposal` would come closer to than the clearance
    /// rules allow, sorted by item and Layer. Copper of the proposal's own Net never
    /// conflicts, and the objects in `ignore` (such as the segment being dragged) are left
    /// out, and so are Zone fills, which make way when they are refilled. An empty result
    /// means the proposal is clear.
    pub fn clearance(
        &self,
        pcb: &PcbDocument,
        proposal: &Proposal,
        ignore: &[Uuid],
    ) -> Vec<ClearanceHit> {
        let (pieces, net) = proposal.pieces(pcb);
        let reach = max_clearance(pcb)
            .0
            .max(largest(pcb, RuleKind::Clearance, RuleField::HoleVia).0)
            .max(largest(pcb, RuleKind::Clearance, RuleField::HolePad).0);
        let planned_bore = match *proposal {
            Proposal::Via { at, hole, .. } => Some(Shape::disc(at, hole)),
            Proposal::Track { .. } => None,
        };
        let bores = self.bores.get_or_init(|| super::bores::Bores::of(pcb));
        let ties = super::ties::Ties::of(pcb);
        let mut hits = Vec::new();
        for (layer, shape) in pieces {
            let has_land = proposal.has_land(layer);
            let mut env = shape.envelope();
            if let Some(bore) = &planned_bore {
                use rstar::Envelope;
                env.merge(&bore.envelope());
            }
            let (lo, hi) = (env.lower(), env.upper());
            let search = AABB::from_corners(
                [lo[0] - reach, lo[1] - reach],
                [hi[0] + reach, hi[1] + reach],
            );
            for bore in bores.near(layer, search).filter(|_| has_land) {
                if ignore.contains(&bore.item.object())
                    || ties.tie_pad_on(net, bore.item)
                    || (net.is_some()
                        && net == bore.net
                        && self.joins_bore(pcb, layer, bore, proposal))
                {
                    continue;
                }
                let required = super::bores::clearance(pcb, net, bore.net, layer, bore.field);
                let gap = shape.distance(&bore.shape);
                if gap < required.0 as f64 {
                    hits.push(ClearanceHit {
                        item: bore.item,
                        layer,
                        net: bore.net,
                        required,
                        actual: Nm(gap.max(0.).floor() as i64),
                    });
                }
            }
            for piece in self.candidates(layer, search) {
                let other = self.net(piece.item);
                // Zone fills make way for new copper when they are refilled.
                if matches!(piece.item, Item::Zone { .. })
                    || (has_land && net.is_some() && other == net)
                    || ignore.contains(&piece.item.object())
                {
                    continue;
                }
                let required =
                    clearance_between_on(pcb, net, other, Some(layer)).unwrap_or(Nm::ZERO);
                let gap = shape.distance(&piece.shape);
                // A Net Tie's other Pads need no clearance, but touching one is a short.
                if ties.tie_pad_on(net, piece.item) && !shape.touches(&piece.shape) {
                    continue;
                }
                if let Some(bore) = &planned_bore
                    && !ties.tie_pad_on(net, piece.item)
                {
                    let bore_limit =
                        super::bores::clearance(pcb, net, other, layer, RuleField::HoleVia);
                    let actual = bore.distance(&piece.shape);
                    if actual < bore_limit.0 as f64 {
                        hits.push(ClearanceHit {
                            item: piece.item,
                            layer,
                            net: other,
                            required: bore_limit,
                            actual: Nm(actual.max(0.).floor() as i64),
                        });
                    }
                }
                if has_land
                    && (gap < required.0 as f64
                        || (piece.item.is_artwork() && gap <= reda_model::geom::grown::TOUCH))
                {
                    hits.push(ClearanceHit {
                        item: piece.item,
                        layer,
                        net: other,
                        required,
                        actual: Nm(gap.max(0.0).floor() as i64),
                    });
                }
            }
        }
        // One hit per item and Layer, showing the measurement furthest below its limit.
        hits.sort_by_key(|h| (h.item, h.layer, h.actual.0 - h.required.0));
        hits.dedup_by_key(|h| (h.item, h.layer));
        hits
    }
}

/// Other-Net copper grown by the clearance a route on one Net must keep from it: the
/// Track Tool's clearance display (`C`). Where the route's centre line may not go is the
/// halo grown by a further half route width.
#[derive(Clone, Debug, PartialEq)]
pub struct Halo {
    pub item: Item,
    /// The copper's outline, each primitive grown by `clearance`.
    pub shape: reda_model::geom::grown::Shape,
    pub clearance: Nm,
}

impl Copper {
    /// The clearance halos on `layer` for a route on `net`: every piece of copper of
    /// another Net (or of no Net) other than Zone fills, grown by the clearance between the
    /// two Nets, sorted by item. With `near`, only copper within that distance of the
    /// point.
    pub fn halos(
        &self,
        pcb: &PcbDocument,
        layer: Uuid,
        net: Option<Uuid>,
        near: Option<(Point, Nm)>,
    ) -> Vec<Halo> {
        self.route_halos(pcb, layer, net, near, None)
    }

    /// Halos for a proposed Via land on this Layer. A larger bore-to-copper gap
    /// can set the exclusion boundary even where copper-to-copper clearance is small.
    pub fn via_halos(
        &self,
        pcb: &PcbDocument,
        layer: Uuid,
        net: Option<Uuid>,
        near: Option<(Point, Nm)>,
        diameter: Option<Nm>,
        hole: Nm,
    ) -> Vec<Halo> {
        self.route_halos(pcb, layer, net, near, Some((diameter, hole)))
    }

    fn route_halos(
        &self,
        pcb: &PcbDocument,
        layer: Uuid,
        net: Option<Uuid>,
        near: Option<(Point, Nm)>,
        via_size: Option<(Option<Nm>, Nm)>,
    ) -> Vec<Halo> {
        let envelope = match near {
            Some((at, reach)) => AABB::from_corners(
                [at.x.0 - reach.0, at.y.0 - reach.0],
                [at.x.0 + reach.0, at.y.0 + reach.0],
            ),
            None => AABB::from_corners([i64::MIN / 4, i64::MIN / 4], [i64::MAX / 4, i64::MAX / 4]),
        };
        let ties = super::ties::Ties::of(pcb);
        let mut out: Vec<Halo> = Vec::new();
        let bare_bore = matches!(via_size, Some((None, _)));
        for piece in self.candidates(layer, envelope) {
            let other = self.net(piece.item);
            if matches!(piece.item, Item::Zone { .. })
                || (!bare_bore && net.is_some() && other == net)
                || (bare_bore && ties.tie_pad_on(net, piece.item))
            {
                continue;
            }
            if let Some((at, reach)) = near
                && piece.shape.distance(&Shape::disc(at, Nm::ZERO)) > reach.0 as f64
            {
                continue;
            }
            let clearance = if ties.tie_pad_on(net, piece.item) {
                Nm::ZERO
            } else {
                let copper = clearance_between_on(pcb, net, other, Some(layer)).unwrap_or(Nm::ZERO);
                via_size.map_or(copper, |(diameter, hole)| {
                    let drill = super::bores::clearance(pcb, net, other, layer, RuleField::HoleVia);
                    // The land's outline will be compared to this displayed halo.
                    diameter.map_or(drill, |pad| {
                        copper.max(Nm(drill.0 + hole.0 / 2 - pad.0 / 2))
                    })
                })
            };
            let mut shape = piece.shape.clone();
            for prim in &mut shape.prims {
                prim.radius += clearance.0;
            }
            out.push(Halo {
                item: piece.item,
                shape,
                clearance,
            });
        }
        let bores = self.bores.get_or_init(|| super::bores::Bores::of(pcb));
        let land_halos: std::collections::BTreeMap<_, _> =
            out.iter().enumerate().map(|(i, h)| (h.item, i)).collect();
        for bore in bores.near(layer, envelope).filter(|_| !bare_bore) {
            if (net.is_some() && net == bore.net && !self.unused.contains(bore.item, layer))
                || ties.tie_pad_on(net, bore.item)
                || near.is_some_and(|(at, r)| {
                    bore.shape.distance(&Shape::disc(at, Nm::ZERO)) > r.0 as f64
                })
            {
                continue;
            }
            let clearance = super::bores::clearance(pcb, net, bore.net, layer, bore.field);
            if clearance == Nm::ZERO {
                continue;
            }
            let mut shape = bore.shape.clone();
            for p in &mut shape.prims {
                p.radius += clearance.0;
            }
            if let Some(&slot) = land_halos.get(&bore.item) {
                let existing = &mut out[slot];
                for prim in shape.prims {
                    if let Some(same) = existing
                        .shape
                        .prims
                        .iter_mut()
                        .find(|p| p.core == prim.core)
                    {
                        same.radius = same.radius.max(prim.radius);
                    } else {
                        existing.shape.prims.push(prim);
                    }
                }
                existing.clearance = existing.clearance.max(clearance);
            } else {
                out.push(Halo {
                    item: bore.item,
                    shape,
                    clearance,
                });
            }
        }
        out.sort_by_key(|h| h.item);
        out
    }
}

/// Two pieces of stored copper that violate the clearance rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Gap {
    pub layers: Vec<Uuid>,
    pub rule: Uuid,
    /// The lesser item of the pair.
    pub a: Item,
    pub b: Item,
    /// The clearance the rules require: the larger of the two Net Classes'.
    pub required: Nm,
    /// The smallest gap on any Layer they share, floored to the nanometre.
    pub actual: Nm,
}

impl Copper {
    /// Every pair of copper pieces of different Nets (or on no Net) that comes closer than
    /// the clearance rules allow, sorted by pair. Touching artwork violates clearance
    /// since it never connects. Other contacts are handled by the connection checks.
    /// Zone fills keep clear by construction, but are also checked against artwork.
    pub(crate) fn gaps(&self, pcb: &PcbDocument) -> Vec<Gap> {
        let reach = max_clearance(pcb).0;
        let ties = super::ties::Ties::of(pcb);
        let mut found: std::collections::BTreeMap<(Item, Item, Uuid), Gap> = Default::default();
        for (&layer, tree) in &self.index {
            for entry in tree.iter() {
                let piece = self.piece(entry.slot);
                // Zone fills are checked from the artwork side of each pair. Copper near
                // a Plated Edge keeps the larger of this clearance and `edge_clearance`,
                // which the edge check measures.
                if matches!(piece.item, Item::Zone { .. } | Item::PlatedEdge { .. }) {
                    continue;
                }
                let (lo, hi) = (entry.envelope.lower(), entry.envelope.upper());
                let search = AABB::from_corners(
                    [lo[0] - reach, lo[1] - reach],
                    [hi[0] + reach, hi[1] + reach],
                );
                for other in self.candidates(layer, search) {
                    let artwork = piece.item.is_artwork() || other.item.is_artwork();
                    if other.item <= piece.item
                        || other.item.is_plated_edge()
                        || (!artwork && matches!(other.item, Item::Zone { .. }))
                    {
                        continue;
                    }
                    let (na, nb) = (self.net(piece.item), self.net(other.item));
                    if na.is_some() && na == nb {
                        continue;
                    }

                    let Some(source) = resolve_pair_on(
                        pcb,
                        na,
                        nb,
                        Some(layer),
                        RuleKind::Clearance,
                        RuleField::Value,
                    ) else {
                        continue;
                    };
                    let required = source.value.length().expect("clearance is a length");
                    let gap = piece.shape.distance(&other.shape);
                    if !breaches_clearance(gap, required, artwork)
                        || exempt_pair(pcb, &ties, (piece.item, na), (other.item, nb))
                    {
                        continue;
                    }
                    let actual = Nm(gap.max(0.0).floor() as i64);
                    let gap = Gap {
                        layers: vec![layer],
                        rule: source.rule,
                        a: piece.item,
                        b: other.item,
                        required,
                        actual,
                    };
                    found
                        .entry((piece.item, other.item, source.rule))
                        .and_modify(|g| {
                            g.actual = g.actual.min(actual);
                            if !g.layers.contains(&layer) {
                                g.layers.push(layer);
                            }
                        })
                        .or_insert(gap);
                }
            }
        }
        found.into_values().collect()
    }
}

/// Ordinary copper contacts are handled by connectivity checks. Free artwork has
/// no electrical connection, so its contact is itself a clearance breach.
pub(crate) fn breaches_clearance(distance: f64, required: Nm, artwork: bool) -> bool {
    if distance <= reda_model::geom::grown::TOUCH {
        artwork
    } else {
        distance < required.0 as f64
    }
}

/// The largest clearance any enabled record asks for.
fn max_clearance(pcb: &PcbDocument) -> Nm {
    largest(pcb, RuleKind::Clearance, RuleField::Value)
}

/// Net Ties and the within-Footprint exception apply to both land and hole clearance.
pub(crate) fn exempt_pair(
    pcb: &PcbDocument,
    ties: &super::ties::Ties,
    a: (Item, Option<Uuid>),
    b: (Item, Option<Uuid>),
) -> bool {
    use crate::rules::net_rules;
    ties.no_clearance(a, b)
        || (matches!((a.0, b.0),
            (Item::Pad { footprint: fa, .. }, Item::Pad { footprint: fb, .. }) if fa == fb)
            && (net_rules(pcb, a.1).ignore_pad_to_pad || net_rules(pcb, b.1).ignore_pad_to_pad))
}
