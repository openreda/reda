//! The OS trash (the Recycle Bin on Windows), where `project.save` sends the files of
//! deleted Documents. The front ends (the app and the CLI) hand it to their engines; the
//! engine itself has no OS code.

use std::path::Path;

use reda_engine::command::Trash;

/// Moves files to the OS trash through the `trash` crate: the freedesktop.org trash on
/// Linux and the BSDs, the Finder's Trash on macOS, the Recycle Bin on Windows.
#[derive(Debug)]
pub struct OsTrash;

impl Trash for OsTrash {
    fn trash(&self, path: &Path) -> std::io::Result<()> {
        if !path.exists() {
            return Err(std::io::ErrorKind::NotFound.into());
        }
        trash::delete(path).map_err(std::io::Error::other)
    }
}
