//! Plated holes on a four-Layer board through the editor's shared Command path.

use reda_engine::model::{Uuid, geom::Nm};
use reda_engine::render::{Primitive, SceneLayer};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::json;

fn board(dir: &TempDir) -> (TestEngine, Uuid, Uuid) {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"name":"Demo","path":dir.path(),"layers":4}),
    );
    let pad = engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind":"plated_through_hole","at":{"x":"5mm","y":"5mm"},
        "shape":"circle","width":"2mm","hole":{"width":"0.5mm"},
        "middle":{"shape":"rect","width":"4mm","height":"1mm"}}),
    )["result"]["uuid"]
        .clone();
    let via = engine.run_ok(
        "pcb.via.add",
        json!({"at":{"x":"10mm","y":"5mm"},
        "diameter":{"start":"0.7mm","middle":"1mm","end":"1.4mm"},"hole":"0.3mm"}),
    )["result"]["uuid"]
        .clone();
    (
        engine,
        serde_json::from_value(pad).unwrap(),
        serde_json::from_value(via).unwrap(),
    )
}

#[test]
fn the_board_canvas_draws_each_land_and_just_one_shared_hole() {
    let dir = TempDir::new("pad-stack-scene");
    let (engine, pad, via) = board(&dir);
    let pcb = engine.project().unwrap().pcb().unwrap();
    let scene = engine.board_scene(pcb.uuid, &Default::default()).unwrap();
    let primitive = |id, layer| {
        &scene
            .items
            .iter()
            .find(|i| i.object == id && i.layer == layer)
            .unwrap()
            .primitive
    };
    for (n, l) in pcb.copper_layers().enumerate() {
        let layer = SceneLayer::Board(l.uuid);
        match n {
            1 | 2 => assert!(matches!(
                primitive(pad, layer),
                Primitive::Rect {
                    size: (Nm(4_000_000), Nm(1_000_000)),
                    ..
                }
            )),
            _ => assert!(matches!(
                primitive(pad, layer),
                Primitive::Ring {
                    outer: Nm(1_000_000),
                    ..
                }
            )),
        }
        let expected = [350000, 500000, 500000, 700000][n];
        assert!(matches!(primitive(via,layer),Primitive::Ring{outer,..} if outer.0==expected));
    }
    for (object, radius) in [(pad, 250000), (via, 150000)] {
        let holes = scene
            .items
            .iter()
            .filter(|i| {
                i.object == object
                    && i.layer == SceneLayer::Drill
                    && matches!(i.primitive,Primitive::Ring{outer,..} if outer.0==radius)
            })
            .count();
        assert_eq!(holes, 1);
    }
}

#[test]
fn the_copper_index_finds_only_the_land_present_on_the_queried_layer() {
    use reda_engine::model::geom::Point;
    use reda_engine::pcb::{Copper, Item};
    let dir = TempDir::new("pad-stack-index");
    let (engine, pad, via) = board(&dir);
    let pcb = engine.project().unwrap().pcb().unwrap();
    let copper = Copper::build(pcb);
    let point = |x, y| Point::new(Nm(x), Nm(y));
    for (n, l) in pcb.copper_layers().enumerate() {
        assert_eq!(
            copper
                .items_at(l.uuid, point(6_900_000, 5_000_000))
                .contains(&Item::FreePad { uuid: pad }),
            n == 1 || n == 2
        );
        assert_eq!(
            copper
                .items_at(l.uuid, point(10_600_000, 5_000_000))
                .contains(&Item::Via { uuid: via }),
            n == 3
        );
    }
}

#[test]
fn adding_an_inner_layer_uses_the_local_pad_stack_and_undo_restores_the_copy() {
    use reda_engine::model::geom::Point;
    use reda_engine::pcb::{Copper, Item};
    let dir = TempDir::new("placed-pad-stack-undo");
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"name":"Demo","path":dir.path()}));
    let added = engine.run_ok(
        "test.footprint.add",
        json!({
            "at":{"x":"5mm","y":"5mm"}, "pads":[{
                "at":{"x":0,"y":0}, "width":"2mm","height":"2mm","shape":"circle",
                "kind":"plated_through_hole","hole":{"width":500000,"height":500000}
            }]
        }),
    );
    let footprint = serde_json::from_value(added["result"]["footprint"].clone()).unwrap();
    let pad = serde_json::from_value(added["result"]["pads"][0].clone()).unwrap();
    let pcb = engine.project().unwrap().pcb().unwrap();
    let document = pcb.uuid;
    let original = pcb.footprint(footprint).unwrap().footprint;
    engine.run_ok(
        "pcb.pad.edit",
        json!({"instance":footprint,"pad":pad,
            "middle":{"shape":"rect","width":"4mm","height":"1mm"}
        }),
    );
    let copy = |e: &TestEngine| {
        e.project()
            .unwrap()
            .pcb()
            .unwrap()
            .footprint(footprint)
            .unwrap()
            .footprint
    };
    let edited = copy(&engine);
    assert_ne!(edited, original);
    let added = engine.run_ok("pcb.layer.add", json!({"document":document}));
    let inner: Uuid = serde_json::from_value(added["result"]["uuid"].clone()).unwrap();
    let indexed_layers = |e: &TestEngine| {
        let project = e.project().unwrap();
        let copper = Copper::of(project).unwrap();
        project
            .pcb()
            .unwrap()
            .copper_layers()
            .filter(|l| {
                copper
                    .items_at(l.uuid, Point::new(Nm(6_900_000), Nm(5_000_000)))
                    .contains(&Item::Pad { footprint, pad })
            })
            .map(|l| l.uuid)
            .collect::<Vec<_>>()
    };
    assert_eq!(indexed_layers(&engine), [inner]);
    assert_eq!(copy(&engine), edited);
    engine.run_ok("edit.undo", json!({"document":document}));
    assert!(indexed_layers(&engine).is_empty());
    engine.run_ok("edit.undo", json!({"document":document}));
    assert_eq!(copy(&engine), original);
    engine.run_ok("edit.redo", json!({"document":document}));
    engine.run_ok("edit.redo", json!({"document":document}));
    assert_eq!(copy(&engine), edited);
    assert_eq!(indexed_layers(&engine), [inner]);
}
