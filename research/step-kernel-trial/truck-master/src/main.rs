//! STEP kernel trial harness (issue #442): truck at master.

mod common;

use common::{board, print_write, ReadReport, Tri};
use std::time::Instant;
use truck_assembly::assy::{Assembly, EdgeEntity, NodeEntity};
use truck_meshalgo::prelude::*;
use truck_modeling::builder;
use truck_stepio::common::PartAttrs;
use truck_stepio::out::{StepDesign, StepDisplay, StepHeaderDescriptor, StepModel};
use truck_stepio::r#in::convert::{ProductShape, StepAssembly};
use truck_stepio::r#in::ruststep;
use truck_stepio::r#in::step_geometry::{
    Conic3D, Curve3D, ElementarySurface, Line, Plane, Processor, RevolutedCurve, Surface, SweptCurve,
};
use truck_stepio::r#in::Table;

type CShell = CompressedShell;
type CSolid = CompressedSolid;
type Model<'a> = StepModel<'a, Point3, Curve3D, Surface>;

const TESS_TOL: f64 = 0.01;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match (args.get(1).map(String::as_str), args.len()) {
        (Some("read"), 3) => read(&args[2]),
        (Some("write"), 4) => write(&args[2], &args[3]),
        _ => {
            eprintln!("usage: truck-master read <file.step> | write <out.step> <component.step>");
            std::process::exit(2);
        }
    }
}

fn parse(path: &str) -> Result<(Table, f64), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read file: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let t0 = Instant::now();
    let exchange = ruststep::parser::parse(&text).map_err(|e| format!("ruststep parse: {e}"))?;
    let Some(data) = exchange.data.first() else {
        return Err("no DATA section".into());
    };
    let table = Table::from_data_section(data);
    Ok((table, t0.elapsed().as_secs_f64() * 1000.0))
}

/// Tessellates one shell. Returns (triangles, faces meshed, faces failed).
fn mesh_shell(shell: &CShell) -> (Vec<Tri>, usize, usize) {
    // TRUCK_TESS=plain selects `triangulation`; the default is `robust_triangulation`,
    // which is what truck's own step-to-mesh example uses for STEP input.
    let meshed = match std::env::var("TRUCK_TESS").as_deref() {
        Ok("plain") => shell.triangulation(TESS_TOL),
        _ => shell.robust_triangulation(TESS_TOL),
    };
    let (mut tris, mut ok, mut failed) = (Vec::new(), 0, 0);
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
        } else {
            failed += 1;
        }
    }
    (tris, ok, failed)
}

fn transform(tris: &[Tri], m: Matrix4, out: &mut Vec<Tri>) {
    let flip = m.determinant() < 0.0;
    for t in tris {
        let mut tri = t.map(|p| {
            let q = m.transform_point(Point3::new(p[0], p[1], p[2]));
            [q.x, q.y, q.z]
        });
        if flip {
            tri.swap(1, 2);
        }
        out.push(tri);
    }
}

