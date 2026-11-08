//! Zones and Keepouts: adding, changing and deleting them; Zone names, shelving, pour
//! order, Zones from the Board Outline or graphics, and Zone Cutouts are in submodules.
//! Filling is in `reda_engine` (it writes the `.reda/` cache); the fill queries are in
//! `zone_fill`.

mod cutout;
mod list;
mod order;
mod outline;
mod scheme;
mod shelve;

use reda_command::AreaValue;
use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Length, Meta, Registry, Unavailable,
};
use reda_model::{InDocument, Keepout, Object, PcbDocument, PourOver, Uuid, Zone};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::from_graphics::{FromGraphics, LoopMode};
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
    registry.add(KeepoutAddFromGraphics);
    registry.add(shelve::ZoneShelve);
    registry.add(shelve::ZoneRestore);
    registry.add(order::ZoneReorder);
    registry.add(order::ZoneBringToFront);
    registry.add(order::ZoneSendToBack);
    registry.add(order::ZoneAutoOrder);
    registry.add(outline::ZoneAddFromBoardOutline);
    registry.add(outline::ZoneDetachOutline);
    registry.add(outline::ZoneAddFromGraphics);
    registry.add(cutout::ZoneCutoutAdd);
    registry.add(cutout::ZoneCutoutAddFromGraphics);
    registry.add(cutout::ZoneCutoutDelete);
    registry.add(list::ZoneList);
    registry.add(scheme::ZoneSetNameScheme);
}

/// A new Zone with the fixed values every Command gives what it is not told: no name
/// (the derived one shows), the rules alone for clearance and width, pour over Zones
/// only, unconnected pieces removed, no minimum island area, priority 0, not shelved.
/// Never a Preference, so a Command gives the same Zone on any machine.
pub(super) fn fixed_zone(
    uuid: Uuid,
    layer: Uuid,
    net: Option<Uuid>,
    vertices: Vec<reda_model::geom::Vertex>,
) -> Zone {
    Zone {
        uuid,
        name: None,
        layer,
        net,
        vertices,
        follows_outline: false,
        clearance: None,
        min_width: None,
        pour_over: PourOver::ZonesOnly,
        remove_unconnected: true,
        min_island_area: 0,
        priority: 0,
        shelved: false,
    }
}

pub struct ZoneAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneAddParams {
    /// A name, unique on the board; left out, the Zone shows one derived from the
    /// board's `zone_name_scheme`.
    #[serde(default)]
    pub name: Option<String>,
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
        let min_island_area = island_area(p.min_island_area)?;
        let name = p.name.map(|n| unique_name(pcb, &n, None)).transpose()?;
        let uuid = cx.new_id();
        let fixed = fixed_zone(uuid, layer, net, vertices);
        let zone = Zone {
            name,
            clearance,
            min_width,
            pour_over: p.pour_over.unwrap_or(fixed.pour_over),
            remove_unconnected: p.remove_unconnected.unwrap_or(fixed.remove_unconnected),
            min_island_area: min_island_area.unwrap_or(fixed.min_island_area),
            priority: p.priority.unwrap_or(fixed.priority),
            ..fixed
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
    /// A new name, unique on the board, or `null` to show the derived name again.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<String>")]
    pub name: Option<Option<String>>,
    /// A new copper Layer.
    #[serde(default)]
    pub layer: Option<LayerRef>,
    /// A new outline; a Zone that followed the Board Outline stops following it.
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
        if let Some(name) = p.name {
            zone.name = name
                .map(|n| unique_name(pcb, &n, Some(zone.uuid)))
                .transpose()?;
        }
        if let Some(layer) = p.layer {
            zone.layer = copper_layer(pcb, layer, "/layer")?;
        }
        if let Some(vertices) = &p.vertices {
            zone.vertices = contour(vertices)?;
            zone.follows_outline = false;
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

/// `name` trimmed, checked to be non-empty and to match no other Zone's name, typed or
/// derived (`except` is the Zone being renamed).
pub(super) fn unique_name(
    pcb: &PcbDocument,
    name: &str,
    except: Option<Uuid>,
) -> Result<String, CommandError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::invalid_params(
            "/name",
            "a Zone name cannot be empty; leave it out for the derived name",
        ));
    }
    let names = crate::zone::names(pcb);
    if let Some((other, _)) = names
        .iter()
        .find(|(uuid, shown)| Some(**uuid) != except && shown.as_str() == name)
    {
        return Err(CommandError::invalid_params(
            "/name",
            format!("Zone {other} is already named {name:?}"),
        ));
    }
    Ok(name.to_owned())
}

pub(super) fn existing_zone(pcb: &PcbDocument, uuid: Uuid) -> Result<&Zone, CommandError> {
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

pub(super) fn delete(
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

pub struct KeepoutAddFromGraphics;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeepoutFromGraphicsParams {
    /// Free lines, arcs and outlined rectangles forming one closed loop.
    pub graphics: Vec<Uuid>,
    /// `centerline` (default) or `outer_edge`.
    #[serde(default)]
    pub mode: LoopMode,
    /// Keep the graphics as well; by default they are removed.
    #[serde(default)]
    pub keep: bool,
    /// The copper Layers it applies to. Default: every copper Layer.
    #[serde(default)]
    pub layers: Option<Vec<LayerRef>>,
    /// Whether copper is forbidden inside. Default: true.
    #[serde(default)]
    pub copper: Option<bool>,
    /// Whether Vias are forbidden inside. Default: true.
    #[serde(default)]
    pub vias: Option<bool>,
}

impl Command for KeepoutAddFromGraphics {
    const META: Meta = Meta {
        id: "pcb.keepout.add_from_graphics",
        title: "Keepout from Selection",
        description: "Adds a Keepout along the closed loop that the free lines, arcs and \
                      rectangles `graphics` form, and removes them unless `keep`. `mode` \
                      `centerline` (default) needs their ends to meet within 0.001 mm and \
                      refuses a gap with its location and size; `outer_edge` follows the \
                      outside of the strokes.",
        category: "pcb",
        icon: "keepout",
        kind: reda_command::Kind::Edit,
    };
    type Params = KeepoutFromGraphicsParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: KeepoutFromGraphicsParams,
    ) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let layers = keepout_layers(pcb, p.layers)?;
        let (copper, vias) = (p.copper.unwrap_or(true), p.vias.unwrap_or(true));
        forbids_something(copper, vias)?;
        let from = FromGraphics {
            graphics: p.graphics,
            mode: p.mode,
            keep: p.keep,
        };
        let vertices = from.contour(pcb)?;
        from.consume(cx)?;
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
