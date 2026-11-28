//! Board Templates in the window, driven through AccessKit with `egui_kittest`: Design ▸
//! Save Board as Template… (its candidates with their defaults, the inner-layer Keepout it
//! can't tick, the target Library, and the one `library.board_template.save` it sends),
//! and the Library panel's template rows with Duplicate, Copy to Library… and Delete….

use std::path::{Path, PathBuf};

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::commands::board_template::{CandidateKind, candidates};
use reda_ui::engine::model::Purpose;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const MINE: &str = "000000aa-0000-4000-8000-000000000000";
const PI: &str = "40f34815-ae6e-4549-bf0e-a54bb293ca59";
const PI_NAME: &str = "Add-on board for Raspberry Pi";
const RESISTOR_SYMBOL: &str = "06c71621-e7e4-49a4-9b90-88485bb936b5";

fn starter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

fn mine(home: &TempDir) -> PathBuf {
    home.path().join("lib")
}

/// A window with the starter Library and the empty user Library Mine, and a 4-layer
/// Project made from the Pi template.
fn harness(home: &TempDir, work: &TempDir) -> Harness<'static, App> {
    let lib = mine(home);
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("Mine.redalib"),
        format!("{{\"format\": 1, \"library\": {{\"uuid\": \"{MINE}\", \"name\": \"Mine\"}}}}"),
    )
    .unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
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
    shell
        .run_reporting(
            "project.new",
            json!({"path": work.path(), "name": "Hat", "layers": 4,
                   "board_template": {"element": PI}}),
        )
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

