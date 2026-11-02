//! Project open and save: mapping a Project folder to and from a [`Project`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reda_command::{CommandError, Trash};
use reda_model::{DocumentEntry, DocumentKind, Object, Project, ProjectProps, Scope, Uuid};

use crate::builders::add_document;
use crate::canonical::CanonicalError;
use crate::format::{
    FileKind, LoadError, ProjectFile, parse, read_bytes, read_pcb_file, read_sheet_file,
    write_pcb_file, write_project_file, write_sheet_file,
};
use crate::fs::write_atomic;

/// The extension of the Project file.
pub const PROJECT_EXTENSION: &str = "reda";

/// The Project file inside `folder` for a Project called `name`.
pub fn project_file_path(folder: &Path, name: &str) -> PathBuf {
    folder.join(format!("{name}.{PROJECT_EXTENSION}"))
}

/// The Project file a user-given path names: the path itself, or for a folder,
/// `<folder>/<folder name>.reda`. Nothing is found by scanning the folder.
pub fn resolve_project_file(path: &Path) -> PathBuf {
    if path.is_dir()
        && let Some(name) = path.file_name().and_then(|n| n.to_str())
    {
        return project_file_path(path, name);
    }
    path.to_owned()
}

/// A Project read from disk.
#[derive(Debug)]
pub struct Opened {
    pub project: Project,
    /// The Project folder.
    pub folder: PathBuf,
    /// Documents the Project file lists whose files are missing. A missing PCB Document
    /// is left out of `project`, so saving drops it from the Project file; a missing
    /// Schematic Sheet is recreated empty at its manifest path (a Recovery Snapshot may
    /// then restore its contents), so saving writes it again.
    pub missing: Vec<DocumentEntry>,
}

/// Reads the Project whose Project file is `project_file`. Returns it with its folder.
pub fn load(project_file: &Path) -> Result<(Project, PathBuf), LoadError> {
    load_from(project_file, read_bytes)
}

/// [`load`] with the file reading supplied by the caller, so tests and the fuzz target can
/// load a Project held in memory. `read` gets each file's path under the Project folder.
pub fn load_from(
    project_file: &Path,
    read: impl FnMut(&Path) -> Result<Vec<u8>, LoadError>,
) -> Result<(Project, PathBuf), LoadError> {
    open_from(project_file, read).map(|o| (o.project, o.folder))
}

/// Like [`load`], and reports the Documents whose files are missing.
pub fn open(project_file: &Path) -> Result<Opened, LoadError> {
    open_from(project_file, read_bytes)
}

/// [`open`] with the file reading supplied by the caller, as for [`load_from`].
pub fn open_from(
    project_file: &Path,
    mut read: impl FnMut(&Path) -> Result<Vec<u8>, LoadError>,
) -> Result<Opened, LoadError> {
    let invalid = |why: String| LoadError::Invalid(project_file.to_owned(), why);
    let name = project_file
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|_| {
            project_file
                .extension()
                .is_some_and(|e| e == PROJECT_EXTENSION)
        })
        .ok_or_else(|| invalid("is not a .reda Project file".into()))?;
    let folder = project_file
        .parent()
        .map(Path::to_owned)
        .unwrap_or_default();
    let file: ProjectFile = parse(FileKind::Project, project_file, &read(project_file)?)?;

    let mut project = Project::new(ProjectProps {
        uuid: file.project.uuid,
        name: name.to_owned(),
        documents: vec![],
        libraries: file.libraries.clone(),
        properties: file.properties.clone(),
        erc: file.erc,
    });
    let mut missing = vec![];
    let mut tx = project.transaction();
    for row in &file.documents {
        let entry = row.entry().map_err(invalid)?;
        let path = folder.join(entry.file_name());
        let objects = match entry.kind {
            DocumentKind::Schematic => match read(&path) {
                Ok(bytes) => read_sheet_file(&path, &bytes, entry.uuid)?,
                Err(LoadError::Missing(_)) => {
                    missing.push(entry.clone());
                    vec![]
                }
                Err(e) => return Err(e),
            },
            DocumentKind::Pcb => match read(&path) {
                Ok(bytes) => read_pcb_file(&path, &bytes, entry.uuid)?,
                Err(LoadError::Missing(_)) => {
                    missing.push(entry);
                    continue;
                }
                Err(e) => return Err(e),
            },
        };
        add_document(&mut tx, entry).map_err(|e| invalid(e.to_string()))?;
        for object in objects {
            tx.insert(object).map_err(|e| invalid(e.to_string()))?;
        }
    }
    let settings = (file.fab_presets.into_iter().map(Object::FabPreset))
        .chain(file.output_jobs.into_iter().map(Object::OutputJob));
    for object in settings {
        tx.insert(object).map_err(|e| invalid(e.to_string()))?;
    }
    tx.commit().map_err(|e| invalid(e.to_string()))?;
    Ok(Opened {
        project,
        folder,
        missing,
    })
}

