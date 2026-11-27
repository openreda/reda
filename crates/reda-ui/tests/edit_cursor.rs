//! The Edit Cursor (docs/spec/v0.3/input-and-defaults.md#the-edit-cursor): the arrows
//! step it by grid steps, from off the grid onto the next line, ignoring Hotspots; Enter
//! clicks at it where the Tool has not bound Enter and Shift+Enter everywhere; the system
//! pointer follows it where the desktop allows; Auto-pan follows it out of view; and a
//! focused text field keeps the arrows.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::edit_cursor;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::preferences_dialog::PageId;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

/// A Shell on a board with two 2 mm Pads on Net SIG at (0, 0) and (10, 0) mm and a
/// Footprint with no Pads whose origin, a Hotspot, is at (2.1, 0.1) mm; its tab open on a
/// 1 mm grid.
fn board(home: &Path, work: &Path) -> (Shell, Uuid) {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, "Board").unwrap();
    let net = shell.run("test.net.add", json!({"name": "SIG"})).unwrap()["result"]["uuid"].clone();
    for x in ["0mm", "10mm"] {
        shell
            .run(
                "test.footprint.add",
                json!({"at": {"x": x, "y": 0}, "pads": [
                    {"at": {"x": 0, "y": 0}, "width": "2mm", "height": "2mm", "net": net}
                ]}),
            )
            .unwrap();
    }
    shell
        .run(
            "test.footprint.add",
            json!({"at": {"x": "2.1mm", "y": "0.1mm"}, "pads": []}),
        )
        .unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": pcb.to_string()}))
        .unwrap();
    shell
        .run("view.grid.set", json!({"document": pcb, "grid": "1mm"}))
        .unwrap();
    (shell, pcb)
}

struct Window {
    h: Harness<'static, App>,
    pcb: Uuid,
    _home: TempDir,
    _work: TempDir,
}

