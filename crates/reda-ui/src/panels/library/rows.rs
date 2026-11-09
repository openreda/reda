//! What the Library panel lists: its mode (Parts to place, or a Library's elements beside
//! a Library editor tab), the rows a search finds in that mode, and each Part row's
//! Library Status over the open Project ("3 placed, 1 differs").

use std::collections::BTreeMap;

use reda_engine::library::{ElementKind, LibraryIndex, LibraryOrigin, SearchQuery};
use reda_engine::model::{ComponentType, Part, Project, Uuid};
use reda_engine::schematic::{ComponentStatus, LibraryStatus};

use super::Placing;
use crate::tr;

/// What the panel lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Parts to place in the active Document (or nowhere).
    Place(Placing),
    /// Beside a Library editor tab: the Symbols, Footprints and Parts of that element's
    /// Library.
    Library(Uuid),
}

/// One row: a Part, or in Library mode also a Symbol or Footprint.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub kind: ElementKind,
    pub uuid: Uuid,
    pub name: String,
    /// The name of the Library that holds it, and that Library.
    pub library_name: String,
    pub library: Uuid,
    /// Whether that Library is the read-only starter Library.
    pub read_only: bool,
    /// The Part, for a Part row.
    pub part: Option<Part>,
}

impl Entry {
    /// The row's accessible label: the name, with the kind in front in Library mode.
    pub fn label(&self, mode: Mode) -> String {
        match mode {
            Mode::Place(_) => self.name.clone(),
            Mode::Library(_) => {
                let kind = match self.kind {
                    ElementKind::Symbol => tr!("library-kind-symbol"),
                    ElementKind::Footprint => tr!("library-kind-footprint"),
                    ElementKind::Part => tr!("library-kind-part"),
                };
                format!("{kind}: {}", self.name)
            }
        }
    }

    /// The row's text: the label, with a Part's Value after it when it says more.
    pub fn text(&self, mode: Mode) -> String {
        let label = self.label(mode);
        match &self.part {
            Some(p) if !p.fields.value.is_empty() && p.fields.value != p.name => {
                format!("{label} · {}", p.fields.value)
            }
            _ => label,
        }
    }
}

/// The rows a search finds in `mode`: on a sheet the Parts with a Symbol, on the board
/// Mechanical Parts; in Library mode that Library's Symbols, Footprints and Parts, in that
/// order, each kind sorted by name. Symbols and Footprints have no fields, so only plain
/// and `name:` words reach them, and the Component Type filter only Parts.
pub fn entries(index: &LibraryIndex, mode: Mode, query: &SearchQuery) -> Vec<Entry> {
    let entry = |library: &reda_engine::library::Library, kind, uuid, name: &str| Entry {
        kind,
        uuid,
        name: name.to_owned(),
        library_name: library.info.name.clone(),
        library: library.info.uuid,
        read_only: library.origin == LibraryOrigin::Starter,
        part: None,
    };
    let part_entry = |library: &reda_engine::library::Library, part: &Part| Entry {
        part: Some(part.clone()),
        ..entry(library, ElementKind::Part, part.uuid, &part.name)
    };
    match mode {
        Mode::Place(target) => index
            .search(query)
            .into_iter()
            .filter(|hit| fits(hit.part, target))
            .map(|hit| part_entry(hit.library, hit.part))
            .collect(),
        Mode::Library(uuid) => {
            let Some(library) = index.library(uuid) else {
                return Vec::new();
            };
            let mut rows = Vec::new();
            if query.component_type.is_none() {
                let mut named: Vec<(&str, Uuid, ElementKind)> = library
                    .symbols
                    .values()
                    .map(|s| (s.name.as_str(), s.uuid, ElementKind::Symbol))
                    .chain(
                        (library.footprints.values())
                            .map(|f| (f.name.as_str(), f.uuid, ElementKind::Footprint)),
                    )
                    .filter(|(name, _, _)| query.matches_name(name))
                    .collect();
                named.sort_by(|a, b| (a.2 as u8, a.0, a.1).cmp(&(b.2 as u8, b.0, b.1)));
                rows.extend(
                    named
                        .into_iter()
                        .map(|(name, uuid, kind)| entry(library, kind, uuid, name)),
                );
            }
            let query = SearchQuery {
                library: Some(uuid),
                ..query.clone()
            };
            rows.extend(
                index
                    .search(&query)
                    .into_iter()
                    .map(|hit| part_entry(hit.library, hit.part)),
            );
            rows
        }
    }
}

fn fits(part: &Part, target: Placing) -> bool {
    match target {
        Placing::Sheet(_) => part.symbol.is_some(),
        Placing::Board(_) => part.component_type == ComponentType::Mechanical,
        Placing::Nowhere => true,
    }
}

/// How the placed Components of one Part compare with the Libraries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowStatus {
    pub placed: usize,
    pub differs: usize,
    pub missing: usize,
}

impl RowStatus {
    /// "3 placed, 1 differs": what the row shows.
    pub fn text(&self) -> String {
        let mut text = tr!("library-status-placed", count = self.placed);
        if self.differs > 0 {
            text += &tr!("library-status-differs", count = self.differs);
        }
        if self.missing > 0 {
            text += &tr!("library-status-missing", count = self.missing);
        }
        text
    }
}

/// Each Part's [`RowStatus`] from the placed Components' Library Statuses.
pub fn row_statuses(statuses: &[ComponentStatus]) -> BTreeMap<Uuid, RowStatus> {
    let mut rows: BTreeMap<Uuid, RowStatus> = BTreeMap::new();
    for status in statuses {
        let row = rows.entry(status.part).or_default();
        row.placed += 1;
        match status.status {
            LibraryStatus::UpToDate => {}
            LibraryStatus::LibraryDiffers => row.differs += 1,
            LibraryStatus::SourceMissing => row.missing += 1,
        }
    }
    rows
}

/// The Library Statuses of `project`'s placed Components against `index`.
pub fn statuses(project: Option<&Project>, index: &LibraryIndex) -> BTreeMap<Uuid, RowStatus> {
    project.map_or_else(BTreeMap::new, |p| {
        row_statuses(&reda_engine::library::library_statuses(p, index))
    })
}
