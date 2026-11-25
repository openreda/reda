//! The self-check: re-tokenizes a written file and verifies its references, the topology and
//! geometry of the PCB shell, and the assembly structure, without a CAD kernel.

use crate::json_string;
use crate::p21::{self, Index, Val};
use std::collections::{HashMap, HashSet};
use std::f64::consts::{PI, TAU};

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

const EPS: f64 = 1e-9;

struct OrientedEdge {
    edge: u64,
    forward: bool,
}

struct Bound {
    outer: bool,
    /// The FACE_BOUND orientation flag; false reverses the loop.
    same: bool,
    edges: Vec<OrientedEdge>,
}

struct Face {
    id: u64,
    surface: u64,
    same_sense: bool,
    bounds: Vec<Bound>,
}

fn rf(a: &[Val], i: usize, what: &str) -> Result<u64, String> {
    a.get(i)
        .and_then(Val::as_ref)
        .ok_or_else(|| format!("missing reference in {what}"))
}

fn first_args(ix: &Index, id: u64) -> Result<(String, Vec<Val>), String> {
    let mut e = ix.get(id)?;
    if e.parts.len() != 1 {
        return Err(format!(
            "#{id} is a complex instance where a simple one was expected"
        ));
    }
    Ok(e.parts.remove(0))
}

fn read_shell(ix: &Index, shell: u64) -> Result<Vec<Face>, String> {
    let (_, a) = first_args(ix, shell)?;
    let mut faces = Vec::new();
    for f in a.get(1).and_then(Val::as_list).unwrap_or(&[]) {
        let id = f.as_ref().ok_or("shell face is not a reference")?;
        let (_, fa) = first_args(ix, id)?;
        let mut bounds = Vec::new();
        for b in fa.get(1).and_then(Val::as_list).unwrap_or(&[]) {
            let bid = b.as_ref().ok_or("face bound is not a reference")?;
            let (bty, ba) = first_args(ix, bid)?;
            let lp = rf(&ba, 1, "face bound")?;
            let (lty, la) = first_args(ix, lp)?;
            let mut edges = Vec::new();
            if lty == "EDGE_LOOP" {
                for oe in la.get(1).and_then(Val::as_list).unwrap_or(&[]) {
                    let oa = ix.args(
                        oe.as_ref().ok_or("loop edge is not a reference")?,
                        "ORIENTED_EDGE",
                    )?;
                    edges.push(OrientedEdge {
                        edge: rf(&oa, 3, "ORIENTED_EDGE")?,
                        forward: oa[4].is_true(),
                    });
                }
            }
            bounds.push(Bound {
                outer: bty == "FACE_OUTER_BOUND",
                same: ba.get(2).is_some_and(Val::is_true),
                edges,
            });
        }
        faces.push(Face {
            id,
            surface: rf(&fa, 2, "face")?,
            same_sense: fa.get(3).is_some_and(Val::is_true),
            bounds,
        });
    }
    Ok(faces)
}

#[derive(Default)]
struct Topology {
    v: usize,
    e: usize,
    f: usize,
    l: usize,
    /// Edges not used exactly once in each direction.
    bad_edges: usize,
    /// Loops whose consecutive edges do not share a vertex.
    open_loops: usize,
}

impl Topology {
    fn closed(&self) -> bool {
        self.bad_edges == 0 && self.open_loops == 0 && self.e > 0
    }
    /// V - E + F - (L - F).
    fn euler(&self) -> i64 {
        self.v as i64 - self.e as i64 + 2 * self.f as i64 - self.l as i64
    }
}

fn topology(ix: &Index, faces: &[Face]) -> Result<Topology, String> {
    let mut uses: HashMap<u64, (u32, u32)> = HashMap::new();
    let mut vertices = HashSet::new();
    let mut t = Topology {
        f: faces.len(),
        ..Default::default()
    };
    for f in faces {
        for b in &f.bounds {
            t.l += 1;
            let mut ends = Vec::new();
            for oe in &b.edges {
                let ea = ix.args(oe.edge, "EDGE_CURVE")?;
                let (v1, v2) = (rf(&ea, 1, "EDGE_CURVE")?, rf(&ea, 2, "EDGE_CURVE")?);
                vertices.insert(v1);
                vertices.insert(v2);
                ends.push(if oe.forward { (v1, v2) } else { (v2, v1) });
                let u = uses.entry(oe.edge).or_default();
                if oe.forward == b.same {
                    u.0 += 1
                } else {
                    u.1 += 1
                }
            }
            let n = ends.len();
            if (0..n).any(|i| ends[i].1 != ends[(i + 1) % n].0) {
                t.open_loops += 1;
            }
        }
    }
    t.v = vertices.len();
    t.e = uses.len();
    t.bad_edges = uses.values().filter(|&&u| u != (1, 1)).count();
    Ok(t)
}

