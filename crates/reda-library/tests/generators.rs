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
    // toe 0.35, heel 0, side 0, F 0.10, P 0.05, rounded to the nearest 0.05 mm:
    // Z 2.470 → 2.45, G 0.670 → 0.65, X 0.970 → 0.95.
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
            ("1".to_owned(), -775_000, 0, 900_000, 950_000),
            ("2".to_owned(), 775_000, 0, 900_000, 950_000),
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
    // side 0.03, F 0.10, P 0.05, to the nearest 0.05 mm: Z 6.915 → 6.90, G 2.978 → 3.00,
    // X 0.599 → 0.60. G would run each Pad 0.50 mm under the body (E1 max 4.00), so the
    // heels are trimmed back to it: G 4.00, Pads 1.45 long at ±2.725.
    let (x, y1, y2) = (2_725_000, 1_905_000, 635_000);
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
            (Some(1_450_000), Some(600_000))
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
            ("1".to_owned(), -775_000, 900_000),
            ("2".to_owned(), 775_000, 900_000)
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
    assert_eq!(reda_library::generate::GENERATORS.len(), 11);
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
    // Z 3.729 → 3.75, G 2.009 → 2.00, X 0.264 → 0.25. Keeping
    // the exposed pad 0.20 mm clear of its 1.65 mm nominal width needs G 2.05, so the
    // heels are trimmed: Pads 0.85 long at ±1.45.
    let (x, w, h) = (1_450_000, 850_000, 250_000);
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
    expected.push(("11".into(), 0, 0, 1_650_000, 2_400_000));
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
    // pad 3.35–3.55 square: Z 5.729 → 5.75, G 4.009 → 4.00, X 0.264 → 0.25.
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
    // heel 0.35, side −0.02, F 0.10, P 0.05: Z 9.915 → 9.90, G 6.798 → 6.80,
    // X 0.280 → 0.30. The heels are trimmed back to the 7.10 mm body: G 7.10, Pads 1.40
    // long at ±4.25.
    let (c, end, long, short) = (4_250_000, 2_750_000, 1_400_000, 300_000);
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
    assert_eq!(out["name"], "DIP762W46P254L1016H533Q8B");
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
    assert_eq!(out["name"], "DIP1524W46P254L3556H533Q28B");
}

#[test]
fn headers_number_across_the_rows_and_sockets_share_the_land_pattern() {
    let dir = TempDir::new("gen-headers");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.header",
        json!({"library": LIBRARY, "rows": 2, "pins_per_row": 3, "pitch": "2.54"}),
    );
    assert_eq!(out["name"], "HDRV254W64P254R2L762T508H850B");
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
    assert_eq!(out["name"], "SKTV0W40P127R1L381T127H400B");
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
    assert_eq!(
        generator["params"]["dimensions"]["length"],
        json!({"min": 1_450_000, "max": 1_750_000}),
        "the standard size is written out as its dimensions"
    );
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
        [
            ("Designator", "assembly_top"),
            ("Designator", "silkscreen_top"),
            ("Value", "assembly_top")
        ]
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

/// The courtyard rectangle's upper-right corner.
fn courtyard_corner(file: &Value) -> (i64, i64) {
    let rects: Vec<_> = file["graphics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["layer"] == "courtyard_top")
        .map(|g| {
            let end = &g["shape"]["end"];
            (end["x"].as_i64().unwrap(), end["y"].as_i64().unwrap())
        })
        .collect();
    assert_eq!(rects.len(), 1, "one courtyard");
    rects[0]
}

/// The outer edge (Z/2), inner edge (G/2) and width (X) of a two-terminal Footprint's
/// right-hand Pad.
fn chip_edges(file: &Value) -> (i64, i64, i64) {
    let pads = pad_boxes(file);
    let (_, x, _, w, h) = pads.last().unwrap().clone();
    (x + w / 2, x - w / 2, h)
}

#[test]
fn a_0402_takes_the_small_chip_goals_with_no_heel_and_a_0_02_mm_round_off() {
    let dir = TempDir::new("gen-chip-0402");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.chip",
        json!({"library": LIBRARY, "size": "0402", "class": "capacitor"}),
    );
    assert_eq!(out["name"], "CAPC1005X40N");
    assert_eq!(out["violations"], json!([]));
    let file = read_json(Path::new(out["file"].as_str().unwrap()));
    // By hand, L 0.95–1.05, W 0.45–0.55, T 0.15–0.35 mm; toe 0.20, heel 0.00, side 0.00,
    // F 0.10, P 0.05, to the nearest 0.02 mm: Z 1.500 → 1.50, G 0.330 → 0.32 (0.35 on a
    // 0.05 mm grid, 0.40 with the old heel of −0.05), X 0.600 → 0.60.
    assert_eq!(
        pad_boxes(&file),
        [
            ("1".into(), -455_000, 0, 590_000, 600_000),
            ("2".into(), 455_000, 0, 590_000, 600_000),
        ]
    );
    // Small-chip courtyard excess 0.15: 0.75 + 0.15, 0.30 + 0.15.
    assert_eq!(courtyard_corner(&file), (900_000, 450_000));
}

