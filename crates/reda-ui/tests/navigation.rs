//! Navigation (docs/spec/v0.1/editing.md#navigation): Home re-centres on the cursor,
//! Shift+Home zooms to the selection, Alt+← goes back to the previous view,
//! `view.selection.next` steps through the selection, and auto-pan pans the canvas when a
//! drag reaches its edge.

use std::path::{Path, PathBuf};

use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::bars::{self, Request};
use reda_ui::canvas::Camera;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::shell::Shell;
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn shell(dir: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    shell
        .run("document.open", json!({"document": SHEET}))
        .unwrap();
    shell
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

fn wire(shell: &mut Shell, document: Uuid, from: (i64, i64), to: (i64, i64)) -> Uuid {
    let at = |(x, y): (i64, i64)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")});
    let out = shell
        .run(
            "schematic.wire.add",
            json!({"document": document, "points": [at(from), at(to)]}),
        )
        .unwrap();
    out["result"]["wires"][0].as_str().unwrap().parse().unwrap()
}

fn camera(center: (f64, f64)) -> Camera {
    Camera {
        center,
        nm_per_point: 100_000.0,
    }
}

#[test]
fn home_recentres_on_the_cursor_and_alt_left_goes_back() {
    let dir = TempDir::new("nav-recenter");
    let mut shell = shell(&dir);
    let tab = shell.document_tab_mut(sheet()).unwrap();
    tab.camera = Some(camera((0.0, 0.0)));
    tab.cursor = Some((40e6, 30e6));
    let out = shell.run("view.recenter", json!({})).unwrap();
    assert_eq!(out["document"], json!(SHEET));
    assert_eq!(
        bars::take_requests(shell.engine()),
        [Request::Recenter(sheet())]
    );
    shell.recenter(sheet());
    let tab = shell.document_tab(sheet()).unwrap();
    let moved = tab.camera.unwrap();
    assert_eq!(moved.center, (40e6, 30e6));
    assert_eq!(moved.nm_per_point, 100_000.0, "the zoom stays");

    // A Go To jump is remembered too; Alt+← walks back through both.
    shell
        .run("view.jump_to", json!({"x": "10mm", "y": "0mm"}))
        .unwrap();
    shell.run("view.previous", json!({})).unwrap();
    let requests = bars::take_requests(shell.engine());
    assert_eq!(requests.last(), Some(&Request::PreviousView(sheet())));
    for request in requests {
        if let Request::JumpTo(d, jump) = request {
            shell.jump(d, jump);
        }
    }
    assert_eq!(
        shell.document_tab(sheet()).unwrap().camera.unwrap().center,
        (10e6, 0.0)
    );
    assert!(shell.previous_view(sheet()));
    assert_eq!(
        shell.document_tab(sheet()).unwrap().camera.unwrap().center,
        (40e6, 30e6)
    );
    assert!(shell.previous_view(sheet()));
    assert_eq!(
        shell.document_tab(sheet()).unwrap().camera.unwrap().center,
        (0.0, 0.0)
    );
    assert!(!shell.previous_view(sheet()), "nothing earlier");
}

#[test]
fn view_selection_next_steps_through_the_selection_zooming_to_each() {
    let dir = TempDir::new("nav-next");
    let mut shell = shell(&dir);
    let a = wire(&mut shell, sheet(), (0, 0), (10, 0));
    let b = wire(&mut shell, sheet(), (50, 0), (60, 0));
    let added = shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    let second: Uuid = added["result"]["uuid"].as_str().unwrap().parse().unwrap();
    let c = wire(&mut shell, second, (0, 0), (10, 0));
    let err = shell.run("view.selection.next", json!({})).unwrap_err();
    assert_eq!(err.message, "nothing is selected");

    shell
        .run("selection.set", json!({"objects": [b, a, c]}))
        .unwrap();
    bars::take_requests(shell.engine());
    let mut stepped = Vec::new();
    for _ in 0..4 {
        let out = shell.run("view.selection.next", json!({})).unwrap();
        stepped.push((out["object"].clone(), out["index"].clone()));
        assert_eq!(out["count"], 3);
    }
    // In pick order, starting over after the last.
    assert_eq!(
        stepped,
        [
            (json!(b), json!(0)),
            (json!(a), json!(1)),
            (json!(c), json!(2)),
            (json!(b), json!(0)),
        ]
    );
    let requests = bars::take_requests(shell.engine());
    assert_eq!(requests[0], Request::ZoomTo(sheet(), vec![b]));
    assert_eq!(requests[2], Request::ZoomTo(second, vec![c]));
    // Its sheet was opened for C, and a new selection starts at its first object.
    assert!(shell.document_tab(second).is_some());
    shell.run("selection.set", json!({"objects": [c]})).unwrap();
    let out = shell.run("view.selection.next", json!({})).unwrap();
    assert_eq!(
        (out["object"].clone(), out["index"].clone()),
        (json!(c), json!(0))
    );
}

fn harness(dir: &TempDir) -> Harness<'static, App> {
    let shell = shell(dir);
    Harness::builder()
        .with_size([1200.0, 800.0])
        .with_step_dt(0.05)
        .with_max_steps(100)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

fn center(h: &Harness<'_, App>) -> (f64, f64) {
    h.state()
        .shell()
        .document_tab(sheet())
        .unwrap()
        .camera
        .unwrap()
        .center
}

fn canvas_rect(h: &Harness<'_, App>) -> egui::Rect {
    h.get_by_role(egui::accesskit::Role::Canvas).rect()
}

#[test]
fn the_keys_recentre_go_back_and_zoom_to_the_selection_in_the_window() {
    let dir = TempDir::new("nav-keys");
    let mut h = harness(&dir);
    let w = wire(h.state_mut().shell_mut(), sheet(), (100, 100), (110, 100));
    h.run();
    let fitted = center(&h);
    // Home re-centres on the cursor.
    let rect = canvas_rect(&h);
    let spot = rect.center() + egui::vec2(150.0, 80.0);
    h.hover_at(spot);
    h.run();
    h.key_press(Key::Home);
    h.run();
    let moved = center(&h);
    assert!(
        moved.0 > fitted.0 && moved.1 < fitted.1,
        "{fitted:?} -> {moved:?}"
    );
    // Alt+← comes back.
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowLeft);
    h.run();
    assert_eq!(center(&h), fitted);
    // Shift+Home zooms to the selection.
    h.state_mut()
        .shell_mut()
        .run("selection.set", json!({"objects": [w]}))
        .unwrap();
    h.key_press_modifiers(Modifiers::SHIFT, Key::Home);
    h.run();
    let zoomed = h
        .state()
        .shell()
        .document_tab(sheet())
        .unwrap()
        .camera
        .unwrap();
    let wire_mid = (105e6, 100e6);
    assert!(
        (zoomed.center.0 - wire_mid.0).abs() < 1e6 && (zoomed.center.1 - wire_mid.1).abs() < 1e6,
        "{zoomed:?}"
    );
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowLeft);
    h.run();
    assert_eq!(center(&h), fitted);
}

