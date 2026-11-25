//! STEP kernel trial harness (issue #442): truck as released on crates.io.

mod common;

use common::{board, print_write, ReadReport, Tri};
use std::time::Instant;
use truck_meshalgo::prelude::*;
use truck_modeling::builder;
use truck_stepio::out::{CompleteStepDisplay, StepHeaderDescriptor, StepModel, StepModels};
use truck_stepio::r#in::alias::{Curve3D, Surface};
use truck_stepio::r#in::{ruststep, Table};
use truck_topology::compress::{CompressedShell, CompressedSolid};

type CShell = CompressedShell<Point3, Curve3D, Surface>;
type CSolid = CompressedSolid<Point3, Curve3D, Surface>;

const TESS_TOL: f64 = 0.01;
const SCHEMA: &str = "ISO-10303-042";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match (args.get(1).map(String::as_str), args.len()) {
        (Some("read"), 3) => read(&args[2]),
        (Some("write"), 4) => write(&args[2], &args[3]),
        _ => {
            eprintln!("usage: truck-released read <file.step> | write <out.step> <component.step>");
            std::process::exit(2);
        }
    }
}

fn parse_text(text: &str) -> Result<Table, String> {
    let exchange = ruststep::parser::parse(text).map_err(|e| format!("ruststep parse: {e}"))?;
    let Some(data) = exchange.data.first() else {
        return Err("no DATA section".into());
    };
    Ok(Table::from_data_section(data))
}

fn parse(path: &str) -> Result<(Table, f64), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read file: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let t0 = Instant::now();
    let table = parse_text(&text)?;
    Ok((table, t0.elapsed().as_secs_f64() * 1000.0))
}

/// Tessellates one shell. Returns (triangles, faces meshed).
fn mesh_shell(shell: &CShell) -> (Vec<Tri>, usize) {
    // TRUCK_TESS=plain selects `triangulation`; the default is `robust_triangulation`,
    // which is what truck's own step-to-mesh example uses for STEP input.
    let meshed = match std::env::var("TRUCK_TESS").as_deref() {
        Ok("plain") => shell.triangulation(TESS_TOL),
        _ => shell.robust_triangulation(TESS_TOL),
    };
    let (mut tris, mut ok) = (Vec::new(), 0);
    for face in &meshed.faces {
        let before = tris.len();
        if let Some(poly) = &face.surface {
            let pos = poly.positions();
            for t in poly.faces().triangle_iter() {
                let mut tri = [0, 1, 2].map(|i| {
                    let p = pos[t[i].pos];
                    [p.x, p.y, p.z]
                });
                if !face.orientation {
                    tri.swap(1, 2);
                }
                tris.push(tri);
            }
        }
        if tris.len() > before {
            ok += 1;
        }
    }
    (tris, ok)
}

fn read(path: &str) {
    let mut r = ReadReport::default();
    let (table, ms) = match parse(path) {
        Ok(x) => x,
        Err(e) => {
            r.fail(e);
            r.print();
            return;
        }
    };
    r.parse_ok = true;
    r.parse_ms = ms;
    r.solids = None;
    r.shells = Some(table.shell.len());
    r.notes.push(
        "colours: reader has no colour or style entities; solids and products: reader has no \
MANIFOLD_SOLID_BREP, product or assembly entities, so all shells are meshed in the file frame and \
placements are not applied"
            .into(),
    );

    let t0 = Instant::now();
    let (mut faces_total, mut meshed) = (0, 0);
    let mut tris: Vec<Tri> = Vec::new();
    let mut ids: Vec<_> = table.shell.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        match table.to_compressed_shell(&table.shell[&id]) {
            Ok(shell) => {
                faces_total += shell.faces.len();
                let (t, ok) = mesh_shell(&shell);
                meshed += ok;
                tris.extend(t);
            }
            Err(e) => r.fail(format!("shell #{id}: {e}")),
        }
    }
    // `to_compressed_shell` silently drops faces it cannot convert.
    let step_faces = table.face_surface.len();
    if step_faces != faces_total {
        r.notes.push(format!(
            "{step_faces} face entities in file, {faces_total} converted"
        ));
    }
    r.tess_ms = Some(t0.elapsed().as_secs_f64() * 1000.0);
    r.faces_total = Some(faces_total.max(step_faces));
    r.faces_meshed = Some(meshed);
    r.faces_failed = Some(r.faces_total.unwrap() - meshed);
    if tris.is_empty() {
        r.fail("no triangles produced");
    }
    r.tris = Some(tris);
    r.print();
}

