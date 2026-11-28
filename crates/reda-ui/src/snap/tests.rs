//! The snapping model, as pure functions: no window, no Shell.

use std::collections::BTreeSet;

use reda_engine::model::Uuid;
use reda_engine::model::geom::{Nm, Point};

use super::*;

const MIL: i64 = 25_400;

fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}

fn at(x: i64, y: i64) -> (f64, f64) {
    (x as f64, y as f64)
}

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn grid(pitch: i64) -> Option<Grid> {
    Some(Grid::square(Nm(pitch), Point::default()))
}

fn rules(capture: i64) -> Rules {
    Rules {
        capture: Nm(capture),
        grid: grid(1000),
        objects: Objects::All,
        off: BTreeSet::new(),
        alignment: true,
        placed: Vec::new(),
    }
}

fn pad(x: i64, y: i64, n: u128) -> Target {
    Target::point(p(x, y), Hotspot::PadCenter, id(n), OnLayers::Copper)
}

#[test]
fn capture_is_a_length_checked_on_both_axes_not_a_true_distance() {
    // 8 mil capture; the pad is 7 mil away in X and in Y: about 9.9 mil in a straight
    // line, too far for a radius, but inside the box on both axes.
    let r = rules(8 * MIL);
    let targets = [pad(0, 0, 1)];
    let snapped = snap(at(7 * MIL, 7 * MIL), &targets, &[], &r);
    assert_eq!(snapped.at, p(0, 0));
    assert_eq!(
        snapped.hit,
        Hit::Hotspot {
            kind: Hotspot::PadCenter,
            object: id(1)
        }
    );
    // Within it on one axis only does not capture.
    let missed = snap(at(9 * MIL, 0), &targets, &[], &r);
    assert_eq!(missed.hit, Hit::Grid);
}

#[test]
fn the_nearest_captured_hotspot_wins() {
    let r = rules(10_000);
    let targets = [pad(0, 0, 1), pad(6_000, 0, 2)];
    let snapped = snap(at(4_000, 1_000), &targets, &[], &r);
    assert_eq!(snapped.at, p(6_000, 0));
}

#[test]
fn hotspot_beats_alignment_guide_which_beats_grid() {
    let r = rules(2_000);
    let targets = [pad(10_400, 10_300, 1)];
    let guides = [Guide::X(10_500.into_nm()), Guide::Y(20_100.into_nm())];
    // Near the pad and the guide: the pad.
    assert_eq!(
        snap(at(10_600, 10_000), &targets, &guides, &r).at,
        p(10_400, 10_300)
    );
    // Near the vertical guide only: x on it, y on the grid.
    let s = snap(at(11_200, 15_300), &targets, &guides, &r);
    assert_eq!((s.at, s.hit), (p(10_500, 15_000), Hit::Alignment));
    // Near both guides: both axes.
    assert_eq!(
        snap(at(11_000, 19_000), &targets, &guides, &r).at,
        p(10_500, 20_100)
    );
    // Near nothing: the grid.
    let s = snap(at(30_400, 30_600), &targets, &guides, &r);
    assert_eq!((s.at, s.hit), (p(30_000, 31_000), Hit::Grid));
    // Suspended (`Mod`), the guides give way to the grid.
    let mut r = r;
    r.alignment = false;
    let s = snap(at(11_200, 15_300), &targets, &guides, &r);
    assert_eq!((s.at, s.hit), (p(11_000, 15_000), Hit::Grid));
}

trait IntoNm {
    fn into_nm(self) -> Nm;
}

impl IntoNm for i64 {
    fn into_nm(self) -> Nm {
        Nm(self)
    }
}

#[test]
fn points_beat_segments_and_a_straight_segment_lands_on_the_grid() {
    let r = rules(3_000);
    let wire = Target {
        shape: Shape::Segment(p(0, 0), p(10_000, 0)),
        kind: Hotspot::Wire,
        object: id(7),
        layers: OnLayers::Every,
    };
    let end = Target::point(p(10_000, 0), Hotspot::WireEnd, id(7), OnLayers::Every);
    let s = snap(at(4_300, 2_000), &[wire, end], &[], &r);
    assert_eq!(s.at, p(4_000, 0));
    assert_eq!(s.hit.hotspot(), Some(Hotspot::Wire));
    assert_eq!(
        snap(at(9_000, 2_000), &[wire, end], &[], &r).hit.hotspot(),
        Some(Hotspot::WireEnd)
    );
}

