//! Replace Part: `schematic.component.replace_preview` shows, and
//! `schematic.component.replace_part` makes, the update compare with a chosen Part as the
//! new one, for every aspect.
//!
//! The Component keeps its UUID (so Sync sees a Footprint change, not a removal and an
//! addition), Designator, Not Fitted, Unit positions and field positions. `keep_fields`
//! says which values stay, Component Type is always the new Part's, the Footprint is the
//! same one by UUID, then by name, then the new default, and Units match by index. Wires
//! never move.

use reda_command::{Command, CommandError, Context, Fingerprint, Kind, Meta, PartCopy};
use reda_model::library_update::{ComponentFieldMissing, LibraryUpdateSettings, LocallyEdited};
use reda_model::local_edit::LocalEdit;
use reda_model::{Footprint, Project, UnitInstance, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::cells::{self, Cell, CellAction, CellCase};
use super::diff::{PartChange, UnitMatch};
use super::plan::{Plan, add_changes, fp_of, pins_without_pads};
use super::status::{self, Aspect, LibraryStatus};
use super::{ComponentUpdate, LostLocalEdits, Placed, Problem, apply_plans, chosen};
use crate::component::EntryContent;

/// Which of the Component's values a Replace keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeepFields {
    /// Values that differ from the old Part's defaults stay; the rest take the new Part's.
    #[default]
    Edited,
    /// Every value the Component holds stays; only fields it has no value for take the new
    /// Part's defaults.
    All,
    /// Every value takes the new Part's default, and fields the new Part lacks go.
    None,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplaceParams {
    /// The Components to replace, by UUID; they all take `part`.
    pub components: Vec<Uuid>,
    /// The new Part, by UUID, from any registered Library. It needs a Symbol.
    pub part: Uuid,
    #[serde(default)]
    pub keep_fields: KeepFields,
    /// One of the new Part's alternate Footprints, by UUID, for every Component. By default
    /// each keeps the same Footprint by UUID, else one with the same name, else takes the
    /// new Part's default.
    pub footprint: Option<Uuid>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplacePartParams {
    /// As for the preview.
    pub components: Vec<Uuid>,
    /// As for the preview.
    pub part: Uuid,
    /// As for the preview.
    #[serde(default)]
    pub keep_fields: KeepFields,
    /// As for the preview.
    pub footprint: Option<Uuid>,
    /// The `preview_hash` of a `schematic.component.replace_preview` run with the same
    /// parameters; when given, a replace that no longer matches fails with `conflict`.
    pub preview_hash: Option<String>,
}

/// What a Replace changes on one Component.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ReplaceRow {
    pub component: Uuid,
    pub designator: String,
    /// The Part it has now, by UUID, and its name.
    pub from_part: Uuid,
    pub from_part_name: String,
    /// The Footprint it gets (`null` = none).
    pub footprint: Option<Uuid>,
    /// Each placed Unit and the new Symbol's Unit it becomes, matched by index; `to` is
    /// left out for one with no match, which blocks.
    pub units: Vec<UnitReplaced>,
    /// The differences: the Part's name, the Symbol (Units, and Pins with the Nets they
    /// leave or join), then the Footprints.
    pub changes: Vec<PartChange>,
    /// The field grid: how each value goes under `keep_fields`. Component Type always
    /// takes the new Part's.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<Cell>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<Problem>,
    /// Whether a Problem stops this Component's Replace. Omitted when false.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub blocked: bool,
    /// The local Pad and graphics edits of its Locally Edited Footprint on the board that
    /// the Replace removes, each with its Pad, property and values. Omitted when none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lost_local_edits: Vec<LocalEdit>,
}

/// One placed Unit in a Replace.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UnitReplaced {
    /// The placed Unit, by UUID.
    pub unit: Uuid,
    /// Its Unit's name in the old Symbol.
    pub from: String,
    /// The name of the new Symbol's Unit at the same index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ReplacePreviewResult {
    /// The new Part, by UUID, and its name.
    pub part: Uuid,
    pub part_name: String,
    /// The Components, by Designator in natural order.
    pub components: Vec<ReplaceRow>,
    /// Pass it to `schematic.component.replace_part` with the same parameters.
    pub preview_hash: String,
}

/// A worked-out Replace: the plans and the preview.
struct Planned {
    plans: Vec<Plan>,
    preview: ReplacePreviewResult,
}

