//! Cartesian Local Grids on a board (docs/spec/v0.3/grids-guides.md#local-grids):
//! `pcb.local_grid.add / set / delete / reorder` with undo, the list order kept as the
//! priority in the file, unique names, the ordinary move, lock and delete Commands, the
//! Output Origin leaving them where they are and Move Board Shape carrying them, and undo
//! restoring the saved bytes after any sequence of Local Grid edits.

use proptest::prelude::*;
use reda_command::ErrorCode;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    AppliesTo, CartesianGrid, LengthUnit, LocalGrid, LocalGridShape, PcbDocument, TypedLength, Uuid,
};
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

fn local(engine: &TestEngine, uuid: Uuid) -> LocalGrid {
    pcb(engine)
        .local_grid(uuid)
        .expect("the Local Grid")
        .clone()
}

fn cartesian(engine: &TestEngine, uuid: Uuid) -> CartesianGrid {
    let LocalGridShape::Cartesian(c) = local(engine, uuid).shape;
    c
}

/// A 10 × 5 mm Local Grid at (`x`, `y`) mm on a 1 mm step.
fn shape(x: i64, y: i64) -> Value {
    json!({"kind": "cartesian", "origin": {"x": x * MM, "y": y * MM}, "step_x": "1mm",
           "width": "10mm", "height": "5mm"})
}

fn add(engine: &mut TestEngine, params: Value) -> Uuid {
    created(&engine.run_ok("pcb.local_grid.add", params))
}

fn order(engine: &TestEngine) -> Vec<Uuid> {
    pcb(engine).local_grids().map(|g| g.uuid).collect()
}

fn mm_text(text: &str) -> TypedLength {
    TypedLength::parse(text).unwrap()
}

#[test]
fn a_cartesian_local_grid_is_added_with_its_shape_and_the_defaults() {
    let dir = TempDir::new("local-grid-add");
    let mut e = engine(&dir);
    let g = add(
        &mut e,
        json!({"name": "Header", "shape": {"kind": "cartesian",
               "origin": {"x": "2mm", "y": "3mm"}, "rotation": -90_000, "step_x": "100mil",
               "step_y": "1.27mm", "width": "25.4mm", "height": "2.54mm"}}),
    );
    assert_eq!(
        local(&e, g),
        LocalGrid {
            uuid: g,
            name: "Header".into(),
            applies_to: AppliesTo::All,
            shown: true,
            enabled: true,
            locked: false,
            shape: LocalGridShape::Cartesian(CartesianGrid {
                origin: mm(2, 3),
                rotation: Millideg(270_000),
                step_x: TypedLength::new(Nm(2_540_000), LengthUnit::Mil),
                step_y: TypedLength::new(Nm(1_270_000), LengthUnit::Mm),
                width: Nm(25_400_000),
                height: Nm(2_540_000),
            }),
            rank: 0,
        }
    );
    // Step Y follows Step X when left out, and a bare nm step is kept in mm.
    let h = add(
        &mut e,
        json!({"shape": {"kind": "cartesian", "origin": {"x": 0, "y": 0}, "step_x": 500_000,
               "width": "1mm", "height": "1mm"}, "applies_to": "components",
               "shown": false, "enabled": false, "locked": true}),
    );
    let c = cartesian(&e, h);
    assert_eq!((c.step_x, c.step_y), (mm_text("0.5mm"), mm_text("0.5mm")));
    let h = local(&e, h);
    assert_eq!(h.applies_to, AppliesTo::Components);
    assert_eq!((h.shown, h.enabled, h.locked), (false, false, true));
}

#[test]
fn unnamed_local_grids_take_the_first_free_number_and_go_last_in_the_order() {
    let dir = TempDir::new("local-grid-names");
    let mut e = engine(&dir);
    let a = add(&mut e, json!({"shape": shape(0, 0)}));
    let b = add(
        &mut e,
        json!({"name": "Local Grid 2", "shape": shape(0, 0)}),
    );
    let c = add(&mut e, json!({"shape": shape(0, 0)}));
    assert_eq!(local(&e, a).name, "Local Grid 1");
    assert_eq!(local(&e, c).name, "Local Grid 3");
    assert_eq!(order(&e), [a, b, c]);
}

