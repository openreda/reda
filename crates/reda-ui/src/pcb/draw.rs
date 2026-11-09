//! The PCB drawing Tools: Board Outline (polygon or rectangle) and Cutouts, Zones and
//! Keepouts, text, lines, arcs and rectangles on the active Layer, Free Pads, the
//! Output Origin and the measure Tool. (Mechanical Parts are placed
//! from the Library panel by the placement Tool, `crate::placement`.)
//!
//! Each is a small state machine over the same pointer events as the select Tool
//! (press, drag, release, hover), fed by the Shell while a drawing [`Tool`] is active:
//!
//! - **Polygons** (outline, cutout, Zone, Keepout): each click puts down a corner; a click
//!   back on the first corner, on the last one again, or `tool.finish` (Enter) closes it.
//!   `tool.undo_segment` (Backspace) takes back the last corner.
//! - **Two points** (outline rectangle, line, rectangle): two clicks, or one drag.
//! - **Arcs**: the two ends, then a point the arc passes through.
//! - **One point** (text, Free Pad, Output Origin): a click; text then
//!   asks for its words ([`Draw::pending_text`]).
//! - **Measure**: a click starts, the cursor follows, a second click holds it.
//!
//! Nothing runs while a shape is drawn: [`Draw::preview`] gives what to draw over the
//! canvas. A finished shape runs exactly one Command; the measure Tool changes
//! nothing. Points snap like the select Tool's (Pads, Track ends and
//! Vias, else the grid).

mod geometry;

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{LayerKind, Project, Uuid};
use reda_engine::pcb::Copper;
use reda_engine::preferences::KeepoutToolShape;
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::snap::{self, Snapped};
use crate::tools::Tool;
use crate::tr;

pub use geometry::arc_through;

/// The Free Pad the Free Pad Tool places: plated, round, 2 mm across, with a 1 mm hole.
const FREE_PAD_WIDTH: &str = "2mm";
const FREE_PAD_HOLE: &str = "1mm";

/// What a Tool in progress draws over the canvas, in board nm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// The path so far, the cursor included.
    pub points: Vec<Point>,
    /// Whether the path closes back to its first point.
    pub closed: bool,
}

/// A measurement between two points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measurement {
    pub from: Point,
    pub to: Point,
}

impl Measurement {
    /// The straight distance, to the nearest nm.
    pub fn distance(&self) -> Nm {
        let (dx, dy) = (
            (self.to.x.0 - self.from.x.0) as f64,
            (self.to.y.0 - self.from.y.0) as f64,
        );
        Nm(dx.hypot(dy).round() as i64)
    }
}

/// How a Tool takes its points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Polygon,
    TwoPoints,
    Arc,
    Point,
    Measure,
    None,
}

fn shape(tool: Tool, keepout: KeepoutToolShape) -> Shape {
    match tool {
        Tool::Keepout if keepout != KeepoutToolShape::Polygon => Shape::TwoPoints,
        Tool::BoardOutline | Tool::BoardCutout | Tool::Zone | Tool::ZoneCutout | Tool::Keepout => {
            Shape::Polygon
        }
        Tool::BoardOutlineRect | Tool::Line | Tool::Rect | Tool::MoveBy | Tool::Swap => {
            Shape::TwoPoints
        }
        Tool::Arc => Shape::Arc,
        Tool::Text | Tool::FreePad | Tool::OutputOrigin | Tool::RotateAbout => Shape::Point,
        Tool::Measure => Shape::Measure,
        // Not a board drawing Tool (see `Tool::draws_on_board`).
        _ => Shape::None,
    }
}

/// The drawing Tools' state between events.
#[derive(Debug)]
pub struct Draw {
    tool: Tool,
    /// The points put down so far.
    points: Vec<Point>,
    /// Where the cursor snapped last.
    cursor: Option<Snapped>,
    /// Where the button went down, snapped, and whether it has moved since.
    pressed: Option<Point>,
    dragged: bool,
    /// Where text waits for its words.
    text_at: Option<Point>,
    measure: Option<Measurement>,
    /// Whether the measurement is held (a second click), not following the cursor.
    held: bool,
    /// The Keepout Tool's shape, from its Preference when the cursor last moved.
    keepout: KeepoutToolShape,
}

