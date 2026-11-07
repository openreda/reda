use reda_model::{
    DocumentEntry, DocumentKind, Object, Project, ProjectProps, Scope, TxError, Uuid,
};

fn id(n: u128) -> Uuid {
    Uuid::v4_from_bits(n)
}

fn empty_project() -> Project {
    Project::new(ProjectProps {
        uuid: id(1),
        name: "Demo".into(),
        documents: vec![],
        libraries: vec![],
        properties: Default::default(),
        erc: Default::default(),
        schematic: Default::default(),
        reannotate: None,
    })
}

fn sheet(n: u128, title: &str) -> DocumentEntry {
    DocumentEntry {
        uuid: id(n),
        kind: DocumentKind::Schematic,
        name: format!("sheet{n}"),
        title: title.into(),
    }
}

/// Adds a sheet the way a Command would: insert its entry and append it to the order.
fn add_sheet(project: &mut Project, entry: DocumentEntry) -> reda_model::ChangeSet {
    let mut tx = project.transaction();
    let mut props = tx.project().props().clone();
    props.documents.push(entry.uuid);
    tx.insert(Object::Document(entry)).unwrap();
    tx.replace(Object::Project(props)).unwrap();
    tx.commit().unwrap()
}

#[test]
fn adding_a_document_creates_its_entry_and_an_empty_sheet() {
    let mut project = empty_project();
    add_sheet(&mut project, sheet(2, "Sheet 1"));

    let docs: Vec<_> = project.documents().map(|d| d.title.as_str()).collect();
    assert_eq!(docs, ["Sheet 1"]);
    assert_eq!(project.schematic_sheet(id(2)).unwrap().uuid, id(2));
}

#[test]
fn change_set_records_before_and_after_per_uuid() {
    let mut project = empty_project();
    let changes = add_sheet(&mut project, sheet(2, "Sheet 1"));

    let created = changes.get(id(2)).unwrap();
    assert_eq!(created.scope, Scope::Project);
    assert_eq!(created.before, None);
    assert_eq!(created.after, Some(Object::Document(sheet(2, "Sheet 1"))));

    let modified = changes.get(id(1)).unwrap();
    assert_eq!(
        modified.before.as_ref().map(Object::uuid),
        Some(id(1)),
        "the Project's own props are an object keyed by its UUID"
    );
    assert_eq!(changes.len(), 2);
}

#[test]
fn summary_groups_created_modified_deleted_by_scope() {
    let mut project = empty_project();
    let changes = add_sheet(&mut project, sheet(2, "Sheet 1"));
    let summary = changes.summary();
    let project_scope = &summary[&Scope::Project];
    assert_eq!(project_scope.created, [id(2)]);
    assert_eq!(project_scope.modified, [id(1)]);
    assert!(project_scope.deleted.is_empty());
}

#[test]
fn repeated_edits_to_one_object_merge_into_one_change() {
    let mut project = empty_project();
    let mut tx = project.transaction();
    for name in ["A", "B"] {
        let mut props = tx.project().props().clone();
        props.name = name.into();
        tx.replace(Object::Project(props)).unwrap();
    }
    let changes = tx.commit().unwrap();
    let change = changes.get(id(1)).unwrap();
    let name = |o: &Option<Object>| match o {
        Some(Object::Project(p)) => p.name.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(name(&change.before), "Demo");
    assert_eq!(name(&change.after), "B");
}

#[test]
fn creating_then_deleting_in_one_transaction_leaves_no_change() {
    let mut project = empty_project();
    let mut tx = project.transaction();
    tx.insert(Object::Document(sheet(2, "Tmp"))).unwrap();
    tx.remove(id(2)).unwrap();
    assert!(tx.commit().unwrap().is_empty());
}

#[test]
fn dropping_an_uncommitted_transaction_rolls_back() {
    let mut project = empty_project();
    let before = project.clone();
    {
        let mut tx = project.transaction();
        tx.insert(Object::Document(sheet(2, "Sheet 1"))).unwrap();
        let mut props = tx.project().props().clone();
        props.name = "Changed".into();
        tx.replace(Object::Project(props)).unwrap();
    }
    assert_eq!(project, before);
}

#[test]
fn commit_refuses_a_document_missing_from_the_order_and_rolls_back() {
    let mut project = empty_project();
    let before = project.clone();
    let mut tx = project.transaction();
    tx.insert(Object::Document(sheet(2, "Orphan"))).unwrap();
    assert!(matches!(tx.commit(), Err(TxError::Invalid(_))));
    assert_eq!(project, before);
}

#[test]
fn insert_refuses_a_taken_uuid_and_replace_or_remove_an_unknown_one() {
    let mut project = empty_project();
    add_sheet(&mut project, sheet(2, "Sheet 1"));
    let mut tx = project.transaction();
    assert_eq!(
        tx.insert(Object::Document(sheet(2, "Again"))),
        Err(TxError::Conflict(id(2)))
    );
    assert_eq!(
        tx.replace(Object::Document(sheet(9, "Nope"))),
        Err(TxError::NotFound(id(9)))
    );
    assert_eq!(tx.remove(id(9)), Err(TxError::NotFound(id(9))));
}

#[test]
fn applying_the_inverted_change_set_undoes_and_reapplying_redoes() {
    let mut project = empty_project();
    let original = project.clone();
    let changes = add_sheet(&mut project, sheet(2, "Sheet 1"));
    let edited = project.clone();

    project.apply(&changes.inverted());
    assert_eq!(project, original);
    project.apply(&changes);
    assert_eq!(project, edited);
}
