//! One face to triangles: its bounds mapped into the surface's parameter space
//! ([`uv`], the trimming curve fitter), closed into polygons there, triangulated with
//! constrained Delaunay between them ([`cdt`]) and mapped back onto the surface.
//!
//! - Boundary points keep the 3D positions of their edges' samples, so faces that share an
//!   edge share its points exactly and the mesh closes.
//! - What a well-made file would not hold is drawn rather than refused:
//!   - a bound that encloses no area (a slit, or an edge out and back) is dropped;
//!   - a band face whose bounds each run once around the surface's axis, with no seam edge
//!     between them, is split along a seam drawn between them;
//!   - a face with one bound around the axis (a cap with no seam) is closed at the pole the
//!     surface meets its axis at, or along the open end of its range (the end of a surface of
//!     revolution's curve).

mod cdt;
mod uv;

use std::collections::HashMap;

use super::MeshOptions;
use super::freeform::End;
use super::geometry::V3;
use super::surface::{Surface, UvBox};
use crate::StepModel;
use crate::records::{
    EdgeCurve, EdgeLoop, Face, FaceBound, FaceBoundKind, OrientedEdge, RecordId, VertexLoop,
};
use uv::{UvLoop, wrap_near};

/// A face's triangles: positions, normals and triangles over them, facing outwards.
#[derive(Default)]
pub(crate) struct FaceMesh {
    pub positions: Vec<V3>,
    pub normals: Vec<V3>,
    pub triangles: Vec<[u32; 3]>,
}

/// Samples each edge once for every face that uses it. Two edges between the same vertices
/// along the same curve (a file that gives neighbouring faces an edge each instead of one
/// between them) take the same points, so the faces still meet.
pub(crate) struct EdgeCache<'a> {
    model: &'a StepModel,
    options: &'a MeshOptions,
    edges: HashMap<RecordId, Result<Vec<V3>, String>>,
    /// The edges sampled so far between each pair of vertices.
    between: HashMap<(RecordId, RecordId), Vec<RecordId>>,
}

/// How near, in mm, two edges' points must all be for the edges to count as one.
const TWIN: f64 = 1e-6;

impl<'a> EdgeCache<'a> {
    pub fn new(model: &'a StepModel, options: &'a MeshOptions) -> Self {
        EdgeCache {
            model,
            options,
            edges: HashMap::new(),
            between: HashMap::new(),
        }
    }

    fn edge(&mut self, id: RecordId) -> Result<&[V3], String> {
        if !self.edges.contains_key(&id) {
            let sampled = self.sample(id);
            self.edges.insert(id, sampled);
        }
        self.edges[&id].as_deref().map_err(|e| e.clone())
    }

    fn sample(&mut self, id: RecordId) -> Result<Vec<V3>, String> {
        let points = super::curve::sample_edge(self.model, id, self.options)?;
        let Some(e) = self.model.get::<EdgeCurve>(id) else {
            return Ok(points);
        };
        let ends = (e.edge_start.min(e.edge_end), e.edge_start.max(e.edge_end));
        let twins = self.between.entry(ends).or_default();
        for &t in twins.iter() {
            let Some(Ok(other)) = self.edges.get(&t) else {
                continue;
            };
            if other.len() != points.len() {
                continue;
            }
            let near = |a: &V3, b: &V3| (*a - *b).length() <= TWIN;
            if other.iter().zip(&points).all(|(a, b)| near(a, b)) {
                return Ok(other.clone());
            }
            if other.iter().rev().zip(&points).all(|(a, b)| near(a, b)) {
                return Ok(other.iter().rev().copied().collect());
            }
        }
        twins.push(id);
        Ok(points)
    }
}

/// A run of boundary points along one edge, in loop order.
struct Run {
    edge: RecordId,
    points: Vec<V3>,
}

/// One bound of a face in 3D: its runs end to end, the last ending where the first starts.
struct Bound3 {
    outer: bool,
    runs: Vec<Run>,
}