struct Geometry<'a> {
    ix: &'a Index<'a>,
}

impl Geometry<'_> {
    fn triple(&self, id: u64, ty: &str) -> Result<V3, String> {
        let a = self.ix.args(id, ty)?;
        let l = a
            .get(1)
            .and_then(Val::as_list)
            .ok_or_else(|| format!("#{id} has no coordinates"))?;
        let g = |i: usize| {
            l.get(i)
                .and_then(Val::as_f64)
                .ok_or_else(|| format!("#{id} has a bad coordinate"))
        };
        Ok([g(0)?, g(1)?, g(2)?])
    }
    /// Location, axis and reference direction of an AXIS2_PLACEMENT_3D.
    fn placement(&self, id: u64) -> Result<(V3, V3, V3), String> {
        let a = self.ix.args(id, "AXIS2_PLACEMENT_3D")?;
        Ok((
            self.triple(rf(&a, 1, "placement")?, "CARTESIAN_POINT")?,
            self.triple(rf(&a, 2, "placement")?, "DIRECTION")?,
            self.triple(rf(&a, 3, "placement")?, "DIRECTION")?,
        ))
    }
    fn vertex(&self, id: u64) -> Result<V3, String> {
        let a = self.ix.args(id, "VERTEX_POINT")?;
        self.triple(rf(&a, 1, "VERTEX_POINT")?, "CARTESIAN_POINT")
    }
    /// Points along an edge in its own direction, start and end included, and whether it is
    /// circular. Checks that both vertices lie on the curve.
    fn edge_points(&self, edge: u64) -> Result<(Vec<V3>, bool), String> {
        let ea = self.ix.args(edge, "EDGE_CURVE")?;
        let (p1, p2) = (
            self.vertex(rf(&ea, 1, "edge")?)?,
            self.vertex(rf(&ea, 2, "edge")?)?,
        );
        let same = ea[4].is_true();
        let curve = rf(&ea, 3, "edge")?;
        let (ty, ca) = first_args(self.ix, curve)?;
        match ty.as_str() {
            "LINE" => {
                let p = self.triple(rf(&ca, 1, "LINE")?, "CARTESIAN_POINT")?;
                let va = self.ix.args(rf(&ca, 2, "LINE")?, "VECTOR")?;
                let d = self.triple(rf(&va, 1, "VECTOR")?, "DIRECTION")?;
                for q in [p1, p2] {
                    if norm(cross(sub(q, p), d)) > EPS {
                        return Err(format!("edge #{edge}: vertex is off its line"));
                    }
                }
                if (dot(sub(p2, p1), d) > 0.0) != same {
                    return Err(format!("edge #{edge}: direction disagrees with its line"));
                }
                Ok((vec![p1, p2], false))
            }
            "CIRCLE" => {
                let (c, z, x) = self.placement(rf(&ca, 1, "CIRCLE")?)?;
                let r = ca.get(2).and_then(Val::as_f64).ok_or("circle radius")?;
                let y = cross(z, x);
                let angle = |q: V3| -> Result<f64, String> {
                    let d = sub(q, c);
                    if dot(d, z).abs() > EPS || (norm(d) - r).abs() > EPS {
                        return Err(format!("edge #{edge}: vertex is off its circle"));
                    }
                    Ok(dot(d, y).atan2(dot(d, x)))
                };
                let (a1, a2) = (angle(p1)?, angle(p2)?);
                // A circle runs counter-clockwise about its axis; a closed edge is a full turn.
                let mut sweep = (a2 - a1).rem_euclid(TAU);
                if sweep < 1e-12 {
                    sweep = TAU;
                }
                if !same {
                    sweep -= TAU;
                    if sweep > -1e-12 {
                        sweep = -TAU;
                    }
                }
                let n = ((sweep.abs() / TAU) * 2880.0).ceil().max(2.0) as usize;
                let pts = (0..=n)
                    .map(|i| {
                        let a = a1 + sweep * i as f64 / n as f64;
                        add(c, add(scale(x, r * a.cos()), scale(y, r * a.sin())))
                    })
                    .collect();
                Ok((pts, true))
            }
            other => Err(format!("edge #{edge}: unexpected curve type {other}")),
        }
    }
    /// Checks every face of the PCB shell and returns `(top area, volume)`. A planar face's
    /// outer bound must run counter-clockwise about the face normal and its inner bounds
    /// clockwise; on a cylindrical face the material must lie on the left of each circular
    /// edge. The volume is the flux of (0, 0, z + 1), to which only horizontal faces contribute.
    fn check_faces(&self, faces: &[Face]) -> Result<(f64, f64), String> {
        let (mut top_area, mut volume) = (0.0, 0.0);
        for f in faces {
            let (sty, sa) = first_args(self.ix, f.surface)?;
            let (loc, axis, _) = self.placement(rf(&sa, 1, "surface")?)?;
            let sign = if f.same_sense { 1.0 } else { -1.0 };
            let radius = sa.get(2).and_then(Val::as_f64);
            let on_surface = |q: V3| -> bool {
                let d = sub(q, loc);
                match sty.as_str() {
                    "PLANE" => dot(d, axis).abs() < EPS,
                    _ => (norm(cross(d, axis)) - radius.unwrap_or(0.0)).abs() < EPS,
                }
            };
            if sty != "PLANE" && sty != "CYLINDRICAL_SURFACE" {
                return Err(format!("face #{}: unexpected surface type {sty}", f.id));
            }
            if sty == "CYLINDRICAL_SURFACE" && norm(cross(axis, [0.0, 0.0, 1.0])) > EPS {
                return Err(format!("face #{}: cylinder axis is not vertical", f.id));
            }
            let mut face_area = 0.0;
            let mut z_sum = (0.0, 0usize);
            let mut circular = Vec::new(); // (midpoint, travel direction) of each circular edge
            for b in &f.bounds {
                let mut poly: Vec<V3> = Vec::new();
                for oe in &b.edges {
                    let (mut pts, is_circle) = self.edge_points(oe.edge)?;
                    if !oe.forward {
                        pts.reverse();
                    }
                    if is_circle {
                        let m = pts.len() / 2;
                        let dir = sub(pts[m + 1], pts[m - 1]);
                        circular.push((pts[m], if b.same { dir } else { scale(dir, -1.0) }));
                    }
                    for q in [pts[0], pts[pts.len() - 1]] {
                        if !on_surface(q) {
                            return Err(format!("face #{}: a vertex is off the surface", f.id));
                        }
                        z_sum = (z_sum.0 + q[2], z_sum.1 + 1);
                    }
                    pts.pop();
                    poly.extend(pts);
                }
                if sty == "PLANE" {
                    // Newell's formula: the area vector of the closed polyline.
                    let mut av = [0.0; 3];
                    for i in 0..poly.len() {
                        av = add(av, cross(poly[i], poly[(i + 1) % poly.len()]));
                    }
                    let mut area = 0.5 * dot(av, axis) * sign;
                    if !b.same {
                        area = -area;
                    }
                    if b.outer != (area > 0.0) {
                        return Err(format!(
                            "face #{}: a bound runs the wrong way about the face normal",
                            f.id
                        ));
                    }
                    face_area += area;
                }
            }
            if sty == "PLANE" {
                let nz = axis[2] * sign;
                volume += (loc[2] + 1.0) * nz * face_area;
                if nz > 0.5 {
                    top_area += face_area;
                }
            } else {
                let z_mid = z_sum.0 / z_sum.1 as f64;
                for (m, dir) in circular {
                    let d = sub(m, loc);
                    let radial = sub(d, scale(axis, dot(d, axis)));
                    let inward = cross(scale(radial, sign), dir);
                    if inward[2] * (z_mid - m[2]) <= 0.0 {
                        return Err(format!(
                            "face #{}: cylindrical face is oriented inside out",
                            f.id
                        ));
                    }
                }
            }
        }
        Ok((top_area, volume))
    }
}

