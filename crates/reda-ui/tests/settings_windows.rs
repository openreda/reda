//! The settings windows in the window, driven through AccessKit with `egui_kittest`: each
//! opens once, in a window of its own from the docking layer, at 1000 × 680 the first
//! time and where it was put after that; it keeps its edits while open and follows the
//! design where untouched; closing it with unapplied changes asks Apply, Discard or
//! Cancel, and the quit prompt lists it.

use std::path::Path;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{ErcCheck, Severity};
use reda_ui::engine::state;
use reda_ui::layout::{SettingsWindow, TabKey, WindowRect};
use reda_ui::settings::project::PropertiesPage;
use reda_ui::settings::windows::{OPENING_SIZE, opening_size};
use reda_ui::shell::Shell;
use serde_json::json;

const PROJECT_SETTINGS: TabKey = TabKey::Settings(SettingsWindow::ProjectSettings);
const BOARD_SETUP: TabKey = TabKey::Settings(SettingsWindow::BoardSetup);

struct Fixture {
    _home: TempDir,
    work: TempDir,
    h: Harness<'static, App>,
}

impl Fixture {
    /// A window on a new Project `Win` with a board.
    fn new(label: &str) -> Fixture {
        let home = TempDir::new(&format!("{label}-home"));
        let work = TempDir::new(&format!("{label}-work"));
        let engine = Engine::builder()
            .user_dirs(Some(UserDirs::under(home.path())))
            .starter_library(None)
            .headless(false)
            .build();
        let mut shell = Shell::new(engine);
        let params = json!({"path": work.path(), "name": "Win",
                            "board": {"width": "30mm", "height": "20mm"}});
        shell.run("project.new", params).unwrap();
        let mut h = Harness::builder()
            .with_size([1300.0, 900.0])
            .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
        h.run();
        Fixture {
            _home: home,
            work,
            h,
        }
    }

    fn home(&self) -> &Path {
        self._home.path()
    }

    fn shell(&self) -> &Shell {
        self.h.state().shell()
    }

    fn open_project_settings(&mut self) {
        self.h.state_mut().open_project_settings();
        self.h.run();
    }

    fn type_into(&mut self, field: &str, text: &str) {
        self.h.get_by_role_and_label(Role::TextInput, field).click();
        self.h.run();
        self.h
            .get_by_role_and_label(Role::TextInput, field)
            .type_text(text);
        self.h.run();
    }

    fn click(&mut self, label: &str) {
        self.h.get_by_role_and_label(Role::Button, label).click();
        self.h.run();
    }

    fn title_draft(&self) -> String {
        let windows = self.h.state().settings_windows();
        let dialog = windows.dialog(SettingsWindow::ProjectSettings).unwrap();
        let page = dialog.page::<PropertiesPage>().unwrap();
        page.title.value().clone()
    }

    fn project_title(&self) -> String {
        let project = self.shell().engine().project().unwrap();
        project.props().properties.title.clone()
    }

    fn is_open(&self, tab: &TabKey) -> bool {
        self.shell().layout().place(tab).is_some()
    }

    fn disabled(&self, label: &str) -> bool {
        self.h
            .get_by_role_and_label(Role::Button, label)
            .accesskit_node()
            .is_disabled()
    }

    /// Clicks the closing prompt's button `label` (the settings window has an Apply
    /// too).
    fn click_in_prompt(&mut self, label: &str) {
        let prompt = egui::Id::NULL.with("closing-prompt");
        let rect = self
            .h
            .ctx
            .memory(|m| m.area_rect(prompt))
            .expect("the prompt");
        self.h
            .query_all_by_role_and_label(Role::Button, label)
            .find(|n| rect.contains(n.rect().center()))
            .expect("the prompt's button")
            .click();
        self.h.run();
    }

    fn prompt_open(&self) -> bool {
        self.h.query_by_label("Unsaved Changes").is_some()
    }
}

