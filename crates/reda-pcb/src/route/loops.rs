//! Loop removal: when a route ends by joining two points already joined on its Net, the
//! old path goes in the same `batch`.
//!
//! - The old path is the shortest same-Net path between the route's two ends that passes
//!   through no Pad. A Track is split where needed, and what stays is re-added.
//! - It stays, with a [`Note`], when the new route is narrower than its widest Track,
//!   when a real branch joins it (a Via touching a Zone included), or when it goes
//!   through a Via and `remove_loops_through_vias` is off.
//! - A dangling stub on it goes too, unless it ends in a Via or `remove_antennas` is off.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, TrackGeometry, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Note, RouteEnd};
use crate::copper::{Copper, Item};

/// The loop-removal switches: `pcb.route.loop_removal` (and `Shift+L`),
/// `pcb.route.remove_antennas` and `pcb.route.remove_loops_through_vias`. A Net's own
/// `loop_removal` turns it off for that Net whatever these say.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LoopOptions {
    pub enabled: bool,
    pub antennas: bool,
    pub through_vias: bool,
}

impl Default for LoopOptions {
    fn default() -> Self {
        LoopOptions {
            enabled: true,
            antennas: true,
            through_vias: true,
        }
    }
}

/// A straight piece of Track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Piece {
    pub layer: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    pub width: Nm,
    pub from: Point,
    pub to: Point,
}

/// What loop removal deletes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Removal {
    /// The Tracks and Vias deleted, sorted.
    pub objects: Vec<Uuid>,
    /// The pieces of split Tracks that stay, re-added in the same batch.
    pub keep: Vec<Piece>,
    /// The pieces of copper that go, for the red preview.
    pub removed: Vec<Piece>,
}

impl Removal {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

type Node = (Uuid, i64, i64);

fn node(layer: Uuid, p: Point) -> Node {
    (layer, p.x.0, p.y.0)
}

fn point(n: Node) -> Point {
    Point::new(Nm(n.1), Nm(n.2))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Track(Uuid, Nm),
    Via(Uuid),
}

#[derive(Clone, Copy)]
struct Edge {
    kind: Kind,
    a: Node,
    b: Node,
    len: i64,
}

impl Edge {
    fn far(&self, n: Node) -> Node {
        if self.a == n { self.b } else { self.a }
    }
}

fn length(a: Point, b: Point) -> i64 {
    ((a.x.0 - b.x.0) as f64).hypot((a.y.0 - b.y.0) as f64) as i64
}

/// The Net's Tracks and Vias as a graph: Track pieces cut wherever other copper of the
/// Net (or a route end) meets their interior, and Vias joining their copper Layers.
struct Graph {
    edges: Vec<Edge>,
    at: BTreeMap<Node, Vec<usize>>,
    /// Each Track's pieces in order from its start.
    pieces: BTreeMap<Uuid, Vec<usize>>,
    tracks: BTreeMap<Uuid, (Uuid, Nm)>,
}

impl Graph {
    fn of(pcb: &PcbDocument, net: Uuid, ends: [RouteEnd; 2]) -> Graph {
        let tracks: Vec<_> = pcb
            .tracks()
            .filter(|t| t.net == Some(net) && matches!(t.geometry, TrackGeometry::Line { .. }))
            .collect();
        let vias: Vec<(Uuid, Point, Vec<Uuid>)> = pcb
            .vias()
            .filter(|v| v.net == Some(net))
            .map(|v| (v.uuid, v.at, pcb.span_layers(v.span)))
            .collect();
        let mut marks: Vec<(Uuid, Point)> = ends.iter().map(|e| (e.layer, e.at)).collect();
        for t in &tracks {
            let (a, b) = t.geometry.ends();
            marks.push((t.layer, a));
            marks.push((t.layer, b));
        }
        for (_, at, layers) in &vias {
            marks.extend(layers.iter().map(|l| (*l, *at)));
        }
        let mut g = Graph {
            edges: Vec::new(),
            at: BTreeMap::new(),
            pieces: BTreeMap::new(),
            tracks: BTreeMap::new(),
        };
        for t in &tracks {
            let (a, b) = t.geometry.ends();
            g.tracks.insert(t.uuid, (t.layer, t.width));
            let mut cuts: Vec<Point> = marks
                .iter()
                .filter(|(l, p)| *l == t.layer && *p != a && *p != b && p.on_segment(a, b))
                .map(|(_, p)| *p)
                .collect();
            cuts.push(a);
            cuts.push(b);
            cuts.sort_by_key(|p| length(a, *p));
            cuts.dedup();
            for pair in cuts.windows(2) {
                g.pieces.entry(t.uuid).or_default().push(g.edges.len());
                g.edges.push(Edge {
                    kind: Kind::Track(t.uuid, t.width),
                    a: node(t.layer, pair[0]),
                    b: node(t.layer, pair[1]),
                    len: length(pair[0], pair[1]),
                });
            }
        }
        for (uuid, at, layers) in &vias {
            for pair in layers.windows(2) {
                g.edges.push(Edge {
                    kind: Kind::Via(*uuid),
                    a: node(pair[0], *at),
                    b: node(pair[1], *at),
                    len: 0,
                });
            }
        }
        for (i, e) in g.edges.iter().enumerate() {
            g.at.entry(e.a).or_default().push(i);
            g.at.entry(e.b).or_default().push(i);
        }
        g
    }

