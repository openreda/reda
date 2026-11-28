//! The DXF reader against the hand-written fixtures in `tests/fixtures/`, each written from
//! Autodesk's DXF Reference: one file per entity it reads, a file with no `$INSUNITS`, one
//! in inches, and one full of entities it skips.

use std::collections::BTreeMap;
use std::path::Path;

use reda_import_dxf::{Drawing, Piece, Shape, Vertex, Xy, read};

fn fixture(name: &str) -> Drawing {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    read(&std::fs::read(&path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn xy(x: f64, y: f64) -> Xy {
    Xy { x, y }
}

#[track_caller]
fn assert_near(actual: Xy, expected: Xy) {
    assert!(
        (actual.x - expected.x).abs() < 1e-9 && (actual.y - expected.y).abs() < 1e-9,
        "{actual:?} is not {expected:?}"
    );
}

#[test]
fn a_line_reads_its_layer_and_both_ends() {
    let drawing = fixture("line.dxf");
    assert_eq!(drawing.insunits, Some(4));
    assert_eq!(drawing.entities.len(), 1);
    let line = &drawing.entities[0];
    assert_eq!(line.layer, "Sketch");
    assert_eq!(
        line.shape,
        Shape::Line {
            from: xy(10.0, -5.5),
            to: xy(40.25, 15.0)
        }
    );
    assert!(drawing.skipped.is_empty());
}

#[test]
fn an_arc_reads_its_centre_radius_and_angles_in_degrees() {
    let drawing = fixture("arc.dxf");
    let shapes: Vec<_> = drawing.entities.iter().map(|e| e.shape.clone()).collect();
    assert_eq!(
        shapes,
        [
            Shape::Arc {
                centre: xy(5.0, 5.0),
                radius: 2.0,
                start: 0.0,
                end: 90.0
            },
            Shape::Arc {
                centre: xy(0.0, 0.0),
                radius: 1.0,
                start: 300.0,
                end: 60.0
            }
        ]
    );
}

#[test]
fn an_arc_sweeps_counter_clockwise_from_start_to_end_even_across_zero() {
    let drawing = fixture("arc.dxf");
    let Piece::Arc {
        from, to, sweep, ..
    } = drawing.entities[0].shape.pieces()[0]
    else {
        panic!("not an arc")
    };
    assert_near(from, xy(7.0, 5.0));
    assert_near(to, xy(5.0, 7.0));
    assert!((sweep - 90.0).abs() < 1e-9);
    // 300° to 60° goes through 0°: 120°, not -240°.
    let Piece::Arc { sweep, from, .. } = drawing.entities[1].shape.pieces()[0] else {
        panic!("not an arc")
    };
    assert!((sweep - 120.0).abs() < 1e-9, "{sweep}");
    assert_near(from, xy(0.5, -(3f64.sqrt()) / 2.0));
}

#[test]
fn a_circle_becomes_two_half_arcs() {
    let drawing = fixture("circle.dxf");
    assert_eq!(drawing.entities[0].layer, "Holes");
    assert_eq!(
        drawing.entities[0].shape,
        Shape::Circle {
            centre: xy(3.0, 4.0),
            radius: 1.5
        }
    );
    let pieces = drawing.entities[0].shape.pieces();
    assert_eq!(pieces.len(), 2);
    for (piece, (a, b)) in pieces
        .iter()
        .zip([((4.5, 4.0), (1.5, 4.0)), ((1.5, 4.0), (4.5, 4.0))])
    {
        let Piece::Arc {
            from,
            to,
            sweep,
            centre,
            radius,
        } = *piece
        else {
            panic!("not an arc")
        };
        assert_near(from, xy(a.0, a.1));
        assert_near(to, xy(b.0, b.1));
        assert_eq!((sweep, centre, radius), (180.0, xy(3.0, 4.0), 1.5));
    }
}

#[test]
fn an_lwpolyline_reads_its_vertices_bulges_and_closed_flag() {
    let drawing = fixture("lwpolyline.dxf");
    let v = |x, y, bulge| Vertex {
        at: xy(x, y),
        bulge,
    };
    assert_eq!(drawing.entities[0].layer, "Slot");
    assert_eq!(
        drawing.entities[0].shape,
        Shape::Polyline {
            vertices: vec![
                v(0.0, 0.0, 0.0),
                v(20.0, 0.0, 1.0),
                v(20.0, 10.0, 0.0),
                v(0.0, 10.0, 1.0)
            ],
            closed: true
        }
    );
    // Per-vertex widths (40, 41) are read past: one line width covers the whole import.
    assert_eq!(
        drawing.entities[1].shape,
        Shape::Polyline {
            vertices: vec![
                v(0.0, 0.0, -0.414_213_562_373_095_03),
                v(10.0, 0.0, 0.0),
                v(10.0, 10.0, 0.0)
            ],
            closed: false
        }
    );
}

#[test]
fn a_closed_polyline_becomes_separate_lines_and_exact_arcs_from_its_bulges() {
    let drawing = fixture("lwpolyline.dxf");
    let pieces = drawing.entities[0].shape.pieces();
    assert_eq!(pieces.len(), 4, "{pieces:?}");
    assert_eq!(
        pieces[0],
        Piece::Line {
            from: xy(0.0, 0.0),
            to: xy(20.0, 0.0)
        }
    );
    // Bulge 1 is a half turn counter-clockwise, centred on the chord's middle.
    let Piece::Arc {
        from,
        to,
        sweep,
        centre,
        radius,
    } = pieces[1]
    else {
        panic!("not an arc: {:?}", pieces[1])
    };
    assert_eq!((from, to), (xy(20.0, 0.0), xy(20.0, 10.0)));
    assert!((sweep - 180.0).abs() < 1e-9);
    assert_near(centre, xy(20.0, 5.0));
    assert!((radius - 5.0).abs() < 1e-9);
    assert_eq!(
        pieces[2],
        Piece::Line {
            from: xy(20.0, 10.0),
            to: xy(0.0, 10.0)
        }
    );
    // The closing segment takes the last vertex's bulge.
    let Piece::Arc {
        from, to, centre, ..
    } = pieces[3]
    else {
        panic!("not an arc")
    };
    assert_eq!((from, to), (xy(0.0, 10.0), xy(0.0, 0.0)));
    assert_near(centre, xy(0.0, 5.0));
}

#[test]
fn a_negative_bulge_turns_clockwise() {
    let drawing = fixture("lwpolyline.dxf");
    let pieces = drawing.entities[1].shape.pieces();
    assert_eq!(pieces.len(), 2);
    // tan(-90° / 4): a quarter turn clockwise from (0, 0) to (10, 0), bulging up from a
    // centre below the chord.
    let Piece::Arc {
        sweep,
        centre,
        radius,
        ..
    } = pieces[0]
    else {
        panic!("not an arc")
    };
    assert!((sweep + 90.0).abs() < 1e-9, "{sweep}");
    assert_near(centre, xy(5.0, -5.0));
    assert!((radius - 50f64.sqrt()).abs() < 1e-9);
}

#[test]
fn a_file_without_insunits_says_so() {
    let drawing = fixture("no_insunits.dxf");
    assert_eq!(drawing.insunits, None);
    assert_eq!(drawing.entities[0].layer, "0");
    assert_eq!(drawing.entities.len(), 1);
}

#[test]
fn an_inch_file_reads_insunits_1() {
    assert_eq!(fixture("inch.dxf").insunits, Some(1));
}

#[test]
fn other_entities_block_contents_and_paper_space_are_skipped_and_counted_per_type() {
    let drawing = fixture("skipped.dxf");
    // Only the model-space LINE is read; the block's LINE is not drawn by itself.
    assert_eq!(drawing.entities.len(), 1);
    assert_eq!(
        drawing.entities[0].shape,
        Shape::Line {
            from: xy(0.0, 0.0),
            to: xy(5.0, 0.0)
        }
    );
    let counts: BTreeMap<String, usize> = [
        ("DIMENSION", 1),
        ("HATCH", 1),
        ("INSERT", 1),
        ("LINE in paper space", 1),
        ("POINT", 2),
        ("TEXT", 1),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    assert_eq!(drawing.skipped, counts);
}

#[test]
fn a_file_that_is_not_dxf_is_refused_with_the_line() {
    let err = read(b"  0\nSECTION\nnot a code\nENTITIES\n").unwrap_err();
    assert_eq!(
        err.to_string(),
        "line 3: expected a DXF group code, found \"not a code\""
    );
    let err = read(b"0\nSECTION\n2\nENTITIES\n0\nLINE\n10\nabc\n0\nENDSEC\n0\nEOF\n").unwrap_err();
    assert_eq!(
        err.to_string(),
        "line 8: group 10 needs a number, found \"abc\""
    );
    let err = read(b"0\nSECTION\n2\n").unwrap_err();
    assert_eq!(err.to_string(), "line 3: group code 2 has no value");
}

#[test]
fn crlf_line_ends_and_a_byte_order_mark_are_read() {
    let text = "\u{feff}0\r\nSECTION\r\n2\r\nENTITIES\r\n0\r\nCIRCLE\r\n8\r\nA\r\n10\r\n1\r\n20\r\n2\r\n40\r\n3\r\n0\r\nENDSEC\r\n0\r\nEOF\r\n";
    let drawing = read(text.as_bytes()).unwrap();
    assert_eq!(drawing.entities[0].layer, "A");
    assert_eq!(
        drawing.entities[0].shape,
        Shape::Circle {
            centre: xy(1.0, 2.0),
            radius: 3.0
        }
    );
}
