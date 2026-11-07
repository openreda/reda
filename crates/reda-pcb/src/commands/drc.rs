//! The DRC Commands: running it, telling whether a run is stale, the severities and the
//! Waivers. They mirror ERC's (`erc.*`); the Waivers live in the PCB Document.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::{DrcCheck, InDocument, Object, Project, Severity, Uuid, Violation, apply_waivers};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{needs_pcb, pcb};
use crate::drc::{check_only, design_fingerprint};
use crate::zone;

mod scope;

pub use scope::AreaParam;
use scope::Scope;

pub(super) fn register_all(
    registry: &mut Registry,
    sync: SyncCheck,
    user_name: reda_command::UserName,
) {
    registry.add(DrcRun { sync });
    registry.add(DrcStale { sync });
    registry.add(DrcSetSeverity);
    registry.add(DrcWaive { sync, user_name });
    registry.add(DrcUnwaive);
    registry.add(DrcFix);
}

/// What DRC learns from comparing the board with the schematic, which the engine does
/// (Sync lives there).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncState {
    /// The `out_of_sync` Violation, when changes are pending and the check is not Off.
    pub violation: Option<Violation>,
    /// A hash of the pending Change List (empty when there is none), so a schematic edit
    /// that changes what Sync would do makes DRC results stale.
    pub changes: String,
}

/// Works out [`SyncState`] for a Project.
pub type SyncCheck = fn(&Project) -> SyncState;

/// The board's Violations now, ignoring Waivers, whether the Zones had to be refilled for
/// them, and the fingerprint of what was checked.
fn current(
    project: &Project,
    fonts: &reda_command::Fonts,
    sync: SyncCheck,
    only: Option<&[DrcCheck]>,
) -> Result<Checked, CommandError> {
    let pcb = project
        .pcb()
        .ok_or_else(|| CommandError::unavailable(super::NO_PCB))?;
    let state = sync(project);
    let wanted = |check: DrcCheck| only.is_none_or(|only| only.contains(&check));
    if only.is_some_and(|only| only.iter().all(|c| c.is_health())) {
        // The health checks read neither copper nor fills: nothing to refill.
        return Ok(Checked {
            violations: crate::drc::health(pcb, only),
            refilled: false,
            design: fingerprint(project, pcb, &state.changes),
        });
    }
    let stored = project.zone_fills(pcb.uuid);
    let refilled = zone::status(pcb, stored).needs_refill;
    let fills = zone::current(pcb, stored);
    let mut violations = check_only(pcb, &fills, fonts, only);
    // `out_of_sync` is the last check of the DRC table.
    let sync_violation = state.violation.filter(|_| wanted(DrcCheck::OutOfSync));
    violations.extend(sync_violation.map(|v| {
        v.located(
            pcb.uuid,
            pcb.outline().and_then(|o| o.vertices.first()).map_or(
                reda_model::geom::Point::new(
                    reda_model::geom::Nm::ZERO,
                    reda_model::geom::Nm::ZERO,
                ),
                |v| v.point(),
            ),
        )
        .on_layers(pcb.copper_layers().map(|l| l.uuid))
    }));
    let presets = crate::rules::presets::job_presets(project, pcb);
    if wanted(DrcCheck::RuleOutsidePreset) {
        violations.extend(crate::rules::presets::outside_preset_violations(
            pcb,
            &presets,
            pcb.setup().severities.get(DrcCheck::RuleOutsidePreset),
        ));
    }
    Ok(Checked {
        violations,
        refilled,
        design: fingerprint(project, pcb, &state.changes),
    })
}

/// One DRC pass over the board.
struct Checked {
    violations: Vec<Violation>,
    refilled: bool,
    design: String,
}

/// The fingerprint of the board, of the Change List pending against it and of the Fab
/// Presets its Output Jobs use, which `rule_outside_preset` compares with.
fn fingerprint(project: &Project, pcb: &reda_model::PcbDocument, changes: &str) -> String {
    let presets: Vec<_> = crate::rules::presets::job_presets(project, pcb)
        .into_iter()
        .map(|p| (p.name, p.rules))
        .collect();
    reda_command::Fingerprint::new()
        .add(&design_fingerprint(pcb))
        .add(changes)
        .add(&presets)
        .finish()
}

