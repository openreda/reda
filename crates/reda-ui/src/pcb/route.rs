//! The routing Tool (T) and the Via Tool (V): manual single Tracks with 45° or 90°
//! corners, and Vias placed mid-route or on their own for stitching.
//!
//! - A click starts a route on the active copper Layer (else the top copper), on the Net
//!   of the copper under it, as wide as that Net's Class says. Each further click places
//!   the corner path to the cursor; `/` cycles the corner mode, `pcb.route.width` changes
//!   the width mid-route, Backspace takes back the last corner, `pcb.route.add_via` (V)
//!   drops a Via at the cursor and carries on on the other side.
//! - The live clearance check (the engine's [`Copper::clearance`] and Keepout queries)
//!   flags the path ahead and refuses to place it; nothing shoves.
//! - Reaching copper of the route's own Net ends the route, and so do Enter
//!   (`pcb.route.finish`) and a second click on the last corner. The route then commits as
//!   one `pcb.track.add`, or one `batch` when it changed Layer or width, so it is one undo
//!   step. Nothing runs before that.
//! - The route's preview depends on the copper under its corners: it registers them with
//!   [`Shell::begin_tool`], so an agent's edit to them cancels the route.
//!
//! The Via Tool previews a Via at the cursor, checked the same way, and each click runs
//! `pcb.via.add` there (taking the Net of the copper under it).

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{LayerKind, PcbDocument, Project, RuleKind, Side, TrackGeometry, Uuid};
use reda_engine::pcb::rules::net_rules;
use reda_engine::pcb::{ClearanceHit, Copper, Proposal, zone};
use serde_json::{Value, json};

use super::snap::{self, SnapKind};
use super::tool::DEFAULT_SNAP_RADIUS;
use crate::shell::{Shell, ToolPreview};
use crate::tools::Tool;
use crate::tr;

/// How a route turns the corner between its last point and the cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Corner {
    /// Straight along the longer axis, then 45° into the cursor.
    #[default]
    Straight45,
    /// 45° first, then straight into the cursor.
    Diagonal45,
    /// Horizontal, then vertical.
    Horizontal90,
    /// Vertical, then horizontal.
    Vertical90,
}

impl Corner {
    /// The mode `/` goes to next.
    pub fn next(self) -> Corner {
        match self {
            Corner::Straight45 => Corner::Diagonal45,
            Corner::Diagonal45 => Corner::Horizontal90,
            Corner::Horizontal90 => Corner::Vertical90,
            Corner::Vertical90 => Corner::Straight45,
        }
    }
}

/// The path from `from` to `to` turning `corner`'s way: `from`, the corner when there is
/// one, and `to`.
pub fn corner_path(from: Point, to: Point, corner: Corner) -> Vec<Point> {
    let (dx, dy) = (to.x.0 - from.x.0, to.y.0 - from.y.0);
    let (sx, sy) = (dx.signum(), dy.signum());
    let d = dx.abs().min(dy.abs());
    let bend = match corner {
        Corner::Straight45 if dx.abs() >= dy.abs() => Point::new(Nm(to.x.0 - sx * d), from.y),
        Corner::Straight45 => Point::new(from.x, Nm(to.y.0 - sy * d)),
        Corner::Diagonal45 => Point::new(Nm(from.x.0 + sx * d), Nm(from.y.0 + sy * d)),
        Corner::Horizontal90 => Point::new(to.x, from.y),
        Corner::Vertical90 => Point::new(from.x, to.y),
    };
    let mut path = vec![from];
    for p in [bend, to] {
        if *path.last().expect("from") != p {
            path.push(p);
        }
    }
    path
}

/// A stretch of route on one Layer at one width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leg {
    pub layer: Uuid,
    pub width: Nm,
    pub points: Vec<Point>,
}

/// What a route has placed so far, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    Leg(Leg),
    Via(Point),
}

#[derive(Clone, Debug)]
struct Route {
    net: Option<Uuid>,
    /// Earlier legs and Vias.
    steps: Vec<Step>,
    /// The leg being routed; its last point is where the path ahead starts.
    leg: Leg,
    /// Objects whose copper is under the route's start, left out of the clearance check
    /// while the route has no Net.
    start_objects: Vec<Uuid>,
    /// Objects the preview depends on, for [`ToolPreview`].
    objects: Vec<Uuid>,
}

