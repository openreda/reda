//! Footprint generators: Commands that compute IPC-7351 Nominal land patterns and write
//! the Footprint into a chosen Library, then run the Library check on it.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, assert_golden, test_engine};
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

fn footprint_file(dir: &TempDir, name: &str) -> PathBuf {
    dir.path()
        .join(format!("Demo/lib/footprints/{name}.redafp"))
}

/// The footprint file's JSON, re-read from disk.
fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/generators/{name}.redafp"))
}

#[test]
fn a_0603_chip_resistor_gets_ipc_7351_nominal_lands() {
    let dir = TempDir::new("gen-chip-0603");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.chip",
        json!({"library": LIBRARY, "size": "0603"}),
    );
    assert_eq!(out["name"], "RESC1608X55N");
    assert_eq!(out["library"], LIBRARY);
    assert_eq!(out["violations"], json!([]));

    let file = read_json(&footprint_file(&dir, "RESC1608X55N"));
    assert_eq!(file["footprint"]["uuid"], out["footprint"]);
    let pads = file["pads"].as_array().unwrap();
    // Worked by hand from IPC-7351B with L 1.45–1.75, W 0.65–0.95, T 0.10–0.50 mm,
    // toe 0.35, heel 0, side 0, F 0.05, P 0.025, rounded to 0.05 mm:
    // Z 2.455 → 2.50, G 0.678 → 0.65, X 0.955 → 1.00.
    let mut lands: Vec<_> = pads
        .iter()
        .map(|p| {
            (
                p["number"].as_str().unwrap().to_owned(),
                p["at"]["x"].as_i64().unwrap(),
                p["at"]["y"].as_i64().unwrap(),
                p["width"].as_i64().unwrap(),
                p["height"].as_i64().unwrap(),
            )
        })
        .collect();
    lands.sort();
    assert_eq!(
        lands,
        [
            ("1".to_owned(), -787_500, 0, 925_000, 1_000_000),
            ("2".to_owned(), 787_500, 0, 925_000, 1_000_000),
        ]
    );
    let courtyard: Vec<_> = file["graphics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["layer"] == "courtyard_top")
        .map(|g| g["shape"].clone())
        .collect();
    assert_eq!(
        courtyard,
        [
            json!({"kind": "rect", "start": {"x": -1_500_000, "y": -750_000},
                "end": {"x": 1_500_000, "y": 750_000}})
        ]
    );
}

/// Each Pad's number and centre, sorted by number.
fn pad_centres(file: &Value) -> Vec<(String, i64, i64)> {
    let mut pads: Vec<_> = file["pads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["number"].as_str().unwrap().to_owned(),
                p["at"]["x"].as_i64().unwrap(),
                p["at"]["y"].as_i64().unwrap(),
            )
        })
        .collect();
    pads.sort_by_key(|(n, _, _)| n.parse::<u32>().unwrap());
    pads
}

#[test]
fn a_soic_8_has_pin_1_at_the_upper_left_and_counts_counter_clockwise() {
    let dir = TempDir::new("gen-soic-8");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.soic",
        json!({"library": LIBRARY, "pins": 8}),
    );
    assert_eq!(out["name"], "SOIC127P600X175-8N");
    let file = read_json(&footprint_file(&dir, "SOIC127P600X175-8N"));
    // By hand, MS-012: E 5.80–6.20, L 0.40–1.27, b 0.31–0.51 mm; toe 0.35, heel 0.35,
    // side 0.03: Z 6.904 → 6.95, G 2.98 → 2.95, X 0.578 → 0.60.
    let (x, y1, y2) = (2_475_000, 1_905_000, 635_000);
    assert_eq!(
        pad_centres(&file),
        [
            ("1".into(), -x, y1),
            ("2".into(), -x, y2),
            ("3".into(), -x, -y2),
            ("4".into(), -x, -y1),
            ("5".into(), x, -y1),
            ("6".into(), x, -y2),
            ("7".into(), x, y2),
            ("8".into(), x, y1),
        ]
    );
    for pad in file["pads"].as_array().unwrap() {
        assert_eq!(
            (pad["width"].as_i64(), pad["height"].as_i64()),
            (Some(2_000_000), Some(600_000))
        );
    }
}

#[test]
fn sot_23_variants_number_their_leads_by_the_standard() {
    let dir = TempDir::new("gen-sot");
    let mut e = engine_with_library(&dir);
    let mut columns = vec![];
    for package in ["sot23_3", "sot23_5", "sot23_6", "sot223"] {
        let out = e.run_ok(
            "library.footprint.generate.sot",
            json!({"library": LIBRARY, "package": package}),
        );
        let file = read_json(Path::new(out["file"].as_str().unwrap()));
        // Each lead as its column (L/R) and row (+1 top, 0 middle, -1 bottom).
        let pitch = if package == "sot223" {
            2_300_000
        } else {
            950_000
        };
        let leads: Vec<String> = pad_centres(&file)
            .into_iter()
            .map(|(n, x, y)| format!("{n}{}{}", if x < 0 { 'L' } else { 'R' }, y / pitch))
            .collect();
        columns.push((out["name"].as_str().unwrap().to_owned(), leads.join(" ")));
    }
    assert_eq!(
        columns,
        [
            ("SOT95P237X112-3N".into(), "1L1 2L-1 3R0".into()),
            ("SOT95P280X145-5N".into(), "1L1 2L0 3L-1 4R-1 5R1".into()),
            (
                "SOT95P280X145-6N".into(),
                "1L1 2L0 3L-1 4R-1 5R0 6R1".into()
            ),
            ("SOT230P700X180-4N".into(), "1L1 2L0 3L-1 4R0".into()),
        ]
    );
}

