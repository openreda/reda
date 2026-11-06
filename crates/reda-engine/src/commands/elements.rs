//! `library.element.new`, `.open`, `.save` and `.close`: the file side of the Library
//! editors. An open element lives in the Project on its own undo stack
//! (`Scope::Element`), where `reda-library`'s edit Commands change it, until it is saved
//! to its file in its Library and closed.

use std::path::PathBuf;

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_ELEMENTS, Registry,
    Session, Unavailable,
};
use reda_library::{ElementKind, LibraryIndex, Origin, check_element_name};
use reda_model::{
    ComponentType, ElementSource, Fields, Footprint, FootprintText, LibraryElement, Part, PinMap,
    Project, ProjectProps, Scope, Symbol, Unit, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::format::library::{write_footprint, write_part, write_symbol};
use crate::fs::write_atomic;
use crate::library::scan_registered;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ElementNew);
    registry.add(ElementOpen);
    registry.add(ElementSave);
    registry.add(ElementClose);
}

/// What the element Commands return about one open element.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ElementSummary {
    /// The element's UUID, which also names its undo stack.
    pub element: Uuid,
    pub kind: ElementKind,
    pub name: String,
    /// The Library it belongs to.
    pub library: Uuid,
    /// Elements of the starter Library cannot be edited.
    pub read_only: bool,
}

fn summary(source: ElementSource, element: &LibraryElement) -> ElementSummary {
    ElementSummary {
        element: element.uuid(),
        kind: ElementKind::of(element),
        name: element.name().to_owned(),
        library: source.library,
        read_only: source.read_only,
    }
}

/// Where to open an element: the open Project, else the workbench, started blank when
/// no element is open yet.
pub(crate) fn holder(session: &mut Session) -> &mut Project {
    if session.project.is_none() && session.workbench.is_none() {
        session.workbench = Some(Project::new(ProjectProps {
            uuid: Uuid::NIL,
            name: String::new(),
            documents: vec![],
            libraries: vec![],
            properties: Default::default(),
            erc: Default::default(),
            schematic: Default::default(),
        }));
    }
    session.elements_mut().expect("a Project or the workbench")
}

fn need_elements(cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
    cx.elements()
}

fn elements_mut(session: &mut Session) -> Result<&mut Project, CommandError> {
    session
        .elements_mut()
        .ok_or_else(|| CommandError::unavailable(NO_ELEMENTS))
}

pub struct ElementNew;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementNewParams {
    /// The registered Library to create it in.
    pub library: Uuid,
    pub kind: ElementKind,
    /// The element's name, which is also its file stem.
    pub name: String,
}

impl Command for ElementNew {
    const META: Meta = Meta {
        id: "library.element.new",
        title: "New Library Element",
        description: "Creates a Symbol (with one Unit, A), a Footprint or a Part called \
                      `name` in a registered Library and opens it for editing on its own \
                      undo stack. Nothing is written until `library.element.save`. The \
                      starter Library is read-only.",
        category: "library",
        icon: "library_element_new",
        kind: Kind::File,
    };
    type Params = ElementNewParams;
    type Output = ElementSummary;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ElementNewParams,
    ) -> Result<ElementSummary, CommandError> {
        check_element_name(&p.name).map_err(|why| CommandError::invalid_params("/name", why))?;
        // A Symbol's second UUID is its Unit's; a Footprint's second and third are its
        // Designator and value texts'.
        let (uuid, second) = (cx.new_id(), cx.new_id());
        let third = (p.kind == ElementKind::Footprint).then(|| cx.new_id());
        let session = cx.session()?;
        let index = scan_registered(session);
        let library = index.library(p.library).ok_or_else(|| {
            CommandError::not_found(format!("no registered Library has UUID {}", p.library))
        })?;
        if library.origin == Origin::Starter {
            return Err(CommandError::rejected(
                "the starter Library is read-only; create the element in your own Library",
            ));
        }
        let path =
            library
                .root
                .join(p.kind.folder())
                .join(format!("{}.{}", p.name, p.kind.extension()));
        if path.exists() {
            return Err(CommandError::rejected(format!(
                "{} already exists",
                path.display()
            )));
        }
        let element = new_element(p.kind, uuid, (second, third), p.name);
        let source = ElementSource {
            library: p.library,
            read_only: false,
        };
        let out = summary(source, &element);
        holder(session)
            .open_element(source, element)
            .map_err(CommandError::conflict)?;
        session.history.forget(Scope::Element(uuid));
        session.history.mark_unsaved(Scope::Element(uuid));
        Ok(out)
    }
}

