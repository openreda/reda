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
use crate::rules::{clearance_between, largest};
use reda_model::{RuleField, RuleKind};

/// Copper the routing Tool is about to add.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proposal {
    /// A Track segment on one copper Layer.
    Track {
        layer: Uuid,
        geometry: TrackGeometry,
        width: Nm,
        net: Option<Uuid>,
    },
    /// A through Via, on every copper Layer.
    Via {
        at: Point,
        diameter: Nm,
        net: Option<Uuid>,
    },
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
        let (layers, shape, net) = match *proposal {
            Proposal::Track {
                layer,
                geometry,
                width,
                net,
            } => (vec![layer], Shape::track(&geometry, width), net),
            Proposal::Via { at, diameter, net } => (
                pcb.copper_layers().map(|l| l.uuid).collect(),
                Shape::disc(at, diameter),
                net,
            ),
        };
        let reach = max_clearance(pcb).0;
        let env = shape.envelope();
        let (lo, hi) = (env.lower(), env.upper());
        let search = AABB::from_corners(
            [lo[0] - reach, lo[1] - reach],
            [hi[0] + reach, hi[1] + reach],
        );
        let ties = super::ties::Ties::of(pcb);
        let mut hits = Vec::new();
        for layer in layers {
            for piece in self.candidates(layer, search) {
                let other = self.net(piece.item);
                // Zone fills make way for new copper when they are refilled.
                if matches!(piece.item, Item::Zone { .. })
                    || (net.is_some() && other == net)
                    || ignore.contains(&piece.item.object())
                {
                    continue;
                }
                let required = clearance_between(pcb, net, other).unwrap_or(Nm::ZERO);
                let gap = shape.distance(&piece.shape);
                // A Net Tie's other Pads need no clearance, but touching one is a short.
                if ties.tie_pad_on(net, piece.item) && !shape.touches(&piece.shape) {
                    continue;
                }
                if gap < required.0 as f64 {
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
        hits.sort_by_key(|h| (h.item, h.layer));
        hits.dedup();
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
        let envelope = match near {
            Some((at, reach)) => AABB::from_corners(
                [at.x.0 - reach.0, at.y.0 - reach.0],
                [at.x.0 + reach.0, at.y.0 + reach.0],
            ),
            None => AABB::from_corners([i64::MIN / 4, i64::MIN / 4], [i64::MAX / 4, i64::MAX / 4]),
        };
        let mut out: Vec<Halo> = Vec::new();
        for piece in self.candidates(layer, envelope) {
            let other = self.net(piece.item);
            if matches!(piece.item, Item::Zone { .. }) || (net.is_some() && other == net) {
                continue;
            }
            if let Some((at, reach)) = near
                && piece.shape.distance(&Shape::disc(at, Nm::ZERO)) > reach.0 as f64
            {
                continue;
            }
            let clearance = clearance_between(pcb, net, other).unwrap_or(Nm::ZERO);
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
        out.sort_by_key(|h| h.item);
        out
    }
}

/// Two pieces of stored copper closer than the clearance rules allow, without touching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gap {
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
    /// the clearance rules allow but does not touch, sorted by pair. Touching copper is a
    /// short (different Nets) or a connection, never a clearance breach. Zone fills are
    /// left out: a current fill keeps its clearance by construction.
    pub(crate) fn gaps(&self, pcb: &PcbDocument) -> Vec<Gap> {
        let reach = max_clearance(pcb).0;
        let ties = super::ties::Ties::of(pcb);
        let mut found: std::collections::BTreeMap<(Item, Item), Gap> = Default::default();
        for (&layer, tree) in &self.index {
            for entry in tree.iter() {
                let piece = self.piece(entry.slot);
                if matches!(piece.item, Item::Zone { .. }) {
                    continue;
                }
                let (lo, hi) = (entry.envelope.lower(), entry.envelope.upper());
                let search = AABB::from_corners(
                    [lo[0] - reach, lo[1] - reach],
                    [hi[0] + reach, hi[1] + reach],
                );
                for other in self.candidates(layer, search) {
                    if other.item <= piece.item || matches!(other.item, Item::Zone { .. }) {
                        continue;
                    }
                    let (na, nb) = (self.net(piece.item), self.net(other.item));
                    if na.is_some() && na == nb {
                        continue;
                    }
                    if piece.shape.touches(&other.shape)
                        || ties.no_clearance((piece.item, na), (other.item, nb))
                    {
                        continue;
                    }
                    let Some(required) = clearance_between(pcb, na, nb) else {
                        continue;
                    };
                    if same_footprint_pads(piece.item, other.item)
                        && ignores_pad_to_pad(pcb, na, nb)
                    {
                        continue;
                    }
                    let gap = piece.shape.distance(&other.shape);
                    if gap >= required.0 as f64 {
                        continue;
                    }
                    let actual = Nm(gap.max(0.0).floor() as i64);
                    let gap = Gap {
                        a: piece.item,
                        b: other.item,
                        required,
                        actual,
                    };
                    found
                        .entry((piece.item, other.item))
                        .and_modify(|g| g.actual = g.actual.min(actual))
                        .or_insert(gap);
                }
            }
        }
        found.into_values().collect()
    }
}

/// The largest clearance any enabled record asks for.
fn max_clearance(pcb: &PcbDocument) -> Nm {
    largest(pcb, RuleKind::Clearance, RuleField::Value)
}

/// Whether `a` and `b` are two Pads of one placed Footprint.
fn same_footprint_pads(a: Item, b: Item) -> bool {
    matches!(
        (a, b),
        (Item::Pad { footprint: fa, .. }, Item::Pad { footprint: fb, .. }) if fa == fb
    )
}

/// Whether the clearance rule of either Net lets Pads within one Footprint sit closer.
fn ignores_pad_to_pad(pcb: &PcbDocument, a: Option<Uuid>, b: Option<Uuid>) -> bool {
    use crate::rules::net_rules;
    net_rules(pcb, a).ignore_pad_to_pad || net_rules(pcb, b).ignore_pad_to_pad
}
