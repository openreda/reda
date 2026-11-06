//! The routing Tool, the Via Tool and Track editing through the Shell with no window: T
//! starts routing, `/` cycles the corner mode, the width comes from the Net Class and can
//! change mid-route, the live clearance check flags and stops the route, V drops a Via
//! and switches Layer, a route ends on its own on same-Net copper and commits as one
//! Command (or one `batch`), a bridged edit cancels it, and segments drag, delete and
//! select connected.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, Signals, UserDirs};
use reda_ui::engine::journal::Source;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::model::{LayerKind, RuleKind, Side, TrackGeometry, Uuid};
use reda_ui::pcb::route::Corner;
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

const TOLERANCE: Nm = Nm::from_um(200);

struct Board {
    shell: Shell,
    pcb: Uuid,
    net: Uuid,
    footprints: [Uuid; 2],
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
    let home = TempDir::new("pcb-route-home");
    let work = TempDir::new("pcb-route-work");
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
    let footprints = [
        add_pad(&mut shell, "0mm", "0mm", &net),
        add_pad(&mut shell, "10mm", "0mm", &net),
    ];
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    run(
        &mut shell,
        "view.grid.set",
        json!({"document": pcb, "grid": "0.5mm"}),
    );
    Board {
        shell,
        pcb,
        net: net.as_str().unwrap().parse().unwrap(),
        footprints,
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

fn tracks(b: &Board) -> Vec<(Uuid, Option<Uuid>, Nm, Point, Point)> {
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    pcb.tracks()
        .map(|t| {
            let (from, to) = t.geometry.ends();
            (t.layer, t.net, t.width, from, to)
        })
        .collect()
}

fn copper(b: &Board, side: Side) -> Uuid {
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    pcb.layers()
        .find(LayerKind::Copper, Some(side))
        .unwrap()
        .uuid
}

fn route_to(b: &mut Board, at: Point) {
    b.shell.route_hover(at);
    b.shell.route_click(at);
}

#[test]
fn t_starts_routing_and_reaching_a_same_net_pad_commits_one_track_add() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:route", json!({}));
    assert_eq!(b.shell.tool(), Tool::Route);
    let before = journal_ids(&b).len();

    route_to(&mut b, mm(0.0, 0.0));
    route_to(&mut b, mm(5.0, 0.0));
    b.shell.route_hover(mm(10.0, 0.2));
    let preview = b.shell.route_preview().expect("routing");
    assert_eq!(preview.net, Some(b.net));
    assert_eq!(
        *preview.ahead.last().unwrap(),
        mm(10.0, 0.0),
        "snaps to the Pad"
    );
    assert!(
        journal_ids(&b)[before..]
            .iter()
            .all(|id| id != "pcb.track.add"),
        "nothing commits in flight"
    );

    b.shell.route_click(mm(10.0, 0.2));
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.add", "the route ends on its own");
    assert_eq!(params["points"].as_array().unwrap().len(), 3);
    assert!(b.shell.route_preview().is_none());
    let rule = b
        .shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .board_rule(RuleKind::TrackWidth)
        .values
        .preferred
        .unwrap();
    let top = copper(&b, Side::Top);
    assert_eq!(
        tracks(&b),
        [
            (top, Some(b.net), rule, mm(0.0, 0.0), mm(5.0, 0.0)),
            (top, Some(b.net), rule, mm(5.0, 0.0), mm(10.0, 0.0)),
        ]
    );
    assert_eq!(
        b.shell.tool(),
        Tool::Route,
        "the Tool stays for the next route"
    );
}

#[test]
fn slash_cycles_the_corner_mode() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    let ahead = |b: &mut Board| {
        b.shell.route_hover(mm(3.0, 1.0));
        b.shell.route_preview().unwrap().ahead
    };
    assert_eq!(b.shell.router().corner(), Corner::Straight45);
    assert_eq!(ahead(&mut b), [mm(0.0, 0.0), mm(2.0, 0.0), mm(3.0, 1.0)]);
    run(&mut b.shell, "tool.cycle_corner", json!({}));
    assert_eq!(ahead(&mut b), [mm(0.0, 0.0), mm(1.0, 1.0), mm(3.0, 1.0)]);
    run(&mut b.shell, "tool.cycle_corner", json!({}));
    assert_eq!(ahead(&mut b), [mm(0.0, 0.0), mm(3.0, 0.0), mm(3.0, 1.0)]);
    run(&mut b.shell, "tool.cycle_corner", json!({}));
    assert_eq!(ahead(&mut b), [mm(0.0, 0.0), mm(0.0, 1.0), mm(3.0, 1.0)]);
    run(&mut b.shell, "tool.cycle_corner", json!({}));
    assert_eq!(b.shell.router().corner(), Corner::Straight45);
}