/// A new, empty element of `kind`: a Symbol with one Unit (UUID `ids.0`), or a Footprint
/// with a Designator and a value text (UUIDs `ids.0` and `ids.1`).
fn new_element(
    kind: ElementKind,
    uuid: Uuid,
    ids: (Uuid, Option<Uuid>),
    name: String,
) -> LibraryElement {
    match kind {
        ElementKind::Symbol => LibraryElement::Symbol(Symbol {
            uuid,
            name,
            description: String::new(),
            designator_prefix: "U".into(),
            units: vec![Unit {
                uuid: ids.0,
                name: "A".into(),
            }],
            pins: vec![],
            graphics: vec![],
            provenance: None,
        }),
        ElementKind::Footprint => {
            let mut texts = vec![
                FootprintText::designator(ids.0),
                FootprintText::value(ids.1.unwrap_or(Uuid::NIL)),
            ];
            texts.sort_by_key(|t| t.uuid);
            LibraryElement::Footprint(Footprint {
                uuid,
                name,
                description: String::new(),
                height: None,
                pads: vec![],
                graphics: vec![],
                texts,
                generator: None,
                provenance: None,
            })
        }
        ElementKind::Part => LibraryElement::Part(Part {
            uuid,
            name,
            component_type: ComponentType::Standard,
            in_bom: ComponentType::Standard.default_in_bom(),
            in_cpl: ComponentType::Standard.default_in_cpl(),
            symbol: None,
            footprints: vec![],
            fields: Fields::default(),
            pin_map: PinMap::default(),
            provenance: None,
        }),
    }
}

/// Params naming one element.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementParams {
    /// The element's UUID.
    pub element: Uuid,
}

pub struct ElementOpen;

impl Command for ElementOpen {
    const META: Meta = Meta {
        id: "library.element.open",
        title: "Edit in Library",
        description: "Opens a Symbol, Footprint or Part of a registered Library for editing \
                      on its own undo stack, keyed by its UUID. Starter Library elements \
                      open read-only. Opening an element that is already open returns it \
                      unchanged. No Project needs to be open; elements open then stay open \
                      when a Project opens.",
        category: "library",
        icon: "library_element_open",
        kind: Kind::File,
    };
    type Params = ElementParams;
    type Output = ElementSummary;

    fn run(&self, cx: &mut Context<'_>, p: ElementParams) -> Result<ElementSummary, CommandError> {
        let session = cx.session()?;
        if let Some(project) = session.elements()
            && let (Some(source), Some(element)) = (
                project.element_source(p.element),
                project.element(p.element),
            )
        {
            return Ok(summary(source, element));
        }
        let index = scan_registered(session);
        let (Some(library), Some(element)) =
            (index.library_of(p.element), index.element(p.element))
        else {
            return Err(CommandError::not_found(format!(
                "no registered Library has an element with UUID {}",
                p.element
            )));
        };
        let source = ElementSource {
            library: library.info.uuid,
            read_only: library.origin == Origin::Starter,
        };
        let out = summary(source, &element);
        holder(session)
            .open_element(source, element)
            .map_err(CommandError::conflict)?;
        session.history.forget(Scope::Element(p.element));
        Ok(out)
    }
}

pub struct ElementSave;

#[derive(Deserialize, JsonSchema, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ElementSaveParams {
    /// Saves only this open element. Omitted, every open element that is not read-only
    /// is saved.
    pub element: Option<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ElementSaveResult {
    /// Every file written.
    pub written: Vec<PathBuf>,
}

