//! The shared text resolver: `${name}` tokens, the `${` refusal and the `$${` escape.

use reda_model::text::{self, TextError};
use reda_model::{
    DocumentEntry, DocumentKind, Fields, InDocument, Object, Project, ProjectProps, SheetText,
    TxError, Uuid,
};

fn vars(name: &str) -> Option<String> {
    match name {
        "project" => Some("Blinky".into()),
        "job" => Some("JLCPCB".into()),
        _ => None,
    }
}

#[test]
fn tokens_are_replaced_by_their_values() {
    assert_eq!(
        text::resolve("${project}-${job}.zip", vars).unwrap(),
        "Blinky-JLCPCB.zip"
    );
    assert_eq!(text::resolve("no tokens", vars).unwrap(), "no tokens");
}

#[test]
fn an_unknown_or_unclosed_token_is_an_error() {
    assert_eq!(
        text::resolve("a ${variant}", vars),
        Err(TextError::Unknown {
            name: "variant".into(),
            at: 3
        })
    );
    assert_eq!(
        text::resolve("${project", vars),
        Err(TextError::Unterminated { at: 1 })
    );
    assert_eq!(
        text::resolve("${}", vars),
        Err(TextError::Unknown {
            name: String::new(),
            at: 1
        })
    );
}

#[test]
fn a_doubled_dollar_before_a_brace_is_a_literal_dollar_brace() {
    assert_eq!(text::resolve("$${project}", vars).unwrap(), "${project}");
    assert_eq!(text::display("cost $${x} and $5"), "cost ${x} and $5");
    // Three dollars: one plain, then the escape.
    assert_eq!(text::display("$$${x}"), "$${x}");
    assert_eq!(text::display("$$ and {}"), "$$ and {}");
}

#[test]
fn stored_text_with_an_unescaped_dollar_brace_is_refused() {
    assert_eq!(text::check("V${CC}"), Err(TextError::Reserved { at: 2 }));
    assert_eq!(text::check("${"), Err(TextError::Reserved { at: 1 }));
    assert_eq!(text::check("$${CC} costs $5 {x}"), Ok(()));
    assert_eq!(text::check("$$${CC}"), Ok(()));
    let message = TextError::Reserved { at: 2 }.to_string();
    assert!(message.contains("\"${\""), "{message}");
    assert!(message.contains("\"$${\""), "{message}");
}

#[test]
fn escaping_any_text_gives_stored_text_that_displays_as_the_original() {
    for original in ["${VAR}/x", "$${x}", "$$${", "plain", "$", "{${${}", "€${ü}"] {
        let stored = text::escape(original);
        assert_eq!(text::check(&stored), Ok(()), "{stored}");
        assert_eq!(text::display(&stored), original, "{stored}");
    }
    assert_eq!(text::escape("${A}"), "$${A}");
}

#[test]
fn character_positions_count_characters_not_bytes() {
    assert_eq!(text::check("Ω ${x}"), Err(TextError::Reserved { at: 3 }));
}

fn id(n: u128) -> Uuid {
    Uuid::v4_from_bits(n)
}

fn project_with_sheet() -> Project {
    let mut project = Project::new(ProjectProps {
        uuid: id(1),
        name: "Demo".into(),
        documents: vec![],
        libraries: vec![],
        properties: Default::default(),
        erc: Default::default(),
        schematic: Default::default(),
        reannotate: None,
        library_update: None,
    });
    let mut tx = project.transaction();
    let mut props = tx.project().props().clone();
    props.documents.push(id(2));
    tx.insert(Object::Document(DocumentEntry {
        uuid: id(2),
        kind: DocumentKind::Schematic,
        name: "sheet".into(),
        title: "Sheet 1".into(),
    }))
    .unwrap();
    tx.replace(Object::Project(props)).unwrap();
    tx.commit().unwrap();
    project
}

fn sheet_text(words: &str) -> Object {
    Object::SheetText(InDocument::new(
        id(2),
        SheetText {
            uuid: id(3),
            text: words.into(),
            at: Default::default(),
            height: reda_model::geom::Nm(1_000_000),
            rotation: Default::default(),
            justify: Default::default(),
        },
    ))
}

#[test]
fn a_transaction_refuses_an_object_whose_text_has_an_unescaped_dollar_brace() {
    let mut project = project_with_sheet();
    let mut tx = project.transaction();
    let err = tx.insert(sheet_text("V${CC}")).unwrap_err();
    let TxError::Invalid(message) = err else {
        panic!("{err:?}")
    };
    assert!(message.contains("sheet_text"), "{message}");
    assert!(message.contains("character 2"), "{message}");
    tx.insert(sheet_text("V$${CC}")).unwrap();
    let mut words = sheet_text("x");
    if let Object::SheetText(t) = &mut words {
        t.object.text = "${".into();
    }
    assert!(tx.replace(words).is_err());
    tx.commit().unwrap();

    let mut tx = project.transaction();
    let mut props = tx.project().props().clone();
    props.properties.title = "${title}".into();
    let message = tx.replace(Object::Project(props)).unwrap_err().to_string();
    assert!(message.contains("title"), "{message}");
}

#[test]
fn every_field_value_is_checked() {
    let mut fields = Fields::default();
    fields.set("LCSC", "C${1}");
    let mut texts = vec![];
    text::fields_texts(&fields, &mut texts);
    assert_eq!(texts, [("field LCSC".to_owned(), "C${1}")]);
}
