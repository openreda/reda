//! Re-annotate: renumbering Designators in a processing order.
//!
//! [`plan`] works out old → new for every Component in a [`Selection`] that is not
//! Designator-locked, under some [`ReannotateSettings`], and [`apply`] writes the plan to
//! every placed Unit, on every sheet. The schematic orders sort by where each Unit's body
//! (or Designator) sits on its sheet, the board orders by the leading edge of each
//! Footprint's Courtyard (or Pads) on the board.
//!
//! Rows and columns are not fixed bands: a part joins the current row (or column) when it
//! is within the threshold of the row's first part, so two nearly level parts are never
//! split by a band edge.

use std::collections::{BTreeMap, BTreeSet};

use reda_command::CommandError;
use reda_model::annotate::{Location, Mode, ReannotateSettings, Sides, Source, Walk};
use reda_model::geom::{Bounds, Nm, Point};
use reda_model::{InDocument, Object, Project, Side, Transaction, UnitInstance, Uuid};
use reda_render::{ColorKey, SchematicColor};
use schemars::JsonSchema;
use serde::Serialize;

/// The Components a Re-annotate renumbers, before locks are taken out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    /// Every Component in the Project.
    Project,
    /// Every Component with a Unit on one of these Schematic Sheets.
    Sheets(Vec<Uuid>),
    /// The Components with these UUIDs, or the Components of these Units or Footprint
    /// instances.
    Objects(Vec<Uuid>),
}

/// One Component's Designator before and after.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Rename {
    pub component: Uuid,
    pub old: String,
    pub new: String,
}

/// A Designator that more than one Component still holds after the plan, because each of
/// them is locked or outside the scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Duplicate {
    pub designator: String,
    /// The Components holding it, by UUID.
    pub components: Vec<Uuid>,
}

/// What a Re-annotate would do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Plan {
    /// Old → new for every Component it numbers, in processing order; a Component whose
    /// number comes out the same is listed with `old` = `new`. Locked Components, those
    /// outside the scope and those on a board side not numbered are left out.
    pub designators: Vec<Rename>,
    /// The Designators still shared after it by locked or out-of-scope Components.
    pub kept_duplicates: Vec<Duplicate>,
}

/// Works out the new Designator of every unlocked Component in `selection`, in the
/// processing order `settings` give. Locked Components and those outside the selection
/// keep their Designators, and their numbers are not handed out again.
pub fn plan(
    project: &Project,
    settings: &ReannotateSettings,
    selection: &Selection,
) -> Result<Plan, CommandError> {
    let all = components(project);
    let chosen = chosen(project, &all, selection)?;
    let mut candidates: Vec<&Component> = all
        .values()
        .filter(|c| chosen.contains(&c.uuid) && !c.locked)
        .collect();
    match settings.source {
        Source::Schematic => sheet_order(project, settings, &mut candidates),
        Source::Board => board_order(project, settings, &mut candidates),
    }
    let numbered: BTreeSet<Uuid> = candidates.iter().map(|c| c.uuid).collect();

    // Numbers held by Components that are not renumbered, under the prefix before them.
    let mut taken: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();
    let mut held: BTreeSet<&str> = BTreeSet::new();
    for c in all.values().filter(|c| !numbered.contains(&c.uuid)) {
        reserve(&mut taken, &c.designator);
        held.insert(&c.designator);
    }
    // Fixing duplicates only: the first holder of a Designator nobody else keeps keeps it.
    let mut keeps = BTreeSet::new();
    if settings.mode == Mode::FixDuplicatesOnly {
        for c in &candidates {
            if held.insert(&c.designator) {
                reserve(&mut taken, &c.designator);
                keeps.insert(c.uuid);
            }
        }
    }
    let mut designators = Vec::with_capacity(candidates.len());
    for c in candidates {
        let new = if keeps.contains(&c.uuid) {
            c.designator.clone()
        } else {
            let used = taken.entry(c.prefix.clone()).or_default();
            let start = settings.start_for(c.first_sheet());
            let n = (start..)
                .find(|n| !used.contains(n))
                .expect("a free number");
            used.insert(n);
            format!("{}{n}", c.prefix)
        };
        designators.push(Rename {
            component: c.uuid,
            old: c.designator.clone(),
            new,
        });
    }

    let new: BTreeMap<Uuid, &str> = designators
        .iter()
        .map(|r| (r.component, r.new.as_str()))
        .collect();
    let mut holders: BTreeMap<&str, Vec<Uuid>> = BTreeMap::new();
    for c in all.values() {
        let designator = new.get(&c.uuid).copied().unwrap_or(&c.designator);
        holders.entry(designator).or_default().push(c.uuid);
    }
    let kept_duplicates = holders
        .into_iter()
        .filter(|(_, components)| components.len() > 1)
        .map(|(designator, components)| Duplicate {
            designator: designator.to_owned(),
            components,
        })
        .collect();
    Ok(Plan {
        designators,
        kept_duplicates,
    })
}

