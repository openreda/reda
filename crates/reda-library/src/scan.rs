//! Scanning Library folders: the folder layout and the [`LibraryFiles`] seam.

use std::path::{Path, PathBuf};

use reda_model::{Footprint, LibraryInfo, Part, Symbol};

use crate::index::{Element, LibraryIndex, Origin};

/// The extension of the file that marks a folder as a Library, without the dot.
pub const LIBRARY_EXTENSION: &str = "redalib";

/// The kinds of element a Library holds, each in its own sub-folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ElementKind {
    Symbol,
    Footprint,
    Part,
}

impl ElementKind {
    pub const ALL: [ElementKind; 3] = [
        ElementKind::Symbol,
        ElementKind::Footprint,
        ElementKind::Part,
    ];

    /// The sub-folder of the Library that holds this kind.
    pub const fn folder(self) -> &'static str {
        match self {
            ElementKind::Symbol => "symbols",
            ElementKind::Footprint => "footprints",
            ElementKind::Part => "parts",
        }
    }

    /// The file extension, without the dot.
    pub const fn extension(self) -> &'static str {
        match self {
            ElementKind::Symbol => "redasym",
            ElementKind::Footprint => "redafp",
            ElementKind::Part => "redapart",
        }
    }
}

/// Where the Library files come from. The engine implements it over the disk and the file
/// format; tests implement it in memory.
pub trait LibraryFiles {
    /// The names of the files directly inside `dir`. A missing folder has none.
    fn list(&mut self, dir: &Path) -> Result<Vec<String>, String>;
    fn read_library(&mut self, path: &Path) -> Result<LibraryInfo, String>;
    fn read_symbol(&mut self, path: &Path) -> Result<Symbol, String>;
    fn read_footprint(&mut self, path: &Path) -> Result<Footprint, String>;
    fn read_part(&mut self, path: &Path) -> Result<Part, String>;
}

/// One registered Library folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryRoot {
    pub path: PathBuf,
    pub origin: Origin,
}

impl LibraryRoot {
    pub fn new(path: impl Into<PathBuf>, origin: Origin) -> LibraryRoot {
        LibraryRoot {
            path: path.into(),
            origin,
        }
    }
}

/// Scans every root in order and indexes what it finds.
///
/// A folder with no `.redalib` (or several), an unreadable file, a second Library with
/// the same UUID and an element whose UUID was already found are each reported as a
/// problem and skipped; everything else still loads. Files are read in name order, so the
/// first of two clashing elements is predictable.
pub fn scan(roots: &[LibraryRoot], files: &mut dyn LibraryFiles) -> LibraryIndex {
    let mut index = LibraryIndex::default();
    for root in roots {
        scan_root(root, files, &mut index);
    }
    index
}

fn scan_root(root: &LibraryRoot, files: &mut dyn LibraryFiles, index: &mut LibraryIndex) {
    let names = match files.list(&root.path) {
        Ok(names) => names,
        Err(why) => return index.problem(&root.path, why),
    };
    let mut markers = with_extension(names, LIBRARY_EXTENSION);
    let marker = match markers.len() {
        1 => markers.remove(0),
        0 => {
            return index.problem(
                &root.path,
                format!("is not a Library: it holds no .{LIBRARY_EXTENSION} file"),
            );
        }
        _ => {
            return index.problem(
                &root.path,
                format!(
                    "holds several .{LIBRARY_EXTENSION} files: {}",
                    markers.join(", ")
                ),
            );
        }
    };
    let marker = root.path.join(marker);
    let info = match files.read_library(&marker) {
        Ok(info) => info,
        Err(why) => return index.problem(&marker, why),
    };
    let Some(library) = index.add_library(info, root) else {
        return;
    };

    for kind in ElementKind::ALL {
        let dir = root.path.join(kind.folder());
        let names = match files.list(&dir) {
            Ok(names) => names,
            Err(why) => {
                index.problem(&dir, why);
                continue;
            }
        };
        for name in with_extension(names, kind.extension()) {
            let path = dir.join(name);
            let element = match kind {
                ElementKind::Symbol => files.read_symbol(&path).map(Element::Symbol),
                ElementKind::Footprint => files.read_footprint(&path).map(Element::Footprint),
                ElementKind::Part => files.read_part(&path).map(Element::Part),
            };
            match element {
                Ok(element) => index.add_element(library, element, path),
                Err(why) => index.problem(&path, why),
            }
        }
    }
}

/// The names ending in `.<extension>`, sorted.
fn with_extension(names: Vec<String>, extension: &str) -> Vec<String> {
    let mut names: Vec<String> = names
        .into_iter()
        .filter(|n| Path::new(n).extension().is_some_and(|e| e == extension))
        .collect();
    names.sort();
    names
}
