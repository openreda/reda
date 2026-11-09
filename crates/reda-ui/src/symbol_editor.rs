//! The Symbol editor: an open Symbol's tab (see [`element_editor`]), a Unit at a time,
//! with the shared canvas Tools. Its keys are Keymap bindings in the `symbol` context
//! (`keymap/reda.toml`).
//!
//! - Units show as tabs over the canvas (`library.symbol.unit.show` switches); the canvas
//!   draws the shown Unit ([`reda_engine::render::symbol_scene`]) and the Tools work on it.
//! - Select picks Pins and graphics (click, Shift+click, box), and dragging moves them on
//!   the grid: one `library.symbol.move` on release.
//! - Pin places a Pin per click on the 50 mil grid, numbered one past the highest; Space
//!   (`tool.rotate`) turns the next one by 90°.
//! - Line, Rect and Circle take two clicks (or a drag), Arc three (centre, start, end,
//!   counter-clockwise), Polygon a click per corner until a click on the first corner or
//!   Enter (`tool.finish`); Backspace (`tool.undo_segment`) takes back a corner. Text
//!   asks for its text over the canvas, then places it where it was clicked.
//! - Below the canvas the pin table lists the shown Unit's Pins for editing and takes
//!   pasted rows ([`parse_pin_rows`]), all of them one undo step.
//!
//! What is in flight is view state; the Symbol changes only through `library.symbol.*`
//! Commands on its own undo stack, which the engine refuses for a read-only (starter
//! Library) Symbol, so the editor does not offer them there.
//!
//! [`element_editor`]: crate::element_editor

mod pin_rows;
pub(crate) mod view;

use std::collections::BTreeMap;

use reda_engine::Engine;
use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{LibraryElement, Pin, Symbol, Uuid, auto_increment};
use reda_engine::render::{Scene, symbol_scene};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canvas::interact::{BoxMode, Pointer};
use crate::canvas::{LiveHandle, Overlay, RubberBand};
use crate::element_editor::{self, Invocation};
use crate::shell::Shell;
use crate::tools::Tool;
use crate::tr;

pub use pin_rows::{PinRow, parse_pin_rows};

/// A Document point in nm, y up.
type P = (f64, f64);

/// The grid Pins snap to: 50 mil, as the Library check expects.
pub const PIN_GRID: Nm = Nm::from_mil(50);
/// The stroke width of drawn graphics: 10 mil.
pub const LINE_WIDTH: Nm = Nm::from_mil(10);
/// The height of placed text: 50 mil.
pub const TEXT_SIZE: Nm = Nm::from_mil(50);
/// How close, in points on screen, the pointer must be to pick something.
const PICK_POINTS: f64 = 5.0;
/// Where pasted Pins without a position start, and how far apart they go.
const PASTE_COLUMN_X: Nm = Nm::from_mil(-300);
const PASTE_PITCH: Nm = Nm::from_mil(100);

/// What one Symbol canvas has in flight.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Interaction {
    #[default]
    Idle,
    /// A selection box from `from` to `to`.
    Boxing {
        from: P,
        to: P,
        modifiers: egui::Modifiers,
    },
    /// The selection follows the pointer, `offset` from where it was picked up.
    Moving {
        objects: Vec<Uuid>,
        anchor: Point,
        offset: Point,
    },
    /// The corners a drawing Tool has been given so far, snapped.
    Drafting { tool: Tool, points: Vec<Point> },
}

/// One Symbol tab's editor state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Canvas {
    /// The Unit picked with the Unit tabs; the first Unit while none is.
    pub unit: Option<Uuid>,
    /// How the next placed Pin is turned.
    pub pin_rotation: Millideg,
    pub interaction: Interaction,
    /// The Text Tool's text being typed, and where it goes.
    pub text: Option<(Point, String)>,
    /// The number and name of the last Pin the Pin Tool placed here, which the next one
    /// auto-increments from.
    pub last_pin: Option<(String, String)>,
}

