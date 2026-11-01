//! `library.search`: the Library panel's search, over every registered Library.

use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_library::{Origin, ScanProblem, SearchQuery};
use reda_model::{ComponentType, Fields, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::library::scan_registered;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(LibrarySearch);
}

pub struct LibrarySearch;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SearchParams {
    /// Matched, ignoring case, as a substring of a Part's name or any of its field values
    /// (Value, Description, Keywords, MPN, the free-form fields…). Empty matches every Part.
    pub text: String,
    /// Only Parts of the Library with this UUID.
    pub library: Option<Uuid>,
    /// Only Parts of this Component Type.
    pub component_type: Option<ComponentType>,
}

/// One scanned Library.
#[derive(Debug, Serialize, JsonSchema)]
pub struct LibrarySummary {
    pub uuid: Uuid,
    pub name: String,
    pub origin: Origin,
    /// The Library folder.
    pub path: PathBuf,
}

/// One Part found.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PartHit {
    pub uuid: Uuid,
    pub name: String,
    pub component_type: ComponentType,
    pub symbol: Option<Uuid>,
    /// The alternate Footprints, the default first.
    pub footprints: Vec<Uuid>,
    pub fields: Fields,
    pub library: LibrarySummary,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SearchResult {
    /// Every registered Library that scanned, in scan order.
    pub libraries: Vec<LibrarySummary>,
    /// The matching Parts, sorted by name.
    pub parts: Vec<PartHit>,
    /// Folders and files that could not be used, and why.
    pub problems: Vec<ScanProblem>,
}

fn summary(library: &reda_library::Library) -> LibrarySummary {
    LibrarySummary {
        uuid: library.info.uuid,
        name: library.info.name.clone(),
        origin: library.origin,
        path: library.root.clone(),
    }
}

impl Command for LibrarySearch {
    const META: Meta = Meta {
        id: "library.search",
        title: "Search Libraries",
        description: "Scans the registered Libraries (the starter Library, the user's from \
                      Preferences and the Project's) and returns the Parts whose name or \
                      field values contain `text`, optionally only from one Library or of \
                      one Component Type.",
        category: "library",
        icon: "search",
        kind: Kind::File,
    };
    type Params = SearchParams;
    type Output = SearchResult;

    fn run(&self, cx: &mut Context<'_>, p: SearchParams) -> Result<SearchResult, CommandError> {
        let index = scan_registered(cx.session()?);
        let query = SearchQuery {
            text: p.text,
            library: p.library,
            component_type: p.component_type,
        };
        let parts = index
            .search(&query)
            .into_iter()
            .map(|hit| PartHit {
                uuid: hit.part.uuid,
                name: hit.part.name.clone(),
                component_type: hit.part.component_type,
                symbol: hit.part.symbol,
                footprints: hit.part.footprints.clone(),
                fields: hit.part.fields.clone(),
                library: summary(hit.library),
            })
            .collect();
        Ok(SearchResult {
            libraries: index.libraries().map(summary).collect(),
            parts,
            problems: index.problems().to_vec(),
        })
    }
}
