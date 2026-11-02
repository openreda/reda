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
use reda_engine::model::{DocumentKind, HERSHEY_FONT, NOTO_SANS_FONT, Severity, Uuid, Violation};
use reda_engine::{Engine, preferences, state, store};
use serde_json::{Value, json};

use crate::canvas::{Camera, SceneHandle, SelectionHandle};
use crate::layout::{self, Layout, TabKey};
use crate::messages::{
    self, CheckKind, CheckRun, Entry, LineKey, Marker, MessageGroup, Messages, Note,
};
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
    selection: Vec<Uuid>,
}

impl Shell {
    /// A shell on `engine`, showing its open Project or else the Start page, in the
    /// Default Layout. It adds the layout Commands to the engine.
    pub fn new(mut engine: Engine) -> Shell {
        if engine.registry().get("panel.show").is_none() {
            engine.register(layout::register_all);
        }
        engine.view_mut().set(Layout::default());
        let mut shell = Shell {
            engine,
            documents: Vec::new(),
            fonts: Arc::new(NoFonts),
            asked_fonts: BTreeSet::new(),
            generation: 0,
            message: None,
            messages: Messages::default(),
            selection: Vec::new(),
        };
        shell.load_preference_warnings();
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
        let out = self.engine.run(id, params)?;
        let base = id.split(':').next().unwrap_or(id);
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

    /// Goes to a line's Violation: opens its Document, zooms to it and selects the
    /// objects involved. A load message about a Document opens that Document.
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
            && self.open_document(document)
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
        self.select(objects);
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

    /// The selected objects.
    pub fn selection(&self) -> &[Uuid] {
        &self.selection
    }

    /// Selects `objects`, which every canvas marks.
    pub fn select(&mut self, objects: Vec<Uuid>) {
        self.generation += 1;
        let handle = SelectionHandle {
            generation: self.generation,
            objects: Arc::from(objects.as_slice()),
        };
        for tab in &mut self.documents {
            tab.selection = handle.clone();
        }
        self.selection = objects;
    }

    /// Opens `document` in a tab, or activates its tab. Returns whether it is open.
    pub fn open_document(&mut self, document: Uuid) -> bool {
        let Some(title) = self
            .engine
            .project()
            .and_then(|p| p.document(document))
            .map(|d| d.title.clone())
        else {
            return false;
        };
        if self.document_tab(document).is_none() {
            let scene = self.scene_of(document);
            let mut tab = DocumentTab::new(document, title, scene);
            if let Some(other) = self.documents.first() {
                tab.selection = other.selection.clone();
            }
            self.documents.push(tab);
        }
        self.layout_mut().open_document(document);
        true
    }

    /// Marks check results stale once the design differs from the run's.
    fn mark_stale_runs(&mut self) {
        let Some(project) = self.engine.project() else {
            return;
        };
        let erc = reda_engine::schematic::erc::design_fingerprint(project);
        if let Some(run) = self.messages.run_mut(CheckKind::Erc) {
            run.stale = run.design != erc;
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
        self.selection.clear();
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

/// The `source` of missing-font warnings.
const FONTS: &str = "fonts";
/// The `source` of Preference warnings.
const PREFERENCES: &str = "preferences";

/// The open Project's PCB Document; only called when there is one.
fn pcb_of(engine: &Engine) -> &reda_engine::model::PcbDocument {
    engine
        .project()
        .and_then(|p| p.pcb())
        .expect("checked by the caller")
}
