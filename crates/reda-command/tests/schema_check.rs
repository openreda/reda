//! `check_schema` on the keywords the file schemas use beyond Command params.

use reda_command::check_schema;
use serde_json::json;

/// A Board Template gate: choice parameter names, each with a list of option keys.
fn gate() -> serde_json::Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "patternProperties": {
            "^[a-z][a-z0-9_]*$": {"type": "array", "items": {"type": "string"}}
        }
    })
}

#[test]
fn keys_matching_a_pattern_property_are_known_and_checked_against_it() {
    assert_eq!(
        check_schema(&gate(), &json!({"lanes": ["x1", "x4"]})),
        Ok(())
    );
    let err = check_schema(&gate(), &json!({"lanes": [4]})).unwrap_err();
    assert_eq!(err.pointer, "/lanes/0");
}

#[test]
fn keys_no_pattern_matches_are_still_unknown() {
    let err = check_schema(&gate(), &json!({"Lanes": ["x1"]})).unwrap_err();
    assert_eq!(err.pointer, "/Lanes");
    assert!(err.message.contains("unknown field"), "{}", err.message);
}
