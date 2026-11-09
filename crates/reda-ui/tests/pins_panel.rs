//! The Pins Panel in the window, driven through AccessKit with `egui_kittest`: it follows
//! the open Symbol, pastes rows through the column-mapping paste, edits a cell on every
//! selected row at once (decorations and descriptions too), jumps to a Pin and sorts by
//! several columns.

use std::path::Path;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{InsideEdge, LibraryElement, OutsideEdge, PinType, Symbol, Uuid};
use reda_ui::pins::{self, PinColumn, Sort};
use reda_ui::shell::Shell;
use serde_json::json;

const MINE: &str = "000000aa-0000-4000-8000-000000000000";

/// A Shell with no Project open and the empty user Library "Mine" registered.
pub fn shell(home: &Path) -> Shell {
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

/// A new Symbol `name` with the Pins `(number, name)` down the left of its Unit.
fn new_symbol(shell: &mut Shell, name: &str, pins: &[(&str, &str)]) -> Uuid {
    let out = shell
        .run(
            "library.element.new",
            json!({"library": MINE, "kind": "symbol", "name": name}),
        )
        .unwrap();
    let symbol: Uuid = out["element"].as_str().unwrap().parse().unwrap();
    let unit = symbol_of(shell, symbol).units[0].uuid;
    for (i, (number, pin)) in pins.iter().enumerate() {
        shell
            .run(
                "library.symbol.pin.add",
                json!({"symbol": symbol, "unit": unit, "number": number, "name": pin,
                       "at": {"x": "-300mil", "y": format!("{}mil", -100 * i as i64)}}),
            )
            .unwrap();
    }
    symbol
}

fn symbol_of(shell: &Shell, symbol: Uuid) -> Symbol {
    match shell.engine().element(symbol) {
        Some(LibraryElement::Symbol(s)) => s.clone(),
        other => panic!("no open Symbol: {other:?}"),
    }
}

fn harness(shell: Shell) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size([1600.0, 1000.0])
        .with_max_steps(200)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn symbol(h: &Harness<'_, App>, uuid: Uuid) -> Symbol {
    symbol_of(h.state().shell(), uuid)
}

fn pin_by_number<'a>(s: &'a Symbol, number: &str) -> &'a reda_ui::engine::model::Pin {
    s.pins.iter().find(|p| p.number == number).unwrap()
}

/// Opens the combo box `label` and picks `option`.
fn pick(h: &mut Harness<'_, App>, label: &str, option: &str) {
    h.get_by_role_and_label(Role::ComboBox, label).click();
    h.run();
    // The popup is drawn last.
    h.get_all_by_label(option).last().unwrap().click();
    h.run();
}

#[test]
fn a_new_symbol_brings_up_the_pins_panel_and_pastes_rows_through_the_column_mapping() {
    let home = TempDir::new("pins-paste");
    let mut shell = shell(home.path());
    let uuid = new_symbol(&mut shell, "MCU", &[]);
    let mut h = harness(shell);
    h.get_by_label("Pins of MCU");
    h.get_by_role_and_label(Role::Button, "Paste Pins…").click();
    h.run();
    h.get_by_role_and_label(Role::MultilineTextInput, "Pasted text")
        .click();
    h.run();
    h.event(egui::Event::Paste(
        "Pin\tSignal\tDir\tNotes\n1\tVCC\tPWR\tsupply\n2\tGND\tpower\tground\n3\tPA0\tI/O\tport\n"
            .into(),
    ));
    h.run();
    // The header row was found and each heading mapped; the Notes column is not used.
    h.get_by_label("First row is headings");
    // Remove Row 3 (the GND row: the header is row 1) and the Notes column.
    h.get_by_role_and_label(Role::Button, "Remove Row 3")
        .click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Remove Column 4")
        .click();
    h.run();
    assert!(
        h.query_by_role_and_label(Role::Button, "Remove Column 4")
            .is_none()
    );
    // Column 2 holds names; map it to the number instead and back.
    pick(&mut h, "Column 2", "Group");
    pick(&mut h, "Column 2", "Name");
    h.get_all_by_role_and_label(Role::Button, "Paste")
        .last()
        .unwrap()
        .click();
    h.run();
    let s = symbol(&h, uuid);
    let mut got: Vec<(String, String, String)> = s
        .pins
        .iter()
        .map(|p| {
            (
                p.number.clone(),
                p.name.clone(),
                format!("{:?}", p.electrical_type),
            )
        })
        .collect();
    got.sort();
    assert_eq!(
        got,
        [
            ("1".into(), "VCC".into(), "PowerIn".into()),
            ("3".into(), "PA0".into(), "Bidirectional".into()),
        ],
        "the aliases PWR and I/O are read, and the removed row is left out"
    );
    assert!(
        h.query_by_label("Pasted text").is_none(),
        "the sub-dialog closed"
    );
    // The paste was one step.
    h.state_mut()
        .shell_mut()
        .run("edit.undo", json!({"document": uuid}))
        .unwrap();
    h.run();
    assert!(symbol(&h, uuid).pins.is_empty());
}

