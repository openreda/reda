//! The window shell's state, kept apart from drawing so it can be tested without a
//! window: the docking [`Layout`] and the open Document tabs, opening and creating
//! Projects, recent Projects, and running Commands on the engine.
//!
//! Every change to the design or the layout goes through [`Shell::run`], which runs a
//! Command and then refreshes what the tabs show (their Scenes) and the fonts the engine
//! has. The Layout lives in the engine's view state, where the layout Commands change it.
//!
//! Each Project's view (the layout and each Document's camera and Layers) is kept in its
//! `.reda/view.json`: see [`view_state`](crate::view_state).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use reda_engine::command::{CommandError, FontFace, Kind};
use reda_engine::model::{DocumentKind, HERSHEY_FONT, NOTO_SANS_FONT, Uuid};
use reda_engine::{Engine, preferences, state, store};
use serde_json::{Value, json};

use crate::canvas::{Camera, SceneHandle};
use crate::layout::{self, Layout, TabKey, saved};
use crate::tr;
use crate::view_state::{self, Debounce, DocumentView, ViewFile};

pub use crate::view_state::{LayerView, VIEW_SAVE_DELAY};

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

/// A Document open in a tab.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentTab {
    pub document: Uuid,
    pub title: String,
    /// The canvas camera; `None` until the canvas first lays out, which fits the Document.
    pub camera: Option<Camera>,
    /// Which Layers show, and how inactive ones are drawn.
    pub layers: LayerView,
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
    /// The open Documents' tabs, wherever the Layout puts them.
    documents: Vec<DocumentTab>,
    fonts: Arc<dyn InstalledFonts>,
    /// Families already asked of [`InstalledFonts`], found or not.
    asked_fonts: BTreeSet<String>,
    generation: u64,
    message: Option<String>,
    /// The saved view of Documents without a tab, kept for when `view.json` is next saved.
    closed_views: BTreeMap<Uuid, DocumentView>,
    view_saves: Debounce,
}

