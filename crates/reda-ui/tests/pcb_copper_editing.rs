//! Editing routed copper on the board (docs/spec/v0.1/pcb.md#dragging-tracks and
//! #selecting-and-clearing-copper): the drag gestures over the drag Commands (slide with
//! `stop`, Track end and corner, midpoint handle and `Mod`+Shift break, Via and Free Pad,
//! the in-flight keys), Tab widening a copper selection, and Backspace / `Mod`+Del
//! unwinding, through the Shell and in the window.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

const TOLERANCE: Nm = Nm::from_um(200);

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

fn shell_with_board(home: &TempDir, work: &TempDir) -> (Shell, Uuid) {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let net = run(&mut shell, "test.net.add", json!({"name": "SIG"}))["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    run(
        &mut shell,
        "view.grid.set",
        json!({"document": pcb, "grid": "0.5mm"}),
    );
    (shell, net)
}

/// An empty board with Net SIG, the PCB open, the grid at 0.5 mm.
fn board() -> Board {
    let home = TempDir::new("pcb-copper-home");
    let work = TempDir::new("pcb-copper-work");
    let (shell, net) = shell_with_board(&home, &work);
    Board {
        shell,
        net,
        _home: home,
        _work: work,
    }
}

fn add_route(b: &mut Board, points: &[(f64, f64)]) -> Vec<Uuid> {
    add_route_on(b, "copper_top", b.net, points)
}

fn add_route_on(b: &mut Board, layer: &str, net: Uuid, points: &[(f64, f64)]) -> Vec<Uuid> {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    let out = run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer": layer, "points": points, "net": net}),
    );
    out["result"]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u.as_str().unwrap().parse().unwrap())
        .collect()
}

