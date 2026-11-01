//! JSON Schemas of the file kinds, generated from the typed file shapes with `schemars`.
//!
//! The checked-in copies live in `docs/schemas/<extension>.schema.json`; a test in
//! `crates/reda-engine/tests/schemas.rs` keeps them fresh.

use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde_json::Value;

use super::library::{FootprintFile, LibraryFile, PartFile, SymbolFile};
use super::pcb_file::PcbFile;
use super::project_file::ProjectFile;
use super::sheet_file::SheetFile;
use super::{FORMAT, FileKind};

/// The JSON Schema (draft 2020-12) of the current format of `kind`.
pub fn schema(kind: FileKind) -> Value {
    match kind {
        FileKind::Project => schema_of::<ProjectFile>(),
        FileKind::Sheet => schema_of::<SheetFile>(),
        FileKind::Pcb => schema_of::<PcbFile>(),
        FileKind::Library => schema_of::<LibraryFile>(),
        FileKind::Symbol => schema_of::<SymbolFile>(),
        FileKind::Footprint => schema_of::<FootprintFile>(),
        FileKind::Part => schema_of::<PartFile>(),
    }
}

fn schema_of<T: JsonSchema>() -> Value {
    let schema = SchemaSettings::draft2020_12()
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<T>();
    schema.to_value()
}

/// `"format"`: always the current version in files REDA writes.
pub(super) fn format_version(_: &mut SchemaGenerator) -> Schema {
    json_schema!({ "type": "integer", "const": FORMAT })
}

/// A UUID: hyphenated hex, written lowercase; either case is read.
pub(super) fn uuid(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "string",
        "format": "uuid",
        "pattern": "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
    })
}