fn window_with(app: impl FnOnce(App) -> App) -> Window {
    let home = TempDir::new("edit-cursor-home");
    let work = TempDir::new("edit-cursor-work");
    let (shell, pcb) = board(home.path(), work.path());
    let mut h = Harness::builder()
        .with_size([1000.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app(App::new(shell)));
    h.run();
    Window {
        h,
        pcb,
        _home: home,
        _work: work,
    }
}

fn window() -> Window {
    window_with(|app| app)
}

fn shell(w: &Window) -> &Shell {
    w.h.state().shell()
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

fn at(p: &Value) -> Point {
    Point::new(
        Nm(p["at"]["x"].as_i64().unwrap()),
        Nm(p["at"]["y"].as_i64().unwrap()),
    )
}

fn canvas_rect(w: &Window) -> egui::Rect {
    w.h.get_by_role(Role::Canvas).rect()
}

fn on_screen(w: &Window, p: Point) -> Pos2 {
    let camera = shell(w).document_tab(w.pcb).unwrap().camera.unwrap();
    camera.screen_point(canvas_rect(w), (p.x.0 as f64, p.y.0 as f64))
}

fn journal_from(w: &Window, from: usize) -> Vec<(String, Value)> {
    shell(w).engine().journal()[from..]
        .iter()
        .map(|e| (e.id.clone(), e.params.clone()))
        .collect()
}

fn keyed(w: &Window) -> Option<Point> {
    edit_cursor::of(shell(w), w.pcb).and_then(|c| c.keyed)
}

fn vias(w: &Window) -> Vec<Point> {
    let pcb = shell(w).engine().project().unwrap().pcb().unwrap();
    pcb.vias().map(|v| v.at).collect()
}

fn press(w: &mut Window, modifiers: Modifiers, key: Key) {
    w.h.key_press_modifiers(modifiers, key);
    w.h.run();
}

#[test]
fn a_step_moves_by_whole_grid_steps_and_from_off_the_grid_lands_on_the_next_line() {
    let home = TempDir::new("edit-cursor-steps-home");
    let work = TempDir::new("edit-cursor-steps-work");
    let (mut shell, pcb) = board(home.path(), work.path());
    let step = |shell: &mut Shell, dx: i64, dy: i64| {
        at(&shell
            .run("view.cursor.step", json!({"dx": dx, "dy": dy}))
            .unwrap())
    };
    // The canvas has not been shown: the keys start at the grid's origin.
    assert_eq!(step(&mut shell, 2, -1), mm(2.0, -1.0));
    assert_eq!(step(&mut shell, 10, 0), mm(12.0, -1.0));
    // On 25 mil (0.635 mm) the Edit Cursor is off the grid: the first press lands on the
    // next line to the right, 2.54 mm being four steps from 1.905, and the axis it does
    // not move goes onto its nearest line, -1.27 mm.
    shell
        .run("view.grid.set", json!({"document": pcb, "grid": "25mil"}))
        .unwrap();
    assert_eq!(step(&mut shell, -1, 0), mm(11.43, -1.27));
    assert_eq!(step(&mut shell, 1, 0), mm(12.065, -1.27));
    // Seen from below, screen right is towards smaller X.
    shell
        .run("pcb.view.from_bottom", json!({"from_bottom": true}))
        .unwrap();
    assert_eq!(step(&mut shell, 1, 1), mm(11.43, -0.635));
    // A click acts at the Edit Cursor, on the board by default.
    let out = shell.run("view.cursor.click", json!({})).unwrap();
    assert_eq!(out["document"], json!(pcb));
    assert_eq!(at(&out), mm(11.43, -0.635));
}

#[test]
fn arrows_step_the_edit_cursor_and_mod_arrows_nudge_the_selection() {
    let mut w = window();
    let footprint = {
        let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
        (pcb.footprints().find(|f| f.at == mm(2.1, 0.1)).unwrap()).uuid
    };
    shell_run(&mut w, "selection.set", json!({"objects": [footprint]}));
    let from = shell(&w).engine().journal().len();
    press(&mut w, Modifiers::NONE, Key::ArrowRight);
    press(&mut w, Modifiers::COMMAND, Key::ArrowRight);
    press(&mut w, Modifiers::COMMAND | Modifiers::SHIFT, Key::ArrowUp);
    let ran: Vec<(String, Value)> = journal_from(&w, from)
        .into_iter()
        .filter(|(id, _)| id.starts_with("view.cursor") || id.starts_with("pcb."))
        .collect();
    assert_eq!(
        ran,
        [
            (
                "view.cursor.step".into(),
                json!({"document": w.pcb, "dx": 1, "dy": 0})
            ),
            ("pcb.nudge".into(), json!({"dx": 1, "dy": 0})),
            (
                "pcb.move".into(),
                json!({"objects": [footprint], "by": {"x": 900_000, "y": 0}})
            ),
            ("pcb.nudge".into(), json!({"dx": 0, "dy": 10})),
            (
                "pcb.move".into(),
                json!({"objects": [footprint], "by": {"x": 0, "y": 9_900_000}})
            ),
        ]
    );
    let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.footprint(footprint).unwrap().at, mm(3.0, 10.0));
}

#[test]
fn arrows_and_enter_place_a_via_on_the_grid_point_a_hotspot_is_beside() {
    let mut w = window();
    shell_run(&mut w, "tool.activate:via", json!({}));
    w.h.hover_at(on_screen(&w, mm(0.0, 0.0)));
    w.h.run();
    let start = shell(&w).engine().journal().len();
    press(&mut w, Modifiers::NONE, Key::ArrowRight);
    press(&mut w, Modifiers::NONE, Key::ArrowRight);
    assert_eq!(keyed(&w), Some(mm(2.0, 0.0)));
    // The Footprint origin at (2.1, 0.1) is within the Snap Distance, but the keys are
    // for an exact count of steps.
    press(&mut w, Modifiers::NONE, Key::Enter);
    let ran = journal_from(&w, start);
    let ids: Vec<&str> = ran.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "view.cursor.step",
            "view.cursor.step",
            "view.cursor.click",
            "pcb.via.add"
        ],
        "{ran:?}"
    );
    assert_eq!(ran[0].1["dx"], json!(1));
    assert_eq!(ran[0].1["dy"], json!(0));
    assert_eq!(vias(&w), [mm(2.0, 0.0)]);
}

#[test]
fn a_mouse_click_on_the_same_point_snaps_to_the_hotspot() {
    // The case the keys leave out: the pointer over (2, 0) is caught by the origin.
    let mut w = window();
    shell_run(&mut w, "tool.activate:via", json!({}));
    let spot = on_screen(&w, mm(2.0, 0.0));
    w.h.hover_at(spot);
    w.h.run();
    w.h.drag_at(spot);
    w.h.drop_at(spot);
    w.h.run();
    assert_eq!(vias(&w), [mm(2.1, 0.1)]);
}

