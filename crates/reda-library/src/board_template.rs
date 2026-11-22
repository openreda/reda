//! Board Templates (ADR-0016): the load checks a `.redaboard` must pass ([`check`]) and
//! the one evaluation function ([`evaluate`]) that turns a template and a set of values
//! into a [`Frame`]. `library.board_template.preview` returns that frame, and creating a
//! board from a template writes it.
//!
//! Expressions go through `reda-geom`'s numeric-expression evaluator: exact, with a bare
//! number read as mm in a length field and as degrees in an angle field. They see one
//! flat namespace, the length parameters plus the names the chosen options bind. The
//! current value `!`, other names and function calls are refused.

mod check;
mod eval;
mod frame;
mod range;

use std::collections::BTreeMap;

use reda_model::geom::expr::{self, Attr, Field, Kind};
use reda_model::geom::{Millideg, Nm};
use reda_model::{BoardTemplate, ChoiceParameter, TemplateNumber, TemplateParameter, Uuid, When};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use check::check;
pub use eval::evaluate;
pub use frame::{
    Frame, FrameBevel, FrameBoard, FrameContour, FrameDimensionEnd, FrameDrawing,
    FrameDrawingGroup, FrameDrill, FrameEdgeConnector, FrameHole, FrameKeepout, FramePad,
    FramePlacement, FrameProblem, FrameVertex, ProblemObject, ResolvedValue,
};
pub use range::{LengthRange, length_ranges};

/// The folder of a Library that holds its Board Templates.
pub const BOARD_TEMPLATE_FOLDER: &str = "board-templates";

/// The extension of a Board Template file, without the dot.
pub const BOARD_TEMPLATE_EXTENSION: &str = "redaboard";

/// Which template to use, at which values, without which objects: the `board_template`
/// param of creation and the params of `library.board_template.preview`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardTemplateChoice {
    /// The template's Library element UUID, from `library.board_template.list`.
    pub element: Uuid,
    /// Parameter values by name: an option key for a choice, a length (integer nm or a
    /// string such as `"65mm"`) for a length. An omitted parameter takes its default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, TemplateValue>,
    /// Template connector, hole and drawing-group UUIDs to leave out. A Keepout that
    /// protects an excluded object, and a Dimension that follows one, go with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<Uuid>,
}

/// A parameter value as given: an integer (a length in nm) or a string (an option key,
/// or a length such as `"65mm"`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateValue {
    Number(i64),
    Text(String),
}

impl From<&str> for TemplateValue {
    fn from(text: &str) -> TemplateValue {
        TemplateValue::Text(text.to_owned())
    }
}

impl JsonSchema for TemplateValue {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TemplateValue".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "An option key for a choice; a length for a length parameter: \
                            integer nanometres or a string such as \"65mm\".",
            "anyOf": [{ "type": "integer" }, { "type": "string" }]
        })
    }
}

/// Why a template could not be evaluated at the values given: bad input or an expression
/// that fails there. Problems with the frame itself are [`Frame::problems`] instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateError {
    /// A JSON pointer into the [`BoardTemplateChoice`] for bad input, such as
    /// `/values/width`, or empty for a failing expression.
    pub pointer: String,
    pub message: String,
}

impl TemplateError {
    fn input(pointer: impl Into<String>, message: impl Into<String>) -> TemplateError {
        TemplateError {
            pointer: pointer.into(),
            message: message.into(),
        }
    }

    fn template(message: impl Into<String>) -> TemplateError {
        TemplateError::input("", message)
    }
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TemplateError {}

/// One choice combination: each choice parameter's option key, by name.
pub type Combination = BTreeMap<String, String>;

/// Every combination of the template's choice options, the first parameter varying
/// slowest, in option order. A template without choices has one, empty combination.
pub fn combinations(template: &BoardTemplate) -> Vec<Combination> {
    let mut out = vec![Combination::new()];
    for choice in choices(template) {
        out = out
            .into_iter()
            .flat_map(|combo| {
                choice.options.iter().map(move |option| {
                    let mut next = combo.clone();
                    next.insert(choice.name.clone(), option.key.clone());
                    next
                })
            })
            .collect();
    }
    out
}

fn choices(template: &BoardTemplate) -> impl Iterator<Item = &ChoiceParameter> {
    template.parameters.iter().filter_map(|p| match p {
        TemplateParameter::Choice(c) => Some(c),
        TemplateParameter::Length(_) => None,
    })
}

/// Whether a row gated by `when` exists under `combination`. No gate always holds.
pub fn gate_holds(when: Option<&When>, combination: &Combination) -> bool {
    when.is_none_or(|gate| {
        gate.iter().all(|(param, keys)| {
            combination
                .get(param)
                .is_some_and(|chosen| keys.contains(chosen))
        })
    })
}

/// A value that names nothing, such as a length parameter's `min`, as a length.
pub fn constant_length(number: &TemplateNumber) -> Result<Nm, String> {
    Namespace::empty().length(number)
}

/// The names an expression can see, with their values.
pub(crate) struct Namespace(Vec<(String, Attr)>);

impl Namespace {
    pub(crate) fn empty() -> Namespace {
        Namespace(vec![])
    }

    pub(crate) fn insert(&mut self, name: &str, value: Attr) {
        self.0.push((name.to_owned(), value));
    }

    fn eval(&self, number: &TemplateNumber, kind: Kind) -> Result<i64, String> {
        match number {
            TemplateNumber::Int(n) => Ok(*n),
            TemplateNumber::Expr(text) => {
                if expr::references(text).map_err(|e| e.to_string())?.current {
                    return Err(format!("`{text}`: `!` is not allowed in a template"));
                }
                let attributes: Vec<(&str, Attr)> =
                    self.0.iter().map(|(n, a)| (n.as_str(), *a)).collect();
                let object = expr::Object {
                    current: None,
                    attributes: &attributes,
                };
                let unit = match kind {
                    Kind::Length => "mm",
                    Kind::Angle => "deg",
                };
                expr::evaluate(text, Field { kind, unit }, &object)
                    .map_err(|e| format!("`{text}`: {e}"))
            }
        }
    }

    pub(crate) fn length(&self, number: &TemplateNumber) -> Result<Nm, String> {
        self.eval(number, Kind::Length).map(Nm)
    }

    pub(crate) fn angle(&self, number: &TemplateNumber) -> Result<Millideg, String> {
        self.eval(number, Kind::Angle).map(Millideg)
    }

    /// A value bound by a choice option: a length (integer nm, or an expression that
    /// gives a length), else an angle.
    pub(crate) fn bound(&self, number: &TemplateNumber) -> Result<Attr, String> {
        match self.length(number) {
            Ok(nm) => Ok(Attr::Length(nm)),
            Err(length_error) => self
                .angle(number)
                .map(Attr::Angle)
                .map_err(|_| length_error),
        }
    }
}
