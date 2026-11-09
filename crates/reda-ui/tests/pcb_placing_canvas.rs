//! The Placement Bar, Choose Component and the locked-move question in the window, driven
//! through `egui_kittest` (docs/spec/v0.1/app.md#placement-bar).

use std::path::{Path, PathBuf};

use egui::accesskit::{Role, Toggled};
use egui::{Key, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::bars::{self, Binding, DocumentType, Item};
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

struct Window {
    h: Harness<'static, App>,
    pcb: Uuid,
    footprints: Vec<Uuid>,
    _home: TempDir,
    _work: TempDir,
}

/// The window on a board with Footprints R1 and R2 at (0, 0) and (10, 0) mm.
fn window() -> Window {
    let home = TempDir::new("pcb-placing-canvas-home");
    let work = TempDir::new("pcb-placing-canvas-work");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let mut footprints = Vec::new();
    for (d, x) in [("R1", "0mm"), ("R2", "10mm")] {
        let out = shell
            .run(
                "test.footprint.add",
                json!({"at": {"x": x, "y": 0}, "designator": d, "pads": [
                    {"at": {"x": 0, "y": 0}, "width": "2mm", "height": "2mm"}
                ]}),
            )
            .unwrap();
        footprints.push(
            out["result"]["footprint"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
        );
    }
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": pcb.to_string()}))
        .unwrap();
    let h = Harness::builder()
        .with_size([1000.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    Window {
        h,
        pcb,
        footprints,
        _home: home,
        _work: work,
    }
}

fn shell(w: &Window) -> &Shell {
    w.h.state().shell()
}

fn run(w: &mut Window, id: &str, params: Value) {
    w.h.state_mut().shell_mut().run(id, params).unwrap();
    w.h.run();
    w.h.run();
}

fn last_command(w: &Window) -> String {
    shell(w).engine().journal().last().unwrap().id.clone()
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6) as i64), Nm((y * 1e6) as i64))
}

fn on_screen(w: &Window, p: Point) -> Pos2 {
    let rect = w.h.get_by_role(Role::Canvas).rect();
    let camera = shell(w).document_tab(w.pcb).unwrap().camera.unwrap();
    camera.screen_point(rect, (p.x.0 as f64, p.y.0 as f64))
}

fn click(w: &mut Window, at: Pos2) {
    w.h.hover_at(at);
    w.h.run();
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
}

fn toggled(w: &Window, label: &str) -> bool {
    w.h.get_by_label(label).accesskit_node().toggled() == Some(Toggled::True)
}

const OBSTACLE: &str = "Next Placement Obstacle Mode";
const GUIDES: &str = "Toggle Alignment Guides";
const RATSNEST: &str = "Next Ratsnest Mode in a Move";

#[test]
fn the_placement_bar_is_a_data_file_bound_to_the_placement_commands_with_its_icons() {
    let bars = bars::for_document(Some(DocumentType::Pcb));
    let bound: Vec<String> = bars.tool_bars["place"]
        .iter()
        .filter_map(|i| match i {
            Item::Action(a) => Some(match &a.binding {
                Binding::Command(id) | Binding::Preset(id) => id.clone(),
                Binding::Raw { command, .. } => command.clone(),
            }),
            _ => None,
        })
        .collect();
    assert_eq!(
        bound,
        [
            "pcb.place.mode:obstacle_mode",
            "pcb.place.mode:alignment_guides",
            "pcb.place.mode:ratsnest_mode",
            "tool.rotate",
            "tool.flip",
            "pcb.place.skip",
            "view.keys_card.toggle",
        ]
    );
    for icon in [
        "place_obstacle_ignore",
        "place_obstacle_avoid",
        "place_guides",
        "place_ratsnest_breaks",
        "place_ratsnest_hidden",
        "place_ratsnest_pad_to_pad",
        "place_queue",
    ] {
        assert!(reda_ui::icons::has(icon), "{icon} has art");
    }
}

#[test]
fn the_placement_bar_shows_with_only_footprints_selected_and_its_buttons_switch_the_modes() {
    use reda_ui::bars::draw::tool_bar_anchor;
    let mut w = window();
    w.h.run();
    assert!(
        w.h.query_by_label(OBSTACLE).is_none(),
        "not with nothing selected"
    );
    let [r1, r2] = [w.footprints[0], w.footprints[1]];
    run(&mut w, "selection.set", json!({"objects": [r1, r2]}));
    let canvas = w.h.get_by_role(Role::Canvas).rect();
    let anchor = tool_bar_anchor(canvas, false);
    let bar = w.h.get_by_label(OBSTACLE).rect();
    assert!(
        (bar.bottom() - anchor.y).abs() < 8.0,
        "{bar:?} at {anchor:?}"
    );
    assert!(!toggled(&w, OBSTACLE), "ignore by default");
    assert!(toggled(&w, GUIDES), "guides on by default");
    for label in [RATSNEST, "Rotate", "Flip Side", "Keys in Tool"] {
        assert!(w.h.query_by_label_contains(label).is_some(), "{label}");
    }
    assert!(
        w.h.query_by_label_contains("Skip Footprint").is_none(),
        "the queue shows only during Reposition Selected"
    );

    w.h.get_by_label(OBSTACLE).click();
    w.h.run();
    w.h.run();
    assert_eq!(last_command(&w), "app.preferences.set");
    assert!(toggled(&w, OBSTACLE), "toggled in avoid");
    w.h.get_by_label(GUIDES).click();
    w.h.run();
    w.h.run();
    assert!(!shell(&w).preferences().pcb.place.alignment_guides);
    w.h.get_by_label(RATSNEST).click();
    w.h.run();
    assert_eq!(
        shell(&w).preferences().pcb.place.ratsnest_mode,
        reda_ui::engine::pcb::place::RatsnestMode::Hidden
    );

    // During Reposition Selected the queue shows "1 of 2", and its click skips one.
    run(&mut w, "pcb.place.reposition", json!({}));
    w.h.get_by_label("Skip Footprint: 1 of 2").click();
    w.h.run();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.place.skip");
    w.h.get_by_label("Skip Footprint: 2 of 2");
}

#[test]
fn the_placement_bar_shows_during_a_move_and_r_switches_it() {
    let mut w = window();
    w.h.run();
    let a = on_screen(&w, mm(0.0, 0.0));
    w.h.hover_at(a);
    w.h.run();
    w.h.drag_at(a);
    w.h.run();
    w.h.hover_at(a + egui::vec2(0.0, 60.0));
    w.h.run();
    w.h.run();
    assert!(shell(&w).pcb_tool().moving());
    assert!(w.h.query_by_label(OBSTACLE).is_some(), "in flight");
    w.h.key_press(Key::R);
    w.h.run();
    w.h.run();
    assert!(toggled(&w, OBSTACLE));
    // The bar's Keys button lists the placement keys.
    let card = reda_ui::keys_card::card(shell(&w), false);
    let keys: Vec<&str> = card.rows.iter().map(|(k, _)| k.as_str()).collect();
    for key in ["R", "N", "Tab", "Space", "L"] {
        assert!(keys.contains(&key), "{key} in {keys:?}");
    }
    w.h.drop_at(a + egui::vec2(0.0, 60.0));
    w.h.run();
}

#[test]
fn moves_click_on_empty_space_opens_choose_component_and_its_button_floats_the_part() {
    let mut w = window();
    w.h.run();
    w.h.key_press(Key::M);
    w.h.run();
    let empty = on_screen(&w, mm(5.0, 3.0));
    click(&mut w, empty);
    w.h.run();
    w.h.get_by_label("Choose Component");
    w.h.get_by_role_and_label(Role::ListItem, "R2").click();
    w.h.run();
    w.h.run();
    w.h.get_by_label("Move component to cursor").click();
    w.h.run();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.place.pick_up");
    assert!(w.h.query_by_label("Choose Component").is_none());
    assert_eq!(shell(&w).pcb_tool().carried(), Some(&[w.footprints[1]][..]));
}

#[test]
fn a_locked_move_with_protection_off_asks_in_a_dialog() {
    let mut w = window();
    let r1 = w.footprints[0];
    run(&mut w, "pcb.lock", json!({"objects": [r1], "locked": true}));
    run(
        &mut w,
        "app.preferences.set",
        json!({"key": "editing.protect_locked_objects", "value": false}),
    );
    run(&mut w, "selection.set", json!({"objects": [r1]}));
    run(&mut w, "tool.rotate", json!({}));
    w.h.get_by_label("Move locked objects?");
    w.h.get_by_label("Move them").click();
    w.h.run();
    w.h.run();
    let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.footprint(r1).unwrap().rotation.0, 90_000);
    assert!(w.h.query_by_label("Move locked objects?").is_none());
}
