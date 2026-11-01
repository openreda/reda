//! `<name>.reda`: the Project properties and the ordered Document list.
//!
//! ```text
//! {
//!   "format": 1,
//!   "project": {"uuid":"…"},
//!   "documents": [
//!     {"uuid":"…","path":"Demo.redasch","title":"Sheet 1"}
//!   ],
//!   "libraries": [
//!     "parts"
//!   ],
//!   "fab_presets": [
//!     {"uuid":"…","name":"JLCPCB","rules":{…},"gerber":{…},"bom":[…],"cpl":[…]}
//!   ],
//!   "output_jobs": [
//!     {"uuid":"…","name":"JLCPCB","pcb":"…","fab_preset":"…","outputs":{…}}
//!   ]
//! }
//! ```
//!
//! The Project's name is the file stem, so it is not stored. `documents` is a list, not a
//! table: its order is the Project's Document order. `libraries`, `fab_presets` and
//! `output_jobs` are written only when there are any; the last two are tables sorted by
//! UUID.

use std::path::Path;

use reda_model::{DocumentEntry, DocumentKind, OutputJob, Project, ProjectFabPreset, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::FORMAT;
use crate::canonical::{CanonicalError, CanonicalWriter};

/// A REDA Project file (`.reda`). The Project's name is the file stem.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "REDA Project file")]
pub(crate) struct ProjectFile {
    /// The file-format version. REDA writes only the current one.
    #[allow(dead_code, reason = "checked by the migration chain")]
    #[schemars(schema_with = "super::schema::format_version")]
    format: u32,
    /// Project-wide properties.
    pub project: ProjectRow,
    /// The Project's Documents, in Project order (sheet numbering, PDF output).
    pub documents: Vec<DocumentRow>,
    /// The Project Libraries: folders relative to the Project folder, `/`-separated.
    /// Written only when there are any.
    #[serde(default)]
    pub libraries: Vec<String>,
    /// The Project's copies of the Fab Presets its Output Jobs use, in the same shape as a
    /// `.redafab` file plus a UUID.
    #[serde(default)]
    pub fab_presets: Vec<ProjectFabPreset>,
    /// The Output Jobs.
    #[serde(default)]
    pub output_jobs: Vec<OutputJob>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectRow {
    #[schemars(schema_with = "super::schema::uuid")]
    pub uuid: Uuid,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentRow {
    #[schemars(schema_with = "super::schema::uuid")]
    pub uuid: Uuid,
    /// The Document's file name, relative to the Project folder; its stem is the
    /// Document's name and its extension its kind.
    pub path: String,
    /// The Document's title, such as "Sheet 1".
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
    let libraries = &project.props().libraries;
    if !libraries.is_empty() {
        w.list("libraries", libraries)?;
    }
    let presets: Vec<_> = project.fab_presets().collect();
    if !presets.is_empty() {
        w.table("fab_presets", presets, |p| p.uuid)?;
    }
    let jobs: Vec<_> = project.output_jobs().collect();
    if !jobs.is_empty() {
        w.table("output_jobs", jobs, |j| j.uuid)?;
    }
    Ok(w.finish())
}
