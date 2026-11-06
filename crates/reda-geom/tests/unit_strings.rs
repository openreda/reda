//! Unit strings, as Command params accept them: `"1.27mm"`, `"10mil"`, `"45deg"`.

use reda_geom::{Millideg, Nm};

#[test]
fn length_strings_convert_exactly_to_nanometres() {
    for (text, nm) in [
        ("1.27mm", 1_270_000),
        ("10mil", 254_000),
        ("0.5in", 12_700_000),
        ("2.54um", 2_540),
        ("7nm", 7),
        ("-0.2mm", -200_000),
        ("+3mm", 3_000_000),
        (".5mm", 500_000),
        ("1 mm", 1_000_000),
        (" 8mil ", 203_200),
        ("1.5mil", 38_100),
        ("1000", 1_000),
    ] {
        assert_eq!(Nm::parse(text), Ok(Nm(nm)), "{text}");
    }
}

#[test]
fn malformed_length_strings_are_refused_with_a_reason() {
    for text in [
        "", "mm", "1.2.3mm", "1ft", "1e3mm", "1,5mm", "--1mm", "1.mm.", "abc", "45deg",
    ] {
        let err = Nm::parse(text).unwrap_err();
        assert!(!err.to_string().is_empty(), "{text}");
    }
}

#[test]
fn a_length_finer_than_a_nanometre_is_refused() {
    let err = Nm::parse("0.0001mil").unwrap_err();
    assert!(err.to_string().contains("nanometre"), "{err}");
}

#[test]
fn a_length_beyond_the_integer_range_is_refused() {
    assert!(Nm::parse("99999999999999999999in").is_err());
    assert!(Nm::parse("9999999999999999999999999999999999999999mm").is_err());
}

#[test]
fn angle_strings_convert_exactly_to_millidegrees() {
    for (text, md) in [
        ("45deg", 45_000),
        ("0.5deg", 500),
        ("-90deg", -90_000),
        ("12.345deg", 12_345),
        ("1500", 1_500),
    ] {
        assert_eq!(Millideg::parse(text), Ok(Millideg(md)), "{text}");
    }
    assert!(Millideg::parse("45.0001deg").is_err());
    assert!(Millideg::parse("45rad").is_err());
    assert!(Millideg::parse("45mm").is_err());
}

#[test]
fn micrometres_may_be_written_with_the_micro_sign() {
    assert_eq!(Nm::parse("35µm"), Ok(Nm(35_000)));
    assert_eq!(Nm::parse("35μm"), Ok(Nm(35_000)));
}

#[test]
fn copper_thickness_is_entered_in_ounces_at_exactly_35_um_each() {
    for (text, nm) in [
        ("1oz", 35_000),
        ("0.5 oz", 17_500),
        ("2oz", 70_000),
        ("18um", 18_000),
        ("0.035mm", 35_000),
        ("1.4mil", 35_560),
        ("35000", 35_000),
    ] {
        assert_eq!(Nm::parse_copper(text), Ok(Nm(nm)), "{text}");
    }
    assert!(Nm::parse("1oz").is_err(), "oz is a copper unit only");
}
