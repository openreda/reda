//! Dragging routed copper on the board (docs/spec/v0.1/pcb.md#dragging-tracks): the
//! gestures of the select Tool over the drag Commands of `reda_pcb`.
//!
//! - A plain drag on a segment slides it (`pcb.track.drag`); in the `stop` obstacle mode
//!   (`pcb.route.obstacle_mode`, shared with routing) it stops at the last clear position.
//! - A drag that starts on a Track's end moves that end or corner (`pcb.track.drag_end`);
//!   `Space` picks Deform or Scale.
//! - A drag on a selected segment's midpoint handle, or a `Mod`+Shift press anywhere on a
//!   segment, breaks it there and drags the new vertex (`pcb.track.break`).
//! - A drag on a Via, or on a Free Pad that is not part of a larger selection, keeps
//!   every Track attached (`pcb.copper.drag`).
//! - `Mod`+drag detaches; `pcb.drag.preserve_angle` turns angle keeping off. While a drag
//!   is in flight the keys of the `tool:drag` context apply: `C` Include Miters, `R` Merge
//!   Collinear, `Space` the corner mode, Shift+R the obstacle mode (`pcb.drag.mode`).
//!
//! The preview is drawn from the same plan the Command runs, and nothing commits until
//! release, which runs one Command.

use std::sync::Arc;

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Preset, Registry, Unavailable,
};
use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{InDocument, Object, PcbDocument, TrackGeometry, Uuid};
use reda_engine::pcb::drag::{self, Corner, Plan, TrackEnd};
use reda_engine::pcb::route::ObstacleMode;
use reda_engine::pcb::{Copper, Item};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::shell::{Shell, ToolPreview};
use crate::snap::{self, Target};

/// What part of the copper the press took hold of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    /// The middle of a segment: slides it.
    Slide,
    /// One end of a segment: moves that end, or the corner it shares.
    End(TrackEnd),
    /// A new vertex at this point on the segment.
    Break(Point),
    /// A Via or Free Pad, with its Tracks.
    Object,
}

/// A copper drag in flight.
#[derive(Debug)]
pub struct CopperDrag {
    pub document: Uuid,
    /// The Track segment, Via or Free Pad grabbed.
    pub target: Uuid,
    pub grab: Grab,
    /// The press.
    press: Point,
    /// The point that snaps: the end, new vertex or object; for a slide the press on the
    /// grid.
    anchor: Point,
    pub detach: bool,
    pub preserve_angle: bool,
    pub miter: bool,
    pub merge_collinear: bool,
    pub corner: Corner,
    /// The unit normal of a straight slid segment.
    normal: Option<(f64, f64)>,
    /// How far the copper moves now.
    pub by: Point,
    /// What the moving point can snap to.
    targets: Vec<Target>,
    copper: Arc<Copper>,
}

/// The gesture a press at `at` on `hit` (with `modifiers`) starts, if it is copper the
/// drag gestures handle. A Free Pad in a larger selection moves with it instead.
pub fn grab_at(
    shell: &Shell,
    pcb: &PcbDocument,
    hit: Uuid,
    at: Point,
    modifiers: egui::Modifiers,
    tolerance: Nm,
) -> Option<Grab> {
    if pcb.via(hit).is_some() {
        return Some(Grab::Object);
    }
    if pcb.free_pad(hit).is_some() {
        let selection = &shell.selection().objects;
        let alone = !selection.contains(&hit) || selection.len() == 1;
        return alone.then_some(Grab::Object);
    }
    let track = pcb.track(hit)?;
    let (from, to) = track.geometry.ends();
    let straight = matches!(track.geometry, TrackGeometry::Line { .. });
    if modifiers.command && modifiers.shift {
        return straight.then(|| Grab::Break(on_segment(from, to, at)));
    }
    let near = |p: Point| distance(p, at) <= tolerance.0 as f64;
    if straight && shell.selection().objects.contains(&hit) && near(midpoint(from, to)) {
        return Some(Grab::Break(midpoint(from, to)));
    }
    if near(from) {
        return Some(Grab::End(TrackEnd::From));
    }
    if near(to) {
        return Some(Grab::End(TrackEnd::To));
    }
    Some(Grab::Slide)
}

/// The point on the segment `a`–`b` nearest `p`.
pub fn on_segment(a: Point, b: Point, p: Point) -> Point {
    let (ux, uy) = ((b.x.0 - a.x.0) as f64, (b.y.0 - a.y.0) as f64);
    let len2 = ux * ux + uy * uy;
    if len2 == 0.0 {
        return a;
    }
    let s = (((p.x.0 - a.x.0) as f64 * ux + (p.y.0 - a.y.0) as f64 * uy) / len2).clamp(0.0, 1.0);
    Point::new(
        Nm(a.x.0 + (ux * s).round() as i64),
        Nm(a.y.0 + (uy * s).round() as i64),
    )
}

