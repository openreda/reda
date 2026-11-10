//! A generated Footprint's stored `generator {command, params}` holds every value
//! resolved, and `library.footprint.regenerate` runs it again in place: it keeps the
//! Footprint's UUID and its Pads' UUIDs by number, and replaces hand edits, which its
//! preview lists.

use std::path::Path;

use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::{Value, json};

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";

/// A new Project in `dir/Demo` with an empty Project Library `dir/Demo/lib`.
fn engine_with_library(dir: &TempDir) -> TestEngine {
    let mut e = test_engine();
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let lib = dir.path().join("Demo/lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("Mine.redalib"),
        format!("{{\"format\": 1, \"library\": {{\"uuid\": \"{LIBRARY}\", \"name\": \"Mine\"}}}}"),
    )
    .unwrap();
    e.run_ok("project.library.add", json!({"path": "lib"}));
    e
}

/// Runs `command` with `params` (plus the Library) and returns its output and the file
/// it wrote.
fn generate(e: &mut TestEngine, command: &str, mut params: Value) -> (Value, Value) {
    params["library"] = json!(LIBRARY);
    let out = e.run_ok(command, params);
    let file =
        serde_json::from_slice(&std::fs::read(Path::new(out["file"].as_str().unwrap())).unwrap())
            .unwrap();
    (out, file)
}

