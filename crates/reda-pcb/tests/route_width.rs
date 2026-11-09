//! The routing width: where a route's width starts, the width modes, clipping to the Net's
//! range and the chooser's list. The Net PWR is in a class with Track widths min 0.2 mm,
//! preferred 0.3 mm and max 0.5 mm.

use reda_model::geom::{Nm, Point};
use reda_model::{LayerKind, Limit, PcbDocument, Side, Uuid};
use reda_pcb::Copper;
use reda_pcb::route::width::{clip_width, mode_width, next_mode, start_width, width_choices};
use reda_pcb::route::{WidthChoice, WidthMode, WidthSource};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn um(n: i64) -> Nm {
    Nm::from_um(n)
}

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm::from_mm(x), Nm::from_mm(y))
}

fn board(dir: &TempDir) -> (TestEngine, Uuid) {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let pwr = uuid(&engine.run_ok("test.net.add", json!({"name": "PWR"}))["result"]["uuid"]);
    let class = engine.run_ok(
        "pcb.net_class.add",
        json!({"name": "Power", "nets": [pwr],
               "rules": [{"kind": "track_width",
                          "values": {"min": "0.2mm", "preferred": "0.3mm", "max": "0.5mm"}}]}),
    );
    assert!(class["result"]["uuid"].is_string());
    (engine, pwr)
}

fn top(engine: &TestEngine) -> Uuid {
    pcb(engine)
        .layers()
        .find(LayerKind::Copper, Some(Side::Top))
        .unwrap()
        .uuid
}

#[test]
fn a_typed_width_is_clipped_to_the_nets_range() {
    let dir = TempDir::new("width-clip");
    let (engine, pwr) = board(&dir);
    let pcb = pcb(&engine);
    let fits = clip_width(pcb, Some(pwr), um(400));
    assert_eq!((fits.width, fits.limit), (um(400), None));
    let wide = clip_width(pcb, Some(pwr), um(800));
    assert_eq!((wide.width, wide.limit), (um(500), Some(Limit::Max)));
    let thin = clip_width(pcb, Some(pwr), um(100));
    assert_eq!((thin.width, thin.limit), (um(200), Some(Limit::Min)));
}

#[test]
fn width_modes_cycle_skipping_absent_values() {
    let dir = TempDir::new("width-modes");
    let (engine, pwr) = board(&dir);
    let pcb = pcb(&engine);
    let net = Some(pwr);
    assert_eq!(mode_width(pcb, net, WidthMode::Min, None), Some(um(200)));
    assert_eq!(
        mode_width(pcb, net, WidthMode::Preferred, None),
        Some(um(300))
    );
    assert_eq!(mode_width(pcb, net, WidthMode::Max, None), Some(um(500)));
    assert_eq!(mode_width(pcb, net, WidthMode::User, None), None);
    assert_eq!(
        mode_width(pcb, net, WidthMode::User, Some(um(900))),
        Some(um(500)),
        "a user value is clipped too"
    );
    assert_eq!(
        next_mode(pcb, net, WidthMode::Max, None),
        WidthMode::Min,
        "an empty user is skipped"
    );
    assert_eq!(
        next_mode(pcb, net, WidthMode::Max, Some(um(350))),
        WidthMode::User
    );
    assert_eq!(
        next_mode(pcb, net, WidthMode::User, Some(um(350))),
        WidthMode::Min
    );
    // The board-wide record has no max: no Net there skips it.
    assert_eq!(mode_width(pcb, None, WidthMode::Max, None), None);
    assert_eq!(
        next_mode(pcb, None, WidthMode::Preferred, None),
        WidthMode::Min
    );
    assert_eq!(WidthMode::default(), WidthMode::Preferred);
}

#[test]
fn a_route_takes_the_track_width_it_starts_on_unless_pickup_is_off() {
    let dir = TempDir::new("width-pickup");
    let (mut engine, pwr) = board(&dir);
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": pwr, "width": "0.45mm",
               "points": [{"x": 0, "y": 0}, {"x": "10mm", "y": 0}]}),
    );
    let copper = Copper::of(engine.project().unwrap()).unwrap();
    let pcb = pcb(&engine);
    let on_track = (top(&engine), mm(5, 0));
    let start = |mode, pickup| start_width(pcb, &copper, Some(pwr), on_track, mode, None, pickup);
    assert_eq!(start(WidthMode::Preferred, true), um(450));
    assert_eq!(start(WidthMode::Preferred, false), um(300));
    assert_eq!(start(WidthMode::Max, false), um(500));
    assert_eq!(
        start(WidthMode::User, false),
        um(300),
        "an empty user falls back"
    );
    let free = (top(&engine), mm(5, 5));
    assert_eq!(
        start_width(pcb, &copper, Some(pwr), free, WidthMode::Min, None, true),
        um(200)
    );
}

#[test]
fn the_chooser_lists_the_nets_widths_favourites_and_widths_on_the_board() {
    let dir = TempDir::new("width-choices");
    let (mut engine, pwr) = board(&dir);
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": pwr, "width": "0.45mm",
               "points": [{"x": 0, "y": 0}, {"x": "10mm", "y": 0}]}),
    );
    let choices = width_choices(pcb(&engine), Some(pwr), &[um(1000), um(300)]);
    let row = |w, source| WidthChoice {
        width: um(w),
        source,
    };
    assert_eq!(
        choices,
        [
            row(200, WidthSource::Min),
            row(300, WidthSource::Preferred),
            row(500, WidthSource::Max),
            row(1000, WidthSource::Favourite),
            row(450, WidthSource::Board),
        ]
    );
}

#[test]
fn the_via_chooser_lists_the_nets_size_then_the_sizes_on_the_board() {
    use reda_pcb::route::width::via_choices;
    use reda_pcb::route::{ViaChoice, ViaSize};
    let dir = TempDir::new("via-choices");
    let (mut engine, pwr) = board(&dir);
    let preferred = {
        let rules = reda_pcb::rules::net_rules(pcb(&engine), Some(pwr));
        ViaSize {
            diameter: rules.via_diameter.default_size(),
            hole: rules.via_hole.default_size(),
        }
    };
    for (x, d, h) in [(0, "1mm", "0.5mm"), (5, "0.8mm", "0.4mm"), (9, "1mm", "0.5mm")] {
        engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": format!("{x}mm"), "y": 0}, "diameter": d, "hole": h}),
        );
    }
    let choices = via_choices(pcb(&engine), Some(pwr));
    assert_eq!(
        choices,
        [
            ViaChoice {
                size: preferred,
                source: WidthSource::Preferred
            },
            ViaChoice {
                size: ViaSize {
                    diameter: um(800),
                    hole: um(400)
                },
                source: WidthSource::Board
            },
            ViaChoice {
                size: ViaSize {
                    diameter: um(1000),
                    hole: um(500)
                },
                source: WidthSource::Board
            },
        ]
    );
}
