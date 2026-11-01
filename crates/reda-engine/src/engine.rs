//! The headless engine: a registry, a session and the injected inputs.

use std::path::{Path, PathBuf};

use reda_command::{
    Clock, CommandError, Env, History, IdSource, RandomIds, Registry, Session, Signals,
    SystemClock, UserDirs,
};
use reda_model::{Project, Scope};
use serde_json::Value;

use crate::journal::{JournalEntry, Source};

/// The full registry: every crate's `register_all()`. Add a line here when a new crate
/// brings Commands.
pub fn registry() -> Registry {
    let mut registry = Registry::new();
    crate::commands::register_all(&mut registry);
    reda_pcb::register_all(&mut registry);
    reda_library::register_all(&mut registry);
    registry
}

/// One session: at most one open Project, changed only by running Commands.
pub struct Engine {
    registry: Registry,
    session: Session,
    ids: Box<dyn IdSource>,
    clock: Box<dyn Clock>,
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
        let mut env = Env {
            ids: &mut *self.ids,
            clock: &*self.clock,
            headless: self.headless,
        };
        let outcome =
            self.registry
                .run_with(&mut self.session, &mut env, id, params.clone(), signals);
        self.journal.push(JournalEntry {
            id: id.to_owned(),
            params,
            source: self.source,
            outcome: outcome.clone(),
        });
        outcome
    }

    /// Every Command run so far, oldest first.
    pub fn journal(&self) -> &[JournalEntry] {
        &self.journal
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
            (Scope::Element(uuid), Some(project)) => project.element(*uuid).is_some(),
            (Scope::Document(_) | Scope::Element(_), None) => false,
        };
        let mut dirty = self.session.history.dirty_scopes();
        dirty.retain(exists);
        dirty
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
}

impl Default for Engine {
    fn default() -> Engine {
        Engine::new()
    }
}

/// Configures an [`Engine`]. Defaults: random UUIDs, the wall clock, headless, unlimited
/// undo, the full [`registry`], the CLI as the source, and the per-user folders from
/// [`crate::user_dirs::discover`].
pub struct EngineBuilder {
    registry: Registry,
    undo_depth: Option<usize>,
    ids: Box<dyn IdSource>,
    clock: Box<dyn Clock>,
    headless: bool,
    source: Source,
    user_dirs: Option<UserDirs>,
    starter_library: Option<PathBuf>,
}

impl Default for EngineBuilder {
    fn default() -> EngineBuilder {
        EngineBuilder {
            registry: registry(),
            undo_depth: None,
            ids: Box::new(RandomIds::new()),
            clock: Box::new(SystemClock),
            headless: true,
            source: Source::default(),
            user_dirs: crate::user_dirs::discover(),
            starter_library: None,
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

    /// The starter Library's folder, which the app ships; `None` (the default) scans no
    /// starter Library.
    pub fn starter_library(mut self, folder: Option<PathBuf>) -> EngineBuilder {
        self.starter_library = folder;
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
        Engine {
            registry: self.registry,
            session,
            ids: self.ids,
            clock: self.clock,
            headless: self.headless,
            source: self.source,
            journal: Vec::new(),
        }
    }
}
