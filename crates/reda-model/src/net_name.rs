//! The syntax of Net names: what Net Label, Port and Power Port names may hold, how names
//! compare, and the overbar markup they (and Pin names) carry
//! ([name syntax](../../../docs/spec/v0.1/schematic.md#name-syntax)).
//!
//! - **Reserved:** `[`, `]`, `{`, `}`, `,`, `..` and control characters are refused, so
//!   later bus syntax needs no format change. Pin names aren't restricted.
//! - **Case:** names compare case-insensitively everywhere, so `abc` and `ABC` are one Net.
//! - **Overbars:** a backslash after a character negates that character (`R\E\S\E\T\`).
//!   With the Single '\' Negation Preference, a leading backslash negates the whole name
//!   (`\RESET`). The markup is part of the name; it only changes how the name is drawn.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What is wrong with a Net Label, Port or Power Port name, or `None` when it is fine.
/// Blank names are not judged here.
pub fn name_problem(name: &str) -> Option<String> {
    if let Some(c) = name.chars().find(|c| c.is_control()) {
        return Some(format!(
            "a Net name cannot hold the control character U+{:04X}",
            c as u32
        ));
    }
    if let Some(c) = name
        .chars()
        .find(|c| matches!(c, '[' | ']' | '{' | '}' | ','))
    {
        return Some(format!(
            "a Net name cannot hold {c:?}: `[ ] {{ }} ,` and `..` are reserved for buses"
        ));
    }
    if name.contains("..") {
        return Some(
            "a Net name cannot hold \"..\": `[ ] { } ,` and `..` are reserved for buses".to_owned(),
        );
    }
    None
}

/// `name` with its reserved characters replaced by `_`, for importers: `[ ] { } ,` and
/// control characters each become `_`, and every second dot of a `..` does too.
pub fn sanitize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let reserved = matches!(c, '[' | ']' | '{' | '}' | ',') || c.is_control();
        if reserved || (c == '.' && out.ends_with('.')) {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    out
}

/// The key names compare by: `abc` and `ABC` have the same key.
pub fn name_key(name: &str) -> String {
    name.to_lowercase()
}

/// Whether two names are the same Net name, ignoring case.
pub fn same_name(a: &str, b: &str) -> bool {
    a == b || name_key(a) == name_key(b)
}

/// One drawn character of a name and whether an overbar runs over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NameGlyph {
    pub ch: char,
    pub overbar: bool,
}

/// How a name is drawn: its characters without the markup, each with its overbar.
///
/// A backslash after a character negates that character and is not drawn. A leading
/// backslash negates the whole name when `single_negation` is on (the Preference, for Net
/// Label, Port and Power Port names; pass `false` for Pin names) and is simply dropped
/// when it is off.
pub fn overbars(name: &str, single_negation: bool) -> Vec<NameGlyph> {
    let (whole, body) = match name.strip_prefix('\\') {
        Some(rest) => (single_negation, rest),
        None => (false, name),
    };
    let mut out: Vec<NameGlyph> = Vec::with_capacity(body.len());
    // Whether the last glyph pushed may still be negated by a following backslash.
    let mut open = false;
    for ch in body.chars() {
        if ch == '\\' {
            if open && let Some(last) = out.last_mut() {
                last.overbar = true;
            }
            open = false;
        } else {
            out.push(NameGlyph { ch, overbar: whole });
            open = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(name: &str, single: bool) -> String {
        overbars(name, single)
            .iter()
            .map(|g| if g.overbar { '_' } else { ' ' })
            .collect()
    }

    fn text(name: &str, single: bool) -> String {
        overbars(name, single).iter().map(|g| g.ch).collect()
    }

    #[test]
    fn a_backslash_after_a_character_negates_it() {
        assert_eq!(text("R\\E\\S\\E\\T\\", false), "RESET");
        assert_eq!(marks("R\\E\\S\\E\\T\\", false), "_____");
        assert_eq!(text("CSN\\", false), "CSN");
        assert_eq!(marks("CSN\\", false), "  _");
        assert_eq!(
            marks("CSN\\", true),
            "  _",
            "the Preference changes nothing here"
        );
    }

    #[test]
    fn a_leading_backslash_negates_the_whole_name_only_under_the_preference() {
        assert_eq!(text("\\RESET", true), "RESET");
        assert_eq!(marks("\\RESET", true), "_____");
        assert_eq!(text("\\RESET", false), "RESET", "off, it isn't drawn");
        assert_eq!(marks("\\RESET", false), "     ");
    }

    #[test]
    fn a_plain_name_has_no_overbar() {
        assert_eq!(marks("GND", true), "   ");
    }

    #[test]
    fn reserved_characters_are_refused_and_sanitised() {
        for bad in ["D[0]", "A{B}", "A,B", "A..B", "A\tB", "A\u{7f}"] {
            assert!(name_problem(bad).is_some(), "{bad:?}");
        }
        for good in ["D0", "A.B", "R\\E\\", "\\RESET", "+3V3", "Net-(R1-1)"] {
            assert_eq!(name_problem(good), None, "{good:?}");
        }
        assert_eq!(sanitize("D[0],x..y{z}\n"), "D_0__x._y_z__");
        assert_eq!(name_problem(&sanitize("a...b")), None);
    }

    #[test]
    fn names_compare_ignoring_case() {
        assert!(same_name("abc", "ABC"));
        assert!(!same_name("abc", "abd"));
    }
}
