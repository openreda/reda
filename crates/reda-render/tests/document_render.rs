//! `document.render` (SVG) and `schematic.export_pdf`, run through the engine.

use std::path::{Path, PathBuf};

mod common;

use reda_model::{LayerKind, Side};
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
fn a_board_renders_as_svg_one_group_per_layer_at_its_opacity() {
    let dir = TempDir::new("render-board-svg");
    let mut engine = common::demo(&dir);
    let board = common::sample_board(&mut engine);
    let svg = |engine: &mut TestEngine, extra: serde_json::Value| -> String {
        let mut params = json!({"document": board.pcb.to_string(), "format": "svg"});
        for (k, v) in extra.as_object().unwrap() {
            params[k] = v.clone();
        }
        let out = engine.run_ok("document.render", params);
        out["svg"].as_str().unwrap().to_owned()
    };
    let all = svg(&mut engine, json!({}));
    // The canvas background, then each Layer with ink: the inactive ones at 0.7, the
    // active top copper, the holes and the Ratsnest opaque.
    assert!(all.contains("<rect x=\"-0.075\""), "{}", &all[..300]);
    let groups = all.matches("<g opacity=\"0.7\">").count();
    // Bottom copper, both silkscreens (the bottom Footprint's Designator), masks and
    // pastes, and the Board Outline.
    assert_eq!(groups, 8, "inactive Layers");
    assert_eq!(
        all.matches("<g>\n").count(),
        3,
        "top copper, drill, Ratsnest"
    );
    assert!(all.contains(&format!("data-object=\"{}\"", board.zone)));

    let dimmed = svg(&mut engine, json!({"inactive_layers": "dimmed"}));
    assert_eq!(dimmed.matches("<g opacity=\"0.25\">").count(), 8);
    let single = svg(&mut engine, json!({"inactive_layers": "hidden"}));
    assert!(!single.contains("opacity="));
    assert!(!single.contains(&format!("data-object=\"{}\"", board.zone)));

    // Only the bottom copper, active: the Zone and nothing of the top.
    let bottom = common::layer(&engine, LayerKind::Copper, Some(Side::Bottom));
    let only = svg(
        &mut engine,
        json!({"layers": [bottom.to_string()], "active_layer": bottom.to_string(),
               "ratsnest": false}),
    );
    assert!(only.contains(&format!("data-object=\"{}\"", board.zone)));
    assert!(!only.contains(&format!("data-object=\"{}\"", board.track)));
    assert!(!only.contains("opacity="));

    // From the bottom, the image is mirrored.
    let flipped = svg(&mut engine, json!({"from_bottom": true}));
    assert!(
        flipped.contains("viewBox=\"-30.075 "),
        "{}",
        &flipped[..200]
    );
    assert!(flipped.contains("<g transform=\"scale(-1 1)\">"));
}

#[test]
fn layer_params_are_checked_against_the_board() {
    let dir = TempDir::new("render-board-params");
    let mut engine = common::demo(&dir);
    let board = common::sample_board(&mut engine);
    let pcb = board.pcb.to_string();
    let missing = "00000000-0000-4000-8000-0000000000ff";
    let pointer = |engine: &mut TestEngine, params: serde_json::Value| -> String {
        let err = engine.run_err("document.render", params);
        serde_json::to_value(err).unwrap()["details"]["pointer"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let top = common::layer(&engine, LayerKind::Copper, Some(Side::Top)).to_string();
    let bottom = common::layer(&engine, LayerKind::Copper, Some(Side::Bottom)).to_string();
    assert_eq!(
        pointer(
            &mut engine,
            json!({"document": pcb, "format": "svg", "layers": [top, missing]})
        ),
        "/layers/1"
    );
    assert_eq!(
        pointer(
            &mut engine,
            json!({"document": pcb, "format": "svg", "active_layer": missing})
        ),
        "/active_layer"
    );
    assert_eq!(
        pointer(
            &mut engine,
            json!({"document": pcb, "format": "svg", "layers": [top], "active_layer": bottom})
        ),
        "/active_layer"
    );
    assert_eq!(
        pointer(
            &mut engine,
            json!({"document": SHEET, "format": "svg", "from_bottom": true})
        ),
        "/from_bottom"
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
        "schematic.sheet.set_frame",
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
