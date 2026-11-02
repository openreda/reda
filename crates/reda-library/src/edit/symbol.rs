//! The Symbol editor's Commands: the Symbol's own properties, its Units, Pins and graphics.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry,
    Unavailable,
};
use reda_model::geom::{Millideg, Nm};
use reda_model::{Pin, PinType, Symbol, SymbolGraphic, Unit, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    GraphicResult, PointParam, ShapeParam, check_element_name, edit_symbol, find_mut, invalid,
    remove,
};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(SymbolEdit);
    registry.add(UnitAdd);
    registry.add(UnitEdit);
    registry.add(UnitRemove);
    registry.add(PinAdd);
    registry.add(PinEdit);
    registry.add(PinRemove);
    registry.add(SymbolGraphicAdd);
    registry.add(SymbolGraphicEdit);
    registry.add(SymbolGraphicRemove);
    registry.add(SymbolMove);
    registry.add(SymbolDelete);
}

/// The default Pin length: 100 mil.
const PIN_LENGTH: Nm = Nm::from_mil(100);
/// The default stroke width of Symbol graphics: 10 mil.
const STROKE: Nm = Nm::from_mil(10);

const fn meta(id: &'static str, title: &'static str, description: &'static str) -> Meta {
    Meta {
        id,
        title,
        description,
        category: "library",
        icon: "symbol_edit",
        kind: Kind::Edit,
    }
}

pub struct SymbolEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolEditParams {
    /// The open Symbol.
    pub symbol: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    /// The Designator prefix of Components placed from it, such as `R` or `U`.
    pub designator_prefix: Option<String>,
}

