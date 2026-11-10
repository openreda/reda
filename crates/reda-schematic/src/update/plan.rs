//! Working out an update without changing anything: per Component the mixed part-table
//! content it gets, its cells, changes, problems and lost local edits. The preview reports
//! it, and the update applies it.

use reda_command::{CommandError, Context, Fingerprint, PartCopy};
use reda_model::library_update::{FootprintChoice, LibraryUpdateSettings};
use reda_model::{
    Footprint, InDocument, Object, PartEntry, PinMap, Project, Symbol, UnitInstance, Uuid,
};

use super::cells::{self, CellAction, CellCase};
use super::diff::{self, PartChange, PinNets, UnitMatch};
use super::status::{self, Aspect, Content, LibraryStatus};
use super::{
    ComponentUpdate, LostLocalEdits, Placed, Problem, UpdateParams, chosen, compared, differs,
};
use crate::component::EntryContent;
use crate::connectivity::{SheetConnectivity, project_connectivity};

/// Every Component an update covers, with the settings it uses.
pub(crate) struct Planned {
    pub plans: Vec<Plan>,
    /// The settings used.
    pub settings: LibraryUpdateSettings,
    /// The settings saved in the Project.
    pub saved: Option<LibraryUpdateSettings>,
}

impl Planned {
    /// The preview hash: of the settings and every Component's report.
    pub fn hash(&self) -> String {
        let mut f = Fingerprint::new();
        f.add(&self.settings);
        for plan in &self.plans {
            f.add(&plan.report);
        }
        f.finish()
    }
}

/// The update of one Component.
pub(crate) struct Plan {
    pub report: ComponentUpdate,
    /// Its Units as they are, with their sheets.
    pub units: Vec<(Uuid, UnitInstance)>,
    /// The part-table content its Units get; `None` for Source Missing.
    pub content: Option<EntryContent>,
    /// The Footprint choice after, when the Footprints aspect is taken.
    pub footprint: Option<Option<Uuid>>,
    /// How the Symbol's Units before and after correspond.
    pub unit_match: UnitMatch,
    /// Each placed Unit's Symbol Unit before and after, where they differ.
    pub unit_map: Vec<(Uuid, Uuid)>,
}

impl Plan {
    /// Whether applying it changes the Component.
    pub fn applies(&self) -> bool {
        self.report.status == LibraryStatus::LibraryDiffers
            && !self.report.aspects.is_empty()
            && self.content.is_some()
    }

    pub fn content(&self) -> EntryContent {
        self.content.clone().expect("an applied plan has content")
    }

    /// Sets how cell `name` goes, for `pointer` in errors.
    pub fn override_cell(
        &mut self,
        pointer: &str,
        name: &str,
        action: CellAction,
    ) -> Result<(), CommandError> {
        let designator = self.report.designator.clone();
        let cell = (self.report.cells.iter_mut())
            .find(|c| c.cell == name)
            .ok_or_else(|| {
                CommandError::invalid_params(
                    pointer,
                    format!("{designator} has no cell {name:?} in the preview"),
                )
            })?;
        match cell.case {
            CellCase::Locked => Err(CommandError::rejected(format!(
                "{name} is a Locked Field of {designator}; unlock it to update it"
            ))),
            CellCase::Cleared => Err(CommandError::rejected(format!(
                "{designator} becomes a Net Tie, which cannot be Not Fitted"
            ))),
            CellCase::Same => Ok(()),
            _ => {
                cell.action = action;
                Ok(())
            }
        }
    }

