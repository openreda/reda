//! Internal builders shared by Commands and by Project loading, so that `project.new`,
//! `document.add` and opening a file all build the same data the same way.

use reda_model::{DocumentEntry, Object, Transaction, TxError};

/// Adds a Document at the end of the Project's Document order.
pub(crate) fn add_document(tx: &mut Transaction<'_>, entry: DocumentEntry) -> Result<(), TxError> {
    let mut props = tx.project().props().clone();
    props.documents.push(entry.uuid);
    tx.insert(Object::Document(entry))?;
    tx.replace(Object::Project(props))
}
