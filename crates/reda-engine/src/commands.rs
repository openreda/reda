//! The engine's own Commands: Project files, undo and redo, and, later, cross-Document
//! work.

pub mod document;
pub mod elements;
pub mod export_pdf;
pub mod fab_preset;
pub mod history;
pub mod import_part;
pub mod library;
pub mod library_check;
pub mod preferences;
pub mod project;
pub mod properties;
pub mod query;
pub mod reannotate;
pub mod sync;

use reda_command::Registry;
use reda_model::{Scope, Uuid};
use serde::{Deserialize, Deserializer};

/// Adds every Command defined in `reda-engine`.
pub fn register_all(registry: &mut Registry) {
    document::register_all(registry);
    fab_preset::register_all(registry);
    elements::register_all(registry);
    export_pdf::register_all(registry);
    history::register_all(registry);
    import_part::register_all(registry);
    library::register_all(registry);
    library_check::register_all(registry);
    preferences::register_all(registry);
    project::register_all(registry);
    properties::register_all(registry);
    query::register_all(registry);
    reannotate::register_all(registry);
    sync::register_all(registry);
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

impl schemars::JsonSchema for DocumentRef {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DocumentRef".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "One file of the Project: \"project\" for the Project file, or a Document's UUID.",
            "anyOf": [
                { "const": "project", "type": "string" },
                { "type": "string", "format": "uuid" }
            ]
        })
    }
}
