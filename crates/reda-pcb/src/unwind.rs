//! Unwinding routed copper (docs/spec/v0.1/pcb.md#selecting-and-clearing-copper): Backspace
//! deletes the one selected end object and selects its neighbour, passing through Vias
//! and stopping at a Pad; `Mod`+Del deletes both ways along the chain up to Pads and
//! branches.

use std::collections::BTreeSet;

use reda_model::{PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::Serialize;

use crate::copper::{Copper, Item};

/// What an unwind deletes and what it selects next.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Unwound {
    /// The Tracks and Vias deleted, sorted.
    pub deleted: Vec<Uuid>,
    /// The Track or Via to select next; left out where the unwind reached a Pad, a
    /// branch or the end of the copper.
    pub next: Option<Uuid>,
}

/// Unwinds from the Track or Via `object`. One step deletes it and hands on to its one
/// neighbour when that is a Track or Via; with `both`, it goes on deleting along the
/// chain each way while a piece has no other neighbour left, so it stops at Pads, Free
/// Pads, Zones and branches.
pub fn unwind(
    pcb: &PcbDocument,
    copper: &Copper,
    object: Uuid,
    both: bool,
) -> Result<Unwound, String> {
    let start =
        routed_item(pcb, object).ok_or_else(|| format!("{object} is not a Track or a Via"))?;
    let mut deleted = BTreeSet::from([start]);
    let first = neighbours(copper, start);
    if !both {
        let next = match first.as_slice() {
            [one] if is_routed(one) => Some(one.object()),
            _ => None,
        };
        return Ok(Unwound {
            deleted: vec![object],
            next,
        });
    }
    let mut todo: Vec<(Item, Vec<Item>)> = joints(pcb, copper, start)
        .into_iter()
        .map(|joint| (start, joint))
        .collect();
    while let Some((from, joint)) = todo.pop() {
        let [next] = joint[..] else {
            continue;
        };
        if !is_routed(&next) || !deleted.insert(next) {
            continue;
        }
        for joint in joints(pcb, copper, next) {
            if !joint.contains(&from) {
                todo.push((next, joint));
            }
        }
    }
    Ok(Unwound {
        deleted: deleted.into_iter().map(|i| i.object()).collect(),
        next: None,
    })
}

/// The joints of `item`, each as the other copper meeting it there: a Track's two
/// ends, and for a Via each piece touching it (a Via passes straight through).
fn joints(pcb: &PcbDocument, copper: &Copper, item: Item) -> Vec<Vec<Item>> {
    match item {
        Item::Track { uuid } => {
            let Some(track) = pcb.track(uuid) else {
                return Vec::new();
            };
            let near = neighbours(copper, item);
            let (a, b) = track.geometry.ends();
            [a, b]
                .into_iter()
                .map(|end| {
                    copper
                        .items_at(track.layer, end)
                        .into_iter()
                        .filter(|i| *i != item && (near.contains(i) || is_stop(i)))
                        .collect()
                })
                .collect()
        }
        _ => {
            let near = neighbours(copper, item);
            if near.len() == 2 {
                near.into_iter().map(|n| vec![n]).collect()
            } else {
                // A Via meeting more than two pieces is a branch: nothing passes it.
                vec![near]
            }
        }
    }
}

fn routed_item(pcb: &PcbDocument, object: Uuid) -> Option<Item> {
    if pcb.track(object).is_some() {
        Some(Item::Track { uuid: object })
    } else if pcb.via(object).is_some() {
        Some(Item::Via { uuid: object })
    } else {
        None
    }
}

fn is_routed(item: &Item) -> bool {
    matches!(item, Item::Track { .. } | Item::Via { .. })
}

fn is_stop(item: &Item) -> bool {
    matches!(
        item,
        Item::Pad { .. } | Item::FreePad { .. } | Item::Zone { .. }
    )
}

/// What `item` touches, counting copper that meets it only on a shared Pad, Free Pad or
/// Zone as meeting there rather than as a neighbour.
fn neighbours(copper: &Copper, item: Item) -> Vec<Item> {
    let stops: BTreeSet<Item> = copper.contacts(item).filter(is_stop).collect();
    copper
        .contacts(item)
        .filter(|n| *n != item)
        .filter(|n| {
            is_stop(n) || {
                let theirs: BTreeSet<Item> = copper.contacts(*n).filter(is_stop).collect();
                stops.is_disjoint(&theirs)
            }
        })
        .collect()
}
