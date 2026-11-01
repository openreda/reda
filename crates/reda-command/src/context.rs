//! What a running Command sees: the session, the injected inputs and, for edits, the open
//! transaction.

use std::collections::BTreeSet;
use std::path::PathBuf;

use reda_model::{Project, Scope, Transaction, Uuid};

use crate::command::NO_PROJECT;
use crate::{Clock, CommandError, IdSource, Timestamp};

/// The engine's mutable state between Commands.
#[derive(Debug, Default)]
pub struct Session {
    /// The open Project, if any.
    pub project: Option<Project>,
    /// The Project folder on disk, once the Project has one.
    pub location: Option<PathBuf>,
    /// Scopes with changes not yet saved.
    pub dirty: BTreeSet<Scope>,
}

/// The injected inputs a Command run needs besides the session.
pub struct Env<'a> {
    pub ids: &'a mut dyn IdSource,
    pub clock: &'a dyn Clock,
    /// No GUI: view Commands are unavailable.
    pub headless: bool,
}

pub(crate) enum Access<'a> {
    Edit(Transaction<'a>),
    /// A query step inside a batch: it reads the transaction's Project but cannot edit it.
    ReadInEdit(Transaction<'a>),
    Read(Option<&'a Project>),
    File(&'a mut Session),
}

/// Handed to [`crate::Command::run`]. What it grants follows the Command's kind; asking
/// for more is an `internal` error, because it is a bug in the Command.
pub struct Context<'a> {
    pub(crate) access: Access<'a>,
    pub(crate) ids: &'a mut dyn IdSource,
    pub(crate) clock: &'a dyn Clock,
}

impl<'a> Context<'a> {
    /// The open Project. For an edit, as the steps so far have left it.
    pub fn project(&self) -> Result<&Project, CommandError> {
        let project = match &self.access {
            Access::Edit(tx) | Access::ReadInEdit(tx) => Some(tx.project()),
            Access::Read(project) => *project,
            Access::File(session) => session.project.as_ref(),
        };
        project.ok_or_else(|| CommandError::unavailable(NO_PROJECT))
    }

    /// The edit's transaction. Only edit Commands get one.
    pub fn tx(&mut self) -> Result<&mut Transaction<'a>, CommandError> {
        match &mut self.access {
            Access::Edit(tx) => Ok(tx),
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

    /// A fresh UUID from the injected `IdSource`.
    pub fn new_id(&mut self) -> Uuid {
        self.ids.next_id()
    }

    /// The time from the injected `Clock`.
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }
}
