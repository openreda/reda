//! The Guides switch of Snap Options and the snap palette, driven through AccessKit with
//! `egui_kittest` (docs/spec/v0.3/grids-guides.md#snapping): on a board, Snap to Guides
//! gives one `view.snap.set {guides}`; a sheet, which has no Guides, offers no such switch.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";

fn harness(home: &TempDir, work: &TempDir) -> Harness<'static, App> {
    let library = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reda-schematic/tests/library/Test");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    Harness::builder()
        .with_size([1400.0, 2600.0])
        .with_max_steps(8)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    h.state_mut().shell_mut().run(id, params).unwrap()
}

fn last_entry(h: &Harness<'_, App>) -> (String, Value) {
    let entry = (h.state().shell().engine().journal().iter())
        .rfind(|e| e.outcome.is_ok())
        .unwrap();
    (entry.id.clone(), entry.params.clone())
}

fn open(h: &mut Harness<'_, App>, document: &str) {
    run(h, "document.open", json!({"document": document}));
    h.run();
}

/// The Active Bar's Snap Options button, not the Properties region of that name.
fn open_palette(h: &mut Harness<'_, App>) {
    h.get_by_role_and_label(Role::Button, "Snap Options")
        .click();
    h.run();
}

fn guides_switches(h: &Harness<'_, App>) -> usize {
    h.query_all_by_role_and_label(Role::CheckBox, "Snap to Guides")
        .count()
}

fn guides_on(h: &mut Harness<'_, App>) -> Value {
    run(h, "view.snap.set", json!({"document": PCB}))["guides"].clone()
}

#[test]
fn snap_options_and_the_snap_palette_switch_guides_on_a_board() {
    let (home, work) = (
        TempDir::new("snap-guides-home"),
        TempDir::new("snap-guides-work"),
    );
    let mut h = harness(&home, &work);
    open(&mut h, PCB);
    // Properties with nothing selected: the Snap Options region.
    assert_eq!(guides_switches(&h), 1);
    h.get_by_role_and_label(Role::CheckBox, "Snap to Guides")
        .click();
    h.run();
    assert_eq!(
        last_entry(&h),
        (
            "view.snap.set".into(),
            json!({"document": PCB, "guides": false})
        )
    );
    assert_eq!(guides_on(&mut h), json!(false));

    // The snap palette (`Mod`+E, or the Active Bar button) holds the same switch.
    open_palette(&mut h);
    assert_eq!(guides_switches(&h), 2);
    h.query_all_by_role_and_label(Role::CheckBox, "Snap to Guides")
        .last()
        .unwrap()
        .click();
    h.run();
    assert_eq!(
        last_entry(&h),
        (
            "view.snap.set".into(),
            json!({"document": PCB, "guides": true})
        )
    );
    assert_eq!(guides_on(&mut h), json!(true));
}

#[test]
fn a_sheet_offers_no_guides_switch() {
    let (home, work) = (
        TempDir::new("snap-guides-sheet-home"),
        TempDir::new("snap-guides-sheet-work"),
    );
    let mut h = harness(&home, &work);
    open(&mut h, SHEET);
    open_palette(&mut h);
    assert!(h.query_all_by_label("Snap to grid").count() >= 1);
    assert_eq!(guides_switches(&h), 0);
}
