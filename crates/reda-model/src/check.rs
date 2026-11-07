//! What checks report: [`Violation`]s and their [`Severity`].
//!
//! ERC, DRC and the Library check all report through the same Violation shape
//! ([checks](../../../docs/spec/v0.1/checks.md#reporting)). A Violation carries no text:
//! the UI turns its check id and arguments into a message, and agents read the same
//! structured list from the Command result.

use std::collections::BTreeMap;

use reda_geom::{Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

mod pin_matrix;
mod waiver;

pub use pin_matrix::pin_conflict;
pub use waiver::{Waived, Waiver, apply_waivers};

// A check set to `Off` does not run, so it reports nothing. (Kept out of the doc comment,
// which is the file schemas' description.)
/// How a check's Violations are reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Off,
}

/// One breach of a rule found by a check, naming the objects involved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Violation {
    /// Which check found it, such as `duplicate_pin_number` or `clearance`. Together with
    /// `args` it picks the message the UI shows.
    pub check: String,
    /// Error or Warning; never Off, because a check set to Off reports nothing.
    pub severity: Severity,
    /// The values the message is built from, by name, such as `{"number": "3"}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, ViolationArg>,
    /// The UUIDs of the objects involved, the most specific last: for a Pin of a Symbol,
    /// the Symbol then the Pin. The UI selects these when the Violation is clicked.
    pub objects: Vec<Uuid>,
    /// The Waiver that waives it. Only waived Violations carry one, and a run lists them
    /// only when asked to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiver: Option<Uuid>,
    /// A measured length in nm or area in square nm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measured: Option<Measurement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<Measurement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<Limit>,
    /// The check's marker point, retained even when an object is deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<Uuid>,
    /// The closest points on a distance check's two objects, in object order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<[Point; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Uuid>,
    /// The record that supplied the limit, including a board-wide record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<Uuid>,
}

impl Violation {
    /// A Violation of `check` with no arguments and no objects yet.
    pub fn new(check: impl Into<String>, severity: Severity) -> Violation {
        Violation {
            check: check.into(),
            severity,
            args: BTreeMap::new(),
            objects: vec![],
            waiver: None,
            measured: None,
            required: None,
            limit: None,
            at: None,
            document: None,
            points: None,
            layers: vec![],
            rule: None,
        }
    }

    /// Records a length comparison and the rule record that supplied the limit.
    pub fn length(mut self, measured: Nm, required: Nm, limit: Limit, rule: Uuid) -> Self {
        self.measured = Some(Measurement::Length(measured));
        self.required = Some(Measurement::Length(required));
        self.limit = Some(limit);
        self.rule = Some(rule);
        self
    }

    /// Records an area comparison in square nanometres.
    pub fn area(mut self, measured: i64, required: i64, limit: Limit, rule: Uuid) -> Self {
        self.measured = Some(Measurement::Area(measured));
        self.required = Some(Measurement::Area(required));
        self.limit = Some(limit);
        self.rule = Some(rule);
        self
    }

    pub fn located(mut self, document: Uuid, at: Point) -> Self {
        self.document = Some(document);
        self.at = Some(at);
        self
    }

    pub fn on_layers(mut self, layers: impl IntoIterator<Item = Uuid>) -> Self {
        self.layers.extend(layers);
        self.layers.sort();
        self.layers.dedup();
        self
    }

    /// Records the closest points and places the marker midway between them.
    pub fn distance_points(mut self, a: Point, b: Point) -> Self {
        self.points = Some([a, b]);
        self.at = Some(Point::new(
            Nm(a.x.0 + (b.x.0 - a.x.0) / 2),
            Nm(a.y.0 + (b.y.0 - a.y.0) / 2),
        ));
        self
    }

    /// Adds the message argument `name`.
    pub fn arg(mut self, name: &str, value: impl Into<ViolationArg>) -> Violation {
        self.args.insert(name.to_owned(), value.into());
        self
    }

    /// Adds an object involved.
    pub fn object(mut self, uuid: Uuid) -> Violation {
        self.objects.push(uuid);
        self
    }

    /// Adds several objects involved, in order.
    pub fn objects(mut self, uuids: impl IntoIterator<Item = Uuid>) -> Violation {
        self.objects.extend(uuids);
        self
    }
}

/// A measured quantity, tagged by dimension. Lengths use nm; areas use square nm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Measurement {
    Length(Nm),
    Area(i64),
}

/// Which limit the measured value violates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Limit {
    Min,
    Max,
}

/// One message argument: a string, or an integer (lengths in nanometres, angles in
/// millidegrees, counts).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ViolationArg {
    Int(i64),
    Text(String),
}

