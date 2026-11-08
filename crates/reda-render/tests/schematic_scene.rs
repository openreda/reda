//! Schematic Scenes, checked as text snapshots.

use std::path::{Path, PathBuf};

use reda_model::geom::{Nm, Point};
use reda_model::{UnitSuffix, Uuid};
use reda_render::{
    Primitive, SceneLayer, SchematicColor, SchematicStyle, schematic_scene, schematic_scene_styled,
};
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
        "schematic.sheet.set_frame",
        json!({"document": SHEET, "size": "a3"}),
    );
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    let (lo, hi) = scene.bounds().unwrap();
    assert_eq!((lo, hi), (Point::default(), Point::new(420.mm(), 297.mm())));
}

#[test]
fn a_sheet_scene_knows_its_paper_so_the_grid_can_stop_there() {
    let dir = TempDir::new("render-paper");
    let mut engine = demo(&dir);
    engine.run_ok(
        "schematic.sheet.set_frame",
        json!({"document": SHEET, "size": "a3"}),
    );
    let mut scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    assert_eq!(
        scene.paper(),
        Some((Point::default(), Point::new(420.mm(), 297.mm())))
    );
    // Without the paper item (a board, a Library element) there is none.
    scene
        .items
        .retain(|i| i.color != reda_render::ColorKey::Schematic(SchematicColor::Paper));
    assert_eq!(scene.paper(), None);
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
fn a_wire_draws_one_stroke_per_segment_at_any_angle() {
    let dir = TempDir::new("render-polyline");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [at(0, 0), at(100, 0), at(200, 100)]}),
    );
    let wire: Uuid = out["result"]["wires"][0].as_str().unwrap().parse().unwrap();
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    let strokes: Vec<(Point, Point)> = scene
        .items_of(wire)
        .map(|item| match item.primitive {
            Primitive::Capsule { a, b, .. } => (a, b),
            ref other => panic!("a Wire draws capsules, not {other:?}"),
        })
        .collect();
    let mil = |x: i64, y: i64| Point::new(x.mil(), y.mil());
    assert_eq!(
        strokes,
        [(mil(0, 0), mil(100, 0)), (mil(100, 0), mil(200, 100))]
    );
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
fn free_text_sits_on_its_justification_point() {
    let dir = TempDir::new("render-justify");
    let mut engine = demo(&dir);
    let mut text = |justify: &str, rotation: &str| -> Uuid {
        let out = engine.run_ok(
            "schematic.text.add",
            json!({"document": SHEET, "text": "First\nSecond\nThird", "at": at(3000, 3000),
                   "height": "100mil", "justify": justify, "rotation": rotation}),
        );
        out["result"]["text"].as_str().unwrap().parse().unwrap()
    };
    let top_right = text("top_right", "0deg");
    let bottom_left = text("bottom_left", "0deg");
    let center = text("center", "0deg");
    // Upside down it reads the right way up, and still hangs from the same corner.
    let turned = text("top_right", "180deg");
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    let bounds = |uuid: Uuid| {
        scene
            .items_of(uuid)
            .next()
            .unwrap()
            .primitive
            .bounds()
            .unwrap()
    };
    let a = Point::new(3000.mil(), 3000.mil());
    let slack = 30.mil().0;

    let (lo, hi) = bounds(top_right);
    assert!(
        hi.x.0 <= a.x.0 + slack && hi.y.0 <= a.y.0 + slack,
        "{lo:?} {hi:?}"
    );
    assert!(lo.x < a.x && lo.y.0 < a.y.0 - 300.mil().0, "{lo:?} {hi:?}");

    // The block's bottom is the last line's baseline, so the lines stack above it.
    let (lo, hi) = bounds(bottom_left);
    assert!(
        lo.x.0 >= a.x.0 - slack && lo.y.0 >= a.y.0 - slack,
        "{lo:?} {hi:?}"
    );
    assert!(hi.y.0 > a.y.0 + 300.mil().0, "{lo:?} {hi:?}");

    let (lo, hi) = bounds(center);
    let mid = |l: Nm, h: Nm| (l.0 + h.0) / 2;
    assert!((mid(lo.x, hi.x) - a.x.0).abs() <= slack, "{lo:?} {hi:?}");
    assert!((mid(lo.y, hi.y) - a.y.0).abs() <= slack, "{lo:?} {hi:?}");

    let (lo, hi) = bounds(turned);
    assert!(
        lo.x.0 >= a.x.0 - slack && lo.y.0 >= a.y.0 - slack,
        "{lo:?} {hi:?}"
    );
}

#[test]
fn a_ports_flag_reaches_away_from_its_wire() {
    let dir = TempDir::new("render-port-side");
    let mut engine = demo(&dir);
    // Both Ports point along +x; the first one's Wire leaves along +x too.
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [at(1000, 1000), at(2000, 1000)]}),
    );
    let mut flag = |x: i64, y: i64| -> Uuid {
        let out = engine.run_ok(
            "schematic.port.add",
            json!({"document": SHEET, "name": "EN", "at": at(x, y), "direction": "out"}),
        );
        out["result"]["port"].as_str().unwrap().parse().unwrap()
    };
    let (wired, free) = (flag(1000, 1000), flag(1000, 3000));
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    let reach = |port: Uuid| {
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        for item in scene.items_of(port) {
            let (a, b) = item.primitive.bounds().unwrap();
            lo = lo.min(a.x.0);
            hi = hi.max(b.x.0);
        }
        (lo, hi)
    };
    let x = 1000.mil().0;
    let (lo, hi) = reach(wired);
    assert!(hi <= x + 100_000 && lo < x - 1_000_000, "{lo} {hi}");
    let (lo, hi) = reach(free);
    assert!(lo >= x - 100_000 && hi > x + 1_000_000, "{lo} {hi}");
}

