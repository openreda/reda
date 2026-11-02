//! The window shell's state, kept apart from drawing so it can be tested without a
//! window: the open tabs (the Start page or Document tabs), opening and creating Projects,
//! recent Projects, and running Commands on the engine.
//!
//! Every change to the design goes through [`Shell::run`], which runs a Command and then
//! refreshes what the tabs show (their Scenes) and the fonts the engine has.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use reda_engine::command::{CommandError, FontFace, Kind};
use reda_engine::model::{DocumentKind, HERSHEY_FONT, NOTO_SANS_FONT, Uuid};
use reda_engine::{Engine, state, store};
use serde_json::{Value, json};

use crate::canvas::{Camera, SceneHandle};
use crate::tr;

/// Where the engine gets the bytes of fonts installed on the machine. The app scans the
/// OS font folders; tests pass a fake.
pub trait InstalledFonts: Send + Sync {
    /// The installed face for `family` (its regular style when there are several), or
    /// `None` when the machine does not have it.
    fn load(&self, family: &str) -> Option<FontFace>;
}

/// No installed fonts: only REDA's bundled ones.
struct NoFonts;

impl InstalledFonts for NoFonts {
    fn load(&self, _family: &str) -> Option<FontFace> {
        None
    }
}

/// A tab in the Document area.
#[derive(Clone, Debug, PartialEq)]
pub enum Tab {
    /// Shown while no Project is open.
    Start,
    Document(DocumentTab),
}

/// A Document open in a tab.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentTab {
    pub document: Uuid,
    pub title: String,
    /// The canvas camera; `None` until the canvas first lays out, which fits the Document.
    pub camera: Option<Camera>,
    /// What the canvas draws.
    pub scene: SceneHandle,
}

/// One row of the Start page's recent Projects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentProject {
    /// The `.reda` file.
    pub path: PathBuf,
    /// The Project name: the file stem.
    pub name: String,
    /// The file is gone; the row is greyed out.
    pub missing: bool,
}

/// The shell: the engine and the tabs showing it.
pub struct Shell {
    engine: Engine,
    tabs: Vec<Tab>,
    active: usize,
    fonts: Arc<dyn InstalledFonts>,
    /// Families already asked of [`InstalledFonts`], found or not.
    asked_fonts: BTreeSet<String>,
    generation: u64,
    message: Option<String>,
}

impl Shell {
    /// A shell on `engine`, showing its open Project or else the Start page.
    pub fn new(engine: Engine) -> Shell {
        let mut shell = Shell {
            engine,
            tabs: vec![Tab::Start],
            active: 0,
            fonts: Arc::new(NoFonts),
            asked_fonts: BTreeSet::new(),
            generation: 0,
            message: None,
        };
        if shell.engine.project().is_some() {
            shell.show_project();
        }
        shell
    }

    /// Takes installed fonts from `fonts` for PCB text that names them.
    pub fn with_fonts(mut self, fonts: Arc<dyn InstalledFonts>) -> Shell {
        self.fonts = fonts;
        self.load_fonts();
        self
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    /// The active tab, to update its camera.
    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    /// The last error to show the user, if any.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn dismiss_message(&mut self) {
        self.message = None;
    }

    /// The recent Projects from `state.json`, newest first.
    pub fn recent_projects(&self) -> Vec<RecentProject> {
        let Some(dirs) = self.engine.user_dirs() else {
            return Vec::new();
        };
        state::read(dirs)
            .recent_projects
            .into_iter()
            .map(|path| RecentProject {
                name: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                missing: !path.is_file(),
                path,
            })
            .collect()
    }

    /// Forgets a recent Project.
    pub fn remove_recent(&mut self, path: &Path) {
        if let Some(dirs) = self.engine.user_dirs()
            && let Err(e) = state::update(dirs, |s| s.recent_projects.retain(|p| p != path))
        {
            self.message = Some(tr!("error-state-file", error = e.to_string()));
        }
    }

    /// Opens the Project at `path` (its `.reda` file or folder), replacing any open one.
    pub fn open_project(&mut self, path: &Path) -> Result<(), CommandError> {
        self.run_reporting("project.open", json!({ "path": path }))?;
        self.after_open();
        Ok(())
    }

