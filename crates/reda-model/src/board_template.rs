//! Board Templates (ADR-0016): a Library element holding a board's mechanical frame,
//! with choice and length parameters, expression-valued geometry and `when` gates.
//!
//! These are the rows of a `.redaboard` file, apart from the part table, whose file rows
//! the engine owns. Loading, checking and evaluating a template live in `reda-library`.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::{
    Corners, FootprintCopy, HoleTolerance, KeepoutShapeKind, Mask, PadCopperShape, PadKind,
    PadShape, Part, Paste, Ratio, Restriction, Side, Symbol, UnusedLayers, Uuid, ZoneConnection,
};

/// A numeric geometry field of a template: an integer in base units (nanometres, or
/// millidegrees for an angle), or an expression string over the template's parameter
/// names, in which a bare number is mm in a length field and degrees in an angle field.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateNumber {
    Int(i64),
    Expr(String),
}

impl TemplateNumber {
    pub const ZERO: TemplateNumber = TemplateNumber::Int(0);

    pub fn is_zero(&self) -> bool {
        *self == TemplateNumber::ZERO
    }

    /// The expression text, or `None` for a plain integer.
    pub fn expression(&self) -> Option<&str> {
        match self {
            TemplateNumber::Int(_) => None,
            TemplateNumber::Expr(text) => Some(text),
        }
    }
}

impl Default for TemplateNumber {
    fn default() -> TemplateNumber {
        TemplateNumber::ZERO
    }
}

impl From<i64> for TemplateNumber {
    fn from(n: i64) -> TemplateNumber {
        TemplateNumber::Int(n)
    }
}

impl From<&str> for TemplateNumber {
    fn from(text: &str) -> TemplateNumber {
        TemplateNumber::Expr(text.to_owned())
    }
}

impl JsonSchema for TemplateNumber {
    fn schema_name() -> Cow<'static, str> {
        "TemplateNumber".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "An integer in nanometres (millidegrees for an angle), or an \
                            expression over the template's parameter names such as \
                            \"width - corner_radius\"; a bare number in an expression is mm \
                            (degrees for an angle).",
            "anyOf": [{ "type": "integer" }, { "type": "string", "minLength": 1 }]
        })
    }
}

/// A point whose coordinates may be expressions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplatePoint {
    pub x: TemplateNumber,
    pub y: TemplateNumber,
}

/// A `when` gate: the row exists when, for every choice parameter named, the chosen
/// option is one of the keys listed. An AND across parameters, an OR within a list.
pub type When = BTreeMap<String, Vec<String>>;

/// One Board Template, as loaded from its `.redaboard` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardTemplate {
    pub uuid: Uuid,
    /// Unique within its Library.
    pub name: String,
    pub description: String,
    /// Groups the picker; `None` lists it under "Other".
    pub category: Option<String>,
    /// A plain copper Layer count, 2 to 32.
    pub suggested_layers: u8,
    pub board: Option<TemplateBoard>,
    /// In the order the dialog shows them.
    pub parameters: Vec<TemplateParameter>,
    pub outline: Vec<TemplateContour>,
    pub cutouts: Vec<TemplateContour>,
    pub keepouts: Vec<TemplateKeepout>,
    pub parts: Vec<TemplatePart>,
    pub footprint_copies: Vec<FootprintCopy>,
    pub connectors: Vec<TemplatePlacement>,
    pub holes: Vec<TemplateHole>,
    pub drawing_groups: Vec<TemplateDrawingGroup>,
    pub drawings: Vec<TemplateDrawing>,
}

impl BoardTemplate {
    /// An empty template, to fill in.
    pub fn new(uuid: Uuid, name: impl Into<String>) -> BoardTemplate {
        BoardTemplate {
            uuid,
            name: name.into(),
            description: String::new(),
            category: None,
            suggested_layers: 2,
            board: None,
            parameters: vec![],
            outline: vec![],
            cutouts: vec![],
            keepouts: vec![],
            parts: vec![],
            footprint_copies: vec![],
            connectors: vec![],
            holes: vec![],
            drawing_groups: vec![],
            drawings: vec![],
        }
    }
}

/// Board properties a template sets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateBoard {
    /// The board's `edge_connector` property.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_connector: Option<TemplateEdgeConnector>,
}

/// The `edge_connector` board property, with expression-valued numbers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateEdgeConnector {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bevel: Option<TemplateBevel>,
    /// Selective hard gold on the Edge Connector Pads; left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hard_gold: bool,
}

/// A card-edge bevel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateBevel {
    /// An angle.
    pub angle: TemplateNumber,
    /// A length.
    pub depth: TemplateNumber,
}

/// One parameter the dialog shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemplateParameter {
    /// One of a fixed set of options, each binding numeric names.
    Choice(ChoiceParameter),
    /// A free length between `min` and `max`.
    Length(LengthParameter),
}

