//! Text geometry shared by the canvas, Gerber and PDF.
//!
//! [`layout`] turns a string, a [`Font`] and a capital height into [`TextGeometry`]: capsule
//! [`Segment`]s for Hershey stroke text, and a triangulated [`Mesh`] for TTF/OTF outline text,
//! with each glyph's simplified contours kept as [`Region`]s for writers that need them.
//! Fonts load from bytes only; finding installed fonts is the app's job.

mod geometry;
mod layout;
mod outline;
mod stroke;

pub use geometry::{Mesh, Region, Segment, TextGeometry};
pub use layout::{Font, layout};
pub use outline::{FontError, OutlineFont};
pub use stroke::{HersheyPoint, JhfError, StrokeFont, StrokeGlyph};
