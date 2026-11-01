//! The geometry of PCB text, as the `pcb.text.geometry` query returns it for outputs and
//! rendering, and how missing fonts are drawn and reported.

use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn with_pcb(mut engine: TestEngine, dir: &TempDir) -> TestEngine {
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn add_text(engine: &mut TestEngine, params: Value) -> String {
    let out = engine.run_ok("pcb.text.add", params);
    out["result"]["uuid"].as_str().unwrap().to_owned()
}

fn geometry(engine: &mut TestEngine, uuid: &str) -> Value {
    let out = engine.run_ok("pcb.text.geometry", json!({"texts": [uuid]}));
    out["texts"][0].clone()
}

fn seg(ax: i64, ay: i64, bx: i64, by: i64) -> Value {
    json!({"a": {"x": ax, "y": ay}, "b": {"x": bx, "y": by}})
}

// Roman Simplex capitals are 21 Hershey units tall, so a 2.1 mm cap height makes one unit
// 0.1 mm. Its `A` is three strokes, from the apex at (0.9 mm, 2.1 mm) relative to the start
// of the baseline (see reda-text's stroke tests).

#[test]
fn hershey_text_is_placed_on_the_board() {
    let dir = TempDir::new("geom-place");
    let mut engine = with_pcb(probe_engine(), &dir);
    let uuid = add_text(
        &mut engine,
        json!({"layer": "silkscreen_top", "text": "A", "x": "10mm", "y": "5mm",
               "height": "2.1mm", "thickness": "0.2mm"}),
    );
    let text = geometry(&mut engine, &uuid);
    assert_eq!(text["uuid"], uuid);
    assert_eq!(text["font"], "Hershey Roman Simplex");
    assert_eq!(text["drawn_font"], "Hershey Roman Simplex");
    assert_eq!(text["missing_font"], false);
    assert_eq!(text["stroke_width"], 200_000);
    assert_eq!(
        text["strokes"],
        json!([
            seg(10_900_000, 7_100_000, 10_100_000, 5_000_000),
            seg(10_900_000, 7_100_000, 11_700_000, 5_000_000),
            seg(10_400_000, 5_700_000, 11_400_000, 5_700_000),
        ])
    );
    assert_eq!(text["fill"]["indices"], json!([]));
}

#[test]
fn rotation_turns_text_about_its_start() {
    let dir = TempDir::new("geom-rotate");
    let mut engine = with_pcb(probe_engine(), &dir);
    let uuid = add_text(
        &mut engine,
        json!({"layer": "silkscreen_top", "text": "A", "x": "10mm", "y": "5mm",
               "height": "2.1mm", "rotation": "90deg"}),
    );
    // A quarter turn counter-clockwise takes (x, y) to (-y, x) about the start.
    assert_eq!(
        geometry(&mut engine, &uuid)["strokes"][0],
        seg(7_900_000, 5_900_000, 10_000_000, 5_100_000)
    );
}

#[test]
fn mirrored_bottom_text_reads_from_below() {
    let dir = TempDir::new("geom-mirror");
    let mut engine = with_pcb(probe_engine(), &dir);
    let uuid = add_text(
        &mut engine,
        json!({"layer": "silkscreen_bottom", "text": "A", "x": "10mm", "y": "5mm",
               "height": "2.1mm"}),
    );
    // Mirrored about the vertical line through the start: x goes to -x.
    assert_eq!(
        geometry(&mut engine, &uuid)["strokes"][0],
        seg(9_100_000, 7_100_000, 9_900_000, 5_000_000)
    );
}

#[test]
fn each_line_sits_below_the_one_before() {
    let dir = TempDir::new("geom-lines");
    let mut engine = with_pcb(probe_engine(), &dir);
    let uuid = add_text(
        &mut engine,
        json!({"layer": "silkscreen_top", "text": "A\nA", "x": 0, "y": 0, "height": "2.1mm"}),
    );
    let strokes = geometry(&mut engine, &uuid)["strokes"].clone();
    assert_eq!(strokes.as_array().unwrap().len(), 6);
    // The second line's apex sits one line pitch (1.6 × the height) below the first's.
    assert_eq!(
        strokes[3]["a"],
        json!({"x": 900_000, "y": 2_100_000 - 3_360_000})
    );
}

#[test]
fn a_missing_font_draws_as_noto_sans_and_is_reported() {
    let dir = TempDir::new("geom-missing");
    let mut engine = with_pcb(probe_engine(), &dir);
    let missing = add_text(
        &mut engine,
        json!({"layer": "silkscreen_top", "text": "Hi", "x": 0, "y": 0, "font": "Comic Sans"}),
    );
    add_text(
        &mut engine,
        json!({"layer": "copper_top", "text": "ok", "x": 0, "y": 0, "font": "noto sans"}),
    );
    let out = engine.run_ok("pcb.text.geometry", json!({}));
    assert_eq!(out["missing_fonts"], json!(["Comic Sans"]));
    let pcb = engine.project().unwrap().pcb().unwrap();
    let fonts = reda_command::Fonts::default();
    assert_eq!(reda_pcb::missing_fonts(pcb, &fonts), ["Comic Sans"]);
    let texts = out["texts"].as_array().unwrap();
    assert_eq!(texts.len(), 2, "every text when none are named");
    let text = texts
        .iter()
        .find(|t| t["uuid"] == missing.as_str())
        .unwrap();
    assert_eq!(text["font"], "Comic Sans");
    assert_eq!(text["drawn_font"], "Noto Sans");
    assert_eq!(text["missing_font"], true);
    assert_eq!(text["strokes"], json!([]));
    assert!(!text["fill"]["indices"].as_array().unwrap().is_empty());
}

#[test]
fn an_unknown_text_is_not_found() {
    let dir = TempDir::new("geom-unknown");
    let mut engine = with_pcb(probe_engine(), &dir);
    let err = engine.run_err(
        "pcb.text.geometry",
        json!({"texts": ["00000000-0000-4000-8000-0000000000ff"]}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::NotFound);
}
