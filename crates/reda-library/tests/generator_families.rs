//! The newer generator families and package options: molded 2-terminal parts, an
//! optional exposed pad on SOIC, SSOP and TSSOP, `absent_pins`, `pin1_location`, a
//! chamfered body corner and a polarity mark.

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

/// Runs generator `family` with `params` (plus the Library) and returns its output and
/// the file it wrote.
fn generate(e: &mut TestEngine, family: &str, mut params: Value) -> (Value, Value) {
    params["library"] = json!(LIBRARY);
    let out = e.run_ok(&format!("library.footprint.generate.{family}"), params);
    let file =
        serde_json::from_slice(&std::fs::read(Path::new(out["file"].as_str().unwrap())).unwrap())
            .unwrap();
    (out, file)
}

/// Each Pad as `(number, x, y, width, height)`, in number order.
fn pads(file: &Value) -> Vec<(String, i64, i64, i64, i64)> {
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
    pads.sort_by_key(|p| p.0.parse::<u32>().unwrap());
    pads
}

fn numbers(file: &Value) -> Vec<String> {
    pads(file).into_iter().map(|p| p.0).collect()
}

/// The graphics on `layer`.
fn graphics<'a>(file: &'a Value, layer: &str) -> Vec<&'a Value> {
    file["graphics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["layer"] == layer)
        .collect()
}

fn courtyard_corner(file: &Value) -> (i64, i64) {
    let rects = graphics(file, "courtyard_top");
    assert_eq!(rects.len(), 1, "one courtyard");
    let end = &rects[0]["shape"]["end"];
    (end["x"].as_i64().unwrap(), end["y"].as_i64().unwrap())
}

#[test]
fn a_sod_123_takes_the_molded_body_goals() {
    let dir = TempDir::new("gen-molded-sod123");
    let mut e = engine_with_library(&dir);
    let (out, file) = generate(&mut e, "molded", json!({"package": "sod123"}));
    // By hand: span 3.55–3.85, terminal 0.25–0.45, width 0.50–0.70 mm; IPC-7351B table
    // 3-13 Nominal: toe 0.15, heel 0.50, side -0.05; F 0.10, P 0.05, nearest 0.05 mm.
    // Z = 3.55 + 0.30 + √(0.30² + F² + P²) = 4.170 → 4.15.
    // S 2.65–3.35, RMS 0.412, Smax 3.206; G = 3.206 − 1.00 − √(0.412² + F² + P²) =
    // 1.779 → 1.80. X = 0.50 − 0.10 + √(0.20² + F² + P²) = 0.629 → 0.65.
    // Pads 1.175 long at ±1.4875.
    assert_eq!(out["name"], "SOD3716X135N");
    assert_eq!(
        pads(&file),
        [
            ("1".into(), -1_487_500, 0, 1_175_000, 650_000),
            ("2".into(), 1_487_500, 0, 1_175_000, 650_000),
        ]
    );
    // Pads reach 2.075 mm; + 0.25 excess, out to 0.05 mm: 2.35. Body 1.80 / 2 + 0.25.
    assert_eq!(courtyard_corner(&file), (2_350_000, 1_150_000));
    assert_eq!(file["footprint"]["height"], 1_350_000);
    assert_eq!(
        file["footprint"]["description"],
        "SOD-123 molded diode, IPC-7351 Nominal"
    );
}

