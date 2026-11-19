//! The routing Tool, the Via Tool and Track editing through the Shell with no window: T
//! starts routing, the width comes from the Net Class and can
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

    // `stop`: the click places up to the last clear point, off the 0.5 mm grid.
    b.shell.route_click(mm(8.0, 0.0));
    let preview = b.shell.route_preview().expect("still routing");
    assert_eq!(preview.placed.len(), 2);
    let clip = preview.placed[1];
    assert_eq!(clip.y, Nm::ZERO);
    assert!(
        clip.x > mm(4.0, 0.0).x && clip.x < mm(4.5, 0.0).x,
        "{clip:?}"
    );
    assert_ne!(clip.x.0 % 500_000, 0, "kept off grid");
    // From there nothing more can be placed: the click says why.
    b.shell.route_click(mm(8.0, 0.0));
    assert_eq!(b.shell.route_preview().unwrap().placed.len(), 2);
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
fn undoing_a_via_at_the_route_start_restores_the_layer_width() {
    let mut b = board();
    let bottom = copper(&b, Side::Bottom);
    run(
        &mut b.shell,
        "pcb.rules.set",
        json!({
            "kind":"track_width","scope":{"layers":[bottom]},"values":{"preferred":"0.45mm"}
        }),
    );
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    run(&mut b.shell, "pcb.route.add_via", json!({}));
    assert_eq!(b.shell.route_preview().unwrap().width, Nm::from_um(450));
    run(&mut b.shell, "tool.undo_segment", json!({}));
    let preview = b.shell.route_preview().unwrap();
    assert_eq!(preview.layer, copper(&b, Side::Top));
    assert_eq!(preview.width, Nm::from_um(250));
    run(&mut b.shell, "pcb.route.width", json!({"width":"0.25mm"}));
    run(&mut b.shell, "pcb.route.add_via", json!({}));
    assert_eq!(b.shell.route_preview().unwrap().width, Nm::from_um(250));
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

    // Mod+drag detaches. (Off the midpoint: on the selected segment's midpoint handle a
    // drag breaks it.)
    b.shell.pcb_press(mm(3.0, 7.0), true, TOLERANCE);
    b.shell.pcb_drag(mm(3.0, 9.0));
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
fn the_drag_passes_pcb_drag_preserve_angle_to_the_command() {
    let mut b = board();
    let t = add_route(&mut b, &[(0.0, 0.0), (0.0, 5.0), (10.0, 5.0), (10.0, 0.0)]);

    // On (the default): a plain drag keeps the neighbours upright.
    b.shell.pcb_press(mm(5.0, 5.0), false, TOLERANCE);
    b.shell.pcb_drag(mm(7.0, 7.0));
    b.shell.pcb_release();
    let (_, params) = last_entry(&b);
    assert_eq!(params["preserve_angle"], json!(true));
    assert_eq!(
        geometry(&b, t[0]),
        TrackGeometry::Line {
            from: mm(0.0, 0.0),
            to: mm(0.0, 7.0)
        }
    );

    // Off: the segment moves by the whole drag and its neighbours' ends follow.
    b.shell
        .run(
            "app.preferences.set",
            json!({"key": "pcb.drag.preserve_angle", "value": false}),
        )
        .unwrap();
    b.shell.pcb_press(mm(3.0, 7.0), false, TOLERANCE);
    b.shell.pcb_drag(mm(5.0, 8.0));
    b.shell.pcb_release();
    let (id, params) = last_entry(&b);
    assert_eq!(id, "pcb.track.drag");
    assert_eq!(params["preserve_angle"], json!(false));
    assert_eq!(
        geometry(&b, t[1]),
        TrackGeometry::Line {
            from: mm(2.0, 8.0),
            to: mm(12.0, 8.0)
        }
    );
    assert_eq!(
        geometry(&b, t[0]),
        TrackGeometry::Line {
            from: mm(0.0, 0.0),
            to: mm(2.0, 8.0)
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

#[test]
fn via_placement_uses_the_output_origins_grid_phase() {
    let mut b = board();
    run(
        &mut b.shell,
        "pcb.output_origin.set",
        json!({"at":{"x":"0.2mm","y":"0.3mm"}}),
    );
    run(&mut b.shell, "tool.activate:via", json!({}));
    route_to(&mut b, mm(5.1, 5.1));
    assert_eq!(
        b.shell
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .vias()
            .next()
            .unwrap()
            .at,
        mm(5.2, 5.3)
    );
}

#[test]
fn a_reroute_deletes_the_old_path_in_the_routes_one_batch() {
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
    b.shell.route_hover(mm(10.0, 0.0));
    let proposed = b.shell.route_preview().unwrap().proposed.unwrap();
    assert!(proposed.ends_route);
    assert_eq!(proposed.removes.removed.len(), 3, "the red preview");

    let before = journal_ids(&b).len();
    b.shell.route_click(mm(10.0, 0.0));
    assert_eq!(journal_ids(&b).len(), before + 1, "one Journal entry");
    let (id, params) = last_entry(&b);
    assert_eq!(id, "batch");
    assert_eq!(params["commands"][0]["id"], "pcb.copper.delete");
    let top = copper(&b, Side::Top);
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
    assert_eq!(
        tracks(&b),
        [(top, Some(net), rule, mm(0.0, 0.0), mm(10.0, 0.0))]
    );
}

#[test]
fn the_route_preferences_drive_the_tool() {
    let mut b = board();
    let other =
        run(&mut b.shell, "test.net.add", json!({"name": "OTHER"}))["result"]["uuid"].clone();
    add_pad(&mut b.shell, "5mm", "3mm", &other);
    let net = b.net;
    run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer": "copper_top", "net": net, "width": "1mm",
               "points": [{"x": 0, "y": "10mm"}, {"x": "10mm", "y": "10mm"}]}),
    );
    let set = |b: &mut Board, key: &str, value: Value| {
        b.shell
            .run("app.preferences.set", json!({"key": key, "value": value}))
            .unwrap();
    };
    run(&mut b.shell, "tool.activate:route", json!({}));

    // `pickup_width`: a route started on a Track takes its width, unless turned off.
    route_to(&mut b, mm(5.0, 10.0));
    assert_eq!(b.shell.route_preview().unwrap().width, Nm::from_mm(1));
    run(&mut b.shell, "tool.activate:select", json!({}));
    run(&mut b.shell, "tool.activate:route", json!({}));
    set(&mut b, "pcb.route.pickup_width", json!(false));
    route_to(&mut b, mm(5.0, 10.0));
    assert_ne!(b.shell.route_preview().unwrap().width, Nm::from_mm(1));
    run(&mut b.shell, "tool.activate:select", json!({}));
    run(&mut b.shell, "tool.activate:route", json!({}));

    // `obstacle_mode`: `ignore` places the whole path through OTHER's Pad.
    set(&mut b, "pcb.route.obstacle_mode", json!("ignore"));
    route_to(&mut b, mm(0.0, 3.0));
    b.shell.route_hover(mm(8.0, 3.0));
    assert!(
        b.shell.route_preview().unwrap().blocked,
        "drawn in the violation colour"
    );
    b.shell.route_click(mm(8.0, 3.0));
    let preview = b.shell.route_preview().unwrap();
    assert_eq!(preview.placed, [mm(0.0, 3.0), mm(8.0, 3.0)]);
}

#[test]
fn stitching_uses_the_chosen_type_and_ignores_copper_beyond_its_span() {
    let mut b = board();
    run(&mut b.shell, "document.delete", json!({"document":b.pcb}));
    run(
        &mut b.shell,
        "document.add",
        json!({"kind":"pcb","layers":4}),
    );
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    let doc = pcb.uuid;
    let layers: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    run(&mut b.shell, "document.open", json!({"document":doc}));
    let kind=run(&mut b.shell,"pcb.via_type.add",json!({"document":doc,"from":layers[1],"to":layers[0],"micro":true,"diameter":"0.45mm","hole":"0.15mm"}))["result"]["uuid"].clone();
    run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer":layers[3],"points":[{"x":0,"y":0},{"x":"5mm","y":0}]}),
    );
    run(&mut b.shell, "tool.activate:via", json!({}));
    run(&mut b.shell, "pcb.via.pick_type", json!({"via_type":kind}));
    b.shell.route_hover(mm(2.0, 0.0));
    let cursor = b.shell.router().via_cursor().unwrap();
    assert_eq!(cursor.diameter, Nm(450000));
    assert!(!cursor.blocked);
    b.shell.route_click(mm(2.0, 0.0));
    let via = b
        .shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .vias()
        .next()
        .unwrap();
    assert_eq!(
        (via.span.from, via.span.to, via.hole),
        (layers[1], layers[0], Nm(150000))
    );
}