/// A boundary point in parameter space with its 3D position.
#[derive(Clone, Copy, Debug)]
struct UvPoint {
    u: f64,
    v: f64,
    pos: V3,
}

impl UvPoint {
    fn c(&self, k: usize) -> f64 {
        if k == 0 { self.u } else { self.v }
    }
    fn c_mut(&mut self, k: usize) -> &mut f64 {
        if k == 0 { &mut self.u } else { &mut self.v }
    }
}

/// Meshes face `id`, or says why it cannot be (`face #n: ...`). `reversed` turns it inside
/// out (a void's shell, or a shell used backwards).
pub(crate) fn mesh_face(
    cache: &mut EdgeCache,
    id: RecordId,
    reversed: bool,
) -> Result<FaceMesh, String> {
    face_mesh(cache, id, reversed).map_err(|e| format!("face #{}: {e}", id.0 + 1))
}

fn face_mesh(cache: &mut EdgeCache, id: RecordId, reversed: bool) -> Result<FaceMesh, String> {
    let model = cache.model;
    let options = cache.options;
    let face = model
        .get::<Face>(id)
        .ok_or_else(|| format!("#{} is not a face", id.index() + 1))?;
    let surface = Surface::of(model, face.face_geometry)?;
    let flip = face.same_sense == reversed;

    let mut bounds = Vec::new();
    for &b in &face.bounds {
        if let Some(bound) = bound(cache, b)? {
            bounds.push(bound);
        }
    }
    if bounds.is_empty() {
        // A face with no edges (only vertex loops, or none) on a closed surface is all of it.
        return cdt::whole(&surface, options, flip)
            .ok_or_else(|| "it has no edges and its surface is not closed".to_string());
    }

    let mut loops: Vec<UvLoop> = Vec::new();
    let mut outer = 0;
    for b in &bounds {
        if b.outer {
            outer = loops.len();
        }
        loops.push(uv::to_uv(&surface, b)?);
    }
    let outer_loop = drop_empty(&surface, options, &mut loops, outer);
    if loops.is_empty() {
        // Nothing but bounds that enclose nothing: the face has no area to draw.
        return Ok(FaceMesh::default());
    }
    let (polygons, outer) = close(&surface, options, face.same_sense, loops, outer_loop)?;
    let mut polygons = polygons;
    align_loops(&surface, &mut polygons, outer);
    cdt::triangulate(&surface, options, &polygons, flip)
}

/// A bound's runs in loop order, or `None` for a bound with no edges (a vertex loop).
fn bound(cache: &mut EdgeCache, id: RecordId) -> Result<Option<Bound3>, String> {
    let model = cache.model;
    let b = model
        .get::<FaceBound>(id)
        .ok_or_else(|| format!("bound #{} is not a face bound", id.0 + 1))?;
    let mut runs = Vec::new();
    if let Some(l) = model.get::<EdgeLoop>(b.bound) {
        for &oe in &l.edge_list {
            let oe = model
                .get::<OrientedEdge>(oe)
                .ok_or_else(|| format!("#{} is not an oriented edge", oe.0 + 1))?;
            let mut points = cache.edge(oe.edge_element)?.to_vec();
            if !oe.orientation {
                points.reverse();
            }
            runs.push(Run {
                edge: oe.edge_element,
                points,
            });
        }
    } else if model.get::<VertexLoop>(b.bound).is_some() {
        return Ok(None);
    } else {
        return Err(format!(
            "loop #{} is a {}, which is not meshed yet",
            b.bound.0 + 1,
            model.record(b.bound).entity_name()
        ));
    }
    if runs.is_empty() {
        return Ok(None);
    }
    if !b.orientation {
        runs.reverse();
        for r in &mut runs {
            r.points.reverse();
        }
    }
    Ok(Some(Bound3 {
        outer: b.kind == FaceBoundKind::Outer,
        runs,
    }))
}

