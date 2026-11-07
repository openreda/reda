//! The engine's own Commands: Project files, undo and redo, and, later, cross-Document
//! work.

pub mod checks_report;
pub mod document;
pub mod element_copy;
pub mod elements;
pub mod export_pdf;
pub mod extract;
pub mod fab_preset;
pub mod history;
pub mod import_part;
pub mod library;
pub mod library_check;
pub mod local_edits;
pub mod output_run;
pub mod pcb_update;
pub mod port_partners;
pub mod preferences;
pub mod project;
pub mod properties;
pub mod query;
pub mod reannotate;
pub mod recovery;
pub mod rules_file;
pub mod similar;
pub mod symbol_wizard;
pub mod sync;
pub mod where_used;
pub mod write_through;
pub mod zone_fill;

use reda_command::Registry;
use reda_model::{Scope, Uuid};
use serde::{Deserialize, Deserializer};

/// Adds every Command defined in `reda-engine`.
pub fn register_all(registry: &mut Registry) {
    checks_report::register_all(registry);
    document::register_all(registry);
    fab_preset::register_all(registry);
    elements::register_all(registry);
    export_pdf::register_all(registry);
    history::register_all(registry);
    import_part::register_all(registry);
    library::register_all(registry);
    where_used::register_all(registry);
    element_copy::register_all(registry);
    extract::register_all(registry);
    library_check::register_all(registry);
    local_edits::register_all(registry);
    output_run::register_all(registry);
    pcb_update::register_all(registry);
    port_partners::register_all(registry);
    preferences::register_all(registry);
    project::register_all(registry);
    properties::register_all(registry);
    query::register_all(registry);
    reannotate::register_all(registry);
    rules_file::register_all(registry);
    similar::register_all(registry);
    symbol_wizard::register_all(registry);
    recovery::register_all(registry);
    sync::register_all(registry);
    zone_fill::register_all(registry);
    write_through::register_all(registry);
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
