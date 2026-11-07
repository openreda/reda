//! Libraries: finding Symbols, Footprints and Parts in registered Library folders,
//! searching them, and the library Commands, including the editors' edit Commands
//! ([`edit`]), and the Footprint generators ([`generate`]).
//!
//! A Library is a folder holding `<name>.redalib` and one file per element under
//! `symbols/`, `footprints/` and `parts/`. Nothing lists the elements: [`scan`] finds them
//! by file extension. It reaches the files only through the [`LibraryFiles`] seam, so this
//! crate never touches the disk and builds for `wasm32`; the engine supplies the
//! implementation that reads and decodes real files.

mod check;
mod commands;
mod copy;
pub mod edit;
mod extract;
pub mod generate;
mod index;
mod scan;
mod search;
mod where_used;
pub mod wizard;

pub use check::{Elements, PIN_GRID, check};
pub use commands::{
    LibraryPathParams, LibraryPathResult, ProjectLibraryAdd, ProjectLibraryRemove,
    normalise_library_path, register_all,
};
pub use copy::{copy_element, free_name};
pub use edit::check_element_name;
pub use extract::{ExtractError, ExtractPlan, plan_extract};
pub use generate::{FootprintStore, GeneratorHost, SeveritiesOf, register_generators};
pub use index::{Library, LibraryIndex, Origin, ScanProblem};
pub use scan::{ElementKind, LIBRARY_EXTENSION, LibraryFiles, LibraryRoot, scan};
pub use search::{SearchHit, SearchQuery};
pub use where_used::{ComponentUse, components_using, project_element};
