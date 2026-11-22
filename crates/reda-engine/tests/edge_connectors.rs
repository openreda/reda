//! Edge Connector Pads and Castellated Pads (docs/spec/v0.2.5/edge-connectors.md): the
//! Pad role `edge_connector` on SMD Pads and `castellated` on plated through-hole Pads and
//! Free Pads, set through the Footprint editor's and the board's Pad Commands, and the
//! `edge_clearance` exemption they get against the Board Outline.

use reda_engine::command::ErrorCode;
use reda_engine::model::{LibraryElement, PadRole};
use reda_testkit::{TempDir, TestEngine, assert_golden, probe_engine, test_engine_in};
use serde_json::{Value, json};

const PCB: &str = "00000000-0000-4000-8000-000000000003";
const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";

fn mm(v: f64) -> i64 {
    (v * 1_000_000.0).round() as i64
}

fn pt(x: f64, y: f64) -> Value {
    json!({"x": mm(x), "y": mm(y)})
}

/// A 30 × 20 mm board.
fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "30mm", "height": "20mm"}),
    );
    engine
}

/// A Footprint with one SMD and one plated through-hole Pad; its instance and Pads.
fn two_pads(engine: &mut TestEngine) -> (Value, Value, Value) {
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(10.0, 10.0), "designator": "J1", "pads": [
            {"at": pt(0.0, 0.0), "width": "1mm", "height": "2mm"},
            {"at": pt(3.0, 0.0), "width": "1.6mm", "height": "1.6mm",
             "kind": "plated_through_hole", "hole": {"width": 800_000, "height": 800_000}}]}),
    )["result"]
        .clone();
    (
        out["footprint"].clone(),
        out["pads"][0].clone(),
        out["pads"][1].clone(),
    )
}