fn plan(
    cx: &Context<'_>,
    p: &ReplaceParams,
    lost_local_edits: LostLocalEdits,
) -> Result<Planned, CommandError> {
    if p.components.is_empty() {
        return Err(CommandError::invalid_params(
            "/components",
            "name at least one Component",
        ));
    }
    let project = cx.project()?;
    let placed = chosen(project, Some(&p.components), "/components")?;
    let copy = cx.library_part(p.part)?;
    if copy.symbol.is_none() {
        return Err(CommandError::rejected(format!(
            "{} has no Symbol, so it cannot replace a Component on a Schematic Sheet",
            copy.part.name
        )));
    }
    if let Some(f) = p.footprint
        && !copy.part.footprints.contains(&f)
    {
        return Err(CommandError::invalid_params(
            "/footprint",
            format!("{f} is not one of {}'s Footprints", copy.part.name),
        ));
    }
    let mut plans: Vec<Plan> = (placed.iter())
        .map(|c| plan_one(project, c, &copy, p, lost_local_edits))
        .collect();
    add_changes(project, &mut plans);
    let rows = plans
        .iter()
        .zip(&placed)
        .map(|(plan, placed)| row(plan, placed))
        .collect();
    let mut preview = ReplacePreviewResult {
        part: copy.part.uuid,
        part_name: copy.part.name.clone(),
        components: rows,
        preview_hash: String::new(),
    };
    let mut f = Fingerprint::new();
    f.add(&p.keep_fields).add(&p.footprint).add(&preview.part);
    for row in &preview.components {
        f.add(row);
    }
    preview.preview_hash = f.finish();
    Ok(Planned { plans, preview })
}

/// The Replace of one Component by `copy`.
fn plan_one(
    project: &Project,
    placed: &Placed<'_>,
    copy: &PartCopy,
    p: &ReplaceParams,
    lost_local_edits: LostLocalEdits,
) -> Plan {
    let content = EntryContent::from_copy(copy.clone()).expect("the Part has a Symbol");
    let old = placed.entry;
    let unit = placed.units[0].1;
    let mut problems = vec![];

    let mut unit_map = vec![];
    for (_, u) in &placed.units {
        match UnitMatch::Index.of(&old.symbol, &content.symbol, u.unit) {
            Some(to) => unit_map.push((u.unit, to.uuid)),
            None => {
                let name = (old.symbol.units.iter())
                    .find(|n| n.uuid == u.unit)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                problems.push(Problem::PlacedUnitRemoved { unit: name });
            }
        }
    }

    let old_fps = status::entry_footprints(placed.home, old);
    let footprint = p
        .footprint
        .or_else(|| same_footprint(&old_fps, unit.footprint, &content))
        .or_else(|| content.part.footprints.first().copied());
    let new_fps: Vec<&Footprint> = content.footprints.iter().map(|(_, f)| f).collect();
    problems.extend(pins_without_pads(
        (&old.symbol, &old.part.pin_map, fp_of(&old_fps, unit.footprint)),
        (&content.symbol, &content.part.pin_map, fp_of(&new_fps, footprint)),
    ));

    let mut cells = replace_cells(&old.part, &content.part, unit, p.keep_fields);
    cells.extend(cells::not_fitted_cell(unit, content.part.component_type));

    let hash = footprint.and_then(|f| {
        (content.footprints.iter())
            .find(|(_, fp)| fp.uuid == f)
            .map(|(h, _)| h.as_str())
    });
    let lost = lost_local_edits(project, placed.component, footprint.zip(hash));
    let report = ComponentUpdate {
        component: placed.component,
        designator: placed.designator().to_owned(),
        part: old.part.uuid,
        part_name: old.part.name.clone(),
        status: LibraryStatus::LibraryDiffers,
        reason: None,
        differs: Aspect::ALL.to_vec(),
        aspects: Aspect::ALL.to_vec(),
        changes: vec![],
        cells,
        blocked: problems.iter().any(Problem::blocks),
        problems,
        lost_local_edits: lost,
    };
    Plan {
        report,
        units: (placed.units.iter())
            .map(|(s, u)| (*s, (*u).clone()))
            .collect(),
        content: Some(content),
        footprint: Some(footprint),
        unit_match: UnitMatch::Index,
        unit_map,
    }
}

/// The new Part's Footprint that `current` (one of `old`'s) carries on to: the same
/// UUID, else the same name.
fn same_footprint(old: &[&Footprint], current: Option<Uuid>, new: &EntryContent) -> Option<Uuid> {
    let current = current?;
    if new.part.footprints.contains(&current) {
        return Some(current);
    }
    let name = &old.iter().find(|f| f.uuid == current)?.name;
    (new.footprints.iter())
        .find(|(_, f)| &f.name == name)
        .map(|(_, f)| f.uuid)
}

/// The cells of a Replace from `old` to `new` under `keep`. Component Type always takes
/// the new Part's; Locked Fields stay.
fn replace_cells(
    old: &reda_model::Part,
    new: &reda_model::Part,
    unit: &UnitInstance,
    keep: KeepFields,
) -> Vec<Cell> {
    let mut settings = LibraryUpdateSettings::default();
    if keep == KeepFields::None {
        settings.locally_edited = LocallyEdited::TakeNew;
        settings.component_field_missing = ComponentFieldMissing::Remove;
    }
    let mut out = cells::cells(old, new, unit, &settings);
    for cell in &mut out {
        if !cell.overridable() {
            continue;
        }
        if cell.cell == cells::COMPONENT_TYPE {
            cell.action = CellAction::Update;
        } else if keep == KeepFields::All {
            cell.action = if cell.case == CellCase::Added {
                CellAction::Update
            } else {
                CellAction::Reject
            };
        }
    }
    out
}

