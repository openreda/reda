//! Edit in Library: `schematic.component.library_element` finds the Library element files
//! a placed Component came from, so the matching editor can open them.

use reda_command::{Command, CommandError, Context, ElementLocation, Kind, Meta};
use reda_model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::properties::units_of;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryElementParams {
    /// The placed Component, by UUID.
    pub component: Uuid,
}

/// One Library element on disk.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ElementFile {
    /// The element's UUID, to open with `library.element.open`.
    pub element: Uuid,
    /// The UUID of the Library that holds it.
    pub library: Uuid,
    /// The element's file.
    pub path: String,
    /// Whether its Library is read-only (the starter Library).
    pub read_only: bool,
}

impl ElementFile {
    fn new(element: Uuid, at: ElementLocation) -> ElementFile {
        ElementFile {
            element,
            library: at.library,
            path: at.path.to_string_lossy().into_owned(),
            read_only: at.read_only,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct LibraryElementResult {
    /// The Component's Part.
    pub part: ElementFile,
    /// The Part's Symbol; omitted when no registered Library holds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<ElementFile>,
    /// The Component's chosen Footprint; omitted when it has none or no registered
    /// Library holds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footprint: Option<ElementFile>,
}

pub struct LibraryElement;

impl Command for LibraryElement {
    const META: Meta = Meta {
        id: "schematic.component.library_element",
        title: "Find Library Element",
        description: "Finds the Library elements a placed Component came from, for Edit in \
                      Library: the files of its Part, the Part's Symbol and the Component's \
                      Footprint in the registered Libraries, and whether each Library is \
                      read-only. `not_found` when no registered Library holds the Part.",
        category: "schematic",
        icon: "library_edit",
        kind: Kind::Query,
    };
    type Params = LibraryElementParams;
    type Output = LibraryElementResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: LibraryElementParams,
    ) -> Result<LibraryElementResult, CommandError> {
        let project = cx.project()?;
        let units = units_of(project, p.component);
        let Some(&(sheet, unit)) = units.first() else {
            return Err(CommandError::not_found(format!(
                "no Component has UUID {}",
                p.component
            )));
        };
        let entry = crate::sheet(project, sheet)?
            .part(unit.part)
            .expect("a Unit's part entry exists");
        let part = entry.part.uuid;
        let symbol = entry.part.symbol;
        let footprint = unit.footprint;
        let wanted: Vec<Uuid> = [Some(part), symbol, footprint]
            .into_iter()
            .flatten()
            .collect();
        let mut found = wanted.iter().copied().zip(cx.library_locations(&wanted));
        let mut next = |element: Option<Uuid>| {
            element?;
            let (element, at) = found.next().expect("one location per element");
            Some(ElementFile::new(element, at?))
        };
        let Some(part_file) = next(Some(part)) else {
            return Err(CommandError::not_found(format!(
                "no registered Library holds Part {} ({part}) of {}",
                entry.part.name, unit.designator
            )));
        };
        Ok(LibraryElementResult {
            part: part_file,
            symbol: next(symbol),
            footprint: next(footprint),
        })
    }
}