fn midpoint(a: Point, b: Point) -> Point {
    Point::new(Nm((a.x.0 + b.x.0) / 2), Nm((a.y.0 + b.y.0) / 2))
}

fn distance(a: Point, b: Point) -> f64 {
    ((a.x.0 - b.x.0) as f64).hypot((a.y.0 - b.y.0) as f64)
}

impl CopperDrag {
    /// Starts the drag `grab` of `target` pressed at `at` (`detach` for `Mod`), selecting
    /// the target, then follows the pointer to `to`.
    pub fn start(
        shell: &mut Shell,
        document: Uuid,
        target: Uuid,
        grab: Grab,
        at: Point,
        detach: bool,
    ) -> Option<CopperDrag> {
        let project = shell.engine().project()?;
        let pcb = project.pcb()?;
        let copper = Copper::of(project)?;
        let (anchor, normal) = match grab {
            Grab::Slide => {
                let geometry = pcb.track(target)?.geometry;
                let normal = match geometry {
                    TrackGeometry::Line { from, to } => {
                        let (dx, dy) = ((to.x.0 - from.x.0) as f64, (to.y.0 - from.y.0) as f64);
                        let len = dx.hypot(dy);
                        (len > 0.0).then(|| (-dy / len, dx / len))
                    }
                    TrackGeometry::Arc { .. } => None,
                };
                (super::tool::board_to_grid(shell, at), normal)
            }
            Grab::End(end) => {
                let (from, to) = pcb.track(target)?.geometry.ends();
                (if end == TrackEnd::From { from } else { to }, None)
            }
            Grab::Break(vertex) => (vertex, None),
            Grab::Object => (
                pcb.via(target)
                    .map(|v| v.at)
                    .or_else(|| pcb.free_pad(target).map(|p| p.at))?,
                None,
            ),
        };
        let moving: Vec<Uuid> = std::iter::once(target)
            .chain(attached(pcb, &copper, target, grab))
            .collect();
        let targets = snap::hotspots::without(super::tool::board_hotspots(shell), &moving);
        if shell.selection().objects != [target] {
            let (id, params) =
                crate::selection::picking::set_command(&crate::selection::Selection {
                    objects: vec![target],
                    ..Default::default()
                });
            let _ = shell.run_reporting(&id, params);
        }
        shell.begin_tool(ToolPreview::new(crate::tr!("tool-drag-track"), [target]));
        Some(CopperDrag {
            document,
            target,
            grab,
            press: at,
            anchor,
            detach,
            preserve_angle: shell.preferences().pcb.drag.preserve_angle,
            miter: true,
            merge_collinear: true,
            corner: Corner::default(),
            normal,
            by: Point::default(),
            targets,
            copper,
        })
    }

    /// Follows the pointer at `to`, redrawing the preview when the drag changed.
    pub fn follow(&mut self, shell: &mut Shell, to: Point) {
        let by = self.wanted(shell, to);
        if by != self.by {
            self.by = by;
            self.show(shell);
        }
    }

