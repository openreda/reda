//! The PCB select-and-place Tool: what pointer presses, drags and the Space / L keys do
//! on the board.
//!
//! - A click selects the topmost object under it (`selection.set`); `Mod`+click toggles
//!   it; a click on nothing clears the selection.
//! - Dragging a Footprint, Free Pad or the Placement Origin moves it (with the rest of the
//!   selection when it is selected). The move is drawn from a preview of the board
//!   (`reda_pcb::placement_preview`) and runs nothing until release, when it becomes one
//!   Command (`pcb.move`), or one `batch` when it was also turned or flipped in flight.
//! - Dragging a Track segment moves it across itself, its neighbours stretching to stay
//!   attached (`Mod`+drag moves it freely, detached), drawn from the same plan as
//!   `pcb.track.drag` (`reda_pcb::track_drag_plan`) and run as that one Command on release.
//! - Dragging from nothing draws a rubber band that selects what lies wholly inside.
//! - Space turns by 90° and L flips to the other side: in flight they join the move,
//!   otherwise they run `pcb.rotate` / `pcb.flip` on the selection.
//! - Moves snap the grabbed point (a Pad centre near the grab, else the object's
//!   position) to Pads, Track ends and Via centres nearby, else to the snap grid.
//!
//! Points are board nm. The Tool talks to the Shell only through its public API, so the
//! canvas and the tests drive it the same way.

use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{InDocument, Object, ObjectType, Project, TrackGeometry, Uuid};
use reda_engine::pcb::{Copper, PlacementOp, placement_anchor, placement_preview, track_drag_plan};
use reda_engine::render::{Item, SceneLayer};
use serde_json::{Value, json};

use super::snap::{self, SnapKind, Target};
use crate::canvas::interact::{BoxMode, box_select_on, hit_test_on};
use crate::shell::{Shell, ToolPreview};

/// How far from the cursor electrical points still catch it when the canvas gives no
/// zoom: 0.5 mm.
pub const DEFAULT_SNAP_RADIUS: Nm = Nm::from_um(500);

/// The Tool's state between events.
#[derive(Debug, Default)]
pub struct Tool {
    state: State,
    /// Where the cursor last snapped, for the snap marker.
    snap: Option<(Point, SnapKind)>,
    /// How far electrical points catch the cursor, from the canvas zoom.
    radius: Option<Nm>,
}

#[derive(Debug, Default)]
enum State {
    #[default]
    Idle,
    /// A button is down and has not moved yet.
    Pressed {
        at: Point,
        hit: Option<Uuid>,
        toggle: bool,
    },
    Band {
        from: Point,
        to: Point,
    },
    Moving(Move),
    TrackDrag(TrackDrag),
    /// The press can do nothing more until release (a refused move, a cancel).
    Spent,
}

#[derive(Debug)]
struct Move {
    document: Uuid,
    objects: Vec<Uuid>,
    /// The grab minus the reference point that snaps.
    offset: Point,
    /// Where the reference point is after `ops`.
    base: Point,
    /// Where the reference point is now (snapped).
    current: Point,
    /// The ops so far, before the final translation to `current`.
    ops: Vec<PlacementOp>,
    /// What the reference point can snap to: everything but the moved objects.
    targets: Vec<Target>,
}

impl Move {
    /// Every op, the move to the cursor included.
    fn all_ops(&self) -> Vec<PlacementOp> {
        let mut ops = self.ops.clone();
        let by = self.current - self.base;
        if by != Point::new(Nm(0), Nm(0)) {
            ops.push(PlacementOp::Translate(by));
        }
        ops
    }

    /// Ends the move so far at the current point, so a turn or flip can follow.
    fn settle(&mut self) {
        self.ops = self.all_ops();
        self.base = self.current;
    }
}

/// A Track segment being dragged.
#[derive(Debug)]
struct TrackDrag {
    document: Uuid,
    track: Uuid,
    /// The press, on the grid.
    grab: Point,
    detach: bool,
    /// The unit normal of a straight segment, which a plain drag moves along.
    normal: Option<(f64, f64)>,
    /// The drag so far.
    by: Point,
    copper: std::sync::Arc<Copper>,
}

