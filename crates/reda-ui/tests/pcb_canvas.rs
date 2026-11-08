//! The PCB canvas and the Layers panel in the window, driven through AccessKit with
//! `egui_kittest`: dragging a Footprint with the mouse runs nothing until release, the
//! view keys run their Commands, the Layers panel hides Layers, and a board with no
//! Board Outline offers to create a rectangle.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::geom::Point;
use reda_ui::engine::model::{LayerKind, Side, Uuid};
use reda_ui::engine::preferences::Units;
use reda_ui::shell::Shell;
use serde_json::json;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

struct Window {
    h: Harness<'static, App>,
    pcb: Uuid,
    footprint: Uuid,
    _home: TempDir,
    _work: TempDir,
}

/// The window on a board with one Footprint (a 2 mm Pad) at the origin, with its PCB tab
/// focused and the Layers panel open.
fn window() -> Window {
    window_with_step(0.25)
}

fn window_with_step(step: f32) -> Window {
    let home = TempDir::new("pcb-canvas-home");
    let work = TempDir::new("pcb-canvas-work");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let out = shell
        .run(
            "test.footprint.add",
            json!({"at": {"x": 0, "y": 0}, "pads": [
                {"at": {"x": 0, "y": 0}, "width": "2mm", "height": "2mm"}
            ]}),
        )
        .unwrap();
    let footprint = out["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": pcb.to_string()}))
        .unwrap();
    shell.run("panel.show", json!({"panel": "layers"})).unwrap();
    let h = Harness::builder()
        .with_step_dt(step)
        .with_max_steps(100)
        .with_size([1000.0, 1100.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    Window {
        h,
        pcb,
        footprint,
        _home: home,
        _work: work,
    }
}

fn shell(w: &Window) -> &Shell {
    w.h.state().shell()
}

fn journal_len(w: &Window) -> usize {
    shell(w).engine().journal().len()
}

fn last_command(w: &Window) -> String {
    shell(w).engine().journal().last().unwrap().id.clone()
}

/// Where the board point `p` shows in the window.
fn on_screen(w: &Window, p: Point) -> Pos2 {
    let canvas = w.h.get_by_role(Role::Canvas);
    let rect = canvas.rect();
    let camera = shell(w).document_tab(w.pcb).unwrap().camera.unwrap();
    camera.screen_point(rect, (p.x.0 as f64, p.y.0 as f64))
}

fn footprint_at(w: &Window) -> Point {
    let pcb = shell(w).engine().project().unwrap().pcb().unwrap();
    pcb.footprint(w.footprint).unwrap().at
}

#[test]
fn pads_follow_the_shared_click_shift_toggle_and_filter_rules() {
    let mut w = window();
    w.h.run();
    let at = on_screen(&w, Point::default());
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
    assert!(
        shell(&w).selection().objects.is_empty(),
        "the Pad is selected independently"
    );
    assert_eq!(shell(&w).selection().pads.len(), 1);
    w.h.event(egui::Event::ModifiersChanged(Modifiers::SHIFT));
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
    assert!(
        shell(&w).selection().is_empty(),
        "Shift toggles the Pad out"
    );
    w.h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    let pcb = w.pcb;
    w.h.state_mut()
        .shell_mut()
        .run(
            "selection.filter.set",
            json!({"document":pcb,"kinds":{"pad":false}}),
        )
        .unwrap();
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
    assert!(
        shell(&w).selection().pads.is_empty(),
        "filtered Pads cannot be picked"
    );
    w.h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    w.h.run();
    assert!(
        shell(&w).selection().pads.is_empty(),
        "Select All honours the same Filter"
    );
    let near_pad = on_screen(&w, Point::default()) + egui::vec2(1.0, 0.0);
    w.h.hover_at(near_pad);
    w.h.run();
    assert_eq!(
        shell(&w).pcb_snap(),
        Some((Point::default(), reda_ui::pcb::snap::SnapKind::Pad)),
        "filtering a Pad does not remove its snap target"
    );
}

#[test]
fn designator_text_selects_and_drags_alone_refuses_delete_and_follows_its_parent() {
    let mut w = window();
    let footprint = w.footprint;
    w.h.state_mut()
        .shell_mut()
        .run(
            "pcb.footprint.text.set",
            json!({"instances":[footprint],"field":"Designator","at":{"x":"4mm","y":0}}),
        )
        .unwrap();
    w.h.state_mut()
        .shell_mut()
        .run("view.zoom_fit", json!({"document":w.pcb}))
        .unwrap();
    w.h.run();
    w.h.run();
    let at = on_screen(
        &w,
        Point::new(
            reda_ui::engine::model::geom::Nm(4_000_000),
            reda_ui::engine::model::geom::Nm(0),
        ),
    );
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
    assert!(shell(&w).selection().objects.is_empty());
    assert_eq!(shell(&w).selection().texts.len(), 1);
    w.h.drag_at(at);
    w.h.run();
    w.h.hover_at(at + egui::vec2(50.0, 0.0));
    w.h.run();
    w.h.drop_at(at + egui::vec2(50.0, 0.0));
    w.h.run();
    assert_eq!(footprint_at(&w), Point::default());
    let before = shell(&w)
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .footprint(footprint)
        .unwrap()
        .clone();
    assert!(
        before
            .text_overrides
            .iter()
            .any(|t| t.at.is_some_and(|at| at.x.0 > 4_000_000))
    );
    w.h.key_press(Key::Delete);
    w.h.run();
    assert!(
        shell(&w)
            .message()
            .is_some_and(|m| m.contains("cannot be deleted alone"))
    );
    assert_eq!(
        shell(&w)
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .footprint(footprint),
        Some(&before)
    );
    let text = shell(&w).selection().texts[0];
    let anchor = |w: &Window| {
        shell(w)
            .document_tab(w.pcb)
            .unwrap()
            .scene
            .scene
            .items
            .iter()
            .find_map(|item| match (&item.child, &item.primitive) {
                (
                    Some(reda_ui::engine::render::Child::FootprintText {
                        footprint: f,
                        text: t,
                    }),
                    reda_ui::engine::render::Primitive::Glyphs(run),
                ) if *f == text.footprint && *t == text.text => Some(run.at),
                _ => None,
            })
            .unwrap()
    };
    let from = anchor(&w);
    w.h.state_mut()
        .shell_mut()
        .run(
            "selection.set",
            json!({"objects":[footprint],"texts":[{"footprint":footprint,"text":text.text}]}),
        )
        .unwrap();
    w.h.state_mut()
        .shell_mut()
        .run("view.zoom_fit", json!({"document":w.pcb}))
        .unwrap();
    w.h.run();
    w.h.run();
    let grab = on_screen(&w, from);
    w.h.drag_at(grab);
    w.h.run();
    w.h.hover_at(grab + egui::vec2(50.0, 0.0));
    w.h.run();
    w.h.drop_at(grab + egui::vec2(50.0, 0.0));
    w.h.run();
    let moved = footprint_at(&w);
    assert!(moved.x.0 > 0);
    assert_eq!(
        anchor(&w),
        from + moved,
        "a parent move carries its child once"
    );
    assert_eq!(
        shell(&w)
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .footprint(footprint)
            .unwrap()
            .text_overrides,
        before.text_overrides,
        "the child keeps its local placement"
    );
}

#[test]
fn active_layer_ranks_before_smaller_objects_on_other_visible_layers() {
    let mut w = window();
    let free =
        w.h.state_mut()
            .shell_mut()
            .run(
                "pcb.free_pad.add",
                json!({"kind":"smd","side":"bottom","at":{"x":0,"y":0},"width":"0.2mm"}),
            )
            .unwrap()["result"]["uuid"]
            .clone();
    w.h.run();
    let at = on_screen(&w, Point::default());
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
    assert_eq!(
        shell(&w).selection().pads.len(),
        1,
        "the larger active-layer Pad ranks first"
    );
    let bottom = shell(&w)
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .layers()
        .find(LayerKind::Copper, Some(Side::Bottom))
        .unwrap()
        .uuid;
    w.h.state_mut()
        .shell_mut()
        .run("pcb.layer.activate", json!({"layer":bottom}))
        .unwrap();
    w.h.state_mut()
        .shell_mut()
        .run("selection.set", json!({}))
        .unwrap();
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
    assert_eq!(
        shell(&w).selection().objects,
        [free.as_str().unwrap().parse::<Uuid>().unwrap()],
        "activating the other Layer changes stack priority"
    );
}

#[test]
fn protected_objects_are_skipped_by_click_but_pick_list_shows_locks_and_selects_explicitly() {
    let mut w = window_with_step(0.01);
    let footprint = w.footprint;
    w.h.state_mut()
        .shell_mut()
        .run("pcb.lock", json!({"footprints":[footprint],"locked":true}))
        .unwrap();
    w.h.state_mut()
        .shell_mut()
        .run(
            "selection.filter.set",
            json!({"document":w.pcb,"kinds":{"footprint_text":false}}),
        )
        .unwrap();
    w.h.run();
    let at = on_screen(&w, Point::default());
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
    assert!(
        shell(&w).selection().is_empty(),
        "protected objects are skipped"
    );
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
    w.h.get_by_label("Pick Object");
    assert!(
        w.h.query_all_by_label("Locked").count() >= 1,
        "the pick list marks locked entries"
    );
    w.h.get_by_label("Footprint · TP").click();
    w.h.run();
    assert_eq!(
        shell(&w).selection().objects,
        [footprint],
        "explicit list picks can select protected objects"
    );
}

#[test]
fn footprint_body_ranks_before_zones_and_keepouts_then_the_board_outline() {
    let mut w = window();
    let mut add = |id: &str, p: serde_json::Value| {
        w.h.state_mut().shell_mut().run(id, p).unwrap()["result"]["uuid"]
            .as_str()
            .unwrap()
            .parse::<Uuid>()
            .unwrap()
    };
    let outline = add(
        "pcb.board_outline.add_rect",
        json!({"width":"20mm","height":"20mm"}),
    );
    let vertices =
        |size: &str| json!([{"x":0,"y":0},{"x":size,"y":0},{"x":size,"y":size},{"x":0,"y":size}]);
    let zone = add(
        "pcb.zone.add",
        json!({"layer":"copper_top","vertices":vertices("4mm")}),
    );
    let keepout = add("pcb.keepout.add", json!({"vertices":vertices("8mm")}));
    w.h.state_mut()
        .shell_mut()
        .run(
            "selection.filter.set",
            json!({"document":w.pcb,"kinds":{"pad":false,"footprint_text":false}}),
        )
        .unwrap();
    w.h.state_mut()
        .shell_mut()
        .run("view.zoom_fit", json!({"document":w.pcb}))
        .unwrap();
    w.h.run();
    w.h.run();
    let at = on_screen(&w, Point::default());
    let click = |w: &mut Window| {
        w.h.drag_at(at);
        w.h.drop_at(at);
        w.h.run();
    };
    click(&mut w);
    assert_eq!(shell(&w).selection().objects, [w.footprint]);
    for (kind, expected) in [
        ("footprint_instance", zone),
        ("zone", keepout),
        ("keepout", outline),
    ] {
        w.h.state_mut()
            .shell_mut()
            .run(
                "selection.filter.set",
                json!({"document":w.pcb,"kinds":{kind:false}}),
            )
            .unwrap();
        click(&mut w);
        assert_eq!(
            shell(&w).selection().objects,
            [expected],
            "the next tier wins after filtering {kind}"
        );
    }
}

#[test]
fn dragging_a_footprint_runs_one_move_on_release() {
    let mut w = window();
    w.h.run();
    let from = on_screen(&w, Point::default());
    let before = journal_len(&w);

    w.h.drag_at(from);
    w.h.run();
    w.h.hover_at(from + egui::vec2(40.0, 0.0));
    w.h.run();
    w.h.hover_at(from + egui::vec2(80.0, 0.0));
    w.h.run();
    assert_eq!(
        footprint_at(&w),
        Point::default(),
        "nothing moves in flight"
    );
    assert!(
        shell(&w).engine().journal()[before..]
            .iter()
            .all(|e| e.id != "pcb.move"),
        "the move waits for release"
    );

    w.h.drop_at(from + egui::vec2(80.0, 0.0));
    w.h.run();
    assert_eq!(last_command(&w), "pcb.move");
    let at = footprint_at(&w);
    assert!(
        at.x.0 > 0 && at.y.0 == 0,
        "moved right, on the grid: {at:?}"
    );
    assert_eq!(at.x.0 % 500_000, 0);
}

#[test]
fn the_view_keys_run_their_commands_on_the_focused_board() {
    let mut w = window();
    w.h.run();
    // Focus the board: a press on its canvas.
    let empty = w.h.get_by_role(Role::Canvas).rect().left_bottom() + egui::vec2(30.0, -60.0);
    w.h.drag_at(empty);
    w.h.drop_at(empty);
    w.h.run();

    // The Keymap binds logical keys: "+" is egui's Key::Plus (typed with Shift or not).
    w.h.key_press_modifiers(Modifiers::SHIFT, Key::Plus);
    w.h.run();
    assert_eq!(last_command(&w), "pcb.layer.step:next");
    let bottom = {
        let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
        pcb.layers()
            .find(LayerKind::Copper, Some(Side::Bottom))
            .unwrap()
            .uuid
    };
    let project = shell(&w).engine().project().unwrap();
    assert_eq!(shell(&w).board_view().active_layer(project), Some(bottom));

    w.h.key_press_modifiers(Modifiers::CTRL, Key::Q);
    w.h.run();
    assert_eq!(shell(&w).board_view().units, Units::Mil);

    w.h.key_press_modifiers(Modifiers::SHIFT, Key::S);
    w.h.run();
    assert_eq!(last_command(&w), "pcb.view.inactive_layers");

    w.h.key_press(Key::G);
    w.h.run();
    assert_eq!(last_command(&w), "view.grid.cycle");

    // Space and L reach the Tool through the Keymap; Esc returns to Select.
    w.h.key_press(Key::Space);
    w.h.run();
    assert_eq!(last_command(&w), "tool.rotate");
    w.h.key_press(Key::Escape);
    w.h.run();
    assert_eq!(last_command(&w), "tool.activate:select");
}

#[test]
fn the_layers_panel_hides_a_layer() {
    let mut w = window();
    w.h.run();
    let silk = {
        let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
        pcb.layers()
            .find(LayerKind::Silkscreen, Some(Side::Top))
            .unwrap()
            .uuid
    };
    w.h.get_by_label("Show Top Silkscreen").click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.layer.show");
    assert!(shell(&w).board_view().layers.hidden.contains(&silk));

    w.h.get_by_label("Ratsnest").click();
    w.h.run();
    assert!(shell(&w).board_view().layers.ratsnest.hidden);
}

#[test]
fn a_board_with_no_outline_offers_a_rectangle() {
    let mut w = window();
    w.h.run();
    let has_outline = |w: &Window| {
        let pcb = shell(w).engine().project().unwrap().pcb().unwrap();
        pcb.outline().is_some()
    };
    assert!(!has_outline(&w));

    w.h.get_by_label("Create rectangle…").click();
    w.h.run();
    w.h.get_by_label("Create").click();
    w.h.run();

    assert!(has_outline(&w));
    assert_eq!(last_command(&w), "pcb.board_outline.add_rect");
    assert!(w.h.query_by_label("Create rectangle…").is_none());
}

#[test]
fn output_origin_can_be_clicked_boxed_selected_all_dragged_and_filtered() {
    let mut w = window();
    let origin: Uuid =
        w.h.state_mut()
            .shell_mut()
            .run("pcb.output_origin.set", json!({"at":{"x":"5mm","y":0}}))
            .unwrap()["result"]["uuid"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
    w.h.state_mut()
        .shell_mut()
        .run("view.zoom_fit", json!({"document":w.pcb}))
        .unwrap();
    w.h.run();
    w.h.run();
    let point = Point::new(
        reda_ui::engine::model::geom::Nm(5_000_000),
        reda_ui::engine::model::geom::Nm(0),
    );
    let at = on_screen(&w, point);
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
    assert_eq!(
        shell(&w).selection().objects,
        [origin],
        "Output Origin is pickable"
    );
    w.h.state_mut()
        .shell_mut()
        .run("selection.set", json!({}))
        .unwrap();
    let from = at - egui::vec2(20.0, 20.0);
    let to = at + egui::vec2(20.0, 20.0);
    w.h.drag_at(from);
    w.h.run();
    w.h.hover_at(to);
    w.h.run();
    w.h.drop_at(to);
    w.h.run();
    assert!(
        shell(&w).selection().objects.contains(&origin),
        "rectangle includes Output Origin"
    );
    w.h.state_mut()
        .shell_mut()
        .run("selection.set", json!({}))
        .unwrap();
    w.h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    w.h.run();
    assert!(
        shell(&w).selection().objects.contains(&origin),
        "Select All includes Output Origin"
    );
    w.h.state_mut()
        .shell_mut()
        .run("selection.set", json!({"objects":[origin]}))
        .unwrap();
    w.h.drag_at(at);
    w.h.run();
    w.h.hover_at(at + egui::vec2(50.0, 0.0));
    w.h.run();
    w.h.drop_at(at + egui::vec2(50.0, 0.0));
    w.h.run();
    assert_eq!(shell(&w).selection().objects, [origin]);
    let moved = shell(&w)
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .output_origin()
        .unwrap()
        .at;
    assert!(moved.x > point.x, "drag moves Output Origin");
    assert_eq!(footprint_at(&w), Point::default());
    w.h.state_mut()
        .shell_mut()
        .run(
            "selection.filter.set",
            json!({"document":w.pcb,"kinds":{"output_origin":false}}),
        )
        .unwrap();
    w.h.state_mut()
        .shell_mut()
        .run("selection.set", json!({}))
        .unwrap();
    let at = on_screen(&w, moved);
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
    assert!(
        shell(&w).selection().is_empty(),
        "Output Origin obeys its Filter"
    );
}

fn mm(x: f64) -> reda_ui::engine::model::geom::Nm {
    reda_ui::engine::model::geom::Nm((x * 1e6) as i64)
}

/// Adds a 1 mm SMD Free Pad on top at (`x`, 0) mm, zooms to fit, and returns its UUID.
fn free_pad(w: &mut Window, x: &str) -> Uuid {
    let free: Uuid =
        w.h.state_mut()
            .shell_mut()
            .run(
                "pcb.free_pad.add",
                json!({"kind":"smd","side":"top","at":{"x":x,"y":0},"width":"1mm","height":"1mm"}),
            )
            .unwrap()["result"]["uuid"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
    let pcb = w.pcb;
    w.h.state_mut()
        .shell_mut()
        .run("view.zoom_fit", json!({"document":pcb}))
        .unwrap();
    w.h.run();
    w.h.run();
    free
}

fn click(w: &mut Window, at: Pos2) {
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
}

/// [`click`], `gap` seconds after the last input.
fn click_after(w: &mut Window, at: Pos2, gap: f64) {
    w.h.input_mut().time = Some(w.h.ctx.input(|i| i.time) + gap);
    click(w, at);
}

#[test]
fn repeat_clicks_and_shift_tab_cycle_the_board_stack() {
    let mut w = window_with_step(0.01);
    w.h.run();
    let at = on_screen(&w, Point::default());
    click_after(&mut w, at, 1.0);
    assert_eq!(shell(&w).selection().pads.len(), 1, "the Pad ranks first");
    // Inside the 600 ms repeat window, but past a double-click.
    click_after(&mut w, at + egui::vec2(1.0, 0.0), 0.45);
    assert_eq!(
        shell(&w).selection().texts.len(),
        1,
        "a repeat click picks the next tier: the Designator on silkscreen"
    );
    click_after(&mut w, at, 0.45);
    assert_eq!(
        shell(&w).selection().objects,
        [w.footprint],
        "and then the Footprint body"
    );
    w.h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    w.h.run();
    assert_eq!(
        shell(&w).selection().pads.len(),
        1,
        "Shift+Tab cycles back to the Pad"
    );
    click_after(&mut w, at, 1.0);
    click_after(&mut w, at, 0.7);
    assert!(
        shell(&w).selection().objects.is_empty(),
        "a click after 600 ms doesn't cycle"
    );
}

#[test]
fn shift_click_adds_on_the_board_and_mod_click_replaces() {
    let mut w = window_with_step(0.01);
    let free = free_pad(&mut w, "5mm");
    let pad = on_screen(&w, Point::default());
    let other = on_screen(&w, Point::new(mm(5.0), mm(0.0)));
    click_after(&mut w, pad, 1.0);
    assert_eq!(shell(&w).selection().pads.len(), 1);
    w.h.event(egui::Event::ModifiersChanged(Modifiers::SHIFT));
    click_after(&mut w, other, 1.0);
    assert_eq!(
        shell(&w).selection().pads.len(),
        1,
        "Shift+click keeps the Pad"
    );
    assert_eq!(
        shell(&w).selection().objects,
        [free],
        "and adds the Free Pad"
    );
    w.h.event(egui::Event::ModifiersChanged(Modifiers::COMMAND));
    click_after(&mut w, other, 1.0);
    assert_eq!(shell(&w).selection().objects, [free]);
    assert!(
        shell(&w).selection().pads.is_empty(),
        "Mod+click doesn't toggle; it selects just the object"
    );
}

#[test]
fn click_clears_selection_off_keeps_the_board_selection() {
    let mut w = window();
    let footprint = w.footprint;
    w.h.state_mut()
        .shell_mut()
        .run(
            "app.preferences.set",
            json!({"key": "editing.click_clears_selection", "value": false}),
        )
        .unwrap();
    w.h.state_mut()
        .shell_mut()
        .run("selection.set", json!({"objects":[footprint]}))
        .unwrap();
    w.h.run();
    let empty = w.h.get_by_role(Role::Canvas).rect().left_bottom() + egui::vec2(30.0, -60.0);
    click_after(&mut w, empty, 1.0);
    assert_eq!(shell(&w).selection().objects, [footprint]);
    w.h.state_mut()
        .shell_mut()
        .run(
            "app.preferences.set",
            json!({"key": "editing.click_clears_selection", "value": true}),
        )
        .unwrap();
    click_after(&mut w, empty, 1.0);
    assert!(shell(&w).selection().is_empty());
}

#[test]
fn board_rectangle_direction_follows_the_screen_also_from_below() {
    let mut w = window();
    let free = free_pad(&mut w, "5mm");
    // A second Pad far off widens the view, so the box below is well inside the canvas.
    free_pad(&mut w, "15mm");
    let pcb = w.pcb;
    // A box over the first Free Pad's right half only (it spans 4.5 to 5.5 mm), its
    // corners clear of the Pad so the drag draws a rectangle.
    let corners = |w: &Window| {
        let (a, b) = (
            on_screen(w, Point::new(mm(5.0), mm(1.0))),
            on_screen(w, Point::new(mm(6.0), mm(-1.0))),
        );
        let rect = w.h.get_by_role(Role::Canvas).rect();
        let mirrored = shell(w).document_tab(pcb).unwrap().mirrored();
        let mirror = |p: Pos2| {
            if mirrored {
                egui::pos2(2.0 * rect.center().x - p.x, p.y)
            } else {
                p
            }
        };
        let (a, b) = (mirror(a), mirror(b));
        // (left, right) as seen on screen.
        if a.x < b.x { (a, b) } else { (b, a) }
    };
    let rectangle = |w: &mut Window, from: Pos2, to: Pos2| {
        w.h.state_mut()
            .shell_mut()
            .run("selection.set", json!({}))
            .unwrap();
        w.h.input_mut().time = Some(w.h.ctx.input(|i| i.time) + 1.0);
        w.h.drag_at(from);
        w.h.run();
        w.h.hover_at(to);
        w.h.run();
        w.h.drop_at(to);
        w.h.run();
        shell(w).selection().objects.clone()
    };
    for from_bottom in [false, true] {
        w.h.state_mut()
            .shell_mut()
            .run("pcb.view.from_bottom", json!({"from_bottom": from_bottom}))
            .unwrap();
        w.h.run();
        w.h.run();
        assert_eq!(
            shell(&w).document_tab(pcb).unwrap().mirrored(),
            from_bottom,
            "the board is seen from below, mirrored"
        );
        let (left, right) = corners(&w);
        assert!(
            rectangle(&mut w, left, right).is_empty(),
            "left to right on screen encloses (from below: {from_bottom})"
        );
        assert_eq!(
            rectangle(&mut w, right, left),
            [free],
            "right to left on screen touches (from below: {from_bottom})"
        );
    }
}

#[test]
fn double_clicking_a_single_board_object_selects_it_and_opens_properties() {
    let mut w = window_with_step(0.01);
    let free = free_pad(&mut w, "8mm");
    let before = journal_len(&w);
    let at = on_screen(&w, Point::new(mm(8.0), mm(0.0)));
    w.h.input_mut().time = Some(w.h.ctx.input(|i| i.time) + 1.0);
    for _ in 0..2 {
        click(&mut w, at);
    }
    assert_eq!(shell(&w).selection().objects, [free]);
    assert!(
        shell(&w).engine().journal()[before..]
            .iter()
            .any(|e| e.id == "panel.show" && e.outcome.is_ok()),
        "a double-click on a single object opens Properties"
    );
}

#[test]
fn a_zone_cutout_has_a_layers_panel_row_and_a_selection_filter_kind() {
    let mut w = window();
    // A Zone Cutout away from the Footprint, from (5, -2) to (9, 2) mm.
    let cutout: Uuid =
        w.h.state_mut()
            .shell_mut()
            .run(
                "pcb.zone_cutout.add",
                json!({"layer": "copper_top", "vertices": [
                    {"x": "5mm", "y": "-2mm"}, {"x": "9mm", "y": "-2mm"},
                    {"x": "9mm", "y": "2mm"}, {"x": "5mm", "y": "2mm"}]}),
            )
            .unwrap()["result"]["uuid"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
    w.h.state_mut()
        .shell_mut()
        .run("view.zoom_fit", json!({"document": w.pcb}))
        .unwrap();
    w.h.run();
    w.h.run();
    let shown = |w: &Window| {
        let tab = shell(w).document_tab(w.pcb).unwrap();
        tab.scene.scene.items_of(cutout).next().is_some()
    };
    assert!(shown(&w));

    // Clicking its edge selects it.
    let edge = on_screen(
        &w,
        Point::new(
            reda_ui::engine::model::geom::Nm(5_000_000),
            reda_ui::engine::model::geom::Nm(0),
        ),
    );
    let click = |w: &mut Window| {
        w.h.drag_at(edge);
        w.h.drop_at(edge);
        w.h.run();
    };
    click(&mut w);
    assert_eq!(shell(&w).selection().objects, [cutout]);

    // Its Selection Filter kind keeps it from being picked.
    let kinds =
        w.h.state_mut()
            .shell_mut()
            .run("selection.filter.set", json!({"document": w.pcb}))
            .unwrap();
    assert!(
        kinds.to_string().contains("\"kind\":\"zone_cutout\""),
        "{kinds}"
    );
    w.h.state_mut()
        .shell_mut()
        .run(
            "selection.filter.set",
            json!({"document": w.pcb, "kinds": {"zone_cutout": false}}),
        )
        .unwrap();
    w.h.state_mut()
        .shell_mut()
        .run("selection.set", json!({}))
        .unwrap();
    click(&mut w);
    assert!(shell(&w).selection().objects.is_empty());

    // The Layers panel's Zone Cutouts row hides every one.
    w.h.get_by_label("Zone Cutouts").click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.view.object_type.show");
    assert!(!shown(&w));
    assert!(
        shell(&w)
            .board_view()
            .layers
            .hidden_types
            .contains(&reda_ui::engine::model::ObjectType::ZoneCutout)
    );
}
