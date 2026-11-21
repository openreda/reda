//! Plated Edges in the board editor (docs/spec/v0.2.5/plated-edges.md#ui): clicking an
//! edge of the Board Outline being edited selects it, the polygon editor carries the
//! marks through its edits, and the Properties panel marks the edge and sets its Net.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::selection::ContourEdge;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const TOLERANCE: Nm = Nm::from_um(200);

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

fn run(shell: &mut Shell, id: &str, params: Value) -> Value {
    match shell.run(id, params.clone()) {
        Ok(out) => out,
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

/// A Shell with a new Project's board open on a 0.5 mm grid, holding a selected 10 × 10 mm
/// Board Outline from the origin whose bottom edge (vertex 0 → 1) is plated.
fn shell(home: &TempDir, work: &TempDir) -> (Shell, Uuid) {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    run(
        &mut shell,
        "view.grid.set",
        json!({"document": pcb, "grid": "0.5mm"}),
    );
    let out = run(
        &mut shell,
        "pcb.board_outline.add",
        json!({"vertices": [
            {"x": "0mm", "y": "0mm", "plated": {}}, {"x": "10mm", "y": "0mm"},
            {"x": "10mm", "y": "10mm"}, {"x": "0mm", "y": "10mm"}]}),
    );
    let outline: Uuid = out["result"]["uuid"].as_str().unwrap().parse().unwrap();
    run(&mut shell, "selection.set", json!({"objects": [outline]}));
    (shell, outline)
}

fn marks(shell: &Shell) -> Vec<(usize, Option<Uuid>)> {
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    pcb.outline()
        .unwrap()
        .plated
        .iter()
        .map(|(i, m)| (*i, m.net))
        .collect()
}

fn vertex_count(shell: &Shell) -> usize {
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    pcb.outline().unwrap().vertices.len()
}

fn click(shell: &mut Shell, at: Point) {
    shell.pcb_hover(at);
    shell.pcb_press(at, false, TOLERANCE);
    shell.pcb_release();
}

fn drag(shell: &mut Shell, from: Point, to: Point) {
    shell.pcb_hover(from);
    shell.pcb_press(from, false, TOLERANCE);
    shell.pcb_drag(to);
    shell.pcb_release();
}

#[test]
fn clicking_an_edge_of_the_edited_outline_selects_that_edge() {
    let (home, work) = (TempDir::new("plated-ui-home"), TempDir::new("plated-ui"));
    let (mut shell, outline) = shell(&home, &work);
    click(&mut shell, mm(10.0, 3.0));
    let selection = shell.selection();
    assert_eq!(selection.objects, [outline], "the outline stays selected");
    assert_eq!(
        selection.edges,
        [ContourEdge {
            contour: outline,
            edge: 1
        }]
    );
    assert!(
        reda_ui::pcb::polygon_edit::selected(&shell).is_some(),
        "its handles still show"
    );
}

#[test]
fn editing_the_outline_carries_its_plated_edges_as_the_edit_table_says() {
    let (home, work) = (
        TempDir::new("plated-edit-home"),
        TempDir::new("plated-edit"),
    );
    let (mut shell, outline) = shell(&home, &work);
    // Inserting a vertex on the plated edge: both halves keep the mark.
    shell.pcb_hover(mm(3.0, 0.0));
    shell.pcb_press(mm(3.0, 0.0), true, TOLERANCE);
    shell.pcb_release();
    assert_eq!(vertex_count(&shell), 5);
    assert_eq!(marks(&shell), [(0, None), (1, None)]);
    // Moving a vertex keeps every mark.
    drag(&mut shell, mm(10.0, 10.0), mm(12.0, 11.0));
    assert_eq!(marks(&shell), [(0, None), (1, None)]);
    // Deleting the inserted vertex joins the halves: the edge before it was plated.
    shell.pcb_hover(mm(3.0, 0.0));
    shell.pcb_press(mm(3.0, 0.0), false, TOLERANCE);
    shell.pcb_drag(mm(3.5, 0.5));
    run(
        &mut shell,
        "pcb.copper.delete",
        json!({"objects": [outline]}),
    );
    shell.pcb_release();
    assert_eq!(vertex_count(&shell), 4);
    assert_eq!(marks(&shell), [(0, None)]);
    // Splitting the edge from its midpoint handle (Space once) keeps it on both halves.
    shell.pcb_hover(mm(5.0, 0.0));
    shell.pcb_press(mm(5.0, 0.0), false, TOLERANCE);
    shell.pcb_drag(mm(5.0, -2.0));
    run(&mut shell, "tool.rotate", json!({}));
    shell.pcb_release();
    assert_eq!(vertex_count(&shell), 5);
    assert_eq!(marks(&shell), [(0, None), (1, None)]);
}

fn harness(shell: Shell) -> Harness<'static, App> {
    Harness::builder()
        .with_size([1200.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

fn last_entry(h: &Harness<'_, App>) -> (String, Value) {
    let entry = (h.state().shell().engine().journal().iter())
        .rfind(|e| e.outcome.is_ok())
        .unwrap();
    (entry.id.clone(), entry.params.clone())
}

#[test]
fn an_edges_properties_mark_it_plated_and_pick_its_net() {
    let (home, work) = (
        TempDir::new("plated-props-home"),
        TempDir::new("plated-props"),
    );
    let (mut shell, outline) = shell(&home, &work);
    let gnd = run(&mut shell, "test.net.add", json!({"name": "GND"}))["result"]["uuid"].clone();
    run(&mut shell, "selection.set", json!({"objects": [outline]}));
    run(
        &mut shell,
        "selection.set",
        json!({"edges": [{"contour": outline, "edge": 1}]}),
    );
    run(&mut shell, "panel.show:properties", json!({}));
    let mut h = harness(shell);
    h.run();
    assert!(
        h.query_by_role_and_label(Role::ComboBox, "Net").is_none(),
        "an unplated edge has no Net picker"
    );
    h.get_by_role_and_label(Role::CheckBox, "Plated")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Plated").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "pcb.contour.edge.set");
    assert_eq!(params["edge"], 1);
    assert_eq!(marks(h.state().shell()), [(0, None), (1, None)]);

    // Now plated, the edge shows its Net picker.
    h.get_by_role_and_label(Role::ComboBox, "Net").click();
    h.run();
    h.get_by_label("GND").click();
    h.run();
    let gnd: Uuid = gnd.as_str().unwrap().parse().unwrap();
    assert_eq!(marks(h.state().shell()), [(0, None), (1, Some(gnd))]);

    h.get_by_role_and_label(Role::Button, "Unplate all").click();
    h.run();
    assert_eq!(last_entry(&h).0, "pcb.contour.edge.set_all");
    assert!(marks(h.state().shell()).is_empty());
    h.get_by_role_and_label(Role::Button, "Plate all edges")
        .click();
    h.run();
    assert_eq!(marks(h.state().shell()).len(), 4);
}
