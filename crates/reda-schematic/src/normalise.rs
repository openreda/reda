//! Wire normalisation: what every edit Command that creates or changes Wires or Units does
//! to the Wires involved, in the same transaction and undo step.
//!
//! [`normalise`] compares a sheet as the Command found it with the sheet as the Command
//! left it. The Wires that changed, and those touching a changed Wire or a moved Pin, are
//! involved; nothing else on the sheet is touched, so a hand-edited or merged file that
//! isn't normalised stays as it is until it is edited there. Loading never normalises.
//!
//! The steps, each switched by [`Wiring`]:
//! 1. Components Cut Wires (needs Optimize Wires): Pins of a placed or moved Unit landing
//!    on one straight stretch of a Wire cut out the pieces between them, unless something
//!    else connects there.
//! 2. Optimize Wires: collinear vertices are removed, and overlapping collinear Wires
//!    merge, as do collinear Wires meeting end to end with nothing else at that point.
//!    The lowest UUID survives.
//! 3. Convert Cross-Junctions: a Wire end making a 4-way is moved one snap grid along a
//!    crossing Wire, with a mitred corner, leaving two adjacent 3-ways.
//! 4. Break Wires At Autojunctions: a Wire splits at each Junction on its interior. The
//!    piece from its first vertex keeps the UUID.
//!
//! Zero-length segments are always dropped.

mod steps;

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{CommandError, Context, Length};
use reda_model::geom::{Nm, Point};
use reda_model::{InDocument, Object, SchematicSheet, UnitInstance, Uuid, Wire};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::connectivity::placed_pins;
pub(crate) use steps::Work;

/// How edits normalise the Wires they touch: the four wiring Preferences, and the snap
/// grid that converting a cross-junction offsets by. A Command given no `wiring` uses
/// these defaults, not the user's Preferences, so its result is the same on every
/// machine; the app passes the Preferences. A field left out takes its default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Wiring {
    /// Optimize Wires (on): overlapping collinear Wires merge, and so do collinear Wires
    /// meeting end to end with nothing else at that point; collinear vertices inside a
    /// Wire are removed. Off, Wires stay as drawn.
    pub optimize_wires: bool,
    /// Components Cut Wires (on; needs Optimize Wires): when two or more Pins of a
    /// placed, moved or pasted Unit land on one straight stretch of a Wire, the pieces
    /// between them are removed unless something else connects there. Off, the Pins
    /// connect and the part may be shorted.
    pub components_cut_wires: bool,
    /// Break Wires At Autojunctions (on): a Wire splits at each Junction on its interior.
    /// Off, it stays one Wire through the Junction.
    pub break_wires_at_autojunctions: bool,
    /// Convert Cross-Junctions (on): a Wire that would make a 4-way is offset into two
    /// adjacent 3-ways, one snap grid apart. Off, the 4-way stays as drawn.
    pub convert_cross_junctions: bool,
    /// The snap grid a converted cross-junction is offset by; 50 mil by default.
    pub snap_grid: Length,
}

/// The sheets' snap grid until one is picked: 50 mil.
pub const DEFAULT_SNAP_GRID: Nm = Nm::from_mil(50);

impl Default for Wiring {
    fn default() -> Wiring {
        Wiring {
            optimize_wires: true,
            components_cut_wires: true,
            break_wires_at_autojunctions: true,
            convert_cross_junctions: true,
            snap_grid: Length(DEFAULT_SNAP_GRID),
        }
    }
}

/// The Commands that take a `wiring` parameter, for front ends that fill it in from the
/// Preferences.
pub const WIRING_COMMANDS: &[&str] = &[
    "schematic.wire.add",
    "schematic.wire.break",
    "schematic.wire.set_vertices",
    "schematic.junctions.convert_cross",
    "schematic.component.place",
    "schematic.unit.place",
    "schematic.unit.move",
    "schematic.unit.cycle",
    "schematic.delete",
    "schematic.cut",
    "schematic.paste",
    "schematic.duplicate",
    "schematic.move",
    "schematic.selection.move_to_sheet",
    "schematic.drag",
    "schematic.rotate",
    "schematic.mirror",
    "schematic.align",
    "schematic.distribute",
    "schematic.snap_to_grid",
];

/// Where the Wires of a normalised sheet came from: each Wire after normalising, with
/// the Wires before it whose drawing it holds.
#[derive(Debug, Default)]
pub(crate) struct Lineage {
    /// Every Wire on the sheet before normalising.
    known: BTreeSet<Uuid>,
    /// Each Wire after, with the Wires it came from.
    origins: BTreeMap<Uuid, BTreeSet<Uuid>>,
}

impl Lineage {
    /// What became of object `uuid`: the Wires holding its drawing now, sorted, when it
    /// was a Wire; itself otherwise.
    pub fn trace(&self, uuid: Uuid) -> Vec<Uuid> {
        if !self.known.contains(&uuid) {
            return vec![uuid];
        }
        self.origins
            .iter()
            .filter(|(_, from)| from.contains(&uuid))
            .map(|(&wire, _)| wire)
            .collect()
    }

    /// [`Lineage::trace`] of each of `uuids`, in order, each result once.
    pub fn trace_all(&self, uuids: &[Uuid]) -> Vec<Uuid> {
        let mut seen = BTreeSet::new();
        uuids
            .iter()
            .flat_map(|&u| self.trace(u))
            .filter(|u| seen.insert(*u))
            .collect()
    }
}

