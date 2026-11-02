//! Net Classes and assigning Nets to them, through Commands.

use reda_command::ErrorCode;
use reda_model::geom::Nm;
use reda_model::{PcbDocument, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine_with_pcb(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn uuid(out: &Value) -> Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

fn add_class(engine: &mut TestEngine, params: Value) -> Uuid {
    uuid(&engine.run_ok("pcb.net_class.add", params))
}

fn add_net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name})))
}

#[test]
fn a_net_class_is_added_with_overrides() {
    let dir = TempDir::new("class-add");
    let mut engine = engine_with_pcb(&dir);
    let power = add_class(
        &mut engine,
        json!({"name": "Power", "track_width": "0.5mm", "via_diameter": "0.8mm", "via_hole": "0.4mm"}),
    );
    let class = pcb(&engine).net_class(power).unwrap();
    assert_eq!(class.name, "Power");
    assert_eq!(class.track_width, Some(Nm::from_um(500)));
    assert_eq!(class.via_diameter, Some(Nm::from_um(800)));
    assert_eq!(class.via_hole, Some(Nm::from_um(400)));
    assert_eq!(class.clearance, None, "falls back to the board-wide rule");
}

#[test]
fn net_class_names_are_unique_ignoring_case() {
    let dir = TempDir::new("class-unique");
    let mut engine = engine_with_pcb(&dir);
    add_class(&mut engine, json!({"name": "Power"}));
    for name in ["power", "DEFAULT", "  "] {
        let err = engine.run_err("pcb.net_class.add", json!({"name": name}));
        assert_eq!(err.code, ErrorCode::InvalidParams, "{name:?}");
        assert_eq!(err.details.as_ref().unwrap()["pointer"], "/name");
    }
}

#[test]
fn a_net_class_can_be_renamed_and_its_overrides_replaced() {
    let dir = TempDir::new("class-rename");
    let mut engine = engine_with_pcb(&dir);
    let class = add_class(&mut engine, json!({"name": "Pwr", "clearance": "0.3mm"}));
    engine.run_ok(
        "pcb.net_class.rename",
        json!({"class": class, "name": "Power"}),
    );
    // Renaming to its own name in another case is fine.
    engine.run_ok(
        "pcb.net_class.rename",
        json!({"class": class, "name": "POWER"}),
    );
    engine.run_ok(
        "pcb.net_class.set_overrides",
        json!({"class": class, "track_width": "1mm"}),
    );
    let c = pcb(&engine).net_class(class).unwrap();
    assert_eq!(c.name, "POWER");
    assert_eq!(c.track_width, Some(Nm::from_mm(1)));
    assert_eq!(c.clearance, None, "set_overrides replaces all four");
    let err = engine.run_err(
        "pcb.net_class.rename",
        json!({"class": class, "name": "Default"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn nets_are_assigned_to_a_class() {
    let dir = TempDir::new("net-set-class");
    let mut engine = engine_with_pcb(&dir);
    let power = add_class(&mut engine, json!({"name": "Power"}));
    let vcc = add_net(&mut engine, "VCC");
    let gnd = add_net(&mut engine, "GND");
    let sig = add_net(&mut engine, "SIG");
    let default = pcb(&engine).setup().default_net_class;
    assert_eq!(pcb(&engine).net(vcc).unwrap().class, default);

    engine.run_ok(
        "pcb.net.set_class",
        json!({"nets": [vcc, gnd], "class": power}),
    );
    let pcb = pcb(&engine);
    assert_eq!(pcb.net(vcc).unwrap().class, power);
    assert_eq!(pcb.net(gnd).unwrap().class, power);
    assert_eq!(pcb.net(sig).unwrap().class, default);
}

#[test]
fn assigning_an_unknown_net_or_class_changes_nothing() {
    let dir = TempDir::new("net-set-class-unknown");
    let mut engine = engine_with_pcb(&dir);
    let power = add_class(&mut engine, json!({"name": "Power"}));
    let vcc = add_net(&mut engine, "VCC");
    let nowhere = "00000000-0000-4000-8000-0000000000ff";
    let err = engine.run_err(
        "pcb.net.set_class",
        json!({"nets": [vcc, nowhere], "class": power}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = engine.run_err(
        "pcb.net.set_class",
        json!({"nets": [vcc], "class": nowhere}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    let default = pcb(&engine).setup().default_net_class;
    assert_eq!(pcb(&engine).net(vcc).unwrap().class, default);
}

#[test]
fn deleting_a_class_moves_its_nets_to_the_default_class() {
    let dir = TempDir::new("class-delete");
    let mut engine = engine_with_pcb(&dir);
    let power = add_class(&mut engine, json!({"name": "Power"}));
    let vcc = add_net(&mut engine, "VCC");
    engine.run_ok("pcb.net.set_class", json!({"nets": [vcc], "class": power}));
    engine.run_ok("pcb.net_class.delete", json!({"class": power}));
    let pcb = pcb(&engine);
    assert!(pcb.net_class(power).is_none());
    assert_eq!(pcb.net(vcc).unwrap().class, pcb.setup().default_net_class);
}

#[test]
fn the_default_class_cannot_be_deleted() {
    let dir = TempDir::new("class-delete-default");
    let mut engine = engine_with_pcb(&dir);
    let default = pcb(&engine).setup().default_net_class;
    let err = engine.run_err("pcb.net_class.delete", json!({"class": default}));
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn deleting_a_class_undoes_with_its_nets() {
    let dir = TempDir::new("class-delete-undo");
    let mut engine = engine_with_pcb(&dir);
    let power = add_class(&mut engine, json!({"name": "Power"}));
    let vcc = add_net(&mut engine, "VCC");
    engine.run_ok("pcb.net.set_class", json!({"nets": [vcc], "class": power}));
    let before = engine.project().unwrap().clone();
    engine.run_ok("pcb.net_class.delete", json!({"class": power}));
    let document = pcb(&engine).uuid;
    engine.run_ok("edit.undo", json!({"document": document}));
    assert_eq!(engine.project().unwrap(), &before);
}

#[test]
fn a_net_class_is_added_with_its_nets_in_one_step() {
    let dir = TempDir::new("class-add-nets");
    let mut engine = engine_with_pcb(&dir);
    let (vcc, gnd) = (add_net(&mut engine, "VCC"), add_net(&mut engine, "GND"));
    let power = add_class(&mut engine, json!({"name": "Power", "nets": [vcc]}));
    assert_eq!(pcb(&engine).net(vcc).unwrap().class, power);
    let default = pcb(&engine).setup().default_net_class;
    assert_eq!(pcb(&engine).net(gnd).unwrap().class, default);

    let err = engine.run_err(
        "pcb.net_class.add",
        json!({"name": "Other", "nets": [gnd, power]}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(pcb(&engine).net_class_named("Other").is_none(), "nothing added");
}
