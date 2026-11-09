//! Silkscreen Preparation (pcb.md#silkscreen-preparation): `pcb.silkscreen.prepare`
//! clips silkscreen lines, arcs and rectangles clear of mask openings and the board edge,
//! moves text to the nearest clear place, and does it all as one undoable edit;
//! `library.footprint.silkscreen.prepare` does the same in the Footprint editor.

use reda_engine::model::{LibraryElement, Shape};
use reda_testkit::{
    TempDir, TestEngine, starter_library, test_engine_in, test_engine_with_library,
};
use serde_json::{Value, json};

const PCB: &str = "00000000-0000-4000-8000-000000000003";
/// The Starter Library's MountingHole_M3_Pad Mechanical Part: a 5.5 mm plated Pad inside a
/// silkscreen circle.
const MOUNTING_HOLE_PAD: &str = "1a21d727-dfca-41ae-9639-309d8abf4714";

/// A Project with an empty 30 × 20 mm board.
fn board(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&starter_library());
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Silk",
               "board": {"width": "30mm", "height": "20mm"}}),
    );
    engine
}

fn query(engine: &mut TestEngine, kind: &str) -> Vec<Value> {
    engine.run_ok("objects.query", json!({"type": kind, "document": PCB}))["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .collect()
}

fn mm(v: f64) -> String {
    format!("{v}mm")
}

fn pad(engine: &mut TestEngine, x: f64, y: f64, diameter: f64) -> Value {
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": mm(x), "y": mm(y)}, "width": mm(diameter)}),
    )["result"]["uuid"]
        .clone()
}

fn line(engine: &mut TestEngine, from: (f64, f64), to: (f64, f64)) -> Value {
    engine.run_ok(
        "pcb.graphic.add_line",
        json!({"layer": "silkscreen_top",
               "from": {"x": mm(from.0), "y": mm(from.1)},
               "to": {"x": mm(to.0), "y": mm(to.1)}}),
    )["result"]["uuid"]
        .clone()
}

fn violations(engine: &mut TestEngine, check: &str) -> Vec<Value> {
    engine.run_ok("drc.run", json!({}))["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == check)
        .cloned()
        .collect()
}

fn graphics(engine: &mut TestEngine) -> Vec<Value> {
    query(engine, "pcb_graphic")
}

#[test]
fn a_line_across_a_pad_is_clipped_clear_of_its_opening() {
    let dir = TempDir::new("silk-line");
    let mut engine = board(&dir);
    pad(&mut engine, 15.0, 10.0, 1.0);
    let uuid = line(&mut engine, (10.0, 10.0), (20.0, 10.0));
    assert_eq!(violations(&mut engine, "silk_to_mask").len(), 1);

    engine.run_ok("pcb.silkscreen.prepare", json!({}));

    let lines = graphics(&mut engine);
    assert_eq!(lines.len(), 2, "{lines:#?}");
    assert!(
        lines.iter().any(|g| g["uuid"] == uuid),
        "the first piece keeps the UUID"
    );
    assert!(violations(&mut engine, "silk_to_mask").is_empty());
    // The cut ends sit just past the clearance: 0.5 mm Pad + 0.1 mm rule + half the pen.
    let mut ends: Vec<i64> = lines
        .iter()
        .flat_map(|g| [&g["shape"]["from"]["x"], &g["shape"]["to"]["x"]])
        .map(|x| x.as_i64().unwrap())
        .collect();
    ends.sort();
    assert_eq!(ends[0], 10_000_000);
    assert_eq!(ends[3], 20_000_000);
    let gap_left = 15_000_000 - ends[1];
    let gap_right = ends[2] - 15_000_000;
    for gap in [gap_left, gap_right] {
        assert!((675_000..=700_000).contains(&gap), "gap {gap}");
    }
}

fn rect(engine: &mut TestEngine, from: (f64, f64), to: (f64, f64)) -> Value {
    engine.run_ok(
        "pcb.graphic.add_rect",
        json!({"layer": "silkscreen_top",
               "from": {"x": mm(from.0), "y": mm(from.1)},
               "to": {"x": mm(to.0), "y": mm(to.1)}}),
    )["result"]["uuid"]
        .clone()
}

fn text(engine: &mut TestEngine, content: &str, x: f64, y: f64) -> Value {
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": content, "x": mm(x), "y": mm(y)}),
    )["result"]["uuid"]
        .clone()
}

