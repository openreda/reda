//! `pcb.via.set`: the Via sizes the Properties panel edits.

mod common;

use common::{created, engine, pcb};
use reda_command::ErrorCode;
use reda_model::geom::Nm;
use reda_testkit::TempDir;
use serde_json::json;

#[test]
fn via_set_changes_the_diameter_and_hole_of_every_via_given() {
    let dir = TempDir::new("via-set");
    let mut engine = engine(&dir);
    let a = created(&engine.run_ok("pcb.via.add", json!({"at": {"x": 0, "y": 0}})));
    let b = created(&engine.run_ok("pcb.via.add", json!({"at": {"x": "5mm", "y": 0}})));

    let out = engine.run_ok(
        "pcb.via.set",
        json!({"vias": [b, a], "diameter": "0.8mm", "hole": 400_000}),
    );

    let mut both = vec![a, b];
    both.sort();
    assert_eq!(out["result"]["vias"], json!(both));
    for via in [a, b] {
        let via = pcb(&engine).via(via).unwrap();
        assert_eq!(
            (via.diameter, via.hole),
            (Nm::from_um(800), Nm::from_um(400))
        );
    }
}

#[test]
fn via_set_refuses_a_hole_as_large_as_the_diameter() {
    let dir = TempDir::new("via-set-refused");
    let mut engine = engine(&dir);
    let via = created(&engine.run_ok("pcb.via.add", json!({"at": {"x": 0, "y": 0}})));
    let before = pcb(&engine).via(via).unwrap().clone();

    let err = engine.run_err("pcb.via.set", json!({"vias": [via], "hole": "5mm"}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let unknown = "00000000-0000-4000-8000-0000000000ff";
    let err = engine.run_err("pcb.via.set", json!({"vias": [unknown], "hole": "0.1mm"}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(pcb(&engine).via(via).unwrap(), &before);
}