fn refused(e: &mut TestEngine, id: &str, params: Value, code: ErrorCode, pointer: Option<&str>) {
    let before = pcb(e).clone();
    let err = e.run_err(id, params.clone());
    assert_eq!(err.code, code, "{id} {params}: {err:?}");
    assert_eq!(
        err.details.as_ref().and_then(|d| d["pointer"].as_str()),
        pointer,
        "{id} {params}: {err:?}"
    );
    assert_eq!(pcb(e), &before, "{id} {params} wrote something");
}

#[test]
fn a_duplicate_name_and_a_bad_shape_are_refused_writing_nothing() {
    let dir = TempDir::new("local-grid-refusals");
    let mut e = engine(&dir);
    let invalid = ErrorCode::InvalidParams;
    let a = add(&mut e, json!({"name": "Header", "shape": shape(0, 0)}));
    let b = add(&mut e, json!({"name": "Connector", "shape": shape(0, 0)}));
    refused(
        &mut e,
        "pcb.local_grid.add",
        json!({"name": "Header", "shape": shape(5, 5)}),
        invalid,
        Some("/name"),
    );
    refused(
        &mut e,
        "pcb.local_grid.set",
        json!({"uuid": b, "name": "Header"}),
        invalid,
        Some("/name"),
    );
    refused(
        &mut e,
        "pcb.local_grid.add",
        json!({"name": " ", "shape": shape(5, 5)}),
        invalid,
        Some("/name"),
    );
    // Renaming a Local Grid to its own name is no duplicate.
    e.run_ok("pcb.local_grid.set", json!({"uuid": a, "name": "Header"}));
    for (field, value) in [
        ("width", json!(0)),
        ("height", json!("-1mm")),
        ("step_x", json!("0mm")),
        ("step_y", json!("-2mil")),
    ] {
        let mut s = shape(0, 0);
        s[field] = value;
        refused(
            &mut e,
            "pcb.local_grid.add",
            json!({"shape": s}),
            invalid,
            Some(&format!("/shape/{field}")),
        );
    }
    let nobody = "00000000-0000-4000-8000-0000000000ff";
    for id in ["pcb.local_grid.set", "pcb.local_grid.delete"] {
        refused(
            &mut e,
            id,
            json!({"uuid": nobody}),
            ErrorCode::NotFound,
            None,
        );
    }
}

#[test]
fn set_changes_the_given_fields_and_a_locked_grid_refuses_a_new_shape() {
    let dir = TempDir::new("local-grid-set");
    let mut e = engine(&dir);
    let g = add(&mut e, json!({"name": "A", "shape": shape(0, 0)}));
    e.run_ok(
        "pcb.local_grid.set",
        json!({"uuid": g, "name": "B", "applies_to": "not_components", "shown": false,
               "shape": shape(1, 2)}),
    );
    let after = local(&e, g);
    assert_eq!(after.name, "B");
    assert_eq!(after.applies_to, AppliesTo::NotComponents);
    assert!(!after.shown && after.enabled && !after.locked);
    assert_eq!(cartesian(&e, g).origin, mm(1, 2));
    e.run_ok("pcb.lock", json!({"objects": [g], "locked": true}));
    assert!(local(&e, g).locked);
    refused(
        &mut e,
        "pcb.local_grid.set",
        json!({"uuid": g, "shape": shape(3, 3)}),
        ErrorCode::Rejected,
        None,
    );
    // The switches and the name are not geometry, and the same shape is no reshape.
    e.run_ok(
        "pcb.local_grid.set",
        json!({"uuid": g, "enabled": false, "name": "C", "shape": shape(1, 2)}),
    );
    e.run_ok(
        "pcb.local_grid.set",
        json!({"uuid": g, "shape": shape(3, 3), "include_locked": true}),
    );
    assert_eq!(cartesian(&e, g).origin, mm(3, 3));
}

