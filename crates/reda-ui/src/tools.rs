//! The active Tool: what the left mouse button does on a canvas. It is view state, changed
//! only by the view Command `tool.activate` (with a Preset per Tool, which the Active Bars
//! and keys bind to), so scripts and MCP switch Tools the way the window does.
//!
//! Editors read [`Shell::tool`](crate::shell::Shell::tool) and keep what a Tool has in
//! flight themselves; a Tool that is no longer active drops it.

use reda_engine::command::{
    Command, CommandError, Context, Kind, Meta, NoParams, Preset, Registry,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use reda_engine::model::Uuid;

use crate::selection::Selection;

/// A canvas Tool.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    /// Click and box selection; dragging the selection moves it (keeping connections, or
    /// detached with Mod).
    #[default]
    Select,
    /// The selection follows the pointer, detached from its Wires, until a click puts it
    /// down (M).
    Move,
    /// The selection follows the pointer with its Wires rubber-banding, until a click puts
    /// it down (D).
    Drag,
    /// Draws the Board Outline as a polygon: each click adds a corner, a click on the
    /// first (or the last again) or Enter closes it.
    BoardOutline,
    /// Draws the Board Outline as a rectangle between two corners.
    BoardOutlineRect,
    /// Draws a Board Cutout as a polygon.
    BoardCutout,
    /// Draws a Zone on the active copper Layer as a polygon (Z).
    Zone,
    /// Draws a Keepout on every copper Layer as a polygon (K).
    Keepout,
    /// Places text on the active Layer.
    Text,
    /// Draws a line on the active Layer.
    Line,
    /// Draws an arc on the active Layer: its two ends, then a point it passes through.
    Arc,
    /// Draws a rectangle on the active Layer between two corners.
    Rect,
    /// Places Free Pads.
    FreePad,
    /// Places (or moves) the Placement Origin.
    PlacementOrigin,
    /// Sets the user origin the cursor position is measured from.
    UserOrigin,
    /// Measures between two points.
    Measure,
    /// Places a Mechanical Part (`part` names it), as the Library panel starts it.
    PlaceMechanical,
}

impl Tool {
    /// The Tool's id, as `tool.activate` takes it.
    pub fn id(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Move => "move",
            Tool::Drag => "drag",
            Tool::BoardOutline => "board_outline",
            Tool::BoardOutlineRect => "board_outline_rect",
            Tool::BoardCutout => "board_cutout",
            Tool::Zone => "zone",
            Tool::Keepout => "keepout",
            Tool::Text => "text",
            Tool::Line => "line",
            Tool::Arc => "arc",
            Tool::Rect => "rect",
            Tool::FreePad => "free_pad",
            Tool::PlacementOrigin => "placement_origin",
            Tool::UserOrigin => "user_origin",
            Tool::Measure => "measure",
            Tool::PlaceMechanical => "place_mechanical",
        }
    }

    /// Whether the Tool draws or places on the board (see [`crate::pcb::draw`]).
    pub fn draws_on_board(self) -> bool {
        !matches!(self, Tool::Select | Tool::Move | Tool::Drag)
    }
}

/// The PCB drawing Tools with a Preset each: (Tool, title, icon).
const BOARD_TOOLS: [(Tool, &str, &str); 13] = [
    (Tool::BoardOutline, "Board Outline", "board_outline"),
    (
        Tool::BoardOutlineRect,
        "Board Outline Rectangle",
        "board_outline_rect",
    ),
    (Tool::BoardCutout, "Board Cutout", "board_cutout"),
    (Tool::Zone, "Zone", "zone"),
    (Tool::Keepout, "Keepout", "keepout"),
    (Tool::Text, "Text", "text"),
    (Tool::Line, "Line", "line"),
    (Tool::Arc, "Arc", "arc"),
    (Tool::Rect, "Rectangle", "rectangle"),
    (Tool::FreePad, "Free Pad", "pad"),
    (Tool::PlacementOrigin, "Placement Origin", "origin"),
    (Tool::UserOrigin, "User Origin", "user_origin"),
    (Tool::Measure, "Measure", "measure"),
];

/// The active Tool, in the engine's view state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActiveTool(pub Tool);

/// Adds `tool.activate`, `tool.rotate`, `tool.flip`, `tool.undo_segment` and
/// `tool.finish` to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(ToolActivate);
    registry.add(ToolRotate);
    registry.add(ToolFlip);
    registry.add(ToolUndoSegment);
    registry.add(ToolFinish);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolActivateParams {
    /// The Tool to make active.
    tool: Tool,
    /// The Mechanical Part `place_mechanical` places, by UUID.
    #[serde(default)]
    part: Option<Uuid>,
}

