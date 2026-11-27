//! One face to triangles: its bounds mapped into the surface's parameter space, triangulated
//! there with constrained Delaunay (`spade`) between the bounds, and mapped back onto the
//! surface.
//!
//! - Boundary points keep the 3D positions of their edges' samples, so faces that share an
//!   edge share its points exactly and the mesh closes.
//! - On a periodic surface the parameter of each edge's run is unwrapped along the run, and
//!   the runs are joined end to end: a seam edge used twice lands on both sides of the
//!   parameter rectangle. Where two runs meet at a pole or a cone's apex (where `u` means
//!   nothing) the join is made from the other end, and the pole point is split into one
//!   point per side so the triangulation sees the pole as a segment.
//! - Inside the bounds, curved surfaces get a grid of points at the steps the chord tolerance
//!   allows, kept clear of the bounds.
//! - Triangles are kept by parity: those reached across an odd number of bounds from outside.

use std::collections::HashMap;
use std::f64::consts::TAU;

use spade::handles::{FixedFaceHandle, PossiblyOuterTag};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

use super::MeshOptions;
use super::geometry::V3;
use super::surface::Surface;
use crate::StepModel;
use crate::records::{
    EdgeLoop, Face, FaceBound, FaceBoundKind, OrientedEdge, RecordId, VertexLoop,
};

/// A face's triangles: positions, normals and triangles over them, facing outwards.
#[derive(Default)]
pub(crate) struct FaceMesh {
    pub positions: Vec<V3>,
    pub normals: Vec<V3>,
    pub triangles: Vec<[u32; 3]>,
}

/// Samples each edge once for every face that uses it.
pub(crate) struct EdgeCache<'a> {
    model: &'a StepModel,
    options: &'a MeshOptions,
    edges: HashMap<RecordId, Result<Vec<V3>, String>>,
}

impl<'a> EdgeCache<'a> {
    pub fn new(model: &'a StepModel, options: &'a MeshOptions) -> Self {
        EdgeCache {
            model,
            options,
            edges: HashMap::new(),
        }
    }

