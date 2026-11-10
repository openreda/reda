//! The Output Job dialog and job runs in the window, driven through AccessKit with
//! `egui_kittest`: Edit… and a double-click open the dialog, Run from the Projects panel
//! or the dialog shows progress and then the run report, and a broken job is flagged.

use std::path::Path;
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::DocumentKind;
use reda_ui::settings::output_job;
use reda_ui::shell::Shell;
use serde_json::json;

/// The job `project.new` makes, named after the Generic Fab Preset.
const JOB: &str = "Generic";

fn harness(home: &Path, work: &Path, board: bool) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    let mut params = json!({"path": work, "name": "Jobs"});
    if board {
        params["board"] = json!({"width": "30mm", "height": "20mm"});
    }
    shell.run("project.new", params).unwrap();
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 10.0);
    h.run();
    h
}

fn job_menu(h: &mut Harness<'static, App>, item: &str) {
    h.get_by_role_and_label(Role::TreeItem, JOB)
        .click_secondary();
    h.run();
    h.get_by_label(item).click();
    // A run keeps the window repainting until it ends, so step rather than settle.
    h.step();
}

fn disabled(h: &Harness<'static, App>, label: &str) -> bool {
    h.get_by_role_and_label(Role::Button, label)
        .accesskit_node()
        .is_disabled()
}

/// Steps frames until `label` shows; the run is on another thread.
fn wait_for(h: &mut Harness<'static, App>, label: &str) {
    let start = Instant::now();
    while h.query_by_label_contains(label).is_none() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "{label:?} never showed"
        );
        std::thread::sleep(Duration::from_millis(10));
        h.step();
    }
}

fn last_journal_id(h: &Harness<'static, App>) -> String {
    let journal = h.state().shell().engine().journal();
    journal.last().map(|e| e.id.clone()).unwrap_or_default()
}

#[test]
fn edit_and_a_double_click_open_the_output_job_dialog() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    let row = h.get_by_role_and_label(Role::TreeItem, JOB);
    row.click();
    row.click();
    h.run();
    h.get_by_label("Output Job: Generic");
    h.get_by_label("Cancel").click();
    h.run();

    job_menu(&mut h, "Edit…");
    h.run();
    h.get_by_label("Output Job: Generic");
    for field in ["Name", "Destination folder"] {
        h.get_by_role_and_label(Role::TextInput, field);
    }
    page(&mut h, "Outputs");
    h.get_by_label("CSV BOM");
    assert!(!disabled(&h, "OK") && disabled(&h, "Apply"));
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.query_by_label("Output Job: Generic").is_none());
}

#[test]
fn ok_applies_the_dialog_as_one_batch() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Edit…");
    h.run();
    page(&mut h, "Outputs");
    h.get_by_label("CSV BOM").click();
    h.run();
    assert!(!disabled(&h, "Apply"));
    h.get_by_label("OK").click();
    h.run();
    assert!(h.query_by_label("Output Job: Generic").is_none());
    assert_eq!(last_journal_id(&h), "batch");
    let project = h.state().shell().engine().project().unwrap();
    assert!(!project.output_jobs().next().unwrap().outputs.bom);
}

#[test]
fn the_fab_notes_drill_report_and_job_file_each_have_a_toggle() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Edit…");
    h.run();
    page(&mut h, "Outputs");
    for label in ["Fab Notes", "Drill report", "Gerber job file"] {
        h.get_by_label(label).click();
        h.run();
    }
    h.get_by_label("OK").click();
    h.run();
    let project = h.state().shell().engine().project().unwrap();
    let outputs = project.output_jobs().next().unwrap().outputs;
    assert!(!outputs.fab_notes && !outputs.drill_report && !outputs.gerber_job);
    assert!(outputs.gerber && outputs.drill && outputs.bom && outputs.cpl);
}

#[test]
fn run_from_the_projects_panel_shows_the_report_and_offers_the_zip() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Run");
    wait_for(&mut h, "Generic finished");
    h.get_by_label_contains("fab/Jobs.gtl");
    h.get_by_label("Show in Folder");
    assert_eq!(last_journal_id(&h), "output.job.run");
    h.get_by_label("Close").click();
    h.run();
    assert!(h.query_by_label_contains("Generic finished").is_none());
}

