//! Moving rules between boards: `pcb.rules.export` writes a `.redarules` file and
//! `pcb.rules.import` reads it into another board, with records, severities and class
//! names but no Net membership.

use reda_model::{RuleKind, Severity};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn project(dir: &TempDir, name: &str) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": name}));
    engine
}

fn uuid(v: &Value) -> reda_model::Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// A board with a Power class holding VCC, class and board records and a changed severity,
/// exported to `<dir>/house.redarules`.
fn exported(dir: &TempDir) -> std::path::PathBuf {
    let mut engine = project(dir, "Source");
    let vcc = engine.run_ok("test.net.add", json!({"name": "VCC"}))["result"]["uuid"].clone();
    engine.run_ok(
        "pcb.net_class.add",
        json!({"name": "Power", "nets": [vcc],
               "rules": [{"kind": "track_width", "values": {"min": "0.4mm", "preferred": "0.5mm"}}]}),
    );
    engine.run_ok("pcb.net_class.add", json!({"name": "Spare"}));
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"value": "0.3mm"}, "comment": "house"}),
    );
    engine.run_ok(
        "drc.set_severity",
        json!({"check": "dangling", "severity": "off"}),
    );
    let path = dir.path().join("house.redarules");
    let out = engine.run_ok("pcb.rules.export", json!({"path": path}));
    assert_eq!(out["path"], json!(path));
    assert_eq!(out["rules"], RuleKind::ALL.len() + 1);
    path
}

#[test]
fn an_exported_rules_file_holds_records_severities_and_class_names_but_no_nets() {
    let dir = TempDir::new("rules-export");
    let path = exported(&dir);
    let text = std::fs::read_to_string(&path).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["format"], reda_engine::format::FORMAT);
    assert_eq!(
        file["net_classes"],
        json!([{"name": "Power"}, {"name": "Spare"}])
    );
    let power: Vec<&Value> = file["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["net_class"] == "Power")
        .collect();
    assert_eq!(
        power,
        [&json!({"kind": "track_width", "net_class": "Power",
                 "values": {"min": 400_000, "preferred": 500_000}})]
    );
    assert_eq!(file["severities"]["dangling"], "off");
    assert!(!text.contains("VCC"), "{text}");
    assert!(!text.contains("uuid"), "{text}");
}

