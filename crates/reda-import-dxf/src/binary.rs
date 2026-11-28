//! Binary DXF (DXF Reference, "Binary DXF Files"): the sentinel, then the same groups as
//! an ASCII file, each a group code and a value in the type its code gives ("Group Code
//! Value Types"). Numbers are little-endian; strings end with a NUL byte.
//!
//! Group codes take two bytes from AutoCAD R13 on, and one byte before (255 and then two
//! bytes for a code above 254). The first group is always `0 SECTION`, so a zero byte after
//! the code's first zero byte marks the two-byte form.

use std::borrow::Cow;

use crate::tags::{Place, Raw, ReadError};

/// The 22 bytes every binary DXF file starts with.
pub(crate) const SENTINEL: &[u8] = b"AutoCAD Binary DXF\r\n\x1a\0";

/// How a group's value is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    Text,
    Real,
    Int16,
    Int32,
    Int64,
    /// One byte, 0 or 1.
    Bool,
    /// A length byte and that many bytes, written out as hex as an ASCII file has them.
    Chunk,
}

fn value_type(code: i32) -> Option<Value> {
    Some(match code {
        0..=9
        | 100..=105
        | 300..=309
        | 320..=369
        | 390..=399
        | 410..=419
        | 430..=439
        | 470..=481
        | 999
        | 1000..=1003
        | 1005..=1009 => Value::Text,
        10..=59 | 110..=149 | 210..=239 | 460..=469 | 1010..=1059 => Value::Real,
        60..=79 | 170..=179 | 270..=289 | 370..=389 | 400..=409 | 1060..=1070 => Value::Int16,
        90..=99 | 420..=429 | 440..=459 | 1071 => Value::Int32,
        160..=169 => Value::Int64,
        290..=299 => Value::Bool,
        310..=319 | 1004 => Value::Chunk,
        _ => return None,
    })
}

struct Reader<'a> {
    bytes: &'a [u8],
    /// Where `bytes` starts in the file, for the places errors give.
    offset: usize,
    wide: bool,
}

/// How many groups ahead a guess at a value's size is checked.
const LOOK_AHEAD: usize = 4;

impl<'a> Reader<'a> {
    fn place(&self, at: usize) -> Place {
        Place::Byte(self.offset + at)
    }

    fn take(&self, at: usize, n: usize) -> Result<&'a [u8], ReadError> {
        self.bytes
            .get(at..at + n)
            .ok_or_else(|| ReadError::at(self.place(at), "the file ends inside a group"))
    }

    fn array<const N: usize>(&self, at: usize) -> Result<[u8; N], ReadError> {
        Ok(self.take(at, N)?.try_into().expect("N bytes taken"))
    }

    /// The group code at `at`, and the bytes it takes; `None` at the end of the file.
    fn code_at(&self, at: usize) -> Option<(i32, usize)> {
        let byte = |i: usize| self.bytes.get(at + i).copied();
        let pair = |i: usize| Some(i32::from(u16::from_le_bytes([byte(i)?, byte(i + 1)?])));
        if self.wide {
            Some((pair(0)?, 2))
        } else if byte(0)? == 255 {
            Some((pair(1)?, 3))
        } else {
            Some((i32::from(byte(0)?), 1))
        }
    }

    /// The group at `at` and where the next one starts; `None` at the end of the file.
    /// `ahead` is how many groups further a guess at a value's size is checked.
    fn group(&self, at: usize, ahead: usize) -> Result<Option<(Raw<'a>, usize)>, ReadError> {
        let Some((code, len)) = self.code_at(at) else {
            if at < self.bytes.len() {
                return Err(ReadError::at(
                    self.place(at),
                    "the file ends inside a group code",
                ));
            }
            return Ok(None);
        };
        let Some(kind) = value_type(code) else {
            return Err(ReadError::at(
                self.place(at),
                format!("{code} is not a DXF group code"),
            ));
        };
        let at = at + len;
        let (value, next): (Cow<'a, [u8]>, usize) = match kind {
            Value::Text => {
                let rest = &self.bytes[at.min(self.bytes.len())..];
                let end = rest.iter().position(|b| *b == 0).ok_or_else(|| {
                    ReadError::at(self.place(at), "a string has no closing NUL byte")
                })?;
                (Cow::Borrowed(&rest[..end]), at + end + 1)
            }
            Value::Real => (number(f64::from_le_bytes(self.array(at)?)), at + 8),
            // The Reference gives 280–289 two bytes, but files are written with one too;
            // the groups after say which.
            Value::Int16 if (280..=289).contains(&code) && self.one_byte_fits(at, ahead) => {
                (integer(i64::from(self.array::<1>(at)?[0] as i8)), at + 1)
            }
            Value::Int16 => (
                integer(i64::from(i16::from_le_bytes(self.array(at)?))),
                at + 2,
            ),
            Value::Int32 => (
                integer(i64::from(i32::from_le_bytes(self.array(at)?))),
                at + 4,
            ),
            Value::Int64 => (integer(i64::from_le_bytes(self.array(at)?)), at + 8),
            Value::Bool => (integer(i64::from(self.array::<1>(at)?[0])), at + 1),
            Value::Chunk => {
                let len = usize::from(self.array::<1>(at)?[0]);
                let chunk = self.take(at + 1, len)?;
                (
                    Cow::Owned(chunk.iter().flat_map(|b| hex(*b)).collect()),
                    at + 1 + len,
                )
            }
        };
        Ok(Some((
            Raw {
                code,
                value,
                at: self.place(at),
            },
            next,
        )))
    }

    /// Whether `ahead` groups read from `at`, or the file ends first.
    fn reads(&self, mut at: usize, ahead: usize) -> bool {
        for _ in 0..ahead {
            match self.group(at, ahead - 1) {
                Ok(Some((_, next))) => at = next,
                Ok(None) => return true,
                Err(_) => return false,
            }
        }
        true
    }

    /// Whether the 280–289 value at `at` takes one byte: the groups after read that way,
    /// or don't read with two bytes either.
    fn one_byte_fits(&self, at: usize, ahead: usize) -> bool {
        ahead == 0 || self.reads(at + 1, ahead) || !self.reads(at + 2, ahead)
    }
}

