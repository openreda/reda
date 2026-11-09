//! `document.render`: a Document drawn as an image, with a view that is explicit and
//! independent of the user's.

use reda_command::{Command, CommandError, Context, Fonts, Kind, Length, Meta, Registry};
use reda_model::geom::Point;
use reda_model::{DocumentKind, LayerKind, PcbDocument, Project, SchematicSheet, Side, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::color::{Palette, Theme};
use crate::layers::{Composite, InactiveLayers, LayerView, board_composites};
use crate::pcb::{BoardGeometry, BoardStyle, pcb_scene_styled};
use crate::scene::Scene;
use crate::schematic::{SchematicStyle, schematic_scene_styled};
use crate::svg::to_svg_layers;
use crate::viewport::Viewport;

/// Derives a sheet's Junctions. Rendering does not derive connectivity itself, so the
/// engine hands in `reda-schematic`'s derivation when it assembles the registry.
pub type Junctions = fn(&SchematicSheet) -> Vec<Point>;

/// Works out what a PCB Scene needs from PCB behaviour (placed Footprints, laid-out text
/// in the given fonts, the Ratsnest, Zone fill staleness), which this crate does not
/// depend on: the engine hands in its own when it assembles the registry.
/// `style` says whether the Ratsnest uses Smart Track Ends.
pub type BoardParts = fn(&Project, &PcbDocument, &Fonts, &BoardStyle) -> BoardGeometry;

/// The derived data Scenes need and this crate does not derive itself.
#[derive(Clone, Copy)]
pub struct Derive {
    pub junctions: Junctions,
    pub board: BoardParts,
}

pub fn register_all(registry: &mut Registry, derive: Derive) {
    registry.add(DocumentRender { derive });
}

/// The Scene of any Document this crate can draw (`not_found` for an unknown Document).
/// `fonts` are the installed fonts PCB text may name; a sheet is drawn in `style`.
pub fn document_scene(
    project: &Project,
    document: Uuid,
    derive: &Derive,
    fonts: &Fonts,
    style: &SchematicStyle,
) -> Result<Scene, CommandError> {
    let entry = project
        .document(document)
        .ok_or_else(|| CommandError::not_found(format!("no Document {document}")))?;
    let missing = || CommandError::not_found(format!("no Document {document}"));
    match entry.kind {
        DocumentKind::Schematic => {
            let sheet = project.schematic_sheet(document).ok_or_else(missing)?;
            let junctions = (derive.junctions)(sheet);
            Ok(schematic_scene_styled(project, document, &junctions, style).unwrap_or_default())
        }
        DocumentKind::Pcb => board_scene(project, document, derive, fonts, &BoardStyle::default()),
    }
}

/// The Scene of PCB Document `document` drawn in `style` (`not_found` when the Project
/// has no such PCB).
pub fn board_scene(
    project: &Project,
    document: Uuid,
    derive: &Derive,
    fonts: &Fonts,
    style: &BoardStyle,
) -> Result<Scene, CommandError> {
    let pcb = project
        .pcb_document(document)
        .ok_or_else(|| CommandError::not_found(format!("no PCB Document {document}")))?;
    let geometry = (derive.board)(project, pcb, fonts, style);
    Ok(pcb_scene_styled(project, document, &geometry, style).unwrap_or_default())
}

pub struct DocumentRender {
    derive: Derive,
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
    /// The image's longer side in pixels, 1 to 8192; 1024 by default.
    #[serde(default = "default_size")]
    pub size: u32,
    #[serde(default)]
    pub theme: RenderTheme,
    /// The PCB Layers to show, by UUID (every Layer by default). A Schematic Sheet has a
    /// single Layer, so leave this out for one.
    pub layers: Option<Vec<Uuid>>,
    /// The PCB Layer drawn last, on top, at full opacity; the top copper by default (when
    /// shown). Leave this out for a Schematic Sheet.
    pub active_layer: Option<Uuid>,
    /// How the other PCB Layers are drawn: `normal` (the default, partly transparent),
    /// `dimmed` or `hidden` (only the active Layer). PCB only.
    pub inactive_layers: Option<InactiveLayers>,
    /// View the PCB from the bottom: mirrored, bottom Layers on top. PCB only; false by
    /// default.
    pub from_bottom: Option<bool>,
    /// Whether the PCB's Ratsnest is drawn; true by default. PCB only.
    pub ratsnest: Option<bool>,
    /// Objects to highlight: everything else is dimmed.
    #[serde(default)]
    pub highlight: Vec<Uuid>,
    #[serde(default)]
    pub format: RenderFormat,
}

fn default_size() -> u32 {
    1024
}

/// The longest image side, in pixels (WebGPU's default texture limit).
pub const MAX_SIZE: u32 = 8192;

#[derive(Debug, Serialize, JsonSchema)]
pub struct DocumentRenderResult {
    pub format: RenderFormat,
    /// Image size in pixels.
    pub width: u32,
    pub height: u32,
    /// The SVG document, for `format: "svg"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svg: Option<String>,
    /// The PNG file, base64-encoded, for `format: "png"`. Over MCP it comes back as an
    /// image content block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png: Option<String>,
}

impl Command for DocumentRender {
    const META: Meta = Meta {
        id: "document.render",
        title: "Render Document",
        description: "Draws a Document as an image, through the same Scene as the canvas. \
                      The view, size, theme and highlight are explicit and independent of \
                      the user's view. `png` (the default) is drawn by the canvas renderer, \
                      headless on a software adapter, and fails naming the cause when no \
                      adapter is available; `svg` returns vector SVG text.",
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
        let project = cx.project()?;
        let style = SchematicStyle::default();
        let scene = document_scene(project, p.document, &self.derive, cx.fonts(), &style)?;
        let composites = match project.pcb_document(p.document) {
            Some(pcb) => Some(pcb_layers(pcb, &p)?),
            None => {
                sheet_layers(&p)?;
                None
            }
        };
        let mirrored = p.from_bottom.unwrap_or(false);
        if !(1..=MAX_SIZE).contains(&p.size) {
            return Err(CommandError::invalid_params(
                "/size",
                format!("size must be 1 to {MAX_SIZE} pixels"),
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
            RenderFormat::Png => {
                cx.check_cancelled()?;
                let palette = Palette::shipped(p.theme.into());
                let layers = composites.as_deref();
                let png = png(&scene, &view, mirrored, &palette, &p.highlight, layers)?;
                Ok(DocumentRenderResult {
                    format: RenderFormat::Png,
                    width: view.width,
                    height: view.height,
                    svg: None,
                    png: Some(crate::base64::encode(&png)),
                })
            }
            RenderFormat::Svg => {
                let svg = to_svg_layers(
                    &scene,
                    &Palette::shipped(p.theme.into()),
                    &view,
                    &p.highlight,
                    composites.as_deref(),
                    mirrored,
                );
                Ok(DocumentRenderResult {
                    format: RenderFormat::Svg,
                    width: view.width,
                    height: view.height,
                    svg: Some(svg),
                    png: None,
                })
            }
        }
    }
}

/// Refuses the PCB-only params for a Schematic Sheet.
fn sheet_layers(p: &DocumentRenderParams) -> Result<(), CommandError> {
    let pcb_only = [
        ("layers", p.layers.is_some()),
        ("active_layer", p.active_layer.is_some()),
        ("inactive_layers", p.inactive_layers.is_some()),
        ("from_bottom", p.from_bottom.is_some()),
        ("ratsnest", p.ratsnest.is_some()),
    ];
    match pcb_only.iter().find(|(_, given)| *given) {
        Some((name, _)) => Err(CommandError::invalid_params(
            &format!("/{name}"),
            format!("a Schematic Sheet has a single Layer: leave out {name}"),
        )),
        None => Ok(()),
    }
}

/// The PCB Layers to draw, in order and at their opacity, from the params.
fn pcb_layers(pcb: &PcbDocument, p: &DocumentRenderParams) -> Result<Vec<Composite>, CommandError> {
    let set = pcb.layers();
    let layers = p.layers.as_deref().unwrap_or_default();
    if let Some(i) = layers.iter().position(|l| set.layer(*l).is_none()) {
        return Err(CommandError::invalid_params(
            &format!("/layers/{i}"),
            format!("{} is not a Layer of this PCB", layers[i]),
        ));
    }
    let shown = |l: Uuid| p.layers.as_ref().is_none_or(|v| v.contains(&l));
    if let Some(active) = p.active_layer {
        if set.layer(active).is_none() {
            return Err(CommandError::invalid_params(
                "/active_layer",
                format!("{active} is not a Layer of this PCB"),
            ));
        }
        if !shown(active) {
            return Err(CommandError::invalid_params(
                "/active_layer",
                "the active Layer must be one of layers",
            ));
        }
    }
    let active = p.active_layer.or_else(|| {
        set.find(LayerKind::Copper, Some(Side::Top))
            .map(|l| l.uuid)
            .filter(|l| shown(*l))
    });
    let view = LayerView {
        visible: p.layers.clone(),
        active,
        inactive: p.inactive_layers.unwrap_or_default(),
        from_bottom: p.from_bottom.unwrap_or(false),
        ratsnest: p.ratsnest.unwrap_or(true),
        pinned: Vec::new(),
    };
    Ok(board_composites(&set.layers, &view))
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

/// Draws a PNG on the shared headless device.
#[cfg(feature = "gpu")]
fn png(
    scene: &Scene,
    view: &Viewport,
    mirrored: bool,
    palette: &Palette,
    highlight: &[Uuid],
    layers: Option<&[Composite]>,
) -> Result<Vec<u8>, CommandError> {
    use crate::gpu::{Flags, Headless, Highlight, RenderError};
    use std::sync::OnceLock;

    // One device per process: opening an adapter is slow, and a missing one stays missing.
    static HEADLESS: OnceLock<Result<Headless, RenderError>> = OnceLock::new();
    let gpu = HEADLESS
        .get_or_init(Headless::new)
        .as_ref()
        .map_err(|e| no_renderer(e.to_string()))?;
    let mut marks = Highlight::new();
    marks.mark_all(highlight.iter().copied(), Flags::HIGHLIGHTED);
    let view = crate::View {
        mirrored,
        ..crate::View::of(view)
    };
    let image = gpu
        .render_layers(scene, &view, palette, &marks, layers)
        .map_err(|e| match e {
            RenderError::NoAdapter(_) | RenderError::NoDevice(_) => no_renderer(e.to_string()),
            _ => CommandError::internal(e.to_string()),
        })?;
    Ok(image.to_png())
}

#[cfg(not(feature = "gpu"))]
fn png(
    _scene: &Scene,
    _view: &Viewport,
    _mirrored: bool,
    _palette: &Palette,
    _highlight: &[Uuid],
    _layers: Option<&[Composite]>,
) -> Result<Vec<u8>, CommandError> {
    Err(no_renderer(
        "no GPU or software renderer is available: this build has no GPU backend; use \
         format \"svg\""
            .into(),
    ))
}

/// `unavailable`, naming the cause, with `details.cause = "no_renderer"`.
fn no_renderer(message: String) -> CommandError {
    CommandError {
        details: Some(serde_json::json!({"cause": "no_renderer"})),
        ..CommandError::unavailable(message)
    }
}
