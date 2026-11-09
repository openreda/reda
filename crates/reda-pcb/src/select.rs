//! What the PCB selection Commands pick: everything on a Layer, the copper of a Net, the
//! Tracks and Vias up to the next Pad, and a Footprint's connections or Nets. The window
//! puts the result in the selection; these functions only read the board.

use std::collections::BTreeSet;

use reda_model::{LayerKind, PadRef, PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::Serialize;

use crate::copper::{Copper, Item};

/// Objects and Footprint Pads to select, each sorted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Picked {
    pub objects: Vec<Uuid>,
    pub pads: Vec<PadRef>,
}

impl Picked {
    fn from_sets(objects: BTreeSet<Uuid>, pads: BTreeSet<(Uuid, Uuid)>) -> Picked {
        Picked {
            objects: objects.into_iter().collect(),
            pads: pads
                .into_iter()
                .map(|(footprint, pad)| PadRef { footprint, pad })
                .collect(),
        }
    }
}

/// Everything on `layer`: copper there (Pads as Footprint Pads), Zones, Zone Cutouts and
/// Keepouts on it, board graphics, text and Dimensions drawn on it, and on the Board
/// Outline Layer the Board Outline and Cutouts.
pub fn on_layer(pcb: &PcbDocument, copper: &Copper, layer: Uuid) -> Picked {
    let mut objects = BTreeSet::new();
    let mut pads = BTreeSet::new();
    for (item, _) in copper.pieces_on(layer) {
        match item {
            Item::Pad { footprint, pad } => {
                pads.insert((footprint, pad));
            }
            other => {
                objects.insert(other.object());
            }
        }
    }
    let kind = pcb
        .layers()
        .layers
        .iter()
        .find(|l| l.uuid == layer)
        .map(|l| l.kind);
    objects.extend(pcb.zones().filter(|z| z.layer == layer).map(|z| z.uuid));
    objects.extend(
        pcb.zone_cutouts()
            .filter(|c| c.layer == layer)
            .map(|c| c.uuid),
    );
    objects.extend(
        pcb.keepouts()
            .filter(|k| {
                k.layer == Some(layer) || (k.layer.is_none() && kind == Some(LayerKind::Copper))
            })
            .map(|k| k.uuid),
    );
    objects.extend(pcb.graphics().filter(|g| g.layer == layer).map(|g| g.uuid));
    objects.extend(pcb.texts().filter(|t| t.layer == layer).map(|t| t.uuid));
    objects.extend(
        pcb.dimensions()
            .filter(|d| d.layer == layer)
            .map(|d| d.uuid),
    );
    if kind == Some(LayerKind::BoardOutline) {
        objects.extend(pcb.outline().map(|o| o.uuid));
        objects.extend(pcb.cutouts().map(|c| c.uuid));
    }
    Picked::from_sets(objects, pads)
}

/// All the copper of `net`: its Tracks, Vias, Free Pads and Zones, and its Footprint Pads.
pub fn net(pcb: &PcbDocument, net: Uuid) -> Picked {
    let on = Some(net);
    let mut objects = BTreeSet::new();
    objects.extend(pcb.tracks().filter(|t| t.net == on).map(|t| t.uuid));
    objects.extend(pcb.vias().filter(|v| v.net == on).map(|v| v.uuid));
    objects.extend(pcb.free_pads().filter(|p| p.net == on).map(|p| p.uuid));
    objects.extend(pcb.zones().filter(|z| z.net == on).map(|z| z.uuid));
    let pads = pcb
        .footprints()
        .flat_map(|f| {
            f.pads
                .iter()
                .filter(|p| p.net == on)
                .map(move |p| (f.uuid, p.pad))
        })
        .collect();
    Picked::from_sets(objects, pads)
}

