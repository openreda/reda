//! The tag-pair layer of a DXF file: every group is a code and a value (DXF Reference,
//! "DXF Format" > "Group Codes in Numerical Order"). In an ASCII file each is a line; a
//! binary file ([`crate::binary`]) holds the same groups as bytes. Both become one list of
//! [`Tag`]s with their text decoded ([`crate::encoding`]); the entity layer above reads
//! those. Nothing here knows what a code means beyond which ones hold numbers.

use std::borrow::Cow;
use std::fmt;

use crate::binary;
use crate::encoding;

/// Where in the file a group is: a 1-based line of an ASCII file, or a byte offset of a
/// binary one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Line(usize),
    Byte(usize),
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::Line(line) => write!(f, "line {line}"),
            Place::Byte(byte) => write!(f, "byte {byte}"),
        }
    }
}

/// Why a file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The file is a DWG drawing, which import does not read.
    Dwg,
    /// The file is not readable DXF; `at` is where reading stopped, when one place is to
    /// blame.
    Malformed { at: Option<Place>, message: String },
}

impl ReadError {
    pub(crate) fn at(at: Place, message: impl Into<String>) -> ReadError {
        ReadError::Malformed {
            at: Some(at),
            message: message.into(),
        }
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Dwg => f.write_str(
                "it is a DWG drawing, and DXF import does not read DWG; save it as DXF from \
                 the CAD tool that made it and import that",
            ),
            ReadError::Malformed {
                at: Some(at),
                message,
            } => write!(f, "{at}: {message}"),
            ReadError::Malformed { at: None, message } => f.write_str(message),
        }
    }
}

impl std::error::Error for ReadError {}

/// One group as read, before its text is decoded. A binary file's numbers are written out
/// as text here, so every value reads the same way.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Raw<'a> {
    pub code: i32,
    pub value: Cow<'a, [u8]>,
    /// Where the value is.
    pub at: Place,
}

/// One group: its code, its value's text, and where the value is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tag<'a> {
    pub code: i32,
    pub value: Cow<'a, str>,
    pub at: Place,
}

impl<'a> Tag<'a> {
    pub fn error(&self, message: impl Into<String>) -> ReadError {
        ReadError::at(self.at, message)
    }

    /// The value of a real-valued group (codes 10–59, 110–149, 210–239, 1010–1059).
    pub fn real(&self) -> Result<f64, ReadError> {
        let text = self.value.trim();
        text.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| {
                self.error(format!(
                    "group {} needs a number, found {text:?}",
                    self.code
                ))
            })
    }

    /// The value of an integer group (codes 60–99, 160–179, 270–289, 370–389, 1060–1071).
    /// A few writers put a decimal point in them, which is read past.
    pub fn int(&self) -> Result<i64, ReadError> {
        let text = self.value.trim();
        text.parse::<i64>()
            .ok()
            .or_else(|| {
                text.parse::<f64>()
                    .ok()
                    .filter(|v| v.fract() == 0.0 && v.abs() < 1e15)
                    .map(|v| v as i64)
            })
            .ok_or_else(|| {
                self.error(format!(
                    "group {} needs a whole number, found {text:?}",
                    self.code
                ))
            })
    }

    /// A string value with the surrounding blanks some writers pad it with removed.
    pub fn text(&self) -> &str {
        self.value.trim()
    }
}

/// The first bytes of every DWG file: `AC` and a release number, such as `AC1032`, or the
/// `MC0.0` of the very first releases. No DXF file starts so.
fn is_dwg(bytes: &[u8]) -> bool {
    match bytes {
        [b'A', b'C', rest @ ..] if rest.len() >= 4 => {
            rest[..4].iter().all(|b| b.is_ascii_digit() || *b == b'.')
        }
        [b'M', b'C', b'0', b'.', ..] => true,
        _ => false,
    }
}

