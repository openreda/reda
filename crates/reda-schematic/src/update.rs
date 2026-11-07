//! Update from Library. `schematic.component.outdated` marks the Components whose Library
//! Part no longer matches the copy in the part table (a content-hash mismatch);
//! `schematic.component.update_preview` shows what an update would change, and
//! `schematic.component.update_from_library` applies it. Nothing updates on its own.

mod diff;

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{Command, CommandError, Context, Kind, Meta, NoParams, PartCopy};
use reda_model::local_edit::LocalEdit;
use reda_model::{InDocument, Object, PartEntry, Project, UnitInstance, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::component::{EntryContent, ensure_entry};
use crate::natural_cmp;
use crate::normalise::{Wiring, normalise};

pub use diff::{FootprintChange, PartChange};

/// One placed Component: its Units with their sheets, in Document order, and the
/// part-table entry its first Unit uses.
pub(crate) struct Placed<'a> {
    pub component: Uuid,
    pub units: Vec<(Uuid, &'a UnitInstance)>,
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

/// The Library copy of each placed Component's Part whose hash differs from the placed
/// one. Components whose Part no registered Library holds are left out.
pub(crate) fn outdated<'a>(
    cx: &Context<'_>,
    placed: Vec<Placed<'a>>,
) -> Vec<(Placed<'a>, PartCopy)> {
    let uuids: Vec<Uuid> = placed.iter().map(|p| p.entry.part.uuid).collect();
    let copies = cx.library_parts(&uuids);
    placed
        .into_iter()
        .zip(copies)
        .filter_map(|(placed, copy)| match copy {
            Ok(copy) if copy.hash != placed.entry.hash => Some((placed, copy)),
            _ => None,
        })
        .collect()
}

/// A Component whose Library Part has a newer version.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OutdatedComponent {
    pub component: Uuid,
    pub designator: String,
    /// The Library Part, by UUID.
    pub part: Uuid,
    pub part_name: String,
    /// The content hash of the copy in the part table.
    pub placed_hash: String,
    /// The content hash of the Part in the Library now.
    pub library_hash: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct OutdatedResult {
    /// By Designator, in natural order.
    pub components: Vec<OutdatedComponent>,
}

pub struct Outdated;

impl Command for Outdated {
    const META: Meta = Meta {
        id: "schematic.component.outdated",
        title: "List Outdated Components",
        description: "Lists the Components marked \"library has a newer version\": those \
                      whose Library Part, with its Symbol and Footprints, no longer matches \
                      the copy in the part table. Components whose Part no registered \
                      Library holds are not listed. Preview an update with \
                      `schematic.component.update_preview`.",
        category: "schematic",
        icon: "library_update",
        kind: Kind::Query,
    };
    type Params = NoParams;
    type Output = OutdatedResult;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<OutdatedResult, CommandError> {
        let project = cx.project()?;
        let components = outdated(cx, placed_components(project))
            .into_iter()
            .map(|(placed, copy)| OutdatedComponent {
                component: placed.component,
                designator: placed.designator().to_owned(),
                part: copy.part.uuid,
                part_name: copy.part.name,
                placed_hash: placed.entry.hash.clone(),
                library_hash: copy.hash,
            })
            .collect();
        Ok(OutdatedResult { components })
    }
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateParams {
    /// The Components to update, by UUID; every outdated Component by default. Listed
    /// Components that are up to date are left out.
    pub components: Option<Vec<Uuid>>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateFromLibraryParams {
    /// The Components to update, by UUID; every outdated Component by default. Listed
    /// Components that are up to date are left out.
    pub components: Option<Vec<Uuid>>,
    /// How the Wires on Pins that move are normalised; the fixed defaults when left out.
    pub wiring: Option<Wiring>,
}

/// What an update changes on one Component.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ComponentUpdate {
    pub component: Uuid,
    pub designator: String,
    /// The Library Part, by UUID.
    pub part: Uuid,
    pub placed_hash: String,
    pub library_hash: String,
    /// The differences, Part first, then Symbol, then Footprints.
    pub changes: Vec<PartChange>,
    /// Whether a change (a placed Unit gone from the Symbol) stops this Component's
    /// update. Omitted when false.
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

/// The Footprint a Component has after an update, with the new copy's hash.
fn footprint_after<'a>(
    unit: &UnitInstance,
    copy: &'a PartCopy,
    changes: &[PartChange],
) -> Option<(Uuid, &'a str)> {
    let chosen = changes
        .iter()
        .find_map(|c| match c {
            PartChange::FootprintChoice { to, .. } => Some(*to),
            _ => None,
        })
        .unwrap_or(unit.footprint)?;
    let index = copy.part.footprints.iter().position(|f| *f == chosen)?;
    Some((chosen, copy.footprints.get(index)?.hash.as_str()))
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UpdateResult {
    /// By Designator, in natural order.
    pub components: Vec<ComponentUpdate>,
}

/// An update worked out for one Component.
struct Plan {
    units: Vec<(Uuid, UnitInstance)>,
    copy: PartCopy,
    report: ComponentUpdate,
}

/// The updates `p` asks for, without changing anything.
fn plan(
    cx: &Context<'_>,
    p: &UpdateParams,
    lost_local_edits: LostLocalEdits,
) -> Result<Vec<Plan>, CommandError> {
    let project = cx.project()?;
    let mut placed = placed_components(project);
    if let Some(wanted) = &p.components {
        for (i, component) in wanted.iter().enumerate() {
            if !placed.iter().any(|c| c.component == *component) {
                return Err(CommandError::invalid_params(
                    &format!("/components/{i}"),
                    format!("no Component has UUID {component}"),
                ));
            }
        }
        placed.retain(|c| wanted.contains(&c.component));
    }
    let plans = outdated(cx, placed)
        .into_iter()
        .map(|(placed, copy)| {
            let sheet = project
                .schematic_sheet(placed.units[0].0)
                .expect("the Unit's sheet exists");
            let units: Vec<&UnitInstance> = placed.units.iter().map(|(_, u)| *u).collect();
            let changes = diff::changes(sheet, placed.entry, &copy, &units);
            let lost = lost_local_edits(
                project,
                placed.component,
                footprint_after(units[0], &copy, &changes),
            );
            let report = ComponentUpdate {
                component: placed.component,
                designator: placed.designator().to_owned(),
                part: copy.part.uuid,
                placed_hash: placed.entry.hash.clone(),
                library_hash: copy.hash.clone(),
                blocked: changes.iter().any(PartChange::blocks),
                changes,
                lost_local_edits: lost,
            };
            Plan {
                units: placed
                    .units
                    .iter()
                    .map(|(s, u)| (*s, (*u).clone()))
                    .collect(),
                copy,
                report,
            }
        })
        .collect();
    Ok(plans)
}

pub struct UpdatePreview {
    pub lost_local_edits: LostLocalEdits,
}

impl Command for UpdatePreview {
    const META: Meta = Meta {
        id: "schematic.component.update_preview",
        title: "Preview Update from Library",
        description: "Shows what Update from Library would change on outdated Components, \
                      without changing anything: the Part's name, Component Type, default \
                      fields and pin-pad map, the Symbol and each alternate Footprint. A \
                      Component keeps field values it has edited. Apply it with \
                      `schematic.component.update_from_library`.",
        category: "schematic",
        icon: "library_update",
        kind: Kind::Query,
    };
    type Params = UpdateParams;
    type Output = UpdateResult;

    fn run(&self, cx: &mut Context<'_>, p: UpdateParams) -> Result<UpdateResult, CommandError> {
        let components = plan(cx, &p, self.lost_local_edits)?
            .into_iter()
            .map(|p| p.report)
            .collect();
        Ok(UpdateResult { components })
    }
}

pub struct UpdateFromLibrary;

impl Command for UpdateFromLibrary {
    const META: Meta = Meta {
        id: "schematic.component.update_from_library",
        title: "Update from Library",
        description: "Updates outdated Components to their Library Part as it is now, \
                      exactly as `schematic.component.update_preview` shows: each sheet \
                      gets a copy of the new Part (unused old copies are removed), fields \
                      the Component has not edited take the new defaults, and a Footprint \
                      that is no longer an alternate moves to the new default. Refused when \
                      a placed Unit is gone from the Symbol. The PCB is not touched: Sync \
                      carries Footprint changes there.",
        category: "schematic",
        icon: "library_update",
        kind: Kind::Edit,
    };
    type Params = UpdateFromLibraryParams;
    type Output = UpdateResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: UpdateFromLibraryParams,
    ) -> Result<UpdateResult, CommandError> {
        let wiring = p.wiring.unwrap_or_default();
        let plans = plan(
            cx,
            &UpdateParams {
                components: p.components,
            },
            no_lost_local_edits,
        )?;
        if let Some(blocked) = plans.iter().find(|p| p.report.blocked) {
            return Err(CommandError::rejected(format!(
                "{} cannot be updated: a placed Unit is no longer in the Library Symbol; \
                 delete that Unit first",
                blocked.report.designator
            )));
        }
        let mut before = BTreeMap::new();
        for plan in &plans {
            for (sheet, _) in &plan.units {
                if !before.contains_key(sheet) {
                    before.insert(*sheet, crate::sheet(cx.project()?, *sheet)?.clone());
                }
            }
        }
        let mut vacated = BTreeSet::new();
        let mut components = Vec::with_capacity(plans.len());
        for plan in plans {
            for (sheet, unit) in &plan.units {
                let content = EntryContent::from_copy(plan.copy.clone())?;
                let entry = ensure_entry(cx, *sheet, content)?;
                let unit_entry = unit.part;
                let unit = diff::updated_unit(unit, entry, &plan.report.changes);
                cx.tx()?
                    .replace(Object::Unit(InDocument::new(*sheet, unit)))?;
                vacated.insert((*sheet, unit_entry));
            }
            components.push(plan.report);
        }
        for (sheet, entry) in vacated {
            remove_if_unused(cx, sheet, entry)?;
        }
        for (sheet, body) in &before {
            normalise(cx, *sheet, body, &wiring)?;
        }
        Ok(UpdateResult { components })
    }
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
