//! What a scan found: the Libraries, their elements by UUID, and the problems.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reda_model::{Footprint, LibraryElement, LibraryInfo, Part, Symbol, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::scan::LibraryRoot;

/// How a Library came to be registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// The starter Library that ships with REDA. Read-only.
    Starter,
    /// Listed in the user's Preferences.
    User,
    /// Listed in the open Project's `.reda` file.
    Project,
}

/// One scanned Library and its elements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Library {
    pub info: LibraryInfo,
    /// The Library folder.
    pub root: PathBuf,
    pub origin: Origin,
    pub symbols: BTreeMap<Uuid, Symbol>,
    pub footprints: BTreeMap<Uuid, Footprint>,
    pub parts: BTreeMap<Uuid, Part>,
}

/// A file or folder a scan could not use, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ScanProblem {
    pub path: PathBuf,
    pub message: String,
}

pub(crate) enum Element {
    Symbol(Symbol),
    Footprint(Footprint),
    Part(Part),
}

impl Element {
    fn uuid(&self) -> Uuid {
        match self {
            Element::Symbol(s) => s.uuid,
            Element::Footprint(f) => f.uuid,
            Element::Part(p) => p.uuid,
        }
    }
}

/// Every scanned Library, in registration order, with lookups by UUID across them all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryIndex {
    libraries: Vec<Library>,
    /// Every element's Library (an index into `libraries`) and file.
    elements: BTreeMap<Uuid, (usize, PathBuf)>,
    problems: Vec<ScanProblem>,
}

impl LibraryIndex {
    /// The Libraries in the order they were registered.
    pub fn libraries(&self) -> impl Iterator<Item = &Library> {
        self.libraries.iter()
    }

    pub fn library(&self, uuid: Uuid) -> Option<&Library> {
        self.libraries.iter().find(|l| l.info.uuid == uuid)
    }

    /// What could not be scanned, in the order it was found.
    pub fn problems(&self) -> &[ScanProblem] {
        &self.problems
    }

    pub fn symbol(&self, uuid: Uuid) -> Option<&Symbol> {
        self.library_of(uuid)?.symbols.get(&uuid)
    }

    pub fn footprint(&self, uuid: Uuid) -> Option<&Footprint> {
        self.library_of(uuid)?.footprints.get(&uuid)
    }

    pub fn part(&self, uuid: Uuid) -> Option<&Part> {
        self.library_of(uuid)?.parts.get(&uuid)
    }

    /// The element with this UUID, whatever its kind, cloned.
    pub fn element(&self, uuid: Uuid) -> Option<LibraryElement> {
        let library = self.library_of(uuid)?;
        let symbol = || {
            library
                .symbols
                .get(&uuid)
                .cloned()
                .map(LibraryElement::Symbol)
        };
        let footprint = || {
            let footprint = library.footprints.get(&uuid).cloned();
            footprint.map(LibraryElement::Footprint)
        };
        let part = || library.parts.get(&uuid).cloned().map(LibraryElement::Part);
        symbol().or_else(footprint).or_else(part)
    }

    /// Every Part of every Library, Library by Library.
    pub fn parts(&self) -> impl Iterator<Item = (&Library, &Part)> {
        self.libraries
            .iter()
            .flat_map(|l| l.parts.values().map(move |p| (l, p)))
    }

    /// The Library that holds the element with this UUID.
    pub fn library_of(&self, element: Uuid) -> Option<&Library> {
        self.elements
            .get(&element)
            .map(|(library, _)| &self.libraries[*library])
    }

    /// The file the element with this UUID was read from.
    pub fn file_of(&self, element: Uuid) -> Option<&Path> {
        self.elements.get(&element).map(|(_, path)| path.as_path())
    }

    pub(crate) fn problem(&mut self, path: &Path, message: impl Into<String>) {
        self.problems.push(ScanProblem {
            path: path.to_owned(),
            message: message.into(),
        });
    }

    /// Adds a Library and returns its index, or reports it when its UUID is taken.
    pub(crate) fn add_library(&mut self, info: LibraryInfo, root: &LibraryRoot) -> Option<usize> {
        if let Some(first) = self.library(info.uuid) {
            let message = format!(
                "holds Library {} ({}), which is already registered from {}",
                info.name,
                info.uuid,
                first.root.display()
            );
            self.problem(&root.path, message);
            return None;
        }
        self.libraries.push(Library {
            info,
            root: root.path.clone(),
            origin: root.origin,
            symbols: BTreeMap::new(),
            footprints: BTreeMap::new(),
            parts: BTreeMap::new(),
        });
        Some(self.libraries.len() - 1)
    }

    /// Adds an element to a Library, or reports it when its UUID is taken.
    pub(crate) fn add_element(&mut self, library: usize, element: Element, path: PathBuf) {
        let uuid = element.uuid();
        if let Some((_, first)) = self.elements.get(&uuid) {
            let message = format!(
                "holds element {uuid}, which was already found in {}",
                first.display()
            );
            self.problem(&path, message);
            return;
        }
        self.elements.insert(uuid, (library, path));
        let library = &mut self.libraries[library];
        match element {
            Element::Symbol(s) => {
                library.symbols.insert(uuid, s);
            }
            Element::Footprint(f) => {
                library.footprints.insert(uuid, f);
            }
            Element::Part(p) => {
                library.parts.insert(uuid, p);
            }
        }
    }
}
