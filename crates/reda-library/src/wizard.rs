//! The Symbol Wizard's pure side, which `library.symbol.generate` and the wizard dialog's
//! live preview share: laying a list of Pins out in one of six layouts with a
//! rectangular body ([`plan`]), applying that to a Unit of a Symbol ([`apply`]), and
//! reading a Unit's Pins back as the wizard's input ([`read_unit`]) when it reopens.
//! Column-mapping paste of Pin rows is in [`paste`].
//!
//! The wizard stores no parameters: the Pins are its whole input, so reopening reads
//! them, and applying keeps each Pin's UUID by its number.
//!
//! Pins sit 100 mil apart on the 50 mil Pin grid and point into the body; the body is a
//! rectangle on the grid with one empty pitch at each end of every side, widened for the
//! Pin names. Pin 1 (or the first row, with no Pin 1) ends at the origin, as
//! `docs/library-style.md` asks of a rectangular body.

pub mod paste;

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{Pin, PinType, Shape, Symbol, SymbolGraphic, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The distance between neighbouring Pins: 100 mil.
pub const PITCH: Nm = Nm::from_mil(100);
/// The Pin length when none is given: 100 mil.
pub const PIN_LENGTH: Nm = Nm::from_mil(100);
/// The body outline's stroke: 10 mil.
const STROKE: Nm = Nm::from_mil(10);
/// How much room one letter of a Pin name takes when sizing the body: a little over
/// the width of a 50 mil letter.
const LETTER: Nm = Nm::from_mil(40);
/// The gap between a Pin's body end and its name.
const NAME_GAP: Nm = Nm::from_mil(20);

/// How the wizard lays Pins out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// Dual in-line: the first half down the left, the rest back up the right, as the
    /// package numbers them. An odd count puts the extra Pin on the left.
    Dil,
    /// Four sides counter-clockwise from the top of the left: down the left, along the
    /// bottom, up the right, back along the top. Earlier sides take any remainder.
    Quad,
    /// A two-row connector: odd rows on the left, each beside the even row after it.
    Connector,
    /// A two-row connector drawn staggered: sides alternate and each Pin is one pitch
    /// below the one before.
    ZigZag,
    /// Every Pin down the left.
    SingleRow,
    /// Each Pin on its row's `side`; left and right top to bottom, top and bottom left
    /// to right, in row order.
    Manual,
}

/// A side of the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    /// The direction from a Pin's connection end into the body on this side.
    fn rotation(self) -> Millideg {
        Millideg(match self {
            Side::Left => 0,
            Side::Bottom => 90_000,
            Side::Right => 180_000,
            Side::Top => 270_000,
        })
    }

    /// The side a Pin pointing `rotation` into the body is on, to the nearest quarter
    /// turn.
    fn of(rotation: Millideg) -> Side {
        match (rotation.0.rem_euclid(360_000) + 45_000) / 90_000 {
            1 => Side::Bottom,
            2 => Side::Right,
            3 => Side::Top,
            _ => Side::Left,
        }
    }
}

/// One row of the wizard's pin grid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WizardPin {
    /// Unique across the Symbol.
    pub number: String,
    #[serde(default)]
    pub name: String,
    /// Omitted, `passive`.
    #[serde(default = "passive")]
    pub electrical_type: PinType,
    /// The side the `manual` layout puts it on; the other layouts choose the side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    /// A change of group between neighbouring Pins on a side leaves one empty pitch.
    /// The `connector` and `zig_zag` layouts ignore it, since their rows pair up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

fn passive() -> PinType {
    PinType::Passive
}

impl WizardPin {
    /// A passive Pin with no side or group.
    pub fn new(number: impl Into<String>, name: impl Into<String>) -> WizardPin {
        WizardPin {
            number: number.into(),
            name: name.into(),
            electrical_type: PinType::Passive,
            side: None,
            group: None,
        }
    }
}

/// A Pin as a layout places it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PlannedPin {
    pub number: String,
    pub name: String,
    pub electrical_type: PinType,
    pub side: Side,
    /// The connection end, relative to the Symbol origin.
    pub at: Point,
    /// The direction from the connection end into the body.
    pub rotation: Millideg,
    pub length: Nm,
}

