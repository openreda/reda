//! Extract to Library: which elements of a Project's part tables go into a Library.
//!
//! The elements keep their UUIDs, so the Library copy matches the placed one and the
//! Components read Up to Date. A Footprint copy carries no `generator` block, so an
//! extracted Footprint offers no Regenerate.

use std::collections::BTreeSet;

use reda_model::{DocumentKind, LibraryElement, PartEntry, Project, SchematicSheet, Uuid};

use crate::index::Library;
use crate::scan::ElementKind;

/// What extracting writes and skips.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtractPlan {
    /// The elements to write, each once: for each part-table entry its Symbol, its
    /// Footprints, then its Part.
    pub write: Vec<LibraryElement>,
    /// The elements the Library already holds, by UUID, each once.
    pub skipped: Vec<LibraryElement>,
}

/// Why nothing can be extracted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtractError {
    /// No sheet of the Project has a Component with this UUID.
    UnknownComponent(Uuid),
    /// The Library already holds the Part of a named Component.
    PartHeld { designator: String, part: String },
    /// Another element of the same kind in the Library, or another element being
    /// extracted, already has this name.
    NameTaken { kind: ElementKind, name: String },
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractError::UnknownComponent(uuid) => {
                write!(f, "the Project has no Component {uuid}")
            }
            ExtractError::PartHeld { designator, part } => write!(
                f,
                "the Library already holds Part {part} of {designator}; nothing was extracted"
            ),
            ExtractError::NameTaken { kind, name } => {
                let kind = match kind {
                    ElementKind::Symbol => "Symbol",
                    ElementKind::Footprint => "Footprint",
                    ElementKind::Part => "Part",
                };
                write!(
                    f,
                    "another {kind} is already called {name}; nothing was extracted"
                )
            }
        }
    }
}

/// Plans extracting the Components `components` of `project`, by Component UUID, into
/// `library`; with `None`, every Component (Make Project Library).
///
/// Naming a Component whose Part `library` already holds is refused. Otherwise the
/// elements `library` holds by UUID are skipped and listed, and any element whose name
/// another element of its kind already has in `library` refuses the whole extraction.
pub fn plan_extract(
    project: &Project,
    components: Option<&[Uuid]>,
    library: &Library,
) -> Result<ExtractPlan, ExtractError> {
    let sheets: Vec<&SchematicSheet> = project
        .documents()
        .filter(|d| d.kind == DocumentKind::Schematic)
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .collect();
    let entries: Vec<(&SchematicSheet, &PartEntry)> = match components {
        None => sheets
            .iter()
            .flat_map(|sheet| sheet.parts().map(move |e| (*sheet, e)))
            .collect(),
        Some(components) => {
            let mut entries = vec![];
            for &component in components {
                let (sheet, unit) = sheets
                    .iter()
                    .find_map(|s| {
                        s.units()
                            .find(|u| u.component.uuid == component)
                            .map(|u| (*s, u))
                    })
                    .ok_or(ExtractError::UnknownComponent(component))?;
                let entry = sheet
                    .part(unit.part)
                    .ok_or(ExtractError::UnknownComponent(component))?;
                if library.holds(entry.part.uuid) {
                    return Err(ExtractError::PartHeld {
                        designator: unit.designator.clone(),
                        part: entry.part.name.clone(),
                    });
                }
                entries.push((sheet, entry));
            }
            entries
        }
    };

    let mut plan = ExtractPlan::default();
    let mut seen = BTreeSet::new();
    let mut names = BTreeSet::new();
    for (sheet, entry) in entries {
        let footprints = entry
            .footprints
            .iter()
            .filter_map(|copy| sheet.footprint(*copy))
            .map(|copy| LibraryElement::Footprint(copy.footprint.without_generator()));
        let elements = std::iter::once(LibraryElement::Symbol(entry.symbol.clone()))
            .chain(footprints)
            .chain(std::iter::once(LibraryElement::Part(entry.part.clone())));
        for element in elements {
            if !seen.insert(element.uuid()) {
                continue;
            }
            if library.holds(element.uuid()) {
                plan.skipped.push(element);
                continue;
            }
            let kind = ElementKind::of(&element);
            let name = (kind, element.name().to_owned());
            if library.holds_name(kind, element.name()) || !names.insert(name) {
                return Err(ExtractError::NameTaken {
                    kind,
                    name: element.name().to_owned(),
                });
            }
            plan.write.push(element);
        }
    }
    Ok(plan)
}
