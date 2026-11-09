//! The registered Libraries on disk: which folders they are, and reading them for
//! `reda-library`'s scan, and writing generated Footprints into them.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use reda_command::{
    CommandError, ElementLocation, FootprintCopyData, PartCopy, PartSource, Session, UserDirs,
};
/// The Footprint generators' Command ids, and the Footprint one would write, for a
/// generator form's preview.
pub use reda_library::generate::{
    GENERATORS, GeneratorWarning, Lost as RegenerateChange, Preview as GeneratorPreview,
    Regenerated, WarningCode as GeneratorWarningCode, calculated as calculated_values,
    preview as preview_footprint, regenerate_footprint,
};
/// How `project.library.add` stores a Project Library path, for front ends that check a
/// path before running it.
pub use reda_library::normalise_library_path;
/// The scanned Libraries and their search, for front ends that list Parts.
pub use reda_library::{Library, LibraryIndex, Origin as LibraryOrigin, SearchHit, SearchQuery};

/// A [`PartSource`] over Parts already copied out of the Libraries, so a Tool that
/// previews the same placement many times reads the disk once.
pub struct ReadyParts(Vec<PartCopy>);

impl ReadyParts {
    pub fn new(parts: Vec<PartCopy>) -> ReadyParts {
        ReadyParts(parts)
    }
}

impl PartSource for ReadyParts {
    fn part(&self, uuid: Uuid) -> Result<PartCopy, CommandError> {
        self.0
            .iter()
            .find(|c| c.part.uuid == uuid)
            .cloned()
            .ok_or_else(|| CommandError::not_found(format!("Part {uuid} is not at hand")))
    }
}
use reda_library::{ElementKind, LibraryFiles, LibraryRoot, Origin, scan};
use reda_model::{Footprint, LibraryElement, LibraryInfo, Part, Symbol, Uuid};

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

/// The active registered Library folders, in resolution order: the open Project's from
/// its `.reda` file, the user's from Preferences, then the Starter Library. A user Library
/// or the Starter Library whose Active checkbox is off is left out. A Project not yet
/// given a folder has no Project Libraries to find.
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

/// The file `element` gets in `library`: `<library>/<folder>/<name>.<extension>`.
pub(crate) fn element_file(library: &Library, element: &LibraryElement) -> PathBuf {
    let kind = ElementKind::of(element);
    library
        .root
        .join(kind.folder())
        .join(format!("{}.{}", element.name(), kind.extension()))
}

/// Writes a new element to its file in `library` (see [`element_file`]), refusing a file
/// that exists already.
pub(crate) fn write_new_element(
    library: &Library,
    element: &LibraryElement,
) -> Result<PathBuf, CommandError> {
    let path = element_file(library, element);
    if path.exists() {
        return Err(CommandError::rejected(format!(
            "{} already exists and holds another element",
            path.display()
        )));
    }
    let bytes = match element {
        LibraryElement::Symbol(s) => write_symbol(s),
        LibraryElement::Footprint(f) => write_footprint(f),
        LibraryElement::Part(p) => write_part(p),
    }
    .map_err(|e| CommandError::internal(e.to_string()))?;
    let io = |e: std::io::Error| CommandError::internal(format!("writing {}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    write_atomic(&path, &bytes).map_err(io)?;
    Ok(path)
}

/// The registered Library `uuid`, which must be writable: not the Starter Library.
pub(crate) fn writable_library(index: &LibraryIndex, uuid: Uuid) -> Result<&Library, CommandError> {
    let library = index
        .library(uuid)
        .ok_or_else(|| CommandError::not_found(format!("no registered Library has UUID {uuid}")))?;
    if library.origin == Origin::Starter {
        return Err(CommandError::rejected(
            "the starter Library is read-only; choose your own Library",
        ));
    }
    Ok(library)
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
        if let Some((folder, libraries)) = &self.project {
            for path in libraries {
                roots.push(LibraryRoot::new(folder.join(path), Origin::Project));
            }
        }
        let prefs = preferences::current(self.user_dirs.as_ref()).libraries;
        for path in prefs.active_paths() {
            roots.push(LibraryRoot::new(path, Origin::User));
        }
        if let Some(starter) = self.starter.as_ref().filter(|_| prefs.starter_active) {
            roots.push(LibraryRoot::new(starter, Origin::Starter));
        }
        roots
    }
}

impl PartSource for DiskParts {
    fn part(&self, uuid: Uuid) -> Result<PartCopy, CommandError> {
        copy_part(&scan(&self.roots(), &mut DiskLibraryFiles), uuid)
    }

    fn parts(&self, uuids: &[Uuid]) -> Vec<Result<PartCopy, CommandError>> {
        if uuids.is_empty() {
            return vec![];
        }
        let index = scan(&self.roots(), &mut DiskLibraryFiles);
        uuids.iter().map(|&uuid| copy_part(&index, uuid)).collect()
    }

    fn footprints(&self, uuids: &[Uuid]) -> Vec<Result<FootprintCopyData, CommandError>> {
        if uuids.is_empty() {
            return vec![];
        }
        let index = scan(&self.roots(), &mut DiskLibraryFiles);
        uuids
            .iter()
            .map(|&uuid| {
                let footprint = index.footprint(uuid).ok_or_else(|| {
                    CommandError::not_found(format!("no registered Library holds Footprint {uuid}"))
                })?;
                copy_footprint(footprint)
            })
            .collect()
    }

    fn locate(&self, elements: &[Uuid]) -> Vec<Option<ElementLocation>> {
        if elements.is_empty() {
            return vec![];
        }
        let index = scan(&self.roots(), &mut DiskLibraryFiles);
        elements
            .iter()
            .map(|&uuid| {
                let library = index.library_of(uuid)?;
                Some(ElementLocation {
                    library: library.info.uuid,
                    path: index.file_of(uuid)?.to_owned(),
                    read_only: library.origin == Origin::Starter,
                })
            })
            .collect()
    }
}

/// The Part `uuid` of the scanned Libraries, copied with its Symbol and Footprints and
/// hashed: each Footprint over its canonical file bytes without its `generator`, the Part
/// over its own file, its Symbol's file and its Footprints' hashes.
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
        let copy = copy_footprint(footprint)?;
        hasher.write(copy.hash.as_bytes());
        footprints.push(copy);
    }
    Ok(PartCopy {
        hash: hasher.finish(),
        part: part.clone(),
        symbol: symbol.cloned(),
        footprints,
    })
}

/// A Library Footprint as a Document copies it, with its content hash: over its canonical
/// file bytes, leaving out how it was generated, which a Document's copy doesn't hold.
fn copy_footprint(footprint: &Footprint) -> Result<FootprintCopyData, CommandError> {
    let footprint = footprint.without_generator();
    let bytes = write_footprint(&footprint).map_err(|e| CommandError::internal(e.to_string()))?;
    let mut hasher = ContentHash::new();
    hasher.write(&bytes);
    Ok(FootprintCopyData {
        hash: hasher.finish(),
        footprint,
    })
}

/// A 128-bit FNV-1a content hash, written as 32 lowercase hex digits. It tells copies of
/// Library elements apart; it is not meant to resist tampering.
pub(crate) struct ContentHash(u128);

impl ContentHash {
    const OFFSET: u128 = 0x6c62272e07bb014262b821756295c58d;
    const PRIME: u128 = 0x0000000001000000000000000000013b;

    pub(crate) fn new() -> ContentHash {
        ContentHash(Self::OFFSET)
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u128::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    pub(crate) fn finish(self) -> String {
        format!("{:032x}", self.0)
    }
}