impl Command for SymbolEdit {
    const META: Meta = meta(
        "library.symbol.edit",
        "Edit Symbol",
        "Changes an open Symbol's name, description or Designator prefix. Omitted fields \
         keep their value.",
    );
    type Params = SymbolEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: SymbolEditParams) -> Result<(), CommandError> {
        if let Some(name) = &p.name {
            check_element_name(name).map_err(invalid("/name"))?;
        }
        if p.designator_prefix.as_ref().is_some_and(|d| d.is_empty()) {
            return Err(CommandError::invalid_params(
                "/designator_prefix",
                "the Designator prefix is empty",
            ));
        }
        edit_symbol(cx, p.symbol, |s| {
            if let Some(name) = p.name {
                s.name = name;
            }
            if let Some(description) = p.description {
                s.description = description;
            }
            if let Some(prefix) = p.designator_prefix {
                s.designator_prefix = prefix;
            }
            Ok(())
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UnitResult {
    /// The new Unit's UUID.
    pub unit: Uuid,
    pub name: String,
}

pub struct UnitAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitAddParams {
    /// The open Symbol.
    pub symbol: Uuid,
    /// Omitted, the next free letter: B after A, and so on.
    pub name: Option<String>,
}

impl Command for UnitAdd {
    const META: Meta = meta(
        "library.symbol.unit.add",
        "Add Unit",
        "Adds a Unit after the Symbol's last one, named `name` or the next free letter.",
    );
    type Params = UnitAddParams;
    type Output = UnitResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: UnitAddParams) -> Result<UnitResult, CommandError> {
        let uuid = cx.new_id();
        edit_symbol(cx, p.symbol, |s| {
            let name = match p.name {
                Some(name) => {
                    check_unit_name(s, &name, None)?;
                    name
                }
                None => next_unit_name(s),
            };
            s.units.push(Unit {
                uuid,
                name: name.clone(),
            });
            Ok(UnitResult { unit: uuid, name })
        })
    }
}

/// The first of A…Z, then U27, U28…, that no Unit is called.
fn next_unit_name(symbol: &Symbol) -> String {
    let taken = |name: &str| symbol.units.iter().any(|u| u.name == name);
    (b'A'..=b'Z')
        .map(|c| char::from(c).to_string())
        .chain((27..).map(|n| format!("U{n}")))
        .find(|name| !taken(name))
        .expect("an unbounded sequence has a free name")
}

fn check_unit_name(symbol: &Symbol, name: &str, unit: Option<Uuid>) -> Result<(), CommandError> {
    if name.trim().is_empty() {
        return Err(CommandError::invalid_params(
            "/name",
            "the Unit name is empty",
        ));
    }
    if symbol
        .units
        .iter()
        .any(|u| u.name == name && Some(u.uuid) != unit)
    {
        return Err(CommandError::rejected(format!(
            "the Symbol already has a Unit called {name}"
        )));
    }
    Ok(())
}

pub struct UnitEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitEditParams {
    /// The open Symbol.
    pub symbol: Uuid,
    pub unit: Uuid,
    pub name: String,
}

impl Command for UnitEdit {
    const META: Meta = meta(
        "library.symbol.unit.edit",
        "Rename Unit",
        "Renames a Unit of an open Symbol. Unit names are unique within the Symbol.",
    );
    type Params = UnitEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: UnitEditParams) -> Result<(), CommandError> {
        edit_symbol(cx, p.symbol, |s| {
            check_unit_name(s, &p.name, Some(p.unit))?;
            find_mut(&mut s.units, p.unit, |u| u.uuid, "Unit")?.name = p.name;
            Ok(())
        })
    }
}

pub struct UnitRemove;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitParams {
    /// The open Symbol.
    pub symbol: Uuid,
    pub unit: Uuid,
}

impl Command for UnitRemove {
    const META: Meta = meta(
        "library.symbol.unit.remove",
        "Remove Unit",
        "Removes a Unit with its Pins and graphics. A Symbol keeps at least one Unit.",
    );
    type Params = UnitParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: UnitParams) -> Result<(), CommandError> {
        edit_symbol(cx, p.symbol, |s| {
            remove(&mut s.units, p.unit, |u| u.uuid, "Unit")?;
            if s.units.is_empty() {
                return Err(CommandError::rejected(
                    "a Symbol needs at least one Unit, so its last cannot be removed",
                ));
            }
            s.pins.retain(|pin| pin.unit != p.unit);
            s.graphics.retain(|g| g.unit != p.unit);
            Ok(())
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PinResult {
    /// The new Pin's UUID.
    pub pin: Uuid,
}

pub struct PinAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PinAddParams {
    /// The open Symbol.
    pub symbol: Uuid,
    /// The Unit it goes on. Omitted, the first.
    pub unit: Option<Uuid>,
    pub name: String,
    /// Unique across the Symbol.
    pub number: String,
    /// Omitted, `passive`.
    pub electrical_type: Option<PinType>,
    /// The connection end, relative to the Symbol origin. The editor keeps it on the
    /// 50 mil grid; the Library check reports Pins off it.
    pub at: PointParam,
    /// The direction from the connection end towards the body, counter-clockwise from +x.
    pub rotation: Option<Angle>,
    /// Omitted, 100 mil.
    pub length: Option<Length>,
    /// Omitted, shown.
    pub name_visible: Option<bool>,
    /// Omitted, shown.
    pub number_visible: Option<bool>,
}

impl Command for PinAdd {
    const META: Meta = meta(
        "library.symbol.pin.add",
        "Add Pin",
        "Adds a Pin to a Unit of an open Symbol. Its number must be unique across the \
         Symbol; a Pin connects to the Pads with the same number unless the Part says \
         otherwise.",
    );
    type Params = PinAddParams;
    type Output = PinResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: PinAddParams) -> Result<PinResult, CommandError> {
        let uuid = cx.new_id();
        edit_symbol(cx, p.symbol, |s| {
            let unit = match p.unit {
                Some(unit) => unit,
                None => s.units[0].uuid,
            };
            let pin = Pin {
                uuid,
                unit,
                name: p.name,
                number: p.number,
                electrical_type: p.electrical_type.unwrap_or(PinType::Passive),
                at: p.at.point(),
                rotation: p.rotation.map_or(Millideg(0), Angle::millideg),
                length: p.length.map_or(PIN_LENGTH, Length::nm),
                name_visible: p.name_visible.unwrap_or(true),
                number_visible: p.number_visible.unwrap_or(true),
            };
            check_pin(s, &pin)?;
            s.pins.push(pin);
            s.pins.sort_by_key(|pin| pin.uuid);
            Ok(PinResult { pin: uuid })
        })
    }
}

fn check_pin(symbol: &Symbol, pin: &Pin) -> Result<(), CommandError> {
    if !symbol.units.iter().any(|u| u.uuid == pin.unit) {
        return Err(CommandError::not_found(format!(
            "the Symbol has no Unit with UUID {}",
            pin.unit
        )));
    }
    if pin.number.trim().is_empty() {
        return Err(CommandError::invalid_params(
            "/number",
            "the Pin number is empty",
        ));
    }
    if pin.length.0 < 0 {
        return Err(CommandError::invalid_params(
            "/length",
            "the Pin length cannot be negative",
        ));
    }
    if let Some(other) = symbol
        .pins
        .iter()
        .find(|o| o.number == pin.number && o.uuid != pin.uuid)
    {
        return Err(CommandError::rejected(format!(
            "Pin {} ({}) already has number {}",
            other.name, other.uuid, pin.number
        )));
    }
    Ok(())
}

pub struct PinEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PinEditParams {
    /// The open Symbol.
    pub symbol: Uuid,
    pub pin: Uuid,
    /// Moves the Pin to another Unit.
    pub unit: Option<Uuid>,
    pub name: Option<String>,
    pub number: Option<String>,
    pub electrical_type: Option<PinType>,
    pub at: Option<PointParam>,
    pub rotation: Option<Angle>,
    pub length: Option<Length>,
    pub name_visible: Option<bool>,
    pub number_visible: Option<bool>,
}

impl Command for PinEdit {
    const META: Meta = meta(
        "library.symbol.pin.edit",
        "Edit Pin",
        "Changes any of a Pin's Unit, name, number, electrical type, position, rotation, \
         length and name and number visibility. Omitted fields keep their value.",
    );
    type Params = PinEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: PinEditParams) -> Result<(), CommandError> {
        edit_symbol(cx, p.symbol, |s| {
            let mut pin = find_mut(&mut s.pins, p.pin, |pin| pin.uuid, "Pin")?.clone();
            if let Some(unit) = p.unit {
                pin.unit = unit;
            }
            if let Some(name) = p.name {
                pin.name = name;
            }
            if let Some(number) = p.number {
                pin.number = number;
            }
            if let Some(electrical_type) = p.electrical_type {
                pin.electrical_type = electrical_type;
            }
            if let Some(at) = p.at {
                pin.at = at.point();
            }
            if let Some(rotation) = p.rotation {
                pin.rotation = rotation.millideg();
            }
            if let Some(length) = p.length {
                pin.length = length.nm();
            }
            if let Some(visible) = p.name_visible {
                pin.name_visible = visible;
            }
            if let Some(visible) = p.number_visible {
                pin.number_visible = visible;
            }
            check_pin(s, &pin)?;
            *find_mut(&mut s.pins, p.pin, |pin| pin.uuid, "Pin")? = pin;
            Ok(())
        })
    }
}

pub struct PinRemove;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PinParams {
    /// The open Symbol.
    pub symbol: Uuid,
    pub pin: Uuid,
}

impl Command for PinRemove {
    const META: Meta = meta(
        "library.symbol.pin.remove",
        "Remove Pin",
        "Removes a Pin from an open Symbol.",
    );
    type Params = PinParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: PinParams) -> Result<(), CommandError> {
        edit_symbol(cx, p.symbol, |s| {
            remove(&mut s.pins, p.pin, |pin| pin.uuid, "Pin").map(drop)
        })
    }
}

pub struct SymbolGraphicAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolGraphicAddParams {
    /// The open Symbol.
    pub symbol: Uuid,
    /// The Unit it is drawn on. Omitted, the first.
    pub unit: Option<Uuid>,
    pub shape: ShapeParam,
    /// The stroke width. Omitted, 10 mil.
    pub width: Option<Length>,
    /// Whether a closed shape is filled. Omitted, not.
    pub fill: Option<bool>,
}

impl Command for SymbolGraphicAdd {
    const META: Meta = meta(
        "library.symbol.graphic.add",
        "Add Symbol Graphic",
        "Draws a line, rectangle, arc, circle, polygon or text on a Unit of an open Symbol.",
    );
    type Params = SymbolGraphicAddParams;
    type Output = GraphicResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SymbolGraphicAddParams,
    ) -> Result<GraphicResult, CommandError> {
        let uuid = cx.new_id();
        let shape = p.shape.shape().map_err(invalid("/shape"))?;
        edit_symbol(cx, p.symbol, |s| {
            let graphic = SymbolGraphic {
                uuid,
                unit: p.unit.unwrap_or(s.units[0].uuid),
                shape,
                width: p.width.map_or(STROKE, Length::nm),
                fill: p.fill.unwrap_or(false),
            };
            check_graphic(s, &graphic)?;
            s.graphics.push(graphic);
            s.graphics.sort_by_key(|g| g.uuid);
            Ok(GraphicResult { graphic: uuid })
        })
    }
}

