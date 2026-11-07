//! Canvas interaction every editor shares, kept free of any one Document type so the
//! schematic and PCB editors pick, box-select and snap the same way:
//!
//! - [`hit_test`] and [`box_select`] run on the CPU against the Document's [`Scene`],
//!   whose items are tagged with the object they draw; the editor says which objects can
//!   be picked. A board picks through its composited Layers with [`hit_test_on`] and
//!   [`box_select_on`] (topmost Layer first, item by item).
//! - [`Combine`] turns the pointer's modifiers into how a pick changes the selection
//!   (Shift toggles), and [`combine`] applies it.
//! - [`SnapTargets`] snaps a cursor to the grid, or onto a nearby electrical target.
//! - [`pointer`] turns one frame of egui input on a canvas into [`Pointer`] events in
//!   Document coordinates, which an editor's Tools consume.

mod hit;
mod pointer;
mod snap;

pub use hit::{BoxMode, box_select, box_select_on, hit_stack_on, hit_test, hit_test_on};
pub use pointer::{Pointer, pointer};
pub use snap::{SnapTargets, Snapped, snap_to_grid, snap_to_grid_from};

use reda_engine::model::Uuid;

/// How a pick changes the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combine {
    /// The pick becomes the selection.
    Replace,
    /// The pick is added.
    Add,
    /// Each picked object is added, or dropped if it was selected (Shift).
    Toggle,
}

impl Combine {
    /// Shift toggles; otherwise the pick replaces the selection.
    pub fn of(modifiers: egui::Modifiers) -> Combine {
        if modifiers.shift {
            Combine::Toggle
        } else {
            Combine::Replace
        }
    }
}

/// A click on the only selected object deselects it. Drags use [`combine`] instead.
pub fn click_selection(current: &[Uuid], picked: &[Uuid], how: Combine) -> Vec<Uuid> {
    if how == Combine::Replace && current.len() == 1 && current == picked {
        Vec::new()
    } else {
        combine(current, picked, how)
    }
}

/// The selection `current` after picking `picked`, in pick order without repeats.
pub fn combine(current: &[Uuid], picked: &[Uuid], how: Combine) -> Vec<Uuid> {
    let mut out: Vec<Uuid> = match how {
        Combine::Replace => Vec::new(),
        Combine::Add | Combine::Toggle => current.to_vec(),
    };
    for &uuid in picked {
        match out.iter().position(|o| *o == uuid) {
            Some(i) if how == Combine::Toggle => {
                out.remove(i);
            }
            Some(_) => {}
            None => out.push(uuid),
        }
    }
    out
}
