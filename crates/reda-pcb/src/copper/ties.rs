//! The Net Tie rule that DRC and the route clearance query share, based on the Net so
//! they agree however a Track was made: the Pads of one tie may touch, and Tracks and
//! Vias on one of the tie's Nets need no clearance to its other Pads (touching one is
//! still a short). Everything else, Zones included, follows the normal rules, and the
//! Nets stay separate.

use std::collections::{BTreeMap, BTreeSet};

use reda_model::{ComponentType, PcbDocument, Uuid};

use super::Item;

/// The placed Net Ties of a board and the Nets of their Pads.
pub(crate) struct Ties {
    nets: BTreeMap<Uuid, BTreeSet<Uuid>>,
}

impl Ties {
    pub fn of(pcb: &PcbDocument) -> Ties {
        let nets = pcb
            .footprints()
            .filter(|f| f.component_type == ComponentType::NetTie)
            .map(|f| (f.uuid, f.pads.iter().filter_map(|p| p.net).collect()))
            .collect();
        Ties { nets }
    }

    /// Whether `a` and `b` are two Pads of one Net Tie, which may touch.
    pub fn same_tie(&self, a: Item, b: Item) -> bool {
        matches!(
            (a, b),
            (Item::Pad { footprint: fa, .. }, Item::Pad { footprint: fb, .. })
                if fa == fb && self.nets.contains_key(&fa)
        )
    }

    /// Whether `item`, Track or Via copper on `net`, needs no clearance to `other`: a Pad
    /// of a Net Tie with a Pad on `net`.
    pub fn exempt(&self, item: Item, net: Option<Uuid>, other: Item) -> bool {
        matches!(item, Item::Track { .. } | Item::Via { .. }) && self.tie_pad_on(net, other)
    }

    /// Whether `other` is a Pad of a Net Tie with a Pad on `net`, so a Track or Via on
    /// `net` needs no clearance to it.
    pub fn tie_pad_on(&self, net: Option<Uuid>, other: Item) -> bool {
        let Some(net) = net else {
            return false;
        };
        matches!(other, Item::Pad { footprint, .. }
            if self.nets.get(&footprint).is_some_and(|nets| nets.contains(&net)))
    }

    /// Whether the pair needs no clearance between them, either way round.
    pub fn no_clearance(&self, a: (Item, Option<Uuid>), b: (Item, Option<Uuid>)) -> bool {
        self.same_tie(a.0, b.0) || self.exempt(a.0, a.1, b.0) || self.exempt(b.0, b.1, a.0)
    }
}
