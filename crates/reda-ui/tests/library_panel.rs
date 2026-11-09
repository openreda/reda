//! The Library panel in the window, driven through AccessKit with `egui_kittest`: what it
//! lists on a sheet and on the board, search (with `field:value` and `*` tokens) and
//! filters, its Library mode beside a Library editor tab, the previews, its buttons and
//! row menu (Where Used, Copy to Library…, Duplicate, Check, Update in Project…), Where
//! Used and "Used in Project: N" in the details, Library Status per row, the Active
//! checkboxes, Make Project Library, and Extract offered by Edit in Library on Source
//! Missing.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const MINE: &str = "000000aa-0000-4000-8000-000000000000";
/// The starter Library's Resistor and MountingHole_M3.
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const RESISTOR_SYMBOL: &str = "06c71621-e7e4-49a4-9b90-88485bb936b5";

fn starter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

fn harness(home: &TempDir, work: &TempDir) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(starter()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
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

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    h.state_mut().shell_mut().run(id, params).unwrap()
}

/// Types `text` into the search box (after what is there).
fn search(h: &mut Harness<'_, App>, text: &str) {
    h.get_by_role_and_label(Role::TextInput, "Search Parts")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Search Parts")
        .type_text(text);
    h.run();
}

/// Searches for `name` and clicks its row.
fn choose(h: &mut Harness<'_, App>, name: &str) {
    search(h, name);
    h.get_by_role_and_label(Role::ListItem, name).click();
    h.run();
}

fn listed(h: &Harness<'_, App>, name: &str) -> bool {
    h.query_by_role_and_label(Role::ListItem, name).is_some()
}

/// The last successful Journal entry, as `(id, params)`.
fn last_entry(h: &Harness<'_, App>) -> (String, Value) {
    let entry = h
        .state()
        .shell()
        .engine()
        .journal()
        .iter()
        .rfind(|e| e.outcome.is_ok())
        .unwrap();
    (entry.id.clone(), entry.params.clone())
}

#[test]
fn a_sheet_lists_parts_with_a_symbol_and_the_board_mechanical_parts() {
    let (home, work) = (TempDir::new("lib-panel-home"), TempDir::new("lib-panel"));
    let mut h = harness(&home, &work);
    assert!(listed(&h, "Resistor"));
    assert!(!listed(&h, "MountingHole_M3"));

    let pcb = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .uuid;
    run(&mut h, "document.open", json!({"document": pcb}));
    h.run();
    assert!(listed(&h, "MountingHole_M3"));
    assert!(!listed(&h, "Resistor"));
}

#[test]
fn search_and_the_component_type_filter_narrow_the_list() {
    let (home, work) = (TempDir::new("lib-search-home"), TempDir::new("lib-search"));
    let mut h = harness(&home, &work);
    search(&mut h, "capac");
    assert!(listed(&h, "Capacitor"));
    assert!(!listed(&h, "Resistor"));

    search(&mut h, "itor_pol");
    assert!(listed(&h, "Capacitor_Polarised"));
    assert!(!listed(&h, "Capacitor"));

    h.get_by_role_and_label(Role::ComboBox, "Component Type")
        .click();
    h.run();
    h.get_by_label("Mechanical").click();
    h.run();
    assert!(!listed(&h, "Capacitor_Polarised"));
    h.get_by_label("No Parts match.");
}

#[test]
fn clicking_a_part_previews_it_and_starts_placing_it() {
    let (home, work) = (TempDir::new("lib-click-home"), TempDir::new("lib-click"));
    let mut h = harness(&home, &work);
    choose(&mut h, "OpAmp_Dual");

    assert_eq!(h.state().shell().tool(), Tool::Place);
    let (id, params) = last_entry(&h);
    assert_eq!(id, "tool.place");
    assert_eq!(params["document"], SHEET);
    h.get_by_role_and_label(Role::Image, "Symbol preview");
    h.get_by_role_and_label(Role::Image, "Footprint preview");

    // Units are tabs; choosing one places that Unit.
    h.get_by_role_and_label(Role::Tab, "Unit B").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "tool.place");
    assert_eq!(params["unit"], "B");
}

