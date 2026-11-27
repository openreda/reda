//! Meshing the freeform surfaces and curves: B-spline surfaces and curves, rational or not,
//! surfaces of revolution and of linear extrusion. Each fixture meshes closed at the volume it
//! was drawn with (tests/fixtures/README.md): a closed form where the shape has one, else
//! FreeCAD's (OpenCascade's) measure.

mod common;

use common::*;

#[test]
fn a_block_with_a_bicubic_b_spline_top_meshes_closed() {
    // A 10 x 8 x 2 block whose top is a bicubic Bézier patch, its four inner control points
    // raised 1.5: 160 + 1.5 · 80 · (1/2)² = 190.
    assert_solid("bspline_bulge.step", 190.0);
}

#[test]
fn a_rational_cylinder_with_its_seam_meshes_closed() {
    // A cylinder r = 1.5, h = 3 converted to NURBS: a rational B-spline surface closed on
    // itself in u, joined by a degree-1 B-spline seam, between two rational B-spline circles
    // on bilinear B-spline planes: π · 1.5² · 3.
    assert_solid("nurbs_cylinder.step", 21.205750);
}

#[test]
fn a_rational_sphere_meets_the_poles_its_rows_collapse_to() {
    // A sphere r = 2 converted to NURBS: one rational B-spline face whose first and last
    // columns of control points collapse to the poles, bounded by its seam: 4/3 · π · 2³.
    assert_solid("nurbs_sphere.step", 33.510322);
}

#[test]
fn an_extruded_b_spline_profile_meshes_closed() {
    // A closed B-spline profile extruded 3 mm: a surface of linear extrusion between two
    // planes bounded by the B-spline.
    assert_solid("extruded_spline.step", 56.976972);
}

#[test]
fn a_revolved_b_spline_profile_meets_its_pole() {
    // A vase: a B-spline profile from the axis out and up, revolved a whole turn and capped:
    // a surface of revolution whose curve meets the axis at a pole.
    assert_solid("revolved_spline.step", 32.400102);
}
