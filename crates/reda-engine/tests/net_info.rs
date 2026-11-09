//! `pcb.net.info`: the Net block of Track Properties, with the IPC-2221 current estimate at
//! the `pcb.net_info.delta_t` Preference.

use std::path::Path;

use reda_command::{ErrorCode, SeededIds, UserDirs};
use reda_engine::Engine;
use reda_model::Uuid;
use reda_testkit::{TempDir, TestEngine, probe_commands};
use serde_json::{Value, json};

fn engine(home: &Path, work: &Path) -> TestEngine {
    let mut engine = TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .user_dirs(Some(UserDirs::under(home)))
            .starter_library(None)
            .commands(probe_commands)
            .build(),
    );
    engine.run_ok("project.new", json!({"path": work, "name": "Demo"}));
    engine
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn pad(engine: &mut TestEngine, x: &str, net: Uuid) {
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": x, "y": 0},
               "pads": [{"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm",
                         "net": net}]}),
    );
}

fn close(actual: &Value, expected: f64, tolerance: f64) {
    let actual = actual
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {actual}"));
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} is not within {tolerance} of {expected}"
    );
}

#[test]
fn net_info_gives_routed_figures_resistance_and_the_current_estimate_at_the_preference() {
    let home = TempDir::new("net-info-home");
    let work = TempDir::new("net-info-work");
    let mut engine = engine(home.path(), work.path());
    let net = uuid(&engine.run_ok("test.net.add", json!({"name": "SIG"}))["result"]["uuid"]);
    pad(&mut engine, "0mm", net);
    pad(&mut engine, "10mm", net);
    pad(&mut engine, "20mm", net);
    engine.run_ok(
        "pcb.net.set_color",
        json!({"nets": [net], "color": "#00ff00"}),
    );
    // 10 mil (0.254 mm) wide, 10 mm long, on 1 oz (35 µm) outer copper.
    let track = uuid(
        &engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_top", "net": net, "width": "0.254mm",
                   "points": [{"x": 0, "y": 0}, {"x": "10mm", "y": 0}]}),
        )["result"]["tracks"][0],
    );

    let info = engine.run_ok("pcb.net.info", json!({"net": net, "track": track}));
    assert_eq!(info["name"], "SIG");
    assert_eq!(info["color"], "#00ff00");
    assert_eq!(info["loop_removal"], true);
    assert_eq!(info["routed_length"], json!(10_000_000));
    assert_eq!(info["connections"], json!({"total": 2, "made": 1}));
    close(&info["routed_percent"], 50.0, 1e-9);
    assert_eq!(info["delta_t"], json!(10.0), "the Preference's default");
    assert_eq!(info["width"], json!(254_000));
    assert_eq!(info["thickness"], json!(35_000));
    assert_eq!(info["outer"], true);
    // ρL/A = 1.72e-8 Ω·m / (0.254 mm × 35 µm) = 1.9348 Ω/m.
    close(&info["resistance_per_mm"], 1.9348e-3, 1e-6);
    close(&info["track_resistance"], 1.9348e-2, 1e-5);
    // IPC-2221 for 10 mil of 1 oz outer copper at ΔT 10 °C: about 0.886 A.
    close(&info["max_current_estimate"], 0.886, 0.002);

    engine.run_ok(
        "app.preferences.set",
        json!({"key": "pcb.net_info.delta_t", "value": 20.0}),
    );
    let info = engine.run_ok("pcb.net.info", json!({"net": net, "width": "0.254mm"}));
    assert_eq!(info["delta_t"], json!(20.0));
    close(
        &info["max_current_estimate"],
        0.886 * 2f64.powf(0.44),
        0.003,
    );
    assert!(info.get("track_resistance").is_none(), "{info}");

    let info = engine.run_ok("pcb.net.info", json!({"net": net, "delta_t": 10.0}));
    assert_eq!(info["width"], json!(250_000), "the Net Class's track width");
    assert_eq!(info["delta_t"], json!(10.0));

    let err = engine.run_err("pcb.net.info", json!({"net": net, "delta_t": 0}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = engine.run_err("pcb.net.info", json!({"net": Uuid::from_u128(4)}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn the_current_estimate_halves_k_on_inner_copper() {
    let w = reda_model::geom::Nm(254_000);
    let t = reda_model::geom::Nm(35_000);
    let outer = reda_pcb::net_info::max_current(w, t, true, 10.0);
    let inner = reda_pcb::net_info::max_current(w, t, false, 10.0);
    assert!((inner * 2.0 - outer).abs() < 1e-12);
}
