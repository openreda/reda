//! A face's polygons in parameter space to triangles: constrained Delaunay (`spade`) between
//! the bounds in roughly metric parameters, a grid of points inside on curved surfaces kept
//! clear of the bounds, and the triangles kept by parity (those reached across an odd number
//! of bounds from outside), mapped back onto the surface.

use std::collections::HashMap;

use spade::handles::{FixedFaceHandle, PossiblyOuterTag};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

use super::{FaceMesh, UvPoint};
use crate::mesh::MeshOptions;
use crate::mesh::geometry::V3;
use crate::mesh::surface::{Lost, Surface, UvBox};

/// At most this many grid points go inside one face.
const MAX_GRID: f64 = 40_000.0;
/// Along a straight direction of a curved surface, grid cells are this many times longer
/// than across it.
const STRAIGHT_ASPECT: f64 = 8.0;

pub(super) fn triangulate(
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
    let uv_box = UvBox {
        u: (umin, umax),
        v: (vmin, vmax),
    };
    let (su, sv) = surface.scale(options, uv_box);
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
    // Points closer than this in scaled parameters are one point: an edge that runs along a
    // knot span of almost no length, or two vertices a rounding apart.
    let merge = 1e-9 * ((umax - umin) * su + (vmax - vmin) * sv);
    for l in loops {
        let mut kept: Vec<UvPoint> = Vec::with_capacity(l.len());
        for p in l {
            let near = |q: &UvPoint| ((p.u - q.u) * su).hypot((p.v - q.v) * sv) <= merge;
            if !kept.last().is_some_and(near) {
                kept.push(*p);
            }
        }
        while kept.len() > 1
            && ((kept[0].u - kept[kept.len() - 1].u) * su)
                .hypot((kept[0].v - kept[kept.len() - 1].v) * sv)
                <= merge
        {
            kept.pop();
        }
        let handles: Vec<_> = kept
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
    let (du, dv) = surface.steps(options, uv_box);
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
            if let Some(lost) = surface.lost(p) {
                // A cone's apex has no normal of its own: each triangle there takes the one
                // along its own direction around the axis (or across a collapsed row).
                let others = corners.iter().filter(|c| c.0 != h);
                let n = match lost {
                    Lost::U => surface.normal(others.map(|c| c.1.0).sum::<f64>() / 2.0, v),
                    Lost::V => surface.normal(u, others.map(|c| c.1.1).sum::<f64>() / 2.0),
                };
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

/// All of a closed surface (a sphere, a torus, or a freeform surface that closes on itself
/// or at poles), as a grid that wraps around and closes at its poles.
pub(super) fn whole(surface: &Surface, options: &MeshOptions, flip: bool) -> Option<FaceMesh> {
    let w = surface.whole()?;
    let (du, dv) = surface.steps(options, w.range);
    let count = |(a, b): (f64, f64), d: Option<f64>, min: f64| {
        d.map_or(8.0, |d| ((b - a) / d).ceil()).clamp(min, 4096.0) as usize
    };
    let nu = count(w.range.u, du, if w.wrap[0] { 3.0 } else { 2.0 });
    let nv = count(w.range.v, dv, if w.wrap[1] { 3.0 } else { 2.0 });
    let n = [nu, nv];
    // Each grid point's vertex: a wrapped index folds back to 0, and a collapsed boundary is
    // one vertex.
    let key = |i: usize, j: usize| -> (usize, usize) {
        let i = if w.wrap[0] { i % nu } else { i };
        let j = if w.wrap[1] { j % nv } else { j };
        let at_end = |x: usize, k: usize| match (x == 0, x == n[k]) {
            (true, _) => Some(0),
            (_, true) => Some(1),
            _ => None,
        };
        if let Some(e) = at_end(i, 0).filter(|&e| w.pole[0][e]) {
            return (usize::MAX - e, 0);
        }
        if let Some(e) = at_end(j, 1).filter(|&e| w.pole[1][e]) {
            return (0, usize::MAX - e);
        }
        (i, j)
    };
    let mut mesh = FaceMesh::default();
    let mut ids: HashMap<(usize, usize), u32> = HashMap::new();
    let mut vertex = |mesh: &mut FaceMesh, i: usize, j: usize| {
        *ids.entry(key(i, j)).or_insert_with(|| {
            let u = w.range.u.0 + (w.range.u.1 - w.range.u.0) * i as f64 / nu as f64;
            let v = w.range.v.0 + (w.range.v.1 - w.range.v.0) * j as f64 / nv as f64;
            mesh.positions.push(surface.eval(u, v));
            let nrm = surface.normal(u, v);
            mesh.normals.push(if flip { -nrm } else { nrm });
            (mesh.positions.len() - 1) as u32
        })
    };
    for i in 0..nu {
        for j in 0..nv {
            // The quad (i, j) (i + 1, j) (i + 1, j + 1) (i, j + 1), counter-clockwise in (u, v).
            let p00 = vertex(&mut mesh, i, j);
            let p10 = vertex(&mut mesh, i + 1, j);
            let p11 = vertex(&mut mesh, i + 1, j + 1);
            let p01 = vertex(&mut mesh, i, j + 1);
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
