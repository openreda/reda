//! `project.package`: the Project folder as one zip to hand on, `.reda/` left out.

use std::path::{Path, PathBuf};

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_PROJECT, Registry,
    Timestamp, Unavailable,
};
use reda_model::{Project, Scope};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::fs::write_atomic;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ProjectPackage);
}

/// The folder of per-user and per-machine state inside a Project folder.
const STATE_FOLDER: &str = ".reda";

pub struct ProjectPackage;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectPackageParams {
    /// The zip file to write.
    pub path: PathBuf,
    /// Also packs the Output Jobs' output folders. Off, they are left out.
    #[serde(default)]
    pub include_outputs: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ProjectPackageResult {
    /// The zip written.
    pub path: PathBuf,
    /// Every file packed, by its `/`-separated path in the zip (relative to the Project
    /// folder), in zip order.
    pub files: Vec<String>,
    /// Project Library paths outside the Project folder, which the package leaves out;
    /// left out when there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outside_libraries: Vec<String>,
    /// The Project has unsaved changes, which the package (made from the files on disk)
    /// does not hold.
    pub unsaved: bool,
}

impl Command for ProjectPackage {
    const META: Meta = Meta {
        id: "project.package",
        title: "Package Project",
        description: "Zips the Project folder as saved on disk into `path`, with paths \
                      relative to the folder, leaving out `.reda/` (recovery and view \
                      state) and, unless `include_outputs`, the Output Jobs' output \
                      folders. Project Libraries inside the folder come along; those \
                      outside it are listed in `outside_libraries`. The same files always \
                      give the same zip.",
        category: "project",
        icon: "output_run",
        kind: Kind::File,
    };
    type Params = ProjectPackageParams;
    type Output = ProjectPackageResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            Some(_) => Ok(()),
            None => Err(Unavailable {
                reason: NO_PROJECT.into(),
            }),
        }
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ProjectPackageParams,
    ) -> Result<ProjectPackageResult, CommandError> {
        let session = cx.session()?;
        let project =
            (session.project.as_ref()).ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
        let folder = (session.location.clone())
            .ok_or_else(|| CommandError::unavailable("the Project has no folder yet"))?;
        let outputs: Vec<String> = if p.include_outputs {
            vec![]
        } else {
            project.output_jobs().map(|j| j.destination()).collect()
        };
        let target = std::path::absolute(&p.path).unwrap_or_else(|_| p.path.clone());
        let skip = |rel: &str, path: &Path| {
            rel == STATE_FOLDER
                || outputs.iter().any(|o| within(rel, o))
                || same_file(path, &target)
        };
        let mut found = vec![];
        walk(&folder, "", &skip, &mut found)?;
        found.sort_by(|a, b| a.0.cmp(&b.0));
        let files: Vec<reda_output::OutputFile> = found
            .into_iter()
            .map(|(name, bytes)| reda_output::OutputFile {
                name,
                function: "project",
                bytes,
            })
            .collect();
        // Zip time stamps start in 1980: every entry gets 1980-01-01, whenever it is made.
        let bytes = reda_output::zip(&files, Timestamp(0));
        write_atomic(&p.path, &bytes)
            .map_err(|e| CommandError::internal(format!("writing {}: {e}", p.path.display())))?;
        let outside_libraries = outside_libraries(project);
        let unsaved = session
            .history
            .dirty_scopes()
            .iter()
            .any(|scope| match scope {
                Scope::Project => true,
                Scope::Document(uuid) => project.document(*uuid).is_some(),
                Scope::Element(_) => false,
            });
        Ok(ProjectPackageResult {
            path: p.path,
            files: files.into_iter().map(|f| f.name).collect(),
            outside_libraries,
            unsaved,
        })
    }
}

/// The Project Library paths outside the Project folder, which a package leaves out.
pub fn outside_libraries(project: &Project) -> Vec<String> {
    (project.props().libraries.iter())
        .filter(|l| l.split('/').next() == Some(".."))
        .cloned()
        .collect()
}

/// Whether the `/`-separated path `rel` is `folder` or inside it.
fn within(rel: &str, folder: &str) -> bool {
    let folder = folder.trim_end_matches('/');
    rel == folder || rel.strip_prefix(folder).is_some_and(|r| r.starts_with('/'))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Every file under `dir` that `skip` keeps, as `(path relative to the Project folder,
/// bytes)`. Symbolic links are not followed.
fn walk(
    dir: &Path,
    prefix: &str,
    skip: &dyn Fn(&str, &Path) -> bool,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), CommandError> {
    let io = |path: &Path, e: std::io::Error| {
        CommandError::internal(format!("reading {}: {e}", path.display()))
    };
    for entry in std::fs::read_dir(dir).map_err(|e| io(dir, e))? {
        let entry = entry.map_err(|e| io(dir, e))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = format!("{prefix}{name}");
        let kind = entry.file_type().map_err(|e| io(&path, e))?;
        if kind.is_symlink() || skip(&rel, &path) {
            continue;
        }
        if kind.is_dir() {
            walk(&path, &format!("{rel}/"), skip, out)?;
        } else if kind.is_file() {
            let bytes = std::fs::read(&path).map_err(|e| io(&path, e))?;
            out.push((rel, bytes));
        }
    }
    Ok(())
}
