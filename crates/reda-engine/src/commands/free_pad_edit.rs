//! `pcb.free_pad.edit`: changes any field of the Pad model on a Free Pad, as Free Pad
//! Properties does (library.md#pad-model). The fields and their checks are the ones
//! `library.footprint.pad.edit` and `pcb.pad.edit` take ([`PadPatch`]).

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, HoleParam, Kind, Length, Meta,
    RatioParam, Registry, Unavailable, ZoneConnectionParam,
};
use reda_library::edit::{PadPatch, PointParam, present};
use reda_model::{Corners, FreePad, InDocument, Object, Pad, PadKind, PadShape, Side, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::local_edits::{needs_pcb, pcb};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(FreePadEdit);
}

pub struct FreePadEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreePadEditParams {
    /// The Free Pad.
    pub pad: Uuid,
    /// Turning a Pad non-plated takes it off its Net.
    pub kind: Option<PadKind>,
    pub shape: Option<PadShape>,
    pub at: Option<PointParam>,
    pub rotation: Option<Angle>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    /// `null` returns a rounded rectangle to the default ratio.
    #[serde(default, deserialize_with = "present")]
    pub corner_ratio: Option<Option<RatioParam>>,
    /// `null` removes it.
    #[serde(default, deserialize_with = "present")]
    pub chamfer_ratio: Option<Option<RatioParam>>,
    /// `null` returns to all four corners.
    #[serde(default, deserialize_with = "present")]
    pub corners: Option<Option<Corners>>,
    /// `null` puts the copper back on the anchor.
    #[serde(default, deserialize_with = "present")]
    pub offset: Option<Option<PointParam>>,
    /// `null` removes the hole.
    #[serde(default, deserialize_with = "present")]
    pub hole: Option<Option<HoleParam>>,
    /// `null` returns an SMD Pad to the top.
    #[serde(default, deserialize_with = "present")]
    pub side: Option<Option<Side>>,
    /// `null` inherits the board's rule.
    #[serde(default, deserialize_with = "present")]
    pub mask: Option<Option<reda_command::MaskParam>>,
    /// `null` inherits the board's rule.
    #[serde(default, deserialize_with = "present")]
    pub paste: Option<Option<reda_command::PasteParam>>,
    /// Replaces the whole Zone connection; `null` inherits every value.
    #[serde(default, deserialize_with = "present")]
    pub zone_connection: Option<Option<ZoneConnectionParam>>,
    /// Edits a locked Free Pad too; without it a locked one refuses.
    #[serde(default)]
    pub include_locked: bool,
}

impl Command for FreePadEdit {
    const META: Meta = Meta {
        id: "pcb.free_pad.edit",
        title: "Edit Free Pad",
        description: "Changes any of a Free Pad's kind, shape, position, rotation, size, \
                      corner and chamfer ratios and corners, offset, hole, side, mask and \
                      paste overrides and Zone connection, as `library.footprint.pad.edit` \
                      does for a Footprint's Pad. Omitted fields keep their value; `null` \
                      clears an optional one. A resized Pad keeps its ratios. Turning it \
                      non-plated takes it off its Net.",
        category: "pcb",
        icon: "pad",
        kind: Kind::Edit,
    };
    type Params = FreePadEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: FreePadEditParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let free = pcb
            .free_pad(p.pad)
            .ok_or_else(|| CommandError::not_found(format!("the PCB has no Free Pad {}", p.pad)))?
            .clone();
        reda_pcb::lock::refuse(pcb, [free.uuid], p.include_locked, "edit")?;
        let patch = PadPatch {
            kind: p.kind,
            shape: p.shape,
            at: p.at,
            rotation: p.rotation,
            width: p.width,
            height: p.height,
            corner_ratio: p.corner_ratio,
            chamfer_ratio: p.chamfer_ratio,
            corners: p.corners,
            offset: p.offset,
            hole: p.hole,
            side: p.side,
            mask: p.mask,
            paste: p.paste,
            zone_connection: p.zone_connection,
        };
        let pad = patch.apply(as_pad(&free))?;
        let edited = FreePad {
            uuid: free.uuid,
            locked: free.locked,
            kind: pad.kind,
            shape: pad.shape,
            at: pad.at,
            rotation: pad.rotation,
            width: pad.width,
            height: pad.height,
            corner_ratio: pad.corner_ratio,
            chamfer_ratio: pad.chamfer_ratio,
            corners: pad.corners,
            offset: pad.offset,
            hole: pad.hole,
            side: pad.side,
            net: free.net.filter(|_| pad.kind != PadKind::NonPlated),
            mask: pad.mask,
            paste: pad.paste,
            zone_connection: pad.zone_connection,
        };
        if let Some((pointer, problem)) = edited.problem() {
            return Err(CommandError::invalid_params(&pointer, problem));
        }
        cx.tx()?
            .replace(Object::FreePad(InDocument::new(document, edited)))?;
        Ok(())
    }
}

/// The Free Pad as a numberless Pad, for [`PadPatch`].
fn as_pad(free: &FreePad) -> Pad {
    Pad {
        uuid: free.uuid,
        number: String::new(),
        kind: free.kind,
        shape: free.shape,
        at: free.at,
        rotation: free.rotation,
        width: free.width,
        height: free.height,
        corner_ratio: free.corner_ratio,
        chamfer_ratio: free.chamfer_ratio,
        corners: free.corners,
        offset: free.offset,
        hole: free.hole,
        side: free.side,
        mask: free.mask,
        paste: free.paste.clone(),
        zone_connection: free.zone_connection,
    }
}
