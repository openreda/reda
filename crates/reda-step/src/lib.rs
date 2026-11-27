//! STEP (ISO 10303-21) for REDA: one typed record model that files are read into and written
//! from, so anything REDA reads it can write.
//!
//! - [`read`] parses `.step`, `.stp` and `.stpz` (gzip) files in AP203, AP214 or AP242 into a
//!   [`StepModel`] of [`records`]: product structure, placements, B-rep shapes, styles and
//!   units. It reads what it can: a malformed or dangling instance is skipped with a warning.
//! - [`StepModel::to_millimetres`] converts a model's lengths to millimetres and its angles to
//!   radians.
//! - [`write`] writes a model as AP214, and [`write_file`] gzips it when the name ends in
//!   `.stpz`.
//! - [`StepModel::mesh`] tessellates a model's faces into coloured meshes, keeping its assembly
//!   tree and placements; [`load`] reads, converts to millimetres and meshes in one go.
//!
//! REDA takes no CAD kernel (ADR-0018): the parser, writer and records are REDA's own.

mod mesh;
mod model;
pub mod part21;
mod read;
pub mod records;
mod units;
mod write;

pub use mesh::{
    BoundingBox, Colour, Instance, Loaded, Mesh, MeshFace, MeshOptions, MeshedModel, Node, Part,
    Solid, Transform, load,
};
pub use model::{AP214_APPLICATION, AP214_SCHEMA, Header, Protocol, StepModel};
pub use read::{Read, ReadError, read, read_file};
pub use records::{Record, RecordId};
pub use units::UnitReport;
pub use write::{Compression, encode, write, write_file};
