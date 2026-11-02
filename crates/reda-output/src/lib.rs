//! Manufacturing outputs: the Fab Presets REDA ships, the Output Job Commands, and the
//! writers a job run uses: Gerber X2 ([`gerber_files`]) and the zip ([`zip`]).
//!
//! `output.job.run` itself reads several Documents (Sync, ERC), so it lives in the engine;
//! the Excellon, BOM and CPL writers join here in later tickets.

mod archive;
mod civil;
mod gerber;
mod jobs;
mod plot;
mod presets;

pub use archive::zip;
pub use jobs::{new_job, register_all};
pub use plot::{OutputFile, Plot, gerber_files};
pub use presets::{GENERIC_NAME, generic, shipped, shipped_named};
