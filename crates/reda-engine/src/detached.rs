//! Detached runs: a long-running File Command split off the engine onto a copy of the
//! session, so a front end can run it on another thread while it keeps drawing (a
//! progress bar with Cancel), then hand the outcome back to the engine's Journal.
//!
//! Only Commands that read the Project and write files elsewhere may be detached (today
//! `output.job.run` and `pcb.zone.fill`, the window's background refill): an edit must
//! run on the engine's own session. The one thing such a
//! run may change in its copy is derived data, the Zone fills it computed, and
//! [`Engine::finish`] keeps those, as a run on the engine would have.

use reda_command::{CommandError, Env, FixedClock, Fonts, Registry, Session, Signals};
use reda_model::{Uuid, ZoneFills};
use serde_json::Value;

use crate::Engine;
use crate::journal::JournalEntry;

/// The Commands [`Engine::detach`] accepts.
fn detachable() -> Registry {
    let mut registry = Registry::new();
    crate::commands::output_run::register_all(&mut registry);
    crate::commands::zone_fill::register_all(&mut registry);
    registry
}

/// A Command run split off an [`Engine`], with what it needs to run alone. It is `Send`:
/// run it on any thread with [`Detached::run`].
pub struct Detached {
    id: String,
    params: Value,
    registry: Registry,
    session: Session,
    clock: FixedClock,
    fonts: Fonts,
    headless: bool,
    /// The PCB Document's Zone fills when the run was split off.
    fills: Option<(Uuid, Option<ZoneFills>)>,
}

/// A detached run's outcome, for [`Engine::finish`].
#[derive(Debug)]
pub struct Finished {
    id: String,
    params: Value,
    outcome: Result<Value, CommandError>,
    /// Zone fills the run computed: the PCB Document, its fills before and after.
    fills: Option<(Uuid, Option<ZoneFills>, ZoneFills)>,
}

impl Finished {
    pub fn outcome(&self) -> &Result<Value, CommandError> {
        &self.outcome
    }
}

impl Detached {
    /// Runs the Command with `signals` (progress and cancellation).
    pub fn run(mut self, signals: &Signals) -> Finished {
        let parts = crate::library::DiskParts::new(&self.session);
        let mut ids = crate::engine::default_ids();
        let mut env = Env {
            ids: &mut ids,
            clock: &self.clock,
            parts: &parts,
            fonts: &self.fonts,
            headless: self.headless,
        };
        let outcome = self.registry.run_with(
            &mut self.session,
            &mut env,
            &self.id,
            self.params.clone(),
            signals,
        );
        let fills = self.fills.and_then(|(pcb, before)| {
            let after = self.session.project.as_ref()?.zone_fills(pcb)?;
            (before.as_ref() != Some(after)).then(|| (pcb, before, after.clone()))
        });
        Finished {
            id: self.id,
            params: self.params,
            outcome,
            fills,
        }
    }
}

impl Engine {
    /// Splits a run of `id` off onto a copy of the open Project, its folder and the
    /// per-user folders, with the time fixed to now. The params and availability are
    /// checked here; a Command that cannot be detached is `rejected`.
    pub fn detach(&self, id: &str, params: Value) -> Result<Detached, CommandError> {
        let registry = detachable();
        let base = id.split(':').next().unwrap_or(id);
        if registry.get(base).is_none() {
            return Err(CommandError::rejected(format!(
                "{id} cannot run in the background"
            )));
        }
        let session = self.session();
        let fills = session
            .project
            .as_ref()
            .and_then(|p| Some((p, p.pcb()?.uuid)))
            .map(|(p, pcb)| (pcb, p.zone_fills(pcb).cloned()));
        self.registry()
            .check(id, &params, session.project.as_ref(), self.is_headless())?;
        Ok(Detached {
            id: id.to_owned(),
            params,
            registry,
            session: Session {
                project: session.project.clone(),
                location: session.location.clone(),
                user_dirs: session.user_dirs.clone(),
                starter_library: session.starter_library.clone(),
                ..Session::default()
            },
            clock: FixedClock(self.now()),
            fonts: self.fonts().clone(),
            headless: self.is_headless(),
            fills,
        })
    }

    /// Records a detached run in the Journal, as if it had run here, keeps the Zone fills
    /// it computed (unless the fills here changed meanwhile), and returns its outcome.
    pub fn finish(&mut self, done: Finished) -> Result<Value, CommandError> {
        if let Some((pcb, before, after)) = done.fills
            && let Some(project) = self.session_mut().project.as_mut()
            && project.zone_fills(pcb) == before.as_ref()
        {
            project.set_zone_fills(pcb, Some(after));
        }
        let source = self.source();
        self.push_journal(JournalEntry {
            id: done.id,
            params: done.params,
            source,
            outcome: done.outcome.clone(),
        });
        done.outcome
    }
}
