//! A Net's own settings: its colour and whether routes on it remove loops.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::{InDocument, Net, Object, PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{board_copper, needs_pcb, pcb};
use crate::copper::{Copper, Item};
use std::collections::BTreeSet;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(SetColor);
    registry.add(SetRouting);
    registry.add(UpdateNets);
}

/// What the Net setting Commands return.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NetsChanged {
    /// The Nets that changed, sorted.
    pub nets: Vec<Uuid>,
}

/// The Nets `nets` names, sorted and without repeats; an unknown one is `not_found`.
fn named_nets(pcb: &PcbDocument, nets: &[Uuid]) -> Result<Vec<Net>, CommandError> {
    let mut out: Vec<Net> = Vec::new();
    for (i, uuid) in nets.iter().enumerate() {
        let net = pcb.net(*uuid).ok_or_else(|| {
            CommandError::not_found(format!("no Net has UUID {uuid} (`nets[{i}]`)"))
        })?;
        if !out.iter().any(|n| n.uuid == *uuid) {
            out.push(net.clone());
        }
    }
    out.sort_by_key(|n| n.uuid);
    Ok(out)
}

/// Stores each Net `edit` changes and returns their UUIDs.
fn edit_nets(
    cx: &mut Context<'_>,
    nets: &[Uuid],
    edit: impl Fn(&mut Net),
) -> Result<NetsChanged, CommandError> {
    let pcb = pcb(cx)?;
    let document = pcb.uuid;
    let changed: Vec<Net> = named_nets(pcb, nets)?
        .into_iter()
        .filter_map(|mut net| {
            let before = net.clone();
            edit(&mut net);
            (net != before).then_some(net)
        })
        .collect();
    let uuids = changed.iter().map(|n| n.uuid).collect();
    let tx = cx.tx()?;
    for net in changed {
        tx.replace(Object::Net(InDocument::new(document, net)))?;
    }
    Ok(NetsChanged { nets: uuids })
}

/// `color` as `#rrggbb` in lower case, or `invalid_params`.
fn hex_color(color: &str) -> Result<String, CommandError> {
    let digits = color.strip_prefix('#').unwrap_or("");
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CommandError::invalid_params(
            "/color",
            format!("`{color}` is not a colour of the form #rrggbb"),
        ));
    }
    Ok(color.to_ascii_lowercase())
}

pub struct SetColor;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetColorParams {
    pub nets: Vec<Uuid>,
    /// The colour as `#rrggbb`; left out (or `null`), the Nets have none.
    #[serde(default)]
    pub color: Option<String>,
}

impl Command for SetColor {
    const META: Meta = Meta {
        id: "pcb.net.set_color",
        title: "Set Net Colour",
        description: "Sets the colour of Nets (`#rrggbb`), or unsets it when `color` is left \
                      out. It always colours the Nets' Ratsnest lines, and their copper with \
                      Net colours on; no manufacturing output reads it. Returns the Nets that \
                      changed.",
        category: "pcb",
        icon: "net",
        kind: Kind::Edit,
    };
    type Params = SetColorParams;
    type Output = NetsChanged;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: SetColorParams) -> Result<NetsChanged, CommandError> {
        let color = p.color.as_deref().map(hex_color).transpose()?;
        edit_nets(cx, &p.nets, |net| net.color = color.clone())
    }
}

pub struct SetRouting;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetRoutingParams {
    pub nets: Vec<Uuid>,
    /// Whether a route that joins two points already joined on the Net removes the old
    /// path. Off for power and ground Nets that need loops.
    pub loop_removal: bool,
}

impl Command for SetRouting {
    const META: Meta = Meta {
        id: "pcb.net.set_routing",
        title: "Set Net Routing",
        description: "Sets how routes behave on Nets: `loop_removal` (on by default) removes \
                      the old path when a route closes a loop. Returns the Nets that changed.",
        category: "pcb",
        icon: "net",
        kind: Kind::Edit,
    };
    type Params = SetRoutingParams;
    type Output = NetsChanged;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: SetRoutingParams) -> Result<NetsChanged, CommandError> {
        edit_nets(cx, &p.nets, |net| net.loop_removal = p.loop_removal)
    }
}

