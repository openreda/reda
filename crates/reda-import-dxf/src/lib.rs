//! DXF import into the PCB Document. Everything is written in-house, with no DXF crate
//! (docs/spec/v0.3/dxf.md):
//!
//! - [`tags`]: the tag-pair parser, one group code and its value per two lines of an ASCII
//!   file, or read from a binary one ([`binary`]); [`encoding`] decodes their text, UTF-8
//!   or Windows-1252, and DWG is refused;
//! - [`reader`]: the HEADER units, the BLOCKS an INSERT explodes and the model-space
//!   entities, as [`Shape`]s in drawing units seen from above ([`transform`]), and the
//!   entities skipped, counted per type;
//! - [`shape`]: each shape as straight and circular [`Piece`]s, bulges as exact arcs,
//!   ellipses and [`Spline`]s fitted with arcs ([`fit`]); [`text`] is TEXT and MTEXT;
//! - [`units`]: `$INSUNITS` and the units a drawing can be read in;
//! - [`mapping`]: DXF layer to board target, with the four pre-fill rules;
//! - [`import`]: the report a preview returns, and the Layers and graphics an import adds;
//! - [`commands`]: `pcb.import_dxf.preview` and `pcb.import_dxf`.

mod binary;
mod commands;
mod encoding;
mod fit;
pub mod import;
pub mod mapping;
mod reader;
mod shape;
mod spline;
mod tags;
mod text;
mod transform;
pub mod units;

pub use commands::{Import, ImportParams, ImportResult, Preview, PreviewParams, register_all};
pub use reader::{Drawing, Entity, read};
pub use shape::{Piece, Shape, Vertex, Xy};
pub use spline::Spline;
pub use tags::{Place, ReadError};
pub use text::Text;