    fn edge(&mut self, id: RecordId) -> Result<&[V3], String> {
        let (model, options) = (self.model, self.options);
        self.edges
            .entry(id)
            .or_insert_with(|| super::curve::sample_edge(model, id, options))
            .as_deref()
            .map_err(|e| e.clone())
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
#[derive(Clone, Copy)]
struct UvPoint {
    u: f64,
    v: f64,
    pos: V3,
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
        return whole(&surface, cache.options, flip)
            .ok_or_else(|| "it has no edges and its surface is not closed".to_string());
    }

    let mut loops: Vec<Vec<UvPoint>> = Vec::new();
    let mut outer = 0;
    for b in &bounds {
        if b.outer {
            outer = loops.len();
        }
        loops.push(to_uv(&surface, b)?);
    }
    align_loops(&surface, &mut loops, outer);
    triangulate(&surface, cache.options, &loops, flip)
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

/// The value of `x + 2πk` nearest `near`.
fn wrap_near(x: f64, near: f64) -> f64 {
    x + ((near - x) / TAU).round() * TAU
}

/// One run's points in parameter space, unwrapped along the run. `None` for a point at a
/// singularity, whose `u` is set later.
struct UvRun {
    u: Vec<Option<f64>>,
    v: Vec<f64>,
    pos: Vec<V3>,
    edge: RecordId,
}

impl UvRun {
    fn first_u(&self) -> Option<f64> {
        self.u.iter().flatten().next().copied()
    }
    fn last_u(&self) -> Option<f64> {
        self.u.iter().rev().flatten().next().copied()
    }
}

/// Maps a bound into parameter space as one closed polygon (the last point not repeated).
fn to_uv(surface: &Surface, bound: &Bound3) -> Result<Vec<UvPoint>, String> {
    let (pu, pv) = surface.periodic();
    let mut runs: Vec<UvRun> = bound
        .runs
        .iter()
        .map(|run| {
            let mut u = Vec::with_capacity(run.points.len());
            let mut v = Vec::with_capacity(run.points.len());
            let mut last_u: Option<f64> = None;
            let mut last_v: Option<f64> = None;
            for &p in &run.points {
                let (mut pu_, mut pv_) = surface.project(p);
                if pv {
                    if let Some(l) = last_v {
                        pv_ = wrap_near(pv_, l);
                    }
                    last_v = Some(pv_);
                }
                if surface.is_singular(p) {
                    u.push(None);
                } else {
                    if let (true, Some(l)) = (pu, last_u) {
                        pu_ = wrap_near(pu_, l);
                    }
                    last_u = Some(pu_);
                    u.push(Some(pu_));
                }
                v.push(pv_);
            }
            UvRun {
                u,
                v,
                pos: run.points.clone(),
                edge: run.edge,
            }
        })
        .collect();

    let n = runs.len();
    // Shift whole runs by multiples of 2π so that each starts where the one before ends.
    if pv {
        for i in 1..n {
            let end = *runs[i - 1].v.last().expect("runs have points");
            let start = runs[i].v[0];
            let shift = wrap_near(start, end) - start;
            runs[i].v.iter_mut().for_each(|v| *v += shift);
        }
    }
    if pu {
        let mut shift: Vec<Option<f64>> = vec![None; n];
        let seed = (0..n).find(|&i| runs[i].first_u().is_some());
        let Some(seed) = seed else {
            return Err("a bound that lies wholly on a pole".into());
        };
        shift[seed] = Some(0.0);
        // A join can be made where the runs meet at a point with a meaningful u.
        let joinable = |a: &UvRun, b: &UvRun| -> Option<(f64, f64)> {
            match (
                a.u.last().copied().flatten(),
                b.u.first().copied().flatten(),
            ) {
                (Some(end), Some(start)) => Some((end, start)),
                _ => None,
            }
        };
        loop {
            let mut changed = false;
            for k in 0..n {
                let (i, j) = (k, (k + 1) % n);
                if let Some((end, start)) = joinable(&runs[i], &runs[j]) {
                    match (shift[i], shift[j]) {
                        (Some(si), None) => {
                            shift[j] = Some(wrap_near(start, end + si) - start);
                            changed = true;
                        }
                        (None, Some(sj)) => {
                            shift[i] = Some(wrap_near(end, start + sj) - end);
                            changed = true;
                        }
                        _ => {}
                    }
                }
            }
            if !changed {
                break;
            }
        }
        // Runs joined to the rest only through poles: a seam's second use goes 2π from its
        // first; anything else stays where it is nearest the rest of the bound.
        for j in 0..n {
            if shift[j].is_some() || runs[j].first_u().is_none() {
                continue;
            }
            let u0 = runs[j].first_u().expect("checked");
            let partner =
                (0..n).find(|&k| k != j && shift[k].is_some() && runs[k].edge == runs[j].edge);
            let resolved: Vec<f64> = (0..n)
                .filter_map(|k| Some(runs[k].first_u()? + shift[k]?))
                .collect();
            let mean = resolved.iter().sum::<f64>() / resolved.len() as f64;
            shift[j] = Some(match partner {
                Some(k) => {
                    let at = runs[k].last_u().expect("resolved runs have u")
                        + shift[k].expect("resolved");
                    let same = wrap_near(u0, at) - u0;
                    let others: Vec<f64> = (0..n)
                        .filter(|&m| m != k && m != j)
                        .filter_map(|m| Some(runs[m].first_u()? + shift[m]?))
                        .collect();
                    // Towards the rest of the bound; for a bound of nothing but the seam,
                    // either side is the same surface.
                    let rest = others.iter().sum::<f64>() / others.len() as f64;
                    let toward = if others.is_empty() || rest >= at {
                        1.0
                    } else {
                        -1.0
                    };
                    same + toward * TAU
                }
                None => wrap_near(u0, mean) - u0,
            });
        }
        // A bound that runs once around the axis does not close in u: that is a band face
        // without a seam edge, which is not meshed here.
        for k in 0..n {
            let (i, j) = (k, (k + 1) % n);
            if let (Some((end, start)), Some(si), Some(sj)) =
                (joinable(&runs[i], &runs[j]), shift[i], shift[j])
                && ((end + si) - (start + sj)).abs() > 1e-6
            {
                return Err("a bound runs around the surface's axis without a seam".into());
            }
        }
        for (r, s) in runs.iter_mut().zip(&shift) {
            if let Some(s) = s {
                r.u.iter_mut().flatten().for_each(|u| *u += s);
            }
        }
    }
    if pv {
        let first = runs[0].v[0];
        let last = *runs[n - 1].v.last().expect("runs have points");
        if (first - last).abs() > 1e-6 {
            return Err("a bound runs around the surface's tube without a seam".into());
        }
    }

    // One closed polygon: each run's first point is the one before's last.
    let mut pts: Vec<(Option<f64>, f64, V3)> = Vec::new();
    for (i, r) in runs.iter().enumerate() {
        let skip = usize::from(i > 0);
        for k in skip..r.pos.len() {
            pts.push((r.u[k], r.v[k], r.pos[k]));
        }
    }
    pts.pop();
    if pts.len() < 2 {
        return Ok(Vec::new());
    }
    if !pu {
        return Ok(pts
            .into_iter()
            .map(|(u, v, pos)| UvPoint {
                u: u.expect("no singular points off surfaces of revolution"),
                v,
                pos,
            })
            .collect());
    }
    // A pole becomes one point on each side: at the u of the point before and of the point
    // after.
    let m = pts.len();
    let mut out = Vec::with_capacity(m + 4);
    for i in 0..m {
        let (u, v, pos) = pts[i];
        match u {
            Some(u) => out.push(UvPoint { u, v, pos }),
            None => {
                let before = (1..m).find_map(|d| pts[(i + m - d) % m].0);
                let after = (1..m).find_map(|d| pts[(i + d) % m].0);
                let (Some(before), Some(after)) = (before, after) else {
                    return Err("a bound that lies wholly on a pole".into());
                };
                out.push(UvPoint { u: before, v, pos });
                if (after - before).abs() > 1e-12 {
                    out.push(UvPoint { u: after, v, pos });
                }
            }
        }
    }
    Ok(out)
}

/// Moves inner bounds by whole turns so they sit inside the outer bound's parameter range.
fn align_loops(surface: &Surface, loops: &mut [Vec<UvPoint>], outer: usize) {
    let (pu, pv) = surface.periodic();
    let centre = |l: &[UvPoint], f: fn(&UvPoint) -> f64| {
        let (lo, hi) = l
            .iter()
            .map(f)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| {
                (a.min(x), b.max(x))
            });
        0.5 * (lo + hi)
    };
    let Some(o) = loops.get(outer) else { return };
    let (cu, cv) = (centre(o, |p| p.u), centre(o, |p| p.v));
    for (i, l) in loops.iter_mut().enumerate() {
        if i == outer || l.is_empty() {
            continue;
        }
        if pu {
            let c = centre(l, |p| p.u);
            let s = wrap_near(c, cu) - c;
            l.iter_mut().for_each(|p| p.u += s);
        }
        if pv {
            let c = centre(l, |p| p.v);
            let s = wrap_near(c, cv) - c;
            l.iter_mut().for_each(|p| p.v += s);
        }
    }
}

/// At most this many grid points go inside one face.
const MAX_GRID: f64 = 40_000.0;
/// Along a straight direction of a curved surface, grid cells are this many times longer
/// than across it.
const STRAIGHT_ASPECT: f64 = 8.0;

fn triangulate(
    surface: &Surface,
    options: &MeshOptions,
    loops: &[Vec<UvPoint>],
    flip: bool,
) -> Result<FaceMesh, String> {
    let all = || loops.iter().flatten();
    let (umin, umax, vmin, vmax) = all().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(a, b, c, d), p| (a.min(p.u), b.max(p.u), c.min(p.v), d.max(p.v)),
    );
    if !(umin.is_finite() && vmin.is_finite()) {
        return Err("a face with no boundary points".into());
    }
    let (su, sv) = surface.scale((vmin, vmax));
    let to2 = |u: f64, v: f64| spade::mitigate_underflow(Point2::new(u * su, v * sv));