#[test]
fn a_cell_edit_on_a_selected_row_changes_every_selected_row_in_one_step() {
    let home = TempDir::new("pins-multi");
    let mut shell = shell(home.path());
    let uuid = new_symbol(&mut shell, "Gate", &[("1", "A"), ("2", "B"), ("3", "Y")]);
    let mut h = harness(shell);
    h.get_by_role_and_label(Role::CheckBox, "Select Pin 1")
        .click();
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Select Pin 2")
        .click();
    h.run();
    pick(&mut h, "2 Type", "Input");
    let s = symbol(&h, uuid);
    assert_eq!(pin_by_number(&s, "1").electrical_type, PinType::Input);
    assert_eq!(pin_by_number(&s, "2").electrical_type, PinType::Input);
    assert_eq!(
        pin_by_number(&s, "3").electrical_type,
        PinType::Passive,
        "not selected"
    );
    pick(&mut h, "1 Inside edge", "Clock");
    let s = symbol(&h, uuid);
    assert_eq!(pin_by_number(&s, "1").inside_edge, Some(InsideEdge::Clock));
    assert_eq!(pin_by_number(&s, "2").inside_edge, Some(InsideEdge::Clock));
    h.state_mut()
        .shell_mut()
        .run("edit.undo", json!({"document": uuid}))
        .unwrap();
    h.run();
    let s = symbol(&h, uuid);
    assert_eq!(pin_by_number(&s, "1").inside_edge, None, "one undo step");
    assert_eq!(pin_by_number(&s, "2").inside_edge, None);
    assert_eq!(pin_by_number(&s, "2").electrical_type, PinType::Input);

    // An unselected row's edit is its own: the decoration and the description.
    pick(&mut h, "3 Outside edge", "Inversion dot");
    h.get_by_role_and_label(Role::TextInput, "3 Description")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "3 Description")
        .type_text("Output, active low");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    let s = symbol(&h, uuid);
    assert_eq!(pin_by_number(&s, "3").outside_edge, Some(OutsideEdge::Dot));
    assert_eq!(pin_by_number(&s, "3").description, "Output, active low");
    assert_eq!(pin_by_number(&s, "1").outside_edge, None);
    assert!(pin_by_number(&s, "1").description.is_empty());

    // Jump to Pin selects it alone.
    h.get_by_role_and_label(Role::Button, "Jump to Pin 3")
        .click();
    h.run();
    let pin3 = pin_by_number(&s, "3").uuid;
    assert_eq!(h.state().shell().selection().objects, [pin3]);
}

#[test]
fn headings_sort_by_several_columns() {
    let home = TempDir::new("pins-sort");
    let mut shell = shell(home.path());
    let uuid = new_symbol(
        &mut shell,
        "Sorted",
        &[("1", "VCC"), ("2", "B"), ("10", "A"), ("3", "GND")],
    );
    for (number, kind) in [("1", "power_in"), ("3", "power_in")] {
        let pin = pin_by_number(&symbol_of(&shell, uuid), number).uuid;
        shell
            .run(
                "library.symbol.pin.edit",
                json!({"symbol": uuid, "pin": pin, "electrical_type": kind}),
            )
            .unwrap();
    }
    let s = symbol_of(&shell, uuid);
    let numbers = |sort: &Sort| -> Vec<String> {
        pins::sorted(&s, sort)
            .iter()
            .map(|p| p.number.clone())
            .collect()
    };
    let mut sort = Sort::default();
    assert_eq!(
        numbers(&sort),
        ["1", "2", "3", "10"],
        "by number, naturally"
    );
    sort.click(PinColumn::Name, false);
    assert_eq!(numbers(&sort), ["10", "2", "3", "1"]);
    sort.click(PinColumn::Name, false);
    assert_eq!(
        numbers(&sort),
        ["1", "3", "2", "10"],
        "a second click turns it round"
    );
    // Type first (passive before power in), then name within each type.
    sort.click(PinColumn::Type, false);
    sort.click(PinColumn::Name, true);
    assert_eq!(numbers(&sort), ["10", "2", "3", "1"]);
    // Shift+click on a key turns it round, leaving the first key leading.
    sort.click(PinColumn::Name, true);
    assert_eq!(numbers(&sort), ["2", "10", "1", "3"]);
    assert_eq!(sort.rank(PinColumn::Name), Some((1, false)));

    // The heading buttons drive the same sort.
    let mut h = harness(shell);
    h.get_by_role_and_label(Role::Button, "Sort by Name")
        .click();
    h.run();
    assert_eq!(
        pins::state(h.state().shell()).sort.keys,
        [(PinColumn::Name, true)]
    );
}
