//! The PCB editor: the board's view state and its Commands (Layers, units, snap grid),
//! electrical snap, and the select-and-place Tool that drives the canvas.
//!
//! - [`view`]: [`BoardView`], the Layer state kept in `view.json`, units and grid.
//! - `commands`: the view Commands the Layers panel and the view keys run.

mod commands;
pub mod view;

pub use commands::register_all;
pub use view::BoardView;
