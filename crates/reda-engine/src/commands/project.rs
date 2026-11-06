//! `project.new`, `project.open`, `project.save` and `project.close`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, NO_PROJECT, Registry,
    Session, Unavailable,
};
use reda_model::geom::Vertex;
use reda_model::{
    BoardOutline, DocumentEntry, DocumentKind, FabPreset, InDocument, Object, Project,
    ProjectFabPreset, ProjectProps, Scope, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::DocumentRef;
use crate::builders::add_document;
use crate::{fab_presets, state, store};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ProjectNew);
    registry.add(ProjectOpen);
    registry.add(ProjectSave);
    registry.add(ProjectClose);
}

/// The title of the first sheet of a new Project.
const FIRST_SHEET_TITLE: &str = "Sheet 1";

/// What `project.new` and `project.open` return.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ProjectSummary {
    pub uuid: Uuid,
    pub name: String,
    /// The Project file.
    pub path: PathBuf,
    /// In Project order.
    pub documents: Vec<DocumentEntry>,
    /// Documents the Project file lists whose files are missing; left out when there are
    /// none. A missing PCB Document is left out of the open Project (`document.add {kind:
    /// pcb}` adds a new one); a missing Schematic Sheet is recreated empty and unsaved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<DocumentEntry>,
    /// The Output Jobs, by name, for `output.job.run`; left out when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_jobs: Vec<JobEntry>,
}

/// An Output Job as `project.new` and `project.open` list it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct JobEntry {
    pub uuid: Uuid,
    pub name: String,
}

fn summary(session: &Session, missing: Vec<DocumentEntry>) -> ProjectSummary {
    let project = session.project.as_ref().expect("a Project is open");
    let folder = session.location.as_deref().unwrap_or(Path::new(""));
    let mut output_jobs: Vec<JobEntry> = project
        .output_jobs()
        .map(|job| JobEntry {
            uuid: job.uuid,
            name: job.name.clone(),
        })
        .collect();
    output_jobs.sort_by_key(|job| job.name.to_lowercase());
    ProjectSummary {
        uuid: project.uuid(),
        name: project.props().name.clone(),
        path: store::project_file_path(folder, &project.props().name),
        documents: project.documents().cloned().collect(),
        missing,
        output_jobs,
    }
}

pub struct ProjectNew;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectNewParams {
    /// The folder in which the Project's own folder `<path>/<name>/` is created.
    pub path: PathBuf,
    /// The Project name: its folder name and the stem of its `.reda` file.
    pub name: String,
    /// The Fab Preset that seeds the board's rules and names the first Output Job: a
    /// shipped preset (Generic, JLCPCB, PCBWay) or one of the user's `.redafab` presets.
    /// Defaults to Generic.
    #[serde(default)]
    pub fab_preset: Option<String>,
    /// A rectangular Board Outline to start with; left out to draw it later.
    #[serde(default)]
    pub board: Option<BoardParam>,
}

/// A rectangular Board Outline with its lower left corner at the origin.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardParam {
    pub width: Length,
    pub height: Length,
    /// Rounds each corner with this radius; left out for sharp corners.
    #[serde(default)]
    pub corner_radius: Option<Length>,
}