impl From<&str> for ViolationArg {
    fn from(s: &str) -> ViolationArg {
        ViolationArg::Text(s.to_owned())
    }
}

impl From<String> for ViolationArg {
    fn from(s: String) -> ViolationArg {
        ViolationArg::Text(s)
    }
}

impl From<i64> for ViolationArg {
    fn from(n: i64) -> ViolationArg {
        ViolationArg::Int(n)
    }
}

impl From<Nm> for ViolationArg {
    fn from(n: Nm) -> ViolationArg {
        ViolationArg::Int(n.0)
    }
}

impl From<Uuid> for ViolationArg {
    fn from(u: Uuid) -> ViolationArg {
        ViolationArg::Text(u.to_string())
    }
}

/// Defines a check enum and the struct holding one [`Severity`] per check, from one table
/// of `field => Variant = DefaultSeverity` rows. The field name is the check id that
/// Violations and Waivers carry.
macro_rules! severities {
    (
        $(#[$emeta:meta])* enum $check:ident;
        $(#[$smeta:meta])* struct $severities:ident;
        $($(#[$doc:meta])* $field:ident => $variant:ident = $default:ident,)*
    ) => {
        $(#[$emeta])*
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
            JsonSchema,
        )]
        #[serde(rename_all = "snake_case")]
        pub enum $check {
            $($(#[$doc])* $variant,)*
        }

        impl $check {
            /// Every check, in the order of its table.
            pub const ALL: &[$check] = &[$($check::$variant,)*];

            /// The check id Violations and Waivers carry, such as `clearance`.
            pub const fn id(self) -> &'static str {
                match self {
                    $($check::$variant => stringify!($field),)*
                }
            }
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        $(#[$smeta])*
        #[serde(deny_unknown_fields)]
        pub struct $severities {
            $($(#[$doc])* pub $field: Severity,)*
        }

        impl $severities {
            /// The defaults of the table.
            pub const DEFAULT: $severities = $severities {
                $($field: Severity::$default,)*
            };

            pub fn get(&self, check: $check) -> Severity {
                match check {
                    $($check::$variant => self.$field,)*
                }
            }

            pub fn set(&mut self, check: $check, severity: Severity) {
                match check {
                    $($check::$variant => self.$field = severity,)*
                }
            }

            /// Whether every check has its default severity.
            pub fn is_default(&self) -> bool {
                *self == $severities::DEFAULT
            }
        }

        impl Default for $severities {
            fn default() -> $severities {
                $severities::DEFAULT
            }
        }
    };
}

pub(crate) use severities;

severities! {
    /// One ERC check, named as in [`ErcSeverities`].
    enum ErcCheck;
    /// The severity of every ERC check, stored in the Project. A check left out has its
    /// default.
    #[serde(default)]
    struct ErcSeverities;
    /// An Input or Power In Pin connected to nothing.
    unconnected_input => UnconnectedInput = Error,
    /// A Pin of another type connected to nothing. Not Connected Pins and Pins with a
    /// No-Connect Marker are exempt.
    unconnected_pin => UnconnectedPin = Warning,
    /// Drivers fighting on a Net: Output and Power Out Pins together.
    pin_conflict => PinConflict = Error,
    /// Pin types that should not mix on a Net, such as Tri-state and Output.
    pin_conflict_mixed => PinConflictMixed = Warning,
    /// A Net with Power In Pins and no Power Out Pin or Power Port.
    undriven_power => UndrivenPower = Warning,
    /// A wired Net with only one Pin.
    single_pin_net => SinglePinNet = Warning,
    /// A Wire end that touches nothing.
    dangling_wire => DanglingWire = Warning,
    /// A Net Label or Port attached to nothing.
    unattached_label => UnattachedLabel = Warning,
    /// A No-Connect Marker on a Pin that is connected.
    no_connect_connected => NoConnectConnected = Warning,
    /// One Net with different names on one sheet; names differing only in case are one
    /// name.
    net_names_on_sheet => NetNamesOnSheet = Error,
    /// One Net with different names on different sheets.
    net_names_across_sheets => NetNamesAcrossSheets = Warning,
    /// Different Nets with the same board name, such as one Net Label on two sheets. It
    /// blocks Sync.
    duplicate_nets => DuplicateNets = Error,
    /// A Port whose name no other sheet has.
    single_sheet_port => SingleSheetPort = Warning,
    /// Several Components with the same Designator.
    duplicate_designator => DuplicateDesignator = Error,
    /// Units of a placed multi-unit Component left unplaced.
    unplaced_units => UnplacedUnits = Warning,
    /// Units of one Component whose Component data differ.
    unit_data_mismatch => UnitDataMismatch = Error,
}
