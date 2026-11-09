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
use reda_ui::working_state::FilterKind;
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
    harness_with_library(dir, step, library())
}

fn harness_with_library(dir: &TempDir, step: f32, library: PathBuf) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
        .starter_library(Some(library))
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
        !h.state()
            .shell()
            .selection()
            .objects
            .contains(&unit.as_str().unwrap().parse().unwrap()),
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
    assert!(
        h.state()
            .shell()
            .engine()
            .journal()
            .iter()
            .any(|entry| entry.id == "selection.cycle" && entry.outcome.is_ok())
    );
    run(
        &mut h,
        "selection.filter.set",
        json!({"document":SHEET,"all":false}),
    );
    run(&mut h, "selection.set", json!({}));
    run(&mut h, "selection.cycle", json!({"document":SHEET}));
    assert!(
        h.state().shell().selection().is_empty(),
        "cycling cannot reuse a filtered stack before another hover"
    );
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
    run(&mut h, "selection.set", json!({"objects":[unit]}));
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
    assert!(
        h.state()
            .shell()
            .engine()
            .journal()
            .iter()
            .any(|entry| entry.id == "selection.delete" && entry.outcome.is_err())
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
    let label = "Wire · 25.40 mm";
    h.get_by_label(label).hover();
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
    h.get_by_label(label).click();
    h.run();
    assert_eq!(h.state().shell().selection().objects, [wire]);
    assert_ne!(wire, unit.as_str().unwrap().parse::<Uuid>().unwrap());
}

#[test]
fn properties_and_active_bar_share_the_selection_filter() {
    let dir = TempDir::new("canvas-filter-controls");
    let mut h = harness(&dir);
    run(&mut h, "panel.show", json!({"panel":"properties"}));
    h.run();
    h.get_by_label("Select Wire").click();
    h.run();
    assert!(
        !h.state()
            .shell()
            .selection_filter(sheet())
            .allows(FilterKind::Wire)
    );
    h.get_by_role_and_label(Role::Button, "Selection Filter")
        .click();
    h.run();
    h.get_all_by_label("All").last().unwrap().click();
    h.run();
    assert!(
        h.state()
            .shell()
            .selection_filter(sheet())
            .allows(FilterKind::Wire)
    );
}

#[test]
fn repeat_click_cycles_within_three_pixels_and_six_hundred_ms() {
    let dir = TempDir::new("canvas-repeat-pick");
    let mut h = harness_with_step(&dir, 0.01);
    let mut ids = Vec::new();
    for points in [
        json!([{"x":"-100mil","y":0},{"x":"100mil","y":0}]),
        json!([{"x":0,"y":"-500mil"},{"x":0,"y":"500mil"}]),
    ] {
        ids.push(
            run(
                &mut h,
                "schematic.wire.add",
                json!({"document":SHEET,"points":points}),
            )["result"]["wires"][0]
                .as_str()
                .unwrap()
                .parse::<Uuid>()
                .unwrap(),
        );
    }
    h.run();
    let at = screen(&h, 0.0, 0.0);
    let click = |h: &mut Harness<'_, App>, at: Pos2, gap: f64| {
        h.input_mut().time = Some(h.ctx.input(|i| i.time) + gap);
        h.drag_at(at);
        h.drop_at(at);
        h.run();
    };
    click(&mut h, at, 0.7);
    assert_eq!(h.state().shell().selection().objects, [ids[0]]);
    click(&mut h, at + vec2(2.0, 0.0), 0.35);
    assert_eq!(
        h.state().shell().selection().objects,
        [ids[1]],
        "repeat within both limits cycles"
    );
    click(&mut h, at + vec2(6.0, 0.0), 0.35);
    assert_eq!(
        h.state().shell().selection().objects,
        [ids[0]],
        "a click more than three pixels away starts again"
    );
    click(&mut h, at, 0.7);
    assert!(
        h.state().shell().selection().objects != [ids[1]],
        "a late click cannot cycle to the next object"
    );
}

