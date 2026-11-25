//! `write`: the trial board (an extruded profile with holes cut in 2D) and the component model
//! read from a STEP file, as one AP242 assembly.

use std::collections::BTreeMap;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

use solidcore::geom::step_matrix::{IDENTITY, Matrix4};
use solidcore_ops::prism::extrude_planar_regions;
use solidcore_record::brep::Brep;
use solidcore_record::{Loop2, Plane, Region2, Seg2, Vec2};
use solidcore_step::write::product::{Colour, ExportBody, ExportProduct, Header, products};

use crate::json;
use crate::read::{Loaded, Paint, load};

fn v(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

/// The board's profile (SPEC.md): outer loop counter-clockwise, holes clockwise.
fn board_region() -> Region2 {
    let (w, h, r) = (50.0, 30.0, 3.0);
    let arc = |cx: f64, cy: f64, start: f64| Seg2::Arc { center: v(cx, cy), radius: r, start, sweep: FRAC_PI_2 };
    let line = |a: Vec2, b: Vec2| Seg2::Line { a, b };
    let outer = Loop2 {
        segs: vec![
            line(v(r, 0.0), v(w - r, 0.0)),
            arc(w - r, r, -FRAC_PI_2),
            line(v(w, r), v(w, h - r)),
            arc(w - r, h - r, 0.0),
            line(v(w - r, h), v(r, h)),
            arc(r, h - r, FRAC_PI_2),
            line(v(0.0, h - r), v(0.0, r)),
            arc(r, r, PI),
        ],
    };
    let mut holes: Vec<Loop2> = [(4.0, 4.0), (w - 4.0, 4.0), (w - 4.0, h - 4.0), (4.0, h - 4.0)]
        .iter()
        .map(|&(x, y)| Loop2 { segs: vec![Seg2::Arc { center: v(x, y), radius: 1.6, start: 0.0, sweep: -TAU }] })
        .collect();
    // The slot: 2 mm wide, its end centres 6 mm apart on y = 15, about (25, 15).
    holes.push(Loop2 {
        segs: vec![
            line(v(22.0, 16.0), v(28.0, 16.0)),
            Seg2::Arc { center: v(28.0, 15.0), radius: 1.0, start: FRAC_PI_2, sweep: -PI },
            line(v(28.0, 14.0), v(22.0, 14.0)),
            Seg2::Arc { center: v(22.0, 15.0), radius: 1.0, start: -FRAC_PI_2, sweep: -PI },
        ],
    });
    Region2 { outer, holes }
}

fn colour(p: Paint) -> Colour {
    Colour { rgb: p.rgb.map(|c| c.clamp(0.0, 1.0) as f32), opacity: p.opacity.clamp(0.0, 1.0) as f32, name: String::new() }
}

/// A rigid placement: `rotation` about +Z (radians), then the translation. solidcore's STEP
/// matrices put the axes in rows 0..3 and the origin in row 3, the same sixteen numbers the
/// writer calls column-major.
fn placement(at: [f64; 3], rotation: f64) -> Matrix4 {
    let (s, c) = rotation.sin_cos();
    [[c, s, 0.0, 0.0], [-s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [at[0], at[1], at[2], 1.0]]
}

fn body<'a>(loaded: &Loaded, solid: &'a solidcore_step::topology::Solid) -> ExportBody<'a> {
    let whole = loaded.paint.get(&solid.item).copied();
    let faces = solid
        .face_origin
        .iter()
        .enumerate()
        .filter_map(|(f, origin)| loaded.paint.get(origin).map(|p| (f, colour(*p))))
        .collect();
    ExportBody { name: solid.name.clone(), record: &solid.brep, color: whole.map(colour), faces }
}

