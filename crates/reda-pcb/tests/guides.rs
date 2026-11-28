//! Guides on a board (docs/spec/v0.3/grids-guides.md#guides): `pcb.guide.add / set /
//! delete` with undo, the ordinary move, lock and delete Commands, the Output Origin
//! leaving them where they are and Move Board Shape carrying them.

use reda_command::ErrorCode;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{Guide, GuideKind, PcbDocument, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

const MM: i64 = 1_000_000;

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn document(engine: &TestEngine) -> Uuid {
    pcb(engine).uuid
}

fn created(out: &Value) -> Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm(x * MM), Nm(y * MM))
}

fn guide(engine: &TestEngine, uuid: Uuid) -> Guide {
    pcb(engine).guide(uuid).expect("the Guide").clone()
}

fn line(engine: &mut TestEngine, x: i64, y: i64, angle: i64) -> Uuid {
    created(&engine.run_ok(
        "pcb.guide.add",
        json!({"kind": "line", "at": {"x": x * MM, "y": y * MM}, "angle": angle}),
    ))
}

fn point(engine: &mut TestEngine, x: i64, y: i64) -> Uuid {
    created(&engine.run_ok(
        "pcb.guide.add",
        json!({"kind": "point", "at": {"x": x * MM, "y": y * MM}}),
    ))
}

#[test]
fn a_line_and_a_point_guide_are_added_shown_enabled_and_unlocked() {
    let dir = TempDir::new("guide-add");
    let mut e = engine(&dir);
    let l = line(&mut e, 3, 4, 90_000);
    let p = point(&mut e, 5, 6);
    assert_eq!(
        guide(&e, l),
        Guide {
            uuid: l,
            kind: GuideKind::Line,
            at: mm(3, 4),
            angle: Some(Millideg(90_000)),
            name: None,
            shown: true,
            enabled: true,
            locked: false,
        }
    );
    let g = guide(&e, p);
    assert_eq!((g.kind, g.at, g.angle), (GuideKind::Point, mm(5, 6), None));
}

#[test]
fn a_line_angle_is_kept_in_half_a_turn() {
    let dir = TempDir::new("guide-angle");
    let mut e = engine(&dir);
    let g = line(&mut e, 0, 0, 225_000);
    assert_eq!(guide(&e, g).angle, Some(Millideg(45_000)));
    let g = line(&mut e, 0, 0, -45_000);
    assert_eq!(guide(&e, g).angle, Some(Millideg(135_000)));
}

#[test]
fn add_takes_name_shown_enabled_and_locked() {
    let dir = TempDir::new("guide-add-fields");
    let mut e = engine(&dir);
    let g = created(&e.run_ok(
        "pcb.guide.add",
        json!({"kind": "line", "at": {"x": 0, "y": "2mm"}, "angle": 0, "name": "Header row",
               "shown": false, "enabled": false, "locked": true}),
    ));
    let g = guide(&e, g);
    assert_eq!(g.name.as_deref(), Some("Header row"));
    assert_eq!((g.shown, g.enabled, g.locked), (false, false, true));
    assert_eq!(g.at, mm(0, 2));
}

