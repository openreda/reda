//! Per-Project view state: `.reda/view.json` keeps the layout, the open tabs and each
//! Document's camera and Layer state. It is saved on close and a moment after a change,
//! never dirties a Document, and is thrown away without a word when it cannot be used.
//! Also: reopening the last Project on start.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reda_testkit::TempDir;
use reda_ui::canvas::Camera;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::engine::{Engine, state};
use reda_ui::layout::{Area, DockSide, Layout, PanelId, TabKey};
use reda_ui::shell::{Shell, VIEW_SAVE_DELAY};
use serde_json::{Value, json};

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

/// Creates the Project `name` in `work` and returns its `.reda` file.
fn make_project(home: &Path, work: &Path, name: &str) -> PathBuf {
    let mut shell = Shell::new(engine_in(home));
    shell.new_project(work, name).unwrap();
    work.join(name).join(format!("{name}.reda"))
}

fn first_sheet(shell: &Shell) -> Uuid {
    shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .next()
        .unwrap()
        .uuid
}

fn view_file(project: &Path) -> PathBuf {
    project.parent().unwrap().join(".reda/view.json")
}

const CAMERA: Camera = Camera {
    center: (1_000_000.0, -2_500_000.0),
    nm_per_point: 12_345.0,
};

#[test]
fn the_layout_and_cameras_come_back_when_the_project_reopens() {
    let home = TempDir::new("view-home");
    let work = TempDir::new("view-work");
    let project = make_project(home.path(), work.path(), "Back");

    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&project).unwrap();
    let sheet = first_sheet(&shell);
    shell
        .run(
            "layout.tab.move_to_dock",
            json!({"tab": "properties", "dock": "left"}),
        )
        .unwrap();
    shell.document_tab_mut(sheet).unwrap().camera = Some(CAMERA);
    shell
        .document_tab_mut(sheet)
        .unwrap()
        .layers
        .hidden
        .insert(Uuid::from_u128(7));
    let layout = shell.layout().clone();
    shell.save_view();

    let mut again = Shell::new(engine_in(home.path()));
    again.open_project(&project).unwrap();

    assert_eq!(again.layout(), &layout);
    let tab = again.document_tab(sheet).unwrap();
    assert_eq!(tab.camera, Some(CAMERA));
    assert!(tab.layers.hidden.contains(&Uuid::from_u128(7)));
}

#[test]
fn opening_another_project_saves_the_view_of_the_one_it_replaces() {
    let home = TempDir::new("view-home");
    let work = TempDir::new("view-work");
    let first = make_project(home.path(), work.path(), "First");
    let second = make_project(home.path(), work.path(), "Second");

    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&first).unwrap();
    shell
        .run("layout.tab.close", json!({"tab": "messages"}))
        .unwrap();
    shell.open_project(&second).unwrap();

    let saved: Value = serde_json::from_slice(&std::fs::read(view_file(&first)).unwrap()).unwrap();
    assert_eq!(saved["layout"]["bottom"]["groups"], json!([]));
}

#[test]
fn changes_are_saved_once_they_settle() {
    let home = TempDir::new("view-home");
    let work = TempDir::new("view-work");
    let project = make_project(home.path(), work.path(), "Settle");
    let file = view_file(&project);
    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&project).unwrap();
    let _ = std::fs::remove_file(&file);
    let t0 = Instant::now();

    // Nothing changed: nothing to write.
    shell.save_view_if_due(t0 + VIEW_SAVE_DELAY * 3);
    assert!(!file.exists());

    shell
        .run("layout.tab.close", json!({"tab": "projects"}))
        .unwrap();
    shell.save_view_if_due(t0);
    assert!(!file.exists(), "written before the change settled");
    shell.save_view_if_due(t0 + VIEW_SAVE_DELAY / 2);
    assert!(!file.exists(), "written before the change settled");
    shell.save_view_if_due(t0 + VIEW_SAVE_DELAY + Duration::from_millis(1));
    assert!(file.exists(), "not written once the change settled");
}

#[test]
fn an_unreadable_view_file_is_silently_replaced_by_the_default_layout() {
    let home = TempDir::new("view-home");
    let work = TempDir::new("view-work");
    let project = make_project(home.path(), work.path(), "Junk");
    std::fs::create_dir_all(view_file(&project).parent().unwrap()).unwrap();
    std::fs::write(view_file(&project), "{ this is not json").unwrap();

    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&project).unwrap();

    let sheet = first_sheet(&shell);
    assert_eq!(
        shell.layout(),
        &Layout::with_centre(vec![TabKey::Document(sheet)])
    );
    assert_eq!(shell.message(), None);
}

