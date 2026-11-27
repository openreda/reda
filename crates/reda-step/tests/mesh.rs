//! Meshing models: each fixture's faces tessellated into closed, coloured meshes in mm, with
//! the assembly tree, placements and bounding box. The expected volumes and boxes are the ones
//! each fixture was drawn with (tests/fixtures/README.md), as FreeCAD (OpenCascade) measured
//! them, not values derived from the mesher.

use reda_step::records::Face;
use reda_step::{Colour, Loaded, Mesh, MeshOptions, Node, Transform, load};

fn fixture(name: &str) -> Loaded {
    fixture_with(name, &MeshOptions::default())
}

fn fixture_with(name: &str, options: &MeshOptions) -> Loaded {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let loaded = load(&bytes, options).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert!(loaded.warnings.is_empty(), "{name}: {:?}", loaded.warnings);
    loaded
}

fn assert_close(what: &str, got: f64, want: f64, relative: f64) {
    assert!(
        (got - want).abs() <= relative * want.abs(),
        "{what}: {got} is not within {}% of {want}",
        relative * 100.0
    );
}

/// The one solid of a single-part fixture meshes closed at the expected volume.
fn assert_solid(name: &str, volume: f64) {
    let loaded = fixture(name);
    let meshed = &loaded.meshed;
    let solids: Vec<_> = meshed.parts.iter().flat_map(|p| &p.solids).collect();
    assert_eq!(solids.len(), 1, "{name}: one solid");
    let mesh = &solids[0].mesh;
    assert!(!mesh.triangles.is_empty(), "{name}: triangles");
    assert!(mesh.is_closed(), "{name}: the mesh is closed");
    assert_normals_agree(name, mesh);
    // The default tolerances keep the volume within the corpus grade of 1 %.
    assert_close(name, mesh.volume(), volume, 0.01);
    assert_close(name, meshed.volume(), volume, 0.01);

    // Finer tolerances close in on the exact volume.
    let fine = MeshOptions {
        chord: 0.0005,
        angle: 2f64.to_radians(),
    };
    let fine = fixture_with(name, &fine);
    assert_close(name, fine.meshed.volume(), volume, 0.001);
    let mesh = &fine.meshed.parts[0].solids[0].mesh;
    assert!(mesh.is_closed(), "{name}: the fine mesh is closed");
}

#[test]
fn a_block_of_planes_and_lines_meshes_closed() {
    assert_solid("ap214_face_colours.step", 1000.0);
}

#[test]
fn a_cylinder_with_its_seam_meshes_closed() {
    // AP203, r = 2, h = 5: two circles, the seam line, a cylindrical face with pcurves.
    assert_solid("ap203_cylinder.step", 62.832);
}

#[test]
fn a_cone_frustum_meshes_closed() {
    // r = 3 at the base, 1.5 at the top, h = 4: π h (R² + R r + r²) / 3.
    assert_solid("cone_frustum.step", 65.973446);
}

#[test]
fn a_cone_to_its_apex_meshes_closed() {
    // r = 2, h = 5, the seam line running into the apex.
    assert_solid("cone_apex.step", 20.943951);
}

#[test]
fn a_whole_sphere_bounded_by_a_vertex_loop_meshes_closed() {
    // r = 3, one face with only a vertex loop at its pole.
    assert_solid("sphere.step", 113.097336);
}

#[test]
fn a_whole_torus_between_two_seams_meshes_closed() {
    // R = 5, r = 1: one face bounded by its two seam circles, each used both ways.
    assert_solid("torus.step", 98.696044);
}

#[test]
fn a_cylinder_cut_on_a_slant_meshes_closed_along_its_ellipse() {
    // r = 2 from z = -5 up to a plane tilted 30° through the origin.
    assert_solid("ellipse_cut.step", 62.831853);
}

#[test]
fn a_plate_with_holes_meshes_closed_around_its_inner_bounds() {
    // 20 x 10 x 1.6 with three holes of r = 0.75, 1 and 1.25.
    assert_solid("plate_with_holes.step", 304.292037);
}