impl Shell {
    /// A shell on `engine`, showing its open Project or else the Start page, in the
    /// Default Layout. It adds the layout Commands to the engine.
    pub fn new(mut engine: Engine) -> Shell {
        if engine.registry().get("panel.show").is_none() {
            let dirs = engine.user_dirs().cloned();
            engine.register(|r| layout::register_with_layouts(r, dirs));
        }
        engine.view_mut().set(Layout::default());
        let mut shell = Shell {
            engine,
            documents: Vec::new(),
            fonts: Arc::new(NoFonts),
            asked_fonts: BTreeSet::new(),
            generation: 0,
            message: None,
            closed_views: BTreeMap::new(),
            view_saves: Debounce::default(),
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

    /// The docking layout: where every tab is.
    pub fn layout(&self) -> &Layout {
        self.engine
            .view()
            .get::<Layout>()
            .expect("the Shell installs its Layout")
    }

    fn layout_mut(&mut self) -> &mut Layout {
        self.engine
            .view_mut()
            .get_mut::<Layout>()
            .expect("the Shell installs its Layout")
    }

    /// The open Documents' tabs.
    pub fn document_tabs(&self) -> &[DocumentTab] {
        &self.documents
    }

    /// The tab of the open Document `document`.
    pub fn document_tab(&self, document: Uuid) -> Option<&DocumentTab> {
        self.documents.iter().find(|d| d.document == document)
    }

    /// The tab of an open Document, to update its camera.
    pub fn document_tab_mut(&mut self, document: Uuid) -> Option<&mut DocumentTab> {
        self.documents.iter_mut().find(|d| d.document == document)
    }

    /// The active Document's tab: the Document last interacted with.
    pub fn active_document(&self) -> Option<&DocumentTab> {
        self.document_tab(self.layout().active_document()?)
    }

    pub fn active_document_mut(&mut self) -> Option<&mut DocumentTab> {
        let document = self.layout().active_document()?;
        self.document_tab_mut(document)
    }

    /// The title of `tab`'s Document, if it is an open Document.
    pub fn document_title(&self, tab: &TabKey) -> Option<&str> {
        match tab {
            TabKey::Document(uuid) => self.document_tab(*uuid).map(|d| d.title.as_str()),
            _ => None,
        }
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

    /// Opens the last Project, when the Preferences ask for it. A last Project that is
    /// gone is not reopened, and the status bar says so.
    pub fn reopen_last_project(&mut self) {
        let Some(dirs) = self.engine.user_dirs() else {
            return;
        };
        if !preferences::current(Some(dirs)).general.reopen_last_project {
            return;
        }
        let Some(last) = self.recent_projects().into_iter().next() else {
            return;
        };
        if last.missing {
            self.message = Some(tr!(
                "status-last-project-missing",
                project = last.name.as_str(),
                path = last.path.display().to_string()
            ));
            return;
        }
        let _ = self.open_project(&last.path);
    }

    /// Saves the open Project's view state now, as when it closes.
    pub fn save_view(&mut self) {
        let Some((folder, bytes)) = self.view_bytes() else {
            return;
        };
        // The view state is disposable: a failed write loses nothing worth reporting.
        if view_state::write(&folder, &bytes).is_ok() {
            self.view_saves.saved(bytes);
        }
    }

    /// Saves the open Project's view state once it has stayed unchanged for
    /// [`VIEW_SAVE_DELAY`]. The window calls this every frame.
    pub fn save_view_if_due(&mut self, now: Instant) {
        let Some((folder, bytes)) = self.view_bytes() else {
            return;
        };
        if let Some(bytes) = self.view_saves.poll(bytes, now)
            && view_state::write(&folder, &bytes).is_ok()
        {
            self.view_saves.saved(bytes);
        }
    }

    /// The open Project's folder and its view state as `view.json` bytes.
    fn view_bytes(&self) -> Option<(PathBuf, Vec<u8>)> {
        let folder = self.engine.location()?.to_path_buf();
        let project = self.engine.project()?;
        let mut documents = self.closed_views.clone();
        documents.retain(|uuid, _| project.documents().any(|d| d.uuid == *uuid));
        for tab in &self.documents {
            documents.insert(
                tab.document,
                DocumentView {
                    camera: tab.camera,
                    layers: tab.layers.clone(),
                },
            );
        }
        let file = ViewFile::new(self.layout().clone(), documents);
        Some((folder, file.encode()))
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

    /// Runs a Command, then refreshes the tabs when it may have changed the design or
    /// closed a Document's tab.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let base = id.split(':').next().unwrap_or(id);
        if matches!(base, "project.open" | "project.new" | "project.close") {
            self.save_view();
        }
        let out = self.engine.run(id, params)?;
        match self.engine.registry().get(base).map(|meta| meta.kind) {
            Some(Kind::Query) => {}
            Some(Kind::View) => self.forget_closed_documents(),
            _ => self.refresh(),
        }
        Ok(out)
    }

    /// [`Shell::run`], keeping any error as the message the status bar shows.
    pub fn run_reporting(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
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

    /// Shows the open Project as its `view.json` left it, or else in the default layout
    /// with its first sheet in the centre.
    fn show_project(&mut self) {
        let Some(project) = self.engine.project() else {
            return;
        };
        let titles: BTreeMap<Uuid, String> = project
            .documents()
            .map(|d| (d.uuid, d.title.clone()))
            .collect();
        let first_sheet = project
            .documents()
            .find(|d| d.kind == DocumentKind::Schematic)
            .map(|d| d.uuid);
        let restored = self.engine.location().and_then(view_state::read);
        let (layout, mut views) = match restored {
            Some(file) => {
                let mut layout = file.layout;
                layout.retain_tabs(|t| match t {
                    TabKey::Document(uuid) => titles.contains_key(uuid),
                    TabKey::Start => false,
                    TabKey::Panel(_) => true,
                });
                (layout, file.documents)
            }
            None => (self.default_layout(first_sheet), BTreeMap::new()),
        };
        self.documents.clear();
        let open: Vec<Uuid> = layout
            .groups()
            .into_iter()
            .flat_map(|(_, g)| g.tabs.iter())
            .filter_map(|t| match t {
                TabKey::Document(uuid) => Some(*uuid),
                _ => None,
            })
            .collect();
        for document in open {
            let view = views.remove(&document).unwrap_or_default();
            let scene = self.scene_of(document);
            self.documents.push(DocumentTab {
                document,
                title: titles[&document].clone(),
                camera: view.camera,
                layers: view.layers,
                scene,
            });
        }
        self.closed_views = views;
        *self.layout_mut() = layout;
        self.load_fonts();
        let bytes = self.view_bytes().map(|(_, bytes)| bytes);
        self.view_saves = Debounce::default();
        if let Some(bytes) = bytes {
            self.view_saves.saved(bytes);
        }
    }

    /// The layout a Project opens in without saved view state: the user's default Saved
    /// Layout when there is a usable one, else the Default Layout, with `document` open.
    fn default_layout(&self, document: Option<Uuid>) -> Layout {
        let mut layout = Layout::with_centre(Vec::new());
        if let Some(document) = document {
            layout.open_document(document);
        }
        let dirs = self.engine.user_dirs();
        let chosen = preferences::current(dirs).general.default_layout;
        let saved = dirs
            .zip(chosen)
            .and_then(|(dirs, name)| saved::load(dirs, &name).ok())
            .and_then(|saved| saved.apply_to(&layout).ok());
        saved.unwrap_or(layout)
    }

    /// Rebuilds every tab's Scene and closes tabs whose Document is gone. With no
    /// Project open, the centre shows the Start page again.
    fn refresh(&mut self) {
        let Some(project) = self.engine.project() else {
            self.documents.clear();
            *self.layout_mut() = Layout::default();
            return;
        };
        let open: Vec<(Uuid, String)> = project
            .documents()
            .map(|d| (d.uuid, d.title.clone()))
            .collect();
        self.layout_mut().retain_tabs(|t| match t {
            TabKey::Start => false,
            TabKey::Document(uuid) => open.iter().any(|(u, _)| u == uuid),
            TabKey::Panel(_) => true,
        });
        self.forget_closed_documents();
        let mut documents = std::mem::take(&mut self.documents);
        for d in &mut documents {
            d.scene = self.scene_of(d.document);
            if let Some((_, title)) = open.iter().find(|(u, _)| *u == d.document) {
                d.title = title.clone();
            }
        }
        self.documents = documents;
        self.load_fonts();
    }

    /// Drops the tab state of Documents whose tabs the layout no longer has.
    fn forget_closed_documents(&mut self) {
        let mut documents = std::mem::take(&mut self.documents);
        documents.retain(|d| self.layout().place(&TabKey::Document(d.document)).is_some());
        self.documents = documents;
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
