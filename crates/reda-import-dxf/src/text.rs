//! TEXT and MTEXT: where a text sits, how big and turned, and its characters with the DXF
//! codes taken out (DXF Reference, "TEXT", "MTEXT", and "Control Codes and Special
//! Characters"; MTEXT's inline format codes are those the MTEXT editor writes).

use reda_model::{TextHJustify, TextJustify, TextVJustify};

use crate::encoding::windows_1252;
use crate::shape::Xy;

/// A text as the board draws it: one style, `\n` between lines.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub text: String,
    /// The justification anchor.
    pub at: Xy,
    /// The capital height, in drawing units.
    pub height: f64,
    /// Counter-clockwise, in degrees.
    pub rotation: f64,
    pub justify: TextJustify,
}

impl Text {
    /// A text whose baseline runs along `along` and whose letters stand along `up`, as long
    /// as the text is high. Text that reads mirrored, from a flipped extrusion, an INSERT
    /// mirrored or TEXT written backward or upside down, is turned to read normally over
    /// the same place: upright where it can be, with its justification flipped to match.
    pub(crate) fn placed(text: String, at: Xy, along: Xy, up: Xy, justify: TextJustify) -> Text {
        let angle = |v: Xy| v.y.atan2(v.x).to_degrees();
        let along_length = along.x.hypot(along.y);
        let height = if along_length > 0.0 {
            (along.x * up.y - along.y * up.x).abs() / along_length
        } else {
            up.x.hypot(up.y)
        };
        let mirrored = along.x * up.y - along.y * up.x < 0.0;
        let (rotation, justify) = if !mirrored {
            (angle(along), justify)
        } else if up.y >= 0.0 {
            // Keep the letters standing up; the text now runs the other way from its anchor.
            let h = match justify.h {
                TextHJustify::Left => TextHJustify::Right,
                TextHJustify::Center => TextHJustify::Center,
                TextHJustify::Right => TextHJustify::Left,
            };
            (angle(up) - 90.0, TextJustify { h, ..justify })
        } else {
            // Keep the baseline's direction; the letters now hang the other way from it.
            let v = match justify.v {
                TextVJustify::Top => TextVJustify::Bottom,
                TextVJustify::Center => TextVJustify::Center,
                TextVJustify::Bottom => TextVJustify::Top,
            };
            (angle(along), TextJustify { v, ..justify })
        };
        Text {
            text,
            at,
            height,
            rotation: rotation.rem_euclid(360.0),
            justify,
        }
    }
}

/// A TEXT value's characters: `%%` codes and `\U+` escapes replaced.
pub(crate) fn text_value(raw: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        i = common(&chars, i, &mut out, ' ');
    }
    clean(out)
}

/// An MTEXT value's characters: its format codes stripped, `\P` paragraph breaks as line
/// breaks, stacked fractions as `a/b`, and the codes TEXT takes too.
pub(crate) fn mtext_value(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match (chars[i], chars.get(i + 1).copied()) {
            ('{' | '}', _) => i += 1,
            ('\\', Some(code)) => {
                i += 2;
                match code {
                    'P' | 'X' | 'N' => out.push('\n'),
                    '~' => out.push(' '),
                    '\\' | '{' | '}' => out.push(code),
                    'L' | 'l' | 'O' | 'o' | 'K' | 'k' => {}
                    // Font, height, width, oblique, tracking, colour, alignment and
                    // paragraph settings run to a semicolon.
                    'F' | 'f' | 'H' | 'W' | 'Q' | 'T' | 'C' | 'c' | 'A' | 'p' => {
                        i = after_semicolon(&chars, i);
                    }
                    'S' => {
                        let end = chars[i..]
                            .iter()
                            .position(|c| *c == ';')
                            .map_or(chars.len(), |n| i + n);
                        for c in &chars[i..end] {
                            out.push(if matches!(c, '^' | '#') { '/' } else { *c });
                        }
                        i = (end + 1).min(chars.len());
                    }
                    'U' => {
                        i -= 2;
                        i = common(&chars, i, &mut out, '\n');
                    }
                    // A double-byte character in some code page: not decodable here.
                    'M' if chars.get(i) == Some(&'+') => {
                        out.push('?');
                        i = (i + 6).min(chars.len());
                    }
                    other => out.push(other),
                }
            }
            _ => i = common(&chars, i, &mut out, '\n'),
        }
    }
    clean(out)
}

/// The index after the next `;` from `i`, or the end.
fn after_semicolon(chars: &[char], i: usize) -> usize {
    chars[i..]
        .iter()
        .position(|c| *c == ';')
        .map_or(chars.len(), |n| i + n + 1)
}

