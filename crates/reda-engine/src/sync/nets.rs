//! Net matching: each derived schematic Net matches the PCB Net it shares the most Pads
//! with, the name breaking ties; unmatched schematic Nets become new PCB Nets, unmatched
//! PCB Nets are removed, and every Pad whose Net differs gets a Pad Net change.
//!
//! A schematic Net becomes a PCB Net when it reaches a Pad and either reaches two Pads or
//! carries a name the user gave it (a Net Label, Port or Power Port). A lone unwired Pin
//! leaves its Pad on no Net.

use std::collections::{BTreeMap, BTreeSet};

use reda_model::net_name::{name_key, same_name};
use reda_model::{ComponentRef, PcbDocument, Project, Uuid};
use reda_schematic::connectivity::{NameSource, project_nets};

/// The board names that several named Nets share, by name key: the duplicates Sync merges
/// and `duplicate_nets` blocks on.
pub(crate) fn shared_names(project: &Project) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut shared = BTreeSet::new();
    for net in project_nets(project) {
        if matches!(net.name_source, None | Some(NameSource::AutoName)) {
            continue;
        }
        if let Some(name) = net.board_name {
            let key = name_key(&name);
            if !seen.insert(key.clone()) {
                shared.insert(key);
            }
        }
    }
    shared
}

use super::components::{Components, linked_instances};
use super::{Change, ChangeKind, NetLink, NetRef, Status, change_id, component_key};

/// A Pad of a Component by number: the key both sides share.
type PadKey = (ComponentRef, String);

/// Where a Pad Net change moves a Pad.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// An existing PCB Net.
    Existing(Uuid),
    /// The Net that the `add_net` change with this id adds.
    New(String),
}

/// One schematic Net that Sync carries to the PCB.
struct SchNet {
    /// Whether a Net Label, Port or Power Port names it (not an auto-name).
    named: bool,
    name: String,
    pads: BTreeSet<PadKey>,
    /// The PCB Net it matches, if any.
    matched: Option<Uuid>,
    /// The id of its `add_net` change, when it matches none.
    add_id: String,
    /// Its project-wide Nets, by index into `project_nets`: more than one when Nets with
    /// one board name merged.
    indices: Vec<usize>,
}

/// A Pin on a Net that Sync would carry, with no Pad through its Part's pin-pad map.
struct Unmapped {
    component: ComponentRef,
    designator: String,
    number: String,
    net: String,
}

/// The planned Net and Pad Net changes, with what applying them needs.
pub(crate) struct NetPlan {
    pub(crate) changes: Vec<Change>,
    /// The target of each `pad_net` change, by change id (`None` = no Net).
    pub(crate) pad_targets: BTreeMap<String, Option<Target>>,
}

