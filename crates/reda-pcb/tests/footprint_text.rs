//! Placed field text through Commands and saved PCB Documents.
mod common;

use common::{engine, pcb, synced_resistors};
use reda_testkit::TempDir;
use serde_json::{Value, json};

/// The UUID of instance `id`'s Silkscreen Designator entry.
fn silk_designator(engine: &reda_testkit::TestEngine, id: reda_model::Uuid) -> reda_model::Uuid {
    let instance = pcb(engine).footprint(id).unwrap();
    pcb(engine)
        .footprint_copy(instance.footprint)
        .unwrap()
        .footprint
        .designator_text()
        .unwrap()
        .uuid
}

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
        // The field selects both Designator entries: Silkscreen and Assembly.
        let changes = instance["text_overrides"].as_array().unwrap();
        assert_eq!(changes.len(), 2);
        for change in changes {
            assert!(change["text"].is_string());
            assert!(change.get("at").is_none());
            assert!(change.get("rotation").is_none());
            assert!(change.get("autoposition").is_none());
            assert_eq!(change["visible"], false);
            assert_eq!(change["style"]["height"], 800_000);
            assert_eq!(change["style"]["thickness"], 100_000);
        }
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
    let silk = silk_designator(&engine, id);
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
            json!({"instances":[id],"texts":[silk],"autoposition":position}),
        );
        // The same side of the (rotated) Courtyard box on the board, at every turn.
        for turn in 0..4 {
            let instance = pcb(&engine).footprint(id).unwrap();
            let text =
                reda_pcb::placed::placed_designator(pcb(&engine), instance, &Default::default())
                    .unwrap();
            let bounds = reda_pcb::placed::footprint_bounds(pcb(&engine), instance);
            let at = (position, turn);
            assert_eq!(text.rotation, reda_model::geom::Millideg::ZERO, "{at:?}");
            match position.split_once('_').map_or(position, |(h, _)| h) {
                "left" => assert!(text.at.x < bounds.min.x, "{at:?}"),
                "right" => assert!(text.at.x > bounds.max.x, "{at:?}"),
                _ => assert!((bounds.min.x..=bounds.max.x).contains(&text.at.x), "{at:?}"),
            }
            match position.rsplit_once('_').map_or(position, |(_, v)| v) {
                "above" => assert!(text.at.y > bounds.max.y, "{at:?}"),
                "below" => assert!(text.at.y < bounds.min.y, "{at:?}"),
                _ => assert!((bounds.min.y..=bounds.max.y).contains(&text.at.y), "{at:?}"),
            }
            engine.run_ok("pcb.rotate", json!({"objects":[id],"angle":"90deg"}));
        }
    }
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[id],"texts":[silk],"at":{"x":"20mm","y":"30mm"}}),
    );
    let instance = pcb(&engine).footprint(id).unwrap().clone();
    // Moving the text returns it to `manual`, the default, which isn't stored.
    assert_eq!(instance.text_overrides[0].autoposition, None);
    assert_eq!(
        reda_pcb::placed::placed_designator(pcb(&engine), &instance, &Default::default())
            .unwrap()
            .at,
        reda_model::geom::Point::new(
            reda_model::geom::Nm::from_mm(20),
            reda_model::geom::Nm::from_mm(30)
        )
    );
}

#[test]
fn an_anchor_moves_one_text_and_overrides_keep_only_changed_values() {
    let dir = TempDir::new("footprint-text-sparse-anchor");
    let mut engine = engine(&dir);
    let [id, other] = synced_resistors(&mut engine);
    let silk = silk_designator(&engine, id);
    // One board point for several texts would stack them there.
    let error = engine.run_err(
        "pcb.footprint.text.set",
        json!({"instances":[id, other],"field":"Designator","at":{"x":"20mm","y":"30mm"}}),
    );
    assert_eq!(error.code, reda_command::ErrorCode::InvalidParams);
    assert!(
        pcb(&engine)
            .footprints()
            .all(|i| i.text_overrides.is_empty())
    );

    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[id],"texts":[silk],"autoposition":"center_above"}),
    );
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[id],"texts":[silk],"autoposition":"manual"}),
    );
    assert!(
        pcb(&engine)
            .footprint(id)
            .unwrap()
            .text_overrides
            .is_empty()
    );

    // The entry's own place and reading direction, given on the board, store nothing.
    let instance = pcb(&engine).footprint(id).unwrap().clone();
    let entry = pcb(&engine)
        .footprint_copy(instance.footprint)
        .unwrap()
        .footprint
        .designator_text()
        .unwrap()
        .clone();
    let frame = reda_pcb::placed::Frame::of(&instance);
    let home = frame.point(entry.at);
    let turn = frame.angle(entry.rotation);
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[id],"texts":[silk],"autoposition":"center_above"}),
    );
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[id],"texts":[silk],"at":{"x":home.x.0,"y":home.y.0},"rotation":turn.0}),
    );
    assert!(
        pcb(&engine)
            .footprint(id)
            .unwrap()
            .text_overrides
            .is_empty()
    );
}

