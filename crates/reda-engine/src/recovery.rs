//! Recovery Snapshots: copies of unsaved Documents, the Project file and open Library
//! elements that the app keeps in `.reda/recovery/` so they survive an unclean exit.
//!
//! The app takes a [`View`] of the engine on its own thread and [`View::write`]s the
//! scopes that changed on a background thread. Each snapshot file holds the scope's full
//! canonical JSON, and [`MANIFEST`] records what each one restores, the path of the file it
//! belongs to and the hash of that file on disk when the edits started. A snapshot left over
//! when a Project opens means the last session ended uncleanly: [`scan`] lists them, and
//! `recovery.restore` loads chosen ones back as unsaved changes.
//!
//! Every write goes through a temp file and a rename. Nothing here touches the real files.

mod restore;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use reda_command::Session;
use reda_library::{ElementKind, LibraryIndex, LibraryRoot, scan as scan_libraries};
use reda_model::{DocumentEntry, DocumentKind, LibraryElement, Project, Scope, Uuid};
use serde::{Deserialize, Serialize};

pub(crate) use restore::restore;

use crate::Engine;
use crate::format::library::{
    read_footprint, read_part, read_symbol, write_footprint, write_part, write_symbol,
};
use crate::format::{
    FileKind, ProjectFile, parse, read_pcb_file, read_sheet_file, write_pcb_file,
    write_project_file, write_sheet_file,
};
use crate::fs::write_atomic;
use crate::library::{ContentHash, DiskLibraryFiles, registered_roots};
use crate::store;

/// The manifest file in a recovery folder.
pub const MANIFEST: &str = "manifest.json";
/// The Journal mirror in a recovery folder: the session's Journal as JSON lines.
pub const JOURNAL: &str = "journal.jsonl";
/// The snapshot of the Project file.
pub const PROJECT_SNAPSHOT: &str = "project.reda";

/// The recovery folder of the Project in `folder`: `<folder>/.reda/recovery`.
pub fn project_dir(folder: &Path) -> PathBuf {
    folder.join(".reda").join("recovery")
}

/// The manifest: one row per snapshot file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub snapshots: Vec<Snapshot>,
}

/// One snapshot file and what it restores.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    /// The snapshot's file name in the recovery folder.
    pub file: String,
    pub target: Target,
    /// The file the snapshot belongs to: relative to the Project folder for the Project
    /// file and Documents, absolute for a Library element.
    pub path: PathBuf,
    /// The hash of that file on disk when the unsaved edits began; `None` when it was not
    /// on disk.
    pub disk_hash: Option<String>,
    /// When the snapshot was written, in milliseconds since the Unix epoch.
    pub taken: i64,
}

/// What a snapshot restores.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    /// The Project file.
    Project,
    /// One Document.
    Document { document: DocumentEntry },
    /// One open Library element, from the Library `library`.
    Element {
        element: Uuid,
        library: Uuid,
        element_kind: ElementKind,
    },
    /// A file in the recovery folder the manifest does not list.
    Unknown,
}

impl Target {
    /// The undo scope the snapshot belongs to.
    pub fn scope(&self) -> Option<Scope> {
        match self {
            Target::Project => Some(Scope::Project),
            Target::Document { document } => Some(Scope::Document(document.uuid)),
            Target::Element { element, .. } => Some(Scope::Element(*element)),
            Target::Unknown => None,
        }
    }
}

/// A snapshot found in a recovery folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub snapshot: Snapshot,
    pub state: State,
    /// The file on disk is no longer the one the edits began from: saving the restored
    /// snapshot would overwrite newer content.
    pub disk_changed: bool,
    /// The file is no longer on disk (deleted, moved, or never saved).
    pub missing: bool,
}

/// Whether a found snapshot can be restored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Ready,
    /// It cannot be read; the reason says why.
    Unparsable(String),
}

/// The hash a manifest records for a file's bytes.
pub fn hash(bytes: &[u8]) -> String {
    let mut h = ContentHash::new();
    h.write(bytes);
    h.finish()
}

fn file_hash(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|b| hash(&b))
}

/// A copy of what the snapshots read, taken on the app's thread so the snapshots can be
/// written on another.
#[derive(Clone, Debug)]
pub struct View {
    project: Option<Project>,
    folder: Option<PathBuf>,
    roots: Vec<LibraryRoot>,
}

impl View {
    /// The engine's open Project as it is now.
    pub fn of(engine: &Engine) -> View {
        View::of_session(engine.session())
    }

    pub(crate) fn of_session(session: &Session) -> View {
        let has_elements = session
            .project
            .as_ref()
            .is_some_and(|p| p.elements().next().is_some());
        View {
            project: session.project.clone(),
            folder: session.location.clone(),
            roots: match has_elements {
                true => registered_roots(session),
                false => Vec::new(),
            },
        }
    }