pub struct UpdateNets;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateNetsParams {
    /// Only copper touching Pads of these Nets; left out, all copper.
    #[serde(default)]
    pub nets: Option<Vec<Uuid>>,
}

/// Copper left as it is because it touches Pads of more than one Net.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NetConflict {
    /// The joined Tracks and Vias, sorted.
    pub objects: Vec<Uuid>,
    /// The Nets of the Pads they touch, sorted.
    pub nets: Vec<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UpdateNetsResult {
    /// The Tracks and Vias whose Net changed, sorted.
    pub changed: Vec<Uuid>,
    /// Copper touching Pads of two or more Nets, which stays as it is.
    pub conflicts: Vec<NetConflict>,
}

impl Command for UpdateNets {
    const META: Meta = Meta {
        id: "pcb.copper.update_nets",
        title: "Update Copper Nets",
        description: "Sets Tracks and Vias to the Net of the Footprint Pads they touch, \
                      through the Tracks and Vias joined to them. Copper touching Pads of two \
                      Nets stays as it is and is reported; copper touching no Pad keeps its \
                      Net. With `nets`, only copper touching Pads of those Nets.",
        category: "pcb",
        icon: "net",
        kind: Kind::Edit,
    };
    type Params = UpdateNetsParams;
    type Output = UpdateNetsResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: UpdateNetsParams,
    ) -> Result<UpdateNetsResult, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let only: Option<BTreeSet<Uuid>> = match &p.nets {
            Some(nets) => Some(named_nets(pcb, nets)?.iter().map(|n| n.uuid).collect()),
            None => None,
        };
        let copper = board_copper(cx)?;
        let mut changes: Vec<(Item, Uuid)> = Vec::new();
        let mut conflicts = Vec::new();
        for (cluster, nets) in routed_clusters(&copper) {
            if only.as_ref().is_some_and(|only| only.is_disjoint(&nets)) {
                continue;
            }
            match nets.len() {
                0 => {}
                1 => {
                    let net = *nets.first().expect("one Net");
                    changes.extend(
                        cluster
                            .iter()
                            .filter(|item| copper.net(**item) != Some(net))
                            .map(|item| (*item, net)),
                    );
                }
                _ => {
                    let mut objects: Vec<Uuid> = cluster.iter().map(|i| i.object()).collect();
                    objects.sort();
                    conflicts.push(NetConflict {
                        objects,
                        nets: nets.into_iter().collect(),
                    });
                }
            }
        }
        let mut objects = Vec::new();
        for (item, net) in changes {
            let object = match item {
                Item::Track { uuid } => {
                    let mut track = pcb.track(uuid).expect("indexed Track").clone();
                    track.net = Some(net);
                    Object::Track(InDocument::new(document, track))
                }
                Item::Via { uuid } => {
                    let mut via = pcb.via(uuid).expect("indexed Via").clone();
                    via.net = Some(net);
                    Object::Via(InDocument::new(document, via))
                }
                _ => continue,
            };
            objects.push(object);
        }
        let mut changed: Vec<Uuid> = objects.iter().map(|o| o.uuid()).collect();
        changed.sort();
        let tx = cx.tx()?;
        for object in objects {
            tx.replace(object)?;
        }
        Ok(UpdateNetsResult { changed, conflicts })
    }
}

/// The board's Tracks and Vias in groups joined through Tracks and Vias, each with the Nets
/// of the Footprint Pads the group touches. Groups are in order of their first item.
fn routed_clusters(copper: &Copper) -> Vec<(BTreeSet<Item>, BTreeSet<Uuid>)> {
    let routed = |item: &Item| matches!(item, Item::Track { .. } | Item::Via { .. });
    let mut left: BTreeSet<Item> = copper.items().filter(routed).collect();
    let mut out = Vec::new();
    while let Some(start) = left.pop_first() {
        let mut cluster = BTreeSet::from([start]);
        let mut nets = BTreeSet::new();
        let mut todo = vec![start];
        while let Some(item) = todo.pop() {
            for next in copper.contacts(item) {
                if routed(&next) {
                    if cluster.insert(next) {
                        left.remove(&next);
                        todo.push(next);
                    }
                } else if matches!(next, Item::Pad { .. })
                    && let Some(net) = copper.net(next)
                {
                    nets.insert(net);
                }
            }
        }
        out.push((cluster, nets));
    }
    out
}
