//! Creating a board from a Board Template: `project.new` and `document.add {kind: pcb}`
//! with `board_template` write the evaluated frame as plain objects
//! (`docs/spec/v0.2.5/creation.md`, `testing.md#creation-path`).

use std::path::{Path, PathBuf};

use reda_engine::Engine;
use reda_engine::command::{ErrorCode, FixedClock, PartSource, SeededIds, Session, UserDirs};
use reda_engine::format::library::write_library_info;
use reda_engine::format::write_board_template;
use reda_engine::library::DiskParts;
use reda_engine::model::{BoardTemplate, FootprintCopy, LibraryInfo, TemplatePart, Uuid};
use reda_testkit::{TempDir, TestEngine, starter_library, test_engine_with_library};
use serde_json::{Value, json};

const CUSTOM_RECTANGLE: &str = "4b5a3569-5fcc-49b3-8af9-5d5fdda27d41";
const RECTANGLE_OUTLINE: &str = "46545762-f814-4177-bf02-4321128791b9";
const HEADER_1X02: &str = "d0560bc9-5203-4a8b-b575-24058968bdd8";
/// Header_1x02's second alternate Footprint, which the fixture's connector uses.
const HEADER_1X02_SECOND: &str = "a3cf2a76-4db1-40d0-b706-993194277cef";
const MOUNTING_HOLE_M3: &str = "326ace02-ef44-4af4-8f9d-21d6c65a74ef";
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";

fn id(n: u128) -> Uuid {
    Uuid::v4_from_bits(n)
}

fn s(n: u128) -> String {
    id(n).to_string()
}

fn mm(n: f64) -> i64 {
    (n * 1e6).round() as i64
}

#[test]
fn project_new_writes_the_custom_rectangle_as_a_locked_outline() {
    let work = TempDir::new("template-rect");
    let mut engine = test_engine_with_library(&starter_library());
    let out = engine.run_ok(
        "project.new",
        json!({"path": work.path(), "name": "Demo", "board_template": {
            "element": CUSTOM_RECTANGLE, "values": {"width": "65mm", "corner_radius": 0}}}),
    );
    let seeded = &out["seeded"];
    let outline = seeded["outline"].as_str().expect("an outline was written");
    assert_eq!(
        seeded["template"],
        json!({
            "element": CUSTOM_RECTANGLE,
            "name": "Custom rectangle",
            "values": {"corner_radius": 0, "height": 80_000_000, "width": 65_000_000},
            "objects": [{"template_object": RECTANGLE_OUTLINE, "created": outline}]
        })
    );
    let got = engine.run_ok("object.get", json!({"uuid": outline}));
    assert_eq!(got["object"]["locked"], true, "{got}");
    assert_eq!(
        got["object"]["vertices"],
        json!([
            {"x": 0, "y": 0},
            {"x": 65_000_000, "y": 0},
            {"x": 65_000_000, "y": 80_000_000},
            {"x": 0, "y": 80_000_000}
        ])
    );
    // The suggestion (2) gives the layer count, and the new Project has nothing to undo.
    assert_eq!(seeded["layers"].as_array().unwrap().len(), 15);
    let err = engine.run_err("edit.undo", json!({"document": "project"}));
    assert_eq!(err.code, ErrorCode::Unavailable, "{err:?}");
}

#[test]
fn every_card_edge_built_in_creates_a_board_drc_passes() {
    // The fingers sit on the outline, the half-moon crosses it and the Courtyards follow
    // the key notches and tab chamfers: none of it may fire on a fresh card. On a JLCPCB
    // board, whose clearance fits the M.2 fingers' 0.15 mm gaps; the starter M3 hole's
    // 0.12 mm silkscreen ring is under its 0.15 mm silk width, which is not the card's.
    const M2: &str = "3d513563-914e-4251-97d8-d3f9836240ad";
    const PCIE: &str = "4ca6b4ca-11f8-45f1-96c1-c610ece56af9";
    let mut points = vec![];
    for key in ["a", "b", "e", "m", "ae", "bm"] {
        points.push((M2, json!({"key": key, "length": "2230"})));
    }
    for lanes in ["x1", "x4", "x8", "x16"] {
        for bracket in ["full", "low_profile"] {
            points.push((PCIE, json!({"lanes": lanes, "bracket": bracket})));
        }
    }
    for (i, (element, values)) in points.into_iter().enumerate() {
        let work = TempDir::new(&format!("card-edge-{i}"));
        let mut engine = test_engine_with_library(&starter_library());
        engine.run_ok(
            "project.new",
            json!({"path": work.path(), "name": "Card", "fab_preset": "JLCPCB",
                   "board_template": {"element": element, "values": values}}),
        );
        let drc = engine.run_ok("drc.run", json!({}));
        let card: Vec<&Value> = (drc["violations"].as_array().unwrap().iter())
            .filter(|v| v["check"] != "silk_width")
            .collect();
        assert_eq!(card, Vec::<&Value>::new(), "{values}: {drc}");
    }
}

