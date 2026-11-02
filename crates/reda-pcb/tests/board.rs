//! The Board Outline, Board Cutouts, design rules and DRC severities, through Commands.

use reda_model::geom::{Millideg, Nm, Point, Vertex};
use reda_model::{DrcCheck, PcbDocument, Severity};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::json;

fn engine_with_pcb(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm::from_mm(x), Nm::from_mm(y))
}

#[test]
fn pcb_commands_are_unavailable_without_a_pcb_document() {
    let dir = TempDir::new("pcb-none");
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let document = pcb(&engine).uuid;
    engine.run_ok("document.delete", json!({"document": document}));
    let err = engine.run_err(
        "pcb.board_outline.add_rect",
        json!({"width": "10mm", "height": "10mm"}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::Unavailable);
    assert!(err.message.contains("document.add"), "{}", err.message);
}

#[test]
fn a_rectangular_outline_takes_unit_strings_and_rounds_its_corners() {
    let dir = TempDir::new("outline-rect");
    let mut engine = engine_with_pcb(&dir);
    let out = engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "100mm", "height": "80mm", "corner_radius": "5mm"}),
    );
    let outline = pcb(&engine).outline().unwrap();
    assert_eq!(out["result"]["uuid"], json!(outline.uuid));
    assert_eq!(outline.vertices.len(), 8);
    assert_eq!(
        outline.vertices[1],
        Vertex::arc(mm(95, 0), Millideg::from_deg(90))
    );
}

#[test]
fn a_new_outline_replaces_the_old_one() {
    let dir = TempDir::new("outline-replace");
    let mut engine = engine_with_pcb(&dir);
    let first = engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "10mm", "height": "10mm"}),
    );
    let second = engine.run_ok(
        "pcb.board_outline.add",
        json!({"vertices": [
            {"x": 0, "y": 0}, {"x": "50mm", "y": 0}, {"x": "50mm", "y": "40mm", "sweep": "90deg"},
            {"x": "10mm", "y": "40mm"}
        ]}),
    );
    let pcb = pcb(&engine);
    let outline = pcb.outline().unwrap();
    assert_eq!(second["result"]["uuid"], json!(outline.uuid));
    assert_ne!(first["result"]["uuid"], second["result"]["uuid"]);
    assert_eq!(outline.vertices[2].sweep, Millideg::from_deg(90));
    assert_eq!(
        engine
            .project()
            .unwrap()
            .object(first["result"]["uuid"].as_str().unwrap().parse().unwrap()),
        None
    );
}

#[test]
fn an_outline_from_lines_and_arcs_chains_them() {
    let dir = TempDir::new("outline-segments");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "pcb.board_outline.add_from_segments",
        json!({"segments": [
            {"from": {"x": "30mm", "y": 0}, "to": {"x": "30mm", "y": "20mm"}},
            {"from": {"x": 0, "y": 0}, "to": {"x": "30mm", "y": 0}},
            {"from": {"x": 0, "y": "20mm"}, "to": {"x": "30mm", "y": "20mm"}, "sweep": "-180deg"},
            {"from": {"x": 0, "y": "20mm"}, "to": {"x": 0, "y": 0}}
        ]}),
    );
    let vertices = &pcb(&engine).outline().unwrap().vertices;
    assert_eq!(vertices.len(), 4);
    assert_eq!(
        vertices[1],
        Vertex::arc(mm(30, 20), Millideg::from_deg(180))
    );
}

#[test]
fn an_open_set_of_segments_is_refused_at_the_gap() {
    let dir = TempDir::new("outline-open");
    let mut engine = engine_with_pcb(&dir);
    let err = engine.run_err(
        "pcb.board_outline.add_from_segments",
        json!({"segments": [
            {"from": {"x": 0, "y": 0}, "to": {"x": 10, "y": 0}},
            {"from": {"x": 10, "y": 0}, "to": {"x": 10, "y": 10}}
        ]}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
    assert!(err.message.contains("open"), "{}", err.message);
}

#[test]
fn bad_outlines_are_refused() {
    let dir = TempDir::new("outline-bad");
    let mut engine = engine_with_pcb(&dir);
    for params in [
        json!({"width": 0, "height": "10mm"}),
        json!({"width": "10mm", "height": "10mm", "corner_radius": "6mm"}),
        json!({"width": "10mm", "height": "10mm", "corner_radius": "-1mm"}),
    ] {
        let err = engine.run_err("pcb.board_outline.add_rect", params.clone());
        assert_eq!(err.code, reda_command::ErrorCode::InvalidParams, "{params}");
    }
    let err = engine.run_err(
        "pcb.board_outline.add",
        json!({"vertices": [{"x": 0, "y": 0}, {"x": 10, "y": 0}]}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
    assert!(pcb(&engine).outline().is_none());
}

#[test]
fn the_outline_can_be_deleted_and_the_deletion_undone() {
    let dir = TempDir::new("outline-delete");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "10mm", "height": "10mm"}),
    );
    engine.run_ok("pcb.board_outline.delete", json!({}));
    assert!(pcb(&engine).outline().is_none());
    let err = engine.run_err("pcb.board_outline.delete", json!({}));
    assert_eq!(err.code, reda_command::ErrorCode::NotFound);
    let document = pcb(&engine).uuid;
    engine.run_ok("edit.undo", json!({"document": document}));
    assert!(pcb(&engine).outline().is_some());
}

#[test]
fn board_cutouts_are_separate_objects() {
    let dir = TempDir::new("cutouts");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "50mm", "height": "50mm"}),
    );
    let square = json!({"vertices": [
        {"x": "10mm", "y": "10mm"}, {"x": "20mm", "y": "10mm"},
        {"x": "20mm", "y": "20mm"}, {"x": "10mm", "y": "20mm"}
    ]});
    let a = engine.run_ok("pcb.board_cutout.add", square.clone());
    engine.run_ok("pcb.board_cutout.add", square);
    assert_eq!(pcb(&engine).cutouts().count(), 2);
    assert!(pcb(&engine).outline().is_some(), "the outline is untouched");

    let a = a["result"]["uuid"].clone();
    engine.run_ok("pcb.board_cutout.delete", json!({"cutout": a}));
    assert_eq!(pcb(&engine).cutouts().count(), 1);
    let err = engine.run_err("pcb.board_cutout.delete", json!({"cutout": a}));
    assert_eq!(err.code, reda_command::ErrorCode::NotFound);
}

