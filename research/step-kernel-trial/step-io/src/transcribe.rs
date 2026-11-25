//! Re-emitting a model step-io read through step-io's writer.
//!
//! step-io has no call that copies entities from a read model into a model
//! being written (the read model is read-only and the writer only takes new
//! entities through its constructors), so this walks the read side's scene
//! handles and feeds the builder element by element: vertex, edge, face,
//! solid, colour, part, placement. Analytic surfaces and curves go across as
//! they are; everything else goes through the read side's `to_nurbs()`.

use std::collections::{BTreeMap, HashMap};

use step_io::build::{
    CurveInput, FaceBoundInput, Frame, Part, ProfileInput, StyleTarget, SurfaceInput,
    Vertex as OutVertex, VoidShellNormals,
};
use step_io::model::{
    AdvancedFaceId, Axis1PlacementRef, Axis2Placement3d, Axis2Placement3dRef, Axis2PlacementRef,
    CartesianPointRef, CurveRef, CurveRefView, DirectionRef, EdgeCurveId, EntityKey, VectorRef,
};
use step_io::scene::{
    CurveKind, Edge, Face, NurbsCurve, NurbsSurface, ProductDef, Scene, Shell, Solid, SurfaceKind,
};
use step_io::{Orientation, StepBuilder, StepModel};

use crate::read::{curve_name, surface_name};

/// What crossed, and what did not.
#[derive(Default, Debug)]
pub struct Stats {
    pub parts: usize,
    pub placements: usize,
    pub solids: usize,
    pub solids_failed: usize,
    pub faces: usize,
    pub faces_failed: usize,
    pub surfaces_exact: usize,
    pub surfaces_nurbs: usize,
    pub edges_exact: usize,
    pub edges_nurbs: usize,
    pub solid_colours: usize,
    pub face_colours: usize,
    pub vertex_loops_skipped: usize,
    pub surface_models_skipped: usize,
    pub mapped_items_skipped: usize,
    pub reversed_outer_shells: usize,
    pub missing_transforms: usize,
    /// Failures by cause.
    pub problems: BTreeMap<String, usize>,
}

impl Stats {
    fn problem(&mut self, what: impl Into<String>) {
        *self.problems.entry(what.into()).or_default() += 1;
    }

    pub fn describe(&self) -> String {
        let mut s = format!(
            "{} parts, {} nested placements, {} solids ({} failed), {} faces ({} failed), \
             surfaces {} exact + {} as NURBS, edges {} exact + {} as NURBS, \
             colours on {} solids + {} faces",
            self.parts,
            self.placements,
            self.solids,
            self.solids_failed,
            self.faces,
            self.faces_failed,
            self.surfaces_exact,
            self.surfaces_nurbs,
            self.edges_exact,
            self.edges_nurbs,
            self.solid_colours,
            self.face_colours,
        );
        for (n, what) in [
            (
                self.vertex_loops_skipped,
                "vertex loops dropped (StepBuilder::face takes edge loops only)",
            ),
            (
                self.surface_models_skipped,
                "surface models dropped (StepBuilder has no open-shell call)",
            ),
            (self.mapped_items_skipped, "mapped items not followed"),
            (
                self.reversed_outer_shells,
                "outer shells used reversed, written as they are",
            ),
            (
                self.missing_transforms,
                "occurrences without a transform, placed at the origin",
            ),
        ] {
            if n > 0 {
                s.push_str(&format!(", {n} {what}"));
            }
        }
        if !self.problems.is_empty() {
            let list: Vec<String> = self
                .problems
                .iter()
                .map(|(k, v)| format!("{k} x{v}"))
                .collect();
            s.push_str(&format!("; problems: {}", list.join(", ")));
        }
        s
    }
}

fn flip(o: Orientation) -> Orientation {
    match o {
        Orientation::Forward => Orientation::Reversed,
        Orientation::Reversed => Orientation::Forward,
    }
}