/// The Tracks and Vias joined to `seed` (a Track or Via) through Tracks and Vias, up to
/// the next Pad, Free Pad or Zone. With `single_layer`, only Tracks on the seed Track's
/// Layer, and no Vias.
pub fn physical_connection(
    pcb: &PcbDocument,
    copper: &Copper,
    seed: Uuid,
    single_layer: bool,
) -> Result<Picked, String> {
    let start = if let Some(track) = pcb.track(seed) {
        (Item::Track { uuid: seed }, Some(track.layer))
    } else if pcb.via(seed).is_some() {
        if single_layer {
            return Err("`single_layer` needs a Track to start from, not a Via".into());
        }
        (Item::Via { uuid: seed }, None)
    } else {
        return Err(format!("{seed} is not a Track or a Via"));
    };
    let layer = start.1.filter(|_| single_layer);
    let found = routed_from(pcb, copper, &[start.0], layer);
    Ok(Picked::from_sets(found, BTreeSet::new()))
}

/// The Tracks and Vias joined to the Pads of `footprints`, each up to the next Pad.
pub fn footprint_connections(
    pcb: &PcbDocument,
    copper: &Copper,
    footprints: &[Uuid],
) -> Result<Picked, String> {
    let mut starts = Vec::new();
    for uuid in footprints {
        let instance = pcb
            .footprint(*uuid)
            .ok_or_else(|| format!("no Footprint instance has UUID {uuid}"))?;
        for pad in &instance.pads {
            let item = Item::Pad {
                footprint: *uuid,
                pad: pad.pad,
            };
            starts.extend(copper.contacts(item).filter(is_routed));
        }
    }
    let found = routed_from(pcb, copper, &starts, None);
    Ok(Picked::from_sets(found, BTreeSet::new()))
}

/// All the copper of every Net a Pad of `footprints` is on.
pub fn footprint_nets(pcb: &PcbDocument, footprints: &[Uuid]) -> Result<Picked, String> {
    let mut nets = BTreeSet::new();
    for uuid in footprints {
        let instance = pcb
            .footprint(*uuid)
            .ok_or_else(|| format!("no Footprint instance has UUID {uuid}"))?;
        nets.extend(instance.pads.iter().filter_map(|p| p.net));
    }
    let mut objects = BTreeSet::new();
    let mut pads = BTreeSet::new();
    for n in nets {
        let picked = net(pcb, n);
        objects.extend(picked.objects);
        pads.extend(picked.pads.into_iter().map(|p| (p.footprint, p.pad)));
    }
    Ok(Picked::from_sets(objects, pads))
}

fn is_routed(item: &Item) -> bool {
    matches!(item, Item::Track { .. } | Item::Via { .. })
}

/// The objects of the Tracks and Vias reached from `starts` through Tracks and Vias; on
/// `layer` only Tracks of that Layer. Two pieces that both touch the same Pad, Free Pad or
/// Zone meet there, so the walk stops at it.
fn routed_from(
    pcb: &PcbDocument,
    copper: &Copper,
    starts: &[Item],
    layer: Option<Uuid>,
) -> BTreeSet<Uuid> {
    let keep = |item: &Item| match (*item, layer) {
        (Item::Track { uuid }, Some(layer)) => pcb.track(uuid).is_some_and(|t| t.layer == layer),
        (_, Some(_)) => false,
        (item, None) => is_routed(&item),
    };
    let mut seen: BTreeSet<Item> = starts.iter().copied().filter(|i| keep(i)).collect();
    let mut todo: Vec<Item> = seen.iter().copied().collect();
    let stops = |item: Item| -> BTreeSet<Item> {
        copper
            .contacts(item)
            .filter(|i| {
                matches!(
                    i,
                    Item::Pad { .. } | Item::FreePad { .. } | Item::Zone { .. }
                )
            })
            .collect()
    };
    while let Some(item) = todo.pop() {
        let at = stops(item);
        for next in copper.contacts(item) {
            // Copper that meets on a Pad meets there, not past it.
            if keep(&next) && at.is_disjoint(&stops(next)) && seen.insert(next) {
                todo.push(next);
            }
        }
    }
    seen.into_iter().map(|i| i.object()).collect()
}
