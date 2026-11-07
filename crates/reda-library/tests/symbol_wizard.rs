//! The Symbol Wizard's pure side: laying out Pins and a body for each of the six layouts,
//! reading a Unit's Pins back as the wizard's input, and applying a layout to a Symbol.

use reda_library::wizard::{
    ApplyError, Layout, Side, SymbolPlan, WizardPin, apply, plan, read_unit,
};
use reda_model::PinType;
use reda_model::geom::{Nm, Point};
use reda_model::{Shape, Uuid};

const MIL: i64 = 25_400;

fn mil(x: i64, y: i64) -> Point {
    Point::new(Nm(x * MIL), Nm(y * MIL))
}

fn pins(n: usize) -> Vec<WizardPin> {
    (1..=n).map(|i| WizardPin::new(i.to_string(), "")).collect()
}

/// Each Pin's number, connection end (in mil) and direction into the body (in degrees).
fn placed(plan: &SymbolPlan) -> Vec<(String, (i64, i64), i64)> {
    plan.pins
        .iter()
        .map(|p| {
            (
                p.number.clone(),
                (p.at.x.0 / MIL, p.at.y.0 / MIL),
                p.rotation.0 / 1000,
            )
        })
        .collect()
}

fn row(number: &str, at: (i64, i64), degrees: i64) -> (String, (i64, i64), i64) {
    (number.to_owned(), at, degrees)
}

fn body(plan: &SymbolPlan) -> ((i64, i64), (i64, i64)) {
    let (a, b) = plan.body;
    ((a.x.0 / MIL, a.y.0 / MIL), (b.x.0 / MIL, b.y.0 / MIL))
}

#[test]
fn dil_runs_down_the_left_and_back_up_the_right_with_pin_1_at_the_origin() {
    let p = plan(Layout::Dil, &pins(8), None).unwrap();
    assert_eq!(
        placed(&p),
        [
            row("1", (0, 0), 0),
            row("2", (0, -100), 0),
            row("3", (0, -200), 0),
            row("4", (0, -300), 0),
            row("5", (400, -300), 180),
            row("6", (400, -200), 180),
            row("7", (400, -100), 180),
            row("8", (400, 0), 180),
        ]
    );
    assert_eq!(body(&p), ((100, 100), (300, -400)));
}

#[test]
fn dil_with_an_odd_count_puts_the_extra_pin_on_the_left_and_bottom_aligns_the_right() {
    let p = plan(Layout::Dil, &pins(5), None).unwrap();
    assert_eq!(
        placed(&p),
        [
            row("1", (0, 0), 0),
            row("2", (0, -100), 0),
            row("3", (0, -200), 0),
            row("4", (400, -200), 180),
            row("5", (400, -100), 180),
        ]
    );
}

#[test]
fn quad_goes_counter_clockwise_round_four_sides() {
    let p = plan(Layout::Quad, &pins(8), None).unwrap();
    // Two Pins a side: left top to bottom, bottom left to right, right bottom to top,
    // top right to left. The body is 300 mil square.
    assert_eq!(
        placed(&p),
        [
            row("1", (0, 0), 0),
            row("2", (0, -100), 0),
            row("3", (200, -300), 90),
            row("4", (300, -300), 90),
            row("5", (500, -100), 180),
            row("6", (500, 0), 180),
            row("7", (300, 200), 270),
            row("8", (200, 200), 270),
        ]
    );
    assert_eq!(body(&p), ((100, 100), (400, -200)));
}

#[test]
fn connector_pairs_odd_pins_on_the_left_with_even_pins_on_the_right() {
    let p = plan(Layout::Connector, &pins(4), None).unwrap();
    assert_eq!(
        placed(&p),
        [
            row("1", (0, 0), 0),
            row("2", (400, 0), 180),
            row("3", (0, -100), 0),
            row("4", (400, -100), 180),
        ]
    );
    assert_eq!(body(&p), ((100, 100), (300, -200)));
}

#[test]
fn zig_zag_alternates_sides_one_pitch_lower_each_pin() {
    let p = plan(Layout::ZigZag, &pins(4), None).unwrap();
    assert_eq!(
        placed(&p),
        [
            row("1", (0, 0), 0),
            row("2", (400, -100), 180),
            row("3", (0, -200), 0),
            row("4", (400, -300), 180),
        ]
    );
    assert_eq!(body(&p), ((100, 100), (300, -400)));
}

#[test]
fn single_row_puts_every_pin_down_the_left() {
    let p = plan(Layout::SingleRow, &pins(3), None).unwrap();
    assert_eq!(
        placed(&p),
        [
            row("1", (0, 0), 0),
            row("2", (0, -100), 0),
            row("3", (0, -200), 0),
        ]
    );
    assert_eq!(body(&p), ((100, 100), (300, -300)));
}

