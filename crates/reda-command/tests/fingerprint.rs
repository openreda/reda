//! The design fingerprint checks use to tell stale results: it hashes canonical JSON, so
//! it is the same in every build of REDA, not only within one.

use reda_command::Fingerprint;
use serde_json::json;

#[test]
fn the_fingerprint_is_fnv1a_over_each_parts_compact_json() {
    // 64-bit FNV-1a over `{"a":1}` 0x1f `[1,"x"]` 0x1f, worked out by hand.
    let mut f = Fingerprint::new();
    f.add(&json!({"a": 1})).add(&json!([1, "x"]));
    assert_eq!(f.finish(), "ced8272abf338fb6");
}

#[test]
fn parts_are_kept_apart() {
    let one = Fingerprint::new().add(&json!("ab")).finish();
    let two = Fingerprint::new()
        .add(&json!("a"))
        .add(&json!("b"))
        .finish();
    assert_ne!(one, two);
}