    let mut cdt: ConstrainedDelaunayTriangulation<Point2<f64>> =
        ConstrainedDelaunayTriangulation::new();
    // Vertex handle index -> (u, v, position, on the boundary).
    let mut verts: Vec<Option<(f64, f64, V3)>> = Vec::new();
    let put = |cdt: &mut ConstrainedDelaunayTriangulation<Point2<f64>>,
               verts: &mut Vec<Option<(f64, f64, V3)>>,
               u: f64,
               v: f64,
               pos: V3| {
        let h = cdt
            .insert(to2(u, v))
            .map_err(|e| format!("a point the triangulation refused: {e:?}"))?;
        let i = h.index();
        if verts.len() <= i {
            verts.resize(i + 1, None);
        }
        if verts[i].is_none() {
            verts[i] = Some((u, v, pos));
        }
        Ok::<_, String>(h)
    };
    let mut segments: Vec<Segment> = Vec::new();
    let mut conflicts = 0;
    for l in loops {
        let handles: Vec<_> = l
            .iter()
            .map(|p| put(&mut cdt, &mut verts, p.u, p.v, p.pos))
            .collect::<Result<_, _>>()?;
        for i in 0..handles.len() {
            let (a, b) = (handles[i], handles[(i + 1) % handles.len()]);
            if a == b || cdt.exists_constraint(a, b) {
                continue;
            }
            if cdt.try_add_constraint(a, b).is_empty() {
                conflicts += 1;
            }
            segments.push((cdt.vertex(a).position(), cdt.vertex(b).position()));
        }
    }
    if conflicts > 0 {
        return Err(format!("its bounds cross themselves ({conflicts} times)"));
    }