#[test]
fn double_clicking_a_single_object_selects_it_and_opens_properties() {
    let dir = TempDir::new("canvas-single-properties");
    let mut h = harness_with_step(&dir, 0.01);
    let wire = run(
        &mut h,
        "schematic.wire.add",
        json!({"document":SHEET,"points":[{"x":"-100mil","y":0},{"x":"100mil","y":0}]}),
    )["result"]["wires"][0]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    h.run();
    let before = h.state().shell().engine().journal().len();
    let at = screen(&h, 0.0, 0.0);
    for _ in 0..2 {
        h.drag_at(at);
        h.drop_at(at);
        h.run();
    }
    assert_eq!(h.state().shell().selection().objects, [wire]);
    assert!(
        h.state().shell().engine().journal()[before..]
            .iter()
            .any(|entry| entry.id == "panel.show" && entry.outcome.is_ok()),
        "double click opens Properties"
    );
}

#[test]
fn explicitly_selected_parent_and_field_move_the_text_once() {
    let dir = TempDir::new("canvas-parent-field-move");
    let mut h = harness(&dir);
    let unit = resistor_and_wire(&mut h);
    let id: Uuid = unit.as_str().unwrap().parse().unwrap();
    run(
        &mut h,
        "schematic.field.move",
        json!({"unit":unit,"field":"designator","at":{"x":"500mil","y":"500mil"}}),
    );
    run(
        &mut h,
        "selection.set",
        json!({"objects":[unit],"fields":[{"unit":unit,"field":"designator"}]}),
    );
    let old = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .unit(id)
        .unwrap()
        .field_text("designator")
        .unwrap()
        .clone();
    let from = screen(&h, 510.0, 500.0);
    h.drag_at(from);
    h.run();
    h.hover_at(from + vec2(60.0, 0.0));
    h.run();
    h.drop_at(from + vec2(60.0, 0.0));
    h.run();
    let unit = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .unit(id)
        .unwrap();
    assert!(unit.at.x.0 > 0);
    assert_eq!(
        unit.field_text("designator"),
        Some(&old),
        "the child keeps its local offset, without a second move"
    );
}

#[test]
fn dragging_a_selected_field_preserves_and_moves_the_other_selected_fields() {
    let dir = TempDir::new("canvas-multiple-fields");
    let mut h = harness(&dir);
    let first = resistor_and_wire(&mut h);
    let second = run(
        &mut h,
        "schematic.component.place",
        json!({"document":SHEET,"part":R_10K,"at":{"x":"1000mil","y":0}}),
    )["result"]["unit"]
        .clone();
    for (unit, x) in [(&first, "300mil"), (&second, "1300mil")] {
        run(
            &mut h,
            "schematic.field.move",
            json!({"unit":unit,"field":"designator","at":{"x":x,"y":"500mil"}}),
        );
    }
    run(
        &mut h,
        "selection.set",
        json!({"fields":[{"unit":first,"field":"designator"},{"unit":second,"field":"designator"}]}),
    );
    h.run();
    let at = screen(&h, 310.0, 500.0);
    h.drag_at(at);
    h.run();
    h.hover_at(at + vec2(60.0, 0.0));
    h.run();
    h.drop_at(at + vec2(60.0, 0.0));
    h.run();
    assert_eq!(
        h.state().shell().selection().fields.len(),
        2,
        "a drag never deselects another field"
    );
    let sheet = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap();
    let offsets: Vec<_> = [first, second]
        .iter()
        .map(|id| {
            sheet
                .unit(id.as_str().unwrap().parse().unwrap())
                .unwrap()
                .field_text("designator")
                .unwrap()
                .offset
        })
        .collect();
    assert!(offsets[0].x.0 > 300 * 25_400);
    assert_eq!(
        offsets[0], offsets[1],
        "both fields move by the same delta in their parent frames"
    );
}