    /// Writes a snapshot of each of `scopes` into `dir`, then the manifest, at time
    /// `taken`. Scopes it cannot snapshot (gone, or a read-only starter Library element)
    /// are skipped. A row already in the manifest keeps its disk hash: the edits still
    /// began from that file.
    pub fn write(&self, dir: &Path, scopes: &[Scope], taken: i64) -> std::io::Result<()> {
        let Some(project) = &self.project else {
            return Ok(());
        };
        let mut index: Option<LibraryIndex> = None;
        let mut manifest = read_manifest(dir).unwrap_or_default();
        std::fs::create_dir_all(dir)?;
        for scope in scopes {
            let Some(content) = self.content(project, *scope, &mut index)? else {
                continue;
            };
            write_atomic(&dir.join(&content.file), &content.bytes)?;
            let disk_hash = match manifest.snapshots.iter().find(|s| s.file == content.file) {
                Some(old) if old.path == content.path => old.disk_hash.clone(),
                _ => file_hash(&self.disk_path(&content.path)),
            };
            manifest.snapshots.retain(|s| s.file != content.file);
            manifest.snapshots.push(Snapshot {
                file: content.file,
                target: content.target,
                path: content.path,
                disk_hash,
                taken,
            });
        }
        write_manifest(dir, &manifest)
    }

    fn disk_path(&self, path: &Path) -> PathBuf {
        match &self.folder {
            Some(folder) if path.is_relative() => folder.join(path),
            _ => path.to_owned(),
        }
    }

    fn content(
        &self,
        project: &Project,
        scope: Scope,
        index: &mut Option<LibraryIndex>,
    ) -> std::io::Result<Option<Content>> {
        let encode = |r: Result<Vec<u8>, crate::canonical::CanonicalError>| {
            r.map_err(|e| std::io::Error::other(e.to_string()))
        };
        Ok(Some(match scope {
            Scope::Project => Content {
                file: PROJECT_SNAPSHOT.into(),
                bytes: encode(write_project_file(project))?,
                target: Target::Project,
                path: store::project_file_path(Path::new(""), &project.props().name),
            },
            Scope::Document(uuid) => {
                let Some(entry) = project.document(uuid) else {
                    return Ok(None);
                };
                let bytes = match entry.kind {
                    DocumentKind::Schematic => {
                        let Some(sheet) = project.schematic_sheet(uuid) else {
                            return Ok(None);
                        };
                        encode(write_sheet_file(sheet))?
                    }
                    DocumentKind::Pcb => {
                        let Some(pcb) = project.pcb_document(uuid) else {
                            return Ok(None);
                        };
                        encode(write_pcb_file(pcb))?
                    }
                };
                Content {
                    file: format!("{uuid}.{}", entry.kind.extension()),
                    bytes,
                    target: Target::Document {
                        document: entry.clone(),
                    },
                    path: PathBuf::from(entry.file_name()),
                }
            }
            Scope::Element(uuid) => {
                let (Some(source), Some(element)) =
                    (project.element_source(uuid), project.element(uuid))
                else {
                    return Ok(None);
                };
                if source.read_only {
                    return Ok(None);
                }
                let index =
                    index.get_or_insert_with(|| scan_libraries(&self.roots, &mut DiskLibraryFiles));
                let kind = ElementKind::of(element);
                let path = match (index.library_of(uuid), index.file_of(uuid)) {
                    (Some(library), Some(file)) if library.info.uuid == source.library => {
                        file.to_owned()
                    }
                    _ => match index.library(source.library) {
                        Some(library) => library.root.join(kind.folder()).join(format!(
                            "{}.{}",
                            element.name(),
                            kind.extension()
                        )),
                        None => return Ok(None),
                    },
                };
                let bytes = match element {
                    LibraryElement::Symbol(s) => encode(write_symbol(s))?,
                    LibraryElement::Footprint(f) => encode(write_footprint(f))?,
                    LibraryElement::Part(p) => encode(write_part(p))?,
                };
                Content {
                    file: format!("{uuid}.{}", kind.extension()),
                    bytes,
                    target: Target::Element {
                        element: uuid,
                        library: source.library,
                        element_kind: kind,
                    },
                    path,
                }
            }
        }))
    }
}

struct Content {
    file: String,
    bytes: Vec<u8>,
    target: Target,
    path: PathBuf,
}

/// The manifest in `dir`: the default when there is none, `Err` with the
/// reason when it cannot be read.
fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    match std::fs::read(dir.join(MANIFEST)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("the manifest: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::default()),
        Err(e) => Err(format!("reading the manifest: {e}")),
    }
}

fn write_manifest(dir: &Path, manifest: &Manifest) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(manifest).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    write_atomic(&dir.join(MANIFEST), &bytes)
}

/// Whether `name` is a file the recovery folder keeps besides snapshots.
fn is_bookkeeping(name: &str) -> bool {
    name == MANIFEST || name.starts_with("journal") || name.starts_with('.')
}

