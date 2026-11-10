//! Editing on a schematic canvas: the Select, Move and Drag Tools, and the wiring Tools
//! ([`wiring`]: Wire, Net Label, Port, Power Port, No-Connect). Its keys are Keymap
//! bindings in the `schematic` and `tool:<id>` contexts (`keymap/reda.toml`); the in-flight
//! keys arrive as view Commands the Shell hands on ([`turn`], [`corner`], [`remove_last`],
//! [`fix_corner`], [`insert_vertex`], [`end_run`]). A Wire run another Tool interrupts
//! waits on the tab ([`suspend`], [`resume`]).
//!
//! The editor turns a frame's [`Pointer`] events into Commands the window runs after the
//! frame (`selection.set`, `schematic.drag`, `schematic.move`, `tool.activate`), and keeps
//! what is in flight on the Document's tab ([`Interaction`]). A move in flight touches
//! neither the Project nor the Journal: the canvas draws it from the tab's marks and
//! overlay, from [`shift_preview`], until release commits exactly one Command.
//!
//! Picking, box selection and snapping are the shared [`interact`](crate::canvas::interact)
//! functions; this module only says what on a sheet can be picked and snapped to.

mod auto_wire;
mod break_wire;
mod clipboard;
mod decorate;
mod drag_check;
mod grab;
mod graphics;
mod handles;
mod in_place;
mod wiring;

use std::collections::BTreeSet;

use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{SchematicSheet, Uuid};
use reda_engine::schematic::connectivity::{net_at, placed_pins};
use reda_engine::schematic::{
    Axis, FlightStep, ShiftPreview, flight_preview, shift_preview_flipped,
};
use serde_json::{Value, json};

use crate::canvas::interact::{BoxMode, Pointer};
use crate::canvas::{LiveHandle, Overlay, RubberBand};
use crate::shell::{Shell, ToolPreview};
use crate::snap::{self, Target};
use crate::tools::Tool;
use crate::tr;

pub use auto_wire::options as auto_wire_options;
pub use break_wire::{Breaking, cycle_cutting_length, options as break_wire_options};
pub use clipboard::{Pasting, Referencing, copy, duplicate, paste};
pub use decorate::{decorate, drag_chip, hover_tip};
pub use drag_check::DragCheck;
pub use grab::{DragKeys, NudgeDirection, flip_corners, grab_point, nudge};
pub use graphics::Drawing;
pub use handles::{Grip, Handle, HandleDrag, delete_held_vertex, insert_held_vertex};
pub use in_place::{
    EditTarget, InPlace, begin_edit, cancel_edit, commit as commit_edit, edit_selected, editing,
    pause_placing, paused, set_text as set_edit_text, target_of,
};
pub use wiring::{Corner, Placing, Wiring};

/// Adds the editor's view Commands to `registry`: `tool.copy`, `tool.paste`,
/// `tool.duplicate`, `schematic.edit_in_place` and `schematic.nudge`. The Shell does this for its engine.
pub fn register_all(registry: &mut reda_engine::command::Registry) {
    clipboard::register(registry);
    registry.add(in_place::EditInPlace);
    registry.add(grab::Nudge);
}

/// A Command for the window to run after the frame.
pub type Invocation = (String, Value);

/// A Document point in nm, y up.
type P = (f64, f64);

/// How close, in points on screen, the pointer must be to pick an object.
const PICK_POINTS: f64 = 5.0;

/// What a schematic canvas has in flight.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Interaction {
    #[default]
    Idle,
    /// A selection box is being dragged out from `from` to `to`.
    Boxing {
        from: P,
        to: P,
        modifiers: egui::Modifiers,
    },
    /// The selection is moving with the pointer.
    Flight(Flight),
    /// The Wire Tool, with or without a run in flight.
    Wiring(Wiring),
    /// A Tool placing one object per click follows the pointer.
    Placing(Placing),
    /// The Line or Rect Tool, with or without a shape in flight.
    Drawing(Drawing),
    /// A selected Wire's handle is being dragged.
    Handle(HandleDrag),
    /// The Break Wire Tool, with the pointer over a Wire or not.
    Breaking(Breaking),
    /// A paste or Duplicate stamp floating on the cursor.
    Pasting(Pasting),
    /// Cut or copy waiting for a click on the reference point.
    Referencing(Referencing),
    /// An in-place edit of a name, field or text.
    Editing(InPlace),
}

impl Interaction {
    /// Whether the canvas shows a preview Scene for this state rather than the Project.
    fn previews(&self) -> bool {
        match self {
            Interaction::Placing(p) => p.shown.is_some(),
            Interaction::Flight(f) => f.shown.is_some(),
            Interaction::Pasting(p) => p.shown.is_some(),
            _ => false,
        }
    }
}

/// What an in-flight key does to what a Tool carries: Space turns it a quarter turn
/// counter-clockwise, X flips it left to right and Y top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    Rotate,
    MirrorX,
    MirrorY,
}

