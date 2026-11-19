//! Width changes as seen through the fill commands and the copper index.
use reda_model::Uuid;
use reda_model::geom::{Nm, Point};
use reda_pcb::{Copper, copper::Item};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

fn track(engine: &mut TestEngine, net: Uuid, width: i64, pts: &[(i64, i64)]) -> Vec<Uuid> {
    let points: Vec<_> = pts
        .iter()
        .map(|(x, y)| json!({"x":x+10_000_000, "y":y+10_000_000}))
        .collect();
    let v = engine.run_ok(
        "pcb.track.add",
        json!({"layer":"copper_top", "net":net, "width":width, "points":points}),
    );
    v["result"]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(uuid)
        .collect()
}

fn board(dir: &TempDir) -> (TestEngine, Uuid) {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path":dir.path(),"name":"Board"}));
    let net = uuid(&engine.run_ok("test.net.add", json!({"name":"SIG"}))["result"]["uuid"]);
    (engine, net)
}

fn enable(engine: &mut TestEngine, shape: Value) {
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"teardrop","values":{"width_change":shape}}),
    );
}

#[test]
fn width_change_makes_copper_belonging_to_the_narrow_track() {
    let dir = TempDir::new("teardrop-copper");
    let (mut engine, net) = board(&dir);
    track(&mut engine, net, 1_000_000, &[(-2_000_000, 0), (0, 0)]);
    let narrow = track(&mut engine, net, 100_000, &[(0, 0), (3_000_000, 0)])[0];
    // At 0.75 mm beyond the junction the wide Track's round cap no longer reaches.
    let point = Point::new(Nm(10_750_000), Nm(10_120_000));
    let pick = |engine: &TestEngine| {
        let project = engine.project().unwrap();
        let layer = project.pcb().unwrap().track(narrow).unwrap().layer;
        Copper::of(project).unwrap().items_at(layer, point)
    };
    assert!(pick(&engine).is_empty());
    enable(&mut engine, json!({"enabled":true,"length":100}));
    engine.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(pick(&engine), [Item::Track { uuid: narrow }]);
    assert_eq!(
        engine.run_ok("pcb.teardrop.status", json!({}))["generated"],
        1
    );
}

#[test]
fn branch_halfway_along_the_narrow_segment_stops_the_taper() {
    let dir = TempDir::new("teardrop-branch");
    let (mut engine, net) = board(&dir);
    track(&mut engine, net, 1_000_000, &[(-2_000_000, 0), (0, 0)]);
    track(&mut engine, net, 100_000, &[(0, 0), (3_000_000, 0)]);
    track(
        &mut engine,
        net,
        100_000,
        &[(300_000, 0), (300_000, 1_000_000)],
    );
    enable(&mut engine, json!({"enabled":true,"length":100}));
    let fill = engine.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(fill["teardrops"]["shortened"], 1);
    let project = engine.project().unwrap();
    let drops = &project
        .zone_fills(project.pcb().unwrap().uuid)
        .unwrap()
        .teardrops;
    assert_eq!(
        drops[0].shapes[0].outer.iter().map(|p| p.x.0).max(),
        Some(10_300_000)
    );
}

