//! Solder Mask and Paste geometry resolved from the board's rules and Pad/Via overrides.
//! Each board contributes openings to the shared writer, which applies its instance transform.

use reda_model::Side;
use reda_pcb::openings::{self, Kind};
use reda_pcb::placed::PlacedPad;

use super::{BoardPlot, flash_pad};
use crate::gerber::Gerber;

/// The Solder Mask file of `side`.
pub(super) fn mask(plot: &BoardPlot<'_>, side: Side, g: &mut Gerber) {
    for opening in mask_openings(plot, side) {
        flash_pad(g, &opening, &[]);
    }
}

/// The same resolved Mask openings used to clear Silkscreen on `side`.
pub(super) fn mask_openings(plot: &BoardPlot<'_>, side: Side) -> Vec<PlacedPad> {
    openings::resolve(plot.pcb)
        .into_iter()
        .filter(|o| o.side == side && o.kind == Kind::Mask)
        .filter_map(|o| o.geometry)
        .collect()
}

/// The Paste file of `side`.
pub(super) fn paste(plot: &BoardPlot<'_>, side: Side, g: &mut Gerber) {
    for opening in openings::resolve(plot.pcb)
        .into_iter()
        .filter(|o| o.side == side && o.kind == Kind::Paste)
        .filter_map(|o| o.geometry)
    {
        flash_pad(g, &opening, &[]);
    }
}

pub(super) fn side_name(side: Side) -> &'static str {
    match side {
        Side::Top => "Top",
        Side::Bottom => "Bot",
    }
}
