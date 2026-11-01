//! The native file format: per-kind file shapes, version checks and migrations.
//!
//! Reading runs raw JSON through the migration chain, then deserialises it into the
//! file's typed shape, where unknown fields are an error. Writing always produces the
//! current [`FORMAT`] through the canonical writer.

mod project_file;
mod sheet_file;

use std::fmt;
use std::path::{Path, PathBuf};

use reda_command::CommandError;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(crate) use project_file::{ProjectFile, write_project_file};
pub(crate) use sheet_file::{read_sheet_file, write_sheet_file};

/// The format version REDA writes. Bump it together with a new [`MIGRATIONS`] step.
pub const FORMAT: u32 = 1;

/// One migration step: rewrites raw JSON of format `n` into format `n + 1`.
type Migration = fn(Value) -> Result<Value, String>;

/// `MIGRATIONS[i]` turns format `i + 1` into `i + 2`. Steps are kept forever, and each one
/// gets a golden test.
const MIGRATIONS: &[Migration] = &[];

const _: () = assert!(MIGRATIONS.len() == FORMAT as usize - 1);

/// Why a Project could not be read.
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
pub(crate) fn parse<T: DeserializeOwned>(path: &Path, bytes: &[u8]) -> Result<T, LoadError> {
    let invalid = |why: String| LoadError::Invalid(path.to_owned(), why);
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| invalid(format!("not valid JSON: {e}")))?;
    let value = migrate(value).map_err(invalid)?;
    serde_json::from_value(value).map_err(|e| invalid(e.to_string()))
}

/// Brings raw JSON of any supported format up to [`FORMAT`].
fn migrate(mut value: Value) -> Result<Value, String> {
    let version = value
        .get("format")
        .and_then(Value::as_u64)
        .ok_or("has no \"format\" version number")?;
    if version > u64::from(FORMAT) {
        return Err(format!(
            "was written by a newer REDA (format {version}); this REDA reads up to format {FORMAT}"
        ));
    }
    if version == 0 {
        return Err("format 0 does not exist".into());
    }
    for (from, step) in MIGRATIONS.iter().enumerate().skip(version as usize - 1) {
        value = step(value).map_err(|e| format!("migrating from format {}: {e}", from + 1))?;
        value["format"] = Value::from(from + 2);
    }
    Ok(value)
}
