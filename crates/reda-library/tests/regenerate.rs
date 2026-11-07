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
