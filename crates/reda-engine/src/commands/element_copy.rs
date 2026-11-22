//! `library.element.copy`: Copy to Library… and Duplicate in the Library panel, for
//! Symbols, Footprints, Parts and Board Templates.

use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_library::{ElementKind, Library, copy_element, free_name};
use reda_model::{BoardTemplate, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::fs::write_atomic;
use crate::library::{element_file, scan_registered, writable_library, write_new_element};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ElementCopy);
}

pub struct ElementCopy;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementCopyParams {
    /// The Symbol, Footprint, Part or Board Template to copy, from any registered Library.
    pub element: Uuid,
    /// The writable Library to copy it into; its own Library duplicates it.
    pub library: Uuid,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ElementCopyResult {
    /// The copy's new UUID.
    pub element: Uuid,
    pub kind: ElementKind,
    /// The copy's name: the original's, with `_1`, `_2`… added when that is taken.
    pub name: String,
    /// The file written.
    pub path: PathBuf,
}

impl Command for ElementCopy {
    const META: Meta = Meta {
        id: "library.element.copy",
        title: "Copy to Library",
        description: "Copies a Symbol, Footprint, Part or Board Template into a writable \
                      registered Library under a new UUID, adding `_1`, `_2`… to a name \
                      already taken there. A copied Part still references its original \
                      Symbol and Footprints, since Parts can reference across Libraries; a \
                      copied Board Template carries its own Part copies.",
        category: "library",
        icon: "duplicate",
        kind: Kind::File,
    };
    type Params = ElementCopyParams;
    type Output = ElementCopyResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ElementCopyParams,
    ) -> Result<ElementCopyResult, CommandError> {
        let uuid = cx.new_id();
        let index = scan_registered(cx.session()?);
        if let Some(template) = index.board_template(p.element) {
            let library = writable_library(&index, p.library)?;
            return copy_template(library, template, uuid);
        }
        let original = index.element(p.element).ok_or_else(|| {
            CommandError::not_found(format!("no registered Library holds element {}", p.element))
        })?;
        let library = writable_library(&index, p.library)?;
        let kind = ElementKind::of(&original);
        let name = free_name(original.name(), |name| {
            let candidate = copy_element(&original, uuid, name);
            library.holds_name(kind, name) || element_file(library, &candidate).exists()
        });
        let copy = copy_element(&original, uuid, &name);
        let path = write_new_element(library, &copy)?;
        Ok(ElementCopyResult {
            element: uuid,
            kind,
            name,
            path,
        })
    }
}

/// The file a Board Template called `name` gets in `library`.
fn template_file(library: &Library, name: &str) -> PathBuf {
    let kind = ElementKind::BoardTemplate;
    library
        .root
        .join(kind.folder())
        .join(format!("{name}.{}", kind.extension()))
}

/// `template` written into `library` as element `uuid`, under a free name.
fn copy_template(
    library: &Library,
    template: &BoardTemplate,
    uuid: Uuid,
) -> Result<ElementCopyResult, CommandError> {
    let kind = ElementKind::BoardTemplate;
    let name = free_name(&template.name, |name| {
        library.holds_name(kind, name) || template_file(library, name).exists()
    });
    let mut copy = template.clone();
    copy.uuid = uuid;
    copy.name = name.clone();
    let bytes = crate::format::write_board_template(&copy)
        .map_err(|e| CommandError::internal(e.to_string()))?;
    let path = template_file(library, &name);
    let io = |e: std::io::Error| CommandError::internal(format!("writing {}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    write_atomic(&path, &bytes).map_err(io)?;
    Ok(ElementCopyResult {
        element: uuid,
        kind,
        name,
        path,
    })
}