impl Route {
    fn last(&self) -> Point {
        *self.leg.points.last().expect("a leg has its start")
    }

    fn segments(&self) -> usize {
        self.steps
            .iter()
            .map(|s| match s {
                Step::Leg(l) => l.points.len() - 1,
                Step::Via(_) => 1,
            })
            .sum::<usize>()
            + self.leg.points.len()
            - 1
    }
}

/// The route in flight, as the canvas draws it and tests read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// The Layer being routed.
    pub layer: Uuid,
    pub net: Option<Uuid>,
    /// The width being routed.
    pub width: Nm,
    /// The current leg's placed points.
    pub placed: Vec<Point>,
    /// The path ahead, from the last placed point to the cursor.
    pub ahead: Vec<Point>,
    /// Whether the path ahead breaks clearance (it then cannot be placed).
    pub blocked: bool,
    /// Earlier legs.
    pub legs: Vec<Leg>,
    /// The Vias placed.
    pub vias: Vec<Point>,
}

/// The Via the Via Tool would place at the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViaCursor {
    pub at: Point,
    pub diameter: Nm,
    pub blocked: bool,
}

/// The routing and Via Tools' state between events.
#[derive(Debug, Default)]
pub struct Router {
    route: Option<Route>,
    corner: Corner,
    /// The cursor, snapped.
    cursor: Option<(Point, SnapKind)>,
    radius: Option<Nm>,
    /// What the path ahead (or the Via at the cursor) hits.
    hits: Vec<ClearanceHit>,
    keepouts: Vec<Uuid>,
    via: Option<ViaCursor>,
}

/// The title of the route's [`ToolPreview`].
fn title() -> String {
    tr!("tool-route")
}

impl Router {
    /// Sets how far electrical points catch the cursor (a few pixels at the zoom).
    pub fn set_snap_radius(&mut self, radius: Nm) {
        self.radius = Some(radius);
    }

    /// The corner mode.
    pub fn corner(&self) -> Corner {
        self.corner
    }

    /// Where the cursor last snapped, for the snap marker.
    pub fn snap(&self) -> Option<(Point, SnapKind)> {
        self.cursor
    }

    /// The Via the Via Tool would place at the cursor.
    pub fn via_cursor(&self) -> Option<ViaCursor> {
        self.via
    }

    /// The route in flight.
    pub fn preview(&self) -> Option<Preview> {
        let route = self.route.as_ref()?;
        let ahead = match self.cursor {
            Some((at, _)) => corner_path(route.last(), at, self.corner),
            None => vec![route.last()],
        };
        Some(Preview {
            layer: route.leg.layer,
            net: route.net,
            width: route.leg.width,
            placed: route.leg.points.clone(),
            ahead,
            blocked: !self.hits.is_empty() || !self.keepouts.is_empty(),
            legs: route
                .steps
                .iter()
                .filter_map(|s| match s {
                    Step::Leg(l) => Some(l.clone()),
                    Step::Via(_) => None,
                })
                .collect(),
            vias: route
                .steps
                .iter()
                .filter_map(|s| match s {
                    Step::Via(at) => Some(*at),
                    Step::Leg(_) => None,
                })
                .collect(),
        })
    }

    /// The cursor is at `at`: snaps it and checks the path ahead (or the Via) there.
    pub fn hover(&mut self, shell: &mut Shell, at: Point) {
        self.notice_cancel(shell);
        let at = self.snap_to(shell, at);
        self.check(shell, at);
    }

    /// A click at `at`: starts a route, places a corner, or ends the route; with the Via
    /// Tool, places a Via.
    pub fn click(&mut self, shell: &mut Shell, at: Point) {
        self.notice_cancel(shell);
        let at = self.snap_to(shell, at);
        if shell.tool() == Tool::Via {
            self.check(shell, at);
            self.place_via(shell, at);
            return;
        }
        if self.route.is_none() {
            self.start(shell, at);
            return;
        }
        self.check(shell, at);
        let route = self.route.as_ref().expect("routing");
        if at == route.last() {
            if route.segments() > 0 {
                self.finish(shell);
            }
            return;
        }
        if let Some(why) = self.why_blocked(shell) {
            shell.report(why);
            return;
        }
        let ends = self.ends_here(shell, at);
        let route = self.route.as_mut().expect("routing");
        let path = corner_path(route.last(), at, self.corner);
        route.leg.points.extend_from_slice(&path[1..]);
        self.depend_on(shell, at);
        if ends {
            self.finish(shell);
        }
    }

