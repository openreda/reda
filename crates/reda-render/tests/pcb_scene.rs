//! PCB Scenes, checked as text snapshots and by what they draw.

use std::path::{Path, PathBuf};

mod common;

use common::{demo, layer, rect, sample_board, uuid};
use reda_model::{LayerKind, Side, Uuid};
use reda_render::{BoardColor, ColorKey, Primitive, Scene, SceneLayer};
use reda_testkit::{TempDir, assert_golden};
use serde_json::json;

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn colors_of(scene: &Scene, object: Uuid) -> Vec<ColorKey> {
    scene.items_of(object).map(|i| i.color).collect()
}

#[test]
fn a_sample_board_draws_every_kind_of_object() {
    let dir = TempDir::new("pcb-scene");
    let mut engine = demo(&dir);
    let board = sample_board(&mut engine);
    let scene = engine.document_scene(board.pcb).unwrap();
    assert_golden(golden("board.scene"), scene.to_text().as_bytes());
}

#[test]
fn zone_fills_are_drawn_filled_stale_or_not_at_all() {
    let dir = TempDir::new("pcb-zones");
    let mut engine = demo(&dir);
    let board = sample_board(&mut engine);
    let fill = ColorKey::Board(BoardColor::CopperBottom);
    let stale = ColorKey::Board(BoardColor::StaleFillBottom);
    let triangles = |scene: &Scene, zone: Uuid| -> Vec<ColorKey> {
        scene
            .items_of(zone)
            .filter(|i| matches!(i.primitive, Primitive::Triangles(_)))
            .map(|i| i.color)
            .collect()
    };
    let outline = |scene: &Scene, zone: Uuid| {
        scene
            .items_of(zone)
            .filter(|i| matches!(i.primitive, Primitive::Capsule { .. }))
            .count()
    };

    let scene = engine.document_scene(board.pcb).unwrap();
    assert!(!triangles(&scene, board.zone).is_empty());
    assert!(triangles(&scene, board.zone).iter().all(|c| *c == fill));
    assert_eq!(outline(&scene, board.zone), 4);

    // A new Zone makes the board's fills stale, and it has never been filled itself.
    let second = engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "vertices": rect(20.0, 2.0, 28.0, 6.0)}),
    );
    let second = uuid(&second["result"]["uuid"]);
    let scene = engine.document_scene(board.pcb).unwrap();
    assert!(!triangles(&scene, board.zone).is_empty());
    assert!(triangles(&scene, board.zone).iter().all(|c| *c == stale));
    assert!(triangles(&scene, second).is_empty());
    assert_eq!(outline(&scene, second), 4);
}

#[test]
fn keepouts_are_outlined_on_each_of_their_layers_and_the_output_origin_is_marked() {
    let dir = TempDir::new("pcb-keepouts");
    let mut engine = demo(&dir);
    let board = sample_board(&mut engine);
    let keepout = engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": rect(2.0, 2.0, 6.0, 6.0), "layers": ["copper_bottom"]}),
    );
    let keepout = uuid(&keepout["result"]["uuid"]);
    let origin = engine.run_ok(
        "pcb.output_origin.set",
        json!({"at": {"x": "1mm", "y": "1mm"}}),
    );
    let origin = uuid(&origin["result"]["uuid"]);
    let scene = engine.document_scene(board.pcb).unwrap();

    let bottom = layer(&engine, LayerKind::Copper, Some(Side::Bottom));
    let items: Vec<_> = scene.items_of(keepout).collect();
    assert_eq!(items.len(), 4, "one stroke per edge");
    assert!(items.iter().all(|i| i.layer == SceneLayer::Board(bottom)));

    let outline = layer(&engine, LayerKind::BoardOutline, None);
    let marks: Vec<_> = scene.items_of(origin).collect();
    assert!(!marks.is_empty());
    assert!(marks.iter().all(|i| i.layer == SceneLayer::Board(outline)));
}

#[test]
fn pads_and_tracks_carry_view_only_labels() {
    let dir = TempDir::new("pcb-labels");
    let mut engine = demo(&dir);
    let board = sample_board(&mut engine);
    let scene = engine.document_scene(board.pcb).unwrap();
    let labels = |object: Uuid| -> Vec<String> {
        scene
            .items_of(object)
            .filter_map(|i| match &i.primitive {
                Primitive::Glyphs(run) if run.view_label => Some(run.text.clone()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(labels(board.fp1), ["1", "SIG", "2", "GND"]);
    assert_eq!(labels(board.track), ["GND"]);
    // The Designator is Document text, always drawn.
    assert!(scene.items_of(board.fp1).any(|i| matches!(
        &i.primitive,
        Primitive::Glyphs(run) if !run.view_label && run.text == "TP"
    )));
}

#[test]
fn colours_follow_layer_kind_and_side() {
    let dir = TempDir::new("pcb-colours");
    let mut engine = demo(&dir);
    let board = sample_board(&mut engine);
    let scene = engine.document_scene(board.pcb).unwrap();
    let top = layer(&engine, LayerKind::Copper, Some(Side::Top));
    assert_eq!(
        colors_of(&scene, board.track)[0],
        ColorKey::Board(BoardColor::CopperTop)
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    for item in &scene.items {
        let SceneLayer::Board(uuid) = item.layer else {
            continue;
        };
        let l = pcb.layers().layer(uuid).unwrap();
        let ColorKey::Board(color) = item.color else {
            panic!("{:?} on a board", item.color);
        };
        let allowed = [
            BoardColor::layer(l.kind, l.side),
            BoardColor::stale_fill(l.side),
            BoardColor::PadNumber,
            BoardColor::NetName,
        ];
        assert!(allowed.contains(&color), "{color:?} on {:?}", l.kind);
    }
    assert!(
        scene
            .items
            .iter()
            .any(|i| i.layer == SceneLayer::Board(top))
    );
}

#[test]
fn the_ratsnest_joins_unrouted_pads_on_its_own_layer() {
    let dir = TempDir::new("pcb-ratsnest");
    let mut engine = demo(&dir);
    let board = sample_board(&mut engine);
    let scene = engine.document_scene(board.pcb).unwrap();
    let lines: Vec<_> = scene
        .items
        .iter()
        .filter(|i| i.layer == SceneLayer::Ratsnest)
        .collect();
    assert_eq!(lines.len(), 1, "the two SIG Pads");
    assert_eq!(lines[0].color, ColorKey::Board(BoardColor::Ratsnest));
    let holes = scene
        .items
        .iter()
        .filter(|i| i.layer == SceneLayer::Drill && i.color == ColorKey::Board(BoardColor::Hole))
        .count();
    assert_eq!(holes, 2, "the mounting hole and the Via");
}