#[test]
fn reopening_a_cache_preserves_local_freshness_and_locks_allow_refill() {
    let dir = TempDir::new("teardrop-reopen");
    let (mut engine, net) = board(&dir);
    let wide = track(&mut engine, net, 1_000_000, &[(-2_000_000, 0), (0, 0)])[0];
    let narrow = track(&mut engine, net, 100_000, &[(0, 0), (3_000_000, 0)])[0];
    engine.run_ok("pcb.lock", json!({"objects":[wide,narrow],"locked":true}));
    enable(&mut engine, json!({"enabled":true,"length":100}));
    let stale = |e: &mut TestEngine| {
        e.run_ok("pcb.zone.fill_status", json!({}))["stale_teardrops"]
            .as_u64()
            .unwrap()
    };
    assert_eq!(stale(&mut engine), 1);
    engine.run_ok("pcb.zone.fill", json!({}));
    engine.run_ok("project.save", json!({}));
    let folder = engine.location().unwrap().to_owned();
    engine.run_ok("project.close", json!({}));
    engine.run_ok("project.open", json!({"path":folder}));
    assert_eq!(stale(&mut engine), 0);
    let other = uuid(&engine.run_ok("test.net.add", json!({"name":"OTHER"}))["result"]["uuid"]);
    track(
        &mut engine,
        other,
        100_000,
        &[(5_000_000, 5_000_000), (8_000_000, 5_000_000)],
    );
    assert_eq!(stale(&mut engine), 0);
    track(
        &mut engine,
        other,
        100_000,
        &[(750_000, 300_000), (750_000, 1_300_000)],
    );
    assert_eq!(stale(&mut engine), 1);
    let fill = engine.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(fill["teardrops"]["shortened"], 1);
    engine.run_ok("pcb.track.set", json!({"tracks":[wide],"teardrop":false}));
    assert_eq!(stale(&mut engine), 1);
    let fill = engine.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(fill["teardrops"]["shortened"], 0);
    assert_eq!(stale(&mut engine), 0);
    engine.run_ok("pcb.track.set", json!({"tracks":[wide],"teardrop":true}));
    let fill = engine.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(
        fill["teardrops"]["shortened_items"][0]["track"],
        json!(narrow)
    );
}

#[test]
fn curved_taper_follows_the_bend_and_obeys_both_shape_caps() {
    let dir = TempDir::new("teardrop-bend");
    let (mut engine, net) = board(&dir);
    track(&mut engine, net, 1_000_000, &[(-2_000_000, 0), (0, 0)]);
    let parts = track(
        &mut engine,
        net,
        100_000,
        &[(0, 0), (300_000, 0), (300_000, 3_000_000)],
    );
    enable(
        &mut engine,
        json!({"enabled":true,"curved":true,"length":200,"max_length":"1mm","max_width":"0.6mm"}),
    );
    engine.run_ok("pcb.zone.fill", json!({}));
    let project = engine.project().unwrap();
    let board = project.pcb().unwrap();
    let drops = &project.zone_fills(board.uuid).unwrap().teardrops;
    assert_eq!(drops.len(), 1);
    let ring = &drops[0].shapes[0].outer;
    let max_y = ring.iter().map(|p| p.y.0).max().unwrap();
    assert_eq!(max_y, 10_700_000);
    assert!(
        ring.iter()
            .any(|p| p.x.0 == 10_000_000 && p.y.0 == 10_300_000)
    );
    assert!(ring.len() > 10, "curved edges need several chords");
    // Halfway down the path the quadratic has 0.1125 mm half-width.
    let copper = Copper::of(project).unwrap();
    let layer = board.track(parts[0]).unwrap().layer;
    let picks = |x| {
        copper
            .items_at(layer, Point::from([x, 10_200_000]))
            .contains(&Item::Track { uuid: parts[0] })
    };
    assert!(picks(10_400_000));
    assert!(!picks(10_430_000));
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config { cases: 32, ..Default::default() })]
    #[test]
    fn teardrops_never_add_a_clearance_violation(
        x in 715_000i64..1_200_000, y in 265_000i64..600_000,
        curved in proptest::prelude::any::<bool>(), length in 100u32..210
    ) {
        let dir = TempDir::new("teardrop-clearance");
        let (mut engine, net) = board(&dir);
        let other = uuid(&engine.run_ok("test.net.add",json!({"name":"OTHER"}))["result"]["uuid"]);
        track(&mut engine,net,1_000_000,&[(-2_000_000,0),(0,0)]);
        track(&mut engine,net,100_000,&[(0,0),(3_000_000,0)]);
        track(&mut engine,other,100_000,&[(x,y),(x,y+1_000_000)]);
        let violations = |e: &mut TestEngine| {
            let v = e.run_ok("drc.run",json!({}));
            v["violations"].as_array().unwrap().iter().filter(|v|matches!(v["check"].as_str(),Some("clearance"|"short_circuit"))).count()
        };
        proptest::prop_assert_eq!(violations(&mut engine),0);
        enable(&mut engine,json!({"enabled":true,"length":length,"max_length":"2mm","curved":curved}));
        // DRC runs on current copper even though the user has not requested a fill.
        proptest::prop_assert_eq!(violations(&mut engine),0);
        engine.run_ok("pcb.zone.fill", json!({}));
        let project = engine.project().unwrap();
        let fills = project.zone_fills(project.pcb().unwrap().uuid).unwrap();
        proptest::prop_assert_eq!(fills.teardrops.len(),1);
        let shapes = &fills.teardrops[0].shapes;
        proptest::prop_assert!(!shapes.is_empty(),"a shortened taper fits between these Tracks");
        // Against the obstacle's round cap, independently measure each plotted edge.
        let probe = [x as f64+10_000_000.,y as f64+10_000_000.];
        for shape in shapes {
            for (a,b) in shape.outer.iter().zip(shape.outer.iter().cycle().skip(1)) {
                let (dx,dy) = ((b.x.0-a.x.0) as f64,(b.y.0-a.y.0) as f64);
                let (px,py) = (probe[0]-a.x.0 as f64,probe[1]-a.y.0 as f64);
                let t = ((dx*px+dy*py)/(dx*dx+dy*dy)).clamp(0.,1.);
                proptest::prop_assert!((px-t*dx).hypot(py-t*dy)>=210_000.,"violates 0.16 mm copper clearance");
            }
        }
    }
}

