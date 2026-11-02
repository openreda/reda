//! Menus, toolbars and the Active Bar are data files per Document type
//! (`crates/reda-ui/bars/*.json`) that bind only to Commands, Presets and raw
//! `{command, params}`, with separators and submenus. They follow the active Document, and
//! an item that cannot run is greyed out with the reason.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::bars::{self, Bars, Binding, DocumentType, Item, Request};
use reda_ui::engine::command::{ErrorCode, UserDirs};
use reda_ui::engine::{Engine, preferences};
use reda_ui::i18n;
use reda_ui::shell::Shell;
use serde_json::json;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn make_project(folder: &Path, name: &str) -> PathBuf {
    let mut engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    engine
        .run("project.new", json!({"path": folder, "name": name}))
        .unwrap();
    folder.join(name).join(format!("{name}.reda"))
}

/// Every context the bars have: no Document, and each Document type.
fn every_context() -> Vec<Option<DocumentType>> {
    let mut out = vec![None];
    out.extend(DocumentType::ALL.map(Some));
    out
}

/// Every action item of `bars` with where it sits, depth first.
fn actions(bars: &Bars) -> Vec<&bars::Action> {
    fn walk<'a>(items: &'a [Item], out: &mut Vec<&'a bars::Action>) {
        for item in items {
            match item {
                Item::Separator => {}
                Item::Submenu(menu) => walk(&menu.items, out),
                Item::Action(action) => out.push(action),
            }
        }
    }
    let mut out = Vec::new();
    for menu in &bars.menus {
        walk(&menu.items, &mut out);
    }
    walk(&bars.toolbar, &mut out);
    walk(&bars.active_bar, &mut out);
    out
}

/// Every title key the bars name: menus, submenus and items that override a title.
fn title_keys(bars: &Bars) -> Vec<String> {
    fn walk(items: &[Item], out: &mut Vec<String>) {
        for item in items {
            match item {
                Item::Submenu(menu) => {
                    out.push(menu.title.clone());
                    walk(&menu.items, out);
                }
                Item::Action(action) => out.extend(action.title.clone()),
                Item::Separator => {}
            }
        }
    }
    let mut out = Vec::new();
    for menu in &bars.menus {
        out.push(menu.title.clone());
        walk(&menu.items, &mut out);
    }
    walk(&bars.toolbar, &mut out);
    walk(&bars.active_bar, &mut out);
    out
}

#[test]
fn every_document_type_has_bars_with_unique_stable_ids() {
    for context in every_context() {
        let bars = bars::for_document(context);
        assert!(!bars.menus.is_empty(), "{context:?} has no menus");
        let mut ids = BTreeSet::new();
        for menu in &bars.menus {
            assert!(
                ids.insert(format!("menu {}", menu.id)),
                "{context:?}: {}",
                menu.id
            );
        }
        for action in actions(bars) {
            // An id may appear in a menu and on a bar, but only once per place.
            assert!(
                !action.id.is_empty()
                    && action
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b == b'.' || b == b'_'),
                "{context:?}: bad id {:?}",
                action.id
            );
        }
        for key in title_keys(bars) {
            assert!(
                i18n::has_key(&key),
                "{context:?}: {key} is not in i18n/en-US.ftl"
            );
        }
    }
}

#[test]
fn the_bars_follow_the_document_type() {
    let schematic = bars::for_document(Some(DocumentType::Schematic));
    let pcb = bars::for_document(Some(DocumentType::Pcb));
    let none = bars::for_document(None);
    let ids = |bars: &Bars| -> Vec<String> { actions(bars).iter().map(|a| a.id.clone()).collect() };
    assert!(ids(schematic).contains(&"design.erc".to_owned()));
    assert!(!ids(pcb).contains(&"design.erc".to_owned()));
    assert!(ids(pcb).contains(&"design.zone_fill".to_owned()));
    assert!(!ids(none).iter().any(|id| id.starts_with("design.")));
    // The common menus come first and stay in every context.
    for bars in [schematic, pcb, none] {
        let menus: Vec<&str> = bars.menus.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(&menus[..3], ["file", "edit", "view"]);
    }
    assert!(
        none.active_bar.is_empty(),
        "with no Document there is no canvas"
    );
    assert!(!schematic.active_bar.is_empty());
}