fn check_graphic(symbol: &Symbol, graphic: &SymbolGraphic) -> Result<(), CommandError> {
    if !symbol.units.iter().any(|u| u.uuid == graphic.unit) {
        return Err(CommandError::not_found(format!(
            "the Symbol has no Unit with UUID {}",
            graphic.unit
        )));
    }
    if graphic.width.0 < 0 {
        return Err(CommandError::invalid_params(
            "/width",
            "the stroke width cannot be negative",
        ));
    }
    Ok(())
}

pub struct SymbolGraphicEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolGraphicEditParams {
    /// The open Symbol.
    pub symbol: Uuid,
    pub graphic: Uuid,
    pub unit: Option<Uuid>,
    /// Replaces the whole shape.
    pub shape: Option<ShapeParam>,
    pub width: Option<Length>,
    pub fill: Option<bool>,
}

impl Command for SymbolGraphicEdit {
    const META: Meta = meta(
        "library.symbol.graphic.edit",
        "Edit Symbol Graphic",
        "Changes a Symbol graphic's Unit, shape, stroke width or fill. Omitted fields keep \
         their value.",
    );
    type Params = SymbolGraphicEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: SymbolGraphicEditParams) -> Result<(), CommandError> {
        let shape = p
            .shape
            .map(ShapeParam::shape)
            .transpose()
            .map_err(invalid("/shape"))?;
        edit_symbol(cx, p.symbol, |s| {
            let mut graphic = find_mut(&mut s.graphics, p.graphic, |g| g.uuid, "graphic")?.clone();
            if let Some(unit) = p.unit {
                graphic.unit = unit;
            }
            if let Some(shape) = shape {
                graphic.shape = shape;
            }
            if let Some(width) = p.width {
                graphic.width = width.nm();
            }
            if let Some(fill) = p.fill {
                graphic.fill = fill;
            }
            check_graphic(s, &graphic)?;
            *find_mut(&mut s.graphics, p.graphic, |g| g.uuid, "graphic")? = graphic;
            Ok(())
        })
    }
}

