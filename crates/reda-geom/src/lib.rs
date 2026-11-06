//! Geometry primitives: nanometre and millidegree types, unit strings, shapes.
//!
//! Stored design data is integer only: lengths are `i64` nanometres and angles are `i64`
//! millidegrees. Rounding happens only when geometry is computed, never in stored data.

mod angle;
mod bounds;
mod contour;
mod corner_rect;
mod length;
mod units;

pub use angle::{Millideg, rotate};
pub use bounds::Bounds;
pub use contour::{ContourError, Segment, Vertex, chain, check_contour, rounded_rect};
pub use corner_rect::{CornerCut, CornerRect};
pub use length::{Nm, Point};
pub use units::{NM_PER_OZ, UnitError};
