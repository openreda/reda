//! The PCB editor: the board's view state and its Commands (Layers, units, snap grid),
//! electrical snap, and the select-and-place Tool that drives the canvas.
//!
//! - [`view`]: [`BoardView`], the Layer state kept in `view.json`, units and grid.
//! - `commands`: the view Commands the Layers panel and the view keys run;
//!   [`view_commands`] adds Layer sets, pinned Layers, per-kind display, Net colours on
//!   copper and the Net label toggles.
//! - Snapping is the shared [`crate::snap`] model.
//! - [`canvas`]: the board canvas in the window: pointer and keys, overlays, the
//!   "No Board Outline" hint.
//! - [`tool`]: the select-and-place [`Tool`]: click, rubber band, drag to move, Space and
//!   L, with the move drawn from a preview until release, and Track segment drags.
//! - [`route`]: the routing and Via Tools ([`Router`]); `route_commands` holds their keys
//!   and select connected.
//! - [`draw`]: the drawing Tools (outline, cutouts, Zones, Keepouts, text and graphics,
//!   Free Pads, origins, Mechanical Parts, measure), previewed until one Command commits.

pub mod canvas;
mod commands;
pub mod draw;
pub mod layer_control;
pub mod live_hover;
pub mod route;
pub mod route_commands;
pub mod routing_bar;
mod select_commands;
pub mod silkscreen;
pub mod tool;
pub mod view;
pub mod view_commands;

pub use route::Router;
pub use tool::SelectTool;

/// Adds the PCB view Commands, the routing Tool's and the drawing Tools' Commands to
/// `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut reda_engine::command::Registry) {
    commands::register_all(registry);
    view_commands::register_all(registry);
    route_commands::register_all(registry);
    select_commands::register_all(registry);
}
pub use view::BoardView;
