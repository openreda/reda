//! ERC: the electrical rules check over every Schematic Sheet, its severities and its
//! Waivers.
//!
//! [`check`] runs every check that is not Off and returns the Violations in the order of
//! the ERC table, each check's in a fixed order. The Commands lay the sheets' Waivers over
//! them ([`reda_model::apply_waivers`]) and fingerprint the design they read, so a front end
//! can tell when results are stale.

mod components;
mod details;
mod matrix;
mod nets;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::{
    ErcCheck, ErcSeverities, Object, Project, Severity, Uuid, Violation, Waiver, apply_waivers,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) fn register_all(registry: &mut Registry, user_name: reda_command::UserName) {
    registry.add(ErcRun);
    registry.add(ErcStale);
    registry.add(ErcSetSeverity);
    registry.add(ErcWaive { user_name });
    registry.add(ErcUnwaive);
}

/// Runs every ERC check that is not Off over the Project's Schematic Sheets, ignoring
/// Waivers. Violations come in the order of the ERC table.
pub fn check(project: &Project) -> Vec<Violation> {
    let mut report = Report {
        severities: project.props().erc,
        found: vec![],
    };
    nets::check(project, &mut report);
    components::check(project, &mut report);
    let mut found = report.found;
    found.sort_by_key(|(check, _)| *check);
    found
        .into_iter()
        .map(|(_, mut v)| {
            details::complete(project, &mut v);
            v
        })
        .collect()
}

/// The fingerprint of everything ERC reads: the Project properties (with the ERC
/// severities) and every Schematic Sheet, Waivers included.
pub fn design_fingerprint(project: &Project) -> String {
    let props = project.props();
    let mut f = reda_command::Fingerprint::new();
    f.add(props);
    for document in &props.documents {
        if let Some(sheet) = project.schematic_sheet(*document) {
            f.add(sheet);
        }
    }
    f.finish()
}

/// Every Waiver on the Project's Schematic Sheets, in Document order.
fn waivers(project: &Project) -> impl Iterator<Item = &Waiver> {
    (project.documents())
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.waivers())
}

/// Collects Violations of the checks that are not Off.
pub(crate) struct Report {
    severities: ErcSeverities,
    found: Vec<(ErcCheck, Violation)>,
}

impl Report {
    /// Reports a Violation of `check`, built by `build`, unless the check is Off.
    pub(crate) fn add(&mut self, check: ErcCheck, build: impl FnOnce(Violation) -> Violation) {
        let severity = self.severities.get(check);
        if severity != Severity::Off {
            self.found
                .push((check, build(Violation::new(check.id(), severity))));
        }
    }

    /// Whether `check` is Off, so it need not run.
    pub(crate) fn is_off(&self, check: ErcCheck) -> bool {
        self.severities.get(check) == Severity::Off
    }
}

pub struct ErcRun;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErcRunParams {
    /// Also list the Violations that Waivers waive, each naming its Waiver.
    #[serde(default)]
    pub include_waived: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ErcRunResult {
    /// The Violations, in the order of the ERC table.
    pub violations: Vec<Violation>,
    /// How many Violations Waivers waived.
    pub waived: usize,
    /// Waivers that match no Violation any more, because their objects no longer break
    /// the check.
    pub unmatched_waivers: Vec<Uuid>,
    /// A fingerprint of the design the run checked. Pass it to `erc.stale` to learn
    /// whether the design was edited since.
    pub design: String,
}

impl Command for ErcRun {
    const META: Meta = Meta {
        id: "erc.run",
        title: "Run ERC",
        description: "Checks every Schematic Sheet with the ERC checks that are not Off and \
                      returns the Violations: each with its check id, severity, message \
                      arguments and the UUIDs of the objects involved. Waived Violations \
                      are left out unless `include_waived`.",
        category: "schematic",
        icon: "erc",
        kind: Kind::Query,
    };
    type Params = ErcRunParams;
    type Output = ErcRunResult;

    fn run(&self, cx: &mut Context<'_>, p: ErcRunParams) -> Result<ErcRunResult, CommandError> {
        let project = cx.project()?;
        let waived = apply_waivers(check(project), waivers(project), p.include_waived, |id| {
            ErcCheck::ALL
                .iter()
                .any(|c| c.id() == id && project.props().erc.get(*c) != Severity::Off)
        });
        Ok(ErcRunResult {
            violations: waived.violations,
            waived: waived.waived,
            unmatched_waivers: waived.unmatched,
            design: design_fingerprint(project),
        })
    }
}

pub struct ErcStale;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErcStaleParams {
    /// The `design` fingerprint an `erc.run` returned.
    pub design: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ErcStaleResult {
    /// Whether the design was edited since that run, so its results are stale.
    pub stale: bool,
}

impl Command for ErcStale {
    const META: Meta = Meta {
        id: "erc.stale",
        title: "Are ERC Results Stale",
        description: "Tells whether the schematic, its Waivers or the ERC severities changed \
                      since the `erc.run` that returned `design`.",
        category: "schematic",
        icon: "erc",
        kind: Kind::Query,
    };
    type Params = ErcStaleParams;
    type Output = ErcStaleResult;