impl Default for Draw {
    fn default() -> Draw {
        Draw {
            tool: Tool::Select,
            points: Vec::new(),
            cursor: None,
            pressed: None,
            dragged: false,
            text_at: None,
            measure: None,
            held: false,
            keepout: KeepoutToolShape::Polygon,
        }
    }
}

impl Draw {
    /// Starts `tool` afresh, dropping anything in progress.
    pub fn start(&mut self, tool: Tool) {
        *self = Draw {
            tool,
            ..Draw::default()
        };
    }

    fn shape(&self) -> Shape {
        shape(self.tool, self.keepout)
    }

    /// The Tool being drawn with.
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// Where the cursor last snapped, for the snap marker.
    pub fn snap(&self) -> Option<Snapped> {
        self.cursor
    }

    /// What to draw over the canvas while a shape is in progress.
    pub fn preview(&self) -> Option<Preview> {
        let cursor = self.cursor.map(|s| s.at);
        match self.shape() {
            Shape::Polygon => {
                if self.points.is_empty() {
                    return None;
                }
                let mut points = self.points.clone();
                points.extend(cursor.filter(|c| points.last() != Some(c)));
                let closed = points.len() > 2;
                Some(Preview { points, closed })
            }
            Shape::TwoPoints => {
                let (&a, b) = (self.points.first()?, cursor?);
                Some(match self.tool {
                    Tool::Keepout if self.keepout == KeepoutToolShape::Circle => Preview {
                        points: geometry::circle_points(a, b),
                        closed: true,
                    },
                    Tool::Line | Tool::MoveBy | Tool::Swap => Preview {
                        points: vec![a, b],
                        closed: false,
                    },
                    _ => Preview {
                        points: geometry::rect_corners(a, b).to_vec(),
                        closed: true,
                    },
                })
            }
            Shape::Arc => {
                let c = cursor?;
                match self.points[..] {
                    [a] => Some(Preview {
                        points: vec![a, c],
                        closed: false,
                    }),
                    [a, b] => Some(Preview {
                        points: geometry::arc_points(a, b, c).unwrap_or_else(|| vec![a, c, b]),
                        closed: false,
                    }),
                    _ => None,
                }
            }
            Shape::Point | Shape::Measure | Shape::None => None,
        }
    }

    /// The measurement shown, while the measure Tool has one.
    pub fn measurement(&self) -> Option<Measurement> {
        self.measure.filter(|_| self.tool == Tool::Measure)
    }

    /// Where text waits for its words, after a click with the text Tool.
    pub fn pending_text(&self) -> Option<Point> {
        self.text_at
    }

    /// The cursor is at `at` with no button down.
    pub fn hover(&mut self, shell: &Shell, at: Point) -> Snapped {
        let keepout = shell.preferences().pcb.keepout.shape;
        if keepout != self.keepout && self.points.is_empty() {
            self.keepout = keepout;
        }
        let snapped = self.snapped(shell, at);
        self.cursor = Some(snapped);
        if let Some(m) = &mut self.measure
            && !self.held
        {
            m.to = snapped.at;
        }
        snapped
    }

    /// A button went down at `at`.
    pub fn press(&mut self, shell: &Shell, at: Point) {
        let p = self.hover(shell, at).at;
        self.pressed = Some(p);
        self.dragged = false;
    }

    /// The pointer moved to `at` with the button down.
    pub fn drag(&mut self, shell: &Shell, at: Point) {
        self.hover(shell, at);
        self.dragged = true;
    }

    /// The button came up: a click puts down the press point; a drag puts down where it
    /// ended, and for a two-point shape not yet started, both ends.
    pub fn release(&mut self, shell: &mut Shell) {
        let Some(pressed) = self.pressed.take() else {
            return;
        };
        let end = self.cursor.map_or(pressed, |s| s.at);
        if self.dragged && end != pressed {
            if self.shape() == Shape::TwoPoints && self.points.is_empty() {
                self.put(shell, pressed);
            }
            self.put(shell, end);
        } else {
            self.put(shell, pressed);
        }
        self.dragged = false;
    }