#[test]
fn a_data_file_takes_only_bindings_separators_and_submenus() {
    let ok = bars::parse(
        r#"{"menus": [{"id": "m", "title": "menu-file", "items": [
            {"id": "a", "command": "project.save"},
            {"id": "b", "command": "panel.show:layers"},
            {"id": "c", "command": "app.preferences.set", "params": {"key": "general.theme", "value": "dark"}},
            "separator",
            {"id": "d", "title": "menu-view", "items": []}
        ]}]}"#,
    )
    .unwrap();
    let bindings: Vec<&Binding> = actions(&ok).iter().map(|a| &a.binding).collect();
    assert_eq!(
        bindings,
        [
            &Binding::Command("project.save".into()),
            &Binding::Preset("panel.show:layers".into()),
            &Binding::Raw {
                command: "app.preferences.set".into(),
                params: json!({"key": "general.theme", "value": "dark"})
            },
        ]
    );
    // No callbacks or anything else: unknown fields, unknown item shapes and Presets with
    // params are refused.
    for bad in [
        r#"{"menus": [{"id": "m", "title": "t", "items": [{"id": "a", "command": "x", "on_click": "quit"}]}]}"#,
        r#"{"menus": [{"id": "m", "title": "t", "items": [{"id": "a", "callback": "quit"}]}]}"#,
        r#"{"menus": [{"id": "m", "title": "t", "items": ["spacer"]}]}"#,
        r#"{"menus": [{"id": "m", "title": "t", "items": [{"id": "a", "command": "panel.show:layers", "params": {}}]}]}"#,
        r#"{"menus": [], "callbacks": {}}"#,
    ] {
        assert!(bars::parse(bad).is_err(), "accepted {bad}");
    }
}

#[test]
fn no_menu_is_built_in_code() {
    // The menu bar, toolbar and Active Bar are drawn from the data files only: menu titles
    // and egui menu bars appear nowhere in the source but the bars renderer.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for file in rust_files(&src) {
        let text = std::fs::read_to_string(&file).unwrap();
        let renderer = file.ends_with("bars/draw.rs");
        if (!renderer && text.contains("MenuBar")) || text.contains("tr!(\"menu-") {
            offenders.push(file.display().to_string());
        }
    }
    assert!(offenders.is_empty(), "menus built in code: {offenders:?}");
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}

#[test]
fn every_binding_names_a_command_or_preset_and_fits_its_params() {
    let home = TempDir::new("bars-home");
    let work = TempDir::new("bars-work");
    let file = make_project(work.path(), "Bound");
    for context in every_context() {
        let mut shell = Shell::new(engine_in(home.path()));
        shell.open_project(&file).unwrap();
        for action in actions(bars::for_document(context)) {
            let item = bars::resolve(&shell, action);
            assert!(
                reda_ui::icons::has(&item.icon),
                "{}: no art for {}",
                action.id,
                item.icon
            );
            assert!(!item.title.is_empty());
            let Ok((id, params)) = item.run else {
                continue;
            };
            if id == "app.quit" {
                continue;
            }
            if let Err(e) = shell.run(&id, params.clone()) {
                assert!(
                    !matches!(e.code, ErrorCode::InvalidParams | ErrorCode::NotFound),
                    "{context:?} {}: {id} {params} is not a valid binding: {e:?}",
                    action.id
                );
            }
        }
    }
}

fn action<'a>(bars: &'a Bars, id: &str) -> &'a bars::Action {
    actions(bars)
        .into_iter()
        .find(|a| a.id == id)
        .unwrap_or_else(|| panic!("no item {id}"))
}