#[test]
fn a_power_port_with_a_hidden_name_draws_only_its_symbol() {
    let dir = TempDir::new("render-hidden-name");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.power_port.add",
        json!({"document": SHEET, "name": "GND", "at": at(1000, 1000), "hide_name": true}),
    );
    let port: Uuid = out["result"]["power_port"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    let items: Vec<_> = scene.items_of(port).collect();
    assert!(!items.is_empty());
    assert!(
        items
            .iter()
            .all(|i| !matches!(i.primitive, Primitive::Glyphs(_))),
        "{items:?}"
    );
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

/// The field text runs a Unit draws, as (text, anchor, rotation, height).
fn field_runs(
    scene: &reda_render::Scene,
    unit: &str,
) -> Vec<(String, Point, i64, reda_model::geom::Nm)> {
    let unit: Uuid = unit.parse().unwrap();
    scene
        .items_of(unit)
        .filter(|i| {
            matches!(
                i.color,
                reda_render::ColorKey::Schematic(
                    SchematicColor::Designator | SchematicColor::Value | SchematicColor::Field
                )
            )
        })
        .filter_map(|i| match &i.primitive {
            Primitive::Glyphs(run) => Some((run.text.clone(), run.at, run.rotation.0, run.height)),
            _ => None,
        })
        .collect()
}

#[test]
fn fields_stack_right_of_the_body_at_0_and_180_and_above_it_at_90_and_270() {
    let dir = TempDir::new("render-field-stack");
    let mut engine = demo(&dir);
    let mut units = vec![];
    for (i, rotation) in [0, 90, 180, 270].into_iter().enumerate() {
        units.push(place(&mut engine, R_10K, 2000 * i as i64 + 1000, 2000, rotation).0);
    }
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    for (i, (unit, rotation)) in units.iter().zip([0, 90, 180, 270]).enumerate() {
        let body = scene
            .items_of(unit.parse().unwrap())
            .filter(|i| !matches!(i.primitive, Primitive::Glyphs(_)))
            .filter_map(|i| i.primitive.bounds())
            .reduce(|(lo, hi), (l, h)| {
                (
                    Point::new(lo.x.min(l.x), lo.y.min(l.y)),
                    Point::new(hi.x.max(h.x), hi.y.max(h.y)),
                )
            })
            .unwrap();
        let runs = field_runs(&scene, unit);
        let texts: Vec<&str> = runs.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(texts, [format!("R{}", i + 1).as_str(), "10k"]);
        for (text, at, angle, _) in &runs {
            assert_eq!(*angle, 0, "{text} at {rotation}° is right-reading");
            if rotation % 180 == 0 {
                assert!(
                    at.x > body.1.x,
                    "{text} at {rotation}° is right of the body"
                );
            } else {
                assert!(at.y > body.1.y, "{text} at {rotation}° is above the body");
                assert_eq!(at.x, Nm((body.0.x.0 + body.1.x.0) / 2), "centred on it");
            }
        }
        assert!(
            runs[0].1.y > runs[1].1.y,
            "the Designator comes first, on top"
        );
    }
}

#[test]
fn a_moved_field_follows_its_unit_and_stays_right_reading() {
    let dir = TempDir::new("render-field-moved");
    let mut engine = demo(&dir);
    let (unit, _) = place(&mut engine, R_10K, 1000, 1000, 0);
    engine.run_ok(
        "schematic.field.move",
        json!({"unit": unit, "field": "value", "at": at(1200, 1000), "size": "2mm"}),
    );
    engine.run_ok(
        "schematic.unit.move",
        json!({"unit": unit, "at": at(1000, 1000), "rotation": "180deg"}),
    );
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();
    let runs = field_runs(&scene, &unit);
    let value = runs.iter().find(|r| r.0 == "10k").unwrap();
    assert_eq!(
        value.1,
        Point::new(800.mil(), 1000.mil()),
        "the offset turns with it"
    );
    assert_eq!(value.2, 0, "upside down would not be right-reading");
    assert_eq!(value.3, 2.mm());
}

#[test]
fn the_unit_suffix_follows_the_designator_from_the_units_position() {
    let dir = TempDir::new("render-unit-suffix");
    let mut engine = demo(&dir);
    let (a, component) = place(&mut engine, LM358, 1000, 1000, 0);
    let b = engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": component, "at": at(3000, 1000)}),
    )["result"]["unit"]
        .as_str()
        .unwrap()
        .to_owned();
    let (r, _) = place(&mut engine, R_10K, 5000, 1000, 0);
    let project = engine.project().unwrap();
    let designators = |style: UnitSuffix| {
        let scene = schematic_scene_styled(
            project,
            sheet(),
            &[],
            &SchematicStyle {
                unit_suffix: style,
                ..SchematicStyle::default()
            },
        )
        .unwrap();
        [&a, &b, &r].map(|u| field_runs(&scene, u)[0].0.clone())
    };
    assert_eq!(designators(UnitSuffix::Alpha), ["U1A", "U1B", "R1"]);
    assert_eq!(designators(UnitSuffix::Numeric), ["U1:1", "U1:2", "R1"]);
    let default = schematic_scene(project, sheet(), &[]).unwrap();
    assert_eq!(field_runs(&default, &a)[0].0, "U1A");
}

#[path = "schematic_scene/overbars.rs"]
mod overbars;