    /// Where the pointer at `to` takes the copper: snapped, across a slid segment, and
    /// in `stop` no further than the last clear position.
    fn wanted(&self, shell: &Shell, to: Point) -> Point {
        let raw = to - self.press;
        match self.grab {
            Grab::Slide => {
                let raw = super::tool::board_to_grid(shell, to) - self.anchor;
                let by = match (self.detach || !self.preserve_angle, self.normal) {
                    (false, Some((nx, ny))) => {
                        let along = raw.x.0 as f64 * nx + raw.y.0 as f64 * ny;
                        Point::new(
                            Nm((along * nx).round() as i64),
                            Nm((along * ny).round() as i64),
                        )
                    }
                    _ => raw,
                };
                if self.obstacle_mode(shell) != ObstacleMode::Stop {
                    return by;
                }
                let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
                    return by;
                };
                drag::last_clear(pcb, &self.copper, by, |b| self.plan(pcb, b).ok())
            }
            _ => super::tool::snap_board(shell, self.anchor + raw, &self.targets).at - self.anchor,
        }
    }

    fn obstacle_mode(&self, shell: &Shell) -> ObstacleMode {
        shell.preferences().pcb.route.obstacle_mode
    }

    fn options(&self) -> drag::Options {
        drag::Options {
            detach: self.detach,
            preserve_angle: self.preserve_angle,
            miter: self.miter,
            merge_collinear: self.merge_collinear,
        }
    }

    /// The plan for moving by `by`, with placeholder UUIDs for new segments.
    fn plan(&self, pcb: &PcbDocument, by: Point) -> Result<Plan, String> {
        let mut n = 0u128;
        let new_id = || {
            n += 1;
            Uuid::v4_from_bits(u128::MAX - n)
        };
        let options = self.options();
        match self.grab {
            Grab::Slide => drag::segment(pcb, &self.copper, self.target, by, options, new_id),
            Grab::End(end) => drag::end(
                pcb,
                &self.copper,
                self.target,
                end,
                by,
                self.corner,
                options,
                new_id,
            ),
            Grab::Break(at) => drag::split(pcb, &self.copper, self.target, at, by, options, new_id),
            Grab::Object => drag::object(pcb, &self.copper, self.target, by, options, new_id),
        }
    }

    /// Draws the board as the drag would leave it.
    fn show(&self, shell: &mut Shell) {
        if self.by == Point::default() && !matches!(self.grab, Grab::Break(_)) {
            shell.redraw(self.document);
            return;
        }
        let Some(project) = shell.engine().project() else {
            return;
        };
        let Some(pcb) = project.pcb() else {
            return;
        };
        let Ok(plan) = self.plan(pcb, self.by) else {
            return;
        };
        let mut project = project.clone();
        let document = self.document;
        let mut tx = project.transaction();
        let objects = plan
            .modified
            .into_iter()
            .map(|t| (false, Object::Track(InDocument::new(document, t))))
            .chain(
                plan.created
                    .into_iter()
                    .map(|t| (true, Object::Track(InDocument::new(document, t)))),
            )
            .chain(
                plan.vias
                    .into_iter()
                    .map(|v| (false, Object::Via(InDocument::new(document, v)))),
            )
            .chain(
                plan.free_pads
                    .into_iter()
                    .map(|p| (false, Object::FreePad(InDocument::new(document, p)))),
            );
        for (new, object) in objects {
            let done = if new {
                tx.insert(object)
            } else {
                tx.replace(object)
            };
            if done.is_err() {
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

    /// A key while dragging: flips Include Miters, Merge Collinear or the corner mode and
    /// redraws. (The obstacle mode is a Preference the Shell sets.)
    pub fn change_mode(&mut self, shell: &mut Shell, mode: DragMode) {
        match mode {
            DragMode::Miter => self.miter = !self.miter,
            DragMode::MergeCollinear => self.merge_collinear = !self.merge_collinear,
            DragMode::Corner => {
                self.corner = match self.corner {
                    Corner::Deform => Corner::Scale,
                    Corner::Scale => Corner::Deform,
                }
            }
            DragMode::ObstacleMode => {}
        }
        self.show(shell);
    }

    /// Redraws after the obstacle mode changed.
    pub fn refresh(&mut self, shell: &mut Shell, to: Option<Point>) {
        if let Some(to) = to {
            self.by = self.wanted(shell, to);
        }
        self.show(shell);
    }

    /// The one Command the drag runs on release; `None` when it went nowhere.
    pub fn command(&self, shell: &Shell) -> Option<(&'static str, Value)> {
        let by = json!({"x": self.by.x, "y": self.by.y});
        let moved = self.by != Point::default();
        Some(match self.grab {
            Grab::Slide if moved => (
                "pcb.track.drag",
                json!({"track": self.target, "by": by, "detach": self.detach,
                       "preserve_angle": self.preserve_angle, "miter": self.miter,
                       "merge_collinear": self.merge_collinear,
                       "obstacle_mode": self.obstacle_mode(shell)}),
            ),
            Grab::End(end) if moved => (
                "pcb.track.drag_end",
                json!({"track": self.target, "end": end, "by": by, "corner": self.corner,
                       "detach": self.detach, "preserve_angle": self.preserve_angle,
                       "miter": self.miter, "merge_collinear": self.merge_collinear}),
            ),
            Grab::Object if moved => (
                "pcb.copper.drag",
                json!({"object": self.target, "by": by, "detach": self.detach,
                       "preserve_angle": self.preserve_angle, "miter": self.miter,
                       "merge_collinear": self.merge_collinear}),
            ),
            Grab::Break(at) => {
                let mut params = json!({"track": self.target, "at": {"x": at.x, "y": at.y},
                                        "merge_collinear": self.merge_collinear});
                if moved {
                    params["by"] = by;
                }
                ("pcb.track.break", params)
            }
            _ => return None,
        })
    }
}

/// The Tracks a drag of `target` carries along, whose Hotspots can't be snap targets.
fn attached(pcb: &PcbDocument, copper: &Copper, target: Uuid, grab: Grab) -> Vec<Uuid> {
    let item = match grab {
        Grab::Object if pcb.via(target).is_some() => Item::Via { uuid: target },
        Grab::Object => Item::FreePad { uuid: target },
        _ => Item::Track { uuid: target },
    };
    copper
        .contacts(item)
        .filter(|i| matches!(i, Item::Track { .. } | Item::Via { .. }))
        .map(|i| i.object())
        .collect()
}

/// A key in flight: the drag option it flips.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DragMode {
    /// Include Miters (`C`).
    Miter,
    /// Merge Collinear (`R`).
    MergeCollinear,
    /// Deform or Scale for a corner (`Space`).
    Corner,
    /// The obstacle mode shared with routing (Shift+R), kept in `pcb.route.obstacle_mode`.
    ObstacleMode,
}

impl DragMode {
    const ALL: [DragMode; 4] = [
        DragMode::Miter,
        DragMode::MergeCollinear,
        DragMode::Corner,
        DragMode::ObstacleMode,
    ];

    fn preset(self) -> (&'static str, &'static str, &'static str) {
        match self {
            DragMode::Miter => ("miter", "Toggle Include Miters", "drag"),
            DragMode::MergeCollinear => ("merge_collinear", "Toggle Merge Collinear", "drag"),
            DragMode::Corner => ("corner", "Deform or Scale Corner", "drag"),
            DragMode::ObstacleMode => {
                ("obstacle_mode", "Next Obstacle Mode", "route_obstacle_stop")
            }
        }
    }
}

/// Whether a copper drag is in flight: the Tool keeps it current for `pcb.drag.mode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DragFlight(pub bool);

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DragModeParams {
    pub mode: DragMode,
}

/// What was asked for.
#[derive(Serialize, JsonSchema)]
pub struct DragModeAsked {
    pub mode: DragMode,
}

struct DragModeCommand;

impl Command for DragModeCommand {
    const META: Meta = Meta {
        id: "pcb.drag.mode",
        title: "Drag Mode",
        description: "While dragging copper: toggles Include Miters or Merge Collinear, \
                      picks Deform or Scale for a corner, or steps the obstacle mode \
                      shared with routing (kept in `pcb.route.obstacle_mode`).",
        category: "tool",
        icon: "drag",
        kind: Kind::View,
    };
    type Params = DragModeParams;
    type Output = DragModeAsked;

    fn presets(&self) -> Vec<Preset> {
        DragMode::ALL
            .iter()
            .map(|m| {
                let (name, title, icon) = m.preset();
                Preset::new(name, title, icon, json!({ "mode": name }))
            })
            .collect()
    }

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            Some(p) if p.pcb().is_some() => Ok(()),
            _ => Err(Unavailable {
                reason: "no PCB is open".into(),
            }),
        }
    }

    fn run(&self, cx: &mut Context<'_>, p: DragModeParams) -> Result<DragModeAsked, CommandError> {
        if !cx.view::<DragFlight>()?.0 {
            return Err(CommandError::rejected("only while dragging copper"));
        }
        Ok(DragModeAsked { mode: p.mode })
    }
}