/// How small an area, relative to the square of its perimeter, a bound may enclose and still
/// count as enclosing nothing.
const NO_AREA: f64 = 1e-9;

/// Drops the bounds that enclose no area (a loop that runs out and back along itself) or have
/// no points; returns where the outer bound now is (`None` if it was dropped).
fn drop_empty(
    surface: &Surface,
    options: &MeshOptions,
    loops: &mut Vec<UvLoop>,
    outer: usize,
) -> Option<usize> {
    let pts = || loops.iter().flat_map(|l| &l.points);
    let (mut b, mut any) = (
        UvBox {
            u: (f64::INFINITY, f64::NEG_INFINITY),
            v: (f64::INFINITY, f64::NEG_INFINITY),
        },
        false,
    );
    for p in pts() {
        any = true;
        b.u = (b.u.0.min(p.u), b.u.1.max(p.u));
        b.v = (b.v.0.min(p.v), b.v.1.max(p.v));
    }
    let (su, sv) = if any {
        surface.scale(options, b)
    } else {
        (1.0, 1.0)
    };
    let mut keep = Vec::with_capacity(loops.len());
    let mut outer_at = None;
    for (i, l) in std::mem::take(loops).into_iter().enumerate() {
        let empty = l.points.len() < 2
            || (l.winding == [0, 0] && {
                let n = l.points.len();
                let (mut area, mut perimeter) = (0.0, 0.0);
                for j in 0..n {
                    let (a, c) = (l.points[j], l.points[(j + 1) % n]);
                    area += (a.u * su) * (c.v * sv) - (c.u * su) * (a.v * sv);
                    perimeter += ((c.u - a.u) * su).hypot((c.v - a.v) * sv);
                }
                (0.5 * area).abs() <= NO_AREA * perimeter * perimeter
            });
        if empty {
            continue;
        }
        if i == outer {
            outer_at = Some(keep.len());
        }
        keep.push(l);
    }
    *loops = keep;
    outer_at
}

/// One side of a band: a bound running around the surface, or the end of the surface's range.
#[derive(Clone, Copy)]
enum Side {
    /// A winding bound, and the shift (whole periods) that puts it beside the other side.
    Bound(usize, f64),
    End(End),
}

