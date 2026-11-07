//! What loading a `.redapcb` checks about Layers, the Stackup and Via spans.

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::{Value, json};

/// A saved Project with one Via, and the path of its PCB file.
fn saved(label: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new(label);
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok("pcb.via.add", json!({"at": {"x": 0, "y": 0}}));
    engine.run_ok("project.save", json!({}));
    let path = dir.path().join("Demo/Demo.redapcb");
    (dir, path)
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// Rewrites the PCB file through `change`, then opens the Project.
fn open_after(path: &Path, change: impl FnOnce(&mut Value)) -> Result<TestEngine, String> {
    let mut file = read(path);
    change(&mut file);
    std::fs::write(path, serde_json::to_vec_pretty(&file).unwrap()).unwrap();
    let mut engine = test_engine();
    let folder = path.parent().unwrap();
    match engine.run("project.open", json!({"path": folder})) {
        Ok(_) => Ok(engine),
        Err(err) => {
            assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);
            Err(err.message)
        }
    }
}

/// The index of the first Layer of this kind and side in the file's Layer list.
fn layer_index(file: &Value, kind: &str, side: Option<&str>) -> usize {
    file["layers"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .position(|l| l["kind"] == kind && l.get("side").and_then(Value::as_str) == side)
        .unwrap()
}

#[test]
fn a_board_as_saved_opens() {
    let (_dir, path) = saved("load-ok");
    open_after(&path, |_| {}).unwrap();
}

#[test]
fn the_outer_copper_must_be_sided_top_and_bottom() {
    for (side, expect) in [(json!("bottom"), "top copper"), (Value::Null, "top copper")] {
        let (_dir, path) = saved("load-copper-side");
        let err = open_after(&path, |f| {
            let i = layer_index(f, "copper", Some("top"));
            let layer = f["layers"]["layers"][i].as_object_mut().unwrap();
            match &side {
                Value::Null => layer.remove("side"),
                side => layer.insert("side".into(), side.clone()),
            };
        })
        .err()
        .expect("refused");
        assert!(err.contains(expect), "{err}");
    }
}

#[test]
fn a_sided_layer_needs_its_side_and_the_others_have_none() {
    let (_dir, path) = saved("load-silk-side");
    let err = open_after(&path, |f| {
        let i = layer_index(f, "silkscreen", Some("top"));
        f["layers"]["layers"][i]
            .as_object_mut()
            .unwrap()
            .remove("side");
    })
    .err()
    .expect("refused");
    assert!(err.contains("needs a side"), "{err}");

    let (_dir, path) = saved("load-silk-wrong-face");
    let err = open_after(&path, |f| {
        let i = layer_index(f, "silkscreen", Some("top"));
        f["layers"]["layers"][i]["side"] = json!("bottom");
    })
    .err()
    .expect("a bottom Layer above the copper is refused");
    assert!(err.contains("other side"), "{err}");

    let (_dir, path) = saved("load-outline-side");
    let err = open_after(&path, |f| {
        let i = layer_index(f, "board_outline", None);
        f["layers"]["layers"][i]["side"] = json!("top");
    })
    .err()
    .expect("refused");
    assert!(err.contains("has no side"), "{err}");
}

#[test]
fn inner_copper_is_copper_with_no_side_and_is_numbered_from_its_position() {
    let (_dir, path) = saved("load-inner");
    let inner = "00000000-0000-4000-8000-0000000000aa";
    let dielectric = "00000000-0000-4000-8000-0000000000ab";
    let engine = open_after(&path, |f| {
        let i = layer_index(f, "copper", Some("bottom"));
        let layers = f["layers"]["layers"].as_array_mut().unwrap();
        layers.insert(i, json!({"uuid": inner, "kind": "copper"}));
        let entries = f["stackups"][0]["entries"].as_array_mut().unwrap();
        entries.insert(2, json!({"layer": inner, "thickness": 35_000}));
        entries.insert(3, json!({"uuid": dielectric}));
    })
    .unwrap();
    let pcb = engine.project().unwrap().pcb().unwrap();
    let numbers: Vec<_> = pcb
        .copper_layers()
        .map(|l| (l.side, pcb.copper_number(l.uuid)))
        .collect();
    use reda_engine::model::Side;
    assert_eq!(
        numbers,
        [
            (Some(Side::Top), Some(1)),
            (None, Some(2)),
            (Some(Side::Bottom), Some(3)),
        ]
    );
}

#[test]
fn the_stackup_names_every_copper_layer_in_order() {
    let (_dir, path) = saved("load-stackup-order");
    let err = open_after(&path, |f| {
        let entries = f["stackups"][0]["entries"].as_array_mut().unwrap();
        entries.swap(0, 2);
    })
    .err()
    .expect("refused");
    assert!(err.contains("one copper entry per copper Layer"), "{err}");

    let (_dir, path) = saved("load-stackup-missing");
    let err = open_after(&path, |f| f["stackups"] = json!([]))
        .err()
        .expect("refused");
    assert!(err.contains("exactly one Stackup"), "{err}");

    let (_dir, path) = saved("load-stackup-shape");
    let err = open_after(&path, |f| {
        f["stackups"][0]["entries"]
            .as_array_mut()
            .unwrap()
            .remove(1);
    })
    .err()
    .expect("two copper entries in a row are refused");
    assert!(err.contains("alternate"), "{err}");
}

#[test]
fn a_stored_negative_remainder_is_refused() {
    let (_dir, path) = saved("load-remainder");
    let err = open_after(&path, |f| f["stackups"][0]["thickness"] = json!(50_000))
        .err()
        .expect("refused");
    assert!(err.contains("exceed"), "{err}");
}

#[test]
fn a_through_via_is_stored_top_to_bottom() {
    let (_dir, path) = saved("load-via");
    let file = read(&path);
    let top = file["layers"]["layers"][layer_index(&file, "copper", Some("top"))]["uuid"].clone();
    let bottom =
        file["layers"]["layers"][layer_index(&file, "copper", Some("bottom"))]["uuid"].clone();
    assert_eq!(file["vias"][0]["span"], json!({"from": top, "to": bottom}));

    let err = open_after(&path, |f| {
        f["vias"][0]["span"] = json!({"from": bottom, "to": top});
    })
    .err()
    .expect("refused");
    assert!(err.contains("upper copper Layer to a lower one"), "{err}");
}

#[test]
fn a_rule_with_a_blocking_problem_still_opens_and_drc_reports_it() {
    let (_dir, path) = saved("load-rule-invalid");
    let mut track_rule = String::new();
    let mut engine = open_after(&path, |f| {
        let rules = f["rules"].as_array_mut().unwrap();
        let record = rules
            .iter_mut()
            .find(|r| r["kind"] == "track_width")
            .unwrap();
        track_rule = record["uuid"].as_str().unwrap().to_owned();
        // A minimum above the maximum, as a merge can leave it.
        record["values"] = json!({"min": 400_000, "preferred": 450_000, "max": 300_000});
    })
    .expect("opens");
    let run = engine.run_ok("drc.run", json!({}));
    let invalid: Vec<&Value> = run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == "rule_invalid")
        .collect();
    assert!(!invalid.is_empty(), "{run:#}");
    let v = invalid[0];
    assert_eq!(v["severity"], "error");
    assert_eq!(v["rule"], track_rule.as_str());
    assert_eq!(v["args"]["rule"], "track_width");
    assert_eq!(v["args"]["value"], "min");
    assert_eq!(v["args"]["problem"], "min_above_max");
    assert!(v.get("at").is_none() && v.get("document").is_none(), "{v}");
    let validate = engine.run_ok("pcb.rules.validate", json!({}));
    assert_eq!(validate["problems"][0]["rule"], track_rule.as_str());
    assert_eq!(validate["problems"][0]["problem"], "min_above_max");
    // The user can fix it.
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "track_width", "values": {"max": null}}),
    );
    assert_eq!(
        engine.run_ok("pcb.rules.validate", json!({}))["problems"],
        json!([])
    );
}

