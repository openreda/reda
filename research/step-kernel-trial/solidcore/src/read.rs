//! `read`: Part 21 text to solidcore's records (the exchange structure, the product structure
//! with its colours and placements, one B-rep record per solid), then one display mesh per
//! solid, placed once per assembly instance.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Instant;

use solidcore::geom::step_matrix::{IDENTITY, Matrix4};
use solidcore_ops::pcurve::{EdgeUse, TrimFace, Trims, pcurves_within};
use solidcore_ops::step_assembly::placed_bodies;
use solidcore_record::brep::Curve2;
use solidcore_record::step::product::{self, Colour, ProductStructure};
use solidcore_record::step::protocol;
use solidcore_step::p21::{self, Exchange};
use solidcore_step::topology::{self, Solid, Solids};
use solidcore_tess::{MeshTolerance, plan};

use crate::json;

/// Chordal deviation, mm (SPEC.md).
const CHORD: f64 = 0.01;
/// Vertices closer than this are one, mm (SPEC.md).
const WELD: f64 = 1e-4;

/// The pcurves the STEP reader has no closed form for, made by the kernel's own fit
/// (`solidcore_ops::pcurve::pcurves_within`). solidcore's crates define the [`Trims`] trait but
/// ship no implementation of it outside their tests; SolidCraft's is in its application crate
/// `solidcraft-smlib`. This one is the harness's: every use of the face is handed to the fit
/// with the pcurves the reader already holds.
#[cfg_attr(feature = "smlib-trims", allow(dead_code))]
struct KernelTrims;

impl Trims for KernelTrims {
    fn trims(&self, face: &TrimFace<'_>) -> Result<Vec<Vec<Curve2>>, String> {
        let surface = face.nurbs.unwrap_or(face.surface);
        let mut loops = Vec::with_capacity(face.loops.len());
        for l in &face.loops {
            let mut uses = Vec::with_capacity(l.len());
            for u in l {
                let e = face.edges.get(u.edge as usize).ok_or("a use of an edge the face does not list")?;
                uses.push(EdgeUse {
                    edge: u.edge,
                    curve: e.nurbs.unwrap_or(e.curve),
                    interval: e.interval,
                    forward: u.forward,
                    tolerance: e.tolerance,
                    given: u.pcurve.as_ref(),
                });
            }
            loops.push(uses);
        }
        pcurves_within(surface, &loops, face.tolerance, Some(face.domain)).map_err(|e| format!("kernel pcurves: {e}"))
    }
}

#[cfg(feature = "smlib-trims")]
const TRIMS: &str = "solidcraft-smlib's SmlibImportTrims";
#[cfg(feature = "smlib-trims")]
fn trims() -> &'static dyn Trims {
    &solidcraft_smlib::pcurves::SmlibImportTrims
}
#[cfg(not(feature = "smlib-trims"))]
const TRIMS: &str = "the harness's own Trims over solidcore_ops::pcurve::pcurves_within";
#[cfg(not(feature = "smlib-trims"))]
fn trims() -> &'static dyn Trims {
    &KernelTrims
}

/// Most reads of one refused solid.
const MAX_ROUNDS: usize = 8;

/// Every "face #id" a refusal names.
fn named_faces(message: &str) -> Vec<u64> {
    message
        .match_indices("face #")
        .filter_map(|(at, m)| {
            let rest = message.get(at + m.len()..)?;
            let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            rest.get(..digits)?.parse().ok()
        })
        .collect()
}

/// The refused solid read without the faces its refusals name, and how many those are.
fn without_named_faces(ex: &Exchange, structure: Option<&ProductStructure>, failure: &topology::Failure) -> Option<(Solid, usize)> {
    let context = structure
        .and_then(|s| s.representations.iter().find(|r| r.items.iter().any(|i| i.entity == failure.item)))
        .and_then(|r| r.context)
        .map(|c| solidcore_step::units::context(ex, c))
        .unwrap_or_default();
    let mut skip: HashSet<u64> = HashSet::new();
    let mut why = failure.message.clone();
    for _ in 0..MAX_ROUNDS {
        let before = skip.len();
        skip.extend(named_faces(&why));
        if skip.len() == before {
            return None;
        }
        match topology::read_solid_without(ex, failure.item, context, trims(), &skip) {
            Ok(mut solid) => {
                solid.notes.push(format!("{} faces left out", skip.len()));
                return Some((solid, skip.len()));
            }
            Err(e) => why = e,
        }
    }
    None
}

