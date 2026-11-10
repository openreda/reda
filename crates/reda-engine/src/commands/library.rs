//! `library.search`: the Library panel's search, over every registered Library, and
//! `library.create`: a new, empty Library folder.

use std::path::{Path, PathBuf};

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_library::{
    LIBRARY_EXTENSION, Origin, ScanProblem, SearchQuery, check_element_name, normalise_library_path,
};
use reda_model::{ComponentType, Fields, LibraryInfo, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::format::library::write_library_info;
use crate::fs::write_atomic;
use crate::library::scan_registered;
use crate::preferences;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(LibrarySearch);
    registry.add(LibraryCreate);
}

pub struct LibrarySearch;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SearchParams {
    /// Words separated by spaces, each of which must match, ignoring case: a plain word as
    /// a substring of a Part's name or any of its field values (Value, Description,
    /// Keywords, MPN, the free-form fields…), a `field:value` word such as `mpn:GRM188` or
    /// `lcsc:C14` only in that field (`name:` for the Part's name; a Part without the field
    /// never matches). `*` stands for any run of characters. Empty matches every Part.
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
    /// Whether its Components go into the BOM, by default.
    pub in_bom: bool,
    /// Whether its Components go into the CPL, by default.
    pub in_cpl: bool,
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

pub(crate) fn summary(library: &reda_library::Library) -> LibrarySummary {
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
        description: "Scans the active registered Libraries (the Project's, the user's from \
                      Preferences and the starter Library, in that resolution order) and \
                      returns the Parts matching every word of `text` (`field:value` words \
                      and `*` wildcards included), optionally only from one Library or of \
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
                in_bom: hit.part.in_bom,
                in_cpl: hit.part.in_cpl,
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

pub struct LibraryCreate;

/// Where a new Library is registered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Register {
    /// Not registered. For a Project Library, run `project.library.add` with the result's
    /// `project_path` next, which is undoable on the Project's stack.
    #[default]
    None,
    /// Added to the user's Libraries in Preferences (`libraries.paths`).
    User,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateParams {
    /// The Library folder: absolute, or relative to the open Project's folder. It may
    /// exist already, but must not hold a Library yet.
    pub path: String,
    /// The Library's name, which is also the stem of its `.redalib` file.
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub register: Register,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CreateResult {
    /// The new Library's UUID.
    pub library: Uuid,
    pub name: String,
    /// The Library folder.
    pub path: PathBuf,
    /// The folder relative to the open Project's folder, in the form
    /// `project.library.add` stores, when it lies inside that folder or was given
    /// relative to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
}

impl Command for LibraryCreate {
    const META: Meta = Meta {
        id: "library.create",
        title: "New Library",
        description: "Creates an empty Library by writing `<name>.redalib` into the folder \
                      `path` (absolute, or relative to the Project folder). `register: user` \
                      also adds it to the user's Libraries in Preferences; for a Project \
                      Library, run `project.library.add` with the returned `project_path`.",
        category: "library",
        icon: "library_add",
        kind: Kind::File,
    };
    type Params = CreateParams;
    type Output = CreateResult;

    fn run(&self, cx: &mut Context<'_>, p: CreateParams) -> Result<CreateResult, CommandError> {
        check_element_name(&p.name).map_err(|why| CommandError::invalid_params("/name", why))?;
        let uuid = cx.new_id();
        let session = cx.session()?;
        let project_folder = session.project.as_ref().and(session.location.clone());
        let given = Path::new(&p.path);
        let (folder, project_path) = if given.is_absolute() {
            let relative = project_folder
                .as_deref()
                .and_then(|base| given.strip_prefix(base).ok())
                .and_then(|rel| normalise_library_path(&rel.to_string_lossy()).ok());
            (given.to_path_buf(), relative)
        } else {
            let base = project_folder.ok_or_else(|| {
                CommandError::invalid_params(
                    "/path",
                    "a relative folder needs an open Project; give an absolute folder",
                )
            })?;
            let relative = normalise_library_path(&p.path)
                .map_err(|why| CommandError::invalid_params("/path", why))?;
            (base.join(&relative), Some(relative))
        };
        if let Ok(entries) = std::fs::read_dir(&folder) {
            let marker = entries
                .flatten()
                .find(|e| e.path().extension().is_some_and(|x| x == LIBRARY_EXTENSION));
            if let Some(marker) = marker {
                return Err(CommandError::rejected(format!(
                    "{} is already a Library ({})",
                    folder.display(),
                    marker.file_name().to_string_lossy()
                )));
            }
        }
        let info = LibraryInfo {
            uuid,
            name: p.name.clone(),
            description: p.description,
        };
        let bytes = write_library_info(&info).map_err(|e| CommandError::internal(e.to_string()))?;
        let file = folder.join(format!("{}.{LIBRARY_EXTENSION}", p.name));
        std::fs::create_dir_all(&folder)
            .and_then(|()| write_atomic(&file, &bytes))
            .map_err(|e| CommandError::internal(format!("writing {}: {e}", file.display())))?;
        if p.register == Register::User {
            let dirs = session.user_dirs.clone().ok_or_else(|| {
                CommandError::unavailable("registering a user Library needs the user folders")
            })?;
            let mut paths = preferences::current(Some(&dirs)).libraries.paths;
            let path = folder.to_string_lossy().into_owned();
            if !paths.contains(&path) {
                paths.push(path);
            }
            preferences::set(&dirs, "libraries.paths", serde_json::json!(paths))?;
        }
        Ok(CreateResult {
            library: uuid,
            name: p.name,
            path: folder,
            project_path,
        })
    }
}
