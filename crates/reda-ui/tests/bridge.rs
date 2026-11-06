//! The app's end of the bridge, without a window: agents asking to connect and being
//! remembered, bridged Commands applied between frames with source-labelled undo and never
//! saved, a Tool's preview cancelled by a bridged edit, and file-association opens handed
//! to the running instance.

use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use reda_testkit::{TempDir, read_tree};
use reda_ui::bridge::{self, Bridge};
use reda_ui::control::client::{self, Connection};
use reda_ui::engine::command::{CommandError, ErrorCode, UserDirs};
use reda_ui::engine::journal::Source;
use reda_ui::engine::model::{DocumentKind, Scope, Uuid};
use reda_ui::engine::{Engine, preferences, state};
use reda_ui::layout::TabKey;
use reda_ui::shell::{Shell, ToolPreview};
use serde_json::{Value, json};

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

/// Creates a Project named `name` in `folder`; returns its `.reda` file.
fn make_project(folder: &Path, name: &str) -> PathBuf {
    let mut engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    engine
        .run("project.new", json!({"path": folder, "name": name}))
        .unwrap();
    std::fs::canonicalize(folder.join(name).join(format!("{name}.reda"))).unwrap()
}

/// A Shell with `project` open and its Bridge started.
fn app(home: &Path, project: &Path) -> (Shell, Bridge) {
    let mut shell = Shell::new(engine_in(home));
    shell.open_project(project).unwrap();
    let bridge = Bridge::start(&shell);
    assert!(bridge.is_on(), "{:?}", bridge.error());
    (shell, bridge)
}