pub struct DrcRun {
    sync: SyncCheck,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcRunParams {
    /// Only report Violations involving one of these objects. The whole board is still
    /// checked, then the result is filtered.
    #[serde(default)]
    pub objects: Option<Vec<Uuid>>,
    /// Only report Violations with an object touching this box.
    #[serde(default)]
    pub area: Option<AreaParam>,
    /// Only report Violations on one of these Layers.
    #[serde(default)]
    pub layers: Option<Vec<super::layer::LayerRef>>,
    /// Also list the Violations that Waivers waive, each naming its Waiver.
    #[serde(default)]
    pub include_waived: bool,
    /// Run only these checks; all that are not Off when left out. The health checks
    /// alone need no Zone refill, so a board can run them as it opens.
    #[serde(default)]
    pub checks: Option<Vec<DrcCheck>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DrcRunResult {
    /// The Violations, in the order of the DRC table.
    pub violations: Vec<Violation>,
    /// How many Violations Waivers waived.
    pub waived: usize,
    /// Waivers that match no Violation any more, because their objects no longer break
    /// the check. Only checks that ran across the whole board report them, so a
    /// `partial` run reports none.
    pub unmatched_waivers: Vec<Uuid>,
    /// The run was narrowed by `objects`, `area` or `layers`, so it lists only the
    /// Violations in that scope and marks no Waiver stale.
    pub partial: bool,
    /// Whether some Zone was unfilled or stale, so the run checked a fresh fill. The fill
    /// is not stored: run `pcb.zone.fill` to keep it.
    pub refilled: bool,
    /// A fingerprint of the board the run checked. Pass it to `drc.stale` to learn
    /// whether the board was edited since.
    pub design: String,
    /// The Locally Edited Footprints: placed Footprints whose Pads or graphics were
    /// changed on the board, by Designator.
    pub locally_edited: Vec<LocallyEdited>,
}

/// A placed Footprint with local edits, as the DRC report lists it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct LocallyEdited {
    /// The Footprint instance.
    pub instance: Uuid,
    pub designator: String,
}

/// Every Locally Edited Footprint of the board, by Designator in natural order.
pub fn locally_edited(pcb: &reda_model::PcbDocument) -> Vec<LocallyEdited> {
    let mut out: Vec<LocallyEdited> = pcb
        .footprints()
        .filter(|i| {
            pcb.footprint_copy(i.footprint)
                .is_some_and(reda_model::FootprintCopy::is_locally_edited)
        })
        .map(|i| LocallyEdited {
            instance: i.uuid,
            designator: i.designator.clone(),
        })
        .collect();
    out.sort_by(|a, b| {
        crate::natural_cmp(&a.designator, &b.designator).then(a.instance.cmp(&b.instance))
    });
    out
}

impl Command for DrcRun {
    const META: Meta = Meta {
        id: "drc.run",
        title: "Run DRC",
        description: "Checks the PCB with the DRC checks that are not Off, after refilling \
                      stale Zones, and returns the Violations: each with its check id, \
                      severity, message arguments and the UUIDs of the objects involved \
                      (a Pad as its Footprint then the Pad). Waived Violations are left out \
                      unless `include_waived`. `locally_edited` lists the Locally Edited \
                      Footprints.",
        category: "pcb",
        icon: "drc",
        kind: Kind::Query,
    };
    type Params = DrcRunParams;
    type Output = DrcRunResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DrcRunParams) -> Result<DrcRunResult, CommandError> {
        run(cx.project()?, cx.fonts(), self.sync, &p)
    }
}