    fn run(&self, cx: &mut Context<'_>, p: ErcStaleParams) -> Result<ErcStaleResult, CommandError> {
        Ok(ErcStaleResult {
            stale: design_fingerprint(cx.project()?) != p.design,
        })
    }
}

pub struct ErcSetSeverity;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErcSetSeverityParams {
    /// The ERC check.
    pub check: ErcCheck,
    pub severity: Severity,
}

impl Command for ErcSetSeverity {
    const META: Meta = Meta {
        id: "erc.set_severity",
        title: "Set ERC Severity",
        description: "Sets how one ERC check reports its Violations: `error`, `warning` or \
                      `off`. Stored in the Project.",
        category: "schematic",
        icon: "severity",
        kind: Kind::Edit,
    };
    type Params = ErcSetSeverityParams;
    type Output = ();

    fn run(&self, cx: &mut Context<'_>, p: ErcSetSeverityParams) -> Result<(), CommandError> {
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        props.erc.set(p.check, p.severity);
        tx.replace(Object::Project(props))?;
        Ok(())
    }
}

pub struct ErcWaive {
    user_name: reda_command::UserName,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErcWaiveParams {
    /// The current Violations to waive together, each with exactly its objects.
    pub violations: Vec<ErcWaiverTarget>,
    /// One reason for every selected Violation.
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErcWaiverTarget {
    pub check: ErcCheck,
    pub objects: Vec<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ErcWaiveResult {
    /// The new Waivers, in the order of the selected Violations.
    pub waivers: Vec<Uuid>,
}

impl Command for ErcWaive {
    const META: Meta = Meta {
        id: "erc.waive",
        title: "Waive ERC Violations",
        description: "Waives the selected current ERC Violations with one reason, author \
                      and creation time, as one undo step. Each Waiver is stored in the \
                      Document holding its objects and stops matching once those objects \
                      no longer break the check.",
        category: "schematic",
        icon: "waive",
        kind: Kind::Edit,
    };
    type Params = ErcWaiveParams;
    type Output = ErcWaiveResult;

    fn run(&self, cx: &mut Context<'_>, p: ErcWaiveParams) -> Result<ErcWaiveResult, CommandError> {
        if p.violations.is_empty() {
            return Err(CommandError::invalid_params(
                "/violations",
                "select at least one Violation",
            ));
        }
        let project = cx.project()?;
        let violations = check(project);
        let selected = reda_command::select_waivers(
            p.violations
                .iter()
                .map(|t| (t.check.id(), t.objects.as_slice())),
            &violations,
            waivers(project),
            |v| {
                v.document.ok_or_else(|| {
                    CommandError::rejected("no sheet holds this Violation's objects")
                })
            },
        )?;
        let waivers = reda_command::insert_waivers(cx, selected, p.reason, self.user_name)?;
        Ok(ErcWaiveResult { waivers })
    }
}

pub struct ErcUnwaive;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErcUnwaiveParams {
    /// The Waivers to delete.
    pub waivers: Vec<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ErcUnwaiveResult {
    /// The deleted Waivers.
    pub waivers: Vec<Uuid>,
}

impl Command for ErcUnwaive {
    const META: Meta = Meta {
        id: "erc.unwaive",
        title: "Delete ERC Waivers",
        description: "Deletes Waivers, so the Violations they waived are reported again.",
        category: "schematic",
        icon: "waive",
        kind: Kind::Edit,
    };
    type Params = ErcUnwaiveParams;
    type Output = ErcUnwaiveResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ErcUnwaiveParams,
    ) -> Result<ErcUnwaiveResult, CommandError> {
        for (i, &uuid) in p.waivers.iter().enumerate() {
            if !matches!(cx.project()?.object(uuid), Some(Object::Waiver(_))) {
                return Err(CommandError::invalid_params(
                    &format!("/waivers/{i}"),
                    format!("{uuid} is not a Waiver"),
                ));
            }
        }
        let tx = cx.tx()?;
        for &uuid in &p.waivers {
            tx.remove(uuid)?;
        }
        Ok(ErcUnwaiveResult { waivers: p.waivers })
    }
}
