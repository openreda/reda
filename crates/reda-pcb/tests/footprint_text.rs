//! Placed field text through Commands and saved PCB Documents.
mod common;

use common::{engine, pcb, synced_resistors};
use reda_testkit::TempDir;
use serde_json::{Value, json};

#[test]
fn bulk_text_changes_are_sparse_and_leave_the_footprint_copy_unchanged() {
    let dir = TempDir::new("footprint-text-bulk");
    let mut engine = engine(&dir);
    let ids = synced_resistors(&mut engine);
    let original: Vec<_> = pcb(&engine)
        .footprint_copies()
        .map(|c| c.hash.clone())
        .collect();
    engine.run_ok("pcb.footprint.text.set", json!({"instances":ids,"field":"Designator","style":{"height":"0.8mm","thickness":"0.1mm","justify":{"h":"right","v":"top"}},"visible":false}));
    engine.run_ok("project.save", json!({}));
    let saved: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("Demo/Demo.redapcb")).unwrap())
            .unwrap();
    for instance in saved["footprints"].as_array().unwrap() {
        assert!(instance.get("designator_text").is_none());
        let changes = instance["text_overrides"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        assert!(change["text"].is_string());
        assert!(change.get("at").is_none());
        assert!(change.get("rotation").is_none());
        assert!(change.get("autoposition").is_none());
        assert_eq!(change["visible"], false);
        assert_eq!(change["style"]["height"], 800_000);
        assert_eq!(change["style"]["thickness"], 100_000);
    }
    assert_eq!(
        pcb(&engine)
            .footprint_copies()
            .map(|c| c.hash.clone())
            .collect::<Vec<_>>(),
        original
    );
    engine.run_ok("edit.undo", json!({"document":pcb(&engine).uuid}));
    engine.run_ok("project.save", json!({}));
    let saved: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("Demo/Demo.redapcb")).unwrap())
            .unwrap();
    assert!(
        saved["footprints"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i.get("text_overrides").is_none())
    );
}

#[test]
fn nine_autopositions_follow_the_rotated_courtyard_and_stay_upright() {
    let dir = TempDir::new("footprint-autoposition");
    let mut engine = engine(&dir);
    let [id, _] = synced_resistors(&mut engine);
    for position in [
        "left_above",
        "center_above",
        "right_above",
        "left_center",
        "center",
        "right_center",
        "left_below",
        "center_below",
        "right_below",
    ] {
        engine.run_ok(
            "pcb.footprint.text.set",
            json!({"instances":[id],"field":"Designator","autoposition":position}),
        );
        let before =
            reda_pcb::placed::placed_designator(pcb(&engine), pcb(&engine).footprint(id).unwrap())
                .unwrap();
        let bounds =
            reda_pcb::placed::footprint_bounds(pcb(&engine), pcb(&engine).footprint(id).unwrap());
        if position.ends_with("above") {
            assert!(before.at.y > bounds.max.y, "{position}");
        }
        if position.ends_with("below") {
            assert!(before.at.y < bounds.min.y, "{position}");
        }
        if position.starts_with("left_") {
            assert!(before.at.x < bounds.min.x, "{position}");
        }
        if position.starts_with("right_") {
            assert!(before.at.x > bounds.max.x, "{position}");
        }
        engine.run_ok("pcb.rotate", json!({"objects":[id],"angle":"90deg"}));
        let after =
            reda_pcb::placed::placed_designator(pcb(&engine), pcb(&engine).footprint(id).unwrap())
                .unwrap();
        assert_eq!(after.rotation, reda_model::geom::Millideg::ZERO);
    }
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[id],"field":"Designator","at":{"x":"20mm","y":"30mm"}}),
    );
    let instance = pcb(&engine).footprint(id).unwrap().clone();
    assert_eq!(
        instance.text_overrides[0].autoposition,
        Some(reda_model::TextAutoposition::Manual)
    );
    assert_eq!(
        reda_pcb::placed::placed_designator(pcb(&engine), &instance)
            .unwrap()
            .at,
        reda_model::geom::Point::new(
            reda_model::geom::Nm::from_mm(20),
            reda_model::geom::Nm::from_mm(30)
        )
    );
}

#[test]
fn changing_a_footprint_remaps_overrides_to_the_same_field_and_layer() {
    let dir = TempDir::new("footprint-text-remap");
    let mut engine = engine(&dir);
    let [id, _] = synced_resistors(&mut engine);
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[id],"field":"Designator","at":{"x":"20mm","y":"30mm"},"visible":false}),
    );
    let component = pcb(&engine)
        .footprint(id)
        .unwrap()
        .component
        .clone()
        .unwrap();
    let before = pcb(&engine).footprint(id).unwrap().text_overrides.clone();
    let alternate = engine
        .project()
        .unwrap()
        .schematic_sheet(common::SHEET.parse().unwrap())
        .unwrap()
        .parts()
        .next()
        .unwrap()
        .footprints
        .iter()
        .find(|copy| **copy != pcb(&engine).footprint(id).unwrap().footprint)
        .copied()
        .unwrap();
    let alternate = engine
        .project()
        .unwrap()
        .schematic_sheet(common::SHEET.parse().unwrap())
        .unwrap()
        .footprint(alternate)
        .unwrap()
        .footprint
        .uuid;
    engine.run_ok(
        "schematic.component.set",
        json!({"components":[component.uuid],"footprint":alternate}),
    );
    let hash = engine.run_ok("sync.preview", json!({}))["hash"].clone();
    engine.run_ok("sync.apply", json!({"hash":hash}));
    let instance = pcb(&engine).footprint(id).unwrap().clone();
    let entry = pcb(&engine)
        .footprint_copy(instance.footprint)
        .unwrap()
        .footprint
        .designator_text()
        .unwrap();
    assert_ne!(before[0].text, entry.uuid);
    assert_eq!(instance.text_overrides[0].text, entry.uuid);
    assert_eq!(instance.text_overrides[0].at, before[0].at);
    assert_eq!(instance.text_overrides[0].visible, Some(false));
    engine.run_ok("project.save", json!({}));
    let mut reopened = reda_testkit::test_engine();
    reopened.run_ok("project.open", json!({"path":dir.path().join("Demo")}));
    assert_eq!(
        pcb(&reopened).footprint(id).unwrap().text_overrides,
        instance.text_overrides
    );
}