fn node_shells(shapes: &[ProductShape]) -> Vec<&CShell> {
    shapes
        .iter()
        .flat_map(|s| match s {
            ProductShape::Solid(solid) => solid.boundaries.iter().collect::<Vec<_>>(),
            ProductShape::Shells(shells) => shells.iter().collect(),
            ProductShape::Matrix(_) => Vec::new(),
        })
        .collect()
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
    r.solids = Some(table.manifold_solid_brep.len());
    r.shells = Some(table.shell.len());
    r.notes
        .push("colours: reader has no colour or style entities".into());

    let t0 = Instant::now();
    let (mut faces_total, mut meshed, mut failed) = (0, 0, 0);
    let mut tris: Vec<Tri> = Vec::new();

    // Route 1: the product structure (`Table::step_assy`), with placements applied.
    let mut via_assy = false;
    match table.step_assy() {
        Ok(assy) => {
            r.products = Some(assy.len());
            let node_count: usize = assy.all_nodes().map(|n| node_shells(n.shape()).len()).sum();
            if node_count > 0 {
                via_assy = true;
                if node_count != table.shell.len() {
                    r.notes.push(format!(
                        "product structure reaches {node_count} of {} shells",
                        table.shell.len()
                    ));
                }
                let mut per_node = std::collections::HashMap::new();
                for node in assy.all_nodes() {
                    let mut node_tris = Vec::new();
                    for shell in node_shells(node.shape()) {
                        faces_total += shell.faces.len();
                        let (t, ok, ng) = mesh_shell(shell);
                        meshed += ok;
                        failed += ng;
                        node_tris.extend(t);
                    }
                    per_node.insert(node.index(), node_tris);
                }
                let mut bad_matrix = 0;
                for top in assy.top_nodes() {
                    for path in assy.paths_iter(top.index()) {
                        let mut m = Matrix4::identity();
                        for edge in path.edges() {
                            match Matrix4::try_from(edge.matrix()) {
                                Ok(e) => m = m * e,
                                Err(_) => bad_matrix += 1,
                            }
                        }
                        let index = path.terminal_node().index();
                        transform(&per_node[&index], m, &mut tris);
                    }
                }
                if bad_matrix > 0 {
                    r.notes
                        .push(format!("{bad_matrix} placements not convertible"));
                }
                r.notes
                    .push("mesh from step_assy, placements applied".into());
            } else {
                r.notes.push(
                    "step_assy returned no shapes (it does not follow SHAPE_REPRESENTATION_RELATIONSHIP); \
meshed all shells in file frame, placements not applied"
                        .into(),
                );
            }
        }
        Err(e) => r.notes.push(format!(
            "step_assy failed ({e}); meshed all shells in file frame, placements not applied"
        )),
    }

    // Route 2: every CLOSED_SHELL / OPEN_SHELL in the table, no placements.
    if !via_assy {
        let mut ids: Vec<_> = table.shell.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            match table.to_compressed_shell(&table.shell[&id]) {
                Ok(shell) => {
                    faces_total += shell.faces.len();
                    let (t, ok, ng) = mesh_shell(&shell);
                    meshed += ok;
                    failed += ng;
                    tris.extend(t);
                }
                Err(e) => r.fail(format!("shell #{id}: {e}")),
            }
        }
    }
    // `to_compressed_shell` silently drops faces it cannot convert.
    let step_faces = table.face_surface.len();
    if step_faces != faces_total && !via_assy {
        r.notes.push(format!(
            "{step_faces} face entities in file, {faces_total} converted"
        ));
    }
    r.tess_ms = Some(t0.elapsed().as_secs_f64() * 1000.0);
    r.faces_total = Some(faces_total.max(if via_assy { 0 } else { step_faces }));
    r.faces_meshed = Some(meshed);
    r.faces_failed = Some(r.faces_total.unwrap() - meshed);
    let _ = failed;
    if tris.is_empty() {
        r.fail("no triangles produced");
    }
    r.tris = Some(tris);
    r.print();
}

// ---------------------------------------------------------------- write

truck_topology::prelude!(Point3, Curve3D, Surface);

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
    builder::try_attach_plane(wires).map_err(|e| format!("try_attach_plane: {e}"))
}

