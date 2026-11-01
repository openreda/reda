//! The native file format: per-kind file shapes, version checks and migrations.
//!
//! Reading runs raw JSON through the migration chain, then deserialises it into the
//! file's typed shape, where unknown fields are an error. Writing always produces the
//! current [`FORMAT`] through the canonical writer.

mod fab_file;
pub mod library;
mod pcb_file;
mod project_file;
mod schema;
mod sheet_file;

use std::fmt;
use std::path::{Path, PathBuf};

use reda_command::CommandError;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(crate) use fab_file::{read_fab_file, write_fab_file};
pub(crate) use pcb_file::{read_pcb_file, write_pcb_file};
pub(crate) use project_file::{ProjectFile, write_project_file};
pub use schema::schema;
pub(crate) use sheet_file::{read_sheet_file, write_sheet_file};

/// The format version REDA writes. Bump it together with a new [`MIGRATIONS`] step.
pub const FORMAT: u32 = 1;

/// The kinds of file REDA reads and writes, each with one typed shape and one JSON Schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileKind {
    /// `<name>.reda`: the Project properties and the ordered Document list.
    Project,
    /// `<name>.redasch`: one Schematic Sheet.
    Sheet,
    /// `<name>.redapcb`: the PCB Document.
    Pcb,
    /// `<name>.redalib`: marks a folder as a Library.
    Library,
    /// `<name>.redasym`: one Library Symbol.
    Symbol,
    /// `<name>.redafp`: one Library Footprint.
    Footprint,
    /// `<name>.redapart`: one Library Part.
    Part,
    /// `<name>.redafab`: one user Fab Preset.
    FabPreset,
}

impl FileKind {
    pub const ALL: [FileKind; 8] = [
        FileKind::Project,
        FileKind::Sheet,
        FileKind::Pcb,
        FileKind::Library,
        FileKind::Symbol,
        FileKind::Footprint,
        FileKind::Part,
        FileKind::FabPreset,
    ];

    /// The file extension, without the dot.
    pub const fn extension(self) -> &'static str {
        match self {
            FileKind::Project => "reda",
            FileKind::Sheet => "redasch",
            FileKind::Pcb => "redapcb",
            FileKind::Library => "redalib",
            FileKind::Symbol => "redasym",
            FileKind::Footprint => "redafp",
            FileKind::Part => "redapart",
            FileKind::FabPreset => "redafab",
        }
    }
}

/// One migration step: rewrites raw JSON of format `n` into format `n + 1`. It gets the
/// file kind so it can leave kinds it does not change alone. The chain stamps `"format"`.
pub type Migration = fn(FileKind, Value) -> Result<Value, String>;

/// `MIGRATIONS[i]` turns format `i + 1` into `i + 2`. Steps are kept forever, and each one
/// gets golden files under `crates/reda-engine/tests/golden/migrations/`.
pub const MIGRATIONS: &[Migration] = &[];

const _: () = assert!(MIGRATIONS.len() == FORMAT as usize - 1);

/// Why a Project or Library file could not be read.
#[derive(Debug)]
pub enum LoadError {
    Missing(PathBuf),
    Io(PathBuf, std::io::Error),
    /// The file exists but is not a valid REDA file of a format this REDA reads.
    Invalid(PathBuf, String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Missing(path) => write!(f, "{} does not exist", path.display()),
            LoadError::Io(path, e) => write!(f, "reading {}: {e}", path.display()),
            LoadError::Invalid(path, why) => write!(f, "{}: {why}", path.display()),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<LoadError> for CommandError {
    fn from(err: LoadError) -> CommandError {
        match err {
            LoadError::Missing(_) => CommandError::not_found(err.to_string()),
            LoadError::Io(..) => CommandError::internal(err.to_string()),
            LoadError::Invalid(..) => CommandError::rejected(err.to_string()),
        }
    }
}

/// Reads a file from disk.
pub(crate) fn read_bytes(path: &Path) -> Result<Vec<u8>, LoadError> {
    std::fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => LoadError::Missing(path.to_owned()),
        _ => LoadError::Io(path.to_owned(), e),
    })
}

/// Parses `bytes` as a REDA file of any supported format and deserialises the current
/// format's shape `T`.
pub(crate) fn parse<T: DeserializeOwned>(
    kind: FileKind,
    path: &Path,
    bytes: &[u8],
) -> Result<T, LoadError> {
    let invalid = |why: String| LoadError::Invalid(path.to_owned(), why);
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| invalid(format!("not valid JSON: {e}")))?;
    let value = migrate(kind, value).map_err(invalid)?;
    serde_json::from_value(value).map_err(|e| invalid(e.to_string()))
}

/// Brings raw JSON of any supported format up to [`FORMAT`].
pub fn migrate(kind: FileKind, value: Value) -> Result<Value, String> {
    migrate_with(kind, value, MIGRATIONS)
}

/// Runs the chain `steps`, where `steps[i]` turns format `i + 1` into `i + 2`, bringing raw
/// JSON up to format `steps.len() + 1`. [`migrate`] passes [`MIGRATIONS`]; tests pass their
/// own steps.
pub fn migrate_with(
    kind: FileKind,
    mut value: Value,
    steps: &[Migration],
) -> Result<Value, String> {
    let current = steps.len() as u64 + 1;
    let version = value
        .get("format")
        .and_then(Value::as_u64)
        .ok_or("has no \"format\" version number")?;
    if version > current {
        return Err(format!(
            "was written by a newer REDA (format {version}); this REDA reads up to format {current}"
        ));
    }
    if version == 0 {
        return Err("format 0 does not exist".into());
    }
    for (from, step) in steps.iter().enumerate().skip(version as usize - 1) {
        let fail = |e: String| format!("migrating from format {}: {e}", from + 1);
        value = step(kind, value).map_err(fail)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| fail("the step did not return a JSON object".into()))?;
        object.insert("format".into(), Value::from(from + 2));
    }
    Ok(value)
}
