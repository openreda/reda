//! What checks report: [`Violation`]s and their [`Severity`].
//!
//! ERC, DRC and the Library check all report through the same Violation shape
//! ([checks](../../../docs/spec/v0.1/checks.md#reporting)). A Violation carries no text:
//! the UI turns its check id and arguments into a message, and agents read the same
//! structured list from the Command result.

use std::collections::BTreeMap;

use reda_geom::Nm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

/// How a check's Violations are reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    /// The check does not run, so it reports no Violations.
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
}

impl Violation {
    /// A Violation of `check` with no arguments and no objects yet.
    pub fn new(check: impl Into<String>, severity: Severity) -> Violation {
        Violation {
            check: check.into(),
            severity,
            args: BTreeMap::new(),
            objects: vec![],
        }
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
