//! Internal builders shared by Commands and by Project loading, so that `project.new`,
//! `document.add` and opening a file all build the same data the same way.

use reda_model::{DocumentEntry, Object, Scope, Transaction, TxError, Uuid};

/// Adds a Document at the end of the Project's Document order.
pub(crate) fn add_document(tx: &mut Transaction<'_>, entry: DocumentEntry) -> Result<(), TxError> {
    let mut props = tx.project().props().clone();
    props.documents.push(entry.uuid);
    tx.insert(Object::Document(entry))?;
    tx.replace(Object::Project(props))
}

/// Removes a Document, everything in it first, and drops it from the Document order.
pub(crate) fn remove_document(tx: &mut Transaction<'_>, uuid: Uuid) -> Result<(), TxError> {
    let contents: Vec<Uuid> = tx
        .project()
        .objects()
        .iter()
        .filter(|o| o.scope() == Scope::Document(uuid))
        .map(Object::uuid)
        .collect();
    for object in contents {
        tx.remove(object)?;
    }
    let mut props = tx.project().props().clone();
    props.documents.retain(|d| *d != uuid);
    tx.replace(Object::Project(props))?;
    tx.remove(uuid)?;
    Ok(())
}
