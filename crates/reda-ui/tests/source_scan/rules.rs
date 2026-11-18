//! What the scan bans: the patterns, and how a numeric literal is told from a token.

use crate::lex::{Source, is_ident, lex};

/// A stock egui widget that has a REDA widget to use in its place.
pub struct StockWidget {
    /// The text that marks a raw use, for example `ui.button(` or `Button::new`.
    pub pattern: &'static str,
    /// What to call instead, shown in the failure.
    pub instead: &'static str,
}

/// One banned use.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    /// The line the banned pattern starts on, from 1.
    pub line: usize,
    /// The banned pattern, as the failure names it.
    pub pattern: String,
}

/// What scanning one file found.
pub struct Scan {
    /// The banned uses that no marker exempts, and every marker that gives no reason.
    pub findings: Vec<Finding>,
    /// How many `// theme-exempt: <reason>` markers the file carries.
    pub markers: usize,
}

/// The marker that exempts the line after it.
pub const MARKER: &str = "// theme-exempt";

/// Banned wherever it appears. A pattern that starts with a letter must start a word.
const ANYWHERE: &[(&str, &str)] = &[
    // Colours.
    ("Color32::from_", "`Color32::from_*`"),
    ("Rgba::from_", "`Rgba::from_*`"),
    ("Hsva::", "`Hsva::*`"),
    ("hex_color!", "`hex_color!`"),
    ("gamma_multiply", "`gamma_multiply`"),
    ("linear_multiply", "`linear_multiply`"),
    // egui style access.
    (".visuals()", "`.visuals()`"),
    (".visuals_mut()", "`.visuals_mut()`"),
    (".spacing()", "`.spacing()`"),
    (".spacing_mut()", "`.spacing_mut()`"),
    (".style()", "`.style()`"),
    (".style_mut()", "`.style_mut()`"),
    (".style_of(", "`.style_of(..)`"),
    ("all_styles_mut", "`all_styles_mut`"),
    ("set_style", "`set_style*`"),
    ("set_visuals", "`set_visuals*`"),
    // A text style is a whole token: nothing builds a font from a size.
    ("FontId::", "`FontId::*`"),
    ("FontId {", "`FontId { .. }`"),
];

/// The one named colour constant that is allowed: it is the zero of colours.
const ZERO_COLOUR: &str = "TRANSPARENT";

/// Calls that may not take a numeric literal: in any argument, or in the one argument
/// (counted from 0) that is a size (a stroke's width, a painted rectangle's radius).
const LITERAL_ARGUMENT: &[(&str, Option<usize>, &str)] = &[
    ("add_space(", None, "`add_space(<n>)`"),
    ("Stroke::new(", Some(0), "`Stroke::new(<n>, ..)`"),
    ("PathStroke::new(", Some(0), "`Stroke::new(<n>, ..)`"),
    (".size(", None, "`.size(<n>)`"),
    (".corner_radius(", None, "`CornerRadius` from a literal"),
    (".rect_filled(", Some(1), "`CornerRadius` from a literal"),
    (".rect_stroke(", Some(1), "`CornerRadius` from a literal"),
    (".rect(", Some(1), "`CornerRadius` from a literal"),
    (".inner_margin(", None, "`Margin` from a literal"),
    (".outer_margin(", None, "`Margin` from a literal"),
    (".spacing(", None, "literal `vec2` in `.spacing(..)`"),
    (".min_size(", None, "literal `vec2` in `.min_size(..)`"),
    (
        "set_min_size(",
        None,
        "literal `vec2` in `set_min_size(..)`",
    ),
];

/// Types that may not be built from a numeric literal, by a constructor
/// (`Margin::same(8)`) or field by field (`Shadow { blur: 8, .. }`).
const LITERAL_VALUE: &[(&str, &str)] = &[
    ("CornerRadius", "`CornerRadius` from a literal"),
    ("Margin", "`Margin` from a literal"),
    ("Shadow", "`Shadow` from a literal"),
];

