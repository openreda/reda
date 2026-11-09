//! The active Tool: what the left mouse button does on a canvas. It is view state, changed
//! only by the view Command `tool.activate` (with a Preset per Tool, which the Active Bars
//! and keys bind to), so scripts and MCP switch Tools the way the window does.
//!
//! Editors read [`Shell::tool`](crate::shell::Shell::tool) and keep what a Tool has in
//! flight themselves; a Tool that is no longer active drops it.

use reda_engine::command::{
    Angle, Command, CommandError, Context, Kind, Meta, NoParams, Preset, Registry,
};
use reda_engine::model::geom::Millideg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::placement::commands::edit_placing;
use crate::placement::{Placement, Placing};
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
    /// Draws a Zone Cutout on the active copper Layer as a polygon.
    ZoneCutout,
    /// Draws a Keepout as a polygon, rectangle or circle, on every copper Layer or the
    /// active one, by its options (K).
    Keepout,
    /// Places text on the active Layer.
    Text,
    /// Draws a line on the active Layer (on a Footprint: the drawing Layer, by a drag).
    Line,
    /// Draws an arc on the active Layer: its two ends, then a point it passes through.
    Arc,
    /// Draws a rectangle on the active Layer between two corners (on a Footprint: the
    /// drawing Layer, by a drag).
    Rect,
    /// Places Free Pads.
    FreePad,
    /// Places (or moves) the Output Origin.
    OutputOrigin,
    /// Measures between two points.
    Measure,
    /// Places a linear Dimension on the active Layer: its two ends (each attached to the
    /// Hotspot under the click, or free), then where the dimension line sits.
    Dimension,
    /// Moves the selection by the offset between two picked points (Move by ΔX/ΔY).
    MoveBy,
    /// Turns the selection by `tool.rotate_about`'s angle about a picked centre.
    RotateAbout,
    /// Swaps the positions of two Footprints picked one after the other, until Escape.
    Swap,
    /// Routes Tracks on the board: clicks place corners, same-Net copper ends the route (T).
    Route,
    /// Places stitching Vias on the board, one per click (V).
    Via,
    /// A Library Part follows the pointer until a click places it (`tool.place`).
    Place,
    /// Draws orthogonal Wire runs: each click fixes a corner, and the run ends on a Pin or
    /// Wire (W).
    Wire,
    /// Cuts a gap out of the Wire under each click (`schematic.wire.break`), as long as
    /// the Cutting Length Preference says; Space cycles it, Tab opens its options.
    BreakWire,
    /// Places Net Labels, one per click (N).
    NetLabel,
    /// Places Ports, one per click (P).
    Port,
    /// Places Power Ports, one per click (B).
    PowerPort,
    /// Marks Pins as deliberately left open, one per click (Q).
    NoConnect,
    /// Footprint editor: each click places a Pad from the Pad Tool's settings (P).
    Pad,
    /// Footprint editor: a drag draws a circle from its centre.
    Circle,
    /// Footprint editor: a click moves the Footprint's origin there, then Select is back.
    Origin,
    /// Symbol editor: each click places a Pin on the 50 mil grid, numbered on (P).
    Pin,
    /// Symbol editor: clicks add corners; a click on the first, or Enter, closes it (O).
    Polygon,
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
            Tool::ZoneCutout => "zone_cutout",
            Tool::Keepout => "keepout",
            Tool::Text => "text",
            Tool::Line => "line",
            Tool::Arc => "arc",
            Tool::Rect => "rect",
            Tool::FreePad => "free_pad",
            Tool::OutputOrigin => "output_origin",
            Tool::Measure => "measure",
            Tool::Dimension => "dimension",
            Tool::MoveBy => "move_by",
            Tool::RotateAbout => "rotate_about",
            Tool::Swap => "swap",
            Tool::Route => "route",
            Tool::Via => "via",
            Tool::Place => "place",
            Tool::Wire => "wire",
            Tool::BreakWire => "break_wire",
            Tool::NetLabel => "net_label",
            Tool::Port => "port",
            Tool::PowerPort => "power_port",
            Tool::NoConnect => "no_connect",
            Tool::Pad => "pad",
            Tool::Circle => "circle",
            Tool::Origin => "origin",
            Tool::Pin => "pin",
            Tool::Polygon => "polygon",
        }
    }

    /// Whether the Tool carries the selection, so needs one to start.
    pub fn moves_selection(self) -> bool {
        matches!(self, Tool::Move | Tool::Drag)
    }

    /// Whether the Tool draws or places on the board (see [`crate::pcb::draw`]); routing
    /// and Vias have their own Tool (`pcb::route`), Library Parts theirs
    /// (`crate::placement`), and sheet and Footprint Tools their editors'.
    pub fn draws_on_board(self) -> bool {
        self == Tool::RotateAbout || BOARD_TOOLS.iter().any(|(tool, _, _)| *tool == self)
    }
}