pub struct SymbolGraphicRemove;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolGraphicParams {
    /// The open Symbol.
    pub symbol: Uuid,
    pub graphic: Uuid,
}

impl Command for SymbolGraphicRemove {
    const META: Meta = meta(
        "library.symbol.graphic.remove",
        "Remove Symbol Graphic",
        "Removes a graphic from an open Symbol.",
    );
    type Params = SymbolGraphicParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: SymbolGraphicParams) -> Result<(), CommandError> {
        edit_symbol(cx, p.symbol, |s| {
            remove(&mut s.graphics, p.graphic, |g| g.uuid, "graphic").map(drop)
        })
    }
}

pub struct SymbolMove;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolMoveParams {
    /// The open Symbol.
    pub symbol: Uuid,
    /// Pins and graphics by UUID.
    pub objects: Vec<Uuid>,
    pub by: PointParam,
}

impl Command for SymbolMove {
    const META: Meta = meta(
        "library.symbol.move",
        "Move in Symbol",
        "Moves any mix of an open Symbol's Pins and graphics by `by`, in one step.",
    );
    type Params = SymbolMoveParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: SymbolMoveParams) -> Result<(), CommandError> {
        let by = p.by.point();
        edit_symbol(cx, p.symbol, |s| {
            for (i, uuid) in p.objects.iter().enumerate() {
                if let Some(pin) = s.pins.iter_mut().find(|pin| pin.uuid == *uuid) {
                    pin.at = pin.at + by;
                } else if let Some(g) = s.graphics.iter_mut().find(|g| g.uuid == *uuid) {
                    g.shape = g.shape.translated(by);
                } else {
                    return Err(not_in_symbol(i, *uuid));
                }
            }
            Ok(())
        })
    }
}

fn not_in_symbol(index: usize, uuid: Uuid) -> CommandError {
    CommandError::not_found(format!(
        "objects/{index}: the Symbol has no Pin or graphic {uuid}"
    ))
}

pub struct SymbolDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolDeleteParams {
    /// The open Symbol.
    pub symbol: Uuid,
    /// Pins and graphics by UUID.
    pub objects: Vec<Uuid>,
}

impl Command for SymbolDelete {
    const META: Meta = meta(
        "library.symbol.delete",
        "Delete in Symbol",
        "Removes any mix of an open Symbol's Pins and graphics in one step.",
    );
    type Params = SymbolDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: SymbolDeleteParams) -> Result<(), CommandError> {
        edit_symbol(cx, p.symbol, |s| {
            for (i, uuid) in p.objects.iter().enumerate() {
                if s.pins.iter().any(|pin| pin.uuid == *uuid) {
                    s.pins.retain(|pin| pin.uuid != *uuid);
                } else if s.graphics.iter().any(|g| g.uuid == *uuid) {
                    s.graphics.retain(|g| g.uuid != *uuid);
                } else {
                    return Err(not_in_symbol(i, *uuid));
                }
            }
            Ok(())
        })
    }
}
