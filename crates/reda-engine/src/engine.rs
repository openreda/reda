//! The headless engine: a registry, a session and the injected inputs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use reda_command::{
    Clock, CommandError, Env, Fonts, History, IdSource, PartCopy, PartSource, RandomIds, Registry,
    SeededIds, Session, Signals, Trash, UserDirs, ViewState,
};
use reda_model::{ElementSource, LibraryElement, Project, Scope, Uuid};
use serde_json::Value;

use crate::journal::{JournalEntry, Source};

/// The seed of the UUIDs an [`Engine::preview`] makes: fixed, so the same edit previews
/// the same way each time, and apart from any a test engine seeds.
const PREVIEW_SEED: u64 = 0xf1e5;

/// The full registry: every crate's `register_all()`. Add a line here when a new crate
/// brings Commands.
pub fn registry() -> Registry {
    let mut registry = Registry::new();
    crate::commands::register_all(&mut registry);
    reda_pcb::register_all_with_user(
        &mut registry,
        crate::sync::drc_state,
        crate::preferences::waiver_author,
    );
    reda_library::register_all(&mut registry);
    reda_library::register_generators(&mut registry, crate::library::store_footprint);
    reda_output::register_all(&mut registry);
    reda_schematic::register_all_with_user(&mut registry, crate::preferences::waiver_author);
    reda_render::register_all(&mut registry, DERIVE);
    registry
}

/// What Scenes need derived by the domain crates: a sheet's Junctions, and a board's
/// placed geometry, Ratsnest and Zone fill states.
const DERIVE: reda_render::Derive = reda_render::Derive {
    junctions,
    board: crate::board_geometry::board_geometry,
};

fn junctions(sheet: &reda_model::SchematicSheet) -> Vec<reda_model::geom::Point> {
    reda_schematic::connectivity::derive(sheet).junctions
}

/// The Scene the canvas draws for `document`, with the sheet's derived Junctions or the
/// board's placed geometry (text in `fonts`): `not_found` for an unknown Document,
/// `rejected` for a kind with no Scene yet. A sheet is drawn in `style`.
pub fn document_scene(
    project: &Project,
    document: reda_model::Uuid,
    fonts: &Fonts,
    style: &reda_render::SchematicStyle,
) -> Result<reda_render::Scene, CommandError> {
    reda_render::document_scene(project, document, &DERIVE, fonts, style)
}

/// One session: at most one open Project, changed only by running Commands.
pub struct Engine {
    registry: Registry,
    session: Session,
    ids: Box<dyn IdSource>,
    clock: Box<dyn Clock>,
    fonts: Fonts,
    headless: bool,
    source: Source,
    journal: Vec<JournalEntry>,
}

