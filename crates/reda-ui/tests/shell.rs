//! The window shell's state, without a window: the Start page and its recent Projects,
//! opening and creating Projects, Document tabs, and handing installed fonts to the engine.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reda_testkit::TempDir;
use reda_ui::engine::command::{FontFace, UserDirs};
use reda_ui::engine::{Engine, state};
use reda_ui::shell::{InstalledFonts, Shell, Tab};
use serde_json::json;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

/// Creates a Project named `name` in `folder` with a throwaway engine; returns its file.
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

#[test]
fn with_no_project_the_shell_shows_only_the_start_page() {
    let home = TempDir::new("shell-home");
    let shell = Shell::new(engine_in(home.path()));
    assert_eq!(shell.tabs(), [Tab::Start]);
    assert_eq!(shell.active(), Some(&Tab::Start));
}

#[test]
fn the_start_page_lists_recent_projects_and_greys_missing_ones() {
    let home = TempDir::new("shell-home");
    let work = TempDir::new("shell-work");
    let present = make_project(work.path(), "Alpha");
    let gone = work.path().join("Gone/Gone.reda");
    let dirs = UserDirs::under(home.path());
    state::update(&dirs, |s| {
        s.recent_projects = vec![present.clone(), gone.clone()];
    })
    .unwrap();

    let shell = Shell::new(engine_in(home.path()));
    let recent = shell.recent_projects();
    let rows: Vec<_> = recent
        .iter()
        .map(|r| (r.name.as_str(), r.path.clone(), r.missing))
        .collect();
    assert_eq!(rows, [("Alpha", present, false), ("Gone", gone, true)]);
}

#[test]
fn removing_a_recent_project_forgets_it_in_state_json() {
    let home = TempDir::new("shell-home");
    let dirs = UserDirs::under(home.path());
    let a = PathBuf::from("/nowhere/A/A.reda");
    let b = PathBuf::from("/nowhere/B/B.reda");
    state::update(&dirs, |s| s.recent_projects = vec![a.clone(), b.clone()]).unwrap();

    let mut shell = Shell::new(engine_in(home.path()));
    shell.remove_recent(&a);
    assert_eq!(state::read(&dirs).recent_projects, std::slice::from_ref(&b));
    assert_eq!(shell.recent_projects().len(), 1);
}

#[test]
fn opening_a_project_closes_the_start_page_and_shows_the_first_sheet() {
    let home = TempDir::new("shell-home");
    let work = TempDir::new("shell-work");
    let file = make_project(work.path(), "Board");
    let mut shell = Shell::new(engine_in(home.path()));

    shell.open_project(&file).unwrap();

    let project = shell.engine().project().unwrap();
    let first_sheet = project.documents().next().unwrap().clone();
    match shell.tabs() {
        [Tab::Document(tab)] => {
            assert_eq!(tab.document, first_sheet.uuid);
            assert_eq!(tab.title, "Sheet 1");
            assert!(
                !tab.scene.scene.items.is_empty(),
                "the sheet frame is drawn"
            );
        }
        tabs => panic!("expected one sheet tab, got {tabs:?}"),
    }
    // The Project goes to the top of the recent list.
    let dirs = UserDirs::under(home.path());
    let recent = &state::read(&dirs).recent_projects;
    assert_eq!(recent[0], std::fs::canonicalize(&file).unwrap());
}

#[test]
fn a_project_that_does_not_open_leaves_the_start_page_with_a_message() {
    let home = TempDir::new("shell-home");
    let mut shell = Shell::new(engine_in(home.path()));
    let err = shell.open_project(Path::new("/no/such/Project.reda"));
    assert!(err.is_err());
    assert_eq!(shell.tabs(), [Tab::Start]);
    assert!(shell.message().is_some());
}

#[test]
fn a_new_project_is_created_and_opened() {
    let home = TempDir::new("shell-home");
    let work = TempDir::new("shell-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Fresh").unwrap();
    assert!(work.path().join("Fresh/Fresh.reda").is_file());
    assert!(matches!(shell.tabs(), [Tab::Document(_)]));
    assert_eq!(shell.recent_projects()[0].name, "Fresh");
}

#[test]
fn an_edit_rebuilds_the_scene_of_open_tabs() {
    let home = TempDir::new("shell-home");
    let work = TempDir::new("shell-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Edits").unwrap();
    let Some(Tab::Document(before)) = shell.active().cloned() else {
        panic!("no sheet tab")
    };
    shell
        .run(
            "schematic.wire.add",
            json!({"document": before.document,
                   "points": [{"x": "10mm", "y": "10mm"}, {"x": "30mm", "y": "10mm"}]}),
        )
        .unwrap();
    let Some(Tab::Document(after)) = shell.active() else {
        panic!("no sheet tab")
    };
    assert_ne!(after.scene.generation, before.scene.generation);
    assert!(after.scene.scene.items.len() > before.scene.scene.items.len());

    // A query changes nothing, so the Scene is kept.
    let generation = after.scene.generation;
    shell.run("project.overview", json!({})).unwrap();
    let Some(Tab::Document(same)) = shell.active() else {
        panic!("no sheet tab")
    };
    assert_eq!(same.scene.generation, generation);
}

#[derive(Default)]
struct FakeFonts {
    asked: Mutex<Vec<String>>,
}

impl InstalledFonts for FakeFonts {
    fn load(&self, family: &str) -> Option<FontFace> {
        self.asked.lock().unwrap().push(family.to_owned());
        (family == "Fancy Sans").then(|| FontFace {
            family: family.to_owned(),
            bytes: Arc::from(&b"not really a font"[..]),
            index: 0,
        })
    }
}

#[test]
fn fonts_named_by_pcb_text_are_loaded_from_the_machine_once() {
    let home = TempDir::new("shell-home");
    let work = TempDir::new("shell-work");
    let fonts = Arc::new(FakeFonts::default());
    let mut shell = Shell::new(engine_in(home.path())).with_fonts(fonts.clone());
    shell.new_project(work.path(), "Texts").unwrap();
    for (text, font) in [
        ("A", "Fancy Sans"),
        ("B", "Fancy Sans"),
        ("C", "Nowhere Mono"),
    ] {
        shell
            .run(
                "pcb.text.add",
                json!({"layer": "silkscreen_top", "text": text, "font": font,
                       "x": "1mm", "y": "1mm"}),
            )
            .unwrap();
    }
    shell
        .run(
            "pcb.text.add",
            json!({"layer": "silkscreen_top", "text": "D",
                                     "x": "1mm", "y": "1mm"}),
        )
        .unwrap();

    let face = shell.engine().fonts().get("fancy sans").expect("loaded");
    assert_eq!(&*face.bytes, b"not really a font");
    assert!(shell.engine().fonts().get("Nowhere Mono").is_none());
    // Each family is asked for once, and the built-in fonts never.
    assert_eq!(*fonts.asked.lock().unwrap(), ["Fancy Sans", "Nowhere Mono"]);
}
