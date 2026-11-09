//! The Track Tool's modes and keys through the Shell with no window
//! (docs/spec/v0.1/pcb.md#preview to #width): look-ahead (`1`), Esc and right-click
//! ending a route, the width mode (`3`) and chooser (`Shift+W`), the Via size chooser
//! (`Shift+V`), the clearance display (`C`), loop removal (`Shift+L`), auto-terminate
//! (`Shift+A`), obstacle mode (`Shift+R`), `E` for a Via that ends the route, and the
//! preview's looks.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::pcb::route::{CornerMode, Posture};
use reda_ui::pcb::route::{Chooser, Look};
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

struct Board {
    shell: Shell,
    net: Uuid,
    _home: TempDir,
    _work: TempDir,
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

fn add_pad(shell: &mut Shell, x: &str, y: &str, net: &Value) -> Uuid {
    let out = run(
        shell,
        "test.footprint.add",
        json!({"at": {"x": x, "y": y}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": net}
        ]}),
    );
    out["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// Two one-Pad (1 mm square, top) Footprints on Net SIG at (0, 0) and (10, 0) mm, the
/// PCB open, the grid at 0.5 mm.
fn board() -> Board {
    let home = TempDir::new("pcb-route-tool-home");
    let work = TempDir::new("pcb-route-tool-work");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let net = run(&mut shell, "test.net.add", json!({"name": "SIG"}))["result"]["uuid"].clone();
    add_pad(&mut shell, "0mm", "0mm", &net);
    add_pad(&mut shell, "10mm", "0mm", &net);
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    run(
        &mut shell,
        "view.grid.set",
        json!({"document": pcb, "grid": "0.5mm"}),
    );
    Board {
        shell,
        net: net.as_str().unwrap().parse().unwrap(),
        _home: home,
        _work: work,
    }
}

fn journal_ids(b: &Board) -> Vec<String> {
    b.shell
        .engine()
        .journal()
        .iter()
        .map(|e| e.id.clone())
        .collect()
}

fn last_entry(b: &Board) -> (String, Value) {
    let e = b.shell.engine().journal().last().unwrap();
    (e.id.clone(), e.params.clone())
}

fn route_to(b: &mut Board, at: Point) {
    b.shell.route_hover(at);
    b.shell.route_click(at);
}

fn pref(b: &Board, key: &str) -> Value {
    let all = serde_json::to_value(b.shell.preferences()).unwrap();
    key.split('.').fold(all, |v, k| v[k].clone())
}

fn route_points(params: &Value) -> Vec<Point> {
    params["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| Point::new(Nm(p["x"].as_i64().unwrap()), Nm(p["y"].as_i64().unwrap())))
        .collect()
}

#[test]
fn look_ahead_is_on_and_1_toggles_it_and_an_ending_click_places_both_segments() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:route", json!({}));
    assert!(b.shell.router().lookahead(), "on by default");
    route_to(&mut b, mm(0.0, 0.0));
    b.shell.route_hover(mm(3.0, 1.0));
    let proposed = b.shell.route_preview().unwrap().proposed.unwrap();
    assert_eq!(proposed.place, [mm(0.0, 0.0), mm(2.0, 0.0)]);
    assert_eq!(proposed.look_ahead, [mm(2.0, 0.0), mm(3.0, 1.0)]);
    b.shell.route_click(mm(3.0, 1.0));
    assert_eq!(
        b.shell.route_preview().unwrap().placed,
        [mm(0.0, 0.0), mm(2.0, 0.0)],
        "a click places only the first segment"
    );

    run(&mut b.shell, "tool.undo_segment", json!({}));
    run(&mut b.shell, "tool.undo_segment", json!({}));
    assert!(b.shell.route_preview().is_none());

    // A click that ends the route places both, look-ahead or not.
    route_to(&mut b, mm(0.0, 0.0));
    route_to(&mut b, mm(0.0, 3.0));
    b.shell.route_hover(mm(10.0, 0.0));
    let proposed = b.shell.route_preview().unwrap().proposed.unwrap();
    assert!(proposed.ends_route);
    assert!(proposed.look_ahead.is_empty());
    b.shell.route_click(mm(10.0, 0.0));
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.add");
    assert_eq!(
        route_points(&params),
        [mm(0.0, 0.0), mm(0.0, 3.0), mm(7.0, 3.0), mm(10.0, 0.0)]
    );

    run(&mut b.shell, "pcb.route.mode:lookahead", json!({}));
    assert!(!b.shell.router().lookahead(), "`1` turns it off");
    route_to(&mut b, mm(0.0, 0.0));
    b.shell.route_hover(mm(3.0, -1.0));
    b.shell.route_click(mm(3.0, -1.0));
    assert_eq!(
        b.shell.route_preview().unwrap().placed,
        [mm(0.0, 0.0), mm(2.0, 0.0), mm(3.0, -1.0)],
        "off, a click places both"
    );
}

#[test]
fn escape_ends_and_keeps_the_route_then_returns_to_select() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    route_to(&mut b, mm(0.0, 3.0));
    b.shell.route_hover(mm(4.0, 3.0));
    // Esc and right-click both run `pcb.route.end`.
    run(&mut b.shell, "pcb.route.end", json!({}));
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.add", "what is placed is kept");
    assert_eq!(route_points(&params), [mm(0.0, 0.0), mm(0.0, 3.0)]);
    assert!(b.shell.route_preview().is_none());
    assert_eq!(b.shell.tool(), Tool::Route, "the Tool stays");

    run(&mut b.shell, "pcb.route.end", json!({}));
    assert_eq!(
        b.shell.tool(),
        Tool::Select,
        "a second Esc returns to Select"
    );
}

#[test]
fn escape_with_only_the_start_point_drops_the_route_and_stays_in_the_tool() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    b.shell.route_hover(mm(4.0, 3.0));
    let before = journal_ids(&b).len();
    run(&mut b.shell, "pcb.route.end", json!({}));
    assert!(b.shell.route_preview().is_none(), "dropped");
    assert!(b.shell.tool_preview().is_none());
    assert_eq!(b.shell.tool(), Tool::Route, "stays in the Tool");
    assert_eq!(
        journal_ids(&b)[before..],
        ["pcb.route.end"],
        "nothing commits"
    );
}

