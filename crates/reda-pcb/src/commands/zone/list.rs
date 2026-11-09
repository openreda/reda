//! `pcb.zone.list`: the Zones per Layer in pour order, as the Zones panel lists them,
//! with their shown names and the areas Properties reads out.

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::{PcbDocument, Uuid, ZoneFills};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::super::layer::LayerRef;
use super::super::track::copper_layer;
use super::super::{needs_pcb, pcb};
use crate::zone::{self, FillState, fill, poly};

pub struct ZoneList;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneListParams {
    /// Only the Zones on this copper Layer; left out, every copper Layer.
    #[serde(default)]
    pub layer: Option<LayerRef>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ZoneListing {
    /// The copper Layers top to bottom, each with its Zones.
    pub layers: Vec<LayerZones>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct LayerZones {
    pub layer: Uuid,
    /// Its Zones in pour order, first to fill first; shelved ones too.
    pub zones: Vec<ZoneRow>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ZoneRow {
    pub uuid: Uuid,
    /// The name it shows: the typed one, or the one derived from the board's
    /// `zone_name_scheme`.
    pub name: String,
    /// Whether the name is typed rather than derived.
    pub typed_name: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    pub priority: u32,
    pub shelved: bool,
    pub follows_outline: bool,
    /// `filled`, `stale`, `unfilled` or `shelved`.
    pub state: FillState,
    /// The area inside the outline, in square nanometres.
    pub outline_area: i64,
    /// The area of its stored fill, in square nanometres (0 while unfilled).
    pub filled_area: i64,
}

impl Command for ZoneList {
    const META: Meta = Meta {
        id: "pcb.zone.list",
        title: "List Zones",
        description: "Lists the Zones on each copper Layer in pour order (first to fill \
                      first), with the name each shows (typed or derived), Net, priority, \
                      shelved and fill state, outline area and filled copper area.",
        category: "pcb",
        icon: "zone",
        kind: Kind::Query,
    };
    type Params = ZoneListParams;
    type Output = ZoneListing;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ZoneListParams) -> Result<ZoneListing, CommandError> {
        let pcb = pcb(cx)?;
        let only = p
            .layer
            .map(|l| copper_layer(pcb, l, "/layer"))
            .transpose()?;
        Ok(listing(pcb, cx.project()?.zone_fills(pcb.uuid), only))
    }
}

/// The Zones on each copper Layer (or only `only`) in pour order, with their shown names,
/// fill states and areas: `pcb.zone.list`'s result and the Board navigator's Zones rows.
pub fn listing(pcb: &PcbDocument, fills: Option<&ZoneFills>, only: Option<Uuid>) -> ZoneListing {
    let states = zone::status(pcb, fills);
    let names = zone::names(pcb);
    let layers = pcb
        .copper_layers()
        .map(|l| l.uuid)
        .filter(|l| only.is_none_or(|o| o == *l))
        .map(|layer| LayerZones {
            layer,
            zones: zone::layer_order(pcb, layer)
                .into_iter()
                .map(|z| ZoneRow {
                    uuid: z.uuid,
                    name: names[&z.uuid].clone(),
                    typed_name: z.name.is_some(),
                    net: z.net,
                    priority: z.priority,
                    shelved: z.shelved,
                    follows_outline: z.follows_outline,
                    state: states
                        .zones
                        .iter()
                        .find(|s| s.zone == z.uuid)
                        .map_or(FillState::Unfilled, |s| s.state),
                    outline_area: poly::outline_area(pcb.zone_vertices(z)) as i64,
                    filled_area: fills
                        .and_then(|f| f.zones.get(&z.uuid))
                        .map_or(0, |f| poly::covered(&fill::to_area(&f.shapes)) as i64),
                })
                .collect(),
        })
        .collect();
    ZoneListing { layers }
}
