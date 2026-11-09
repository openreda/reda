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

#[test]
fn each_cross_reference_entry_is_a_link_to_its_sheet() {
    let dir = TempDir::new("render-cross-ref-links");
    let mut engine = demo(&dir);
    let right = port(&mut engine, 1000, "0deg");
    let left = port(&mut engine, 5000, "180deg");
    let (two, three) = (Uuid::v4_from_bits(0x2), Uuid::v4_from_bits(0x3));
    let target = |entry: &str, sheet: Uuid, x: i64| NoteLink {
        entry: entry.into(),
        sheet,
        at: Point::new(Nm::from_mm(x), Nm::from_mm(10)),
    };
    let style = SchematicStyle {
        cross_references: [
            (right, "[2:B3]".to_owned()),
            (left, "[2:A1, 3:C4]".to_owned()),
        ]
        .into_iter()
        .collect(),
        cross_reference_links: [
            (right, vec![target("2:B3", two, 1)]),
            (left, vec![target("2:A1", two, 2), target("3:C4", three, 3)]),
        ]
        .into_iter()
        .collect(),
        ..SchematicStyle::default()
    };
    let scene = schematic_scene_styled(engine.project().unwrap(), sheet(), &[], &style).unwrap();
    let links: Vec<_> = scene
        .links
        .iter()
        .map(|l| (l.object, l.sheet, l.at))
        .collect();
    assert_eq!(
        links,
        [
            (right, two, Point::new(Nm::from_mm(1), Nm::from_mm(10))),
            (left, two, Point::new(Nm::from_mm(2), Nm::from_mm(10))),
            (left, three, Point::new(Nm::from_mm(3), Nm::from_mm(10))),
        ]
    );
    let inside = |(lo, hi): (Point, Point), (blo, bhi): (Point, Point)| {
        lo.x >= blo.x - Nm(1) && hi.x <= bhi.x + Nm(1) && lo.y <= blo.y && hi.y >= bhi.y
    };
    let (_, note) = reach(&scene, right, "[2:B3]");
    let one = scene.links[0].area;
    assert!(inside(one, note), "{one:?} spans the note {note:?} tall");
    assert!(
        one.0.x > note.0.x && one.1.x < note.1.x,
        "the brackets are left out"
    );
    // The flag reaches left, and the note still reads left to right: the first entry is
    // left of the second, both inside the note.
    let (_, note) = reach(&scene, left, "[2:A1, 3:C4]");
    let (a, b) = (scene.links[1].area, scene.links[2].area);
    assert!(
        inside(a, note) && inside(b, note),
        "{a:?} {b:?} in {note:?}"
    );
    assert!(a.1.x < b.0.x, "reading order: {a:?} then {b:?}");
}
