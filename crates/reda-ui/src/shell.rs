//! The window shell's state, kept apart from drawing so it can be tested without a
//! window: the docking [`Layout`] and the open Document tabs, opening and creating
//! Projects, recent Projects, and running Commands on the engine.
//!
//! Every change to the design or the layout goes through [`Shell::run`], which runs a
//! Command and then refreshes what the tabs show (their Scenes) and the fonts the engine
//! has. The Layout lives in the engine's view state, where the layout Commands change it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use reda_engine::command::{CommandError, FontFace, Kind};
use reda_engine::model::geom::Point;
use reda_engine::model::{
    DocumentKind, HERSHEY_FONT, LayerKind, NOTO_SANS_FONT, Scope, Severity, Side, Uuid, Violation,
};
use reda_engine::render::{LayerView, board_composites};
use reda_engine::{Engine, preferences, state, store};
use serde_json::{Value, json};

use crate::canvas::{Camera, SceneHandle, SelectionHandle};
use crate::keymap::{self, ActiveTool, Keymap};
use crate::layout::{self, Layout, TabKey};
use crate::messages::{
    self, CheckKind, CheckRun, Entry, LineKey, Marker, MessageGroup, Messages, Note,
};
use crate::selection::{self, Selection};
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

/// A Document open in a tab.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentTab {
    pub document: Uuid,
    pub title: String,
    /// The canvas camera; `None` until the canvas first lays out, which fits the Document.
    pub camera: Option<Camera>,
    /// What the canvas draws.
    pub scene: SceneHandle,
    /// A box to zoom the camera to when the canvas next lays out, in Document nm.
    pub zoom_to: Option<(Point, Point)>,
    /// The selected objects the canvas marks.
    pub selection: SelectionHandle,
    /// The Violation markers the canvas draws over the Document.
    pub markers: Arc<[Marker]>,
}

impl DocumentTab {
    fn new(document: Uuid, title: String, scene: SceneHandle) -> DocumentTab {
        DocumentTab {
            document,
            title,
            camera: None,
            scene,
            zoom_to: None,
            selection: SelectionHandle::default(),
            markers: Arc::from([]),
        }
    }
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
    messages: Messages,
    /// The [`Selection`]'s objects as the canvases mark them, renewed when it changes.
    marked: SelectionHandle,
    keymap: Keymap,
}