impl NetPlan {
    pub(crate) fn new(project: &Project, pcb: &PcbDocument, components: &Components) -> NetPlan {
        let (mut nets, unmapped) = schematic_nets(project, components);
        let pcb_pads = pcb_pad_nets(pcb);
        match_nets(&mut nets, pcb, &pcb_pads);
        let mut by_pad: BTreeMap<&PadKey, usize> = BTreeMap::new();
        for (i, net) in nets.iter().enumerate() {
            for pad in &net.pads {
                by_pad.entry(pad).or_insert(i);
            }
        }

        let mut changes = Vec::new();
        for net in nets.iter().filter(|n| n.matched.is_none()) {
            changes.push(Change {
                id: net.add_id.clone(),
                requires: vec![],
                status: Status::Ok,
                message: None,
                change: ChangeKind::AddNet {
                    name: net.name.clone(),
                },
            });
        }
        for net in &nets {
            let Some(uuid) = net.matched else { continue };
            let current = pcb.net(uuid).expect("a matched Net exists");
            if current.name != net.name {
                changes.push(Change {
                    id: change_id("rename_net", &[&uuid.to_string()]),
                    requires: vec![],
                    status: Status::Ok,
                    message: None,
                    change: ChangeKind::RenameNet {
                        net: uuid,
                        from: current.name.clone(),
                        to: net.name.clone(),
                    },
                });
            }
        }

        // Pad Net changes, and what removing each PCB Net has to wait for.
        let linked = linked_instances(pcb);
        let mut pad_changes = Vec::new();
        let mut pad_targets = BTreeMap::new();
        // (instance, Pad) → the change that takes the Pad off its current Net.
        let mut leaves: BTreeMap<(Uuid, Uuid), String> = BTreeMap::new();
        for c in components.in_sheet_order() {
            let key = component_key(&c.key);
            let instance = linked.get(&c.key).copied();
            let swapped = instance.is_some_and(|i| {
                pcb.footprint_copy(i.footprint)
                    .is_some_and(|copy| c.footprint_differs(copy))
            });
            let mut pads: Vec<_> = c.copy.footprint.pads.iter().collect();
            pads.sort_by_key(|p| p.uuid);
            for pad in pads {
                let target =
                    by_pad.get(&(c.key.clone(), pad.number.clone())).map(|&i| {
                        match nets[i].matched {
                            Some(uuid) => Target::Existing(uuid),
                            None => Target::New(nets[i].add_id.clone()),
                        }
                    });
                let current = match instance {
                    Some(i) if !swapped => i.pad_net(pad.uuid),
                    _ => None,
                };
                if target == current.map(Target::Existing) {
                    continue;
                }
                let id = change_id("pad_net", &[&key, &pad.uuid.to_string()]);
                let mut requires = vec![];
                match instance {
                    None => requires.push(change_id("add_component", &[&key])),
                    Some(_) if swapped => requires.push(change_id("change_footprint", &[&key])),
                    Some(i) => {
                        leaves.insert((i.uuid, pad.uuid), id.clone());
                    }
                }
                let to = target.as_ref().map(|t| match t {
                    Target::Existing(uuid) => NetRef {
                        net: Some(*uuid),
                        name: nets[by_pad[&(c.key.clone(), pad.number.clone())]]
                            .name
                            .clone(),
                    },
                    Target::New(add) => {
                        requires.push(add.clone());
                        NetRef {
                            net: None,
                            name: nets
                                .iter()
                                .find(|n| n.add_id == *add)
                                .expect("an added Net")
                                .name
                                .clone(),
                        }
                    }
                });
                let from = current.map(|uuid| NetRef {
                    net: Some(uuid),
                    name: pcb.net(uuid).map(|n| n.name.clone()).unwrap_or_default(),
                });
                pad_changes.push(Change {
                    id: id.clone(),
                    requires,
                    status: Status::Ok,
                    message: None,
                    change: ChangeKind::PadNet {
                        component: c.key.clone(),
                        instance: instance.map(|i| i.uuid),
                        designator: c.designator.clone(),
                        pad: pad.uuid,
                        number: pad.number.clone(),
                        from,
                        to: to.clone(),
                    },
                });
                pad_targets.insert(id, target);
            }
        }

        let matched: BTreeSet<Uuid> = nets.iter().filter_map(|n| n.matched).collect();
        for net in pcb.nets().filter(|n| !matched.contains(&n.uuid)) {
            let mut requires = BTreeSet::new();
            for (key, instance) in &linked {
                let ckey = component_key(key);
                let Some(copy) = pcb.footprint_copy(instance.footprint) else {
                    continue;
                };
                for pad in instance.pads.iter().filter(|p| p.net == Some(net.uuid)) {
                    let c = components.get(key);
                    let blocker = match c {
                        None => change_id("remove_component", &[&ckey]),
                        Some(c) if c.footprint_differs(copy) => {
                            change_id("change_footprint", &[&ckey])
                        }
                        Some(_) => match leaves.get(&(instance.uuid, pad.pad)) {
                            Some(id) => id.clone(),
                            // A Pad the schematic's Footprint lacks cannot happen while
                            // the copies agree; nothing to wait for.
                            None => continue,
                        },
                    };
                    requires.insert(blocker);
                }
            }
            changes.push(Change {
                id: change_id("remove_net", &[&net.uuid.to_string()]),
                requires: requires.into_iter().collect(),
                status: Status::Ok,
                message: None,
                change: ChangeKind::RemoveNet {
                    net: net.uuid,
                    name: net.name.clone(),
                    merge_into: merge_target(&nets, &pcb_pads, net.uuid),
                },
            });
        }
        changes.extend(pad_changes);
        for pin in unmapped {
            let mut change = Change {
                id: change_id(
                    "unmapped_pin",
                    &[&component_key(&pin.component), &pin.number],
                ),
                requires: vec![],
                status: Status::Ok,
                message: None,
                change: ChangeKind::UnmappedPin {
                    instance: linked.get(&pin.component).map(|i| i.uuid),
                    component: pin.component,
                    designator: pin.designator.clone(),
                    pin: pin.number.clone(),
                    net: pin.net.clone(),
                },
            };
            change.problem(format!(
                "Pin {} of {} is on Net {} but has no Pad through its Part's pin-pad map; \
                 map it to a Pad in the Library.",
                pin.number, pin.designator, pin.net
            ));
            changes.push(change);
        }
        NetPlan {
            changes,
            pad_targets,
        }
    }
}

