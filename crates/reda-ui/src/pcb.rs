//! The PCB editor: the board's view state and its Commands (Layers, units, snap grid),
//! electrical snap, and the select-and-place Tool that drives the canvas.
//!
//! - [`view`]: [`BoardView`], the Layer state kept in `view.json`, units and grid.
//! - `commands`: the view Commands the Layers panel and the view keys run.
//! - [`snap`]: electrical and grid snap.
//! - [`tool`]: the select-and-place [`Tool`]: click, rubber band, drag to move, Space and
//!   L, with the move drawn from a preview until release.

mod commands;
pub mod snap;
pub mod tool;
pub mod view;

pub use commands::register_all;
pub use tool::Tool;
pub use view::BoardView;
