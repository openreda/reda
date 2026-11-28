//! The tag-pair layer of a DXF file: every group is a code line followed by a value line
//! (DXF Reference, "DXF Format" > "Group Codes in Numerical Order"). The entity layer above
//! reads these pairs; nothing here knows what a code means beyond which ones hold numbers.

use std::fmt;

/// One group: its code, its value as written, and the 1-based line of the code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tag<'a> {
    pub code: i32,
    pub value: &'a str,
    pub line: usize,
}

/// Why a file could not be read, with the 1-based line where reading stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ReadError {}

impl<'a> Tag<'a> {
    fn error(&self, message: String) -> ReadError {
        ReadError {
            line: self.line + 1,
            message,
        }
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
    pub fn text(&self) -> &'a str {
        self.value.trim()
    }
}

/// The text of an ASCII DXF, without a byte order mark. Bytes that are not UTF-8 become
/// U+FFFD; codes and numbers are ASCII, so they always read.
pub(crate) fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Every group of `text` up to and including `0 EOF`. Anything after `EOF` is ignored, as
/// is a missing `EOF`.
pub(crate) fn tags(text: &str) -> Result<Vec<Tag<'_>>, ReadError> {
    // The file's last line end ends a line rather than starting an empty one.
    let text = text.strip_suffix('\n').unwrap_or(text);
    let mut lines = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .enumerate();
    let mut tags = Vec::new();
    while let Some((index, code)) = lines.next() {
        let trimmed = code.trim();
        if trimmed.is_empty() && lines.clone().all(|(_, l)| l.trim().is_empty()) {
            break;
        }
        let code: i32 = trimmed.parse().map_err(|_| ReadError {
            line: index + 1,
            message: format!("expected a DXF group code, found {trimmed:?}"),
        })?;
        let Some((_, value)) = lines.next() else {
            return Err(ReadError {
                line: index + 1,
                message: format!("group code {code} has no value"),
            });
        };
        let tag = Tag {
            code,
            value,
            line: index + 1,
        };
        tags.push(tag);
        if code == 0 && tag.text() == "EOF" {
            break;
        }
    }
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_may_be_padded_and_values_keep_their_text() {
        let tags = tags("  0\nSECTION\n  2\nENTITIES\n999\n a comment \n").unwrap();
        let pairs: Vec<_> = tags.iter().map(|t| (t.code, t.value, t.line)).collect();
        assert_eq!(
            pairs,
            [
                (0, "SECTION", 1),
                (2, "ENTITIES", 3),
                (999, " a comment ", 5)
            ]
        );
        assert_eq!(tags[2].text(), "a comment");
    }

    #[test]
    fn reading_stops_at_eof() {
        let tags = tags("0\nEOF\nwhatever follows\n").unwrap();
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn numbers_parse_with_exponents_signs_and_padding() {
        let tag = |value| Tag {
            code: 10,
            value,
            line: 1,
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
    fn a_byte_order_mark_is_dropped_and_other_bytes_still_read() {
        assert_eq!(decode(b"\xef\xbb\xbf0\nEOF\n"), "0\nEOF\n");
        assert_eq!(decode(b"8\nCaf\xe9\n"), "8\nCaf\u{fffd}\n");
    }
}