fn shell_run(w: &mut Window, id: &str, params: Value) {
    w.h.state_mut().shell_mut().run(id, params).unwrap();
    w.h.run();
}

#[test]
fn enter_ends_a_route_and_shift_enter_places_a_route_corner() {
    let mut w = window();
    shell_run(&mut w, "tool.activate:route", json!({}));
    let a = on_screen(&w, mm(0.0, 0.0));
    w.h.hover_at(a);
    w.h.run();
    w.h.drag_at(a);
    w.h.drop_at(a);
    w.h.run();
    let start = shell(&w).engine().journal().len();
    for _ in 0..3 {
        press(&mut w, Modifiers::NONE, Key::ArrowDown);
    }
    // Shift+Enter clicks: a corner at (0, -3).
    press(&mut w, Modifiers::SHIFT, Key::Enter);
    press(&mut w, Modifiers::SHIFT, Key::ArrowRight);
    assert_eq!(keyed(&w), Some(mm(10.0, -3.0)));
    // Enter keeps its finishing meaning in the route Tool.
    press(&mut w, Modifiers::NONE, Key::Enter);
    let ids: Vec<String> = journal_from(&w, start)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(
        ids.iter().filter(|id| *id == "view.cursor.click").count(),
        1,
        "{ids:?}"
    );
    assert!(ids.iter().any(|id| id == "pcb.route.finish"), "{ids:?}");
    let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
    let ends: Vec<(Point, Point)> = pcb.tracks().map(|t| t.geometry.ends()).collect();
    assert!(
        ends.contains(&(mm(0.0, 0.0), mm(0.0, -3.0))),
        "a Track to the corner placed by keys: {ends:?}"
    );
}

#[test]
fn enter_clicks_at_the_edit_cursor_in_a_tool_that_does_not_bind_it() {
    let mut w = window();
    w.h.hover_at(on_screen(&w, mm(0.0, 0.0)));
    w.h.run();
    let start = shell(&w).engine().journal().len();
    press(&mut w, Modifiers::NONE, Key::Enter);
    let ids: Vec<String> = journal_from(&w, start)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(
        ids.first().map(String::as_str),
        Some("view.cursor.click"),
        "{ids:?}"
    );
    // A click on the Pad there selects it, as the mouse would.
    let selection = shell(&w).selection();
    assert!(
        !selection.objects.is_empty() || !selection.pads.is_empty(),
        "{ids:?} {selection:?}"
    );
}

#[test]
fn the_system_pointer_follows_and_the_mouse_takes_the_edit_cursor_back() {
    let mut w = window();
    let origin = on_screen(&w, mm(0.0, 0.0));
    w.h.hover_at(origin);
    w.h.run();
    press(&mut w, Modifiers::NONE, Key::ArrowUp);
    let moved_to = edit_cursor::of(shell(&w), w.pcb).unwrap().warped;
    let expected = on_screen(&w, mm(0.0, 1.0));
    assert!(
        moved_to.is_some_and(|p| (p - expected).length() < 0.5),
        "{moved_to:?} vs {expected:?}"
    );
    // The desktop moves the pointer there; the Edit Cursor stays.
    w.h.hover_at(expected + egui::vec2(1.0, 0.0));
    w.h.run();
    assert_eq!(keyed(&w), Some(mm(0.0, 1.0)));
    // A real move gives it back to the pointer.
    w.h.hover_at(expected + egui::vec2(30.0, 0.0));
    w.h.run();
    assert_eq!(keyed(&w), None);
}

/// Where the desktop refuses, or with the Preference off, the system pointer stays where
/// it is and the Edit Cursor leaves it behind until the mouse moves a few pixels.
fn edit_cursor_stays_until_the_mouse_moves(w: &mut Window) {
    let origin = on_screen(w, mm(0.0, 0.0));
    w.h.hover_at(origin);
    w.h.run();
    press(w, Modifiers::NONE, Key::ArrowUp);
    press(w, Modifiers::NONE, Key::ArrowUp);
    assert_eq!(edit_cursor::of(shell(w), w.pcb).unwrap().warped, None);
    w.h.hover_at(origin + egui::vec2(2.0, 1.0));
    w.h.run();
    assert_eq!(keyed(w), Some(mm(0.0, 2.0)));
    w.h.hover_at(origin + egui::vec2(8.0, 0.0));
    w.h.run();
    assert_eq!(keyed(w), None);
}