/// One colour an item of the file is styled with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint {
    pub rgb: [f64; 3],
    pub opacity: f64,
}

/// A STEP file as solidcore reads it.
pub struct Loaded {
    pub ex: Exchange,
    pub structure: Option<ProductStructure>,
    pub solids: Solids,
    /// (index into `solids.solids`, placement in the root frame, row-vector convention).
    pub placed: Vec<(usize, Matrix4)>,
    /// The first surface colour of each styled item, by the item's entity id.
    pub paint: BTreeMap<u64, Paint>,
    pub errors: Vec<String>,
    pub notes: Vec<String>,
}

fn colour_rgb(c: &Colour) -> Option<[f64; 3]> {
    match c {
        Colour::Rgb { rgb, .. } => Some(*rgb),
        Colour::PreDefined { rgb, .. } => *rgb,
    }
}

/// Parse and read everything short of meshing. `Err` only when the exchange structure itself
/// does not parse.
pub fn load(text: &str) -> Result<Loaded, String> {
    let ex = p21::parse(text).map_err(|e| format!("p21: {e}"))?;
    let mut errors = Vec::new();
    let mut notes = Vec::new();
    let declared = protocol::declared(&ex.schemas);
    notes.push(format!("schema {}", declared.protocol));
    if let Some(w) = declared.warning {
        notes.push(w);
    }
    let structure = match product::read(&ex) {
        Ok(s) => Some(s),
        Err(e) => {
            errors.push(format!("product structure: {e}"));
            None
        }
    };
    let mut solids = topology::read_solids_with(&ex, trims());
    notes.push(format!("pcurves without a closed form by {TRIMS}"));
    for f in &solids.failures {
        errors.push(format!("solid #{}: {}", f.item, f.message));
    }
    // A refused solid whose refusal names faces is read again without them, as SolidCraft's
    // import does (`crates/kernel/src/step_in/parallel.rs`, `surface_part`): what is left is a
    // body of open shells.
    let mut partial = 0;
    let mut left_out = 0;
    for f in std::mem::take(&mut solids.failures) {
        match without_named_faces(&ex, structure.as_ref(), &f) {
            Some((solid, skipped)) => {
                partial += 1;
                left_out += skipped;
                solids.solids.push(solid);
            }
            None => solids.failures.push(f),
        }
    }
    if partial > 0 {
        notes.push(format!("{partial} refused solids read again as open shells without the {left_out} faces their refusals name"));
    }
    // A solid item is read as one or more pieces (a surface model's closed shells are read on
    // their own), each a `Solid` named by its piece's entity id.
    let by_item: HashMap<u64, usize> = solids.solids.iter().enumerate().map(|(i, s)| (s.item, i)).collect();
    let mut placed = Vec::new();
    let mut reached = BTreeSet::new();
    if let Some(s) = &structure {
        match placed_bodies(s) {
            Ok(bodies) => {
                for b in bodies {
                    for piece in topology::items(&ex, b.entity) {
                        if let Some(&i) = by_item.get(&piece) {
                            placed.push((i, b.transform));
                            reached.insert(i);
                        }
                    }
                }
            }
            Err(e) => errors.push(format!("assembly: {e}")),
        }
    }
    let unplaced: Vec<usize> = (0..solids.solids.len()).filter(|i| !reached.contains(i)).collect();
    if !unplaced.is_empty() {
        notes.push(format!("{} solids outside the product tree, placed by the identity", unplaced.len()));
        placed.extend(unplaced.into_iter().map(|i| (i, IDENTITY)));
    }
    let mut paint = BTreeMap::new();
    if let Some(s) = &structure {
        for style in &s.styles {
            let found = style.surface.iter().find_map(|u| {
                let rgb = u.fill.as_ref().and_then(colour_rgb).or_else(|| u.rendering.as_ref().and_then(colour_rgb))?;
                Some(Paint { rgb, opacity: 1.0 - u.transparency.unwrap_or(0.0) })
            });
            if let Some(p) = found {
                paint.entry(style.item).or_insert(p);
            }
        }
    }
    Ok(Loaded { ex, structure, solids, placed, paint, errors, notes })
}