#[test]
fn manual_puts_each_pin_on_its_side_in_reading_order() {
    let mut rows = pins(5);
    for (row, side) in rows.iter_mut().zip([
        Side::Left,
        Side::Right,
        Side::Right,
        Side::Top,
        Side::Bottom,
    ]) {
        row.side = Some(side);
    }
    let p = plan(Layout::Manual, &rows, None).unwrap();
    // Left and right top to bottom, top and bottom left to right.
    assert_eq!(
        placed(&p),
        [
            row("1", (0, 0), 0),
            row("2", (400, 0), 180),
            row("3", (400, -100), 180),
            row("4", (200, 200), 270),
            row("5", (200, -300), 90),
        ]
    );
}

#[test]
fn manual_needs_a_side_on_every_pin() {
    let mut rows = pins(2);
    rows[0].side = Some(Side::Left);
    let err = plan(Layout::Manual, &rows, None).unwrap_err();
    assert_eq!(err.pointer, "/pins/1/side");
}

#[test]
fn a_change_of_group_leaves_one_empty_slot() {
    let mut rows = pins(4);
    rows[2].group = Some("power".into());
    rows[3].group = Some("power".into());
    let p = plan(Layout::SingleRow, &rows, None).unwrap();
    assert_eq!(
        placed(&p),
        [
            row("1", (0, 0), 0),
            row("2", (0, -100), 0),
            row("3", (0, -300), 0),
            row("4", (0, -400), 0),
        ]
    );
}

#[test]
fn the_body_widens_for_long_pin_names_and_stays_on_the_grid() {
    let rows: Vec<WizardPin> = [("1", "RESET"), ("2", "GND"), ("3", "VCC"), ("4", "OUT")]
        .into_iter()
        .map(|(n, name)| WizardPin::new(n, name))
        .collect();
    let p = plan(Layout::Dil, &rows, None).unwrap();
    let ((left, _), (right, _)) = body(&p);
    // RESET (5 letters) and OUT (3) need about 360 mil of names, plus a pitch between.
    assert_eq!(right - left, 500);
    assert!(
        placed(&p)
            .iter()
            .all(|(_, (x, y), _)| x % 50 == 0 && y % 50 == 0)
    );
}

#[test]
fn pin_1_goes_to_the_origin_wherever_it_is_listed() {
    let mut rows = pins(4);
    rows.rotate_left(1); // 2, 3, 4, 1
    let p = plan(Layout::SingleRow, &rows, None).unwrap();
    let one = p.pins.iter().find(|p| p.number == "1").unwrap();
    assert_eq!(one.at, mil(0, 0));
    assert_eq!(p.pins[0].at, mil(0, 300));
}

#[test]
fn the_pin_length_moves_the_connection_ends_out() {
    let p = plan(Layout::Dil, &pins(2), Some(Nm(200 * MIL))).unwrap();
    assert_eq!(placed(&p), [row("1", (0, 0), 0), row("2", (600, 0), 180)]);
    assert!(p.pins.iter().all(|p| p.length == Nm(200 * MIL)));
}

#[test]
fn bad_rows_are_refused_with_a_pointer() {
    assert_eq!(plan(Layout::Dil, &[], None).unwrap_err().pointer, "/pins");
    let mut rows = pins(3);
    rows[2].number = "1".into();
    assert_eq!(
        plan(Layout::Dil, &rows, None).unwrap_err().pointer,
        "/pins/2/number"
    );
    rows[2].number = " ".into();
    assert_eq!(
        plan(Layout::Dil, &rows, None).unwrap_err().pointer,
        "/pins/2/number"
    );
    assert_eq!(
        plan(Layout::Dil, &pins(2), Some(Nm(-1)))
            .unwrap_err()
            .pointer,
        "/pin_length"
    );
}

#[test]
fn rows_keep_their_name_and_type() {
    let mut rows = pins(2);
    rows[0].name = "CLK".into();
    rows[0].electrical_type = PinType::Input;
    let p = plan(Layout::Dil, &rows, None).unwrap();
    assert_eq!(p.pins[0].name, "CLK");
    assert_eq!(p.pins[0].electrical_type, PinType::Input);
    assert_eq!(p.pins[1].electrical_type, PinType::Passive);
}

fn symbol_with_unit() -> reda_model::Symbol {
    reda_model::Symbol {
        uuid: Uuid::from_u128(100),
        name: "Chip".into(),
        description: String::new(),
        designator_prefix: "U".into(),
        units: vec![
            reda_model::Unit {
                uuid: Uuid::from_u128(101),
                name: "A".into(),
            },
            reda_model::Unit {
                uuid: Uuid::from_u128(102),
                name: "B".into(),
            },
        ],
        pins: vec![],
        graphics: vec![],
        provenance: None,
    }
}

fn counter(from: u128) -> impl FnMut() -> Uuid {
    let mut next = from;
    move || {
        next += 1;
        Uuid::from_u128(next)
    }
}

const A: Uuid = Uuid::from_u128(101);
const B: Uuid = Uuid::from_u128(102);

#[test]
fn applying_puts_the_pins_and_one_body_rectangle_on_the_unit() {
    let mut s = symbol_with_unit();
    let p = plan(Layout::Dil, &pins(4), None).unwrap();
    apply(&mut s, A, &p, &mut counter(1000)).unwrap();
    assert_eq!(s.pins.len(), 4);
    assert!(s.pins.iter().all(|pin| pin.unit == A));
    assert_eq!(s.graphics.len(), 1);
    assert_eq!(
        s.graphics[0].shape,
        Shape::Rect {
            start: p.body.0,
            end: p.body.1
        }
    );
}

