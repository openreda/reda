//! Display Cross-Overs: small bridges where Wires cross without connecting, drawn only
//! when the canvas asks for them.

use reda_render::ColorKey;

use super::*;

/// Adds a Wire as drawn: no Wire is split or merged, so crossings stay crossings.
fn wire(engine: &mut TestEngine, points: &[(i64, i64)]) -> Uuid {
    let points: Vec<_> = points.iter().map(|&(x, y)| at(x, y)).collect();
    let wiring = json!({"optimize_wires": false, "components_cut_wires": false,
                        "break_wires_at_autojunctions": false,
                        "convert_cross_junctions": false, "snap_grid": "100mil"});
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": points, "wiring": wiring}),
    )["result"]["wires"][0]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn strokes(scene: &reda_render::Scene, wire: Uuid) -> Vec<(Point, Point)> {
    scene
        .items_of(wire)
        .filter(|i| i.color == ColorKey::Schematic(SchematicColor::Wire))
        .map(|i| match i.primitive {
            Primitive::Capsule { a, b, .. } => (a, b),
            ref other => panic!("{other:?}"),
        })
        .collect()
}

fn with_cross_overs() -> SchematicStyle {
    SchematicStyle {
        cross_overs: true,
        ..SchematicStyle::default()
    }
}

fn mil(p: Point) -> (i64, i64) {
    (p.x.0 / 25_400, p.y.0 / 25_400)
}

#[test]
fn a_wire_bridges_another_where_they_cross_without_connecting() {
    let dir = TempDir::new("render-cross-over");
    let mut engine = demo(&dir);
    let across = wire(&mut engine, &[(1000, 1000), (3000, 1000)]);
    let down = wire(&mut engine, &[(2000, 500), (2000, 1500)]);
    let project = engine.project().unwrap();

    let plain = schematic_scene(project, sheet(), &[]).unwrap();
    assert_eq!(strokes(&plain, across).len(), 1, "off by default");

    let scene = schematic_scene_styled(project, sheet(), &[], &with_cross_overs()).unwrap();
    assert_eq!(strokes(&scene, down).len(), 1, "the upright Wire stays straight");
    let pieces = strokes(&scene, across);
    assert!(pieces.len() > 3, "{pieces:?}");
    // It still runs from end to end ...
    assert_eq!(mil(pieces[0].0), (1000, 1000));
    assert_eq!(mil(pieces.last().unwrap().1), (3000, 1000));
    for w in pieces.windows(2) {
        assert_eq!(w[0].1, w[1].0, "one unbroken line");
    }
    // ... but hops over the crossing: no stroke stays on the Wire's line there.
    let crossing = Point::new(2000.mil(), 1000.mil());
    for (a, b) in &pieces {
        let flat_through = a.y == crossing.y && b.y == crossing.y && a.x < crossing.x && b.x > crossing.x;
        assert!(!flat_through, "{a:?} {b:?}");
    }
    let top = pieces.iter().map(|(a, b)| a.y.max(b.y)).max().unwrap();
    assert!(top > crossing.y + Nm::from_um(500), "the bridge rises over the crossing");
    assert_golden(golden("cross_overs.scene"), scene.to_text().as_bytes());
}

#[test]
fn connected_wires_get_no_bridge() {
    let dir = TempDir::new("render-cross-over-connected");
    let mut engine = demo(&dir);
    // A T: one Wire ends on the other.
    wire(&mut engine, &[(1000, 1000), (3000, 1000)]);
    wire(&mut engine, &[(2000, 1000), (2000, 2000)]);
    // A crossing with a third Wire ending on it is joined there by a Junction (and stays
    // a crossing of two unbroken Wires, as nothing splits them).
    wire(&mut engine, &[(1000, 3000), (3000, 3000)]);
    wire(&mut engine, &[(2000, 2500), (2000, 3500)]);
    wire(&mut engine, &[(2000, 3000), (2500, 2500)]);
    let junctions = junctions(&mut engine);
    assert!(junctions.contains(&Point::new(2000.mil(), 3000.mil())));
    let project = engine.project().unwrap();
    let wires = |style: &SchematicStyle| -> Vec<_> {
        let scene = schematic_scene_styled(project, sheet(), &junctions, style).unwrap();
        scene
            .items
            .iter()
            .filter(|i| i.color == ColorKey::Schematic(SchematicColor::Wire))
            .cloned()
            .collect()
    };
    assert_eq!(wires(&with_cross_overs()), wires(&SchematicStyle::default()));
}