/// What a layout makes: the Pins in row order, and the body's opposite corners.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SymbolPlan {
    pub pins: Vec<PlannedPin>,
    pub body: (Point, Point),
}

/// Why rows cannot be laid out, with the JSON pointer of the param at fault (relative to
/// the Command's params).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanError {
    pub pointer: String,
    pub message: String,
}

impl PlanError {
    fn new(pointer: impl Into<String>, message: impl Into<String>) -> PlanError {
        PlanError {
            pointer: pointer.into(),
            message: message.into(),
        }
    }
}

/// Each side's slots in reading order (left and right top to bottom, top and bottom left
/// to right): the index of the row in it, or `None` for an empty pitch.
#[derive(Default)]
struct Sides {
    left: Vec<Option<usize>>,
    right: Vec<Option<usize>>,
    top: Vec<Option<usize>>,
    bottom: Vec<Option<usize>>,
}

/// Lays `pins` out in `layout`, with Pins `pin_length` long (omitted, 100 mil).
pub fn plan(
    layout: Layout,
    pins: &[WizardPin],
    pin_length: Option<Nm>,
) -> Result<SymbolPlan, PlanError> {
    let length = pin_length.unwrap_or(PIN_LENGTH);
    if length.0 < 0 {
        return Err(PlanError::new(
            "/pin_length",
            "the Pin length cannot be negative",
        ));
    }
    check_rows(pins)?;
    let sides = sides(layout, pins)?;

    let name_room = |slots: &[Option<usize>]| {
        slots
            .iter()
            .flatten()
            .map(|&i| name_width(&pins[i].name))
            .max()
            .unwrap_or(Nm::ZERO)
    };
    let rows = sides.left.len().max(sides.right.len()) as i64;
    let columns = sides.top.len().max(sides.bottom.len()) as i64;
    let width = PITCH
        * (columns + 1)
            .max(pitches(
                name_room(&sides.left) + name_room(&sides.right) + PITCH,
            ))
            .max(2);
    let height = PITCH
        * (rows + 1)
            .max(pitches(
                name_room(&sides.top) + name_room(&sides.bottom) + PITCH,
            ))
            .max(2);

    // Laid out with the body's top-left corner at (0, 0), then moved so Pin 1 ends at the
    // origin.
    let mut placed: Vec<Option<(Side, Point)>> = vec![None; pins.len()];
    let mut put = |slots: &[Option<usize>], side: Side, at: &dyn Fn(i64) -> Point| {
        for (slot, row) in slots.iter().enumerate() {
            if let Some(i) = row {
                placed[*i] = Some((side, at(slot as i64 + 1)));
            }
        }
    };
    put(&sides.left, Side::Left, &|k| {
        Point::new(-length, -(PITCH * k))
    });
    put(&sides.right, Side::Right, &|k| {
        Point::new(width + length, -(PITCH * k))
    });
    put(&sides.top, Side::Top, &|k| Point::new(PITCH * k, length));
    put(&sides.bottom, Side::Bottom, &|k| {
        Point::new(PITCH * k, -height - length)
    });

    let first = pins.iter().position(|p| p.number == "1").unwrap_or(0);
    let anchor = placed[first].expect("every row is placed").1;
    let shift = |p: Point| Point::new(p.x - anchor.x, p.y - anchor.y);
    let pins = pins
        .iter()
        .zip(placed)
        .map(|(row, at)| {
            let (side, at) = at.expect("every row is placed");
            PlannedPin {
                number: row.number.clone(),
                name: row.name.clone(),
                electrical_type: row.electrical_type,
                side,
                at: shift(at),
                rotation: side.rotation(),
                length,
            }
        })
        .collect();
    Ok(SymbolPlan {
        pins,
        body: (
            shift(Point::new(Nm::ZERO, Nm::ZERO)),
            shift(Point::new(width, -height)),
        ),
    })
}

fn check_rows(pins: &[WizardPin]) -> Result<(), PlanError> {
    if pins.is_empty() {
        return Err(PlanError::new("/pins", "the wizard needs at least one Pin"));
    }
    for (i, pin) in pins.iter().enumerate() {
        if pin.number.trim().is_empty() {
            return Err(PlanError::new(
                format!("/pins/{i}/number"),
                "the Pin number is empty",
            ));
        }
        if pins[..i].iter().any(|p| p.number == pin.number) {
            return Err(PlanError::new(
                format!("/pins/{i}/number"),
                format!("Pin {} is listed twice", pin.number),
            ));
        }
    }
    Ok(())
}

