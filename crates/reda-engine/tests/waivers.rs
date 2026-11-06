//! Waivers through Commands and saved Project files.
use reda_engine::command::{FixedClock, SeededIds, UserDirs};
use reda_testkit::{TempDir, TestEngine, probe_commands};
use serde_json::{Value, json};

const PCB: &str = "00000000-0000-4000-8000-000000000003";
fn engine(home: &std::path::Path) -> TestEngine {
    TestEngine(
        reda_engine::Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .user_dirs(Some(UserDirs::under(home)))
            .starter_library(None)
            .commands(probe_commands)
            .build(),
    )
}
fn width_violations(e: &mut TestEngine) -> Vec<Value> {
    e.run_ok("drc.run", json!({}))["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == "track_width")
        .cloned()
        .collect()
}
fn two_narrow_tracks(e: &mut TestEngine) {
    for y in [0, 10_000_000] {
        e.run_ok("pcb.track.add", json!({"layer": "copper_top", "width": "0.1mm", "points": [{"x":0,"y":y},{"x":"5mm","y":y}]}));
    }
}
#[test]
fn batch_waivers_share_reason_author_and_clock_and_undo_together_after_roundtrip() {
    let dir = TempDir::new("waiver-metadata");
    let mut e = engine(dir.path());
    e.run_ok("project.new", json!({"path": dir.path(), "name":"Demo"}));
    two_narrow_tracks(&mut e);
    e.run_ok(
        "app.preferences.set",
        json!({"key":"user.name","value":"Ada"}),
    );
    let violations = width_violations(&mut e);
    let targets: Vec<Value> = violations
        .iter()
        .map(|v| json!({"check":v["check"],"objects":v["objects"]}))
        .collect();
    let waived = e.run_ok(
        "drc.waive",
        json!({"violations":targets,"reason":"Test coupon"}),
    );
    let waivers = waived["result"]["waivers"].as_array().unwrap();
    assert_eq!(waivers.len(), 2);
    for (id, v) in waivers.iter().zip(&violations) {
        let w = e.run_ok("object.get", json!({"uuid":id}))["object"].clone();
        assert_eq!(w["author"], "Ada");
        assert_eq!(w["created"], 1_767_225_600_000_i64);
        assert_eq!(w["reason"], "Test coupon");
        assert_eq!(w["objects"], v["objects"]);
        assert!(w.get("comment").is_none());
    }
    assert!(width_violations(&mut e).is_empty());
    e.run_ok("edit.undo", json!({"document":PCB}));
    assert_eq!(width_violations(&mut e).len(), 2);
    e.run_ok("edit.redo", json!({"document":PCB}));
    assert!(width_violations(&mut e).is_empty());
    e.run_ok("project.save", json!({}));
    let mut reopened = engine(dir.path());
    reopened.run_ok("project.open", json!({"path":dir.path().join("Demo")}));
    for id in waivers {
        let w = reopened.run_ok("object.get", json!({"uuid":id}))["object"].clone();
        assert_eq!(w["author"], "Ada");
        assert_eq!(w["reason"], "Test coupon");
        assert_eq!(w["created"], 1_767_225_600_000_i64);
    }
    assert!(width_violations(&mut reopened).is_empty());
}

#[test]
fn off_checks_do_not_report_stale_waivers_and_full_runs_never_delete_them() {
    let dir = TempDir::new("waiver-stale-off");
    let mut e = engine(dir.path());
    e.run_ok("project.new", json!({"path":dir.path(),"name":"Demo"}));
    two_narrow_tracks(&mut e);
    let violations = width_violations(&mut e);
    let w = e.run_ok(
        "drc.waive",
        json!({"violations":[{"check":"track_width","objects":violations[0]["objects"]}]}),
    )["result"]["waivers"][0]
        .clone();
    e.run_ok(
        "pcb.track.delete",
        json!({"tracks":violations[0]["objects"]}),
    );
    assert_eq!(
        e.run_ok("drc.run", json!({}))["unmatched_waivers"],
        json!([w])
    );
    assert_eq!(
        e.run_ok("object.get", json!({"uuid":w}))["object"]["check"],
        "track_width"
    );
    e.run_ok(
        "drc.set_severity",
        json!({"check":"track_width","severity":"off"}),
    );
    assert_eq!(
        e.run_ok("drc.run", json!({}))["unmatched_waivers"],
        json!([])
    );
    assert_eq!(
        e.run_ok("object.get", json!({"uuid":w}))["object"]["check"],
        "track_width"
    );
    e.run_ok("edit.undo", json!({"document":PCB}));
    assert_eq!(
        e.run_ok("drc.run", json!({}))["unmatched_waivers"],
        json!([w])
    );
    e.run_ok("edit.undo", json!({"document":PCB}));
    let run = e.run_ok("drc.run", json!({}));
    assert_eq!(run["waived"], 1);
    assert_eq!(run["unmatched_waivers"], json!([]));
}

#[test]
fn a_blank_author_preference_falls_back_to_the_os_user_name() {
    let dir = TempDir::new("waiver-os-author");
    let mut e = engine(dir.path());
    e.run_ok("project.new", json!({"path":dir.path(),"name":"Demo"}));
    let default = e.run_ok("app.preferences.get", json!({"key":"user.name"}))["default"].clone();
    e.run_ok(
        "app.preferences.set",
        json!({"key":"user.name","value":"   "}),
    );
    two_narrow_tracks(&mut e);
    let targets = width_violations(&mut e);
    let invalid = e.run_err("drc.waive",json!({"violations":[{"check":"track_width","objects":targets[0]["objects"]},{"check":"short_circuit","objects":[]}]}));
    assert_eq!(invalid.code, reda_engine::command::ErrorCode::Rejected);
    assert_eq!(width_violations(&mut e).len(), 2);
    let id = e.run_ok(
        "drc.waive",
        json!({"violations":[{"check":"track_width","objects":targets[0]["objects"]}]}),
    )["result"]["waivers"][0]
        .clone();
    let w = e.run_ok("object.get", json!({"uuid":id}))["object"].clone();
    assert_eq!(w["author"], default);
    assert!(
        w.get("reason").is_none(),
        "an omitted reason stays optional"
    );
}