#[test]
fn the_footprint_picker_shows_the_alternates() {
    let (home, work) = (TempDir::new("lib-fp-home"), TempDir::new("lib-fp"));
    let mut h = harness(&home, &work);
    choose(&mut h, "Resistor");
    h.get_by_role_and_label(Role::ComboBox, "Alternate Footprint")
        .click();
    h.run();
    h.get_by_label("RESC1608X55N").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "tool.place");
    assert_eq!(params["part"], RESISTOR);
    assert_eq!(params["footprint"], "0691e5e7-9eb6-49f3-9f3e-d4159258693e");
}

#[test]
fn edit_opens_the_part_and_new_asks_for_the_new_parts_name() {
    let (home, work) = (TempDir::new("lib-edit-home"), TempDir::new("lib-edit"));
    let mut h = harness(&home, &work);
    choose(&mut h, "Resistor");
    h.get_by_role_and_label(Role::Button, "Edit Part").click();
    h.run();
    // The Part opens, then its Part editor tab, which links to its Symbol.
    let journal = h.state().shell().engine().journal();
    let entry = journal[journal.len() - 2].clone();
    assert_eq!(entry.id, "library.element.open");
    assert_eq!(entry.params, json!({"element": RESISTOR}));
    assert_eq!(journal.last().unwrap().id, "document.open");
    let part = RESISTOR.parse().unwrap();
    assert_eq!(h.state().shell().layout().active_document(), Some(part));
    h.get_by_role_and_label(Role::Button, "Edit Symbol").click();
    h.run();
    let journal = h.state().shell().engine().journal();
    let entry = journal[journal.len() - 2].clone();
    assert_eq!(entry.params, json!({"element": RESISTOR_SYMBOL}));

    h.get_by_role_and_label(Role::Button, "New…").click();
    h.run();
    h.get_by_label("New Part…").click();
    h.run();
    // library.element.new lacks the Library and name: its params form asks for them.
    h.get_by_label("New Library Element");
    let last = h
        .state()
        .shell()
        .engine()
        .journal()
        .last()
        .unwrap()
        .id
        .clone();
    assert_eq!(last, "document.open", "nothing ran yet");
}

/// [`harness`] with an empty user Library "Mine" registered too.
fn harness_with_mine(home: &TempDir, work: &TempDir) -> Harness<'static, App> {
    let lib = mine(home);
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("Mine.redalib"),
        format!("{{\"format\": 1, \"library\": {{\"uuid\": \"{MINE}\", \"name\": \"Mine\"}}}}"),
    )
    .unwrap();
    let mut h = harness(home, work);
    run(
        &mut h,
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": [lib]}),
    );
    h.run();
    h
}

