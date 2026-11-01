//! Project open and save: mapping a Project folder to and from a [`Project`].

use std::path::{Path, PathBuf};

use reda_command::CommandError;
use reda_model::{DocumentKind, Project, ProjectProps, Scope};

use crate::builders::add_document;
use crate::canonical::CanonicalError;
use crate::format::{
    FileKind, LoadError, ProjectFile, parse, read_bytes, read_sheet_file, write_project_file,
    write_sheet_file,
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

/// Reads the Project whose Project file is `project_file`. Returns it with its folder.
pub fn load(project_file: &Path) -> Result<(Project, PathBuf), LoadError> {
    load_from(project_file, read_bytes)
}

/// [`load`] with the file reading supplied by the caller, so tests and the fuzz target can
/// load a Project held in memory. `read` gets each file's path under the Project folder.
pub fn load_from(
    project_file: &Path,
    mut read: impl FnMut(&Path) -> Result<Vec<u8>, LoadError>,
) -> Result<(Project, PathBuf), LoadError> {
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
    });
    let mut tx = project.transaction();
    for row in &file.documents {
        let entry = row.entry().map_err(invalid)?;
        let path = folder.join(entry.file_name());
        match entry.kind {
            DocumentKind::Schematic => {
                read_sheet_file(&path, &read(&path)?, entry.uuid)?;
            }
        }
        add_document(&mut tx, entry).map_err(|e| invalid(e.to_string()))?;
    }
    tx.commit().map_err(|e| invalid(e.to_string()))?;
    Ok((project, folder))
}

/// Every file of the Project as canonical bytes, keyed by its `/`-separated path relative to
/// the Project folder, in write order: the Project file comes last.
pub fn encode(project: &Project) -> Result<Vec<(String, Vec<u8>)>, CanonicalError> {
    encode_scoped(project, None)
}

/// Like [`encode`], but with `only` = `Some(scope)` just the one file `scope` names (the
/// Project file comes with `.reda/.gitignore`).
fn encode_scoped(
    project: &Project,
    only: Option<Scope>,
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
        };
        files.push((entry.file_name(), bytes));
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
/// `folder`.
pub fn save(
    folder: &Path,
    project: &Project,
    only: Option<Scope>,
) -> Result<Vec<String>, CommandError> {
    let io = |path: &Path, e: std::io::Error| {
        CommandError::internal(format!("writing {}: {e}", path.display()))
    };
    let files = encode_scoped(project, only).map_err(|e| CommandError::internal(e.to_string()))?;
    if only.is_none_or(|only| only == Scope::Project) {
        let state = folder.join(".reda");
        std::fs::create_dir_all(&state).map_err(|e| io(&state, e))?;
    }
    for (rel, bytes) in &files {
        let path = folder.join(rel);
        write_atomic(&path, bytes).map_err(|e| io(&path, e))?;
    }
    Ok(files.into_iter().map(|(rel, _)| rel).collect())
}