/// The PCB drawing Tools with a Preset each: (Tool, title, icon).
const BOARD_TOOLS: [(Tool, &str, &str); 16] = [
    (Tool::BoardOutline, "Board Outline", "board_outline"),
    (
        Tool::BoardOutlineRect,
        "Board Outline Rectangle",
        "board_outline_rect",
    ),
    (Tool::BoardCutout, "Board Cutout", "board_cutout"),
    (Tool::Zone, "Zone", "zone"),
    (Tool::ZoneCutout, "Zone Cutout", "zone"),
    (Tool::Keepout, "Keepout", "keepout"),
    (Tool::Text, "Text", "text"),
    (Tool::Line, "Line", "line"),
    (Tool::Arc, "Arc", "arc"),
    (Tool::Rect, "Rectangle", "rectangle"),
    (Tool::Dimension, "Dimension", "measure"),
    (Tool::FreePad, "Free Pad", "pad"),
    (Tool::OutputOrigin, "Output Origin", "origin"),
    (Tool::Measure, "Measure", "measure"),
    (Tool::MoveBy, "Move by Two Points", "move"),
    (Tool::Swap, "Swap", "move"),
];

/// The angle the Rotate About a Point Tool turns by, from `tool.rotate_about`, in the
/// engine's view state. 90° until set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotateAboutAngle(pub Millideg);

impl Default for RotateAboutAngle {
    fn default() -> RotateAboutAngle {
        RotateAboutAngle(Millideg::from_deg(90))
    }
}

/// The active Tool, in the engine's view state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActiveTool(pub Tool);

/// The Net name the Power Port Tool starts with, from a Power Port Preset, in the
/// engine's view state. None starts it on GND.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerPortName(pub Option<String>);

/// The Wire Tool's mode, which Shift+Space (`tool.cycle_wire_mode`) cycles through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WireMode {
    /// Horizontal and vertical segments, one corner on the way to the pointer.
    #[default]
    Ortho,
    /// Horizontal, vertical and 45° segments, one corner on the way to the pointer.
    Diagonal,
    /// One straight segment to the pointer, at any angle.
    AnyAngle,
    /// A click routes from the last vertex to the clicked point around placed objects
    /// (`schematic.wire.route`).
    Auto,
}

impl WireMode {
    /// The mode after this one: 90°, 45°, Any Angle, Auto Wire, then 90° again.
    pub fn next(self) -> WireMode {
        match self {
            WireMode::Ortho => WireMode::Diagonal,
            WireMode::Diagonal => WireMode::AnyAngle,
            WireMode::AnyAngle => WireMode::Auto,
            WireMode::Auto => WireMode::Ortho,
        }
    }

    /// The mode's name as the status bar shows it.
    pub fn label(self) -> String {
        match self {
            WireMode::Ortho => crate::tr!("wire-mode-ortho"),
            WireMode::Diagonal => crate::tr!("wire-mode-diagonal"),
            WireMode::AnyAngle => crate::tr!("wire-mode-any-angle"),
            WireMode::Auto => crate::tr!("wire-mode-auto"),
        }
    }
}

