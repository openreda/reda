//! The schematic canvas in the window, driven with `egui_kittest`: a real mouse drag
//! previews without touching the Journal and commits exactly one move Command on release;
//! the keys and the Edit menu run their Commands.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Modifiers, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const MIL: f64 = 25_400.0;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn harness(dir: &TempDir) -> Harness<'static, App> {
    harness_with_step(dir, 0.25)
}

fn harness_with_step(dir: &TempDir, step: f32) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    Harness::builder()
        .with_step_dt(step)
        .with_max_steps(100)
        .with_size([1200.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    h.state_mut().shell_mut().run(id, params).unwrap()
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

/// A resistor at the origin and a Wire up from its Pin 1, at (0, 150) mil.
fn resistor_and_wire(h: &mut Harness<'_, App>) -> Value {
    let unit = run(
        h,
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": {"x": 0, "y": 0}}),
    )["result"]["unit"]
        .clone();
    run(
        h,
        "schematic.wire.add",
        json!({"document": SHEET, "points": [{"x": 0, "y": "150mil"}, {"x": 0, "y": "500mil"}]}),
    );
    h.run();
    unit
}

/// Where the sheet point (`x`, `y`) mil is on screen.
fn screen(h: &Harness<'_, App>, x: f64, y: f64) -> Pos2 {
    let rect = h.get_by_role(Role::Canvas).rect();
    let camera = h
        .state()
        .shell()
        .document_tab(sheet())
        .unwrap()
        .camera
        .unwrap();
    camera.screen_point(rect, (x * MIL, y * MIL))
}

/// The ids of the Journal's successful entries from `from` on.
fn journal_since(h: &Harness<'_, App>, from: usize) -> Vec<String> {
    h.state().shell().engine().journal()[from..]
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| e.id.clone())
        .collect()
}

#[test]
fn selection_click_toggles_with_shift_and_deselects_the_only_object() {
    let dir = TempDir::new("canvas-selection-rules");
    let mut h = harness(&dir);
    let unit = resistor_and_wire(&mut h);
    let click = |h: &mut Harness<'_, App>, at: Pos2| {
        h.drag_at(at);
        h.drop_at(at);
        h.run();
    };
    let body = screen(&h, 0.0, 120.0);
    click(&mut h, body);
    assert_eq!(
        h.state().shell().selection().objects,
        [unit.as_str().unwrap().parse::<Uuid>().unwrap()]
    );
    click(&mut h, body);
    assert!(
        h.state().shell().selection().is_empty(),
        "the only selected object deselects"
    );
    click(&mut h, body);
    h.event(egui::Event::ModifiersChanged(Modifiers::SHIFT));
    click(&mut h, body);
    assert!(
        h.state().shell().selection().is_empty(),
        "Shift toggles a selected object out"
    );
}