/// Follows PRODUCT name -> formation -> definition, returning the definition ids.
fn definitions_of_product(file: &p21::StepFile, ix: &Index, name: &str) -> Vec<u64> {
    let third_is = |ty_ok: &dyn Fn(&str) -> bool, targets: &HashSet<u64>| -> HashSet<u64> {
        file.records
            .iter()
            .filter(|r| ty_ok(p21::type_name(&r.body)))
            .filter(|r| {
                p21::parse_entity(&r.body)
                    .ok()
                    .and_then(|e| e.parts[0].1.get(2).and_then(Val::as_ref))
                    .is_some_and(|t| targets.contains(&t))
            })
            .map(|r| r.id)
            .collect()
    };
    let products: HashSet<u64> = file
        .records
        .iter()
        .filter(|r| p21::type_name(&r.body) == "PRODUCT")
        .filter(|r| {
            ix.args(r.id, "PRODUCT")
                .ok()
                .and_then(|a| a.first().and_then(Val::as_str))
                .as_deref()
                == Some(name)
        })
        .map(|r| r.id)
        .collect();
    let formations = third_is(
        &|t| t.starts_with("PRODUCT_DEFINITION_FORMATION"),
        &products,
    );
    let mut v: Vec<u64> = third_is(&|t| t == "PRODUCT_DEFINITION", &formations)
        .into_iter()
        .collect();
    v.sort();
    v
}

