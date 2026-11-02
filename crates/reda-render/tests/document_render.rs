//! `document.render` (SVG) and `schematic.export_pdf`, run through the engine.

use std::path::{Path, PathBuf};

use reda_testkit::{TempDir, TestEngine, assert_golden, test_engine};
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

/// An error's code as the wire spells it.
fn code(err: &impl serde::Serialize) -> String {
    serde_json::to_value(err).unwrap()["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_sheet_renders_as_svg_fitted_to_1024_pixels() {
    let dir = TempDir::new("render-svg");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "document.render",
        json!({"document": SHEET, "format": "svg", "theme": "light"}),
    );
    assert_eq!(out["format"], "svg");
    // A4 landscape, 297 x 210 mm.
    assert_eq!(
        (out["width"].as_u64(), out["height"].as_u64()),
        (Some(1024), Some(724))
    );
    let svg = out["svg"].as_str().unwrap();
    assert_golden(golden("empty_sheet.svg"), svg.as_bytes());
}

#[test]
fn the_view_can_be_a_rectangle_or_fit_a_set_of_objects() {
    let dir = TempDir::new("render-view");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "document.render",
        json!({"document": SHEET, "format": "svg", "size": 200,
               "view": {"rect": {"min": {"x": "10mm", "y": "20mm"},
                                 "max": {"x": "50mm", "y": "40mm"}}}}),
    );
    assert_eq!(
        (out["width"].as_u64(), out["height"].as_u64()),
        (Some(200), Some(100))
    );
    assert!(
        out["svg"]
            .as_str()
            .unwrap()
            .contains("viewBox=\"10 -40 40 20\"")
    );

    let added = engine.run_ok(
        "schematic.graphic.add_rect",
        json!({"document": SHEET, "from": {"x": "100mm", "y": "100mm"},
               "to": {"x": "120mm", "y": "110mm"}}),
    );
    let rect = added["result"]["graphic"].as_str().unwrap().to_owned();
    let out = engine.run_ok(
        "document.render",
        json!({"document": SHEET, "format": "svg", "view": {"objects": [rect]},
               "highlight": [rect]}),
    );
    // The rectangle's outline, 0.254 mm wide, framed exactly.
    assert!(
        out["svg"]
            .as_str()
            .unwrap()
            .contains("viewBox=\"99.873 -110.127 20.254 10.254\""),
        "{}",
        &out["svg"].as_str().unwrap()[..200]
    );
}

#[test]
fn unknown_documents_are_not_found() {
    let dir = TempDir::new("render-errors");
    let mut engine = demo(&dir);
    let err = engine.run_err(
        "document.render",
        json!({"document": "00000000-0000-4000-8000-0000000000ff", "format": "svg"}),
    );
    assert_eq!(code(&err), "not_found", "{err:?}");
}

#[test]
fn the_schematic_pdf_has_every_sheet_in_document_order() {
    let dir = TempDir::new("render-pdf");
    let mut engine = demo(&dir);
    engine.run_ok(
        "document.add",
        json!({"kind": "schematic", "title": "Power"}),
    );
    engine.run_ok(
        "schematic.sheet.set_size",
        json!({"document": SHEET, "size": "a3"}),
    );
    engine.run_ok(
        "schematic.text.add",
        json!({"document": SHEET, "text": "Hello", "at": {"x": "20mm", "y": "100mm"}}),
    );
    let out = engine.run_ok("schematic.export_pdf", json!({"path": "Demo.pdf"}));
    assert_eq!(out["pages"], 2);
    let folder = dir.path().join("Demo");
    let path = folder.join("Demo.pdf");
    assert_eq!(out["path"], json!(path));
    let pdf = std::fs::read(&path).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    let boxes: Vec<&str> = text
        .match_indices("/MediaBox")
        .map(|(i, _)| &text[i..i + 36])
        .collect();
    assert!(
        boxes[0].starts_with("/MediaBox [0 0 1190.551 841.89]"),
        "A3 first: {boxes:?}"
    );
    assert!(
        boxes[1].starts_with("/MediaBox [0 0 841.89 595.276]"),
        "then A4: {boxes:?}"
    );
    assert!(
        text.contains("/CreationDate (D:20260101000000Z)"),
        "from the test Clock"
    );
    assert_golden(golden("two_sheets.pdf"), &pdf);

    // The same Project and Clock give the same bytes.
    engine.run_ok("schematic.export_pdf", json!({"path": "Again.pdf"}));
    assert_eq!(std::fs::read(folder.join("Again.pdf")).unwrap(), pdf);
}
