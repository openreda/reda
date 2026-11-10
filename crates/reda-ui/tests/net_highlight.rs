//! Net highlighting on the schematic, on a Shell without a window: Alt+click on a Net
//! highlights it on every sheet (view state: never undone, never dirtying), every canvas
//! marks it, and `view.highlight` does the same by Net name for scripts and menus.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::canvas::interact::Pointer;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::schematic_editor::{self, Scale};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const MIL: f64 = 25_400.0;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn shell(dir: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    shell
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

fn wire(shell: &mut Shell, document: Uuid, points: &[(i64, i64)]) -> Uuid {
    let points: Vec<Value> = points.iter().map(|&(x, y)| at(x, y)).collect();
    let out = shell
        .run(
            "schematic.wire.add",
            json!({"document": document, "points": points}),
        )
        .unwrap();
    uuid(&out["result"]["wires"][0])
}

fn port(shell: &mut Shell, document: Uuid, name: &str, x: i64, y: i64) -> Uuid {
    let out = shell
        .run(
            "schematic.port.add",
            json!({"document": document, "name": name, "at": at(x, y)}),
        )
        .unwrap();
    uuid(&out["result"]["port"])
}

/// Clicks on the first sheet's canvas at (`x`, `y`) mil with `modifiers`, running the
/// Commands the editor asks for; returns their ids.
fn click(shell: &mut Shell, x: f64, y: f64, modifiers: Modifiers) -> Vec<String> {
    let events = [Pointer::Click {
        at: (x * MIL, y * MIL),
        modifiers,
    }];
    let commands = schematic_editor::pointer(shell, sheet(), &events, Scale::at(MIL));
    commands
        .into_iter()
        .map(|(id, params)| {
            shell.run(&id, params).unwrap();
            id
        })
        .collect()
}

/// Two sheets whose Wires a Port pair joins, plus an unrelated Wire; both sheets open.
struct Fixture {
    second: Uuid,
    a: Uuid,
    b: Uuid,
    other: Uuid,
}

fn fixture(shell: &mut Shell) -> Fixture {
    let added = shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    let second = uuid(&added["result"]["uuid"]);
    shell
        .run(
            "schematic.component.place",
            json!({"document": SHEET, "part": R_10K, "at": at(0, 0)}),
        )
        .unwrap();
    let a = wire(shell, sheet(), &[(0, 150), (0, 300)]);
    port(shell, sheet(), "SDA", 0, 300);
    let b = wire(shell, second, &[(0, 0), (500, 0)]);
    port(shell, second, "SDA", 500, 0);
    let other = wire(shell, sheet(), &[(2000, 0), (2500, 0)]);
    shell
        .run("document.open", json!({"document": SHEET}))
        .unwrap();
    shell
        .run("document.open", json!({"document": second}))
        .unwrap();
    Fixture {
        second,
        a,
        b,
        other,
    }
}

fn marked(shell: &Shell, document: Uuid) -> Vec<Uuid> {
    shell
        .document_tab(document)
        .unwrap()
        .selection
        .highlighted
        .to_vec()
}

#[test]
fn alt_click_highlights_the_net_on_every_sheet_and_every_canvas_marks_it() {
    let dir = TempDir::new("net-highlight");
    let mut shell = shell(&dir);
    let f = fixture(&mut shell);
    let journal = shell.engine().journal().len();
    let was_dirty = shell.engine().dirty_scopes();

    let ran = click(&mut shell, 0.0, 200.0, Modifiers::ALT);
    assert_eq!(ran, ["view.highlight"]);
    let highlight = shell.highlight().clone();
    assert_eq!(highlight.net.as_deref(), Some("SDA"));
    for object in [f.a, f.b] {
        assert!(highlight.objects.contains(&object));
        assert!(marked(&shell, sheet()).contains(&object));
        assert!(marked(&shell, f.second).contains(&object));
    }
    assert!(!highlight.objects.contains(&f.other));
    // A highlight is not a selection, an edit or an undo step.
    assert!(shell.selection().objects.is_empty());
    assert_eq!(shell.engine().dirty_scopes(), was_dirty);
    assert_eq!(shell.engine().journal().len(), journal + 1);

    // Alt+click on nothing clears it.
    assert_eq!(
        click(&mut shell, 5000.0, 5000.0, Modifiers::ALT),
        ["view.highlight"]
    );
    assert!(shell.highlight().objects.is_empty());
    assert!(marked(&shell, f.second).is_empty());

    // A plain click still selects.
    assert_eq!(
        click(&mut shell, 0.0, 200.0, Modifiers::NONE),
        ["selection.set"]
    );
    assert!(shell.highlight().objects.is_empty());
}

