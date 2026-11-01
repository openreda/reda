//! The library search query behind the Library panel's search box.

use reda_model::{ComponentType, Part, Uuid};

use crate::index::{Library, LibraryIndex};

/// A search over the Parts of the scanned Libraries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    /// Matched, ignoring case, as a substring of the Part's name or any of its field
    /// values (Value, Description, Keywords, MPN, the free-form fields…). Empty matches
    /// every Part.
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
        let needle = query.text.trim().to_lowercase();
        let mut hits: Vec<SearchHit<'_>> = self
            .parts()
            .filter(|(library, _)| query.library.is_none_or(|l| l == library.info.uuid))
            .filter(|(_, part)| {
                query
                    .component_type
                    .is_none_or(|t| t == part.component_type)
            })
            .filter(|(_, part)| needle.is_empty() || matches(part, &needle))
            .map(|(library, part)| SearchHit { library, part })
            .collect();
        hits.sort_by(|a, b| (&a.part.name, a.part.uuid).cmp(&(&b.part.name, b.part.uuid)));
        hits
    }
}

fn matches(part: &Part, needle: &str) -> bool {
    std::iter::once(part.name.as_str())
        .chain(part.fields.values())
        .any(|text| text.to_lowercase().contains(needle))
}
