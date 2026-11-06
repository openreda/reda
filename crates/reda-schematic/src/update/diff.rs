//! What an Update from Library changes on one Component: the differences between the
//! part-table copy it was placed from and the Library Part now, and the Component's own
//! data after the update.

use std::collections::BTreeSet;

use reda_command::PartCopy;
use reda_model::{ComponentType, Fields, PartEntry, SchematicSheet, UnitInstance, Uuid};
use schemars::JsonSchema;
use serde::Serialize;

/// One difference between the placed copy and the Library Part.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PartChange {
    /// The Part was renamed.
    Name { from: String, to: String },
    /// The Part's Component Type changed. `edited` when the Component has its own
    /// Component Type, which it keeps.
    ComponentType {
        from: ComponentType,
        to: ComponentType,
        #[serde(default, skip_serializing_if = "is_false")]
        edited: bool,
    },
    /// A default field changed (`""` = not set). `edited` when the Component has its own
    /// value for it, which it keeps; otherwise it takes the new default.
    Field {
        name: String,
        from: String,
        to: String,
        #[serde(default, skip_serializing_if = "is_false")]
        edited: bool,
    },
    /// The Part's pin-pad map overrides changed.
    PinMap,
    /// The Symbol changed (pins, graphics, Units or names); `name` is its name now.
    Symbol { name: String },
    /// A Unit of the Component that is placed no longer exists in the Symbol. The
    /// Component cannot be updated until that Unit is deleted.
    UnitRemoved { unit: String },
    /// An alternate Footprint was added, removed or changed. The PCB gets a Footprint
    /// change through Sync, not through the update.
    Footprint {
        footprint: Uuid,
        name: String,
        change: FootprintChange,
    },
    /// The alternates are in a new order; the first is the default.
    FootprintOrder,
    /// The Component's Footprint is no longer an alternate, so it moves to the new
    /// default (`null` when the Part has none).
    FootprintChoice { from: Uuid, to: Option<Uuid> },
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
    /// Whether this change stops the update.
    pub fn blocks(&self) -> bool {
        matches!(self, PartChange::UnitRemoved { .. })
    }
}

/// The differences between `entry` (on `sheet`) and `copy`, for a Component whose Units
/// are `units` (all carrying the same Component data).
pub fn changes(
    sheet: &SchematicSheet,
    entry: &PartEntry,
    copy: &PartCopy,
    units: &[&UnitInstance],
) -> Vec<PartChange> {
    let (old, new) = (&entry.part, &copy.part);
    let component = units[0];
    let mut out = vec![];
    if old.name != new.name {
        out.push(PartChange::Name {
            from: old.name.clone(),
            to: new.name.clone(),
        });
    }
    if old.component_type != new.component_type {
        out.push(PartChange::ComponentType {
            from: old.component_type,
            to: new.component_type,
            edited: component.component_type != old.component_type,
        });
    }
    for name in field_names(&old.fields, &new.fields) {
        let (from, to) = (old.fields.get(&name), new.fields.get(&name));
        if from != to {
            out.push(PartChange::Field {
                edited: component.fields.get(&name) != from,
                from: from.to_owned(),
                to: to.to_owned(),
                name,
            });
        }
    }
    if old.pin_map != new.pin_map {
        out.push(PartChange::PinMap);
    }
    if let Some(symbol) = &copy.symbol {
        if *symbol != entry.symbol {
            out.push(PartChange::Symbol {
                name: symbol.name.clone(),
            });
        }
        let kept: BTreeSet<Uuid> = symbol.units.iter().map(|u| u.uuid).collect();
        for unit in &entry.symbol.units {
            let placed = units.iter().any(|u| u.unit == unit.uuid);
            if placed && !kept.contains(&unit.uuid) {
                out.push(PartChange::UnitRemoved {
                    unit: unit.name.clone(),
                });
            }
        }
    }
    footprint_changes(sheet, entry, copy, &mut out);
    if let Some(from) = component.footprint
        && !new.footprints.contains(&from)
    {
        out.push(PartChange::FootprintChoice {
            from,
            to: new.footprints.first().copied(),
        });
    }
    out
}

fn footprint_changes(
    sheet: &SchematicSheet,
    entry: &PartEntry,
    copy: &PartCopy,
    out: &mut Vec<PartChange>,
) {
    let old: Vec<_> = entry
        .footprints
        .iter()
        .map(|c| sheet.footprint(*c).expect("an entry's copies exist"))
        .collect();
    for new in &copy.footprints {
        let fp = &new.footprint;
        let change = match old.iter().find(|c| c.footprint.uuid == fp.uuid) {
            None => FootprintChange::Added,
            Some(c) if c.hash != new.hash => FootprintChange::Modified,
            Some(_) => continue,
        };
        out.push(PartChange::Footprint {
            footprint: fp.uuid,
            name: fp.name.clone(),
            change,
        });
    }
    for c in &old {
        if !copy.part.footprints.contains(&c.footprint.uuid) {
            out.push(PartChange::Footprint {
                footprint: c.footprint.uuid,
                name: c.footprint.name.clone(),
                change: FootprintChange::Removed,
            });
        }
    }
    let kept_old: Vec<Uuid> = entry
        .part
        .footprints
        .iter()
        .filter(|f| copy.part.footprints.contains(f))
        .copied()
        .collect();
    let kept_new: Vec<Uuid> = copy
        .part
        .footprints
        .iter()
        .filter(|f| entry.part.footprints.contains(f))
        .copied()
        .collect();
    if kept_old != kept_new {
        out.push(PartChange::FootprintOrder);
    }
}

/// The built-in field names, then the free-form ones: the old Part's in its order, then
/// the new Part's new ones.
fn field_names(old: &Fields, new: &Fields) -> Vec<String> {
    let mut names: Vec<String> = Fields::BUILT_IN.iter().map(|&n| n.to_owned()).collect();
    for field in old.custom.iter().chain(&new.custom) {
        if !names.contains(&field.name) {
            names.push(field.name.clone());
        }
    }
    names
}

/// The Unit as the update leaves it: pointing at `entry`, with every changed field and
/// Component Type the Component had not edited taking the new default, a Footprint that
/// is no longer an alternate moved to the new default, and Not Fitted cleared on a Net Tie.
pub fn updated_unit(unit: &UnitInstance, entry: Uuid, changes: &[PartChange]) -> UnitInstance {
    let mut unit = unit.clone();
    unit.part = entry;
    for change in changes {
        match change {
            PartChange::Field {
                name,
                to,
                edited: false,
                ..
            } => unit.fields.set(name, to),
            PartChange::ComponentType {
                to, edited: false, ..
            } => unit.component_type = *to,
            PartChange::FootprintChoice { to, .. } => unit.footprint = *to,
            _ => {}
        }
    }
    // A Net Tie refuses Not Fitted, so becoming one clears it.
    if !unit.component_type.allows_not_fitted() {
        unit.not_fitted = false;
    }
    unit
}