/// The kinds of object the Tool can move.
fn movable(kind: ObjectType) -> bool {
    matches!(
        kind,
        ObjectType::FootprintInstance | ObjectType::FreePad | ObjectType::PlacementOrigin
    )
}

/// The PCB Document and the board's project, when one is open.
fn board(shell: &Shell) -> Option<(&Project, Uuid)> {
    let project = shell.engine().project()?;
    Some((project, project.pcb()?.uuid))
}

impl Tool {
    /// Sets how far electrical points catch the cursor (a few pixels at the zoom).
    pub fn set_snap_radius(&mut self, radius: Nm) {
        self.radius = Some(radius);
    }

    fn radius(&self) -> Nm {
        self.radius.unwrap_or(DEFAULT_SNAP_RADIUS)
    }

    /// Where the cursor last snapped to, while a move is in flight or after a hover.
    pub fn snap(&self) -> Option<(Point, SnapKind)> {
        self.snap
    }

    /// The rubber band's corners while one is drawn.
    pub fn band(&self) -> Option<(Point, Point)> {
        match self.state {
            State::Band { from, to } => Some((from, to)),
            _ => None,
        }
    }

    /// Whether a move is in flight.
    pub fn moving(&self) -> bool {
        matches!(self.state, State::Moving(_))
    }

    /// The cursor is at `at` with no button down: snaps it for the marker.
    pub fn hover(&mut self, shell: &Shell, at: Point) -> (Point, SnapKind) {
        let grid = board_grid(shell);
        let targets = board(shell)
            .and_then(|(p, _)| p.pcb())
            .filter(|_| electrical_snap(shell))
            .map(snap::targets)
            .unwrap_or_default();
        let snapped = snap::snap(at, grid, &targets, self.radius());
        self.snap = Some(snapped);
        snapped
    }

    /// A button went down at `at`; `tolerance` is how near counts as on an object.
    pub fn press(&mut self, shell: &Shell, at: Point, toggle: bool, tolerance: Nm) {
        let hit = board(shell).and_then(|(project, pcb)| {
            let tab = shell.document_tab(pcb)?;
            let scene = &tab.scene;
            hit_test_on(
                &scene.scene,
                scene.layers.as_deref(),
                xy(at),
                tolerance.0 as f64,
                |i| pickable(project, i),
            )
        });
        self.state = State::Pressed { at, hit, toggle };
    }

    /// The pointer moved to `to` with the button down.
    pub fn drag(&mut self, shell: &mut Shell, to: Point) {
        match std::mem::take(&mut self.state) {
            State::Pressed { at, hit, toggle } => match hit {
                None => self.state = State::Band { from: at, to },
                Some(object) if is_track(shell, object) => {
                    self.start_track_drag(shell, at, object, toggle, to)
                }
                Some(object) => self.start_move(shell, at, object, toggle, to),
            },
            State::TrackDrag(mut d) => {
                follow(shell, &mut d, to);
                self.state = State::TrackDrag(d);
            }
            State::Band { from, .. } => self.state = State::Band { from, to },
            State::Moving(mut m) => {
                self.track(shell, &mut m, to);
                self.state = State::Moving(m);
            }
            other => self.state = other,
        }
    }