/// The schematic Nets that become PCB Nets, in `schematic.project_nets` order, and the
/// Pins of those Nets that have no Pad.
fn schematic_nets(project: &Project, components: &Components) -> (Vec<SchNet>, Vec<Unmapped>) {
    let mut out: Vec<SchNet> = Vec::new();
    let mut unmapped = Vec::new();
    for (index, net) in project_nets(project).into_iter().enumerate() {
        let mut pads = BTreeSet::new();
        let mut padless = Vec::new();
        for pin in &net.pins {
            let Some(c) = components.get(&pin.pin.component) else {
                continue;
            };
            let mut found = false;
            for number in c.pin_map.pads_of(&pin.pin.number) {
                if c.copy.footprint.pads.iter().any(|p| p.number == number) {
                    pads.insert((c.key.clone(), number.to_owned()));
                    found = true;
                }
            }
            if !found {
                padless.push(&pin.pin);
            }
        }
        let named = !matches!(net.name_source, None | Some(NameSource::AutoName));
        let name = net.board_name.unwrap_or_default();
        // A Net worth carrying, had every Pin a Pad.
        if !padless.is_empty() && (pads.len() + padless.len() >= 2 || !net.names.is_empty()) {
            unmapped.extend(padless.into_iter().map(|pin| Unmapped {
                component: pin.component.clone(),
                designator: pin.designator.clone(),
                number: pin.number.clone(),
                net: name.clone(),
            }));
        }
        if pads.is_empty() || (pads.len() < 2 && net.names.is_empty()) {
            continue;
        }
        // Named Nets with one board name are one board Net (`duplicate_nets` blocks the
        // apply unless it is Off or waived). Auto-names only collide through duplicate
        // Designators, which don't block Sync: those Nets stay apart.
        if named && let Some(same) = (out.iter_mut()).find(|n| n.named && same_name(&n.name, &name))
        {
            same.pads.extend(pads);
            same.indices.push(index);
            continue;
        }
        out.push(SchNet {
            named,
            name,
            pads,
            matched: None,
            add_id: String::new(),
            indices: vec![index],
        });
    }
    for net in &mut out {
        let (first, number) = net.pads.first().expect("a Net with Pads");
        net.add_id = change_id("add_net", &[&net.name, &component_key(first), number]);
    }
    (out, unmapped)
}