#[test]
fn select_similar_honours_filter_changes_and_refuses_a_filtered_template() {
    let dir = TempDir::new("canvas-similar-filter");
    let mut h = harness(&dir);
    let unit = resistor_and_wire(&mut h);
    run(&mut h, "selection.set", json!({"objects":[unit]}));
    run(&mut h, "schematic.dialog.open:select_similar", json!({}));
    h.run();
    h.get_by_label("Select Similar to R1");
    run(
        &mut h,
        "selection.filter.set",
        json!({"document":SHEET,"kinds":{"unit":false}}),
    );
    h.get_all_by_label("Select").last().unwrap().click();
    h.run();
    assert!(
        h.state().shell().selection().is_empty(),
        "Apply filters the results using current working state"
    );
    run(&mut h, "selection.set", json!({"objects":[unit]}));
    run(&mut h, "schematic.dialog.open:select_similar", json!({}));
    h.run();
    assert!(
        h.query_by_label("Select Similar to R1").is_none(),
        "a filtered template cannot open Select Similar"
    );
}

#[test]
fn dragging_an_eligible_object_leaves_filtered_objects_in_the_selection_in_place() {
    let dir = TempDir::new("canvas-filtered-selection-move");
    let mut h = harness(&dir);
    let unit = resistor_and_wire(&mut h);
    let wire = run(
        &mut h,
        "schematic.wire.add",
        json!({"document":SHEET,"points":[{"x":"1500mil","y":0},{"x":"2500mil","y":0}]}),
    )["result"]["wires"][0]
        .clone();
    run(&mut h, "selection.set", json!({"objects":[unit,wire]}));
    run(
        &mut h,
        "selection.filter.set",
        json!({"document":SHEET,"kinds":{"unit":false}}),
    );
    h.run();
    let at = screen(&h, 2000.0, 0.0);
    h.drag_at(at);
    h.run();
    h.hover_at(at + vec2(50.0, 0.0));
    h.run();
    h.drop_at(at + vec2(50.0, 0.0));
    h.run();
    assert_eq!(
        h.state()
            .shell()
            .engine()
            .project()
            .unwrap()
            .schematic_sheet(sheet())
            .unwrap()
            .unit(unit.as_str().unwrap().parse().unwrap())
            .unwrap()
            .at,
        reda_ui::engine::model::geom::Point::default(),
        "a filtered Unit cannot move as part of a selection"
    );
    let selected = &h.state().shell().selection().objects;
    assert_eq!(selected.len(), 2, "a drag preserves filtered selections");
    assert!(selected.contains(&unit.as_str().unwrap().parse().unwrap()));
    assert!(selected.contains(&wire.as_str().unwrap().parse().unwrap()));
    let wire = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .wire(wire.as_str().unwrap().parse().unwrap())
        .unwrap();
    assert!(
        wire.vertices[0].x.0 > 1500 * 25_400,
        "the eligible Wire moves"
    );
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
    // Mod+D starts stamping copies (Duplicate), which floats them on the cursor.
    assert!(ids.contains(&"tool.duplicate".to_owned()), "{ids:?}");
    assert_eq!(ids.last().unwrap(), "schematic.delete");
    // Esc ends the stamping.
    h.key_press(Key::Escape);
    h.run();

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

#[test]
fn component_body_ranks_after_larger_field_text_but_pins_keep_their_priority() {
    let dir = TempDir::new("canvas-body-text-priority");
    let lib = dir.path().join("Test");
    for folder in ["", "symbols", "parts", "footprints"] {
        std::fs::create_dir_all(lib.join(folder)).unwrap();
        for entry in std::fs::read_dir(library().join(folder)).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_file() {
                std::fs::copy(entry.path(), lib.join(folder).join(entry.file_name())).unwrap();
            }
        }
    }
    let path = lib.join("symbols/R.redasym");
    let mut symbol: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    symbol["graphics"] = json!([{"uuid":"a0000000-0000-4000-8000-000000000099","unit":"a0000000-0000-4000-8000-000000000011","shape":{"kind":"rect","start":{"x":-500000,"y":-1000000},"end":{"x":500000,"y":1000000}},"width":100000,"fill":true}]);
    std::fs::write(path, serde_json::to_vec(&symbol).unwrap()).unwrap();
    let mut h = harness_with_library(&dir, 0.25, lib);
    let unit = resistor_and_wire(&mut h);
    run(
        &mut h,
        "schematic.field.move",
        json!({"unit":unit,"field":"value","at":{"x":"-250mil","y":0},"size":"500mil"}),
    );
    h.run();
    let at = screen(&h, 0.0, 0.0);
    h.drag_at(at);
    h.drop_at(at);
    h.run();
    assert!(
        h.state().shell().selection().objects.is_empty(),
        "field text wins over the smaller body"
    );
    assert_eq!(h.state().shell().selection().fields.len(), 1);
    assert_eq!(h.state().shell().selection().fields[0].field, "value");
    run(
        &mut h,
        "schematic.field.move",
        json!({"unit":unit,"field":"value","at":{"x":"-250mil","y":"120mil"},"size":"500mil"}),
    );
    run(&mut h, "selection.set", json!({}));
    h.run();
    let at = screen(&h, 0.0, 120.0);
    h.drag_at(at);
    h.drop_at(at);
    h.run();
    assert_eq!(
        h.state().shell().selection().objects,
        [unit.as_str().unwrap().parse::<Uuid>().unwrap()],
        "an exact Pin hit still ranks by size with text"
    );
}