#[test]
fn run_from_the_dialog_applies_first_and_lists_blockers() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    // No Board Outline: the run is blocked.
    let mut h = harness(home.path(), work.path(), false);
    job_menu(&mut h, "Edit…");
    h.run();
    page(&mut h, "Outputs");
    h.get_by_label("CSV CPL (pick and place)").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Run").click();
    h.step();
    wait_for(&mut h, "Generic did not run");
    h.get_by_label("Blocking problems");
    assert!(
        h.get_all_by_label_contains("Board Outline is missing")
            .count()
            >= 1
    );
    let journal: Vec<String> = h
        .state()
        .shell()
        .engine()
        .journal()
        .iter()
        .map(|e| e.id.clone())
        .collect();
    assert_eq!(journal[journal.len() - 2..], ["batch", "output.job.run"]);
    let project = h.state().shell().engine().project().unwrap();
    assert!(!project.output_jobs().next().unwrap().outputs.cpl);
    let messages = h.state().shell().message_groups();
    assert!(
        messages
            .iter()
            .flat_map(|g| &g.lines)
            .any(|l| l.text.contains("Board Outline")),
        "{messages:?}"
    );
}

#[test]
fn a_job_whose_pcb_is_gone_is_flagged_broken_and_does_not_run() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    let pcb = {
        let project = h.state().shell().engine().project().unwrap();
        let pcb = project.documents().find(|d| d.kind == DocumentKind::Pcb);
        pcb.unwrap().uuid
    };
    h.state_mut()
        .shell_mut()
        .run("document.delete", json!({"document": pcb}))
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::TreeItem, "Generic (broken)")
        .click_secondary();
    h.run();
    assert!(disabled(&h, "Run"));
    h.get_by_label("Edit…").click();
    h.run();
    h.get_by_label("This job's PCB Document is gone. Pick another to repair it.");
    assert!(disabled(&h, "OK"));
}

/// The starter Library's Resistor and its 0603 Footprint.
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const RESC1608: &str = "0691e5e7-9eb6-49f3-9f3e-d4159258693e";

/// The app on a Project with a board and two 10k resistors on it.
fn harness_with_parts(home: &Path, work: &Path) -> Harness<'static, App> {
    let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(Some(library))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .run(
            "project.new",
            json!({"path": work, "name": "Jobs", "board": {"width": "30mm", "height": "20mm"}}),
        )
        .unwrap();
    let sheet = shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .find(|d| d.kind == DocumentKind::Schematic)
        .unwrap()
        .uuid;
    for x in ["0mm", "20mm"] {
        shell
            .run(
                "schematic.component.place",
                json!({"document": sheet, "part": RESISTOR, "footprint": RESC1608,
                       "at": {"x": x, "y": "40mm"}}),
            )
            .unwrap();
    }
    let hash = shell.run("sync.preview", json!({})).unwrap()["hash"].clone();
    shell.run("sync.apply", json!({"hash": hash})).unwrap();
    let mut h = Harness::builder()
        .with_size([1100.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

/// Replaces the text of the field announced as `label` with `text`.
fn retype(h: &mut Harness<'static, App>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run();
}

fn page(h: &mut Harness<'static, App>, title: &str) {
    h.get_by_role_and_label(Role::TreeItem, title).click();
    h.run();
}

#[test]
fn the_window_has_its_seven_pages_and_run() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Edit…");
    h.run();
    for title in [
        "General", "Outputs", "Gerber", "Drill", "BOM", "CPL", "Last run",
    ] {
        h.get_by_role_and_label(Role::TreeItem, title);
    }
    h.get_by_role_and_label(Role::TextInput, "Name template");
    page(&mut h, "Outputs");
    h.get_by_label("CSV BOM");
    h.get_by_label("Remove");
    assert!(!disabled(&h, "Run"));
}

#[test]
fn general_previews_the_paths_and_flags_the_name_template_inline() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Edit…");
    h.run();
    h.get_by_label("outputs/Generic/Jobs-Generic.zip");
    h.get_by_label("outputs/Generic/Jobs-Generic-BOM.csv");
    h.get_by_label("Jobs.gtl");
    retype(&mut h, "Name template", "${project}-${rev}");
    // The field and the problem under it.
    assert!(h.get_all_by_label_contains("rev").count() >= 2);
    assert!(disabled(&h, "OK") && disabled(&h, "Run"));
    h.get_by_label("outputs/Generic/Jobs-Generic.zip");

    retype(&mut h, "Name template", "Jöbs");
    h.get_by_label("outputs/Generic/Jöbs.zip");
    h.get_by_label_contains("non-ASCII");
    assert!(!disabled(&h, "OK"));
    h.get_by_label("OK").click();
    h.run();
    let project = h.state().shell().engine().project().unwrap();
    let job = project.output_jobs().next().unwrap();
    assert_eq!(job.name_template.as_str(), "Jöbs");
}

#[test]
fn gerber_and_drill_pages_list_each_file_and_whether_it_is_plotted() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Edit…");
    h.run();
    page(&mut h, "Gerber");
    for label in [
        "Layer",
        "File name",
        "Top Copper",
        "Jobs.gtl",
        "Board Outline",
        "Jobs.gm1",
    ] {
        h.get_by_label(label);
    }
    assert!(h.get_all_by_label("Empty, left out").count() >= 4);
    h.get_by_label("Edit preset copy…");
    page(&mut h, "Drill");
    h.get_by_label("Routed (G00, M15, G01, M16, G05)");
    h.get_by_label("Split: plated and non-plated files");
    h.get_by_label("Jobs-PTH.drl");
}