// ---------------------------------------------------------------- write

use truck_modeling::{Face, Solid, Vertex, Wire};

fn v(x: f64, y: f64) -> Vertex { builder::vertex(Point3::new(x, y, 0.0)) }
fn pt(x: f64, y: f64) -> Point3 { Point3::new(x, y, 0.0) }

/// A full circle as two arcs, clockwise seen from +Z.
fn hole(cx: f64, cy: f64, r: f64) -> Wire {
    let (a, b) = (v(cx + r, cy), v(cx - r, cy));
    vec![
        builder::circle_arc(&a, &b, pt(cx, cy - r)),
        builder::circle_arc(&b, &a, pt(cx, cy + r)),
    ]
    .into()
}

fn board_face() -> Result<Face, String> {
    use board::*;
    let s = std::f64::consts::FRAC_1_SQRT_2 * R;
    let o = [
        v(R, 0.0),
        v(W - R, 0.0),
        v(W, R),
        v(W, H - R),
        v(W - R, H),
        v(R, H),
        v(0.0, H - R),
        v(0.0, R),
    ];
    let outer: Wire = vec![
        builder::line(&o[0], &o[1]),
        builder::circle_arc(&o[1], &o[2], pt(W - R + s, R - s)),
        builder::line(&o[2], &o[3]),
        builder::circle_arc(&o[3], &o[4], pt(W - R + s, H - R + s)),
        builder::line(&o[4], &o[5]),
        builder::circle_arc(&o[5], &o[6], pt(R - s, H - R + s)),
        builder::line(&o[6], &o[7]),
        builder::circle_arc(&o[7], &o[0], pt(R - s, R - s)),
    ]
    .into();
    let mut wires = vec![outer];
    for (cx, cy) in [
        (HOLE_INSET, HOLE_INSET),
        (W - HOLE_INSET, HOLE_INSET),
        (W - HOLE_INSET, H - HOLE_INSET),
        (HOLE_INSET, H - HOLE_INSET),
    ] {
        wires.push(hole(cx, cy, HOLE_R));
    }
    let [cx, cy] = SLOT_C;
    let q = [
        v(cx - SLOT_HALF, cy + SLOT_R),
        v(cx + SLOT_HALF, cy + SLOT_R),
        v(cx + SLOT_HALF, cy - SLOT_R),
        v(cx - SLOT_HALF, cy - SLOT_R),
    ];
    wires.push(
        vec![
            builder::line(&q[0], &q[1]),
            builder::circle_arc(&q[1], &q[2], pt(cx + SLOT_HALF + SLOT_R, cy)),
            builder::line(&q[2], &q[3]),
            builder::circle_arc(&q[3], &q[0], pt(cx - SLOT_HALF - SLOT_R, cy)),
        ]
        .into(),
    );
    builder::try_attach_plane(&wires).map_err(|e| format!("try_attach_plane: {e}"))
}

fn placed(solid: &CSolid, m: Matrix4) -> CSolid {
    let mut solid = solid.clone();
    for shell in &mut solid.boundaries {
        for p in &mut shell.vertices {
            *p = m.transform_point(*p);
        }
        for edge in &mut shell.edges {
            edge.curve.transform_by(m);
        }
        for face in &mut shell.faces {
            face.surface.transform_by(m);
        }
    }
    solid
}

/// The released reader has no solid entity: wrap every shell as a one-shell solid.
fn shells_as_solids(table: &Table) -> Result<Vec<CSolid>, String> {
    let mut ids: Vec<_> = table.shell.keys().copied().collect();
    ids.sort_unstable();
    ids.iter()
        .map(|id| {
            let shell = table
                .to_compressed_shell(&table.shell[id])
                .map_err(|e| format!("shell #{id}: {e}"))?;
            Ok(CompressedSolid {
                boundaries: vec![shell],
            })
        })
        .collect()
}