fn add_via(b: &mut Board, at: (f64, f64)) -> Uuid {
    run(
        &mut b.shell,
        "pcb.via.add",
        json!({"at": {"x": format!("{}mm", at.0), "y": format!("{}mm", at.1)}, "net": b.net}),
    )["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn line(b: &Board, track: Uuid) -> (Point, Point) {
    b.shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .track(track)
        .unwrap()
        .geometry
        .ends()
}

fn last_entry(b: &Board) -> (String, Value) {
    let e = b.shell.engine().journal().last().unwrap();
    (e.id.clone(), e.params.clone())
}

fn journal_len(b: &Board) -> usize {
    b.shell.engine().journal().len()
}

fn drag(b: &mut Board, from: Point, to: Point, modifiers: Modifiers) {
    b.shell.pcb_press_with_modifiers(from, modifiers, TOLERANCE);
    b.shell.pcb_drag(to);
}

#[test]
fn a_slide_in_stop_stops_at_the_last_clear_position_and_ignore_goes_through() {
    let mut b = board();
    let other = run(&mut b.shell, "test.net.add", json!({"name": "OTHER"}))["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let t = add_route(&mut b, &[(0.0, 0.0), (0.0, 5.0), (10.0, 5.0), (10.0, 0.0)]);
    add_route_on(&mut b, "copper_top", other, &[(-5.0, 8.0), (15.0, 8.0)]);
    assert_eq!(
        b.shell.preferences().pcb.route.obstacle_mode,
        reda_ui::engine::pcb::route::ObstacleMode::Stop,
        "stop by default, shared with routing"
    );

    drag(&mut b, mm(3.0, 5.0), mm(3.0, 10.0), Modifiers::NONE);
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.drag");
    assert_eq!(params["obstacle_mode"], json!("stop"));
    let (a, _) = line(&b, t[1]);
    assert!(
        a.y > mm(0.0, 7.0).y && a.y < mm(0.0, 7.6).y,
        "stopped short of OTHER at y = 8 mm: {a:?}"
    );

    // Shift+R in flight steps the obstacle mode (stop -> ignore), and the slide goes on.
    let start = a;
    drag(
        &mut b,
        mm(3.0, start.y.0 as f64 / 1e6),
        mm(3.0, 12.0),
        Modifiers::NONE,
    );
    run(
        &mut b.shell,
        "pcb.drag.mode",
        json!({"mode": "obstacle_mode"}),
    );
    assert_eq!(
        b.shell.preferences().pcb.route.obstacle_mode,
        reda_ui::engine::pcb::route::ObstacleMode::Ignore
    );
    b.shell.pcb_release();
    let (_, params) = last_entry(&b);
    assert_eq!(params["obstacle_mode"], json!("ignore"));
    // Through OTHER at y = 8 mm (the drag keeps the off-grid offset the stop left).
    assert!(line(&b, t[1]).0.y > mm(0.0, 11.5).y);
}

#[test]
fn c_and_r_in_flight_turn_miters_and_merge_collinear_off_for_the_command() {
    let mut b = board();
    // A segment between two Vias: its ends can't follow, so connectors are added.
    add_via(&mut b, (0.0, 0.0));
    add_via(&mut b, (10.0, 0.0));
    let t = add_route(&mut b, &[(0.0, 0.0), (10.0, 0.0)]);
    drag(&mut b, mm(3.0, 0.0), mm(3.0, 3.0), Modifiers::NONE);
    assert!(
        b.shell.pcb_tool().copper_drag().is_some(),
        "a drag is in flight"
    );
    assert_eq!(
        reda_ui::keymap::active(&b.shell).tool.as_deref(),
        Some("drag"),
        "the drag keys apply"
    );
    run(&mut b.shell, "pcb.drag.mode", json!({"mode": "miter"}));
    run(
        &mut b.shell,
        "pcb.drag.mode",
        json!({"mode": "merge_collinear"}),
    );
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.drag");
    assert_eq!(params["miter"], json!(false));
    assert_eq!(params["merge_collinear"], json!(false));
    assert_eq!(line(&b, t[0]), (mm(0.0, 3.0), mm(10.0, 3.0)));
    // Square connectors: straight down to each Via.
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    let mut others: Vec<(Point, Point)> = pcb
        .tracks()
        .filter(|x| x.uuid != t[0])
        .map(|x| x.geometry.ends())
        .collect();
    others.sort_by_key(|(p, _)| p.x);
    assert_eq!(
        others,
        [(mm(0.0, 0.0), mm(0.0, 3.0)), (mm(10.0, 0.0), mm(10.0, 3.0))]
    );
    // Without a drag in flight the keys are refused.
    assert!(
        b.shell
            .run("pcb.drag.mode", json!({"mode": "miter"}))
            .is_err()
    );
}

#[test]
fn dragging_a_corner_deforms_and_space_scales_it() {
    let mut b = board();
    let t = add_route(&mut b, &[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)]);
    // Deform (the default): the shared vertex follows the cursor.
    drag(&mut b, mm(5.0, 0.0), mm(6.0, -1.0), Modifiers::NONE);
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.drag_end");
    assert_eq!(params["corner"], json!("deform"));
    assert_eq!(line(&b, t[0]), (mm(0.0, 0.0), mm(6.0, -1.0)));
    assert_eq!(line(&b, t[1]), (mm(6.0, -1.0), mm(5.0, 5.0)));

    // Space while dragging: Scale keeps the corner's shape.
    let mut b = board();
    let t = add_route(&mut b, &[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)]);
    drag(&mut b, mm(5.0, 0.0), mm(6.0, -1.0), Modifiers::NONE);
    run(&mut b.shell, "pcb.drag.mode", json!({"mode": "corner"}));
    b.shell.pcb_release();
    let (_, params) = last_entry(&b);
    assert_eq!(params["corner"], json!("scale"));
    let (a, c) = line(&b, t[0]);
    let (c2, d) = line(&b, t[1]);
    assert_eq!(c, c2, "still one corner");
    assert_eq!(a.y, c.y, "the first segment stays horizontal");
    assert_eq!(c2.x, d.x, "the second stays vertical");
}

#[test]
fn a_free_track_end_moves_alone() {
    let mut b = board();
    let t = add_route(&mut b, &[(0.0, 0.0), (5.0, 0.0)]);
    drag(&mut b, mm(5.0, 0.0), mm(7.0, 2.0), Modifiers::NONE);
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.drag_end");
    assert_eq!(params["end"], json!("to"));
    assert_eq!(line(&b, t[0]), (mm(0.0, 0.0), mm(7.0, 2.0)));
}

