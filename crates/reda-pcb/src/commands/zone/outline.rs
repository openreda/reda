//! Zones from the Board Outline and from graphics: `pcb.zone.add_from_board_outline`
//! (Zones that follow the outline), `pcb.zone.detach_outline` and
//! `pcb.zone.add_from_graphics`. New Zones take the same fixed values as `pcb.zone.add`.

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::{InDocument, Object, Uuid, Zone};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::super::from_graphics::{FromGraphics, LoopMode};
use super::super::layer::LayerRef;
use super::super::track::{copper_layer, existing_net};
use super::super::{Created, needs_pcb, pcb};
use super::{existing_zone, fixed_zone, unique_name};

pub struct ZoneAddFromBoardOutline;
pub struct ZoneDetachOutline;
pub struct ZoneAddFromGraphics;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FromOutlineParams {
    /// The Net's UUID; left out for fills on no Net.
    #[serde(default)]
    pub net: Option<Uuid>,
    /// The copper Layers: one Zone each.
    pub layers: Vec<LayerRef>,
}

/// The Zones a Command made, in the order of its `layers`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CreatedZones {
    pub zones: Vec<Uuid>,
}

impl Command for ZoneAddFromBoardOutline {
    const META: Meta = Meta {
        id: "pcb.zone.add_from_board_outline",
        title: "Add Zones from Board Outline",
        description: "Adds one Zone per copper Layer in `layers` that follows the Board \
                      Outline (`follows_outline`): its fill follows later outline edits \
                      and keeps the edge clearance, so it needs no inset. It has no \
                      vertices of its own until `pcb.zone.detach_outline`. Other values \
                      are fixed, as for `pcb.zone.add`.",
        category: "pcb",
        icon: "zone",
        kind: Kind::Edit,
    };
    type Params = FromOutlineParams;
    type Output = CreatedZones;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: FromOutlineParams,
    ) -> Result<CreatedZones, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        if pcb.outline().is_none() {
            return Err(CommandError::rejected("the board has no Board Outline"));
        }
        if p.layers.is_empty() {
            return Err(CommandError::invalid_params(
                "/layers",
                "name one or more copper Layers",
            ));
        }
        let net = existing_net(pcb, p.net, "/net")?;
        let mut layers = Vec::new();
        for (i, layer) in p.layers.into_iter().enumerate() {
            let layer = copper_layer(pcb, layer, &format!("/layers/{i}"))?;
            if layers.contains(&layer) {
                return Err(CommandError::invalid_params(
                    &format!("/layers/{i}"),
                    "that Layer is listed twice",
                ));
            }
            layers.push(layer);
        }
        let zones: Vec<Zone> = layers
            .into_iter()
            .map(|layer| Zone {
                follows_outline: true,
                ..fixed_zone(cx.new_id(), layer, net, Vec::new())
            })
            .collect();
        let uuids = zones.iter().map(|z| z.uuid).collect();
        let tx = cx.tx()?;
        for zone in zones {
            tx.insert(Object::Zone(InDocument::new(document, zone)))?;
        }
        Ok(CreatedZones { zones: uuids })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DetachParams {
    /// The Zone's UUID.
    pub zone: Uuid,
}

impl Command for ZoneDetachOutline {
    const META: Meta = Meta {
        id: "pcb.zone.detach_outline",
        title: "Detach Zone from Board Outline",
        description: "Turns a Zone that follows the Board Outline into an ordinary one, \
                      with the outline's vertices as its own: it no longer follows outline \
                      edits, and its vertices can be edited.",
        category: "pcb",
        icon: "zone",
        kind: Kind::Edit,
    };
    type Params = DetachParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DetachParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let zone = existing_zone(pcb, p.zone)?;
        if !zone.follows_outline {
            return Err(CommandError::rejected(
                "the Zone does not follow the Board Outline",
            ));
        }
        let vertices = pcb.zone_vertices(zone).to_vec();
        if vertices.is_empty() {
            return Err(CommandError::rejected(
                "the board has no Board Outline to take the vertices from",
            ));
        }
        let zone = Zone {
            vertices,
            follows_outline: false,
            ..zone.clone()
        };
        cx.tx()?
            .replace(Object::Zone(InDocument::new(document, zone)))?;
        Ok(())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneFromGraphicsParams {
    /// Free lines, arcs and outlined rectangles forming one closed loop.
    pub graphics: Vec<Uuid>,
    /// `centerline` (default) or `outer_edge`.
    #[serde(default)]
    pub mode: LoopMode,
    /// Keep the graphics as well; by default they are removed.
    #[serde(default)]
    pub keep: bool,
    /// The copper Layer; the GUI passes the active one.
    pub layer: LayerRef,
    /// The Net's UUID; left out for a fill on no Net.
    #[serde(default)]
    pub net: Option<Uuid>,
    /// A name, unique on the board; left out, the derived name.
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for ZoneAddFromGraphics {
    const META: Meta = Meta {
        id: "pcb.zone.add_from_graphics",
        title: "Zone from Selection",
        description: "Adds a Zone on copper `layer` along the closed loop that the free \
                      lines, arcs and rectangles `graphics` form, and removes them unless \
                      `keep`. `mode` `centerline` (default) needs their ends to meet \
                      within 0.001 mm and refuses a gap with its location and size; \
                      `outer_edge` follows the outside of the strokes. Other values are \
                      fixed, as for `pcb.zone.add`.",
        category: "pcb",
        icon: "zone",
        kind: Kind::Edit,
    };
    type Params = ZoneFromGraphicsParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ZoneFromGraphicsParams,
    ) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let layer = copper_layer(pcb, p.layer, "/layer")?;
        let net = existing_net(pcb, p.net, "/net")?;
        let name = p.name.map(|n| unique_name(pcb, &n, None)).transpose()?;
        let from = FromGraphics {
            graphics: p.graphics,
            mode: p.mode,
            keep: p.keep,
        };
        let vertices = from.contour(pcb)?;
        from.consume(cx)?;
        let uuid = cx.new_id();
        let zone = Zone {
            name,
            ..fixed_zone(uuid, layer, net, vertices)
        };
        cx.tx()?
            .insert(Object::Zone(InDocument::new(document, zone)))?;
        Ok(Created { uuid })
    }
}
