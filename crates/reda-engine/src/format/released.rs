//! The JSON Schemas each release shipped, frozen in `docs/schemas/v<n>/`.
//!
//! A migration step reads a file as its format was declared, so before the step runs
//! the file is checked against that format's frozen schemas, not the current ones.

use std::sync::LazyLock;

use serde_json::Value;

use super::{FileKind, RELEASED};

const _: () = assert!(RELEASED == 2, "add the new release's frozen schemas here");

/// Checks `file` against the schema format `format` shipped for `kind`. Formats no release
/// shipped, and kinds a format did not have, have nothing to check against and fail.
pub fn check_released(format: u32, kind: FileKind, file: &Value) -> Result<(), String> {
    let schema = match format {
        1 => v1(kind),
        2 => v2(kind),
        _ => return Err(format!("format {format} was never released")),
    }
    .ok_or_else(|| format!("format {format} had no {} files", kind.extension()))?;
    reda_command::check_schema(schema, file).map_err(|e| format!("{}: {}", e.pointer, e.message))
}

/// One frozen schema, parsed once and kept across all Project Documents.
macro_rules! frozen {
    ($version:literal, $ext:literal) => {{
        static SCHEMA: LazyLock<Value> = LazyLock::new(|| {
            serde_json::from_str(include_str!(concat!(
                "../../../../docs/schemas/",
                $version,
                "/",
                $ext,
                ".schema.json"
            )))
            .expect("a frozen schema is JSON")
        });
        &*SCHEMA
    }};
}

fn v1(kind: FileKind) -> Option<&'static Value> {
    Some(match kind {
        FileKind::Project => frozen!("v1", "reda"),
        FileKind::Pcb => frozen!("v1", "redapcb"),
        FileKind::Sheet => frozen!("v1", "redasch"),
        FileKind::Library => frozen!("v1", "redalib"),
        FileKind::Symbol => frozen!("v1", "redasym"),
        FileKind::Footprint => frozen!("v1", "redafp"),
        FileKind::Part => frozen!("v1", "redapart"),
        FileKind::FabPreset => frozen!("v1", "redafab"),
        FileKind::Rules => frozen!("v1", "redarules"),
        FileKind::BoardTemplate => return None,
    })
}

fn v2(kind: FileKind) -> Option<&'static Value> {
    Some(match kind {
        FileKind::Project => frozen!("v2", "reda"),
        FileKind::Pcb => frozen!("v2", "redapcb"),
        FileKind::Sheet => frozen!("v2", "redasch"),
        FileKind::Library => frozen!("v2", "redalib"),
        FileKind::Symbol => frozen!("v2", "redasym"),
        FileKind::Footprint => frozen!("v2", "redafp"),
        FileKind::Part => frozen!("v2", "redapart"),
        FileKind::FabPreset => frozen!("v2", "redafab"),
        FileKind::Rules => frozen!("v2", "redarules"),
        FileKind::BoardTemplate => frozen!("v2", "redaboard"),
    })
}
