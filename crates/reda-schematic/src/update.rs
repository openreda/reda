//! Library Status and Update from Library.
//!
//! `schematic.component.library_status` compares each placed Component's part-table copy
//! with its Part in the registered Libraries, aspect by aspect ([`status`]).
//! `schematic.component.update_preview` shows what an update would change, with a
//! `preview_hash`, and `schematic.component.update_from_library` applies it, failing with
//! `conflict` when the result no longer matches. Nothing updates on its own.
//!
//! Each Component takes Full Replace or any of the aspects Symbol, Fields and Footprints.
//! Taking only some writes a mixed part-table entry: the aspects not taken stay as they
//! were, and its status stays Library Differs for them. Wires never move.

mod cells;
mod diff;
mod plan;
mod replace;
mod status;

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{Command, CommandError, Context, Kind, Meta, PartCopy};
use reda_model::library_update::{
    ComponentFieldMissing, FootprintChoice, LibraryFieldMissing, LibraryUpdateSettings,
    LocallyEdited,
};
use reda_model::local_edit::LocalEdit;
use reda_model::{InDocument, Object, PartEntry, Project, SchematicSheet, UnitInstance, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::component::ensure_entry;
use crate::natural_cmp;

pub use cells::{Cell, CellAction, CellCase};
pub use diff::{FootprintChange, FromTo, PartChange};
pub use replace::{
    KeepFields, ReplaceParams, ReplacePart, ReplacePartParams, ReplacePreview,
    ReplacePreviewResult, ReplaceRow, Replaced,
};
pub use status::{Aspect, ComponentStatus, LibraryStatus};

/// One placed Component: its Units with their sheets, in Document order, and the
/// part-table entry its first Unit uses, on `home`.
pub(crate) struct Placed<'a> {
    pub component: Uuid,
    pub units: Vec<(Uuid, &'a UnitInstance)>,
    pub home: &'a SchematicSheet,
    pub entry: &'a PartEntry,
}

impl Placed<'_> {
    fn designator(&self) -> &str {
        &self.units[0].1.designator
    }
}

/// Every placed Component of the top level, once each, by Designator in natural order.
pub(crate) fn placed_components(project: &Project) -> Vec<Placed<'_>> {
    let mut seen: BTreeMap<Uuid, Placed<'_>> = BTreeMap::new();
    for sheet in project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
    {
        for unit in sheet.units().filter(|u| u.component.path.is_empty()) {
            seen.entry(unit.component.uuid)
                .or_insert_with(|| Placed {
                    component: unit.component.uuid,
                    units: vec![],
                    home: sheet,
                    entry: sheet.part(unit.part).expect("a Unit's part entry exists"),
                })
                .units
                .push((sheet.uuid, unit));
        }
    }
    let mut placed: Vec<Placed<'_>> = seen.into_values().collect();
    placed.sort_by(|a, b| {
        natural_cmp(a.designator(), b.designator()).then(a.component.cmp(&b.component))
    });
    placed
}

/// How Footprint `new` differs from `old`, as an update preview lists it: a `footprint`
/// change (`modified`) with the Pad numbers added or removed and the Pads moved or
/// resized.
pub fn footprint_change(old: &reda_model::Footprint, new: &reda_model::Footprint) -> PartChange {
    diff::modified(old, new)
}

/// Reads the registered Libraries for these Parts, in the same order.
pub type PartsOf<'a> = &'a dyn Fn(&[Uuid]) -> Vec<Result<PartCopy, CommandError>>;

/// Each placed Component with its Library copy (or why there is none).
fn compared<'a>(
    parts: PartsOf<'_>,
    placed: Vec<Placed<'a>>,
) -> Vec<(Placed<'a>, Result<PartCopy, CommandError>)> {
    let uuids: Vec<Uuid> = placed.iter().map(|p| p.entry.part.uuid).collect();
    placed.into_iter().zip(parts(&uuids)).collect()
}

/// The aspects in which `placed` differs from `copy`.
fn differs(placed: &Placed<'_>, copy: &PartCopy) -> Vec<Aspect> {
    let old_fps = status::entry_footprints(placed.home, placed.entry);
    let new_fps = status::copy_footprints(copy);
    let old = status::Content {
        part: &placed.entry.part,
        symbol: Some(&placed.entry.symbol),
        footprints: &old_fps,
    };
    let new = status::Content {
        part: &copy.part,
        symbol: copy.symbol.as_ref(),
        footprints: &new_fps,
    };
    old.differs(&new)
}