/// The face's bounds as closed polygons, and which is the outer one. Bounds that close are
/// kept as they are; bounds that run around the surface are paired into bands, or closed at
/// the end of the surface's range, each band cut open along a seam into one polygon.
fn close(
    surface: &Surface,
    options: &MeshOptions,
    same_sense: bool,
    loops: Vec<UvLoop>,
    outer: Option<usize>,
) -> Result<(Vec<Vec<UvPoint>>, usize), String> {
    let winding: Vec<usize> = (0..loops.len())
        .filter(|&i| loops[i].winding != [0, 0])
        .collect();
    if winding.is_empty() {
        let outer = outer.unwrap_or(0);
        return Ok((loops.into_iter().map(|l| l.points).collect(), outer));
    }
    // The direction the bounds run around (k), and the one across it (a).
    let k = if loops[winding[0]].winding[0] != 0 {
        0
    } else {
        1
    };
    let a = 1 - k;
    for &i in &winding {
        let w = loops[i].winding;
        if w[a] != 0 || w[k].abs() != 1 {
            return Err("a bound runs around the surface more than once".into());
        }
    }
    let periods = surface.periods();
    let per = if k == 0 { periods.0 } else { periods.1 }.expect("winding needs a period");
    let across_period = if a == 0 { periods.0 } else { periods.1 };
    let (low, high) = surface.ends(k);

    // Which side of each winding bound the face lies on, across it: a bound has the face on
    // its left seen from the face's normal side, and (u, v) is right-handed about the
    // surface's normal.
    let sense = if same_sense { 1 } else { -1 };
    let turn = if k == 0 { 1 } else { -1 };
    let mean = |i: usize| {
        let p = &loops[i].points;
        p.iter().map(|q| q.c(a)).sum::<f64>() / p.len() as f64
    };
    let mut order: Vec<(f64, usize, bool)> = winding
        .iter()
        .map(|&i| (mean(i), i, loops[i].winding[k] * sense * turn > 0))
        .collect();
    order.sort_by(|x, y| x.0.total_cmp(&y.0));

    // Where the surface has no end to close a band at, it is closed by a seam along the
    // parameter line through the farthest point of its bounds, left open.
    let across = || {
        winding
            .iter()
            .flat_map(|&i| loops[i].points.iter().map(|p| p.c(a)))
    };
    let seam_low = End {
        at: across().fold(f64::INFINITY, f64::min),
        pole: false,
    };
    let seam_high = End {
        at: across().fold(f64::NEG_INFINITY, f64::max),
        pole: false,
    };
    let (seam_low, seam_high) = (low.or(Some(seam_low)), high.or(Some(seam_high)));
    // A band must have room between its sides: an end beyond its bound, an upper bound above
    // its lower one.
    let extent = |i: usize| {
        loops[i]
            .points
            .iter()
            .map(|p| p.c(a))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
                (lo.min(x), hi.max(x))
            })
    };
    let fits = |bands: &Vec<(Side, Side)>| {
        bands.iter().all(|&(lower, upper)| match (lower, upper) {
            (Side::Bound(i, _), Side::End(e)) => e.at > extent(i).0,
            (Side::End(e), Side::Bound(j, _)) => e.at < extent(j).1,
            (Side::Bound(i, _), Side::Bound(j, shift)) => mean(j) + shift > mean(i),
            (Side::End(_), Side::End(_)) => false,
        })
    };
    let bands = [
        pair_by_side(&order, across_period, low, high),
        pair_in_order(&order, low, high),
        pair_by_side(&order, across_period, seam_low, seam_high),
        pair_in_order(&order, seam_low, seam_high),
    ]
    .into_iter()
    .flatten()
    .find(fits)
    .ok_or("a bound runs around the surface without a seam, and nothing closes it")?;

    let plain: Vec<usize> = (0..loops.len())
        .filter(|&i| loops[i].winding == [0, 0])
        .collect();
    let others: Vec<&[UvPoint]> = plain.iter().map(|&i| &loops[i].points[..]).collect();
    let mut polygons = Vec::with_capacity(bands.len() + plain.len());
    for (lower, upper) in bands {
        polygons.push(band(
            surface, options, &loops, k, per, lower, upper, &others,
        )?);
    }
    for i in plain {
        polygons.push(loops[i].points.clone());
    }
    Ok((polygons, 0))
}

/// Pairs winding bounds into bands by the side the face lies on: a bound with the face above
/// it opens a band, the next one above with the face below closes it (across a period when
/// the direction across wraps), and a band left open runs to the end of the range.
fn pair_by_side(
    order: &[(f64, usize, bool)],
    across_period: Option<f64>,
    low: Option<End>,
    high: Option<End>,
) -> Option<Vec<(Side, Side)>> {
    let n = order.len();
    let mut used = vec![false; n];
    let mut bands = Vec::new();
    for i in 0..n {
        if used[i] || !order[i].2 {
            continue;
        }
        used[i] = true;
        let above = (i + 1..n).find(|&j| !used[j] && !order[j].2);
        let upper = match (above, across_period) {
            (Some(j), _) => {
                used[j] = true;
                Side::Bound(order[j].1, 0.0)
            }
            (None, Some(p)) => {
                let j = (0..i).find(|&j| !used[j] && !order[j].2)?;
                used[j] = true;
                Side::Bound(order[j].1, p)
            }
            (None, None) => Side::End(high?),
        };
        bands.push((Side::Bound(order[i].1, 0.0), upper));
    }
    for j in 0..n {
        if !used[j] {
            used[j] = true;
            bands.push((Side::End(low?), Side::Bound(order[j].1, 0.0)));
        }
    }
    Some(bands)
}

