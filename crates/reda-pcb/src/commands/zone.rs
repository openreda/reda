//! Zones and Keepouts: adding, changing and deleting them. Filling is in
//! `reda_engine` (it writes the `.reda/` cache); the fill queries are in `zone_fill`.

use reda_command::AreaValue;
use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Length, Meta, Registry, Unavailable,
};
use reda_model::{InDocument, Keepout, Object, PcbDocument, PourOver, Uuid, Zone};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::layer::LayerRef;
use super::outline::{VertexParam, contour};
use super::track::{copper_layer, existing_net};
use super::{Created, needs_pcb, non_negative, nullable, pcb, positive};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(ZoneAdd);
    registry.add(ZoneSet);
    registry.add(ZoneDelete);
    registry.add(KeepoutAdd);
    registry.add(KeepoutSet);
    registry.add(KeepoutDelete);
}

pub struct ZoneAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneAddParams {
    /// The copper Layer, such as `copper_bottom`.
    pub layer: LayerRef,
    /// The outline's vertices in order; the last joins the first.
    pub vertices: Vec<VertexParam>,
    /// The Net's UUID; left out for a fill on no Net.
    #[serde(default)]
    pub net: Option<Uuid>,
    /// The gap to other Nets' copper, where it is larger than the clearance rule's. Left
    /// out, the rule alone applies.
    #[serde(default)]
    pub clearance: Option<Length>,
    /// The narrowest copper the fill may leave. Left out, the Net's `track_width` minimum.
    #[serde(default)]
    pub min_width: Option<Length>,
    /// How the fill treats same-Net copper: `zones_only` (default), `all` or `none`.
    #[serde(default)]
    pub pour_over: Option<PourOver>,
    /// Whether fill pieces touching no copper of the Net are dropped. Default: true.
    #[serde(default)]
    pub remove_unconnected: Option<bool>,
    /// Fill pieces smaller than this area are dropped, connected or not. Default: 0.
    #[serde(default)]
    pub min_island_area: Option<AreaValue>,
    /// Higher fills first where Zones overlap. Default: 0.
    #[serde(default)]
    pub priority: Option<u32>,
}

impl Command for ZoneAdd {
    const META: Meta = Meta {
        id: "pcb.zone.add",
        title: "Add Zone",
        description: "Adds a copper Zone: a solid fill on a Net inside the outline `vertices` \
                      on one copper Layer, keeping clear of other Nets' copper. A value left \
                      out takes a fixed default, never a Preference. How the fill joins Pads \
                      and Vias comes from the `zone_connection` rule. The Zone is filled by \
                      `pcb.zone.fill`.",
        category: "pcb",
        icon: "zone",
        kind: reda_command::Kind::Edit,
    };
    type Params = ZoneAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ZoneAddParams) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let layer = copper_layer(pcb, p.layer, "/layer")?;
        let vertices = contour(&p.vertices)?;
        let net = existing_net(pcb, p.net, "/net")?;
        let clearance = p
            .clearance
            .map(|c| non_negative("/clearance", c))
            .transpose()?;
        let min_width = p.min_width.map(|w| positive("/min_width", w)).transpose()?;
        let min_island_area = island_area(p.min_island_area)?.unwrap_or(0);
        let uuid = cx.new_id();
        let zone = Zone {
            uuid,
            layer,
            net,
            vertices,
            clearance,
            min_width,
            pour_over: p.pour_over.unwrap_or_default(),
            remove_unconnected: p.remove_unconnected.unwrap_or(true),
            min_island_area,
            priority: p.priority.unwrap_or(0),
        };
        cx.tx()?
            .insert(Object::Zone(InDocument::new(document, zone)))?;
        Ok(Created { uuid })
    }
}

pub struct ZoneSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneSetParams {
    /// The Zone's UUID.
    pub zone: Uuid,
    /// A new copper Layer.
    #[serde(default)]
    pub layer: Option<LayerRef>,
    /// A new outline.
    #[serde(default)]
    pub vertices: Option<Vec<VertexParam>>,
    /// A new Net's UUID, or `null` for no Net.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Uuid>")]
    pub net: Option<Option<Uuid>>,
    /// A new clearance, or `null` to leave only the rule.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub clearance: Option<Option<Length>>,
    /// A new minimum width, or `null` for the Net's `track_width` minimum.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub min_width: Option<Option<Length>>,
    #[serde(default)]
    pub pour_over: Option<PourOver>,
    #[serde(default)]
    pub remove_unconnected: Option<bool>,
    #[serde(default)]
    pub min_island_area: Option<AreaValue>,
    #[serde(default)]
    pub priority: Option<u32>,
}

