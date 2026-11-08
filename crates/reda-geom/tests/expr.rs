//! Numeric expressions typed into Properties fields (`editing.md#numeric-expressions`):
//! plain values with units, `!` for the current value, `+ - * /` and brackets, and other
//! attributes of the same object by English schema name.

use reda_geom::expr::{Attr, Field, Kind, Object, evaluate, evaluate_each};
use reda_geom::{Millideg, Nm};

const MM: Field = Field {
    kind: Kind::Length,
    unit: "mm",
};
const MIL: Field = Field {
    kind: Kind::Length,
    unit: "mil",
};
const DEG: Field = Field {
    kind: Kind::Angle,
    unit: "deg",
};

/// A Track 0.25 mm wide on Net Class priority 3, rotated 90°.
const TRACK: &[(&str, Attr)] = &[
    ("width", Attr::Length(Nm(250_000))),
    ("clearance", Attr::Length(Nm(200_000))),
    ("rotation", Attr::Angle(Millideg(90_000))),
    ("priority", Attr::Number(3)),
];

fn with(current: i64) -> Object<'static> {
    Object {
        current: Some(current),
        attributes: TRACK,
    }
}

fn eval(text: &str, field: Field, current: i64) -> Result<i64, String> {
    evaluate(text, field, &with(current)).map_err(|e| e.to_string())
}

fn refused(text: &str, field: Field) -> String {
    eval(text, field, 1_000_000).expect_err(text)
}

#[test]
fn a_plain_value_takes_the_display_units_and_a_suffix_overrides_them() {
    assert_eq!(eval("1.5", MM, 0), Ok(1_500_000));
    assert_eq!(eval("10", MIL, 0), Ok(254_000));
    assert_eq!(eval("10mil", MM, 0), Ok(254_000));
    assert_eq!(eval("0.1 mm", MIL, 0), Ok(100_000));
    assert_eq!(eval("2in", MM, 0), Ok(50_800_000));
    assert_eq!(eval("250um", MM, 0), Ok(250_000));
    assert_eq!(eval("7nm", MM, 0), Ok(7));
    assert_eq!(eval(".5", MM, 0), Ok(500_000));
    assert_eq!(eval("45", DEG, 0), Ok(45_000));
    assert_eq!(eval("12.5deg", DEG, 0), Ok(12_500));
}

#[test]
fn a_leading_sign_is_absolute_not_relative() {
    // `+0.1mm` is +0.1 mm, as in the leading suite, whatever the current value.
    assert_eq!(eval("+0.1mm", MM, 5_000_000), Ok(100_000));
    assert_eq!(eval("-2", MM, 5_000_000), Ok(-2_000_000));
    assert_eq!(eval("--2", MM, 0), Ok(2_000_000));
}

#[test]
fn bang_is_the_current_value() {
    assert_eq!(eval("!", MM, 300_000), Ok(300_000));
    assert_eq!(eval("! + 0.1mm", MM, 300_000), Ok(400_000));
    assert_eq!(eval("! - 0.1", MM, 300_000), Ok(200_000));
    assert_eq!(eval("! + 5", MIL, 254_000), Ok(381_000));
    assert_eq!(eval("! * 2", MM, 300_000), Ok(600_000));
    assert_eq!(eval("2 * !", MM, 300_000), Ok(600_000));
    assert_eq!(eval("! / 4", MM, 300_000), Ok(75_000));
    assert_eq!(eval("! + 15deg", DEG, 90_000), Ok(105_000));
    assert_eq!(eval("-!", MM, 300_000), Ok(-300_000));
}

#[test]
fn operators_follow_precedence_and_brackets() {
    // (0.3 mm + 5 mil) / 2 = (300000 + 127000) / 2.
    assert_eq!(eval("(! + 5mil) / 2", MM, 300_000), Ok(213_500));
    assert_eq!(eval("1 + 2 * 3", MM, 0), Ok(7_000_000));
    assert_eq!(eval("(1 + 2) * 3", MM, 0), Ok(9_000_000));
    assert_eq!(eval("10 - 4 - 3", MM, 0), Ok(3_000_000));
    assert_eq!(eval("12 / 3 / 2", MM, 0), Ok(2_000_000));
    assert_eq!(eval("((1mm))", MIL, 0), Ok(1_000_000));
}

#[test]
fn arithmetic_is_exact_until_the_result_is_rounded_to_the_nearest_unit() {
    // 0.1 + 0.2 is exactly 0.3 here, unlike floating point.
    assert_eq!(eval("0.1 + 0.2", MM, 0), Ok(300_000));
    assert_eq!(eval("1 / 3 * 3", MM, 0), Ok(1_000_000));
    // 1 mm / 3 = 333 333.33… nm; 2 mm / 3 = 666 666.67 nm; halves round away from zero.
    assert_eq!(eval("! / 3", MM, 1_000_000), Ok(333_333));
    assert_eq!(eval("! / 3", MM, 2_000_000), Ok(666_667));
    assert_eq!(eval("1nm / 2", MM, 0), Ok(1));
    assert_eq!(eval("-1nm / 2", MM, 0), Ok(-1));
}

