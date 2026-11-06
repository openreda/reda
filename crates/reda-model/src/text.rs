//! Stored text and the one resolver that displays it.
//!
//! Text stores the raw string. `${name}` is a variable token, and `$${` is a literal `${`.
//! Stored design text holds no variables yet, so an unescaped `${` in it is refused, by
//! Commands and on load ([`check`]); it shows through [`display`]. Output name templates
//! fill their tokens with [`resolve`]. Importers [`escape`] what they bring in.
//! ([architecture](../../../docs/spec/v0.1/architecture.md#file-format))

use std::borrow::Cow;
use std::fmt;

use crate::{Fields, Footprint, Object, Part, Shape, Symbol};

/// What is wrong with a piece of text. `at` is the position of the `$`, counting
/// characters from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    /// An unescaped `${` in text that takes no variables.
    Reserved { at: usize },
    /// A `${` with no closing `}`.
    Unterminated { at: usize },
    /// A `${name}` token naming no variable.
    Unknown { name: String, at: usize },
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextError::Reserved { at } => write!(
                f,
                "\"${{\" at character {at} is reserved for variables; write \"$${{\" for a \
                 literal \"${{\""
            ),
            TextError::Unterminated { at } => {
                write!(f, "\"${{\" at character {at} has no closing \"}}\"")
            }
            TextError::Unknown { name, at } => {
                write!(f, "unknown variable \"${{{name}}}\" at character {at}")
            }
        }
    }
}

impl std::error::Error for TextError {}

/// One piece of a raw string.
enum Piece<'a> {
    Literal(&'a str),
    /// A `${name}` token; `at` is the character position of its `$`, from 1.
    Token {
        name: &'a str,
        at: usize,
    },
    /// A `${` with no closing `}`, and the rest of the string after it.
    Unterminated {
        at: usize,
        rest: &'a str,
    },
}

/// Splits `raw` into literals and tokens. `$${` becomes the literal `${`.
fn pieces(raw: &str) -> Vec<Piece<'_>> {
    let mut out = vec![];
    let mut start = 0; // start of the pending literal, in bytes
    let mut i = 0;
    let bytes = raw.as_bytes();
    let at = |byte: usize| raw[..byte].chars().count() + 1;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"$${") {
            out.push(Piece::Literal(&raw[start..i]));
            out.push(Piece::Literal("${"));
            i += 3;
            start = i;
        } else if bytes[i..].starts_with(b"${") {
            out.push(Piece::Literal(&raw[start..i]));
            match raw[i + 2..].find('}') {
                Some(len) => {
                    out.push(Piece::Token {
                        name: &raw[i + 2..i + 2 + len],
                        at: at(i),
                    });
                    i += 2 + len + 1;
                    start = i;
                }
                None => {
                    out.push(Piece::Unterminated {
                        at: at(i),
                        rest: &raw[i..],
                    });
                    return out;
                }
            }
        } else {
            i += 1;
        }
    }
    out.push(Piece::Literal(&raw[start..]));
    out
}

/// `raw` with every `${name}` token replaced by `lookup(name)`, and `$${` shown as `${`.
/// A token `lookup` does not know, or one never closed, is an error.
pub fn resolve(raw: &str, lookup: impl Fn(&str) -> Option<String>) -> Result<String, TextError> {
    let mut out = String::with_capacity(raw.len());
    for piece in pieces(raw) {
        match piece {
            Piece::Literal(s) => out.push_str(s),
            Piece::Token { name, at } => match lookup(name) {
                Some(value) => out.push_str(&value),
                None => {
                    return Err(TextError::Unknown {
                        name: name.to_owned(),
                        at,
                    });
                }
            },
            Piece::Unterminated { at, .. } => return Err(TextError::Unterminated { at }),
        }
    }
    Ok(out)
}

/// Stored text as it shows: `$${` becomes `${`. Stored text takes no variables, so a
/// `${` that slipped past [`check`] shows as written.
pub fn display(raw: &str) -> Cow<'_, str> {
    if !raw.contains("${") {
        return Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len());
    for piece in pieces(raw) {
        match piece {
            Piece::Literal(s) => out.push_str(s),
            Piece::Token { name, .. } => {
                out.push_str("${");
                out.push_str(name);
                out.push('}');
            }
            Piece::Unterminated { rest, .. } => out.push_str(rest),
        }
    }
    Cow::Owned(out)
}

