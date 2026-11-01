//! The engine's own Commands: Project files and, later, cross-Document work.

pub mod preferences;
pub mod project;

use reda_command::Registry;

/// Adds every Command defined in `reda-engine`.
pub fn register_all(registry: &mut Registry) {
    preferences::register_all(registry);
    project::register_all(registry);
}
