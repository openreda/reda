//! Resolve solid connections against the actual poured copper before fitting Teardrops.
use super::{fill, layer_order, poly};
use crate::{Copper, Item, drill::DrillHole};
use reda_model::geom::Point;
use reda_model::{ConnectionStyle, PcbDocument, Uuid};
use std::collections::BTreeMap;

pub(crate) struct SolidPours<'a> {
    pcb: &'a PcbDocument,
    copper: &'a Copper,
    holes: &'a [DrillHole],
    layers: BTreeMap<Uuid, Vec<(Uuid, poly::ZoneArea)>>,
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

    pub(crate) fn connects(
        &mut self,
        net: Option<Uuid>,
        layer: Uuid,
        target: Item,
        at: Point,
    ) -> bool {
        let Some(net) = net else { return false };
        if matches!(target, Item::Track { .. })
            || !self
                .pcb
                .active_zones()
                .any(|z| z.layer == layer && z.net == Some(net))
            || fill::connection_style(self.pcb, Some(net), layer, target)
                != Some(ConnectionStyle::Solid)
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
                .into_iter()
                .filter_map(|f| Some((f.zone.net?, poly::ZoneArea::new(f.area)?)))
                .collect()
        });
        let bounds = land.bounds();
        poured
            .iter()
            .any(|(zone_net, area)| *zone_net == net && area.covers(land, at, bounds))
    }
}
