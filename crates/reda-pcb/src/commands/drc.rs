//! The DRC Commands: running it, telling whether a run is stale, the severities and the
//! Waivers. They mirror ERC's (`erc.*`); the Waivers live in the PCB Document.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::{DrcCheck, InDocument, Object, Project, Severity, Uuid, Violation, apply_waivers};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{needs_pcb, pcb};
use crate::drc::{check, design_fingerprint};
use crate::zone;

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
fn current(cx: &Context<'_>, sync: SyncCheck) -> Result<Checked, CommandError> {
    let pcb = pcb(cx)?;
    let project = cx.project()?;
    let stored = project.zone_fills(pcb.uuid);
    let refilled = zone::status(pcb, stored).needs_refill;
    let fills = zone::current(pcb, stored);
    let state = sync(project);
    let mut violations = check(pcb, &fills, cx.fonts());
    // `out_of_sync` is the last check of the DRC table.
    violations.extend(state.violation.map(|v| {
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
    Ok(Checked {
        violations,
        refilled,
        design: fingerprint(pcb, &state.changes),
    })
}

/// One DRC pass over the board.
struct Checked {
    violations: Vec<Violation>,
    refilled: bool,
    design: String,
}

/// The fingerprint of the board and of the Change List pending against it.
fn fingerprint(pcb: &reda_model::PcbDocument, changes: &str) -> String {
    reda_command::Fingerprint::new()
        .add(&design_fingerprint(pcb))
        .add(changes)
        .finish()
}

pub struct DrcRun {
    sync: SyncCheck,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcRunParams {
    /// Also list the Violations that Waivers waive, each naming its Waiver.
    #[serde(default)]
    pub include_waived: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DrcRunResult {
    /// The Violations, in the order of the DRC table.
    pub violations: Vec<Violation>,
    /// How many Violations Waivers waived.
    pub waived: usize,
    /// Waivers that match no Violation any more, because their objects no longer break
    /// the check.
    pub unmatched_waivers: Vec<Uuid>,
    /// Whether some Zone was unfilled or stale, so the run checked a fresh fill. The fill
    /// is not stored: run `pcb.zone.fill` to keep it.
    pub refilled: bool,
    /// A fingerprint of the board the run checked. Pass it to `drc.stale` to learn
    /// whether the board was edited since.
    pub design: String,
}

impl Command for DrcRun {
    const META: Meta = Meta {
        id: "drc.run",
        title: "Run DRC",
        description: "Checks the PCB with the DRC checks that are not Off, after refilling \
                      stale Zones, and returns the Violations: each with its check id, \
                      severity, message arguments and the UUIDs of the objects involved \
                      (a Pad as its Footprint then the Pad). Waived Violations are left out \
                      unless `include_waived`.",
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
        let checked = current(cx, self.sync)?;
        let pcb = pcb(cx)?;
        let waived = apply_waivers(checked.violations, pcb.waivers(), p.include_waived, |id| {
            DrcCheck::ALL
                .iter()
                .any(|c| c.id() == id && pcb.setup().severities.get(*c) != Severity::Off)
        });
        Ok(DrcRunResult {
            violations: waived.violations,
            waived: waived.waived,
            unmatched_waivers: waived.unmatched,
            refilled: checked.refilled,
            design: checked.design,
        })
    }
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
            stale: fingerprint(pcb(cx)?, &changes) != p.design,
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
        let violations = current(cx, self.sync)?.violations;
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
