//! The Component side of Sync: the schematic Components that need a Footprint instance,
//! and the add, remove, Footprint and data changes between them and the PCB.

use std::collections::BTreeMap;

use reda_model::local_edit::{LocalEdit, local_edits};
use reda_model::{
    ComponentRef, ComponentType, DocumentKind, Fields, FootprintCopy, FootprintInstance,
    PcbDocument, PinMap, Project, Uuid,
};
use reda_schematic::natural_cmp;

use super::{Change, ChangeKind, FootprintRef, change_id, component_key};

/// One schematic Component that has a Footprint, with the data its Footprint instance
/// copies.
#[derive(Clone, Debug)]
pub(crate) struct SchComponent {
    pub(crate) key: ComponentRef,
    /// The sheet of its first Unit, and that sheet's place among the Schematic Sheets.
    pub(crate) sheet: Uuid,
    pub(crate) sheet_index: usize,
    pub(crate) designator: String,
    pub(crate) fields: Fields,
    pub(crate) component_type: ComponentType,
    pub(crate) in_bom: bool,
    pub(crate) in_cpl: bool,
    pub(crate) not_fitted: bool,
    /// The chosen Footprint, as its sheet copied it.
    pub(crate) copy: FootprintCopy,
    pub(crate) pin_map: PinMap,
}

impl SchComponent {
    /// The Footprint as copied, for the Change List.
    pub(crate) fn footprint_ref(&self) -> FootprintRef {
        footprint_ref(&self.copy)
    }

    /// Whether the PCB copy `copy` is a different Footprint (or a different copy of it).
    /// A Locally Edited Footprint compares the copy it was edited from, so local edits
    /// are neither out of sync nor reverted.
    pub(crate) fn footprint_differs(&self, copy: &FootprintCopy) -> bool {
        copy.footprint.uuid != self.copy.footprint.uuid || copy.sync_hash() != self.copy.hash
    }
}

pub(crate) fn footprint_ref(copy: &FootprintCopy) -> FootprintRef {
    FootprintRef {
        footprint: copy.footprint.uuid,
        name: copy.footprint.name.clone(),
        hash: copy.hash.clone(),
    }
}

/// Every schematic Component with a Footprint, by Component key.
pub(crate) struct Components {
    pub(crate) by_key: BTreeMap<ComponentRef, SchComponent>,
}

impl Components {
    /// The Components of every Schematic Sheet. A multi-unit Component takes its data and
    /// its sheet from its first Unit (the lowest Symbol Unit, on the earliest sheet).
    pub(crate) fn gather(project: &Project) -> Components {
        // (Symbol Unit index, sheet index) of the Unit chosen so far, with its data.
        let mut best: BTreeMap<ComponentRef, ((usize, usize), Option<SchComponent>)> =
            BTreeMap::new();
        let sheets = project
            .documents()
            .filter(|d| d.kind == DocumentKind::Schematic)
            .filter_map(|d| project.schematic_sheet(d.uuid));
        for (sheet_index, sheet) in sheets.enumerate() {
            for unit in sheet.units() {
                let Some(entry) = sheet.part(unit.part) else {
                    continue;
                };
                let unit_index = entry
                    .symbol
                    .units
                    .iter()
                    .position(|u| u.uuid == unit.unit)
                    .unwrap_or(usize::MAX);
                let rank = (unit_index, sheet_index);
                if best.get(&unit.component).is_some_and(|(r, _)| *r <= rank) {
                    continue;
                }
                let copy = unit
                    .footprint
                    .and_then(|fp| entry.part.footprints.iter().position(|f| *f == fp))
                    .and_then(|i| entry.footprints.get(i))
                    .and_then(|copy| sheet.footprint(*copy));
                let component = copy.map(|copy| SchComponent {
                    key: unit.component.clone(),
                    sheet: sheet.uuid,
                    sheet_index,
                    designator: unit.designator.clone(),
                    fields: unit.fields.clone(),
                    component_type: unit.component_type,
                    in_bom: unit.in_bom,
                    in_cpl: unit.in_cpl,
                    not_fitted: unit.not_fitted,
                    copy: copy.clone(),
                    pin_map: entry.part.pin_map.clone(),
                });
                best.insert(unit.component.clone(), (rank, component));
            }
        }
        Components {
            by_key: best
                .into_iter()
                .filter_map(|(key, (_, c))| Some((key, c?)))
                .collect(),
        }
    }

    pub(crate) fn get(&self, key: &ComponentRef) -> Option<&SchComponent> {
        self.by_key.get(key)
    }

    /// The Components in placement order: by sheet, then by Designator.
    pub(crate) fn in_sheet_order(&self) -> Vec<&SchComponent> {
        let mut list: Vec<&SchComponent> = self.by_key.values().collect();
        list.sort_by(|a, b| {
            a.sheet_index
                .cmp(&b.sheet_index)
                .then_with(|| natural_cmp(&a.designator, &b.designator))
                .then_with(|| a.key.cmp(&b.key))
        });
        list
    }