#[test]
fn the_width_comes_from_the_net_class_and_changes_mid_route() {
    let mut b = board();
    let class = run(
        &mut b.shell,
        "pcb.net_class.add",
        json!({"name": "Power", "rules": [{"kind": "track_width", "values": {"preferred": "0.5mm"}}]}),
    )["result"]["uuid"]
        .clone();
    run(
        &mut b.shell,
        "pcb.net.add_to_class",
        json!({"nets": [b.net], "class": class}),
    );
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    assert_eq!(b.shell.route_preview().unwrap().width, Nm::from_um(500));
    route_to(&mut b, mm(5.0, 0.0));

    run(&mut b.shell, "pcb.route.width", json!({"width": "0.3mm"}));
    assert_eq!(b.shell.route_preview().unwrap().width, Nm::from_um(300));
    route_to(&mut b, mm(10.0, 0.0));

    let (id, params) = last_entry(&b);
    assert_eq!(id, "batch", "two widths: one batch, one undo entry");
    let steps = params["commands"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    let widths: Vec<Nm> = tracks(&b).iter().map(|t| t.2).collect();
    assert_eq!(widths, [Nm::from_um(500), Nm::from_um(300)]);

    run(&mut b.shell, "edit.undo", json!({"document": b.pcb}));
    assert!(tracks(&b).is_empty(), "one undo removes the whole route");
}

#[test]
fn the_width_steps_through_the_boards_widths() {
    let mut b = board();
    run(
        &mut b.shell,
        "pcb.net_class.add",
        json!({"name": "Power", "rules": [{"kind": "track_width", "values": {"preferred": "1mm"}}]}),
    );
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    let rule = b
        .shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .board_rule(RuleKind::TrackWidth)
        .values
        .preferred
        .unwrap();
    assert_eq!(b.shell.route_preview().unwrap().width, rule);
    run(&mut b.shell, "pcb.route.width:next", json!({}));
    assert_eq!(b.shell.route_preview().unwrap().width, Nm::from_mm(1));
    run(&mut b.shell, "pcb.route.width:previous", json!({}));
    assert_eq!(b.shell.route_preview().unwrap().width, rule);
}

#[test]
fn a_clearance_violation_flags_the_route_and_stops_it() {
    let mut b = board();
    let other =
        run(&mut b.shell, "test.net.add", json!({"name": "OTHER"}))["result"]["uuid"].clone();
    add_pad(&mut b.shell, "5mm", "0mm", &other);
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));

    b.shell.route_hover(mm(3.0, 3.0));
    assert!(!b.shell.route_preview().unwrap().blocked, "clear of OTHER");
    b.shell.route_hover(mm(8.0, 0.0));
    let preview = b.shell.route_preview().unwrap();
    assert!(preview.blocked, "through OTHER's Pad");

    b.shell.route_click(mm(8.0, 0.0));
    let preview = b.shell.route_preview().expect("still routing");
    assert_eq!(preview.placed, [mm(0.0, 0.0)], "the corner is not placed");
    assert!(b.shell.message().is_some(), "says why");
    assert!(tracks(&b).is_empty(), "nothing shoves, nothing commits");
}

#[test]
fn v_drops_a_via_mid_route_switches_layer_and_the_route_commits_as_one_batch() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    b.shell.route_hover(mm(4.0, 0.0));
    run(&mut b.shell, "pcb.route.add_via", json!({}));
    let bottom = copper(&b, Side::Bottom);
    let preview = b.shell.route_preview().unwrap();
    assert_eq!(preview.layer, bottom);
    let project = b.shell.engine().project().unwrap();
    assert_eq!(b.shell.board_view().active_layer(project), Some(bottom));

    route_to(&mut b, mm(4.0, 5.0));
    run(&mut b.shell, "pcb.route.finish", json!({}));
    let (id, params) = last_entry(&b);
    assert_eq!(id, "batch");
    let steps: Vec<&str> = params["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(steps, ["pcb.track.add", "pcb.via.add", "pcb.track.add"]);
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    let via = pcb.vias().next().unwrap();
    assert_eq!(via.at, mm(4.0, 0.0));
    assert_eq!(via.net, Some(b.net));
    assert_eq!(
        tracks(&b),
        [
            (
                copper(&b, Side::Top),
                Some(b.net),
                Nm::from_um(250),
                mm(0.0, 0.0),
                mm(4.0, 0.0)
            ),
            (
                bottom,
                Some(b.net),
                Nm::from_um(250),
                mm(4.0, 0.0),
                mm(4.0, 5.0)
            ),
        ]
    );
}

