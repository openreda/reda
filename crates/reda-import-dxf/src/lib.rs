//! DXF import into the PCB Document. Everything is written in-house, with no DXF crate
//! (docs/spec/v0.3/dxf.md):
//!
//! - [`tags`]: the tag-pair parser, one group code and its value per two lines;
//! - [`reader`]: the HEADER units and the model-space entities, as [`Shape`]s in drawing
//!   units, and the entities skipped, counted per type;
//! - [`shape`]: each shape as straight and circular [`Piece`]s, bulges as exact arcs;
//! - [`units`]: `$INSUNITS` and the units a drawing can be read in;
//! - [`mapping`]: DXF layer to board target, with the four pre-fill rules;
//! - [`import`]: the report a preview returns, and the Layers and graphics an import adds;
//! - [`commands`]: `pcb.import_dxf.preview` and `pcb.import_dxf`.

mod commands;
pub mod import;
pub mod mapping;
mod reader;
mod shape;
mod tags;
pub mod units;

pub use commands::{Import, ImportParams, ImportResult, Preview, PreviewParams, register_all};
pub use reader::{Drawing, Entity, read};
pub use shape::{Piece, Shape, Vertex, Xy};
pub use tags::ReadError;
