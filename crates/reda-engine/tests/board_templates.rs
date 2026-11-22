//! `library.board_template.list` and `.preview`: Board Templates found by scanning the
//! registered Libraries, and evaluated through the one path creation also uses
//! (`docs/spec/v0.2.5/commands.md#library-commands`).

use std::path::{Path, PathBuf};

use reda_engine::Engine;
use reda_engine::command::{ErrorCode, FixedClock, SeededIds, UserDirs};
use reda_engine::format::library::write_library_info;
use reda_engine::model::{LibraryInfo, Uuid};
use reda_testkit::{TempDir, TestEngine, starter_library, test_engine_with_library};
use serde_json::{Value, json};

const CUSTOM_RECTANGLE: &str = "4b5a3569-5fcc-49b3-8af9-5d5fdda27d41";

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

/// The starter Custom rectangle's file as JSON.
fn custom_rectangle() -> Value {
    let path = starter_library().join("board-templates/Custom_rectangle.redaboard");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// A Library in `folder` whose `board-templates/` holds `templates` by file stem.
fn write_library(folder: &Path, n: u128, templates: &[(&str, Value)]) {
    std::fs::create_dir_all(folder.join("board-templates")).unwrap();
    let info = LibraryInfo {
        uuid: Uuid::v4_from_bits(n << 100),
        name: format!("Lib {n}"),
        description: String::new(),
    };
    std::fs::write(
        folder.join(format!("lib{n}.redalib")),
        write_library_info(&info).unwrap(),
    )
    .unwrap();
    for (stem, template) in templates {
        let bytes = serde_json::to_vec_pretty(template).unwrap();
        std::fs::write(
            folder.join(format!("board-templates/{stem}.redaboard")),
            bytes,
        )
        .unwrap();
    }
}

/// The Custom rectangle renamed, under a new UUID.
fn renamed(n: u128, name: &str) -> Value {
    let mut t = custom_rectangle();
    t["uuid"] = json!(Uuid::v4_from_bits(n).to_string());
    t["name"] = json!(name);
    t
}

#[test]
fn list_and_preview_work_with_no_project_open() {
    let mut engine = test_engine_with_library(&starter_library());
    let out = engine.run_ok("library.board_template.list", json!({}));
    let rect = out["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["element"] == CUSTOM_RECTANGLE)
        .expect("the starter Library ships Custom rectangle");
    assert_eq!(rect["name"], "Custom rectangle");
    assert_eq!(rect["category"], "Generic");
    assert_eq!(rect["origin"], "starter");
    assert_eq!(rect["suggested_layers"], 2);
    let params: Vec<(&str, &Value, &Value, &Value)> = rect["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["name"].as_str().unwrap(),
                &p["default"],
                &p["min"],
                &p["max"],
            )
        })
        .collect();
    assert_eq!(
        params,
        [
            (
                "width",
                &json!(100_000_000),
                &json!(1_000_000),
                &json!(1_000_000_000)
            ),
            (
                "height",
                &json!(80_000_000),
                &json!(1_000_000),
                &json!(1_000_000_000)
            ),
            ("corner_radius", &json!(0), &json!(0), &json!(500_000_000)),
        ]
    );
    assert_eq!(rect["parameters"][0]["kind"], "length");
    assert_eq!(rect["parameters"][0]["unit"], "mm");
    assert_eq!(
        rect["objects"],
        json!([{"uuid": "46545762-f814-4177-bf02-4321128791b9", "kind": "outline"}])
    );

    let frame = engine.run_ok(
        "library.board_template.preview",
        json!({"element": CUSTOM_RECTANGLE, "values": {"width": "65mm", "corner_radius": 0}}),
    );
    assert_eq!(
        frame["outline"]["vertices"],
        json!([
            {"x": 0, "y": 0},
            {"x": 65_000_000, "y": 0},
            {"x": 65_000_000, "y": 80_000_000},
            {"x": 0, "y": 80_000_000}
        ])
    );
    assert_eq!(
        frame["values"],
        json!({"corner_radius": 0, "height": 80_000_000, "width": 65_000_000})
    );
    assert_eq!(frame["problems"], json!([]));
}

