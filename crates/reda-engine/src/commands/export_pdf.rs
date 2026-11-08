//! `schematic.export_pdf`: every Schematic Sheet, in Document order, as one vector PDF.
//! It spans Documents, so it lives in the engine; the drawing and the PDF writer are
//! `reda-render`'s.

use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::DocumentKind;
use reda_render::{Palette, PdfInfo, RenderTheme, schematic_scene_styled, to_pdf};
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
    /// The colour scheme; light by default, for paper.
    #[serde(default = "light")]
    pub theme: RenderTheme,
}

fn light() -> RenderTheme {
    RenderTheme::Light
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
        description: "Writes every Schematic Sheet, in Document order, as one vector PDF: \
                      one page per sheet at its paper size. Glyphs are filled outlines and \
                      no fonts are embedded, so the text is not searchable.",
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
        let created = cx.now().0;
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
        // Drawn as this user draws sheets; headless, with the defaults.
        let preferences = crate::preferences::current(session.user_dirs.as_ref());
        let style = crate::schematic_style::schematic_style(
            project,
            &preferences,
            crate::schematic_style::Drawn::Output,
        );
        let scenes: Vec<_> = project
            .documents()
            .filter(|d| d.kind == DocumentKind::Schematic)
            .filter_map(|d| {
                let sheet = project.schematic_sheet(d.uuid)?;
                let junctions = reda_schematic::connectivity::derive(sheet).junctions;
                schematic_scene_styled(project, d.uuid, &junctions, &style)
            })
            .collect();
        let props = project.props();
        let title = if props.properties.title.is_empty() {
            props.name.clone()
        } else {
            reda_model::text::display(&props.properties.title).into_owned()
        };
        let pdf = to_pdf(
            &scenes,
            &Palette::shipped(p.theme.into()),
            &PdfInfo { title, created },
        );
        write_atomic(&path, &pdf).map_err(|e| {
            CommandError::internal(format!("could not write {}: {e}", path.display()))
        })?;
        Ok(ExportPdfResult {
            path,
            pages: scenes.len(),
        })
    }
}