#[test]
fn bom_and_cpl_pages_preview_the_writers_rows() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness_with_parts(home.path(), work.path());
    job_menu(&mut h, "Edit…");
    h.run();
    page(&mut h, "BOM");
    for label in [
        "Designator",
        "Quantity",
        "Value",
        "R1,R2",
        "10k",
        "RESC1608X55N",
    ] {
        h.get_by_label(label);
    }
    // No resistor names a manufacturer: an empty column shows before the upload.
    h.get_by_label("Manufacturer (empty)");
    h.get_by_label("from preset Generic");
    page(&mut h, "CPL");
    for label in ["Mid X", "Rotation", "R1", "R2"] {
        h.get_by_label(label);
    }
}

#[test]
fn edit_preset_copy_opens_that_page_of_the_preset_in_project_settings() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Edit…");
    h.run();
    page(&mut h, "BOM");
    h.get_by_label("Edit preset copy…").click();
    h.run();
    let windows = h.state().settings_windows();
    let dialog = windows
        .dialog(reda_ui::layout::SettingsWindow::ProjectSettings)
        .expect("Project Settings opened");
    let shown = &dialog.pages()[dialog.selected()];
    let copy = (shown.as_ref() as &dyn std::any::Any)
        .downcast_ref::<reda_ui::settings::project::CopyPage>()
        .expect("a preset copy page");
    assert_eq!(copy.page(), reda_ui::settings::fab_preset::PresetPage::Bom);
}

#[test]
fn last_run_reads_the_manifest_and_says_not_run_yet_without_one() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Edit…");
    h.run();
    // The Properties panel says "Not run yet" of the checks too: the page adds one.
    let before = h.query_all_by_label("Not run yet").count();
    page(&mut h, "Last run");
    assert_eq!(h.query_all_by_label("Not run yet").count(), before + 1);
    assert!(disabled(&h, "Show in Folder"));

    h.get_by_role_and_label(Role::Button, "Run").click();
    h.step();
    wait_for(&mut h, "Generic finished");
    h.get_all_by_label("Close").last().unwrap().click();
    h.run();
    assert_eq!(h.query_all_by_label("Not run yet").count(), before);
    h.get_by_label_contains("Fab Preset Generic.");
    h.get_by_label_contains("files match the manifest");
    h.get_by_label("fab/Jobs.gtl");
    h.get_by_label("Jobs-Generic.zip");
    assert!(!disabled(&h, "Show in Folder"));
}

#[test]
fn run_all_on_the_output_jobs_node_runs_every_job_with_one_report() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Duplicate");
    h.run();
    h.get_by_label("Output Jobs").click_secondary();
    h.run();
    h.get_by_label("Run All").click();
    h.step();
    wait_for(&mut h, "Run all finished");
    h.get_by_label("Generic: written");
    h.get_by_label("Generic copy: written");
    h.get_by_role_and_label(Role::Button, "Show Generic copy in Folder");
    assert_eq!(last_journal_id(&h), "output.job.run_all");
    h.get_all_by_label("Close").last().unwrap().click();
    h.run();
    assert!(h.query_by_label("Run all finished").is_none());
}

struct Window {
    dialog: reda_ui::settings::SettingsDialog,
    shell: Shell,
    outcome: reda_ui::settings::Outcome,
}

#[test]
fn show_in_folder_on_last_run_asks_for_the_destination() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .run(
            "project.new",
            json!({"path": work.path(), "name": "Jobs", "board": {"width": "30mm", "height": "20mm"}}),
        )
        .unwrap();
    let job = shell
        .engine()
        .project()
        .unwrap()
        .output_jobs()
        .next()
        .unwrap()
        .uuid;
    shell.run("output.job.run", json!({"job": job})).unwrap();
    let mut dialog = output_job::dialog(shell.engine().project().unwrap(), job).unwrap();
    dialog.select(6);
    let state = Window {
        dialog,
        shell,
        outcome: reda_ui::settings::Outcome::Open,
    };
    let mut h = Harness::builder()
        .with_size([1000.0, 700.0])
        .build_ui_state(
            |ui, w: &mut Window| {
                let outcome = w.dialog.show(ui, &mut w.shell);
                if outcome != reda_ui::settings::Outcome::Open {
                    w.outcome = outcome;
                }
            },
            state,
        );
    h.run();
    h.get_by_label("Show in Folder").click();
    h.run();
    let folder = work.path().join("Jobs").join("outputs").join("Generic");
    assert_eq!(
        h.state().outcome,
        reda_ui::settings::Outcome::ShowFolder(folder)
    );
}
