//! REDA's user interface: the window shell, the Start page, the schematic canvas, the
//! theme and the UI string catalogue.
//!
//! The UI reads `&Project` directly to draw and changes it only by running Commands on
//! the [`Engine`](reda_engine::Engine), through [`Shell::run`](shell::Shell::run).
//! `reda-app` builds the engine (with the OS trash and the machine's fonts) and hands it to
//! [`run`].

mod agents;
mod app;
pub mod bars;
pub mod bridge;
pub mod canvas;
pub mod command_dialog;
pub mod crash;
pub mod cross_probe;
mod dialogs;
mod docking;
pub mod element_editor;
pub mod footprint_editor;
pub mod grid;
pub mod help;
pub mod i18n;
pub mod icons;
pub mod job_run;
mod journal_panel;
pub mod keymap;
pub mod layout;
pub mod messages;
mod new_project;
mod os;
pub mod palette;
mod panels;
pub mod pcb;
pub mod placement;
pub mod preferences_dialog;
mod projects_panel;
pub mod properties;
pub mod recovery;
mod recovery_dialog;
pub mod schematic_editor;
pub mod selection;
pub mod settings;
pub mod shell;
mod start_page;
pub mod theme;
mod tool_preview;
pub mod tools;
mod user_preferences;
pub mod view_state;

pub use app::{App, install_canvas, run};
pub use docking::windows;
/// The control port, re-exported for tests that act as agents.
pub use reda_control as control;
/// The engine, re-exported so the app depends only on the UI.
pub use reda_engine as engine;