#[test]
fn importing_previews_then_overwrites_the_chosen_kinds_and_creates_missing_classes_empty() {
    let dir = TempDir::new("rules-import");
    let path = exported(&dir);
    let target = TempDir::new("rules-import-target");
    let mut engine = project(&target, "Target");
    let gnd = engine.run_ok("test.net.add", json!({"name": "GND"}))["result"]["uuid"].clone();
    let spare = uuid(
        &engine.run_ok("pcb.net_class.add", json!({"name": "Spare", "nets": [gnd]}))["result"]["uuid"],
    );
    let before = engine.project().unwrap().pcb().unwrap().clone();

    let preview =
        engine.run_ok("pcb.rules.import", json!({"path": path, "preview": true}))["result"].clone();
    assert_eq!(preview["applied"], false);
    assert_eq!(preview["new_classes"], json!(["Power"]));
    assert_eq!(
        preview["severities"],
        json!([{"check": "dangling", "before": "warning", "after": "off"}])
    );
    let clearance = preview["overwrites"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["kind"] == "clearance")
        .unwrap();
    assert_eq!(clearance["before"]["value"], 160_000);
    assert_eq!(clearance["after"]["value"], 300_000);
    assert_eq!(engine.project().unwrap().pcb().unwrap(), &before);

    // Only the Track width records, without severities.
    engine.run_ok(
        "pcb.rules.import",
        json!({"path": path, "kinds": ["track_width"], "severities": false}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let power = pcb.net_classes().find(|c| c.name == "Power").unwrap();
    assert!(
        pcb.nets().all(|n| !n.classes.contains(&power.uuid)),
        "created empty"
    );
    assert_eq!(
        pcb.class_rule(RuleKind::TrackWidth, power.uuid)
            .unwrap()
            .values
            .min,
        Some(reda_model::geom::Nm::from_um(400))
    );
    assert_eq!(
        pcb.board_rule(RuleKind::Clearance).values.value,
        Some(reda_model::geom::Nm::from_um(160))
    );
    assert_eq!(
        pcb.setup().severities.get(reda_model::DrcCheck::Dangling),
        Severity::Warning
    );
    // The existing class matched by name keeps its Nets.
    assert_eq!(pcb.net_classes().filter(|c| c.name == "Spare").count(), 1);
    assert!(pcb.nets().any(|n| n.classes.contains(&spare)));

    // Everything, as one undo step.
    let out = engine.run_ok("pcb.rules.import", json!({"path": path}));
    assert_eq!(out["result"]["applied"], true);
    assert_eq!(out["result"]["new_classes"], json!([]));
    let pcb = engine.project().unwrap().pcb().unwrap();
    assert_eq!(
        pcb.board_rule(RuleKind::Clearance).values.value,
        Some(reda_model::geom::Nm::from_um(300))
    );
    assert_eq!(
        pcb.board_rule(RuleKind::Clearance).comment.as_deref(),
        Some("house")
    );
    assert_eq!(
        pcb.setup().severities.get(reda_model::DrcCheck::Dangling),
        Severity::Off
    );
    let document = pcb.uuid;
    engine.run_ok("edit.undo", json!({"document": document}));
    let pcb = engine.project().unwrap().pcb().unwrap();
    assert_eq!(
        pcb.board_rule(RuleKind::Clearance).values.value,
        Some(reda_model::geom::Nm::from_um(160))
    );
    assert_eq!(
        pcb.setup().severities.get(reda_model::DrcCheck::Dangling),
        Severity::Warning
    );
}

#[test]
fn a_rules_file_naming_an_unlisted_class_or_a_relative_path_is_refused() {
    let dir = TempDir::new("rules-import-bad");
    let mut engine = project(&dir, "Demo");
    let err = engine.run_err("pcb.rules.import", json!({"path": "house.redarules"}));
    assert_eq!(err.code, reda_engine::command::ErrorCode::InvalidParams);
    let path = dir.path().join("bad.redarules");
    let file = json!({"format": 1, "net_classes": [],
        "rules": [{"kind": "clearance", "net_class": "Power", "values": {"value": 1}}],
        "severities": {}});
    std::fs::write(&path, file.to_string()).unwrap();
    let err = engine.run_err("pcb.rules.import", json!({"path": path}));
    assert_eq!(err.code, reda_engine::command::ErrorCode::Rejected);
    assert!(err.message.contains("Power"), "{}", err.message);
}

#[test]
fn rule_files_export_scopes_and_via_permissions_by_name_and_physical_position() {
    let dir = TempDir::new("portable-scopes");
    let mut engine = project(&dir, "Source");
    let document = engine.project().unwrap().pcb().unwrap().uuid;
    for _ in 0..2 {
        engine.run_ok("pcb.layer.add", json!({"document":document}));
    }
    let layers: Vec<_> = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    let sense =
        engine.run_ok("pcb.net_class.add", json!({"name":"Sense"}))["result"]["uuid"].clone();
    let power =
        engine.run_ok("pcb.net_class.add", json!({"name":"Power"}))["result"]["uuid"].clone();
    let laser = engine.run_ok(
        "pcb.via_type.add",
        json!({"document":document,"from":layers[3],"to":layers[2],"micro":true}),
    )["result"]["uuid"]
        .clone();
    engine.run_ok("pcb.rules.set",json!({"kind":"clearance","scope":{"net_class_pair":["Power","Sense"],"layers":[layers[2],layers[1]]},"values":{"value":"0.5mm"}}));
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"via_span","scope":{"net_class":power},"values":{"allowed":[laser]}}),
    );
    let path = dir.path().join("scoped.redarules");
    engine.run_ok("pcb.rules.export", json!({"path":path}));
    let raw = std::fs::read_to_string(&path).unwrap();
    for id in [&power, &sense, &laser] {
        assert!(!raw.contains(id.as_str().unwrap()));
    }
    let file: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(file["copper_layers"], 4);
    let rules = file["rules"].as_array().unwrap();
    assert!(
        rules
            .iter()
            .any(|r| r["layers"] == json!([2, 3])
                && r["net_class_pair"] == json!(["Power", "Sense"]))
    );
    assert!(rules.iter().any(|r| r["net_class"] == "Power"
        && r["values"]["allowed"] == json!([{"from":4,"to":3,"micro":true}])));

    // The same engine keeps minting distinct UUIDs for the second Project.
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Target","layers":4}),
    );
    let target = engine.project().unwrap().pcb().unwrap();
    let target_id = target.uuid;
    let target_layers: Vec<_> = target.copper_layers().map(|l| l.uuid).collect();
    assert!(target_layers.iter().all(|layer| !layers.contains(layer)));
    let target_laser = engine.run_ok(
        "pcb.via_type.add",
        json!({"document":target_id,"from":target_layers[3],"to":target_layers[2],"micro":true}),
    )["result"]["uuid"]
        .clone();
    assert_ne!(target_laser, laser);
    engine.run_ok("pcb.rules.import", json!({"path":path}));
    let target = engine.project().unwrap().pcb().unwrap();
    let target_power = target.net_class_named("Power").unwrap();
    let target_sense = target.net_class_named("Sense").unwrap();
    assert_ne!(uuid(&power), target_power.uuid);
    assert_ne!(uuid(&sense), target_sense.uuid);
    assert_eq!(
        target
            .class_rule(RuleKind::ViaSpan, target_power.uuid)
            .unwrap()
            .values
            .allowed
            .as_deref(),
        Some([uuid(&target_laser)].as_slice())
    );
    assert!(target_sense.rank < target_power.rank);
    let copy = dir.path().join("target.redarules");
    engine.run_ok("pcb.rules.export", json!({"path":copy}));
    assert_eq!(std::fs::read_to_string(copy).unwrap(), raw);
}