    fn edges_at(&self, n: Node) -> &[usize] {
        self.at.get(&n).map_or(&[], Vec::as_slice)
    }
}

/// What goes when a route on `net` from `origin` to `end` (at `width`, its narrowest)
/// joins two points already joined, and the note when the old path stays.
pub fn plan(
    pcb: &PcbDocument,
    copper: &Copper,
    net: Uuid,
    origin: RouteEnd,
    end: RouteEnd,
    width: Nm,
    options: &LoopOptions,
) -> (Removal, Option<Note>) {
    let none = (Removal::default(), None);
    if !options.enabled || pcb.net(net).is_some_and(|n| !n.loop_removal) {
        return none;
    }
    let g = Graph::of(pcb, net, [origin, end]);
    let is_pad = |n: Node| {
        copper
            .items_at(n.0, point(n))
            .iter()
            .any(|i| matches!(i, Item::Pad { .. } | Item::FreePad { .. }))
    };
    // The nodes a route end stands on: its own point, and every node on the same Pad or
    // Via.
    let anchors = |e: RouteEnd| -> BTreeSet<Node> {
        let solid: Vec<Item> = copper
            .items_at(e.layer, e.at)
            .into_iter()
            .filter(|i| {
                matches!(
                    i,
                    Item::Pad { .. } | Item::FreePad { .. } | Item::Via { .. }
                )
            })
            .collect();
        g.at.keys()
            .filter(|n| {
                **n == node(e.layer, e.at)
                    || (!solid.is_empty()
                        && copper
                            .items_at(n.0, point(**n))
                            .iter()
                            .any(|i| solid.contains(i)))
            })
            .copied()
            .collect()
    };
    let sources = anchors(origin);
    let targets = anchors(end);
    if sources.is_empty() || targets.is_empty() || !sources.is_disjoint(&targets) {
        return none;
    }

    // The shortest old path that passes through no Pad.
    let mut dist: BTreeMap<Node, i64> = BTreeMap::new();
    let mut prev: BTreeMap<Node, usize> = BTreeMap::new();
    let mut heap = BinaryHeap::new();
    for s in &sources {
        dist.insert(*s, 0);
        heap.push(Reverse((0i64, *s)));
    }
    let mut found = None;
    while let Some(Reverse((d, n))) = heap.pop() {
        if dist.get(&n).is_some_and(|x| *x < d) {
            continue;
        }
        if targets.contains(&n) {
            found = Some(n);
            break;
        }
        if !sources.contains(&n) && is_pad(n) {
            continue;
        }
        for &e in g.edges_at(n) {
            let far = g.edges[e].far(n);
            let next = d + g.edges[e].len;
            if dist.get(&far).is_none_or(|x| next < *x) {
                dist.insert(far, next);
                prev.insert(far, e);
                heap.push(Reverse((next, far)));
            }
        }
    }
    let Some(mut n) = found else {
        return none;
    };
    let mut path: Vec<usize> = Vec::new();
    let mut seq = vec![n];
    while !sources.contains(&n) {
        let e = prev[&n];
        path.push(e);
        n = g.edges[e].far(n);
        seq.push(n);
    }
    path.reverse();
    seq.reverse();
    if path.is_empty() {
        return none;
    }

    let widest = path
        .iter()
        .filter_map(|e| match g.edges[*e].kind {
            Kind::Track(_, w) => Some(w),
            Kind::Via(_) => None,
        })
        .max()
        .unwrap_or(Nm::ZERO);
    if width < widest {
        return (Removal::default(), Some(Note::LoopNarrower));
    }
    let vias_on_path = path
        .iter()
        .any(|e| matches!(g.edges[*e].kind, Kind::Via(_)));
    if vias_on_path && !options.through_vias {
        return (Removal::default(), Some(Note::LoopThroughVias));
    }

    // Stubs and branches leaving the path's inner nodes.
    let on_path: BTreeSet<usize> = path.iter().copied().collect();
    let mut gone: BTreeSet<usize> = on_path.clone();
    for n in &seq[1..seq.len() - 1] {
        for &first in g.edges_at(*n) {
            if on_path.contains(&first) {
                continue;
            }
            match walk_stub(&g, copper, *n, first, &is_pad) {
                Stub::Dangling(edges) => {
                    if options.antennas {
                        gone.extend(edges);
                    }
                }
                Stub::EndsInVia => {}
                Stub::Branch => return (Removal::default(), Some(Note::LoopBranch)),
            }
        }
    }
    // A Via into a pour holds something up too.
    let via_in_zone = path.iter().any(|e| match g.edges[*e].kind {
        Kind::Via(uuid) => copper
            .contacts(Item::Via { uuid })
            .any(|c| matches!(c, Item::Zone { .. })),
        Kind::Track(..) => false,
    });
    if via_in_zone {
        return (Removal::default(), Some(Note::LoopBranch));
    }

    // Locked copper stays, and with it the old path.
    let locked = gone.iter().any(|e| match g.edges[*e].kind {
        Kind::Via(uuid) | Kind::Track(uuid, _) => crate::lock::is_locked(pcb, uuid),
    });
    if locked {
        return (Removal::default(), Some(Note::LoopLocked));
    }

    let mut out = Removal::default();
    let mut tracks = BTreeSet::new();
    for e in &gone {
        let edge = g.edges[*e];
        match edge.kind {
            Kind::Via(uuid) => {
                out.objects.push(uuid);
            }
            Kind::Track(uuid, width) => {
                out.removed.push(Piece {
                    layer: edge.a.0,
                    net: Some(net),
                    width,
                    from: point(edge.a),
                    to: point(edge.b),
                });
                tracks.insert(uuid);
            }
        }
    }
    for uuid in &tracks {
        out.objects.push(*uuid);
        let (layer, width) = g.tracks[uuid];
        let mut run: Option<(Point, Point)> = None;
        let mut keep = |run: &mut Option<(Point, Point)>| {
            if let Some((from, to)) = run.take() {
                out.keep.push(Piece {
                    layer,
                    net: Some(net),
                    width,
                    from,
                    to,
                });
            }
        };
        for e in &g.pieces[uuid] {
            let edge = g.edges[*e];
            if gone.contains(e) {
                keep(&mut run);
            } else {
                run = Some(match run {
                    Some((from, _)) => (from, point(edge.b)),
                    None => (point(edge.a), point(edge.b)),
                });
            }
        }
        keep(&mut run);
    }
    out.objects.sort();
    out.objects.dedup();
    (out, None)
}

enum Stub {
    /// A dead end of Tracks only: these edges.
    Dangling(Vec<usize>),
    /// Tracks ending in a Via that holds nothing else up: it stays.
    EndsInVia,
    /// It reaches a Pad, a junction or copper beyond a Via.
    Branch,
}

/// Follows what leaves the path at `from` along `first`.
fn walk_stub(
    g: &Graph,
    copper: &Copper,
    from: Node,
    first: usize,
    is_pad: &impl Fn(Node) -> bool,
) -> Stub {
    let mut walked = vec![first];
    let mut cur = from;
    let mut edge = first;
    loop {
        let e = g.edges[edge];
        if let Kind::Via(uuid) = e.kind {
            // A Via is a dead end only when nothing but this stub touches it.
            let others = g.at.iter().any(|(n, es)| {
                n.1 == cur.1
                    && n.2 == cur.2
                    && es
                        .iter()
                        .any(|x| !walked.contains(x) && !matches!(g.edges[*x].kind, Kind::Via(_)))
            });
            let zone = copper
                .contacts(Item::Via { uuid })
                .any(|c| matches!(c, Item::Zone { .. }));
            return if others || zone || walked.len() == 1 {
                Stub::Branch
            } else {
                Stub::EndsInVia
            };
        }
        cur = e.far(cur);
        if is_pad(cur) {
            return Stub::Branch;
        }
        let others: Vec<usize> = g
            .edges_at(cur)
            .iter()
            .copied()
            .filter(|x| *x != edge)
            .collect();
        match others[..] {
            [] => return Stub::Dangling(walked),
            [next] => {
                walked.push(next);
                edge = next;
            }
            _ => return Stub::Branch,
        }
    }
}