#[test]
fn a_route_starting_on_an_etched_graphic_keeps_its_clearance() {
    let mut b = board();
    run(
        &mut b.shell,
        "pcb.graphic.add_rect",
        json!({
            "layer":"copper_top", "from":{"x":"3mm","y":"3mm"},"to":{"x":"6mm","y":"6mm"},
            "filled":true, "width":0
        }),
    );
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(4.0, 4.0));
    b.shell.route_hover(mm(7.0, 4.0));
    assert!(b.shell.route_preview().unwrap().blocked);
}

#[test]
fn a_layer_jump_places_a_stack_as_one_route_step_and_one_edit() {
    let mut b = board();
    let top = copper(&b, Side::Top);
    let bottom = copper(&b, Side::Bottom);
    run(&mut b.shell, "pcb.layer.add", json!({"document":b.pcb}));
    let inner = b
        .shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .nth(1)
        .unwrap()
        .uuid;
    for (from, to, hole, diameter) in [
        (inner, top, "0.2mm", "0.45mm"),
        (inner, bottom, "0.3mm", "0.85mm"),
    ] {
        run(
            &mut b.shell,
            "pcb.via_type.add",
            json!({"document":b.pcb,"from":from,"to":to,"micro":to==top,"hole":hole,"diameter":diameter}),
        );
    }
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0.0, 0.0));
    b.shell.route_hover(mm(3.0, 4.0));
    run(&mut b.shell, "pcb.route.cycle_via", json!({}));
    run(&mut b.shell, "pcb.layer.activate:3", json!({}));
    assert_eq!(b.shell.route_preview().unwrap().layer, bottom);
    run(&mut b.shell, "tool.undo_segment", json!({}));
    assert_eq!(b.shell.route_preview().unwrap().layer, top);
    assert!(b.shell.route_preview().unwrap().vias.is_empty());
    run(&mut b.shell, "pcb.route.previous_layer", json!({}));
    run(&mut b.shell, "pcb.route.finish", json!({}));
    let (id, params) = last_entry(&b);
    assert_eq!(id, "batch");
    let steps = params["commands"].as_array().unwrap();
    assert_eq!(steps.len(), 3);
    for (entry, from, to, diameter) in [
        (&steps[1], inner, top, 450_000),
        (&steps[2], inner, bottom, 850_000),
    ] {
        assert_eq!(entry["id"], "pcb.via.add");
        let p = &entry["params"];
        assert_eq!(p["from"], json!(from));
        assert_eq!(p["to"], json!(to));
        assert_eq!(p["diameter"], diameter);
        assert_eq!(p["net"], json!(b.net));
        assert_eq!(p["at"], json!({"x":3_000_000,"y":4_000_000}));
    }
    run(&mut b.shell, "edit.undo", json!({"document":b.pcb}));
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    assert_eq!((pcb.vias().count(), pcb.tracks().count()), (0, 0));
}

