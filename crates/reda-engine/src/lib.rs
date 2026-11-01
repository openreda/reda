//! The headless entry point: Project open/save, cross-Document Commands, registry assembly.

pub mod canonical;
pub mod fs;

/// The Command system, re-exported so front ends depend only on the engine.
pub use reda_command as command;
/// The design-data model, re-exported so front ends depend only on the engine.
pub use reda_model as model;