#[test]
fn changing_a_footprint_remaps_overrides_to_the_same_field_and_layer() {
    let dir = TempDir::new("footprint-text-remap");
    let mut engine = engine(&dir);
    let [id, _] = synced_resistors(&mut engine);
    let silk = silk_designator(&engine, id);
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[id],"texts":[silk],"at":{"x":"20mm","y":"30mm"},"visible":false}),
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

#[test]
fn a_placed_text_font_override_is_reported_when_the_font_is_missing() {
    let dir = TempDir::new("footprint-font-missing");
    let mut engine = engine(&dir);
    let [id, _] = synced_resistors(&mut engine);
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[id],"field":"Designator","style":{"font":"Unavailable Test Family"}}),
    );
    assert_eq!(
        reda_pcb::missing_fonts(pcb(&engine), &reda_command::Fonts::default()),
        ["Unavailable Test Family"]
    );
}

#[test]
fn an_explicit_text_uuid_moves_only_that_entry_of_that_instance() {
    let dir = TempDir::new("footprint-text-uuid");
    let mut engine = engine(&dir);
    let [first, second] = synced_resistors(&mut engine);
    let text = pcb(&engine)
        .footprint_copy(pcb(&engine).footprint(first).unwrap().footprint)
        .unwrap()
        .footprint
        .texts
        .iter()
        .find(|t| t.field == "Value")
        .unwrap()
        .uuid;
    engine.run_ok("pcb.flip", json!({"objects":[first]}));
    engine.run_ok("pcb.rotate", json!({"objects":[first],"angle":"90deg"}));
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[first],"texts":[text],"at":{"x":"3mm","y":"4mm"},"rotation":"45deg"}),
    );
    assert!(
        pcb(&engine)
            .footprint(second)
            .unwrap()
            .text_overrides
            .is_empty()
    );
    let changed = pcb(&engine)
        .footprint(first)
        .unwrap()
        .text_overrides
        .clone();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].text, text);
    let error = engine.run_err(
        "pcb.footprint.text.set",
        json!({"instances":[first,second],"texts":[pcb(&engine).uuid],"visible":false}),
    );
    assert_eq!(error.code, reda_command::ErrorCode::Rejected);
    assert_eq!(
        pcb(&engine).footprint(first).unwrap().text_overrides,
        changed
    );
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[first],"texts":[text],"reset":true}),
    );
    assert!(
        pcb(&engine)
            .footprint(first)
            .unwrap()
            .text_overrides
            .is_empty()
    );
}