    /// V while routing: places the path to the cursor, drops a Via there and carries on on
    /// the other side.
    pub fn add_via(&mut self, shell: &mut Shell) {
        self.notice_cancel(shell);
        let (Some(route), Some((at, _))) = (&self.route, self.cursor) else {
            return;
        };
        let net = route.net;
        self.check(shell, at);
        if let Some(why) = self.why_blocked(shell) {
            shell.report(why);
            return;
        }
        let Some((project, _)) = board(shell) else {
            return;
        };
        let pcb = project.pcb().expect("a board");
        let diameter = net_rules(pcb, net).via_diameter.default_size();
        let proposal = Proposal::Via { at, diameter, net };
        if let Some(why) = self.blocked_by(shell, &proposal) {
            shell.report(why);
            return;
        }
        let other = other_side(pcb, self.route.as_ref().expect("routing").leg.layer);
        let route = self.route.as_mut().expect("routing");
        let path = corner_path(route.last(), at, self.corner);
        route.leg.points.extend_from_slice(&path[1..]);
        let next = Leg {
            layer: other,
            width: route.leg.width,
            points: vec![at],
        };
        let done = std::mem::replace(&mut route.leg, next);
        if done.points.len() > 1 {
            route.steps.push(Step::Leg(done));
        }
        route.steps.push(Step::Via(at));
        self.depend_on(shell, at);
        let _ = shell.run_reporting("pcb.layer.activate", json!({ "layer": other }));
        self.check(shell, at);
    }

    /// Backspace: takes back the last corner (or Via); with nothing left, drops the route.
    pub fn undo_segment(&mut self, shell: &mut Shell) {
        self.notice_cancel(shell);
        let Some(route) = self.route.as_mut() else {
            return;
        };
        if route.leg.points.len() > 1 {
            route.leg.points.pop();
        } else if let Some(Step::Via(_)) = route.steps.last() {
            route.steps.pop();
            let at = route.last();
            route.leg = match route.steps.pop() {
                Some(Step::Leg(leg)) => leg,
                _ => Leg {
                    layer: other_side_of(shell, route.leg.layer),
                    width: route.leg.width,
                    points: vec![at],
                },
            };
            let layer = route.leg.layer;
            let _ = shell.run_reporting("pcb.layer.activate", json!({ "layer": layer }));
        } else if route.steps.is_empty() {
            self.cancel(shell);
            return;
        } else if let Some(Step::Leg(leg)) = route.steps.pop() {
            // A width change left a one-point leg: go back into the earlier one.
            route.leg = leg;
            route.leg.points.pop();
        }
        if let Some((at, _)) = self.cursor {
            self.check(shell, at);
        }
    }

    /// `/`: the next corner mode.
    pub fn cycle_corner(&mut self, shell: &mut Shell) {
        self.corner = self.corner.next();
        if let Some((at, _)) = self.cursor {
            self.check(shell, at);
        }
    }

    /// Changes the width from here on: `width`, or with `step` the board's next or
    /// previous Track width (its rule and Net Classes').
    pub fn set_width(&mut self, shell: &mut Shell, width: Option<Nm>, step: Option<i32>) {
        self.notice_cancel(shell);
        let Some(current) = self.route.as_ref().map(|r| r.leg.width) else {
            return;
        };
        let width = match (width, step) {
            (Some(w), _) => w,
            (None, Some(step)) => {
                let Some((project, _)) = board(shell) else {
                    return;
                };
                let widths = widths(project.pcb().expect("a board"));
                let found = if step > 0 {
                    widths.iter().copied().find(|w| *w > current)
                } else {
                    widths.iter().rev().copied().find(|w| *w < current)
                };
                found.unwrap_or(current)
            }
            (None, None) => return,
        };
        let route = self.route.as_mut().expect("routing");
        if width == route.leg.width {
            return;
        }
        if route.leg.points.len() > 1 {
            let next = Leg {
                layer: route.leg.layer,
                width,
                points: vec![route.last()],
            };
            let done = std::mem::replace(&mut route.leg, next);
            route.steps.push(Step::Leg(done));
        } else {
            route.leg.width = width;
        }
        if let Some((at, _)) = self.cursor {
            self.check(shell, at);
        }
    }