/// Reserves a Designator's trailing number under the prefix before it.
fn reserve(taken: &mut BTreeMap<String, BTreeSet<u64>>, designator: &str) {
    let digits = designator.len()
        - designator
            .bytes()
            .rev()
            .take_while(u8::is_ascii_digit)
            .count();
    if let Ok(n) = designator[digits..].parse() {
        taken
            .entry(designator[..digits].to_owned())
            .or_default()
            .insert(n);
    }
}

/// Writes each new Designator to every placed Unit of its Component that does not
/// already carry it.
pub fn apply(tx: &mut Transaction<'_>, renames: &[Rename]) -> Result<(), CommandError> {
    let new: BTreeMap<Uuid, &str> = renames
        .iter()
        .map(|r| (r.component, r.new.as_str()))
        .collect();
    let mut changed = Vec::new();
    for (sheet, unit) in placed_units(tx.project()) {
        if let Some(designator) = new.get(&unit.component.uuid)
            && unit.designator != *designator
        {
            let mut unit = unit.clone();
            unit.designator = (*designator).to_owned();
            changed.push(Object::Unit(InDocument::new(sheet, unit)));
        }
    }
    for object in changed {
        tx.replace(object)?;
    }
    Ok(())
}

/// One placed Unit of a Component.
#[derive(Clone, Copy, Debug)]
struct Placement {
    /// The sheet's place in the Project's Document order.
    sheet_index: usize,
    sheet: Uuid,
    unit: Uuid,
}

/// A Component as Re-annotate sees it, with every placed Unit of it.
#[derive(Debug)]
struct Component {
    uuid: Uuid,
    designator: String,
    /// The Designator prefix of its Part's Symbol, such as `R`.
    prefix: String,
    locked: bool,
    /// Its Units, in Document order.
    units: Vec<Placement>,
}

impl Component {
    /// The earliest sheet it has a Unit on.
    fn first_sheet(&self) -> Option<Uuid> {
        self.units.first().map(|p| p.sheet)
    }
}

/// Where a position comes in a walk: its group (sheet or board side, walked one after
/// another), then the coordinates across rows (or columns) and along them, each signed
/// so that lower comes first.
#[derive(Clone, Copy, Debug)]
struct Key {
    group: usize,
    across: i64,
    along: i64,
    tie: Uuid,
}

/// Sorts `items` by their keys: by group, then in rows (or columns) that each start at
/// their first item and take every later one within `threshold` of it, then along each
/// row. Ties go by UUID.
fn walk_sort<T: Copy>(items: &mut Vec<(T, Key)>, threshold: Nm) {
    items.sort_by_key(|(_, k)| (k.group, k.across, k.along, k.tie));
    let mut first: Option<(usize, i64)> = None;
    let mut row = 0usize;
    let mut rows: Vec<(usize, (T, Key))> = Vec::with_capacity(items.len());
    for &(item, k) in items.iter() {
        let joins = first
            .is_some_and(|(group, across)| group == k.group && k.across - across <= threshold.0);
        if !joins {
            row += 1;
            first = Some((k.group, k.across));
        }
        rows.push((row, (item, k)));
    }
    rows.sort_by_key(|(row, (_, k))| (*row, k.along, k.tie));
    *items = rows.into_iter().map(|(_, e)| e).collect();
}

/// The key of a box in `walk`: its leading edge on each axis, X mirrored when `mirror`.
fn box_key(group: usize, b: Bounds, walk: Walk, mirror: bool, tie: Uuid) -> Key {
    let (x_lo, x_hi) = match mirror {
        true => (-b.max.x.0, -b.min.x.0),
        false => (b.min.x.0, b.max.x.0),
    };
    let (y_lo, y_hi) = (b.min.y.0, b.max.y.0);
    let lead = |lo: i64, hi: i64, ascending: bool| if ascending { lo } else { -hi };
    let (across, along) = match walk.rows {
        true => (lead(y_lo, y_hi, walk.across), lead(x_lo, x_hi, walk.along)),
        false => (lead(x_lo, x_hi, walk.across), lead(y_lo, y_hi, walk.along)),
    };
    Key {
        group,
        across,
        along,
        tie,
    }
}

