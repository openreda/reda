//! What an Update from Library changes on one Component, aspect by aspect: the Part's
//! name, the Symbol with its Units and Pins (with the Nets each Pin leaves or joins), and
//! the alternate Footprints with their Pads, pin map and the Footprint choice.

use std::collections::BTreeSet;

use reda_model::{Footprint, Pad, PinMap, PinType, Symbol, Unit, Uuid};
use schemars::JsonSchema;
use serde::Serialize;

use super::status::{Aspect, Content};

/// A value before and after.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FromTo<T> {
    pub from: T,
    pub to: T,
}

/// One difference an update makes, under the aspect that brings it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PartChange {
    /// Fields: the Part was renamed.
    Name { from: String, to: String },
    /// Symbol: the Symbol changed; `name` is its name after, and `graphics` whether its
    /// drawing changed.
    Symbol {
        name: String,
        #[serde(default, skip_serializing_if = "is_false")]
        graphics: bool,
    },
    /// Symbol: a Unit new in the Symbol. It appears as unplaced.
    UnitAdded { unit: String },
    /// Symbol: a Unit gone from the Symbol. A placed one blocks the Component until the
    /// user deletes that Unit.
    UnitRemoved {
        unit: String,
        #[serde(default, skip_serializing_if = "is_false")]
        placed: bool,
    },
    /// Symbol: a Pin added, removed, renamed, retyped or moved, matched by number within
    /// its Unit. On a placed Unit, `leaves` and `joins` name the Nets the Pin leaves and
    /// joins: Wires never move, so a moved Pin may leave its Net.
    Pin {
        unit: String,
        number: String,
        #[serde(default, skip_serializing_if = "is_false")]
        added: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        removed: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<FromTo<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        electrical_type: Option<FromTo<PinType>>,
        #[serde(default, skip_serializing_if = "is_false")]
        moved: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        leaves: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        joins: Option<String>,
    },
    /// Footprints: an alternate added, removed or changed. On a changed one, the Pad
    /// numbers added or removed and the Pads (by number) moved or resized. The PCB gets
    /// the change through Sync, not through the update.
    Footprint {
        footprint: Uuid,
        name: String,
        change: FootprintChange,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pads_added: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pads_removed: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pads_moved: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pads_resized: Vec<String>,
    },
    /// Footprints: the alternates kept are in a new order; the first is the default.
    FootprintOrder,
    /// Footprints: the pin-pad map overrides, old and new.
    PinMap { from: PinMap, to: PinMap },
    /// Footprints: the Component's Footprint choice changes (`null` = none).
    FootprintChoice {
        from: Option<Uuid>,
        to: Option<Uuid>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FootprintChange {
    Added,
    Removed,
    Modified,
}

fn is_false(b: &bool) -> bool {
    !b
}

impl PartChange {
    /// The aspect that brings this change.
    pub fn aspect(&self) -> Aspect {
        match self {
            PartChange::Name { .. } => Aspect::Fields,
            PartChange::Symbol { .. }
            | PartChange::UnitAdded { .. }
            | PartChange::UnitRemoved { .. }
            | PartChange::Pin { .. } => Aspect::Symbol,
            _ => Aspect::Footprints,
        }
    }

    /// Whether this change stops the update: a placed Unit gone from the Symbol.
    pub fn blocks(&self) -> bool {
        matches!(self, PartChange::UnitRemoved { placed: true, .. })
    }
}

/// Where a placed Pin's Net is, before and after the update: `None` for a Pin connected
/// to nothing.
pub(crate) trait PinNets {
    /// The Net of Pin `pin` (by UUID) of placed Unit `unit` before the update.
    fn before(&self, unit: Uuid, pin: Uuid) -> Option<String>;
    /// The same after it.
    fn after(&self, unit: Uuid, pin: Uuid) -> Option<String>;
}

/// How the Units of a Symbol before and after an update correspond.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnitMatch {
    /// By UUID: Update from Library.
    Uuid,
    /// By position in the Symbol's list: Replace Part.
    Index,
}

