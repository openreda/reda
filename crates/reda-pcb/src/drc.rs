//! DRC: the design rules check over the PCB Document's copper, placement and silkscreen,
//! its severities and its Waivers.
//!
//! [`check`] runs every check that is not Off against the board and a current set of Zone
//! fills, and returns the Violations in the order of the DRC table, each check's in a
//! fixed order. Only `out_of_sync` is not among them: Sync lives in the engine, which
//! hands the `drc.*` Commands a [`crate::SyncCheck`] for it. The Commands refill stale
//! Zones first ([`crate::zone::current`]), add the `out_of_sync` Violation, lay the
//! board's Waivers over the result ([`reda_model::apply_waivers`]) and fingerprint the
//! board and the Change List pending against it, so a front end can tell when results
//! are stale.

mod connections;
mod copper;
mod edge;
mod holes;
mod placement;
mod silkscreen;

use reda_command::Fonts;
use reda_model::{DrcCheck, DrcSeverities, PcbDocument, Severity, Uuid, Violation, ZoneFills};

use crate::Copper;
use crate::copper::Item;

/// Runs every DRC check that is not Off over `pcb`, with `fills` as its Zone fills (pass
/// current ones: [`crate::zone::current`]). Waivers are ignored. Violations come in the
/// order of the DRC table.
pub fn check(pcb: &PcbDocument, fills: &ZoneFills, fonts: &Fonts) -> Vec<Violation> {
    let mut copper = Copper::build(pcb);
    copper.set_zone_fills(pcb, Some(fills));
    let mut report = Report {
        severities: pcb.setup().severities,
        found: vec![],
    };
    let board = Board {
        pcb,
        copper: &copper,
    };
    copper::check(&board, &mut report);
    holes::check(&board, &mut report);
    edge::check(&board, &mut report);
    placement::check(&board, &mut report);
    silkscreen::check(&board, fonts, &mut report);
    connections::check(&board, &mut report);
    for opening in crate::openings::resolve(pcb)
        .into_iter()
        .filter(|o| o.geometry.is_none())
    {
        report.add(DrcCheck::OpeningCollapsed, |v| {
            v.objects(opening.objects)
                .arg(
                    "kind",
                    match opening.kind {
                        crate::openings::Kind::Mask => "mask",
                        crate::openings::Kind::Paste => "paste",
                    },
                )
                .arg(
                    "side",
                    match opening.side {
                        reda_model::Side::Top => "top",
                        reda_model::Side::Bottom => "bottom",
                    },
                )
        });
    }
    let mut found = report.found;
    found.sort_by_key(|(check, _)| *check);
    found.into_iter().map(|(_, v)| v).collect()
}

/// The fingerprint of everything DRC reads: the PCB Document, its rules, severities and
/// Waivers included. Zone fills are derived from it, so they need no part of their own.
pub fn design_fingerprint(pcb: &PcbDocument) -> String {
    reda_command::Fingerprint::new().add(pcb).finish()
}

/// The board under check: the Document and its copper index, Zone fills included.
pub(crate) struct Board<'a> {
    pub pcb: &'a PcbDocument,
    pub copper: &'a Copper,
}

impl Board<'_> {
    /// The name of a Net, for message arguments; empty for no Net.
    pub fn net_name(&self, net: Option<Uuid>) -> String {
        net.and_then(|n| self.pcb.net(n))
            .map_or_else(String::new, |n| n.name.clone())
    }
}

/// The objects a Violation names for a piece of copper: the Footprint instance then the
/// Pad for a Pad, the stored object otherwise.
pub(crate) fn item_objects(item: Item) -> Vec<Uuid> {
    match item {
        Item::Pad { footprint, pad } => vec![footprint, pad],
        other => vec![other.object()],
    }
}

/// Collects Violations of the checks that are not Off.
pub(crate) struct Report {
    severities: DrcSeverities,
    found: Vec<(DrcCheck, Violation)>,
}

impl Report {
    /// Reports a Violation of `check`, built by `build`, unless the check is Off.
    pub fn add(&mut self, check: DrcCheck, build: impl FnOnce(Violation) -> Violation) {
        let severity = self.severities.get(check);
        if severity != Severity::Off {
            self.found
                .push((check, build(Violation::new(check.id(), severity))));
        }
    }

    /// Whether `check` is Off, so it need not run.
    pub fn is_off(&self, check: DrcCheck) -> bool {
        self.severities.get(check) == Severity::Off
    }
}
