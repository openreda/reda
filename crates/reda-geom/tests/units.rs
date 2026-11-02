use reda_geom::{Bounds, Millideg, Nm, Point, rotate};

#[test]
fn nanometre_constructors_scale_to_integer_nm() {
    assert_eq!(Nm::from_mm(1), Nm(1_000_000));
    assert_eq!(Nm::from_um(254), Nm(254_000));
    assert_eq!(Nm::from_mil(10), Nm(254_000));
    assert_eq!(Nm::from_inch(1), Nm(25_400_000));
}

#[test]
fn nanometres_add_subtract_negate_and_scale() {
    assert_eq!(Nm(5) + Nm(3), Nm(8));
    assert_eq!(Nm(5) - Nm(8), Nm(-3));
    assert_eq!(-Nm(4), Nm(-4));
    assert_eq!(Nm(3) * 4, Nm(12));
}

#[test]
fn points_add_component_wise() {
    let p = Point::new(Nm(1), Nm(2)) + Point::new(Nm(10), Nm(-20));
    assert_eq!(p, Point::new(Nm(11), Nm(-18)));
}

#[test]
fn millidegrees_from_degrees_and_normalised_to_one_turn() {
    assert_eq!(Millideg::from_deg(90), Millideg(90_000));
    assert_eq!(Millideg(450_000).normalized(), Millideg(90_000));
    assert_eq!(Millideg(-90_000).normalized(), Millideg(270_000));
    assert_eq!(Millideg(360_000).normalized(), Millideg(0));
}

#[test]
fn units_serialise_as_bare_integers() {
    assert_eq!(serde_json::to_string(&Nm(-12)).unwrap(), "-12");
    assert_eq!(serde_json::to_string(&Millideg(45_000)).unwrap(), "45000");
    assert_eq!(
        serde_json::to_string(&Point::new(Nm(1), Nm(2))).unwrap(),
        r#"{"x":1,"y":2}"#
    );
    assert_eq!(serde_json::from_str::<Nm>("7").unwrap(), Nm(7));
    assert!(
        serde_json::from_str::<Nm>("7.5").is_err(),
        "floats are refused"
    );
}

#[test]
fn rotation_is_exact_on_quarter_turns_and_rounds_otherwise() {
    assert_eq!(rotate(3, 4, Millideg(90_000)), (-4, 3));
    assert_eq!(rotate(3, 4, Millideg(-90_000)), (4, -3));
    assert_eq!(rotate(3, 4, Millideg(540_000)), (-3, -4));
    assert_eq!(rotate(1_000_000, 0, Millideg(30_000)), (866_025, 500_000));
    let p = Point::new(Nm(5), Nm(1));
    let about = Point::new(Nm(1), Nm(1));
    assert_eq!(
        p.rotated_about(about, Millideg(90_000)),
        Point::new(Nm(1), Nm(5))
    );
}

#[test]
fn bounds_hold_their_points_and_centre_rounds_down() {
    let pt = |x, y| Point::new(Nm(x), Nm(y));
    let b = Bounds::of_points([pt(3, -1), pt(-2, 4), pt(0, 0)]).unwrap();
    assert_eq!((b.min, b.max), (pt(-2, -1), pt(3, 4)));
    assert_eq!((b.width(), b.height()), (Nm(5), Nm(5)));
    assert_eq!(b.center(), pt(0, 1));
    assert_eq!(b.moved(pt(1, 1)).min, pt(-1, 0));
    assert_eq!(Bounds::around(pt(0, 0), Nm(2)).max, pt(2, 2));
    assert!(Bounds::of_points([]).is_none());
}