#[test]
fn the_grid_runs_from_its_origin() {
    let r = Rules::grid_only(Some(Grid::square(Nm(1_000), p(250, -300))));
    assert_eq!(snap(at(1_100, 1_100), &[], &[], &r).at, p(1_250, 700));
}

#[test]
fn a_grid_with_a_step_y_snaps_each_axis_to_its_own_step() {
    let r = Rules::grid_only(Some(Grid {
        pitch: crate::grid::Step {
            x: Nm(500),
            y: Nm(1_270),
        },
        origin: p(0, 100),
    }));
    assert_eq!(snap(at(740, 700), &[], &[], &r).at, p(500, 100));
    assert_eq!(snap(at(760, 800), &[], &[], &r).at, p(1_000, 1_370));
}

#[test]
fn gridless_keeps_hotspots_and_leaves_the_rest_free() {
    let mut r = rules(1_000);
    r.grid = None;
    let s = snap(at(5_432, 1_234), &[pad(0, 0, 1)], &[], &r);
    assert_eq!((s.at, s.hit), (p(5_432, 1_234), Hit::Free));
    assert_eq!(snap(at(500, -400), &[pad(0, 0, 1)], &[], &r).at, p(0, 0));
}

#[test]
fn a_kind_off_in_the_checklist_is_skipped() {
    let mut r = rules(1_000);
    r.off.insert(Hotspot::PadCenter);
    let via = Target::point(p(800, 0), Hotspot::ViaCenter, id(2), OnLayers::Copper);
    let s = snap(at(100, 0), &[pad(0, 0, 1), via], &[], &r);
    assert_eq!(s.hit.hotspot(), Some(Hotspot::ViaCenter));
}

#[test]
fn modes_pick_objects_by_layer() {
    let (top, silk) = (id(100), id(101));
    let track = Target::point(p(0, 0), Hotspot::TrackEnd, id(1), OnLayers::One(top));
    let via = Target::point(p(0, 0), Hotspot::ViaCenter, id(2), OnLayers::Copper);
    let text = Target::point(p(0, 0), Hotspot::TextAnchor, id(3), OnLayers::One(silk));
    let origin = Target::point(p(0, 0), Hotspot::FootprintOrigin, id(4), OnLayers::Every);
    let all = [track, via, text, origin];
    let kinds = |objects: Objects| -> Vec<Hotspot> {
        let mut r = rules(10);
        r.objects = objects;
        all.iter()
            .filter(|t| snap(at(0, 0), &[**t], &[], &r).hit != Hit::Grid)
            .map(|t| t.kind)
            .collect()
    };
    use Hotspot::*;
    assert_eq!(
        kinds(Objects::Layer {
            layer: top,
            copper: true
        }),
        [TrackEnd, ViaCenter, FootprintOrigin]
    );
    // Through-hole Pads and Vias count on copper Layers only.
    assert_eq!(
        kinds(Objects::Layer {
            layer: silk,
            copper: false
        }),
        [TextAnchor, FootprintOrigin]
    );
    assert_eq!(
        kinds(Objects::All),
        [TrackEnd, ViaCenter, TextAnchor, FootprintOrigin]
    );
    assert_eq!(kinds(Objects::Off), Vec::<Hotspot>::new());
}

#[test]
fn checklists_are_data_per_document_type() {
    use Hotspot::*;
    assert_eq!(
        offered(DocumentType::Pcb),
        [
            PadCenter,
            ViaCenter,
            TrackEnd,
            ArcEnd,
            ArcCenter,
            OutlineVertex,
            FootprintOrigin,
            TextAnchor
        ]
    );
    assert_eq!(offered(DocumentType::Footprint), offered(DocumentType::Pcb));
    assert_eq!(
        offered(DocumentType::Schematic),
        [PinEnd, WireEnd, WireVertex, Wire, PortAnchor, LabelAnchor]
    );
    assert_eq!(
        offered(DocumentType::Symbol),
        offered(DocumentType::Schematic)
    );
}

#[test]
fn mod_suspends_object_snap_and_mod_shift_the_grid_too_in_flight() {
    let mut s = Suspend::default();
    s.update(false, false, false);
    s.update(true, false, false);
    assert!(!s.objects());
    s.update(true, true, false);
    assert!(s.objects() && !s.grid());
    s.update(true, true, true);
    assert!(s.objects() && s.grid());
    s.update(true, false, true);
    assert!(!s.objects() && !s.grid());
    // Out of flight nothing is suspended.
    s.update(false, true, true);
    assert!(!s.objects() && !s.grid());
}

