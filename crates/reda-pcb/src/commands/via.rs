//! Vias: placed mid-route or on their own for stitching.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_model::{InDocument, LayerSpan, Object, PcbDocument, Uuid, Via};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::outline::PointParam;
use super::track::existing_net;
use super::{needs_pcb, pcb, positive};
use crate::rules::net_rules;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(ViaAdd);
    registry.add(ViaDelete);
}

/// The span of a through Via: the outermost copper Layers.
pub(super) fn through(pcb: &PcbDocument) -> Result<LayerSpan, CommandError> {
    let mut copper = pcb.copper_layers();
    let first = copper.next();
    match (first, copper.last()) {
        (Some(from), Some(to)) => Ok(LayerSpan {
            from: from.uuid,
            to: to.uuid,
        }),
        _ => Err(CommandError::rejected(
            "a Via needs two copper Layers to join",
        )),
    }
}

pub struct ViaAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViaAddParams {
    /// The centre.
    pub at: PointParam,
    pub mask: Option<reda_command::MaskParam>,
    /// The Net's UUID. Left out, the Via takes the Net of the copper under its centre on
    /// any copper Layer, or else no Net.
    #[serde(default)]
    pub net: Option<Uuid>,
    /// The copper diameter; left out, the Net Class's Via diameter, else the board's rule.
    #[serde(default)]
    pub diameter: Option<Length>,
    /// The hole; left out, the Net Class's Via hole, else the board's rule.
    #[serde(default)]
    pub hole: Option<Length>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ViaAddResult {
    /// The new Via.
    pub uuid: Uuid,
    /// Its Net; left out for none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
}

impl Command for ViaAdd {
    const META: Meta = Meta {
        id: "pcb.via.add",
        title: "Add Via",
        description: "Places a through Via. Its Net defaults to the copper under it, its \
                      sizes to its Net Class.",
        category: "pcb",
        icon: "via",
        kind: Kind::Edit,
    };
    type Params = ViaAddParams;
    type Output = ViaAddResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ViaAddParams) -> Result<ViaAddResult, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let span = through(pcb)?;
        let at = p.at.point();
        let net = match existing_net(pcb, p.net, "/net")? {
            Some(net) => Some(net),
            None => {
                let copper = super::board_copper(cx)?;
                pcb.span_layers(span)
                    .into_iter()
                    .find_map(|layer| copper.net_at(layer, at))
            }
        };
        let rules = net_rules(pcb, net);
        let diameter = p
            .diameter
            .map_or(Ok(rules.via_diameter.default_size()), |d| {
                positive("/diameter", d)
            })?;
        let hole = p
            .hole
            .map_or(Ok(rules.via_hole.default_size()), |h| positive("/hole", h))?;
        if diameter <= hole {
            return Err(CommandError::invalid_params(
                "/diameter",
                "the diameter must be larger than the hole",
            ));
        }
        let uuid = cx.new_id();
        let via = Via {
            uuid,
            at,
            net,
            diameter,
            hole,
            span,
            mask: reda_command::mask(p.mask),
        };
        cx.tx()?
            .insert(Object::Via(InDocument::new(document, via)))?;
        Ok(ViaAddResult { uuid, net })
    }
}

pub struct ViaDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViaDeleteParams {
    /// The Vias' UUIDs.
    pub vias: Vec<Uuid>,
}

impl Command for ViaDelete {
    const META: Meta = Meta {
        id: "pcb.via.delete",
        title: "Delete Via",
        description: "Deletes Vias.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = ViaDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ViaDeleteParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        for (i, uuid) in p.vias.iter().enumerate() {
            if pcb.via(*uuid).is_none() {
                return Err(CommandError::invalid_params(
                    &format!("/vias/{i}"),
                    format!("no Via has UUID {uuid}"),
                ));
            }
        }
        let tx = cx.tx()?;
        for uuid in p.vias {
            tx.remove(uuid)?;
        }
        Ok(())
    }
}
