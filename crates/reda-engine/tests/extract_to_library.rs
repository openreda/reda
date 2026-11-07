//! `library.extract_from_project`: Extract to Library writes Components' Parts, Symbols
//! and Footprints from the part table into a writable Library, keeping their UUIDs, so
//! the Library copy matches the placed one. With no `components` it is Make Project
//! Library, which skips and lists the elements that Library already holds.

use std::path::{Path, PathBuf};

use reda_engine::Engine;
use reda_engine::command::{ErrorCode, FixedClock, SeededIds, UserDirs};
use reda_testkit::{TempDir, TestEngine, starter_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const NPN: &str = "614301c4-68c2-4d22-998f-5c9f4d2dbd19";
const NPN_SYMBOL: &str = "4b09f27b-e1a9-46d0-af71-1fa9775f2c85";
const SOT23: &str = "43ca8637-7f8d-4597-866a-d91d359dd6bb";
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";

struct Fixture {
    engine: TestEngine,
    /// The Project Library's UUID and folder.
    library: String,
    folder: PathBuf,
    _dir: TempDir,
}

/// A Project with NPN placed twice and a resistor once, and an empty Project Library
/// `lib`.
fn fixture(name: &str) -> Fixture {
    let dir = TempDir::new(name);
    let mut engine = TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
            .starter_library(Some(starter_library()))
            .build(),
    );
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    for (part, x) in [(NPN, 0), (NPN, 20), (RESISTOR, 40)] {
        engine.run_ok(
            "schematic.component.place",
            json!({"document": SHEET, "part": part, "at": {"x": format!("{x}mm"), "y": "0mm"}}),
        );
    }
    let out = engine.run_ok(
        "library.create",
        json!({"path": "lib", "name": "Demo Parts"}),
    );
    engine.run_ok("project.library.add", json!({"path": "lib"}));
    Fixture {
        library: out["library"].as_str().unwrap().to_owned(),
        folder: dir.path().join("Demo/lib"),
        engine,
        _dir: dir,
    }
}