fn component_status(placed: &Placed<'_>, copy: &Result<PartCopy, CommandError>) -> ComponentStatus {
    let (status, aspects, reason) = match copy {
        Err(e) => (LibraryStatus::SourceMissing, None, Some(e.message.clone())),
        Ok(copy) => {
            let aspects = differs(placed, copy);
            if aspects.is_empty() {
                (LibraryStatus::UpToDate, None, None)
            } else {
                (LibraryStatus::LibraryDiffers, Some(aspects), None)
            }
        }
    };
    ComponentStatus {
        component: placed.component,
        designator: placed.designator().to_owned(),
        part: placed.entry.part.uuid,
        part_name: placed.entry.part.name.clone(),
        status,
        aspects,
        reason,
    }
}

/// One placed Component whose status is Library Differs.
pub(crate) struct Differing {
    pub designator: String,
    /// Its placed Units.
    pub units: Vec<Uuid>,
    pub aspects: Vec<Aspect>,
}

/// Every placed Component whose status is Library Differs: what ERC's
/// `library_out_of_date` reports.
pub(crate) fn library_differs(project: &Project, parts: PartsOf<'_>) -> Vec<Differing> {
    compared(parts, placed_components(project))
        .into_iter()
        .filter_map(|(placed, copy)| {
            let aspects = differs(&placed, copy.as_ref().ok()?);
            (!aspects.is_empty()).then(|| Differing {
                designator: placed.designator().to_owned(),
                units: placed.units.iter().map(|(_, u)| u.uuid).collect(),
                aspects,
            })
        })
        .collect()
}

fn statuses(parts: PartsOf<'_>, placed: Vec<Placed<'_>>) -> Vec<ComponentStatus> {
    compared(parts, placed)
        .iter()
        .map(|(placed, copy)| component_status(placed, copy))
        .collect()
}

/// Every placed Component's Library Status, as `schematic.component.library_status`
/// gives it, for front ends that show it without running a Command.
pub fn library_statuses(project: &Project, parts: PartsOf<'_>) -> Vec<ComponentStatus> {
    statuses(parts, placed_components(project))
}

/// The placed Components `wanted` names, or every one; `invalid_params` at
/// `<param>/<i>` for a UUID no placed Component has.
fn chosen<'a>(
    project: &'a Project,
    wanted: Option<&[Uuid]>,
    param: &str,
) -> Result<Vec<Placed<'a>>, CommandError> {
    let mut placed = placed_components(project);
    if let Some(wanted) = wanted {
        for (i, component) in wanted.iter().enumerate() {
            if !placed.iter().any(|c| c.component == *component) {
                return Err(CommandError::invalid_params(
                    &format!("{param}/{i}"),
                    format!("no Component has UUID {component}"),
                ));
            }
        }
        placed.retain(|c| wanted.contains(&c.component));
    }
    Ok(placed)
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryStatusParams {
    /// The Components to compare, by UUID; every placed Component by default.
    pub components: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct LibraryStatusResult {
    /// By Designator, in natural order.
    pub components: Vec<ComponentStatus>,
}

pub struct LibraryStatusQuery;

impl Command for LibraryStatusQuery {
    const META: Meta = Meta {
        id: "schematic.component.library_status",
        title: "Library Status",
        description: "Compares each placed Component's copy in the part table with its Part \
                      in the registered Libraries: `up_to_date`, `library_differs` naming \
                      the aspects that differ (`symbol`, `fields`, `footprints`), or \
                      `source_missing` when no registered Library holds the Part or an \
                      element it references. Preview an update with \
                      `schematic.component.update_preview`.",
        category: "schematic",
        icon: "library_update",
        kind: Kind::Query,
    };
    type Params = LibraryStatusParams;
    type Output = LibraryStatusResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: LibraryStatusParams,
    ) -> Result<LibraryStatusResult, CommandError> {
        let project = cx.project()?;
        let placed = chosen(project, p.components.as_deref(), "/components")?;
        let parts = |uuids: &[Uuid]| cx.library_parts(uuids);
        let components = statuses(&parts, placed);
        Ok(LibraryStatusResult { components })
    }
}

/// The aspects one Component takes.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentAspects {
    pub component: Uuid,
    pub aspects: Vec<Aspect>,
}