    /// Enter: commits what is placed (the path ahead is dropped).
    pub fn finish(&mut self, shell: &mut Shell) {
        self.notice_cancel(shell);
        let Some(route) = self.route.take() else {
            return;
        };
        shell.end_tool();
        self.hits.clear();
        self.keepouts.clear();
        let mut steps: Vec<Value> = Vec::new();
        let net = route.net;
        let mut parts = route.steps;
        parts.push(Step::Leg(route.leg));
        // A Via at the end of the route with nothing after it still counts.
        for part in parts {
            match part {
                Step::Leg(leg) if leg.points.len() > 1 => {
                    let points: Vec<Value> = leg
                        .points
                        .iter()
                        .map(|p| json!({"x": p.x, "y": p.y}))
                        .collect();
                    let mut params = json!({
                        "layer": leg.layer, "points": points, "width": leg.width.0
                    });
                    if let Some(net) = net {
                        params["net"] = json!(net);
                    }
                    steps.push(json!({"id": "pcb.track.add", "params": params}));
                }
                Step::Leg(_) => {}
                Step::Via(at) => {
                    let mut params = json!({"at": {"x": at.x, "y": at.y}});
                    if let Some(net) = net {
                        params["net"] = json!(net);
                    }
                    steps.push(json!({"id": "pcb.via.add", "params": params}));
                }
            }
        }
        match steps.len() {
            0 => {}
            1 => {
                let id = steps[0]["id"].as_str().expect("a step id").to_owned();
                let _ = shell.run_reporting(&id, steps[0]["params"].clone());
            }
            _ => {
                let _ = shell.run_reporting("batch", json!({ "commands": steps }));
            }
        }
    }

    /// Esc, or another Tool: drops the route (or the Via at the cursor) unplaced.
    pub fn cancel(&mut self, shell: &mut Shell) {
        if self.route.take().is_some() && shell.tool_preview().is_some_and(|t| t.title == title()) {
            shell.end_tool();
        }
        self.hits.clear();
        self.keepouts.clear();
        self.via = None;
    }

    /// Drops the route when an agent's edit cancelled its preview.
    fn notice_cancel(&mut self, shell: &Shell) {
        if self.route.is_some() && shell.tool_preview().is_none_or(|t| t.title != title()) {
            self.route = None;
            self.hits.clear();
            self.keepouts.clear();
        }
    }

    fn snap_to(&mut self, shell: &Shell, at: Point) -> Point {
        let grid = match board(shell) {
            Some((_, pcb)) => shell.grid(pcb),
            None => crate::grid::pcb_default(shell.board_view().units),
        };
        let targets = board(shell)
            .and_then(|(p, _)| p.pcb())
            .filter(|_| shell.preferences().pcb.electrical_snap)
            .map(snap::targets)
            .unwrap_or_default();
        let snapped = snap::snap(
            at,
            grid,
            &targets,
            self.radius.unwrap_or(DEFAULT_SNAP_RADIUS),
        );
        self.cursor = Some(snapped);
        snapped.0
    }

    /// The board's copper index, kept in the Project between edits.
    fn copper(&self, shell: &Shell) -> Option<std::sync::Arc<Copper>> {
        let (project, _) = board(shell)?;
        Copper::of(project)
    }

    /// Checks the path ahead to `at` (or, with the Via Tool, a Via at `at`).
    fn check(&mut self, shell: &Shell, at: Point) {
        self.hits.clear();
        self.keepouts.clear();
        if shell.tool() == Tool::Via && self.route.is_none() {
            let Some((project, _)) = board(shell) else {
                return;
            };
            let pcb = project.pcb().expect("a board");
            let net = self
                .copper(shell)
                .and_then(|c| pcb.copper_layers().find_map(|l| c.net_at(l.uuid, at)));
            let diameter = net_rules(pcb, net).via_diameter.default_size();
            let proposal = Proposal::Via { at, diameter, net };
            let (hits, keepouts) = self.query(shell, &proposal, &[]);
            self.via = Some(ViaCursor {
                at,
                diameter,
                blocked: !hits.is_empty() || !keepouts.is_empty(),
            });
            self.hits = hits;
            self.keepouts = keepouts;
            return;
        }
        self.via = None;
        let Some(route) = &self.route else {
            return;
        };
        let path = corner_path(route.last(), at, self.corner);
        let (layer, width, net) = (route.leg.layer, route.leg.width, route.net);
        let ignore = if net.is_none() {
            route.start_objects.clone()
        } else {
            Vec::new()
        };
        for pair in path.windows(2) {
            let proposal = Proposal::Track {
                layer,
                geometry: TrackGeometry::Line {
                    from: pair[0],
                    to: pair[1],
                },
                width,
                net,
            };
            let (hits, keepouts) = self.query(shell, &proposal, &ignore);
            self.hits.extend(hits);
            self.keepouts.extend(keepouts);
        }
    }

