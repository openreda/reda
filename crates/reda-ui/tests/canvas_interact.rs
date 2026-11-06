//! Canvas interaction every editor shares: hit-testing a Scene, box selection, combining
//! picks with the selection, and snapping to the grid and to electrical targets.

use reda_ui::canvas::interact::{BoxMode, Combine, SnapTargets, box_select, combine, hit_test};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Millideg, Nm, Point};
use reda_ui::engine::render::{
    ColorKey, Item, Primitive, Scene, SceneLayer, SceneSource, SchematicColor,
};

fn mm(v: f64) -> Nm {
    Nm((v * 1e6) as i64)
}

fn p(x: f64, y: f64) -> Point {
    Point::new(mm(x), mm(y))
}

/// A Document point in nm, as the camera gives it.
fn at(x: f64, y: f64) -> (f64, f64) {
    (x * 1e6, y * 1e6)
}

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn push(scene: &mut Scene, object: Uuid, primitive: Primitive) {
    scene.items.push(Item {
        child: None,
        object,
        layer: SceneLayer::Schematic,
        color: ColorKey::Schematic(SchematicColor::Wire),
        primitive,
    });
}

/// Paper (1), a horizontal Wire (2) from (0, 0) to (10, 0) mm, a Unit (3) drawn as an
/// outlined 4 × 4 mm body at (20, 0) plus a Pin stub, and a second Wire (4) on top of the
/// Unit's body.
fn sheet() -> Scene {
    let mut s = Scene::new(SceneSource::None);
    push(
        &mut s,
        id(1),
        Primitive::Rect {
            center: p(50.0, 50.0),
            size: (mm(200.0), mm(200.0)),
            rotation: Millideg(0),
            corner_radius: Nm(0),
            stroke: None,
        },
    );
    push(
        &mut s,
        id(2),
        Primitive::Capsule {
            a: p(0.0, 0.0),
            b: p(10.0, 0.0),
            width: mm(0.2),
        },
    );
    push(
        &mut s,
        id(3),
        Primitive::Rect {
            center: p(20.0, 0.0),
            size: (mm(4.0), mm(4.0)),
            rotation: Millideg(0),
            corner_radius: Nm(0),
            stroke: Some(mm(0.2)),
        },
    );
    push(
        &mut s,
        id(3),
        Primitive::Capsule {
            a: p(22.0, 0.0),
            b: p(24.0, 0.0),
            width: mm(0.2),
        },
    );
    push(
        &mut s,
        id(4),
        Primitive::Capsule {
            a: p(19.0, 1.0),
            b: p(21.0, 1.0),
            width: mm(0.2),
        },
    );
    s
}

/// Everything but the paper can be picked.
fn pickable(uuid: Uuid) -> bool {
    uuid != id(1)
}

#[test]
fn a_click_picks_the_object_under_the_pointer_within_the_tolerance() {
    let scene = sheet();
    let tol = 0.5e6;
    assert_eq!(hit_test(&scene, at(5.0, 0.3), tol, pickable), Some(id(2)));
    assert_eq!(hit_test(&scene, at(5.0, 2.0), tol, pickable), None);
    // Inside an outlined body counts as on it.
    assert_eq!(hit_test(&scene, at(20.0, -1.0), tol, pickable), Some(id(3)));
    // Where two objects are hit, the nearer wins; on a tie, the one drawn on top.
    assert_eq!(hit_test(&scene, at(20.0, 1.0), tol, pickable), Some(id(4)));
    // The paper is never picked.
    assert_eq!(hit_test(&scene, at(50.0, 50.0), tol, pickable), None);
}