/// The Component UUID of the Component with this Designator.
fn component(engine: &mut TestEngine, part: &str, designator: &str) -> Value {
    let used = engine.run_ok("library.where_used", json!({"element": part}));
    used["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["designator"] == designator)
        .unwrap()["component"]
        .clone()
}

/// The `(kind, name)` of each listed element, sorted.
fn listed(out: &Value, key: &str) -> Vec<(String, String)> {
    let mut list: Vec<_> = out[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["kind"].as_str().unwrap().to_owned(),
                e["name"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    list.sort();
    list
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(k, n)| ((*k).to_owned(), (*n).to_owned()))
        .collect()
}

fn files_in(folder: &Path) -> Vec<String> {
    let mut files = vec![];
    for sub in ["symbols", "footprints", "parts"] {
        if let Ok(entries) = std::fs::read_dir(folder.join(sub)) {
            for e in entries.flatten() {
                files.push(format!("{sub}/{}", e.file_name().to_string_lossy()));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn extracting_a_component_keeps_uuids_so_its_status_is_up_to_date() {
    let mut f = fixture("extract-one");
    let q1 = component(&mut f.engine, NPN, "Q1");
    let out = f.engine.run_ok(
        "library.extract_from_project",
        json!({"components": [q1], "library": f.library}),
    );
    assert_eq!(
        listed(&out, "written"),
        pairs(&[
            ("footprint", "SOT95P237X112-3N"),
            ("part", "NPN"),
            ("symbol", "NPN")
        ])
    );
    assert_eq!(out["skipped"], json!([]));
    let uuids: Vec<_> = out["written"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["element"].as_str().unwrap())
        .collect();
    for uuid in [NPN, NPN_SYMBOL, SOT23] {
        assert!(uuids.contains(&uuid), "{uuid} not kept: {uuids:?}");
    }
    assert_eq!(
        files_in(&f.folder),
        [
            "footprints/SOT95P237X112-3N.redafp",
            "parts/NPN.redapart",
            "symbols/NPN.redasym"
        ]
    );

    // With the Starter Library off, the Part is found only in the Project Library, and
    // its copy there matches the placed one: Up to Date, not outdated or missing.
    f.engine.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.starter_active", "value": false}),
    );
    let used = f
        .engine
        .run_ok("library.where_used", json!({"element": NPN}));
    assert_eq!(used["library"]["origin"], "project");
    let outdated = f.engine.run_ok("schematic.component.outdated", json!({}));
    assert_eq!(outdated["components"], json!([]));
    let search = f.engine.run_ok("library.search", json!({}));
    assert_eq!(search["problems"], json!([]));

    // Changing the extracted Part makes the Component outdated, so it is compared.
    f.engine
        .run_ok("library.element.open", json!({"element": NPN}));
    f.engine
        .run_ok("library.part.edit", json!({"part": NPN, "in_cpl": false}));
    f.engine
        .run_ok("library.element.save", json!({"element": NPN}));
    let outdated = f.engine.run_ok("schematic.component.outdated", json!({}));
    let names: Vec<_> = outdated["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["designator"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Q1", "Q2"]);
}

#[test]
fn extracting_a_component_whose_part_the_library_holds_is_refused() {
    let mut f = fixture("extract-refused");
    let q1 = component(&mut f.engine, NPN, "Q1");
    let q2 = component(&mut f.engine, NPN, "Q2");
    let params = json!({"components": [q1], "library": f.library});
    f.engine.run_ok("library.extract_from_project", params);
    let before = files_in(&f.folder);
    let r1 = component(&mut f.engine, RESISTOR, "R1");
    let err = f.engine.run_err(
        "library.extract_from_project",
        json!({"components": [r1, q2], "library": f.library}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
    assert!(err.message.contains("NPN"), "{}", err.message);
    // Nothing was written, not even the resistor's elements.
    assert_eq!(files_in(&f.folder), before);
}

#[test]
fn a_name_another_element_has_in_the_library_is_refused() {
    let mut f = fixture("extract-name");
    // Another Symbol called NPN, under a new UUID.
    f.engine.run_ok(
        "library.element.copy",
        json!({"element": NPN_SYMBOL, "library": f.library}),
    );
    let q1 = component(&mut f.engine, NPN, "Q1");
    let err = f.engine.run_err(
        "library.extract_from_project",
        json!({"components": [q1], "library": f.library}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
    assert!(err.message.contains("NPN"), "{}", err.message);
    assert_eq!(files_in(&f.folder), ["symbols/NPN.redasym"]);
}

#[test]
fn make_project_library_extracts_every_component_and_lists_what_was_held() {
    let mut f = fixture("extract-all");
    let q1 = component(&mut f.engine, NPN, "Q1");
    f.engine.run_ok(
        "library.extract_from_project",
        json!({"components": [q1], "library": f.library}),
    );
    let out = f.engine.run_ok(
        "library.extract_from_project",
        json!({"library": f.library}),
    );
    assert_eq!(
        listed(&out, "skipped"),
        pairs(&[
            ("footprint", "SOT95P237X112-3N"),
            ("part", "NPN"),
            ("symbol", "NPN")
        ])
    );
    let written = listed(&out, "written");
    let kinds: Vec<_> = written.iter().map(|(k, _)| k.as_str()).collect();
    // The resistor: its Part, its Symbol and all five alternate Footprints.
    assert_eq!(kinds.iter().filter(|k| **k == "footprint").count(), 5);
    assert!(written.contains(&("part".to_owned(), "Resistor".to_owned())));
    assert_eq!(written.len(), 7);
    assert_eq!(files_in(&f.folder).len(), 10);

    // Running it again has nothing left to write.
    let again = f.engine.run_ok(
        "library.extract_from_project",
        json!({"library": f.library}),
    );
    assert_eq!(again["written"], json!([]));
    assert_eq!(again["skipped"].as_array().unwrap().len(), 10);
}

#[test]
fn the_starter_library_and_unknown_components_are_refused() {
    let mut f = fixture("extract-bad");
    let starter = f
        .engine
        .run_ok("library.where_used", json!({"element": NPN}))["library"]["uuid"]
        .clone();
    let err = f
        .engine
        .run_err("library.extract_from_project", json!({"library": starter}));
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
    let nobody = "00000000-0000-4000-8000-0000000000ff";
    let err = f.engine.run_err(
        "library.extract_from_project",
        json!({"components": [nobody], "library": f.library}),
    );
    assert_eq!(err.code, ErrorCode::NotFound, "{err:?}");
    assert_eq!(files_in(&f.folder), Vec::<String>::new());
}