/// A Library Part as a template part-table entry `entry`, with its Footprint copies
/// numbered from `entry + 1`, exactly as the starter Library holds it.
fn copied_part(
    part: &str,
    entry: u128,
    element: Option<&str>,
) -> (TemplatePart, Vec<FootprintCopy>) {
    let session = Session {
        starter_library: Some(starter_library()),
        ..Session::default()
    };
    let copy = DiskParts::new(&session)
        .part(part.parse().unwrap())
        .expect("a starter Part");
    let copies: Vec<FootprintCopy> = copy
        .footprints
        .iter()
        .enumerate()
        .map(|(i, f)| FootprintCopy {
            uuid: id(entry + 1 + i as u128),
            hash: f.hash.clone(),
            edited_from: None,
            footprint: f.footprint.clone(),
        })
        .collect();
    let mut part = copy.part;
    if let Some(element) = element {
        part.uuid = element.parse().unwrap();
    }
    let entry = TemplatePart {
        uuid: id(entry),
        hash: copy.hash,
        part,
        symbol: copy.symbol,
        footprints: copies.iter().map(|c| c.uuid).collect(),
    };
    (entry, copies)
}

/// The fixture board: a 60 × 40 mm outline whose bottom edge is plated, a cutout, a
/// header connector on the bottom (a second one only for `size: large`), an M3 Mechanical
/// Part hole with a Keepout that protects it, a Free Pad hole, a Key dimensions group
/// with a Dimension from an outline vertex to the Free Pad and a line, a Documentation
/// group with a text, and an edge connector property.
fn fixture(header_element: Option<&str>) -> BoardTemplate {
    let mut t = BoardTemplate::new(id(100), "Fixture board");
    t.suggested_layers = 4;
    t.parameters = serde_json::from_value(json!([
        {"kind":"choice","name":"size","label":"Size","default":"small","options":[
            {"key":"small","label":"Small","bind":{"width":"60"}},
            {"key":"large","label":"Large","bind":{"width":"80"}}
        ]},
        {"kind":"choice","name":"finish","label":"Finish","when":{"size":["large"]},"options":[
            {"key":"plain","label":"Plain","bind":{}},
            {"key":"fancy","label":"Fancy","bind":{}}
        ]},
        {"kind":"length","name":"height","label":"Height","default":"40","min":"20","max":"100","unit":"mm"}
    ]))
    .unwrap();
    t.board = serde_json::from_value(
        json!({"edge_connector": {"bevel": {"angle": "20deg", "depth": "1.3"}, "hard_gold": true}}),
    )
    .unwrap();
    t.outline = serde_json::from_value(json!([{"uuid": s(101), "locked": true, "vertices": [
        {"x":0,"y":0,"plated":true},
        {"x":"width","y":0},
        {"x":"width","y":"height"},
        {"x":"width","y":"height"},
        {"x":0,"y":"height"}
    ]}]))
    .unwrap();
    t.cutouts = serde_json::from_value(json!([{"uuid": s(102), "locked": true, "vertices": [
        {"x":"20","y":"10"},{"x":"25","y":"10"},{"x":"25","y":"15"},{"x":"20","y":"15"}
    ]}]))
    .unwrap();
    let (header, header_copies) = copied_part(HEADER_1X02, 200, header_element);
    let (hole, hole_copies) = copied_part(MOUNTING_HOLE_M3, 300, None);
    let second = header_copies[1].uuid;
    t.parts = vec![header, hole];
    t.footprint_copies = header_copies.into_iter().chain(hole_copies).collect();
    t.connectors = serde_json::from_value(json!([
        {"uuid": s(110), "part": s(200), "footprint": second.to_string(),
         "at": {"x":"10","y":"height / 2"}, "rotation":"90", "side":"bottom", "locked": true},
        {"uuid": s(111), "part": s(200), "footprint": s(201),
         "at": {"x":"40","y":"5"}, "rotation":0, "side":"top", "locked": true,
         "when": {"size": ["large"]}}
    ]))
    .unwrap();
    t.holes = serde_json::from_value(json!([
        {"uuid": s(120), "part": s(300), "footprint": s(301), "side": "top",
         "at": {"x":"width - 5","y":"height - 5"}, "locked": true},
        {"uuid": s(121), "at": {"x":"5","y":"5"}, "locked": true,
         "pad": {"kind":"non_plated","shape":"circle","width":"3.2","height":"3.2",
                 "hole":{"width":"3.2","height":"3.2"}}}
    ]))
    .unwrap();
    t.keepouts = serde_json::from_value(json!([
        {"uuid": s(130), "layer":"bottom_copper", "restrict":["tracks","vias"], "shape":"polygon",
         "protects": s(120), "locked": true, "vertices": [
            {"x":"width - 9","y":"height - 9"},{"x":"width - 1","y":"height - 9"},
            {"x":"width - 1","y":"height - 1"},{"x":"width - 9","y":"height - 1"}
        ]},
        {"uuid": s(131), "layer":"all_copper", "restrict":["zones"], "shape":"polygon",
         "locked": true, "vertices": [
            {"x":"30","y":"20"},{"x":"35","y":"20"},{"x":"35","y":"25"}
        ]}
    ]))
    .unwrap();
    t.drawing_groups = serde_json::from_value(json!([
        {"uuid": s(140), "name": "Key dimensions", "layer": "mechanical"},
        {"uuid": s(141), "name": "Host board outline (approximate)", "layer": "documentation"}
    ]))
    .unwrap();
    t.drawings = serde_json::from_value(json!([
        {"uuid": s(150), "group": s(140), "kind": "dimension", "locked": true,
         "a": {"follow": {"object": s(101), "vertex": 4}}, "b": {"follow": {"object": s(121)}},
         "offset": "-3", "suffix": " mm"},
        {"uuid": s(151), "group": s(140), "kind": "dimension", "locked": true,
         "a": {"follow": {"object": s(120)}}, "b": {"x": 0, "y": "height"}},
        {"uuid": s(152), "group": s(141), "kind": "text", "locked": true, "text": "HOST",
         "position": {"x":"10","y":"-5"}, "height": "2"},
        {"uuid": s(153), "group": s(141), "kind": "line", "locked": true, "width": "0.1",
         "from": {"x":0,"y":"-2"}, "to": {"x":"width","y":"-2"}}
    ]))
    .unwrap();
    t
}