#[test]
fn the_keys_are_bound_in_the_track_tool() {
    use reda_ui::bars::{Binding, DocumentType};
    use reda_ui::keymap::{Active, Chord};
    let b = board();
    let keymap = b.shell.keymap();
    let active = Active {
        tool: Some("route".to_owned()),
        document: Some(DocumentType::Pcb),
        part: false,
    };
    for (key, command) in [
        ("1", "pcb.route.mode:lookahead"),
        ("3", "pcb.route.mode:width_mode"),
        ("Space", "pcb.route.mode:posture"),
        ("Shift+R", "pcb.route.mode:obstacle_mode"),
        ("Shift+A", "pcb.route.mode:auto_terminate"),
        ("Shift+L", "pcb.route.mode:loop_removal"),
        ("C", "pcb.route.mode:clearance_display"),
        ("Shift+W", "pcb.route.chooser:width"),
        ("Shift+V", "pcb.route.chooser:via"),
        ("E", "pcb.route.via_end"),
        ("Esc", "pcb.route.end"),
        ("/", "tool.cycle_corner"),
    ] {
        let action = keymap.lookup(&Chord::parse(key).unwrap(), &active);
        let bound = action.map(|a| match &a.binding {
            Binding::Command(id) | Binding::Preset(id) => id.clone(),
            Binding::Raw { command, .. } => command.clone(),
        });
        assert_eq!(bound.as_deref(), Some(command), "{key}");
    }
}