    /// `unit` as the update leaves it, pointing at part-table entry `entry`.
    pub fn updated_unit(
        &self,
        unit: &UnitInstance,
        entry: Uuid,
        settings: &LibraryUpdateSettings,
    ) -> UnitInstance {
        let mut unit = unit.clone();
        unit.part = entry;
        if let Some((_, to)) = self.unit_map.iter().find(|(from, _)| *from == unit.unit) {
            unit.unit = *to;
        }
        cells::apply(&mut unit, &self.report.cells);
        // A Net Tie refuses Not Fitted, so becoming one clears it.
        if !unit.component_type.allows_not_fitted() {
            unit.not_fitted = false;
        }
        if let Some(footprint) = self.footprint {
            unit.footprint = footprint;
        }
        if !settings.preserve_field_locations {
            unit.field_text.clear();
        }
        if !settings.preserve_field_visibility {
            unit.visible_fields = UnitInstance::default_visible_fields();
        }
        unit
    }
}

/// The updates `p` asks for.
pub(crate) fn plan(
    cx: &Context<'_>,
    p: &UpdateParams,
    lost_local_edits: LostLocalEdits,
) -> Result<Planned, CommandError> {
    let project = cx.project()?;
    let saved = project.props().library_update.clone();
    let base = saved.clone().unwrap_or_default();
    let settings = p.settings.as_ref().map_or(base.clone(), |s| s.over(&base));
    for (i, c) in p.component_aspects.iter().enumerate() {
        chosen(project, Some(&[c.component]), "").map_err(|_| {
            CommandError::invalid_params(
                &format!("/component_aspects/{i}/component"),
                format!("no Component has UUID {}", c.component),
            )
        })?;
    }
    let mut placed = chosen(project, p.components.as_deref(), "/components")?;
    if let Some(sheets) = &p.sheets {
        for (i, sheet) in sheets.iter().enumerate() {
            if project.schematic_sheet(*sheet).is_none() {
                return Err(CommandError::invalid_params(
                    &format!("/sheets/{i}"),
                    format!("no Schematic Sheet has UUID {sheet}"),
                ));
            }
        }
        placed.retain(|c| c.units.iter().any(|(s, _)| sheets.contains(s)));
    }
    if let Some(parts) = &p.parts {
        placed.retain(|c| parts.contains(&c.entry.part.uuid));
    }
    let parts = |uuids: &[Uuid]| cx.library_parts(uuids);
    let mut plans = vec![];
    for (placed, copy) in compared(&parts, placed) {
        let asked = (p.component_aspects.iter())
            .rev()
            .find(|a| a.component == placed.component)
            .map(|a| a.aspects.as_slice())
            .or(p.aspects.as_deref())
            .unwrap_or(&Aspect::ALL);
        if let Some(plan) = first_pass(project, &placed, copy, asked, &settings, lost_local_edits)?
        {
            plans.push(plan);
        }
    }
    add_changes(project, &mut plans)?;
    Ok(Planned {
        plans,
        settings,
        saved,
    })
}

