//! Keys pressed in the window, driven with `egui_kittest`: the Keymap runs the binding of
//! the most specific context (active Tool → Document type → global), text fields take
//! keys first, and menus show the shortcuts.

use std::path::Path;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::keymap::Chord;
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;

/// G is bound in every context, each to a different theme (showing a Panel would focus
/// it, which takes the canvas's keys away).
const LAYERED: &str = r#"
[[bind]]
key = "G"
command = "view.theme.set:follow_os"

[[bind]]
context = "schematic"
key = "G"
command = "view.theme.set:dark"

[[bind]]
context = "tool:move"
key = "G"
command = "view.theme.set:light"
"#;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn harness(home: &TempDir, keymap: &str) -> Harness<'static, App> {
    let dirs = UserDirs::under(home.path());
    std::fs::create_dir_all(&dirs.config).unwrap();
    std::fs::write(dirs.config.join("keymap.toml"), keymap).unwrap();
    let app = App::new(Shell::new(engine_in(home.path())));
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    h.run();
    h
}

/// The Commands run since the last call, by id (Presets with their name).
fn ran(h: &mut Harness<'static, App>, seen: &mut usize) -> Vec<String> {
    let journal = h.state().shell().engine().journal();
    let out = journal[*seen..]
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| e.id.clone())
        .collect();
    *seen = journal.len();
    out
}

fn press(h: &mut Harness<'static, App>, key: Key) {
    h.key_press(key);
    h.run();
}

#[test]
fn the_most_specific_context_takes_the_key() {
    let (home, work) = (TempDir::new("keys-home"), TempDir::new("keys-work"));
    let mut h = harness(&home, LAYERED);
    let mut seen = 0;

    // The Start page: no Document, so the global binding.
    ran(&mut h, &mut seen);
    press(&mut h, Key::G);
    assert_eq!(ran(&mut h, &mut seen), ["view.theme.set:follow_os"]);

    // A schematic is the active Document: its type's binding.
    h.state_mut()
        .shell_mut()
        .new_project(work.path(), "Keys")
        .unwrap();
    h.run();
    ran(&mut h, &mut seen);
    press(&mut h, Key::G);
    assert_eq!(ran(&mut h, &mut seen), ["view.theme.set:dark"]);

    // With the Move Tool active (it needs a selection), the Tool's binding.
    let shell = h.state_mut().shell_mut();
    let sheet = shell.active_document().unwrap().document;
    let wire = shell
        .run(
            "schematic.wire.add",
            serde_json::json!({"document": sheet, "points": [{"x": 0, "y": 0}, {"x": 2_540_000, "y": 0}]}),
        )
        .unwrap()["result"]["wires"][0]
        .clone();
    shell
        .run("selection.set", serde_json::json!({ "objects": [wire] }))
        .unwrap();
    shell
        .run("tool.activate:move", serde_json::json!({}))
        .unwrap();
    assert_eq!(shell.tool(), Tool::Move);
    ran(&mut h, &mut seen);
    press(&mut h, Key::G);
    assert_eq!(ran(&mut h, &mut seen), ["view.theme.set:light"]);

    // A Tool that does not bind G falls back to the Document type's.
    h.state_mut()
        .shell_mut()
        .run("tool.activate:drag", serde_json::json!({}))
        .unwrap();
    ran(&mut h, &mut seen);
    press(&mut h, Key::G);
    assert_eq!(ran(&mut h, &mut seen), ["view.theme.set:dark"]);

    // A focused Panel takes the canvas's keys away: only global ones apply.
    h.state_mut()
        .shell_mut()
        .run("panel.show:journal", serde_json::json!({}))
        .unwrap();
    h.run();
    ran(&mut h, &mut seen);
    press(&mut h, Key::G);
    assert_eq!(ran(&mut h, &mut seen), ["view.theme.set:follow_os"]);

    // Keys no context binds do nothing.
    press(&mut h, Key::J);
    assert_eq!(ran(&mut h, &mut seen), Vec::<String>::new());
}

#[test]
fn the_default_keymap_saves_and_undoes() {
    let (home, work) = (TempDir::new("keys-home"), TempDir::new("keys-work"));
    let mut h = harness(&home, "");
    h.state_mut()
        .shell_mut()
        .new_project(work.path(), "Keys")
        .unwrap();
    h.run();
    let mut seen = 0;
    ran(&mut h, &mut seen);
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run();
    assert_eq!(ran(&mut h, &mut seen), ["project.save"]);
    h.key_press_modifiers(Modifiers::ALT, Key::Num3);
    h.run();
    assert_eq!(ran(&mut h, &mut seen), ["panel.show:messages"]);
}

#[test]
fn a_dialog_takes_keys_first() {
    let home = TempDir::new("keys-home");
    let mut h = harness(&home, LAYERED);
    let mut seen = 0;
    // The New Project dialog is open: the Keymap leaves keys to it.
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run();
    assert_eq!(ran(&mut h, &mut seen)[0], "app.dialog.open:new_project");
    assert!(h.query_by_label("Create").is_some(), "the dialog is open");
    press(&mut h, Key::G);
    assert_eq!(ran(&mut h, &mut seen), Vec::<String>::new());
}

#[test]
fn menus_show_the_shortcut_of_each_item() {
    let home = TempDir::new("keys-home");
    let mut h = harness(&home, "");
    h.get_by_label("File").click();
    h.run();
    // Labelled as on the platform egui reports.
    let label = Chord::parse("Mod+S").unwrap().label(h.ctx.os().is_mac());
    assert!(
        h.query_by_label(&format!("Save All {label}")).is_some(),
        "the File menu shows {label} beside Save All"
    );
}

#[test]
fn the_settings_dialog_is_a_project_dialog() {
    let (home, work) = (TempDir::new("keys-home"), TempDir::new("keys-work"));
    let mut h = harness(&home, LAYERED);
    h.state_mut()
        .shell_mut()
        .new_project(work.path(), "Keys")
        .unwrap();
    h.state_mut()
        .shell_mut()
        .run(
            "project.properties.set",
            serde_json::json!({"copper_weight": "2 oz"}),
        )
        .unwrap();
    h.run();
    h.state_mut().open_project_settings();
    h.run();
    assert!(
        h.query_by_label("Project Settings").is_some(),
        "the dialog is open"
    );
    let mut seen = 0;
    ran(&mut h, &mut seen);

    // Only global keys apply: G is the global binding, not the schematic's.
    press(&mut h, Key::G);
    assert_eq!(ran(&mut h, &mut seen), ["view.theme.set:follow_os"]);

    // Undo goes to the Project stack, whatever has focus.
    let before = h.state().shell().engine().journal().len();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    let journal = h.state().shell().engine().journal();
    let undo = journal[before..]
        .iter()
        .find(|e| e.id == "edit.undo")
        .expect("Mod+Z ran edit.undo");
    assert_eq!(undo.params["document"], "project");
    assert!(undo.outcome.is_ok(), "the Project edit was undone");
}
