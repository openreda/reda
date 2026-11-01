//! What a running Command sees: the session, the injected inputs and, for edits, the open
//! transaction.

use std::path::PathBuf;

use reda_model::{Project, Scope, Transaction, Uuid};

use crate::command::NO_PROJECT;
use crate::{Clock, CommandError, History, IdSource, Timestamp, UserDirs};

/// The engine's mutable state between Commands.
#[derive(Debug, Default)]
pub struct Session {
    /// The open Project, if any.
    pub project: Option<Project>,
    /// The Project folder on disk, once the Project has one.
    pub location: Option<PathBuf>,
    /// The undo stacks and dirty state of the open Project.
    pub history: History,
    /// The per-user folders, or `None` when there is no home folder (or in tests that
    /// keep away from disk). Preferences and `state.json` live here.
    pub user_dirs: Option<UserDirs>,
    /// The folder of the starter Library that ships with REDA, if this front end has one.
    pub starter_library: Option<PathBuf>,
}

/// The injected inputs a Command run needs besides the session.
pub struct Env<'a> {
    pub ids: &'a mut dyn IdSource,
    pub clock: &'a dyn Clock,
    /// No GUI: view Commands are unavailable.
    pub headless: bool,
}

pub(crate) enum Access<'a> {
    Edit(Transaction<'a>, &'a History),
    Read(Option<&'a Project>),
    File(&'a mut Session),
}

/// Handed to [`crate::Command::run`]. What it grants follows the Command's kind; asking
/// for more is an `internal` error, because it is a bug in the Command.
pub struct Context<'a> {
    pub(crate) access: Access<'a>,
    pub(crate) ids: &'a mut dyn IdSource,
    pub(crate) clock: &'a dyn Clock,
    /// Set when an edit replays an undo or redo entry instead of making a new change.
    pub(crate) step: Option<Step>,
}

/// An undo or redo an edit Command carried out, for the registry to move on the stack.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Step {
    Undo(Scope),
    Redo(Scope),
}

impl<'a> Context<'a> {
    /// The open Project. For an edit, as the steps so far have left it.
    pub fn project(&self) -> Result<&Project, CommandError> {
        let project = match &self.access {
            Access::Edit(tx, _) => Some(tx.project()),
            Access::Read(project) => *project,
            Access::File(session) => session.project.as_ref(),
        };
        project.ok_or_else(|| CommandError::unavailable(NO_PROJECT))
    }

    /// The edit's transaction. Only edit Commands get one.
    pub fn tx(&mut self) -> Result<&mut Transaction<'a>, CommandError> {
        match &mut self.access {
            Access::Edit(tx, _) => Ok(tx),
            _ => Err(CommandError::internal(
                "only edit Commands can change design data",
            )),
        }
    }

    /// The whole session, to open, replace or save the Project. Only file Commands get it.
    pub fn session(&mut self) -> Result<&mut Session, CommandError> {
        match &mut self.access {
            Access::File(session) => Ok(session),
            _ => Err(CommandError::internal(
                "only file Commands can open, close or save a Project",
            )),
        }
    }

    /// Undoes the newest entry on `scope`'s stack and returns its label. Only edit
    /// Commands can, and only once per run. Fails as `unavailable` when the stack is
    /// empty, and as `conflict` when the entry no longer fits the data.
    pub fn undo(&mut self, scope: Scope) -> Result<String, CommandError> {
        self.replay(Step::Undo(scope))
    }

    /// Redoes the newest undone entry on `scope`'s stack and returns its label. See
    /// [`Context::undo`].
    pub fn redo(&mut self, scope: Scope) -> Result<String, CommandError> {
        self.replay(Step::Redo(scope))
    }

    fn replay(&mut self, step: Step) -> Result<String, CommandError> {
        let Access::Edit(tx, history) = &mut self.access else {
            return Err(CommandError::internal(
                "only edit Commands can undo or redo",
            ));
        };
        if self.step.is_some() {
            return Err(CommandError::internal("an edit can undo or redo only once"));
        }
        let (entry, changes, nothing) = match step {
            Step::Undo(scope) => {
                let entry = history.next_undo(scope);
                (
                    entry,
                    entry.map(|e| e.changes.inverted()),
                    "nothing to undo",
                )
            }
            Step::Redo(scope) => {
                let entry = history.next_redo(scope);
                (entry, entry.map(|e| e.changes.clone()), "nothing to redo")
            }
        };
        let (Some(entry), Some(changes)) = (entry, changes) else {
            return Err(CommandError::unavailable(nothing));
        };
        tx.apply(&changes)?;
        self.step = Some(step);
        Ok(entry.label.clone())
    }

    /// A fresh UUID from the injected `IdSource`.
    pub fn new_id(&mut self) -> Uuid {
        self.ids.next_id()
    }

    /// The time from the injected `Clock`.
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }
}
