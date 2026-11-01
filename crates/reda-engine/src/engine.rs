//! The headless engine: a registry, a session and the injected inputs.

use std::path::Path;

use reda_command::{
    Clock, CommandError, Env, IdSource, RandomIds, Registry, Session, SystemClock, UserDirs,
};
use reda_model::Project;
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
    /// A headless engine with random UUIDs, the wall clock and the per-user folders from
    /// `REDA_HOME` or the platform, for real sessions.
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

    /// Whether the open Project has changes not yet saved.
    pub fn is_dirty(&self) -> bool {
        !self.session.dirty.is_empty()
    }
}

impl Default for Engine {
    fn default() -> Engine {
        Engine::new()
    }
}

/// Configures an [`Engine`]. Defaults: random UUIDs, the wall clock, headless, and the
/// per-user folders from [`crate::user_dirs::discover`].
pub struct EngineBuilder {
    ids: Box<dyn IdSource>,
    clock: Box<dyn Clock>,
    headless: bool,
    user_dirs: Option<UserDirs>,
}

impl Default for EngineBuilder {
    fn default() -> EngineBuilder {
        EngineBuilder {
            ids: Box::new(RandomIds::new()),
            clock: Box::new(SystemClock),
            headless: true,
            user_dirs: crate::user_dirs::discover(),
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

    /// The per-user folders, or `None` to keep Preferences and state off disk.
    pub fn user_dirs(mut self, user_dirs: Option<UserDirs>) -> EngineBuilder {
        self.user_dirs = user_dirs;
        self
    }

    pub fn build(self) -> Engine {
        Engine {
            registry: registry(),
            session: Session {
                user_dirs: self.user_dirs,
                ..Session::default()
            },
            ids: self.ids,
            clock: self.clock,
            headless: self.headless,
        }
    }
}
