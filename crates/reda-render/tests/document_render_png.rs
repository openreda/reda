//! `document.render` with `format: png`: params, errors, and the PNG it returns.
//!
//! Tests that need pixels skip (with a message) when the machine has no GPU or software
//! adapter; they then check that the error names the cause instead.

use std::path::{Path, PathBuf};

use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

/// The rendered PNG's bytes, or `None` (after checking the error names the cause) when
/// this machine cannot render.
fn render(engine: &mut TestEngine, params: Value) -> Option<Vec<u8>> {
    match engine.run("document.render", params) {
        Ok(out) => {
            assert_eq!(out["format"], "png");
            let png = decode_base64(out["png"].as_str().unwrap());
            let (w, h) = png_size(&png);
            assert_eq!(
                (out["width"].as_u64(), out["height"].as_u64()),
                (Some(w), Some(h))
            );
            Some(png)
        }
        Err(e) => {
            assert_eq!(e.code, reda_command::ErrorCode::Unavailable, "{e:?}");
            assert!(e.message.contains("no GPU or software renderer"), "{e:?}");
            eprintln!("skipping: {}", e.message);
            None
        }
    }
}

fn refuse(engine: &mut TestEngine, params: Value) -> Value {
    serde_json::to_value(engine.run_err("document.render", params)).unwrap()
}

fn png_size(png: &[u8]) -> (u64, u64) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let be = |i: usize| u64::from(u32::from_be_bytes(png[i..i + 4].try_into().unwrap()));
    (be(16), be(20))
}

fn decode_base64(text: &str) -> Vec<u8> {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes().filter(|c| *c != b'=') {
        acc = acc << 6 | ABC.iter().position(|a| *a == c).expect("base64 digit") as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

#[test]
fn a_sheet_renders_fitted_at_1024_px_on_the_long_side() {
    let dir = TempDir::new("doc-render-fit");
    let mut engine = demo(&dir);
    let Some(png) = render(&mut engine, json!({"document": SHEET})) else {
        return;
    };
    // A4 landscape: 297 × 210 mm.
    assert_eq!(png_size(&png), (1024, 724));
}

#[test]
fn size_sets_the_long_side_and_a_rect_view_sets_the_shape() {
    let dir = TempDir::new("doc-render-rect");
    let mut engine = demo(&dir);
    let params = json!({"document": SHEET, "size": 300,
        "view": {"rect": {"min": {"x": "0mm", "y": "0mm"}, "max": {"x": "50mm", "y": "100mm"}}}});
    let Some(png) = render(&mut engine, params) else {
        return;
    };
    assert_eq!(png_size(&png), (150, 300));
}

#[test]
fn themes_and_highlights_change_the_image() {
    let dir = TempDir::new("doc-render-theme");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": {"x": "100mm", "y": "100mm"}}),
    );
    let unit = out["result"]["unit"].as_str().unwrap().to_owned();
    let base = json!({"document": SHEET, "size": 200});
    let Some(dark) = render(&mut engine, base.clone()) else {
        return;
    };
    let light = render(
        &mut engine,
        json!({"document": SHEET, "size": 200, "theme": "light"}),
    );
    let lit = render(
        &mut engine,
        json!({"document": SHEET, "size": 200, "highlight": [unit]}),
    );
    let fitted = render(
        &mut engine,
        json!({"document": SHEET, "size": 200, "view": {"objects": [unit]}}),
    );
    let again = render(
        &mut engine,
        json!({"document": SHEET, "size": 200, "theme": "dark", "view": "fit"}),
    );
    assert_ne!(Some(&dark), light.as_ref());
    assert_ne!(Some(&dark), lit.as_ref());
    assert_ne!(Some(&dark), fitted.as_ref());
    assert_eq!(Some(&dark), again.as_ref());
}

#[test]
fn bad_params_are_refused_before_rendering() {
    let dir = TempDir::new("doc-render-bad");
    let mut engine = demo(&dir);
    let missing = "00000000-0000-4000-8000-0000000000ff";
    let e = refuse(&mut engine, json!({"document": missing}));
    assert_eq!(e["code"], "not_found");

    let e = refuse(&mut engine, json!({"document": SHEET, "format": "gif"}));
    assert_eq!(e["code"], "invalid_params");

    let e = refuse(&mut engine, json!({"document": SHEET, "size": 0}));
    assert_eq!(e["details"]["pointer"], "/size");

    let e = refuse(&mut engine, json!({"document": SHEET, "size": 9000}));
    assert_eq!(e["details"]["pointer"], "/size");

    let e = refuse(&mut engine, json!({"document": SHEET, "layers": [missing]}));
    assert_eq!(e["details"]["pointer"], "/layers");

    let e = refuse(&mut engine, json!({"document": PCB}));
    assert_eq!(e["code"], "rejected");
}
