//! Local Grids in the Properties panel, driven through AccessKit with `egui_kittest`
//! (docs/spec/v0.3/grids-guides.md#local-grids): with nothing selected the board's Grids
//! & Guides region lists them in priority order, a row each whose name selects it, with
//! shown and enabled switches and Remove, and a grip that drags it up or down; a selected
//! Local Grid takes its fields.

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::geom::Nm;
use reda_ui::engine::model::{AppliesTo, LocalGrid, LocalGridShape, Uuid};
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

fn local_grid(h: &mut Harness<'_, App>, name: &str, x: &str) -> Uuid {
    let out = run(
        h,
        "pcb.local_grid.add",
        json!({"name": name, "shape": {"kind": "cartesian", "origin": {"x": x, "y": 0},
               "step_x": "0.3mm", "width": "4mm", "height": "4mm"}}),
    );
    serde_json::from_value(out["result"]["uuid"].clone()).unwrap()
}

fn grids(h: &Harness<'_, App>) -> Vec<LocalGrid> {
    let project = h.state().shell().engine().project().unwrap();
    project.pcb().unwrap().local_grids().cloned().collect()
}

fn names(h: &Harness<'_, App>) -> Vec<String> {
    grids(h).into_iter().map(|g| g.name).collect()
}

fn type_into(h: &mut Harness<'_, App>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .set_text_value_accesskit(text);
    h.run();
    h.key_press(Key::Enter);
    h.run();
}

#[test]
fn the_grids_and_guides_region_lists_switches_removes_and_drags_local_grids() {
    let (home, work) = (
        TempDir::new("local-grids-panel-home"),
        TempDir::new("local-grids-panel-work"),
    );
    let mut h = harness(&home, &work);
    h.run();
    h.get_by_label("No Local Grids on this board.");
    let a = local_grid(&mut h, "A", "0mm");
    let b = local_grid(&mut h, "B", "10mm");
    h.run();
    h.get_by_label("Local Grids");
    assert!(
        h.get_by_label("A: 0.3mm, All").rect().top() < h.get_by_label("B: 0.3mm, All").rect().top(),
        "the rows follow the priority order"
    );

    h.get_by_role_and_label(Role::CheckBox, "Shown: A").click();
    h.run();
    assert_eq!(
        last_entry(&h),
        (
            "pcb.local_grid.set".into(),
            json!({"uuid": a, "shown": false})
        )
    );
    h.get_by_role_and_label(Role::CheckBox, "Enabled: B")
        .click();
    h.run();
    assert_eq!(
        last_entry(&h),
        (
            "pcb.local_grid.set".into(),
            json!({"uuid": b, "enabled": false})
        )
    );

    h.get_by_label("B: 0.3mm, All").click();
    h.run();
    assert_eq!(h.state().shell().selection().objects, [b]);
    run(&mut h, "selection.set", json!({}));
    h.run();

    // Dragging B's grip onto the top half of A's row puts it first.
    let from = h.get_by_label("Drag B to reorder").rect().center();
    let target = h.get_by_label("A: 0.3mm, All").rect();
    let to = egui::pos2(target.center().x, target.top() + 1.0);
    h.hover_at(from);
    h.run();
    h.drag_at(from);
    h.run();
    for step in 1..=5 {
        h.hover_at(from + (to - from) * (step as f32 / 5.0));
        h.run();
    }
    h.drop_at(to);
    h.run();
    assert_eq!(
        last_entry(&h),
        (
            "pcb.local_grid.reorder".into(),
            json!({"local_grid": b, "before": a})
        )
    );
    assert_eq!(names(&h), ["B", "A"]);

    h.get_by_role_and_label(Role::Button, "Remove A").click();
    h.run();
    assert_eq!(
        last_entry(&h),
        ("pcb.local_grid.delete".into(), json!({"uuid": a}))
    );
    assert_eq!(names(&h), ["B"]);

    h.get_by_role_and_label(Role::Button, "Add Local Grid")
        .click();
    h.run();
    assert_eq!(h.state().shell().tool(), Tool::LocalGrid);
}

#[test]
fn a_selected_local_grid_takes_its_size_step_name_and_applies_to() {
    let (home, work) = (
        TempDir::new("local-grid-fields-home"),
        TempDir::new("local-grid-fields-work"),
    );
    let mut h = harness(&home, &work);
    let a = local_grid(&mut h, "A", "0mm");
    run(&mut h, "selection.set", json!({"objects": [a]}));
    h.run();
    type_into(&mut h, "Width", "8");
    let LocalGridShape::Cartesian(c) = grids(&h)[0].shape;
    assert_eq!(c.width, Nm(8_000_000));
    assert_eq!(c.height, Nm(4_000_000), "the rest of the shape is kept");
    type_into(&mut h, "Step Y", "25mil");
    let LocalGridShape::Cartesian(c) = grids(&h)[0].shape;
    assert_eq!(
        (c.step_x.to_string(), c.step_y.to_string()),
        ("0.3mm".into(), "25mil".into())
    );
    type_into(&mut h, "Name", "Fine");
    assert_eq!(names(&h), ["Fine"]);

    h.get_by_role_and_label(Role::ComboBox, "Applies to")
        .click();
    h.run();
    h.get_by_label("All but Components").click();
    h.run();
    assert_eq!(
        last_entry(&h),
        (
            "pcb.local_grid.set".into(),
            json!({"uuid": a, "applies_to": "not_components"})
        )
    );
    assert_eq!(grids(&h)[0].applies_to, AppliesTo::NotComponents);
}
