//! `state.json`: machine-written per-user state, updated by read-merge-write.

use std::path::Path;

use reda_engine::command::UserDirs;
use reda_engine::state::{self, UserState, WindowGeometry};
use reda_testkit::TempDir;
use serde_json::json;

#[test]
fn a_missing_file_reads_as_empty_state() {
    let home = TempDir::new("state-missing");
    let dirs = UserDirs::under(home.path());
    assert_eq!(state::read(&dirs), UserState::default());
    assert!(!dirs.state_file().exists(), "reading never writes");
}

#[test]
fn update_writes_recent_projects_allowed_agents_and_window_geometry() {
    let home = TempDir::new("state-write");
    let dirs = UserDirs::under(home.path());
    let written = state::update(&dirs, |s| {
        s.add_recent_project(Path::new("/p/Alpha"));
        s.allowed_agents.push("claude-code".into());
        s.window = Some(WindowGeometry {
            x: 10,
            y: 20,
            width: 1280,
            height: 800,
            maximized: false,
        });
    })
    .expect("written");
    assert_eq!(state::read(&dirs), written);

    let on_disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dirs.state_file()).unwrap()).unwrap();
    assert_eq!(
        on_disk,
        json!({
            "recent_projects": ["/p/Alpha"],
            "allowed_agents": ["claude-code"],
            "window": {"x": 10, "y": 20, "width": 1280, "height": 800, "maximized": false},
        })
    );
}

#[test]
fn update_merges_with_what_another_instance_wrote() {
    let home = TempDir::new("state-merge");
    let dirs = UserDirs::under(home.path());
    state::update(&dirs, |s| s.allowed_agents.push("agent-a".into())).unwrap();
    // Another instance writes in between, including a field this version does not know.
    std::fs::write(
        dirs.state_file(),
        r#"{"allowed_agents": ["agent-a", "agent-b"], "from_the_future": {"x": 1}}"#,
    )
    .unwrap();
    state::update(&dirs, |s| s.add_recent_project(Path::new("/p/Beta"))).unwrap();

    let on_disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dirs.state_file()).unwrap()).unwrap();
    assert_eq!(on_disk["allowed_agents"], json!(["agent-a", "agent-b"]));
    assert_eq!(on_disk["recent_projects"], json!(["/p/Beta"]));
    assert_eq!(on_disk["from_the_future"], json!({"x": 1}));
}

#[test]
fn recent_projects_are_newest_first_without_duplicates_and_capped() {
    let mut s = UserState::default();
    for i in 0..15 {
        s.add_recent_project(Path::new(&format!("/p/{i}")));
    }
    s.add_recent_project(Path::new("/p/12"));
    assert_eq!(s.recent_projects.len(), state::MAX_RECENT_PROJECTS);
    assert_eq!(s.recent_projects[0], Path::new("/p/12"));
    assert_eq!(s.recent_projects[1], Path::new("/p/14"));
    assert_eq!(
        s.recent_projects
            .iter()
            .filter(|p| *p == Path::new("/p/12"))
            .count(),
        1
    );
}

#[test]
fn an_unreadable_file_is_replaced_on_the_next_update() {
    let home = TempDir::new("state-corrupt");
    let dirs = UserDirs::under(home.path());
    std::fs::create_dir_all(&dirs.data).unwrap();
    std::fs::write(dirs.state_file(), "{ not json").unwrap();
    assert_eq!(state::read(&dirs), UserState::default());
    state::update(&dirs, |s| s.allowed_agents.push("a".into())).unwrap();
    assert_eq!(state::read(&dirs).allowed_agents, ["a"]);
}
