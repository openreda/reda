//! The PCB editor: the board's view state and its Commands (Layers, units, snap grid),
//! electrical snap, and the select-and-place Tool that drives the canvas.
//!
//! - [`view`]: [`BoardView`], the Layer state kept in `view.json`, units and grid.
//! - `commands`: the view Commands the Layers panel and the view keys run.
//! - [`snap`]: electrical and grid snap.
//! - [`canvas`]: the board canvas in the window: pointer and keys, overlays, the
//!   "No Board Outline" hint.
//! - [`tool`]: the select-and-place [`Tool`]: click, rubber band, drag to move, Space and
//!   L, with the move drawn from a preview until release.
//! - [`draw`]: the drawing Tools (outline, cutouts, Zones, Keepouts, text and graphics,
//!   Free Pads, origins, Mechanical Parts, measure), previewed until one Command commits.

pub mod canvas;
mod commands;
pub mod draw;
pub mod snap;
pub mod tool;
pub mod view;

/// Adds the PCB view Commands to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut reda_engine::command::Registry) {
    commands::register_all(registry);
    draw::register_all(registry);
}

pub use tool::Tool;
pub use view::BoardView;
