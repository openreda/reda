//! Cross-select and Net highlighting between the schematic and the PCB: what the canvases
//! mark as highlighted (everything else dims) because of what is selected on the other
//! side. Both are view state: never undone, never saved in a Document.
//!
//! - **Cross-select** mirrors by Component UUID: a selected Unit highlights its Footprint
//!   instance, a selected Footprint every Unit of its Component, whatever sheet it is on.
//!   Un-synced Components have nothing on the other side.
//! - **Net highlighting** maps through Sync's Net matching ([`sync::net_links`]): a
//!   highlighted PCB Net lights up the schematic objects on its schematic Net, and a
//!   selected schematic Net object (a Wire, Net Label, Port or Power Port) lights up its
//!   PCB Net. A PCB Net shows as its copper (Tracks, Vias, Zones, Free Pads) and the
//!   Footprints with a Pad on it.
//!
//! [`marks`] is pure; other cross-probe features (Find, Select Similar) can use
//! [`pcb_net_of`] and [`schematic_objects_of`] for the same mapping.

use std::collections::BTreeSet;

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::{DocumentKind, Object, Project, Uuid};
use reda_engine::sync::{self, NetLink};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::selection::Selection;

/// Which cross-probes are on. Both are by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrossProbe {
    /// Selection mirrors between Units and Footprints.
    pub selection: bool,
    /// Net highlighting maps between schematic Nets and PCB Nets.
    pub nets: bool,
}

impl Default for CrossProbe {
    fn default() -> CrossProbe {
        CrossProbe {
            selection: true,
            nets: true,
        }
    }
}

/// What the canvases highlight because of `selection`, sorted by UUID.
pub fn marks(project: &Project, selection: &Selection, on: CrossProbe) -> Vec<Uuid> {
    let mut out = BTreeSet::new();
    if on.selection {
        for uuid in &selection.objects {
            match project.object(*uuid) {
                Some(Object::Unit(unit)) => {
                    let component = &unit.object.component;
                    out.extend(
                        project
                            .pcb()
                            .into_iter()
                            .flat_map(|pcb| pcb.footprints())
                            .filter(|f| f.component.as_ref() == Some(component))
                            .map(|f| f.uuid),
                    );
                }
                Some(Object::FootprintInstance(footprint)) => {
                    if let Some(component) = &footprint.object.component {
                        out.extend(units_of(project, component.uuid));
                    }
                }
                _ => {}
            }
        }
    }
    if on.nets {
        let picks_schematic_net = selection.objects.iter().any(|uuid| {
            matches!(
                project.object(*uuid),
                Some(
                    Object::Wire(_) | Object::NetLabel(_) | Object::Port(_) | Object::PowerPort(_)
                )
            )
        });
        if !selection.nets.is_empty() || picks_schematic_net {
            let links = sync::net_links(project);
            let mut pcb_nets: BTreeSet<Uuid> = selection.nets.iter().copied().collect();
            for net in &selection.nets {
                out.extend(schematic_objects_in(&links, *net));
            }
            for uuid in &selection.objects {
                pcb_nets.extend(pcb_net_in(&links, *uuid));
            }
            for net in pcb_nets {
                out.extend(pcb_objects_of(project, net));
            }
        }
    }
    // What is selected is marked as selected already.
    for uuid in &selection.objects {
        out.remove(uuid);
    }
    out.into_iter().collect()
}

/// Every Unit of Component `component`, on every sheet.
fn units_of(project: &Project, component: Uuid) -> Vec<Uuid> {
    project
        .documents()
        .filter(|d| d.kind == DocumentKind::Schematic)
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|sheet| sheet.units())
        .filter(|u| u.component.uuid == component)
        .map(|u| u.uuid)
        .collect()
}

/// The PCB Net that schematic object `object` (a Wire, Net Label, Port or Power Port) is
/// on, through Sync's matching; `None` when its Net is not on the board yet.
pub fn pcb_net_of(project: &Project, object: Uuid) -> Option<Uuid> {
    pcb_net_in(&sync::net_links(project), object)
}

