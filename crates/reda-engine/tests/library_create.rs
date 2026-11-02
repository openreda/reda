//! `library.create`: making a new, empty Library folder with its `.redalib` file, for the
//! user (registered in Preferences) or for a Project (registered with
//! `project.library.add` afterwards, on the Project's undo stack).

use std::path::Path;

use reda_engine::Engine;
use reda_engine::command::{ErrorCode, FixedClock, SeededIds, UserDirs};
use reda_testkit::{TempDir, TestEngine};
use serde_json::json;

fn engine(home: &Path) -> TestEngine {
    TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .user_dirs(Some(UserDirs::under(home)))
            .starter_library(None)
            .build(),
    )
}

fn library_names(engine: &mut TestEngine) -> Vec<String> {
    let out = engine.run_ok("library.search", json!({}));
    out["libraries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_user_library_is_written_and_registered_in_preferences() {
    let home = TempDir::new("lib-create-home");
    let folder = home.path().join("libs/Mine");
    let mut engine = engine(home.path());

    let out = engine.run_ok(
        "library.create",
        json!({"path": folder, "name": "Mine", "description": "My parts", "register": "user"}),
    );
    assert_eq!(out["name"], "Mine");
    assert!(out["library"].as_str().is_some());
    let text = std::fs::read_to_string(folder.join("Mine.redalib")).unwrap();
    assert!(text.contains("\"Mine\""), "{text}");
    assert!(text.contains("My parts"), "{text}");

    let prefs = engine.run_ok("app.preferences.get", json!({"key": "libraries.paths"}));
    assert_eq!(prefs["value"], json!([folder.to_str().unwrap()]));
    assert_eq!(library_names(&mut engine), ["Mine"]);
}

#[test]
fn a_project_library_resolves_against_the_project_folder_and_can_be_added() {
    let home = TempDir::new("lib-create-proj-home");
    let dir = TempDir::new("lib-create-proj");
    let mut engine = engine(home.path());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));

    let out = engine.run_ok(
        "library.create",
        json!({"path": "libs/Board", "name": "Board"}),
    );
    assert!(dir.path().join("Demo/libs/Board/Board.redalib").is_file());
    assert_eq!(out["project_path"], "libs/Board");
    // Nothing is registered until the Project edit runs.
    assert!(library_names(&mut engine).is_empty());
    assert!(
        !engine
            .dirty_scopes()
            .contains(&reda_engine::model::Scope::Project)
    );

    engine.run_ok("project.library.add", json!({"path": out["project_path"]}));
    assert_eq!(library_names(&mut engine), ["Board"]);
}

#[test]
fn an_existing_library_or_a_bad_name_is_refused() {
    let home = TempDir::new("lib-create-refuse");
    let folder = home.path().join("L");
    let mut engine = engine(home.path());
    engine.run_ok("library.create", json!({"path": folder, "name": "L"}));

    let err = engine.run_err("library.create", json!({"path": folder, "name": "Other"}));
    assert_eq!(err.code, ErrorCode::Rejected);
    let err = engine.run_err(
        "library.create",
        json!({"path": home.path().join("M"), "name": "a/b"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    // A relative folder needs an open Project to resolve against.
    let err = engine.run_err("library.create", json!({"path": "rel", "name": "R"}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
}