impl TemplateParameter {
    pub fn name(&self) -> &str {
        match self {
            TemplateParameter::Choice(c) => &c.name,
            TemplateParameter::Length(l) => &l.name,
        }
    }

    /// The gate that hides it in the dialog.
    pub fn when(&self) -> Option<&When> {
        match self {
            TemplateParameter::Choice(c) => c.when.as_ref(),
            TemplateParameter::Length(l) => l.when.as_ref(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChoiceParameter {
    /// `snake_case`, unique across the template's names.
    pub name: String,
    /// What the dialog shows.
    pub label: String,
    /// An option `key`; left out, the first option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    pub options: Vec<ChoiceOption>,
    /// Hides the parameter in the dialog unless it holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "when_schema")]
    pub when: Option<When>,
}

impl ChoiceParameter {
    /// The default option's key: `default`, else the first option's.
    pub fn default_key(&self) -> Option<&str> {
        self.default
            .as_deref()
            .or_else(|| self.options.first().map(|o| o.key.as_str()))
    }
}

/// One option of a choice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChoiceOption {
    pub key: String,
    pub label: String,
    /// The numeric names this option binds: lengths (an integer is nanometres) or angles
    /// (with an angle unit, such as `"20deg"`). Every option of a choice binds the same
    /// names.
    #[serde(default)]
    #[schemars(schema_with = "bind_schema")]
    pub bind: BTreeMap<String, TemplateNumber>,
    /// Overrides the template's `suggested_layers`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 2, max = 32))]
    pub suggested_layers: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LengthParameter {
    /// `snake_case`, unique across the template's names.
    pub name: String,
    /// What the dialog shows.
    pub label: String,
    /// A plain value.
    pub default: TemplateNumber,
    /// A plain value, or an expression over the names choice options bind, so the range
    /// can follow the options chosen (a PCIe card starts at its lane width's tab end).
    pub min: TemplateNumber,
    /// As `min`.
    pub max: TemplateNumber,
    /// How the dialog shows it. It does not change how the file is read.
    pub unit: DisplayUnit,
    /// Hides the parameter in the dialog unless it holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "when_schema")]
    pub when: Option<When>,
}

/// The unit a length parameter is shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DisplayUnit {
    Mm,
    Mil,
    In,
}

/// The Board Outline or a Board Cutout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateContour {
    pub uuid: Uuid,
    /// Each vertex holds the edge that leaves it; the last joins the first.
    pub vertices: Vec<TemplateVertex>,
    /// Created Locked; left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "when_schema")]
    pub when: Option<When>,
}

/// One contour vertex and the edge that leaves it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateVertex {
    pub x: TemplateNumber,
    pub y: TemplateNumber,
    /// The edge to the next vertex: left out for a straight edge, otherwise an arc
    /// sweeping this angle, counter-clockwise when positive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sweep: Option<TemplateNumber>,
    /// Marks the edge this vertex starts as a Plated Edge, with no Net. Only on the
    /// outline and cutouts; left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub plated: bool,
}

/// A copper Layer named symbolically, since a template has no Layer UUIDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateCopperLayer {
    AllCopper,
    TopCopper,
    BottomCopper,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateKeepout {
    pub uuid: Uuid,
    pub layer: TemplateCopperLayer,
    /// Any of `tracks`, `vias`, `zones`, `smd_pads`, `tht_pads` and `footprints`.
    pub restrict: BTreeSet<Restriction>,
    /// `polygon` or `line`.
    pub shape: KeepoutShapeKind,
    pub vertices: Vec<TemplateVertex>,
    /// A line's width; only for `line`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<TemplateNumber>,
    /// A connector or hole row this Keepout goes with when it is excluded or gated out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protects: Option<Uuid>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "when_schema")]
    pub when: Option<When>,
}

/// A connector: any Part placed as a linked Component.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplatePlacement {
    pub uuid: Uuid,
    /// A `parts` entry UUID.
    pub part: Uuid,
    /// A `footprint_copies` UUID: the Footprint it uses.
    pub footprint: Uuid,
    /// Where the Footprint origin goes.
    pub at: TemplatePoint,
    /// Counter-clockwise, applied after flipping to the bottom.
    pub rotation: TemplateNumber,
    pub side: Side,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "when_schema")]
    pub when: Option<When>,
}

/// A hole: a Mechanical Part row (`part`, `footprint` and `side`, as a connector) or a
/// Free Pad row (`pad`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateHole {
    pub uuid: Uuid,
    /// A Mechanical Part row's `parts` entry UUID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<Uuid>,
    /// A Mechanical Part row's `footprint_copies` UUID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Uuid>,
    /// A Mechanical Part row's side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    /// A Free Pad row's Pad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad: Option<TemplatePad>,
    /// The Footprint origin, or the Free Pad's anchor.
    pub at: TemplatePoint,
    /// Counter-clockwise; left out for 0.
    #[serde(default, skip_serializing_if = "TemplateNumber::is_zero")]
    pub rotation: TemplateNumber,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "when_schema")]
    pub when: Option<When>,
}