fn click(h: &mut Harness<'_, App>, label: &str) {
    h.get_by_label(label).click();
    h.run();
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

/// The tick of the capture row `label`: (enabled, ticked).
fn tick(h: &Harness<'_, App>, label: &str) -> (bool, bool) {
    let node = h.get_by_role_and_label(Role::CheckBox, label);
    let node = node.accesskit_node();
    (!node.is_disabled(), node.toggled() == Some(Toggled::True))
}

#[test]
fn save_board_as_template_lists_the_candidates_and_sends_the_ticked_ones() {
    let (home, work) = (
        TempDir::new("save-dialog-home"),
        TempDir::new("save-dialog"),
    );
    let mut h = harness(&home, &work);
    let pcb = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .clone();
    run(&mut h, "document.open", json!({"document": pcb.uuid}));
    // An inner-layer Keepout, and the connector unlocked.
    run(
        &mut h,
        "pcb.keepout.add",
        json!({"layer": "copper_l2", "vertices": [
            {"x":"30mm","y":"20mm"},{"x":"35mm","y":"20mm"},{"x":"35mm","y":"25mm"}]}),
    );
    let j1 = pcb
        .footprints()
        .find(|f| f.designator == "J1")
        .unwrap()
        .uuid;
    run(
        &mut h,
        "pcb.lock",
        json!({"objects": [j1], "locked": false}),
    );
    // A third Mechanical Layer with a text, and an empty one.
    let mechanical = |h: &mut Harness<'_, App>, params: Value| {
        let mut params = params;
        params["kind"] = json!("mechanical");
        params["document"] = json!(pcb.uuid);
        run(h, "pcb.layer.add", params)["result"]["uuid"].clone()
    };
    let fab = mechanical(
        &mut h,
        json!({"name": "Fab notes", "purpose": "fabrication_notes"}),
    );
    mechanical(&mut h, json!({"name": "Empty"}));
    run(
        &mut h,
        "pcb.text.add",
        json!({"layer": fab, "text": "ENIG", "x": "10mm", "y": "-5mm"}),
    );
    h.run();

    click(&mut h, "Design");
    click(&mut h, "Save Board as Template…");
    h.get_by_label("Save Board as Template");
    // The outline always comes; the Locked holes are ticked; the unlocked connector not.
    assert_eq!(tick(&h, "Board Outline (always saved)"), (false, true));
    assert_eq!(tick(&h, "Board Cutout 1"), (true, true));
    let starting = |h: &Harness<'_, App>, prefix: &str| {
        h.query_all_by_role(Role::CheckBox)
            .filter_map(|n| n.accesskit_node().label())
            .find(|l| l.starts_with(prefix))
            .unwrap_or_else(|| panic!("no row starts with {prefix}"))
    };
    let hole = starting(&h, "H1 ");
    assert!(hole.ends_with("(top)"), "{hole}");
    assert_eq!(tick(&h, &hole), (true, true));
    let connector = starting(&h, "J1 ");
    assert!(connector.contains("SKTV254"), "{connector}");
    assert_eq!(tick(&h, &connector), (true, false));
    assert_eq!(tick(&h, "Drawings on Dimensions"), (true, true));
    assert_eq!(tick(&h, "Drawings on Reference"), (true, true));
    // One tick per Mechanical Layer with content: none for the empty one.
    assert_eq!(tick(&h, "Drawings on Fab notes"), (true, true));
    let drawing_rows: Vec<String> = h
        .query_all_by_role(Role::CheckBox)
        .filter_map(|n| n.accesskit_node().label())
        .filter(|l| l.starts_with("Drawings on "))
        .collect();
    assert_eq!(
        drawing_rows,
        [
            "Drawings on Dimensions",
            "Drawings on Reference",
            "Drawings on Fab notes"
        ]
    );
    // The inner-layer Keepout is listed but can't be ticked, and says why.
    let inner = h
        .query_all_by_role(Role::CheckBox)
        .filter_map(|n| n.accesskit_node().label())
        .find(|l| l.contains("inner layer"))
        .expect("the inner-layer Keepout's row");
    assert_eq!(tick(&h, &inner), (false, false));
    h.get_by_label("inner-layer Keepouts can't go in a template yet");
    // The target Library is offered, Mine first as the only writable one.
    let picker = h.get_by_role_and_label(Role::ComboBox, "Library");
    assert_eq!(picker.accesskit_node().value().as_deref(), Some("Mine"));

    // Save Template stays off until there is a name.
    assert!(
        h.get_by_role_and_label(Role::Button, "Save Template")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_role_and_label(Role::TextInput, "Name").focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Name")
        .type_text("My hat");
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Drawings on Reference")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Drawings on Reference")
        .click();
    h.run();
    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Save Template")
        .click();
    h.run();
    let sent = entries_since(&h, from);
    assert_eq!(sent.len(), 1, "{sent:?}");
    let (id, params) = &sent[0];
    assert_eq!(id, "library.board_template.save");
    assert_eq!(params["library"], MINE);
    assert_eq!(params["name"], "My hat");
    let objects: Vec<&str> = (params["objects"].as_array().unwrap().iter())
        .map(|o| o.as_str().unwrap())
        .collect();
    // The drawing rows stand for their Layers.
    let drawings = |kind| {
        let project = h.state().shell().engine().project().unwrap();
        (candidates(project).into_iter())
            .find(|c| c.kind == CandidateKind::Drawings && c.purpose == Some(kind))
            .unwrap()
            .object
            .to_string()
    };
    let mechanical = drawings(Purpose::Dimensions);
    let documentation = drawings(Purpose::Reference);
    assert!(objects.contains(&mechanical.as_str()), "{objects:?}");
    assert!(!objects.contains(&documentation.as_str()), "{objects:?}");
    assert!(objects.contains(&fab.as_str().unwrap()), "{objects:?}");
    assert!(!objects.contains(&j1.to_string().as_str()), "{objects:?}");
    assert!(h.query_by_label("Save Board as Template").is_none());
    // Each ticked Layer is one drawing group named after it, carrying its Purpose.
    let saved = std::fs::read(mine(&home).join("board-templates/My hat.redaboard")).unwrap();
    let saved: Value = serde_json::from_slice(&saved).unwrap();
    let groups: Vec<(&str, &str)> = (saved["drawing_groups"].as_array().unwrap().iter())
        .map(|g| (g["name"].as_str().unwrap(), g["purpose"].as_str().unwrap()))
        .collect();
    assert_eq!(
        groups,
        [
            ("Dimensions", "dimensions"),
            ("Fab notes", "fabrication_notes")
        ]
    );
}