#[test]
fn a_view_file_from_another_version_is_silently_discarded() {
    let home = TempDir::new("view-home");
    let work = TempDir::new("view-work");
    let project = make_project(home.path(), work.path(), "Other");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&project).unwrap();
    shell
        .run("layout.tab.close", json!({"tab": "projects"}))
        .unwrap();
    shell.save_view();
    let file = view_file(&project);
    let mut saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    saved["reda"] = json!("999.0.0");
    std::fs::write(&file, saved.to_string()).unwrap();

    let mut again = Shell::new(engine_in(home.path()));
    again.open_project(&project).unwrap();

    assert_eq!(
        again
            .layout()
            .place(&TabKey::Panel(PanelId::Projects))
            .map(|p| p.area),
        Some(Area::Dock(DockSide::Left))
    );
    assert_eq!(again.message(), None);
}

#[test]
fn tabs_of_documents_that_no_longer_exist_are_dropped() {
    let home = TempDir::new("view-home");
    let work = TempDir::new("view-work");
    let project = make_project(home.path(), work.path(), "Gone");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&project).unwrap();
    shell.save_view();
    let file = view_file(&project);
    let text = std::fs::read_to_string(&file).unwrap();
    let sheet = first_sheet(&shell).to_string();
    let stranger = Uuid::from_u128(42).to_string();
    std::fs::write(&file, text.replace(&sheet, &stranger)).unwrap();

    let mut again = Shell::new(engine_in(home.path()));
    again.open_project(&project).unwrap();

    assert!(again.document_tabs().is_empty());
    assert_eq!(
        again
            .layout()
            .place(&TabKey::Document(stranger.parse().unwrap())),
        None
    );
}

#[test]
fn viewing_never_dirties_a_document() {
    let home = TempDir::new("view-home");
    let work = TempDir::new("view-work");
    let project = make_project(home.path(), work.path(), "Clean");
    let before = std::fs::read(&project).unwrap();
    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&project).unwrap();
    let sheet = first_sheet(&shell);

    shell
        .run("layout.tab.move_to_new_window", json!({"tab": "messages"}))
        .unwrap();
    shell
        .run("layout.dock.collapse", json!({"dock": "left"}))
        .unwrap();
    shell.document_tab_mut(sheet).unwrap().camera = Some(CAMERA);
    shell.save_view();

    assert!(!shell.engine().is_dirty());
    assert!(shell.engine().dirty_scopes().is_empty());
    assert_eq!(std::fs::read(&project).unwrap(), before);
}

#[test]
fn the_last_project_reopens_on_start() {
    let home = TempDir::new("view-home");
    let work = TempDir::new("view-work");
    let project = make_project(home.path(), work.path(), "Last");

    let mut shell = Shell::new(engine_in(home.path()));
    shell.reopen_last_project();

    assert_eq!(shell.engine().project().unwrap().props().name, "Last");
    let _ = project;
}

#[test]
fn a_missing_last_project_is_not_reopened_and_a_status_note_says_so() {
    let home = TempDir::new("view-home");
    let gone = home.path().join("Gone/Gone.reda");
    state::update(&UserDirs::under(home.path()), |s| {
        s.recent_projects = vec![gone.clone()];
    })
    .unwrap();

    let mut shell = Shell::new(engine_in(home.path()));
    shell.reopen_last_project();

    assert!(shell.engine().project().is_none());
    let note = shell.message().expect("a status note");
    assert!(note.contains("Gone"), "{note}");
    assert_eq!(shell.layout(), &Layout::default());
}

#[test]
fn the_last_project_stays_closed_when_the_preference_says_so() {
    let home = TempDir::new("view-home");
    let work = TempDir::new("view-work");
    make_project(home.path(), work.path(), "Stay");
    let mut shell = Shell::new(engine_in(home.path()));
    shell
        .run(
            "app.preferences.set",
            json!({"key": "general.reopen_last_project", "value": false}),
        )
        .unwrap();

    shell.reopen_last_project();

    assert!(shell.engine().project().is_none());
    assert_eq!(shell.message(), None);
}
