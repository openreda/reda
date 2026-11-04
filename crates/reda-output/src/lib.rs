//! Manufacturing outputs: the Fab Presets REDA ships, the Output Job Commands, and the
//! writers a job run uses: Gerber X2 ([`gerber_files`]), Excellon drill ([`drill_files`]),
//! the CSV BOM and CPL ([`bom_file`], [`cpl_file`]) and the zip ([`zip`]).
//!
//! `output.job.run` itself reads several Documents (Sync, ERC), so it lives in the engine.

mod archive;
mod assembly;
mod civil;
mod drill;
mod gerber;
mod jobs;
mod plot;
mod presets;

pub use archive::zip;
pub use assembly::{bom_file, cpl_file, cpl_origin};
pub use drill::drill_files;
pub use jobs::{job_destination, new_job, register_all};
pub use plot::{OutputFile, Plot, gerber_files};
pub use presets::{GENERIC_NAME, generic, shipped, shipped_named};