fn kinds(graphics: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = graphics
        .iter()
        .map(|g| g["shape"]["kind"].as_str().unwrap().to_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn a_clipped_rectangle_becomes_lines_and_an_untouched_one_stays() {
    let dir = TempDir::new("silk-rect");
    let mut engine = board(&dir);
    pad(&mut engine, 10.0, 5.0, 1.0);
    rect(&mut engine, (5.0, 5.0), (15.0, 8.0));
    rect(&mut engine, (18.0, 12.0), (25.0, 16.0));

    engine.run_ok("pcb.silkscreen.prepare", json!({}));

    let shapes = graphics(&mut engine);
    // The bottom side is cut in two around the Pad; the other three sides stay whole.
    assert_eq!(
        kinds(&shapes),
        ["line", "line", "line", "line", "line", "rect"]
    );
    assert!(violations(&mut engine, "silk_to_mask").is_empty());
}

#[test]
fn lines_under_min_remaining_length_go_whether_clipped_or_not() {
    let dir = TempDir::new("silk-short");
    let mut engine = board(&dir);
    // 0.4 mm and 0.6 mm lines far from anything: the default 0.5 mm takes only the first.
    let short = line(&mut engine, (5.0, 5.0), (5.4, 5.0));
    let long = line(&mut engine, (5.0, 8.0), (5.6, 8.0));
    // A line whose piece left of the Pad is 0.3 mm long: that piece goes too.
    pad(&mut engine, 15.0, 12.0, 1.0);
    line(&mut engine, (14.0, 12.0), (20.0, 12.0));

    let out = engine.run_ok("pcb.silkscreen.prepare", json!({}));

    let shapes = graphics(&mut engine);
    let uuids: Vec<&Value> = shapes.iter().map(|g| &g["uuid"]).collect();
    assert!(!uuids.contains(&&short));
    assert!(uuids.contains(&&long));
    assert_eq!(shapes.len(), 2, "{shapes:#?}");
    assert!(
        out["result"]["graphics"]
            .as_array()
            .unwrap()
            .contains(&short)
    );

    // A smaller Min Remaining Length keeps a 0.4 mm line.
    let dir = TempDir::new("silk-short-option");
    let mut engine = board(&dir);
    let short = line(&mut engine, (5.0, 5.0), (5.4, 5.0));
    engine.run_ok(
        "pcb.silkscreen.prepare",
        json!({"min_remaining_length": "0.3mm"}),
    );
    assert_eq!(graphics(&mut engine)[0]["uuid"], short);
}

#[test]
fn silkscreen_outside_the_board_goes_unless_asked_to_stay() {
    let dir = TempDir::new("silk-outside");
    let mut engine = board(&dir);
    // The board is 30 × 20 mm from the origin; this line runs 10 mm past its right edge.
    line(&mut engine, (20.0, 10.0), (40.0, 10.0));
    line(&mut engine, (35.0, 5.0), (39.0, 5.0));

    engine.run_ok("pcb.silkscreen.prepare", json!({}));

    let shapes = graphics(&mut engine);
    assert_eq!(shapes.len(), 1);
    assert!(shapes[0]["shape"]["to"]["x"].as_i64().unwrap() < 30_000_000 - 254_000);
    assert!(violations(&mut engine, "silk_to_edge").is_empty());

    let dir = TempDir::new("silk-outside-kept");
    let mut engine = board(&dir);
    let outside = line(&mut engine, (35.0, 5.0), (39.0, 5.0));
    engine.run_ok(
        "pcb.silkscreen.prepare",
        json!({"delete_outside_board": false}),
    );
    assert_eq!(graphics(&mut engine)[0]["uuid"], outside);
}

#[test]
fn locked_silkscreen_stays_unless_clip_locked() {
    let dir = TempDir::new("silk-locked");
    let mut engine = board(&dir);
    pad(&mut engine, 15.0, 10.0, 1.0);
    let uuid = line(&mut engine, (10.0, 10.0), (20.0, 10.0));
    engine.run_ok("pcb.lock", json!({"objects": [uuid], "locked": true}));

    engine.run_ok("pcb.silkscreen.prepare", json!({}));
    assert_eq!(graphics(&mut engine).len(), 1);

    engine.run_ok("pcb.silkscreen.prepare", json!({"clip_locked": true}));
    assert_eq!(graphics(&mut engine).len(), 2);
    assert!(violations(&mut engine, "silk_to_mask").is_empty());
}

#[test]
fn without_design_rules_the_manual_clearance_applies() {
    let dir = TempDir::new("silk-manual");
    let mut engine = board(&dir);
    pad(&mut engine, 15.0, 10.0, 1.0);
    line(&mut engine, (10.0, 10.0), (20.0, 10.0));

    engine.run_ok(
        "pcb.silkscreen.prepare",
        json!({"use_design_rules": false, "clearance": "0.3mm"}),
    );

    let lines = graphics(&mut engine);
    let right = lines
        .iter()
        .map(|g| g["shape"]["from"]["x"].as_i64().unwrap())
        .max()
        .unwrap();
    // 0.5 mm Pad + 0.3 mm + half the 0.15 mm pen.
    let gap = right - 15_000_000;
    assert!((875_000..=900_000).contains(&gap), "gap {gap}");
}

#[test]
fn the_whole_preparation_is_one_undo_step() {
    let dir = TempDir::new("silk-undo");
    let mut engine = board(&dir);
    pad(&mut engine, 15.0, 10.0, 1.0);
    pad(&mut engine, 5.0, 5.0, 1.0);
    line(&mut engine, (10.0, 10.0), (20.0, 10.0));
    rect(&mut engine, (3.0, 5.0), (8.0, 8.0));
    let before = graphics(&mut engine);

    engine.run_ok("pcb.silkscreen.prepare", json!({}));
    assert_ne!(graphics(&mut engine), before);
    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(graphics(&mut engine), before);
}

#[test]
fn text_over_a_pad_moves_to_the_nearest_clear_place() {
    let dir = TempDir::new("silk-text");
    let mut engine = board(&dir);
    pad(&mut engine, 10.0, 10.0, 0.6);
    let label = text(&mut engine, "A", 10.0, 10.0);
    assert!(!violations(&mut engine, "silk_to_mask").is_empty());

    let out = engine.run_ok("pcb.silkscreen.prepare", json!({}));

    assert_eq!(out["result"]["moved"], json!([label]));
    assert!(violations(&mut engine, "silk_to_mask").is_empty());
    let moved = query(&mut engine, "pcb_text")
        .into_iter()
        .find(|t| t["uuid"] == label)
        .unwrap();
    let (dx, dy) = (
        moved["position"]["x"].as_i64().unwrap() - 10_000_000,
        moved["position"]["y"].as_i64().unwrap() - 10_000_000,
    );
    let distance = (dx as f64).hypot(dy as f64);
    assert!(distance > 0.0 && distance <= 2_000_000.0, "{distance}");
}

#[test]
fn text_that_needs_to_move_further_than_max_distance_stays() {
    let dir = TempDir::new("silk-text-stuck");
    let mut engine = board(&dir);
    // A 6 mm Pad: a 1 mm text at its middle is more than 2 mm from clear space.
    pad(&mut engine, 10.0, 10.0, 6.0);
    let label = text(&mut engine, "B", 10.0, 10.0);

    let out = engine.run_ok("pcb.silkscreen.prepare", json!({}));

    assert_eq!(out["result"]["moved"], json!([]));
    assert_eq!(
        out["result"]["unplaced"],
        json!([{"object": label, "content": "B"}])
    );
    // With a longer reach it finds a place.
    let out = engine.run_ok("pcb.silkscreen.prepare", json!({"max_distance": "5mm"}));
    assert_eq!(out["result"]["moved"], json!([label]));
    assert!(violations(&mut engine, "silk_to_mask").is_empty());
}

/// A mounting hole with a plated Pad, placed at (10, 10) mm.
fn mounting_hole(engine: &mut TestEngine) -> Value {
    engine.run_ok(
        "pcb.mechanical.place",
        json!({"part": MOUNTING_HOLE_PAD, "at": {"x": "10mm", "y": "10mm"}}),
    )["result"]["footprint"]
        .clone()
}

fn instance(engine: &mut TestEngine, uuid: &Value) -> Value {
    query(engine, "footprint_instance")
        .into_iter()
        .find(|i| i["uuid"] == *uuid)
        .unwrap()
}

fn copy_of(engine: &mut TestEngine, instance: &Value) -> Value {
    query(engine, "footprint_copy")
        .into_iter()
        .find(|c| c["uuid"] == instance["footprint"])
        .unwrap()
}

#[test]
fn clipping_footprint_silkscreen_makes_a_locally_edited_footprint() {
    let dir = TempDir::new("silk-footprint");
    let mut engine = board(&dir);
    let hole = mounting_hole(&mut engine);
    // A test point on the hole's 3.07 mm silkscreen circle, where the circle starts.
    pad(&mut engine, 13.07, 10.0, 1.0);
    assert!(!violations(&mut engine, "silk_to_mask").is_empty());

    let out = engine.run_ok("pcb.silkscreen.prepare", json!({}));

    assert_eq!(out["result"]["footprints"], json!([hole]));
    let placed = instance(&mut engine, &hole);
    let copy = copy_of(&mut engine, &placed);
    assert!(copy["edited_from"].is_object(), "{copy:#?}");
    // The circle becomes one arc around the rest of the way: its cut ends join.
    let silk: Vec<&Value> = copy["footprint"]["graphics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["layer"] == "silkscreen_top")
        .collect();
    assert_eq!(silk.len(), 1, "{silk:#?}");
    assert_eq!(silk[0]["shape"]["kind"], "arc");
    let sweep = silk[0]["shape"]["sweep"].as_i64().unwrap();
    assert!((300_000..360_000).contains(&sweep), "{sweep}");
    assert!(violations(&mut engine, "silk_to_mask").is_empty());
    // The DRC report lists the Locally Edited Footprint.
    assert_eq!(
        engine.run_ok("drc.run", json!({}))["locally_edited"][0]["instance"],
        hole
    );
}

#[test]
fn a_moved_designator_becomes_manual() {
    let dir = TempDir::new("silk-designator");
    let mut engine = board(&dir);
    let hole = mounting_hole(&mut engine);
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances": [hole], "field": "Designator", "autoposition": "center_above"}),
    );
    // A test point just above the courtyard, under the Designator.
    pad(&mut engine, 10.0, 13.8, 0.6);
    assert!(!violations(&mut engine, "silk_to_mask").is_empty());

    let out = engine.run_ok("pcb.silkscreen.prepare", json!({}));

    assert_eq!(out["result"]["moved"].as_array().unwrap().len(), 1);
    let placed = instance(&mut engine, &hole);
    let change = &placed["text_overrides"][0];
    assert!(change["at"].is_object(), "{placed:#?}");
    assert!(
        change.get("autoposition").is_none(),
        "manual is left out: {change}"
    );
    assert!(violations(&mut engine, "silk_to_mask").is_empty());
    // Moving text is not a local edit of the Footprint.
    let copy = copy_of(&mut engine, &placed);
    assert!(copy.get("edited_from").is_none());
}

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";

/// A new Footprint open in its editor, in a writable Library.
fn new_footprint(home: &TempDir, work: &TempDir) -> (TestEngine, String) {
    let lib = work.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let info = reda_engine::model::LibraryInfo {
        uuid: LIBRARY.parse().unwrap(),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(
        lib.join("Mine.redalib"),
        reda_engine::format::library::write_library_info(&info).unwrap(),
    )
    .unwrap();
    let mut e = test_engine_in(home.path());
    e.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": [lib]}),
    );
    let out = e.run_ok(
        "library.element.new",
        json!({"library": LIBRARY, "kind": "footprint", "name": "Test"}),
    );
    (e, out["element"].as_str().unwrap().to_owned())
}

fn element(e: &TestEngine, fp: &str) -> reda_engine::model::Footprint {
    match e.element(fp.parse().unwrap()) {
        Some(LibraryElement::Footprint(f)) => f.clone(),
        _ => panic!("open"),
    }
}

#[test]
fn the_footprint_editor_prepares_its_own_silkscreen() {
    let (home, work) = (TempDir::new("silk-fp-home"), TempDir::new("silk-fp-work"));
    let (mut e, fp) = new_footprint(&home, &work);
    e.run_ok(
        "library.footprint.pad.add",
        json!({"footprint": fp, "number": "1", "kind": "smd", "shape": "rect",
               "at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}),
    );
    e.run_ok(
        "library.footprint.graphic.add",
        json!({"footprint": fp, "layer": "silkscreen_top",
               "shape": {"kind": "line", "start": {"x": "-3mm", "y": 0}, "end": {"x": "3mm", "y": 0}}}),
    );
    let before = element(&e, &fp);
    // A new Footprint's Designator sits at its origin, on the Pad.
    let designator = before.designator_text().unwrap().clone();
    assert_eq!(designator.at, Default::default());

    let out = e.run_ok(
        "library.footprint.silkscreen.prepare",
        json!({"footprint": fp}),
    );

    assert_eq!(out["result"]["moved"], json!([designator.uuid]));
    let after = element(&e, &fp);
    let mut ends: Vec<i64> = after
        .graphics
        .iter()
        .filter(|g| g.layer.is_silkscreen())
        .flat_map(|g| match &g.shape {
            Shape::Line { start, end } => [start.x.0, end.x.0],
            other => panic!("{other:?}"),
        })
        .collect();
    ends.sort();
    assert_eq!(ends.len(), 4);
    // 0.5 mm Pad half-width + a new board's 0.1 mm + half the 0.12 mm pen.
    assert!((-700_000..=-660_000).contains(&ends[1]), "{ends:?}");
    assert!((660_000..=700_000).contains(&ends[2]), "{ends:?}");
    let moved = after.designator_text().unwrap().at;
    let distance = (moved.x.0 as f64).hypot(moved.y.0 as f64);
    assert!(distance > 0.0 && distance <= 2_000_000.0, "{moved:?}");

    // One undo step puts it all back.
    e.run_ok("edit.undo", json!({"document": fp}));
    assert_eq!(element(&e, &fp), before);
}

#[test]
fn min_remaining_length_defaults_to_half_a_millimetre_and_max_distance_to_two() {
    let registry = reda_engine::registry();
    for id in [
        "pcb.silkscreen.prepare",
        "library.footprint.silkscreen.prepare",
    ] {
        let schema = registry.params_schema(id).unwrap();
        let props = &schema["properties"];
        assert_eq!(props["min_remaining_length"]["default"], 500_000, "{id}");
        assert_eq!(props["max_distance"]["default"], 2_000_000, "{id}");
        assert_eq!(props["use_design_rules"]["default"], true, "{id}");
        assert_eq!(props["delete_outside_board"]["default"], true, "{id}");
        assert_eq!(props["clip_locked"]["default"], false, "{id}");
    }
}

#[test]
fn text_moves_up_to_two_millimetres_by_default() {
    // A 1 mm text centred on Pads of growing size: the further its nearest clear place,
    // the larger the Pad. Up to 2 mm away it moves; beyond it stays.
    let reach = |diameter: f64| {
        let dir = TempDir::new("silk-reach");
        let mut engine = board(&dir);
        pad(&mut engine, 10.0, 10.0, diameter);
        text(&mut engine, "C", 10.0, 10.0);
        let out = engine.run_ok("pcb.silkscreen.prepare", json!({}));
        out["result"]["moved"].as_array().unwrap().len()
    };
    assert_eq!(reach(2.0), 1);
    assert_eq!(reach(5.0), 0);
}