/// The folder of the user Library "Mine" under `home`.
fn mine(home: &TempDir) -> PathBuf {
    home.path().join("lib")
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

/// The labels of the panel's rows.
fn rows(h: &Harness<'_, App>) -> Vec<String> {
    h.query_all_by_role(Role::ListItem)
        .filter_map(|n| n.accesskit_node().label())
        .collect()
}

/// Replaces the search text with `text`.
fn search_for(h: &mut Harness<'_, App>, text: &str) {
    h.state_mut().shell_mut().library_panel_mut().text.clear();
    h.run();
    search(h, text);
}

/// Places the Part `part` on the sheet at `x` mil.
fn place(h: &mut Harness<'_, App>, part: &str, x: i64) -> Value {
    let at = json!({"x": format!("{x}mil"), "y": "2000mil"});
    let out = run(
        h,
        "schematic.component.place",
        json!({"document": SHEET, "part": part, "at": at}),
    );
    h.run();
    out
}

/// Right-clicks the row `row` and clicks `item` in its menu: of the buttons so labelled,
/// the one nearest the row (the details may have one too).
fn row_menu(h: &mut Harness<'_, App>, row: &str, item: &str) {
    let at = h.get_by_role_and_label(Role::ListItem, row).rect().center();
    h.get_by_role_and_label(Role::ListItem, row)
        .click_secondary();
    h.run();
    let distance = |r: egui::Rect| r.center().distance(at);
    h.get_all_by_role_and_label(Role::Button, item)
        .min_by(|a, b| distance(a.rect()).total_cmp(&distance(b.rect())))
        .expect("the menu item")
        .click();
    h.run();
}

#[test]
fn field_value_and_wildcard_tokens_narrow_the_rows() {
    let (home, work) = (TempDir::new("lib-tokens-home"), TempDir::new("lib-tokens"));
    let mut h = harness(&home, &work);
    search_for(&mut h, "mpn:STM32*");
    assert_eq!(rows(&h), ["STM32F042F6P6"]);
    // A field the Part lacks matches nothing, however loose the pattern.
    search_for(&mut h, "lcsc:*");
    assert!(rows(&h).is_empty(), "{:?}", rows(&h));
    search_for(&mut h, "Sock*1x1");
    assert_eq!(rows(&h), ["Socket_1x10"]);
}

#[test]
fn beside_a_library_editor_tab_it_lists_that_librarys_symbols_footprints_and_parts() {
    let (home, work) = (TempDir::new("lib-mode-home"), TempDir::new("lib-mode"));
    let mut h = harness(&home, &work);
    run(
        &mut h,
        "library.element.open",
        json!({"element": RESISTOR_SYMBOL}),
    );
    run(
        &mut h,
        "document.open",
        json!({"document": RESISTOR_SYMBOL}),
    );
    h.run();
    search_for(&mut h, "Resistor");
    assert_eq!(rows(&h), ["Symbol: Resistor", "Part: Resistor"]);
    // A field token reaches only the Parts; `name:` reaches every element.
    search_for(&mut h, "name:RESC1005*");
    assert_eq!(rows(&h), ["Footprint: RESC1005X40N"]);
    // Choosing a row and Edit opens it in its editor.
    h.get_by_role_and_label(Role::ListItem, "Footprint: RESC1005X40N")
        .click();
    h.run();
    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Edit Element")
        .click();
    h.run();
    let opened = entries_since(&h, from);
    assert_eq!(opened[0].0, "library.element.open", "{opened:?}");
    // Back on the sheet it lists Parts to place again.
    run(&mut h, "document.open", json!({"document": SHEET}));
    h.run();
    search_for(&mut h, "Resistor");
    assert_eq!(rows(&h), ["Resistor"]);
}

/// The Parts called `name` the registered Libraries hold, as (Library, Part) UUIDs.
fn parts_named(h: &mut Harness<'_, App>, name: &str) -> Vec<(String, String)> {
    let found = run(
        h,
        "library.search",
        json!({ "text": format!("name:{name}") }),
    );
    (found["parts"].as_array().unwrap().iter())
        .filter(|p| p["name"] == name)
        .map(|p| {
            let library = (p["library"].as_str().or(p["library"]["uuid"].as_str()))
                .unwrap_or_default()
                .to_owned();
            (library, p["uuid"].as_str().unwrap().to_owned())
        })
        .collect()
}

/// The labels of the buttons whose label starts with `prefix`.
fn buttons(h: &Harness<'_, App>, prefix: &str) -> Vec<String> {
    h.query_all_by_role(Role::Button)
        .filter_map(|n| n.accesskit_node().label())
        .filter(|l| l.starts_with(prefix))
        .collect()
}

#[test]
fn where_used_is_in_the_row_menu_and_the_details_with_used_in_project() {
    let (home, work) = (TempDir::new("lib-used-home"), TempDir::new("lib-used"));
    let mut h = harness(&home, &work);
    place(&mut h, RESISTOR, 1000);
    place(&mut h, RESISTOR, 2000);
    choose(&mut h, "Resistor");
    h.key_press(egui::Key::Escape);
    h.run();
    h.get_by_label("Used in Project: 2");

    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Where Used")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::Button, "Where Used").click();
    h.run();
    assert_eq!(
        entries_since(&h, from)[0],
        (
            "library.where_used".to_owned(),
            json!({"element": RESISTOR})
        )
    );
    h.get_by_label("Placed as R1, R2");

    // The row menu runs it too; for a Symbol it lists the Parts that use it.
    run(
        &mut h,
        "library.element.open",
        json!({"element": RESISTOR_SYMBOL}),
    );
    run(
        &mut h,
        "document.open",
        json!({"document": RESISTOR_SYMBOL}),
    );
    search_for(&mut h, "Resistor");
    let from = journal_len(&h);
    row_menu(&mut h, "Symbol: Resistor", "Where Used");
    assert_eq!(
        entries_since(&h, from)[0],
        (
            "library.where_used".to_owned(),
            json!({"element": RESISTOR_SYMBOL})
        )
    );
    h.get_by_label("Part Resistor in Starter");
    h.get_by_label("Placed as R1, R2");
}

#[test]
fn each_part_row_shows_its_library_status_over_the_project() {
    let (home, work) = (TempDir::new("lib-status-home"), TempDir::new("lib-status"));
    let mut h = harness_with_mine(&home, &work);
    place(&mut h, RESISTOR, 1000);
    place(&mut h, RESISTOR, 2000);
    search_for(&mut h, "Resistor");
    assert_eq!(
        buttons(&h, "Library Status"),
        ["Library Status of Resistor: 2 placed"]
    );

    // A copy in Mine, placed once, then changed on disk: its row says it differs.
    run(
        &mut h,
        "library.element.copy",
        json!({"element": RESISTOR, "library": MINE}),
    );
    let copy = parts_named(&mut h, "Resistor")
        .into_iter()
        .find(|(l, _)| l == MINE)
        .unwrap()
        .1;
    place(&mut h, &copy, 3000);
    let file = mine(&home).join("parts/Resistor.redapart");
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(
        &file,
        text.replace("\"description\":\"", "\"description\":\"Changed. "),
    )
    .unwrap();
    h.get_by_role_and_label(Role::Button, "Refresh").click();
    h.run();
    let mut shown = buttons(&h, "Library Status");
    shown.sort();
    assert_eq!(
        shown,
        [
            "Library Status of Resistor: 1 placed, 1 differs",
            "Library Status of Resistor: 2 placed",
        ]
    );
    // Clicking it opens Update from Library.
    let from = journal_len(&h);
    h.get_by_role_and_label(
        Role::Button,
        "Library Status of Resistor: 1 placed, 1 differs",
    )
    .click();
    h.run();
    assert_eq!(
        entries_since(&h, from)[0],
        (
            "schematic.update_from_library.open".to_owned(),
            json!({"parts": [copy]})
        )
    );
}

#[test]
fn an_inactive_library_leaves_the_panel() {
    let (home, work) = (TempDir::new("lib-active-home"), TempDir::new("lib-active"));
    let mut h = harness_with_mine(&home, &work);
    run(
        &mut h,
        "library.element.copy",
        json!({"element": RESISTOR, "library": MINE}),
    );
    h.run();
    search_for(&mut h, "Resistor");
    assert_eq!(rows(&h), ["Resistor", "Resistor"]);
    let path = mine(&home).to_string_lossy().into_owned();
    run(
        &mut h,
        "app.preferences.set",
        json!({"key": "libraries.inactive", "value": [path]}),
    );
    h.run();
    assert_eq!(rows(&h), ["Resistor"]);
    h.get_by_role_and_label(Role::ComboBox, "Library").click();
    h.run();
    assert!(h.query_by_label("Mine").is_none());
}

#[test]
fn copy_to_library_asks_for_the_library_and_duplicate_copies_beside_it() {
    let (home, work) = (TempDir::new("lib-copy-home"), TempDir::new("lib-copy"));
    let mut h = harness_with_mine(&home, &work);
    search_for(&mut h, "Resistor");
    // The starter Library can't be written to, so it offers no Duplicate.
    h.get_by_role_and_label(Role::ListItem, "Resistor")
        .click_secondary();
    h.run();
    assert!(h.get_by_label("Duplicate").accesskit_node().is_disabled());
    h.get_by_label("Copy to Library…").click();
    h.run();
    h.get_by_label("Copy to Library");
    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Copy").click();
    h.run();
    assert_eq!(
        entries_since(&h, from)[0],
        (
            "library.element.copy".to_owned(),
            json!({"element": RESISTOR, "library": MINE})
        )
    );
    assert_eq!(rows(&h), ["Resistor", "Resistor"]);

    // Mine's copy can be duplicated in Mine, under a free name.
    h.get_by_role_and_label(Role::ComboBox, "Library").click();
    h.run();
    h.get_by_label("Mine").click();
    h.run();
    row_menu(&mut h, "Resistor", "Duplicate");
    search_for(&mut h, "Resistor");
    assert_eq!(rows(&h), ["Resistor", "Resistor_1"]);
    assert!(mine(&home).join("parts/Resistor_1.redapart").exists());
}

#[test]
fn check_in_the_row_menu_reports_into_messages() {
    let (home, work) = (TempDir::new("lib-check-home"), TempDir::new("lib-check"));
    let mut h = harness(&home, &work);
    search_for(&mut h, "Resistor");
    let from = journal_len(&h);
    row_menu(&mut h, "Resistor", "Check");
    assert_eq!(
        entries_since(&h, from)[0],
        ("library.check".to_owned(), json!({"element": RESISTOR}))
    );
    let note = h.state().shell().note().map(str::to_owned);
    assert_eq!(
        note.as_deref(),
        Some("Library check: no problems in Resistor")
    );
}

/// Registers a new Project Library "Parts" in the Project folder; returns its UUID and
/// folder.
fn project_library(h: &mut Harness<'_, App>) -> (String, PathBuf) {
    let folder = h.state().shell().engine().location().unwrap().join("parts");
    let made = run(
        h,
        "library.create",
        json!({"path": folder, "name": "Parts"}),
    );
    run(h, "project.library.add", json!({"path": "parts"}));
    h.run();
    (made["library"].as_str().unwrap().to_owned(), folder)
}

#[test]
fn make_project_library_extracts_every_placed_part_into_the_chosen_library() {
    let (home, work) = (TempDir::new("lib-make-home"), TempDir::new("lib-make"));
    let mut h = harness(&home, &work);
    let (library, folder) = project_library(&mut h);
    place(&mut h, RESISTOR, 1000);
    h.get_by_role_and_label(Role::Button, "New…").click();
    h.run();
    h.get_by_label("Make Project Library…").click();
    h.run();
    h.get_by_label("Make Project Library");
    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Extract").click();
    h.run();
    assert_eq!(
        entries_since(&h, from)[0],
        (
            "library.extract_from_project".to_owned(),
            json!({"library": library})
        )
    );
    assert!(folder.join("parts/Resistor.redapart").exists());
    // Its Symbol and its five alternate Footprints come along.
    assert!(folder.join("symbols/Resistor.redasym").exists());
    let note = h.state().shell().note().map(str::to_owned).unwrap();
    assert!(note.starts_with("Wrote 7 elements"), "{note}");
}

#[test]
fn edit_in_library_on_source_missing_offers_to_extract_it() {
    let (home, work) = (
        TempDir::new("lib-extract-home"),
        TempDir::new("lib-extract"),
    );
    let mut h = harness_with_mine(&home, &work);
    run(
        &mut h,
        "library.element.copy",
        json!({"element": RESISTOR, "library": MINE}),
    );
    let copy = parts_named(&mut h, "Resistor")
        .into_iter()
        .find(|(l, _)| l == MINE)
        .unwrap()
        .1;
    let placed = place(&mut h, &copy, 1000);
    let component = placed["result"]["component"]["uuid"].clone();
    let unit = placed["result"]["unit"].clone();
    // The copy goes from Mine: the Component reads Source Missing.
    std::fs::remove_file(mine(&home).join("parts/Resistor.redapart")).unwrap();
    run(
        &mut h,
        "library.element.edit_in_library",
        json!({"objects": [unit]}),
    );
    h.run();
    h.get_by_label("Extract to Library");
    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Extract").click();
    h.run();
    assert_eq!(
        entries_since(&h, from)[0],
        (
            "library.extract_from_project".to_owned(),
            json!({"components": [component], "library": MINE})
        )
    );
    assert!(mine(&home).join("parts/Resistor.redapart").exists());
}
