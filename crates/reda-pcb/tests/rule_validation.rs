//! `pcb.rules.validate`: blocking problems on effective values, and notes that never
//! block.
//!
//! The board is the Generic preset's, and the new Project's Output Job uses Generic:
//! clearance 0.16 mm, Track width 0.20 mm min and 0.25 mm preferred.

use reda_model::Uuid;
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

fn class(engine: &mut TestEngine, name: &str, nets: &[Uuid]) -> Uuid {
    uuid(&engine.run_ok("pcb.net_class.add", json!({"name": name, "nets": nets}))["result"]["uuid"])
}

/// `(note, value)` of every note.
fn notes(out: &Value) -> Vec<(String, Value)> {
    out["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["note"].as_str().unwrap().to_owned(), n["value"].clone()))
        .collect()
}

#[test]
fn a_fresh_board_validates_clean() {
    let dir = TempDir::new("validate-clean");
    let mut engine = engine(&dir);
    let out = engine.run_ok("pcb.rules.validate", json!({}));
    assert_eq!(out, json!({"problems": [], "notes": []}));
}

#[test]
fn notes_name_what_is_odd_without_blocking() {
    let dir = TempDir::new("validate-notes");
    let mut engine = engine(&dir);
    let vcc = net(&mut engine, "VCC");
    let power = class(&mut engine, "Power", &[vcc]);
    let empty = class(&mut engine, "Empty", &[]);
    // Weaker than the board, and below Generic's 0.16 mm.
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "scope": {"net_class": power}, "values": {"value": "0.12mm"}}),
    );
    // Equal to what it inherits.
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "track_width", "scope": {"net_class": empty}, "values": {"preferred": "0.25mm"}}),
    );
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "paste_expansion", "enabled": false}),
    );
    let out = engine.run_ok("pcb.rules.validate", json!({}));
    assert_eq!(out["problems"], json!([]));
    assert_eq!(
        notes(&out),
        [
            ("outside_preset".into(), json!("value")),
            ("weaker_than_board".into(), json!("value")),
            ("same_as_inherited".into(), json!("preferred")),
            ("class_without_nets".into(), Value::Null),
            ("outputs_change".into(), Value::Null),
        ],
        "{out:#}"
    );
    let all = out["notes"].as_array().unwrap();
    assert_eq!(all[0]["preset"], "Generic");
    assert_eq!(all[0]["net_class"], json!(power));
    assert_eq!(all[3]["net_class"], json!(empty));
    assert_eq!(all[4]["kind"], "paste_expansion");
    assert!(all.iter().all(|n| n["message"].is_string()));
}

#[test]
fn a_net_whose_classes_combine_into_an_invalid_range_is_a_note_naming_it() {
    let dir = TempDir::new("validate-net-range");
    let mut engine = engine(&dir);
    let vcc = net(&mut engine, "VCC");
    let wide = class(&mut engine, "Wide", &[vcc]);
    let narrow = class(&mut engine, "Narrow", &[vcc]);
    // Each class is valid against the board, but together min 0.5 mm > max 0.3 mm.
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "track_width", "scope": {"net_class": wide},
               "values": {"min": "0.5mm", "preferred": "0.6mm"}}),
    );
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "track_width", "scope": {"net_class": narrow}, "values": {"max": "0.3mm"}}),
    );
    let out = engine.run_ok("pcb.rules.validate", json!({}));
    assert_eq!(out["problems"], json!([]), "{out:#}");
    let net_notes: Vec<&Value> = out["notes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["note"] == "net_range_invalid")
        .collect();
    assert_eq!(net_notes.len(), 2, "{out:#}");
    assert_eq!(net_notes[0]["net"], json!(vcc));
    assert_eq!(net_notes[0]["kind"], "track_width");
    assert_eq!(net_notes[0]["value"], "min");
    assert_eq!(net_notes[1]["value"], "preferred");
    assert!(
        net_notes[0]["message"].as_str().unwrap().contains("VCC"),
        "{out:#}"
    );
}

#[test]
fn named_layer_ranges_inherit_the_plane_and_inner_values() {
    let dir = TempDir::new("validate-layer-ranges");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Four","layers":4}),
    );
    let inner = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .nth(1)
        .unwrap()
        .uuid;
    engine.run_ok("pcb.layer.set", json!({"layer":inner,"role":"plane"}));
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"track_width","scope":{"layers":"inner"},"values":{"preferred":"0.4mm"}}),
    );
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"track_width","scope":{"layers":"plane"},"values":{"preferred":"0.5mm"}}),
    );
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"track_width","scope":{"layers":[inner]},"values":{"min":"0.45mm"}}),
    );
    let invalid = engine.run_err(
        "pcb.rules.set",
        json!({"kind":"track_width","scope":{"layers":"plane"},"values":{"preferred":null}}),
    );
    assert_eq!(invalid.code, reda_command::ErrorCode::InvalidParams);
    assert_eq!(
        engine.run_ok("pcb.rules.validate", json!({}))["problems"],
        json!([])
    );
}

