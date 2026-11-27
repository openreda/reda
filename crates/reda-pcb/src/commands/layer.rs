//! Naming a Layer in params, and which Layers take free text and graphics.

use std::borrow::Cow;

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::{
    CopperRole, InDocument, Layer, LayerKind, Object, PcbDocument, Purpose, Side, Uuid,
};
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

const KINDS: [(&str, LayerKind); 8] = [
    ("copper", LayerKind::Copper),
    ("silkscreen", LayerKind::Silkscreen),
    ("solder_mask", LayerKind::SolderMask),
    ("paste", LayerKind::Paste),
    ("courtyard", LayerKind::Courtyard),
    ("assembly", LayerKind::Assembly),
    ("board_outline", LayerKind::BoardOutline),
    ("mechanical", LayerKind::Mechanical),
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

/// Whether free text and graphics may be drawn on this Layer: copper, silkscreen and
/// Mechanical.
pub fn takes_graphics(layer: &Layer) -> bool {
    matches!(
        layer.kind,
        LayerKind::Copper | LayerKind::Silkscreen | LayerKind::Mechanical
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
            "text and graphics go on copper, silkscreen or Mechanical Layers",
        ));
    }
    Ok(layer)
}

pub(super) struct LayerSetCommand;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerSetParams {
    pub layer: LayerRef,
    /// An inner copper Layer's name, where null or blank clears it; or a Mechanical
    /// Layer's name, which can't be cleared and is unique among the Mechanical Layers.
    #[serde(default, deserialize_with = "super::nullable")]
    #[schemars(with = "Option<String>")]
    pub name: Option<Option<String>>,
    /// The copper Layer's role in the Stackup.
    pub role: Option<CopperRole>,
    /// A Mechanical Layer's Purpose. Its objects stay as they are.
    pub purpose: Option<Purpose>,
    /// A Mechanical Layer's side, `top` or `bottom`; null makes it unsided. Text on it
    /// is mirrored on the bottom and unmirrored elsewhere.
    #[serde(default, deserialize_with = "super::nullable")]
    #[schemars(with = "Option<Side>")]
    pub side: Option<Option<Side>>,
}

impl Command for LayerSetCommand {
    const META: Meta = Meta {
        id: "pcb.layer.set",
        title: "Set Layer",
        description: "Sets a copper Layer's role or an inner Layer's name (null clears it), or a \
                      Mechanical Layer's name, Purpose or side.",
        category: "pcb",
        icon: "layers",
        kind: Kind::Edit,
    };
    type Params = LayerSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        super::needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: Self::Params) -> Result<(), CommandError> {
        let pcb = super::pcb(cx)?;
        let document = pcb.uuid;
        let layer = p.layer.resolve(pcb, "/layer")?;
        let mut layers = pcb.layers().clone();
        let mut texts = Vec::new();
        match layer.kind {
            LayerKind::Copper => {
                for (given, pointer) in [
                    (p.purpose.is_some(), "/purpose"),
                    (p.side.is_some(), "/side"),
                ] {
                    if given {
                        return Err(CommandError::invalid_params(
                            pointer,
                            "only a Mechanical Layer has a Purpose or a side to set",
                        ));
                    }
                }
                let edit = layers
                    .layers
                    .iter_mut()
                    .find(|l| l.uuid == layer.uuid)
                    .unwrap();
                if let Some(name) = p.name {
                    let name = name.filter(|s| !s.trim().is_empty());
                    if layer.side.is_some() && name.is_some() {
                        return Err(CommandError::rejected(
                            "the outer copper Layers have fixed names",
                        ));
                    }
                    edit.name = name;
                }
                if let Some(role) = p.role {
                    edit.role = role;
                }
            }
            LayerKind::Mechanical => {
                if p.role.is_some() {
                    return Err(CommandError::invalid_params(
                        "/role",
                        "only a copper Layer has a role",
                    ));
                }
                let name = match p.name {
                    Some(None) => {
                        return Err(CommandError::invalid_params(
                            "/name",
                            "a Mechanical Layer's name can't be cleared",
                        ));
                    }
                    Some(Some(name)) => Some(super::layer_count::mechanical::checked_name(
                        &layers,
                        &name,
                        Some(layer.uuid),
                        "/name",
                    )?),
                    None => None,
                };
                let edit = layers
                    .layers
                    .iter_mut()
                    .find(|l| l.uuid == layer.uuid)
                    .unwrap();
                if let Some(name) = name {
                    edit.name = Some(name);
                }
                if let Some(purpose) = p.purpose {
                    edit.purpose = purpose;
                }
                if let Some(side) = p.side {
                    edit.side = side;
                    let mirrored = side == Some(Side::Bottom);
                    texts = pcb
                        .texts()
                        .filter(|t| t.layer == layer.uuid && t.mirrored != mirrored)
                        .map(|t| {
                            let mut t = t.clone();
                            t.mirrored = mirrored;
                            Object::PcbText(InDocument::new(document, t))
                        })
                        .collect();
                }
            }
            _ => {
                return Err(CommandError::invalid_params(
                    "/layer",
                    "choose a copper or Mechanical Layer",
                ));
            }
        }
        let tx = cx.tx()?;
        tx.replace(Object::LayerSet(InDocument::new(document, layers)))?;
        for text in texts {
            tx.replace(text)?;
        }
        Ok(())
    }
}