    /// `tool.finish` (Enter): closes a polygon with three corners or more.
    pub fn finish(&mut self, shell: &mut Shell) {
        if self.shape() == Shape::Polygon && self.points.len() > 2 {
            let points = std::mem::take(&mut self.points);
            self.commit(shell, &points);
        }
    }

    /// `tool.undo_segment` (Backspace): takes back the last point.
    pub fn undo_segment(&mut self) {
        if self.points.pop().is_none() && self.tool == Tool::Measure {
            self.measure = None;
        }
    }

    /// Esc, or another Tool: drops what is in progress.
    pub fn cancel(&mut self) {
        let tool = self.tool;
        self.start(tool);
    }

    /// Text's words arrived: places it where the text Tool was clicked.
    pub fn place_text(&mut self, shell: &mut Shell, text: &str) {
        let Some(at) = self.text_at.take() else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }
        let Some(layer) = graphics_layer(shell) else {
            return;
        };
        let _ = shell.run_reporting(
            "pcb.text.add",
            json!({"layer": layer, "text": text, "x": at.x, "y": at.y}),
        );
    }

    /// Drops text waiting for its words.
    pub fn cancel_text(&mut self) {
        self.text_at = None;
    }

    fn snapped(&self, shell: &Shell, at: Point) -> Snapped {
        let mut targets = super::tool::board_hotspots(shell);
        // A polygon's own first corner catches the cursor, to close it.
        if self.shape() == Shape::Polygon
            && self.points.len() > 2
            && let Some(&first) = self.points.first()
        {
            targets.push(snap::Target::point(
                first,
                snap::Hotspot::OutlineVertex,
                Uuid::NIL,
                snap::OnLayers::Every,
            ));
        }
        super::tool::snap_board(shell, at, &targets)
    }

    fn radius(&self, shell: &Shell) -> Nm {
        shell.preferences().pcb.snap_distance.nm()
    }

    /// Puts down point `p`.
    fn put(&mut self, shell: &mut Shell, p: Point) {
        let first = self.points.is_empty();
        match self.shape() {
            Shape::Polygon => {
                let closes = self.points.len() > 2
                    && (self.points.last() == Some(&p)
                        || near(self.points[0], p, self.radius(shell)));
                if closes {
                    let points = std::mem::take(&mut self.points);
                    self.commit(shell, &points);
                } else if self.points.last() != Some(&p) && (!first || self.can_start(shell, p)) {
                    self.points.push(p);
                }
            }
            Shape::TwoPoints => {
                if first {
                    if self.can_start(shell, p) {
                        self.points.push(p);
                    }
                } else if self.points[0] != p {
                    let points = [self.points[0], p];
                    self.points.clear();
                    self.commit(shell, &points);
                }
            }
            Shape::Arc => {
                if (first && !self.can_start(shell, p)) || self.points.contains(&p) {
                    return;
                }
                self.points.push(p);
                if self.points.len() == 3 {
                    let points = std::mem::take(&mut self.points);
                    self.commit(shell, &points);
                }
            }
            Shape::Point => {
                if self.tool == Tool::Text {
                    if self.can_start(shell, p) {
                        self.text_at = Some(p);
                    }
                } else {
                    self.commit(shell, &[p]);
                }
            }
            Shape::Measure => {
                if self.measure.is_none() || self.held {
                    self.measure = Some(Measurement { from: p, to: p });
                    self.held = false;
                } else if let Some(m) = &mut self.measure {
                    m.to = p;
                    self.held = true;
                }
            }
            Shape::None => {}
        }
    }

    /// Whether the active Layer suits the Tool; reports why not.
    fn can_start(&self, shell: &mut Shell, p: Point) -> bool {
        let problem = match self.tool {
            Tool::Zone | Tool::ZoneCutout => {
                let copper = active_layer(shell).is_some_and(|(kind, _)| kind == LayerKind::Copper);
                (!copper).then(|| tr!("pcb-draw-needs-copper"))
            }
            Tool::Keepout if !shell.preferences().pcb.keepout.all_layers => {
                let copper = active_layer(shell).is_some_and(|(kind, _)| kind == LayerKind::Copper);
                (!copper).then(|| tr!("pcb-draw-needs-copper"))
            }
            Tool::Text | Tool::Line | Tool::Arc | Tool::Rect => graphics_layer(shell)
                .is_none()
                .then(|| tr!("pcb-draw-needs-graphics-layer")),
            Tool::MoveBy | Tool::RotateAbout => shell
                .selection()
                .objects
                .is_empty()
                .then(|| tr!("pcb-draw-needs-selection")),
            Tool::Swap => footprint_at(shell, Some(p))
                .is_none()
                .then(|| tr!("pcb-swap-pick-footprint")),
            _ => None,
        };
        match problem {
            Some(message) => {
                shell.report(message);
                false
            }
            None => true,
        }
    }

    /// Runs the one Command the finished shape `points` makes.
    fn commit(&mut self, shell: &mut Shell, points: &[Point]) {
        let Some((id, params)) = self.command(shell, points) else {
            return;
        };
        let _ = shell.run_reporting(id, params);
    }

    fn command(&self, shell: &mut Shell, points: &[Point]) -> Option<(&'static str, Value)> {
        let vertices = || -> Value { points.iter().map(|p| xy(*p)).collect() };
        let layer = || graphics_layer(shell);
        Some(match self.tool {
            Tool::BoardOutline => ("pcb.board_outline.add", json!({"vertices": vertices()})),
            Tool::BoardOutlineRect => {
                let corners = geometry::rect_corners(points[0], points[1]);
                let vertices: Vec<Value> = corners.iter().map(|p| xy(*p)).collect();
                ("pcb.board_outline.add", json!({ "vertices": vertices }))
            }
            Tool::BoardCutout => ("pcb.board_cutout.add", json!({"vertices": vertices()})),
            Tool::Zone => {
                let project = shell.engine().project()?;
                let (layer, net) = zone_layer_and_net(shell, project, points[0])?;
                let mut params = json!({"layer": layer, "vertices": vertices()});
                if let Some(net) = net {
                    params["net"] = json!(net);
                }
                zone_tool_values(shell, &mut params);
                ("pcb.zone.add", params)
            }
            Tool::ZoneCutout => {
                let (_, layer) = active_layer(shell)?;
                (
                    "pcb.zone_cutout.add",
                    json!({"layer": layer, "vertices": vertices()}),
                )
            }
            Tool::Keepout => {
                let options = shell.preferences().pcb.keepout.clone();
                let vertices: Value = match (options.shape, points) {
                    (KeepoutToolShape::Rectangle, [a, b]) => geometry::rect_corners(*a, *b)
                        .iter()
                        .map(|p| xy(*p))
                        .collect(),
                    (KeepoutToolShape::Circle, [c, edge]) => circle(*c, *edge)?,
                    _ => vertices(),
                };
                let mut params = json!({"vertices": vertices, "restrict": options.restrict});
                if !options.all_layers {
                    let (_, layer) = active_layer(shell)?;
                    params["layer"] = json!(layer);
                }
                ("pcb.keepout.add", params)
            }
            Tool::Line => (
                "pcb.graphic.add_line",
                json!({"layer": layer()?, "from": xy(points[0]), "to": xy(points[1])}),
            ),
            Tool::Rect => (
                "pcb.graphic.add_rect",
                json!({"layer": layer()?, "from": xy(points[0]), "to": xy(points[1])}),
            ),
            Tool::Arc => {
                let Some(sweep) = arc_through(points[0], points[1], points[2]) else {
                    shell.report(tr!("pcb-draw-arc-straight"));
                    return None;
                };
                (
                    "pcb.graphic.add_arc",
                    json!({"layer": layer()?, "from": xy(points[0]), "to": xy(points[1]),
                           "sweep": sweep.0}),
                )
            }
            Tool::FreePad => (
                "pcb.free_pad.add",
                json!({"kind": "plated_through_hole", "shape": "circle", "at": xy(points[0]),
                       "width": FREE_PAD_WIDTH, "hole": {"width": FREE_PAD_HOLE}}),
            ),
            Tool::OutputOrigin => ("pcb.output_origin.set", json!({"at": xy(points[0])})),
            Tool::MoveBy => {
                let by = points[1] - points[0];
                (
                    "pcb.move",
                    json!({"objects": shell.selection().objects, "by": xy(by)}),
                )
            }
            Tool::RotateAbout => {
                let angle = shell
                    .engine()
                    .view()
                    .get::<crate::tools::RotateAboutAngle>()
                    .copied()
                    .unwrap_or_default();
                (
                    "pcb.rotate",
                    json!({"objects": shell.selection().objects, "center": xy(points[0]),
                           "angle": angle.0.0}),
                )
            }
            Tool::Swap => {
                let a = footprint_at(shell, Some(points[0]))?;
                let Some(b) = footprint_at(shell, Some(points[1])).filter(|b| *b != a) else {
                    shell.report(tr!("pcb-swap-pick-footprint"));
                    return None;
                };
                ("pcb.swap", json!({"a": a, "b": b}))
            }
            // Text and Measure commit elsewhere; the rest are not board drawing Tools.
            _ => return None,
        })
    }
}