impl Command for ProjectNew {
    const META: Meta = Meta {
        id: "project.new",
        title: "New Project",
        description: "Creates a Project called `name` in the folder `<path>/<name>/`, writes \
                      it to disk and opens it: a first Schematic Sheet titled \"Sheet 1\", a \
                      PCB Document whose rules are seeded from `fab_preset` (default \
                      Generic), optionally a rectangular Board Outline (`board`), and one \
                      Output Job named after the preset. A folder that exists and is not \
                      empty is refused.",
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
        let preset_name = p.fab_preset.as_deref().unwrap_or(reda_output::GENERIC_NAME);
        let preset = fab_presets::find_or_fail("/fab_preset", preset_name, cx.user_dirs())?.preset;
        let outline = p
            .board
            .map(|b| reda_pcb::rect_outline("/board", b.width, b.height, b.corner_radius))
            .transpose()?;
        check_free_folder(&folder)?;

        let project = build_project(cx, p.name, preset, outline)?;
        std::fs::create_dir_all(&folder)
            .map_err(|e| CommandError::internal(format!("creating {}: {e}", folder.display())))?;
        store::save(&folder, &project, None)?;

        let session = cx.session()?;
        session.history.clear();
        session.history.mark_all_saved();
        if let Some(dirs) = &session.user_dirs {
            let name = project.fab_presets().next().map(|p| p.name.clone());
            // The dialog's default is a convenience; failing to remember it fails nothing.
            let _ = state::update(dirs, |s| s.last_fab_preset = name);
        }
        session.files_on_disk = files_of(&project);
        let mut project = project;
        take_open_elements(session, &mut project);
        session.project = Some(project);
        session.location = Some(folder);
        Ok(summary(session, vec![]))
    }
}

/// Moves the Library elements open in the old Project, or on the workbench, into
/// `project`: they stay open, with their undo stacks, while Projects come and go.
fn take_open_elements(session: &mut Session, project: &mut Project) {
    let old = session.project.take().or(session.workbench.take());
    for (source, element) in old.iter().flat_map(|p| p.elements()) {
        // A Project fresh from disk has no open elements, so this cannot clash.
        let _ = project.open_element(source, element.clone());
    }
}

/// A new Project in memory, built in one transaction with the same builders as
/// `document.add`, `fab_preset.add`, `output.job.add` and `pcb.board_outline.add_rect`.
fn build_project(
    cx: &mut Context<'_>,
    name: String,
    preset: FabPreset,
    outline: Option<Vec<Vertex>>,
) -> Result<Project, CommandError> {
    let mut project = Project::new(ProjectProps {
        uuid: cx.new_id(),
        name: name.clone(),
        documents: vec![],
        libraries: vec![],
        properties: Default::default(),
        erc: Default::default(),
        schematic: Default::default(),
    });
    let sheet = DocumentEntry {
        uuid: cx.new_id(),
        kind: DocumentKind::Schematic,
        name: name.clone(),
        title: FIRST_SHEET_TITLE.into(),
    };
    let pcb = DocumentEntry {
        uuid: cx.new_id(),
        kind: DocumentKind::Pcb,
        name: name.clone(),
        title: name,
    };
    let board = reda_pcb::new_board(pcb.uuid, preset.rules, || cx.new_id());
    let copy = ProjectFabPreset::new(cx.new_id(), preset);
    let job = reda_output::new_job(cx.new_id(), copy.name.clone(), pcb.uuid, copy.uuid);
    let outline = outline.map(|vertices| {
        let outline = BoardOutline {
            uuid: cx.new_id(),
            vertices,
        };
        Object::BoardOutline(InDocument::new(pcb.uuid, outline))
    });

    let mut tx = project.transaction();
    add_document(&mut tx, sheet)?;
    add_document(&mut tx, pcb)?;
    for object in board.into_iter().chain(outline) {
        tx.insert(object)?;
    }
    tx.insert(Object::FabPreset(copy))?;
    tx.insert(Object::OutputJob(job))?;
    tx.commit()?;
    Ok(project)
}

pub struct ProjectOpen;

#[derive(Deserialize, JsonSchema)]
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
                      Project already open is closed without saving. Missing Document files \
                      are reported in `missing`: a PCB Document is left out, which dirties \
                      the Project file; a Schematic Sheet is recreated empty and unsaved.",
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
        let mut opened = store::open(&store::resolve_project_file(&p.path))?;
        super::zone_fill::load_caches(&mut opened.project, &opened.folder);
        let session = cx.session()?;
        session.files_on_disk = files_of(&opened.project);
        take_open_elements(session, &mut opened.project);
        session.project = Some(opened.project);
        session.location = Some(opened.folder);
        session.history.clear();
        for entry in &opened.missing {
            let project = session.project.as_ref().expect("just opened");
            if project.document(entry.uuid).is_some() {
                // A recreated sheet: saving writes its file again.
                session.history.mark_unsaved(Scope::Document(entry.uuid));
            } else {
                // The open Project no longer lists what the Project file on disk lists.
                session.history.mark_unsaved(Scope::Project);
            }
        }
        Ok(summary(session, opened.missing))
    }
}