#[test]
fn mod_held_at_the_start_means_detach_until_pressed_again() {
    let mut s = Suspend::default();
    s.update(false, true, false);
    s.update(true, true, false);
    assert!(!s.objects(), "held from the start: detach, not suspend");
    s.update(true, false, false);
    s.update(true, true, false);
    assert!(s.objects(), "released and pressed again: suspends");
}

fn line_guide(n: u128, x: i64, y: i64, deg: i64) -> PlacedGuide {
    PlacedGuide {
        object: id(n),
        at: p(x, y),
        angle: Some(reda_engine::model::geom::Millideg(deg * 1000)),
    }
}

fn point_guide(n: u128, x: i64, y: i64) -> PlacedGuide {
    PlacedGuide {
        object: id(n),
        at: p(x, y),
        angle: None,
    }
}

fn with_guides(capture: i64, guides: Vec<PlacedGuide>) -> Rules {
    let mut r = rules(capture);
    r.placed = guides;
    r
}

#[test]
fn a_line_guide_catches_the_cursor_with_the_other_axis_on_the_grid() {
    let r = with_guides(2_000, vec![line_guide(9, 0, 10_300, 0)]);
    let s = snap(at(4_400, 9_000), &[], &[], &r);
    assert_eq!(
        (s.at, s.hit),
        (p(4_000, 10_300), Hit::Guide { object: id(9) })
    );
    // Beyond the Snap Distance from it: the grid.
    let s = snap(at(4_400, 7_000), &[], &[], &r);
    assert_eq!((s.at, s.hit), (p(4_000, 7_000), Hit::Grid));
    let r = with_guides(2_000, vec![line_guide(9, 2_250, 0, 90)]);
    assert_eq!(snap(at(3_000, 5_600), &[], &[], &r).at, p(2_250, 6_000));
}

#[test]
fn along_a_slanted_guide_the_snap_is_the_nearest_grid_line_crossing() {
    // y = x + 500: from the cursor's foot on it, (3 100, 3 600), the crossing with the
    // grid line x = 3 000 is nearer than the one with y = 4 000.
    let r = with_guides(2_000, vec![line_guide(9, 0, 500, 45)]);
    let s = snap(at(3_000, 3_700), &[], &[], &r);
    assert_eq!(
        (s.at, s.hit),
        (p(3_000, 3_500), Hit::Guide { object: id(9) })
    );
    // Gridless, the cursor's foot on the line.
    let mut r = r;
    r.grid = None;
    assert_eq!(snap(at(3_000, 3_700), &[], &[], &r).at, p(3_100, 3_600));
}

#[test]
fn guide_crossings_and_point_guides_are_snap_points() {
    let r = with_guides(
        2_000,
        vec![
            line_guide(1, 10_250, 0, 90),
            line_guide(2, 0, 20_250, 0),
            point_guide(3, 5_250, 5_250),
        ],
    );
    let s = snap(at(10_900, 20_900), &[], &[], &r);
    assert_eq!(s.at, p(10_250, 20_250));
    assert!(matches!(s.hit, Hit::Guide { .. }));
    let s = snap(at(6_000, 6_000), &[], &[], &r);
    assert_eq!(
        (s.at, s.hit),
        (p(5_250, 5_250), Hit::Guide { object: id(3) })
    );
    // A point Guide catches on both axes, as a Hotspot does.
    assert_eq!(snap(at(6_000, 7_500), &[], &[], &r).hit, Hit::Grid);
}

#[test]
fn hotspot_beats_guide_which_beats_alignment_guide_which_beats_grid() {
    let targets = [pad(10_400, 10_300, 1)];
    let r = with_guides(2_000, vec![line_guide(9, 10_200, 0, 90)]);
    let aligned = [Guide::X(10_500.into_nm())];
    assert_eq!(
        snap(at(10_600, 10_100), &targets, &aligned, &r)
            .hit
            .hotspot(),
        Some(Hotspot::PadCenter)
    );
    let s = snap(at(10_400, 0), &targets, &aligned, &r);
    assert_eq!((s.at, s.hit), (p(10_200, 0), Hit::Guide { object: id(9) }));
    let r = with_guides(2_000, Vec::new());
    let s = snap(at(10_400, 0), &targets, &aligned, &r);
    assert_eq!((s.at, s.hit), (p(10_500, 0), Hit::Alignment));
}

