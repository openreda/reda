//! The canvas's GPU side: it re-records its offscreen frame only when something it shows
//! changed, and otherwise reuses the last frame. Skips (with a message) without an adapter.

use std::sync::Arc;

use reda_ui::canvas::{CanvasFrame, CanvasResources, LiveHandle, SceneHandle, SelectionHandle};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Millideg, Nm, Point};
use reda_ui::engine::render::gpu::{GridSpec, Headless, wgpu};
use reda_ui::engine::render::{
    ColorKey, Item, Primitive, Rgba, Scene, SceneLayer, SceneSource, SchematicColor, Theme, View,
};

fn headless() -> Option<Headless> {
    Headless::new().map_err(|e| eprintln!("skipping: {e}")).ok()
}

fn paper() -> Scene {
    let mut scene = Scene::new(SceneSource::None);
    scene.items.push(Item {
        object: Uuid::from_u128(1),
        layer: SceneLayer::Schematic,
        color: ColorKey::Schematic(SchematicColor::Paper),
        primitive: Primitive::Rect {
            center: Point::new(Nm(50_000_000), Nm(50_000_000)),
            size: (Nm(100_000_000), Nm(100_000_000)),
            rotation: Millideg(0),
            corner_radius: Nm(0),
            stroke: None,
        },
    });
    scene
}

fn frame(generation: u64, scene: &Arc<Scene>) -> CanvasFrame {
    CanvasFrame {
        canvas: egui::Id::NULL.with("sheet"),
        scene: SceneHandle {
            generation,
            scene: scene.clone(),
            layers: None,
        },
        theme: Theme::Dark,
        view: View {
            center: (50_000_000.0, 50_000_000.0),
            nm_per_px: 1_000_000.0,
            width: 120,
            height: 80,
            mirrored: false,
        },
        palette: Default::default(),
        grid: Some(GridSpec {
            origin: (0.0, 0.0),
            pitch: 1_270_000.0,
            radius_px: 1.0,
            color: Rgba([255, 255, 255, 128]),
        }),
        selection: SelectionHandle::default(),
        live: LiveHandle::default(),
        moving_offset: (0.0, 0.0),
    }
}

/// Runs one prepare like the paint callback does; returns whether it re-recorded.
fn prepare(gpu: &Headless, res: &mut CanvasResources, frame: &CanvasFrame) -> bool {
    let mut encoder = gpu.device().create_command_encoder(&Default::default());
    let recorded = res.prepare(gpu.device(), gpu.queue(), &mut encoder, frame);
    gpu.queue().submit([encoder.finish()]);
    recorded
}

#[test]
fn an_unchanged_canvas_reuses_its_last_frame() {
    let Some(gpu) = headless() else { return };
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut res = CanvasResources::new(gpu.device(), format, 1);
    let scene = Arc::new(paper());
    let first = frame(1, &scene);

    assert!(prepare(&gpu, &mut res, &first), "the first frame records");
    let writes = res.canvas(first.canvas).unwrap().writes();
    assert!(!prepare(&gpu, &mut res, &first), "nothing changed");
    assert!(!prepare(&gpu, &mut res, &first.clone()), "nothing changed");
    // Reusing the frame writes nothing at all, not even the view.
    assert_eq!(res.canvas(first.canvas).unwrap().writes(), writes);
    assert_eq!(res.canvas(first.canvas).unwrap().recorded(), 1);
}

#[test]
fn a_canvas_records_again_when_what_it_shows_changes() {
    let Some(gpu) = headless() else { return };
    let mut res = CanvasResources::new(gpu.device(), wgpu::TextureFormat::Rgba8Unorm, 1);
    let scene = Arc::new(paper());
    let base = frame(1, &scene);
    prepare(&gpu, &mut res, &base);

    let mut panned = base.clone();
    panned.view.center.0 += 3_000_000.0;
    let mut resized = base.clone();
    resized.view.width = 200;
    let mut light = base.clone();
    light.theme = Theme::Light;
    let mut no_grid = base.clone();
    no_grid.grid = None;
    let edited = frame(2, &Arc::new(paper()));

    let mut last = base.clone();
    for (what, next) in [
        ("pan", panned),
        ("resize", resized),
        ("theme", light),
        ("grid", no_grid),
        ("Document edit", edited),
    ] {
        assert!(prepare(&gpu, &mut res, &next), "{what} should re-record");
        assert!(!prepare(&gpu, &mut res, &next), "{what} twice should not");
        last = next;
    }
    let canvas = res.canvas(last.canvas).unwrap();
    assert_eq!(canvas.recorded(), 6);
    // The geometry went up twice: once at first, once for the edited Document.
    assert_eq!(canvas.writes().0.geometry, 2);
}

#[test]
fn selecting_objects_rewrites_only_the_flags() {
    let Some(gpu) = headless() else { return };
    let mut res = CanvasResources::new(gpu.device(), wgpu::TextureFormat::Rgba8Unorm, 1);
    let scene = Arc::new(paper());
    let base = frame(1, &scene);
    prepare(&gpu, &mut res, &base);
    let before = res.canvas(base.canvas).unwrap().writes().0;
    assert_eq!(before.flags, 0, "an empty selection writes no flags");

    let mut selected = base.clone();
    selected.selection = SelectionHandle {
        generation: 7,
        objects: Arc::from([Uuid::from_u128(1)]),
        ..SelectionHandle::default()
    };
    assert!(prepare(&gpu, &mut res, &selected), "a selection re-records");
    assert!(!prepare(&gpu, &mut res, &selected));
    let after = res.canvas(base.canvas).unwrap().writes().0;
    assert_eq!(after.flags, 1);
    assert_eq!(after.geometry, before.geometry);
}
