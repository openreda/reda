use reda_model::Uuid;

#[test]
fn formats_lowercase_hyphenated() {
    let id = Uuid::from_u128(0x0123_4567_89ab_4def_8123_4567_89ab_cdef);
    assert_eq!(id.to_string(), "01234567-89ab-4def-8123-456789abcdef");
}

#[test]
fn parses_hand_edited_uppercase_and_normalises_it() {
    let id: Uuid = "01234567-89AB-4DEF-8123-456789ABCDEF".parse().unwrap();
    assert_eq!(id.to_string(), "01234567-89ab-4def-8123-456789abcdef");
}

#[test]
fn refuses_malformed_strings() {
    for bad in [
        "",
        "01234567-89ab-4def-8123-456789abcde",
        "0123456789ab4def8123456789abcdef",
        "01234567-89ab-4def-8123-456789abcdeg",
        "01234567_89ab-4def-8123-456789abcdef",
    ] {
        assert!(bad.parse::<Uuid>().is_err(), "{bad:?} should be refused");
    }
}

#[test]
fn serialises_as_a_string() {
    let id = Uuid::from_u128(1);
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, r#""00000000-0000-0000-0000-000000000001""#);
    assert_eq!(serde_json::from_str::<Uuid>(&json).unwrap(), id);
    assert!(serde_json::from_str::<Uuid>("1").is_err());
}

#[test]
fn v4_sets_version_and_variant_bits() {
    let id = Uuid::v4_from_bits(0);
    assert_eq!(id.to_string(), "00000000-0000-4000-8000-000000000000");
    let id = Uuid::v4_from_bits(u128::MAX);
    assert_eq!(id.to_string(), "ffffffff-ffff-4fff-bfff-ffffffffffff");
}