#[test]
fn a_settings_window_opens_once_in_a_window_of_its_own_and_keeps_its_edits() {
    let mut f = Fixture::new("sw-once");
    f.open_project_settings();
    let layout = f.shell().layout();
    let window = layout
        .window_of(&PROJECT_SETTINGS)
        .expect("in a window of its own");
    assert_eq!(layout.tabs_in_window(window), vec![&PROJECT_SETTINGS]);
    assert_eq!(opening_size(layout, window), OPENING_SIZE);
    assert_eq!(OPENING_SIZE, egui::vec2(1000.0, 680.0));
    // Not a Panel: the Panels button does not list it.
    assert!(
        layout
            .closed_panels()
            .iter()
            .all(|p| p.id() != "project_settings")
    );

    f.type_into("Title", "Motor driver");
    f.open_project_settings();
    let layout = f.shell().layout();
    assert_eq!(layout.windows.len(), 1, "opening again adds no window");
    assert_eq!(layout.window_of(&PROJECT_SETTINGS), Some(window));
    assert_eq!(f.title_draft(), "Motor driver", "its edits are kept");
}

#[test]
fn project_settings_and_board_setup_are_open_at_once_beside_the_canvas() {
    let mut f = Fixture::new("sw-both");
    f.open_project_settings();
    f.h.state_mut().open_board_setup();
    f.h.run();
    assert!(f.is_open(&PROJECT_SETTINGS) && f.is_open(&BOARD_SETUP));
    let layout = f.shell().layout();
    assert_ne!(
        layout.window_of(&PROJECT_SETTINGS),
        layout.window_of(&BOARD_SETUP)
    );
    // The rest of the app stays usable: an edit elsewhere goes through.
    let sheet = f.shell().layout().active_document();
    assert!(sheet.is_some(), "the sheet stays open in the main window");
}

#[test]
fn ok_cancel_and_apply_with_one_undo_entry_per_apply() {
    let mut f = Fixture::new("sw-buttons");
    f.open_project_settings();
    assert!(f.disabled("Apply"), "nothing to apply yet");
    f.type_into("Title", "Amp");
    let journal = f.shell().engine().journal().len();
    f.click("Apply");
    assert_eq!(f.project_title(), "Amp");
    assert_eq!(f.shell().engine().journal().len(), journal + 1);
    assert!(f.is_open(&PROJECT_SETTINGS), "Apply keeps it open");
    f.type_into("Revision", "B");
    f.click("Cancel");
    assert!(
        !f.is_open(&PROJECT_SETTINGS),
        "Cancel closes without asking"
    );
    let project = f.shell().engine().project().unwrap();
    assert_eq!(project.props().properties.revision, "");
}

#[test]
fn untouched_fields_follow_the_design_and_touched_ones_keep_their_value() {
    let mut f = Fixture::new("sw-follow");
    f.open_project_settings();
    f.type_into("Title", "Mine");
    f.h.state_mut()
        .shell_mut()
        .run(
            "project.properties.set",
            json!({"title": "Theirs", "author": "Ada"}),
        )
        .unwrap();
    f.h.run();
    let windows = f.h.state().settings_windows();
    let page = (windows.dialog(SettingsWindow::ProjectSettings))
        .and_then(|d| d.page::<PropertiesPage>())
        .unwrap();
    assert_eq!(page.author.value(), "Ada", "untouched: follows the design");
    assert_eq!(
        page.title.value(),
        "Mine",
        "touched: keeps the user's value"
    );
}

#[test]
fn closing_one_with_unapplied_changes_asks_apply_discard_or_cancel() {
    let mut f = Fixture::new("sw-close");
    f.open_project_settings();
    f.type_into("Title", "Amp");
    f.click("Close Project Settings");
    assert!(f.prompt_open());
    f.h.get_by_label("Apply the changes in Project Settings before closing it?");
    // Escape is the prompt's Cancel.
    f.h.key_press(egui::Key::Escape);
    f.h.run();
    assert!(!f.prompt_open() && f.is_open(&PROJECT_SETTINGS));
    assert_eq!(f.title_draft(), "Amp");

    f.click("Close Project Settings");
    f.click_in_prompt("Discard");
    assert!(!f.is_open(&PROJECT_SETTINGS));
    assert_eq!(f.project_title(), "");

    f.open_project_settings();
    assert_eq!(f.title_draft(), "", "a closed window's edits are gone");
    f.type_into("Title", "Amp");
    f.click("Close Project Settings");
    f.click_in_prompt("Apply");
    assert!(!f.is_open(&PROJECT_SETTINGS));
    assert_eq!(f.project_title(), "Amp");
}