    /// The button came up.
    pub fn release(&mut self, shell: &mut Shell) {
        match std::mem::take(&mut self.state) {
            State::Pressed { hit, toggle, .. } => {
                let mut objects = shell.selection().objects.clone();
                match (hit, toggle) {
                    (Some(o), true) => {
                        if let Some(i) = objects.iter().position(|x| *x == o) {
                            objects.remove(i);
                        } else {
                            objects.push(o);
                        }
                    }
                    (Some(o), false) => objects = vec![o],
                    (None, true) => return,
                    (None, false) => objects.clear(),
                }
                select(shell, objects);
            }
            State::Band { from, to } => {
                let found = board(shell).and_then(|(project, pcb)| {
                    let tab = shell.document_tab(pcb)?;
                    let scene = &tab.scene;
                    Some(box_select_on(
                        &scene.scene,
                        scene.layers.as_deref(),
                        xy(from),
                        xy(to),
                        BoxMode::Enclose,
                        |i| pickable(project, i),
                    ))
                });
                select(shell, found.unwrap_or_default());
            }
            State::Moving(m) => {
                self.snap = None;
                let ops = m.all_ops();
                let steps: Vec<Value> = ops.iter().map(|op| step(&m.objects, *op)).collect();
                match steps.len() {
                    0 => shell.redraw(m.document),
                    1 => {
                        let one = &steps[0];
                        let id = one["id"].as_str().expect("a step id").to_owned();
                        if shell.run_reporting(&id, one["params"].clone()).is_err() {
                            shell.redraw(m.document);
                        }
                    }
                    _ => {
                        let batch = json!({ "commands": steps });
                        if shell.run_reporting("batch", batch).is_err() {
                            shell.redraw(m.document);
                        }
                    }
                }
            }
            State::TrackDrag(d) => {
                shell.end_tool();
                if d.by == Point::default() {
                    shell.redraw(d.document);
                } else {
                    let by = json!({"x": d.by.x, "y": d.by.y});
                    let params = json!({"track": d.track, "by": by, "detach": d.detach});
                    if shell.run_reporting("pcb.track.drag", params).is_err() {
                        shell.redraw(d.document);
                    }
                }
            }
            State::Idle | State::Spent => {}
        }
    }

    /// Esc: drops a move or rubber band in flight, leaving the board as it was.
    pub fn cancel(&mut self, shell: &mut Shell) {
        if let State::Moving(m) = &self.state {
            shell.redraw(m.document);
        }
        if let State::TrackDrag(d) = &self.state {
            shell.end_tool();
            shell.redraw(d.document);
        }
        if !matches!(self.state, State::Idle) {
            self.state = State::Spent;
        }
        self.snap = None;
    }

    /// Space: turns the move in flight by 90°, or else the selection (`pcb.rotate`).
    pub fn rotate(&mut self, shell: &mut Shell) {
        self.turn(shell, |at| PlacementOp::Rotate(at, Millideg::from_deg(90)));
    }

    /// L: flips the move in flight to the other side, or else the selection (`pcb.flip`).
    pub fn flip(&mut self, shell: &mut Shell) {
        self.turn(shell, |at| PlacementOp::Flip(at.x));
    }

    /// Adds `op` (made for the point it turns about) to the move in flight, or runs it on
    /// the selection about its anchor.
    fn turn(&mut self, shell: &mut Shell, op: impl Fn(Point) -> PlacementOp) {
        match &mut self.state {
            State::Moving(m) => {
                m.settle();
                m.ops.push(op(m.current));
                let ops = m.all_ops();
                let (document, objects) = (m.document, m.objects.clone());
                preview(shell, document, &objects, &ops);
            }
            _ => {
                let objects = placeable_selection(shell);
                if objects.is_empty() {
                    return;
                }
                let Some(anchor) = board(shell)
                    .and_then(|(p, _)| p.pcb())
                    .and_then(|pcb| placement_anchor(pcb, &objects).ok())
                else {
                    return;
                };
                let s = step(&objects, op(anchor));
                let id = s["id"].as_str().expect("a step id").to_owned();
                let _ = shell.run_reporting(&id, s["params"].clone());
            }
        }
    }

