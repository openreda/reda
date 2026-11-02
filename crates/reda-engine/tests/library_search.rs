//! `library.search`: scanning the registered Libraries on disk (starter, user paths from
//! Preferences, Project paths from `.reda`) and searching their Parts.

use std::path::{Path, PathBuf};

use reda_engine::Engine;
use reda_engine::command::{FixedClock, SeededIds, UserDirs};
use reda_engine::format::library::{write_library_info, write_part};
use reda_engine::model::{ComponentType, Fields, LibraryInfo, Part, PinMap, Uuid};
use reda_testkit::{TempDir, TestEngine};
use serde_json::{Value, json};

/// The Demo Library of the file-format goldens: LM358 and MountingHole_M3.
fn demo_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/library/Demo")
}

fn engine(home: &Path, starter: Option<PathBuf>) -> TestEngine {
    TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .user_dirs(Some(UserDirs::under(home)))
            .starter_library(starter)
            .build(),
    )
}

/// Writes a one-Part Library into `folder`. Its Part uses the Demo Library's op-amp Symbol.
fn write_library(folder: &Path, n: u128, part_name: &str, value: &str) {
    let uuid = |m: u128| Uuid::v4_from_bits((n << 100) | m);
    std::fs::create_dir_all(folder.join("parts")).unwrap();
    let info = LibraryInfo {
        uuid: uuid(1),
        name: format!("Lib {n}"),
        description: String::new(),
    };
    std::fs::write(
        folder.join(format!("lib{n}.redalib")),
        write_library_info(&info).unwrap(),
    )
    .unwrap();
    let part = Part {
        uuid: uuid(2),
        name: part_name.into(),
        component_type: ComponentType::Standard,
        symbol: Some("00000010-0000-4000-8000-000000000000".parse().unwrap()),
        footprints: vec![],
        fields: Fields {
            value: value.into(),
            ..Fields::default()
        },
        pin_map: PinMap::default(),
        provenance: None,
    };
    std::fs::write(
        folder.join("parts").join(format!("{part_name}.redapart")),
        write_part(&part).unwrap(),
    )
    .unwrap();
}

fn names(out: &Value) -> Vec<&str> {
    out["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect()
}

#[test]
fn search_covers_the_starter_user_and_project_libraries() {
    let home = TempDir::new("search-home");
    let work = TempDir::new("search-work");
    let user_lib = work.path().join("user-lib");
    write_library(&user_lib, 1, "TL072", "TL072");
    let mut engine = engine(home.path(), Some(demo_library()));
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": [user_lib]}),
    );
    engine.run_ok("project.new", json!({"path": work.path(), "name": "Demo"}));
    write_library(
        &work.path().join("Demo").join("parts"),
        2,
        "NE5532",
        "NE5532",
    );
    engine.run_ok("project.library.add", json!({"path": "parts"}));

    let out = engine.run_ok("library.search", json!({}));
    assert_eq!(out["problems"], json!([]));
    assert_eq!(names(&out), ["LM358", "MountingHole_M3", "NE5532", "TL072"]);
    let lm358 = &out["parts"][0];
    assert_eq!(lm358["library"]["name"], "Demo");
    assert_eq!(lm358["library"]["origin"], "starter");
    assert_eq!(lm358["fields"]["mpn"], "LM358DR");
    assert_eq!(lm358["symbol"], "00000010-0000-4000-8000-000000000000");
    assert_eq!(out["parts"][2]["library"]["origin"], "project");
    assert_eq!(out["parts"][3]["library"]["origin"], "user");
    let libraries: Vec<_> = out["libraries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["origin"].as_str().unwrap())
        .collect();
    assert_eq!(libraries, ["starter", "user", "project"]);
}

#[test]
fn search_filters_by_text_library_and_component_type() {
    let home = TempDir::new("search-filter");
    let mut engine = engine(home.path(), Some(demo_library()));

    let out = engine.run_ok("library.search", json!({"text": "lm358dr"}));
    assert_eq!(names(&out), ["LM358"]);
    let out = engine.run_ok("library.search", json!({"component_type": "mechanical"}));
    assert_eq!(names(&out), ["MountingHole_M3"]);
    let demo = "00000001-0000-4000-8000-000000000000";
    let out = engine.run_ok("library.search", json!({"library": demo, "text": "hole"}));
    assert_eq!(names(&out), ["MountingHole_M3"]);
    let other = "00000002-0000-4000-8000-000000000000";
    let out = engine.run_ok("library.search", json!({"library": other}));
    assert_eq!(names(&out), Vec::<&str>::new());
}

#[test]
fn search_reports_libraries_it_cannot_use() {
    let home = TempDir::new("search-problems");
    let work = TempDir::new("search-problems-work");
    let mut engine = engine(home.path(), None);
    let missing = work.path().join("nowhere");
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": [missing]}),
    );

    let out = engine.run_ok("library.search", json!({}));
    assert_eq!(names(&out), Vec::<&str>::new());
    let problems = out["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0]["path"], json!(missing));
}

#[test]
fn search_works_with_no_project_and_no_user_folder() {
    let mut engine = reda_testkit::test_engine();
    let out = engine.run_ok("library.search", json!({"text": "x"}));
    assert_eq!(out, json!({"libraries": [], "parts": [], "problems": []}));
}
