//! Free Pads and the Placement Origin: `pcb.free_pad.add`, `pcb.free_pad.set_net`,
//! `pcb.free_pad.delete`, `pcb.placement_origin.set` and `pcb.placement_origin.delete`.
//!
//! Free Pads and the Placement Origin are moved, turned and flipped with the placement
//! Commands (`pcb.move` and the rest).

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, NoParams,
    Registry, Unavailable,
};
use reda_model::geom::Millideg;
use reda_model::{
    FreePad, Hole, InDocument, Object, PadKind, PadShape, PcbDocument, PlacementOrigin, Side, Uuid,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::outline::PointParam;
use super::placement::Placed;
use super::{Created, needs_pcb, non_negative, pcb, positive};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(FreePadAdd);
    registry.add(FreePadSetNet);
    registry.add(FreePadDelete);
    registry.add(PlacementOriginSet);
    registry.add(PlacementOriginDelete);
}

/// A drilled hole param.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HoleParam {
    pub width: Length,
    /// A slot when it differs from the width; round when left out.
    #[serde(default)]
    pub height: Option<Length>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreePadAddParams {
    /// `smd` (plated, no hole: fiducials, test points), `plated_through_hole` or
    /// `non_plated` (a bare hole).
    pub kind: PadKind,
    /// Circle when left out.
    #[serde(default)]
    pub shape: Option<PadShape>,
    /// The Pad centre.
    pub at: PointParam,
    pub width: Length,
    /// The width when left out.
    #[serde(default)]
    pub height: Option<Length>,
    /// For a rounded rectangle.
    #[serde(default)]
    pub corner_radius: Option<Length>,
    /// Counter-clockwise; 0 when left out.
    #[serde(default)]
    pub rotation: Option<Angle>,
    /// The drilled hole; required unless the Pad is SMD, which has none.
    #[serde(default)]
    pub hole: Option<HoleParam>,
    /// The copper side of an SMD Pad; top when left out. Pads with a hole have none.
    #[serde(default)]
    pub side: Option<Side>,
    /// The PCB Net, by UUID; non-plated Pads have none.
    #[serde(default)]
    pub net: Option<Uuid>,
}

pub struct FreePadAdd;

impl Command for FreePadAdd {
    const META: Meta = Meta {
        id: "pcb.free_pad.add",
        title: "Add Free Pad",
        description: "Adds a Pad that belongs to no Footprint: plated (SMD or through-hole) \
                      and optionally on a Net, or non-plated (a bare hole). For mounting \
                      holes, fiducials and test points.",
        category: "pcb",
        icon: "pad",
        kind: Kind::Edit,
    };
    type Params = FreePadAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: FreePadAddParams) -> Result<Created, CommandError> {
        let width = positive("/width", p.width)?;
        let height = p.height.map_or(Ok(width), |h| positive("/height", h))?;
        let corner_radius = p
            .corner_radius
            .map(|r| non_negative("/corner_radius", r))
            .transpose()?;
        let hole = p
            .hole
            .map(|h| -> Result<Hole, CommandError> {
                let width = positive("/hole/width", h.width)?;
                let height = h
                    .height
                    .map_or(Ok(width), |l| positive("/hole/height", l))?;
                Ok(Hole { width, height })
            })
            .transpose()?;
        let smd = p.kind == PadKind::Smd;
        match hole {
            Some(_) if smd => {
                return Err(CommandError::invalid_params(
                    "/hole",
                    "an SMD Pad has no hole",
                ));
            }
            None if !smd => {
                return Err(CommandError::invalid_params(
                    "/hole",
                    "a plated through-hole or non-plated Pad needs a hole",
                ));
            }
            Some(h) if h.width > width || h.height > height => {
                return Err(CommandError::invalid_params(
                    "/hole",
                    "the hole is larger than the Pad",
                ));
            }
            _ => {}
        }
        if !smd && p.side.is_some() {
            return Err(CommandError::invalid_params(
                "/side",
                "a Pad with a hole is on both sides",
            ));
        }
        let pcb = pcb(cx)?;
        if let Some(net) = p.net {
            check_net(pcb, p.kind, net, "/net")?;
        }
        let document = pcb.uuid;
        let uuid = cx.new_id();
        let pad = FreePad {
            uuid,
            kind: p.kind,
            shape: p.shape.unwrap_or(PadShape::Circle),
            at: p.at.point(),
            rotation: p
                .rotation
                .map_or(Millideg::ZERO, |r| r.millideg().normalized()),
            width,
            height,
            corner_radius,
            hole,
            side: smd.then(|| p.side.unwrap_or(Side::Top)),
            net: p.net,
        };
        if let Some(problem) = pad.problem() {
            return Err(CommandError::invalid_params("", problem));
        }
        cx.tx()?
            .insert(Object::FreePad(InDocument::new(document, pad)))?;
        Ok(Created { uuid })
    }
}

/// `invalid_params` at `pointer` unless `net` is a Net of the board that a `kind` Pad can
/// be on.
fn check_net(
    pcb: &PcbDocument,
    kind: PadKind,
    net: Uuid,
    pointer: &str,
) -> Result<(), CommandError> {
    if kind == PadKind::NonPlated {
        return Err(CommandError::invalid_params(
            pointer,
            "a non-plated Pad has no copper and cannot be on a Net",
        ));
    }
    if pcb.net(net).is_none() {
        return Err(CommandError::invalid_params(
            pointer,
            format!("{net} is not a Net of the board"),
        ));
    }
    Ok(())
}

