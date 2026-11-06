//! The closing and quitting prompts in the window, driven through AccessKit with
//! `egui_kittest`: closing a dirty Document's tab, quitting, and opening another Project,
//! each with Save, Don't Save, Cancel and a failed Save, and what each leaves of the
//! Recovery Snapshots.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::accesskit::{Role, Toggled};
use egui::{ViewportEvent, ViewportId};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::closing::Item;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::format::library::write_library_info;
use reda_ui::engine::model::{LibraryInfo, Scope, Uuid};
use reda_ui::engine::recovery as snapshots;
use reda_ui::layout::TabKey;
use reda_ui::shell::Shell;
use serde_json::json;

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

struct Fixture {
    _home: TempDir,
    work: TempDir,
    h: Harness<'static, App>,
    t0: Instant,
}

impl Fixture {
    /// A window on a new Project `Demo`, its first sheet in a tab.
    fn new(label: &str) -> Fixture {
        let home = TempDir::new(&format!("{label}-home"));
        let work = TempDir::new(&format!("{label}-work"));
        let mut shell = Shell::new(engine_in(home.path()));
        shell.new_project(work.path(), "Demo").unwrap();
        let mut h = Harness::builder()
            .with_size([1100.0, 700.0])
            .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
        h.run();
        Fixture {
            _home: home,
            work,
            h,
            t0: Instant::now(),
        }
    }

    fn folder(&self) -> PathBuf {
        self.work.path().join("Demo")
    }

    fn shell(&self) -> &Shell {
        self.h.state().shell()
    }

    fn shell_mut(&mut self) -> &mut Shell {
        self.h.state_mut().shell_mut()
    }

    fn sheet(&self) -> Uuid {
        let project = self.shell().engine().project().unwrap();
        project.documents().next().unwrap().uuid
    }

    fn sheet_file(&self) -> PathBuf {
        self.folder().join("Demo.redasch")
    }

    fn wire(&mut self) {
        let sheet = self.sheet();
        self.shell_mut()
            .run(
                "schematic.wire.add",
                json!({"document": sheet, "points": [{"x": "0mm", "y": "0mm"}, {"x": "5mm", "y": "0mm"}]}),
            )
            .unwrap();
        self.h.run();
    }

    fn retitle(&mut self) {
        self.shell_mut()
            .run("project.properties.set", json!({"title": "Amp"}))
            .unwrap();
        self.h.run();
    }

    /// Writes the snapshots of everything unsaved, as the schedule would.
    fn snapshot(&mut self) {
        let t0 = self.t0;
        let shell = self.shell_mut();
        shell.tick_recovery(t0);
        shell.tick_recovery(t0 + Duration::from_secs(30));
        shell.flush_recovery();
    }

    fn snapshot_dir(&self) -> PathBuf {
        snapshots::project_dir(&self.folder())
    }

    fn snapshot_files(&self) -> usize {
        snapshots::scan(&self.snapshot_dir(), Some(&self.folder())).len()
    }

    fn dirty(&self) -> Vec<Scope> {
        self.shell().engine().dirty_scopes()
    }

    fn has_tab(&self, uuid: Uuid) -> bool {
        self.shell()
            .layout()
            .place(&TabKey::Document(uuid))
            .is_some()
    }

    fn close_sheet_tab(&mut self) {
        self.h
            .get_by_role_and_label(Role::Button, "Close Sheet 1")
            .click();
        self.h.run();
    }

    fn prompt_open(&self) -> bool {
        self.h.query_by_label("Unsaved Changes").is_some()
    }

    fn click(&mut self, label: &str) {
        self.h.get_by_role_and_label(Role::Button, label).click();
        self.h.run();
    }

    fn quit_from_menu(&mut self) {
        self.h.get_by_label("File").click();
        self.h.run();
        self.h.get_by_label("Quit").click();
        self.h.run();
    }

    /// The OS asks the window to close (its title bar's close button).
    fn close_window(&mut self) {
        let input = self.h.input_mut();
        let root = input.viewports.entry(ViewportId::ROOT).or_default();
        root.events.push(ViewportEvent::Close);
        self.h.run();
    }

    fn exit(&mut self) {
        <App as eframe::App>::on_exit(self.h.state_mut());
    }

    /// Makes saving the sheet fail: a folder now stands where its file goes.
    fn break_sheet_file(&self) {
        let file = self.sheet_file();
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir_all(file.join("blocker")).unwrap();
    }

    fn sheet_on_disk(&self) -> String {
        std::fs::read_to_string(self.sheet_file()).unwrap()
    }
}

#[test]
fn a_clean_tab_closes_without_asking() {
    let mut f = Fixture::new("close-clean");
    let sheet = f.sheet();
    f.close_sheet_tab();
    assert!(!f.prompt_open());
    assert!(!f.has_tab(sheet));
}

