//! `schematic.export_pdf`: the schematic PDF's sheets, outline, links, colour modes and
//! what it leaves to the canvas, checked against byte goldens.

use std::path::{Path, PathBuf};

use reda_engine::preferences;
use reda_engine::render::schematic_scene_styled;
use reda_engine::schematic_style::{Drawn, schematic_style};
use reda_model::Uuid;
use reda_testkit::{TempDir, TestEngine, assert_golden, test_engine_with_library_in};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";

/// Paper colours as the PDF writes them (four places).
const NOT_FITTED_CROSS: &str = "0.8157 0.0627 0.0627";
const NOT_FITTED_MESH: &str = "0.4863 0.502 0.5255";
const MASKED_BODY: &str = "0.8471 0.7686 0.7529";
const BODY: &str = "0.6196 0.1333 0.102";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/schematic_pdf")
        .join(name)
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library_in(&library(), &dir.path().join("home"));
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mm"), "y": format!("{y}mm")})
}

fn add_sheet(engine: &mut TestEngine, title: &str) -> String {
    let out = engine.run_ok("document.add", json!({"kind": "schematic", "title": title}));
    out["result"]["uuid"].as_str().unwrap().to_owned()
}

/// Places Part `part` on `sheet`; returns its Component's UUID.
fn place(engine: &mut TestEngine, sheet: &str, part: &str, x: i64, y: i64) -> String {
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": sheet, "part": part, "at": at(x, y)}),
    );
    out["result"]["component"]["uuid"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Exports with `params` (and `path`) and returns the file's bytes.
fn export(engine: &mut TestEngine, dir: &TempDir, params: Value) -> Vec<u8> {
    let mut params = params;
    params["path"] = json!("out.pdf");
    engine.run_ok("schematic.export_pdf", params);
    std::fs::read(dir.path().join("Demo/out.pdf")).unwrap()
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// Every bookmark's title, in file order (the outline is written depth first).
fn titles(pdf: &str) -> Vec<&str> {
    pdf.lines()
        .filter(|l| l.starts_with("<< /Title (") && l.contains("/Parent"))
        .map(|l| {
            let rest = &l["<< /Title (".len()..];
            &rest[..rest.find(") /Parent").unwrap()]
        })
        .collect()
}

/// Every stroked straight line, `x y m` then `x y l` then `S`, as its four numbers.
fn strokes(pdf: &str) -> Vec<[String; 4]> {
    let lines: Vec<&str> = pdf.lines().collect();
    lines
        .windows(3)
        .filter(|w| w[0].ends_with(" m") && w[1].ends_with(" l") && w[2] == "S")
        .map(|w| {
            let a: Vec<&str> = w[0].split(' ').collect();
            let b: Vec<&str> = w[1].split(' ').collect();
            [a[0], a[1], b[0], b[1]].map(str::to_owned)
        })
        .collect()
}

#[test]
fn the_outline_is_the_title_then_each_sheet_and_its_components_in_natural_sort() {
    let dir = TempDir::new("pdf-outline");
    let mut engine = demo(&dir);
    let power = add_sheet(&mut engine, "Power");
    for i in 0..11 {
        place(&mut engine, SHEET, R_10K, 20 + 10 * i, 100);
    }
    place(&mut engine, &power, LM358, 100, 100);
    let pdf = text(&export(&mut engine, &dir, json!({})));
    let first = engine
        .project()
        .unwrap()
        .document(SHEET.parse().unwrap())
        .unwrap()
        .title
        .clone();
    let mut expected = vec!["Demo".to_owned(), format!("1 {first}")];
    expected.extend((1..=11).map(|n| format!("R{n} 10k")));
    expected.extend(["2 Power".to_owned(), "U1A LM358".to_owned()]);
    assert_eq!(titles(&pdf), expected, "R10 after R9, with the Unit Suffix");
    assert!(pdf.contains("/PageMode /UseOutlines"));
    // R1 at (20, 100) mm opens there on the first page.
    assert!(
        pdf.contains("<< /Title (R1 10k) /Parent 10 0 R /Dest [4 0 R /XYZ 56.693 283.465 null]"),
        "{pdf}"
    );
    assert!(!pdf.contains("Date"), "a PDF writes no date");
}

#[test]
fn each_cross_reference_entry_links_to_its_partners_page() {
    let dir = TempDir::new("pdf-links");
    let mut engine = demo(&dir);
    let two = add_sheet(&mut engine, "Bus");
    for sheet in [SHEET, &two] {
        engine.run_ok(
            "schematic.port.add",
            json!({"document": sheet, "name": "SDA", "at": at(10, 10)}),
        );
    }
    let plain = text(&export(&mut engine, &dir, json!({})));
    assert!(!plain.contains("/Subtype /Link"), "no notes, no links");
    engine.run_ok("schematic.options.set", json!({"cross_references": true}));
    let bytes = export(&mut engine, &dir, json!({}));
    let pdf = text(&bytes);
    let links: Vec<&str> = pdf
        .lines()
        .filter(|l| l.contains("/Subtype /Link"))
        .collect();
    assert_eq!(links.len(), 2, "{links:?}");
    // Page 1 (object 4) leads to page 2 (object 6) at its Port, and back.
    assert!(
        links[0].ends_with("/Dest [6 0 R /XYZ 28.346 28.346 null] >>"),
        "{links:?}"
    );
    assert!(
        links[1].ends_with("/Dest [4 0 R /XYZ 28.346 28.346 null] >>"),
        "{links:?}"
    );
    assert!(pdf.contains("/Contents 5 0 R /Annots [8 0 R]"));
    assert!(pdf.contains("/Contents 7 0 R /Annots [9 0 R]"));
    assert_golden(golden("links.pdf"), &bytes);

    // With only the current sheet, its partner's page is not in the file: no link.
    let pdf = text(&export(
        &mut engine,
        &dir,
        json!({"sheets": "current", "document": two}),
    ));
    assert!(pdf.contains("/Count 1 >>"), "one page");
    assert!(!pdf.contains("/Subtype /Link"));
}

#[test]
fn sheets_current_needs_a_sheet() {
    let dir = TempDir::new("pdf-current");
    let mut engine = demo(&dir);
    let err = engine.run_err(
        "schematic.export_pdf",
        json!({"path": "out.pdf", "sheets": "current"}),
    );
    assert_eq!(
        serde_json::to_value(&err).unwrap()["code"],
        "invalid_params"
    );
    let err = engine.run_err(
        "schematic.export_pdf",
        json!({"path": "out.pdf", "theme": "light"}),
    );
    assert_eq!(
        serde_json::to_value(&err).unwrap()["code"],
        "invalid_params",
        "`theme` is gone: `colour` replaces it"
    );
}

/// A sheet with one Not Fitted resistor, drawn with `display`.
fn not_fitted(dir: &TempDir, display: &str) -> TestEngine {
    let mut engine = demo(dir);
    let component = place(&mut engine, SHEET, R_10K, 50, 100);
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "not_fitted": true}),
    );
    engine.run_ok(
        "schematic.options.set",
        json!({"not_fitted_display": display}),
    );
    engine
}

