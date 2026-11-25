//! `read`: parse one STEP file and report what step-io exposes. step-io does
//! not tessellate, so every mesh field is null.

use std::collections::{BTreeMap, HashSet};
use std::time::Instant;

use step_io::scene::{CurveKind, Face, Rgb, Scene, SurfaceKind};
use step_io::{Report, StepModel};

use crate::json::{Obj, shorten};

/// What the scene handles give for one model.
#[derive(Default)]
pub struct Summary {
    pub solids: usize,
    pub solids_with_voids: usize,
    pub shells: usize,
    pub surface_models: usize,
    pub faces_total: usize,
    pub faces_all: usize,
    pub faces_coloured: usize,
    pub faces_own_colour: usize,
    pub solids_coloured: usize,
    pub colours: Vec<Rgb>,
    pub products: usize,
    pub definitions: usize,
    pub roots: usize,
    pub occurrences: usize,
    pub occurrences_without_transform: usize,
    pub mapped_instances: usize,
    pub meshes: usize,
    pub vertex_loops: usize,
    pub surfaces: BTreeMap<String, usize>,
    pub curves: BTreeMap<String, usize>,
    pub length_unit: Option<(String, f64)>,
    pub angle_unit: Option<(String, f64)>,
    pub scene_warnings: usize,
    pub first_warning: Option<String>,
}

fn colour_key(c: Rgb) -> [i64; 3] {
    [c.red, c.green, c.blue].map(|v| (v * 1e6).round() as i64)
}