#[test]
fn delete_removes_one_local_grid_and_refuses_a_locked_one() {
    let dir = TempDir::new("local-grid-delete");
    let mut e = engine(&dir);
    let a = add(&mut e, json!({"shape": shape(0, 0)}));
    let b = add(&mut e, json!({"shape": shape(0, 0), "locked": true}));
    e.run_ok("pcb.local_grid.delete", json!({"uuid": a}));
    assert_eq!(order(&e), [b]);
    refused(
        &mut e,
        "pcb.local_grid.delete",
        json!({"uuid": b}),
        ErrorCode::Rejected,
        None,
    );
    e.run_ok(
        "pcb.local_grid.delete",
        json!({"uuid": b, "include_locked": true}),
    );
    assert!(order(&e).is_empty());
}

#[test]
fn reorder_moves_a_local_grid_before_another_or_to_the_end() {
    let dir = TempDir::new("local-grid-reorder");
    let mut e = engine(&dir);
    let doc = document(&e);
    let a = add(&mut e, json!({"shape": shape(0, 0)}));
    let b = add(&mut e, json!({"shape": shape(0, 0)}));
    let c = add(&mut e, json!({"shape": shape(0, 0)}));
    e.run_ok(
        "pcb.local_grid.reorder",
        json!({"local_grid": c, "before": a}),
    );
    assert_eq!(order(&e), [c, a, b]);
    e.run_ok("pcb.local_grid.reorder", json!({"local_grid": c}));
    assert_eq!(order(&e), [a, b, c]);
    e.run_ok(
        "pcb.local_grid.reorder",
        json!({"local_grid": a, "before": c}),
    );
    assert_eq!(order(&e), [b, a, c]);
    // Each reorder is one undo step.
    e.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(order(&e), [a, b, c]);
    e.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(order(&e), [c, a, b]);
    // Already in place: nothing changes.
    let out = e.run_ok("pcb.local_grid.reorder", json!({"local_grid": b}));
    assert_eq!(order(&e), [c, a, b], "{out}");
    let invalid = ErrorCode::InvalidParams;
    refused(
        &mut e,
        "pcb.local_grid.reorder",
        json!({"local_grid": a, "before": a}),
        invalid,
        Some("/before"),
    );
    refused(
        &mut e,
        "pcb.local_grid.reorder",
        json!({"local_grid": a, "before": Uuid::NIL}),
        invalid,
        Some("/before"),
    );
    refused(
        &mut e,
        "pcb.local_grid.reorder",
        json!({"local_grid": Uuid::NIL}),
        ErrorCode::NotFound,
        None,
    );
}

