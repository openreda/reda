//! Resolve solid connections against the actual poured copper before fitting Teardrops.
use super::{fill, layer_order, poly};
use crate::{Copper, Item, drill::DrillHole};
use reda_model::{ConnectionStyle, PcbDocument, Uuid};
use std::collections::BTreeMap;

pub(crate) struct SolidPours<'a> {
    pcb: &'a PcbDocument,
    copper: &'a Copper,
    holes: &'a [DrillHole],
    layers: BTreeMap<Uuid, Vec<fill::Filled<'a>>>,
}

impl<'a> SolidPours<'a> {
    pub(crate) fn new(pcb: &'a PcbDocument, copper: &'a Copper, holes: &'a [DrillHole]) -> Self {
        Self {
            pcb,
            copper,
            holes,
            layers: BTreeMap::new(),
        }
    }

    pub(crate) fn connects(&mut self, net: Option<Uuid>, layer: Uuid, target: Item) -> bool {
        if matches!(target, Item::Track { .. })
            || net.is_none()
            || !self
                .pcb
                .active_zones()
                .any(|z| z.layer == layer && z.net == net)
            || fill::connection_style(self.pcb, net, layer, target) != Some(ConnectionStyle::Solid)
        {
            return false;
        }
        let Some((_, land)) = self
            .copper
            .item_pieces(target)
            .into_iter()
            .find(|(l, _)| *l == layer)
        else {
            return false;
        };
        let poured = self.layers.entry(layer).or_insert_with(|| {
            let mut poured = Vec::new();
            for zone in layer_order(self.pcb, layer)
                .into_iter()
                .filter(|z| !z.shelved)
            {
                let area = fill::fill_zone(self.pcb, self.copper, zone, &poured, self.holes);
                poured.push(fill::Filled { zone, area });
            }
            poured
        });
        let land = poly::union(poly::grown(land, 1));
        poured
            .iter()
            .any(|f| f.zone.net == net && poly::overlaps(&land, &f.area))
    }
}
