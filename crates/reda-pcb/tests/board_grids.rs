//! Board Grids (docs/spec/v0.3/grids-guides.md#board-grids): `pcb.board_grid.add / set /
//! delete` with undo, each step keeping the unit it was typed in, at most one Default,
//! refusals that write nothing, the file, and undo restoring the saved bytes after any
//! sequence of Board Grid edits.

use proptest::prelude::*;
use reda_command::ErrorCode;
use reda_model::geom::Nm;
use reda_model::{BoardGrid, LengthUnit, PcbDocument, TypedLength, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn created(out: &Value) -> Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

fn grid(engine: &TestEngine, uuid: Uuid) -> BoardGrid {
    pcb(engine)
        .board_grid(uuid)
        .expect("the Board Grid")
        .clone()
}

fn add(engine: &mut TestEngine, params: Value) -> Uuid {
    created(&engine.run_ok("pcb.board_grid.add", params))
}

fn mm(text: &str) -> TypedLength {
    TypedLength::parse(text).unwrap()
}

#[test]
fn a_board_grid_keeps_the_unit_each_step_was_typed_in() {
    let dir = TempDir::new("board-grid-add");
    let mut e = engine(&dir);
    let a = add(&mut e, json!({"step_x": "0.3mm"}));
    assert_eq!(
        grid(&e, a),
        BoardGrid {
            uuid: a,
            step_x: TypedLength::new(Nm(300_000), LengthUnit::Mm),
            step_y: None,
            visible: None,
            in_cycle: true,
            default: false,
        }
    );
    let b = add(
        &mut e,
        json!({"step_x": "25mil", "step_y": "0.5mm", "visible": "100mil", "in_cycle": false}),
    );
    let g = grid(&e, b);
    assert_eq!(g.step_x.text().as_deref(), Some("25mil"));
    assert_eq!(g.step_y.and_then(|y| y.text()).as_deref(), Some("0.5mm"));
    assert_eq!(g.visible.and_then(|v| v.text()).as_deref(), Some("100mil"));
    assert!(!g.in_cycle);
    // A bare number of nanometres has no unit of its own: it is kept in mm.
    let c = add(&mut e, json!({"step_x": 254_000}));
    assert_eq!(grid(&e, c).step_x.text().as_deref(), Some("0.254mm"));
}

#[test]
fn set_changes_the_given_fields_and_null_clears_step_y_and_visible() {
    let dir = TempDir::new("board-grid-set");
    let mut e = engine(&dir);
    let a = add(
        &mut e,
        json!({"step_x": "0.5mm", "step_y": "1.27mm", "visible": "2.5mm"}),
    );
    e.run_ok(
        "pcb.board_grid.set",
        json!({"uuid": a, "step_x": "20mil", "in_cycle": false, "default": true}),
    );
    let g = grid(&e, a);
    assert_eq!(g.step_x, mm("20mil"));
    assert_eq!(g.step_y, Some(mm("1.27mm")));
    assert!(!g.in_cycle && g.default);
    e.run_ok(
        "pcb.board_grid.set",
        json!({"uuid": a, "step_y": null, "visible": null}),
    );
    let g = grid(&e, a);
    assert_eq!((g.step_y, g.visible), (None, None));
    assert_eq!(pcb(&e).default_board_grid().map(|g| g.uuid), Some(a));
}

#[test]
fn add_set_and_delete_are_each_one_undo_step() {
    let dir = TempDir::new("board-grid-undo");
    let mut e = engine(&dir);
    let doc = pcb(&e).uuid;
    let a = add(&mut e, json!({"step_x": "0.3mm", "default": true}));
    e.run_ok("pcb.board_grid.set", json!({"uuid": a, "step_x": "0.4mm"}));
    e.run_ok("pcb.board_grid.delete", json!({"uuid": a}));
    assert!(pcb(&e).board_grid(a).is_none());
    e.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(grid(&e, a).step_x, mm("0.4mm"));
    e.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(grid(&e, a).step_x, mm("0.3mm"));
    e.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(pcb(&e).board_grids().count(), 0);
    e.run_ok("edit.redo", json!({"document": doc}));
    assert!(grid(&e, a).default);
}

/// Runs `id` expecting an invalid-params refusal at `pointer` (or `code` without one), and
/// checks the board is as it was.
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
fn a_second_default_and_bad_steps_are_refused_writing_nothing() {
    let dir = TempDir::new("board-grid-refusals");
    let mut e = engine(&dir);
    let first = add(&mut e, json!({"step_x": "0.3mm", "default": true}));
    let other = add(&mut e, json!({"step_x": "0.4mm"}));
    let invalid = ErrorCode::InvalidParams;
    // A second Default, added or set.
    refused(
        &mut e,
        "pcb.board_grid.add",
        json!({"step_x": "0.6mm", "default": true}),
        invalid,
        Some("/default"),
    );
    refused(
        &mut e,
        "pcb.board_grid.set",
        json!({"uuid": other, "default": true}),
        invalid,
        Some("/default"),
    );
    // Setting the Default row's own mark again is no second Default.
    e.run_ok(
        "pcb.board_grid.set",
        json!({"uuid": first, "default": true}),
    );
    // Steps must be positive.
    for (field, value) in [("step_x", "0mm"), ("step_y", "-1mm"), ("visible", "0mil")] {
        let mut params = json!({"step_x": "1mm"});
        params[field] = json!(value);
        refused(
            &mut e,
            "pcb.board_grid.add",
            params,
            invalid,
            Some(&format!("/{field}")),
        );
    }
    refused(
        &mut e,
        "pcb.board_grid.set",
        json!({"uuid": other, "step_x": 0}),
        invalid,
        Some("/step_x"),
    );
    // An unknown UUID.
    let nobody = "00000000-0000-4000-8000-0000000000ff";
    refused(
        &mut e,
        "pcb.board_grid.set",
        json!({"uuid": nobody, "in_cycle": false}),
        ErrorCode::NotFound,
        None,
    );
    refused(
        &mut e,
        "pcb.board_grid.delete",
        json!({"uuid": nobody}),
        ErrorCode::NotFound,
        None,
    );
}

#[test]
fn moving_the_default_mark_clears_the_old_one_first() {
    let dir = TempDir::new("board-grid-default-move");
    let mut e = engine(&dir);
    let first = add(&mut e, json!({"step_x": "0.3mm", "default": true}));
    let other = add(&mut e, json!({"step_x": "0.4mm"}));
    e.run_ok(
        "batch",
        json!({"commands": [
            {"id": "pcb.board_grid.set", "params": {"uuid": first, "default": false}},
            {"id": "pcb.board_grid.set", "params": {"uuid": other, "default": true}},
        ]}),
    );
    assert_eq!(pcb(&e).default_board_grid().map(|g| g.uuid), Some(other));
}

#[test]
fn board_grids_round_trip_through_the_file_as_length_text() {
    let dir = TempDir::new("board-grid-file");
    let mut e = engine(&dir);
    let file = dir.path().join("Demo/Demo.redapcb");
    e.run_ok("project.save", json!({}));
    let empty = std::fs::read_to_string(&file).unwrap();
    assert!(!empty.contains("\"grids\""), "{empty}");
    let a = add(&mut e, json!({"step_x": "0.3mm", "default": true}));
    let b = add(
        &mut e,
        json!({"step_x": "25mil", "step_y": "1.27mm", "visible": "100mil", "in_cycle": false}),
    );
    e.run_ok("project.save", json!({}));
    let text = std::fs::read_to_string(&file).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value["grids"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["board"],
        "no Local Grids: `local` is left out"
    );
    let rows = value["grids"]["board"].as_array().expect("a board table");
    let row = |uuid: Uuid| {
        rows.iter()
            .find(|r| r["uuid"] == json!(uuid))
            .expect("a row")
            .clone()
    };
    assert_eq!(
        row(a),
        json!({"uuid": a, "step_x": "0.3mm", "in_cycle": true, "default": true})
    );
    assert_eq!(
        row(b),
        json!({"uuid": b, "step_x": "25mil", "step_y": "1.27mm", "visible": "100mil",
               "in_cycle": false})
    );
    // Each row on its own line, sorted by UUID.
    let first = format!("\"grids\": {{\"board\":[\n    {{\"uuid\":\"{a}\",\"step_x\":\"0.3mm\"");
    assert!(text.contains(&first), "{text}");
    let mut reopened = probe_engine();
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(grid(&reopened, a), grid(&e, a));
    assert_eq!(grid(&reopened, b), grid(&e, b));
    reopened.run_ok("project.save", json!({}));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn the_file_refuses_a_step_without_a_unit_and_a_second_default() {
    let dir = TempDir::new("board-grid-file-refusals");
    let mut e = engine(&dir);
    let a = add(&mut e, json!({"step_x": "0.3mm", "default": true}));
    add(&mut e, json!({"step_x": "0.4mm"}));
    e.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let file = folder.join("Demo.redapcb");
    let text = std::fs::read_to_string(&file).unwrap();
    for (from, to, says) in [
        ("\"step_x\":\"0.3mm\"", "\"step_x\":300000", "unit"),
        (
            "\"step_x\":\"0.4mm\"",
            "\"step_x\":\"0.4mm\",\"default\":true",
            "Default",
        ),
        ("\"step_x\":\"0.4mm\"", "\"step_x\":\"0mm\"", "positive"),
    ] {
        assert!(text.contains(from), "{text}");
        std::fs::write(&file, text.replace(from, to)).unwrap();
        let err = probe_engine().run_err("project.open", json!({"path": folder}));
        assert_eq!(err.code, ErrorCode::Rejected, "{to}: {err:?}");
        assert!(err.message.contains(says), "{to}: {}", err.message);
    }
    std::fs::write(&file, &text).unwrap();
    let mut reopened = probe_engine();
    reopened.run_ok("project.open", json!({"path": folder}));
    assert!(grid(&reopened, a).default);
}

fn saved(e: &mut TestEngine, dir: &TempDir) -> Vec<u8> {
    e.run_ok("project.save", json!({}));
    std::fs::read(dir.path().join("Demo/Demo.redapcb")).unwrap()
}

/// One random Board Grid edit. Each, as run, changes the board, so each is one undo step.
#[derive(Clone, Debug)]
enum Edit {
    /// Step X, an optional Step Y and Visible, and whether to ask for the Default mark.
    Add(String, Option<String>, Option<String>, bool),
    /// Which one, a new Step X, Step Y (`Some(None)` clears it), and the Default mark.
    Set(usize, Option<String>, Option<Option<String>>, bool),
    Delete(usize),
}

/// A positive length in mm or mil, as typed.
fn length() -> impl Strategy<Value = String> {
    prop_oneof![
        (1u32..5_000).prop_map(|k| format!("{}mm", f64::from(k) / 1000.0)),
        (1u32..400).prop_map(|k| format!("{k}mil")),
    ]
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (
            length(),
            prop::option::of(length()),
            prop::option::of(length()),
            any::<bool>()
        )
            .prop_map(|(x, y, v, d)| Edit::Add(x, y, v, d)),
        (
            any::<usize>(),
            prop::option::of(length()),
            prop::option::of(prop::option::of(length())),
            any::<bool>(),
        )
            .prop_map(|(i, x, y, d)| Edit::Set(i, x, y, d)),
        any::<usize>().prop_map(Edit::Delete),
    ]
}

/// Runs `edit`, falling back to adding a grid when there is none to act on, and asking
/// for the Default mark only where it is free.
fn run_edit(e: &mut TestEngine, edit: Edit) {
    let grids: Vec<BoardGrid> = pcb(e).board_grids().cloned().collect();
    let default = pcb(e).default_board_grid().map(|g| g.uuid);
    match edit {
        Edit::Set(i, x, y, mark) if !grids.is_empty() => {
            let g = &grids[i % grids.len()];
            // Always turn In cycle over, so the edit changes the grid.
            let mut params = json!({"uuid": g.uuid, "in_cycle": !g.in_cycle});
            if let Some(x) = x {
                params["step_x"] = json!(x);
            }
            if let Some(y) = y {
                params["step_y"] = json!(y);
            }
            if mark && default.is_none_or(|d| d == g.uuid) {
                params["default"] = json!(!g.default);
            }
            e.run_ok("pcb.board_grid.set", params);
        }
        Edit::Delete(i) if !grids.is_empty() => {
            let uuid = grids[i % grids.len()].uuid;
            e.run_ok("pcb.board_grid.delete", json!({"uuid": uuid}));
        }
        Edit::Add(x, y, visible, mark) => {
            let mut params = json!({"step_x": x, "default": mark && default.is_none()});
            if let Some(y) = y {
                params["step_y"] = json!(y);
            }
            if let Some(v) = visible {
                params["visible"] = json!(v);
            }
            e.run_ok("pcb.board_grid.add", params);
        }
        _ => {
            e.run_ok("pcb.board_grid.add", json!({"step_x": "1mm"}));
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..Default::default() })]

    /// docs/spec/v0.3/testing.md#property-tests: undoing any sequence of grid edits
    /// restores the saved bytes.
    #[test]
    fn undoing_any_sequence_of_board_grid_edits_restores_the_saved_bytes(
        edits in prop::collection::vec(edit(), 1..16)
    ) {
        let dir = TempDir::new("board-grid-undo-bytes");
        let mut e = engine(&dir);
        let doc = pcb(&e).uuid;
        add(&mut e, json!({"step_x": "0.3mm", "default": true}));
        add(&mut e, json!({"step_x": "25mil", "step_y": "1.27mm", "in_cycle": false}));
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
