//! Canvas rendering.
//!
//! Each Document is turned into a GPU-free [`Scene`]: a flat list of primitives (capsules,
//! rects, rings, triangle lists and glyph runs), each tagged with its object UUID and
//! Layer. The wgpu backend, `document.render` and the PDF and SVG writers all draw from it.
//! [`Scene::to_text`] serialises a Scene for snapshot tests.

mod color;
mod outline;
mod pdf;
mod polygon;
mod scene;
mod schematic;
mod snapshot;
mod svg;
mod text;
mod transform;
mod viewport;

pub use color::{ColorKey, Palette, Rgba, SchematicColor, Theme};
pub use pdf::{PdfInfo, to_pdf};
pub use scene::{Align, GlyphRun, HAlign, Item, Primitive, Scene, SceneLayer, SceneSource, VAlign};
pub use schematic::schematic_scene;
pub use svg::to_svg;
pub use viewport::Viewport;