/// A JSON number with four decimals, or `null` when the value could not be computed.
fn num(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.4}")
    } else {
        "null".into()
    }
}

pub fn run(path: &str) -> i32 {
    let mut errors: Vec<String> = Vec::new();
    let mut fields: Vec<String> = vec![format!("\"file\":{}", json_string(path))];
    let result = (|| -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("cannot read: {e}"))?;
        let file = p21::parse_file(&bytes)?;
        let ix = Index::new(&file);
        fields.push(format!(
            "\"schema\":{}",
            json_string(&crate::embed::declared_schema(&file))
        ));
        fields.push(format!("\"records\":{}", file.records.len()));

        let duplicates = file.records.len() - ix.by_id.len();
        let mut dangling = 0usize;
        let mut reference_count = 0usize;
        for r in &file.records {
            for n in p21::refs(&r.body) {
                reference_count += 1;
                if !ix.by_id.contains_key(&n) {
                    dangling += 1;
                    if dangling <= 3 {
                        errors.push(format!("#{} refers to undefined #{n}", r.id));
                    }
                }
            }
        }
        if duplicates > 0 {
            errors.push(format!("{duplicates} ids are defined twice"));
        }
        fields.push(format!("\"references\":{reference_count},\"duplicate_ids\":{duplicates},\"dangling_refs\":{dangling}"));

        // Every record must parse as a simple or complex instance.
        let unparsed = file
            .records
            .iter()
            .filter(|r| p21::parse_entity(&r.body).is_err())
            .count();
        if unparsed > 0 {
            errors.push(format!("{unparsed} records do not parse"));
        }
        fields.push(format!("\"unparsed_records\":{unparsed}"));

        // The PCB shell: PRODUCT 'PCB' -> definition -> shape -> representation -> solid -> shell.
        let pcb_defs = definitions_of_product(&file, &ix, "PCB");
        let mut pcb_shell = None;
        for r in &file.records {
            if p21::type_name(&r.body) != "SHAPE_DEFINITION_REPRESENTATION" {
                continue;
            }
            let a = ix.args(r.id, "SHAPE_DEFINITION_REPRESENTATION")?;
            let shape = ix.args(rf(&a, 0, "SDR")?, "PRODUCT_DEFINITION_SHAPE");
            if !shape.is_ok_and(|s| {
                s.get(2)
                    .and_then(Val::as_ref)
                    .is_some_and(|d| pcb_defs.contains(&d))
            }) {
                continue;
            }
            let (_, rep) = first_args(&ix, rf(&a, 1, "SDR")?)?;
            for item in rep
                .get(1)
                .and_then(Val::as_list)
                .unwrap_or(&[])
                .iter()
                .filter_map(Val::as_ref)
            {
                if ix.type_of(item) == "MANIFOLD_SOLID_BREP" {
                    pcb_shell = Some(rf(&ix.args(item, "MANIFOLD_SOLID_BREP")?, 1, "solid")?);
                }
            }
        }
        match pcb_shell {
            None => errors.push("no PCB product with a MANIFOLD_SOLID_BREP found".into()),
            Some(shell) => {
                let faces = read_shell(&ix, shell)?;
                let t = topology(&ix, &faces)?;
                let genus = (2 - t.euler()) / 2;
                if !t.closed() {
                    errors.push(format!(
                        "PCB shell is not a closed oriented manifold: {} edges not used once each way, {} open loops",
                        t.bad_edges, t.open_loops
                    ));
                }
                if t.euler() != 2 * (1 - 5) {
                    errors.push(format!(
                        "PCB shell: V-E+F-(L-F) = {}, expected -8 for genus 5",
                        t.euler()
                    ));
                }
                let area = 50.0 * 30.0 - (4.0 - PI) * 9.0 - 4.0 * PI * 1.6 * 1.6 - (12.0 + PI);
                let expected = area * 1.6;
                let (top_area, volume) = match (Geometry { ix: &ix }).check_faces(&faces) {
                    Ok(v) => v,
                    Err(e) => {
                        errors.push(format!("PCB geometry: {e}"));
                        (f64::NAN, f64::NAN)
                    }
                };
                if !volume.is_nan()
                    && ((volume - expected).abs() > 1e-3 || (top_area - area).abs() > 1e-3)
                {
                    errors.push(format!(
                        "PCB volume {volume:.4} or top area {top_area:.4} differs from expected {expected:.4} / {area:.4}"
                    ));
                }
                fields.push(format!(
                    "\"pcb\":{{\"V\":{},\"E\":{},\"F\":{},\"L\":{},\"euler\":{},\"genus\":{genus},\"edges_not_used_once_each_way\":{},\"open_loops\":{},\"top_area\":{},\"expected_top_area\":{area:.4},\"volume\":{},\"expected_volume\":{expected:.4}}}",
                    t.v, t.e, t.f, t.l, t.euler(), t.bad_edges, t.open_loops, num(top_area), num(volume)
                ));
            }
        }

        // Every CLOSED_SHELL in the file (the component's included): topology only.
        let (mut shells, mut closed, mut unreadable) = (0, 0, 0);
        for r in &file.records {
            if p21::type_name(&r.body) == "CLOSED_SHELL" {
                shells += 1;
                match read_shell(&ix, r.id).and_then(|f| topology(&ix, &f)) {
                    Ok(t) if t.closed() => closed += 1,
                    Ok(_) => {}
                    Err(_) => unreadable += 1,
                }
            }
        }
        fields.push(format!(
            "\"closed_shells\":{shells},\"closed_shells_passing_edge_check\":{closed},\"closed_shells_unreadable\":{unreadable}"
        ));

        // Assembly: occurrences directly under 'trial-board'.
        let asm_defs = definitions_of_product(&file, &ix, "trial-board");
        let mut occurrences: Vec<(String, u64)> = Vec::new();
        let mut all_nauo = 0;
        for r in &file.records {
            if p21::type_name(&r.body) == "NEXT_ASSEMBLY_USAGE_OCCURRENCE" {
                all_nauo += 1;
                let a = ix.args(r.id, "NEXT_ASSEMBLY_USAGE_OCCURRENCE")?;
                if asm_defs.contains(&rf(&a, 3, "NAUO")?) {
                    occurrences.push((a[1].as_str().unwrap_or_default(), rf(&a, 4, "NAUO")?));
                }
            }
        }
        let names: Vec<String> = occurrences.iter().map(|(n, _)| json_string(n)).collect();
        let child = |n: &str| occurrences.iter().find(|(m, _)| m == n).map(|&(_, d)| d);
        let shared = child("U1").is_some() && child("U1") == child("U2");
        if !asm_defs.is_empty() {
            if names != ["\"PCB\"", "\"U1\"", "\"U2\""] {
                errors.push("trial-board does not have exactly the children PCB, U1, U2".into());
            }
            if !shared {
                errors.push("U1 and U2 do not share a product definition".into());
            }
            if child("PCB").is_none_or(|d| !pcb_defs.contains(&d)) {
                errors.push("occurrence PCB does not point at the PCB product".into());
            }
        }
        fields.push(format!(
            "\"assembly\":{},\"occurrences\":[{}],\"u1_u2_share_definition\":{shared},\"occurrences_in_file\":{all_nauo}",
            !asm_defs.is_empty(),
            names.join(",")
        ));
        Ok(())
    })();
    if let Err(e) = result {
        errors.push(e);
    }
    let list: Vec<String> = errors.iter().map(|e| json_string(e)).collect();
    println!(
        "{{\"ok\":{},{},\"errors\":[{}]}}",
        errors.is_empty(),
        fields.join(","),
        list.join(",")
    );
    if errors.is_empty() { 0 } else { 1 }
}
