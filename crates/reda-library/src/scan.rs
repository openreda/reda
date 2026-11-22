//! Scanning Library folders: the folder layout and the [`LibraryFiles`] seam.

use std::path::{Path, PathBuf};

use reda_model::{BoardTemplate, Footprint, LibraryElement, LibraryInfo, Part, Symbol};
use schemars::JsonSchema;

use crate::index::{Element, LibraryIndex, Origin};

/// The extension of the file that marks a folder as a Library, without the dot.
pub const LIBRARY_EXTENSION: &str = "redalib";

/// The kinds of element a Library holds, each in its own sub-folder.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    Symbol,
    Footprint,
    Part,
    /// A Board Template (ADR-0016): copied, duplicated and deleted like the others, but
    /// with no editor and never a [`LibraryElement`].
    BoardTemplate,
}

impl ElementKind {
    pub const ALL: [ElementKind; 4] = [
        ElementKind::Symbol,
        ElementKind::Footprint,
        ElementKind::Part,
        ElementKind::BoardTemplate,
    ];

    /// The kinds an element editor opens: every kind but Board Templates.
    pub const EDITABLE: [ElementKind; 3] = [
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
            ElementKind::BoardTemplate => crate::board_template::BOARD_TEMPLATE_FOLDER,
        }
    }

    /// The kind's name in messages: "Symbol", "Board Template".
    pub const fn name(self) -> &'static str {
        match self {
            ElementKind::Symbol => "Symbol",
            ElementKind::Footprint => "Footprint",
            ElementKind::Part => "Part",
            ElementKind::BoardTemplate => "Board Template",
        }
    }

    /// The kind of an element.
    pub const fn of(element: &LibraryElement) -> ElementKind {
        match element {
            LibraryElement::Symbol(_) => ElementKind::Symbol,
            LibraryElement::Footprint(_) => ElementKind::Footprint,
            LibraryElement::Part(_) => ElementKind::Part,
        }
    }

    /// The file extension, without the dot.
    pub const fn extension(self) -> &'static str {
        match self {
            ElementKind::Symbol => "redasym",
            ElementKind::Footprint => "redafp",
            ElementKind::Part => "redapart",
            ElementKind::BoardTemplate => crate::board_template::BOARD_TEMPLATE_EXTENSION,
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
    /// Reads a `.redaboard` file's shape; the scan runs the template's load checks.
    fn read_board_template(&mut self, path: &Path) -> Result<BoardTemplate, String> {
        let _ = path;
        Err("this Library source cannot read Board Templates".into())
    }
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

/// Scans every root in resolution order and indexes what it finds: Project Libraries,
/// then user Libraries, then the Starter Library, each group in the order given. When two
/// Libraries hold the same UUID, the first in that order wins, so a Project Library can
/// deliberately hold its own copy of an element.
///
/// A folder with no `.redalib` (or several), an unreadable file, a second Library with
/// the same UUID and an element whose UUID was already found are each reported as a
/// problem and skipped; everything else still loads. Files are read in name order, so the
/// first of two clashing elements is predictable.
pub fn scan(roots: &[LibraryRoot], files: &mut dyn LibraryFiles) -> LibraryIndex {
    let mut index = LibraryIndex::default();
    let mut roots: Vec<&LibraryRoot> = roots.iter().collect();
    roots.sort_by_key(|root| root.origin.resolution_rank());
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

    for kind in ElementKind::EDITABLE {
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
                ElementKind::BoardTemplate => unreachable!("scanned by scan_board_templates"),
            };
            match element {
                Ok(element) => index.add_element(library, element, path),
                Err(why) => index.problem(&path, why),
            }
        }
    }
    scan_board_templates(root, library, files, index);
}

/// The Board Templates in `board-templates/`: each must read, pass its load checks and
/// have a name no other template of the Library has, or it is a problem and is left out.
fn scan_board_templates(
    root: &LibraryRoot,
    library: usize,
    files: &mut dyn LibraryFiles,
    index: &mut LibraryIndex,
) {
    use crate::board_template::{BOARD_TEMPLATE_EXTENSION, BOARD_TEMPLATE_FOLDER, check};
    let dir = root.path.join(BOARD_TEMPLATE_FOLDER);
    let names = match files.list(&dir) {
        Ok(names) => names,
        Err(why) => return index.problem(&dir, why),
    };
    for name in with_extension(names, BOARD_TEMPLATE_EXTENSION) {
        let path = dir.join(name);
        let template = match files.read_board_template(&path) {
            Ok(template) => template,
            Err(why) => {
                index.problem(&path, why);
                continue;
            }
        };
        let problems = check(&template);
        if !problems.is_empty() {
            index.problem(
                &path,
                format!(
                    "Board Template {:?} can't be used: {}",
                    template.name,
                    problems.join("; ")
                ),
            );
            continue;
        }
        if index.board_template_named(library, &template.name) {
            index.problem(
                &path,
                format!(
                    "another Board Template of this Library is already named {:?}",
                    template.name
                ),
            );
            continue;
        }
        index.add_element(library, Element::BoardTemplate(template), path);
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