/// Pairs winding bounds into bands in order across the surface, whatever their sides say
/// (a file whose orientations disagree), the odd one closed at whichever end there is.
fn pair_in_order(
    order: &[(f64, usize, bool)],
    low: Option<End>,
    high: Option<End>,
) -> Option<Vec<(Side, Side)>> {
    let mut bands = Vec::new();
    let mut rest = order;
    if order.len() % 2 == 1 {
        match (low, high) {
            (Some(e), _) => {
                bands.push((Side::End(e), Side::Bound(order[0].1, 0.0)));
                rest = &order[1..];
            }
            (None, Some(e)) => {
                let last = order.len() - 1;
                bands.push((Side::Bound(order[last].1, 0.0), Side::End(e)));
                rest = &order[..last];
            }
            (None, None) => return None,
        }
    }
    for pair in rest.chunks(2) {
        bands.push((Side::Bound(pair[0].1, 0.0), Side::Bound(pair[1].1, 0.0)));
    }
    Some(bands)
}

/// One band as a polygon: the lower side from a cut at `c` along `k` to `c + per`, a seam
/// across to the upper side, the upper side back, and the seam's other copy down to the
/// start. The two copies of the seam share their points' positions, so the band closes on
/// itself. The cut is put where the seam crosses no bound (`others` are the face's bounds
/// that do not run around), and then where it is farthest from the others.
#[allow(clippy::too_many_arguments)]
fn band(
    surface: &Surface,
    options: &MeshOptions,
    loops: &[UvLoop],
    k: usize,
    per: f64,
    lower: Side,
    upper: Side,
    others: &[&[UvPoint]],
) -> Result<Vec<UvPoint>, String> {
    let a = 1 - k;
    // The bound the cut is chosen on, and the point of it to cut at.
    let (cut_side, other_side) = match lower {
        Side::Bound(..) => (lower, upper),
        Side::End(_) => (upper, lower),
    };
    let Side::Bound(ci, _) = cut_side else {
        return Err("a band with no bound on either side".into());
    };
    // Each side as a path along +k from its cut to one period on.
    let side_path = |side: Side, near: f64, cut_at: usize| -> Vec<UvPoint> {
        match side {
            Side::Bound(i, shift) => {
                let l = &loops[i];
                let at = if i == ci {
                    cut_at
                } else {
                    nearest(&l.points, k, near, per)
                };
                let mut p = around(l, at, k, per);
                for q in &mut p {
                    *q.c_mut(a) += shift;
                }
                // Start at the period nearest the cut.
                let s = wrap_near(p[0].c(k), near, per) - p[0].c(k);
                for q in &mut p {
                    *q.c_mut(k) += s;
                }
                p
            }
            Side::End(e) => end_path(surface, options, k, per, e, near),
        }
    };
    let paths = |cut_at: usize| {
        let c = loops[ci].points[cut_at].c(k);
        let cut_path = side_path(cut_side, c, cut_at);
        let other_path = side_path(other_side, c, cut_at);
        match lower {
            Side::Bound(..) => (cut_path, other_path),
            Side::End(_) => (other_path, cut_path),
        }
    };
    // How bad a cut is: how many bound segments its seam crosses, then how near it comes to
    // the other bounds.
    let score = |cut_at: usize| {
        let (lp, up) = paths(cut_at);
        let (from, to) = (lp[0], up[0]);
        let mid = 0.5 * (from.c(k) + to.c(k));
        let mut crossings = 0;
        let mut clearance = f64::INFINITY;
        for path in [&lp[..], &up[..]].into_iter().chain(others.iter().copied()) {
            for w in path.windows(2) {
                if seam_hits(from, to, w[0], w[1], k, mid, per) {
                    crossings += 1;
                }
            }
        }
        for o in others {
            for q in o.iter() {
                clearance = clearance.min((wrap_near(q.c(k), mid, per) - mid).abs());
            }
        }
        (crossings, -clearance)
    };
    let n = loops[ci].points.len();
    let cut_at = (0..n)
        .step_by((n / 64).max(1))
        .map(|i| (score(i), i))
        .min_by(|x, y| x.0.0.cmp(&y.0.0).then(x.0.1.total_cmp(&y.0.1)))
        .map(|(_, i)| i)
        .expect("a bound has points");
    let (lower_path, upper_path) = paths(cut_at);

    let (l0, u0) = (lower_path[0], upper_path[0]);
    let seam = seam(surface, options, l0, u0);
    let mut out: Vec<UvPoint> = Vec::new();
    // Lower side along +k, without its last point (the seam's copy starts there).
    out.extend(&lower_path[..lower_path.len() - 1]);
    let lower_end = *lower_path.last().expect("paths have points");
    out.push(lower_end);
    // The seam a period on, up to the upper side.
    for s in &seam {
        let mut q = *s;
        *q.c_mut(k) += per;
        out.push(q);
    }
    // The upper side back along -k.
    out.extend(upper_path.iter().rev());
    // The seam down to the start (the start itself is the polygon's first point).
    out.extend(seam.iter().rev());
    Ok(out)
}

