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
//! - Edits made from the PCB side write through ([`write_through`]): they change the
//!   schematic and carry the Sync changes they cause to the board in the same edit.

mod apply;
mod components;
mod nets;
mod notes;
mod placement;
mod write_through;

use std::collections::BTreeSet;

use reda_model::{
    ComponentRef, ComponentType, DrcCheck, Fields, Project, Severity, Uuid, Violation, ViolationArg,
};
use schemars::JsonSchema;
use serde::Serialize;

use crate::library::ContentHash;
use reda_model::local_edit::LocalEdit;

pub use apply::{Applied, apply};
pub use write_through::write_through;

/// The pending changes, grouped by kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ChangeList {
    /// A hash of the whole list; `sync.apply` refuses to run when the list no longer
    /// hashes the same.
    pub hash: String,
    /// How many changes are pending: the "out of sync" count. Problem rows are not
    /// counted.
    pub count: usize,
    /// The non-empty groups, in [`GroupKind::ALL`] order.
    pub groups: Vec<Group>,
    /// Set while a `duplicate_nets` Violation stands: `sync.apply` refuses until it is
    /// fixed, waived or the check is Off. Not a row: a banner over the whole list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_nets: Option<DuplicateNets>,
}

/// Why the Change List can't apply: different Nets share a board name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DuplicateNets {
    pub message: String,
    /// One entry per board name, as the standing Violations give them.
    pub nets: Vec<DuplicateNet>,
}

/// One board name that several Nets share.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DuplicateNet {
    pub name: String,
    /// How many Nets share it.
    pub count: usize,
    /// The objects naming them, as the Violation lists them.
    pub objects: Vec<Uuid>,
}

impl DuplicateNets {
    /// The block for the standing `duplicate_nets` Violations; `None` when none stand.
    fn of(project: &Project) -> Option<DuplicateNets> {
        let found = reda_schematic::erc::standing_duplicate_nets(project);
        if found.is_empty() {
            return None;
        }
        // Auto-names collide only through duplicate Designators, which don't block Sync.
        let shared = nets::shared_names(project);
        let found: Vec<Violation> = (found.into_iter())
            .filter(|v| match v.args.get("name") {
                Some(ViolationArg::Text(name)) => {
                    shared.contains(&reda_model::net_name::name_key(name))
                }
                _ => false,
            })
            .collect();
        if found.is_empty() {
            return None;
        }
        let nets: Vec<DuplicateNet> = (found.into_iter())
            .map(|v| DuplicateNet {
                name: match v.args.get("name") {
                    Some(ViolationArg::Text(name)) => name.clone(),
                    _ => String::new(),
                },
                count: match v.args.get("count") {
                    Some(ViolationArg::Int(n)) => usize::try_from(*n).unwrap_or(0),
                    _ => 0,
                },
                objects: v.objects,
            })
            .collect();
        let names: Vec<&str> = nets.iter().map(|n| n.name.as_str()).collect();
        Some(DuplicateNets {
            message: format!(
                "Sync can't apply while ERC's duplicate_nets stands: different Nets share \
                 the board name {}. Rename a Net, turn on Append Sheet Numbers to Local \
                 Nets, or waive the Violation (or turn the check Off) to merge them.",
                names.join(", ")
            ),
            nets,
        })
    }
}

impl ChangeList {
    /// Every change, in list order.
    pub fn changes(&self) -> impl Iterator<Item = &Change> {
        self.groups.iter().flat_map(|g| &g.changes)
    }

