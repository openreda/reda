//! Drag and drop, as the model sees it: where a dragged tab can land, the one layout
//! Command a drop there runs, and a dry run that says before the release what it would do.

use serde_json::{Value, json};

use super::{DockSide, GroupId, Layout, SplitSide, TabKey};

/// Where a dragged tab can land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropTarget {
    /// Into a group's tab strip, before the tab at `slot` among its tabs as they are now
    /// (the dragged tab included); `None` puts it last.
    Group { group: GroupId, slot: Option<usize> },
    /// Into a new group on `side` of `group`.
    Split { group: GroupId, side: SplitSide },
    /// After the tabs of the Dock at this window edge.
    Dock(DockSide),
}

/// What a drop would do, found before the release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropOutcome {
    /// The layout would change.
    Moves,
    /// The tab would stay where it is, so the drop runs nothing.
    Unchanged,
    /// The layout would refuse the drop, for this reason.
    Refused(String),
}

/// A drop: the layout Command it runs and what that would do.
#[derive(Clone, Debug, PartialEq)]
pub struct DropPlan {
    pub command: &'static str,
    pub params: Value,
    pub outcome: DropOutcome,
}

impl Layout {
    /// Plans dropping `tab` on `target`: the layout Command to run on release, and a dry
    /// run of it on a copy of the layout.
    pub fn plan_drop(&self, tab: &TabKey, target: DropTarget) -> DropPlan {
        let name = tab.to_string();
        let mut copy = self.clone();
        let (command, params, result) = match target {
            DropTarget::Group { group, slot } => {
                // The slot counts the dragged tab if it is in this group already; the
                // Command's index counts the tabs without it.
                let index = slot.map(|slot| match self.place(tab) {
                    Some(p) if p.group == group && p.index < slot => slot - 1,
                    _ => slot,
                });
                let mut params = json!({"tab": name, "group": group});
                if let Some(index) = index {
                    params["index"] = json!(index);
                }
                let result = copy.move_to_group(tab, group, index).map(drop);
                ("layout.tab.move_to_group", params, result)
            }
            DropTarget::Split { group, side } => (
                "layout.tab.split",
                json!({"tab": name, "group": group, "side": side}),
                copy.split_beside(tab, Some(group), side).map(drop),
            ),
            DropTarget::Dock(dock) => (
                "layout.tab.move_to_dock",
                json!({"tab": name, "dock": dock}),
                copy.move_to_dock(tab, dock).map(drop),
            ),
        };
        let outcome = match result {
            Err(e) => DropOutcome::Refused(e.message),
            Ok(()) if copy.same_arrangement(self) => DropOutcome::Unchanged,
            Ok(()) => DropOutcome::Moves,
        };
        DropPlan {
            command,
            params,
            outcome,
        }
    }
}