#[test]
fn other_attributes_of_the_object_by_english_schema_name() {
    assert_eq!(eval("Width + 5mil", MM, 0), Ok(377_000));
    assert_eq!(eval("width", MIL, 0), Ok(250_000));
    assert_eq!(eval("WIDTH * 2 - clearance", MM, 0), Ok(300_000));
    assert_eq!(eval("rotation / 2", DEG, 0), Ok(45_000));
    assert_eq!(eval("! * priority", MM, 100_000), Ok(300_000));
    assert_eq!(eval("! * (Width / clearance)", MM, 100_000), Ok(125_000));
    let unknown = refused("Height + 1", MM);
    assert!(unknown.contains("Height"), "{unknown}");
    assert!(
        unknown.contains("width"),
        "names what the object has: {unknown}"
    );
}

#[test]
fn function_names_are_reserved_and_refused_for_now() {
    for text in [
        "sqrt(!)",
        "sqrt",
        "abs(-1)",
        "min(1, 2)",
        "Max(!, 1mm)",
        "round(!)",
    ] {
        let why = refused(text, MM);
        assert!(why.contains("reserved"), "{text}: {why}");
    }
    // Even an object attribute can't take a reserved name.
    let object = Object {
        current: Some(0),
        attributes: &[("sqrt", Attr::Length(Nm(1)))],
    };
    let why = evaluate("sqrt", MM, &object).unwrap_err().to_string();
    assert!(why.contains("reserved"), "{why}");
    // Any call is refused, reserved name or not.
    assert!(refused("width(2)", MM).contains("function"));
    assert!(reda_geom::expr::RESERVED_FUNCTIONS.contains(&"sqrt"));
}

#[test]
fn mismatched_quantities_and_bad_input_are_refused_with_a_reason() {
    for (text, field, needle) in [
        ("", MM, "empty"),
        ("   ", MM, "empty"),
        ("1mm + 5deg", MM, "angle"),
        ("rotation", MM, "angle"),
        ("1mm", DEG, "length"),
        ("! * !", MM, "area"),
        ("2 / !", MM, "divide"),
        ("! / 0", MM, "zero"),
        ("! / (1mm - 1mm)", MM, "zero"),
        ("priority", MM, "plain number"),
        ("Width / clearance", MM, "plain number"),
        ("priority + 1mm", MM, "plain number"),
        ("5xyz", MM, "unit"),
        ("1 +", MM, "end"),
        ("(1 + 2", MM, ")"),
        ("1 + 2)", MM, ")"),
        ("1 2", MM, "2"),
        ("1 % 2", MM, "%"),
        ("{old=new}", MM, "{"),
        ("1.2.3", MM, "number"),
        ("99999999999999999999999999999999999999999in", MM, "large"),
        ("9999999999in * 9999999999", MM, "large"),
    ] {
        let why = refused(text, field);
        assert!(why.contains(needle), "{text:?}: {why}");
    }
}

#[test]
fn bang_without_a_value_of_its_own_is_refused() {
    let object = Object {
        current: None,
        attributes: TRACK,
    };
    assert_eq!(evaluate("1mm", MM, &object).map_err(|_| ()), Ok(1_000_000));
    let why = evaluate("! + 1mm", MM, &object).unwrap_err().to_string();
    assert!(why.contains("!"), "{why}");
}

#[test]
fn each_object_gets_its_own_result_and_one_failure_fails_them_all() {
    let a = Object {
        current: Some(100_000),
        attributes: &[("width", Attr::Length(Nm(100_000)))],
    };
    let b = Object {
        current: Some(300_000),
        attributes: &[("width", Attr::Length(Nm(250_000)))],
    };
    assert_eq!(
        evaluate_each("! + width", MM, &[a, b]),
        Ok(vec![200_000, 550_000])
    );
    assert_eq!(evaluate_each("1mm", MM, &[a, b]), Ok(vec![1_000_000; 2]));
    let none = Object {
        current: None,
        attributes: &[],
    };
    let failure = evaluate_each("! * 2", MM, &[a, none, b]).unwrap_err();
    assert_eq!(failure.object, 1);
    assert!(failure.error.to_string().contains("!"));
    // A syntax error is reported once, against the first object.
    assert_eq!(evaluate_each("1 +", MM, &[a, b]).unwrap_err().object, 0);
}