#[test]
fn selection_filter_governs_mouse_rectangle_and_select_all_but_explicit_selection_bypasses_it() {
    let dir = TempDir::new("canvas-selection-filter");
    let mut h = harness(&dir);
    let unit = resistor_and_wire(&mut h);
    run(
        &mut h,
        "selection.filter.set",
        json!({"document": SHEET, "kinds": {"unit": false}}),
    );
    let body = screen(&h, 0.0, 120.0);
    h.drag_at(body);
    h.drop_at(body);
    h.run();
    assert!(
        h.state().shell().selection().is_empty(),
        "filtered Unit cannot be clicked"
    );
    let from = screen(&h, -500.0, -500.0);
    let to = screen(&h, 500.0, 600.0);
    h.drag_at(from);
    h.run();
    h.hover_at(to);
    h.run();
    h.drop_at(to);
    h.run();
    assert!(
        !h.state()
            .shell()
            .selection()
            .objects
            .contains(&unit.as_str().unwrap().parse().unwrap())
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    assert!(
        !h.state()
            .shell()
            .selection()
            .objects
            .contains(&unit.as_str().unwrap().parse().unwrap())
    );
    run(&mut h, "selection.set", json!({"objects": [unit]}));
    assert_eq!(h.state().shell().selection().objects.len(), 1);
}

#[test]
fn click_clears_and_directional_rectangle_preferences_apply_to_the_canvas() {
    let dir = TempDir::new("canvas-selection-preferences");
    let mut h = harness(&dir);
    let unit = resistor_and_wire(&mut h);
    run(
        &mut h,
        "app.preferences.set",
        json!({"key": "editing.click_clears_selection", "value": false}),
    );
    run(&mut h, "selection.set", json!({"objects": [unit]}));
    let empty = screen(&h, -600.0, -500.0);
    h.drag_at(empty);
    h.drop_at(empty);
    h.run();
    assert_eq!(
        h.state().shell().selection().objects.len(),
        1,
        "empty click keeps selection when the preference is off"
    );
    run(
        &mut h,
        "app.preferences.set",
        json!({"key": "editing.directional_rectangle", "value": false}),
    );
    let from = screen(&h, 400.0, 400.0);
    let to = screen(&h, -400.0, 200.0);
    h.drag_at(from);
    h.run();
    h.hover_at(to);
    h.run();
    h.drop_at(to);
    h.run();
    assert!(
        h.state().shell().selection().is_empty(),
        "right-to-left rectangle must enclose with directional selection off"
    );
}

#[test]
fn overlapping_objects_rank_smallest_first_and_shift_tab_cycles() {
    let dir = TempDir::new("canvas-selection-stack");
    let mut h = harness(&dir);
    let mut wires = Vec::new();
    for points in [
        json!([{"x": "-100mil", "y": 0}, {"x": "100mil", "y": 0}]),
        json!([{"x": 0, "y": "-500mil"}, {"x": 0, "y": "500mil"}]),
    ] {
        wires.push(
            run(
                &mut h,
                "schematic.wire.add",
                json!({"document": SHEET, "points": points}),
            )["result"]["wires"][0]
                .as_str()
                .unwrap()
                .parse::<Uuid>()
                .unwrap(),
        );
    }
    assert_ne!(wires[0], wires[1]);
    h.run();
    let at = screen(&h, 0.0, 0.0);
    h.drag_at(at);
    h.drop_at(at);
    h.run();
    assert_eq!(
        h.state().shell().selection().objects,
        [wires[0]],
        "the short Wire wins over one drawn later"
    );
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run();
    assert_eq!(h.state().shell().selection().objects, [wires[1]]);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run();
    assert_eq!(h.state().shell().selection().objects, [wires[0]]);
}

#[test]
fn component_field_text_can_be_selected_and_moved_alone_but_cannot_be_deleted() {
    let dir = TempDir::new("canvas-field-selection");
    let mut h = harness(&dir);
    let unit = resistor_and_wire(&mut h);
    let id: Uuid = unit.as_str().unwrap().parse().unwrap();
    let (x, y) = {
        let scene = &h.state().shell().document_tab(sheet()).unwrap().scene.scene;
        let run = scene
            .items
            .iter()
            .find_map(|item| match &item.primitive {
                reda_ui::engine::render::Primitive::Glyphs(run)
                    if item.object == id && run.text == "R1" =>
                {
                    Some(run)
                }
                _ => None,
            })
            .unwrap();
        (run.at.x.0 as f64 / MIL, run.at.y.0 as f64 / MIL)
    };
    let at = screen(&h, x + 10.0, y);
    h.drag_at(at);
    h.drop_at(at);
    h.run();
    assert!(
        h.state().shell().selection().objects.is_empty(),
        "selecting field text does not select its parent"
    );
    assert_eq!(h.state().shell().selection().len(), 1);
    h.drag_at(at);
    h.run();
    h.hover_at(at + vec2(50.0, 0.0));
    h.run();
    h.drop_at(at + vec2(50.0, 0.0));
    h.run();
    let unit_before_delete = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .unit(id)
        .unwrap()
        .clone();
    assert_eq!(
        unit_before_delete.at,
        reda_ui::engine::model::geom::Point::default()
    );
    assert!(
        unit_before_delete.field_text("designator").is_some(),
        "drag moves the field text alone"
    );
    h.key_press(Key::Delete);
    h.run();
    assert!(
        h.state()
            .shell()
            .message()
            .is_some_and(|message| message.contains("cannot be deleted alone")),
        "standalone field deletion reports its refusal"
    );
    assert_eq!(
        h.state()
            .shell()
            .engine()
            .project()
            .unwrap()
            .schematic_sheet(sheet())
            .unwrap()
            .unit(id),
        Some(&unit_before_delete)
    );
}

#[test]
fn double_clicking_a_stack_opens_a_pick_list_and_hover_previews_without_selecting() {
    let dir = TempDir::new("canvas-pick-list");
    let mut h = harness_with_step(&dir, 0.01);
    let unit = resistor_and_wire(&mut h);
    let wire = run(
        &mut h,
        "schematic.wire.add",
        json!({"document": SHEET, "points":[{"x":"-500mil","y":0},{"x":"500mil","y":0}]}),
    )["result"]["wires"][0]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    h.run();
    let at = screen(&h, 0.0, 0.0);
    for _ in 0..2 {
        h.drag_at(at);
        h.drop_at(at);
        h.run();
    }
    h.get_by_label("Pick Object");
    let before = h.state().shell().selection().clone();
    let label = format!("wire · {wire}");
    h.get_by_label(&label).hover();
    h.run();
    assert_eq!(
        h.state().shell().selection(),
        &before,
        "preview does not commit selection"
    );
    assert_eq!(
        h.state()
            .shell()
            .document_tab(sheet())
            .unwrap()
            .live
            .hovered,
        Some(wire)
    );
    h.get_by_label(&label).click();
    h.run();
    assert_eq!(h.state().shell().selection().objects, [wire]);
    assert_ne!(wire, unit.as_str().unwrap().parse::<Uuid>().unwrap());
}

#[test]
fn a_drag_commits_exactly_one_move_command() {
    let dir = TempDir::new("canvas-drag");
    let mut h = harness(&dir);
    let unit = resistor_and_wire(&mut h);
    let before = h.state().shell().engine().journal().len();
    let project = h.state().shell().engine().project().unwrap().clone();

    // Grab the resistor by its Pin 1 stub and drag it right in several steps.
    let start = screen(&h, 0.0, 120.0);
    h.hover_at(start);
    h.run();
    h.drag_at(start);
    h.run();
    for step in 1..=6 {
        h.hover_at(start + vec2(15.0 * step as f32, 0.0));
        h.run();
    }
    // In flight: drawn moved, but nothing is committed and the Journal only selected.
    let tab = h.state().shell().document_tab(sheet()).unwrap();
    assert!(tab.moving_offset.0 > 0.0);
    assert!(h.state().shell().engine().project().unwrap() == &project);
    assert_eq!(journal_since(&h, before), ["selection.set"]);

    h.drop_at(start + vec2(90.0, 0.0));
    h.run();
    h.run();
    let ids = journal_since(&h, before);
    assert_eq!(ids, ["selection.set", "schematic.drag"]);
    let moved = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .unit(unit.as_str().unwrap().parse().unwrap())
        .unwrap()
        .at;
    // Snapped: the grabbed Pin lands on the 50 mil grid.
    assert!(
        moved.x.0 > 0 && moved.x.0 % 1_270_000 == 0 && moved.y.0 % 1_270_000 == 0,
        "{moved:?}"
    );
    // Rubber-banded: the Wire now turns a corner to the moved Pin, a vertex of its own.
    let project = h.state().shell().engine().project().unwrap();
    let wires: Vec<_> = project.schematic_sheet(sheet()).unwrap().wires().collect();
    assert_eq!(wires.len(), 1);
    assert_eq!(wires[0].vertices.len(), 3);
    let tab = h.state().shell().document_tab(sheet()).unwrap();
    assert!(tab.live.moving.is_empty());
}

#[test]
fn keys_on_the_canvas_run_commands_and_the_edit_menu_aligns() {
    let dir = TempDir::new("canvas-keys");
    let mut h = harness(&dir);
    resistor_and_wire(&mut h);
    let before = h.state().shell().engine().journal().len();
    // A click on the canvas gives it the keyboard and selects the resistor.
    h.get_by_role(Role::Canvas).hover();
    let pin = screen(&h, 0.0, 120.0);
    h.hover_at(pin);
    h.run();
    h.drag_at(pin);
    h.run();
    h.drop_at(pin);
    h.run();
    h.key_press(Key::G);
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::D);
    h.run();
    h.key_press(Key::Delete);
    h.run();
    let ids = journal_since(&h, before);
    assert!(ids.contains(&"view.grid.cycle".to_owned()), "{ids:?}");
    assert!(ids.contains(&"schematic.duplicate".to_owned()), "{ids:?}");
    assert_eq!(ids.last().unwrap(), "schematic.delete");

    // Select everything and align it from the Edit menu.
    h.hover_at(pin);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(Role::Button, "Edit").click();
    h.run();
    h.get_by_label("Align").hover();
    h.run();
    h.get_by_label("Align Left").click();
    h.run();
    assert_eq!(journal_since(&h, before).last().unwrap(), "schematic.align");
}