/// The UUID of a Wire added along `points`.
fn wire(h: &mut Harness<'_, App>, points: Value) -> Uuid {
    run(
        h,
        "schematic.wire.add",
        json!({"document": SHEET, "points": points}),
    )["result"]["wires"][0]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn click_at(h: &mut Harness<'_, App>, at: Pos2) {
    h.drag_at(at);
    h.drop_at(at);
    h.run();
}

#[test]
fn shift_click_adds_to_the_selection_and_mod_click_replaces_it() {
    let dir = TempDir::new("canvas-shift-add");
    let mut h = harness(&dir);
    let unit: Uuid = resistor_and_wire(&mut h).as_str().unwrap().parse().unwrap();
    let wire = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .wires()
        .next()
        .unwrap()
        .uuid;
    let body = screen(&h, 0.0, 120.0);
    let on_wire = screen(&h, 0.0, 400.0);
    click_at(&mut h, body);
    assert_eq!(h.state().shell().selection().objects, [unit]);
    h.event(egui::Event::ModifiersChanged(Modifiers::SHIFT));
    click_at(&mut h, on_wire);
    assert_eq!(
        h.state().shell().selection().objects,
        [unit, wire],
        "Shift+click adds an object to the selection"
    );
    h.event(egui::Event::ModifiersChanged(Modifiers::COMMAND));
    click_at(&mut h, body);
    assert_eq!(
        h.state().shell().selection().objects,
        [unit],
        "Mod+click no longer toggles: it selects just the object, as a click does"
    );
}

#[test]
fn left_to_right_rectangle_encloses_and_right_to_left_touches() {
    let dir = TempDir::new("canvas-rectangle-direction");
    let mut h = harness(&dir);
    let wire = wire(&mut h, json!([{"x":"-100mil","y":0},{"x":"100mil","y":0}]));
    h.run();
    // A box over the Wire's right half only.
    let (left, right) = (screen(&h, 0.0, 100.0), screen(&h, 400.0, -100.0));
    let rectangle = |h: &mut Harness<'_, App>, from: Pos2, to: Pos2| {
        h.drag_at(from);
        h.run();
        h.hover_at(to);
        h.run();
        let mode = h
            .state()
            .shell()
            .document_tab(sheet())
            .unwrap()
            .overlay
            .rubber_band
            .map(|band| band.mode);
        h.drop_at(to);
        h.run();
        mode
    };
    let mode = rectangle(&mut h, left, right);
    assert_eq!(mode, Some(reda_ui::canvas::interact::BoxMode::Enclose));
    assert!(
        h.state().shell().selection().is_empty(),
        "left to right selects only what it encloses"
    );
    let mode = rectangle(&mut h, right, left);
    assert_eq!(mode, Some(reda_ui::canvas::interact::BoxMode::Touch));
    assert_eq!(
        h.state().shell().selection().objects,
        [wire],
        "right to left selects what it touches"
    );
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        assert_ne!(
            reda_ui::theme::selection_box(theme, true),
            reda_ui::theme::selection_box(theme, false),
            "each direction has its own colour"
        );
    }
}