#[test]
fn preview_refuses_bad_input_and_reports_frame_problems() {
    let mut engine = test_engine_with_library(&starter_library());
    let preview = |engine: &mut TestEngine, values: Value| {
        engine.run_err(
            "library.board_template.preview",
            json!({"element": CUSTOM_RECTANGLE, "values": values}),
        )
    };
    let err = preview(&mut engine, json!({"width": "0.5mm"}));
    assert_eq!(err.code, ErrorCode::InvalidParams, "{err:?}");
    let err = preview(&mut engine, json!({"lanes": "x4"}));
    assert_eq!(err.code, ErrorCode::InvalidParams, "{err:?}");
    let err = engine.run_err(
        "library.board_template.preview",
        json!({"element": CUSTOM_RECTANGLE, "exclude": ["46545762-f814-4177-bf02-4321128791b9"]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams, "{err:?}");
    let err = engine.run_err(
        "library.board_template.preview",
        json!({"element": Uuid::v4_from_bits(7).to_string()}),
    );
    assert_eq!(err.code, ErrorCode::NotFound, "{err:?}");

    // An invalid frame is still a frame: the preview marks the problem.
    let frame = engine.run_ok(
        "library.board_template.preview",
        json!({"element": CUSTOM_RECTANGLE, "values": {"width": "10", "corner_radius": "6"}}),
    );
    let problems = frame["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0]["kind"], "outline");
    assert!(problems[0]["at"].is_object(), "{problems:?}");
}

#[test]
fn templates_come_from_every_registered_library_in_resolution_order() {
    let home = TempDir::new("templates-home");
    let work = TempDir::new("templates-work");
    let user_lib = work.path().join("user-lib");
    write_library(&user_lib, 1, &[("Wide", renamed(11, "Wide board"))]);
    let mut engine = engine(home.path(), Some(starter_library()));
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": [user_lib]}),
    );
    engine.run_ok("project.new", json!({"path": work.path(), "name": "Demo"}));
    write_library(
        &work.path().join("Demo").join("lib"),
        2,
        &[("Mine", renamed(12, "Project board"))],
    );
    engine.run_ok("project.library.add", json!({"path": "lib"}));

    let out = engine.run_ok("library.board_template.list", json!({}));
    assert_eq!(out["problems"], json!([]));
    let listed: Vec<(&str, &str)> = out["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (t["name"].as_str().unwrap(), t["origin"].as_str().unwrap()))
        .collect();
    assert_eq!(
        listed[..2],
        [("Project board", "project"), ("Wide board", "user")]
    );
    assert!(
        listed[2..].iter().all(|(_, origin)| *origin == "starter"),
        "{listed:?}"
    );
    assert!(listed.contains(&("Custom rectangle", "starter")));

    let only = Uuid::v4_from_bits(1 << 100).to_string();
    let out = engine.run_ok("library.board_template.list", json!({"library": only}));
    assert_eq!(out["templates"].as_array().unwrap().len(), 1);

    // An inactive Library is not scanned.
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.inactive", "value": [user_lib]}),
    );
    let out = engine.run_ok("library.board_template.list", json!({}));
    assert!(!out["templates"].to_string().contains("Wide board"));
}

#[test]
fn a_template_that_fails_its_load_checks_is_a_problem_and_cannot_be_picked() {
    let home = TempDir::new("templates-broken-home");
    let work = TempDir::new("templates-broken-work");
    let user_lib = work.path().join("user-lib");
    let mut broken = renamed(21, "Broken");
    broken["outline"][0]["vertices"][1]["x"] = json!("width + depth");
    let mut unknown_field = renamed(22, "Unknown field");
    unknown_field["colour"] = json!("green");
    let clash = renamed(23, "Wide board");
    write_library(
        &user_lib,
        1,
        &[
            ("Broken", broken),
            ("Unknown", unknown_field),
            ("Wide", renamed(24, "Wide board")),
            ("Wide2", clash),
        ],
    );
    let mut engine = engine(home.path(), None);
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": [user_lib]}),
    );
    let out = engine.run_ok("library.board_template.list", json!({}));
    let names: Vec<&str> = out["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Wide board"]);
    let problems: Vec<String> = out["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            format!(
                "{} {}",
                p["path"].as_str().unwrap(),
                p["message"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(problems.len(), 3, "{problems:#?}");
    assert!(
        problems[0].contains("Broken.redaboard") && problems[0].contains("`depth`"),
        "{problems:#?}"
    );
    assert!(
        problems[1].contains("Unknown.redaboard") && problems[1].contains("colour"),
        "{problems:#?}"
    );
    assert!(
        problems[2].contains("Wide2.redaboard") && problems[2].contains("already named"),
        "{problems:#?}"
    );

    let err = engine.run_err(
        "library.board_template.preview",
        json!({"element": Uuid::v4_from_bits(21).to_string()}),
    );
    assert_eq!(err.code, ErrorCode::NotFound, "{err:?}");
}

const PCIE: &str = "4ca6b4ca-11f8-45f1-96c1-c610ece56af9";

/// The PCIe card's `card_length` starts at the chosen lane width's tab end, 57.15 mm plus
/// that width's finger run (#416 decision): the list gives one range per lane width, and
/// preview and creation refuse a length below it as out of range.
#[test]
fn a_length_range_that_names_a_choice_is_listed_per_option_and_enforced() {
    let mut engine = test_engine_with_library(&starter_library());
    let out = engine.run_ok("library.board_template.list", json!({}));
    let pcie = out["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["element"] == PCIE)
        .expect("the starter Library ships the PCIe card");
    let length = pcie["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "card_length")
        .unwrap();
    // At the default x1: from the x1 tab end, 57.15 + 8.15.
    assert_eq!(length["min"], 65_300_000);
    assert_eq!(length["max"], 167_650_000);
    assert_eq!(
        length["ranges"],
        json!([
            {"when": {"lanes": ["x1"]}, "min": 65_300_000, "max": 167_650_000},
            {"when": {"lanes": ["x4"]}, "min": 79_300_000, "max": 167_650_000},
            {"when": {"lanes": ["x8"]}, "min": 96_300_000, "max": 167_650_000},
            {"when": {"lanes": ["x16"]}, "min": 129_300_000, "max": 167_650_000}
        ])
    );
    // A constant range is one range with no gate.
    let rect = out["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["element"] == CUSTOM_RECTANGLE)
        .unwrap();
    assert_eq!(
        rect["parameters"][0]["ranges"],
        json!([{"when": {}, "min": 1_000_000, "max": 1_000_000_000}])
    );

    let err = engine.run_err(
        "library.board_template.preview",
        json!({"element": PCIE, "values": {"lanes": "x16", "card_length": "100"}}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams, "{err:?}");
    assert!(
        err.message
            .contains("out of range: from 129.3 to 167.65 mm at lanes=x16, not 100 mm"),
        "{err:?}"
    );
    let frame = engine.run_ok(
        "library.board_template.preview",
        json!({"element": PCIE, "values": {"lanes": "x1", "card_length": "100"}}),
    );
    assert_eq!(frame["problems"], json!([]));
    let frame = engine.run_ok(
        "library.board_template.preview",
        json!({"element": PCIE, "values": {"lanes": "x16", "card_length": "129.3"}}),
    );
    assert_eq!(frame["problems"], json!([]));
}
