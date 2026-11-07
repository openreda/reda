//! The library Commands: `project.library.add` and `project.library.remove`, which edit the
//! Project Library paths in `.reda` on the Project's undo stack.

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::Object;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Adds every Command defined in `reda-library`.
pub fn register_all(registry: &mut Registry) {
    registry.add(ProjectLibraryAdd);
    registry.add(ProjectLibraryRemove);
    crate::edit::register_all(registry);
    registry.add(crate::generate::Regenerate);
    registry.add(crate::generate::RegeneratePreview);
}

/// Params naming one Project Library folder.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryPathParams {
    /// The Library folder, relative to the Project folder, such as `parts` or
    /// `../shared/libs`. Either slash separates folders.
    pub path: String,
}

/// The Project Library paths after the edit, in order.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct LibraryPathResult {
    pub libraries: Vec<String>,
}

pub struct ProjectLibraryAdd;

impl Command for ProjectLibraryAdd {
    const META: Meta = Meta {
        id: "project.library.add",
        title: "Add Project Library",
        description: "Registers a Library folder with the Project. The path is relative to \
                      the Project folder and is stored in the `.reda` file, so the Library \
                      travels with the Project.",
        category: "library",
        icon: "library_add",
        kind: Kind::Edit,
    };
    type Params = LibraryPathParams;
    type Output = LibraryPathResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: LibraryPathParams,
    ) -> Result<LibraryPathResult, CommandError> {
        let path = normalise_library_path(&p.path)
            .map_err(|why| CommandError::invalid_params("/path", why))?;
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        if props.libraries.contains(&path) {
            return Err(CommandError::rejected(format!(
                "{path} is already a Project Library"
            )));
        }
        props.libraries.push(path);
        let libraries = props.libraries.clone();
        tx.replace(Object::Project(props))?;
        Ok(LibraryPathResult { libraries })
    }
}

pub struct ProjectLibraryRemove;

impl Command for ProjectLibraryRemove {
    const META: Meta = Meta {
        id: "project.library.remove",
        title: "Remove Project Library",
        description: "Unregisters a Library folder from the Project. The folder and its \
                      files stay on disk.",
        category: "library",
        icon: "library_remove",
        kind: Kind::Edit,
    };
    type Params = LibraryPathParams;
    type Output = LibraryPathResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: LibraryPathParams,
    ) -> Result<LibraryPathResult, CommandError> {
        let path = normalise_library_path(&p.path)
            .map_err(|why| CommandError::invalid_params("/path", why))?;
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        let Some(at) = props.libraries.iter().position(|l| *l == path) else {
            return Err(CommandError::not_found(format!(
                "{path} is not a Project Library"
            )));
        };
        props.libraries.remove(at);
        let libraries = props.libraries.clone();
        tx.replace(Object::Project(props))?;
        Ok(LibraryPathResult { libraries })
    }
}

/// A Project Library path in its stored form: relative, `/`-separated, with no `.`
/// segments, no `name/..` pairs and no trailing slash. Absolute paths are refused, because
/// the path must keep working when the Project folder moves.
pub fn normalise_library_path(path: &str) -> Result<String, String> {
    let bytes = path.as_bytes();
    let has_drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if path.starts_with(['/', '\\']) || has_drive {
        return Err(format!(
            "{path:?} is absolute; give the Library folder relative to the Project folder"
        ));
    }
    let mut segments: Vec<&str> = vec![];
    for segment in path.split(['/', '\\']) {
        match segment {
            "" | "." => {}
            ".." if segments.last().is_some_and(|s| *s != "..") => {
                segments.pop();
            }
            _ => segments.push(segment),
        }
    }
    if segments.is_empty() {
        return Err(format!(
            "{path:?} names the Project folder itself, not a Library folder in it"
        ));
    }
    Ok(segments.join("/"))
}
