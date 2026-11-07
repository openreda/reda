//! `library.extract_from_project`: Extract to Library, and with no `components` Make
//! Project Library. The planning is `reda_library::plan_extract`; this writes the files.

use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_library::{ElementKind, ExtractError, plan_extract};
use reda_model::{LibraryElement, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::library::{scan_registered, writable_library, write_new_element};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ExtractFromProject);
}

pub struct ExtractFromProject;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractParams {
    /// The Components to extract, by UUID. Left out: every Component of the Project (Make
    /// Project Library).
    #[serde(default)]
    pub components: Option<Vec<Uuid>>,
    /// The writable registered Library to write into.
    pub library: Uuid,
}

/// One element written.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ExtractedElement {
    /// Its UUID, the same as in the part table.
    pub element: Uuid,
    pub kind: ElementKind,
    pub name: String,
    /// The file written.
    pub path: PathBuf,
}

/// One element the Library already held, so it was left as it is.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SkippedElement {
    pub element: Uuid,
    pub kind: ElementKind,
    pub name: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ExtractResult {
    pub written: Vec<ExtractedElement>,
    /// The elements that Library already held.
    pub skipped: Vec<SkippedElement>,
}

impl Command for ExtractFromProject {
    const META: Meta = Meta {
        id: "library.extract_from_project",
        title: "Extract to Library",
        description: "Writes the Parts, Symbols and Footprints of Components from the part \
                      tables into a writable registered Library, keeping their UUIDs, so \
                      the Components read Up to Date. Naming a Component whose Part the \
                      Library already holds is refused. Leaving out `components` extracts \
                      the whole Project (Make Project Library). Elements the Library \
                      already holds are skipped and listed; a name another element already \
                      has there refuses the whole extraction. Extracted Footprints carry no \
                      `generator`.",
        category: "library",
        icon: "library_add",
        kind: Kind::File,
    };
    type Params = ExtractParams;
    type Output = ExtractResult;

    fn run(&self, cx: &mut Context<'_>, p: ExtractParams) -> Result<ExtractResult, CommandError> {
        let session = cx.session()?;
        let index = scan_registered(session);
        let library = writable_library(&index, p.library)?;
        let project = session
            .project
            .as_ref()
            .ok_or_else(|| CommandError::unavailable("no Project is open"))?;
        let plan =
            plan_extract(project, p.components.as_deref(), library).map_err(|e| match e {
                ExtractError::UnknownComponent(_) => CommandError::not_found(e.to_string()),
                _ => CommandError::rejected(e.to_string()),
            })?;
        let mut written = vec![];
        for element in &plan.write {
            let path = write_new_element(library, element)?;
            written.push(ExtractedElement {
                element: element.uuid(),
                kind: ElementKind::of(element),
                name: element.name().to_owned(),
                path,
            });
        }
        let skipped = plan
            .skipped
            .iter()
            .map(|e: &LibraryElement| SkippedElement {
                element: e.uuid(),
                kind: ElementKind::of(e),
                name: e.name().to_owned(),
            })
            .collect();
        Ok(ExtractResult { written, skipped })
    }
}