#[test]
fn reapplying_keeps_pin_uuids_by_number_and_drops_numbers_that_are_gone() {
    let mut s = symbol_with_unit();
    apply(
        &mut s,
        A,
        &plan(Layout::Dil, &pins(4), None).unwrap(),
        &mut counter(1000),
    )
    .unwrap();
    let uuid_of =
        |s: &reda_model::Symbol, n: &str| s.pins.iter().find(|p| p.number == n).map(|p| p.uuid);
    let two = uuid_of(&s, "2").unwrap();
    s.pins
        .iter_mut()
        .find(|p| p.number == "2")
        .unwrap()
        .description = "kept".into();

    let mut rows = pins(3);
    rows[2].number = "9".into();
    apply(
        &mut s,
        A,
        &plan(Layout::Quad, &rows, None).unwrap(),
        &mut counter(2000),
    )
    .unwrap();
    assert_eq!(uuid_of(&s, "2"), Some(two));
    assert_eq!(
        s.pins.iter().find(|p| p.number == "2").unwrap().description,
        "kept"
    );
    assert_eq!(uuid_of(&s, "4"), None);
    assert!(uuid_of(&s, "9").unwrap() > Uuid::from_u128(2000));
    assert_eq!(s.graphics.len(), 1, "the old body is replaced");
}

#[test]
fn a_number_another_unit_holds_is_refused() {
    let mut s = symbol_with_unit();
    apply(
        &mut s,
        A,
        &plan(Layout::Dil, &pins(2), None).unwrap(),
        &mut counter(1000),
    )
    .unwrap();
    let err = apply(
        &mut s,
        B,
        &plan(Layout::Dil, &pins(2), None).unwrap(),
        &mut counter(2000),
    )
    .unwrap_err();
    assert!(
        matches!(err, ApplyError::NumberTaken { ref number, ref unit } if number == "1" && unit == "A")
    );
    assert_eq!(
        apply(
            &mut s,
            Uuid::from_u128(5),
            &plan(Layout::Dil, &pins(2), None).unwrap(),
            &mut counter(0)
        ),
        Err(ApplyError::NoUnit(Uuid::from_u128(5)))
    );
}

#[test]
fn reopening_reads_each_pin_back_on_its_side_and_manual_rebuilds_the_same_pins() {
    let mut s = symbol_with_unit();
    let mut rows = pins(10);
    rows[0].name = "VCC".into();
    rows[0].electrical_type = PinType::PowerIn;
    for row in &mut rows[5..] {
        row.group = Some("io".into());
    }
    let original = plan(Layout::Quad, &rows, None).unwrap();
    apply(&mut s, A, &original, &mut counter(1000)).unwrap();

    let input = read_unit(&s, A);
    assert_eq!(input.layout, Layout::Manual);
    assert_eq!(input.pin_length, Nm(100 * MIL));
    let one = input.pins.iter().find(|p| p.number == "1").unwrap();
    assert_eq!(
        (one.name.as_str(), one.electrical_type, one.side),
        ("VCC", PinType::PowerIn, Some(Side::Left))
    );
    let sides: Vec<_> = input
        .pins
        .iter()
        .map(|p| (p.number.as_str(), p.side.unwrap()))
        .collect();
    assert_eq!(
        sides,
        [
            ("1", Side::Left),
            ("2", Side::Left),
            ("3", Side::Left),
            ("4", Side::Bottom),
            ("5", Side::Bottom),
            ("6", Side::Bottom),
            ("8", Side::Right),
            ("7", Side::Right),
            ("10", Side::Top),
            ("9", Side::Top),
        ]
    );
    // Pins 5 and 6 changed group along the bottom, so a gap separates them.
    let group = |n: &str| {
        input
            .pins
            .iter()
            .find(|p| p.number == n)
            .unwrap()
            .group
            .clone()
    };
    assert_ne!(group("5"), group("6"));
    assert_eq!(group("1"), group("2"));

    let rebuilt = plan(input.layout, &input.pins, Some(input.pin_length)).unwrap();
    let at = |p: &SymbolPlan, n: &str| {
        p.pins
            .iter()
            .find(|q| q.number == n)
            .map(|q| (q.at, q.rotation))
    };
    for n in 1..=10 {
        let n = n.to_string();
        assert_eq!(at(&rebuilt, &n), at(&original, &n), "Pin {n}");
    }
    assert_eq!(rebuilt.body, original.body);
}

#[test]
fn a_unit_without_gaps_reads_back_with_no_groups() {
    let mut s = symbol_with_unit();
    apply(
        &mut s,
        A,
        &plan(Layout::Dil, &pins(4), None).unwrap(),
        &mut counter(1000),
    )
    .unwrap();
    assert!(read_unit(&s, A).pins.iter().all(|p| p.group.is_none()));
    assert!(read_unit(&s, B).pins.is_empty());
}
