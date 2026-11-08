//! Not Fitted Components: a masked body under a `cross` or `mesh` overlay.

use reda_render::ColorKey;

use super::*;

fn colors_of(scene: &reda_render::Scene, unit: &str) -> Vec<SchematicColor> {
    scene
        .items_of(unit.parse().unwrap())
        .map(|i| match i.color {
            ColorKey::Schematic(c) => c,
            other => panic!("{other:?}"),
        })
        .collect()
}

fn count(colors: &[SchematicColor], color: SchematicColor) -> usize {
    colors.iter().filter(|c| **c == color).count()
}

/// A fitted resistor and a Not Fitted one.
fn sheet_with_not_fitted(engine: &mut TestEngine) -> (String, String) {
    let (fitted, _) = place(engine, R_10K, 1000, 1000, 0);
    let (unit, component) = place(engine, R_10K, 3000, 1000, 0);
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "not_fitted": true}),
    );
    (fitted, unit)
}

#[test]
fn a_not_fitted_component_is_crossed_out_in_red_over_a_masked_body_by_default() {
    let dir = TempDir::new("render-not-fitted-cross");
    let mut engine = demo(&dir);
    let (fitted, unit) = sheet_with_not_fitted(&mut engine);
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    let colors = colors_of(&scene, &unit);
    assert_eq!(count(&colors, SchematicColor::Body), 0, "{colors:?}");
    assert_eq!(count(&colors, SchematicColor::Pin), 0, "{colors:?}");
    assert!(count(&colors, SchematicColor::MaskedBody) > 0, "{colors:?}");
    // Two Pins and their two numbers.
    assert_eq!(count(&colors, SchematicColor::MaskedPin), 4, "{colors:?}");
    assert_eq!(count(&colors, SchematicColor::NotFittedCross), 2, "{colors:?}");
    // The cross spans the body from corner to corner.
    let body = body_box(&scene, &unit);
    for item in scene.items_of(unit.parse().unwrap()) {
        if item.color == ColorKey::Schematic(SchematicColor::NotFittedCross) {
            let Primitive::Capsule { a, b, .. } = item.primitive else {
                panic!("{item:?}");
            };
            let (lo, hi) = (a.x.min(b.x), a.x.max(b.x));
            assert!(lo <= body.0.x && hi >= body.1.x, "{a:?} {b:?} {body:?}");
            assert_ne!(a.y, b.y, "a diagonal");
        }
    }
    let fitted = colors_of(&scene, &fitted);
    assert!(count(&fitted, SchematicColor::Body) > 0);
    assert_eq!(count(&fitted, SchematicColor::NotFittedCross), 0);
    assert_golden(golden("not_fitted_cross.scene"), scene.to_text().as_bytes());
}

/// The box a Unit's body (its graphics and Pins, not their text) covers.
fn body_box(scene: &reda_render::Scene, unit: &str) -> (Point, Point) {
    scene
        .items_of(unit.parse().unwrap())
        .filter(|i| {
            matches!(
                i.color,
                ColorKey::Schematic(SchematicColor::MaskedBody | SchematicColor::MaskedPin)
            )
        })
        .filter(|i| !matches!(i.primitive, Primitive::Glyphs(_)))
        .filter_map(|i| i.primitive.bounds())
        .reduce(|(lo, hi), (l, h)| {
            (
                Point::new(lo.x.min(l.x), lo.y.min(l.y)),
                Point::new(hi.x.max(h.x), hi.y.max(h.y)),
            )
        })
        .unwrap()
}

#[test]
fn the_mesh_display_draws_a_grey_meshed_box_over_the_masked_body() {
    let dir = TempDir::new("render-not-fitted-mesh");
    let mut engine = demo(&dir);
    let (_, unit) = sheet_with_not_fitted(&mut engine);
    engine.run_ok(
        "schematic.options.set",
        json!({"not_fitted_display": "mesh"}),
    );
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    let colors = colors_of(&scene, &unit);
    assert_eq!(count(&colors, SchematicColor::NotFittedCross), 0);
    assert_eq!(count(&colors, SchematicColor::Body), 0, "masked");
    let body = body_box(&scene, &unit);
    let mesh: Vec<_> = scene
        .items_of(unit.parse().unwrap())
        .filter(|i| i.color == ColorKey::Schematic(SchematicColor::NotFittedMesh))
        .collect();
    assert!(mesh.len() > 4, "a box and its mesh: {mesh:?}");
    let boxes = mesh
        .iter()
        .filter(|i| matches!(i.primitive, Primitive::Rect { stroke: Some(_), .. }))
        .count();
    assert_eq!(boxes, 1);
    for item in &mesh {
        let (lo, hi) = item.primitive.bounds().unwrap();
        let slack = Nm::from_um(500);
        assert!(
            lo.x >= body.0.x - slack
                && lo.y >= body.0.y - slack
                && hi.x <= body.1.x + slack
                && hi.y <= body.1.y + slack,
            "the mesh stays in the box: {item:?} {body:?}"
        );
    }
    assert_golden(golden("not_fitted_mesh.scene"), scene.to_text().as_bytes());
}

#[test]
fn the_none_display_draws_a_not_fitted_component_as_any_other() {
    let dir = TempDir::new("render-not-fitted-none");
    let mut engine = demo(&dir);
    let (fitted, unit) = sheet_with_not_fitted(&mut engine);
    engine.run_ok(
        "schematic.options.set",
        json!({"not_fitted_display": "none"}),
    );
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    let mut colors = colors_of(&scene, &unit);
    let mut expected = colors_of(&scene, &fitted);
    colors.sort_by_key(|c| c.name());
    expected.sort_by_key(|c| c.name());
    assert_eq!(colors, expected);
}
