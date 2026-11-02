//! `document.render`: a Document drawn as an image, with a view that is explicit and
//! independent of the user's.

use reda_command::{Command, CommandError, Context, Kind, Length, Meta, Registry};
use reda_model::geom::Point;
use reda_model::{DocumentKind, Project, SchematicSheet, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::color::{Palette, Theme};
use crate::scene::Scene;
use crate::schematic::schematic_scene;
use crate::svg::to_svg;
use crate::viewport::Viewport;

/// Derives a sheet's Junctions. Rendering does not derive connectivity itself, so the
/// engine hands in `reda-schematic`'s derivation when it assembles the registry.
pub type Junctions = fn(&SchematicSheet) -> Vec<Point>;

pub fn register_all(registry: &mut Registry, junctions: Junctions) {
    registry.add(DocumentRender { junctions });
}

/// The Scene of any Document this crate can draw: `not_found` for an unknown Document,
/// `rejected` for a kind with no Scene yet.
pub fn document_scene(
    project: &Project,
    document: Uuid,
    junctions: Junctions,
) -> Result<Scene, CommandError> {
    let entry = project
        .document(document)
        .ok_or_else(|| CommandError::not_found(format!("no Document {document}")))?;
    match entry.kind {
        DocumentKind::Schematic => {
            let sheet = project
                .schematic_sheet(document)
                .ok_or_else(|| CommandError::not_found(format!("no Document {document}")))?;
            Ok(schematic_scene(project, document, &junctions(sheet)).unwrap_or_default())
        }
        kind => Err(CommandError::rejected(format!(
            "{kind:?} Documents cannot be rendered yet"
        ))),
    }
}

pub struct DocumentRender {
    junctions: Junctions,
}

/// What the image shows.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RenderView {
    /// The whole Document.
    #[default]
    Fit,
    /// A rectangle of the Document, y up.
    Rect { min: RenderPoint, max: RenderPoint },
    /// Everything these objects draw.
    Objects(Vec<Uuid>),
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderPoint {
    pub x: Length,
    pub y: Length,
}

impl RenderPoint {
    fn point(&self) -> Point {
        Point::new(self.x.nm(), self.y.nm())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RenderFormat {
    /// A PNG drawn by the GPU backend.
    #[default]
    Png,
    /// Vector SVG, with glyphs as filled outlines.
    Svg,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RenderTheme {
    #[default]
    Dark,
    Light,
}

impl From<RenderTheme> for Theme {
    fn from(theme: RenderTheme) -> Theme {
        match theme {
            RenderTheme::Dark => Theme::Dark,
            RenderTheme::Light => Theme::Light,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentRenderParams {
    /// The Document to draw.
    pub document: Uuid,
    /// What to show: `"fit"` (the default), `{"rect": {"min", "max"}}` or
    /// `{"objects": [uuid…]}`.
    #[serde(default)]
    pub view: RenderView,
    /// The image's longer side in pixels.
    #[serde(default = "default_size")]
    pub size: u32,
    #[serde(default)]
    pub theme: RenderTheme,
    /// Objects to highlight: everything else is dimmed.
    #[serde(default)]
    pub highlight: Vec<Uuid>,
    #[serde(default)]
    pub format: RenderFormat,
}

fn default_size() -> u32 {
    1024
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DocumentRenderResult {
    pub format: RenderFormat,
    /// Image size in pixels.
    pub width: u32,
    pub height: u32,
    /// The SVG document, for `format: "svg"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svg: Option<String>,
}

impl Command for DocumentRender {
    const META: Meta = Meta {
        id: "document.render",
        title: "Render Document",
        description: "Draws a Document as an image, through the same Scene as the canvas. \
                      The view, size, theme and highlight are explicit and independent of \
                      the user's view. `svg` returns vector SVG text; `png` needs the GPU \
                      backend.",
        category: "document",
        icon: "render",
        kind: Kind::Query,
    };
    type Params = DocumentRenderParams;
    type Output = DocumentRenderResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: DocumentRenderParams,
    ) -> Result<DocumentRenderResult, CommandError> {
        let scene = document_scene(cx.project()?, p.document, self.junctions)?;
        if p.size == 0 {
            return Err(CommandError::invalid_params(
                "/size",
                "size must be at least 1",
            ));
        }
        let (min, max) = match &p.view {
            RenderView::Fit => scene.bounds(),
            RenderView::Rect { min, max } => {
                let (a, b) = (min.point(), max.point());
                (a.x < b.x && a.y < b.y).then_some((a, b)).ok_or_else(|| {
                    CommandError::invalid_params("/view/rect", "max must be above and right of min")
                })?;
                Some((a, b))
            }
            RenderView::Objects(objects) => objects_bounds(&scene, objects),
        }
        .ok_or_else(|| CommandError::rejected("nothing to frame: the view is empty"))?;
        let view = Viewport::fit(min, max, p.size);
        match p.format {
            RenderFormat::Png => Err(CommandError::unavailable(
                "PNG rendering needs the GPU backend, which this build does not have; \
                 use format \"svg\"",
            )),
            RenderFormat::Svg => {
                let svg = to_svg(
                    &scene,
                    &Palette::shipped(p.theme.into()),
                    &view,
                    &p.highlight,
                );
                Ok(DocumentRenderResult {
                    format: RenderFormat::Svg,
                    width: view.width,
                    height: view.height,
                    svg: Some(svg),
                })
            }
        }
    }
}

/// The bounds of everything `objects` draw.
fn objects_bounds(scene: &Scene, objects: &[Uuid]) -> Option<(Point, Point)> {
    scene
        .items
        .iter()
        .filter(|i| objects.contains(&i.object))
        .filter_map(|i| i.primitive.bounds())
        .reduce(|(lo, hi), (l, h)| {
            (
                Point::new(lo.x.min(l.x), lo.y.min(l.y)),
                Point::new(hi.x.max(h.x), hi.y.max(h.y)),
            )
        })
}
