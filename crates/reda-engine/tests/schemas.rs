//! The JSON Schema of each file kind, generated from the Rust file shapes and checked in
//! under `docs/schemas/`. A stale copy fails here; `REDA_BLESS=1` regenerates them.

use reda_engine::format::{FORMAT, FileKind, schema};
use reda_testkit::assert_golden;
use serde_json::{Value, json};

const SCHEMAS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/schemas");

#[test]
fn the_checked_in_schemas_are_fresh() {
    for kind in FileKind::ALL {
        let mut text = serde_json::to_string_pretty(&schema(kind)).unwrap();
        text.push('\n');
        assert_golden(
            format!("{SCHEMAS}/{}.schema.json", kind.extension()),
            text.as_bytes(),
        );
    }
}

#[test]
fn schemas_refuse_unknown_fields_at_every_level() {
    fn check(value: &Value, at: &str) {
        if let Some(map) = value.as_object() {
            if map.get("type") == Some(&json!("object")) {
                assert_eq!(map.get("additionalProperties"), Some(&json!(false)), "{at}");
            }
            for (key, child) in map {
                check(child, &format!("{at}/{key}"));
            }
        }
    }
    for kind in FileKind::ALL {
        check(&schema(kind), kind.extension());
    }
}

#[test]
fn schemas_pin_the_current_format_and_describe_uuids() {
    let project = schema(FileKind::Project);
    assert_eq!(project["properties"]["format"]["const"], FORMAT);
    assert_eq!(
        project["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    let text = project.to_string();
    assert!(text.contains(r#""format":"uuid""#), "{text}");
}