#[test]
fn validation_explains_an_empty_layer_group_and_a_relaxed_pair() {
    let dir = TempDir::new("layer-pair-notes");
    let mut engine = engine(&dir);
    let power = class(&mut engine, "Power", &[]);
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","scope":{"net_class":power},"values":{"value":"1mm"}}),
    );
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","scope":{"layers":"inner"},"values":{"value":"0.2mm"}}),
    );
    let scope = json!({"net_class_pair":["Default","Power"]});
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","scope":scope,"values":{"value":"0.4mm"}}),
    );
    let out = engine.run_ok("pcb.rules.validate", json!({"scope":scope}));
    assert!(
        notes(&out).iter().any(|(n, _)| n == "pair_below_class"),
        "{out}"
    );
    // A pair equal to the board still relaxes Power. With no pair it inherits Power's 1 mm.
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","scope":scope,"values":{"value":"0.16mm"}}),
    );
    let out = engine.run_ok("pcb.rules.validate", json!({"scope":scope}));
    assert!(!notes(&out).iter().any(|(n, _)| n == "same_as_inherited"));
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","scope":scope,"values":{"value":"1mm"}}),
    );
    let out = engine.run_ok("pcb.rules.validate", json!({"scope":scope}));
    assert!(notes(&out).iter().any(|(n, _)| n == "same_as_inherited"));
    let out = engine.run_ok("pcb.rules.validate", json!({"scope":{"layers":"inner"}}));
    assert!(
        notes(&out).iter().any(|(n, _)| n == "no_matching_layers"),
        "{out}"
    );
}

#[test]
fn an_inner_fab_limit_is_compared_only_where_the_record_supplies_it() {
    let dir = TempDir::new("layer-preset-limits");
    let mut e = probe_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Four","layers":4}),
    );
    let preset = e.project().unwrap().fab_presets().next().unwrap().uuid;
    e.run_ok(
        "fab_preset.set",
        json!({"preset":preset,"values":{"rules":{"inner":{"clearance":300000}}}}),
    );
    let outside = |e: &mut TestEngine| {
        e.run_ok("pcb.rules.validate", json!({}))["notes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["note"] == "outside_preset")
            .count()
    };
    assert_eq!(outside(&mut e), 1);
    e.run_ok("pcb.rules.apply_fab_preset", json!({"preset":preset}));
    assert_eq!(outside(&mut e), 0);
}

#[test]
fn overlapping_named_layers_and_invalid_scope_combinations_are_refused() {
    let dir = TempDir::new("layer-scope-validation");
    let mut e = engine(&dir);
    let layers: Vec<_> = e
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    let class = class(&mut e, "Power", &[]);
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","scope":{"layers":layers},"values":{"value":"0.2mm"}}),
    );
    for (kind, scope, values) in [
        (
            "clearance",
            json!({"layers":[layers[0]]}),
            json!({"value":"0.5mm"}),
        ),
        (
            "clearance",
            json!({"layers":[class]}),
            json!({"value":"0.5mm"}),
        ),
        (
            "clearance",
            json!({"net_class":class,"net_class_pair":["Default","Power"]}),
            json!({"value":"0.2mm"}),
        ),
        (
            "clearance",
            json!({"net_class_pair":["Default","Power"]}),
            json!({"ignore_pad_to_pad":true}),
        ),
        (
            "track_width",
            json!({"net_class_pair":["Default","Power"]}),
            json!({"min":"0.2mm"}),
        ),
        (
            "hole_size",
            json!({"layers":"outer"}),
            json!({"min":"0.2mm"}),
        ),
    ] {
        let err = e.run_err(
            "pcb.rules.set",
            json!({"kind":kind,"scope":scope,"values":values}),
        );
        assert_eq!(err.code, reda_command::ErrorCode::InvalidParams, "{err:?}");
    }
    // Same class, same list in reverse order edits the original record.
    e.run_ok("pcb.rules.set",json!({"kind":"clearance","scope":{"layers":[layers[1],layers[0]]},"values":{"value":"0.55mm"}}));
    assert_eq!(
        e.project()
            .unwrap()
            .pcb()
            .unwrap()
            .rules()
            .filter(|r| r.scope.layers.is_some())
            .count(),
        1
    );
}
