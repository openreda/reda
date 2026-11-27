//! The trimming curve fitter on its own: bounds built from known points of a surface, whose
//! curves in the surface's parameters are known, fitted back.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use super::super::Run;
use super::*;
use crate::mesh::geometry::Frame;
use crate::mesh::surface::UvBox;

fn bound(runs: Vec<(u32, Vec<V3>)>) -> Bound3 {
    Bound3 {
        outer: true,
        runs: runs
            .into_iter()
            .map(|(edge, points)| Run {
                edge: RecordId(edge),
                points,
            })
            .collect(),
    }
}

/// `n + 1` points from `a` to `b` along `f`.
fn along(n: usize, a: f64, b: f64, f: impl Fn(f64) -> V3) -> Vec<V3> {
    (0..=n)
        .map(|i| f(a + (b - a) * i as f64 / n as f64))
        .collect()
}

/// The signed area of a polygon in parameter space.
fn area(points: &[UvPoint]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.u * b.v - b.u * a.v
        })
        .sum::<f64>()
        / 2.0
}

fn span(points: &[UvPoint], k: usize) -> f64 {
    let (lo, hi) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p.c(k)), hi.max(p.c(k)))
        });
    hi - lo
}

/// Every fitted point lies where its edge's point is, unless the surface loses a parameter
/// there.
fn assert_on_edge(surface: &Surface, fitted: &UvLoop) {
    for p in &fitted.points {
        if surface.lost(p.pos).is_some() {
            continue;
        }
        let d = (surface.eval(p.u, p.v) - p.pos).length();
        assert!(d < 1e-6, "({}, {}) is {d} from its edge point", p.u, p.v);
    }
}

fn assert_near(what: &str, got: f64, want: f64) {
    assert!((got - want).abs() < 1e-6, "{what}: {got}, not {want}");
}

/// The rational B-spline cylinder of `nurbs_cylinder.step`, closed on itself.
fn nurbs_cylinder() -> (Surface, usize, UvBox, f64) {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/nurbs_cylinder.step"
    );
    let model = crate::read(&std::fs::read(path).unwrap()).unwrap().model;
    model
        .ids()
        .find_map(|(id, _)| {
            let s = Surface::of(&model, id).ok()?;
            let Surface::Spline(ref f) = s else {
                return None;
            };
            let (k, period) = match f.periods() {
                (Some(p), _) => (0, p),
                (None, Some(p)) => (1, p),
                (None, None) => return None,
            };
            let domain = f.domain();
            Some((s, k, domain, period))
        })
        .expect("a closed B-spline surface")
}

#[test]
fn a_bound_with_its_seam_twice_lands_on_both_sides_of_a_closed_b_spline_surface() {
    // The cylinder's whole side: around at one end, up the seam, back around at the other
    // end and down the seam again. Fitted, it is the parameter rectangle, the seam's two uses
    // a period apart, each point found from the one before by the B-spline inversion.
    let (s, k, b, period) = nurbs_cylinder();
    let (around, across) = if k == 0 { (b.u, b.v) } else { (b.v, b.u) };
    let at = |a: f64, c: f64| if k == 0 { s.eval(a, c) } else { s.eval(c, a) };
    let (a0, a1) = (around.0, around.0 + period);
    let fitted = to_uv(
        &s,
        &bound(vec![
            (1, along(40, a0, a1, |a| at(a, across.0))),
            (2, along(8, across.0, across.1, |c| at(a1, c))),
            (3, along(40, a1, a0, |a| at(a, across.1))),
            (2, along(8, across.1, across.0, |c| at(a1, c))),
        ]),
    )
    .unwrap();
    assert_eq!(fitted.winding, [0, 0]);
    assert_on_edge(&s, &fitted);
    assert_near("around", span(&fitted.points, k), period);
    assert_near("across", span(&fitted.points, 1 - k), across.1 - across.0);
    assert_near(
        "area",
        area(&fitted.points).abs(),
        period * (across.1 - across.0),
    );
}

#[test]
fn a_bound_through_a_pole_is_split_there_into_one_point_per_side() {
    // A sphere's seam from pole to pole and back: u means nothing at the poles, so each pole
    // becomes two points, and the bound is the whole parameter rectangle.
    let frame = Frame::from_axes(V3(1.0, 2.0, 3.0), V3(0.0, 1.0, 1.0), V3::X).unwrap();
    let s = Surface::Sphere { frame, radius: 2.0 };
    let up = along(12, -FRAC_PI_2, FRAC_PI_2, |v| s.eval(0.0, v));
    let down: Vec<V3> = up.iter().rev().copied().collect();
    let fitted = to_uv(&s, &bound(vec![(7, up), (7, down)])).unwrap();
    assert_eq!(fitted.winding, [0, 0]);
    assert_on_edge(&s, &fitted);
    // 25 points less the repeated start, and each pole once more.
    assert_eq!(fitted.points.len(), 26);
    assert_near("around", span(&fitted.points, 0), TAU);
    assert_near("area", area(&fitted.points).abs(), TAU * PI);
}

#[test]
fn a_bound_once_around_a_surface_is_kept_open_with_its_winding() {
    // A cylinder's end circle alone (a band's bound): it comes back a period on.
    let frame = Frame::from_axes(V3(0.0, 0.0, 0.0), V3::Z, V3::X).unwrap();
    let s = Surface::Cylinder { frame, radius: 1.5 };
    let circle = along(24, 0.3, 0.3 - TAU, |u| s.eval(u, 2.0));
    let fitted = to_uv(&s, &bound(vec![(1, circle)])).unwrap();
    assert_eq!(fitted.winding, [-1, 0]);
    assert_on_edge(&s, &fitted);
    assert_eq!(fitted.points.len(), 24);
    assert_near("around", span(&fitted.points, 0), TAU * 23.0 / 24.0);
    assert!(fitted.points.iter().all(|p| (p.v - 2.0).abs() < 1e-12));
}

#[test]
fn a_bound_whose_runs_do_not_meet_is_reported() {
    let frame = Frame::from_axes(V3(0.0, 0.0, 0.0), V3::Z, V3::X).unwrap();
    let s = Surface::Cylinder { frame, radius: 1.0 };
    let fitted = to_uv(
        &s,
        &bound(vec![
            (1, along(4, 0.0, 1.0, |u| s.eval(u, 0.0))),
            (2, along(4, 2.0, 0.0, |u| s.eval(u, 0.0))),
        ]),
    );
    assert_eq!(
        fitted.err().as_deref(),
        Some("its bound's runs do not meet")
    );
}
