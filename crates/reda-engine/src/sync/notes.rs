//! The Notes of the Change List: rows that apply, with a consequence worth reading.
//!
//! - A removed Component leaves the copper attached to its Pads in place.
//! - A removed Net leaves its copper in place, unassigned or moved to the Net it merged into.
//! - A Footprint change removes local Pad and graphics edits, and leaves the copper at the
//!   old Pads as it is.
//! - A remove and an add with the same Designator and Footprint look like a cut and paste
//!   (a likely relink): placement and routing are lost unless Move to Sheet is used.

use std::collections::{BTreeMap, BTreeSet};

use reda_model::{PcbDocument, Project, Uuid};
use reda_pcb::{Copper, Item};

use super::{Change, ChangeKind};

/// Adds the Notes to `changes`.
pub(super) fn add(project: &Project, pcb: &PcbDocument, changes: &mut [Change]) {
    relinks(pcb, changes);
    let copper = Copper::of(project);
    for change in changes.iter_mut() {
        let note = match &change.change {
            ChangeKind::RemoveComponent { instance, .. } => {
                let touching = attached(pcb, copper.as_deref(), *instance);
                (touching.any()).then(|| {
                    format!(
                        "Removing it leaves {} attached to its Pads in place.",
                        touching.describe()
                    )
                })
            }
            ChangeKind::ChangeFootprint {
                instance,
                lost_local_edits,
                ..
            } => {
                let mut parts = Vec::new();
                if !lost_local_edits.is_empty() {
                    parts.push(format!(
                        "The swap removes {}.",
                        plural(lost_local_edits.len(), "local Pad or graphics edit")
                    ));
                }
                let touching = attached(pcb, copper.as_deref(), *instance);
                if touching.any() {
                    parts.push(format!(
                        "{} at the old Pads stay as they are; DRC reports any dangling copper.",
                        capitalise(&touching.describe())
                    ));
                }
                (!parts.is_empty()).then(|| parts.join(" "))
            }
            ChangeKind::RemoveNet {
                net, merge_into, ..
            } => {
                let count = copper_on(pcb, *net);
                (count > 0).then(|| {
                    let objects = plural(count, "copper object");
                    let (stay, flag) = if count == 1 {
                        ("stays", "flags it")
                    } else {
                        ("stay", "flags them")
                    };
                    match merge_into.and_then(|m| pcb.net(m)) {
                        Some(into) => {
                            format!("Its {objects} {stay} in place, moved to Net {}.", into.name)
                        }
                        None => {
                            format!(
                                "Its {objects} {stay} in place, unassigned (no Net); DRC {flag}."
                            )
                        }
                    }
                })
            }
            _ => None,
        };
        if let Some(note) = note {
            change.note(note);
        }
    }
}

/// Notes each remove and add pair with the same Designator and Library Footprint.
fn relinks(pcb: &PcbDocument, changes: &mut [Change]) {
    let key = |change: &Change| match &change.change {
        ChangeKind::RemoveComponent {
            instance,
            designator,
            ..
        } => {
            let footprint = pcb
                .footprint(*instance)
                .and_then(|i| pcb.footprint_copy(i.footprint))
                .map(|c| c.footprint.uuid)?;
            Some((false, designator.clone(), footprint))
        }
        ChangeKind::AddComponent {
            designator,
            footprint,
            ..
        } => Some((true, designator.clone(), (*footprint)?)),
        _ => None,
    };
    let mut seen: BTreeMap<(bool, String, Uuid), usize> = BTreeMap::new();
    for change in changes.iter().filter(|c| !c.is_problem()) {
        if let Some(k) = key(change) {
            *seen.entry(k).or_default() += 1;
        }
    }
    for change in changes.iter_mut().filter(|c| !c.is_problem()) {
        let Some((added, designator, footprint)) = key(change) else {
            continue;
        };
        if seen.contains_key(&(!added, designator, footprint)) {
            change.note(
                "A remove and an add with the same Designator and Footprint: placement and \
                 routing will be lost. If this was a cut and paste, undo it and use Move to \
                 Sheet to keep them."
                    .to_owned(),
            );
        }
    }
}

/// The copper touching a Footprint instance's Pads, other than its own Pads.
#[derive(Default)]
struct Attached {
    tracks: BTreeSet<Uuid>,
    vias: BTreeSet<Uuid>,
}

impl Attached {
    fn any(&self) -> bool {
        !self.tracks.is_empty() || !self.vias.is_empty()
    }

    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.tracks.is_empty() {
            parts.push(plural(self.tracks.len(), "Track"));
        }
        if !self.vias.is_empty() {
            parts.push(plural(self.vias.len(), "Via"));
        }
        parts.join(" and ")
    }
}

fn attached(pcb: &PcbDocument, copper: Option<&Copper>, instance: Uuid) -> Attached {
    let mut out = Attached::default();
    let (Some(copper), Some(placed)) = (copper, pcb.footprint(instance)) else {
        return out;
    };
    for pad in &placed.pads {
        let item = Item::Pad {
            footprint: instance,
            pad: pad.pad,
        };
        for touching in copper.contacts(item) {
            match touching {
                Item::Track { uuid } => {
                    out.tracks.insert(uuid);
                }
                Item::Via { uuid } => {
                    out.vias.insert(uuid);
                }
                _ => {}
            }
        }
    }
    out
}

/// The copper on PCB Net `net` that a removal leaves in place: Tracks, Vias, Free Pads,
/// Zones and the Pads of Footprints Sync does not manage.
fn copper_on(pcb: &PcbDocument, net: Uuid) -> usize {
    let on = |n: Option<Uuid>| n == Some(net);
    pcb.tracks().filter(|t| on(t.net)).count()
        + pcb.vias().filter(|v| on(v.net)).count()
        + pcb.free_pads().filter(|p| on(p.net)).count()
        + pcb.zones().filter(|z| on(z.net)).count()
        + (pcb.footprints())
            .filter(|i| i.component.is_none())
            .flat_map(|i| &i.pads)
            .filter(|p| on(p.net))
            .count()
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
