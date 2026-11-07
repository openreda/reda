//! The sections every Footprint generator shares (`goals`, `tolerances`, `pads`,
//! `outputs`), their overrides and the non-compliance warning, and the Pads, paste
//! windows, trimming and texts every generated Footprint gets.

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

fn pad<'a>(file: &'a Value, number: &str) -> &'a Value {
    file["pads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["number"] == number)
        .unwrap_or_else(|| panic!("no Pad {number}"))
}

#[test]
fn lead_pads_are_rounded_at_25_percent_capped_at_a_quarter_millimetre_radius() {
    let dir = TempDir::new("gen-sections-corners");
    let mut e = engine_with_library(&dir);
    // 0603: Pads 0.90 × 0.95 mm, shorter side 0.90: 25% of 0.45 is a 0.1125 mm radius,
    // under the cap.
    let (_, chip) = generate(&mut e, "chip", json!({"size": "0603"}));
    for number in ["1", "2"] {
        let p = pad(&chip, number);
        assert_eq!(p["shape"], "rounded_rect");
        assert_eq!(p["corner_ratio"], 25_000, "25% in thousandths of a percent");
    }
    // 2512: Pads 1.25 × 3.40 mm. 25% of 0.625 would be 0.15625 mm, under the cap too;
    // a QFN-32's 3.45 mm exposed pad would take 0.43 mm at 25%, so the cap holds it to
    // 0.25 mm: 0.25 / 1.725 = 14.493%.
    let (_, qfn) = generate(&mut e, "qfn", json!({"pins": 32}));
    let exposed = pad(&qfn, "33");
    assert_eq!(exposed["width"], 3_450_000);
    assert_eq!(exposed["corner_ratio"], 14_493);
    assert_eq!(pad(&qfn, "1")["corner_ratio"], 25_000);
}

#[test]
fn exposed_pads_connect_solid_and_lead_pads_inherit() {
    let dir = TempDir::new("gen-sections-zone");
    let mut e = engine_with_library(&dir);
    for (family, pins, exposed) in [("qfn", 32, "33"), ("dfn", 8, "9")] {
        let (_, file) = generate(&mut e, family, json!({"pins": pins}));
        assert_eq!(
            pad(&file, exposed)["zone_connection"],
            json!({"style": "solid"}),
            "{family}"
        );
        assert!(pad(&file, "1").get("zone_connection").is_none(), "{family}");
    }
    let (_, soic) = generate(&mut e, "soic", json!({"pins": 8}));
    for p in soic["pads"].as_array().unwrap() {
        assert!(p.get("zone_connection").is_none());
    }
}

/// A DFN-6 on a 4 × 4 mm body with an exposed pad `length` (along y) by `width`.
fn dfn_with_exposed_pad(length: &str, width: &str) -> Value {
    json!({"pins": 6, "dimensions": {
        "pitch": "0.65mm",
        "body_length": {"min": "3.9mm", "max": "4.1mm"},
        "body_width": {"min": "3.9mm", "max": "4.1mm"},
        "lead_width": {"min": "0.25mm", "max": "0.35mm"},
        "lead_length": {"min": "0.3mm", "max": "0.5mm"},
        "height": "0.8mm",
        "exposed_pad": {
            "length": {"min": length, "max": length},
            "width": {"min": width, "max": width},
        },
    }})
}