/// Settings that override the ones saved in the Project for this run. Each left out
/// takes the saved value, or its default.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsParams {
    pub library_field_missing: Option<LibraryFieldMissing>,
    pub component_field_missing: Option<ComponentFieldMissing>,
    pub locally_edited: Option<LocallyEdited>,
    /// The cells that may start as Update: field names, `component_type`, `in_bom` and
    /// `in_cpl`.
    pub fields_to_update: Option<Vec<String>>,
    pub add_new_footprints: Option<bool>,
    pub remove_missing_footprints: Option<bool>,
    pub footprint_choice: Option<FootprintChoice>,
    pub preserve_field_locations: Option<bool>,
    pub preserve_field_visibility: Option<bool>,
}

impl SettingsParams {
    /// `saved` with these overrides.
    pub fn over(&self, saved: &LibraryUpdateSettings) -> LibraryUpdateSettings {
        let mut s = saved.clone();
        macro_rules! take {
            ($($f:ident),*) => {$(if let Some(v) = &self.$f { s.$f = v.clone(); })*};
        }
        take!(
            library_field_missing,
            component_field_missing,
            locally_edited,
            add_new_footprints,
            remove_missing_footprints,
            footprint_choice,
            preserve_field_locations,
            preserve_field_visibility
        );
        if let Some(names) = &self.fields_to_update {
            s.fields_to_update = Some(names.clone());
        }
        s
    }
}

/// Which Components an update covers and how: what `update_preview` and
/// `update_from_library` share.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateParams {
    /// The Components to update, by UUID (Selected Parts Only); every placed Component by
    /// default. Up to Date ones are left out.
    pub components: Option<Vec<Uuid>>,
    /// Only the Components with a Unit on one of these sheets. A Component on several
    /// sheets is updated as a whole.
    pub sheets: Option<Vec<Uuid>>,
    /// Only the Components placed from these Library Parts, by UUID.
    pub parts: Option<Vec<Uuid>>,
    /// The aspects every Component takes; all three (Full Replace) by default.
    pub aspects: Option<Vec<Aspect>>,
    /// The aspects of single Components, over `aspects`.
    #[serde(default)]
    pub component_aspects: Vec<ComponentAspects>,
    /// Settings over the ones saved in the Project.
    pub settings: Option<SettingsParams>,
}

/// One cell of one Component, for overriding how it starts.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CellRef {
    pub component: Uuid,
    /// The cell: a field's name, or `component_type`, `in_bom` or `in_cpl`.
    pub cell: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateFromLibraryParams {
    /// The `preview_hash` of the `schematic.component.update_preview` this applies, run
    /// with the same scope, aspects and settings.
    pub preview_hash: String,
    /// As for the preview.
    pub components: Option<Vec<Uuid>>,
    /// As for the preview.
    pub sheets: Option<Vec<Uuid>>,
    /// As for the preview.
    pub parts: Option<Vec<Uuid>>,
    /// As for the preview.
    pub aspects: Option<Vec<Aspect>>,
    /// As for the preview.
    #[serde(default)]
    pub component_aspects: Vec<ComponentAspects>,
    /// As for the preview. The settings used are saved in the Project.
    pub settings: Option<SettingsParams>,
    /// Components of the preview left as they are.
    #[serde(default)]
    pub excluded_components: Vec<Uuid>,
    /// Cells rejected (Reject Selected): the Component keeps its value.
    #[serde(default)]
    pub excluded_cells: Vec<CellRef>,
    /// Cells updated (Update Selected): the Component takes the new default.
    #[serde(default)]
    pub updated_cells: Vec<CellRef>,
}

impl UpdateFromLibraryParams {
    fn scope(&self) -> UpdateParams {
        UpdateParams {
            components: self.components.clone(),
            sheets: self.sheets.clone(),
            parts: self.parts.clone(),
            aspects: self.aspects.clone(),
            component_aspects: self.component_aspects.clone(),
            settings: self.settings.clone(),
        }
    }
}

/// Something in an update that needs the user: a blocking one stops that Component.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Problem {
    /// A placed Unit is gone from the Symbol: delete it first. Blocks.
    PlacedUnitRemoved { unit: String },
    /// The Library Part has no Symbol, so the Symbol cannot be taken. Blocks.
    NoSymbol,
    /// The mix leaves a Pin with no Pad on the chosen Footprint.
    PinWithoutPad { unit: String, number: String },
}

impl Problem {
    pub fn blocks(&self) -> bool {
        !matches!(self, Problem::PinWithoutPad { .. })
    }
}