#[test]
fn smx_bodies_are_named_as_molded_diodes() {
    let dir = TempDir::new("gen-molded-smx");
    let mut e = engine_with_library(&dir);
    for (package, name) in [
        ("sod323", "SOD2513X110N"),
        ("sma", "DIOM5226X244N"),
        ("smb", "DIOM5436X244N"),
        ("smc", "DIOM7959X244N"),
    ] {
        let (out, file) = generate(&mut e, "molded", json!({"package": package}));
        assert_eq!(out["name"], name, "{package}");
        assert_eq!(numbers(&file), ["1", "2"]);
        assert_eq!(out["violations"], json!([]), "{package}");
    }
    // A datasheet's dimensions; the class sets the prefix.
    let out = e.run_ok(
        "library.footprint.generate.molded",
        json!({"library": LIBRARY, "class": "capacitor", "dimensions": {
            "span": {"min": "3.0mm", "max": "3.4mm"},
            "body_width": {"min": "1.4mm", "max": "1.8mm"},
            "body_length": {"min": "2.0mm", "max": "2.4mm"},
            "terminal_width": {"min": "1.1mm", "max": "1.3mm"},
            "terminal_length": {"min": "0.6mm", "max": "1.0mm"},
            "height": "1.6mm",
        }}),
    );
    assert_eq!(out["name"], "CAPM3216X160N");
}

/// The vertical Silkscreen lines, as `(x, y top)`.
fn vertical_silkscreen(file: &Value) -> Vec<(i64, i64)> {
    graphics(file, "silkscreen_top")
        .into_iter()
        .filter_map(|g| {
            let (s, e) = (&g["shape"]["start"], &g["shape"]["end"]);
            (g["shape"]["kind"] == "line" && s["x"] == e["x"]).then(|| {
                (
                    s["x"].as_i64().unwrap(),
                    s["y"].as_i64().unwrap().max(e["y"].as_i64().unwrap()),
                )
            })
        })
        .collect()
}

#[test]
fn a_polarity_mark_bars_terminal_1_beyond_its_pad() {
    let dir = TempDir::new("gen-polarity");
    let mut e = engine_with_library(&dir);
    // SOD-123: Pad 1's outer edge at -2.075; 0.20 clearance and half the 0.12 line:
    // the bar at -2.335, as tall as the body's lines (1.80 / 2 + 0.06).
    let (_, sod) = generate(&mut e, "molded", json!({"package": "sod123"}));
    assert_eq!(vertical_silkscreen(&sod), [(-2_335_000, 960_000)]);
    let (_, plain) = generate(
        &mut e,
        "molded",
        json!({"package": "sod123", "polarity_mark": false, "name": "Plain"}),
    );
    assert_eq!(vertical_silkscreen(&plain), []);
    // A chip has none unless asked: 0603 Pads 0.90 × 0.95 at ±0.775, so the bar is at
    // -(1.225 + 0.26), reaching past the Pad (0.475 + 0.06).
    let (_, chip) = generate(&mut e, "chip", json!({"size": "0603"}));
    assert_eq!(vertical_silkscreen(&chip), []);
    let (_, marked) = generate(
        &mut e,
        "chip",
        json!({"size": "0603", "class": "capacitor", "polarity_mark": true}),
    );
    assert_eq!(vertical_silkscreen(&marked), [(-1_485_000, 535_000)]);
}

/// The Assembly outline's shape kind.
fn assembly_outline(file: &Value) -> String {
    let outlines: Vec<_> = graphics(file, "assembly_top")
        .into_iter()
        .filter(|g| g["shape"]["kind"] != "text")
        .collect();
    assert_eq!(outlines.len(), 1);
    outlines[0]["shape"]["kind"].as_str().unwrap().to_owned()
}