impl Engine {
    /// A headless engine with random UUIDs, the wall clock and the per-user folders from
    /// `REDA_HOME` or the platform, for real sessions.
    pub fn new() -> Engine {
        Engine::builder().build()
    }

    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    /// Runs one Command and records it in the Journal. See [`Registry::run`] for the
    /// result shape.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        self.run_with(id, params, &Signals::default())
    }

    /// [`Engine::run`] with a cancellation token and progress reporter for a long-running
    /// Command (see [`Signals`]).
    pub fn run_with(
        &mut self,
        id: &str,
        params: Value,
        signals: &Signals,
    ) -> Result<Value, CommandError> {
        self.run_from(self.source, id, params, signals)
    }

    /// [`Engine::run_with`] for a Command from another source than the engine's own, such
    /// as a CLI or MCP client bridged to the app; the Journal records that source.
    pub fn run_from(
        &mut self,
        source: Source,
        id: &str,
        params: Value,
        signals: &Signals,
    ) -> Result<Value, CommandError> {
        let parts = crate::library::DiskParts::new(&self.session);
        let mut env = Env {
            ids: &mut *self.ids,
            clock: &*self.clock,
            parts: &parts,
            fonts: &self.fonts,
            headless: self.headless,
        };
        let mark = self.session.history.mark();
        let outcome =
            self.registry
                .run_with(&mut self.session, &mut env, id, params.clone(), signals);
        if source != self.source {
            // A bridged edit lands in the same stacks, labelled with who made it.
            let prefix = format!("{}: ", source.label());
            self.session.history.prefix_labels_since(mark, &prefix);
        }
        self.journal.push(JournalEntry {
            id: id.to_owned(),
            params,
            source,
            outcome: outcome.clone(),
        });
        outcome
    }

    /// Every Command run so far, oldest first.
    pub fn journal(&self) -> &[JournalEntry] {
        &self.journal
    }

    pub(crate) fn session(&self) -> &Session {
        &self.session
    }

    pub(crate) fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    pub(crate) fn push_journal(&mut self, entry: JournalEntry) {
        self.journal.push(entry);
    }

    pub(crate) fn source(&self) -> Source {
        self.source
    }

    pub(crate) fn is_headless(&self) -> bool {
        self.headless
    }

    /// The injected clock's time.
    pub(crate) fn now(&self) -> reda_command::Timestamp {
        self.clock.now()
    }

    /// The per-user folders, if there are any.
    pub fn user_dirs(&self) -> Option<&UserDirs> {
        self.session.user_dirs.as_ref()
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn project(&self) -> Option<&Project> {
        self.session.project.as_ref()
    }

    /// Whether the Command or Preset `id` can run now, and if not, why: what menus grey
    /// out. Sees the open Project and, with none, the open Library elements. A window
    /// asks with `headless` false whatever the engine was built with.
    pub fn available(&self, id: &str, headless: bool) -> Result<(), CommandError> {
        self.registry.available_with(
            id,
            self.session.project.as_ref(),
            self.session.workbench.as_ref(),
            headless,
        )
    }

    /// An open Library element, in the open Project or, with none open, on its own.
    pub fn element(&self, uuid: Uuid) -> Option<&LibraryElement> {
        self.session.elements()?.element(uuid)
    }

    /// Where an open Library element came from.
    pub fn element_source(&self, uuid: Uuid) -> Option<ElementSource> {
        self.session.elements()?.element_source(uuid)
    }

    /// The Project holding the open Library elements: the open Project, else the blank
    /// one elements open in while no Project is open. Read elements from it; its other
    /// contents are empty.
    pub fn elements(&self) -> Option<&Project> {
        self.session.elements()
    }

    /// The open Project's folder.
    pub fn location(&self) -> Option<&Path> {
        self.session.location.as_deref()
    }

    /// Whether the open Project has changes not yet saved, in any file.
    pub fn is_dirty(&self) -> bool {
        !self.dirty_scopes().is_empty()
    }

    /// The Project file, the Documents and the open Library elements with changes not yet
    /// saved, the Project first.
    /// A deleted Document keeps its stack, so undoing the delete brings it back as it
    /// was, but is not listed while it is gone.
    pub fn dirty_scopes(&self) -> Vec<Scope> {
        let exists = |scope: &Scope| match (scope, &self.session.project) {
            (Scope::Project, _) => true,
            (Scope::Document(uuid), Some(project)) => project.document(*uuid).is_some(),
            (Scope::Element(uuid), _) => self.element(*uuid).is_some(),
            (Scope::Document(_), None) => false,
        };
        let mut dirty = self.session.history.dirty_scopes();
        dirty.retain(exists);
        dirty
    }

    /// The font bytes handed to the engine for text in installed fonts.
    pub fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    /// The fonts, to add to. The app adds the fonts installed on the machine here, at
    /// start-up or whenever it finds them; text in a family not here draws as Noto Sans
    /// and is reported as missing.
    pub fn fonts_mut(&mut self) -> &mut Fonts {
        &mut self.fonts
    }

    /// The Scene the canvas draws for `document` of the open Project, as
    /// [`document_scene`] builds it with this engine's fonts; `unavailable` with no
    /// Project open.
    pub fn document_scene(
        &self,
        document: reda_model::Uuid,
    ) -> Result<reda_render::Scene, CommandError> {
        let project = self
            .session
            .project
            .as_ref()
            .ok_or_else(|| CommandError::unavailable("no Project is open"))?;
        document_scene(project, document, &self.fonts, &Default::default())
    }

    /// Scans the registered Libraries (the starter Library, the user's and the open
    /// Project's) from disk, as `library.search` does, for the Library panel to list.
    pub fn libraries(&self) -> crate::library::LibraryIndex {
        crate::library::scan_registered(&self.session)
    }

    /// The Part `uuid` from the registered Libraries with its Symbol and Footprints, as
    /// placing it would copy it; `not_found` when no Library holds it whole.
    pub fn library_part(&self, uuid: reda_model::Uuid) -> Result<PartCopy, CommandError> {
        crate::library::DiskParts::new(&self.session).part(uuid)
    }

    /// The open Project as the edit Command `id` would leave it, with the Command's
    /// result, taking Library Parts from `parts`. Nothing runs for real: no Journal entry,
    /// undo entry or dirty mark, so a Tool can draw an edit before the user commits it.
    pub fn preview(
        &self,
        id: &str,
        params: Value,
        parts: &dyn PartSource,
    ) -> Result<(Project, Value), CommandError> {
        let project = self
            .session
            .project
            .as_ref()
            .ok_or_else(|| CommandError::unavailable("no Project is open"))?;
        let mut ids = SeededIds::new(PREVIEW_SEED);
        let mut env = Env {
            ids: &mut ids,
            clock: &*self.clock,
            parts,
            fonts: &self.fonts,
            headless: self.headless,
        };
        self.registry
            .preview(project, &self.session.history, &mut env, id, params)
    }

    /// The undo stacks of the open Project, for menus to label Undo and Redo.
    pub fn history(&self) -> &History {
        &self.session.history
    }

    /// Caps each undo stack at `depth` entries, trimming the oldest; `None` is unlimited,
    /// the default. Front ends pass the *undo depth* Preference here.
    pub fn set_undo_depth(&mut self, depth: Option<usize>) {
        self.session.history.set_depth(depth);
    }

    /// The front end's view state, which its view Commands change (see [`ViewState`]).
    pub fn view(&self) -> &ViewState {
        &self.session.view
    }

    /// The view state, for the front end to install its own state in, such as the app's
    /// docking Layout when a Project opens.
    pub fn view_mut(&mut self) -> &mut ViewState {
        &mut self.session.view
    }

    /// Adds Commands after the engine is built, such as the app's view Commands.
    pub fn register(&mut self, register_all: impl FnOnce(&mut Registry)) {
        register_all(&mut self.registry);
    }
}