/// The schematic objects on the schematic Net that PCB Net `net` matches, on every sheet.
pub fn schematic_objects_of(project: &Project, net: Uuid) -> Vec<Uuid> {
    schematic_objects_in(&sync::net_links(project), net).collect()
}

fn pcb_net_in(links: &[NetLink], object: Uuid) -> Option<Uuid> {
    links
        .iter()
        .find(|l| l.objects.binary_search(&object).is_ok())
        .map(|l| l.pcb)
}

fn schematic_objects_in(links: &[NetLink], net: Uuid) -> impl Iterator<Item = Uuid> + '_ {
    links
        .iter()
        .filter(move |l| l.pcb == net)
        .flat_map(|l| l.objects.iter().copied())
}

/// What draws PCB Net `net`: its copper, and the Footprints with a Pad on it.
fn pcb_objects_of(project: &Project, net: Uuid) -> Vec<Uuid> {
    let Some(pcb) = project.pcb() else {
        return Vec::new();
    };
    let on = Some(net);
    pcb.tracks()
        .filter(|t| t.net == on)
        .map(|t| t.uuid)
        .chain(pcb.vias().filter(|v| v.net == on).map(|v| v.uuid))
        .chain(pcb.zones().filter(|z| z.net == on).map(|z| z.uuid))
        .chain(pcb.free_pads().filter(|p| p.net == on).map(|p| p.uuid))
        .chain(
            pcb.footprints()
                .filter(|f| f.pads.iter().any(|p| p.net == on))
                .map(|f| f.uuid),
        )
        .collect()
}

/// Adds the view Commands that switch the cross-probes. The Shell does this.
pub fn register_all(registry: &mut Registry) {
    registry.add(SelectionToggle);
    registry.add(NetsToggle);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToggleParams {
    /// On or off; it flips when left out.
    #[serde(default)]
    on: Option<bool>,
}

/// Whether the cross-probe is now on.
#[derive(Serialize, JsonSchema)]
pub struct ToggleResult {
    pub on: bool,
}

/// Sets (or flips) one switch: Net cross-probing when `nets`, else cross-select.
fn toggle(cx: &mut Context<'_>, p: ToggleParams, nets: bool) -> Result<ToggleResult, CommandError> {
    let state = cx.view::<CrossProbe>()?;
    let flag = if nets {
        &mut state.nets
    } else {
        &mut state.selection
    };
    *flag = p.on.unwrap_or(!*flag);
    Ok(ToggleResult { on: *flag })
}

struct SelectionToggle;

impl Command for SelectionToggle {
    const META: Meta = Meta {
        id: "view.cross_select.toggle",
        title: "Cross-Select",
        description: "Switches cross-select on or off: selecting a Unit highlights its \
                      Footprint on the board, and selecting a Footprint highlights every \
                      Unit of its Component on the sheets. On by default; view state only.",
        category: "view",
        icon: "select",
        kind: Kind::View,
    };
    type Params = ToggleParams;
    type Output = ToggleResult;

    fn run(&self, cx: &mut Context<'_>, p: ToggleParams) -> Result<ToggleResult, CommandError> {
        toggle(cx, p, false)
    }
}

struct NetsToggle;

impl Command for NetsToggle {
    const META: Meta = Meta {
        id: "view.net_cross_probe.toggle",
        title: "Cross-Probe Nets",
        description: "Switches Net cross-probing on or off: a highlighted PCB Net lights up \
                      its schematic Net on every sheet, and a selected schematic Net lights \
                      up its PCB Net, matched as Sync matches them. On by default; view \
                      state only.",
        category: "view",
        icon: "net",
        kind: Kind::View,
    };
    type Params = ToggleParams;
    type Output = ToggleResult;

    fn run(&self, cx: &mut Context<'_>, p: ToggleParams) -> Result<ToggleResult, CommandError> {
        toggle(cx, p, true)
    }
}