/// Everything but the Fields name change, Symbol and Footprints changes, which need every
/// plan: `None` for an Up to Date Component. A part entry naming a Footprint copy its sheet
/// lacks is an error, not a panic (the model's checks keep such a Project from opening).
fn first_pass(
    project: &Project,
    placed: &Placed<'_>,
    copy: Result<PartCopy, CommandError>,
    asked: &[Aspect],
    settings: &LibraryUpdateSettings,
    lost_local_edits: LostLocalEdits,
) -> Result<Option<Plan>, CommandError> {
    let units: Vec<(Uuid, UnitInstance)> = (placed.units.iter())
        .map(|(s, u)| (*s, (*u).clone()))
        .collect();
    let mut report = ComponentUpdate {
        component: placed.component,
        designator: placed.designator().to_owned(),
        part: placed.entry.part.uuid,
        part_name: placed.entry.part.name.clone(),
        status: LibraryStatus::LibraryDiffers,
        reason: None,
        differs: vec![],
        aspects: vec![],
        changes: vec![],
        cells: vec![],
        problems: vec![],
        blocked: false,
        lost_local_edits: vec![],
    };
    let copy = match copy {
        Ok(copy) => copy,
        Err(e) => {
            report.status = LibraryStatus::SourceMissing;
            report.reason = Some(e.message);
            return Ok(Some(Plan {
                report,
                units,
                content: None,
                footprint: None,
                unit_match: UnitMatch::Uuid,
                unit_map: vec![],
            }));
        }
    };
    report.differs = differs(placed, &copy);
    if report.differs.is_empty() {
        return Ok(None);
    }
    report.aspects = (report.differs.iter())
        .copied()
        .filter(|a| asked.contains(a))
        .collect();
    let taken = |a: Aspect| report.aspects.contains(&a);
    let unit = placed.units[0].1;
    let old = placed.entry;
    let home = placed.home;

    // The mixed content: each aspect taken from the Library, the rest as it was.
    let mut part = old.part.clone();
    let mut symbol = old.symbol.clone();
    let old_copies: Vec<(String, Footprint)> = (old.footprints.iter())
        .map(|c| {
            let c = home.footprint(*c).ok_or_else(|| {
                corrupt(format!(
                    "part entry {} names Footprint copy {c}, which its sheet does not hold",
                    old.uuid
                ))
            })?;
            Ok((c.hash.clone(), c.footprint.clone()))
        })
        .collect::<Result<_, CommandError>>()?;
    let mut footprints = old_copies.clone();
    if taken(Aspect::Fields) {
        let new = &copy.part;
        part.name.clone_from(&new.name);
        part.fields = new.fields.clone();
        part.component_type = new.component_type;
        part.in_bom = new.in_bom;
        part.in_cpl = new.in_cpl;
        part.provenance.clone_from(&new.provenance);
    }
    if taken(Aspect::Symbol) {
        match &copy.symbol {
            Some(new) => {
                part.symbol = copy.part.symbol;
                symbol = new.clone();
                for (_, u) in &placed.units {
                    if !new.units.iter().any(|n| n.uuid == u.unit) {
                        let name = (old.symbol.units.iter())
                            .find(|n| n.uuid == u.unit)
                            .map(|n| n.name.clone())
                            .unwrap_or_default();
                        report
                            .problems
                            .push(Problem::PlacedUnitRemoved { unit: name });
                    }
                }
            }
            None => report.problems.push(Problem::NoSymbol),
        }
    }
    let mut footprint = None;
    if taken(Aspect::Footprints) {
        let new = &copy.part.footprints;
        let mut list: Vec<Uuid> = (new.iter())
            .filter(|f| settings.add_new_footprints || old.part.footprints.contains(f))
            .copied()
            .collect();
        if !settings.remove_missing_footprints {
            list.extend(old.part.footprints.iter().filter(|f| !new.contains(f)));
        }
        footprints = (list.iter())
            .map(|f| match new.iter().position(|n| n == f) {
                Some(i) => {
                    let c = &copy.footprints[i];
                    (c.hash.clone(), c.footprint.clone())
                }
                None => {
                    let i = (old.part.footprints.iter())
                        .position(|o| o == f)
                        .expect("an old alternate");
                    old_copies[i].clone()
                }
            })
            .collect();
        part.footprints = list;
        part.pin_map = copy.part.pin_map.clone();
        let choice = match (settings.footprint_choice, unit.footprint) {
            (FootprintChoice::KeepIfPresent, Some(f)) if part.footprints.contains(&f) => Some(f),
            _ => part.footprints.first().copied(),
        };
        footprint = Some(choice);
    }

    // The entry hash: the Library's or the old one when the content is one of them,
    // otherwise a mixed one.
    let new_fps = status::copy_footprints(&copy);
    let library = Content {
        part: &copy.part,
        symbol: copy.symbol.as_ref(),
        footprints: &new_fps,
    };
    let mixed_fps: Vec<&Footprint> = footprints.iter().map(|(_, f)| f).collect();
    let mixed = Content {
        part: &part,
        symbol: Some(&symbol),
        footprints: &mixed_fps,
    };
    let old_fps = status::entry_footprints(home, old);
    let before = Content {
        part: &old.part,
        symbol: Some(&old.symbol),
        footprints: &old_fps,
    };
    let hash = if mixed.differs(&library).is_empty() {
        copy.hash.clone()
    } else if mixed.differs(&before).is_empty() {
        old.hash.clone()
    } else {
        mixed.mixed_hash()
    };

    if taken(Aspect::Fields) {
        report.cells = cells::cells(&old.part, &copy.part, unit, settings);
        let starts_as = report
            .cells
            .iter()
            .find(|c| c.cell == cells::COMPONENT_TYPE);
        let component_type = match starts_as {
            Some(c) if c.action == CellAction::Update => copy.part.component_type,
            _ => unit.component_type,
        };
        report
            .cells
            .extend(cells::not_fitted_cell(unit, component_type));
    }

    // A Pin the mix leaves with no Pad on the chosen Footprint.
    let chosen_after = footprint.unwrap_or(unit.footprint);
    report.problems.extend(pins_without_pads(
        (
            &old.symbol,
            &old.part.pin_map,
            fp_of(&old_fps, unit.footprint),
        ),
        (&symbol, &part.pin_map, fp_of(&mixed_fps, chosen_after)),
    ));
    report.blocked = report.problems.iter().any(Problem::blocks);

    let hash_of = |f: Uuid| {
        (footprints.iter())
            .find(|(_, fp)| fp.uuid == f)
            .map(|(h, _)| h.as_str())
    };
    report.lost_local_edits = lost_local_edits(
        project,
        placed.component,
        chosen_after.and_then(|f| Some((f, hash_of(f)?))),
    );
    Ok(Some(Plan {
        report,
        units,
        content: Some(EntryContent {
            hash,
            part,
            symbol,
            footprints,
        }),
        footprint,
        unit_match: UnitMatch::Uuid,
        unit_map: vec![],
    }))
}

