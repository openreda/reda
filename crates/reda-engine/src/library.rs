//! The registered Libraries on disk: which folders they are, and reading them for
//! `reda-library`'s scan.

use std::io::ErrorKind;
use std::path::Path;

use reda_command::Session;
use reda_library::{LibraryFiles, LibraryIndex, LibraryRoot, Origin, scan};
use reda_model::{Footprint, LibraryInfo, Part, Symbol};

use crate::format::library::{read_footprint, read_library_info, read_part, read_symbol};
use crate::preferences;

/// Library files read from disk and decoded with the file format.
pub struct DiskLibraryFiles;

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("could not read the file: {e}"))
}

impl LibraryFiles for DiskLibraryFiles {
    fn list(&mut self, dir: &Path) -> Result<Vec<String>, String> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(format!("could not list the folder: {e}")),
        };
        let mut names = vec![];
        for entry in entries {
            let entry = entry.map_err(|e| format!("could not list the folder: {e}"))?;
            let is_file = entry.file_type().is_ok_and(|t| t.is_file());
            if let (true, Some(name)) = (is_file, entry.file_name().to_str()) {
                names.push(name.to_owned());
            }
        }
        Ok(names)
    }

    fn read_library(&mut self, path: &Path) -> Result<LibraryInfo, String> {
        read_library_info(path, &read(path)?).map_err(|e| reason(path, e))
    }

    fn read_symbol(&mut self, path: &Path) -> Result<Symbol, String> {
        read_symbol(path, &read(path)?).map_err(|e| reason(path, e))
    }

    fn read_footprint(&mut self, path: &Path) -> Result<Footprint, String> {
        read_footprint(path, &read(path)?).map_err(|e| reason(path, e))
    }

    fn read_part(&mut self, path: &Path) -> Result<Part, String> {
        read_part(path, &read(path)?).map_err(|e| reason(path, e))
    }
}

/// The error without its leading path, which the scan problem already carries.
fn reason(path: &Path, err: crate::format::LoadError) -> String {
    let text = err.to_string();
    let prefix = format!("{}: ", path.display());
    text.strip_prefix(&prefix)
        .map(str::to_owned)
        .unwrap_or(text)
}

/// The registered Library folders, in scan order: the starter Library, the user's from
/// Preferences, then the open Project's from its `.reda` file. A Project not yet given a
/// folder has no Project Libraries to find.
pub fn registered_roots(session: &Session) -> Vec<LibraryRoot> {
    let mut roots = vec![];
    if let Some(starter) = &session.starter_library {
        roots.push(LibraryRoot::new(starter, Origin::Starter));
    }
    let prefs = preferences::current(session.user_dirs.as_ref());
    for path in prefs.libraries.paths {
        roots.push(LibraryRoot::new(path, Origin::User));
    }
    if let (Some(project), Some(folder)) = (&session.project, &session.location) {
        for path in &project.props().libraries {
            roots.push(LibraryRoot::new(folder.join(path), Origin::Project));
        }
    }
    roots
}

/// Scans every registered Library from disk.
pub fn scan_registered(session: &Session) -> LibraryIndex {
    scan(&registered_roots(session), &mut DiskLibraryFiles)
}