/// What an update changes on one Component.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ComponentUpdate {
    pub component: Uuid,
    pub designator: String,
    /// The Library Part, by UUID.
    pub part: Uuid,
    pub part_name: String,
    /// `library_differs` or `source_missing`; a Source Missing row cannot be updated.
    pub status: LibraryStatus,
    /// For Source Missing: what is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The aspects that differ.
    pub differs: Vec<Aspect>,
    /// The aspects the update takes: those asked for that differ.
    pub aspects: Vec<Aspect>,
    /// The differences of the aspects taken: Fields, Symbol, then Footprints.
    pub changes: Vec<PartChange>,
    /// The field grid, when the Fields aspect is taken.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<Cell>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<Problem>,
    /// Whether a Problem stops this Component's update. Omitted when false.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub blocked: bool,
    /// The local Pad and graphics edits of its Locally Edited Footprint on the board that
    /// the update removes, because it changes that Footprint's hash. Omitted when none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lost_local_edits: Vec<LocalEdit>,
}

/// Which local edits on the board an update removes: given the Component and the
/// Footprint it will have (`None` for none) with that copy's hash, the edits of its Locally
/// Edited Footprint when that Footprint or hash differs from the one it was edited from.
pub type LostLocalEdits = fn(&Project, Uuid, Option<(Uuid, &str)>) -> Vec<LocalEdit>;