#[test]
fn not_fitted_display_reaches_the_pdf() {
    let dir = TempDir::new("pdf-not-fitted");
    let mut engine = not_fitted(&dir, "cross");
    let cross = export(&mut engine, &dir, json!({}));
    let pdf = text(&cross);
    assert!(pdf.contains(NOT_FITTED_CROSS), "a red cross");
    assert!(pdf.contains(MASKED_BODY), "over a masked body");
    assert!(!pdf.contains(BODY) && !pdf.contains(NOT_FITTED_MESH));
    assert_golden(golden("not_fitted_cross.pdf"), &cross);

    engine.run_ok(
        "schematic.options.set",
        json!({"not_fitted_display": "mesh"}),
    );
    let mesh = export(&mut engine, &dir, json!({}));
    let pdf = text(&mesh);
    assert!(pdf.contains(NOT_FITTED_MESH), "a grey mesh");
    assert!(pdf.contains(MASKED_BODY), "over a masked body");
    assert!(!pdf.contains(NOT_FITTED_CROSS));
    assert_golden(golden("not_fitted_mesh.pdf"), &mesh);

    engine.run_ok(
        "schematic.options.set",
        json!({"not_fitted_display": "none"}),
    );
    let pdf = text(&export(&mut engine, &dir, json!({})));
    assert!(pdf.contains(BODY), "drawn as if fitted");
    assert!(!pdf.contains(NOT_FITTED_CROSS) && !pdf.contains(NOT_FITTED_MESH));
    assert!(!pdf.contains(MASKED_BODY));
}