/// The Wire Tool's settings for the session, in the engine's view state: its mode, and
/// Auto Wire's options, which Tab (`tool.edit_properties`) shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireTool {
    pub mode: WireMode,
    /// Auto Wire's Time Out After, in milliseconds.
    pub timeout_ms: u32,
    /// Auto Wire's Avoid cutting wires.
    pub avoid: reda_engine::schematic::Avoid,
    /// The Auto Wire options window is open.
    pub options_open: bool,
}

impl Default for WireTool {
    fn default() -> WireTool {
        WireTool {
            mode: WireMode::default(),
            timeout_ms: 3000,
            avoid: reda_engine::schematic::Avoid::default(),
            options_open: false,
        }
    }
}

/// The one-level Tool stack, in the engine's view state. A Tool with a run in flight
/// (a Wire run) is suspended when another Tool starts, and comes back with its run when
/// that Tool finishes with Esc (`tool.activate {tool: select}`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolStack {
    /// The active Tool has a run in flight; the window sets this before each
    /// `tool.activate`.
    pub run: bool,
    /// The Tool whose run waits for the active Tool to finish.
    pub suspended: Option<Tool>,
}

/// Adds `tool.activate` and the in-flight Commands (`tool.rotate`, `tool.mirror_x`,
/// `tool.mirror_y`, `tool.flip_side`, `tool.edit_properties`, `tool.cycle_corner`,
/// `tool.cycle_wire_mode`, `tool.cycle_cutting_length`, `tool.insert_vertex`,
/// `tool.undo_segment`, `tool.finish`)
/// to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(ToolActivate);
    registry.add(ToolRotate);
    registry.add(ToolMirrorX);
    registry.add(ToolMirrorY);
    registry.add(ToolFlipSide);
    registry.add(ToolEditProperties);
    registry.add(ToolCycleCorner);
    registry.add(ToolCycleWireMode);
    registry.add(ToolInsertVertex);
    registry.add(ToolCycleCuttingLength);
    registry.add(ToolAutoWireSet);
    registry.add(ToolUndoSegment);
    registry.add(ToolFinish);
    registry.add(ToolMoveBoardShape);
    registry.add(ToolMoveBoard);
    registry.add(ToolRotateAbout);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolRotateAboutParams {
    /// Counter-clockwise; any angle.
    pub angle: Angle,
}

/// `tool.rotate_about`: Rotate About a Point. Keeps the typed angle and makes the Rotate
/// About Tool active; its click picks the centre and runs `pcb.rotate` on the selection.
struct ToolRotateAbout;