#[test]
fn a_local_pad_edit_warns_in_the_dialog() {
    let (home, work) = (TempDir::new("save-warn-home"), TempDir::new("save-warn"));
    let mut h = harness(&home, &work);
    let pcb = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .clone();
    run(&mut h, "document.open", json!({"document": pcb.uuid}));
    let j1 = pcb.footprints().find(|f| f.designator == "J1").unwrap();
    let pad = pcb.footprint_copy(j1.footprint).unwrap().footprint.pads[0].uuid;
    run(
        &mut h,
        "pcb.pad.edit",
        json!({"instance": j1.uuid, "pad": pad, "width": "2mm"}),
    );
    h.run();
    click(&mut h, "Design");
    click(&mut h, "Save Board as Template…");
    h.get_by_label("J1 has local Pad edits that won't be saved");
}

/// Types `text` into the Library panel's search box, replacing what is there.
fn search_for(h: &mut Harness<'_, App>, text: &str) {
    h.state_mut().shell_mut().library_panel_mut().text.clear();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Search Parts")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Search Parts")
        .type_text(text);
    h.run();
}

fn rows(h: &Harness<'_, App>) -> Vec<String> {
    h.query_all_by_role(Role::ListItem)
        .filter_map(|n| n.accesskit_node().label())
        .collect()
}

/// Opens `row`'s menu and clicks `item`.
fn row_menu(h: &mut Harness<'_, App>, row: &str, item: &str) {
    h.get_by_role_and_label(Role::ListItem, row)
        .click_secondary();
    h.run();
    h.get_by_role_and_label(Role::Button, item).click();
    h.run();
}

#[test]
fn the_library_panel_lists_templates_in_a_library_tab_with_their_menu() {
    let (home, work) = (
        TempDir::new("template-panel-home"),
        TempDir::new("template-panel"),
    );
    let mut h = harness(&home, &work);
    run(&mut h, "panel.show", json!({"panel": "library"}));
    let row = format!("Board Template: {PI_NAME}");
    // Only beside a Library editor tab: the Starter Library's here.
    search_for(&mut h, "Raspberry");
    assert!(!rows(&h).contains(&row), "{:?}", rows(&h));
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
    search_for(&mut h, "Raspberry");
    assert!(rows(&h).contains(&row), "{:?}", rows(&h));
    // A template has no editor; its details say what it is.
    h.get_by_role_and_label(Role::ListItem, &row).click();
    h.run();
    assert!(
        h.get_by_role_and_label(Role::Button, "Edit Element")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_label("Suggested copper Layers: 2");
    // The Starter Library's can be copied, not duplicated or deleted.
    h.get_by_role_and_label(Role::ListItem, &row)
        .click_secondary();
    h.run();
    assert!(h.get_by_label("Duplicate").accesskit_node().is_disabled());
    assert!(h.get_by_label("Delete…").accesskit_node().is_disabled());
    assert!(h.query_by_label("Where Used").is_none());
    click(&mut h, "Copy to Library…");
    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Copy").click();
    h.run();
    assert_eq!(
        entries_since(&h, from)[0],
        (
            "library.element.copy".to_owned(),
            json!({"element": PI, "library": MINE})
        )
    );
    let copied = mine(&home).join(format!("board-templates/{PI_NAME}.redaboard"));
    assert!(copied.exists());

    // Mine's copy, beside one of Mine's elements: Duplicate, then Delete… with a
    // confirmation.
    let symbol = run(
        &mut h,
        "library.element.copy",
        json!({"element": RESISTOR_SYMBOL, "library": MINE}),
    )["element"]
        .clone();
    run(&mut h, "library.element.open", json!({"element": symbol}));
    run(&mut h, "document.open", json!({"document": symbol}));
    h.run();
    search_for(&mut h, "Raspberry");
    assert_eq!(rows(&h), std::slice::from_ref(&row));
    row_menu(&mut h, &row, "Duplicate");
    search_for(&mut h, "Raspberry");
    assert_eq!(rows(&h), [row.clone(), format!("{row}_1")]);
    row_menu(&mut h, &row, "Delete…");
    h.get_by_label(&format!("Delete {PI_NAME}"));
    assert!(
        copied.exists(),
        "nothing is deleted before the confirmation"
    );
    let from = journal_len(&h);
    h.get_by_role_and_label(Role::Button, "Delete").click();
    h.run();
    let sent = entries_since(&h, from);
    assert_eq!(sent[0].0, "library.element.delete", "{sent:?}");
    assert!(!copied.exists());
    search_for(&mut h, "Raspberry");
    assert_eq!(rows(&h), [format!("{row}_1")]);
}