    /// The ids left out when `unticked` are: those, the Problem rows, plus, transitively,
    /// every change that requires one of them.
    pub fn cascade(&self, unticked: &BTreeSet<String>) -> BTreeSet<String> {
        let mut out = unticked.clone();
        out.extend(
            self.changes()
                .filter(|c| c.is_problem())
                .map(|c| c.id.clone()),
        );
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

/// The Change List's groups: one per kind of change, in this order. The ids are stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    /// `add_component`.
    AddComponents,
    /// `remove_component`.
    RemoveComponents,
    /// `change_footprint`.
    ChangeFootprints,
    /// `designator`.
    Designators,
    /// `fields`, value included.
    Fields,
    /// `component_type`.
    ComponentTypes,
    /// `in_bom`.
    InBom,
    /// `in_cpl`.
    InCpl,
    /// `not_fitted`.
    NotFitted,
    /// `add_net`.
    AddNets,
    /// `rename_net`.
    RenameNets,
    /// `remove_net`.
    RemoveNets,
    /// `pad_net`, and the Problem rows `unmapped_pin` of Pins with no Pad.
    PadNets,
}

impl GroupKind {
    /// Every group, in list order.
    pub const ALL: [GroupKind; 13] = [
        GroupKind::AddComponents,
        GroupKind::RemoveComponents,
        GroupKind::ChangeFootprints,
        GroupKind::Designators,
        GroupKind::Fields,
        GroupKind::ComponentTypes,
        GroupKind::InBom,
        GroupKind::InCpl,
        GroupKind::NotFitted,
        GroupKind::AddNets,
        GroupKind::RenameNets,
        GroupKind::RemoveNets,
        GroupKind::PadNets,
    ];
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
    pub status: Status,
    /// What the status means for this change; left out when it is OK.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(flatten)]
    pub change: ChangeKind,
}

impl Change {
    pub fn is_problem(&self) -> bool {
        self.status == Status::Problem
    }

    /// Gives the change a Note, or adds `message` to the one it has.
    pub(crate) fn note(&mut self, message: String) {
        if self.status == Status::Problem {
            return;
        }
        self.status = Status::Note;
        self.message = Some(match self.message.take() {
            Some(before) => format!("{before} {message}"),
            None => message,
        });
    }

    /// Makes the change a Problem row: it can't apply.
    pub(crate) fn problem(&mut self, message: String) {
        self.status = Status::Problem;
        self.message = Some(message);
    }
}

/// A row's status in the Change List.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Applies, with nothing to say.
    Ok,
    /// Applies, with a consequence worth reading in `message`.
    Note,
    /// Can't apply: it can't be ticked and `sync.apply` skips it, with every change that
    /// requires it. It is not counted as pending.
    Problem,
}