#[test]
fn the_midpoint_handle_of_a_selected_segment_breaks_it_and_drags_the_new_vertex() {
    let mut b = board();
    let t = add_route(&mut b, &[(0.0, 0.0), (10.0, 0.0)]);
    run(&mut b.shell, "selection.set", json!({"objects": [t[0]]}));
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    assert_eq!(
        reda_ui::pcb::copper_drag::handles(&b.shell, pcb),
        [mm(5.0, 0.0)]
    );
    let before = journal_len(&b);
    drag(&mut b, mm(5.0, 0.0), mm(5.0, 2.0), Modifiers::NONE);
    assert_eq!(journal_len(&b), before, "nothing commits in flight");
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.break");
    assert!(params["by"].is_object());
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    let mut lines: Vec<(Point, Point)> = pcb.tracks().map(|x| x.geometry.ends()).collect();
    lines.sort_by_key(|(p, _)| p.x);
    assert_eq!(
        lines,
        [(mm(0.0, 0.0), mm(5.0, 2.0)), (mm(5.0, 2.0), mm(10.0, 0.0))]
    );
}

#[test]
fn mod_shift_press_breaks_a_segment_at_the_point() {
    let mut b = board();
    add_route(&mut b, &[(0.0, 0.0), (10.0, 0.0)]);
    let both = Modifiers::COMMAND | Modifiers::SHIFT;
    // A click breaks it there.
    b.shell
        .pcb_press_with_modifiers(mm(3.0, 0.05), both, TOLERANCE);
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.break");
    assert_eq!(params["at"], json!({"x": 3_000_000, "y": 0}));
    assert_eq!(
        b.shell
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .tracks()
            .count(),
        2
    );
    // A drag breaks and drags the new vertex.
    drag(&mut b, mm(7.0, 0.0), mm(7.0, -2.0), both);
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.break");
    assert_eq!(params["at"], json!({"x": 7_000_000, "y": 0}));
    assert_eq!(params["by"], json!({"x": 0, "y": -2_000_000}));
}

#[test]
fn a_via_or_a_lone_free_pad_drags_with_its_tracks_attached() {
    let mut b = board();
    let v = add_via(&mut b, (5.0, 5.0));
    let t = add_route(&mut b, &[(0.0, 5.0), (5.0, 5.0)]);
    drag(&mut b, mm(5.0, 5.0), mm(5.0, 7.0), Modifiers::NONE);
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.copper.drag");
    assert_eq!(params["object"], json!(v));
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.via(v).unwrap().at, mm(5.0, 7.0));
    let ends: Vec<Point> = pcb
        .tracks()
        .flat_map(|x| {
            let (a, b) = x.geometry.ends();
            [a, b]
        })
        .collect();
    assert!(ends.contains(&mm(5.0, 7.0)), "still attached: {ends:?}");
    let _ = t;

    let pad = run(
        &mut b.shell,
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": "20mm", "y": "0mm"}, "width": "1mm", "net": b.net}),
    )["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    add_route(&mut b, &[(20.0, 0.0), (25.0, 0.0)]);
    drag(&mut b, mm(20.0, 0.0), mm(20.0, 3.0), Modifiers::NONE);
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.copper.drag");
    assert_eq!(params["object"], json!(pad));
}

#[test]
fn tab_widens_a_copper_selection_and_comes_back_to_the_start() {
    let mut b = board();
    // Layer: (0,0)-(5,0)-(5,5) on top, a Via at (5,5), then bottom to (10,5); another
    // Net-SIG Track far away.
    let top = add_route(&mut b, &[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)]);
    let v = add_via(&mut b, (5.0, 5.0));
    let net = b.net;
    let bottom = add_route_on(&mut b, "copper_bottom", net, &[(5.0, 5.0), (10.0, 5.0)]);
    let far = add_route(&mut b, &[(30.0, 0.0), (35.0, 0.0)]);
    run(&mut b.shell, "selection.set", json!({"objects": [top[0]]}));
    let sorted = |mut v: Vec<Uuid>| {
        v.sort();
        v
    };
    let step = |b: &mut Board| -> (String, Vec<Uuid>) {
        let out = run(&mut b.shell, "pcb.selection.widen", json!({}));
        (
            out["step"].as_str().unwrap().to_owned(),
            sorted(b.shell.selection().objects.clone()),
        )
    };
    assert_eq!(step(&mut b), ("layer".into(), sorted(top.clone())));
    let mut connected = top.clone();
    connected.extend([v, bottom[0]]);
    assert_eq!(step(&mut b), ("copper".into(), sorted(connected.clone())));
    let mut net = connected.clone();
    net.extend(far.iter().copied());
    assert_eq!(step(&mut b), ("net".into(), sorted(net)));
    assert_eq!(step(&mut b), ("start".into(), vec![top[0]]));

    // A new selection starts a new cycle; U selects connected copper directly.
    run(&mut b.shell, "selection.set", json!({"objects": [far[0]]}));
    assert_eq!(step(&mut b).0, "layer");
    run(
        &mut b.shell,
        "selection.set",
        json!({"objects": [bottom[0]]}),
    );
    run(&mut b.shell, "pcb.selection.connected", json!({}));
    assert_eq!(
        sorted(b.shell.selection().objects.clone()),
        sorted(connected)
    );
    run(
        &mut b.shell,
        "pcb.selection.connected",
        json!({"seeds": [far[0]], "scope": "layer"}),
    );
    assert!(b.shell.selection().objects.contains(&far[0]));

    // Not with a Tool running.
    run(&mut b.shell, "tool.activate", json!({"tool": "route"}));
    assert!(b.shell.run("pcb.selection.widen", json!({})).is_err());
}

