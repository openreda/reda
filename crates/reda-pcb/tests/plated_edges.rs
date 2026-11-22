//! Plated Edges (docs/spec/v0.2.5/plated-edges.md#marking): marking edges of the Board
//! Outline and Board Cutouts, and how contour edits carry the marks.

use reda_model::{PcbDocument, PlatedEdge, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine_with_outline(dir: &TempDir) -> (TestEngine, Uuid) {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let out = engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "40mm", "height": "30mm"}),
    );
    let uuid = out["result"]["uuid"].as_str().unwrap().parse().unwrap();
    (engine, uuid)
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

/// The marked edges of the Board Outline, with their Nets.
fn marks(engine: &TestEngine) -> Vec<(usize, Option<Uuid>)> {
    pcb(engine)
        .outline()
        .unwrap()
        .plated
        .iter()
        .map(|(i, m)| (*i, m.net))
        .collect()
}

fn net(engine: &mut TestEngine, name: &str) -> Uuid {
    engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn set(engine: &mut TestEngine, contour: Uuid, edge: usize, plated: Value) -> Value {
    engine.run_ok(
        "pcb.contour.edge.set",
        json!({"contour": contour, "edge": edge, "plated": plated}),
    )
}

#[test]
fn an_edge_is_marked_with_an_optional_net_and_cleared_with_null() {
    let dir = TempDir::new("plated-set");
    let (mut engine, outline) = engine_with_outline(&dir);
    let gnd = net(&mut engine, "GND");
    set(&mut engine, outline, 1, json!({}));
    set(&mut engine, outline, 3, json!({"net": gnd}));
    assert_eq!(marks(&engine), [(1, None), (3, Some(gnd))]);
    set(&mut engine, outline, 1, Value::Null);
    assert_eq!(marks(&engine), [(3, Some(gnd))]);
    let doc = pcb(&engine).uuid;
    engine.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(marks(&engine), [(1, None), (3, Some(gnd))]);
}

#[test]
fn a_mark_needs_an_edge_the_contour_has_and_a_net_that_exists() {
    let dir = TempDir::new("plated-bad");
    let (mut engine, outline) = engine_with_outline(&dir);
    let err = engine.run_err(
        "pcb.contour.edge.set",
        json!({"contour": outline, "edge": 4, "plated": {}}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
    let err = engine.run_err(
        "pcb.contour.edge.set",
        json!({"contour": outline, "edge": 0, "plated": {"net": Uuid::from_u128(0x1234)}}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
    let zone_like = engine.run_err(
        "pcb.contour.edge.set",
        json!({"contour": Uuid::from_u128(0x1234), "edge": 0, "plated": {}}),
    );
    assert_eq!(zone_like.code, reda_command::ErrorCode::InvalidParams);
}

#[test]
fn plate_all_and_unplate_all_set_and_clear_every_edge_of_a_cutout() {
    let dir = TempDir::new("plated-all");
    let (mut engine, _) = engine_with_outline(&dir);
    let cutout: Uuid = engine.run_ok(
        "pcb.board_cutout.add",
        json!({"vertices": [
            {"x": "10mm", "y": "10mm"}, {"x": "15mm", "y": "10mm"}, {"x": "12mm", "y": "14mm"}
        ]}),
    )["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let vcc = net(&mut engine, "VCC");
    engine.run_ok(
        "pcb.contour.edge.set_all",
        json!({"contour": cutout, "plated": {"net": vcc}}),
    );
    let plated = &pcb(&engine).cutout(cutout).unwrap().plated;
    assert_eq!(plated.len(), 3);
    assert!(plated.values().all(|m| m.net == Some(vcc)));
    engine.run_ok(
        "pcb.contour.edge.set_all",
        json!({"contour": cutout, "plated": null}),
    );
    assert!(pcb(&engine).cutout(cutout).unwrap().plated.is_empty());
}

#[test]
fn set_vertices_keeps_only_the_marks_its_vertices_carry() {
    let dir = TempDir::new("plated-replace");
    let (mut engine, outline) = engine_with_outline(&dir);
    set(&mut engine, outline, 0, json!({}));
    engine.run_ok(
        "pcb.polygon.set_vertices",
        json!({"object": outline, "vertices": [
            {"x": 0, "y": 0}, {"x": "40mm", "y": 0, "plated": {}}, {"x": "40mm", "y": "30mm"},
            {"x": 0, "y": "30mm"}
        ]}),
    );
    assert_eq!(marks(&engine), [(1, None)]);
}

#[test]
fn replacing_the_outline_with_a_rectangle_drops_the_marks() {
    let dir = TempDir::new("plated-rect");
    let (mut engine, outline) = engine_with_outline(&dir);
    set(&mut engine, outline, 0, json!({}));
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "20mm", "height": "20mm"}),
    );
    assert!(marks(&engine).is_empty());
    engine.run_ok(
        "pcb.board_outline.add",
        json!({"vertices": [
            {"x": 0, "y": 0, "plated": {}}, {"x": "20mm", "y": 0}, {"x": "20mm", "y": "20mm"}
        ]}),
    );
    assert_eq!(marks(&engine), [(0, None)]);
}

#[test]
fn only_board_contours_take_plated_vertices() {
    let dir = TempDir::new("plated-zone");
    let (mut engine, _) = engine_with_outline(&dir);
    let zone = engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "vertices": [
            {"x": 0, "y": 0}, {"x": "5mm", "y": 0}, {"x": "5mm", "y": "5mm"}
        ]}),
    )["result"]["uuid"]
        .clone();
    let err = engine.run_err(
        "pcb.polygon.set_vertices",
        json!({"object": zone, "vertices": [
            {"x": 0, "y": 0, "plated": {}}, {"x": "5mm", "y": 0}, {"x": "5mm", "y": "5mm"}
        ]}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
}

#[test]
fn a_chamfer_keeps_the_marks_and_plates_the_bevel_between_two_plated_edges() {
    let dir = TempDir::new("plated-chamfer");
    let (mut engine, outline) = engine_with_outline(&dir);
    set(&mut engine, outline, 0, json!({}));
    set(&mut engine, outline, 1, json!({}));
    engine.run_ok(
        "pcb.polygon.chamfer",
        json!({"object": outline, "vertices": [1, 3], "distance": "2mm"}),
    );
    // 0, bevel start, bevel end, 2, bevel start, bevel end.
    assert_eq!(pcb(&engine).outline().unwrap().vertices.len(), 6);
    assert_eq!(marks(&engine), [(0, None), (1, None), (2, None)]);
}

#[test]
fn moving_the_board_keeps_the_marks() {
    let dir = TempDir::new("plated-move");
    let (mut engine, outline) = engine_with_outline(&dir);
    set(&mut engine, outline, 2, json!({}));
    engine.run_ok("pcb.board.move", json!({"dx": "5mm", "dy": "1mm"}));
    assert_eq!(marks(&engine), [(2, None)]);
}

#[test]
fn the_file_stores_the_mark_on_the_vertex_that_starts_the_edge() {
    let dir = TempDir::new("plated-file");
    let (mut engine, outline) = engine_with_outline(&dir);
    let gnd = net(&mut engine, "GND");
    set(&mut engine, outline, 1, json!({"net": gnd}));
    let row = serde_json::to_value(pcb(&engine).outline().unwrap()).unwrap();
    assert_eq!(row["vertices"][1]["plated"], json!({"net": gnd}));
    assert_eq!(row["vertices"][0].get("plated"), None);
    assert!(row.get("plated").is_none());
    let back: reda_model::BoardOutline = serde_json::from_value(row).unwrap();
    assert_eq!(back.plated.get(&1), Some(&PlatedEdge { net: Some(gnd) }));
}