/// A move or drag in flight.
#[derive(Clone, Debug, PartialEq)]
pub struct Flight {
    pub objects: Vec<Uuid>,
    /// A drag (Wires rubber-band) rather than a detached move.
    pub keep_connections: bool,
    /// Started by the Move or Drag Tool (a click puts it down), not by dragging.
    pub by_tool: bool,
    /// The snapped point picked up; the selection moves by the snapped pointer minus it.
    pub anchor: Point,
    /// From the pointer to the anchor where it was picked up (nm): the anchor follows
    /// the pointer at this distance and snaps there, so a grab beside a Pin end moves the
    /// Pin end onto the grid without a jump.
    pub lead: (f64, f64),
    /// What the pointer snaps to while moving: everything on the sheet not moving.
    pub targets: Vec<Target>,
    pub offset: Point,
    pub preview: ShiftPreview,
    /// Space, X and Y pressed in flight, applied in order about the anchor before the
    /// offset. Once there are any, the canvas draws a preview Scene instead of marks.
    pub turns: Vec<Turn>,
    /// The preview Scene drawn: for (offset, turns), as Scene generation.
    shown: Option<(Point, usize, u64)>,
    /// Released: its Command is queued, and it keeps showing until the Shell runs it.
    pub committing: bool,
    /// `Mod`+Space: the rubber-banded Wires turn their corners the other way.
    pub flip: bool,
    /// Display When Dragging: the Junctions and Net change at the current offset.
    pub check: Option<DragCheck>,
}

impl Flight {
    /// What the flight does to its objects, in order: its turns about the anchor, then
    /// the offset.
    fn steps(&self) -> Vec<FlightStep> {
        let center = self.anchor;
        let mut steps: Vec<FlightStep> = self
            .turns
            .iter()
            .map(|turn| match turn {
                Turn::Rotate => FlightStep::Rotate {
                    center,
                    angle: Millideg::from_deg(90),
                },
                Turn::MirrorX => FlightStep::Mirror {
                    center,
                    axis: Axis::X,
                },
                Turn::MirrorY => FlightStep::Mirror {
                    center,
                    axis: Axis::Y,
                },
            })
            .collect();
        if self.offset != Point::default() {
            steps.push(FlightStep::Translate(self.offset));
        }
        steps
    }
}

/// How far on the Document the pick distance reaches at the canvas's zoom. Snapping
/// captures within the grid preset's Electrical Range instead ([`crate::snap`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub pick: f64,
}

impl Scale {
    /// The distances for a canvas showing `nm_per_point` nm per screen point.
    pub fn at(nm_per_point: f64) -> Scale {
        Scale {
            pick: PICK_POINTS * nm_per_point,
        }
    }
}

/// Runs the active Tool on a frame's pointer `events` over `document`'s canvas. Returns
/// the Commands to run; updates the tab's marks and overlay.
pub fn pointer(
    shell: &mut Shell,
    document: Uuid,
    events: &[Pointer],
    scale: Scale,
) -> Vec<Invocation> {
    if let Some(commands) =
        crate::selection::picking::text_pointer(shell, document, events, scale.pick)
    {
        return commands;
    }
    let Some(tab) = shell.document_tab_mut(document) else {
        return Vec::new();
    };
    let mut state = std::mem::take(&mut tab.interaction);
    let mut hovered = tab.live.hovered;
    let mut cursor = tab.cursor;
    let previewed = state.previews();
    // A bridged edit that changed what the run was drawn from has cancelled it.
    if let Interaction::Wiring(w) = &mut state
        && w.previewing
        && shell.tool_preview().is_none()
    {
        w.points.clear();
        w.previewing = false;
    }
    let mut cx = Cx {
        shell,
        document,
        scale,
        commands: Vec::new(),
    };
    let mut state = cx.sync_tool(state);
    // The Move and Drag Tools pick the selection up where the pointer was when they were
    // activated, so its first movement already moves the selection.
    if let Some(at) = cursor {
        state = cx.start_tool_flight(state, at);
    }
    for &event in events {
        if let Some(at) = event_at(event) {
            cursor = Some(at);
            state = cx.start_tool_flight(state, at);
        }
        state = cx.step(state, event, &mut hovered);
    }
    if let Some(at) = cursor {
        state = cx.start_tool_flight(state, at);
    }
    let commands = cx.commands;
    if !matches!(state, Interaction::Idle) {
        hovered = None;
    }
    show(shell, document, state, hovered, cursor, previewed);
    commands
}

/// Drops whatever is in flight on `document`'s canvas (Escape).
pub fn cancel(shell: &mut Shell, document: Uuid) {
    if let Some(tab) = shell.document_tab_mut(document)
        && !matches!(&tab.interaction, Interaction::Flight(f) if f.committing)
    {
        let cursor = tab.cursor;
        let previewed = tab.interaction.previews();
        show(shell, document, Interaction::Idle, None, cursor, previewed);
    }
}

/// Applies an in-flight key (Space, X, Y) to what `document`'s canvas carries. Returns
/// `false` when nothing there takes it, so the key applies to the selection instead.
pub fn turn(shell: &mut Shell, document: Uuid, how: Turn) -> bool {
    let Some(tab) = shell.document_tab_mut(document) else {
        return false;
    };
    match &mut tab.interaction {
        Interaction::Placing(p) if p.tool != Tool::NoConnect => p.turn(how),
        Interaction::Flight(f) if !f.committing => f.turns.push(how),
        _ => return false,
    }
    restate(shell, document);
    true
}

