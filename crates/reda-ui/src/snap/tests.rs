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
    Some(Grid {
        pitch: Nm(pitch),
        origin: Point::default(),
    })
}

fn rules(capture: i64) -> Rules {
    Rules {
        capture: Nm(capture),
        grid: grid(1000),
        objects: Objects::All,
        off: BTreeSet::new(),
        guides: true,
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
    assert_eq!((s.at, s.hit), (p(10_500, 15_000), Hit::Guide));
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
    r.guides = false;
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
    let r = Rules::grid_only(Some(Grid {
        pitch: Nm(1_000),
        origin: p(250, -300),
    }));
    assert_eq!(snap(at(1_100, 1_100), &[], &[], &r).at, p(1_250, 700));
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
