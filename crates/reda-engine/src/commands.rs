//! The engine's own Commands: Project files, undo and redo, and, later, cross-Document
//! work.

pub mod history;
pub mod project;

use reda_command::Registry;
use reda_model::{Scope, Uuid};
use serde::{Deserialize, Deserializer};

/// Adds every Command defined in `reda-engine`.
pub fn register_all(registry: &mut Registry) {
    history::register_all(registry);
    project::register_all(registry);
}

/// A param naming one file of the Project: `"project"` for the Project file, or a
/// Document's UUID. The same keys name the scopes of the edit result envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentRef(pub Scope);

impl<'de> Deserialize<'de> for DocumentRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<DocumentRef, D::Error> {
        let s = String::deserialize(d)?;
        if s == "project" {
            return Ok(DocumentRef(Scope::Project));
        }
        let uuid: Uuid = s.parse().map_err(|_| {
            serde::de::Error::custom(format!(
                "`document` must be \"project\" or a Document UUID, not {s:?}"
            ))
        })?;
        Ok(DocumentRef(Scope::Document(uuid)))
    }
}
