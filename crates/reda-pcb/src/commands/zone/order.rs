//! Pour order: where Zones overlap on a Layer, the one earlier in the order fills first
//! and the others keep clear of it. The order is the Zones' `priority`, highest first,
//! a tie going to the lower UUID; these Commands rewrite the priorities of one Layer as
//! `n - 1` down to `0`, so each change is one edit.

use std::collections::BTreeSet;

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::{InDocument, Object, PcbDocument, Uuid, Zone};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::super::layer::LayerRef;
use super::super::track::copper_layer;
use super::super::{needs_pcb, pcb};
use super::existing_zone;
use super::shelve::at;
use crate::zone::{layer_order, poly};

pub struct ZoneReorder;
pub struct ZoneBringToFront;
pub struct ZoneSendToBack;
pub struct ZoneAutoOrder;

/// The pour order of the Layers a Command changed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Ordered {
    /// Each changed Layer's Zones, first to fill first.
    pub layers: Vec<LayerOrder>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct LayerOrder {
    pub layer: Uuid,
    pub zones: Vec<Uuid>,
}

/// Sets the priorities of the Zones of one Layer from `order` (first fills first),
/// writing only the Zones whose priority changes.
fn apply(cx: &mut Context<'_>, order: &[Zone]) -> Result<(), CommandError> {
    let document = pcb(cx)?.uuid;
    let n = order.len() as u32;
    let changed: Vec<Zone> = order
        .iter()
        .enumerate()
        .filter(|(i, z)| z.priority != n - 1 - *i as u32)
        .map(|(i, z)| Zone {
            priority: n - 1 - i as u32,
            ..z.clone()
        })
        .collect();
    if !changed.is_empty() {
        let tx = cx.tx()?;
        for zone in changed {
            tx.replace(Object::Zone(InDocument::new(document, zone)))?;
        }
    }
    Ok(())
}

fn owned(zones: Vec<&Zone>) -> Vec<Zone> {
    zones.into_iter().cloned().collect()
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReorderParams {
    /// The copper Layer.
    pub layer: LayerRef,
    /// Every Zone on the Layer, shelved ones too, first to fill first.
    pub order: Vec<Uuid>,
}

impl Command for ZoneReorder {
    const META: Meta = Meta {
        id: "pcb.zone.reorder",
        title: "Reorder Zones",
        description: "Sets the pour order of the Zones on one copper Layer: `order` lists \
                      every Zone there, first to fill first. Where Zones overlap, the \
                      earlier fills first and the others keep clear.",
        category: "pcb",
        icon: "zone",
        kind: Kind::Edit,
    };
    type Params = ReorderParams;
    type Output = Ordered;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ReorderParams) -> Result<Ordered, CommandError> {
        let pcb = pcb(cx)?;
        let layer = copper_layer(pcb, p.layer, "/layer")?;
        let on_layer: BTreeSet<Uuid> = layer_order(pcb, layer).iter().map(|z| z.uuid).collect();
        let mut seen = BTreeSet::new();
        let mut order = Vec::new();
        for (i, uuid) in p.order.iter().enumerate() {
            let pointer = format!("/order/{i}");
            if !on_layer.contains(uuid) {
                return Err(CommandError::invalid_params(
                    &pointer,
                    format!("{uuid} is not a Zone on that Layer"),
                ));
            }
            if !seen.insert(*uuid) {
                return Err(CommandError::invalid_params(
                    &pointer,
                    format!("Zone {uuid} is listed twice"),
                ));
            }
            order.push(pcb.zone(*uuid).expect("on the Layer").clone());
        }
        if let Some(missing) = on_layer.difference(&seen).next() {
            return Err(CommandError::invalid_params(
                "/order",
                format!("the order leaves out Zone {missing}"),
            ));
        }
        apply(cx, &order)?;
        Ok(result(super::super::pcb(cx)?, [layer]))
    }
}