#[test]
fn closing_one_with_nothing_changed_does_not_ask() {
    let mut f = Fixture::new("sw-clean");
    f.open_project_settings();
    f.click("Close Project Settings");
    assert!(!f.prompt_open() && !f.is_open(&PROJECT_SETTINGS));
}

#[test]
fn the_quit_prompt_lists_unapplied_windows_and_applies_a_ticked_one_before_saving() {
    let mut f = Fixture::new("sw-quit");
    f.open_project_settings();
    f.type_into("Title", "Amp");
    let ctx = f.h.ctx.clone();
    f.h.state_mut().quit(&ctx);
    f.h.run();
    assert!(f.prompt_open());
    f.h.get_by_role_and_label(Role::CheckBox, "Project Settings (changes not applied)");
    f.click("Save Selected");
    assert!(f.h.state().closing().quit_approved());
    assert_eq!(f.project_title(), "Amp");
    assert!(
        f.shell().engine().dirty_scopes().is_empty(),
        "what the Apply changed is saved"
    );
    let file = std::fs::read_to_string(f.work.path().join("Win/Win.reda")).unwrap();
    assert!(file.contains("Amp"));
}

#[test]
fn reset_to_defaults_shows_the_page_defaults_as_pending_edits_until_apply() {
    let mut f = Fixture::new("sw-reset");
    f.open_project_settings();
    f.h.get_by_role_and_label(Role::TreeItem, "ERC").click();
    f.h.run();
    f.h.get_by_label("Single-Pin Net: Off").click();
    f.h.run();
    f.click("Apply");
    let erc = |f: &Fixture| f.shell().engine().project().unwrap().props().erc.clone();
    assert_eq!(
        erc(&f).severities.get(ErcCheck::SinglePinNet),
        Some(Severity::Off)
    );
    f.click("Reset to Defaults");
    assert_eq!(
        erc(&f).severities.get(ErcCheck::SinglePinNet),
        Some(Severity::Off),
        "pending"
    );
    assert!(!f.disabled("Apply"));
    f.click("Apply");
    assert_eq!(
        erc(&f).severities.get(ErcCheck::SinglePinNet),
        Some(Severity::Warning)
    );
}

#[test]
fn each_window_keeps_its_place_in_state_json() {
    let mut f = Fixture::new("sw-place");
    f.open_project_settings();
    let window = f.shell().layout().window_of(&PROJECT_SETTINGS).unwrap();
    let rect = WindowRect {
        x: 40,
        y: 50,
        width: 900,
        height: 600,
    };
    f.h.state_mut()
        .shell_mut()
        .run(
            "layout.window.place",
            json!({"window": window, "x": 40, "y": 50, "width": 900, "height": 600}),
        )
        .unwrap();
    f.h.run();
    f.click("Close Project Settings");
    let saved = state::read(&UserDirs::under(f.home()));
    let place = saved.settings_windows["project_settings"];
    assert_eq!(
        (place.x, place.y, place.width, place.height),
        (40, 50, 900, 600)
    );

    f.open_project_settings();
    let layout = f.shell().layout();
    let window = layout.window_of(&PROJECT_SETTINGS).unwrap();
    let floating = layout.windows.iter().find(|w| w.id == window).unwrap();
    assert_eq!(floating.rect, Some(rect), "it opens where it was");
}

#[test]
fn saved_layouts_and_view_json_do_not_keep_settings_windows() {
    let mut f = Fixture::new("sw-layouts");
    f.open_project_settings();
    let saved = reda_ui::layout::SavedLayout::of(f.shell().layout());
    let applied = saved.apply_to(f.shell().layout()).unwrap();
    // It stays open through a Saved Layout, but the Saved Layout does not hold it.
    assert!(applied.place(&PROJECT_SETTINGS).is_some());
    assert!(saved.windows.is_empty());
    f.h.state_mut().shell_mut().save_view();
    let view = std::fs::read_to_string(f.work.path().join("Win/.reda/view.json")).unwrap();
    assert!(!view.contains("project_settings"));
}
