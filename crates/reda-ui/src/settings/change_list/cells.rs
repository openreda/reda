//! What a Change List row shows in each column: Kind, Target, From → To and Status, and
//! the one-line description the row's tick box is announced by.

use reda_engine::model::{Fields, Uuid};
use reda_engine::sync::{Change, ChangeKind, GroupKind, NetRef, Status};

use crate::tr;

/// A row's columns, after the tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cells {
    pub kind: String,
    pub target: String,
    pub from_to: String,
    pub status: String,
}

/// The columns of `change`; `applied` once Apply carried it out.
pub fn cells(change: &Change, pcb_nets: &[(Uuid, String)], applied: bool) -> Cells {
    let (target, from_to) = target_and_from_to(&change.change, pcb_nets);
    let message = change.message.as_deref().unwrap_or("");
    let status = if applied {
        tr!("change-list-status-applied")
    } else {
        match change.status {
            Status::Ok => tr!("change-list-status-ok"),
            Status::Note => tr!("change-list-note", message = message),
            Status::Problem => tr!("change-list-problem", message = message),
        }
    };
    Cells {
        kind: kind_name(&change.change),
        target,
        from_to,
        status,
    }
}

fn arrow(from: &str, to: &str) -> String {
    tr!("change-list-from-to", from = from, to = to)
}

fn yes_no(on: bool) -> String {
    if on {
        tr!("change-list-yes")
    } else {
        tr!("change-list-no")
    }
}

fn target_and_from_to(change: &ChangeKind, pcb_nets: &[(Uuid, String)]) -> (String, String) {
    let none = tr!("change-list-none");
    match change {
        ChangeKind::AddComponent {
            designator,
            footprint_name,
            ..
        } => (
            designator.clone(),
            arrow(&none, footprint_name.as_deref().unwrap_or(&none)),
        ),
        ChangeKind::RemoveComponent { designator, .. } => (designator.clone(), String::new()),
        ChangeKind::ChangeFootprint {
            designator,
            from,
            to,
            ..
        } => (designator.clone(), arrow(&from.name, &to.name)),
        ChangeKind::Designator { from, to, .. } => (from.clone(), arrow(from, to)),
        ChangeKind::Fields {
            designator,
            from,
            to,
            ..
        } => {
            let changes: Vec<String> = changed_fields(from, to)
                .iter()
                .map(|n| {
                    tr!(
                        "change-list-field",
                        field = n.as_str(),
                        change = arrow(from.get(n), to.get(n))
                    )
                })
                .collect();
            (designator.clone(), changes.join("; "))
        }
        ChangeKind::ComponentType {
            designator,
            from,
            to,
            ..
        } => (
            designator.clone(),
            arrow(
                &crate::panels::library::type_name(*from),
                &crate::panels::library::type_name(*to),
            ),
        ),
        ChangeKind::InBom {
            designator,
            from,
            to,
            ..
        }
        | ChangeKind::InCpl {
            designator,
            from,
            to,
            ..
        }
        | ChangeKind::NotFitted {
            designator,
            from,
            to,
            ..
        } => (designator.clone(), arrow(&yes_no(*from), &yes_no(*to))),
        ChangeKind::AddNet { name } => (name.clone(), String::new()),
        ChangeKind::RenameNet { from, to, .. } => (from.clone(), arrow(from, to)),
        ChangeKind::RemoveNet {
            name, merge_into, ..
        } => match merge_into.and_then(|m| pcb_nets.iter().find(|(u, _)| *u == m)) {
            Some((_, into)) => (name.clone(), arrow(name, into)),
            None => (name.clone(), String::new()),
        },
        ChangeKind::PadNet {
            designator,
            number,
            from,
            to,
            ..
        } => (
            format!("{designator}.{number}"),
            arrow(&net_name(from.as_ref()), &net_name(to.as_ref())),
        ),
        ChangeKind::UnmappedPin {
            designator,
            pin,
            net,
            ..
        } => (
            tr!(
                "change-list-pin",
                designator = designator.as_str(),
                pin = pin.as_str()
            ),
            net.clone(),
        ),
    }
}

fn kind_name(change: &ChangeKind) -> String {
    match change {
        ChangeKind::AddComponent { .. } => tr!("change-kind-add-component"),
        ChangeKind::RemoveComponent { .. } => tr!("change-kind-remove-component"),
        ChangeKind::ChangeFootprint { .. } => tr!("change-kind-change-footprint"),
        ChangeKind::Designator { .. } => tr!("change-kind-designator"),
        ChangeKind::Fields { .. } => tr!("change-kind-fields"),
        ChangeKind::ComponentType { .. } => tr!("change-kind-component-type"),
        ChangeKind::InBom { .. } => tr!("change-kind-in-bom"),
        ChangeKind::InCpl { .. } => tr!("change-kind-in-cpl"),
        ChangeKind::NotFitted { .. } => tr!("change-kind-not-fitted"),
        ChangeKind::AddNet { .. } => tr!("change-kind-add-net"),
        ChangeKind::RenameNet { .. } => tr!("change-kind-rename-net"),
        ChangeKind::RemoveNet { .. } => tr!("change-kind-remove-net"),
        ChangeKind::PadNet { .. } => tr!("change-kind-pad-net"),
        ChangeKind::UnmappedPin { .. } => tr!("change-kind-unmapped-pin"),
    }
}