/// A Library in `folder` holding `templates`.
fn write_library(folder: &Path, n: u128, templates: &[(&str, &BoardTemplate)]) {
    std::fs::create_dir_all(folder.join("board-templates")).unwrap();
    let info = LibraryInfo {
        uuid: id(n << 100),
        name: format!("Lib {n}"),
        description: String::new(),
    };
    std::fs::write(
        folder.join(format!("lib{n}.redalib")),
        write_library_info(&info).unwrap(),
    )
    .unwrap();
    for (stem, template) in templates {
        std::fs::write(
            folder.join(format!("board-templates/{stem}.redaboard")),
            write_board_template(template).unwrap(),
        )
        .unwrap();
    }
}

/// An engine with the starter Library and a user Library at `<work>/user-lib` holding
/// the fixture (and, as "Orphan board", a copy whose header is in no Library).
fn engine_with_fixture(home: &Path, work: &Path) -> TestEngine {
    let user_lib: PathBuf = work.join("user-lib");
    let mut orphan = fixture(Some(&s(999)));
    orphan.uuid = id(101_000);
    orphan.name = "Orphan board".into();
    write_library(
        &user_lib,
        1,
        &[("Fixture", &fixture(None)), ("Orphan", &orphan)],
    );
    let mut engine = TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .user_dirs(Some(UserDirs::under(home)))
            .starter_library(Some(starter_library()))
            .commands(reda_testkit::probe_commands)
            .build(),
    );
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": [user_lib]}),
    );
    let listed = engine.run_ok("library.board_template.list", json!({}));
    assert_eq!(listed["problems"], json!([]), "{listed}");
    engine
}

