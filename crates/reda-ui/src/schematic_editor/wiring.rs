//! The wiring Tools on a sheet: the Wire Tool's runs, and the Net Label, Port, Power Port
//! and No-Connect Tools, which place one object per click.
//!
//! A run in flight touches neither the Project nor the Journal: each click (or Enter,
//! `tool.finish`) fixes a corner, and the run ends, as ONE `schematic.wire.add`, when a
//! click lands on a Pin or a Wire, or on the same point twice (as a double-click does).
//! The Wire Tool's mode ([`WireMode`], Shift+Space `tool.cycle_wire_mode`) shapes the way
//! to the pointer: 90° and 45° turn one corner there, and the segment on the pointer is a
//! look-ahead that a click leaves unfixed; Any Angle goes straight; Auto Wire routes each
//! click around placed objects (`schematic.wire.route`) and draws the route dotted. Space
//! or `/` turns the corner the other way, Insert adds a vertex at the pointer and Backspace
//! or Delete takes the last segment off (`tool.cycle_corner`, `tool.insert_vertex`,
//! `tool.undo_segment`). Esc ends a run with its fixed corners (`tool.activate` with
//! Select: see [`crate::tools::ToolStack`]).

use reda_engine::model::geom::Nm;
use reda_engine::model::geom::{Millideg, Point};
use reda_engine::model::{
    InDocument, Justify, NetLabel, NoConnect, Object, Port, PortDirection, PowerPort, Project,
    SheetText, Uuid,
};
use reda_engine::schematic::connectivity::placed_pins;
use reda_engine::schematic::default_power_port_style;
use serde_json::{Value, json};

use super::{Cx, Interaction, P};
pub use crate::tools::WireMode;
use crate::tools::{PowerPortName, Tool};

/// Which way a Wire run turns its corner on the way to the pointer: the start and end
/// corner modes Space toggles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Corner {
    /// 90°: along x first, then along y. 45°: straight first, then the 45° stretch.
    #[default]
    HorizontalFirst,
    /// 90°: along y first, then along x. 45°: the 45° stretch first, then straight.
    VerticalFirst,
}

/// The Wire Tool: the run in flight, if any, and where the pointer snapped.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wiring {
    /// The run's fixed corners from its start; empty while no run is in flight.
    pub points: Vec<Point>,
    /// Where the pointer snapped.
    pub at: Point,
    pub corner: Corner,
    /// The Tool's mode, as the session keeps it ([`crate::tools::WireTool`]).
    pub mode: WireMode,
    /// The fixed segments Auto Wire routed, drawn dotted.
    pub routed: Vec<(Point, Point)>,
    /// Where the last click fixed the run: a second click there ends it.
    pub(super) last_click: Option<Point>,
    /// A finished run's segments, still drawn until the Shell has run its Command.
    pub committed: Vec<(Point, Point)>,
    /// The run has started the Shell's Tool preview (so a bridged edit can cancel it).
    pub(super) previewing: bool,
}

impl Wiring {
    /// The run as it would be drawn to the pointer: its fixed segments, then the corner to
    /// where the pointer snapped.
    pub fn segments(&self) -> Vec<(Point, Point)> {
        if self.points.is_empty() {
            return Vec::new();
        }
        let path = self.path_to(self.at);
        path.windows(2).map(|w| (w[0], w[1])).collect()
    }

    /// The run as the canvas draws it: (solid, dotted) segments. Auto Wire's routes, and
    /// in Auto Wire the line on to the pointer, are dotted.
    pub fn drawn(&self) -> (Vec<(Point, Point)>, Vec<(Point, Point)>) {
        let (mut solid, mut dotted): (Vec<_>, Vec<_>) = self
            .segments()
            .into_iter()
            .partition(|s| !self.routed.contains(s));
        if self.mode == WireMode::Auto
            && let Some(&last) = self.points.last()
            && last != self.at
        {
            solid.retain(|s| *s != (last, self.at));
            dotted.push((last, self.at));
        }
        (solid, dotted)
    }

    /// The corner the run turns on the way from `last` to `to`, in its mode.
    fn elbow(&self, last: Point, to: Point) -> Option<Point> {
        let (dx, dy) = ((to.x - last.x).0, (to.y - last.y).0);
        match self.mode {
            WireMode::Ortho if dx != 0 && dy != 0 => Some(match self.corner {
                Corner::HorizontalFirst => Point::new(to.x, last.y),
                Corner::VerticalFirst => Point::new(last.x, to.y),
            }),
            WireMode::Diagonal if dx != 0 && dy != 0 && dx.abs() != dy.abs() => {
                let d = dx.abs().min(dy.abs());
                let (ddx, ddy) = (Nm(d * dx.signum()), Nm(d * dy.signum()));
                Some(match self.corner {
                    Corner::HorizontalFirst => Point::new(to.x - ddx, to.y - ddy),
                    Corner::VerticalFirst => Point::new(last.x + ddx, last.y + ddy),
                })
            }
            _ => None,
        }
    }