#[test]
fn keepout_covering_the_junction_leaves_it_out_and_reports_its_location() {
    let dir = TempDir::new("teardrop-omit");
    let (mut engine, net) = board(&dir);
    let wide = track(&mut engine, net, 1_000_000, &[(-2_000_000, 0), (0, 0)])[0];
    let narrow = track(&mut engine, net, 100_000, &[(0, 0), (3_000_000, 0)])[0];
    engine.run_ok("pcb.keepout.add",json!({"layer":"copper_top","shape":"polygon","restrict":["tracks"],"vertices":[{"x":"9.9mm","y":"9.9mm"},{"x":"10.1mm","y":"9.9mm"},{"x":"10.1mm","y":"10.1mm"},{"x":"9.9mm","y":"10.1mm"}]}));
    enable(&mut engine, json!({"enabled":true}));
    engine.run_ok("pcb.zone.fill", json!({}));
    let status = engine.run_ok("pcb.teardrop.status", json!({}));
    assert_eq!(status["left_out"], 1);
    assert_eq!(status["left_out_items"][0]["track"], json!(narrow));
    assert_eq!(status["left_out_items"][0]["target"], json!(wide));
}

#[test]
fn the_zone_and_route_clear_the_generated_copper() {
    let dir = TempDir::new("teardrop-zone-route");
    let (mut engine, net) = board(&dir);
    let other = uuid(&engine.run_ok("test.net.add", json!({"name":"GND"}))["result"]["uuid"]);
    track(&mut engine, net, 1_000_000, &[(-2_000_000, 0), (0, 0)]);
    let narrow = track(&mut engine, net, 100_000, &[(0, 0), (3_000_000, 0)])[0];
    let zone = uuid(&engine.run_ok("pcb.zone.add",json!({"net":other,"layer":"copper_top","remove_unconnected":false,
        "vertices":[{"x":"7mm","y":"8mm"},{"x":"14mm","y":"8mm"},{"x":"14mm","y":"12mm"},{"x":"7mm","y":"12mm"}]}))["result"]["uuid"]);
    let probe = |e: &TestEngine| {
        let project = e.project().unwrap();
        let pcb = project.pcb().unwrap();
        let layer = pcb.track(narrow).unwrap().layer;
        let copper = Copper::of(project).unwrap();
        let point = Point::from([10_750_000, 10_310_000]);
        let poured = copper
            .items_at(layer, point)
            .iter()
            .any(|i| matches!(i,Item::Zone{zone:z,..} if *z==zone));
        let route = reda_pcb::Proposal::Track {
            layer,
            net: Some(other),
            width: Nm(50_000),
            geometry: reda_model::TrackGeometry::Line {
                from: point,
                to: Point::from([10_750_000, 11_310_000]),
            },
        };
        (
            poured,
            copper
                .clearance(pcb, &route, &[])
                .iter()
                .any(|h| h.item == Item::Track { uuid: narrow }),
        )
    };
    engine.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(probe(&engine), (true, false));
    enable(
        &mut engine,
        json!({"enabled":true,"length":200,"max_length":"2mm"}),
    );
    engine.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(probe(&engine), (false, true));
}

