//! The Track Tool's brain: [`propose`], one pure function from the Copper index and the
//! Tool's state to what the next click does, and [`commit`], which turns a finished route
//! into the Commands of its one `batch` (loop-removal deletes, then `pcb.track.add` and
//! `pcb.via.add`).
//!
//! Neither changes anything, so both are tested with no window. The obstacle `mode` is an
//! open enum: each v0.4 mode (`walkaround`, `push`, `hug_push`) is one more arm.
//!
//! - Corners and postures are in [`path`], loop removal in [`loops`], and the routing
//!   width (its modes, pickup, clipping and the chooser's list) in [`width`].

use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, PourOver, TrackGeometry, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::copper::shape::BoardShape as _;
use crate::copper::{ClearanceHit, Copper, Item, Proposal};

pub mod loops;
pub mod path;
pub mod width;

pub use loops::{LoopOptions, Piece, Removal};
pub use path::{CornerMode, Posture, corner_path};
pub use width::{WidthChoice, WidthMode, WidthSource};

/// How the Tool treats copper in the way. Remembered in `pcb.route.obstacle_mode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObstacleMode {
    /// The route goes anywhere, drawn in the violation colour; DRC reports it.
    Ignore,
    /// The proposed path is clipped at its last clear point, which may be off grid.
    #[default]
    Stop,
    /// The route goes anywhere, and the copper, Keepouts and edges it breaks the rules
    /// with are named so the canvas can highlight them.
    Highlight,
}

impl ObstacleMode {
    /// The mode `Shift+R` goes to next.
    pub fn next(self) -> ObstacleMode {
        match self {
            ObstacleMode::Stop => ObstacleMode::Ignore,
            ObstacleMode::Ignore => ObstacleMode::Highlight,
            ObstacleMode::Highlight => ObstacleMode::Stop,
        }
    }
}

/// One end of a route: a point on a copper Layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteEnd {
    pub layer: Uuid,
    pub at: Point,
}

/// The Tool's state that [`propose`] reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The last placed vertex, where the proposed segments start.
    pub start: Point,
    pub cursor: Point,
    pub mode: ObstacleMode,
    pub corner: CornerMode,
    pub posture: Posture,
    /// Look-ahead (`1`): a click places only the first of the two segments.
    pub lookahead: bool,
    pub width: Nm,
    pub layer: Uuid,
    pub net: Option<Uuid>,
    /// Where the route began, for loop removal; `None` is `start` on `layer`.
    pub origin: Option<RouteEnd>,
    /// The narrowest width placed so far, which loop removal compares with the old path.
    pub narrowest: Option<Nm>,
    /// Auto-terminate (`Shift+A`): reaching same-Net copper ends the route.
    pub auto_terminate: bool,
    pub loops: LoopOptions,
    /// Objects left out of the clearance check, such as the copper a no-Net route
    /// started on.
    pub ignore: Vec<Uuid>,
}

impl Request {
    /// A request with the defaults: `stop`, 45°, look-ahead, auto-terminate and loop
    /// removal on.
    pub fn new(start: Point, cursor: Point, layer: Uuid, width: Nm, net: Option<Uuid>) -> Self {
        Request {
            start,
            cursor,
            mode: ObstacleMode::default(),
            corner: CornerMode::default(),
            posture: Posture::default(),
            lookahead: true,
            width,
            layer,
            net,
            origin: None,
            narrowest: None,
            auto_terminate: true,
            loops: LoopOptions::default(),
            ignore: Vec::new(),
        }
    }
}

/// How the proposed path stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// It keeps every rule.
    #[default]
    Clear,
    /// `stop` cut it short at the last clear point.
    Clipped,
    /// It breaks a rule and is placed anyway (`ignore` or `highlight`).
    Violating,
}

