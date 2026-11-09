//! Cut, copy and paste on a sheet in the GUI, on a Shell without a window: Mod+V floats
//! the clipboard on the cursor with a hotspot on the grid until a click; Mod+D stamps
//! copies until Esc; Clipboard Reference asks for the reference point; Copy As Text; and
//! the Net tooltip of a hovered Wire.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::canvas::interact::Pointer;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::keymap::{self, Chord};
use reda_ui::schematic_editor::{self, Interaction, Scale};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const MIL: f64 = 25_400.0;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn shell(dir: &TempDir, home: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
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

fn at(x: f64, y: f64) -> (f64, f64) {
    (x * MIL, y * MIL)
}

fn place(shell: &mut Shell, x: i64, y: i64) -> Uuid {
    let out = shell
        .run(
            "schematic.component.place",
            json!({"document": SHEET, "part": R_10K,
                   "at": {"x": format!("{x}mil"), "y": format!("{y}mil")}}),
        )
        .unwrap();
    out["result"]["unit"].as_str().unwrap().parse().unwrap()
}

fn run_all(shell: &mut Shell, commands: Vec<(String, Value)>) -> Vec<String> {
    commands
        .into_iter()
        .map(|(id, params)| {
            shell.run(&id, params).unwrap();
            id
        })
        .collect()
}

fn pointer(shell: &mut Shell, events: &[Pointer]) -> Vec<String> {
    let commands = schematic_editor::pointer(shell, sheet(), events, Scale::at(MIL));
    run_all(shell, commands)
}

fn press(shell: &mut Shell, key: &str) -> Vec<String> {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(sheet()));
    let action = shell.keymap().lookup(&chord, &active).cloned().unwrap();
    match bars::resolve_for(shell, &action, Some(sheet())).run {
        Ok(command) => run_all(shell, vec![command]),
        Err(why) => panic!("{key} cannot run: {why}"),
    }
}

fn click(shell: &mut Shell, p: (f64, f64)) -> Vec<String> {
    let modifiers = Modifiers::NONE;
    pointer(shell, &[Pointer::Click { at: p, modifiers }])
}

/// `(designator, x, y)` of every Unit on the sheet, x and y in mil, sorted.
fn units(shell: &Shell) -> Vec<(String, i64, i64)> {
    let project = shell.engine().project().unwrap();
    let mut out: Vec<_> = project
        .schematic_sheet(sheet())
        .unwrap()
        .units()
        .map(|u| (u.designator.clone(), u.at.x.0 / 25_400, u.at.y.0 / 25_400))
        .collect();
    out.sort();
    out
}

fn interaction(shell: &Shell) -> Interaction {
    shell.document_tab(sheet()).unwrap().interaction.clone()
}

#[test]
fn a_paste_floats_on_the_cursor_with_a_hotspot_on_the_grid_until_a_click() {
    let dir = TempDir::new("float-paste");
    let home = TempDir::new("float-paste-home");
    let mut shell = shell(&dir, &home);
    let r1 = place(&mut shell, 0, 0);
    shell
        .run("selection.set", json!({"objects": [r1]}))
        .unwrap();
    assert_eq!(press(&mut shell, "Mod+C"), ["tool.copy"]);
    assert!(shell.clipboard().is_some());
    pointer(&mut shell, &[Pointer::Hover(at(990.0, 1020.0))]);
    let ran = press(&mut shell, "Mod+V");
    assert_eq!(ran, ["tool.paste"]);
    // Floating: nothing is pasted yet.
    assert!(matches!(interaction(&shell), Interaction::Pasting(_)));
    assert_eq!(units(&shell).len(), 1);
    // Off grid, the Symbol origin (the hotspot) still lands on the 50 mil grid.
    pointer(&mut shell, &[Pointer::Hover(at(2010.0, 1990.0))]);
    let Interaction::Pasting(p) = interaction(&shell) else {
        panic!("still floating");
    };
    let spot = p.at + p.hold;
    assert_eq!(spot.x.0 % (50 * 25_400), 0, "{spot:?}");
    assert_eq!(spot.y.0 % (50 * 25_400), 0, "{spot:?}");
    // The click pastes once, as one Command, and the paste ends.
    assert_eq!(click(&mut shell, at(2010.0, 1990.0)), ["schematic.paste"]);
    // The pointer holds the copy's lower-left corner (on grid): R2 lands 2000 mil right
    // of R1's place in the copy and on the grid.
    assert_eq!(
        units(&shell),
        [("R1".into(), 0, 0), ("R2".into(), 2000, 2150)]
    );
    assert!(matches!(interaction(&shell), Interaction::Idle));
}