#[test]
fn holes_and_board_cutouts_shorten_an_otherwise_clear_taper() {
    for (command, obstacle) in [
        (
            "pcb.free_pad.add",
            json!({"kind":"non_plated","shape":"circle","at":{"x":"10.9mm","y":"10.56mm"},"width":"0.16mm","hole":{"width":"0.16mm"}}),
        ),
        (
            "pcb.board_cutout.add",
            json!({"vertices":[{"x":"10.9mm","y":"10.49mm"},{"x":"11mm","y":"10.49mm"},{"x":"11mm","y":"11mm"},{"x":"10.9mm","y":"11mm"}]}),
        ),
    ] {
        let dir = TempDir::new("teardrop-edge");
        let (mut e, net) = board(&dir);
        track(&mut e, net, 1_000_000, &[(-2_000_000, 0), (0, 0)]);
        track(&mut e, net, 100_000, &[(0, 0), (3_000_000, 0)]);
        enable(
            &mut e,
            json!({"enabled":true,"length":200,"max_length":"2mm"}),
        );
        assert_eq!(
            e.run_ok("pcb.zone.fill", json!({}))["teardrops"]["generated"],
            1
        );
        e.run_ok(command, obstacle);
        assert_eq!(
            e.run_ok("pcb.zone.fill", json!({}))["teardrops"]["shortened"],
            1,
            "{command}"
        );
        let drc = e.run_ok("drc.run", json!({}));
        assert!(
            drc["violations"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v["check"] != "edge_clearance"),
            "{drc}"
        );
    }
}

#[test]
fn a_taper_reduced_entirely_inside_the_wide_cap_is_left_out() {
    let dir = TempDir::new("teardrop-no-room");
    let (mut e, net) = board(&dir);
    track(&mut e, net, 1_000_000, &[(-2_000_000, 0), (0, 0)]);
    track(&mut e, net, 100_000, &[(0, 0), (3_000_000, 0)]);
    let other = uuid(&e.run_ok("test.net.add", json!({"name":"OTHER"}))["result"]["uuid"]);
    track(
        &mut e,
        other,
        100_000,
        &[(250_000, 210_000), (250_000, 800_000)],
    );
    enable(&mut e, json!({"enabled":true,"length":100}));
    assert_eq!(
        e.run_ok("pcb.zone.fill", json!({}))["teardrops"]["left_out"],
        1
    );
}

#[test]
fn imported_board_and_net_class_rules_control_the_next_fill() {
    let source = TempDir::new("teardrop-import");
    let (mut e, _) = board(&source);
    let class = uuid(&e.run_ok("pcb.net_class.add", json!({"name":"RF"}))["result"]["uuid"]);
    enable(&mut e, json!({"enabled":true,"length":100}));
    e.run_ok("pcb.rules.set",json!({"kind":"teardrop","scope":{"net_class":class},"values":{"width_change":{"enabled":false}}}));
    let path = source.path().join("house.redarules");
    e.run_ok("pcb.rules.export", json!({"path":path}));
    let target = TempDir::new("teardrop-import-board");
    let (mut e, digital) = board(&target);
    let rf = uuid(&e.run_ok("test.net.add", json!({"name":"RF_IN"}))["result"]["uuid"]);
    e.run_ok("pcb.net_class.add", json!({"name":"RF","nets":[rf]}));
    for (net, y) in [(rf, 0), (digital, 3_000_000)] {
        track(&mut e, net, 1_000_000, &[(-2_000_000, y), (0, y)]);
        track(&mut e, net, 100_000, &[(0, y), (3_000_000, y)]);
    }
    e.run_ok(
        "pcb.rules.import",
        json!({"path":path,"kinds":["teardrop"]}),
    );
    assert_eq!(
        e.run_ok("pcb.zone.fill", json!({}))["teardrops"]["generated"],
        1
    );
    let project = e.project().unwrap();
    let pcb = project.pcb().unwrap();
    let d = &project.zone_fills(pcb.uuid).unwrap().teardrops[0];
    assert_eq!(pcb.track(d.location.track).unwrap().net, Some(digital));
}

