//! The Pin Matrix: how strongly ERC reports each pairing of Pin types and Port directions
//! on a Net, and each Pin type left unconnected. It sets the severity of `unconnected_pin`
//! and `pin_conflict`, one cell at a time.
//!
//! The Project stores only the cells that differ from the default
//! (`erc.pin_matrix_overrides`), keyed `"a|b"` with the two names in [`MatrixType::ALL`]
//! order, such as `{"output|output": "warning", "passive|unconnected": "off"}`.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{ErcCheck, ErcSeverities, PinType, PortDirection, Severity};

/// What a row or column of the Pin Matrix stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MatrixType {
    Pin(PinType),
    Port(PortDirection),
    /// A Pin connected to nothing; a column for Pin types only.
    Unconnected,
}

impl MatrixType {
    /// Every row and column, in the matrix's order: the Pin types in the Pin type table's
    /// order, then Port `in`, `out` and `bidirectional`, then Unconnected.
    pub const ALL: &[MatrixType] = &[
        MatrixType::Pin(PinType::Input),
        MatrixType::Pin(PinType::Output),
        MatrixType::Pin(PinType::Bidirectional),
        MatrixType::Pin(PinType::TriState),
        MatrixType::Pin(PinType::Passive),
        MatrixType::Pin(PinType::PowerIn),
        MatrixType::Pin(PinType::PowerOut),
        MatrixType::Pin(PinType::OpenCollector),
        MatrixType::Pin(PinType::OpenEmitter),
        MatrixType::Pin(PinType::NotConnected),
        MatrixType::Port(PortDirection::In),
        MatrixType::Port(PortDirection::Out),
        MatrixType::Port(PortDirection::Bidirectional),
        MatrixType::Unconnected,
    ];

    /// The name keys and Commands use, such as `power_out`, `port_in` or `unconnected`.
    pub const fn id(self) -> &'static str {
        match self {
            MatrixType::Pin(t) => match t {
                PinType::Input => "input",
                PinType::Output => "output",
                PinType::Bidirectional => "bidirectional",
                PinType::TriState => "tri_state",
                PinType::Passive => "passive",
                PinType::PowerIn => "power_in",
                PinType::PowerOut => "power_out",
                PinType::OpenCollector => "open_collector",
                PinType::OpenEmitter => "open_emitter",
                PinType::NotConnected => "not_connected",
            },
            MatrixType::Port(PortDirection::In) => "port_in",
            MatrixType::Port(PortDirection::Out) => "port_out",
            MatrixType::Port(PortDirection::Bidirectional) => "port_bidirectional",
            MatrixType::Unconnected => "unconnected",
        }
    }

    pub fn from_id(id: &str) -> Option<MatrixType> {
        MatrixType::ALL.iter().copied().find(|t| t.id() == id)
    }

    /// The place in [`MatrixType::ALL`].
    pub fn index(self) -> usize {
        MatrixType::ALL
            .iter()
            .position(|t| *t == self)
            .expect("every type is listed")
    }
}

impl Ord for MatrixType {
    fn cmp(&self, other: &MatrixType) -> Ordering {
        self.index().cmp(&other.index())
    }
}

impl PartialOrd for MatrixType {
    fn partial_cmp(&self, other: &MatrixType) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Serialize for MatrixType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.id())
    }
}

impl<'de> Deserialize<'de> for MatrixType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<MatrixType, D::Error> {
        let id = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        MatrixType::from_id(&id)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown Pin Matrix type {id:?}")))
    }
}

impl JsonSchema for MatrixType {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "MatrixType".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let ids: Vec<&str> = MatrixType::ALL.iter().map(|t| t.id()).collect();
        schemars::json_schema!({
            "type": "string",
            "enum": ids,
            "description": "A Pin type, `port_in`, `port_out`, `port_bidirectional` or `unconnected`."
        })
    }
}

/// One cell of the symmetric Pin Matrix: two types, kept in matrix order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cell {
    a: MatrixType,
    b: MatrixType,
}

impl Cell {
    /// The cell of `a` and `b` in either order; `None` when the matrix has no such cell:
    /// Unconnected pairs with Pin types only.
    pub fn new(a: MatrixType, b: MatrixType) -> Option<Cell> {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let pin = |t: MatrixType| matches!(t, MatrixType::Pin(_));
        (b != MatrixType::Unconnected || pin(a)).then_some(Cell { a, b })
    }

    /// The cell of a Pin type and Unconnected.
    pub fn unconnected(t: PinType) -> Cell {
        Cell {
            a: MatrixType::Pin(t),
            b: MatrixType::Unconnected,
        }
    }

    /// Every cell, row by row in matrix order.
    pub fn all() -> impl Iterator<Item = Cell> {
        MatrixType::ALL.iter().enumerate().flat_map(|(i, &a)| {
            MatrixType::ALL[i..]
                .iter()
                .filter_map(move |&b| Cell::new(a, b))
        })
    }

    /// The two types, in matrix order.
    pub fn types(self) -> (MatrixType, MatrixType) {
        (self.a, self.b)
    }

    /// The key the Project file stores, such as `output|power_out`.
    pub fn key(self) -> String {
        format!("{}|{}", self.a.id(), self.b.id())
    }

    /// The cell a key names; `None` for an unknown name, a pair out of matrix order or a
    /// cell the matrix lacks.
    pub fn from_key(key: &str) -> Option<Cell> {
        let (a, b) = key.split_once('|')?;
        let (a, b) = (MatrixType::from_id(a)?, MatrixType::from_id(b)?);
        Cell::new(a, b).filter(|c| c.a == a)
    }