    /// Creates the Project `name` in a new folder inside `folder`, and opens it.
    pub fn new_project(&mut self, folder: &Path, name: &str) -> Result<(), CommandError> {
        self.run_reporting("project.new", json!({ "path": folder, "name": name }))?;
        self.after_open();
        Ok(())
    }

    /// Runs a Command, then refreshes the tabs when it may have changed the design.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let out = self.engine.run(id, params)?;
        let base = id.split(':').next().unwrap_or(id);
        let read_only = self
            .engine
            .registry()
            .get(base)
            .is_some_and(|meta| meta.kind == Kind::Query);
        if !read_only {
            self.refresh();
        }
        Ok(out)
    }

    fn run_reporting(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let out = self.run(id, params);
        match &out {
            Ok(_) => self.message = None,
            Err(e) => self.message = Some(tr!("error-command", error = e.message.clone())),
        }
        out
    }

    fn after_open(&mut self) {
        self.show_project();
        if let (Some(dirs), Some(file)) = (self.engine.user_dirs(), self.project_file())
            && let Err(e) = state::update(dirs, |s| s.add_recent_project(&file))
        {
            self.message = Some(tr!("error-state-file", error = e.to_string()));
        }
    }

    /// The open Project's `.reda` file, canonical when it exists.
    fn project_file(&self) -> Option<PathBuf> {
        let folder = self.engine.location()?;
        let name = &self.engine.project()?.props().name;
        let file = store::project_file_path(folder, name);
        Some(std::fs::canonicalize(&file).unwrap_or(file))
    }

    /// Replaces the tabs with the open Project's first sheet.
    fn show_project(&mut self) {
        let first_sheet = self.engine.project().and_then(|p| {
            p.documents()
                .find(|d| d.kind == DocumentKind::Schematic)
                .map(|d| (d.uuid, d.title.clone()))
        });
        self.tabs = match first_sheet {
            Some((document, title)) => vec![Tab::Document(DocumentTab {
                document,
                title,
                camera: None,
                scene: self.scene_of(document),
            })],
            None => Vec::new(),
        };
        self.active = 0;
        self.load_fonts();
    }

    /// Rebuilds every tab's Scene and drops tabs whose Document is gone.
    fn refresh(&mut self) {
        let Some(project) = self.engine.project() else {
            self.tabs = vec![Tab::Start];
            self.active = 0;
            return;
        };
        let open: Vec<(Uuid, String)> = project
            .documents()
            .map(|d| (d.uuid, d.title.clone()))
            .collect();
        let mut tabs = std::mem::take(&mut self.tabs);
        tabs.retain(|t| match t {
            Tab::Start => false,
            Tab::Document(d) => open.iter().any(|(u, _)| *u == d.document),
        });
        for tab in &mut tabs {
            if let Tab::Document(d) = tab {
                d.scene = self.scene_of(d.document);
                if let Some((_, title)) = open.iter().find(|(u, _)| *u == d.document) {
                    d.title = title.clone();
                }
            }
        }
        self.tabs = tabs;
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        self.load_fonts();
    }

    fn scene_of(&mut self, document: Uuid) -> SceneHandle {
        self.generation += 1;
        let scene = self
            .engine
            .project()
            .and_then(|p| reda_engine::document_scene(p, document).ok())
            .unwrap_or_default();
        SceneHandle {
            generation: self.generation,
            scene: Arc::new(scene),
        }
    }

    /// Hands the engine the installed fonts that PCB text names and it does not have yet.
    fn load_fonts(&mut self) {
        let Some(pcb) = self.engine.project().and_then(|p| p.pcb()) else {
            return;
        };
        let wanted: BTreeSet<String> = pcb
            .texts()
            .map(|t| t.font.clone())
            .filter(|f| f != HERSHEY_FONT && f != NOTO_SANS_FONT)
            .filter(|f| !self.asked_fonts.contains(f))
            .collect();
        for family in wanted {
            if let Some(face) = self.fonts.load(&family) {
                self.engine
                    .fonts_mut()
                    .add_face(family.clone(), face.bytes, face.index);
            }
            self.asked_fonts.insert(family);
        }
    }
}