fn row(plan: &Plan, placed: &Placed<'_>) -> ReplaceRow {
    let report = &plan.report;
    let content = plan.content.as_ref().expect("a Replace plan has content");
    let units = (placed.units.iter())
        .map(|(_, u)| UnitReplaced {
            unit: u.uuid,
            from: unit_name(&placed.entry.symbol, u.unit),
            to: (plan.unit_map.iter())
                .find(|(from, _)| *from == u.unit)
                .map(|(_, to)| unit_name(&content.symbol, *to)),
        })
        .collect();
    ReplaceRow {
        component: report.component,
        designator: report.designator.clone(),
        from_part: report.part,
        from_part_name: report.part_name.clone(),
        footprint: plan.footprint.flatten(),
        units,
        changes: report.changes.clone(),
        cells: report.cells.clone(),
        problems: report.problems.clone(),
        blocked: report.blocked,
        lost_local_edits: report.lost_local_edits.clone(),
    }
}

fn unit_name(symbol: &reda_model::Symbol, unit: Uuid) -> String {
    (symbol.units.iter())
        .find(|u| u.uuid == unit)
        .map(|u| u.name.clone())
        .unwrap_or_default()
}

pub struct ReplacePreview {
    pub lost_local_edits: LostLocalEdits,
}

impl Command for ReplacePreview {
    const META: Meta = Meta {
        id: "schematic.component.replace_preview",
        title: "Preview Replace Part",
        description: "Shows what Replace Part would change, without changing anything: \
                      the Update from Library compare with `part` as the new Part, for every \
                      aspect. Per Component: its Units matched by index, the Symbol's Units \
                      and Pins with the Nets each Pin leaves or joins (Wires never move), the \
                      Footprints with their Pads and pin map, the Footprint it gets, the \
                      field grid under `keep_fields`, problems and the local Pad edits \
                      removed. Apply it with `schematic.component.replace_part`.",
        category: "schematic",
        icon: "library_update",
        kind: Kind::Query,
    };
    type Params = ReplaceParams;
    type Output = ReplacePreviewResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ReplaceParams,
    ) -> Result<ReplacePreviewResult, CommandError> {
        Ok(plan(cx, &p, self.lost_local_edits)?.preview)
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Replaced {
    /// The Components replaced, by Designator in natural order.
    pub replaced: Vec<Uuid>,
}

pub struct ReplacePart {
    pub lost_local_edits: LostLocalEdits,
}

impl Command for ReplacePart {
    const META: Meta = Meta {
        id: "schematic.component.replace_part",
        title: "Replace Part",
        description: "Gives Components another Part, from any registered Library. Each \
                      keeps its UUID (so Sync swaps its Footprint in place), Designator, Not \
                      Fitted, Unit positions and field positions. `keep_fields`: `edited` \
                      (default) keeps values that differ from the old Part's defaults, `all` \
                      keeps every value, `none` takes the new Part's. Component Type is \
                      always the new Part's; In BOM and In CPL follow `keep_fields`. The \
                      Footprint is the same one by UUID, then by name, then the new default, \
                      unless `footprint` names one. Units match by index, and a placed Unit \
                      with no match blocks that Component. Refused for a Part with no Symbol. \
                      Wires never move. Preview it with `schematic.component.replace_preview`.",
        category: "schematic",
        icon: "library_update",
        kind: Kind::Edit,
    };
    type Params = ReplacePartParams;
    type Output = Replaced;

    fn run(&self, cx: &mut Context<'_>, p: ReplacePartParams) -> Result<Replaced, CommandError> {
        let params = ReplaceParams {
            components: p.components,
            part: p.part,
            keep_fields: p.keep_fields,
            footprint: p.footprint,
        };
        let planned = plan(cx, &params, self.lost_local_edits)?;
        if let Some(hash) = &p.preview_hash
            && *hash != planned.preview.preview_hash
        {
            return Err(CommandError::conflict(
                "the Replace no longer matches the preview; run \
                 schematic.component.replace_preview again",
            ));
        }
        let blocked: Vec<&str> = (planned.plans.iter())
            .filter(|p| p.report.blocked)
            .map(|p| p.report.designator.as_str())
            .collect();
        if !blocked.is_empty() {
            return Err(CommandError::rejected(format!(
                "{} cannot be replaced: a placed Unit has no Unit at the same index in the \
                 new Symbol; delete that Unit first",
                blocked.join(", ")
            )));
        }
        let plans: Vec<&Plan> = planned.plans.iter().collect();
        let replaced = apply_plans(cx, &plans, &LibraryUpdateSettings::default())?;
        Ok(Replaced { replaced })
    }
}
