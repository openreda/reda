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
//!   ],
//!   "parts": [
//!     {
//!       "part": {"uuid":"…","name":"LM358"},
//!       "pins": [
//!         {"uuid":"…","number":"1"}
//!       ]
//!     }
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
//! - [`CanonicalWriter::containers`] writes a table of containers (part-table entries,
//!   Footprint instances with their Pads): each row is an object spanning lines, whose
//!   values, table rows and list elements each sit on their own line, one level deeper.
//! - Values are integers only: a float anywhere is an error.

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

/// Builds one canonical JSON file. Call [`CanonicalWriter::finish`] for the bytes. It
/// derefs to the [`ObjectWriter`] of the file's top-level object.
pub struct CanonicalWriter {
    body: ObjectWriter,
}

impl CanonicalWriter {
    /// Starts a file of the given format version.
    pub fn new(format: u32) -> CanonicalWriter {
        let mut body = ObjectWriter::new(1);
        body.key("format");
        body.out.push_str(&format.to_string());
        CanonicalWriter { body }
    }

    /// The finished file.
    pub fn finish(self) -> Vec<u8> {
        let mut out = self.body.out;
        out.push_str("\n}\n");
        out.into_bytes()
    }
}

impl std::ops::Deref for CanonicalWriter {
    type Target = ObjectWriter;
    fn deref(&self) -> &ObjectWriter {
        &self.body
    }
}

impl std::ops::DerefMut for CanonicalWriter {
    fn deref_mut(&mut self) -> &mut ObjectWriter {
        &mut self.body
    }
}

/// The keys of one multi-line object: a whole file, or one container row of
/// [`ObjectWriter::containers`]. Keys follow in the order they are written.
pub struct ObjectWriter {
    /// Starts with `{`; the closing brace is added by the owner.
    out: String,
    /// The nesting depth of this object's keys; each level is two spaces.
    depth: usize,
}

impl ObjectWriter {
    fn new(depth: usize) -> ObjectWriter {
        ObjectWriter {
            out: "{".into(),
            depth,
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
        let rows = sorted_by_uuid(key, rows, uuid_of)?;
        self.list(key, rows)
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
        self.array(key, lines);
        Ok(())
    }

    /// Writes a table of containers, sorted by the UUID `uuid_of` returns. Each row is an
    /// object spanning lines, whose keys `write` fills in with the same calls as a file's:
    /// values, table rows and list elements each on their own line, one level deeper.
    /// Containers nest. Two rows with the same UUID are an error.
    pub fn containers<T>(
        &mut self,
        key: &str,
        rows: impl IntoIterator<Item = T>,
        uuid_of: impl Fn(&T) -> Uuid,
        mut write: impl FnMut(T, &mut ObjectWriter) -> Result<(), CanonicalError>,
    ) -> Result<(), CanonicalError> {
        let rows = sorted_by_uuid(key, rows, uuid_of)?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let mut child = ObjectWriter::new(self.depth + 2);
            write(row, &mut child)?;
            child.out.push('\n');
            child.out.push_str(&indent(self.depth + 1));
            child.out.push('}');
            items.push(child.out);
        }
        self.array(key, items);
        Ok(())
    }

    /// Writes `"key": [...]` with each already-rendered item on its own line.
    fn array(&mut self, key: &str, items: Vec<String>) {
        self.key(key);
        if items.is_empty() {
            self.out.push_str("[]");
            return;
        }
        let item_indent = indent(self.depth + 1);
        self.out.push('[');
        for (i, item) in items.iter().enumerate() {
            self.out.push_str(if i == 0 { "\n" } else { ",\n" });
            self.out.push_str(&item_indent);
            self.out.push_str(item);
        }
        self.out.push('\n');
        self.out.push_str(&indent(self.depth));
        self.out.push(']');
    }

    fn key(&mut self, key: &str) {
        if self.out != "{" {
            self.out.push(',');
        }
        self.out.push('\n');
        self.out.push_str(&indent(self.depth));
        self.out
            .push_str(&serde_json::to_string(key).expect("a string serialises"));
        self.out.push_str(": ");
    }
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

/// The rows sorted by UUID, refusing a UUID that appears twice.
fn sorted_by_uuid<T>(
    key: &str,
    rows: impl IntoIterator<Item = T>,
    uuid_of: impl Fn(&T) -> Uuid,
) -> Result<Vec<T>, CanonicalError> {
    let mut rows: Vec<(Uuid, T)> = rows.into_iter().map(|r| (uuid_of(&r), r)).collect();
    rows.sort_by_key(|(uuid, _)| *uuid);
    if let Some(pair) = rows.windows(2).find(|pair| pair[0].0 == pair[1].0) {
        return Err(CanonicalError(format!(
            "table {key:?} holds UUID {} twice",
            pair[0].0
        )));
    }
    Ok(rows.into_iter().map(|(_, row)| row).collect())
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
