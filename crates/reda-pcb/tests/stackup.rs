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
fn a_new_board_is_1_6_mm_with_1_oz_copper_and_one_blank_dielectric() {
    let dir = TempDir::new("stackup-new");
    let engine = engine(&dir);
    let (top, bottom, dielectric) = ids(&engine);
    let stackup = stackup(&engine);
    assert_eq!(stackup.thickness, Nm(1_600_000));
    assert_eq!(
        serde_json::to_value(&stackup.entries).unwrap(),
        json!([
            {"layer": top, "thickness": 35_000},
            {"uuid": dielectric},
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
        stackup(&engine).entries[1],
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
