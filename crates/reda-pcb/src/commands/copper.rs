//! Copper queries: select connected, the Ratsnest and the live clearance check.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_model::geom::Nm;
use reda_model::{PcbDocument, TrackGeometry, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::layer::LayerRef;
use super::outline::PointParam;
use super::track::{copper_layer, existing_net};
use super::{needs_pcb, pcb, positive};
use crate::copper::{ClearanceHit, Copper, Item, Proposal, RatsnestLine};
use crate::net_rules::net_rules;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(Connected);
    registry.add(Ratsnest);
    registry.add(ClearanceTrack);
    registry.add(ClearanceVia);
}

pub struct Connected;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectedParams {
    /// A Track, a Via, a Free Pad, or a Footprint instance.
    pub object: Uuid,
    /// With a Footprint instance: one of its Pads. Left out, the copper connected to any
    /// of its Pads.
    #[serde(default)]
    pub pad: Option<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ConnectedResult {
    /// The connected copper, the object's own included, sorted.
    pub items: Vec<Item>,
}

impl Command for Connected {
    const META: Meta = Meta {
        id: "pcb.copper.connected",
        title: "Select Connected Copper",
        description: "The copper physically connected to a Track, Via, Pad or Free Pad: everything \
                      reachable through touching copper on any Layer and through Vias and \
                      plated holes, whatever its Net.",
        category: "pcb",
        icon: "select_connected",
        kind: Kind::Query,
    };
    type Params = ConnectedParams;
    type Output = ConnectedResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ConnectedParams,
    ) -> Result<ConnectedResult, CommandError> {
        let pcb = pcb(cx)?;
        let copper = super::board_copper(cx)?;
        let starts: Vec<Item> = if pcb.track(p.object).is_some() {
            vec![Item::Track { uuid: p.object }]
        } else if pcb.via(p.object).is_some() {
            vec![Item::Via { uuid: p.object }]
        } else if pcb.free_pad(p.object).is_some() {
            vec![Item::FreePad { uuid: p.object }]
        } else if let Some(instance) = pcb.footprint(p.object) {
            let pads: Vec<Uuid> = match p.pad {
                Some(pad) if instance.pads.iter().any(|q| q.pad == pad) => vec![pad],
                Some(pad) => {
                    return Err(CommandError::invalid_params(
                        "/pad",
                        format!("the Footprint has no Pad {pad}"),
                    ));
                }
                None => instance.pads.iter().map(|q| q.pad).collect(),
            };
            pads.into_iter()
                .map(|pad| Item::Pad {
                    footprint: p.object,
                    pad,
                })
                .collect()
        } else {
            return Err(CommandError::invalid_params(
                "/object",
                format!("{} is not a Track, Via, Free Pad or Footprint", p.object),
            ));
        };
        let mut items: Vec<Item> = starts
            .into_iter()
            .flat_map(|item| copper.connected(item))
            .collect();
        items.sort();
        items.dedup();
        Ok(ConnectedResult { items })
    }
}