#[test]
fn the_candidate_shows_its_drilling_direction_lands_and_the_obstruction_on_another_layer() {
    let mut b = board();
    run(&mut b.shell, "pcb.layer.add", json!({"document":b.pcb}));
    let copper: Vec<_> = b
        .shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    run(
        &mut b.shell,
        "pcb.via_type.add",
        json!({"document":b.pcb, "from":copper[1], "to":copper[0],
        "micro":true, "diameter":{"start":"2mm","end":"0.4mm"},"hole":"0.1mm"}),
    );
    run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer":copper[1],"points":[{"x":"5mm","y":"0.8mm"},{"x":"9mm","y":"0.8mm"}],"width":"0.2mm"}),
    );
    run(&mut b.shell, "pcb.layer.activate:2", json!({}));
    run(&mut b.shell, "pcb.layer.activate:1", json!({}));
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0., 0.));
    b.shell.route_hover(mm(5., 0.));
    let proposed = b.shell.route_preview().unwrap().via_candidate.unwrap();
    assert_eq!(proposed.target, copper[1]);
    assert_eq!(proposed.members[0].span.from, copper[1]);
    assert_eq!(proposed.members[0].diameter.end, Some(Nm::from_um(400)));
    assert!(proposed.blocked.is_some());
    assert!(
        !b.shell.router().halos().is_empty(),
        "the inner Layer has the only obstacle"
    );
    run(&mut b.shell, "pcb.route.previous_layer", json!({}));
    assert_eq!(b.shell.route_preview().unwrap().layer, copper[0]);
    assert!(b.shell.route_preview().unwrap().vias.is_empty());
    b.shell.route_hover(mm(5., 4.));
    assert!(
        b.shell
            .route_preview()
            .unwrap()
            .via_candidate
            .unwrap()
            .blocked
            .is_none()
    );
}