#[test]
fn importing_outer_layer_teardrop_rules_enables_the_next_fill() {
    let source = TempDir::new("teardrop-rules-source");
    let (mut e, _) = board(&source);
    e.run_ok("pcb.rules.set",json!({"kind":"teardrop","scope":{"layers":"outer"},"values":{"width_change":{"enabled":true,"length":100}}}));
    let path = source.path().join("teardrops.redarules");
    e.run_ok("pcb.rules.export", json!({"path":path}));
    let target = TempDir::new("teardrop-rules-target");
    let (mut e, net) = board(&target);
    track(&mut e, net, 1_000_000, &[(-2_000_000, 0), (0, 0)]);
    track(&mut e, net, 100_000, &[(0, 0), (3_000_000, 0)]);
    e.run_ok(
        "pcb.rules.import",
        json!({"path":path,"kinds":["teardrop"]}),
    );
    assert_eq!(
        e.run_ok("pcb.zone.fill", json!({}))["teardrops"]["generated"],
        1
    );
}

#[test]
fn opposing_tapers_keep_clear_of_each_other_and_reuse_their_fills() {
    let dir = TempDir::new("teardrop-neighbours");
    let (mut e, a) = board(&dir);
    let b = uuid(&e.run_ok("test.net.add", json!({"name":"OTHER"}))["result"]["uuid"]);
    track(&mut e, a, 1_000_000, &[(-2_000_000, 0), (0, 0)]);
    track(
        &mut e,
        a,
        100_000,
        &[(0, 0), (600_000, 400_000), (3_000_000, 400_000)],
    );
    track(
        &mut e,
        b,
        1_000_000,
        &[(-2_000_000, 1_420_000), (0, 1_420_000)],
    );
    track(
        &mut e,
        b,
        100_000,
        &[(0, 1_420_000), (600_000, 1_020_000), (3_000_000, 1_020_000)],
    );
    enable(
        &mut e,
        json!({"enabled":true,"max_length":"2mm","length":200}),
    );
    let out = e.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(out["teardrops"]["generated"], 1);
    assert_eq!(out["teardrops"]["shortened"], 1);
    assert_eq!(
        e.run_ok("pcb.zone.fill_status", json!({}))["stale_teardrops"],
        0
    );
    let drc = e.run_ok("drc.run", json!({}));
    assert!(
        drc["violations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["check"] != "clearance" && v["check"] != "short_circuit"),
        "{drc}"
    );
}

#[test]
fn a_track_ending_off_centre_on_a_free_pad_uses_the_physical_lands_edge_and_transverse_size() {
    let dir = TempDir::new("off-centre-pad-teardrop");
    let (mut e, net) = board(&dir);
    let pad = e.run_ok(
        "pcb.free_pad.add",
        json!({"kind":"plated_through_hole","net":net,"shape":"rect","width":"2mm","height":"1mm",
        "hole":{"width":"0.4mm"},"at":{"x":"10mm","y":"10mm"}}),
    )["result"]["uuid"]
        .clone();
    let narrow = track(&mut e, net, 100_000, &[(500_000, 0), (3_000_000, 0)])[0];
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"teardrop","values":{"through_hole_pad":{"enabled":true}}}),
    );
    let fill = e.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(fill["teardrops"]["generated"], 1);
    let project = e.project().unwrap();
    let pcb = project.pcb().unwrap();
    let drop = &project.zone_fills(pcb.uuid).unwrap().teardrops[0];
    assert_eq!(drop.location.track, narrow);
    assert_eq!(json!(drop.location.target), pad);
    let points = &drop.shapes[0].outer;
    assert!(
        points
            .iter()
            .map(|p| p.x.0)
            .min()
            .unwrap()
            .abs_diff(11_000_000)
            <= 1
    );
    assert!(
        points
            .iter()
            .map(|p| p.x.0)
            .max()
            .unwrap()
            .abs_diff(11_500_000)
            <= 1
    );
    assert_eq!(points.iter().map(|p| p.y.0).min(), Some(9_500_000));
    assert_eq!(points.iter().map(|p| p.y.0).max(), Some(10_500_000));
}