fn result(pcb: &PcbDocument, layers: impl IntoIterator<Item = Uuid>) -> Ordered {
    let layers = layers
        .into_iter()
        .map(|layer| LayerOrder {
            layer,
            zones: layer_order(pcb, layer).iter().map(|z| z.uuid).collect(),
        })
        .collect();
    Ordered { layers }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveParams {
    /// The Zones' UUIDs. Each moves within its own Layer; they keep their order among
    /// themselves.
    pub zones: Vec<Uuid>,
}

/// Moves `zones` to the front (`front`) or back of their Layers' orders.
fn move_to(cx: &mut Context<'_>, zones: &[Uuid], front: bool) -> Result<Ordered, CommandError> {
    let pcb = pcb(cx)?;
    if zones.is_empty() {
        return Err(CommandError::invalid_params(
            "/zones",
            "name one or more Zones",
        ));
    }
    let mut layers = BTreeSet::new();
    for (i, uuid) in zones.iter().enumerate() {
        layers.insert(existing_zone(pcb, *uuid).map_err(|e| at(e, i))?.layer);
    }
    let mut orders = Vec::new();
    for layer in &layers {
        let (moved, rest): (Vec<&Zone>, Vec<&Zone>) = layer_order(pcb, *layer)
            .into_iter()
            .partition(|z| zones.contains(&z.uuid));
        let order = if front {
            owned(moved.into_iter().chain(rest).collect())
        } else {
            owned(rest.into_iter().chain(moved).collect())
        };
        orders.push(order);
    }
    for order in orders {
        apply(cx, &order)?;
    }
    Ok(result(super::super::pcb(cx)?, layers))
}

impl Command for ZoneBringToFront {
    const META: Meta = Meta {
        id: "pcb.zone.bring_to_front",
        title: "Bring Zones to Front",
        description: "Moves Zones to the front of their Layers' pour order, so they fill \
                      first.",
        category: "pcb",
        icon: "zone",
        kind: Kind::Edit,
    };
    type Params = MoveParams;
    type Output = Ordered;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: MoveParams) -> Result<Ordered, CommandError> {
        move_to(cx, &p.zones, true)
    }
}

impl Command for ZoneSendToBack {
    const META: Meta = Meta {
        id: "pcb.zone.send_to_back",
        title: "Send Zones to Back",
        description: "Moves Zones to the back of their Layers' pour order, so they fill \
                      last and keep clear of the Zones they overlap.",
        category: "pcb",
        icon: "zone",
        kind: Kind::Edit,
    };
    type Params = MoveParams;
    type Output = Ordered;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: MoveParams) -> Result<Ordered, CommandError> {
        move_to(cx, &p.zones, false)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AutoOrderParams {
    /// One copper Layer; left out, every copper Layer.
    #[serde(default)]
    pub layer: Option<LayerRef>,
}

impl Command for ZoneAutoOrder {
    const META: Meta = Meta {
        id: "pcb.zone.auto_order",
        title: "Auto Order Zones",
        description: "Orders the Zones on each copper Layer (or only `layer`) by outline \
                      area, smallest first, so a small Zone inside a large one fills \
                      first; equal areas go by UUID.",
        category: "pcb",
        icon: "zone",
        kind: Kind::Edit,
    };
    type Params = AutoOrderParams;
    type Output = Ordered;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: AutoOrderParams) -> Result<Ordered, CommandError> {
        let pcb = pcb(cx)?;
        let layers: Vec<Uuid> = match p.layer {
            Some(layer) => vec![copper_layer(pcb, layer, "/layer")?],
            None => pcb.copper_layers().map(|l| l.uuid).collect(),
        };
        let mut orders = Vec::new();
        for layer in &layers {
            let mut zones: Vec<(i128, Zone)> = layer_order(pcb, *layer)
                .into_iter()
                .map(|z| (poly::outline_area(pcb.zone_vertices(z)), z.clone()))
                .collect();
            zones.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.uuid.cmp(&b.1.uuid)));
            orders.push(zones.into_iter().map(|(_, z)| z).collect::<Vec<_>>());
        }
        for order in orders {
            apply(cx, &order)?;
        }
        Ok(result(super::super::pcb(cx)?, layers))
    }
}