#[test]
fn a_chip_is_small_when_either_its_length_or_its_width_is_under_1608() {
    let dir = TempDir::new("gen-chip-small-test");
    let mut e = engine_with_library(&dir);
    let chip = |e: &mut TestEngine, name: &str, length: [&str; 2], width: [&str; 2]| {
        let out = e.run_ok(
            "library.footprint.generate.chip",
            json!({"library": LIBRARY, "name": name, "dimensions": {
                "length": {"min": length[0], "max": length[1]},
                "width": {"min": width[0], "max": width[1]},
                "terminal": {"min": "0.25mm", "max": "0.35mm"},
                "height": "0.5mm"}}),
        );
        read_json(Path::new(out["file"].as_str().unwrap()))
    };
    // 1.70 × 0.70 mm: long enough, but too narrow. By hand, toe 0.20 and the 0.02 mm grid:
    // Z 2.200 → 2.20, G 0.980 → 0.98, X 0.800 → 0.80. The large-chip goals would give
    // Z 2.50 and G 1.00.
    let narrow = chip(&mut e, "Narrow", ["1.65mm", "1.75mm"], ["0.65mm", "0.75mm"]);
    assert_eq!(chip_edges(&narrow), (1_100_000, 490_000, 800_000));
    // Courtyard excess 0.15: 1.10 + 0.15 → 1.25, 0.40 + 0.15 → 0.55.
    assert_eq!(courtyard_corner(&narrow), (1_250_000, 550_000));
    // 1.54 × 0.86 mm: wide enough, but too short. Z 2.037 → 2.04, G 0.824 → 0.82,
    // X 0.964 → 0.96.
    let short = chip(&mut e, "Short", ["1.50mm", "1.58mm"], ["0.80mm", "0.92mm"]);
    assert_eq!(chip_edges(&short), (1_020_000, 410_000, 960_000));
    // 1.60 × 0.80 mm is not smaller than 1608: toe 0.35 on the 0.05 mm grid,
    // Z 2.470 → 2.45, G 0.816 → 0.80, X 0.970 → 0.95 (the small-chip goals would give
    // 2.18, 0.82 and 0.98).
    let full = chip(&mut e, "Full", ["1.45mm", "1.75mm"], ["0.65mm", "0.95mm"]);
    assert_eq!(chip_edges(&full), (1_225_000, 400_000, 950_000));
    // Courtyard excess 0.25: 1.225 + 0.25 → 1.50.
    assert_eq!(courtyard_corner(&full).0, 1_500_000);
}