impl Default for Engine {
    fn default() -> Engine {
        Engine::new()
    }
}

/// Configures an [`Engine`]. Defaults: random UUIDs, the wall clock, headless, unlimited
/// undo, no installed fonts, the full [`registry`], the CLI as the source, the per-user
/// folders from [`crate::user_dirs::discover`] and the starter Library from
/// [`crate::starter::discover`].
pub struct EngineBuilder {
    registry: Registry,
    undo_depth: Option<usize>,
    ids: Box<dyn IdSource>,
    clock: Box<dyn Clock>,
    fonts: Fonts,
    headless: bool,
    source: Source,
    user_dirs: Option<UserDirs>,
    starter_library: Option<PathBuf>,
    trash: Option<Arc<dyn Trash>>,
}

/// Fresh random UUIDs. On wasm32 the seed comes from the hasher keys alone, which may
/// repeat between sessions: a wasm host should pass `.ids(RandomIds::from_seed(..))`.
pub(crate) fn default_ids() -> RandomIds {
    #[cfg(not(target_arch = "wasm32"))]
    {
        RandomIds::new()
    }
    #[cfg(target_arch = "wasm32")]
    {
        use std::hash::{BuildHasher, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_usize(&hasher as *const _ as usize);
        RandomIds::from_seed(hasher.finish())
    }
}

/// The wall clock; on wasm32, which has none in `std`, the fixed epoch until the host
/// passes `.clock(..)`.
fn default_clock() -> Box<dyn Clock> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Box::new(reda_command::SystemClock)
    }
    #[cfg(target_arch = "wasm32")]
    {
        Box::new(reda_command::FixedClock::EPOCH)
    }
}

impl Default for EngineBuilder {
    fn default() -> EngineBuilder {
        EngineBuilder {
            registry: registry(),
            undo_depth: None,
            ids: Box::new(default_ids()),
            clock: default_clock(),
            fonts: Fonts::default(),
            headless: true,
            source: Source::default(),
            user_dirs: crate::user_dirs::discover(),
            starter_library: crate::starter::discover(),
            trash: None,
        }
    }
}

impl EngineBuilder {
    pub fn ids(mut self, ids: impl IdSource + 'static) -> EngineBuilder {
        self.ids = Box::new(ids);
        self
    }

    pub fn clock(mut self, clock: impl Clock + 'static) -> EngineBuilder {
        self.clock = Box::new(clock);
        self
    }

    /// The font bytes for text in installed fonts; see [`Engine::fonts_mut`].
    pub fn fonts(mut self, fonts: Fonts) -> EngineBuilder {
        self.fonts = fonts;
        self
    }

    /// `false` when a GUI drives the engine, which makes view Commands available.
    pub fn headless(mut self, headless: bool) -> EngineBuilder {
        self.headless = headless;
        self
    }

    /// Who drives this engine, as recorded in the Journal.
    pub fn source(mut self, source: Source) -> EngineBuilder {
        self.source = source;
        self
    }

    /// The per-user folders, or `None` to keep Preferences and state off disk.
    pub fn user_dirs(mut self, user_dirs: Option<UserDirs>) -> EngineBuilder {
        self.user_dirs = user_dirs;
        self
    }

    /// The starter Library's folder, which the app ships (by default the one
    /// [`crate::starter::discover`] finds); `None` scans no starter Library.
    pub fn starter_library(mut self, folder: Option<PathBuf>) -> EngineBuilder {
        self.starter_library = folder;
        self
    }

    /// Where `project.save` sends deleted Documents' files: the OS trash in the CLI and the
    /// app. Without one (the default), they are left on disk.
    pub fn trash(mut self, trash: impl Trash + 'static) -> EngineBuilder {
        self.trash = Some(Arc::new(trash));
        self
    }

    /// Adds Commands beyond the full [`registry`], such as test-only ones.
    pub fn commands(mut self, register_all: impl FnOnce(&mut Registry)) -> EngineBuilder {
        register_all(&mut self.registry);
        self
    }

    /// See [`Engine::set_undo_depth`].
    pub fn undo_depth(mut self, depth: Option<usize>) -> EngineBuilder {
        self.undo_depth = depth;
        self
    }

    pub fn build(self) -> Engine {
        let mut session = Session::default();
        session.history.set_depth(self.undo_depth);
        session.user_dirs = self.user_dirs;
        session.starter_library = self.starter_library;
        session.trash = self.trash;
        Engine {
            registry: self.registry,
            session,
            ids: self.ids,
            clock: self.clock,
            fonts: self.fonts,
            headless: self.headless,
            source: self.source,
            journal: Vec::new(),
        }
    }
}