impl UnitMatch {
    /// The Unit of `new` that Unit `unit` of `old` becomes, if any.
    pub fn of<'a>(self, old: &Symbol, new: &'a Symbol, unit: Uuid) -> Option<&'a Unit> {
        match self {
            UnitMatch::Uuid => new.units.iter().find(|u| u.uuid == unit),
            UnitMatch::Index => {
                let i = old.units.iter().position(|u| u.uuid == unit)?;
                new.units.get(i)
            }
        }
    }

    /// The Unit of `old` that Unit `unit` of `new` comes from, if any.
    fn from<'a>(self, old: &'a Symbol, new: &Symbol, unit: Uuid) -> Option<&'a Unit> {
        self.of(new, old, unit)
    }
}

/// The Symbol changes from `old` to `new` for a Component whose placed Units are
/// `placed` (Unit instance UUID, Symbol Unit UUID in `old`), Units matched by `matching`.
pub(crate) fn symbol_changes(
    old: &Symbol,
    new: &Symbol,
    matching: UnitMatch,
    placed: &[(Uuid, Uuid)],
    nets: &dyn PinNets,
    out: &mut Vec<PartChange>,
) {
    if old == new {
        return;
    }
    out.push(PartChange::Symbol {
        name: new.name.clone(),
        graphics: old.graphics != new.graphics,
    });
    let instance = |unit: Uuid| placed.iter().find(|(_, u)| *u == unit).map(|(i, _)| *i);
    for unit in &old.units {
        if matching.of(old, new, unit.uuid).is_none() {
            out.push(PartChange::UnitRemoved {
                unit: unit.name.clone(),
                placed: instance(unit.uuid).is_some(),
            });
        }
    }
    for unit in &new.units {
        if matching.from(old, new, unit.uuid).is_none() {
            out.push(PartChange::UnitAdded {
                unit: unit.name.clone(),
            });
        }
    }
    for unit in &new.units {
        let Some(before) = matching.from(old, new, unit.uuid) else {
            continue;
        };
        let at = instance(before.uuid);
        let olds: Vec<_> = old.unit_pins(before.uuid).collect();
        let news: Vec<_> = new.unit_pins(unit.uuid).collect();
        let mut numbers: Vec<&str> = olds.iter().map(|p| p.number.as_str()).collect();
        for p in &news {
            if !numbers.contains(&p.number.as_str()) {
                numbers.push(&p.number);
            }
        }
        for number in numbers {
            let o = olds.iter().find(|p| p.number == number);
            let n = news.iter().find(|p| p.number == number);
            let before = at.zip(o).and_then(|(i, p)| nets.before(i, p.uuid));
            let after = at.zip(n).and_then(|(i, p)| nets.after(i, p.uuid));
            let name = (o.zip(n))
                .filter(|(o, n)| o.name != n.name)
                .map(|(o, n)| FromTo {
                    from: o.name.clone(),
                    to: n.name.clone(),
                });
            let electrical_type = (o.zip(n))
                .filter(|(o, n)| o.electrical_type != n.electrical_type)
                .map(|(o, n)| FromTo {
                    from: o.electrical_type,
                    to: n.electrical_type,
                });
            let moved = o.zip(n).is_some_and(|(o, n)| o.at != n.at);
            let (added, removed) = (o.is_none(), n.is_none());
            let changed = added || removed || name.is_some() || electrical_type.is_some() || moved;
            let net_changed = before != after;
            if !changed && !net_changed {
                continue;
            }
            let (leaves, joins) = if net_changed {
                (before, after)
            } else {
                (None, None)
            };
            out.push(PartChange::Pin {
                unit: unit.name.clone(),
                number: number.to_owned(),
                added,
                removed,
                name,
                electrical_type,
                moved,
                leaves,
                joins,
            });
        }
    }
}