/// Apply the same portable file to each copper count, including a reversed laser entry.
#[test]
fn selected_position_records_map_or_drop_by_layer_and_do_not_broaden_via_permissions() {
    let dir = TempDir::new("map-position-rules");
    let path = dir.path().join("scoped.redarules");
    let file = json!({"format":2,"copper_layers":4,
        "net_classes":[{"name":"Power"},{"name":"LaserOnly"}],"severities":{},
        "rules":[
            {"kind":"clearance","net_class_pair":["Default","Power"],"layers":[2,3],"values":{"value":310000}},
            {"kind":"via_span","net_class":"Power","values":{"allowed":[{"from":1,"to":4,"micro":false},{"from":4,"to":3,"micro":true}]}},
            {"kind":"via_span","net_class":"LaserOnly","values":{"allowed":[{"from":4,"to":3,"micro":true}]}},
            {"kind":"clearance","layers":"plane","values":{"value":270000}}]});
    std::fs::write(&path, file.to_string()).unwrap();
    for count in [2, 4] {
        let mut engine = project(&dir, &format!("Target{count}"));
        let document = engine.project().unwrap().pcb().unwrap().uuid;
        for _ in 2..count {
            engine.run_ok("pcb.layer.add", json!({"document":document}));
        }
        let pcb = engine.project().unwrap().pcb().unwrap();
        let copper: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
        let through = pcb.stackup().through_type().uuid;
        let power =
            engine.run_ok("pcb.net_class.add", json!({"name":"power"}))["result"]["uuid"].clone();
        engine.run_ok(
            "pcb.rules.set",
            json!({"kind":"via_span","scope":{"net_class":power},"values":{"allowed":[through]}}),
        );
        let initial = engine.project().unwrap().pcb().unwrap().clone();
        let preview =
            engine.run_ok("pcb.rules.import", json!({"path":path,"preview":true}))["result"]
                .clone();
        assert_eq!(engine.project().unwrap().pcb().unwrap(), &initial);
        let rows = preview["position_records"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["selected"], count == 4);
        assert_eq!(rows[0]["different_copper_count"], count != 4);
        assert_eq!(rows[0]["net_class_pair"], json!(["Default", "Power"]));
        assert_eq!(rows[2]["available"], false);
        assert_eq!(
            rows[1]["missing_types"].as_array().unwrap().len(),
            if count == 2 { 2 } else { 1 }
        );
        assert_eq!(
            rows[0]["missing_layers"],
            if count == 2 { json!([3]) } else { json!([]) }
        );
        assert_eq!(preview["new_classes"], json!([]));
        engine.run_ok(
            "pcb.rules.import",
            json!({"path":path,"position_records":[0,1,2]}),
        );
        let pcb = engine.project().unwrap().pcb().unwrap();
        let mut pair = [reda_model::Uuid::NIL, uuid(&power)];
        pair.sort();
        let mut named: Vec<_> = copper.iter().skip(1).take(2).copied().collect();
        named.sort();
        let scope = reda_model::RuleScope {
            net_class_pair: Some(pair),
            layers: Some(reda_model::RuleLayers::Named(named)),
            net_class: None,
        };
        assert_eq!(
            pcb.scoped_rule(RuleKind::Clearance, &scope)
                .unwrap()
                .values
                .value
                .unwrap()
                .0,
            310_000
        );
        assert!(pcb.net_class_named("LaserOnly").is_none());
        assert_eq!(
            pcb.class_rule(RuleKind::ViaSpan, uuid(&power))
                .unwrap()
                .values
                .allowed
                .as_deref(),
            Some(&[through][..])
        );
        assert!(
            pcb.scoped_rule(
                RuleKind::Clearance,
                &reda_model::RuleScope {
                    layers: Some(reda_model::RuleLayers::Group(reda_model::LayerGroup::Plane)),
                    ..Default::default()
                }
            )
            .is_some()
        );
        engine.run_ok("edit.undo", json!({"document":document}));
        assert_eq!(engine.project().unwrap().pcb().unwrap(), &initial);
        if count == 4 {
            let laser = engine.run_ok(
                "pcb.via_type.add",
                json!({"document":document,"from":copper[2],"to":copper[3],"micro":true}),
            )["result"]["uuid"]
                .clone();
            engine.run_ok("pcb.rules.import", json!({"path":path}));
            let pcb = engine.project().unwrap().pcb().unwrap();
            let new_class = pcb.net_class_named("LaserOnly").unwrap();
            assert_eq!(
                pcb.class_rule(RuleKind::ViaSpan, new_class.uuid)
                    .unwrap()
                    .values
                    .allowed
                    .as_deref(),
                Some(&[uuid(&laser)][..])
            );
        }
    }
}

