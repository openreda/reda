//! The Solder Mask and Paste files: one flash per Pad opening, the Pad's copper grown (or
//! shrunk) by its margin.
//!
//! The Solder Mask image is the openings (negative polarity). It opens over every Pad on
//! that side, through-hole and non-plated Pads on both; Vias stay tented. The Paste image
//! is the stencil apertures of the SMD Pads on that side.

use super::{Plot, file_attributes, file_attributes_with, flash_pad};
use crate::gerber::Gerber;
use reda_model::Side;
use reda_pcb::openings::{self, Kind};
use reda_pcb::placed::PlacedPad;

pub(super) fn mask(plot: &Plot<'_>, side: Side) -> Vec<u8> {
    let mut g = Gerber::new(plot.gerber.x2_attributes);
    for opening in mask_openings(plot, side) {
        flash_pad(&mut g, &opening, &[]);
    }
    g.finish(&file_attributes_with(
        plot,
        format!("Soldermask,{}", side_name(side)),
        "Negative",
    ))
}
pub(super) fn mask_openings(plot: &Plot<'_>, side: Side) -> Vec<PlacedPad> {
    openings::resolve(plot.pcb)
        .into_iter()
        .filter(|o| o.side == side && o.kind == Kind::Mask)
        .filter_map(|o| o.geometry)
        .collect()
}
pub(super) fn paste(plot: &Plot<'_>, side: Side) -> Vec<u8> {
    let mut g = Gerber::new(plot.gerber.x2_attributes);
    for opening in openings::resolve(plot.pcb)
        .into_iter()
        .filter(|o| o.side == side && o.kind == Kind::Paste)
        .filter_map(|o| o.geometry)
    {
        flash_pad(&mut g, &opening, &[]);
    }
    g.finish(&file_attributes(plot, format!("Paste,{}", side_name(side))))
}
pub(super) fn side_name(side: Side) -> &'static str {
    match side {
        Side::Top => "Top",
        Side::Bottom => "Bot",
    }
}
