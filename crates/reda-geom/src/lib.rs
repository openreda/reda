//! Geometry primitives: nanometre and millidegree types, unit strings, shapes.
//!
//! Stored design data is integer only: lengths are `i64` nanometres and angles are `i64`
//! millidegrees. Rounding happens only when geometry is computed, never in stored data.

mod angle;
mod contour;
mod length;
mod units;

pub use angle::{Millideg, rotate};
pub use contour::{ContourError, Segment, Vertex, chain, check_contour, rounded_rect};
pub use length::{Nm, Point};
pub use units::UnitError;