mod order {
    //! Snap order as a property: whatever is near, the snap comes from the first level
    //! that catches the cursor (docs/spec/v0.3/testing.md#property-tests).
    use proptest::prelude::*;

    use super::*;

    const CAPTURE: i64 = 2_000;

    fn guide() -> impl Strategy<Value = PlacedGuide> {
        (
            0u128..1_000,
            -20_000i64..20_000,
            -20_000i64..20_000,
            prop::option::of(prop::sample::select(vec![0i64, 45, 90, 135, 30])),
        )
            .prop_map(|(n, x, y, deg)| PlacedGuide {
                object: id(n),
                at: p(x, y),
                angle: deg.map(|d| reda_engine::model::geom::Millideg(d * 1000)),
            })
    }

    /// How far `q` is from the Guide: perpendicular for a line, on the worse axis for a
    /// point.
    fn off(g: &PlacedGuide, q: (f64, f64)) -> f64 {
        let (x, y) = (g.at.x.0 as f64, g.at.y.0 as f64);
        match g.angle {
            None => (q.0 - x).abs().max((q.1 - y).abs()),
            Some(a) => {
                let t = (a.0 as f64 / 1000.0).to_radians();
                ((q.0 - x) * t.sin() - (q.1 - y) * t.cos()).abs()
            }
        }
    }

    proptest! {
        #[test]
        fn the_first_level_that_catches_the_cursor_gives_the_snap(
            cursor in (-20_000i64..20_000, -20_000i64..20_000),
            pads in prop::collection::vec((-20_000i64..20_000, -20_000i64..20_000), 0..3),
            guides in prop::collection::vec(guide(), 0..4),
        ) {
            let r = with_guides(CAPTURE, guides.clone());
            let targets: Vec<Target> = pads
                .iter()
                .enumerate()
                .map(|(i, (x, y))| pad(*x, *y, 10_000 + i as u128))
                .collect();
            let c = at(cursor.0, cursor.1);
            let s = snap(c, &targets, &[], &r);
            let capture = CAPTURE as f64;
            let caught = |x: i64, y: i64| {
                (x as f64 - c.0).abs() <= capture && (y as f64 - c.1).abs() <= capture
            };
            let hotspot = pads.iter().any(|(x, y)| caught(*x, *y));
            let point = guides
                .iter()
                .any(|g| g.angle.is_none() && caught(g.at.x.0, g.at.y.0));
            let line = guides
                .iter()
                .any(|g| g.angle.is_some() && off(g, c) <= capture);
            // Where two lines cross, by solving their two line equations.
            let lines: Vec<_> = guides.iter().filter_map(|g| {
                let t = (g.angle?.0 as f64 / 1000.0).to_radians();
                Some((g.at.x.0 as f64, g.at.y.0 as f64, t.cos(), t.sin()))
            }).collect();
            let crossing = lines.iter().enumerate().any(|(i, a)| {
                lines[i + 1..].iter().any(|b| {
                    let det = a.2 * b.3 - a.3 * b.2;
                    if det.abs() < 1e-9 {
                        return false;
                    }
                    let s = ((b.0 - a.0) * b.3 - (b.1 - a.1) * b.2) / det;
                    let (x, y) = (a.0 + s * a.2, a.1 + s * a.3);
                    (x - c.0).abs() <= capture && (y - c.1).abs() <= capture
                })
            });
            if hotspot {
                prop_assert!(s.hit.hotspot().is_some(), "{s:?}");
            } else if point || line || crossing {
                let Hit::Guide { object } = s.hit else {
                    return Err(TestCaseError::fail(format!("{s:?}")));
                };
                // The snap lies on a Guide of that UUID (within rounding to the nm).
                let landed = (s.at.x.0 as f64, s.at.y.0 as f64);
                let on = guides
                    .iter()
                    .filter(|g| g.object == object)
                    .any(|g| off(g, landed) <= 1.0);
                prop_assert!(on, "{s:?}");
            } else {
                prop_assert_eq!(s, Snapped { at: r.to_grid(c), hit: Hit::Grid });
            }
        }
    }
}
