//! Meshing a model: every face on a plane, cylinder, cone, sphere or torus tessellated within
//! a chord tolerance into triangles with its colour, grouped by solid and by part, with the
//! assembly tree and its placements.
//!
//! [`StepModel::mesh`] takes a model in millimetres (see [`StepModel::to_millimetres`]);
//! [`load`] reads, converts and meshes a file in one go. A face that cannot be meshed is left
//! out and reported in [`MeshedModel::warnings`]; the rest of the model is still drawn.

mod assembly;
mod curve;
mod face;
mod geometry;
mod style;
mod surface;

use std::collections::HashMap;
use std::ops::Range;

pub use geometry::{BoundingBox, Transform};

use crate::records::*;
use crate::{Protocol, ReadError, StepModel};
use face::EdgeCache;

/// How finely faces are cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshOptions {
    /// The largest distance, in mm, between a curved surface or edge and the chord or
    /// triangle that stands for it.
    pub chord: f64,
    /// The largest angle, in radians, an arc may turn through within one chord.
    pub angle: f64,
}

impl Default for MeshOptions {
    fn default() -> Self {
        MeshOptions {
            chord: 0.01,
            angle: 7.5f64.to_radians(),
        }
    }
}

/// A colour, each channel from 0 to 1; `alpha` is 1 for opaque.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colour {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Colour {
    pub fn rgb(red: f32, green: f32, blue: f32) -> Colour {
        Colour {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }
}

/// The triangles one STEP face became, and the colour it is drawn in.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshFace {
    /// The face's record (an `ADVANCED_FACE` or `FACE_SURFACE`).
    pub face: RecordId,
    /// Its triangles in [`Mesh::triangles`].
    pub triangles: Range<usize>,
    /// The face's own colour, else its shell's, else its solid's.
    pub colour: Option<Colour>,
}

/// Triangles in mm, in the coordinates of the part that holds them. Each face has its own
/// vertices (with that face's normals); faces that share an edge share its vertex positions
/// exactly.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f64; 3]>,
    /// Unit normals, one per position, pointing out of the solid.
    pub normals: Vec<[f64; 3]>,
    /// Counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
    pub faces: Vec<MeshFace>,
}

impl Mesh {
    /// The volume the triangles enclose (positive when they face outwards).
    pub fn volume(&self) -> f64 {
        self.triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| self.positions[i as usize]);
                a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0])
            })
            .sum::<f64>()
            / 6.0
    }

    /// Whether the triangles close up: with vertices joined where their positions are equal,
    /// every edge is used by exactly two triangles, once in each direction.
    pub fn is_closed(&self) -> bool {
        let mut ids: HashMap<[u64; 3], u32> = HashMap::new();
        let weld: Vec<u32> = self
            .positions
            .iter()
            .map(|p| {
                let n = ids.len() as u32;
                *ids.entry(p.map(f64::to_bits)).or_insert(n)
            })
            .collect();
        let mut edges: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
        for t in &self.triangles {
            for k in 0..3 {
                let (a, b) = (weld[t[k] as usize], weld[t[(k + 1) % 3] as usize]);
                let e = edges.entry((a.min(b), a.max(b))).or_default();
                if a < b {
                    e.0 += 1;
                } else {
                    e.1 += 1;
                }
            }
        }
        !self.triangles.is_empty() && edges.values().all(|&(f, b)| f == 1 && b == 1)
    }

    pub fn bounding_box(&self) -> Option<BoundingBox> {
        BoundingBox::of_points(&self.positions)
    }

    fn append(&mut self, face: RecordId, m: face::FaceMesh, colour: Option<Colour>) {
        let base = self.positions.len() as u32;
        let first = self.triangles.len();
        self.positions.extend(m.positions.iter().map(|p| p.array()));
        self.normals.extend(m.normals.iter().map(|n| n.array()));
        self.triangles
            .extend(m.triangles.iter().map(|t| t.map(|i| i + base)));
        self.faces.push(MeshFace {
            face,
            triangles: first..self.triangles.len(),
            colour,
        });
    }

    fn reverse(&mut self) {
        for t in &mut self.triangles {
            t.swap(1, 2);
        }
        for n in &mut self.normals {
            *n = n.map(|x| -x);
        }
    }
}

/// One solid (or one shell of a surface model), meshed.
#[derive(Clone, Debug, PartialEq)]
pub struct Solid {
    pub name: String,
    /// The representation item it was meshed from (a `MANIFOLD_SOLID_BREP`, a
    /// `BREP_WITH_VOIDS`, a shell).
    pub item: RecordId,
    /// The solid's own colour (each face's is in [`MeshFace::colour`]).
    pub colour: Option<Colour>,
    /// Whether its shells are closed ones. A closed solid whose every face meshed gives a
    /// closed mesh.
    pub closed: bool,
    pub mesh: Mesh,
}

/// The shape of one product: its solids, in its own coordinates. Each occurrence of the
/// product in the assembly tree places this one mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    pub solids: Vec<Solid>,
}

