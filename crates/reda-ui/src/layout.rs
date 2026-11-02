//! REDA's docking layer (ADR-0011), as a model with no drawing: tab groups, splits, Docks
//! at the window edges and floating windows.
//!
//! The [`Layout`] lives in the engine's view state, so it changes only through the layout
//! Commands in [`commands`] (`layout.tab.*`, `layout.dock.collapse`, `panel.show`), which
//! the menus, keys, scripts and MCP all run alike. Each action works on a copy that is
//! tidied and checked as a whole ([`Layout::check`]) before it replaces the layout, so a
//! refused action leaves the layout as it was.

mod commands;
mod drop;
pub mod focus;
mod model;
pub mod saved;

pub use commands::{register_all, register_with_layouts};
pub use drop::{DropOutcome, DropPlan, DropTarget};
pub use model::{
    Area, Axis, Dock, DockSide, FloatingWindow, Group, GroupId, Layout, Node, PanelId, Place,
    SplitSide, TabKey, WindowId, WindowRect,
};
pub use saved::{SavedLayout, Slot};
