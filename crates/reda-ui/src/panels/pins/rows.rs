//! The Pins Panel's table on a Symbol, without drawing: its columns, its rows in the
//! chosen multi-column sort, and the Commands a cell edit becomes. An edit to a selected
//! row is made to every selected row (multi-cell edit), as one `batch`, so one undo step.

use std::cmp::Ordering;

use reda_engine::model::geom::{Millideg, Nm};
use reda_engine::model::{InsideEdge, OutsideEdge, Pin, PinType, Symbol, Uuid};
use reda_engine::schematic::natural_cmp;
use serde_json::{Value, json};

use crate::element_editor::Invocation;

/// A column of the Pins Panel on a Symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PinColumn {
    Number,
    Name,
    Type,
    Unit,
    Side,
    Length,
    Description,
    InsideEdge,
    OutsideEdge,
    NameVisible,
    NumberVisible,
}

impl PinColumn {
    /// Every column, in the order the table shows them.
    pub const ALL: [PinColumn; 11] = [
        PinColumn::Number,
        PinColumn::Name,
        PinColumn::Type,
        PinColumn::Unit,
        PinColumn::Side,
        PinColumn::Length,
        PinColumn::Description,
        PinColumn::InsideEdge,
        PinColumn::OutsideEdge,
        PinColumn::NameVisible,
        PinColumn::NumberVisible,
    ];

    /// The catalogue key of its heading.
    pub fn title_key(self) -> &'static str {
        match self {
            PinColumn::Number => "pin-table-number",
            PinColumn::Name => "pin-table-name",
            PinColumn::Type => "pin-table-type",
            PinColumn::Unit => "pin-table-unit",
            PinColumn::Side => "pin-table-side",
            PinColumn::Length => "pin-table-length",
            PinColumn::Description => "pin-table-description",
            PinColumn::InsideEdge => "pin-table-inside-edge",
            PinColumn::OutsideEdge => "pin-table-outside-edge",
            PinColumn::NameVisible => "pin-table-name-visible",
            PinColumn::NumberVisible => "pin-table-number-visible",
        }
    }
}

/// The side of the body a Pin is on, from the direction it points into the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PinSide {
    Left,
    Right,
    Top,
    Bottom,
}

impl PinSide {
    pub const ALL: [PinSide; 4] = [PinSide::Left, PinSide::Right, PinSide::Top, PinSide::Bottom];

    /// The side a Pin pointing `rotation` into the body is on, to the nearest quarter turn.
    pub fn of(rotation: Millideg) -> PinSide {
        match (rotation.0.rem_euclid(360_000) + 45_000) / 90_000 {
            1 => PinSide::Bottom,
            2 => PinSide::Right,
            3 => PinSide::Top,
            _ => PinSide::Left,
        }
    }

    /// The direction a Pin on this side points into the body.
    pub fn rotation(self) -> Millideg {
        Millideg(match self {
            PinSide::Left => 0,
            PinSide::Bottom => 90_000,
            PinSide::Right => 180_000,
            PinSide::Top => 270_000,
        })
    }

    pub fn title_key(self) -> &'static str {
        match self {
            PinSide::Left => "pin-side-left",
            PinSide::Right => "pin-side-right",
            PinSide::Top => "pin-side-top",
            PinSide::Bottom => "pin-side-bottom",
        }
    }
}

/// The multi-column sort: the first key decides, the next breaks its ties, and so on.
/// Empty, Pins go by number.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sort {
    /// Each key's column and whether it ascends.
    pub keys: Vec<(PinColumn, bool)>,
}

impl Sort {
    /// A click on `column`'s heading: alone it sorts by that column only (turning it
    /// round when it already led); with `add` (Shift) it adds the column as one more key,
    /// or turns that key round when it is one already.
    pub fn click(&mut self, column: PinColumn, add: bool) {
        match self.keys.iter().position(|(c, _)| *c == column) {
            Some(i) if add => self.keys[i].1 = !self.keys[i].1,
            Some(0) if self.keys.len() == 1 => self.keys[0].1 = !self.keys[0].1,
            _ if add => self.keys.push((column, true)),
            _ => self.keys = vec![(column, true)],
        }
    }

    /// Where `column` is in the sort (0 leads) and whether it ascends.
    pub fn rank(&self, column: PinColumn) -> Option<(usize, bool)> {
        let i = self.keys.iter().position(|(c, _)| *c == column)?;
        Some((i, self.keys[i].1))
    }
}

/// The Symbol's Pins in the order `sort` gives, every Unit's.
pub fn sorted<'a>(symbol: &'a Symbol, sort: &Sort) -> Vec<&'a Pin> {
    let mut pins: Vec<&Pin> = symbol.pins.iter().collect();
    pins.sort_by(|a, b| {
        for (column, ascending) in &sort.keys {
            let order = compare(symbol, *column, a, b);
            let order = if *ascending { order } else { order.reverse() };
            if order != Ordering::Equal {
                return order;
            }
        }
        natural_cmp(&a.number, &b.number)
    });
    pins
}

