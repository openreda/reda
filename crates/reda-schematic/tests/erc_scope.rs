//! Scoped ERC runs: `erc.run {documents?, objects?, checks?}` and `partial`.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const R_PIN_1: &str = "a0000000-0000-4000-8000-000000000012";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erc_library/Erc")
}

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

fn place_on(engine: &mut TestEngine, sheet: &str, x: i64, y: i64) -> String {
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": sheet, "part": R_10K, "at": at(x, y)}),
    );
    out["result"]["unit"].as_str().unwrap().to_owned()
}

/// The `unconnected_pin` Violations of a run, as their objects.
fn unconnected(out: &Value) -> Vec<Value> {
    out["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == "unconnected_pin")
        .map(|v| v["objects"].clone())
        .collect()
}

/// A Project with an R on its first sheet and another on a second sheet.
fn two_sheets(dir: &TempDir) -> (TestEngine, String, String, String) {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let second = engine.run_ok("document.add", json!({"kind": "schematic"}))["result"]["uuid"]
        .as_str()
        .unwrap()
        .to_owned();
    let r1 = place_on(&mut engine, SHEET, 1000, 1000);
    let r2 = place_on(&mut engine, &second, 1000, 1000);
    (engine, second, r1, r2)
}

#[test]
fn a_run_scoped_to_documents_or_objects_keeps_violations_touching_them_and_is_partial() {
    let dir = TempDir::new("erc-scope");
    let (mut engine, second, r1, r2) = two_sheets(&dir);
    let full = engine.run_ok("erc.run", json!({}));
    assert_eq!(full["partial"], false);
    assert_eq!(unconnected(&full).len(), 4, "{full}");

    let on_second = engine.run_ok("erc.run", json!({"documents": [second]}));
    assert_eq!(on_second["partial"], true);
    let found = unconnected(&on_second);
    assert_eq!(found.len(), 2, "{on_second}");
    assert!(found.iter().all(|o| o[0] == r2.as_str()), "{on_second}");
    assert!(
        on_second["violations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["document"] == second.as_str()),
        "{on_second}"
    );

    let of_r1 = engine.run_ok("erc.run", json!({"objects": [r1]}));
    assert_eq!(of_r1["partial"], true);
    let found = unconnected(&of_r1);
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|o| o[0] == r1.as_str()));

    // Both given: a Violation must be in both.
    let none = engine.run_ok("erc.run", json!({"documents": [second], "objects": [r1]}));
    assert!(none["violations"].as_array().unwrap().is_empty(), "{none}");
}

#[test]
fn a_check_subset_runs_only_those_checks_over_the_whole_design() {
    let dir = TempDir::new("erc-subset");
    let (mut engine, ..) = two_sheets(&dir);
    let out = engine.run_ok("erc.run", json!({"checks": ["unconnected_pin"]}));
    let violations = out["violations"].as_array().unwrap();
    assert_eq!(violations.len(), 4, "{out}");
    assert!(violations.iter().all(|v| v["check"] == "unconnected_pin"));
    let other = engine.run_ok("erc.run", json!({"checks": ["duplicate_designator"]}));
    assert!(
        other["violations"].as_array().unwrap().is_empty(),
        "{other}"
    );
}

#[test]
fn a_scoped_run_never_marks_waivers_stale() {
    let dir = TempDir::new("erc-scope-waivers");
    let (mut engine, second, r1, _) = two_sheets(&dir);
    let waiver = engine.run_ok(
        "erc.waive",
        json!({"violations": [{"check": "unconnected_pin", "objects": [r1, R_PIN_1]}]}),
    )["result"]["waivers"][0]
        .clone();
    engine.run_ok("schematic.delete", json!({"objects": [r1]}));
    assert_eq!(
        engine.run_ok("erc.run", json!({}))["unmatched_waivers"],
        json!([waiver])
    );
    for params in [
        json!({"documents": [SHEET]}),
        json!({"documents": [second]}),
        json!({"objects": [r1]}),
    ] {
        let out = engine.run_ok("erc.run", params.clone());
        assert_eq!(out["partial"], true, "{params}");
        assert_eq!(out["unmatched_waivers"], json!([]), "{params}");
    }
    // A check subset still checks the whole design, so its own Waivers can be stale.
    let subset = engine.run_ok("erc.run", json!({"checks": ["unconnected_pin"]}));
    assert_eq!(subset["unmatched_waivers"], json!([waiver]));
    let other = engine.run_ok("erc.run", json!({"checks": ["duplicate_designator"]}));
    assert_eq!(other["unmatched_waivers"], json!([]));
}

#[test]
fn a_scope_naming_no_schematic_sheet_is_refused() {
    let dir = TempDir::new("erc-scope-bad");
    let (mut engine, ..) = two_sheets(&dir);
    let err = engine.run_err(
        "erc.run",
        json!({"documents": ["00000000-0000-4000-8000-0000000000ff"]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/documents/0");
}