fn pad_with_zone(e: &mut TestEngine, net: Uuid, style: &str) -> Uuid {
    e.run_ok(
        "pcb.free_pad.add",
        json!({"kind":"plated_through_hole","net":net,"width":"2mm",
        "hole":{"width":"0.4mm"},"at":{"x":"10mm","y":"10mm"},
        "zone_connection":{"style":style,"gap":"0.25mm","spokes":2,"angle":"90deg"}}),
    );
    track(e, net, 100_000, &[(0, 0), (3_000_000, 0)]);
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"teardrop","values":{"through_hole_pad":{"enabled":true,"length":100}}}),
    );
    uuid(&e.run_ok("pcb.zone.add",json!({"layer":"copper_top","net":net,"remove_unconnected":false,"vertices":[
        {"x":"5mm","y":"5mm"},{"x":"15mm","y":"5mm"},{"x":"15mm","y":"15mm"},{"x":"5mm","y":"15mm"}]}))["result"]["uuid"])
}

#[test]
fn only_a_solid_connection_to_actual_poured_copper_suppresses_the_pad_taper() {
    let dir = TempDir::new("solid-pad-teardrop");
    let (mut e, net) = board(&dir);
    pad_with_zone(&mut e, net, "solid");
    e.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(e.run_ok("pcb.teardrop.status", json!({}))["generated"], 0);
    e.run_ok("pcb.zone_cutout.add",json!({"layer":"copper_top","vertices":[
        {"x":"8mm","y":"8mm"},{"x":"12mm","y":"8mm"},{"x":"12mm","y":"12mm"},{"x":"8mm","y":"12mm"}]}));
    assert_eq!(
        e.run_ok("pcb.zone.fill_status", json!({}))["stale_teardrops"],
        1
    );
    assert_eq!(
        e.run_ok("pcb.zone.fill", json!({}))["teardrops"]["generated"],
        1
    );
}

#[test]
fn the_zone_gap_includes_the_taper_of_a_relieved_or_disconnected_pad() {
    for style in ["relief", "none"] {
        let dir = TempDir::new("gap-pad-teardrop");
        let (mut e, net) = board(&dir);
        let zone = pad_with_zone(&mut e, net, style);
        assert_eq!(
            e.run_ok("pcb.zone.fill", json!({}))["teardrops"]["generated"],
            1
        );
        let project = e.project().unwrap();
        let layer = project.pcb().unwrap().zone(zone).unwrap().layer;
        let copper = Copper::of(project).unwrap();
        let in_zone = |x, y| {
            copper
                .items_at(layer, Point::new(Nm(x), Nm(y)))
                .iter()
                .any(|i| matches!(i,Item::Zone{zone:z,..} if *z==zone))
        };
        assert!(
            !in_zone(11_500_000, 10_750_000),
            "{style} gap must continue along taper"
        );
        assert!(
            in_zone(11_500_000, 11_250_000),
            "{style} should still fill outside the gap"
        );
    }
}