#[test]
fn a_dome_meets_its_pole_and_its_cylinder() {
    // A cylinder r = 2.5, h = 5 under a hemisphere whose meridian seam runs into the pole.
    assert_solid("led_dome.step", 130.899694);
}

#[test]
fn a_filleted_pin_meshes_its_part_of_a_torus() {
    // A cylinder r = 1, h = 3, its top edge rounded with r = 0.3.
    assert_solid("rounded_pin.step", 9.311555);
}

#[test]
fn a_cone_cut_along_its_axis_meshes_closed_along_its_hyperbola() {
    // A cone r = 3, h = 6 cut by the plane x = 1: two hyperbola edges, the apex kept.
    assert_solid("hyperbola_cut.step", 44.529942);
}

#[test]
fn a_cone_cut_along_a_generator_meshes_closed_along_its_parabola() {
    // The same cone cut by a plane parallel to a generator: one parabola edge, the apex cut
    // away.
    assert_solid("parabola_cut.step", 29.991623);
}

#[test]
fn a_model_in_metres_is_meshed_in_millimetres() {
    // The parabola cut with its length unit made the metre: every length, the parabola's
    // focal distance among them, grows 1000 times.
    let text = fixture_text("parabola_cut.step");
    let metres = text.replace("SI_UNIT(.MILLI.,.METRE.)", "SI_UNIT($,.METRE.)");
    assert_ne!(metres, text);
    let loaded = load(metres.as_bytes(), &MeshOptions::default()).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let mesh = &loaded.meshed.parts[0].solids[0].mesh;
    assert!(mesh.is_closed());
    assert_close("parabola cut in metres", mesh.volume(), 29.991623e9, 0.001);
    let b = loaded.meshed.bounding_box().unwrap();
    assert!((b.max[2] - 4000.0).abs() < 1.0, "{b:?}");
}

#[test]
fn a_degenerate_torus_meets_its_axis_at_a_pole() {
    // The upper half of a spindle torus R = 1, r = 2 on a disk: an apple with a dimple at its
    // stem, its meridian seam running into the pole on the axis.
    assert_solid("half_apple.step", 42.643139);
}

#[test]
fn a_void_is_meshed_facing_into_its_solid() {
    // A 4 x 4 x 4 block with a spherical cavity r = 1: BREP_WITH_VOIDS, the void an
    // ORIENTED_CLOSED_SHELL used inside out. 64 - 4π/3.
    assert_solid("hollow_block.step", 59.81121);
    let solid = &fixture("hollow_block.step").meshed.parts[0].solids[0];
    assert_eq!(solid.mesh.faces.len(), 7);
}

