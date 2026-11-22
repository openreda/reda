//! `library.element.delete`: Delete in the Library panel, for Symbols, Footprints, Parts
//! and Board Templates.

use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_library::ElementKind;
use reda_model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::library::{scan_registered, writable_library};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ElementDelete);
}

pub struct ElementDelete;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementDeleteParams {
    /// The Symbol, Footprint, Part or Board Template to delete.
    pub element: Uuid,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ElementDeleteResult {
    pub kind: ElementKind,
    pub name: String,
    /// The file removed.
    pub path: PathBuf,
}

impl Command for ElementDelete {
    const META: Meta = Meta {
        id: "library.element.delete",
        title: "Delete from Library",
        description: "Deletes a Symbol, Footprint, Part or Board Template from its writable \
                      Library by removing its file. It can't be undone. Refuses the Starter \
                      Library, an element open in its editor, and a Symbol or Footprint a \
                      Part of a registered Library still uses. Placed copies in Projects \
                      are unchanged.",
        category: "library",
        icon: "delete",
        kind: Kind::File,
    };
    type Params = ElementDeleteParams;
    type Output = ElementDeleteResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ElementDeleteParams,
    ) -> Result<ElementDeleteResult, CommandError> {
        let session = cx.session()?;
        let index = scan_registered(session);
        let not_found = || {
            CommandError::not_found(format!("no registered Library holds element {}", p.element))
        };
        let held = index.library_of(p.element).ok_or_else(not_found)?;
        let element = index.element(p.element).ok_or_else(not_found)?;
        let (kind, name) = (ElementKind::of(&element), element.name().to_owned());
        let library = writable_library(&index, held.info.uuid, kind)?;
        if session
            .elements_mut()
            .is_some_and(|e| e.element(p.element).is_some())
        {
            return Err(CommandError::rejected(format!(
                "{} {name} is open in its editor; close it first",
                kind.name()
            )));
        }
        if matches!(kind, ElementKind::Symbol | ElementKind::Footprint) {
            let users: Vec<String> = (index.parts_using(p.element).into_iter())
                .map(|hit| hit.part.name.clone())
                .collect();
            if !users.is_empty() {
                return Err(CommandError::rejected(format!(
                    "{} {name} is used by {}; delete or change those Parts first",
                    kind.name(),
                    users.join(", ")
                )));
            }
        }
        let path = index.file_of(p.element).ok_or_else(not_found)?.to_owned();
        std::fs::remove_file(&path).map_err(|e| {
            CommandError::internal(format!(
                "deleting {} from {}: {e}",
                path.display(),
                library.info.name
            ))
        })?;
        Ok(ElementDeleteResult { kind, name, path })
    }
}