/// Refuses stored text with an unescaped `${`, which is reserved for variables.
pub fn check(raw: &str) -> Result<(), TextError> {
    for piece in pieces(raw) {
        match piece {
            Piece::Literal(_) => {}
            Piece::Token { at, .. } | Piece::Unterminated { at, .. } => {
                return Err(TextError::Reserved { at });
            }
        }
    }
    Ok(())
}

/// Text from outside REDA made into stored text that displays as it was: every `${`
/// becomes `$${`.
pub fn escape(s: &str) -> Cow<'_, str> {
    match s.contains("${") {
        true => Cow::Owned(s.replace("${", "$${")),
        false => Cow::Borrowed(s),
    }
}

/// Every field value, named `field <name>`.
pub fn fields_texts<'a>(fields: &'a Fields, out: &mut Vec<(String, &'a str)>) {
    for (name, value) in Fields::BUILT_IN.iter().map(|n| (*n, fields.get(n))) {
        if !value.is_empty() {
            out.push((format!("field {name}"), value));
        }
    }
    for field in &fields.custom {
        out.push((format!("field {}", field.name), field.value.as_str()));
    }
}

fn shape_text<'a>(owner: &str, shape: &'a Shape, out: &mut Vec<(String, &'a str)>) {
    if let Shape::Text { text, .. } = shape {
        out.push((format!("{owner} text"), text.as_str()));
    }
}

/// Every stored text of a Symbol: the text in its body drawing.
pub fn symbol_texts<'a>(symbol: &'a Symbol, out: &mut Vec<(String, &'a str)>) {
    for graphic in &symbol.graphics {
        shape_text(&format!("graphic {}", graphic.uuid), &graphic.shape, out);
    }
}

/// Every stored text of a Footprint: the text in its drawing.
pub fn footprint_texts<'a>(footprint: &'a Footprint, out: &mut Vec<(String, &'a str)>) {
    for graphic in &footprint.graphics {
        shape_text(&format!("graphic {}", graphic.uuid), &graphic.shape, out);
    }
}

/// Every stored text of a Part: its field values.
pub fn part_texts<'a>(part: &'a Part, out: &mut Vec<(String, &'a str)>) {
    fields_texts(&part.fields, out);
}

/// Every stored text of `object`, each named by where it is in the object: free text,
/// text in drawings, field values and the Project properties.
pub fn object_texts(object: &Object) -> Vec<(String, &str)> {
    let mut out = vec![];
    match object {
        Object::Project(p) => {
            let props = &p.properties;
            for (name, value) in [
                ("title", &props.title),
                ("revision", &props.revision),
                ("date", &props.date),
                ("author", &props.author),
                ("company", &props.company),
                ("copper_weight", &props.copper_weight),
            ] {
                out.push((name.to_owned(), value.as_str()));
            }
        }
        Object::Symbol(s) => symbol_texts(s, &mut out),
        Object::Footprint(f) => footprint_texts(f, &mut out),
        Object::Part(p) => part_texts(p, &mut out),
        Object::PartEntry(e) => {
            part_texts(&e.part, &mut out);
            symbol_texts(&e.symbol, &mut out);
        }
        Object::FootprintCopy(c) => footprint_texts(&c.footprint, &mut out),
        Object::Unit(u) => fields_texts(&u.fields, &mut out),
        Object::SheetFrame(f) => {
            out.push(("drawing_number".to_owned(), f.drawing_number.as_str()));
            out.push(("revision".to_owned(), f.revision.as_str()));
        }
        Object::SheetText(t) => out.push(("text".to_owned(), t.text.as_str())),
        Object::PcbText(t) => out.push(("text".to_owned(), t.text.as_str())),
        Object::FootprintInstance(f) => fields_texts(&f.fields, &mut out),
        _ => {}
    }
    out
}

/// Refuses `texts` of the object `what` if any has an unescaped `${`, naming the text.
pub fn check_texts(what: &str, texts: &[(String, &str)]) -> Result<(), String> {
    for (name, raw) in texts {
        check(raw).map_err(|e| format!("{what}, {name}: {e}"))?;
    }
    Ok(())
}

/// Refuses `object` if any of its stored texts has an unescaped `${`.
pub fn check_object(object: &Object) -> Result<(), String> {
    let texts = object_texts(object);
    if texts.is_empty() {
        return Ok(());
    }
    let kind = serde_json::to_value(object.object_type())
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    check_texts(&format!("{kind} {}", object.uuid()), &texts)
}