    fn start_move(&mut self, shell: &mut Shell, at: Point, object: Uuid, toggle: bool, to: Point) {
        let Some((project, document)) = board(shell) else {
            return;
        };
        let pcb = project.pcb().expect("a board");
        let kind = project.object(object).map(|o| o.object_type());
        let mut objects = shell.selection().objects.clone();
        if !objects.contains(&object) {
            objects = vec![object];
        }
        objects.retain(|o| project.object(*o).is_some_and(|x| movable(x.object_type())));
        if !kind.is_some_and(movable) || objects.is_empty() {
            // Dragging what cannot move just selects it.
            self.state = State::Pressed {
                at,
                hit: Some(object),
                toggle,
            };
            return;
        }
        let all = snap::targets(pcb);
        let own: Vec<Target> = all
            .iter()
            .filter(|t| objects.contains(&t.object))
            .copied()
            .collect();
        let others: Vec<Target> = all
            .into_iter()
            .filter(|t| !objects.contains(&t.object))
            .collect();
        let reference = snap::nearest(&own, at, self.radius())
            .map(|t| t.at)
            .or_else(|| match project.object(object) {
                Some(Object::FootprintInstance(f)) => Some(f.object.at),
                Some(Object::FreePad(p)) => Some(p.object.at),
                Some(Object::PlacementOrigin(o)) => Some(o.object.at),
                _ => None,
            })
            .unwrap_or(at);
        if let Err(e) = placement_preview(pcb, &objects, &[]) {
            if shell.selection().objects != objects {
                select(shell, objects);
            }
            shell.report(crate::tr!("error-command", error = e.message));
            self.state = State::Spent;
            return;
        }
        if shell.selection().objects != objects {
            select(shell, objects.clone());
        }
        let mut m = Move {
            document,
            objects,
            offset: at - reference,
            base: reference,
            current: reference,
            ops: Vec::new(),
            targets: if electrical_snap(shell) {
                others
            } else {
                Vec::new()
            },
        };
        self.track(shell, &mut m, to);
        self.state = State::Moving(m);
    }

    /// Follows the cursor at `to` with the move `m`, redrawing the preview when it changed.
    fn track(&mut self, shell: &mut Shell, m: &mut Move, to: Point) {
        let grid = board_grid(shell);
        let (current, kind) = snap::snap(to - m.offset, grid, &m.targets, self.radius());
        self.snap = Some((current, kind));
        if current != m.current {
            m.current = current;
            let ops = m.all_ops();
            preview(shell, m.document, &m.objects, &ops);
        }
    }
}

impl Tool {
    fn start_track_drag(
        &mut self,
        shell: &mut Shell,
        at: Point,
        track: Uuid,
        detach: bool,
        to: Point,
    ) {
        let Some((project, document)) = board(shell) else {
            return;
        };
        let pcb = project.pcb().expect("a board");
        let geometry = pcb.track(track).expect("a Track").geometry;
        let normal = match geometry {
            TrackGeometry::Line { from, to } => {
                let (dx, dy) = ((to.x.0 - from.x.0) as f64, (to.y.0 - from.y.0) as f64);
                let len = (dx * dx + dy * dy).sqrt();
                (len > 0.0).then(|| (-dy / len, dx / len))
            }
            TrackGeometry::Arc { .. } => None,
        };
        let Some(copper) = Copper::of(project) else {
            return;
        };
        if shell.selection().objects != [track] {
            select(shell, vec![track]);
        }
        shell.begin_tool(ToolPreview::new(crate::tr!("tool-drag-track"), [track]));
        let mut d = TrackDrag {
            document,
            track,
            grab: snap::to_grid(at, board_grid(shell)),
            detach,
            normal,
            by: Point::default(),
            copper,
        };
        follow(shell, &mut d, to);
        self.state = State::TrackDrag(d);
    }
}

/// Whether `object` is a Track of the board.
fn is_track(shell: &Shell, object: Uuid) -> bool {
    board(shell)
        .and_then(|(p, _)| p.pcb())
        .is_some_and(|pcb| pcb.track(object).is_some())
}