/// Normalises the Wires an edit involved on sheet `document`, comparing it with `before`,
/// the sheet as the Command found it, and writes the result into the Command's
/// transaction.
pub(crate) fn normalise(
    cx: &mut Context<'_>,
    document: Uuid,
    before: &SchematicSheet,
    wiring: &Wiring,
) -> Result<Lineage, CommandError> {
    let after = crate::sheet(cx.project()?, document)?.clone();
    let seeds = Seeds::between(before, &after);
    let work = run(&after, &seeds, wiring, &mut || cx.new_id());
    apply(cx, document, &after, &work)?;
    Ok(work.lineage())
}

/// Runs every step `wiring` switches on over `after`, starting from `seeds`.
pub(crate) fn run(
    after: &SchematicSheet,
    seeds: &Seeds,
    wiring: &Wiring,
    new_id: &mut dyn FnMut() -> Uuid,
) -> Work {
    let mut work = Work::new(
        after,
        seeds.wires.clone(),
        seeds.changed.clone(),
        seeds.points.clone(),
    );
    work.drop_zero_length();
    let optimize = wiring.optimize_wires;
    if optimize && wiring.components_cut_wires {
        for pins in &seeds.moved_pins {
            work.split_wires_at_pins(pins, new_id);
        }
    }
    if optimize {
        work.optimize();
    }
    if wiring.convert_cross_junctions {
        let points = work.involved_ends();
        work.convert_cross(&points, wiring.snap_grid.nm());
        if optimize {
            work.optimize();
        }
    }
    if wiring.break_wires_at_autojunctions {
        work.break_at_junctions(new_id);
    }
    work
}

/// Writes the Wires of `work` over those of `after` in the Command's transaction.
pub(crate) fn apply(
    cx: &mut Context<'_>,
    document: Uuid,
    after: &SchematicSheet,
    work: &Work,
) -> Result<(), CommandError> {
    let tx = cx.tx()?;
    for wire in after.wires() {
        if !work.wires.contains_key(&wire.uuid) {
            tx.remove(wire.uuid)?;
        }
    }
    for (&uuid, vertices) in &work.wires {
        let wire = Wire {
            uuid,
            vertices: vertices.clone(),
        };
        match after.wire(uuid) {
            Some(old) if *old == wire => {}
            Some(_) => tx.replace(Object::Wire(InDocument::new(document, wire)))?,
            None => tx.insert(Object::Wire(InDocument::new(document, wire)))?,
        }
    }
    Ok(())
}

/// What an edit changed on a sheet, as normalising starts from it.
#[derive(Debug, Default)]
pub(crate) struct Seeds {
    /// The Wires involved: those created or changed, and those touching one of them, a
    /// removed Wire or a moved Pin.
    pub wires: BTreeSet<Uuid>,
    /// The Wires created or changed.
    pub changed: BTreeSet<Uuid>,
    /// The Pin points of Units placed, moved or reshaped, before and after.
    pub points: BTreeSet<Point>,
    /// The Pins of each Unit placed or moved, which may cut Wires.
    pub moved_pins: Vec<Vec<Point>>,
}

impl Seeds {
    /// The seeds of the edit that turned `before` into `after`.
    pub fn between(before: &SchematicSheet, after: &SchematicSheet) -> Seeds {
        let changed: Vec<&Wire> = after
            .wires()
            .filter(|w| before.wire(w.uuid) != Some(*w))
            .collect();
        let gone: Vec<&Wire> = before
            .wires()
            .filter(|w| after.wire(w.uuid) != Some(*w))
            .collect();
        let pins_before = pins_by_unit(before);
        let pins_after = pins_by_unit(after);
        let mut points = BTreeSet::new();
        for (unit, pins) in &pins_before {
            if pins_after.get(unit) != Some(pins) {
                points.extend(pins.iter().copied());
            }
        }
        for (unit, pins) in &pins_after {
            if pins_before.get(unit) != Some(pins) {
                points.extend(pins.iter().copied());
            }
        }
        let moved_pins = after
            .units()
            .filter(|u| before.unit(u.uuid).is_none_or(|b| placed_differently(b, u)))
            .filter_map(|u| pins_after.get(&u.uuid).cloned())
            .collect();
        let changed_ids: BTreeSet<Uuid> = changed.iter().map(|w| w.uuid).collect();
        let mut wires = changed_ids.clone();
        let touched: Vec<&Wire> = changed.iter().chain(&gone).copied().collect();
        for wire in after.wires() {
            if touched.iter().any(|t| touches(wire, t)) || points.iter().any(|&p| wire.contains(p))
            {
                wires.insert(wire.uuid);
            }
        }
        Seeds {
            wires,
            changed: changed_ids,
            points,
            moved_pins,
        }
    }

    /// Seeds holding every Wire of `sheet`.
    pub fn all(sheet: &SchematicSheet) -> Seeds {
        Seeds {
            wires: sheet.wires().map(|w| w.uuid).collect(),
            ..Seeds::default()
        }
    }
}

/// Whether `a` and `b` touch: an end of one lies on the other.
fn touches(a: &Wire, b: &Wire) -> bool {
    a.ends().iter().any(|&e| b.contains(e)) || b.ends().iter().any(|&e| a.contains(e))
}

/// Whether Unit `b` sits elsewhere than `a`: moved, turned, flipped or another Unit.
fn placed_differently(a: &UnitInstance, b: &UnitInstance) -> bool {
    a.at != b.at || a.rotation != b.rotation || a.mirror != b.mirror || a.unit != b.unit
}

/// Each placed Unit's Pin points, in Pin order.
fn pins_by_unit(sheet: &SchematicSheet) -> BTreeMap<Uuid, Vec<Point>> {
    let mut out: BTreeMap<Uuid, Vec<Point>> = BTreeMap::new();
    for (unit, _, at) in placed_pins(sheet) {
        out.entry(unit.uuid).or_default().push(at);
    }
    out
}