fn placed_pad(engine: &mut TestEngine, instance: &Value, pad: &Value) -> Value {
    let objects = |engine: &mut TestEngine, kind: &str| -> Vec<Value> {
        engine.run_ok("objects.query", json!({"type": kind, "document": PCB}))["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["object"].clone())
            .collect()
    };
    let instance = objects(engine, "footprint_instance")
        .into_iter()
        .find(|i| i["uuid"] == *instance)
        .unwrap();
    let copy = objects(engine, "footprint_copy")
        .into_iter()
        .find(|c| c["uuid"] == instance["footprint"])
        .unwrap();
    copy["footprint"]["pads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["uuid"] == *pad)
        .unwrap()
        .clone()
}

fn free_pad(engine: &mut TestEngine, uuid: &Value) -> Value {
    engine.run_ok(
        "objects.query",
        json!({"type": "free_pad", "document": PCB}),
    )["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .find(|o| o["uuid"] == *uuid)
        .unwrap()
}

#[test]
fn a_placed_smd_pad_takes_the_edge_connector_role_and_other_kinds_are_refused() {
    let dir = TempDir::new("edge-connector-role");
    let mut engine = board(&dir);
    let (instance, smd, pth) = two_pads(&mut engine);
    engine.run_ok(
        "pcb.pad.edit",
        json!({"instance": instance, "pad": smd, "role": "edge_connector"}),
    );
    assert_eq!(
        placed_pad(&mut engine, &instance, &smd)["role"],
        "edge_connector"
    );

    let refused = engine.run_err(
        "pcb.pad.edit",
        json!({"instance": instance, "pad": pth, "role": "edge_connector"}),
    );
    assert_eq!(refused.code, ErrorCode::InvalidParams);
    assert_eq!(refused.details.unwrap()["pointer"], "/role");
    // Turning an Edge Connector Pad plated would leave a role it can't have.
    let refused = engine.run_err(
        "pcb.pad.edit",
        json!({"instance": instance, "pad": smd, "kind": "plated_through_hole",
               "hole": {"width": "0.8mm"}}),
    );
    assert_eq!(refused.details.unwrap()["pointer"], "/role");

    engine.run_ok(
        "pcb.pad.edit",
        json!({"instance": instance, "pad": smd, "role": null}),
    );
    assert!(
        placed_pad(&mut engine, &instance, &smd)
            .get("role")
            .is_none()
    );
}

#[test]
fn a_placed_plated_pad_is_castellated_and_other_kinds_are_refused() {
    let dir = TempDir::new("castellated-pad");
    let mut engine = board(&dir);
    let (instance, smd, pth) = two_pads(&mut engine);
    engine.run_ok(
        "pcb.pad.edit",
        json!({"instance": instance, "pad": pth, "castellated": true}),
    );
    assert_eq!(
        placed_pad(&mut engine, &instance, &pth)["castellated"],
        true
    );

    let refused = engine.run_err(
        "pcb.pad.edit",
        json!({"instance": instance, "pad": smd, "castellated": true}),
    );
    assert_eq!(refused.code, ErrorCode::InvalidParams);
    assert_eq!(refused.details.unwrap()["pointer"], "/castellated");

    engine.run_ok(
        "pcb.pad.edit",
        json!({"instance": instance, "pad": pth, "castellated": false}),
    );
    assert!(
        placed_pad(&mut engine, &instance, &pth)
            .get("castellated")
            .is_none()
    );
}

#[test]
fn a_plated_free_pad_is_castellated_and_a_bare_hole_is_refused() {
    let dir = TempDir::new("castellated-free-pad");
    let mut engine = board(&dir);
    let plated = engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "at": pt(0.0, 10.0), "width": "2mm",
               "hole": {"width": "1mm"}, "castellated": true}),
    )["result"]["uuid"]
        .clone();
    assert_eq!(free_pad(&mut engine, &plated)["castellated"], true);
    engine.run_ok(
        "pcb.free_pad.edit",
        json!({"pad": plated, "castellated": false}),
    );
    assert!(free_pad(&mut engine, &plated).get("castellated").is_none());
    engine.run_ok(
        "pcb.free_pad.edit",
        json!({"pad": plated, "castellated": true}),
    );
    assert_eq!(free_pad(&mut engine, &plated)["castellated"], true);

    let bare = engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "at": pt(5.0, 5.0), "width": "1mm",
               "hole": {"width": "1mm"}}),
    )["result"]["uuid"]
        .clone();
    let refused = engine.run_err(
        "pcb.free_pad.edit",
        json!({"pad": bare, "castellated": true}),
    );
    assert_eq!(refused.code, ErrorCode::InvalidParams);
    assert_eq!(refused.details.unwrap()["pointer"], "/castellated");
    let refused = engine.run_err(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": pt(5.0, 5.0), "width": "1mm", "castellated": true}),
    );
    assert_eq!(refused.details.unwrap()["pointer"], "/castellated");
}

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
        json!({"library": LIBRARY, "kind": "footprint", "name": "Fingers"}),
    );
    (e, out["element"].as_str().unwrap().to_owned())
}

fn library_pads(e: &TestEngine, fp: &str) -> Vec<reda_engine::model::Pad> {
    match e.element(fp.parse().unwrap()) {
        Some(LibraryElement::Footprint(f)) => f.pads.clone(),
        _ => panic!("open"),
    }
}

#[test]
fn the_footprint_editor_marks_fingers_and_castellations_on_the_pads_that_can_have_them() {
    let (home, work) = (TempDir::new("fp-role-home"), TempDir::new("fp-role-work"));
    let (mut e, fp) = new_footprint(&home, &work);
    let finger = e.run_ok(
        "library.footprint.pad.add",
        json!({"footprint": fp, "number": "1", "kind": "smd", "shape": "rect",
               "at": {"x": "0mm", "y": "0mm"}, "width": "0.6mm", "height": "4mm",
               "role": "edge_connector"}),
    )["result"]["pad"]
        .as_str()
        .unwrap()
        .to_owned();
    let half_hole = e.run_ok(
        "library.footprint.pad.add",
        json!({"footprint": fp, "number": "2", "kind": "plated_through_hole",
               "shape": "circle", "at": {"x": "3mm", "y": "0mm"}, "width": "1.6mm",
               "height": "1.6mm", "hole": {"width": "1mm"}}),
    )["result"]["pad"]
        .as_str()
        .unwrap()
        .to_owned();
    e.run_ok(
        "library.footprint.pad.edit",
        json!({"footprint": fp, "pad": half_hole, "castellated": true}),
    );
    let pads = library_pads(&e, &fp);
    let by = |uuid: &str| pads.iter().find(|p| p.uuid.to_string() == uuid).unwrap();
    assert_eq!(by(&finger).role, Some(PadRole::EdgeConnector));
    assert!(!by(&finger).castellated);
    assert!(by(&half_hole).castellated);
    assert_eq!(by(&half_hole).role, None);

    for (pad, field, value) in [
        (&half_hole, "role", json!("edge_connector")),
        (&finger, "castellated", json!(true)),
    ] {
        let refused = e.run_err(
            "library.footprint.pad.edit",
            json!({"footprint": fp, "pad": pad, field: value}),
        );
        assert_eq!(refused.code, ErrorCode::InvalidParams);
        assert_eq!(refused.details.unwrap()["pointer"], format!("/{field}"));
    }
}

