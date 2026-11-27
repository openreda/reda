//! The wheel mapping's `canvas.wheel.*` Preferences (docs/spec/v0.3/commands.md#preferences,
//! input-and-defaults.md#wheel-mapping), read and written through `app.preferences.get`,
//! `app.preferences.set` and `app.preferences.reset` against a test `REDA_HOME`.

use std::path::{Path, PathBuf};

use reda_engine::command::UserDirs;
use reda_engine::preferences::{self, WheelAction, WheelPreset};
use reda_testkit::{TempDir, test_engine_in};
use serde_json::{Value, json};

fn prefs_file(home: &Path) -> PathBuf {
    UserDirs::under(home).preferences_file()
}

fn write_prefs(home: &Path, text: &str) {
    let path = prefs_file(home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn read_prefs(home: &Path) -> String {
    std::fs::read_to_string(prefs_file(home)).unwrap_or_default()
}

fn get(home: &Path, key: &str) -> Value {
    test_engine_in(home).run_ok("app.preferences.get", json!({"key": key}))
}

fn set(home: &Path, key: &str, value: Value) -> Result<Value, String> {
    test_engine_in(home)
        .run("app.preferences.set", json!({"key": key, "value": value}))
        .map_err(|e| e.message)
}

#[test]
fn the_defaults_are_the_reda_table() {
    let home = TempDir::new("wheel-defaults");
    for (key, value) in [
        ("canvas.wheel.zoom", json!("")),
        ("canvas.wheel.scroll_x", json!("shift")),
        ("canvas.wheel.scroll_y", json!("ctrl")),
        ("canvas.wheel.step_layer", json!("unbound")),
        ("canvas.wheel.zoom_speed", json!(1.0)),
        ("canvas.wheel.reverse_zoom", json!(false)),
        ("canvas.wheel.touchpad", json!("auto")),
    ] {
        let out = get(home.path(), key);
        assert_eq!(out["value"], value, "{key}");
        assert_eq!(out["default"], value, "{key}");
        assert_eq!(
            preferences::pages::page_of(key),
            "editing.mouse_wheel",
            "{key}"
        );
    }
    let prefs = preferences::Preferences::default();
    assert_eq!(prefs.canvas.wheel.preset(), Some(WheelPreset::Reda));
}

#[test]
fn a_combination_is_written_in_one_spelling() {
    let home = TempDir::new("wheel-spelling");
    assert_eq!(
        set(home.path(), "canvas.wheel.step_layer", json!("Alt+Ctrl")),
        Ok(json!({"key": "canvas.wheel.step_layer", "value": "ctrl+alt"}))
    );
    set(home.path(), "canvas.wheel.zoom_speed", json!(2.5)).unwrap();
    set(home.path(), "canvas.wheel.reverse_zoom", json!(true)).unwrap();
    set(home.path(), "canvas.wheel.touchpad", json!("pan")).unwrap();
    assert_eq!(
        read_prefs(home.path()),
        "[canvas]\n\n[canvas.wheel]\nstep_layer = \"ctrl+alt\"\nzoom_speed = 2.5\nreverse_zoom = true\ntouchpad = \"pan\"\n"
    );
    let out = get(home.path(), "canvas.wheel.step_layer");
    assert_eq!(out["value"], "ctrl+alt");
    // Back to its default, the key leaves the file.
    set(home.path(), "canvas.wheel.step_layer", json!("unbound")).unwrap();
    assert!(!read_prefs(home.path()).contains("step_layer"));
}

#[test]
fn values_that_are_not_combinations_or_in_range_are_refused() {
    let home = TempDir::new("wheel-invalid");
    for (key, value) in [
        ("canvas.wheel.zoom", json!("hyper")),
        ("canvas.wheel.zoom", json!("shift+shift")),
        ("canvas.wheel.zoom_speed", json!(0.0)),
        ("canvas.wheel.zoom_speed", json!(50.0)),
        ("canvas.wheel.touchpad", json!("sometimes")),
    ] {
        assert!(
            set(home.path(), key, value.clone()).is_err(),
            "{key} = {value}"
        );
    }
    assert_eq!(read_prefs(home.path()), "");
}

#[test]
fn a_clashing_combination_is_refused_naming_the_action_that_holds_it() {
    let home = TempDir::new("wheel-clash");
    let why = set(home.path(), "canvas.wheel.scroll_y", json!("shift")).unwrap_err();
    assert!(why.contains("Scroll sideways"), "{why}");
    let why = set(home.path(), "canvas.wheel.step_layer", json!("")).unwrap_err();
    assert!(why.contains("Zoom at the cursor"), "{why}");
    assert_eq!(read_prefs(home.path()), "");
    // Unbound never clashes, and a freed combination can be taken.
    set(home.path(), "canvas.wheel.scroll_x", json!("unbound")).unwrap();
    set(home.path(), "canvas.wheel.step_layer", json!("unbound")).unwrap();
    set(home.path(), "canvas.wheel.scroll_y", json!("shift")).unwrap();
}

#[test]
fn a_reset_that_would_clash_is_refused() {
    let home = TempDir::new("wheel-reset-clash");
    set(home.path(), "canvas.wheel.zoom", json!("alt")).unwrap();
    set(home.path(), "canvas.wheel.scroll_y", json!("")).unwrap();
    // The plain wheel is the zoom's default, now held by Scroll up and down.
    let err = test_engine_in(home.path())
        .run(
            "app.preferences.reset",
            json!({"keys": ["canvas.wheel.zoom"]}),
        )
        .unwrap_err();
    assert!(
        err.message.contains("Scroll up and down"),
        "{}",
        err.message
    );
    // The whole page goes back together.
    let out = test_engine_in(home.path()).run_ok(
        "app.preferences.reset",
        json!({"page": "editing.mouse_wheel"}),
    );
    assert_eq!(
        out,
        json!({"reset": ["canvas.wheel.scroll_y", "canvas.wheel.zoom"]})
    );
}

#[test]
fn the_wheel_scrolls_table_reads_back_without_warnings() {
    let home = TempDir::new("wheel-scrolls-file");
    // Read in file order over the defaults, zoom = "ctrl" alone would clash with the
    // default scroll_y; the table is checked whole.
    write_prefs(
        home.path(),
        "[canvas.wheel]\nzoom = \"ctrl\"\nscroll_x = \"shift\"\nscroll_y = \"\"\n",
    );
    let out = test_engine_in(home.path()).run_ok("app.preferences.get", json!({}));
    assert_eq!(out["warnings"], json!([]));
    assert_eq!(out["values"]["canvas"]["wheel"]["zoom"], "ctrl");
    assert_eq!(out["values"]["canvas"]["wheel"]["scroll_y"], "");
    let loaded = preferences::parse(&read_prefs(home.path()));
    assert_eq!(
        loaded.values.canvas.wheel.preset(),
        Some(WheelPreset::WheelScrolls)
    );
}

#[test]
fn a_hand_edited_clash_falls_back_to_the_reda_table_with_a_warning() {
    let loaded =
        preferences::parse("[canvas.wheel]\nzoom = \"alt\"\nscroll_x = \"alt\"\nzoom_speed = 2\n");
    assert_eq!(loaded.values.canvas.wheel.preset(), Some(WheelPreset::Reda));
    assert_eq!(loaded.values.canvas.wheel.zoom_speed, 2.0);
    assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
    assert!(
        loaded.warnings[0].contains("Zoom at the cursor")
            && loaded.warnings[0].contains("Scroll sideways"),
        "{:?}",
        loaded.warnings
    );
}

#[test]
fn an_edited_table_reads_custom_and_the_preset_is_never_stored() {
    let home = TempDir::new("wheel-custom");
    set(home.path(), "canvas.wheel.step_layer", json!("alt")).unwrap();
    let prefs = preferences::parse(&read_prefs(home.path())).values;
    assert_eq!(prefs.canvas.wheel.preset(), None);
    assert!(
        preferences::keys()
            .iter()
            .all(|k| !k.starts_with("canvas.wheel.preset")),
        "no preset key"
    );
}

#[test]
fn a_preset_is_reached_one_clash_free_write_at_a_time() {
    let home = TempDir::new("wheel-preset-steps");
    let now = || {
        preferences::parse(&read_prefs(home.path()))
            .values
            .canvas
            .wheel
    };
    // From the REDA table to Wheel scrolls and back: the plain wheel and Ctrl swap
    // between zoom and scroll up and down, so one of them is unbound on the way.
    for to in [WheelPreset::WheelScrolls, WheelPreset::Reda] {
        let steps = now().steps_to(&to.table());
        assert_eq!(steps.len(), 3, "{steps:?}");
        for (action, binding) in steps {
            set(home.path(), action.key(), json!(binding.to_string()))
                .unwrap_or_else(|why| panic!("{action:?} {binding}: {why}"));
        }
        assert_eq!(now().preset(), Some(to));
    }
    assert_eq!(now().steps_to(&WheelPreset::Reda.table()), vec![]);
    assert_eq!(WheelAction::ALL.len(), 4);
    assert_eq!(WheelAction::StepLayer.key(), "canvas.wheel.step_layer");
}
