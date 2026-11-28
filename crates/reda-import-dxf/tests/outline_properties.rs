//! The loop builder on random lines and arcs: it never panics, every segment ends up in
//! exactly one place, and what it keeps is a simple contour.

use proptest::prelude::*;
use reda_import_dxf::outline::build;
use reda_model::geom::{self, Millideg, Nm, Point, Segment};

fn point() -> impl Strategy<Value = Point> {
    // A coarse grid, so ends often meet and loops close.
    (0i64..6, 0i64..6).prop_map(|(x, y)| Point::new(Nm(x * 1_000_000), Nm(y * 1_000_000)))
}

fn segment() -> impl Strategy<Value = Segment> {
    (point(), point(), prop_oneof![Just(0i64), -170i64..170])
        .prop_filter("a segment has length", |(a, b, _)| a != b)
        .prop_map(|(a, b, deg)| Segment::arc(a, b, Millideg::from_deg(deg)))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn every_segment_is_a_contour_edge_or_a_stray(
        segments in prop::collection::vec(segment(), 0..24),
    ) {
        let loops = build(&segments, Nm::ZERO);
        let kept: Vec<_> = loops.outline.iter().chain(&loops.cutouts).collect();
        let edges: usize = kept.iter().map(|c| c.len()).sum();
        prop_assert_eq!(edges + loops.strays.len(), segments.len());
        prop_assert!(loops.strays.keys().all(|&i| i < segments.len()));
        for contour in &kept {
            prop_assert!(geom::check_contour(contour).is_ok());
            prop_assert_eq!(geom::crossing(contour), None);
        }
        prop_assert!(loops.cutouts.is_empty() || loops.outline.is_some());
    }

    #[test]
    fn a_tolerance_never_panics_and_keeps_contours_simple(
        segments in prop::collection::vec(segment(), 0..24),
        tolerance in 0i64..2_000_000,
    ) {
        let loops = build(&segments, Nm(tolerance));
        for contour in loops.outline.iter().chain(&loops.cutouts) {
            prop_assert!(geom::check_contour(contour).is_ok());
            prop_assert_eq!(geom::crossing(contour), None);
        }
    }
}
