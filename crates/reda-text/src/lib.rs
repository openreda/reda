//! Text geometry shared by the canvas, Gerber and PDF.
//!
//! [`layout`] turns a string, a [`Font`] and a capital height into [`TextGeometry`]: capsule
//! [`Segment`]s for Hershey stroke text, and a triangulated [`Mesh`] for TTF/OTF outline text.
//! Fonts load from bytes only; finding installed fonts is the app's job.

mod geometry;
mod layout;
mod outline;
mod stroke;

pub use geometry::{Mesh, Segment, TextGeometry};
pub use layout::{Font, layout};
pub use outline::{FontError, OutlineFont};
pub use stroke::{HersheyPoint, JhfError, StrokeFont, StrokeGlyph};
