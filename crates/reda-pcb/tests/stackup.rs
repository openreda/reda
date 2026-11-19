//! The Stackup: what a new board starts with, and `pcb.stackup.set`.

use reda_command::ErrorCode;
use reda_model::geom::Nm;
use reda_model::{LayerKind, PcbDocument, Side, Stackup, StackupEntry, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::json;

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn stackup(engine: &TestEngine) -> &Stackup {
    pcb(engine).stackup()
}

/// The top copper, bottom copper and the one dielectric between them.
fn ids(engine: &TestEngine) -> (Uuid, Uuid, Uuid) {
    let layers = pcb(engine).layers();
    let top = layers
        .find(LayerKind::Copper, Some(Side::Top))
        .unwrap()
        .uuid;
    let bottom = layers
        .find(LayerKind::Copper, Some(Side::Bottom))
        .unwrap()
        .uuid;
    let dielectric = stackup(engine).dielectrics().next().unwrap().uuid;
    (top, bottom, dielectric)
}

#[test]
fn a_new_board_is_1_6_mm_with_1_oz_copper_and_one_core_of_unspecified_thickness() {
    let dir = TempDir::new("stackup-new");
    let engine = engine(&dir);
    let (top, bottom, dielectric) = ids(&engine);
    let stackup = stackup(&engine);
    assert_eq!(stackup.thickness, Nm(1_600_000));
    assert_eq!(
        serde_json::to_value(&stackup.entries).unwrap(),
        json!([
            {"layer": top, "thickness": 35_000},
            {"uuid": dielectric, "kind": "core"},
            {"layer": bottom, "thickness": 35_000},
        ])
    );
    assert_eq!(stackup.finish, None);
    assert_eq!(stackup.mask_color, None);
    assert_eq!(stackup.silk_color, None);
}

#[test]
fn copper_is_entered_in_ounces_at_exactly_35_um_and_stored_in_nm() {
    let dir = TempDir::new("stackup-oz");
    let mut engine = engine(&dir);
    let (top, bottom, _) = ids(&engine);
    let out = engine.run_ok(
        "pcb.stackup.set",
        json!({"copper": [
            {"layer": top, "thickness": "2oz"},
            {"layer": bottom, "thickness": "0.5 oz"},
        ]}),
    );
    let copper: Vec<Nm> = stackup(&engine).copper().map(|c| c.thickness).collect();
    assert_eq!(copper, [Nm(70_000), Nm(17_500)]);
    assert_eq!(out["result"]["totals"]["copper"], 87_500);
    assert_eq!(out["result"]["totals"]["remainder"], 1_512_500);

    engine.run_ok(
        "pcb.stackup.set",
        json!({"copper": [{"layer": top, "thickness": "18µm"}]}),
    );
    assert_eq!(
        stackup(&engine).copper_of(top).unwrap().thickness,
        Nm(18_000)
    );
}

#[test]
fn finish_and_colours_are_optional_free_text() {
    let dir = TempDir::new("stackup-text");
    let mut engine = engine(&dir);
    engine.run_ok(
        "pcb.stackup.set",
        json!({"finish": "ENIG", "mask_color": "Matte black", "silk_color": "White"}),
    );
    let s = stackup(&engine);
    assert_eq!(s.finish.as_deref(), Some("ENIG"));
    assert_eq!(s.mask_color.as_deref(), Some("Matte black"));
    assert_eq!(s.silk_color.as_deref(), Some("White"));

    engine.run_ok("pcb.stackup.set", json!({"finish": null, "mask_color": ""}));
    let s = stackup(&engine);
    assert_eq!(s.finish, None);
    assert_eq!(s.mask_color, None);
    assert_eq!(s.silk_color.as_deref(), Some("White"));
}

#[test]
fn a_negative_remainder_is_refused_on_the_thickness() {
    let dir = TempDir::new("stackup-negative");
    let mut engine = engine(&dir);
    let err = engine.run_err("pcb.stackup.set", json!({"thickness": "0.05mm"}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/thickness");
    assert_eq!(stackup(&engine).thickness, Nm(1_600_000));
}

#[test]
fn every_dielectric_typed_in_keeps_the_thickness_and_warns_on_a_mismatch() {
    let dir = TempDir::new("stackup-mismatch");
    let mut engine = engine(&dir);
    let (_, _, dielectric) = ids(&engine);
    let out = engine.run_ok(
        "pcb.stackup.set",
        json!({"dielectrics": [{"uuid": dielectric, "thickness": "1.5mm"}]}),
    );
    assert_eq!(stackup(&engine).thickness, Nm(1_600_000));
    assert_eq!(out["result"]["totals"]["total"], 1_570_000);
    assert_eq!(out["result"]["totals"]["remainder"], json!(null));
    let warnings = out["result"]["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");

    // Typed in to match, nothing is warned.
    let out = engine.run_ok("pcb.stackup.set", json!({"thickness": "1.57mm"}));
    assert_eq!(out["result"]["warnings"], json!([]));

    // Left blank again, the dielectric takes the remainder.
    let out = engine.run_ok(
        "pcb.stackup.set",
        json!({"dielectrics": [{"uuid": dielectric, "thickness": null}]}),
    );
    assert_eq!(out["result"]["totals"]["remainder"], 1_500_000);
    assert!(matches!(
        &stackup(&engine).entries[1],
        StackupEntry::Dielectric(d) if d.thickness.is_none()
    ));
}

#[test]
fn unknown_entries_and_non_positive_thicknesses_are_refused_with_a_pointer() {
    let dir = TempDir::new("stackup-refused");
    let mut engine = engine(&dir);
    let (top, _, dielectric) = ids(&engine);
    for (params, pointer) in [
        (
            json!({"copper": [{"layer": dielectric, "thickness": "1oz"}]}),
            "/copper/0/layer",
        ),
        (
            json!({"copper": [{"layer": top, "thickness": 0}]}),
            "/copper/0/thickness",
        ),
        (
            json!({"dielectrics": [{"uuid": top, "thickness": "1mm"}]}),
            "/dielectrics/0/uuid",
        ),
        (
            json!({"dielectrics": [{"uuid": dielectric, "thickness": "-1mm"}]}),
            "/dielectrics/0/thickness",
        ),
        (json!({"thickness": 0}), "/thickness"),
        (
            json!({"copper": [{"layer": top, "thickness": "1ft"}]}),
            "/copper/0/thickness",
        ),
    ] {
        let err = engine.run_err("pcb.stackup.set", params.clone());
        assert_eq!(err.code, ErrorCode::InvalidParams, "{params}");
        assert_eq!(err.details.unwrap()["pointer"], pointer, "{params}");
    }
}

#[test]
fn setting_the_stackup_is_undoable() {
    let dir = TempDir::new("stackup-undo");
    let mut engine = engine(&dir);
    engine.run_ok("pcb.stackup.set", json!({"thickness": "0.8mm"}));
    assert_eq!(stackup(&engine).thickness, Nm(800_000));
    let document = pcb(&engine).uuid;
    engine.run_ok("edit.undo", json!({"document": document}));
    assert_eq!(stackup(&engine).thickness, Nm(1_600_000));
}

#[test]
fn copper_layers_can_be_named_by_their_copper_number() {
    let dir = TempDir::new("stackup-copper-number");
    let mut engine = engine(&dir);
    let (top, bottom, _) = ids(&engine);
    let mut layer_of = |name: &str| -> Result<Uuid, ErrorCode> {
        let params = json!({"layer": name, "from": {"x": 0, "y": 0},
                            "to": {"x": "10mm", "y": 0}, "width": "0.2mm"});
        match engine.run("pcb.graphic.add_line", params) {
            Ok(out) => Ok(out["result"]["uuid"].as_str().unwrap().parse().unwrap()),
            Err(err) => Err(err.code),
        }
    };
    let l1 = layer_of("copper_l1").unwrap();
    let l2 = layer_of("copper_l2").unwrap();
    assert_eq!(layer_of("copper_l3"), Err(ErrorCode::InvalidParams));
    // Bare "copper" would mean inner copper, which is named by number instead.
    assert_eq!(layer_of("copper"), Err(ErrorCode::InvalidParams));
    let pcb = pcb(&engine);
    assert_eq!(pcb.graphic(l1).unwrap().layer, top);
    assert_eq!(pcb.graphic(l2).unwrap().layer, bottom);
    assert_eq!(pcb.copper_number(top), Some(1));
    assert_eq!(pcb.copper_number(bottom), Some(2));
}

#[test]
fn fab_notes_are_free_text_on_the_board_that_saves_clears_and_undoes() {
    let dir = TempDir::new("stackup-fab-notes");
    let mut engine = engine(&dir);
    assert_eq!(pcb(&engine).setup().fab_notes, None);
    let text = "Please route the slot with a 0.8 mm cutter.\nNo X-outs.";
    engine.run_ok("pcb.fab_notes.set", json!({"text": text}));
    assert_eq!(pcb(&engine).setup().fab_notes.as_deref(), Some(text));

    engine.run_ok("project.save", json!({}));
    let file = std::fs::read_to_string(dir.path().join("Demo/Demo.redapcb")).unwrap();
    assert!(file.contains(r#""fab_notes":"Please route"#), "{file}");
    engine.run_ok(
        "project.open",
        json!({"path": dir.path().join("Demo/Demo.reda")}),
    );
    assert_eq!(pcb(&engine).setup().fab_notes.as_deref(), Some(text));

    let document = pcb(&engine).uuid;
    engine.run_ok("pcb.fab_notes.set", json!({"text": "  "}));
    assert_eq!(pcb(&engine).setup().fab_notes, None);
    engine.run_ok("edit.undo", json!({"document": document}));
    assert_eq!(pcb(&engine).setup().fab_notes.as_deref(), Some(text));
    engine.run_ok("pcb.fab_notes.set", json!({"text": null}));
    assert_eq!(pcb(&engine).setup().fab_notes, None);
}

#[test]
fn dielectric_material_edits_preserve_omitted_fields_and_round_trip() {
    let dir = TempDir::new("stackup-sheet-data");
    let mut engine = engine(&dir);
    let (_, _, sheet) = ids(&engine);
    let edit = json!({"uuid":sheet,"kind":"prepreg","name":"Bond film","thickness":"0.2mm",
        "material":"FR408HR","dk":4180,"df":9200,"test_frequency":10_000_000_000_u64});
    engine.run_ok("pcb.stackup.set", json!({"dielectrics":[edit]}));
    engine.run_ok(
        "pcb.stackup.set",
        json!({"dielectrics":[{"uuid":sheet,"material":"7628","df":null}]}),
    );
    engine.run_ok("project.save", json!({}));
    engine.run_ok("project.open", json!({"path":dir.path().join("Demo")}));
    assert_eq!(
        serde_json::to_value(stackup(&engine).dielectrics().next()).unwrap(),
        json!({"uuid":sheet,"kind":"prepreg","name":"Bond film","thickness":200000,
               "material":"7628","dk":4180,"test_frequency":10_000_000_000_u64})
    );
}

#[test]
fn named_inner_copper_and_roles_survive_save_and_can_be_reset() {
    let dir = TempDir::new("stackup-layer-data");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(), "name":"Demo", "layers":3}),
    );
    let inner = pcb(&engine).copper_layers().nth(1).unwrap().uuid;
    let top = pcb(&engine).copper_layers().next().unwrap().uuid;
    engine.run_ok(
        "pcb.layer.set",
        json!({"layer":inner,"role":"mixed","name":"GND"}),
    );
    engine.run_ok("pcb.layer.set", json!({"layer":top,"role":"plane"}));
    engine.run_ok("project.save", json!({}));
    engine.run_ok("project.open", json!({"path":dir.path().join("Demo")}));
    let row = |e: &TestEngine| serde_json::to_value(pcb(e).layers().layer(inner).unwrap()).unwrap();
    assert_eq!(
        row(&engine),
        json!({"uuid":inner,"kind":"copper","role":"mixed","name":"GND"})
    );
    engine.run_ok(
        "pcb.layer.set",
        json!({"layer":inner,"role":"signal","name":null}),
    );
    assert_eq!(row(&engine), json!({"uuid":inner,"kind":"copper"}));
    assert_eq!(
        engine
            .run_err("pcb.layer.set", json!({"layer":top,"name":"outside"}))
            .code,
        ErrorCode::Rejected
    );
}

#[test]
fn adding_and_removing_a_sheet_keeps_at_least_one_in_each_gap() {
    let dir = TempDir::new("stackup-multiple-sheets");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":3}),
    );
    let original = stackup(&engine).clone();
    let gaps: Vec<_> = original.dielectrics().map(|d| d.uuid).collect();
    let extra = Uuid::from_u128(0xbeef);
    let added = engine.run_ok(
        "pcb.stackup.dielectric.add",
        json!({"beside":gaps[0],"uuid":extra}),
    );
    assert_eq!(added["result"]["uuid"], json!(extra));
    let sheets = [gaps[0], extra, gaps[1]];
    assert_eq!(
        stackup(&engine)
            .dielectrics()
            .map(|d| d.uuid)
            .collect::<Vec<_>>(),
        sheets
    );
    let err = engine.run_err(
        "pcb.stackup.dielectric.remove",
        json!({"dielectric":gaps[1]}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    engine.run_ok("pcb.stackup.dielectric.remove", json!({"dielectric":extra}));
    assert_eq!(stackup(&engine), &original);
    let document = pcb(&engine).uuid;
    engine.run_ok("edit.undo", json!({"document":document}));
    assert_eq!(
        stackup(&engine)
            .dielectrics()
            .map(|d| d.uuid)
            .collect::<Vec<_>>(),
        sheets
    );
}

#[test]
fn stackup_info_resolves_equal_shares_and_checks_mirrored_material_and_thickness() {
    let dir = TempDir::new("stackup-checks");
    let mut engine = engine(&dir);
    let (top, _, first) = ids(&engine);
    let document = pcb(&engine).uuid;
    let added = engine.run_ok("pcb.stackup.dielectric.add", json!({"beside":first}));
    let second = &added["result"]["uuid"];
    let info =
        |engine: &mut TestEngine| engine.run_ok("pcb.stackup.info", json!({"document":document}));
    engine.run_ok("pcb.stackup.set", json!({"thickness":1_600_001}));
    engine.run_ok("pcb.layer.set", json!({"layer":top,"role":"mixed"}));
    let readout = info(&mut engine);
    assert_eq!(readout["symmetric"], true);
    assert_eq!(
        readout["entries"][1],
        json!({"uuid":first, "thickness":765000.5})
    );
    assert_eq!(
        readout["entries"][2],
        json!({"uuid":second, "thickness":765000.5})
    );

    let err = engine.run_err("pcb.stackup.set", json!({"thickness":69999}));
    assert_eq!(err.details.unwrap()["pointer"], "/thickness");
    engine.run_ok(
        "pcb.stackup.set",
        json!({"dielectrics":[{"uuid":first,"material":"FR408HR"}]}),
    );
    let readout = info(&mut engine);
    assert_eq!(readout["symmetric"], false);
    assert_eq!(readout["symmetry_mismatches"], json!([[first, second]]));

    engine.run_ok(
        "pcb.stackup.set",
        json!({"thickness":1000000, "dielectrics":[
        {"uuid":first, "material":null, "thickness":515000},{"uuid":second, "thickness":515000}]}),
    );
    let readout = info(&mut engine);
    assert_eq!(readout["symmetric"], true);
    assert_eq!(readout["thickness_warning"], false, "exactly ten percent");
    engine.run_ok(
        "pcb.stackup.set",
        json!({"dielectrics":[{"uuid":second,"thickness":515001}]}),
    );
    assert_eq!(info(&mut engine)["thickness_warning"], true);
}

#[test]
fn an_odd_copper_count_is_valid_and_not_symmetric() {
    let dir = TempDir::new("stackup-odd-check");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":3}),
    );
    let document = pcb(&engine).uuid;
    let info = engine.run_ok("pcb.stackup.info", json!({"document":document}));
    assert_eq!(info["symmetric"], false);
    assert_eq!(info["thickness_warning"], false);
}

#[test]
fn an_unrepresentable_sum_is_refused_without_losing_the_previous_stackup() {
    let dir = TempDir::new("stackup-thickness-overflow");
    let mut engine = engine(&dir);
    let (_, _, sheet) = ids(&engine);
    let before = stackup(&engine).clone();
    let err = engine.run_err(
        "pcb.stackup.set",
        json!({"dielectrics":[{"uuid":sheet, "thickness":i64::MAX}]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(stackup(&engine), &before);
}

#[test]
fn impedance_uses_physical_references_and_the_width_on_each_layer() {
    use reda_pcb::impedance;

    let dir = TempDir::new("stackup-impedance");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":6}),
    );
    let layers: Vec<_> = pcb(&engine).copper_layers().map(|l| l.uuid).collect();
    let sheets: Vec<_> = stackup(&engine).dielectrics().map(|d| d.uuid).collect();
    let impedance = |e: &TestEngine| {
        let board = pcb(e);
        impedance::layers(board, &board.stackup().info())
    };
    assert!(impedance(&engine).iter().all(|r| r.estimate.is_none()));
    for &id in &[layers[1], layers[3]] {
        engine.run_ok("pcb.layer.set", json!({"layer":id,"role":"plane"}));
    }
    for &id in &sheets {
        engine.run_ok(
            "pcb.stackup.set",
            json!({"dielectrics":[{"uuid":id,"thickness":200000,"dk":4400,"material":"FR4"}]}),
        );
    }
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"track_width","scope":{"layers":"inner"},"values":{"preferred":"0.35mm"}}),
    );
    let readings = impedance(&engine);
    assert_eq!(readings[2].width, Nm(350_000));
    let models = |rows: &[impedance::LayerImpedance]| {
        rows.iter()
            .map(|r| r.estimate.as_ref().map(|z| z.model))
            .collect::<Vec<_>>()
    };
    use impedance::Model::*;
    assert_eq!(
        models(&readings),
        [
            Some(SurfaceMicrostrip),
            None,
            Some(CentredStripline),
            None,
            Some(EmbeddedMicrostrip),
            Some(SurfaceMicrostrip)
        ]
    );
    assert_eq!(
        readings[5].estimate.as_ref().unwrap().references,
        vec![layers[3]]
    );
    assert!(!readings[0].estimate.as_ref().unwrap().missing_dk);
    engine.run_ok(
        "pcb.stackup.set",
        json!({"dielectrics":[{"uuid":sheets[2],"dk":null}]}),
    );
    engine.run_ok("pcb.layer.set", json!({"layer":layers[1],"role":"mixed"}));
    let readings = impedance(&engine);
    let inner = readings[2].estimate.as_ref().unwrap();
    assert_eq!(inner.model, OffsetStripline);
    assert!(inner.missing_dk);
    assert_eq!(inner.references, vec![layers[1], layers[3]]);
    assert_eq!(
        readings[1].estimate.as_ref().unwrap().model,
        EmbeddedMicrostrip
    );
}