/// A node of the assembly tree: a product, placed in its parent.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// The product's name.
    pub name: String,
    /// The name of this use of the product in its parent assembly (`None` at a root).
    pub occurrence: Option<String>,
    /// Where the product sits in its parent's coordinates.
    pub transform: Transform,
    /// The product's own shape, an index into [`MeshedModel::parts`].
    pub part: Option<usize>,
    pub children: Vec<Node>,
}

/// One placed part: a [`Part`] where a path of the assembly tree puts it.
#[derive(Clone, Copy, Debug)]
pub struct Instance<'a> {
    pub part: &'a Part,
    pub index: usize,
    /// From the part's coordinates to the model's.
    pub transform: Transform,
}

/// A meshed model: the parts' meshes and the tree that places them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshedModel {
    pub parts: Vec<Part>,
    pub roots: Vec<Node>,
    /// The faces, solids and placements that could not be drawn.
    pub warnings: Vec<String>,
}

impl MeshedModel {
    /// Every placed part, in tree order.
    pub fn instances(&self) -> Vec<Instance<'_>> {
        fn walk<'a>(m: &'a MeshedModel, n: &Node, parent: &Transform, out: &mut Vec<Instance<'a>>) {
            let t = parent.compose(&n.transform);
            if let Some(i) = n.part {
                out.push(Instance {
                    part: &m.parts[i],
                    index: i,
                    transform: t,
                });
            }
            for c in &n.children {
                walk(m, c, &t, out);
            }
        }
        let mut out = Vec::new();
        for r in &self.roots {
            walk(self, r, &Transform::IDENTITY, &mut out);
        }
        out
    }

    /// The box around every placed triangle, in the model's coordinates (mm); `None` when
    /// nothing was drawn.
    pub fn bounding_box(&self) -> Option<BoundingBox> {
        let mut b: Option<BoundingBox> = None;
        for i in self.instances() {
            for s in &i.part.solids {
                for t in &s.mesh.triangles {
                    for &v in t {
                        let p = i.transform.apply(s.mesh.positions[v as usize]);
                        b = Some(b.map_or(BoundingBox { min: p, max: p }, |b| b.with(p)));
                    }
                }
            }
        }
        b
    }

    /// The volume of every placed solid together.
    pub fn volume(&self) -> f64 {
        self.instances()
            .iter()
            .flat_map(|i| &i.part.solids)
            .map(|s| s.mesh.volume())
            .sum()
    }

    /// Whether nothing at all was drawn.
    pub fn draws_nothing(&self) -> bool {
        self.instances()
            .iter()
            .all(|i| i.part.solids.iter().all(|s| s.mesh.triangles.is_empty()))
    }
}

/// A file read, converted to millimetres and meshed.
#[derive(Clone, Debug)]
pub struct Loaded {
    /// The records, in millimetres and radians.
    pub model: StepModel,
    pub protocol: Protocol,
    pub meshed: MeshedModel,
    /// What reading, converting and meshing had to leave out, in that order.
    pub warnings: Vec<String>,
}

/// Reads a STEP file's bytes (`.step`, `.stp` or `.stpz`), converts it to millimetres and
/// meshes it.
pub fn load(bytes: &[u8], options: &MeshOptions) -> Result<Loaded, ReadError> {
    let read = crate::read(bytes)?;
    let mut model = read.model;
    let mut warnings = read.warnings;
    warnings.extend(model.to_millimetres().warnings);
    let meshed = model.mesh(options);
    warnings.extend(meshed.warnings.iter().cloned());
    Ok(Loaded {
        model,
        protocol: read.protocol,
        meshed,
        warnings,
    })
}

/// At most this many warnings name single faces; the rest are counted.
const MAX_WARNINGS: usize = 50;

impl StepModel {
    /// Meshes every product's solids and builds the assembly tree. Lengths are taken as the
    /// model holds them, so convert it with [`StepModel::to_millimetres`] first.
    pub fn mesh(&self, options: &MeshOptions) -> MeshedModel {
        let mut mesher = Mesher {
            model: self,
            colours: style::colours(self),
            cache: EdgeCache::new(self, options),
            warnings: Vec::new(),
        };
        let (parts, roots) = assembly::build(&mut mesher);
        let mut warnings = mesher.warnings;
        if warnings.len() > MAX_WARNINGS {
            let more = warnings.len() - MAX_WARNINGS;
            warnings.truncate(MAX_WARNINGS);
            warnings.push(format!("and {more} more"));
        }
        MeshedModel {
            parts,
            roots,
            warnings,
        }
    }
}

/// What meshing one model needs: the records, the colours of styled items, the edge
/// samples so far and the warnings.
pub(crate) struct Mesher<'a> {
    model: &'a StepModel,
    colours: HashMap<RecordId, Colour>,
    cache: EdgeCache<'a>,
    warnings: Vec<String>,
}