/// The error for Project data the model's checks should have kept out.
fn corrupt(message: String) -> CommandError {
    CommandError::internal(format!("the Project is inconsistent: {message}"))
}

/// A Symbol, its pin map and the Footprint chosen, before or after.
pub(crate) type PinsAndPads<'a> = (&'a Symbol, &'a PinMap, Option<&'a Footprint>);

/// The Problem rows for the Pins `after` newly leaves with no Pad on its Footprint.
pub(crate) fn pins_without_pads(before: PinsAndPads<'_>, after: PinsAndPads<'_>) -> Vec<Problem> {
    let Some(fp) = after.2 else {
        return vec![];
    };
    let had = before
        .2
        .map(|fp| diff::pins_without_pads(before.0, before.1, fp))
        .unwrap_or_default();
    diff::pins_without_pads(after.0, after.1, fp)
        .into_iter()
        .filter(|pin| !had.contains(pin))
        .map(|(unit, number)| Problem::PinWithoutPad { unit, number })
        .collect()
}

/// The Footprint `f` among `fps`.
pub(crate) fn fp_of<'a>(fps: &[&'a Footprint], f: Option<Uuid>) -> Option<&'a Footprint> {
    f.and_then(|f| fps.iter().find(|fp| fp.uuid == f).copied())
}

/// The Nets of placed Pins before and after every plan is applied.
struct Nets {
    before: Vec<SheetConnectivity>,
    after: Vec<SheetConnectivity>,
}

fn net_of(sheets: &[SheetConnectivity], unit: Uuid, pin: Uuid) -> Option<String> {
    let net = sheets.iter().find_map(|s| s.pin_net(unit, pin))?;
    let connected = net.pins.len() > 1
        || !net.wires.is_empty()
        || !net.net_labels.is_empty()
        || !net.ports.is_empty()
        || !net.power_ports.is_empty();
    if connected { net.name.clone() } else { None }
}