/// The Symbol editor's view state: each Symbol tab's canvas.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SymbolEditorState {
    pub canvases: BTreeMap<Uuid, Canvas>,
}

/// Adds `library.symbol.unit.show` and `library.element.edit_in_library` to `registry`.
pub fn register_all(registry: &mut Registry) {
    registry.add(UnitShow);
    registry.add(EditInLibrary);
}

/// The editor state of the Symbol tab `symbol`.
pub fn canvas(shell: &Shell, symbol: Uuid) -> Canvas {
    canvas_in(shell.engine(), symbol)
}

fn canvas_in(engine: &Engine, symbol: Uuid) -> Canvas {
    engine
        .view()
        .get::<SymbolEditorState>()
        .and_then(|s| s.canvases.get(&symbol))
        .cloned()
        .unwrap_or_default()
}

fn canvas_mut(shell: &mut Shell, symbol: Uuid) -> &mut Canvas {
    let view = shell.engine.view_mut();
    if view.get::<SymbolEditorState>().is_none() {
        view.set(SymbolEditorState::default());
    }
    let state = view.get_mut::<SymbolEditorState>().expect("just set");
    state.canvases.entry(symbol).or_default()
}

/// The open Symbol `symbol`, if it is one.
pub fn symbol_of(engine: &Engine, symbol: Uuid) -> Option<&Symbol> {
    match engine.element(symbol)? {
        LibraryElement::Symbol(s) => Some(s),
        _ => None,
    }
}

fn unit_in(symbol: &Symbol, picked: Option<Uuid>) -> Option<Uuid> {
    picked
        .filter(|u| symbol.units.iter().any(|unit| unit.uuid == *u))
        .or_else(|| symbol.units.first().map(|u| u.uuid))
}

/// The Unit the Symbol editor of `symbol` shows.
pub fn shown_unit(shell: &Shell, symbol: Uuid) -> Option<Uuid> {
    let s = symbol_of(shell.engine(), symbol)?;
    unit_in(s, canvas(shell, symbol).unit)
}

/// The canvas Scene of the open Symbol `symbol`: its shown Unit.
pub fn scene(engine: &Engine, symbol: Uuid) -> Option<Scene> {
    let s = symbol_of(engine, symbol)?;
    symbol_scene(s, unit_in(s, canvas_in(engine, symbol).unit)?)
}

/// Turns the next Pin the Pin Tool places by 90° (Space).
pub fn rotate(shell: &mut Shell, symbol: Uuid) {
    let canvas = canvas_mut(shell, symbol);
    canvas.pin_rotation = Millideg((canvas.pin_rotation.0 + 90_000) % 360_000);
}

/// Drops whatever is in flight on the Symbol canvas `symbol` (Escape, another Tool).
pub fn cancel(shell: &mut Shell, symbol: Uuid) {
    let canvas = canvas_mut(shell, symbol);
    canvas.interaction = Interaction::Idle;
    canvas.text = None;
    show(shell, symbol, None);
}

/// Takes back the last corner of the shape being drawn (Backspace).
pub fn undo_segment(shell: &mut Shell, symbol: Uuid) {
    if let Interaction::Drafting { points, .. } = &mut canvas_mut(shell, symbol).interaction {
        points.pop();
    }
    show(shell, symbol, None);
}

/// Closes the polygon being drawn (Enter): the Command that adds it, if it has three
/// corners.
pub fn finish(shell: &mut Shell, symbol: Uuid) -> Option<Invocation> {
    let unit = shown_unit(shell, symbol)?;
    let canvas = canvas_mut(shell, symbol);
    let Interaction::Drafting {
        tool: Tool::Polygon,
        points,
    } = &canvas.interaction
    else {
        return None;
    };
    let shape = (points.len() >= 3).then(|| shape_of(Tool::Polygon, points))??;
    canvas.interaction = Interaction::Idle;
    show(shell, symbol, None);
    Some(add_graphic(symbol, unit, shape))
}

