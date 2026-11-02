//! The Solder Mask and Paste files: one flash per Pad opening, the Pad's copper grown (or
//! shrunk) by its margin.
//!
//! The Solder Mask image is the openings (negative polarity). It opens over every Pad on
//! that side, through-hole and non-plated Pads on both; Vias stay tented. The Paste image
//! is the stencil apertures of the SMD Pads on that side.

use reda_model::geom::Nm;
use reda_model::{PadKind, Side};
use reda_pcb::placed::{self, PlacedPad};

use super::{Plot, file_attributes, file_attributes_with, flash_pad};
use crate::gerber::Gerber;

/// The solder-mask margin of a Pad without its own. The board has no setting yet, so the
/// openings match the copper and the fab applies its own expansion.
const MASK_MARGIN: Nm = Nm::ZERO;

/// The solder-paste margin of a Pad without its own: apertures match the copper.
const PASTE_MARGIN: Nm = Nm::ZERO;

/// The Solder Mask file of `side`.
pub(super) fn mask(plot: &Plot<'_>, side: Side) -> Vec<u8> {
    let mut g = Gerber::new(plot.gerber.x2_attributes);
    for opening in mask_openings(plot, side) {
        flash_pad(&mut g, &opening, &[]);
    }
    let function = format!("Soldermask,{}", side_name(side));
    g.finish(&file_attributes_with(plot, function, "Negative"))
}

/// The Solder Mask openings on `side`: each Pad there grown by its mask margin.
pub(super) fn mask_openings(plot: &Plot<'_>, side: Side) -> Vec<PlacedPad> {
    pads(plot)
        .filter(|pad| match pad.kind {
            PadKind::Smd => pad.side.unwrap_or(Side::Top) == side,
            PadKind::PlatedThroughHole | PadKind::NonPlated => true,
        })
        .filter_map(|pad| grown(&pad, pad.mask_margin.unwrap_or(MASK_MARGIN)))
        .collect()
}

/// The Paste file of `side`.
pub(super) fn paste(plot: &Plot<'_>, side: Side) -> Vec<u8> {
    let mut g = Gerber::new(plot.gerber.x2_attributes);
    for pad in pads(plot) {
        let on_side = pad.kind == PadKind::Smd && pad.side.unwrap_or(Side::Top) == side;
        if on_side && let Some(grown) = grown(&pad, pad.paste_margin.unwrap_or(PASTE_MARGIN)) {
            flash_pad(&mut g, &grown, &[]);
        }
    }
    g.finish(&file_attributes(plot, format!("Paste,{}", side_name(side))))
}

/// `Top` or `Bot`, as `.FileFunction` names a side.
pub(super) fn side_name(side: Side) -> &'static str {
    match side {
        Side::Top => "Top",
        Side::Bottom => "Bot",
    }
}

/// Every Pad of the board: the Footprints' in order, then the Free Pads.
fn pads<'a>(plot: &'a Plot<'_>) -> impl Iterator<Item = PlacedPad> + 'a {
    let pcb = plot.pcb;
    pcb.footprints()
        .flat_map(move |f| placed::placed_pads(pcb, f))
        .chain(pcb.free_pads().map(placed::placed_free_pad))
}

/// The Pad grown by `margin` on every side (shrunk when negative); `None` when nothing is
/// left of it. Rounded corners grow with it; square corners stay square.
fn grown(pad: &PlacedPad, margin: Nm) -> Option<PlacedPad> {
    let (width, height) = (pad.width + margin + margin, pad.height + margin + margin);
    if width <= Nm::ZERO || height <= Nm::ZERO {
        return None;
    }
    let most = Nm(width.0.min(height.0) / 2);
    let corner_radius = pad
        .corner_radius
        .map(|r| Nm((r + margin).0.clamp(0, most.0)));
    Some(PlacedPad {
        width,
        height,
        corner_radius,
        ..pad.clone()
    })
}
