//! The Symbol editor's depth in the window, driven through AccessKit with
//! `egui_kittest`: the modal Symbol Wizard (from the Tools menu and from a Part editor),
//! Set Origin and Jump to Origin from the menus, and the question asked on the first
//! change to a Symbol that two Parts share.

use std::path::Path;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::model::{LibraryElement, Part, Symbol, Uuid};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const MINE: &str = "000000aa-0000-4000-8000-000000000000";

fn shell(home: &Path) -> Shell {
    let lib = home.join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("Mine.redalib"),
        format!("{{\"format\": 1, \"library\": {{\"uuid\": \"{MINE}\", \"name\": \"Mine\"}}}}"),
    )
    .unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&home.join("home"))))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .run(
            "app.preferences.set",
            json!({"key": "libraries.paths", "value": [lib]}),
        )
        .unwrap();
    shell
}

fn new_element(shell: &mut Shell, kind: &str, name: &str) -> Uuid {
    let out = shell
        .run(
            "library.element.new",
            json!({"library": MINE, "kind": kind, "name": name}),
        )
        .unwrap();
    out["element"].as_str().unwrap().parse().unwrap()
}

fn harness(shell: Shell) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size([1600.0, 1100.0])
        .with_max_steps(200)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn symbol(h: &Harness<'_, App>, uuid: Uuid) -> Symbol {
    match h.state().shell().engine().element(uuid) {
        Some(LibraryElement::Symbol(s)) => s.clone(),
        other => panic!("no open Symbol: {other:?}"),
    }
}

fn part(h: &Harness<'_, App>, uuid: Uuid) -> Part {
    match h.state().shell().engine().element(uuid) {
        Some(LibraryElement::Part(p)) => p.clone(),
        other => panic!("no open Part: {other:?}"),
    }
}

/// Clicks the last node labelled `label` (menus and dialogs are drawn last).
fn click(h: &mut Harness<'_, App>, label: &str) {
    h.get_all_by_label(label).last().unwrap().click();
    h.run_steps(8);
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    let out = h.state_mut().shell_mut().run(id, params).unwrap();
    h.run();
    out
}

fn journal_ids(h: &Harness<'_, App>) -> Vec<String> {
    (h.state().shell().engine().journal().iter())
        .filter(|e| e.outcome.is_ok())
        .map(|e| e.id.clone())
        .collect()
}

fn type_into(h: &mut Harness<'_, App>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run();
}

#[test]
fn the_tools_menu_opens_the_modal_wizard_which_lays_out_pasted_rows_and_reopens_them() {
    let home = TempDir::new("wizard-tools");
    let mut shell = shell(home.path());
    let uuid = new_element(&mut shell, "symbol", "Timer");
    let mut h = harness(shell);
    click(&mut h, "Tools");
    click(&mut h, "Symbol Wizard…");
    h.get_by_label("Symbol Wizard");
    let target = h.get_by_role_and_label(Role::ComboBox, "Make");
    assert_eq!(target.value().as_deref(), Some("This Unit, laid out again"));
    // The grid starts with eight rows; paste six in their place.
    h.get_all_by_role_and_label(Role::Button, "Paste Pins…")
        .last()
        .unwrap()
        .click();
    h.run();
    h.get_by_role_and_label(Role::MultilineTextInput, "Pasted text")
        .click();
    h.run();
    h.event(egui::Event::Paste(
        "1,GND,pwr\n2,TRIG,in\n3,OUT,out\n4,RESET,in\n5,CTRL,in\n6,VCC,power\n".into(),
    ));
    h.run();
    h.get_all_by_role_and_label(Role::Button, "Paste")
        .last()
        .unwrap()
        .click();
    h.run();
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Row 7 Number")
            .is_none()
    );
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Row 6 Number")
            .is_some()
    );
    h.get_by_role_and_label(Role::Image, "Preview");
    click(&mut h, "Make Symbol");
    assert!(
        h.query_by_label("Symbol Wizard").is_none(),
        "the wizard closed"
    );
    let s = symbol(&h, uuid);
    assert_eq!(s.pins.len(), 6);
    let pin1 = s.pins.iter().find(|p| p.number == "1").unwrap().clone();
    assert_eq!(pin1.name, "GND");
    assert_eq!(pin1.at, Point::new(Nm(0), Nm(0)), "Pin 1 is at the origin");
    // A dual in-line layout: three down the left, three up the right.
    assert!(s.pins.iter().filter(|p| p.at.x == Nm(0)).count() == 3);

    // Reopening reads the Pins back as the grid; a new name keeps the Pin's UUID.
    click(&mut h, "Tools");
    click(&mut h, "Symbol Wizard…");
    let row = h.get_by_role_and_label(Role::TextInput, "Row 1 Name");
    assert_eq!(row.value().as_deref(), Some("GND"));
    type_into(&mut h, "Row 1 Name", "VSS");
    click(&mut h, "Make Symbol");
    let s = symbol(&h, uuid);
    let again = s.pins.iter().find(|p| p.number == "1").unwrap();
    assert_eq!(again.name, "VSS");
    assert_eq!(again.uuid, pin1.uuid, "Pins keep their UUIDs by number");
    // Each run was one step.
    run(&mut h, "edit.undo", json!({"document": uuid}));
    assert_eq!(
        symbol(&h, uuid)
            .pins
            .iter()
            .find(|p| p.number == "1")
            .unwrap()
            .name,
        "GND"
    );
}

