//! `<name>.reda`: the Project properties and the ordered Document list.
//!
//! ```text
//! {
//!   "format": 1,
//!   "project": {"uuid":"…"},
//!   "documents": [
//!     {"uuid":"…","path":"Demo.redasch","title":"Sheet 1"}
//!   ]
//! }
//! ```
//!
//! The Project's name is the file stem, so it is not stored. `documents` is a list, not a
//! table: its order is the Project's Document order.

use std::path::Path;

use reda_model::{DocumentEntry, DocumentKind, Project, Uuid};
use serde::{Deserialize, Serialize};

use super::FORMAT;
use crate::canonical::{CanonicalError, CanonicalWriter};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectFile {
    #[allow(dead_code, reason = "checked by the migration chain")]
    format: u32,
    pub project: ProjectRow,
    pub documents: Vec<DocumentRow>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectRow {
    pub uuid: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentRow {
    pub uuid: Uuid,
    /// Relative to the Project folder.
    pub path: String,
    pub title: String,
}

impl DocumentRow {
    /// The Document entry this row describes; the path must be a plain file name with a
    /// known extension.
    pub fn entry(&self) -> Result<DocumentEntry, String> {
        let path = Path::new(&self.path);
        let bad = || {
            format!(
                "document path {:?} is not a REDA Document file name",
                self.path
            )
        };
        if path.components().count() != 1 || self.path.contains(['/', '\\']) {
            return Err(bad());
        }
        let kind = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(DocumentKind::from_extension)
            .ok_or_else(bad)?;
        let name = path.file_stem().and_then(|s| s.to_str()).ok_or_else(bad)?;
        Ok(DocumentEntry {
            uuid: self.uuid,
            kind,
            name: name.to_owned(),
            title: self.title.clone(),
        })
    }
}

pub(crate) fn write_project_file(project: &Project) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field(
        "project",
        &ProjectRow {
            uuid: project.uuid(),
        },
    )?;
    w.list(
        "documents",
        project.documents().map(|d| DocumentRow {
            uuid: d.uuid,
            path: d.file_name(),
            title: d.title.clone(),
        }),
    )?;
    Ok(w.finish())
}
