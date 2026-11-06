//! Free Pads and the Output Origin: `pcb.free_pad.add`, `pcb.free_pad.set_net`,
//! `pcb.free_pad.delete`, `pcb.output_origin.set` and `pcb.output_origin.delete`.
//!
//! Free Pads and the Output Origin are moved, turned and flipped with the placement
//! Commands (`pcb.move` and the rest).

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, HoleParam, Kind, Length, Meta,
    NoParams, RatioParam, Registry, Unavailable, ZoneConnectionParam, zone_connection,
};
use reda_model::geom::{Millideg, Point};
use reda_model::{
    Corners, FreePad, InDocument, Object, OutputOrigin, PadKind, PadShape, PcbDocument, Ratio,
    Side, Uuid,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::outline::PointParam;
use super::placement::Placed;
use super::{Created, needs_pcb, pcb, positive};

/// The corner ratio of a `rounded_rect` that gives none: a radius of a quarter of the
/// shorter side.
const DEFAULT_CORNER_RATIO: Ratio = Ratio::percent(50);

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(FreePadAdd);
    registry.add(FreePadSetNet);
    registry.add(FreePadDelete);
    registry.add(OutputOriginSet);
    registry.add(OutputOriginDelete);
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
    /// The Pad's anchor: the hole sits there and Tracks connect there.
    pub at: PointParam,
    pub width: Length,
    /// The width when left out.
    #[serde(default)]
    pub height: Option<Length>,
    /// For a `rounded_rect` (50% when left out) and the uncut corners of a
    /// `chamfered_rect`.
    #[serde(default)]
    pub corner_ratio: Option<RatioParam>,
    /// For a `chamfered_rect`, which needs it.
    #[serde(default)]
    pub chamfer_ratio: Option<RatioParam>,
    /// The corners a `rounded_rect` rounds or a `chamfered_rect` chamfers. All four when
    /// left out.
    #[serde(default)]
    pub corners: Option<Corners>,
    /// Moves the copper away from `at`, in the Pad's frame.
    #[serde(default)]
    pub offset: Option<PointParam>,
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
    pub mask: Option<reda_command::MaskParam>,
    pub paste: Option<reda_command::PasteParam>,
    /// How Zone fills of its Net join it; each value left out inherits.
    #[serde(default)]
    pub zone_connection: Option<ZoneConnectionParam>,
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
        let shape = p.shape.unwrap_or(PadShape::Circle);
        let corner_ratio = p
            .corner_ratio
            .map(RatioParam::ratio)
            .or((shape == PadShape::RoundedRect).then_some(DEFAULT_CORNER_RATIO));
        let pcb = pcb(cx)?;
        if let Some(net) = p.net {
            check_net(pcb, p.kind, net, "/net")?;
        }
        let document = pcb.uuid;
        let uuid = cx.new_id();
        let pad = FreePad {
            uuid,
            kind: p.kind,
            shape,
            at: p.at.point(),
            rotation: p
                .rotation
                .map_or(Millideg::ZERO, |r| r.millideg().normalized()),
            width,
            height,
            corner_ratio,
            chamfer_ratio: p.chamfer_ratio.map(RatioParam::ratio),
            corners: p.corners.filter(|c| *c != Corners::ALL),
            offset: p
                .offset
                .map(PointParam::point)
                .filter(|o| *o != Point::default()),
            hole: p.hole.map(HoleParam::hole),
            side: p.side.filter(|s| *s != Side::Top || p.kind != PadKind::Smd),
            net: p.net,
            mask: reda_command::mask(p.mask),
            paste: reda_command::paste(p.paste),
            zone_connection: zone_connection(p.zone_connection),
        };
        if let Some((pointer, problem)) = pad.problem() {
            return Err(CommandError::invalid_params(&pointer, problem));
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
pub struct OutputOriginSetParams {
    /// Where Footprint positions are measured from in the manufacturing outputs.
    pub at: PointParam,
}

pub struct OutputOriginSet;

impl Command for OutputOriginSet {
    const META: Meta = Meta {
        id: "pcb.output_origin.set",
        title: "Set Output Origin",
        description: "Sets the Output Origin, which the editor measures coordinates and the grid \
                      from. Gerber, drill and CPL use it together when the Fab Preset asks for \
                      a relative file origin; without it they use the Board Outline's lower-left.",
        category: "pcb",
        icon: "origin",
        kind: Kind::Edit,
    };
    type Params = OutputOriginSetParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: OutputOriginSetParams) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let at = p.at.point();
        match pcb.output_origin() {
            Some(origin) => {
                let origin = OutputOrigin {
                    uuid: origin.uuid,
                    at,
                };
                let uuid = origin.uuid;
                cx.tx()?
                    .replace(Object::OutputOrigin(InDocument::new(document, origin)))?;
                Ok(Created { uuid })
            }
            None => {
                let uuid = cx.new_id();
                cx.tx()?.insert(Object::OutputOrigin(InDocument::new(
                    document,
                    OutputOrigin { uuid, at },
                )))?;
                Ok(Created { uuid })
            }
        }
    }
}

pub struct OutputOriginDelete;

impl Command for OutputOriginDelete {
    const META: Meta = Meta {
        id: "pcb.output_origin.delete",
        title: "Remove Output Origin",
        description: "Removes the Output Origin. Relative manufacturing outputs then use \
                      the lower-left of the Board Outline's bounding box.",
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
        let Some(origin) = pcb(cx)?.output_origin() else {
            return Err(CommandError::rejected("the board has no Output Origin"));
        };
        let uuid = origin.uuid;
        cx.tx()?.remove(uuid)?;
        Ok(Placed {
            objects: vec![uuid],
        })
    }
}
