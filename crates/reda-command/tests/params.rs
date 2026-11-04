//! Params are checked against the Command's schema; lengths and angles take unit strings.

mod support;

use reda_command::ErrorCode;
use serde_json::json;
use support::{Harness, pointer};

#[test]
fn length_and_angle_params_take_integers_or_unit_strings_and_return_integers() {
    let mut h = Harness::with_project();
    let out = h
        .run(
            "test.measure",
            json!({
                "width": "1.27mm",
                "angle": "45deg",
                "points": [{"x": "10mil", "y": 0}, {"x": 5, "y": "0.5in"}]
            }),
        )
        .unwrap();
    assert_eq!(
        out,
        json!({"width": 1_270_000, "angle": 45_000, "sums": [254_000, 12_700_005]})
    );
    let out = h
        .run("test.measure", json!({"width": 7, "angle": 1500}))
        .unwrap();
    assert_eq!(out, json!({"width": 7, "angle": 1500, "sums": []}));
}

#[test]
fn length_and_angle_params_are_a_one_of_in_the_schema() {
    let h = Harness::new();
    let described = h.registry.describe("test.measure").unwrap();
    let length = &described["params"]["$defs"]["Length"];
    assert_eq!(length["oneOf"][0]["type"], "integer");
    assert_eq!(length["oneOf"][1]["type"], "string");
    assert_eq!(length["oneOf"][1]["format"], "length");
    assert!(
        length["description"]
            .as_str()
            .unwrap()
            .contains("\"1.27mm\""),
        "{length}"
    );
    let angle = &described["params"]["$defs"]["Angle"];
    assert_eq!(angle["oneOf"][0]["type"], "integer");
    assert_eq!(angle["oneOf"][1]["format"], "angle");
    assert_eq!(
        described["result"]["properties"]["width"]["type"],
        "integer"
    );
}

#[test]
fn bad_params_carry_a_json_pointer_to_the_bad_field() {
    let mut h = Harness::with_project();
    for (params, expected) in [
        (json!({"width": "1ft"}), "/width"),
        (json!({"width": 1.5}), "/width"),
        (json!({"width": 1, "angle": "45mm"}), "/angle"),
        (
            json!({"width": 1, "points": [{"x": 1, "y": 2}, {"x": "x", "y": 2}]}),
            "/points/1/x",
        ),
        (json!({"width": 1, "points": [{"x": 1}]}), "/points/0/y"),
        (json!({"width": 1, "extra": true}), "/extra"),
        (json!({"width": 1, "points": {}}), "/points"),
        (json!({}), "/width"),
        (json!([1]), ""),
    ] {
        let err = h.run("test.measure", params.clone()).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams, "{params}");
        assert_eq!(pointer(&err), expected, "{params}: {}", err.message);
    }
}

#[test]
fn a_bad_unit_string_says_what_is_wrong() {
    let mut h = Harness::with_project();
    let err = h.run("test.measure", json!({"width": "1ft"})).unwrap_err();
    assert!(err.message.contains("unknown unit"), "{}", err.message);
    let err = h
        .run("test.measure", json!({"width": "0.0001mil"}))
        .unwrap_err();
    assert!(err.message.contains("nanometre"), "{}", err.message);
}

#[test]
fn a_pointer_escapes_tilde_and_slash_in_field_names() {
    let mut h = Harness::with_project();
    let err = h
        .run("test.measure", json!({"width": 1, "a/b~c": 0}))
        .unwrap_err();
    assert_eq!(pointer(&err), "/a~1b~0c");
}

#[test]
fn a_bad_uuid_is_invalid_params_at_its_field() {
    let mut h = Harness::with_project();
    let err = h
        .run("test.sheet.add", json!({"uuid": "nope", "title": "A"}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(pointer(&err), "/uuid");
}

#[test]
fn null_params_mean_no_params() {
    let mut h = Harness::with_project();
    assert!(h.run("test.project.name", json!(null)).is_ok());
}

#[test]
fn one_param_can_be_checked_alone_for_a_form_to_flag_it() {
    let h = Harness::new();
    let r = &h.registry;
    assert!(
        r.check_param("test.measure", "width", &json!("1.27mm"))
            .is_ok()
    );
    let err = r
        .check_param("test.measure", "width", &json!("1ft"))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(pointer(&err), "/width");
    let err = r
        .check_param("test.measure", "points", &json!([{"x": 1, "y": true}]))
        .unwrap_err();
    assert_eq!(pointer(&err), "/points/0/y");
    let err = r
        .check_param("test.measure", "nope", &json!(1))
        .unwrap_err();
    assert_eq!(pointer(&err), "/nope");
    assert_eq!(
        r.check_param("no.such", "x", &json!(1)).unwrap_err().code,
        ErrorCode::NotFound
    );
}

#[test]
fn whole_params_can_be_checked_without_running() {
    let h = Harness::new();
    let r = &h.registry;
    assert!(r.check_params("test.measure", &json!({"width": 3})).is_ok());
    let err = r.check_params("test.measure", &json!({})).unwrap_err();
    assert_eq!(pointer(&err), "/width");
}