#[test]
fn view_highlight_takes_a_net_name_and_clears_with_nothing() {
    let dir = TempDir::new("net-highlight-named");
    let mut shell = shell(&dir);
    let f = fixture(&mut shell);

    let out = shell.run("view.highlight", json!({"net": "SDA"})).unwrap();
    assert!(out["count"].as_u64().unwrap() >= 4);
    assert!(shell.highlight().objects.contains(&f.b));
    assert!(marked(&shell, sheet()).contains(&f.a));

    let err = shell
        .run("view.highlight", json!({"net": "NOPE"}))
        .unwrap_err();
    assert_eq!(err.details.unwrap()["pointer"], "/net");

    shell.run("view.highlight", json!({})).unwrap();
    assert!(shell.highlight().objects.is_empty());
    assert_eq!(shell.highlight().net, None);
}

#[test]
fn deleting_highlighted_objects_drops_them_from_the_highlight() {
    let dir = TempDir::new("net-highlight-delete");
    let mut shell = shell(&dir);
    let f = fixture(&mut shell);
    shell.run("view.highlight", json!({"net": "SDA"})).unwrap();
    shell
        .run("schematic.delete", json!({"objects": [f.a]}))
        .unwrap();
    assert!(!shell.highlight().objects.contains(&f.a));
    assert!(shell.highlight().objects.contains(&f.b));
}

#[test]
fn shift_alt_click_adds_a_net_and_shift_alt_on_nothing_keeps_the_highlight() {
    let dir = TempDir::new("net-highlight-add");
    let mut shell = shell(&dir);
    let f = fixture(&mut shell);
    click(&mut shell, 0.0, 200.0, Modifiers::ALT);
    assert!(!shell.highlight().objects.contains(&f.other));

    let ran = click(&mut shell, 2200.0, 0.0, Modifiers::ALT | Modifiers::SHIFT);
    assert_eq!(ran, ["view.highlight"]);
    for object in [f.a, f.b, f.other] {
        assert!(shell.highlight().objects.contains(&object), "{object}");
    }
    // Shift+Alt on nothing adds nothing and clears nothing.
    assert!(
        click(
            &mut shell,
            5000.0,
            5000.0,
            Modifiers::ALT | Modifiers::SHIFT
        )
        .is_empty()
    );
    assert!(shell.highlight().objects.contains(&f.other));
    // A plain Alt+click replaces the highlight with one Net again.
    click(&mut shell, 2200.0, 0.0, Modifiers::ALT);
    assert!(!shell.highlight().objects.contains(&f.a));
    assert!(shell.highlight().objects.contains(&f.other));
}

/// A Shell whose user `keymap.toml` is `keymap`.
fn shell_with_keymap(dir: &TempDir, home: &TempDir, keymap: &str) -> Shell {
    let dirs = reda_ui::engine::command::UserDirs::under(home.path());
    std::fs::create_dir_all(&dirs.config).unwrap();
    std::fs::write(dirs.config.join("keymap.toml"), keymap).unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(dirs))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.reload_keymap();
    shell.new_project(dir.path(), "Demo").unwrap();
    shell
}

#[test]
fn the_keymap_modifiers_table_moves_net_highlight_off_alt() {
    let dir = TempDir::new("net-highlight-role");
    let home = TempDir::new("net-highlight-role-home");
    // Where the window manager takes Alt+click.
    let mut shell = shell_with_keymap(&dir, &home, "[modifiers]\nnet_highlight_sheet = \"Ctrl\"\n");
    assert_eq!(
        shell.modifier_roles().net_highlight_sheet,
        reda_ui::keymap::Held::Ctrl
    );
    let f = fixture(&mut shell);
    // Alt+click is a plain pick now.
    assert_eq!(
        click(&mut shell, 0.0, 200.0, Modifiers::ALT),
        ["selection.set"]
    );
    assert!(shell.highlight().objects.is_empty());
    assert_eq!(
        click(&mut shell, 0.0, 200.0, Modifiers::CTRL),
        ["view.highlight"]
    );
    assert!(shell.highlight().objects.contains(&f.b));
}
