//! Measuring and board figures, through Commands: `pcb.measure.clearance`,
//! `pcb.measure.length` and `pcb.board.info`.

use reda_command::ErrorCode;
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

fn uuids(value: &Value) -> Vec<Uuid> {
    value.as_array().unwrap().iter().map(uuid).collect()
}

fn add_net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

fn route(engine: &mut TestEngine, layer: &str, points: &[(f64, f64)]) -> Vec<Uuid> {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    uuids(
        &engine.run_ok("pcb.track.add", json!({"layer": layer, "points": points}))["result"]["tracks"],
    )
}

/// A Footprint at `(x, y)` mm with one 1 mm square SMD Pad per `nets` entry, 5 mm apart
/// along X; returns (footprint, pads).
fn footprint(engine: &mut TestEngine, x: f64, y: f64, nets: &[Option<Uuid>]) -> (Uuid, Vec<Uuid>) {
    let pads: Vec<Value> = nets
        .iter()
        .enumerate()
        .map(|(i, net)| {
            let mut pad = json!({"at": {"x": format!("{}mm", 5 * i), "y": 0},
                                 "width": "1mm", "height": "1mm"});
            if let Some(net) = net {
                pad["net"] = json!(net);
            }
            pad
        })
        .collect();
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": format!("{x}mm"), "y": format!("{y}mm")}, "pads": pads}),
    )["result"]
        .clone();
    (uuid(&out["footprint"]), uuids(&out["pads"]))
}

fn clearance(engine: &mut TestEngine, a: Value, b: Value) -> Value {
    engine.run_ok("pcb.measure.clearance", json!({"a": a, "b": b}))
}

fn layer(engine: &TestEngine, side: reda_model::Side) -> Uuid {
    let pcb = engine.project().unwrap().pcb().unwrap();
    pcb.copper_layers()
        .find(|l| l.side == Some(side))
        .unwrap()
        .uuid
}

#[test]
fn clearance_gives_the_smallest_gap_with_its_layers_and_where_it_falls() {
    let dir = TempDir::new("measure-clearance");
    let mut engine = engine(&dir);
    let top = layer(&engine, reda_model::Side::Top);
    let bottom = layer(&engine, reda_model::Side::Bottom);
    // 0.25 mm wide Tracks, centrelines 2 mm apart: 1.75 mm of gap.
    let a = route(&mut engine, "copper_top", &[(0.0, 0.0), (10.0, 0.0)])[0];
    let b = route(&mut engine, "copper_top", &[(12.0, 2.0), (20.0, 2.0)])[0];
    let out = clearance(&mut engine, json!(a), json!(b));
    // The closest points are the ends (10, 0) and (12, 2): √8 mm less two half widths.
    let expected = (8.0f64.sqrt() * 1e6 - 250_000.0).round() as i64;
    assert_eq!(out["xy"], json!(expected), "{out}");
    assert_eq!(out["same_layer"]["distance"], json!(expected));
    assert_eq!(out["same_layer"]["layer"], json!(top));
    assert_eq!(out["a"]["layer"], json!(top));
    assert_eq!(out["b"]["layer"], json!(top));
    let half = (125_000.0 / 2.0f64.sqrt()).round() as i64;
    assert_eq!(out["a"]["at"], json!({"x": 10_000_000 + half, "y": half}));
    assert_eq!(
        out["b"]["at"],
        json!({"x": 12_000_000 - half, "y": 2_000_000 - half})
    );

    // On another Layer: no same-Layer gap, but the flattened one.
    let c = route(&mut engine, "copper_bottom", &[(0.0, 3.0), (10.0, 3.0)])[0];
    let out = clearance(&mut engine, json!(a), json!(c));
    assert_eq!(out["xy"], json!(2_750_000), "{out}");
    assert!(out.get("same_layer").is_none(), "{out}");
    assert_eq!(out["b"]["layer"], json!(bottom));

    // Overlapping copper measures 0.
    let d = route(&mut engine, "copper_top", &[(5.0, -1.0), (5.0, 1.0)])[0];
    let out = clearance(&mut engine, json!(a), json!(d));
    assert_eq!(out["xy"], json!(0));
    assert_eq!(out["same_layer"]["distance"], json!(0));
}