pub struct ProjectSave;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectSaveParams {
    /// Saves only this file: a Document's UUID, or `"project"` for the Project file.
    /// Omitted, every file is saved (Save All).
    #[serde(default)]
    pub document: Option<DocumentRef>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ProjectSaveResult {
    /// The Project file.
    pub path: PathBuf,
    /// Every file written, relative to the Project folder.
    pub written: Vec<String>,
    /// Files of renamed Documents moved to their new names; left out when none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub renamed: Vec<RenamedFile>,
    /// Files of deleted Documents sent to the trash; left out when none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trashed: Vec<String>,
}

/// A file renamed on save, relative to the Project folder.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RenamedFile {
    pub from: String,
    pub to: String,
}

impl Command for ProjectSave {
    const META: Meta = Meta {
        id: "project.save",
        title: "Save All",
        description: "Writes the Project file and every Document to the Project folder, \
                      each through a temp file and a rename. With `document`, writes only \
                      that Document's file, or only the Project file for `\"project\"`. \
                      Saving the Project file also renames the files of renamed Documents \
                      and sends those of deleted Documents to the OS trash.",
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
        let trash = session.trash.clone();
        let saved = store::save_tracked(
            &folder,
            project,
            only,
            &mut session.files_on_disk,
            trash.as_deref(),
        )?;
        let path = store::project_file_path(&folder, &project.props().name);
        match only {
            Some(scope) => session.history.mark_saved(scope),
            // Open Library elements are saved to their Libraries, not with the Project.
            None => {
                session.history.mark_saved(Scope::Project);
                for document in project.documents() {
                    session.history.mark_saved(Scope::Document(document.uuid));
                }
            }
        }
        Ok(ProjectSaveResult {
            path,
            written: saved.written,
            renamed: (saved.renamed.into_iter())
                .map(|(from, to)| RenamedFile { from, to })
                .collect(),
            trashed: saved.trashed,
        })
    }
}

pub struct ProjectClose;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectCloseParams {}

/// What `project.close` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ProjectCloseResult {
    /// The Project file of the Project closed.
    pub path: PathBuf,
}

impl Command for ProjectClose {
    const META: Meta = Meta {
        id: "project.close",
        title: "Close Project",
        description: "Closes the open Project without saving: unsaved changes to its files \
                      are dropped, and its files stay as last saved. Library elements open \
                      for editing stay open, with their unsaved changes and undo history.",
        category: "project",
        icon: "close",
        kind: Kind::File,
    };
    type Params = ProjectCloseParams;
    type Output = ProjectCloseResult;

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
        _: ProjectCloseParams,
    ) -> Result<ProjectCloseResult, CommandError> {
        let session = cx.session()?;
        let project = session
            .project
            .take()
            .ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
        let folder = session.location.take().unwrap_or_default();
        let path = store::project_file_path(&folder, &project.props().name);
        session.files_on_disk.clear();
        // Only the open Library elements' stacks stay.
        session.history.clear();
        for (source, element) in project.elements() {
            // The workbench starts empty, so this cannot clash.
            let _ = super::elements::holder(session).open_element(source, element.clone());
        }
        Ok(ProjectCloseResult { path })
    }
}

/// Every Document's file name, by UUID.
fn files_of(project: &Project) -> BTreeMap<Uuid, String> {
    project
        .documents()
        .map(|d| (d.uuid, d.file_name()))
        .collect()
}

/// Checks that `name` can be a file stem on every OS REDA runs on.
pub(crate) fn check_file_stem(name: &str) -> Result<(), String> {
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
