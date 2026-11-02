//! REDA's user interface: the window shell, the Start page, the schematic canvas, the
//! theme and the UI string catalogue.
//!
//! The UI reads `&Project` directly to draw and changes it only by running Commands on
//! the [`Engine`](reda_engine::Engine), through [`Shell::run`](shell::Shell::run).
//! `reda-app` builds the engine (with the OS trash and the machine's fonts) and hands it to
//! [`run`].

mod app;
pub mod bars;
pub mod canvas;
pub mod crash;
mod dialogs;
mod docking;
pub mod i18n;
pub mod icons;
mod journal_panel;
pub mod layout;
mod new_project;
mod os;
mod panels;
mod projects_panel;
pub mod properties;
pub mod recovery;
mod recovery_dialog;
pub mod selection;
pub mod shell;
mod start_page;
pub mod theme;

pub use app::{App, install_canvas, run};
/// The engine, re-exported so the app depends only on the UI.
pub use reda_engine as engine;