/// Two identical SMD Pads touching the board's left edge, one an Edge Connector Pad, and
/// one crossing it; Board Cutout and non-plated hole beside an Edge Connector Pad; a
/// castellated Free Pad cut by the right edge.
fn edge_board(dir: &TempDir) -> TestEngine {
    let mut engine = board(dir);
    let pads = |rows: &[(f64, f64)]| -> Vec<Value> {
        rows.iter()
            .map(|&(x, y)| json!({"at": pt(x, y), "width": "1mm", "height": "2mm"}))
            .collect()
    };
    // The left edge is x = 0: each Pad's copper runs from x = 0 to x = 1 mm.
    let normal = engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(0.5, 4.0), "designator": "J1", "pads": pads(&[(0.0, 0.0)])}),
    )["result"]
        .clone();
    let fingers = engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(0.5, 10.0), "designator": "J2",
               "pads": pads(&[(0.0, 0.0), (-0.3, 4.0)])}),
    )["result"]
        .clone();
    for pad in fingers["pads"].as_array().unwrap() {
        engine.run_ok(
            "pcb.pad.edit",
            json!({"instance": fingers["footprint"], "pad": pad, "role": "edge_connector"}),
        );
    }
    let _ = normal;
    // A cutout and a bare hole still keep their clearance from an Edge Connector Pad.
    let near = engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(10.0, 10.0), "designator": "J3", "pads": pads(&[(0.0, 0.0), (6.0, 0.0)])}),
    )["result"]
        .clone();
    for pad in near["pads"].as_array().unwrap() {
        engine.run_ok(
            "pcb.pad.edit",
            json!({"instance": near["footprint"], "pad": pad, "role": "edge_connector"}),
        );
    }
    engine.run_ok(
        "pcb.board_cutout.add",
        json!({"vertices": [pt(10.6, 9.0), pt(12.0, 9.0), pt(12.0, 11.0), pt(10.6, 11.0)]}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "at": pt(16.9, 10.0), "width": "0.6mm",
               "hole": {"width": "0.6mm"}}),
    );
    // A castellated Pad is centred on the right edge.
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "at": pt(30.0, 10.0), "width": "1.6mm",
               "hole": {"width": "1mm"}, "castellated": true}),
    );
    engine
}

#[test]
fn edge_clearance_spares_edge_connector_pads_at_the_outline_but_not_across_it() {
    let dir = TempDir::new("edge-connector-drc");
    let mut engine = edge_board(&dir);
    let mut report = engine.run_ok("drc.run", json!({"checks": ["edge_clearance"]}));
    let violations = report["violations"].as_array().unwrap();
    let designators = |engine: &mut TestEngine, v: &Value| -> Vec<String> {
        let instances = engine.run_ok(
            "objects.query",
            json!({"type": "footprint_instance", "document": PCB}),
        )["objects"]
            .as_array()
            .unwrap()
            .clone();
        v["objects"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|o| {
                instances
                    .iter()
                    .find(|i| i["object"]["uuid"] == *o)
                    .map(|i| i["object"]["designator"].as_str().unwrap().to_owned())
            })
            .collect()
    };
    let mut hits: Vec<String> = violations
        .iter()
        .flat_map(|v| designators(&mut engine, v))
        .collect();
    hits.sort();
    // J1's normal Pad at the outline fires; J2's finger touching it does not, while its
    // finger that crosses it does; J3's fingers still keep clear of the cutout and the
    // bare hole. The castellated Free Pad fires nothing.
    assert_eq!(hits, ["J1", "J2", "J3", "J3"], "{report:#}");
    assert_eq!(violations.len(), 4, "{report:#}");
    report.as_object_mut().unwrap().remove("design");
    let text = serde_json::to_string_pretty(&report).unwrap() + "\n";
    assert_golden(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden/edge_connectors/drc.json"),
        text.as_bytes(),
    );
}