#[test]
fn the_body_corner_at_pin_1_is_chamfered_where_asked() {
    let dir = TempDir::new("gen-chamfer");
    let mut e = engine_with_library(&dir);
    let (_, soic) = generate(&mut e, "soic", json!({"pins": 8}));
    assert_eq!(assembly_outline(&soic), "polygon", "ICs chamfer by default");
    let (_, square) = generate(
        &mut e,
        "soic",
        json!({"pins": 8, "chamfer": false, "name": "Square"}),
    );
    assert_eq!(assembly_outline(&square), "rect");
    let (_, sod) = generate(&mut e, "molded", json!({"package": "sod123"}));
    assert_eq!(
        assembly_outline(&sod),
        "rect",
        "molded bodies don't by default"
    );
    let (_, chamfered) = generate(
        &mut e,
        "molded",
        json!({"package": "sod123", "chamfer": true, "name": "Chamfered"}),
    );
    assert_eq!(assembly_outline(&chamfered), "polygon");
    for family in [
        ("sot", json!({"package": "sot23_3"})),
        ("ssop", json!({"pins": 20})),
        ("tssop", json!({"pins": 14})),
        ("dfn", json!({"pins": 8})),
        ("qfn", json!({"pins": 32})),
        ("qfp", json!({"pins": 48})),
        ("dip", json!({"pins": 8})),
    ] {
        let mut params = family.1.clone();
        params["chamfer"] = json!(false);
        let (_, file) = generate(&mut e, family.0, params);
        assert_eq!(assembly_outline(&file), "rect", "{}", family.0);
    }
}