    fn query(
        &mut self,
        shell: &Shell,
        proposal: &Proposal,
        ignore: &[Uuid],
    ) -> (Vec<ClearanceHit>, Vec<Uuid>) {
        let Some(pcb) = board(shell).and_then(|(p, _)| p.pcb()) else {
            return (Vec::new(), Vec::new());
        };
        let keepouts = zone::keepouts_hit(pcb, proposal);
        let hits = self
            .copper(shell)
            .map(|c| c.clearance(pcb, proposal, ignore))
            .unwrap_or_default();
        (hits, keepouts)
    }

    /// Why the last check refused, for the status bar.
    fn why_blocked(&self, shell: &Shell) -> Option<String> {
        if let Some(hit) = self.hits.first() {
            let units = shell.board_view().units;
            return Some(tr!(
                "route-blocked",
                actual = super::view::format_length(hit.actual, units),
                required = super::view::format_length(hit.required, units)
            ));
        }
        if !self.keepouts.is_empty() {
            return Some(tr!("route-keepout"));
        }
        None
    }

    fn blocked_by(&mut self, shell: &Shell, proposal: &Proposal) -> Option<String> {
        let (hits, keepouts) = self.query(shell, proposal, &[]);
        self.hits = hits;
        self.keepouts = keepouts;
        self.why_blocked(shell)
    }

    /// Starts a route at `at` on the active copper Layer.
    fn start(&mut self, shell: &mut Shell, at: Point) {
        let Some((project, _)) = board(shell) else {
            return;
        };
        let pcb = project.pcb().expect("a board");
        let active = shell.board_view().active_layer(project).filter(|l| {
            pcb.layers()
                .layer(*l)
                .is_some_and(|x| x.kind == LayerKind::Copper)
        });
        let Some(layer) = active.or_else(|| {
            pcb.layers()
                .find(LayerKind::Copper, Some(Side::Top))
                .map(|l| l.uuid)
        }) else {
            return;
        };
        let Some(copper) = self.copper(shell) else {
            return;
        };
        let items = copper.items_at(layer, at);
        let net = copper.net_at(layer, at);
        let start_objects: Vec<Uuid> = items.iter().map(|i| i.object()).collect();
        let project = shell.engine().project().expect("a Project");
        let width = net_rules(project.pcb().expect("a board"), net)
            .track_width
            .default_size();
        self.route = Some(Route {
            net,
            steps: Vec::new(),
            leg: Leg {
                layer,
                width,
                points: vec![at],
            },
            objects: start_objects.clone(),
            start_objects,
        });
        shell.begin_tool(ToolPreview::new(title(), self.objects()));
        self.check(shell, at);
    }

    fn objects(&self) -> Vec<Uuid> {
        self.route
            .as_ref()
            .map(|r| r.objects.clone())
            .unwrap_or_default()
    }

    /// Adds the copper under `at` to what the preview depends on.
    fn depend_on(&mut self, shell: &mut Shell, at: Point) {
        let Some(layer) = self.route.as_ref().map(|r| r.leg.layer) else {
            return;
        };
        let found: Vec<Uuid> = self
            .copper(shell)
            .map(|c| c.items_at(layer, at).iter().map(|i| i.object()).collect())
            .unwrap_or_default();
        let route = self.route.as_mut().expect("routing");
        for o in found {
            if !route.objects.contains(&o) {
                route.objects.push(o);
            }
        }
        shell.begin_tool(ToolPreview::new(title(), self.objects()));
    }

