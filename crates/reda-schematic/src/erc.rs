//! ERC: the electrical rules check over every Schematic Sheet, its severities and its
//! Waivers.
//!
//! [`check`] runs every check that is not Off and returns the Violations in the order of
//! the ERC table, each check's in a fixed order. The Commands lay the sheets' Waivers over
//! them ([`reda_model::apply_waivers`]) and fingerprint the design they read, so a front end
//! can tell when results are stale.

mod components;
mod details;
mod nets;
mod placement;

pub use placement::CONNECTION_GRID;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::{
    Cell, ErcCheck, ErcSettings, MatrixType, Object, Project, Severity, Uuid, Violation, Waiver,
    apply_waivers,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) fn register_all(registry: &mut Registry, user_name: reda_command::UserName) {
    registry.add(ErcRun);
    registry.add(ErcStale);
    registry.add(ErcSetSeverity);
    registry.add(ErcSetPinMatrix);
    registry.add(ErcWaive { user_name });
    registry.add(ErcUnwaive);
}

/// Runs every ERC check that is not Off over the Project's Schematic Sheets, ignoring
/// Waivers. Violations come in the order of the ERC table. `library` reads the registered
/// Libraries for `library_out_of_date`; it is only called when that check is not Off.
pub fn check(project: &Project, library: crate::PartsOf<'_>) -> Vec<Violation> {
    let mut report = Report {
        settings: &project.props().erc,
        found: vec![],
    };
    nets::check(project, &mut report);
    components::check(project, &mut report);
    placement::check(project, &mut report);
    if !report.is_off(ErcCheck::LibraryOutOfDate) {
        for found in crate::update::library_differs(project, library) {
            let aspects: Vec<&str> = (found.aspects.iter())
                .map(|a| match a {
                    crate::Aspect::Symbol => "symbol",
                    crate::Aspect::Fields => "fields",
                    crate::Aspect::Footprints => "footprints",
                })
                .collect();
            report.add(ErcCheck::LibraryOutOfDate, |v| {
                v.arg("designator", found.designator.as_str())
                    .arg("aspects", aspects.join(", "))
                    .objects(found.units.iter().copied())
            });
        }
    }
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
pub(crate) struct Report<'a> {
    settings: &'a ErcSettings,
    found: Vec<(ErcCheck, Violation)>,
}

impl Report<'_> {
    /// Reports a Violation of `check`, built by `build`, unless the check is Off. The Pin
    /// Matrix checks report through [`Report::add_cell`].
    pub(crate) fn add(&mut self, check: ErcCheck, build: impl FnOnce(Violation) -> Violation) {
        let severity = (self.settings.severities.get(check))
            .expect("a Pin Matrix check reports through its cell");
        self.push(check, severity, build);
    }

    /// Reports a Violation of a Pin Matrix cell's check, built by `build`, with the
    /// cell's severity, unless the cell is Off.
    pub(crate) fn add_cell(&mut self, cell: Cell, build: impl FnOnce(Violation) -> Violation) {
        let severity = self.settings.pin_matrix_overrides.get(cell);
        self.push(cell.check(), severity, build);
    }

    fn push(
        &mut self,
        check: ErcCheck,
        severity: Severity,
        build: impl FnOnce(Violation) -> Violation,
    ) {
        if severity != Severity::Off {
            self.found
                .push((check, build(Violation::new(check.id(), severity))));
        }
    }

    /// Whether `check` is Off (for a matrix check, all its cells), so it need not run.
    pub(crate) fn is_off(&self, check: ErcCheck) -> bool {
        !self.settings.is_on(check)
    }
}

pub struct ErcRun;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErcRunParams {
    /// Only report Violations on these Schematic Sheets: those with an object on one of
    /// them. The whole design is still checked, then the result is filtered.
    #[serde(default)]
    pub documents: Option<Vec<Uuid>>,
    /// Only report Violations involving one of these objects.
    #[serde(default)]
    pub objects: Option<Vec<Uuid>>,
    /// Run only these checks; all that are not Off when left out.
    #[serde(default)]
    pub checks: Option<Vec<ErcCheck>>,
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
    /// the check. Only checks that ran across the whole design report them, so a
    /// `partial` run reports none.
    pub unmatched_waivers: Vec<Uuid>,
    /// The run was narrowed by `documents` or `objects`, so it lists only the Violations
    /// in that scope and marks no Waiver stale.
    pub partial: bool,
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
        let library = |uuids: &[Uuid]| cx.library_parts(uuids);
        run(cx.project()?, &library, &p)
    }
}

