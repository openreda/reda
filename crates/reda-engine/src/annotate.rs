//! Re-annotate: renumbering Designators in a processing order.
//!
//! [`plan`] works out old → new for every Component in a [`Selection`] that is not
//! Designator-locked, and [`apply`] writes the plan to every placed Unit, on every sheet.
//! The processing order is a [`ProcessingOrder`]: the schematic orders here sort by where
//! the Units sit on their sheets, and the board position order by where their Footprints
//! sit on the PCB.

use std::collections::{BTreeMap, BTreeSet};

use reda_command::CommandError;
use reda_model::geom::{Nm, Point};
use reda_model::{InDocument, Object, Project, Side, Transaction, UnitInstance, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The Components a Re-annotate renumbers, before locks are taken out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    /// Every Component in the Project.
    Project,
    /// Every Component with a Unit on this Schematic Sheet.
    Sheet(Uuid),
    /// The Components with these UUIDs, or the Components of these Units.
    Objects(Vec<Uuid>),
}

/// Where one Unit of a Component sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The sheet's place in the Project's Document order.
    pub sheet_index: usize,
    pub sheet: Uuid,
    pub at: Point,
}

/// A Component to renumber, with every placed Unit of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub component: Uuid,
    pub designator: String,
    /// The Designator prefix of its Part's Symbol, such as `R`.
    pub prefix: String,
    /// Its Units, in Document order.
    pub units: Vec<Placement>,
}

/// How Re-annotate orders the Components it numbers. Each gets the smallest free number
/// for its prefix, in the order this leaves them in.
pub trait ProcessingOrder {
    fn arrange(&self, project: &Project, candidates: &mut Vec<Candidate>);
}

/// The height of a row (for orders that go across first) and the width of a column (for
/// orders that go down or up first): Units whose origins fall in the same 0.5 in band
/// count as level with each other, so Symbols of different sizes still line up.
pub const BAND: Nm = Nm(12_700_000);

/// The row height of the board position order: Footprints whose origins fall in the same
/// 5 mm band count as level with each other.
pub const BOARD_BAND: Nm = Nm(5_000_000);

/// The processing orders a Re-annotate can take. For the sheet orders, sheets are always
/// numbered in Document order; within a sheet, the order says how to walk it. Sheet y
/// grows upwards, so "down" starts at the top of the sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// Rows from the top, each left to right.
    AcrossThenDown,
    /// Rows from the bottom, each left to right.
    AcrossThenUp,
    /// Columns from the left, each top to bottom.
    DownThenAcross,
    /// Columns from the left, each bottom to top.
    UpThenAcross,
    /// By where the Footprints sit on the board: the top side first, in rows from the top
    /// (board y grows upwards), each left to right; then the bottom side, in rows from the
    /// top, each right to left as seen from below. Components with no Footprint on the
    /// board come last, across then down on their sheets. Needs a PCB Document.
    BoardPosition,
}

impl Order {
    /// The [`ProcessingOrder`] that carries out this order.
    pub fn processing(self) -> Box<dyn ProcessingOrder> {
        match self {
            Order::BoardPosition => Box::new(BoardOrder),
            sheet => Box::new(SheetOrder(sheet)),
        }
    }
}

/// The schematic orders: by sheet, then by band and position on the sheet.
struct SheetOrder(Order);

impl SheetOrder {
    /// The sort key of one Unit; lower comes first.
    fn key(&self, p: &Placement) -> (usize, i64, i64, i64) {
        let band = |v: Nm| v.0.div_euclid(BAND.0);
        let (x, y) = (p.at.x, p.at.y);
        let (a, b, c) = match self.0 {
            // Board position never walks a sheet; [`BoardOrder`] uses across then down.
            Order::AcrossThenDown | Order::BoardPosition => (-band(y), x.0, -y.0),
            Order::AcrossThenUp => (band(y), x.0, y.0),
            Order::DownThenAcross => (band(x), -y.0, x.0),
            Order::UpThenAcross => (band(x), y.0, x.0),
        };
        (p.sheet_index, a, b, c)
    }
}

impl ProcessingOrder for SheetOrder {
    fn arrange(&self, _: &Project, candidates: &mut Vec<Candidate>) {
        // A Component goes where its first Unit in this order goes, so its Units stay
        // together.
        let first = |c: &Candidate| c.units.iter().map(|p| self.key(p)).min();
        candidates.sort_by(|a, b| {
            first(a)
                .cmp(&first(b))
                .then_with(|| a.component.cmp(&b.component))
        });
    }
}

