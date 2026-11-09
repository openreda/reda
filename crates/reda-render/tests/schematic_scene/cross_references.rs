//! Port Cross-References drawn past each Port's flag.

use super::*;

fn port(engine: &mut TestEngine, x: i64, rotation: &str) -> Uuid {
    engine.run_ok(
        "schematic.port.add",
        json!({"document": SHEET, "name": "SDA", "at": at(x, 3000), "rotation": rotation}),
    )["result"]["port"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// The x range an object's drawing other than `text` covers, and `text`'s run's bounds.
fn reach(scene: &reda_render::Scene, object: Uuid, text: &str) -> ((i64, i64), (Point, Point)) {
    let (mut lo, mut hi) = (i64::MAX, i64::MIN);
    let mut note = None;
    for item in scene.items_of(object) {
        let bounds = item.primitive.bounds().unwrap();
        match &item.primitive {
            Primitive::Glyphs(run) if run.text == text => note = Some(bounds),
            _ => {
                lo = lo.min(bounds.0.x.0);
                hi = hi.max(bounds.1.x.0);
            }
        }
    }
    (
        (lo, hi),
        note.unwrap_or_else(|| panic!("no {text:?} drawn")),
    )
}

#[test]
fn a_ports_cross_reference_is_drawn_past_its_flag() {
    let dir = TempDir::new("render-cross-ref");
    let mut engine = demo(&dir);
    let right = port(&mut engine, 1000, "0deg");
    let left = port(&mut engine, 5000, "180deg");
    let bare = port(&mut engine, 8000, "0deg");
    let style = SchematicStyle {
        cross_references: [
            (right, "[2:B3]".to_owned()),
            (left, "[2:A1, 3:C4]".to_owned()),
        ]
        .into_iter()
        .collect(),
        ..SchematicStyle::default()
    };
    let scene = schematic_scene_styled(engine.project().unwrap(), sheet(), &[], &style).unwrap();

    let ((_, flag_end), (lo, _)) = reach(&scene, right, "[2:B3]");
    assert!(
        lo.x.0 > flag_end,
        "past the far end of a flag reaching right"
    );
    // Turned half a turn, the flag reaches left and the note is left of it, reading
    // left to right.
    let ((flag_end, _), (_, hi)) = reach(&scene, left, "[2:A1, 3:C4]");
    assert!(
        hi.x.0 < flag_end,
        "past the far end of a flag reaching left"
    );
    let texts = |uuid| {
        scene
            .items_of(uuid)
            .filter(|i| matches!(i.primitive, Primitive::Glyphs(_)))
            .count()
    };
    assert_eq!(texts(bare), 1, "a Port with no note draws only its name");
    assert_golden(golden("cross_references.scene"), scene.to_text().as_bytes());
}
