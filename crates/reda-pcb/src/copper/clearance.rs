//! The clearance query: would proposed copper come too close to copper of another Net?
//!
//! The required clearance between two pieces of copper is the larger of their Net Classes'
//! clearances (the board-wide rule where a class sets none). Copper on no Net keeps its
//! clearance from everything, other unassigned copper included.

use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, TrackGeometry, Uuid};
use rstar::AABB;
use schemars::JsonSchema;
use serde::Serialize;

use super::shape::Shape;
use super::{Copper, Item};
use crate::net_rules::{clearance_between, net_rules};

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
                let required = clearance_between(pcb, net, other);
                let gap = shape.distance(&piece.shape);
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

/// The largest clearance any Net Class asks for.
fn max_clearance(pcb: &PcbDocument) -> Nm {
    pcb.net_classes()
        .filter_map(|c| c.clearance)
        .chain([net_rules(pcb, None).clearance, pcb.setup().rules.clearance])
        .max()
        .unwrap_or(Nm::ZERO)
}