/// SIG in a class with Track widths min 0.2 mm, preferred 0.3 mm and max 0.5 mm.
fn class_widths(b: &mut Board) {
    let class = run(
        &mut b.shell,
        "pcb.net_class.add",
        json!({"name": "Power", "rules": [{"kind": "track_width",
               "values": {"min": "0.2mm", "preferred": "0.3mm", "max": "0.5mm"}}]}),
    )["result"]["uuid"]
        .clone();
    let net = b.net;
    run(
        &mut b.shell,
        "pcb.net.add_to_class",
        json!({"nets": [net], "class": class}),
    );
}

#[test]
fn three_cycles_the_width_mode_and_the_route_takes_its_width() {
    let mut b = board();
    class_widths(&mut b);
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    assert_eq!(b.shell.route_preview().unwrap().width, Nm::from_um(300));
    let width = |b: &Board| b.shell.route_preview().unwrap().width;

    run(&mut b.shell, "pcb.route.mode:width_mode", json!({}));
    assert_eq!(pref(&b, "pcb.route.width_mode"), "max");
    assert_eq!(width(&b), Nm::from_um(500));
    // An empty `user` is skipped.
    run(&mut b.shell, "pcb.route.mode:width_mode", json!({}));
    assert_eq!(pref(&b, "pcb.route.width_mode"), "min");
    assert_eq!(width(&b), Nm::from_um(200));
    run(&mut b.shell, "pcb.route.mode:width_mode", json!({}));
    assert_eq!(pref(&b, "pcb.route.width_mode"), "preferred");
    assert_eq!(width(&b), Nm::from_um(300));
}

#[test]
fn shift_w_opens_the_width_chooser_and_a_choice_applies_from_the_last_vertex() {
    let mut b = board();
    class_widths(&mut b);
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key": "pcb.route.favourite_widths", "value": ["0.4mm"]}),
    );
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    route_to(&mut b, mm(0.0, 3.0));
    run(&mut b.shell, "pcb.route.chooser:width", json!({}));
    let Some(Chooser::Width(rows)) = b.shell.router().chooser() else {
        panic!("the width chooser is open");
    };
    let widths: Vec<Nm> = rows.iter().map(|r| r.width).collect();
    assert_eq!(
        widths,
        [
            Nm::from_um(200),
            Nm::from_um(300),
            Nm::from_um(500),
            Nm::from_um(400)
        ]
    );
    // Choosing (or typing) runs `pcb.route.width`, which closes the chooser.
    run(&mut b.shell, "pcb.route.width", json!({"width": "0.4mm"}));
    assert!(b.shell.router().chooser().is_none());
    assert_eq!(b.shell.route_preview().unwrap().width, Nm::from_um(400));
    assert_eq!(
        b.shell.route_preview().unwrap().legs[0].width,
        Nm::from_um(300)
    );
    // A typed width outside the Net's range is clipped, with a note.
    run(&mut b.shell, "pcb.route.width", json!({"width": "2mm"}));
    assert_eq!(b.shell.route_preview().unwrap().width, Nm::from_um(500));
    assert!(b.shell.message().is_some());
}

#[test]
fn shift_v_opens_the_via_size_chooser_and_the_next_via_takes_the_size() {
    let mut b = board();
    run(
        &mut b.shell,
        "pcb.via.add",
        json!({"at": {"x": "5mm", "y": "8mm"}, "diameter": "0.9mm", "hole": "0.45mm"}),
    );
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    run(&mut b.shell, "pcb.route.chooser:via", json!({}));
    let Some(Chooser::Via(rows)) = b.shell.router().chooser() else {
        panic!("the Via chooser is open");
    };
    assert_eq!(rows.len(), 2, "the Net's size and the one on the board");
    assert_eq!(rows[1].size.diameter, Nm::from_um(900));
    run(
        &mut b.shell,
        "pcb.route.via_size",
        json!({"diameter": "0.9mm", "hole": "0.45mm"}),
    );
    assert!(b.shell.router().chooser().is_none());
    b.shell.route_hover(mm(0.0, 4.0));
    run(&mut b.shell, "pcb.route.add_via", json!({}));
    route_to(&mut b, mm(3.0, 4.0));
    run(&mut b.shell, "pcb.route.finish", json!({}));
    let (id, params) = last_entry(&b);
    assert_eq!(id, "batch");
    let via = &params["commands"][1];
    assert_eq!(via["id"], "pcb.via.add");
    assert_eq!(via["params"]["diameter"], 900_000);
    assert_eq!(via["params"]["hole"], 450_000);
}