/// Runs the active Tool on a frame's pointer `events` over the Symbol `symbol`'s canvas,
/// at `nm_per_point` nm per screen point. Returns the Commands to run; updates the tab's
/// overlay and the editor state.
pub fn pointer(
    shell: &mut Shell,
    symbol: Uuid,
    events: &[Pointer],
    nm_per_point: f64,
) -> Vec<Invocation> {
    let Some(s) = symbol_of(shell.engine(), symbol).cloned() else {
        return Vec::new();
    };
    let Some(tab) = shell.document_tab(symbol) else {
        return Vec::new();
    };
    let mut hovered = tab.live.hovered;
    let mut cursor = tab.cursor;
    let mut state = canvas(shell, symbol);
    let read_only = element_editor::read_only(shell.engine(), symbol);
    let mut cx = Cx {
        shell,
        symbol: &s,
        unit: unit_in(&s, state.unit),
        pick: PICK_POINTS * nm_per_point,
        read_only,
        commands: Vec::new(),
    };
    state.interaction = cx.sync_tool(state.interaction);
    for &event in events {
        if let Some(at) = event_at(event) {
            cursor = Some(at);
        }
        cx.step(&mut state, event, &mut hovered);
    }
    let commands = cx.commands;
    if !matches!(state.interaction, Interaction::Idle) {
        hovered = None;
    }
    if let Some(tab) = shell.document_tab_mut(symbol) {
        tab.cursor = cursor;
    }
    *canvas_mut(shell, symbol) = state;
    show(shell, symbol, hovered);
    commands
}

fn event_at(event: Pointer) -> Option<P> {
    match event {
        Pointer::Hover(at) | Pointer::Drag(at) | Pointer::DragEnd(at) => Some(at),
        Pointer::Click { at, .. } | Pointer::DragStart { at, .. } => Some(at),
    }
}

fn point(p: Point) -> P {
    (p.x.0 as f64, p.y.0 as f64)
}

/// The grid `tool` snaps to: Pins to 50 mil, everything else to the tab's grid.
fn snap_grid(shell: &Shell, symbol: Uuid, tool: Tool) -> Nm {
    match tool {
        Tool::Pin => PIN_GRID,
        _ => shell.grid(symbol),
    }
}

/// Writes what `symbol`'s canvas has in flight to its tab: the moving selection, the
/// shape being drawn up to the cursor, the selection box and the snap cross.
fn show(shell: &mut Shell, symbol: Uuid, hovered: Option<Uuid>) {
    let tool = shell.tool();
    let grid = snap_grid(shell, symbol, tool);
    let state = canvas(shell, symbol);
    let mode = match &state.interaction {
        Interaction::Boxing { from, to, .. } => {
            crate::selection::rules::box_mode(shell, symbol, *from, *to)
        }
        _ => BoxMode::Enclose,
    };
    let rules = crate::snap::rules_on(shell, symbol, grid);
    let Some(tab) = shell.document_tab_mut(symbol) else {
        return;
    };
    let snapped = tab.cursor.map(|c| rules.to_grid(c));
    let drawing = tool != Tool::Select;
    let (moving, offset, overlay) = match &state.interaction {
        Interaction::Idle => (
            Vec::new(),
            (0.0, 0.0),
            Overlay {
                cursor: snapped.filter(|_| drawing),
                ..Overlay::default()
            },
        ),
        Interaction::Boxing { from, to, .. } => (
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
        Interaction::Moving {
            objects,
            anchor,
            offset,
        } => (
            objects.clone(),
            point(*offset),
            Overlay {
                cursor: Some(*anchor + *offset),
                ..Overlay::default()
            },
        ),
        Interaction::Drafting { points, .. } => {
            let mut path = points.clone();
            path.extend(snapped);
            let wires: Vec<(Point, Point)> = path.windows(2).map(|w| (w[0], w[1])).collect();
            let overlay = Overlay {
                wires: wires.into(),
                cursor: snapped,
                ..Overlay::default()
            };
            (Vec::new(), (0.0, 0.0), overlay)
        }
    };
    if !tab.live.shows(hovered, &moving, &[]) {
        tab.live = LiveHandle::new(hovered, &moving, &[]);
    }
    tab.moving_offset = offset;
    tab.overlay = overlay;
}

struct Cx<'a> {
    shell: &'a mut Shell,
    symbol: &'a Symbol,
    unit: Option<Uuid>,
    pick: f64,
    read_only: bool,
    commands: Vec<Invocation>,
}