#[test]
fn local_grids_round_trip_through_the_file_in_priority_order() {
    let dir = TempDir::new("local-grid-file");
    let mut e = engine(&dir);
    let file = dir.path().join("Demo/Demo.redapcb");
    let a = add(&mut e, json!({"name": "Header", "shape": shape(0, 0)}));
    let b = add(
        &mut e,
        json!({"name": "Connector", "applies_to": "components", "shape": {"kind": "cartesian",
               "origin": {"x": "1mm", "y": "2mm"}, "rotation": 45_000, "step_x": "25mil",
               "step_y": "0.5mm", "width": "4mm", "height": "3mm"}}),
    );
    // Put the second first, so the file order is not the UUID order.
    e.run_ok(
        "pcb.local_grid.reorder",
        json!({"local_grid": b, "before": a}),
    );
    e.run_ok("project.save", json!({}));
    let text = std::fs::read_to_string(&file).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    let keys: Vec<&String> = value["grids"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["local"], "no Board Grids: `board` is left out");
    assert_eq!(
        value["grids"]["local"],
        json!([
            {"uuid": b, "name": "Connector", "applies_to": "components", "shown": true,
             "enabled": true, "locked": false, "shape": {"kind": "cartesian",
             "origin": {"x": MM, "y": 2 * MM}, "rotation": 45_000, "step_x": "25mil",
             "step_y": "0.5mm", "width": 4 * MM, "height": 3 * MM}},
            {"uuid": a, "name": "Header", "applies_to": "all", "shown": true,
             "enabled": true, "locked": false, "shape": {"kind": "cartesian",
             "origin": {"x": 0, "y": 0}, "rotation": 0, "step_x": "1mm", "step_y": "1mm",
             "width": 10 * MM, "height": 5 * MM}},
        ])
    );
    // Each row on its own line.
    let first = format!("\"grids\": {{\"local\":[\n    {{\"uuid\":\"{b}\",\"name\":\"Connector\"");
    assert!(text.contains(&first), "{text}");
    e.run_ok("pcb.board_grid.add", json!({"step_x": "0.3mm"}));
    e.run_ok("project.save", json!({}));
    let text = std::fs::read_to_string(&file).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    let keys: Vec<&String> = value["grids"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["board", "local"]);
    assert!(text.contains("\n  ],\"local\":[\n    {"), "{text}");
    let mut reopened = probe_engine();
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(order(&reopened), [b, a]);
    assert_eq!(cartesian(&reopened, b), cartesian(&e, b));
    reopened.run_ok("project.save", json!({}));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn the_file_refuses_a_duplicate_name_and_a_shape_without_a_size() {
    let dir = TempDir::new("local-grid-file-refusals");
    let mut e = engine(&dir);
    add(&mut e, json!({"name": "Header", "shape": shape(0, 0)}));
    add(&mut e, json!({"name": "Other", "shape": shape(0, 0)}));
    e.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let file = folder.join("Demo.redapcb");
    let text = std::fs::read_to_string(&file).unwrap();
    for (from, to, says) in [
        ("\"name\":\"Other\"", "\"name\":\"Header\"", "unique"),
        ("\"width\":10000000", "\"width\":0", "positive"),
        ("\"step_x\":\"1mm\"", "\"step_x\":1000000", "unit"),
        (
            "\"kind\":\"cartesian\"",
            "\"kind\":\"hexagonal\"",
            "hexagonal",
        ),
    ] {
        assert!(text.contains(from), "{text}");
        std::fs::write(&file, text.replacen(from, to, 1)).unwrap();
        let err = probe_engine().run_err("project.open", json!({"path": folder}));
        assert_eq!(err.code, ErrorCode::Rejected, "{to}: {err:?}");
        assert!(err.message.contains(says), "{to}: {}", err.message);
    }
}

#[test]
fn a_local_grid_moves_turns_flips_and_locks_as_any_placed_object() {
    let dir = TempDir::new("local-grid-move");
    let mut e = engine(&dir);
    let g = add(&mut e, json!({"shape": shape(1, 0)}));
    e.run_ok(
        "pcb.move",
        json!({"objects": [g], "by": {"x": "1mm", "y": "1mm"}}),
    );
    assert_eq!(cartesian(&e, g).origin, mm(2, 1));
    e.run_ok(
        "pcb.rotate",
        json!({"objects": [g], "angle": 90_000, "center": {"x": 0, "y": 0}}),
    );
    let c = cartesian(&e, g);
    assert_eq!((c.origin, c.rotation), (mm(-1, 2), Millideg(90_000)));
    // The 10 × 5 mm rectangle now runs up from (-1, 2) and to the left: corners at
    // x -6..-1, y 2..12. Flipped about x = 0 it covers x 1..6, y 2..12.
    e.run_ok(
        "pcb.flip",
        json!({"objects": [g], "center": {"x": 0, "y": 0}}),
    );
    let mut corners = local(&e, g).outline();
    corners.sort_by_key(|p| (p.x, p.y));
    assert_eq!(corners, [mm(1, 2), mm(1, 12), mm(6, 2), mm(6, 12)]);
    e.run_ok("pcb.lock", json!({"objects": [g], "locked": true}));
    let err = e.run_err("pcb.move", json!({"objects": [g], "by": {"x": 1, "y": 0}}));
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("Local Grid"), "{}", err.message);
}