/// Reads the character or code at `i` into `out` and returns the index after it: `%%`
/// codes, `\U+XXXX`, and `^` control codes, where `^J` is `line_feed`.
fn common(chars: &[char], i: usize, out: &mut String, line_feed: char) -> usize {
    let at = |k: usize| chars.get(i + k).copied();
    match (chars[i], at(1), at(2)) {
        ('%', Some('%'), Some(code)) => {
            match code.to_ascii_lowercase() {
                'd' => out.push('°'),
                'p' => out.push('±'),
                'c' => out.push('⌀'),
                '%' => out.push('%'),
                // Underline, overline and strike-through switches.
                'u' | 'o' | 'k' => {}
                d if d.is_ascii_digit() => {
                    let digits: String = chars[i + 2..]
                        .iter()
                        .take(3)
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if let Some(c) = digits.parse::<u8>().ok().map(windows_1252) {
                        out.push(c);
                    }
                    return i + 2 + digits.len();
                }
                _ => {
                    out.push_str("%%");
                    return i + 2;
                }
            }
            i + 3
        }
        ('\\', Some('U' | 'u'), Some('+')) => {
            let hex: String = chars[i + 3..].iter().take(4).collect();
            match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                Some(c) if hex.len() == 4 => {
                    out.push(c);
                    i + 7
                }
                _ => {
                    out.push('\\');
                    i + 1
                }
            }
        }
        ('^', Some(' '), _) => {
            out.push('^');
            i + 2
        }
        ('^', Some('I'), _) => {
            out.push(' ');
            i + 2
        }
        ('^', Some('J'), _) => {
            out.push(line_feed);
            i + 2
        }
        ('^', Some('M'), _) => i + 2,
        (c, _, _) => {
            out.push(c);
            i + 1
        }
    }
}

/// Tabs as spaces, other control characters dropped, and blanks at the very end trimmed.
fn clean(text: String) -> String {
    let text: String = text
        .chars()
        .filter_map(|c| match c {
            '\t' => Some(' '),
            '\n' => Some('\n'),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();
    text.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_codes_become_their_characters() {
        assert_eq!(text_value("45%%d %%p0.1 %%c3 100%%%"), "45° ±0.1 ⌀3 100%");
        assert_eq!(text_value("%%uunder%%u %%065 \\U+00E9"), "under A é");
        assert_eq!(text_value("a^Ib ^ c"), "a b ^c");
    }

    #[test]
    fn mtext_codes_are_stripped_and_breaks_kept() {
        assert_eq!(
            mtext_value("{\\fArial|b1|i0|c0|p34;Hello}\\P\\H2.5x;World\\~again"),
            "Hello\nWorld again"
        );
        assert_eq!(
            mtext_value("\\LUnder\\l \\C1;red \\S1/2; \\S+0.1^-0.1;"),
            "Under red 1/2 +0.1/-0.1"
        );
        assert_eq!(mtext_value("a\\\\b \\{c\\} \\pxqc;d"), "a\\b {c} d");
        assert_eq!(mtext_value("1\\P2^J3 %%d"), "1\n2\n3 °");
    }

    fn justify(h: TextHJustify, v: TextVJustify) -> TextJustify {
        TextJustify { h, v }
    }

    #[test]
    fn plain_text_keeps_its_rotation_height_and_justification() {
        let t = Text::placed(
            "A".into(),
            Xy::new(1.0, 2.0),
            Xy::new(0.0, 1.0),
            Xy::new(-2.5, 0.0),
            justify(TextHJustify::Left, TextVJustify::Bottom),
        );
        assert_eq!((t.rotation, t.height), (90.0, 2.5));
        assert_eq!(t.justify, justify(TextHJustify::Left, TextVJustify::Bottom));
    }

    #[test]
    fn mirrored_text_reads_normally_over_the_same_place() {
        // Mirrored in X: the baseline runs left, the letters still stand up.
        let t = Text::placed(
            "A".into(),
            Xy::new(0.0, 0.0),
            Xy::new(-1.0, 0.0),
            Xy::new(0.0, 2.0),
            justify(TextHJustify::Left, TextVJustify::Bottom),
        );
        assert_eq!((t.rotation, t.height), (0.0, 2.0));
        assert_eq!(
            t.justify,
            justify(TextHJustify::Right, TextVJustify::Bottom)
        );
        // Mirrored in Y: upside down; the baseline still runs right.
        let t = Text::placed(
            "A".into(),
            Xy::new(0.0, 0.0),
            Xy::new(1.0, 0.0),
            Xy::new(0.0, -2.0),
            justify(TextHJustify::Left, TextVJustify::Bottom),
        );
        assert_eq!((t.rotation, t.height), (0.0, 2.0));
        assert_eq!(t.justify, justify(TextHJustify::Left, TextVJustify::Top));
    }
}
