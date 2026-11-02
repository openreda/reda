//! Sync in the window, driven through AccessKit with `egui_kittest`: the "out of sync"
//! badge in the status bar and on the PCB tab, and the Change List dialog it opens
//! (also from F8), whose OK runs one `sync.apply` without the unticked changes.

use std::path::{Path, PathBuf};

use egui::Key;
use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::shell::Shell;
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

/// The window on a Project with R1 and R2 whose pin 2s are wired and labelled SIG, not
/// yet synced: five pending changes.
fn harness(work: &Path) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, "Demo").unwrap();
    for x in ["0mm", "10mm"] {
        shell
            .run(
                "schematic.component.place",
                json!({"document": SHEET, "part": R_10K, "at": {"x": x, "y": 0}}),
            )
            .unwrap();
    }
    shell
        .run(
            "schematic.wire.add",
            json!({"document": SHEET, "points": [
                {"x": "0mm", "y": "-3.81mm"}, {"x": "10mm", "y": "-3.81mm"}]}),
        )
        .unwrap();
    shell
        .run(
            "schematic.net_label.add",
            json!({"document": SHEET, "name": "SIG", "at": {"x": "5mm", "y": "-3.81mm"}}),
        )
        .unwrap();
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn ticked(h: &Harness<'static, App>, label: &str) -> bool {
    h.get_by_role_and_label(Role::CheckBox, label)
        .accesskit_node()
        .toggled()
        == Some(Toggled::True)
}

#[test]
fn the_status_bar_and_the_pcb_tab_show_the_out_of_sync_count() {
    let work = TempDir::new("sync-win-badge");
    let mut h = harness(work.path());
    h.state_mut()
        .shell_mut()
        .run("document.open", json!({ "document": PCB }))
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::Button, "out of sync: 5 changes");
    h.get_by_label("Demo, out of sync: 5 changes");
}

#[test]
fn the_badge_opens_the_change_list_and_ok_applies_the_ticked_changes() {
    let work = TempDir::new("sync-win-apply");
    let mut h = harness(work.path());
    h.get_by_role_and_label(Role::Button, "out of sync: 5 changes")
        .click();
    h.run();
    h.get_by_label("Update PCB from Schematic");
    h.get_by_label("Nets (1)");
    assert!(ticked(&h, "Add R1 (R_0603)"));
    assert!(ticked(&h, "R1.2: (no Net) to SIG"));

    // Unticking the Net unticks the Pad Nets that need it.
    h.get_by_role_and_label(Role::CheckBox, "Add Net SIG")
        .click();
    h.run();
    assert!(!ticked(&h, "R1.2: (no Net) to SIG"));
    assert!(!ticked(&h, "R2.2: (no Net) to SIG"));
    h.get_by_label("2 of 5 changes ticked. Unticked changes show up again next time.");

    h.get_by_role_and_label(Role::Button, "OK").click();
    h.run();
    assert!(h.query_by_label("Update PCB from Schematic").is_none());
    let journal = h.state().shell().engine().journal();
    assert_eq!(journal.last().unwrap().id, "batch");
    h.get_by_role_and_label(Role::Button, "out of sync: 3 changes");
}

#[test]
fn f8_opens_the_change_list() {
    let work = TempDir::new("sync-win-f8");
    let mut h = harness(work.path());
    h.key_press(Key::F8);
    h.run();
    h.get_by_label("Update PCB from Schematic");
    h.key_press(Key::Escape);
    h.run();
    assert!(h.query_by_label("Update PCB from Schematic").is_none());
}
