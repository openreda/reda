//! Free text on the board, through Commands.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{LayerKind, PcbDocument, Side, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::json;

fn engine_with_pcb(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok("document.add", json!({"kind": "pcb"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn layer(engine: &TestEngine, kind: LayerKind, side: Option<Side>) -> Uuid {
    pcb(engine).layers().find(kind, side).unwrap().uuid
}

fn pointer(err: &reda_command::CommandError) -> &str {
    err.details
        .as_ref()
        .and_then(|d| d["pointer"].as_str())
        .unwrap_or("")
}

fn created(out: &serde_json::Value) -> Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

#[test]
fn text_is_added_on_a_layer_named_by_kind_and_side() {
    let dir = TempDir::new("text-add");
    let mut engine = engine_with_pcb(&dir);
    let out = engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "REDA v0.1", "x": "10mm", "y": "5mm",
               "height": "1.5mm", "rotation": "90deg"}),
    );
    let silk = layer(&engine, LayerKind::Silkscreen, Some(Side::Top));
    let text = pcb(&engine).text(created(&out)).unwrap();
    assert_eq!(text.layer, silk);
    assert_eq!(text.text, "REDA v0.1");
    assert_eq!(text.position, Point::new(Nm::from_mm(10), Nm::from_mm(5)));
    assert_eq!(text.height, Nm::from_um(1500));
    assert_eq!(text.rotation, Millideg::from_deg(90));
    assert_eq!(text.font, "Hershey Roman Simplex");
    assert!(!text.mirrored);
}

#[test]
fn text_is_refused_on_layers_that_do_not_take_graphics() {
    let dir = TempDir::new("text-layers");
    let mut engine = engine_with_pcb(&dir);
    for name in [
        "solder_mask_top",
        "paste_bottom",
        "courtyard_top",
        "board_outline",
    ] {
        let err = engine.run_err(
            "pcb.text.add",
            json!({"layer": name, "text": "X", "x": 0, "y": 0}),
        );
        assert_eq!(pointer(&err), "/layer", "{name}: {}", err.message);
    }
    for name in [
        "copper_top",
        "copper_bottom",
        "silkscreen_bottom",
        "mechanical",
        "documentation",
    ] {
        engine.run_ok(
            "pcb.text.add",
            json!({"layer": name, "text": "X", "x": 0, "y": 0}),
        );
    }
    let mech = layer(&engine, LayerKind::Mechanical, None);
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": mech, "text": "by uuid", "x": 0, "y": 0}),
    );
    let err = engine.run_err(
        "pcb.text.add",
        json!({"layer": "silk", "text": "X", "x": 0, "y": 0}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
}

#[test]
fn bottom_text_is_mirrored_by_default_and_top_text_cannot_be() {
    let dir = TempDir::new("text-mirror");
    let mut engine = engine_with_pcb(&dir);
    let bottom = engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_bottom", "text": "B", "x": 0, "y": 0}),
    );
    let plain = engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_bottom", "text": "B", "x": 0, "y": 0, "mirrored": false}),
    );
    assert!(pcb(&engine).text(created(&bottom)).unwrap().mirrored);
    assert!(!pcb(&engine).text(created(&plain)).unwrap().mirrored);
    let err = engine.run_err(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "T", "x": 0, "y": 0, "mirrored": true}),
    );
    assert_eq!(pointer(&err), "/mirrored");
}

#[test]
fn moving_text_between_sides_follows_the_mirroring_default() {
    let dir = TempDir::new("text-move");
    let mut engine = engine_with_pcb(&dir);
    let uuid = created(&engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "A", "x": 0, "y": 0}),
    ));
    engine.run_ok(
        "pcb.text.set",
        json!({"uuid": uuid, "layer": "silkscreen_bottom"}),
    );
    let text = pcb(&engine).text(uuid).unwrap();
    assert!(text.mirrored);
    assert_eq!(
        text.layer,
        layer(&engine, LayerKind::Silkscreen, Some(Side::Bottom))
    );
    engine.run_ok(
        "pcb.text.set",
        json!({"uuid": uuid, "text": "AB", "font": "Noto Sans", "x": "1mm", "y": "2mm",
               "height": "2mm", "rotation": "-90deg", "mirrored": false}),
    );
    let text = pcb(&engine).text(uuid).unwrap();
    assert_eq!(
        (text.text.as_str(), text.font.as_str()),
        ("AB", "Noto Sans")
    );
    assert_eq!(text.rotation, Millideg::from_deg(270));
    assert_eq!(text.position, Point::new(Nm::from_mm(1), Nm::from_mm(2)));
    assert!(!text.mirrored);
    engine.run_ok("pcb.text.set", json!({"uuid": uuid, "layer": "copper_top"}));
    assert!(!pcb(&engine).text(uuid).unwrap().mirrored);
}

#[test]
fn text_params_are_checked() {
    let dir = TempDir::new("text-params");
    let mut engine = engine_with_pcb(&dir);
    let base = json!({"layer": "silkscreen_top", "text": "A", "x": 0, "y": 0});
    for (key, value, expected) in [
        ("text", json!("  "), "/text"),
        ("height", json!(0), "/height"),
        ("thickness", json!("-1mm"), "/thickness"),
        ("font", json!(""), "/font"),
    ] {
        let mut params = base.clone();
        params[key] = value;
        let err = engine.run_err("pcb.text.add", params);
        assert_eq!(pointer(&err), expected, "{key}: {}", err.message);
    }
}

#[test]
fn deleting_text_undoes_with_the_document() {
    let dir = TempDir::new("text-delete");
    let mut engine = engine_with_pcb(&dir);
    let uuid = created(&engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "A", "x": 0, "y": 0}),
    ));
    engine.run_ok("pcb.text.delete", json!({"uuid": uuid}));
    assert!(pcb(&engine).text(uuid).is_none());
    let doc = pcb(&engine).uuid;
    engine.run_ok("edit.undo", json!({"document": doc}));
    assert!(pcb(&engine).text(uuid).is_some());
    let err = engine.run_err("pcb.text.delete", json!({"uuid": doc}));
    assert_eq!(err.code, reda_command::ErrorCode::NotFound);
}