fn number(value: f64) -> Cow<'static, [u8]> {
    // Shortest round-trip form, so the text reads back as the same number.
    Cow::Owned(format!("{value:e}").into_bytes())
}

fn integer(value: i64) -> Cow<'static, [u8]> {
    Cow::Owned(value.to_string().into_bytes())
}

fn hex(byte: u8) -> [u8; 2] {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    [
        DIGITS[usize::from(byte >> 4)],
        DIGITS[usize::from(byte & 15)],
    ]
}

/// The groups of a binary file after its sentinel, which ends `offset` bytes in, up to and
/// including `0 EOF`.
pub(crate) fn raw_tags(bytes: &[u8], offset: usize) -> Result<Vec<Raw<'_>>, ReadError> {
    let reader = Reader {
        bytes,
        offset,
        wide: bytes.starts_with(&[0, 0]),
    };
    let mut tags = Vec::new();
    let mut at = 0;
    while let Some((tag, next)) = reader.group(at, LOOK_AHEAD)? {
        let eof = tag.code == 0 && tag.value.as_ref() == b"EOF";
        tags.push(tag);
        at = next;
        if eof {
            break;
        }
    }
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `0 SECTION`, `2 ENTITIES`, a CIRCLE with layer, centre, radius and a 280 group,
    /// `0 ENDSEC`, `0 EOF`, with two-byte codes.
    fn wide_file(flag_bytes: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        let code = |b: &mut Vec<u8>, c: u16| b.extend(c.to_le_bytes());
        let text = |b: &mut Vec<u8>, c: u16, s: &str| {
            code(b, c);
            b.extend(s.as_bytes());
            b.push(0);
        };
        text(&mut b, 0, "SECTION");
        text(&mut b, 2, "ENTITIES");
        text(&mut b, 0, "CIRCLE");
        text(&mut b, 8, "Holes");
        for (c, v) in [(10, 3.0f64), (20, -4.5), (40, 1.25)] {
            code(&mut b, c);
            b.extend(v.to_le_bytes());
        }
        code(&mut b, 280);
        b.extend(flag_bytes);
        text(&mut b, 0, "ENDSEC");
        text(&mut b, 0, "EOF");
        b
    }

    fn values(raws: &[Raw<'_>]) -> Vec<(i32, String)> {
        raws.iter()
            .map(|r| (r.code, String::from_utf8(r.value.to_vec()).unwrap()))
            .collect()
    }

    #[test]
    fn two_byte_codes_read_strings_reals_and_integers() {
        let file = wide_file(&[1, 0]);
        let raws = raw_tags(&file, 22).unwrap();
        let want: Vec<(i32, String)> = [
            (0, "SECTION"),
            (2, "ENTITIES"),
            (0, "CIRCLE"),
            (8, "Holes"),
            (10, "3e0"),
            (20, "-4.5e0"),
            (40, "1.25e0"),
            (280, "1"),
            (0, "ENDSEC"),
            (0, "EOF"),
        ]
        .into_iter()
        .map(|(c, v)| (c, v.to_owned()))
        .collect();
        assert_eq!(values(&raws), want);
        assert_eq!(raws[0].at, Place::Byte(24));
    }

    #[test]
    fn a_one_byte_280_value_is_read_when_the_next_code_says_so() {
        let file = wide_file(&[1]);
        let raws = raw_tags(&file, 22).unwrap();
        assert_eq!(values(&raws)[7], (280, "1".to_owned()));
        assert_eq!(raws.last().unwrap().value.as_ref(), b"EOF");
    }

    #[test]
    fn one_byte_codes_read_with_255_for_larger_codes() {
        let mut file = vec![0];
        file.extend(b"SECTION\0");
        file.extend([255, 0xE8, 0x03]); // 1000
        file.extend(b"x\0");
        file.push(0);
        file.extend(b"EOF\0");
        let raws = raw_tags(&file, 22).unwrap();
        assert_eq!(
            values(&raws),
            [
                (0, "SECTION".to_owned()),
                (1000, "x".to_owned()),
                (0, "EOF".to_owned())
            ]
        );
    }

    #[test]
    fn a_cut_short_file_or_an_unknown_code_is_refused_with_its_byte() {
        let file = wide_file(&[1, 0]);
        let err = raw_tags(&file[..40], 22).unwrap_err();
        assert!(
            matches!(
                err,
                ReadError::Malformed {
                    at: Some(Place::Byte(_)),
                    ..
                }
            ),
            "{err}"
        );
        let err = raw_tags(&[0, 0, b'S', 0, 0x0f, 0x27], 22).unwrap_err();
        assert_eq!(err.to_string(), "byte 26: 9999 is not a DXF group code");
    }
}