fn xyz(v: &[f64]) -> [f64; 3] {
    [0, 1, 2].map(|i| v.get(i).copied().unwrap_or(0.0))
}

struct Tx<'a, 'm> {
    b: &'a mut StepBuilder,
    model: &'m StepModel,
    /// Source length unit to millimetres, and source angle unit to radians.
    scale: f64,
    angle: f64,
    vertices: HashMap<EntityKey, OutVertex>,
    /// The written edge, and whether it runs against the source edge.
    edges: HashMap<EntityKey, Option<(EdgeCurveId, bool)>>,
    parts: HashMap<EntityKey, Part>,
    done_solids: HashMap<EntityKey, ()>,
    stats: Stats,
}

impl<'m> Tx<'_, 'm> {
    fn point(&self, r: &CartesianPointRef) -> Option<[f64; 3]> {
        let CartesianPointRef::CartesianPoint(id) = r else {
            return None;
        };
        Some(xyz(&self.model[*id].coordinates).map(|c| c * self.scale))
    }

    fn dir(&self, r: &DirectionRef) -> Option<[f64; 3]> {
        let DirectionRef::Direction(id) = r else {
            return None;
        };
        Some(xyz(&self.model[*id].direction_ratios))
    }

    fn frame(&self, a: &Axis2Placement3d) -> Option<Frame> {
        let origin = self.point(&a.location)?;
        let axis = match &a.axis {
            Some(d) => self.dir(d)?,
            None => [0.0, 0.0, 1.0],
        };
        let ref_dir = match &a.ref_direction {
            Some(d) => self.dir(d)?,
            // The standard's default: X, or Y when the axis is along X.
            None if axis[1] == 0.0 && axis[2] == 0.0 => [0.0, 1.0, 0.0],
            None => [1.0, 0.0, 0.0],
        };
        Some(Frame {
            origin,
            axis,
            ref_dir,
        })
    }

    fn frame3(&self, r: &Axis2Placement3dRef) -> Option<Frame> {
        let Axis2Placement3dRef::Axis2Placement3d(id) = r;
        self.frame(&self.model[*id])
    }

    fn frame_any(&self, r: &Axis2PlacementRef) -> Option<Frame> {
        let Axis2PlacementRef::Axis2Placement3d(id) = r else {
            return None;
        };
        self.frame(&self.model[*id])
    }

    /// Direction and length (in millimetres) of a `VECTOR`.
    fn vector(&self, r: &VectorRef) -> Option<([f64; 3], f64)> {
        let VectorRef::Vector(id) = r else {
            return None;
        };
        let v = &self.model[*id];
        Some((self.dir(&v.orientation)?, v.magnitude * self.scale))
    }

    fn scale_curve(&self, mut c: NurbsCurve) -> NurbsCurve {
        for p in &mut c.control_points {
            *p = p.map(|v| v * self.scale);
        }
        c
    }

    fn scale_surface(&self, mut s: NurbsSurface) -> NurbsSurface {
        for p in s.control_points.iter_mut().flatten() {
            *p = p.map(|v| v * self.scale);
        }
        s
    }

    fn vertex(&mut self, v: step_io::scene::Vertex<'m>) -> Option<OutVertex> {
        if let Some(&out) = self.vertices.get(&v.key()) {
            return Some(out);
        }
        let p = v.point()?.xyz().map(|c| c * self.scale);
        let out = self.b.vertex(p).ok()?;
        self.vertices.insert(v.key(), out);
        Some(out)
    }

    fn edge(&mut self, e: Edge<'m>) -> Option<(EdgeCurveId, bool)> {
        if let Some(&done) = self.edges.get(&e.key()) {
            return done;
        }
        let done = self.edge_uncached(e);
        self.edges.insert(e.key(), done);
        done
    }

    fn edge_uncached(&mut self, e: Edge<'m>) -> Option<(EdgeCurveId, bool)> {
        let (Some(start), Some(end)) = (e.start(), e.end()) else {
            self.stats.problem("edge without two vertex points");
            return None;
        };
        let same_point = start.key() == end.key();
        let (Some(v0), Some(v1)) = (self.vertex(start), self.vertex(end)) else {
            self.stats.problem("edge vertex without a cartesian point");
            return None;
        };
        // An edge on a surface curve follows that curve's 3D curve.
        let mut curve = e.curve();
        while let Some(c3) = curve.curve_3d() {
            curve = c3;
        }
        // The builder writes a conic edge running with its curve
        // (`same_sense` true). An edge that runs against its curve is written
        // from its end to its start, and used the other way round in loops.
        let against = !e.same_sense();
        let exact = match curve.kind() {
            CurveKind::Line(_) if !same_point => Some((CurveInput::Line, false)),
            CurveKind::Circle(c) => self
                .frame_any(&c.position)
                .map(|f| (CurveInput::Circle(f, c.radius * self.scale), against)),
            CurveKind::Ellipse(c) => self.frame_any(&c.position).map(|f| {
                let (a, b) = (c.semi_axis_1 * self.scale, c.semi_axis_2 * self.scale);
                (CurveInput::Ellipse(f, a, b), against)
            }),
            _ => None,
        };
        let name = curve_name(&curve.kind());
        let (input, reversed) = match exact {
            Some(x) => {
                self.stats.edges_exact += 1;
                x
            }
            // The edge's own span as a NURBS curve, already start to end.
            None => match e.to_nurbs() {
                Some(n) => {
                    self.stats.edges_nurbs += 1;
                    (CurveInput::Nurbs(self.scale_curve(n)), false)
                }
                None => {
                    self.stats.problem(format!(
                        "edge curve {name}: no exact input and no NURBS form"
                    ));
                    return None;
                }
            },
        };
        let (from, to) = if reversed { (v1, v0) } else { (v0, v1) };
        match self.b.edge(from, to, input) {
            Ok(id) => Some((id, reversed)),
            Err(err) => {
                self.stats.problem(format!("edge curve {name}: {err}"));
                None
            }
        }
    }

    fn profile(&self, r: &CurveRef) -> Option<ProfileInput> {
        match r.resolve(self.model) {
            CurveRefView::Line(l) => {
                let (dir, _) = self.vector(&l.dir)?;
                Some(ProfileInput::Line(self.point(&l.pnt)?, dir))
            }
            CurveRefView::Circle(c) => Some(ProfileInput::Circle(
                self.frame_any(&c.position)?,
                c.radius * self.scale,
            )),
            CurveRefView::Ellipse(c) => Some(ProfileInput::Ellipse(
                self.frame_any(&c.position)?,
                c.semi_axis_1 * self.scale,
                c.semi_axis_2 * self.scale,
            )),
            // The scene's `Curve` handle (and its `to_nurbs`) cannot be made
            // from a `CurveRef` outside the crate; the whole face goes through
            // `Face::to_nurbs` instead.
            _ => None,
        }
    }

    fn surface(&mut self, face: Face<'m>) -> Option<SurfaceInput> {
        let exact = match face.surface().kind() {
            SurfaceKind::Plane(p) => self.frame3(&p.position).map(SurfaceInput::Plane),
            SurfaceKind::Cylindrical(c) => self
                .frame3(&c.position)
                .map(|f| SurfaceInput::Cylinder(f, c.radius * self.scale)),
            SurfaceKind::Conical(c) => self
                .frame3(&c.position)
                .map(|f| SurfaceInput::Cone(f, c.radius * self.scale, c.semi_angle * self.angle)),
            SurfaceKind::Spherical(c) => self
                .frame3(&c.position)
                .map(|f| SurfaceInput::Sphere(f, c.radius * self.scale)),
            SurfaceKind::Toroidal(c) => self.frame3(&c.position).map(|f| {
                SurfaceInput::Torus(f, c.major_radius * self.scale, c.minor_radius * self.scale)
            }),
            SurfaceKind::LinearExtrusion(s) => (|| {
                let profile = self.profile(&s.swept_curve)?;
                let (dir, len) = self.vector(&s.extrusion_axis)?;
                Some(SurfaceInput::LinearExtrusion(profile, dir.map(|c| c * len)))
            })(),
            SurfaceKind::Revolution(s) => (|| {
                let profile = self.profile(&s.swept_curve)?;
                let Axis1PlacementRef::Axis1Placement(id) = &s.axis_position;
                let axis = &self.model[*id];
                let dir = match &axis.axis {
                    Some(d) => self.dir(d)?,
                    None => [0.0, 0.0, 1.0],
                };
                Some(SurfaceInput::Revolution(
                    profile,
                    self.point(&axis.location)?,
                    dir,
                ))
            })(),
            _ => None,
        };
        if exact.is_some() {
            self.stats.surfaces_exact += 1;
            return exact;
        }
        let nurbs = face.to_nurbs()?;
        self.stats.surfaces_nurbs += 1;
        Some(SurfaceInput::Nurbs(self.scale_surface(nurbs)))
    }

    fn face(&mut self, face: Face<'m>) -> Option<AdvancedFaceId> {
        let name = surface_name(&face.surface().kind());
        let mut bounds = Vec::new();
        for bound in face.bounds() {
            if bound.vertex().is_some() {
                self.stats.vertex_loops_skipped += 1;
                continue;
            }
            let mut edges = Vec::new();
            for (edge, along) in bound.oriented_edges() {
                let (id, reversed) = self.edge(edge)?;
                edges.push((id, if reversed { flip(along) } else { along }));
            }
            if edges.is_empty() {
                self.stats
                    .problem("face bound without edges the scene can reach");
                return None;
            }
            bounds.push(FaceBoundInput {
                edges,
                outer: bound.is_outer(),
                orientation: bound.orientation(),
            });
        }
        let Some(surface) = self.surface(face) else {
            self.stats
                .problem(format!("surface {name}: no exact input and no NURBS form"));
            return None;
        };
        match self.b.face(surface, face.orientation(), bounds) {
            Ok(id) => {
                if let Some(c) = face.color() {
                    if self
                        .b
                        .style(StyleTarget::Face(id), c, face.transparency())
                        .is_ok()
                    {
                        self.stats.face_colours += 1;
                    }
                }
                Some(id)
            }
            Err(err) => {
                self.stats.problem(format!("surface {name}: {err}"));
                None
            }
        }
    }

    /// The shell's faces; `None` when none of them crossed.
    fn shell(&mut self, shell: Shell<'m>) -> Option<Vec<AdvancedFaceId>> {
        let mut out = Vec::new();
        for face in shell.faces() {
            match self.face(face) {
                Some(id) => {
                    self.stats.faces += 1;
                    out.push(id);
                }
                None => self.stats.faces_failed += 1,
            }
        }
        (!out.is_empty()).then_some(out)
    }

    fn solid(&mut self, part: Part, solid: Solid<'m>) {
        self.done_solids.insert(solid.key(), ());
        let Some((outer, used)) = solid.outer() else {
            self.stats.solids_failed += 1;
            self.stats.problem("solid without a resolvable outer shell");
            return;
        };
        if used == Orientation::Reversed {
            self.stats.reversed_outer_shells += 1;
        }
        let shell_colour = outer.color();
        let Some(faces) = self.shell(outer) else {
            self.stats.solids_failed += 1;
            return;
        };
        let voids: Vec<_> = solid.voids().collect();
        let colour = solid.color().or(shell_colour);
        let transparency = solid.transparency();
        let target = if voids.is_empty() {
            self.b
                .solid(part, solid.name(), faces)
                .map(StyleTarget::Solid)
        } else {
            // One declaration covers every void; take the first shell's.
            let normals = match voids[0].1 {
                Orientation::Forward => VoidShellNormals::AwayFromMaterial,
                Orientation::Reversed => VoidShellNormals::TowardMaterial,
            };
            let mut groups = Vec::new();
            for (shell, _) in voids {
                if let Some(faces) = self.shell(shell) {
                    groups.push(faces);
                }
            }
            self.b
                .solid_with_voids(part, solid.name(), faces, groups, normals)
                .map(StyleTarget::VoidSolid)
        };
        match target {
            Ok(target) => {
                self.stats.solids += 1;
                if let Some(c) = colour {
                    if self.b.style(target, c, transparency).is_ok() {
                        self.stats.solid_colours += 1;
                    }
                }
            }
            Err(err) => {
                self.stats.solids_failed += 1;
                self.stats.problem(format!("solid: {err}"));
            }
        }
    }

    /// The part for a product definition, with its solids and its children.
    fn definition(&mut self, def: ProductDef<'m>) -> Part {
        if let Some(&part) = self.parts.get(&def.key()) {
            return part;
        }
        let name = def.product().map_or(def.id(), |p| p.name());
        let part = self.b.part(name);
        self.parts.insert(def.key(), part);
        self.stats.parts += 1;
        for solid in def.solids() {
            if !self.done_solids.contains_key(&solid.key()) {
                self.solid(part, solid);
            }
        }
        self.stats.surface_models_skipped += def.surface_models().count();
        for occ in def.occurrences() {
            let Some(child) = occ.definition() else {
                self.stats.problem("occurrence without a definition");
                continue;
            };
            let child_part = self.definition(child);
            // `p' = M p`: column 3 is the origin, column 2 the axis, column 0
            // the reference direction.
            let at = match occ.transform().and_then(|t| t.matrix()) {
                Some(m) => Frame {
                    origin: [m[0][3], m[1][3], m[2][3]].map(|c| c * self.scale),
                    axis: [m[0][2], m[1][2], m[2][2]],
                    ref_dir: [m[0][0], m[1][0], m[2][0]],
                },
                None => {
                    self.stats.missing_transforms += 1;
                    IDENTITY
                }
            };
            match self.b.place(part, child_part, at) {
                Ok(_) => self.stats.placements += 1,
                Err(err) => self.stats.problem(format!("placement: {err}")),
            }
        }
        part
    }
}