#[test]
fn closing_a_dirty_tab_asks_and_cancel_keeps_it() {
    let mut f = Fixture::new("close-cancel");
    let sheet = f.sheet();
    f.wire();
    f.close_sheet_tab();
    assert!(f.prompt_open());
    f.h.get_by_label_contains("Save the changes to Sheet 1");
    f.h.get_by_role_and_label(Role::Button, "Save");
    f.h.get_by_role_and_label(Role::Button, "Don't Save");
    f.click("Cancel");
    assert!(!f.prompt_open());
    assert!(f.has_tab(sheet));
    assert_eq!(f.dirty(), [Scope::Document(sheet)]);
}

#[test]
fn save_on_a_tab_writes_the_document_then_closes_it() {
    let mut f = Fixture::new("close-save");
    let sheet = f.sheet();
    let before = f.sheet_on_disk();
    f.wire();
    f.snapshot();
    assert_eq!(f.snapshot_files(), 1);
    f.close_sheet_tab();
    f.click("Save");
    assert!(!f.prompt_open());
    assert!(!f.has_tab(sheet));
    assert!(f.dirty().is_empty());
    assert_ne!(f.sheet_on_disk(), before);
    f.snapshot();
    assert_eq!(f.snapshot_files(), 0, "saving deletes the snapshot");
}

#[test]
fn dont_save_on_a_tab_reverts_the_document_and_deletes_its_snapshot() {
    let mut f = Fixture::new("close-dont");
    let sheet = f.sheet();
    let before = f.sheet_on_disk();
    let objects = f.shell().engine().project().unwrap().objects().len();
    f.wire();
    f.snapshot();
    assert_eq!(f.snapshot_files(), 1);
    f.close_sheet_tab();
    f.click("Don't Save");
    assert!(!f.prompt_open());
    assert!(!f.has_tab(sheet));
    assert!(f.dirty().is_empty(), "{:?}", f.dirty());
    let project = f.shell().engine().project().unwrap();
    assert_eq!(project.objects().len(), objects, "the wire is gone");
    assert_eq!(f.sheet_on_disk(), before);
    f.snapshot();
    assert_eq!(f.snapshot_files(), 0);
}

#[test]
fn a_failed_save_on_a_tab_keeps_the_tab_the_prompt_and_the_snapshot() {
    let mut f = Fixture::new("close-fail");
    let sheet = f.sheet();
    f.wire();
    f.snapshot();
    f.break_sheet_file();
    f.close_sheet_tab();
    f.click("Save");
    assert!(f.prompt_open(), "the prompt stays open");
    f.h.get_by_label_contains("could not be saved");
    assert!(f.has_tab(sheet));
    assert_eq!(f.dirty(), [Scope::Document(sheet)]);
    f.snapshot();
    assert_eq!(f.snapshot_files(), 1);
    f.click("Cancel");
    assert!(f.has_tab(sheet));
}

#[test]
fn quitting_with_nothing_unsaved_closes_at_once() {
    let mut f = Fixture::new("quit-clean");
    f.quit_from_menu();
    assert!(!f.prompt_open());
    assert!(f.h.state().closing().quit_approved());
}

#[test]
fn quitting_lists_every_unsaved_file_with_a_tick_each() {
    let mut f = Fixture::new("quit-list");
    f.wire();
    f.retitle();
    f.quit_from_menu();
    assert!(f.prompt_open());
    assert!(!f.h.state().closing().quit_approved());
    for row in ["Demo.reda", "Sheet 1"] {
        let node = f.h.get_by_role_and_label(Role::CheckBox, row);
        assert_eq!(
            node.accesskit_node().toggled(),
            Some(Toggled::True),
            "{row}"
        );
    }
    f.h.get_by_role_and_label(Role::Button, "Save Selected");
    f.h.get_by_role_and_label(Role::Button, "Don't Save");
    f.click("Cancel");
    assert!(!f.prompt_open());
    assert!(!f.h.state().closing().quit_approved());
    assert_eq!(f.dirty().len(), 2);
}

#[test]
fn the_window_close_button_asks_too_and_an_unanswered_prompt_keeps_the_snapshots() {
    let mut f = Fixture::new("quit-os");
    f.wire();
    f.snapshot();
    f.close_window();
    assert!(f.prompt_open());
    assert!(!f.h.state().closing().quit_approved());
    // The window is made to exit anyway (the OS logs out): nothing got past the prompt.
    f.exit();
    assert_eq!(f.snapshot_files(), 1);
}

#[test]
fn save_selected_saves_the_ticked_files_then_quits_and_the_exit_clears_the_snapshots() {
    let mut f = Fixture::new("quit-save");
    let before = f.sheet_on_disk();
    let project_file = f.folder().join("Demo.reda");
    let project_before = std::fs::read_to_string(&project_file).unwrap();
    f.wire();
    f.retitle();
    f.snapshot();
    assert_eq!(f.snapshot_files(), 2);
    f.quit_from_menu();
    // Untick the Project file: it is left unsaved.
    f.h.get_by_role_and_label(Role::CheckBox, "Demo.reda")
        .click();
    f.h.run();
    f.click("Save Selected");
    assert!(!f.prompt_open());
    assert!(f.h.state().closing().quit_approved());
    assert_ne!(f.sheet_on_disk(), before);
    assert_eq!(
        std::fs::read_to_string(&project_file).unwrap(),
        project_before
    );
    f.exit();
    assert_eq!(f.snapshot_files(), 0);
    assert!(!f.snapshot_dir().join(snapshots::MANIFEST).exists());
}