/// `(x, y, width, height)` of each paste window, in the Pad's frame.
fn windows(pad: &Value) -> Vec<(i64, i64, i64, i64)> {
    pad["paste"]["windows"]
        .as_array()
        .unwrap_or_else(|| panic!("no paste windows: {pad}"))
        .iter()
        .map(|w| {
            (
                w["at"]["x"].as_i64().unwrap(),
                w["at"]["y"].as_i64().unwrap(),
                w["width"].as_i64().unwrap(),
                w["height"].as_i64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn an_exposed_pad_of_2_1_by_1_6_mm_or_more_gets_paste_windows_at_about_half_coverage() {
    let dir = TempDir::new("gen-sections-paste");
    let mut e = engine_with_library(&dir);
    // 1.6 wide by 2.1 long: 2 × 2 cells of 0.80 × 1.05 mm, each window √½ of its cell on
    // both sides, rounded down to 0.01 mm: 0.56 × 0.74 mm, 49.3% of the pad.
    let (_, file) = generate(&mut e, "dfn", dfn_with_exposed_pad("2.1mm", "1.6mm"));
    let exposed = pad(&file, "7");
    assert_eq!(
        (exposed["width"].clone(), exposed["height"].clone()),
        (json!(1_600_000), json!(2_100_000))
    );
    let mut got = windows(exposed);
    got.sort();
    let (x, y, w, h) = (400_000, 525_000, 560_000, 740_000);
    assert_eq!(
        got,
        [(-x, -y, w, h), (-x, y, w, h), (x, -y, w, h), (x, y, w, h)]
    );
    let coverage = got.iter().map(|(.., w, h)| w * h).sum::<i64>() as f64 / (1.6 * 2.1e12);
    assert!((0.45..=0.55).contains(&coverage), "{coverage}");
    assert_eq!(exposed["paste"]["windows"][0]["corner_ratio"], 25_000);

    // Turned the other way it is just as large: 2 × 2 again.
    let mut turned = dfn_with_exposed_pad("1.6mm", "2.1mm");
    turned["name"] = json!("Turned");
    let (_, file) = generate(&mut e, "dfn", turned);
    assert_eq!(windows(pad(&file, "7")).len(), 4);
    // A QFN-32's 3.45 mm square pad: 3 × 3 cells of 1.15 mm, windows 0.81 mm.
    let (_, file) = generate(&mut e, "qfn", json!({"pins": 32}));
    let mut got = windows(pad(&file, "33"));
    got.sort();
    let mut expected = vec![];
    for x in [-1_150_000, 0, 1_150_000] {
        for y in [-1_150_000, 0, 1_150_000] {
            expected.push((x, y, 810_000, 810_000));
        }
    }
    assert_eq!(got, expected);

    // Just under the size, in either direction, the pad inherits the board's paste.
    for (i, (length, width)) in [("2.05mm", "1.6mm"), ("2.1mm", "1.55mm")]
        .into_iter()
        .enumerate()
    {
        let mut params = dfn_with_exposed_pad(length, width);
        params["name"] = json!(format!("Small {i}"));
        let (_, file) = generate(&mut e, "dfn", params);
        assert!(pad(&file, "7").get("paste").is_none(), "{length} × {width}");
    }
    // Lead Pads inherit too.
    assert!(pad(&file, "1").get("paste").is_none());
}

/// Each Pad's number, centre and size.
fn pad_box(file: &Value, number: &str) -> (i64, i64, i64, i64) {
    let p = pad(file, number);
    let n = |v: &Value| v.as_i64().unwrap();
    (
        n(&p["at"]["x"]),
        n(&p["at"]["y"]),
        n(&p["width"]),
        n(&p["height"]),
    )
}

/// The warnings of `code`, as the Pads each names.
fn warned(out: &Value, code: &str) -> Vec<Vec<String>> {
    out["warnings"]
        .as_array()
        .unwrap_or_else(|| panic!("no warnings list: {out}"))
        .iter()
        .filter(|w| w["code"] == code)
        .map(|w| {
            assert!(w["message"].as_str().is_some_and(|m| !m.is_empty()), "{w}");
            w["pads"]
                .as_array()
                .map(|p| p.iter().map(|n| n.as_str().unwrap().to_owned()).collect())
                .unwrap_or_default()
        })
        .collect()
}

#[test]
fn gull_wing_pads_are_trimmed_back_to_the_body_edge_with_a_warning() {
    let dir = TempDir::new("gen-sections-trim-body");
    let mut e = engine_with_library(&dir);
    // TO-236: IPC-7351 gives Z 3.35 and G 0.35 mm, which runs each Pad 0.525 mm under the
    // 1.40 mm body (E1 max). The heels are trimmed back to the body edge: G 1.40, so each
    // Pad is 0.975 mm long with its centre 1.1875 mm out.
    let (out, file) = generate(&mut e, "sot", json!({"package": "sot23_3"}));
    assert_eq!(pad_box(&file, "1"), (-1_187_500, 950_000, 975_000, 600_000));
    assert_eq!(pad_box(&file, "3"), (1_187_500, 0, 975_000, 600_000));
    assert_eq!(warned(&out, "trimmed"), [["1", "2", "3"]]);
    assert_eq!(out["violations"], json!([]));

    // No gull-wing Pad runs under its body (E1 max between the lead rows).
    let cases = [
        ("sot", json!({"package": "sot23_5"}), 1_700_000),
        ("sot", json!({"package": "sot223"}), 3_700_000),
        ("soic", json!({"pins": 8}), 4_000_000),
        ("ssop", json!({"pins": 20}), 5_600_000),
        ("tssop", json!({"pins": 14}), 4_500_000),
        ("qfp", json!({"pins": 32}), 7_100_000),
        ("qfp", json!({"pins": 100}), 14_100_000),
    ];
    for (family, params, body) in cases {
        let (out, file) = generate(&mut e, family, params);
        for p in file["pads"].as_array().unwrap() {
            let number = p["number"].as_str().unwrap();
            let (x, y, w, h) = pad_box(&file, number);
            let inner = if x.abs() >= y.abs() {
                x.abs() - w / 2
            } else {
                y.abs() - h / 2
            };
            assert!(
                inner >= body / 2,
                "{}: Pad {number} runs under the body",
                out["name"]
            );
        }
    }

    // A 0603 chip needs no trimming and warns of nothing.
    let (out, _) = generate(&mut e, "chip", json!({"size": "0603"}));
    assert_eq!(out["warnings"], json!([]));
}

#[test]
fn lead_pads_are_trimmed_to_keep_clear_of_the_exposed_pad_with_a_warning() {
    let dir = TempDir::new("gen-sections-trim-exposed");
    let mut e = engine_with_library(&dir);
    // QFN-16, 3 × 3 mm: Z 3.75, G 2.00 mm, and the exposed pad 1.65 mm square (its
    // nominal size, kept). Keeping 0.20 mm from it needs G 2.05, so every lead Pad's heel
    // comes in by 0.025 mm: 0.85 mm long, centred 1.45 mm out. One warning per trimmed
    // side pair.
    let (out, file) = generate(&mut e, "qfn", json!({"pins": 16}));
    assert_eq!(pad_box(&file, "17"), (0, 0, 1_650_000, 1_650_000));
    assert_eq!(pad_box(&file, "1"), (-1_450_000, 750_000, 850_000, 250_000));
    assert_eq!(
        pad_box(&file, "5"),
        (-750_000, -1_450_000, 250_000, 850_000)
    );
    let mut trims = warned(&out, "trimmed");
    for pads in &mut trims {
        pads.sort_by_key(|n| n.parse::<u32>().unwrap());
    }
    trims.sort();
    let numbers =
        |r: std::ops::RangeInclusive<u32>| -> Vec<String> { r.map(|n| n.to_string()).collect() };
    let mut left_right = numbers(1..=4);
    left_right.extend(numbers(9..=12));
    let mut top_bottom = numbers(5..=8);
    top_bottom.extend(numbers(13..=16));
    assert_eq!(trims, [left_right, top_bottom]);
    assert_eq!(out["violations"], json!([]));
}

#[test]
fn every_family_writes_its_height_and_an_assembly_designator() {
    let dir = TempDir::new("gen-sections-texts");
    let mut e = engine_with_library(&dir);
    let cases = [
        ("chip", json!({"size": "0805"}), 600_000),
        ("sot", json!({"package": "sot223"}), 1_800_000),
        ("soic", json!({"pins": 8}), 1_750_000),
        ("ssop", json!({"pins": 20}), 2_000_000),
        ("tssop", json!({"pins": 14}), 1_200_000),
        ("dfn", json!({"pins": 8}), 800_000),
        ("qfn", json!({"pins": 32}), 1_000_000),
        ("qfp", json!({"pins": 44}), 1_600_000),
        ("dip", json!({"pins": 8}), 5_330_000),
        (
            "header",
            json!({"rows": 2, "pins_per_row": 5, "pitch": "2.54"}),
            8_500_000,
        ),
    ];
    for (family, params, height) in cases {
        let (out, file) = generate(&mut e, family, params);
        let name = out["name"].as_str().unwrap();
        assert_eq!(file["footprint"]["height"], height, "{name}");
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
                ("Value", "assembly_top"),
            ],
            "{name}"
        );
        // The Assembly Designator sits in the middle of the body; the Value text sits
        // below the courtyard, clear of it.
        let on = |field: &str, layer: &str| {
            texts
                .iter()
                .find(|t| t["field"] == field && t["layer"] == layer)
                .unwrap()
        };
        let assembly = on("Designator", "assembly_top");
        assert_eq!(assembly["at"], json!({"x": 0, "y": 0}), "{name}");
        let courtyard_bottom = file["graphics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["layer"] == "courtyard_top")
            .unwrap()["shape"]["start"]["y"]
            .as_i64()
            .unwrap();
        let value = on("Value", "assembly_top");
        let top_of_value =
            value["at"]["y"].as_i64().unwrap() + value["style"]["height"].as_i64().unwrap() / 2;
        assert!(top_of_value < courtyard_bottom, "{name}: {value}");
        assert_eq!(value["at"]["x"], 0, "{name}");
        assert_eq!(out["violations"], json!([]), "{name}");
    }
}

/// The non-compliance warnings, as the params each names.
fn non_compliance(out: &Value) -> Vec<Vec<String>> {
    out["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "not_ipc_compliant")
        .map(|w| {
            w["params"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p.as_str().unwrap().to_owned())
                .collect()
        })
        .collect()
}

/// Generates a 0603 chip resistor named `name` with `extra` params merged in.
fn chip_0603(e: &mut TestEngine, name: &str, extra: Value) -> (Value, Value) {
    let mut params = json!({"size": "0603", "name": name});
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    generate(e, "chip", params)
}

#[test]
fn an_override_that_breaks_ipc_compliance_gives_one_warning_and_keeps_the_name() {
    let dir = TempDir::new("gen-sections-override");
    let mut e = engine_with_library(&dir);
    // The Most toe goal at Nominal: Z = 1.45 + 2·0.55 + √(0.30² + 0.10² + 0.05²) = 2.870,
    // rounded to 2.85; G stays 0.65. Pads 1.10 mm long, centred 0.875 mm out.
    let (out, file) = generate(
        &mut e,
        "chip",
        json!({"size": "0603", "goals": {"fillet": {"toe": "0.55mm"}}}),
    );
    assert_eq!(
        out["name"], "RESC1608X55N",
        "the suggested name doesn't change"
    );
    assert_eq!(pad_box(&file, "2"), (875_000, 0, 1_100_000, 950_000));
    assert_eq!(non_compliance(&out), [["/goals/fillet/toe"]]);

    // Several overrides still give one warning, naming each.
    let (out, _) = chip_0603(
        &mut e,
        "Several",
        json!({
            "goals": {"fillet": {"toe": "0.55mm"}},
            "tolerances": {"fabrication": "0.2mm"},
            "pads": {"width": "1.2mm"},
        }),
    );
    let warnings = non_compliance(&out);
    assert_eq!(warnings.len(), 1, "{out}");
    let mut params = warnings[0].clone();
    params.sort();
    assert_eq!(
        params,
        [
            "/goals/fillet/toe",
            "/pads/width",
            "/tolerances/fabrication"
        ]
    );

    // An override that gives IPC-7351's own land pattern warns of nothing.
    let (out, file) = chip_0603(
        &mut e,
        "Same",
        json!({"goals": {"fillet": {"toe": "0.35mm"}}, "pads": {"width": "0.95mm"}}),
    );
    assert_eq!(out["warnings"], json!([]));
    assert_eq!(pad_box(&file, "2"), (775_000, 0, 900_000, 950_000));
}

#[test]
fn every_calculated_value_can_be_overridden() {
    let dir = TempDir::new("gen-sections-every");
    let mut e = engine_with_library(&dir);
    // The 0603 by IPC-7351: Z 2.45, G 0.65, X 0.95 mm (Pads 0.90 × 0.95 at ±0.775).
    // Each case: an override and the right-hand Pad it gives, worked by hand.
    let cases = [
        // Fillet goals: side 0.10 makes X = 0.65 + 0.20 + 0.320 = 1.170 → 1.15.
        (
            json!({"goals": {"fillet": {"side": "0.1mm"}}}),
            (775_000, 0, 900_000, 1_150_000),
        ),
        // Heel 0.10 makes G = 0.670 − 0.20 → 0.45: Pads 1.00 long at ±0.725.
        (
            json!({"goals": {"fillet": {"heel": "0.1mm"}}}),
            (725_000, 0, 1_000_000, 950_000),
        ),
        // F 0.50: X = 0.65 + √(0.30² + 0.50² + 0.05²) = 1.235 → 1.25, Z = 2.735 → 2.75,
        // G = 1.320 − √(0.41 + 0.25 + 0.0025) = 0.506 → 0.50: Pads 1.125 at ±0.8125.
        (
            json!({"tolerances": {"fabrication": "0.5mm"}}),
            (812_500, 0, 1_125_000, 1_250_000),
        ),
        // P 0.50: X 1.242 → 1.25, Z 2.742 → 2.75, G 0.502 → 0.50, the same Pads.
        (
            json!({"tolerances": {"placement": "0.5mm"}}),
            (812_500, 0, 1_125_000, 1_250_000),
        ),
        // CW 0: X = 0.65 + √(0.10² + 0.05²) = 0.762 → 0.75.
        (
            json!({"tolerances": {"component": {"width": 0}}}),
            (775_000, 0, 900_000, 750_000),
        ),
        // CL 0: Z = 1.45 + 0.70 + 0.112 = 2.262 → 2.25; Pads 0.80 at ±0.725.
        (
            json!({"tolerances": {"component": {"span": 0}}}),
            (725_000, 0, 800_000, 950_000),
        ),
        // CS 0: Smax = 1.55 − 1.10/2 = 1.00, G = 1.00 − 0.112 = 0.888 → 0.90; Pads 0.775
        // at ±0.8375.
        (
            json!({"tolerances": {"component": {"inner": 0}}}),
            (837_500, 0, 775_000, 950_000),
        ),
        // Round-off to 0.01 mm: X 0.970 → 0.97 (size); Z 2.470 → 2.47 and G 0.670 → 0.67
        // (place), so Pads 0.90 at ±0.785.
        (
            json!({"tolerances": {"round_off": {"size": "0.01mm", "place": "0.01mm"}}}),
            (785_000, 0, 900_000, 970_000),
        ),
        // Pad size and spacing.
        (
            json!({"pads": {"length": "1mm"}}),
            (775_000, 0, 1_000_000, 950_000),
        ),
        (
            json!({"pads": {"spacing": "2mm"}}),
            (1_000_000, 0, 900_000, 950_000),
        ),
        (
            json!({"pads": {"width": "1.2mm"}}),
            (775_000, 0, 900_000, 1_200_000),
        ),
    ];
    for (i, (extra, expected)) in cases.into_iter().enumerate() {
        let (out, file) = chip_0603(&mut e, &format!("Case {i}"), extra.clone());
        assert_eq!(pad_box(&file, "2"), expected, "{extra}");
        assert_eq!(non_compliance(&out).len(), 1, "{extra}: {out}");
    }

    // Courtyard excess 1.00: the Pad's outer edge 1.225 + 1.00 → 2.25 along x, the body's
    // 0.475 + 1.00 → 1.50 along y.
    let (out, file) = chip_0603(
        &mut e,
        "Courtyard",
        json!({"tolerances": {"courtyard_excess": "1mm"}}),
    );
    let courtyard = file["graphics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["layer"] == "courtyard_top")
        .unwrap()
        .clone();
    assert_eq!(
        courtyard["shape"]["end"],
        json!({"x": 2_250_000, "y": 1_500_000})
    );
    assert_eq!(non_compliance(&out), [["/tolerances/courtyard_excess"]]);

    // The Pads' look is REDA's own, not IPC-7351's: no warning.
    let (out, file) = chip_0603(&mut e, "Rect", json!({"pads": {"shape": "rect"}}));
    assert_eq!(pad(&file, "1")["shape"], "rect");
    assert!(pad(&file, "1").get("corner_ratio").is_none());
    assert_eq!(out["warnings"], json!([]));
    let (_, file) = chip_0603(&mut e, "Round", json!({"pads": {"corner_ratio": "50%"}}));
    assert_eq!(pad(&file, "1")["corner_ratio"], 50_000);
    // A 0.10 mm cap on the 0.45 mm half-side: 22.222%.
    let (_, file) = chip_0603(
        &mut e,
        "Capped",
        json!({"pads": {"max_corner_radius": "0.1mm"}}),
    );
    assert_eq!(pad(&file, "1")["corner_ratio"], 22_222);

    // The description, in the target section.
    let (_, file) = chip_0603(&mut e, "Described", json!({"description": "Mine"}));
    assert_eq!(file["footprint"]["description"], "Mine");
}

#[test]
fn quad_spacings_exposed_pads_paste_and_trimming_can_be_overridden() {
    let dir = TempDir::new("gen-sections-quad");
    let mut e = engine_with_library(&dir);
    let qfn = |extra: Value, name: &str| {
        let mut p = json!({"pins": 32, "name": name});
        for (k, v) in extra.as_object().unwrap() {
            p[k] = v.clone();
        }
        p
    };
    // QFN-32 by IPC-7351: Pads 0.875 long at ±2.4375 on every side.
    let (out, file) = generate(
        &mut e,
        "qfn",
        qfn(json!({"pads": {"spacing_y": "5mm"}}), "Y"),
    );
    assert_eq!(
        pad_box(&file, "1").0,
        -2_437_500,
        "the left and right Pads stay"
    );
    assert_eq!(pad_box(&file, "9").1, -2_500_000, "the bottom ones move");
    assert_eq!(non_compliance(&out), [["/pads/spacing_y"]]);

    // The exposed pad's size, its paste array and coverage are REDA's own.
    let (out, file) = generate(
        &mut e,
        "qfn",
        qfn(
            json!({"pads": {"exposed_pad": {"width": "3mm", "length": "2.5mm"}}}),
            "Exposed",
        ),
    );
    assert_eq!(pad_box(&file, "33"), (0, 0, 3_000_000, 2_500_000));
    assert_eq!(out["warnings"], json!([]));
    let (_, file) = generate(
        &mut e,
        "qfn",
        qfn(
            json!({"pads": {"paste_array": {"columns": 2, "rows": 1}}}),
            "Array",
        ),
    );
    // Cells 1.725 × 3.45 at √½: 1.21 × 2.43 mm.
    let mut got = windows(pad(&file, "33"));
    got.sort();
    assert_eq!(
        got,
        [
            (-862_500, 0, 1_210_000, 2_430_000),
            (862_500, 0, 1_210_000, 2_430_000)
        ]
    );
    // 30% of each 1.15 mm cell: √0.3 · 1.15 = 0.6299 → 0.62 mm.
    let (_, file) = generate(
        &mut e,
        "qfn",
        qfn(json!({"pads": {"paste_coverage": 30}}), "Thin"),
    );
    assert!(
        windows(pad(&file, "33"))
            .iter()
            .all(|w| (w.2, w.3) == (620_000, 620_000))
    );

    // Trimming off: the SOT-23's IPC-7351 Pads run under the body, untrimmed and
    // compliant: Z 3.35, G 0.35, so 1.50 long at ±0.925.
    let (out, file) = generate(
        &mut e,
        "sot",
        json!({"package": "sot23_3", "name": "Untrimmed", "pads": {"trim": false}}),
    );
    assert_eq!(pad_box(&file, "3"), (925_000, 0, 1_500_000, 600_000));
    assert_eq!(out["warnings"], json!([]));
}

#[test]
fn outputs_switch_and_size_the_silkscreen_courtyard_and_assembly_outline() {
    let dir = TempDir::new("gen-sections-outputs");
    let mut e = engine_with_library(&dir);
    let layers = |file: &Value| -> Vec<(String, i64)> {
        let mut l: Vec<_> = file["graphics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| {
                (
                    g["layer"].as_str().unwrap().to_owned(),
                    g["width"].as_i64().unwrap(),
                )
            })
            .collect();
        l.sort();
        l.dedup();
        l
    };
    let (_, file) = generate(&mut e, "soic", json!({"pins": 8}));
    assert_eq!(
        layers(&file),
        [
            ("assembly_top".to_owned(), 100_000),
            ("courtyard_top".to_owned(), 50_000),
            ("silkscreen_top".to_owned(), 120_000),
        ]
    );
    let (_, file) = generate(
        &mut e,
        "soic",
        json!({"pins": 8, "name": "Plain", "outputs": {
            "silkscreen": false, "assembly_width": "0.2mm", "courtyard_width": "0.1mm"}}),
    );
    assert_eq!(
        layers(&file),
        [
            ("assembly_top".to_owned(), 200_000),
            ("courtyard_top".to_owned(), 100_000)
        ]
    );
    let (_, file) = generate(
        &mut e,
        "dip",
        json!({"pins": 8, "outputs": {"assembly": false, "courtyard": false, "silkscreen_width": "0.2mm"}}),
    );
    assert_eq!(layers(&file), [("silkscreen_top".to_owned(), 200_000)]);
}

#[test]
fn through_hole_holes_lands_and_courtyard_can_be_overridden() {
    let dir = TempDir::new("gen-sections-th");
    let mut e = engine_with_library(&dir);
    // DIP-8 by IPC-7251 Nominal: 0.90 mm holes in 1.40 mm Pads.
    let (out, file) = generate(
        &mut e,
        "dip",
        json!({"pins": 8, "pads": {"hole": "1mm", "diameter": "1.8mm"}}),
    );
    let p = pad(&file, "2");
    assert_eq!(
        (p["hole"]["width"].clone(), p["width"].clone()),
        (json!(1_000_000), json!(1_800_000))
    );
    assert_eq!(out["name"], "DIP762W46P254L1016H533Q8B");
    let mut params = non_compliance(&out).concat();
    params.sort();
    assert_eq!(params, ["/pads/diameter", "/pads/hole"]);
    let (out, _) = generate(
        &mut e,
        "header",
        json!({"rows": 1, "pins_per_row": 4, "pitch": "2.54", "tolerances": {"courtyard_excess": "0.5mm"}}),
    );
    assert_eq!(non_compliance(&out), [["/tolerances/courtyard_excess"]]);
}

#[test]
fn overrides_that_do_not_apply_to_a_family_are_refused() {
    let dir = TempDir::new("gen-sections-refused");
    let mut e = engine_with_library(&dir);
    let cases = [
        (
            "chip",
            json!({"size": "0603", "pads": {"spacing_y": "1mm"}}),
            "/pads/spacing_y",
        ),
        (
            "soic",
            json!({"pins": 8, "pads": {"spacing_y": "1mm"}}),
            "/pads/spacing_y",
        ),
        (
            "soic",
            json!({"pins": 8, "pads": {"exposed_pad": {"width": "1mm", "length": "1mm"}}}),
            "/pads/exposed_pad",
        ),
        (
            "qfp",
            json!({"pins": 32, "pads": {"paste_array": {"columns": 2, "rows": 2}}}),
            "/pads/paste_array",
        ),
        (
            "qfn",
            json!({"pins": 32, "pads": {"paste_coverage": 0}}),
            "/pads/paste_coverage",
        ),
        (
            "qfn",
            json!({"pins": 32, "pads": {"paste_array": {"columns": 0, "rows": 2}}}),
            "/pads/paste_array/columns",
        ),
    ];
    for (family, mut params, pointer) in cases {
        params["library"] = json!(LIBRARY);
        let err = e.run_err(
            &format!("library.footprint.generate.{family}"),
            params.clone(),
        );
        assert_eq!(
            err.details.as_ref().map(|d| d["pointer"].clone()),
            Some(json!(pointer)),
            "{params}: {err:?}"
        );
    }
}

/// Defaults are fixed in the code and written out, with every calculated value, so the
/// stored params say what the Footprint was made with, and running them again gives it
/// back.
#[test]
fn the_stored_params_hold_every_fixed_default_and_rerun_to_the_same_footprint() {
    let dir = TempDir::new("gen-sections-defaults");
    let mut e = engine_with_library(&dir);
    let (out, file) = generate(&mut e, "qfn", json!({"pins": 32}));
    let params = &file["generator"]["params"];
    assert_eq!(params["goals"]["density"], "nominal");
    assert_eq!(params["tolerances"]["fabrication"], 100_000);
    assert_eq!(params["tolerances"]["placement"], 50_000);
    // The calculated values are written out too. By hand, body 4.90–5.10, L 0.30–0.50,
    // b 0.18–0.30 mm, toe 0.30, heel 0, side -0.04: Z 5.729 → 5.75, G 4.009 → 4.00, so
    // Pads 0.875 long, 4.875 apart each way; X 0.264 → 0.25. The exposed pad is 3.45 mm
    // square, with 3 × 3 paste windows.
    assert_eq!(
        params["pads"],
        json!({
            "shape": "rounded_rect", "corner_ratio": 25_000, "max_corner_radius": 250_000,
            "length": 875_000, "length_y": 875_000, "width": 250_000,
            "spacing": 4_875_000, "spacing_y": 4_875_000,
            "exposed_pad": {"width": 3_450_000, "length": 3_450_000},
            "trim": true, "paste_coverage": 50, "paste_array": {"columns": 3, "rows": 3},
        })
    );
    assert_eq!(
        params["outputs"],
        json!({
            "silkscreen": true, "silkscreen_width": 120_000,
            "courtyard": true, "courtyard_width": 50_000,
            "assembly": true, "assembly_width": 100_000,
        })
    );
    let mut again = params.clone();
    again["name"] = json!("Again");
    let (rerun_out, rerun) = generate(&mut e, "qfn", again);
    assert_eq!(rerun["pads"].as_array().unwrap().len(), 33);
    for (a, b) in file["pads"]
        .as_array()
        .unwrap()
        .iter()
        .zip(rerun["pads"].as_array().unwrap())
    {
        let strip = |p: &Value| {
            let mut p = p.clone();
            p.as_object_mut().unwrap().remove("uuid");
            p
        };
        assert_eq!(strip(a), strip(b));
    }
    assert_eq!(rerun_out["warnings"], out["warnings"]);
}
