//! The Symbol Wizard's Commands: `library.symbol.generate`, which lays Pins out in one of
//! six layouts with a rectangular body onto a new Symbol, a new Unit of an open Symbol or
//! an existing Unit (reopening), and `library.symbol.generate.read`, which reads a Unit's
//! Pins back as the wizard's input. The layout itself is `reda_library::wizard`.
//!
//! A new Symbol is opened unsaved like `library.element.new`, so the Command is a file
//! Command; on an open Symbol it records one undo step on that Symbol's stack.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Session,
    Unavailable,
};
use reda_library::ElementKind;
use reda_library::wizard::{self, ApplyError, Layout, WizardInput, WizardPin};
use reda_model::{LibraryElement, Object, Symbol, Unit, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::elements::{new_element, new_element_source, open_new_element};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(SymbolGenerate);
    registry.add(SymbolGenerateRead);
}

/// What the wizard makes.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WizardTarget {
    /// A new Symbol with one Unit, A, in a registered Library, opened unsaved for editing
    /// like `library.element.new`.
    NewSymbol {
        library: Uuid,
        /// Its name, which is also its file stem.
        name: String,
        /// Omitted, `J` for the `connector` and `zig_zag` layouts and `U` for the rest.
        designator_prefix: Option<String>,
    },
    /// A new Unit after the last one of an open Symbol.
    NewUnit {
        symbol: Uuid,
        /// Omitted, the next free letter.
        name: Option<String>,
    },
    /// An existing Unit of an open Symbol, when the wizard reopens: its Pins become the
    /// rows, keeping UUIDs by number, and its graphics become the new body.
    Unit { symbol: Uuid, unit: Uuid },
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolGenerateParams {
    pub layout: Layout,
    /// The pin grid's rows, in order. Numbers are unique across the Symbol.
    pub pins: Vec<WizardPin>,
    pub target: WizardTarget,
    /// Omitted, 100 mil.
    pub pin_length: Option<Length>,
}

/// What the wizard made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Generated {
    pub symbol: Uuid,
    pub unit: Uuid,
    /// The Unit's name, such as `A`.
    pub unit_name: String,
    /// Each row's Pin UUID, in row order.
    pub pins: Vec<Uuid>,
}

pub struct SymbolGenerate;

impl Command for SymbolGenerate {
    const META: Meta = Meta {
        id: "library.symbol.generate",
        title: "Symbol Wizard",
        description: "Lays the rows `pins` out in a `layout` (dil, quad, connector, zig_zag, \
                      single_row or manual) with a rectangular body on the 50 mil grid and \
                      Pin 1 at the origin, onto a new Symbol (opened unsaved), a new Unit of \
                      an open Symbol, or an existing Unit, whose Pins keep their UUIDs by \
                      number and whose graphics become the new body. Reopening reads the \
                      rows with `library.symbol.generate.read`.",
        category: "library",
        icon: "symbol_edit",
        kind: Kind::File,
    };
    type Params = SymbolGenerateParams;
    type Output = Generated;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SymbolGenerateParams,
    ) -> Result<Generated, CommandError> {
        let plan = wizard::plan(p.layout, &p.pins, p.pin_length.map(Length::nm))
            .map_err(|e| CommandError::invalid_params(&e.pointer, e.message))?;
        // Every UUID the Command could need, taken before the session is borrowed: the
        // Symbol and its Unit, one per row and the body.
        let mut ids: Vec<Uuid> = (0..p.pins.len() + 3).map(|_| cx.new_id()).collect();
        ids.reverse();
        let mut new_id = || ids.pop().expect("enough UUIDs were taken");
        let session = cx.session()?;
        let (symbol, unit) = match p.target {
            WizardTarget::NewSymbol {
                library,
                name,
                designator_prefix,
            } => {
                reda_library::check_element_name(&name)
                    .map_err(|why| CommandError::invalid_params("/target/name", why))?;
                if designator_prefix
                    .as_ref()
                    .is_some_and(|d| d.trim().is_empty())
                {
                    return Err(CommandError::invalid_params(
                        "/target/designator_prefix",
                        "the Designator prefix is empty",
                    ));
                }
                let source = new_element_source(session, library, ElementKind::Symbol, &name)?;
                let (uuid, unit) = (new_id(), new_id());
                let LibraryElement::Symbol(mut symbol) =
                    new_element(ElementKind::Symbol, uuid, (unit, None), name)
                else {
                    unreachable!("a Symbol was asked for")
                };
                symbol.designator_prefix = designator_prefix.unwrap_or_else(|| {
                    match p.layout {
                        Layout::Connector | Layout::ZigZag => "J",
                        _ => "U",
                    }
                    .into()
                });
                applied(&mut symbol, unit, &plan, &mut new_id)?;
                open_new_element(session, source, LibraryElement::Symbol(symbol.clone()))?;
                (symbol, unit)
            }
            WizardTarget::NewUnit { symbol, name } => {
                let mut s = open_symbol(session, symbol)?;
                let name = match name {
                    Some(name) if name.trim().is_empty() => {
                        return Err(CommandError::invalid_params(
                            "/target/name",
                            "the Unit name is empty",
                        ));
                    }
                    Some(name) if s.units.iter().any(|u| u.name == name) => {
                        return Err(CommandError::rejected(format!(
                            "the Symbol already has a Unit called {name}"
                        )));
                    }
                    Some(name) => name,
                    None => next_unit_name(&s),
                };
                let unit = new_id();
                s.units.push(Unit { uuid: unit, name });
                applied(&mut s, unit, &plan, &mut new_id)?;
                record(session, &s)?;
                (s, unit)
            }
            WizardTarget::Unit { symbol, unit } => {
                let mut s = open_symbol(session, symbol)?;
                applied(&mut s, unit, &plan, &mut new_id)?;
                record(session, &s)?;
                (s, unit)
            }
        };
        let pins = plan
            .pins
            .iter()
            .map(|planned| {
                symbol
                    .pins
                    .iter()
                    .find(|pin| pin.number == planned.number)
                    .expect("every row became a Pin")
                    .uuid
            })
            .collect();
        let unit_name = symbol
            .units
            .iter()
            .find(|u| u.uuid == unit)
            .map(|u| u.name.clone())
            .unwrap_or_default();
        Ok(Generated {
            symbol: symbol.uuid,
            unit,
            unit_name,
            pins,
        })
    }
}

