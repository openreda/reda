//! Plated bores indexed on their physical Layers, including where no land survives.
use super::shape::{BoardShape as _, P, Shape};
use super::{Copper, Entry, Item, Proposal};
use crate::drill::holes;
use crate::placed::{placed_free_pad, placed_pads};
use reda_model::{PcbDocument, RuleField, RuleKind, Uuid, geom::Nm};
use rstar::{AABB, RTree};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(super) struct Bore {
    pub item: Item,
    pub field: RuleField,
    pub net: Option<Uuid>,
    pub shape: Shape,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Bores {
    bores: Vec<Bore>,
    layers: BTreeMap<Uuid, RTree<Entry>>,
}

impl Bores {
    pub fn of(pcb: &PcbDocument) -> Self {
        let mut rows = Self::default();
        for h in holes(pcb).into_iter().filter(|h| h.ring.is_some()) {
            let slot = rows.bores.len();
            let envelope = h.shape.envelope();
            for layer in h.layers(pcb) {
                rows.layers
                    .entry(layer)
                    .or_default()
                    .insert(Entry { slot, envelope });
            }
            rows.bores.push(Bore {
                field: h.field(),
                item: h.item,
                net: h.net,
                shape: h.shape,
            });
        }
        rows
    }

    pub fn near(&self, layer: Uuid, search: AABB<P>) -> impl Iterator<Item = &Bore> {
        self.layers
            .get(&layer)
            .into_iter()
            .flat_map(move |tree| tree.locate_in_envelope_intersecting(search))
            .map(|entry| &self.bores[entry.slot])
    }
}

pub(super) fn clearance(
    pcb: &PcbDocument,
    a: Option<Uuid>,
    b: Option<Uuid>,
    layer: Uuid,
    field: RuleField,
) -> Nm {
    crate::rules::resolve_pair_on(pcb, a, b, Some(layer), RuleKind::Clearance, field)
        .and_then(|s| s.value.length())
        .unwrap_or(Nm::ZERO)
}

impl Copper {
    /// A Track endpoint inside the design land restores it when the edit commits.
    pub(super) fn joins_bore(
        &self,
        pcb: &PcbDocument,
        layer: Uuid,
        bore: &Bore,
        proposal: &Proposal,
    ) -> bool {
        if !self.unused.contains(bore.item, layer) {
            return true;
        }
        let Proposal::Track { geometry, .. } = proposal else {
            return false;
        };
        let side = pcb.layers().layer(layer).and_then(|l| l.side);
        let land = match bore.item {
            Item::Via { uuid } => pcb
                .via(uuid)
                .map(|v| Shape::disc(v.at, v.diameter_on(layer))),
            Item::FreePad { uuid } => pcb
                .free_pad(uuid)
                .map(|p| super::lands::pad_shape(&placed_free_pad(p), side)),
            Item::Pad { footprint, pad } => pcb
                .footprint(footprint)
                .and_then(|f| placed_pads(pcb, f).into_iter().find(|p| p.pad == pad))
                .map(|p| super::lands::pad_shape(&p, side)),
            _ => None,
        };
        let (start, end) = geometry.ends();
        land.is_some_and(|shape| shape.contains(start) || shape.contains(end))
    }
}