impl Command for ToolRotateAbout {
    const META: Meta = Meta {
        id: "tool.rotate_about",
        title: "Rotate About a Point",
        description: "Turns the selection on the board by a typed `angle` (counter-clockwise) \
                      about a centre picked with the next click, as one `pcb.rotate`.",
        category: "tool",
        icon: "rotate",
        kind: Kind::View,
    };
    type Params = ToolRotateAboutParams;
    type Output = ToolActivateResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ToolRotateAboutParams,
    ) -> Result<ToolActivateResult, CommandError> {
        if cx.view::<Selection>()?.objects.is_empty() {
            return Err(CommandError::rejected(
                "nothing is selected: select what to turn first",
            ));
        }
        cx.view::<RotateAboutAngle>()?.0 = p.angle.millideg();
        cx.view::<ActiveTool>()?.0 = Tool::RotateAbout;
        cx.view::<Placement>()?.placing = None;
        Ok(ToolActivateResult {
            tool: Tool::RotateAbout,
            run: None,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolActivateParams {
    /// The Tool to make active.
    tool: Tool,
    /// For the Power Port Tool: the Net name it places, such as `3V3` (a Power Port
    /// Preset). GND when left out.
    name: Option<String>,
}

/// The Tool now active.
#[derive(Serialize, JsonSchema)]
pub struct ToolActivateResult {
    tool: Tool,
    /// What happened to a run in flight: `suspended` (the Tool asked for waits on top of
    /// it), `resumed` (it is back) or `ended` (Select asked for during the run: the run
    /// commits and its Tool stays).
    #[serde(skip_serializing_if = "Option::is_none")]
    run: Option<RunChange>,
}

/// What `tool.activate` did to a run in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunChange {
    Suspended,
    Resumed,
    Ended,
}

struct ToolActivate;

impl Command for ToolActivate {
    const META: Meta = Meta {
        id: "tool.activate",
        title: "Activate Tool",
        description: "Makes a canvas Tool active. Move and Drag pick up the selection, which \
                      then follows the pointer until a click puts it down; Route and Via \
                      draw copper on the board; Wire draws Wire runs, and Net Label, Port, \
                      Power Port and No-Connect place one object per click; Select (also \
                      what Escape returns to) cancels a Tool in flight. A Wire run in flight is \
                      suspended by another Tool and comes back when that Tool finishes with \
                      Select (Escape), one level deep; Select during the run itself ends \
                      the run and keeps the Wire Tool. On a Footprint, Pad \
                      places Pads, Line, Rect and Circle draw on the drawing Layer and \
                      Origin moves the origin. On a Symbol, Pin places Pins and Line, Rect, \
                      Arc, Circle, Polygon and Text draw on the shown Unit.",
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
            Preset::new("route", "Route Tracks", "track", json!({"tool": "route"})),
            Preset::new("via", "Place Vias", "via", json!({"tool": "via"})),
            Preset::new("wire", "Wire", "wire", json!({"tool": "wire"})),
            Preset::new(
                "break_wire",
                "Break Wire",
                "split",
                json!({"tool": "break_wire"}),
            ),
            Preset::new(
                "net_label",
                "Net Label",
                "net_label",
                json!({"tool": "net_label"}),
            ),
            Preset::new("port", "Port", "port", json!({"tool": "port"})),
            Preset::new(
                "power_port",
                "Power Port",
                "power_port",
                json!({"tool": "power_port"}),
            ),
            Preset::new(
                "no_connect",
                "No-Connect Marker",
                "no_connect",
                json!({"tool": "no_connect"}),
            ),
            Preset::new("pad", "Place Pad", "pad", json!({"tool": "pad"})),
            Preset::new("circle", "Draw Circle", "circle", json!({"tool": "circle"})),
            Preset::new("origin", "Set Origin", "origin", json!({"tool": "origin"})),
            Preset::new("pin", "Place Pin", "pin", json!({"tool": "pin"})),
            Preset::new(
                "polygon",
                "Draw Polygon",
                "polygon",
                json!({"tool": "polygon"}),
            ),
        ];
        for (name, _) in reda_engine::schematic::POWER_PORT_PRESETS {
            presets.push(Preset::new(
                format!("power_port_{}", name.to_lowercase()),
                format!("{name} Power Port"),
                "power_port",
                json!({"tool": "power_port", "name": name}),
            ));
        }
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
        if p.tool == Tool::Place {
            return Err(CommandError::rejected(
                "pick the Part to place in the Library panel (tool.place)",
            ));
        }
        // On the board Move needs no selection: its click picks up, or opens Choose
        // Component.
        let on_board = p.tool == Tool::Move && {
            let active = cx.view::<crate::layout::Layout>()?.active_document();
            let board = cx.project().ok().and_then(|p| p.pcb()).map(|p| p.uuid);
            active.is_some() && active == board
        };
        if p.tool.moves_selection() && !on_board && cx.view::<Selection>()?.objects.is_empty() {
            return Err(CommandError::rejected(
                "nothing is selected: select what to move first",
            ));
        }
        let current = cx.view::<ActiveTool>()?.0;
        let stack = *cx.view::<ToolStack>()?;
        let (tool, run) = if p.tool == Tool::Select && stack.run {
            // Esc during a run ends it; its Tool stays.
            (current, Some(RunChange::Ended))
        } else if let Some(suspended) = stack
            .suspended
            .filter(|s| p.tool == Tool::Select || p.tool == *s)
        {
            // Finishing the Tool on top (or asking for the waiting one) brings the run back.
            cx.view::<ToolStack>()?.suspended = None;
            (suspended, Some(RunChange::Resumed))
        } else if stack.run && p.tool != current && stack.suspended.is_none() {
            // One level deep: the run waits under the new Tool.
            cx.view::<ToolStack>()?.suspended = Some(current);
            (p.tool, Some(RunChange::Suspended))
        } else {
            (p.tool, None)
        };
        cx.view::<ToolStack>()?.run = false;
        if run == Some(RunChange::Ended) {
            return Ok(ToolActivateResult { tool, run });
        }
        cx.view::<ActiveTool>()?.0 = tool;
        cx.view::<PowerPortName>()?.0 = match tool {
            Tool::PowerPort => p.name,
            _ => None,
        };
        // Another Tool ends a placement.
        cx.view::<Placement>()?.placing = None;
        Ok(ToolActivateResult { tool, run })
    }
}

/// What `tool.rotate`, `tool.flip_side`, `tool.undo_segment` and `tool.finish` return.
#[derive(Serialize, JsonSchema)]
pub struct Turned {}

/// `tool.rotate` (Space): turns what the active Tool carries by 90°, or else the
/// selection. What a Tool carries lives in the window, so the window's Shell applies it
/// after the Command runs (the PCB Tool: `Shell::pcb_rotate`; a sheet's:
/// `schematic_editor::turn`); the Command records the request, so keys, menus and the
/// Palette call it like any other.
struct ToolRotate;

impl Command for ToolRotate {
    const META: Meta = Meta {
        id: "tool.rotate",
        title: "Rotate",
        description: "Turns what the active Tool carries by 90°, or else the selection, on \
                      the active board or sheet.",
        category: "tool",
        icon: "rotate",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        edit_placing(cx, Placing::rotate)?;
        Ok(Turned {})
    }
}

/// `tool.flip_side` (L): flips what the active Tool carries, or else the selection, to
/// the other side of the board; applied by the window's Shell as [`ToolRotate`] is.
struct ToolFlipSide;

impl Command for ToolFlipSide {
    const META: Meta = Meta {
        id: "tool.flip_side",
        title: "Flip Side",
        description: "Flips what the active Tool carries, or else the selection, to the \
                      other side of the active board.",
        category: "tool",
        icon: "flip",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        edit_placing(cx, Placing::flip)?;
        Ok(Turned {})
    }
}

/// What `tool.mirror_x` and `tool.mirror_y` return: the axis, for the Shell to apply.
#[derive(Serialize, JsonSchema)]
pub struct Mirrored {
    pub axis: MirrorAxis,
}

/// Which way a mirror flips: `x` left to right, `y` top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MirrorAxis {
    X,
    Y,
}

