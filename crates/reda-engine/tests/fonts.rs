//! Font bytes handed to the engine, as the app does with the fonts installed on the
//! machine.

use reda_engine::Engine;
use reda_engine::command::{FixedClock, Fonts, SeededIds};
use reda_testkit::{TempDir, TestEngine};
use serde_json::json;

const NOTO_SANS: &[u8] = include_bytes!("../../reda-text/fonts/noto-sans/NotoSans-Regular.ttf");

fn engine_with(fonts: Fonts, dir: &TempDir) -> TestEngine {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .fonts(fonts)
        .build();
    let mut engine = TestEngine(engine);
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn text_in(engine: &mut TestEngine, font: &str) -> serde_json::Value {
    let out = engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "Hi", "x": 0, "y": 0, "font": font}),
    );
    let uuid = &out["result"]["uuid"];
    engine.run_ok("pcb.text.geometry", json!({"texts": [uuid]}))
}

#[test]
fn a_font_handed_to_the_engine_draws_its_text() {
    let dir = TempDir::new("fonts-handed");
    let mut fonts = Fonts::default();
    fonts.add("My Font", NOTO_SANS.to_vec());
    let mut engine = engine_with(fonts, &dir);
    let out = text_in(&mut engine, "my font");
    assert_eq!(out["missing_fonts"], json!([]));
    let text = &out["texts"][0];
    assert_eq!(text["drawn_font"], "My Font");
    assert_eq!(text["missing_font"], false);
    assert!(!text["fill"]["indices"].as_array().unwrap().is_empty());
}

#[test]
fn fonts_can_be_handed_over_after_the_engine_starts() {
    let dir = TempDir::new("fonts-later");
    let mut engine = engine_with(Fonts::default(), &dir);
    assert_eq!(
        text_in(&mut engine, "Late")["missing_fonts"],
        json!(["Late"])
    );
    engine.fonts_mut().add("Late", NOTO_SANS.to_vec());
    assert_eq!(engine.fonts().families().collect::<Vec<_>>(), ["Late"]);
    assert_eq!(text_in(&mut engine, "Late")["missing_fonts"], json!([]));
}

#[test]
fn unreadable_font_bytes_count_as_missing() {
    let dir = TempDir::new("fonts-broken");
    let mut fonts = Fonts::default();
    fonts.add("Broken", b"not a font".to_vec());
    let mut engine = engine_with(fonts, &dir);
    let out = text_in(&mut engine, "Broken");
    assert_eq!(out["missing_fonts"], json!(["Broken"]));
    assert_eq!(out["texts"][0]["drawn_font"], "Noto Sans");
}
