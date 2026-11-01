//! Transactions: the only way design data changes.

use std::fmt;

use crate::{ChangeSet, Object, Project, Uuid};

/// Why a transaction step or commit was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxError {
    /// No object has this UUID.
    NotFound(Uuid),
    /// An object with this UUID already exists.
    Conflict(Uuid),
    /// The step or the resulting Project breaks a model invariant.
    Invalid(String),
}

impl fmt::Display for TxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxError::NotFound(uuid) => write!(f, "no object has UUID {uuid}"),
            TxError::Conflict(uuid) => write!(f, "an object with UUID {uuid} already exists"),
            TxError::Invalid(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for TxError {}

/// An open transaction on a Project. Each step applies at once and is recorded in a
/// [`ChangeSet`]. [`Transaction::commit`] checks the invariants and hands the change set
/// back; dropping the transaction without committing rolls every step back.
pub struct Transaction<'p> {
    project: &'p mut Project,
    changes: ChangeSet,
    committed: bool,
}

impl Project {
    pub fn transaction(&mut self) -> Transaction<'_> {
        Transaction {
            project: self,
            changes: ChangeSet::default(),
            committed: false,
        }
    }
}

impl Transaction<'_> {
    /// The Project as the steps so far have left it.
    pub fn project(&self) -> &Project {
        self.project
    }

    /// Adds a new object. Refused if its UUID is taken.
    pub fn insert(&mut self, object: Object) -> Result<(), TxError> {
        let uuid = object.uuid();
        if self.project.object(uuid).is_some() {
            return Err(TxError::Conflict(uuid));
        }
        self.set(uuid, None, Some(object));
        Ok(())
    }

    /// Replaces an existing object of the same kind.
    pub fn replace(&mut self, object: Object) -> Result<(), TxError> {
        let uuid = object.uuid();
        let before = self.project.object(uuid).ok_or(TxError::NotFound(uuid))?;
        if !before.same_kind(&object) {
            return Err(TxError::Invalid(format!(
                "object {uuid} cannot change its kind"
            )));
        }
        self.set(uuid, Some(before), Some(object));
        Ok(())
    }

    /// Removes an object and returns it.
    pub fn remove(&mut self, uuid: Uuid) -> Result<Object, TxError> {
        if uuid == self.project.uuid() {
            return Err(TxError::Invalid(
                "the Project itself cannot be removed".into(),
            ));
        }
        let before = self.project.object(uuid).ok_or(TxError::NotFound(uuid))?;
        self.set(uuid, Some(before.clone()), None);
        Ok(before)
    }

    /// Checks the Project's invariants and keeps the changes. On failure everything is
    /// rolled back.
    pub fn commit(mut self) -> Result<ChangeSet, TxError> {
        self.project.check().map_err(TxError::Invalid)?;
        self.committed = true;
        Ok(std::mem::take(&mut self.changes))
    }

    /// Reverts every step. Same as dropping the transaction.
    pub fn rollback(self) {}

    fn set(&mut self, uuid: Uuid, before: Option<Object>, after: Option<Object>) {
        self.project.put(uuid, after.clone());
        self.changes.record(uuid, before, after);
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let undo = std::mem::take(&mut self.changes).inverted();
            self.project.apply(&undo);
        }
    }
}
