//! Guides in the Properties panel, driven through AccessKit with `egui_kittest`
//! (docs/spec/v0.3/grids-guides.md#editing): with nothing selected the board's Grids &
//! Guides region has a row per Guide whose name selects it, with shown and enabled
//! switches and Remove, the Show Grids & Guides toggle and Add; a selected line Guide
//! takes a typed angle.

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::Millideg;
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::{Value, json};

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
    shell
        .run("document.open", json!({"document": PCB}))
        .unwrap();
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

fn guide(h: &mut Harness<'_, App>, name: &str) -> Uuid {
    let out = run(
        h,
        "pcb.guide.add",
        json!({"kind": "line", "at": {"x": "1mm", "y": "2mm"}, "angle": 0, "name": name}),
    );
    serde_json::from_value(out["result"]["uuid"].clone()).unwrap()
}

fn shown_and_enabled(h: &Harness<'_, App>, g: Uuid) -> (bool, bool) {
    let project = h.state().shell().engine().project().unwrap();
    let guide = project.pcb().unwrap().guide(g).unwrap();
    (guide.shown, guide.enabled)
}

#[test]
fn the_grids_and_guides_region_lists_selects_switches_and_removes_guides() {
    let (home, work) = (
        TempDir::new("guides-panel-home"),
        TempDir::new("guides-panel-work"),
    );
    let mut h = harness(&home, &work);
    let g = guide(&mut h, "Datum");
    h.run();
    h.get_by_label("Grids & Guides");

    h.get_by_role_and_label(Role::CheckBox, "Shown").click();
    h.run();
    assert_eq!(
        last_entry(&h),
        ("pcb.guide.set".into(), json!({"uuid": g, "shown": false}))
    );
    assert_eq!(shown_and_enabled(&h, g), (false, true));
    h.get_by_role_and_label(Role::CheckBox, "Enabled").click();
    h.run();
    assert_eq!(shown_and_enabled(&h, g), (false, false));

    h.get_by_role_and_label(Role::CheckBox, "Show Grids & Guides")
        .click();
    h.run();
    assert_eq!(last_entry(&h).0, "view.grids_guides.show");
    assert!(h.state().shell().grids().aids_hidden);

    h.get_by_label("Datum").click();
    h.run();
    assert_eq!(h.state().shell().selection().objects, [g]);
    run(&mut h, "selection.set", json!({}));
    h.run();

    h.get_by_role_and_label(Role::Button, "Remove").click();
    h.run();
    assert_eq!(
        last_entry(&h),
        ("pcb.guide.delete".into(), json!({"uuid": g}))
    );
    let project = h.state().shell().engine().project().unwrap();
    assert_eq!(project.pcb().unwrap().guides().count(), 0);
    h.get_by_label("No Guides on this board.");

    h.get_by_role_and_label(Role::Button, "Add Point Guide")
        .click();
    h.run();
    assert_eq!(h.state().shell().tool(), Tool::PointGuide);
}

#[test]
fn a_selected_line_guide_takes_a_typed_angle() {
    let (home, work) = (
        TempDir::new("guide-angle-home"),
        TempDir::new("guide-angle-work"),
    );
    let mut h = harness(&home, &work);
    let g = guide(&mut h, "Datum");
    run(&mut h, "selection.set", json!({"objects": [g]}));
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Angle").focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Angle")
        .set_text_value_accesskit("30");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        last_entry(&h),
        ("pcb.guide.set".into(), json!({"uuid": g, "angle": 30_000}))
    );
    let project = h.state().shell().engine().project().unwrap();
    assert_eq!(
        project.pcb().unwrap().guide(g).unwrap().angle,
        Some(Millideg(30_000))
    );
}