    /// The run's corners when it goes on to `to`: straight stretches merged, no empty
    /// segments.
    fn path_to(&self, to: Point) -> Vec<Point> {
        let mut path = self.points.clone();
        let Some(&last) = path.last() else {
            return vec![to];
        };
        if let Some(elbow) = self.elbow(last, to) {
            push(&mut path, elbow);
        }
        push(&mut path, to);
        path
    }

    /// Takes the last fixed segment off; with only the start left, the run ends.
    pub(super) fn remove_last(&mut self) {
        self.points.pop();
        let points = &self.points;
        self.routed
            .retain(|s| points.windows(2).any(|w| (w[0], w[1]) == *s));
        self.last_click = None;
    }
}

/// Appends `p` to the run `path`, skipping an empty segment and merging a segment that
/// goes straight on from the one before.
fn push(path: &mut Vec<Point>, p: Point) {
    if path.last() == Some(&p) {
        return;
    }
    if let [.., a, b] = path[..] {
        let (ux, uy) = ((b.x - a.x).0 as i128, (b.y - a.y).0 as i128);
        let (vx, vy) = ((p.x - b.x).0 as i128, (p.y - b.y).0 as i128);
        if ux * vy == uy * vx && ux * vx + uy * vy > 0 {
            path.pop();
        }
    }
    path.push(p);
}

/// A Tool that places one object per click: Net Label, Port, Power Port, No-Connect or
/// Text (whose words the window then asks for).
#[derive(Clone, Debug, PartialEq)]
pub struct Placing {
    pub tool: Tool,
    /// Where the object goes: the pointer, snapped.
    pub at: Point,
    /// Counter-clockwise; Space turns it, X and Y flip it.
    pub rotation: Millideg,
    /// The Net name the next object gets (none for a No-Connect Marker; for Text, the
    /// words the preview shows).
    pub name: String,
    /// What the canvas shows: the preview drawn for (at, rotation, name), and the Scene
    /// generation it was drawn as.
    pub(super) shown: Option<(Point, Millideg, String, u64)>,
}

impl Placing {
    /// The object the next click places, as it would be on `document`.
    pub(super) fn object(&self, document: Uuid, uuid: Uuid) -> Option<Object> {
        let (name, at, rotation) = (self.name.clone(), self.at, self.rotation);
        Some(match self.tool {
            Tool::NetLabel => Object::NetLabel(InDocument::new(
                document,
                NetLabel {
                    uuid,
                    name,
                    at,
                    rotation,
                },
            )),
            Tool::Port => Object::Port(InDocument::new(
                document,
                Port {
                    uuid,
                    name,
                    direction: PortDirection::default(),
                    at,
                    rotation,
                    hide_cross_reference: false,
                },
            )),
            Tool::PowerPort => Object::PowerPort(InDocument::new(
                document,
                PowerPort {
                    uuid,
                    style: default_power_port_style(&name),
                    name,
                    at,
                    rotation,
                    hide_name: false,
                },
            )),
            Tool::NoConnect => Object::NoConnect(InDocument::new(document, NoConnect { uuid, at })),
            Tool::Text => Object::SheetText(InDocument::new(
                document,
                SheetText {
                    uuid,
                    text: name,
                    at,
                    height: Nm::from_mil(50),
                    rotation,
                    justify: Justify::default(),
                },
            )),
            _ => return None,
        })
    }

    /// Turns what is being placed a quarter turn counter-clockwise, or flips it (`x` left
    /// to right, `y` top to bottom), as `schematic.rotate`/`mirror` would.
    pub(super) fn turn(&mut self, how: super::Turn) {
        let half = Millideg::from_deg(180);
        self.rotation = match how {
            super::Turn::Rotate => self.rotation + Millideg::from_deg(90),
            super::Turn::MirrorX => half - self.rotation,
            super::Turn::MirrorY => -self.rotation,
        }
        .normalized();
    }
}

