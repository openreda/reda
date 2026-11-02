//! The library workflow in the window, driven through AccessKit with `egui_kittest`: the
//! Part editor tab (pickers, links into the Symbol and Footprint editors, fields), Import
//! part with its one-time notice, and a Footprint generator's form with its preview, both
//! reached from the Library panel's New… menu.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{LibraryElement, Part, Uuid};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const MINE: &str = "000000aa-0000-4000-8000-000000000000";
/// The starter Library's Resistor, its Symbol and its first Footprint.
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const RESISTOR_SYMBOL: &str = "06c71621-e7e4-49a4-9b90-88485bb936b5";
const RESC1005: &str = "cece716c-4fd0-42c8-900c-0f3bc3c14225";

fn starter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reda-import-kicad/tests/fixtures")
        .join(name)
}

/// The window on a new Project with its sheet open and the Library panel shown; the
/// starter Library is registered, and an empty user Library "Mine".
fn harness(home: &TempDir, work: &TempDir) -> Harness<'static, App> {
    let lib = home.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("Mine.redalib"),
        format!("{{\"format\": 1, \"library\": {{\"uuid\": \"{MINE}\", \"name\": \"Mine\"}}}}"),
    )
    .unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&home.path().join("home"))))
        .starter_library(Some(starter()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .run(
            "app.preferences.set",
            json!({"key": "libraries.paths", "value": [lib]}),
        )
        .unwrap();
    shell.new_project(work.path(), "Demo").unwrap();
    shell
        .run("document.open", json!({"document": SHEET}))
        .unwrap();
    shell
        .run("panel.show", json!({"panel": "library"}))
        .unwrap();
    let mut h = Harness::builder()
        .with_size([1400.0, 1000.0])
        .with_max_steps(200)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

/// The successful Journal entries from `from` on, as `(id, params)`.
fn entries_since(h: &Harness<'_, App>, from: usize) -> Vec<(String, Value)> {
    h.state().shell().engine().journal()[from..]
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| (e.id.clone(), e.params.clone()))
        .collect()
}

fn journal_len(h: &Harness<'_, App>) -> usize {
    h.state().shell().engine().journal().len()
}

fn part(h: &Harness<'_, App>, uuid: Uuid) -> Part {
    match h.state().shell().engine().element(uuid) {
        Some(LibraryElement::Part(p)) => p.clone(),
        other => panic!("no open Part: {other:?}"),
    }
}

/// Opens the Library panel's New… menu and clicks `item`.
fn new_menu(h: &mut Harness<'_, App>, item: &str) {
    h.get_by_role_and_label(Role::Button, "New…").click();
    h.run();
    h.get_by_label(item).click();
    h.run();
}

/// Types `text` into the open picker's filter.
fn filter(h: &mut Harness<'_, App>, text: &str) {
    h.get_by_role_and_label(Role::TextInput, "Filter").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Filter")
        .type_text(text);
    h.run();
}

/// Types `text` into the text field `label` and presses Enter.
fn type_into(h: &mut Harness<'_, App>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
}

#[test]
fn edit_part_opens_the_part_editor_which_links_to_its_symbol_and_footprints() {
    let (home, work) = (TempDir::new("wf-edit-home"), TempDir::new("wf-edit"));
    let mut h = harness(&home, &work);
    h.get_by_role_and_label(Role::TextInput, "Search Parts")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Search Parts")
        .type_text("Resistor");
    h.run();
    h.get_by_role_and_label(Role::ListItem, "Resistor").click();
    h.run();
    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Edit Part").click();
    h.run();
    let ids: Vec<_> = entries_since(&h, from);
    assert_eq!(
        ids[0],
        (
            "library.element.open".to_owned(),
            json!({"element": RESISTOR})
        )
    );
    let resistor: Uuid = RESISTOR.parse().unwrap();
    assert_eq!(h.state().shell().layout().active_document(), Some(resistor));
    // The starter Part reads read-only; its Symbol and Footprints are named and linked.
    h.get_by_label("Read-only: this Part is in the starter Library.");
    h.get_by_role_and_label(Role::ComboBox, "Symbol");
    h.get_by_label("RESC1005X40N (default)");

    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Edit RESC1005X40N")
        .click();
    h.run();
    assert_eq!(
        entries_since(&h, from)[0],
        (
            "library.element.open".to_owned(),
            json!({"element": RESC1005})
        )
    );
    // Back on the Part's tab, Edit Symbol opens its Symbol.
    h.state_mut()
        .shell_mut()
        .run("document.open", json!({"document": RESISTOR}))
        .unwrap();
    h.run();
    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Edit Symbol").click();
    h.run();
    assert_eq!(
        entries_since(&h, from)[0],
        (
            "library.element.open".to_owned(),
            json!({"element": RESISTOR_SYMBOL})
        )
    );
}

#[test]
fn a_new_part_gets_its_symbol_footprint_and_fields_from_its_form() {
    let (home, work) = (TempDir::new("wf-new-home"), TempDir::new("wf-new"));
    let mut h = harness(&home, &work);
    new_menu(&mut h, "New Part…");
    // library.element.new lacks the Library and the name: its params form asks for them.
    h.get_by_label("New Library Element");
    h.key_press(egui::Key::Escape);
    h.run();
    let out = h
        .state_mut()
        .shell_mut()
        .run(
            "library.element.new",
            json!({"library": MINE, "kind": "part", "name": "Divider"}),
        )
        .unwrap();
    let uuid: Uuid = out["element"].as_str().unwrap().parse().unwrap();
    h.run();
    h.get_by_label("A Standard Part needs a Symbol.");

    h.get_by_role_and_label(Role::ComboBox, "Symbol").click();
    h.run();
    filter(&mut h, "Resistor");
    h.get_by_label("Starter / Resistor").click();
    h.run();
    assert_eq!(
        part(&h, uuid).symbol,
        Some(RESISTOR_SYMBOL.parse().unwrap())
    );

    h.get_by_role_and_label(Role::ComboBox, "Add Footprint…")
        .click();
    h.run();
    filter(&mut h, "RESC1005");
    h.get_by_label("Starter / RESC1005X40N").click();
    h.run();
    assert_eq!(part(&h, uuid).footprints, [RESC1005.parse().unwrap()]);

    // Pin 2 of the resistor goes to Pads 2 and 3: an override.
    type_into(&mut h, "Pads of Pin 2", ", 3");
    let p = part(&h, uuid);
    assert_eq!(p.pin_map.pins.len(), 1);
    assert_eq!(p.pin_map.pins[0].pads, ["2", "3"]);
    h.get_by_label("overridden");

    type_into(&mut h, "MPN", "RC0402");
    assert_eq!(part(&h, uuid).fields.mpn, "RC0402");
    h.get_by_role_and_label(Role::Button, "Add Field").click();
    h.run();
    assert_eq!(part(&h, uuid).fields.custom[0].name, "Field 1");
    // Every edit is on the Part's own undo stack.
    h.state_mut()
        .shell_mut()
        .run("edit.undo", json!({"document": uuid}))
        .unwrap();
    assert!(part(&h, uuid).fields.custom.is_empty());
}

#[test]
fn import_part_from_the_library_panel_reports_its_check_and_shows_the_notice_once() {
    let (home, work) = (TempDir::new("wf-import-home"), TempDir::new("wf-import"));
    let mut h = harness(&home, &work);
    new_menu(&mut h, "Import Part…");
    h.get_by_label("Import Part");
    // Nothing to import until both files are given; the Library starts on "Mine".
    assert!(
        h.get_by_role_and_label(Role::Button, "Import")
            .accesskit_node()
            .is_disabled()
    );
    for (label, file) in [
        ("Symbol file", "divider.kicad_sym"),
        ("Footprint file", "divider.kicad_mod"),
    ] {
        h.get_by_role_and_label(Role::TextInput, label).click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, label)
            .type_text(&fixture(file).display().to_string());
        h.run();
    }
    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Import").click();
    h.run();
    let entries = entries_since(&h, from);
    assert_eq!(entries[0].0, "library.import_part");
    assert_eq!(entries[0].1["library"], MINE);
    assert!(
        h.query_by_label("Import Part").is_none(),
        "the dialog closed"
    );
    // The one-time notice about vendor terms.
    h.get_by_label("Vendor Parts");
    h.get_by_role_and_label(Role::Button, "OK").click();
    h.run();
    assert!(h.query_by_label("Vendor Parts").is_none());
    // The imported Part is in the Library now.
    let index = h.state_mut().shell_mut().library_index();
    let mine = index.library(MINE.parse().unwrap()).unwrap();
    assert_eq!(mine.parts.len(), 1);
    assert_eq!(mine.symbols.len(), 1);
    assert_eq!(mine.footprints.len(), 1);
}