impl Cx<'_> {
    fn element(&self) -> Uuid {
        self.symbol.uuid
    }

    /// `at` snapped onto the Symbol's Pin ends, else its grid (the Pin Tool's own).
    fn snap(&self, at: P) -> Point {
        self.snap_moving(at, &[])
    }

    /// `at` snapped as a move of `objects` lands: onto the other Pins' ends.
    fn snap_moving(&self, at: P, objects: &[Uuid]) -> Point {
        let tool = self.shell.tool();
        let pitch = snap_grid(self.shell, self.element(), tool);
        let rules = crate::snap::rules_on(self.shell, self.element(), pitch);
        let targets =
            crate::snap::hotspots::without(crate::snap::hotspots::symbol(self.symbol), objects);
        crate::snap::snap(at, &targets, &[], &rules).at
    }

    fn run(&mut self, id: &str, params: Value) {
        self.commands.push((id.to_owned(), params));
    }

    fn hit(&self, at: P) -> Option<Uuid> {
        crate::selection::picking::hit(self.shell, self.element(), at, self.pick)
    }

    /// Drops what a Tool that is no longer active had in flight.
    fn sync_tool(&self, state: Interaction) -> Interaction {
        match state {
            Interaction::Drafting { tool, .. } if tool != self.shell.tool() => Interaction::Idle,
            Interaction::Moving { .. } | Interaction::Boxing { .. }
                if self.shell.tool() != Tool::Select =>
            {
                Interaction::Idle
            }
            other => other,
        }
    }

    fn step(&mut self, state: &mut Canvas, event: Pointer, hovered: &mut Option<Uuid>) {
        let interaction = std::mem::take(&mut state.interaction);
        state.interaction = match self.shell.tool() {
            Tool::Pin => {
                if let Pointer::Click { at, .. } = event {
                    self.place_pin(at, state.pin_rotation, &mut state.last_pin);
                }
                Interaction::Idle
            }
            Tool::Text => {
                if let Pointer::Click { at, .. } = event
                    && !self.read_only
                    && self.unit.is_some()
                {
                    state.text = Some((self.snap(at), String::new()));
                }
                Interaction::Idle
            }
            tool @ (Tool::Line | Tool::Rect | Tool::Circle | Tool::Arc | Tool::Polygon) => {
                self.draw(tool, interaction, event)
            }
            _ => self.select_step(interaction, event, hovered),
        };
    }

    fn place_pin(&mut self, at: P, rotation: Millideg, last: &mut Option<(String, String)>) {
        let Some(unit) = self.unit.filter(|_| !self.read_only) else {
            return;
        };
        let steps = self.shell.preferences().schematic.auto_increment;
        let (number, name) = next_pin(self.symbol, last.as_ref(), steps.primary, steps.secondary);
        *last = Some((number.clone(), name.clone()));
        let at = self.snap(at);
        self.run(
            "library.symbol.pin.add",
            json!({"symbol": self.element(), "unit": unit, "name": name, "number": number,
                   "at": json_point(at), "rotation": rotation.0}),
        );
    }

    /// A drawing Tool's click (or drag) at `at`: one more corner, and the shape once it
    /// has them all.
    fn draw(&mut self, tool: Tool, state: Interaction, event: Pointer) -> Interaction {
        let Some(unit) = self.unit.filter(|_| !self.read_only) else {
            return Interaction::Idle;
        };
        let mut points = match state {
            Interaction::Drafting { tool: t, points } if t == tool => points,
            _ => Vec::new(),
        };
        let at = match event {
            Pointer::Click { at, .. } => self.snap(at),
            // A drag draws a two-point shape from where it started to where it ends.
            Pointer::DragStart { from, .. } if points.is_empty() => {
                return Interaction::Drafting {
                    tool,
                    points: vec![self.snap(from)],
                };
            }
            Pointer::DragEnd(at) if points.len() == 1 && corners(tool) == Some(2) => self.snap(at),
            _ => return Interaction::Drafting { tool, points },
        };
        if points.last() == Some(&at) {
            return Interaction::Drafting { tool, points };
        }
        let closes = tool == Tool::Polygon && points.len() >= 3 && points[0] == at;
        if !closes {
            points.push(at);
        }
        if !closes && corners(tool).is_none_or(|n| points.len() < n) {
            return Interaction::Drafting { tool, points };
        }
        if let Some(shape) = shape_of(tool, &points) {
            let (id, params) = add_graphic(self.element(), unit, shape);
            self.run(&id, params);
        }
        Interaction::Idle
    }

    fn select_step(
        &mut self,
        state: Interaction,
        event: Pointer,
        hovered: &mut Option<Uuid>,
    ) -> Interaction {
        match (state, event) {
            (
                Interaction::Moving {
                    objects, anchor, ..
                },
                Pointer::Drag(at),
            ) => Interaction::Moving {
                offset: self.snap_moving(at, &objects) - anchor,
                objects,
                anchor,
            },
            (
                Interaction::Moving {
                    objects, anchor, ..
                },
                Pointer::DragEnd(at),
            ) => {
                let offset = self.snap_moving(at, &objects) - anchor;
                if offset != Point::default() && !self.read_only {
                    self.run(
                        "library.symbol.move",
                        json!({"symbol": self.element(), "objects": objects,
                               "by": json_point(offset)}),
                    );
                }
                Interaction::Idle
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
            (Interaction::Idle, Pointer::Hover(at)) => {
                crate::selection::picking::hover(self.shell, self.element(), at, self.pick);
                *hovered = self.hit(at);
                Interaction::Idle
            }
            (Interaction::Idle, Pointer::Click { at, modifiers }) => {
                if let Some(command) = crate::selection::picking::click(
                    self.shell,
                    self.element(),
                    at,
                    self.pick,
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
                    self.element(),
                    from,
                    self.pick,
                ) else {
                    return Interaction::Boxing {
                        from,
                        to: at,
                        modifiers,
                    };
                };
                let parts = element_editor::parts(self.shell.engine(), self.element());
                let drag = crate::selection::picking::drag_start(
                    self.shell,
                    self.element(),
                    hit,
                    modifiers,
                    |id| parts.contains(&id),
                );
                self.commands.extend(drag.select);
                let objects = drag.objects;
                if self.read_only {
                    return Interaction::Idle;
                }
                let anchor = self.snap(from);
                Interaction::Moving {
                    offset: self.snap_moving(at, &objects) - anchor,
                    objects,
                    anchor,
                }
            }
            (state, _) => state,
        }
    }

    fn box_select(&mut self, from: P, to: P, modifiers: egui::Modifiers) {
        if let Some(command) = crate::selection::picking::box_selection(
            self.shell,
            self.element(),
            from,
            to,
            modifiers,
        ) {
            self.commands.push(command);
        }
    }
}