#[test]
fn microvia_limits_apply_to_the_user_size_and_each_size_mode_in_a_stay_or_end_jump() {
    let mut b = board();
    run(&mut b.shell, "pcb.layer.add", json!({"document":b.pcb}));
    let layers: Vec<_> = b
        .shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    run(
        &mut b.shell,
        "pcb.via_type.add",
        json!({"document":b.pcb,"from":layers[0],"to":layers[1],"micro":true,"diameter":"0.4mm","hole":"0.15mm"}),
    );
    run(
        &mut b.shell,
        "pcb.rules.set",
        json!({"kind":"microvia_size","values":{"diameter":{"min":"0.25mm","max":"0.8mm"},"hole":{"min":"0.12mm","max":"0.22mm"}}}),
    );
    run(&mut b.shell, "pcb.layer.activate:1", json!({}));
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0., 0.));
    b.shell.route_hover(mm(2., 0.));
    run(
        &mut b.shell,
        "pcb.route.pick_via",
        json!({"target":layers[1],"index":0}),
    );
    run(
        &mut b.shell,
        "pcb.route.via_size",
        json!({"diameter":"0.35mm","hole":"0.1mm"}),
    );
    let sizes = |s: &Shell| {
        let v = s.route_preview().unwrap().via_candidate.unwrap().members[0];
        (v.diameter.start.0, v.hole.0)
    };
    assert_eq!(sizes(&b.shell), (350_000, 120_000));
    for expected in [
        (250_000, 120_000),
        (400_000, 150_000),
        (800_000, 220_000),
        (350_000, 120_000),
    ] {
        run(&mut b.shell, "pcb.route.mode:via_size", json!({}));
        assert_eq!(sizes(&b.shell), expected);
    }
    run(&mut b.shell, "pcb.route.via_stay", json!({}));
    assert_eq!(b.shell.route_preview().unwrap().layer, layers[0]);
    b.shell.route_hover(mm(4., 2.));
    run(
        &mut b.shell,
        "pcb.route.pick_via",
        json!({"target":layers[1],"index":0}),
    );
    run(&mut b.shell, "pcb.route.via_end", json!({}));
    assert!(b.shell.route_preview().is_none());
    assert_eq!(b.shell.board_view().layers.active, Some(layers[0]));
    assert_eq!(last_entry(&b).0, "batch");
    assert_eq!(
        b.shell
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .vias()
            .count(),
        2
    );
}

#[test]
fn a_no_net_route_across_other_copper_keeps_its_net_on_the_track_and_via() {
    let mut b = board();
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key":"pcb.route.obstacle_mode","value":"ignore"}),
    );
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0., 4.));
    b.shell.route_hover(mm(0., 0.));
    run(&mut b.shell, "pcb.route.via_end", json!({}));
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.vias().next().unwrap().net, None);
    assert_eq!(pcb.tracks().next().unwrap().net, None);
}

