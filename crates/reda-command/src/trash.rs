//! Where deleted Documents' files go on save.
//!
//! The engine moves no file to the OS trash itself, so it stays free of OS code: the front
//! end hands it a [`Trash`] (the CLI and the app pass the OS trash). Without one, a deleted
//! Document's file is left on disk.

use std::fmt;
use std::path::Path;

/// Moves a file somewhere the user can get it back from.
pub trait Trash: fmt::Debug + Send + Sync {
    /// Moves the file at `path` to the trash. The file is gone from `path` on success.
    fn trash(&self, path: &Path) -> std::io::Result<()>;
}