#[test]
fn a_rotation_stored_as_a_full_turn_is_normalised_on_load() {
    let dir = TempDir::new("load-rotation-turn");
    let mut engine = reda_testkit::probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "pads": [{"at": {"x": 0, "y": 0}, "width": 1_000_000, "height": 1_000_000}]}),
    );
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "A", "x": 0, "y": 0}),
    );
    engine.run_ok("project.save", json!({}));
    let path = dir.path().join("Demo/Demo.redapcb");
    let engine = open_after(&path, |f| {
        f["footprints"][0]["rotation"] = json!(360_000);
        f["texts"][0]["rotation"] = json!(360_000);
    })
    .expect("opens");
    let pcb = engine.project().unwrap().pcb().unwrap();
    let footprint = pcb.footprints().next().unwrap();
    assert_eq!(footprint.rotation.0, 0);
    assert_eq!(pcb.texts().next().unwrap().rotation.0, 0);
    // Normalising is not an edit: the Project opens clean.
    assert!(!engine.is_dirty());
}

#[test]
fn a_track_with_no_length_still_opens() {
    let (_dir, path) = saved("load-zero-track");
    let engine = open_after(&path, |f| {
        let layer = f["layers"]["layers"][layer_index(f, "copper", Some("top"))]["uuid"].clone();
        f["tracks"] = json!([{"uuid": "00000000-0000-4000-8000-0000000000f1", "layer": layer,
            "width": 200_000,
            "geometry": {"kind": "line", "from": {"x": 0, "y": 0}, "to": {"x": 0, "y": 0}}}]);
    })
    .expect("opens");
    assert_eq!(engine.project().unwrap().pcb().unwrap().tracks().count(), 1);
}