/// A Footprint at `at` whose Pads are each `(x, y, castellated)` mm from its origin: a
/// Ø5.5 plated land on a Ø3.5 hole for a castellated one, a 1 × 2 mm SMD Pad otherwise,
/// with an optional R3.25 top courtyard round the origin.
fn hole_footprint(
    engine: &mut TestEngine,
    designator: &str,
    at: (f64, f64),
    pads: &[(f64, f64, bool)],
    courtyard: bool,
) -> Value {
    let pads: Vec<Value> = pads
        .iter()
        .map(|&(x, y, castellated)| {
            if castellated {
                json!({"at": pt(x, y), "width": "5.5mm", "height": "5.5mm", "shape": "circle",
                       "kind": "plated_through_hole", "castellated": true,
                       "hole": {"width": 3_500_000, "height": 3_500_000}})
            } else {
                json!({"at": pt(x, y), "width": "1mm", "height": "2mm"})
            }
        })
        .collect();
    let graphics: Vec<Value> = if courtyard {
        vec![json!({"layer": "courtyard_top", "width": "0.05mm",
                    "shape": {"kind": "circle", "center": pt(0.0, 0.0), "radius": mm(3.25)}})]
    } else {
        vec![]
    };
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(at.0, at.1), "designator": designator, "pads": pads,
               "graphics": graphics}),
    )["result"]["footprint"]
        .clone()
}

#[test]
fn a_castellated_pad_the_outline_cuts_may_take_its_footprint_across_the_outline() {
    // edge-connectors.md#castellated-pads: a Castellated Pad is meant to cross the outline,
    // so `footprint_outside_board` spares each piece of its Footprint that sticks out with
    // it. A piece outside the board with no such Pad in it still fires.
    let dir = TempDir::new("castellated-outside");
    let mut engine = board(&dir);
    // H1: a half-moon centred on the right edge, its Pad and courtyard both cut in half.
    let on_edge = hole_footprint(&mut engine, "H1", (30.0, 10.0), &[(0.0, 0.0, true)], true);
    // H2: the same, wholly off the board: the outline cuts nothing.
    let off_board = hole_footprint(&mut engine, "H2", (40.0, 10.0), &[(0.0, 0.0, true)], true);
    // H3: a Castellated Pad on the top edge and a plain Pad below the bottom edge. With no
    // courtyard its extent is the box round both, which sticks out at the top and bottom.
    let two_ends = hole_footprint(
        &mut engine,
        "H3",
        (15.0, 20.0),
        &[(0.0, 0.0, true), (0.0, -25.0, false)],
        false,
    );
    // H4: a plain plated Pad centred on the left edge is still outside the board.
    let plain = engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(0.0, 10.0), "designator": "H4", "pads": [
            {"at": pt(0.0, 0.0), "width": "5.5mm", "height": "5.5mm", "shape": "circle",
             "kind": "plated_through_hole", "hole": {"width": 3_500_000, "height": 3_500_000}}]}),
    )["result"]["footprint"]
        .clone();
    let report = engine.run_ok("drc.run", json!({"checks": ["footprint_outside_board"]}));
    let mut hits: Vec<Value> = report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["objects"][0].clone())
        .collect();
    hits.sort_by_key(|v| v.to_string());
    let mut expected = vec![off_board, two_ends, plain];
    expected.sort_by_key(|v| v.to_string());
    assert_eq!(hits, expected, "{report:#}");
    assert!(!hits.contains(&on_edge));
}