impl Command for ElementSave {
    const META: Meta = Meta {
        id: "library.element.save",
        title: "Save Library Element",
        description: "Writes an open Library element to its file in its Library (through a \
                      temp file and a rename), or with no `element` every open element that \
                      is not read-only. A new element's file is `<name>.<extension>` in the \
                      Library's `symbols/`, `footprints/` or `parts/` folder.",
        category: "library",
        icon: "save",
        kind: Kind::File,
    };
    type Params = ElementSaveParams;
    type Output = ElementSaveResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        need_elements(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ElementSaveParams,
    ) -> Result<ElementSaveResult, CommandError> {
        let session = cx.session()?;
        let project = session
            .elements()
            .ok_or_else(|| CommandError::unavailable(NO_ELEMENTS))?;
        let wanted: Vec<(ElementSource, LibraryElement)> = match p.element {
            Some(uuid) => {
                let (Some(source), Some(element)) =
                    (project.element_source(uuid), project.element(uuid))
                else {
                    return Err(CommandError::not_found(format!(
                        "no open Library element has UUID {uuid}"
                    )));
                };
                if source.read_only {
                    return Err(CommandError::rejected(format!(
                        "{} is in the starter Library, which is read-only",
                        element.name()
                    )));
                }
                vec![(source, element.clone())]
            }
            None => project
                .elements()
                .filter(|(source, _)| !source.read_only)
                .map(|(source, element)| (source, element.clone()))
                .collect(),
        };
        let index = scan_registered(session);
        let mut written = vec![];
        for (source, element) in &wanted {
            let path = element_path(&index, *source, element)?;
            let bytes = match element {
                LibraryElement::Symbol(s) => write_symbol(s),
                LibraryElement::Footprint(f) => write_footprint(f),
                LibraryElement::Part(p) => write_part(p),
            }
            .map_err(|e| CommandError::internal(e.to_string()))?;
            let io = |e: std::io::Error| {
                CommandError::internal(format!("writing {}: {e}", path.display()))
            };
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(io)?;
            }
            write_atomic(&path, &bytes).map_err(io)?;
            session.history.mark_saved(Scope::Element(element.uuid()));
            written.push(path);
        }
        Ok(ElementSaveResult { written })
    }
}

/// The file an element saves to: the one it was read from, or for a new element
/// `<library>/<folder>/<name>.<extension>`, which must not belong to another element.
fn element_path(
    index: &LibraryIndex,
    source: ElementSource,
    element: &LibraryElement,
) -> Result<PathBuf, CommandError> {
    let uuid = element.uuid();
    if index.library_of(uuid).map(|l| l.info.uuid) == Some(source.library)
        && let Some(file) = index.file_of(uuid)
    {
        return Ok(file.to_owned());
    }
    let library = index.library(source.library).ok_or_else(|| {
        CommandError::not_found(format!(
            "Library {} of {} is no longer registered",
            source.library,
            element.name()
        ))
    })?;
    let kind = ElementKind::of(element);
    let path =
        library
            .root
            .join(kind.folder())
            .join(format!("{}.{}", element.name(), kind.extension()));
    if path.exists() {
        return Err(CommandError::rejected(format!(
            "{} already exists and holds another element",
            path.display()
        )));
    }
    Ok(path)
}

pub struct ElementClose;

impl Command for ElementClose {
    const META: Meta = Meta {
        id: "library.element.close",
        title: "Close Library Element",
        description: "Closes an open Library element and forgets its undo stack. Unsaved \
                      changes are discarded; its file is left as it is.",
        category: "library",
        icon: "close",
        kind: Kind::File,
    };
    type Params = ElementParams;
    type Output = ElementSummary;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        need_elements(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ElementParams) -> Result<ElementSummary, CommandError> {
        let session = cx.session()?;
        let project = elements_mut(session)?;
        let source = project.element_source(p.element).ok_or_else(|| {
            CommandError::not_found(format!("no open Library element has UUID {}", p.element))
        })?;
        let element = project
            .close_element(p.element)
            .expect("the element is open");
        session.history.forget(Scope::Element(p.element));
        // The workbench goes with its last element.
        if session
            .workbench
            .as_ref()
            .is_some_and(|w| w.elements().next().is_none())
        {
            session.workbench = None;
        }
        Ok(summary(source, &element))
    }
}
