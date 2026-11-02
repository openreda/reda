//! Mechanical Parts placed straight from a Library onto the PCB, with no Component link.

mod common;

use common::{FIDUCIAL, MOUNTING_HOLE_M3, PCB, RESISTOR, engine, footprint, pcb, synced};
use reda_command::ErrorCode;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{Side, Uuid};
use reda_testkit::{TempDir, TestEngine};
use serde_json::{Value, json};

fn place(engine: &mut TestEngine, part: &str, params: Value) -> Uuid {
    let mut p = json!({"part": part, "at": {"x": "5mm", "y": "5mm"}});
    p.as_object_mut()
        .unwrap()
        .extend(params.as_object().unwrap().clone());
    let out = engine.run_ok("pcb.mechanical.place", p);
    out["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn a_mechanical_part_places_with_no_component_link() {
    let dir = TempDir::new("mechanical-place");
    let mut engine = engine(&dir);

    let out = engine.run_ok(
        "pcb.mechanical.place",
        json!({"part": MOUNTING_HOLE_M3, "at": {"x": "5mm", "y": "4mm"},
               "rotation": "90deg", "side": "bottom"}),
    );

    assert_eq!(out["result"]["designator"], "H1");
    let uuid: Uuid = out["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let hole = footprint(&engine, uuid);
    assert_eq!(hole.component, None);
    assert_eq!(hole.at, Point::new(Nm::from_mm(5), Nm::from_mm(4)));
    assert_eq!(hole.rotation, Millideg::from_deg(90));
    assert_eq!(hole.side, Side::Bottom);
    assert_eq!(hole.fields.value, "M3");
    assert_eq!(hole.pads.len(), 1);
    let copy = pcb(&engine).footprint_copy(hole.footprint).unwrap();
    assert_eq!(copy.footprint.name, "MountingHole_M3");
}

#[test]
fn designators_count_up_per_prefix_from_the_parts_keywords() {
    let dir = TempDir::new("mechanical-designators");
    let mut engine = engine(&dir);
    let h1 = place(&mut engine, MOUNTING_HOLE_M3, json!({}));
    let h2 = place(&mut engine, MOUNTING_HOLE_M3, json!({}));
    let fid = place(&mut engine, FIDUCIAL, json!({}));
    let named = place(&mut engine, FIDUCIAL, json!({"designator": "MARK1"}));

    assert_eq!(footprint(&engine, h1).designator, "H1");
    assert_eq!(footprint(&engine, h2).designator, "H2");
    assert_eq!(footprint(&engine, fid).designator, "FID1");
    assert_eq!(footprint(&engine, named).designator, "MARK1");
    assert_eq!(
        footprint(&engine, h1).footprint,
        footprint(&engine, h2).footprint,
        "one copy of the Footprint serves both"
    );
}

#[test]
fn standard_parts_and_parts_without_footprints_are_refused() {
    let dir = TempDir::new("mechanical-standard");
    let mut engine = engine(&dir);

    let err = engine.run_err(
        "pcb.mechanical.place",
        json!({"part": RESISTOR, "at": {"x": 0, "y": 0}}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("Sync"), "{}", err.message);

    let err = engine.run_err(
        "pcb.mechanical.place",
        json!({"part": MOUNTING_HOLE_M3, "at": {"x": 0, "y": 0}, "footprint": RESISTOR}),
    );
    assert_eq!(err.details.unwrap()["pointer"], "/footprint");

    let err = engine.run_err(
        "pcb.mechanical.place",
        json!({"part": MOUNTING_HOLE_M3, "at": {"x": 0, "y": 0}, "designator": " "}),
    );
    assert_eq!(err.details.unwrap()["pointer"], "/designator");
}

#[test]
fn sync_leaves_mechanical_parts_alone() {
    let dir = TempDir::new("mechanical-sync");
    let mut engine = engine(&dir);
    let hole = place(&mut engine, MOUNTING_HOLE_M3, json!({}));
    assert_eq!(engine.run_ok("sync.preview", json!({}))["count"], 0);

    let r = synced(&mut engine, 1);

    assert!(pcb(&engine).footprint(hole).is_some());
    assert!(pcb(&engine).footprint(r[0]).is_some());
    assert_eq!(engine.run_ok("sync.preview", json!({}))["count"], 0);
}

#[test]
fn mechanical_fields_and_not_fitted_are_edited_on_the_pcb() {
    let dir = TempDir::new("mechanical-set");
    let mut engine = engine(&dir);
    let hole = place(&mut engine, MOUNTING_HOLE_M3, json!({}));

    engine.run_ok(
        "pcb.mechanical.set",
        json!({"footprints": [hole], "designator": "H9", "value": "M3 screw",
               "fields": {"mpn": "SCREW-M3", "LCSC": "C1", "description": null}, "not_fitted": true}),
    );

    let edited = footprint(&engine, hole);
    assert_eq!(edited.designator, "H9");
    assert_eq!(edited.fields.value, "M3 screw");
    assert_eq!(edited.fields.get("mpn"), "SCREW-M3");
    assert_eq!(edited.fields.get("LCSC"), "C1");
    assert_eq!(edited.fields.description, "");
    assert!(edited.not_fitted);

    engine.run_ok(
        "pcb.mechanical.set",
        json!({"footprints": [hole], "not_fitted": false}),
    );
    assert!(!footprint(&engine, hole).not_fitted);
}

#[test]
fn mechanical_edits_refuse_synced_footprints_and_taken_designators() {
    let dir = TempDir::new("mechanical-set-refused");
    let mut engine = engine(&dir);
    let hole = place(&mut engine, MOUNTING_HOLE_M3, json!({}));
    let other = place(&mut engine, MOUNTING_HOLE_M3, json!({}));
    let r = synced(&mut engine, 1);

    let err = engine.run_err(
        "pcb.mechanical.set",
        json!({"footprints": [hole, r[0]], "value": "x"}),
    );
    assert_eq!(err.details.unwrap()["pointer"], "/footprints/1");

    for taken in ["H2", "R1"] {
        let err = engine.run_err(
            "pcb.mechanical.set",
            json!({"footprints": [hole], "designator": taken}),
        );
        assert_eq!(err.code, ErrorCode::Conflict, "{taken}");
    }
    let err = engine.run_err(
        "pcb.mechanical.set",
        json!({"footprints": [hole, other], "designator": "H7"}),
    );
    assert_eq!(err.details.unwrap()["pointer"], "/designator");
}

#[test]
fn deleting_a_mechanical_part_drops_its_unused_footprint_copy() {
    let dir = TempDir::new("mechanical-delete");
    let mut engine = engine(&dir);
    let a = place(&mut engine, MOUNTING_HOLE_M3, json!({}));
    let b = place(&mut engine, MOUNTING_HOLE_M3, json!({}));
    let r = synced(&mut engine, 1);

    engine.run_ok("pcb.mechanical.delete", json!({"footprints": [a]}));
    assert_eq!(pcb(&engine).footprint_copies().count(), 2);
    engine.run_ok("pcb.mechanical.delete", json!({"footprints": [b]}));
    assert_eq!(pcb(&engine).footprint_copies().count(), 1);

    let err = engine.run_err("pcb.mechanical.delete", json!({"footprints": [r[0]]}));
    assert_eq!(err.details.unwrap()["pointer"], "/footprints/0");
}

#[test]
fn a_mechanical_part_survives_save_and_open() {
    let dir = TempDir::new("mechanical-save");
    let mut engine = engine(&dir);
    let hole = place(&mut engine, MOUNTING_HOLE_M3, json!({"side": "bottom"}));
    engine.run_ok(
        "pcb.mechanical.set",
        json!({"footprints": [hole], "not_fitted": true}),
    );
    engine.run_ok("project.save", json!({}));

    let mut reopened = reda_testkit::test_engine();
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    let pcb_doc = reopened.project().unwrap().pcb().unwrap();
    assert_eq!(pcb_doc.uuid.to_string(), PCB);
    let again = pcb_doc.footprint(hole).unwrap();
    assert_eq!(again.component, None);
    assert!(again.not_fitted);
    assert_eq!(again.side, Side::Bottom);
}