/// Why the Tool shows a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Note {
    /// The old path stays: the new route is narrower than its widest Track.
    LoopNarrower,
    /// The old path stays: a branch joins it; delete it by hand.
    LoopBranch,
    /// The old path stays: it goes through a Via and `remove_loops_through_vias` is off.
    LoopThroughVias,
    /// The old path stays: it has locked copper; unlock it or delete it by hand.
    LoopLocked,
    /// The cursor is in a `zones_only` Zone of the route's Net: a Track ending there isn't
    /// connected; end with a Via or Enter.
    ZoneNotConnected,
}

/// What breaks the rules on the proposed path.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Obstacles {
    /// Copper of other Nets it comes too close to.
    pub copper: Vec<ClearanceHit>,
    /// Keepouts it enters.
    pub keepouts: Vec<Uuid>,
    /// Board Outline or Board Cutouts it comes closer to than `edge_clearance`.
    pub edges: Vec<Uuid>,
}

impl Obstacles {
    pub fn is_empty(&self) -> bool {
        self.copper.is_empty() && self.keepouts.is_empty() && self.edges.is_empty()
    }

    fn extend(&mut self, other: Obstacles) {
        self.copper.extend(other.copper);
        self.keepouts.extend(other.keepouts);
        self.edges.extend(other.edges);
        self.copper.sort_by_key(|h| (h.item, h.layer));
        self.copper.dedup();
        self.keepouts.sort();
        self.keepouts.dedup();
        self.edges.sort();
        self.edges.dedup();
    }
}

/// What the next click does.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Proposed {
    /// The points the next click places, from `start` (translucent). Only `start` places
    /// nothing.
    pub place: Vec<Point>,
    /// The look-ahead tail the click doesn't place, from the end of `place` (hollow).
    pub look_ahead: Vec<Point>,
    /// From the clip point to the cursor, out of reach in `stop` (a thin ghost).
    pub blocked: Vec<Point>,
    pub status: Status,
    /// The click ends the route: it reaches same-Net copper with auto-terminate on.
    pub ends_route: bool,
    /// What loop removal deletes if the route ends here (drawn red).
    pub removes: Removal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<Note>,
    /// What the path breaks the rules with: in `stop` what clipped it, in `highlight`
    /// everything it passes; `ignore` names nothing.
    pub obstacles: Obstacles,
}

impl Proposed {
    /// Whether the next click places anything.
    pub fn places(&self) -> bool {
        self.place.len() > 1
    }
}

/// The preview for the Track Tool's next click: the corner path from `start` to the
/// cursor, checked against the clearance rules, `edge_clearance`, Keepouts and the Net
/// Tie rule (the same ones DRC uses), clipped, split for look-ahead, and with what loop
/// removal would delete if the click ends the route. Reads `copper` and changes nothing.
pub fn propose(copper: &Copper, pcb: &PcbDocument, r: &Request) -> Proposed {
    let path = corner_path(r.start, r.cursor, r.corner, r.posture);
    let mut obstacles = Obstacles::default();
    let mut reached = vec![path[0]];
    let mut blocked = Vec::new();
    let mut violating = false;
    for (i, pair) in path.windows(2).enumerate() {
        let found = check(copper, pcb, r, pair[0], pair[1]);
        if found.is_empty() {
            reached.push(pair[1]);
            continue;
        }
        if r.mode != ObstacleMode::Stop {
            violating = true;
            if r.mode == ObstacleMode::Highlight {
                obstacles.extend(found);
            }
            reached.push(pair[1]);
            continue;
        }
        let clip = last_clear(copper, pcb, r, pair[0], pair[1]);
        obstacles.extend(found);
        if clip != pair[0] {
            reached.push(clip);
        }
        blocked.push(clip);
        blocked.extend_from_slice(&path[i + 1..]);
        break;
    }
    let status = if !blocked.is_empty() {
        Status::Clipped
    } else if violating {
        Status::Violating
    } else {
        Status::Clear
    };
    let reaches = blocked.is_empty();
    let on_net = reaches && on_own_copper(copper, r.layer, r.net, r.cursor);
    let ends_route = on_net && r.auto_terminate;
    let (place, look_ahead) = if r.lookahead && !ends_route && reached.len() > 2 {
        (reached[..2].to_vec(), reached[1..].to_vec())
    } else {
        (reached, Vec::new())
    };
    let mut out = Proposed {
        place,
        look_ahead,
        blocked,
        status,
        ends_route,
        obstacles,
        ..Proposed::default()
    };
    if ends_route && let Some(net) = r.net {
        let origin = r.origin.unwrap_or(RouteEnd {
            layer: r.layer,
            at: r.start,
        });
        let end = RouteEnd {
            layer: r.layer,
            at: r.cursor,
        };
        let width = r.narrowest.map_or(r.width, |n| n.min(r.width));
        let (removes, note) = loops::plan(pcb, copper, net, origin, end, width, &r.loops);
        out.removes = removes;
        out.note = note;
    } else if reaches && in_zone_only(copper, pcb, r.layer, r.net, r.cursor) {
        out.note = Some(Note::ZoneNotConnected);
    }
    out
}