#[test]
fn where_the_desktop_refuses_the_pointer_is_left_behind() {
    let mut w = window_with(|app| app.with_desktop_moves_pointer(false));
    edit_cursor_stays_until_the_mouse_moves(&mut w);
}

#[test]
fn with_the_preference_off_the_pointer_is_left_behind() {
    let mut w = window();
    shell_run(
        &mut w,
        "app.preferences.set",
        json!({"key": "canvas.arrows_move_pointer", "value": false}),
    );
    edit_cursor_stays_until_the_mouse_moves(&mut w);
}

const ARROWS_MOVE_POINTER: &str = "Arrow keys move the mouse pointer";

#[test]
fn where_the_desktop_refuses_the_preference_shows_ticked_disabled_and_explained() {
    let mut w = window_with(|app| app.with_desktop_moves_pointer(false));
    shell_run(
        &mut w,
        "app.preferences.set",
        json!({"key": "canvas.arrows_move_pointer", "value": false}),
    );
    w.h.state_mut().open_preferences(Some(PageId::EDITING));
    w.h.run();
    let node =
        w.h.get_by_role_and_label(Role::CheckBox, ARROWS_MOVE_POINTER);
    assert_eq!(
        node.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    assert!(node.accesskit_node().is_disabled());
    w.h.get_by_label("Your desktop does not let REDA move the mouse pointer.");
    // The stored value is left alone.
    assert!(!shell(&w).preferences().canvas.arrows_move_pointer);
}

#[test]
fn where_the_desktop_allows_the_preference_is_a_plain_check_box() {
    let mut w = window();
    w.h.state_mut().open_preferences(Some(PageId::EDITING));
    w.h.run();
    let node =
        w.h.get_by_role_and_label(Role::CheckBox, ARROWS_MOVE_POINTER);
    assert!(!node.accesskit_node().is_disabled());
    node.click();
    w.h.run();
    assert!(!shell(&w).preferences().canvas.arrows_move_pointer);
}

#[test]
fn auto_pan_follows_the_edit_cursor_out_of_view_unless_it_is_off() {
    let mut w = window();
    w.h.hover_at(on_screen(&w, mm(0.0, 0.0)));
    w.h.run();
    let centre = |w: &Window| shell(w).document_tab(w.pcb).unwrap().camera.unwrap().center;
    let before = centre(&w);
    // Far enough right to leave the view whatever its zoom.
    let rect = canvas_rect(&w);
    let tab = shell(&w).document_tab(w.pcb).unwrap();
    let across = f64::from(rect.width()) * tab.camera.unwrap().nm_per_point / 1e6;
    let presses = (across / 10.0).ceil() as usize + 1;
    for _ in 0..presses {
        press(&mut w, Modifiers::SHIFT, Key::ArrowRight);
    }
    let keyed_at = keyed(&w).unwrap();
    assert!(centre(&w).0 > before.0, "the view moved right");
    let shown = on_screen(&w, keyed_at);
    assert!(rect.contains(shown), "{shown:?} is in {rect:?}");

    shell_run(
        &mut w,
        "app.preferences.set",
        json!({"key": "editing.auto_pan.style", "value": "off"}),
    );
    let still = centre(&w);
    for _ in 0..presses {
        press(&mut w, Modifiers::SHIFT, Key::ArrowRight);
    }
    assert_eq!(centre(&w), still);
}

#[test]
fn a_focused_text_field_keeps_the_arrows_and_enter() {
    let mut w = window();
    // A Footprint is selected, so Mod+arrows could nudge it.
    let footprint = {
        let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
        (pcb.footprints().find(|f| f.at == mm(0.0, 0.0)).unwrap()).uuid
    };
    shell_run(&mut w, "selection.set", json!({"objects": [footprint]}));
    w.h.hover_at(on_screen(&w, mm(0.0, 0.0)));
    w.h.run();
    // The Library panel's search field takes the keyboard; the board stays the active
    // Document.
    shell_run(&mut w, "panel.show", json!({"panel": "library"}));
    w.h.get_all_by_role(Role::TextInput)
        .next()
        .expect("the Library panel has a search field")
        .click();
    w.h.run();
    let pcb = w.pcb.to_string();
    shell_run(&mut w, "layout.tab.activate", json!({"tab": pcb}));
    assert!(
        w.h.ctx.memory(|m| m.focused().is_some()),
        "the field keeps focus"
    );
    let start = shell(&w).engine().journal().len();
    press(&mut w, Modifiers::NONE, Key::ArrowLeft);
    press(&mut w, Modifiers::SHIFT, Key::ArrowRight);
    // Mod+arrows move by words in the field; they don't nudge the selection.
    press(&mut w, Modifiers::COMMAND, Key::ArrowLeft);
    press(
        &mut w,
        Modifiers::COMMAND | Modifiers::SHIFT,
        Key::ArrowRight,
    );
    let ids: Vec<String> = journal_from(&w, start)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(
        !ids.iter()
            .any(|id| id.starts_with("view.cursor") || id.ends_with(".nudge")),
        "{ids:?}"
    );
    let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.footprint(footprint).unwrap().at, mm(0.0, 0.0));
    assert_eq!(keyed(&w), None);
}