/// Every group of a DXF file, ASCII or binary, up to and including `0 EOF`, with its text
/// decoded; and a warning when the file's code page may have been decoded wrongly.
pub(crate) fn tags(bytes: &[u8]) -> Result<(Vec<Tag<'_>>, Option<String>), ReadError> {
    if is_dwg(bytes) {
        return Err(ReadError::Dwg);
    }
    let raws = match bytes.strip_prefix(binary::SENTINEL) {
        Some(rest) => binary::raw_tags(rest, binary::SENTINEL.len())?,
        None => ascii_tags(bytes)?,
    };
    let (encoding, warning) = encoding::choose(&raws);
    let tags = raws
        .into_iter()
        .map(|raw| Tag {
            code: raw.code,
            value: encoding.decode(raw.value),
            at: raw.at,
        })
        .collect();
    Ok((tags, warning))
}

/// The groups of an ASCII file, two lines each; anything after `0 EOF` is ignored, as is a
/// missing `EOF` and a byte order mark.
fn ascii_tags(bytes: &[u8]) -> Result<Vec<Raw<'_>>, ReadError> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    // The file's last line end ends a line rather than starting an empty one.
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let mut lines = bytes
        .split(|b| *b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .enumerate();
    let mut tags = Vec::new();
    while let Some((index, code)) = lines.next() {
        let trimmed = code.trim_ascii();
        if trimmed.is_empty() && lines.clone().all(|(_, l)| l.trim_ascii().is_empty()) {
            break;
        }
        let line = index + 1;
        let code: i32 = std::str::from_utf8(trimmed)
            .ok()
            .and_then(|t| t.parse().ok())
            .ok_or_else(|| {
                ReadError::at(
                    Place::Line(line),
                    format!(
                        "expected a DXF group code, found {:?}",
                        String::from_utf8_lossy(trimmed)
                    ),
                )
            })?;
        let Some((_, value)) = lines.next() else {
            return Err(ReadError::at(
                Place::Line(line),
                format!("group code {code} has no value"),
            ));
        };
        let eof = code == 0 && value.trim_ascii() == b"EOF";
        tags.push(Raw {
            code,
            value: Cow::Borrowed(value),
            at: Place::Line(line + 1),
        });
        if eof {
            break;
        }
    }
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Vec<Tag<'_>> {
        tags(text.as_bytes()).unwrap().0
    }

    #[test]
    fn codes_may_be_padded_and_values_keep_their_text() {
        let tags = read("  0\nSECTION\n  2\nENTITIES\n999\n a comment \n");
        let pairs: Vec<_> = tags
            .iter()
            .map(|t| (t.code, t.value.as_ref(), t.at))
            .collect();
        assert_eq!(
            pairs,
            [
                (0, "SECTION", Place::Line(2)),
                (2, "ENTITIES", Place::Line(4)),
                (999, " a comment ", Place::Line(6))
            ]
        );
        assert_eq!(tags[2].text(), "a comment");
    }

    #[test]
    fn reading_stops_at_eof() {
        assert_eq!(read("0\nEOF\nwhatever follows\n").len(), 1);
    }

    #[test]
    fn numbers_parse_with_exponents_signs_and_padding() {
        let tag = |value| Tag {
            code: 10,
            value: Cow::Borrowed(value),
            at: Place::Line(1),
        };
        assert_eq!(tag(" 1.5E+01").real(), Ok(15.0));
        assert_eq!(tag("-2.5e-1 ").real(), Ok(-0.25));
        assert_eq!(tag("+3").real(), Ok(3.0));
        assert_eq!(tag("     4").int(), Ok(4));
        assert_eq!(tag("1.0").int(), Ok(1));
        assert!(tag("nan").real().is_err());
        assert!(tag("1.5").int().is_err());
    }

    #[test]
    fn a_byte_order_mark_is_dropped() {
        assert_eq!(read("\u{feff}0\nEOF\n")[0].value, "EOF");
    }

    #[test]
    fn dwg_files_are_told_apart_from_dxf_by_their_first_bytes() {
        for dwg in [&b"AC1032\0\0\0"[..], b"AC1015", b"AC2.10\x00", b"MC0.0\x00"] {
            assert_eq!(tags(dwg).unwrap_err(), ReadError::Dwg, "{dwg:?}");
        }
        assert!(tags(b"0\nEOF\n").is_ok());
        assert!(tags(b"AC").is_err_and(|e| e != ReadError::Dwg));
    }
}
