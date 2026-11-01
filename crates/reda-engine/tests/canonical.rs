use reda_engine::canonical::CanonicalWriter;
use reda_engine::model::Uuid;
use serde::Serialize;

#[derive(Serialize)]
struct Row {
    uuid: Uuid,
    name: &'static str,
    width: i64,
}

fn row(n: u128, name: &'static str) -> Row {
    Row {
        uuid: Uuid::v4_from_bits(n),
        name,
        width: 254_000,
    }
}

#[test]
fn writes_format_first_then_fields_and_tables_one_object_per_line() {
    let mut w = CanonicalWriter::new(1);
    w.field("sheet", &row(9, "Main")).unwrap();
    w.table("wires", [row(3, "c"), row(1, "a"), row(2, "b")], |r| r.uuid)
        .unwrap();
    w.table("labels", Vec::<Row>::new(), |r| r.uuid).unwrap();
    let text = String::from_utf8(w.finish()).unwrap();
    assert_eq!(
        text,
        r#"{
  "format": 1,
  "sheet": {"uuid":"00000000-0000-4000-8000-000000000009","name":"Main","width":254000},
  "wires": [
    {"uuid":"00000000-0000-4000-8000-000000000001","name":"a","width":254000},
    {"uuid":"00000000-0000-4000-8000-000000000002","name":"b","width":254000},
    {"uuid":"00000000-0000-4000-8000-000000000003","name":"c","width":254000}
  ],
  "labels": []
}
"#
    );
}

#[test]
fn lists_keep_their_given_order() {
    let mut w = CanonicalWriter::new(1);
    w.list("documents", [row(3, "c"), row(1, "a")]).unwrap();
    let text = String::from_utf8(w.finish()).unwrap();
    assert!(
        text.find("\"c\"").unwrap() < text.find("\"a\"").unwrap(),
        "{text}"
    );
}

#[test]
fn floats_are_refused() {
    #[derive(Serialize)]
    struct Bad {
        x: f64,
    }
    let mut w = CanonicalWriter::new(1);
    let err = w.field("bad", &Bad { x: 0.5 }).unwrap_err();
    assert!(err.to_string().contains("bad"), "{err}");
}

#[test]
fn duplicate_uuids_in_a_table_are_refused() {
    let mut w = CanonicalWriter::new(1);
    assert!(
        w.table("wires", [row(1, "a"), row(1, "b")], |r| r.uuid)
            .is_err()
    );
}

#[test]
fn non_ascii_text_stays_utf8() {
    let mut w = CanonicalWriter::new(1);
    w.field("title", &"Ω µ").unwrap();
    let text = String::from_utf8(w.finish()).unwrap();
    assert!(text.contains(r#""title": "Ω µ""#), "{text}");
}
