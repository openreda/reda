//! Schematic Scenes, checked as text snapshots.

use std::path::{Path, PathBuf};

use reda_model::Uuid;
use reda_model::geom::Point;
use reda_render::{Primitive, SceneLayer, schematic_scene};
use reda_testkit::{TempDir, TestEngine, assert_golden, test_engine_with_library};
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test")
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

#[test]
fn an_empty_sheet_draws_its_frame_and_filled_title_block() {
    let dir = TempDir::new("render-empty");
    let mut engine = demo(&dir);
    engine.run_ok(
        "project.properties.set",
        json!({"title": "Demo board", "revision": "B", "date": "2026-11-01",
               "author": "Ada", "company": "Open REDA"}),
    );
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    assert_golden(golden("empty_sheet.scene"), scene.to_text().as_bytes());
}

#[test]
fn a_bigger_sheet_draws_a_bigger_frame() {
    let dir = TempDir::new("render-a3");
    let mut engine = demo(&dir);
    engine.run_ok(
        "schematic.sheet.set_size",
        json!({"document": SHEET, "size": "a3"}),
    );
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    let (lo, hi) = scene.bounds().unwrap();
    assert_eq!((lo, hi), (Point::default(), Point::new(420.mm(), 297.mm())));
}

const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";

/// A point given in mil, as Command params.
fn at(x: i64, y: i64) -> serde_json::Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

/// Places Part `part` and returns the placed Unit's and Component's UUIDs.
fn place(engine: &mut TestEngine, part: &str, x: i64, y: i64, rotation: i64) -> (String, String) {
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": part, "at": at(x, y),
               "rotation": format!("{rotation}deg")}),
    );
    let r = &out["result"];
    (
        r["unit"].as_str().unwrap().to_owned(),
        r["component"]["uuid"].as_str().unwrap().to_owned(),
    )
}

fn junctions(engine: &mut TestEngine) -> Vec<Point> {
    let nets = engine.run_ok("schematic.nets", json!({"document": SHEET}));
    serde_json::from_value(nets["junctions"].clone()).unwrap()
}

/// A sheet holding one of every schematic object kind: Units of three Parts (one mirrored,
/// one turned, a multi-unit Part with a Pin mapped to two Pads), Wires meeting in a T
/// Junction, a Net Label and a No-Connect Marker.
fn busy_sheet(engine: &mut TestEngine) -> Vec<Point> {
    let (_, r1) = place(engine, R_10K, 2000, 5000, 0);
    place(engine, C_100N, 3000, 5000, 90);
    let (_, u1) = place(engine, LM358, 5000, 5000, 0);
    engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": u1, "at": at(5000, 3000), "mirror": true}),
    );
    engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": u1, "at": at(7000, 5000), "rotation": "180deg"}),
    );
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [r1], "visible": {"mpn": true}}),
    );
    // R1 pin 1 (2000, 5150) up to a bus, and C1 pin 2 (3150, 5000) into it from the side.
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [at(2000, 5150), at(2000, 5600), at(4000, 5600)]}),
    );
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [at(3150, 5000), at(3400, 5000), at(3400, 5600)]}),
    );
    engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "VIN", "at": at(2500, 5600)}),
    );
    engine.run_ok(
        "schematic.no_connect.add",
        json!({"document": SHEET, "at": at(4700, 3000)}),
    );
    junctions(engine)
}

#[test]
fn every_schematic_object_kind_is_drawn() {
    let dir = TempDir::new("render-busy");
    let mut engine = demo(&dir);
    let junctions = busy_sheet(&mut engine);
    assert_eq!(junctions.len(), 1, "the T makes one Junction");
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &junctions).unwrap();
    assert_golden(golden("busy_sheet.scene"), scene.to_text().as_bytes());
}

#[test]
fn every_item_is_tagged_with_an_object_on_the_sheet() {
    let dir = TempDir::new("render-tags");
    let mut engine = demo(&dir);
    let junctions = busy_sheet(&mut engine);
    let listed = engine.run_ok("objects.query", json!({"document": SHEET}));
    let mut objects: Vec<Uuid> = listed["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"]["uuid"].as_str().unwrap().parse().unwrap())
        .collect();
    objects.push(sheet());
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &junctions).unwrap();
    for item in &scene.items {
        assert!(objects.contains(&item.object), "{item:?}");
        assert_eq!(item.layer, SceneLayer::Schematic);
    }
    let units = listed["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| &o["object"])
        .filter(|o| o["type"] == "unit");
    for unit in units {
        let uuid = unit["uuid"].as_str().unwrap().parse().unwrap();
        assert!(scene.items_of(uuid).count() > 0, "Unit {uuid} is drawn");
    }
}

