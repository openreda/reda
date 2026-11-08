//! Write-through: a schematic edit made from the PCB side, carried to the board at once.
//!
//! The edit runs first, in the open transaction. The Sync changes it caused (those the
//! Change List did not hold, word for word, before it) are then applied in the same
//! transaction, with every change they require. Changes that were already pending stay
//! pending, so a write-through never brings anything else across: in particular it never
//! adds or removes a Footprint, even when the edit renamed a Component still waiting to
//! be added.

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{CommandError, Context};

use super::{ChangeKind, Plan, apply};

/// Runs `edit`, then applies the Sync changes it caused. Without a PCB Document only the
/// edit runs. The registry records one undo entry per Document touched, so the schematic
/// and the board each undo their own half.
pub fn write_through<T>(
    cx: &mut Context<'_>,
    edit: impl FnOnce(&mut Context<'_>) -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    let before = Plan::new(cx.project()?).map(|p| p.list);
    let out = edit(cx)?;
    let (Some(before), Some(after)) = (before, Plan::new(cx.project()?)) else {
        return Ok(out);
    };
    let by_id: BTreeMap<&str, _> = after.list.changes().map(|c| (c.id.as_str(), c)).collect();
    let mut chosen: BTreeSet<&str> = after
        .list
        .changes()
        .filter(|c| {
            // A status that changed alone does not make a change new.
            caused_directly(&c.change)
                && !(before.changes()).any(|b| b.change == c.change && b.requires == c.requires)
        })
        .map(|c| c.id.as_str())
        .collect();
    // Bring along what the chosen changes require.
    let mut todo: Vec<&str> = chosen.iter().copied().collect();
    while let Some(id) = todo.pop() {
        for req in &by_id[id].requires {
            if chosen.insert(req.as_str()) {
                todo.push(req.as_str());
            }
        }
    }
    if chosen.is_empty() {
        return Ok(out);
    }
    let left_out: BTreeSet<String> = after
        .list
        .changes()
        .filter(|c| !chosen.contains(c.id.as_str()))
        .map(|c| c.id.clone())
        .collect();
    // Problem rows never apply, nor what requires them.
    let left_out = after.list.cascade(&left_out);
    apply::carry_out(cx, &after, &left_out)?;
    Ok(out)
}

/// Whether a new change of this kind is carried by itself. Adding or removing Footprints
/// (and the Pads of a Footprint still to be added) stays with Sync, and a Net is added
/// only when a carried change needs it.
fn caused_directly(change: &ChangeKind) -> bool {
    !matches!(
        change,
        ChangeKind::AddComponent { .. }
            | ChangeKind::RemoveComponent { .. }
            | ChangeKind::AddNet { .. }
            | ChangeKind::PadNet { instance: None, .. }
    )
}