/// `tool.mirror_x` (X) and `tool.mirror_y` (Y): flip what the active Tool carries (a Part
/// being placed, a move or drag, a label or Port being placed), or else the selection, on
/// the active sheet. A Part being placed turns here; the rest is applied by the window's
/// Shell as [`ToolRotate`] is.
fn mirror(cx: &mut Context<'_>, axis: MirrorAxis) -> Result<Mirrored, CommandError> {
    let on_board = cx
        .view::<crate::placement::Placement>()?
        .placing
        .as_ref()
        .is_some_and(|placing| placing.target == crate::placement::Target::Board);
    if on_board {
        return Err(CommandError::rejected(
            reda_engine::pcb::place::MIRROR_REFUSED,
        ));
    }
    edit_placing(cx, |placing| placing.mirror(axis == MirrorAxis::X))?;
    Ok(Mirrored { axis })
}

/// `tool.mirror_x`: see [`mirror`].
struct ToolMirrorX;

impl Command for ToolMirrorX {
    const META: Meta = Meta {
        id: "tool.mirror_x",
        title: "Mirror X",
        description: "Flips what the active Tool carries (a Part being placed, a move or \
                      drag, a label or Port being placed), or else the selection, left to \
                      right on the active sheet. Refused on a Footprint in flight or \
                      selected on the board: mirroring makes an unbuildable part, so L \
                      (Flip Side) turns it over instead.",
        category: "tool",
        icon: "mirror",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Mirrored;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Mirrored, CommandError> {
        mirror(cx, MirrorAxis::X)
    }
}

/// `tool.mirror_y`: see [`mirror`].
struct ToolMirrorY;

impl Command for ToolMirrorY {
    const META: Meta = Meta {
        id: "tool.mirror_y",
        title: "Mirror Y",
        description: "Flips what the active Tool carries (a Part being placed, a move or \
                      drag, a label or Port being placed), or else the selection, top to \
                      bottom on the active sheet. Refused on a Footprint in flight or \
                      selected on the board, as Mirror X is.",
        category: "tool",
        icon: "mirror",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Mirrored;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Mirrored, CommandError> {
        mirror(cx, MirrorAxis::Y)
    }
}

/// `tool.move_board_shape`: Move Board Shape. The Board Outline (with any selected Board
/// Cutouts) follows the pointer until a click puts it down, as one `pcb.move`; Space and L
/// turn and flip it in flight. The Command makes Select active; the window's Shell picks
/// the outline up (`Shell::pcb_move_board_shape`).
struct ToolMoveBoardShape;

impl Command for ToolMoveBoardShape {
    const META: Meta = Meta {
        id: "tool.move_board_shape",
        title: "Move Board Shape",
        description: "Picks up the Board Outline alone, with any selected Board Cutouts: \
                      it follows the pointer, turning and flipping with the rotate and flip \
                      keys, until a click puts it down. Everything else stays; \
                      `pcb.board.move` moves the whole board.",
        category: "tool",
        icon: "move",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        let has_outline = cx
            .project()?
            .pcb()
            .is_some_and(|pcb| pcb.outline().is_some());
        if !has_outline {
            return Err(CommandError::rejected(
                "the board has no Board Outline to move",
            ));
        }
        cx.view::<ActiveTool>()?.0 = Tool::Select;
        cx.view::<Placement>()?.placing = None;
        Ok(Turned {})
    }
}

/// `tool.move_board`: Move Board. The Board Outline and its Board Cutouts follow the
/// pointer until a click puts the whole board down, as one `pcb.board.move` (which also
/// takes the Output Origin and everything with a part inside the outline). The Command
/// makes Select active; the window's Shell picks the board up (`Shell::pcb_move_board`).
struct ToolMoveBoard;

impl Command for ToolMoveBoard {
    const META: Meta = Meta {
        id: "tool.move_board",
        title: "Move Board",
        description: "Picks up the whole board: its outline follows the pointer until a \
                      click puts it down, and then `pcb.board.move` moves the Board \
                      Outline, its Board Cutouts, the Output Origin and every object with \
                      any part inside the outline by that much.",
        category: "tool",
        icon: "move",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, cx: &mut Context<'_>, p: NoParams) -> Result<Turned, CommandError> {
        ToolMoveBoardShape.run(cx, p)
    }
}

/// `tool.cycle_corner` (/): the next corner mode of the Wire run or route in flight;
/// applied by the window's Shell.
struct ToolCycleCorner;

impl Command for ToolCycleCorner {
    const META: Meta = Meta {
        id: "tool.cycle_corner",
        title: "Cycle Corner Mode",
        description: "Switches the corners of the Wire run or route in flight: a 90° Wire \
                      run between horizontal-then-vertical and vertical-then-horizontal, a \
                      45° one between straight-then-45° and 45°-then-straight; a route \
                      between straight-then-45°, 45°-then-straight, horizontal-then-vertical \
                      and vertical-then-horizontal. During a drag (Mod+Space) it turns the \
                      rubber-banded Wires' corners the other way.",
        category: "tool",
        icon: "line",
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
        description: "Takes back the last corner or point the active Tool has put down: a \
                      Wire run's last segment (with none left, the run ends without drawing \
                      anything), a route's or a shape's last corner.",
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
                      polygon. In the Wire Tool it fixes a corner where the pointer is, as \
                      a click does.",
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

/// Refuses a Tool key unless the active Tool is one of `tools`.
fn only_in(cx: &mut Context<'_>, tools: &[Tool], what: &str) -> Result<Tool, CommandError> {
    let tool = cx.view::<ActiveTool>()?.0;
    if tools.contains(&tool) {
        Ok(tool)
    } else {
        Err(CommandError::rejected(format!(
            "{what} works only while the {} Tool is active",
            tools
                .iter()
                .map(|t| t.id())
                .collect::<Vec<_>>()
                .join(" or ")
        )))
    }
}

/// What `tool.cycle_wire_mode` returns: the Wire Tool's mode now.
#[derive(Serialize, JsonSchema)]
pub struct WireModeResult {
    pub mode: WireMode,
}

/// `tool.cycle_wire_mode` (Shift+Space): the Wire Tool's next mode, also during a run. The
/// session keeps it for the next time the Tool is active.
struct ToolCycleWireMode;

impl Command for ToolCycleWireMode {
    const META: Meta = Meta {
        id: "tool.cycle_wire_mode",
        title: "Cycle Wire Mode",
        description: "Switches the Wire Tool to its next mode: 90°, 45°, Any Angle, then Auto \
                      Wire, which routes each click from the last vertex around placed \
                      objects. It works during a run, and the session keeps the mode.",
        category: "tool",
        icon: "wire",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = WireModeResult;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<WireModeResult, CommandError> {
        only_in(cx, &[Tool::Wire], "Cycle Wire Mode")?;
        let wire = cx.view::<WireTool>()?;
        wire.mode = wire.mode.next();
        Ok(WireModeResult { mode: wire.mode })
    }
}

/// The Break Wire Tool's session state: whether its options window is open (Tab).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BreakWireTool {
    pub options_open: bool,
}

/// `tool.cycle_cutting_length` (Space in the Break Wire Tool): the next Cutting Length,
/// written back to the Preference by the window's Shell.
struct ToolCycleCuttingLength;

impl Command for ToolCycleCuttingLength {
    const META: Meta = Meta {
        id: "tool.cycle_cutting_length",
        title: "Cycle Cutting Length",
        description: "Steps the Break Wire Tool's Cutting Length: Snap To Segment, Snap Grid \
                      Size Multiple, Fixed Length. The window writes it straight back to the \
                      `schematic.break_wire.cutting_length` Preference.",
        category: "tool",
        icon: "split",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        only_in(cx, &[Tool::BreakWire], "Cycle Cutting Length")?;
        Ok(Turned {})
    }
}

/// `tool.insert_vertex` (Insert): adds a vertex to the Wire run where the pointer is,
/// look-ahead segment and all, or, with the Select Tool, to the segment whose handle is
/// held; applied by the window's Shell.
struct ToolInsertVertex;

impl Command for ToolInsertVertex {
    const META: Meta = Meta {
        id: "tool.insert_vertex",
        title: "Insert Vertex",
        description: "Adds a vertex to the Wire run in flight where the pointer is, keeping \
                      the corner on the way there, or starts a run there. With the Select \
                      Tool, while a Wire segment's handle is held, adds a vertex there that \
                      the pointer then carries.",
        category: "tool",
        icon: "wire",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        only_in(cx, &[Tool::Wire, Tool::Select], "Insert Vertex")?;
        Ok(Turned {})
    }
}

/// What `tool.edit_properties` returns: the Tool whose properties it shows.
#[derive(Serialize, JsonSchema)]
pub struct EditPropertiesResult {
    pub tool: Tool,
}

/// `tool.edit_properties` (Tab): the active Tool's properties. The Wire Tool opens (or
/// closes) its Auto Wire options, the Break Wire Tool its Cutting Length and Display
/// options; a Part being placed opens the Properties Panel.
struct ToolEditProperties;

impl Command for ToolEditProperties {
    const META: Meta = Meta {
        id: "tool.edit_properties",
        title: "Edit Tool Properties",
        description: "Shows the active Tool's properties in flight: the Wire Tool's Auto \
                      Wire options (Time Out After and Avoid cutting wires), opened or \
                      closed; the Break Wire Tool's options (Cutting Length, Show Cutter \
                      Box, Show Extremity Markers), which write straight back to the \
                      Preferences; for a Part being placed, the Properties Panel, whose \
                      edits later placements keep; for a Net Label, Port or Power Port \
                      being placed, an in-place edit of its name (the Tool pauses).",
        category: "tool",
        icon: "properties",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = EditPropertiesResult;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<EditPropertiesResult, CommandError> {
        let tool = only_in(
            cx,
            &[
                Tool::Wire,
                Tool::BreakWire,
                Tool::Place,
                Tool::NetLabel,
                Tool::Port,
                Tool::PowerPort,
            ],
            "Edit Tool Properties",
        )?;
        if tool == Tool::Wire {
            let wire = cx.view::<WireTool>()?;
            wire.options_open = !wire.options_open;
        }
        if tool == Tool::BreakWire {
            let options = cx.view::<BreakWireTool>()?;
            options.options_open = !options.options_open;
        }
        Ok(EditPropertiesResult { tool })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AutoWireSetParams {
    /// Time Out After, in milliseconds, 100 to 60000.
    timeout_ms: Option<u32>,
    /// How hard a route tries not to cross Wires.
    avoid_cutting_wires: Option<reda_engine::schematic::Avoid>,
}

/// Auto Wire's options now.
#[derive(Serialize, JsonSchema)]
pub struct AutoWireOptions {
    timeout_ms: u32,
    avoid_cutting_wires: reda_engine::schematic::Avoid,
}

/// `tool.auto_wire.set`: Auto Wire's options for the session, as its window sets them.
struct ToolAutoWireSet;

impl Command for ToolAutoWireSet {
    const META: Meta = Meta {
        id: "tool.auto_wire.set",
        title: "Set Auto Wire Options",
        description: "Sets the Wire Tool's Auto Wire options for the session: Time Out After \
                      (`timeout_ms`, 100 to 60000, 3000 at first) and Avoid cutting wires \
                      (low, medium or high). Its routes ask `schematic.wire.route` with \
                      them.",
        category: "tool",
        icon: "wire",
        kind: Kind::View,
    };
    type Params = AutoWireSetParams;
    type Output = AutoWireOptions;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: AutoWireSetParams,
    ) -> Result<AutoWireOptions, CommandError> {
        if let Some(ms) = p.timeout_ms
            && !(100..=60_000).contains(&ms)
        {
            return Err(CommandError::invalid_params(
                "/timeout_ms",
                "Time Out After is 100 to 60000 ms",
            ));
        }
        let wire = cx.view::<WireTool>()?;
        wire.timeout_ms = p.timeout_ms.unwrap_or(wire.timeout_ms);
        wire.avoid = p.avoid_cutting_wires.unwrap_or(wire.avoid);
        Ok(AutoWireOptions {
            timeout_ms: wire.timeout_ms,
            avoid_cutting_wires: wire.avoid,
        })
    }
}