/// `/`: the Wire run on `document`'s canvas turns its corner the other way.
pub fn corner(shell: &mut Shell, document: Uuid) {
    if let Some(tab) = shell.document_tab_mut(document)
        && let Interaction::Wiring(w) = &mut tab.interaction
    {
        w.corner = match w.corner {
            Corner::HorizontalFirst => Corner::VerticalFirst,
            Corner::VerticalFirst => Corner::HorizontalFirst,
        };
        restate(shell, document);
    }
}

/// Backspace: takes the last fixed segment off the Wire run on `document`'s canvas; with
/// only its start left, the run ends without drawing anything.
pub fn remove_last(shell: &mut Shell, document: Uuid) {
    if let Some(tab) = shell.document_tab_mut(document)
        && let Interaction::Wiring(w) = &mut tab.interaction
    {
        w.remove_last();
        restate(shell, document);
    }
}

/// Whether a Wire run is in flight on `document`'s canvas.
pub fn run_in_flight(shell: &Shell, document: Uuid) -> bool {
    shell.document_tab(document).is_some_and(
        |tab| matches!(&tab.interaction, Interaction::Wiring(w) if !w.points.is_empty()),
    )
}

/// Runs `step` on the Wire Tool of `document`'s canvas, shows the result and returns the
/// Commands it queued.
fn wire_key(
    shell: &mut Shell,
    document: Uuid,
    step: impl FnOnce(&mut Cx<'_>, Wiring) -> Wiring,
) -> Vec<Invocation> {
    let Some(tab) = shell.document_tab_mut(document) else {
        return Vec::new();
    };
    let Interaction::Wiring(w) = std::mem::take(&mut tab.interaction) else {
        return Vec::new();
    };
    let (hovered, cursor) = (tab.live.hovered, tab.cursor);
    let mut cx = Cx {
        shell,
        document,
        scale: Scale::at(1.0),
        commands: Vec::new(),
    };
    let w = step(&mut cx, w);
    let commands = cx.commands;
    show(
        shell,
        document,
        Interaction::Wiring(w),
        hovered,
        cursor,
        false,
    );
    commands
}

/// Enter: fixes a corner of the Wire run on `document`'s canvas where the pointer is, as
/// a click does. Returns the run's Command if that ends it.
pub fn fix_corner(shell: &mut Shell, document: Uuid) -> Vec<Invocation> {
    wire_key(shell, document, |cx, w| cx.fix(w))
}

/// Insert: adds a vertex to the Wire run on `document`'s canvas where the pointer is.
pub fn insert_vertex(shell: &mut Shell, document: Uuid) -> Vec<Invocation> {
    wire_key(shell, document, |cx, w| cx.insert_vertex(w))
}

/// Esc (or a right-click) during a Wire run on `document`'s canvas: the run commits its
/// fixed corners and the Tool stays. Returns the run's Command, if it has a segment.
pub fn end_run(shell: &mut Shell, document: Uuid) -> Vec<Invocation> {
    wire_key(shell, document, |cx, w| cx.end_run(w))
}

/// Shows the Wire run on `document`'s canvas again after its mode changed.
pub fn restate_wiring(shell: &mut Shell, document: Uuid) {
    restate(shell, document);
}

/// Another Tool starts during the Wire run on `document`'s canvas: the run waits on the
/// tab, out of the canvas, until [`resume`].
pub fn suspend(shell: &mut Shell, document: Uuid) {
    let Some(tab) = shell.document_tab_mut(document) else {
        return;
    };
    let Interaction::Wiring(mut w) = std::mem::take(&mut tab.interaction) else {
        return;
    };
    w.previewing = false;
    tab.suspended = Some(Interaction::Wiring(w));
    shell.end_tool();
    restate(shell, document);
}

/// The Tool on top finished: the suspended Wire run on `document`'s canvas is back.
pub fn resume(shell: &mut Shell, document: Uuid) {
    let Some(tab) = shell.document_tab_mut(document) else {
        return;
    };
    let Some(state) = tab.suspended.take() else {
        return;
    };
    tab.interaction = state;
    restate(shell, document);
}

/// Backspace: takes the last point back from the line or rectangle being drawn on
/// `document`'s canvas. Returns `false` when nothing is being drawn there.
pub fn undo_point(shell: &mut Shell, document: Uuid) -> bool {
    let Some(tab) = shell.document_tab_mut(document) else {
        return false;
    };
    let Interaction::Drawing(d) = &mut tab.interaction else {
        return false;
    };
    d.undo_point();
    restate(shell, document);
    true
}

/// Enter: finishes the polyline being drawn on `document`'s canvas. Returns the Command
/// to run, if there is a line to draw.
pub fn finish(shell: &mut Shell, document: Uuid) -> Vec<Invocation> {
    let Some(tab) = shell.document_tab_mut(document) else {
        return Vec::new();
    };
    let Interaction::Drawing(mut d) = std::mem::take(&mut tab.interaction) else {
        return Vec::new();
    };
    let (hovered, cursor) = (tab.live.hovered, tab.cursor);
    let mut cx = Cx {
        shell,
        document,
        scale: Scale::at(1.0),
        commands: Vec::new(),
    };
    cx.finish_line(&mut d);
    let commands = cx.commands;
    show(
        shell,
        document,
        Interaction::Drawing(d),
        hovered,
        cursor,
        false,
    );
    commands
}

/// Shows the tab's state again after a key changed it.
fn restate(shell: &mut Shell, document: Uuid) {
    let Some(tab) = shell.document_tab_mut(document) else {
        return;
    };
    let state = std::mem::take(&mut tab.interaction);
    let (hovered, cursor) = (tab.live.hovered, tab.cursor);
    let previewed = state.previews();
    show(shell, document, state, hovered, cursor, previewed);
}

/// Ends what was put down on `tab` once its Command has run: a released move, or a
/// finished Wire run still drawn. Returns whether anything ended.
pub(crate) fn end_commit(tab: &mut crate::shell::DocumentTab) -> bool {
    match &mut tab.interaction {
        Interaction::Flight(f) if f.committing => {
            tab.interaction = Interaction::Idle;
            tab.live = LiveHandle::default();
            tab.moving_offset = (0.0, 0.0);
            tab.overlay = Overlay::default();
            true
        }
        Interaction::Wiring(w) if !w.committed.is_empty() => {
            w.committed.clear();
            let (solid, dotted) = w.drawn();
            tab.overlay.wires = solid.into();
            tab.overlay.dotted = dotted.into();
            true
        }
        Interaction::Drawing(d) if !d.committed.is_empty() => {
            d.committed.clear();
            tab.overlay.wires = d.segments().into();
            true
        }
        _ => false,
    }
}

fn event_at(event: Pointer) -> Option<P> {
    match event {
        Pointer::Hover(at) | Pointer::Drag(at) | Pointer::DragEnd(at) => Some(at),
        Pointer::Click { at, .. } | Pointer::DragStart { at, .. } => Some(at),
    }
}

/// Writes `state` and what it shows to the tab.
fn show(
    shell: &mut Shell,
    document: Uuid,
    mut state: Interaction,
    hovered: Option<Uuid>,
    cursor: Option<P>,
    previewed: bool,
) {
    if let Interaction::Wiring(w) = &mut state {
        w.mode = shell
            .engine()
            .view()
            .get::<crate::tools::WireTool>()
            .map(|t| t.mode)
            .unwrap_or_default();
    }
    preview_run(shell, document, &mut state);
    preview_scene(shell, document, &mut state, previewed);
    let mode = match &state {
        Interaction::Boxing { from, to, .. } => {
            crate::selection::rules::box_mode(shell, document, *from, *to)
        }
        _ => BoxMode::Enclose,
    };
    let idle = idle_overlay(shell, document);
    let breaking = match &state {
        Interaction::Breaking(b) => Some(break_wire::overlay(shell, document, b)),
        _ => None,
    };
    let Some(tab) = shell.document_tab_mut(document) else {
        return;
    };
    let (moving, hidden, offset, overlay) = match &state {
        Interaction::Idle => (Vec::new(), Vec::new(), (0.0, 0.0), idle),
        Interaction::Breaking(_) => (
            Vec::new(),
            Vec::new(),
            (0.0, 0.0),
            breaking.unwrap_or_default(),
        ),
        Interaction::Handle(d) => {
            let mut found = Vec::new();
            handles::wire_handles_of(d, &mut found);
            let handles: Vec<(Point, bool)> = found.iter().map(|h| (h.at, h.red)).collect();
            (
                Vec::new(),
                vec![d.wire],
                (0.0, 0.0),
                Overlay {
                    wires: d.vertices.windows(2).map(|p| (p[0], p[1])).collect(),
                    handles: handles.into(),
                    ..Overlay::default()
                },
            )
        }
        Interaction::Boxing { from, to, .. } => (
            Vec::new(),
            Vec::new(),
            (0.0, 0.0),
            Overlay {
                rubber_band: Some(RubberBand {
                    from: *from,
                    to: *to,
                    mode,
                }),
                ..Overlay::default()
            },
        ),
        Interaction::Flight(f) if !f.turns.is_empty() => (
            Vec::new(),
            Vec::new(),
            (0.0, 0.0),
            Overlay {
                cursor: Some(f.anchor + f.offset),
                junctions: f
                    .check
                    .as_ref()
                    .map(|c| c.junctions.clone())
                    .unwrap_or_default()
                    .into(),
                ..Overlay::default()
            },
        ),
        Interaction::Flight(f) => (
            f.preview.moved.clone(),
            f.preview.hidden.clone(),
            (f.offset.x.0 as f64, f.offset.y.0 as f64),
            Overlay {
                wires: f
                    .preview
                    .wires
                    .iter()
                    .flat_map(|v| v.windows(2).map(|pair| (pair[0], pair[1])))
                    .collect(),
                cursor: Some(f.anchor + f.offset),
                junctions: f
                    .check
                    .as_ref()
                    .map(|c| c.junctions.clone())
                    .unwrap_or_default()
                    .into(),
                ..Overlay::default()
            },
        ),
        Interaction::Wiring(w) => {
            let mut wires = w.committed.clone();
            let (solid, dotted) = w.drawn();
            wires.extend(solid);
            (
                Vec::new(),
                Vec::new(),
                (0.0, 0.0),
                Overlay {
                    wires: wires.into(),
                    dotted: dotted.into(),
                    cursor: Some(w.at),
                    ..Overlay::default()
                },
            )
        }
        Interaction::Placing(p) => (
            Vec::new(),
            Vec::new(),
            (0.0, 0.0),
            Overlay {
                cursor: Some(p.at),
                ..Overlay::default()
            },
        ),
        Interaction::Pasting(p) => (
            Vec::new(),
            Vec::new(),
            (0.0, 0.0),
            Overlay {
                cursor: Some(p.at + p.hold),
                ..Overlay::default()
            },
        ),
        Interaction::Referencing(_) | Interaction::Editing(_) => {
            (Vec::new(), Vec::new(), (0.0, 0.0), idle)
        }
        Interaction::Drawing(d) => {
            let mut wires = d.committed.clone();
            wires.extend(d.segments());
            (
                Vec::new(),
                Vec::new(),
                (0.0, 0.0),
                Overlay {
                    wires: wires.into(),
                    cursor: Some(d.at),
                    ..Overlay::default()
                },
            )
        }
    };
    if !tab.live.shows(hovered, &moving, &hidden) {
        tab.live = LiveHandle::new(hovered, &moving, &hidden);
    }
    tab.moving_offset = offset;
    tab.overlay = overlay;
    tab.interaction = state;
    tab.cursor = cursor;
}

/// What the canvas shows with nothing in flight: the selected Wires' handles and
/// segments with the Select Tool.
fn idle_overlay(shell: &Shell, document: Uuid) -> Overlay {
    let Some(sheet) = shell
        .engine()
        .project()
        .and_then(|p| p.schematic_sheet(document))
    else {
        return Overlay::default();
    };
    match shell.tool() {
        Tool::Select => {
            let selection = shell.selection();
            let handles: Vec<(Point, bool)> = (handles::handles(sheet, selection).iter())
                .map(|h| (h.at, h.red))
                .collect();
            Overlay {
                handles: handles.into(),
                segments: handles::selected_segments(sheet, selection).into(),
                ..Overlay::default()
            }
        }
        _ => Overlay::default(),
    }
}

/// Starts the Shell's Tool preview when a Wire run begins, depending on the Wires and
/// Units at its start (a bridged edit to them cancels the run), and ends it with the run.
fn preview_run(shell: &mut Shell, document: Uuid, state: &mut Interaction) {
    let Interaction::Wiring(w) = state else {
        return;
    };
    match (w.points.first().copied(), w.previewing) {
        (Some(start), false) => {
            let mut objects = Vec::new();
            if let Some(sheet) = shell
                .engine()
                .project()
                .and_then(|p| p.schematic_sheet(document))
            {
                objects.extend(sheet.wires().filter(|w| w.contains(start)).map(|w| w.uuid));
                objects.extend(
                    placed_pins(sheet)
                        .into_iter()
                        .filter(|(_, _, at)| *at == start)
                        .map(|(u, _, _)| u.uuid),
                );
            }
            shell.begin_tool(ToolPreview::new(tr!("tool-wire"), objects));
            w.previewing = true;
        }
        (None, true) => {
            shell.end_tool();
            w.previewing = false;
        }
        _ => {}
    }
}

/// Draws the object a placing Tool is about to place into the canvas's Scene (a preview
/// of the Project with it added); draws the Project again once nothing previews.
fn preview_scene(shell: &mut Shell, document: Uuid, state: &mut Interaction, previewed: bool) {
    let Some(generation) = shell.document_tab(document).map(|t| t.scene.generation) else {
        return;
    };
    if let Interaction::Flight(f) = state
        && !f.turns.is_empty()
    {
        let key = (f.offset, f.turns.len(), generation);
        if f.shown == Some(key) {
            return;
        }
        let preview = shell.engine().project().and_then(|project| {
            flight_preview(project, &f.objects, &f.steps(), f.keep_connections).ok()
        });
        if let Some(project) = preview {
            shell.show_preview(document, &project);
            let generation = shell
                .document_tab(document)
                .map_or(generation, |t| t.scene.generation);
            f.shown = Some((f.offset, f.turns.len(), generation));
        }
        return;
    }
    if let Interaction::Pasting(p) = state {
        clipboard::preview(shell, document, p, generation);
        return;
    }
    let Interaction::Placing(p) = state else {
        if previewed {
            shell.redraw(document);
        }
        return;
    };
    let key = (p.at, p.rotation, p.name.clone(), generation);
    if p.shown.as_ref() == Some(&key) {
        return;
    }
    let preview = shell.engine().project().and_then(|project| {
        let object = p.object(document, Uuid::from_u128(PREVIEW_UUID))?;
        let mut project = project.clone();
        let mut tx = project.transaction();
        tx.insert(object).ok()?;
        tx.commit().ok()?;
        Some(project)
    });
    match preview {
        Some(project) => {
            shell.show_preview(document, &project);
            let generation = shell
                .document_tab(document)
                .map_or(generation, |t| t.scene.generation);
            p.shown = Some((p.at, p.rotation, p.name.clone(), generation));
        }
        None => {
            if previewed || p.shown.is_some() {
                shell.redraw(document);
            }
            p.shown = None;
        }
    }
}

/// The UUID of the object a placing Tool previews; never stored.
const PREVIEW_UUID: u128 = 0x0000_0000_0000_4000_8000_7072_6576_6965;

struct Cx<'a> {
    shell: &'a mut Shell,
    document: Uuid,
    scale: Scale,
    commands: Vec<Invocation>,
}

