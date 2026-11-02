//! Saved Layouts: `layout.save` writes the layout to `layouts/<name>.json` in the config
//! folder as Panels by id and Document slots, `layout.apply:<name>` puts it back, and new
//! Projects start from the default Saved Layout the Preferences name.

use std::path::Path;

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{ErrorCode, UserDirs};
use reda_ui::layout::{Area, DockSide, GroupId, Layout, PanelId, TabKey};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn documents(shell: &Shell) -> Vec<TabKey> {
    shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .map(|d| TabKey::Document(d.uuid))
        .collect()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn a_saved_layout_stores_panels_by_id_and_document_slots_but_never_documents() {
    let home = TempDir::new("saved-home");
    let work = TempDir::new("saved-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Saved").unwrap();
    let sheet = documents(&shell)[0].clone();
    shell
        .run(
            "layout.tab.move_to_dock",
            json!({"tab": "properties", "dock": "bottom"}),
        )
        .unwrap();

    shell.run("layout.save", json!({"name": "wide"})).unwrap();

    let file = home.path().join("config/layouts/wide.json");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains(&sheet.to_string()), "{text}");
    let saved = read_json(&file);
    assert_eq!(saved["centre"]["group"]["tabs"], json!(["documents"]));
    assert_eq!(saved["left"]["groups"][0]["tabs"], json!(["projects"]));
    assert_eq!(
        saved["bottom"]["groups"][0]["tabs"],
        json!(["messages", "properties"])
    );
    assert_eq!(saved["right"]["groups"], json!([]));
}

#[test]
fn applying_a_saved_layout_puts_panels_back_and_documents_in_its_document_group() {
    let home = TempDir::new("saved-home");
    let work = TempDir::new("saved-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Saved").unwrap();
    let sheet = documents(&shell)[0].clone();
    shell.run("layout.save", json!({"name": "plain"})).unwrap();

    shell
        .run(
            "layout.tab.move_to_dock",
            json!({"tab": sheet.to_string(), "dock": "right"}),
        )
        .unwrap();
    shell
        .run("layout.tab.close", json!({"tab": "projects"}))
        .unwrap();

    shell.run("layout.apply:plain", json!({})).unwrap();

    let layout = shell.layout();
    assert_eq!(layout.place(&sheet).unwrap().area, Area::Centre);
    assert_eq!(
        layout
            .place(&TabKey::Panel(PanelId::Projects))
            .unwrap()
            .area,
        Area::Dock(DockSide::Left)
    );
    assert_eq!(
        layout.dock(DockSide::Right).groups[0].tabs,
        [TabKey::Panel(PanelId::Properties)]
    );
    assert_eq!(shell.document_tabs().len(), 1);
}

#[test]
fn documents_go_into_the_first_document_slot_only() {
    let home = TempDir::new("saved-home");
    let work = TempDir::new("saved-work");
    let layouts = home.path().join("config/layouts");
    std::fs::create_dir_all(&layouts).unwrap();
    let saved = json!({
        "format": 1,
        "centre": {"split": {"axis": "horizontal", "children": [
            {"group": {"id": 1, "tabs": ["documents"]}},
            {"group": {"id": 7, "tabs": ["documents", "properties"]}}
        ]}},
        "left": {"groups": []},
        "right": {"groups": []},
        "bottom": {"groups": [
            {"id": 4, "tabs": ["messages"]},
            {"id": 9, "tabs": ["documents"]}
        ], "collapsed": true}
    });
    std::fs::write(layouts.join("two_up.json"), saved.to_string()).unwrap();
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Saved").unwrap();
    let sheet = documents(&shell)[0].clone();

    shell.run("layout.apply:two_up", json!({})).unwrap();

    let layout = shell.layout();
    assert_eq!(layout.group(GroupId(1)).unwrap().tabs, [sheet]);
    assert_eq!(
        layout.group(GroupId(7)).unwrap().tabs,
        [TabKey::Panel(PanelId::Properties)]
    );
    // The Document slots left without Documents go away.
    assert_eq!(layout.group(GroupId(9)), None);
    assert!(layout.dock(DockSide::Bottom).collapsed);
    assert_eq!(layout.place(&TabKey::Panel(PanelId::Projects)), None);
}

#[test]
fn saved_layouts_are_offered_as_presets_once_saved() {
    let home = TempDir::new("saved-home");
    let mut shell = Shell::new(engine_in(home.path()));
    let err = shell.run("layout.apply:mine", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);

    shell.run("layout.save", json!({"name": "mine"})).unwrap();

    let described = shell.engine().registry().describe("layout.apply").unwrap();
    assert_eq!(described["presets"][0]["id"], "layout.apply:mine");
    shell.run("layout.apply:mine", json!({})).unwrap();
}

#[test]
fn saved_layout_names_are_lowercase_snake_case() {
    let home = TempDir::new("saved-home");
    let mut shell = Shell::new(engine_in(home.path()));
    for bad in ["", "Wide", "../escape", "a b", "x:y"] {
        let err = shell.run("layout.save", json!({"name": bad})).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams, "{bad:?}");
    }
    assert!(!home.path().join("config/layouts").exists());
}

#[test]
fn an_unreadable_saved_layout_is_refused_and_leaves_the_layout_as_it_was() {
    let home = TempDir::new("saved-home");
    let layouts = home.path().join("config/layouts");
    std::fs::create_dir_all(&layouts).unwrap();
    std::fs::write(layouts.join("broken.json"), "{ not json").unwrap();
    let mut shell = Shell::new(engine_in(home.path()));
    let before = shell.layout().clone();

    let err = shell
        .run("layout.apply", json!({"name": "broken"}))
        .unwrap_err();

    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(shell.layout(), &before);
}

#[test]
fn new_projects_start_from_the_default_saved_layout() {
    let home = TempDir::new("saved-home");
    let work = TempDir::new("saved-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell
        .run(
            "layout.tab.move_to_dock",
            json!({"tab": "messages", "dock": "left"}),
        )
        .unwrap();
    shell
        .run("layout.tab.close", json!({"tab": "properties"}))
        .unwrap();
    shell.run("layout.save", json!({"name": "lean"})).unwrap();
    shell
        .run(
            "app.preferences.set",
            json!({"key": "general.default_layout", "value": "lean"}),
        )
        .unwrap();

    shell.new_project(work.path(), "Lean").unwrap();

    let layout = shell.layout();
    let sheet = documents(&shell)[0].clone();
    assert_eq!(layout.place(&sheet).unwrap().area, Area::Centre);
    assert_eq!(layout.place(&TabKey::Panel(PanelId::Properties)), None);
    assert_eq!(
        layout
            .place(&TabKey::Panel(PanelId::Messages))
            .unwrap()
            .area,
        Area::Dock(DockSide::Left)
    );
    assert_eq!(layout.place(&TabKey::Start), None);
}

#[test]
fn a_missing_default_saved_layout_falls_back_to_the_default_layout() {
    let home = TempDir::new("saved-home");
    let work = TempDir::new("saved-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell
        .run(
            "app.preferences.set",
            json!({"key": "general.default_layout", "value": "gone"}),
        )
        .unwrap();

    shell.new_project(work.path(), "Plain").unwrap();

    let sheet = documents(&shell)[0].clone();
    assert_eq!(shell.layout(), &Layout::with_centre(vec![sheet]));
}
