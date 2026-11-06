//! The wgpu backend, driven headless: what it draws, and what it uploads when.
//!
//! Every test skips (with a message) when the machine has no GPU or software adapter.

use reda_model::Uuid;
use reda_model::geom::{Millideg, Nm, Point};
use reda_render::gpu::{
    Canvas, Flags, GpuScene, Grid, GridSpec, Headless, Highlight, Image, RenderError, Writes,
};
use reda_render::{
    Align, BoardColor, ColorKey, Composite, GlyphRun, Palette, Primitive, Rgba, Scene, SceneLayer,
    SceneSource, SchematicColor, Theme, View,
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
        child: None,
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
fn moving_objects_are_drawn_at_the_offset_and_hidden_ones_not_at_all() {
    let Some(gpu) = headless() else { return };
    let scene = square_on_paper();
    let palette = Palette::shipped(Theme::Dark);
    let view = view_of(&scene, 100);
    let wire = palette.color(ColorKey::Schematic(SchematicColor::Wire)).0;
    let paper = palette.color(ColorKey::Schematic(SchematicColor::Paper)).0;
    let mut gs = GpuScene::new(gpu.device(), &scene);
    let mut cv = Canvas::new(gpu.device(), gpu.renderer());
    cv.set_palette(gpu.queue(), &palette);
    gs.set_highlight(gpu.queue(), Highlight::new().mark(id(2), Flags::MOVING));

    // 30 mm right and 20 mm up: the square's centre goes from (50, 50) px to (80, 30) px.
    cv.set_moving_offset((30_000_000.0, 20_000_000.0));
    let img = gpu.render_frame(&gs, &mut cv, &view).unwrap();
    assert_eq!(pixel(&img, 50, 50), paper);
    assert_eq!(pixel(&img, 80, 30), wire);
    // A new offset is only a view write.
    let flags = gs.writes().flags;
    cv.set_moving_offset((0.0, 0.0));
    let img = gpu.render_frame(&gs, &mut cv, &view).unwrap();
    assert_eq!(pixel(&img, 50, 50), wire);
    assert_eq!(gs.writes().flags, flags);

    gs.set_highlight(gpu.queue(), Highlight::new().mark(id(2), Flags::HIDDEN));
    let img = gpu.render_frame(&gs, &mut cv, &view).unwrap();
    assert_eq!(pixel(&img, 50, 50), paper);
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
fn highlighting_objects_another_scene_draws_dims_nothing_here() {
    let Some(gpu) = headless() else { return };
    let scene = square_on_paper();
    let palette = Palette::shipped(Theme::Dark);
    let view = view_of(&scene, 100);
    let plain = gpu
        .render(&scene, &view, &palette, &Highlight::new())
        .unwrap();
    // A cross-probe marks the other Document's objects on every canvas.
    let elsewhere = gpu
        .render(
            &scene,
            &view,
            &palette,
            Highlight::new().mark(id(99), Flags::HIGHLIGHTED),
        )
        .unwrap();
    assert_eq!(pixel(&elsewhere, 50, 50), pixel(&plain, 50, 50));
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

#[test]
fn the_grid_draws_dots_on_grid_points_over_the_scene() {
    let Some(gpu) = headless() else { return };
    // Paper only: 100 mm square filling a 100 px image, so 1 px = 1 mm.
    let mut scene = Scene::new(SceneSource::None);
    let c = Point::new(mm(50), mm(50));
    push(
        &mut scene,
        id(1),
        SchematicColor::Paper,
        filled_rect(c, 100, 100),
    );
    let palette = Palette::shipped(Theme::Light);
    let gs = GpuScene::new(gpu.device(), &scene);
    let mut cv = Canvas::new(gpu.device(), gpu.renderer());
    cv.set_palette(gpu.queue(), &palette);
    let view = view_of(&scene, 100);
    let mut grid = Grid::new(gpu.device(), gpu.renderer());
    let spec = GridSpec {
        origin: (0.0, 0.0),
        pitch: 10.0 * 1_000_000.0,
        radius_px: 1.5,
        color: Rgba([0, 0, 0, 255]),
    };
    grid.set(gpu.queue(), &view, Some(&spec));
    let img = gpu
        .render_frame_with(&gs, &mut cv, &view, |pass| grid.draw(pass))
        .unwrap();
    let paper = rgb(palette.color(ColorKey::Schematic(SchematicColor::Paper)));
    // Document (20 mm, 30 mm) is a grid point: pixel (20, 70) from the top left. The
    // pixel centre sits half a pixel off it, well inside the 1.5 px dot.
    assert_eq!(pixel(&img, 20, 70), [0, 0, 0, 255]);
    assert_eq!(pixel(&img, 19, 69), [0, 0, 0, 255]);
    // Midway between dots is untouched paper.
    assert_eq!(pixel(&img, 25, 65), paper);

    // Hidden, the same frame is plain paper.
    grid.set(gpu.queue(), &view, None);
    let img = gpu
        .render_frame_with(&gs, &mut cv, &view, |pass| grid.draw(pass))
        .unwrap();
    assert_eq!(pixel(&img, 20, 70), paper);
}

fn push_board(scene: &mut Scene, layer: u128, color: BoardColor, primitive: Primitive) {
    scene.items.push(reda_render::Item {
        child: None,
        object: id(100 + layer),
        layer: SceneLayer::Board(id(layer)),
        color: ColorKey::Board(color),
        primitive,
    });
}

/// How far each channel of `a` is from `b`, at most.
fn distance(a: [u8; 4], b: [u8; 4]) -> u8 {
    (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap()
}

fn mix(under: [u8; 4], over: [u8; 4], opacity: f32) -> [u8; 4] {
    let m = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * opacity).round() as u8;
    [
        m(under[0], over[0]),
        m(under[1], over[1]),
        m(under[2], over[2]),
        255,
    ]
}

#[test]
fn a_layer_is_drawn_opaque_then_blended_at_its_opacity() {
    let Some(gpu) = headless() else { return };
    // Layer 1: a 60 mm bottom copper square. Layer 2: two overlapping top copper squares,
    // composited at half opacity: where they overlap it is no darker than where they don't.
    let mut scene = Scene::new(SceneSource::None);
    let c = |x, y| Point::new(mm(x), mm(y));
    push_board(
        &mut scene,
        1,
        BoardColor::CopperBottom,
        filled_rect(c(50, 50), 60, 60),
    );
    push_board(
        &mut scene,
        2,
        BoardColor::CopperTop,
        filled_rect(c(40, 50), 30, 30),
    );
    push_board(
        &mut scene,
        2,
        BoardColor::CopperTop,
        filled_rect(c(60, 50), 30, 30),
    );
    push_board(
        &mut scene,
        1,
        BoardColor::CopperBottom,
        filled_rect(c(0, 0), 1, 1),
    );
    push_board(
        &mut scene,
        1,
        BoardColor::CopperBottom,
        filled_rect(c(100, 100), 1, 1),
    );
    let palette = Palette::shipped(Theme::Dark);
    let layers = [
        Composite::opaque(SceneLayer::Board(id(1))),
        Composite {
            layer: SceneLayer::Board(id(2)),
            opacity: 0.5,
        },
    ];
    let view = view_of(&scene, 100);
    let img = gpu
        .render_layers(&scene, &view, &palette, &Highlight::new(), Some(&layers))
        .unwrap();
    let bottom = rgb(palette.color(ColorKey::Board(BoardColor::CopperBottom)));
    let top = rgb(palette.color(ColorKey::Board(BoardColor::CopperTop)));
    let blended = mix(bottom, top, 0.5);
    // Inside only the left square, inside both, and inside only the right one.
    for x in [33, 50, 67] {
        let p = pixel(&img, x, 50);
        assert!(distance(p, blended) <= 3, "x={x}: {p:?} vs {blended:?}");
    }
    // The bottom copper alone, and the background around it.
    assert!(distance(pixel(&img, 50, 25), bottom) <= 1);
    assert!(distance(pixel(&img, 10, 50), rgb(palette.background())) <= 1);

    // Swapping the order draws the bottom copper on top, opaque.
    let swapped = [layers[1], layers[0]];
    let img = gpu
        .render_layers(&scene, &view, &palette, &Highlight::new(), Some(&swapped))
        .unwrap();
    assert!(distance(pixel(&img, 50, 50), bottom) <= 1);
    // A Layer left out is not drawn.
    let img = gpu
        .render_layers(
            &scene,
            &view,
            &palette,
            &Highlight::new(),
            Some(&layers[1..]),
        )
        .unwrap();
    let over_bg = mix(rgb(palette.background()), top, 0.5);
    assert!(distance(pixel(&img, 50, 50), over_bg) <= 3);
    assert!(distance(pixel(&img, 50, 25), rgb(palette.background())) <= 1);
}

#[test]
fn view_only_labels_hide_below_their_minimum_size_on_screen() {
    let Some(gpu) = headless() else { return };
    // A 1 mm high Pad number in the middle of a 100 mm square of bottom copper.
    let mut scene = Scene::new(SceneSource::None);
    let c = Point::new(mm(50), mm(50));
    push_board(
        &mut scene,
        1,
        BoardColor::CopperBottom,
        filled_rect(c, 100, 100),
    );
    let label = GlyphRun {
        text: "1".into(),
        at: c,
        height: mm(1),
        rotation: Millideg(0),
        align: Align::default(),
        view_label: true,
        strokes: vec![reda_render::Segment::new(
            Point::new(mm(50), mm(49)),
            Point::new(mm(50), mm(51)),
        )],
        stroke_width: mm(1),
        fill: Default::default(),
    };
    push_board(
        &mut scene,
        1,
        BoardColor::PadNumber,
        Primitive::Glyphs(label),
    );
    let palette = Palette::shipped(Theme::Dark);
    let white = rgb(palette.color(ColorKey::Board(BoardColor::PadNumber)));
    let copper = rgb(palette.color(ColorKey::Board(BoardColor::CopperBottom)));
    // Fitted, 1 mm is 1 px: hidden.
    let img = gpu
        .render(&scene, &view_of(&scene, 100), &palette, &Highlight::new())
        .unwrap();
    assert_eq!(pixel(&img, 50, 50), copper);
    // At 10 px per mm it shows.
    let zoomed = View::fit_into(
        Point::new(mm(45), mm(45)),
        Point::new(mm(55), mm(55)),
        100,
        100,
        0.0,
    );
    let img = gpu
        .render(&scene, &zoomed, &palette, &Highlight::new())
        .unwrap();
    assert_eq!(pixel(&img, 50, 50), white);
}