fn compare(symbol: &Symbol, column: PinColumn, a: &Pin, b: &Pin) -> Ordering {
    let unit = |p: &Pin| {
        symbol
            .units
            .iter()
            .position(|u| u.uuid == p.unit)
            .unwrap_or(usize::MAX)
    };
    match column {
        PinColumn::Number => natural_cmp(&a.number, &b.number),
        PinColumn::Name => natural_cmp(&a.name, &b.name),
        PinColumn::Type => type_rank(a.electrical_type).cmp(&type_rank(b.electrical_type)),
        PinColumn::Unit => unit(a).cmp(&unit(b)),
        PinColumn::Side => PinSide::of(a.rotation).cmp(&PinSide::of(b.rotation)),
        PinColumn::Length => a.length.cmp(&b.length),
        PinColumn::Description => a.description.cmp(&b.description),
        PinColumn::InsideEdge => a.inside_edge.is_some().cmp(&b.inside_edge.is_some()),
        PinColumn::OutsideEdge => outside_rank(a.outside_edge).cmp(&outside_rank(b.outside_edge)),
        PinColumn::NameVisible => a.name_visible.cmp(&b.name_visible),
        PinColumn::NumberVisible => a.number_visible.cmp(&b.number_visible),
    }
}

fn type_rank(t: PinType) -> usize {
    PinType::ALL.iter().position(|x| *x == t).unwrap_or(0)
}

fn outside_rank(d: Option<OutsideEdge>) -> usize {
    match d {
        None => 0,
        Some(OutsideEdge::Dot) => 1,
        Some(OutsideEdge::ActiveLowInput) => 2,
        Some(OutsideEdge::ActiveLowOutput) => 3,
    }
}

/// A new value for a cell.
#[derive(Clone, Debug, PartialEq)]
pub enum CellValue {
    Text(String),
    Type(PinType),
    Unit(Uuid),
    Side(PinSide),
    Length(Nm),
    InsideEdge(Option<InsideEdge>),
    OutsideEdge(Option<OutsideEdge>),
    Flag(bool),
}

/// The `library.symbol.pin.edit` field and value that set `column` to `value`; None when
/// they do not fit together.
fn field(column: PinColumn, value: &CellValue) -> Option<(&'static str, Value)> {
    Some(match (column, value) {
        (PinColumn::Number, CellValue::Text(t)) => ("number", json!(t.trim())),
        (PinColumn::Name, CellValue::Text(t)) => ("name", json!(t)),
        (PinColumn::Description, CellValue::Text(t)) => ("description", json!(t)),
        (PinColumn::Type, CellValue::Type(t)) => ("electrical_type", json!(t)),
        (PinColumn::Unit, CellValue::Unit(u)) => ("unit", json!(u)),
        (PinColumn::Side, CellValue::Side(s)) => ("rotation", json!(s.rotation().0)),
        (PinColumn::Length, CellValue::Length(l)) => ("length", json!(l.0)),
        (PinColumn::InsideEdge, CellValue::InsideEdge(d)) => ("inside_edge", json!(d)),
        (PinColumn::OutsideEdge, CellValue::OutsideEdge(d)) => ("outside_edge", json!(d)),
        (PinColumn::NameVisible, CellValue::Flag(f)) => ("name_visible", json!(f)),
        (PinColumn::NumberVisible, CellValue::Flag(f)) => ("number_visible", json!(f)),
        _ => return None,
    })
}

/// The Command that sets `column` of `pin` to `value`: on that Pin alone, or, when `pin`
/// is among `selected`, on every selected Pin of the Symbol (multi-cell edit) as one
/// `batch`. A Pin's number is unique, so a number edit goes to `pin` alone.
pub fn edit(
    symbol: &Symbol,
    selected: &[Uuid],
    pin: Uuid,
    column: PinColumn,
    value: &CellValue,
) -> Option<Invocation> {
    let (name, value) = field(column, value)?;
    let mut pins: Vec<Uuid> = if selected.contains(&pin) && column != PinColumn::Number {
        (symbol.pins.iter())
            .map(|p| p.uuid)
            .filter(|p| selected.contains(p))
            .collect()
    } else {
        vec![pin]
    };
    pins.retain(|p| symbol.pins.iter().any(|x| x.uuid == *p));
    let step = |p: Uuid| {
        json!({"id": "library.symbol.pin.edit",
               "params": {"symbol": symbol.uuid, "pin": p, name: value.clone()}})
    };
    match pins.as_slice() {
        [] => None,
        [one] => {
            let s = step(*one);
            Some((s["id"].as_str()?.to_owned(), s["params"].clone()))
        }
        many => Some((
            "batch".to_owned(),
            json!({"commands": many.iter().map(|p| step(*p)).collect::<Vec<_>>()}),
        )),
    }
}

/// A length typed in a cell: bare numbers are mil, as the cells show them.
pub fn parse_length(text: &str) -> Option<Nm> {
    let t = text.trim();
    let mil: f64 = t.strip_suffix("mil").unwrap_or(t).trim().parse().ok()?;
    Some(Nm((mil * 25_400.0).round() as i64))
}

/// A length as the cells show it, in mil.
pub fn mil(n: Nm) -> String {
    let v = n.0 as f64 / 25_400.0;
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}