/// How many clicks `tool` takes for one shape; `None` for a polygon, which closes on its
/// first corner or Enter.
fn corners(tool: Tool) -> Option<usize> {
    match tool {
        Tool::Line | Tool::Rect | Tool::Circle => Some(2),
        Tool::Arc => Some(3),
        _ => None,
    }
}

/// The shape `tool` draws through `points`, or `None` when they make none (a circle of
/// no size, an arc of no sweep).
fn shape_of(tool: Tool, points: &[Point]) -> Option<Value> {
    let p = |i: usize| json_point(points[i]);
    Some(match tool {
        Tool::Line => json!({"kind": "line", "start": p(0), "end": p(1)}),
        Tool::Rect if points[0].x != points[1].x && points[0].y != points[1].y => {
            json!({"kind": "rect", "start": p(0), "end": p(1)})
        }
        Tool::Circle => {
            let radius = distance(points[0], points[1]);
            if radius == 0 {
                return None;
            }
            json!({"kind": "circle", "center": p(0), "radius": radius})
        }
        Tool::Arc => {
            let sweep = sweep(points[0], points[1], points[2]);
            if sweep == 0 || points[0] == points[1] {
                return None;
            }
            json!({"kind": "arc", "center": p(0), "start": p(1), "sweep": sweep})
        }
        Tool::Polygon => json!({"kind": "polygon",
                                "points": points.iter().map(|q| json_point(*q)).collect::<Vec<_>>()}),
        _ => return None,
    })
}