/// Every file of the Project as canonical bytes, keyed by its `/`-separated path relative to
/// the Project folder, in write order: the Project file comes last.
pub fn encode(project: &Project) -> Result<Vec<(String, Vec<u8>)>, CanonicalError> {
    encode_scoped(project, None, &DocumentEntry::file_name)
}

/// Like [`encode`], but with `only` = `Some(scope)` just the one file `scope` names (the
/// Project file comes with `.reda/.gitignore`). `file_name` names each Document's file.
fn encode_scoped(
    project: &Project,
    only: Option<Scope>,
    file_name: &dyn Fn(&DocumentEntry) -> String,
) -> Result<Vec<(String, Vec<u8>)>, CanonicalError> {
    let wanted = |scope: Scope| only.is_none_or(|only| only == scope);
    let mut files: Vec<(String, Vec<u8>)> = vec![];
    if wanted(Scope::Project) {
        files.push((".reda/.gitignore".into(), b"*\n".to_vec()));
    }
    for entry in project
        .documents()
        .filter(|d| wanted(Scope::Document(d.uuid)))
    {
        let bytes = match entry.kind {
            DocumentKind::Schematic => {
                let sheet = project
                    .schematic_sheet(entry.uuid)
                    .expect("every Schematic entry has a sheet body");
                write_sheet_file(sheet)?
            }
            DocumentKind::Pcb => {
                let pcb = project
                    .pcb_document(entry.uuid)
                    .expect("every PCB entry has a PCB body");
                write_pcb_file(pcb)?
            }
        };
        files.push((file_name(entry), bytes));
    }
    if wanted(Scope::Project) {
        let name = format!("{}.{PROJECT_EXTENSION}", project.props().name);
        files.push((name, write_project_file(project)?));
    }
    Ok(files)
}

/// Writes the Project's files into `folder`, each through a temp file and a rename, the
/// Project file last: every file with `only` = `None`, or just the one `only` names (the
/// Project file comes with `.reda/.gitignore`). Returns the written paths relative to
/// `folder`. For a Project with no files on disk yet; [`save_tracked`] also renames and
/// trashes the files of renamed and deleted Documents.
pub fn save(
    folder: &Path,
    project: &Project,
    only: Option<Scope>,
) -> Result<Vec<String>, CommandError> {
    let mut on_disk = BTreeMap::new();
    save_tracked(folder, project, only, &mut on_disk, None).map(|saved| saved.written)
}

/// What [`save_tracked`] did on disk. Paths are relative to the Project folder.
#[derive(Debug, Default)]
pub struct Saved {
    /// Every file written, in write order.
    pub written: Vec<String>,
    /// Files moved to a renamed Document's new name, as `(from, to)`.
    pub renamed: Vec<(String, String)>,
    /// Files of deleted Documents sent to the trash.
    pub trashed: Vec<String>,
}