/// The names a Net Label, Port or Power Port Tool starts with, or the next one after
/// `name` in a run: Net Labels and Ports count up (NET1, NET2…) past every name used on the
/// Project; Power Ports keep theirs (GND).
fn next_name(project: Option<&Project>, tool: Tool, name: Option<&str>) -> String {
    let prefix = match tool {
        Tool::NetLabel => "NET",
        Tool::Port => "PORT",
        Tool::PowerPort => return name.unwrap_or("GND").to_owned(),
        Tool::Text => return crate::tr!("schematic-text-preview"),
        _ => return String::new(),
    };
    let mut used = std::collections::BTreeSet::new();
    if let Some(project) = project {
        for document in project.documents() {
            if let Some(sheet) = project.schematic_sheet(document.uuid) {
                used.extend(sheet.net_labels().map(|l| l.name.clone()));
                used.extend(sheet.ports().map(|p| p.name.clone()));
                used.extend(sheet.power_ports().map(|p| p.name.clone()));
            }
        }
    }
    if let Some(name) = name {
        used.insert(name.to_owned());
    }
    (1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|n| !used.contains(n))
        .expect("an unused name")
}

impl Cx<'_> {
    /// The pointer at `at` snapped onto the sheet's Hotspots, else its grid.
    pub(super) fn snap(&self, at: P) -> Point {
        let targets = self
            .sheet()
            .map(crate::snap::hotspots::sheet)
            .unwrap_or_default();
        crate::snap::snap_at(self.shell, self.document, at, &targets).at
    }

    /// The state a wiring Tool starts in with the pointer at `at`.
    pub(super) fn start_wiring(&mut self, at: P) -> Option<Interaction> {
        let tool = self.shell.tool();
        match tool {
            Tool::Wire => Some(Interaction::Wiring(Wiring {
                at: self.snap(at),
                ..Wiring::default()
            })),
            Tool::Line | Tool::Rect => Some(Interaction::Drawing(self.start_drawing(tool, at))),
            Tool::NetLabel | Tool::Port | Tool::PowerPort | Tool::NoConnect | Tool::Text => {
                Some(Interaction::Placing(Placing {
                    tool,
                    at: self.snap(at),
                    rotation: Millideg(0),
                    name: {
                        let engine = self.shell.engine();
                        let preset = engine
                            .view()
                            .get::<PowerPortName>()
                            .and_then(|n| n.0.clone());
                        next_name(engine.project(), tool, preset.as_deref())
                    },
                    shown: None,
                }))
            }
            _ => None,
        }
    }

    /// Whether a Pin or a Wire is at `p`: where a run ends.
    fn connects(&self, p: Point) -> bool {
        let Some(sheet) = self.sheet() else {
            return false;
        };
        placed_pins(sheet).iter().any(|(_, _, at)| *at == p) || sheet.wires().any(|w| w.contains(p))
    }

    /// The Wire Tool on one pointer event.
    pub(super) fn wire_step(
        &mut self,
        mut w: Wiring,
        event: crate::canvas::interact::Pointer,
    ) -> Wiring {
        use crate::canvas::interact::Pointer;
        match event {
            Pointer::Hover(at) | Pointer::Drag(at) => w.at = self.snap(at),
            Pointer::Click { at, .. } | Pointer::DragEnd(at) => {
                w.at = self.snap(at);
                w = self.fix(w);
            }
            Pointer::DragStart { from, at, .. } => {
                w.at = self.snap(from);
                w = self.fix(w);
                w.at = self.snap(at);
            }
        }
        w
    }

    /// Fixes the run's next corner where the pointer snapped: starts a run, extends it, or
    /// ends it on a Pin or Wire (or on the same point twice). In 90° and 45° the segment on
    /// the pointer stays a look-ahead: the click fixes the run up to its corner.
    pub(super) fn fix(&mut self, mut w: Wiring) -> Wiring {
        let p = w.at;
        let Some(&last) = w.points.last() else {
            w.points.push(p);
            w.committed.clear();
            w.routed.clear();
            w.last_click = Some(p);
            return w;
        };
        if w.last_click == Some(p) || last == p {
            w.points = w.path_to(p);
            if w.points.len() >= 2 {
                self.commit_run(&mut w);
            }
            return w;
        }
        let lands = self.connects(p);
        if w.mode == WireMode::Auto {
            self.route(&mut w, last, p);
        } else if lands {
            w.points = w.path_to(p);
        } else {
            let corner = w.elbow(last, p).unwrap_or(p);
            push(&mut w.points, corner);
        }
        w.last_click = Some(p);
        if lands && w.points.last() == Some(&p) && w.points.len() >= 2 {
            self.commit_run(&mut w);
        }
        w
    }

    /// Auto Wire: asks `schematic.wire.route` for a way from `from` to `to` and adds it to
    /// the run. A route not found leaves the run as it was, and the status bar says why.
    fn route(&mut self, w: &mut Wiring, from: Point, to: Point) {
        let options = self
            .shell
            .engine()
            .view()
            .get::<crate::tools::WireTool>()
            .copied()
            .unwrap_or_default();
        let grid = self.shell.grid(self.document);
        let params = json!({
            "document": self.document,
            "from": {"x": from.x.0, "y": from.y.0},
            "to": {"x": to.x.0, "y": to.y.0},
            "grid": grid.0,
            "avoid_cutting_wires": options.avoid,
            "timeout_ms": options.timeout_ms,
        });
        let Ok(out) = self.shell.run_reporting("schematic.wire.route", params) else {
            return;
        };
        let result = out.get("result").unwrap_or(&out);
        let route: Vec<Point> =
            serde_json::from_value(result["points"].clone()).unwrap_or_default();
        let start = w.points.len() - 1;
        for &q in route.iter().skip(1) {
            push(&mut w.points, q);
        }
        let start = start.min(w.points.len().saturating_sub(1));
        let added: Vec<_> = w.points[start..].windows(2).map(|s| (s[0], s[1])).collect();
        w.routed.extend(added);
    }

    /// Insert: adds a vertex where the pointer snapped, with the corner on the way there,
    /// or starts a run there.
    pub(super) fn insert_vertex(&mut self, mut w: Wiring) -> Wiring {
        if w.points.is_empty() {
            return self.fix(w);
        }
        w.points = w.path_to(w.at);
        w.last_click = None;
        w
    }

    /// Esc during a run: commits its fixed corners, drops the look-ahead, and stays in
    /// the Tool.
    pub(super) fn end_run(&mut self, mut w: Wiring) -> Wiring {
        if w.points.len() >= 2 {
            self.commit_run(&mut w);
        } else {
            w.points.clear();
            w.routed.clear();
            w.last_click = None;
        }
        w
    }

    /// Queues the run's one `schematic.wire.add` and starts afresh.
    fn commit_run(&mut self, w: &mut Wiring) {
        let points: Vec<Value> = w
            .points
            .iter()
            .map(|p| json!({"x": p.x.0, "y": p.y.0}))
            .collect();
        self.commands.push((
            "schematic.wire.add".to_owned(),
            json!({"document": self.document, "points": points}),
        ));
        w.committed = w.points.windows(2).map(|s| (s[0], s[1])).collect();
        w.points.clear();
        w.routed.clear();
        w.last_click = None;
    }

    /// A placing Tool on one pointer event: follows the pointer, places on a click.
    pub(super) fn place_step(
        &mut self,
        mut p: Placing,
        event: crate::canvas::interact::Pointer,
    ) -> Placing {
        use crate::canvas::interact::Pointer;
        match event {
            Pointer::Hover(at) | Pointer::Drag(at) | Pointer::DragStart { at, .. } => {
                p.at = self.snap(at)
            }
            Pointer::Click { at, .. } | Pointer::DragEnd(at) => {
                p.at = self.snap(at);
                self.place(&mut p);
            }
        }
        p
    }

    /// Queues the Command placing `p`'s object, and names the next one.
    fn place(&mut self, p: &mut Placing) {
        let (document, at) = (self.document, json!({"x": p.at.x.0, "y": p.at.y.0}));
        let rotation = p.rotation.0;
        let (id, params) = match p.tool {
            Tool::NetLabel => (
                "schematic.net_label.add",
                json!({"document": document, "name": p.name, "at": at, "rotation": rotation}),
            ),
            Tool::Port => (
                "schematic.port.add",
                json!({"document": document, "name": p.name, "at": at, "rotation": rotation}),
            ),
            Tool::PowerPort => (
                "schematic.power_port.add",
                json!({"document": document, "name": p.name, "at": at, "rotation": rotation}),
            ),
            Tool::NoConnect => (
                "schematic.no_connect.add",
                json!({"document": document, "at": at}),
            ),
            // The words are left out: the window asks for them.
            Tool::Text => (
                "schematic.text.add",
                json!({"document": document, "at": at, "rotation": rotation}),
            ),
            _ => return,
        };
        self.commands.push((id.to_owned(), params));
        p.name = match p.tool {
            // Auto-increment steps the name just placed, taken or not.
            Tool::NetLabel | Tool::Port => reda_engine::model::auto_increment::step(
                &p.name,
                self.shell.preferences().schematic.auto_increment.primary,
            ),
            _ => next_name(self.shell.engine().project(), p.tool, Some(&p.name)),
        };
    }
}