impl Shell {
    /// A shell on `engine`, showing its open Project or else the Start page, in the
    /// Default Layout. It adds the layout Commands to the engine.
    pub fn new(mut engine: Engine) -> Shell {
        if engine.registry().get("panel.show").is_none() {
            engine.register(layout::register_all);
        }
        if engine.registry().get("selection.set").is_none() {
            engine.register(selection::register_all);
        }
        engine.view_mut().set(Layout::default());
        engine.view_mut().set(Selection::default());
        engine.view_mut().set(ActiveTool::default());
        crate::bars::install(&mut engine);
        let mut shell = Shell {
            engine,
            documents: Vec::new(),
            fonts: Arc::new(NoFonts),
            asked_fonts: BTreeSet::new(),
            generation: 0,
            message: None,
            messages: Messages::default(),
            marked: SelectionHandle::default(),
            keymap: Keymap::default(),
        };
        shell.load_preference_warnings();
        shell.reload_keymap();
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

    /// What is selected; it changes through `selection.set`.
    pub fn selection(&self) -> &Selection {
        self.engine
            .view()
            .get::<Selection>()
            .expect("the Shell installs its Selection")
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

    /// Shows `message` in the status bar, as for a failed Command.
    pub fn report(&mut self, message: String) {
        self.message = Some(message);
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

    /// Opens the Project at `path`, replacing any open one. `path` is its `.reda` file or
    /// folder, or one of its Document files (`.redasch`, `.redapcb`), which opens the
    /// Project with that Document's tab focused.
    pub fn open_project(&mut self, path: &Path) -> Result<(), CommandError> {
        let document = document_file(path);
        let project = match &document {
            Some(_) => path.parent().unwrap_or(path),
            None => path,
        };
        self.run_reporting("project.open", json!({ "path": project }))?;
        if let Some((stem, kind)) = document {
            let uuid = self.engine.project().and_then(|p| {
                p.documents()
                    .find(|d| d.kind == kind && d.name == stem)
                    .map(|d| d.uuid)
            });
            if let Some(uuid) = uuid {
                self.run_reporting("document.open", json!({ "document": uuid }))?;
            }
        }
        Ok(())
    }

    /// The Fab Presets a new Project can start from (shipped, then the user's) and the
    /// default pick: the last one used, else Generic.
    pub fn fab_preset_choices(&mut self) -> (Vec<String>, String) {
        let list = self
            .engine
            .run("fab_preset.list", json!({}))
            .unwrap_or_default();
        let names = list["presets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["preset"]["name"].as_str().map(str::to_owned))
            .collect();
        let default = list["default"].as_str().unwrap_or("Generic").to_owned();
        (names, default)
    }

    /// Creates the Project `name` in a new folder inside `folder`, and opens it.
    pub fn new_project(&mut self, folder: &Path, name: &str) -> Result<(), CommandError> {
        self.run_reporting("project.new", json!({ "path": folder, "name": name }))?;
        Ok(())
    }

    /// Runs a Command, then refreshes the tabs when it may have changed the design or
    /// closed a Document's tab. Opening, creating or saving a Project puts it at the top
    /// of the recent Projects.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let out = self.engine.run(id, params)?;
        let base = id.split(':').next().unwrap_or(id);
        match base {
            "project.open" | "project.new" => self.show_project(),
            _ => match self.engine.registry().get(base).map(|meta| meta.kind) {
                Some(Kind::Query) => {}
                Some(Kind::View) => self.sync_document_tabs(),
                _ => self.refresh(),
            },
        }
        self.sync_selection();
        if matches!(base, "project.open" | "project.new" | "project.save") {
            self.remember_project();
        }
        Ok(out)
    }

    /// The Undo stack `Mod+Z` uses: a focused Document tab's own, else the Project's.
    /// The Projects panel (and any Panel that is not about one Document) uses the
    /// Project stack, and so do the Project dialogs, which the window says by
    /// `project_dialog`.
    pub fn undo_scope(&self, project_dialog: bool) -> Option<Scope> {
        self.engine.project()?;
        if project_dialog {
            return Some(Scope::Project);
        }
        match &self.layout().focus {
            Some(TabKey::Document(uuid)) => Some(Scope::Document(*uuid)),
            _ => Some(Scope::Project),
        }
    }

    /// Undoes (or with `redo`, redoes) on the stack [`Shell::undo_scope`] picks.
    pub fn undo(&mut self, redo: bool, project_dialog: bool) -> Result<Value, CommandError> {
        let Some(scope) = self.undo_scope(project_dialog) else {
            return Err(CommandError::unavailable("no Project is open"));
        };
        let document = match scope {
            Scope::Document(uuid) => json!(uuid),
            _ => json!("project"),
        };
        let id = if redo { "edit.redo" } else { "edit.undo" };
        self.run_reporting(id, json!({ "document": document }))
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

    /// Saves the session's Journal to `path` as a script that `reda run --script`
    /// replays. View Commands are left out: only the app can run them. A failure is also
    /// kept as the message the status bar shows.
    pub fn export_journal(&mut self, path: &Path) -> std::io::Result<()> {
        let script = crate::journal_panel::script(&self.engine);
        let out = reda_engine::fs::write_atomic(path, script.as_bytes());
        if let Err(e) = &out {
            self.message = Some(tr!("journal-export-failed", error = e.to_string()));
        }
        out
    }

    // ---- Messages ----

    /// What the Messages panel lists.
    pub fn messages(&self) -> &Messages {
        &self.messages
    }

    /// The Messages panel's lines, grouped by Document.
    pub fn message_groups(&self) -> Vec<MessageGroup> {
        self.messages.groups(self.engine.project())
    }

    /// Runs ERC or DRC (`erc.run`/`drc.run`) and shows its Violations, waived ones
    /// included so the panel can toggle them. An error goes to the status bar.
    pub fn run_check(&mut self, kind: CheckKind) -> Result<(), CommandError> {
        let out = self.run_reporting(
            &format!("{}.run", kind.id()),
            json!({"include_waived": true}),
        )?;
        let violations: Vec<Violation> =
            serde_json::from_value(out["violations"].clone()).unwrap_or_default();
        let project = self.engine.project();
        let entries = violations
            .into_iter()
            .map(|violation| Entry {
                document: project.and_then(|p| messages::violation_document(p, &violation)),
                text: messages::violation_text(&violation),
                violation,
            })
            .collect();
        self.messages.set_run(CheckRun {
            kind,
            entries,
            design: out["design"].as_str().unwrap_or_default().to_owned(),
            stale: false,
        });
        self.update_markers();
        Ok(())
    }

    /// Lists waived Violations too, or hides them again.
    pub fn set_show_waived(&mut self, show: bool) {
        self.messages.set_show_waived(show);
        self.update_markers();
    }

    /// Goes to a line's Violation: opens its Document (`document.open`), zooms to it and
    /// selects the objects involved (`selection.set`). A load message about a Document
    /// opens that Document.
    pub fn go_to(&mut self, key: LineKey) {
        let (document, objects) = match key {
            LineKey::Violation(..) => match self.messages.entry(key) {
                Some(e) => (e.document, e.violation.objects.clone()),
                None => return,
            },
            LineKey::Note(i) => match self.messages.notes().get(i) {
                Some(n) => (n.document, Vec::new()),
                None => return,
            },
        };
        self.messages.set_current(Some(key));
        if let Some(document) = document
            && self
                .run_reporting("document.open", json!({ "document": document }))
                .is_ok()
        {
            let location = self.messages.entry(key).and_then(|e| {
                let tab = self.document_tab(document)?;
                let project = self.engine.project()?;
                messages::locate(project, document, &tab.scene.scene, &e.violation)
            });
            if let (Some(location), Some(tab)) = (location, self.document_tab_mut(document)) {
                tab.zoom_to = Some(location.frame);
            }
        }
        // `selection.set` takes only objects the Project has; a Violation may also name
        // what has no UUID of its own in the model.
        let objects: Vec<Uuid> = match self.engine.project() {
            Some(project) => objects
                .into_iter()
                .filter(|o| project.object(*o).is_some())
                .collect(),
            None => Vec::new(),
        };
        let _ = self.run_reporting("selection.set", json!({ "objects": objects }));
        self.update_markers();
    }

    /// Waives a line's Violation (`erc.waive`/`drc.waive`) and runs the check again.
    pub fn waive(&mut self, key: LineKey) -> Result<(), CommandError> {
        let Some((kind, v)) = self.violation_of(key) else {
            return Ok(());
        };
        self.run_reporting(
            &format!("{}.waive", kind.id()),
            json!({"check": v.check, "objects": v.objects}),
        )?;
        self.run_check(kind)
    }

    /// Deletes the Waiver waiving a line's Violation and runs the check again.
    pub fn unwaive(&mut self, key: LineKey) -> Result<(), CommandError> {
        let Some((kind, v)) = self.violation_of(key) else {
            return Ok(());
        };
        let Some(waiver) = v.waiver else {
            return Ok(());
        };
        self.run_reporting(
            &format!("{}.unwaive", kind.id()),
            json!({"waivers": [waiver]}),
        )?;
        self.run_check(kind)
    }

    /// Sets the severity of a line's check (`erc.set_severity`/`drc.set_severity`) and
    /// runs the check again.
    pub fn set_severity(&mut self, key: LineKey, severity: Severity) -> Result<(), CommandError> {
        let Some((kind, v)) = self.violation_of(key) else {
            return Ok(());
        };
        self.run_reporting(
            &format!("{}.set_severity", kind.id()),
            json!({"check": v.check, "severity": severity}),
        )?;
        self.run_check(kind)
    }

    fn violation_of(&self, key: LineKey) -> Option<(CheckKind, Violation)> {
        match key {
            LineKey::Violation(kind, _) => {
                Some((kind, self.messages.entry(key)?.violation.clone()))
            }
            LineKey::Note(_) => None,
        }
    }

    /// Replaces the load errors and warnings `source` reports (such as `keymap`), each
    /// about the whole application.
    pub fn set_load_messages(&mut self, source: &str, messages: Vec<(Severity, String)>) {
        let notes = messages
            .into_iter()
            .map(|(severity, text)| Note {
                source: source.to_owned(),
                document: None,
                severity,
                text,
            })
            .collect();
        self.messages.set_notes(source, notes);
    }

    /// The Violation markers on `document`'s canvas; none when it is not open.
    pub fn markers(&self, document: Uuid) -> Arc<[Marker]> {
        self.document_tab(document)
            .map(|t| t.markers.clone())
            .unwrap_or_else(|| Arc::from([]))
    }

    /// Marks check results stale once the design differs from the run's.
    fn mark_stale_runs(&mut self) {
        let Some(project) = self.engine.project() else {
            return;
        };
        let erc = reda_engine::schematic::erc::design_fingerprint(project);
        let drc = project.pcb().map(reda_engine::pcb::drc::design_fingerprint);
        if let Some(run) = self.messages.run_mut(CheckKind::Erc) {
            run.stale = run.design != erc;
        }
        if let Some(run) = self.messages.run_mut(CheckKind::Drc) {
            run.stale = drc.as_ref() != Some(&run.design);
        }
    }

    /// Recomputes every open tab's Violation markers.
    fn update_markers(&mut self) {
        let Some(project) = self.engine.project() else {
            return;
        };
        let current = self.messages.current();
        for tab in &mut self.documents {
            let markers: Vec<Marker> = self
                .messages
                .listed()
                .filter(|(_, e)| e.document == Some(tab.document))
                .filter_map(|(key, e)| {
                    let location =
                        messages::locate(project, tab.document, &tab.scene.scene, &e.violation)?;
                    Some(Marker {
                        at: location.at,
                        severity: e.violation.severity,
                        waived: e.violation.waiver.is_some(),
                        current: current == Some(key),
                    })
                })
                .collect();
            tab.markers = Arc::from(markers);
        }
    }

    /// The Keymap in force: the base Keymap the Preferences pick, with the user's
    /// `keymap.toml` laid over it.
    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// Makes `tool` the active Tool (`None`: no Tool), whose `tool:<name>` Keymap context
    /// then comes first. Tool Commands can set [`ActiveTool`] in the view state instead.
    pub fn set_active_tool(&mut self, tool: Option<&str>) {
        self.engine
            .view_mut()
            .set(ActiveTool(tool.map(str::to_owned)));
    }

    /// Reads the Keymap again (after `keymap.toml` or `keymap.base` changed) and lists its
    /// problems in Messages, replacing the earlier ones.
    pub fn reload_keymap(&mut self) {
        let dirs = self.engine.user_dirs();
        let base = preferences::current(dirs).keymap.base;
        let mut problems = Vec::new();
        let user = dirs.and_then(|d| {
            let path = d.config.join(keymap::USER_FILE);
            match std::fs::read_to_string(&path) {
                Ok(text) => Some(text),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    problems.push(tr!(
                        "keymap-unreadable",
                        file = keymap::USER_FILE,
                        error = e.to_string()
                    ));
                    None
                }
            }
        });
        let (mut map, found) = Keymap::load(base, user.as_deref());
        problems.extend(found);
        problems.extend(map.check(self.engine.registry()));
        self.keymap = map;
        let problems = problems
            .into_iter()
            .map(|text| (Severity::Error, text))
            .collect();
        self.set_load_messages(KEYMAP, problems);
    }

    /// Lists the warnings reading `preferences.toml` gave.
    fn load_preference_warnings(&mut self) {
        let Some(dirs) = self.engine.user_dirs() else {
            return;
        };
        let warnings = preferences::load(dirs).warnings;
        let notes = warnings
            .into_iter()
            .map(|w| Note {
                source: PREFERENCES.to_owned(),
                document: None,
                severity: Severity::Warning,
                text: tr!("message-preferences", warning = w),
            })
            .collect();
        self.messages.set_notes(PREFERENCES, notes);
    }

    /// Puts the open Project at the top of the recent Projects in `state.json`.
    fn remember_project(&mut self) {
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

    /// Shows the open Project in the Default Layout, with its first sheet in the centre.
    fn show_project(&mut self) {
        let first_sheet = self.engine.project().and_then(|p| {
            p.documents()
                .find(|d| d.kind == DocumentKind::Schematic)
                .map(|d| (d.uuid, d.title.clone()))
        });
        let mut layout = Layout::with_centre(Vec::new());
        self.documents.clear();
        if let Some((document, title)) = first_sheet {
            layout.open_document(document);
            let scene = self.scene_of(document);
            self.documents
                .push(DocumentTab::new(document, title, scene));
        }
        *self.layout_mut() = layout;
        self.engine.view_mut().set(Selection::default());
        self.sync_selection();
        self.messages.clear_runs();
        self.load_fonts();
    }

    /// Rebuilds every tab's Scene and closes tabs whose Document is gone. With no
    /// Project open, the centre shows the Start page again.
    fn refresh(&mut self) {
        let Some(project) = self.engine.project() else {
            self.documents.clear();
            *self.layout_mut() = Layout::default();
            self.messages.clear_runs();
            self.messages.set_notes(FONTS, Vec::new());
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
        self.mark_stale_runs();
        self.update_markers();
    }

    /// Matches the Document tabs to the layout after a view Command: drops the state of
    /// closed tabs and starts it for tabs `document.open` added.
    fn sync_document_tabs(&mut self) {
        self.forget_closed_documents();
        let opened: Vec<Uuid> = self
            .layout()
            .groups()
            .into_iter()
            .flat_map(|(_, g)| g.tabs.iter())
            .filter_map(|t| match t {
                TabKey::Document(uuid) if self.document_tab(*uuid).is_none() => Some(*uuid),
                _ => None,
            })
            .collect();
        for document in opened {
            let title = self
                .engine
                .project()
                .and_then(|p| p.document(document))
                .map(|d| d.title.clone())
                .unwrap_or_default();
            let scene = self.scene_of(document);
            self.documents
                .push(DocumentTab::new(document, title, scene));
        }
        self.sync_selection();
    }

    /// Hands every canvas the [`Selection`]'s objects to mark, with a new generation
    /// when they changed.
    fn sync_selection(&mut self) {
        if self.marked.objects[..] != self.selection().objects[..] {
            let objects = Arc::from(self.selection().objects.as_slice());
            self.generation += 1;
            self.marked = SelectionHandle {
                generation: self.generation,
                objects,
            };
        }
        for tab in &mut self.documents {
            if tab.selection.generation != self.marked.generation {
                tab.selection = self.marked.clone();
            }
        }
    }

    /// Drops the tab state of Documents whose tabs the layout no longer has.
    fn forget_closed_documents(&mut self) {
        let mut documents = std::mem::take(&mut self.documents);
        documents.retain(|d| self.layout().place(&TabKey::Document(d.document)).is_some());
        self.documents = documents;
    }

    fn scene_of(&mut self, document: Uuid) -> SceneHandle {
        self.generation += 1;
        let scene = self.engine.document_scene(document).unwrap_or_default();
        // Until the Layers panel and view state exist (#89-#92), a board shows every Layer
        // with the top copper active and the Ratsnest visible.
        let layers = self
            .engine
            .project()
            .and_then(|p| p.pcb_document(document))
            .map(|pcb| {
                let set = pcb.layers();
                let view = LayerView {
                    active: set.find(LayerKind::Copper, Some(Side::Top)).map(|l| l.uuid),
                    ratsnest: true,
                    ..LayerView::default()
                };
                board_composites(&set.layers, &view).into()
            });
        SceneHandle {
            generation: self.generation,
            scene: Arc::new(scene),
            layers,
        }
    }

    /// Hands the engine the installed fonts that PCB text names and it does not have yet.
    fn load_fonts(&mut self) {
        let Some(pcb) = self.engine.project().and_then(|p| p.pcb()) else {
            self.messages.set_notes(FONTS, Vec::new());
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
        let missing = reda_engine::pcb::missing_fonts(pcb_of(&self.engine), self.engine.fonts());
        let document = pcb_of(&self.engine).uuid;
        let notes = missing
            .into_iter()
            .map(|font| Note {
                source: FONTS.to_owned(),
                document: Some(document),
                severity: Severity::Warning,
                text: tr!("message-missing-font", font = font),
            })
            .collect();
        self.messages.set_notes(FONTS, notes);
    }
}

/// The stem and kind of `path` when it names a Document file rather than a Project.
fn document_file(path: &Path) -> Option<(String, DocumentKind)> {
    let kind = DocumentKind::from_extension(path.extension()?.to_str()?)?;
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    Some((stem, kind))
}

/// The `source` of missing-font warnings.
const FONTS: &str = "fonts";
/// The `source` of Preference warnings.
const PREFERENCES: &str = "preferences";
const KEYMAP: &str = "keymap";

/// The open Project's PCB Document; only called when there is one.
fn pcb_of(engine: &Engine) -> &reda_engine::model::PcbDocument {
    engine
        .project()
        .and_then(|p| p.pcb())
        .expect("checked by the caller")
}
