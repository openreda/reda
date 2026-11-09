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
mod details;
pub(crate) mod edge;
pub mod health;
mod holes;
mod placement;
mod silkscreen;
mod zones;

use reda_command::Fonts;
use reda_model::{
    DrcCheck, DrcSeverities, LayerKind, PcbDocument, Severity, Uuid, Violation, ZoneFills,
};

use crate::Copper;
use crate::copper::Item;

/// Runs every DRC check that is not Off over `pcb`, with `fills` as its Zone fills (pass
/// current ones: [`crate::zone::current`]). Waivers are ignored. Violations come in the
/// order of the DRC table.
pub fn check(pcb: &PcbDocument, fills: &ZoneFills, fonts: &Fonts) -> Vec<Violation> {
    check_only(pcb, fills, fonts, None)
}

/// Like [`check`], running only the checks in `only` (all when `None`).
pub fn check_only(
    pcb: &PcbDocument,
    fills: &ZoneFills,
    fonts: &Fonts,
    only: Option<&[DrcCheck]>,
) -> Vec<Violation> {
    if only.is_some_and(|only| only.iter().all(|c| c.is_health())) {
        return health(pcb, only);
    }
    let mut copper = Copper::build(pcb);
    copper.set_zone_fills(pcb, Some(fills));
    let mut report = Report {
        severities: pcb.setup().severities,
        only: only.map(<[DrcCheck]>::to_vec),
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
    zones::check(&board, fills, &mut report);
    for finding in health::findings(pcb) {
        report.add(finding.check, |v| {
            let mut found = finding.violation(pcb, v.severity);
            found.check = v.check;
            found
        });
    }
    rule_invalid(pcb, &mut report);
    for opening in crate::openings::resolve(pcb)
        .into_iter()
        .filter(|o| o.geometry.is_none())
    {
        report.add(DrcCheck::OpeningCollapsed, |v| {
            v.objects(opening.objects)
                .located(pcb.uuid, opening.at)
                .on_layers(
                    pcb.layers()
                        .find(
                            match opening.kind {
                                crate::openings::Kind::Mask => LayerKind::SolderMask,
                                crate::openings::Kind::Paste => LayerKind::Paste,
                            },
                            Some(opening.side),
                        )
                        .map(|layer| layer.uuid),
                )
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
    found
        .into_iter()
        .map(|(_, mut v)| {
            details::complete(&board, &mut v);
            v
        })
        .collect()
}

/// The Violations of the health checks alone (of those in `only`, when given) that are
/// not Off, in table order. It reads neither copper nor Zone fills, so a board can run it
/// as it opens.
pub fn health(pcb: &PcbDocument, only: Option<&[DrcCheck]>) -> Vec<Violation> {
    let severities = pcb.setup().severities;
    let mut found: Vec<(DrcCheck, Violation)> = health::findings(pcb)
        .into_iter()
        .filter(|f| only.is_none_or(|only| only.contains(&f.check)))
        .filter(|f| severities.get(f.check) != Severity::Off)
        .map(|f| (f.check, f.violation(pcb, severities.get(f.check))))
        .collect();
    found.sort_by_key(|(check, _)| *check);
    found.into_iter().map(|(_, v)| v).collect()
}

/// One `rule_invalid` per rule record with a blocking problem: its first, with the
/// arguments `rule` (the kind), `value`, `problem` and, for a class record, `net_class`.
/// Its objects are the class, if any, then the record.
fn rule_invalid(pcb: &PcbDocument, report: &mut Report) {
    let mut seen = Vec::new();
    for problem in crate::rules::validation::problems(pcb) {
        if seen.contains(&problem.rule) {
            continue;
        }
        seen.push(problem.rule);
        let class = problem.net_class.and_then(|c| pcb.net_class(c));
        report.add(DrcCheck::RuleInvalid, |mut v| {
            v = v
                .with_rule(problem.rule)
                .arg("rule", problem.kind.id())
                .arg("value", problem.value.as_str())
                .arg(
                    "problem",
                    match problem.problem {
                        crate::rules::ProblemKind::Negative => "negative",
                        crate::rules::ProblemKind::MinAboveMax => "min_above_max",
                        crate::rules::ProblemKind::PreferredOutside => "preferred_outside",
                    },
                );
            if let Some(class) = class {
                v = v.arg("net_class", class.name.as_str()).object(class.uuid);
            }
            v.object(problem.rule)
        });
    }
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
    /// The checks to run, when not all.
    only: Option<Vec<DrcCheck>>,
    found: Vec<(DrcCheck, Violation)>,
}

impl Report {
    /// Reports a Violation of `check`, built by `build`, unless the check is Off.
    pub fn add(&mut self, check: DrcCheck, build: impl FnOnce(Violation) -> Violation) {
        let severity = self.severities.get(check);
        if !self.is_off(check) {
            self.found
                .push((check, build(Violation::new(check.id(), severity))));
        }
    }

    /// Whether `check` is Off, so it need not run.
    pub fn is_off(&self, check: DrcCheck) -> bool {
        self.severities.get(check) == Severity::Off
            || self
                .only
                .as_ref()
                .is_some_and(|only| !only.contains(&check))
    }
}