#[test]
fn on_a_sheet_the_arrows_step_and_shift_enter_places_a_wire_corner() {
    const SHEET: &str = "00000000-0000-4000-8000-000000000002";
    let home = TempDir::new("edit-cursor-sheet-home");
    let work = TempDir::new("edit-cursor-sheet-work");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut sheet_shell = Shell::new(engine);
    sheet_shell.new_project(work.path(), "Sheet").unwrap();
    let sheet: Uuid = SHEET.parse().unwrap();
    sheet_shell
        .run("document.open", json!({"document": sheet}))
        .unwrap();
    sheet_shell
        .run("layout.tab.activate", json!({"tab": SHEET}))
        .unwrap();
    sheet_shell
        .run(
            "view.grid.set",
            json!({"document": sheet, "grid": "100mil"}),
        )
        .unwrap();
    sheet_shell
        .run("tool.activate", json!({"tool": "wire"}))
        .unwrap();
    let mut w = Window {
        h: Harness::builder()
            .with_size([1000.0, 900.0])
            .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(sheet_shell)),
        pcb: sheet,
        _home: home,
        _work: work,
    };
    w.h.run();
    let inch = |x: f64, y: f64| mm(x * 25.4, y * 25.4);
    w.h.hover_at(on_screen(&w, inch(1.0, 1.0)));
    w.h.run();
    let start = shell(&w).engine().journal().len();
    // The Wire Tool binds Enter (finish), so Shift+Enter clicks: the run starts at (1, 1).
    press(&mut w, Modifiers::SHIFT, Key::Enter);
    press(&mut w, Modifiers::NONE, Key::ArrowRight);
    press(&mut w, Modifiers::NONE, Key::ArrowRight);
    assert_eq!(keyed(&w), Some(inch(1.2, 1.0)));
    press(&mut w, Modifiers::SHIFT, Key::Enter);
    press(&mut w, Modifiers::NONE, Key::Enter);
    let ids: Vec<String> = journal_from(&w, start)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(
        ids.iter().filter(|id| *id == "view.cursor.step").count(),
        2,
        "{ids:?}"
    );
    let project = shell(&w).engine().project().unwrap();
    let sheet = project.schematic_sheet(sheet).unwrap();
    let wires: Vec<Vec<Point>> = sheet.wires().map(|w| w.vertices.clone()).collect();
    assert_eq!(wires, [vec![inch(1.0, 1.0), inch(1.2, 1.0)]], "{ids:?}");
}

#[test]
fn home_recentres_on_the_edit_cursor() {
    let mut w = window();
    w.h.hover_at(on_screen(&w, mm(0.0, 0.0)));
    w.h.run();
    press(&mut w, Modifiers::SHIFT, Key::ArrowUp);
    press(&mut w, Modifiers::NONE, Key::Home);
    let camera = shell(&w).document_tab(w.pcb).unwrap().camera.unwrap();
    assert_eq!(camera.center, (0.0, 10e6));
}
