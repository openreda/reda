//! The Library seam: how an edit Command gets a Part to copy into a Document.
//!
//! Commands run against the Project, but the registered Libraries live on disk and are
//! read with the file format, both of which only the engine knows. So the engine injects a
//! [`PartSource`] into every run, and a Command such as `schematic.component.place` asks it
//! for a ready [`PartCopy`].

use reda_model::{Footprint, Part, Symbol, Uuid};

use crate::CommandError;

/// A Library Part with everything a Document needs to hold it on its own: its Symbol and
/// its alternate Footprints, each with a content hash of what was copied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartCopy {
    /// The content hash of the Part with its Symbol and Footprints.
    pub hash: String,
    pub part: Part,
    /// `None` for a Mechanical Part without one.
    pub symbol: Option<Symbol>,
    /// In the order of `part.footprints`.
    pub footprints: Vec<FootprintCopyData>,
}

/// One alternate Footprint of a [`PartCopy`], with its own content hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootprintCopyData {
    pub hash: String,
    pub footprint: Footprint,
}

/// Where Parts come from: the registered Libraries.
pub trait PartSource {
    /// The Part with this UUID from whichever registered Library holds it, or
    /// `not_found` when none does or an element it names is missing.
    fn part(&self, uuid: Uuid) -> Result<PartCopy, CommandError>;
}

/// A [`PartSource`] with no Libraries, for runs that have none.
pub struct NoLibraries;

impl PartSource for NoLibraries {
    fn part(&self, uuid: Uuid) -> Result<PartCopy, CommandError> {
        Err(CommandError::not_found(format!(
            "no registered Library holds Part {uuid}; no Libraries are available"
        )))
    }
}