impl Loaded {
    /// The colour of record face `face` of `solid`: the face's own, else its solid's.
    pub fn face_paint(&self, solid: &Solid, face: usize) -> Option<Paint> {
        let own = solid.face_origin.get(face).and_then(|id| self.paint.get(id));
        own.or_else(|| self.paint.get(&solid.item)).copied()
    }
}

fn count(ex: &Exchange, names: &[&str]) -> usize {
    ex.entities.values().filter(|e| !e.is_complex() && names.contains(&e.name())).count()
}

/// `p · m` in the row-vector convention solidcore's STEP matrices are in.
fn place(m: &Matrix4, p: [f64; 3]) -> [f64; 3] {
    let mut out = [0.0; 3];
    for (j, o) in out.iter_mut().enumerate() {
        *o = p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j] + m[3][j];
    }
    out
}

#[derive(Default)]
struct Totals {
    triangles: usize,
    area: f64,
    volume: f64,
    open_edges: usize,
    min: [f64; 3],
    max: [f64; 3],
    any: bool,
}

impl Totals {
    /// One placed copy of a solid's mesh.
    fn add(&mut self, positions: &[[f64; 3]], triangles: &[[u32; 3]], m: &Matrix4) {
        let pts: Vec<[f64; 3]> = positions.iter().map(|p| place(m, *p)).collect();
        for p in &pts {
            if !self.any {
                self.min = *p;
                self.max = *p;
                self.any = true;
            }
            for k in 0..3 {
                self.min[k] = self.min[k].min(p[k]);
                self.max[k] = self.max[k].max(p[k]);
            }
        }
        // Weld on a grid of WELD: each solid's copy on its own, so touching bodies stay apart.
        let mut ids: HashMap<[i64; 3], u32> = HashMap::new();
        let welded: Vec<u32> = pts
            .iter()
            .map(|p| {
                let key = [(p[0] / WELD).round() as i64, (p[1] / WELD).round() as i64, (p[2] / WELD).round() as i64];
                let n = ids.len() as u32;
                *ids.entry(key).or_insert(n)
            })
            .collect();
        let mut uses: HashMap<(u32, u32), u32> = HashMap::new();
        for t in triangles {
            let (Some(a), Some(b), Some(c)) = (pts.get(t[0] as usize), pts.get(t[1] as usize), pts.get(t[2] as usize)) else { continue };
            let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            self.area += 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            self.volume += (a[0] * n[0] + a[1] * n[1] + a[2] * n[2]) / 6.0;
            self.triangles += 1;
            let w = [welded[t[0] as usize], welded[t[1] as usize], welded[t[2] as usize]];
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                if w[i] != w[j] {
                    *uses.entry((w[i].min(w[j]), w[i].max(w[j]))).or_insert(0) += 1;
                }
            }
        }
        self.open_edges += uses.values().filter(|n| **n == 1).count();
    }
}