#[test]
fn autoposition_uses_the_pad_box_when_the_footprint_has_no_courtyard() {
    let dir = TempDir::new("footprint-text-fallback");
    let mut engine = reda_testkit::probe_engine();
    engine.run_ok("project.new", json!({"path":dir.path(),"name":"Demo"}));
    let footprint=engine.run_ok("test.footprint.add",json!({"at":{"x":0,"y":0},"designator":"A","pads":[{"at":{"x":0,"y":0},"width":"2mm","height":"1mm"}]}))["result"]["footprint"].clone();
    let id: reda_model::Uuid = footprint.as_str().unwrap().parse().unwrap();
    engine.run_ok(
        "pcb.footprint.text.set",
        json!({"instances":[id],"field":"Designator","autoposition":"center_above"}),
    );
    let placed = reda_pcb::placed::placed_designator(
        pcb(&engine),
        pcb(&engine).footprint(id).unwrap(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(placed.at.x, reda_model::geom::Nm::ZERO);
    assert!(placed.at.y > reda_model::geom::Nm::from_um(500));
}

#[test]
fn replacement_prefers_stable_text_ids_and_drops_entries_with_no_match() {
    let dir = TempDir::new("footprint-text-match-priority");
    let mut original = engine(&dir);
    let [id, _] = synced_resistors(&mut original);
    for field in ["Designator", "Value"] {
        original.run_ok(
            "pcb.footprint.text.set",
            json!({"instances":[id],"field":field,"visible":false}),
        );
    }
    let instance = pcb(&original).footprint(id).unwrap().clone();
    let old_copy = pcb(&original)
        .footprint_copy(instance.footprint)
        .unwrap()
        .clone();
    let old_designator = old_copy.footprint.designator_text().unwrap().uuid;
    let sheet = original
        .project()
        .unwrap()
        .schematic_sheet(common::SHEET.parse().unwrap())
        .unwrap();
    let alternate = sheet
        .parts()
        .next()
        .unwrap()
        .footprints
        .iter()
        .find_map(|copy| {
            let entry = sheet.footprint(*copy).unwrap();
            (entry.footprint.uuid != old_copy.footprint.uuid).then_some(entry.footprint.uuid)
        })
        .unwrap();
    original.run_ok("project.save", json!({}));
    // Native file input can retain an entry UUID while its field name changes. Another
    // old field disappears entirely. Sync must preserve identities, remap matching
    // field/Layer entries and discard the obsolete override.
    let folder = dir.path().join("Demo");
    let sheet_path = folder.join("Demo.redasch");
    let mut sheet: Value = serde_json::from_slice(&std::fs::read(&sheet_path).unwrap()).unwrap();
    let target = sheet["footprints"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|copy| copy["footprint"]["uuid"] == json!(alternate))
        .unwrap();
    let new_value = target["texts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|text| text["field"] == "Value")
        .unwrap()["uuid"]
        .clone();
    let designator = target["texts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|text| text["field"] == "Designator")
        .unwrap();
    designator["uuid"] = json!(old_designator);
    designator["field"] = json!("MPN");
    std::fs::write(&sheet_path, serde_json::to_vec_pretty(&sheet).unwrap()).unwrap();
    let pcb_path = folder.join("Demo.redapcb");
    let mut board: Value = serde_json::from_slice(&std::fs::read(&pcb_path).unwrap()).unwrap();
    let obsolete = reda_model::Uuid::v4_from_bits(0x5555);
    let copy = board["footprint_copies"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|copy| copy["uuid"] == json!(instance.footprint))
        .unwrap();
    let mut extra = copy["texts"][0].clone();
    extra["uuid"] = json!(obsolete);
    extra["field"] = json!("Obsolete");
    copy["texts"].as_array_mut().unwrap().push(extra);
    let stored = board["footprints"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["uuid"] == json!(id))
        .unwrap();
    stored["text_overrides"]
        .as_array_mut()
        .unwrap()
        .push(json!({"text":obsolete,"visible":false}));
    std::fs::write(&pcb_path, serde_json::to_vec_pretty(&board).unwrap()).unwrap();
    let mut reopened = original;
    reopened.run_ok("project.open", json!({"path":folder}));
    reopened.run_ok(
        "schematic.component.set",
        json!({"components":[instance.component.unwrap().uuid],"footprint":alternate}),
    );
    let hash = reopened.run_ok("sync.preview", json!({}))["hash"].clone();
    reopened.run_ok("sync.apply", json!({"hash":hash}));
    let changes = &pcb(&reopened).footprint(id).unwrap().text_overrides;
    assert_eq!(changes.len(), 2);
    assert!(
        changes
            .iter()
            .any(|change| change.text == old_designator && change.visible == Some(false)),
        "UUID match wins despite the renamed field"
    );
    assert!(
        changes
            .iter()
            .any(|change| json!(change.text) == new_value && change.visible == Some(false)),
        "Value follows the matching field and Layer"
    );
    assert!(changes.iter().all(|change| change.text != obsolete));
}
