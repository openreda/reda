//! The headless entry point: Project open/save, cross-Document Commands, registry assembly.
//!
//! Front ends (the CLI, MCP, the GUI) build an [`Engine`] and drive it only through
//! [`Engine::run`]. Tests build one with `reda_testkit::test_engine()`.

mod builders;
pub mod canonical;
pub mod commands;
mod engine;
pub mod format;
pub mod fs;
pub mod store;

pub use engine::{Engine, EngineBuilder, registry};

/// The Command system, re-exported so front ends depend only on the engine.
pub use reda_command as command;
/// The design-data model, re-exported so front ends depend only on the engine.
pub use reda_model as model;