impl Command for ZoneSet {
    const META: Meta = Meta {
        id: "pcb.zone.set",
        title: "Change Zone",
        description: "Changes a Zone's Layer, outline, Net or fill settings; what is left \
                      out stays as it is.",
        category: "pcb",
        icon: "zone",
        kind: reda_command::Kind::Edit,
    };
    type Params = ZoneSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ZoneSetParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut zone = existing_zone(pcb, p.zone)?.clone();
        if let Some(layer) = p.layer {
            zone.layer = copper_layer(pcb, layer, "/layer")?;
        }
        if let Some(vertices) = &p.vertices {
            zone.vertices = contour(vertices)?;
        }
        if let Some(net) = p.net {
            zone.net = existing_net(pcb, net, "/net")?;
        }
        if let Some(c) = p.clearance {
            zone.clearance = c.map(|c| non_negative("/clearance", c)).transpose()?;
        }
        if let Some(w) = p.min_width {
            zone.min_width = w.map(|w| positive("/min_width", w)).transpose()?;
        }
        if let Some(pour_over) = p.pour_over {
            zone.pour_over = pour_over;
        }
        if let Some(r) = p.remove_unconnected {
            zone.remove_unconnected = r;
        }
        if let Some(area) = island_area(p.min_island_area)? {
            zone.min_island_area = area;
        }
        if let Some(priority) = p.priority {
            zone.priority = priority;
        }
        cx.tx()?
            .replace(Object::Zone(InDocument::new(document, zone)))?;
        Ok(())
    }
}

/// A `min_island_area` param in square nanometres; negative areas are refused.
fn island_area(area: Option<AreaValue>) -> Result<Option<i64>, CommandError> {
    match area {
        Some(a) if a.0 < 0 => Err(CommandError::invalid_params(
            "/min_island_area",
            "the minimum island area cannot be negative",
        )),
        a => Ok(a.map(|a| a.0)),
    }
}

fn existing_zone(pcb: &PcbDocument, uuid: Uuid) -> Result<&Zone, CommandError> {
    pcb.zone(uuid)
        .ok_or_else(|| CommandError::invalid_params("/zone", format!("no Zone has UUID {uuid}")))
}

/// What deleting Zones or Keepouts returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Deleted {
    /// The deleted objects, sorted by UUID.
    pub deleted: Vec<Uuid>,
}

pub struct ZoneDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneDeleteParams {
    /// The Zones' UUIDs.
    pub zones: Vec<Uuid>,
}

impl Command for ZoneDelete {
    const META: Meta = Meta {
        id: "pcb.zone.delete",
        title: "Delete Zone",
        description: "Deletes Zones, outline and fill.",
        category: "pcb",
        icon: "delete",
        kind: reda_command::Kind::Edit,
    };
    type Params = ZoneDeleteParams;
    type Output = Deleted;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ZoneDeleteParams) -> Result<Deleted, CommandError> {
        delete(cx, p.zones, "zones", |pcb, u| pcb.zone(u).is_some(), "Zone")
    }
}

fn delete(
    cx: &mut Context<'_>,
    uuids: Vec<Uuid>,
    field: &str,
    exists: fn(&PcbDocument, Uuid) -> bool,
    kind: &str,
) -> Result<Deleted, CommandError> {
    let pcb = pcb(cx)?;
    for (i, uuid) in uuids.iter().enumerate() {
        if !exists(pcb, *uuid) {
            return Err(CommandError::invalid_params(
                &format!("/{field}/{i}"),
                format!("no {kind} has UUID {uuid}"),
            ));
        }
    }
    let mut deleted = uuids;
    deleted.sort();
    deleted.dedup();
    let tx = cx.tx()?;
    for uuid in &deleted {
        tx.remove(*uuid)?;
    }
    Ok(Deleted { deleted })
}

pub struct KeepoutAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeepoutAddParams {
    /// The copper Layers it applies to. Default: every copper Layer.
    #[serde(default)]
    pub layers: Option<Vec<LayerRef>>,
    /// The outline's vertices in order; the last joins the first.
    pub vertices: Vec<VertexParam>,
    /// Whether copper (Tracks, Pads, Zone fills) is forbidden inside. Default: true.
    #[serde(default)]
    pub copper: Option<bool>,
    /// Whether Vias are forbidden inside. Default: true.
    #[serde(default)]
    pub vias: Option<bool>,
}