/// Follows the cursor at `to` with the segment drag `d`, redrawing the preview when the
/// drag changed. A plain drag of a straight segment moves it across itself only.
fn follow(shell: &mut Shell, d: &mut TrackDrag, to: Point) {
    let raw = snap::to_grid(to, board_grid(shell)) - d.grab;
    let by = match (d.detach, d.normal) {
        (false, Some((nx, ny))) => {
            let along = raw.x.0 as f64 * nx + raw.y.0 as f64 * ny;
            Point::new(
                Nm((along * nx).round() as i64),
                Nm((along * ny).round() as i64),
            )
        }
        _ => raw,
    };
    if by == d.by {
        return;
    }
    d.by = by;
    if by == Point::default() {
        shell.redraw(d.document);
        return;
    }
    let Some(project) = shell.engine().project() else {
        return;
    };
    let Some(pcb) = project.pcb() else {
        return;
    };
    let mut n = 0u128;
    let Ok(plan) = track_drag_plan(pcb, &d.copper, d.track, by, d.detach, || {
        n += 1;
        Uuid::v4_from_bits(u128::MAX - n)
    }) else {
        return;
    };
    let mut project = project.clone();
    let mut tx = project.transaction();
    let document = d.document;
    for track in plan.modified {
        if tx
            .replace(Object::Track(InDocument::new(document, track)))
            .is_err()
        {
            return;
        }
    }
    for track in plan.created {
        if tx
            .insert(Object::Track(InDocument::new(document, track)))
            .is_err()
        {
            return;
        }
    }
    for uuid in plan.deleted {
        if tx.remove(uuid).is_err() {
            return;
        }
    }
    if tx.commit().is_err() {
        return;
    }
    shell.show_preview(document, &project);
}

/// `p` as the shared canvas functions take it: nm, y up.
fn xy(p: Point) -> (f64, f64) {
    (p.x.0 as f64, p.y.0 as f64)
}

/// The board's snap grid.
fn board_grid(shell: &Shell) -> Nm {
    match board(shell) {
        Some((_, pcb)) => shell.grid(pcb),
        None => crate::grid::pcb_default(shell.board_view().units),
    }
}

/// Whether the Preference asks for electrical snap on the board.
fn electrical_snap(shell: &Shell) -> bool {
    shell.preferences().pcb.electrical_snap
}

/// Objects the Tool can pick: anything drawn that is an object of the Project, except
/// the Ratsnest (whose lines are tagged with their Net).
fn pickable(project: &Project, item: &Item) -> bool {
    item.layer != SceneLayer::Ratsnest && project.object(item.object).is_some()
}

/// The selected objects that can be placed.
fn placeable_selection(shell: &Shell) -> Vec<Uuid> {
    let Some(project) = shell.engine().project() else {
        return Vec::new();
    };
    shell
        .selection()
        .objects
        .iter()
        .copied()
        .filter(|o| project.object(*o).is_some_and(|x| movable(x.object_type())))
        .collect()
}

fn select(shell: &mut Shell, objects: Vec<Uuid>) {
    let _ = shell.run_reporting("selection.set", json!({ "objects": objects }));
}

/// Draws the board with `objects` after `ops`.
fn preview(shell: &mut Shell, document: Uuid, objects: &[Uuid], ops: &[PlacementOp]) {
    let Some(project) = shell.engine().project() else {
        return;
    };
    let Some(placed) = project
        .pcb()
        .and_then(|pcb| placement_preview(pcb, objects, ops).ok())
    else {
        return;
    };
    let mut project = project.clone();
    let mut tx = project.transaction();
    for object in placed {
        if tx.replace(object).is_err() {
            return;
        }
    }
    if tx.commit().is_err() {
        return;
    }
    shell.show_preview(document, &project);
}

/// The Command (as a `batch` step) that does `op` to `objects`.
fn step(objects: &[Uuid], op: PlacementOp) -> Value {
    let point = |p: Point| json!({"x": p.x, "y": p.y});
    match op {
        PlacementOp::Translate(by) => {
            json!({"id": "pcb.move", "params": {"objects": objects, "by": point(by)}})
        }
        PlacementOp::Rotate(center, angle) => json!({"id": "pcb.rotate", "params": {
            "objects": objects, "angle": angle.0, "center": point(center)
        }}),
        PlacementOp::Flip(x) => json!({"id": "pcb.flip", "params": {
            "objects": objects, "center": {"x": x, "y": 0}
        }}),
    }
}
