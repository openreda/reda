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
    assert_eq!(file["format"], 1);
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