/// Every snapshot in `dir`, in manifest order, then files the manifest does not list.
/// `folder` is the Project folder that relative paths are under.
pub fn scan(dir: &Path, folder: Option<&Path>) -> Vec<Found> {
    let (manifest, manifest_error) = match read_manifest(dir) {
        Ok(m) => (m, None),
        Err(why) => (Manifest::default(), Some(why)),
    };
    let mut out: Vec<Found> = manifest
        .snapshots
        .iter()
        .map(|s| {
            let disk = match folder {
                Some(folder) if s.path.is_relative() => folder.join(&s.path),
                _ => s.path.clone(),
            };
            let on_disk = file_hash(&disk);
            Found {
                state: check(dir, s),
                disk_changed: on_disk.is_some() && on_disk != s.disk_hash,
                missing: on_disk.is_none(),
                snapshot: s.clone(),
            }
        })
        .collect();
    let mut others: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|n| !is_bookkeeping(n) && !manifest.snapshots.iter().any(|s| s.file == *n))
        .collect();
    others.sort();
    for file in others {
        let why = manifest_error
            .clone()
            .unwrap_or_else(|| "the manifest does not list it".into());
        out.push(Found {
            snapshot: Snapshot {
                path: dir.join(&file),
                file,
                target: Target::Unknown,
                disk_hash: None,
                taken: 0,
            },
            state: State::Unparsable(why),
            disk_changed: false,
            missing: false,
        });
    }
    out
}

/// Whether the snapshot file reads as what the manifest says it is.
fn check(dir: &Path, snapshot: &Snapshot) -> State {
    match parse_snapshot(dir, snapshot) {
        Ok(_) => State::Ready,
        Err(why) => State::Unparsable(why),
    }
}

/// A snapshot's contents, read and checked.
pub(crate) enum Parsed {
    /// The Project file's bytes, checked to parse.
    Project(Vec<u8>),
    Document(DocumentEntry, Vec<u8>),
    Element(Uuid, LibraryElement),
}

pub(crate) fn parse_snapshot(dir: &Path, snapshot: &Snapshot) -> Result<Parsed, String> {
    let path = dir.join(&snapshot.file);
    let bytes = std::fs::read(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let reason = |e: crate::format::LoadError| e.to_string();
    Ok(match &snapshot.target {
        Target::Project => {
            parse::<ProjectFile>(FileKind::Project, &path, &bytes).map_err(reason)?;
            Parsed::Project(bytes)
        }
        Target::Document { document } => {
            match document.kind {
                DocumentKind::Schematic => read_sheet_file(&path, &bytes, document.uuid),
                DocumentKind::Pcb => read_pcb_file(&path, &bytes, document.uuid),
            }
            .map_err(reason)?;
            Parsed::Document(document.clone(), bytes)
        }
        Target::Element {
            element,
            element_kind,
            ..
        } => {
            let read = match element_kind {
                ElementKind::Symbol => read_symbol(&path, &bytes).map(LibraryElement::Symbol),
                ElementKind::Footprint => {
                    read_footprint(&path, &bytes).map(LibraryElement::Footprint)
                }
                ElementKind::Part => read_part(&path, &bytes).map(LibraryElement::Part),
            }
            .map_err(reason)?;
            if read.uuid() != *element {
                return Err(format!(
                    "{} holds element {} instead of {element}",
                    path.display(),
                    read.uuid()
                ));
            }
            Parsed::Element(*element, read)
        }
        Target::Unknown => return Err("the manifest does not list it".into()),
    })
}

/// Deletes the snapshots of `scopes` and their manifest rows: after a save, or when a
/// Document closes without saving.
pub fn remove(dir: &Path, scopes: &[Scope]) -> std::io::Result<()> {
    let manifest = read_manifest(dir).unwrap_or_default();
    let files: Vec<String> = manifest
        .snapshots
        .iter()
        .filter(|s| s.target.scope().is_some_and(|sc| scopes.contains(&sc)))
        .map(|s| s.file.clone())
        .collect();
    discard(dir, &files)
}

/// Deletes these snapshot files and their manifest rows.
pub fn discard(dir: &Path, files: &[String]) -> std::io::Result<()> {
    if files.is_empty() || !dir.exists() {
        return Ok(());
    }
    for file in files {
        match std::fs::remove_file(dir.join(file)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    if let Ok(mut manifest) = read_manifest(dir) {
        manifest.snapshots.retain(|s| !files.contains(&s.file));
        write_manifest(dir, &manifest)?;
    }
    Ok(())
}

/// Deletes the whole recovery folder: the Project closed cleanly.
pub fn clear(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Appends `lines` (JSON lines, each ending in a newline) to the Journal mirror in `dir`.
pub fn append_journal(dir: &Path, lines: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(JOURNAL))?;
    file.write_all(lines.as_bytes())?;
    file.flush()
}

/// Every snapshot row of the manifest in `dir`, by file name.
pub(crate) fn manifest_rows(dir: &Path) -> Result<BTreeMap<String, Snapshot>, String> {
    Ok(read_manifest(dir)?
        .snapshots
        .into_iter()
        .map(|s| (s.file.clone(), s))
        .collect())
}