    // Grid points inside, for curved surfaces.
    let (du, dv) = surface.steps(options, (vmin, vmax));
    if du.is_some() || dv.is_some() {
        let (w, h) = ((umax - umin) * su, (vmax - vmin) * sv);
        let mut cu = du.map_or(f64::INFINITY, |d| d * su);
        let mut cv = dv.map_or(f64::INFINITY, |d| d * sv);
        if cu.is_infinite() {
            cu = cv * STRAIGHT_ASPECT;
        }
        if cv.is_infinite() {
            cv = cu * STRAIGHT_ASPECT;
        }
        let mut nu = (w / cu).ceil().max(1.0);
        let mut nv = (h / cv).ceil().max(1.0);
        if nu * nv > MAX_GRID {
            let k = (nu * nv / MAX_GRID).sqrt();
            nu = (nu / k).ceil();
            nv = (nv / k).ceil();
        }
        let (cu, cv) = (w / nu, h / nv);
        if nu * nv > 1.0 && cu > 0.0 && cv > 0.0 {
            let clear = 0.3 * cu.min(cv);
            let near = SegmentGrid::new(&segments, cu, cv, umin * su, vmin * sv);
            for i in 0..nu as usize {
                for j in 0..nv as usize {
                    let x = umin * su + (i as f64 + 0.5) * cu;
                    let y = vmin * sv + (j as f64 + 0.5) * cv;
                    if near.within(x, y, clear) {
                        continue;
                    }
                    let (u, v) = (x / su, y / sv);
                    put(&mut cdt, &mut verts, u, v, surface.eval(u, v))?;
                }
            }
        }
    }

    // Parity of each face: how many bounds lie between it and the outside.
    let mut inside = vec![None::<bool>; cdt.num_all_faces()];
    let outer = cdt.outer_face().fix();
    inside[outer.index()] = Some(false);
    let mut stack: Vec<FixedFaceHandle<PossiblyOuterTag>> = vec![outer];
    while let Some(f) = stack.pop() {
        let parity = inside[f.index()].expect("set before pushing");
        let face = cdt.face(f);
        let mut edges = Vec::new();
        if let Some(inner) = face.as_inner() {
            edges.extend(inner.adjacent_edges());
        } else if let Some(first) = face.adjacent_edge() {
            let mut e = first;
            loop {
                edges.push(e);
                e = e.next();
                if e == first || edges.len() > cdt.num_directed_edges() {
                    break;
                }
            }
        }
        for e in edges {
            let other = e.rev().face().fix();
            if inside[other.index()].is_none() {
                inside[other.index()] = Some(parity ^ e.is_constraint_edge());
                stack.push(other);
            }
        }
    }

    let mut mesh = FaceMesh::default();
    let mut index: HashMap<usize, u32> = HashMap::new();
    for f in cdt.inner_faces() {
        if inside[f.fix().index()] != Some(true) {
            continue;
        }
        let vs = f.vertices();
        let corners = vs.map(|vh| {
            let h = vh.fix().index();
            (h, verts[h].expect("every vertex was put"))
        });
        let mut tri = [0u32; 3];
        let mut pos = [V3::default(); 3];
        for (k, &(h, (u, v, p))) in corners.iter().enumerate() {
            pos[k] = p;
            if surface.is_singular(p) {
                // A cone's apex has no normal of its own: each triangle there takes the one
                // along its own direction around the axis.
                let others = corners.iter().filter(|c| c.0 != h).map(|c| c.1.0);
                let u_mid = others.sum::<f64>() / 2.0;
                let n = surface.normal(u_mid, v);
                mesh.positions.push(p);
                mesh.normals.push(if flip { -n } else { n });
                tri[k] = (mesh.positions.len() - 1) as u32;
                continue;
            }
            tri[k] = *index.entry(h).or_insert_with(|| {
                mesh.positions.push(p);
                let n = surface.normal(u, v);
                mesh.normals.push(if flip { -n } else { n });
                (mesh.positions.len() - 1) as u32
            });
        }
        if pos[0] == pos[1] || pos[1] == pos[2] || pos[2] == pos[0] {
            continue; // a sliver at a pole
        }
        if flip {
            tri.swap(1, 2);
        }
        mesh.triangles.push(tri);
    }
    Ok(mesh)
}

type Segment = (Point2<f64>, Point2<f64>);

