//! Mechanical Layers through `pcb.layer.*`: adding, setting, moving and removing them.
use reda_model::{LayerKind, PcbDocument, Purpose, Side, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":2}),
    );
    engine
}

fn pcb(e: &TestEngine) -> &PcbDocument {
    e.project().unwrap().pcb().unwrap()
}

/// The Mechanical Layers in Layer order: (name, Purpose, side).
fn mechanical(e: &TestEngine) -> Vec<(String, Purpose, Option<Side>)> {
    pcb(e)
        .layers()
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Mechanical)
        .map(|l| (l.name.clone().unwrap(), l.purpose, l.side))
        .collect()
}

fn add(e: &mut TestEngine, params: Value) -> Uuid {
    let mut params = params;
    params["document"] = json!(pcb(e).uuid);
    params["kind"] = json!("mechanical");
    let out = e.run_ok("pcb.layer.add", params);
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

#[test]
fn a_new_mechanical_layer_is_named_after_its_purpose_and_numbered_on_a_clash() {
    let dir = TempDir::new("mech-add");
    let mut e = board(&dir);
    use Purpose as P;
    assert_eq!(
        mechanical(&e),
        [
            ("Dimensions".into(), P::Dimensions, None),
            ("Reference".into(), P::Reference, None)
        ]
    );
    let general = add(&mut e, json!({}));
    add(&mut e, json!({"purpose": "dimensions"}));
    add(&mut e, json!({"purpose": "dimensions", "side": "bottom"}));
    add(
        &mut e,
        json!({"purpose": "fabrication_notes", "name": "  Fab drawing "}),
    );
    assert_eq!(
        mechanical(&e),
        [
            ("Dimensions".into(), P::Dimensions, None),
            ("Reference".into(), P::Reference, None),
            ("General".into(), P::General, None),
            ("Dimensions 2".into(), P::Dimensions, None),
            ("Dimensions 3".into(), P::Dimensions, Some(Side::Bottom)),
            ("Fab drawing".into(), P::FabricationNotes, None),
        ]
    );
    // `before` puts it ahead of a Mechanical Layer; the result is one undo step.
    add(
        &mut e,
        json!({"purpose": "assembly_notes", "before": general}),
    );
    assert_eq!(mechanical(&e)[2].0, "Assembly Notes");
    assert_eq!(mechanical(&e)[3].0, "General");
    let doc = pcb(&e).uuid;
    e.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(mechanical(&e).len(), 6);
    assert_eq!(mechanical(&e)[2].0, "General");
}

fn uuid_of(e: &TestEngine, name: &str) -> Uuid {
    pcb(e)
        .layers()
        .layers
        .iter()
        .find(|l| l.kind == LayerKind::Mechanical && l.name.as_deref() == Some(name))
        .unwrap()
        .uuid
}

#[test]
fn set_renames_repurposes_and_sides_a_mechanical_layer() {
    let dir = TempDir::new("mech-set");
    let mut e = board(&dir);
    let reference = uuid_of(&e, "Reference");
    e.run_ok(
        "pcb.text.add",
        json!({"layer": reference, "x": "5mm", "y": "5mm", "text": "HOST"}),
    );
    let mirrored = |e: &TestEngine| pcb(e).texts().next().unwrap().mirrored;
    assert!(!mirrored(&e));
    e.run_ok(
        "pcb.layer.set",
        json!({"layer": reference, "name": " Enclosure ", "purpose": "general", "side": "bottom"}),
    );
    assert_eq!(
        mechanical(&e)[1],
        ("Enclosure".into(), Purpose::General, Some(Side::Bottom))
    );
    // Text on a Layer that turns bottom reads from below, as on other bottom Layers.
    assert!(mirrored(&e));
    // Renaming never changes the Purpose, and null makes the Layer unsided again.
    e.run_ok(
        "pcb.layer.set",
        json!({"layer": reference, "name": "Host board", "side": null}),
    );
    assert_eq!(
        mechanical(&e)[1],
        ("Host board".into(), Purpose::General, None)
    );
    assert!(!mirrored(&e));
    // Keeping its own name is no clash.
    e.run_ok(
        "pcb.layer.set",
        json!({"layer": reference, "name": "Host board", "purpose": "reference"}),
    );
    assert_eq!(mechanical(&e)[1].1, Purpose::Reference);
}

#[test]
fn move_reorders_among_the_mechanical_layers_only() {
    let dir = TempDir::new("mech-move");
    let mut e = board(&dir);
    let reference = uuid_of(&e, "Reference");
    let names =
        |e: &TestEngine| -> Vec<String> { mechanical(e).into_iter().map(|m| m.0).collect() };
    e.run_ok(
        "pcb.layer.move",
        json!({"layer": reference, "direction": "up"}),
    );
    assert_eq!(names(&e), ["Reference", "Dimensions"]);
    let err = e.run_err(
        "pcb.layer.move",
        json!({"layer": reference, "direction": "up"}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::Rejected, "{err:?}");
    e.run_ok(
        "pcb.layer.move",
        json!({"layer": reference, "direction": "down"}),
    );
    assert_eq!(names(&e), ["Dimensions", "Reference"]);
    let err = e.run_err(
        "pcb.layer.move",
        json!({"layer": reference, "direction": "down"}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::Rejected, "{err:?}");
    // The Board Outline stays just before the Mechanical Layers.
    let kinds: Vec<_> = pcb(&e).layers().layers.iter().map(|l| l.kind).collect();
    let outline = kinds
        .iter()
        .position(|k| *k == LayerKind::BoardOutline)
        .unwrap();
    assert_eq!(kinds[outline + 1..], [LayerKind::Mechanical; 2]);
}

#[test]
fn remove_deletes_the_layer_s_objects_after_the_preview_lists_them() {
    let dir = TempDir::new("mech-remove");
    let mut e = board(&dir);
    let dimensions = uuid_of(&e, "Dimensions");
    let reference = uuid_of(&e, "Reference");
    for layer in [dimensions, reference] {
        e.run_ok(
            "pcb.graphic.add_line",
            json!({"layer": layer, "from": {"x": 0, "y": 0}, "to": {"x": "10mm", "y": 0}}),
        );
    }
    e.run_ok(
        "pcb.text.add",
        json!({"layer": dimensions, "x": "5mm", "y": "5mm", "text": "NOTE"}),
    );
    e.run_ok(
        "pcb.dimension.add",
        json!({"layer": dimensions, "a": {"point": {"x": 0, "y": 0}},
               "b": {"point": {"x": "10mm", "y": 0}}, "offset": "2mm"}),
    );
    let counts = e.run_ok(
        "pcb.stackup.change_preview",
        json!({"change": {"id": "pcb.layer.remove", "params": {"layer": dimensions}}}),
    );
    assert_eq!(
        counts["objects_deleted"],
        json!({"dimension": 1, "graphic": 1, "text": 1})
    );
    // The preview changes nothing.
    assert_eq!(mechanical(&e).len(), 2);
    let stackup = pcb(&e).stackup().clone();
    let doc = pcb(&e).uuid;
    let out = e.run_ok("pcb.layer.remove", json!({"layer": dimensions}));
    let changes = &out["changes"][doc.to_string()];
    assert_eq!(changes["deleted"].as_array().unwrap().len(), 3);
    // Only the Layer set changes; the Stackup is untouched.
    assert_eq!(changes["modified"].as_array().unwrap().len(), 1);
    assert_eq!(mechanical(&e)[0].0, "Reference");
    assert_eq!(pcb(&e).graphics().count(), 1);
    assert_eq!(pcb(&e).texts().count() + pcb(&e).dimensions().count(), 0);
    assert_eq!(pcb(&e).stackup(), &stackup);
    // The last one goes too: a board may have no Mechanical Layer.
    e.run_ok("pcb.layer.remove", json!({"layer": reference}));
    assert!(mechanical(&e).is_empty());
    e.run_ok("edit.undo", json!({"document": doc}));
    e.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(mechanical(&e).len(), 2);
    assert_eq!(pcb(&e).graphics().count(), 2);
}

/// Runs `id` expecting `code` with `pointer`, and checks the board is as it was.
fn refused(e: &mut TestEngine, id: &str, params: Value, pointer: Option<&str>) {
    use reda_command::ErrorCode;
    let before = pcb(e).clone();
    let err = e.run_err(id, params.clone());
    match pointer {
        Some(pointer) => {
            assert_eq!(err.code, ErrorCode::InvalidParams, "{id} {params}: {err:?}");
            assert_eq!(
                err.details.as_ref().and_then(|d| d["pointer"].as_str()),
                Some(pointer),
                "{id} {params}: {err:?}"
            );
        }
        None => assert_eq!(err.code, ErrorCode::Rejected, "{id} {params}: {err:?}"),
    }
    assert_eq!(pcb(e), &before, "{id} {params} wrote something");
}

#[test]
fn refusals_write_nothing() {
    let dir = TempDir::new("mech-refusals");
    let mut e = board(&dir);
    let doc = pcb(&e).uuid;
    let dimensions = uuid_of(&e, "Dimensions");
    let sheet = pcb(&e).stackup().dielectrics().next().unwrap().uuid;
    let add = |extra: Value| {
        let mut p = json!({"document": doc, "kind": "mechanical"});
        p.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        p
    };
    // `split` is for copper.
    refused(
        &mut e,
        "pcb.layer.add",
        add(json!({"split": sheet})),
        Some("/split"),
    );
    // A duplicate name, also after trimming, and a blank one.
    for name in ["Reference", " Dimensions ", "  "] {
        refused(
            &mut e,
            "pcb.layer.add",
            add(json!({"name": name})),
            Some("/name"),
        );
    }
    refused(
        &mut e,
        "pcb.layer.set",
        json!({"layer": dimensions, "name": "Reference"}),
        Some("/name"),
    );
    // A Mechanical Layer's name can't be cleared.
    for name in [json!(null), json!(" ")] {
        refused(
            &mut e,
            "pcb.layer.set",
            json!({"layer": dimensions, "name": name}),
            Some("/name"),
        );
    }
    // `purpose` and `side` on a Layer that isn't Mechanical, and a role on one that is.
    refused(
        &mut e,
        "pcb.layer.set",
        json!({"layer": "copper_top", "purpose": "dimensions"}),
        Some("/purpose"),
    );
    refused(
        &mut e,
        "pcb.layer.set",
        json!({"layer": "copper_bottom", "side": "top"}),
        Some("/side"),
    );
    refused(
        &mut e,
        "pcb.layer.set",
        json!({"layer": "silkscreen_top", "purpose": "dimensions"}),
        Some("/layer"),
    );
    refused(
        &mut e,
        "pcb.layer.set",
        json!({"layer": dimensions, "role": "plane"}),
        Some("/role"),
    );
    // The Mechanical params on a copper Layer, and `before` naming another kind.
    for key in ["name", "purpose", "side", "before"] {
        let value = match key {
            "purpose" => json!("dimensions"),
            "side" => json!("top"),
            "before" => json!(dimensions),
            _ => json!("Extra"),
        };
        refused(
            &mut e,
            "pcb.layer.add",
            json!({"document": doc, key: value}),
            Some(&format!("/{key}")),
        );
    }
    refused(
        &mut e,
        "pcb.layer.add",
        add(json!({"before": "copper_top"})),
        Some("/before"),
    );
    // The outer copper and the other kinds can't be moved or removed.
    for layer in ["copper_top", "board_outline", "silkscreen_top"] {
        refused(&mut e, "pcb.layer.remove", json!({"layer": layer}), None);
        refused(
            &mut e,
            "pcb.layer.move",
            json!({"layer": layer, "direction": "down"}),
            None,
        );
    }
}

#[test]
fn a_dimension_with_no_layer_goes_on_the_first_dimensions_layer_in_layer_order() {
    let dir = TempDir::new("mech-dimension");
    let mut e = board(&dir);
    let dimension = |e: &mut TestEngine| -> Uuid {
        let out = e.run_ok(
            "pcb.dimension.add",
            json!({"a": {"point": {"x": 0, "y": 0}}, "b": {"point": {"x": "10mm", "y": 0}}}),
        );
        let uuid: Uuid = out["result"]["uuid"].as_str().unwrap().parse().unwrap();
        pcb(e).dimension(uuid).unwrap().layer
    };
    assert_eq!(dimension(&mut e), uuid_of(&e, "Dimensions"));
    // A second Dimensions Layer moved ahead of the first takes over.
    let second = add(&mut e, json!({"purpose": "dimensions", "name": "Drawing"}));
    assert_eq!(dimension(&mut e), uuid_of(&e, "Dimensions"));
    for _ in 0..2 {
        e.run_ok(
            "pcb.layer.move",
            json!({"layer": second, "direction": "up"}),
        );
    }
    assert_eq!(dimension(&mut e), second);
    // With none, the Layer must be named.
    for layer in [second, uuid_of(&e, "Dimensions")] {
        e.run_ok(
            "pcb.layer.set",
            json!({"layer": layer, "purpose": "general"}),
        );
    }
    let err = e.run_err(
        "pcb.dimension.add",
        json!({"a": {"point": {"x": 0, "y": 0}}, "b": {"point": {"x": "10mm", "y": 0}}}),
    );
    assert_eq!(
        err.details.as_ref().and_then(|d| d["pointer"].as_str()),
        Some("/layer"),
        "{err:?}"
    );
}
