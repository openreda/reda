//! Tolerant reading (docs/spec/v0.3/step.md#tolerant-reading): the mesher draws what it can of
//! an imperfect model rather than refusing it. Each case has its fixture
//! (tests/fixtures/README.md), an edit of a well-made one whose volume or area is known.

mod common;

use common::*;
use reda_step::{DRAWS_NOTHING, Mesh, MeshOptions, load};

/// The area of a mesh's triangles.
fn area(mesh: &Mesh) -> f64 {
    mesh.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
            let (u, v) = (sub(b, a), sub(c, a));
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        })
        .sum()
}

#[test]
fn an_open_shell_is_meshed_open() {
    // The 10 x 20 x 5 block without its top, as the OPEN_SHELL of a surface model: its five
    // faces are drawn, 200 + 2 · 50 + 2 · 100 mm², and the mesh stays open.
    let loaded = fixture("open_box.step");
    let solids: Vec<_> = loaded.meshed.parts.iter().flat_map(|p| &p.solids).collect();
    assert_eq!(solids.len(), 1);
    let s = solids[0];
    assert!(!s.closed);
    assert_eq!(s.mesh.faces.len(), 5);
    assert!(!s.mesh.is_closed());
    assert_close("open box area", area(&s.mesh), 500.0, 1e-9);
}

#[test]
fn a_closed_shell_that_does_not_close_is_meshed_open() {
    // The block's CLOSED_SHELL with its top left out: drawn as it is, open.
    let text = fixture_text("ap214_face_colours.step");
    let open = text.replace(
        "#162=CLOSED_SHELL('',(#61,#93,#115,#137,#149,#161));",
        "#162=CLOSED_SHELL('',(#61,#115,#137,#149,#161));",
    );
    assert_ne!(open, text);
    let loaded = load(open.as_bytes(), &MeshOptions::default()).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let s = &loaded.meshed.parts[0].solids[0];
    assert_eq!(s.mesh.faces.len(), 5);
    assert!(!s.mesh.is_closed());
    assert_close("unclosed block area", area(&s.mesh), 500.0, 1e-9);
}

#[test]
fn a_loop_that_encloses_no_area_is_dropped() {
    // The block's top has an inner bound out and back along one line: the bound is dropped
    // and the block meshes closed.
    assert_solid("slit_loop.step", 1000.0);
}

#[test]
fn a_band_face_with_several_loops_around_its_axis_is_split_along_a_seam() {
    // A pin r = 2, h = 5 drilled across, its side face bounded by its two circles and the
    // hole's two ends with no seam edge: the seam the mesher draws runs clear of the holes,
    // one of which the file's own seam ran through. FreeCAD: 59.714956 mm³.
    assert_solid("banded_pin.step", 59.714956);
}

#[test]
fn an_open_surface_of_revolution_with_one_bound_gets_a_pole() {
    // The vase's side bounded by its top circle alone, its seam taken out: the face closes at
    // the pole where its curve meets the axis, and the vase at its volume (as drawn).
    assert_solid("revolved_cap.step", 32.400102);
}

#[test]
fn an_open_surface_of_revolution_with_one_bound_gets_a_seam() {
    // A cup wall bounded by its top circle alone, its curve off the axis at both ends: the
    // face is split along a meridian seam and runs down to its curve's end, where it is left
    // open. FreeCAD's area of the whole wall: 36.302046 mm².
    let loaded = fixture("cup_wall.step");
    let s = &loaded.meshed.parts[0].solids[0];
    assert!(!s.mesh.is_closed());
    assert_close("cup wall area", area(&s.mesh), 36.302046, 0.005);
    let fine = fixture_with("cup_wall.step", &FINE);
    let s = &fine.meshed.parts[0].solids[0];
    assert_close("cup wall area", area(&s.mesh), 36.302046, 0.0005);
}

#[test]
fn a_solid_that_still_fails_is_skipped_and_reported_and_the_rest_drawn() {
    // Two blocks; the second's bottom lies on a sphere of negative radius.
    let loaded = load_fixture("failing_solid.step", &MeshOptions::default());
    let solids: Vec<_> = loaded.meshed.parts.iter().flat_map(|p| &p.solids).collect();
    assert_eq!(solids.len(), 1, "only the good block is drawn");
    assert_eq!(solids[0].name, "block");
    assert!(solids[0].mesh.is_closed());
    assert_close("good block", loaded.meshed.volume(), 1000.0, 1e-9);
    let [w] = &loaded.meshed.warnings[..] else {
        panic!("one warning: {:?}", loaded.meshed.warnings)
    };
    assert!(
        w.starts_with("solid #")
            && w.contains("'broken block' is skipped: face #")
            && w.contains("its radius is not positive"),
        "{w}"
    );
    assert_eq!(loaded.warnings, loaded.meshed.warnings);
}

#[test]
fn a_model_that_parses_but_meshes_nothing_is_reported_as_drawing_nothing() {
    // One block with a face on a sphere of negative radius: the file reads, and its only
    // solid is skipped.
    let loaded = load_fixture("draws_nothing.step", &MeshOptions::default());
    assert!(loaded.meshed.draws_nothing());
    assert!(loaded.meshed.bounding_box().is_none());
    let w = &loaded.meshed.warnings;
    assert_eq!(w.len(), 2, "{w:?}");
    assert!(w[0].contains("'block' is skipped"), "{w:?}");
    assert_eq!(w[1], DRAWS_NOTHING);
    // The records are still there to export.
    assert!(loaded.model.records().len() > 100);
}

#[test]
fn a_model_that_draws_is_not_reported_as_drawing_nothing() {
    let loaded = fixture("ap214_face_colours.step");
    assert!(!loaded.meshed.draws_nothing());
    assert!(!loaded.warnings.iter().any(|w| w == DRAWS_NOTHING));
}