/// What a segment from `from` to `to` breaks the rules with.
fn check(copper: &Copper, pcb: &PcbDocument, r: &Request, from: Point, to: Point) -> Obstacles {
    let proposal = Proposal::Track {
        layer: r.layer,
        geometry: TrackGeometry::Line { from, to },
        width: r.width,
        net: r.net,
    };
    Obstacles {
        copper: copper.clearance(pcb, &proposal, &r.ignore),
        keepouts: crate::keepout::keepouts_hit(pcb, &proposal),
        edges: edges_hit(pcb, &proposal),
    }
}

/// The Board Outline and Board Cutouts `proposal` comes closer to than the Net's
/// `edge_clearance`, or crosses to the wrong side of.
pub fn edges_hit(pcb: &PcbDocument, proposal: &Proposal) -> Vec<Uuid> {
    let Proposal::Track {
        geometry,
        width,
        net,
        ..
    } = *proposal
    else {
        return Vec::new();
    };
    let Some(required) = crate::rules::net_rules(pcb, net).edge_clearance else {
        return Vec::new();
    };
    let shape = crate::copper::shape::Shape::track(&geometry, width);
    let (a, b) = geometry.ends();
    crate::drc::edge::Edge::all(pcb)
        .into_iter()
        .filter(|edge| {
            let wrong_side = [a, b]
                .iter()
                .any(|p| edge.contains([p.x.0, p.y.0]) != edge.keeps_inside);
            wrong_side || edge.gap(&shape).0 < required.0 as f64
        })
        .map(|edge| edge.uuid)
        .collect()
}

/// The furthest point from `from` towards `to` up to which the segment stays clear, to
/// the nanometre: it may be off grid, because pulling back to the grid could stop short
/// of a gap the route fits through.
fn last_clear(copper: &Copper, pcb: &PcbDocument, r: &Request, from: Point, to: Point) -> Point {
    let along = |t: f64| {
        Point::new(
            Nm((from.x.0 as f64 + (to.x.0 - from.x.0) as f64 * t).floor() as i64),
            Nm((from.y.0 as f64 + (to.y.0 - from.y.0) as f64 * t).floor() as i64),
        )
    };
    let length = ((to.x.0 - from.x.0) as f64).hypot((to.y.0 - from.y.0) as f64);
    if !check(copper, pcb, r, from, from).is_empty() {
        return from;
    }
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    while (hi - lo) * length > 1.0 {
        let mid = (lo + hi) / 2.0;
        if check(copper, pcb, r, from, along(mid)).is_empty() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    along(lo)
}

/// Whether `at` is on a Pad, Free Pad, Via or Track of `net` on `layer`. Zones don't end
/// a route.
fn on_own_copper(copper: &Copper, layer: Uuid, net: Option<Uuid>, at: Point) -> bool {
    let Some(net) = net else {
        return false;
    };
    copper
        .items_at(layer, at)
        .into_iter()
        .any(|i| !matches!(i, Item::Zone { .. }) && copper.net(i) == Some(net))
}

/// Whether `at` is in a `zones_only` Zone of `net` on `layer`.
fn in_zone_only(
    copper: &Copper,
    pcb: &PcbDocument,
    layer: Uuid,
    net: Option<Uuid>,
    at: Point,
) -> bool {
    let Some(net) = net else {
        return false;
    };
    copper.items_at(layer, at).into_iter().any(|i| match i {
        Item::Zone { zone, .. } => {
            copper.net(i) == Some(net)
                && pcb
                    .zone(zone)
                    .is_some_and(|z| z.pour_over == PourOver::ZonesOnly)
        }
        _ => false,
    })
}

/// A stretch of placed route on one Layer at one width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leg {
    pub layer: Uuid,
    pub width: Nm,
    pub points: Vec<Point>,
}