/// What `erc.run` returns for `p`: [`check`], narrowed to the checks and scope asked for,
/// with the sheets' Waivers laid over it. `checks.report` writes the same result.
pub fn run(
    project: &Project,
    library: crate::PartsOf<'_>,
    p: &ErcRunParams,
) -> Result<ErcRunResult, CommandError> {
    for (i, document) in p.documents.iter().flatten().enumerate() {
        if project.schematic_sheet(*document).is_none() {
            return Err(CommandError::invalid_params(
                &format!("/documents/{i}"),
                format!("{document} is not a Schematic Sheet of the Project"),
            ));
        }
    }
    let wanted = |check: ErcCheck| p.checks.as_ref().is_none_or(|only| only.contains(&check));
    let partial = p.documents.is_some() || p.objects.is_some();
    let in_scope = |v: &Violation| {
        let on_documents = p.documents.as_ref().is_none_or(|documents| {
            v.document.is_some_and(|d| documents.contains(&d))
                || v.objects.iter().any(|o| {
                    project.object(*o).is_some_and(|object| {
                        matches!(object.scope(), reda_model::Scope::Document(d) if documents.contains(&d))
                    })
                })
        });
        let of_objects = (p.objects.as_ref())
            .is_none_or(|objects| v.objects.iter().any(|o| objects.contains(o)));
        on_documents && of_objects
    };
    let found: Vec<Violation> = check(project, library)
        .into_iter()
        .filter(|v| {
            (ErcCheck::ALL.iter())
                .find(|c| c.id() == v.check)
                .is_none_or(|c| wanted(*c))
        })
        .filter(in_scope)
        .collect();
    let waived = apply_waivers(found, waivers(project), p.include_waived, |id| {
        !partial
            && ErcCheck::ALL
                .iter()
                .any(|c| c.id() == id && project.props().erc.is_on(*c) && wanted(*c))
    });
    Ok(ErcRunResult {
        violations: waived.violations,
        waived: waived.waived,
        unmatched_waivers: waived.unmatched,
        partial,
        design: design_fingerprint(project),
    })
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
        if p.check.is_matrix() {
            return Err(CommandError::invalid_params(
                "/check",
                format!(
                    "the Pin Matrix sets the severity of {}, one cell at a time: use \
                     erc.set_pin_matrix",
                    p.check.id()
                ),
            ));
        }
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        props.erc.severities.set(p.check, p.severity);
        tx.replace(Object::Project(props))?;
        Ok(())
    }
}

pub struct ErcSetPinMatrix;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErcSetPinMatrixParams {
    /// The cells to set, in any order; a later entry for the same cell wins.
    pub cells: Vec<PinMatrixCellParam>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PinMatrixCellParam {
    /// One type of the pair: a Pin type, `port_in`, `port_out`, `port_bidirectional` or
    /// `unconnected`.
    pub a: MatrixType,
    /// The other type, in either order.
    pub b: MatrixType,
    /// The cell's new severity; null resets it to its default.
    pub severity: Option<Severity>,
}

impl Command for ErcSetPinMatrix {
    const META: Meta = Meta {
        id: "erc.set_pin_matrix",
        title: "Set Pin Matrix Cells",
        description: "Sets the severity of Pin Matrix cells, one, many or all, as one undo \
                      step. Each cell pairs two of the ten Pin types and Port `in`, `out` \
                      and `bidirectional`, or a Pin type with `unconnected`, and sets how \
                      `pin_conflict` or `unconnected_pin` reports that pairing. A null \
                      severity resets the cell. The Project stores only cells that differ \
                      from the default.",
        category: "schematic",
        icon: "severity",
        kind: Kind::Edit,
    };
    type Params = ErcSetPinMatrixParams;
    type Output = ();

    fn run(&self, cx: &mut Context<'_>, p: ErcSetPinMatrixParams) -> Result<(), CommandError> {
        let mut cells = Vec::with_capacity(p.cells.len());
        for (i, c) in p.cells.iter().enumerate() {
            let cell = Cell::new(c.a, c.b).ok_or_else(|| {
                CommandError::invalid_params(
                    &format!("/cells/{i}"),
                    format!(
                        "the Pin Matrix has no cell for {} and {}: only Pin types pair \
                         with unconnected",
                        c.a.id(),
                        c.b.id()
                    ),
                )
            })?;
            cells.push((cell, c.severity));
        }
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        for (cell, severity) in cells {
            props.erc.pin_matrix_overrides.set(cell, severity);
        }
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
        let library = |uuids: &[Uuid]| cx.library_parts(uuids);
        let violations = check(project, &library);
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