#[test]
fn add_refuses_a_line_without_an_angle_and_a_point_with_one() {
    let dir = TempDir::new("guide-add-refused");
    let mut e = engine(&dir);
    let err = e.run_err(
        "pcb.guide.add",
        json!({"kind": "line", "at": {"x": 0, "y": 0}}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/angle");
    let err = e.run_err(
        "pcb.guide.add",
        json!({"kind": "point", "at": {"x": 0, "y": 0}, "angle": 0}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/angle");
    let err = e.run_err(
        "pcb.guide.add",
        json!({"kind": "point", "at": {"x": 0, "y": 0}, "name": "  "}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert!(pcb(&e).guides().next().is_none());
}

#[test]
fn set_changes_the_given_fields_and_a_null_name_clears_it() {
    let dir = TempDir::new("guide-set");
    let mut e = engine(&dir);
    let g = line(&mut e, 0, 0, 0);
    e.run_ok(
        "pcb.guide.set",
        json!({"uuid": g, "at": {"x": "1mm", "y": "2mm"}, "angle": 30_000, "name": "Edge",
               "shown": false}),
    );
    let after = guide(&e, g);
    assert_eq!(after.at, mm(1, 2));
    assert_eq!(after.angle, Some(Millideg(30_000)));
    assert_eq!(after.name.as_deref(), Some("Edge"));
    assert!(!after.shown && after.enabled && !after.locked);
    e.run_ok(
        "pcb.guide.set",
        json!({"uuid": g, "name": null, "enabled": false}),
    );
    let after = guide(&e, g);
    assert_eq!(after.name, None);
    assert!(!after.enabled);
}

#[test]
fn set_refuses_an_angle_on_a_point_and_moving_a_locked_guide() {
    let dir = TempDir::new("guide-set-refused");
    let mut e = engine(&dir);
    let p = point(&mut e, 0, 0);
    let err = e.run_err("pcb.guide.set", json!({"uuid": p, "angle": 0}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let g = line(&mut e, 0, 0, 0);
    e.run_ok("pcb.guide.set", json!({"uuid": g, "locked": true}));
    let err = e.run_err("pcb.guide.set", json!({"uuid": g, "at": {"x": 1, "y": 1}}));
    assert_eq!(err.code, ErrorCode::Rejected);
    // Its switches are not geometry, so a locked Guide still takes them.
    e.run_ok("pcb.guide.set", json!({"uuid": g, "shown": false}));
    e.run_ok(
        "pcb.guide.set",
        json!({"uuid": g, "at": {"x": 1, "y": 1}, "include_locked": true}),
    );
    assert_eq!(guide(&e, g).at, Point::new(Nm(1), Nm(1)));
    let err = e.run_err("pcb.guide.set", json!({"uuid": Uuid::NIL}));
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn delete_removes_one_guide_and_refuses_a_locked_one() {
    let dir = TempDir::new("guide-delete");
    let mut e = engine(&dir);
    let a = line(&mut e, 0, 0, 0);
    let b = point(&mut e, 1, 1);
    e.run_ok("pcb.guide.delete", json!({"uuid": a}));
    assert!(pcb(&e).guide(a).is_none());
    e.run_ok("pcb.lock", json!({"objects": [b], "locked": true}));
    let err = e.run_err("pcb.guide.delete", json!({"uuid": b}));
    assert_eq!(err.code, ErrorCode::Rejected);
    e.run_ok(
        "pcb.guide.delete",
        json!({"uuid": b, "include_locked": true}),
    );
    assert!(pcb(&e).guides().next().is_none());
    let err = e.run_err("pcb.guide.delete", json!({"uuid": b}));
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn each_guide_edit_is_one_undo_step() {
    let dir = TempDir::new("guide-undo");
    let mut e = engine(&dir);
    let doc = document(&e);
    let g = line(&mut e, 0, 0, 0);
    e.run_ok("pcb.guide.set", json!({"uuid": g, "angle": 90_000}));
    e.run_ok("pcb.guide.delete", json!({"uuid": g}));
    e.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(guide(&e, g).angle, Some(Millideg(90_000)));
    e.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(guide(&e, g).angle, Some(Millideg(0)));
    e.run_ok("edit.undo", json!({"document": doc}));
    assert!(pcb(&e).guides().next().is_none());
}

#[test]
fn guides_round_trip_through_the_file_in_their_own_table() {
    let dir = TempDir::new("guide-file");
    let mut e = engine(&dir);
    let l = created(&e.run_ok(
        "pcb.guide.add",
        json!({"kind": "line", "at": {"x": "1mm", "y": "2mm"}, "angle": 45_000,
               "name": "Diagonal"}),
    ));
    let p = point(&mut e, 3, 4);
    e.run_ok("project.save", json!({}));
    let file = dir.path().join("Demo/Demo.redapcb");
    let text = std::fs::read_to_string(&file).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    let rows = value["guides"].as_array().expect("a guides table");
    assert_eq!(rows.len(), 2);
    let row = |uuid: Uuid| {
        rows.iter()
            .find(|r| r["uuid"] == json!(uuid))
            .expect("a row")
            .clone()
    };
    assert_eq!(
        row(l),
        json!({"uuid": l, "kind": "line", "at": {"x": MM, "y": 2 * MM}, "angle": 45_000,
               "name": "Diagonal", "shown": true, "enabled": true, "locked": false})
    );
    assert_eq!(
        row(p),
        json!({"uuid": p, "kind": "point", "at": {"x": 3 * MM, "y": 4 * MM},
               "shown": true, "enabled": true, "locked": false})
    );
    let mut reopened = probe_engine();
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(guide(&reopened, l), guide(&e, l));
    assert_eq!(guide(&reopened, p), guide(&e, p));
    reopened.run_ok("pcb.guide.set", json!({"uuid": p, "name": "x"}));
    reopened.run_ok("edit.undo", json!({"document": document(&reopened)}));
    reopened.run_ok("project.save", json!({}));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn a_guide_moves_turns_flips_and_locks_as_any_placed_object() {
    let dir = TempDir::new("guide-move");
    let mut e = engine(&dir);
    let l = line(&mut e, 1, 0, 30_000);
    let p = point(&mut e, 2, 3);
    e.run_ok(
        "pcb.move",
        json!({"objects": [l, p], "by": {"x": "1mm", "y": "1mm"}}),
    );
    assert_eq!(guide(&e, l).at, mm(2, 1));
    assert_eq!(guide(&e, p).at, mm(3, 4));
    e.run_ok(
        "pcb.rotate",
        json!({"objects": [l], "angle": 90_000, "center": {"x": 0, "y": 0}}),
    );
    let g = guide(&e, l);
    assert_eq!((g.at, g.angle), (mm(-1, 2), Some(Millideg(120_000))));
    e.run_ok(
        "pcb.flip",
        json!({"objects": [l], "center": {"x": 0, "y": 0}}),
    );
    let g = guide(&e, l);
    assert_eq!((g.at, g.angle), (mm(1, 2), Some(Millideg(60_000))));
    e.run_ok("pcb.lock", json!({"objects": [p], "locked": true}));
    assert!(guide(&e, p).locked);
    let err = e.run_err("pcb.move", json!({"objects": [p], "by": {"x": 1, "y": 0}}));
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn guides_stay_put_when_the_output_origin_moves() {
    let dir = TempDir::new("guide-origin");
    let mut e = engine(&dir);
    let l = line(&mut e, 5, 5, 0);
    let p = point(&mut e, 6, 6);
    e.run_ok(
        "pcb.output_origin.set",
        json!({"at": {"x": "10mm", "y": "10mm"}}),
    );
    let origin = pcb(&e).output_origin().unwrap().uuid;
    e.run_ok(
        "pcb.move",
        json!({"objects": [origin], "by": {"x": "3mm", "y": "0mm"}}),
    );
    assert_eq!(guide(&e, l).at, mm(5, 5));
    assert_eq!(guide(&e, p).at, mm(6, 6));
}

#[test]
fn move_board_shape_and_move_board_carry_every_guide() {
    let dir = TempDir::new("guide-board-shape");
    let mut e = engine(&dir);
    let vertices: Vec<Value> = [(0, 0), (20, 0), (20, 10), (0, 10)]
        .iter()
        .map(|&(x, y)| json!({"x": x * MM, "y": y * MM}))
        .collect();
    e.run_ok("pcb.board_outline.add", json!({"vertices": vertices}));
    let outline = pcb(&e).outline().unwrap().uuid;
    // Inside and far outside the board, and locked: a Guide belongs to the board.
    let inside = line(&mut e, 5, 5, 0);
    let outside = point(&mut e, 100, 100);
    e.run_ok("pcb.lock", json!({"objects": [outside], "locked": true}));
    e.run_ok(
        "pcb.move",
        json!({"objects": [outline], "by": {"x": "2mm", "y": "3mm"}}),
    );
    assert_eq!(guide(&e, inside).at, mm(7, 8));
    assert_eq!(guide(&e, outside).at, mm(102, 103));
    assert!(guide(&e, outside).locked);
    e.run_ok(
        "pcb.rotate",
        json!({"objects": [outline], "angle": 90_000, "center": {"x": 0, "y": 0}}),
    );
    let g = guide(&e, inside);
    assert_eq!((g.at, g.angle), (mm(-8, 7), Some(Millideg(90_000))));
    let out = e.run_ok("pcb.board.move", json!({"dx": "1mm", "dy": "0mm"}));
    assert_eq!(guide(&e, inside).at, mm(-7, 7));
    assert_eq!(guide(&e, outside).at, mm(-102, 102));
    let moved = out["result"]["objects"].as_array().unwrap();
    assert!(moved.contains(&json!(inside)) && moved.contains(&json!(outside)));
}

#[test]
fn moving_a_guide_alone_leaves_the_board_shape_where_it_is() {
    let dir = TempDir::new("guide-alone");
    let mut e = engine(&dir);
    let vertices: Vec<Value> = [(0, 0), (20, 0), (20, 10), (0, 10)]
        .iter()
        .map(|&(x, y)| json!({"x": x * MM, "y": y * MM}))
        .collect();
    e.run_ok("pcb.board_outline.add", json!({"vertices": vertices}));
    let before = pcb(&e).outline().unwrap().clone();
    let a = line(&mut e, 5, 5, 0);
    let b = point(&mut e, 1, 1);
    e.run_ok(
        "pcb.move",
        json!({"objects": [a], "by": {"x": 0, "y": "1mm"}}),
    );
    assert_eq!(pcb(&e).outline().unwrap(), &before);
    assert_eq!(guide(&e, b).at, mm(1, 1));
}
