//! Sync: the one-way Change List from the Schematic Sheets to the PCB Document (ADR-0006).
//!
//! [`plan`] compares the schematic with the PCB and returns every pending change as a
//! [`ChangeList`]; nothing about it is stored, so unapplied changes simply show up again.
//! [`apply`] carries out a planned list, leaving out the changes the caller unticked and
//! every change that depends on them, in one transaction (only the PCB Document changes).
//!
//! - Components match Footprint instances by [`ComponentRef`] (Sheet Instance path +
//!   Component UUID), and Pads are found through the Part's pin-pad map.
//! - Derived schematic Nets match stored PCB Nets by greatest Pad overlap, name breaking
//!   ties ([`nets`]).
//! - New Footprints go into a fresh cluster outside the board ([`placement`]).

mod apply;
mod components;
mod nets;
mod placement;

use std::collections::BTreeSet;

use reda_model::{ComponentRef, DrcCheck, Fields, Project, Severity, Uuid, Violation};
use schemars::JsonSchema;
use serde::Serialize;

use crate::library::ContentHash;

pub use apply::{Applied, apply};

/// The pending changes, grouped by kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ChangeList {
    /// A hash of the whole list; `sync.apply` refuses to run when the list no longer
    /// hashes the same.
    pub hash: String,
    /// How many changes are pending: the "out of sync" count.
    pub count: usize,
    /// The non-empty groups, in a fixed order: components, footprints, component data,
    /// Nets, Pad Nets.
    pub groups: Vec<Group>,
}

impl ChangeList {
    /// Every change, in list order.
    pub fn changes(&self) -> impl Iterator<Item = &Change> {
        self.groups.iter().flat_map(|g| &g.changes)
    }

    /// The ids left out when `unticked` are: those plus, transitively, every change that
    /// requires one of them.
    pub fn cascade(&self, unticked: &BTreeSet<String>) -> BTreeSet<String> {
        let mut out = unticked.clone();
        loop {
            let before = out.len();
            for change in self.changes() {
                if change.requires.iter().any(|r| out.contains(r)) {
                    out.insert(change.id.clone());
                }
            }
            if out.len() == before {
                return out;
            }
        }
    }
}

/// One group of the Change List.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Group {
    pub group: GroupKind,
    pub changes: Vec<Change>,
}

/// The kinds of change, as the Change List groups them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    /// Adding or removing a Component.
    Components,
    /// Changing a Footprint.
    Footprints,
    /// Changing a Designator, the fields (value included) or Not Fitted.
    ComponentData,
    /// Adding, renaming or removing a Net.
    Nets,
    /// Changing a Pad's Net.
    PadNets,
}

/// One pending change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Change {
    /// A stable id: a hash of the change's kind and targets, the same in every preview
    /// while those targets exist.
    pub id: String,
    /// The changes this one depends on; leaving one of them out leaves this one out too.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(flatten)]
    pub change: ChangeKind,
}

/// What a change does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeKind {
    /// Places a Footprint instance for a Component the PCB does not have yet.
    AddComponent {
        component: ComponentRef,
        designator: String,
        /// The Library Footprint chosen for it.
        footprint: Uuid,
        footprint_name: String,
        /// The Schematic Sheet it is grouped with when placed: its first Unit's.
        sheet: Uuid,
    },
    /// Deletes the Footprint instance of a Component the schematic no longer has (or that
    /// no longer has a Footprint). Copper attached to it stays.
    RemoveComponent {
        component: ComponentRef,
        /// The Footprint instance.
        instance: Uuid,
        designator: String,
    },
    /// Swaps a Footprint in place, keeping position, rotation, side, lock and Designator
    /// text placement; its Pads get their Nets again from the new pin-pad map.
    ChangeFootprint {
        component: ComponentRef,
        instance: Uuid,
        designator: String,
        from: FootprintRef,
        to: FootprintRef,
    },
    /// Changes the Designator.
    Designator {
        component: ComponentRef,
        instance: Uuid,
        from: String,
        to: String,
    },
    /// Changes the fields, value included.
    Fields {
        component: ComponentRef,
        instance: Uuid,
        designator: String,
        from: Fields,
        to: Fields,
    },
    /// Changes Not Fitted.
    NotFitted {
        component: ComponentRef,
        instance: Uuid,
        designator: String,
        from: bool,
        to: bool,
    },
    /// Adds a PCB Net, in the Default Net Class.
    AddNet { name: String },
    /// Renames a PCB Net, keeping its UUID.
    RenameNet { net: Uuid, from: String, to: String },
    /// Removes a PCB Net no schematic Net matches any more.
    RemoveNet {
        net: Uuid,
        name: String,
        /// The PCB Net its Pads went to, when it merged into another one.
        #[serde(skip_serializing_if = "Option::is_none")]
        merge_into: Option<Uuid>,
    },
    /// Moves a Pad to another Net, or to none.
    PadNet {
        component: ComponentRef,
        /// The Footprint instance; left out while it is still to be added.
        #[serde(skip_serializing_if = "Option::is_none")]
        instance: Option<Uuid>,
        designator: String,
        /// The Pad of the (new) Footprint.
        pad: Uuid,
        number: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        from: Option<NetRef>,
        #[serde(skip_serializing_if = "Option::is_none")]
        to: Option<NetRef>,
    },
}

