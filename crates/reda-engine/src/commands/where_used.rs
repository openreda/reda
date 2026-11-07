//! `library.where_used`: the Parts across the registered Libraries that use a Symbol or
//! Footprint, and the Components of the open Project that use any element. It backs the
//! Library panel's Where Used, "Used in Project: N" and the shared-element dialog.

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_library::{ElementKind, components_using, project_element};
use reda_model::Uuid;
use reda_schematic::natural_cmp;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::library::{LibrarySummary, summary};
use crate::library::scan_registered;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(WhereUsed);
}

pub struct WhereUsed;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WhereUsedParams {
    /// The Symbol, Footprint or Part, by UUID.
    pub element: Uuid,
}

/// A Library Part that uses the element.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UsingPart {
    pub uuid: Uuid,
    pub name: String,
    pub library: LibrarySummary,
}

/// A Component of the open Project that uses the element.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UsingComponent {
    pub component: Uuid,
    pub designator: String,
    /// The sheet its Units are on.
    pub sheet: Uuid,
    /// The Library Part it was placed from.
    pub part: Uuid,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WhereUsedResult {
    pub kind: ElementKind,
    pub name: String,
    /// The Library that holds the element; null when no active registered Library does
    /// and only the Project's part tables still hold a copy.
    pub library: Option<LibrarySummary>,
    /// The Parts across the registered Libraries that use a Symbol or Footprint, sorted by
    /// name. Always empty for a Part.
    pub parts: Vec<UsingPart>,
    /// The Components of the open Project that use the element, by Designator.
    pub components: Vec<UsingComponent>,
    /// How many Components that is: "Used in Project: N".
    pub used_in_project: usize,
}

impl Command for WhereUsed {
    const META: Meta = Meta {
        id: "library.where_used",
        title: "Where Used",
        description: "Lists the Parts across the registered Libraries that use a Symbol or \
                      Footprint, and for any element the Components of the open Project \
                      that use it, found by scanning the part tables.",
        category: "library",
        icon: "where_used",
        kind: Kind::File,
    };
    type Params = WhereUsedParams;
    type Output = WhereUsedResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: WhereUsedParams,
    ) -> Result<WhereUsedResult, CommandError> {
        let session = cx.session()?;
        let index = scan_registered(session);
        let project = session.project.as_ref();
        let in_library = index.element(p.element).map(|e| {
            let library = index.library_of(p.element).map(summary);
            (ElementKind::of(&e), e.name().to_owned(), library)
        });
        let in_project = || {
            project
                .and_then(|project| project_element(project, p.element))
                .map(|(kind, name)| (kind, name, None))
        };
        let (kind, name, library) = in_library.or_else(in_project).ok_or_else(|| {
            CommandError::not_found(format!(
                "no registered Library or part table holds element {}",
                p.element
            ))
        })?;
        let parts = index
            .parts_using(p.element)
            .into_iter()
            .map(|hit| UsingPart {
                uuid: hit.part.uuid,
                name: hit.part.name.clone(),
                library: summary(hit.library),
            })
            .collect();
        let mut components: Vec<UsingComponent> = project
            .map(|project| components_using(project, p.element))
            .unwrap_or_default()
            .into_iter()
            .map(|c| UsingComponent {
                component: c.component,
                designator: c.designator,
                sheet: c.sheet,
                part: c.part,
            })
            .collect();
        components.sort_by(|a, b| {
            natural_cmp(&a.designator, &b.designator).then(a.component.cmp(&b.component))
        });
        Ok(WhereUsedResult {
            kind,
            name,
            library,
            parts,
            used_in_project: components.len(),
            components,
        })
    }
}