#[test]
fn backspace_takes_back_the_last_corner_and_escape_drops_the_route() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    route_to(&mut b, mm(3.0, 0.0));
    route_to(&mut b, mm(3.0, 3.0));
    run(&mut b.shell, "tool.undo_segment", json!({}));
    assert_eq!(
        b.shell.route_preview().unwrap().placed,
        [mm(0.0, 0.0), mm(3.0, 0.0)]
    );
    run(&mut b.shell, "tool.activate:select", json!({}));
    assert!(b.shell.route_preview().is_none());
    assert!(b.shell.tool_preview().is_none());
    assert!(tracks(&b).is_empty());
}

#[test]
fn a_bridged_edit_to_the_routes_copper_cancels_it() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    assert_eq!(
        b.shell.tool_preview().map(|t| t.title.clone()),
        Some("Route".to_owned())
    );
    b.shell
        .run_bridged(
            Source::Mcp,
            "pcb.move",
            json!({"objects": [b.footprints[0]], "by": {"x": "1mm", "y": 0}}),
            &Signals::default(),
        )
        .unwrap();
    assert!(b.shell.note().is_some());
    b.shell.route_hover(mm(5.0, 0.0));
    assert!(b.shell.route_preview().is_none(), "the route is dropped");
}

#[test]
fn the_via_tool_places_stitching_vias() {
    let mut b = board();
    run(&mut b.shell, "tool.activate:via", json!({}));
    b.shell.route_hover(mm(5.0, 5.0));
    b.shell.route_click(mm(5.0, 5.0));
    assert_eq!(last_entry(&b).0, "pcb.via.add");
    // 0.15 mm from SIG's Pad with no Net (0.16 mm needed): refused.
    let before = journal_ids(&b).len();
    b.shell.route_click(mm(1.0, 0.0));
    assert!(b.shell.route_preview().is_none_or(|p| p.vias.is_empty()));
    assert!(
        !journal_ids(&b)[before..]
            .iter()
            .any(|id| id == "pcb.via.add")
    );
    assert_eq!(
        b.shell
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .vias()
            .count(),
        1
    );
}

fn add_route(b: &mut Board, points: &[(f64, f64)]) -> Vec<Uuid> {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    let out = run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer": "copper_top", "points": points}),
    );
    out["result"]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u.as_str().unwrap().parse().unwrap())
        .collect()
}

fn geometry(b: &Board, track: Uuid) -> TrackGeometry {
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    pcb.track(track).unwrap().geometry
}

#[test]
fn dragging_a_segment_previews_then_runs_one_track_drag_keeping_neighbours() {
    let mut b = board();
    let t = add_route(&mut b, &[(0.0, 0.0), (0.0, 5.0), (10.0, 5.0), (10.0, 0.0)]);
    let before = journal_ids(&b).len();

    b.shell.pcb_press(mm(5.0, 5.0), false, TOLERANCE);
    b.shell.pcb_drag(mm(5.0, 6.0));
    b.shell.pcb_drag(mm(6.0, 7.0));
    assert!(
        !journal_ids(&b)[before..]
            .iter()
            .any(|id| id == "pcb.track.drag"),
        "nothing commits in flight"
    );
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.drag");
    assert_eq!(params["detach"], json!(false));
    assert_eq!(
        geometry(&b, t[1]),
        TrackGeometry::Line {
            from: mm(0.0, 7.0),
            to: mm(10.0, 7.0)
        },
        "moves across itself only"
    );
    assert_eq!(
        geometry(&b, t[0]),
        TrackGeometry::Line {
            from: mm(0.0, 0.0),
            to: mm(0.0, 7.0)
        }
    );

    // Mod+drag detaches.
    b.shell.pcb_press(mm(5.0, 7.0), true, TOLERANCE);
    b.shell.pcb_drag(mm(5.0, 9.0));
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.drag");
    assert_eq!(params["detach"], json!(true));
    assert_eq!(
        geometry(&b, t[0]),
        TrackGeometry::Line {
            from: mm(0.0, 0.0),
            to: mm(0.0, 7.0)
        }
    );
}

#[test]
fn select_connected_and_delete_work_on_the_selection() {
    let mut b = board();
    let t = add_route(&mut b, &[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)]);
    b.shell.pcb_press(mm(2.0, 0.0), false, TOLERANCE);
    b.shell.pcb_release();
    assert_eq!(b.shell.selection().objects, [t[0]]);

    run(&mut b.shell, "pcb.selection.connected", json!({}));
    let selection = b.shell.selection();
    assert!(selection.objects.contains(&t[0]) && selection.objects.contains(&t[1]));
    assert_eq!(selection.pads.len(), 2, "both Pads");

    run(
        &mut b.shell,
        "pcb.copper.delete",
        json!({"objects": [t[1]]}),
    );
    assert_eq!(tracks(&b).len(), 1);
    assert!(!b.shell.selection().objects.contains(&t[1]));
}
