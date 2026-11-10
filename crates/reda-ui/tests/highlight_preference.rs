//! The shared highlight Preference (docs/spec/v0.1/editing.md#highlighting): select, zoom
//! with closeness, dim with strength, and a mask strength; `[` and `]` change the
//! strengths for the session only; Find follows it; Shift+C clears everything but the
//! selection.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::shell::Shell;
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn shell(dir: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    shell
        .run("document.open", json!({"document": SHEET}))
        .unwrap();
    shell
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

fn wire(shell: &mut Shell, x: i64) -> Uuid {
    let at = |x: i64| json!({"x": format!("{x}mm"), "y": "0mm"});
    let out = shell
        .run(
            "schematic.wire.add",
            json!({"document": SHEET, "points": [at(x), at(x + 10)]}),
        )
        .unwrap();
    out["result"]["wires"][0].as_str().unwrap().parse().unwrap()
}

fn set(shell: &mut Shell, key: &str, value: serde_json::Value) {
    shell
        .run("app.preferences.set", json!({"key": key, "value": value}))
        .unwrap();
}

fn canvas_strengths(shell: &Shell) -> (f32, f32) {
    shell.document_tab(sheet()).unwrap().selection.strengths
}

#[test]
fn the_preference_has_the_spec_defaults() {
    let prefs = reda_ui::engine::preferences::HighlightPreferences::default();
    assert!(prefs.select && prefs.zoom && prefs.dim);
    assert_eq!(prefs.closeness, 0.6);
    assert_eq!(prefs.dim_strength, 0.6);
    assert_eq!(prefs.mask_strength, 0.75);
}

#[test]
fn the_strengths_reach_the_canvas_and_brackets_change_them_for_the_session_only() {
    let dir = TempDir::new("highlight-strengths");
    let mut shell = shell(&dir);
    wire(&mut shell, 0);
    assert_eq!(shell.highlight_strengths(), (0.6, 0.75));
    assert_eq!(canvas_strengths(&shell), (0.6, 0.75));

    set(&mut shell, "editing.highlight.dim_strength", json!(0.3));
    shell.run("view.highlight.clear", json!({})).unwrap();
    assert_eq!(canvas_strengths(&shell), (0.3, 0.75));

    // `]`: stronger, `[`: weaker; each within 0..1.
    let out = shell
        .run("view.highlight.strength:stronger", json!({}))
        .unwrap();
    assert_eq!(
        (out["dim"].clone(), out["mask"].clone()),
        (json!(0.4), json!(0.85))
    );
    assert_eq!(canvas_strengths(&shell), (0.4, 0.85));
    for _ in 0..5 {
        shell
            .run("view.highlight.strength:stronger", json!({}))
            .unwrap();
    }
    assert_eq!(shell.highlight_strengths(), (0.9, 1.0));
    shell
        .run("view.highlight.strength:weaker", json!({}))
        .unwrap();
    assert_eq!(shell.highlight_strengths(), (0.8, 0.9));
    // The Preference keeps its values: the session's are view state.
    assert_eq!(shell.preferences().editing.highlight.dim_strength, 0.3);
    assert_eq!(shell.preferences().editing.highlight.mask_strength, 0.75);
    let (id, params) = (
        "view.highlight.strength",
        json!({"step": f64::NAN.to_string()}),
    );
    assert!(shell.run(id, params).is_err());
}

#[test]
fn shift_c_clears_highlight_dim_and_mask_but_never_the_selection() {
    let dir = TempDir::new("highlight-clear");
    let mut shell = shell(&dir);
    let a = wire(&mut shell, 0);
    let b = wire(&mut shell, 50);
    shell.run("selection.set", json!({"objects": [a]})).unwrap();
    shell
        .run("view.highlight", json!({"objects": [b], "mask": true}))
        .unwrap();
    assert!(shell.is_masked(sheet(), a, None));
    let (id, params) = {
        let keymap = shell.keymap();
        let active = reda_ui::keymap::active(&shell);
        let action = keymap
            .lookup(&reda_ui::keymap::Chord::parse("Shift+C").unwrap(), &active)
            .unwrap()
            .clone();
        match action.binding {
            reda_ui::bars::Binding::Command(id) => (id, json!({})),
            other => panic!("{other:?}"),
        }
    };
    shell.run(&id, params).unwrap();
    assert!(shell.highlight().objects.is_empty());
    assert!(!shell.is_masked(sheet(), a, None));
    assert_eq!(shell.selection().objects, [a]);
}

#[test]
fn find_shows_a_match_as_the_preference_says() {
    let dir = TempDir::new("highlight-find");
    let mut shell = shell(&dir);
    let a = wire(&mut shell, 0);
    wire(&mut shell, 50);
    shell.reveal(sheet(), &[a]);
    assert_eq!(shell.selection().objects, [a]);
    assert_eq!(shell.highlight().objects, [a], "dimmed around it");
    assert!(shell.document_tab(sheet()).unwrap().zoom_to.is_some());

    shell.run("view.highlight.clear", json!({})).unwrap();
    shell.run("selection.set", json!({"objects": []})).unwrap();
    shell.document_tab_mut(sheet()).unwrap().zoom_to = None;
    for key in ["select", "zoom", "dim"] {
        set(
            &mut shell,
            &format!("editing.highlight.{key}"),
            json!(false),
        );
    }
    shell.reveal(sheet(), &[a]);
    assert!(shell.selection().objects.is_empty());
    assert!(shell.highlight().objects.is_empty());
    assert!(shell.document_tab(sheet()).unwrap().zoom_to.is_none());
}