/// The JSON pointers of every `null` in `value`.
fn nulls(value: &Value, at: &str, out: &mut Vec<String>) {
    match value {
        Value::Null => out.push(at.to_owned()),
        Value::Object(map) => {
            for (key, v) in map {
                nulls(v, &format!("{at}/{key}"), out);
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                nulls(v, &format!("{at}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// One run of every family, with the package options and overrides that change what is
/// resolved.
fn every_family() -> Vec<(&'static str, Value)> {
    let g = |family: &str| format!("library.footprint.generate.{family}");
    let exposed = json!({
        "length": {"min": "3.1mm", "max": "3.3mm"},
        "width": {"min": "2.3mm", "max": "2.5mm"},
    });
    vec![
        (g("chip").leak(), json!({"size": "0603"})),
        (
            g("chip").leak(),
            json!({"size": "0402", "goals": {"density": "most"}}),
        ),
        (g("molded").leak(), json!({"package": "sma"})),
        (g("sot").leak(), json!({"package": "sot223"})),
        (g("sot").leak(), json!({"package": "sot23_5"})),
        (g("soic").leak(), json!({"pins": 8})),
        (
            g("soic").leak(),
            json!({"pins": 8, "exposed_pad": exposed, "absent_pins": [2]}),
        ),
        (
            g("ssop").leak(),
            json!({"pins": 20, "pads": {"length": "2mm"}}),
        ),
        (
            g("tssop").leak(),
            json!({"pins": 14, "pads": {"trim": false}}),
        ),
        (g("dfn").leak(), json!({"pins": 8})),
        (
            g("qfn").leak(),
            json!({"pins": 32, "pin1_location": "top_centre"}),
        ),
        (g("qfn").leak(), json!({"pins": 16})),
        (
            g("qfp").leak(),
            json!({"pins": 48, "tolerances": {"courtyard_excess": "0.5mm"}}),
        ),
        (g("dip").leak(), json!({"pins": 8})),
        (
            g("header").leak(),
            json!({"rows": 2, "pins_per_row": 5, "pitch": "1.27"}),
        ),
    ]
}

#[test]
fn stored_params_hold_every_value_resolved() {
    let dir = TempDir::new("regen-resolved");
    let mut e = engine_with_library(&dir);
    for (i, (command, mut params)) in every_family().into_iter().enumerate() {
        params["name"] = json!(format!("F{i}"));
        let (_, file) = generate(&mut e, command, params.clone());
        let stored = &file["generator"]["params"];
        let mut found = vec![];
        nulls(stored, "", &mut found);
        assert_eq!(found, Vec::<String>::new(), "{command} {params}: {stored}");
    }

    // SOIC-8 by hand (see generators.rs): toe and heel 0.35, side 0.03, courtyard 0.25,
    // both grids 0.05; CL 0.40, CW 0.20, CS √(0.40² + 2 · 0.87²) = 1.2938. Before the
    // heels are trimmed to the body: Z 6.90, G 3.00, so Pads 1.95 long, 4.95 apart, and
    // X 0.60.
    let (_, file) = generate(
        &mut e,
        "library.footprint.generate.soic",
        json!({"pins": 8, "name": "S"}),
    );
    let p = &file["generator"]["params"];
    assert_eq!(
        p["goals"],
        json!({"density": "nominal", "fillet": {"toe": 350_000, "heel": 350_000, "side": 30_000}})
    );
    assert_eq!(p["tolerances"]["courtyard_excess"], 250_000);
    assert_eq!(
        p["tolerances"]["round_off"],
        json!({"size": 50_000, "place": 50_000})
    );
    assert_eq!(p["tolerances"]["component"]["span"], 400_000);
    assert_eq!(p["tolerances"]["component"]["width"], 200_000);
    let inner = p["tolerances"]["component"]["inner"].as_i64().unwrap();
    assert!((1_293_700..=1_293_800).contains(&inner), "{inner}");
    assert_eq!(p["pads"]["length"], 1_950_000);
    assert_eq!(p["pads"]["spacing"], 4_950_000);
    assert_eq!(p["pads"]["width"], 600_000);
    // The standard body is written out as its dimensions, and the name and description.
    assert_eq!(
        p["dimensions"]["lead_span"],
        json!({"min": 5_800_000, "max": 6_200_000})
    );
    assert_eq!(
        p["dimensions"]["body_width"],
        json!({"min": 3_800_000, "max": 4_000_000})
    );
    assert_eq!(p["name"], "S");
    assert_eq!(p["description"], "SOIC-8, 1.27 mm pitch, IPC-7351 Nominal");
    // A chip's size becomes its dimensions.
    let (_, chip) = generate(
        &mut e,
        "library.footprint.generate.chip",
        json!({"size": "0805"}),
    );
    let p = &chip["generator"]["params"];
    assert!(p.get("size").is_none());
    assert_eq!(
        p["dimensions"]["length"],
        json!({"min": 1_800_000, "max": 2_200_000})
    );
    assert_eq!(p["name"], "RESC2012X60N");
    // An exposed pad's paste array is resolved, and a small one's "inherit".
    let (_, qfn) = generate(
        &mut e,
        "library.footprint.generate.qfn",
        json!({"pins": 32}),
    );
    assert_eq!(
        qfn["generator"]["params"]["pads"]["paste_array"],
        json!({"columns": 3, "rows": 3})
    );
    let (_, small) = generate(
        &mut e,
        "library.footprint.generate.qfn",
        json!({"pins": 16}),
    );
    assert_eq!(
        small["generator"]["params"]["pads"]["paste_array"],
        "inherit"
    );
}

#[test]
fn the_stored_params_run_again_to_the_same_footprint_params_and_warnings() {
    for (i, (command, params)) in every_family().into_iter().enumerate() {
        let dir = TempDir::new(&format!("regen-again-{i}"));
        let mut e = engine_with_library(&dir);
        let (out, file) = generate(&mut e, command, params.clone());
        let dir2 = TempDir::new(&format!("regen-again-{i}-b"));
        let mut again = engine_with_library(&dir2);
        let (rerun_out, rerun) = generate(&mut again, command, file["generator"]["params"].clone());
        assert_eq!(rerun, file, "{command} {params}");
        assert_eq!(rerun_out["warnings"], out["warnings"], "{command} {params}");
    }
}

#[test]
fn a_non_compliant_override_warns_again_naming_only_itself() {
    let dir = TempDir::new("regen-warn");
    let mut e = engine_with_library(&dir);
    let command = "library.footprint.generate.ssop";
    let (out, file) = generate(
        &mut e,
        command,
        json!({"pins": 20, "pads": {"length": "2mm"}}),
    );
    let warning = |out: &Value| -> Vec<Value> {
        out["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|w| w["code"] == "not_ipc_compliant")
            .map(|w| w["params"].clone())
            .collect()
    };
    assert_eq!(warning(&out), [json!(["/pads/length"])]);
    let dir2 = TempDir::new("regen-warn-b");
    let mut again = engine_with_library(&dir2);
    let (out, _) = generate(&mut again, command, file["generator"]["params"].clone());
    assert_eq!(warning(&out), [json!(["/pads/length"])]);
}

/// A generated Footprint, opened for editing: its UUID and file.
fn generated_open(e: &mut TestEngine, family: &str, params: Value) -> (String, Value) {
    let (out, file) = generate(e, &format!("library.footprint.generate.{family}"), params);
    let uuid = out["footprint"].as_str().unwrap().to_owned();
    e.run_ok("library.element.open", json!({"element": uuid}));
    (uuid, file)
}

/// The open Footprint, saved, as its file reads.
fn saved(e: &mut TestEngine, uuid: &str) -> Value {
    let out = e.run_ok("library.element.save", json!({"element": uuid}));
    let file = out["written"][0].as_str().unwrap().to_owned();
    serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
}

/// Each Pad's number and UUID, in number order.
fn pad_uuids(file: &Value) -> Vec<(String, String)> {
    let mut pads: Vec<_> = file["pads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["number"].as_str().unwrap().to_owned(),
                p["uuid"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    pads.sort_by_key(|p| p.0.parse::<u32>().unwrap());
    pads
}

#[test]
fn regenerating_unchanged_params_gives_back_the_same_footprint() {
    let dir = TempDir::new("regen-same");
    let mut e = engine_with_library(&dir);
    let (uuid, file) = generated_open(&mut e, "soic", json!({"pins": 8}));
    let preview = e.run_ok(
        "library.footprint.regenerate.preview",
        json!({"footprint": uuid}),
    );
    assert_eq!(preview["lost"], json!([]));
    assert_eq!(preview["added"], json!([]));
    assert_eq!(preview["unchanged"], true);
    let out = e.run_ok("library.footprint.regenerate", json!({"footprint": uuid}));
    assert_eq!(out["result"]["lost"], json!([]));
    // Every UUID is kept, so the file, and the hash a placed Component compares, are
    // the same: regenerating to the same geometry doesn't read Library Differs.
    assert_eq!(saved(&mut e, &uuid), file);
}

#[test]
fn regenerating_keeps_uuids_by_number_and_replaces_hand_edits_after_a_preview() {
    let dir = TempDir::new("regen-edits");
    let mut e = engine_with_library(&dir);
    let (uuid, file) = generated_open(&mut e, "soic", json!({"pins": 8}));
    let before = pad_uuids(&file);
    let pad_1 = before[0].1.clone();
    // Hand edits: Pad 1 moved, a Silkscreen line added, the description changed.
    e.run_ok(
        "library.footprint.pad.edit",
        json!({"footprint": uuid, "pad": pad_1, "at": {"x": "-3mm", "y": "1.905mm"}}),
    );
    let added = e.run_ok(
        "library.footprint.graphic.add",
        json!({"footprint": uuid, "layer": "silkscreen_top", "shape": {
            "kind": "line", "start": {"x": "0mm", "y": "0mm"}, "end": {"x": "1mm", "y": "0mm"}}}),
    )["result"]["graphic"]
        .as_str()
        .unwrap()
        .to_owned();
    e.run_ok(
        "library.footprint.edit",
        json!({"footprint": uuid, "description": "mine"}),
    );
    // New params: the Most density, and pin 5 left out.
    let mut params = file["generator"]["params"].clone();
    let p = params.as_object_mut().unwrap();
    for section in ["goals", "tolerances", "pads"] {
        p.remove(section);
    }
    p.remove("description");
    params["goals"] = json!({"density": "most"});
    params["absent_pins"] = json!([5]);
    let request = json!({"footprint": uuid, "params": params});
    let preview = e.run_ok("library.footprint.regenerate.preview", request.clone());
    let lost: Vec<(String, String, String)> = preview["lost"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["kind"].as_str().unwrap().to_owned(),
                l["what"].as_str().unwrap().to_owned(),
                l["change"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    for expected in [
        ("pad", "Pad 1", "changed"),
        ("pad", "Pad 5", "removed"),
        ("graphic", "Line on silkscreen_top", "removed"),
        ("footprint", "description", "changed"),
    ] {
        let expected = (expected.0.into(), expected.1.into(), expected.2.into());
        assert!(lost.contains(&expected), "{expected:?} in {lost:?}");
    }
    assert_eq!(preview["unchanged"], false);
    // The preview changes nothing.
    let mut file_now = saved(&mut e, &uuid);
    assert_eq!(file_now["footprint"]["description"], "mine");

    let out = e.run_ok("library.footprint.regenerate", request);
    assert_eq!(out["result"]["lost"], preview["lost"]);
    file_now = saved(&mut e, &uuid);
    assert_eq!(
        file_now["footprint"]["uuid"], uuid,
        "the Footprint keeps its UUID"
    );
    assert_eq!(
        file_now["footprint"]["name"], "SOIC127P600X175-8N",
        "and its name"
    );
    let mut expected = before.clone();
    expected.remove(4);
    assert_eq!(
        pad_uuids(&file_now),
        expected,
        "Pads keep their UUIDs by number"
    );
    assert!(
        !file_now["graphics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["uuid"] == added),
        "the hand-drawn line is gone"
    );
    assert_eq!(
        file_now["footprint"]["description"],
        "SOIC-8, 1.27 mm pitch, IPC-7351 Most"
    );
    assert_eq!(file_now["generator"]["params"]["goals"]["density"], "most");
    assert_eq!(
        file_now["generator"]["params"]["name"],
        "SOIC127P600X175-8N"
    );
    // Most density: larger Pads than Nominal's 1.45 mm.
    let pad_1_now = file_now["pads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["number"] == "1")
        .unwrap();
    assert!(pad_1_now["width"].as_i64().unwrap() > 1_450_000);

    // One undo step brings the hand-edited Footprint back.
    e.run_ok("edit.undo", json!({"document": uuid}));
    let undone = saved(&mut e, &uuid);
    assert_eq!(undone["footprint"]["description"], "mine");
    assert_eq!(pad_uuids(&undone), before);
}

#[test]
fn only_a_generated_editable_footprint_regenerates() {
    let dir = TempDir::new("regen-refused");
    let mut e = engine_with_library(&dir);
    let made = e.run_ok(
        "library.element.new",
        json!({"library": LIBRARY, "kind": "footprint", "name": "Hand"}),
    );
    let hand = made["element"].as_str().unwrap();
    let err = e.run_err("library.footprint.regenerate", json!({"footprint": hand}));
    assert_eq!(err.code, reda_command::ErrorCode::Rejected, "{err:?}");
    let err = e.run_err(
        "library.footprint.regenerate",
        json!({"footprint": "00000000-0000-4000-8000-0000000000ff"}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::NotFound);
    // Params that don't work are refused with the family's pointer.
    let (uuid, _) = generated_open(&mut e, "soic", json!({"pins": 8}));
    let err = e.run_err(
        "library.footprint.regenerate",
        json!({"footprint": uuid, "params": {"pins": 9}}),
    );
    assert_eq!(err.details.unwrap()["pointer"], "/params/pins");
}