#[test]
fn dont_save_quits_without_saving() {
    let mut f = Fixture::new("quit-dont");
    let before = f.sheet_on_disk();
    f.wire();
    f.snapshot();
    f.close_window();
    f.click("Don't Save");
    assert!(f.h.state().closing().quit_approved());
    assert_eq!(f.sheet_on_disk(), before);
    // The close that follows is let through.
    f.close_window();
    assert!(!f.prompt_open());
    f.exit();
    assert_eq!(f.snapshot_files(), 0);
}

#[test]
fn a_failed_save_selected_keeps_the_window_open_and_the_snapshots() {
    let mut f = Fixture::new("quit-fail");
    f.wire();
    f.retitle();
    f.snapshot();
    f.break_sheet_file();
    f.quit_from_menu();
    f.click("Save Selected");
    assert!(f.prompt_open());
    assert!(!f.h.state().closing().quit_approved());
    f.h.get_by_label_contains("could not be saved");
    // The Project file saved and left the list; the sheet stays.
    assert!(f.h.query_by_label("Demo.reda").is_none());
    f.h.get_by_role_and_label(Role::CheckBox, "Sheet 1");
    f.exit();
    assert_eq!(f.snapshot_files(), 1, "the sheet's snapshot stays");
}

#[test]
fn opening_another_project_asks_first() {
    let mut f = Fixture::new("open-other");
    let other = TempDir::new("open-other-b");
    let mut maker = Shell::new(engine_in(other.path()));
    maker.new_project(other.path(), "Other").unwrap();
    let other_file = other.path().join("Other/Other.reda");
    drop(maker);

    f.wire();
    f.h.state_mut().open_project(&other_file);
    f.h.run();
    assert!(f.prompt_open());
    f.h.get_by_role_and_label(Role::CheckBox, "Sheet 1");
    f.click("Cancel");
    let name = |f: &Fixture| f.shell().engine().project().unwrap().props().name.clone();
    assert_eq!(name(&f), "Demo");
    assert!(!f.dirty().is_empty());

    f.h.state_mut().open_project(&other_file);
    f.h.run();
    f.click("Don't Save");
    assert_eq!(name(&f), "Other");
}

#[test]
fn opening_another_project_with_nothing_unsaved_does_not_ask() {
    let mut f = Fixture::new("open-clean");
    let other = TempDir::new("open-clean-b");
    let mut maker = Shell::new(engine_in(other.path()));
    maker.new_project(other.path(), "Other").unwrap();
    drop(maker);
    f.h.state_mut()
        .open_project(&other.path().join("Other/Other.reda"));
    f.h.run();
    assert!(!f.prompt_open());
    let project = f.shell().engine().project().unwrap();
    assert_eq!(project.props().name, "Other");
}

/// Registers an empty user Library `Mine` under `home`, for Library editor tabs.
fn user_library(home: &Path, at: &Path) {
    std::fs::create_dir_all(at).unwrap();
    let info = LibraryInfo {
        uuid: LIBRARY.parse().unwrap(),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(at.join("Mine.redalib"), write_library_info(&info).unwrap()).unwrap();
    let dirs = UserDirs::under(home);
    std::fs::create_dir_all(&dirs.config).unwrap();
    std::fs::write(
        dirs.config.join("preferences.toml"),
        format!("[libraries]\npaths = [{:?}]\n", at.display().to_string()),
    )
    .unwrap();
}

#[test]
fn library_editor_tabs_ask_on_close_and_join_the_quit_list() {
    let home = TempDir::new("close-element-home");
    user_library(home.path(), &home.path().join("mine"));
    let mut shell = Shell::new(engine_in(home.path()));
    let out = shell
        .run(
            "library.element.new",
            json!({"library": LIBRARY, "kind": "symbol", "name": "Opamp"}),
        )
        .unwrap();
    let symbol: Uuid = out["element"].as_str().unwrap().parse().unwrap();
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();

    // The quit list has the unsaved element.
    h.get_by_label("File").click();
    h.run();
    h.get_by_label("Quit").click();
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Opamp");
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run();

    // Closing its tab asks; Don't Save closes the element, discarding it.
    h.get_by_role_and_label(Role::Button, "Close Opamp").click();
    h.run();
    h.get_by_label_contains("Save the changes to Opamp");
    let closing = h.state().closing().ask().cloned();
    assert!(matches!(
        closing,
        Some(reda_ui::closing::Ask::Tab { item: Item::Element(e), .. }) if e == symbol
    ));
    h.get_by_role_and_label(Role::Button, "Don't Save").click();
    h.run();
    let engine = h.state().shell().engine();
    assert!(engine.element(symbol).is_none());
    assert!(engine.dirty_scopes().is_empty());
}
