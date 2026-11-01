//! The headless engine: a registry, a session and the injected inputs.

use std::path::Path;

use reda_command::{
    Clock, CommandError, Env, History, IdSource, RandomIds, Registry, Session, SystemClock,
};
use reda_model::{Project, Scope};
use serde_json::Value;

/// The full registry: every crate's `register_all()`. Add a line here when a new crate
/// brings Commands.
pub fn registry() -> Registry {
    let mut registry = Registry::new();
    crate::commands::register_all(&mut registry);
    registry
}

/// One session: at most one open Project, changed only by running Commands.
pub struct Engine {
    registry: Registry,
    session: Session,
    ids: Box<dyn IdSource>,
    clock: Box<dyn Clock>,
    headless: bool,
}

impl Engine {
    /// A headless engine with random UUIDs and the wall clock, for real sessions.
    pub fn new() -> Engine {
        Engine::builder().build()
    }

    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    /// Runs one Command. See [`Registry::run`] for the result shape.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let mut env = Env {
            ids: &mut *self.ids,
            clock: &*self.clock,
            headless: self.headless,
        };
        self.registry.run(&mut self.session, &mut env, id, params)
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
        !self.session.history.dirty_scopes().is_empty()
    }

    /// The Project file and the Documents with changes not yet saved, the Project first.
    pub fn dirty_scopes(&self) -> Vec<Scope> {
        self.session.history.dirty_scopes()
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
/// undo, and the full [`registry`].
pub struct EngineBuilder {
    registry: Registry,
    undo_depth: Option<usize>,
    ids: Box<dyn IdSource>,
    clock: Box<dyn Clock>,
    headless: bool,
}

impl Default for EngineBuilder {
    fn default() -> EngineBuilder {
        EngineBuilder {
            registry: registry(),
            undo_depth: None,
            ids: Box::new(RandomIds::new()),
            clock: Box::new(SystemClock),
            headless: true,
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
        Engine {
            registry: self.registry,
            session,
            ids: self.ids,
            clock: self.clock,
            headless: self.headless,
        }
    }
}