pub const IDENTITY: Frame = Frame {
    origin: [0.0, 0.0, 0.0],
    axis: [0.0, 0.0, 1.0],
    ref_dir: [1.0, 0.0, 0.0],
};

/// Write everything `scene` holds into `b` as one part (an assembly when the
/// source is one) and return it. `fallback_name` names the part when the
/// source has several roots or geometry outside any product.
pub fn transcribe(b: &mut StepBuilder, scene: &Scene<'_>, fallback_name: &str) -> (Part, Stats) {
    let units = scene.units();
    let mut tx = Tx {
        b,
        model: scene.model(),
        scale: units.length.map_or(1.0, |u| u.to_si * 1000.0),
        angle: units.angle.map_or(1.0, |u| u.to_si),
        vertices: HashMap::new(),
        edges: HashMap::new(),
        parts: HashMap::new(),
        done_solids: HashMap::new(),
        stats: Stats::default(),
    };
    let roots: Vec<Part> = scene.root_definitions().map(|d| tx.definition(d)).collect();
    let orphans: Vec<_> = scene
        .all_solids()
        .filter(|s| !tx.done_solids.contains_key(&s.key()))
        .collect();
    tx.stats.mapped_items_skipped = scene
        .all_mapped_instances()
        .filter(|mi| mi.solids().next().is_some() || mi.surface_models().next().is_some())
        .count();
    let part = if roots.len() == 1 && orphans.is_empty() {
        roots[0]
    } else {
        let wrapper = tx.b.part(fallback_name);
        tx.stats.parts += 1;
        for root in roots {
            match tx.b.place(wrapper, root, IDENTITY) {
                Ok(_) => tx.stats.placements += 1,
                Err(err) => tx.stats.problem(format!("placement: {err}")),
            }
        }
        for solid in orphans {
            tx.solid(wrapper, solid);
        }
        wrapper
    };
    (part, tx.stats)
}