#[test]
fn from_wizard_in_a_part_editor_makes_a_new_symbol_for_that_part() {
    let home = TempDir::new("wizard-part");
    let mut shell = shell(home.path());
    let uuid = new_element(&mut shell, "part", "Opto");
    let mut h = harness(shell);
    click(&mut h, "From Wizard…");
    h.get_by_label("Symbol Wizard");
    let target = h.get_by_role_and_label(Role::ComboBox, "Make");
    assert_eq!(target.value().as_deref(), Some("A new Symbol"));
    type_into(&mut h, "Symbol name", "Opto_Symbol");
    click(&mut h, "Make Symbol");
    let made = part(&h, uuid).symbol.expect("the Part has the new Symbol");
    let s = symbol(&h, made);
    assert_eq!(s.name, "Opto_Symbol");
    assert_eq!(s.pins.len(), 8);
}

#[test]
fn set_origin_and_jump_to_origin_are_in_the_symbol_menus() {
    let home = TempDir::new("symbol-origin");
    let mut shell = shell(home.path());
    let uuid = new_element(&mut shell, "symbol", "Chip");
    let unit = match shell.engine().element(uuid) {
        Some(LibraryElement::Symbol(s)) => s.units[0].uuid,
        _ => unreachable!(),
    };
    for (n, y) in [("1", "200mil"), ("2", "100mil")] {
        shell
            .run(
                "library.symbol.pin.add",
                json!({"symbol": uuid, "unit": unit, "number": n, "name": n,
                       "at": {"x": "-300mil", "y": y}}),
            )
            .unwrap();
    }
    let mut h = harness(shell);
    click(&mut h, "Place");
    click(&mut h, "Set Origin");
    click(&mut h, "At Pin 1");
    let pin1 = |h: &Harness<'_, App>| {
        symbol(h, uuid)
            .pins
            .iter()
            .find(|p| p.number == "1")
            .unwrap()
            .at
    };
    assert_eq!(pin1(&h), Point::new(Nm(0), Nm(0)));
    click(&mut h, "Place");
    click(&mut h, "Set Origin");
    click(&mut h, "At Centre");
    // The centre of the box around both Pins, their lines 100 mil long included.
    assert_eq!(pin1(&h), Point::new(Nm::from_mil(-50), Nm::from_mil(50)));
    click(&mut h, "Place");
    click(&mut h, "Set Origin");
    click(&mut h, "Pick…");
    assert_eq!(journal_ids(&h).last().unwrap(), "tool.activate:origin");

    // Jump to Origin centres the canvas on the origin, keeping the zoom.
    h.state_mut()
        .shell_mut()
        .document_tab_mut(uuid)
        .unwrap()
        .camera = Some(reda_ui::canvas::Camera {
        center: (5_000_000.0, -3_000_000.0),
        nm_per_point: 20_000.0,
    });
    h.run();
    click(&mut h, "View");
    click(&mut h, "Jump to Origin");
    assert_eq!(journal_ids(&h).last().unwrap(), "view.jump_to");
    let camera = h
        .state()
        .shell()
        .document_tab(uuid)
        .unwrap()
        .camera
        .unwrap();
    assert_eq!(camera.center, (0.0, 0.0));
    assert_eq!(camera.nm_per_point, 20_000.0);
}