impl Cx<'_> {
    fn sheet(&self) -> Option<&SchematicSheet> {
        self.shell
            .engine()
            .project()?
            .schematic_sheet(self.document)
    }

    /// The selected objects on this sheet.
    fn selected(&self) -> Vec<Uuid> {
        let on_sheet = self.sheet().map(selectable).unwrap_or_default();
        self.shell
            .selection()
            .objects
            .iter()
            .copied()
            .filter(|o| {
                on_sheet.contains(o)
                    && crate::selection::rules::allowed(self.shell, self.document, *o)
            })
            .collect()
    }

    fn hit(&self, at: P) -> Option<Uuid> {
        crate::selection::picking::hit(self.shell, self.document, at, self.scale.pick)
    }

    /// Ends a Tool flight once its Tool is no longer active, and a committed flight's
    /// Command has run (the Shell drops it then).
    fn sync_tool(&mut self, state: Interaction) -> Interaction {
        match state {
            Interaction::Wiring(_) if self.shell.tool() != Tool::Wire => Interaction::Idle,
            Interaction::Placing(p) if self.shell.tool() != p.tool => Interaction::Idle,
            Interaction::Drawing(d) if self.shell.tool() != d.tool => Interaction::Idle,
            Interaction::Breaking(_) if self.shell.tool() != Tool::BreakWire => Interaction::Idle,
            Interaction::Flight(f) if f.by_tool && !f.committing => {
                let wanted = if f.keep_connections {
                    Tool::Drag
                } else {
                    Tool::Move
                };
                if self.shell.tool() == wanted {
                    Interaction::Flight(f)
                } else {
                    Interaction::Idle
                }
            }
            other => other,
        }
    }

    /// Picks the selection up at `at` when the Move or Drag Tool is active.
    fn start_tool_flight(&mut self, state: Interaction, at: P) -> Interaction {
        let keep_connections = match (self.shell.tool(), &state) {
            (Tool::Move, Interaction::Idle) => false,
            (Tool::Drag, Interaction::Idle) => true,
            (_, Interaction::Idle) => return self.start_wiring(at).unwrap_or(state),
            _ => return state,
        };
        let objects = self.selected();
        if objects.is_empty() {
            self.commands
                .push(("tool.activate:select".to_owned(), json!({})));
            return state;
        }
        let grabbed = self.hit(at).filter(|h| objects.contains(h));
        self.start_flight(objects, keep_connections, true, grabbed, at, at)
    }

    fn start_flight(
        &mut self,
        objects: Vec<Uuid>,
        keep_connections: bool,
        by_tool: bool,
        grabbed: Option<Uuid>,
        from: P,
        at: P,
    ) -> Interaction {
        let Some(sheet) = self.sheet() else {
            return Interaction::Idle;
        };
        let all = snap::hotspots::sheet(sheet);
        // The grab Preferences hold the object at its nearest connection point or its
        // reference point; with neither, where it was clicked.
        let grab = grabbed.and_then(|g| {
            let g = match crate::selection::rules::child(self.shell, self.document, g) {
                Some(child) => child.owner(),
                None => g,
            };
            grab::grab_point(sheet, &self.shell.preferences().editing, g, point(from))
        });
        // The held point keeps its distance from the pointer (nothing jumps on the press),
        // and it is what snaps as the drag goes.
        let (anchor, lead) = match grab {
            Some(held) => (held, (held.x.0 as f64 - from.0, held.y.0 as f64 - from.1)),
            None => {
                let anchor = snap::snap_at(self.shell, self.document, from, &all).at;
                let lead = (anchor.x.0 as f64 - from.0, anchor.y.0 as f64 - from.1);
                (anchor, lead)
            }
        };
        let flight = Flight {
            targets: snap::hotspots::without(all, &objects),
            objects,
            keep_connections,
            by_tool,
            anchor,
            lead,
            offset: Point::default(),
            preview: ShiftPreview::default(),
            turns: Vec::new(),
            shown: None,
            committing: false,
            flip: false,
            check: None,
        };
        let mut flight = self.fly(flight, from, true);
        flight = self.fly(flight, at, false);
        Interaction::Flight(flight)
    }

    /// Moves `flight` to the pointer at `at`.
    fn fly(&self, mut flight: Flight, at: P, force: bool) -> Flight {
        let at = (at.0 + flight.lead.0, at.1 + flight.lead.1);
        let target = snap::snap_at(self.shell, self.document, at, &flight.targets).at;
        let mut offset = target - flight.anchor;
        // Alt held: only along the axis it has gone further on.
        if grab::alt_held(self.shell) {
            if offset.x.0.abs() >= offset.y.0.abs() {
                offset.y = Nm(0);
            } else {
                offset.x = Nm(0);
            }
        }
        if !flight.turns.is_empty() {
            // Turned: drawn from a preview Scene (see `preview_scene`), not marks.
            if offset != flight.offset || force {
                flight.offset = offset;
                flight.check = self.drag_check(&flight);
            }
        } else if offset != flight.offset || force {
            flight.offset = offset;
            flight.preview = self.shift_preview(&flight);
            flight.check = self.drag_check(&flight);
        }
        flight
    }

    /// The marks `flight` shows at its offset.
    fn shift_preview(&self, flight: &Flight) -> ShiftPreview {
        let (objects, keep) = (&flight.objects, flight.keep_connections);
        self.shell
            .engine()
            .project()
            .and_then(|p| shift_preview_flipped(p, objects, flight.offset, keep, flight.flip).ok())
            .unwrap_or_else(|| ShiftPreview {
                moved: flight.objects.clone(),
                ..ShiftPreview::default()
            })
    }

    /// Display When Dragging: the Junctions and Net change the drop would make.
    fn drag_check(&self, flight: &Flight) -> Option<DragCheck> {
        if !self.shell.preferences().schematic.display_when_dragging {
            return None;
        }
        let project = self.shell.engine().project()?;
        let after = flight_preview(
            project,
            &flight.objects,
            &flight.steps(),
            flight.keep_connections,
        )
        .ok()?;
        Some(drag_check::check(project, &after, self.document))
    }

    /// Puts `flight` down: queues its one Command, unless it never moved.
    fn commit(&mut self, mut flight: Flight) -> Interaction {
        if flight.by_tool {
            self.commands
                .push(("tool.activate:select".to_owned(), json!({})));
        }
        if flight.offset == Point::default() && flight.turns.is_empty() {
            return Interaction::Idle;
        }
        let id = if flight.keep_connections {
            "schematic.drag"
        } else {
            "schematic.move"
        };
        let mut shift = json!({
            "objects": flight.objects,
            "by": {"x": flight.offset.x.0, "y": flight.offset.y.0},
        });
        if flight.keep_connections && flight.flip {
            shift["flip_corners"] = json!(true);
        }
        if flight.turns.is_empty() {
            self.commands.push((id.to_owned(), shift));
        } else {
            // Turned in flight: one batch, so one Journal entry and one undo step.
            let (objects, keep) = (&flight.objects, flight.keep_connections);
            let center = json!({"x": flight.anchor.x.0, "y": flight.anchor.y.0});
            let mut steps: Vec<Value> = flight
                .turns
                .iter()
                .map(|turn| match turn {
                    Turn::Rotate => json!({"id": "schematic.rotate", "params": {
                        "objects": objects, "center": center, "keep_connections": keep}}),
                    Turn::MirrorX | Turn::MirrorY => {
                        let axis = if *turn == Turn::MirrorX { "x" } else { "y" };
                        json!({"id": "schematic.mirror", "params": {
                            "objects": objects, "axis": axis, "center": center,
                            "keep_connections": keep}})
                    }
                })
                .collect();
            if flight.offset != Point::default() {
                steps.push(json!({"id": id, "params": shift}));
            }
            self.commands
                .push(("batch".to_owned(), json!({ "commands": steps })));
        }
        flight.committing = true;
        Interaction::Flight(flight)
    }

    fn step(
        &mut self,
        state: Interaction,
        event: Pointer,
        hovered: &mut Option<Uuid>,
    ) -> Interaction {
        let tool = self.shell.tool();
        match (state, event) {
            (Interaction::Handle(d), event) => self.handle_step(d, event),
            (Interaction::Breaking(b), event) => self.break_step(b, event),
            (Interaction::Pasting(p), event) => self.paste_step(p, event),
            (Interaction::Referencing(r), event) => self.reference_step(r, event),
            // The text field has the keyboard; Enter or Esc ends the edit.
            (Interaction::Editing(edit), _) => Interaction::Editing(edit),
            (Interaction::Idle, Pointer::Click { at, modifiers })
                if tool == Tool::Select
                    && modifiers.is_none()
                    && let Some(edit) = self.slow_second_click(at) =>
            {
                Interaction::Editing(edit)
            }
            (Interaction::Idle, event) if tool == Tool::BreakWire => {
                self.break_step(Breaking::default(), event)
            }
            (Interaction::Idle, Pointer::Click { at, modifiers })
                if tool == Tool::Select
                    && !self.highlights(modifiers)
                    && self.segment_click(at, modifiers) =>
            {
                Interaction::Idle
            }
            (Interaction::Idle, Pointer::DragStart { from, at, .. })
                if tool == Tool::Select && self.grab_handle(from, at).is_some() =>
            {
                *hovered = None;
                self.grab_handle(from, at).expect("checked")
            }
            (Interaction::Wiring(w), event) => Interaction::Wiring(self.wire_step(w, event)),
            (Interaction::Placing(p), event) => Interaction::Placing(self.place_step(p, event)),
            (Interaction::Drawing(d), event) => Interaction::Drawing(self.draw_step(d, event)),
            // A released move keeps showing until its Command has run.
            (Interaction::Flight(f), _) if f.committing => Interaction::Flight(f),
            (Interaction::Flight(f), Pointer::Hover(at) | Pointer::Drag(at)) => {
                Interaction::Flight(self.fly(f, at, false))
            }
            (Interaction::Flight(f), Pointer::DragEnd(at)) if !f.by_tool => {
                let f = self.fly(f, at, false);
                self.commit(f)
            }
            (Interaction::Flight(f), Pointer::Click { at, .. } | Pointer::DragEnd(at)) => {
                let f = self.fly(f, at, false);
                self.commit(f)
            }
            (Interaction::Flight(f), Pointer::DragStart { at, .. }) => {
                Interaction::Flight(self.fly(f, at, false))
            }
            (
                Interaction::Boxing {
                    from, modifiers, ..
                },
                Pointer::Drag(to),
            ) => Interaction::Boxing {
                from,
                to,
                modifiers,
            },
            (
                Interaction::Boxing {
                    from, modifiers, ..
                },
                Pointer::DragEnd(to),
            ) => {
                self.box_select(from, to, modifiers);
                Interaction::Idle
            }
            (
                Interaction::Boxing {
                    from,
                    to,
                    modifiers,
                },
                _,
            ) => Interaction::Boxing {
                from,
                to,
                modifiers,
            },
            (Interaction::Idle, Pointer::Hover(at)) => {
                crate::selection::picking::hover(self.shell, self.document, at, self.scale.pick);
                *hovered = self.hit(at);
                Interaction::Idle
            }
            (Interaction::Idle, Pointer::Click { at, modifiers }) if self.highlights(modifiers) => {
                self.highlight_net(at, modifiers.shift);
                Interaction::Idle
            }
            (Interaction::Idle, Pointer::Click { at, modifiers }) => {
                if let Some(command) = crate::selection::picking::click(
                    self.shell,
                    self.document,
                    at,
                    self.scale.pick,
                    modifiers,
                ) {
                    self.commands.push(command);
                }
                Interaction::Idle
            }
            (
                Interaction::Idle,
                Pointer::DragStart {
                    from,
                    at,
                    modifiers,
                },
            ) => {
                *hovered = None;
                let Some(hit) = crate::selection::picking::movement_hit(
                    self.shell,
                    self.document,
                    from,
                    self.scale.pick,
                ) else {
                    return Interaction::Boxing {
                        from,
                        to: at,
                        modifiers,
                    };
                };
                let on_sheet = self.sheet().map(selectable).unwrap_or_default();
                let drag = crate::selection::picking::drag_start(
                    self.shell,
                    self.document,
                    hit,
                    modifiers,
                    |id| on_sheet.contains(&id),
                );
                self.commands.extend(drag.select);
                let objects = drag.objects;
                // Mod+drag detaches; a plain drag keeps connections.
                let detach = self.shell.modifier_roles().detach.held(&modifiers);
                self.start_flight(objects, !detach, false, Some(hit), from, at)
            }
            (Interaction::Idle, Pointer::Drag(_) | Pointer::DragEnd(_)) => Interaction::Idle,
        }
    }

    /// The in-place edit a slow second click at `at` starts, if it is one.
    fn slow_second_click(&self, at: P) -> Option<InPlace> {
        let hit = crate::selection::picking::slow_second_click(
            self.shell,
            self.document,
            at,
            self.scale.pick,
        )?;
        let project = self.shell.engine().project()?;
        let target = match crate::selection::rules::child(self.shell, self.document, hit) {
            Some(reda_engine::render::Child::Field { unit, field }) => {
                target_of(project, *unit, Some(field))
            }
            // A Unit's body is not text: a second click on it deselects it.
            _ if matches!(
                project.object(hit),
                Some(reda_engine::model::Object::Unit(_))
            ) =>
            {
                None
            }
            _ => target_of(project, hit, None),
        }?;
        in_place::start(self.shell, target)
    }

    /// Whether a click with `modifiers` highlights a Net: the `net_highlight_sheet` role
    /// (Alt by default) is held.
    fn highlights(&self, modifiers: egui::Modifiers) -> bool {
        (self.shell.modifier_roles().net_highlight_sheet).held(&modifiers)
    }

    /// Alt+click: highlights the Net under the pointer on every sheet, or clears the
    /// highlight when there is none. With Shift (`add`) the Net joins what is highlighted,
    /// and a click on nothing changes nothing.
    fn highlight_net(&mut self, at: P, add: bool) {
        let net = self.hit(at).and_then(|object| {
            let project = self.shell.engine().project()?;
            net_at(project, self.document, object, point(at))
        });
        let params = match net {
            Some(net) if add && !self.shell.highlight().objects.is_empty() => {
                let mut objects = self.shell.highlight().objects.clone();
                objects.extend(net.objects);
                objects.sort();
                objects.dedup();
                json!({"objects": objects})
            }
            Some(net) => json!({"net": net.name, "objects": net.objects}),
            None if add => return,
            None => json!({}),
        };
        self.commands.push(("view.highlight".to_owned(), params));
    }

    fn box_select(&mut self, from: P, to: P, modifiers: egui::Modifiers) {
        if let Some(command) =
            crate::selection::picking::box_selection(self.shell, self.document, from, to, modifiers)
        {
            self.commands.push(command);
        }
    }
}

/// The sheet point nearest the Document point `at`.
fn point((x, y): P) -> Point {
    Point::new(Nm(x.round() as i64), Nm(y.round() as i64))
}

/// Everything on `sheet` that can be selected.
pub fn selectable(sheet: &SchematicSheet) -> BTreeSet<Uuid> {
    let mut out = BTreeSet::new();
    out.extend(sheet.units().map(|o| o.uuid));
    out.extend(sheet.wires().map(|o| o.uuid));
    out.extend(sheet.net_labels().map(|o| o.uuid));
    out.extend(sheet.no_connects().map(|o| o.uuid));
    out.extend(sheet.ports().map(|o| o.uuid));
    out.extend(sheet.power_ports().map(|o| o.uuid));
    out.extend(sheet.texts().map(|o| o.uuid));
    out.extend(sheet.graphics().map(|o| o.uuid));
    out
}