#[test]
fn ports_and_power_ports_are_drawn_in_every_direction_and_style() {
    let dir = TempDir::new("render-ports");
    let mut engine = demo(&dir);
    for (i, direction) in ["in", "out", "bidirectional"].into_iter().enumerate() {
        let y = 6000 - 500 * i as i64;
        engine.run_ok(
            "schematic.port.add",
            json!({"document": SHEET, "name": "SDA", "at": at(1000, y), "direction": direction}),
        );
        // Turned half a turn, the flag reaches left and its name still reads left to right.
        engine.run_ok(
            "schematic.port.add",
            json!({"document": SHEET, "name": "SCL_OUT", "at": at(4000, y),
                   "direction": direction, "rotation": "180deg"}),
        );
    }
    let styles = [
        "bar",
        "arrow",
        "circle",
        "wave",
        "ground",
        "signal_ground",
        "earth",
    ];
    for (i, style) in styles.into_iter().enumerate() {
        engine.run_ok(
            "schematic.power_port.add",
            json!({"document": SHEET, "name": "VCC", "at": at(1000 + 600 * i as i64, 3000),
                   "style": style}),
        );
    }
    engine.run_ok(
        "schematic.power_port.add",
        json!({"document": SHEET, "name": "GND", "at": at(6000, 3000), "rotation": "90deg"}),
    );
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    assert_golden(golden("ports.scene"), scene.to_text().as_bytes());
}

#[test]
fn upside_down_text_is_turned_to_read_left_to_right() {
    let dir = TempDir::new("render-readable");
    let mut engine = demo(&dir);
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [at(1000, 1000), at(3000, 1000)]}),
    );
    let out = engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SDA", "at": at(2000, 1000), "rotation": "180deg"}),
    );
    let label: Uuid = out["result"]["net_label"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    let item = scene.items_of(label).next().unwrap();
    let Primitive::Glyphs(run) = &item.primitive else {
        panic!("a Net Label draws text: {item:?}");
    };
    assert_eq!(run.rotation.0, 0);
    // Turned half a turn about its anchor, the text ends there and stays above the Wire.
    let (lo, hi) = item.primitive.bounds().unwrap();
    let anchor = Point::new(2000.mil(), 1000.mil());
    assert!(hi.x <= anchor.x && lo.x < anchor.x, "{lo:?} {hi:?}");
    assert!(lo.y > anchor.y, "{lo:?}");
}

#[test]
fn free_text_lines_and_rectangles_are_drawn() {
    let dir = TempDir::new("render-drawing");
    let mut engine = demo(&dir);
    engine.run_ok(
        "schematic.text.add",
        json!({"document": SHEET, "text": "Power supply\nrev A", "at": at(1000, 6000)}),
    );
    engine.run_ok(
        "schematic.text.add",
        json!({"document": SHEET, "text": "Turned", "at": at(4000, 6000),
               "height": "100mil", "rotation": "90deg"}),
    );
    engine.run_ok(
        "schematic.graphic.add_line",
        json!({"document": SHEET, "from": at(1000, 5000), "to": at(3000, 4000)}),
    );
    engine.run_ok(
        "schematic.graphic.add_rect",
        json!({"document": SHEET, "from": at(1000, 3000), "to": at(3000, 2000)}),
    );
    engine.run_ok(
        "schematic.graphic.add_rect",
        json!({"document": SHEET, "from": at(4000, 3000), "to": at(5000, 2000), "filled": true}),
    );
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    assert_golden(golden("drawing.scene"), scene.to_text().as_bytes());
}

trait Mil {
    fn mil(self) -> reda_model::geom::Nm;
}

impl Mil for i64 {
    fn mil(self) -> reda_model::geom::Nm {
        reda_model::geom::Nm::from_mil(self)
    }
}

trait Mm {
    fn mm(self) -> reda_model::geom::Nm;
}

impl Mm for i64 {
    fn mm(self) -> reda_model::geom::Nm {
        reda_model::geom::Nm::from_mm(self)
    }
}
