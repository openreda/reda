//! Zones and Keepouts: adding, changing and deleting them. Filling is in
//! `reda_engine` (it writes the `.reda/` cache); the fill queries are in `zone_fill`.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Length, Meta, Registry, Unavailable,
};
use reda_model::geom::Nm;
use reda_model::{InDocument, Keepout, Object, PadConnection, PcbDocument, Uuid, Zone};
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
    /// The gap to other Nets' copper. Default: the board's clearance rule.
    #[serde(default)]
    pub clearance: Option<Length>,
    /// The narrowest copper the fill may leave. Default: the board's Track width rule.
    #[serde(default)]
    pub min_width: Option<Length>,
    /// `thermal` (default) or `solid`.
    #[serde(default)]
    pub pad_connection: Option<PadConnection>,
    /// The thermal gap around Pads. Default: the Zone's clearance.
    #[serde(default)]
    pub thermal_gap: Option<Length>,
    /// The thermal spoke width. Default: the Zone's minimum width.
    #[serde(default)]
    pub thermal_spoke_width: Option<Length>,
    /// Whether fill pieces touching no copper of the Net are dropped. Default: true.
    #[serde(default)]
    pub remove_islands: Option<bool>,
    /// Higher fills first where Zones overlap. Default: 0.
    #[serde(default)]
    pub priority: Option<u32>,
}

impl Command for ZoneAdd {
    const META: Meta = Meta {
        id: "pcb.zone.add",
        title: "Add Zone",
        description: "Adds a copper Zone: a solid fill on a Net inside the outline `vertices` \
                      on one copper Layer, keeping clear of other Nets' copper. The Zone is \
                      filled by `pcb.zone.fill`.",
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
        let rules = crate::rules::net_rules(pcb, net);
        let clearance = match p.clearance {
            Some(c) => non_negative("/clearance", c)?,
            None => rules.clearance.unwrap_or(Nm::ZERO),
        };
        let min_width = match p.min_width {
            Some(w) => positive("/min_width", w)?,
            None => rules.track_width.min.unwrap_or(rules.track_width.preferred),
        };
        let thermal_gap = match p.thermal_gap {
            Some(g) => non_negative("/thermal_gap", g)?,
            None => clearance,
        };
        let thermal_spoke_width = match p.thermal_spoke_width {
            Some(w) => positive("/thermal_spoke_width", w)?,
            None => min_width,
        };
        let uuid = cx.new_id();
        let zone = Zone {
            uuid,
            layer,
            net,
            vertices,
            clearance,
            min_width,
            pad_connection: p.pad_connection.unwrap_or(PadConnection::Thermal),
            thermal_gap,
            thermal_spoke_width,
            remove_islands: p.remove_islands.unwrap_or(true),
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
    #[serde(default)]
    pub clearance: Option<Length>,
    #[serde(default)]
    pub min_width: Option<Length>,
    #[serde(default)]
    pub pad_connection: Option<PadConnection>,
    #[serde(default)]
    pub thermal_gap: Option<Length>,
    #[serde(default)]
    pub thermal_spoke_width: Option<Length>,
    #[serde(default)]
    pub remove_islands: Option<bool>,
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
            zone.clearance = non_negative("/clearance", c)?;
        }
        if let Some(w) = p.min_width {
            zone.min_width = positive("/min_width", w)?;
        }
        if let Some(c) = p.pad_connection {
            zone.pad_connection = c;
        }
        if let Some(g) = p.thermal_gap {
            zone.thermal_gap = non_negative("/thermal_gap", g)?;
        }
        if let Some(w) = p.thermal_spoke_width {
            zone.thermal_spoke_width = positive("/thermal_spoke_width", w)?;
        }
        if let Some(r) = p.remove_islands {
            zone.remove_islands = r;
        }
        if let Some(priority) = p.priority {
            zone.priority = priority;
        }
        cx.tx()?
            .replace(Object::Zone(InDocument::new(document, zone)))?;
        Ok(())
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