/// A Free Pad's whole Pad model, with expression-valued sizes. It has no Net.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplatePad {
    pub kind: PadKind,
    /// A plated through-hole Pad the Board Outline cuts through: a Castellated Pad.
    #[serde(default, skip_serializing_if = "is_false")]
    pub castellated: bool,
    pub shape: PadShape,
    pub width: TemplateNumber,
    pub height: TemplateNumber,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_ratio: Option<Ratio>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chamfer_ratio: Option<Ratio>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corners: Option<Corners>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<TemplatePoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole: Option<TemplateDrill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middle: Option<PadCopperShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<PadCopperShape>,
    #[serde(default, skip_serializing_if = "UnusedLayers::is_keep")]
    pub unused_layers: UnusedLayers,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<Mask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paste: Option<Paste>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_connection: Option<ZoneConnection>,
    /// False opts the land out of derived Teardrops.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub teardrop: bool,
}

/// A Free Pad's drilled hole.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateDrill {
    pub width: TemplateNumber,
    pub height: TemplateNumber,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<TemplateNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<HoleTolerance>,
}

/// One part-table entry: a copied Part with its Symbol (a Mechanical Part may have none)
/// and the template's Footprint copies of its alternates. No Library reference: the Part
/// is found by the normal registered-Library lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplatePart {
    /// The entry's own UUID, which connector and hole rows name.
    pub uuid: Uuid,
    /// The content hash of the Part with its Symbol and Footprints, as copied.
    pub hash: String,
    pub part: Part,
    pub symbol: Option<Symbol>,
    /// The template's Footprint copy of each alternate, in the order of `part.footprints`.
    pub footprints: Vec<Uuid>,
}

/// A named group of drawings on one Layer: one Includes tick in the dialog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateDrawingGroup {
    pub uuid: Uuid,
    pub name: String,
    pub layer: DrawingLayer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "when_schema")]
    pub when: Option<When>,
}

/// The Layer a drawing group goes on, named symbolically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrawingLayer {
    Mechanical,
    Documentation,
}

/// What a drawing row draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrawingKind {
    Line,
    Arc,
    Rectangle,
    Text,
    Dimension,
}

/// One drawing. Which fields it needs depends on `kind`:
///
/// - `line`: `from`, `to` and `width`; `arc` adds `sweep`; `rectangle` takes `from`, `to`,
///   `width` and `filled`.
/// - `text`: `text`, `position` and `height`, with an optional `rotation`.
/// - `dimension`: ends `a` and `b`, with optional `rotation` (the direction measured
///   along), `offset`, `prefix` and `suffix`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateDrawing {
    pub uuid: Uuid,
    /// The drawing group's UUID.
    pub group: Uuid,
    pub kind: DrawingKind,
    /// The pen width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<TemplateNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<TemplatePoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<TemplatePoint>,
    /// An arc's sweep from `from` to `to`, counter-clockwise when positive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sweep: Option<TemplateNumber>,
    /// A filled rectangle; left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub filled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A text's anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<TemplatePoint>,
    /// A text's height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<TemplateNumber>,
    /// A text's rotation, or the direction a Dimension measures along.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<TemplateNumber>,
    /// A Dimension's first end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a: Option<TemplateDimensionEnd>,
    /// A Dimension's second end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<TemplateDimensionEnd>,
    /// How far a Dimension's line sits to the left of `a`, looking along `rotation`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<TemplateNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "when_schema")]
    pub when: Option<When>,
}

/// A Dimension end: a fixed point (`x`, `y`), or one that follows a template object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateDimensionEnd {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<TemplateNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<TemplateNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<TemplateFollow>,
}

/// The template object a Dimension end follows: an outline or cutout vertex (with
/// `vertex`), a Keepout vertex, or a hole's or connector's origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateFollow {
    pub object: Uuid,
    /// A vertex index, counted from 0, for a contour or Keepout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertex: Option<usize>,
}

fn when_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "description": "A gate: choice parameter names, each with the option keys under \
                        which the row exists.",
        "type": "object",
        "patternProperties": {
            "^[a-z][a-z0-9_]*$": { "type": "array", "items": { "type": "string" } }
        },
        "additionalProperties": false
    })
}

fn bind_schema(generator: &mut SchemaGenerator) -> Schema {
    let number = generator.subschema_for::<TemplateNumber>();
    json_schema!({
        "type": "object",
        "patternProperties": { "^[a-z][a-z0-9_]*$": number },
        "additionalProperties": false
    })
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_true(value: &bool) -> bool {
    *value
}

fn yes() -> bool {
    true
}