#[test]
fn duplicate_stamps_copies_until_escape() {
    let dir = TempDir::new("stamp");
    let home = TempDir::new("stamp-home");
    let mut shell = shell(&dir, &home);
    let r1 = place(&mut shell, 0, 0);
    shell
        .run("selection.set", json!({"objects": [r1]}))
        .unwrap();
    pointer(&mut shell, &[Pointer::Hover(at(0.0, 0.0))]);
    assert_eq!(press(&mut shell, "Mod+D"), ["tool.duplicate"]);
    // Duplicate does not touch what Mod+V pastes.
    assert!(shell.clipboard().is_none());
    click(&mut shell, at(1000.0, 0.0));
    click(&mut shell, at(2000.0, 0.0));
    let all = units(&shell);
    let names: Vec<&str> = all.iter().map(|u| u.0.as_str()).collect();
    assert_eq!(names, ["R1", "R2", "R3"]);
    // One stamp per click, the same distance apart as the clicks.
    assert_eq!(all[2].1 - all[1].1, 1000);
    assert_eq!(all[2].2, all[1].2);
    assert!(matches!(interaction(&shell), Interaction::Pasting(_)));
    press(&mut shell, "Escape");
    assert!(matches!(interaction(&shell), Interaction::Idle));
    click(&mut shell, at(3000.0, 0.0));
    assert_eq!(units(&shell).len(), 3);
}

#[test]
fn clipboard_reference_asks_for_the_point_a_paste_then_holds() {
    let dir = TempDir::new("reference");
    let home = TempDir::new("reference-home");
    let mut shell = shell(&dir, &home);
    let r1 = place(&mut shell, 0, 0);
    shell
        .run(
            "app.preferences.set",
            json!({"key": "schematic.clipboard_reference", "value": true}),
        )
        .unwrap();
    shell
        .run("selection.set", json!({"objects": [r1]}))
        .unwrap();
    assert_eq!(press(&mut shell, "Mod+X"), ["tool.copy"]);
    // Nothing is cut until the reference point is clicked.
    assert!(matches!(interaction(&shell), Interaction::Referencing(_)));
    assert_eq!(units(&shell).len(), 1);
    assert_eq!(click(&mut shell, at(100.0, 100.0)), ["schematic.cut"]);
    assert!(units(&shell).is_empty());
    let origin = &shell.clipboard().unwrap()["origin"];
    assert_eq!(origin, &json!({"x": 100 * 25_400, "y": 100 * 25_400}));
}

#[test]
fn copy_as_text_leaves_the_names_for_the_system_clipboard() {
    let dir = TempDir::new("copy-text");
    let home = TempDir::new("copy-text-home");
    let mut shell = shell(&dir, &home);
    let r1 = place(&mut shell, 0, 0);
    let label = shell
        .run(
            "schematic.net_label.add",
            json!({"document": SHEET, "name": "SDA", "at": {"x": "0mil", "y": "1000mil"}}),
        )
        .unwrap();
    let label = &label["result"]["net_label"];
    shell
        .run("selection.set", json!({"objects": [r1, label]}))
        .unwrap();
    let action = (bars::for_document(Some(bars::DocumentType::Schematic))
        .menus
        .iter())
    .flat_map(|m| m.items.iter())
    .find_map(|i| match i {
        bars::Item::Action(a) if a.id == "edit.copy_as_text" => Some(a.clone()),
        _ => None,
    })
    .unwrap();
    let (id, params) = bars::resolve_for(&shell, &action, Some(sheet()))
        .run
        .unwrap();
    shell.run(&id, params).unwrap();
    assert_eq!(shell.take_copied_text().as_deref(), Some("R1\nSDA"));
    assert_eq!(shell.take_copied_text(), None);
}

#[test]
fn hovering_a_wire_shows_its_net_and_board_name() {
    let dir = TempDir::new("wire-tip");
    let home = TempDir::new("wire-tip-home");
    let mut shell = shell(&dir, &home);
    shell
        .run(
            "schematic.wire.add",
            json!({"document": SHEET, "points": [
                {"x": "0mil", "y": "0mil"}, {"x": "1000mil", "y": "0mil"}]}),
        )
        .unwrap();
    shell
        .run(
            "schematic.net_label.add",
            json!({"document": SHEET, "name": "SDA", "at": {"x": "0mil", "y": "0mil"}}),
        )
        .unwrap();
    pointer(&mut shell, &[Pointer::Hover(at(500.0, 0.0))]);
    assert_eq!(
        schematic_editor::hover_tip(&shell, sheet()).as_deref(),
        Some("Net: SDA")
    );
    shell
        .run(
            "schematic.options.set",
            json!({"append_sheet_numbers": true}),
        )
        .unwrap();
    pointer(&mut shell, &[Pointer::Hover(at(500.0, 0.0))]);
    let tip = schematic_editor::hover_tip(&shell, sheet()).unwrap();
    assert!(tip.contains("SDA_1"), "{tip}");
    pointer(&mut shell, &[Pointer::Hover(at(500.0, 3000.0))]);
    assert_eq!(schematic_editor::hover_tip(&shell, sheet()), None);
}