#[test]
fn a_plain_click_after_a_shift_click_picks_the_top_of_the_stack() {
    let dir = TempDir::new("canvas-shift-then-click");
    let mut h = harness_with_step(&dir, 0.01);
    let short = wire(&mut h, json!([{"x":"-100mil","y":0},{"x":"100mil","y":0}]));
    wire(&mut h, json!([{"x":0,"y":"-500mil"},{"x":0,"y":"500mil"}]));
    let other = wire(
        &mut h,
        json!([{"x":"-500mil","y":"300mil"},{"x":"-300mil","y":"300mil"}]),
    );
    run(&mut h, "selection.set", json!({"objects":[other]}));
    h.run();
    let at = screen(&h, 0.0, 0.0);
    h.event(egui::Event::ModifiersChanged(Modifiers::SHIFT));
    click_at(&mut h, at);
    assert_eq!(h.state().shell().selection().objects, [other, short]);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    // Inside the 600 ms repeat window, but past a double-click.
    h.input_mut().time = Some(h.ctx.input(|i| i.time) + 0.45);
    click_at(&mut h, at);
    assert_eq!(
        h.state().shell().selection().objects,
        [short],
        "a click replaces the selection with the top object; a Shift+click doesn't start a cycle"
    );
}

#[test]
fn double_clicking_a_stack_keeps_the_selection_until_an_entry_is_chosen() {
    let dir = TempDir::new("canvas-pick-list-selection");
    let mut h = harness_with_step(&dir, 0.01);
    wire(&mut h, json!([{"x":"-100mil","y":0},{"x":"100mil","y":0}]));
    wire(&mut h, json!([{"x":0,"y":"-500mil"},{"x":0,"y":"500mil"}]));
    let other = wire(
        &mut h,
        json!([{"x":"-500mil","y":"300mil"},{"x":"-300mil","y":"300mil"}]),
    );
    run(&mut h, "selection.set", json!({"objects":[other]}));
    h.run();
    let at = screen(&h, 0.0, 0.0);
    for _ in 0..2 {
        click_at(&mut h, at);
    }
    h.get_by_label("Pick Object");
    assert_eq!(
        h.state().shell().selection().objects,
        [other],
        "the double-click's clicks don't change the selection the list opens over"
    );
}

