//! The text encoding of a DXF file (DXF Reference, HEADER variables `$ACADVER` and
//! `$DWGCODEPAGE`). AutoCAD 2007 (`AC1021`) and later write UTF-8 whatever the code page
//! says; earlier releases write the code page, which for most drawings is `ANSI_1252`.
//!
//! Text that is valid UTF-8 is read as UTF-8, whatever the header says: Windows-1252 text
//! with accents is almost never valid UTF-8, and many writers put UTF-8 into older
//! releases. Anything else is read as Windows-1252. Another code page in a file older than
//! 2007 is read the same way, with a warning that its non-ASCII text may be wrong.

use std::borrow::Cow;

use crate::tags::Raw;

/// How a file's text is decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Encoding {
    Utf8,
    Windows1252,
}

/// The first release that always writes UTF-8: AutoCAD 2007, `AC1021`.
const UTF8_RELEASE: u32 = 1021;

/// The encoding for `raws`, and the warning when the code page is one neither encoding is.
pub(crate) fn choose(raws: &[Raw<'_>]) -> (Encoding, Option<String>) {
    let header = |name: &str, code: i32| {
        let at = raws
            .iter()
            .position(|r| r.code == 9 && r.value.trim_ascii() == name.as_bytes())?;
        raws[at + 1..]
            .iter()
            .take_while(|r| r.code != 9 && r.code != 0)
            .find(|r| r.code == code)
            .map(|r| String::from_utf8_lossy(r.value.trim_ascii()).into_owned())
    };
    let release = header("$ACADVER", 1)
        .and_then(|v| v.strip_prefix("AC").and_then(|n| n.parse::<u32>().ok()))
        .unwrap_or(0);
    let utf8 = raws.iter().all(|r| std::str::from_utf8(&r.value).is_ok());
    let encoding = if utf8 {
        Encoding::Utf8
    } else {
        Encoding::Windows1252
    };
    let warning = header("$DWGCODEPAGE", 3)
        .filter(|page| {
            release < UTF8_RELEASE
                && !["ANSI_1252", "UTF-8", "UTF8"]
                    .iter()
                    .any(|known| page.eq_ignore_ascii_case(known))
        })
        .map(|page| {
            let read_as = match encoding {
                Encoding::Utf8 => "UTF-8",
                Encoding::Windows1252 => "Windows-1252",
            };
            format!(
                "the file's code page is {page}, which DXF import does not decode; its text \
                 was read as {read_as}, so non-ASCII text may be wrong"
            )
        });
    (encoding, warning)
}

impl Encoding {
    pub(crate) fn decode(self, bytes: Cow<'_, [u8]>) -> Cow<'_, str> {
        match bytes {
            Cow::Borrowed(bytes) => match std::str::from_utf8(bytes) {
                Ok(text) if self == Encoding::Utf8 || bytes.is_ascii() => Cow::Borrowed(text),
                _ => Cow::Owned(self.decode_owned(bytes)),
            },
            Cow::Owned(bytes) => Cow::Owned(self.decode_owned(&bytes)),
        }
    }

    fn decode_owned(self, bytes: &[u8]) -> String {
        match self {
            Encoding::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            Encoding::Windows1252 => bytes.iter().map(|b| windows_1252(*b)).collect(),
        }
    }
}

/// The characters of Windows-1252 bytes 0x80 to 0x9F, from the Unicode Consortium's
/// mapping table (`CP1252.TXT`). The five bytes it leaves undefined keep their own code
/// point, as in ISO 8859-1; every other byte is its own code point too.
const HIGH: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
];

pub(crate) fn windows_1252(byte: u8) -> char {
    match byte {
        0x80..=0x9F => HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::Place;

    fn raw(code: i32, value: &[u8]) -> Raw<'_> {
        Raw {
            code,
            value: Cow::Borrowed(value),
            at: Place::Line(1),
        }
    }

    #[test]
    fn windows_1252_maps_its_high_bytes_to_their_characters() {
        let text = Encoding::Windows1252.decode(Cow::Borrowed(b"Caf\xe9 \x80 \x93q\x94 \xb0"));
        assert_eq!(text, "Café € “q” °");
    }

    #[test]
    fn valid_utf8_is_read_as_utf8_and_anything_else_as_windows_1252() {
        let utf8 = [raw(8, "Café".as_bytes())];
        assert_eq!(choose(&utf8), (Encoding::Utf8, None));
        let latin = [raw(8, b"Caf\xe9")];
        assert_eq!(choose(&latin), (Encoding::Windows1252, None));
    }

    #[test]
    fn another_code_page_before_2007_warns_and_after_does_not() {
        let file = |release: &'static [u8], page: &'static [u8]| {
            [
                raw(9, b"$ACADVER"),
                raw(1, release),
                raw(9, b"$DWGCODEPAGE"),
                raw(3, page),
            ]
        };
        let (_, warning) = choose(&file(b"AC1015", b"ANSI_1251"));
        assert!(warning.unwrap().contains("ANSI_1251"));
        assert_eq!(choose(&file(b"AC1015", b"ansi_1252")).1, None);
        assert_eq!(choose(&file(b"AC1032", b"ANSI_1251")).1, None);
    }
}