/// How many pitches `room` needs, rounded up.
fn pitches(room: Nm) -> i64 {
    (room.0 + PITCH.0 - 1).div_euclid(PITCH.0)
}

/// The room a Pin name takes inside the body, overbar markers left out.
fn name_width(name: &str) -> Nm {
    let letters = name.chars().filter(|c| *c != '\\').count() as i64;
    if letters == 0 {
        Nm::ZERO
    } else {
        NAME_GAP + LETTER * letters
    }
}

fn sides(layout: Layout, pins: &[WizardPin]) -> Result<Sides, PlanError> {
    let n = pins.len();
    let grouped = |rows: &mut dyn Iterator<Item = usize>| filled(pins, rows);
    let mut s = Sides::default();
    match layout {
        Layout::Dil => {
            let half = n.div_ceil(2);
            s.left = grouped(&mut (0..half));
            s.right = grouped(&mut (half..n).rev());
            // The right side counts up from the bottom, so it lines up at the bottom.
            while s.right.len() < s.left.len() {
                s.right.insert(0, None);
            }
        }
        Layout::Quad => {
            let count = |side: usize| n / 4 + usize::from(side < n % 4);
            let ends: Vec<usize> = (0..4)
                .scan(0, |end, side| {
                    *end += count(side);
                    Some(*end)
                })
                .collect();
            s.left = grouped(&mut (0..ends[0]));
            s.bottom = grouped(&mut (ends[0]..ends[1]));
            s.right = grouped(&mut (ends[1]..ends[2]).rev());
            s.top = grouped(&mut (ends[2]..ends[3]).rev());
        }
        Layout::Connector => {
            s.left = (0..n).step_by(2).map(Some).collect();
            s.right = (1..n).step_by(2).map(Some).collect();
        }
        Layout::ZigZag => {
            s.left = (0..n).map(|i| (i % 2 == 0).then_some(i)).collect();
            s.right = (0..n).map(|i| (i % 2 == 1).then_some(i)).collect();
            s.left.truncate(n.saturating_sub(1) / 2 * 2 + 1);
        }
        Layout::SingleRow => s.left = grouped(&mut (0..n)),
        Layout::Manual => {
            if let Some(i) = pins.iter().position(|p| p.side.is_none()) {
                return Err(PlanError::new(
                    format!("/pins/{i}/side"),
                    "the manual layout needs a side on every Pin",
                ));
            }
            let on = |side: Side| {
                let mut rows = (0..n).filter(move |&i| pins[i].side == Some(side));
                filled(pins, &mut rows)
            };
            s.left = on(Side::Left);
            s.right = on(Side::Right);
            s.top = on(Side::Top);
            s.bottom = on(Side::Bottom);
        }
    }
    Ok(s)
}

/// The slots for `rows` in order, with an empty one wherever the group changes.
fn filled(pins: &[WizardPin], rows: &mut dyn Iterator<Item = usize>) -> Vec<Option<usize>> {
    let mut slots = Vec::new();
    let mut last: Option<usize> = None;
    for i in rows {
        if last.is_some_and(|l| pins[l].group != pins[i].group) {
            slots.push(None);
        }
        slots.push(Some(i));
        last = Some(i);
    }
    slots
}

/// The wizard's input, as reopening reads it from a Unit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct WizardInput {
    /// Always `manual`, which puts each Pin back on its side in its order.
    pub layout: Layout,
    /// The length of the Unit's first Pin.
    pub pin_length: Nm,
    /// Left top to bottom, then bottom, right and top. A gap of more than a pitch between
    /// neighbours on a side starts a new group (`1`, `2`, …); with no gaps there are no
    /// groups.
    pub pins: Vec<WizardPin>,
}

