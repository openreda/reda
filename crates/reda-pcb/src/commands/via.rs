//! Vias: placed mid-route or on their own for stitching.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_model::{InDocument, LayerSpan, Object, Uuid, Via};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::outline::PointParam;
use super::track::existing_net;
use super::via_type::{DiameterParam, check_sizes, check_span};
use super::{needs_pcb, pcb, positive};
use crate::rules::via_starting_size;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(ViaAdd);
    registry.add(ViaDelete);
    registry.add(ViaSet);
}

pub struct ViaAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViaAddParams {
    /// The centre.
    pub at: PointParam,
    pub mask: Option<reda_command::MaskParam>,
    pub unused_layers: Option<reda_model::UnusedLayers>,
    pub teardrop: Option<bool>,
    pub zone_connection: Option<reda_command::ZoneConnectionParam>,
    pub from: Option<Uuid>,
    pub to: Option<Uuid>,
    /// The Net's UUID. Left out, the Via takes the Net of the copper under its centre on
    /// any copper Layer, or else no Net.
    #[serde(default)]
    pub net: Option<Uuid>,
    /// The copper diameter; left out, the Net Class's Via diameter, else the board's rule.
    #[serde(default)]
    pub diameter: Option<DiameterParam>,
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
        description: "Places a Via on the requested span. Its Net defaults to the copper under it.",
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
        let through = pcb.stackup().through_type();
        let span = LayerSpan {
            from: p.from.unwrap_or(through.from),
            to: p.to.unwrap_or(through.to),
        };
        check_span(pcb, span)?;
        let kind = pcb.stackup().type_for_span(span);
        let span = kind.map_or(span, |v| v.span());
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
        let (default_diameter, default_hole) = via_starting_size(pcb, net, kind.unwrap_or(through));
        let diameter = p.diameter.map_or(default_diameter, DiameterParam::diameter);
        let hole = p.hole.map_or(default_hole, Length::nm);
        check_sizes(diameter, hole, "/diameter")?;
        let unused_layers = p
            .unused_layers
            .or_else(|| kind.map(|k| k.unused_layers))
            .unwrap_or_default();
        let zone_connection = connection(p.zone_connection)?;
        let uuid = cx.new_id();
        let via = Via {
            locked: false,
            uuid,
            at,
            net,
            diameter,
            hole,
            span,
            mask: reda_command::mask(p.mask),
            unused_layers,
            zone_connection,
            teardrop: p.teardrop.unwrap_or(true),
        };
        cx.tx()?
            .insert(Object::Via(InDocument::new(document, via)))?;
        Ok(ViaAddResult { uuid, net })
    }
}

fn connection(
    param: Option<reda_command::ZoneConnectionParam>,
) -> Result<Option<reda_model::ZoneConnection>, CommandError> {
    let value = reda_command::zone_connection(param);
    if let Some((field, why)) = value.and_then(|v| v.problem()) {
        return Err(CommandError::invalid_params(
            &format!("/zone_connection{field}"),
            why,
        ));
    }
    Ok(value)
}

pub struct ViaDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViaDeleteParams {
    /// The Vias' UUIDs.
    pub vias: Vec<Uuid>,
    /// Deletes locked objects too; without it a locked one refuses the whole Command.
    #[serde(default)]
    pub include_locked: bool,
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
        crate::lock::refuse(pcb, p.vias.iter().copied(), p.include_locked, "delete")?;
        let tx = cx.tx()?;
        for uuid in p.vias {
            tx.remove(uuid)?;
        }
        Ok(())
    }
}

pub struct ViaSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViaSetParams {
    /// The Vias' UUIDs; each gets every value given.
    pub vias: Vec<Uuid>,
    /// The copper diameter.
    #[serde(default)]
    pub diameter: Option<DiameterParam>,
    pub from: Option<Uuid>,
    pub to: Option<Uuid>,
    pub unused_layers: Option<reda_model::UnusedLayers>,
    pub teardrop: Option<bool>,
    #[serde(default, deserialize_with = "super::nullable")]
    #[schemars(with = "Option<reda_command::MaskParam>")]
    pub mask: Option<Option<reda_command::MaskParam>>,
    #[serde(default, deserialize_with = "super::nullable")]
    #[schemars(with = "Option<reda_command::ZoneConnectionParam>")]
    pub zone_connection: Option<Option<reda_command::ZoneConnectionParam>>,
    /// The drilled hole.
    #[serde(default)]
    pub hole: Option<Length>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ViaSetResult {
    /// The Vias that changed, sorted.
    pub vias: Vec<Uuid>,
}

impl Command for ViaSet {
    const META: Meta = Meta {
        id: "pcb.via.set",
        title: "Set Via",
        description: "Sets the copper diameter and the hole of Vias. Values left out stay; \
                      the diameter must stay larger than the hole. Returns the Vias that \
                      changed.",
        category: "pcb",
        icon: "via",
        kind: Kind::Edit,
    };
    type Params = ViaSetParams;
    type Output = ViaSetResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ViaSetParams) -> Result<ViaSetResult, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let diameter = p.diameter.map(DiameterParam::diameter);
        let hole = p.hole.map(|h| positive("/hole", h)).transpose()?;
        let mut changed: Vec<Via> = Vec::new();
        for (i, uuid) in p.vias.iter().enumerate() {
            let before = pcb.via(*uuid).ok_or_else(|| {
                CommandError::invalid_params(
                    &format!("/vias/{i}"),
                    format!("no Via has UUID {uuid}"),
                )
            })?;
            let mut via = before.clone();
            via.diameter = diameter.unwrap_or(via.diameter);
            via.hole = hole.unwrap_or(via.hole);
            let span = LayerSpan {
                from: p.from.unwrap_or(via.span.from),
                to: p.to.unwrap_or(via.span.to),
            };
            check_span(pcb, span)?;
            via.span = pcb.stackup().type_for_span(span).map_or(span, |t| t.span());
            via.unused_layers = p.unused_layers.unwrap_or(via.unused_layers);
            via.teardrop = p.teardrop.unwrap_or(via.teardrop);
            if let Some(mask) = p.mask {
                via.mask = reda_command::mask(mask);
            }
            if let Some(param) = p.zone_connection {
                via.zone_connection = connection(param)?;
            }
            check_sizes(
                via.diameter,
                via.hole,
                if hole.is_some() { "/hole" } else { "/diameter" },
            )?;
            if via != *before && !changed.iter().any(|v| v.uuid == *uuid) {
                changed.push(via);
            }
        }
        changed.sort_by_key(|v| v.uuid);
        let vias = changed.iter().map(|v| v.uuid).collect();
        let tx = cx.tx()?;
        for via in changed {
            tx.replace(Object::Via(InDocument::new(document, via)))?;
        }
        Ok(ViaSetResult { vias })
    }
}
