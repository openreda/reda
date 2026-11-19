//! Per-document cross-section and table selection shared by the GUI and bridge.
use std::collections::BTreeMap;

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::{PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum Scale {
    #[default]
    Physical,
    Schematic,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Display {
    pub selected: Option<Uuid>,
    pub scale: Scale,
}

#[derive(Default)]
pub(crate) struct Displays(BTreeMap<Uuid, Display>);

impl Displays {
    pub(super) fn get(&self, document: Uuid) -> Display {
        self.0.get(&document).copied().unwrap_or_default()
    }
}

pub(crate) fn register(registry: &mut Registry) {
    registry.add(Select);
    registry.add(SetScale);
}

fn board<'a>(cx: &'a Context<'_>, id: Uuid) -> Result<&'a PcbDocument, CommandError> {
    cx.project()?
        .pcb_document(id)
        .ok_or_else(|| CommandError::not_found("no such PCB Document"))
}

pub(super) const SELECT: &str = "pcb.stackup.select";
pub(super) const SCALE: &str = "pcb.stackup.scale";

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Selection {
    document: Uuid,
    /// Copper Layer, Dielectric or Via type. Null clears the selection.
    entry: Option<Uuid>,
}

struct Select;
impl Command for Select {
    const META: Meta = Meta {
        id: SELECT,
        title: "Select Stackup row",
        description: "Selects a copper Layer, Dielectric or Via type in the Stackup tab and its cross-section.",
        category: "pcb",
        icon: "layers",
        kind: Kind::View,
    };
    type Params = Selection;
    type Output = ();
    fn run(&self, cx: &mut Context<'_>, p: Selection) -> Result<(), CommandError> {
        let stack = board(cx, p.document)?.stackup();
        if let Some(id) = p.entry
            && !stack.entries.iter().any(|e| e.uuid() == id)
            && !stack.via_types.iter().any(|t| t.uuid == id)
        {
            return Err(CommandError::invalid_params(
                "/entry",
                "choose a row in this Stackup",
            ));
        }
        cx.view::<Displays>()?
            .0
            .entry(p.document)
            .or_default()
            .selected = p.entry;
        Ok(())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ScaleParams {
    document: Uuid,
    scale: Scale,
}

struct SetScale;
impl Command for SetScale {
    const META: Meta = Meta {
        id: SCALE,
        title: "Set Stackup scale",
        description: "Shows the cross-section to physical scale or at schematic band heights.",
        category: "pcb",
        icon: "layers",
        kind: Kind::View,
    };
    type Params = ScaleParams;
    type Output = ();
    fn run(&self, cx: &mut Context<'_>, p: ScaleParams) -> Result<(), CommandError> {
        board(cx, p.document)?;
        cx.view::<Displays>()?
            .0
            .entry(p.document)
            .or_default()
            .scale = p.scale;
        Ok(())
    }
}