#[test]
fn conflicting_position_selections_are_refused_atomically_and_the_user_can_pick_one() {
    let dir = TempDir::new("colliding-position-rules");
    let mut engine = project(&dir, "Target");
    let path = dir.path().join("collision.redarules");
    let file = json!({"format":2,"copper_layers":4,"net_classes":[],"severities":{},"rules":[
        {"kind":"clearance","layers":[1,3],"values":{"value":300000}},
        {"kind":"clearance","layers":[1,4],"values":{"value":400000}}]});
    std::fs::write(&path, file.to_string()).unwrap();
    let before = engine.project().unwrap().pcb().unwrap().clone();
    for (selection, error) in [
        (json!([0, 1]), "same target Layer"),
        (json!([2]), "index 2"),
    ] {
        let refusal = engine.run_err(
            "pcb.rules.import",
            json!({"path":path,"position_records":selection}),
        );
        assert!(refusal.message.contains(error), "{refusal:?}");
        assert_eq!(engine.project().unwrap().pcb().unwrap(), &before);
    }
    let out = engine.run_ok(
        "pcb.rules.import",
        json!({"path":path,"position_records":[1]}),
    );
    assert_eq!(out["result"]["overwrites"][0]["after"]["value"], 400000);
}

#[test]
fn preview_and_import_refuse_a_layer_shared_with_a_kept_rule() {
    let dir = TempDir::new("overlapping-import");
    let mut engine = project(&dir, "Target");
    let copper: Vec<_> = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","scope":{"layers":[copper[0]]},"values":{"value":"0.3mm"}}),
    );
    let path = dir.path().join("overlap.redarules");
    let rules = json!({"format":2,"copper_layers":2,"net_classes":[],"severities":{},
        "rules":[{"kind":"clearance","layers":[1,2],"values":{"value":400000}}]});
    std::fs::write(&path, rules.to_string()).unwrap();
    let before = engine.project().unwrap().pcb().unwrap().clone();
    for preview in [true, false] {
        let refusal = engine.run_err("pcb.rules.import", json!({"path":path,"preview":preview}));
        assert_eq!(refusal.code, reda_engine::command::ErrorCode::Rejected);
        assert!(refusal.message.contains("Layer"), "{refusal:?}");
        assert_eq!(engine.project().unwrap().pcb().unwrap(), &before);
    }
}

#[test]
fn imported_class_ranges_are_checked_against_the_target_layer_rules() {
    let dir = TempDir::new("import-layer-range");
    let mut engine = project(&dir, "Target");
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"track_width","scope":{"layers":"outer"},
        "values":{"min":"0.6mm","preferred":"0.65mm"}}),
    );
    let path = dir.path().join("class-range.redarules");
    let rules = json!({"format":2,"copper_layers":2,"net_classes":[{"name":"Power"}],"severities":{},
        "rules":[{"kind":"track_width","net_class":"Power","values":{"max":500000}}]});
    std::fs::write(&path, rules.to_string()).unwrap();
    let before = engine.project().unwrap().pcb().unwrap().clone();
    for preview in [true, false] {
        let refusal = engine.run_err("pcb.rules.import", json!({"path":path,"preview":preview}));
        assert_eq!(refusal.code, reda_engine::command::ErrorCode::Rejected);
        assert!(refusal.message.contains("track_width"), "{refusal:?}");
        assert_eq!(engine.project().unwrap().pcb().unwrap(), &before);
    }
}

