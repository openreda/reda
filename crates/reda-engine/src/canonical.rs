//! The canonical JSON writer (ADR-0005): the same data always gives the same bytes.
//!
//! A file is one top-level object, written as:
//!
//! ```text
//! {
//!   "format": 1,
//!   "sheet": {"uuid":"…"},
//!   "wires": [
//!     {"uuid":"…","from":{"x":0,"y":0},"to":{"x":2540000,"y":0}}
//!   ]
//! }
//! ```
//!
//! - 2-space indent, UTF-8, LF line endings, a trailing newline.
//! - `"format"` comes first; the other keys follow in the order the caller writes them.
//! - Each value or table row is compact JSON on one line. Inside it, keys follow the
//!   Rust struct's field order, so a type's field order is part of the file format. Use
//!   `BTreeMap`, never `HashMap`, for maps inside a row.
//! - [`CanonicalWriter::table`] sorts rows by UUID; [`CanonicalWriter::list`] keeps the given
//!   order, for data whose order is meaningful (the Project's Document order).
//! - Values are integers only: a float anywhere is an error.
//!
//! Containers whose children each get their own line (Footprint instances with Pads, the
//! Board Outline) are not supported yet; they arrive with the first such object kind.

use std::fmt;

use reda_model::Uuid;
use serde::Serialize;
use serde_json::Value;

/// A value the canonical format cannot hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalError(String);

impl fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CanonicalError {}

/// Builds one canonical JSON file. Call [`CanonicalWriter::finish`] for the bytes.
pub struct CanonicalWriter {
    out: String,
}

impl CanonicalWriter {
    /// Starts a file of the given format version.
    pub fn new(format: u32) -> CanonicalWriter {
        CanonicalWriter {
            out: format!("{{\n  \"format\": {format}"),
        }
    }

    /// Writes `"key": <value>` with the value compact on one line.
    pub fn field<T: Serialize + ?Sized>(
        &mut self,
        key: &str,
        value: &T,
    ) -> Result<(), CanonicalError> {
        let line = compact(key, value)?;
        self.key(key);
        self.out.push_str(&line);
        Ok(())
    }

    /// Writes a table: one row per line, sorted by the UUID `uuid_of` returns. Two rows with
    /// the same UUID are an error.
    pub fn table<T: Serialize>(
        &mut self,
        key: &str,
        rows: impl IntoIterator<Item = T>,
        uuid_of: impl Fn(&T) -> Uuid,
    ) -> Result<(), CanonicalError> {
        let mut rows: Vec<(Uuid, T)> = rows.into_iter().map(|r| (uuid_of(&r), r)).collect();
        rows.sort_by_key(|(uuid, _)| *uuid);
        if let Some(pair) = rows.windows(2).find(|pair| pair[0].0 == pair[1].0) {
            return Err(CanonicalError(format!(
                "table {key:?} holds UUID {} twice",
                pair[0].0
            )));
        }
        self.list(key, rows.into_iter().map(|(_, row)| row))
    }

    /// Writes an array: one element per line, in the given order.
    pub fn list<T: Serialize>(
        &mut self,
        key: &str,
        rows: impl IntoIterator<Item = T>,
    ) -> Result<(), CanonicalError> {
        let lines = rows
            .into_iter()
            .map(|row| compact(key, &row))
            .collect::<Result<Vec<_>, _>>()?;
        self.key(key);
        if lines.is_empty() {
            self.out.push_str("[]");
            return Ok(());
        }
        self.out.push('[');
        for (i, line) in lines.iter().enumerate() {
            self.out.push_str(if i == 0 { "\n    " } else { ",\n    " });
            self.out.push_str(line);
        }
        self.out.push_str("\n  ]");
        Ok(())
    }

    /// The finished file.
    pub fn finish(mut self) -> Vec<u8> {
        self.out.push_str("\n}\n");
        self.out.into_bytes()
    }

    fn key(&mut self, key: &str) {
        self.out.push_str(",\n  ");
        self.out
            .push_str(&serde_json::to_string(key).expect("a string serialises"));
        self.out.push_str(": ");
    }
}

/// `value` as compact JSON, refusing floats.
fn compact<T: Serialize + ?Sized>(key: &str, value: &T) -> Result<String, CanonicalError> {
    let fail = |e: serde_json::Error| CanonicalError(format!("{key:?}: {e}"));
    let tree = serde_json::to_value(value).map_err(fail)?;
    if has_float(&tree) {
        return Err(CanonicalError(format!(
            "{key:?} holds a non-integer number; stored values are integers only"
        )));
    }
    serde_json::to_string(value).map_err(fail)
}

fn has_float(value: &Value) -> bool {
    match value {
        Value::Number(n) => n.is_f64(),
        Value::Array(items) => items.iter().any(has_float),
        Value::Object(map) => map.values().any(has_float),
        _ => false,
    }
}
