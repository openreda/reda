//! Fill queries: whether the Zones need refilling, and the fills themselves. Filling is
//! `pcb.zone.fill`, in `reda-engine`, because it writes the `.reda/` cache.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NoParams, Registry,
    Unavailable,
};
use reda_model::{FillShape, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{needs_pcb, pcb};
use crate::zone::{self, FillState, FillStatus};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(FillStatusQuery);
    registry.add(Fills);
    registry.add(TeardropStatusQuery);
}

pub struct FillStatusQuery;

impl Command for FillStatusQuery {
    const META: Meta = Meta {
        id: "pcb.zone.fill_status",
        title: "Zone Fill Status",
        description: "Whether the Zones need refilling (some Zone is unfilled, or the board \
                      changed since the last fill) and each Zone's state: `filled`, `stale` \
                      or `unfilled`.",
        category: "pcb",
        icon: "zone_fill",
        kind: Kind::Query,
    };
    type Params = NoParams;
    type Output = FillStatus;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<FillStatus, CommandError> {
        let pcb = pcb(cx)?;
        Ok(zone::status_with_fonts(
            pcb,
            cx.project()?.zone_fills(pcb.uuid),
            cx.fonts(),
        ))
    }
}

pub struct Fills;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FillsParams {
    /// Only these Zones; left out, every Zone.
    #[serde(default)]
    pub zones: Option<Vec<Uuid>>,
}

/// One Zone's stored fill.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ZoneFill {
    pub zone: Uuid,
    pub layer: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    pub state: FillState,
    /// The filled copper: polygons with holes, in nm. Empty when unfilled.
    pub shapes: Vec<FillShape>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FillsResult {
    /// The Zones, sorted by UUID.
    pub zones: Vec<ZoneFill>,
}

impl Command for Fills {
    const META: Meta = Meta {
        id: "pcb.zone.fills",
        title: "Zone Fills",
        description: "The stored fill of each Zone (polygons with holes), with whether it is \
                      current, stale or missing. Stale fills are returned as they were.",
        category: "pcb",
        icon: "zone_fill",
        kind: Kind::Query,
    };
    type Params = FillsParams;
    type Output = FillsResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: FillsParams) -> Result<FillsResult, CommandError> {
        let pcb = pcb(cx)?;
        if let Some(zones) = &p.zones {
            for (i, uuid) in zones.iter().enumerate() {
                if pcb.zone(*uuid).is_none() {
                    return Err(CommandError::invalid_params(
                        &format!("/zones/{i}"),
                        format!("no Zone has UUID {uuid}"),
                    ));
                }
            }
        }
        let stored = cx.project()?.zone_fills(pcb.uuid);
        let status = zone::status_with_fonts(pcb, stored, cx.fonts());
        let zones = pcb
            .zones()
            .zip(status.zones)
            .filter(|(z, _)| p.zones.as_ref().is_none_or(|only| only.contains(&z.uuid)))
            .map(|(z, s)| ZoneFill {
                zone: z.uuid,
                layer: z.layer,
                net: z.net,
                state: s.state,
                shapes: stored
                    .and_then(|f| f.zones.get(&z.uuid))
                    .map(|f| f.shapes.clone())
                    .unwrap_or_default(),
            })
            .collect();
        Ok(FillsResult { zones })
    }
}

pub struct TeardropStatusQuery;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TeardropStatusParams {
    #[serde(default)]
    pub document: Option<Uuid>,
}

impl Command for TeardropStatusQuery {
    const META: Meta = Meta {
        id: "pcb.teardrop.status",
        title: "Teardrop Status",
        description: "Counts generated, shortened and left-out Teardrops from the last fill, and lists the shortened and left-out junctions.",
        category: "pcb",
        icon: "zone_fill",
        kind: Kind::Query,
    };
    type Params = TeardropStatusParams;
    type Output = crate::teardrop::TeardropStatus;
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> { needs_pcb(cx) }
    fn run(&self,cx: &mut Context<'_>,p: Self::Params) -> Result<Self::Output,CommandError> {
        let board = pcb(cx)?;
        if p.document.is_some_and(|doc| doc != board.uuid) {
            return Err(CommandError::invalid_params("/document","unknown PCB Document"));
        }
        Ok(crate::teardrop::status(cx.project()?.zone_fills(board.uuid)))
    }
}