/// The Free Pads named by `uuids`, or `invalid_params` at `/pads/i`.
fn free_pads(pcb: &PcbDocument, uuids: &[Uuid]) -> Result<Vec<FreePad>, CommandError> {
    if uuids.is_empty() {
        return Err(CommandError::invalid_params("/pads", "nothing is selected"));
    }
    let mut out: Vec<FreePad> = Vec::new();
    for (i, &uuid) in uuids.iter().enumerate() {
        let Some(pad) = pcb.free_pad(uuid) else {
            return Err(CommandError::invalid_params(
                &format!("/pads/{i}"),
                format!("{uuid} is not a Free Pad"),
            ));
        };
        if !out.iter().any(|p| p.uuid == uuid) {
            out.push(pad.clone());
        }
    }
    Ok(out)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreePadSetNetParams {
    /// The Free Pads, by UUID.
    pub pads: Vec<Uuid>,
    /// The PCB Net, by UUID; null takes them off any Net.
    pub net: Option<Uuid>,
}

pub struct FreePadSetNet;

impl Command for FreePadSetNet {
    const META: Meta = Meta {
        id: "pcb.free_pad.set_net",
        title: "Set Free Pad Net",
        description: "Puts plated Free Pads on a Net, or takes them off any Net (`net: null`).",
        category: "pcb",
        icon: "net",
        kind: Kind::Edit,
    };
    type Params = FreePadSetNetParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: FreePadSetNetParams) -> Result<Placed, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut pads = free_pads(pcb, &p.pads)?;
        if let Some(net) = p.net {
            if pcb.net(net).is_none() {
                return Err(CommandError::invalid_params(
                    "/net",
                    format!("{net} is not a Net of the board"),
                ));
            }
            for (i, uuid) in p.pads.iter().enumerate() {
                let pad = pads.iter().find(|pad| pad.uuid == *uuid).expect("resolved");
                check_net(pcb, pad.kind, net, &format!("/pads/{i}"))?;
            }
        }
        let objects = pads.iter().map(|p| p.uuid).collect();
        let tx = cx.tx()?;
        for pad in &mut pads {
            pad.net = p.net;
            tx.replace(Object::FreePad(InDocument::new(document, pad.clone())))?;
        }
        Ok(Placed { objects })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreePadDeleteParams {
    /// The Free Pads, by UUID.
    pub pads: Vec<Uuid>,
}

pub struct FreePadDelete;

impl Command for FreePadDelete {
    const META: Meta = Meta {
        id: "pcb.free_pad.delete",
        title: "Delete Free Pad",
        description: "Removes Free Pads from the board.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = FreePadDeleteParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: FreePadDeleteParams) -> Result<Placed, CommandError> {
        let pads = free_pads(pcb(cx)?, &p.pads)?;
        let objects: Vec<Uuid> = pads.iter().map(|p| p.uuid).collect();
        let tx = cx.tx()?;
        for uuid in &objects {
            tx.remove(*uuid)?;
        }
        Ok(Placed { objects })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlacementOriginSetParams {
    /// Where Footprint positions are measured from in the manufacturing outputs.
    pub at: PointParam,
}

pub struct PlacementOriginSet;

impl Command for PlacementOriginSet {
    const META: Meta = Meta {
        id: "pcb.placement_origin.set",
        title: "Set Placement Origin",
        description: "Sets the point the CPL measures Footprint positions from, creating the \
                      Placement Origin or moving the existing one. Without it the outputs \
                      measure from the lower-left corner of the Board Outline's bounding \
                      box.",
        category: "pcb",
        icon: "origin",
        kind: Kind::Edit,
    };
    type Params = PlacementOriginSetParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: PlacementOriginSetParams,
    ) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let at = p.at.point();
        match pcb.placement_origin() {
            Some(origin) => {
                let origin = PlacementOrigin {
                    uuid: origin.uuid,
                    at,
                };
                let uuid = origin.uuid;
                cx.tx()?
                    .replace(Object::PlacementOrigin(InDocument::new(document, origin)))?;
                Ok(Created { uuid })
            }
            None => {
                let uuid = cx.new_id();
                cx.tx()?.insert(Object::PlacementOrigin(InDocument::new(
                    document,
                    PlacementOrigin { uuid, at },
                )))?;
                Ok(Created { uuid })
            }
        }
    }
}

pub struct PlacementOriginDelete;

impl Command for PlacementOriginDelete {
    const META: Meta = Meta {
        id: "pcb.placement_origin.delete",
        title: "Remove Placement Origin",
        description: "Removes the Placement Origin, so the outputs measure from the lower-left \
                      corner of the Board Outline's bounding box again.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = NoParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Placed, CommandError> {
        let Some(origin) = pcb(cx)?.placement_origin() else {
            return Err(CommandError::rejected("the board has no Placement Origin"));
        };
        let uuid = origin.uuid;
        cx.tx()?.remove(uuid)?;
        Ok(Placed {
            objects: vec![uuid],
        })
    }
}