/// Boundary segments bucketed by grid cell, to keep grid points clear of the bounds.
struct SegmentGrid {
    cells: HashMap<(i64, i64), Vec<Segment>>,
    cu: f64,
    cv: f64,
    x0: f64,
    y0: f64,
}

impl SegmentGrid {
    fn new(segments: &[Segment], cu: f64, cv: f64, x0: f64, y0: f64) -> Self {
        let mut g = SegmentGrid {
            cells: HashMap::new(),
            cu,
            cv,
            x0,
            y0,
        };
        for &(a, b) in segments {
            let (i0, j0) = g.cell(a.x.min(b.x), a.y.min(b.y));
            let (i1, j1) = g.cell(a.x.max(b.x), a.y.max(b.y));
            if (i1 - i0 + 1) * (j1 - j0 + 1) > 4096 {
                // A long segment: walk it instead of its box.
                let steps =
                    (((b.x - a.x) / cu).abs() + ((b.y - a.y) / cv).abs()).ceil() as usize + 1;
                for s in 0..=steps {
                    let t = s as f64 / steps as f64;
                    let c = g.cell(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                    g.cells.entry(c).or_default().push((a, b));
                }
                continue;
            }
            for i in i0..=i1 {
                for j in j0..=j1 {
                    g.cells.entry((i, j)).or_default().push((a, b));
                }
            }
        }
        g
    }

    fn cell(&self, x: f64, y: f64) -> (i64, i64) {
        (
            ((x - self.x0) / self.cu).floor() as i64,
            ((y - self.y0) / self.cv).floor() as i64,
        )
    }

    /// Whether a segment passes within `d` of `(x, y)` (`d` below a cell's size).
    fn within(&self, x: f64, y: f64, d: f64) -> bool {
        let (ci, cj) = self.cell(x, y);
        for i in ci - 1..=ci + 1 {
            for j in cj - 1..=cj + 1 {
                if let Some(segs) = self.cells.get(&(i, j))
                    && segs.iter().any(|&(a, b)| distance(x, y, a, b) < d)
                {
                    return true;
                }
            }
        }
        false
    }
}

fn distance(x: f64, y: f64, a: Point2<f64>, b: Point2<f64>) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((x - a.x) * dx + (y - a.y) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (x - a.x - t * dx).hypot(y - a.y - t * dy)
}

/// All of a sphere or a torus, as a grid that wraps around and closes at the poles.
fn whole(surface: &Surface, options: &MeshOptions, flip: bool) -> Option<FaceMesh> {
    let ((u0, u1), (v0, v1)) = surface.whole()?;
    let (du, dv) = surface.steps(options, (v0, v1));
    let nu = ((u1 - u0) / du?).ceil().max(3.0) as usize;
    let nv = ((v1 - v0) / dv?).ceil().max(2.0) as usize;
    let sphere = matches!(surface, Surface::Sphere { .. });
    let mut mesh = FaceMesh::default();
    let push = |mesh: &mut FaceMesh, u: f64, v: f64| {
        mesh.positions.push(surface.eval(u, v));
        let n = surface.normal(u, v);
        mesh.normals.push(if flip { -n } else { n });
        (mesh.positions.len() - 1) as u32
    };
    // Rows of v; each row's vertices around u. A sphere's first and last rows are its poles.
    let mut rows: Vec<Vec<u32>> = Vec::new();
    let row_count = if sphere { nv + 1 } else { nv };
    for j in 0..row_count {
        let v = v0 + (v1 - v0) * j as f64 / nv as f64;
        if sphere && (j == 0 || j == nv) {
            rows.push(vec![push(&mut mesh, 0.0, v)]);
        } else {
            rows.push(
                (0..nu)
                    .map(|i| push(&mut mesh, u0 + (u1 - u0) * i as f64 / nu as f64, v))
                    .collect(),
            );
        }
    }
    let at = |row: &Vec<u32>, i: usize| if row.len() == 1 { row[0] } else { row[i % nu] };
    for j in 0..nv {
        let (a, b) = (&rows[j], &rows[(j + 1) % rows.len()]);
        for i in 0..nu {
            // The quad (i, j) (i + 1, j) (i + 1, j + 1) (i, j + 1), counter-clockwise in (u, v).
            let (p00, p10, p11, p01) = (at(a, i), at(a, i + 1), at(b, i + 1), at(b, i));
            for mut t in [[p00, p10, p11], [p00, p11, p01]] {
                if t[0] == t[1] || t[1] == t[2] || t[2] == t[0] {
                    continue;
                }
                if flip {
                    t.swap(1, 2);
                }
                mesh.triangles.push(t);
            }
        }
    }
    Some(mesh)
}
