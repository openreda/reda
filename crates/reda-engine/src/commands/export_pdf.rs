//! `schematic.export_pdf`: the Schematic Sheets, in Document order, as one vector PDF.
//! It spans Documents, so it lives in the engine (see [`crate::schematic_pdf`]); the
//! drawing and the PDF writer are `reda-render`'s.

use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::{PdfColour, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::fs::write_atomic;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ExportPdf);
}

pub struct ExportPdf;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportPdfParams {
    /// Where to write the PDF. A relative path is taken from the Project folder.
    pub path: PathBuf,
    /// Every sheet (the default), or only `document`.
    #[serde(default)]
    pub sheets: SheetChoice,
    /// The sheet to write with `sheets: current`: the one in view.
    #[serde(default)]
    pub document: Option<Uuid>,
    /// `colour` (the default) or `mono`, black on white without masking. A PDF always
    /// uses paper colours.
    #[serde(default)]
    pub colour: PdfColour,
}

/// Which sheets a PDF holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SheetChoice {
    /// Every Schematic Sheet, in Document order.
    #[default]
    All,
    /// Only the sheet named by `document`.
    Current,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ExportPdfResult {
    /// The file written.
    pub path: PathBuf,
    /// One page per Schematic Sheet.
    pub pages: usize,
}

impl Command for ExportPdf {
    const META: Meta = Meta {
        id: "schematic.export_pdf",
        title: "Export Schematic PDF",
        description: "Writes the Schematic Sheets (every one, in Document order, or with \
                      `sheets: current` only `document`) as one vector PDF: one page per \
                      sheet at its paper size, in paper colours, `colour` or `mono`. Glyphs \
                      are filled outlines and no fonts are embedded, so the text is not \
                      searchable. The outline holds the Project title, a bookmark per sheet \
                      and, under it, one per Component in natural sort; each Port \
                      Cross-Reference entry links to its page. The file holds no date.",
        category: "schematic",
        icon: "export_pdf",
        kind: Kind::File,
    };
    type Params = ExportPdfParams;
    type Output = ExportPdfResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ExportPdfParams,
    ) -> Result<ExportPdfResult, CommandError> {
        let session = cx.session()?;
        let project = session
            .project
            .as_ref()
            .ok_or_else(|| CommandError::unavailable("no Project is open"))?;
        let path = match &session.location {
            Some(folder) if p.path.is_relative() => folder.join(&p.path),
            None if p.path.is_relative() => {
                return Err(CommandError::rejected(
                    "the Project has no folder yet: give an absolute path",
                ));
            }
            _ => p.path.clone(),
        };
        let sheets = match (p.sheets, p.document) {
            (SheetChoice::All, _) => crate::schematic_pdf::sheets(project),
            (SheetChoice::Current, None) => {
                return Err(CommandError::invalid_params(
                    "/document",
                    "`sheets: current` needs `document`, the sheet in view",
                ));
            }
            (SheetChoice::Current, Some(sheet)) => {
                if project.schematic_sheet(sheet).is_none() {
                    return Err(CommandError::not_found(format!(
                        "no Schematic Sheet {sheet}"
                    )));
                }
                vec![sheet]
            }
        };
        // Drawn as this user draws sheets; headless, with the defaults.
        let preferences = crate::preferences::current(session.user_dirs.as_ref());
        let (pdf, pages) =
            crate::schematic_pdf::schematic_pdf(project, &preferences, &sheets, p.colour);
        write_atomic(&path, &pdf).map_err(|e| {
            CommandError::internal(format!("could not write {}: {e}", path.display()))
        })?;
        Ok(ExportPdfResult { path, pages })
    }
}
