//! `library.check`: the Library check run as a Command over the registered Libraries, its
//! Violations in the result, and that it never blocks saving.

use std::path::Path;

use reda_engine::command::ErrorCode;
use reda_engine::format::library::{write_footprint, write_library_info, write_part};
use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{
    ComponentType, Fields, Footprint, FootprintText, LibraryInfo, Pad, PadKind, PadShape, Part,
    PinMap, Uuid,
};
use reda_testkit::{TempDir, TestEngine, test_engine, test_engine_in};
use serde_json::json;

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";
const FOOTPRINT: &str = "000000f1-0000-4000-8000-000000000000";
const PART: &str = "000000a1-0000-4000-8000-000000000000";
const MISSING_SYMBOL: &str = "000000e1-0000-4000-8000-000000000000";

fn uuid(s: &str) -> Uuid {
    s.parse().unwrap()
}

/// A Library in `folder` holding a one-Pad Footprint with no courtyard, and a Part on it
/// whose Symbol does not exist.
fn write_flawed_library(folder: &Path) {
    for sub in ["footprints", "parts"] {
        std::fs::create_dir_all(folder.join(sub)).unwrap();
    }
    let info = LibraryInfo {
        uuid: uuid(LIBRARY),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(
        folder.join("Mine.redalib"),
        write_library_info(&info).unwrap(),
    )
    .unwrap();
    let footprint = Footprint {
        uuid: uuid(FOOTPRINT),
        name: "TP".into(),
        description: "Test point".into(),
        pads: vec![Pad {
            uuid: uuid("000000f2-0000-4000-8000-000000000000"),
            number: "1".into(),
            kind: PadKind::Smd,
            shape: PadShape::Circle,
            at: Point::new(Nm(0), Nm(0)),
            rotation: Millideg(0),
            width: Nm::from_mm(1),
            height: Nm::from_mm(1),
            corner_ratio: None,
            chamfer_ratio: None,
            corners: None,
            offset: None,
            hole: None,
            mask: None,
            paste: None,
            side: None,
            zone_connection: None,
        }],
        graphics: vec![],
        height: None,
        texts: vec![FootprintText::designator(uuid(
            "000000f3-0000-4000-8000-000000000000",
        ))],
        generator: None,
        provenance: None,
    };
    std::fs::write(
        folder.join("footprints/TP.redafp"),
        write_footprint(&footprint).unwrap(),
    )
    .unwrap();
    let part = Part {
        uuid: uuid(PART),
        name: "TestPoint".into(),
        component_type: ComponentType::Standard,
        in_bom: ComponentType::Standard.default_in_bom(),
        in_cpl: ComponentType::Standard.default_in_cpl(),
        symbol: Some(uuid(MISSING_SYMBOL)),
        footprints: vec![uuid(FOOTPRINT)],
        fields: Fields {
            description: "Test point".into(),
            ..Fields::default()
        },
        pin_map: PinMap::default(),
        provenance: None,
    };
    std::fs::write(
        folder.join("parts/TestPoint.redapart"),
        write_part(&part).unwrap(),
    )
    .unwrap();
}

/// A new Project in `dir/Demo` with the flawed Project Library `dir/Demo/lib`.
fn engine_with_library(dir: &TempDir) -> TestEngine {
    let mut e = test_engine();
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    write_flawed_library(&dir.path().join("Demo/lib"));
    e.run_ok("project.library.add", json!({"path": "lib"}));
    e
}

#[test]
fn checking_a_library_returns_its_violations() {
    let dir = TempDir::new("check-library");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok("library.check", json!({ "library": LIBRARY }));
    assert_eq!(
        out["violations"],
        json!([
            {"check": "missing_courtyard", "severity": "warning", "objects": [FOOTPRINT]},
            {"check": "broken_reference", "severity": "error",
             "args": {"kind": "symbol", "reference": MISSING_SYMBOL}, "objects": [PART]},
        ])
    );
    assert_eq!(out["problems"], json!([]));
}

#[test]
fn checking_one_element_returns_only_its_violations() {
    let dir = TempDir::new("check-element");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok("library.check", json!({ "element": PART }));
    let checks: Vec<_> = out["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["check"].as_str().unwrap())
        .collect();
    assert_eq!(checks, ["broken_reference"]);
}

#[test]
fn with_no_params_every_registered_library_is_checked() {
    let dir = TempDir::new("check-all");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok("library.check", json!({}));
    assert_eq!(out["violations"].as_array().unwrap().len(), 2);
}

#[test]
fn an_open_element_is_checked_as_edited_and_still_saves() {
    let dir = TempDir::new("check-open");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.element.new",
        json!({"library": LIBRARY, "kind": "symbol", "name": "Odd"}),
    );
    let symbol = out["element"].as_str().unwrap().to_owned();
    let pin = e.run_ok(
        "library.symbol.pin.add",
        json!({"symbol": symbol, "name": "A", "number": "1", "at": {"x": "10mil", "y": 0}}),
    )["result"]["pin"]
        .as_str()
        .unwrap()
        .to_owned();

    let out = e.run_ok("library.check", json!({ "element": symbol }));
    assert_eq!(
        out["violations"],
        json!([{"check": "missing_description", "severity": "warning", "objects": [symbol]},
               {"check": "off_grid_pin", "severity": "warning",
                "args": {"pin": "1", "grid": 1_270_000}, "objects": [symbol, pin]}])
    );

    e.run_ok("library.element.save", json!({ "element": symbol }));
    assert!(dir.path().join("Demo/lib/symbols/Odd.redasym").exists());
    let out = e.run_ok("library.check", json!({ "library": LIBRARY }));
    let checks: Vec<_> = out["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["check"].as_str().unwrap())
        .collect();
    assert_eq!(
        checks,
        [
            "missing_description",
            "off_grid_pin",
            "missing_courtyard",
            "broken_reference"
        ]
    );
}

#[test]
fn an_unknown_element_or_library_is_not_found() {
    let dir = TempDir::new("check-unknown");
    let mut e = engine_with_library(&dir);
    let nobody = "000000bb-0000-4000-8000-000000000000";
    let err = e.run_err("library.check", json!({ "element": nobody }));
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = e.run_err("library.check", json!({ "library": nobody }));
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn an_element_and_a_library_together_are_refused() {
    let dir = TempDir::new("check-both");
    let mut e = engine_with_library(&dir);
    let err = e.run_err(
        "library.check",
        json!({"element": PART, "library": LIBRARY}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn paste_windows_outside_copper_are_warnings_and_never_block_saving() {
    let dir = TempDir::new("paste-window-check");
    let mut e = engine_with_library(&dir);
    let fp = e.run_ok(
        "library.element.new",
        json!({"library":LIBRARY,"kind":"footprint","name":"Windows"}),
    )["element"]
        .clone();
    e.run_ok(
        "library.footprint.pad.add",
        json!({"footprint":fp,"number":"1","kind":"smd","shape":"circle",
        "at":{"x":0,"y":0},"width":"2mm","height":"2mm","paste":{"windows":[
            {"at":{"x":0,"y":0},"width":"1mm","height":"1mm"},
            {"at":{"x":"0.8mm","y":0},"width":"1mm","height":"1mm"},
            {"at":{"x":0,"y":0},"width":"2mm","height":"2mm","corner_ratio":100000}]}}),
    );
    let out = e.run_ok("library.check", json!({"element":fp}));
    let found: Vec<_> = out["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == "paste_window_outside_copper")
        .collect();
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(found[0]["severity"], "warning");
    assert_eq!(found[0]["args"]["window"], 1);
    e.run_ok("library.element.save", json!({"element":fp}));
}

/// The flawed Library as a Project Library, with per-user folders under `dir/home`.
fn engine_with_user(dir: &TempDir) -> TestEngine {
    let mut e = test_engine_in(&dir.path().join("home"));
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    write_flawed_library(&dir.path().join("Demo/lib"));
    e.run_ok("project.library.add", json!({"path": "lib"}));
    e
}

fn set_severity(e: &mut TestEngine, check: &str, severity: &str) {
    e.run_ok(
        "app.preferences.set",
        json!({"key": format!("libraries.severities.{check}"), "value": severity}),
    );
}

#[test]
fn severities_come_from_the_users_preferences() {
    let dir = TempDir::new("check-severities");
    let mut e = engine_with_user(&dir);
    set_severity(&mut e, "missing_courtyard", "error");
    set_severity(&mut e, "broken_reference", "off");
    let out = e.run_ok("library.check", json!({ "library": LIBRARY }));
    assert_eq!(
        out["violations"],
        json!([{"check": "missing_courtyard", "severity": "error", "objects": [FOOTPRINT]}])
    );

    // Reset, they are the defaults again.
    e.run_ok(
        "app.preferences.reset",
        json!({"keys": ["libraries.severities.missing_courtyard",
                        "libraries.severities.broken_reference"]}),
    );
    let out = e.run_ok("library.check", json!({ "library": LIBRARY }));
    let severities: Vec<_> = out["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["check"].as_str().unwrap(),
                v["severity"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        severities,
        [
            ("missing_courtyard", "warning"),
            ("broken_reference", "error")
        ]
    );
}

#[test]
fn every_check_is_a_preference_starting_at_the_specs_default() {
    let dir = TempDir::new("check-severity-keys");
    let mut e = engine_with_user(&dir);
    let out = e.run_ok("app.preferences.get", json!({}));
    let value = &out["values"]["libraries"]["severities"];
    assert_eq!(value["duplicate_pin_number"], "error");
    assert_eq!(value["missing_pad_number"], "error");
    assert_eq!(value["origin_off_centroid"], "warning");
    assert_eq!(value["generator_out_of_date"], "warning");
    assert_eq!(value["missing_pin_name"], "off");
    assert_eq!(value.as_object().unwrap().len(), 23, "{value}");
    let err = e.run_err(
        "app.preferences.set",
        json!({"key": "libraries.severities.missing_pin_name", "value": "info"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams, "no Info level");
}

/// Generates a chip Footprint into the flawed Library; returns the generator's output.
fn generate_chip(e: &mut TestEngine, extra: serde_json::Value) -> serde_json::Value {
    let mut params = json!({"library": LIBRARY, "size": "0603"});
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    e.run_ok("library.footprint.generate.chip", params)
}

#[test]
fn a_generator_checks_what_it_wrote_with_the_users_severities() {
    let dir = TempDir::new("check-generate");
    let mut e = engine_with_user(&dir);
    let out = generate_chip(&mut e, json!({"outputs": {"courtyard": false}}));
    assert_eq!(
        out["violations"],
        json!([{"check": "missing_courtyard", "severity": "warning", "objects": [out["footprint"]]}])
    );
    set_severity(&mut e, "missing_courtyard", "off");
    let out = generate_chip(
        &mut e,
        json!({"name": "Other", "outputs": {"courtyard": false}}),
    );
    assert_eq!(out["violations"], json!([]));
}

#[test]
fn a_hand_edited_generated_footprint_is_out_of_date_and_regenerating_fixes_it() {
    let dir = TempDir::new("check-out-of-date");
    let mut e = engine_with_user(&dir);
    let out = generate_chip(&mut e, json!({}));
    assert_eq!(out["violations"], json!([]));
    let footprint = out["footprint"].clone();
    e.run_ok("library.element.open", json!({"element": footprint}));
    let opened = e.run_ok("library.check", json!({"element": footprint}));
    assert_eq!(opened["violations"], json!([]));

    let file: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out["file"].as_str().unwrap()).unwrap()).unwrap();
    let pad = file["pads"][0]["uuid"].clone();
    e.run_ok(
        "library.footprint.pad.edit",
        json!({"footprint": footprint, "pad": pad, "width": "2mm"}),
    );
    let edited = e.run_ok("library.check", json!({"element": footprint}));
    assert_eq!(
        edited["violations"],
        json!([{"check": "generator_out_of_date", "severity": "warning",
                "args": {"generator": "library.footprint.generate.chip"},
                "objects": [footprint]}])
    );
    // It warns and never blocks: the edit saves.
    e.run_ok("library.element.save", json!({"element": footprint}));

    e.run_ok(
        "library.footprint.regenerate",
        json!({"footprint": footprint}),
    );
    let again = e.run_ok("library.check", json!({"element": footprint}));
    assert_eq!(again["violations"], json!([]));
}