/// What a change does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeKind {
    /// Places a Footprint instance for a Component the PCB does not have yet.
    AddComponent {
        component: ComponentRef,
        designator: String,
        /// The Library Footprint chosen for it; left out on the Problem row of a
        /// Component with none.
        #[serde(skip_serializing_if = "Option::is_none")]
        footprint: Option<Uuid>,
        #[serde(skip_serializing_if = "Option::is_none")]
        footprint_name: Option<String>,
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
        /// The local Pad and graphics edits the swap removes, when the instance is a
        /// Locally Edited Footprint; left out when there are none.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        lost_local_edits: Vec<LocalEdit>,
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
    /// Changes the Component Type.
    ComponentType {
        component: ComponentRef,
        instance: Uuid,
        designator: String,
        from: ComponentType,
        to: ComponentType,
    },
    /// Changes In BOM.
    InBom {
        component: ComponentRef,
        instance: Uuid,
        designator: String,
        from: bool,
        to: bool,
    },
    /// Changes In CPL.
    InCpl {
        component: ComponentRef,
        instance: Uuid,
        designator: String,
        from: bool,
        to: bool,
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
    /// A Problem row: a Pin on a Net that has no Pad through the Part's pin-pad map, so
    /// its connection can't reach the board.
    UnmappedPin {
        component: ComponentRef,
        /// The Footprint instance; left out while it is still to be added.
        #[serde(skip_serializing_if = "Option::is_none")]
        instance: Option<Uuid>,
        designator: String,
        /// The Pin number.
        pin: String,
        /// The Net's board name.
        net: String,
    },
}

impl ChangeKind {
    pub fn group(&self) -> GroupKind {
        match self {
            ChangeKind::AddComponent { .. } => GroupKind::AddComponents,
            ChangeKind::RemoveComponent { .. } => GroupKind::RemoveComponents,
            ChangeKind::ChangeFootprint { .. } => GroupKind::ChangeFootprints,
            ChangeKind::Designator { .. } => GroupKind::Designators,
            ChangeKind::Fields { .. } => GroupKind::Fields,
            ChangeKind::ComponentType { .. } => GroupKind::ComponentTypes,
            ChangeKind::InBom { .. } => GroupKind::InBom,
            ChangeKind::InCpl { .. } => GroupKind::InCpl,
            ChangeKind::NotFitted { .. } => GroupKind::NotFitted,
            ChangeKind::AddNet { .. } => GroupKind::AddNets,
            ChangeKind::RenameNet { .. } => GroupKind::RenameNets,
            ChangeKind::RemoveNet { .. } => GroupKind::RemoveNets,
            ChangeKind::PadNet { .. } | ChangeKind::UnmappedPin { .. } => GroupKind::PadNets,
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
    drc_state(project).violation
}

/// What `drc.run` learns from Sync: the `out_of_sync` Violation and the pending Change
/// List's hash. Handed to [`reda_pcb::register_all`].
pub fn drc_state(project: &Project) -> reda_pcb::SyncState {
    let Some(list) = plan(project) else {
        return reda_pcb::SyncState::default();
    };
    let severity = project.pcb().map_or(Severity::Off, |pcb| {
        pcb.setup().severities.get(DrcCheck::OutOfSync)
    });
    let violation = (severity != Severity::Off && list.count > 0).then(|| {
        let count = i64::try_from(list.count).unwrap_or(i64::MAX);
        Violation::new(DrcCheck::OutOfSync.id(), severity).arg("changes", count)
    });
    reda_pcb::SyncState {
        violation,
        changes: if list.count == 0 {
            String::new()
        } else {
            list.hash
        },
    }
}

/// A schematic Net and the PCB Net Sync matches it with: what Net highlighting maps across.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetLink {
    /// The PCB Net.
    pub pcb: Uuid,
    /// The schematic Net's name.
    pub name: String,
    /// The schematic objects on the Net, on every sheet: Wires, Net Labels, Ports, Power
    /// Ports, No-Connect Markers and the Units with a Pin on it, sorted by UUID.
    pub objects: Vec<Uuid>,
}

/// Every schematic Net that matches a PCB Net, by the same matching Sync uses. Empty
/// without a PCB Document.
pub fn net_links(project: &Project) -> Vec<NetLink> {
    let Some(pcb) = project.pcb() else {
        return Vec::new();
    };
    let components = components::Components::gather(project);
    nets::links(project, pcb, &components)
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
        problem_dependants(&mut changes);
        notes::add(project, pcb, &mut changes);
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
        let duplicate_nets = DuplicateNets::of(project);
        let mut hasher = ContentHash::new();
        hasher.write(&serde_json::to_vec(&groups).expect("a Change List serialises"));
        if let Some(block) = &duplicate_nets {
            hasher.write(&serde_json::to_vec(block).expect("a block serialises"));
        }
        let list = ChangeList {
            duplicate_nets,
            hash: hasher.finish(),
            count: (groups.iter().flat_map(|g| &g.changes))
                .filter(|c| !c.is_problem())
                .count(),
            groups,
        };
        Some(Plan {
            list,
            components,
            nets,
        })
    }
}

/// Makes every change that requires a Problem row (transitively) a Problem row too.
fn problem_dependants(changes: &mut [Change]) {
    loop {
        let problems: BTreeSet<String> = (changes.iter())
            .filter(|c| c.is_problem())
            .map(|c| c.id.clone())
            .collect();
        let mut more = false;
        for change in changes.iter_mut().filter(|c| !c.is_problem()) {
            if change.requires.iter().any(|r| problems.contains(r)) {
                change.problem("Waits for a change that can't apply.".to_owned());
                more = true;
            }
        }
        if !more {
            return;
        }
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

/// The local edits on the board that an Update from Library removes from `component`,
/// whose Footprint becomes `after` (Footprint UUID and copy hash; `None` for none): those of
/// its Locally Edited Footprint, when `after` is not the copy it was edited from.
/// The local edits of PCB copy `copy` against the copy it was edited from: none when it
/// isn't Locally Edited.
pub(crate) use components::lost_local_edits as lost_local_edits_of;

pub(crate) fn lost_local_edits(
    project: &Project,
    component: Uuid,
    after: Option<(Uuid, &str)>,
) -> Vec<reda_model::local_edit::LocalEdit> {
    let Some(pcb) = project.pcb() else {
        return Vec::new();
    };
    let Some(copy) = pcb
        .footprints()
        .find(|i| {
            i.component
                .as_ref()
                .is_some_and(|c| c.path.is_empty() && c.uuid == component)
        })
        .and_then(|i| pcb.footprint_copy(i.footprint))
    else {
        return Vec::new();
    };
    let kept = after.is_some_and(|(footprint, hash)| {
        copy.footprint.uuid == footprint && copy.sync_hash() == hash
    });
    if kept {
        return Vec::new();
    }
    components::lost_local_edits(pcb, copy)
}