impl PinNets for Nets {
    fn before(&self, unit: Uuid, pin: Uuid) -> Option<String> {
        net_of(&self.before, unit, pin)
    }

    fn after(&self, unit: Uuid, pin: Uuid) -> Option<String> {
        net_of(&self.after, unit, pin)
    }
}

/// Adds each plan's changes: the Part's name, then the Symbol's (with Nets, worked out
/// on a copy of the Project with every plan applied), then the Footprints'.
pub(crate) fn add_changes(project: &Project, plans: &mut [Plan]) -> Result<(), CommandError> {
    let symbols = plans
        .iter()
        .any(|p| p.report.aspects.contains(&Aspect::Symbol) && p.content.is_some());
    let nets = symbols.then(|| Nets {
        before: project_connectivity(project),
        after: applied_connectivity(project, plans),
    });
    for plan in plans.iter_mut() {
        let Some(content) = &plan.content else {
            continue;
        };
        let (sheet, first) = &plan.units[0];
        let home = project
            .schematic_sheet(*sheet)
            .ok_or_else(|| corrupt(format!("Unit {} is on missing sheet {sheet}", first.uuid)))?;
        let old = home.part(first.part).ok_or_else(|| {
            corrupt(format!(
                "Unit {} names part entry {}, which its sheet does not hold",
                first.uuid, first.part
            ))
        })?;
        let taken = |a: Aspect| plan.report.aspects.contains(&a);
        let mut changes = vec![];
        if taken(Aspect::Fields) && old.part.name != content.part.name {
            changes.push(PartChange::Name {
                from: old.part.name.clone(),
                to: content.part.name.clone(),
            });
        }
        if let Some(nets) = nets.as_ref().filter(|_| taken(Aspect::Symbol)) {
            let placed: Vec<(Uuid, Uuid)> =
                plan.units.iter().map(|(_, u)| (u.uuid, u.unit)).collect();
            diff::symbol_changes(
                &old.symbol,
                &content.symbol,
                plan.unit_match,
                &placed,
                nets,
                &mut changes,
            );
        }
        if taken(Aspect::Footprints) {
            let old_fps = status::entry_footprints(home, old);
            let new_fps: Vec<_> = content.footprints.iter().map(|(_, f)| f).collect();
            let before = Content {
                part: &old.part,
                symbol: Some(&old.symbol),
                footprints: &old_fps,
            };
            let after = Content {
                part: &content.part,
                symbol: Some(&content.symbol),
                footprints: &new_fps,
            };
            let choice = (first.footprint, plan.footprint.unwrap_or(first.footprint));
            diff::footprint_changes(&before, &after, choice, &mut changes);
        }
        plan.report.changes = changes;
    }
    Ok(())
}

/// The connectivity of a copy of `project` with every unblocked plan's Units pointing at
/// their new content. Wires stay where they are.
fn applied_connectivity(project: &Project, plans: &[Plan]) -> Vec<SheetConnectivity> {
    let mut copy = project.clone();
    let tx = &mut copy.transaction();
    let mut next = u128::MAX;
    let settings = LibraryUpdateSettings::default();
    for plan in plans.iter().filter(|p| p.applies() && !p.report.blocked) {
        let content = plan.content();
        for (sheet, unit) in &plan.units {
            let uuid = Uuid::from_u128(next);
            next -= 1;
            let entry = PartEntry {
                uuid,
                hash: content.hash.clone(),
                part: content.part.clone(),
                symbol: content.symbol.clone(),
                footprints: vec![],
            };
            let unit = plan.updated_unit(unit, uuid, &settings);
            let steps = tx
                .insert(Object::PartEntry(InDocument::new(*sheet, entry)))
                .and_then(|()| tx.replace(Object::Unit(InDocument::new(*sheet, unit))));
            debug_assert!(steps.is_ok(), "{steps:?}");
        }
    }
    project_connectivity(tx.project())
}
