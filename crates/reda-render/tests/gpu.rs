//! The wgpu backend, driven headless: what it draws, and what it uploads when.
//!
//! Every test skips (with a message) when the machine has no GPU or software adapter.

use reda_model::Uuid;
use reda_model::geom::{Millideg, Nm, Point};
use reda_render::gpu::{Canvas, Flags, GpuScene, Headless, Highlight, Image, RenderError, Writes};
use reda_render::{
    ColorKey, Palette, Primitive, Rgba, Scene, SceneLayer, SceneSource, SchematicColor, Theme, View,
};

fn headless() -> Option<Headless> {
    match Headless::new() {
        Ok(h) => {
            eprintln!("adapter: {:?}", h.adapter());
            Some(h)
        }
        Err(e) => {
            eprintln!("skipping: {e}");
            None
        }
    }
}

fn mm(v: i64) -> Nm {
    Nm(v * 1_000_000)
}

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn push(scene: &mut Scene, object: Uuid, color: SchematicColor, primitive: Primitive) {
    scene.items.push(reda_render::Item {
        object,
        layer: SceneLayer::Schematic,
        color: ColorKey::Schematic(color),
        primitive,
    });
}

fn filled_rect(center: Point, w: i64, h: i64) -> Primitive {
    Primitive::Rect {
        center,
        size: (mm(w), mm(h)),
        rotation: Millideg(0),
        corner_radius: Nm(0),
        stroke: None,
    }
}

/// A 100 × 100 mm paper with a 20 × 20 mm Wire-coloured square at its centre.
fn square_on_paper() -> Scene {
    let mut s = Scene::new(SceneSource::None);
    let c = Point::new(mm(50), mm(50));
    push(
        &mut s,
        id(1),
        SchematicColor::Paper,
        filled_rect(c, 100, 100),
    );
    push(&mut s, id(2), SchematicColor::Wire, filled_rect(c, 20, 20));
    s
}

fn view_of(scene: &Scene, px: u32) -> View {
    let (lo, hi) = scene.bounds().unwrap();
    View::fit_into(lo, hi, px, px, 0.0)
}

fn pixel(img: &Image, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width + x) * 4) as usize;
    img.rgba[i..i + 4].try_into().unwrap()
}

fn rgb(c: Rgba) -> [u8; 4] {
    c.0
}

#[test]
fn a_filled_rect_draws_in_its_palette_colour() {
    let Some(gpu) = headless() else { return };
    let scene = square_on_paper();
    let palette = Palette::shipped(Theme::Light);
    let img = gpu
        .render(&scene, &view_of(&scene, 100), &palette, &Highlight::new())
        .unwrap();
    assert_eq!((img.width, img.height), (100, 100));
    assert_eq!(
        pixel(&img, 50, 50),
        rgb(palette.color(ColorKey::Schematic(SchematicColor::Wire)))
    );
    assert_eq!(
        pixel(&img, 10, 10),
        rgb(palette.color(ColorKey::Schematic(SchematicColor::Paper)))
    );
}

#[test]
fn frames_write_only_the_view_and_geometry_uploads_once() {
    let Some(gpu) = headless() else { return };
    let scene = square_on_paper();
    let mut gs = GpuScene::new(gpu.device(), &scene);
    let mut cv = Canvas::new(gpu.device(), gpu.renderer());
    cv.set_palette(gpu.queue(), &Palette::shipped(Theme::Dark));
    let view = view_of(&scene, 64);
    for zoom in [1.0, 2.0, 4.0] {
        let v = View {
            nm_per_px: view.nm_per_px / zoom,
            ..view
        };
        gpu.render_frame(&gs, &mut cv, &v).unwrap();
    }
    assert_eq!(
        gs.writes(),
        Writes {
            geometry: 1,
            ..Writes::default()
        }
    );
    assert_eq!(
        cv.writes(),
        Writes {
            view: 3,
            palette: 1,
            ..Writes::default()
        }
    );

    gs.set_highlight(gpu.queue(), Highlight::new().mark(id(2), Flags::SELECTED));
    gpu.render_frame(&gs, &mut cv, &view).unwrap();
    assert_eq!(
        gs.writes(),
        Writes {
            geometry: 1,
            flags: 1,
            ..Writes::default()
        }
    );

    let mut changed = scene.clone();
    changed.items.pop();
    gs.replace(gpu.device(), &changed);
    assert_eq!(gs.writes().geometry, 2);
    assert_eq!(gs.instance_count(), 1);
}

#[test]
fn a_selected_object_is_lighter_and_haloed_without_new_geometry() {
    let Some(gpu) = headless() else { return };
    let scene = square_on_paper();
    let palette = Palette::shipped(Theme::Dark);
    let view = view_of(&scene, 100);
    let plain = gpu
        .render(&scene, &view, &palette, &Highlight::new())
        .unwrap();
    let selected = gpu
        .render(
            &scene,
            &view,
            &palette,
            Highlight::new().mark(id(2), Flags::SELECTED),
        )
        .unwrap();
    let wire = palette.color(ColorKey::Schematic(SchematicColor::Wire)).0;
    assert_eq!(pixel(&plain, 50, 50), wire);
    let lit = pixel(&selected, 50, 50);
    assert!(
        (0..3).all(|c| lit[c] > wire[c]),
        "{lit:?} is not lighter than {wire:?}"
    );
    // The halo shows just outside the square's edge (x = 40 px), but not further out.
    let paper = pixel(&plain, 38, 50);
    assert_ne!(pixel(&selected, 38, 50), paper);
    assert_eq!(pixel(&selected, 30, 50), paper);
}

#[test]
fn highlighting_dims_everything_else_but_the_paper() {
    let Some(gpu) = headless() else { return };
    let mut scene = square_on_paper();
    let corner = Point::new(mm(10), mm(10));
    push(
        &mut scene,
        id(3),
        SchematicColor::Body,
        filled_rect(corner, 10, 10),
    );
    let palette = Palette::shipped(Theme::Dark);
    let view = view_of(&scene, 100);
    let plain = gpu
        .render(&scene, &view, &palette, &Highlight::new())
        .unwrap();
    let lit = gpu
        .render(
            &scene,
            &view,
            &palette,
            Highlight::new().mark(id(2), Flags::HIGHLIGHTED),
        )
        .unwrap();
    assert_eq!(pixel(&lit, 50, 50), pixel(&plain, 50, 50));
    assert_ne!(
        pixel(&lit, 10, 90),
        pixel(&plain, 10, 90),
        "the other object dims"
    );
    assert_eq!(
        pixel(&lit, 90, 10),
        pixel(&plain, 90, 10),
        "the paper does not"
    );
}

#[test]
fn with_no_adapter_the_error_names_the_cause() {
    let Err(err) = Headless::with_backends(wgpu::Backends::empty()) else {
        panic!("an instance with no backends found an adapter");
    };
    assert!(matches!(err, RenderError::NoAdapter(_)), "{err:?}");
    let message = err.to_string();
    assert!(message.contains("no GPU or software renderer"), "{message}");
    assert!(message.contains("no backends enabled"), "{message}");
}
