//! Where used: the Parts of the scanned Libraries that use a Symbol or Footprint, and the
//! Components of a Project whose part-table copies use any element.

use std::collections::BTreeSet;

use reda_model::{DocumentKind, PartEntry, Project, Uuid};

use crate::index::LibraryIndex;
use crate::scan::ElementKind;
use crate::search::SearchHit;

impl LibraryIndex {
    /// The Parts of every scanned Library that name `element` as their Symbol or as one of
    /// their alternate Footprints, sorted by name and then UUID. A Part uses nothing else.
    pub fn parts_using(&self, element: Uuid) -> Vec<SearchHit<'_>> {
        let mut hits: Vec<SearchHit<'_>> = self
            .parts()
            .filter(|(_, part)| part.symbol == Some(element) || part.footprints.contains(&element))
            .map(|(library, part)| SearchHit { library, part })
            .collect();
        hits.sort_by(|a, b| (&a.part.name, a.part.uuid).cmp(&(&b.part.name, b.part.uuid)));
        hits
    }
}

/// One Component of a Project that uses an element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentUse {
    pub component: Uuid,
    pub designator: String,
    /// The sheet its Units are on.
    pub sheet: Uuid,
    /// The Library Part it was placed from.
    pub part: Uuid,
}

/// Whether the part-table entry was copied from `element`, or from a Part that uses it.
fn entry_uses(entry: &PartEntry, element: Uuid) -> bool {
    entry.part.uuid == element
        || entry.symbol.uuid == element
        || entry.part.footprints.contains(&element)
}

/// The Components of `project` whose part-table entry uses `element` (the Part itself,
/// its Symbol or one of its alternate Footprints), once each, in sheet order and then by
/// Component UUID.
pub fn components_using(project: &Project, element: Uuid) -> Vec<ComponentUse> {
    let mut found = vec![];
    let mut seen = BTreeSet::new();
    let sheets = project
        .documents()
        .filter(|d| d.kind == DocumentKind::Schematic)
        .filter_map(|d| project.schematic_sheet(d.uuid).map(|s| (d.uuid, s)));
    for (uuid, sheet) in sheets {
        let mut units: Vec<_> = sheet.units().collect();
        units.sort_by_key(|u| u.component.uuid);
        for unit in units {
            let Some(entry) = sheet.part(unit.part) else {
                continue;
            };
            if entry_uses(entry, element) && seen.insert(unit.component.uuid) {
                found.push(ComponentUse {
                    component: unit.component.uuid,
                    designator: unit.designator.clone(),
                    sheet: uuid,
                    part: entry.part.uuid,
                });
            }
        }
    }
    found
}

/// The kind and name of `element` as a part table of `project` holds a copy of it, for an
/// element no scanned Library holds.
pub fn project_element(project: &Project, element: Uuid) -> Option<(ElementKind, String)> {
    let sheets = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid));
    for sheet in sheets {
        for entry in sheet.parts() {
            if entry.part.uuid == element {
                return Some((ElementKind::Part, entry.part.name.clone()));
            }
            if entry.symbol.uuid == element {
                return Some((ElementKind::Symbol, entry.symbol.name.clone()));
            }
        }
        if let Some(copy) = sheet.footprints().find(|c| c.footprint.uuid == element) {
            return Some((ElementKind::Footprint, copy.footprint.name.clone()));
        }
    }
    None
}
