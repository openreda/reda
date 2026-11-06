//! Footprint generators: Commands that compute IPC-7351 Nominal Footprints and write
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
fn a_0603_chip_resistor_gets_ipc_7351_nominal_pads() {
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
    let mut pad_specs: Vec<_> = pads
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
    pad_specs.sort();
    assert_eq!(
        pad_specs,
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
    ("library.footprint.generate.dfn", r#"{"pins": 8}"#),
    ("library.footprint.generate.qfn", r#"{"pins": 16}"#),
    ("library.footprint.generate.qfn", r#"{"pins": 32}"#),
    ("library.footprint.generate.qfp", r#"{"pins": 44}"#),
    ("library.footprint.generate.qfp", r#"{"pins": 64}"#),
    ("library.footprint.generate.dip", r#"{"pins": 8}"#),
    (
        "library.footprint.generate.dip",
        r#"{"pins": 28, "wide": true}"#,
    ),
    (
        "library.footprint.generate.header",
        r#"{"rows": 1, "pins_per_row": 4, "pitch": "2.54"}"#,
    ),
    (
        "library.footprint.generate.header",
        r#"{"rows": 2, "pins_per_row": 5, "pitch": "2.54"}"#,
    ),
    (
        "library.footprint.generate.header",
        r#"{"rows": 2, "pins_per_row": 5, "pitch": "1.27", "socket": true}"#,
    ),
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
        (
            "library.footprint.generate.qfn",
            json!({"pins": 30}),
            "/pins",
        ),
        (
            "library.footprint.generate.qfn",
            json!({"pins": 36}),
            "/pins",
        ),
        (
            "library.footprint.generate.dfn",
            json!({"pins": 7}),
            "/pins",
        ),
        (
            "library.footprint.generate.qfp",
            json!({"pins": 50}),
            "/pins",
        ),
        (
            "library.footprint.generate.dip",
            json!({"pins": 9}),
            "/pins",
        ),
        (
            "library.footprint.generate.header",
            json!({"rows": 3, "pins_per_row": 4, "pitch": "2.54"}),
            "/rows",
        ),
        (
            "library.footprint.generate.qfn",
            json!({"pins": 40, "dimensions": {
                "pitch": "0.5mm",
                "body_length": {"min": "2.9mm", "max": "3.1mm"},
                "body_width": {"min": "2.9mm", "max": "3.1mm"},
                "lead_width": {"min": "0.18mm", "max": "0.3mm"},
                "lead_length": {"min": "0.3mm", "max": "0.5mm"},
                "height": "1mm"}}),
            "/pins",
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

#[test]
fn a_preview_computes_the_footprint_a_generator_would_write_without_writing_it() {
    let chip = "library.footprint.generate.chip";
    let params = json!({"library": LIBRARY, "size": "0603"});
    let footprint = reda_library::generate::preview(chip, &params)
        .expect("the chip generator has a preview")
        .unwrap();
    assert_eq!(footprint.name, "RESC1608X55N");
    let mut pads: Vec<_> = footprint
        .pads
        .iter()
        .map(|p| (p.number.clone(), p.at.x.0, p.width.0))
        .collect();
    pads.sort();
    // The same Pads as the written Footprint above.
    assert_eq!(
        pads,
        [
            ("1".to_owned(), -787_500, 925_000),
            ("2".to_owned(), 787_500, 925_000)
        ]
    );
    // A name the caller gives replaces the IPC-7351 one; bad params say which one.
    let named = json!({"library": LIBRARY, "size": "0603", "name": "R0603"});
    let preview = reda_library::generate::preview(chip, &named);
    assert_eq!(preview.unwrap().unwrap().name, "R0603");
    let bad = json!({"library": LIBRARY, "rows": 3, "pins_per_row": 4, "pitch": "2.54"});
    let error = reda_library::generate::preview("library.footprint.generate.header", &bad)
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidParams);
    // Every generator has one; other Commands have none.
    for id in reda_library::generate::GENERATORS {
        assert!(
            reda_library::generate::preview(id, &json!({})).is_some(),
            "{id}"
        );
    }
    assert_eq!(reda_library::generate::GENERATORS.len(), 10);
    assert!(reda_library::generate::preview("library.check", &json!({})).is_none());
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
    // Z 3.35 and G 0.35, which would put 0.525 mm of each Pad under a 0.70 mm half-body;
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
        (
            "library.footprint.generate.qfp",
            json!({"pins": 32}),
            7_100_000,
        ),
        (
            "library.footprint.generate.qfp",
            json!({"pins": 100}),
            14_100_000,
        ),
    ];
    for (id, mut params, body) in cases {
        params["library"] = json!(LIBRARY);
        let out = e.run_ok(id, params);
        let file = read_json(Path::new(out["file"].as_str().unwrap()));
        for (number, x, y, w, h) in pad_boxes(&file) {
            // Measured towards the body's centre from the side the Pad is on.
            let reach = if x.abs() >= y.abs() {
                body / 2 - (x.abs() - w / 2)
            } else {
                body / 2 - (y.abs() - h / 2)
            };
            assert!(
                reach <= body / 4,
                "{}: spec {number} reaches {reach} nm under the body",
                out["name"]
            );
        }
    }
}

#[test]
fn a_dfn_10_gets_no_lead_lands_and_an_exposed_pad() {
    let dir = TempDir::new("gen-dfn-10");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.dfn",
        json!({"library": LIBRARY, "pins": 10}),
    );
    assert_eq!(out["name"], "SON50P300X300X80-11N");
    assert_eq!(out["violations"], json!([]));
    let file = read_json(Path::new(out["file"].as_str().unwrap()));
    // By hand, 3 × 3 mm body: D and E 2.90–3.10, L 0.30–0.50, b 0.18–0.30 mm, exposed
    // pad 2.30–2.50 × 1.55–1.75; no-lead toe 0.30, heel 0, side −0.04:
    // Z 3.708 → 3.75, G 2.022 → 2.00, X 0.232 → 0.25. The exposed pad keeps 0.2 mm from
    // the Pads, so it narrows from 1.65 to 1.60.
    let (x, w, h) = (1_437_500, 875_000, 250_000);
    let mut expected: Vec<(String, i64, i64, i64, i64)> = vec![];
    for (i, y) in [1_000_000, 500_000, 0, -500_000, -1_000_000]
        .iter()
        .enumerate()
    {
        expected.push(((i + 1).to_string(), -x, *y, w, h));
    }
    for (i, y) in [-1_000_000, -500_000, 0, 500_000, 1_000_000]
        .iter()
        .enumerate()
    {
        expected.push(((i + 6).to_string(), x, *y, w, h));
    }
    expected.push(("11".into(), 0, 0, 1_600_000, 2_400_000));
    assert_eq!(pad_boxes(&file), expected);
}

#[test]
fn a_qfn_32_counts_counter_clockwise_around_four_sides_and_an_exposed_pad() {
    let dir = TempDir::new("gen-qfn-32");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.qfn",
        json!({"library": LIBRARY, "pins": 32}),
    );
    assert_eq!(out["name"], "QFN50P500X500X100-33N");
    assert_eq!(out["violations"], json!([]));
    let file = read_json(Path::new(out["file"].as_str().unwrap()));
    // By hand, 5 × 5 mm body: D and E 4.90–5.10, L 0.30–0.50, b 0.18–0.30 mm, exposed
    // pad 3.35–3.55 square: Z 5.708 → 5.75, G 4.022 → 4.00, X 0.232 → 0.25.
    let (c, end, long, short) = (2_437_500, 1_750_000, 875_000, 250_000);
    let pads = pad_boxes(&file);
    assert_eq!(pads.len(), 33);
    let pick: Vec<_> = [0, 7, 8, 15, 16, 23, 24, 31, 32]
        .iter()
        .map(|&i| pads[i].clone())
        .collect();
    assert_eq!(
        pick,
        [
            ("1".into(), -c, end, long, short),
            ("8".into(), -c, -end, long, short),
            ("9".into(), -end, -c, short, long),
            ("16".into(), end, -c, short, long),
            ("17".into(), c, -end, long, short),
            ("24".into(), c, end, long, short),
            ("25".into(), end, c, short, long),
            ("32".into(), -end, c, short, long),
            ("33".into(), 0, 0, 3_450_000, 3_450_000),
        ]
    );
}

#[test]
fn an_lqfp_48_gets_gull_wing_lands_on_four_sides() {
    let dir = TempDir::new("gen-qfp-48");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.qfp",
        json!({"library": LIBRARY, "pins": 48}),
    );
    assert_eq!(out["name"], "QFP50P900X900X160-48N");
    assert_eq!(out["violations"], json!([]));
    let file = read_json(Path::new(out["file"].as_str().unwrap()));
    // By hand, MS-026 7 × 7 mm: E 8.80–9.20, L 0.45–0.75, b 0.17–0.27 mm; toe 0.35,
    // heel 0.35, side −0.02: Z 9.904 → 9.95, G 6.806 → 6.80, X 0.245 → 0.25.
    let (c, end, long, short) = (4_187_500, 2_750_000, 1_575_000, 250_000);
    let pads = pad_boxes(&file);
    assert_eq!(pads.len(), 48);
    let pick: Vec<_> = [0, 11, 12, 23, 24, 35, 36, 47]
        .iter()
        .map(|&i| pads[i].clone())
        .collect();
    assert_eq!(
        pick,
        [
            ("1".into(), -c, end, long, short),
            ("12".into(), -c, -end, long, short),
            ("13".into(), -end, -c, short, long),
            ("24".into(), end, -c, short, long),
            ("25".into(), c, -end, long, short),
            ("36".into(), c, end, long, short),
            ("37".into(), end, c, short, long),
            ("48".into(), -end, c, short, long),
        ]
    );
}

/// Each through-hole Pad's number, centre, diameter, shape and hole, sorted by number.
fn holes(file: &Value) -> Vec<(String, i64, i64, i64, String, i64)> {
    let mut pads: Vec<_> = file["pads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            assert_eq!(p["kind"], "plated_through_hole");
            assert_eq!(p["width"], p["height"]);
            assert_eq!(p["hole"]["width"], p["hole"]["height"]);
            (
                p["number"].as_str().unwrap().to_owned(),
                p["at"]["x"].as_i64().unwrap(),
                p["at"]["y"].as_i64().unwrap(),
                p["width"].as_i64().unwrap(),
                p["shape"].as_str().unwrap().to_owned(),
                p["hole"]["width"].as_i64().unwrap(),
            )
        })
        .collect();
    pads.sort_by_key(|(n, ..)| n.parse::<u32>().unwrap());
    pads
}

#[test]
fn a_dip_8_gets_through_hole_pads_with_a_square_pin_1() {
    let dir = TempDir::new("gen-dip-8");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.dip",
        json!({"library": LIBRARY, "pins": 8}),
    );
    assert_eq!(out["name"], "DIP254P762X533-8");
    assert_eq!(out["violations"], json!([]));
    let file = read_json(Path::new(out["file"].as_str().unwrap()));
    // MS-001 leads 0.36–0.56 × 0.20–0.36 mm: diagonal 0.666, hole + 0.20 → 0.90,
    // Pad + 0.50 → 1.40.
    let (x, y1, y2, d, h) = (3_810_000, 3_810_000, 1_270_000, 1_400_000, 900_000);
    let pad = |n: &str, x: i64, y: i64, shape: &str| (n.to_owned(), x, y, d, shape.to_owned(), h);
    assert_eq!(
        holes(&file),
        [
            pad("1", -x, y1, "rect"),
            pad("2", -x, y2, "circle"),
            pad("3", -x, -y2, "circle"),
            pad("4", -x, -y1, "circle"),
            pad("5", x, -y1, "circle"),
            pad("6", x, -y2, "circle"),
            pad("7", x, y2, "circle"),
            pad("8", x, y1, "circle"),
        ]
    );
    let out = e.run_ok(
        "library.footprint.generate.dip",
        json!({"library": LIBRARY, "pins": 28, "wide": true}),
    );
    assert_eq!(out["name"], "DIP254P1524X533-28");
}

#[test]
fn headers_number_across_the_rows_and_sockets_share_the_land_pattern() {
    let dir = TempDir::new("gen-headers");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.header",
        json!({"library": LIBRARY, "rows": 2, "pins_per_row": 3, "pitch": "2.54"}),
    );
    assert_eq!(out["name"], "HDR254P2X3");
    assert_eq!(out["violations"], json!([]));
    let file = read_json(Path::new(out["file"].as_str().unwrap()));
    // 0.64 mm square posts: diagonal 0.905, hole + 0.20 → 1.15, Pad + 0.50 → 1.65.
    let (x, y, d, h) = (1_270_000, 2_540_000, 1_650_000, 1_150_000);
    let pad = |n: &str, x: i64, y: i64, shape: &str| (n.to_owned(), x, y, d, shape.to_owned(), h);
    assert_eq!(
        holes(&file),
        [
            pad("1", -x, y, "rect"),
            pad("2", x, y, "circle"),
            pad("3", -x, 0, "circle"),
            pad("4", x, 0, "circle"),
            pad("5", -x, -y, "circle"),
            pad("6", x, -y, "circle"),
        ]
    );

    let out = e.run_ok(
        "library.footprint.generate.header",
        json!({"library": LIBRARY, "rows": 1, "pins_per_row": 3, "pitch": "1.27",
            "socket": true}),
    );
    assert_eq!(out["name"], "SKT127P1X3");
    assert_eq!(out["violations"], json!([]));
    let file = read_json(Path::new(out["file"].as_str().unwrap()));
    // 0.40 mm square posts: diagonal 0.566, hole + 0.20 → 0.80; the Pad would be 1.30
    // but keeps 0.25 mm between neighbours at 1.27 mm pitch: 1.00.
    let (y, d, h) = (1_270_000, 1_000_000, 800_000);
    let pad = |n: &str, y: i64, shape: &str| (n.to_owned(), 0, y, d, shape.to_owned(), h);
    assert_eq!(
        holes(&file),
        [
            pad("1", y, "rect"),
            pad("2", 0, "circle"),
            pad("3", -y, "circle"),
        ]
    );
}

/// A generated Footprint records the Command and its params, with every default filled
/// in, so it can be regenerated; and it carries its height and field texts.
#[test]
fn a_generated_footprint_records_its_generator_height_and_texts() {
    let dir = TempDir::new("gen-record");
    let mut e = engine_with_library(&dir);
    e.run_ok(
        "library.footprint.generate.chip",
        json!({"library": LIBRARY, "size": "0603"}),
    );
    let file = read_json(&footprint_file(&dir, "RESC1608X55N"));
    let generator = &file["generator"];
    assert_eq!(generator["command"], "library.footprint.generate.chip");
    assert_eq!(generator["params"]["library"], LIBRARY);
    assert_eq!(generator["params"]["size"], "0603");
    assert_eq!(
        generator["params"]["class"], "resistor",
        "the default class is written out"
    );
    assert_eq!(file["footprint"]["height"], 550_000, "the 0603's 0.55 mm");
    let texts = file["texts"].as_array().unwrap();
    let mut fields: Vec<(&str, &str)> = texts
        .iter()
        .map(|t| (t["field"].as_str().unwrap(), t["layer"].as_str().unwrap()))
        .collect();
    fields.sort();
    assert_eq!(
        fields,
        [("Designator", "silkscreen_top"), ("Value", "assembly_top")]
    );

    // Running the recorded params again gives the same Footprint.
    let dir2 = TempDir::new("gen-record-again");
    let mut again = engine_with_library(&dir2);
    let mut params = generator["params"].clone();
    params["name"] = json!("Again");
    again.run_ok("library.footprint.generate.chip", params);
    let rerun = read_json(&footprint_file(&dir2, "Again"));
    assert_eq!(rerun["pads"].as_array().unwrap().len(), 2);
    let geometry = |f: &Value| {
        f["pads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["at"].clone(), p["width"].clone(), p["height"].clone()))
            .collect::<Vec<_>>()
    };
    let mut a = geometry(&file);
    let mut b = geometry(&rerun);
    a.sort_by_key(|p| p.0.to_string());
    b.sort_by_key(|p| p.0.to_string());
    assert_eq!(a, b);
}