#[test]
fn picking_up_copper_on_another_layer_sets_previous_and_l_changes_only_the_start() {
    let mut b = board();
    run(&mut b.shell, "pcb.layer.add", json!({"document":b.pcb}));
    let layers: Vec<_> = b
        .shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    run(
        &mut b.shell,
        "pcb.via_type.add",
        json!({"document":b.pcb,"from":layers[2],"to":layers[1]}),
    );
    run(
        &mut b.shell,
        "pcb.via.add",
        json!({"at":{"x":"4mm","y":"4mm"},"net":b.net,"from":layers[2],"to":layers[1]}),
    );
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(4., 4.));
    assert_eq!(b.shell.route_preview().unwrap().layer, layers[1]);
    assert_eq!(b.shell.board_view().layers.previous, Some(layers[0]));
    run(&mut b.shell, "pcb.route.start_layer", json!({}));
    let preview = b.shell.route_preview().unwrap();
    assert_eq!(preview.layer, layers[2]);
    assert_eq!(preview.net, Some(b.net));
    assert_eq!(b.shell.board_view().layers.previous, Some(layers[1]));
    assert!(preview.vias.is_empty());
    run(&mut b.shell, "pcb.route.finish", json!({}));
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

#[test]
fn a_land_on_the_far_end_of_the_candidate_keeps_the_board_edge_clear() {
    let mut b = board();
    run(
        &mut b.shell,
        "pcb.board_outline.add_rect",
        json!({"width":"20mm","height":"20mm"}),
    );
    run(&mut b.shell, "pcb.layer.add", json!({"document":b.pcb}));
    let layers: Vec<_> = b
        .shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    run(
        &mut b.shell,
        "pcb.via_type.add",
        json!({"document":b.pcb,"from":layers[0],"to":layers[1],"micro":true,
        "diameter":{"start":"0.4mm","end":"2.5mm"},"hole":"0.1mm"}),
    );
    run(&mut b.shell, "pcb.layer.activate:2", json!({}));
    run(&mut b.shell, "pcb.layer.activate:1", json!({}));
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(5., 5.));
    b.shell.route_hover(mm(1., 5.));
    let preview = b.shell.route_preview().unwrap();
    assert!(!preview.blocked, "the Track fits");
    assert!(
        preview.via_candidate.unwrap().blocked.is_some(),
        "the far-side land crosses the outline"
    );
    run(&mut b.shell, "pcb.layer.next", json!({}));
    assert_eq!(b.shell.route_preview().unwrap().layer, layers[0]);
}

#[test]
fn reopening_reuses_the_reverse_pair_choice_and_restricted_nets_only_see_allowed_candidates() {
    let mut b = board();
    run(&mut b.shell, "pcb.layer.add", json!({"document":b.pcb}));
    let layers: Vec<_> = b
        .shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    let mut kinds = Vec::new();
    for pair in layers.windows(2) {
        kinds.push(
            run(
                &mut b.shell,
                "pcb.via_type.add",
                json!({"document":b.pcb,"from":pair[0],"to":pair[1]}),
            )["result"]["uuid"]
                .clone(),
        );
    }
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0., 0.));
    run(&mut b.shell, "pcb.route.cycle_via", json!({}));
    run(&mut b.shell, "tool.activate:select", json!({}));
    run(&mut b.shell, "pcb.layer.activate:3", json!({}));
    run(&mut b.shell, "project.save", json!({}));
    let path = b.shell.engine().location().unwrap().to_owned();
    run(&mut b.shell, "project.close", json!({}));
    b.shell.open_project(&path).unwrap();
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(5., 5.));
    let preview = b.shell.route_preview().unwrap().via_candidate.unwrap();
    assert_eq!(preview.target, layers[0]);
    assert_eq!(preview.members.len(), 2);
    run(&mut b.shell, "tool.activate:select", json!({}));
    let class = run(
        &mut b.shell,
        "pcb.net_class.add",
        json!({"name":"Restricted","rules":[{"kind":"via_span","values":{"allowed":[kinds[0]]}}]}),
    )["result"]["uuid"]
        .clone();
    run(
        &mut b.shell,
        "pcb.net.add_to_class",
        json!({"nets":[b.net],"class":class}),
    );
    run(&mut b.shell, "pcb.layer.activate:1", json!({}));
    run(
        &mut b.shell,
        "pcb.layer.show",
        json!({"layer":layers[2],"visible":false}),
    );
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0., 0.));
    b.shell.route_hover(mm(5., 0.));
    let prior = b.shell.board_view().layers.clone();
    run(&mut b.shell, "pcb.layer.activate:3", json!({}));
    assert_eq!(
        b.shell.board_view().layers,
        prior,
        "a refused jump does not even reveal the Layer"
    );
    assert!(b.shell.message().unwrap().contains("No permitted Via"));
    let p = b.shell.route_preview().unwrap();
    assert!(p.vias.is_empty());
    assert_eq!(p.placed, [mm(0., 0.)]);
    assert_eq!(p.net, Some(b.net));
    run(&mut b.shell, "pcb.layer.next", json!({}));
    assert_eq!(b.shell.route_preview().unwrap().layer, layers[1]);
}

