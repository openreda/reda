//! Manufacturing outputs: the Fab Presets REDA ships and the Output Job Commands.
//!
//! The Gerber, Excellon, BOM and CPL writers and `output.job.run` join here in later
//! tickets.

mod jobs;
mod presets;

pub use jobs::{new_job, register_all};
pub use presets::{GENERIC_NAME, generic, shipped, shipped_named};