/// One shell of a solid: the shell record, and whether it is used inside out.
struct ShellUse {
    shell: RecordId,
    reversed: bool,
    void: bool,
}

impl Mesher<'_> {
    /// Whether a representation item is one this mesher draws as a solid.
    fn is_solid_item(&self, item: RecordId) -> bool {
        let m = self.model;
        m.get::<SolidBrep>(item).is_some()
            || m.get::<BrepWithVoids>(item).is_some()
            || m.get::<ShellBasedSurfaceModel>(item).is_some()
            || m.get::<Shell>(item).is_some()
    }

    /// The solids of a representation item: one for a B-rep, one per shell of a surface
    /// model.
    fn solids(&mut self, item: RecordId) -> Vec<Solid> {
        let m = self.model;
        let mut out = Vec::new();
        if let Some(s) = m.get::<SolidBrep>(item) {
            let shell = vec![ShellUse {
                shell: s.outer,
                reversed: false,
                void: false,
            }];
            out.extend(self.solid(item, &s.name, shell));
        } else if let Some(s) = m.get::<BrepWithVoids>(item) {
            let mut shells = vec![ShellUse {
                shell: s.outer,
                reversed: false,
                void: false,
            }];
            for &v in &s.voids {
                let (shell, reversed) = match m.get::<OrientedClosedShell>(v) {
                    Some(o) => (o.closed_shell_element, !o.orientation),
                    None => (v, true),
                };
                shells.push(ShellUse {
                    shell,
                    reversed,
                    void: true,
                });
            }
            out.extend(self.solid(item, &s.name, shells));
        } else if let Some(s) = m.get::<ShellBasedSurfaceModel>(item) {
            for &shell in &s.sbsm_boundary {
                let uses = vec![ShellUse {
                    shell,
                    reversed: false,
                    void: false,
                }];
                out.extend(self.solid(shell, &s.name, uses));
            }
        } else if m.get::<Shell>(item).is_some() {
            let uses = vec![ShellUse {
                shell: item,
                reversed: false,
                void: false,
            }];
            out.extend(self.solid(item, "", uses));
        } else if m.get::<MappedItem>(item).is_none()
            && m.get::<Axis2Placement3d>(item).is_none()
            && m.get::<Axis2Placement2d>(item).is_none()
            && m.get::<Axis1Placement>(item).is_none()
        {
            // Mapped items are placed by the assembly tree, and placements draw nothing; any
            // other item is geometry the mesher does not draw, so it is said, not dropped.
            self.warn(format!(
                "item #{} is a {}, which is not drawn",
                item.0 + 1,
                m.record(item).entity_name()
            ));
        }
        out
    }

    fn solid(&mut self, item: RecordId, name: &str, shells: Vec<ShellUse>) -> Option<Solid> {
        let m = self.model;
        let solid_colour = self.colours.get(&item).copied();
        let mut mesh = Mesh::default();
        let mut closed = true;
        let mut failed = 0;
        for u in &shells {
            let Some(shell) = m.get::<Shell>(u.shell) else {
                self.warn(format!(
                    "shell #{} is a {}, which is not drawn",
                    u.shell.0 + 1,
                    m.record(u.shell).entity_name()
                ));
                closed = false;
                continue;
            };
            closed &= shell.kind == ShellKind::Closed;
            let failed_before = failed;
            let shell_colour = self.colours.get(&u.shell).copied().or(solid_colour);
            let mut shell_mesh = Mesh::default();
            for &f in &shell.cfs_faces {
                match face::mesh_face(&mut self.cache, f, u.reversed) {
                    Ok(fm) => {
                        let colour = self.colours.get(&f).copied().or(shell_colour);
                        shell_mesh.append(f, fm, colour);
                    }
                    Err(e) => {
                        failed += 1;
                        self.warn(e);
                    }
                }
            }
            // A closed shell faces out of its solid, and a void's into it, whatever the
            // file's orientation flags say.
            if shell.kind == ShellKind::Closed && failed == failed_before {
                let v = shell_mesh.volume();
                if (v < 0.0) != u.void {
                    shell_mesh.reverse();
                }
            }
            let base = mesh.positions.len() as u32;
            let first = mesh.triangles.len();
            mesh.positions.extend(shell_mesh.positions);
            mesh.normals.extend(shell_mesh.normals);
            mesh.triangles
                .extend(shell_mesh.triangles.iter().map(|t| t.map(|i| i + base)));
            mesh.faces.extend(shell_mesh.faces.into_iter().map(|mut f| {
                f.triangles = f.triangles.start + first..f.triangles.end + first;
                f
            }));
        }
        if mesh.triangles.is_empty() {
            self.warn(format!(
                "solid #{} '{name}' draws nothing and is skipped",
                item.0 + 1
            ));
            return None;
        }
        Some(Solid {
            name: name.to_string(),
            item,
            colour: solid_colour,
            closed,
            mesh,
        })
    }

    fn warn(&mut self, w: String) {
        self.warnings.push(w);
    }
}