/// Pumps the bridge, as the window does between frames, until `done` holds.
fn pump_until(shell: &mut Shell, bridge: &mut Bridge, mut done: impl FnMut(&Bridge) -> bool) {
    let start = Instant::now();
    while !done(bridge) {
        bridge.pump(shell);
        assert!(start.elapsed() < Duration::from_secs(20), "timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Runs `work` on another thread (an agent), pumping the bridge until it finishes.
fn as_agent<T: Send + 'static>(
    shell: &mut Shell,
    bridge: &mut Bridge,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    let agent: JoinHandle<T> = std::thread::spawn(work);
    while !agent.is_finished() {
        bridge.pump(shell);
        std::thread::sleep(Duration::from_millis(2));
    }
    agent.join().unwrap()
}

fn connect(home: &Path, project: &Path, agent: &'static str) -> std::io::Result<Connection> {
    let dirs = UserDirs::under(home);
    let instance = client::instances(&dirs)
        .into_iter()
        .find(|i| i.project.as_deref() == Some(project))
        .expect("the app's instance file names its Project");
    Connection::connect_as(&instance, Source::Mcp, agent)
}

#[test]
fn the_bridge_is_on_by_default_and_the_preference_turns_it_off() {
    let home = TempDir::new("ui-bridge-switch");
    let work = TempDir::new("ui-bridge-switch-work");
    let project = make_project(work.path(), "Demo");
    let (mut shell, mut bridge) = app(home.path(), &project);
    let dirs = UserDirs::under(home.path());
    assert_eq!(client::instances(&dirs).len(), 1);
    assert_eq!(
        client::instances(&dirs)[0].project.as_deref(),
        Some(project.as_path())
    );

    bridge.set_enabled(&mut shell, false);
    assert!(!bridge.is_on());
    assert!(client::instances(&dirs).is_empty(), "the port is closed");
    assert!(!preferences::current(Some(&dirs)).agents.bridge);
    // A new window follows the Preference.
    assert!(!Bridge::start(&shell).is_on());

    bridge.set_enabled(&mut shell, true);
    assert!(bridge.is_on());
    assert_eq!(client::instances(&dirs).len(), 1);
    bridge.stop();
}

#[test]
fn a_new_agent_is_asked_about_and_an_allowed_one_is_remembered() {
    let home = TempDir::new("ui-bridge-allow");
    let work = TempDir::new("ui-bridge-allow-work");
    let project = make_project(work.path(), "Demo");
    let (mut shell, mut bridge) = app(home.path(), &project);
    let dirs = UserDirs::under(home.path());

    let (h, p) = (home.path().to_owned(), project.clone());
    let agent = std::thread::spawn(move || connect(&h, &p, "Claude").map(|_| ()));
    pump_until(&mut shell, &mut bridge, |b| b.asking().is_some());
    let asking = bridge.asking().unwrap();
    assert_eq!(asking.name, "Claude");
    assert_eq!(asking.source, Source::Mcp);
    bridge.answer(&mut shell, true);
    agent.join().unwrap().expect("allowed");
    assert_eq!(state::read(&dirs).allowed_agents, ["Claude"]);

    // The next connection of an allowed agent is let in without asking.
    let (h, p) = (home.path().to_owned(), project.clone());
    let mut connection = as_agent(&mut shell, &mut bridge, move || {
        connect(&h, &p, "Claude").expect("remembered")
    });
    assert!(bridge.asking().is_none());
    assert_eq!(bridge.agents().len(), 1, "{:?}", bridge.agents());

    // A denied agent is refused and not remembered.
    let (h, p) = (home.path().to_owned(), project.clone());
    let other = std::thread::spawn(move || connect(&h, &p, "Other").map(|_| ()));
    pump_until(&mut shell, &mut bridge, |b| b.asking().is_some());
    bridge.answer(&mut shell, false);
    let refused = other.join().unwrap().expect_err("denied");
    assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(state::read(&dirs).allowed_agents, ["Claude"]);

    // Forgetting an agent asks again next time.
    bridge.forget_agent(&mut shell, "Claude");
    assert!(state::read(&dirs).allowed_agents.is_empty());
    let _ = connection.list();
    drop(connection);
    bridge.stop();
}

/// Allows every agent named in `state.json` up front, so tests can skip the prompt.
fn allow(home: &Path, agent: &str) {
    state::update(&UserDirs::under(home), |s| {
        s.allowed_agents.push(agent.to_owned());
    })
    .unwrap();
}

#[test]
fn bridged_commands_apply_between_frames_labelled_by_source_and_never_save() {
    let home = TempDir::new("ui-bridge-run");
    let work = TempDir::new("ui-bridge-run-work");
    let project = make_project(work.path(), "Demo");
    allow(home.path(), "Claude");
    let (mut shell, mut bridge) = app(home.path(), &project);
    let before = read_tree(work.path());

    let (h, p) = (home.path().to_owned(), project.clone());
    let (catalogue, added, saved) = as_agent(&mut shell, &mut bridge, move || {
        let mut c = connect(&h, &p, "Claude").unwrap();
        let catalogue = c.list().unwrap();
        let added = c.run(
            "document.add",
            json!({"kind": "schematic", "title": "Power"}),
        );
        let saved = c.run("project.save", json!({}));
        (catalogue, added, saved)
    });
    // The app's own Commands are listed too.
    let ids: Vec<&str> = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"panel.show"), "app Commands are listed");
    assert!(ids.contains(&"document.add"));
    added.expect("the edit applies");
    assert_eq!(saved.unwrap_err().code, ErrorCode::Rejected);

    let titles: Vec<String> = shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .map(|d| d.title.clone())
        .collect();
    assert!(titles.contains(&"Power".to_owned()), "{titles:?}");
    let undo = shell.engine().history().next_undo(Scope::Project).unwrap();
    assert_eq!(undo.label, "MCP: Add Document");
    assert!(shell.engine().is_dirty());
    assert_eq!(read_tree(work.path()), before, "nothing was saved");
    let sources: Vec<Source> = shell
        .engine()
        .journal()
        .iter()
        .filter(|e| e.id == "document.add")
        .map(|e| e.source)
        .collect();
    assert_eq!(sources, [Source::Mcp]);
    bridge.stop();
}