/// Scans one file's text.
pub fn scan(text: &str, stock_widgets: &[StockWidget]) -> Scan {
    let source = lex(text);
    let mut found = Vec::new();
    banned_anywhere(&source, &mut found);
    named_colours(&source, &mut found);
    hex_literals(&source, &mut found);
    literal_sizes(&source, &mut found);
    for widget in stock_widgets {
        for at in occurrences(&source.code, widget.pattern) {
            let pattern = format!(
                "raw stock widget `{}` (use {})",
                widget.pattern, widget.instead
            );
            found.push(Finding {
                line: line_of(&source.code, at),
                pattern,
            });
        }
    }

    let mut exempt = Vec::new();
    let mut markers = 0;
    for (index, line) in text.lines().enumerate() {
        let Some(rest) = line.trim_start().strip_prefix(MARKER) else {
            continue;
        };
        match rest.strip_prefix(':').map(str::trim) {
            Some(reason) if !reason.is_empty() => {
                markers += 1;
                exempt.push(index + 2);
            }
            _ => found.push(Finding {
                line: index + 1,
                pattern: format!("`{MARKER}` with no reason"),
            }),
        }
    }
    found.retain(|finding| !exempt.contains(&finding.line));
    found.sort();
    found.dedup();
    Scan {
        findings: found,
        markers,
    }
}

fn banned_anywhere(source: &Source, found: &mut Vec<Finding>) {
    for (needle, pattern) in ANYWHERE {
        for at in occurrences(&source.code, needle) {
            found.push(Finding {
                line: line_of(&source.code, at),
                pattern: (*pattern).to_owned(),
            });
        }
    }
}

/// `Color32::RED` and the like: `Color32::` and then a constant's capital letter.
fn named_colours(source: &Source, found: &mut Vec<Finding>) {
    for at in occurrences(&source.code, "Color32::") {
        let name: String = source.code[at + "Color32::".len()..]
            .chars()
            .take_while(|c| is_ident(*c as u8))
            .collect();
        if name.starts_with(|c: char| c.is_ascii_uppercase()) && name != ZERO_COLOUR {
            found.push(Finding {
                line: line_of(&source.code, at),
                pattern: format!("named colour `Color32::{name}`"),
            });
        }
    }
}

/// A colour written in hex: `0xRRGGBB` or `0xRRGGBBAA` in code, `#RGB`, `#RRGGBB` or
/// `#RRGGBBAA` in a string. Other hex numbers (a mask, an id, `#294`) are not colours.
fn hex_literals(source: &Source, found: &mut Vec<Finding>) {
    for at in occurrences(&source.code, "0x") {
        let digits = source.code[at + 2..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit() || *c == '_')
            .filter(|c| *c != '_')
            .count();
        if digits == 6 || digits == 8 {
            found.push(Finding {
                line: line_of(&source.code, at),
                pattern: "hex colour literal".to_owned(),
            });
        }
    }
    for (line, string) in &source.strings {
        let bytes = string.as_bytes();
        for (at, _) in string.match_indices('#') {
            let digits = bytes[at + 1..]
                .iter()
                .take_while(|b| b.is_ascii_hexdigit())
                .count();
            let ends = bytes.get(at + 1 + digits).is_none_or(|b| !is_ident(*b));
            let starts = at == 0 || !is_ident(bytes[at - 1]);
            // Three digits with no letter is a number (`#294`), not a colour.
            let number = digits == 3 && bytes[at + 1..at + 4].iter().all(u8::is_ascii_digit);
            if starts && ends && matches!(digits, 3 | 6 | 8) && !number {
                let line = line + string[..at].matches('\n').count();
                found.push(Finding {
                    line,
                    pattern: "hex colour literal".to_owned(),
                });
            }
        }
    }
}