#[test]
fn soic_ssop_and_tssop_take_an_optional_exposed_pad() {
    let dir = TempDir::new("gen-so-exposed");
    let mut e = engine_with_library(&dir);
    let exposed = json!({
        "length": {"min": "3.1mm", "max": "3.3mm"},
        "width": {"min": "2.3mm", "max": "2.5mm"},
    });
    let (out, file) = generate(
        &mut e,
        "soic",
        json!({"pins": 8, "exposed_pad": exposed.clone()}),
    );
    // The exposed pad counts in the name's pin count, as on a QFN.
    assert_eq!(out["name"], "SOIC127P600X175-9N");
    let boxes = pads(&file);
    assert_eq!(boxes.len(), 9);
    assert_eq!(boxes[8], ("9".into(), 0, 0, 2_400_000, 3_200_000));
    // The lead Pads are as without it: their heels (at the body, 2.00 mm out) keep well
    // over 0.20 mm from the exposed pad's edge at 1.20.
    assert_eq!(
        boxes[0],
        ("1".into(), -2_725_000, 1_905_000, 1_450_000, 600_000)
    );
    let nine = file["pads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["number"] == "9")
        .unwrap();
    assert_eq!(nine["zone_connection"], json!({"style": "solid"}));
    // 2.40 × 3.20 is over 2.1 × 1.6: 2 × 3 paste windows.
    assert_eq!(nine["paste"]["windows"].as_array().unwrap().len(), 6);
    for family in ["ssop", "tssop"] {
        let (out, file) = generate(
            &mut e,
            family,
            json!({"pins": 20, "exposed_pad": exposed.clone()}),
        );
        assert_eq!(numbers(&file).len(), 21, "{family}");
        assert!(out["name"].as_str().unwrap().ends_with("-21N"), "{family}");
    }
    // A pad that reaches the lead Pads trims them, with a warning.
    let (out, file) = generate(
        &mut e,
        "tssop",
        json!({"pins": 14, "name": "Wide EP", "exposed_pad": {
            "length": {"min": "4.0mm", "max": "4.0mm"},
            "width": {"min": "4.4mm", "max": "4.4mm"},
        }}),
    );
    let trims: Vec<_> = out["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "trimmed" && w["message"].as_str().unwrap().contains("exposed"))
        .collect();
    assert_eq!(trims.len(), 1);
    let (_, x, _, w, _) = pads(&file)[0].clone();
    assert_eq!(
        x + w / 2,
        -2_400_000,
        "the heel keeps 0.20 mm from the 2.20 mm edge"
    );
    // Without one, an exposed pad override is refused as before.
    let mut params = json!({"pins": 8, "pads": {"exposed_pad": {"width": "2mm", "length": "2mm"}}});
    params["library"] = json!(LIBRARY);
    let err = e.run_err("library.footprint.generate.soic", params);
    assert_eq!(err.details.unwrap()["pointer"], "/pads/exposed_pad");
}

#[test]
fn absent_pins_leave_their_positions_empty_and_keep_the_other_numbers() {
    let dir = TempDir::new("gen-absent");
    let mut e = engine_with_library(&dir);
    let (out, soic) = generate(&mut e, "soic", json!({"pins": 8, "absent_pins": [5]}));
    assert_eq!(numbers(&soic), ["1", "2", "3", "4", "6", "7", "8"]);
    assert_eq!(
        out["name"], "SOIC127P600X175-7N",
        "the name counts the Pads"
    );
    let (out, qfn) = generate(&mut e, "qfn", json!({"pins": 16, "absent_pins": [16, 1]}));
    let mut expected: Vec<String> = (2..=15).map(|n| n.to_string()).collect();
    expected.push("17".into());
    assert_eq!(numbers(&qfn), expected, "the exposed pad keeps its number");
    assert!(out["name"].as_str().unwrap().ends_with("-15N"));
    let (out, dip) = generate(&mut e, "dip", json!({"pins": 8, "absent_pins": [2]}));
    assert_eq!(numbers(&dip).len(), 7);
    assert!(out["name"].as_str().unwrap().ends_with("Q7B"));
    let (_, header) = generate(
        &mut e,
        "header",
        json!({"rows": 1, "pins_per_row": 4, "pitch": "2.54", "absent_pins": [3]}),
    );
    assert_eq!(numbers(&header), ["1", "2", "4"]);
    for (family, extra) in [
        ("ssop", json!({"pins": 20})),
        ("tssop", json!({"pins": 14})),
        ("dfn", json!({"pins": 8})),
        ("qfp", json!({"pins": 32})),
    ] {
        let mut params = extra.clone();
        params["absent_pins"] = json!([2]);
        let (_, file) = generate(&mut e, family, params);
        assert!(!numbers(&file).contains(&"2".to_string()), "{family}");
    }
    for (absent, pointer) in [
        (json!([9]), "/absent_pins/0"),
        (json!([3, 3]), "/absent_pins/1"),
        (json!([1, 2, 3, 4, 5, 6, 7, 8]), "/absent_pins"),
    ] {
        let err = e.run_err(
            "library.footprint.generate.soic",
            json!({"library": LIBRARY, "pins": 8, "absent_pins": absent}),
        );
        assert_eq!(err.details.unwrap()["pointer"], pointer);
    }
}

#[test]
fn pin_1_can_sit_in_the_middle_of_the_top_side() {
    let dir = TempDir::new("gen-pin1");
    let mut e = engine_with_library(&dir);
    // LQFP-32, 8 per side at 0.80 mm: the top side's middle-left lead is at x -0.40;
    // counter-clockwise from there pins 2–4 run left along the top, then 5 starts the
    // left side at its top, y 2.80.
    let (_, qfp) = generate(
        &mut e,
        "qfp",
        json!({"pins": 32, "pin1_location": "top_centre"}),
    );
    let qfp_pads = pads(&qfp);
    let (_, x1, y1, _, _) = qfp_pads[0];
    assert_eq!(x1, -400_000);
    assert!(y1 > 0);
    assert_eq!((qfp_pads[3].1, qfp_pads[3].2), (-2_800_000, y1));
    let (_, x5, y5, _, _) = qfp_pads[4];
    assert!(x5 < 0);
    assert_eq!(y5, 2_800_000);
    // Pin 32 is the lead right of pin 1.
    assert_eq!((qfp_pads[31].1, qfp_pads[31].2), (400_000, y1));
    let (_, qfn) = generate(
        &mut e,
        "qfn",
        json!({"pins": 16, "pin1_location": "top_centre"}),
    );
    assert_eq!(
        pads(&qfn)[0].1,
        -250_000,
        "4 per side at 0.50: left of the middle"
    );
    let err = e.run_err(
        "library.footprint.generate.dfn",
        json!({"library": LIBRARY, "pins": 8, "pin1_location": "top_centre"}),
    );
    assert_eq!(err.details.unwrap()["pointer"], "/pin1_location");
}
