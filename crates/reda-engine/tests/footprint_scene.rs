//! The Footprint editor's Scene: an open Footprint drawn on the board Layers, each item
//! tagged with the Pad, graphic or text the editor selects.

use reda_engine::footprint_scene::{DESIGNATOR_TEXT, VALUE_TEXT};
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
    assert!(
        scene.items_of(DESIGNATOR_TEXT).next().is_some(),
        "the Designator shows as REF**"
    );
    assert_eq!(
        scene.items_of(VALUE_TEXT).next().is_some(),
        footprint.value.visible
    );
    assert!(
        scene
            .items
            .iter()
            .all(|i| matches!(i.layer, SceneLayer::Board(_) | SceneLayer::Drill))
    );
}