#[test]
fn delete_with_objects_and_text_selected_deletes_the_objects() {
    let dir = TempDir::new("canvas-mixed-delete");
    let mut h = harness(&dir);
    let unit: Uuid = resistor_and_wire(&mut h).as_str().unwrap().parse().unwrap();
    let wire = wire(
        &mut h,
        json!([{"x":"-500mil","y":"-300mil"},{"x":"-300mil","y":"-300mil"}]),
    );
    run(
        &mut h,
        "selection.set",
        json!({"objects":[wire],"fields":[{"unit":unit,"field":"designator"}]}),
    );
    h.run();
    // Focus the sheet with a press on empty canvas, keeping the selection.
    run(
        &mut h,
        "app.preferences.set",
        json!({"key": "editing.click_clears_selection", "value": false}),
    );
    let empty = screen(&h, -900.0, -700.0);
    click_at(&mut h, empty);
    let before = h.state().shell().engine().journal().len();
    h.key_press(Key::Delete);
    h.run();
    assert_eq!(journal_since(&h, before), ["schematic.delete"]);
    let project = h.state().shell().engine().project().unwrap();
    let sheet = project.schematic_sheet(sheet()).unwrap();
    assert!(
        project.object(wire).is_none(),
        "the selected Wire is deleted"
    );
    assert!(
        sheet.unit(unit).is_some(),
        "the text, which can't be deleted alone, stays with its Component"
    );
}

#[test]
fn only_deletes_of_the_selection_are_refused_for_text_alone() {
    use reda_ui::bars::{Action, Binding};
    let dir = TempDir::new("canvas-delete-scope");
    let mut h = harness(&dir);
    let unit = resistor_and_wire(&mut h);
    run(
        &mut h,
        "selection.set",
        json!({"fields":[{"unit":unit,"field":"designator"}]}),
    );
    let action = |command: &str, params: Value| Action {
        id: command.into(),
        binding: Binding::Raw {
            command: command.into(),
            params,
        },
        title: None,
        icon: None,
    };
    let shell = h.state().shell();
    let of_selection = reda_ui::bars::resolve_for(
        shell,
        &action("schematic.delete", json!({"objects":"$selection"})),
        Some(sheet()),
    );
    assert_eq!(of_selection.command, "selection.delete");
    let unrelated = reda_ui::bars::resolve_for(
        shell,
        &action("output.job.delete", json!({"job": unit})),
        Some(sheet()),
    );
    assert_eq!(
        unrelated.command, "output.job.delete",
        "a delete that doesn't act on the selection is left alone"
    );
}

/// The snap palette's working state for the sheet.
fn snap_options(h: &mut Harness<'_, App>) -> Value {
    run(h, "view.snap.set", json!({"document": SHEET}))
}

#[test]
fn mod_e_opens_the_snap_palette_in_flight_and_the_active_bar_button_opens_it_too() {
    let dir = TempDir::new("canvas-snap-palette");
    let mut h = harness(&dir);
    resistor_and_wire(&mut h);
    // A Wire run in flight.
    h.hover_at(screen(&h, 1000.0, 1000.0));
    h.run();
    h.key_press(Key::W);
    h.run();
    let start = screen(&h, 1000.0, 1000.0);
    h.drag_at(start);
    h.drop_at(start);
    h.run();
    assert!(h.query_by_label("Hotspots").is_none());
    h.key_press_modifiers(Modifiers::COMMAND, Key::E);
    h.run();
    assert!(
        h.query_by_label("Hotspots").is_some(),
        "Mod+E opens the palette"
    );
    assert_eq!(
        h.state().shell().tool(),
        reda_ui::tools::Tool::Wire,
        "the Tool stays in flight"
    );
    // The palette holds the grid toggle, the mode and the Hotspot checklist.
    h.get_by_label("Pin end").click();
    h.run();
    assert_eq!(snap_options(&mut h)["hotspots_off"], json!(["pin_end"]));
    h.get_by_label("Snap to grid").click();
    h.run();
    assert_eq!(snap_options(&mut h)["gridless"], json!(true));
    h.get_by_label("Off").click();
    h.run();
    assert_eq!(snap_options(&mut h)["mode"], json!("off"));
    // Mod+E again closes it; the Active Bar button opens the same palette.
    h.key_press_modifiers(Modifiers::COMMAND, Key::E);
    h.run();
    assert!(h.query_by_label("Hotspots").is_none());
    h.get_by_label("Snap Options").click();
    h.run();
    assert!(h.query_by_label("Hotspots").is_some());
}