#[test]
fn a_box_dragged_left_to_right_encloses_and_right_to_left_touches() {
    assert_eq!(
        BoxMode::of_drag(at(0.0, 0.0), at(5.0, 5.0)),
        BoxMode::Enclose
    );
    assert_eq!(BoxMode::of_drag(at(5.0, 0.0), at(0.0, 5.0)), BoxMode::Touch);

    let scene = sheet();
    // Covers the whole Wire 2 and part of the Unit 3.
    let (lo, hi) = (at(-1.0, -1.0), at(21.5, 1.5));
    assert_eq!(
        box_select(&scene, lo, hi, BoxMode::Enclose, pickable),
        [id(2), id(4)]
    );
    assert_eq!(
        box_select(&scene, lo, hi, BoxMode::Touch, pickable),
        [id(2), id(3), id(4)]
    );
    // A Wire's bounding box crossing the box is not touching it unless the Wire does.
    let mut diagonal = Scene::new(SceneSource::None);
    push(
        &mut diagonal,
        id(5),
        Primitive::Capsule {
            a: p(0.0, 0.0),
            b: p(10.0, 10.0),
            width: mm(0.1),
        },
    );
    let (lo, hi) = (at(6.0, 0.0), at(10.0, 3.0));
    assert!(box_select(&diagonal, lo, hi, BoxMode::Touch, pickable).is_empty());
}

#[test]
fn a_wire_is_picked_on_any_of_its_segments_and_enclosed_only_whole() {
    // A Wire through (0, 0), (10, 0) and (15, 5) mm draws one capsule per segment.
    let mut scene = Scene::new(SceneSource::None);
    for (a, b) in [(p(0.0, 0.0), p(10.0, 0.0)), (p(10.0, 0.0), p(15.0, 5.0))] {
        push(
            &mut scene,
            id(7),
            Primitive::Capsule {
                a,
                b,
                width: mm(0.2),
            },
        );
    }
    let tol = 0.5e6;
    assert_eq!(hit_test(&scene, at(12.5, 2.6), tol, pickable), Some(id(7)));
    assert_eq!(hit_test(&scene, at(12.5, -2.0), tol, pickable), None);
    let (lo, hi) = (at(-1.0, -1.0), at(11.0, 1.0));
    assert!(box_select(&scene, lo, hi, BoxMode::Enclose, pickable).is_empty());
    assert_eq!(
        box_select(&scene, lo, hi, BoxMode::Touch, pickable),
        [id(7)]
    );
    let (lo, hi) = (at(-1.0, -1.0), at(16.0, 6.0));
    assert_eq!(
        box_select(&scene, lo, hi, BoxMode::Enclose, pickable),
        [id(7)]
    );
}

#[test]
fn shift_adds_to_the_selection_and_mod_toggles() {
    let current = [id(2), id(3)];
    assert_eq!(combine(&current, &[id(4)], Combine::Replace), [id(4)]);
    assert_eq!(
        combine(&current, &[id(3), id(4)], Combine::Add),
        [id(2), id(3), id(4)]
    );
    assert_eq!(
        combine(&current, &[id(3), id(4)], Combine::Toggle),
        [id(2), id(4)]
    );
    let mods = |shift, command| egui::Modifiers {
        shift,
        command,
        ..egui::Modifiers::NONE
    };
    assert_eq!(Combine::of(mods(false, false)), Combine::Replace);
    assert_eq!(Combine::of(mods(true, false)), Combine::Add);
    assert_eq!(Combine::of(mods(false, true)), Combine::Toggle);
}

#[test]
fn the_cursor_snaps_to_the_grid_unless_an_electrical_target_is_near() {
    let grid = Nm(1_270_000);
    let targets = SnapTargets {
        points: vec![p(3.0, 3.0)],
        segments: vec![(p(10.0, 0.0), p(10.0, 20.0))],
    };
    let radius = 0.5e6;
    // Nothing near: the nearest 50 mil grid point.
    let snapped = targets.snap(at(1.0, 1.4), grid, radius);
    assert_eq!(snapped.at, Point::new(Nm(1_270_000), Nm(1_270_000)));
    assert!(!snapped.electrical);
    // A Pin or Wire end near the cursor wins over the grid.
    let snapped = targets.snap(at(3.2, 2.9), grid, radius);
    assert_eq!(snapped.at, p(3.0, 3.0));
    assert!(snapped.electrical);
    // Near a Wire, the cursor lands on the Wire at the grid point along it.
    let snapped = targets.snap(at(10.3, 5.0), grid, radius);
    assert_eq!(snapped.at, Point::new(mm(10.0), Nm(1_270_000 * 4)));
    assert!(snapped.electrical);
}
