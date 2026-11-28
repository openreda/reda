//! Where each DXF layer goes: a [`Target`] given in the `mapping` param, or the first
//! pre-fill [`Rule`] that matches (docs/spec/v0.3/dxf.md#layer-mapping).

use std::borrow::Cow;

use reda_model::{Layer, LayerKind, LayerSet, PcbDocument, Purpose, Uuid};
use reda_pcb::LayerRef;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::IntoDeserializer;
use serde::{Deserialize, Deserializer, Serialize};

/// A DXF layer's target, as the `mapping` param names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Not imported.
    Skip,
    /// Made into the Board Outline.
    BoardOutline,
    /// A new Mechanical Layer named after the DXF layer, with the Purpose Reference.
    NewMechanical,
    /// The board's first Mechanical Layer with the Purpose Reference, added if it has none.
    Reference,
    /// A board Layer that takes free graphics.
    Layer(LayerRef),
}

impl Target {
    /// The word for each target that is not a Layer.
    const WORDS: [(&str, Target); 4] = [
        ("skip", Target::Skip),
        ("board_outline", Target::BoardOutline),
        ("new_mechanical", Target::NewMechanical),
        ("reference", Target::Reference),
    ];

    fn parse(text: &str) -> Result<Target, String> {
        if let Some((_, target)) = Target::WORDS.iter().find(|(w, _)| *w == text) {
            return Ok(*target);
        }
        let d: serde::de::value::StrDeserializer<'_, serde::de::value::Error> =
            text.into_deserializer();
        LayerRef::deserialize(d)
            .map(Target::Layer)
            .map_err(|e| e.to_string())
    }

    /// How a row's target is written back: a word, or the Layer's UUID.
    pub(crate) fn word(self) -> Option<&'static str> {
        Target::WORDS
            .iter()
            .find(|(_, t)| *t == self)
            .map(|(w, _)| *w)
    }
}

impl<'de> Deserialize<'de> for Target {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Target, D::Error> {
        let text = String::deserialize(d)?;
        Target::parse(&text).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Target {
    fn schema_name() -> Cow<'static, str> {
        "DxfTarget".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "Where a DXF layer goes: \"skip\"; \"board_outline\"; \"new_mechanical\", a new Mechanical Layer named after the DXF layer with the Purpose Reference; \"reference\", the first Mechanical Layer with the Purpose Reference, added if there is none; or a copper, silkscreen, Courtyard, Assembly or Mechanical Layer by UUID or name, such as \"silkscreen_top\"."
        })
    }
}

/// The pre-fill rule that chose a row's target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// The DXF layer has a board Layer's name, in any case.
    SameName,
    /// Its name contains "outline", "edge", "board", "profile" or "contour".
    Outline,
    /// Its name suggests a Purpose, such as "dim" for Dimensions.
    Purpose,
    /// Anything else: the first Reference Layer.
    Reference,
}

/// Whether a DXF layer may land on this board Layer: one that takes free graphics.
pub fn takes_drawing(layer: &Layer) -> bool {
    matches!(
        layer.kind,
        LayerKind::Copper
            | LayerKind::Silkscreen
            | LayerKind::Courtyard
            | LayerKind::Assembly
            | LayerKind::Mechanical
    )
}

const OUTLINE_WORDS: [&str; 5] = ["outline", "edge", "board", "profile", "contour"];

/// The words that suggest each Purpose, matched anywhere in the name.
const PURPOSE_WORDS: [(&str, Purpose); 5] = [
    ("dim", Purpose::Dimensions),
    ("fab", Purpose::FabricationNotes),
    ("assy", Purpose::AssemblyNotes),
    ("assembly", Purpose::AssemblyNotes),
    ("ref", Purpose::Reference),
];

/// The pre-filled target of the DXF layer `name`. `outline_free` is false once another
/// row has the Board Outline. A Layer target names it by UUID.
pub fn prefill(pcb: &PcbDocument, name: &str, outline_free: bool) -> (Target, Rule) {
    let layers = pcb.layers();
    let candidates = || {
        layers
            .layers
            .iter()
            .filter(|l| takes_drawing(l) || (outline_free && l.kind == LayerKind::BoardOutline))
    };
    let name = name.trim();
    let same = candidates()
        .find(|l| pcb.layer_title(l) == name)
        .or_else(|| candidates().find(|l| pcb.layer_title(l).eq_ignore_ascii_case(name)));
    if let Some(layer) = same {
        let target = match layer.kind {
            LayerKind::BoardOutline => Target::BoardOutline,
            _ => Target::Layer(LayerRef::Uuid(layer.uuid)),
        };
        return (target, Rule::SameName);
    }
    let lower = name.to_lowercase();
    if outline_free && OUTLINE_WORDS.iter().any(|w| lower.contains(w)) {
        return (Target::BoardOutline, Rule::Outline);
    }
    let suggested = PURPOSE_WORDS
        .iter()
        .filter(|(word, _)| lower.contains(word))
        .find_map(|(_, purpose)| layers.first_with_purpose(*purpose));
    if let Some(layer) = suggested {
        return (Target::Layer(LayerRef::Uuid(layer.uuid)), Rule::Purpose);
    }
    (Target::Reference, Rule::Reference)
}

/// The UUID of the board's first Reference Layer, as `board` has its Layers, or of the
/// one added at the end of `layers` when it has none. `added_reference` remembers that
/// one, so every row shares it rather than a New Mechanical Layer that took the Purpose.
pub(crate) fn reference_layer(
    board: &LayerSet,
    layers: &mut LayerSet,
    added: &mut Vec<Uuid>,
    added_reference: &mut Option<Uuid>,
    new_id: &mut dyn FnMut() -> Uuid,
) -> Uuid {
    if let Some(layer) = board.first_with_purpose(Purpose::Reference) {
        return layer.uuid;
    }
    if let Some(uuid) = *added_reference {
        return uuid;
    }
    let uuid = new_id();
    *added_reference = Some(uuid);
    layers.push_mechanical(uuid, Purpose::Reference);
    added.push(uuid);
    uuid
}

/// A new Mechanical Layer named after the DXF layer `name`, with the Purpose Reference,
/// numbered on a clash with another Mechanical Layer's name ("Notes 2").
pub(crate) fn new_mechanical(
    layers: &mut LayerSet,
    name: &str,
    added: &mut Vec<Uuid>,
    new_id: &mut dyn FnMut() -> Uuid,
) -> Uuid {
    let uuid = new_id();
    let base = name.trim();
    let taken = |layers: &LayerSet, n: &str| {
        layers
            .layers
            .iter()
            .any(|l| l.kind == LayerKind::Mechanical && l.name.as_deref() == Some(n))
    };
    let name = if base.is_empty() {
        layers.new_mechanical_name(Purpose::Reference)
    } else if !taken(layers, base) {
        base.to_owned()
    } else {
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|n| !taken(layers, n))
            .expect("an unbounded count of names")
    };
    layers.push_mechanical(uuid, Purpose::Reference);
    layers.layers.last_mut().expect("just pushed").name = Some(name);
    added.push(uuid);
    uuid
}
