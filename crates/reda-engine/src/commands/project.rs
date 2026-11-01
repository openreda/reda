//! `project.new`, `project.open` and `project.save`.

use std::path::{Path, PathBuf};

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_PROJECT, Registry, Session,
    Unavailable,
};
use reda_model::{DocumentEntry, DocumentKind, Project, ProjectProps, Scope, Uuid};
use serde::{Deserialize, Serialize};

use super::DocumentRef;
use crate::builders::add_document;
use crate::store;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ProjectNew);
    registry.add(ProjectOpen);
    registry.add(ProjectSave);
}

/// The title of the first sheet of a new Project.
const FIRST_SHEET_TITLE: &str = "Sheet 1";

/// What `project.new` and `project.open` return.
#[derive(Debug, Serialize)]
pub struct ProjectSummary {
    pub uuid: Uuid,
    pub name: String,
    /// The Project file.
    pub path: PathBuf,
    /// In Project order.
    pub documents: Vec<DocumentEntry>,
}

fn summary(session: &Session) -> ProjectSummary {
    let project = session.project.as_ref().expect("a Project is open");
    let folder = session.location.as_deref().unwrap_or(Path::new(""));
    ProjectSummary {
        uuid: project.uuid(),
        name: project.props().name.clone(),
        path: store::project_file_path(folder, &project.props().name),
        documents: project.documents().cloned().collect(),
    }
}

pub struct ProjectNew;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectNewParams {
    /// The folder in which the Project's own folder `<path>/<name>/` is created.
    pub path: PathBuf,
    /// The Project name: its folder name and the stem of its `.reda` file.
    pub name: String,
}

impl Command for ProjectNew {
    const META: Meta = Meta {
        id: "project.new",
        title: "New Project",
        description: "Creates a Project called `name` in the folder `<path>/<name>/` with \
                      one empty Schematic Sheet titled \"Sheet 1\", and opens it. The \
                      folder must not exist or be empty. Nothing is written until \
                      `project.save`.",
        category: "project",
        icon: "project_new",
        kind: Kind::File,
    };
    type Params = ProjectNewParams;
    type Output = ProjectSummary;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ProjectNewParams,
    ) -> Result<ProjectSummary, CommandError> {
        check_file_stem(&p.name).map_err(|why| CommandError::invalid_params("/name", why))?;
        let folder = p.path.join(&p.name);
        check_free_folder(&folder)?;

        let mut project = Project::new(ProjectProps {
            uuid: cx.new_id(),
            name: p.name.clone(),
            documents: vec![],
        });
        let sheet = DocumentEntry {
            uuid: cx.new_id(),
            kind: DocumentKind::Schematic,
            name: p.name,
            title: FIRST_SHEET_TITLE.into(),
        };
        let mut tx = project.transaction();
        add_document(&mut tx, sheet)?;
        tx.commit()?;

        let session = cx.session()?;
        // Nothing of a new Project is on disk yet, so every scope is dirty.
        session.history.clear();
        session.history.mark_unsaved(Scope::Project);
        for document in project.documents() {
            session.history.mark_unsaved(Scope::Document(document.uuid));
        }
        session.project = Some(project);
        session.location = Some(folder);
        Ok(summary(session))
    }
}

pub struct ProjectOpen;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectOpenParams {
    /// The `.reda` file, or the Project folder that holds `<folder name>.reda`.
    pub path: PathBuf,
}

impl Command for ProjectOpen {
    const META: Meta = Meta {
        id: "project.open",
        title: "Open Project",
        description: "Opens the Project at `path`: its `.reda` file or its folder. Any \
                      Project already open is closed without saving.",
        category: "project",
        icon: "project_open",
        kind: Kind::File,
    };
    type Params = ProjectOpenParams;
    type Output = ProjectSummary;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ProjectOpenParams,
    ) -> Result<ProjectSummary, CommandError> {
        let (project, folder) = store::load(&store::resolve_project_file(&p.path))?;
        let session = cx.session()?;
        session.project = Some(project);
        session.location = Some(folder);
        session.history.clear();
        Ok(summary(session))
    }
}

pub struct ProjectSave;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSaveParams {
    /// Saves only this file: a Document's UUID, or `"project"` for the Project file.
    /// Omitted, every file is saved (Save All).
    #[serde(default)]
    pub document: Option<DocumentRef>,
}

#[derive(Debug, Serialize)]
pub struct ProjectSaveResult {
    /// The Project file.
    pub path: PathBuf,
    /// Every file written, relative to the Project folder.
    pub written: Vec<String>,
}

impl Command for ProjectSave {
    const META: Meta = Meta {
        id: "project.save",
        title: "Save All",
        description: "Writes the Project file and every Document to the Project folder, \
                      each through a temp file and a rename. With `document`, writes only \
                      that Document's file, or only the Project file for `\"project\"`.",
        category: "project",
        icon: "save_all",
        kind: Kind::File,
    };
    type Params = ProjectSaveParams;
    type Output = ProjectSaveResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            Some(_) => Ok(()),
            None => Err(Unavailable {
                reason: NO_PROJECT.into(),
            }),
        }
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ProjectSaveParams,
    ) -> Result<ProjectSaveResult, CommandError> {
        let only = p.document.map(|d| d.0);
        let session = cx.session()?;
        let project = session
            .project
            .as_ref()
            .ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
        let folder = session
            .location
            .clone()
            .ok_or_else(|| CommandError::unavailable("the Project has no folder yet"))?;
        std::fs::create_dir_all(&folder)
            .map_err(|e| CommandError::internal(format!("creating {}: {e}", folder.display())))?;
        if let Some(Scope::Document(uuid)) = only
            && project.document(uuid).is_none()
        {
            return Err(CommandError::not_found(format!(
                "no Document has UUID {uuid}"
            )));
        }
        let written = store::save(&folder, project, only)?;
        let path = store::project_file_path(&folder, &project.props().name);
        match only {
            Some(scope) => session.history.mark_saved(scope),
            None => session.history.mark_all_saved(),
        }
        Ok(ProjectSaveResult { path, written })
    }
}

/// Checks that `name` can be a file stem on every OS REDA runs on.
fn check_file_stem(name: &str) -> Result<(), String> {
    const RESERVED: &[char] = &['/', '\\', '<', '>', ':', '"', '|', '?', '*'];
    if name.trim().is_empty() {
        return Err("the name is empty".into());
    }
    if name == "." || name == ".." {
        return Err(format!("{name:?} cannot be a file name"));
    }
    if let Some(c) = name
        .chars()
        .find(|c| RESERVED.contains(c) || c.is_control())
    {
        return Err(format!(
            "the name contains {c:?}, which file names cannot hold"
        ));
    }
    if name.ends_with(['.', ' ']) {
        return Err("the name cannot end in a dot or a space".into());
    }
    Ok(())
}

/// A new Project's folder must not exist, or be empty.
fn check_free_folder(folder: &Path) -> Result<(), CommandError> {
    match std::fs::read_dir(folder) {
        Ok(mut entries) => match entries.next() {
            None => Ok(()),
            Some(_) => Err(CommandError::rejected(format!(
                "{} exists and is not empty",
                folder.display()
            ))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) if folder.exists() => Err(CommandError::rejected(format!(
            "{} exists and is not an empty folder",
            folder.display()
        ))),
        Err(e) => Err(CommandError::internal(format!(
            "reading {}: {e}",
            folder.display()
        ))),
    }
}