#[test]
fn local_grids_stay_put_when_the_output_origin_moves() {
    let dir = TempDir::new("local-grid-origin");
    let mut e = engine(&dir);
    let g = add(&mut e, json!({"shape": shape(5, 5)}));
    e.run_ok(
        "pcb.output_origin.set",
        json!({"at": {"x": "10mm", "y": "10mm"}}),
    );
    let origin = pcb(&e).output_origin().unwrap().uuid;
    e.run_ok(
        "pcb.move",
        json!({"objects": [origin], "by": {"x": "3mm", "y": "0mm"}}),
    );
    assert_eq!(cartesian(&e, g).origin, mm(5, 5));
}

#[test]
fn move_board_shape_and_move_board_carry_every_local_grid() {
    let dir = TempDir::new("local-grid-board-shape");
    let mut e = engine(&dir);
    let vertices: Vec<Value> = [(0, 0), (20, 0), (20, 10), (0, 10)]
        .iter()
        .map(|&(x, y)| json!({"x": x * MM, "y": y * MM}))
        .collect();
    e.run_ok("pcb.board_outline.add", json!({"vertices": vertices}));
    let outline = pcb(&e).outline().unwrap().uuid;
    // Inside and far outside the board, and locked: a Local Grid belongs to the board.
    let inside = add(&mut e, json!({"shape": shape(5, 5)}));
    let outside = add(&mut e, json!({"shape": shape(100, 100), "locked": true}));
    let out = e.run_ok(
        "pcb.move",
        json!({"objects": [outline], "by": {"x": "2mm", "y": "3mm"}}),
    );
    assert_eq!(cartesian(&e, inside).origin, mm(7, 8));
    assert_eq!(cartesian(&e, outside).origin, mm(102, 103));
    let moved = out["result"]["objects"].as_array().unwrap();
    assert!(moved.contains(&json!(inside)) && moved.contains(&json!(outside)));
    e.run_ok(
        "pcb.rotate",
        json!({"objects": [outline], "angle": 90_000, "center": {"x": 0, "y": 0}}),
    );
    let c = cartesian(&e, inside);
    assert_eq!((c.origin, c.rotation), (mm(-8, 7), Millideg(90_000)));
    let out = e.run_ok("pcb.board.move", json!({"dx": "1mm", "dy": "0mm"}));
    assert_eq!(cartesian(&e, inside).origin, mm(-7, 7));
    assert_eq!(cartesian(&e, outside).origin, mm(-102, 102));
    assert!(local(&e, outside).locked);
    let moved = out["result"]["objects"].as_array().unwrap();
    assert!(moved.contains(&json!(inside)) && moved.contains(&json!(outside)));
}

fn saved(e: &mut TestEngine, dir: &TempDir) -> Vec<u8> {
    e.run_ok("project.save", json!({}));
    std::fs::read(dir.path().join("Demo/Demo.redapcb")).unwrap()
}

/// One random Local Grid edit. Each, as run, changes the board, so each is one undo step.
#[derive(Clone, Debug)]
enum Edit {
    /// Origin (mm), rotation (degrees), Step X (µm), width and height (mm).
    Add(i64, i64, i64, u32, i64, i64),
    /// Which one, a new origin x (mm), and whether to turn Applies to.
    Set(usize, Option<i64>, bool),
    /// Which one, and which to put it before (`None`: the end).
    Reorder(usize, Option<usize>),
    /// Which one, moved by (mm).
    Move(usize, i64),
    Delete(usize),
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (
            -50i64..50,
            -50i64..50,
            0i64..360,
            1u32..5_000,
            1i64..30,
            1i64..30
        )
            .prop_map(|(x, y, r, s, w, h)| Edit::Add(x, y, r, s, w, h)),
        (any::<usize>(), prop::option::of(-50i64..50), any::<bool>())
            .prop_map(|(i, x, a)| Edit::Set(i, x, a)),
        (any::<usize>(), prop::option::of(any::<usize>())).prop_map(|(i, b)| Edit::Reorder(i, b)),
        (any::<usize>(), 1i64..20).prop_map(|(i, d)| Edit::Move(i, d)),
        any::<usize>().prop_map(Edit::Delete),
    ]
}

