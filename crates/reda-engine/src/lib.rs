//! The headless entry point: Project open/save, cross-Document Commands, registry assembly.
//!
//! Front ends (the CLI, MCP, the GUI) build an [`Engine`] and drive it only through
//! [`Engine::run`]. Tests build one with `reda_testkit::test_engine()`.

pub mod annotate;
mod builders;
pub mod canonical;
pub mod commands;
mod engine;
pub mod fab_presets;
pub mod format;
pub mod fs;
pub mod journal;
pub mod library;
pub mod preferences;
pub mod starter;
pub mod state;
pub mod store;
pub mod sync;
pub mod user_dirs;

pub use engine::{Engine, EngineBuilder, document_scene, registry};
pub use journal::{JournalEntry, Source};

/// The Command system, re-exported so front ends depend only on the engine.
pub use reda_command as command;
/// The design-data model, re-exported so front ends depend only on the engine.
pub use reda_model as model;
/// The PCB domain (text geometry, missing fonts), re-exported for the UI.
pub use reda_pcb as pcb;
/// Scenes and the canvas renderer, re-exported so the app draws with the engine's copy.
pub use reda_render as render;
/// The schematic domain (connectivity, Pin positions), re-exported for the UI.
pub use reda_schematic as schematic;
