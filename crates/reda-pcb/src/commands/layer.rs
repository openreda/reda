//! Naming a Layer in params, and which Layers take free text and graphics.

use std::borrow::Cow;

use reda_command::CommandError;
use reda_model::{Layer, LayerKind, PcbDocument, Side, Uuid};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer};

/// A Layer param: its UUID, or its kind with the side for sided kinds, such as
/// `silkscreen_top`, `copper_bottom` or `mechanical`, or a copper Layer by its copper
/// number, such as `copper_l2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerRef {
    Uuid(Uuid),
    Named(LayerKind, Option<Side>),
    /// The copper Layer with this copper number, 1 for the top copper.
    Copper(usize),
}

const KINDS: [(&str, LayerKind); 9] = [
    ("copper", LayerKind::Copper),
    ("silkscreen", LayerKind::Silkscreen),
    ("solder_mask", LayerKind::SolderMask),
    ("paste", LayerKind::Paste),
    ("courtyard", LayerKind::Courtyard),
    ("assembly", LayerKind::Assembly),
    ("board_outline", LayerKind::BoardOutline),
    ("mechanical", LayerKind::Mechanical),
    ("documentation", LayerKind::Documentation),
];

impl LayerRef {
    fn parse(text: &str) -> Option<LayerRef> {
        if let Ok(uuid) = text.parse() {
            return Some(LayerRef::Uuid(uuid));
        }
        if let Some(number) = text.strip_prefix("copper_l") {
            return number.parse().ok().filter(|n| *n > 0).map(LayerRef::Copper);
        }
        let (kind, side) = match (text.strip_suffix("_top"), text.strip_suffix("_bottom")) {
            (Some(kind), _) => (kind, Some(Side::Top)),
            (_, Some(kind)) => (kind, Some(Side::Bottom)),
            _ => (text, None),
        };
        let kind = KINDS.iter().find(|(name, _)| *name == kind)?.1;
        // Copper with no side is inner copper, which is named by its number instead.
        if kind == LayerKind::Copper && side.is_none() {
            return None;
        }
        Some(LayerRef::Named(kind, side))
    }

    /// The Layer this names in `pcb`; `pointer` names the param in the error.
    pub fn resolve<'p>(
        self,
        pcb: &'p PcbDocument,
        pointer: &str,
    ) -> Result<&'p Layer, CommandError> {
        let layers = pcb.layers();
        let found = match self {
            LayerRef::Uuid(uuid) => layers.layer(uuid),
            LayerRef::Named(kind, side) => layers.find(kind, side),
            LayerRef::Copper(number) => pcb.copper_layers().nth(number - 1),
        };
        found.ok_or_else(|| CommandError::invalid_params(pointer, "the board has no such Layer"))
    }
}

impl<'de> Deserialize<'de> for LayerRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<LayerRef, D::Error> {
        let text = String::deserialize(d)?;
        LayerRef::parse(&text).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "{text:?} is neither a Layer UUID nor a Layer name such as silkscreen_top"
            ))
        })
    }
}

impl JsonSchema for LayerRef {
    fn schema_name() -> Cow<'static, str> {
        "LayerRef".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "A Layer: its UUID, its kind followed by _top or _bottom for sided kinds, such as \"silkscreen_top\", \"copper_bottom\" or \"mechanical\", or a copper Layer by its copper number, such as \"copper_l1\" for the top copper."
        })
    }
}

/// Whether free text and graphics may be drawn on this Layer: copper, silkscreen,
/// mechanical and documentation.
pub fn takes_graphics(layer: &Layer) -> bool {
    matches!(
        layer.kind,
        LayerKind::Copper
            | LayerKind::Silkscreen
            | LayerKind::Mechanical
            | LayerKind::Documentation
    )
}

/// The Layer a text or graphic param names, checked to take free text and graphics.
pub fn drawing_layer<'p>(
    pcb: &'p PcbDocument,
    layer: LayerRef,
    pointer: &str,
) -> Result<&'p Layer, CommandError> {
    let layer = layer.resolve(pcb, pointer)?;
    if !takes_graphics(layer) {
        return Err(CommandError::invalid_params(
            pointer,
            "text and graphics go on copper, silkscreen, mechanical or documentation Layers",
        ));
    }
    Ok(layer)
}