/// Runs `edit`, falling back to adding a grid when the edit would change nothing.
fn run_edit(e: &mut TestEngine, edit: Edit) {
    let grids: Vec<Uuid> = order(e);
    let pick = |i: usize| grids[i % grids.len()];
    let fallback = json!({"shape": shape(0, 0)});
    match edit {
        Edit::Add(x, y, r, s, w, h) => {
            e.run_ok(
                "pcb.local_grid.add",
                json!({"shape": {"kind": "cartesian", "origin": {"x": x * MM, "y": y * MM},
                       "rotation": r * 1000, "step_x": format!("{}mm", f64::from(s) / 1000.0),
                       "width": w * MM, "height": h * MM}}),
            );
        }
        Edit::Set(i, x, turn) if !grids.is_empty() => {
            let g = local(e, pick(i));
            // Always turn shown over, so the edit changes the grid.
            let mut params = json!({"uuid": g.uuid, "shown": !g.shown});
            if turn {
                let to = match g.applies_to {
                    AppliesTo::All => "components",
                    AppliesTo::Components => "not_components",
                    AppliesTo::NotComponents => "all",
                };
                params["applies_to"] = json!(to);
            }
            if let Some(x) = x {
                params["shape"] = shape(x, 1);
            }
            e.run_ok("pcb.local_grid.set", params);
        }
        Edit::Reorder(i, before) if grids.len() > 1 => {
            let g = pick(i);
            let at = grids.iter().position(|o| *o == g).unwrap();
            let before = before.map(|b| grids[b % grids.len()]).filter(|b| *b != g);
            // Only a reorder that changes the order is an undo step.
            let mut after: Vec<Uuid> = grids.iter().copied().filter(|o| *o != g).collect();
            let to = before.and_then(|b| after.iter().position(|o| *o == b));
            after.insert(to.unwrap_or(after.len()), g);
            if after == grids || (before.is_none() && at == grids.len() - 1) {
                e.run_ok("pcb.local_grid.add", fallback);
                return;
            }
            let mut params = json!({"local_grid": g});
            if let Some(b) = before {
                params["before"] = json!(b);
            }
            e.run_ok("pcb.local_grid.reorder", params);
        }
        Edit::Move(i, d) if !grids.is_empty() => {
            e.run_ok(
                "pcb.move",
                json!({"objects": [pick(i)], "by": {"x": d * MM, "y": 0}}),
            );
        }
        Edit::Delete(i) if !grids.is_empty() => {
            e.run_ok("pcb.local_grid.delete", json!({"uuid": pick(i)}));
        }
        _ => {
            e.run_ok("pcb.local_grid.add", fallback);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..Default::default() })]

    /// docs/spec/v0.3/testing.md#property-tests: undoing any sequence of grid edits
    /// restores the saved bytes.
    #[test]
    fn undoing_any_sequence_of_local_grid_edits_restores_the_saved_bytes(
        edits in prop::collection::vec(edit(), 1..16)
    ) {
        let dir = TempDir::new("local-grid-undo-bytes");
        let mut e = engine(&dir);
        let doc = document(&e);
        let a = add(&mut e, json!({"name": "Header", "shape": shape(0, 0)}));
        add(&mut e, json!({"shape": shape(3, 3), "applies_to": "components"}));
        e.run_ok("pcb.local_grid.reorder", json!({"local_grid": a}));
        let original = saved(&mut e, &dir);
        let count = edits.len();
        for edit in edits {
            run_edit(&mut e, edit);
        }
        for _ in 0..count {
            e.run_ok("edit.undo", json!({"document": doc}));
        }
        prop_assert_eq!(saved(&mut e, &dir), original);
    }
}