/// `tsweep` makes every side wall a SURFACE_OF_LINEAR_EXTRUSION. Replace the ones swept from a
/// line with a PLANE and the ones swept from a circle with a CYLINDRICAL_SURFACE.
/// Returns (planes, cylinders) replaced.
fn analytic_walls(solid: &mut CSolid) -> (usize, usize) {
    let (mut planes, mut cylinders) = (0, 0);
    for face in solid.boundaries.iter_mut().flat_map(|s| s.faces.iter_mut()) {
        let Surface::SweptCurve(SweptCurve::ExtrudedCurve(ext)) = &face.surface else {
            continue;
        };
        let ((u0, u1), (v0, v1)) = ext.range_tuple();
        let (u, w) = ((u0 + u1) / 2.0, (v0 + v1) / 2.0);
        let normal = ext.normal(u, w);
        let vector = ext.extruding_vector();
        match ext.entity_curve() {
            Curve3D::Line(Line(p, q)) => {
                let mut plane = Plane::new(*p, *q, *p + vector);
                if plane.normal().dot(normal) < 0.0 {
                    plane = Plane::new(*p, *p + vector, *q);
                }
                face.surface = Surface::ElementarySurface(ElementarySurface::Plane(plane));
                planes += 1;
            }
            Curve3D::Conic(Conic3D::Ellipse(circle)) => {
                let m = *circle.transform();
                let (x, y) = (m[0].truncate(), m[1].truncate());
                if !x.magnitude().near(&y.magnitude()) {
                    continue;
                }
                let centre = Point3::from_homogeneous(m[3]);
                let axis = vector.normalize();
                let p = centre + x;
                // Same construction as truck's own CYLINDRICAL_SURFACE reader:
                // inverted = outward normal.
                let line = Line(p, p + axis);
                let mut cyl = Processor::new(RevolutedCurve::by_revolution(line, centre, axis));
                cyl.invert();
                let radial = ext.subs(u, w) - centre;
                let radial = radial - axis * radial.dot(axis);
                if normal.dot(radial) < 0.0 {
                    cyl.invert();
                }
                face.surface =
                    Surface::ElementarySurface(ElementarySurface::CylindricalSurface(cyl));
                cylinders += 1;
            }
            _ => {}
        }
    }
    (planes, cylinders)
}

fn attrs(name: &str) -> PartAttrs {
    PartAttrs {
        id: name.into(),
        name: name.into(),
        description: String::new(),
    }
}

fn write(out_path: &str, component: &str) {
    let mut notes: Vec<String> = Vec::new();
    match write_inner(out_path, component, &mut notes) {
        Ok(bytes) => print_write(true, None, Some(bytes), "ISO-10303-203", &notes),
        Err(e) => print_write(false, Some(e), None, "ISO-10303-203", &notes),
    }
}