#[test]
fn clearance_measures_pads_vias_graphics_and_the_board_edge_but_not_groups() {
    let dir = TempDir::new("measure-kinds");
    let mut engine = engine(&dir);
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "50mm", "height": "40mm"}),
    );
    let outline = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .outline()
        .unwrap()
        .uuid;
    let (fp, pads) = footprint(&mut engine, 10.0, 10.0, &[None]);
    let track = route(&mut engine, "copper_top", &[(13.0, 10.0), (20.0, 10.0)])[0];
    let pad = json!({"footprint": fp, "pad": pads[0]});
    // Pad edge at x = 10.5, Track edge at x = 12.875.
    assert_eq!(
        clearance(&mut engine, pad.clone(), json!(track))["xy"],
        json!(2_375_000)
    );
    // The Board Outline's bottom edge is y = 0; the Pad's edge is at y = 9.5.
    let out = clearance(&mut engine, pad.clone(), json!(outline));
    assert_eq!(out["xy"], json!(9_500_000), "{out}");
    assert!(
        out.get("same_layer").is_none(),
        "the outline is on its own Layer"
    );

    let via = uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": "30mm", "y": "10mm"}, "diameter": "1mm", "hole": "0.5mm"}),
        )["result"]["uuid"],
    );
    // The Track ends at x = 20 + 0.125; the Via's copper starts at 29.5.
    assert_eq!(
        clearance(&mut engine, json!(track), json!(via))["xy"],
        json!(9_375_000)
    );
    let line = uuid(
        &engine.run_ok(
            "pcb.graphic.add_line",
            json!({"layer": "silkscreen_top", "width": "0.2mm",
                   "from": {"x": "30mm", "y": "20mm"}, "to": {"x": "40mm", "y": "20mm"}}),
        )["result"]["uuid"],
    );
    // Via edge at y = 10.5, line edge at y = 19.9.
    assert_eq!(
        clearance(&mut engine, json!(via), json!(line))["xy"],
        json!(9_400_000)
    );

    let dimension = uuid(
        &engine.run_ok(
            "pcb.dimension.add",
            json!({"layer": "mechanical", "a": {"point": {"x": 0, "y": "30mm"}},
                   "b": {"point": {"x": "10mm", "y": "30mm"}}}),
        )["result"]["uuid"],
    );
    for (group, what) in [(json!(fp), "Footprint"), (json!(dimension), "Dimension")] {
        let err = engine.run_err("pcb.measure.clearance", json!({"a": group, "b": track}));
        assert_eq!(err.code, ErrorCode::InvalidParams);
        assert!(err.message.contains(what), "{}", err.message);
    }
    let err = engine.run_err(
        "pcb.measure.clearance",
        json!({"a": track, "b": Uuid::from_u128(5)}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.as_ref().unwrap()["pointer"], "/b");
}

#[test]
fn length_totals_tracks_and_leaves_out_everything_else() {
    let dir = TempDir::new("measure-length");
    let mut engine = engine(&dir);
    let route = route(
        &mut engine,
        "copper_top",
        &[(0.0, 0.0), (3.0, 4.0), (3.0, 10.0)],
    );
    let via = uuid(
        &engine.run_ok("pcb.via.add", json!({"at": {"x": "3mm", "y": "10mm"}}))["result"]["uuid"],
    );
    let (fp, _) = footprint(&mut engine, 20.0, 0.0, &[None]);
    let out = engine.run_ok(
        "pcb.measure.length",
        json!({"objects": [route[0], route[1], via, fp]}),
    );
    assert_eq!(out["length"], json!(11_000_000));
    assert_eq!(out["tracks"], json!(2));
    let err = engine.run_err(
        "pcb.measure.length",
        json!({"objects": [Uuid::from_u128(3)]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn board_info_gives_size_area_density_routed_share_and_counts() {
    let dir = TempDir::new("board-info");
    let mut engine = engine(&dir);
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "50mm", "height": "40mm"}),
    );
    engine.run_ok(
        "pcb.board_cutout.add",
        json!({"vertices": [{"x": "40mm", "y": "30mm"}, {"x": "45mm", "y": "30mm"},
                            {"x": "45mm", "y": "35mm"}, {"x": "40mm", "y": "35mm"}]}),
    );
    let gnd = add_net(&mut engine, "GND");
    let vcc = add_net(&mut engine, "VCC");
    // Two Footprints of two 1 mm Pads 5 mm apart: each takes a 6 mm × 1 mm box.
    footprint(&mut engine, 5.0, 5.0, &[Some(gnd), Some(vcc)]);
    footprint(&mut engine, 5.0, 15.0, &[Some(gnd), Some(vcc)]);

    let info = engine.run_ok("pcb.board.info", json!({}));
    assert_eq!(info["width"], json!(50_000_000));
    assert_eq!(info["height"], json!(40_000_000));
    assert_eq!(
        info["area"],
        json!(1975.0),
        "the cutout is left out: {info}"
    );
    assert_eq!(info["component_area"], json!(12.0));
    assert_eq!(info["density"], json!(12.0 / (2.0 * 1975.0)));
    assert_eq!(info["routed_percent"], json!(0.0));
    assert_eq!(info["components"], json!(2));
    assert_eq!(info["layers"], json!(2));
    assert_eq!(info["nets"], json!(2));
    assert_eq!(info["primitives"], json!(4), "four Pads");

    // GND routed: one of the two connections made.
    route(&mut engine, "copper_top", &[(5.0, 5.0), (5.0, 15.0)]);
    let info = engine.run_ok("pcb.board.info", json!({}));
    assert_eq!(info["routed_percent"], json!(50.0));
    assert_eq!(info["primitives"], json!(5));
}

#[test]
fn board_info_without_an_outline_has_no_size() {
    let dir = TempDir::new("board-info-empty");
    let mut engine = engine(&dir);
    let info = engine.run_ok("pcb.board.info", json!({}));
    assert!(info.get("width").is_none(), "{info}");
    assert_eq!(info["area"], json!(0.0));
    assert!(info.get("density").is_none(), "{info}");
    assert_eq!(info["routed_percent"], json!(100.0), "nothing to route");
}