/// One representative run of every family and the most common sizes.
const REPRESENTATIVE: &[(&str, &str)] = &[
    ("library.footprint.generate.chip", r#"{"size": "0201"}"#),
    (
        "library.footprint.generate.chip",
        r#"{"size": "0402", "class": "capacitor"}"#,
    ),
    ("library.footprint.generate.chip", r#"{"size": "0805"}"#),
    ("library.footprint.generate.chip", r#"{"size": "2512"}"#),
    (
        "library.footprint.generate.sot",
        r#"{"package": "sot23_3"}"#,
    ),
    (
        "library.footprint.generate.sot",
        r#"{"package": "sot23_5"}"#,
    ),
    ("library.footprint.generate.sot", r#"{"package": "sot223"}"#),
    ("library.footprint.generate.soic", r#"{"pins": 8}"#),
    (
        "library.footprint.generate.soic",
        r#"{"pins": 16, "wide": true}"#,
    ),
    ("library.footprint.generate.ssop", r#"{"pins": 20}"#),
    ("library.footprint.generate.tssop", r#"{"pins": 14}"#),
];

/// The file with its UUIDs renumbered `…001`, `…002`, … in order of first appearance, so
/// the goldens do not change when creating a Project takes more or fewer UUIDs.
fn renumber_uuids(bytes: &[u8]) -> String {
    let text = std::str::from_utf8(bytes).unwrap();
    let mut seen: Vec<String> = vec![];
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("\"00000000-0000-4000-8000-") {
        let (before, from) = rest.split_at(at + 1);
        let uuid = &from[..36];
        let n = match seen.iter().position(|u| u == uuid) {
            Some(i) => i + 1,
            None => {
                seen.push(uuid.to_owned());
                seen.len()
            }
        };
        out.push_str(before);
        out.push_str(&format!("00000000-0000-4000-8000-{n:012x}"));
        rest = &from[36..];
    }
    out.push_str(rest);
    out
}

#[test]
fn representative_footprints_match_their_goldens_and_pass_the_library_check() {
    let dir = TempDir::new("gen-goldens");
    let mut e = engine_with_library(&dir);
    for (id, params) in REPRESENTATIVE {
        let mut params: Value = serde_json::from_str(params).unwrap();
        params["library"] = json!(LIBRARY);
        let out = e.run_ok(id, params);
        assert_eq!(out["violations"], json!([]), "{id} {out}");
        let name = out["name"].as_str().unwrap();
        let bytes = std::fs::read(out["file"].as_str().unwrap()).unwrap();
        assert_golden(golden(name), renumber_uuids(&bytes).as_bytes());

        let file: Value = serde_json::from_slice(&bytes).unwrap();
        let layers: Vec<&str> = file["graphics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["layer"].as_str().unwrap())
            .collect();
        assert!(layers.contains(&"courtyard_top"), "{name}: {layers:?}");
        assert!(layers.contains(&"assembly_top"), "{name}: {layers:?}");
    }
}

#[test]
fn the_check_finds_the_new_footprint_in_the_library() {
    let dir = TempDir::new("gen-check");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.tssop",
        json!({"library": LIBRARY, "pins": 8, "name": "My TSSOP"}),
    );
    assert_eq!(out["name"], "My TSSOP");
    assert!(footprint_file(&dir, "My TSSOP").exists());
    let check = e.run_ok("library.check", json!({"element": out["footprint"]}));
    assert_eq!(check["violations"], json!([]));
}

#[test]
fn custom_dimensions_replace_the_standard_body() {
    let dir = TempDir::new("gen-custom");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.chip",
        json!({"library": LIBRARY, "class": "inductor", "dimensions": {
            "length": {"min": "1.45mm", "max": "1.75mm"},
            "width": {"min": "0.65mm", "max": "0.95mm"},
            "terminal": {"min": "0.1mm", "max": "0.5mm"},
            "height": "0.55mm"}}),
    );
    assert_eq!(out["name"], "INDC1608X55N");
    let out = e.run_ok(
        "library.footprint.generate.ssop",
        json!({"library": LIBRARY, "pins": 10, "dimensions": {
            "pitch": "0.5mm",
            "body_length": {"min": "2.9mm", "max": "3.1mm"},
            "body_width": {"min": "2.9mm", "max": "3.1mm"},
            "lead_span": {"min": "4.75mm", "max": "5.05mm"},
            "lead_width": {"min": "0.17mm", "max": "0.27mm"},
            "lead_length": {"min": "0.4mm", "max": "0.7mm"},
            "height": "1.1mm"}}),
    );
    assert_eq!(out["name"], "SOP50P490X110-10N");
    assert_eq!(out["violations"], json!([]));
}

#[test]
fn unusable_params_are_refused_with_a_pointer() {
    let dir = TempDir::new("gen-invalid");
    let mut e = engine_with_library(&dir);
    let cases = [
        ("library.footprint.generate.chip", json!({}), "/size"),
        (
            "library.footprint.generate.soic",
            json!({"pins": 12}),
            "/pins",
        ),
        (
            "library.footprint.generate.tssop",
            json!({"pins": 7}),
            "/pins",
        ),
        (
            "library.footprint.generate.chip",
            json!({"dimensions": {
                "length": {"min": "1mm", "max": "1.2mm"},
                "width": {"min": "0.5mm", "max": "0.4mm"},
                "terminal": {"min": "0.2mm", "max": "0.3mm"},
                "height": "0.4mm"}}),
            "/dimensions/width/max",
        ),
        (
            "library.footprint.generate.chip",
            json!({"size": "0603", "name": "a/b"}),
            "/name",
        ),
    ];
    for (id, mut params, pointer) in cases {
        params["library"] = json!(LIBRARY);
        let err = e.run_err(id, params);
        assert_eq!(err.code, ErrorCode::InvalidParams, "{id}: {}", err.message);
        assert_eq!(
            err.details.unwrap()["pointer"],
            pointer,
            "{id}: {}",
            err.message
        );
    }
}

#[test]
fn an_existing_file_or_an_unknown_library_is_refused() {
    let dir = TempDir::new("gen-refused");
    let mut e = engine_with_library(&dir);
    let params = json!({"library": LIBRARY, "size": "0603"});
    e.run_ok("library.footprint.generate.chip", params.clone());
    let err = e.run_err("library.footprint.generate.chip", params);
    assert_eq!(err.code, ErrorCode::Rejected);

    let nowhere = "000000bb-0000-4000-8000-000000000000";
    let err = e.run_err(
        "library.footprint.generate.chip",
        json!({"library": nowhere, "size": "0603"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

/// Each Pad's number, centre and size, sorted by number.
fn pad_boxes(file: &Value) -> Vec<(String, i64, i64, i64, i64)> {
    let mut pads: Vec<_> = file["pads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["number"].as_str().unwrap().to_owned(),
                p["at"]["x"].as_i64().unwrap(),
                p["at"]["y"].as_i64().unwrap(),
                p["width"].as_i64().unwrap(),
                p["height"].as_i64().unwrap(),
            )
        })
        .collect();
    pads.sort_by_key(|(n, ..)| n.parse::<u32>().unwrap_or(u32::MAX));
    pads
}

#[test]
fn sot_23_3_lands_stop_short_of_the_middle_of_the_body() {
    let dir = TempDir::new("gen-sot23-3");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.sot",
        json!({"library": LIBRARY, "package": "sot23_3"}),
    );
    let file = read_json(Path::new(out["file"].as_str().unwrap()));
    // TO-236: E 2.10–2.64, L 0.40–0.60, b 0.30–0.50, E1 1.20–1.40 mm. IPC-7351 gives
    // Z 3.35 and G 0.35, which would put 0.525 mm of each land under a 0.70 mm half-body;
    // the heel edge is held back to a quarter of E1 max from the body edge: G 0.70.
    let (x, y, w, h) = (1_012_500, 950_000, 1_325_000, 600_000);
    assert_eq!(
        pad_boxes(&file),
        [
            ("1".into(), -x, y, w, h),
            ("2".into(), -x, -y, w, h),
            ("3".into(), x, 0, w, h),
        ]
    );
}

#[test]
fn gull_wing_lands_reach_under_the_body_by_at_most_a_quarter_of_its_width() {
    let dir = TempDir::new("gen-under-body");
    let mut e = engine_with_library(&dir);
    // Each package with its greatest body width between the lead rows (E1).
    let cases = [
        (
            "library.footprint.generate.sot",
            json!({"package": "sot23_3"}),
            1_400_000,
        ),
        (
            "library.footprint.generate.sot",
            json!({"package": "sot23_5"}),
            1_700_000,
        ),
        (
            "library.footprint.generate.sot",
            json!({"package": "sot223"}),
            3_700_000,
        ),
        (
            "library.footprint.generate.soic",
            json!({"pins": 8}),
            4_000_000,
        ),
        (
            "library.footprint.generate.ssop",
            json!({"pins": 20}),
            5_600_000,
        ),
        (
            "library.footprint.generate.tssop",
            json!({"pins": 14}),
            4_500_000,
        ),
    ];
    for (id, mut params, body) in cases {
        params["library"] = json!(LIBRARY);
        let out = e.run_ok(id, params);
        let file = read_json(Path::new(out["file"].as_str().unwrap()));
        for (number, x, _, w, _) in pad_boxes(&file) {
            let reach = body / 2 - (x.abs() - w / 2);
            assert!(
                reach <= body / 4,
                "{}: land {number} reaches {reach} nm under the body",
                out["name"]
            );
        }
    }
}
