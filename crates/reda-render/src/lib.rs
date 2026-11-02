//! Canvas rendering.
//!
//! Each Document is turned into a GPU-free [`Scene`]: a flat list of primitives (capsules,
//! rects, rings, triangle lists and glyph runs), each tagged with its object UUID and
//! Layer. The wgpu backend, `document.render` and the PDF and SVG writers all draw from it.
//! [`Scene::to_text`] serialises a Scene for snapshot tests.

mod arc;
mod base64;
mod color;
mod command;
#[cfg(feature = "gpu")]
pub mod gpu;
mod layers;
mod outline;
mod pcb;
mod pdf;
mod polygon;
mod scene;
mod schematic;
mod snapshot;
mod svg;
mod text;
mod transform;
mod view;
mod viewport;

pub use color::{BoardColor, ColorKey, Palette, Rgba, SchematicColor, Theme};
pub use command::{
    BoardParts, Derive, DocumentRender, DocumentRenderParams, DocumentRenderResult, Junctions,
    RenderFormat, RenderPoint, RenderTheme, RenderView, document_scene, register_all,
};
pub use layers::{
    Composite, DIMMED_OPACITY, InactiveLayers, LayerView, NORMAL_OPACITY, board_composites,
};
pub use pcb::{
    BoardGeometry, BoardGraphic, BoardPad, BoardText, RatsnestLine, ZoneFillState, pcb_scene,
};
pub use pdf::{PdfInfo, to_pdf};
pub use reda_text::{Mesh, Segment};
pub use scene::{Align, GlyphRun, HAlign, Item, Primitive, Scene, SceneLayer, SceneSource, VAlign};
pub use schematic::schematic_scene;
pub use svg::{to_svg, to_svg_layers};
pub use view::View;
pub use viewport::Viewport;
