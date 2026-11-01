//! Replaying a change set inside a transaction: how undo and redo change the Project.

use reda_model::{DocumentEntry, DocumentKind, Object, Project, ProjectProps, TxError, Uuid};

fn id(n: u128) -> Uuid {
    Uuid::v4_from_bits(n)
}

fn empty_project() -> Project {
    Project::new(ProjectProps {
        uuid: id(1),
        name: "Demo".into(),
        documents: vec![],
        libraries: vec![],
    })
}

fn add_sheet(project: &mut Project, n: u128) -> reda_model::ChangeSet {
    let mut tx = project.transaction();
    let mut props = tx.project().props().clone();
    props.documents.push(id(n));
    tx.insert(Object::Document(DocumentEntry {
        uuid: id(n),
        kind: DocumentKind::Schematic,
        name: format!("sheet{n}"),
        title: format!("Sheet {n}"),
    }))
    .unwrap();
    tx.replace(Object::Project(props)).unwrap();
    tx.commit().unwrap()
}

#[test]
fn applying_the_inverse_in_a_transaction_undoes_and_records_the_reverse() {
    let mut project = empty_project();
    let before = project.clone();
    let added = add_sheet(&mut project, 2);
    let after = project.clone();

    let mut tx = project.transaction();
    tx.apply(&added.inverted()).unwrap();
    let undone = tx.commit().unwrap();
    assert_eq!(project, before);
    assert_eq!(undone, added.inverted());

    let mut tx = project.transaction();
    tx.apply(&added).unwrap();
    tx.commit().unwrap();
    assert_eq!(project, after);
}

#[test]
fn applying_a_stale_change_set_is_a_conflict_and_changes_nothing() {
    let mut project = empty_project();
    let added = add_sheet(&mut project, 2);
    let current = project.clone();

    // Replaying the creation again: the sheet already exists, so its `before` is stale.
    let mut tx = project.transaction();
    assert_eq!(tx.apply(&added), Err(TxError::Stale(id(1))));
    drop(tx);
    assert_eq!(project, current);
}
