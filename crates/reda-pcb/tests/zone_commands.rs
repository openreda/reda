//! The Commands around Zones: names, shelving, pour order, Zones from the Board Outline
//! and closed loops from graphics, and the fixed values new Zones take.

use reda_command::ErrorCode;
use reda_model::{PcbDocument, PourOver, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

const MM: i64 = 1_000_000;

fn engine_with_pcb(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn add_net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

/// A rectangle (x0, y0)–(x1, y1) in mm.
fn rect(x0: i64, y0: i64, x1: i64, y1: i64) -> Value {
    let p = |x: i64, y: i64| json!({"x": x * MM, "y": y * MM});
    json!([p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)])
}

fn zone(
    engine: &mut TestEngine,
    layer: &str,
    net: Option<Uuid>,
    area: Value,
    extra: Value,
) -> Uuid {
    let mut params = json!({"layer": layer, "vertices": area});
    if let Some(net) = net {
        params["net"] = json!(net.to_string());
    }
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    uuid(&engine.run_ok("pcb.zone.add", params)["result"]["uuid"])
}

/// `pcb.zone.list`'s rows for one Layer, as (UUID, name) in pour order.
fn listed(engine: &mut TestEngine, layer: &str) -> Vec<(Uuid, String)> {
    let out = engine.run_ok("pcb.zone.list", json!({"layer": layer}));
    out["layers"][0]["zones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|z| (uuid(&z["uuid"]), z["name"].as_str().unwrap().to_owned()))
        .collect()
}

fn names(engine: &mut TestEngine, layer: &str) -> Vec<String> {
    listed(engine, layer).into_iter().map(|(_, n)| n).collect()
}

fn order(engine: &mut TestEngine, layer: &str) -> Vec<Uuid> {
    listed(engine, layer).into_iter().map(|(u, _)| u).collect()
}

fn undo(engine: &mut TestEngine) {
    let document = pcb(engine).uuid.to_string();
    engine.run_ok("edit.undo", json!({"document": document}));
}

fn redo(engine: &mut TestEngine) {
    let document = pcb(engine).uuid.to_string();
    engine.run_ok("edit.redo", json!({"document": document}));
}

fn violations(engine: &mut TestEngine, check: &str) -> Vec<Value> {
    let run = engine.run_ok("drc.run", json!({}));
    run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == check)
        .cloned()
        .collect()
}

fn state(engine: &mut TestEngine, zone: Uuid) -> String {
    let st = engine.run_ok("pcb.zone.fill_status", json!({}));
    st["zones"]
        .as_array()
        .unwrap()
        .iter()
        .find(|z| z["zone"] == zone.to_string())
        .unwrap()["state"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Two GND Pads joined only by a Zone under them, and the Zone.
fn two_pads_joined_by_a_zone(engine: &mut TestEngine) -> Uuid {
    let gnd = add_net(engine, "GND");
    for x in [2, 8] {
        engine.run_ok(
            "test.footprint.add",
            json!({"at": {"x": x * MM, "y": 5 * MM},
                   "pads": [{"at": {"x": 0, "y": 0}, "width": MM, "height": MM,
                             "net": gnd.to_string()}]}),
        );
    }
    zone(
        engine,
        "copper_top",
        Some(gnd),
        rect(0, 0, 10, 10),
        json!({}),
    )
}

fn unrouted(engine: &mut TestEngine) -> usize {
    engine.run_ok("pcb.zone.fill", json!({}));
    violations(engine, "unrouted").len()
}

#[test]
fn a_shelved_zone_is_skipped_by_fill_and_connectivity_and_drc_flags_it() {
    let dir = TempDir::new("zone-shelve");
    let mut engine = engine_with_pcb(&dir);
    let z = two_pads_joined_by_a_zone(&mut engine);
    assert_eq!(unrouted(&mut engine), 0, "the fill joins the Pads");
    assert!(violations(&mut engine, "shelved_zone").is_empty());

    let out = engine.run_ok("pcb.zone.shelve", json!({"zones": [z.to_string()]}));
    assert_eq!(out["result"]["changed"], json!([z.to_string()]));
    assert!(
        pcb(&engine).zone(z).unwrap().shelved,
        "it stays in the file"
    );
    assert_eq!(state(&mut engine, z), "shelved");
    let st = engine.run_ok("pcb.zone.fill_status", json!({}));
    assert_eq!(st["needs_refill"], false, "a shelved Zone needs no refill");
    engine.run_ok("pcb.zone.fill", json!({}));
    let fills = engine
        .project()
        .unwrap()
        .zone_fills(pcb(&engine).uuid)
        .unwrap();
    assert!(!fills.zones.contains_key(&z), "fill skips it");
    assert_eq!(unrouted(&mut engine), 1, "connectivity skips it");
    let flagged = violations(&mut engine, "shelved_zone");
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged[0]["severity"], "warning");
    assert_eq!(flagged[0]["objects"], json!([z.to_string()]));
    assert!(
        violations(&mut engine, "empty_zone").is_empty(),
        "nor checked"
    );

    // Shelving again changes nothing.
    let out = engine.run_ok("pcb.zone.shelve", json!({}));
    assert_eq!(out["result"]["changed"], json!([]));

    // Restore All brings it back; it refills like any stale Zone.
    let out = engine.run_ok("pcb.zone.restore", json!({}));
    assert_eq!(out["result"]["changed"], json!([z.to_string()]));
    assert!(!pcb(&engine).zone(z).unwrap().shelved);
    assert_eq!(state(&mut engine, z), "unfilled");
    assert_eq!(unrouted(&mut engine), 0);
    assert!(violations(&mut engine, "shelved_zone").is_empty());
}

#[test]
fn shelving_names_its_zones_and_refuses_unknown_ones() {
    let dir = TempDir::new("zone-shelve-some");
    let mut engine = engine_with_pcb(&dir);
    let a = zone(&mut engine, "copper_top", None, rect(0, 0, 5, 5), json!({}));
    let b = zone(&mut engine, "copper_top", None, rect(6, 0, 9, 5), json!({}));
    let out = engine.run_ok("pcb.zone.shelve", json!({}));
    let mut both = [a.to_string(), b.to_string()];
    both.sort();
    assert_eq!(out["result"]["changed"], json!(both));
    engine.run_ok("pcb.zone.restore", json!({"zones": [b.to_string()]}));
    assert!(pcb(&engine).zone(a).unwrap().shelved);
    assert!(!pcb(&engine).zone(b).unwrap().shelved);
    let err = engine.run_err(
        "pcb.zone.shelve",
        json!({"zones": [Uuid::from_u128(7).to_string()]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/zones/0");
    // Undo puts a shelve back in one step.
    engine.run_ok("pcb.zone.shelve", json!({"zones": [b.to_string()]}));
    undo(&mut engine);
    assert!(!pcb(&engine).zone(b).unwrap().shelved);
}

#[test]
fn pour_order_commands_rewrite_one_layers_priorities() {
    let dir = TempDir::new("zone-order");
    let mut engine = engine_with_pcb(&dir);
    let big = zone(
        &mut engine,
        "copper_top",
        None,
        rect(0, 0, 20, 20),
        json!({}),
    );
    let mid = zone(
        &mut engine,
        "copper_top",
        None,
        rect(1, 1, 11, 11),
        json!({}),
    );
    let small = zone(&mut engine, "copper_top", None, rect(2, 2, 4, 4), json!({}));
    let bottom = zone(
        &mut engine,
        "copper_bottom",
        None,
        rect(0, 0, 1, 1),
        json!({"priority": 5}),
    );
    // All at priority 0: the tie goes to the lower UUID.
    assert_eq!(order(&mut engine, "copper_top"), vec![big, mid, small]);

    let out = engine.run_ok(
        "pcb.zone.reorder",
        json!({"layer": "copper_top",
               "order": [small.to_string(), big.to_string(), mid.to_string()]}),
    );
    assert_eq!(
        out["result"]["layers"][0]["zones"],
        json!([small.to_string(), big.to_string(), mid.to_string()])
    );
    let prio = |e: &TestEngine, z: Uuid| pcb(e).zone(z).unwrap().priority;
    assert_eq!(
        (prio(&engine, small), prio(&engine, big), prio(&engine, mid)),
        (2, 1, 0)
    );
    assert_eq!(prio(&engine, bottom), 5, "other Layers keep theirs");
    // One undo step puts the whole order back.
    undo(&mut engine);
    assert_eq!(order(&mut engine, "copper_top"), vec![big, mid, small]);
    redo(&mut engine);

    engine.run_ok(
        "pcb.zone.bring_to_front",
        json!({"zones": [mid.to_string()]}),
    );
    assert_eq!(order(&mut engine, "copper_top"), vec![mid, small, big]);
    engine.run_ok("pcb.zone.send_to_back", json!({"zones": [mid.to_string()]}));
    assert_eq!(order(&mut engine, "copper_top"), vec![small, big, mid]);

    // Auto order: smallest outline first.
    engine.run_ok("pcb.zone.auto_order", json!({}));
    assert_eq!(order(&mut engine, "copper_top"), vec![small, mid, big]);
    assert_eq!(prio(&engine, bottom), 0, "every Layer, renumbered from 0");

    // The order must list every Zone on the Layer, once.
    for bad in [
        json!([small.to_string(), mid.to_string()]),
        json!([
            small.to_string(),
            mid.to_string(),
            big.to_string(),
            big.to_string()
        ]),
        json!([small.to_string(), mid.to_string(), bottom.to_string()]),
    ] {
        let err = engine.run_err(
            "pcb.zone.reorder",
            json!({"layer": "copper_top", "order": bad}),
        );
        assert_eq!(err.code, ErrorCode::InvalidParams, "{bad}");
    }
}

#[test]
fn the_pour_order_decides_which_overlapping_zone_fills_first() {
    let dir = TempDir::new("zone-order-fill");
    let mut engine = engine_with_pcb(&dir);
    let a = add_net(&mut engine, "A");
    let b = add_net(&mut engine, "B");
    let off = json!({"remove_unconnected": false});
    let za = zone(
        &mut engine,
        "copper_top",
        Some(a),
        rect(0, 0, 10, 10),
        off.clone(),
    );
    let zb = zone(&mut engine, "copper_top", Some(b), rect(5, 0, 15, 10), off);
    let filled_area = |engine: &mut TestEngine, z: Uuid| {
        engine.run_ok("pcb.zone.fill", json!({}));
        let rows = engine.run_ok("pcb.zone.list", json!({"layer": "copper_top"}));
        rows["layers"][0]["zones"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["uuid"] == z.to_string())
            .unwrap()["filled_area"]
            .as_i64()
            .unwrap()
    };
    engine.run_ok(
        "pcb.zone.bring_to_front",
        json!({"zones": [za.to_string()]}),
    );
    let first = filled_area(&mut engine, za);
    assert!(first > 99 * MM * MM, "A fills its whole square: {first}");
    engine.run_ok(
        "pcb.zone.bring_to_front",
        json!({"zones": [zb.to_string()]}),
    );
    let second = filled_area(&mut engine, za);
    assert!(second < 60 * MM * MM, "now A keeps clear of B: {second}");
}

#[test]
fn derived_names_follow_the_scheme_net_layer_and_pour_order() {
    let dir = TempDir::new("zone-names");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let first = zone(
        &mut engine,
        "copper_top",
        Some(gnd),
        rect(0, 0, 5, 5),
        json!({"priority": 2}),
    );
    let second = zone(
        &mut engine,
        "copper_top",
        Some(gnd),
        rect(6, 0, 9, 5),
        json!({}),
    );
    let none = zone(&mut engine, "copper_top", None, rect(0, 6, 5, 9), json!({}));
    let under = zone(
        &mut engine,
        "copper_bottom",
        Some(gnd),
        rect(0, 0, 5, 5),
        json!({}),
    );
    let named = |engine: &mut TestEngine, z: Uuid, layer: &str| {
        listed(engine, layer)
            .into_iter()
            .find(|(u, _)| *u == z)
            .unwrap()
            .1
    };
    // P numbers count the Zones of one Net and Layer in pour order; NONET for no Net.
    assert_eq!(named(&mut engine, first, "copper_top"), "GND_L01_P000");
    assert_eq!(named(&mut engine, second, "copper_top"), "GND_L01_P001");
    assert_eq!(named(&mut engine, none, "copper_top"), "NONET_L01_P000");
    assert_eq!(named(&mut engine, under, "copper_bottom"), "GND_L02_P000");

    // The name is derived, never stored: it follows the pour order.
    assert_eq!(pcb(&engine).zone(first).unwrap().name, None);
    engine.run_ok(
        "pcb.zone.bring_to_front",
        json!({"zones": [second.to_string()]}),
    );
    assert_eq!(named(&mut engine, second, "copper_top"), "GND_L01_P000");
    assert_eq!(named(&mut engine, first, "copper_top"), "GND_L01_P001");

    for (scheme, expected) in [
        ("LXX_NET_PXXX", "L02_GND_P000"),
        ("NET_LAYERNAME_PXXX", "GND_copper_bottom_P000"),
        ("LAYERNAME_NET_PXXX", "copper_bottom_GND_P000"),
        ("NET_LXX_PXXX", "GND_L02_P000"),
    ] {
        engine.run_ok("pcb.zone.set_name_scheme", json!({"scheme": scheme}));
        assert_eq!(
            named(&mut engine, under, "copper_bottom"),
            expected,
            "{scheme}"
        );
    }
    let err = engine.run_err("pcb.zone.set_name_scheme", json!({"scheme": "NET"}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn zone_names_are_unique_and_clearing_a_typed_name_restores_the_derived_one() {
    let dir = TempDir::new("zone-names-typed");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let a = zone(
        &mut engine,
        "copper_top",
        Some(gnd),
        rect(0, 0, 5, 5),
        json!({"name": "Shield"}),
    );
    let b = zone(
        &mut engine,
        "copper_top",
        Some(gnd),
        rect(6, 0, 9, 5),
        json!({}),
    );
    assert_eq!(
        pcb(&engine).zone(a).unwrap().name.as_deref(),
        Some("Shield")
    );
    assert_eq!(
        names(&mut engine, "copper_top"),
        vec!["Shield", "GND_L01_P001"]
    );

    // A typed name matching another Zone's name, typed or derived, is rejected.
    for taken in ["Shield", "GND_L01_P001"] {
        let err = engine.run_err(
            "pcb.zone.add",
            json!({"layer": "copper_top", "vertices": rect(0, 6, 5, 9), "name": taken}),
        );
        assert_eq!(err.code, ErrorCode::InvalidParams, "{taken}");
        assert_eq!(err.details.unwrap()["pointer"], "/name");
    }
    let err = engine.run_err(
        "pcb.zone.set",
        json!({"zone": b.to_string(), "name": "Shield"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    // A Zone may keep its own name.
    engine.run_ok(
        "pcb.zone.set",
        json!({"zone": a.to_string(), "name": "Shield"}),
    );

    engine.run_ok("pcb.zone.set", json!({"zone": a.to_string(), "name": null}));
    assert_eq!(pcb(&engine).zone(a).unwrap().name, None);
    assert_eq!(
        names(&mut engine, "copper_top"),
        vec!["GND_L01_P000", "GND_L01_P001"]
    );
}

#[test]
fn zones_from_the_board_outline_follow_it_until_detached() {
    let dir = TempDir::new("zone-outline");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "20mm", "height": "10mm"}),
    );
    let out = engine.run_ok(
        "pcb.zone.add_from_board_outline",
        json!({"net": gnd.to_string(), "layers": ["copper_top", "copper_bottom"]}),
    );
    let zones: Vec<Uuid> = out["result"]["zones"]
        .as_array()
        .unwrap()
        .iter()
        .map(uuid)
        .collect();
    assert_eq!(zones.len(), 2);
    let top = pcb(&engine).zone(zones[0]).unwrap().clone();
    assert!(top.follows_outline);
    assert!(top.vertices.is_empty(), "no vertices of its own");
    assert_eq!(top.net, Some(gnd));
    assert_eq!(top.layer, pcb(&engine).copper_layers().next().unwrap().uuid);
    // The rest is fixed, as for pcb.zone.add.
    assert_eq!(
        (top.clearance, top.min_width, top.priority),
        (None, None, 0)
    );
    assert_eq!(top.pour_over, PourOver::ZonesOnly);
    assert!(top.remove_unconnected);

    let area = |engine: &mut TestEngine| {
        let rows = engine.run_ok("pcb.zone.list", json!({"layer": "copper_top"}));
        rows["layers"][0]["zones"][0]["outline_area"]
            .as_i64()
            .unwrap()
    };
    assert_eq!(area(&mut engine), 200 * MM * MM);
    // The fill keeps the edge clearance with no inset of its own.
    engine.run_ok(
        "pcb.zone.set",
        json!({"zone": zones[0].to_string(), "remove_unconnected": false}),
    );
    engine.run_ok("pcb.zone.fill", json!({}));
    let fills = engine
        .project()
        .unwrap()
        .zone_fills(pcb(&engine).uuid)
        .unwrap();
    let outer = &fills.zones[&zones[0]].shapes[0].outer;
    let min_x = outer.iter().map(|p| p.x.0).min().unwrap();
    assert!(min_x > 0, "kept back from the edge: {min_x}");

    // It follows a later outline edit.
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "30mm", "height": "10mm"}),
    );
    assert_eq!(area(&mut engine), 300 * MM * MM);
    assert_eq!(state(&mut engine, zones[0]), "stale");

    // Detach turns it into ordinary vertices.
    engine.run_ok(
        "pcb.zone.detach_outline",
        json!({"zone": zones[0].to_string()}),
    );
    let top = pcb(&engine).zone(zones[0]).unwrap().clone();
    assert!(!top.follows_outline);
    assert_eq!(top.vertices, pcb(&engine).outline().unwrap().vertices);
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "10mm", "height": "10mm"}),
    );
    assert_eq!(area(&mut engine), 300 * MM * MM, "no longer follows");
    let err = engine.run_err(
        "pcb.zone.detach_outline",
        json!({"zone": zones[0].to_string()}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);

    engine.run_ok("pcb.board_outline.delete", json!({}));
    let err = engine.run_err(
        "pcb.zone.add_from_board_outline",
        json!({"layers": ["copper_top"]}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

/// Lines of width 0.2 mm on the mechanical Layer along `points`, closing back to the first.
fn lines(engine: &mut TestEngine, points: &[(i64, i64)]) -> Vec<String> {
    (0..points.len())
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            engine.run_ok(
                "pcb.graphic.add_line",
                json!({"layer": "mechanical", "width": 200_000,
                       "from": {"x": a.0, "y": a.1}, "to": {"x": b.0, "y": b.1}}),
            )["result"]["uuid"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[test]
fn every_polygon_object_comes_from_graphics_the_same_way() {
    let dir = TempDir::new("zone-from-graphics");
    let mut engine = engine_with_pcb(&dir);
    let square = [(0, 0), (10 * MM, 0), (10 * MM, 10 * MM), (0, 10 * MM)];
    let cases: [(&str, Value); 5] = [
        ("pcb.board_outline.add_from_graphics", json!({})),
        ("pcb.board_cutout.add_from_graphics", json!({})),
        ("pcb.zone.add_from_graphics", json!({"layer": "copper_top"})),
        (
            "pcb.zone_cutout.add_from_graphics",
            json!({"layer": "copper_bottom"}),
        ),
        ("pcb.keepout.add_from_graphics", json!({})),
    ];
    for (id, extra) in cases {
        let graphics = lines(&mut engine, &square);
        let mut params = extra.clone();
        params["graphics"] = json!(graphics);
        let out = engine.run_ok(id, params);
        let made = uuid(&out["result"]["uuid"]);
        let pcb = pcb(&engine);
        let vertices = if let Some(z) = pcb.zone(made) {
            z.vertices.clone()
        } else if let Some(c) = pcb.zone_cutout(made) {
            c.vertices.clone()
        } else if let Some(k) = pcb.keepout(made) {
            k.vertices.clone()
        } else if let Some(c) = pcb.cutout(made) {
            c.vertices.clone()
        } else {
            pcb.outline().unwrap().vertices.clone()
        };
        let corners: Vec<(i64, i64)> = vertices.iter().map(|v| (v.x.0, v.y.0)).collect();
        assert_eq!(corners, square.to_vec(), "{id}");
        // The graphics are removed unless `keep`.
        assert!(
            graphics
                .iter()
                .all(|g| pcb.graphic(uuid(&json!(g))).is_none()),
            "{id}"
        );

        let graphics = lines(&mut engine, &square);
        let mut params = extra;
        params["graphics"] = json!(graphics);
        params["keep"] = json!(true);
        engine.run_ok(id, params);
        let pcb = self::pcb(&engine);
        assert!(
            graphics
                .iter()
                .all(|g| pcb.graphic(uuid(&json!(g))).is_some()),
            "{id}"
        );
    }
}

#[test]
fn ends_within_a_micrometre_meet_and_a_larger_gap_is_refused_with_its_place() {
    let dir = TempDir::new("zone-graphics-gap");
    let mut engine = engine_with_pcb(&dir);
    // The last corner misses the first by 1 µm: still a loop.
    let mut graphics = lines(&mut engine, &[(0, 0), (10 * MM, 0), (10 * MM, 10 * MM)]);
    graphics.pop();
    graphics.push(
        engine.run_ok(
            "pcb.graphic.add_line",
            json!({"layer": "mechanical", "width": 200_000,
                   "from": {"x": 10 * MM, "y": 10 * MM}, "to": {"x": 1_000, "y": 0}}),
        )["result"]["uuid"]
            .as_str()
            .unwrap()
            .to_owned(),
    );
    engine.run_ok(
        "pcb.zone.add_from_graphics",
        json!({"layer": "copper_top", "graphics": graphics, "keep": true}),
    );

    // 50 µm short: refused, with the gap's place and size, suggesting outer_edge.
    let open = [(0, 0), (10 * MM, 0), (10 * MM, 10 * MM)];
    let mut graphics = lines(&mut engine, &open);
    graphics.pop();
    graphics.push(
        engine.run_ok(
            "pcb.graphic.add_line",
            json!({"layer": "mechanical", "width": 200_000,
                   "from": {"x": 10 * MM, "y": 10 * MM}, "to": {"x": 50_000, "y": 50_000}}),
        )["result"]["uuid"]
            .as_str()
            .unwrap()
            .to_owned(),
    );
    for id in [
        "pcb.board_outline.add_from_graphics",
        "pcb.board_cutout.add_from_graphics",
        "pcb.keepout.add_from_graphics",
    ] {
        let err = engine.run_err(id, json!({"graphics": graphics}));
        assert_eq!(err.code, ErrorCode::Rejected, "{id}");
        let details = err.details.unwrap();
        assert_eq!(
            details["gap"]["at"],
            json!({"x": 25_000, "y": 25_000}),
            "{id}"
        );
        assert_eq!(details["gap"]["size"], 70_711, "{id}");
        assert_eq!(details["retry"]["mode"], "outer_edge");
        assert!(err.message.contains("0.070711 mm"), "{}", err.message);
    }
    assert!(pcb(&engine).graphics().count() >= 4, "nothing removed");

    // The outer_edge retry closes it along the outside of the 0.2 mm strokes.
    let out = engine.run_ok(
        "pcb.zone.add_from_graphics",
        json!({"layer": "copper_top", "graphics": graphics, "mode": "outer_edge"}),
    );
    let z = pcb(&engine).zone(uuid(&out["result"]["uuid"])).unwrap();
    let xs: Vec<i64> = z.vertices.iter().map(|v| v.x.0).collect();
    let ys: Vec<i64> = z.vertices.iter().map(|v| v.y.0).collect();
    let (lo, hi) = (*xs.iter().min().unwrap(), *xs.iter().max().unwrap());
    assert!((-102_000..=-100_000).contains(&lo), "{lo}");
    assert!(
        (10 * MM + 100_000..=10 * MM + 102_000).contains(&hi),
        "{hi}"
    );
    assert!(*ys.iter().min().unwrap() <= -100_000);
    assert!(
        graphics
            .iter()
            .all(|g| pcb(&engine).graphic(uuid(&json!(g))).is_none())
    );

    // Strokes that enclose nothing are refused in outer_edge too.
    let line = lines(&mut engine, &[(0, 20 * MM), (5 * MM, 20 * MM)]);
    let err = engine.run_err(
        "pcb.zone.add_from_graphics",
        json!({"layer": "copper_top", "graphics": [line[0]], "mode": "outer_edge"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn a_zone_cutout_removes_every_zone_fill_on_its_layer_inside_it() {
    let dir = TempDir::new("zone-cutout");
    let mut engine = engine_with_pcb(&dir);
    let off = json!({"remove_unconnected": false});
    let top = zone(
        &mut engine,
        "copper_top",
        None,
        rect(0, 0, 10, 10),
        off.clone(),
    );
    let bottom = zone(&mut engine, "copper_bottom", None, rect(0, 0, 10, 10), off);
    let out = engine.run_ok(
        "pcb.zone_cutout.add",
        json!({"layer": "copper_top", "vertices": rect(0, 0, 5, 10)}),
    );
    let cutout = uuid(&out["result"]["uuid"]);
    engine.run_ok("pcb.zone.fill", json!({}));
    let rows = engine.run_ok("pcb.zone.list", json!({}));
    let filled = |layer: usize| {
        rows["layers"][layer]["zones"][0]["filled_area"]
            .as_i64()
            .unwrap()
    };
    // Exactly to its edge: the top keeps the other half, less nothing.
    let half = 50 * MM * MM;
    assert!((filled(0) - half).abs() < MM * MM / 100, "{}", filled(0));
    assert!(filled(1) > 99 * MM * MM, "other Layers keep their fill");
    let _ = (top, bottom);
    engine.run_ok(
        "pcb.zone_cutout.delete",
        json!({"cutouts": [cutout.to_string()]}),
    );
    assert_eq!(state(&mut engine, top), "stale");
    assert_eq!(state(&mut engine, bottom), "filled");
}

#[test]
fn new_zones_take_fixed_values_never_preferences() {
    let dir = TempDir::new("zone-fixed");
    let mut engine = engine_with_pcb(&dir);
    let graphics = lines(&mut engine, &[(0, 0), (MM, 0), (MM, MM)]);
    let made = [
        zone(&mut engine, "copper_top", None, rect(0, 0, 5, 5), json!({})),
        uuid(
            &engine.run_ok(
                "pcb.zone.add_from_graphics",
                json!({"layer": "copper_top", "graphics": graphics}),
            )["result"]["uuid"],
        ),
    ];
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "5mm", "height": "5mm"}),
    );
    let from_outline = engine.run_ok(
        "pcb.zone.add_from_board_outline",
        json!({"layers": ["copper_top"]}),
    );
    let made: Vec<Uuid> = made
        .into_iter()
        .chain([uuid(&from_outline["result"]["zones"][0])])
        .collect();
    for z in made {
        let z = pcb(&engine).zone(z).unwrap();
        assert_eq!(z.name, None);
        assert_eq!(z.clearance, None);
        assert_eq!(z.min_width, None);
        assert_eq!(z.pour_over, PourOver::ZonesOnly);
        assert!(z.remove_unconnected);
        assert_eq!(z.min_island_area, 0);
        assert_eq!(z.priority, 0);
        assert!(!z.shelved);
    }
}