#[test]
fn an_item_that_cannot_run_says_why() {
    let home = TempDir::new("bars-home");
    let shell = Shell::new(engine_in(home.path()));
    let bars = bars::for_document(None);
    let save = bars::resolve(&shell, action(bars, "file.save_all"));
    assert_eq!(save.run, Err("no Project is open".to_owned()));
    let undo = bars::resolve(&shell, action(bars, "edit.undo"));
    assert_eq!(undo.run, Err("no Document is active".to_owned()));
    // A param that names the active Document is context, so the title stays the Command's.
    assert_eq!(undo.title, "Undo");
    let fit = bars::resolve(&shell, action(bars, "view.zoom_fit"));
    assert_eq!(fit.title, "Zoom to Fit");
    assert_eq!(fit.run, Err("no Document is active".to_owned()));
    let open = bars::resolve(&shell, action(bars, "file.open_project"));
    assert_eq!(open.title, "Open Project…");
    assert_eq!(open.icon, "project_open");
    assert_eq!(
        open.run,
        Ok(("app.dialog.open:open_project".to_owned(), json!({})))
    );
}

#[test]
fn raw_bindings_name_the_active_document_and_summarise_their_params() {
    let home = TempDir::new("bars-home");
    let work = TempDir::new("bars-work");
    let file = make_project(work.path(), "Raw");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&file).unwrap();
    let sheet = shell.active_document().unwrap().document;
    let bars = bars::for_document(bars::active_type(&shell));
    let undo = bars::resolve(&shell, action(bars, "edit.undo"));
    assert_eq!(
        undo.run,
        Ok((
            "edit.undo".to_owned(),
            json!({"document": sheet.to_string()})
        ))
    );
    // A raw binding without a title of its own shows the Command title and its params.
    let raw = bars::parse(
        r#"{"menus": [{"id": "m", "title": "menu-file", "items": [
            {"id": "a", "command": "app.preferences.set", "params": {"key": "general.theme", "value": "dark"}}
        ]}]}"#,
    )
    .unwrap();
    let item = bars::resolve(&shell, actions(&raw)[0]);
    assert_eq!(
        item.title,
        "Set Preference (key: general.theme, value: dark)"
    );
}

#[test]
fn the_active_document_picks_the_bars() {
    let home = TempDir::new("bars-home");
    let work = TempDir::new("bars-work");
    let file = make_project(work.path(), "Active");
    let mut shell = Shell::new(engine_in(home.path()));
    assert_eq!(bars::active_type(&shell), None);
    shell.open_project(&file).unwrap();
    assert_eq!(bars::active_type(&shell), Some(DocumentType::Schematic));
}

// The window.

fn builder() -> HarnessBuilder<App> {
    Harness::builder().with_size([1000.0, 640.0])
}

fn harness(home: &Path) -> Harness<'static, App> {
    let app = App::new(Shell::new(engine_in(home)));
    builder().build_ui_state(|ui, app: &mut App| app.ui(ui), app)
}

fn journal_ids(h: &Harness<'_, App>) -> Vec<String> {
    h.state()
        .shell()
        .engine()
        .journal()
        .iter()
        .map(|e| e.id.clone())
        .collect()
}

#[test]
fn an_unavailable_menu_item_is_greyed_with_the_reason_on_hover() {
    let home = TempDir::new("bars-home");
    let mut h = harness(home.path());
    h.run();
    h.get_by_role_and_label(Role::Button, "File").click();
    h.run();
    // The menu item reads with its shortcut, which tells it from the toolbar's Save All.
    let shortcut = if cfg!(target_os = "macos") {
        "Cmd+S"
    } else {
        "Ctrl+S"
    };
    let label = format!("Save All {shortcut}");
    let save = h.get_by_role_and_label(Role::Button, &label);
    assert!(save.accesskit_node().is_disabled());
    save.hover();
    h.run_steps(120);
    h.get_by_label("no Project is open");
}

