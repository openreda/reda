//! ELLIPSE, SPLINE, TEXT, MTEXT, INSERT, R12 POLYLINE and flipped extrusions, against the
//! hand-written fixtures in `tests/fixtures/` (docs/spec/v0.3/dxf.md#conversion). The
//! expected shapes are worked out by hand from the DXF Reference.

use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};
use std::path::Path;

use reda_import_dxf::{Drawing, Piece, Shape, Text, Vertex, Xy, read};
use reda_model::{TextHJustify as H, TextJustify, TextVJustify as V};

/// 0.005 mm, in a drawing in mm.
const TOLERANCE: f64 = 0.005;

fn fixture(name: &str) -> Drawing {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    read(&std::fs::read(&path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn xy(x: f64, y: f64) -> Xy {
    Xy { x, y }
}

fn counts(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
}

#[track_caller]
fn near(actual: Xy, expected: Xy) {
    assert!(
        (actual.x - expected.x).abs() < 1e-9 && (actual.y - expected.y).abs() < 1e-9,
        "{actual:?} is not {expected:?}"
    );
}

#[track_caller]
fn near_f(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{actual} is not {expected}"
    );
}

/// How far `p` is from the nearest piece.
fn distance(pieces: &[Piece], p: Xy) -> f64 {
    let along = |from: Xy, centre: Xy, sweep: f64, p: Xy| {
        let a0 = (from.y - centre.y).atan2(from.x - centre.x);
        let a = (p.y - centre.y).atan2(p.x - centre.x);
        let d = if sweep >= 0.0 { a - a0 } else { a0 - a };
        d.rem_euclid(TAU) <= sweep.to_radians().abs()
    };
    let dist = |a: Xy, b: Xy| (a.x - b.x).hypot(a.y - b.y);
    pieces
        .iter()
        .map(|piece| match *piece {
            Piece::Line { from, to } => {
                let (dx, dy) = (to.x - from.x, to.y - from.y);
                let s = (((p.x - from.x) * dx + (p.y - from.y) * dy) / (dx * dx + dy * dy))
                    .clamp(0.0, 1.0);
                dist(p, xy(from.x + dx * s, from.y + dy * s))
            }
            Piece::Arc {
                from,
                to,
                sweep,
                centre,
                radius,
            } => {
                if along(from, centre, sweep, p) {
                    (dist(p, centre) - radius).abs()
                } else {
                    dist(p, from).min(dist(p, to))
                }
            }
        })
        .fold(f64::INFINITY, f64::min)
}

/// Every one of 2,000 samples of `curve` over [t0, t1] is within the tolerance of a piece,
/// and the pieces run end to end from the curve's start to its end.
#[track_caller]
fn fits(pieces: &[Piece], curve: impl Fn(f64) -> Xy, t0: f64, t1: f64) {
    for i in 0..=2000 {
        let p = curve(t0 + (t1 - t0) * i as f64 / 2000.0);
        let d = distance(pieces, p);
        assert!(d <= TOLERANCE, "{p:?} is {d} from the fit");
    }
    let ends = |p: &Piece| match *p {
        Piece::Line { from, to } | Piece::Arc { from, to, .. } => (from, to),
    };
    for pair in pieces.windows(2) {
        assert_eq!(ends(&pair[0]).1, ends(&pair[1]).0, "a gap between pieces");
    }
    near(ends(&pieces[0]).0, curve(t0));
    near(ends(pieces.last().unwrap()).1, curve(t1));
}

#[test]
fn ellipses_are_fitted_with_arcs_within_the_tolerance() {
    let drawing = fixture("ellipse.dxf");
    let full = &drawing.entities[0];
    assert_eq!(full.layer, "Ellipses");
    // Major axis (4, 0), ratio 0.5: the minor axis is (0, 2), a quarter turn
    // counter-clockwise for the extrusion +Z.
    let Shape::Ellipse {
        centre,
        u,
        v,
        start,
        sweep,
    } = full.shape
    else {
        panic!("not an ellipse: {:?}", full.shape)
    };
    assert_eq!((centre, u, start), (xy(10.0, 5.0), xy(4.0, 0.0), 0.0));
    near(v, xy(0.0, 2.0));
    near_f(sweep, TAU);
    let pieces = full.shape.pieces(TOLERANCE);
    assert!(
        pieces.iter().all(|p| matches!(p, Piece::Arc { .. })),
        "{pieces:?}"
    );
    fits(
        &pieces,
        |t| xy(10.0 + 4.0 * t.cos(), 5.0 + 2.0 * t.sin()),
        0.0,
        TAU,
    );

    // Standing up, ratio 0.25, half way round: from the top, through the left, to the
    // bottom.
    let half = drawing.entities[1].shape.pieces(TOLERANCE);
    fits(&half, |t| xy(-0.75 * t.sin(), 3.0 * t.cos()), 0.0, PI);
}

#[test]
fn a_circular_ellipse_is_one_true_arc() {
    let drawing = fixture("ellipse.dxf");
    let circle = &drawing.entities[2];
    // Major axis (1, 1), ratio 1, a quarter turn from it: 45° to 135°, radius √2.
    let Shape::Arc {
        centre,
        radius,
        start,
        end,
    } = circle.shape
    else {
        panic!("not an arc: {:?}", circle.shape)
    };
    assert_eq!(centre, xy(2.0, 2.0));
    near_f(radius, 2f64.sqrt());
    near_f(start, 45.0);
    near_f(end, 135.0);
    let pieces = circle.shape.pieces(TOLERANCE);
    assert_eq!(pieces.len(), 1, "{pieces:?}");
    let Piece::Arc { sweep, .. } = pieces[0] else {
        panic!("not an arc")
    };
    near_f(sweep, 90.0);
}

#[test]
fn a_flipped_ellipse_turns_clockwise_seen_from_above() {
    let drawing = fixture("ellipse.dxf");
    // Extrusion (0, 0, -1): the minor axis is N × major = (0, -1), so the quarter from
    // the major axis's end goes down, to (5, -1).
    let flipped = &drawing.entities[3];
    assert_eq!(flipped.layer, "Flipped");
    let pieces = flipped.shape.pieces(TOLERANCE);
    fits(
        &pieces,
        |t| xy(5.0 + 2.0 * t.cos(), -t.sin()),
        0.0,
        PI / 2.0,
    );
}

#[test]
fn a_flipped_extrusion_mirrors_arcs_circles_and_polylines_in_x() {
    let drawing = fixture("extrusion.dxf");
    // ARC at OCS (10, 0), r 2, 0° to 90°: the world sees the centre at (-10, 0) and the arc
    // from 90° to 180°.
    let Shape::Arc {
        centre,
        radius,
        start,
        end,
    } = drawing.entities[0].shape
    else {
        panic!("not an arc: {:?}", drawing.entities[0].shape)
    };
    near(centre, xy(-10.0, 0.0));
    near_f(radius, 2.0);
    near_f(start, 90.0);
    near_f(end, 180.0);
    assert_eq!(
        drawing.entities[1].shape,
        Shape::Circle {
            centre: xy(-3.0, 4.0),
            radius: 1.0
        }
    );
    // The bulge turns the other way.
    assert_eq!(
        drawing.entities[2].shape,
        Shape::Polyline {
            vertices: vec![
                Vertex {
                    at: xy(0.0, 0.0),
                    bulge: -1.0
                },
                Vertex {
                    at: xy(-2.0, 0.0),
                    bulge: 0.0
                }
            ],
            closed: false
        }
    );
    // Seen from below, the half circle bulged down from (0, 0) to (2, 0); from above it
    // still bulges down, now towards (-1, -1).
    let pieces = drawing.entities[2].shape.pieces(TOLERANCE);
    let Piece::Arc { sweep, centre, .. } = pieces[0] else {
        panic!("not an arc")
    };
    near_f(sweep, -180.0);
    near(centre, xy(-1.0, 0.0));
    assert!(distance(&pieces, xy(-1.0, -1.0)) < 1e-9);
}

#[test]
fn splines_are_fitted_with_arcs_within_the_tolerance() {
    let drawing = fixture("spline.dxf");
    assert_eq!(drawing.entities.len(), 3);
    // A quadratic Bézier: (1-t)² P0 + 2t(1-t) P1 + t² P2.
    let parabola = drawing.entities[0].shape.pieces(TOLERANCE);
    fits(&parabola, |t| xy(10.0 * t, 20.0 * t * (1.0 - t)), 0.0, 1.0);

    // The rational quarter circle is a circle, so its one arc is exact.
    let quarter = drawing.entities[1].shape.pieces(TOLERANCE);
    assert_eq!(quarter.len(), 1, "{quarter:?}");
    let Piece::Arc {
        from,
        to,
        sweep,
        centre,
        radius,
    } = quarter[0]
    else {
        panic!("not an arc")
    };
    near(from, xy(10.0, 0.0));
    near(to, xy(0.0, 10.0));
    near_f(sweep, 90.0);
    near(centre, xy(0.0, 0.0));
    near_f(radius, 10.0);
}

#[test]
fn a_spline_of_fit_points_passes_through_each_and_leaves_along_its_tangent() {
    let drawing = fixture("spline.dxf");
    assert_eq!(drawing.entities[2].layer, "Fit");
    let pieces = drawing.entities[2].shape.pieces(TOLERANCE);
    for p in [xy(0.0, 0.0), xy(10.0, 5.0), xy(20.0, 0.0), xy(30.0, 5.0)] {
        assert!(distance(&pieces, p) <= TOLERANCE, "{p:?}");
    }
    // Start tangent (1, 0): the first arc leaves the origin flat, so its centre is right
    // above or below it. End tangent (0, -1): the last arc comes straight down into
    // (30, 5), its centre level with it.
    let Piece::Arc {
        from,
        centre,
        radius,
        ..
    } = pieces[0]
    else {
        panic!("not an arc: {:?}", pieces[0])
    };
    assert_eq!(from, xy(0.0, 0.0));
    assert!((centre.x - from.x).abs() < 0.05 * radius, "{centre:?}");
    let Piece::Arc {
        from,
        to,
        centre,
        radius,
        ..
    } = *pieces.last().unwrap()
    else {
        panic!("not an arc")
    };
    assert_eq!(to, xy(30.0, 5.0));
    assert!((centre.y - to.y).abs() < 0.05 * radius, "{centre:?}");
    assert!(from.y > to.y, "{from:?}");
}

#[test]
fn a_spline_with_bad_knots_is_skipped_and_counted() {
    let drawing = fixture("spline.dxf");
    assert_eq!(drawing.skipped, counts(&[("malformed SPLINE", 1)]));
}

fn texts(drawing: &Drawing) -> Vec<&Text> {
    drawing
        .entities
        .iter()
        .filter_map(|e| match &e.shape {
            Shape::Text(t) => Some(t),
            _ => None,
        })
        .collect()
}

#[track_caller]
fn assert_text(t: &Text, text: &str, at: Xy, height: f64, rotation: f64, h: H, v: V) {
    assert_eq!(t.text, text);
    near(t.at, at);
    near_f(t.height, height);
    near_f(t.rotation, rotation);
    assert_eq!(t.justify, TextJustify { h, v }, "{text}");
}

#[test]
fn text_keeps_its_height_rotation_and_justification() {
    let drawing = fixture("text.dxf");
    assert!(drawing.entities.iter().all(|e| e.layer == "Notes"));
    let t = texts(&drawing);
    assert_eq!(t.len(), 6);
    // Left and baseline: the first alignment point.
    assert_text(t[0], "Hello °", xy(1.0, 2.0), 2.5, 30.0, H::Left, V::Bottom);
    // Otherwise the second.
    assert_text(
        t[1],
        "Centre top",
        xy(5.0, 5.0),
        1.0,
        0.0,
        H::Center,
        V::Top,
    );
    assert_text(t[2], "Middle", xy(7.0, 8.0), 1.0, 0.0, H::Center, V::Center);
    // Flipped: at (-4, 0), mirrored; it reads normally over the same place, running left
    // from its anchor.
    assert_text(
        t[3],
        "Flipped",
        xy(-4.0, 0.0),
        1.0,
        0.0,
        H::Right,
        V::Bottom,
    );
    assert_eq!(drawing.skipped, counts(&[("empty TEXT", 1)]));
}

#[test]
fn mtext_codes_are_stripped_and_its_breaks_kept() {
    let drawing = fixture("text.dxf");
    let t = texts(&drawing);
    // Chunks in 3 before the 1; attachment 5 is middle centre; 50 is in radians.
    assert_text(
        t[4],
        "First line\nSecond line",
        xy(10.0, 10.0),
        3.0,
        90.0,
        H::Center,
        V::Center,
    );
    // The direction 11/21/31 sets the rotation; attachment 7 is bottom left.
    assert_text(
        t[5],
        "Up\nwards",
        xy(20.0, 0.0),
        2.0,
        90.0,
        H::Left,
        V::Bottom,
    );
}

#[test]
fn r12_polylines_read_their_vertices_and_skip_meshes() {
    let drawing = fixture("polyline_r12.dxf");
    let v = |x, y, bulge| Vertex {
        at: xy(x, y),
        bulge,
    };
    let shapes: Vec<(&str, &Shape)> = drawing
        .entities
        .iter()
        .map(|e| (e.layer.as_str(), &e.shape))
        .collect();
    assert_eq!(
        shapes,
        [
            (
                "Outline",
                &Shape::Polyline {
                    vertices: vec![
                        v(0.0, 0.0, 0.0),
                        v(10.0, 0.0, 1.0),
                        v(10.0, 10.0, 0.0),
                        v(0.0, 10.0, 0.0)
                    ],
                    closed: true
                }
            ),
            // A 3D polyline, seen from above.
            (
                "Path3d",
                &Shape::Polyline {
                    vertices: vec![v(0.0, 0.0, 0.0), v(3.0, 4.0, 0.0)],
                    closed: false
                }
            ),
            // A spline-fit polyline draws its fitted vertices, not its frame.
            (
                "Smooth",
                &Shape::Polyline {
                    vertices: vec![v(0.0, 0.0, 0.0), v(10.0, 0.0, 0.0)],
                    closed: false
                }
            ),
        ]
    );
    // VERTEX and SEQEND belong to their POLYLINE and are not counted.
    assert_eq!(drawing.skipped, counts(&[("POLYLINE mesh", 1)]));
}

#[test]
fn inserts_explode_nested_blocks_in_place() {
    let drawing = fixture("insert.dxf");
    let got: Vec<(&str, Vec<Xy>)> = drawing
        .entities
        .iter()
        .map(|e| {
            let points = match e.shape {
                Shape::Line { from, to } => vec![from, to],
                Shape::Circle { centre, radius } => vec![centre, xy(radius, 0.0)],
                ref other => panic!("unexpected {other:?}"),
            };
            (e.layer.as_str(), points)
        })
        .collect();
    // Outer at (10, 0) turned 90°, base (1, 1); in it Inner at (1, 1) scaled 2. Entities on
    // layer 0 take the layer of the INSERT that draws them: "Parts", through both.
    let want = [
        ("Parts", vec![xy(10.0, 0.0), xy(10.0, 2.0)]),
        ("Holes", vec![xy(10.0, 0.0), xy(1.0, 0.0)]),
        ("Frame", vec![xy(10.0, 0.0), xy(9.0, 0.0)]),
        // Inner twice, 5 apart.
        ("Array", vec![xy(0.0, 20.0), xy(1.0, 20.0)]),
        ("Holes", vec![xy(0.0, 20.0), xy(0.5, 0.0)]),
        ("Array", vec![xy(5.0, 20.0), xy(6.0, 20.0)]),
        ("Holes", vec![xy(5.0, 20.0), xy(0.5, 0.0)]),
    ];
    assert_eq!(got.len(), want.len(), "{got:?}");
    for ((layer, points), (want_layer, want_points)) in got.iter().zip(&want) {
        assert_eq!(layer, want_layer);
        for (p, q) in points.iter().zip(want_points) {
            near(*p, *q);
        }
    }
    assert_eq!(
        drawing.skipped,
        counts(&[
            ("INSERT of a block inside itself", 1),
            ("INSERT of a missing block", 1)
        ])
    );
}