fn header() -> StepHeaderDescriptor {
    StepHeaderDescriptor {
        file_name: "trial-board".into(),
        organization_system: "reda step-kernel-trial truck-released".into(),
        ..Default::default()
    }
}

fn write(out_path: &str, component: &str) {
    let mut notes: Vec<String> = Vec::new();
    match write_inner(out_path, component, &mut notes) {
        Ok(bytes) => print_write(true, None, Some(bytes), SCHEMA, &notes),
        Err(e) => print_write(false, Some(e), None, SCHEMA, &notes),
    }
}

fn write_inner(out_path: &str, component: &str, notes: &mut Vec<String>) -> Result<usize, String> {
    // Board, in truck-modeling's own geometry types.
    let face = board_face()?;
    let solid: Solid = builder::tsweep(&face, Vector3::new(0.0, 0.0, board::T));
    let pcb = solid.compress();
    let pcb_text = CompleteStepDisplay::new(StepModel::from(&pcb), header()).to_string();

    // The writer takes one geometry type per file, and the reader's types (`in::alias`) differ
    // from truck-modeling's. Bring the board into the reader's types by reading back its STEP.
    let pcb_table = parse_text(&pcb_text)?;
    let mut solids = shells_as_solids(&pcb_table).map_err(|e| format!("board read-back: {e}"))?;
    if solids.len() != 1 {
        return Err(format!("board read-back gave {} solids", solids.len()));
    }
    notes.push(
        "board: truck-modeling 0.6 has no circle or cylinder geometry, so the 28 arcs are rational \
B-spline curves and the 14 arc walls rational B-spline surfaces (flat walls and caps are PLANE); \
board went through truck's own STEP writer and reader to share a geometry type with the component"
            .into(),
    );

    // Component, through truck's own reader.
    let (table, _) = parse(component)?;
    let comp = shells_as_solids(&table).map_err(|e| format!("component: {e}"))?;
    if comp.is_empty() {
        return Err("component: no shell readable".into());
    }
    notes.push(
        "component: reader exposes shells only, each one was written back as its own \
MANIFOLD_SOLID_BREP; the writer turns its CYLINDRICAL_SURFACE faces into SURFACE_OF_REVOLUTION"
            .into(),
    );
    let u1 = Matrix4::from_translation(Vector3::new(20.0, 10.0, board::T))
        * Matrix4::from_angle_z(Rad(std::f64::consts::FRAC_PI_2));
    let u2 = Matrix4::from_translation(Vector3::new(35.0, 20.0, board::T));
    for m in [u1, u2] {
        solids.extend(comp.iter().map(|s| placed(s, m)));
    }
    notes.push(format!(
        "missing: assembly (writer emits one PRODUCT with an empty name holding {} solids; no \
trial-board/PCB/U1/U2 names, no NEXT_ASSEMBLY_USAGE_OCCURRENCE, no shared definition: the component \
geometry is written twice with placements baked in); missing: all colour",
        solids.len()
    ));
    notes.push(
        "header: truck writes FILE_SCHEMA 'ISO-10303-042' (not a schema name) and it cannot be set \
through the API; a copy with AUTOMOTIVE_DESIGN is written next to the output as *.schema-fixed.step"
            .into(),
    );

    let models: StepModels<'_, Point3, Curve3D, Surface> = solids.iter().collect();
    let text = CompleteStepDisplay::new(models, header()).to_string();
    std::fs::write(out_path, &text).map_err(|e| format!("write: {e}"))?;
    let fixed = text.replacen(
        "FILE_SCHEMA(('ISO-10303-042'))",
        "FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'))",
        1,
    );
    let fixed_path = std::path::Path::new(out_path).with_extension("schema-fixed.step");
    std::fs::write(&fixed_path, fixed).map_err(|e| format!("write: {e}"))?;
    Ok(text.len())
}