pub struct Ratsnest;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RatsnestParams {
    /// Only these Nets; left out, every Net.
    #[serde(default)]
    pub nets: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RatsnestResult {
    /// The connections copper does not make yet: per Net (in UUID order) the lines of a
    /// minimum spanning tree over its copper islands.
    pub lines: Vec<RatsnestLine>,
}

impl Command for Ratsnest {
    const META: Meta = Meta {
        id: "pcb.ratsnest",
        title: "Ratsnest",
        description: "The Ratsnest: for each Net, the shortest set of lines that would join \
                      the islands its copper forms. Connections copper already makes are \
                      left out.",
        category: "pcb",
        icon: "ratsnest",
        kind: Kind::Query,
    };
    type Params = RatsnestParams;
    type Output = RatsnestResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: RatsnestParams) -> Result<RatsnestResult, CommandError> {
        let pcb = pcb(cx)?;
        let copper = super::board_copper(cx)?;
        let lines = match p.nets {
            None => copper.ratsnest(),
            Some(mut nets) => {
                for (i, net) in nets.iter().enumerate() {
                    existing_net(pcb, Some(*net), &format!("/nets/{i}"))?;
                }
                nets.sort();
                nets.dedup();
                nets.into_iter()
                    .flat_map(|net| copper.net_ratsnest(net))
                    .collect()
            }
        };
        Ok(RatsnestResult { lines })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ClearanceResult {
    /// Whether the copper keeps its clearance from everything and stays out of Keepouts.
    pub clear: bool,
    /// The copper of other Nets it comes too close to, sorted.
    pub hits: Vec<ClearanceHit>,
    /// The Keepouts it would enter, sorted; left out when none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keepouts: Vec<Uuid>,
}

fn check(pcb: &PcbDocument, proposal: Proposal, ignore: &[Uuid]) -> ClearanceResult {
    let hits = Copper::build(pcb).clearance(pcb, &proposal, ignore);
    let keepouts = crate::zone::keepouts_hit(pcb, &proposal);
    ClearanceResult {
        clear: hits.is_empty() && keepouts.is_empty(),
        hits,
        keepouts,
    }
}

pub struct ClearanceTrack;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClearanceTrackParams {
    /// The copper Layer.
    pub layer: LayerRef,
    pub from: PointParam,
    pub to: PointParam,
    /// The Net the segment would be on; left out, none.
    #[serde(default)]
    pub net: Option<Uuid>,
    /// The width; left out, the Net Class's track width.
    #[serde(default)]
    pub width: Option<Length>,
    /// Objects to leave out, such as the Tracks being dragged.
    #[serde(default)]
    pub ignore: Vec<Uuid>,
}

impl Command for ClearanceTrack {
    const META: Meta = Meta {
        id: "pcb.clearance.track",
        title: "Check Track Clearance",
        description: "Whether a proposed Track segment keeps its clearance from copper of \
                      other Nets; where two Net Classes meet, the larger clearance applies.",
        category: "pcb",
        icon: "clearance",
        kind: Kind::Query,
    };
    type Params = ClearanceTrackParams;
    type Output = ClearanceResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ClearanceTrackParams,
    ) -> Result<ClearanceResult, CommandError> {
        let pcb = pcb(cx)?;
        let layer = copper_layer(pcb, p.layer, "/layer")?;
        let net = existing_net(pcb, p.net, "/net")?;
        let width = match p.width {
            Some(w) => positive("/width", w)?,
            None => net_rules(pcb, net).track_width,
        };
        let geometry = TrackGeometry::Line {
            from: p.from.point(),
            to: p.to.point(),
        };
        let proposal = Proposal::Track {
            layer,
            geometry,
            width,
            net,
        };
        Ok(check(pcb, proposal, &p.ignore))
    }
}

pub struct ClearanceVia;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClearanceViaParams {
    pub at: PointParam,
    /// The Net the Via would be on; left out, none.
    #[serde(default)]
    pub net: Option<Uuid>,
    /// The diameter; left out, the Net Class's Via diameter.
    #[serde(default)]
    pub diameter: Option<Length>,
    /// Objects to leave out.
    #[serde(default)]
    pub ignore: Vec<Uuid>,
}

impl Command for ClearanceVia {
    const META: Meta = Meta {
        id: "pcb.clearance.via",
        title: "Check Via Clearance",
        description: "Whether a proposed through Via keeps its clearance from copper of \
                      other Nets on every copper Layer.",
        category: "pcb",
        icon: "clearance",
        kind: Kind::Query,
    };
    type Params = ClearanceViaParams;
    type Output = ClearanceResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ClearanceViaParams,
    ) -> Result<ClearanceResult, CommandError> {
        let pcb = pcb(cx)?;
        let net = existing_net(pcb, p.net, "/net")?;
        let diameter: Nm = match p.diameter {
            Some(d) => positive("/diameter", d)?,
            None => net_rules(pcb, net).via_diameter,
        };
        let proposal = Proposal::Via {
            at: p.at.point(),
            diameter,
            net,
        };
        Ok(check(pcb, proposal, &p.ignore))
    }
}
