//! In-place edit on a Shell without a window: F2 or a slow second click edits a name,
//! Designator, value, field or text; Enter commits one Command, Esc cancels; a taken
//! Designator is refused inline; the In-Place Editing Preference turns it off; Tab while
//! placing a Net Label pauses the Tool to edit the next name, which repeats counting up.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::canvas::interact::Pointer;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{Object, Uuid};
use reda_ui::keymap::{self, Chord};
use reda_ui::schematic_editor::{self, EditTarget, Interaction, Scale};
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";
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

fn click(shell: &mut Shell, p: (f64, f64)) -> Vec<String> {
    let modifiers = Modifiers::NONE;
    pointer(shell, &[Pointer::Click { at: p, modifiers }])
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

fn add(shell: &mut Shell, id: &str, params: Value, key: &str) -> Uuid {
    let out = shell.run(id, params).unwrap();
    out["result"][key].as_str().unwrap().parse().unwrap()
}

fn object(shell: &Shell, uuid: Uuid) -> Object {
    shell.engine().project().unwrap().object(uuid).unwrap()
}

fn select(shell: &mut Shell, objects: &[Uuid]) {
    shell
        .run("selection.set", json!({"objects": objects}))
        .unwrap();
}

/// Types `text` into the edit and presses Enter; returns the Commands it ran.
fn type_and_commit(shell: &mut Shell, text: &str) -> Vec<String> {
    schematic_editor::set_edit_text(shell, sheet(), text.to_owned());
    let commands = schematic_editor::commit_edit(shell, sheet());
    run_all(shell, commands)
}

fn interaction(shell: &Shell) -> Interaction {
    shell.document_tab(sheet()).unwrap().interaction.clone()
}

#[test]
fn f2_edits_names_text_designators_values_and_fields_in_place() {
    let dir = TempDir::new("in-place");
    let home = TempDir::new("in-place-home");
    let mut shell = shell(&dir, &home);
    let at0 = json!({"x": "0mil", "y": "2000mil"});
    let label = add(
        &mut shell,
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SDA", "at": at0}),
        "net_label",
    );
    select(&mut shell, &[label]);
    assert_eq!(press(&mut shell, "F2"), ["schematic.edit_in_place"]);
    let edit = schematic_editor::editing(&shell, sheet()).unwrap();
    assert_eq!(edit.target, EditTarget::NetLabel(label));
    assert_eq!(edit.text, "SDA");
    assert_eq!(
        type_and_commit(&mut shell, "SCL"),
        ["schematic.net_label.set"]
    );
    let Object::NetLabel(l) = object(&shell, label) else {
        panic!()
    };
    assert_eq!(l.object.name, "SCL");
    assert!(matches!(interaction(&shell), Interaction::Idle));

    let port = add(
        &mut shell,
        "schematic.port.add",
        json!({"document": SHEET, "name": "IN", "at": {"x": "1000mil", "y": "2000mil"}}),
        "port",
    );
    select(&mut shell, &[port]);
    press(&mut shell, "F2");
    assert_eq!(type_and_commit(&mut shell, "OUT"), ["schematic.port.set"]);
    let power = add(
        &mut shell,
        "schematic.power_port.add",
        json!({"document": SHEET, "name": "GND", "at": {"x": "2000mil", "y": "2000mil"}}),
        "power_port",
    );
    select(&mut shell, &[power]);
    press(&mut shell, "F2");
    assert_eq!(
        type_and_commit(&mut shell, "VCC"),
        ["schematic.power_port.set"]
    );
    let text = add(
        &mut shell,
        "schematic.text.add",
        json!({"document": SHEET, "text": "Note", "at": {"x": "0mil", "y": "3000mil"}}),
        "text",
    );
    select(&mut shell, &[text]);
    press(&mut shell, "F2");
    assert!(
        schematic_editor::editing(&shell, sheet())
            .unwrap()
            .multiline
    );
    assert_eq!(
        type_and_commit(&mut shell, "Two\nlines"),
        ["schematic.text.set"]
    );
    let Object::SheetText(t) = object(&shell, text) else {
        panic!()
    };
    assert_eq!(t.object.text, "Two\nlines");

    // A Unit edits its Designator, and a rename applies to every Unit.
    let u1a = shell
        .run(
            "schematic.component.place",
            json!({"document": SHEET, "part": LM358, "at": {"x": 0, "y": 0}}),
        )
        .unwrap()["result"]
        .clone();
    let u1b = add(
        &mut shell,
        "schematic.unit.place",
        json!({"document": SHEET, "component": u1a["component"]["uuid"],
               "at": {"x": "3000mil", "y": 0}}),
        "unit",
    );
    let u1a: Uuid = u1a["unit"].as_str().unwrap().parse().unwrap();
    select(&mut shell, &[u1a]);
    press(&mut shell, "F2");
    assert_eq!(
        type_and_commit(&mut shell, "U7"),
        ["schematic.component.set"]
    );
    for unit in [u1a, u1b] {
        let Object::Unit(u) = object(&shell, unit) else {
            panic!()
        };
        assert_eq!(u.object.designator, "U7");
    }
    // The value, a visible field, edited in place by its field.
    assert!(schematic_editor::begin_edit(
        &mut shell,
        EditTarget::Field {
            unit: u1a,
            field: "value".into()
        }
    ));
    assert_eq!(
        type_and_commit(&mut shell, "TL072"),
        ["schematic.component.set"]
    );
    let Object::Unit(u) = object(&shell, u1b) else {
        panic!()
    };
    assert_eq!(u.object.fields.value, "TL072");
}

#[test]
fn a_taken_designator_is_refused_inline_and_esc_cancels() {
    let dir = TempDir::new("in-place-taken");
    let home = TempDir::new("in-place-taken-home");
    let mut shell = shell(&dir, &home);
    let r1 = add(
        &mut shell,
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": {"x": 0, "y": 0}}),
        "unit",
    );
    add(
        &mut shell,
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": {"x": "1000mil", "y": 0}}),
        "unit",
    );
    select(&mut shell, &[r1]);
    press(&mut shell, "F2");
    assert!(type_and_commit(&mut shell, "R2").is_empty());
    let edit = schematic_editor::editing(&shell, sheet()).unwrap();
    assert!(edit.error.as_deref().unwrap().contains("R2"), "{edit:?}");
    // Esc drops the edit: nothing changed.
    schematic_editor::cancel_edit(&mut shell, sheet());
    assert!(matches!(interaction(&shell), Interaction::Idle));
    let Object::Unit(u) = object(&shell, r1) else {
        panic!()
    };
    assert_eq!(u.object.designator, "R1");

    // Off, F2 does nothing.
    shell
        .run(
            "app.preferences.set",
            json!({"key": "schematic.in_place_editing", "value": false}),
        )
        .unwrap();
    press(&mut shell, "F2");
    assert!(schematic_editor::editing(&shell, sheet()).is_none());
}

#[test]
fn a_slow_second_click_on_the_selection_edits_it_and_a_quick_one_does_not() {
    let dir = TempDir::new("slow-click");
    let home = TempDir::new("slow-click-home");
    let mut shell = shell(&dir, &home);
    let label = add(
        &mut shell,
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SDA", "at": {"x": "0mil", "y": "0mil"}}),
        "net_label",
    );
    shell.selection_picking_mut().set_time(10.0);
    click(&mut shell, at(40.0, 25.0));
    assert_eq!(shell.selection().objects, [label]);
    // Quick: a double-click's second click, not an edit.
    shell.selection_picking_mut().set_time(10.3);
    click(&mut shell, at(40.0, 25.0));
    assert!(schematic_editor::editing(&shell, sheet()).is_none());
    shell.selection_picking_mut().set_time(20.0);
    click(&mut shell, at(40.0, 25.0));
    shell.selection_picking_mut().set_time(21.0);
    click(&mut shell, at(40.0, 25.0));
    let edit = schematic_editor::editing(&shell, sheet()).unwrap();
    assert_eq!(edit.target, EditTarget::NetLabel(label));
}

#[test]
fn tab_while_placing_pauses_to_edit_the_name_and_placement_counts_up_from_it() {
    let dir = TempDir::new("paused");
    let home = TempDir::new("paused-home");
    let mut shell = shell(&dir, &home);
    pointer(&mut shell, &[Pointer::Hover(at(0.0, 0.0))]);
    shell.run("tool.activate:net_label", json!({})).unwrap();
    pointer(&mut shell, &[Pointer::Hover(at(0.0, 0.0))]);
    assert_eq!(press(&mut shell, "Tab"), ["tool.edit_properties"]);
    assert!(schematic_editor::paused(&shell, sheet()).is_some());
    // Paused: a click places nothing.
    assert!(click(&mut shell, at(0.0, 0.0)).is_empty());
    assert!(type_and_commit(&mut shell, "D09").is_empty());
    assert!(schematic_editor::paused(&shell, sheet()).is_none());
    click(&mut shell, at(0.0, 0.0));
    click(&mut shell, at(0.0, 500.0));
    // Esc while paused resumes without the edit.
    press(&mut shell, "Tab");
    schematic_editor::set_edit_text(&mut shell, sheet(), "X1".into());
    schematic_editor::cancel_edit(&mut shell, sheet());
    click(&mut shell, at(0.0, 1000.0));
    assert_eq!(shell.tool(), Tool::NetLabel);
    let project = shell.engine().project().unwrap();
    let mut names: Vec<String> = (project.schematic_sheet(sheet()).unwrap().net_labels())
        .map(|l| l.name.clone())
        .collect();
    names.sort();
    assert_eq!(names, ["D09", "D10", "D11"]);
}
