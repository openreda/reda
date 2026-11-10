//! `library.element.copy`: Copy to Library… and Duplicate in the Library panel.

use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_library::{ElementKind, copy_element, free_name};
use reda_model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::library::{element_file, scan_registered, writable_library, write_new_element};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ElementCopy);
}

pub struct ElementCopy;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementCopyParams {
    /// The Symbol, Footprint or Part to copy, from any registered Library.
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
        description: "Copies a Symbol, Footprint or Part into a writable registered Library \
                      under a new UUID, adding `_1`, `_2`… to a name already taken there. A \
                      copied Part still references its original Symbol and Footprints, \
                      since Parts can reference across Libraries.",
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