#[test]
fn board_wide_rules_change_one_at_a_time() {
    let dir = TempDir::new("rules");
    let mut engine = engine_with_pcb(&dir);
    let out = engine.run_ok(
        "pcb.rules.set",
        json!({"clearance": "0.1mm", "edge_clearance": "8mil"}),
    );
    assert_eq!(out["result"]["clearance"], 100_000);
    let rules = pcb(&engine).setup().rules;
    assert_eq!(rules.clearance, Nm::from_um(100));
    assert_eq!(rules.edge_clearance, Nm::from_mil(8));
    assert_eq!(rules.track_width, Nm::from_um(200), "unchanged");
}

#[test]
fn a_negative_rule_is_refused_with_its_pointer() {
    let dir = TempDir::new("rules-negative");
    let mut engine = engine_with_pcb(&dir);
    let err = engine.run_err("pcb.rules.set", json!({"track_width": "-0.1mm"}));
    assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
    assert_eq!(err.details.as_ref().unwrap()["pointer"], "/track_width");
}

#[test]
fn each_drc_check_has_its_own_severity() {
    let dir = TempDir::new("severity");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "drc.set_severity",
        json!({"check": "courtyard_overlap", "severity": "off"}),
    );
    engine.run_ok(
        "drc.set_severity",
        json!({"check": "unrouted", "severity": "warning"}),
    );
    let severities = pcb(&engine).setup().severities;
    assert_eq!(severities.get(DrcCheck::CourtyardOverlap), Severity::Off);
    assert_eq!(severities.get(DrcCheck::Unrouted), Severity::Warning);
    assert_eq!(severities.get(DrcCheck::Clearance), Severity::Error);
    let err = engine.run_err(
        "drc.set_severity",
        json!({"check": "no_such_check", "severity": "off"}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
}

#[test]
fn board_setup_edits_undo_on_the_pcb_stack() {
    let dir = TempDir::new("setup-undo");
    let mut engine = engine_with_pcb(&dir);
    let document = pcb(&engine).uuid;
    engine.run_ok("pcb.rules.set", json!({"clearance": "0.3mm"}));
    engine.run_ok("edit.undo", json!({"document": document}));
    assert_eq!(pcb(&engine).setup().rules.clearance, Nm::from_um(160));
}

#[test]
fn fab_preset_rules_are_previewed_then_reseed_the_board() {
    let dir = TempDir::new("rules-apply");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "fab_preset": "PCBWay"}),
    );
    engine.run_ok("pcb.rules.set", json!({"clearance": "0.3mm"}));
    let preset = engine.project().unwrap().fab_presets().next().unwrap().uuid;

    let preview = engine.run_ok("pcb.rules.fab_preset_diff", json!({"preset": preset}));
    assert_eq!(
        preview,
        json!({"changes": [{"rule": "clearance", "board": 300000, "preset": 100000}]})
    );
    assert_eq!(pcb(&engine).setup().rules.clearance, Nm::from_um(300));

    let out = engine.run_ok("pcb.rules.apply_fab_preset", json!({"preset": preset}));
    assert_eq!(out["result"]["changes"], preview["changes"]);
    assert_eq!(pcb(&engine).setup().rules.clearance, Nm::from_um(100));
    let document = pcb(&engine).uuid;
    engine.run_ok("edit.undo", json!({"document": document}));
    assert_eq!(pcb(&engine).setup().rules.clearance, Nm::from_um(300));

    let err = engine.run_err(
        "pcb.rules.apply_fab_preset",
        json!({"preset": "00000000-0000-4000-8000-0000000000ff"}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::NotFound);
}
