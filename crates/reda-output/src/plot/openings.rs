//! Solder Mask and Paste geometry resolved from the board's rules and Pad/Via overrides.
//! Each board contributes openings to the shared writer, which applies its instance transform.

use reda_model::{NotFittedPaste, Side};
use reda_pcb::openings::{self, Kind};
use reda_pcb::placed::PlacedPad;

use super::{BoardPlot, pad_template};
use crate::gerber::Gerber;

/// The aperture function of what is not copper: mask, paste and ink.
pub(super) const MATERIAL: &str = "Material";

fn flash(g: &mut Gerber, opening: &PlacedPad) {
    let (template, at) = pad_template(opening);
    g.flash(template, MATERIAL, at, &[]);
}

/// The Solder Mask file of `side`.
pub(super) fn mask(plot: &BoardPlot<'_>, side: Side, g: &mut Gerber) {
    for opening in mask_openings(plot, side) {
        flash(g, &opening);
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

/// The Paste file of `side`. With `not_fitted_paste: remove`, a Not Fitted Footprint's
/// Pads open no paste.
pub(super) fn paste(plot: &BoardPlot<'_>, side: Side, g: &mut Gerber) {
    let removed = |o: &openings::Opening| {
        plot.not_fitted_paste == NotFittedPaste::Remove
            && o.objects
                .first()
                .and_then(|&u| plot.pcb.footprint(u))
                .is_some_and(|f| f.not_fitted)
    };
    for opening in openings::resolve(plot.pcb)
        .into_iter()
        .filter(|o| o.side == side && o.kind == Kind::Paste && !removed(o))
        .filter_map(|o| o.geometry)
    {
        flash(g, &opening);
    }
}

pub(super) fn side_name(side: Side) -> &'static str {
    match side {
        Side::Top => "Top",
        Side::Bottom => "Bot",
    }
}
