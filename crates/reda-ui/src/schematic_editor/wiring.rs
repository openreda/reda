//! The wiring Tools on a sheet: the Wire Tool's orthogonal runs, and the Net Label, Port,
//! Power Port and No-Connect Tools, which place one object per click.
//!
//! A run in flight touches neither the Project nor the Journal: each click fixes a corner,
//! and the run ends, as ONE `schematic.wire.add`, when a click lands on a Pin or a Wire (or
//! on the run's last corner again, as a double-click does). `/` turns the corner the other
//! way and Backspace takes the last segment off (`tool.cycle_corner`, `tool.undo_segment`).

use reda_engine::model::geom::Nm;
use reda_engine::model::geom::{Millideg, Point};
use reda_engine::model::{
    InDocument, Justify, NetLabel, NoConnect, Object, Port, PortDirection, PowerPort, Project,
    SheetText, Uuid,
};
use reda_engine::schematic::connectivity::placed_pins;
use reda_engine::schematic::default_power_port_style;
use serde_json::{Value, json};

use super::{Cx, Interaction, P, snap_targets};
use crate::canvas::interact::SnapTargets;
use crate::tools::{PowerPortName, Tool};

/// Which way a Wire run turns its corner on the way to the pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Corner {
    /// Along x first, then along y.
    #[default]
    HorizontalFirst,
    /// Along y first, then along x.
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

    /// The run's corners when it goes on to `to`: straight stretches merged, no empty
    /// segments.
    fn path_to(&self, to: Point) -> Vec<Point> {
        let mut path = self.points.clone();
        let Some(&last) = path.last() else {
            return vec![to];
        };
        if last.x != to.x && last.y != to.y {
            let elbow = match self.corner {
                Corner::HorizontalFirst => Point::new(to.x, last.y),
                Corner::VerticalFirst => Point::new(last.x, to.y),
            };
            push(&mut path, elbow);
        }
        push(&mut path, to);
        path
    }
}

/// Appends `p` to the run `path`, skipping an empty segment and merging a segment that
/// goes straight on from the one before.
fn push(path: &mut Vec<Point>, p: Point) {
    if path.last() == Some(&p) {
        return;
    }
    if let [.., a, b] = path[..] {
        let straight_on =
            (a.x == b.x && b.x == p.x && (b.y - a.y).0.signum() == (p.y - b.y).0.signum())
                || (a.y == b.y && b.y == p.y && (b.x - a.x).0.signum() == (p.x - b.x).0.signum());
        if straight_on {
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
    /// Everything on the sheet the pointer snaps onto.
    fn all_targets(&self) -> SnapTargets {
        self.sheet()
            .map(|s| snap_targets(s, &Default::default()))
            .unwrap_or_default()
    }

    pub(super) fn snap(&self, at: P) -> Point {
        self.all_targets().snap(at, self.grid(), self.scale.snap).at
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
    /// ends it on a Pin or Wire (or on its own last corner).
    fn fix(&mut self, mut w: Wiring) -> Wiring {
        let p = w.at;
        if w.points.is_empty() {
            w.points.push(p);
            w.committed.clear();
            return w;
        }
        let again = w.points.last() == Some(&p);
        w.points = w.path_to(p);
        if (again || self.connects(p)) && w.points.len() >= 2 {
            self.commit_run(&mut w);
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
        p.name = next_name(self.shell.engine().project(), p.tool, Some(&p.name));
    }
}
