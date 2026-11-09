//! Manufacturing outputs: the Fab Presets REDA ships, the Output Job Commands, and the
//! writers a job run uses: Gerber X2 ([`gerber_files`]), Excellon drill ([`drill_files`]),
//! the CSV BOM and CPL ([`bom_file`], [`cpl_file`]), the files that go to the fab beside
//! them (Fab Notes [`fab_notes_file`], the drill report [`drill_report_file`] and the
//! Gerber job file [`gerber_job_file`]) and the zip ([`zip`]).
//!
//! `output.job.run` itself reads several Documents (Sync, ERC), so it lives in the engine.

mod archive;
mod assembly;
mod drill;
mod drill_report;
mod fab_notes;
mod gerber;
mod instance;
mod job_file;
mod jobs;
mod plot;
mod presets;

pub use archive::zip;
pub use assembly::{bom_file, cpl_file};
pub use drill::{Drills, drill_file_names, drill_files};
pub use drill_report::{drill_report_file, drill_report_name};
pub use fab_notes::{fab_notes_file, fab_notes_name};
pub use gerber::MAX_COORDINATE;
pub use instance::{BoardInstance, InstanceTransform, resolve_origin};
pub use job_file::{gerber_job_file, gerber_job_name};
pub use jobs::{job_destination, new_job, register_all};
pub use plot::{
    CoordinateOutOfRange, FileAttributes, Gerbers, OutputFile, Part, Plot, SkippedFile,
    gerber_files,
};
pub use presets::{GENERIC_NAME, generic, shipped, shipped_named};
