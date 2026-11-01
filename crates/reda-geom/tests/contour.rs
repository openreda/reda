//! Closed contours: Board Outlines and Board Cutouts as vertex lists with straight or
//! arc edges.

use reda_geom::{ContourError, Millideg, Nm, Point, Segment, Vertex, chain, check_contour};

fn mm(v: i64) -> Nm {
    Nm::from_mm(v)
}

fn p(x: i64, y: i64) -> Point {
    Point::new(mm(x), mm(y))
}

fn line(from: Point, to: Point) -> Segment {
    Segment::line(from, to)
}

#[test]
fn a_rectangle_without_corner_radius_has_four_straight_corners() {
    let rect = reda_geom::rounded_rect(mm(100), mm(80), Nm::ZERO);
    assert_eq!(
        rect,
        vec![
            Vertex::at(p(0, 0)),
            Vertex::at(p(100, 0)),
            Vertex::at(p(100, 80)),
            Vertex::at(p(0, 80)),
        ]
    );
}

#[test]
fn a_rounded_rectangle_turns_each_corner_with_a_quarter_arc() {
    let rect = reda_geom::rounded_rect(mm(100), mm(80), mm(5));
    let quarter = Millideg::from_deg(90);
    assert_eq!(
        rect,
        vec![
            Vertex::at(p(5, 0)),
            Vertex::arc(p(95, 0), quarter),
            Vertex::at(p(100, 5)),
            Vertex::arc(p(100, 75), quarter),
            Vertex::at(p(95, 80)),
            Vertex::arc(p(5, 80), quarter),
            Vertex::at(p(0, 75)),
            Vertex::arc(p(0, 5), quarter),
        ]
    );
    assert_eq!(check_contour(&rect), Ok(()));
}

#[test]
fn a_fully_rounded_side_drops_its_zero_length_edge() {
    // 20 mm high with 10 mm corners: the left and right sides vanish.
    let rect = reda_geom::rounded_rect(mm(100), mm(20), mm(10));
    assert_eq!(rect.len(), 6);
    assert_eq!(check_contour(&rect), Ok(()));
}

#[test]
fn segments_in_any_order_and_direction_chain_into_one_contour() {
    let quarter = Millideg::from_deg(90);
    let segments = [
        line(p(10, 10), p(0, 10)),
        line(p(0, 0), p(10, 0)),
        // Drawn backwards: from the top back down to the start, as a clockwise arc.
        Segment::arc(p(0, 10), p(0, 0), Millideg::from_deg(-180)),
        Segment::arc(p(10, 0), p(10, 10), quarter),
    ];
    let contour = chain(&segments).unwrap();
    assert_eq!(
        contour,
        vec![
            Vertex::at(p(10, 10)),
            Vertex::arc(p(0, 10), Millideg::from_deg(-180)),
            Vertex::at(p(0, 0)),
            Vertex::arc(p(10, 0), quarter),
        ]
    );
}

#[test]
fn a_reversed_segment_flips_its_arc() {
    let segments = [
        line(p(0, 0), p(10, 0)),
        line(p(10, 0), p(10, 10)),
        Segment::arc(p(0, 0), p(10, 10), Millideg::from_deg(-90)),
    ];
    let contour = chain(&segments).unwrap();
    assert_eq!(contour[2], Vertex::arc(p(10, 10), Millideg::from_deg(90)));
}

#[test]
fn an_open_chain_is_refused_at_its_loose_end() {
    let segments = [line(p(0, 0), p(10, 0)), line(p(10, 0), p(10, 10))];
    assert_eq!(chain(&segments), Err(ContourError::OpenEnd(p(0, 0))));
}

#[test]
fn a_branch_is_refused() {
    let segments = [
        line(p(0, 0), p(10, 0)),
        line(p(10, 0), p(0, 10)),
        line(p(0, 10), p(0, 0)),
        line(p(0, 0), p(-5, -5)),
        line(p(-5, -5), p(-5, 0)),
        line(p(-5, 0), p(0, 0)),
    ];
    assert_eq!(chain(&segments), Err(ContourError::Branch(p(0, 0))));
}

#[test]
fn two_separate_loops_are_refused() {
    let triangle = |dx| {
        [
            line(p(dx, 0), p(dx + 10, 0)),
            line(p(dx + 10, 0), p(dx, 10)),
            line(p(dx, 10), p(dx, 0)),
        ]
    };
    let segments: Vec<Segment> = triangle(0).into_iter().chain(triangle(50)).collect();
    assert_eq!(chain(&segments), Err(ContourError::SeveralLoops));
}

#[test]
fn degenerate_contours_are_refused() {
    assert_eq!(check_contour(&[]), Err(ContourError::TooFewVertices));
    let two_lines = [Vertex::at(p(0, 0)), Vertex::at(p(10, 0))];
    assert_eq!(check_contour(&two_lines), Err(ContourError::TooFewVertices));
    let repeated = [
        Vertex::at(p(0, 0)),
        Vertex::at(p(0, 0)),
        Vertex::at(p(10, 0)),
    ];
    assert_eq!(
        check_contour(&repeated),
        Err(ContourError::ZeroLengthEdge(p(0, 0)))
    );
    let full_turn = [
        Vertex::arc(p(0, 0), Millideg::TURN),
        Vertex::at(p(10, 0)),
        Vertex::at(p(0, 10)),
    ];
    assert_eq!(
        check_contour(&full_turn),
        Err(ContourError::BadSweep(p(0, 0)))
    );
}

#[test]
fn a_circle_of_two_half_arcs_is_a_contour() {
    let half = Millideg::from_deg(180);
    let circle = [Vertex::arc(p(0, 0), half), Vertex::arc(p(10, 0), half)];
    assert_eq!(check_contour(&circle), Ok(()));
}

#[test]
fn vertices_store_straight_edges_without_a_sweep() {
    let json = serde_json::to_string(&[
        Vertex::at(p(1, 2)),
        Vertex::arc(p(3, 4), Millideg::from_deg(90)),
    ])
    .unwrap();
    assert_eq!(
        json,
        r#"[{"x":1000000,"y":2000000},{"x":3000000,"y":4000000,"sweep":90000}]"#
    );
    let back: Vec<Vertex> = serde_json::from_str(&json).unwrap();
    assert_eq!(back[0], Vertex::at(p(1, 2)));
}