    /// The add, remove, Footprint and data changes.
    pub(crate) fn changes(&self, pcb: &PcbDocument) -> Vec<Change> {
        let linked = linked_instances(pcb);
        let mut out = Vec::new();
        let mut data = Vec::new();
        for c in self.in_sheet_order() {
            let key = component_key(&c.key);
            let Some(instance) = linked.get(&c.key) else {
                out.push(Change {
                    id: change_id("add_component", &[&key]),
                    requires: vec![],
                    change: ChangeKind::AddComponent {
                        component: c.key.clone(),
                        designator: c.designator.clone(),
                        footprint: c.copy.footprint.uuid,
                        footprint_name: c.copy.footprint.name.clone(),
                        sheet: c.sheet,
                    },
                });
                continue;
            };
            let current = pcb.footprint_copy(instance.footprint);
            if let Some(current) = current
                && c.footprint_differs(current)
            {
                out.push(Change {
                    id: change_id("change_footprint", &[&key]),
                    requires: vec![],
                    change: ChangeKind::ChangeFootprint {
                        component: c.key.clone(),
                        instance: instance.uuid,
                        designator: c.designator.clone(),
                        from: footprint_ref(current),
                        to: c.footprint_ref(),
                        lost_local_edits: lost_local_edits(pcb, current),
                    },
                });
            }
            if instance.designator != c.designator {
                data.push(Change {
                    id: change_id("designator", &[&key]),
                    requires: vec![],
                    change: ChangeKind::Designator {
                        component: c.key.clone(),
                        instance: instance.uuid,
                        from: instance.designator.clone(),
                        to: c.designator.clone(),
                    },
                });
            }
            if instance.fields != c.fields {
                data.push(Change {
                    id: change_id("fields", &[&key]),
                    requires: vec![],
                    change: ChangeKind::Fields {
                        component: c.key.clone(),
                        instance: instance.uuid,
                        designator: c.designator.clone(),
                        from: instance.fields.clone(),
                        to: c.fields.clone(),
                    },
                });
            }
            if instance.component_type != c.component_type {
                data.push(Change {
                    id: change_id("component_type", &[&key]),
                    requires: vec![],
                    change: ChangeKind::ComponentType {
                        component: c.key.clone(),
                        instance: instance.uuid,
                        designator: c.designator.clone(),
                        from: instance.component_type,
                        to: c.component_type,
                    },
                });
            }
            if instance.in_bom != c.in_bom {
                data.push(Change {
                    id: change_id("in_bom", &[&key]),
                    requires: vec![],
                    change: ChangeKind::InBom {
                        component: c.key.clone(),
                        instance: instance.uuid,
                        designator: c.designator.clone(),
                        from: instance.in_bom,
                        to: c.in_bom,
                    },
                });
            }
            if instance.in_cpl != c.in_cpl {
                data.push(Change {
                    id: change_id("in_cpl", &[&key]),
                    requires: vec![],
                    change: ChangeKind::InCpl {
                        component: c.key.clone(),
                        instance: instance.uuid,
                        designator: c.designator.clone(),
                        from: instance.in_cpl,
                        to: c.in_cpl,
                    },
                });
            }
            if instance.not_fitted != c.not_fitted {
                data.push(Change {
                    id: change_id("not_fitted", &[&key]),
                    requires: vec![],
                    change: ChangeKind::NotFitted {
                        component: c.key.clone(),
                        instance: instance.uuid,
                        designator: c.designator.clone(),
                        from: instance.not_fitted,
                        to: c.not_fitted,
                    },
                });
            }
        }
        let mut gone: Vec<&FootprintInstance> = linked
            .iter()
            .filter(|(key, _)| !self.by_key.contains_key(*key))
            .map(|(_, i)| *i)
            .collect();
        gone.sort_by(|a, b| natural_cmp(&a.designator, &b.designator).then(a.uuid.cmp(&b.uuid)));
        for instance in gone {
            let component = instance.component.clone().expect("a linked instance");
            out.push(Change {
                id: change_id("remove_component", &[&component_key(&component)]),
                requires: vec![],
                change: ChangeKind::RemoveComponent {
                    component,
                    instance: instance.uuid,
                    designator: instance.designator.clone(),
                },
            });
        }
        out.extend(data);
        out
    }
}

/// The Footprint instances linked to a Component, by Component key. Mechanical Parts
/// placed only on the PCB have no link, and Sync leaves them alone.
pub(crate) fn linked_instances(pcb: &PcbDocument) -> BTreeMap<ComponentRef, &FootprintInstance> {
    pcb.footprints()
        .filter_map(|i| Some((i.component.clone()?, i)))
        .collect()
}

/// The local edits a Footprint change would remove from an instance on PCB copy `copy`.
pub(crate) fn lost_local_edits(pcb: &PcbDocument, copy: &FootprintCopy) -> Vec<LocalEdit> {
    copy.edited_from
        .as_ref()
        .and_then(|from| pcb.footprint_copy(from.uuid))
        .map_or_else(Vec::new, |origin| {
            local_edits(&origin.footprint, &copy.footprint)
        })
}