    /// The check a Violation of this cell belongs to.
    pub fn check(self) -> ErcCheck {
        if self.b == MatrixType::Unconnected {
            ErcCheck::UnconnectedPin
        } else {
            ErcCheck::PinConflict
        }
    }

    /// The cell's default severity.
    pub fn default_severity(self) -> Severity {
        use MatrixType::{Pin, Port, Unconnected};
        use PinType::*;
        let driver = |t: MatrixType| matches!(t, Pin(Output | PowerOut));
        let (a, b) = (self.a, self.b);
        if driver(a) && driver(b) {
            return Severity::Error;
        }
        let mixed = |x: MatrixType, y: MatrixType| match x {
            Pin(TriState | OpenCollector | OpenEmitter) if driver(y) => true,
            Pin(Bidirectional) => y == Pin(PowerOut),
            Pin(OpenCollector) => matches!(y, Pin(OpenEmitter | TriState)),
            Pin(OpenEmitter) => y == Pin(TriState),
            _ => false,
        };
        if mixed(a, b) || mixed(b, a) {
            return Severity::Warning;
        }
        match (a, b) {
            (Pin(Input | PowerIn), Unconnected) => Severity::Error,
            (Pin(NotConnected), Unconnected) => Severity::Off,
            (Pin(_), Unconnected) => Severity::Warning,
            (Pin(Output | PowerOut | Bidirectional), Port(PortDirection::In)) => Severity::Warning,
            (Port(PortDirection::Out), Port(PortDirection::Out)) => Severity::Warning,
            _ => Severity::Off,
        }
    }
}

/// The Project's Pin Matrix: every cell's severity, stored as the cells that differ from
/// the default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PinMatrix {
    overrides: BTreeMap<Cell, Severity>,
}

impl PinMatrix {
    /// Every cell at its default.
    pub const DEFAULT: PinMatrix = PinMatrix {
        overrides: BTreeMap::new(),
    };

    /// The severity of a cell.
    pub fn get(&self, cell: Cell) -> Severity {
        self.overrides
            .get(&cell)
            .copied()
            .unwrap_or_else(|| cell.default_severity())
    }

    /// The severity of the pair `a`, `b`; Off for a pair the matrix has no cell for.
    pub fn severity(&self, a: MatrixType, b: MatrixType) -> Severity {
        Cell::new(a, b).map_or(Severity::Off, |c| self.get(c))
    }

    /// Sets a cell; `None`, or its default, resets it.
    pub fn set(&mut self, cell: Cell, severity: Option<Severity>) {
        match severity.filter(|s| *s != cell.default_severity()) {
            Some(s) => {
                self.overrides.insert(cell, s);
            }
            None => {
                self.overrides.remove(&cell);
            }
        }
    }

    /// The cells that differ from the default, in matrix order.
    pub fn overrides(&self) -> impl Iterator<Item = (Cell, Severity)> + '_ {
        self.overrides.iter().map(|(c, s)| (*c, *s))
    }

    /// Whether every cell has its default.
    pub fn is_default(&self) -> bool {
        self.overrides.is_empty()
    }

    /// Whether any cell of `check` is not Off, so the check need run.
    pub fn any_on(&self, check: ErcCheck) -> bool {
        Cell::all().any(|c| c.check() == check && self.get(c) != Severity::Off)
    }
}

impl Serialize for PinMatrix {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let map: BTreeMap<String, Severity> =
            self.overrides.iter().map(|(c, v)| (c.key(), *v)).collect();
        map.serialize(s)
    }
}

impl<'de> Deserialize<'de> for PinMatrix {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<PinMatrix, D::Error> {
        let map = BTreeMap::<String, Severity>::deserialize(d)?;
        let mut matrix = PinMatrix::DEFAULT;
        for (key, severity) in map {
            let cell = Cell::from_key(&key).ok_or_else(|| {
                serde::de::Error::custom(format!(
                    "{key:?} is not a Pin Matrix cell: write two of its types in matrix \
                     order, such as \"output|power_out\""
                ))
            })?;
            matrix.set(cell, Some(severity));
        }
        Ok(matrix)
    }
}

impl JsonSchema for PinMatrix {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PinMatrix".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let severity = generator.subschema_for::<Severity>().to_value();
        let properties: serde_json::Map<String, serde_json::Value> =
            Cell::all().map(|c| (c.key(), severity.clone())).collect();
        schemars::json_schema!({
            "type": "object",
            "description": "The Pin Matrix cells that differ from the default, keyed by the two types in matrix order.",
            "properties": properties,
            "additionalProperties": false
        })
    }
}

/// The Project's ERC settings: each check's severity and the Pin Matrix, which sets
/// `unconnected_pin` and `pin_conflict`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErcSettings {
    /// The severity of each check the Pin Matrix does not set. A check left out has its
    /// default.
    #[serde(flatten)]
    pub severities: ErcSeverities,
    /// The Pin Matrix cells that differ from the default.
    #[serde(default, skip_serializing_if = "PinMatrix::is_default")]
    pub pin_matrix_overrides: PinMatrix,
}

impl ErcSettings {
    pub const DEFAULT: ErcSettings = ErcSettings {
        severities: ErcSeverities::DEFAULT,
        pin_matrix_overrides: PinMatrix::DEFAULT,
    };

    /// Whether every severity and cell has its default.
    pub fn is_default(&self) -> bool {
        self.severities.is_default() && self.pin_matrix_overrides.is_default()
    }

    /// Whether `check` can report anything: for a matrix check, whether any of its cells
    /// is not Off.
    pub fn is_on(&self, check: ErcCheck) -> bool {
        match self.severities.get(check) {
            Some(s) => s != Severity::Off,
            None => self.pin_matrix_overrides.any_on(check),
        }
    }
}

#[cfg(test)]
mod tests;
