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
mod instance;
mod jobs;
mod plot;
mod presets;

pub use archive::zip;
pub use assembly::{bom_file, cpl_file};
pub use drill::drill_files;
pub use gerber::MAX_COORDINATE;
pub use instance::{BoardInstance, InstanceTransform, resolve_origin};
pub use jobs::{job_destination, new_job, register_all};
pub use plot::{CoordinateOutOfRange, Gerbers, OutputFile, Part, Plot, SkippedFile, gerber_files};
pub use presets::{GENERIC_NAME, generic, shipped, shipped_named};
