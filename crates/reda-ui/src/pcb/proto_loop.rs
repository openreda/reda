//! PROTOTYPE (issue 136), throwaway. Loop removal: when a finished route joins two points
//! of its Net that were already joined, plan what old copper goes.
//!
//! The rules being tried: the old path is the shortest one between the route's ends that
//! passes through no Pad. A narrower new route keeps it. Dangling stubs on it go, unless
//! they end in a Via. A real branch keeps the whole old path, with a note.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{PcbDocument, TrackGeometry, Uuid};
use reda_engine::pcb::{Copper, Item};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoopPlan {
    pub delete_tracks: Vec<Uuid>,
    pub delete_vias: Vec<Uuid>,
    /// Pieces of split Tracks that stay: re-added in the same batch.
    pub keep: Vec<Part>,
    /// What goes, for the preview.
    pub removed: Vec<Part>,
    pub note: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part {
    pub layer: Uuid,
    pub net: Option<Uuid>,
    pub width: Nm,
    pub from: Point,
    pub to: Point,
}

type Node = (Uuid, i64, i64);

fn node(layer: Uuid, p: Point) -> Node {
    (layer, p.x.0, p.y.0)
}

#[derive(Clone, Copy)]
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

fn length(a: Point, b: Point) -> i64 {
    ((a.x.0 - b.x.0) as f64).hypot((a.y.0 - b.y.0) as f64) as i64
}

fn on_segment(p: Point, a: Point, b: Point) -> bool {
    if p == a || p == b {
        return false;
    }
    let (dx, dy) = ((b.x.0 - a.x.0) as f64, (b.y.0 - a.y.0) as f64);
    let (px, py) = ((p.x.0 - a.x.0) as f64, (p.y.0 - a.y.0) as f64);
    let len = dx.hypot(dy);
    if len == 0.0 {
        return false;
    }
    let along = (px * dx + py * dy) / len;
    let off = (px * dy - py * dx).abs() / len;
    off <= 2.0 && along > 0.0 && along < len
}

pub fn plan(
    pcb: &PcbDocument,
    copper: &Copper,
    net: Uuid,
    start: (Uuid, Point),
    end: (Uuid, Point),
    new_width: Nm,
    through_vias: bool,
) -> LoopPlan {
    let mut out = LoopPlan::default();
    let tracks: Vec<_> = pcb
        .tracks()
        .filter(|t| t.net == Some(net) && matches!(t.geometry, TrackGeometry::Line { .. }))
        .collect();
    let vias: Vec<_> = pcb.vias().filter(|v| v.net == Some(net)).collect();

    // Every Track is cut where something else of the Net joins it, so that a T in the
    // middle of a segment counts as a branch.
    let mut marks: Vec<(Uuid, Point)> = vec![start, end];
    for t in &tracks {
        let (a, b) = t.geometry.ends();
        marks.push((t.layer, a));
        marks.push((t.layer, b));
    }
    for v in &vias {
        for l in pcb.copper_layers() {
            marks.push((l.uuid, v.at));
        }
    }
    let mut edges: Vec<Edge> = Vec::new();
    let mut parts: BTreeMap<Uuid, Vec<usize>> = BTreeMap::new();
    let mut info: BTreeMap<Uuid, (Uuid, Nm)> = BTreeMap::new();
    for t in &tracks {
        let (a, b) = t.geometry.ends();
        info.insert(t.uuid, (t.layer, t.width));
        let mut cuts: Vec<Point> = marks
            .iter()
            .filter(|(l, p)| *l == t.layer && on_segment(*p, a, b))
            .map(|(_, p)| *p)
            .collect();
        cuts.push(a);
        cuts.push(b);
        cuts.sort_by_key(|p| length(a, *p));
        cuts.dedup();
        for pair in cuts.windows(2) {
            parts.entry(t.uuid).or_default().push(edges.len());
            edges.push(Edge {
                kind: Kind::Track(t.uuid, t.width),
                a: node(t.layer, pair[0]),
                b: node(t.layer, pair[1]),
                len: length(pair[0], pair[1]),
            });
        }
    }
    let copper_layers: Vec<Uuid> = pcb.copper_layers().map(|l| l.uuid).collect();
    for v in &vias {
        if let [top, bottom] = copper_layers[..] {
            edges.push(Edge {
                kind: Kind::Via(v.uuid),
                a: node(top, v.at),
                b: node(bottom, v.at),
                len: 0,
            });
        }
    }
    let mut at_node: BTreeMap<Node, Vec<usize>> = BTreeMap::new();
    for (i, e) in edges.iter().enumerate() {
        at_node.entry(e.a).or_default().push(i);
        at_node.entry(e.b).or_default().push(i);
    }
    let point = |n: Node| Point::new(Nm(n.1), Nm(n.2));
    let on_pad = |n: Node| {
        copper
            .items_at(n.0, point(n))
            .iter()
            .any(|i| matches!(i, Item::Pad { .. } | Item::FreePad { .. }))
    };
    // The nodes a route end stands on: its own point, and every node on the same Pad or Via.
    let anchors = |(layer, p): (Uuid, Point)| -> BTreeSet<Node> {
        let solid: Vec<Item> = copper
            .items_at(layer, p)
            .into_iter()
            .filter(|i| matches!(i, Item::Pad { .. } | Item::FreePad { .. } | Item::Via { .. }))
            .collect();
        at_node
            .keys()
            .filter(|n| {
                **n == node(layer, p)
                    || (!solid.is_empty()
                        && copper
                            .items_at(n.0, point(**n))
                            .iter()
                            .any(|i| solid.contains(i)))
            })
            .copied()
            .collect()
    };
    let sources = anchors(start);
    let targets = anchors(end);
    if sources.is_empty() || targets.is_empty() || !sources.is_disjoint(&targets) {
        return out;
    }

    // Shortest old path that passes through no Pad.
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
        if on_pad(n) && !sources.contains(&n) {
            continue;
        }
        for &e in at_node.get(&n).into_iter().flatten() {
            let edge = edges[e];
            let far = if edge.a == n { edge.b } else { edge.a };
            let next = d + edge.len;
            if dist.get(&far).is_none_or(|x| next < *x) {
                dist.insert(far, next);
                prev.insert(far, e);
                heap.push(Reverse((next, far)));
            }
        }
    }
    let Some(mut n) = found else {
        return out;
    };
    let mut path: Vec<usize> = Vec::new();
    let mut seq = vec![n];
    while !sources.contains(&n) {
        let e = prev[&n];
        path.push(e);
        n = if edges[e].a == n { edges[e].b } else { edges[e].a };
        seq.push(n);
    }
    path.reverse();
    seq.reverse();
    if path.is_empty() {
        return out;
    }

    let widest = path
        .iter()
        .filter_map(|e| match edges[*e].kind {
            Kind::Track(_, w) => Some(w),
            Kind::Via(_) => None,
        })
        .max()
        .unwrap_or(Nm(0));
    if new_width < widest {
        out.note = Some("loop kept: the new route is narrower than the old one".into());
        return out;
    }
    if !through_vias && path.iter().any(|e| matches!(edges[*e].kind, Kind::Via(_))) {
        out.note = Some("loop kept: the old path goes through a Via".into());
        return out;
    }

    // Stubs and branches on the old path's inner nodes.
    let on_path: BTreeSet<usize> = path.iter().copied().collect();
    let mut stubs: BTreeMap<Node, Vec<usize>> = BTreeMap::new();
    let mut branch_at: BTreeSet<Node> = BTreeSet::new();
    for n in &seq[1..seq.len() - 1] {
        for &first in &at_node[n] {
            if on_path.contains(&first) {
                continue;
            }
            let mut walked = vec![first];
            let mut cur = *n;
            let mut edge = first;
            let stub = loop {
                let e = edges[edge];
                cur = if e.a == cur { e.b } else { e.a };
                if matches!(e.kind, Kind::Via(_)) || on_pad(cur) {
                    break false;
                }
                let others: Vec<usize> = at_node[&cur]
                    .iter()
                    .copied()
                    .filter(|x| *x != edge)
                    .collect();
                match others[..] {
                    [] => break true,
                    [next] if matches!(edges[next].kind, Kind::Track(..)) => {
                        walked.push(next);
                        edge = next;
                    }
                    _ => break false,
                }
            };
            if stub {
                stubs.entry(*n).or_default().extend(walked);
            } else {
                branch_at.insert(*n);
            }
        }
    }
    // A Via into a pour holds something up too.
    for (i, e) in path.iter().enumerate() {
        if let Kind::Via(uuid) = edges[*e].kind
            && copper
                .contacts(Item::Via { uuid })
                .any(|c| matches!(c, Item::Zone { .. }))
        {
            branch_at.insert(seq[i]);
        }
    }

    // Longest section between real branches.
    let mut sections: Vec<(Vec<usize>, Vec<Node>)> = vec![(Vec::new(), Vec::new())];
    for (i, e) in path.iter().enumerate() {
        sections.last_mut().expect("one").0.push(*e);
        if i + 1 < path.len() {
            if branch_at.contains(&seq[i + 1]) {
                sections.push((Vec::new(), Vec::new()));
            } else {
                sections.last_mut().expect("one").1.push(seq[i + 1]);
            }
        }
    }
    let total = sections.len();
    // Decided on the ticket: a real branch keeps the whole old path, and says so.
    if total > 1 {
        out.note = Some("loop kept: a branch joins the old path, delete it by hand".into());
        return out;
    }
    let (mut gone, inner) = sections
        .into_iter()
        .max_by_key(|(es, _)| es.iter().map(|e| edges[*e].len).sum::<i64>())
        .expect("one");
    for n in inner {
        if let Some(s) = stubs.get(&n) {
            gone.extend(s.iter().copied());
        }
    }
    let gone: BTreeSet<usize> = gone.into_iter().collect();
    for e in &gone {
        let edge = edges[*e];
        match edge.kind {
            Kind::Via(uuid) => out.delete_vias.push(uuid),
            Kind::Track(uuid, width) => {
                out.removed.push(Part {
                    layer: edge.a.0,
                    net: Some(net),
                    width,
                    from: point(edge.a),
                    to: point(edge.b),
                });
                if !out.delete_tracks.contains(&uuid) {
                    out.delete_tracks.push(uuid);
                }
            }
        }
    }
    for uuid in out.delete_tracks.clone() {
        let (layer, width) = info[&uuid];
        let mut run: Option<(Point, Point)> = None;
        for e in &parts[&uuid] {
            let edge = edges[*e];
            if gone.contains(e) {
                if let Some((from, to)) = run.take() {
                    out.keep.push(Part { layer, net: Some(net), width, from, to });
                }
            } else {
                run = Some(match run {
                    Some((from, _)) => (from, point(edge.b)),
                    None => (point(edge.a), point(edge.b)),
                });
            }
        }
        if let Some((from, to)) = run {
            out.keep.push(Part { layer, net: Some(net), width, from, to });
        }
    }
    out
}