/// What a route has placed, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Leg(Leg),
    Via(Point),
}

/// The Commands that commit a route, and what loop removal decided.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Commit {
    /// `{id, params}` steps, in order: loop-removal deletes, the pieces of split Tracks
    /// that stay, then the route's Tracks and Vias.
    pub steps: Vec<Value>,
    pub removes: Removal,
    pub note: Option<Note>,
}

impl Commit {
    /// The one Command to run: a single Track with nothing else as a plain
    /// `pcb.track.add`, anything more as one `batch` (one Journal entry, one undo step).
    pub fn command(&self) -> Option<(String, Value)> {
        match self.steps.len() {
            0 => None,
            1 => Some((
                self.steps[0]["id"].as_str().expect("a step id").to_owned(),
                self.steps[0]["params"].clone(),
            )),
            _ => Some(("batch".to_owned(), json!({ "commands": self.steps }))),
        }
    }
}

/// The Commands that commit `steps` on `net`: with loop removal, the old path between the
/// route's two ends goes in the same batch, before the new copper.
pub fn commit(
    copper: &Copper,
    pcb: &PcbDocument,
    net: Option<Uuid>,
    steps: &[Step],
    options: &LoopOptions,
) -> Commit {
    let mut out = Commit::default();
    let legs: Vec<&Leg> = steps
        .iter()
        .filter_map(|s| match s {
            Step::Leg(l) if l.points.len() > 1 => Some(l),
            _ => None,
        })
        .collect();
    if let (Some(net), Some(first), Some(last)) = (net, legs.first(), legs.last()) {
        let origin = RouteEnd {
            layer: first.layer,
            at: first.points[0],
        };
        let end = RouteEnd {
            layer: last.layer,
            at: *last.points.last().expect("two points"),
        };
        let width = legs.iter().map(|l| l.width).min().expect("a leg");
        let (removes, note) = loops::plan(pcb, copper, net, origin, end, width, options);
        out.removes = removes;
        out.note = note;
    }
    let net_param = |params: &mut Value| {
        if let Some(net) = net {
            params["net"] = json!(net);
        }
    };
    if !out.removes.objects.is_empty() {
        out.steps.push(json!({"id": "pcb.copper.delete",
                              "params": {"objects": out.removes.objects}}));
    }
    for piece in &out.removes.keep {
        let mut params = json!({
            "layer": piece.layer,
            "points": [point(piece.from), point(piece.to)],
            "width": piece.width.0,
        });
        net_param(&mut params);
        out.steps
            .push(json!({"id": "pcb.track.add", "params": params}));
    }
    for step in steps {
        match step {
            Step::Leg(leg) if leg.points.len() > 1 => {
                let points: Vec<Value> = leg.points.iter().map(|p| point(*p)).collect();
                let mut params = json!({
                    "layer": leg.layer, "points": points, "width": leg.width.0
                });
                net_param(&mut params);
                out.steps
                    .push(json!({"id": "pcb.track.add", "params": params}));
            }
            Step::Leg(_) => {}
            Step::Via(at) => {
                let mut params = json!({"at": point(*at)});
                net_param(&mut params);
                out.steps
                    .push(json!({"id": "pcb.via.add", "params": params}));
            }
        }
    }
    out
}

fn point(p: Point) -> Value {
    json!({"x": p.x.0, "y": p.y.0})
}