/// Adds `pcb.drag.mode` to `registry`.
pub fn register_all(registry: &mut Registry) {
    registry.add(DragModeCommand);
}

/// Draws the midpoint handle of each selected straight Track segment, which a drag
/// breaks the segment at.
pub fn draw(ui: &egui::Ui, shell: &Shell, input: &crate::canvas::CanvasInputs) {
    if shell.tool() != crate::tools::Tool::Select || shell.pcb_tool().copper_drag().is_some() {
        return;
    }
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        return;
    };
    let painter = ui.painter_at(input.rect);
    let accent = ui.visuals().selection.stroke.color;
    let fill = ui.visuals().extreme_bg_color;
    for track in handles(shell, pcb) {
        let pos = input.screen_point((track.x.0 as f64, track.y.0 as f64));
        painter.circle(pos, 3.5, fill, egui::Stroke::new(1.5, accent));
    }
}

/// Where the midpoint handles are: one per selected straight Track segment.
pub fn handles(shell: &Shell, pcb: &PcbDocument) -> Vec<Point> {
    shell
        .selection()
        .objects
        .iter()
        .filter_map(|o| pcb.track(*o))
        .filter_map(|t| match t.geometry {
            TrackGeometry::Line { from, to } => Some(midpoint(from, to)),
            TrackGeometry::Arc { .. } => None,
        })
        .collect()
}
