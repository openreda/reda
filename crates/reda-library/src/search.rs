//! The library search query behind the Library panel's search box.

use reda_model::{ComponentType, Fields, Part, Uuid};

use crate::index::{Library, LibraryIndex};

/// A search over the Parts of the scanned Libraries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    /// Words separated by spaces, each of which must match, ignoring case. A plain word
    /// matches a substring of the Part's name or any of its field values (Value,
    /// Description, Keywords, MPN, the free-form fields…). A `field:value` word, such as
    /// `mpn:GRM188` or `lcsc:C14`, matches only that field (`name:` is the Part's name),
    /// and never a Part that lacks the field; `mpn:` alone matches any Part with an MPN.
    /// `*` stands for any run of characters. Empty matches every Part.
    pub text: String,
    /// Only Parts of this Library.
    pub library: Option<Uuid>,
    /// Only Parts of this Component Type.
    pub component_type: Option<ComponentType>,
}

/// One Part a search found, with the Library that holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchHit<'a> {
    pub library: &'a Library,
    pub part: &'a Part,
}

impl LibraryIndex {
    /// The Parts matching `query`, sorted by name and then UUID.
    pub fn search(&self, query: &SearchQuery) -> Vec<SearchHit<'_>> {
        let tokens: Vec<Token> = query.text.split_whitespace().map(Token::parse).collect();
        let mut hits: Vec<SearchHit<'_>> = self
            .parts()
            .filter(|(library, _)| query.library.is_none_or(|l| l == library.info.uuid))
            .filter(|(_, part)| {
                query
                    .component_type
                    .is_none_or(|t| t == part.component_type)
            })
            .filter(|(_, part)| tokens.iter().all(|t| t.matches(part)))
            .map(|(library, part)| SearchHit { library, part })
            .collect();
        hits.sort_by(|a, b| (&a.part.name, a.part.uuid).cmp(&(&b.part.name, b.part.uuid)));
        hits
    }
}

impl SearchQuery {
    /// Whether an element called `name` (a Symbol or Footprint, which have no fields)
    /// matches the search text: every plain word and every `name:` word must match the
    /// name, and a word for any other field never does.
    pub fn matches_name(&self, name: &str) -> bool {
        self.text
            .split_whitespace()
            .map(Token::parse)
            .all(|t| t.field.as_deref().is_none_or(|f| f == "name") && t.matches_text(name))
    }
}

/// One word of a search: a pattern, optionally restricted to one field.
struct Token {
    /// The field name, lowercased, of a `field:value` word.
    field: Option<String>,
    /// The lowercased pattern split at its `*`s.
    pieces: Vec<String>,
}

impl Token {
    fn parse(word: &str) -> Token {
        let word = word.to_lowercase();
        let (field, pattern) = match word.split_once(':') {
            Some((field, pattern)) if !field.is_empty() => (Some(field.to_owned()), pattern),
            _ => (None, word.as_str()),
        };
        Token {
            field,
            pieces: pattern.split('*').map(str::to_owned).collect(),
        }
    }

    fn matches(&self, part: &Part) -> bool {
        match &self.field {
            None => std::iter::once(part.name.as_str())
                .chain(part.fields.values())
                .any(|text| self.matches_text(text)),
            Some(field) => field_value(part, field).is_some_and(|v| self.matches_text(v)),
        }
    }

    /// Whether the pieces occur in `text` in order, ignoring case.
    fn matches_text(&self, text: &str) -> bool {
        let text = text.to_lowercase();
        let mut rest = text.as_str();
        for piece in &self.pieces {
            match rest.find(piece.as_str()) {
                Some(at) => rest = &rest[at + piece.len()..],
                None => return false,
            }
        }
        true
    }
}

/// The non-empty value of the field called `field` (lowercase), if the Part has one.
fn field_value<'a>(part: &'a Part, field: &str) -> Option<&'a str> {
    let value = if field == "name" {
        part.name.as_str()
    } else if Fields::BUILT_IN.contains(&field) {
        part.fields.get(field)
    } else {
        let custom = part.fields.custom.iter();
        custom
            .filter(|f| f.name.to_lowercase() == field)
            .map(|f| f.value.as_str())
            .next()
            .unwrap_or("")
    };
    (!value.is_empty()).then_some(value)
}