#[test]
fn a_generator_form_previews_the_land_pattern_before_writing_it() {
    let (home, work) = (TempDir::new("wf-gen-home"), TempDir::new("wf-gen"));
    let mut h = harness(&home, &work);
    h.get_by_role_and_label(Role::Button, "New…").click();
    h.run();
    h.get_by_label("Generate Footprint").click();
    h.run();
    h.get_by_label("Generate Chip Footprint…").click();
    h.run();
    // Neither a size nor dimensions yet: no land pattern.
    assert!(h.query_by_label("RESC1608X55N").is_none());
    // A standard size is one of its choices.
    h.get_by_role_and_label(Role::ComboBox, "Size (optional)")
        .click();
    h.run();
    h.get_by_label("0603").click();
    h.run();
    h.get_by_label("RESC1608X55N");
    h.get_by_label("2 Pads");
    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Generate").click();
    h.run();
    let entries = entries_since(&h, from);
    assert_eq!(entries[0].0, "library.footprint.generate.chip");
    assert_eq!(entries[0].1["library"], MINE);
    assert_eq!(
        h.state().shell().note(),
        Some("Library check: no problems in RESC1608X55N")
    );
    let file = home.path().join("lib/footprints/RESC1608X55N.redafp");
    assert!(file.exists());
}