pub fn surface_name(kind: &SurfaceKind<'_>) -> &'static str {
    match kind {
        SurfaceKind::Plane(_) => "plane",
        SurfaceKind::Cylindrical(_) => "cylinder",
        SurfaceKind::Conical(_) => "cone",
        SurfaceKind::Spherical(_) => "sphere",
        SurfaceKind::Toroidal(_) => "torus",
        SurfaceKind::DegenerateToroidal(_) => "degenerate_torus",
        SurfaceKind::Offset(_) => "offset",
        SurfaceKind::BSpline(_) => "b_spline_surface",
        SurfaceKind::BSplineWithKnots(_) => "b_spline_with_knots",
        SurfaceKind::LinearExtrusion(_) => "linear_extrusion",
        SurfaceKind::Revolution(_) => "revolution",
        SurfaceKind::QuasiUniform(_) => "quasi_uniform",
        SurfaceKind::Uniform(_) => "uniform",
        SurfaceKind::Bezier(_) => "bezier",
        // A rational B-spline surface is a complex instance.
        SurfaceKind::Other("COMPLEX") => "complex(rational_b_spline?)",
        SurfaceKind::Other(s) => s,
        _ => "unknown",
    }
}

pub fn curve_name(kind: &CurveKind<'_>) -> &'static str {
    match kind {
        CurveKind::Line(_) => "line",
        CurveKind::Circle(_) => "circle",
        CurveKind::Ellipse(_) => "ellipse",
        CurveKind::Parabola(_) => "parabola",
        CurveKind::Hyperbola(_) => "hyperbola",
        CurveKind::BSpline(_) => "b_spline_curve",
        CurveKind::BSplineWithKnots(_) => "b_spline_with_knots",
        CurveKind::Trimmed(_) => "trimmed",
        CurveKind::Polyline(_) => "polyline",
        CurveKind::Composite(_) => "composite",
        CurveKind::QuasiUniform(_) => "quasi_uniform",
        CurveKind::Uniform(_) => "uniform",
        CurveKind::Bezier(_) => "bezier",
        CurveKind::SurfaceCurve(_) => "surface_curve",
        CurveKind::SeamCurve(_) => "seam_curve",
        CurveKind::BoundedSurfaceCurve(_) => "bounded_surface_curve",
        CurveKind::IntersectionCurve(_) => "intersection_curve",
        CurveKind::Other("COMPLEX") => "complex(rational_b_spline?)",
        CurveKind::Other(s) => s,
        _ => "unknown",
    }
}

pub fn summarize(model: &StepModel) -> Summary {
    let scene = model.scene();
    let mut s = Summary::default();
    let mut colours: HashSet<[i64; 3]> = HashSet::new();
    let mut edges_seen = HashSet::new();
    let mut note_colour = |s: &mut Summary, c: Rgb| {
        if colours.insert(colour_key(c)) {
            s.colours.push(c);
        }
    };

    let mut visit_face = |s: &mut Summary, face: Face<'_>, inherited: bool, scene: &Scene<'_>| {
        let _ = scene;
        s.faces_total += 1;
        *s.surfaces
            .entry(surface_name(&face.surface().kind()).to_owned())
            .or_default() += 1;
        let own = face.color();
        if own.is_some() {
            s.faces_own_colour += 1;
        }
        if own.is_some() || inherited {
            s.faces_coloured += 1;
        }
        for bound in face.bounds() {
            if bound.vertex().is_some() {
                s.vertex_loops += 1;
            }
            for edge in bound.edges() {
                if !edges_seen.insert(edge.key()) {
                    continue;
                }
                // An edge on a surface curve follows that curve's 3D curve.
                let mut curve = edge.curve();
                while let Some(c3) = curve.curve_3d() {
                    curve = c3;
                }
                *s.curves
                    .entry(curve_name(&curve.kind()).to_owned())
                    .or_default() += 1;
            }
        }
        own
    };

    for solid in scene.all_solids() {
        s.solids += 1;
        let solid_colour = solid.color();
        if let Some(c) = solid_colour {
            s.solids_coloured += 1;
            note_colour(&mut s, c);
        }
        let mut shells = Vec::new();
        shells.extend(solid.outer());
        let voids: Vec<_> = solid.voids().collect();
        if !voids.is_empty() {
            s.solids_with_voids += 1;
        }
        shells.extend(voids);
        for (shell, _) in shells {
            s.shells += 1;
            let shell_colour = shell.color();
            if let Some(c) = shell_colour {
                note_colour(&mut s, c);
            }
            for face in shell.faces() {
                let inherited = solid_colour.is_some() || shell_colour.is_some();
                if let Some(c) = visit_face(&mut s, face, inherited, &scene) {
                    note_colour(&mut s, c);
                }
            }
        }
    }
    for sm in scene.all_surface_models() {
        s.surface_models += 1;
        let model_colour = sm.color();
        if let Some(c) = model_colour {
            note_colour(&mut s, c);
        }
        for (shell, _) in sm.shells() {
            s.shells += 1;
            let shell_colour = shell.color();
            if let Some(c) = shell_colour {
                note_colour(&mut s, c);
            }
            for face in shell.faces() {
                let inherited = model_colour.is_some() || shell_colour.is_some();
                if let Some(c) = visit_face(&mut s, face, inherited, &scene) {
                    note_colour(&mut s, c);
                }
            }
        }
    }
    s.faces_all = scene.all_faces().count();

    s.products = scene.all_products().count();
    s.roots = scene.root_definitions().count();
    for def in scene.all_product_definitions() {
        s.definitions += 1;
        for occ in def.occurrences() {
            s.occurrences += 1;
            if occ.transform().and_then(|t| t.matrix()).is_none() {
                s.occurrences_without_transform += 1;
            }
        }
    }
    s.mapped_instances = scene.all_mapped_instances().count();
    s.meshes = scene.all_meshes().count();

    let units = scene.units();
    s.length_unit = units.length.map(|u| (u.name, u.to_si));
    s.angle_unit = units.angle.map(|u| (u.name, u.to_si));
    let warnings = scene.warnings();
    s.scene_warnings = warnings.len();
    s.first_warning = warnings.first().map(|w| w.message.clone());
    s
}

fn histogram(h: &BTreeMap<String, usize>) -> String {
    if h.is_empty() {
        return "none".to_owned();
    }
    h.iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn drop_summary(report: &Report) -> String {
    if report.dropped.is_empty() {
        return String::new();
    }
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for (_, reason) in &report.dropped {
        *kinds.entry(reason.kind.to_string()).or_default() += 1;
    }
    let first = &report.dropped[0];
    format!(
        " ({}; first #{}: {})",
        histogram(&kinds),
        first.0,
        shorten(&first.1.to_string())
    )
}

pub fn notes(model: &StepModel, report: &Report, s: &Summary) -> String {
    let unit = |u: &Option<(String, f64)>| match u {
        Some((name, to_si)) => format!("{name} (x{to_si} to SI)"),
        None => "none found".to_owned(),
    };
    let mut n = format!(
        "step-io 0.3.1 has no tessellator: mesh fields are null. schema {}; \
         entities read {} kept {} dropped {}{} normalized {} parse-warnings {}; \
         length unit {}, angle unit {}; \
         faces_coloured counts faces with their own colour ({}) or one inherited from \
         their solid or shell; coloured solids {} of {}; \
         products is the PRODUCT count ({} definitions, {} roots, {} placed occurrences, \
         {} without a usable transform, {} mapped items); \
         surfaces by face: {}; curves by edge: {}",
        model.header().schema,
        report.entities_read,
        report.entities_kept,
        report.dropped.len(),
        drop_summary(report),
        report.normalized.len(),
        report.parse_warnings.len(),
        unit(&s.length_unit),
        unit(&s.angle_unit),
        s.faces_own_colour,
        s.solids_coloured,
        s.solids,
        s.definitions,
        s.roots,
        s.occurrences,
        s.occurrences_without_transform,
        s.mapped_instances,
        histogram(&s.surfaces),
        histogram(&s.curves),
    );
    if s.solids_with_voids > 0 {
        n.push_str(&format!("; solids with voids {}", s.solids_with_voids));
    }
    if s.surface_models > 0 {
        n.push_str(&format!(
            "; surface models (not solids) {}",
            s.surface_models
        ));
    }
    if s.vertex_loops > 0 {
        n.push_str(&format!("; vertex loops {}", s.vertex_loops));
    }
    if s.faces_all != s.faces_total {
        n.push_str(&format!(
            "; faces in the file {} against {} reached through solids and surface models",
            s.faces_all, s.faces_total
        ));
    }
    if s.meshes > 0 {
        n.push_str(&format!("; display meshes stored in the file {}", s.meshes));
    }
    if s.scene_warnings > 0 {
        n.push_str(&format!(
            "; scene warnings {} (first: {})",
            s.scene_warnings,
            shorten(s.first_warning.as_deref().unwrap_or(""))
        ));
    }
    n
}

pub fn run(path: &str) -> String {
    let mut o = Obj::new();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return failed(&format!("cannot open {path}: {e}"), 0.0),
    };
    let start = Instant::now();
    let parsed = step_io::read(bytes.as_slice());
    let parse_ms = start.elapsed().as_secs_f64() * 1000.0;
    let (model, report) = match parsed {
        Ok(v) => v,
        Err(e) => return failed(&e.to_string(), parse_ms),
    };
    let s = summarize(&model);
    o.bool("parse_ok", true)
        .num("parse_ms", parse_ms)
        .null("error")
        .int("solids", s.solids)
        .int("shells", s.shells)
        .int("faces_total", s.faces_total);
    mesh_nulls(&mut o);
    o.int("colours_distinct", s.colours.len())
        .int("faces_coloured", s.faces_coloured)
        .int("products", s.products)
        .str("notes", &notes(&model, &report, &s));
    o.finish()
}

fn mesh_nulls(o: &mut Obj) {
    for k in [
        "faces_meshed",
        "faces_failed",
        "triangles",
        "tess_ms",
        "bbox_min",
        "bbox_max",
        "area",
        "volume",
        "open_edges",
    ] {
        o.null(k);
    }
}

fn failed(error: &str, parse_ms: f64) -> String {
    let mut o = Obj::new();
    o.bool("parse_ok", false)
        .num("parse_ms", parse_ms)
        .str("error", &shorten(error))
        .null("solids")
        .null("shells")
        .null("faces_total");
    mesh_nulls(&mut o);
    o.null("colours_distinct")
        .null("faces_coloured")
        .null("products")
        .str("notes", "step-io 0.3.1: the file did not parse");
    o.finish()
}