/// What `drc.run` returns for `p`: the board checked with the checks asked for, narrowed
/// to the scope asked for, with its Waivers laid over it. `checks.report` writes the same
/// result.
pub fn run(
    project: &Project,
    fonts: &reda_command::Fonts,
    sync: SyncCheck,
    p: &DrcRunParams,
) -> Result<DrcRunResult, CommandError> {
    let pcb = project
        .pcb()
        .ok_or_else(|| CommandError::unavailable(super::NO_PCB))?;
    let scope = Scope::resolve(pcb, p)?;
    let checked = current(project, fonts, sync, p.checks.as_deref())?;
    let partial = scope.is_narrowed();
    let found = (checked.violations.into_iter())
        .filter(|v| scope.contains(pcb, v))
        .collect();
    let waived = apply_waivers(found, pcb.waivers(), p.include_waived, |id| {
        !partial
            && DrcCheck::ALL.iter().any(|c| {
                c.id() == id
                    && pcb.setup().severities.get(*c) != Severity::Off
                    && p.checks.as_ref().is_none_or(|only| only.contains(c))
            })
    });
    Ok(DrcRunResult {
        violations: waived.violations,
        waived: waived.waived,
        unmatched_waivers: waived.unmatched,
        partial,
        refilled: checked.refilled,
        design: checked.design,
        locally_edited: locally_edited(pcb),
    })
}

pub struct DrcStale {
    sync: SyncCheck,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcStaleParams {
    /// The `design` fingerprint a `drc.run` returned.
    pub design: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DrcStaleResult {
    /// Whether the board was edited since that run, so its results are stale.
    pub stale: bool,
}

impl Command for DrcStale {
    const META: Meta = Meta {
        id: "drc.stale",
        title: "Are DRC Results Stale",
        description: "Tells whether the PCB, its rules, its DRC severities, its Waivers \
                      or the changes pending from the schematic changed since the `drc.run` that returned `design`.",
        category: "pcb",
        icon: "drc",
        kind: Kind::Query,
    };
    type Params = DrcStaleParams;
    type Output = DrcStaleResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DrcStaleParams) -> Result<DrcStaleResult, CommandError> {
        let changes = (self.sync)(cx.project()?).changes;
        Ok(DrcStaleResult {
            stale: fingerprint(cx.project()?, pcb(cx)?, &changes) != p.design,
        })
    }
}

pub struct DrcSetSeverity;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcSetSeverityParams {
    /// The DRC check.
    pub check: DrcCheck,
    pub severity: Severity,
}

impl Command for DrcSetSeverity {
    const META: Meta = Meta {
        id: "drc.set_severity",
        title: "Set DRC Severity",
        description: "Sets how one DRC check reports its Violations: `error`, `warning` or \
                      `off`. Stored in the PCB Document, next to its rules.",
        category: "pcb",
        icon: "severity",
        kind: Kind::Edit,
    };
    type Params = DrcSetSeverityParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DrcSetSeverityParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut setup = pcb.setup().clone();
        setup.severities.set(p.check, p.severity);
        cx.tx()?
            .replace(Object::BoardSetup(InDocument::new(document, setup)))?;
        Ok(())
    }
}

