//! The board's `edge_connector` property (docs/spec/v0.2.5/edge-connectors.md): the
//! bevel and selective hard gold set by `pcb.edge_connector.set`, and the DRC warning
//! when they are set on a board with no Edge Connector Pads.

use reda_command::ErrorCode;
use reda_model::geom::{Millideg, Nm};
use reda_model::{EdgeBevel, EdgeConnector, PcbDocument};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
    );
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn pcie() -> EdgeConnector {
    EdgeConnector {
        bevel: Some(EdgeBevel {
            angle: Millideg(20_000),
            depth: Nm(1_300_000),
        }),
        hard_gold: true,
    }
}

#[test]
fn the_bevel_and_hard_gold_are_set_saved_cleared_and_undone_as_one_edit() {
    let dir = TempDir::new("edge-connector-set");
    let mut engine = engine(&dir);
    assert_eq!(pcb(&engine).setup().edge_connector, None);
    engine.run_ok(
        "pcb.edge_connector.set",
        json!({"edge_connector": {"bevel": {"angle": "20deg", "depth": "1.3mm"},
                                  "hard_gold": true}}),
    );
    assert_eq!(pcb(&engine).setup().edge_connector, Some(pcie()));

    engine.run_ok("project.save", json!({}));
    let file = std::fs::read_to_string(dir.path().join("Demo/Demo.redapcb")).unwrap();
    assert!(
        file.contains(
            r#""edge_connector":{"bevel":{"angle":20000,"depth":1300000},"hard_gold":true}"#
        ),
        "{file}"
    );
    engine.run_ok(
        "project.open",
        json!({"path": dir.path().join("Demo/Demo.reda")}),
    );
    assert_eq!(pcb(&engine).setup().edge_connector, Some(pcie()));

    let document = pcb(&engine).uuid;
    engine.run_ok("pcb.edge_connector.set", json!({"edge_connector": null}));
    assert_eq!(pcb(&engine).setup().edge_connector, None);
    engine.run_ok("edit.undo", json!({"document": document}));
    assert_eq!(pcb(&engine).setup().edge_connector, Some(pcie()));

    // Hard gold alone, with no bevel.
    engine.run_ok(
        "pcb.edge_connector.set",
        json!({"edge_connector": {"hard_gold": true}}),
    );
    assert_eq!(
        pcb(&engine).setup().edge_connector,
        Some(EdgeConnector {
            bevel: None,
            hard_gold: true
        })
    );
}

#[test]
fn a_bevel_needs_an_angle_between_0_and_90_degrees_and_a_positive_depth() {
    let dir = TempDir::new("edge-connector-bad");
    let mut engine = engine(&dir);
    for (bevel, pointer) in [
        (
            json!({"angle": "0deg", "depth": "1mm"}),
            "/edge_connector/bevel/angle",
        ),
        (
            json!({"angle": "90deg", "depth": "1mm"}),
            "/edge_connector/bevel/angle",
        ),
        (
            json!({"angle": "20deg", "depth": "0mm"}),
            "/edge_connector/bevel/depth",
        ),
    ] {
        let err = engine.run_err(
            "pcb.edge_connector.set",
            json!({"edge_connector": {"bevel": bevel}}),
        );
        assert_eq!(err.code, ErrorCode::InvalidParams, "{bevel}");
        assert_eq!(err.details.as_ref().unwrap()["pointer"], pointer, "{bevel}");
    }
    // The key itself is required: leaving it out is not a clear.
    let err = engine.run_err("pcb.edge_connector.set", json!({}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(pcb(&engine).setup().edge_connector, None);
}

fn warnings(engine: &mut TestEngine) -> Vec<Value> {
    let report = engine.run_ok("drc.run", json!({}));
    report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == "edge_connector_without_pads")
        .cloned()
        .collect()
}

#[test]
fn drc_warns_when_the_property_is_set_on_a_board_with_no_edge_connector_pads() {
    let dir = TempDir::new("edge-connector-drc");
    let mut engine = engine(&dir);
    assert!(warnings(&mut engine).is_empty());

    engine.run_ok(
        "pcb.edge_connector.set",
        json!({"edge_connector": {"hard_gold": true}}),
    );
    let found = warnings(&mut engine);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["severity"], "warning");

    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "designator": "J1", "pads": [
            {"at": {"x": 500_000, "y": 5_000_000}, "width": "1mm", "height": "3mm",
             "role": "edge_connector"}]}),
    );
    assert!(warnings(&mut engine).is_empty());
}