#[test]
fn tapers_follow_a_nonuniform_vias_actual_copper_on_each_layer() {
    let dir = TempDir::new("via-diameter-teardrops");
    let mut e = probe_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Vias","layers":4}),
    );
    let net = uuid(&e.run_ok("test.net.add", json!({"name":"SIG"}))["result"]["uuid"]);
    let pcb = e.project().unwrap().pcb().unwrap();
    let layers: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    let at = json!({"x":"10mm","y":"10mm"});
    let via = uuid(
        &e.run_ok(
            "pcb.via.add",
            json!({"net":net,"at":at,"hole":"0.25mm",
        "diameter":{"start":"0.6mm","middle":"1mm","end":"1.4mm"},"unused_layers":"remove"}),
        )["result"]["uuid"],
    );
    for layer in [layers[0], layers[1], layers[3]] {
        e.run_ok(
            "pcb.track.add",
            json!({"layer":layer,"net":net,"width":"0.10mm","points":[at,{"x":"14mm","y":"10mm"}]}),
        );
    }
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"teardrop","values":{"via":{"enabled":true}}}),
    );
    assert_eq!(
        e.run_ok("pcb.zone.fill", json!({}))["teardrops"]["generated"],
        3
    );
    let project = e.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fills = project.zone_fills(pcb.uuid).unwrap();
    let copper = Copper::of(project).unwrap();
    assert!(
        !copper
            .items_at(layers[2], Point::new(Nm(10_000_000), Nm(10_000_000)))
            .contains(&Item::Via { uuid: via })
    );
    for (layer, radius) in [
        (layers[0], 300_000),
        (layers[1], 500_000),
        (layers[3], 700_000),
    ] {
        let drop = fills
            .teardrops
            .iter()
            .find(|d| d.location.layer == layer && d.location.target == via)
            .unwrap();
        assert!(drop.location.at.x.0.abs_diff(10_000_000 + radius) <= 1);
        let max = drop.shapes[0].outer.iter().map(|p| p.y.0).max().unwrap();
        assert_eq!(max, 10_000_000 + radius);
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config { cases: 24, ..Default::default() })]
    #[test]
    fn enabling_pad_and_via_tapers_keeps_other_net_tracks_clear(
        x in 715_000i64..1_200_000, y in 275_000i64..640_000,
        offset in -190_000i64..190_000, curved in proptest::prelude::any::<bool>(),
        length in 80u32..180,
    ) {
        for kind in ["via", "through_hole_pad", "smd_pad"] {
            let dir = TempDir::new("land-clearance-property");
            let (mut e, net) = board(&dir);
            let other = uuid(&e.run_ok("test.net.add", json!({"name":"OTHER"}))["result"]["uuid"]);
            let at=json!({"x":"10mm","y":"10mm"});
            match kind {
                "via" => { e.run_ok("pcb.via.add",json!({"net":net,"at":at,"hole":"0.3mm","diameter":"1mm"})); },
                "through_hole_pad" => { e.run_ok("pcb.free_pad.add",json!({"kind":"plated_through_hole","net":net,"at":at,"hole":{"width":"0.4mm"},"width":"1mm","shape":"rect"})); },
                _ => { e.run_ok("pcb.free_pad.add",json!({"kind":"smd","net":net,"at":at,"width":"1mm","shape":"rounded_rect"})); },
            };
            track(&mut e,net,100_000,&[(0,offset),(3_000_000,offset)]);
            track(&mut e,other,100_000,&[(x,y+offset),(x,y+offset+1_000_000)]);
            let report=|e:&mut TestEngine| {
                let value=e.run_ok("drc.run",json!({}));
                value["violations"].as_array().unwrap().iter()
                    .filter(|v|matches!(v["check"].as_str(),Some("clearance"|"short_circuit")))
                    .cloned().collect::<Vec<_>>()
            };
            let before=report(&mut e);
            // The other Track stays outside the original 1 mm land and the narrow route.
            proptest::prop_assert!(before.is_empty(),"{kind}: {before:?}");
            e.run_ok("pcb.rules.set",json!({"kind":"teardrop","values":{kind:{"enabled":true,"length":length,"max_length":"2mm","curved":curved}}}));
            proptest::prop_assert_eq!(report(&mut e),before);
            e.run_ok("pcb.zone.fill",json!({}));
            let project=e.project().unwrap();
            let fills=project.zone_fills(project.pcb().unwrap().uuid).unwrap();
            proptest::prop_assert_eq!(fills.teardrops.len(),1);
        }
    }
}