/// The board position order: by where each Component's Footprint instance sits.
struct BoardOrder;

impl ProcessingOrder for BoardOrder {
    fn arrange(&self, project: &Project, candidates: &mut Vec<Candidate>) {
        let band = |v: Nm| v.0.div_euclid(BOARD_BAND.0);
        let at: BTreeMap<Uuid, (Side, Point)> = project
            .pcb()
            .into_iter()
            .flat_map(|pcb| pcb.footprints())
            .filter_map(|f| {
                let c = f.component.as_ref()?;
                c.path.is_empty().then_some((c.uuid, (f.side, f.at)))
            })
            .collect();
        // (side, row from the top, across), with the bottom seen from below.
        let key = |c: &Candidate| {
            at.get(&c.component).map(|&(side, p)| match side {
                Side::Top => (0, -band(p.y), p.x.0, -p.y.0),
                Side::Bottom => (1, -band(p.y), -p.x.0, -p.y.0),
            })
        };
        let (mut on_board, mut off): (Vec<Candidate>, Vec<Candidate>) =
            candidates.drain(..).partition(|c| key(c).is_some());
        on_board.sort_by(|a, b| {
            key(a)
                .cmp(&key(b))
                .then_with(|| a.component.cmp(&b.component))
        });
        SheetOrder(Order::AcrossThenDown).arrange(project, &mut off);
        candidates.extend(on_board);
        candidates.extend(off);
    }
}

/// One Component's Designator before and after.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Rename {
    pub component: Uuid,
    pub old: String,
    pub new: String,
}

/// Works out the new Designator of every unlocked Component in `selection`, in
/// processing order. Locked Components and those outside the selection keep their
/// Designators, and their numbers are not handed out again.
pub fn plan(
    project: &Project,
    selection: &Selection,
    order: &dyn ProcessingOrder,
) -> Result<Vec<Rename>, CommandError> {
    let all = components(project);
    let chosen = chosen(project, &all, selection)?;
    let mut taken: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();
    let mut candidates = Vec::new();
    for (uuid, component) in all {
        if chosen.contains(&uuid) && !component.locked {
            candidates.push(component.candidate);
            continue;
        }
        // Reserve its number under every prefix it could be read with.
        let designator = &component.candidate.designator;
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
    order.arrange(project, &mut candidates);
    let mut renames = Vec::with_capacity(candidates.len());
    for c in candidates {
        let used = taken.entry(c.prefix.clone()).or_default();
        let n = (1..).find(|n| !used.contains(n)).expect("a free number");
        used.insert(n);
        renames.push(Rename {
            component: c.component,
            old: c.designator,
            new: format!("{}{n}", c.prefix),
        });
    }
    Ok(renames)
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

/// A Component as Re-annotate sees it.
struct Component {
    candidate: Candidate,
    locked: bool,
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
            at: unit.at,
        };
        let component = out.entry(unit.component.uuid).or_insert_with(|| {
            let prefix = project
                .schematic_sheet(sheet)
                .and_then(|s| s.part(unit.part))
                .map(|e| e.symbol.designator_prefix.clone())
                .unwrap_or_default();
            Component {
                candidate: Candidate {
                    component: unit.component.uuid,
                    designator: unit.designator.clone(),
                    prefix,
                    units: Vec::new(),
                },
                locked: false,
            }
        });
        component.locked |= unit.designator_locked;
        component.candidate.units.push(placement);
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
        Selection::Sheet(sheet) => {
            let sheet = project.schematic_sheet(*sheet).ok_or_else(|| {
                CommandError::not_found(format!("no Schematic Sheet has UUID {sheet}"))
            })?;
            Ok(sheet.units().map(|u| u.component.uuid).collect())
        }
        Selection::Objects(objects) => objects
            .iter()
            .map(|uuid| {
                if all.contains_key(uuid) {
                    return Ok(*uuid);
                }
                placed_units(project)
                    .find(|(_, u)| u.uuid == *uuid)
                    .map(|(_, u)| u.component.uuid)
                    .ok_or_else(|| {
                        CommandError::not_found(format!("no Component or Unit has UUID {uuid}"))
                    })
            })
            .collect(),
    }
}