/// Every schematic Net Sync carries to the board that a PCB Net already matches, with
/// the objects on it.
pub(crate) fn links(project: &Project, pcb: &PcbDocument, components: &Components) -> Vec<NetLink> {
    let (mut nets, _) = schematic_nets(project, components);
    match_nets(&mut nets, pcb, &pcb_pad_nets(pcb));
    if nets.iter().all(|n| n.matched.is_none()) {
        return Vec::new();
    }
    let mut project_nets = project_nets(project);
    nets.into_iter()
        .filter_map(|net| {
            let pcb = net.matched?;
            let mut objects: Vec<Uuid> = (net.indices.iter())
                .flat_map(|&i| std::mem::take(&mut project_nets[i].objects))
                .collect();
            objects.sort();
            Some(NetLink {
                pcb,
                objects,
                name: net.name,
            })
        })
        .collect()
}

/// The current PCB Nets of every linked Footprint instance's Pads, by Pad key.
fn pcb_pad_nets(pcb: &PcbDocument) -> BTreeMap<PadKey, BTreeSet<Uuid>> {
    let mut out: BTreeMap<PadKey, BTreeSet<Uuid>> = BTreeMap::new();
    for (key, instance) in linked_instances(pcb) {
        let Some(copy) = pcb.footprint_copy(instance.footprint) else {
            continue;
        };
        for pad in &copy.footprint.pads {
            if let Some(net) = instance.pad_net(pad.uuid) {
                out.entry((key.clone(), pad.number.clone()))
                    .or_default()
                    .insert(net);
            }
        }
    }
    out
}

/// How many of schematic Net `net`'s Pads are on PCB Net `uuid`.
fn overlap(net: &SchNet, pcb_pads: &BTreeMap<PadKey, BTreeSet<Uuid>>, uuid: Uuid) -> usize {
    net.pads
        .iter()
        .filter(|k| pcb_pads.get(*k).is_some_and(|n| n.contains(&uuid)))
        .count()
}

/// Matches each schematic Net to at most one PCB Net and back: the largest overlaps first,
/// a matching name breaking ties, then Nets with no shared Pad by name alone.
fn match_nets(nets: &mut [SchNet], pcb: &PcbDocument, pcb_pads: &BTreeMap<PadKey, BTreeSet<Uuid>>) {
    let mut candidates = Vec::new();
    for (i, net) in nets.iter().enumerate() {
        let touched: BTreeSet<Uuid> = net
            .pads
            .iter()
            .filter_map(|k| pcb_pads.get(k))
            .flatten()
            .copied()
            .collect();
        for uuid in touched {
            let Some(pcb_net) = pcb.net(uuid) else {
                continue;
            };
            let shared = overlap(net, pcb_pads, uuid);
            let other_name = !same_name(&pcb_net.name, &net.name);
            candidates.push((std::cmp::Reverse(shared), other_name, i, uuid));
        }
    }
    candidates.sort();
    let mut used = BTreeSet::new();
    for (_, _, i, uuid) in candidates {
        if nets[i].matched.is_none() && !used.contains(&uuid) {
            nets[i].matched = Some(uuid);
            used.insert(uuid);
        }
    }
    for net in nets.iter_mut().filter(|n| n.matched.is_none()) {
        if let Some(pcb_net) = pcb
            .nets()
            .find(|p| !used.contains(&p.uuid) && same_name(&p.name, &net.name))
        {
            net.matched = Some(pcb_net.uuid);
            used.insert(pcb_net.uuid);
        }
    }
}

/// The PCB Net a removed Net's Pads went to: the match of the schematic Net that took the
/// most of them.
fn merge_target(
    nets: &[SchNet],
    pcb_pads: &BTreeMap<PadKey, BTreeSet<Uuid>>,
    removed: Uuid,
) -> Option<Uuid> {
    nets.iter()
        .filter(|n| n.matched.is_some())
        .map(|n| (overlap(n, pcb_pads, removed), n))
        .filter(|(shared, _)| *shared > 0)
        .min_by_key(|(shared, _)| std::cmp::Reverse(*shared))
        .and_then(|(_, n)| n.matched)
}
