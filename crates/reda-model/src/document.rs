//! Documents: their entries in the Project's Document list and their bodies.

use serde::{Deserialize, Serialize};

use crate::Uuid;

/// What kind of Document an entry is. The PCB Document joins in a later ticket.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Schematic,
}

impl DocumentKind {
    /// The file extension, without the dot.
    pub const fn extension(self) -> &'static str {
        match self {
            DocumentKind::Schematic => "redasch",
        }
    }

    pub fn from_extension(ext: &str) -> Option<DocumentKind> {
        match ext {
            "redasch" => Some(DocumentKind::Schematic),
            _ => None,
        }
    }
}

/// One Document in the Project's Document list, stored in `<name>.reda`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentEntry {
    pub uuid: Uuid,
    pub kind: DocumentKind,
    /// The file stem; the Document's file is `<name>.<extension>` in the Project folder.
    pub name: String,
    /// What the user typed as the Document's title, such as "Sheet 1".
    pub title: String,
}

impl DocumentEntry {
    /// The file name relative to the Project folder.
    pub fn file_name(&self) -> String {
        format!("{}.{}", self.name, self.kind.extension())
    }
}

/// The body of a Schematic Sheet. Its tables (Wires, Net Labels, Units…) arrive in later
/// tickets as UUID-keyed maps whose objects use `Scope::Document(sheet)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchematicSheet {
    pub uuid: Uuid,
}