    /// Whether `at` is on copper of the route's own Net (other than where it started), so
    /// placing the path there ends the route.
    fn ends_here(&mut self, shell: &Shell, at: Point) -> bool {
        let Some((layer, net)) = self.route.as_ref().map(|r| (r.leg.layer, r.net)) else {
            return false;
        };
        let Some(net) = net else {
            return false;
        };
        let Some(copper) = self.copper(shell) else {
            return false;
        };
        copper
            .items_at(layer, at)
            .into_iter()
            .any(|i| copper.net(i) == Some(net))
    }

    /// The Via Tool's click: a stitching Via at `at` unless it breaks clearance.
    fn place_via(&mut self, shell: &mut Shell, at: Point) {
        if let Some(why) = self.why_blocked(shell) {
            shell.report(why);
            return;
        }
        let _ = shell.run_reporting("pcb.via.add", json!({"at": {"x": at.x, "y": at.y}}));
        self.check(shell, at);
    }
}

/// The board's Track widths to step through: the preferred width of every `track_width`
/// record, the board-wide one's and every enabled Net Class's, sorted.
fn widths(pcb: &PcbDocument) -> Vec<Nm> {
    let mut out: Vec<Nm> = pcb
        .rules()
        .filter(|r| r.kind == RuleKind::TrackWidth && (r.enabled || r.scope.is_board_wide()))
        .filter_map(|r| r.values.preferred)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The copper Layer a Via switches to from `layer`: the other outer copper from an outer
/// Layer, and the next copper Layer down from inner copper. It walks the copper Layers, so
/// inner copper is never skipped as a route's next Layer.
fn other_side(pcb: &PcbDocument, layer: Uuid) -> Uuid {
    let copper: Vec<Uuid> = pcb.copper_layers().map(|l| l.uuid).collect();
    let (Some(&first), Some(&last)) = (copper.first(), copper.last()) else {
        return layer;
    };
    match copper.iter().position(|l| *l == layer) {
        Some(0) => last,
        Some(i) if i + 1 == copper.len() => first,
        Some(i) => copper[i + 1],
        None => first,
    }
}

fn other_side_of(shell: &Shell, layer: Uuid) -> Uuid {
    board(shell)
        .and_then(|(p, _)| p.pcb())
        .map_or(layer, |pcb| other_side(pcb, layer))
}

/// The open Project and its PCB Document's UUID.
fn board(shell: &Shell) -> Option<(&Project, Uuid)> {
    let project = shell.engine().project()?;
    Some((project, project.pcb()?.uuid))
}

/// Draws the route in flight (placed legs, the path ahead in red while it breaks
/// clearance, its Vias) or the Via Tool's Via at the cursor, over the board canvas.
pub fn draw(ui: &egui::Ui, shell: &Shell, input: &crate::canvas::CanvasInputs) {
    use egui::{Color32, Stroke};
    let router = shell.router();
    let painter = ui.painter_at(input.rect);
    let accent = ui.visuals().selection.stroke.color;
    let refused = ui.visuals().error_fg_color;
    let screen = |p: Point| input.screen_point((p.x.0 as f64, p.y.0 as f64));
    let points = |n: Nm| (n.0 as f64 / input.camera.nm_per_point).max(1.0) as f32;
    let path = |pts: &[Point], width: Nm, color: Color32| {
        for pair in pts.windows(2) {
            painter.line_segment(
                [screen(pair[0]), screen(pair[1])],
                Stroke::new(points(width), color),
            );
        }
    };
    let via = |at: Point, diameter: Nm, color: Color32| {
        painter.circle_stroke(screen(at), points(diameter) / 2.0, Stroke::new(2.0, color));
    };
    if let Some(p) = router.preview() {
        let placed = accent.gamma_multiply(0.8);
        for leg in &p.legs {
            path(&leg.points, leg.width, placed);
        }
        path(&p.placed, p.width, placed);
        let ahead = if p.blocked { refused } else { accent };
        path(&p.ahead, p.width, ahead.gamma_multiply(0.6));
        let diameter = board(shell)
            .and_then(|(project, _)| project.pcb())
            .map_or(Nm::from_um(600), |pcb| {
                net_rules(pcb, p.net).via_diameter.default_size()
            });
        for at in p.vias {
            via(at, diameter, placed);
        }
    } else if shell.tool() == Tool::Via
        && let Some(v) = router.via_cursor()
    {
        via(v.at, v.diameter, if v.blocked { refused } else { accent });
    }
}
