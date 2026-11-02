//! `library.check`: the Library check over the registered Libraries, one Library or one
//! element. Open elements are checked as edited, unsaved changes included. It only
//! reports: nothing it finds stops an element from being saved.

use std::collections::BTreeMap;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_library::{ElementKind, Elements, LibraryIndex, ScanProblem, check};
use reda_model::{Footprint, LibraryElement, Project, Symbol, Uuid, Violation};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::library::scan_registered;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(LibraryCheck);
}

pub struct LibraryCheck;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct LibraryCheckParams {
    /// Check only this element (Symbol, Footprint or Part), open or in a registered
    /// Library.
    pub element: Option<Uuid>,
    /// Check every element of this registered Library. With neither `element` nor
    /// `library`, every registered Library is checked.
    pub library: Option<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct LibraryCheckResult {
    /// What the check found, element by element: Symbols, then Footprints, then Parts,
    /// each sorted by UUID, Library by Library.
    pub violations: Vec<Violation>,
    /// Library folders and files that could not be read, and so were not checked.
    pub problems: Vec<ScanProblem>,
}

/// The scanned Libraries with the open elements laid over them, so an element being edited
/// is checked as it is now rather than as it was last saved.
struct Overlay<'a> {
    index: &'a LibraryIndex,
    project: Option<&'a Project>,
}

impl Overlay<'_> {
    fn element(&self, uuid: Uuid) -> Option<LibraryElement> {
        let open = self.project.and_then(|p| p.element(uuid)).cloned();
        open.or_else(|| self.index.element(uuid))
    }

    /// Every element of Library `library`, scanned or open, in result order.
    fn library_elements(&self, library: Uuid) -> Vec<LibraryElement> {
        let mut found: BTreeMap<(usize, Uuid), LibraryElement> = BTreeMap::new();
        let mut add = |element: LibraryElement| {
            let kind = ElementKind::ALL
                .iter()
                .position(|k| *k == ElementKind::of(&element))
                .expect("every kind is listed");
            found.insert((kind, element.uuid()), element);
        };
        if let Some(lib) = self.index.library(library) {
            let uuids = lib.symbols.keys().chain(lib.footprints.keys());
            for uuid in uuids.chain(lib.parts.keys()) {
                add(self.element(*uuid).expect("scanned elements exist"));
            }
        }
        for (source, element) in self.project.iter().flat_map(|p| p.elements()) {
            if source.library == library {
                add(element.clone());
            }
        }
        found.into_values().collect()
    }
}

impl Elements for Overlay<'_> {
    fn symbol(&self, uuid: Uuid) -> Option<&Symbol> {
        match self.project.and_then(|p| p.element(uuid)) {
            Some(LibraryElement::Symbol(s)) => Some(s),
            Some(_) => None,
            None => self.index.symbol(uuid),
        }
    }

    fn footprint(&self, uuid: Uuid) -> Option<&Footprint> {
        match self.project.and_then(|p| p.element(uuid)) {
            Some(LibraryElement::Footprint(f)) => Some(f),
            Some(_) => None,
            None => self.index.footprint(uuid),
        }
    }
}

impl Command for LibraryCheck {
    const META: Meta = Meta {
        id: "library.check",
        title: "Check Library",
        description: "Runs the Library check on one element, one Library or every registered \
                      Library, and returns its Violations: off-grid Pins, duplicate Pin \
                      numbers, Pins with no Pad in some alternate Footprint, Pads mapped to \
                      no Pin and not marked unconnected, missing courtyards and broken \
                      references. Open elements are checked with their unsaved changes. It \
                      never blocks saving.",
        category: "library",
        icon: "rule",
        kind: Kind::File,
    };
    type Params = LibraryCheckParams;
    type Output = LibraryCheckResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: LibraryCheckParams,
    ) -> Result<LibraryCheckResult, CommandError> {
        let session = cx.session()?;
        let index = scan_registered(session);
        let overlay = Overlay {
            index: &index,
            project: session.elements(),
        };
        let elements = match (p.element, p.library) {
            (Some(_), Some(_)) => {
                return Err(CommandError::invalid_params(
                    "/library",
                    "give `element` or `library`, not both",
                ));
            }
            (Some(uuid), None) => vec![overlay.element(uuid).ok_or_else(|| {
                CommandError::not_found(format!(
                    "no open element or element of a registered Library has UUID {uuid}"
                ))
            })?],
            (None, Some(uuid)) => {
                let open = overlay
                    .project
                    .is_some_and(|p| p.elements().any(|(s, _)| s.library == uuid));
                if index.library(uuid).is_none() && !open {
                    return Err(CommandError::not_found(format!(
                        "no registered Library has UUID {uuid}"
                    )));
                }
                overlay.library_elements(uuid)
            }
            (None, None) => {
                let libraries: Vec<Uuid> = index.libraries().map(|l| l.info.uuid).collect();
                libraries
                    .into_iter()
                    .flat_map(|l| overlay.library_elements(l))
                    .collect()
            }
        };
        let violations = elements.iter().flat_map(|e| check(&overlay, e)).collect();
        Ok(LibraryCheckResult {
            violations,
            problems: index.problems().to_vec(),
        })
    }
}