#[test]
fn a_bridged_edit_to_a_tools_objects_cancels_its_preview_with_a_note() {
    let home = TempDir::new("ui-bridge-tool");
    let work = TempDir::new("ui-bridge-tool-work");
    let project = make_project(work.path(), "Demo");
    allow(home.path(), "Claude");
    let (mut shell, mut bridge) = app(home.path(), &project);
    let sheet = shell.active_document().unwrap().document;
    let set = shell
        .run(
            "schematic.sheet.set_frame",
            json!({"document": sheet, "size": "a3"}),
        )
        .unwrap();
    let frame: Uuid =
        serde_json::from_value(set["changes"][sheet.to_string()]["created"][0].clone()).unwrap();
    shell.begin_tool(ToolPreview::new("Move", [frame]));

    let bridged = |id: &'static str, params: Value| {
        let (h, p) = (home.path().to_owned(), project.clone());
        move || -> Result<Value, CommandError> {
            connect(&h, &p, "Claude").unwrap().run(id, params)
        }
    };
    // An edit that leaves the Tool's objects alone leaves the Tool be.
    let untouched = bridged("project.properties.set", json!({"title": "Amp"}));
    as_agent(&mut shell, &mut bridge, untouched).unwrap();
    assert_eq!(shell.tool_preview().map(|t| t.title.as_str()), Some("Move"));
    assert_eq!(shell.note(), None);

    let touching = bridged(
        "schematic.sheet.set_frame",
        json!({"document": sheet, "size": "a2"}),
    );
    as_agent(&mut shell, &mut bridge, touching).unwrap();
    assert!(shell.tool_preview().is_none(), "the preview is cancelled");
    let note = shell.note().expect("a status-bar note");
    assert!(note.contains("Move"), "{note}");
    bridge.stop();
}

#[test]
fn opening_a_document_file_focuses_its_tab_in_the_instance_holding_the_project() {
    let home = TempDir::new("ui-bridge-open");
    let work = TempDir::new("ui-bridge-open-work");
    let project = make_project(work.path(), "Demo");
    let folder = project.parent().unwrap().to_owned();
    let (mut shell, mut bridge) = app(home.path(), &project);
    let dirs = UserDirs::under(home.path());
    let pcb = shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .find(|d| d.kind == DocumentKind::Pcb)
        .map(|d| (d.uuid, d.name.clone()))
        .unwrap();
    let file = folder.join(format!("{}.redapcb", pcb.1));
    assert!(file.is_file());

    let (d, f) = (dirs.clone(), file.clone());
    let handed = as_agent(&mut shell, &mut bridge, move || bridge::hand_off(&d, &f));
    assert!(handed, "the running instance took it");
    assert!(bridge.take_focus_request(), "the window asks to be raised");
    assert_eq!(shell.layout().focus, Some(TabKey::Document(pcb.0)));
    // No prompt: it is the app itself, not an agent.
    assert!(bridge.asking().is_none());

    // A file of another Project starts a new instance instead.
    let other = make_project(work.path(), "Other");
    let other_sheet = std::fs::read_dir(other.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "redasch"))
        .unwrap();
    let d = dirs.clone();
    assert!(!as_agent(
        &mut shell,
        &mut bridge,
        move || bridge::hand_off(&d, &other_sheet)
    ));
    bridge.stop();
    // No instance at all: start a new one.
    assert!(!bridge::hand_off(&dirs, &file));
}

#[test]
fn the_window_shows_the_indicator_asks_about_an_agent_and_runs_its_commands() {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use reda_ui::App;

    let home = TempDir::new("ui-bridge-window");
    let work = TempDir::new("ui-bridge-window-work");
    let project = make_project(work.path(), "Demo");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&project).unwrap();
    let bridge = Bridge::start(&shell);
    let app = App::new(shell).with_bridge(bridge);
    let mut h = Harness::builder()
        .with_size([900.0, 560.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    h.run();
    assert!(h.query_by_label("Agents: on").is_some());

    let (hm, p) = (home.path().to_owned(), project.clone());
    let agent = std::thread::spawn(move || {
        let mut c = connect(&hm, &p, "Claude")?;
        c.run(
            "document.add",
            json!({"kind": "schematic", "title": "Power"}),
        )
        .map_err(std::io::Error::other)?;
        // Still connected for the indicator to count.
        Ok::<_, std::io::Error>(c)
    });
    let start = Instant::now();
    while h.query_by_label("Allow this agent?").is_none() {
        h.run();
        assert!(start.elapsed() < Duration::from_secs(20), "no prompt");
        std::thread::sleep(Duration::from_millis(5));
    }
    h.get_by_label("Allow").click();
    while !agent.is_finished() {
        h.run();
        std::thread::sleep(Duration::from_millis(5));
    }
    let _connection = agent.join().unwrap().expect("the edit applied");
    h.run();
    assert!(h.query_by_label("Agents: 1 connected").is_some());
    let titles: Vec<String> = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .documents()
        .map(|d| d.title.clone())
        .collect();
    assert!(titles.contains(&"Power".to_owned()), "{titles:?}");
    assert_eq!(
        state::read(&UserDirs::under(home.path())).allowed_agents,
        ["Claude"]
    );
}
