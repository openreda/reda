//! PNG snapshots of `document.render`, compared pixel by pixel with a tolerance.
//!
//! Goldens are drawn by Mesa's llvmpipe software renderer, so the comparison runs only on
//! it: Linux CI installs Mesa and sets `REDA_PNG_SNAPSHOTS=1`, which also makes a missing
//! or different adapter a failure instead of a skip. Locally, run
//! `LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl REDA_PNG_SNAPSHOTS=1 cargo test -p reda-render
//! --test png_snapshots`, and add `REDA_BLESS=1` to rewrite the goldens.
//!
//! A failure writes `<name>.actual.png` and `<name>.diff.png` (differing pixels in red) to
//! `target/tmp/png-snapshots/`, which CI uploads.

use std::path::{Path, PathBuf};

mod common;

use reda_model::{LayerKind, Side};
use reda_render::gpu::{Headless, Image};
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";

/// A channel may differ by this much before the pixel counts as different.
const CHANNEL_TOLERANCE: u8 = 24;
/// The share of pixels that may differ (anti-aliased edges move between Mesa versions).
const PIXEL_TOLERANCE: f64 = 0.002;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test")
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/png")
        .join(format!("{name}.png"))
}

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

fn place(engine: &mut TestEngine, part: &str, x: i64, y: i64, rotation: i64) -> Value {
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": part, "at": at(x, y),
               "rotation": format!("{rotation}deg")}),
    );
    out["result"].clone()
}

/// A sheet with Units, Wires meeting in a Junction, a Net Label, a No-Connect Marker, a
/// Port, a Power Port and text. Returns R1's Component UUID.
fn busy_sheet(engine: &mut TestEngine) -> String {
    let r1 = place(engine, R_10K, 2000, 5000, 0)["component"]["uuid"].clone();
    place(engine, C_100N, 3000, 5000, 90);
    place(engine, LM358, 5000, 5000, 0);
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [at(2000, 5150), at(2000, 5600), at(4000, 5600)]}),
    );
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [at(3150, 5000), at(3400, 5000), at(3400, 5600)]}),
    );
    engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "VIN", "at": at(2500, 5600)}),
    );
    engine.run_ok(
        "schematic.no_connect.add",
        json!({"document": SHEET, "at": at(2000, 4850)}),
    );
    engine.run_ok(
        "schematic.port.add",
        json!({"document": SHEET, "name": "SDA", "at": at(4000, 5600), "direction": "out"}),
    );
    engine.run_ok(
        "schematic.power_port.add",
        json!({"document": SHEET, "name": "GND", "at": at(2000, 4500)}),
    );
    engine.run_ok(
        "schematic.text.add",
        json!({"document": SHEET, "text": "Input stage", "at": at(2000, 6000)}),
    );
    r1.as_str().unwrap().to_owned()
}

/// Whether to compare, or why not.
fn comparing() -> Result<(), String> {
    let required = std::env::var_os("REDA_PNG_SNAPSHOTS").is_some_and(|v| v == "1");
    let why_not = match Headless::new() {
        Err(e) => e.to_string(),
        Ok(h) if h.adapter().name.contains("llvmpipe") => return Ok(()),
        Ok(h) => format!(
            "the adapter is {:?}, not Mesa's llvmpipe that drew the goldens",
            h.adapter().name
        ),
    };
    if required {
        panic!("REDA_PNG_SNAPSHOTS=1 but {why_not}");
    }
    Err(why_not)
}

fn snapshot(name: &str, engine: &mut TestEngine, params: Value) {
    if let Err(why) = comparing() {
        eprintln!("skipping PNG snapshot {name}: {why}");
        return;
    }
    let out = engine.run_ok("document.render", params);
    let png = decode_base64(out["png"].as_str().unwrap());
    let actual = Image::from_png(&png).unwrap();
    let path = golden(name);
    if std::env::var_os("REDA_BLESS").is_some_and(|v| v == "1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &png).unwrap();
        return;
    }
    let expected = std::fs::read(&path)
        .map_err(|e| format!("{}: {e}; run with REDA_BLESS=1", path.display()))
        .and_then(|bytes| Image::from_png(&bytes))
        .unwrap();
    if let Err(message) = compare(&expected, &actual) {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("png-snapshots");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.actual.png")), &png).unwrap();
        if let Some(diff) = diff_image(&expected, &actual) {
            std::fs::write(dir.join(format!("{name}.diff.png")), diff.to_png()).unwrap();
        }
        panic!(
            "PNG snapshot {name} differs: {message}; see {}",
            dir.display()
        );
    }
}

