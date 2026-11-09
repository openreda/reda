//! What a Change List row has on each side of Sync, for **Show**: the Footprint instance
//! or the PCB Net's copper on the board, and the Component's Units or the schematic Net's
//! objects on the sheets.

use reda_engine::model::net_name::same_name;
use reda_engine::model::{DocumentKind, Project, Uuid};
use reda_engine::schematic::connectivity::project_nets;
use reda_engine::sync::{Change, ChangeKind};

use crate::show_on_board::Sides;

/// The objects `change` concerns on the board and on the sheets.
pub fn sides(project: &Project, change: &Change) -> Sides {
    let pcb = project.pcb();
    let on_board = |uuid: Uuid| pcb.and_then(|p| p.footprint(uuid)).map(|f| f.uuid);
    let net_copper = |net: Uuid| {
        pcb.map(|p| crate::cross_probe::pcb_objects_of(p, net))
            .unwrap_or_default()
    };
    match &change.change {
        ChangeKind::AddComponent { component, .. } => Sides {
            board: Vec::new(),
            sheet: units(project, component.uuid),
        },
        ChangeKind::RemoveComponent { instance, .. } => Sides {
            board: on_board(*instance).into_iter().collect(),
            sheet: None,
        },
        ChangeKind::ChangeFootprint {
            component,
            instance,
            ..
        }
        | ChangeKind::Designator {
            component,
            instance,
            ..
        }
        | ChangeKind::Fields {
            component,
            instance,
            ..
        }
        | ChangeKind::ComponentType {
            component,
            instance,
            ..
        }
        | ChangeKind::InBom {
            component,
            instance,
            ..
        }
        | ChangeKind::InCpl {
            component,
            instance,
            ..
        }
        | ChangeKind::NotFitted {
            component,
            instance,
            ..
        } => Sides {
            board: on_board(*instance).into_iter().collect(),
            sheet: units(project, component.uuid),
        },
        ChangeKind::PadNet {
            component,
            instance,
            ..
        }
        | ChangeKind::UnmappedPin {
            component,
            instance,
            ..
        } => Sides {
            board: instance.and_then(on_board).into_iter().collect(),
            sheet: units(project, component.uuid),
        },
        ChangeKind::AddNet { name } => Sides {
            board: Vec::new(),
            sheet: schematic_net(project, name),
        },
        ChangeKind::RenameNet { net, to, .. } => Sides {
            board: net_copper(*net),
            sheet: schematic_net(project, to),
        },
        ChangeKind::RemoveNet { net, .. } => Sides {
            board: net_copper(*net),
            sheet: None,
        },
    }
}

/// Every Unit of Component `component`, and the sheet of the first.
fn units(project: &Project, component: Uuid) -> Option<(Uuid, Vec<Uuid>)> {
    let mut sheet = None;
    let mut out = Vec::new();
    for document in project.documents() {
        if document.kind != DocumentKind::Schematic {
            continue;
        }
        let Some(content) = project.schematic_sheet(document.uuid) else {
            continue;
        };
        for unit in content.units().filter(|u| u.component.uuid == component) {
            sheet.get_or_insert(document.uuid);
            out.push(unit.uuid);
        }
    }
    Some((sheet?, out))
}

/// The objects of the schematic Net the board calls `name`, and its first sheet.
fn schematic_net(project: &Project, name: &str) -> Option<(Uuid, Vec<Uuid>)> {
    let mut sheet = None;
    let mut out: Vec<Uuid> = Vec::new();
    for net in project_nets(project) {
        if net
            .board_name
            .as_deref()
            .is_some_and(|n| same_name(n, name))
        {
            if sheet.is_none() {
                sheet = net.sheets.first().copied();
            }
            out.extend(net.objects);
        }
    }
    Some((sheet?, out))
}