/// The Zone Tool's values for a new Zone, from its Preference, into `params`.
fn zone_tool_values(shell: &Shell, params: &mut Value) {
    let values = &shell.preferences().pcb.zone.tool;
    if let Some(c) = values.clearance {
        params["clearance"] = json!(c.nm().0);
    }
    if let Some(w) = values.min_width {
        params["min_width"] = json!(w.nm().0);
    }
    params["pour_over"] = json!(values.pour_over);
    params["remove_unconnected"] = json!(values.remove_unconnected);
    params["min_island_area"] = json!(values.min_island_area.0);
}

/// A circle about `centre` through `edge`, as two half-circle arcs.
fn circle(centre: Point, edge: Point) -> Option<Value> {
    if centre == edge {
        return None;
    }
    let opposite = centre + (centre - edge);
    Some(json!([
        {"x": edge.x.0, "y": edge.y.0, "sweep": 180_000},
        {"x": opposite.x.0, "y": opposite.y.0, "sweep": 180_000},
    ]))
}

/// `p` as a point param, in nm.
fn xy(p: Point) -> Value {
    json!({"x": p.x.0, "y": p.y.0})
}

fn near(a: Point, b: Point, radius: Nm) -> bool {
    let (dx, dy) = ((a.x.0 - b.x.0) as f64, (a.y.0 - b.y.0) as f64);
    dx.hypot(dy) <= radius.0 as f64
}

