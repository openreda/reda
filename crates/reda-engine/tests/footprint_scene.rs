//! The Footprint editor's Scene: an open Footprint drawn on the board Layers, each item
//! tagged with the Pad, graphic or text the editor selects.

use reda_engine::model::LibraryElement;
use reda_engine::render::{BoardColor, ColorKey, SceneLayer};
use reda_engine::{Engine, footprint_scene};
use reda_testkit::TempDir;
use serde_json::json;

const DEMO_FOOTPRINT: &str = "00000030-0000-4000-8000-000000000000";

fn open_demo() -> (reda_testkit::TestEngine, TempDir) {
    let dir = TempDir::new("fp-scene");
    let demo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/library/Demo");
    let e = Engine::builder()
        .ids(reda_engine::command::SeededIds::new(0))
        .user_dirs(None)
        .starter_library(Some(demo))
        .build();
    let mut e = reda_testkit::TestEngine(e);
    e.run_ok("library.element.open", json!({"element": DEMO_FOOTPRINT}));
    (e, dir)
}

#[test]
fn pads_drawing_and_texts_are_tagged_with_what_the_editor_selects() {
    let (e, _dir) = open_demo();
    let Some(LibraryElement::Footprint(footprint)) = e.element(DEMO_FOOTPRINT.parse().unwrap())
    else {
        panic!("the demo footprint is open");
    };
    let drawn = footprint_scene(footprint, e.fonts());
    assert_eq!(drawn.layers.len(), 15, "the fixed v0.1 Layers");
    let scene = &drawn.scene;
    for pad in &footprint.pads {
        let mut items = scene.items_of(pad.uuid);
        assert!(
            items.any(|i| i.color
                == ColorKey::Board(BoardColor::layer(
                    reda_engine::model::LayerKind::Copper,
                    Some(reda_engine::model::Side::Top)
                ))),
            "pad {} has top copper",
            pad.number
        );
    }
    for graphic in &footprint.graphics {
        assert!(
            scene.items_of(graphic.uuid).next().is_some(),
            "graphic drawn"
        );
    }
    assert!(!footprint.texts.is_empty());
    for text in &footprint.texts {
        assert_eq!(
            scene.items_of(text.uuid).next().is_some(),
            text.visible,
            "the {} text is drawn when visible (the Designator as REF**)",
            text.field
        );
    }
    assert!(
        scene
            .items
            .iter()
            .all(|i| matches!(i.layer, SceneLayer::Board(_) | SceneLayer::Drill))
    );
}

/// The items of `pad` on the `kind` Layer of the top side, as (width, height) in nm.
fn opening_sizes(
    scene: &reda_engine::render::Scene,
    pad: reda_engine::model::Uuid,
    kind: reda_engine::model::LayerKind,
) -> Vec<(i64, i64)> {
    let color = ColorKey::Board(BoardColor::layer(kind, Some(reda_engine::model::Side::Top)));
    scene
        .items_of(pad)
        .filter(|i| i.color == color)
        .map(|i| match &i.primitive {
            reda_engine::render::Primitive::Rect { size, .. } => (size.0.0, size.1.0),
            other => panic!("a rectangular opening, not {other:?}"),
        })
        .collect()
}

#[test]
fn mask_and_paste_openings_follow_the_generic_fab_preset_and_pad_overrides() {
    use reda_engine::model::geom::{Nm, Point};
    use reda_engine::model::{LayerKind, MaskSide, Paste, PasteWindow, Tenting};
    let (e, _dir) = open_demo();
    let Some(LibraryElement::Footprint(footprint)) = e.element(DEMO_FOOTPRINT.parse().unwrap())
    else {
        panic!("the demo footprint is open");
    };
    let mut footprint = footprint.clone();
    let template = footprint
        .pads
        .iter()
        .find(|p| p.kind == reda_engine::model::PadKind::Smd)
        .expect("an SMD pad")
        .clone();
    let mut plain = template.clone();
    plain.uuid = "00000000-0000-4000-8000-00000000a001".parse().unwrap();
    plain.shape = reda_engine::model::PadShape::Rect;
    plain.corner_ratio = None;
    plain.width = Nm(1_000_000);
    plain.height = Nm(2_000_000);
    plain.mask = None;
    plain.paste = None;
    plain.side = None;
    let mut tented = plain.clone();
    tented.uuid = "00000000-0000-4000-8000-00000000a002".parse().unwrap();
    tented.mask = Some(reda_engine::model::Mask {
        top: Some(MaskSide::Mode(Tenting::Tent)),
        ..Default::default()
    });
    let mut windows = plain.clone();
    windows.uuid = "00000000-0000-4000-8000-00000000a003".parse().unwrap();
    windows.paste = Some(Paste::Windows {
        windows: vec![
            PasteWindow {
                at: Point::new(Nm(0), Nm(-400_000)),
                width: Nm(800_000),
                height: Nm(600_000),
                corner_ratio: None,
            },
            PasteWindow {
                at: Point::new(Nm(0), Nm(400_000)),
                width: Nm(800_000),
                height: Nm(600_000),
                corner_ratio: None,
            },
        ],
    });
    footprint.pads = vec![plain.clone(), tented.clone(), windows.clone()];
    let scene = footprint_scene(&footprint, e.fonts()).scene;

    // The Generic Fab Preset seeds a 0.05 mm mask expansion and no paste change.
    assert_eq!(
        opening_sizes(&scene, plain.uuid, LayerKind::SolderMask),
        [(1_100_000, 2_100_000)]
    );
    assert_eq!(
        opening_sizes(&scene, plain.uuid, LayerKind::Paste),
        [(1_000_000, 2_000_000)]
    );
    // A tented side opens nothing; paste windows replace the opening.
    assert!(opening_sizes(&scene, tented.uuid, LayerKind::SolderMask).is_empty());
    assert_eq!(
        opening_sizes(&scene, windows.uuid, LayerKind::Paste),
        [(800_000, 600_000), (800_000, 600_000)]
    );
}