/// Drags a selection box from the middle of the canvas to `to`, holding it there for a
/// few frames; returns how far the view's centre moved along x.
fn drag_to(h: &mut Harness<'_, App>, to: Pos2) -> f64 {
    let rect = canvas_rect(h);
    let before = center(h);
    let start = rect.center();
    h.drag_at(start);
    h.run();
    h.hover_at(start + egui::vec2(30.0, 0.0));
    h.run();
    h.hover_at(to);
    for _ in 0..5 {
        h.step();
    }
    h.drop_at(to);
    h.run();
    center(h).0 - before.0
}

#[test]
fn a_drag_past_the_canvas_edge_auto_pans_unless_it_is_off() {
    let dir = TempDir::new("nav-auto-pan");
    let mut h = harness(&dir);
    h.run();
    let rect = canvas_rect(&h);
    let past = egui::pos2(rect.right() + 10.0, rect.center().y);
    let panned = drag_to(&mut h, past);
    assert!(panned > 0.0, "the view moves right: {panned}");
    h.state_mut()
        .shell_mut()
        .run(
            "app.preferences.set",
            json!({"key": "editing.auto_pan.style", "value": "off"}),
        )
        .unwrap();
    h.run();
    assert_eq!(drag_to(&mut h, past), 0.0);
    // Hovering near the edge with nothing in flight never pans.
    h.state_mut()
        .shell_mut()
        .run(
            "app.preferences.set",
            json!({"key": "editing.auto_pan.style", "value": "fixed_size_jump"}),
        )
        .unwrap();
    let before = center(&h);
    h.hover_at(egui::pos2(rect.right() - 2.0, rect.center().y));
    for _ in 0..5 {
        h.step();
    }
    assert_eq!(center(&h), before);
}