fn add_graphic(symbol: Uuid, unit: Uuid, shape: Value) -> Invocation {
    (
        "library.symbol.graphic.add".to_owned(),
        json!({"symbol": symbol, "unit": unit, "shape": shape, "width": LINE_WIDTH.0}),
    )
}

/// The Text Tool's typing is done: the Command that places `text` where it clicked, and
/// the typing ends either way.
pub fn commit_text(shell: &mut Shell, symbol: Uuid, text: &str) -> Option<Invocation> {
    let unit = shown_unit(shell, symbol);
    let (at, _) = canvas_mut(shell, symbol).text.take()?;
    let unit = unit.filter(|_| !text.trim().is_empty())?;
    let shape = json!({"kind": "text", "at": json_point(at), "text": text,
                       "size": TEXT_SIZE.0, "rotation": 0});
    Some(add_graphic(symbol, unit, shape))
}

/// Keeps the Text Tool's typing between frames.
pub(crate) fn set_text(shell: &mut Shell, symbol: Uuid, text: Option<(Point, String)>) {
    canvas_mut(shell, symbol).text = text;
}

/// What a paste into the pin table did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pasted {
    pub added: usize,
    pub updated: usize,
}

/// The one `batch` that applies pasted pin rows to `symbol`: a row whose number is a Pin's
/// updates that Pin, any other adds a Pin to the shown Unit, below the others when it has
/// no position. `Err` says what is wrong with the text.
pub fn paste_command(
    shell: &Shell,
    symbol: Uuid,
    text: &str,
) -> Result<(Invocation, Pasted), String> {
    let rows = parse_pin_rows(text)?;
    let s = symbol_of(shell.engine(), symbol).ok_or("the Symbol is not open")?;
    let unit = shown_unit(shell, symbol).ok_or("the Symbol has no Unit")?;
    let mut next_at = paste_start(s, unit);
    let mut pasted = Pasted::default();
    let mut steps = Vec::new();
    for row in rows {
        let mut params = json!({"symbol": symbol, "name": row.name});
        if let Some(t) = row.electrical_type {
            params["electrical_type"] = json!(t);
        }
        if let Some(r) = row.rotation {
            params["rotation"] = json!(r.0);
        }
        if let Some(l) = row.length {
            params["length"] = json!(l.0);
        }
        if let Some(at) = row.at {
            params["at"] = json_point(at);
        }
        match s.pins.iter().find(|p| p.number == row.number) {
            Some(pin) => {
                params["pin"] = json!(pin.uuid);
                steps.push(json!({"id": "library.symbol.pin.edit", "params": params}));
                pasted.updated += 1;
            }
            None => {
                params["number"] = json!(row.number);
                params["unit"] = json!(unit);
                if row.at.is_none() {
                    params["at"] = json_point(next_at);
                    next_at.y = next_at.y - PASTE_PITCH;
                }
                steps.push(json!({"id": "library.symbol.pin.add", "params": params}));
                pasted.added += 1;
            }
        }
    }
    if steps.is_empty() {
        return Err(tr!("pin-table-paste-empty"));
    }
    Ok((("batch".to_owned(), json!({ "commands": steps })), pasted))
}