pub struct DrcWaive {
    sync: SyncCheck,
    user_name: reda_command::UserName,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcWaiveParams {
    /// The current Violations to waive together, each with exactly its objects.
    pub violations: Vec<DrcWaiverTarget>,
    /// One reason for every selected Violation.
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcWaiverTarget {
    pub check: DrcCheck,
    pub objects: Vec<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DrcWaiveResult {
    /// The new Waivers, in the order of the selected Violations.
    pub waivers: Vec<Uuid>,
}

impl Command for DrcWaive {
    const META: Meta = Meta {
        id: "drc.waive",
        title: "Waive DRC Violations",
        description: "Waives the selected current DRC Violations with one reason, author \
                      and creation time, as one undo step. Each Waiver is stored in the \
                      Document holding its objects and stops matching once those objects \
                      no longer break the check.",
        category: "pcb",
        icon: "waive",
        kind: Kind::Edit,
    };
    type Params = DrcWaiveParams;
    type Output = DrcWaiveResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DrcWaiveParams) -> Result<DrcWaiveResult, CommandError> {
        if p.violations.is_empty() {
            return Err(CommandError::invalid_params(
                "/violations",
                "select at least one Violation",
            ));
        }
        let violations = current(cx.project()?, cx.fonts(), self.sync, None)?.violations;
        let board = pcb(cx)?;
        let selected = reda_command::select_waivers(
            p.violations
                .iter()
                .map(|t| (t.check.id(), t.objects.as_slice())),
            &violations,
            board.waivers(),
            |_| Ok(board.uuid),
        )?;
        let waivers = reda_command::insert_waivers(cx, selected, p.reason, self.user_name)?;
        Ok(DrcWaiveResult { waivers })
    }
}

pub struct DrcUnwaive;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcUnwaiveParams {
    /// The Waivers to delete.
    pub waivers: Vec<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DrcUnwaiveResult {
    /// The deleted Waivers.
    pub waivers: Vec<Uuid>,
}

impl Command for DrcUnwaive {
    const META: Meta = Meta {
        id: "drc.unwaive",
        title: "Delete DRC Waivers",
        description: "Deletes Waivers of DRC Violations, so the Violations they waived are \
                      reported again.",
        category: "pcb",
        icon: "waive",
        kind: Kind::Edit,
    };
    type Params = DrcUnwaiveParams;
    type Output = DrcUnwaiveResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: DrcUnwaiveParams,
    ) -> Result<DrcUnwaiveResult, CommandError> {
        let pcb = pcb(cx)?;
        for (i, &uuid) in p.waivers.iter().enumerate() {
            if pcb.waiver(uuid).is_none() {
                return Err(CommandError::invalid_params(
                    &format!("/waivers/{i}"),
                    format!("{uuid} is not a Waiver on the PCB"),
                ));
            }
        }
        let tx = cx.tx()?;
        for &uuid in &p.waivers {
            tx.remove(uuid)?;
        }
        Ok(DrcUnwaiveResult { waivers: p.waivers })
    }
}

pub struct DrcFix;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcFixParams {
    /// The health check whose findings to fix: `zero_length_track`, `micro_segment`,
    /// `duplicate_copper` or `zero_area_zone`.
    pub check: DrcCheck,
    /// The objects to remove, each one the check finds (for `duplicate_copper`, the
    /// duplicate, the last object of its Violation). Left out, everything it finds.
    #[serde(default)]
    pub objects: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DrcFixResult {
    /// The objects removed.
    pub removed: Vec<Uuid>,
}

impl Command for DrcFix {
    const META: Meta = Meta {
        id: "drc.fix",
        title: "Fix Health Check Findings",
        description: "Removes what a health check finds, as one undo step: zero-length \
                      Tracks, micro-segments, duplicate Tracks and Vias (keeping the \
                      first), or Zones with no area. `objects` picks which; left out, all \
                      of them.",
        category: "pcb",
        icon: "drc",
        kind: Kind::Edit,
    };
    type Params = DrcFixParams;
    type Output = DrcFixResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DrcFixParams) -> Result<DrcFixResult, CommandError> {
        if !p.check.is_health() {
            return Err(CommandError::invalid_params(
                "/check",
                format!(
                    "`{}` is not a health check; only `zero_length_track`, \
                     `micro_segment`, `duplicate_copper` and `zero_area_zone` can be fixed",
                    p.check.id()
                ),
            ));
        }
        let found: Vec<Uuid> = crate::drc::health::findings(pcb(cx)?)
            .into_iter()
            .filter(|f| f.check == p.check)
            .map(|f| f.remove)
            .collect();
        let removed = match p.objects {
            None => found,
            Some(objects) => {
                for (i, uuid) in objects.iter().enumerate() {
                    if !found.contains(uuid) {
                        return Err(CommandError::invalid_params(
                            &format!("/objects/{i}"),
                            format!("`{}` finds nothing to fix in {uuid}", p.check.id()),
                        ));
                    }
                }
                let mut objects = objects;
                objects.dedup();
                objects
            }
        };
        if !removed.is_empty() {
            let tx = cx.tx()?;
            for &uuid in &removed {
                tx.remove(uuid)?;
            }
        }
        Ok(DrcFixResult { removed })
    }
}
