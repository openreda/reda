//! Derive omitted lands from the design intent on each Layer, independently of fills.
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::Point;
use reda_model::{
    ConnectionStyle, ConnectionTarget, PcbDocument, Side, UnusedLayers, Uuid, ZoneConnection,
};
use rstar::RTree;

use super::Item;
use super::shape::{BoardShape as _, P, Shape};
use crate::placed::{PlacedPad, placed_free_pad, placed_pads};
use crate::zone::{fill, poly};

/// Physical Pad or Via lands removed by their owner's unused-Layer mode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnusedLands(pub(super) BTreeSet<(Item, Uuid)>);

impl UnusedLands {
    pub fn of(pcb: &PcbDocument) -> Self {
        let mut out = Self::default();
        // A board using the default keep mode needs no endpoint or Zone index.
        let connections = OnceCell::new();
        let mut remove_pad = |item, pad: &PlacedPad| {
            for layer in pcb.copper_layers().filter(|l| l.side.is_none()) {
                let connected = pad.net.is_some_and(|net| {
                    connections.get_or_init(|| Connections::of(pcb)).connected(
                        pcb,
                        (layer.uuid, net),
                        &pad_shape(pad, layer.side),
                        pad.at,
                        ConnectionTarget::ThroughHole,
                        pad.zone_connection.as_ref(),
                    )
                });
                if !connected {
                    out.0.insert((item, layer.uuid));
                }
            }
        };
        for pad in pcb
            .free_pads()
            .filter(|p| p.unused_layers == UnusedLayers::Remove)
        {
            remove_pad(Item::FreePad { uuid: pad.uuid }, &placed_free_pad(pad));
        }
        for instance in pcb.footprints() {
            if pcb
                .footprint_copy(instance.footprint)
                .is_some_and(|c| c.footprint.pads.iter().any(|p| !p.unused_layers.is_keep()))
            {
                for pad in placed_pads(pcb, instance)
                    .iter()
                    .filter(|p| p.unused_layers == UnusedLayers::Remove)
                {
                    remove_pad(
                        Item::Pad {
                            footprint: instance.uuid,
                            pad: pad.pad,
                        },
                        pad,
                    );
                }
            }
        }
        for via in pcb.vias().filter(|v| !v.unused_layers.is_keep()) {
            for layer in pcb.span_layers(via.span) {
                if via.unused_layers == UnusedLayers::RemoveKeepEnds
                    && (layer == via.span.from || layer == via.span.to)
                {
                    continue;
                }
                let connected = via.net.is_some_and(|net| {
                    connections.get_or_init(|| Connections::of(pcb)).connected(
                        pcb,
                        (layer, net),
                        &Shape::disc(via.at, via.diameter_on(layer)),
                        via.at,
                        ConnectionTarget::Via,
                        via.zone_connection.as_ref(),
                    )
                });
                if !connected {
                    out.0.insert((Item::Via { uuid: via.uuid }, layer));
                }
            }
        }
        out
    }

    pub fn contains(&self, item: Item, layer: Uuid) -> bool {
        self.0.contains(&(item, layer))
    }

    pub fn iter(&self) -> impl Iterator<Item = (Item, Uuid)> + '_ {
        self.0.iter().copied()
    }
}

pub(super) fn pad_shape(pad: &PlacedPad, side: Option<Side>) -> Shape {
    let (_, outline) = pad.copper_on(side);
    Shape::outline(&outline, |p| {
        let q = pad.point(p.into());
        [q.x.0, q.y.0]
    })
}

#[derive(Default)]
struct Connections {
    ends: BTreeMap<(Uuid, Uuid), RTree<P>>,
    outlines: BTreeMap<(Uuid, Uuid), Vec<poly::Area>>,
}

impl Connections {
    fn of(pcb: &PcbDocument) -> Self {
        let mut out = Self::default();
        for track in pcb.tracks() {
            let Some(net) = track.net else { continue };
            let tree = out.ends.entry((track.layer, net)).or_default();
            let (from, to) = track.geometry.ends();
            tree.insert([from.x.0, from.y.0]);
            tree.insert([to.x.0, to.y.0]);
        }
        for zone in pcb.active_zones() {
            if let Some(net) = zone.net {
                out.outlines
                    .entry((zone.layer, net))
                    .or_default()
                    .push(fill::outline_area(pcb, zone));
            }
        }
        out
    }

    fn connected(
        &self,
        pcb: &PcbDocument,
        (layer, net): (Uuid, Uuid),
        land: &Shape,
        anchor: Point,
        target: ConnectionTarget,
        own: Option<&ZoneConnection>,
    ) -> bool {
        if self.ends.get(&(layer, net)).is_some_and(|ends| {
            ends.locate_in_envelope(land.envelope())
                .any(|&p| land.contains(p.into()))
        }) {
            return true;
        }
        self.outlines
            .get(&(layer, net))
            .is_some_and(|zones| zones.iter().any(|area| poly::inside(area, anchor)))
            && crate::rules::zone_connection_on(pcb, Some(net), Some(layer), target, own).style
                != ConnectionStyle::None
    }
}