#[test]
fn unwinding_selects_the_neighbour_and_both_ways_selects_nothing() {
    let mut b = board();
    let t = add_route(&mut b, &[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (10.0, 5.0)]);
    run(&mut b.shell, "selection.set", json!({"objects": [t[2]]}));
    run(
        &mut b.shell,
        "pcb.copper.unwind",
        json!({"objects": [t[2]]}),
    );
    assert_eq!(b.shell.selection().objects, [t[1]]);
    run(
        &mut b.shell,
        "pcb.copper.unwind",
        json!({"objects": [t[1]], "both": true}),
    );
    assert!(b.shell.selection().objects.is_empty());
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.tracks().count(), 0);
}

// ---- In the window: the keys ----

struct Window {
    h: Harness<'static, App>,
    pcb: Uuid,
    net: Uuid,
    _home: TempDir,
    _work: TempDir,
}

fn window() -> Window {
    let home = TempDir::new("pcb-copper-canvas-home");
    let work = TempDir::new("pcb-copper-canvas-work");
    let (mut shell, net) = shell_with_board(&home, &work);
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(
        &mut shell,
        "layout.tab.activate",
        json!({"tab": pcb.to_string()}),
    );
    let h = Harness::builder()
        .with_size([1000.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    Window {
        h,
        pcb,
        net,
        _home: home,
        _work: work,
    }
}

fn wshell(w: &Window) -> &Shell {
    w.h.state().shell()
}

fn wrun(w: &mut Window, id: &str, params: Value) -> Value {
    let out = w.h.state_mut().shell_mut().run(id, params).unwrap();
    w.h.run();
    out
}

fn on_screen(w: &Window, p: Point) -> Pos2 {
    let rect = w.h.get_by_role(Role::Canvas).rect();
    let camera = wshell(w).document_tab(w.pcb).unwrap().camera.unwrap();
    camera.screen_point(rect, (p.x.0 as f64, p.y.0 as f64))
}

fn last_command(w: &Window) -> (String, Value) {
    let e = wshell(w).engine().journal().last().unwrap();
    (e.id.clone(), e.params.clone())
}

fn wroute(w: &mut Window, points: &[(f64, f64)]) -> Vec<Uuid> {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    let net = w.net;
    let out = wrun(
        w,
        "pcb.track.add",
        json!({"layer": "copper_top", "points": points, "net": net}),
    );
    out["result"]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u.as_str().unwrap().parse().unwrap())
        .collect()
}

#[test]
fn in_the_window_c_while_dragging_turns_miters_off_and_tab_and_backspace_edit_the_selection() {
    let mut w = window();
    w.h.run();
    let t = wroute(&mut w, &[(0.0, 0.0), (0.0, 5.0), (10.0, 5.0), (10.0, 0.0)]);
    // A slide with C pressed in flight.
    let grab = on_screen(&w, mm(5.0, 5.0));
    let to = on_screen(&w, mm(5.0, 8.0));
    w.h.drag_at(grab);
    w.h.run();
    w.h.hover_at(to);
    w.h.run();
    w.h.key_press(Key::C);
    w.h.run();
    w.h.drop_at(to);
    w.h.run();
    let (id, params) = last_command(&w);
    assert_eq!(id, "pcb.track.drag");
    assert_eq!(params["miter"], json!(false));
    assert_eq!(
        wshell(&w).selection().objects,
        [t[1]],
        "the dragged segment is selected"
    );
    // Tab widens to the contiguous Track.
    w.h.key_press(Key::Tab);
    w.h.run();
    assert_eq!(last_command(&w).0, "pcb.selection.widen");
    assert_eq!(wshell(&w).selection().objects.len(), 3);
    assert_eq!(
        w.h.ctx.memory(|m| m.focused()),
        None,
        "Tab on the board widens instead of moving the keyboard focus"
    );
    // Back to one end segment; Backspace unwinds it and selects its neighbour.
    wrun(&mut w, "selection.set", json!({"objects": [t[2]]}));
    let before = wshell(&w).engine().journal().len();
    w.h.key_press(Key::Backspace);
    w.h.run();
    let ids: Vec<String> = wshell(&w).engine().journal()[before..]
        .iter()
        .map(|e| e.id.clone())
        .collect();
    assert_eq!(ids, ["pcb.copper.unwind", "selection.set"]);
    assert_eq!(wshell(&w).selection().objects, [t[1]]);
    // Mod+Del unwinds both ways.
    let before = wshell(&w).engine().journal().len();
    w.h.key_press_modifiers(Modifiers::COMMAND, Key::Delete);
    w.h.run();
    let entry = wshell(&w).engine().journal()[before].clone();
    let (id, params) = (entry.id, entry.params);
    assert_eq!(id, "pcb.copper.unwind");
    assert_eq!(params["both"], json!(true));
    assert_eq!(
        wshell(&w)
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .tracks()
            .count(),
        0
    );
}

// ---- Track Properties and the Net block ----

fn tall_window() -> Window {
    let home = TempDir::new("pcb-track-props-home");
    let work = TempDir::new("pcb-track-props-work");
    let (mut shell, net) = shell_with_board(&home, &work);
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(
        &mut shell,
        "layout.tab.activate",
        json!({"tab": pcb.to_string()}),
    );
    let h = Harness::builder()
        .with_size([1400.0, 2600.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    Window {
        h,
        pcb,
        net,
        _home: home,
        _work: work,
    }
}

fn type_into(w: &mut Window, label: &str, text: &str) {
    w.h.get_by_role_and_label(Role::TextInput, label).focus();
    w.h.run();
    w.h.get_by_role_and_label(Role::TextInput, label)
        .set_text_value_accesskit(text);
    w.h.run();
    w.h.key_press(Key::Enter);
    w.h.run();
}

#[test]
fn track_properties_edit_start_end_length_layer_and_net_with_a_canvas_pick() {
    let mut w = tall_window();
    w.h.run();
    let t = wroute(&mut w, &[(0.0, 0.0), (10.0, 0.0)])[0];
    wrun(&mut w, "selection.set", json!({"objects": [t]}));
    w.h.run();
    w.h.get_by_label("Track");

    type_into(&mut w, "Start Y", "1");
    let (id, params) = last_command(&w);
    assert_eq!(id, "pcb.track.set");
    assert_eq!(params["start"], json!({"x": 0, "y": 1_000_000}));
    type_into(&mut w, "End X", "12mm");
    let (_, params) = last_command(&w);
    assert_eq!(params["end"], json!({"x": 12_000_000, "y": 0}));
    // Length keeps Start and moves End along the line.
    type_into(&mut w, "Length", "5");
    let (id, params) = last_command(&w);
    assert_eq!(id, "pcb.track.set");
    assert!(params["length"].is_number());
    let pcb_track = |w: &Window| {
        wshell(w)
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .track(t)
            .unwrap()
            .clone()
    };
    let (a, b) = pcb_track(&w).geometry.ends();
    assert_eq!(a, mm(0.0, 1.0));
    let len = ((b.x.0 - a.x.0) as f64).hypot((b.y.0 - a.y.0) as f64);
    assert!((len - 5e6).abs() < 2.0, "{len}");

    // Layer.
    w.h.get_by_role_and_label(Role::ComboBox, "Layer").click();
    w.h.run();
    let bottom = "Bottom Copper";
    w.h.get_all_by_label(bottom).last().unwrap().click();
    w.h.run();
    let (id, params) = last_command(&w);
    assert_eq!(id, "pcb.track.set");
    assert!(params["layer"].is_string());
    assert_ne!(
        pcb_track(&w).layer,
        wshell(&w)
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .copper_layers()
            .next()
            .unwrap()
            .uuid
    );

    // Net: Pick, then a click on copper of another Net gives the Track that Net.
    let gnd: Uuid = wrun(&mut w, "test.net.add", json!({"name": "GND"}))["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    wrun(
        &mut w,
        "pcb.track.add",
        json!({"layer": "copper_top", "net": gnd,
               "points": [{"x": "0mm", "y": "-10mm"}, {"x": "10mm", "y": "-10mm"}]}),
    );
    wrun(&mut w, "selection.set", json!({"objects": [t]}));
    w.h.get_by_label("Pick").click();
    w.h.run();
    assert_eq!(wshell(&w).net_pick(), [t]);
    let at = on_screen(&w, mm(5.0, -10.0));
    w.h.hover_at(at);
    w.h.run();
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
    let (id, params) = last_command(&w);
    assert_eq!(id, "pcb.track.set");
    assert_eq!(params["net"], json!(gnd));
    assert_eq!(pcb_track(&w).net, Some(gnd));
    assert!(wshell(&w).net_pick().is_empty());
    // Esc drops a waiting pick.
    wrun(&mut w, "view.properties.pick_net", json!({"tracks": [t]}));
    assert_eq!(wshell(&w).net_pick(), [t]);
    w.h.key_press(Key::Escape);
    w.h.run();
    assert!(wshell(&w).net_pick().is_empty());
}

#[test]
fn the_net_block_shows_loop_removal_the_routed_length_and_the_current_estimate() {
    let mut w = tall_window();
    w.h.run();
    let t = wroute(&mut w, &[(0.0, 0.0), (10.0, 0.0)])[0];
    wrun(&mut w, "selection.set", json!({"objects": [t]}));
    w.h.run();
    w.h.get_by_label("SIG");
    w.h.get_by_label_contains("(estimate)");
    w.h.get_by_label("10 mm");

    let loop_removal = w.h.get_by_role_and_label(Role::CheckBox, "Loop removal");
    loop_removal.click();
    w.h.run();
    let (id, params) = last_command(&w);
    assert_eq!(id, "pcb.net.set_routing");
    assert_eq!(params["loop_removal"], json!(false));

    let before =
        w.h.get_by_label_contains("(estimate)")
            .accesskit_node()
            .value()
            .unwrap_or_default()
            .to_string();
    type_into(&mut w, "Temperature rise ΔT", "20");
    let (id, params) = last_command(&w);
    assert_eq!(id, "app.preferences.set");
    assert_eq!(params["key"], json!("pcb.net_info.delta_t"));
    assert_eq!(wshell(&w).preferences().pcb.net_info.delta_t, 20.0);
    w.h.run();
    let after =
        w.h.get_by_label_contains("(estimate)")
            .accesskit_node()
            .value()
            .unwrap_or_default()
            .to_string();
    assert_ne!(before, after, "a larger ΔT allows more current");
}

#[test]
fn the_route_menu_unroutes_asking_about_locked_copper_and_cleans_up() {
    let mut w = tall_window();
    w.h.run();
    let t = wroute(&mut w, &[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)]);
    wrun(
        &mut w,
        "pcb.lock",
        json!({"objects": [t[0]], "locked": true}),
    );
    w.h.get_by_label("Route").click();
    w.h.run();
    w.h.get_by_label("Unroute All").click();
    w.h.run();
    w.h.run();
    let (id, params) = last_command(&w);
    assert_eq!(id, "pcb.route.unroute");
    assert_eq!(params, json!({"all": true}));
    let count = |w: &Window| {
        wshell(w)
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .tracks()
            .count()
    };
    assert_eq!(count(&w), 1, "the locked one stays");
    w.h.get_by_label("Unroute them").click();
    w.h.run();
    w.h.run();
    let (id, params) = last_command(&w);
    assert_eq!(id, "pcb.route.unroute");
    assert_eq!(params["include_locked"], json!(true));
    assert_eq!(count(&w), 0);

    // Clean Up Tracks merges collinear segments.
    wroute(&mut w, &[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)]);
    w.h.get_by_label("Route").click();
    w.h.run();
    w.h.get_by_label("Clean Up Tracks").click();
    w.h.run();
    w.h.run();
    assert_eq!(last_command(&w).0, "pcb.track.cleanup");
    assert_eq!(count(&w), 1);
}