#[test]
fn c_cycles_the_clearance_display_near_all_and_off() {
    let mut b = board();
    let other =
        run(&mut b.shell, "test.net.add", json!({"name": "OTHER"}))["result"]["uuid"].clone();
    add_pad(&mut b.shell, "3mm", "3mm", &other);
    add_pad(&mut b.shell, "30mm", "30mm", &other);
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    b.shell.route_hover(mm(1.0, 1.0));
    assert_eq!(pref(&b, "pcb.route.clearance_display"), "near");
    assert_eq!(
        b.shell.router().halos().len(),
        1,
        "within 5 mm of the cursor"
    );
    run(&mut b.shell, "pcb.route.mode:clearance_display", json!({}));
    assert_eq!(pref(&b, "pcb.route.clearance_display"), "all");
    assert_eq!(b.shell.router().halos().len(), 2);
    run(&mut b.shell, "pcb.route.mode:clearance_display", json!({}));
    assert_eq!(pref(&b, "pcb.route.clearance_display"), "off");
    assert!(b.shell.router().halos().is_empty());
}

#[test]
fn shift_l_shift_a_and_shift_r_toggle_their_preferences() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:route", json!({}));
    for (mode, key, values) in [
        (
            "loop_removal",
            "pcb.route.loop_removal",
            [json!(false), json!(true)],
        ),
        (
            "auto_terminate",
            "pcb.route.auto_terminate",
            [json!(false), json!(true)],
        ),
        (
            "obstacle_mode",
            "pcb.route.obstacle_mode",
            [json!("ignore"), json!("highlight")],
        ),
    ] {
        for value in values {
            run(&mut b.shell, &format!("pcb.route.mode:{mode}"), json!({}));
            assert_eq!(pref(&b, key), value, "{mode}");
        }
    }
    run(&mut b.shell, "pcb.route.mode:obstacle_mode", json!({}));
    assert_eq!(pref(&b, "pcb.route.obstacle_mode"), "stop");

    // Auto-terminate off: reaching SIG's Pad doesn't end the route.
    run(&mut b.shell, "pcb.route.mode:auto_terminate", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    b.shell.route_hover(mm(10.0, 0.0));
    assert!(
        !b.shell
            .route_preview()
            .unwrap()
            .proposed
            .unwrap()
            .ends_route
    );
}

#[test]
fn shift_l_off_keeps_the_old_path_on_a_reroute() {
    let mut b = board();
    let net = b.net;
    run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer": "copper_top", "net": net,
               "points": [{"x": 0, "y": 0}, {"x": 0, "y": "5mm"},
                          {"x": "10mm", "y": "5mm"}, {"x": "10mm", "y": 0}]}),
    );
    run(&mut b.shell, "tool.activate:route", json!({}));
    run(&mut b.shell, "pcb.route.mode:loop_removal", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    b.shell.route_hover(mm(10.0, 0.0));
    let proposed = b.shell.route_preview().unwrap().proposed.unwrap();
    assert!(proposed.removes.removed.is_empty(), "no red preview");
    b.shell.route_click(mm(10.0, 0.0));
    assert_eq!(last_entry(&b).0, "pcb.track.add", "nothing deleted");
}

