//! The PCB view's drawing choices (`BoardStyle`): the Ratsnest's screen-width lines and
//! Net colours, Net colour on copper (`F5`), the Net label toggles and per-Footprint
//! Ratsnest hiding.

mod common;

use common::{demo, layer, sample_board, uuid, xy};
use reda_model::{LayerKind, Side, Uuid};
use reda_render::{
    BoardColor, BoardStyle, ColorKey, Look, NET_COLOR_ZOOM_OUT, NetColorPattern, NetColorStyle,
    NetColorZoomOut, NetLabels, Primitive, Rgba, Scene, SceneLayer,
};
use reda_testkit::{TempDir, TestEngine};
use serde_json::json;

fn net(engine: &TestEngine, name: &str) -> Uuid {
    let pcb = engine.project().unwrap().pcb().unwrap();
    pcb.nets().find(|n| n.name == name).unwrap().uuid
}

fn ratsnest(scene: &Scene) -> Vec<&reda_render::Item> {
    scene
        .items
        .iter()
        .filter(|i| i.layer == SceneLayer::Ratsnest)
        .collect()
}

fn labels(scene: &Scene, object: Uuid) -> Vec<String> {
    scene
        .items_of(object)
        .filter_map(|i| match &i.primitive {
            Primitive::Glyphs(run) if run.view_label => Some(run.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_ratsnest_line_is_a_screen_width_line_in_its_nets_colour() {
    let dir = TempDir::new("view-ratsnest");
    let mut engine = demo(&dir);
    let board = sample_board(&mut engine);
    let scene = engine.document_scene(board.pcb).unwrap();
    let lines = ratsnest(&scene);
    assert_eq!(lines.len(), 1);
    assert!(matches!(lines[0].primitive, Primitive::Hairline { .. }));
    assert_eq!(lines[0].color, ColorKey::Board(BoardColor::Ratsnest));

    let sig = net(&engine, "SIG");
    engine.run_ok(
        "pcb.net.set_color",
        json!({"nets": [sig], "color": "#ff8000"}),
    );
    let scene = engine.document_scene(board.pcb).unwrap();
    assert_eq!(
        ratsnest(&scene)[0].color,
        ColorKey::Net(Rgba::rgb(0xff, 0x80, 0x00)),
        "a Net's colour always applies to its lines"
    );
    // Copper keeps its Layer colour until F5.
    let pad_colors: Vec<ColorKey> = scene.items_of(board.fp1).map(|i| i.color).collect();
    assert!(!pad_colors.iter().any(|c| matches!(c, ColorKey::Net(_))));
}

#[test]
fn f5_lays_the_net_colour_over_copper_in_the_preferred_pattern_and_zoom_out() {
    let dir = TempDir::new("view-net-colors");
    let mut engine = demo(&dir);
    let board = sample_board(&mut engine);
    let gnd = net(&engine, "GND");
    engine.run_ok(
        "pcb.net.set_color",
        json!({"nets": [gnd], "color": "#00ff00"}),
    );
    let green = ColorKey::Net(Rgba::rgb(0, 0xff, 0));
    let top = SceneLayer::Board(layer(&engine, LayerKind::Copper, Some(Side::Top)));
    let overlay = |scene: &Scene, object: Uuid| -> Vec<(SceneLayer, Look)> {
        scene
            .items_of(object)
            .filter(|i| i.color == green)
            .map(|i| (i.layer, i.look))
            .collect()
    };

    let style = BoardStyle {
        net_colors: Some(NetColorStyle::default()),
        ..BoardStyle::default()
    };
    let scene = engine.board_scene(board.pcb, &style).unwrap();
    // The Track: its capsule in the Layer colour, then the same capsule in the Net's.
    let track: Vec<_> = scene.items_of(board.track).collect();
    assert_eq!(track[0].color, ColorKey::Board(BoardColor::CopperTop));
    assert_eq!(track[0].primitive, track[1].primitive);
    assert_eq!(overlay(&scene, board.track), [(top, Look::default())]);
    assert!(!overlay(&scene, board.zone).is_empty(), "the GND Zone fill");
    // The SIG Pad of fp1 has no colour; its GND Pad does, on copper only (not mask).
    assert_eq!(overlay(&scene, board.fp1), [(top, Look::default())]);

    let style = BoardStyle {
        net_colors: Some(NetColorStyle {
            pattern: NetColorPattern::Checker,
            zoom_out: NetColorZoomOut::LayerColor,
        }),
        ..BoardStyle::default()
    };
    let scene = engine.board_scene(board.pcb, &style).unwrap();
    let look = Look {
        checker: true,
        zoom_out: Some(NET_COLOR_ZOOM_OUT),
        ..Look::default()
    };
    assert_eq!(overlay(&scene, board.track), [(top, look)]);
    assert!(
        scene
            .to_text()
            .contains("Net#00ff00 capsule (9,10) (15,10) w=0.5 checker zoom_out=1"),
        "{}",
        scene.to_text()
    );
}

#[test]
fn pad_nets_via_nets_and_repeated_net_names_are_view_toggles() {
    let dir = TempDir::new("view-labels");
    let mut engine = demo(&dir);
    let board = sample_board(&mut engine);
    let via = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .vias()
        .next()
        .unwrap()
        .uuid;
    let long = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": net(&engine, "SIG").to_string(),
               "width": "0.5mm", "points": [xy(2.0, 18.0), xy(28.0, 18.0)]}),
    );
    let long = uuid(&long["result"]["tracks"][0]);

    let scene = engine
        .board_scene(board.pcb, &BoardStyle::default())
        .unwrap();
    assert_eq!(labels(&scene, board.fp1), ["1", "SIG", "2", "GND"]);
    assert_eq!(labels(&scene, via), ["GND"]);
    assert_eq!(labels(&scene, long), ["SIG"], "one name, in the middle");

    let style = BoardStyle {
        labels: NetLabels {
            pad_nets: false,
            via_nets: false,
            repeated_net_names: true,
        },
        ..BoardStyle::default()
    };
    let scene = engine.board_scene(board.pcb, &style).unwrap();
    assert_eq!(labels(&scene, board.fp1), ["1", "2"], "Pad numbers stay");
    assert!(labels(&scene, via).is_empty());
    let repeated = labels(&scene, long);
    assert!(repeated.len() > 3, "{repeated:?}");
    assert!(repeated.iter().all(|t| t == "SIG"));
    // Each is still a view label, hidden below the minimum on-screen size.
    assert!(scene.items_of(long).all(|i| match &i.primitive {
        Primitive::Glyphs(run) => run.view_label,
        _ => true,
    }));
}

#[test]
fn a_footprints_ratsnest_lines_can_be_hidden() {
    let dir = TempDir::new("view-ratsnest-footprints");
    let mut engine = demo(&dir);
    let board = sample_board(&mut engine);
    let style = BoardStyle {
        hidden_ratsnest_footprints: [board.fp1].into(),
        ..BoardStyle::default()
    };
    let scene = engine.board_scene(board.pcb, &style).unwrap();
    assert!(ratsnest(&scene).is_empty(), "the one SIG line ends on fp1");
    let style = BoardStyle {
        hidden_ratsnest_footprints: [Uuid::from_u128(7)].into(),
        ..BoardStyle::default()
    };
    let scene = engine.board_scene(board.pcb, &style).unwrap();
    assert_eq!(ratsnest(&scene).len(), 1);
}