pub fn run(path: &str) -> String {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return failed(&format!("read {path}: {e}"), 0.0),
    };
    let text = String::from_utf8_lossy(&bytes);
    let t0 = Instant::now();
    let loaded = load(&text);
    let parse_ms = t0.elapsed().as_secs_f64() * 1e3;
    let loaded = match loaded {
        Ok(l) => l,
        Err(e) => return failed(&e, parse_ms),
    };
    let mut errors = loaded.errors.clone();
    let mut notes = loaded.notes.clone();

    // One mesh per solid, placed once per instance.
    let t1 = Instant::now();
    let mut totals = Totals::default();
    let mut meshed_faces: BTreeSet<u64> = BTreeSet::new();
    let mut read_faces: BTreeSet<u64> = BTreeSet::new();
    let mut coloured: BTreeSet<u64> = BTreeSet::new();
    let mut refused_faces = 0usize;
    let mut meshes: Vec<Option<(Vec<[f64; 3]>, Vec<[u32; 3]>)>> = Vec::new();
    for solid in &loaded.solids.solids {
        for (f, origin) in solid.face_origin.iter().enumerate() {
            read_faces.insert(*origin);
            if loaded.face_paint(solid, f).is_some() {
                coloured.insert(*origin);
            }
        }
        match plan(&solid.brep, &MeshTolerance::chord(CHORD), 0.0) {
            Ok((p, _stats)) => {
                let (mesh, refused) = p.evaluate();
                refused_faces += refused.len();
                if let Some(r) = refused.first() {
                    errors.push(format!("mesh of solid #{} face {}: {}", solid.item, r.face, r.reason));
                }
                for f in &mesh.tri_face {
                    if let Some(origin) = solid.face_origin.get(*f as usize) {
                        meshed_faces.insert(*origin);
                    }
                }
                let positions = mesh.positions.iter().map(|p| [p.x, p.y, p.z]).collect();
                meshes.push(Some((positions, mesh.triangles)));
            }
            Err(e) => {
                errors.push(format!("mesh of solid #{}: {e}", solid.item));
                meshes.push(None);
            }
        }
    }
    for (i, m) in &loaded.placed {
        if let Some(Some((positions, triangles))) = meshes.get(*i) {
            totals.add(positions, triangles, m);
        }
    }
    let tess_ms = t1.elapsed().as_secs_f64() * 1e3;

    let ex = &loaded.ex;
    let solids = loaded.solids.solids.len() + loaded.solids.failures.len();
    let shells = count(ex, &["CLOSED_SHELL", "OPEN_SHELL"]);
    let faces_total = count(ex, &["ADVANCED_FACE", "FACE_SURFACE"]);
    let faces_meshed = meshed_faces.len();
    let distinct: BTreeSet<[i64; 3]> = loaded.paint.values().map(|p| p.rgb.map(|c| (c * 1e4).round() as i64)).collect();
    let products = loaded.structure.as_ref().and_then(|s| s.instances().ok()).map(|i| i.len());
    notes.push(format!(
        "solids {} read, {} refused; {} placed instances; triangles, area, volume, open_edges summed over placed instances; faces counted once per file face ({} read into records, {} refused by the mesher); faces_coloured counts faces (own style, else their solid's); chord {CHORD} mm, no angle limit",
        loaded.solids.solids.len(),
        loaded.solids.failures.len(),
        loaded.placed.len(),
        read_faces.len(),
        refused_faces,
    ));
    let repairs: usize = loaded.solids.solids.iter().map(|s| s.notes.len()).sum();
    if repairs > 0 {
        notes.push(format!("{repairs} reader notes on solids (seams added, tolerances raised, ...)"));
    }
    if errors.len() > 1 {
        notes.push(format!("{} failures in all", errors.len()));
    }
    format!(
        "{{\"parse_ok\":true,\"parse_ms\":{},\"error\":{},\"solids\":{},\"shells\":{},\"faces_total\":{},\"faces_meshed\":{},\"faces_failed\":{},\"triangles\":{},\"tess_ms\":{},\"bbox_min\":{},\"bbox_max\":{},\"area\":{},\"volume\":{},\"open_edges\":{},\"colours_distinct\":{},\"faces_coloured\":{},\"products\":{},\"notes\":{}}}",
        json::number(parse_ms),
        json::opt_string(errors.first().map(|e| json::shortened(e)).as_deref()),
        solids,
        shells,
        faces_total,
        faces_meshed,
        faces_total.saturating_sub(faces_meshed),
        totals.triangles,
        json::number(tess_ms),
        json::opt_point(totals.any.then_some(totals.min)),
        json::opt_point(totals.any.then_some(totals.max)),
        json::opt_number(totals.any.then_some(totals.area)),
        json::opt_number(totals.any.then_some(totals.volume)),
        json::opt_count(totals.any.then_some(totals.open_edges)),
        distinct.len(),
        coloured.len(),
        json::opt_count(products),
        json::string(&notes.join("; ")),
    )
}

fn failed(error: &str, parse_ms: f64) -> String {
    format!(
        "{{\"parse_ok\":false,\"parse_ms\":{},\"error\":{},\"solids\":null,\"shells\":null,\"faces_total\":null,\"faces_meshed\":null,\"faces_failed\":null,\"triangles\":null,\"tess_ms\":null,\"bbox_min\":null,\"bbox_max\":null,\"area\":null,\"volume\":null,\"open_edges\":null,\"colours_distinct\":null,\"faces_coloured\":null,\"products\":null,\"notes\":\"\"}}",
        json::number(parse_ms),
        json::string(&json::shortened(error)),
    )
}
