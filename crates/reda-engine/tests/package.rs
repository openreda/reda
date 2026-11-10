//! `project.package`: the Project folder as one zip, `.reda/` left out, with relative
//! paths, the same bytes every time.

use std::io::Read;
use std::path::{Path, PathBuf};

use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::json;

/// A saved Project `Demo` with a Project Library `lib` inside its folder, an output file
/// under the Output Job's folder, and the recovery and view state REDA keeps in `.reda/`.
fn project(dir: &Path) -> (TestEngine, PathBuf) {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir, "name": "Demo"}));
    let folder = dir.join("Demo");
    engine.run_ok("library.create", json!({"path": "lib", "name": "Parts"}));
    engine.run_ok("project.library.add", json!({"path": "lib"}));
    engine.run_ok("project.save", json!({}));
    std::fs::create_dir_all(folder.join("outputs/Generic")).unwrap();
    std::fs::write(folder.join("outputs/Generic/Demo.gtl"), b"G04*\n").unwrap();
    std::fs::write(folder.join(".reda/view.json"), b"{}").unwrap();
    std::fs::write(folder.join("notes.txt"), b"rev A\n").unwrap();
    (engine, folder)
}

/// Every entry of the zip at `path`: its name and contents.
fn entries(path: &Path) -> Vec<(String, Vec<u8>)> {
    let file = std::fs::File::open(path).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut entry = zip.by_index(i).unwrap();
            let mut bytes = vec![];
            entry.read_to_end(&mut bytes).unwrap();
            (entry.name().unwrap().to_string(), bytes)
        })
        .collect()
}

fn names(path: &Path) -> Vec<String> {
    entries(path).into_iter().map(|(name, _)| name).collect()
}

#[test]
fn the_package_holds_the_project_folder_without_its_state_and_outputs() {
    let dir = TempDir::new("package");
    let (mut engine, folder) = project(dir.path());
    let zip = dir.path().join("Demo.zip");
    let out = engine.run_ok("project.package", json!({"path": zip}));
    assert_eq!(
        names(&zip),
        [
            "Demo.reda",
            "Demo.redapcb",
            "Demo.redasch",
            "lib/Parts.redalib",
            "notes.txt"
        ]
    );
    assert_eq!(out["files"].as_array().unwrap().len(), 5, "{out}");
    assert!(out.get("outside_libraries").is_none(), "{out}");
    // The files go in as they are on disk.
    let reda = std::fs::read(folder.join("Demo.reda")).unwrap();
    assert!(entries(&zip).contains(&("Demo.reda".to_owned(), reda)));
}

#[test]
fn include_outputs_adds_the_output_jobs_folders() {
    let dir = TempDir::new("package-outputs");
    let (mut engine, _) = project(dir.path());
    let zip = dir.path().join("Demo.zip");
    engine.run_ok(
        "project.package",
        json!({"path": zip, "include_outputs": true}),
    );
    assert!(names(&zip).contains(&"outputs/Generic/Demo.gtl".to_owned()));
}

#[test]
fn packaging_twice_gives_the_same_bytes_and_leaves_out_the_package_itself() {
    let dir = TempDir::new("package-twice");
    let (mut engine, folder) = project(dir.path());
    // Written into the Project folder: it must not package itself.
    let zip = folder.join("Demo.zip");
    engine.run_ok("project.package", json!({"path": zip}));
    let first = std::fs::read(&zip).unwrap();
    assert!(!names(&zip).contains(&"Demo.zip".to_owned()));
    engine.run_ok("project.package", json!({"path": zip}));
    assert_eq!(std::fs::read(&zip).unwrap(), first);

    // The same files elsewhere, at another time, give the same zip.
    let copy = TempDir::new("package-twice-copy");
    let mut other = test_engine();
    let (_, _) = project(copy.path());
    other.run_ok("project.open", json!({"path": copy.path().join("Demo")}));
    let zip2 = copy.path().join("Demo.zip");
    other.run_ok("project.package", json!({"path": zip2}));
    assert_eq!(std::fs::read(&zip2).unwrap(), first);
}

#[test]
fn a_library_outside_the_project_folder_is_reported_and_left_out() {
    let dir = TempDir::new("package-outside");
    let (mut engine, _) = project(dir.path());
    engine.run_ok(
        "library.create",
        json!({"path": "../shared", "name": "Shared"}),
    );
    engine.run_ok("project.library.add", json!({"path": "../shared"}));
    engine.run_ok("project.save", json!({}));
    let zip = dir.path().join("Demo.zip");
    let out = engine.run_ok("project.package", json!({"path": zip}));
    assert_eq!(out["outside_libraries"], json!(["../shared"]));
    assert!(names(&zip).iter().all(|n| !n.contains("Shared")));
}

#[test]
fn unsaved_changes_are_reported_since_the_files_on_disk_are_packaged() {
    let dir = TempDir::new("package-unsaved");
    let (mut engine, _) = project(dir.path());
    engine.run_ok("document.add", json!({"kind": "schematic"}));
    let zip = dir.path().join("Demo.zip");
    let out = engine.run_ok("project.package", json!({"path": zip}));
    assert_eq!(out["unsaved"], true);
    assert!(!names(&zip).contains(&"sheet-2.redasch".to_owned()));
}

#[test]
fn packaging_needs_a_project() {
    let mut engine = test_engine();
    let err = engine.run_err("project.package", json!({"path": "x.zip"}));
    assert_eq!(err.code, reda_engine::command::ErrorCode::Unavailable);
}