/// Whether the seam from `a` to `b` meets the segment `p`–`q` (moved by whole periods along `k`
/// to lie by `mid`) anywhere but at the seam's own ends: crossing it, or running along it.
fn seam_hits(a: UvPoint, b: UvPoint, p: UvPoint, q: UvPoint, k: usize, mid: f64, per: f64) -> bool {
    let shift = wrap_near(p.c(k), mid, per) - p.c(k);
    let at = |x: UvPoint| {
        let (mut u, mut v) = (x.u, x.v);
        if k == 0 {
            u += shift;
        } else {
            v += shift;
        }
        (u, v)
    };
    let (p, q) = (at(p), at(q));
    let (a, b) = ((a.u, a.v), (b.u, b.v));
    let size = (b.0 - a.0).abs() + (b.1 - a.1).abs() + (q.0 - p.0).abs() + (q.1 - p.1).abs();
    let eps = 1e-9 * size.max(1e-300);
    let same = |x: (f64, f64), y: (f64, f64)| (x.0 - y.0).abs() + (x.1 - y.1).abs() <= eps;
    let orient = |o: (f64, f64), x: (f64, f64), y: (f64, f64)| {
        (x.0 - o.0) * (y.1 - o.1) - (x.1 - o.1) * (y.0 - o.0)
    };
    // Whether x lies on the segment o–y, short of its ends.
    let inside = |o: (f64, f64), y: (f64, f64), x: (f64, f64)| {
        let len2 = (y.0 - o.0).powi(2) + (y.1 - o.1).powi(2);
        let t = ((x.0 - o.0) * (y.0 - o.0) + (x.1 - o.1) * (y.1 - o.1)) / len2.max(1e-300);
        orient(o, y, x).abs() <= eps * len2.sqrt() && t > 1e-9 && t < 1.0 - 1e-9
    };
    if same(a, b) {
        return false;
    }
    let touches = same(p, a) || same(p, b) || same(q, a) || same(q, b);
    if touches {
        // Sharing an end, they meet elsewhere only by running along each other.
        return inside(a, b, p) || inside(a, b, q) || inside(p, q, a) || inside(p, q, b);
    }
    let (d1, d2) = (orient(a, b, p), orient(a, b, q));
    let (d3, d4) = (orient(p, q, a), orient(p, q, b));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return true;
    }
    inside(a, b, p) || inside(a, b, q) || inside(p, q, a) || inside(p, q, b)
}

/// The index of the point of `points` nearest `x` along `k`, a period either way.
fn nearest(points: &[UvPoint], k: usize, x: f64, per: f64) -> usize {
    (0..points.len())
        .min_by(|&i, &j| {
            let d = |p: &UvPoint| (wrap_near(p.c(k), x, per) - x).abs();
            d(&points[i]).total_cmp(&d(&points[j]))
        })
        .expect("a bound has points")
}