/// The Tool now active.
#[derive(Serialize, JsonSchema)]
pub struct ToolActivateResult {
    tool: Tool,
    /// The Mechanical Part it places.
    #[serde(skip_serializing_if = "Option::is_none")]
    part: Option<Uuid>,
}

struct ToolActivate;

impl Command for ToolActivate {
    const META: Meta = Meta {
        id: "tool.activate",
        title: "Activate Tool",
        description: "Makes a canvas Tool active. Move and Drag pick up the selection, which \
                      then follows the pointer until a click puts it down; Select (also \
                      what Escape returns to) cancels a Tool in flight.",
        category: "tool",
        icon: "select",
        kind: Kind::View,
    };
    type Params = ToolActivateParams;
    type Output = ToolActivateResult;

    fn presets(&self) -> Vec<Preset> {
        let mut presets = vec![
            Preset::new("select", "Select", "select", json!({"tool": "select"})),
            Preset::new("move", "Move", "move", json!({"tool": "move"})),
            Preset::new("drag", "Drag", "drag", json!({"tool": "drag"})),
        ];
        for (tool, title, icon) in BOARD_TOOLS {
            let id = tool.id();
            presets.push(Preset::new(id, title, icon, json!({ "tool": id })));
        }
        presets
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ToolActivateParams,
    ) -> Result<ToolActivateResult, CommandError> {
        if matches!(p.tool, Tool::Move | Tool::Drag) && cx.view::<Selection>()?.objects.is_empty() {
            return Err(CommandError::rejected(
                "nothing is selected: select what to move first",
            ));
        }
        let part = match (p.tool, p.part) {
            (Tool::PlaceMechanical, None) => {
                return Err(CommandError::invalid_params(
                    "/part",
                    "place_mechanical needs the Mechanical Part to place",
                ));
            }
            (Tool::PlaceMechanical, part) => part,
            (_, Some(_)) => {
                return Err(CommandError::invalid_params(
                    "/part",
                    "only place_mechanical takes a part",
                ));
            }
            (_, None) => None,
        };
        cx.view::<ActiveTool>()?.0 = p.tool;
        Ok(ToolActivateResult { tool: p.tool, part })
    }
}

/// What `tool.rotate`, `tool.flip`, `tool.undo_segment` and `tool.finish` return.
#[derive(Serialize, JsonSchema)]
pub struct Turned {}

/// `tool.rotate` (Space): turns what the active Tool carries by 90°, or else the
/// selection. What a Tool carries lives in the window, so the window's Shell applies it
/// after the Command runs (the PCB Tool: `Shell::pcb_rotate`); the Command records the
/// request, so keys, menus and the Palette call it like any other.
struct ToolRotate;

impl Command for ToolRotate {
    const META: Meta = Meta {
        id: "tool.rotate",
        title: "Rotate",
        description: "Turns what the active Tool carries by 90°, or else the selection, on \
                      the active board.",
        category: "tool",
        icon: "rotate",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, _: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        Ok(Turned {})
    }
}

/// `tool.flip` (L): flips what the active Tool carries, or else the selection, to the
/// other side of the board; applied by the window's Shell as [`ToolRotate`] is.
struct ToolFlip;

impl Command for ToolFlip {
    const META: Meta = Meta {
        id: "tool.flip",
        title: "Flip Side",
        description: "Flips what the active Tool carries, or else the selection, to the \
                      other side of the active board.",
        category: "tool",
        icon: "flip",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, _: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        Ok(Turned {})
    }
}

/// `tool.undo_segment` (Backspace): takes back the last corner or point the active Tool
/// has put down; applied by the window's Shell as [`ToolRotate`] is.
struct ToolUndoSegment;

impl Command for ToolUndoSegment {
    const META: Meta = Meta {
        id: "tool.undo_segment",
        title: "Remove Last Point",
        description: "Takes back the last corner or point the active Tool has put down.",
        category: "tool",
        icon: "undo",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, _: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        Ok(Turned {})
    }
}

/// `tool.finish` (Enter): finishes the shape the active Tool is drawing, such as closing
/// a polygon; applied by the window's Shell as [`ToolRotate`] is.
struct ToolFinish;

impl Command for ToolFinish {
    const META: Meta = Meta {
        id: "tool.finish",
        title: "Finish Shape",
        description: "Finishes the shape the active Tool is drawing, such as closing a \
                      polygon.",
        category: "tool",
        icon: "select",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, _: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        Ok(Turned {})
    }
}