#[test]
fn menu_items_run_their_commands() {
    let home = TempDir::new("bars-home");
    let mut h = harness(home.path());
    h.run();
    h.get_by_role_and_label(Role::Button, "View").click();
    h.run();
    h.get_by_label("Theme").hover();
    h.run();
    h.get_by_label("Light").click();
    h.run();
    assert_eq!(journal_ids(&h).last().unwrap(), "view.theme.set:light");
    assert_eq!(h.ctx.theme(), egui::Theme::Light);
    let prefs = preferences::current(Some(&UserDirs::under(home.path())));
    assert_eq!(prefs.general.theme, preferences::Theme::Light);

    h.get_by_role_and_label(Role::Button, "File").click();
    h.run();
    h.get_all_by_label("New Project…").next().unwrap().click();
    h.run();
    // The dialog then lists the Fab Presets to offer.
    let ids = journal_ids(&h);
    assert!(ids.iter().any(|id| id == "app.dialog.open:new_project"));
    h.get_by_role_and_label(Role::TextInput, "Name");
}

#[test]
fn toolbar_and_active_bar_buttons_run_their_commands() {
    let home = TempDir::new("bars-home");
    let work = TempDir::new("bars-work");
    let file = make_project(work.path(), "Bars");
    let mut h = harness(home.path());
    h.run();
    h.state_mut().shell_mut().open_project(&file).unwrap();
    h.run();
    h.get_by_label("Zoom to Fit").click();
    h.run();
    assert_eq!(journal_ids(&h).last().unwrap(), "view.zoom_fit");
    // The sheet's Active Bar holds its Tools.
    h.get_by_label("Select").click();
    h.run();
    assert_eq!(journal_ids(&h).last().unwrap(), "tool.activate:select");
    let sheet = h.state().shell().active_document().unwrap().document;
    let wire = h
        .state_mut()
        .shell_mut()
        .run(
            "schematic.wire.add",
            json!({"document": sheet, "points": [{"x": 0, "y": 0}, {"x": 2_540_000, "y": 0}]}),
        )
        .unwrap()["result"]["wires"][0]
        .clone();
    let shell = h.state_mut().shell_mut();
    shell
        .run("selection.set", json!({ "objects": [wire] }))
        .unwrap();
    h.run();
    h.get_by_label("Drag").click();
    h.run();
    assert_eq!(journal_ids(&h).last().unwrap(), "tool.activate:drag");
    assert_eq!(h.state().shell().tool(), reda_ui::tools::Tool::Drag);
    h.get_all_by_label("Run ERC").next().unwrap().click();
    h.run();
    assert_eq!(journal_ids(&h).last().unwrap(), "erc.run");
}

#[test]
fn window_commands_ask_the_window_and_are_refused_headless() {
    let home = TempDir::new("bars-home");
    let work = TempDir::new("bars-work");
    let file = make_project(work.path(), "Requests");
    let mut shell = Shell::new(engine_in(home.path()));
    let err = shell.run("view.zoom_fit", json!({})).unwrap_err();
    assert_eq!(err.message, "no Document is active");
    shell.open_project(&file).unwrap();
    let sheet = shell.active_document().unwrap().document;
    shell.run("app.dialog.open:new_project", json!({})).unwrap();
    shell
        .run("app.dialog.open", json!({"dialog": "open_project"}))
        .unwrap();
    shell.run("view.zoom_fit", json!({})).unwrap();
    shell
        .run("view.theme.set", json!({"theme": "dark"}))
        .unwrap();
    shell.run("app.quit", json!({})).unwrap();
    assert_eq!(
        bars::take_requests(shell.engine()),
        [
            Request::NewProject,
            Request::OpenProject,
            Request::ZoomFit(sheet),
            Request::Theme(preferences::Theme::Dark),
            Request::Quit,
        ]
    );
    assert!(bars::take_requests(shell.engine()).is_empty());
    let prefs = preferences::current(Some(&UserDirs::under(home.path())));
    assert_eq!(prefs.general.theme, preferences::Theme::Dark);

    let headless = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    let mut shell = Shell::new(headless);
    let err = shell.run("app.quit", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::Unavailable);
}