#[test]
fn a_mono_pdf_draws_the_cross_and_the_mesh_in_black_without_masking() {
    let dir = TempDir::new("pdf-mono");
    let mut engine = not_fitted(&dir, "none");
    let mono = json!({"colour": "mono"});
    let none = strokes(&text(&export(&mut engine, &dir, mono.clone())));
    engine.run_ok(
        "schematic.options.set",
        json!({"not_fitted_display": "cross"}),
    );
    let bytes = export(&mut engine, &dir, mono.clone());
    let pdf = text(&bytes);
    let mut colours: Vec<&str> = pdf
        .lines()
        .filter(|l| l.ends_with(" rg") || l.ends_with(" RG"))
        .map(|l| &l[..l.len() - 3])
        .collect();
    colours.sort();
    colours.dedup();
    assert_eq!(
        colours,
        ["0 0 0", "1 1 1"],
        "black on white: nothing masked"
    );
    assert_eq!(
        strokes(&pdf).len(),
        none.len() + 2,
        "the cross's two strokes"
    );
    assert_golden(golden("not_fitted_mono.pdf"), &bytes);
    engine.run_ok(
        "schematic.options.set",
        json!({"not_fitted_display": "mesh"}),
    );
    let pdf = text(&export(&mut engine, &dir, mono));
    assert!(strokes(&pdf).len() > none.len() + 2, "the box and its mesh");
}

/// A PDF of one sheet holding one Net Label named `name`.
fn labelled(name: &str) -> Vec<u8> {
    let dir = TempDir::new("pdf-overbars");
    let mut engine = demo(&dir);
    engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": name, "at": at(50, 50)}),
    );
    export(&mut engine, &dir, json!({}))
}

#[test]
fn overbars_draw_in_the_pdf() {
    let plain = strokes(&text(&labelled("RST")));
    let bytes = labelled("RS\\T");
    let extra: Vec<_> = strokes(&text(&bytes))
        .into_iter()
        .filter(|s| !plain.contains(s))
        .collect();
    assert_eq!(extra.len(), 1, "one bar over S: {extra:?}");
    let [x0, y0, x1, y1] = &extra[0];
    assert_eq!(y0, y1, "level");
    let x = |s: &str| s.parse::<f64>().unwrap();
    assert!(x(x1) - x(x0) > 0.5, "as wide as a letter: {x0}..{x1}");
    assert!(x(y0) > 50.0, "above the label, at 50 mm");
    assert_golden(golden("overbars.pdf"), &bytes);
}

#[test]
fn display_cross_overs_never_reaches_the_pdf() {
    let dir = TempDir::new("pdf-cross-overs");
    let mut engine = demo(&dir);
    let wiring = json!({"optimize_wires": false, "components_cut_wires": false,
                        "break_wires_at_autojunctions": false,
                        "convert_cross_junctions": false, "snap_grid": "100mil"});
    for points in [[at(10, 50), at(90, 50)], [at(50, 10), at(50, 90)]] {
        engine.run_ok(
            "schematic.wire.add",
            json!({"document": SHEET, "points": points, "wiring": wiring}),
        );
    }
    let off = export(&mut engine, &dir, json!({}));
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "schematic.display_cross_overs", "value": true}),
    );
    let prefs = preferences::current(engine.user_dirs());
    let project = engine.project().unwrap();
    let sheet: Uuid = SHEET.parse().unwrap();
    let drawn = |how| {
        let style = schematic_style(project, &prefs, how);
        schematic_scene_styled(project, sheet, &[], &style)
            .unwrap()
            .items
            .len()
    };
    assert!(
        drawn(Drawn::Canvas) > drawn(Drawn::Output),
        "the canvas bridges the crossing"
    );
    assert_eq!(
        export(&mut engine, &dir, json!({})),
        off,
        "the PDF does not"
    );
}
