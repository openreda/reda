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
pub mod edit;
pub mod generate;
mod index;
mod scan;
mod search;
pub mod wizard;

pub use check::{Elements, PIN_GRID, check};
pub use commands::{
    LibraryPathParams, LibraryPathResult, ProjectLibraryAdd, ProjectLibraryRemove,
    normalise_library_path, register_all,
};
pub use edit::check_element_name;
pub use generate::{FootprintStore, register_generators};
pub use index::{Library, LibraryIndex, Origin, ScanProblem};
pub use scan::{ElementKind, LIBRARY_EXTENSION, LibraryFiles, LibraryRoot, scan};
pub use search::{SearchHit, SearchQuery};