fn fixture_text(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn an_ellipse_is_cut_the_same_whichever_semi_axis_comes_first() {
    // The same ellipse written with its semi-axes the other way round: semi_axis_2 the longer,
    // the reference direction turned a quarter turn about the axis to match.
    let text = fixture_text("ellipse_cut.step");
    let swapped = text
        .replace(
            "#24 = ELLIPSE('',#25,2.309401076759,2.);",
            "#24 = ELLIPSE('',#25,2.,2.309401076759);",
        )
        .replace(
            "#28 = DIRECTION('',(0.,0.866025403784,0.5));",
            "#28 = DIRECTION('',(-1.,0.,0.));",
        );
    assert_eq!(swapped.matches("2.,2.309401076759").count(), 1);
    assert!(swapped.contains("(-1.,0.,0.)"));
    // A loose angle, so the chord tolerance alone sets the ellipse's steps.
    let options = MeshOptions {
        chord: 0.01,
        angle: 1.0,
    };
    let triangles = |text: &str| {
        let loaded = load(text.as_bytes(), &options).unwrap();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let mesh = &loaded.meshed.parts[0].solids[0].mesh;
        assert!(mesh.is_closed());
        assert_close("ellipse cut", mesh.volume(), 62.831853, 0.01);
        mesh.triangles.len()
    };
    assert_eq!(triangles(&swapped), triangles(&text));
}

#[test]
fn a_face_that_cannot_be_meshed_is_reported_and_the_rest_drawn() {
    let path = format!(
        "{}/tests/fixtures/ap214_face_colours.step",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    // The bottom face's plane becomes a sphere of negative radius.
    let broken = text.replace("#60=PLANE('',#59);", "#60=SPHERICAL_SURFACE('',#59,-1.);");
    assert_ne!(broken, text);
    let loaded = load(broken.as_bytes(), &MeshOptions::default()).unwrap();
    let solid = &loaded.meshed.parts[0].solids[0];
    assert_eq!(solid.mesh.faces.len(), 5, "the other faces are drawn");
    assert!(!solid.mesh.is_closed());
    assert_eq!(
        loaded.meshed.warnings.len(),
        1,
        "{:?}",
        loaded.meshed.warnings
    );
    let w = &loaded.meshed.warnings[0];
    assert!(
        w.starts_with("face #") && w.contains("its radius is not positive"),
        "{w}"
    );
    assert_eq!(loaded.warnings, loaded.meshed.warnings);
}

/// Each triangle's winding and its vertices' normals point the same way (outwards, since the
/// volume comes out positive).
fn assert_normals_agree(name: &str, mesh: &Mesh) {
    for t in &mesh.triangles {
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        let (u, v) = (sub(b, a), sub(c, a));
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for &i in t {
            let vn = mesh.normals[i as usize];
            let d = n[0] * vn[0] + n[1] * vn[1] + n[2] * vn[2];
            assert!(
                d >= 0.0,
                "{name}: a vertex normal faces against its triangle"
            );
        }
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn assert_box(name: &str, min: [f64; 3], max: [f64; 3]) {
    let b = fixture(name)
        .meshed
        .bounding_box()
        .expect("something drawn");
    for i in 0..3 {
        // The mesh's vertices lie on the surface, so the box is within the chord tolerance.
        assert!(
            (b.min[i] - min[i]).abs() <= 0.01 && (b.max[i] - max[i]).abs() <= 0.01,
            "{name}: box {:?} {:?}, want {min:?} {max:?}",
            b.min,
            b.max
        );
    }
}

#[test]
fn the_bounding_box_is_the_models() {
    assert_box(
        "ap214_face_colours.step",
        [0.0, 0.0, 0.0],
        [10.0, 20.0, 5.0],
    );
    assert_box("cone_frustum.step", [-3.0, -3.0, 0.0], [3.0, 3.0, 4.0]);
    assert_box("sphere.step", [-3.0; 3], [3.0; 3]);
    assert_box("torus.step", [-6.0, -6.0, -1.0], [6.0, 6.0, 1.0]);
    assert_box("ellipse_cut.step", [-2.0, -2.0, -5.0], [2.0, 2.0, 1.154701]);
    assert_box("led_dome.step", [-2.5, -2.5, 0.0], [2.5, 2.5, 7.5]);
    assert_box("rounded_pin.step", [-1.0, -1.0, 0.0], [1.0, 1.0, 3.0]);
    assert_box("half_apple.step", [-3.0, -3.0, 0.0], [3.0, 3.0, 2.0]);
    assert_box("hyperbola_cut.step", [-3.0, -3.0, 0.0], [1.0, 3.0, 6.0]);
    assert_box("parabola_cut.step", [-3.0, -3.0, 0.0], [1.0, 3.0, 4.0]);
}

#[test]
fn an_inch_model_is_meshed_in_millimetres() {
    // A 1 x 0.5 x 0.25 in block.
    assert_box("inch_block.step", [0.0; 3], [25.4, 12.7, 6.35]);
    let volume = fixture("inch_block.step").meshed.volume();
    assert_close("inch block", volume, 2048.383, 1e-6);
}

#[test]
fn each_face_carries_its_own_colour() {
    let loaded = fixture("ap214_face_colours.step");
    let solid = &loaded.meshed.parts[0].solids[0];
    assert_eq!(solid.colour, None);
    let mut by_face: Vec<(String, Option<Colour>)> = solid
        .mesh
        .faces
        .iter()
        .map(|f| {
            assert!(!f.triangles.is_empty());
            let name = loaded.model.get::<Face>(f.face).unwrap().name.clone();
            (name, f.colour)
        })
        .collect();
    by_face.sort_by(|a, b| a.0.cmp(&b.0));
    let rgb = |r, g, b| Some(Colour::rgb(r, g, b));
    assert_eq!(
        by_face,
        [
            ("back".to_string(), rgb(0.0, 1.0, 0.0)),
            ("bottom".to_string(), rgb(0.1, 0.1, 0.1)),
            ("front".to_string(), rgb(1.0, 0.0, 0.0)),
            ("left".to_string(), rgb(0.0, 0.0, 1.0)),
            ("right".to_string(), rgb(1.0, 0.5, 0.0)),
            ("top".to_string(), rgb(1.0, 1.0, 1.0)),
        ]
    );
}

#[test]
fn a_solids_colour_and_transparency_reach_every_face() {
    let loaded = fixture("ap242_solid_colour.step");
    let solid = &loaded.meshed.parts[0].solids[0];
    let red = Colour {
        red: 0.8,
        green: 0.1,
        blue: 0.1,
        alpha: 0.75,
    };
    assert_eq!(solid.colour, Some(red));
    assert_eq!(solid.mesh.faces.len(), 6);
    assert!(solid.mesh.faces.iter().all(|f| f.colour == Some(red)));
    assert!(solid.mesh.is_closed());
    assert_close("AP242 block", solid.mesh.volume(), 24.0, 1e-9);
}

fn assert_transform(t: &Transform, rotation: [[f64; 3]; 3], translation: [f64; 3]) {
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(
        (0..3).all(|i| near(t.translation[i], translation[i])
            && (0..3).all(|j| near(t.rotation[i][j], rotation[i][j]))),
        "{t:?}, want {rotation:?} {translation:?}"
    );
}

const UNTURNED: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

#[test]
fn a_nested_assembly_keeps_its_tree_and_placements() {
    let loaded = fixture("nested_assembly.step");
    let m = &loaded.meshed;
    let [root] = &m.roots[..] else {
        panic!(
            "one root: {:?}",
            m.roots.iter().map(|r| &r.name).collect::<Vec<_>>()
        )
    };
    assert_eq!(root.name, "board-assy");
    assert_eq!(root.occurrence, None);
    let names = |n: &Node| {
        n.children
            .iter()
            .map(|c| (c.name.clone(), c.occurrence.clone().unwrap_or_default()))
            .collect::<Vec<_>>()
    };
    let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(
        names(root),
        [
            pair("module", "module:1"),
            pair("pin", "pin:3"),
            pair("cap", "cap:1")
        ]
    );
    let module = &root.children[0];
    assert_eq!(names(module), [pair("pin", "pin:1"), pair("pin", "pin:2")]);
    assert_eq!(module.part, None, "the module has no solids of its own");

    // Placements, as FreeCAD reads them: the module at (10, 0, 1.6) turned 90° about z, its
    // pins at 0 and 2.54 along its x; pin:3 at (-5, 5, 1.6); the cap at (0, -5, 0) flipped
    // about x.
    let quarter = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let flipped = [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]];
    assert_transform(&module.transform, quarter, [10.0, 0.0, 1.6]);
    assert_transform(&module.children[0].transform, UNTURNED, [0.0; 3]);
    assert_transform(&module.children[1].transform, UNTURNED, [2.54, 0.0, 0.0]);
    assert_transform(&root.children[1].transform, UNTURNED, [-5.0, 5.0, 1.6]);
    assert_transform(&root.children[2].transform, flipped, [0.0, -5.0, 0.0]);

    // The three pins share one meshed part.
    let pin = module.children[0].part.expect("the pin has a shape");
    assert_eq!(module.children[1].part, Some(pin));
    assert_eq!(root.children[1].part, Some(pin));
    assert_eq!(m.parts.len(), 2);
    let instances = m.instances();
    assert_eq!(instances.len(), 4);
    assert_eq!(instances.iter().filter(|i| i.index == pin).count(), 3);

    // Per-solid colours: the pins in the predefined yellow, the cap in RGB.
    let yellow = Some(Colour::rgb(1.0, 1.0, 0.0));
    assert_eq!(m.parts[pin].solids[0].colour, yellow);
    assert!(
        m.parts[pin].solids[0]
            .mesh
            .faces
            .iter()
            .all(|f| f.colour == yellow)
    );
    let cap = root.children[2].part.unwrap();
    assert_eq!(
        m.parts[cap].solids[0].colour,
        Some(Colour::rgb(0.2, 0.2, 0.8))
    );

    // FreeCAD's volume and box of the whole assembly.
    assert_close("assembly", m.volume(), 12.0, 1e-9);
    let b = m.bounding_box().unwrap();
    let want = ([-5.5, -5.5, -3.0], [10.5, 5.5, 4.6]);
    for i in 0..3 {
        let ok = (b.min[i] - want.0[i]).abs() < 1e-9 && (b.max[i] - want.1[i]).abs() < 1e-9;
        assert!(ok, "{b:?}");
    }
}

#[test]
fn a_stpz_file_meshes_as_its_step_twin() {
    let step = fixture("nested_assembly.step").meshed;
    let stpz = fixture("nested_assembly.stpz").meshed;
    assert_eq!(step, stpz);
}

#[test]
fn mapped_items_place_one_shape_twice() {
    // A 2 x 1 x 0.5 in tile, in a shape no product names, placed by two MAPPED_ITEMs: at the
    // origin, and 5 in along x turned 90° about z. As FreeCAD reads it: 2 in³ and a box to
    // (5, 2, 0.5) in.
    let loaded = fixture("mapped_items.step");
    let m = &loaded.meshed;
    let [root] = &m.roots[..] else {
        panic!(
            "one root: {:?}",
            m.roots.iter().map(|r| &r.name).collect::<Vec<_>>()
        )
    };
    assert_eq!(root.name, "tile array");
    assert_eq!(root.part, None, "the array has no solids of its own");
    let uses: Vec<_> = root
        .children
        .iter()
        .map(|c| (c.name.as_str(), c.occurrence.as_deref()))
        .collect();
    assert_eq!(uses, [("tile", Some("tile:1")), ("tile", Some("tile:2"))]);
    let quarter = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    assert_transform(&root.children[0].transform, UNTURNED, [0.0; 3]);
    assert_transform(&root.children[1].transform, quarter, [127.0, 0.0, 0.0]);
    let tile = root.children[0].part.expect("the tile is drawn");
    assert_eq!(root.children[1].part, Some(tile));
    assert_eq!(m.parts.len(), 1);
    let green = Some(Colour::rgb(0.1, 0.6, 0.2));
    assert_eq!(m.parts[tile].solids[0].colour, green);

    assert_close("tiles", m.volume(), 32774.128, 1e-6);
    let b = m.bounding_box().unwrap();
    let want = ([0.0; 3], [127.0, 50.8, 12.7]);
    for i in 0..3 {
        let ok = (b.min[i] - want.0[i]).abs() < 1e-9 && (b.max[i] - want.1[i]).abs() < 1e-9;
        assert!(ok, "{b:?}");
    }
}

#[test]
fn an_item_the_mesher_does_not_draw_is_reported() {
    // A loose point among a shape's items is neither a solid nor a placement.
    let text = fixture_text("ap214_face_colours.step");
    let (at, _) = text
        .match_indices("ADVANCED_BREP_SHAPE_REPRESENTATION(")
        .next()
        .unwrap();
    let line_end = at + text[at..].find(";\n").unwrap();
    let line = &text[at..line_end];
    let items_end = line.find("),").unwrap();
    let with_point = format!("{},#9999{}", &line[..items_end], &line[items_end..]);
    let broken = text.replacen(line, &with_point, 1).replacen(
        "ENDSEC;\nEND-ISO",
        "#9999=CARTESIAN_POINT('loose',(1.,2.,3.));\nENDSEC;\nEND-ISO",
        1,
    );
    let loaded = load(broken.as_bytes(), &MeshOptions::default()).unwrap();
    assert_eq!(
        loaded.meshed.parts[0].solids.len(),
        1,
        "the solid is still drawn"
    );
    let [w] = &loaded.meshed.warnings[..] else {
        panic!("one warning: {:?}", loaded.meshed.warnings)
    };
    assert!(
        w.contains("CARTESIAN_POINT") && w.contains("not drawn"),
        "{w}"
    );
}