fn write_inner(out_path: &str, component: &str, notes: &mut Vec<String>) -> Result<usize, String> {
    // Board.
    let face = board_face()?;
    let solid: Solid = builder::tsweep(&face, Vector3::new(0.0, 0.0, board::T));
    let mut pcb: CSolid = solid.compress();
    let raw = std::env::var_os("TRUCK_RAW_SWEEP").is_some();
    if raw {
        notes.push("board side walls left as SURFACE_OF_LINEAR_EXTRUSION (TRUCK_RAW_SWEEP)".into());
    } else {
        let (planes, cylinders) = analytic_walls(&mut pcb);
        notes.push(format!(
            "board: tsweep makes every side wall a SURFACE_OF_LINEAR_EXTRUSION; harness replaced them by hand with {planes} PLANE and {cylinders} CYLINDRICAL_SURFACE (TRUCK_RAW_SWEEP=1 keeps truck's own output)"
        ));
    }

    // Component, through truck's own reader.
    let (table, _) = parse(component)?;
    let comp_assy: Option<StepAssembly> = match table.step_assy() {
        Ok(a) if a.all_nodes().any(|n| !node_shells(n.shape()).is_empty()) => Some(a),
        Ok(_) => None,
        Err(e) => {
            notes.push(format!("component step_assy failed: {e}"));
            None
        }
    };
    // Fallback when the reader's product structure holds no B-rep: every solid in the file.
    let mut flat_solids: Vec<CSolid> = Vec::new();
    if comp_assy.is_none() {
        let mut ids: Vec<_> = table.manifold_solid_brep.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let solid = table
                .to_compressed_solid(&table.manifold_solid_brep[&id])
                .map_err(|e| format!("component solid #{id}: {e}"))?;
            flat_solids.push(solid);
        }
        if flat_solids.is_empty() {
            return Err("component: no MANIFOLD_SOLID_BREP readable".into());
        }
        notes.push(format!(
            "component: step_assy exposes no B-rep for this file, so its {} solids were taken straight from the entity table as one product; internal placements and product names lost",
            flat_solids.len()
        ));
    }

    let mut assy = Assembly::<Vec<Model<'_>>, PartAttrs, Matrix4, PartAttrs>::new();
    let root = assy.create_node(NodeEntity {
        shape: Vec::new(),
        attrs: attrs("trial-board"),
    });
    let pcb_node = assy.create_node(NodeEntity {
        shape: vec![StepModel::from(&pcb)],
        attrs: attrs("PCB"),
    });
    assy.create_edge(
        root,
        pcb_node,
        EdgeEntity {
            matrix: Matrix4::identity(),
            attrs: attrs("PCB"),
        },
    );

    // Component definition nodes; U1 and U2 are two edges to the same node(s).
    let mut comp_tops = Vec::new();
    if let Some(comp) = &comp_assy {
        let mut map = std::collections::HashMap::new();
        for node in comp.all_nodes() {
            let shape = node
                .shape()
                .iter()
                .filter_map(|s| match s {
                    ProductShape::Solid(solid) => Some(StepModel::from(solid)),
                    ProductShape::Shells(shells) => Some(shells.iter().collect()),
                    ProductShape::Matrix(_) => None,
                })
                .collect();
            let index = assy.create_node(NodeEntity {
                shape,
                attrs: node.attrs().clone(),
            });
            map.insert(node.index(), index);
        }
        for node in comp.all_nodes() {
            for edge in node.edges() {
                let (from, to) = edge.nodes();
                let matrix = Matrix4::try_from(edge.matrix())
                    .map_err(|e| format!("component placement: {e}"))?;
                assy.create_edge(
                    map[&from],
                    map[&to],
                    EdgeEntity {
                        matrix,
                        attrs: edge.attrs().clone(),
                    },
                );
            }
        }
        comp_tops.extend(comp.top_nodes().map(|n| map[&n.index()]));
    } else {
        let name = std::path::Path::new(component)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        comp_tops.push(assy.create_node(NodeEntity {
            shape: flat_solids.iter().map(StepModel::from).collect(),
            attrs: attrs(&name),
        }));
    }
    let u1 = Matrix4::from_translation(Vector3::new(20.0, 10.0, board::T))
        * Matrix4::from_angle_z(Rad(std::f64::consts::FRAC_PI_2));
    let u2 = Matrix4::from_translation(Vector3::new(35.0, 20.0, board::T));
    for (name, matrix) in [("U1", u1), ("U2", u2)] {
        for &top in &comp_tops {
            assy.create_edge(
                root,
                top,
                EdgeEntity {
                    matrix,
                    attrs: attrs(name),
                },
            );
        }
    }
    notes.push(
        "U1/U2 are NEXT_ASSEMBLY_USAGE_OCCURRENCE names on one shared component product".into(),
    );
    notes.push(
        "missing: all colour (no STYLED_ITEM/COLOUR_RGB writer, and none read from the component)"
            .into(),
    );
    notes.push(
        "header: truck writes FILE_SCHEMA 'ISO-10303-203' (not a schema name) and it cannot be set through the API; a copy with CONFIG_CONTROL_DESIGN is written next to the output as *.schema-fixed.step"
            .into(),
    );

    let design = StepDesign::<Model<'_>, Vec<Model<'_>>, Matrix4>::new(assy);
    let header = StepHeaderDescriptor {
        file_name: "trial-board".into(),
        organization_system: "reda step-kernel-trial truck-master".into(),
        ..Default::default()
    };
    let text = StepDisplay::new(header, design).to_string();
    std::fs::write(out_path, &text).map_err(|e| format!("write: {e}"))?;
    let fixed = text.replacen(
        "FILE_SCHEMA(('ISO-10303-203'))",
        "FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'))",
        1,
    );
    let fixed_path = std::path::Path::new(out_path).with_extension("schema-fixed.step");
    std::fs::write(&fixed_path, fixed).map_err(|e| format!("write: {e}"))?;
    Ok(text.len())
}