/// A winding bound as a path from point `at` once around along +k, ending on `at` again a
/// period on (the same 3D point).
fn around(l: &UvLoop, at: usize, k: usize, per: f64) -> Vec<UvPoint> {
    let m = l.points.len();
    let w = l.winding[k] as f64;
    // In the bound's own order the points after the last continue a period (w) on.
    let mut path: Vec<UvPoint> = (0..=m)
        .map(|d| {
            let i = at + d;
            let mut p = l.points[i % m];
            if i >= m {
                *p.c_mut(k) += w * per;
            }
            p
        })
        .collect();
    if w < 0.0 {
        path.reverse();
    }
    path
}

/// The end of the surface's range across `k` as a path along +k from `near` a period on:
/// two points at a pole (one point in space), or the open edge sampled along it.
fn end_path(
    surface: &Surface,
    options: &MeshOptions,
    k: usize,
    per: f64,
    e: End,
    near: f64,
) -> Vec<UvPoint> {
    let at = |x: f64| if k == 0 { (x, e.at) } else { (e.at, x) };
    let (u, v) = at(near);
    let first = surface.eval(u, v);
    let n = if e.pole {
        1
    } else {
        let b = UvBox {
            u: if k == 0 {
                (near, near + per)
            } else {
                (e.at, e.at)
            },
            v: if k == 0 {
                (e.at, e.at)
            } else {
                (near, near + per)
            },
        };
        let (du, dv) = surface.steps(options, b);
        let step = if k == 0 { du } else { dv };
        step.map_or(8, |s| (per / s).ceil().clamp(8.0, 1e5) as usize)
    };
    (0..=n)
        .map(|i| {
            let x = near + per * i as f64 / n as f64;
            let (u, v) = at(x);
            let pos = if e.pole || i == 0 || i == n {
                first
            } else {
                surface.eval(u, v)
            };
            UvPoint { u, v, pos }
        })
        .collect()
}

/// The points strictly between `from` and `to` along a straight line in parameter space,
/// spaced within the surface's steps.
fn seam(surface: &Surface, options: &MeshOptions, from: UvPoint, to: UvPoint) -> Vec<UvPoint> {
    let b = UvBox {
        u: (from.u.min(to.u), from.u.max(to.u)),
        v: (from.v.min(to.v), from.v.max(to.v)),
    };
    let (du, dv) = surface.steps(options, b);
    let count = |d: f64, step: Option<f64>| step.map_or(1.0, |s| (d.abs() / s).ceil());
    let n = count(to.u - from.u, du)
        .max(count(to.v - from.v, dv))
        .clamp(1.0, 1e5) as usize;
    (1..n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let (u, v) = (from.u + (to.u - from.u) * t, from.v + (to.v - from.v) * t);
            UvPoint {
                u,
                v,
                pos: surface.eval(u, v),
            }
        })
        .collect()
}

/// Moves inner bounds by whole periods so they sit inside the outer bound's parameter range.
fn align_loops(surface: &Surface, loops: &mut [Vec<UvPoint>], outer: usize) {
    let periods = surface.periods();
    let centre = |l: &[UvPoint], k: usize| {
        let (lo, hi) = l
            .iter()
            .map(|p| p.c(k))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| {
                (a.min(x), b.max(x))
            });
        0.5 * (lo + hi)
    };
    let Some(o) = loops.get(outer) else { return };
    let c = [centre(o, 0), centre(o, 1)];
    for (i, l) in loops.iter_mut().enumerate() {
        if i == outer || l.is_empty() {
            continue;
        }
        for (k, per) in [periods.0, periods.1].into_iter().enumerate() {
            let Some(per) = per else { continue };
            let m = centre(l, k);
            let s = wrap_near(m, c[k], per) - m;
            l.iter_mut().for_each(|p| *p.c_mut(k) += s);
        }
    }
}