/// The wizard's input for `unit` of `symbol`, read from its Pins.
pub fn read_unit(symbol: &Symbol, unit: Uuid) -> WizardInput {
    let pins: Vec<&Pin> = symbol.pins.iter().filter(|p| p.unit == unit).collect();
    let pin_length = pins.first().map_or(PIN_LENGTH, |p| p.length);
    let mut rows = Vec::new();
    let mut group = 1;
    let mut any_gap = false;
    for side in [Side::Left, Side::Bottom, Side::Right, Side::Top] {
        let mut on: Vec<&Pin> = pins
            .iter()
            .copied()
            .filter(|p| Side::of(p.rotation) == side)
            .collect();
        // Reading order: down the left and right sides, along the top and bottom.
        let along = |p: &Pin| match side {
            Side::Left | Side::Right => -p.at.y.0,
            Side::Top | Side::Bottom => p.at.x.0,
        };
        on.sort_by_key(|p| (along(p), p.number.clone()));
        for (k, pin) in on.iter().enumerate() {
            if k > 0 && along(pin) - along(on[k - 1]) > PITCH.0 * 3 / 2 {
                group += 1;
                any_gap = true;
            }
            rows.push((
                group,
                WizardPin {
                    number: pin.number.clone(),
                    name: pin.name.clone(),
                    electrical_type: pin.electrical_type,
                    side: Some(side),
                    group: None,
                },
            ));
        }
    }
    let pins = rows
        .into_iter()
        .map(|(group, mut row)| {
            if any_gap {
                row.group = Some(group.to_string());
            }
            row
        })
        .collect();
    WizardInput {
        layout: Layout::Manual,
        pin_length,
        pins,
    }
}

/// Why a plan cannot go onto a Symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// The Symbol has no such Unit.
    NoUnit(Uuid),
    /// A Pin of another Unit already has this number.
    NumberTaken { number: String, unit: String },
}

/// Puts `plan` onto `unit` of `symbol`: the Unit's Pins become the plan's, each keeping
/// the UUID, visibility, description and decorations of the Pin it had with the same
/// number (new numbers get `new_id`), Pins whose numbers are gone are removed, and the
/// Unit's graphics are replaced by the body rectangle.
pub fn apply(
    symbol: &mut Symbol,
    unit: Uuid,
    plan: &SymbolPlan,
    new_id: &mut dyn FnMut() -> Uuid,
) -> Result<(), ApplyError> {
    if !symbol.units.iter().any(|u| u.uuid == unit) {
        return Err(ApplyError::NoUnit(unit));
    }
    for planned in &plan.pins {
        if let Some(other) = symbol
            .pins
            .iter()
            .find(|p| p.unit != unit && p.number == planned.number)
        {
            let unit = symbol
                .units
                .iter()
                .find(|u| u.uuid == other.unit)
                .map_or_else(String::new, |u| u.name.clone());
            return Err(ApplyError::NumberTaken {
                number: planned.number.clone(),
                unit,
            });
        }
    }
    let (old, mut kept): (Vec<Pin>, Vec<Pin>) = std::mem::take(&mut symbol.pins)
        .into_iter()
        .partition(|p| p.unit == unit);
    for planned in &plan.pins {
        let before = old.iter().find(|p| p.number == planned.number);
        kept.push(Pin {
            uuid: before.map_or_else(&mut *new_id, |p| p.uuid),
            unit,
            name: planned.name.clone(),
            number: planned.number.clone(),
            electrical_type: planned.electrical_type,
            at: planned.at,
            rotation: planned.rotation,
            length: planned.length,
            name_visible: before.is_none_or(|p| p.name_visible),
            number_visible: before.is_none_or(|p| p.number_visible),
            description: before.map(|p| p.description.clone()).unwrap_or_default(),
            inside_edge: before.and_then(|p| p.inside_edge),
            outside_edge: before.and_then(|p| p.outside_edge),
        });
    }
    kept.sort_by_key(|p| p.uuid);
    symbol.pins = kept;
    symbol.graphics.retain(|g| g.unit != unit);
    symbol.graphics.push(SymbolGraphic {
        uuid: new_id(),
        unit,
        shape: Shape::Rect {
            start: plan.body.0,
            end: plan.body.1,
        },
        width: STROKE,
        fill: false,
    });
    symbol.graphics.sort_by_key(|g| g.uuid);
    Ok(())
}