#[test]
fn a_stack_can_clear_an_obstacle_on_an_omitted_land_but_its_bore_still_needs_clearance() {
    for mode in ["remove", "remove_keep_ends"] {
        let mut b = board();
        for _ in 0..2 {
            run(&mut b.shell, "pcb.layer.add", json!({"document": b.pcb}));
        }
        let layers: Vec<_> = b
            .shell
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .copper_layers()
            .map(|l| l.uuid)
            .collect();
        for (from, to) in [(0, 2), (2, 3)] {
            run(
                &mut b.shell,
                "pcb.via_type.add",
                json!({"document": b.pcb,
                "from": layers[from], "to": layers[to], "hole": "0.2mm",
                "diameter":{"start":"0.6mm","middle":"2mm"}, "unused_layers": mode}),
            );
        }
        run(
            &mut b.shell,
            "pcb.rules.set",
            json!({"kind":"clearance",
            "values":{"value":"0.1mm","hole":{"via":"0.3mm"}}}),
        );
        let obstacle = run(
            &mut b.shell,
            "pcb.track.add",
            json!({"layer":layers[1], "net":(mode == "remove").then_some(b.net),
            "points":[{"x":"2mm","y":"0.7mm"},{"x":"8mm","y":"0.7mm"}],"width":"0.1mm"}),
        );
        let track: Uuid = obstacle["result"]["tracks"][0]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        run(&mut b.shell, "pcb.layer.activate:4", json!({}));
        run(&mut b.shell, "pcb.layer.activate:1", json!({}));
        run(&mut b.shell, "tool.activate:route", json!({}));
        route_to(&mut b, mm(0., 0.));
        run(&mut b.shell, "pcb.route.cycle_via", json!({}));
        b.shell.route_hover(mm(5., 0.5));
        let candidate = b.shell.route_preview().unwrap().via_candidate.unwrap();
        assert_eq!(candidate.members.len(), 2);
        assert!(
            candidate.blocked.is_some(),
            "the plated bore still needs 0.3 mm"
        );
        run(&mut b.shell, "pcb.route.previous_layer", json!({}));
        assert!(b.shell.route_preview().unwrap().vias.is_empty());
        b.shell.route_hover(mm(5., 0.));
        let candidate = b.shell.route_preview().unwrap().via_candidate.unwrap();
        assert_eq!(
            candidate.blocked, None,
            "the middle land is absent in {mode}"
        );
        assert!(
            !candidate.rings.contains(&Nm::from_mm(2)),
            "the omitted land has no preview ring"
        );
        let halo = b
            .shell
            .router()
            .halos()
            .iter()
            .find(|h| h.item.object() == track)
            .unwrap();
        assert_eq!(
            halo.clearance,
            Nm::from_um(300),
            "shown relative to the bore"
        );
        run(&mut b.shell, "pcb.route.previous_layer", json!({}));
        assert_eq!(b.shell.route_preview().unwrap().layer, layers[3]);
        run(&mut b.shell, "pcb.route.finish", json!({}));
        let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
        assert_eq!(pcb.vias().count(), 2);
        let first = pcb.vias().find(|v| v.span.from == layers[0]).unwrap();
        let item = reda_ui::engine::pcb::Item::Via { uuid: first.uuid };
        let copper = reda_ui::engine::pcb::Copper::build(pcb);
        assert!(
            copper.items_at(layers[0], first.at).contains(&item),
            "the placed Track restores the start land"
        );
        assert!(!copper.items_at(layers[1], first.at).contains(&item));
    }
}

