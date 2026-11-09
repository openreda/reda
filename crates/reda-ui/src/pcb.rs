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
//!   L, with the move drawn from a preview until release; [`copper_drag`] holds its
//!   copper drag gestures (slide, Track end and corner, break, Via and Free Pad).
//! - [`placing`]: Footprint moves' modes, the Reposition Selected queue, Choose
//!   Component and the locked-move question; [`placement_bar`] shows and switches them.
//! - [`route`]: the routing and Via Tools ([`Router`]); `route_commands` holds their keys.
//! - `select_commands`: the PCB selection Commands, select connected (`U`) and the copper
//!   selection cycle (`Tab`).
//! - [`draw`]: the drawing Tools (outline, cutouts, Zones, Keepouts, text and graphics,
//!   Free Pads, origins, Mechanical Parts, measure), previewed until one Command commits.

pub mod board_shapes;
pub mod canvas;
mod commands;
pub mod copper_drag;
pub mod delete;
pub mod dimension_edit;
pub mod draw;
pub mod layer_control;
pub mod live_hover;
pub mod measure;
pub mod placement_bar;
pub mod placing;
pub mod polygon_edit;
pub mod route;
pub mod route_commands;
pub mod routing_bar;
mod select_commands;
pub mod silkscreen;
pub mod tool;
pub mod update_dialog;
pub mod view;
pub mod view_commands;

pub use route::Router;
pub use select_commands::CopperCycle;
pub use tool::SelectTool;

/// Adds the PCB view Commands, the routing Tool's and the drawing Tools' Commands to
/// `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut reda_engine::command::Registry) {
    commands::register_all(registry);
    view_commands::register_all(registry);
    route_commands::register_all(registry);
    placing::register_all(registry);
    select_commands::register_all(registry);
    copper_drag::register_all(registry);
    delete::register_all(registry);
}
pub use view::BoardView;