fn applied(
    symbol: &mut Symbol,
    unit: Uuid,
    plan: &wizard::SymbolPlan,
    new_id: &mut dyn FnMut() -> Uuid,
) -> Result<(), CommandError> {
    wizard::apply(symbol, unit, plan, new_id).map_err(|e| match e {
        ApplyError::NoUnit(unit) => {
            CommandError::not_found(format!("the Symbol has no Unit with UUID {unit}"))
        }
        ApplyError::NumberTaken { number, unit } => {
            CommandError::rejected(format!("Unit {unit} already has a Pin numbered {number}"))
        }
    })
}

/// A copy of the open, editable Symbol `uuid`.
fn open_symbol(session: &Session, uuid: Uuid) -> Result<Symbol, CommandError> {
    let not_open = || {
        CommandError::not_found(format!(
            "no open Symbol has UUID {uuid}; open it with library.element.open"
        ))
    };
    let project = session.elements().ok_or_else(not_open)?;
    let (Some(source), Some(LibraryElement::Symbol(symbol))) =
        (project.element_source(uuid), project.element(uuid))
    else {
        return Err(not_open());
    };
    if source.read_only {
        return Err(CommandError::rejected(format!(
            "{} is in the starter Library, which is read-only; copy it into your own \
             Library to change it",
            symbol.name
        )));
    }
    Ok(symbol.clone())
}

/// Replaces the open Symbol with `symbol` as one undo step on its stack.
fn record(session: &mut Session, symbol: &Symbol) -> Result<(), CommandError> {
    let project = session
        .elements_mut()
        .ok_or_else(|| CommandError::not_found("the Symbol is no longer open"))?;
    let mut tx = project.transaction();
    tx.replace(Object::from(LibraryElement::Symbol(symbol.clone())))?;
    let changes = tx.commit()?;
    session.history.record(SymbolGenerate::META.title, &changes);
    Ok(())
}

/// The first of A…Z, then U27, U28…, that no Unit is called (as `library.symbol.unit.add`
/// names one).
fn next_unit_name(symbol: &Symbol) -> String {
    let taken = |name: &str| symbol.units.iter().any(|u| u.name == name);
    (b'A'..=b'Z')
        .map(|c| char::from(c).to_string())
        .chain((27..).map(|n| format!("U{n}")))
        .find(|name| !taken(name))
        .expect("an unbounded sequence has a free name")
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolGenerateReadParams {
    /// The open Symbol.
    pub symbol: Uuid,
    /// Omitted, its first Unit.
    pub unit: Option<Uuid>,
}

pub struct SymbolGenerateRead;

impl Command for SymbolGenerateRead {
    const META: Meta = Meta {
        id: "library.symbol.generate.read",
        title: "Read Symbol Wizard Rows",
        description: "Reads a Unit's Pins back as `library.symbol.generate` params, for \
                      reopening the wizard: the `manual` layout with each Pin's side, in \
                      order down the left, along the bottom, down the right and along the \
                      top, groups where a side has gaps, and the first Pin's length.",
        category: "library",
        icon: "symbol_edit",
        kind: Kind::Query,
    };
    type Params = SymbolGenerateReadParams;
    type Output = WizardInput;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SymbolGenerateReadParams,
    ) -> Result<WizardInput, CommandError> {
        let project = cx.project()?;
        let Some(LibraryElement::Symbol(symbol)) = project.element(p.symbol) else {
            return Err(CommandError::not_found(format!(
                "no open Symbol has UUID {}",
                p.symbol
            )));
        };
        let unit = p.unit.unwrap_or(symbol.units[0].uuid);
        if !symbol.units.iter().any(|u| u.uuid == unit) {
            return Err(CommandError::not_found(format!(
                "the Symbol has no Unit with UUID {unit}"
            )));
        }
        Ok(wizard::read_unit(symbol, unit))
    }
}
