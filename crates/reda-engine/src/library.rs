//! The registered Libraries on disk: which folders they are, and reading them for
//! `reda-library`'s scan, and writing generated Footprints into them.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use reda_command::{CommandError, FootprintCopyData, PartCopy, PartSource, Session, UserDirs};
use reda_library::{ElementKind, LibraryFiles, LibraryIndex, LibraryRoot, Origin, scan};
use reda_model::{Footprint, LibraryInfo, Part, Symbol, Uuid};

use crate::format::library::{
    read_footprint, read_library_info, read_part, read_symbol, write_footprint, write_part,
    write_symbol,
};
use crate::fs::write_atomic;
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
    DiskParts::new(session).roots()
}

/// Scans every registered Library from disk.
pub fn scan_registered(session: &Session) -> LibraryIndex {
    scan(&registered_roots(session), &mut DiskLibraryFiles)
}

/// Writes a generated Footprint to `<library>/footprints/<name>.redafp` in a registered
/// Library: the [`reda_library::FootprintStore`] the generators write through.
pub fn store_footprint(
    session: &mut Session,
    library: Uuid,
    footprint: &Footprint,
) -> Result<PathBuf, CommandError> {
    let index = scan_registered(session);
    let library = index.library(library).ok_or_else(|| {
        CommandError::not_found(format!("no registered Library has UUID {library}"))
    })?;
    if library.origin == Origin::Starter {
        return Err(CommandError::rejected(
            "the starter Library is read-only; generate the Footprint into your own Library",
        ));
    }
    let kind = ElementKind::Footprint;
    let path =
        library
            .root
            .join(kind.folder())
            .join(format!("{}.{}", footprint.name, kind.extension()));
    if path.exists() {
        return Err(CommandError::rejected(format!(
            "{} already exists; give the Footprint another name",
            path.display()
        )));
    }
    let bytes = write_footprint(footprint).map_err(|e| CommandError::internal(e.to_string()))?;
    let io = |e: std::io::Error| CommandError::internal(format!("writing {}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    write_atomic(&path, &bytes).map_err(io)?;
    Ok(path)
}

/// The engine's [`PartSource`]: the registered Libraries on disk, as they were when the
/// Command started. It scans only when a Command asks for a Part.
pub struct DiskParts {
    starter: Option<PathBuf>,
    user_dirs: Option<UserDirs>,
    /// The Project folder and its Library paths.
    project: Option<(PathBuf, Vec<String>)>,
}

impl DiskParts {
    pub fn new(session: &Session) -> DiskParts {
        let project = match (&session.project, &session.location) {
            (Some(project), Some(folder)) if !project.props().libraries.is_empty() => {
                Some((folder.clone(), project.props().libraries.clone()))
            }
            _ => None,
        };
        DiskParts {
            starter: session.starter_library.clone(),
            user_dirs: session.user_dirs.clone(),
            project,
        }
    }

    fn roots(&self) -> Vec<LibraryRoot> {
        let mut roots = vec![];
        if let Some(starter) = &self.starter {
            roots.push(LibraryRoot::new(starter, Origin::Starter));
        }
        let prefs = preferences::current(self.user_dirs.as_ref());
        for path in prefs.libraries.paths {
            roots.push(LibraryRoot::new(path, Origin::User));
        }
        if let Some((folder, libraries)) = &self.project {
            for path in libraries {
                roots.push(LibraryRoot::new(folder.join(path), Origin::Project));
            }
        }
        roots
    }
}

impl PartSource for DiskParts {
    fn part(&self, uuid: Uuid) -> Result<PartCopy, CommandError> {
        copy_part(&scan(&self.roots(), &mut DiskLibraryFiles), uuid)
    }
}

/// The Part `uuid` of the scanned Libraries, copied with its Symbol and Footprints and
/// hashed: each Footprint over its canonical file bytes, the Part over its own file, its
/// Symbol's file and its Footprints' hashes.
pub fn copy_part(index: &LibraryIndex, uuid: Uuid) -> Result<PartCopy, CommandError> {
    let internal = |e: crate::canonical::CanonicalError| CommandError::internal(e.to_string());
    let part = index.part(uuid).ok_or_else(|| {
        CommandError::not_found(format!("no registered Library holds Part {uuid}"))
    })?;
    let missing = |what: &str, element: Uuid| {
        CommandError::not_found(format!(
            "Part {} ({uuid}) names {what} {element}, which no registered Library holds",
            part.name
        ))
    };
    let symbol = match part.symbol {
        Some(symbol) => Some(
            index
                .symbol(symbol)
                .ok_or_else(|| missing("Symbol", symbol))?,
        ),
        None => None,
    };
    let mut hasher = ContentHash::new();
    hasher.write(&write_part(part).map_err(internal)?);
    if let Some(symbol) = symbol {
        hasher.write(&write_symbol(symbol).map_err(internal)?);
    }
    let mut footprints = Vec::with_capacity(part.footprints.len());
    for &footprint in &part.footprints {
        let footprint = index
            .footprint(footprint)
            .ok_or_else(|| missing("Footprint", footprint))?;
        let mut fp_hasher = ContentHash::new();
        fp_hasher.write(&write_footprint(footprint).map_err(internal)?);
        let hash = fp_hasher.finish();
        hasher.write(hash.as_bytes());
        footprints.push(FootprintCopyData {
            hash,
            footprint: footprint.clone(),
        });
    }
    Ok(PartCopy {
        hash: hasher.finish(),
        part: part.clone(),
        symbol: symbol.cloned(),
        footprints,
    })
}

/// A 128-bit FNV-1a content hash, written as 32 lowercase hex digits. It tells copies of
/// Library elements apart; it is not meant to resist tampering.
struct ContentHash(u128);

impl ContentHash {
    const OFFSET: u128 = 0x6c62272e07bb014262b821756295c58d;
    const PRIME: u128 = 0x0000000001000000000000000000013b;

    fn new() -> ContentHash {
        ContentHash(Self::OFFSET)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u128::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn finish(self) -> String {
        format!("{:032x}", self.0)
    }
}