#[test]
fn an_uncommitted_track_restores_a_removed_via_land_only_on_its_own_layer() {
    let mut b = board();
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    let through = pcb.stackup().through_type().uuid;
    let top = copper(&b, Side::Top);
    let bottom = copper(&b, Side::Bottom);
    run(
        &mut b.shell,
        "pcb.via_type.set",
        json!({"via_type":through,
        "unused_layers":"remove","diameter":"2mm", "hole":"0.25mm"}),
    );
    for (layer, x) in [(top, "2mm"), (bottom, "7mm")] {
        run(
            &mut b.shell,
            "pcb.track.add",
            json!({"layer":layer,
            "points":[{"x":x,"y":"0.7mm"},{"x":x,"y":"3mm"}],"width":"0.1mm"}),
        );
    }
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0., 0.));
    b.shell.route_hover(mm(2., 0.));
    let p = b.shell.route_preview().unwrap();
    assert!(!p.blocked);
    assert!(
        p.via_candidate.unwrap().blocked.is_some(),
        "arrival restores the large top land"
    );
    b.shell.route_hover(mm(7., 0.));
    run(
        &mut b.shell,
        "pcb.layer.highlight",
        json!({"layer":bottom,"on":true}),
    );
    let p = b.shell.route_preview().unwrap();
    assert_eq!(p.layer, top, "highlighting a Layer is a display operation");
    assert!(p.vias.is_empty());
    assert!(
        p.via_candidate.unwrap().blocked.is_none(),
        "ending at this Via leaves the bottom land absent"
    );
    run(&mut b.shell, "pcb.route.previous_layer", json!({}));
    b.shell.route_hover(mm(9., 0.));
    let p = b.shell.route_preview().unwrap();
    assert!(
        p.blocked,
        "leaving the stack would restore the bottom land into the obstacle"
    );
    b.shell.route_click(mm(9., 0.));
    assert_eq!(b.shell.route_preview().unwrap().placed, vec![mm(7., 0.)]);
    run(&mut b.shell, "pcb.route.finish", json!({}));
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.vias().count(), 1);
    assert_eq!(pcb.tracks().filter(|t| t.net == Some(b.net)).count(), 1);
}

#[test]
fn the_candidate_list_can_jump_to_its_last_page_and_cycle_back_to_the_single_via() {
    use reda_ui::pcb::route::Chooser;
    let mut b = board();
    let add_layer = json!({"id":"pcb.layer.add","params":{"document":b.pcb}});
    run(
        &mut b.shell,
        "batch",
        json!({"commands":vec![add_layer; 10]}),
    );
    let layers: Vec<_> = b
        .shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    let mut add_types = Vec::new();
    for (i, &from) in layers.iter().enumerate() {
        for &to in &layers[i + 1..] {
            if i != 0 || to != layers[11] {
                add_types.push(json!({"id":"pcb.via_type.add","params":{"document":b.pcb,"from":from,"to":to}}));
            }
        }
    }
    run(&mut b.shell, "batch", json!({"commands":add_types}));
    run(&mut b.shell, "tool.activate:route", json!({}));
    route_to(&mut b, mm(0., 0.));
    b.shell.route_hover(mm(4., 0.));
    run(
        &mut b.shell,
        "pcb.route.pick_via",
        json!({"target":layers[11],"page":usize::MAX}),
    );
    assert!(matches!(
        b.shell.router().chooser(),
        Some(Chooser::Candidates {
            page: 63,
            count: 1024,
            ..
        })
    ));
    run(&mut b.shell, "pcb.route.pick_via", json!({"index":1023}));
    assert!(
        b.shell
            .route_preview()
            .unwrap()
            .via_candidate
            .unwrap()
            .members
            .len()
            > 1
    );
    run(&mut b.shell, "pcb.route.cycle_via", json!({}));
    let candidate = b.shell.route_preview().unwrap().via_candidate.unwrap();
    assert_eq!(candidate.members.len(), 1);
    assert_eq!(candidate.members[0].span.from, layers[0]);
    assert_eq!(candidate.members[0].span.to, layers[11]);
}