impl ChangeKind {
    pub fn group(&self) -> GroupKind {
        match self {
            ChangeKind::AddComponent { .. } | ChangeKind::RemoveComponent { .. } => {
                GroupKind::Components
            }
            ChangeKind::ChangeFootprint { .. } => GroupKind::Footprints,
            ChangeKind::Designator { .. }
            | ChangeKind::Fields { .. }
            | ChangeKind::NotFitted { .. } => GroupKind::ComponentData,
            ChangeKind::AddNet { .. }
            | ChangeKind::RenameNet { .. }
            | ChangeKind::RemoveNet { .. } => GroupKind::Nets,
            ChangeKind::PadNet { .. } => GroupKind::PadNets,
        }
    }
}

/// A Footprint as copied: the Library Footprint and the content hash of the copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FootprintRef {
    pub footprint: Uuid,
    pub name: String,
    pub hash: String,
}

/// A PCB Net a Pad is or will be on: its UUID once it exists, and its name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct NetRef {
    /// Left out for a Net that this Sync adds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    pub name: String,
}

/// Why there is nothing to compare: the Project has no PCB Document.
pub const NO_PCB: &str = "the Project has no PCB Document; add one with `document.add {kind: pcb}`";

/// The pending Change List, or `None` when the Project has no PCB Document.
pub fn plan(project: &Project) -> Option<ChangeList> {
    Some(Plan::new(project)?.list)
}

/// How many changes are pending: the "out of sync: N changes" count. Zero without a PCB
/// Document.
pub fn pending_count(project: &Project) -> usize {
    plan(project).map_or(0, |l| l.count)
}

/// DRC's `out_of_sync` Violation: present while changes are pending and the board's
/// severity for the check is not Off.
pub fn out_of_sync_violation(project: &Project) -> Option<Violation> {
    let pcb = project.pcb()?;
    let severity = pcb.setup().severities.get(DrcCheck::OutOfSync);
    let count = pending_count(project);
    if severity == Severity::Off || count == 0 {
        return None;
    }
    let count = i64::try_from(count).unwrap_or(i64::MAX);
    Some(Violation::new("out_of_sync", severity).arg("changes", count))
}

/// A planned Change List with what applying it needs.
pub(crate) struct Plan {
    pub(crate) list: ChangeList,
    pub(crate) components: components::Components,
    pub(crate) nets: nets::NetPlan,
}

impl Plan {
    pub(crate) fn new(project: &Project) -> Option<Plan> {
        let pcb = project.pcb()?;
        let components = components::Components::gather(project);
        let nets = nets::NetPlan::new(project, pcb, &components);
        let mut changes = components.changes(pcb);
        changes.extend(nets.changes.iter().cloned());
        changes.sort_by_key(|c| c.change.group());
        let mut groups: Vec<Group> = Vec::new();
        for change in changes {
            let group = change.change.group();
            match groups.last_mut() {
                Some(g) if g.group == group => g.changes.push(change),
                _ => groups.push(Group {
                    group,
                    changes: vec![change],
                }),
            }
        }
        let mut hasher = ContentHash::new();
        hasher.write(&serde_json::to_vec(&groups).expect("a Change List serialises"));
        let list = ChangeList {
            hash: hasher.finish(),
            count: groups.iter().map(|g| g.changes.len()).sum(),
            groups,
        };
        Some(Plan {
            list,
            components,
            nets,
        })
    }
}

/// A change id: the hash of its kind and targets.
pub(crate) fn change_id(kind: &str, targets: &[&str]) -> String {
    let mut hasher = ContentHash::new();
    hasher.write(kind.as_bytes());
    for target in targets {
        hasher.write(&[0]);
        hasher.write(target.as_bytes());
    }
    hasher.finish()
}

/// A Component key as one string, for change ids: the path, then the UUID.
pub(crate) fn component_key(component: &ComponentRef) -> String {
    let mut key = String::new();
    for uuid in &component.path {
        key.push_str(&uuid.to_string());
        key.push('/');
    }
    key.push_str(&component.uuid.to_string());
    key
}