fn differs(a: &[u8], b: &[u8]) -> bool {
    a.iter()
        .zip(b)
        .any(|(x, y)| x.abs_diff(*y) > CHANNEL_TOLERANCE)
}

fn compare(expected: &Image, actual: &Image) -> Result<(), String> {
    if (expected.width, expected.height) != (actual.width, actual.height) {
        return Err(format!(
            "size {}×{} instead of {}×{}",
            actual.width, actual.height, expected.width, expected.height
        ));
    }
    let total = (expected.width * expected.height) as usize;
    let bad = expected
        .rgba
        .chunks_exact(4)
        .zip(actual.rgba.chunks_exact(4))
        .filter(|(e, a)| differs(e, a))
        .count();
    if bad as f64 > total as f64 * PIXEL_TOLERANCE {
        return Err(format!("{bad} of {total} pixels differ"));
    }
    Ok(())
}

/// The actual image greyed out, with differing pixels in red.
fn diff_image(expected: &Image, actual: &Image) -> Option<Image> {
    if (expected.width, expected.height) != (actual.width, actual.height) {
        return None;
    }
    let rgba = expected
        .rgba
        .chunks_exact(4)
        .zip(actual.rgba.chunks_exact(4))
        .flat_map(|(e, a)| {
            if differs(e, a) {
                [255, 0, 0, 255]
            } else {
                let grey = ((u16::from(a[0]) + u16::from(a[1]) + u16::from(a[2])) / 6) as u8;
                [grey, grey, grey, 255]
            }
        })
        .collect();
    Some(Image {
        width: actual.width,
        height: actual.height,
        rgba,
    })
}

fn decode_base64(text: &str) -> Vec<u8> {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
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

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok(
        "project.properties.set",
        json!({"title": "Snapshot", "revision": "A", "date": "2026-11-01"}),
    );
    engine
}

#[test]
fn a_busy_sheet_in_the_light_theme() {
    let dir = TempDir::new("png-light");
    let mut engine = demo(&dir);
    busy_sheet(&mut engine);
    snapshot(
        "busy_sheet_light",
        &mut engine,
        json!({"document": SHEET, "theme": "light"}),
    );
}

#[test]
fn a_zoomed_view_in_the_dark_theme_with_a_highlight() {
    let dir = TempDir::new("png-dark");
    let mut engine = demo(&dir);
    let r1 = busy_sheet(&mut engine);
    let units = engine.run_ok("objects.query", json!({"document": SHEET, "type": "unit"}));
    let r1_unit = units["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["object"]["component"]["uuid"] == r1.as_str())
        .unwrap()["object"]["uuid"]
        .clone();
    snapshot(
        "zoomed_dark_highlight",
        &mut engine,
        json!({"document": SHEET, "theme": "dark", "size": 640, "highlight": [r1_unit],
               "view": {"rect": {"min": at(1500, 4200), "max": at(6000, 6300)}}}),
    );
}

#[test]
fn a_board_from_the_top_with_its_layers_composited() {
    let dir = TempDir::new("png-board-top");
    let mut engine = common::demo(&dir);
    let board = common::sample_board(&mut engine);
    snapshot(
        "board_top_dark",
        &mut engine,
        json!({"document": board.pcb.to_string(), "size": 600}),
    );
}

#[test]
fn a_board_from_the_bottom_with_the_others_dimmed() {
    let dir = TempDir::new("png-board-bottom");
    let mut engine = common::demo(&dir);
    let board = common::sample_board(&mut engine);
    let bottom = common::layer(&engine, LayerKind::Copper, Some(Side::Bottom));
    snapshot(
        "board_bottom_light_dimmed",
        &mut engine,
        json!({"document": board.pcb.to_string(), "size": 600, "theme": "light",
               "from_bottom": true, "active_layer": bottom.to_string(),
               "inactive_layers": "dimmed"}),
    );
}

#[test]
fn a_zoomed_board_shows_pad_labels() {
    let dir = TempDir::new("png-board-labels");
    let mut engine = common::demo(&dir);
    let board = common::sample_board(&mut engine);
    snapshot(
        "board_labels_zoomed",
        &mut engine,
        json!({"document": board.pcb.to_string(), "size": 600,
               "view": {"objects": [board.fp1.to_string()]}}),
    );
}
