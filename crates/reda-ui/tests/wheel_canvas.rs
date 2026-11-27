//! The wheel mapping on the editors' canvases (docs/spec/v0.3/input-and-defaults.md#wheel-mapping):
//! every canvas follows the one `canvas.wheel.*` table, a touchpad scroll pans under
//! Touchpad Auto, and Step Layer steps the copper Layer in the PCB editor when bound.

use egui::accesskit::Role;
use egui::{Event, Modifiers, MouseWheelSource, MouseWheelUnit, TouchPhase, Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::canvas::Camera;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{DocumentKind, LayerKind, Side, Uuid};
use reda_ui::engine::preferences;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

struct Window {
    h: Harness<'static, App>,
    pcb: Uuid,
    sheet: Uuid,
    _home: TempDir,
    _work: TempDir,
}

/// The window on a new Project's board (Top and Bottom Copper), its tab shown, with the
/// Preferences `prefs` set first.
fn window(prefs: &[(&str, Value)]) -> Window {
    let home = TempDir::new("wheel-canvas-home");
    let work = TempDir::new("wheel-canvas-work");
    let dirs = UserDirs::under(home.path());
    for (key, value) in prefs {
        preferences::set(&dirs, key, value.clone()).unwrap();
    }
    let engine = Engine::builder()
        .user_dirs(Some(dirs))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let project = shell.engine().project().unwrap();
    let pcb = project.pcb().unwrap().uuid;
    let sheet = project
        .documents()
        .find(|d| d.kind == DocumentKind::Schematic)
        .unwrap()
        .uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": pcb.to_string()}))
        .unwrap();
    let mut h = Harness::builder()
        .with_size([1000.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    Window {
        h,
        pcb,
        sheet,
        _home: home,
        _work: work,
    }
}

fn camera(w: &Window, document: Uuid) -> Camera {
    w.h.state()
        .shell()
        .document_tab(document)
        .unwrap()
        .camera
        .unwrap()
}

/// One wheel event over the middle of the canvas.
fn scroll(
    w: &mut Window,
    unit: MouseWheelUnit,
    delta: Vec2,
    source: MouseWheelSource,
    held: Modifiers,
) {
    let at = w.h.get_by_role(Role::Canvas).rect().center();
    w.h.event(Event::PointerMoved(at));
    w.h.run();
    w.h.event(Event::MouseWheel {
        unit,
        delta,
        phase: TouchPhase::Move,
        source,
        modifiers: held,
    });
    w.h.run();
}

fn notch(w: &mut Window, lines: f32, held: Modifiers) {
    scroll(
        w,
        MouseWheelUnit::Line,
        vec2(0.0, lines),
        MouseWheelSource::Wheel,
        held,
    );
}

fn copper(w: &Window, side: Side) -> Uuid {
    let project = w.h.state().shell().engine().project().unwrap();
    let pcb = project.pcb().unwrap();
    pcb.layers()
        .find(LayerKind::Copper, Some(side))
        .unwrap()
        .uuid
}

fn activate(w: &mut Window, layer: Uuid) {
    w.h.state_mut()
        .shell_mut()
        .run("pcb.layer.activate", json!({"layer": layer}))
        .unwrap();
    w.h.run();
    assert_eq!(w.h.state().shell().board_view().layers.active, Some(layer));
}

fn journal_ids(w: &Window) -> Vec<String> {
    let journal = w.h.state().shell().engine().journal();
    journal.iter().map(|e| e.id.clone()).collect()
}

#[test]
fn the_plain_wheel_zooms_and_shift_and_ctrl_scroll_on_the_reda_table() {
    let mut w = window(&[]);
    let before = camera(&w, w.pcb);
    notch(&mut w, 1.0, Modifiers::NONE);
    let zoomed = camera(&w, w.pcb);
    assert!(
        zoomed.nm_per_point < before.nm_per_point,
        "wheel up zooms in"
    );

    notch(&mut w, -1.0, Modifiers::SHIFT);
    let sideways = camera(&w, w.pcb);
    assert_eq!(sideways.nm_per_point, zoomed.nm_per_point);
    // The wheel turned down moves the content left: the view goes right.
    assert!(sideways.center.0 > zoomed.center.0);
    assert_eq!(sideways.center.1, zoomed.center.1);

    notch(&mut w, 1.0, Modifiers::CTRL);
    let up = camera(&w, w.pcb);
    assert_eq!(up.nm_per_point, zoomed.nm_per_point);
    assert_eq!(up.center.0, sideways.center.0);
    // The wheel turned up moves the content down: the view goes up (y up).
    assert!(up.center.1 > sideways.center.1);
}

#[test]
fn the_wheel_scrolls_table_scrolls_on_the_plain_wheel_and_zooms_with_ctrl_on_every_canvas() {
    let table = preferences::WheelPreset::WheelScrolls.table();
    let prefs: Vec<(&str, Value)> = vec![
        ("canvas.wheel.zoom", json!("unbound")),
        ("canvas.wheel.scroll_y", json!(table.scroll_y.to_string())),
        ("canvas.wheel.zoom", json!(table.zoom.to_string())),
    ];
    let mut w = window(&prefs);
    for document in [w.pcb, w.sheet] {
        w.h.state_mut()
            .shell_mut()
            .run("document.open", json!({"document": document}))
            .unwrap();
        w.h.state_mut()
            .shell_mut()
            .run("layout.tab.activate", json!({"tab": document.to_string()}))
            .unwrap();
        w.h.run();
        let before = camera(&w, document);
        notch(&mut w, 1.0, Modifiers::NONE);
        let scrolled = camera(&w, document);
        assert_eq!(scrolled.nm_per_point, before.nm_per_point, "{document}");
        assert!(scrolled.center.1 > before.center.1, "{document}");
        notch(&mut w, 1.0, Modifiers::CTRL);
        assert!(
            camera(&w, document).nm_per_point < scrolled.nm_per_point,
            "{document}"
        );
    }
}

#[test]
fn touchpad_auto_pans_for_a_touchpad_scroll_and_a_pinch_zooms() {
    let mut w = window(&[]);
    let before = camera(&w, w.pcb);
    scroll(
        &mut w,
        MouseWheelUnit::Point,
        vec2(-20.0, 10.0),
        MouseWheelSource::Trackpad,
        Modifiers::NONE,
    );
    let panned = camera(&w, w.pcb);
    assert_eq!(panned.nm_per_point, before.nm_per_point);
    let moved = (
        (panned.center.0 - before.center.0) / before.nm_per_point,
        (panned.center.1 - before.center.1) / before.nm_per_point,
    );
    // The content follows the fingers: 20 points left and 10 down.
    assert!(
        (moved.0 - 20.0).abs() < 1e-6 && (moved.1 - 10.0).abs() < 1e-6,
        "{moved:?}"
    );

    let at = w.h.get_by_role(Role::Canvas).rect().center();
    w.h.event(Event::PointerMoved(at));
    w.h.event(Event::Zoom(2.0));
    w.h.run();
    let pinched = camera(&w, w.pcb);
    assert!((pinched.nm_per_point - panned.nm_per_point / 2.0).abs() < 1e-6);
}

#[test]
fn step_layer_steps_the_copper_layer_in_the_pcb_editor_when_bound() {
    let mut w = window(&[("canvas.wheel.step_layer", json!("alt"))]);
    let (top, bottom) = (copper(&w, Side::Top), copper(&w, Side::Bottom));
    activate(&mut w, top);
    let zoom = camera(&w, w.pcb).nm_per_point;
    notch(&mut w, -1.0, Modifiers::ALT);
    w.h.run();
    assert_eq!(
        journal_ids(&w).last().map(String::as_str),
        Some("pcb.layer.step")
    );
    assert_eq!(w.h.state().shell().board_view().layers.active, Some(bottom));
    assert_eq!(
        camera(&w, w.pcb).nm_per_point,
        zoom,
        "the wheel did not zoom"
    );
    notch(&mut w, 1.0, Modifiers::ALT);
    w.h.run();
    assert_eq!(w.h.state().shell().board_view().layers.active, Some(top));
}

#[test]
fn unbound_step_layer_leaves_the_layer_alone() {
    let mut w = window(&[]);
    let top = copper(&w, Side::Top);
    activate(&mut w, top);
    let steps = journal_ids(&w).len();
    notch(&mut w, -1.0, Modifiers::ALT);
    w.h.run();
    assert_eq!(journal_ids(&w).len(), steps);
    assert_eq!(w.h.state().shell().board_view().layers.active, Some(top));
}