fn build(out: &str, component: &str, notes: &mut Vec<String>) -> Result<usize, String> {
    // The board: one region with its holes, extruded. No 3D boolean is called for one region.
    let board = extrude_planar_regions(&Plane::XY, &[board_region()], 0.0, 1.6).map_err(|e| format!("board extrude: {e}"))?;
    let (board, _order): (Brep, _) = board.to_record().map_err(|e| format!("board record: {e}"))?;
    notes.push(format!("board record: {} faces, {} edges, {} vertices", board.faces.len(), board.edges.len(), board.vertices.len()));

    let bytes = std::fs::read(component).map_err(|e| format!("read {component}: {e}"))?;
    let loaded = load(&String::from_utf8_lossy(&bytes))?;
    if !loaded.errors.is_empty() {
        notes.push(format!("component: {} read failures, first: {}", loaded.errors.len(), json::shortened(&loaded.errors[0])));
    }
    let part_name = loaded
        .structure
        .as_ref()
        .and_then(|s| s.roots.first().and_then(|r| s.definitions.get(*r)).and_then(|d| s.products.get(d.product)))
        .map(|p| p.name.clone())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "component".into());

    let green = Colour { rgb: [0.0, 0.5, 0.2], opacity: 1.0, name: String::new() };
    let mut list: Vec<ExportProduct> = vec![
        ExportProduct {
            name: "trial-board".into(),
            bodies: Vec::new(),
            children: vec![
                (1, IDENTITY, "PCB".into()),
                (2, placement([20.0, 10.0, 1.6], FRAC_PI_2), "U1".into()),
                (2, placement([35.0, 20.0, 1.6], 0.0), "U2".into()),
            ],
        },
        ExportProduct { name: "PCB".into(), bodies: vec![ExportBody { name: "PCB".into(), record: &board, color: Some(green), faces: Vec::new() }], children: Vec::new() },
        ExportProduct { name: part_name, bodies: Vec::new(), children: Vec::new() },
    ];
    // The component's solids: one placed by the identity is a body of the component's product;
    // any other is a product of its own, placed once per instance.
    let mut own: BTreeMap<usize, usize> = BTreeMap::new();
    let (mut direct, mut nested) = (0usize, 0usize);
    for (i, m) in &loaded.placed {
        let Some(solid) = loaded.solids.solids.get(*i) else { continue };
        if *m == IDENTITY {
            list[2].bodies.push(body(&loaded, solid));
            direct += 1;
        } else {
            let at = match own.get(i) {
                Some(at) => *at,
                None => {
                    list.push(ExportProduct { name: solid.name.clone(), bodies: vec![body(&loaded, solid)], children: Vec::new() });
                    own.insert(*i, list.len() - 1);
                    list.len() - 1
                }
            };
            let name = solid.name.clone();
            list[2].children.push((at, *m, name));
            nested += 1;
        }
    }
    notes.push(format!(
        "component: {} solids re-written from their records ({direct} in the component's product, {nested} as placed sub-products), {} styled items carried; U1 and U2 are two occurrences of one product",
        loaded.placed.len(),
        loaded.paint.len(),
    ));
    let header = Header {
        file_name: "solidcore.step".into(),
        description: "REDA issue #442 STEP trial board".into(),
        preprocessor_version: "solidcore-step 34d50552".into(),
        originating_system: "step-trial-solidcore".into(),
        ..Header::default()
    };
    let text = products(&list, 0, &header).map_err(|e| format!("write: {e}"))?;
    std::fs::write(out, &text).map_err(|e| format!("write {out}: {e}"))?;
    Ok(text.len())
}

pub fn run(out: &str, component: &str) -> String {
    let mut notes = Vec::new();
    let result = build(out, component, &mut notes);
    let (ok, error, bytes) = match &result {
        Ok(n) => (true, None, Some(*n)),
        Err(e) => (false, Some(json::shortened(e)), None),
    };
    // The schema is the file's own FILE_SCHEMA line.
    let schema = std::fs::read_to_string(out)
        .ok()
        .filter(|_| ok)
        .and_then(|t| t.lines().find(|l| l.starts_with("FILE_SCHEMA")).map(str::to_string));
    format!(
        "{{\"ok\":{ok},\"error\":{},\"bytes\":{},\"schema\":{},\"notes\":{}}}",
        json::opt_string(error.as_deref()),
        json::opt_count(bytes),
        json::opt_string(schema.as_deref()),
        json::string(&notes.join("; ")),
    )
}