/// The active Layer's kind and side.
fn active_layer(shell: &Shell) -> Option<(LayerKind, Uuid)> {
    let project = shell.engine().project()?;
    let uuid = shell.board_view().active_layer(project)?;
    let layer = project.pcb()?.layers().layer(uuid)?;
    Some((layer.kind, uuid))
}

/// The active Layer, when free text and graphics may go on it: copper, silkscreen,
/// mechanical or documentation.
fn graphics_layer(shell: &Shell) -> Option<Uuid> {
    let (kind, uuid) = active_layer(shell)?;
    matches!(
        kind,
        LayerKind::Copper
            | LayerKind::Silkscreen
            | LayerKind::Mechanical
            | LayerKind::Documentation
    )
    .then_some(uuid)
}

/// The active copper Layer, and the Net of the copper under `first` on it, which a new
/// Zone takes.
fn zone_layer_and_net(
    shell: &Shell,
    project: &Project,
    first: Point,
) -> Option<(Uuid, Option<Uuid>)> {
    let (kind, layer) = active_layer(shell)?;
    if kind != LayerKind::Copper {
        return None;
    }
    let net = Copper::of(project)?.net_at(layer, first);
    Some((layer, net))
}

/// The Footprint under `at`: the smallest whose placed box holds it.
fn footprint_at(shell: &Shell, at: Option<Point>) -> Option<Uuid> {
    let at = at?;
    let pcb = shell.engine().project()?.pcb()?;
    pcb.footprints()
        .map(|f| (f.uuid, reda_engine::pcb::placed::footprint_bounds(pcb, f)))
        .filter(|(_, b)| b.min.x <= at.x && at.x <= b.max.x && b.min.y <= at.y && at.y <= b.max.y)
        .min_by_key(|(_, b)| i128::from(b.width().0) * i128::from(b.height().0))
        .map(|(uuid, _)| uuid)
}
