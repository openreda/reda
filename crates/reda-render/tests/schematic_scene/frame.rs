//! The sheet frame: Border Zones, the title block and its tinted empty fields.

use super::*;

/// Every text run drawn in `color`, with its anchor.
fn texts(scene: &reda_render::Scene, color: SchematicColor) -> Vec<(String, Point)> {
    scene
        .items
        .iter()
        .filter(|i| i.color == reda_render::ColorKey::Schematic(color))
        .filter_map(|i| match &i.primitive {
            Primitive::Glyphs(run) => Some((run.text.clone(), run.at)),
            _ => None,
        })
        .collect()
}

fn canvas() -> SchematicStyle {
    SchematicStyle {
        canvas: true,
        ..SchematicStyle::default()
    }
}

fn mm_f(v: Nm) -> f64 {
    v.0 as f64 / 1e6
}

#[test]
fn border_zones_number_the_columns_and_letter_the_rows_on_every_side() {
    let dir = TempDir::new("render-zones");
    let mut engine = demo(&dir);
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    // A4 landscape, 297 x 210 mm: six columns of 49.5 mm and four rows of 52.5 mm.
    let zones = texts(&scene, SchematicColor::Zone);
    let named = |name: &str| -> Vec<Point> {
        zones.iter().filter(|(t, _)| t == name).map(|(_, p)| *p).collect()
    };
    for (i, column) in ["1", "2", "3", "4", "5", "6"].into_iter().enumerate() {
        let at = named(column);
        assert_eq!(at.len(), 2, "column {column} at the top and the bottom: {at:?}");
        let centre = 49.5 * i as f64 + 24.75;
        for p in &at {
            assert!((mm_f(p.x) - centre).abs() < 0.01, "column {column} at {p:?}");
        }
        assert!(at.iter().any(|p| mm_f(p.y) < 10.0) && at.iter().any(|p| mm_f(p.y) > 200.0));
    }
    for (i, row) in ["A", "B", "C", "D"].into_iter().enumerate() {
        let at = named(row);
        assert_eq!(at.len(), 2, "row {row} on the left and the right: {at:?}");
        // Row A is at the top.
        let centre = 210.0 - 52.5 * i as f64 - 26.25;
        for p in &at {
            assert!((mm_f(p.y) - centre).abs() < 0.01, "row {row} at {p:?}");
        }
        assert!(at.iter().any(|p| mm_f(p.x) < 10.0) && at.iter().any(|p| mm_f(p.x) > 287.0));
    }
    assert_eq!(zones.len(), 2 * (6 + 4));

    engine.run_ok(
        "schematic.sheet.set_frame",
        json!({"document": SHEET, "border_zones": false}),
    );
    let off = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    assert!(texts(&off, SchematicColor::Zone).is_empty());
    assert!(
        !off.items
            .iter()
            .any(|i| i.color == reda_render::ColorKey::Schematic(SchematicColor::Zone)),
        "no zone dividers either"
    );
}

#[test]
fn a_sheet_with_its_title_block_off_draws_none() {
    let dir = TempDir::new("render-no-title-block");
    let mut engine = demo(&dir);
    engine.run_ok(
        "schematic.sheet.set_frame",
        json!({"document": SHEET, "title_block": false}),
    );
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    assert!(texts(&scene, SchematicColor::FrameText).is_empty());
    let frame_lines = scene
        .items
        .iter()
        .filter(|i| i.color == reda_render::ColorKey::Schematic(SchematicColor::Frame))
        .count();
    assert_eq!(frame_lines, 1, "only the border");
}

#[test]
fn the_title_block_shows_the_drawing_number_and_the_sheets_revision_or_the_projects() {
    let dir = TempDir::new("render-revision");
    let mut engine = demo(&dir);
    engine.run_ok("project.properties.set", json!({"revision": "B"}));
    let shown = |engine: &TestEngine, style: &SchematicStyle| {
        let scene = schematic_scene_styled(engine.project().unwrap(), sheet(), &[], style).unwrap();
        let mut all = texts(&scene, SchematicColor::FrameText);
        all.extend(texts(&scene, SchematicColor::FrameTextInherited));
        let inherited: Vec<String> = texts(&scene, SchematicColor::FrameTextInherited)
            .into_iter()
            .map(|t| t.0)
            .collect();
        (all.into_iter().map(|t| t.0).collect::<Vec<_>>(), inherited)
    };
    // No revision of its own: the Project's, greyed on the canvas only.
    let (all, inherited) = shown(&engine, &canvas());
    assert!(all.contains(&"Revision: B".to_owned()), "{all:?}");
    assert_eq!(inherited, ["Revision: B"]);
    assert!(shown(&engine, &SchematicStyle::default()).1.is_empty());

    engine.run_ok(
        "schematic.sheet.set_frame",
        json!({"document": SHEET, "drawing_number": "DWG-0042", "revision": "C"}),
    );
    let (all, inherited) = shown(&engine, &canvas());
    assert!(all.contains(&"Revision: C".to_owned()), "{all:?}");
    assert!(all.contains(&"Drawing number: DWG-0042".to_owned()), "{all:?}");
    assert!(inherited.is_empty());
}

#[test]
fn empty_title_fields_are_tinted_on_the_canvas_and_never_in_outputs() {
    let dir = TempDir::new("render-tint");
    let mut engine = demo(&dir);
    engine.run_ok(
        "project.properties.set",
        json!({"title": "Demo board", "author": "Ada"}),
    );
    let project = engine.project().unwrap();
    let tints = |scene: &reda_render::Scene| {
        scene
            .items
            .iter()
            .filter(|i| i.color == reda_render::ColorKey::Schematic(SchematicColor::EmptyField))
            .count()
    };
    let output = schematic_scene(project, sheet(), &[]).unwrap();
    assert_eq!(tints(&output), 0);
    let on_canvas = schematic_scene_styled(project, sheet(), &[], &canvas()).unwrap();
    // Revision, date, company and drawing number are empty.
    assert_eq!(tints(&on_canvas), 4);
    assert_golden(golden("frame_canvas.scene"), on_canvas.to_text().as_bytes());
}