/// Applies pasted pin rows to `symbol` as one undo step (see [`paste_command`]).
pub fn paste_pins(shell: &mut Shell, symbol: Uuid, text: &str) -> Result<Pasted, CommandError> {
    let ((id, params), pasted) =
        paste_command(shell, symbol, text).map_err(|why| CommandError::invalid_params("", why))?;
    shell.run_reporting(&id, params)?;
    Ok(pasted)
}

/// Where the first pasted Pin without a position goes: under the lowest Pin of the
/// left-most column of `unit`, or at the paste column with none.
fn paste_start(symbol: &Symbol, unit: Uuid) -> Point {
    let pins: Vec<&Pin> = symbol.pins.iter().filter(|p| p.unit == unit).collect();
    let Some(x) = pins.iter().map(|p| p.at.x).min() else {
        return Point::new(PASTE_COLUMN_X, Nm(0));
    };
    let lowest = pins
        .iter()
        .filter(|p| p.at.x == x)
        .map(|p| p.at.y)
        .min()
        .unwrap_or(Nm(0));
    Point::new(x, lowest - PASTE_PITCH)
}

/// The number and name of the next Pin the Pin Tool places in `symbol`, after `last`
/// (the number and name of the one it placed before, if any): the number steps by
/// `primary` (letters too, A → B, skipping numbers taken) and the name by `secondary`
/// (its trailing number only), as the Pin now is if it is still there under that number,
/// so edits made to it carry on. With nothing to step from, the number is one past the
/// highest numeric one and the name is empty.
pub fn next_pin(
    symbol: &Symbol,
    last: Option<&(String, String)>,
    primary: i64,
    secondary: i64,
) -> (String, String) {
    let taken = |n: &str| symbol.pins.iter().any(|p| p.number == n);
    let stepped = last.and_then(|(number, name)| {
        let name = symbol
            .pins
            .iter()
            .find(|p| p.number == *number)
            .map_or(name.as_str(), |p| p.name.as_str());
        let next = auto_increment::next_number(number, primary, |n| !taken(n))?;
        Some((next, auto_increment::step(name, secondary)))
    });
    stepped.unwrap_or_else(|| {
        let highest = symbol
            .pins
            .iter()
            .filter_map(|p| p.number.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        let mut n = highest + 1;
        while taken(&n.to_string()) {
            n += 1;
        }
        (n.to_string(), String::new())
    })
}

fn json_point(p: Point) -> Value {
    json!({"x": p.x.0, "y": p.y.0})
}

fn distance(a: Point, b: Point) -> i64 {
    let (dx, dy) = ((b.x - a.x).0 as f64, (b.y - a.y).0 as f64);
    dx.hypot(dy).round() as i64
}

/// The counter-clockwise sweep (millidegrees, in (0, 360°)) from `start` to `end` around
/// `center`.
fn sweep(center: Point, start: Point, end: Point) -> i64 {
    let angle = |p: Point| ((p.y - center.y).0 as f64).atan2((p.x - center.x).0 as f64);
    let mut degrees = (angle(end) - angle(start)).to_degrees();
    if degrees <= 0.0 {
        degrees += 360.0;
    }
    ((degrees * 1000.0).round() as i64) % 360_000
}

/// Opens the Library Symbol of the placed Unit `unit` (or the Component it belongs to)
/// in the Symbol editor: Edit in Library. On Source Missing it offers Extract to Library
/// instead.
pub(crate) fn edit_in_library(shell: &mut Shell, unit: Uuid) {
    let component = shell.engine().project().and_then(|p| {
        p.documents()
            .filter_map(|d| p.schematic_sheet(d.uuid))
            .flat_map(|s| s.units())
            .find(|u| u.uuid == unit || u.component.uuid == unit)
            .map(|u| u.component.uuid)
    });
    let Some(component) = component else {
        shell.report(tr!("error-edit-in-library-nothing"));
        return;
    };
    // On Source Missing there is nothing to open: offer to extract it to a Library.
    let index = shell.library_index();
    let missing = shell.engine().project().is_some_and(|p| {
        reda_engine::library::library_statuses(p, &index)
            .iter()
            .any(|s| {
                s.component == component
                    && s.status == reda_engine::schematic::LibraryStatus::SourceMissing
            })
    });
    if missing {
        shell.queue_call(
            "library.extract_from_project",
            json!({ "components": [component] }),
        );
        return;
    }
    let Ok(found) = shell.run_reporting(
        "schematic.component.library_element",
        json!({ "component": component }),
    ) else {
        return;
    };
    let Some(element) = found["symbol"]["element"].as_str() else {
        shell.report(tr!("error-edit-in-library-nothing"));
        return;
    };
    let element = element.to_owned();
    let _ = shell.run_reporting("library.element.open", json!({ "element": element }));
}

/// `library.symbol.unit.show`: which Unit of an open Symbol its editor shows (the Unit
/// tabs over the canvas). The Tools draw on that Unit.
struct UnitShow;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitShowParams {
    /// The open Symbol.
    symbol: Uuid,
    /// One of its Units.
    unit: Uuid,
}

/// The Symbol and the Unit it now shows.
#[derive(Serialize, JsonSchema)]
pub struct UnitShown {
    symbol: Uuid,
    unit: Uuid,
}

impl Command for UnitShow {
    const META: Meta = Meta {
        id: "library.symbol.unit.show",
        title: "Show Unit",
        description: "Shows one Unit of an open Symbol in its editor, as clicking its Unit \
                      tab does. The editor's Tools then draw on that Unit.",
        category: "library",
        icon: "symbol_edit",
        kind: Kind::View,
    };
    type Params = UnitShowParams;
    type Output = UnitShown;

    fn run(&self, cx: &mut Context<'_>, p: UnitShowParams) -> Result<UnitShown, CommandError> {
        // A view Command sees the open Project, else the workbench: where elements open.
        let symbol = match cx.project()?.element(p.symbol) {
            Some(LibraryElement::Symbol(s)) => s.clone(),
            _ => {
                return Err(CommandError::not_found(format!(
                    "no open Symbol has UUID {}",
                    p.symbol
                )));
            }
        };
        if !symbol.units.iter().any(|u| u.uuid == p.unit) {
            return Err(CommandError::not_found(format!(
                "the Symbol has no Unit {}",
                p.unit
            )));
        }
        let canvas = cx
            .view::<SymbolEditorState>()?
            .canvases
            .entry(p.symbol)
            .or_default();
        canvas.unit = Some(p.unit);
        canvas.interaction = Interaction::Idle;
        canvas.text = None;
        Ok(UnitShown {
            symbol: p.symbol,
            unit: p.unit,
        })
    }
}

/// `library.element.edit_in_library`: opens the Library Symbol of a placed Component in
/// the Symbol editor. The window's Shell opens it after the Command runs.
struct EditInLibrary;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditInLibraryParams {
    /// Selected objects; the first placed Unit among them is used.
    objects: Vec<Uuid>,
}

/// The placed Unit whose Symbol opens.
#[derive(Serialize, JsonSchema)]
pub struct EditInLibraryResult {
    object: Uuid,
}

impl Command for EditInLibrary {
    const META: Meta = Meta {
        id: "library.element.edit_in_library",
        title: "Edit in Library",
        description: "Opens the Library Symbol of the first selected placed Component in \
                      the Symbol editor.",
        category: "library",
        icon: "library_element_open",
        kind: Kind::View,
    };
    type Params = EditInLibraryParams;
    type Output = EditInLibraryResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: EditInLibraryParams,
    ) -> Result<EditInLibraryResult, CommandError> {
        let project = cx.project()?;
        let is_unit = |uuid: &Uuid| {
            project.documents().any(|d| {
                project
                    .schematic_sheet(d.uuid)
                    .is_some_and(|s| s.units().any(|u| u.uuid == *uuid))
            })
        };
        let object = p.objects.into_iter().find(is_unit).ok_or_else(|| {
            CommandError::rejected("select a placed Component to edit it in its Library")
        })?;
        Ok(EditInLibraryResult { object })
    }
}
