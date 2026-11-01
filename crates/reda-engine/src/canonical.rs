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
//! - A container (the Board Outline, a Footprint instance with its Pads) spreads one array
//!   field over several lines, one child per line, with [`CanonicalWriter::container`] or,
//!   for a table of them, [`CanonicalWriter::containers`]. The rest of the row stays
//!   compact on its first line:
//!
//! ```text
//!   "board_outline": {"uuid":"…","vertices":[
//!     {"x":0,"y":0},
//!     {"x":100000000,"y":0}
//!   ]},
//!   "board_cutouts": [
//!     {"uuid":"…","vertices":[
//!       {"x":10000000,"y":10000000},
//!       …
//!     ]}
//!   ]
//! ```

use std::fmt;
use std::io;

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
        self.list(key, sorted(key, rows, uuid_of)?)
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
        self.lines(key, lines);
        Ok(())
    }

    /// Writes `"key": <value>` as a container: compact, except that each element of its
    /// array field `children` goes on its own line. The value must have that field.
    pub fn container<T: Serialize + ?Sized>(
        &mut self,
        key: &str,
        value: &T,
        children: &str,
    ) -> Result<(), CanonicalError> {
        let text = spread(key, value, children, 2)?;
        self.key(key);
        self.out.push_str(&text);
        Ok(())
    }

    /// Writes a table of containers (see [`CanonicalWriter::container`]), sorted by UUID as
    /// [`CanonicalWriter::table`] sorts.
    pub fn containers<T: Serialize>(
        &mut self,
        key: &str,
        rows: impl IntoIterator<Item = T>,
        uuid_of: impl Fn(&T) -> Uuid,
        children: &str,
    ) -> Result<(), CanonicalError> {
        let lines = sorted(key, rows, uuid_of)?
            .iter()
            .map(|row| spread(key, row, children, 4))
            .collect::<Result<Vec<_>, _>>()?;
        self.lines(key, lines);
        Ok(())
    }

    /// The finished file.
    pub fn finish(mut self) -> Vec<u8> {
        self.out.push_str("\n}\n");
        self.out.into_bytes()
    }

    /// Writes `"key": [` and one pre-rendered element per line.
    fn lines(&mut self, key: &str, lines: Vec<String>) {
        self.key(key);
        if lines.is_empty() {
            self.out.push_str("[]");
            return;
        }
        self.out.push('[');
        for (i, line) in lines.iter().enumerate() {
            self.out.push_str(if i == 0 { "\n    " } else { ",\n    " });
            self.out.push_str(line);
        }
        self.out.push_str("\n  ]");
    }

    fn key(&mut self, key: &str) {
        self.out.push_str(",\n  ");
        self.out
            .push_str(&serde_json::to_string(key).expect("a string serialises"));
        self.out.push_str(": ");
    }
}

/// The rows sorted by UUID; two rows with the same UUID are an error.
fn sorted<T>(
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
    check_integers(key, value)?;
    serde_json::to_string(value).map_err(|e| CanonicalError(format!("{key:?}: {e}")))
}

/// `value` as compact JSON with each element of its array field `children` on its own
/// line, indented two more than `indent`, the row's own indent.
fn spread<T: Serialize + ?Sized>(
    key: &str,
    value: &T,
    children: &str,
    indent: usize,
) -> Result<String, CanonicalError> {
    check_integers(key, value)?;
    let mut formatter = Spread {
        children,
        indent,
        depth: 0,
        arrays: Vec::new(),
        key: None,
        last_key: String::new(),
        found: false,
    };
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, &mut formatter);
    value
        .serialize(&mut ser)
        .map_err(|e| CanonicalError(format!("{key:?}: {e}")))?;
    if !formatter.found {
        return Err(CanonicalError(format!(
            "{key:?} has no array field {children:?} to spread over lines"
        )));
    }
    Ok(String::from_utf8(out).expect("serde_json writes UTF-8"))
}

fn check_integers<T: Serialize + ?Sized>(key: &str, value: &T) -> Result<(), CanonicalError> {
    let tree = serde_json::to_value(value).map_err(|e| CanonicalError(format!("{key:?}: {e}")))?;
    if has_float(&tree) {
        return Err(CanonicalError(format!(
            "{key:?} holds a non-integer number; stored values are integers only"
        )));
    }
    Ok(())
}

fn has_float(value: &Value) -> bool {
    match value {
        Value::Number(n) => n.is_f64(),
        Value::Array(items) => items.iter().any(has_float),
        Value::Object(map) => map.values().any(has_float),
        _ => false,
    }
}

/// serde_json's compact output, except that the elements of one array field of the row
/// itself go on their own lines.
struct Spread<'a> {
    /// The field whose elements are spread.
    children: &'a str,
    /// The row's indent; children get two more.
    indent: usize,
    /// How many objects are open; the row itself is depth 1.
    depth: usize,
    /// For each open array: whether it is spread, and whether it has an element yet.
    arrays: Vec<(bool, bool)>,
    /// The key being written, while it is one of the row's own keys.
    key: Option<String>,
    /// The row's key written last.
    last_key: String,
    /// Whether the children field was met.
    found: bool,
}

impl serde_json::ser::Formatter for &mut Spread<'_> {
    fn begin_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.depth += 1;
        w.write_all(b"{")
    }

    fn end_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.depth -= 1;
        w.write_all(b"}")
    }

    fn begin_object_key<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if self.depth == 1 && self.arrays.is_empty() {
            self.key = Some(String::new());
        }
        if first { Ok(()) } else { w.write_all(b",") }
    }

    fn end_object_key<W: ?Sized + io::Write>(&mut self, _: &mut W) -> io::Result<()> {
        if let Some(key) = self.key.take() {
            self.last_key = key;
        }
        Ok(())
    }

    fn write_string_fragment<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        if let Some(key) = &mut self.key {
            key.push_str(fragment);
        }
        w.write_all(fragment.as_bytes())
    }

    fn begin_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        // An array straight under the row: the value of the row's last key.
        let spread = self.depth == 1 && self.arrays.is_empty() && self.last_key == self.children;
        self.found |= spread;
        self.arrays.push((spread, false));
        w.write_all(b"[")
    }

    fn begin_array_value<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if !first {
            w.write_all(b",")?;
        }
        let indent = self.indent + 2;
        let top = self.arrays.last_mut().expect("inside an array");
        top.1 = true;
        if top.0 {
            write!(w, "\n{:indent$}", "")?;
        }
        Ok(())
    }

    fn end_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        let (spread, any) = self.arrays.pop().expect("inside an array");
        if spread && any {
            write!(w, "\n{:indent$}", "", indent = self.indent)?;
        }
        w.write_all(b"]")
    }
}