/// A saved Symbol "Shared" that the saved Parts "First" and "Second" both use.
fn shared_symbol(shell: &mut Shell) -> (Uuid, Uuid) {
    let symbol = new_element(shell, "symbol", "Shared");
    let unit = match shell.engine().element(symbol) {
        Some(LibraryElement::Symbol(s)) => s.units[0].uuid,
        _ => unreachable!(),
    };
    shell
        .run(
            "library.symbol.pin.add",
            json!({"symbol": symbol, "unit": unit, "number": "1", "name": "A",
                   "at": {"x": "-300mil", "y": "0mil"}}),
        )
        .unwrap();
    shell
        .run("library.element.save", json!({"element": symbol}))
        .unwrap();
    let mut first = None;
    for name in ["First", "Second"] {
        let part = new_element(shell, "part", name);
        shell
            .run(
                "library.part.symbol.set",
                json!({"part": part, "symbol": symbol}),
            )
            .unwrap();
        shell
            .run("library.element.save", json!({"element": part}))
            .unwrap();
        first.get_or_insert(part);
    }
    (symbol, first.unwrap())
}

#[test]
fn the_first_change_to_a_shared_symbol_asks_and_edit_shared_lets_it_through() {
    let home = TempDir::new("shared-edit");
    let mut shell = shell(home.path());
    let (uuid, _) = shared_symbol(&mut shell);
    shell
        .run("library.element.close", json!({"element": uuid}))
        .unwrap();
    // Opening it to look raises nothing.
    shell
        .run("library.element.open", json!({"element": uuid}))
        .unwrap();
    let mut h = harness(shell);
    assert!(h.query_by_label("Shared element").is_none());
    type_into(&mut h, "1 Name", "B");
    h.key_press(egui::Key::Enter);
    h.run();
    h.get_by_label("Shared element");
    h.get_by_role_and_label(Role::ListItem, "Mine / First");
    h.get_by_role_and_label(Role::ListItem, "Mine / Second");
    assert_eq!(symbol(&h, uuid).pins[0].name, "A", "the change waits");
    click(&mut h, "Cancel");
    assert!(h.query_by_label("Shared element").is_none());
    assert_eq!(symbol(&h, uuid).pins[0].name, "A", "Cancel drops it");

    type_into(&mut h, "1 Name", "B");
    h.key_press(egui::Key::Enter);
    h.run();
    click(&mut h, "Edit Shared");
    assert_eq!(symbol(&h, uuid).pins[0].name, "B");
    // Only the first change asks.
    type_into(&mut h, "1 Name", "C");
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.query_by_label("Shared element").is_none());
    assert_eq!(symbol(&h, uuid).pins[0].name, "C");
}

#[test]
fn make_a_copy_from_a_part_editor_changes_a_copy_and_repoints_that_part() {
    let home = TempDir::new("shared-copy");
    let mut shell = shell(home.path());
    let (uuid, first) = shared_symbol(&mut shell);
    shell
        .run("library.element.close", json!({"element": uuid}))
        .unwrap();
    shell
        .run("document.open", json!({"document": first}))
        .unwrap();
    let mut h = harness(shell);
    click(&mut h, "Edit Symbol");
    assert_eq!(h.state().shell().layout().active_document(), Some(uuid));
    type_into(&mut h, "1 Name", "B");
    h.key_press(egui::Key::Enter);
    h.run();
    click(&mut h, "Make a Copy");
    let copy = h.state().shell().layout().active_document().unwrap();
    assert_ne!(copy, uuid);
    let s = symbol(&h, copy);
    assert_eq!(s.name, "Shared_1");
    assert_eq!(s.pins[0].name, "B", "the change went to the copy");
    assert!(
        h.state().shell().engine().element(uuid).is_none(),
        "the original closed unchanged"
    );
    assert_eq!(
        part(&h, first).symbol,
        Some(copy),
        "the Part now uses the copy"
    );
}