/// Orders `candidates` by where their Units sit on the sheets, sheets in Document order.
/// A Component goes where its first Unit on its earliest sheet goes.
fn sheet_order(project: &Project, settings: &ReannotateSettings, candidates: &mut Vec<&Component>) {
    let walk = settings.order.walk();
    let points = unit_points(project, candidates, settings.location);
    // Every Unit on its Component's earliest sheet, by its Component's index.
    let mut units: Vec<(usize, Key)> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let Some(first) = c.units.first() else {
            continue;
        };
        for p in c.units.iter().filter(|p| p.sheet == first.sheet) {
            let at = points.get(&p.unit).copied().unwrap_or_default();
            let key = box_key(p.sheet_index, Bounds::of_point(at), walk, false, c.uuid);
            units.push((i, key));
        }
    }
    walk_sort(&mut units, settings.threshold);
    let mut seen = BTreeSet::new();
    let order: Vec<&Component> = units
        .into_iter()
        .filter(|(i, _)| seen.insert(*i))
        .map(|(i, _)| candidates[i])
        .collect();
    *candidates = order;
}

/// Orders `candidates` by where their Footprints sit on the board: the top side, then the
/// bottom (mirrored in X, as seen from below), then those with no Footprint on the board,
/// across then down on their sheets. Components the sides leave out are taken out of
/// `candidates`: with one side only, those with no Footprint on the board too.
fn board_order(project: &Project, settings: &ReannotateSettings, candidates: &mut Vec<&Component>) {
    let walk = settings.order.walk();
    let boxes: BTreeMap<Uuid, (Side, Bounds)> = project
        .pcb()
        .into_iter()
        .flat_map(|pcb| {
            pcb.footprints().filter_map(move |f| {
                let c = f.component.as_ref()?;
                c.path
                    .is_empty()
                    .then(|| (c.uuid, (f.side, reda_pcb::placed::courtyard_bounds(pcb, f))))
            })
        })
        .collect();
    let mut on_board: Vec<(&Component, Key)> = Vec::new();
    let mut off: Vec<&Component> = Vec::new();
    for c in candidates.drain(..) {
        match boxes.get(&c.uuid) {
            Some(&(side, b)) => {
                let wanted = match settings.sides {
                    Sides::TopThenBottom => true,
                    Sides::Top => side == Side::Top,
                    Sides::Bottom => side == Side::Bottom,
                };
                if wanted {
                    let bottom = side == Side::Bottom;
                    on_board.push((c, box_key(usize::from(bottom), b, walk, bottom, c.uuid)));
                }
            }
            None if settings.sides == Sides::TopThenBottom => off.push(c),
            None => {}
        }
    }
    walk_sort(&mut on_board, settings.threshold);
    let mut rest = ReannotateSettings::new(Source::Schematic);
    rest.threshold = settings.threshold;
    sheet_order(project, &rest, &mut off);
    candidates.extend(on_board.into_iter().map(|(c, _)| c));
    candidates.extend(off);
}