/// The Footprints changes from `old` to `new`, and the Footprint choice change.
pub(crate) fn footprint_changes(
    old: &Content<'_>,
    new: &Content<'_>,
    (from, to): (Option<Uuid>, Option<Uuid>),
    out: &mut Vec<PartChange>,
) {
    let footprint = |fp: &Footprint, change: FootprintChange| PartChange::Footprint {
        footprint: fp.uuid,
        name: fp.name.clone(),
        change,
        pads_added: vec![],
        pads_removed: vec![],
        pads_moved: vec![],
        pads_resized: vec![],
    };
    for fp in new.footprints {
        match old.footprints.iter().find(|o| o.uuid == fp.uuid) {
            None => out.push(footprint(fp, FootprintChange::Added)),
            Some(o) if o != fp => out.push(modified(o, fp)),
            Some(_) => {}
        }
    }
    for fp in old.footprints {
        if !new.footprints.iter().any(|n| n.uuid == fp.uuid) {
            out.push(footprint(fp, FootprintChange::Removed));
        }
    }
    let kept = |a: &Content<'_>, b: &Content<'_>| -> Vec<Uuid> {
        (a.part.footprints.iter())
            .filter(|f| b.part.footprints.contains(f))
            .copied()
            .collect()
    };
    if kept(old, new) != kept(new, old) {
        out.push(PartChange::FootprintOrder);
    }
    if old.part.pin_map != new.part.pin_map {
        out.push(PartChange::PinMap {
            from: old.part.pin_map.clone(),
            to: new.part.pin_map.clone(),
        });
    }
    if from != to {
        out.push(PartChange::FootprintChoice { from, to });
    }
}

/// A changed Footprint with its Pad detail.
fn modified(old: &Footprint, new: &Footprint) -> PartChange {
    let numbers =
        |fp: &Footprint| -> BTreeSet<String> { fp.pads.iter().map(|p| p.number.clone()).collect() };
    let (o, n) = (numbers(old), numbers(new));
    let mut moved = BTreeSet::new();
    let mut resized = BTreeSet::new();
    for pad in &new.pads {
        if let Some(before) = old.pads.iter().find(|p| p.uuid == pad.uuid) {
            if (before.at, before.rotation) != (pad.at, pad.rotation) {
                moved.insert(pad.number.clone());
            }
            if !same_size(before, pad) {
                resized.insert(pad.number.clone());
            }
        }
    }
    PartChange::Footprint {
        footprint: new.uuid,
        name: new.name.clone(),
        change: FootprintChange::Modified,
        pads_added: n.difference(&o).cloned().collect(),
        pads_removed: o.difference(&n).cloned().collect(),
        pads_moved: moved.into_iter().collect(),
        pads_resized: resized.into_iter().collect(),
    }
}

/// Whether two Pads have the same size and shape.
fn same_size(a: &Pad, b: &Pad) -> bool {
    a.shape == b.shape
        && a.width == b.width
        && a.height == b.height
        && a.corner_ratio == b.corner_ratio
        && a.chamfer_ratio == b.chamfer_ratio
        && a.corners == b.corners
        && a.hole == b.hole
}

/// The Pins of `symbol` (other than Not Connected ones) that reach no Pad of `footprint`
/// through `pin_map`, as (Unit name, Pin number).
pub(crate) fn pins_without_pads(
    symbol: &Symbol,
    pin_map: &PinMap,
    footprint: &Footprint,
) -> Vec<(String, String)> {
    (symbol.pins.iter())
        .filter(|p| p.electrical_type != PinType::NotConnected)
        .filter(|p| {
            !(pin_map.pads_of(&p.number).iter())
                .any(|n| footprint.pads.iter().any(|pad| pad.number == *n))
        })
        .map(|p| {
            let unit = symbol.units.iter().find(|u| u.uuid == p.unit);
            (
                unit.map(|u| u.name.clone()).unwrap_or_default(),
                p.number.clone(),
            )
        })
        .collect()
}