pub fn group_title(group: GroupKind) -> String {
    match group {
        GroupKind::AddComponents => tr!("change-list-group-add-components"),
        GroupKind::RemoveComponents => tr!("change-list-group-remove-components"),
        GroupKind::ChangeFootprints => tr!("change-list-group-change-footprints"),
        GroupKind::Designators => tr!("change-list-group-designators"),
        GroupKind::Fields => tr!("change-list-group-fields"),
        GroupKind::ComponentTypes => tr!("change-list-group-component-types"),
        GroupKind::InBom => tr!("change-list-group-in-bom"),
        GroupKind::InCpl => tr!("change-list-group-in-cpl"),
        GroupKind::NotFitted => tr!("change-list-group-not-fitted"),
        GroupKind::AddNets => tr!("change-list-group-add-nets"),
        GroupKind::RenameNets => tr!("change-list-group-rename-nets"),
        GroupKind::RemoveNets => tr!("change-list-group-remove-nets"),
        GroupKind::PadNets => tr!("change-list-group-pad-nets"),
    }
}

/// One line describing `change`.
pub fn describe(change: &Change, pcb_nets: &[(Uuid, String)]) -> String {
    match &change.change {
        ChangeKind::AddComponent {
            designator,
            footprint_name,
            ..
        } => match footprint_name {
            Some(footprint) => tr!(
                "change-add-component",
                designator = designator.as_str(),
                footprint = footprint.as_str()
            ),
            None => tr!(
                "change-add-component-no-footprint",
                designator = designator.as_str()
            ),
        },
        ChangeKind::RemoveComponent { designator, .. } => {
            tr!("change-remove-component", designator = designator.as_str())
        }
        ChangeKind::ChangeFootprint {
            designator,
            from,
            to,
            ..
        } => tr!(
            "change-footprint",
            designator = designator.as_str(),
            from = from.name.as_str(),
            to = to.name.as_str()
        ),
        ChangeKind::Designator { from, to, .. } => {
            tr!("change-designator", from = from.as_str(), to = to.as_str())
        }
        ChangeKind::Fields {
            designator,
            from,
            to,
            ..
        } => tr!(
            "change-fields",
            designator = designator.as_str(),
            fields = changed_fields(from, to).join(", ")
        ),
        ChangeKind::ComponentType {
            designator,
            from,
            to,
            ..
        } => tr!(
            "change-component-type",
            designator = designator.as_str(),
            from = crate::panels::library::type_name(*from),
            to = crate::panels::library::type_name(*to)
        ),
        ChangeKind::InBom { designator, to, .. } => {
            if *to {
                tr!("change-in-bom-on", designator = designator.as_str())
            } else {
                tr!("change-in-bom-off", designator = designator.as_str())
            }
        }
        ChangeKind::InCpl { designator, to, .. } => {
            if *to {
                tr!("change-in-cpl-on", designator = designator.as_str())
            } else {
                tr!("change-in-cpl-off", designator = designator.as_str())
            }
        }
        ChangeKind::NotFitted { designator, to, .. } => {
            if *to {
                tr!("change-not-fitted-on", designator = designator.as_str())
            } else {
                tr!("change-not-fitted-off", designator = designator.as_str())
            }
        }
        ChangeKind::AddNet { name } => tr!("change-add-net", net = name.as_str()),
        ChangeKind::RenameNet { from, to, .. } => {
            tr!("change-rename-net", from = from.as_str(), to = to.as_str())
        }
        ChangeKind::RemoveNet {
            name, merge_into, ..
        } => match merge_into.and_then(|m| pcb_nets.iter().find(|(u, _)| *u == m)) {
            Some((_, into)) => tr!(
                "change-merge-net",
                net = name.as_str(),
                into = into.as_str()
            ),
            None => tr!("change-remove-net", net = name.as_str()),
        },
        ChangeKind::PadNet {
            designator,
            number,
            from,
            to,
            ..
        } => tr!(
            "change-pad-net",
            designator = designator.as_str(),
            pad = number.as_str(),
            from = net_name(from.as_ref()),
            to = net_name(to.as_ref())
        ),
        ChangeKind::UnmappedPin {
            designator,
            pin,
            net,
            ..
        } => tr!(
            "change-unmapped-pin",
            designator = designator.as_str(),
            pin = pin.as_str(),
            net = net.as_str()
        ),
    }
}

fn net_name(net: Option<&NetRef>) -> String {
    match net {
        Some(net) => net.name.clone(),
        None => tr!("change-no-net"),
    }
}

/// The names of the fields that differ, built-in ones first.
fn changed_fields(from: &Fields, to: &Fields) -> Vec<String> {
    let mut names: Vec<String> = Fields::BUILT_IN.iter().map(|n| (*n).to_owned()).collect();
    for field in from.custom.iter().chain(&to.custom) {
        if !names.contains(&field.name) {
            names.push(field.name.clone());
        }
    }
    names
        .into_iter()
        .filter(|n| from.get(n) != to.get(n))
        .collect()
}