#[test]
fn e_places_a_via_at_the_cursor_and_ends_the_route() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    b.shell.route_hover(mm(0.0, 4.0));
    run(&mut b.shell, "pcb.route.via_end", json!({}));
    assert!(b.shell.route_preview().is_none(), "the route ended");
    let (id, params) = last_entry(&b);
    assert_eq!(id, "batch");
    let ids: Vec<&str> = params["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["pcb.track.add", "pcb.via.add"]);
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.vias().next().unwrap().at, mm(0.0, 4.0));
    assert_eq!(b.shell.tool(), Tool::Route);
}

#[test]
fn slash_cycles_45_90_and_any_angle_and_space_flips_the_posture() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    let ahead = |b: &mut Board| {
        b.shell.route_hover(mm(3.0, 1.0));
        b.shell.route_preview().unwrap().ahead
    };
    assert_eq!(b.shell.router().corner(), CornerMode::Deg45);
    assert_eq!(ahead(&mut b), [mm(0.0, 0.0), mm(2.0, 0.0), mm(3.0, 1.0)]);
    run(&mut b.shell, "pcb.route.mode:posture", json!({}));
    assert_eq!(b.shell.router().posture(), Posture::Flipped);
    assert_eq!(ahead(&mut b), [mm(0.0, 0.0), mm(1.0, 1.0), mm(3.0, 1.0)]);
    run(&mut b.shell, "pcb.route.mode:posture", json!({}));
    run(&mut b.shell, "tool.cycle_corner", json!({}));
    assert_eq!(b.shell.router().corner(), CornerMode::Deg90);
    assert_eq!(ahead(&mut b), [mm(0.0, 0.0), mm(3.0, 0.0), mm(3.0, 1.0)]);
    run(&mut b.shell, "tool.cycle_corner", json!({}));
    assert_eq!(b.shell.router().corner(), CornerMode::Any);
    assert_eq!(ahead(&mut b), [mm(0.0, 0.0), mm(3.0, 1.0)], "no second leg");
    run(&mut b.shell, "tool.cycle_corner", json!({}));
    assert_eq!(b.shell.router().corner(), CornerMode::Deg45);
}

#[test]
fn the_preview_draws_proposed_segments_translucent_look_ahead_hollow_and_removals_red() {
    use reda_ui::pcb::route::strokes;
    let mut b = board();
    let net = b.net;
    run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer": "copper_top", "net": net,
               "points": [{"x": 0, "y": 0}, {"x": 0, "y": "5mm"},
                          {"x": "10mm", "y": "5mm"}, {"x": "10mm", "y": 0}]}),
    );
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    b.shell.route_hover(mm(3.0, -1.0));
    let accent = egui::Color32::from_rgb(60, 140, 220);
    let red = egui::Color32::from_rgb(230, 60, 60);
    let drawn = strokes(&b.shell.route_preview().unwrap());
    let proposed: Vec<_> = drawn.iter().filter(|s| s.look == Look::Proposed).collect();
    assert_eq!(proposed.len(), 1);
    assert_eq!(proposed[0].points, [mm(0.0, 0.0), mm(2.0, 0.0)]);
    let color = Look::Proposed.color(accent, red);
    assert!(color.a() > 0 && color.a() < 255, "translucent: {color:?}");
    assert!(!Look::Proposed.hollow());
    let tail: Vec<_> = drawn.iter().filter(|s| s.look == Look::LookAhead).collect();
    assert_eq!(tail[0].points, [mm(2.0, 0.0), mm(3.0, -1.0)]);
    assert!(Look::LookAhead.hollow());

    b.shell.route_hover(mm(10.0, 0.0));
    let drawn = strokes(&b.shell.route_preview().unwrap());
    let removed: Vec<_> = drawn.iter().filter(|s| s.look == Look::Removed).collect();
    assert_eq!(removed.len(), 3, "the old path, in red");
    let color = Look::Removed.color(accent, red);
    assert_eq!(
        (color.r(), color.g(), color.b()),
        (red.r(), red.g(), red.b())
    );
}
