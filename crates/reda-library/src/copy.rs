//! Copying elements between Libraries: free names and the copy itself.

use reda_model::{LibraryElement, Uuid};

use crate::index::Library;
use crate::scan::ElementKind;

impl Library {
    /// Whether an element of this kind in this Library is called `name`.
    pub fn holds_name(&self, kind: ElementKind, name: &str) -> bool {
        match kind {
            ElementKind::Symbol => self.symbols.values().any(|s| s.name == name),
            ElementKind::Footprint => self.footprints.values().any(|f| f.name == name),
            ElementKind::Part => self.parts.values().any(|p| p.name == name),
        }
    }

    /// Whether this Library holds the element with this UUID.
    pub fn holds(&self, element: Uuid) -> bool {
        self.symbols.contains_key(&element)
            || self.footprints.contains_key(&element)
            || self.parts.contains_key(&element)
    }
}

/// `name` when it is free, otherwise the first of `name_1`, `name_2`… that is.
pub fn free_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_owned();
    }
    (1..)
        .map(|n| format!("{name}_{n}"))
        .find(|candidate| !taken(candidate))
        .expect("some suffix is free")
}

/// `element` as a new element: the same content under another UUID and name. A copied
/// Part keeps referencing its original Symbol and Footprints.
pub fn copy_element(element: &LibraryElement, uuid: Uuid, name: &str) -> LibraryElement {
    let mut copy = element.clone();
    match &mut copy {
        LibraryElement::Symbol(s) => (s.uuid, s.name) = (uuid, name.to_owned()),
        LibraryElement::Footprint(f) => (f.uuid, f.name) = (uuid, name.to_owned()),
        LibraryElement::Part(p) => (p.uuid, p.name) = (uuid, name.to_owned()),
    }
    copy
}