impl Command for KeepoutAdd {
    const META: Meta = Meta {
        id: "pcb.keepout.add",
        title: "Add Keepout",
        description: "Adds a Keepout: an area inside `vertices` on some copper Layers where \
                      copper, Vias or both may not go. Zone fills leave it empty.",
        category: "pcb",
        icon: "keepout",
        kind: reda_command::Kind::Edit,
    };
    type Params = KeepoutAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: KeepoutAddParams) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let layers = keepout_layers(pcb, p.layers)?;
        let vertices = contour(&p.vertices)?;
        let (copper, vias) = (p.copper.unwrap_or(true), p.vias.unwrap_or(true));
        forbids_something(copper, vias)?;
        let uuid = cx.new_id();
        let keepout = Keepout {
            uuid,
            layers,
            vertices,
            copper,
            vias,
        };
        cx.tx()?
            .insert(Object::Keepout(InDocument::new(document, keepout)))?;
        Ok(Created { uuid })
    }
}

/// The copper Layers a Keepout param names, top to bottom; every copper Layer when left
/// out.
fn keepout_layers(
    pcb: &PcbDocument,
    layers: Option<Vec<LayerRef>>,
) -> Result<Vec<Uuid>, CommandError> {
    let Some(layers) = layers else {
        return Ok(pcb.copper_layers().map(|l| l.uuid).collect());
    };
    if layers.is_empty() {
        return Err(CommandError::invalid_params(
            "/layers",
            "a Keepout needs one or more copper Layers",
        ));
    }
    let mut chosen = Vec::new();
    for (i, layer) in layers.into_iter().enumerate() {
        chosen.push(copper_layer(pcb, layer, &format!("/layers/{i}"))?);
    }
    Ok(pcb
        .copper_layers()
        .map(|l| l.uuid)
        .filter(|l| chosen.contains(l))
        .collect())
}

fn forbids_something(copper: bool, vias: bool) -> Result<(), CommandError> {
    if !copper && !vias {
        return Err(CommandError::invalid_params(
            "/vias",
            "a Keepout must forbid copper, Vias or both",
        ));
    }
    Ok(())
}

pub struct KeepoutSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeepoutSetParams {
    /// The Keepout's UUID.
    pub keepout: Uuid,
    #[serde(default)]
    pub layers: Option<Vec<LayerRef>>,
    #[serde(default)]
    pub vertices: Option<Vec<VertexParam>>,
    #[serde(default)]
    pub copper: Option<bool>,
    #[serde(default)]
    pub vias: Option<bool>,
}

impl Command for KeepoutSet {
    const META: Meta = Meta {
        id: "pcb.keepout.set",
        title: "Change Keepout",
        description: "Changes a Keepout's Layers, outline or what it forbids; what is left \
                      out stays as it is.",
        category: "pcb",
        icon: "keepout",
        kind: reda_command::Kind::Edit,
    };
    type Params = KeepoutSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: KeepoutSetParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut keepout = pcb
            .keepout(p.keepout)
            .ok_or_else(|| {
                CommandError::invalid_params(
                    "/keepout",
                    format!("no Keepout has UUID {}", p.keepout),
                )
            })?
            .clone();
        if p.layers.is_some() {
            keepout.layers = keepout_layers(pcb, p.layers)?;
        }
        if let Some(vertices) = &p.vertices {
            keepout.vertices = contour(vertices)?;
        }
        keepout.copper = p.copper.unwrap_or(keepout.copper);
        keepout.vias = p.vias.unwrap_or(keepout.vias);
        forbids_something(keepout.copper, keepout.vias)?;
        cx.tx()?
            .replace(Object::Keepout(InDocument::new(document, keepout)))?;
        Ok(())
    }
}

pub struct KeepoutDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeepoutDeleteParams {
    /// The Keepouts' UUIDs.
    pub keepouts: Vec<Uuid>,
}

impl Command for KeepoutDelete {
    const META: Meta = Meta {
        id: "pcb.keepout.delete",
        title: "Delete Keepout",
        description: "Deletes Keepouts.",
        category: "pcb",
        icon: "delete",
        kind: reda_command::Kind::Edit,
    };
    type Params = KeepoutDeleteParams;
    type Output = Deleted;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: KeepoutDeleteParams) -> Result<Deleted, CommandError> {
        delete(
            cx,
            p.keepouts,
            "keepouts",
            |pcb, u| pcb.keepout(u).is_some(),
            "Keepout",
        )
    }
}