#[test]
fn explicitly_unrestricted_via_permissions_can_replace_a_class_restriction() {
    let dir = TempDir::new("unrestricted-rules");
    let mut engine = project(&dir, "Source");
    let class =
        engine.run_ok("pcb.net_class.add", json!({"name": "Power"}))["result"]["uuid"].clone();
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"via_span","scope":{"net_class":class},"values":{"allowed":[]}}),
    );
    let path = dir.path().join("unrestricted.redarules");
    engine.run_ok("pcb.rules.export", json!({"path":path}));
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Target","layers":4}),
    );
    let through = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .stackup()
        .through_type()
        .uuid;
    let class =
        engine.run_ok("pcb.net_class.add", json!({"name": "Power"}))["result"]["uuid"].clone();
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"via_span","scope":{"net_class":class},"values":{"allowed":[through]}}),
    );
    let preview = engine.run_ok(
        "pcb.rules.import",
        json!({"path":path,"preview":true,"kinds":["via_span"]}),
    );
    let row = &preview["result"]["position_records"][0];
    assert_eq!(row["allowed"], json!([]));
    assert_eq!(row["available"], true);
    assert_eq!(row["selected"], false);
    engine.run_ok(
        "pcb.rules.import",
        json!({"path":path,"kinds":["via_span"],"position_records":[row["index"]]}),
    );
    let target = engine.project().unwrap().pcb().unwrap();
    assert_eq!(
        target
            .class_rule(RuleKind::ViaSpan, uuid(&class))
            .unwrap()
            .values
            .allowed,
        Some(vec![])
    );
}

#[test]
fn malformed_positions_spans_and_class_pairs_are_rejected_before_import() {
    let dir = TempDir::new("invalid-rule-mapping");
    let mut engine = project(&dir, "Target");
    let before = engine.project().unwrap().pcb().unwrap().clone();
    let path = dir.path().join("invalid.redarules");
    for (rule, reason) in [
        (
            json!({"kind":"clearance","layers":[1,3],"values":{"value":123000}}),
            "copper positions",
        ),
        (
            json!({"kind":"clearance","layers":[2,2],"values":{"value":123000}}),
            "copper positions",
        ),
        (
            json!({"kind":"clearance","net_class_pair":["Power","Missing"],"values":{"value":123000}}),
            "Missing",
        ),
        (
            json!({"kind":"via_span","net_class":"Power","values":{"allowed":[{"from":1,"to":2,"micro":false},{"from":2,"to":1,"micro":false}]}}),
            "allowed Via spans",
        ),
    ] {
        let file = json!({"format":2,"copper_layers":2,"severities":{},"net_classes":[{"name":"Power"}],"rules":[rule]});
        std::fs::write(&path, file.to_string()).unwrap();
        let err = engine.run_err("pcb.rules.import", json!({"path":path}));
        assert_eq!(err.code, reda_engine::command::ErrorCode::Rejected);
        assert!(err.message.contains(reason), "{err:?}");
        assert_eq!(engine.project().unwrap().pcb().unwrap(), &before);
    }
}

#[test]
fn a_released_rules_file_preserves_target_via_permissions_through_save_and_reopen() {
    let dir = TempDir::new("released-rules-import");
    let mut engine = project(&dir, "Target");
    let legacy = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/projects/formats/v1/standalone/Blinky.redarules");
    let through = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .stackup()
        .through_type()
        .clone();
    let power =
        engine.run_ok("pcb.net_class.add", json!({"name":"Power"}))["result"]["uuid"].clone();
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"via_span","scope":{"net_class":power},"values":{"allowed":[through.uuid]}}),
    );
    engine.run_ok("pcb.rules.import", json!({"path":legacy}));
    engine.run_ok("project.save", json!({}));
    engine.run_ok(
        "project.open",
        json!({"path":dir.path().join("Target/Target.reda")}),
    );
    let target = engine.project().unwrap().pcb().unwrap();
    assert_eq!(target.stackup().through_type(), &through);
    let diameter = target
        .board_rule(RuleKind::ViaSize)
        .values
        .diameter
        .unwrap();
    assert_eq!(diameter.min.unwrap().0, 510_000);
    assert_eq!(diameter.preferred, None);
    assert_eq!(
        target
            .class_rule(RuleKind::ViaSpan, uuid(&power))
            .unwrap()
            .values
            .allowed
            .as_deref(),
        Some([through.uuid].as_slice())
    );
}