/// The point each Unit of `candidates` is sorted by: the centre of its body (its graphics,
/// or its Pins when it has none, leaving out their names and numbers), or of its Designator text when `location` says so and
/// the Designator is shown.
fn unit_points(
    project: &Project,
    candidates: &[&Component],
    location: Location,
) -> BTreeMap<Uuid, Point> {
    let units: BTreeSet<Uuid> = candidates
        .iter()
        .flat_map(|c| c.units.iter().map(|p| p.unit))
        .collect();
    let sheets: BTreeSet<Uuid> = candidates
        .iter()
        .flat_map(|c| c.units.iter().map(|p| p.sheet))
        .collect();
    let body = |c: &ColorKey| {
        matches!(
            c,
            ColorKey::Schematic(SchematicColor::Body | SchematicColor::BodyFill)
        )
    };
    let mut out = BTreeMap::new();
    for sheet in sheets {
        let Some(scene) = reda_render::schematic_scene(project, sheet, &[]) else {
            continue;
        };
        // Per Unit: its body graphics, the rest of it (its Pins), and its Designator.
        let mut boxes: BTreeMap<Uuid, [Option<Bounds>; 3]> = BTreeMap::new();
        for item in scene.items.iter().filter(|i| units.contains(&i.object)) {
            let Some((min, max)) = item.primitive.bounds() else {
                continue;
            };
            let text = matches!(item.primitive, reda_render::Primitive::Glyphs(_));
            let slot = match &item.child {
                // Pin names and numbers are not the body.
                None if text => continue,
                None if body(&item.color) => 0,
                None => 1,
                Some(reda_render::Child::Field { field, .. })
                    if field == UnitInstance::DESIGNATOR =>
                {
                    2
                }
                Some(_) => continue,
            };
            let b = Bounds { min, max };
            let entry = &mut boxes.entry(item.object).or_default()[slot];
            *entry = Some(entry.map_or(b, |e| e.union(b)));
        }
        for (unit, [graphics, pins, designator]) in boxes {
            let text = designator.filter(|_| location == Location::Designator);
            if let Some(b) = text.or(graphics).or(pins) {
                out.insert(unit, center(b));
            }
        }
    }
    // A Unit that draws nothing sorts by its origin.
    for (_, unit) in placed_units(project) {
        if units.contains(&unit.uuid) {
            out.entry(unit.uuid).or_insert(unit.at);
        }
    }
    out
}

fn center(b: Bounds) -> Point {
    Point::new(
        Nm(b.min.x.0 + (b.max.x.0 - b.min.x.0) / 2),
        Nm(b.min.y.0 + (b.max.y.0 - b.min.y.0) / 2),
    )
}

/// Every placed Unit with its sheet, in Document order. Units inside Sheet Instances
/// (none in v0.1) are left out.
fn placed_units(project: &Project) -> impl Iterator<Item = (Uuid, &UnitInstance)> {
    project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.units().map(move |u| (s.uuid, u)))
        .filter(|(_, u)| u.component.path.is_empty())
}

/// Every Component in the Project, by UUID.
fn components(project: &Project) -> BTreeMap<Uuid, Component> {
    let sheet_index: BTreeMap<Uuid, usize> = project
        .documents()
        .enumerate()
        .map(|(i, d)| (d.uuid, i))
        .collect();
    let mut out: BTreeMap<Uuid, Component> = BTreeMap::new();
    for (sheet, unit) in placed_units(project) {
        let placement = Placement {
            sheet_index: sheet_index[&sheet],
            sheet,
            unit: unit.uuid,
        };
        let component = out.entry(unit.component.uuid).or_insert_with(|| Component {
            uuid: unit.component.uuid,
            designator: unit.designator.clone(),
            prefix: project
                .schematic_sheet(sheet)
                .and_then(|s| s.part(unit.part))
                .map(|e| e.symbol.designator_prefix.clone())
                .unwrap_or_default(),
            locked: false,
            units: Vec::new(),
        });
        component.locked |= unit.designator_locked;
        component.units.push(placement);
    }
    out
}

/// The UUIDs of the Components `selection` names.
fn chosen(
    project: &Project,
    all: &BTreeMap<Uuid, Component>,
    selection: &Selection,
) -> Result<BTreeSet<Uuid>, CommandError> {
    match selection {
        Selection::Project => Ok(all.keys().copied().collect()),
        Selection::Sheets(sheets) => {
            let mut out = BTreeSet::new();
            for uuid in sheets {
                let sheet = project.schematic_sheet(*uuid).ok_or_else(|| {
                    CommandError::not_found(format!("no Schematic Sheet has UUID {uuid}"))
                })?;
                out.extend(sheet.units().map(|u| u.component.uuid));
            }
            Ok(out)
        }
        Selection::Objects(objects) => objects
            .iter()
            .map(|uuid| {
                if all.contains_key(uuid) {
                    return Ok(*uuid);
                }
                let of_unit = placed_units(project)
                    .find(|(_, u)| u.uuid == *uuid)
                    .map(|(_, u)| u.component.uuid);
                let of_footprint = || {
                    let instance = project.pcb()?.footprint(*uuid)?;
                    let c = instance.component.as_ref()?;
                    c.path.is_empty().then_some(c.uuid)
                };
                of_unit.or_else(of_footprint).ok_or_else(|| {
                    CommandError::not_found(format!(
                        "no Component, Unit or Footprint has UUID {uuid}"
                    ))
                })
            })
            .collect(),
    }
}
