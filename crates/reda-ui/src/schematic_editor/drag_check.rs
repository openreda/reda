//! Display When Dragging (docs/spec/v0.1/schematic.md#editing): during a drag or move,
//! the Junctions the drop would make and a chip by the cursor comparing the Nets at the
//! current snapped position, OK or an alert naming the change ("joins GND and VCC"). It
//! never blocks the drop.

use std::collections::{BTreeMap, BTreeSet};

use reda_engine::model::geom::Point;
use reda_engine::model::{Object, Project, Uuid};
use reda_engine::schematic::connectivity::{project_nets, sheet_connectivity};

use crate::tr;

/// What a drag in flight would do to connectivity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DragCheck {
    /// The Junctions on the sheet after the drop that are not there now.
    pub junctions: Vec<Point>,
    /// The Net change, as the chip says it; `None` is OK.
    pub change: Option<String>,
}

impl DragCheck {
    /// The chip's text: OK, or the change.
    pub fn chip(&self) -> String {
        self.change.clone().unwrap_or_else(|| tr!("drag-ok"))
    }
}

/// Compares `before` with `after` (the Project as the drop would leave it) on `document`.
pub fn check(before: &Project, after: &Project, document: Uuid) -> DragCheck {
    let now: BTreeSet<Point> = sheet_connectivity(before, document)
        .map(|c| c.junctions.into_iter().collect())
        .unwrap_or_default();
    let junctions = sheet_connectivity(after, document)
        .map(|c| c.junctions)
        .unwrap_or_default()
        .into_iter()
        .filter(|j| !now.contains(j))
        .collect();
    DragCheck {
        junctions,
        change: change(before, after),
    }
}

/// What a Net member is, for comparing Nets before and after: a Pin of a Unit, or a
/// Wire, label, Port or Power Port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Member {
    Pin(Uuid, Uuid),
    Object(Uuid),
}

/// Each Net's name (`None` for one without) and members in `project`.
fn members(project: &Project) -> Vec<(Option<String>, BTreeSet<Member>)> {
    project_nets(project)
        .into_iter()
        .map(|net| {
            let name = net.name;
            let mut members: BTreeSet<Member> = (net.pins.iter())
                .map(|p| Member::Pin(p.pin.unit, p.pin.pin))
                .collect();
            members.extend(
                (net.objects.iter())
                    .filter(|o| !matches!(project.object(**o), Some(Object::Unit(_))))
                    .map(|o| Member::Object(*o)),
            );
            (name, members)
        })
        .collect()
}

/// "joins A and B" when members of two Nets now share one, else "splits A" when a Net's
/// members now sit on two; `None` when the Nets stay as they are.
fn change(before: &Project, after: &Project) -> Option<String> {
    let old = members(before);
    let mut was: BTreeMap<Member, &str> = BTreeMap::new();
    for (name, set) in &old {
        let Some(name) = name else { continue };
        for m in set {
            was.insert(*m, name);
        }
    }
    let new = members(after);
    for (_, set) in &new {
        let names: BTreeSet<&str> = set.iter().filter_map(|m| was.get(m).copied()).collect();
        if names.len() >= 2 {
            let mut names: Vec<&str> = names.into_iter().collect();
            names.sort_by(|a, b| natural(a, b));
            let last = names.pop().expect("two or more");
            return Some(tr!(
                "drag-joins",
                nets = names.join(", "),
                last = last.to_owned()
            ));
        }
    }
    let mut now: BTreeMap<Member, usize> = BTreeMap::new();
    for (i, (_, set)) in new.iter().enumerate() {
        for m in set {
            now.insert(*m, i);
        }
    }
    for (name, set) in old.iter().filter_map(|(n, s)| Some((n.as_ref()?, s))) {
        let parts: BTreeSet<usize> = set.iter().filter_map(|m| now.get(m).copied()).collect();
        if parts.len() >= 2 {
            return Some(tr!("drag-splits", net = name.clone()));
        }
    }
    None
}

fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}