/// `seeded.template.objects` by template object.
fn created(seeded: &Value) -> std::collections::BTreeMap<String, Value> {
    seeded["template"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| (o["template_object"].as_str().unwrap().to_owned(), o.clone()))
        .collect()
}

/// The objects of `kind` in `document`.
fn query(engine: &mut TestEngine, document: &Value, kind: &str) -> Vec<Value> {
    let out = engine.run_ok("objects.query", json!({"document": document, "type": kind}));
    out["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .collect()
}

fn get(engine: &mut TestEngine, uuid: &Value) -> Value {
    engine.run_ok("object.get", json!({"uuid": uuid}))["object"].clone()
}

#[test]
fn project_new_writes_every_kind_of_template_object() {
    let home = TempDir::new("template-home");
    let work = TempDir::new("template-work");
    let mut engine = engine_with_fixture(home.path(), work.path());
    let out = engine.run_ok(
        "project.new",
        json!({"path": work.path(), "name": "Card",
               "board_template": {"element": s(100), "values": {"finish": "fancy"}}}),
    );
    let seeded = &out["seeded"];
    // Omitted `layers` takes the suggestion: 4 copper Layers.
    assert_eq!(
        seeded["template"]["values"],
        json!({"finish": "plain", "height": 40_000_000, "size": "small"})
    );
    let objects = created(seeded);
    let order: Vec<&str> = seeded["template"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["template_object"].as_str().unwrap())
        .collect();
    // The gated-out connector is absent; everything else is there once.
    assert_eq!(
        order,
        [
            101, 102, 130, 131, 110, 120, 121, 140, 150, 151, 141, 152, 153
        ]
        .map(s)
    );

    // Outline: zero-length edge dropped, the plated edge on no Net, Locked.
    let outline = get(&mut engine, &objects[&s(101)]["created"]);
    assert_eq!(outline["locked"], true);
    assert_eq!(
        outline["vertices"],
        json!([
            {"x": 0, "y": 0, "plated": {}},
            {"x": 60_000_000, "y": 0},
            {"x": 60_000_000, "y": 40_000_000},
            {"x": 0, "y": 40_000_000}
        ])
    );
    assert_eq!(seeded["outline"], objects[&s(101)]["created"]);
    let cutout = get(&mut engine, &objects[&s(102)]["created"]);
    assert_eq!(cutout["locked"], true);

    // Keepouts: the symbolic layer mapped, `restrict` copied.
    let protecting = get(&mut engine, &objects[&s(130)]["created"]);
    let layers = get(&mut engine, &seeded["layer_set"]);
    let copper: Vec<Value> = layers["layers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["kind"] == "copper")
        .map(|l| l["uuid"].clone())
        .collect();
    // Omitted `layers` takes the suggestion: 4 copper Layers.
    assert_eq!(copper.len(), 4);
    assert_eq!(protecting["layer"], copper[3], "{protecting}");
    assert_eq!(protecting["restrict"], json!(["tracks", "vias"]));
    assert_eq!(protecting["locked"], true);
    let all = get(&mut engine, &objects[&s(131)]["created"]);
    assert!(all.get("layer").is_none_or(Value::is_null), "{all}");

    // The connector: a linked Component, Symbol on the first sheet, Footprint at `at`.
    let connector = &objects[&s(110)];
    assert_eq!(connector["designator"], "J1");
    let footprint = get(&mut engine, &connector["created"]);
    assert_eq!(footprint["at"], json!({"x": 10_000_000, "y": 20_000_000}));
    assert_eq!(footprint["rotation"], 90_000);
    assert_eq!(footprint["side"], "bottom");
    assert_eq!(footprint["locked"], true);
    assert_eq!(footprint["component"]["uuid"], connector["component"]);
    let sheet = out["documents"][0]["uuid"].clone();
    let units = query(&mut engine, &sheet, "unit");
    assert_eq!(units.len(), 1, "{units:?}");
    assert_eq!(units[0]["component"]["uuid"], connector["component"]);
    assert_eq!(units[0]["designator"], "J1");
    assert_eq!(units[0]["footprint"], HEADER_1X02_SECOND);
    // No Net Labels or wires.
    assert_eq!(query(&mut engine, &sheet, "net_label"), Vec::<Value>::new());
    assert_eq!(query(&mut engine, &sheet, "wire"), Vec::<Value>::new());
    // The same state as placing, syncing and moving: nothing pending.
    let changes = engine.run_ok("sync.preview", json!({}));
    assert_eq!(changes["count"], 0, "{changes}");
    // Built from a starter Part, it is Up to Date.
    let status = engine.run_ok("schematic.component.library_status", json!({}));
    assert_eq!(status["components"][0]["status"], "up_to_date", "{status}");

    // Holes: a Mechanical Part placed directly with a Designator, and a Free Pad.
    let hole = &objects[&s(120)];
    assert_eq!(hole["designator"], "H1");
    assert!(hole.get("component").is_none());
    let mechanical = get(&mut engine, &hole["created"]);
    assert!(mechanical["component"].is_null(), "{mechanical}");
    assert_eq!(mechanical["at"], json!({"x": 55_000_000, "y": 35_000_000}));
    assert_eq!(mechanical["locked"], true);
    let pad = get(&mut engine, &objects[&s(121)]["created"]);
    assert_eq!(pad["at"], json!({"x": 5_000_000, "y": 5_000_000}));
    assert_eq!(pad["locked"], true);
    assert!(pad.get("net").is_none_or(Value::is_null), "{pad}");

    // Drawings: on their group's Layer, Locked, Dimension ends following the new objects.
    let group = &objects[&s(140)];
    assert!(group.get("created").is_none(), "{group}");
    let dimension = get(&mut engine, &objects[&s(150)]["created"]);
    assert_eq!(dimension["locked"], true);
    assert_eq!(
        dimension["a"],
        json!({"point": {"x": 0, "y": 40_000_000},
               "ref": {"object": objects[&s(101)]["created"], "hotspot": {"vertex": 3}}})
    );
    assert_eq!(
        dimension["b"]["ref"],
        json!({"object": objects[&s(121)]["created"], "hotspot": "center"})
    );
    assert_eq!(dimension["offset"], -3_000_000);
    assert_eq!(dimension["suffix"], " mm");
    let to_hole = get(&mut engine, &objects[&s(151)]["created"]);
    assert_eq!(
        to_hole["a"]["ref"],
        json!({"object": hole["created"], "hotspot": "origin"})
    );
    assert!(to_hole["b"].get("ref").is_none(), "{to_hole}");
    let text = get(&mut engine, &objects[&s(152)]["created"]);
    assert_eq!(text["text"], "HOST");
    assert_eq!(text["height"], mm(2.0));
    let layer_of = |kind: &str| {
        layers["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["kind"] == kind)
            .unwrap()["uuid"]
            .clone()
    };
    assert_eq!(dimension["layer"], layer_of("mechanical"));
    assert_eq!(text["layer"], layer_of("documentation"));
    let line = get(&mut engine, &objects[&s(153)]["created"]);
    assert_eq!(line["layer"], layer_of("documentation"));

    // The edge connector property is copied.
    let setup = engine.run_ok("object.get", json!({"uuid": seeded["board"]}))["object"].clone();
    assert_eq!(
        setup["edge_connector"],
        json!({"bevel": {"angle": 20_000, "depth": 1_300_000}, "hard_gold": true})
    );
}

#[test]
fn a_plated_edge_from_a_template_warns_it_has_no_net_until_one_is_set() {
    let home = TempDir::new("template-home");
    let work = TempDir::new("template-work");
    let mut engine = engine_with_fixture(home.path(), work.path());
    let out = engine.run_ok(
        "project.new",
        json!({"path": work.path(), "name": "Card", "board_template": {"element": s(100)}}),
    );
    let outline = out["seeded"]["outline"].clone();
    let warnings = |engine: &mut TestEngine| -> Vec<Value> {
        engine.run_ok("drc.run", json!({"checks": ["plated_edge_without_net"]}))["violations"]
            .as_array()
            .unwrap()
            .clone()
    };
    let found = warnings(&mut engine);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0]["severity"], "warning");
    assert_eq!(found[0]["objects"], json!([outline]));
    assert_eq!(found[0]["args"]["edge"], 0);

    let gnd = engine.run_ok("test.net.add", json!({"name": "GND"}));
    engine.run_ok(
        "pcb.contour.edge.set",
        json!({"contour": outline, "edge": 0, "plated": {"net": gnd["result"]["uuid"]}}),
    );
    assert_eq!(warnings(&mut engine), Vec::<Value>::new());
}

#[test]
fn excluding_a_hole_and_a_group_drops_what_goes_with_them() {
    let home = TempDir::new("template-home");
    let work = TempDir::new("template-work");
    let mut engine = engine_with_fixture(home.path(), work.path());
    let out = engine.run_ok(
        "project.new",
        json!({"path": work.path(), "name": "Card", "layers": 2, "board_template": {
            "element": s(100),
            // The large-only connector is gated out at `small`: accepted and ignored.
            "exclude": [s(120), s(141), s(111)],
            "values": {"height": "50mm"}}}),
    );
    let seeded = &out["seeded"];
    let objects = created(seeded);
    // The Keepout protecting the hole and the Dimension that follows it go with it; the
    // Documentation group goes with its drawings.
    for gone in [120, 130, 151, 141, 152, 153, 111] {
        assert!(!objects.contains_key(&s(gone)), "{} is there", s(gone));
    }
    for kept in [101, 102, 131, 110, 121, 140, 150] {
        assert!(objects.contains_key(&s(kept)), "{} is missing", s(kept));
    }
    // An explicit `layers` wins over the suggestion.
    assert_eq!(seeded["layers"].as_array().unwrap().len(), 15);
    let outline = get(&mut engine, &objects[&s(101)]["created"]);
    assert_eq!(
        outline["vertices"][2],
        json!({"x": 60_000_000, "y": 50_000_000})
    );
}

#[test]
fn document_add_makes_the_board_beside_the_first_sheet_s_content_in_one_edit() {
    let home = TempDir::new("template-home");
    let work = TempDir::new("template-work");
    let mut engine = engine_with_fixture(home.path(), work.path());
    let out = engine.run_ok("project.new", json!({"path": work.path(), "name": "Card"}));
    let sheet = out["documents"][0]["uuid"].clone();
    let pcb = out["documents"][1]["uuid"].clone();
    engine.run_ok("document.delete", json!({"document": pcb}));
    let resistor = engine.run_ok(
        "schematic.component.place",
        json!({"document": sheet, "part": RESISTOR, "at": {"x": "100mm", "y": "60mm"}}),
    );
    // A Project Library's template is offered to Add PCB Document.
    write_library(
        &work.path().join("Card").join("lib"),
        2,
        &[("Mine", &{
            let mut t = fixture(None);
            t.uuid = id(102_000);
            t.name = "Project board".into();
            t
        })],
    );
    engine.run_ok("project.library.add", json!({"path": "lib"}));
    let added = engine.run_ok(
        "document.add",
        json!({"kind": "pcb", "board_template": {"element": s(102_000)}}),
    );
    let objects = created(&added["result"]["seeded"]);
    let connector = &objects[&s(110)];
    // R1 holds its Designator; the connector is J1, to the right of it.
    assert_eq!(connector["designator"], "J1");
    let units = query(&mut engine, &sheet, "unit");
    let j1 = units.iter().find(|u| u["designator"] == "J1").unwrap();
    let r1 = units
        .iter()
        .find(|u| u["uuid"] == resistor["result"]["unit"])
        .unwrap();
    assert!(
        j1["at"]["x"].as_i64().unwrap() > r1["at"]["x"].as_i64().unwrap() + mm(5.0),
        "{j1} {r1}"
    );
    // Only R1, placed by hand, is pending; the connector is in sync.
    let changes = engine.run_ok("sync.preview", json!({}));
    assert!(changes.to_string().contains("R1"), "{changes}");
    assert!(!changes.to_string().contains("J1"), "{changes}");

    // It is one Journal entry; undoing it on the Project and the sheet removes it all.
    let ids: Vec<&str> = engine.journal().iter().map(|e| e.id.as_str()).collect();
    let after = ids
        .iter()
        .position(|id| *id == "project.library.add")
        .unwrap();
    let edits: Vec<&&str> = ids[after + 1..]
        .iter()
        .filter(|id| !id.ends_with(".query") && !id.ends_with(".preview"))
        .collect();
    assert_eq!(edits, [&"document.add"]);
    engine.run_ok("edit.undo", json!({"document": "project"}));
    engine.run_ok("edit.undo", json!({"document": sheet}));
    let overview = engine.run_ok("project.overview", json!({}));
    assert_eq!(
        overview["documents"].as_array().unwrap().len(),
        1,
        "{overview}"
    );
    assert_eq!(query(&mut engine, &sheet, "unit").len(), 1);
}

#[test]
fn a_part_no_library_holds_still_creates_and_shows_source_missing() {
    let home = TempDir::new("template-home");
    let work = TempDir::new("template-work");
    let mut engine = engine_with_fixture(home.path(), work.path());
    let out = engine.run_ok(
        "project.new",
        json!({"path": work.path(), "name": "Orphan",
               "board_template": {"element": id(101_000).to_string()}}),
    );
    let objects = created(&out["seeded"]);
    assert!(objects.contains_key(&s(110)));
    let status = engine.run_ok("schematic.component.library_status", json!({}));
    assert_eq!(
        status["components"][0]["status"], "source_missing",
        "{status}"
    );
}

#[test]
fn refusals_write_nothing_and_carry_the_preview_s_problems() {
    let home = TempDir::new("template-home");
    let work = TempDir::new("template-work");
    let mut engine = engine_with_fixture(home.path(), work.path());
    let refuse = |engine: &mut TestEngine, name: &str, choice: Value| {
        let err = engine.run_err(
            "project.new",
            json!({"path": work.path(), "name": name, "board_template": choice}),
        );
        assert_eq!(err.code, ErrorCode::InvalidParams, "{name}: {err:?}");
        assert!(!work.path().join(name).exists(), "{name} wrote a folder");
        err
    };
    let rect = |values: Value| json!({"element": CUSTOM_RECTANGLE, "values": values});

    let err = refuse(
        &mut engine,
        "Invalid",
        rect(json!({"width": "10", "corner_radius": "6"})),
    );
    let details = err.details.unwrap();
    assert_eq!(details["pointer"], "/board_template");
    let problems = details["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0]["kind"], "outline");

    let err = refuse(&mut engine, "Unknown", json!({"element": s(7)}));
    assert_eq!(err.details.unwrap()["pointer"], "/board_template/element");
    let err = refuse(&mut engine, "Short", rect(json!({"width": "0.5mm"})));
    assert_eq!(
        err.details.unwrap()["pointer"],
        "/board_template/values/width"
    );
    let err = refuse(
        &mut engine,
        "Option",
        json!({"element": s(100), "values": {"size": "huge"}}),
    );
    assert_eq!(
        err.details.unwrap()["pointer"],
        "/board_template/values/size"
    );
    let err = refuse(
        &mut engine,
        "Exclude",
        json!({"element": s(100), "exclude": [s(77)]}),
    );
    assert_eq!(err.details.unwrap()["pointer"], "/board_template/exclude/0");
    let err = refuse(
        &mut engine,
        "Keepout",
        json!({"element": s(100), "exclude": [s(130)]}),
    );
    assert!(err.message.contains("Keepout"), "{err:?}");

    // A Project Library's template is not available to a new Project.
    engine.run_ok("project.new", json!({"path": work.path(), "name": "Host"}));
    write_library(
        &work.path().join("Host").join("lib"),
        3,
        &[("Mine", &{
            let mut t = fixture(None);
            t.uuid = id(103_000);
            t.name = "Project board".into();
            t
        })],
    );
    engine.run_ok("project.library.add", json!({"path": "lib"}));
    let err = refuse(
        &mut engine,
        "FromProject",
        json!({"element": id(103_000).to_string()}),
    );
    assert!(
        err.message.contains("not available to a new Project"),
        "{err:?}"
    );

    // document.add refuses too, and the Project keeps no PCB Document.
    let out = engine.run_ok("project.overview", json!({}));
    let pcb = out["documents"][1]["uuid"].clone();
    engine.run_ok("document.delete", json!({"document": pcb}));
    let err = engine.run_err(
        "document.add",
        json!({"kind": "pcb", "board_template": rect(json!({"height": "2000mm"}))}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams, "{err:?}");
    let out = engine.run_ok("project.overview", json!({}));
    assert_eq!(out["documents"].as_array().unwrap().len(), 1, "{out}");
}