fn literal_sizes(source: &Source, found: &mut Vec<Finding>) {
    let code = &source.code;
    for (needle, only, pattern) in LITERAL_ARGUMENT {
        for at in occurrences(code, needle) {
            let inside = group(code, at + needle.len() - 1);
            let literal = match only {
                Some(index) => parts(inside)
                    .get(*index)
                    .is_some_and(|part| has_literal(part)),
                None => has_literal(inside),
            };
            if literal {
                found.push(Finding {
                    line: line_of(code, at),
                    pattern: (*pattern).to_owned(),
                });
            }
        }
    }
    for (name, pattern) in LITERAL_VALUE {
        for at in occurrences(code, name) {
            let after = at + name.len();
            let rest = &code[after..];
            // `Name::constructor(` or `Name {`, and nothing else (a type in a signature).
            let open = if let Some(path) = rest.strip_prefix("::") {
                let ident = path.bytes().take_while(|b| is_ident(*b)).count();
                (path[ident..].starts_with('(')).then_some(after + 2 + ident)
            } else {
                // `-> Margin {` and `impl From<X> for Margin {` open a body, not a value.
                let before = code[..at].trim_end();
                let value = !["->", " for", "impl"]
                    .iter()
                    .any(|word| before.ends_with(word));
                (value && rest.starts_with(" {")).then_some(after + 1)
            };
            if open.is_some_and(|open| has_literal(group(code, open))) {
                found.push(Finding {
                    line: line_of(code, at),
                    pattern: (*pattern).to_owned(),
                });
            }
        }
    }
}

/// Where `needle` occurs in `code`. A needle that starts with a letter must start a word,
/// so `set_style` does not match `reset_style`.
fn occurrences<'a>(code: &'a str, needle: &'a str) -> impl Iterator<Item = usize> + 'a {
    let word = needle.as_bytes().first().is_some_and(|b| is_ident(*b));
    code.match_indices(needle)
        .map(|(at, _)| at)
        .filter(move |&at| !word || at == 0 || !is_ident(code.as_bytes()[at - 1]))
}

fn line_of(code: &str, at: usize) -> usize {
    code[..at].matches('\n').count() + 1
}

/// The text inside the bracket that opens at `open`, up to its matching close.
fn group(code: &str, open: usize) -> &str {
    let mut depth = 0;
    for (offset, b) in code.bytes().enumerate().skip(open) {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &code[open + 1..offset];
                }
            }
            _ => {}
        }
    }
    &code[open + 1..]
}

/// Splits on the commas that are not inside a nested bracket.
fn parts(inside: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (offset, b) in inside.bytes().enumerate() {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(&inside[start..offset]);
                start = offset + 1;
            }
            _ => {}
        }
    }
    parts.push(&inside[start..]);
    parts
}

/// Whether a value is written with a number of its own: a part that is a literal other
/// than zero and names nothing. `8.0`, `-2`, `4.0 * 2.0` and `vec2(8.0, t.space.gap)`
/// are; `0.0`, `t.space.gap` and `t.space.gap * 2.0` are not. Arithmetic on a token is
/// allowed, which is an accepted loophole.
fn has_literal(inside: &str) -> bool {
    parts(inside).into_iter().any(|part| {
        // A field's name is not part of its value.
        let value = match part.split_once(':') {
            Some((field, value))
                if !value.starts_with(':') && field.trim().bytes().all(is_ident) =>
            {
                value
            }
            _ => part,
        };
        let value = value.trim();
        // Look inside `vec2(..)`, `Vec2::splat(..)`, an array or a tuple.
        let constructor = [
            "egui::vec2(",
            "vec2(",
            "egui::Vec2::new(",
            "Vec2::new(",
            "egui::Vec2::splat(",
            "Vec2::splat(",
            "[",
            "(",
        ];
        if let Some(open) = constructor.iter().find(|open| value.starts_with(**open)) {
            let open = open.len() - 1;
            let nested = group(value, open);
            if open + nested.len() + 2 == value.len() {
                return has_literal(nested);
            }
        }
        is_nonzero_literal(value)
    })
}

/// A number, or sums and products of numbers, that is not zero and names nothing.
fn is_nonzero_literal(value: &str) -> bool {
    let mut nonzero = false;
    let mut rest = value;
    while let Some(c) = rest.chars().next() {
        if c.is_ascii_digit() {
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
                .unwrap_or(rest.len());
            let number = &rest[..end];
            nonzero |= number
                .chars()
                .take_while(|c| !c.is_ascii_alphabetic())
                .any(|c| matches!(c, '1'..='9'));
            rest = &rest[end..];
        } else if c.is_whitespace() || "+-*/()".contains(c) {
            rest = &rest[1..];
        } else {
            return false;
        }
    }
    nonzero
}
