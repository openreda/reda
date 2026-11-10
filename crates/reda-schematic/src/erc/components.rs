//! The ERC checks of Components: duplicate Designators, unplaced Units of placed
//! multi-unit Components, Units placed twice, Components with no Footprint choice and Unit
//! data mismatches.

use std::collections::BTreeMap;

use reda_model::{
    ComponentRef, ComponentType, ErcCheck, Fields, PartEntry, Project, UnitInstance, Uuid,
};

use super::Report;
use crate::natural::natural_cmp;

/// One placed Unit with the part-table entry it was placed from.
struct Placed<'a> {
    unit: &'a UnitInstance,
    entry: &'a PartEntry,
}

pub(super) fn check(project: &Project, report: &mut Report<'_>) {
    // Every Component's Units, Components in order of their first Unit (Document order,
    // then Unit UUID).
    let mut order: Vec<&ComponentRef> = vec![];
    let mut components: BTreeMap<&ComponentRef, Vec<Placed>> = BTreeMap::new();
    for document in project.documents() {
        let Some(sheet) = project.schematic_sheet(document.uuid) else {
            continue;
        };
        for unit in sheet.units() {
            let entry = sheet.part(unit.part).expect("a Unit's part entry exists");
            let units = components.entry(&unit.component).or_default();
            if units.is_empty() {
                order.push(&unit.component);
            }
            units.push(Placed { unit, entry });
        }
    }
    let components: Vec<&Vec<Placed>> = order.iter().map(|c| &components[c]).collect();
    duplicate_designators(&components, report);
    for units in &components {
        unplaced_units(units, report);
    }
    for units in &components {
        placed_twice(units, report);
    }
    for units in &components {
        no_footprint_choice(units, report);
    }
    for units in &components {
        mismatch(units, report);
    }
}

fn duplicate_designators(components: &[&Vec<Placed>], report: &mut Report<'_>) {
    // Designator -> the Components that carry it, each with its Units that do.
    let mut by_designator: BTreeMap<&str, Vec<Vec<Uuid>>> = BTreeMap::new();
    for units in components {
        let mut mine: BTreeMap<&str, Vec<Uuid>> = BTreeMap::new();
        for placed in units.iter() {
            (mine.entry(placed.unit.designator.as_str()).or_default()).push(placed.unit.uuid);
        }
        for (designator, units) in mine {
            by_designator.entry(designator).or_default().push(units);
        }
    }
    let mut designators: Vec<&str> = by_designator.keys().copied().collect();
    designators.sort_by(|a, b| natural_cmp(a, b));
    for designator in designators {
        let carriers = &by_designator[designator];
        if carriers.len() > 1 {
            report.add(ErcCheck::DuplicateDesignator, |v| {
                v.arg("designator", designator)
                    .objects(carriers.iter().flatten().copied())
            });
        }
    }
}

fn unplaced_units(units: &[Placed], report: &mut Report<'_>) {
    let first = &units[0];
    let symbol = &first.entry.symbol;
    if symbol.units.len() < 2 {
        return;
    }
    let missing: Vec<&str> = (symbol.units.iter())
        .filter(|u| !units.iter().any(|p| p.unit.unit == u.uuid))
        .map(|u| u.name.as_str())
        .collect();
    if !missing.is_empty() {
        report.add(ErcCheck::UnplacedUnits, |v| {
            v.arg("designator", first.unit.designator.as_str())
                .arg("units", missing.join(", "))
                .objects(units.iter().map(|p| p.unit.uuid))
        });
    }
}

/// One Violation per Symbol Unit of a Component placed more than once, in Symbol Unit
/// order, naming every placement.
fn placed_twice(units: &[Placed], report: &mut Report<'_>) {
    let first = &units[0];
    for symbol_unit in &first.entry.symbol.units {
        let placed: Vec<Uuid> = (units.iter())
            .filter(|p| p.unit.unit == symbol_unit.uuid)
            .map(|p| p.unit.uuid)
            .collect();
        if placed.len() > 1 {
            report.add(ErcCheck::UnitPlacedTwice, |v| {
                v.arg("designator", first.unit.designator.as_str())
                    .arg("unit", symbol_unit.name.as_str())
                    .arg("count", placed.len() as i64)
                    .objects(placed)
            });
        }
    }
}

/// A Standard or Net Tie Component with no Footprint chosen, so it cannot reach the board.
fn no_footprint_choice(units: &[Placed], report: &mut Report<'_>) {
    let first = units[0].unit;
    let needs = matches!(
        first.component_type,
        ComponentType::Standard | ComponentType::NetTie
    );
    if needs && units.iter().all(|p| p.unit.footprint.is_none()) {
        report.add(ErcCheck::NoFootprintChoice, |v| {
            v.arg("designator", first.designator.as_str())
                .objects(units.iter().map(|p| p.unit.uuid))
        });
    }
}

/// Units of one Component whose Designator, value, other fields, Footprint choice,
/// Component Type, In BOM, In CPL or Not Fitted differ.
fn mismatch(units: &[Placed], report: &mut Report<'_>) {
    let first = units[0].unit;
    let differs: [(&str, Differs); 8] = [
        ("designator", |a, b| a.designator != b.designator),
        ("value", |a, b| a.fields.value != b.fields.value),
        ("fields", |a, b| other_fields(a) != other_fields(b)),
        ("footprint", |a, b| a.footprint != b.footprint),
        ("component_type", |a, b| {
            a.component_type != b.component_type
        }),
        ("in_bom", |a, b| a.in_bom != b.in_bom),
        ("in_cpl", |a, b| a.in_cpl != b.in_cpl),
        ("not_fitted", |a, b| a.not_fitted != b.not_fitted),
    ];
    let found: Vec<&str> = differs
        .iter()
        .filter(|(_, differ)| units.iter().any(|p| differ(p.unit, first)))
        .map(|(name, _)| *name)
        .collect();
    if !found.is_empty() {
        report.add(ErcCheck::UnitDataMismatch, |v| {
            v.arg("designator", first.designator.as_str())
                .arg("differs", found.join(", "))
                .objects(units.iter().map(|p| p.unit.uuid))
        });
    }
}

/// Whether two Units differ in one piece of Component data.
type Differs = fn(&UnitInstance, &UnitInstance) -> bool;

/// A Unit's fields other than the value.
fn other_fields(unit: &UnitInstance) -> Fields {
    let mut fields = unit.fields.clone();
    fields.value.clear();
    fields
}