/// A [`LostLocalEdits`] for a registry without the engine: it finds none.
pub fn no_lost_local_edits(_: &Project, _: Uuid, _: Option<(Uuid, &str)>) -> Vec<LocalEdit> {
    Vec::new()
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UpdateResult {
    /// The Components in scope that are not Up to Date, by Designator in natural order.
    pub components: Vec<ComponentUpdate>,
    /// The settings used: the saved ones with the parameters over them.
    pub settings: LibraryUpdateSettings,
    /// Pass it to `schematic.component.update_from_library` with the same parameters.
    pub preview_hash: String,
}

pub struct UpdatePreview {
    pub lost_local_edits: LostLocalEdits,
}

impl Command for UpdatePreview {
    const META: Meta = Meta {
        id: "schematic.component.update_preview",
        title: "Preview Update from Library",
        description: "Shows what Update from Library would change, without changing \
                      anything: per Component its Library Status, the aspects taken (all \
                      three, Full Replace, by default), the Symbol's Units and Pins with \
                      the Nets each Pin leaves or joins, the Footprints with their Pads and \
                      pin map, the field grid (old default, new default, the Component's \
                      value and how each cell starts), problems and the local Pad edits \
                      removed. Apply it with `schematic.component.update_from_library` \
                      and the returned `preview_hash`.",
        category: "schematic",
        icon: "library_update",
        kind: Kind::Query,
    };
    type Params = UpdateParams;
    type Output = UpdateResult;

    fn run(&self, cx: &mut Context<'_>, p: UpdateParams) -> Result<UpdateResult, CommandError> {
        let planned = plan::plan(cx, &p, self.lost_local_edits)?;
        Ok(UpdateResult {
            preview_hash: planned.hash(),
            settings: planned.settings,
            components: planned.plans.into_iter().map(|p| p.report).collect(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UpdateApplied {
    /// The Components updated, by Designator in natural order.
    pub updated: Vec<Uuid>,
    /// The Components of the preview left as they were: excluded, taking no aspect, or
    /// Source Missing.
    pub not_updated: Vec<Uuid>,
}

pub struct UpdateFromLibrary {
    pub lost_local_edits: LostLocalEdits,
}

impl Command for UpdateFromLibrary {
    const META: Meta = Meta {
        id: "schematic.component.update_from_library",
        title: "Update from Library",
        description: "Applies the update `schematic.component.update_preview` showed, given \
                      its `preview_hash` and the same parameters; fails with `conflict` when \
                      the result no longer matches. Excluded Components stay as they are, \
                      and cells start as the preview says unless `excluded_cells` or \
                      `updated_cells` override them (Locked cells cannot be). Each taken \
                      aspect replaces that part of the sheet's copy, so a partial update \
                      writes a mixed entry; the Fields aspect moves every default to the \
                      Library's whichever way each cell went. Refused while an included \
                      Component is blocked. Wires never move, and the PCB is not touched: \
                      Sync carries Footprint changes there. The settings used are saved \
                      in the Project.",
        category: "schematic",
        icon: "library_update",
        kind: Kind::Edit,
    };
    type Params = UpdateFromLibraryParams;
    type Output = UpdateApplied;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: UpdateFromLibraryParams,
    ) -> Result<UpdateApplied, CommandError> {
        let planned = plan::plan(cx, &p.scope(), self.lost_local_edits)?;
        if planned.hash() != p.preview_hash {
            return Err(CommandError::conflict(
                "the update no longer matches the preview; run \
                 schematic.component.update_preview again",
            ));
        }
        let plan::Planned {
            mut plans,
            settings,
            saved,
        } = planned;
        for (i, c) in p.excluded_components.iter().enumerate() {
            if !plans.iter().any(|p| p.report.component == *c) {
                return Err(CommandError::invalid_params(
                    &format!("/excluded_components/{i}"),
                    format!("Component {c} is not in the preview"),
                ));
            }
        }
        for (param, list, action) in [
            ("/excluded_cells", &p.excluded_cells, CellAction::Reject),
            ("/updated_cells", &p.updated_cells, CellAction::Update),
        ] {
            for (i, r) in list.iter().enumerate() {
                let pointer = format!("{param}/{i}");
                let plan = (plans.iter_mut())
                    .find(|p| p.report.component == r.component)
                    .ok_or_else(|| {
                        CommandError::invalid_params(
                            &pointer,
                            format!("Component {} is not in the preview", r.component),
                        )
                    })?;
                plan.override_cell(&pointer, &r.cell, action)?;
            }
        }
        let applied: Vec<&plan::Plan> = (plans.iter())
            .filter(|plan| plan.applies())
            .filter(|plan| !p.excluded_components.contains(&plan.report.component))
            .collect();
        let blocked: Vec<&str> = (applied.iter())
            .filter(|p| p.report.blocked)
            .map(|p| p.report.designator.as_str())
            .collect();
        if !blocked.is_empty() {
            return Err(CommandError::rejected(format!(
                "{} cannot be updated: a placed Unit is no longer in the Library Symbol, or \
                 the Library Part has no Symbol; delete that Unit, leave out the Symbol, or \
                 exclude the Component",
                blocked.join(", ")
            )));
        }
        let updated = apply_plans(cx, &applied, &settings)?;
        if p.settings.is_some() && saved.as_ref() != Some(&settings) {
            let mut props = cx.project()?.props().clone();
            props.library_update = Some(settings);
            cx.tx()?.replace(Object::Project(props))?;
        }
        let not_updated = (plans.iter())
            .map(|p| p.report.component)
            .filter(|c| !updated.contains(c))
            .collect();
        Ok(UpdateApplied {
            updated,
            not_updated,
        })
    }
}

/// Points each plan's Units at its new content, and removes the entries and copies no
/// Unit uses any more. Returns the Components changed.
fn apply_plans(
    cx: &mut Context<'_>,
    plans: &[&plan::Plan],
    settings: &LibraryUpdateSettings,
) -> Result<Vec<Uuid>, CommandError> {
    let mut vacated = BTreeSet::new();
    let mut changed = Vec::with_capacity(plans.len());
    for plan in plans {
        for (sheet, unit) in &plan.units {
            let entry = ensure_entry(cx, *sheet, plan.content())?;
            let after = plan.updated_unit(unit, entry, settings);
            cx.tx()?
                .replace(Object::Unit(InDocument::new(*sheet, after)))?;
            vacated.insert((*sheet, unit.part));
        }
        changed.push(plan.report.component);
    }
    for (sheet, entry) in vacated {
        remove_if_unused(cx, sheet, entry)?;
    }
    Ok(changed)
}

/// Removes the part-table entry `entry` from `sheet` when no Unit uses it any more, and
/// then each of its Footprint copies no other entry lists.
fn remove_if_unused(cx: &mut Context<'_>, sheet: Uuid, entry: Uuid) -> Result<(), CommandError> {
    let body = crate::sheet(cx.project()?, sheet)?;
    if body.units().any(|u| u.part == entry) {
        return Ok(());
    }
    let Some(old) = body.part(entry) else {
        return Ok(());
    };
    let copies: Vec<Uuid> = old
        .footprints
        .iter()
        .copied()
        .filter(|c| {
            !body
                .parts()
                .any(|e| e.uuid != entry && e.footprints.contains(c))
        })
        .collect();
    let tx = cx.tx()?;
    tx.remove(entry)?;
    for copy in copies {
        tx.remove(copy)?;
    }
    Ok(())
}