/// Like [`save`], keeping the Documents' files in step with the Project. `on_disk` holds
/// each Document's file as REDA last read or wrote it, and is updated to match.
///
/// Saving the Project file (`only` = `None` or the Project) first sends the files of
/// deleted Documents to `trash` (left on disk without one), then renames the files of
/// renamed Documents, each through a temp name so swapped names and case-only changes
/// work. Saving a Document alone writes it to the file the Project file on disk lists,
/// and first trashes a deleted Document's file it would overwrite.
pub fn save_tracked(
    folder: &Path,
    project: &Project,
    only: Option<Scope>,
    on_disk: &mut BTreeMap<Uuid, String>,
    trash: Option<&dyn Trash>,
) -> Result<Saved, CommandError> {
    let io = |path: &Path, e: std::io::Error| {
        CommandError::internal(format!("writing {}: {e}", path.display()))
    };
    let with_project = only.is_none_or(|only| only == Scope::Project);
    // Each Document's file as this save writes it.
    let names: BTreeMap<Uuid, String> = project
        .documents()
        .map(|d| match on_disk.get(&d.uuid) {
            Some(name) if !with_project => (d.uuid, name.clone()),
            _ => (d.uuid, d.file_name()),
        })
        .collect();
    let files = encode_scoped(project, only, &|d| names[&d.uuid].clone())
        .map_err(|e| CommandError::internal(e.to_string()))?;
    let mut saved = Saved::default();
    let same = |a: &str, b: &str| a.to_lowercase() == b.to_lowercase();

    // Deleted Documents' files.
    let doomed: Vec<Uuid> = on_disk
        .iter()
        .filter(|(uuid, name)| {
            project.document(**uuid).is_none()
                && (with_project || files.iter().any(|(rel, _)| same(rel, name)))
        })
        .map(|(uuid, _)| *uuid)
        .collect();
    for uuid in doomed {
        let name = on_disk.remove(&uuid).expect("listed above");
        let claimed = on_disk
            .iter()
            .any(|(other, n)| project.document(*other).is_some() && same(n, &name));
        let Some(trash) = trash.filter(|_| !claimed) else {
            continue;
        };
        let path = folder.join(&name);
        match trash.trash(&path) {
            Ok(()) => saved.trashed.push(name),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(CommandError::internal(format!(
                    "moving {} to the trash: {e}",
                    path.display()
                )));
            }
        }
    }

    // Renamed Documents' files: every one to a temp name first, then to its new name.
    if with_project {
        let mut moves = vec![];
        for entry in project.documents() {
            let Some(old) = on_disk.get(&entry.uuid) else {
                continue;
            };
            let new = entry.file_name();
            if *old == new {
                continue;
            }
            let temp = format!(".{old}.{}.rename.tmp", std::process::id());
            match std::fs::rename(folder.join(old), folder.join(&temp)) {
                Ok(()) => moves.push((entry.uuid, old.clone(), temp, new)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(&folder.join(old), e)),
            }
        }
        for (uuid, old, temp, new) in moves {
            let path = folder.join(&new);
            std::fs::rename(folder.join(temp), &path).map_err(|e| io(&path, e))?;
            on_disk.insert(uuid, new.clone());
            saved.renamed.push((old, new));
        }
    }

    if with_project {
        let state = folder.join(".reda");
        std::fs::create_dir_all(&state).map_err(|e| io(&state, e))?;
    }
    let documents: BTreeMap<&String, Uuid> = names.iter().map(|(u, n)| (n, *u)).collect();
    for (rel, bytes) in files {
        let path = folder.join(&rel);
        write_atomic(&path, &bytes).map_err(|e| io(&path, e))?;
        if let Some(uuid) = documents.get(&rel) {
            on_disk.insert(*uuid, rel.clone());
        }
        saved.written.push(rel);
    }
    Ok(saved)
}