#[test]
fn density_sets_the_fillet_goals_the_courtyard_and_the_last_letter_of_the_name() {
    let dir = TempDir::new("gen-density");
    let mut e = engine_with_library(&dir);
    let mut chip = |density: &str| {
        let out = e.run_ok(
            "library.footprint.generate.chip",
            json!({"library": LIBRARY, "size": "0603", "goals": {"density": density}}),
        );
        let file = read_json(Path::new(out["file"].as_str().unwrap()));
        (
            out["name"].as_str().unwrap().to_owned(),
            chip_edges(&file),
            courtyard_corner(&file),
            file["generator"]["params"]["goals"]["density"].clone(),
        )
    };
    // By hand from the 0603 body (L 1.45–1.75, W 0.65–0.95, T 0.10–0.50 mm), G 0.670 → 0.65
    // at every level since the heel goal is 0.00:
    // Most: toe 0.55, side 0.05, Z 2.870 → 2.85, X 1.070 → 1.05, courtyard excess 0.50.
    assert_eq!(
        chip("most"),
        (
            "RESC1608X55M".into(),
            (1_425_000, 325_000, 1_050_000),
            (1_950_000, 1_050_000),
            json!("most")
        )
    );
    // Least: toe 0.15, side −0.05, Z 2.070 → 2.05, X 0.870 → 0.85, excess 0.10 (around
    // the 0.95 mm body, wider than the Pads).
    assert_eq!(
        chip("least"),
        (
            "RESC1608X55L".into(),
            (1_025_000, 325_000, 850_000),
            (1_150_000, 600_000),
            json!("least")
        )
    );

    // A gull-wing part: SOIC-8 (E 5.80–6.20, L 0.40–1.27, b 0.31–0.51 mm), untrimmed so
    // the heel goal shows.
    let mut soic = |density: &str| {
        let out = e.run_ok(
            "library.footprint.generate.soic",
            json!({"library": LIBRARY, "pins": 8, "goals": {"density": density},
                   "pads": {"trim": false}}),
        );
        let file = read_json(Path::new(out["file"].as_str().unwrap()));
        let (_, x, _, w, h) = pad_boxes(&file)[0].clone();
        (out["name"].as_str().unwrap().to_owned(), -x, w, h)
    };
    // Most: toe 0.55, heel 0.45, side 0.05: Z 7.315 → 7.30, G 2.778 → 2.80, X 0.639 → 0.65.
    assert_eq!(
        soic("most"),
        ("SOIC127P600X175-8M".into(), 2_525_000, 2_250_000, 650_000)
    );
    // Least: toe 0.15, heel 0.25, side 0.01: Z 6.515 → 6.50, G 3.178 → 3.20, X 0.559 → 0.55.
    assert_eq!(
        soic("least"),
        ("SOIC127P600X175-8L".into(), 2_425_000, 1_650_000, 550_000)
    );

    // Every surface-mount family takes it; the default is Nominal.
    for (id, params, name) in [
        ("sot", json!({"package": "sot23_3"}), "SOT95P237X112-3M"),
        ("ssop", json!({"pins": 20}), "SOP65P780X200-20M"),
        ("tssop", json!({"pins": 14}), "SOP65P640X120-14M"),
        ("dfn", json!({"pins": 8}), "SON65P300X300X80-9M"),
        ("qfn", json!({"pins": 32}), "QFN50P500X500X100-33M"),
        ("qfp", json!({"pins": 48}), "QFP50P900X900X160-48M"),
    ] {
        let mut params = params;
        params["library"] = json!(LIBRARY);
        params["goals"] = json!({"density": "most"});
        let out = e.run_ok(&format!("library.footprint.generate.{id}"), params);
        assert_eq!(out["name"], name, "{id}");
        assert_eq!(out["violations"], json!([]), "{id}");
    }
    let out = e.run_ok(
        "library.footprint.generate.tssop",
        json!({"library": LIBRARY, "pins": 8}),
    );
    let file = read_json(Path::new(out["file"].as_str().unwrap()));
    assert_eq!(
        file["generator"]["params"]["goals"],
        json!({"density": "nominal", "fillet": {"toe": 350_000, "heel": 350_000, "side": 30_000}}),
        "the goals are written out at the density's values (table 3-2 at 0.65 mm pitch)"
    );
}

#[test]
fn through_hole_families_stay_at_level_b_and_take_no_density() {
    let dir = TempDir::new("gen-th-density");
    let mut e = engine_with_library(&dir);
    for (id, params) in [
        ("dip", json!({"pins": 8})),
        (
            "header",
            json!({"rows": 1, "pins_per_row": 2, "pitch": "2.54"}),
        ),
    ] {
        let mut params = params;
        params["library"] = json!(LIBRARY);
        let out = e.run_ok(&format!("library.footprint.generate.{id}"), params.clone());
        assert!(out["name"].as_str().unwrap().ends_with('B'), "{out}");
        params["goals"] = json!({"density": "most"});
        let err = e.run_err(&format!("library.footprint.generate.{id}"), params);
        assert_eq!(err.code, ErrorCode::InvalidParams, "{id}");
    }
}

#[test]
fn qfn_and_son_names_put_the_width_first_and_count_the_exposed_pad() {
    let dir = TempDir::new("gen-son-name");
    let mut e = engine_with_library(&dir);
    let out = e.run_ok(
        "library.footprint.generate.dfn",
        json!({"library": LIBRARY, "pins": 8, "dimensions": {
            "pitch": "0.5mm",
            "body_length": {"min": "2.9mm", "max": "3.1mm"},
            "body_width": {"min": "1.9mm", "max": "2.1mm"},
            "lead_width": {"min": "0.18mm", "max": "0.30mm"},
            "lead_length": {"min": "0.30mm", "max": "0.50mm"},
            "height": "0.8mm",
            "exposed_pad": {"length": {"min": "2.3mm", "max": "2.5mm"},
                "width": {"min": "0.4mm", "max": "0.5mm"}}}}),
    );
    // Width (E 2.00) before length (D 3.00), and 8 terminals plus the exposed pad.
    assert_eq!(out["name"], "SON50P200X300X80-9N");
}
