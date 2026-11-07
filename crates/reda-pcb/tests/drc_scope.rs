//! Scoped DRC runs: `drc.run {objects?, area?, layers?, checks?}` and `partial`.

mod common;

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine};
use serde_json::{Value, json};

/// Adds a 0.1 mm Track on `layer` from (0, `y`) to (5 mm, `y`), too narrow for the
/// board's `track_width`, and returns its UUID.
fn narrow_track(engine: &mut TestEngine, layer: &str, y: &str) -> String {
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": layer, "width": "0.1mm", "points": [{"x": 0, "y": y}, {"x": "5mm", "y": y}]}),
    );
    out["result"]["tracks"][0].as_str().unwrap().to_owned()
}

/// Three narrow Tracks: two on the top copper (y 0 and 10 mm), one on the bottom (20 mm).
fn three_tracks(dir: &TempDir) -> (TestEngine, [String; 3]) {
    let mut engine = common::engine(dir);
    let a = narrow_track(&mut engine, "copper_top", "0mm");
    let b = narrow_track(&mut engine, "copper_top", "10mm");
    let c = narrow_track(&mut engine, "copper_bottom", "20mm");
    (engine, [a, b, c])
}

/// The Tracks a run's `track_width` Violations name.
fn too_narrow(out: &Value) -> Vec<String> {
    out["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == "track_width")
        .map(|v| v["objects"][0].as_str().unwrap().to_owned())
        .collect()
}

/// Whether every Violation of `out` involves `track`.
fn all_involve(out: &Value, track: &str) -> bool {
    out["violations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["objects"].as_array().unwrap().iter().any(|o| o == track))
}

#[test]
fn a_full_run_and_a_check_subset_are_not_partial() {
    let dir = TempDir::new("drc-scope-full");
    let (mut engine, tracks) = three_tracks(&dir);
    let full = engine.run_ok("drc.run", json!({}));
    assert_eq!(full["partial"], false);
    assert_eq!(too_narrow(&full), tracks.to_vec());
    let subset = engine.run_ok("drc.run", json!({"checks": ["track_width"]}));
    assert_eq!(subset["partial"], false);
    assert_eq!(too_narrow(&subset), tracks.to_vec());
}

#[test]
fn a_run_scoped_to_objects_lists_only_violations_involving_them() {
    let dir = TempDir::new("drc-scope-objects");
    let (mut engine, [a, ..]) = three_tracks(&dir);
    let out = engine.run_ok("drc.run", json!({"objects": [a]}));
    assert_eq!(out["partial"], true);
    assert_eq!(too_narrow(&out), vec![a.clone()]);
    assert!(all_involve(&out, &a), "{out}");
}

#[test]
fn a_run_scoped_to_layers_lists_only_violations_on_them() {
    let dir = TempDir::new("drc-scope-layers");
    let (mut engine, [_, _, c]) = three_tracks(&dir);
    let out = engine.run_ok("drc.run", json!({"layers": ["copper_bottom"]}));
    assert_eq!(out["partial"], true);
    assert_eq!(too_narrow(&out), vec![c]);
    let bottom = common::pcb(&engine)
        .copper_layers()
        .last()
        .unwrap()
        .uuid
        .to_string();
    for v in out["violations"].as_array().unwrap() {
        assert!(
            v["layers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|l| l == bottom.as_str()),
            "{v}"
        );
    }
    let err = engine.run_err("drc.run", json!({"layers": ["copper_l9"]}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/layers/0");
}

#[test]
fn a_run_scoped_to_an_area_lists_only_violations_with_an_object_in_it() {
    let dir = TempDir::new("drc-scope-area");
    let (mut engine, [a, b, _]) = three_tracks(&dir);
    // Corners in either order; the box only reaches the first Track.
    let area = json!({"from": {"x": "6mm", "y": "1mm"}, "to": {"x": "-1mm", "y": "-1mm"}});
    let out = engine.run_ok("drc.run", json!({"area": area}));
    assert_eq!(out["partial"], true);
    assert_eq!(too_narrow(&out), vec![a.clone()]);
    assert!(all_involve(&out, &a), "{out}");
    // A box touching the second Track's copper edge (y 10 mm - 0.05 mm) takes it in.
    let edge = json!({"from": {"x": "1mm", "y": "5mm"}, "to": {"x": "2mm", "y": "9.95mm"}});
    assert_eq!(
        too_narrow(&engine.run_ok("drc.run", json!({"area": edge}))),
        vec![b]
    );
    // Scopes combine: a Violation must be in each.
    let both = engine.run_ok(
        "drc.run",
        json!({"area": area, "layers": ["copper_bottom"]}),
    );
    assert!(both["violations"].as_array().unwrap().is_empty(), "{both}");
}

#[test]
fn a_scoped_run_never_marks_waivers_stale() {
    let dir = TempDir::new("drc-scope-waivers");
    let (mut engine, [a, b, _]) = three_tracks(&dir);
    let waiver = engine.run_ok(
        "drc.waive",
        json!({"violations": [{"check": "track_width", "objects": [b]}]}),
    )["result"]["waivers"][0]
        .clone();
    engine.run_ok("pcb.track.delete", json!({"tracks": [b]}));
    assert_eq!(
        engine.run_ok("drc.run", json!({}))["unmatched_waivers"],
        json!([waiver])
    );
    for params in [
        json!({"objects": [a]}),
        json!({"objects": [b]}),
        json!({"layers": ["copper_top"]}),
        json!({"area": {"from": {"x": "-50mm", "y": "-50mm"}, "to": {"x": "50mm", "y": "50mm"}}}),
        json!({"objects": [b], "checks": ["track_width"]}),
    ] {
        let out = engine.run_ok("drc.run", params.clone());
        assert_eq!(out["partial"], true, "{params}");
        assert_eq!(out["unmatched_waivers"], json!([]), "{params}");
    }
    // A check subset still checks the whole board, so its own Waivers can be stale.
    let subset = engine.run_ok("drc.run", json!({"checks": ["track_width"]}));
    assert_eq!(subset["unmatched_waivers"], json!([waiver]));
}
